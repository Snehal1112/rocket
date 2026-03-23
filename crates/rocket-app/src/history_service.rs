use rocket_history::{HistoryEntry, HistoryRepository};
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
}
