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

    // Workspace events
    WorkspaceCreated  { id: String, name: String, path: String },
    WorkspaceSwitched { id: String, name: String, path: String },
    WorkspaceRenamed  { id: String, old_name: String, new_name: String },
    WorkspaceClosed   { id: String },
    WorkspaceDeleted  { id: String },
    WorkspacePinned   { id: String },
    WorkspaceUnpinned { id: String },
    WorkspaceDescriptionUpdated { id: String, description: Option<String> },

    // HTTP execution events
    RequestExecuted { method: String, url: String, status: u16, duration_ms: u64 },

    // File system events
    FileChanged { path: String, event_type: FileChangeKind, collection: Option<String> },

    // History events
    HistoryCleared,

    // Git events
    GitStatusChanged { collection: String },
    GitCommit { collection: String, message: String, sha: String },
    GitPush { collection: String, remote: String },
    GitPull { collection: String, remote: String },
    BranchSwitched { collection: String, branch: String },
    BranchMerged { collection: String, branch: String },
    GitStashChanged { collection: String },
    GitConflictDetected { collection: String, files: Vec<String> },
    GitCloned { url: String, dest: String },
    GitRemoteAdded { collection: String, name: String, url: String },
    GitRemoteRemoved { collection: String, name: String },

    // Script events
    /// Emitted after all script phases complete. Carries combined console output.
    ConsoleOutput {
        request_name: String,
        /// JSON array of {level, message} objects.
        entries: Vec<serde_json::Value>,
    },
    /// Emitted after the tests phase completes.
    TestsCompleted {
        request_name: String,
        /// JSON array of {name, status, error?} objects.
        results: Vec<serde_json::Value>,
    },
    /// Emitted when a script throws an uncaught exception.
    ScriptError {
        request_name: String,
        /// "before-request" | "after-response" | "tests"
        phase: String,
        message: String,
    },
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

    #[test]
    fn workspace_created_serializes() {
        let event = DomainEvent::WorkspaceCreated {
            id: "abc-123".into(),
            name: "My API".into(),
            path: "/home/user/my-api".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("workspaceCreated") || json.contains("WorkspaceCreated"));
        assert!(json.contains("abc-123"));
        assert!(json.contains("My API"));
    }

    #[test]
    fn workspace_switched_serializes() {
        let event = DomainEvent::WorkspaceSwitched {
            id: "def-456".into(),
            name: "Staging".into(),
            path: "/home/user/staging".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("def-456"));
    }

    #[test]
    fn workspace_renamed_serializes() {
        let event = DomainEvent::WorkspaceRenamed {
            id: "abc-123".into(),
            old_name: "Old".into(),
            new_name: "New".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Old"));
        assert!(json.contains("New"));
    }

    #[test]
    fn workspace_closed_serializes() {
        let event = DomainEvent::WorkspaceClosed { id: "abc-123".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("abc-123"));
    }

    #[test]
    fn workspace_deleted_serializes() {
        let event = DomainEvent::WorkspaceDeleted { id: "abc-123".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("abc-123"));
    }

    #[test]
    fn workspace_pinned_serializes() {
        let event = DomainEvent::WorkspacePinned { id: "ws-123".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ws-123"));
    }

    #[test]
    fn workspace_unpinned_serializes() {
        let event = DomainEvent::WorkspaceUnpinned { id: "ws-123".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ws-123"));
    }

    #[test]
    fn workspace_description_updated_serializes() {
        let event = DomainEvent::WorkspaceDescriptionUpdated {
            id: "ws-123".into(),
            description: Some("New desc".into()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ws-123"));
        assert!(json.contains("New desc"));
    }
}
