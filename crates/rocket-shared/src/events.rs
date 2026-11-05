use serde::{Deserialize, Serialize};

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

/// The collection events left after the split into per-context enums.
/// Plan 6 moves them too.
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
        pub_.publish(&DomainEvent::FileWatcherFailed {
            message: "m".into(),
        });
    }

    #[test]
    fn null_publisher_accepts_a_published_event() {
        NullEventPublisher.publish(&DomainEvent::CollectionCreated { name: "a".into() });
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
    }

    // The envelope keys the bus adds must not collide with a top-level field.
    #[test]
    fn wire_contract_envelope_keys_are_free() {
        for event in [DomainEvent::FileWatcherFailed {
            message: "m".into(),
        }] {
            let keys = sorted_keys(&event);
            for reserved in ["seq", "tsMs", "ts_ms", "workspaceId"] {
                assert!(!keys.iter().any(|k| k == reserved));
            }
        }
    }

    #[test]
    fn wire_contract_watcher_events() {
        assert_keys(
            &DomainEvent::FileWatcherFailed {
                message: "m".into(),
            },
            &["type", "message"],
        );
    }
}
