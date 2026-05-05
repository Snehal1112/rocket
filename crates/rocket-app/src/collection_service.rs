use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{Collection, CollectionRepository, CollectionSummary, CollectionVariable, Request};
use rocket_shared::description::Documentation;
use rocket_shared::error::DomainResult;
use std::sync::Arc;

pub struct CollectionService {
    repo: Box<dyn CollectionRepository>,
    audit: Arc<dyn SecurityAuditPublisher>,
}

impl CollectionService {
    pub fn new(repo: Box<dyn CollectionRepository>) -> Self {
        Self {
            repo,
            audit: Arc::new(NullSecurityAuditPublisher),
        }
    }

    pub fn new_with_audit(
        repo: Box<dyn CollectionRepository>,
        audit: Arc<dyn SecurityAuditPublisher>,
    ) -> Self {
        Self { repo, audit }
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
        self.repo.create(name)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.audit.publish(
            "system".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: name.to_string() },
        );
        Ok(())
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> DomainResult<()> {
        Collection::validate_name(new_name)?;
        self.repo.rename(old_name, new_name)
    }

    pub fn save_request(&self, collection: &str, path: &str, request: &Request) -> DomainResult<Request> {
        let actual_path = self.repo.save_request(collection, path, request)?;
        self.repo.get_request(collection, &actual_path)
    }

    pub fn rename_request(&self, collection: &str, old_path: &str, new_name: &str) -> DomainResult<()> {
        // Only update the name field inside the JSON. The filename stays the same.
        // This produces a single Modify filesystem event.
        let mut request = self.repo.get_request(collection, old_path)?;
        request.name = new_name.to_string();
        self.repo.save_request(collection, old_path, &request)?;
        Ok(())
    }

    pub fn update_request_docs(&self, collection: &str, path: &str, docs: Option<String>) -> DomainResult<()> {
        let mut request = self.repo.get_request(collection, path)?;
        request.docs = docs.map(Documentation::text);
        self.repo.save_request(collection, path, &request)?;
        Ok(())
    }

    pub fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_request(collection, path)
    }

    pub fn create_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.create_folder(collection, path)
    }

    pub fn delete_folder(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_folder(collection, path)
    }

    pub fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        self.repo.move_item(src_collection, src_path, dst_collection, dst_path)
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
    use std::sync::Mutex;

    struct MockCollectionRepo {
        collections: Mutex<Vec<Collection>>,
    }

    impl MockCollectionRepo {
        fn new() -> Self {
            Self { collections: Mutex::new(Vec::new()) }
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

        fn get_request(&self, _: &str, _: &str) -> DomainResult<Request> { unimplemented!() }
        fn save_request(&self, _: &str, path: &str, _: &Request) -> DomainResult<String> { Ok(path.to_string()) }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> { unimplemented!() }
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
        CollectionService::new(Box::new(MockCollectionRepo::new()))
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

    #[test]
    fn delete_emits_security_audit_event() {
        let publisher = Arc::new(CapturingPublisher { captured: Mutex::new(vec![]) });
        let svc = CollectionService::new_with_audit(
            Box::new(MockCollectionRepo::new()),
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
