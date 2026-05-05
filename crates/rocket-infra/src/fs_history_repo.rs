use std::fs;
use std::path::PathBuf;

use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

pub struct FsHistoryRepo {
    dir: PathBuf,
}

impl FsHistoryRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.yml", id))
    }
}

impl HistoryRepository for FsHistoryRepo {
    fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }

        // Collect file paths with mtime so we can sort before parsing.
        let mut paths: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "yml") {
                let mtime = entry
                    .metadata()?
                    .modified()
                    .unwrap_or(std::time::UNIX_EPOCH);
                paths.push((mtime, path));
            }
        }
        // Most recently written first — mtime is a reliable proxy for entry.timestamp
        // because history files are written once and never modified.
        paths.sort_by(|a, b| b.0.cmp(&a.0));

        let cap = limit.unwrap_or(usize::MAX);
        let mut entries = Vec::with_capacity(cap.min(paths.len()));
        for (_, path) in paths {
            if entries.len() >= cap {
                break;
            }
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Ok(h) = serde_yaml::from_str::<HistoryEntry>(&content) {
                entries.push(h);
            }
        }
        // Secondary sort by timestamp in case mtime ties (same-second writes).
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(entries)
    }

    fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
        let path = self.file_path(id);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("HistoryEntry '{}'", id)));
        }
        let content = fs::read_to_string(&path)?;
        serde_yaml::from_str(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse YAML: {e}")))
    }

    fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
        fs::create_dir_all(&self.dir)?;
        let yaml = serde_yaml::to_string(entry)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize YAML: {e}")))?;
        atomic_write(&self.file_path(&entry.id), yaml.as_bytes())?;
        Ok(())
    }

    fn clear(&self) -> DomainResult<()> {
        if self.dir.exists() {
            for entry in fs::read_dir(&self.dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "yml") {
                    fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }

    fn search(&self, filter: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
        const SEARCH_LIMIT: usize = 200;

        if !self.dir.exists() {
            return Ok(vec![]);
        }

        // Collect and sort file paths by modified-time descending before reading,
        // so we return the most-recent matches and can stop early once the cap is hit.
        let mut paths: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "yml") {
                let mtime = entry.metadata()?.modified().unwrap_or(std::time::UNIX_EPOCH);
                paths.push((mtime, path));
            }
        }
        paths.sort_by(|a, b| b.0.cmp(&a.0));

        let mut results = Vec::new();
        for (_, path) in paths {
            if results.len() >= SEARCH_LIMIT {
                break;
            }
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let entry: HistoryEntry = match serde_yaml::from_str(&content) {
                Ok(e) => e,
                Err(_) => continue,
            };
            if let Some(method) = &filter.method {
                if !entry.method.eq_ignore_ascii_case(method) {
                    continue;
                }
            }
            if let Some(url_pattern) = &filter.url_contains {
                if !entry.url.contains(url_pattern.as_str()) {
                    continue;
                }
            }
            if let Some(min) = filter.status_min {
                if entry.status < min {
                    continue;
                }
            }
            if let Some(max) = filter.status_max {
                if entry.status > max {
                    continue;
                }
            }
            results.push(entry);
        }
        Ok(results)
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

    #[test]
    fn search_empty_filter_returns_all() {
        let (_dir, repo) = setup();
        repo.save(&HistoryEntry::new("GET", "/a", 200, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("POST", "/b", 201, 20, 0)).unwrap();
        let results = repo.search(&HistoryFilter::default()).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_by_method_returns_matching_entries() {
        let (_dir, repo) = setup();
        repo.save(&HistoryEntry::new("GET", "/a", 200, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("POST", "/b", 201, 20, 0)).unwrap();
        repo.save(&HistoryEntry::new("get", "/c", 204, 5, 0)).unwrap();
        let filter = HistoryFilter { method: Some("GET".to_string()), ..Default::default() };
        let results = repo.search(&filter).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.method.to_uppercase() == "GET"));
    }

    #[test]
    fn search_by_url_contains_returns_matching_entries() {
        let (_dir, repo) = setup();
        repo.save(&HistoryEntry::new("GET", "https://api.example.com/users", 200, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("GET", "https://api.example.com/items", 200, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("GET", "https://other.io/users", 200, 10, 0)).unwrap();
        let filter = HistoryFilter { url_contains: Some("example.com".to_string()), ..Default::default() };
        let results = repo.search(&filter).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.url.contains("example.com")));
    }

    #[test]
    fn search_by_status_range_returns_2xx_only() {
        let (_dir, repo) = setup();
        repo.save(&HistoryEntry::new("GET", "/ok", 200, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("GET", "/created", 201, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("GET", "/not-found", 404, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("GET", "/error", 500, 10, 0)).unwrap();
        let filter = HistoryFilter { status_min: Some(200), status_max: Some(299), ..Default::default() };
        let results = repo.search(&filter).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.status >= 200 && e.status <= 299));
    }

    #[test]
    fn search_combined_method_and_status_filters() {
        let (_dir, repo) = setup();
        repo.save(&HistoryEntry::new("GET", "/a", 200, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("GET", "/b", 404, 10, 0)).unwrap();
        repo.save(&HistoryEntry::new("POST", "/c", 200, 10, 0)).unwrap();
        let filter = HistoryFilter {
            method: Some("GET".to_string()),
            status_min: Some(200),
            status_max: Some(299),
            ..Default::default()
        };
        let results = repo.search(&filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].method, "GET");
        assert_eq!(results[0].status, 200);
    }

    #[test]
    fn list_with_limit_reads_only_needed_files() {
        // This test verifies correctness of the mtime-first approach, not I/O count
        // (we cannot intercept fs calls in unit tests). It saves 10 entries and asserts
        // that list(3) returns the 3 most recently *saved* ones.
        let (_dir, repo) = setup();
        let mut ids = Vec::new();
        for i in 0..10u64 {
            let e = HistoryEntry::new("GET", format!("/path/{}", i), 200, i, 0);
            ids.push(e.id.clone());
            repo.save(&e).unwrap();
            // Sleep 2 ms so mtime is strictly increasing.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let list = repo.list(Some(3)).unwrap();
        assert_eq!(list.len(), 3);
        // The 3 most recently written entries (ids 7, 8, 9) must be returned.
        let returned_ids: std::collections::HashSet<_> = list.iter().map(|e| e.id.as_str()).collect();
        for id in &ids[7..] {
            assert!(returned_ids.contains(id.as_str()), "expected id {} in results", id);
        }
    }
}
