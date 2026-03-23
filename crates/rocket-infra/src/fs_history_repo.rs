use std::fs;
use std::path::PathBuf;

use rocket_history::{HistoryEntry, HistoryRepository};
use rocket_shared::error::{DomainError, DomainResult};

pub struct FsHistoryRepo {
    dir: PathBuf,
}

impl FsHistoryRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", id))
    }
}

impl HistoryRepository for FsHistoryRepo {
    fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
        let mut entries = Vec::new();
        if !self.dir.exists() {
            return Ok(entries);
        }
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(h) = serde_json::from_str::<HistoryEntry>(&content) {
                    entries.push(h);
                }
            }
        }
        // Sort by timestamp descending (most recent first).
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if let Some(n) = limit {
            entries.truncate(n);
        }
        Ok(entries)
    }

    fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
        let path = self.file_path(id);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("HistoryEntry '{}'", id)));
        }
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(entry)?;
        fs::write(self.file_path(&entry.id), json)?;
        Ok(())
    }

    fn clear(&self) -> DomainResult<()> {
        if self.dir.exists() {
            for entry in fs::read_dir(&self.dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsHistoryRepo) {
        let dir = TempDir::new().unwrap();
        let repo = FsHistoryRepo::new(dir.path().to_path_buf());
        (dir, repo)
    }

    #[test]
    fn save_and_list() {
        let (_dir, repo) = setup();
        let entry = HistoryEntry::new("GET", "https://api.example.com", 200, 100, 512);
        repo.save(&entry).unwrap();
        let list = repo.list(None).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].method, "GET");
    }

    #[test]
    fn list_with_limit() {
        let (_dir, repo) = setup();
        for i in 0..5 {
            let e = HistoryEntry::new("GET", format!("/path/{}", i), 200, 10, 0);
            repo.save(&e).unwrap();
        }
        let list = repo.list(Some(3)).unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn get_by_id() {
        let (_dir, repo) = setup();
        let entry = HistoryEntry::new("POST", "/api", 201, 50, 128);
        let id = entry.id.clone();
        repo.save(&entry).unwrap();
        let loaded = repo.get(&id).unwrap();
        assert_eq!(loaded.id, id);
    }

    #[test]
    fn clear_history() {
        let (_dir, repo) = setup();
        repo.save(&HistoryEntry::new("GET", "/a", 200, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("POST", "/b", 201, 20, 0)).unwrap();
        repo.clear().unwrap();
        assert!(repo.list(None).unwrap().is_empty());
    }
}
