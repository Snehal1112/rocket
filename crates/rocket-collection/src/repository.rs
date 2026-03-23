use rocket_shared::error::DomainResult;

use crate::collection::Collection;
use crate::request::Request;
use crate::summary::CollectionSummary;

/// Repository trait for Collection persistence.
/// Implemented by FsCollectionRepo in rocket-infra.
pub trait CollectionRepository: Send + Sync {
    /// List all collections (lightweight summaries).
    fn list(&self) -> DomainResult<Vec<CollectionSummary>>;

    /// Get full collection tree by name.
    fn get(&self, name: &str) -> DomainResult<Collection>;

    /// Create a new empty collection.
    fn create(&self, name: &str) -> DomainResult<Collection>;

    /// Delete a collection and all its contents.
    fn delete(&self, name: &str) -> DomainResult<()>;

    /// Rename a collection.
    fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()>;

    /// Read a single request file by collection name and relative path.
    fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request>;

    /// Save a request to a specific path within a collection.
    fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<()>;

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
