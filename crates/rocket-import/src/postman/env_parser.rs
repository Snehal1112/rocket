use crate::error::{ImportError, ImportResult};
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

pub(crate) fn parse_postman_environment(path: &Path) -> ImportResult<PostmanEnvironment> {
    let content = std::fs::read_to_string(path).map_err(ImportError::IoError)?;

    serde_json::from_str(&content).map_err(|e| ImportError::JsonParseError {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/postman")
            .join(name)
    }

    #[test]
    fn parses_environment_json() {
        let env = parse_postman_environment(&fixture("environment.json")).unwrap();
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
            parse_postman_environment(&path),
            Err(ImportError::JsonParseError { .. })
        ));
    }
}
