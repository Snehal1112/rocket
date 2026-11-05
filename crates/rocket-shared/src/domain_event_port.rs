//! `PublishedEvent` for the global `DomainEvent`. It is removed when the
//! event enum is split into per-context enums.

use crate::events::DomainEvent;
use crate::published_event::PublishedEvent;

impl PublishedEvent for DomainEvent {
    fn channel(&self) -> &'static str {
        match self {
            // File watcher events are collection changes, so the sidebar
            // refreshes when files are created, modified or deleted.
            DomainEvent::FileChanged { .. } => "collection-changed",
            DomainEvent::RequestExecuted { .. } => "request-executed",
            DomainEvent::FileWatcherFailed { .. } => "file-watcher-failed",
            DomainEvent::RunnerStarted { .. } => "runner-started",
            DomainEvent::RunnerStepCompleted { .. } => "runner-step-completed",
            DomainEvent::RunnerFinished { .. } => "runner-finished",
            DomainEvent::WebSocketMessage { .. } => "ws:message",
            DomainEvent::WebSocketStatus { .. } => "ws:status",
            DomainEvent::GraphQlSubscriptionMessage { .. } => "graphql:subscription-message",
            DomainEvent::GraphQlSubscriptionStatus { .. } => "graphql:subscription-status",
            DomainEvent::OpenApiSyncChecked { .. } => "openapi-sync-checked",
            DomainEvent::OpenApiSyncApplied { .. } => "openapi-sync-applied",
            DomainEvent::CollectionCreated { .. }
            | DomainEvent::CollectionDeleted { .. }
            | DomainEvent::CollectionRenamed { .. }
            | DomainEvent::RequestSaved { .. }
            | DomainEvent::RequestDeleted { .. }
            | DomainEvent::ItemMoved { .. }
            | DomainEvent::FolderCreated { .. }
            | DomainEvent::FolderDeleted { .. }
            | DomainEvent::ItemsReordered { .. }
            | DomainEvent::CollectionSettingsSaved { .. }
            | DomainEvent::FolderVariablesSaved { .. }
            | DomainEvent::FolderSettingsSaved { .. }
            | DomainEvent::RequestVariablesSaved { .. }
            | DomainEvent::CollectionVariableWritten { .. } => "collection-changed",
            DomainEvent::CollectionTrustChanged { .. } => "collection-trust-changed",
            DomainEvent::ConsoleOutput { .. } => "script-console",
            DomainEvent::TestsCompleted { .. } => "script-tests",
            DomainEvent::ScriptError { .. } => "script-error",
            DomainEvent::ScriptVariableWritten { .. } => "script-variable-written",
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// Only events that the bus also sends on the `collection-changed`
    /// channel count, so a dropped watcher event is always covered by a
    /// service event.
    fn written_collections(&self) -> Vec<&str> {
        match self {
            DomainEvent::CollectionCreated { name } | DomainEvent::CollectionDeleted { name } => {
                vec![name.as_str()]
            }
            DomainEvent::CollectionRenamed { old_name, new_name } => {
                vec![old_name.as_str(), new_name.as_str()]
            }
            DomainEvent::ItemMoved {
                src_collection,
                dst_collection,
                ..
            } => vec![src_collection.as_str(), dst_collection.as_str()],
            DomainEvent::RequestSaved { collection, .. }
            | DomainEvent::RequestDeleted { collection, .. }
            | DomainEvent::FolderCreated { collection, .. }
            | DomainEvent::FolderDeleted { collection, .. }
            | DomainEvent::ItemsReordered { collection, .. }
            | DomainEvent::CollectionSettingsSaved { collection }
            | DomainEvent::FolderVariablesSaved { collection, .. }
            | DomainEvent::FolderSettingsSaved { collection, .. }
            | DomainEvent::RequestVariablesSaved { collection, .. }
            | DomainEvent::CollectionVariableWritten { collection, .. } => {
                vec![collection.as_str()]
            }
            _ => Vec::new(),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::*;
    use std::collections::BTreeSet;

    fn s() -> String {
        "x".to_string()
    }

    /// One value of every variant.
    fn one_of_each() -> Vec<DomainEvent> {
        vec![
            DomainEvent::CollectionCreated { name: s() },
            DomainEvent::CollectionDeleted { name: s() },
            DomainEvent::CollectionRenamed {
                old_name: "a".into(),
                new_name: "b".into(),
            },
            DomainEvent::RequestSaved {
                collection: s(),
                path: s(),
            },
            DomainEvent::RequestDeleted {
                collection: s(),
                path: s(),
            },
            DomainEvent::ItemMoved {
                src_collection: "a".into(),
                src_path: s(),
                dst_collection: "b".into(),
                dst_path: s(),
            },
            DomainEvent::FolderCreated {
                collection: s(),
                path: s(),
            },
            DomainEvent::FolderDeleted {
                collection: s(),
                path: s(),
            },
            DomainEvent::ItemsReordered {
                collection: s(),
                folder_path: s(),
            },
            DomainEvent::CollectionSettingsSaved { collection: s() },
            DomainEvent::CollectionTrustChanged { collection: s() },
            DomainEvent::FolderVariablesSaved {
                collection: s(),
                folder_path: s(),
            },
            DomainEvent::RequestVariablesSaved {
                collection: s(),
                request_path: s(),
            },
            DomainEvent::FolderSettingsSaved {
                collection: s(),
                folder_path: s(),
            },
            DomainEvent::RequestExecuted {
                method: s(),
                url: s(),
                status: 200,
                duration_ms: 1,
            },
            DomainEvent::RunnerStarted {
                run_id: s(),
                collection: s(),
                folder_path: None,
                total_steps: 1,
            },
            DomainEvent::RunnerStepCompleted {
                run_id: s(),
                index: 0,
                item_name: s(),
                request_path: s(),
                status: s(),
                status_code: None,
                duration_ms: 1,
                test_pass_count: 0,
                test_fail_count: 0,
                script_error: None,
                error: None,
            },
            DomainEvent::RunnerFinished {
                run_id: s(),
                stopped_reason: s(),
                step_count: 1,
                failed_count: 0,
            },
            DomainEvent::OpenApiSyncChecked {
                collection: s(),
                status: s(),
            },
            DomainEvent::OpenApiSyncApplied {
                collection: s(),
                applied: 1,
                stale: 0,
            },
            DomainEvent::WebSocketMessage {
                session_id: s(),
                direction: WebSocketDirection::In,
                kind: WebSocketPayloadKind::Text,
                data: s(),
                size: 1,
                timestamp_ms: 0,
                truncated: false,
            },
            DomainEvent::WebSocketStatus {
                session_id: s(),
                state: WebSocketSessionState::Open,
                subprotocol: None,
                code: None,
                reason: None,
            },
            DomainEvent::GraphQlSubscriptionMessage {
                session_id: s(),
                event: GraphQlSubscriptionEventKind::Next,
                data: s(),
                timestamp_ms: 0,
            },
            DomainEvent::GraphQlSubscriptionStatus {
                session_id: s(),
                state: WebSocketSessionState::Open,
                dialect: None,
                reason: None,
            },
            DomainEvent::FileChanged {
                path: s(),
                event_type: FileChangeKind::Modify,
                collection: None,
            },
            DomainEvent::FileWatcherFailed { message: s() },
            DomainEvent::ConsoleOutput {
                request_name: s(),
                entries: Vec::new(),
            },
            DomainEvent::TestsCompleted {
                request_name: s(),
                results: Vec::new(),
            },
            DomainEvent::ScriptError {
                request_name: s(),
                phase: s(),
                message: s(),
            },
            DomainEvent::ScriptVariableWritten {
                scope: s(),
                environment: None,
                collection: None,
                key: s(),
            },
            DomainEvent::CollectionVariableWritten {
                collection: s(),
                key: s(),
            },
        ]
    }

    /// The channel of each variant, keyed by its JSON  tag.
    const CHANNELS: [(&str, &str); 31] = [
        ("fileChanged", "collection-changed"),
        ("requestExecuted", "request-executed"),
        ("fileWatcherFailed", "file-watcher-failed"),
        ("runnerStarted", "runner-started"),
        ("runnerStepCompleted", "runner-step-completed"),
        ("runnerFinished", "runner-finished"),
        ("webSocketMessage", "ws:message"),
        ("webSocketStatus", "ws:status"),
        ("graphQlSubscriptionMessage", "graphql:subscription-message"),
        ("graphQlSubscriptionStatus", "graphql:subscription-status"),
        ("openApiSyncChecked", "openapi-sync-checked"),
        ("openApiSyncApplied", "openapi-sync-applied"),
        ("collectionCreated", "collection-changed"),
        ("collectionDeleted", "collection-changed"),
        ("collectionRenamed", "collection-changed"),
        ("requestSaved", "collection-changed"),
        ("requestDeleted", "collection-changed"),
        ("itemMoved", "collection-changed"),
        ("folderCreated", "collection-changed"),
        ("folderDeleted", "collection-changed"),
        ("itemsReordered", "collection-changed"),
        ("collectionSettingsSaved", "collection-changed"),
        ("folderVariablesSaved", "collection-changed"),
        ("folderSettingsSaved", "collection-changed"),
        ("requestVariablesSaved", "collection-changed"),
        ("collectionVariableWritten", "collection-changed"),
        ("collectionTrustChanged", "collection-trust-changed"),
        ("consoleOutput", "script-console"),
        ("testsCompleted", "script-tests"),
        ("scriptError", "script-error"),
        ("scriptVariableWritten", "script-variable-written"),
    ];

    fn tag(event: &DomainEvent) -> String {
        let json = event.to_json().expect("event serializes");
        json["type"].as_str().expect("type tag").to_string()
    }

    #[test]
    fn one_of_each_covers_every_variant() {
        let events = one_of_each();
        assert_eq!(events.len(), 31);
        let tags: BTreeSet<String> = events.iter().map(tag).collect();
        assert_eq!(tags.len(), 31);
    }

    #[test]
    fn every_variant_keeps_its_channel() {
        for event in one_of_each() {
            let tag = tag(&event);
            let expected = CHANNELS
                .iter()
                .find(|(name, _)| *name == tag)
                .map(|(_, channel)| *channel)
                .expect("tag is in the table");
            assert_eq!(event.channel(), expected, "channel of {tag}");
        }
    }

    #[test]
    fn to_json_equals_serde_to_value() {
        for event in one_of_each() {
            let expected = serde_json::to_value(&event).expect("event serializes");
            assert_eq!(event.to_json().expect("event serializes"), expected);
        }
    }

    #[test]
    fn written_collections_name_the_written_collections() {
        let saved = DomainEvent::RequestSaved {
            collection: "api".into(),
            path: "p".into(),
        };
        assert_eq!(saved.written_collections(), vec!["api"]);
        let moved = DomainEvent::ItemMoved {
            src_collection: "a".into(),
            src_path: "p".into(),
            dst_collection: "b".into(),
            dst_path: "q".into(),
        };
        assert_eq!(moved.written_collections(), vec!["a", "b"]);
        let renamed = DomainEvent::CollectionRenamed {
            old_name: "old".into(),
            new_name: "new".into(),
        };
        assert_eq!(renamed.written_collections(), vec!["old", "new"]);
        let changed = DomainEvent::FileChanged {
            path: "p".into(),
            event_type: FileChangeKind::Modify,
            collection: Some("api".into()),
        };
        assert!(changed.written_collections().is_empty());
    }

    #[test]
    fn domain_events_emit_on_one_channel_only() {
        for event in one_of_each() {
            assert!(event.also_emit_on().is_empty(), "{}", tag(&event));
        }
    }
}
