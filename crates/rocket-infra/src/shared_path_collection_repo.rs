use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    Request,
};
use rocket_shared::error::DomainResult;

use crate::FsCollectionRepo;

/// Delegates all `CollectionRepository` operations to a short-lived
/// `FsCollectionRepo` whose base directory is resolved from a shared,
/// mutable workspace path at call time. This lets the active workspace change
/// at runtime without rebuilding the Tauri service graph.
pub struct SharedPathCollectionRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
    collection_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl SharedPathCollectionRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
            collection_locks: Arc::new(DashMap::new()),
        }
    }

    fn repo(&self) -> FsCollectionRepo {
        let base = self.active_workspace_path.lock().unwrap_or_else(|e| e.into_inner()).join("collections");
        FsCollectionRepo::new(base, Arc::clone(&self.collection_locks))
    }
}

impl CollectionRepository for SharedPathCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        self.repo().list()
    }

    fn get(&self, name: &str) -> DomainResult<Collection> {
        self.repo().get(name)
    }

    fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
        self.repo().get_summaries(name)
    }

    fn create(&self, name: &str) -> DomainResult<Collection> {
        self.repo().create(name)
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo().delete(name)
    }

    fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        self.repo().rename(old_name, new_name)
    }

    fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
        self.repo().get_request(collection, path)
    }

    fn save_request(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<String> {
        self.repo().save_request(collection, path, request)
    }

    fn rename_request(
        &self,
        collection: &str,
        old_path: &str,
        new_name: &str,
    ) -> DomainResult<()> {
        self.repo().rename_request(collection, old_path, new_name)
    }

    fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo().delete_request(collection, path)
    }

    fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo().create_folder(collection, path)
    }

    fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo().delete_folder(collection, path)
    }

    fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        self.repo().move_item(src_collection, src_path, dst_collection, dst_path)
    }

    fn reorder_items(
        &self,
        collection: &str,
        folder_path: &str,
        ordered_names: &[String],
    ) -> DomainResult<()> {
        self.repo().reorder_items(collection, folder_path, ordered_names)
    }

    fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
        self.repo().get_settings(name)
    }

    fn save_settings(
        &self,
        name: &str,
        settings: &CollectionSettings,
    ) -> DomainResult<()> {
        self.repo().save_settings(name, settings)
    }

    fn get_folder_chain_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo().get_folder_chain_variables(collection, request_path)
    }

    fn get_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo().get_folder_variables(collection, folder_path)
    }

    fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.repo().save_folder_variables(collection, folder_path, vars)
    }

    fn get_request_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo().get_request_variables(collection, request_path)
    }

    fn save_request_variables(
        &self,
        collection: &str,
        request_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.repo().save_request_variables(collection, request_path, vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::CollectionRepository;
    use rocket_shared::types::HttpMethod;
    use tempfile::TempDir;

    fn setup() -> (TempDir, SharedPathCollectionRepo) {
        let dir = TempDir::new().unwrap();
        let path = Arc::new(Mutex::new(dir.path().to_path_buf()));
        let repo = SharedPathCollectionRepo::new(Arc::clone(&path));
        (dir, repo)
    }

    // --- Path resolution ---

    #[test]
    fn resolves_collections_subdirectory_under_workspace() {
        let (dir, repo) = setup();
        // The `collections/` sub-dir does not yet exist; list() must still succeed.
        let result = repo.list();
        assert!(
            result.is_ok(),
            "list() on an empty workspace should succeed, got: {:?}",
            result
        );
        assert!(result.unwrap().is_empty());

        // Verify that creating a collection writes inside `<workspace>/collections/`.
        repo.create("my-api").unwrap();
        let expected = dir.path().join("collections").join("my-api");
        assert!(expected.exists(), "collection directory should be at {expected:?}");
    }

    // --- Runtime path switching ---

    #[test]
    fn switching_workspace_path_redirects_subsequent_calls() {
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();

        let shared_path = Arc::new(Mutex::new(dir_a.path().to_path_buf()));
        let repo = SharedPathCollectionRepo::new(Arc::clone(&shared_path));

        // Create a collection in workspace A.
        repo.create("alpha").unwrap();
        assert_eq!(repo.list().unwrap().len(), 1);

        // Switch the shared path to workspace B — no rebuild needed.
        *shared_path.lock().unwrap() = dir_b.path().to_path_buf();

        // The repo now operates on workspace B: alpha should not be visible.
        let list_b = repo.list().unwrap();
        assert!(
            list_b.is_empty(),
            "after workspace switch, list() should show workspace B's collections"
        );

        // Collections created now go into workspace B.
        repo.create("beta").unwrap();
        assert!(dir_b.path().join("collections").join("beta").exists());
        assert!(!dir_a.path().join("collections").join("beta").exists());
    }

    // --- Delegation correctness (smoke tests) ---

    #[test]
    fn create_and_list_roundtrip() {
        let (_dir, repo) = setup();
        repo.create("api-v1").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "api-v1");
    }

    #[test]
    fn save_and_get_request() {
        let (_dir, repo) = setup();
        repo.create("api-v1").unwrap();
        let req = rocket_collection::Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        repo.save_request("api-v1", "get-users.yml", &req).unwrap();
        let loaded = repo.get_request("api-v1", "get-users.yml").unwrap();
        assert_eq!(loaded.name, "Get Users");
    }

    #[test]
    fn delete_collection() {
        let (_dir, repo) = setup();
        repo.create("to-delete").unwrap();
        repo.delete("to-delete").unwrap();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn rename_collection() {
        let (_dir, repo) = setup();
        repo.create("old-name").unwrap();
        repo.rename("old-name", "new-name").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "new-name");
    }

    // --- Lock sharing ---

    #[test]
    fn concurrent_saves_to_same_collection_both_complete_without_error() {
        // Operations such as `save_request` acquire the per-collection mutex from
        // the shared DashMap before writing. When two calls race, the second blocks
        // until the first releases the lock — both must complete successfully rather
        // than racing to corrupt the file.
        let (_dir, repo) = setup();
        let repo = Arc::new(repo);

        repo.create("shared-col").unwrap();
        let req_a = rocket_collection::Request::new("Req A", HttpMethod::Get, "/a");
        let req_b = rocket_collection::Request::new("Req B", HttpMethod::Post, "/b");

        let r1 = Arc::clone(&repo);
        let r2 = Arc::clone(&repo);

        let h1 = std::thread::spawn(move || r1.save_request("shared-col", "req-a.yml", &req_a));
        let h2 = std::thread::spawn(move || r2.save_request("shared-col", "req-b.yml", &req_b));

        h1.join().unwrap().expect("first concurrent save_request should succeed");
        h2.join().unwrap().expect("second concurrent save_request should succeed");

        // Both files must now be readable.
        let loaded_a = repo.get_request("shared-col", "req-a.yml").unwrap();
        let loaded_b = repo.get_request("shared-col", "req-b.yml").unwrap();
        assert_eq!(loaded_a.name, "Req A");
        assert_eq!(loaded_b.name, "Req B");
    }
}
