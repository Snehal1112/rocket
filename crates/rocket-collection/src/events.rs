//! Events the collection files, the collection settings and the file watcher publish.

use rocket_shared::published_event::PublishedEvent;
use serde::{Deserialize, Serialize};

/// The kind of change the file watcher saw. Not the same type as the rocket-import FileChangeKind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeKind {
    Create,
    Modify,
    Remove,
}

/// Wire format: the `type` tag is camelCase and variant fields stay snake_case.
/// A rename breaks the frontend listeners and needs its own decision.
// Only the unit tests below read events back as JSON, so no Deserialize is derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CollectionEvent {
    // Collection events
    CollectionCreated {
        name: String,
    },
    CollectionDeleted {
        name: String,
    },
    CollectionRenamed {
        old_name: String,
        new_name: String,
    },

    // Request events
    RequestSaved {
        collection: String,
        path: String,
    },
    RequestDeleted {
        collection: String,
        path: String,
    },
    ItemMoved {
        src_collection: String,
        src_path: String,
        dst_collection: String,
        dst_path: String,
    },

    // Folder events
    FolderCreated {
        collection: String,
        path: String,
    },
    FolderDeleted {
        collection: String,
        path: String,
    },
    ItemsReordered {
        collection: String,
        folder_path: String,
    },

    // Collection settings/variable events
    CollectionSettingsSaved {
        collection: String,
    },
    /// A collection's trust grants changed on this computer (not a collection file edit).
    CollectionTrustChanged {
        collection: String,
    },
    FolderVariablesSaved {
        collection: String,
        folder_path: String,
    },
    RequestVariablesSaved {
        collection: String,
        request_path: String,
    },
    /// A folder's settings (folder.yml) were saved from the Folder Settings tab.
    FolderSettingsSaved {
        collection: String,
        folder_path: String,
    },

    // File system events
    FileChanged {
        path: String,
        event_type: FileChangeKind,
        collection: Option<String>,
    },

    /// The file watcher could not start, so the sidebar will not refresh by itself.
    /// The message holds the directory and the error text only.
    FileWatcherFailed {
        message: String,
    },

    /// Emitted when a collection-scoped variable is written (currently only via
    /// `rok.setCollectionVar` / `runtime.actions` collection-scope writes; manual
    /// collection-settings saves publish `CollectionSettingsSaved` instead, not
    /// this variant).
    CollectionVariableWritten {
        collection: String,
        key: String,
    },
}

impl PublishedEvent for CollectionEvent {
    fn channel(&self) -> &'static str {
        match self {
            CollectionEvent::CollectionTrustChanged { .. } => "collection-trust-changed",
            CollectionEvent::FileWatcherFailed { .. } => "file-watcher-failed",
            // File watcher events are collection changes, so the sidebar
            // refreshes when files are created, modified or deleted.
            CollectionEvent::FileChanged { .. }
            | CollectionEvent::CollectionCreated { .. }
            | CollectionEvent::CollectionDeleted { .. }
            | CollectionEvent::CollectionRenamed { .. }
            | CollectionEvent::RequestSaved { .. }
            | CollectionEvent::RequestDeleted { .. }
            | CollectionEvent::ItemMoved { .. }
            | CollectionEvent::FolderCreated { .. }
            | CollectionEvent::FolderDeleted { .. }
            | CollectionEvent::ItemsReordered { .. }
            | CollectionEvent::CollectionSettingsSaved { .. }
            | CollectionEvent::FolderVariablesSaved { .. }
            | CollectionEvent::FolderSettingsSaved { .. }
            | CollectionEvent::RequestVariablesSaved { .. }
            | CollectionEvent::CollectionVariableWritten { .. } => "collection-changed",
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
            CollectionEvent::CollectionCreated { name }
            | CollectionEvent::CollectionDeleted { name } => vec![name.as_str()],
            CollectionEvent::CollectionRenamed { old_name, new_name } => {
                vec![old_name.as_str(), new_name.as_str()]
            }
            CollectionEvent::ItemMoved {
                src_collection,
                dst_collection,
                ..
            } => vec![src_collection.as_str(), dst_collection.as_str()],
            CollectionEvent::RequestSaved { collection, .. }
            | CollectionEvent::RequestDeleted { collection, .. }
            | CollectionEvent::FolderCreated { collection, .. }
            | CollectionEvent::FolderDeleted { collection, .. }
            | CollectionEvent::ItemsReordered { collection, .. }
            | CollectionEvent::CollectionSettingsSaved { collection }
            | CollectionEvent::FolderVariablesSaved { collection, .. }
            | CollectionEvent::FolderSettingsSaved { collection, .. }
            | CollectionEvent::RequestVariablesSaved { collection, .. }
            | CollectionEvent::CollectionVariableWritten { collection, .. } => {
                vec![collection.as_str()]
            }
            CollectionEvent::CollectionTrustChanged { .. }
            | CollectionEvent::FileChanged { .. }
            | CollectionEvent::FileWatcherFailed { .. } => Vec::new(),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::BTreeSet;

    fn s(v: &str) -> String {
        v.to_string()
    }

    /// Every case: the event, the frozen JSON and the written collections.
    fn cases() -> Vec<(CollectionEvent, Value, Vec<&'static str>)> {
        vec![
            (
                CollectionEvent::CollectionCreated { name: s("a") },
                json!({"type": "collectionCreated", "name": "a"}),
                vec!["a"],
            ),
            (
                CollectionEvent::CollectionDeleted { name: s("a") },
                json!({"type": "collectionDeleted", "name": "a"}),
                vec!["a"],
            ),
            (
                CollectionEvent::CollectionRenamed {
                    old_name: s("a"),
                    new_name: s("b"),
                },
                json!({"type": "collectionRenamed", "old_name": "a", "new_name": "b"}),
                vec!["a", "b"],
            ),
            (
                CollectionEvent::RequestSaved {
                    collection: s("c"),
                    path: s("p"),
                },
                json!({"type": "requestSaved", "collection": "c", "path": "p"}),
                vec!["c"],
            ),
            (
                CollectionEvent::RequestDeleted {
                    collection: s("c"),
                    path: s("p"),
                },
                json!({"type": "requestDeleted", "collection": "c", "path": "p"}),
                vec!["c"],
            ),
            (
                CollectionEvent::ItemMoved {
                    src_collection: s("a"),
                    src_path: s("sp"),
                    dst_collection: s("b"),
                    dst_path: s("dp"),
                },
                json!({
                    "type": "itemMoved",
                    "src_collection": "a",
                    "src_path": "sp",
                    "dst_collection": "b",
                    "dst_path": "dp"
                }),
                vec!["a", "b"],
            ),
            (
                CollectionEvent::FolderCreated {
                    collection: s("c"),
                    path: s("p"),
                },
                json!({"type": "folderCreated", "collection": "c", "path": "p"}),
                vec!["c"],
            ),
            (
                CollectionEvent::FolderDeleted {
                    collection: s("c"),
                    path: s("p"),
                },
                json!({"type": "folderDeleted", "collection": "c", "path": "p"}),
                vec!["c"],
            ),
            (
                CollectionEvent::ItemsReordered {
                    collection: s("c"),
                    folder_path: s("f"),
                },
                json!({"type": "itemsReordered", "collection": "c", "folder_path": "f"}),
                vec!["c"],
            ),
            (
                CollectionEvent::CollectionSettingsSaved { collection: s("c") },
                json!({"type": "collectionSettingsSaved", "collection": "c"}),
                vec!["c"],
            ),
            (
                CollectionEvent::CollectionTrustChanged { collection: s("c") },
                json!({"type": "collectionTrustChanged", "collection": "c"}),
                vec![],
            ),
            (
                CollectionEvent::FolderVariablesSaved {
                    collection: s("c"),
                    folder_path: s("f"),
                },
                json!({"type": "folderVariablesSaved", "collection": "c", "folder_path": "f"}),
                vec!["c"],
            ),
            (
                CollectionEvent::RequestVariablesSaved {
                    collection: s("c"),
                    request_path: s("r"),
                },
                json!({"type": "requestVariablesSaved", "collection": "c", "request_path": "r"}),
                vec!["c"],
            ),
            (
                CollectionEvent::FolderSettingsSaved {
                    collection: s("c"),
                    folder_path: s("f"),
                },
                json!({"type": "folderSettingsSaved", "collection": "c", "folder_path": "f"}),
                vec!["c"],
            ),
            (
                CollectionEvent::FileChanged {
                    path: s("p"),
                    event_type: FileChangeKind::Modify,
                    collection: None,
                },
                json!({"type": "fileChanged", "path": "p", "event_type": "modify", "collection": null}),
                vec![],
            ),
            (
                CollectionEvent::FileChanged {
                    path: s("p"),
                    event_type: FileChangeKind::Create,
                    collection: Some(s("c")),
                },
                json!({"type": "fileChanged", "path": "p", "event_type": "create", "collection": "c"}),
                vec![],
            ),
            (
                CollectionEvent::FileWatcherFailed { message: s("m") },
                json!({"type": "fileWatcherFailed", "message": "m"}),
                vec![],
            ),
            (
                CollectionEvent::CollectionVariableWritten {
                    collection: s("c"),
                    key: s("k"),
                },
                json!({"type": "collectionVariableWritten", "collection": "c", "key": "k"}),
                vec!["c"],
            ),
        ]
    }

    #[test]
    fn collection_events_json_are_unchanged() {
        for (event, frozen, _) in cases() {
            assert_eq!(event.to_json().expect("serializes"), frozen);
        }
    }

    #[test]
    fn collection_events_keep_their_channels() {
        let expected: BTreeSet<(String, String)> = [
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
            ("fileChanged", "collection-changed"),
            ("collectionTrustChanged", "collection-trust-changed"),
            ("fileWatcherFailed", "file-watcher-failed"),
        ]
        .iter()
        .map(|(t, c)| (t.to_string(), c.to_string()))
        .collect();
        let actual: BTreeSet<(String, String)> = cases()
            .iter()
            .map(|(event, json, _)| {
                (
                    json["type"].as_str().expect("tag").to_string(),
                    event.channel().to_string(),
                )
            })
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), 17);
    }

    #[test]
    fn written_collections_match_the_old_mapping() {
        for (event, json, written) in cases() {
            assert_eq!(event.written_collections(), written, "{json}");
        }
    }

    #[test]
    fn collection_events_emit_on_one_channel_only() {
        for (event, json, _) in cases() {
            assert!(event.also_emit_on().is_empty(), "{json}");
        }
    }

    #[test]
    fn collection_variable_written_serializes() {
        let event = CollectionEvent::CollectionVariableWritten {
            collection: "my-api".into(),
            key: "BASE_URL".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("collectionVariableWritten"));
        assert!(json.contains("my-api"));
        assert!(json.contains("BASE_URL"));
    }

    #[test]
    fn file_watcher_failed_wire_shape() {
        let event = CollectionEvent::FileWatcherFailed {
            message: "boom".into(),
        };
        let value = serde_json::to_value(&event).expect("serializes");
        assert_eq!(
            value,
            json!({ "type": "fileWatcherFailed", "message": "boom" })
        );
    }

    fn wire(event: &CollectionEvent) -> String {
        serde_json::to_string(event).expect("serialize")
    }

    #[test]
    fn folder_created_wire_shape() {
        let event = CollectionEvent::FolderCreated {
            collection: "my-api".into(),
            path: "auth".into(),
        };
        assert_eq!(
            wire(&event),
            r#"{"type":"folderCreated","collection":"my-api","path":"auth"}"#
        );
    }

    #[test]
    fn folder_deleted_wire_shape() {
        let event = CollectionEvent::FolderDeleted {
            collection: "my-api".into(),
            path: "auth".into(),
        };
        assert_eq!(
            wire(&event),
            r#"{"type":"folderDeleted","collection":"my-api","path":"auth"}"#
        );
    }

    #[test]
    fn items_reordered_wire_shape() {
        let event = CollectionEvent::ItemsReordered {
            collection: "my-api".into(),
            folder_path: "auth".into(),
        };
        assert_eq!(
            wire(&event),
            r#"{"type":"itemsReordered","collection":"my-api","folder_path":"auth"}"#
        );
    }

    #[test]
    fn collection_settings_saved_wire_shape() {
        let event = CollectionEvent::CollectionSettingsSaved {
            collection: "my-api".into(),
        };
        assert_eq!(
            wire(&event),
            r#"{"type":"collectionSettingsSaved","collection":"my-api"}"#
        );
    }

    #[test]
    fn collection_trust_changed_wire_shape() {
        let event = CollectionEvent::CollectionTrustChanged {
            collection: "my-api".into(),
        };
        assert_eq!(
            wire(&event),
            r#"{"type":"collectionTrustChanged","collection":"my-api"}"#
        );
    }

    #[test]
    fn folder_variables_saved_wire_shape() {
        let event = CollectionEvent::FolderVariablesSaved {
            collection: "my-api".into(),
            folder_path: "auth".into(),
        };
        assert_eq!(
            wire(&event),
            r#"{"type":"folderVariablesSaved","collection":"my-api","folder_path":"auth"}"#
        );
    }

    #[test]
    fn request_variables_saved_wire_shape() {
        let event = CollectionEvent::RequestVariablesSaved {
            collection: "my-api".into(),
            request_path: "users.yml".into(),
        };
        assert_eq!(
            wire(&event),
            r#"{"type":"requestVariablesSaved","collection":"my-api","request_path":"users.yml"}"#
        );
    }

    #[test]
    fn folder_settings_saved_wire_shape() {
        let event = CollectionEvent::FolderSettingsSaved {
            collection: "my-api".into(),
            folder_path: "auth/login".into(),
        };
        assert_eq!(
            wire(&event),
            r#"{"type":"folderSettingsSaved","collection":"my-api","folder_path":"auth/login"}"#
        );
    }

    #[test]
    fn collection_events_keep_field_order() {
        let frozen: Vec<(CollectionEvent, &str)> = vec![
            (
                CollectionEvent::CollectionCreated { name: s("a") },
                r#"{"type":"collectionCreated","name":"a"}"#,
            ),
            (
                CollectionEvent::CollectionDeleted { name: s("a") },
                r#"{"type":"collectionDeleted","name":"a"}"#,
            ),
            (
                CollectionEvent::CollectionRenamed {
                    old_name: s("a"),
                    new_name: s("b"),
                },
                r#"{"type":"collectionRenamed","old_name":"a","new_name":"b"}"#,
            ),
            (
                CollectionEvent::RequestSaved {
                    collection: s("c"),
                    path: s("p"),
                },
                r#"{"type":"requestSaved","collection":"c","path":"p"}"#,
            ),
            (
                CollectionEvent::RequestDeleted {
                    collection: s("c"),
                    path: s("p"),
                },
                r#"{"type":"requestDeleted","collection":"c","path":"p"}"#,
            ),
            (
                CollectionEvent::ItemMoved {
                    src_collection: s("a"),
                    src_path: s("sp"),
                    dst_collection: s("b"),
                    dst_path: s("dp"),
                },
                r#"{"type":"itemMoved","src_collection":"a","src_path":"sp","dst_collection":"b","dst_path":"dp"}"#,
            ),
            (
                CollectionEvent::FileChanged {
                    path: s("p"),
                    event_type: FileChangeKind::Modify,
                    collection: None,
                },
                r#"{"type":"fileChanged","path":"p","event_type":"modify","collection":null}"#,
            ),
            (
                CollectionEvent::FileChanged {
                    path: s("p"),
                    event_type: FileChangeKind::Create,
                    collection: Some(s("c")),
                },
                r#"{"type":"fileChanged","path":"p","event_type":"create","collection":"c"}"#,
            ),
            (
                CollectionEvent::FileWatcherFailed { message: s("m") },
                r#"{"type":"fileWatcherFailed","message":"m"}"#,
            ),
            (
                CollectionEvent::CollectionVariableWritten {
                    collection: s("c"),
                    key: s("k"),
                },
                r#"{"type":"collectionVariableWritten","collection":"c","key":"k"}"#,
            ),
        ];
        for (event, expected) in frozen {
            assert_eq!(wire(&event), expected);
        }
    }

    fn sorted_keys(event: &CollectionEvent) -> Vec<String> {
        let value = serde_json::to_value(event).expect("serialize");
        let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        keys
    }

    fn assert_keys(event: &CollectionEvent, expected: &[&str]) {
        let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
        want.sort();
        assert_eq!(sorted_keys(event), want);
    }

    // Wire contract: tag is camelCase, fields stay snake_case. A rename here
    // breaks the frontend listeners and needs its own decision.
    #[test]
    fn wire_contract_collection_and_git_events() {
        assert_keys(
            &CollectionEvent::CollectionRenamed {
                old_name: "a".into(),
                new_name: "b".into(),
            },
            &["type", "old_name", "new_name"],
        );
    }

    // The envelope keys the bus adds must not collide with a top-level field.
    #[test]
    fn wire_contract_envelope_keys_are_free() {
        for (event, _, _) in cases() {
            let keys = sorted_keys(&event);
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(!keys.iter().any(|k| k == reserved));
            }
        }
    }

    #[test]
    fn file_change_kind_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&FileChangeKind::Create).expect("serializes"),
            "\"create\""
        );
        assert_eq!(
            serde_json::to_string(&FileChangeKind::Modify).expect("serializes"),
            "\"modify\""
        );
        assert_eq!(
            serde_json::to_string(&FileChangeKind::Remove).expect("serializes"),
            "\"remove\""
        );
    }

    #[test]
    fn wire_contract_watcher_events() {
        assert_keys(
            &CollectionEvent::FileWatcherFailed {
                message: "m".into(),
            },
            &["type", "message"],
        );
    }
}
