//! Create, read, save, rename and delete `.js` script files in a collection.
//!
//! Every path is validated against the collection directory first. Only regular
//! `.js` files are touched, never symlinks, directories or other file types.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rocket_collection::{normalize_script_name, Collection, SCRIPT_TEMPLATE};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

use super::FsCollectionRepo;

/// Resolves an existing script file and checks it is a regular, non-symlink `.js` file.
fn resolve_existing(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<(PathBuf, PathBuf)> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let full = repo.validate_path(&collection_dir, Path::new(path))?;
    if full.extension().and_then(|e| e.to_str()) != Some("js") {
        return Err(DomainError::InvalidInput(
            "Only .js script files can be used here".into(),
        ));
    }
    // Check the unresolved path too, since `validate_path` follows symlinks.
    let unresolved = collection_dir.join(path);
    let meta = fs::symlink_metadata(&unresolved)
        .map_err(|_| DomainError::NotFound(format!("Script file '{path}' not found")))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(DomainError::InvalidInput(format!(
            "'{path}' is not a regular script file"
        )));
    }
    Ok((collection_dir, full))
}

fn relative_path(folder_path: &str, file_name: &str) -> String {
    let folder = folder_path.trim_matches('/');
    if folder.is_empty() {
        file_name.to_string()
    } else {
        format!("{folder}/{file_name}")
    }
}

pub(super) fn create_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    folder_path: &str,
    name: &str,
) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let file_name = normalize_script_name(name)?;
    let collection_dir = repo.collection_path(collection);
    let folder = repo.validate_path(&collection_dir, Path::new(folder_path))?;
    if !folder.is_dir() {
        return Err(DomainError::NotFound(format!(
            "Folder '{folder_path}' not found"
        )));
    }
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let target = folder.join(&file_name);
    let template = SCRIPT_TEMPLATE.replace("THIS_FILE.js", &file_name);
    // `create_new` fails when the file exists, so an existing script is never overwritten.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                DomainError::InvalidInput(format!("'{file_name}' already exists"))
            } else {
                DomainError::from(e)
            }
        })?;
    file.write_all(template.as_bytes())?;
    Ok(relative_path(folder_path, &file_name))
}

pub(super) fn read_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<String> {
    let (_, full) = resolve_existing(repo, collection, path)?;
    fs::read_to_string(&full).map_err(|e| {
        if e.kind() == std::io::ErrorKind::InvalidData {
            DomainError::InvalidInput(format!("'{path}' is not valid UTF-8 text"))
        } else {
            DomainError::from(e)
        }
    })
}

pub(super) fn save_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    content: &str,
) -> DomainResult<()> {
    let (_, full) = resolve_existing(repo, collection, path)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    atomic_write(&full, content.as_bytes())?;
    Ok(())
}

pub(super) fn rename_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    new_name: &str,
) -> DomainResult<String> {
    let (_, full) = resolve_existing(repo, collection, path)?;
    let new_file_name = normalize_script_name(new_name)?;
    let parent = full
        .parent()
        .ok_or_else(|| DomainError::InvalidInput("Script has no parent folder".into()))?;
    let target = parent.join(&new_file_name);
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if target.exists() {
        return Err(DomainError::InvalidInput(format!(
            "'{new_file_name}' already exists"
        )));
    }
    fs::rename(&full, &target)?;
    let folder_part = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(relative_path(&folder_part, &new_file_name))
}

pub(super) fn delete_script_file(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<()> {
    let (_, full) = resolve_existing(repo, collection, path)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    fs::remove_file(&full)?;
    Ok(())
}
