use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_flow::{Flow, FlowRepository};
use rocket_shared::error::DomainResult;

use crate::FsFlowRepo;

/// Delegates all `FlowRepository` operations to a short-lived `FsFlowRepo`
/// whose base directory is resolved from a shared, mutable workspace path
/// at call time — mirrors `SharedPathCollectionRepo` exactly, so Flow CRUD
/// and Flow execution follow workspace switches without rebuilding the
/// Tauri service graph.
pub struct SharedPathFlowRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl SharedPathFlowRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    fn repo(&self) -> FsFlowRepo {
        let base = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("collections");
        FsFlowRepo::new(base)
    }
}

impl FlowRepository for SharedPathFlowRepo {
    fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        self.repo().list(collection)
    }

    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        self.repo().get(collection, name)
    }

    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
        self.repo().save(collection, flow)
    }

    fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        self.repo().delete(collection, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    #[test]
    fn switching_workspace_path_redirects_subsequent_calls() {
        let dir_a = TempDir::new().expect("temp dir a");
        let dir_b = TempDir::new().expect("temp dir b");
        std::fs::create_dir_all(dir_a.path().join("collections").join("acme"))
            .expect("create collection a");
        std::fs::create_dir_all(dir_b.path().join("collections").join("acme"))
            .expect("create collection b");

        let shared_path = Arc::new(Mutex::new(dir_a.path().to_path_buf()));
        let repo = SharedPathFlowRepo::new(Arc::clone(&shared_path));

        repo.save("acme", &sample("A-side Flow")).expect("save in workspace a");
        assert_eq!(repo.list("acme").expect("list a").len(), 1);

        *shared_path.lock().expect("lock shared path") = dir_b.path().to_path_buf();

        assert!(
            repo.list("acme").expect("list b").is_empty(),
            "after workspace switch, list() should show workspace B's flows"
        );
    }
}
