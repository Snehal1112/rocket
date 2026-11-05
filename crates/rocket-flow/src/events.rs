//! Events the flow context publishes.
//!
//! The payload helper types live in `rocket-shared` and are camelCase inside
//! these snake_case events.

use crate::run_types::{
    FlowCallbackInfo, FlowDebugRequest, FlowLiveProgress, FlowLogEntry, FlowNodeStatus,
    FlowPartialRunInfo, FlowRewriteFailure, FlowSkipReason, FlowStepTrace,
};
use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

/// A flow ran, or flows followed a moved request.
// Only the unit tests below read events back, so Deserialize is test-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FlowEvent {
    /// Emitted once when a Flow run starts, before its first node executes.
    FlowRunStarted {
        run_id: String,
        flow_name: String,
        collection: String,
        total_nodes: usize,
        /// Every Wait for callback node of the run, with no URL. Events are
        /// broadcast to every listener, so the bearer URL is fetched on
        /// demand instead. The nested fields are camelCase inside this
        /// snake_case event.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        callbacks: Vec<FlowCallbackInfo>,
        /// Set for a partial run. `total_nodes` then counts only its nodes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        partial: Option<FlowPartialRunInfo>,
    },
    /// Emitted immediately before a node is dispatched — once per node that
    /// is actually attempted, never for a node marked `Skipped` (those never
    /// reach dispatch).
    FlowStepStarted { run_id: String, node_id: String },
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
        /// Structured progress for the Last run tab. The nested fields are
        /// camelCase inside this snake_case event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        live: Option<Box<FlowLiveProgress>>,
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
        /// Always omitted from this event. The masked sent record of a debug-mode
        /// Request node is on the step of the run summary. Kept for old payloads.
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
    /// Emitted after a request or folder move, with the flows that followed it.
    FlowRequestPathsRewritten {
        collection: String,
        from: String,
        to: String,
        flows: Vec<String>,
        failed: Vec<FlowRewriteFailure>,
    },
}

impl PublishedEvent for FlowEvent {
    fn channel(&self) -> &'static str {
        match self {
            FlowEvent::FlowRunStarted { .. } => "flow-run-started",
            FlowEvent::FlowStepStarted { .. } => "flow-step-started",
            FlowEvent::FlowStepProgress { .. } => "flow-step-progress",
            FlowEvent::FlowStepCompleted { .. } => "flow-step-completed",
            FlowEvent::FlowRunFinished { .. } => "flow-run-finished",
            FlowEvent::FlowRequestPathsRewritten { .. } => "flow-request-paths-rewritten",
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
    use crate::run_types::{
        FlowDebugHeader, FlowDebugResponse, FlowLogLevel, FlowPartialMode, FlowPollDetail,
        FlowRejectedCall, FlowRouteEval, FlowWaitDetail, FlowWireValue,
    };
    use serde_json::json;

    fn s(v: &str) -> String {
        v.to_string()
    }

    fn rejected() -> FlowRejectedCall {
        FlowRejectedCall {
            method: s("POST"),
            url: s("/cb/…"),
            headers: vec![FlowDebugHeader {
                key: s("k"),
                value: s("v"),
            }],
            body: s("b"),
            body_truncated: true,
            reason: s("wrong method"),
        }
    }

    fn exchange() -> FlowDebugRequest {
        FlowDebugRequest {
            method: s("GET"),
            url: s("http://x"),
            headers: vec![FlowDebugHeader {
                key: s("a"),
                value: s("b"),
            }],
            body: Some(s("body")),
            body_truncated: true,
            response: Some(FlowDebugResponse {
                status: 200,
                status_text: s("OK"),
                duration_ms: 5,
                size_bytes: 9,
                headers: vec![],
                body: s("ok"),
                truncated: true,
            }),
            error: Some(s("e")),
        }
    }

    /// Minimal forms first (optionals absent), then populated forms.
    fn cases() -> Vec<(FlowEvent, serde_json::Value)> {
        vec![
            (
                FlowEvent::FlowRunStarted {
                    run_id: s("r1"),
                    flow_name: s("F"),
                    collection: s("c"),
                    total_nodes: 3,
                    callbacks: vec![],
                    partial: None,
                },
                json!({"type":"flowRunStarted","run_id":"r1","flow_name":"F","collection":"c","total_nodes":3}),
            ),
            (
                FlowEvent::FlowRunStarted {
                    run_id: s("r1"),
                    flow_name: s("F"),
                    collection: s("c"),
                    total_nodes: 3,
                    callbacks: vec![FlowCallbackInfo {
                        node_id: s("w"),
                        name: s("pay"),
                    }],
                    partial: Some(FlowPartialRunInfo {
                        base_run_id: s("r0"),
                        start_node_id: s("n2"),
                        mode: FlowPartialMode::FromHere,
                        node_ids: vec![s("n2"), s("n3")],
                    }),
                },
                json!({"type":"flowRunStarted","run_id":"r1","flow_name":"F","collection":"c","total_nodes":3,"callbacks":[{"nodeId":"w","name":"pay"}],"partial":{"baseRunId":"r0","startNodeId":"n2","mode":"fromHere","nodeIds":["n2","n3"]}}),
            ),
            (
                FlowEvent::FlowStepStarted {
                    run_id: s("r1"),
                    node_id: s("n1"),
                },
                json!({"type":"flowStepStarted","run_id":"r1","node_id":"n1"}),
            ),
            (
                FlowEvent::FlowStepProgress {
                    run_id: s("r1"),
                    node_id: s("n1"),
                    attempt: None,
                    max_attempts: None,
                    message: s("m"),
                    live: None,
                },
                json!({"type":"flowStepProgress","run_id":"r1","node_id":"n1","attempt":null,"max_attempts":null,"message":"m"}),
            ),
            (
                FlowEvent::FlowStepProgress {
                    run_id: s("r1"),
                    node_id: s("n1"),
                    attempt: Some(3),
                    max_attempts: Some(30),
                    message: s("attempt 3/30"),
                    live: Some(Box::new(FlowLiveProgress {
                        last_status_code: Some(202),
                        condition_met: Some(false),
                        elapsed_ms: Some(10),
                        remaining_ms: Some(20),
                        ignored: Some(1),
                        last_rejected: Some(rejected()),
                    })),
                },
                json!({"type":"flowStepProgress","run_id":"r1","node_id":"n1","attempt":3,"max_attempts":30,"message":"attempt 3/30","live":{"lastStatusCode":202,"conditionMet":false,"elapsedMs":10,"remainingMs":20,"ignored":1,"lastRejected":{"method":"POST","url":"/cb/…","headers":[{"key":"k","value":"v"}],"body":"b","bodyTruncated":true,"reason":"wrong method"}}}),
            ),
            (
                FlowEvent::FlowStepCompleted {
                    run_id: s("r1"),
                    node_id: s("n1"),
                    status: FlowNodeStatus::Success,
                    status_code: None,
                    duration_ms: None,
                    error: None,
                    value: None,
                    skip_reason: None,
                    branch: None,
                    logs: vec![],
                    debug_request: None,
                    attempts: None,
                    exchange: None,
                    trace: None,
                },
                json!({"type":"flowStepCompleted","run_id":"r1","node_id":"n1","status":"success","status_code":null,"duration_ms":null,"error":null,"value":null}),
            ),
            (
                FlowEvent::FlowStepCompleted {
                    run_id: s("r1"),
                    node_id: s("n1"),
                    status: FlowNodeStatus::Skipped,
                    status_code: Some(200),
                    duration_ms: Some(7),
                    error: Some(s("err")),
                    value: Some(s("val")),
                    skip_reason: Some(FlowSkipReason::BranchNotTaken),
                    branch: Some(s("true")),
                    logs: vec![FlowLogEntry {
                        level: FlowLogLevel::Warn,
                        message: s("w"),
                    }],
                    debug_request: Some(Box::new(exchange())),
                    attempts: Some(2),
                    exchange: Some(Box::new(exchange())),
                    trace: Some(Box::new(FlowStepTrace {
                        wires: vec![FlowWireValue {
                            edge_id: s("e"),
                            source_node_id: s("n0"),
                            target_field: s("url"),
                            value: Some(s("v")),
                            truncated: true,
                            credential: true,
                            expression_hash: Some(s("h")),
                            error: Some(s("x")),
                        }],
                        route: Some(FlowRouteEval {
                            kind: s("switch"),
                            value: s("a"),
                            matched_case: Some(s("c1")),
                        }),
                        poll: Some(FlowPollDetail {
                            attempts: 2,
                            max_attempts: 5,
                            last_status_code: Some(200),
                            condition_met: Some(true),
                            elapsed_ms: 3,
                            timeout_ms: 9,
                        }),
                        wait: Some(FlowWaitDetail {
                            ignored: 1,
                            timeout_ms: 100,
                            last_rejected: Some(rejected()),
                        }),
                        failed_edge_id: Some(s("e")),
                        value_truncated: true,
                    })),
                },
                json!({"type":"flowStepCompleted","run_id":"r1","node_id":"n1","status":"skipped","status_code":200,"duration_ms":7,"error":"err","value":"val","skip_reason":"branch_not_taken","branch":"true","logs":[{"level":"warn","message":"w"}],"debug_request":{"method":"GET","url":"http://x","headers":[{"key":"a","value":"b"}],"body":"body","bodyTruncated":true,"response":{"status":200,"statusText":"OK","durationMs":5,"sizeBytes":9,"headers":[],"body":"ok","truncated":true},"error":"e"},"attempts":2,"exchange":{"method":"GET","url":"http://x","headers":[{"key":"a","value":"b"}],"body":"body","bodyTruncated":true,"response":{"status":200,"statusText":"OK","durationMs":5,"sizeBytes":9,"headers":[],"body":"ok","truncated":true},"error":"e"},"trace":{"wires":[{"edgeId":"e","sourceNodeId":"n0","targetField":"url","value":"v","truncated":true,"credential":true,"expressionHash":"h","error":"x"}],"route":{"kind":"switch","value":"a","matchedCase":"c1"},"poll":{"attempts":2,"maxAttempts":5,"lastStatusCode":200,"conditionMet":true,"elapsedMs":3,"timeoutMs":9},"wait":{"ignored":1,"timeoutMs":100,"lastRejected":{"method":"POST","url":"/cb/…","headers":[{"key":"k","value":"v"}],"body":"b","bodyTruncated":true,"reason":"wrong method"}},"failedEdgeId":"e","valueTruncated":true}}),
            ),
            (
                FlowEvent::FlowRunFinished {
                    run_id: s("r1"),
                    stopped_reason: s("completed"),
                    node_count: 5,
                    failed_count: 1,
                    skipped_count: 2,
                    not_taken_count: 0,
                },
                json!({"type":"flowRunFinished","run_id":"r1","stopped_reason":"completed","node_count":5,"failed_count":1,"skipped_count":2,"not_taken_count":0}),
            ),
            (
                FlowEvent::FlowRunFinished {
                    run_id: s("r1"),
                    stopped_reason: s("completed"),
                    node_count: 5,
                    failed_count: 1,
                    skipped_count: 2,
                    not_taken_count: 1,
                },
                json!({"type":"flowRunFinished","run_id":"r1","stopped_reason":"completed","node_count":5,"failed_count":1,"skipped_count":2,"not_taken_count":1}),
            ),
            (
                FlowEvent::FlowRequestPathsRewritten {
                    collection: s("c"),
                    from: s("a.yml"),
                    to: s("b.yml"),
                    flows: vec![],
                    failed: vec![],
                },
                json!({"type":"flowRequestPathsRewritten","collection":"c","from":"a.yml","to":"b.yml","flows":[],"failed":[]}),
            ),
            (
                FlowEvent::FlowRequestPathsRewritten {
                    collection: s("c"),
                    from: s("a.yml"),
                    to: s("b.yml"),
                    flows: vec![s("f1")],
                    failed: vec![FlowRewriteFailure {
                        flow: s("f2"),
                        message: s("bad"),
                    }],
                },
                json!({"type":"flowRequestPathsRewritten","collection":"c","from":"a.yml","to":"b.yml","flows":["f1"],"failed":[{"flow":"f2","message":"bad"}]}),
            ),
        ]
    }

    #[test]
    fn flow_events_json_are_unchanged() {
        for (event, want) in cases() {
            assert_eq!(event.to_json().expect("serializes"), want);
        }
    }

    #[test]
    fn flow_run_started_ignores_a_url_in_an_old_payload() {
        let json = r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"f","collection":"c","total_nodes":1,"callbacks":[{"nodeId":"w","name":"p","url":"http://h/cb/tok"}]}"#;
        let event: FlowEvent = serde_json::from_str(json).expect("old payload");
        let back = serde_json::to_string(&event).expect("serialize");
        assert!(!back.contains("tok"), "{back}");
    }

    #[test]
    fn flow_events_keep_their_channels() {
        let channels: Vec<&str> = cases().iter().map(|(e, _)| e.channel()).collect();
        assert_eq!(
            channels,
            vec![
                "flow-run-started",
                "flow-run-started",
                "flow-step-started",
                "flow-step-progress",
                "flow-step-progress",
                "flow-step-completed",
                "flow-step-completed",
                "flow-run-finished",
                "flow-run-finished",
                "flow-request-paths-rewritten",
                "flow-request-paths-rewritten",
            ]
        );
    }

    #[test]
    fn flow_events_have_no_extra_channels_or_written_collections() {
        for (event, _) in cases() {
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    #[test]
    fn flow_step_progress_keeps_its_wire_keys() {
        let event = FlowEvent::FlowStepProgress {
            run_id: s("r"),
            node_id: s("n"),
            attempt: None,
            max_attempts: None,
            message: s("m"),
            live: None,
        };
        let value = event.to_json().expect("serializes");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "attempt",
                "max_attempts",
                "message",
                "node_id",
                "run_id",
                "type"
            ]
        );
    }

    #[test]
    fn flow_request_paths_rewritten_serializes_with_a_type_tag() {
        let event = FlowEvent::FlowRequestPathsRewritten {
            collection: "c".into(),
            from: "a.yml".into(),
            to: "b.yml".into(),
            flows: vec!["f1".into()],
            failed: vec![FlowRewriteFailure {
                flow: "f2".into(),
                message: "bad".into(),
            }],
        };
        let value = serde_json::to_value(&event).expect("serializes");
        assert_eq!(value["type"], "flowRequestPathsRewritten");
        assert_eq!(value["failed"][0]["flow"], "f2");
    }

    #[test]
    fn flow_run_started_carries_callback_names_but_no_url() {
        let event = FlowEvent::FlowRunStarted {
            run_id: "01J".into(),
            flow_name: "Pay".into(),
            collection: "acme".into(),
            total_nodes: 2,
            callbacks: vec![FlowCallbackInfo {
                node_id: "w".into(),
                name: "payment".into(),
            }],
            partial: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains(r#""callbacks":[{"nodeId":"w","name":"payment"}]"#),
            "{json}"
        );
        assert!(!json.contains("url") && !json.contains("/cb/"), "{json}");
    }

    #[test]
    fn flow_run_started_without_callbacks_still_deserializes() {
        let json = r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"Login Flow","collection":"acme","total_nodes":3}"#;
        match serde_json::from_str::<FlowEvent>(json).expect("old payload") {
            FlowEvent::FlowRunStarted { callbacks, .. } => assert!(callbacks.is_empty()),
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn flow_run_started_wire_shape() {
        let event = FlowEvent::FlowRunStarted {
            run_id: "01J".into(),
            flow_name: "Login Flow".into(),
            collection: "acme".into(),
            total_nodes: 3,
            callbacks: Vec::new(),
            partial: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"Login Flow","collection":"acme","total_nodes":3}"#
        );
    }

    #[test]
    fn flow_run_started_carries_partial_run_info() {
        let event = FlowEvent::FlowRunStarted {
            run_id: "01J".into(),
            flow_name: "f".into(),
            collection: "c".into(),
            total_nodes: 1,
            callbacks: Vec::new(),
            partial: Some(FlowPartialRunInfo {
                base_run_id: "01A".into(),
                start_node_id: "n2".into(),
                mode: FlowPartialMode::Node,
                node_ids: vec!["n2".into()],
            }),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"f","collection":"c","total_nodes":1,"partial":{"baseRunId":"01A","startNodeId":"n2","mode":"node","nodeIds":["n2"]}}"#
        );
        let old = r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"f","collection":"c","total_nodes":1}"#;
        let back: FlowEvent = serde_json::from_str(old).expect("old JSON still reads");
        assert!(matches!(
            back,
            FlowEvent::FlowRunStarted { partial: None, .. }
        ));
    }

    #[test]
    fn flow_step_progress_carries_live_detail_in_camel_case() {
        let event = FlowEvent::FlowStepProgress {
            run_id: "01J".into(),
            node_id: "n".into(),
            attempt: Some(2),
            max_attempts: Some(5),
            message: "attempt 2/5 · condition false".into(),
            live: Some(Box::new(FlowLiveProgress {
                last_status_code: Some(202),
                condition_met: Some(false),
                remaining_ms: Some(12_000),
                ..Default::default()
            })),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains(
                r#""live":{"lastStatusCode":202,"conditionMet":false,"remainingMs":12000}"#
            ),
            "{json}"
        );
    }

    #[test]
    fn flow_step_progress_without_live_still_deserializes() {
        let json = r#"{"type":"flowStepProgress","run_id":"01J","node_id":"n","attempt":3,"max_attempts":30,"message":"attempt 3/30"}"#;
        match serde_json::from_str::<FlowEvent>(json).expect("old payload") {
            FlowEvent::FlowStepProgress { live, .. } => assert!(live.is_none()),
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn flow_step_progress_wire_shape() {
        let event = FlowEvent::FlowStepProgress {
            run_id: "01J".into(),
            node_id: "n".into(),
            attempt: Some(3),
            max_attempts: Some(30),
            message: "attempt 3/30".into(),
            live: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowStepProgress","run_id":"01J","node_id":"n","attempt":3,"max_attempts":30,"message":"attempt 3/30"}"#
        );
    }

    #[test]
    fn flow_step_progress_without_attempts_sends_nulls() {
        let event = FlowEvent::FlowStepProgress {
            run_id: "01J".into(),
            node_id: "n".into(),
            attempt: None,
            max_attempts: None,
            message: "waiting… 42s left".into(),
            live: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains(r#""attempt":null,"max_attempts":null"#),
            "got {json}"
        );
    }

    #[test]
    fn flow_step_completed_serializes_logs_in_lowercase_and_omits_empty_logs() {
        let with_logs = FlowEvent::FlowStepCompleted {
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

        let without = FlowEvent::FlowStepCompleted {
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
        let event: FlowEvent = serde_json::from_str(json).expect("deserialize old payload");
        match &event {
            FlowEvent::FlowStepCompleted { attempts, .. } => assert_eq!(*attempts, None),
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
        let event = |debug_request| FlowEvent::FlowStepCompleted {
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
        let event = FlowEvent::FlowStepCompleted {
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
        let event = FlowEvent::FlowStepCompleted {
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
        let event: FlowEvent = serde_json::from_str(json).expect("deserialize");
        match event {
            FlowEvent::FlowStepCompleted {
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
    fn flow_run_finished_wire_shape_tracks_failed_and_skipped_separately() {
        let event = FlowEvent::FlowRunFinished {
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
    fn flow_step_completed_wire_shape_with_skip_reason() {
        let event = FlowEvent::FlowStepCompleted {
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
        let event = FlowEvent::FlowStepCompleted {
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
        let event: FlowEvent = serde_json::from_str(json).expect("deserialize");
        match event {
            FlowEvent::FlowStepCompleted {
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
        let event: FlowEvent = serde_json::from_str(json).expect("deserialize");
        match event {
            FlowEvent::FlowRunFinished {
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
    fn flow_step_completed_without_exchange_still_deserializes() {
        let json = r#"{"type":"flowStepCompleted","run_id":"r","node_id":"n","status":"success","status_code":200,"duration_ms":5,"error":null,"value":null}"#;
        let event: FlowEvent = serde_json::from_str(json).expect("old payload");
        match event {
            FlowEvent::FlowStepCompleted { exchange, .. } => assert!(exchange.is_none()),
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn flow_step_completed_carries_the_exchange_in_camel_case() {
        let event = FlowEvent::FlowStepCompleted {
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
        let event = |trace| FlowEvent::FlowStepCompleted {
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
        let event: FlowEvent = serde_json::from_str(json).expect("old payload");
        match event {
            FlowEvent::FlowStepCompleted { trace, .. } => assert!(trace.is_none()),
            other => panic!("unexpected event {other:?}"),
        }
    }
}
