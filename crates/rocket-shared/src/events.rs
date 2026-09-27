use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeStatus {
    Running,
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DomainEvent {
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
    FolderVariablesSaved {
        collection: String,
        folder_path: String,
    },
    RequestVariablesSaved {
        collection: String,
        request_path: String,
    },

    // Environment events
    EnvironmentSaved {
        name: String,
    },
    EnvironmentDeleted {
        name: String,
    },

    // Workspace events
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

    // HTTP execution events
    RequestExecuted {
        method: String,
        url: String,
        status: u16,
        duration_ms: u64,
    },

    // Collection Runner events
    /// Emitted once when a run starts, before its first step.
    /// `total_steps` is the run set's length; a script jumping with
    /// `setNextRequest` can make the number of executed steps differ from it.
    RunnerStarted {
        run_id: String,
        collection: String,
        folder_path: Option<String>,
        total_steps: usize,
    },
    /// Emitted after every step of a run, in execution order.
    /// `status` is `"completed"`, `"skipped"`, or `"error"`.
    RunnerStepCompleted {
        run_id: String,
        /// Position in the emitted step stream, starting at 0.
        index: usize,
        item_name: String,
        request_path: String,
        status: String,
        /// `None` for a skipped or errored step.
        status_code: Option<u16>,
        duration_ms: u64,
        test_pass_count: usize,
        test_fail_count: usize,
        /// Uncaught script exception message, if any.
        script_error: Option<String>,
        /// Transport or sequencing error, if any.
        error: Option<String>,
    },
    /// Emitted once when a run ends, for any reason.
    /// `stopped_reason` is `"completed"`, `"stoppedByScript"`,
    /// `"stoppedOnFailure"`, `"unknownNextRequest"`, `"cancelled"`, or
    /// `"stepLimitReached"`.
    RunnerFinished {
        run_id: String,
        stopped_reason: String,
        step_count: usize,
        failed_count: usize,
    },

    // Flow events
    /// Emitted once when a Flow run starts, before its first node executes.
    FlowRunStarted {
        run_id: String,
        flow_name: String,
        collection: String,
        total_nodes: usize,
    },
    /// Emitted after every node of a run, in topological execution order.
    FlowStepCompleted {
        run_id: String,
        node_id: String,
        status: FlowNodeStatus,
        /// `None` for a node with no HTTP response (Input/Output nodes, or
        /// a Skipped/Failed Request node that never got a response).
        status_code: Option<u16>,
        /// `None` for a node that never executed (Skipped) or has no
        /// meaningful duration (Input/Output nodes).
        duration_ms: Option<u64>,
        error: Option<String>,
    },
    /// Emitted once when a Flow run ends, for any reason.
    FlowRunFinished {
        run_id: String,
        stopped_reason: String,
        node_count: usize,
        failed_count: usize,
        skipped_count: usize,
    },

    // ACP session events
    /// Emitted once a spawned agent process completes its ACP handshake.
    AcpSessionStarted {
        session_id: String,
    },
    /// Emitted per streamed text chunk while a prompt is being answered.
    AcpSessionChunk {
        session_id: String,
        text: String,
    },
    /// Emitted once a prompt's turn ends. `stop_reason` is the raw ACP
    /// `stopReason` string (`end_turn`, `max_tokens`, `max_turn_requests`,
    /// `refusal`, `cancelled`, or an agent-specific custom reason) — kept as
    /// a plain string rather than a fixed enum because ACP's own
    /// `StopReason` type is `#[non_exhaustive]`.
    AcpSessionFinished {
        session_id: String,
        stop_reason: String,
    },
    /// Emitted when a session ends abnormally: the agent process crashed,
    /// a protocol-level error occurred, or `send_prompt` timed out.
    AcpSessionFailed {
        session_id: String,
        error: String,
    },

    // File system events
    FileChanged {
        path: String,
        event_type: FileChangeKind,
        collection: Option<String>,
    },

    // History events
    HistoryCleared,

    // Git events
    GitStatusChanged {
        collection: String,
    },
    GitCommit {
        collection: String,
        message: String,
        sha: String,
    },
    GitPush {
        collection: String,
        remote: String,
    },
    GitPull {
        collection: String,
        remote: String,
    },
    BranchSwitched {
        collection: String,
        branch: String,
    },
    BranchMerged {
        collection: String,
        branch: String,
    },
    GitStashChanged {
        collection: String,
    },
    GitConflictDetected {
        collection: String,
        files: Vec<String>,
    },
    GitCloned {
        url: String,
        dest: String,
    },
    GitRemoteAdded {
        collection: String,
        name: String,
        url: String,
    },
    GitRemoteRemoved {
        collection: String,
        name: String,
    },

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
    /// Emitted when a script (`rok.setEnvVar`/`setGlobalEnvVar`/`setCollectionVar`)
    /// or a declarative `runtime.actions` set-variable write persists a variable.
    /// Distinct from `EnvironmentSaved`/`CollectionVariableWritten`, which fire
    /// alongside it — this variant exists so the frontend can distinguish an
    /// automated script write from a manual user edit without correlating
    /// timestamps.
    ScriptVariableWritten {
        /// "environment" | "collection"
        scope: String,
        environment: Option<String>,
        collection: Option<String>,
        key: String,
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
        let event = DomainEvent::WorkspaceClosed {
            id: "abc-123".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("abc-123"));
    }

    #[test]
    fn workspace_deleted_serializes() {
        let event = DomainEvent::WorkspaceDeleted {
            id: "abc-123".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("abc-123"));
    }

    #[test]
    fn workspace_pinned_serializes() {
        let event = DomainEvent::WorkspacePinned {
            id: "ws-123".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ws-123"));
    }

    #[test]
    fn workspace_unpinned_serializes() {
        let event = DomainEvent::WorkspaceUnpinned {
            id: "ws-123".into(),
        };
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

    #[test]
    fn script_variable_written_serializes() {
        let event = DomainEvent::ScriptVariableWritten {
            scope: "environment".into(),
            environment: Some("staging".into()),
            collection: None,
            key: "API_KEY".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("scriptVariableWritten") || json.contains("ScriptVariableWritten"));
        assert!(json.contains("staging"));
        assert!(json.contains("API_KEY"));
    }

    #[test]
    fn collection_variable_written_serializes() {
        let event = DomainEvent::CollectionVariableWritten {
            collection: "my-api".into(),
            key: "BASE_URL".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains("collectionVariableWritten")
                || json.contains("CollectionVariableWritten")
        );
        assert!(json.contains("my-api"));
        assert!(json.contains("BASE_URL"));
    }

    #[test]
    fn runner_started_wire_shape() {
        let event = DomainEvent::RunnerStarted {
            run_id: "01J".into(),
            collection: "my-api".into(),
            folder_path: Some("auth".into()),
            total_steps: 3,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"runnerStarted","run_id":"01J","collection":"my-api","folder_path":"auth","total_steps":3}"#
        );
    }

    #[test]
    fn runner_step_completed_wire_shape() {
        let event = DomainEvent::RunnerStepCompleted {
            run_id: "01J".into(),
            index: 0,
            item_name: "Login".into(),
            request_path: "auth/login.yml".into(),
            status: "completed".into(),
            status_code: Some(200),
            duration_ms: 12,
            test_pass_count: 2,
            test_fail_count: 0,
            script_error: None,
            error: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains(r#""type":"runnerStepCompleted""#));
        // Struct-variant fields stay snake_case — the enum's rename_all only
        // renames variants. The frontend contract depends on this.
        assert!(json.contains(r#""run_id":"01J""#));
        assert!(json.contains(r#""item_name":"Login""#));
        assert!(json.contains(r#""test_pass_count":2"#));
        assert!(json.contains(r#""status_code":200"#));
    }

    #[test]
    fn runner_finished_wire_shape() {
        let event = DomainEvent::RunnerFinished {
            run_id: "01J".into(),
            stopped_reason: "completed".into(),
            step_count: 3,
            failed_count: 1,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"runnerFinished","run_id":"01J","stopped_reason":"completed","step_count":3,"failed_count":1}"#
        );
    }

    #[test]
    fn folder_created_wire_shape() {
        let event = DomainEvent::FolderCreated {
            collection: "my-api".into(),
            path: "auth".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"folderCreated","collection":"my-api","path":"auth"}"#
        );
    }

    #[test]
    fn folder_deleted_wire_shape() {
        let event = DomainEvent::FolderDeleted {
            collection: "my-api".into(),
            path: "auth".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"folderDeleted","collection":"my-api","path":"auth"}"#
        );
    }

    #[test]
    fn items_reordered_wire_shape() {
        let event = DomainEvent::ItemsReordered {
            collection: "my-api".into(),
            folder_path: "auth".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"itemsReordered","collection":"my-api","folder_path":"auth"}"#
        );
    }

    #[test]
    fn collection_settings_saved_wire_shape() {
        let event = DomainEvent::CollectionSettingsSaved {
            collection: "my-api".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"collectionSettingsSaved","collection":"my-api"}"#
        );
    }

    #[test]
    fn folder_variables_saved_wire_shape() {
        let event = DomainEvent::FolderVariablesSaved {
            collection: "my-api".into(),
            folder_path: "auth".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"folderVariablesSaved","collection":"my-api","folder_path":"auth"}"#
        );
    }

    #[test]
    fn request_variables_saved_wire_shape() {
        let event = DomainEvent::RequestVariablesSaved {
            collection: "my-api".into(),
            request_path: "users.yml".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"requestVariablesSaved","collection":"my-api","request_path":"users.yml"}"#
        );
    }

    #[test]
    fn flow_node_status_wire_shapes() {
        assert_eq!(
            serde_json::to_string(&FlowNodeStatus::Running).expect("serialize"),
            r#""running""#
        );
        assert_eq!(
            serde_json::to_string(&FlowNodeStatus::Success).expect("serialize"),
            r#""success""#
        );
        assert_eq!(
            serde_json::to_string(&FlowNodeStatus::Failed).expect("serialize"),
            r#""failed""#
        );
        assert_eq!(
            serde_json::to_string(&FlowNodeStatus::Skipped).expect("serialize"),
            r#""skipped""#
        );
    }

    #[test]
    fn flow_run_started_wire_shape() {
        let event = DomainEvent::FlowRunStarted {
            run_id: "01J".into(),
            flow_name: "Login Flow".into(),
            collection: "acme".into(),
            total_nodes: 3,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"Login Flow","collection":"acme","total_nodes":3}"#
        );
    }

    #[test]
    fn flow_step_completed_wire_shape_with_all_fields_present() {
        let event = DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "node-1".into(),
            status: FlowNodeStatus::Success,
            status_code: Some(200),
            duration_ms: Some(184),
            error: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowStepCompleted","run_id":"01J","node_id":"node-1","status":"success","status_code":200,"duration_ms":184,"error":null}"#
        );
    }

    #[test]
    fn flow_step_completed_serializes_with_optional_fields_absent() {
        let event = DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "node-2".into(),
            status: FlowNodeStatus::Skipped,
            status_code: None,
            duration_ms: None,
            error: Some("upstream failed".into()),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains(r#""status":"skipped""#));
        assert!(json.contains(r#""status_code":null"#));
        assert!(json.contains(r#""duration_ms":null"#));
        assert!(json.contains(r#""error":"upstream failed""#));
    }

    #[test]
    fn flow_step_completed_deserializes_with_optional_keys_missing() {
        // Optional fields must be truly optional on the wire, not just nullable.
        let json =
            r#"{"type":"flowStepCompleted","run_id":"01J","node_id":"node-3","status":"failed"}"#;
        let event: DomainEvent = serde_json::from_str(json).expect("deserialize");
        match event {
            DomainEvent::FlowStepCompleted {
                run_id,
                node_id,
                status,
                status_code,
                duration_ms,
                error,
            } => {
                assert_eq!(run_id, "01J");
                assert_eq!(node_id, "node-3");
                assert_eq!(status, FlowNodeStatus::Failed);
                assert_eq!(status_code, None);
                assert_eq!(duration_ms, None);
                assert_eq!(error, None);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn flow_node_status_round_trips() {
        for status in [
            FlowNodeStatus::Running,
            FlowNodeStatus::Success,
            FlowNodeStatus::Failed,
            FlowNodeStatus::Skipped,
        ] {
            let json = serde_json::to_string(&status).expect("serialize");
            let back: FlowNodeStatus = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, status);
        }
    }

    #[test]
    fn flow_run_finished_wire_shape_tracks_failed_and_skipped_separately() {
        let event = DomainEvent::FlowRunFinished {
            run_id: "01J".into(),
            stopped_reason: "completed".into(),
            node_count: 5,
            failed_count: 1,
            skipped_count: 2,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowRunFinished","run_id":"01J","stopped_reason":"completed","node_count":5,"failed_count":1,"skipped_count":2}"#
        );
    }

    #[test]
    fn acp_session_started_wire_shape() {
        let event = DomainEvent::AcpSessionStarted {
            session_id: "sess-1".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"type":"acpSessionStarted","session_id":"sess-1"}"#);
    }

    #[test]
    fn acp_session_chunk_wire_shape() {
        let event = DomainEvent::AcpSessionChunk {
            session_id: "sess-1".into(),
            text: "Hello, ".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpSessionChunk","session_id":"sess-1","text":"Hello, "}"#
        );
    }

    #[test]
    fn acp_session_finished_wire_shape_carries_arbitrary_stop_reason_unchanged() {
        // stop_reason is a plain String (not a closed Rust enum) precisely
        // because ACP's own StopReason is #[non_exhaustive] — an
        // agent-specific custom reason (the spec's example: an underscore-
        // prefixed value) must round-trip unchanged, not just a known value
        // like "end_turn".
        let event = DomainEvent::AcpSessionFinished {
            session_id: "sess-1".into(),
            stop_reason: "_custom_agent_reason".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpSessionFinished","session_id":"sess-1","stop_reason":"_custom_agent_reason"}"#
        );
    }

    #[test]
    fn acp_session_failed_wire_shape() {
        let event = DomainEvent::AcpSessionFailed {
            session_id: "sess-1".into(),
            error: "agent process exited unexpectedly".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpSessionFailed","session_id":"sess-1","error":"agent process exited unexpectedly"}"#
        );
    }
}
