use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeStatus {
    Running,
    Success,
    Failed,
    Skipped,
}

/// Why a Flow node was skipped instead of executed. Reported alongside
/// `FlowNodeStatus::Skipped` so the UI can tell a failure cascade apart
/// from a routing branch that was simply not chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowSkipReason {
    UpstreamFailed,
    BranchNotTaken,
}

/// Severity of one script console line reported by a Flow step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowLogLevel {
    Log,
    Warn,
    Error,
}

/// One script console line captured while a Flow step ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowLogEntry {
    pub level: FlowLogLevel,
    pub message: String,
}

/// One header line in a Flow debug record, already masked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDebugHeader {
    pub key: String,
    pub value: String,
}

/// The response half of a Flow debug record, already masked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDebugResponse {
    pub status: u16,
    pub status_text: String,
    pub duration_ms: u64,
    pub size_bytes: u64,
    pub headers: Vec<FlowDebugHeader>,
    pub body: String,
    /// True when `body` was cut to the exchange size limit. `size_bytes`
    /// still holds the full size.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// The request a Flow step sent and what came back, already masked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDebugRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<FlowDebugHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// True when `body` was cut to the exchange size limit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub body_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<FlowDebugResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What one Flow step saw and decided. Every field is optional on the wire,
/// so a payload from before this field existed still parses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStepTrace {
    /// One entry per data wire into the step, in the order they were read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wires: Vec<FlowWireValue>,
    /// How an If or Switch node decided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<FlowRouteEval>,
    /// The wire whose failure failed the step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_edge_id: Option<String>,
    /// True when the step's `value` was cut to the step value limit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub value_truncated: bool,
}

/// The value one wire delivered to a step, already masked and size-capped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowWireValue {
    pub edge_id: String,
    pub source_node_id: String,
    pub target_field: String,
    /// `None` for a credential wire and for a wire that failed before it had a value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// True when `value` was cut.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// True for an `auth` wire. Its credential is never recorded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub credential: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// How a routing node decided, already masked and size-capped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRouteEval {
    /// `"if"` or `"switch"`.
    pub kind: String,
    /// The coerced condition (`"true"` or `"false"`) or the Switch value.
    pub value: String,
    /// The Switch case id that matched. `None` for If and for the default exit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_case: Option<String>,
}

/// Direction of a WebSocket frame from the client's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSocketDirection {
    In,
    Out,
}

/// Wire kind of a logged frame. `Binary` data is base64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSocketPayloadKind {
    Text,
    Binary,
}

/// What a GraphQL subscription result line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphQlSubscriptionEventKind {
    Next,
    Error,
    Complete,
}

/// Lifecycle of a streaming session. `Closed` is a clean close; `Failed` is a
/// refused connect or an unclean end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSocketSessionState {
    Connecting,
    Open,
    Closed,
    Failed,
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
    /// A folder's settings (folder.yml) were saved from the Folder Settings tab.
    FolderSettingsSaved {
        collection: String,
        folder_path: String,
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
    /// Emitted immediately before a node is dispatched — once per node that
    /// is actually attempted, never for a node marked `Skipped` (those never
    /// reach dispatch).
    FlowStepStarted {
        run_id: String,
        node_id: String,
    },
    /// Emitted while a node is still running, to report progress such as a
    /// poll attempt or a callback wait. It never changes the node's status.
    FlowStepProgress {
        run_id: String,
        node_id: String,
        /// 1-based attempt number, or `None` when attempts do not apply.
        attempt: Option<u32>,
        max_attempts: Option<u32>,
        /// Short text shown on the node, such as "attempt 3/30".
        message: String,
    },
    /// Emitted after every node of a run, in topological execution order.
    FlowStepCompleted {
        run_id: String,
        node_id: String,
        status: FlowNodeStatus,
        /// `None` for a node with no HTTP response (Input/Output nodes, or
        /// a Skipped/Failed Request node that never got a response).
        status_code: Option<u16>,
        /// How long the node ran. A Request reports its response time and a
        /// repeat-until poll its total. `None` only for a node that never ran.
        duration_ms: Option<u64>,
        error: Option<String>,
        /// The node's captured output value for Output and Input nodes, or the
        /// received method (e.g. `POST`) for a succeeded Wait for callback
        /// node. `None` for every other node.
        value: Option<String>,
        /// Set only when `status` is `Skipped`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skip_reason: Option<FlowSkipReason>,
        /// The exit a succeeded If/Switch node took, e.g. `"true"` or
        /// `"case:<id>"`. `None` for every other node.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Script console output from this step, oldest first.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        logs: Vec<FlowLogEntry>,
        /// The request as sent and its response, masked. Only for Request nodes in debug mode.
        /// The nested `FlowDebugRequest` fields are camelCase inside this snake_case event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        debug_request: Option<Box<FlowDebugRequest>>,
        /// How many times a repeat-until Request node sent its request.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempts: Option<u32>,
        /// The request this step sent and its response, masked and
        /// size-capped. Set for every Request node that sent and for an
        /// accepted Wait for callback, whatever the Debug mode.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exchange: Option<Box<FlowDebugRequest>>,
        /// What the step saw on its wires and how it routed, masked and capped.
        /// The nested fields are camelCase inside this snake_case event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trace: Option<Box<FlowStepTrace>>,
    },
    /// Emitted once when a Flow run ends, for any reason.
    FlowRunFinished {
        run_id: String,
        stopped_reason: String,
        node_count: usize,
        failed_count: usize,
        /// Every skipped node, whatever the reason.
        skipped_count: usize,
        /// The subset of `skipped_count` skipped as `BranchNotTaken`.
        #[serde(default)]
        not_taken_count: usize,
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

    // gRPC session events
    /// Emitted once a streaming gRPC call has been opened.
    GrpcSessionStarted {
        session_id: String,
        /// `client-streaming`, `server-streaming` or `bidi-streaming`.
        method_type: String,
    },
    /// Emitted when the response headers arrive.
    GrpcSessionHeaders {
        session_id: String,
        headers: Vec<crate::grpc::GrpcMetadataPair>,
    },
    /// Emitted for every response message, as protobuf JSON. `index` counts from 0.
    GrpcSessionMessage {
        session_id: String,
        index: u64,
        json: String,
    },
    /// Emitted once when the call ends, for any reason. `code` is the gRPC status
    /// code, and 1 (CANCELLED) when the user cancelled.
    GrpcSessionFinished {
        session_id: String,
        code: i32,
        code_name: String,
        message: String,
        trailers: Vec<crate::grpc::GrpcMetadataPair>,
        duration_ms: u64,
    },

    // WebSocket session events
    /// One frame sent or received on a WebSocket session.
    WebSocketMessage {
        session_id: String,
        direction: WebSocketDirection,
        kind: WebSocketPayloadKind,
        /// Text as is, or base64 for binary frames.
        data: String,
        /// Payload size in bytes (before base64).
        size: usize,
        timestamp_ms: i64,
    },
    /// A WebSocket session changed state. `code` and `reason` describe a close.
    WebSocketStatus {
        session_id: String,
        state: WebSocketSessionState,
        subprotocol: Option<String>,
        code: Option<u16>,
        reason: Option<String>,
    },
    // GraphQL subscription events
    /// One result, error or completion of a GraphQL subscription.
    GraphQlSubscriptionMessage {
        session_id: String,
        event: GraphQlSubscriptionEventKind,
        /// Pretty-printed JSON. Empty for `complete`.
        data: String,
        timestamp_ms: i64,
    },
    /// A subscription changed state. `dialect` is the subprotocol the server selected.
    GraphQlSubscriptionStatus {
        session_id: String,
        state: WebSocketSessionState,
        dialect: Option<String>,
        reason: Option<String>,
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
    fn folder_settings_saved_wire_shape() {
        let event = DomainEvent::FolderSettingsSaved {
            collection: "my-api".into(),
            folder_path: "auth/login".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"folderSettingsSaved","collection":"my-api","folder_path":"auth/login"}"#
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
    fn flow_step_progress_wire_shape() {
        let event = DomainEvent::FlowStepProgress {
            run_id: "01J".into(),
            node_id: "n".into(),
            attempt: Some(3),
            max_attempts: Some(30),
            message: "attempt 3/30".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowStepProgress","run_id":"01J","node_id":"n","attempt":3,"max_attempts":30,"message":"attempt 3/30"}"#
        );
    }

    #[test]
    fn flow_step_progress_without_attempts_sends_nulls() {
        let event = DomainEvent::FlowStepProgress {
            run_id: "01J".into(),
            node_id: "n".into(),
            attempt: None,
            max_attempts: None,
            message: "waiting… 42s left".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains(r#""attempt":null,"max_attempts":null"#),
            "got {json}"
        );
    }

    #[test]
    fn flow_step_completed_serializes_logs_in_lowercase_and_omits_empty_logs() {
        let with_logs = DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "n".into(),
            status: FlowNodeStatus::Success,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: None,
            branch: None,
            debug_request: None,
            attempts: None,
            logs: vec![FlowLogEntry {
                level: FlowLogLevel::Warn,
                message: "hi".into(),
            }],
            exchange: None,
            trace: None,
        };
        let json = serde_json::to_string(&with_logs).expect("serialize");
        assert!(
            json.contains(r#""logs":[{"level":"warn","message":"hi"}]"#),
            "got {json}"
        );

        let without = DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "n".into(),
            status: FlowNodeStatus::Success,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: None,
            branch: None,
            debug_request: None,
            attempts: None,
            logs: vec![],
            exchange: None,
            trace: None,
        };
        let json = serde_json::to_string(&without).expect("serialize");
        assert!(!json.contains("logs"), "got {json}");
    }

    #[test]
    fn flow_step_completed_attempts_is_optional_and_omitted_when_none() {
        let json = r#"{"type":"flowStepCompleted","run_id":"r","node_id":"n","status":"success","status_code":200,"duration_ms":5,"error":null,"value":null}"#;
        let event: DomainEvent = serde_json::from_str(json).expect("deserialize old payload");
        match &event {
            DomainEvent::FlowStepCompleted { attempts, .. } => assert_eq!(*attempts, None),
            other => panic!("unexpected {other:?}"),
        }
        let back = serde_json::to_string(&event).expect("serialize");
        assert!(!back.contains("attempts"), "got: {back}");
    }

    #[test]
    fn flow_step_completed_carries_a_debug_request_and_omits_it_when_absent() {
        let debug = FlowDebugRequest {
            method: "GET".into(),
            url: "https://x.test/a".into(),
            headers: vec![],
            body: None,
            body_truncated: false,
            response: None,
            error: Some("boom".into()),
        };
        let event = |debug_request| DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "n".into(),
            status: FlowNodeStatus::Failed,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: None,
            branch: None,
            debug_request,
            attempts: None,
            logs: vec![],
            exchange: None,
            trace: None,
        };
        let json = serde_json::to_string(&event(Some(Box::new(debug)))).expect("serialize");
        assert!(
            json.contains(
                r#""debug_request":{"method":"GET","url":"https://x.test/a","headers":[],"error":"boom"}"#
            ),
            "got {json}"
        );
        let json = serde_json::to_string(&event(None)).expect("serialize");
        assert!(!json.contains("debug_request"), "got {json}");
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
            value: Some("bob".into()),
            skip_reason: None,
            branch: None,
            debug_request: None,
            attempts: None,
            logs: vec![],
            exchange: None,
            trace: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowStepCompleted","run_id":"01J","node_id":"node-1","status":"success","status_code":200,"duration_ms":184,"error":null,"value":"bob"}"#
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
            error: None,
            value: None,
            skip_reason: Some(FlowSkipReason::UpstreamFailed),
            branch: None,
            debug_request: None,
            attempts: None,
            logs: vec![],
            exchange: None,
            trace: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains(r#""status":"skipped""#));
        assert!(json.contains(r#""status_code":null"#));
        assert!(json.contains(r#""duration_ms":null"#));
        assert!(json.contains(r#""error":null"#));
        assert!(json.contains(r#""value":null"#));
        assert!(json.contains(r#""skip_reason":"upstream_failed""#));
        assert!(!json.contains("branch"));
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
                value,
                skip_reason,
                branch,
                logs,
                debug_request,
                attempts,
                exchange,
                trace,
            } => {
                assert_eq!(run_id, "01J");
                assert_eq!(node_id, "node-3");
                assert_eq!(status, FlowNodeStatus::Failed);
                assert_eq!(status_code, None);
                assert_eq!(duration_ms, None);
                assert_eq!(error, None);
                assert_eq!(value, None);
                assert_eq!(skip_reason, None);
                assert_eq!(branch, None);
                assert!(logs.is_empty());
                assert_eq!(debug_request, None);
                assert_eq!(attempts, None);
                assert_eq!(exchange, None);
                assert_eq!(trace, None);
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
            not_taken_count: 1,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowRunFinished","run_id":"01J","stopped_reason":"completed","node_count":5,"failed_count":1,"skipped_count":2,"not_taken_count":1}"#
        );
    }

    #[test]
    fn flow_skip_reason_wire_shapes() {
        assert_eq!(
            serde_json::to_string(&FlowSkipReason::UpstreamFailed).expect("serialize"),
            r#""upstream_failed""#
        );
        assert_eq!(
            serde_json::to_string(&FlowSkipReason::BranchNotTaken).expect("serialize"),
            r#""branch_not_taken""#
        );
        for reason in [
            FlowSkipReason::UpstreamFailed,
            FlowSkipReason::BranchNotTaken,
        ] {
            let json = serde_json::to_string(&reason).expect("serialize");
            let back: FlowSkipReason = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, reason);
        }
    }

    #[test]
    fn flow_step_completed_wire_shape_with_skip_reason() {
        let event = DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "node-2".into(),
            status: FlowNodeStatus::Skipped,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: Some(FlowSkipReason::BranchNotTaken),
            branch: None,
            debug_request: None,
            attempts: None,
            logs: vec![],
            exchange: None,
            trace: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowStepCompleted","run_id":"01J","node_id":"node-2","status":"skipped","status_code":null,"duration_ms":null,"error":null,"value":null,"skip_reason":"branch_not_taken"}"#
        );
    }

    #[test]
    fn flow_step_completed_wire_shape_with_branch() {
        let event = DomainEvent::FlowStepCompleted {
            run_id: "01J".into(),
            node_id: "if-1".into(),
            status: FlowNodeStatus::Success,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: None,
            branch: Some("case:01JCASE".into()),
            debug_request: None,
            attempts: None,
            logs: vec![],
            exchange: None,
            trace: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowStepCompleted","run_id":"01J","node_id":"if-1","status":"success","status_code":null,"duration_ms":null,"error":null,"value":null,"branch":"case:01JCASE"}"#
        );
    }

    #[test]
    fn flow_step_completed_deserializes_without_phase2_keys() {
        // A pre-Phase-2 payload carries neither `skip_reason` nor `branch`.
        let json = r#"{"type":"flowStepCompleted","run_id":"01J","node_id":"n","status":"skipped","status_code":null,"duration_ms":null,"error":"upstream node failed","value":null}"#;
        let event: DomainEvent = serde_json::from_str(json).expect("deserialize");
        match event {
            DomainEvent::FlowStepCompleted {
                skip_reason,
                branch,
                error,
                ..
            } => {
                assert_eq!(skip_reason, None);
                assert_eq!(branch, None);
                assert_eq!(error.as_deref(), Some("upstream node failed"));
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn flow_run_finished_deserializes_without_not_taken_count() {
        let json = r#"{"type":"flowRunFinished","run_id":"01J","stopped_reason":"completed","node_count":3,"failed_count":1,"skipped_count":1}"#;
        let event: DomainEvent = serde_json::from_str(json).expect("deserialize");
        match event {
            DomainEvent::FlowRunFinished {
                not_taken_count,
                skipped_count,
                ..
            } => {
                assert_eq!(not_taken_count, 0);
                assert_eq!(skipped_count, 1);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn acp_session_started_wire_shape() {
        let event = DomainEvent::AcpSessionStarted {
            session_id: "sess-1".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpSessionStarted","session_id":"sess-1"}"#
        );
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

    #[test]
    fn graphql_subscription_events_serialize_like_the_websocket_ones() {
        let message = DomainEvent::GraphQlSubscriptionMessage {
            session_id: "s1".into(),
            event: GraphQlSubscriptionEventKind::Next,
            data: "{}".into(),
            timestamp_ms: 7,
        };
        let json = serde_json::to_value(&message).expect("serialize");
        assert_eq!(json["type"], "graphQlSubscriptionMessage");
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["event"], "next");
        assert_eq!(json["timestamp_ms"], 7);

        let status = DomainEvent::GraphQlSubscriptionStatus {
            session_id: "s1".into(),
            state: WebSocketSessionState::Open,
            dialect: Some("graphql-transport-ws".into()),
            reason: None,
        };
        let json = serde_json::to_value(&status).expect("serialize");
        assert_eq!(json["type"], "graphQlSubscriptionStatus");
        assert_eq!(json["state"], "open");
        assert_eq!(json["dialect"], "graphql-transport-ws");
        assert!(json["reason"].is_null());
    }

    #[test]
    fn websocket_events_serialize_with_snake_case_fields_and_lowercase_enums() {
        let message = DomainEvent::WebSocketMessage {
            session_id: "s1".into(),
            direction: WebSocketDirection::In,
            kind: WebSocketPayloadKind::Binary,
            data: "AQID".into(),
            size: 3,
            timestamp_ms: 42,
        };
        let json = serde_json::to_value(&message).expect("serialize");
        assert_eq!(json["type"], "webSocketMessage");
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["direction"], "in");
        assert_eq!(json["kind"], "binary");
        assert_eq!(json["timestamp_ms"], 42);

        let status = DomainEvent::WebSocketStatus {
            session_id: "s1".into(),
            state: WebSocketSessionState::Failed,
            subprotocol: None,
            code: Some(4001),
            reason: Some("bye".into()),
        };
        let json = serde_json::to_value(&status).expect("serialize");
        assert_eq!(json["type"], "webSocketStatus");
        assert_eq!(json["state"], "failed");
        assert_eq!(json["code"], 4001);
        assert!(json["subprotocol"].is_null());
    }

    #[test]
    fn flow_step_completed_without_exchange_still_deserializes() {
        let json = r#"{"type":"flowStepCompleted","run_id":"r","node_id":"n","status":"success","status_code":200,"duration_ms":5,"error":null,"value":null}"#;
        let event: DomainEvent = serde_json::from_str(json).expect("old payload");
        match event {
            DomainEvent::FlowStepCompleted { exchange, .. } => assert!(exchange.is_none()),
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn a_response_that_was_not_cut_serializes_without_truncated() {
        let response = FlowDebugResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 1,
            size_bytes: 2,
            headers: Vec::new(),
            body: "{}".into(),
            truncated: false,
        };
        let json = serde_json::to_string(&response).expect("serialize");
        assert!(!json.contains("truncated"), "{json}");
        let old: FlowDebugResponse = serde_json::from_str(
            r#"{"status":200,"statusText":"OK","durationMs":1,"sizeBytes":2,"headers":[],"body":"{}"}"#,
        )
        .expect("old record");
        assert!(!old.truncated);
    }

    #[test]
    fn flow_step_completed_carries_the_exchange_in_camel_case() {
        let event = DomainEvent::FlowStepCompleted {
            run_id: "r".into(),
            node_id: "n".into(),
            status: FlowNodeStatus::Success,
            status_code: Some(200),
            duration_ms: Some(5),
            error: None,
            value: None,
            skip_reason: None,
            branch: None,
            logs: Vec::new(),
            debug_request: None,
            attempts: None,
            trace: None,
            exchange: Some(Box::new(FlowDebugRequest {
                method: "GET".into(),
                url: "https://x.test".into(),
                headers: Vec::new(),
                body: Some("sent".into()),
                body_truncated: true,
                response: Some(FlowDebugResponse {
                    status: 200,
                    status_text: "OK".into(),
                    duration_ms: 5,
                    size_bytes: 300_000,
                    headers: Vec::new(),
                    body: "x".into(),
                    truncated: true,
                }),
                error: None,
            })),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains(r#""exchange":{"method":"GET""#), "{json}");
        assert!(json.contains(r#""sizeBytes":300000"#), "{json}");
        assert!(json.contains(r#""truncated":true"#), "{json}");
        assert!(json.contains(r#""bodyTruncated":true"#), "{json}");
    }

    #[test]
    fn a_request_body_flag_is_omitted_when_false_and_defaults_when_missing() {
        let old: FlowDebugRequest = serde_json::from_str(
            r#"{"method":"GET","url":"https://x.test","headers":[],"body":"a"}"#,
        )
        .expect("old record");
        assert!(!old.body_truncated);
        let json = serde_json::to_string(&old).expect("serialize");
        assert!(!json.contains("bodyTruncated"), "{json}");
    }

    #[test]
    fn an_empty_flow_step_trace_serializes_to_an_empty_object() {
        let json = serde_json::to_string(&FlowStepTrace::default()).expect("serialize");
        assert_eq!(json, "{}");
        let back: FlowStepTrace = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(back, FlowStepTrace::default());
    }

    #[test]
    fn a_flow_wire_value_is_camel_case_and_omits_false_flags() {
        let wire = FlowWireValue {
            edge_id: "e1".into(),
            source_node_id: "in".into(),
            target_field: "headers[X-Id].value".into(),
            value: Some("42".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&wire).expect("serialize");
        assert_eq!(
            json,
            r#"{"edgeId":"e1","sourceNodeId":"in","targetField":"headers[X-Id].value","value":"42"}"#
        );
        let credential = FlowWireValue {
            credential: true,
            ..wire.clone()
        };
        let json = serde_json::to_string(&FlowWireValue {
            value: None,
            ..credential
        })
        .expect("serialize");
        assert!(json.contains(r#""credential":true"#), "{json}");
        // The target field itself ends in ".value", so match the key.
        assert!(!json.contains(r#""value":"#), "{json}");
        assert!(!json.contains("truncated"), "{json}");
    }

    #[test]
    fn flow_step_completed_carries_a_trace_and_omits_it_when_absent() {
        let trace = FlowStepTrace {
            route: Some(FlowRouteEval {
                kind: "switch".into(),
                value: "admin".into(),
                matched_case: Some("c1".into()),
            }),
            failed_edge_id: Some("e2".into()),
            ..Default::default()
        };
        let event = |trace| DomainEvent::FlowStepCompleted {
            run_id: "r".into(),
            node_id: "n".into(),
            status: FlowNodeStatus::Success,
            status_code: None,
            duration_ms: Some(3),
            error: None,
            value: None,
            skip_reason: None,
            branch: None,
            logs: Vec::new(),
            debug_request: None,
            attempts: None,
            exchange: None,
            trace,
        };
        let json = serde_json::to_string(&event(Some(Box::new(trace)))).expect("serialize");
        assert!(
            json.contains(
                r#""trace":{"route":{"kind":"switch","value":"admin","matchedCase":"c1"},"failedEdgeId":"e2"}"#
            ),
            "{json}"
        );
        let json = serde_json::to_string(&event(None)).expect("serialize");
        assert!(!json.contains("trace"), "{json}");
    }

    #[test]
    fn flow_step_completed_without_trace_still_deserializes() {
        let json = r#"{"type":"flowStepCompleted","run_id":"r","node_id":"n","status":"success","status_code":200,"duration_ms":5,"error":null,"value":null}"#;
        let event: DomainEvent = serde_json::from_str(json).expect("old payload");
        match event {
            DomainEvent::FlowStepCompleted { trace, .. } => assert!(trace.is_none()),
            other => panic!("unexpected event {other:?}"),
        }
    }
}
