# Flow Phase 2 — Plan 02: Events and Step Results Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Flow run events and step results a typed skip reason and a routing-branch field, and report failure skips through that reason instead of free text.

**Architecture:** `rocket-shared` gains a `FlowSkipReason` enum. `DomainEvent::FlowStepCompleted` gains two optional fields (`skip_reason`, `branch`) and `DomainEvent::FlowRunFinished` gains `not_taken_count`, all backward-compatible on the wire. `rocket-app`'s `FlowStepResult` IPC struct mirrors the two optional fields. The existing failure-skip path in `FlowExecutionService::run` sets `skip_reason = Some(UpstreamFailed)` and leaves `error` empty. Nothing produces `BranchNotTaken` or `branch` yet; plan 03 does.

**Tech Stack:** Rust, serde / serde_json, tokio tests.

**Spec:** `docs/superpowers/specs/2026-09-28-flow-phase2-branching-design.md` (§6.8, §8.1, §8.2). Plan index: `00-plan-index.md`.

## Global Constraints

- `FlowNodeStatus` stays exactly `running | success | failed | skipped`. Do not add a variant.
- `FlowSkipReason` is `#[serde(rename_all = "snake_case")]`, so the values on the wire are `"upstream_failed"` and `"branch_not_taken"`.
- `DomainEvent` variant fields stay snake_case (`skip_reason`, `branch`, `not_taken_count`). `FlowStepResult` is a camelCase IPC struct (`skipReason`, `branch`).
- New `Option` fields use `#[serde(default, skip_serializing_if = "Option::is_none")]`. `not_taken_count` uses `#[serde(default)]` and is always serialized.
- `skipped_count` stays the total of all skipped steps. `not_taken_count` counts only the `BranchNotTaken` ones.
- Skipped steps set `error: None`. `skip_reason` is authoritative (spec §6.8).
- `rocket-shared` depends on no other workspace crate.
- Always pass `-j4` to cargo. A PreToolUse hook rejects any file containing the literal unwrap method call (the word "unwrap" followed by empty parentheses). Use `.expect("…")` in tests.
- Commits use the `dev-workflow-skills:1-git-commit` skill with conventional-commit subjects.

## Review Focus

1. **A payload saved or emitted before this plan** (no `skip_reason`, `branch` or `not_taken_count` keys) must still deserialize, with `None`/`0` defaults. Tested in Task 1: `flow_step_completed_deserializes_without_phase2_keys` and `flow_run_finished_deserializes_without_not_taken_count`.
2. **A successful step's wire shape must not change.** An unset `skip_reason`/`branch` must not appear as `null` keys that break exact-match consumers. Tested in Task 1: the existing exact-JSON test `flow_step_completed_wire_shape_with_all_fields_present` stays byte-identical.
3. **For a skipped node, the summary and the live event must agree.** Both carry `skip_reason: upstream_failed` with no `error`, so the canvas shows the same thing live and after the run. Tested in Task 2: `skipped_step_reports_upstream_failed_in_summary_and_event`.
4. **A node that failed must not get a skip reason.** Only skipped nodes carry one, and no step carries `branch` before plan 03. Tested in Task 2: `failed_and_successful_steps_have_no_skip_reason_or_branch`.
5. **Casing is mixed on the IPC summary.** The key is camelCase (`skipReason`) but the value is snake_case (`"upstream_failed"`). The frontend (plan 04) relies on exactly that. Tested in Task 2: `flow_step_result_serializes_skip_reason_camel_key_snake_value`.

---

### Task 1: `FlowSkipReason` and new event fields (`rocket-shared`)

**Files:**
- Modify: `crates/rocket-shared/src/events.rs:1-10` (add the enum after `FlowNodeStatus`)
- Modify: `crates/rocket-shared/src/events.rs:175-197` (`FlowStepCompleted`, `FlowRunFinished`)
- Modify: `crates/rocket-shared/src/events.rs:651-744` (existing flow event tests, plus new tests)
- Modify: `crates/rocket-app/src/flow_execution_service.rs:446-454, 488-496, 513-519` (keep it compiling: fill the new fields)

**Interfaces:**
- Consumes: nothing from earlier plans.
- Produces:
  - `rocket_shared::events::FlowSkipReason { UpstreamFailed, BranchNotTaken }`, deriving `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`.
  - `DomainEvent::FlowStepCompleted { run_id, node_id, status, status_code, duration_ms, error, value, skip_reason: Option<FlowSkipReason>, branch: Option<String> }`
  - `DomainEvent::FlowRunFinished { run_id, stopped_reason, node_count, failed_count, skipped_count, not_taken_count: usize }`

- [ ] **Step 1: Read the reference spec**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

Then read spec §8.1.

- [ ] **Step 2: Write the failing tests**

In `crates/rocket-shared/src/events.rs`, inside the existing `#[cfg(test)] mod tests`, add these tests right after `flow_run_finished_wire_shape_tracks_failed_and_skipped_separately`:

```rust
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
        for reason in [FlowSkipReason::UpstreamFailed, FlowSkipReason::BranchNotTaken] {
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
```

In the same test module, update the existing tests that build these variants so they compile against the new shape. The first three stay semantically unchanged; the fourth gains the new count.

1. `flow_step_completed_wire_shape_with_all_fields_present`: add `skip_reason: None, branch: None,` after `value: Some("bob".into()),`. **Leave the expected JSON string exactly as it is.** It proves that unset new fields are omitted (Review Focus 2).

2. `flow_step_completed_serializes_with_optional_fields_absent`: replace the constructor and assertions with the following, since skipped steps no longer carry an error string:

```rust
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
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains(r#""status":"skipped""#));
        assert!(json.contains(r#""status_code":null"#));
        assert!(json.contains(r#""duration_ms":null"#));
        assert!(json.contains(r#""error":null"#));
        assert!(json.contains(r#""value":null"#));
        assert!(json.contains(r#""skip_reason":"upstream_failed""#));
        assert!(!json.contains("branch"));
```

3. `flow_step_completed_deserializes_with_optional_keys_missing`: the match destructures every field without `..`, so add the two new bindings and assertions:

```rust
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
            }
```

4. `flow_run_finished_wire_shape_tracks_failed_and_skipped_separately`: add `not_taken_count: 1,` after `skipped_count: 2,` and change the expected JSON to:

```rust
            r#"{"type":"flowRunFinished","run_id":"01J","stopped_reason":"completed","node_count":5,"failed_count":1,"skipped_count":2,"not_taken_count":1}"#
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-shared flow_`
Expected: FAIL to compile, with errors like `cannot find type 'FlowSkipReason' in this scope` and `variant 'DomainEvent::FlowStepCompleted' has no field named 'skip_reason'`.

- [ ] **Step 4: Implement the enum and fields**

In `crates/rocket-shared/src/events.rs`, directly below the `FlowNodeStatus` enum (after line 10), add:

```rust
/// Why a Flow node was skipped instead of executed. Reported alongside
/// `FlowNodeStatus::Skipped` so the UI can tell a failure cascade apart
/// from a routing branch that was simply not chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowSkipReason {
    UpstreamFailed,
    BranchNotTaken,
}
```

Replace the `FlowStepCompleted` and `FlowRunFinished` variants (lines 174-197) with:

```rust
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
        /// The node's captured value, populated only for `Output`-kind
        /// nodes. See `rocket_app::flow_execution_service::FlowStepResult`.
        value: Option<String>,
        /// Set only when `status` is `Skipped`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skip_reason: Option<FlowSkipReason>,
        /// The exit a succeeded If/Switch node took, e.g. `"true"` or
        /// `"case:<id>"`. `None` for every other node.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
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
```

- [ ] **Step 5: Keep `rocket-app` compiling**

In `crates/rocket-app/src/flow_execution_service.rs`, the two `DomainEvent::FlowStepCompleted { … }` constructions in `run()` (around lines 446 and 488) end with `value: step.value.clone(),`. Add these two lines after it in **both** places. Task 2 replaces them with the step's own fields.

```rust
                    skip_reason: None,
                    branch: None,
```

In the `DomainEvent::FlowRunFinished { … }` construction (around line 513), add after `skipped_count,`:

```rust
            not_taken_count: 0,
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-shared flow_`
Expected: PASS. That covers every `flow_*` test, including the 5 new ones.

Run: `cargo check -j4 --workspace`
Expected: `Finished` with no errors. (`src-tauri/src/tauri_event_bus.rs` matches these variants with `{ .. }`, so it needs no change.)

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-shared/src/events.rs crates/rocket-app/src/flow_execution_service.rs
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add skip reason and branch to flow events`.

---

### Task 2: `FlowStepResult` fields and typed failure skips (`rocket-app`)

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs:283` (import)
- Modify: `crates/rocket-app/src/flow_execution_service.rs:295-311` (`FlowStepResult`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs:437-457` (skip path in `run()`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs:488-519` (completed event and finished counts)
- Modify: `crates/rocket-app/src/flow_execution_service.rs:642-684` (`result_to_step`)
- Test: `crates/rocket-app/src/flow_execution_service.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes (Task 1): `rocket_shared::events::FlowSkipReason`, and the new `FlowStepCompleted` / `FlowRunFinished` fields.
- Produces (plan 03 extends these, plan 04 reads them over IPC):
  - `FlowStepResult { node_id, status, status_code, duration_ms, error, value, skip_reason: Option<FlowSkipReason>, branch: Option<String> }`, serialized camelCase as `skipReason` / `branch` and omitted when `None`.
  - A private helper `fn step_completed_event(run_id: &str, step: &FlowStepResult) -> DomainEvent`, used by every step-completed publish in `run()`. Plan 03 keeps using it.

- [ ] **Step 1: Read the reference spec**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

Then read spec §6.8 and §8.2.

- [ ] **Step 2: Write the failing tests**

In `crates/rocket-app/src/flow_execution_service.rs`, inside `mod tests`, add the tests below after `failure_skips_every_transitive_dependent_with_exact_counts`. They reuse the existing helpers `request_flow_node`, `wire`, `run_input`, `service_with_publisher`, `RecordingPublisher`, `RecordingExecutor`, `recording_exec`, `fixed_wire`, `status_of` and `finished_counts`.

```rust
    #[tokio::test]
    async fn skipped_step_reports_upstream_failed_in_summary_and_event() {
        // a -> b; a fails (500), so b is skipped because upstream failed.
        let flow = Flow {
            name: "chain".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("chain")).await.expect("run");

        let b = summary
            .steps
            .iter()
            .find(|s| s.node_id == "b")
            .expect("step for b");
        assert_eq!(b.status, FlowNodeStatus::Skipped);
        assert_eq!(b.skip_reason, Some(FlowSkipReason::UpstreamFailed));
        assert_eq!(b.error, None, "skip_reason replaces the old error text");
        assert_eq!(b.branch, None);

        let b_event = publisher
            .events()
            .into_iter()
            .find_map(|e| match e {
                DomainEvent::FlowStepCompleted {
                    node_id,
                    status,
                    error,
                    skip_reason,
                    branch,
                    ..
                } if node_id == "b" => Some((status, error, skip_reason, branch)),
                _ => None,
            })
            .expect("FlowStepCompleted for b");
        assert_eq!(
            b_event,
            (
                FlowNodeStatus::Skipped,
                None,
                Some(FlowSkipReason::UpstreamFailed),
                None
            )
        );
    }

    #[tokio::test]
    async fn failed_and_successful_steps_have_no_skip_reason_or_branch() {
        // a fails, c is independent and succeeds.
        let flow = Flow {
            name: "mixed".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("c", "https://api.example.com/c"),
            ],
            edges: vec![],
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        let summary = service.run(&exec, run_input("mixed")).await.expect("run");

        for step in &summary.steps {
            assert_eq!(step.skip_reason, None, "node {}", step.node_id);
            assert_eq!(step.branch, None, "node {}", step.node_id);
        }
        assert_eq!(status_of(&summary, "a"), FlowNodeStatus::Failed);
        assert_eq!(status_of(&summary, "c"), FlowNodeStatus::Success);
    }

    #[tokio::test]
    async fn run_finished_reports_zero_not_taken_for_failure_skips() {
        let flow = Flow {
            name: "chain".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher);
        let executor = RecordingExecutor::new();
        executor.set_status("example.com/a", 500);
        let exec = recording_exec(&executor, fixed_wire("https://api.example.com/wired"));

        service.run(&exec, run_input("chain")).await.expect("run");

        let not_taken: Vec<usize> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowRunFinished {
                    not_taken_count, ..
                } => Some(not_taken_count),
                _ => None,
            })
            .collect();
        assert_eq!(not_taken, vec![0]);
        assert_eq!(finished_counts(&publisher), (2, 1, 1));
    }

    #[test]
    fn flow_step_result_serializes_skip_reason_camel_key_snake_value() {
        let step = FlowStepResult {
            node_id: "b".into(),
            status: FlowNodeStatus::Skipped,
            status_code: None,
            duration_ms: None,
            error: None,
            value: None,
            skip_reason: Some(FlowSkipReason::BranchNotTaken),
            branch: None,
        };
        let json = serde_json::to_value(&step).expect("serialize");
        assert_eq!(json["skipReason"], "branch_not_taken");
        assert!(json.get("skip_reason").is_none());
        assert!(json.get("branch").is_none(), "None branch is omitted");

        let routed = FlowStepResult {
            skip_reason: None,
            branch: Some("true".into()),
            status: FlowNodeStatus::Success,
            ..step
        };
        let json = serde_json::to_value(&routed).expect("serialize");
        assert_eq!(json["branch"], "true");
        assert!(json.get("skipReason").is_none(), "None skipReason is omitted");

        let back: FlowStepResult = serde_json::from_value(serde_json::json!({
            "nodeId": "x", "status": "success", "statusCode": null,
            "durationMs": null, "error": null
        }))
        .expect("deserialize pre-Phase-2 summary step");
        assert_eq!(back.skip_reason, None);
        assert_eq!(back.branch, None);
    }
```

`use super::*;` at the top of `mod tests` brings `FlowSkipReason` into scope once Step 4 imports it in the parent module.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_execution_service::tests`
Expected: FAIL to compile, with `struct 'FlowStepResult' has no field named 'skip_reason'` and `cannot find type 'FlowSkipReason'`.

- [ ] **Step 4: Implement**

Change the import at line 283:

```rust
use rocket_shared::events::{DomainEvent, FlowNodeStatus, FlowSkipReason};
```

Replace `FlowStepResult` (lines 295-311) with:

```rust
/// One node's outcome within a run, as reported in `FlowRunSummary::steps`
/// and the `FlowStepCompleted` event. IPC DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStepResult {
    pub node_id: String,
    pub status: FlowNodeStatus,
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    /// The node's captured output value, populated only for `Output`-kind
    /// nodes (see `result_to_step`). `None` for a Request node (its result is
    /// the HTTP response, not a single value), an Input node, or a node that
    /// never produced output (Skipped/Failed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Set only when `status` is `Skipped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<FlowSkipReason>,
    /// The exit a succeeded If/Switch node took. `None` for every other node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// Builds the `FlowStepCompleted` event for one recorded step, so the live
/// event and the returned summary can never disagree.
fn step_completed_event(run_id: &str, step: &FlowStepResult) -> DomainEvent {
    DomainEvent::FlowStepCompleted {
        run_id: run_id.to_string(),
        node_id: step.node_id.clone(),
        status: step.status,
        status_code: step.status_code,
        duration_ms: step.duration_ms,
        error: step.error.clone(),
        value: step.value.clone(),
        skip_reason: step.skip_reason,
        branch: step.branch.clone(),
    }
}
```

In `run()`, replace the skip block (lines 437-457) with:

```rust
            if skipped.contains(node_id) {
                let step = FlowStepResult {
                    node_id: node_id.clone(),
                    status: FlowNodeStatus::Skipped,
                    status_code: None,
                    duration_ms: None,
                    error: None,
                    value: None,
                    skip_reason: Some(FlowSkipReason::UpstreamFailed),
                    branch: None,
                };
                self.events.publish(step_completed_event(&run_id, &step));
                steps.push(step);
                continue;
            }
```

Replace the second `self.events.publish(DomainEvent::FlowStepCompleted { … });` (the block that followed the `reachable_from` loop) with:

```rust
            self.events.publish(step_completed_event(&run_id, &step));
```

Replace the counting and `FlowRunFinished` publish (lines 505-519) with:

```rust
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
```

In `result_to_step` (lines 642-684), add `skip_reason: None, branch: None,` after the `value: …` line in each of its three `FlowStepResult { … }` literals (the `Request`, `Value` and `Err` arms).

Then confirm no other literal was missed:

Run: `grep -rn "FlowStepResult {" crates src-tauri/src`
Expected: only the struct definition, the skip block, the three `result_to_step` arms and the new test.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_execution_service::tests`
Expected: PASS. That includes the 4 new tests and every existing test (`failed_node_skips_only_its_downstream_dependents`, `failure_skips_every_transitive_dependent_with_exact_counts`, `step_started_is_published_before_step_completed_and_never_for_a_skipped_node`, …). No existing executor test asserted the old `"upstream node failed"` text.

Run: `cargo test -j4 -p rocket-shared -p rocket-app -p rocket-flow`
Expected: PASS.

Run: `cargo check -j4 --workspace`
Expected: `Finished` with no errors or new warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): report failure skips via typed skip reason`.

---

## Next Plan

[Plan 03 — Executor routing semantics](2026-09-28-flow-phase2-branching-plan-03-executor-routing.md). It replaces the `skipped` set with per-node outcomes and starts producing `BranchNotTaken` and `branch`, using `step_completed_event` from Task 2.

## Post-Implementation Review

Before starting plan 03, dispatch an Opus-model subagent (Agent tool, `model: "opus"`) to review everything this plan changed (`crates/rocket-shared/src/events.rs`, `crates/rocket-app/src/flow_execution_service.rs`). It has authority to fix what it finds directly. Checks:

- Every `FlowStepCompleted` publish in `run()` goes through `step_completed_event`. No hand-built copy remains that could drift from the summary.
- No new field on a `DomainEvent` variant uses camelCase, and `FlowStepResult` uses only camelCase keys.
- `skip_serializing_if` is on both new `Option` fields in both types, and `not_taken_count` is always serialized.
- There is no leftover `"upstream node failed"` string anywhere in `crates/` or `src-tauri/src/`. Frontend references are plan 04's job; note any that exist.
- `cargo test -j4 -p rocket-shared -p rocket-app` and `cargo check -j4 --workspace` are green after any fixes.
