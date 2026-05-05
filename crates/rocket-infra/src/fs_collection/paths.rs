use std::fs;
use std::path::{Path, PathBuf};

use rocket_collection::generate_uid;
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::opencollection::{OcCollection, OcFolderInfo};

use super::FsCollectionRepo;

/// Read UID from YAML metadata (opencollection.yml or folder.yml).
/// Falls back to legacy .uid file, migrating the value into YAML.
pub(super) fn read_uid_from_yaml(dir: &Path) -> String {
    // Try opencollection.yml first (collection root).
    let oc_path = dir.join("opencollection.yml");
    if oc_path.exists() {
        if let Ok(content) = fs::read_to_string(&oc_path) {
            if let Ok(mut oc) = serde_yaml::from_str::<OcCollection>(&content) {
                if let Some(ref uid) = oc.uid {
                    if !uid.is_empty() {
                        return uid.clone();
                    }
                }
                // No UID in YAML — check legacy .uid file.
                let uid = read_legacy_uid(dir);
                oc.uid = Some(uid.clone());
                if let Ok(yaml) = serde_yaml::to_string(&oc) {
                    if atomic_write(&oc_path, yaml.as_bytes()).is_ok() {
                        cleanup_legacy_uid(dir);
                    }
                }
                return uid;
            }
        }
    }

    // Try folder.yml (subfolder).
    let folder_path = dir.join("folder.yml");
    if folder_path.exists() {
        if let Ok(content) = fs::read_to_string(&folder_path) {
            if let Ok(mut info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                if let Some(ref uid) = info.uid {
                    if !uid.is_empty() {
                        return uid.clone();
                    }
                }
                let uid = read_legacy_uid(dir);
                info.uid = Some(uid.clone());
                if let Ok(yaml) = serde_yaml::to_string(&info) {
                    if atomic_write(&folder_path, yaml.as_bytes()).is_ok() {
                        cleanup_legacy_uid(dir);
                    }
                }
                return uid;
            }
        }
    }

    // No YAML metadata at all — use legacy .uid.
    read_legacy_uid(dir)
}

/// Read UID from legacy .uid file, or generate a new one.
pub(super) fn read_legacy_uid(dir: &Path) -> String {
    let uid_path = dir.join(".uid");
    if let Ok(uid) = fs::read_to_string(&uid_path) {
        let trimmed = uid.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    generate_uid()
}

/// Delete the legacy .uid file if it exists.
pub(super) fn cleanup_legacy_uid(dir: &Path) {
    let uid_path = dir.join(".uid");
    if uid_path.exists() {
        let _ = fs::remove_file(&uid_path);
    }
}

/// Return an error if `path` is a symlink. Protects destructive ops from traversal via symlink.
pub(super) fn reject_symlink(path: &Path) -> DomainResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(DomainError::InvalidInput(
            format!("Refusing operation on symlink: {}", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(e) => Err(DomainError::Io(e.to_string())),
    }
}

pub(super) fn count_request_files(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_request_files(&path);
            } else if is_request_file(&path) {
                count += 1;
            }
        }
    }
    count
}

pub(super) fn is_request_file(path: &Path) -> bool {
    // Exclude reserved sidecar and config files.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if matches!(name, "collection.json" | "_order.json" | "_order.yml" | "opencollection.yml" | "folder.yml") {
            return false;
        }
    }
    path.extension().is_some_and(|ext| ext == "json" || ext == "yml" || ext == "yaml" || ext == "bru")
}

/// Resolve a request file path, trying .yml first, then .json for backward compat.
pub(super) fn resolve_request_path(repo: &FsCollectionRepo, collection_dir: &Path, path: &str) -> DomainResult<PathBuf> {
    // Try .yml first.
    let yml = if path.ends_with(".yml") || path.ends_with(".yaml") {
        path.to_string()
    } else {
        format!("{}.yml", path.strip_suffix(".json").unwrap_or(path))
    };
    if let Ok(p) = repo.validate_path(collection_dir, Path::new(&yml)) {
        if p.exists() {
            return Ok(p);
        }
    }
    // Fall back to .json.
    let json = if path.ends_with(".json") { path.to_string() } else { format!("{}.json", path) };
    repo.validate_path(collection_dir, Path::new(&json))
        .or_else(|_| repo.validate_path(collection_dir, Path::new(path)))
}
