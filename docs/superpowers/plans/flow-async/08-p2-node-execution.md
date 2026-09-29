# Flow Async P2 — Plan 08: Wait for Callback Node Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a Wait for callback node actually wait: accept the first call that passes `accept_when`, fail after `timeout_ms`, stop at once on cancel, report progress every second, and hand the call downstream as a response.

**Architecture:** `RequestExecutionService` gains `evaluate_flow_callback_condition`, a sibling of `evaluate_flow_route_expression` that exposes the call as `request` instead of `response`. A new `flow_wait.rs` holds `callback_output` (call → `ExecuteRequestOutput`) and `FlowExecutionService::wait_for_callback`, a `tokio::select!` loop over the endpoint's calls, the deadline, the cancel signal and a one-second progress ticker. `execute_node` replaces plan 07's interim arm with a call to it.

**Tech Stack:** Rust, tokio (`select!`, `time::{interval, sleep_until, Instant}`), the Deno script engine.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` §7.4 ("When a WaitForCallback node runs"), §8. Interfaces are locked in `docs/superpowers/plans/flow-async/00-index.md` ("P2 — model").

## Global Constraints

- The deadline is `timeout_ms` from the moment the node starts. Time spent before its turn does not count.
- Calls that arrived before the node's turn are held and checked in arrival order.
- `accept_when` sees `request` with `method`, `path`, `query` (object), `headers` (object), `body` (parsed JSON, else text), and uses the same sandbox, timeout and `FlowCoercion::Bool` as an If condition.
- A script error in `accept_when` fails the node at once. A false result counts one ignored call and keeps waiting.
- Progress every second: `waiting… {left}s left · {k} ignored call(s)`.
- An accepted call succeeds the node. Its output is a `CapturedOutput::Request` with status `200`, the call's headers and body, and `duration_ms` measured from the node's start.
- Timeout error text: `no matching callback within {secs}s ({k} ignored)`.
- Stop during the wait fails the node with `cancelled` and ends the run with `stopped_reason = "cancelled"` (plan 01 handles the run side).
- No panicking-unwrap calls in production paths. Cargo always `-j4`, one crate at a time.

## Review Focus

1. A call that arrives before the node's turn (while an earlier request is still running) must not be lost. → Task 2 test `a_call_before_the_nodes_turn_is_accepted`.
2. A provider that sends `payment.pending` before `payment.completed` must end on the completed call, not the first one. → Task 2 test `accept_when_skips_calls_that_do_not_match`.
3. A plain-text or form body must reach `accept_when` and downstream wires as a string, not break JSON parsing. → Task 1 test `real_engine_callback_condition_gets_a_text_body_as_a_string`.
4. Stop while waiting on a long (60 s) timeout must end the run within a second. → Task 2 test `stop_during_the_wait_ends_the_run_promptly`.
5. A Wait node that never runs (its upstream failed) must still close its endpoint when the run ends. → Task 2 test `a_skipped_wait_node_still_closes_its_endpoint`.

---

### Task 1: `evaluate_flow_callback_condition` and `callback_output`

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (script wrapper, evaluation method, tests)
- Create: `crates/rocket-app/src/flow_wait.rs` (`callback_output`)
- Modify: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `ReceivedCall` (plan 06), `ExecuteRequestOutput.deferred_history` (plan 04), `FlowScriptOutcome`, `to_flow_logs` (`pub(crate)` since plan 04), `RequestExecutionService::evaluate_expression_with_logs`.
- Produces:
  - `RequestExecutionService::evaluate_flow_callback_condition(&self, collection: &str, call: &ReceivedCall, source: &str, secret_values: &HashSet<String>) -> FlowScriptOutcome` — result `"true"` / `"false"`.
  - `crate::flow_wait::callback_output(call: &ReceivedCall, duration_ms: u64) -> ExecuteRequestOutput` — `status_text` carries the call's method (see Step 3).

- [ ] **Step 1: Write the failing tests**

Add to the "Real script engine" section of the `tests` module in `crates/rocket-app/src/flow_execution_service.rs` (after `real_engine_wire_reads_json_body_field`):

```rust
    fn payment_call(body: &str) -> crate::callback_listener::ReceivedCall {
        crate::callback_listener::ReceivedCall {
            method: "POST".to_string(),
            path: "/cb/abc".to_string(),
            query: vec![("id".to_string(), "7".to_string())],
            headers: vec![("x-event".to_string(), "payment.completed".to_string())],
            body: body.to_string(),
        }
    }

    #[tokio::test]
    async fn real_engine_callback_condition_sees_method_path_query_headers_and_json_body() {
        let svc = real_engine_service();
        let value = svc
            .evaluate_flow_callback_condition(
                "my-api",
                &payment_call(r#"{"event":"payment.completed","orderId":42}"#),
                "request.method === 'POST' && request.path === '/cb/abc' \
                 && request.query.id === '7' \
                 && request.headers['x-event'] === 'payment.completed' \
                 && request.body.orderId === 42",
                &HashSet::new(),
            )
            .await
            .result
            .expect("the condition must evaluate");
        assert_eq!(value, "true");
    }

    #[tokio::test]
    async fn real_engine_callback_condition_is_false_when_it_does_not_match() {
        let svc = real_engine_service();
        let value = svc
            .evaluate_flow_callback_condition(
                "my-api",
                &payment_call(r#"{"event":"payment.pending"}"#),
                "request.body.event === 'payment.completed'",
                &HashSet::new(),
            )
            .await
            .result
            .expect("the condition must evaluate");
        assert_eq!(value, "false");
    }

    #[tokio::test]
    async fn real_engine_callback_condition_gets_a_text_body_as_a_string() {
        let svc = real_engine_service();
        let value = svc
            .evaluate_flow_callback_condition(
                "my-api",
                &payment_call("status=done&id=7"),
                "request.body === 'status=done&id=7'",
                &HashSet::new(),
            )
            .await
            .result
            .expect("a text body must not break evaluation");
        assert_eq!(value, "true");
    }

    #[tokio::test]
    async fn real_engine_callback_condition_reports_a_script_error() {
        let svc = real_engine_service();
        let outcome = svc
            .evaluate_flow_callback_condition(
                "my-api",
                &payment_call("{}"),
                "request.body.missing.deeper === 1",
                &HashSet::new(),
            )
            .await;
        assert!(outcome.result.is_err(), "a thrown TypeError must be an error");
    }
```

Create `crates/rocket-app/src/flow_wait.rs` with only its test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::callback_listener::ReceivedCall;

    #[test]
    fn callback_output_turns_a_call_into_a_200_response() {
        let call = ReceivedCall {
            method: "PUT".to_string(),
            path: "/cb/abc".to_string(),
            query: Vec::new(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: r#"{"orderId":42}"#.to_string(),
        };

        let out = callback_output(&call, 3100);

        assert_eq!(out.response.status, 200);
        assert_eq!(out.response.status_text, "PUT", "the method is reported here");
        assert_eq!(out.response.body, r#"{"orderId":42}"#);
        assert_eq!(out.response.duration_ms, 3100);
        assert_eq!(out.response.size_bytes, 14);
        assert_eq!(out.response.headers.len(), 1);
        assert_eq!(out.response.headers[0].key, "content-type");
        assert_eq!(out.response.headers[0].value, "application/json");
        assert!(out.deferred_history.is_none());
        assert!(out.script_error.is_none());
    }
}
```

Register it in `crates/rocket-app/src/lib.rs`, after `pub(crate) mod flow_callbacks;`:

```rust
pub(crate) mod flow_wait;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app callback_condition`
Expected: FAIL to compile — `no method named evaluate_flow_callback_condition`.

Run: `cargo test -j4 -p rocket-app callback_output`
Expected: FAIL to compile — `cannot find function callback_output`.

- [ ] **Step 3: Implement `callback_output`**

Put above the test module in `crates/rocket-app/src/flow_wait.rs`:

```rust
//! Execution of a Wait for callback node: turning an accepted call into
//! the node's output, and waiting for that call.

use rocket_http::HttpResponse;
use rocket_shared::types::Header;

use crate::callback_listener::ReceivedCall;
use crate::execution_service::ExecuteRequestOutput;

/// The node's output for an accepted call, shaped like a response so a
/// downstream wire reads it as `response.body` / `response.headers`.
/// `status_text` carries the call's method (for example `POST`), so the
/// step and `response.statusText` can say what was received.
pub(crate) fn callback_output(call: &ReceivedCall, duration_ms: u64) -> ExecuteRequestOutput {
    ExecuteRequestOutput {
        response: HttpResponse {
            status: 200,
            status_text: call.method.clone(),
            headers: call
                .headers
                .iter()
                .map(|(key, value)| Header {
                    key: key.clone(),
                    value: value.clone(),
                    enabled: true,
                    description: None,
                })
                .collect(),
            body: call.body.clone(),
            duration_ms,
            ttfb_ms: duration_ms,
            size_bytes: call.body.len(),
        },
        test_results: Vec::new(),
        console_entries: Vec::new(),
        script_error: None,
        deferred_history: None,
    }
}
```

- [ ] **Step 4: Implement `evaluate_flow_callback_condition`**

In `crates/rocket-app/src/flow_execution_service.rs`, add after `flow_script`:

```rust
/// Builds the JS that runs an `accept_when` condition against a `request`
/// object. `res.getBody()` returns the call object built by
/// `callback_request_json`, parsed. The same three parse forms as
/// `flow_script` apply, and the result is coerced with `!!(`.
fn flow_callback_script(source: &str) -> DomainResult<String> {
    let literal = serde_json::to_string(source)
        .map_err(|e| DomainError::Internal(format!("failed to encode flow script: {e}")))?;
    Ok(format!(
        r#"(() => {{
  const src = {literal};
  const isParseError = (e) => e instanceof SyntaxError;
  let fn = null;
  const asExpression = (text) => new Function('request', 'return (' + text + '\n)');
  try {{ fn = asExpression(src); }}
  catch (e) {{ if (!isParseError(e)) throw e; }}
  if (fn === null) {{
    try {{ fn = asExpression(src.replace(/;\s*$/, '')); }}
    catch (e) {{ if (!isParseError(e)) throw e; }}
  }}
  if (fn === null) fn = new Function('request', src);
  const request = res.getBody();
  return !!(fn(request));
}})()"#
    ))
}

/// The `request` object an `accept_when` condition sees. Headers and query
/// become objects (a repeated name keeps its last value); the body is parsed
/// JSON when it parses, else the raw text.
fn callback_request_json(call: &crate::callback_listener::ReceivedCall) -> serde_json::Value {
    let body = serde_json::from_str::<serde_json::Value>(&call.body)
        .unwrap_or_else(|_| serde_json::Value::String(call.body.clone()));
    let to_object = |pairs: &[(String, String)]| {
        serde_json::Value::Object(
            pairs
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
        )
    };
    serde_json::json!({
        "method": call.method,
        "path": call.path,
        "query": to_object(&call.query),
        "headers": to_object(&call.headers),
        "body": body,
    })
}
```

Add the method to the existing `impl RequestExecutionService` block in this file, after `evaluate_flow_route_expression`:

```rust
    /// Evaluates a Wait for callback node's `accept_when` against one
    /// received call. The script sees `request` (see `callback_request_json`)
    /// and its result is `"true"` or `"false"`.
    pub async fn evaluate_flow_callback_condition(
        &self,
        collection: &str,
        call: &crate::callback_listener::ReceivedCall,
        source: &str,
        secret_values: &HashSet<String>,
    ) -> FlowScriptOutcome {
        let script = match flow_callback_script(source) {
            Ok(script) => script,
            Err(e) => return FlowScriptOutcome::failed(e),
        };
        let body = callback_request_json(call).to_string();
        let carrier = rocket_http::HttpResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: Vec::new(),
            size_bytes: body.len(),
            body,
            duration_ms: 0,
            ttfb_ms: 0,
        };
        let response_json = match serde_json::to_string(&carrier) {
            Ok(json) => json,
            Err(e) => {
                return FlowScriptOutcome::failed(DomainError::Internal(format!(
                    "failed to serialize callback: {e}"
                )))
            }
        };
        let (result, entries) = self
            .evaluate_expression_with_logs(collection, &script, &response_json, secret_values.clone())
            .await;
        FlowScriptOutcome {
            result: result.map(|value| match value {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            }),
            logs: to_flow_logs(entries),
        }
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app callback_condition`
Expected: PASS — 4 tests.

Run: `cargo test -j4 -p rocket-app callback_output`
Expected: PASS — 1 test.

- [ ] **Step 6: Commit**

Stage `crates/rocket-app/src/flow_execution_service.rs`, `crates/rocket-app/src/flow_wait.rs`, `crates/rocket-app/src/lib.rs`, then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): evaluate callback conditions against the call`.

---

### Task 2: Wait for a matching call in `execute_node`

**Files:**
- Modify: `crates/rocket-app/src/flow_wait.rs` (`wait_for_callback`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`execute_node` arm, `result_to_step`, tests)
- Modify: `crates/rocket-app/src/flow_callbacks.rs` (drop the `dead_code` attribute on `endpoint_mut`)
- Modify: `crates/rocket-shared/src/events.rs` and `crates/rocket-app/src/flow_execution_service.rs` doc comments on `value` (see Step 4)

**Interfaces:**
- Consumes: `RunCallbacks::endpoint_mut` (plan 07), `NodeRunContext` and `CancelSignal::cancelled` (plan 01), `FlowExecutionService::publish_progress` (plan 02), `evaluate_flow_callback_condition` and `callback_output` (Task 1). The `execute_node` context parameter is named `ctx` since plan 04.
- Produces:

```rust
impl FlowExecutionService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn wait_for_callback(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        node: &FlowNode,
        timeout_ms: u64,
        accept_when: Option<&str>,
        secret_values: &HashSet<String>,
        logs: &mut Vec<FlowLogEntry>,
        ctx: &mut NodeRunContext,
        callbacks: &mut RunCallbacks,
    ) -> DomainResult<ExecutedNode>;
}
```

  `FlowStepResult.value` of a succeeded Wait for callback node is the received method (for example `"POST"`), so the card can show `✓ received POST · 3.1s`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module of `crates/rocket-app/src/flow_execution_service.rs`, next to plan 07's `wait_node` helper:

```rust
    fn wait_node_with(id: &str, timeout_ms: u64, accept_when: Option<&str>) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::WaitForCallback {
                label: format!("Wait {id}"),
                name: "payment".to_string(),
                timeout_ms,
                accept_when: accept_when.map(str::to_string),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn event_call(event: &str) -> crate::callback_listener::ReceivedCall {
        crate::callback_listener::ReceivedCall {
            method: "POST".to_string(),
            path: "/cb/0".to_string(),
            query: Vec::new(),
            headers: Vec::new(),
            body: format!(r#"{{"event":"{event}","orderId":42}}"#),
        }
    }

    /// `register -> wait (Run when)`.
    fn register_then_wait(wait: FlowNode) -> Flow {
        Flow {
            name: "cb".to_string(),
            nodes: vec![request_flow_node("reg", "https://api.example.com/register"), wait],
            edges: vec![trigger_edge("e1", "reg", handle::RESULT, "w")],
            callback_host: None,
        }
    }

    #[tokio::test]
    async fn a_call_before_the_nodes_turn_is_accepted() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        // Delivered when the endpoint opens, before `reg` even runs.
        fake.queue_on_open(event_call("payment.completed"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service_with_listener(register_then_wait(wait_node_with("w", 60_000, None)), &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        let step = step_of(&summary, "w");
        assert_eq!(step.status, FlowNodeStatus::Success, "{:?}", step.error);
        assert_eq!(step.status_code, Some(200));
        assert_eq!(step.value.as_deref(), Some("POST"));
        assert!(fake.is_closed(0), "the endpoint closes after a success");
    }

    #[tokio::test]
    async fn a_call_during_the_wait_is_accepted_and_progress_is_reported() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(register_then_wait(wait_node_with("w", 60_000, None)), &publisher)
            .with_callback_listener(Box::new(Arc::clone(&fake)));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let (summary, ()) = tokio::join!(service.run(&exec, run_input("cb")), async {
            fake.wait_opened(1).await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            fake.sender(0)
                .send(event_call("payment.completed"))
                .await
                .expect("send");
        });
        let summary = summary.expect("run");

        assert_eq!(status_of(&summary, "w"), FlowNodeStatus::Success);
        let progress: Vec<String> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepProgress { node_id, message, .. } if node_id == "w" => {
                    Some(message)
                }
                _ => None,
            })
            .collect();
        assert!(!progress.is_empty(), "the wait reports progress");
        assert!(
            progress[0].starts_with("waiting… ") && progress[0].ends_with("0 ignored call(s)"),
            "got: {progress:?}"
        );
    }

    #[tokio::test]
    async fn accept_when_skips_calls_that_do_not_match() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.pending"));
        fake.queue_on_open(event_call("payment.completed"));
        let executor = RecordingExecutor::new();
        // The generated accept_when script contains `const request`; this
        // rule answers from the carried call body.
        let exec = recording_exec(
            &executor,
            scripted(vec![(
                "const request",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.is_some_and(|r| r.body.contains("payment.completed")))
                }),
            )]),
        );
        let flow = register_then_wait(wait_node_with(
            "w",
            60_000,
            Some("request.body.event === 'payment.completed'"),
        ));

        let summary = service_with_listener(flow, &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "w"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn timeout_fails_the_node_and_reports_ignored_calls() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.pending"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("const request", Scripted::Value(serde_json::json!(false)))]),
        );
        let flow = register_then_wait(wait_node_with("w", 1000, Some("request.body.ok")));

        let summary = service_with_listener(flow, &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        let step = step_of(&summary, "w");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert_eq!(
            step.error.as_deref(),
            Some("Invalid input: no matching callback within 1s (1 ignored)")
        );
        assert!(fake.is_closed(0), "the endpoint closes after a failure");
    }

    #[tokio::test]
    async fn an_accept_when_script_error_fails_the_node_at_once() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.completed"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("const request", Scripted::Throw("ReferenceError: nope"))]),
        );
        let flow = register_then_wait(wait_node_with("w", 60_000, Some("nope.ok")));

        let started = std::time::Instant::now();
        let summary = service_with_listener(flow, &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "w"), FlowNodeStatus::Failed);
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "no 60 s wait");
    }

    #[tokio::test]
    async fn stop_during_the_wait_ends_the_run_promptly() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(register_then_wait(wait_node_with("w", 60_000, None)), &publisher)
            .with_callback_listener(Box::new(Arc::clone(&fake)));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let started = std::time::Instant::now();
        let (summary, ()) = tokio::join!(service.run(&exec, run_input("cb")), async {
            fake.wait_opened(1).await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let run_id = publisher
                .events()
                .into_iter()
                .find_map(|e| match e {
                    DomainEvent::FlowRunStarted { run_id, .. } => Some(run_id),
                    _ => None,
                })
                .expect("the run started");
            service.cancel(&run_id);
        });
        let summary = summary.expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        let step = step_of(&summary, "w");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert_eq!(step.error.as_deref(), Some("cancelled"));
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(fake.is_closed(0), "the endpoint closes after a cancel");
    }

    #[tokio::test]
    async fn a_skipped_wait_node_still_closes_its_endpoint() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let executor = RecordingExecutor::new();
        executor.set_status("register", 500);
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service_with_listener(register_then_wait(wait_node_with("w", 60_000, None)), &fake)
            .run(&exec, run_input("cb"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "w"), FlowNodeStatus::Skipped);
        assert!(fake.is_closed(0));
    }

    #[tokio::test]
    async fn a_downstream_wire_reads_the_callback_body() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.completed"));
        let flow = Flow {
            name: "cb".to_string(),
            nodes: vec![wait_node_with("w", 60_000, None), output_node_named("out")],
            edges: vec![edge_from("e1", "w", handle::RESULT, "out", "value", "response.body.orderId")],
            callback_host: None,
        };

        let summary = service_with_listener(flow, &fake)
            .run(&real_engine_service(), run_input("cb"))
            .await
            .expect("run");

        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("42"));
    }
```

Check the error prefix before relying on it: `DomainError::InvalidInput` displays as `Invalid input: …` (`crates/rocket-shared/src/error.rs:9`), and `result_to_step` stores `e.to_string()`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app wait`
Expected: FAIL — the Wait node is `Failed` with plan 07's interim message `Wait for callback nodes cannot run yet` in every success test, and there are no progress events.

- [ ] **Step 3: Implement `wait_for_callback`**

Append to `crates/rocket-app/src/flow_wait.rs`, above the test module (merge the `use` lines with the ones from Task 1):

```rust
use std::collections::HashSet;
use std::time::Duration;

use rocket_flow::FlowNode;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::FlowLogEntry;
use tokio::time::{interval, sleep_until, Instant};

use crate::execution_service::RequestExecutionService;
use crate::flow_callbacks::RunCallbacks;
use crate::flow_execution_service::{
    CapturedOutput, ExecutedNode, FlowExecutionService, NodeRunContext, RunFlowInput,
};

impl FlowExecutionService {
    /// Waits for the first call to this node's endpoint that passes
    /// `accept_when`. Fails on timeout, on an `accept_when` script error,
    /// or when the run is cancelled.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn wait_for_callback(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        node: &FlowNode,
        timeout_ms: u64,
        accept_when: Option<&str>,
        secret_values: &HashSet<String>,
        logs: &mut Vec<FlowLogEntry>,
        ctx: &mut NodeRunContext,
        callbacks: &mut RunCallbacks,
    ) -> DomainResult<ExecutedNode> {
        let endpoint = callbacks.endpoint_mut(&node.id).ok_or_else(|| {
            DomainError::Internal(format!("no callback endpoint for node '{}'", node.id))
        })?;
        let started = Instant::now();
        let deadline = started + Duration::from_millis(timeout_ms);
        // The first tick fires at once, so progress shows as soon as the node waits.
        let mut ticker = interval(Duration::from_secs(1));
        let mut ignored: u32 = 0;

        loop {
            tokio::select! {
                biased;
                _ = ctx.cancel.cancelled() => {
                    return Err(DomainError::Internal("cancelled".into()));
                }
                _ = sleep_until(deadline) => {
                    let secs = timeout_ms as f64 / 1000.0;
                    return Err(DomainError::InvalidInput(format!(
                        "no matching callback within {secs}s ({ignored} ignored)"
                    )));
                }
                call = endpoint.calls.recv() => {
                    let Some(call) = call else {
                        return Err(DomainError::Internal(format!(
                            "the callback endpoint of node '{}' closed",
                            node.id
                        )));
                    };
                    if let Some(source) = accept_when {
                        let outcome = exec
                            .evaluate_flow_callback_condition(
                                &input.collection,
                                &call,
                                source,
                                secret_values,
                            )
                            .await;
                        logs.extend(outcome.logs);
                        match outcome.result?.as_str() {
                            "true" => {}
                            "false" => {
                                ignored += 1;
                                continue;
                            }
                            other => {
                                return Err(DomainError::InvalidInput(format!(
                                    "accept_when of node '{}' evaluated to '{other}', expected true or false",
                                    node.id
                                )))
                            }
                        }
                    }
                    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                    return Ok(ExecutedNode::plain(CapturedOutput::Request(Box::new(
                        callback_output(&call, duration_ms),
                    ))));
                }
                _ = ticker.tick() => {
                    let left = deadline.saturating_duration_since(Instant::now()).as_secs();
                    self.publish_progress(
                        ctx,
                        None,
                        None,
                        format!("waiting… {left}s left · {ignored} ignored call(s)"),
                    );
                }
            }
        }
    }
}
```

`tokio::select!` drops the other branch futures before running a handler, so the `ctx` borrow in the progress handler does not clash with `ctx.cancel.cancelled()`. `ExecutedNode` is already `pub(crate)`, `NodeRunContext` (plan 01) lives in `flow_execution_service.rs`, and `RunFlowInput` is `pub`. `ExecutedNode::plain` is still a private `fn`; change it to `pub(crate) fn plain` so `flow_wait.rs` can call it.

In `crates/rocket-app/src/flow_callbacks.rs`, remove the `#[cfg_attr(not(test), allow(dead_code))]` line above `endpoint_mut`.

- [ ] **Step 4: Call it from `execute_node` and report the method**

In `execute_node`, replace plan 07's interim arm with:

```rust
            FlowNodeKind::WaitForCallback {
                timeout_ms,
                accept_when,
                ..
            } => {
                self.wait_for_callback(
                    exec,
                    input,
                    node,
                    *timeout_ms,
                    accept_when.as_deref(),
                    &secret_values,
                    logs,
                    ctx,
                    callbacks,
                )
                .await
            }
```

In `result_to_step`, inside the `Ok(ExecutedNode { output: CapturedOutput::Request(out), .. })` arm, set `value` for Wait nodes. Add before building the result:

```rust
            let is_wait = matches!(kind, Some(FlowNodeKind::WaitForCallback { .. }));
```

and add this field to the returned `FlowStepResult` (keep the fields plan 04 added, such as `attempts`):

```rust
                // A Wait for callback node reports the method it received.
                value: is_wait.then(|| out.response.status_text.clone()),
```

Update the doc comment on `FlowStepResult.value` in `flow_execution_service.rs` and on `DomainEvent::FlowStepCompleted.value` in `crates/rocket-shared/src/events.rs` to read: "The node's captured output value for `Output` nodes, or the received method (e.g. `POST`) for a succeeded Wait for callback node. `None` for every other node."

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app wait`
Expected: PASS, including the 8 tests from Step 1 (the timeout test takes about 1 s).

Run: `cargo test -j4 -p rocket-app callback`
Expected: PASS (plan 06 and plan 07 tests still green; plan 07's run tests now wait up to 1 s each for their unanswered Wait nodes).

Run: `cargo test -j4 -p rocket-app flow_execution_service`
Expected: PASS.

Run: `cargo check -j4 -p rocket --tests`
Expected: no errors.

- [ ] **Step 6: Commit**

Stage `crates/rocket-app/src/flow_wait.rs`, `crates/rocket-app/src/flow_execution_service.rs`, `crates/rocket-app/src/flow_callbacks.rs`, `crates/rocket-shared/src/events.rs`, then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): wait for a matching callback in a run`.
