use rocket_shared::error::DomainResult;

use crate::collection::Collection;
use crate::request::Request;
use crate::settings::{CollectionSettings, CollectionVariable};
use crate::summary::CollectionSummary;

/// Repository trait for Collection persistence.
/// Implemented by FsCollectionRepo in rocket-infra.
pub trait CollectionRepository: Send + Sync {
    /// List all collections (lightweight summaries).
    fn list(&self) -> DomainResult<Vec<CollectionSummary>>;

    /// Get full collection tree by name.
    fn get(&self, name: &str) -> DomainResult<Collection>;

    /// Get collection tree with lightweight request summaries instead of full Request bodies.
    /// Use for sidebar loads; call `get_request` for the full body when the user opens a request.
    fn get_summaries(&self, name: &str) -> DomainResult<Collection>;

    /// Create a new empty collection.
    fn create(&self, name: &str) -> DomainResult<Collection>;

    /// Delete a collection and all its contents.
    fn delete(&self, name: &str) -> DomainResult<()>;

    /// Rename a collection.
    fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()>;

    /// Read a single request file by collection name and relative path.
    fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request>;

    /// Save a request to a specific path within a collection.
    /// Returns the actual filename used (may differ from `path` if a unique name was generated).
    ///
    /// **Variables ownership:** `request.variables` is NOT authoritative for the stored
    /// `runtime.variables` block. Use `save_request_variables` to mutate variables; callers
    /// that pass an empty `request.variables` will have the existing on-disk variables preserved.
    fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<String>;

    /// Rename a request file within a collection (fs::rename, single event).
    fn rename_request(&self, collection: &str, old_path: &str, new_path: &str) -> DomainResult<()>;

    /// Delete a request file.
    fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Create a folder within a collection.
    fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Delete a folder and its contents.
    fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()>;

    /// Move a request or folder within or across collections.
    fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()>;

    /// Write an explicit ordering for items in a folder within a collection.
    /// `folder_path` is relative to the collection root; pass `""` for the root.
    /// `ordered_names` is the full ordered list of entry names (files include `.json`).
    fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()>;

    /// Read collection-level settings (auth, headers) from collection.json.
    /// Returns default settings if the file does not exist.
    fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings>;

    /// Persist collection-level settings to collection.json.
    fn save_settings(&self, name: &str, settings: &CollectionSettings) -> DomainResult<()>;

    /// Walk the full folder ancestor chain for a request path and return
    /// merged variables (outer folder first; inner folder wins on collision).
    fn get_folder_chain_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>>;

    /// Read only this folder's own variables from its folder.yml (no chain walk).
    /// Returns an empty vec if the folder or its folder.yml does not exist.
    fn get_folder_variables(&self, collection: &str, folder_path: &str) -> DomainResult<Vec<CollectionVariable>>;

    /// Persist folder-level variables to the folder's folder.yml.
    fn save_folder_variables(&self, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()>;

    /// Read request-level variables from a request .yml file's runtime.variables[].
    fn get_request_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>>;

    /// Persist request-level variables to a request .yml file's runtime.variables[].
    fn save_request_variables(&self, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        // Compile-time check.
        fn _assert_object_safe(_: Box<dyn CollectionRepository>) {}
    }
}
