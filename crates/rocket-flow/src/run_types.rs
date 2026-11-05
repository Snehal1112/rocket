//! Flow run payload types. They ride inside `FlowEvent` and the stored step traces.

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

/// One flow that could not be rewritten after a request or folder move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRewriteFailure {
    pub flow: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
