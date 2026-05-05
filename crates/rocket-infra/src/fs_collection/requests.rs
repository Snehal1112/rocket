use std::fs;
use std::path::Path;

use rocket_collection::{request_filename_for, Collection, Request};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::conversions::{oc_http_request_to_request, request_to_oc_http_request};
use crate::oc::OcHttpRequest;

use super::paths::resolve_request_path;
use super::FsCollectionRepo;

pub(super) fn get_request(repo: &FsCollectionRepo, collection: &str, path: &str) -> DomainResult<Request> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);

    // Try .yml first, then .json for backward compatibility.
    let yml_path = if path.ends_with(".yml") || path.ends_with(".yaml") {
        path.to_string()
    } else {
        format!("{}.yml", path.strip_suffix(".json").unwrap_or(path))
    };

    // Try .yml first.
    if let Ok(file_path) = repo.validate_path(&collection_dir, Path::new(&yml_path)) {
        if file_path.exists() && file_path.extension().map_or(false, |e| e == "yml" || e == "yaml") {
            let content = fs::read_to_string(&file_path)?;
            let oc: OcHttpRequest = serde_yaml::from_str(&content)
                .map_err(|e| DomainError::Internal(format!("Failed to parse YAML request: {e}")))?;
            let mut req = oc_http_request_to_request(oc);
            req.file_name = file_path.file_name().map(|n| n.to_string_lossy().to_string());
            return Ok(req);
        }
    }

    // Fall back to .json for legacy files.
    let json_path = if path.ends_with(".json") { path.to_string() } else { format!("{}.json", path) };
    let file_path = repo.validate_path(&collection_dir, Path::new(&json_path))
        .or_else(|_| repo.validate_path(&collection_dir, Path::new(path)))?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    let content = fs::read_to_string(&file_path)?;
    Ok(serde_json::from_str(&content)?)
}

#[tracing::instrument(name = "collection_save_request", skip(repo, request), fields(collection_name = %collection, request_path = %path))]
pub(super) fn save_request(repo: &FsCollectionRepo, collection: &str, path: &str, request: &Request) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if request.uid.is_empty() {
        return Err(DomainError::Internal(format!(
            "save_request: empty uid on request for '{path}' in collection '{collection}'; callers must construct via Request::new()"
        )));
    }

    let collection_dir = repo.collection_path(collection);
    let normalized = request_filename_for(path);
    let file_path = repo.validate_path(&collection_dir, Path::new(&normalized))?;

    let oc = request_to_oc_http_request(request);
    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize request YAML: {e}")))?;

    atomic_write(&file_path, yaml.as_bytes())?;

    // Return the actual filename relative to the collection directory.
    let actual = file_path
        .strip_prefix(&collection_dir)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();
    Ok(actual)
}

pub(super) fn rename_request(repo: &FsCollectionRepo, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let old_file = resolve_request_path(repo, &collection_dir, old_path)?;
    let new_ext = if new_path.ends_with(".yml") || new_path.ends_with(".yaml") || new_path.ends_with(".json") {
        new_path.to_string()
    } else {
        format!("{}.yml", new_path)
    };
    let new_file = repo.validate_path(&collection_dir, Path::new(&new_ext))?;
    fs::rename(&old_file, &new_file)?;
    Ok(())
}

pub(super) fn delete_request(repo: &FsCollectionRepo, collection: &str, path: &str) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, path)?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, path)));
    }
    fs::remove_file(&file_path)?;
    Ok(())
}
