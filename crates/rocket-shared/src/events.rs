use serde::{Deserialize, Serialize};

use crate::acp::{ConfigOption, PermissionOption};

pub use crate::published_event::PublishedEvent;

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

/// Which nodes a partial Flow run executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowPartialMode {
    /// "Run this node": the start node and the Input and Auth nodes it reads.
    Node,
    /// "Run from here": the start node and every node downstream of it.
    FromHere,
}

/// Describes a partial run on `FlowRunStarted` and on the run summary.
/// Nested keys are camelCase, like `FlowDebugRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowPartialRunInfo {
    /// The run whose cached results feed this one.
    pub base_run_id: String,
    pub start_node_id: String,
    pub mode: FlowPartialMode,
    /// Every node this run executes, in execution order.
    pub node_ids: Vec<String>,
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
    /// How a repeat-until poll went.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll: Option<FlowPollDetail>,
    /// How a callback wait went.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<FlowWaitDetail>,
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
    /// FNV-1a hash of the edge's script text when the wire was read. It holds
    /// no value, so it is safe to keep. The UI compares it with the script now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression_hash: Option<String>,
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

/// A call a Wait for callback node turned down, already masked. The body is
/// masked first and cut second.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRejectedCall {
    pub method: String,
    /// The path and query. The token-bearing path is shown as `/cb/…`.
    pub url: String,
    pub headers: Vec<FlowDebugHeader>,
    pub body: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub body_truncated: bool,
    /// Why the call was turned down.
    pub reason: String,
}

/// How a callback wait went. Kept in the step trace, also on a timeout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowWaitDetail {
    pub ignored: u32,
    pub timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rejected: Option<FlowRejectedCall>,
}

/// One Wait for callback node of a run, without its URL. The URL holds a
/// bearer token, so it never goes into an event. The UI asks for it with
/// `get_flow_run_callbacks` while the run is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowCallbackInfo {
    pub node_id: String,
    pub name: String,
}

/// Structured progress of a node that is still running. Every field is
/// optional, so each node kind sends only what applies to it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowLiveProgress {
    /// Status code of the last poll response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status_code: Option<u16>,
    /// Verdict of the last poll condition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition_met: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    /// Time left before the node gives up. The UI counts down from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_ms: Option<u64>,
    /// Calls a Wait for callback node turned down so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignored: Option<u32>,
    /// The call just turned down, with its body cut at 2 KB. Only the
    /// turn-down event carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rejected: Option<FlowRejectedCall>,
}

/// How a repeat-until poll went. Kept in the step trace, also on failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowPollDetail {
    pub attempts: u32,
    pub max_attempts: u32,
    /// `None` until an attempt got a response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status_code: Option<u16>,
    /// `None` when no verdict was reached, as after a condition script error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition_met: Option<bool>,
    pub elapsed_ms: u64,
    pub timeout_ms: u64,
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

/// One flow that could not be rewritten after a request or folder move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRewriteFailure {
    pub flow: String,
    pub message: String,
}

/// Every event the backend publishes.
///
/// Wire format, pinned by the `wire_contract_*` tests below:
/// - The `type` tag is camelCase.
/// - Variant fields are deliberately snake_case.
/// - Nested helper DTOs (`FlowCallbackInfo` and similar) are camelCase.
///
/// Renaming any of these is a breaking change for the frontend listeners
/// and needs its own decision. The bus adds `seq` and `tsMs` at emit time.
// Only the back-compat unit tests below read events back, so Deserialize is test-only.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
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
    /// Emitted per streamed reasoning chunk while a prompt is being answered.
    AcpThoughtChunk {
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
    /// Emitted by `McpToolService` after a successful call of a read tool or
    /// `run_request`, so agent-driven actions are auditable and
    /// distinguishable from manual ones. Refused or failed calls publish
    /// nothing. `propose_changes` and `list_proposals` do not publish it;
    /// proposals have `AcpProposalCreated` and `AcpProposalResolved`. No
    /// timestamp field, matching every other `DomainEvent` variant —
    /// ordering comes from emission order, not a payload timestamp.
    AcpToolInvoked {
        session_id: String,
        tool: String,
        summary: String,
    },
    /// Emitted once per proposal the assistant queues. Nothing was written.
    AcpProposalCreated {
        session_id: String,
        proposal_id: String,
        summary: String,
    },
    /// Emitted when a proposal leaves `pending`. `status` is `accepted`,
    /// `rejected`, `stale` or `failed`.
    AcpProposalResolved {
        session_id: String,
        proposal_id: String,
        status: String,
    },
    /// Emitted for every tool call start or change during a prompt turn.
    /// `title` and `status` are the last known values, so the UI can upsert
    /// by `call_id`. `status` is `pending`, `in_progress`, `completed` or
    /// `failed`.
    AcpToolActivity {
        session_id: String,
        call_id: String,
        title: String,
        status: String,
    },
    /// Emitted with the agent's full option list whenever it changes, during
    /// a turn or after `set_config_option`.
    AcpConfigOptionsChanged {
        session_id: String,
        options: Vec<ConfigOption>,
    },
    /// Emitted when the agent reports context window use. `cost_usd` is the
    /// cumulative session cost, present only when reported in US dollars.
    AcpUsage {
        session_id: String,
        used: u64,
        size: u64,
        cost_usd: Option<f64>,
    },
    /// Emitted when the agent asks the user to approve something. The turn
    /// waits for the answer. `kind` is the ACP tool kind in snake_case;
    /// `summary` is short, masked text about the call.
    AcpPermissionRequested {
        session_id: String,
        request_id: String,
        call_id: String,
        title: String,
        kind: String,
        summary: String,
        options: Vec<PermissionOption>,
    },
    /// Emitted when a permission request was answered. `option_id` is the
    /// chosen option, or `None` when the request was cancelled.
    AcpPermissionResolved {
        session_id: String,
        request_id: String,
        option_id: Option<String>,
    },
    /// Emitted after an assistant conversation was saved. Carries the id
    /// only, never message text.
    AssistantConversationSaved {
        conversation_id: String,
    },
    /// Emitted after one stored assistant conversation was deleted.
    AssistantConversationDeleted {
        conversation_id: String,
    },
    /// Emitted after every stored conversation of the workspace was deleted.
    AssistantConversationsCleared,
    /// Emitted after a background OpenAPI sync check. `status` is `unchanged`,
    /// `changed` or `failed`. It is a refresh signal and carries no URL or message.
    OpenApiSyncChecked {
        collection: String,
        status: String,
    },
    /// Emitted once after an OpenAPI sync apply finished. `applied` and `stale` are counts,
    /// so the event carries no names or paths. It is a refresh signal.
    OpenApiSyncApplied {
        collection: String,
        applied: usize,
        stale: usize,
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
        /// True when `data` was cut to the live event size cap. Text then
        /// ends with a marker; base64 is cut whole so it still decodes.
        /// Left out of the JSON when false, so the pinned key set holds.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
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

    /// The file watcher could not start, so the sidebar will not refresh by itself.
    /// The message holds the directory and the error text only.
    FileWatcherFailed {
        message: String,
    },

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
    fn publish(&self, event: &dyn PublishedEvent);
}

/// No-op publisher for tests and contexts where events aren't needed.
pub struct NullEventPublisher;

impl EventPublisher for NullEventPublisher {
    fn publish(&self, _event: &dyn PublishedEvent) {
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
    fn file_watcher_failed_wire_shape() {
        let event = DomainEvent::FileWatcherFailed {
            message: "boom".into(),
        };
        let value = serde_json::to_value(&event).expect("serializes");
        assert_eq!(
            value,
            serde_json::json!({ "type": "fileWatcherFailed", "message": "boom" })
        );
    }

    #[test]
    fn event_publisher_trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn EventPublisher>) {}
    }

    #[test]
    fn null_publisher_does_not_panic() {
        let pub_ = NullEventPublisher;
        pub_.publish(&DomainEvent::AssistantConversationsCleared);
    }

    #[test]
    fn null_publisher_accepts_a_published_event() {
        NullEventPublisher.publish(&DomainEvent::CollectionCreated { name: "a".into() });
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
    fn collection_trust_changed_wire_shape() {
        let event = DomainEvent::CollectionTrustChanged {
            collection: "my-api".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"collectionTrustChanged","collection":"my-api"}"#
        );
    }

    #[test]
    fn openapi_sync_checked_wire_shape() {
        let event = DomainEvent::OpenApiSyncChecked {
            collection: "my-api".into(),
            status: "changed".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"openApiSyncChecked","collection":"my-api","status":"changed"}"#
        );
    }

    #[test]
    fn openapi_sync_applied_wire_shape() {
        let event = DomainEvent::OpenApiSyncApplied {
            collection: "api".into(),
            applied: 3,
            stale: 1,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"openApiSyncApplied","collection":"api","applied":3,"stale":1}"#
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
    fn flow_partial_run_info_wire_shape() {
        let info = FlowPartialRunInfo {
            base_run_id: "01A".into(),
            start_node_id: "n2".into(),
            mode: FlowPartialMode::FromHere,
            node_ids: vec!["n2".into(), "n3".into()],
        };
        let json = serde_json::to_string(&info).expect("serialize");
        assert_eq!(
            json,
            r#"{"baseRunId":"01A","startNodeId":"n2","mode":"fromHere","nodeIds":["n2","n3"]}"#
        );
        let back: FlowPartialRunInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, info);
        assert_eq!(
            serde_json::to_string(&FlowPartialMode::Node).expect("serialize"),
            r#""node""#
        );
    }

    #[test]
    fn a_flow_poll_detail_rides_in_the_step_trace() {
        let trace = FlowStepTrace {
            poll: Some(FlowPollDetail {
                attempts: 3,
                max_attempts: 30,
                last_status_code: Some(200),
                condition_met: Some(true),
                elapsed_ms: 2_500,
                timeout_ms: 60_000,
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&trace).expect("serialize");
        assert_eq!(
            json,
            r#"{"poll":{"attempts":3,"maxAttempts":30,"lastStatusCode":200,"conditionMet":true,"elapsedMs":2500,"timeoutMs":60000}}"#
        );
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
    fn acp_thought_chunk_wire_shape() {
        let event = DomainEvent::AcpThoughtChunk {
            session_id: "sess-1".into(),
            text: "Checking the auth header. ".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpThoughtChunk","session_id":"sess-1","text":"Checking the auth header. "}"#
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
            truncated: false,
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
    fn a_flow_wire_value_carries_its_expression_hash_in_camel_case() {
        let wire = FlowWireValue {
            edge_id: "e1".into(),
            source_node_id: "in".into(),
            target_field: "body".into(),
            expression_hash: Some("eb16adfc".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&wire).expect("serialize");
        assert!(json.contains(r#""expressionHash":"eb16adfc""#), "{json}");
        let back: FlowWireValue = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, wire);
    }

    #[test]
    fn a_flow_wire_value_saved_before_the_hash_existed_still_parses() {
        let old = r#"{"edgeId":"e1","sourceNodeId":"in","targetField":"body","value":"42"}"#;
        let wire: FlowWireValue = serde_json::from_str(old).expect("deserialize");
        assert_eq!(wire.expression_hash, None);
        assert_eq!(wire.value.as_deref(), Some("42"));
    }

    #[test]
    fn a_flow_wait_detail_carries_the_last_rejected_call() {
        let trace = FlowStepTrace {
            wait: Some(FlowWaitDetail {
                ignored: 2,
                timeout_ms: 60_000,
                last_rejected: Some(FlowRejectedCall {
                    method: "POST".into(),
                    url: "/cb/…?x=1".into(),
                    headers: Vec::new(),
                    body: "{}".into(),
                    body_truncated: false,
                    reason: "Accept when returned false.".into(),
                }),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&trace).expect("serialize");
        assert_eq!(
            json,
            r#"{"wait":{"ignored":2,"timeoutMs":60000,"lastRejected":{"method":"POST","url":"/cb/…?x=1","headers":[],"body":"{}","reason":"Accept when returned false."}}}"#
        );
    }

    #[test]
    fn acp_proposal_created_wire_shape() {
        let event = DomainEvent::AcpProposalCreated {
            session_id: "sess-1".into(),
            proposal_id: "p-1".into(),
            summary: "Create folder 'reports' in demo".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpProposalCreated","session_id":"sess-1","proposal_id":"p-1","summary":"Create folder 'reports' in demo"}"#
        );
    }

    #[test]
    fn acp_proposal_resolved_wire_shape() {
        let event = DomainEvent::AcpProposalResolved {
            session_id: "sess-1".into(),
            proposal_id: "p-1".into(),
            status: "stale".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpProposalResolved","session_id":"sess-1","proposal_id":"p-1","status":"stale"}"#
        );
    }

    #[test]
    fn acp_tool_invoked_wire_shape() {
        let event = DomainEvent::AcpToolInvoked {
            session_id: "sess-1".into(),
            tool: "run_request".into(),
            summary: "Ran GET /users".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpToolInvoked","session_id":"sess-1","tool":"run_request","summary":"Ran GET /users"}"#
        );
    }

    #[test]
    fn assistant_conversation_events_carry_ids_only() {
        let saved = DomainEvent::AssistantConversationSaved {
            conversation_id: "c1".into(),
        };
        assert_eq!(
            serde_json::to_string(&saved).expect("serialize"),
            r#"{"type":"assistantConversationSaved","conversation_id":"c1"}"#
        );
        let deleted = DomainEvent::AssistantConversationDeleted {
            conversation_id: "c1".into(),
        };
        assert_eq!(
            serde_json::to_string(&deleted).expect("serialize"),
            r#"{"type":"assistantConversationDeleted","conversation_id":"c1"}"#
        );
        assert_eq!(
            serde_json::to_string(&DomainEvent::AssistantConversationsCleared).expect("serialize"),
            r#"{"type":"assistantConversationsCleared"}"#
        );
    }

    #[test]
    fn acp_tool_activity_wire_shape() {
        let event = DomainEvent::AcpToolActivity {
            session_id: "sess-1".into(),
            call_id: "call-1".into(),
            title: "Read file".into(),
            status: "in_progress".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpToolActivity","session_id":"sess-1","call_id":"call-1","title":"Read file","status":"in_progress"}"#
        );
    }

    #[test]
    fn acp_config_options_changed_wire_shape() {
        use crate::acp::ConfigChoice;
        let event = DomainEvent::AcpConfigOptionsChanged {
            session_id: "sess-1".into(),
            options: vec![ConfigOption {
                id: "model".into(),
                name: "Model".into(),
                category: Some("model".into()),
                current_value: "opus".into(),
                choices: vec![ConfigChoice {
                    value: "opus".into(),
                    name: "Opus".into(),
                    description: None,
                }],
            }],
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpConfigOptionsChanged","session_id":"sess-1","options":[{"id":"model","name":"Model","category":"model","current_value":"opus","choices":[{"value":"opus","name":"Opus","description":null}]}]}"#
        );
    }

    #[test]
    fn acp_usage_wire_shape_with_and_without_cost() {
        let with_cost = DomainEvent::AcpUsage {
            session_id: "sess-1".into(),
            used: 53_000,
            size: 200_000,
            cost_usd: Some(0.045),
        };
        assert_eq!(
            serde_json::to_string(&with_cost).expect("serialize"),
            r#"{"type":"acpUsage","session_id":"sess-1","used":53000,"size":200000,"cost_usd":0.045}"#
        );
        let without_cost = DomainEvent::AcpUsage {
            session_id: "sess-1".into(),
            used: 1,
            size: 2,
            cost_usd: None,
        };
        assert_eq!(
            serde_json::to_string(&without_cost).expect("serialize"),
            r#"{"type":"acpUsage","session_id":"sess-1","used":1,"size":2,"cost_usd":null}"#
        );
    }

    #[test]
    fn acp_permission_events_wire_shape() {
        use crate::acp::{PermissionOption, PermissionOptionKind};
        let requested = DomainEvent::AcpPermissionRequested {
            session_id: "sess-1".into(),
            request_id: "perm-1".into(),
            call_id: "call-1".into(),
            title: "propose_changes".into(),
            kind: "other".into(),
            summary: "{}".into(),
            options: vec![PermissionOption {
                id: "proceed_once".into(),
                name: "Allow once".into(),
                kind: PermissionOptionKind::AllowOnce,
            }],
        };
        assert_eq!(
            serde_json::to_string(&requested).expect("serialize"),
            r#"{"type":"acpPermissionRequested","session_id":"sess-1","request_id":"perm-1","call_id":"call-1","title":"propose_changes","kind":"other","summary":"{}","options":[{"id":"proceed_once","name":"Allow once","kind":"allow_once"}]}"#
        );
        let resolved = DomainEvent::AcpPermissionResolved {
            session_id: "sess-1".into(),
            request_id: "perm-1".into(),
            option_id: None,
        };
        assert_eq!(
            serde_json::to_string(&resolved).expect("serialize"),
            r#"{"type":"acpPermissionResolved","session_id":"sess-1","request_id":"perm-1","option_id":null}"#
        );
    }

    fn sorted_keys(event: &DomainEvent) -> Vec<String> {
        let value = serde_json::to_value(event).expect("serialize");
        let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        keys
    }

    fn assert_keys(event: &DomainEvent, expected: &[&str]) {
        let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
        want.sort();
        assert_eq!(sorted_keys(event), want);
    }

    // Wire contract: tag is camelCase, fields stay snake_case. A rename here
    // breaks the frontend listeners and needs its own decision.
    #[test]
    fn wire_contract_collection_and_git_events() {
        assert_keys(
            &DomainEvent::CollectionRenamed {
                old_name: "a".into(),
                new_name: "b".into(),
            },
            &["type", "old_name", "new_name"],
        );
        assert_keys(
            &DomainEvent::GitCommit {
                collection: "c".into(),
                message: "m".into(),
                sha: "s".into(),
            },
            &["type", "collection", "message", "sha"],
        );
        assert_keys(
            &DomainEvent::BranchSwitched {
                collection: "c".into(),
                branch: "b".into(),
            },
            &["type", "collection", "branch"],
        );
        assert_keys(&DomainEvent::AssistantConversationsCleared, &["type"]);
    }

    #[test]
    fn wire_contract_stream_events() {
        assert_keys(
            &DomainEvent::WebSocketMessage {
                session_id: "s".into(),
                direction: WebSocketDirection::Out,
                kind: WebSocketPayloadKind::Text,
                data: "d".into(),
                size: 1,
                timestamp_ms: 0,
                truncated: false,
            },
            &[
                "type",
                "session_id",
                "direction",
                "kind",
                "data",
                "size",
                "timestamp_ms",
            ],
        );
        // The cut flag is additive: present only when true.
        assert_keys(
            &DomainEvent::WebSocketMessage {
                session_id: "s".into(),
                direction: WebSocketDirection::In,
                kind: WebSocketPayloadKind::Binary,
                data: "AAAA".into(),
                size: 9,
                timestamp_ms: 0,
                truncated: true,
            },
            &[
                "type",
                "session_id",
                "direction",
                "kind",
                "data",
                "size",
                "timestamp_ms",
                "truncated",
            ],
        );
    }

    // The envelope keys the bus adds must not collide with a top-level field.
    #[test]
    fn wire_contract_envelope_keys_are_free() {
        for event in [
            DomainEvent::AssistantConversationsCleared,
            DomainEvent::FileWatcherFailed {
                message: "m".into(),
            },
        ] {
            let keys = sorted_keys(&event);
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(!keys.iter().any(|k| k == reserved));
            }
        }
    }

    #[test]
    fn wire_contract_acp_and_watcher_events() {
        assert_keys(
            &DomainEvent::AcpSessionChunk {
                session_id: "s".into(),
                text: "t".into(),
            },
            &["type", "session_id", "text"],
        );
        assert_keys(
            &DomainEvent::AcpUsage {
                session_id: "s".into(),
                used: 1,
                size: 2,
                cost_usd: None,
            },
            &["type", "session_id", "used", "size", "cost_usd"],
        );
        assert_keys(
            &DomainEvent::FileWatcherFailed {
                message: "m".into(),
            },
            &["type", "message"],
        );
    }
}
