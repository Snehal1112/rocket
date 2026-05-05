use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    Request,
};
use rocket_shared::error::{DomainError, DomainResult};

mod folders;
mod paths;
mod requests;
mod settings;
mod tree;
mod variables;

#[cfg(test)]
mod tests;

pub struct FsCollectionRepo {
    pub(super) base_dir: PathBuf,
    pub(super) locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl FsCollectionRepo {
    /// Create a repo that shares `locks` with other repo instances (e.g., inside `SharedPathCollectionRepo`).
    pub fn new(base_dir: PathBuf, locks: Arc<DashMap<String, Arc<Mutex<()>>>>) -> Self {
        Self { base_dir, locks }
    }

    /// Create a standalone repo with its own private lock map. Use this when no lock sharing is needed.
    pub fn new_standalone(base_dir: PathBuf) -> Self {
        Self::new(base_dir, Arc::new(DashMap::new()))
    }

    /// Return the per-collection mutex, creating it on first access.
    pub(super) fn collection_mutex(&self, name: &str) -> Arc<Mutex<()>> {
        Arc::clone(
            self.locks
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .value(),
        )
    }

    pub(super) fn collection_path(&self, name: &str) -> PathBuf {
        self.base_dir.join(name)
    }

    pub(super) fn settings_path(&self, name: &str) -> PathBuf {
        self.collection_path(name).join("opencollection.yml")
    }

    /// Resolves `path` under `base` and verifies it stays inside `base`.
    /// Works for paths that do not exist yet by canonicalizing the nearest
    /// existing ancestor and then appending the remaining components.
    pub(super) fn validate_path(&self, base: &Path, path: &Path) -> Result<PathBuf, DomainError> {
        let full = base.join(path);

        let canonical_base = base
            .canonicalize()
            .map_err(|_| DomainError::NotFound("Base dir not found".into()))?;

        // Walk up to find the deepest ancestor that already exists on disk.
        let mut existing = full.as_path();
        while !existing.exists() {
            match existing.parent() {
                Some(p) => existing = p,
                None => {
                    return Err(DomainError::NotFound("Path not found".into()));
                }
            }
        }

        let canonical_existing = existing
            .canonicalize()
            .map_err(|_| DomainError::NotFound("Path not found".into()))?;

        // Reconstruct the full canonical path by appending any not-yet-existing suffix.
        let suffix = full.strip_prefix(existing).unwrap_or(Path::new(""));
        let canonical_full = if suffix == Path::new("") {
            canonical_existing
        } else {
            canonical_existing.join(suffix)
        };

        if !canonical_full.starts_with(&canonical_base) {
            return Err(DomainError::InvalidInput("Path traversal detected".into()));
        }

        Ok(canonical_full)
    }
}

impl CollectionRepository for FsCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        folders::list(self)
    }

    fn get(&self, name: &str) -> DomainResult<Collection> {
        folders::get(self, name)
    }

    fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
        folders::get_summaries(self, name)
    }

    fn create(&self, name: &str) -> DomainResult<Collection> {
        folders::create(self, name)
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        folders::delete(self, name)
    }

    fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        folders::rename(self, old_name, new_name)
    }

    fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
        requests::get_request(self, collection, path)
    }

    fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<String> {
        requests::save_request(self, collection, path, request)
    }

    fn rename_request(&self, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()> {
        requests::rename_request(self, collection, old_path, new_path)
    }

    fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        requests::delete_request(self, collection, path)
    }

    fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        folders::create_folder(self, collection, path)
    }

    fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        folders::delete_folder(self, collection, path)
    }

    fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        folders::move_item(self, src_collection, src_path, dst_collection, dst_path)
    }

    fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
        folders::reorder_items(self, collection, folder_path, ordered_names)
    }

    fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
        settings::get_settings(self, name)
    }

    fn save_settings(&self, name: &str, s: &CollectionSettings) -> DomainResult<()> {
        settings::save_settings(self, name, s)
    }

    fn get_folder_chain_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
        variables::get_folder_chain_variables(self, collection, request_path)
    }

    fn get_folder_variables(&self, collection: &str, folder_path: &str) -> DomainResult<Vec<CollectionVariable>> {
        variables::get_folder_variables(self, collection, folder_path)
    }

    fn save_folder_variables(&self, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
        variables::save_folder_variables(self, collection, folder_path, vars)
    }

    fn get_request_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
        variables::get_request_variables(self, collection, request_path)
    }

    fn save_request_variables(&self, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
        variables::save_request_variables(self, collection, request_path, vars)
    }
}
