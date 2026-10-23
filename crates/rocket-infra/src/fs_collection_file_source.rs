//! Reads the raw files of a collection for export.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use rocket_collection::Collection;
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

/// The collection name must follow the domain rule and be one plain path component. A
/// component check (not a character list) also refuses a Windows drive prefix such as
/// `C:x`, which makes `Path::join` drop the workspace path.
fn validate_name(name: &str) -> DomainResult<()> {
    let invalid = || DomainError::InvalidInput("Invalid collection name".to_string());
    if name.contains('\0') || name == "." || name == ".." {
        return Err(invalid());
    }
    Collection::validate_name(name).map_err(|_| invalid())?;
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(invalid()),
    }
}

/// Only these files go into an export, so nothing else is read into memory.
fn is_exportable(name: &str) -> bool {
    matches!(
        name.rsplit_once('.').map(|(_, ext)| ext),
        Some("yml" | "yaml" | "proto" | "js")
    )
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
        } else if file_type.is_file() && is_exportable(&rel) {
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
    fn file_source_reads_only_exportable_files() {
        let ws = tempfile::tempdir().expect("tempdir");
        let root = ws.path().join("collections/my-api");
        std::fs::create_dir_all(root.join("protos")).expect("mkdir");
        std::fs::create_dir_all(root.join("shared")).expect("mkdir");
        std::fs::create_dir_all(root.join("images")).expect("mkdir");
        for (path, bytes) in [
            ("opencollection.yml", "a: 1"),
            ("a.yaml", "b: 2"),
            ("protos/a.proto", "syntax = \"proto3\";"),
            ("shared/util.js", "x"),
            ("big.bin", "binary fixture"),
            ("images/logo.png", "png"),
            ("notes.txt", "text"),
        ] {
            std::fs::write(root.join(path), bytes).expect("write");
        }
        let source = FsCollectionFileSource::new(Arc::new(Mutex::new(ws.path().to_path_buf())));
        let files = source.list_files("my-api").expect("lists");
        let paths: Vec<_> = files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "a.yaml",
                "opencollection.yml",
                "protos/a.proto",
                "shared/util.js"
            ]
        );
    }

    #[test]
    fn file_source_rejects_unsafe_collection_name() {
        let ws = tempfile::tempdir().expect("tempdir");
        let source = FsCollectionFileSource::new(Arc::new(Mutex::new(ws.path().to_path_buf())));
        let mut names = vec!["../x", "a/b", "a\\b", "", ".hidden", "."];
        if cfg!(windows) {
            // A drive prefix makes `Path::join` drop the workspace path on Windows.
            names.extend(["C:x", "C:\\x"]);
        }
        for name in names {
            assert!(
                matches!(source.list_files(name), Err(DomainError::InvalidInput(_))),
                "name {name:?} must be refused"
            );
        }
    }
}
