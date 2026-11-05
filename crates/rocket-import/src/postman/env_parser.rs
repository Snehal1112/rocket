use crate::error::{ImportError, ImportResult};
use crate::files::ImportFiles;
use crate::limits::{read_text_capped, MAX_IMPORT_FILE_BYTES};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanEnvironment {
    pub name: String,
    #[serde(default)]
    pub values: Vec<PostmanEnvVar>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostmanEnvVar {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

pub(crate) fn parse_postman_environment(
    files: &dyn ImportFiles,
    path: &Path,
) -> ImportResult<PostmanEnvironment> {
    let content = read_text_capped(files, path, MAX_IMPORT_FILE_BYTES)?;

    serde_json::from_str(&content).map_err(|e| ImportError::JsonParseError {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::test_files::StdFiles;
    use std::path::PathBuf;

    #[test]
    fn parse_postman_environment_refuses_an_oversized_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("big.json");
        let file = std::fs::File::create(&path).expect("create");
        file.set_len(crate::limits::MAX_IMPORT_FILE_BYTES + 1)
            .expect("set_len");

        let result = parse_postman_environment(&StdFiles, &path);

        assert!(matches!(
            result,
            Err(ImportError::IoError(ref e)) if e.kind() == std::io::ErrorKind::InvalidInput
        ));
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/postman")
            .join(name)
    }

    #[test]
    fn parses_environment_json() {
        let env = parse_postman_environment(&StdFiles, &fixture("environment.json")).unwrap();
        assert_eq!(env.name, "Local");
        assert_eq!(env.values.len(), 3);
        assert_eq!(env.values[0].key, "baseUrl");
        assert_eq!(env.values[0].value, "http://localhost:3000");
        assert!(env.values[0].enabled);
        assert!(!env.values[2].enabled);
    }

    #[test]
    fn rejects_invalid_env_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{bad json}").unwrap();
        assert!(matches!(
            parse_postman_environment(&StdFiles, &path),
            Err(ImportError::JsonParseError { .. })
        ));
    }
}
