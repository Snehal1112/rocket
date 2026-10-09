use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_collection::{
    Collection, CollectionRepository, CollectionSummary, CollectionVariable, FolderSettings,
    GraphQlRequest, GrpcRequest, Request, RequestKind, RequestScriptPhase, WebSocketRequest,
};
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
        Self {
            repo,
            events,
            audit,
        }
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

    /// True when anything sits at `path` on disk, case-insensitively. See
    /// `CollectionRepository::path_exists`.
    pub fn path_exists(&self, name: &str, path: &str) -> DomainResult<bool> {
        self.repo.path_exists(name, path)
    }

    /// Get the full request at `path`, including body/headers/auth/scripts.
    /// Used by the frontend to fetch full data on demand for a sidebar item
    /// that was loaded via `get_summaries`.
    pub fn get_request(&self, collection: &str, path: &str) -> DomainResult<Request> {
        self.repo.get_request(collection, path)
    }

    /// Get one WebSocket request by collection and relative path.
    pub fn get_websocket_request(
        &self,
        collection: &str,
        path: &str,
    ) -> DomainResult<WebSocketRequest> {
        self.repo.get_websocket_request(collection, path)
    }

    /// Save a WebSocket request and return it re-read from disk, like `save_request`.
    pub fn save_websocket_request(
        &self,
        collection: &str,
        path: &str,
        request: &WebSocketRequest,
    ) -> DomainResult<WebSocketRequest> {
        let actual_path = self
            .repo
            .save_websocket_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_websocket_request(collection, &actual_path)
    }

    /// Get the full GraphQL request at `path`.
    pub fn get_graphql_request(
        &self,
        collection: &str,
        path: &str,
    ) -> DomainResult<GraphQlRequest> {
        self.repo.get_graphql_request(collection, path)
    }

    /// Save a GraphQL request and return it as stored (the file name may differ from `path`).
    pub fn save_graphql_request(
        &self,
        collection: &str,
        path: &str,
        request: &GraphQlRequest,
    ) -> DomainResult<GraphQlRequest> {
        let actual_path = self.repo.save_graphql_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_graphql_request(collection, &actual_path)
    }

    /// Get the full gRPC request at `path`.
    pub fn get_grpc_request(&self, collection: &str, path: &str) -> DomainResult<GrpcRequest> {
        self.repo.get_grpc_request(collection, path)
    }

    /// Save a gRPC request and return it as stored (the file name may differ from `path`).
    pub fn save_grpc_request(
        &self,
        collection: &str,
        path: &str,
        request: &GrpcRequest,
    ) -> DomainResult<GrpcRequest> {
        let actual_path = self.repo.save_grpc_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_grpc_request(collection, &actual_path)
    }

    pub fn create(&self, name: &str) -> DomainResult<Collection> {
        Collection::validate_name(name)?;
        let collection = self.repo.create(name)?;
        self.events.publish(DomainEvent::CollectionCreated {
            name: name.to_string(),
        });
        Ok(collection)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.audit.publish(
            "system".into(),
            None,
            AuditEventKind::CollectionDeleted {
                collection: name.to_string(),
            },
        );
        self.events.publish(DomainEvent::CollectionDeleted {
            name: name.to_string(),
        });
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

    pub fn save_request(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<Request> {
        let actual_path = self.repo.save_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_request(collection, &actual_path)
    }

    pub fn rename_request(
        &self,
        collection: &str,
        old_path: &str,
        new_name: &str,
    ) -> DomainResult<()> {
        let kind = self.repo.request_kind(collection, old_path)?;
        if kind == RequestKind::GraphQl {
            let mut request = self.repo.get_graphql_request(collection, old_path)?;
            request.name = new_name.to_string();
            let actual_path = self
                .repo
                .save_graphql_request(collection, old_path, &request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
        if kind == RequestKind::WebSocket {
            let mut websocket = self.repo.get_websocket_request(collection, old_path)?;
            websocket.name = new_name.to_string();
            let actual_path = self
                .repo
                .save_websocket_request(collection, old_path, &websocket)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
        if kind == RequestKind::Grpc {
            let mut request = self.repo.get_grpc_request(collection, old_path)?;
            request.name = new_name.to_string();
            let actual_path = self.repo.save_grpc_request(collection, old_path, &request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
        // Only update the name field inside the JSON. The filename stays the same.
        // This produces a single Modify filesystem event.
        let mut request = self.repo.get_request(collection, old_path)?;
        request.name = new_name.to_string();
        let actual_path = self.repo.save_request(collection, old_path, &request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path,
        });
        Ok(())
    }

    pub fn update_request_docs(
        &self,
        collection: &str,
        path: &str,
        docs: Option<String>,
    ) -> DomainResult<()> {
        let kind = self.repo.request_kind(collection, path)?;
        if kind == RequestKind::GraphQl {
            let mut request = self.repo.get_graphql_request(collection, path)?;
            request.docs = docs.map(Documentation::text);
            let actual_path = self.repo.save_graphql_request(collection, path, &request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
        if kind == RequestKind::WebSocket {
            let mut websocket = self.repo.get_websocket_request(collection, path)?;
            websocket.docs = docs;
            let actual_path = self
                .repo
                .save_websocket_request(collection, path, &websocket)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
        if kind == RequestKind::Grpc {
            let mut request = self.repo.get_grpc_request(collection, path)?;
            request.docs = docs;
            let actual_path = self.repo.save_grpc_request(collection, path, &request)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
        let mut request = self.repo.get_request(collection, path)?;
        request.docs = docs.map(Documentation::text);
        let actual_path = self.repo.save_request(collection, path, &request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path,
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

    /// Creates a folder that must not exist yet. See
    /// `CollectionRepository::create_folder_exclusive`.
    pub fn create_folder_exclusive(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.create_folder_exclusive(collection, path)?;
        self.events.publish(DomainEvent::FolderCreated {
            collection: collection.to_string(),
            path: path.to_string(),
        });
        Ok(())
    }

    /// Saves a request that must not exist yet. See
    /// `CollectionRepository::create_request_exclusive`.
    pub fn create_request_exclusive(
        &self,
        collection: &str,
        path: &str,
        request: &Request,
    ) -> DomainResult<Request> {
        let actual_path = self.repo.create_request_exclusive(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_request(collection, &actual_path)
    }

    /// Moves an item without replacing the destination. See
    /// `CollectionRepository::move_item_no_replace`.
    pub fn move_item_no_replace(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        self.repo
            .move_item_no_replace(src_collection, src_path, dst_collection, dst_path)?;
        self.events.publish(DomainEvent::ItemMoved {
            src_collection: src_collection.to_string(),
            src_path: src_path.to_string(),
            dst_collection: dst_collection.to_string(),
            dst_path: dst_path.to_string(),
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

    /// Creates a starter `.js` file. The file watcher reports the change to the UI.
    pub fn create_script_file(
        &self,
        collection: &str,
        folder_path: &str,
        name: &str,
    ) -> DomainResult<String> {
        self.repo.create_script_file(collection, folder_path, name)
    }

    pub fn read_script_file(&self, collection: &str, path: &str) -> DomainResult<String> {
        self.repo.read_script_file(collection, path)
    }

    pub fn save_script_file(
        &self,
        collection: &str,
        path: &str,
        content: &str,
    ) -> DomainResult<()> {
        self.repo.save_script_file(collection, path, content)
    }

    pub fn rename_script_file(
        &self,
        collection: &str,
        path: &str,
        new_name: &str,
    ) -> DomainResult<String> {
        self.repo.rename_script_file(collection, path, new_name)
    }

    pub fn delete_script_file(&self, collection: &str, path: &str) -> DomainResult<()> {
        self.repo.delete_script_file(collection, path)
    }

    pub fn move_item(
        &self,
        src_collection: &str,
        src_path: &str,
        dst_collection: &str,
        dst_path: &str,
    ) -> DomainResult<()> {
        self.repo
            .move_item(src_collection, src_path, dst_collection, dst_path)?;
        self.events.publish(DomainEvent::ItemMoved {
            src_collection: src_collection.to_string(),
            src_path: src_path.to_string(),
            dst_collection: dst_collection.to_string(),
            dst_path: dst_path.to_string(),
        });
        Ok(())
    }

    /// Overwrites one script phase of a request, then tells listeners the
    /// request changed. The other phases and fields stay as they are.
    pub fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        self.repo
            .save_request_script(collection, request_path, phase, body)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: request_path.to_string(),
        });
        Ok(())
    }

    pub fn reorder_items(
        &self,
        collection: &str,
        folder_path: &str,
        ordered_names: &[String],
    ) -> DomainResult<()> {
        self.repo
            .reorder_items(collection, folder_path, ordered_names)?;
        self.events.publish(DomainEvent::ItemsReordered {
            collection: collection.to_string(),
            folder_path: folder_path.to_string(),
        });
        Ok(())
    }

    pub fn get_settings(&self, name: &str) -> DomainResult<rocket_collection::CollectionSettings> {
        self.repo.get_settings(name)
    }

    pub fn save_settings(
        &self,
        name: &str,
        settings: &rocket_collection::CollectionSettings,
    ) -> DomainResult<()> {
        self.repo.save_settings(name, settings)?;
        self.events.publish(DomainEvent::CollectionSettingsSaved {
            collection: name.to_string(),
        });
        Ok(())
    }

    pub fn get_folder_chain_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo
            .get_folder_chain_variables(collection, request_path)
    }

    pub fn get_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo.get_folder_variables(collection, folder_path)
    }

    pub fn save_folder_variables(
        &self,
        collection: &str,
        folder_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.repo
            .save_folder_variables(collection, folder_path, vars)?;
        self.events.publish(DomainEvent::FolderVariablesSaved {
            collection: collection.to_string(),
            folder_path: folder_path.to_string(),
        });
        Ok(())
    }

    /// Reads one folder's own settings from its folder.yml. No chain walk.
    pub fn get_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
    ) -> DomainResult<FolderSettings> {
        self.repo.get_folder_settings(collection, folder_path)
    }

    /// Writes one folder's settings, then tells listeners the folder changed.
    pub fn save_folder_settings(
        &self,
        collection: &str,
        folder_path: &str,
        settings: &FolderSettings,
    ) -> DomainResult<()> {
        self.repo
            .save_folder_settings(collection, folder_path, settings)?;
        self.events.publish(DomainEvent::FolderSettingsSaved {
            collection: collection.to_string(),
            folder_path: folder_path.to_string(),
        });
        Ok(())
    }

    pub fn get_request_variables(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<CollectionVariable>> {
        self.repo.get_request_variables(collection, request_path)
    }

    pub fn save_request_variables(
        &self,
        collection: &str,
        request_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.repo
            .save_request_variables(collection, request_path, vars)?;
        self.events.publish(DomainEvent::RequestVariablesSaved {
            collection: collection.to_string(),
            request_path: request_path.to_string(),
        });
        Ok(())
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
            Self {
                collections: Mutex::new(Vec::new()),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl CollectionRepository for MockCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            let cols = self.collections.lock().unwrap();
            Ok(cols
                .iter()
                .map(|c| {
                    CollectionSummary::new(String::new(), &c.name, "", c.request_count(), None)
                })
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
        fn save_request(
            &self,
            collection: &str,
            path: &str,
            request: &Request,
        ) -> DomainResult<String> {
            let mut requests = self.requests.lock().expect("lock");
            requests.retain(|(c, p, _)| !(c == collection && p == path));
            requests.push((collection.to_string(), path.to_string(), request.clone()));
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            unimplemented!()
        }
        fn delete_request(&self, collection: &str, path: &str) -> DomainResult<()> {
            let mut requests = self.requests.lock().expect("lock");
            let len_before = requests.len();
            requests.retain(|(c, p, _)| !(c == collection && p == path));
            if requests.len() == len_before {
                return Err(DomainError::NotFound(format!("{collection}/{path}")));
            }
            Ok(())
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            Ok(())
        }
        fn get_settings(&self, _: &str) -> DomainResult<rocket_collection::CollectionSettings> {
            Ok(rocket_collection::CollectionSettings::default())
        }
        fn save_settings(
            &self,
            _: &str,
            _: &rocket_collection::CollectionSettings,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn save_request_script(
            &self,
            _: &str,
            _: &str,
            _: rocket_collection::RequestScriptPhase,
            _: String,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    fn make_service() -> CollectionService {
        CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(NullEventPublisher),
        )
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
        assert!(
            result.is_err(),
            "getting a non-existent collection must fail"
        );
    }

    #[test]
    fn rename_validates_new_name() {
        let svc = make_service();
        svc.create("col").unwrap();
        assert!(
            svc.rename("col", "").is_err(),
            "empty new name must be rejected"
        );
        assert!(
            svc.rename("col", "bad/name").is_err(),
            "slash in new name must be rejected"
        );
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
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("my-api").expect("create");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published
                .iter()
                .any(|e| matches!(e, DomainEvent::CollectionCreated { name } if name == "my-api")),
            "expected CollectionCreated, got {:?}",
            *published
        );
    }

    #[test]
    fn delete_emits_collection_deleted() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.create("temp").expect("create");
        svc.delete("temp").expect("delete");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published
                .iter()
                .any(|e| matches!(e, DomainEvent::CollectionDeleted { name } if name == "temp")),
            "expected CollectionDeleted, got {:?}",
            *published
        );
    }

    #[test]
    fn rename_emits_collection_renamed() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
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
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");
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
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");
        svc.delete_request("my-api", "users.yml")
            .expect("delete_request");
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
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let result = svc.delete_request("my-api", "ghost.yml");
        assert!(result.is_err(), "deleting a nonexistent request must fail");
        assert!(
            publisher.events.lock().expect("lock").is_empty(),
            "no event should publish on failure"
        );
    }

    #[test]
    fn rename_request_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");
        publisher.events.lock().expect("lock").clear();
        svc.rename_request("my-api", "users.yml", "List Users")
            .expect("rename_request");
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
    fn get_request_returns_the_saved_request() {
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(NullEventPublisher),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");

        let loaded = svc.get_request("my-api", "users.yml").expect("get_request");
        assert_eq!(loaded.name, "Get Users");
        assert_eq!(loaded.url, "https://api.example.com/users");
    }

    #[test]
    fn get_request_errors_for_missing_request() {
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(NullEventPublisher),
        );
        assert!(svc.get_request("my-api", "ghost.yml").is_err());
    }

    #[test]
    fn update_request_docs_emits_request_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        let request = Request::new(
            "Get Users",
            HttpMethod::Get,
            "https://api.example.com/users",
        );
        svc.save_request("my-api", "users.yml", &request)
            .expect("save_request");
        publisher.events.lock().expect("lock").clear();
        svc.update_request_docs("my-api", "users.yml", Some("Fetches all users.".into()))
            .expect("update_request_docs");
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
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.move_item("my-api", "users.yml", "other-api", "users.yml")
            .expect("move_item");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::ItemMoved { src_collection, src_path, dst_collection, dst_path }
                    if src_collection == "my-api" && src_path == "users.yml"
                        && dst_collection == "other-api" && dst_path == "users.yml"
            )),
            "expected ItemMoved, got {:?}",
            *published
        );
    }

    #[test]
    fn create_folder_emits_folder_created() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
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
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
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
    fn reorder_items_emits_items_reordered() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.reorder_items(
            "my-api",
            "auth",
            &["login.yml".to_string(), "logout.yml".to_string()],
        )
        .expect("reorder_items");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::ItemsReordered { collection, folder_path } if collection == "my-api" && folder_path == "auth"
            )),
            "expected ItemsReordered, got {:?}", *published
        );
    }

    #[test]
    fn save_settings_emits_collection_settings_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_settings("my-api", &rocket_collection::CollectionSettings::default())
            .expect("save_settings");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::CollectionSettingsSaved { collection } if collection == "my-api"
            )),
            "expected CollectionSettingsSaved, got {:?}",
            *published
        );
    }

    #[test]
    fn save_folder_variables_emits_folder_variables_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_folder_variables("my-api", "auth", vec![])
            .expect("save_folder_variables");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::FolderVariablesSaved { collection, folder_path } if collection == "my-api" && folder_path == "auth"
            )),
            "expected FolderVariablesSaved, got {:?}", *published
        );
    }

    #[test]
    fn save_request_variables_emits_request_variables_saved() {
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(MockCollectionRepo::new()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );
        svc.save_request_variables("my-api", "users.yml", vec![])
            .expect("save_request_variables");
        let published = publisher.events.lock().expect("lock");
        assert!(
            published.iter().any(|e| matches!(
                e,
                DomainEvent::RequestVariablesSaved { collection, request_path } if collection == "my-api" && request_path == "users.yml"
            )),
            "expected RequestVariablesSaved, got {:?}", *published
        );
    }

    #[test]
    fn rename_request_keeps_a_graphql_item_graphql() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let g = GraphQlRequest::new("Old", "https://x/graphql").with_query("{ a }");
        repo.save_graphql_request("api", "q.yml", &g).expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.rename_request("api", "q.yml", "New").expect("rename");

        let back = svc.get_graphql_request("api", "q.yml").expect("get");
        assert_eq!(back.name, "New");
        assert_eq!(back.body.query, "{ a }");
    }

    #[test]
    fn delete_emits_security_audit_event() {
        let publisher = Arc::new(CapturingPublisher {
            captured: Mutex::new(vec![]),
        });
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

    #[test]
    fn rename_request_keeps_a_grpc_item_grpc() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let g = GrpcRequest::new("Old", "localhost:50051");
        repo.save_grpc_request("api", "call.yml", &g).expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.rename_request("api", "call.yml", "New").expect("rename");

        let back = svc.get_grpc_request("api", "call.yml").expect("get");
        assert_eq!(back.name, "New");
        assert_eq!(back.url, "localhost:50051");
        assert_eq!(back.uid, g.uid);
    }

    #[test]
    fn update_request_docs_keeps_a_grpc_item_grpc() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        repo.save_grpc_request("api", "call.yml", &GrpcRequest::new("A", "h:1"))
            .expect("save");

        let svc = CollectionService::new(Box::new(repo), Box::new(NullEventPublisher));
        svc.update_request_docs("api", "call.yml", Some("Calls the greeter".into()))
            .expect("docs");

        let back = svc.get_grpc_request("api", "call.yml").expect("get");
        assert_eq!(back.docs.as_deref(), Some("Calls the greeter"));
    }

    #[test]
    fn save_grpc_request_returns_the_stored_request_and_publishes_an_event() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.path().to_path_buf());
        repo.create("api").expect("create collection");
        let publisher = Arc::new(RecordingEventPublisher {
            events: Mutex::new(vec![]),
        });
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        );

        let saved = svc
            .save_grpc_request("api", "call.yml", &GrpcRequest::new("A", "h:1"))
            .expect("save");
        assert_eq!(saved.file_name.as_deref(), Some("call.yml"));
        let events = publisher.events.lock().expect("lock");
        assert!(
            matches!(events.as_slice(), [DomainEvent::RequestSaved { path, .. }] if path == "call.yml"),
            "{events:?}"
        );
    }
}

#[cfg(test)]
mod websocket_tests {
    use super::*;
    use rocket_collection::WebSocketRequest;
    use rocket_shared::events::NullEventPublisher;

    fn service(dir: &std::path::Path) -> CollectionService {
        CollectionService::new(
            Box::new(rocket_infra::FsCollectionRepo::new_standalone(
                dir.to_path_buf(),
            )),
            Box::new(NullEventPublisher),
        )
    }

    #[test]
    fn rename_request_renames_a_websocket_request_in_place() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        let saved = svc
            .save_websocket_request(
                "api",
                "chat",
                &WebSocketRequest::new("Chat", "wss://chat.example.com/ws"),
            )
            .expect("save");
        assert_eq!(saved.file_name.as_deref(), Some("chat.yml"));

        svc.rename_request("api", "chat.yml", "Team Chat")
            .expect("rename");

        let renamed = svc
            .get_websocket_request("api", "chat.yml")
            .expect("reload");
        assert_eq!(renamed.name, "Team Chat");
        assert_eq!(renamed.uid, saved.uid);
        assert_eq!(renamed.url, "wss://chat.example.com/ws");
    }

    #[test]
    fn update_request_docs_keeps_a_websocket_request_a_websocket_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        svc.save_websocket_request("api", "chat", &WebSocketRequest::new("Chat", "wss://x/ws"))
            .expect("save");

        svc.update_request_docs("api", "chat.yml", Some("# Notes".into()))
            .expect("update docs");

        let back = svc
            .get_websocket_request("api", "chat.yml")
            .expect("reload");
        assert_eq!(back.docs.as_deref(), Some("# Notes"));
        assert_eq!(back.url, "wss://x/ws");
    }

    #[test]
    fn request_variables_round_trip_for_a_websocket_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        svc.save_websocket_request("api", "chat", &WebSocketRequest::new("Chat", "wss://x/ws"))
            .expect("save");
        let var = rocket_collection::CollectionVariable {
            key: "room".into(),
            value: "general".into(),
            initial_value: "general".into(),
            enabled: true,
            secret: false,
        };

        svc.save_request_variables("api", "chat.yml", vec![var.clone()])
            .expect("save vars");

        assert_eq!(
            svc.get_request_variables("api", "chat.yml")
                .expect("get vars"),
            vec![var]
        );
        assert!(
            svc.get_websocket_request("api", "chat.yml").is_ok(),
            "still a websocket file"
        );
    }

    #[test]
    fn rename_request_still_works_for_an_http_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        let req = Request::new("Get", rocket_shared::types::HttpMethod::Get, "https://x");
        svc.save_request("api", "get", &req).expect("save");

        svc.rename_request("api", "get.yml", "Fetch")
            .expect("rename");

        assert_eq!(
            svc.get_request("api", "get.yml").expect("reload").name,
            "Fetch"
        );
    }
}

#[cfg(test)]
mod folder_settings_tests {
    use super::*;
    use rocket_shared::types::Header;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<DomainEvent>>);

    struct SharedRecorder(Arc<Recorder>);

    impl EventPublisher for SharedRecorder {
        fn publish(&self, event: DomainEvent) {
            self.0 .0.lock().expect("lock").push(event);
        }
    }

    /// A service over a temp collection "api" with one folder "auth".
    fn service(dir: &std::path::Path) -> (CollectionService, Arc<Recorder>) {
        let repo = rocket_infra::FsCollectionRepo::new_standalone(dir.to_path_buf());
        repo.create("api").expect("create collection");
        repo.create_folder("api", "auth").expect("create folder");
        let recorder = Arc::new(Recorder::default());
        let svc = CollectionService::new(
            Box::new(repo),
            Box::new(SharedRecorder(Arc::clone(&recorder))),
        );
        (svc, recorder)
    }

    fn sample() -> FolderSettings {
        FolderSettings {
            headers: vec![Header::new("X-Team", "core")],
            pre_request_script: Some("console.log('auth pre');".into()),
            docs: Some("# Auth folder".into()),
            ..FolderSettings::default()
        }
    }

    #[test]
    fn save_folder_settings_persists_and_emits_folder_settings_saved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        svc.save_folder_settings("api", "auth", &sample())
            .expect("save_folder_settings");

        assert_eq!(
            svc.get_folder_settings("api", "auth").expect("get_folder_settings"),
            sample()
        );
        let events = recorder.0.lock().expect("lock");
        assert!(
            matches!(
                events.as_slice(),
                [DomainEvent::FolderSettingsSaved { collection, folder_path }]
                    if collection == "api" && folder_path == "auth"
            ),
            "expected one FolderSettingsSaved, got {events:?}"
        );
    }

    #[test]
    fn get_folder_settings_publishes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        let settings = svc.get_folder_settings("api", "auth").expect("get");

        assert_eq!(settings, FolderSettings::default());
        assert!(recorder.0.lock().expect("lock").is_empty());
    }

    #[test]
    fn failed_folder_settings_save_publishes_no_event() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (svc, recorder) = service(dir.path());

        // The spec makes a save to a missing folder an InvalidInput error (Plan 02).
        let result = svc.save_folder_settings("api", "ghost", &sample());

        assert!(result.is_err(), "a save to a missing folder must fail");
        assert!(
            recorder.0.lock().expect("lock").is_empty(),
            "a failed save must not publish an event"
        );
    }
}
