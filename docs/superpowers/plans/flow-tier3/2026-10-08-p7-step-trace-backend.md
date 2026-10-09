# Step Trace Backend Implementation Plan

> **Execute this plan:** P7. Before starting it, make sure these are merged to main: none (P7, P9 and P19 all edit `execute_node` in `flow_execution_service.rs`, so never run them in parallel). After it is merged, the next plan to execute is P8. Status and the full order are in `00-plan-index.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every Flow step that ran reports how long it took, what value arrived on each of its wires (masked and size-capped), how an If or Switch decided, and which wire failed.

**Architecture:** One optional carrier, `trace: Option<FlowStepTrace>`, is added to the `FlowStepResult` IPC DTO and to the `FlowStepCompleted` event, so every existing struct literal only gains `trace: None`. A new internal module `flow_trace.rs` holds `NodeTrace`, a `FlowStepTrace` being filled in. `execute_node` gets it as one more `&mut` out-param next to `poll_stats`, so wires recorded before an error still reach the step. The run loop times every `execute_node` call and fills `duration_ms` when the node did not set its own. Every recorded value is masked first and capped second.

**Tech Stack:** Rust, serde, tokio tests, `rocket-shared` events, `rocket-app` Flow engine.

**Spec:** Roadmap items F-32 (durations), F-38 (route evaluation), F-39 (wire values and edge-named errors) in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` section "P7 Step trace backend".

## Global Constraints

- No OpenCollection spec read is needed: no `.yml`, collection, environment, auth model or variable-resolution change. The Request arm keeps resolving exactly as before.
- No new IPC command. `FlowStepResult` and the new trace types are IPC DTOs and use `#[serde(rename_all = "camelCase")]`. `DomainEvent` top-level keys stay snake_case. No persistence struct changes.
- No `unwrap()` or `expect()` outside `#[cfg(test)]`.
- Masking rules (mandatory): mask first, cap second. Wire values, route values and wire errors are masked with the Output-arm set: `secret_values` plus `credentials.secret_forms()`. Never use the Input or Transform set (`secret_values` only) for trace values. Record wires before the Request arm rebinds `secret_values` with the send-time credential. An `auth` wire is recorded as `{ credential: true, value: None }` and `credentials.auth_for_node` is never read for the trace. Never embed a resolved value in an error message.
- Known masking limits, unchanged by this plan: secrets shorter than `MIN_REDACTION_LEN` (6) and encoded forms (base64, URL-encoded inside a wire value) are not masked.
- Cargo commands use `-j4` and `-p <crate>`. Never `--workspace` or `--all`.
- Code comments are short full sentences ending with a punctuation mark.
- Commits go through the `dev-workflow-skills:1-git-commit` skill with explicit staged paths. Never `git add -A`, `--all` or `.`.
- Only one implementer at a time touches `execute_node` (P7, P9 and P19 all edit it).
- Not in scope: poll and wait details (`FlowPollDetail`, `FlowWaitDetail`, P9), any frontend change (P8), a `total_ms` run field, redacting step `error` text (roadmap F-02).

## Decisions assumed

- No open decision from the index (D1 to D6) affects this plan.
- `cap_body` in `flow_debug.rs` becomes a call to a new `pub(crate) fn cap_text(text, limit)`, instead of making `cap_body` itself public, because the trace needs other limits than 256 KB.
- The Input, Transform and Output step `value` is capped at 256 KB (`STEP_VALUE_LIMIT`). The cut is flagged in `trace.valueTruncated`, so `FlowStepResult` gains no second new field.
- A single-input node (If, Switch, Transform) records the text its source captured (a response body or a value), masked and capped. It is not a re-evaluation of the wire.
- A routing node records `route` whenever its script produced a value, also when that value then fails the node. A script error records no route.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A secret leaks through a new field: an Auth token wired into a non-sensitive header, or a secret environment value wired from an Input, shows unmasked in `trace.wires` or in the serialized summary. Tests pinned in Task 2 (`an_auth_token_wired_into_a_header_is_recorded_masked`, `an_input_wired_into_an_output_records_the_wire_masked`, `a_switch_records_its_value_masked_and_the_matched_case`).
2. An `auth` credential wire records its credential. Test pinned in Task 2 (`an_auth_wire_is_recorded_as_a_credential_without_a_value`).
3. A very large value: a 300 KB value is cut before masking, so half a secret survives, or one step's wires exceed 64 KB in total. Tests pinned in Task 1 (`a_large_input_value_is_masked_before_it_is_capped`) and Task 2 (`an_oversized_wire_value_is_cut_and_flagged`, `record_wire_stops_at_the_step_total`).
4. An old JSON payload without the new keys fails to deserialize, or the new keys appear when empty and break the exact-JSON tests. Tests pinned in Task 1 (`flow_step_completed_without_trace_still_deserializes`, existing exact-JSON tests stay green, the pre-Phase-2 `FlowStepResult` test).
5. A failing wire loses the values of the wires before it, or its error quotes the resolved value. Tests pinned in Task 3 (`an_earlier_wire_keeps_its_value_when_a_later_wire_fails`, `a_wire_error_never_quotes_the_resolved_value`).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-shared/src/events.rs` (modify) | New DTOs `FlowStepTrace`, `FlowWireValue`, `FlowRouteEval`. `FlowStepCompleted` gains `trace`. Wire-shape tests. |
| `crates/rocket-app/src/flow_trace.rs` (new) | `NodeTrace`, the limits, `mask_then_cap`, and the record helpers. Pure, unit-tested. |
| `crates/rocket-app/src/lib.rs` (modify) | `pub(crate) mod flow_trace;`. |
| `crates/rocket-app/src/flow_debug.rs` (modify) | `pub(crate) fn cap_text(text, limit)`; `cap_body` calls it. |
| `crates/rocket-app/src/flow_execution_service.rs` (modify) | `FlowStepResult.trace`, run-loop timing and merge, `execute_node` out-param, wire capture, route eval, edge-named errors. |
| `crates/rocket-app/src/flow_routing.rs` (modify) | The "live inputs" failure lists its edge ids. |
| `crates/rocket-app/CLAUDE.md` (modify) | One short paragraph on the step trace and its masking rules. |

Existing tests to know: `crates/rocket-shared/src/events.rs` test module (exact-JSON tests from line 862), `crates/rocket-app/src/flow_execution_service.rs` test module (helpers `wire` :3169, `run_input` :3180, `env_with` :3666, `scoped_exec` :3687, `input_node_with` :3722, `output_node_named` :3733, `recording_exec` :3839, `fixed_wire` :3857, `service_with_publisher` :3863, `edge_from` :4784, `step_of` :4800, `if_node` :4989, `switch_node` :5001, `input_edge` :5020, `if_flow` :5029, `run_transform` :7743, `auth_and_request_flow` :3408, `service_with_saved_request` :3431, `auth_wire` :3564, `recording_http_exec` :3390, `ErrorJsonqEngine` :1932, `scripted` :1988). Line numbers are from HEAD b047bbc6. Find each by name if they moved.

---

### Task 1: Trace types, step durations and capped step values

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (new types after `FlowDebugRequest`, which ends at line 78; `FlowStepCompleted` at lines 295-332; tests from line 909)
- Create: `crates/rocket-app/src/flow_trace.rs`
- Modify: `crates/rocket-app/src/lib.rs:23-24`
- Modify: `crates/rocket-app/src/flow_debug.rs:133-145`
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`FlowStepResult` :687-718, `step_completed_event` :722-738, run loop :1018-1078, `execute_node` :1176-1192, Input arm :1212-1218, Output arm :1245-1248, Transform arm :1470-1471, `result_to_step` base :1574-1587, `skipped_step` :1646, `failed_step` :1663, test literal :4484-4497)

**Interfaces:**
- Produces (Rust, `rocket_shared::events`): `FlowStepTrace { wires: Vec<FlowWireValue>, route: Option<FlowRouteEval>, failed_edge_id: Option<String>, value_truncated: bool }`, `FlowWireValue { edge_id, source_node_id, target_field, value: Option<String>, truncated: bool, credential: bool, error: Option<String> }`, `FlowRouteEval { kind: String, value: String, matched_case: Option<String> }`. All derive `Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize` (`FlowRouteEval` without `Default`).
- Produces (JSON, camelCase inside the snake_case event): `trace: { wires?: [{ edgeId, sourceNodeId, targetField, value?, truncated?, credential?, error? }], route?: { kind, value, matchedCase? }, failedEdgeId?, valueTruncated? }`. Empty collections, `None` and `false` are omitted.
- Produces: `FlowStepResult.trace: Option<FlowStepTrace>`, `DomainEvent::FlowStepCompleted.trace: Option<Box<FlowStepTrace>>`.
- Produces (crate): `flow_trace::NodeTrace { pub(crate) step: FlowStepTrace, .. }`, `NodeTrace::into_trace(self) -> Option<FlowStepTrace>`, `flow_trace::mask_then_cap(raw, masks, limit) -> (String, bool)`, `flow_trace::STEP_VALUE_LIMIT`, `flow_debug::cap_text(&mut String, usize) -> bool`.
- Produces: `execute_node(..., poll_stats, trace: &mut NodeTrace, ctx, callbacks)`. P9 and P19 rely on this position.

- [ ] **Step 1: Write the failing wire-shape tests**

In `crates/rocket-shared/src/events.rs`, inside `mod tests`, append:

```rust
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
        assert!(!json.contains("value"), "{json}");
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
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-shared flow_step`
Expected: FAIL to compile (`FlowStepTrace`, `FlowWireValue`, `FlowRouteEval` and the `trace` field do not exist).

- [ ] **Step 3: Add the types and the event field**

In `crates/rocket-shared/src/events.rs`, after the closing `}` of `FlowDebugRequest` (line 78), insert:

```rust
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
```

In `DomainEvent::FlowStepCompleted`, replace the `duration_ms` doc comment (lines 302-303):

```rust
        /// `None` for a node that never executed (Skipped) or has no
        /// meaningful duration (Input/Output nodes).
```

with:

```rust
        /// How long the node ran. A Request reports its response time and a
        /// repeat-until poll its total. `None` only for a node that never ran.
```

and add after the `exchange` field (line 331):

```rust
        /// What the step saw on its wires and how it routed, masked and capped.
        /// The nested fields are camelCase inside this snake_case event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trace: Option<Box<FlowStepTrace>>,
```

- [ ] **Step 4: Add `trace: None` to the existing event literals and the exhaustive pattern**

In the same test module, add `trace: None,` after the `exchange: ...,` line of each `DomainEvent::FlowStepCompleted { ... }` struct literal (8 literals, in the tests starting at lines 910, 967, 1004, 1028, 1146, 1170 and 1375; the first test has two). In `flow_step_completed_deserializes_with_optional_keys_missing` (line 1055), add `trace,` to the destructuring pattern after `exchange,` and `assert_eq!(trace, None);` after `assert_eq!(exchange, None);`.

- [ ] **Step 5: Run the shared tests**

Run: `cargo test -j4 -p rocket-shared`
Expected: PASS, including every existing exact-JSON test (a `None` trace is omitted).

- [ ] **Step 6: Write the failing `flow_trace` unit tests**

Create `crates/rocket-app/src/flow_trace.rs` with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction::REDACTED;

    fn masks(values: &[&str]) -> HashSet<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn an_untouched_trace_is_none() {
        assert_eq!(NodeTrace::default().into_trace(), None);
    }

    #[test]
    fn a_trace_with_only_a_value_cut_is_kept() {
        let mut trace = NodeTrace::default();
        trace.step.value_truncated = true;
        assert!(trace.into_trace().is_some_and(|t| t.value_truncated));
    }

    #[test]
    fn mask_then_cap_masks_before_it_cuts() {
        // Cut first, the limit would keep "sk-l" of the secret.
        let raw = format!("{}sk-live-123456", "a".repeat(20));
        let (text, cut) = mask_then_cap(&raw, &masks(&["sk-live-123456"]), 24);
        assert!(cut);
        assert!(!text.contains("sk-"), "{text}");
        assert!(text.len() <= 24);
        assert!(text.starts_with(&"a".repeat(20)));
    }

    #[test]
    fn mask_then_cap_leaves_short_text_alone() {
        let (text, cut) = mask_then_cap("token sk-live-123456", &masks(&["sk-live-123456"]), 100);
        assert!(!cut);
        assert_eq!(text, format!("token {REDACTED}"));
    }
}
```

In `crates/rocket-app/src/lib.rs`, add between `pub mod flow_service;` (line 23) and `pub(crate) mod flow_wait;` (line 24):

```rust
pub(crate) mod flow_trace;
```

- [ ] **Step 7: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_trace`
Expected: FAIL to compile (`NodeTrace`, `mask_then_cap`, `HashSet` are missing).

- [ ] **Step 8: Add `cap_text` and the module body**

In `crates/rocket-app/src/flow_debug.rs`, replace `cap_body` (lines 133-145):

```rust
/// Cuts `body` to `EXCHANGE_BODY_LIMIT` bytes at a UTF-8 boundary. Returns
/// true when it cut anything.
fn cap_body(body: &mut String) -> bool {
    if body.len() <= EXCHANGE_BODY_LIMIT {
        return false;
    }
    let mut cut = EXCHANGE_BODY_LIMIT;
    while !body.is_char_boundary(cut) {
        cut -= 1;
    }
    body.truncate(cut);
    true
}
```

with:

```rust
/// Cuts `body` to `EXCHANGE_BODY_LIMIT` bytes at a UTF-8 boundary. Returns
/// true when it cut anything.
fn cap_body(body: &mut String) -> bool {
    cap_text(body, EXCHANGE_BODY_LIMIT)
}

/// Cuts `text` to at most `limit` bytes at a UTF-8 boundary. Returns true
/// when it cut anything.
pub(crate) fn cap_text(text: &mut String, limit: usize) -> bool {
    if text.len() <= limit {
        return false;
    }
    let mut cut = limit;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    true
}
```

At the top of `crates/rocket-app/src/flow_trace.rs`, above the test module, add:

```rust
//! Collects what one Flow step saw: the value on each wire into it, how a
//! routing node decided, and which wire failed. Every value is masked first
//! and capped second, so a cap can never cut a secret in half and show the rest.

use std::collections::HashSet;

use rocket_shared::events::FlowStepTrace;

use crate::flow_debug::cap_text;
use crate::redaction::redact_secrets;

/// The largest value an Input, Transform or Output step reports, in bytes.
pub(crate) const STEP_VALUE_LIMIT: usize = crate::flow_debug::EXCHANGE_BODY_LIMIT;

/// Masks `raw` with `masks`, then cuts it to `limit` bytes. Returns the text
/// and whether it was cut.
pub(crate) fn mask_then_cap(raw: &str, masks: &HashSet<String>, limit: usize) -> (String, bool) {
    let mut text = redact_secrets(raw, masks);
    let cut = cap_text(&mut text, limit);
    (text, cut)
}

/// The trace of the node that is running. `execute_node` fills it as it goes,
/// so what was recorded before an error still reaches the step.
#[derive(Debug, Default)]
pub(crate) struct NodeTrace {
    pub(crate) step: FlowStepTrace,
}

impl NodeTrace {
    /// The finished trace, or `None` when nothing was recorded.
    pub(crate) fn into_trace(self) -> Option<FlowStepTrace> {
        (self.step != FlowStepTrace::default()).then_some(self.step)
    }
}
```

- [ ] **Step 9: Run the unit tests**

Run: `cargo test -j4 -p rocket-app flow_trace`
Expected: PASS (4 tests). `cargo test -j4 -p rocket-app flow_debug` also passes (the exchange cap is unchanged).

- [ ] **Step 10: Write the failing engine tests for durations and the step value cap**

In `crates/rocket-app/src/flow_execution_service.rs`, inside `mod tests`, after `a_transform_reports_a_secret_masked_but_passes_it_on_raw` (ends near line 7848), add:

```rust
    #[tokio::test]
    async fn every_node_that_ran_reports_a_duration() {
        let summary = run_transform("tf-duration", Scripted::Value(serde_json::json!("PRO"))).await;

        for id in ["in", "t", "out"] {
            assert!(
                step_of(&summary, id).duration_ms.is_some(),
                "node {id} ran, so it has a duration"
            );
        }
    }

    #[tokio::test]
    async fn a_routing_node_has_a_duration_and_a_skipped_node_has_none() {
        let service = service_with_flow(if_flow("if-duration"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service
            .run(&exec, run_input("if-duration"))
            .await
            .expect("run");

        assert!(step_of(&summary, "check").duration_ms.is_some());
        assert_eq!(step_of(&summary, "no").status, FlowNodeStatus::Skipped);
        assert_eq!(step_of(&summary, "no").duration_ms, None);
    }

    #[tokio::test]
    async fn a_request_keeps_its_response_time_as_its_duration() {
        let service = service_with_flow(if_flow("if-response-time"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service
            .run(&exec, run_input("if-response-time"))
            .await
            .expect("run");

        // The run loop only fills a duration a node did not report itself.
        let login = step_of(&summary, "login");
        let exchange_ms = login
            .exchange
            .as_ref()
            .and_then(|e| e.response.as_ref())
            .map(|r| r.duration_ms);
        assert_eq!(login.duration_ms, exchange_ms);
    }

    #[tokio::test]
    async fn a_large_input_value_is_masked_before_it_is_capped() {
        let limit = crate::flow_trace::STEP_VALUE_LIMIT;
        let mut env = env_with(&[]);
        let mut key = rocket_environment::Variable::new("apiKey", "sk-live-123456");
        key.secret = true;
        env.set_variable(key);
        let exec = scoped_exec(env, Vec::new());
        // The secret straddles the cap. Cutting first would leave "sk-l".
        let text = format!("{}{{{{apiKey}}}}", "a".repeat(limit - 4));
        let flow = Flow {
            name: "big-input".to_string(),
            nodes: vec![input_node_with("in", &text)],
            edges: Vec::new(),
            callback_host: None,
        };
        let mut input = run_input("big-input");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_flow(flow)
            .run(&exec, input)
            .await
            .expect("run");

        let step = step_of(&summary, "in");
        let value = step.value.as_deref().expect("value");
        assert!(value.len() <= limit);
        assert!(!value.contains("sk-"), "half a secret leaked");
        assert!(step.trace.as_ref().is_some_and(|t| t.value_truncated));
    }
```

- [ ] **Step 11: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app duration`
Expected: FAIL to compile (`FlowStepResult` has no `trace` field) once the next step's field is referenced, or FAIL at runtime (`duration_ms` is `None` for Input, If, Transform and Output; the big value is not capped).

- [ ] **Step 12: Add `trace` to `FlowStepResult` and the event builder**

In `FlowStepResult` (line 687), replace

```rust
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
```

with:

```rust
    pub status_code: Option<u16>,
    /// How long the node ran. A Request reports its response time, a
    /// repeat-until poll its total and a callback wait its wait. `None` only
    /// for a node that never ran.
    pub duration_ms: Option<u64>,
```

and add after the `exchange` field (line 717):

```rust
    /// What the step saw on its wires and how it routed, masked and capped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<FlowStepTrace>,
```

Change the `use rocket_shared::events::{...}` at line 669 to:

```rust
use rocket_shared::events::{
    DomainEvent, FlowDebugRequest, FlowLogEntry, FlowLogLevel, FlowNodeStatus, FlowSkipReason,
    FlowStepTrace,
};
```

and add below it:

```rust
use crate::flow_trace::{mask_then_cap, NodeTrace, STEP_VALUE_LIMIT};
```

In `step_completed_event`, add after `attempts: step.attempts,`:

```rust
        trace: step.trace.clone().map(Box::new),
```

Add `trace: None,` after `exchange: None,` in the `base` literal of `result_to_step` (line 1586), in `skipped_step` (line 1659), in `failed_step` (line 1676) and in the test literal of `flow_step_result_serializes_skip_reason_camel_key_snake_value` (line 4495). In that same test, after `assert_eq!(back.branch, None);` (line 4522), add:

```rust
        assert_eq!(back.trace, None, "a summary step from before the trace still parses");
```

- [ ] **Step 13: Thread the trace through `execute_node` and time the call**

In the `execute_node` comment block above `#[allow(clippy::too_many_arguments)]` (lines 1172-1175), replace

```rust
    // `exchange` the capped exchange record and `poll_stats` how a failed
    // poll went.
```

with:

```rust
    // `exchange` the capped exchange record, `poll_stats` how a failed
    // poll went, and `trace` what the step saw on its wires.
```

and change the parameter list so that after `poll_stats: &mut Option<crate::flow_poll::FailedPollStats>,` it reads:

```rust
        poll_stats: &mut Option<crate::flow_poll::FailedPollStats>,
        trace: &mut NodeTrace,
        ctx: &mut NodeRunContext,
```

In the run loop (lines 1026-1078), replace

```rust
                    let mut node_poll_stats = None;
                    let mut ctx = NodeRunContext {
                        run_id: run_id.clone(),
                        node_id: node_id.clone(),
                        cancel: cancel_signal.clone(),
                    };
                    let result = match node_opt {
                        Some(node) => {
                            self.execute_node(
                                exec,
                                &input,
                                node,
                                &data_edges,
                                &captured,
                                &external_secrets,
                                &credentials,
                                &mut node_logs,
                                &mut node_debug,
                                &mut node_exchange,
                                &mut node_poll_stats,
                                &mut ctx,
                                &mut callbacks,
                            )
                            .await
                        }
```

with:

```rust
                    let mut node_poll_stats = None;
                    let mut node_trace = NodeTrace::default();
                    let mut ctx = NodeRunContext {
                        run_id: run_id.clone(),
                        node_id: node_id.clone(),
                        cancel: cancel_signal.clone(),
                    };
                    let started_at = std::time::Instant::now();
                    let result = match node_opt {
                        Some(node) => {
                            self.execute_node(
                                exec,
                                &input,
                                node,
                                &data_edges,
                                &captured,
                                &external_secrets,
                                &credentials,
                                &mut node_logs,
                                &mut node_debug,
                                &mut node_exchange,
                                &mut node_poll_stats,
                                &mut node_trace,
                                &mut ctx,
                                &mut callbacks,
                            )
                            .await
                        }
```

then, right after the `None => Err(...)` arm closes (`};` at line 1054), add:

```rust
                    let elapsed_ms =
                        u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
```

replace

```rust
                    let step = FlowStepResult {
                        logs: node_logs,
                        debug_request: node_debug,
                        exchange: node_exchange,
                        ..step
                    };
```

with:

```rust
                    let step = FlowStepResult {
                        logs: node_logs,
                        debug_request: node_debug,
                        exchange: node_exchange,
                        trace: node_trace.into_trace(),
                        ..step
                    };
```

and after the poll override `let step = match node_poll_stats { ... };` (ends at line 1078) add:

```rust
                    // Every node that ran has a duration. A Request keeps its
                    // response time and a poll or a wait keeps its own total.
                    let step = FlowStepResult {
                        duration_ms: step.duration_ms.or(Some(elapsed_ms)),
                        ..step
                    };
```

- [ ] **Step 14: Cap the reported step values**

In the Input arm, replace (lines 1213-1214)

```rust
                // Wires get the raw value. The step shows it with secrets masked.
                let reported = crate::redaction::redact_secrets(&resolved, &secret_values);
```

with:

```rust
                // Wires get the raw value. The step shows it masked, then capped.
                let (reported, cut) = mask_then_cap(&resolved, &secret_values, STEP_VALUE_LIMIT);
                trace.step.value_truncated = cut;
```

In the Output arm, replace (lines 1245-1248)

```rust
                // Wires get the raw value. The step shows every secret masked.
                let mut masked = secret_values.clone();
                masked.extend(credentials.secret_forms());
                let reported = crate::redaction::redact_secrets(&value, &masked);
```

with:

```rust
                // Wires get the raw value. The step shows every secret masked, then capped.
                let mut masked = secret_values.clone();
                masked.extend(credentials.secret_forms());
                let (reported, cut) = mask_then_cap(&value, &masked, STEP_VALUE_LIMIT);
                trace.step.value_truncated = cut;
```

In the Transform arm, replace (lines 1470-1471)

```rust
                // Wires get the raw text. The step shows it with secrets masked.
                let reported = crate::redaction::redact_secrets(value.data(), &secret_values);
```

with:

```rust
                // Wires get the raw text. The step shows it masked, then capped.
                let (reported, cut) = mask_then_cap(value.data(), &secret_values, STEP_VALUE_LIMIT);
                trace.step.value_truncated = cut;
```

- [ ] **Step 15: Run the engine tests**

Run: `cargo test -j4 -p rocket-app flow_execution_service`
Expected: PASS, including the four new tests and every existing test (no existing engine test asserts `duration_ms == None` for a node that ran).

- [ ] **Step 16: Gates and commit**

Run: `cargo check -j4 -p rocket-shared -p rocket-app && cargo clippy -j4 -p rocket-app && cargo check -j4 -p rocket`
Expected: no errors, and no new clippy warnings in the files this task touched. (`rocket` is the `src-tauri` package; it only re-checks that nothing there builds a `FlowStepCompleted` literal.)

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-shared/src/events.rs crates/rocket-app/src/flow_trace.rs crates/rocket-app/src/lib.rs crates/rocket-app/src/flow_debug.rs crates/rocket-app/src/flow_execution_service.rs`
Suggested subject: `feat(flow): report a duration and a trace carrier for every step`.

---

### Task 2: Record wire values and routing decisions

**Files:**
- Modify: `crates/rocket-app/src/flow_trace.rs` (limits, record helpers, unit tests)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (top of `execute_node` :1193-1199, Output arm :1220-1253, Request wire loop :1275-1307, If arm :1383-1412, Switch arm :1413-1437, Transform arm :1457-1476, new helpers after `single_input` :1557, tests)

**Interfaces:**
- Consumes: `NodeTrace`, `mask_then_cap`, `cap_text`, the `trace` out-param (Task 1).
- Produces (crate): `WIRE_VALUE_LIMIT = 16_384`, `WIRE_TOTAL_LIMIT = 65_536`, `ROUTE_VALUE_LIMIT = 1_024`, `NodeTrace::record_wire(&mut self, edge: &FlowEdge, raw: &str, masks: &HashSet<String>)`, `NodeTrace::record_credential_wire(&mut self, edge: &FlowEdge)`, `NodeTrace::record_route(&mut self, kind: &str, raw: &str, matched_case: Option<String>, masks: &HashSet<String>)`.
- Produces (behaviour): `trace.wires` for Output, Request, If, Switch and Transform steps; `trace.route` for If (`kind: "if"`) and Switch (`kind: "switch"`).

- [ ] **Step 1: Write the failing unit tests for the record helpers**

In `crates/rocket-app/src/flow_trace.rs`, inside `mod tests`, add:

```rust
    fn edge(id: &str, field: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: "src".to_string(),
            target_node_id: "dst".to_string(),
            target_field: field.to_string(),
            expression: "response.body".to_string(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        }
    }

    #[test]
    fn record_wire_masks_and_names_the_wire() {
        let mut trace = NodeTrace::default();
        trace.record_wire(&edge("e1", "body"), "key=sk-live-123456", &masks(&["sk-live-123456"]));
        let wire = &trace.step.wires[0];
        assert_eq!(wire.edge_id, "e1");
        assert_eq!(wire.source_node_id, "src");
        assert_eq!(wire.target_field, "body");
        assert_eq!(wire.value.as_deref(), Some(format!("key={REDACTED}").as_str()));
        assert!(!wire.truncated && !wire.credential && wire.error.is_none());
    }

    #[test]
    fn record_wire_cuts_one_value_at_the_wire_limit() {
        let mut trace = NodeTrace::default();
        trace.record_wire(&edge("e1", "body"), &"x".repeat(WIRE_VALUE_LIMIT + 1), &masks(&[]));
        let wire = &trace.step.wires[0];
        assert!(wire.truncated);
        assert_eq!(wire.value.as_ref().map(String::len), Some(WIRE_VALUE_LIMIT));
    }

    #[test]
    fn record_wire_stops_at_the_step_total() {
        let mut trace = NodeTrace::default();
        let big = "x".repeat(WIRE_VALUE_LIMIT);
        for i in 0..5 {
            trace.record_wire(&edge(&format!("e{i}"), "body"), &big, &masks(&[]));
        }
        let total: usize = trace
            .step
            .wires
            .iter()
            .filter_map(|w| w.value.as_ref())
            .map(String::len)
            .sum();
        assert_eq!(total, WIRE_TOTAL_LIMIT);
        assert_eq!(trace.step.wires.len(), 5, "every wire is still listed");
        assert!(trace.step.wires[4].truncated);
        assert_eq!(trace.step.wires[4].value.as_deref(), Some(""));
    }

    #[test]
    fn a_credential_wire_has_no_value() {
        let mut trace = NodeTrace::default();
        trace.record_credential_wire(&edge("ea", rocket_flow::handle::AUTH));
        let wire = &trace.step.wires[0];
        assert!(wire.credential);
        assert_eq!(wire.value, None);
    }

    #[test]
    fn record_route_masks_and_caps_the_value() {
        let mut trace = NodeTrace::default();
        let raw = format!("sk-live-123456{}", "y".repeat(ROUTE_VALUE_LIMIT));
        trace.record_route("switch", &raw, Some("c1".into()), &masks(&["sk-live-123456"]));
        let route = trace.step.route.expect("route");
        assert_eq!(route.kind, "switch");
        assert!(route.value.starts_with(REDACTED));
        assert!(route.value.len() <= ROUTE_VALUE_LIMIT);
        assert_eq!(route.matched_case.as_deref(), Some("c1"));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_trace`
Expected: FAIL to compile (the record helpers and limits do not exist).

- [ ] **Step 3: Implement the record helpers**

In `crates/rocket-app/src/flow_trace.rs`, change the imports to:

```rust
use std::collections::HashSet;

use rocket_flow::FlowEdge;
use rocket_shared::events::{FlowRouteEval, FlowStepTrace, FlowWireValue};

use crate::flow_debug::cap_text;
use crate::redaction::redact_secrets;
```

add below `STEP_VALUE_LIMIT`:

```rust
/// The largest value one wire record keeps, in bytes.
pub(crate) const WIRE_VALUE_LIMIT: usize = 16_384;
/// The most wire value bytes one step keeps over all its wires.
pub(crate) const WIRE_TOTAL_LIMIT: usize = 65_536;
/// The largest Switch value a route record keeps, in bytes.
pub(crate) const ROUTE_VALUE_LIMIT: usize = 1_024;
```

replace the `NodeTrace` struct with:

```rust
/// The trace of the node that is running. `execute_node` fills it as it goes,
/// so what was recorded before an error still reaches the step.
#[derive(Debug, Default)]
pub(crate) struct NodeTrace {
    pub(crate) step: FlowStepTrace,
    /// Wire value bytes recorded so far, for `WIRE_TOTAL_LIMIT`.
    wire_bytes: usize,
}
```

and add to `impl NodeTrace`:

```rust
    /// Records the value `edge` delivered, masked, then cut to the wire limit
    /// and to what is left of the step total.
    pub(crate) fn record_wire(&mut self, edge: &FlowEdge, raw: &str, masks: &HashSet<String>) {
        let (mut value, mut truncated) = mask_then_cap(raw, masks, WIRE_VALUE_LIMIT);
        let room = WIRE_TOTAL_LIMIT.saturating_sub(self.wire_bytes);
        if cap_text(&mut value, room) {
            truncated = true;
        }
        self.wire_bytes += value.len();
        self.step.wires.push(FlowWireValue {
            value: Some(value),
            truncated,
            ..blank_wire(edge)
        });
    }

    /// Records an `auth` wire. Its credential is never read or recorded.
    pub(crate) fn record_credential_wire(&mut self, edge: &FlowEdge) {
        self.step.wires.push(FlowWireValue {
            credential: true,
            ..blank_wire(edge)
        });
    }

    /// Records how a routing node decided. `raw` is masked, then capped.
    pub(crate) fn record_route(
        &mut self,
        kind: &str,
        raw: &str,
        matched_case: Option<String>,
        masks: &HashSet<String>,
    ) {
        let (value, _) = mask_then_cap(raw, masks, ROUTE_VALUE_LIMIT);
        self.step.route = Some(FlowRouteEval {
            kind: kind.to_string(),
            value,
            matched_case,
        });
    }
```

and after the `impl` block:

```rust
/// A wire record that names `edge` and holds nothing else yet.
fn blank_wire(edge: &FlowEdge) -> FlowWireValue {
    FlowWireValue {
        edge_id: edge.id.clone(),
        source_node_id: edge.source_node_id.clone(),
        target_field: edge.target_field.clone(),
        ..Default::default()
    }
}
```

- [ ] **Step 4: Run the unit tests**

Run: `cargo test -j4 -p rocket-app flow_trace`
Expected: PASS (9 tests).

- [ ] **Step 5: Write the failing engine tests**

In `crates/rocket-app/src/flow_execution_service.rs`, inside `mod tests`, after the tests added in Task 1, add:

```rust
    #[tokio::test]
    async fn an_input_wired_into_an_output_records_the_wire_masked() {
        let mut env = env_with(&[]);
        let mut key = rocket_environment::Variable::new("apiKey", "sk-live-123456");
        key.secret = true;
        env.set_variable(key);
        let exec = scoped_exec(env, Vec::new());
        let flow = Flow {
            name: "scope".to_string(),
            nodes: vec![
                input_node_with("in", "{{apiKey}}"),
                output_node_named("out"),
            ],
            edges: vec![FlowEdge {
                target_field: "value".to_string(),
                ..wire("e1", "in", "out")
            }],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let mut input = run_input("scope");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_publisher(flow, &publisher)
            .run(&exec, input)
            .await
            .expect("run");

        let trace = step_of(&summary, "out").trace.clone().expect("a trace");
        assert_eq!(trace.wires.len(), 1);
        let recorded = &trace.wires[0];
        assert_eq!(recorded.edge_id, "e1");
        assert_eq!(recorded.source_node_id, "in");
        assert_eq!(recorded.target_field, "value");
        assert_eq!(recorded.value.as_deref(), Some(crate::redaction::REDACTED));
        let event_trace = publisher.events().iter().find_map(|e| match e {
            DomainEvent::FlowStepCompleted { node_id, trace, .. } if node_id == "out" => {
                trace.clone()
            }
            _ => None,
        });
        assert_eq!(event_trace.map(|t| *t), Some(trace));
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(!json.contains("sk-live-123456"), "{json}");
    }

    #[tokio::test]
    async fn an_auth_token_wired_into_a_header_is_recorded_masked() {
        use rocket_shared::types::Auth;

        let flow = Flow {
            name: "auth-header".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: FlowNodeKind::Auth {
                        label: "Sign in".to_string(),
                        auth: Auth::Bearer {
                            token: "static-token-123456".to_string(),
                        },
                        apply_to_inherit: false,
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                request_flow_node("r", "https://api.example.com/me"),
            ],
            // X-Token is not a sensitive header, so only secret masking hides it.
            edges: vec![edge_from(
                "e1",
                "a",
                handle::RESULT,
                "r",
                "headers[X-Token].value",
                "response.body",
            )],
            callback_host: None,
        };
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("static-token-123456"));

        let summary = service_with_flow(flow)
            .run(&exec, run_input("auth-header"))
            .await
            .expect("run");

        let trace = step_of(&summary, "r").trace.clone().expect("a trace");
        assert_eq!(
            trace.wires[0].value.as_deref(),
            Some(crate::redaction::REDACTED)
        );
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(!json.contains("static-token-123456"), "{json}");
    }

    #[tokio::test]
    async fn an_auth_wire_is_recorded_as_a_credential_without_a_value() {
        use rocket_shared::events::FlowWireValue;
        use rocket_shared::types::Auth;

        let executor = crate::test_doubles::RecordingExecutor::new();
        let exec = recording_http_exec(&executor);
        let service = service_with_saved_request(
            auth_and_request_flow(false, vec![auth_wire()]),
            Auth::Inherit,
        );

        let summary = service
            .run(&exec, run_input("auth-req"))
            .await
            .expect("run must succeed");

        let trace = step_of(&summary, "r").trace.clone().expect("a trace");
        assert_eq!(
            trace.wires,
            vec![FlowWireValue {
                edge_id: "e1".to_string(),
                source_node_id: "a".to_string(),
                target_field: handle::AUTH.to_string(),
                credential: true,
                ..Default::default()
            }]
        );
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(!json.contains("flow-token-123456"), "{json}");
    }

    #[tokio::test]
    async fn an_oversized_wire_value_is_cut_and_flagged() {
        let limit = crate::flow_trace::WIRE_VALUE_LIMIT;
        let big = "x".repeat(limit + 100);
        let flow = Flow {
            name: "big-wire".to_string(),
            nodes: vec![input_node_with("in", &big), output_node_named("out")],
            edges: vec![FlowEdge {
                target_field: "value".to_string(),
                ..wire("e1", "in", "out")
            }],
            callback_host: None,
        };
        let exec = scoped_exec(env_with(&[]), Vec::new());

        let summary = service_with_flow(flow)
            .run(&exec, run_input("big-wire"))
            .await
            .expect("run");

        let out = step_of(&summary, "out");
        let recorded = &out.trace.as_ref().expect("a trace").wires[0];
        assert!(recorded.truncated);
        assert_eq!(recorded.value.as_ref().map(String::len), Some(limit));
        // The step value has its own, larger limit.
        assert_eq!(out.value.as_ref().map(String::len), Some(big.len()));
    }

    #[tokio::test]
    async fn an_if_records_its_condition_result_and_its_input_wire() {
        use rocket_shared::events::FlowRouteEval;

        let service = service_with_flow(if_flow("if-route"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("if-route")).await.expect("run");

        let trace = step_of(&summary, "check").trace.clone().expect("a trace");
        assert_eq!(
            trace.route,
            Some(FlowRouteEval {
                kind: "if".to_string(),
                value: "true".to_string(),
                matched_case: None,
            })
        );
        assert_eq!(trace.wires.len(), 1, "the input wire is recorded once");
        assert_eq!(trace.wires[0].edge_id, "e1");
        assert!(trace.wires[0].value.is_some());
    }

    #[tokio::test]
    async fn a_switch_records_its_value_masked_and_the_matched_case() {
        use rocket_shared::events::FlowRouteEval;

        let mut env = env_with(&[]);
        let mut key = rocket_environment::Variable::new("apiKey", "sk-live-123456");
        key.secret = true;
        env.set_variable(key);
        let exec = scoped_exec(env, Vec::new());
        let flow = Flow {
            name: "sw-secret".to_string(),
            nodes: vec![
                input_node_with("in", "{{apiKey}}"),
                switch_node(
                    "route",
                    "response.body",
                    &[("c1", "sk-live-123456"), ("c2", "other")],
                ),
            ],
            edges: vec![input_edge("e1", "in", "route")],
            callback_host: None,
        };
        let mut input = run_input("sw-secret");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_flow(flow)
            .run(&exec, input)
            .await
            .expect("run");

        let step = step_of(&summary, "route");
        assert_eq!(step.branch.as_deref(), Some("case:c1"));
        let trace = step.trace.clone().expect("a trace");
        assert_eq!(
            trace.route,
            Some(FlowRouteEval {
                kind: "switch".to_string(),
                value: crate::redaction::REDACTED.to_string(),
                matched_case: Some("c1".to_string()),
            })
        );
        assert_eq!(
            trace.wires[0].value.as_deref(),
            Some(crate::redaction::REDACTED)
        );
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(!json.contains("sk-live-123456"), "{json}");
    }

    #[tokio::test]
    async fn a_switch_that_takes_the_default_exit_has_no_matched_case() {
        let flow = Flow {
            name: "sw-default".to_string(),
            nodes: vec![
                input_node_with("in", "pro"),
                switch_node("route", "response.body", &[("c1", "free")]),
            ],
            edges: vec![input_edge("e1", "in", "route")],
            callback_host: None,
        };
        let exec = scoped_exec(env_with(&[]), Vec::new());

        let summary = service_with_flow(flow)
            .run(&exec, run_input("sw-default"))
            .await
            .expect("run");

        let route = step_of(&summary, "route")
            .trace
            .clone()
            .and_then(|t| t.route)
            .expect("a route");
        assert_eq!(route.value, "pro");
        assert_eq!(route.matched_case, None);
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app records`
Expected: FAIL (`trace` is `None` or has no wires and no route).

- [ ] **Step 7: Build the trace mask set and helpers**

At the top of `execute_node`, after the `let secret_values = exec.secret_values(...);` statement (ends at line 1199), add:

```rust
        // Trace values are masked like an Output value: every secret variable
        // and every form of every Auth-node credential. The Request arm
        // rebinds `secret_values` later, so this set is built first.
        let mut trace_masks = secret_values.clone();
        trace_masks.extend(credentials.secret_forms());
```

After `single_input` (ends at line 1557), add:

```rust
/// The text a node captured, as a single-input node received it: a
/// response body or a value.
fn captured_text(output: &CapturedOutput) -> &str {
    match output {
        CapturedOutput::Request(out) => &out.response.body,
        CapturedOutput::Value(value) => value.data(),
    }
}

/// Records the one `input` wire of an If, Switch or Transform node with what
/// its source captured. Nothing is evaluated again.
fn record_input_wire(
    trace: &mut NodeTrace,
    data_edges: &[&FlowEdge],
    source: &CapturedOutput,
    masks: &HashSet<String>,
) {
    if let [edge] = data_edges {
        trace.record_wire(edge, captured_text(source), masks);
    }
}
```

- [ ] **Step 8: Record wires in the Output and Request arms**

In the Output arm, replace (from line 1244)

```rust
                let value = outcome.result?;
                // Wires get the raw value. The step shows every secret masked, then capped.
                let mut masked = secret_values.clone();
                masked.extend(credentials.secret_forms());
                let (reported, cut) = mask_then_cap(&value, &masked, STEP_VALUE_LIMIT);
                trace.step.value_truncated = cut;
```

with:

```rust
                let value = outcome.result?;
                trace.record_wire(edge, &value, &trace_masks);
                // Wires get the raw value. The step shows every secret masked, then capped.
                let (reported, cut) = mask_then_cap(&value, &trace_masks, STEP_VALUE_LIMIT);
                trace.step.value_truncated = cut;
```

In the Request arm's wire loop, replace

```rust
                    if edge.target_field == handle::AUTH {
                        request_input.auth = credentials
```

with:

```rust
                    if edge.target_field == handle::AUTH {
                        // Only the fact that a credential arrived is recorded.
                        trace.record_credential_wire(edge);
                        request_input.auth = credentials
```

and replace

```rust
                    let value = outcome.result?;
                    resolved.insert(edge.id.clone(), value);
```

with:

```rust
                    let value = outcome.result?;
                    trace.record_wire(edge, &value, &trace_masks);
                    resolved.insert(edge.id.clone(), value);
```

- [ ] **Step 9: Record the input wire and the route in If, Switch and Transform**

In the If arm, replace

```rust
                let source = single_input(node, data_edges, captured)?;
                let outcome = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        condition,
                        FlowCoercion::Bool,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let raw = outcome.result?;
```

with:

```rust
                let source = single_input(node, data_edges, captured)?;
                record_input_wire(trace, data_edges, source, &trace_masks);
                let outcome = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        condition,
                        FlowCoercion::Bool,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let raw = outcome.result?;
                // Recorded before the check, so a non-boolean result shows too.
                trace.record_route("if", &raw, None, &trace_masks);
```

In the Switch arm, replace

```rust
                let source = single_input(node, data_edges, captured)?;
                let outcome = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        value,
                        FlowCoercion::Str,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let raw = outcome.result?;
                let chosen_exit = cases
                    .iter()
                    .find(|case| case.matches == raw)
                    .map(|case| handle::case_handle(&case.id))
                    .unwrap_or_else(|| handle::DEFAULT.to_string());
```

with:

```rust
                let source = single_input(node, data_edges, captured)?;
                record_input_wire(trace, data_edges, source, &trace_masks);
                let outcome = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        value,
                        FlowCoercion::Str,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let raw = outcome.result?;
                let matched = cases.iter().find(|case| case.matches == raw);
                trace.record_route(
                    "switch",
                    &raw,
                    matched.map(|case| case.id.clone()),
                    &trace_masks,
                );
                let chosen_exit = matched
                    .map(|case| handle::case_handle(&case.id))
                    .unwrap_or_else(|| handle::DEFAULT.to_string());
```

In the Transform arm, replace

```rust
            FlowNodeKind::Transform { script, .. } => {
                let source = single_input(node, data_edges, captured)?;
```

with:

```rust
            FlowNodeKind::Transform { script, .. } => {
                let source = single_input(node, data_edges, captured)?;
                record_input_wire(trace, data_edges, source, &trace_masks);
```

- [ ] **Step 10: Run the tests**

Run: `cargo test -j4 -p rocket-app flow_execution_service`
Expected: PASS, including the 7 new tests and every existing test.

- [ ] **Step 11: Gates and commit**

Run: `cargo check -j4 -p rocket-app && cargo clippy -j4 -p rocket-app`
Expected: no errors, and no new clippy warnings in the files this task touched.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-app/src/flow_trace.rs crates/rocket-app/src/flow_execution_service.rs`
Suggested subject: `feat(flow): record masked wire values and routing decisions per step`.

---

### Task 3: Edge-named wire errors and `failed_edge_id`

**Files:**
- Modify: `crates/rocket-app/src/flow_trace.rs` (`record_failure`, `wire_err`, unit tests)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`apply_wired_overrides` :555-582 and its two helpers :585-664, Output arm :1226-1244, Request wire loop :1293-1307, tests :4576-4620, :4932-4959, :2595-2608, new tests)
- Modify: `crates/rocket-app/src/flow_routing.rs:97-121` and its test at `:343-352`
- Modify: `crates/rocket-app/CLAUDE.md` (Flow Auth nodes section, end of file)

**Interfaces:**
- Consumes: `NodeTrace`, `record_wire` (Task 2).
- Produces (crate): `NodeTrace::record_failure(&mut self, edge: &FlowEdge, message: &str, masks: &HashSet<String>)`, `flow_trace::wire_err(edge: &FlowEdge, e: DomainError) -> DomainError`.
- Produces (messages): a failed wire reads `wire '<edge id>' (<source>.<source handle> -> <target>.<target field>): <cause>`, keeping the `DomainError` variant for `InvalidInput` and `Internal`. `field 'url' has 2 live inputs (edges e1, e2)`. `output node 'out' has more than one incoming wire (edges e1, e2)`.

- [ ] **Step 1: Write the failing unit tests**

In `crates/rocket-app/src/flow_trace.rs`, inside `mod tests`, add:

```rust
    #[test]
    fn wire_err_names_the_wire_and_keeps_the_variant() {
        use rocket_shared::error::DomainError;

        let e = edge("e7", "headers[X-Id].value");
        let named = wire_err(&e, DomainError::InvalidInput("boom".into()));
        assert_eq!(
            named,
            DomainError::InvalidInput(
                "wire 'e7' (src.result -> dst.headers[X-Id].value): boom".into()
            )
        );
        assert!(matches!(
            wire_err(&e, DomainError::Internal("x".into())),
            DomainError::Internal(_)
        ));
    }

    #[test]
    fn record_failure_marks_the_wire_and_the_step() {
        let mut trace = NodeTrace::default();
        trace.record_failure(&edge("e1", "url"), "token sk-live-123456 failed", &masks(&["sk-live-123456"]));
        assert_eq!(trace.step.failed_edge_id.as_deref(), Some("e1"));
        let wire = &trace.step.wires[0];
        assert_eq!(wire.value, None);
        assert_eq!(wire.error.as_deref(), Some(format!("token {REDACTED} failed").as_str()));
    }

    #[test]
    fn record_failure_on_a_recorded_wire_keeps_its_value() {
        let mut trace = NodeTrace::default();
        let e = edge("e1", "headers[3].value");
        trace.record_wire(&e, "abcdef-value", &masks(&[]));
        trace.record_failure(&e, "header index 3 out of range", &masks(&[]));
        assert_eq!(trace.step.wires.len(), 1, "one row per wire");
        assert_eq!(trace.step.wires[0].value.as_deref(), Some("abcdef-value"));
        assert!(trace.step.wires[0].error.is_some());
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_trace`
Expected: FAIL to compile (`wire_err`, `record_failure` do not exist).

- [ ] **Step 3: Implement `wire_err` and `record_failure`**

In `crates/rocket-app/src/flow_trace.rs`, add `use rocket_shared::error::DomainError;` to the imports, add to `impl NodeTrace`:

```rust
    /// Marks `edge` as the wire that failed the step. The message is masked
    /// and capped. A wire recorded earlier keeps its value.
    pub(crate) fn record_failure(&mut self, edge: &FlowEdge, message: &str, masks: &HashSet<String>) {
        let (error, _) = mask_then_cap(message, masks, WIRE_VALUE_LIMIT);
        match self.step.wires.iter_mut().find(|w| w.edge_id == edge.id) {
            Some(wire) => wire.error = Some(error),
            None => self.step.wires.push(FlowWireValue {
                error: Some(error),
                ..blank_wire(edge)
            }),
        }
        self.step.failed_edge_id = Some(edge.id.clone());
    }
```

and after `blank_wire`:

```rust
/// Names the wire in an error, as
/// `wire '<id>' (<source>.<exit> -> <target>.<field>): <cause>`. It never
/// adds a value. `InvalidInput` and `Internal` keep their variant.
pub(crate) fn wire_err(edge: &FlowEdge, e: DomainError) -> DomainError {
    let context = format!(
        "wire '{}' ({}.{} -> {}.{})",
        edge.id, edge.source_node_id, edge.source_handle, edge.target_node_id, edge.target_field
    );
    match e {
        DomainError::Internal(m) => DomainError::Internal(format!("{context}: {m}")),
        DomainError::InvalidInput(m) => DomainError::InvalidInput(format!("{context}: {m}")),
        other => DomainError::InvalidInput(format!("{context}: {other}")),
    }
}
```

Run: `cargo test -j4 -p rocket-app flow_trace`
Expected: PASS.

- [ ] **Step 4: Write the failing engine and routing tests**

In `crates/rocket-app/src/flow_routing.rs`, change the assertion of `two_live_edges_into_one_data_field_fail_the_node` (line 350) to:

```rust
            NodeFate::Fail("field 'body' has 2 live inputs (edges e1, e2)".to_string())
```

In `crates/rocket-app/src/flow_execution_service.rs`:

1. In `two_unconditional_wires_into_one_field_now_fail_the_target` (line 4957), change the assertion to:

```rust
        assert_eq!(
            c.error.as_deref(),
            Some("field 'url' has 2 live inputs (edges e1, e2)")
        );
```

2. In `output_node_with_two_incoming_wires_fails_instead_of_dropping_one`, after `assert_eq!(status_of(&summary, "out"), FlowNodeStatus::Failed);` (line 4619), add:

```rust
        assert!(
            step_of(&summary, "out")
                .error
                .as_deref()
                .is_some_and(|e| e.contains("(edges e1, e2)")),
            "the error lists both wires"
        );
```

3. In `body_override_into_form_body_is_an_error_not_a_silent_noop`, replace `assert!(matches!(err, DomainError::InvalidInput(_)));` (line 2607) with:

```rust
        assert!(
            matches!(&err, DomainError::InvalidInput(m)
                if m.starts_with("wire 'e1' (src.result -> n2.body): ")),
            "got {err}"
        );
```

4. In `wire_expression_failure_fails_the_node_and_skips_its_dependents`, after the existing `ReferenceError` assertion (ends at line 4565), add:

```rust
        let error = b.error.as_deref().unwrap_or_default();
        assert!(error.contains("wire 'e1' (a.result -> b.url)"), "got {error}");
        let trace = b.trace.clone().expect("a trace");
        assert_eq!(trace.failed_edge_id.as_deref(), Some("e1"));
        assert_eq!(trace.wires[0].value, None);
        assert!(trace.wires[0]
            .error
            .as_deref()
            .is_some_and(|e| e.contains("ReferenceError")));
```

5. After the Task 2 tests, add:

```rust
    #[tokio::test]
    async fn an_earlier_wire_keeps_its_value_when_a_later_wire_fails() {
        let flow = Flow {
            name: "two-wires".to_string(),
            nodes: vec![
                input_node_with("in", "hello"),
                request_flow_node("r", "https://api.example.com/r"),
            ],
            edges: vec![
                edge_from("e1", "in", handle::RESULT, "r", "body", "response.body"),
                edge_from("e2", "in", handle::RESULT, "r", "headers[X-Id].value", "boom()"),
            ],
            callback_host: None,
        };
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![
                ("boom()", Scripted::Throw("ReferenceError: boom is not defined")),
                ("response.body", Scripted::Value(serde_json::json!("hello"))),
            ]),
        );

        let summary = service_with_flow(flow)
            .run(&exec, run_input("two-wires"))
            .await
            .expect("run");

        let r = step_of(&summary, "r");
        assert_eq!(r.status, FlowNodeStatus::Failed);
        let trace = r.trace.clone().expect("a trace");
        assert_eq!(trace.failed_edge_id.as_deref(), Some("e2"));
        assert_eq!(trace.wires.len(), 2);
        assert_eq!(trace.wires[0].edge_id, "e1");
        assert_eq!(trace.wires[0].value.as_deref(), Some("hello"));
        assert_eq!(trace.wires[1].edge_id, "e2");
        assert!(trace.wires[1].error.is_some());
        assert!(executor.sent_urls().is_empty(), "r must not be sent");
    }

    #[tokio::test]
    async fn a_wire_error_never_quotes_the_resolved_value() {
        let mut env = env_with(&[]);
        let mut key = rocket_environment::Variable::new("apiKey", "sk-live-123456");
        key.secret = true;
        env.set_variable(key);
        let exec = scoped_exec(env, Vec::new());
        // The request has no headers, so index 3 is out of range.
        let flow = Flow {
            name: "bad-header".to_string(),
            nodes: vec![
                input_node_with("in", "{{apiKey}}"),
                request_flow_node("r", "https://api.example.com/r"),
            ],
            edges: vec![edge_from(
                "e1",
                "in",
                handle::RESULT,
                "r",
                "headers[3].value",
                "response.body",
            )],
            callback_host: None,
        };
        let mut input = run_input("bad-header");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_flow(flow)
            .run(&exec, input)
            .await
            .expect("run");

        let r = step_of(&summary, "r");
        let error = r.error.as_deref().expect("an error");
        assert!(error.contains("wire 'e1' (in.result -> r.headers[3].value)"), "got {error}");
        assert!(!error.contains("sk-live-123456"), "got {error}");
        let trace = r.trace.clone().expect("a trace");
        assert_eq!(trace.failed_edge_id.as_deref(), Some("e1"));
        assert_eq!(
            trace.wires[0].value.as_deref(),
            Some(crate::redaction::REDACTED)
        );
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(!json.contains("sk-live-123456"), "{json}");
    }
```

- [ ] **Step 5: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app wire`
Expected: the new and changed assertions FAIL (no edge names, no `failed_edge_id`). `cargo test -j4 -p rocket-app flow_routing` FAILS on the new message.

- [ ] **Step 6: List edge ids in the routing failure**

In `crates/rocket-app/src/flow_routing.rs`, replace (lines 97-121)

```rust
    let mut data_edges = Vec::new();
    let mut ambiguous: Option<(&str, usize)> = None;
```

with:

```rust
    let mut data_edges = Vec::new();
    let mut ambiguous: Option<(&str, Vec<&'a str>)> = None;
```

replace

```rust
        if live.len() > 1 && ambiguous.is_none() {
            ambiguous = Some((*field, live.len()));
        }
```

with:

```rust
        if live.len() > 1 && ambiguous.is_none() {
            ambiguous = Some((*field, live.iter().map(|e| e.id.as_str()).collect()));
        }
```

and replace

```rust
    if let Some((field, count)) = ambiguous {
        return NodeFate::Fail(format!("field '{field}' has {count} live inputs"));
    }
```

with:

```rust
    if let Some((field, ids)) = ambiguous {
        return NodeFate::Fail(format!(
            "field '{field}' has {} live inputs (edges {})",
            ids.len(),
            ids.join(", ")
        ));
    }
```

- [ ] **Step 7: Name the wire in `apply_wired_overrides`**

In `crates/rocket-app/src/flow_execution_service.rs`, replace the body of `apply_wired_overrides` (lines 560-581) with:

```rust
    for e in edges {
        let Some(value) = resolved.get(&e.id) else {
            continue;
        };
        let applied = match e.target_field.as_str() {
            "url" => {
                input.url = value.clone();
                Ok(())
            }
            "body" => apply_body_override(input, value),
            field => match field
                .strip_prefix("headers[")
                .and_then(|rest| rest.strip_suffix("].value"))
            {
                Some(selector) => apply_header_override(input, field, selector, value),
                None => Err(DomainError::InvalidInput(format!(
                    "unrecognized target_field '{field}'"
                ))),
            },
        };
        applied.map_err(|err| crate::flow_trace::wire_err(e, err))?;
    }
    Ok(())
```

Change `apply_body_override` to `fn apply_body_override(input: &mut ExecuteRequestInput, value: &str) -> DomainResult<()>` and its error to:

```rust
            return Err(DomainError::InvalidInput(format!(
                "cannot wire a value into a {:?} body",
                body.mode
            )));
```

Change `apply_header_override` to `fn apply_header_override(input: &mut ExecuteRequestInput, field: &str, selector: &str, value: &str) -> DomainResult<()>` and its three errors to:

```rust
        return Err(DomainError::InvalidInput(format!(
            "malformed target_field '{field}'"
        )));
```

```rust
        let index: usize = selector.parse().map_err(|_| {
            DomainError::InvalidInput(format!("malformed target_field '{field}'"))
        })?;
```

```rust
        let header = input.headers.get_mut(index).ok_or_else(|| {
            DomainError::InvalidInput(format!(
                "header index {index} out of range (request has {headers_len} headers)"
            ))
        })?;
```

The wrapper now adds the edge id, its source and its target field, so the helpers drop their own `edge '<id>':` prefix. Update the doc comment of `apply_wired_overrides` by adding this line before `pub fn`:

```rust
/// Every error names its wire (see `flow_trace::wire_err`) and never the value.
```

- [ ] **Step 8: Name the wire in the Output and Request arms**

In the Output arm, replace

```rust
                if data_edges.len() > 1 {
                    return Err(DomainError::InvalidInput(format!(
                        "output node '{}' has more than one incoming wire",
                        node.id
                    )));
                }
                let source_output = captured_source(node, edge, captured)?;
```

with:

```rust
                if data_edges.len() > 1 {
                    let ids: Vec<&str> = data_edges.iter().map(|e| e.id.as_str()).collect();
                    return Err(DomainError::InvalidInput(format!(
                        "output node '{}' has more than one incoming wire (edges {})",
                        node.id,
                        ids.join(", ")
                    )));
                }
                let source_output = match captured_source(node, edge, captured) {
                    Ok(output) => output,
                    Err(e) => return Err(fail_wire(trace, edge, e, &trace_masks)),
                };
```

and replace

```rust
                logs.extend(outcome.logs);
                let value = outcome.result?;
                trace.record_wire(edge, &value, &trace_masks);
```

(in the Output arm) with:

```rust
                logs.extend(outcome.logs);
                let value = match outcome.result {
                    Ok(value) => value,
                    Err(e) => return Err(fail_wire(trace, edge, e, &trace_masks)),
                };
                trace.record_wire(edge, &value, &trace_masks);
```

In the Request arm's wire loop, replace

```rust
                    let source_output = captured_source(node, edge, captured)?;
```

with:

```rust
                    let source_output = match captured_source(node, edge, captured) {
                        Ok(output) => output,
                        Err(e) => return Err(fail_wire(trace, edge, e, &trace_masks)),
                    };
```

replace

```rust
                    logs.extend(outcome.logs);
                    let value = outcome.result?;
                    trace.record_wire(edge, &value, &trace_masks);
                    resolved.insert(edge.id.clone(), value);
                }
                let edges_owned: Vec<FlowEdge> = data_edges.iter().map(|e| (*e).clone()).collect();
                apply_wired_overrides(&mut request_input, &resolved, &edges_owned)?;
```

with:

```rust
                    logs.extend(outcome.logs);
                    let value = match outcome.result {
                        Ok(value) => value,
                        Err(e) => return Err(fail_wire(trace, edge, e, &trace_masks)),
                    };
                    trace.record_wire(edge, &value, &trace_masks);
                    resolved.insert(edge.id.clone(), value);
                }
                // One wire at a time, so a failure marks the wire it came from.
                for edge in data_edges {
                    if let Err(err) = apply_wired_overrides(
                        &mut request_input,
                        &resolved,
                        std::slice::from_ref(*edge),
                    ) {
                        trace.record_failure(edge, &err.to_string(), &trace_masks);
                        return Err(err);
                    }
                }
```

After `record_input_wire` (added in Task 2), add:

```rust
/// Marks `edge` as failed in the trace and returns its error, named after
/// the wire.
fn fail_wire(
    trace: &mut NodeTrace,
    edge: &FlowEdge,
    e: DomainError,
    masks: &HashSet<String>,
) -> DomainError {
    trace.record_failure(edge, &e.to_string(), masks);
    crate::flow_trace::wire_err(edge, e)
}
```

- [ ] **Step 9: Run the tests**

Run: `cargo test -j4 -p rocket-app flow_`
Expected: PASS (flow_routing, flow_trace, flow_execution_service, flow_debug, flow_poll, flow_wait, flow_callbacks).

- [ ] **Step 10: Document the trace for the crate**

In `crates/rocket-app/CLAUDE.md`, append at the end of the file:

```markdown
## Flow step trace (`flow_trace.rs`)

`execute_node` fills a `NodeTrace` out-param; the run loop moves it into
`FlowStepResult.trace` and the `FlowStepCompleted` event. It records each data
wire's value (`record_wire`), an `auth` wire as `credential: true` with no
value, an If/Switch decision (`record_route`) and the failing wire
(`record_failure`, `failed_edge_id`). Values are masked with `secret_values`
plus `credentials.secret_forms()` first and capped second (16 KB per wire,
64 KB per step, 1 KB per route value, 256 KB per step value). Wire errors are
named by `wire_err` and never quote a resolved value. The run loop fills
`duration_ms` for every node that ran and did not set its own.
```

- [ ] **Step 11: Gates and commit**

Run: `cargo check -j4 -p rocket-app && cargo clippy -j4 -p rocket-app && cargo check -j4 -p rocket`
Expected: no errors, and no new clippy warnings in the files this task touched.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-app/src/flow_trace.rs crates/rocket-app/src/flow_execution_service.rs crates/rocket-app/src/flow_routing.rs crates/rocket-app/CLAUDE.md`
Suggested subject: `feat(flow): name the failing wire in step errors and traces`.

---

## Self-Review

- **Spec coverage:** F-32 durations for every node that ran (Task 1, run loop) and the 256 KB step value cap (Task 1). F-39 wire values for Output, Request and single-input nodes, masked with the Output set and capped per wire and per step (Task 2); `auth` wire as a credential (Task 2); edge-named errors at the Output, Request and `captured_source` sites plus `apply_wired_overrides`, and `failed_edge_id` (Task 3); "field has N live inputs" and "more than one incoming wire" list edge ids (Task 3). F-38 If records `"true"`/`"false"` with the `!!(` coercion untouched; Switch records the masked, 1 KB capped value and the matched case or `None` (Task 2).
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `FlowStepTrace`, `FlowWireValue`, `FlowRouteEval` and `NodeTrace` keep the same fields in all tasks. P8 reads `trace.wires[].{edgeId, sourceNodeId, targetField, value, truncated, credential, error}`, `trace.route.{kind, value, matchedCase}`, `trace.failedEdgeId`, `trace.valueTruncated`. P9 adds `poll` and `wait` to `FlowStepTrace` and passes `trace: &mut NodeTrace` on to `run_repeat_until` and `wait_for_callback`.
- **Review Focus coverage:** item 1 is pinned in Task 2, item 2 in Task 2, item 3 in Task 1 and Task 2, item 4 in Task 1, item 5 in Task 3.

Known follow-ups outside this plan: step `error` text is still not masked (roadmap F-02); a value under 6 bytes or an encoded secret is not masked anywhere.
