//! Reads one `.env` file for an environment. Nothing is loaded into the process environment.

use std::path::{Component, Path, PathBuf};

use rocket_environment::{DotEnvFileInfo, DotEnvLoader};
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
        read_pairs(&path)
    }

    fn load_default(&self, base: Option<&Path>) -> Result<Vec<(String, String)>, String> {
        let Some(base) = base else {
            return Ok(Vec::new());
        };
        let path = base.join(".env");
        match std::fs::metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            _ => read_pairs(&path),
        }
    }

    fn describe(&self, raw_path: Option<&str>, base: Option<&Path>) -> DotEnvFileInfo {
        let path = match raw_path {
            Some(raw) => match resolve_dot_env_path(raw, base) {
                Ok(p) => p,
                Err(message) => {
                    return DotEnvFileInfo {
                        error: Some(message),
                        ..DotEnvFileInfo::default()
                    }
                }
            },
            None => match base {
                Some(base) => base.join(".env"),
                None => return DotEnvFileInfo::default(),
            },
        };
        let shown = Some(path.display().to_string());
        if let Err(e) = std::fs::metadata(&path) {
            let error = (e.kind() != std::io::ErrorKind::NotFound)
                .then(|| format!("Could not read the .env file {}: {e}", path.display()));
            return DotEnvFileInfo {
                path: shown,
                error,
                ..DotEnvFileInfo::default()
            };
        }
        match read_pairs(&path) {
            Ok(pairs) => DotEnvFileInfo {
                path: shown,
                exists: true,
                key_count: pairs.len(),
                error: None,
            },
            Err(message) => DotEnvFileInfo {
                path: shown,
                exists: true,
                key_count: 0,
                error: Some(message),
            },
        }
    }
}

/// Reads and parses one file. The `Err` text is a warning message.
fn read_pairs(path: &Path) -> Result<Vec<(String, String)>, String> {
    let bytes = std::fs::read(path)
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

    #[test]
    fn load_default_reads_dot_env_in_the_base() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".env"), "A=1\n").expect("write");
        let got = FsDotEnvLoader.load_default(Some(dir.path())).expect("ok");
        assert_eq!(got, vec![("A".to_string(), "1".to_string())]);
    }

    #[test]
    fn load_default_of_a_missing_file_is_empty_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let got = FsDotEnvLoader.load_default(Some(dir.path())).expect("ok");
        assert!(got.is_empty());
    }

    #[test]
    fn load_default_with_no_base_is_empty() {
        let got = FsDotEnvLoader.load_default(None).expect("ok");
        assert!(got.is_empty());
    }

    #[test]
    fn load_default_of_a_directory_named_dot_env_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join(".env")).expect("mkdir");
        assert!(FsDotEnvLoader.load_default(Some(dir.path())).is_err());
    }

    #[test]
    fn load_default_of_a_malformed_file_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".env"), "A=\"unterminated\n").expect("write");
        assert!(FsDotEnvLoader.load_default(Some(dir.path())).is_err());
    }

    #[test]
    fn describe_reports_path_exists_and_key_count() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.env"), "A=1\nB=2\n").expect("write");
        let info = FsDotEnvLoader.describe(Some("a.env"), Some(dir.path()));
        assert!(info.exists);
        assert_eq!(info.key_count, 2);
        assert_eq!(info.error, None);
        assert_eq!(
            info.path,
            Some(dir.path().join("a.env").display().to_string())
        );
        let default = FsDotEnvLoader.describe(None, Some(dir.path()));
        assert!(!default.exists);
        std::fs::write(dir.path().join(".env"), "X=1\n").expect("write");
        let default = FsDotEnvLoader.describe(None, Some(dir.path()));
        assert!(default.exists);
        assert_eq!(default.key_count, 1);
    }

    #[test]
    fn describe_of_a_missing_file_is_not_an_error_and_exists_false() {
        let dir = tempfile::tempdir().expect("tempdir");
        let info = FsDotEnvLoader.describe(Some("nope.env"), Some(dir.path()));
        assert!(!info.exists);
        assert_eq!(info.key_count, 0);
        assert_eq!(info.error, None);
        assert!(info.path.is_some());
    }

    #[test]
    fn describe_of_a_dotdot_path_reports_the_refusal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let info = FsDotEnvLoader.describe(Some("../x.env"), Some(dir.path()));
        assert!(!info.exists);
        let error = info.error.expect("error");
        assert!(error.contains("contains '..' and is refused"));
    }

    #[test]
    fn describe_never_contains_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".env"), "TOKEN=supersecretvalue\n").expect("write");
        let info = FsDotEnvLoader.describe(None, Some(dir.path()));
        assert_eq!(info.key_count, 1);
        assert!(!format!("{info:?}").contains("supersecretvalue"));
    }
}
