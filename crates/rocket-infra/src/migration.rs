//! Migration logic for converting legacy JSON collections to OpenCollection YAML.

use std::fs;
use std::path::Path;

/// Detected format of a collection directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionFormat {
    /// OpenCollection YAML — has opencollection.yml.
    OpenCollection,
    /// Legacy JSON — has .json request files but no opencollection.yml.
    LegacyJson,
    /// Empty directory — no request files at all.
    Empty,
}

/// Detect the format of a collection directory.
pub fn detect_format(collection_dir: &Path) -> CollectionFormat {
    // If opencollection.yml exists, it's already OpenCollection format.
    if collection_dir.join("opencollection.yml").exists() {
        return CollectionFormat::OpenCollection;
    }

    // Check for any .json files (excluding reserved names).
    if has_json_request_files(collection_dir) {
        return CollectionFormat::LegacyJson;
    }

    CollectionFormat::Empty
}

/// Recursively check if a directory contains any .json request files.
fn has_json_request_files(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if has_json_request_files(&path) {
                return true;
            }
        } else if is_legacy_request_file(&path) {
            return true;
        }
    }
    false
}

/// Check if a file is a legacy JSON request (not a reserved sidecar file).
fn is_legacy_request_file(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if matches!(name, "collection.json" | "_order.json") {
            return false;
        }
    }
    path.extension().is_some_and(|ext| ext == "json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detect_empty_dir() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_format(dir.path()), CollectionFormat::Empty);
    }

    #[test]
    fn detect_opencollection_format() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("opencollection.yml"), "opencollection: \"0.1\"").unwrap();
        assert_eq!(detect_format(dir.path()), CollectionFormat::OpenCollection);
    }

    #[test]
    fn detect_legacy_json_format() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("get-users.json"), "{}").unwrap();
        assert_eq!(detect_format(dir.path()), CollectionFormat::LegacyJson);
    }

    #[test]
    fn detect_legacy_json_in_subfolder() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("auth");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("login.json"), "{}").unwrap();
        assert_eq!(detect_format(dir.path()), CollectionFormat::LegacyJson);
    }

    #[test]
    fn collection_json_alone_is_not_legacy() {
        let dir = TempDir::new().unwrap();
        // Only collection.json (settings file) — not a request, should be Empty.
        fs::write(dir.path().join("collection.json"), "{}").unwrap();
        assert_eq!(detect_format(dir.path()), CollectionFormat::Empty);
    }

    #[test]
    fn opencollection_takes_priority_over_json_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("opencollection.yml"), "opencollection: \"0.1\"").unwrap();
        fs::write(dir.path().join("leftover.json"), "{}").unwrap();
        assert_eq!(detect_format(dir.path()), CollectionFormat::OpenCollection);
    }
}
