//! Events the request, runner, WebSocket, GraphQL and script paths publish.

use rocket_shared::published_event::PublishedEvent;
use serde::{Deserialize, Serialize};

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

/// Wire format: the `type` tag is camelCase and variant fields stay snake_case.
/// A rename breaks the frontend listeners and needs its own decision.
// Only the unit tests below read events back as JSON, so no Deserialize is derived.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExecutionEvent {
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
        /// Number of passes over the run set: the data file's rows, or 1 without a file.
        total_iterations: usize,
    },
    /// Emitted after every step of a run, in execution order.
    /// `status` is `"completed"`, `"skipped"`, or `"error"`.
    RunnerStepCompleted {
        run_id: String,
        /// Position in the emitted step stream, starting at 0.
        index: usize,
        /// Zero-based pass over the run set (0 without a data file).
        iteration: usize,
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
}

impl PublishedEvent for ExecutionEvent {
    fn channel(&self) -> &'static str {
        match self {
            ExecutionEvent::RequestExecuted { .. } => "request-executed",
            ExecutionEvent::RunnerStarted { .. } => "runner-started",
            ExecutionEvent::RunnerStepCompleted { .. } => "runner-step-completed",
            ExecutionEvent::RunnerFinished { .. } => "runner-finished",
            ExecutionEvent::WebSocketMessage { .. } => "ws:message",
            ExecutionEvent::WebSocketStatus { .. } => "ws:status",
            ExecutionEvent::GraphQlSubscriptionMessage { .. } => "graphql:subscription-message",
            ExecutionEvent::GraphQlSubscriptionStatus { .. } => "graphql:subscription-status",
            ExecutionEvent::ConsoleOutput { .. } => "script-console",
            ExecutionEvent::TestsCompleted { .. } => "script-tests",
            ExecutionEvent::ScriptError { .. } => "script-error",
            ExecutionEvent::ScriptVariableWritten { .. } => "script-variable-written",
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

    /// Every variant (populated and `None` forms), its JSON and its channel. The JSON
    /// was captured from the old `DomainEvent` variants.
    fn cases() -> Vec<(ExecutionEvent, serde_json::Value, &'static str)> {
        vec![
            (
                ExecutionEvent::RequestExecuted {
                    method: s("GET"),
                    url: s("https://x.test/a"),
                    status: 200,
                    duration_ms: 12,
                },
                json!({"type":"requestExecuted","method":"GET","url":"https://x.test/a","status":200,"duration_ms":12}),
                "request-executed",
            ),
            (
                ExecutionEvent::RunnerStarted {
                    run_id: s("r"),
                    collection: s("c"),
                    folder_path: Some(s("f")),
                    total_steps: 3,
                    total_iterations: 3,
                },
                json!({"type":"runnerStarted","run_id":"r","collection":"c","folder_path":"f","total_steps":3,"total_iterations":3}),
                "runner-started",
            ),
            (
                ExecutionEvent::RunnerStarted {
                    run_id: s("r"),
                    collection: s("c"),
                    folder_path: None,
                    total_steps: 3,
                    total_iterations: 1,
                },
                json!({"type":"runnerStarted","run_id":"r","collection":"c","folder_path":null,"total_steps":3,"total_iterations":1}),
                "runner-started",
            ),
            (
                ExecutionEvent::RunnerStepCompleted {
                    run_id: s("r"),
                    index: 0,
                    iteration: 1,
                    item_name: s("L"),
                    request_path: s("a.yml"),
                    status: s("completed"),
                    status_code: Some(200),
                    duration_ms: 12,
                    test_pass_count: 2,
                    test_fail_count: 1,
                    script_error: Some(s("se")),
                    error: Some(s("e")),
                },
                json!({"type":"runnerStepCompleted","run_id":"r","index":0,"iteration":1,"item_name":"L","request_path":"a.yml","status":"completed","status_code":200,"duration_ms":12,"test_pass_count":2,"test_fail_count":1,"script_error":"se","error":"e"}),
                "runner-step-completed",
            ),
            (
                ExecutionEvent::RunnerStepCompleted {
                    run_id: s("r"),
                    index: 0,
                    iteration: 1,
                    item_name: s("L"),
                    request_path: s("a.yml"),
                    status: s("skipped"),
                    status_code: None,
                    duration_ms: 0,
                    test_pass_count: 0,
                    test_fail_count: 0,
                    script_error: None,
                    error: None,
                },
                json!({"type":"runnerStepCompleted","run_id":"r","index":0,"iteration":1,"item_name":"L","request_path":"a.yml","status":"skipped","status_code":null,"duration_ms":0,"test_pass_count":0,"test_fail_count":0,"script_error":null,"error":null}),
                "runner-step-completed",
            ),
            (
                ExecutionEvent::RunnerFinished {
                    run_id: s("r"),
                    stopped_reason: s("completed"),
                    step_count: 3,
                    failed_count: 1,
                },
                json!({"type":"runnerFinished","run_id":"r","stopped_reason":"completed","step_count":3,"failed_count":1}),
                "runner-finished",
            ),
            (
                ExecutionEvent::WebSocketMessage {
                    session_id: s("s"),
                    direction: WebSocketDirection::In,
                    kind: WebSocketPayloadKind::Text,
                    data: s("hi"),
                    size: 2,
                    timestamp_ms: 5,
                    truncated: false,
                },
                json!({"type":"webSocketMessage","session_id":"s","direction":"in","kind":"text","data":"hi","size":2,"timestamp_ms":5}),
                "ws:message",
            ),
            (
                ExecutionEvent::WebSocketMessage {
                    session_id: s("s"),
                    direction: WebSocketDirection::Out,
                    kind: WebSocketPayloadKind::Binary,
                    data: s("AQID"),
                    size: 3,
                    timestamp_ms: 5,
                    truncated: true,
                },
                json!({"type":"webSocketMessage","session_id":"s","direction":"out","kind":"binary","data":"AQID","size":3,"timestamp_ms":5,"truncated":true}),
                "ws:message",
            ),
            (
                ExecutionEvent::WebSocketStatus {
                    session_id: s("s"),
                    state: WebSocketSessionState::Failed,
                    subprotocol: Some(s("p")),
                    code: Some(4001),
                    reason: Some(s("bye")),
                },
                json!({"type":"webSocketStatus","session_id":"s","state":"failed","subprotocol":"p","code":4001,"reason":"bye"}),
                "ws:status",
            ),
            (
                ExecutionEvent::WebSocketStatus {
                    session_id: s("s"),
                    state: WebSocketSessionState::Open,
                    subprotocol: None,
                    code: None,
                    reason: None,
                },
                json!({"type":"webSocketStatus","session_id":"s","state":"open","subprotocol":null,"code":null,"reason":null}),
                "ws:status",
            ),
            (
                ExecutionEvent::GraphQlSubscriptionMessage {
                    session_id: s("s"),
                    event: GraphQlSubscriptionEventKind::Next,
                    data: s("{}"),
                    timestamp_ms: 7,
                },
                json!({"type":"graphQlSubscriptionMessage","session_id":"s","event":"next","data":"{}","timestamp_ms":7}),
                "graphql:subscription-message",
            ),
            (
                ExecutionEvent::GraphQlSubscriptionStatus {
                    session_id: s("s"),
                    state: WebSocketSessionState::Open,
                    dialect: Some(s("graphql-transport-ws")),
                    reason: Some(s("r")),
                },
                json!({"type":"graphQlSubscriptionStatus","session_id":"s","state":"open","dialect":"graphql-transport-ws","reason":"r"}),
                "graphql:subscription-status",
            ),
            (
                ExecutionEvent::GraphQlSubscriptionStatus {
                    session_id: s("s"),
                    state: WebSocketSessionState::Closed,
                    dialect: None,
                    reason: None,
                },
                json!({"type":"graphQlSubscriptionStatus","session_id":"s","state":"closed","dialect":null,"reason":null}),
                "graphql:subscription-status",
            ),
            (
                ExecutionEvent::ConsoleOutput {
                    request_name: s("R"),
                    entries: vec![json!({"level":"log","message":"m"})],
                },
                json!({"type":"consoleOutput","request_name":"R","entries":[{"level":"log","message":"m"}]}),
                "script-console",
            ),
            (
                ExecutionEvent::ConsoleOutput {
                    request_name: s("R"),
                    entries: vec![],
                },
                json!({"type":"consoleOutput","request_name":"R","entries":[]}),
                "script-console",
            ),
            (
                ExecutionEvent::TestsCompleted {
                    request_name: s("R"),
                    results: vec![json!({"name":"t","status":"fail","error":"x"})],
                },
                json!({"type":"testsCompleted","request_name":"R","results":[{"error":"x","name":"t","status":"fail"}]}),
                "script-tests",
            ),
            (
                ExecutionEvent::TestsCompleted {
                    request_name: s("R"),
                    results: vec![],
                },
                json!({"type":"testsCompleted","request_name":"R","results":[]}),
                "script-tests",
            ),
            (
                ExecutionEvent::ScriptError {
                    request_name: s("R"),
                    phase: s("tests"),
                    message: s("boom"),
                },
                json!({"type":"scriptError","request_name":"R","phase":"tests","message":"boom"}),
                "script-error",
            ),
            (
                ExecutionEvent::ScriptVariableWritten {
                    scope: s("environment"),
                    environment: Some(s("staging")),
                    collection: None,
                    key: s("K"),
                },
                json!({"type":"scriptVariableWritten","scope":"environment","environment":"staging","collection":null,"key":"K"}),
                "script-variable-written",
            ),
            (
                ExecutionEvent::ScriptVariableWritten {
                    scope: s("collection"),
                    environment: None,
                    collection: Some(s("c")),
                    key: s("K"),
                },
                json!({"type":"scriptVariableWritten","scope":"collection","environment":null,"collection":"c","key":"K"}),
                "script-variable-written",
            ),
        ]
    }

    fn sorted_keys(event: &ExecutionEvent) -> Vec<String> {
        let value = serde_json::to_value(event).expect("serialize");
        let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        keys
    }

    fn assert_keys(event: &ExecutionEvent, expected: &[&str]) {
        let mut want: Vec<String> = expected.iter().map(|k| k.to_string()).collect();
        want.sort();
        assert_eq!(sorted_keys(event), want);
    }

    #[test]
    fn execution_events_json_are_unchanged() {
        for (event, expected, _) in cases() {
            assert_eq!(event.to_json().expect("event serializes"), expected);
        }
    }

    #[test]
    fn execution_events_keep_their_channels() {
        let mut seen = std::collections::BTreeSet::new();
        for (event, _, channel) in cases() {
            assert_eq!(event.channel(), channel);
            seen.insert(channel);
        }
        let expected: std::collections::BTreeSet<&str> = [
            "request-executed",
            "runner-started",
            "runner-step-completed",
            "runner-finished",
            "ws:message",
            "ws:status",
            "graphql:subscription-message",
            "graphql:subscription-status",
            "script-console",
            "script-tests",
            "script-error",
            "script-variable-written",
        ]
        .into_iter()
        .collect();
        assert_eq!(seen, expected);
    }

    #[test]
    fn execution_events_have_no_extra_channels_or_written_collections() {
        for (event, _, _) in cases() {
            assert!(event.also_emit_on().is_empty(), "{:?}", event);
            assert!(event.written_collections().is_empty(), "{:?}", event);
        }
        // A script variable write does not feed RecentWrites.
        let written = ExecutionEvent::ScriptVariableWritten {
            scope: s("collection"),
            environment: None,
            collection: Some(s("c")),
            key: s("K"),
        };
        assert!(written.written_collections().is_empty());
    }

    #[test]
    fn script_variable_written_serializes() {
        let event = ExecutionEvent::ScriptVariableWritten {
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
    fn runner_started_wire_shape() {
        let event = ExecutionEvent::RunnerStarted {
            run_id: "01J".into(),
            collection: "my-api".into(),
            folder_path: Some("auth".into()),
            total_steps: 3,
            total_iterations: 2,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"runnerStarted","run_id":"01J","collection":"my-api","folder_path":"auth","total_steps":3,"total_iterations":2}"#
        );
    }

    #[test]
    fn runner_step_completed_wire_shape() {
        let event = ExecutionEvent::RunnerStepCompleted {
            run_id: "01J".into(),
            index: 0,
            iteration: 1,
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
        assert!(json.contains(r#""iteration":1"#));
    }

    #[test]
    fn runner_finished_wire_shape() {
        let event = ExecutionEvent::RunnerFinished {
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
    fn graphql_subscription_events_serialize_like_the_websocket_ones() {
        let message = ExecutionEvent::GraphQlSubscriptionMessage {
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

        let status = ExecutionEvent::GraphQlSubscriptionStatus {
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
        let message = ExecutionEvent::WebSocketMessage {
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

        let status = ExecutionEvent::WebSocketStatus {
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
    fn wire_contract_stream_events() {
        assert_keys(
            &ExecutionEvent::WebSocketMessage {
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
            &ExecutionEvent::WebSocketMessage {
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
}
