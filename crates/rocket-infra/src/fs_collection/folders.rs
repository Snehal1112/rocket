use std::fs;
use std::path::Path;

use rocket_collection::{Collection, CollectionSummary};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::migration::{detect_format, is_migration_interrupted, migrate_collection, CollectionFormat};
use crate::oc::{OcCollection, OcFolderInfo, OcInfo};
use rocket_collection::generate_uid;

use super::paths::{count_request_files, read_uid_from_yaml, reject_symlink};
use super::tree::build_folder_tree;
use super::FsCollectionRepo;

pub(super) fn list(repo: &FsCollectionRepo) -> DomainResult<Vec<CollectionSummary>> {
    let mut result = Vec::new();
    if !repo.base_dir.exists() {
        return Ok(result);
    }
    for entry in fs::read_dir(&repo.base_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            // Auto-migrate legacy JSON collections to OpenCollection YAML.
            match detect_format(&path) {
                CollectionFormat::OpenCollection => {} // Already migrated.
                CollectionFormat::LegacyJson => {
                    if let Err(e) = migrate_collection(&path) {
                        tracing::warn!(collection = %name, error = %e, "failed to migrate collection");
                        continue;
                    }
                }
                CollectionFormat::Empty => continue,
            }
            let count = count_request_files(&path);
            let uid = read_uid_from_yaml(&path);
            let modified_at = fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs().to_string());
            result.push(CollectionSummary::new(
                uid,
                &name,
                path.to_string_lossy().to_string(),
                count,
                modified_at,
            ));
        }
    }
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(result)
}

#[tracing::instrument(name = "collection_get", skip(repo), fields(collection_name = %name))]
pub(super) fn get(repo: &FsCollectionRepo, name: &str) -> DomainResult<Collection> {
    Collection::validate_name(name)?;
    let path = repo.collection_path(name);
    if !path.exists() {
        return Err(DomainError::NotFound(format!("Collection '{}'", name)));
    }
    // Surface interrupted migrations before attempting to load.
    if is_migration_interrupted(&path) {
        return Err(DomainError::Internal(format!(
            "Collection '{}' has an incomplete migration. \
             Restore from .legacy_backup/ or remove .migration_in_progress to retry.",
            name
        )));
    }
    // Auto-migrate legacy JSON if needed.
    if detect_format(&path) == CollectionFormat::LegacyJson {
        migrate_collection(&path)?;
    }
    let root = build_folder_tree(&path)?;
    let settings = super::settings::get_settings(repo, name).unwrap_or_default();
    Ok(Collection { name: name.to_string(), root, settings })
}

#[tracing::instrument(name = "collection_create", skip(repo), fields(collection_name = %name))]
pub(super) fn create(repo: &FsCollectionRepo, name: &str) -> DomainResult<Collection> {
    Collection::validate_name(name)?;
    let path = repo.collection_path(name);
    if path.exists() {
        return Err(DomainError::AlreadyExists(format!("Collection '{}'", name)));
    }
    fs::create_dir_all(&path)?;

    // Write opencollection.yml with basic info.
    let oc = OcCollection {
        opencollection: Some("1.0.0".into()),
        uid: Some(generate_uid()),
        info: Some(OcInfo {
            name: name.into(),
            summary: None,
            version: None,
            authors: None,
        }),
        config: None,
        items: None,
        request: None,
        docs: None,
        bundled: None,
        extensions: None,
    };
    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize opencollection.yml: {e}")))?;
    atomic_write(&path.join("opencollection.yml"), yaml.as_bytes())?;

    Ok(Collection::new(name))
}

#[tracing::instrument(name = "collection_delete", skip(repo), fields(collection_name = %name))]
pub(super) fn delete(repo: &FsCollectionRepo, name: &str) -> DomainResult<()> {
    Collection::validate_name(name)?;
    let path = repo.collection_path(name);
    if !path.exists() {
        return Err(DomainError::NotFound(format!("Collection '{}'", name)));
    }
    reject_symlink(&path)?;
    fs::remove_dir_all(&path)?;
    Ok(())
}

#[tracing::instrument(name = "collection_rename", skip(repo), fields(old_name = %old_name, new_name = %new_name))]
pub(super) fn rename(repo: &FsCollectionRepo, old_name: &str, new_name: &str) -> DomainResult<()> {
    Collection::validate_name(old_name)?;
    Collection::validate_name(new_name)?;
    let old_path = repo.collection_path(old_name);
    let new_path = repo.collection_path(new_name);
    if !old_path.exists() {
        return Err(DomainError::NotFound(format!("Collection '{}'", old_name)));
    }
    if new_path.exists() {
        return Err(DomainError::AlreadyExists(format!("Collection '{}'", new_name)));
    }
    fs::rename(&old_path, &new_path)?;
    Ok(())
}

pub(super) fn create_folder(repo: &FsCollectionRepo, collection: &str, path: &str) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let dir_path = repo.validate_path(&collection_dir, Path::new(path))?;
    fs::create_dir_all(&dir_path)?;

    // Write folder.yml with folder metadata.
    let folder_name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    let info = OcFolderInfo {
        name: folder_name,
        uid: Some(generate_uid()),
        description: None,
        folder_type: Some("folder".into()),
        seq: None,
        tags: Vec::new(),
        request: None,
    };
    let yaml = serde_yaml::to_string(&info)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize folder.yml: {e}")))?;
    atomic_write(&dir_path.join("folder.yml"), yaml.as_bytes())?;

    Ok(())
}

pub(super) fn delete_folder(repo: &FsCollectionRepo, collection: &str, path: &str) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let dir_path = repo.validate_path(&collection_dir, Path::new(path))?;
    if !dir_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    reject_symlink(&dir_path)?;
    fs::remove_dir_all(&dir_path)?;
    Ok(())
}

pub(super) fn move_item(
    repo: &FsCollectionRepo,
    src_collection: &str,
    src_path: &str,
    dst_collection: &str,
    dst_path: &str,
) -> DomainResult<()> {
    Collection::validate_name(src_collection)?;
    Collection::validate_name(dst_collection)?;
    // Acquire locks in sorted order to prevent deadlock.
    let (first, second) = if src_collection <= dst_collection {
        (src_collection, dst_collection)
    } else {
        (dst_collection, src_collection)
    };
    let mutex1 = repo.collection_mutex(first);
    let _guard1 = mutex1.lock().unwrap_or_else(|e| e.into_inner());
    let mutex2 = (src_collection != dst_collection).then(|| repo.collection_mutex(second));
    let _guard2 = mutex2.as_ref().map(|m| m.lock().unwrap_or_else(|e| e.into_inner()));
    let src_collection_dir = repo.collection_path(src_collection);
    let dst_collection_dir = repo.collection_path(dst_collection);
    let src = repo.validate_path(&src_collection_dir, Path::new(src_path))?;
    let dst = repo.validate_path(&dst_collection_dir, Path::new(dst_path))?;
    if !src.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", src_collection, src_path)));
    }
    if dst.starts_with(&src) {
        return Err(DomainError::InvalidInput("Cannot move into itself".into()));
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&src, &dst)?;

    // When a folder (directory) is moved or renamed, update the name field
    // inside folder.yml so it matches the new directory name. Without this,
    // build_folder_tree reads the stale name from folder.yml and the sidebar
    // keeps showing the old name after rename.
    if dst.is_dir() {
        let new_name = dst
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let folder_yml = dst.join("folder.yml");
        if folder_yml.exists() {
            let content = fs::read_to_string(&folder_yml)?;
            if let Ok(mut info) = serde_yaml::from_str::<OcFolderInfo>(&content) {
                info.name = new_name;
                let yaml = serde_yaml::to_string(&info)
                    .map_err(|e| DomainError::Internal(format!("Failed to serialize folder.yml: {e}")))?;
                atomic_write(&folder_yml, yaml.as_bytes())?;
            }
        }
    }

    Ok(())
}

pub(super) fn reorder_items(repo: &FsCollectionRepo, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let dir = if folder_path.is_empty() {
        collection_dir.clone()
    } else {
        repo.validate_path(&collection_dir, Path::new(folder_path))?
    };
    if !dir.is_dir() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, folder_path)));
    }
    let yaml = serde_yaml::to_string(ordered_names)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize order: {e}")))?;
    atomic_write(&dir.join("_order.yml"), yaml.as_bytes())?;
    Ok(())
}
