# Flow — Visual Workflow Builder — Design Spec

**Date:** 2026-09-27
**Status:** Resolved — ready for implementation planning
**Scope:** Phase 1 (MVP) of "Flow" — a visual, node-canvas API workflow builder and runner, distinct from the existing linear, tree-order Collection Runner (`2026-09-16-collection-runner-design.md`).
**Out of scope (deferred to later phases, each getting its own spec):**
- Condition/branch nodes and non-linear (branching) graph execution — Phase 2.
- Transform/script nodes (reshaping a value between nodes with a JS snippet as a first-class node, distinct from a field wiring expression) — Phase 3.
- Multi-user real-time collaboration (cursors/avatars, as seen in the reference screenshot) — not planned; Rocket is a single-user desktop app per collection checkout.
- Data-driven/iteration runs (CSV-parameterized), scheduled/CI runs — same non-goals as the existing Collection Runner, not reconsidered here.

**Reference:** A ComfyUI-style node-canvas screenshot supplied by the user — informed the visual language (dotted-grid background, rounded node cards, colored field-level ports, curved bezier edges) and the core insight that connections should be able to carry a specific piece of data into a specific field, not just imply ordering.

---

## 1. Background

Rocket already has a Collection Runner that executes every request in a folder/collection in a fixed, tree-defined order (`crates/rocket-app/src/collection_runner_service.rs`, `RunnerPane.tsx`). It has no concept of manual sequencing, branching, or visual data wiring between requests — order is implicit (the folder tree) and data passes between steps only via scripted `rok.setVar`/`rok.getVar` runtime variables.

Flow adds a second, complementary way to compose multi-request work: a canvas where the user explicitly places nodes and draws the connections between them, including which specific field of one request receives which specific piece of another node's output. This is a new subsystem — a new domain crate, a new backend execution service, and a new frontend surface — not an extension of the existing Runner.

## 2. Goals (Phase 1 / MVP)

- A user can open a "Flow" tab, drag Request nodes (backed by an existing saved collection request, or defined inline) onto an infinite pannable/zoomable canvas, and connect them.
- A user can wire a specific output of one node (its whole response) into a specific field of another node (URL, a header value, the body) via a small expression evaluated against that output.
- A user can add Input nodes (constant/variable-backed values) and Output nodes (read-only result inspectors) to compose a full graph, matching the reference screenshot's Model/Positive/Negative → Image Generator pattern.
- Running a Flow executes every node in dependency order, using the exact same per-request execution path (`RequestExecutionService::execute`) the Send button and the existing Runner use — Flow is an orchestration layer, not a second HTTP execution engine.
- A Flow is saved as part of the collection, git-shared with the team, without altering the OpenCollection schema.
- Per-node run status (idle/running/success/failure, with HTTP status and timing) is visible live on the canvas while a run is in progress.

## 3. Non-goals (Phase 1)

See the scope section above. Additionally, not required for Phase 1:
- Parallel execution of independent branches (correct dependency-ordering is required; concurrent execution of unrelated branches is a later optimization, not a correctness requirement).
- Autosave (an explicit Save action is sufficient for v1).
- Undo/redo on the canvas (standard React Flow interactions — click-drag, delete — are enough; a full undo stack is a nice-to-have, not required).

## 4. Data model — new `rocket-flow` crate

A new domain crate, sibling to `rocket-collection`/`rocket-environment`, following the same DDD shape (types + a repository trait, no I/O):

```rust
pub struct Flow {
    pub name: String,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
}

pub struct FlowNode {
    pub id: String,           // ULID, stable across edits
    pub kind: FlowNodeKind,
    pub position: NodePosition,   // { x: f64, y: f64 } — canvas layout only
}

pub enum FlowNodeKind {
    Request { label: String, source: RequestSource },
    Input { label: String, value: VariableValue },   // reuses the existing VariableValue type from rocket-environment/rocket-shared
    Output { label: String },
}

pub enum RequestSource {
    /// Live reference into the collection tree, resolved at run time — edits to
    /// the saved request are picked up automatically, matching the tolerance
    /// the existing Collection Runner already has for tree data.
    Saved { request_path: String },
    /// A full ad hoc request embedded directly in the flow file. Reuses the
    /// existing internal request shape (method/url/headers/body/auth) that
    /// `rocket-collection`'s `HttpRequest`-equivalent type and
    /// `ExecuteRequestInput` already use — Flow does not define a parallel
    /// request schema.
    Inline { request: InlineRequestData },
}

pub struct FlowEdge {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub target_field: String,     // e.g. "url", "headers[1].value", "body" — a
                                    // path into the target node's own field set,
                                    // not a fixed enum, so new wireable fields
                                    // don't require a schema change here
    pub expression: String,       // JS expression, evaluated against the
                                    // source node's captured output (see §6)
}

pub struct FlowRepository { /* trait: list, get, save, delete — mirrors CollectionRepository */ }
```

`rocket-flow` has no dependency on `rocket-collection` for `Inline` requests' shape — it depends on the shared request-shape type wherever that already lives (verified at implementation-plan time; likely `rocket-shared` or `rocket-collection`'s public types), rather than duplicating a body/header/auth model.

## 5. Persistence

`FlowRepository`'s concrete implementation, `FsFlowRepo` (in `rocket-infra`), mirrors `FsCollectionRepo` exactly: trait-only in the domain crate, all disk I/O in infra.

On-disk layout:
```
<collection>/
  opencollection.yml
  environments/
  flows/
    <slug>.yml            ← one file per Flow, snake_case fields, .yml only
```

This is a **Rocket-specific extension**, not part of the OpenCollection schema — the spec's `additionalProperties: false` and closed `items[]` discriminator set (`http`/`graphql`/`grpc`/`websocket`/`folder`/`script`) forbid adding a `flow` item type or a `flows` field to `opencollection.yml`. A `flows/` directory sitting alongside `opencollection.yml` and `environments/` is invisible to the schema validator and to other OpenCollection-compliant tools, while still being a plain file that git tracks and teammates receive on pull — same visibility model as everything else in the collection folder.

## 6. Wiring semantics

Per the approved node mockup: a Request node renders one small input dot on its left edge next to *each* connectable field (URL, Headers, Body), and one output dot on its right edge for its whole captured result.

```
IDLE
┌──── ● GET  Get Auth Token ────────── ⋮ ──┐
│                                            │
○  URL       https://api.example.com/login  │
○  Headers   2 set                          │
○  Body      { "user": "{{u}}", ... }       │
│                                            │
└──────────────────────────── result ● ──────┘

RUNNING → SUCCESS                         RUNNING → FAILURE
┌── ● GET  Get Auth Token ─ ⋮ ──┐          ┌── ● GET  Get Auth Token ─ ⋮ ──┐
│ ✓ 200 OK · 184ms              │          │ ✕ 401 Unauthorized · 92ms     │
○  URL      https://...         │          ○  URL      https://...        │
...                             │          ...                            │
└─────────────── result ● ──────┘          └─────────────── result ● ─────┘
(header strip glows green)                  (header strip glows red)
```

Dragging a wire from a source node's output dot onto a target field's input dot creates a `FlowEdge` and opens a small inline expression box, pre-filled with a sensible default (e.g. `response.body`), editable by the user.

**Expression evaluation reuses the existing jsonq mechanism, not a new language.** In this codebase "jsonq" (used by `ActionSetVariable`, see `rocket-shared/src/action.rs`) is not a JSONPath/jq library — it is a plain JS expression run through the existing sandboxed script engine, and `RequestExecutionService` already exposes a preview-evaluation entry point for exactly this shape of call (`evaluate_var_expression` in `execution_service.rs`). `FlowExecutionService` calls this same mechanism, passing the source node's captured output as the evaluation context.

Input nodes have no incoming wires; their configured `VariableValue` (itself `{{variable}}`-resolvable through the existing environment/collection variable resolution) is their captured output. Output nodes have exactly one incoming wire and no outgoing ones; their captured output is shown read-only in the node body and in an expanded detail view.

## 7. Execution — new `FlowExecutionService` in `rocket-app`

Closely mirrors the proven shape of `CollectionRunnerService` (`collection_runner_service.rs`): a `Ulid` run id, external secrets resolved once up front for the whole run, cancellation checked before each node, a `MAX_RUN_STEPS`-equivalent guard, and domain events published at start/per-step/finish.

Where it differs, because the traversal itself is a different shape (topological DAG vs. tree-order cursor):

1. **Validation at save time, not just run time**: `save_flow` rejects a graph containing a cycle (Phase 1 has no loop-forming node type, so any cycle is a user error) with a clear domain error — a `Flow` value that fails to topologically sort is never persisted.
2. **Traversal**: nodes execute in topological order (Kahn's algorithm). Independent branches (e.g. the screenshot's three parallel input nodes) may execute in either relative order in Phase 1 — concurrency across independent branches is not required for v1 (§3).
3. **Output capture**: instead of a flat `HashMap<String, String>` of runtime variables carried step-to-step, `FlowExecutionService` keeps a `HashMap<node_id, CapturedOutput>` — each node's full captured result (an `Input` node's value, or a `Request` node's `ExecuteRequestOutput`), available to any downstream edge's expression.
4. **Failure containment falls out of the graph, with no separate flag**: if a node fails (non-2xx, execution error, or a wiring expression that throws), every node reachable from it downstream is marked `Skipped { reason: "upstream failed" }` and not executed; nodes on independent branches with no dependency on the failed node continue normally. This replaces the Collection Runner's opt-in `stop_on_failure` flag — for a dependency graph, "stop only what actually depends on the failure" is the correct default, and there's no other meaningful thing "stop on failure" could mean here.

`FlowExecutionService` does **not** inherit from or share a base type with `CollectionRunnerService`. Both have a similar outer shell (Ulid, secrets-once, cancellation, start/step/finish events), but the core loop is genuinely different, and forcing a shared abstraction now would mean guessing the right generalization before a third orchestration use case exists to confirm it. Phase 1 duplicates that small amount of scaffolding deliberately; extracting a shared base is a reasonable follow-up once (if) a third case appears — not a Phase 1 concern.

New domain events in `rocket-shared/src/events.rs`, matching the existing Runner events' shape:
```rust
FlowRunStarted { run_id: String, flow_name: String, collection: String, total_nodes: usize }
FlowStepCompleted { run_id: String, node_id: String, status: FlowNodeStatus, /* status code, timing, error, as applicable */ }
FlowRunFinished { run_id: String, stopped_reason: String, node_count: usize, failed_count: usize, skipped_count: usize }
```

Each executed Request node still produces a normal `HistoryEntry` via the same `RequestExecutionService::execute` path every other execution surface uses — no new history mechanism, exactly like the existing Runner (§8.4 of its spec).

## 8. Tauri IPC — new `src-tauri/src/commands/flow.rs`

- `list_flows(collection) -> Vec<FlowSummary>`
- `get_flow(collection, flow_name) -> FlowDto`
- `save_flow(collection, flow: FlowDto) -> ()` — runs the cycle-validation from §7.1 before persisting; rejects with a stable IPC error on a cyclic graph.
- `delete_flow(collection, flow_name) -> ()`
- `run_flow(collection, flow_name, environment_name?) -> { run_id }` — streams `flow-step-completed` / `flow-run-finished` events over the existing `TauriEventBus`, the same pattern already used for `RunnerStepCompleted`/`RunnerFinished`.
- `cancel_flow_run(run_id) -> ()`

IPC DTOs (`FlowDto`, `FlowNodeDto`, `FlowEdgeDto`, `FlowSummary`) use `#[serde(rename_all = "camelCase")]`; the underlying `rocket-flow` domain types stay snake_case — never the same struct doing both jobs.

## 9. Frontend

- **Canvas**: `@xyflow/react` (React Flow) — its built-in `<Background variant="dots">` gives the dotted-grid look from the reference screenshot for free, along with pan/zoom and `<Handle>` components that map directly onto the field-level ports from §6. This is an allowed addition to the frontend's dependency set; the canvas itself is DOM+SVG (per the project's existing "canvas/SVG" exception to the shadcn-only rule), not an HTML5 `<canvas>` bitmap surface.
- **Tab type**: a new `FlowTab` joins the `Tab` union in `src/types/pane-types.ts`, following the exact shape `RunnerTab` already established (id, name, and Flow-specific fields: `flowName`, `nodes`, `edges`, `runState`). A new `FlowPane.tsx` is registered in `PaneRenderer.tsx`.
- **Node components**: three custom React Flow node types (`RequestNode`, `InputNode`, `OutputNode`), each rendering per the approved mockup states (idle/running/success/failure), registered via React Flow's `nodeTypes` prop.
- **Adding nodes**: dragging an existing request from the collection sidebar tree onto the canvas creates a `Saved` Request node at the drop position. A small node palette (or a "+" menu) adds Input/Output nodes and inline Request nodes.
- **Wiring UI**: dragging from a node's output dot onto a target field's input dot creates the edge and opens the inline expression editor described in §6.
- **Run controls**: a toolbar Run/Stop button; per-node status updates live from the streamed `flow-step-completed` events, patched into the `FlowTab`'s node state the same way `RunnerTab` patches `RunnerRequestEntry.status` today — no separate parallel store.
- **State management**: Zustand, following the existing pane-store convention — never fully destructuring store state at component top level.

## 10. Error handling

- A cyclic graph is rejected at `save_flow` time with a specific error identifying a node in the cycle; the canvas should visually flag the offending edge(s) rather than just showing a toast.
- A wiring expression that throws during evaluation, or targets a field that no longer exists, fails that node (recorded in `FlowStepCompleted` with the error message) and cascades a `Skipped` status to its dependents, per §7.4.
- A `Saved` Request node whose `request_path` no longer resolves (request deleted/moved) fails that node with a clear "referenced request not found" error and cascades the same way — Flow does not attempt to fuzzy-match a renamed request.
- No panicking unwraps in any new Rust code path; all failures map to stable, typed IPC-facing errors via `DomainResult`.

## 11. Testing

- **Rust**: cycle detection and topological sort (`rocket-flow`, pure unit tests, no I/O); `FlowExecutionService` orchestration and skip-cascade behavior (`rocket-app`, following the existing test style in `collection_runner_service.rs` — fixture-built flows, fake `EventPublisher`, assertions on emitted events and final `FlowRunSummary`); `FsFlowRepo` round-trip tests (`rocket-infra`, `tempfile`-based, mirroring `FsCollectionRepo`'s tests).
- **Frontend**: Vitest for pane-store Flow actions (add/remove node, add/remove edge, patch status from streamed events) and node/edge reducers; component tests for each of the three node types covering the idle/running/success/failure visual states from the approved mockup; a focused test for the cycle-rejection error surfacing in the UI.
- **Verification commands**: `cargo check -j4` and focused `cargo test -j4 -p rocket-flow -p rocket-app`, `yarn tsc --noEmit`, `yarn check`.

## 12. Acceptance criteria (Phase 1)

- A user can create a new Flow, drag in at least one Saved Request node and one Input node, wire the Input node's value into a field on the Request node, and save it — the file appears at `<collection>/flows/<slug>.yml` and is picked up by git status.
- Running the Flow executes nodes in dependency order, calls the same `RequestExecutionService::execute` path as a normal Send, and writes a `HistoryEntry` per executed request.
- Per-node status (idle/running/success/failure) updates live on the canvas during a run, matching the approved mockup's visual states.
- A node that fails causes its downstream dependents (and only those) to show as skipped; independent branches still run and report their own results.
- A cyclic graph cannot be saved; the error identifies the offending connection.
- Reopening the app and the same collection reloads a previously saved Flow with its nodes, edges, and canvas positions intact.
