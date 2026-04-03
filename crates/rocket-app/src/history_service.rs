use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};

pub struct HistoryService {
    repo: Box<dyn HistoryRepository>,
    events: Box<dyn EventPublisher>,
}

impl HistoryService {
    pub fn new(repo: Box<dyn HistoryRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self { repo, events }
    }

    pub fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
        self.repo.list(limit)
    }

    pub fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
        self.repo.get(id)
    }

    pub fn clear(&self) -> DomainResult<()> {
        self.repo.clear()?;
        self.events.publish(DomainEvent::HistoryCleared);
        Ok(())
    }

    pub fn search(&self, filter: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
        self.repo.search(filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;

    struct MockHistoryRepo {
        entries: Mutex<Vec<HistoryEntry>>,
    }

    impl MockHistoryRepo {
        fn new() -> Self {
            Self { entries: Mutex::new(Vec::new()) }
        }
    }

    impl HistoryRepository for MockHistoryRepo {
        fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
            let entries = self.entries.lock().unwrap();
            let mut result = entries.clone();
            if let Some(n) = limit {
                result.truncate(n);
            }
            Ok(result)
        }

        fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
            self.entries
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.id == id)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(id.into()))
        }

        fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }

        fn clear(&self) -> DomainResult<()> {
            self.entries.lock().unwrap().clear();
            Ok(())
        }

        fn search(&self, filter: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
            // Delegate to the default in-memory filter logic.
            let all = self.list(None)?;
            Ok(all
                .into_iter()
                .filter(|e| {
                    if let Some(m) = &filter.method {
                        if !e.method.eq_ignore_ascii_case(m) {
                            return false;
                        }
                    }
                    if let Some(p) = &filter.url_contains {
                        if !e.url.contains(p.as_str()) {
                            return false;
                        }
                    }
                    if let Some(min) = filter.status_min {
                        if e.status < min {
                            return false;
                        }
                    }
                    if let Some(max) = filter.status_max {
                        if e.status > max {
                            return false;
                        }
                    }
                    true
                })
                .collect())
        }
    }

    fn make_service() -> HistoryService {
        HistoryService::new(Box::new(MockHistoryRepo::new()), Box::new(NullEventPublisher))
    }

    #[test]
    fn list_empty_initially() {
        let svc = make_service();
        assert!(svc.list(None).unwrap().is_empty());
    }

    #[test]
    fn clear_empties_history() {
        let repo = MockHistoryRepo::new();
        repo.entries
            .lock()
            .unwrap()
            .push(HistoryEntry::new("GET", "/", 200, 10, 0));
        let svc = HistoryService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.clear().unwrap();
        assert!(svc.list(None).unwrap().is_empty());
    }

    #[test]
    fn list_with_limit() {
        let repo = MockHistoryRepo::new();
        {
            let mut entries = repo.entries.lock().unwrap();
            entries.push(HistoryEntry::new("GET", "/a", 200, 10, 0));
            entries.push(HistoryEntry::new("GET", "/b", 200, 10, 0));
            entries.push(HistoryEntry::new("GET", "/c", 200, 10, 0));
        }
        let svc = HistoryService::new(Box::new(repo), Box::new(NullEventPublisher));
        assert_eq!(svc.list(Some(2)).unwrap().len(), 2);
    }

    #[test]
    fn search_delegates_to_repo() {
        let repo = MockHistoryRepo::new();
        {
            let mut entries = repo.entries.lock().unwrap();
            entries.push(HistoryEntry::new("GET", "/users", 200, 10, 0));
            entries.push(HistoryEntry::new("POST", "/users", 201, 10, 0));
        }
        let svc = HistoryService::new(Box::new(repo), Box::new(NullEventPublisher));
        let filter = HistoryFilter { method: Some("POST".to_string()), ..Default::default() };
        let results = svc.search(&filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].method, "POST");
    }

    #[test]
    fn search_empty_filter_returns_all_entries() {
        let repo = MockHistoryRepo::new();
        {
            let mut entries = repo.entries.lock().unwrap();
            entries.push(HistoryEntry::new("GET", "/a", 200, 10, 0));
            entries.push(HistoryEntry::new("DELETE", "/b", 204, 10, 0));
        }
        let svc = HistoryService::new(Box::new(repo), Box::new(NullEventPublisher));
        let results = svc.search(&HistoryFilter::default()).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn get_entry_by_id() {
        let repo = MockHistoryRepo::new();
        let entry = HistoryEntry::new("GET", "/ping", 200, 5, 0);
        let id = entry.id.clone();
        repo.entries.lock().unwrap().push(entry);
        let svc = HistoryService::new(Box::new(repo), Box::new(NullEventPublisher));
        let found = svc.get(&id).unwrap();
        assert_eq!(found.url, "/ping");
    }

    #[test]
    fn get_nonexistent_id_returns_error() {
        let svc = make_service();
        let result = svc.get("does-not-exist");
        assert!(result.is_err(), "get with unknown id must return an error");
    }

    #[test]
    fn search_by_url_contains() {
        let repo = MockHistoryRepo::new();
        {
            let mut entries = repo.entries.lock().unwrap();
            entries.push(HistoryEntry::new("GET", "/api/users", 200, 10, 0));
            entries.push(HistoryEntry::new("GET", "/api/orders", 200, 10, 0));
            entries.push(HistoryEntry::new("GET", "/health", 200, 10, 0));
        }
        let svc = HistoryService::new(Box::new(repo), Box::new(NullEventPublisher));
        let filter = HistoryFilter { url_contains: Some("/api/".to_string()), ..Default::default() };
        let results = svc.search(&filter).unwrap();
        assert_eq!(results.len(), 2, "only /api/* entries should match");
    }

    #[test]
    fn search_by_status_range() {
        let repo = MockHistoryRepo::new();
        {
            let mut entries = repo.entries.lock().unwrap();
            entries.push(HistoryEntry::new("GET", "/a", 200, 10, 0));
            entries.push(HistoryEntry::new("GET", "/b", 404, 10, 0));
            entries.push(HistoryEntry::new("GET", "/c", 500, 10, 0));
        }
        let svc = HistoryService::new(Box::new(repo), Box::new(NullEventPublisher));
        let filter = HistoryFilter { status_min: Some(400), status_max: Some(499), ..Default::default() };
        let results = svc.search(&filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, 404);
    }

    #[test]
    fn search_method_is_case_insensitive() {
        let repo = MockHistoryRepo::new();
        repo.entries.lock().unwrap().push(HistoryEntry::new("GET", "/", 200, 10, 0));
        let svc = HistoryService::new(Box::new(repo), Box::new(NullEventPublisher));
        let filter = HistoryFilter { method: Some("get".to_string()), ..Default::default() };
        let results = svc.search(&filter).unwrap();
        assert_eq!(results.len(), 1, "method filter must be case-insensitive");
    }
}
