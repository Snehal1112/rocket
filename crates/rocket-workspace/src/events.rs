//! Events the workspace context publishes.

use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

/// A workspace changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorkspaceEvent {
    WorkspaceCreated {
        id: String,
        name: String,
        path: String,
    },
    WorkspaceSwitched {
        id: String,
        name: String,
        path: String,
    },
    WorkspaceRenamed {
        id: String,
        old_name: String,
        new_name: String,
    },
    WorkspaceClosed {
        id: String,
    },
    WorkspaceDeleted {
        id: String,
    },
    WorkspacePinned {
        id: String,
    },
    WorkspaceUnpinned {
        id: String,
    },
    WorkspaceDescriptionUpdated {
        id: String,
        description: Option<String>,
    },
}

impl PublishedEvent for WorkspaceEvent {
    fn channel(&self) -> &'static str {
        match self {
            WorkspaceEvent::WorkspaceCreated { .. } => "workspace-created",
            WorkspaceEvent::WorkspaceSwitched { .. } => "workspace-switched",
            WorkspaceEvent::WorkspaceRenamed { .. } => "workspace-renamed",
            WorkspaceEvent::WorkspaceClosed { .. } => "workspace-closed",
            WorkspaceEvent::WorkspaceDeleted { .. } => "workspace-deleted",
            WorkspaceEvent::WorkspacePinned { .. } => "workspace-pinned",
            WorkspaceEvent::WorkspaceUnpinned { .. } => "workspace-unpinned",
            WorkspaceEvent::WorkspaceDescriptionUpdated { .. } => "workspace-description-updated",
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn s(v: &str) -> String {
        v.to_string()
    }

    fn one_of_each() -> Vec<WorkspaceEvent> {
        vec![
            WorkspaceEvent::WorkspaceCreated {
                id: s("w1"),
                name: s("My API"),
                path: s("/p/api"),
            },
            WorkspaceEvent::WorkspaceSwitched {
                id: s("w1"),
                name: s("My API"),
                path: s("/p/api"),
            },
            WorkspaceEvent::WorkspaceRenamed {
                id: s("w1"),
                old_name: s("Old"),
                new_name: s("New"),
            },
            WorkspaceEvent::WorkspaceClosed { id: s("w1") },
            WorkspaceEvent::WorkspaceDeleted { id: s("w1") },
            WorkspaceEvent::WorkspacePinned { id: s("w1") },
            WorkspaceEvent::WorkspaceUnpinned { id: s("w1") },
            WorkspaceEvent::WorkspaceDescriptionUpdated {
                id: s("w1"),
                description: Some(s("d")),
            },
        ]
    }

    #[test]
    fn workspace_events_json_are_unchanged() {
        let expected = vec![
            json!({"type":"workspaceCreated","id":"w1","name":"My API","path":"/p/api"}),
            json!({"type":"workspaceSwitched","id":"w1","name":"My API","path":"/p/api"}),
            json!({"type":"workspaceRenamed","id":"w1","old_name":"Old","new_name":"New"}),
            json!({"type":"workspaceClosed","id":"w1"}),
            json!({"type":"workspaceDeleted","id":"w1"}),
            json!({"type":"workspacePinned","id":"w1"}),
            json!({"type":"workspaceUnpinned","id":"w1"}),
            json!({"type":"workspaceDescriptionUpdated","id":"w1","description":"d"}),
        ];
        for (event, want) in one_of_each().iter().zip(expected) {
            assert_eq!(event.to_json().expect("serializes"), want);
        }
        let cleared = WorkspaceEvent::WorkspaceDescriptionUpdated {
            id: s("w1"),
            description: None,
        };
        assert_eq!(
            cleared.to_json().expect("serializes"),
            json!({"type":"workspaceDescriptionUpdated","id":"w1","description":null})
        );
    }

    #[test]
    fn workspace_events_keep_their_channels() {
        let channels: Vec<&str> = one_of_each().iter().map(|e| e.channel()).collect();
        assert_eq!(
            channels,
            vec![
                "workspace-created",
                "workspace-switched",
                "workspace-renamed",
                "workspace-closed",
                "workspace-deleted",
                "workspace-pinned",
                "workspace-unpinned",
                "workspace-description-updated",
            ]
        );
    }

    #[test]
    fn renamed_keeps_snake_case_field_names() {
        let event = WorkspaceEvent::WorkspaceRenamed {
            id: s("w1"),
            old_name: s("Old"),
            new_name: s("New"),
        };
        let value = event.to_json().expect("serializes");
        assert!(value.get("old_name").is_some());
        assert!(value.get("new_name").is_some());
        assert!(value.get("oldName").is_none());
    }

    #[test]
    fn workspace_events_have_no_extra_channels_or_written_collections() {
        for event in one_of_each() {
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }
}
