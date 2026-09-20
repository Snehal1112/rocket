use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{Collection, CollectionRepository, CollectionSummary, CollectionVariable, Request};
use rocket_shared::description::Documentation;
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use std::sync::Arc;

pub struct CollectionService {
    repo: Box<dyn CollectionRepository>,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
}

impl CollectionService {
    pub fn new(repo: Box<dyn CollectionRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self {
            repo,
            events,
            audit: Arc::new(NullSecurityAuditPublisher),
        }
    }

    pub fn new_with_audit(
        repo: Box<dyn CollectionRepository>,
        events: Box<dyn EventPublisher>,
        audit: Arc<dyn SecurityAuditPublisher>,
    ) -> Self {
        Self { repo, events, audit }
    }

    pub fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
        self.repo.list()
    }

    pub fn get(&self, name: &str) -> DomainResult<Collection> {
        self.repo.get(name)
    }

    /// Get collection tree with lightweight request summaries for sidebar display.
    pub fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
        self.repo.get_summaries(name)
    }

    pub fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        let collection = self.repo.create(name)?;
        self.events.publish(DomainEvent::CollectionCreated { name: name.to_string() });
        Ok(collection)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.audit.publish(
            "system".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: name.to_string() },
        );
        self.events.publish(DomainEvent::CollectionDeleted { name: name.to_string() });
        Ok(())
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        Collection::validate_name(new_name)?;
        self.repo.rename(old_name, new_name)?;
        self.events.publish(DomainEvent::CollectionRenamed {
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
        });
        Ok(())
    }

    pub fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<Request> {
        let actual_path = self.repo.save_request(collection, path, request)?;
        let saved = self.repo.get_request(collection, &actual_path)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path,
        });
        Ok(saved)
    }

    pub fn rename_request(&self, collection: &str, old_path: &str, new_name: &str) -> DomainResult<()> {
        // Only update the name field inside the JSON. The filename stays the same.
        // This produces a single Modify filesystem event.
        let mut request = self.repo.get_request(collection, old_path)?;
        request.name = new_name.to_string();
        self.repo.save_request(collection, old_path, &request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: old_path.to_string(),
        });
        Ok(())
    }

    pub fn update_request_docs(&self, collection: &str, path: &str, docs: Option<String>) -> DomainResult<()> {
        let mut request = self.repo.get_request(collection, path)?;
        request.docs = docs.map(Documentation::text);
        self.repo.save_request(collection, path, &request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_request(collection, path)?;
        self.events.publish(DomainEvent::RequestDeleted {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.create_folder(collection, path)?;
        self.events.publish(DomainEvent::FolderCreated {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_folder(collection, path)?;
        self.events.publish(DomainEvent::FolderDeleted {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    pub fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        self.repo.move_item(src_collection, src_path, dst_collection, dst_path)?;
        self.events.publish(DomainEvent::ItemMoved {
            src_collection: src_collection.to_string(),
            src_path: src_path.to_string(),
            dst_collection: dst_collection.to_string(),
            dst_path: dst_path.to_string(),
        });
        Ok(())
    }

    pub fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
        self.repo.reorder_items(collection, folder_path, ordered_names)
    }

    pub fn get_settings(&self, name: &str) -> DomainResult<rocket_collection::CollectionSettings> {
        self.repo.get_settings(name)
    }

    pub fn save_settings(
        &self,
        name: &str,
        settings: &rocket_collection::CollectionSettings,
    ) -> DomainResult<()> {
        self.repo.save_settings(name, settings)
    }

    pub fn get_folder_chain_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
        self.repo.get_folder_chain_variables(collection, request_path)
    }

    pub fn get_folder_variables(&self, collection: &str, folder_path: &str) -> DomainResult<Vec<CollectionVariable>> {
        self.repo.get_folder_variables(collection, folder_path)
    }

    pub fn save_folder_variables(&self, collection: &str, folder_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
        self.repo.save_folder_variables(collection, folder_path, vars)
    }

    pub fn get_request_variables(&self, collection: &str, request_path: &str) -> DomainResult<Vec<CollectionVariable>> {
        self.repo.get_request_variables(collection, request_path)
    }

    pub fn save_request_variables(&self, collection: &str, request_path: &str, vars: Vec<CollectionVariable>) -> DomainResult<()> {
        self.repo.save_request_variables(collection, request_path, vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::{DomainEvent, EventPublisher, NullEventPublisher};
    use rocket_shared::types::HttpMethod;
    use std::sync::{Arc, Mutex};

    struct MockCollectionRepo {
        collections: Mutex<Vec<Collection>>,
        requests: Mutex<Vec<(String, String, Request)>>,
    }

    impl MockCollectionRepo {
        fn new() -> Self {
            Self { collections: Mutex::new(Vec::new()), requests: Mutex::new(Vec::new()) }
        }
    }

    impl CollectionRepository for MockCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            let cols = self.collections.lock().unwrap();
            Ok(cols
                .iter()
                .map(|c| CollectionSummary::new(String::new(), &c.name, "", c.request_count(), None))
                .collect())
        }

        fn get(&self, name: &str) -> DomainResult<Collection> {
            let cols = self.collections.lock().unwrap();
            cols.iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }

        fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
            self.get(name)
        }

        fn create(&self, name: &str) -> DomainResult<Collection> {
            let mut cols = self.collections.lock().unwrap();
            if cols.iter().any(|c| c.name == name) {
                return Err(DomainError::AlreadyExists(name.into()));
            }
            let col = Collection::new(name);
            cols.push(col.clone());
            Ok(col)
        }

        fn delete(&self, name: &str) -> DomainResult<()> {
            let mut cols = self.collections.lock().unwrap();
            cols.retain(|c| c.name != name);
            Ok(())
        }

        fn rename(&self, old: &str, new: &str) -> DomainResult<()> {
            let mut cols = self.collections.lock().unwrap();
            if let Some(c) = cols.iter_mut().find(|c| c.name == old) {
                c.name = new.to_string();
                Ok(())
            } else {
                Err(DomainError::NotFound(old.into()))
            }
        }

        fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
            self.requests
                .lock()
                .expect("lock")
                .iter()
                .find(|(c, p, _)| c == collection && p == path)
                .map(|(_, _, r)| r.clone())
                .ok_or_else(|| DomainError::NotFound(format!("{collection}/{path}")))
        }
        fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<String> {
            let mut requests = self.requests.lock().expect("lock");
            requests.retain(|(c, p, _)| !(c == collection && p == path));
            requests.push((collection.to_string(), path.to_string(), request.clone()));
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
            let mut requests = self.requests.lock().expect("lock");
            let len_before = requests.len();
            requests.retain(|(c, p, _)| !(c == collection && p == path));
            if requests.len() == len_before {
                return Err(DomainError::NotFound(format!("{collection}/{path}")));
            }
            Ok(())
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { Ok(()) }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> { Ok(()) }
        fn get_settings(&self, _: &str) -> DomainResult<rocket_collection::CollectionSettings> {
            Ok(rocket_collection::CollectionSettings::default())
        }
        fn save_settings(&self, _: &str, _: &rocket_collection::CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn save_folder_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> { Ok(()) }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> { Ok(vec![]) }
        fn save_request_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> { Ok(()) }
    }

    fn make_service() -> CollectionService {
        CollectionService::new(Box::new(MockCollectionRepo::new()), Box::new(NullEventPublisher))
    }

    #[test]
    fn create_and_list() {
        let svc = make_service();
        svc.create("my-api").unwrap();
        let list = svc.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn create_validates_name() {
        let svc = make_service();
        assert!(svc.create("").is_err());
        assert!(svc.create("has/slash").is_err());
    }

    #[test]
    fn rename() {
        let svc = make_service();
        svc.create("old").unwrap();
        svc.rename("old", "new").unwrap();
        let list = svc.list().unwrap();
        assert_eq!(list[0].name, "new");
    }

    #[test]
    fn delete_removes_collection() {
        let svc = make_service();
        svc.create("temp").unwrap();
        svc.delete("temp").unwrap();
        assert!(svc.list().unwrap().is_empty());
    }

    #[test]
    fn get_existing_collection() {
        let svc = make_service();
        svc.create("my-col").unwrap();
        let col = svc.get("my-col").unwrap();
        assert_eq!(col.name, "my-col");
    }

    #[test]
    fn get_nonexistent_collection_returns_error() {
        let svc = make_service();
        let result = svc.get("ghost");
        assert!(result.is_err(), "getting a non-existent collection must fail");
    }

    #[test]
    fn rename_validates_new_name() {
        let svc = make_service();
        svc.create("col").unwrap();
        assert!(svc.rename("col", "").is_err(), "empty new name must be rejected");
        assert!(svc.rename("col", "bad/name").is_err(), "slash in new name must be rejected");
    }

    #[test]
    fn list_empty_initially() {
        let svc = make_service();
        assert!(svc.list().unwrap().is_empty());
    }

    struct CapturingPublisher {
        captured: Mutex<Vec<AuditEventKind>>,
    }
    impl SecurityAuditPublisher for CapturingPublisher {
        fn publish(&self, _actor: String, _workspace_id: Option<String>, kind: AuditEventKind) {
            self.captured.lock().unwrap().push(kind);
        }
    }

    struct RecordingEventPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }
    impl EventPublisher for RecordingEventPublisher {
        fn publish(&self, event: DomainEvent) {
            self.events.lock().expect("lock").push(event);
        }
    }
    struct SharedEventPublisher(Arc<RecordingEventPublisher>);
    impl EventPublisher for SharedEventPublisher {
        fn publish(&self, event: DomainEvent) {
            self.0.publish(event);
        }
    }

    #[test]
    fn create_emits_collection_created() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("my-api").expect("create");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(e, DomainEvent::CollectionCreated { name } if name == "my-api")),
            "expected CollectionCreated, got {:?}", *published
        );
    }

    #[test]
    fn delete_emits_collection_deleted() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("temp").expect("create");
        svc.delete("temp").expect("delete");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(e, DomainEvent::CollectionDeleted { name } if name == "temp")),
            "expected CollectionDeleted, got {:?}", *published
        );
    }

    #[test]
    fn rename_emits_collection_renamed() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("old").expect("create");
        svc.rename("old", "new").expect("rename");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::CollectionRenamed { old_name, new_name } if old_name == "old" && new_name == "new"
            )),
            "expected CollectionRenamed, got {:?}", *published
        );
    }

    #[test]
    fn save_request_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        svc.save_request("my-api", "users.yml", &request).expect("save_request");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestSaved { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestSaved, got {:?}", *published
        );
    }

    #[test]
    fn delete_request_emits_request_deleted() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        svc.save_request("my-api", "users.yml", &request).expect("save_request");
        svc.delete_request("my-api", "users.yml").expect("delete_request");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestDeleted { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestDeleted, got {:?}", *published
        );
    }

    #[test]
    fn delete_request_on_missing_request_publishes_nothing() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let result = svc.delete_request("my-api", "ghost.yml");
        assert!(result.is_err(), "deleting a nonexistent request must fail");
        assert!(publisher.events.lock().expect("lock").is_empty(), "no event should publish on failure");
    }

    #[test]
    fn rename_request_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        svc.save_request("my-api", "users.yml", &request).expect("save_request");
        publisher.events.lock().expect("lock").clear();
        svc.rename_request("my-api", "users.yml", "List Users").expect("rename_request");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestSaved { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestSaved, got {:?}", *published
        );
    }

    #[test]
    fn update_request_docs_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new("Get Users", HttpMethod::Get, "https://api.example.com/users");
        svc.save_request("my-api", "users.yml", &request).expect("save_request");
        publisher.events.lock().expect("lock").clear();
        svc.update_request_docs("my-api", "users.yml", Some("Fetches all users.".into())).expect("update_request_docs");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestSaved { collection, path } if collection == "my-api" && path == "users.yml"
            )),
            "expected RequestSaved, got {:?}", *published
        );
    }

    #[test]
    fn move_item_emits_item_moved() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.move_item("my-api", "users.yml", "other-api", "users.yml").expect("move_item");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::ItemMoved { src_collection, src_path, dst_collection, dst_path }
                    if src_collection == "my-api" && src_path == "users.yml"
                        && dst_collection == "other-api" && dst_path == "users.yml"
            )),
            "expected ItemMoved, got {:?}", *published
        );
    }

    #[test]
    fn create_folder_emits_folder_created() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create_folder("my-api", "auth").expect("create_folder");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::FolderCreated { collection, path } if collection == "my-api" && path == "auth"
            )),
            "expected FolderCreated, got {:?}", *published
        );
    }

    #[test]
    fn delete_folder_emits_folder_deleted() {
        let publisher = Arc::new(RecordingEventPublisher { events: Mutex::new(vec![]) });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.delete_folder("my-api", "auth").expect("delete_folder");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::FolderDeleted { collection, path } if collection == "my-api" && path == "auth"
            )),
            "expected FolderDeleted, got {:?}", *published
        );
    }

    #[test]
    fn delete_emits_security_audit_event() {
        let publisher = Arc::new(CapturingPublisher { captured: Mutex::new(vec![]) });
        let svc = CollectionService::new_with_audit(
            Box::new(MockCollectionRepo::new()),
            Box::new(NullEventPublisher),
            publisher.clone(),
        );
        svc.create("victim").unwrap();
        svc.delete("victim").unwrap();

        let captured = publisher.captured.lock().unwrap();
        assert!(
            captured.iter().any(|k| matches!(
                k,
                AuditEventKind::CollectionDeleted { collection } if collection == "victim"
            )),
            "expected CollectionDeleted, got {:?}",
            *captured
        );
    }
}
