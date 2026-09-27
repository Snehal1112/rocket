# Flow Visual Workflow Builder — Plan Index

**Spec:** [../../specs/2026-09-27-flow-visual-workflow-builder-design.md](../../specs/2026-09-27-flow-visual-workflow-builder-design.md)

**Context:** Phase 1 (MVP) of "Flow" — a node-canvas visual API workflow
builder, distinct from the existing linear, tree-order Collection Runner
(`crates/rocket-app/src/collection_runner_service.rs`). Condition/branch
nodes, transform/script nodes, and non-linear execution are explicitly
deferred to later phases (see the spec's scope section) — none of the plans
below implement them.

## Plan breakdown — 10 plans, 23 tasks (max 3 per plan)

| # | Plan | Tasks | Crate/area | Depends on |
|---|---|---|---|---|
| 01 | [rocket-flow: core node/edge types](2026-09-27-flow-visual-workflow-builder-plan-01-domain-types.md) | 2 | `rocket-flow` (new) | — |
| 02 | [rocket-flow: graph validation](2026-09-27-flow-visual-workflow-builder-plan-02-graph-validation.md) | 1 | `rocket-flow` | 01 |
| 03 | [FsFlowRepo persistence](2026-09-27-flow-visual-workflow-builder-plan-03-persistence.md) | 2 | `rocket-infra` | 01 |
| 04 | [Flow domain events](2026-09-27-flow-visual-workflow-builder-plan-04-domain-events.md) | 1 | `rocket-shared` | — |
| 05 | [Wiring resolution + request building](2026-09-27-flow-visual-workflow-builder-plan-05-wiring-and-request-building.md) | 3 | `rocket-app` | 01 |
| 06 | [FlowExecutionService](2026-09-27-flow-visual-workflow-builder-plan-06-execution-service.md) | 3 | `rocket-app` | 01, 02, 03, 04, 05 |
| 07 | [Tauri Flow commands](2026-09-27-flow-visual-workflow-builder-plan-07-tauri-commands.md) | 3 | `src-tauri` | 02, 03, 04, 06 |
| 08 | [Frontend: types, bindings, FlowTab](2026-09-27-flow-visual-workflow-builder-plan-08-frontend-types-and-tab.md) | 2 | frontend | 07 |
| 09 | [Frontend: canvas + node components](2026-09-27-flow-visual-workflow-builder-plan-09-frontend-canvas-nodes.md) | 3 | frontend | 08 |
| 10 | [Frontend: wiring UI + run controls](2026-09-27-flow-visual-workflow-builder-plan-10-frontend-wiring-and-run.md) | 3 | frontend | 09 |

Run the plans in numeric order. Plan 02 and Plan 04 are single-task plans —
each is one genuinely atomic deliverable (a pure algorithm; a set of event
variant definitions) with no natural second slice, the same reasoning the
ACP plan index used for its own single-task plans.

Each plan file ends with a **Next Plan** section linking the file above, and
a **Post-Implementation Review** section: before moving to the next plan,
dispatch an Opus-model subagent (Agent tool, `model: "opus"`) to review
everything that plan added or modified for interface gaps, code quality,
duplication, and DDD boundary conformance, with explicit authority to fix
what it finds directly (not just report it) before the next plan starts.
Within a plan, use `superpowers:subagent-driven-development` so a
complexity-appropriate subagent implements each task (a fresh subagent per
task, reviewed before the next task starts) — the plan's own parent/Opus
review pass in the "Post-Implementation Review" section happens once, after
all of that plan's tasks are done, not per-task.

## Key design resolutions locked at planning time

**Superseded by Plans 05-06 — read those plans' own "Corrections to the plan
index" sections for the authoritative, source-grounded versions of the two
points below.** Left here only so the history of what changed is visible;
Plan 07 and any other plan referencing this section must use the corrected
versions, not this one.

- ~~`InlineRequestData` does not reuse `rocket-http::HttpRequest`~~ — still
  true, unchanged. `rocket-flow` has zero cross-domain-crate dependencies;
  `InlineRequestData` stays a minimal, `rocket-flow`-local shape (Plan 01).
- ~~There is no existing Rust "Request → ExecuteRequestInput" mapping to
  reuse~~ — **wrong, corrected by Plan 05.** It exists:
  `crate::runner_sequence::build_step_input` (`crates/rocket-app/src/runner_sequence.rs:112`),
  already used by the Collection Runner for the identical problem. Plan 05
  Task 2 reuses it directly (for both `Saved` and `Inline` sources, the
  latter via a small `InlineRequestData → rocket_collection::Request`
  adapter) instead of writing a second, parallel mapping.

Two further corrections Plans 05-06 made to signatures this index originally
sketched:

- `RequestExecutionService::resolve_flow_wire_expression` takes an additional
  `collection: &str` parameter (Plan 05 Task 1) — required because the
  jsonq mechanism it wraps (`evaluate_var_expression`) needs a collection
  root to resolve collection-scope variables.
- `FlowExecutionService` needs **no `MAX_RUN_STEPS`-equivalent guard**
  (Plan 06) — Flow's topological traversal has no jump mechanism that could
  loop forever, unlike the Collection Runner's `setNextRequest`. Its
  constructor also stays exactly `(flow_repo, collection_repo, events)` — no
  `EnvironmentRepository` — because Input-node `{{variable}}` resolution is
  intentionally scoped to collection-level variables only in Phase 1 (see
  Plan 06's "Corrections" section for the full justification).

**If Plan 07 (Tauri commands) was already written before this update, verify
it against Plan 05/06's actual signatures before implementing it** —
specifically `resolve_flow_wire_expression`'s extra `collection` parameter
(irrelevant to Plan 07 directly, but confirms the pattern) and
`FlowExecutionService::new`'s exact 3-argument constructor.

**Plan 07 made one further addition of its own, confirmed and folded in
here:** Flow CRUD (list/get/save/delete) and cycle validation do not belong
inline in the Tauri command layer per `.claude/rules/tauri-ipc-boundaries.md`
("no domain business logic in command modules"). Plan 07 adds a new
`FlowService` in `rocket-app` (`crates/rocket-app/src/flow_service.rs`,
separate module from `flow_execution_service.rs`), mirroring the existing
`CollectionService`/`CollectionRunnerService` split:

```rust
pub struct FlowService {
    flow_repo: Box<dyn rocket_flow::FlowRepository>,
}
impl FlowService {
    pub fn new(flow_repo: Box<dyn rocket_flow::FlowRepository>) -> Self;
    pub fn list(&self, collection: &str) -> DomainResult<Vec<String>>;
    pub fn get(&self, collection: &str, name: &str) -> DomainResult<Flow>;
    pub fn delete(&self, collection: &str, name: &str) -> DomainResult<()>;
    /// Runs `rocket_flow::topological_sort` before persisting; a
    /// `FlowGraphError::Cycle` is mapped to a `DomainError` that names the
    /// offending node ids, and nothing is written to disk in that case.
    pub fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()>;
}
```

`run_flow` also does not return a `run_id` immediately and stream events for
a caller to await separately — Plan 07 corrected this against the real
`runner.rs` prior art: it's a plain `async` Tauri command that stays
suspended until the run completes, publishing `FlowRunStarted` (carrying the
`run_id`) as a Tauri event mid-await, and returning the full
`FlowRunSummary` at the end — not the index's original
`Result<String, String>` sketch. `list_flows`/`get_flow`/`save_flow`/
`delete_flow`/`cancel_flow_run` all return `Result<T, DomainError>` (Tauri's
serde-based error bridging), not `Result<T, String>`.

## Locked interface contract

Every plan below is written against these exact types/signatures. If an
implementer needs to deviate, update this index and every downstream plan
file that references the changed name — do not let two plan files disagree
on a signature.

### `rocket-flow` (new, Plans 01–02)

```rust
// crates/rocket-flow/src/node.rs
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FlowNodeKind {
    Request { label: String, source: RequestSource },
    Input { label: String, value: VariableValue },   // VariableValue from rocket_shared
    Output { label: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RequestSource {
    /// Live reference into the collection tree — `request_path` is relative
    /// to the collection root, e.g. "auth/login.yml".
    Saved { request_path: String },
    /// A full ad hoc request embedded directly in the flow file.
    Inline { request: InlineRequestData },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineRequestData {
    pub method: String,             // "GET", "POST", etc. — plain string, not
                                      // rocket_shared::types::HttpMethod, to
                                      // keep rocket-flow dependency-free of
                                      // that enum's exact variant set; Plan 05
                                      // Task 2 parses it.
    pub url: String,
    #[serde(default)]
    pub headers: Vec<InlineHeader>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    pub kind: FlowNodeKind,
    pub position: NodePosition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowEdge {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    /// Path into the target node's own field set, e.g. "url",
    /// "headers[1].value", "body". Not a fixed enum — new wireable fields
    /// on a node type don't require a schema change here.
    pub target_field: String,
    /// JS expression evaluated against the source node's captured output
    /// (see Plan 05 Task 1) — reuses the existing jsonq/script-engine
    /// mechanism, not a new expression language.
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
}

pub trait FlowRepository: Send + Sync {
    fn list(&self, collection: &str) -> DomainResult<Vec<String>>; // flow names
    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow>;
    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()>;
    fn delete(&self, collection: &str, name: &str) -> DomainResult<()>;
}
```

```rust
// crates/rocket-flow/src/graph.rs (Plan 02)
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FlowGraphError {
    /// Nodes on a cycle (or on a path between two cycles); nodes that are
    /// only downstream of a cycle are excluded.
    #[error("cycle detected through node(s): {node_ids:?}")]
    Cycle { node_ids: Vec<String> },
    #[error("edge references unknown node: {node_id}")]
    UnknownNode { node_id: String },
    /// Added in the Plan 02 post-implementation review: duplicate ids would
    /// otherwise underflow in-degree counts (debug panic) or yield a node
    /// twice in the order (release). Checked before `UnknownNode`.
    #[error("duplicate node id: {node_id}")]
    DuplicateNode { node_id: String },
}

/// Kahn's-algorithm topological sort. Returns node ids in an order where
/// every node appears after all nodes it depends on (i.e. after every node
/// that has an edge pointing *into* it). Independent nodes/branches may
/// appear in either relative order.
pub fn topological_sort(flow: &Flow) -> Result<Vec<String>, FlowGraphError>;
```

**No `additionalProperties: false` schema constraint applies to `rocket-flow`
types** — unlike OpenCollection types, these are a Rocket-only extension
format (spec §5), so ordinary serde derives are sufficient; do not add
schema-validation code.

### `rocket-infra` (new, Plan 03)

```rust
// crates/rocket-infra/src/fs_flow_repo.rs
pub struct FsFlowRepo { /* base_dir: PathBuf, the collections dir FsCollectionRepo uses */ }
impl FsFlowRepo {
    pub fn new(base_dir: PathBuf) -> Self; // collections dir, not workspace root
}
impl FlowRepository for FsFlowRepo { /* ... */ }
// On-disk: <collection-dir>/flows/<slugify(flow.name)>.yml — mirrors
// FsCollectionRepo's directory-creation conventions (see
// FsCollectionRepo::save_request). Collection names go through
// Collection::validate_name. A name whose slug is empty is InvalidInput;
// saving a different name that shares an existing file's slug is Conflict;
// get/delete only match a file whose stored name equals the requested name.
```

### `rocket-shared` (modified, Plan 04)

```rust
// crates/rocket-shared/src/events.rs — new DomainEvent variants, added
// alongside the existing Collection Runner variants (RunnerStarted et al.)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeStatus {
    Running,
    Success,
    Failed,
    Skipped,
}

// New DomainEvent variants (added to the existing DomainEvent enum):
// FlowRunStarted { run_id: String, flow_name: String, collection: String, total_nodes: usize }
// FlowStepCompleted { run_id: String, node_id: String, status: FlowNodeStatus, status_code: Option<u16>, duration_ms: Option<u64>, error: Option<String> }
// FlowRunFinished { run_id: String, stopped_reason: String, node_count: usize, failed_count: usize, skipped_count: usize }
```

### `rocket-app` (new, Plan 05)

```rust
// crates/rocket-app/src/flow_execution_service.rs (module shared with Plan 06)
#[derive(Debug, Clone)]
pub enum CapturedOutput {
    Request(Box<ExecuteRequestOutput>),
    Value(rocket_shared::VariableValue),
}

impl RequestExecutionService {
    /// Evaluates `expression` (a jsonq/JS snippet, e.g. "response.body.token")
    /// against `output`, reusing the existing sandboxed-script evaluation
    /// path (`evaluate_var_expression`'s underlying mechanism). Returns the
    /// stringified result.
    pub async fn resolve_flow_wire_expression(
        &self,
        collection: &str,
        output: &CapturedOutput,
        expression: &str,
    ) -> DomainResult<String>;
}

/// Builds an `ExecuteRequestInput` for a `FlowNodeKind::Request` node, before
/// any wire overrides are applied. `Saved` resolves via
/// `collection_repo.get_request`; `Inline` maps `InlineRequestData` directly.
/// `environment_name`/`collection` populate the same-named `ExecuteRequestInput`
/// fields the run was invoked with.
pub fn build_execute_request_input(
    collection_repo: &dyn rocket_collection::CollectionRepository,
    collection: &str,
    environment_name: Option<&str>,
    node: &rocket_flow::FlowNode,
) -> DomainResult<ExecuteRequestInput>;

/// Mutates `input` in place, applying each edge's resolved value into the
/// field its `target_field` path names ("url", "headers[N].value", "body").
/// An out-of-range header index or unknown path segment is a `DomainError`,
/// not a silent no-op.
pub fn apply_wired_overrides(
    input: &mut ExecuteRequestInput,
    resolved: &std::collections::HashMap<String, String>, // edge_id -> resolved value
    edges: &[rocket_flow::FlowEdge],
) -> DomainResult<()>;
```

### `rocket-app` (new, Plan 06)

```rust
// crates/rocket-app/src/flow_execution_service.rs
pub struct RunFlowInput {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FlowStepResult {
    pub node_id: String,
    pub status: FlowNodeStatus,      // from rocket_shared::events
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FlowRunSummary {
    pub run_id: String,
    pub steps: Vec<FlowStepResult>,
    pub stopped_reason: String,
}

pub struct FlowExecutionService {
    flow_repo: Box<dyn rocket_flow::FlowRepository>,
    collection_repo: Box<dyn rocket_collection::CollectionRepository>,
    events: Box<dyn rocket_shared::events::EventPublisher>,
}
impl FlowExecutionService {
    pub fn new(
        flow_repo: Box<dyn rocket_flow::FlowRepository>,
        collection_repo: Box<dyn rocket_collection::CollectionRepository>,
        events: Box<dyn rocket_shared::events::EventPublisher>,
    ) -> Self;

    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
    ) -> DomainResult<FlowRunSummary>;

    pub fn cancel(&self, run_id: &str);
}
```

`run()` mirrors `CollectionRunnerService::run`'s outer shell (Ulid run id,
`exec.resolve_external_secrets` called once, cancellation checked per node,
a `MAX_RUN_STEPS`-equivalent guard reusing that same constant) but does
**not** share a base type with it (spec §7 — deliberately duplicated
scaffolding, not a forced abstraction). A failed node's downstream
dependents are marked `FlowNodeStatus::Skipped` and not executed;
independent branches continue.

### `src-tauri` (new, Plan 07)

```rust
// src-tauri/src/commands/flow.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDto { pub name: String, pub nodes: Vec<FlowNodeDto>, pub edges: Vec<FlowEdgeDto> }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNodeDto { pub id: String, pub kind: serde_json::Value, pub position: (f64, f64) }
// FlowNodeDto.kind carries FlowNodeKind's JSON shape opaquely (same tagged
// enum, camelCase re-keyed at the boundary is not required since the tagged
// variant fields are already the DTO's job to rename — see Plan 07 Task 1
// for the exact per-field camelCase mapping, not a raw passthrough).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEdgeDto { pub id: String, pub source_node_id: String, pub target_node_id: String, pub target_field: String, pub expression: String }

// list_flows(collection: String) -> Result<Vec<String>, String>
// get_flow(collection: String, name: String) -> Result<FlowDto, String>
// save_flow(collection: String, flow: FlowDto) -> Result<(), String>   // runs topological_sort first; a Cycle error surfaces as a distinguishable error string/code
// delete_flow(collection: String, name: String) -> Result<(), String>
// run_flow(collection: String, name: String, environment_name: Option<String>) -> Result<String, String>  // returns run_id; streams "flow-step-completed" / "flow-run-finished" via TauriEventBus
// cancel_flow_run(run_id: String) -> Result<(), String>
```

### Frontend (new, Plans 08–10)

**Superseded by Plan 08 — the block below is corrected against this repo's
actual conventions, verified by reading `src/types/pane-types.ts` and
`src/stores/pane-store.ts` directly (not the original sketch further down in
this section, kept only for history).** All Flow domain types live in
`src/lib/tauri-api.ts` (the same file `Header`/`Body`/`Auth`/etc. already
live in — there is no separate `src/types/flow-types.ts`), and `FlowTab`
uses this repo's real tab-discriminant convention:

```typescript
// src/lib/tauri-api.ts (Plan 08)
export type FlowNodeKind =
  | { kind: 'Request'; label: string; source: RequestSource }
  | { kind: 'Input'; label: string; value: unknown }
  | { kind: 'Output'; label: string };

export type RequestSource =
  | { type: 'Saved'; requestPath: string }
  | { type: 'Inline'; request: { method: string; url: string; headers: { name: string; value: string }[]; body?: string } };

export interface FlowNode { id: string; kind: FlowNodeKind; position: { x: number; y: number } }
export interface FlowEdge { id: string; sourceNodeId: string; targetNodeId: string; targetField: string; expression: string }
export interface Flow { name: string; nodes: FlowNode[]; edges: FlowEdge[] }

export type FlowNodeStatus = 'idle' | 'running' | 'success' | 'failed' | 'skipped';

// listFlows, getFlow, saveFlow, deleteFlow, runFlow, cancelFlowRun
// onFlowStepCompleted(runId, cb), onFlowRunFinished(runId, cb)

// src/types/pane-types.ts (Plan 08) — tabType/collectionName, NOT type/collectionRoot
// (collectionRoot is a distinct field ContractTab already uses for an absolute path)
export interface FlowTab extends BaseTab {
  tabType: 'flow';
  collectionName: string | null;
  flowName: string | null;
  nodes: FlowNode[];
  edges: FlowEdge[];
  nodeStatus: Record<string, FlowNodeStatus>;
  runState: 'idle' | 'running' | 'done';
  // Added by Plan 10: runId?: string (correlates streamed run events to the
  // active run) and nodeDetail?: Record<string, { statusCode?: number;
  // durationMs?: number; error?: string }> (status-code/timing shown on
  // each node per the approved mockup).
}

// pane-store.ts actions (Plan 08): openFlowTab, updateFlowNodes,
// updateFlowEdges, patchFlowNodeStatus(tabId, nodeId, status, detail?)
// (detail? added by Plan 10). Plan 10 also adds setFlowRunState(tabId,
// runState, runId?) — there is no generic updateFlowTab(tabId, patch)
// action; this repo's convention is one bespoke setter per concern
// (see updateRequest/updateTabTitle/toggleRunnerEntry in pane-store.ts).

// Registration: FlowPane is wired into src/components/panes/EditorGroup.tsx
// (NOT PaneRenderer.tsx, which only handles the resizable split/leaf tree),
// the same place RunnerPane is switched on.
```

## Execution note for whoever runs these plans

Run the plans in numeric order — each one's Global Constraints section
repeats the interfaces it consumes from earlier plans so it's runnable by a
fresh Claude Code session that has only read that one file, but the actual
code those interfaces reference won't exist yet if an earlier plan was
skipped. Use `superpowers:subagent-driven-development` per plan, per each
file's own header — a complexity-appropriate subagent per task within the
plan. After each plan's tasks are done, run that plan's
**Post-Implementation Review** step (an Opus-model subagent with authority
to fix what it finds) before starting the next plan.
