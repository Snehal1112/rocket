use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    FolderSettings, GraphQlRequest, GrpcRequest, Request, RequestKind, RequestScriptPhase,
    WebSocketRequest,
};
use rocket_shared::error::{DomainError, DomainResult};

pub(crate) mod folder_file;
mod folder_settings;
mod folders;
mod paths;
mod requests;
mod script_files;
mod settings;
mod tree;
mod variables;

#[cfg(test)]
mod tests;

// Kept in its own group so rustfmt does not sort it before `tests`.
#[cfg(test)]
mod schema_shape_tests;

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
    fn collection_root_path(&self, name: &str) -> DomainResult<PathBuf> {
        Collection::validate_name(name)?;
        let path = self.collection_path(name);
        if !path.is_dir() {
            return Err(DomainError::NotFound(format!(
                "Collection '{name}' not found"
            )));
        }
        Ok(path)
    }

    fn create_script_file(
        &self,
        collection: &str,
        folder_path: &str,
        name: &str,
    ) -> DomainResult<String> {
        script_files::create_script_file(self, collection, folder_path, name)
    }

    fn read_script_file(&self, collection: &str, path: &str) -> DomainResult<String> {
        script_files::read_script_file(self, collection, path)
    }

    fn save_script_file(&self, collection: &str, path: &str, content: &str) -> DomainResult<()> {
        script_files::save_script_file(self, collection, path, content)
    }

    fn rename_script_file(
        &self,
        collection: &str,
        path: &str,
        new_name: &str,
    ) -> DomainResult<String> {
        script_files::rename_script_file(self, collection, path, new_name)
    }

    fn delete_script_file(&self, collection: &str, path: &str) -> DomainResult<()> {
        script_files::delete_script_file(self, collection, path)
    }

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

    fn save_request(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<String> {
        requests::save_request(self, collection, path, request)
    }

    fn rename_request(&self, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()> {
        requests::rename_request(self, collection, old_path, new_path)
    }

    fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        requests::delete_request(self, collection, path)
    }

    fn get_graphql_request(&self, collection: &str, path: &str) -> DomainResult<GraphQlRequest> {
        requests::get_graphql_request(self, collection, path)
    }

    fn get_websocket_request(
        &self,
        collection: &str,
        path: &str,
    ) -> DomainResult<WebSocketRequest> {
        requests::get_websocket_request(self, collection, path)
    }

    fn save_websocket_request(
        &self,
        collection: &str,
        path: &str,
        request: &WebSocketRequest,
    ) -> DomainResult<String> {
        requests::save_websocket_request(self, collection, path, request)
    }

    fn get_grpc_request(&self, collection: &str, path: &str) -> DomainResult<GrpcRequest> {
        requests::get_grpc_request(self, collection, path)
    }

    fn save_grpc_request(
        &self,
        collection: &str,
        path: &str,
        request: &GrpcRequest,
    ) -> DomainResult<String> {
        requests::save_grpc_request(self, collection, path, request)
    }

    fn save_graphql_request(
        &self,
        collection: &str,
        path: &str,
        request: &GraphQlRequest,
    ) -> DomainResult<String> {
        requests::save_graphql_request(self, collection, path, request)
    }

    fn request_kind(&self, collection: &str, path: &str) -> DomainResult<RequestKind> {
        requests::request_kind(self, collection, path)
    }

    fn path_exists(&self, collection: &str, path: &str) -> DomainResult<bool> {
        paths::path_exists(self, collection, path)
    }

    fn create_folder_exclusive(&self, collection: &str, path: &str) -> DomainResult<()> {
        folders::create_folder_exclusive(self, collection, path)
    }

    fn create_request_exclusive(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<String> {
        requests::create_request_exclusive(self, collection, path, request)
    }

    fn move_item_no_replace(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        folders::move_item_impl(self, src_collection, src_path, dst_collection, dst_path, true)
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

    fn reorder_items(
        &self,
        collection: &str,
        folder_path: &str,
        ordered_names: &[String],
    ) -> DomainResult<()> {
        folders::reorder_items(self, collection, folder_path, ordered_names)
    }

    fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
        settings::get_settings(self, name)
    }

    fn save_settings(&self, name: &str, s: &CollectionSettings) -> DomainResult<()> {
        settings::save_settings(self, name, s)
    }

    fn get_folder_chain_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        variables::get_folder_chain_variables(self, collection, request_path)
    }

    fn get_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        variables::get_folder_variables(self, collection, folder_path)
    }

    fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        variables::save_folder_variables(self, collection, folder_path, vars)
    }

    fn get_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<FolderSettings> {
        folder_settings::get_folder_settings(self, collection, folder_path)
    }

    fn save_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
        settings: &FolderSettings,
    ) -> DomainResult<()> {
        folder_settings::save_folder_settings(self, collection, folder_path, settings)
    }

    fn get_folder_chain_settings(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<FolderSettings>> {
        folder_settings::get_folder_chain_settings(self, collection, request_path)
    }

    fn get_request_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        variables::get_request_variables(self, collection, request_path)
    }

    fn save_request_variables(
        &self,
        collection: &str,
        request_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        variables::save_request_variables(self, collection, request_path, vars)
    }

    fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        variables::save_request_script(self, collection, request_path, phase, body)
    }
}
