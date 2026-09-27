# Flow Plan 04: Domain Events — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `FlowNodeStatus` and the three new `DomainEvent` variants
(`FlowRunStarted`, `FlowStepCompleted`, `FlowRunFinished`) that Plan 06's
`FlowExecutionService` publishes and Plan 07's Tauri layer streams to the
frontend.

**Architecture:** A pure addition to the existing `DomainEvent` enum in
`rocket-shared`, mirroring the existing Collection Runner events
(`RunnerStarted`/`RunnerStepCompleted`/`RunnerFinished`) field-for-field in
style. No I/O, no cross-crate dependencies beyond what this file already
imports.

**Tech Stack:** Rust, serde, serde_json (tests).

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§7 Execution). Plan index:
`docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md`.

## Global Constraints

- Modifies exactly one file: `crates/rocket-shared/src/events.rs`. No other
  file in this plan.
- `DomainEvent` carries `#[serde(tag = "type", rename_all = "camelCase")]`
  at the enum level — this renames the variant's wire-tag (e.g.
  `FlowRunStarted` → `"flowRunStarted"`) but does **not** rename the fields
  inside a variant; those stay snake_case, matching every existing variant
  (see the existing `runner_step_completed_wire_shape` test's comment:
  "Struct-variant fields stay snake_case — the enum's rename_all only
  renames variants. The frontend contract depends on this."). Do not add a
  per-field `rename_all` to the new variants.
- `FlowNodeStatus` is a new, separate enum (not a plain `String` field) —
  this is a deliberate small improvement over the existing
  `RunnerStepCompleted.status: String` convention (which only documents its
  allowed values in a comment), not a mistake to "fix" toward consistency.
  It still serializes to a snake_case string on the wire via
  `#[serde(rename_all = "snake_case")]`, so the frontend-facing JSON shape a
  consumer sees is equivalent to a plain string field.
- Test code uses `.expect("message")` for fallible calls, never the bare
  panicking shorthand.

## Review Focus

- `FlowNodeStatus`'s four variants (`Running`, `Success`, `Failed`,
  `Skipped`) each serialize to their exact snake_case wire string
  (`"running"`, `"success"`, `"failed"`, `"skipped"`) — assert the literal
  JSON, not just that serialization succeeds, the same way
  `runner_started_wire_shape` asserts the exact JSON string rather than just
  checking `is_ok()`.
- None of the three new variant names, or `FlowNodeStatus`'s name, collides
  with any existing `DomainEvent` variant or type already in this file —
  grep the file for `Flow` before adding, to catch a possible existing
  placeholder.
- `FlowStepCompleted.status_code`/`.duration_ms`/`.error` are all
  `Option<...>` (a `Skipped` or `Failed` step may have no status code or
  duration) — a test must construct at least one event with these fields
  `None` and confirm it still serializes (an `Option::None` field must not
  be required as `Some` for the JSON to round-trip).
- `FlowRunFinished` carries independent `failed_count`/`skipped_count`
  fields (not just one combined "unsuccessful" count) — a test must
  construct a summary with both nonzero and distinct, and assert both
  values individually, since a future edit collapsing them into one field
  would silently break Plan 06/07's ability to report each separately.

---

## Task 1: `FlowNodeStatus` + Flow run events

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`

**Interfaces:**
- Produces: `FlowNodeStatus { Running, Success, Failed, Skipped }`,
  `DomainEvent::FlowRunStarted { run_id, flow_name, collection, total_nodes
  }`, `DomainEvent::FlowStepCompleted { run_id, node_id, status,
  status_code, duration_ms, error }`, `DomainEvent::FlowRunFinished { run_id,
  stopped_reason, node_count, failed_count, skipped_count }` — consumed by
  Plan 06 (`FlowExecutionService`, publishes these) and Plan 07/08
  (`src-tauri`/frontend, stream and render these).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-shared/src/events.rs (add to the existing #[cfg(test)] mod tests)

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
fn flow_step_completed_wire_shape_with_all_fields_present() {
    let event = DomainEvent::FlowStepCompleted {
        run_id: "01J".into(),
        node_id: "node-1".into(),
        status: FlowNodeStatus::Success,
        status_code: Some(200),
        duration_ms: Some(184),
        error: None,
    };
    let json = serde_json::to_string(&event).expect("serialize");
    assert_eq!(
        json,
        r#"{"type":"flowStepCompleted","run_id":"01J","node_id":"node-1","status":"success","status_code":200,"duration_ms":184,"error":null}"#
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
        error: Some("upstream failed".into()),
    };
    let json = serde_json::to_string(&event).expect("serialize");
    assert!(json.contains(r#""status":"skipped""#));
    assert!(json.contains(r#""status_code":null"#));
    assert!(json.contains(r#""duration_ms":null"#));
    assert!(json.contains(r#""error":"upstream failed""#));
}

#[test]
fn flow_run_finished_wire_shape_tracks_failed_and_skipped_separately() {
    let event = DomainEvent::FlowRunFinished {
        run_id: "01J".into(),
        stopped_reason: "completed".into(),
        node_count: 5,
        failed_count: 1,
        skipped_count: 2,
    };
    let json = serde_json::to_string(&event).expect("serialize");
    assert_eq!(
        json,
        r#"{"type":"flowRunFinished","run_id":"01J","stopped_reason":"completed","node_count":5,"failed_count":1,"skipped_count":2}"#
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-shared events -j4`
Expected: FAIL — `FlowNodeStatus` and the three `DomainEvent` variants don't
exist yet (compile error).

- [ ] **Step 3: Add `FlowNodeStatus` and the new variants**

Add `FlowNodeStatus` above the `DomainEvent` enum definition:

```rust
// crates/rocket-shared/src/events.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeStatus {
    Running,
    Success,
    Failed,
    Skipped,
}
```

Add the three new variants to `DomainEvent`, in their own group (mirroring
how "Collection Runner events" is its own labeled group in the existing
enum):

```rust
// crates/rocket-shared/src/events.rs — inside `pub enum DomainEvent { ... }`,
// add a new group after the existing "Collection Runner events" group:

    // Flow events
    /// Emitted once when a Flow run starts, before its first node executes.
    FlowRunStarted {
        run_id: String,
        flow_name: String,
        collection: String,
        total_nodes: usize,
    },
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
    },
    /// Emitted once when a Flow run ends, for any reason.
    FlowRunFinished {
        run_id: String,
        stopped_reason: String,
        node_count: usize,
        failed_count: usize,
        skipped_count: usize,
    },
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-shared events -j4`
Expected: PASS — all existing `events` tests plus the 5 new ones from this
task.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-shared
git commit -m "feat(shared): add Flow run domain events"
```

---

## Next Plan

[Plan 05: Wiring resolution + request building](2026-09-27-flow-visual-workflow-builder-plan-05-wiring-and-request-building.md) —
adds, in `rocket-app`, the jsonq-based wire-expression evaluator and the
`FlowNode` → `ExecuteRequestInput` builder that Plan 06's
`FlowExecutionService` will call per node.

## Post-Implementation Review

Before starting Plan 05, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review the one file this plan modified: `crates/rocket-shared/src/events.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — do `FlowNodeStatus` and the
>    three new `DomainEvent` variants match exactly what the plan index's
>    locked interface contract promises Plan 06/07 will consume (field
>    names, types, and the snake_case-fields-inside-camelCase-tag
>    convention)?
> 2. Code quality and test coverage versus this plan's Review Focus section
>    (exact wire-shape strings, no naming collisions with existing
>    variants, `Option` fields genuinely optional on the wire, independent
>    `failed_count`/`skipped_count`).
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    confirm this change introduced zero I/O and zero new cross-crate
>    dependencies into `rocket-shared`.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-shared -j4` and
> `cargo check -p rocket-shared -j4`, and confirm they still pass. Report
> what you found and fixed.

Only proceed to Plan 05 once this review comes back clean (or its fixes are
applied and re-verified).
