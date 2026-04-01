use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
}

impl SharedPathCollectionRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self { active_workspace_path }
    }

    fn repo(&self) -> FsCollectionRepo {
        let base = self.active_workspace_path.lock().expect("active workspace path lock poisoned").join("collections");
        FsCollectionRepo::new(base)
    }
}

impl CollectionRepository for SharedPathCollectionRepo {
    fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        self.repo().list()
    }

    fn get(&self, name: &str) -> DomainResult<Collection> {
        self.repo().get(name)
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
