//! Reads the raw files of a collection for export.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rocket_export::{CollectionFileSource, SourceFile};
use rocket_shared::error::{DomainError, DomainResult};

pub struct FsCollectionFileSource {
    workspace_path: Arc<Mutex<PathBuf>>,
}

impl FsCollectionFileSource {
    pub fn new(workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self { workspace_path }
    }
}

fn validate_name(name: &str) -> DomainResult<()> {
    let bad = name.is_empty()
        || name.contains('\0')
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == "..";
    if bad {
        return Err(DomainError::InvalidInput(
            "Invalid collection name".to_string(),
        ));
    }
    Ok(())
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<SourceFile>) -> DomainResult<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "node_modules" {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if file_type.is_dir() {
            walk(&entry.path(), &rel, out)?;
        } else if file_type.is_file() {
            out.push(SourceFile {
                rel_path: rel,
                bytes: std::fs::read(entry.path())?,
            });
        }
    }
    Ok(())
}

impl CollectionFileSource for FsCollectionFileSource {
    fn list_files(&self, collection: &str) -> DomainResult<Vec<SourceFile>> {
        validate_name(collection)?;
        let root = self
            .workspace_path
            .lock()
            .map_err(|_| DomainError::Internal("workspace path lock poisoned".to_string()))?
            .join("collections")
            .join(collection);
        if !root.is_dir() {
            return Err(DomainError::NotFound(format!(
                "Collection '{collection}' not found"
            )));
        }
        let mut out = Vec::new();
        walk(&root, "", &mut out)?;
        out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_source_lists_collection_files_and_skips_git() {
        let ws = tempfile::tempdir().expect("tempdir");
        let root = ws.path().join("collections/my-api");
        std::fs::create_dir_all(root.join("users")).expect("mkdir");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        std::fs::write(root.join("opencollection.yml"), "a: 1").expect("write");
        std::fs::write(root.join("users/get.yml"), "b: 2").expect("write");
        std::fs::write(root.join(".git/config"), "x").expect("write");
        let source = FsCollectionFileSource::new(Arc::new(Mutex::new(ws.path().to_path_buf())));
        let files = source.list_files("my-api").expect("lists");
        let paths: Vec<_> = files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(paths, vec!["opencollection.yml", "users/get.yml"]);
    }

    #[cfg(unix)]
    #[test]
    fn file_source_skips_symlinks_to_files_and_directories() {
        let outside = tempfile::tempdir().expect("tempdir");
        std::fs::write(outside.path().join("secret.yml"), "token: LEAK").expect("write");
        std::fs::create_dir_all(outside.path().join("dir")).expect("mkdir");
        std::fs::write(outside.path().join("dir/inner.yml"), "x: 1").expect("write");

        let ws = tempfile::tempdir().expect("tempdir");
        let root = ws.path().join("collections/my-api");
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::write(root.join("opencollection.yml"), "a: 1").expect("write");
        std::os::unix::fs::symlink(outside.path().join("secret.yml"), root.join("link.yml"))
            .expect("file symlink");
        std::os::unix::fs::symlink(outside.path().join("dir"), root.join("linked-dir"))
            .expect("dir symlink");

        let source = FsCollectionFileSource::new(Arc::new(Mutex::new(ws.path().to_path_buf())));
        let files = source.list_files("my-api").expect("lists");
        let paths: Vec<_> = files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(paths, vec!["opencollection.yml"]);
    }

    #[test]
    fn file_source_rejects_unsafe_collection_name() {
        let ws = tempfile::tempdir().expect("tempdir");
        let source = FsCollectionFileSource::new(Arc::new(Mutex::new(ws.path().to_path_buf())));
        for name in ["../x", "a/b", "a\\b", ""] {
            assert!(
                matches!(source.list_files(name), Err(DomainError::InvalidInput(_))),
                "name {name:?} must be refused"
            );
        }
    }
}
