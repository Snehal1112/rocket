//! Hands out `EnvironmentService`s for one collection or for the workspace.
//!
//! The composition root builds this once. Each call asks the repository factories for a
//! repository that points at the workspace that is active at that moment, so a
//! read-modify-write stays in one workspace. The collection name is checked here, before
//! it can become part of a path.

use std::path::Path;
use std::sync::Arc;

use rocket_audit::actor::AuditActor;
use rocket_audit::publisher::SecurityAuditPublisher;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{EventPublisher, PublishedEvent};

use crate::environment_service::EnvironmentService;

/// Builds the workspace-level environment repository for the active workspace.
pub type GlobalEnvironmentRepoFactory =
    Arc<dyn Fn() -> Box<dyn EnvironmentRepository> + Send + Sync>;

/// Lets a shared publisher stand in where `EnvironmentService` wants a `Box`.
struct ForwardEvents(Arc<dyn EventPublisher>);

impl EventPublisher for ForwardEvents {
    fn publish(&self, event: &dyn PublishedEvent) {
        self.0.publish(event);
    }
}

pub struct EnvironmentServices {
    collections: Arc<dyn EnvironmentRepositoryFactory>,
    global: GlobalEnvironmentRepoFactory,
    events: Arc<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
}

impl EnvironmentServices {
    pub fn new(
        collections: Arc<dyn EnvironmentRepositoryFactory>,
        global: GlobalEnvironmentRepoFactory,
        events: Arc<dyn EventPublisher>,
        audit: Arc<dyn SecurityAuditPublisher>,
    ) -> Self {
        Self {
            collections,
            global,
            events,
            audit,
        }
    }

    /// True when `name` is safe to use as a collection folder name: not empty, no NUL
    /// byte, not absolute, and no `..` segment with either path separator.
    pub fn is_valid_collection_name(name: &str) -> bool {
        !name.is_empty()
            && !name.contains('\0')
            && !name.starts_with('/')
            && !name.starts_with('\\')
            && !name.split(['/', '\\']).any(|segment| segment == "..")
            && !Path::new(name)
                .components()
                .any(|c| c == std::path::Component::ParentDir)
    }

    /// The environments of one collection of the active workspace.
    pub fn for_collection(&self, collection: &str) -> DomainResult<EnvironmentService> {
        if !Self::is_valid_collection_name(collection) {
            return Err(DomainError::InvalidInput("invalid collection name".into()));
        }
        Ok(self.service_over(self.collections.for_collection(collection)))
    }

    /// The workspace-level (global) environments of the active workspace.
    pub fn global(&self) -> EnvironmentService {
        self.service_over((self.global)())
    }

    fn service_over(&self, repo: Box<dyn EnvironmentRepository>) -> EnvironmentService {
        EnvironmentService::new_with_audit(
            repo,
            Box::new(ForwardEvents(Arc::clone(&self.events))),
            Arc::clone(&self.audit),
        )
        .with_audit_actor(AuditActor::User)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::RecordingPublisher;
    use rocket_audit::publisher::NullSecurityAuditPublisher;
    use rocket_environment::Environment;
    use std::sync::Mutex;

    /// In-memory repository.
    #[derive(Default)]
    struct MemRepo(Mutex<Vec<Environment>>);

    impl EnvironmentRepository for MemRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.lock().expect("lock").clone())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .lock()
                .expect("lock")
                .iter()
                .find(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, env: &Environment) -> DomainResult<()> {
            self.0.lock().expect("lock").push(env.clone());
            Ok(())
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    /// Records the collection names it is asked for.
    #[derive(Default)]
    struct RecordingFactory(Mutex<Vec<String>>);

    impl EnvironmentRepositoryFactory for RecordingFactory {
        fn for_collection(&self, collection: &str) -> Box<dyn EnvironmentRepository> {
            self.0.lock().expect("lock").push(collection.to_string());
            Box::new(MemRepo::default())
        }
    }

    fn services(factory: Arc<RecordingFactory>) -> (EnvironmentServices, Arc<RecordingPublisher>) {
        let events = RecordingPublisher::new();
        let svc = EnvironmentServices::new(
            factory,
            Arc::new(|| Box::new(MemRepo::default()) as Box<dyn EnvironmentRepository>),
            events.clone(),
            Arc::new(NullSecurityAuditPublisher),
        );
        (svc, events)
    }

    #[test]
    fn rejects_unsafe_collection_names() {
        for name in ["", "..", "a/../b", "a\\..\\b", "/abs", "\\win", "a\0b"] {
            assert!(
                !EnvironmentServices::is_valid_collection_name(name),
                "{name:?} must be rejected"
            );
        }
    }

    #[test]
    fn accepts_ordinary_collection_names() {
        for name in ["My API", "a.b", "a..b", "v1 (copy)", "日本語"] {
            assert!(
                EnvironmentServices::is_valid_collection_name(name),
                "{name:?} must be accepted"
            );
        }
    }

    #[test]
    fn for_collection_rejects_a_bad_name_without_asking_the_factory() {
        let factory = Arc::new(RecordingFactory::default());
        let (svc, _) = services(Arc::clone(&factory));

        let result = svc.for_collection("../etc");

        assert!(
            matches!(result, Err(DomainError::InvalidInput(m)) if m == "invalid collection name")
        );
        assert!(factory.0.lock().expect("lock").is_empty());
    }

    #[test]
    fn for_collection_asks_the_factory_for_that_collection() {
        let factory = Arc::new(RecordingFactory::default());
        let (svc, _) = services(Arc::clone(&factory));

        svc.for_collection("Shop")
            .expect("service")
            .list()
            .expect("list");

        assert_eq!(*factory.0.lock().expect("lock"), vec!["Shop".to_string()]);
    }

    #[test]
    fn global_does_not_ask_the_collection_factory() {
        let factory = Arc::new(RecordingFactory::default());
        let (svc, _) = services(Arc::clone(&factory));

        svc.global().list().expect("list");

        assert!(factory.0.lock().expect("lock").is_empty());
    }

    #[test]
    fn save_publishes_through_the_shared_publisher() {
        let (svc, events) = services(Arc::new(RecordingFactory::default()));

        svc.for_collection("Shop")
            .expect("service")
            .save(&Environment::new("dev"))
            .expect("save");
        svc.global()
            .save(&Environment::new("shared"))
            .expect("save");

        let saved: Vec<(String, serde_json::Value)> = events.others();
        assert_eq!(
            saved,
            vec![
                (
                    "environment-changed".to_string(),
                    serde_json::json!({"type": "environmentSaved", "name": "dev"})
                ),
                (
                    "environment-changed".to_string(),
                    serde_json::json!({"type": "environmentSaved", "name": "shared"})
                ),
            ]
        );
    }
}
