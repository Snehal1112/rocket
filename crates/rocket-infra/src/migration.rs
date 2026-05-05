//! Migration logic for converting legacy JSON collections to OpenCollection YAML.

use std::fs;
use std::path::{Path, PathBuf};

use rocket_collection::generate_uid;
use rocket_shared::error::{DomainError, DomainResult};

use crate::{atomic_write, atomic_write_bulk};
use crate::conversions::request_to_oc_http_request;
use crate::oc::{OcCollection, OcFolderInfo, OcInfo};

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

/// Return `true` if a previous migration of this collection was interrupted.
/// Callers can surface a warning instead of showing a broken collection.
pub fn is_migration_interrupted(collection_dir: &Path) -> bool {
    collection_dir.join(".migration_in_progress").exists()
}

/// Copy all `.json` and `.uid` files from `collection_dir` (recursively) into
/// `collection_dir/.legacy_backup/`, preserving the relative directory structure.
pub(crate) fn snapshot_legacy_files(collection_dir: &Path) -> DomainResult<()> {
    let backup_dir = collection_dir.join(".legacy_backup");
    fs::create_dir_all(&backup_dir)?;
    copy_legacy_tree(collection_dir, collection_dir, &backup_dir)
}

fn copy_legacy_tree(base: &Path, src: &Path, backup_dir: &Path) -> DomainResult<()> {
    let entries = fs::read_dir(src)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip the backup dir itself and hidden entries — except .uid files, which must be backed up.
        if name == ".legacy_backup" || (name.starts_with('.') && name != ".uid") {
            continue;
        }
        if path.is_dir() {
            // Skip symlinked directories.
            if std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                continue;
            }
            let rel = path.strip_prefix(base).map_err(|e| {
                DomainError::Internal(format!("Failed to strip prefix during backup: {e}"))
            })?;
            fs::create_dir_all(backup_dir.join(rel))?;
            copy_legacy_tree(base, &path, backup_dir)?;
        } else if path.extension().is_some_and(|e| e == "json")
            || path.file_name().is_some_and(|n| n == ".uid")
        {
            let rel = path.strip_prefix(base).map_err(|e| {
                DomainError::Internal(format!("Failed to strip prefix during backup: {e}"))
            })?;
            let dst = backup_dir.join(rel);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &dst)?;
        }
    }
    Ok(())
}

/// Migrate a legacy JSON collection to OpenCollection YAML format.
/// Idempotent — does nothing if already in OC format.
pub fn migrate_collection(collection_dir: &Path) -> DomainResult<()> {
    if detect_format(collection_dir) != CollectionFormat::LegacyJson {
        return Ok(());
    }

    let sentinel = collection_dir.join(".migration_in_progress");

    // Refuse to restart a migration that was previously interrupted — backup is still available.
    if sentinel.exists() {
        return Err(DomainError::Internal(format!(
            "Previous migration of '{}' was interrupted. \
             Restore from .legacy_backup/ or remove .migration_in_progress to retry.",
            collection_dir.display()
        )));
    }

    // Snapshot originals before touching anything.
    snapshot_legacy_files(collection_dir)?;

    // Write sentinel: migration is now in progress.
    atomic_write(&sentinel, b"")?;

    let name = collection_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".into());

    // Perform the migration. If anything fails, sentinel + backup stay for recovery.
    let result = (|| -> DomainResult<()> {
        migrate_directory(collection_dir)?;

        let uid = read_legacy_uid_value(collection_dir);
        let oc = OcCollection {
            opencollection: Some("0.1".into()),
            uid: Some(uid),
            info: Some(OcInfo { name, summary: None, version: None, authors: None }),
            config: None,
            items: None,
            request: None,
            docs: None,
            bundled: None,
            extensions: None,
        };
        let yaml = serde_yaml::to_string(&oc)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize opencollection.yml: {e}")))?;
        atomic_write(&collection_dir.join("opencollection.yml"), yaml.as_bytes())?;

        let uid_path = collection_dir.join(".uid");
        if uid_path.exists() {
            let _ = fs::remove_file(&uid_path);
        }

        Ok(())
    })();

    match result {
        Ok(()) => {
            // Remove sentinel and backup on success.
            let _ = fs::remove_file(&sentinel);
            let backup_dir = collection_dir.join(".legacy_backup");
            if backup_dir.exists() {
                let _ = fs::remove_dir_all(&backup_dir);
            }
            Ok(())
        }
        Err(e) => {
            // Sentinel stays. Backup stays. Caller can check is_migration_interrupted().
            Err(e)
        }
    }
}

/// Migrate a single directory: convert .json requests to .yml, create folder.yml for subdirs.
fn migrate_directory(dir: &Path) -> DomainResult<()> {
    let entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .collect();

    // Collect request file writes to batch-fsync at the end of this directory.
    let mut request_writes: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    // Source paths are removed only after atomic_write_bulk succeeds.
    let mut source_paths: Vec<PathBuf> = Vec::new();

    for entry in &entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if name.starts_with('.') {
                continue;
            }
            // Skip symlinked directories — following them during migration can exfiltrate or corrupt files outside the workspace.
            if std::fs::symlink_metadata(&path).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                tracing::warn!(path = %path.display(), "skipping symlinked directory during migration");
                continue;
            }
            // Write folder.yml for subdirectories.
            let folder_yml = path.join("folder.yml");
            if !folder_yml.exists() {
                let uid = read_legacy_uid_value(&path);
                let info = OcFolderInfo {
                    name: name.clone(),
                    uid: Some(uid),
                    description: None,
                    folder_type: Some("folder".into()),
                    seq: None,
                    tags: Vec::new(),
                    request: None,
                };
                let yaml = serde_yaml::to_string(&info)
                    .map_err(|e| DomainError::Internal(format!("Failed to serialize folder.yml: {e}")))?;
                atomic_write(&folder_yml, yaml.as_bytes())?;
            }
            // Clean up legacy .uid in subfolder.
            let uid_path = path.join(".uid");
            if uid_path.exists() {
                let _ = fs::remove_file(&uid_path);
            }
            // Recurse into subfolder.
            migrate_directory(&path)?;
        } else if is_legacy_request_file(&path) {
            // Collect the converted bytes rather than writing one-by-one.
            if let Some((yml_path, yaml_bytes)) = prepare_request_migration(&path)? {
                request_writes.push((yml_path, yaml_bytes));
                source_paths.push(path.clone());
            }
        } else if name == "_order.json" {
            migrate_order_file(&path)?;
        }
    }

    // Batch-write all request files in this directory with a single parent-dir fsync.
    if !request_writes.is_empty() {
        let refs: Vec<(&std::path::Path, &[u8])> = request_writes
            .iter()
            .map(|(p, b)| (p.as_path(), b.as_slice()))
            .collect();
        atomic_write_bulk(&refs)?;
        // Delete originals only after the new .yml files are safely written.
        for src in &source_paths {
            fs::remove_file(src)?;
        }
    }

    Ok(())
}

/// Convert a .json request file to YAML bytes without writing to disk.
/// Returns `(yml_path, yaml_bytes)` on success, or `None` if the JSON can't be parsed.
fn prepare_request_migration(json_path: &Path) -> DomainResult<Option<(std::path::PathBuf, Vec<u8>)>> {
    let content = fs::read_to_string(json_path)?;
    let request: rocket_collection::Request = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(path = %json_path.display(), error = %e, "skipping unparseable JSON request during migration");
            return Ok(None);
        }
    };
    let oc = request_to_oc_http_request(&request);
    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize YAML request: {e}")))?;
    let yml_path = json_path.with_extension("yml");
    Ok(Some((yml_path, yaml.into_bytes())))
}

/// Convert _order.json to _order.yml.
fn migrate_order_file(json_path: &Path) -> DomainResult<()> {
    let content = fs::read_to_string(json_path)?;
    let order: Vec<String> = serde_json::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse _order.json: {e}")))?;

    // Update .json extensions in the order list to .yml.
    let updated: Vec<String> = order.into_iter()
        .map(|name| {
            if name.ends_with(".json") {
                format!("{}.yml", name.strip_suffix(".json").unwrap())
            } else {
                name
            }
        })
        .collect();

    let yaml = serde_yaml::to_string(&updated)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize _order.yml: {e}")))?;
    let yml_path = json_path.with_extension("yml");
    atomic_write(&yml_path, yaml.as_bytes())?;
    fs::remove_file(json_path)?;
    Ok(())
}

/// Read the legacy .uid file value, or generate a new UUID.
fn read_legacy_uid_value(dir: &Path) -> String {
    let uid_path = dir.join(".uid");
    if let Ok(uid) = fs::read_to_string(&uid_path) {
        let trimmed = uid.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    generate_uid()
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

    #[test]
    fn migrate_simple_collection() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("my-api");
        fs::create_dir(&col).unwrap();

        // Write a legacy JSON request.
        let json = r#"{"uid":"123","name":"Get Users","method":"GET","url":"https://api.example.com/users","headers":[],"body":null,"auth":{"authType":"none"}}"#;
        fs::write(col.join("get-users.json"), json).unwrap();

        // Write a legacy .uid file.
        fs::write(col.join(".uid"), "legacy-collection-uid").unwrap();

        // Migrate.
        migrate_collection(&col).unwrap();

        // Verify: opencollection.yml exists.
        assert!(col.join("opencollection.yml").exists());

        // Verify: .yml file created, .json deleted.
        assert!(col.join("get-users.yml").exists());
        assert!(!col.join("get-users.json").exists());

        // Verify: .uid file deleted.
        assert!(!col.join(".uid").exists());

        // Verify: opencollection.yml contains the legacy UID.
        let content = fs::read_to_string(col.join("opencollection.yml")).unwrap();
        assert!(content.contains("legacy-collection-uid"));
    }

    #[test]
    fn migrate_collection_with_nested_folders() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("api");
        let auth = col.join("auth");
        fs::create_dir_all(&auth).unwrap();

        let json = r#"{"uid":"456","name":"Login","method":"POST","url":"/login","headers":[],"body":null,"auth":{"authType":"none"}}"#;
        fs::write(auth.join("login.json"), json).unwrap();
        fs::write(auth.join(".uid"), "folder-uid").unwrap();

        migrate_collection(&col).unwrap();

        // Root: opencollection.yml exists.
        assert!(col.join("opencollection.yml").exists());

        // Subfolder: folder.yml exists, request migrated.
        assert!(auth.join("folder.yml").exists());
        assert!(auth.join("login.yml").exists());
        assert!(!auth.join("login.json").exists());
        assert!(!auth.join(".uid").exists());

        // folder.yml contains the legacy UID.
        let content = fs::read_to_string(auth.join("folder.yml")).unwrap();
        assert!(content.contains("folder-uid"));
    }

    #[test]
    fn migrate_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("api");
        fs::create_dir(&col).unwrap();

        let json = r#"{"uid":"789","name":"Test","method":"GET","url":"/test","headers":[],"body":null,"auth":{"authType":"none"}}"#;
        fs::write(col.join("test.json"), json).unwrap();

        migrate_collection(&col).unwrap();
        assert!(col.join("opencollection.yml").exists());
        assert!(col.join("test.yml").exists());

        // Run again — should be a no-op.
        migrate_collection(&col).unwrap();
        assert!(col.join("test.yml").exists());
        assert!(!col.join("test.json").exists());
    }

    #[test]
    fn migrate_order_json_to_yml() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("api");
        fs::create_dir(&col).unwrap();

        fs::write(col.join("a.json"), r#"{"uid":"1","name":"A","method":"GET","url":"/a","headers":[],"body":null,"auth":{"authType":"none"}}"#).unwrap();
        fs::write(col.join("_order.json"), r#"["a.json"]"#).unwrap();

        migrate_collection(&col).unwrap();

        // _order.yml should exist with updated names.
        assert!(col.join("_order.yml").exists());
        assert!(!col.join("_order.json").exists());
        let content = fs::read_to_string(col.join("_order.yml")).unwrap();
        assert!(content.contains("a.yml"));
    }

    #[test]
    fn sentinel_file_removed_on_successful_migration() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("my-api");
        fs::create_dir(&col).unwrap();
        let json = r#"{"uid":"1","name":"A","method":"GET","url":"/a","headers":[],"body":null,"auth":{"authType":"none"}}"#;
        fs::write(col.join("a.json"), json).unwrap();

        migrate_collection(&col).unwrap();

        assert!(!col.join(".migration_in_progress").exists(), "sentinel must be removed on success");
        assert!(col.join("opencollection.yml").exists());
    }

    #[test]
    fn is_migration_interrupted_false_when_no_sentinel() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("clean-api");
        fs::create_dir(&col).unwrap();
        assert!(!is_migration_interrupted(&col));
    }

    #[test]
    fn is_migration_interrupted_true_when_sentinel_exists() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("broken-api");
        fs::create_dir(&col).unwrap();
        fs::write(col.join(".migration_in_progress"), b"").unwrap();
        assert!(is_migration_interrupted(&col));
    }

    #[test]
    fn legacy_backup_removed_after_successful_migration() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("my-api");
        fs::create_dir(&col).unwrap();
        let json = r#"{"uid":"2","name":"B","method":"POST","url":"/b","headers":[],"body":null,"auth":{"authType":"none"}}"#;
        fs::write(col.join("b.json"), json).unwrap();

        migrate_collection(&col).unwrap();

        assert!(!col.join(".legacy_backup").exists(), ".legacy_backup must be cleaned up on success");
    }

    #[test]
    fn snapshot_preserves_json_files() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("snap-api");
        fs::create_dir(&col).unwrap();
        let json = r#"{"uid":"3","name":"C","method":"GET","url":"/c","headers":[],"body":null,"auth":{"authType":"none"}}"#;
        fs::write(col.join("c.json"), json).unwrap();

        snapshot_legacy_files(&col).unwrap();

        let backup_file = col.join(".legacy_backup").join("c.json");
        assert!(backup_file.exists(), "backup file must exist");
        let content = fs::read_to_string(&backup_file).unwrap();
        assert!(content.contains("\"uid\""));
    }

    #[test]
    fn interrupted_migration_returns_error_on_retry() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("broken-api");
        fs::create_dir(&col).unwrap();
        // Simulate interrupted migration: JSON file present + sentinel exists.
        let json = r#"{"uid":"4","name":"D","method":"GET","url":"/d","headers":[],"body":null,"auth":{"authType":"none"}}"#;
        fs::write(col.join("d.json"), json).unwrap();
        fs::write(col.join(".migration_in_progress"), b"").unwrap();

        let result = migrate_collection(&col);
        assert!(result.is_err(), "retry of interrupted migration must return Err");
        // Sentinel must still be there (not removed by the error path).
        assert!(col.join(".migration_in_progress").exists());
    }

    #[test]
    fn snapshot_backs_up_uid_files() {
        let dir = TempDir::new().unwrap();
        let col = dir.path().join("uid-api");
        fs::create_dir(&col).unwrap();
        fs::write(col.join(".uid"), b"my-legacy-uid").unwrap();
        fs::write(col.join("a.json"), b"{}").unwrap();

        snapshot_legacy_files(&col).unwrap();

        let backed_up_uid = col.join(".legacy_backup").join(".uid");
        assert!(backed_up_uid.exists(), ".uid file must be backed up");
        assert_eq!(fs::read_to_string(&backed_up_uid).unwrap(), "my-legacy-uid");
    }
}
