use crate::error::{ImportError, ImportResult};
use crate::postman::ast::PostmanCollection;
use std::path::Path;

/// Read a Postman Collection JSON file and return the parsed AST.
/// Returns `ImportError::NotAPostmanCollection` if `info.schema` does not
/// contain `schema.getpostman.com`.
pub(crate) fn parse_postman_json(path: &Path) -> ImportResult<PostmanCollection> {
    let content = std::fs::read_to_string(path).map_err(ImportError::IoError)?;

    let col: PostmanCollection =
        serde_json::from_str(&content).map_err(|e| ImportError::JsonParseError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

    if !col.info.schema.contains("schema.getpostman.com") {
        return Err(ImportError::NotAPostmanCollection(path.to_path_buf()));
    }

    Ok(col)
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
    fn parses_minimal_collection_from_disk() {
        let col = parse_postman_json(&fixture("minimal-collection.json")).unwrap();
        assert_eq!(col.info.name, "Minimal API");
        assert_eq!(col.item.len(), 2);
    }

    #[test]
    fn parses_full_collection_from_disk() {
        let col = parse_postman_json(&fixture("full-collection.json")).unwrap();
        assert_eq!(col.info.name, "Full API");
        assert!(!col.item.is_empty());
        assert!(!col.variable.is_empty());
        assert!(col.auth.is_some());
        assert_eq!(col.environment.len(), 2);
        assert_eq!(col.environment[0].name, "Local");
        assert_eq!(col.environment[1].name, "Staging");
    }

    #[test]
    fn parses_v2_0_collection_from_disk() {
        let col = parse_postman_json(&fixture("v2.0-collection.json")).unwrap();
        assert_eq!(col.info.name, "Legacy API");
    }

    #[test]
    fn rejects_non_postman_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("other.json");
        std::fs::write(
            &path,
            r#"{"info": {"name": "X", "schema": "https://example.com"}}"#,
        )
        .unwrap();
        assert!(matches!(
            parse_postman_json(&path),
            Err(ImportError::NotAPostmanCollection(_))
        ));
    }

    #[test]
    fn rejects_invalid_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(
            parse_postman_json(&path),
            Err(ImportError::JsonParseError { .. })
        ));
    }
}
