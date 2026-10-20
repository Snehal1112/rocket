//! Reads one `.env` file for an environment. Nothing is loaded into the process environment.

use std::path::{Component, Path, PathBuf};

use rocket_environment::DotEnvLoader;
use rocket_shared::paths::expand_tilde;

/// Filesystem implementation of `DotEnvLoader`.
pub struct FsDotEnvLoader;

/// Resolves a raw `dotEnvFilePath`. A `..` segment is refused.
pub(crate) fn resolve_dot_env_path(raw: &str, base: Option<&Path>) -> Result<PathBuf, String> {
    let expanded = expand_tilde(raw.trim());
    if expanded.components().any(|c| c == Component::ParentDir) {
        return Err(format!(
            "The .env path '{raw}' contains '..' and is refused."
        ));
    }
    if expanded.is_absolute() {
        return Ok(expanded);
    }
    match base {
        Some(base) => Ok(base.join(expanded)),
        None => Err(format!(
            "The .env path '{raw}' is relative and no base folder is known."
        )),
    }
}

impl DotEnvLoader for FsDotEnvLoader {
    fn load(&self, raw_path: &str, base: Option<&Path>) -> Result<Vec<(String, String)>, String> {
        let path = resolve_dot_env_path(raw_path, base)?;
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("Could not read the .env file {}: {e}", path.display()))?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| format!("The .env file {} is not valid UTF-8.", path.display()))?;
        let mut pairs = Vec::new();
        for item in dotenvy::Iter::new(text.as_bytes()) {
            match item {
                Ok(pair) => pairs.push(pair),
                Err(e) => {
                    return Err(format!(
                        "The .env file {} could not be parsed: {e}",
                        path.display()
                    ))
                }
            }
        }
        Ok(pairs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_resolves_against_the_base() {
        let got = resolve_dot_env_path(".env", Some(Path::new("/col"))).expect("ok");
        assert_eq!(got, PathBuf::from("/col/.env"));
    }

    #[test]
    fn an_absolute_path_is_used_as_written() {
        let got = resolve_dot_env_path("/etc/x.env", Some(Path::new("/col"))).expect("ok");
        assert_eq!(got, PathBuf::from("/etc/x.env"));
    }

    #[test]
    fn a_tilde_path_is_expanded() {
        let got = resolve_dot_env_path("~/x.env", None);
        if let Ok(p) = got {
            assert!(p.is_absolute());
            assert!(p.ends_with("x.env"));
        }
    }

    #[test]
    fn a_path_with_a_parent_segment_is_refused() {
        assert!(resolve_dot_env_path("../x.env", Some(Path::new("/col"))).is_err());
        assert!(resolve_dot_env_path("/a/../x.env", None).is_err());
    }

    #[test]
    fn a_relative_path_without_a_base_is_an_error() {
        assert!(resolve_dot_env_path(".env", None).is_err());
    }

    fn load_text(content: &str) -> Result<Vec<(String, String)>, String> {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".env"), content).expect("write");
        FsDotEnvLoader.load(".env", Some(dir.path()))
    }

    #[test]
    fn quotes_export_prefix_and_empty_values_parse_like_dotenvy() {
        let got = load_text("A=\"x # y\"\nexport B=two\nC=\nD='s q'\n# note\n").expect("ok");
        let get = |k: &str| got.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        assert_eq!(get("A").as_deref(), Some("x # y"));
        assert_eq!(get("B").as_deref(), Some("two"));
        assert_eq!(get("C").as_deref(), Some(""));
        assert_eq!(get("D").as_deref(), Some("s q"));
    }

    #[test]
    fn a_missing_file_is_an_error_text_naming_the_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = FsDotEnvLoader
            .load("nope.env", Some(dir.path()))
            .expect_err("missing");
        assert!(err.contains("nope.env"));
    }

    #[test]
    fn a_directory_is_an_error_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("d.env")).expect("mkdir");
        assert!(FsDotEnvLoader.load("d.env", Some(dir.path())).is_err());
    }

    #[test]
    fn a_file_that_is_not_utf8_is_an_error_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".env"), [0xff, 0xfe, b'=', 0xff]).expect("write");
        assert!(FsDotEnvLoader.load(".env", Some(dir.path())).is_err());
    }

    #[test]
    fn loading_does_not_change_the_process_environment() {
        let got = load_text("ROCKET_DOT_ENV_PROBE_KEY=1\n").expect("ok");
        assert_eq!(got.len(), 1);
        assert!(std::env::var("ROCKET_DOT_ENV_PROBE_KEY").is_err());
    }
}
