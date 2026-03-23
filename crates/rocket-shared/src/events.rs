use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DomainEvent {
    // Collection events
    CollectionCreated { name: String },
    CollectionDeleted { name: String },
    CollectionRenamed { old_name: String, new_name: String },

    // Request events
    RequestSaved { collection: String, path: String },
    RequestDeleted { collection: String, path: String },
    ItemMoved { src_collection: String, src_path: String, dst_collection: String, dst_path: String },

    // Environment events
    EnvironmentSaved { name: String },
    EnvironmentDeleted { name: String },

    // HTTP execution events
    RequestExecuted { method: String, url: String, status: u16, duration_ms: u64 },

    // File system events
    FileChanged { path: String, event_type: FileChangeKind, collection: Option<String> },

    // History events
    HistoryCleared,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeKind {
    Create,
    Modify,
    Remove,
}

/// Trait for publishing domain events.
/// Implemented by TauriEventBus in infrastructure layer.
pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: DomainEvent);
}

/// No-op publisher for tests and contexts where events aren't needed.
pub struct NullEventPublisher;

impl EventPublisher for NullEventPublisher {
    fn publish(&self, _event: DomainEvent) {
        // Intentionally empty.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_event_serialization() {
        let event = DomainEvent::CollectionCreated {
            name: "my-api".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("CollectionCreated") || json.contains("collectionCreated"));
        assert!(json.contains("my-api"));
    }

    #[test]
    fn event_publisher_trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn EventPublisher>) {}
    }

    #[test]
    fn null_publisher_does_not_panic() {
        let pub_ = NullEventPublisher;
        pub_.publish(DomainEvent::HistoryCleared);
    }
}
