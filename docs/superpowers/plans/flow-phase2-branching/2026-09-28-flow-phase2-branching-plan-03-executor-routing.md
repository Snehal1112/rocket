# Flow Phase 2 — Plan 03: Executor Routing Semantics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `FlowExecutionService` execute If/Switch routing nodes and skip not-taken branches using the per-field join rule, reporting `skip_reason`/`branch`/`not_taken_count`.

**Architecture:** The node-fate decision (spec §6.3) moves into a new pure, synchronous module `crates/rocket-app/src/flow_routing.rs` (`NodeOutcome`, `NodeFate`, `is_live`, `decide_fate`), unit-tested without async or fakes. `FlowExecutionService::run` replaces its precomputed `skipped: HashSet` with a per-run `outcomes: HashMap<String, NodeOutcome>` and asks `decide_fate` for every node in topological order. `execute_node` receives only the live data edges `decide_fate` returns, and gains If/Switch arms that evaluate through a new `RequestExecutionService::evaluate_flow_route_expression` and pass the input's captured output through unchanged.

**Tech Stack:** Rust (edition as per workspace), `tokio` tests, `async-trait`, `serde_json`, crate-local inline fakes (`rocket-app` convention).

**Spec:** `docs/superpowers/specs/2026-09-28-flow-phase2-branching-design.md` — §5.5 (intentional duplicate-wire change), §6 (execution semantics, incl. §6.3.1 failure observation), §7 (`load_ordered_nodes` caller; `FlowService::save` is owned by plan 01 Task 4), §8.1–8.2 (event/step fields), §11 (executor tests). Plan index: `docs/superpowers/plans/flow-phase2-branching/00-plan-index.md` (cross-plan interface contract).

## Global Constraints

- No panicking unwraps in new production paths; every fallible path returns `DomainResult` (spec §10).
- Execution stays sequential in topological order; no parallelism (spec §3, §6.1).
- `FlowStepStarted` is published only for nodes that actually run — never for skipped ones (spec §6.8).
- Handle spellings come only from `rocket_flow::handle` (`RESULT`, `TRUE`, `FALSE`, `DEFAULT`, `INPUT`, `TRIGGER`, `case_handle`) — never string literals in production code (spec §5.3).
- Ambiguity message is exactly `field '<target_field>' has <n> live inputs`; the `trigger` group is exempt (spec §6.3).
- If evaluates `format!("!!({condition})")`; Switch evaluates `format!("String({value})")`; `null`/`undefined` become `"null"`/`"undefined"` (spec §6.4–6.6).
- Skipped steps set `skip_reason` and leave `error: None` (spec §6.8).
- `FlowRunFinished.skipped_count` stays the total of all skipped steps; `not_taken_count` counts `BranchNotTaken` only (spec §8.1).
- Failure observation (spec §6.3.1): a Request that failed only because of a non-2xx status is `Failed { responded: true }` and its response is captured; an edge from it is live only into an If/Switch `input`. Every other failure is `Failed { responded: false }` with nothing captured.
- Two live wires into the same data field fail the node even when both are plain wires — an intentional Phase 1 change documented in spec §5.5.
- `rocket-app` does no I/O and uses inline per-module mocks (`crates/rocket-app/CLAUDE.md`).
- Always pass `-j4` to cargo.
- A repository PreToolUse hook rejects any file containing the literal panicking-unwrap call; use `.expect("…")` in tests.

## Review Focus

1. **An If node downstream of a non-2xx Request** (e.g. "if login returned 401, refresh") — spec §6.3.1: the Request stays `failed`, but the If observes the 401 response and routes on it, while the Request's plain dependents are still `upstream_failed`. The easy mistake is capturing nothing for a failed Request, or making the failed Request live into *every* dependent. Pinned by `if_observes_a_non_2xx_request_and_routes_on_it` (Task 4) and the `flow_routing` unit tests `a_responded_failure_*` (Task 2).
2. **A transport error (no response) upstream of an If** — nothing to observe, so the If must be `upstream_failed`, not run against a missing capture (which would surface as a confusing "topological order violated" internal error). Pinned by `if_after_a_transport_error_is_skipped_as_upstream_failed` (Task 4) and `a_failure_without_a_response_still_skips_a_routing_node` (Task 2).
3. **A Phase 1 flow with two unconditional wires into the same field** (e.g. `a→c.url` and `b→c.url`) — Phase 1 applied both (last wins); by the user's decision recorded in spec §5.5 this now fails the node with `field 'url' has 2 live inputs`. Pinned by `two_unconditional_wires_into_one_field_now_fail_the_target` (Task 3).
4. **A `trigger` edge reaching `apply_wired_overrides`** — it would fail with "unrecognized target_field 'trigger'". Pinned by `trigger_edge_gates_but_never_overrides_a_field` (Task 3).
5. **A Switch whose value is a number** (`response.status` → `200`) must match a case whose `matches` is `"200"`. Pinned by `route_expression_stringifies_non_string_results` (Task 1) and `switch_routes_a_numeric_value_to_its_string_case` (Task 4).

(A routing node fed by an Input node — a `Value` capture — is also pinned, by `if_fed_by_an_input_node_passes_its_value_through`, Task 4.)

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-app/src/flow_routing.rs` (create) | Pure fate decision: `NodeOutcome`, `NodeFate`, `is_live`, `decide_fate` + unit tests. |
| `crates/rocket-app/src/lib.rs` (modify) | Register `pub(crate) mod flow_routing;`. |
| `crates/rocket-app/src/flow_execution_service.rs` (modify) | `evaluate_flow_route_expression`, `ExecutedNode`, rewritten `run`/`execute_node`/`result_to_step`, `load_ordered_nodes` via `validate`, If/Switch arms, tests. |

`crates/rocket-app/src/flow_service.rs` is **not** touched here: plan 01 Task 4 already switched `FlowService::save` to `validate`.

## Preconditions (plans 01 and 02 are merged)

- `rocket_flow::{validate, SwitchCase, FlowGraphError::{InvalidNode, InvalidEdge}}`, `rocket_flow::handle::*`, `FlowNodeKind::{If, Switch}`, `FlowEdge.source_handle` exist, and `FlowService::save` already calls `validate` (plan 01 Task 4).
- `rocket_shared::events::FlowSkipReason { UpstreamFailed, BranchNotTaken }` exists; `FlowStepCompleted` has `skip_reason: Option<FlowSkipReason>`, `branch: Option<String>`; `FlowRunFinished` has `not_taken_count: usize`; `FlowStepResult` has `skip_reason`, `branch`.
- `execute_node` contains a temporary arm (added by plan 01) returning `DomainError::InvalidInput` for `If { .. } | Switch { .. }`. Task 4 removes it.
- Existing test fixtures (`wire`, `linear_flow`, …) already set `source_handle: rocket_flow::handle::RESULT.to_string()` (plan 01).

---

### Task 1: Route-expression evaluator

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs:24-71` (the `impl RequestExecutionService` block)
- Test: same file, `mod tests`

**Interfaces:**
- Consumes: `RequestExecutionService::evaluate_var_expression(&self, collection_root: &str, expression: &str, response_json: &str) -> DomainResult<serde_json::Value>` (`execution_service.rs:1493`).
- Produces: `pub async fn evaluate_flow_route_expression(&self, collection: &str, output: &CapturedOutput, wrapped_expression: &str) -> DomainResult<String>`; test fakes `Scripted`, `ScriptedJsonqEngine` (used by Tasks 3–4).

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Add the scripted fake engine and write the failing tests**

Append inside `mod tests` in `crates/rocket-app/src/flow_execution_service.rs`, directly after `ErrorJsonqEngine` (around line 926):

```rust
    /// What `ScriptedJsonqEngine` answers for one rule.
    enum Scripted {
        /// Resolve to this JSON value.
        Value(serde_json::Value),
        /// Report a script error with this message.
        Throw(&'static str),
        /// Compute the value from the response the expression runs against.
        FromResponse(fn(Option<&HttpResponse>) -> serde_json::Value),
    }

    /// Script engine that answers by the first rule whose needle occurs in
    /// the generated code, so route wrappers (`!!(`, `String(`) and plain
    /// wire expressions can be told apart in one run. Unmatched code
    /// resolves to `"https://api.example.com/wired"`.
    struct ScriptedJsonqEngine {
        rules: Vec<(&'static str, Scripted)>,
    }
    #[async_trait]
    impl ScriptEngine for ScriptedJsonqEngine {
        async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
            let rule = self
                .rules
                .iter()
                .find(|(needle, _)| ctx.code.contains(needle))
                .map(|(_, answer)| answer);
            let value = match rule {
                Some(Scripted::Value(v)) => v.clone(),
                Some(Scripted::Throw(message)) => {
                    return Ok(ScriptResult {
                        error: Some((*message).to_string()),
                        ..Default::default()
                    })
                }
                Some(Scripted::FromResponse(f)) => f(ctx.response.as_ref()),
                None => serde_json::json!("https://api.example.com/wired"),
            };
            let mut vars = HashMap::new();
            vars.insert("__jsonq_result__".to_string(), value);
            Ok(ScriptResult {
                runtime_vars: vars,
                ..Default::default()
            })
        }
    }

    fn scripted(rules: Vec<(&'static str, Scripted)>) -> Box<dyn ScriptEngine> {
        Box::new(ScriptedJsonqEngine { rules })
    }

    #[tokio::test]
    async fn route_expression_passes_the_wrapped_expression_verbatim() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            scripted(vec![(
                "!!(response.status === 200)",
                Scripted::Value(serde_json::json!(true)),
            )]),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .evaluate_flow_route_expression("my-api", &output, "!!(response.status === 200)")
            .await
            .expect("route expression must resolve");

        assert_eq!(value, "true");
    }

    #[tokio::test]
    async fn route_expression_null_is_the_string_null_not_an_error() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::Value::Null,
            }),
        );
        let output = CapturedOutput::Value(VariableValue::simple("x"));

        let value = svc
            .evaluate_flow_route_expression("my-api", &output, "String(response.body.plan)")
            .await
            .expect("a null route result must not be an error");

        assert_eq!(value, "null");
    }

    #[tokio::test]
    async fn route_expression_returns_strings_unquoted() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("pro"),
            }),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .evaluate_flow_route_expression("my-api", &output, "String(response.body.plan)")
            .await
            .expect("resolve");

        assert_eq!(value, "pro");
    }

    #[tokio::test]
    async fn route_expression_stringifies_non_string_results() {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!(200),
            }),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let value = svc
            .evaluate_flow_route_expression("my-api", &output, "String(response.status)")
            .await
            .expect("resolve");

        assert_eq!(value, "200");
    }

    #[tokio::test]
    async fn route_expression_script_error_is_invalid_input() {
        let svc = service_with_engine(FakeCollectionRepo::new(), Box::new(ErrorJsonqEngine));
        let output = CapturedOutput::Request(Box::new(sample_response_output()));

        let err = svc
            .evaluate_flow_route_expression("my-api", &output, "!!(nope.nope)")
            .await
            .expect_err("a throwing route expression must be an Err");

        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_execution_service::tests::route_expression`
Expected: FAIL to compile — `no method named 'evaluate_flow_route_expression' found for struct 'RequestExecutionService'`.

- [ ] **Step 4: Implement the evaluator and share the response-building code**

Replace the whole `impl RequestExecutionService { … }` block at `flow_execution_service.rs:24-71` with:

```rust
/// Serializes `output` into the `HttpResponse` JSON a wire or route
/// expression runs against. `Value` outputs (Input/Output nodes) become a
/// synthetic 200 response whose `body` is the raw value, so one
/// `response.xxx` convention works for every node kind.
fn captured_output_response_json(output: &CapturedOutput) -> DomainResult<String> {
    let response = match output {
        CapturedOutput::Request(out) => out.response.clone(),
        CapturedOutput::Value(value) => rocket_http::HttpResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: Vec::new(),
            body: value.data().to_string(),
            duration_ms: 0,
            ttfb_ms: 0,
            size_bytes: value.data().len(),
        },
    };
    serde_json::to_string(&response)
        .map_err(|e| DomainError::Internal(format!("failed to serialize captured output: {e}")))
}

impl RequestExecutionService {
    /// Evaluates `expression` (a jsonq/JS snippet such as `"response.body"` or
    /// `"response.body.token"`) against `output`, reusing the same
    /// script-engine mechanism `evaluate_var_expression` uses for the Vars
    /// tab's preview — not a second sandbox invocation path.
    ///
    /// A string result is returned as-is and other JSON values are
    /// stringified. A `null` or `undefined` result is an `InvalidInput`
    /// error, so a wire never injects the literal text "null".
    pub async fn resolve_flow_wire_expression(
        &self,
        collection: &str,
        output: &CapturedOutput,
        expression: &str,
    ) -> DomainResult<String> {
        let response_json = captured_output_response_json(output)?;
        let result = self
            .evaluate_var_expression(collection, expression, &response_json)
            .await?;
        match result {
            // A `null` or `undefined` result would wire the literal text "null"
            // into the request, so it is an error instead.
            serde_json::Value::Null => Err(DomainError::InvalidInput(format!(
                "expression '{expression}' resolved to null/undefined"
            ))),
            serde_json::Value::String(s) => Ok(s),
            other => Ok(other.to_string()),
        }
    }

    /// Evaluates an If/Switch routing expression against `output`. The caller
    /// passes it already wrapped (`!!(…)` for If, `String(…)` for Switch), so
    /// the result is always a string; a `null` result becomes `"null"` rather
    /// than an error, which keeps a missing value routable by a case.
    pub async fn evaluate_flow_route_expression(
        &self,
        collection: &str,
        output: &CapturedOutput,
        wrapped_expression: &str,
    ) -> DomainResult<String> {
        let response_json = captured_output_response_json(output)?;
        let result = self
            .evaluate_var_expression(collection, wrapped_expression, &response_json)
            .await?;
        Ok(match result {
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        })
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_execution_service::tests`
Expected: PASS — the 5 new `route_expression_*` tests plus every pre-existing test in the module (the wire-expression tests prove the shared helper kept `resolve_flow_wire_expression`'s behaviour).

- [ ] **Step 6: Commit**

Stage `crates/rocket-app/src/flow_execution_service.rs`, then commit using the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add route-expression evaluator for If/Switch`.

---

### Task 2: Pure node-fate decision (`flow_routing.rs`)

**Files:**
- Create: `crates/rocket-app/src/flow_routing.rs`
- Modify: `crates/rocket-app/src/lib.rs` (module list, after line 15 `pub mod flow_service;`)
- Test: `crates/rocket-app/src/flow_routing.rs` (`mod tests`)

**Interfaces:**
- Consumes: `rocket_flow::{FlowEdge, handle}`; `rocket_shared::events::FlowSkipReason` (must derive `Clone, Copy, Debug, PartialEq, Eq` — plan 02).
- Produces (all `pub(crate)`):
  - `enum NodeOutcome { Succeeded { chosen_exit: String }, Failed { responded: bool }, Skipped(FlowSkipReason) }`
  - `enum NodeFate<'a> { Run { data_edges: Vec<&'a FlowEdge> }, Skip(FlowSkipReason), Fail(String) }`
  - `fn is_live(edge: &FlowEdge, outcomes: &HashMap<String, NodeOutcome>, target_is_routing: bool) -> bool`
  - `fn decide_fate<'a>(incoming: &[&'a FlowEdge], outcomes: &HashMap<String, NodeOutcome>, target_is_routing: bool) -> NodeFate<'a>`
  - `target_is_routing` is true when the node whose incoming edges are being judged is an If/Switch. It is what lets a `Failed { responded: true }` source feed a routing node's `input` (spec §6.3.1).

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Confirm the `FlowSkipReason` derives**

Run: `grep -n -B2 "pub enum FlowSkipReason" crates/rocket-shared/src/events.rs`
Expected: the derive line includes `Copy` and `PartialEq`. If `Copy` is missing, add it to that derive list (plan 02 owns the type; `Copy` is required because one reason is stored both in the outcome and in the step).

- [ ] **Step 3: Write the failing tests**

Create `crates/rocket-app/src/flow_routing.rs` containing only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::handle;
    use rocket_shared::events::FlowSkipReason;
    use std::collections::HashMap;

    fn edge(id: &str, from: &str, exit: &str, field: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: "t".to_string(),
            target_field: field.to_string(),
            expression: String::new(),
            source_handle: exit.to_string(),
        }
    }

    fn ok(exit: &str) -> NodeOutcome {
        NodeOutcome::Succeeded {
            chosen_exit: exit.to_string(),
        }
    }

    fn outcomes(entries: &[(&str, NodeOutcome)]) -> HashMap<String, NodeOutcome> {
        entries
            .iter()
            .map(|(id, o)| (id.to_string(), o.clone()))
            .collect()
    }

    fn data_ids(fate: &NodeFate<'_>) -> Vec<String> {
        match fate {
            NodeFate::Run { data_edges } => data_edges.iter().map(|e| e.id.clone()).collect(),
            other => panic!("expected Run, got {other:?}"),
        }
    }

    fn failed(responded: bool) -> NodeOutcome {
        NodeOutcome::Failed { responded }
    }

    #[test]
    fn edge_is_live_only_when_source_succeeded_on_the_same_exit() {
        let o = outcomes(&[
            ("plain", ok(handle::RESULT)),
            ("if", ok(handle::TRUE)),
            ("bad", failed(false)),
        ]);
        assert!(is_live(&edge("e1", "plain", handle::RESULT, "url"), &o, false));
        assert!(is_live(&edge("e2", "if", handle::TRUE, "url"), &o, false));
        assert!(!is_live(&edge("e3", "if", handle::FALSE, "url"), &o, false));
        assert!(!is_live(&edge("e4", "bad", handle::RESULT, "url"), &o, false));
        assert!(!is_live(&edge("e5", "unknown", handle::RESULT, "url"), &o, false));
    }

    #[test]
    fn a_node_without_incoming_edges_runs() {
        let fate = decide_fate(&[], &HashMap::new(), false);
        assert_eq!(fate, NodeFate::Run { data_edges: Vec::new() });
    }

    #[test]
    fn a_failed_source_skips_as_upstream_failed() {
        let e = edge("e1", "a", handle::RESULT, "url");
        for responded in [false, true] {
            let o = outcomes(&[("a", failed(responded))]);
            assert_eq!(
                decide_fate(&[&e], &o, false),
                NodeFate::Skip(FlowSkipReason::UpstreamFailed),
                "responded = {responded}"
            );
        }
    }

    #[test]
    fn an_upstream_failed_skip_propagates_as_upstream_failed() {
        let e = edge("e1", "a", handle::RESULT, "url");
        let o = outcomes(&[("a", NodeOutcome::Skipped(FlowSkipReason::UpstreamFailed))]);
        assert_eq!(
            decide_fate(&[&e], &o, false),
            NodeFate::Skip(FlowSkipReason::UpstreamFailed)
        );
    }

    #[test]
    fn failure_wins_over_not_taken_even_through_a_join() {
        // Spec §6.3 rule 3 runs before rule 4: one arm failed, the other was
        // not taken — the join is upstream_failed, not branch_not_taken.
        let failed_arm = edge("e1", "a", handle::RESULT, "body");
        let not_taken_arm = edge("e2", "b", handle::RESULT, "body");
        let o = outcomes(&[
            ("a", failed(false)),
            ("b", NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken)),
        ]);
        assert_eq!(
            decide_fate(&[&failed_arm, &not_taken_arm], &o, false),
            NodeFate::Skip(FlowSkipReason::UpstreamFailed)
        );
    }

    #[test]
    fn a_responded_failure_is_live_into_a_routing_input() {
        // Spec §6.3.1: a 401 Request feeds an If/Switch `input`.
        let e = edge("e1", "login", handle::RESULT, handle::INPUT);
        let o = outcomes(&[("login", failed(true))]);
        assert!(is_live(&e, &o, true));
        assert_eq!(data_ids(&decide_fate(&[&e], &o, true)), vec!["e1".to_string()]);
    }

    #[test]
    fn a_responded_failure_is_not_live_into_a_request_field_or_an_output() {
        // Only a routing node's `input` observes failures. A Request field,
        // an Output `value`, a trigger, or an `input`-named edge into a
        // non-routing node all stay upstream_failed.
        let o = outcomes(&[("login", failed(true))]);
        for field in ["url", "value", handle::TRIGGER, handle::INPUT] {
            let e = edge("e1", "login", handle::RESULT, field);
            assert!(!is_live(&e, &o, false), "field {field}");
            assert_eq!(
                decide_fate(&[&e], &o, false),
                NodeFate::Skip(FlowSkipReason::UpstreamFailed),
                "field {field}"
            );
        }
    }

    #[test]
    fn a_failure_without_a_response_still_skips_a_routing_node() {
        // A transport error captured nothing, so there is nothing to observe.
        let e = edge("e1", "login", handle::RESULT, handle::INPUT);
        let o = outcomes(&[("login", failed(false))]);
        assert!(!is_live(&e, &o, true));
        assert_eq!(
            decide_fate(&[&e], &o, true),
            NodeFate::Skip(FlowSkipReason::UpstreamFailed)
        );
    }

    #[test]
    fn a_not_taken_exit_skips_as_branch_not_taken() {
        let e = edge("e1", "if", handle::TRUE, "trigger");
        let o = outcomes(&[("if", ok(handle::FALSE))]);
        assert_eq!(
            decide_fate(&[&e], &o, false),
            NodeFate::Skip(FlowSkipReason::BranchNotTaken)
        );
    }

    #[test]
    fn not_taken_propagates_transitively_as_branch_not_taken() {
        let e = edge("e1", "mid", handle::RESULT, "url");
        let o = outcomes(&[("mid", NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken))]);
        assert_eq!(
            decide_fate(&[&e], &o, false),
            NodeFate::Skip(FlowSkipReason::BranchNotTaken)
        );
    }

    #[test]
    fn merge_into_one_field_runs_with_only_the_live_edge() {
        // Case 1: both arms feed `body`; only the false arm ran.
        let from_true_arm = edge("e1", "profile", handle::RESULT, "body");
        let from_false_arm = edge("e2", "refresh", handle::RESULT, "body");
        let o = outcomes(&[
            ("profile", NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken)),
            ("refresh", ok(handle::RESULT)),
        ]);
        let fate = decide_fate(&[&from_true_arm, &from_false_arm], &o, false);
        assert_eq!(data_ids(&fate), vec!["e2".to_string()]);
    }

    #[test]
    fn a_different_field_without_a_live_edge_skips_the_node() {
        // Case 2: url is live, the token header only comes from a not-taken arm.
        let url = edge("e1", "config", handle::RESULT, "url");
        let token = edge("e2", "get_token", handle::RESULT, "headers[Authorization].value");
        let o = outcomes(&[
            ("config", ok(handle::RESULT)),
            ("get_token", NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken)),
        ]);
        assert_eq!(
            decide_fate(&[&url, &token], &o, false),
            NodeFate::Skip(FlowSkipReason::BranchNotTaken)
        );
    }

    #[test]
    fn two_live_edges_into_one_data_field_fail_the_node() {
        // Applies to plain wires too — the intentional Phase 1 change in spec §5.5.
        let a = edge("e1", "a", handle::RESULT, "body");
        let b = edge("e2", "b", handle::RESULT, "body");
        let o = outcomes(&[("a", ok(handle::RESULT)), ("b", ok(handle::RESULT))]);
        assert_eq!(
            decide_fate(&[&a, &b], &o, false),
            NodeFate::Fail("field 'body' has 2 live inputs".to_string())
        );
    }

    #[test]
    fn several_live_triggers_run_and_are_not_data_edges() {
        let t1 = edge("e1", "a", handle::RESULT, handle::TRIGGER);
        let t2 = edge("e2", "b", handle::RESULT, handle::TRIGGER);
        let url = edge("e3", "c", handle::RESULT, "url");
        let o = outcomes(&[
            ("a", ok(handle::RESULT)),
            ("b", ok(handle::RESULT)),
            ("c", ok(handle::RESULT)),
        ]);
        let fate = decide_fate(&[&t1, &t2, &url], &o, false);
        assert_eq!(data_ids(&fate), vec!["e3".to_string()]);
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/rocket-app/src/lib.rs`, add after `pub mod flow_service;`:

```rust
pub(crate) mod flow_routing;
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_routing`
Expected: FAIL to compile — `cannot find type 'NodeOutcome' in this scope` (and `NodeFate`, `is_live`, `decide_fate`).

- [ ] **Step 6: Implement the module**

Insert above the `#[cfg(test)]` module in `crates/rocket-app/src/flow_routing.rs`:

```rust
//! Decides, for one Flow node at its turn in topological order, whether it
//! runs, is skipped, or fails — from the recorded outcomes of its direct
//! predecessors (spec §6.2–6.3). Pure and synchronous.

use std::collections::HashMap;

use rocket_flow::{handle, FlowEdge};
use rocket_shared::events::FlowSkipReason;

/// What happened to a node earlier in the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeOutcome {
    /// The node ran successfully and left through `chosen_exit`
    /// (`handle::RESULT` for plain nodes).
    Succeeded { chosen_exit: String },
    /// `responded` is true only for a Request that failed because of a
    /// non-2xx status. Its response is captured, so a routing node may
    /// observe it (spec §6.3.1).
    Failed { responded: bool },
    Skipped(FlowSkipReason),
}

/// What a node should do at its turn.
#[derive(Debug, PartialEq)]
pub(crate) enum NodeFate<'a> {
    /// Run it, feeding only these live, non-trigger edges.
    Run { data_edges: Vec<&'a FlowEdge> },
    Skip(FlowSkipReason),
    /// Fail it without executing, with this error message.
    Fail(String),
}

/// An edge is live when its source succeeded and left through the exit the
/// edge is attached to, or when it carries a failed Request's captured
/// response into an If/Switch `input` (spec §6.3.1). `target_is_routing`
/// says whether the edge's target node is an If/Switch.
pub(crate) fn is_live(
    edge: &FlowEdge,
    outcomes: &HashMap<String, NodeOutcome>,
    target_is_routing: bool,
) -> bool {
    match outcomes.get(&edge.source_node_id) {
        Some(NodeOutcome::Succeeded { chosen_exit }) => *chosen_exit == edge.source_handle,
        Some(NodeOutcome::Failed { responded: true }) => {
            target_is_routing && edge.target_field == handle::INPUT
        }
        _ => false,
    }
}

/// Applies spec §6.3 rules 2–5 (rule 1, cancellation, stays in the caller).
/// Several edges into the same `target_field` are alternatives: one live
/// edge is enough. Different fields are all required.
pub(crate) fn decide_fate<'a>(
    incoming: &[&'a FlowEdge],
    outcomes: &HashMap<String, NodeOutcome>,
    target_is_routing: bool,
) -> NodeFate<'a> {
    if incoming.is_empty() {
        return NodeFate::Run {
            data_edges: Vec::new(),
        };
    }

    // Rule 3 only looks at edges that are not live, so an edge made live
    // by failure observation does not poison its routing node.
    let upstream_failed = incoming.iter().any(|e| {
        !is_live(e, outcomes, target_is_routing)
            && matches!(
                outcomes.get(&e.source_node_id),
                Some(NodeOutcome::Failed { .. })
                    | Some(NodeOutcome::Skipped(FlowSkipReason::UpstreamFailed))
            )
    });
    if upstream_failed {
        return NodeFate::Skip(FlowSkipReason::UpstreamFailed);
    }

    // Group by target field, keeping first-seen order so messages are stable.
    let mut groups: Vec<(&'a str, Vec<&'a FlowEdge>)> = Vec::new();
    for &e in incoming {
        match groups.iter_mut().find(|(field, _)| *field == e.target_field) {
            Some((_, edges)) => edges.push(e),
            None => groups.push((e.target_field.as_str(), vec![e])),
        }
    }

    let mut data_edges = Vec::new();
    let mut ambiguous: Option<(&str, usize)> = None;
    for (field, edges) in &groups {
        let live: Vec<&'a FlowEdge> = edges
            .iter()
            .copied()
            .filter(|e| is_live(e, outcomes, target_is_routing))
            .collect();
        if live.is_empty() {
            return NodeFate::Skip(FlowSkipReason::BranchNotTaken);
        }
        // A trigger carries no data, so several live triggers are fine.
        if *field == handle::TRIGGER {
            continue;
        }
        if live.len() > 1 && ambiguous.is_none() {
            ambiguous = Some((*field, live.len()));
        }
        data_edges.extend(live);
    }

    if let Some((field, count)) = ambiguous {
        return NodeFate::Fail(format!("field '{field}' has {count} live inputs"));
    }
    NodeFate::Run { data_edges }
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_routing`
Expected: PASS — 14 tests. A `dead_code` warning for the `pub(crate)` items is expected until Task 3 uses them; it must be gone after Task 3.

- [ ] **Step 8: Commit**

Stage `crates/rocket-app/src/flow_routing.rs` and `crates/rocket-app/src/lib.rs` (plus `crates/rocket-shared/src/events.rs` only if Step 2 added `Copy`), then commit using the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add per-field node-fate decision`.

---

### Task 3: Outcome-driven run loop, live-edge wiring, `load_ordered_nodes` via `validate`

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` — imports (lines 1-14, 280-284), `load_ordered_nodes` (:355-365), `run` (:399-526), `execute_node` (:532-634), `result_to_step` (:642-684)
- Test: `flow_execution_service.rs` `mod tests`
- Not modified: `crates/rocket-app/src/flow_service.rs` — plan 01 Task 4 owns `FlowService::save` → `validate`.

**Interfaces:**
- Consumes: Task 2's `NodeOutcome` (incl. `Failed { responded }`), `NodeFate`, `decide_fate(incoming, outcomes, target_is_routing)`; `rocket_flow::validate(flow: &Flow) -> Result<Vec<String>, FlowGraphError>`; plan 02 fields `FlowStepResult { skip_reason, branch }`, `DomainEvent::FlowStepCompleted { skip_reason, branch, .. }`, `DomainEvent::FlowRunFinished { not_taken_count, .. }`.
- Produces: `pub(crate) struct ExecutedNode { pub(crate) output: CapturedOutput, pub(crate) chosen_exit: String }`; `execute_node(&self, exec, input, node, data_edges: &[&FlowEdge], captured, external_secrets) -> DomainResult<ExecutedNode>`; `result_to_step(node_id, node, &DomainResult<ExecutedNode>) -> FlowStepResult`; test helpers `edge_from`, `step_of`, `not_taken_count` (used by Task 4).

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing tests**

Append at the end of `mod tests` in `flow_execution_service.rs`:

```rust
    // ---- Phase 2: outcome-driven run loop --------------------------------

    use rocket_flow::handle;
    use rocket_shared::events::FlowSkipReason;

    fn edge_from(id: &str, from: &str, exit: &str, to: &str, field: &str, expression: &str) -> FlowEdge {
        FlowEdge {
            source_handle: exit.to_string(),
            target_field: field.to_string(),
            expression: expression.to_string(),
            ..wire(id, from, to)
        }
    }

    fn step_of<'s>(summary: &'s FlowRunSummary, id: &str) -> &'s FlowStepResult {
        summary
            .steps
            .iter()
            .find(|s| s.node_id == id)
            .unwrap_or_else(|| panic!("no step recorded for node '{id}'"))
    }

    fn not_taken_count(publisher: &RecordingPublisher) -> usize {
        publisher
            .events()
            .into_iter()
            .find_map(|e| match e {
                DomainEvent::FlowRunFinished { not_taken_count, .. } => Some(not_taken_count),
                _ => None,
            })
            .expect("a FlowRunFinished event")
    }

    #[tokio::test]
    async fn upstream_failed_skips_carry_a_skip_reason_and_no_error_text() {
        // Phase 1 regression: same statuses as before, reason now structured.
        let flow = Flow {
            name: "diamond2".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("d", "https://api.example.com/d"),
            ],
            edges: vec![
                wire("e1", "a", "b"),
                edge_from("e2", "b", handle::RESULT, "d", "body", "response.body"),
            ],
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("diamond2")).await.expect("run");

        for id in ["b", "d"] {
            let s = step_of(&summary, id);
            assert_eq!(s.status, FlowNodeStatus::Skipped, "node {id}");
            assert_eq!(s.skip_reason, Some(FlowSkipReason::UpstreamFailed), "node {id}");
            assert_eq!(s.error, None, "node {id}");
        }
        assert_eq!(finished_counts(&publisher), (3, 1, 2));
        assert_eq!(not_taken_count(&publisher), 0);
    }

    #[tokio::test]
    async fn trigger_edge_gates_but_never_overrides_a_field() {
        let flow = Flow {
            name: "trigger".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![edge_from("t1", "a", handle::RESULT, "b", handle::TRIGGER, "")],
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("trigger")).await.expect("run");

        assert_eq!(status_of(&summary, "b"), FlowNodeStatus::Success);
        assert_eq!(
            executor.sent_urls(),
            vec![
                "https://api.example.com/a".to_string(),
                "https://api.example.com/b".to_string(),
            ],
            "a trigger must not write into any field of b"
        );
    }

    #[tokio::test]
    async fn trigger_from_a_failed_node_skips_the_target_as_upstream_failed() {
        let flow = Flow {
            name: "trigger-fail".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![edge_from("t1", "a", handle::RESULT, "b", handle::TRIGGER, "")],
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service.run(&exec, run_input("trigger-fail")).await.expect("run");

        assert_eq!(
            step_of(&summary, "b").skip_reason,
            Some(FlowSkipReason::UpstreamFailed)
        );
    }

    #[tokio::test]
    async fn two_unconditional_wires_into_one_field_now_fail_the_target() {
        // Intentional Phase 1 change (spec §5.5, user decision): Phase 1
        // silently used the last wire; now the node fails with a clear error.
        let flow = Flow {
            name: "ambiguous".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
                request_flow_node("c", "https://api.example.com/c"),
            ],
            edges: vec![wire("e1", "a", "c"), wire("e2", "b", "c")],
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("ambiguous")).await.expect("run");

        let c = step_of(&summary, "c");
        assert_eq!(c.status, FlowNodeStatus::Failed);
        assert_eq!(c.error.as_deref(), Some("field 'url' has 2 live inputs"));
        assert_eq!(executor.sent_urls().len(), 2, "c must never be sent");
    }

    #[test]
    fn load_ordered_nodes_rejects_a_structurally_invalid_flow() {
        // V1: an If node needs exactly one `input` edge; this one has none.
        // Acyclic on purpose, so the rejection comes from `validate`'s
        // structural rules, not from `topological_sort`.
        let mut flow = linear_flow();
        flow.nodes.push(FlowNode {
            id: "lonely_if".to_string(),
            kind: FlowNodeKind::If {
                label: "Lonely".to_string(),
                condition: "true".to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        });
        flow.name = "invalid".to_string();
        let service = service_with_flow(flow);

        let err = service
            .load_ordered_nodes("my-api", "invalid")
            .expect_err("an invalid flow must not load for execution");

        assert!(matches!(err, DomainError::InvalidInput(ref m) if m.contains("not runnable")));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_`
Expected: FAIL.
- `upstream_failed_skips_carry_a_skip_reason_and_no_error_text` fails, because either `error` is still `Some("upstream node failed")` or `skip_reason` is `None` (it depends on plan 02's interim choice).
- `trigger_edge_gates_but_never_overrides_a_field` fails with `unrecognized target_field 'trigger'`.
- `two_unconditional_wires_into_one_field_now_fail_the_target` fails because c succeeds.
- `load_ordered_nodes_rejects_a_structurally_invalid_flow` fails because `topological_sort` accepts that flow.

- [ ] **Step 4: Rewrite imports, `ExecutedNode`, and `load_ordered_nodes`**

In `flow_execution_service.rs`, change line 6 and add the routing import after line 14:

```rust
use rocket_flow::{handle, FlowEdge, FlowNode, FlowNodeKind, InlineRequestData, RequestSource};
```

```rust
use crate::flow_routing::{decide_fate, NodeFate, NodeOutcome};
```

Change the events import (line 283) to:

```rust
use rocket_shared::events::{DomainEvent, FlowNodeStatus, FlowSkipReason};
```

Add directly below the `CapturedOutput` enum (after line 22):

```rust
/// A node that ran: its captured output plus the exit it left through
/// (`handle::RESULT` for every non-routing node).
#[derive(Debug, Clone)]
pub(crate) struct ExecutedNode {
    pub(crate) output: CapturedOutput,
    pub(crate) chosen_exit: String,
}

impl ExecutedNode {
    fn plain(output: CapturedOutput) -> Self {
        Self {
            output,
            chosen_exit: handle::RESULT.to_string(),
        }
    }
}
```

Replace `load_ordered_nodes`'s body:

```rust
        let flow = self.flow_repo.get(collection, flow_name)?;
        let order = rocket_flow::validate(&flow).map_err(|e| {
            DomainError::InvalidInput(format!("flow '{flow_name}' is not runnable: {e}"))
        })?;
        Ok((flow, order))
```

- [ ] **Step 5: Rewrite `run`**

Replace the whole `run` method (and its doc comment) with:

```rust
    /// Runs every node of `input.flow_name` in dependency order. Each node's
    /// fate is decided at its turn from its predecessors' outcomes
    /// (`flow_routing::decide_fate`): it runs, is skipped (`upstream_failed`
    /// or `branch_not_taken`), or fails on ambiguous inputs. Cancellation is
    /// checked before each node, so a cancelled run records no step for the
    /// nodes it never reached.
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
    ) -> DomainResult<FlowRunSummary> {
        let (flow, order) = self.load_ordered_nodes(&input.collection, &input.flow_name)?;

        // Fetch every External Secret value once for the whole run. Each
        // Request node reuses this map, so a run of N requests makes one
        // vault round-trip per secret instead of N.
        let external_secrets = exec
            .resolve_external_secrets(Some(&input.collection), input.environment_name.as_deref())
            .await?;
        let nodes_by_id: HashMap<&str, &FlowNode> =
            flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

        let run_id = Ulid::new().to_string();
        if let Ok(mut set) = self.in_flight.lock() {
            set.insert(run_id.clone());
        }
        self.events.publish(DomainEvent::FlowRunStarted {
            run_id: run_id.clone(),
            flow_name: input.flow_name.clone(),
            collection: input.collection.clone(),
            total_nodes: flow.nodes.len(),
        });

        let mut captured: HashMap<String, CapturedOutput> = HashMap::new();
        let mut outcomes: HashMap<String, NodeOutcome> = HashMap::new();
        let mut steps: Vec<FlowStepResult> = Vec::new();
        let mut stopped_reason = "completed".to_string();

        for node_id in &order {
            if self.is_cancelled(&run_id) {
                stopped_reason = "cancelled".to_string();
                break;
            }

            let incoming: Vec<&FlowEdge> = flow
                .edges
                .iter()
                .filter(|e| e.target_node_id == *node_id)
                .collect();
            // Routing nodes may observe a failed Request's response (§6.3.1).
            let target_is_routing = matches!(
                nodes_by_id.get(node_id.as_str()).map(|n| &n.kind),
                Some(FlowNodeKind::If { .. }) | Some(FlowNodeKind::Switch { .. })
            );

            let (step, outcome) = match decide_fate(&incoming, &outcomes, target_is_routing) {
                NodeFate::Skip(reason) => (skipped_step(node_id, reason), NodeOutcome::Skipped(reason)),
                NodeFate::Fail(message) => {
                    self.publish_started(&run_id, node_id);
                    (failed_step(node_id, message), NodeOutcome::Failed { responded: false })
                }
                NodeFate::Run { data_edges } => {
                    self.publish_started(&run_id, node_id);
                    // `validate` only returns ids from `flow.nodes`, so a miss
                    // here is a bug. It fails this node instead of panicking.
                    let node_opt = nodes_by_id.get(node_id.as_str()).copied();
                    let result = match node_opt {
                        Some(node) => {
                            self.execute_node(
                                exec,
                                &input,
                                node,
                                &data_edges,
                                &captured,
                                &external_secrets,
                            )
                            .await
                        }
                        None => Err(DomainError::Internal(format!(
                            "node '{node_id}' is missing from the flow"
                        ))),
                    };
                    let step = result_to_step(node_id, node_opt, &result);
                    let outcome = match &result {
                        Ok(executed) if step.status == FlowNodeStatus::Success => {
                            NodeOutcome::Succeeded {
                                chosen_exit: executed.chosen_exit.clone(),
                            }
                        }
                        // `Ok` but not a success is only possible for a Request
                        // with a non-2xx response (see `result_to_step`). It
                        // has a response a routing node may observe (§6.3.1).
                        Ok(_) => NodeOutcome::Failed { responded: true },
                        Err(_) => NodeOutcome::Failed { responded: false },
                    };
                    // Every downstream consumer reads from this map, so a node
                    // with several dependents (fan-out) is captured once. A
                    // non-2xx Request is captured too — that is what a routing
                    // node observes. An `Err` captures nothing.
                    if let Ok(executed) = result {
                        captured.insert(node_id.clone(), executed.output);
                    }
                    (step, outcome)
                }
            };

            self.publish_completed(&run_id, &step);
            outcomes.insert(node_id.clone(), outcome);
            steps.push(step);
        }

        self.clear_cancellation(&run_id);
        if let Ok(mut set) = self.in_flight.lock() {
            set.remove(&run_id);
        }

        let failed_count = steps
            .iter()
            .filter(|s| s.status == FlowNodeStatus::Failed)
            .count();
        let skipped_count = steps
            .iter()
            .filter(|s| s.status == FlowNodeStatus::Skipped)
            .count();
        let not_taken_count = steps
            .iter()
            .filter(|s| s.skip_reason == Some(FlowSkipReason::BranchNotTaken))
            .count();
        self.events.publish(DomainEvent::FlowRunFinished {
            run_id: run_id.clone(),
            stopped_reason: stopped_reason.clone(),
            node_count: steps.len(),
            failed_count,
            skipped_count,
            not_taken_count,
        });

        Ok(FlowRunSummary {
            run_id,
            steps,
            stopped_reason,
        })
    }

    fn publish_started(&self, run_id: &str, node_id: &str) {
        self.events.publish(DomainEvent::FlowStepStarted {
            run_id: run_id.to_string(),
            node_id: node_id.to_string(),
        });
    }

    fn publish_completed(&self, run_id: &str, step: &FlowStepResult) {
        self.events.publish(DomainEvent::FlowStepCompleted {
            run_id: run_id.to_string(),
            node_id: step.node_id.clone(),
            status: step.status,
            status_code: step.status_code,
            duration_ms: step.duration_ms,
            error: step.error.clone(),
            value: step.value.clone(),
            skip_reason: step.skip_reason,
            branch: step.branch.clone(),
        });
    }
```

- [ ] **Step 6: Rewrite `execute_node` for live data edges**

Replace `execute_node` (keep the temporary If/Switch arm from plan 01 unchanged at the end of the `match`) with:

```rust
    /// Dispatches one node by kind, feeding it only `data_edges` — the live,
    /// non-trigger edges `decide_fate` selected. Returns the node's captured
    /// output and chosen exit, or an error if the node itself failed.
    async fn execute_node(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        node: &FlowNode,
        data_edges: &[&FlowEdge],
        captured: &HashMap<String, CapturedOutput>,
        external_secrets: &HashMap<String, String>,
    ) -> DomainResult<ExecutedNode> {
        match &node.kind {
            FlowNodeKind::Input { value, .. } => {
                let settings = self.collection_repo.get_settings(&input.collection)?;
                let mut vars = HashMap::new();
                for cv in settings.variables.iter().filter(|v| v.enabled) {
                    let v = if cv.value.is_empty() {
                        cv.initial_value.clone()
                    } else {
                        cv.value.clone()
                    };
                    vars.insert(cv.key.clone(), v);
                }
                // An unknown `{{name}}` stays as literal text. When the value is
                // wired into a Request field, `execute` resolves it again with
                // the full environment scope, so environment variables still work.
                let resolved = rocket_environment::resolve(value.data(), &vars).output;
                Ok(ExecutedNode::plain(CapturedOutput::Value(VariableValue::simple(
                    resolved,
                ))))
            }
            FlowNodeKind::Output { .. } => {
                let Some(edge) = data_edges.first() else {
                    return Ok(ExecutedNode::plain(CapturedOutput::Value(
                        VariableValue::simple(""),
                    )));
                };
                // An Output node shows one value. Picking one of several wires
                // would silently drop the others, so this is an error.
                if data_edges.len() > 1 {
                    return Err(DomainError::InvalidInput(format!(
                        "output node '{}' has more than one incoming wire",
                        node.id
                    )));
                }
                let source_output = captured_source(node, edge, captured)?;
                let value = exec
                    .resolve_flow_wire_expression(&input.collection, source_output, &edge.expression)
                    .await?;
                Ok(ExecutedNode::plain(CapturedOutput::Value(VariableValue::simple(
                    value,
                ))))
            }
            FlowNodeKind::Request { .. } => {
                let mut request_input = build_execute_request_input(
                    self.collection_repo.as_ref(),
                    &input.collection,
                    input.environment_name.as_deref(),
                    input.global_env_name.as_deref(),
                    node,
                )?;

                let mut resolved = HashMap::new();
                for edge in data_edges {
                    let source_output = captured_source(node, edge, captured)?;
                    let value = exec
                        .resolve_flow_wire_expression(
                            &input.collection,
                            source_output,
                            &edge.expression,
                        )
                        .await?;
                    resolved.insert(edge.id.clone(), value);
                }
                let edges_owned: Vec<FlowEdge> = data_edges.iter().map(|e| (*e).clone()).collect();
                apply_wired_overrides(&mut request_input, &resolved, &edges_owned)?;

                let output = exec
                    .execute_with_external_secrets(request_input, external_secrets)
                    .await?;
                Ok(ExecutedNode::plain(CapturedOutput::Request(Box::new(output))))
            }
            // Keep plan 01's temporary If/Switch arm here; Task 4 replaces it.
        }
    }
```

Add this free function below `impl FlowExecutionService { … }`:

```rust
/// The captured output of `edge`'s source. Topological order guarantees it
/// exists; a miss is reported as an internal error, not a panic.
fn captured_source<'c>(
    node: &FlowNode,
    edge: &FlowEdge,
    captured: &'c HashMap<String, CapturedOutput>,
) -> DomainResult<&'c CapturedOutput> {
    captured.get(&edge.source_node_id).ok_or_else(|| {
        DomainError::Internal(format!(
            "node '{}' depends on '{}' which has not executed yet — topological order violated",
            node.id, edge.source_node_id
        ))
    })
}
```

- [ ] **Step 7: Rewrite `result_to_step` and add the step builders**

Replace `result_to_step` with:

```rust
/// Turns one node's `execute_node` outcome into its `FlowStepResult`. A node
/// counts as failed when `execute_node` errored, or when it produced a
/// `Request` response that is not 2xx. A routing node always succeeds when
/// it evaluated, even though it passes a `Request` capture through, and
/// reports the exit it took in `branch`.
fn result_to_step(
    node_id: &str,
    node: Option<&FlowNode>,
    result: &DomainResult<ExecutedNode>,
) -> FlowStepResult {
    let kind = node.map(|n| &n.kind);
    let is_routing = matches!(
        kind,
        Some(FlowNodeKind::If { .. }) | Some(FlowNodeKind::Switch { .. })
    );
    let base = FlowStepResult {
        node_id: node_id.to_string(),
        status: FlowNodeStatus::Success,
        status_code: None,
        duration_ms: None,
        error: None,
        value: None,
        skip_reason: None,
        branch: None,
    };
    match result {
        Ok(executed) if is_routing => FlowStepResult {
            branch: Some(executed.chosen_exit.clone()),
            ..base
        },
        Ok(ExecutedNode {
            output: CapturedOutput::Request(out),
            ..
        }) => {
            let status = out.response.status;
            let success = out.response.is_success();
            FlowStepResult {
                status: if success {
                    FlowNodeStatus::Success
                } else {
                    FlowNodeStatus::Failed
                },
                status_code: Some(status),
                duration_ms: Some(out.response.duration_ms),
                error: (!success).then(|| format!("non-2xx response: {status}")),
                ..base
            }
        }
        Ok(ExecutedNode {
            output: CapturedOutput::Value(v),
            ..
        }) => {
            let is_output = matches!(kind, Some(FlowNodeKind::Output { .. }));
            FlowStepResult {
                value: is_output.then(|| v.data().to_string()),
                ..base
            }
        }
        Err(e) => failed_step(node_id, e.to_string()),
    }
}

fn skipped_step(node_id: &str, reason: FlowSkipReason) -> FlowStepResult {
    FlowStepResult {
        node_id: node_id.to_string(),
        status: FlowNodeStatus::Skipped,
        status_code: None,
        duration_ms: None,
        error: None,
        value: None,
        skip_reason: Some(reason),
        branch: None,
    }
}

fn failed_step(node_id: &str, message: String) -> FlowStepResult {
    FlowStepResult {
        node_id: node_id.to_string(),
        status: FlowNodeStatus::Failed,
        status_code: None,
        duration_ms: None,
        error: Some(message),
        value: None,
        skip_reason: None,
        branch: None,
    }
}
```

Delete the now-unused `use` of `rocket_flow::graph::reachable_from` if any remains, and remove every other reference to the old `skipped` set.

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_`
Expected: PASS — the 5 new tests and every pre-existing test in `flow_execution_service`, `flow_service` (unchanged here; plan 01 Task 4's tests) and `flow_routing` (notably `failure_skips_every_transitive_dependent_with_exact_counts`, `cancel_is_checked_before_a_skipped_node_too`, `step_started_is_published_before_step_completed_and_never_for_a_skipped_node`, `output_node_with_two_incoming_wires_fails_instead_of_dropping_one` — now failing via the ambiguity rule — and the cycle-rejection test with its unchanged message).

Run: `cargo check -j4 -p rocket-app`
Expected: no warnings from `flow_routing.rs` or `flow_execution_service.rs`.

- [ ] **Step 9: Commit**

Stage `crates/rocket-app/src/flow_execution_service.rs`, then commit using the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): decide node fate from predecessor outcomes`.

---

### Task 4: If/Switch execution and routing tests

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` — `execute_node` (replace plan 01's temporary If/Switch arm)
- Test: same file, `mod tests`

**Interfaces:**
- Consumes: Task 1's `evaluate_flow_route_expression`, `Scripted`, `scripted`; Task 3's `ExecutedNode`, `captured_source`, `edge_from`, `step_of`, `not_taken_count`; `rocket_flow::{SwitchCase, handle::{TRUE, FALSE, DEFAULT, INPUT, TRIGGER, case_handle}}`.
- Produces: routing behaviour consumed by plans 04–05 through `FlowStepResult.branch` (`"true" | "false" | "case:<id>" | "default"`) and `skip_reason`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing tests**

Append at the end of `mod tests`:

```rust
    // ---- Phase 2: If / Switch routing -------------------------------------

    use rocket_flow::SwitchCase;

    fn if_node(id: &str, condition: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::If {
                label: id.to_string(),
                condition: condition.to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// `cases` are `(case id, matches)`; the label equals the id.
    fn switch_node(id: &str, value: &str, cases: &[(&str, &str)]) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Switch {
                label: id.to_string(),
                value: value.to_string(),
                cases: cases
                    .iter()
                    .map(|(case_id, matches)| SwitchCase {
                        id: case_id.to_string(),
                        label: case_id.to_string(),
                        matches: matches.to_string(),
                    })
                    .collect(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn input_edge(id: &str, from: &str, to: &str) -> FlowEdge {
        edge_from(id, from, handle::RESULT, to, handle::INPUT, "")
    }

    fn trigger_edge(id: &str, from: &str, exit: &str, to: &str) -> FlowEdge {
        edge_from(id, from, exit, to, handle::TRIGGER, "")
    }

    /// login -> if(check) -> true: yes, false: no.
    fn if_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                request_flow_node("login", "https://api.example.com/login"),
                if_node("check", "response.status === 200"),
                request_flow_node("yes", "https://api.example.com/yes"),
                request_flow_node("no", "https://api.example.com/no"),
            ],
            edges: vec![
                input_edge("e1", "login", "check"),
                trigger_edge("e2", "check", handle::TRUE, "yes"),
                trigger_edge("e3", "check", handle::FALSE, "no"),
            ],
        }
    }

    #[tokio::test]
    async fn if_true_runs_the_true_exit_and_marks_the_false_exit_not_taken() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(if_flow("if-true"), &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("if-true")).await.expect("run");

        let check = step_of(&summary, "check");
        assert_eq!(check.status, FlowNodeStatus::Success);
        assert_eq!(check.branch.as_deref(), Some(handle::TRUE));
        assert_eq!(check.status_code, None, "a routing node has no HTTP status");
        assert_eq!(status_of(&summary, "yes"), FlowNodeStatus::Success);
        let no = step_of(&summary, "no");
        assert_eq!(no.status, FlowNodeStatus::Skipped);
        assert_eq!(no.skip_reason, Some(FlowSkipReason::BranchNotTaken));
        assert_eq!(
            executor.sent_urls(),
            vec![
                "https://api.example.com/login".to_string(),
                "https://api.example.com/yes".to_string(),
            ]
        );
        assert_eq!(finished_counts(&publisher), (4, 0, 1));
        assert_eq!(not_taken_count(&publisher), 1);
        let completed_branch = publisher.events().into_iter().find_map(|e| match e {
            DomainEvent::FlowStepCompleted { node_id, branch, .. } if node_id == "check" => branch,
            _ => None,
        });
        assert_eq!(completed_branch.as_deref(), Some(handle::TRUE));
    }

    #[tokio::test]
    async fn if_false_runs_the_false_exit_only() {
        let service = service_with_flow(if_flow("if-false"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(false)))]),
        );

        let summary = service.run(&exec, run_input("if-false")).await.expect("run");

        assert_eq!(step_of(&summary, "check").branch.as_deref(), Some(handle::FALSE));
        assert_eq!(
            step_of(&summary, "yes").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
        assert_eq!(status_of(&summary, "no"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn if_condition_error_fails_the_node_and_skips_both_exits_as_upstream_failed() {
        let service = service_with_flow(if_flow("if-error"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Throw("ReferenceError: nope"))]),
        );

        let summary = service.run(&exec, run_input("if-error")).await.expect("run");

        let check = step_of(&summary, "check");
        assert_eq!(check.status, FlowNodeStatus::Failed);
        assert!(check.error.as_deref().is_some_and(|m| m.contains("ReferenceError")));
        for id in ["yes", "no"] {
            assert_eq!(
                step_of(&summary, id).skip_reason,
                Some(FlowSkipReason::UpstreamFailed),
                "node {id}"
            );
        }
    }

    #[tokio::test]
    async fn if_condition_that_is_not_a_boolean_string_fails_the_node() {
        let service = service_with_flow(if_flow("if-weird"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!("maybe")))]),
        );

        let summary = service.run(&exec, run_input("if-weird")).await.expect("run");

        assert_eq!(status_of(&summary, "check"), FlowNodeStatus::Failed);
    }

    #[tokio::test]
    async fn if_observes_a_non_2xx_request_and_routes_on_it() {
        // Spec §6.3.1: login gets a 401 and stays failed. The If still sees
        // the 401 response and routes to `false`. Login's plain dependent is
        // skipped as upstream_failed.
        let mut flow = if_flow("if-after-401");
        flow.nodes.push(request_flow_node("plain", "https://api.example.com/plain"));
        flow.edges.push(wire("e4", "login", "plain"));
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/login", 401);
        // The condition answers from the response it is given, so a `false`
        // result proves the If saw the real 401 and not a missing capture.
        let exec = recording_exec(
            &executor,
            scripted(vec![(
                "!!(",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.map(|r| r.status == 200).unwrap_or(true))
                }),
            )]),
        );

        let summary = service.run(&exec, run_input("if-after-401")).await.expect("run");

        let login = step_of(&summary, "login");
        assert_eq!(login.status, FlowNodeStatus::Failed);
        assert_eq!(login.status_code, Some(401));
        let check = step_of(&summary, "check");
        assert_eq!(check.status, FlowNodeStatus::Success);
        assert_eq!(check.branch.as_deref(), Some(handle::FALSE));
        assert_eq!(status_of(&summary, "no"), FlowNodeStatus::Success);
        assert_eq!(
            step_of(&summary, "yes").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
        assert_eq!(
            step_of(&summary, "plain").skip_reason,
            Some(FlowSkipReason::UpstreamFailed),
            "only routing inputs observe a failure"
        );
        assert_eq!(finished_counts(&publisher), (5, 1, 2));
        assert_eq!(not_taken_count(&publisher), 1);
        assert!(!executor.sent_urls().iter().any(|u| u.contains("/plain")));
    }

    #[tokio::test]
    async fn if_after_a_transport_error_is_skipped_as_upstream_failed() {
        // A status of 0 makes `RecordingExecutor` fail with a transport error.
        // With no response captured there is nothing to observe.
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(if_flow("if-after-transport"), &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/login", 0);
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service
            .run(&exec, run_input("if-after-transport"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "login"), FlowNodeStatus::Failed);
        for id in ["check", "yes", "no"] {
            assert_eq!(
                step_of(&summary, id).skip_reason,
                Some(FlowSkipReason::UpstreamFailed),
                "node {id}"
            );
        }
        assert_eq!(finished_counts(&publisher), (4, 1, 3));
    }

    #[tokio::test]
    async fn a_wire_leaving_an_if_exit_reads_the_ifs_input_response() {
        // login answers 201; the wire out of `true` echoes the status it sees.
        // A synthetic capture would report 200, so 201 proves pass-through.
        let mut flow = if_flow("pass-through");
        flow.edges[1] = edge_from("e2", "check", handle::TRUE, "yes", "url", "response.status");
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/login", 201);
        let exec = recording_exec(
            &executor,
            scripted(vec![
                ("!!(", Scripted::Value(serde_json::json!(true))),
                (
                    "response.status",
                    Scripted::FromResponse(|r| {
                        let status = r.map(|r| r.status).unwrap_or_default();
                        serde_json::json!(format!("https://api.example.com/from-{status}"))
                    }),
                ),
            ]),
        );

        let summary = service.run(&exec, run_input("pass-through")).await.expect("run");

        assert_eq!(status_of(&summary, "yes"), FlowNodeStatus::Success);
        assert!(executor
            .sent_urls()
            .contains(&"https://api.example.com/from-201".to_string()));
    }

    #[tokio::test]
    async fn if_fed_by_an_input_node_passes_its_value_through() {
        let flow = Flow {
            name: "input-if".to_string(),
            nodes: vec![
                FlowNode {
                    id: "in".to_string(),
                    kind: FlowNodeKind::Input {
                        label: "Plan".to_string(),
                        value: VariableValue::simple("pro"),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                if_node("check", "response.body === 'pro'"),
                FlowNode {
                    id: "out".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Out".to_string(),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
            ],
            edges: vec![
                input_edge("e1", "in", "check"),
                edge_from("e2", "check", handle::TRUE, "out", "value", "response.body"),
            ],
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![
                ("!!(", Scripted::Value(serde_json::json!(true))),
                (
                    "response.body",
                    Scripted::FromResponse(|r| {
                        serde_json::json!(r.map(|r| r.body.clone()).unwrap_or_default())
                    }),
                ),
            ]),
        );

        let summary = service.run(&exec, run_input("input-if")).await.expect("run");

        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("pro"));
    }

    /// login -> switch(plan) -> free: f, pro: p, default: d.
    fn switch_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                request_flow_node("login", "https://api.example.com/login"),
                switch_node("plan", "response.body.plan", &[("free", "free"), ("pro", "pro")]),
                request_flow_node("f", "https://api.example.com/f"),
                request_flow_node("p", "https://api.example.com/p"),
                request_flow_node("d", "https://api.example.com/d"),
            ],
            edges: vec![
                input_edge("e1", "login", "plan"),
                trigger_edge("e2", "plan", &handle::case_handle("free"), "f"),
                trigger_edge("e3", "plan", &handle::case_handle("pro"), "p"),
                trigger_edge("e4", "plan", handle::DEFAULT, "d"),
            ],
        }
    }

    async fn run_switch(name: &str, value: serde_json::Value) -> FlowRunSummary {
        let service = service_with_flow(switch_flow(name));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, scripted(vec![("String(", Scripted::Value(value))]));
        service.run(&exec, run_input(name)).await.expect("run")
    }

    #[tokio::test]
    async fn switch_routes_to_the_matching_case_only() {
        let summary = run_switch("sw-pro", serde_json::json!("pro")).await;

        assert_eq!(
            step_of(&summary, "plan").branch,
            Some(handle::case_handle("pro"))
        );
        assert_eq!(status_of(&summary, "p"), FlowNodeStatus::Success);
        for id in ["f", "d"] {
            assert_eq!(
                step_of(&summary, id).skip_reason,
                Some(FlowSkipReason::BranchNotTaken),
                "node {id}"
            );
        }
    }

    #[tokio::test]
    async fn switch_without_a_match_routes_to_default() {
        let summary = run_switch("sw-default", serde_json::json!("enterprise")).await;

        assert_eq!(step_of(&summary, "plan").branch.as_deref(), Some(handle::DEFAULT));
        assert_eq!(status_of(&summary, "d"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn switch_null_value_routes_to_a_case_matching_null() {
        let mut flow = switch_flow("sw-null");
        if let FlowNodeKind::Switch { cases, .. } = &mut flow.nodes[1].kind {
            cases[0].matches = "null".to_string();
        }
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("String(", Scripted::Value(serde_json::Value::Null))]),
        );

        let summary = service.run(&exec, run_input("sw-null")).await.expect("run");

        assert_eq!(
            step_of(&summary, "plan").branch,
            Some(handle::case_handle("free"))
        );
    }

    #[tokio::test]
    async fn switch_routes_a_numeric_value_to_its_string_case() {
        let mut flow = switch_flow("sw-num");
        if let FlowNodeKind::Switch { cases, .. } = &mut flow.nodes[1].kind {
            cases[1].matches = "200".to_string();
        }
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("String(", Scripted::Value(serde_json::json!(200)))]),
        );

        let summary = service.run(&exec, run_input("sw-num")).await.expect("run");

        assert_eq!(
            step_of(&summary, "plan").branch,
            Some(handle::case_handle("pro"))
        );
    }

    /// Spec §6.3 case 1 and case 2 in one graph (false taken):
    /// login -> check; true -> profile, false -> refresh;
    /// profile.body & refresh.body -> save (merge);
    /// config.url + profile.header -> call (accidental join).
    fn join_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                request_flow_node("login", "https://api.example.com/login"),
                if_node("check", "response.status === 200"),
                request_flow_node("profile", "https://api.example.com/profile"),
                request_flow_node("refresh", "https://api.example.com/refresh"),
                request_flow_node("save", "https://api.example.com/save"),
                request_flow_node("config", "https://api.example.com/config"),
                request_flow_node("call", "https://api.example.com/call"),
            ],
            edges: vec![
                input_edge("e1", "login", "check"),
                trigger_edge("e2", "check", handle::TRUE, "profile"),
                trigger_edge("e3", "check", handle::FALSE, "refresh"),
                edge_from("e4", "profile", handle::RESULT, "save", "body", "response.body"),
                edge_from("e5", "refresh", handle::RESULT, "save", "body", "response.body"),
                edge_from("e6", "config", handle::RESULT, "call", "url", "response.body"),
                edge_from(
                    "e7",
                    "profile",
                    handle::RESULT,
                    "call",
                    "headers[Authorization].value",
                    "response.body",
                ),
            ],
        }
    }

    #[tokio::test]
    async fn per_field_join_merges_alternatives_and_skips_a_missing_field() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(join_flow("join"), &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(false)))]),
        );

        let summary = service.run(&exec, run_input("join")).await.expect("run");

        assert_eq!(status_of(&summary, "save"), FlowNodeStatus::Success, "case 1 merge");
        assert_eq!(
            step_of(&summary, "call").skip_reason,
            Some(FlowSkipReason::BranchNotTaken),
            "case 2: the Authorization field has no live input"
        );
        let sent = executor.sent_urls();
        assert_eq!(
            sent.iter().filter(|u| u.contains("/save")).count(),
            1,
            "the merge node runs exactly once"
        );
        assert!(!sent.iter().any(|u| u.contains("/call")));
        assert_eq!(not_taken_count(&publisher), 2, "profile and call");
    }

    #[tokio::test]
    async fn a_failed_arm_poisons_the_join_even_when_the_other_arm_was_not_taken() {
        let service = service_with_flow(join_flow("join-fail"));
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/profile", 500);
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("join-fail")).await.expect("run");

        assert_eq!(status_of(&summary, "profile"), FlowNodeStatus::Failed);
        assert_eq!(
            step_of(&summary, "refresh").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
        assert_eq!(
            step_of(&summary, "save").skip_reason,
            Some(FlowSkipReason::UpstreamFailed)
        );
    }

    #[tokio::test]
    async fn dependents_of_a_not_taken_node_are_not_taken_too() {
        let mut flow = if_flow("transitive");
        flow.nodes.push(request_flow_node("after_no", "https://api.example.com/after"));
        flow.edges.push(wire("e4", "no", "after_no"));
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("transitive")).await.expect("run");

        assert_eq!(
            step_of(&summary, "after_no").skip_reason,
            Some(FlowSkipReason::BranchNotTaken)
        );
    }

    #[tokio::test]
    async fn several_live_triggers_into_one_node_run_it_once() {
        let mut flow = if_flow("two-triggers");
        flow.nodes.push(request_flow_node("both", "https://api.example.com/both"));
        flow.edges.push(trigger_edge("e4", "check", handle::TRUE, "both"));
        flow.edges.push(trigger_edge("e5", "login", handle::RESULT, "both"));
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("two-triggers")).await.expect("run");

        assert_eq!(status_of(&summary, "both"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn cancelling_before_a_routing_node_records_nothing_for_it() {
        let cancelled: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let mut service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", if_flow("cancel-if"))),
            Box::new(FakeCollectionRepo::new()),
            Box::new(CancelAfterSteps {
                cancel_after: 1,
                seen: Mutex::new(0),
                cancelled: Arc::clone(&cancelled),
            }),
        );
        service.cancelled = Arc::clone(&cancelled);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        let summary = service.run(&exec, run_input("cancel-if")).await.expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        assert_eq!(summary.steps.len(), 1);
        assert_eq!(summary.steps[0].node_id, "login");
    }

    #[tokio::test]
    async fn step_started_is_never_published_for_a_not_taken_node() {
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(if_flow("started-if"), &publisher);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(true)))]),
        );

        service.run(&exec, run_input("started-if")).await.expect("run");

        let started: Vec<String> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepStarted { node_id, .. } => Some(node_id),
                _ => None,
            })
            .collect();
        assert_eq!(started, vec!["login".to_string(), "check".to_string(), "yes".to_string()]);
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_execution_service::tests`
Expected: FAIL. Every new routing test reports the routing node as `Failed` with plan 01's temporary "not executable" `InvalidInput` message. For example, `if_true_runs_the_true_exit_and_marks_the_false_exit_not_taken` fails on `check.status`, and `if_observes_a_non_2xx_request_and_routes_on_it` fails on `check.status` too.

`if_after_a_transport_error_is_skipped_as_upstream_failed` and `cancelling_before_a_routing_node_records_nothing_for_it` may already pass.

- [ ] **Step 4: Replace the temporary arm with real If/Switch execution**

In `execute_node`, delete plan 01's temporary `FlowNodeKind::If { .. } | FlowNodeKind::Switch { .. }` arm and add:

```rust
            FlowNodeKind::If { condition, .. } => {
                let source = single_route_input(node, data_edges, captured)?;
                let raw = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        &format!("!!({condition})"),
                    )
                    .await?;
                let chosen_exit = match raw.as_str() {
                    "true" => handle::TRUE,
                    "false" => handle::FALSE,
                    other => {
                        return Err(DomainError::InvalidInput(format!(
                            "condition of node '{}' evaluated to '{other}', expected true or false",
                            node.id
                        )))
                    }
                };
                Ok(ExecutedNode {
                    output: source.clone(),
                    chosen_exit: chosen_exit.to_string(),
                })
            }
            FlowNodeKind::Switch { value, cases, .. } => {
                let source = single_route_input(node, data_edges, captured)?;
                let raw = exec
                    .evaluate_flow_route_expression(
                        &input.collection,
                        source,
                        &format!("String({value})"),
                    )
                    .await?;
                let chosen_exit = cases
                    .iter()
                    .find(|case| case.matches == raw)
                    .map(|case| handle::case_handle(&case.id))
                    .unwrap_or_else(|| handle::DEFAULT.to_string());
                Ok(ExecutedNode {
                    output: source.clone(),
                    chosen_exit,
                })
            }
```

Add below `captured_source`:

```rust
/// The captured output feeding a routing node through its single live
/// `input` edge. `validate` (V1) and `decide_fate` guarantee exactly one;
/// anything else is reported, never panicked on. The source may be a
/// Request that failed with a non-2xx status (spec §6.3.1): its response
/// was captured and is used exactly like a successful one.
fn single_route_input<'c>(
    node: &FlowNode,
    data_edges: &[&FlowEdge],
    captured: &'c HashMap<String, CapturedOutput>,
) -> DomainResult<&'c CapturedOutput> {
    match data_edges {
        [edge] if edge.target_field == handle::INPUT => captured_source(node, edge, captured),
        _ => Err(DomainError::InvalidInput(format!(
            "routing node '{}' needs exactly one live '{}' input, found {}",
            node.id,
            handle::INPUT,
            data_edges.len()
        ))),
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_`
Expected: PASS — all 17 new routing tests plus every earlier test.

Run: `cargo check -j4 && cargo test -j4 -p rocket-flow -p rocket-app -p rocket-shared`
Expected: PASS with no new warnings.

- [ ] **Step 6: Commit**

Stage `crates/rocket-app/src/flow_execution_service.rs`, then commit using the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): execute If and Switch routing nodes`. Footer: `Relates to: #31`.

---

## Spec coverage (self-review)

| Spec item | Task |
|---|---|
| §6.1 outcomes map replaces `skipped` set | 3 |
| §6.2 edge liveness | 2 (`is_live`) |
| §6.3 rules 2–5, ambiguity, trigger exemption, trigger not overriding | 2, 3 |
| §6.3.1 failure observation: `Failed { responded }`, non-2xx capture, live only into If/Switch `input`, transport error still skips | 2 (unit), 3 (outcome + capture), 4 (run-level) |
| §5.5 intentional change: two plain wires into one field fail | 2, 3 |
| §6.4 If (`!!(…)`, pass-through, error → Failed) | 1, 4 |
| §6.5 Switch (`String(…)`, first match, default, `null`) | 1, 4 |
| §6.6 `evaluate_flow_route_expression` | 1 |
| §6.7 Request live non-trigger edges; Output single live `value` | 3 |
| §6.8 `skip_reason`, `branch`, no error text on skips, no started event for skips | 3, 4 |
| §7 callers: `load_ordered_nodes` uses `validate` (`FlowService::save` is plan 01 Task 4) | 3 |
| §8.1 `not_taken_count`, `skipped_count` total | 3, 4 |
| §11 executor bullets incl. Phase 1 regression | 3, 4 |

## Next Plan

[Plan 04 — Frontend types, store and wiring](2026-09-28-flow-phase2-branching-plan-04-frontend-types-store-wiring.md)

## Post-Implementation Review

Before starting plan 04, dispatch an Opus-model subagent (Agent tool, `model: "opus"`) to review everything this plan added or changed in `crates/rocket-app/src/flow_routing.rs` and `flow_execution_service.rs`. It checks:

- interface gaps against the index contract and plan 02's event fields;
- that no handle literal leaks into production code;
- that every error path returns `DomainResult`;
- that `run` stays readable;
- that failure observation (§6.3.1) is confined to If/Switch `input` edges from a `Failed { responded: true }` source;
- that the Review Focus items are pinned by tests.

It may fix what it finds directly before plan 04 starts. Review Focus items 1–3 are settled user decisions (spec §6.3.1 and §5.5). The reviewer must not change their behaviour.
