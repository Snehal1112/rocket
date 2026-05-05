use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use rocket_shared::error::{DomainError, DomainResult};

/// Read all `.yml` files in `dir` and parse each with serde_yaml.
/// Files that fail to parse are silently skipped.
/// Returns an empty Vec if `dir` does not exist.
pub(crate) fn read_dir_yaml<T: DeserializeOwned>(dir: &Path) -> DomainResult<Vec<(PathBuf, T)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "yml") {
            let content = fs::read_to_string(&path)?;
            if let Ok(item) = serde_yaml::from_str::<T>(&content) {
                out.push((path, item));
            }
        }
    }
    Ok(out)
}

/// Collect all `.yml` file paths in `dir` paired with their modification time,
/// sorted most-recently-modified first. Returns an empty Vec if `dir` does not exist.
/// Used by repos that sort by mtime before parsing (e.g., history, search).
pub(crate) fn read_dir_by_mtime(dir: &Path) -> DomainResult<Vec<(std::time::SystemTime, PathBuf)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "yml") {
            let mtime = entry
                .metadata()?
                .modified()
                .unwrap_or(std::time::UNIX_EPOCH);
            paths.push((mtime, path));
        }
    }
    paths.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(paths)
}

/// Remove a file at `path`, returning `DomainError::NotFound(label)` if it does not exist.
pub(crate) fn delete_if_exists(path: &Path, label: &str) -> DomainResult<()> {
    if !path.exists() {
        return Err(DomainError::NotFound(label.to_string()));
    }
    fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Item {
        name: String,
        value: u32,
    }

    #[test]
    fn read_dir_yaml_returns_empty_when_dir_missing() {
        let dir = TempDir::new().unwrap();
        let absent = dir.path().join("nonexistent");
        let result: Vec<(PathBuf, Item)> = read_dir_yaml(&absent).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn read_dir_yaml_parses_yml_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.yml"), b"name: alpha\nvalue: 1\n").unwrap();
        fs::write(dir.path().join("b.yml"), b"name: beta\nvalue: 2\n").unwrap();

        let mut items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
        items.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].1, Item { name: "alpha".into(), value: 1 });
        assert_eq!(items[1].1, Item { name: "beta".into(), value: 2 });
    }

    #[test]
    fn read_dir_yaml_skips_non_yml_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.yml"), b"name: alpha\nvalue: 1\n").unwrap();
        fs::write(dir.path().join("b.json"), b"{\"name\":\"beta\",\"value\":2}").unwrap();
        fs::write(dir.path().join("c.txt"), b"plain text").unwrap();

        let items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1.name, "alpha");
    }

    #[test]
    fn read_dir_yaml_skips_unparseable_yml_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("good.yml"), b"name: good\nvalue: 42\n").unwrap();
        fs::write(dir.path().join("bad.yml"), b"not: valid: yaml: {{").unwrap();

        let items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1.name, "good");
    }

    #[test]
    fn read_dir_yaml_returns_path_alongside_item() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("thing.yml"), b"name: thing\nvalue: 7\n").unwrap();

        let items: Vec<(PathBuf, Item)> = read_dir_yaml(dir.path()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, dir.path().join("thing.yml"));
    }

    #[test]
    fn delete_if_exists_removes_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("target.yml");
        fs::write(&path, b"data").unwrap();

        delete_if_exists(&path, "thing 'x'").unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn delete_if_exists_returns_not_found_when_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.yml");

        let err = delete_if_exists(&path, "item 'missing'").unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)), "expected NotFound, got {:?}", err);
    }

    #[test]
    fn read_dir_by_mtime_returns_empty_when_dir_missing() {
        let dir = TempDir::new().unwrap();
        let absent = dir.path().join("nonexistent");
        let result = read_dir_by_mtime(&absent).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn read_dir_by_mtime_returns_yml_paths_sorted_newest_first() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.yml"), b"x").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(dir.path().join("b.yml"), b"x").unwrap();

        let paths = read_dir_by_mtime(dir.path()).unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].1.file_name().unwrap(), "b.yml");
        assert_eq!(paths[1].1.file_name().unwrap(), "a.yml");
    }

    #[test]
    fn read_dir_by_mtime_skips_non_yml_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.yml"), b"x").unwrap();
        fs::write(dir.path().join("b.json"), b"{}").unwrap();

        let paths = read_dir_by_mtime(dir.path()).unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].1.file_name().unwrap(), "a.yml");
    }
}
