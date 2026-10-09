use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
use rocket_shared::error::DomainResult;

use crate::FsHistoryRepo;

/// Delegates all `HistoryRepository` operations to a short-lived
/// `FsHistoryRepo` whose directory is `<active workspace>/history`, resolved
/// from a shared, mutable workspace path at call time. A reader built on it
/// always sees the current workspace's history, even after a switch.
pub struct SharedPathHistoryRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl SharedPathHistoryRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    fn repo(&self) -> FsHistoryRepo {
        let dir = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("history");
        FsHistoryRepo::new(dir)
    }
}

impl HistoryRepository for SharedPathHistoryRepo {
    fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
        self.repo().list(limit)
    }

    fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
        self.repo().get(id)
    }

    fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
        self.repo().save(entry)
    }

    fn clear(&self) -> DomainResult<()> {
        self.repo().clear()
    }

    fn search(&self, filter: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
        self.repo().search(filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_follows_the_active_workspace_path() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(first.path().to_path_buf()));
        let repo = SharedPathHistoryRepo::new(Arc::clone(&path));
        std::fs::create_dir_all(first.path().join("history")).expect("history dir");
        repo.save(&HistoryEntry::new("GET", "https://a.test/", 200, 1, 1))
            .expect("save");
        assert_eq!(repo.list(None).expect("list").len(), 1);

        *path.lock().expect("lock") = second.path().to_path_buf();
        assert!(repo.list(None).expect("list after switch").is_empty());
    }
}
