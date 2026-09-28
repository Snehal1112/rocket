# Flow Phase 2 — Condition/Branch Nodes and Non-Linear Execution — Design Spec

**Date:** 2026-09-28
**Status:** Approved in brainstorming — pending written-spec review
**Tracks:** GitHub issue #31
**Builds on:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md` (Phase 1), whose §"Out of scope" deferred this work to Phase 2.

**Out of scope (unchanged or newly deferred):**
- Loops, retries, "repeat until" — the graph stays acyclic; a cycle remains a save-time error.
- A dedicated Merge node — not needed; the per-field join rule (§6.3) makes if/else paths merge naturally.
- A general node inspector / editing panel (#26) — If/Switch nodes are edited inline in the node body; other node kinds are unaffected.
- Parallel execution of independent branches — still sequential (Phase 1 §3).
- Click-to-edit of an existing wire's expression — unchanged from Phase 1.
- Transform/script nodes — Phase 3.

---

## 1. Background

Phase 1 executes a Flow as a linear DAG: every node runs in topological order, and every edge is an unconditional data wire. The only non-linearity is failure containment — when a node fails, `FlowExecutionService::run` adds `reachable_from(flow, failed_id)` to a precomputed `skipped` set and records those nodes as `Skipped` with `error: "upstream node failed"`.

There is no way to express "if the login returned 200 call A, otherwise call B". Phase 2 adds that: two routing node kinds, If and Switch, and the execution semantics needed to skip the branches that are not taken — reported distinctly from failure skips.

Relevant current code (verified 2026-09-28 at `dbd8e1e9`):

| Area | Location |
|---|---|
| Node/edge/flow types | `crates/rocket-flow/src/node.rs`, `flow.rs` |
| Graph validation | `crates/rocket-flow/src/graph.rs` — `topological_sort` (:28), `reachable_from` (:188), `FlowGraphError` |
| Save-time validation | `crates/rocket-app/src/flow_service.rs:25` (`FlowService::save` → `topological_sort`) |
| Executor | `crates/rocket-app/src/flow_execution_service.rs` — `run` (:399), `execute_node` (:532), `result_to_step` (:642), `apply_wired_overrides` (:169) |
| Wire evaluation | `RequestExecutionService::resolve_flow_wire_expression` → `evaluate_var_expression` (`execution_service.rs:1493`) |
| Events | `crates/rocket-shared/src/events.rs` — `FlowNodeStatus` (:5), `FlowStep*`/`FlowRun*` (:161+) |
| Persistence | `crates/rocket-infra/src/fs_flow_repo.rs` — `serde_yaml` of the domain `Flow`, snake_case |
| IPC | `src-tauri/src/commands/flow.rs` — `FlowDto`, `FlowNodeKindDto`, `FlowEdgeDto`, … |
| Frontend | `src/components/flow/` (`FlowPane`, `FlowCanvas`, `FlowToolbar`, `NodePalette`, `WireExpressionPopover`, `nodes/*`), `src/lib/flow-wiring.ts`, `src/lib/tauri-api.ts:1728-1886`, `FlowTab` in `src/types/pane-types.ts:155` |

## 2. Goals

- A user can add an **If** node (one boolean condition, `true`/`false` exits) and a **Switch** node (one value, N named cases plus `default`) from the node palette.
- A user can wire any node's output into an If/Switch node, and wire an If/Switch exit into downstream nodes — either into a data field (URL/header/body) or into a new data-less **"Run when"** (`trigger`) input.
- When a Flow runs, only the chosen exit's downstream nodes run; nodes that depend only on non-chosen exits are skipped with a reason distinct from failure skips.
- if/else paths can rejoin into a single downstream node without duplicating it.
- The canvas shows which exit each routing node took, which edges were live, and why each skipped node was skipped.
- Every Phase 1 flow file continues to load, run with the same per-node statuses, and re-save byte-identically.

## 3. Non-goals

See the out-of-scope list above. Additionally: no new Tauri commands, no change to the OpenCollection files (Flow files remain a Rocket-specific extension under `<collection>/flows/`), no change to how Request nodes execute HTTP (still `RequestExecutionService::execute_with_external_secrets`).

## 4. Chosen approach

Three representations were considered:

1. **Named exits on edges (chosen).** Add an optional `source_handle` to `FlowEdge`. Routing nodes expose named exits; the executor tracks which edges are live in a run. Smallest model change, backward compatible, maps directly onto React Flow's multi-handle nodes, and fits both If and Switch.
2. Separate control-edge list — two edge types to draw, persist, validate and explain; duplicates graph logic. Rejected.
3. Conditions on edges (a `when` expression per wire), no new nodes — a switch becomes N wires each repeating a check; conflicts with the decision to have dedicated nodes. Rejected.

## 5. Data model (`rocket-flow`)

### 5.1 Node kinds

```rust
#[serde(tag = "kind")]
pub enum FlowNodeKind {
    Request { label: String, source: RequestSource },   // unchanged
    Input   { label: String, value: VariableValue },    // unchanged
    Output  { label: String },                          // unchanged
    /// Routes to the `true` exit when `!!(condition)` is true, otherwise `false`.
    If { label: String, condition: String },
    /// Routes to the first case whose `matches` equals `String(value)`, otherwise `default`.
    Switch { label: String, value: String, cases: Vec<SwitchCase> },
}

pub struct SwitchCase {
    /// Stable id (ULID). Edges reference the case by id, so renaming a case never breaks wires.
    pub id: String,
    /// Display name on the canvas, e.g. "Pro plan".
    pub label: String,
    /// Exact string compared against `String(value)`, e.g. "pro".
    pub matches: String,
}
```

`SwitchCase` derives the same traits as the other node types (`Debug, Clone, PartialEq, Serialize, Deserialize`), no `rename_all` (persistence struct — snake_case on disk).

### 5.2 Edge exit

```rust
pub struct FlowEdge {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub target_field: String,
    pub expression: String,
    /// Which exit of the source node this edge leaves from.
    #[serde(default = "default_source_handle", skip_serializing_if = "is_result_handle")]
    pub source_handle: String,
}
```

### 5.3 Handle vocabulary

Constants live in a new `rocket_flow::handle` module so the executor, validator and tests share one spelling:

| Constant | Value | Where |
|---|---|---|
| `RESULT` | `"result"` | exit of Request / Input (default for every edge) |
| `TRUE` / `FALSE` | `"true"` / `"false"` | exits of If |
| `DEFAULT` | `"default"` | fallback exit of Switch |
| `case_handle(id)` | `"case:<id>"` | per-case exit of Switch |
| `INPUT` | `"input"` | the single data input (`target_field`) of If / Switch |
| `TRIGGER` | `"trigger"` | data-less "Run when" input (`target_field`) of Request / Output |

`target_field` values `url`, `body`, `headers[N].value`, `headers[Name].value` (Request) and `value` (Output — see §5.4) are unchanged.

### 5.4 Output node input name

Phase 1's Output node renders a target handle named `value`, but the executor ignores `target_field` for Output nodes (it only counts incoming edges). Phase 2 formalizes this: an Output node accepts `value` (data) and `trigger` (gate only). Existing Output edges saved with `target_field: value` remain valid.

### 5.5 Backward compatibility

- A Phase 1 file has no `source_handle` keys → every edge defaults to `result`, which is correct for every Phase 1 source kind.
- `source_handle` is omitted when it equals `result`, so loading and re-saving a Phase 1 flow produces a byte-identical file (no git noise).
- New node kinds only appear in files that use them.
- **One intentional runtime change:** a Phase 1 flow with two wires into the *same* Request field (Phase 1 silently used the last one) now fails that node with `field '<f>' has 2 live inputs` (§6.3). Every other Phase 1 flow runs with the same per-node statuses.

### 5.6 Example file

```yaml
name: Login flow
nodes:
  - id: login
    kind: { kind: Request, label: Login, source: { type: Saved, request_path: auth/login } }
    position: { x: 0, y: 0 }
  - id: 01J9IF
    kind: { kind: If, label: "Logged in?", condition: "response.status === 200" }
    position: { x: 300, y: 0 }
  - id: profile
    kind: { kind: Request, label: Get Profile, source: { type: Saved, request_path: users/me } }
    position: { x: 600, y: -100 }
edges:
  - { id: e1, source_node_id: login, target_node_id: 01J9IF, target_field: input, expression: "" }
  - id: e2
    source_node_id: 01J9IF
    source_handle: "true"
    target_node_id: profile
    target_field: headers[Authorization].value
    expression: "'Bearer ' + response.body.token"
```

Note `e2`: the If node passes its input through (§6.4), so a wire leaving an If exit evaluates against the **Login** response.

## 6. Execution semantics (`FlowExecutionService`)

### 6.1 Overview

Execution stays sequential in topological order. What changes is how a node decides whether to run: instead of consulting a `skipped` set precomputed from `reachable_from` on failure, each node decides **at its turn** from the recorded outcomes of its direct predecessors. Topological order guarantees every predecessor is decided first.

Per-run state:

```rust
enum NodeOutcome {
    Succeeded { chosen_exit: String },     // "result" for plain nodes
    /// `responded` is true when a Request failed only because of a non-2xx
    /// status: it has a captured response that routing nodes may observe (§6.3.1).
    Failed { responded: bool },
    Skipped(SkipReason),
}
captured: HashMap<String, CapturedOutput>  // unchanged
outcomes: HashMap<String, NodeOutcome>     // new; replaces the `skipped: HashSet`
```

`reachable_from` is no longer used by the executor. It stays in `rocket-flow` (tested, public via `graph::`) — removal is not required by this work.

### 6.2 Edge liveness

An edge `e` is **live** iff either:

- `outcomes[e.source_node_id]` is `Succeeded { chosen_exit }` **and** `chosen_exit == e.source_handle`; or
- **(failure observation, §6.3.1)** `outcomes[e.source_node_id]` is `Failed { responded: true }` **and** the target is an If/Switch node **and** `e.target_field == "input"`.

For plain nodes `chosen_exit` is always `result`, and their edges always have `source_handle == result`, so the first clause reduces to "source succeeded" — Phase 1 behaviour.

### 6.3 Deciding a node's fate (per-field join rule)

For each node in topological order, the first matching rule wins:

1. **Cancelled** → stop the run (unchanged: `stopped_reason = "cancelled"`, no step recorded for this or later nodes).
2. **No incoming edges** → run.
3. **Any incoming edge that is not live has a source that is `Failed { .. }` or `Skipped(UpstreamFailed)`** → `Skipped(UpstreamFailed)`. Failure containment stays strict: a failed node poisons everything downstream, even through a join. The only exception is an edge made live by failure observation (§6.3.1).
4. **Group incoming edges by `target_field`. If any group contains no live edge** → `Skipped(BranchNotTaken)`.
5. **Otherwise** → run, using **only live edges**. Dead edges are ignored entirely (by rule 4 every wired field has at least one live edge, so no wired field is ever left unfed).

Rationale: several wires into the **same** field are alternatives (if/else merge — one live is enough); wires into **different** fields are all required (a request never fires with a missing wired token). Worked cases:

```
Case 1 — merge (false taken):
Login ✓ ─▶ If ✓(false) ─ true  ─▶ Get Profile ⊘ not taken ─┐ body
                       └ false ─▶ Refresh ✓ ────────────────┴▶ Save Result ✓   (body has 1 live edge)

Case 2 — accidental join (false taken):
Config ✓ ───────────────────────────────────┐ url
Login ✓ ─▶ If ✓(false) ─ true ─▶ Get Token ⊘ ┴ headers[Authorization].value ─▶ Call API ⊘ not taken
                                                (url live, header group has no live edge)
```

#### 6.3.1 Failure observation (routing on error responses)

A Request that receives a non-2xx response is still recorded as **failed** (red on the canvas, counted in `failed_count`, plain dependents skipped as `upstream_failed`). But its response is **captured** (`captured[id] = Request(output)`, which Phase 1 did not store for failed nodes), and it is recorded as `Failed { responded: true }`. An If/Switch whose `input` is wired from it treats that edge as live, receives the response, and routes normally — so "if login returns 401, refresh the token" is expressible:

```
Login ✕ 401 ─▶ If (status === 200) ✓(false) ─ false ─▶ Refresh Token ✓
             └─▶ Get Profile ⊘ upstream failed       (plain dependent: still skipped)
```

Failures with no response — transport/execution errors, wire-expression errors, a Saved request that no longer resolves, a routing expression that throws — are `Failed { responded: false }`: nothing is captured and routing nodes downstream are skipped as `upstream_failed`, as before.

**Ambiguity:** if a field group has **more than one live edge** (e.g. two independent If nodes both routing into `body`), the node **fails** with `field '<target_field>' has <n> live inputs`. This generalizes Phase 1's "Output node with two incoming wires fails instead of dropping one". It also applies to two **plain** wires into the same Request field, which Phase 1 silently resolved as last-wins — an intentional behaviour change (such a flow is almost certainly a wiring mistake), called out in §5.5. The `trigger` group is exempt: several live triggers are fine (a trigger carries no data, so there is nothing to disambiguate).

**Trigger edges** participate only in rules 3–4; they are not passed to `apply_wired_overrides` and their `expression` is ignored (stored as `""`).

### 6.4 Running an If node

1. Its single live `input` edge's source is `S` (validation guarantees exactly one `input` edge; rule 4 guarantees it is live). `S` may be a Request that failed with a non-2xx response (§6.3.1); its captured response is used exactly like a successful one.
2. Evaluate `!!(<condition>)` against `captured[S]` using the same context as wire expressions (`resolve_flow_wire_expression`'s `response.*` shape — a `Value` output is wrapped as a synthetic `HttpResponse { status: 200, body }`).
3. Result `"true"` → `chosen_exit = "true"`, `"false"` → `"false"`.
4. `captured[if_id] = captured[S].clone()` — **pass-through**, so downstream wires can read the upstream response.
5. An evaluation error (script throws, invalid syntax) → the If node is `Failed` with the engine's message; everything downstream becomes `Skipped(UpstreamFailed)` by rule 3.

### 6.5 Running a Switch node

1. Same input resolution as If.
2. Evaluate `String(<value>)`. `null`/`undefined` become `"null"`/`"undefined"` — never an error, and matchable by a case.
3. `chosen_exit` = `case:<id>` of the **first** case (in `cases` order) whose `matches == result`; otherwise `default`.
4. Pass-through and failure behaviour as If.

### 6.6 Evaluation entry point

Add a string-returning sibling to the wire evaluator on `RequestExecutionService`:

```rust
pub async fn evaluate_flow_route_expression(
    &self, collection: &str, output: &CapturedOutput, wrapped_expression: &str,
) -> DomainResult<String>
```

It reuses `evaluate_var_expression` exactly as `resolve_flow_wire_expression` does, except that it does **not** treat a `null` result as an error (the wrapping `!!(…)` / `String(…)` guarantees a non-null string anyway; the `null` check is simply skipped for robustness). The executor passes `format!("!!({})", condition)` or `format!("String({})", value)`.

### 6.7 Request and Output nodes

- Request: `apply_wired_overrides` receives only live, non-`trigger` edges. Unchanged otherwise.
- Output: data comes from its single live `value` edge (0 live `value` edges and only triggers → `Value("")`, matching Phase 1's no-edge behaviour). More than one live `value` edge fails per §6.3.

### 6.8 Step results

`result_to_step` and the skip path populate two new optional fields (§8): `skip_reason` for skipped nodes, `branch` for succeeded If/Switch nodes. Skipped steps no longer set `error: "upstream node failed"` — `skip_reason` is authoritative.

`FlowStepStarted` is still published only for nodes that actually run (never for skipped ones) — unchanged.

## 7. Validation

A new public `rocket_flow::validate(flow: &Flow) -> Result<Vec<String>, FlowGraphError>` returns the topological order on success. It runs `topological_sort` (existing duplicate/unknown/cycle checks) and then these structural rules:

| # | Rule | Error |
|---|---|---|
| V1 | If/Switch has exactly one incoming edge, and its `target_field` is `input` | `InvalidNode` |
| V2 | `target_field == "input"` only targets If/Switch | `InvalidEdge` |
| V3 | `target_field == "trigger"` only targets Request/Output | `InvalidEdge` |
| V4 | No edge targets an Input node | `InvalidEdge` |
| V5 | `source_handle` exists on the source: Request/Input → `result`; If → `true`/`false`; Switch → `default` or `case:<id>` of an existing case; Output → none | `InvalidEdge` |
| V6 | Switch `cases[].id` unique | `InvalidNode` |
| V7 | Switch `cases[].matches` unique | `InvalidNode` |
| V8 | If `condition` / Switch `value` non-empty after trim | `InvalidNode` |

New variants:

```rust
pub enum FlowGraphError {
    Cycle { node_ids: Vec<String>, edge_ids: Vec<String> },   // existing
    UnknownNode { node_id: String },                           // existing
    DuplicateNode { node_id: String },                         // existing
    InvalidNode { node_id: String, reason: String },
    InvalidEdge { edge_id: String, reason: String },
}
```

All variants' `Display` end with the tail already parsed by the frontend's `parseCycleErrorMessage`: `…node(s): <ids>; edge(s): <ids>` (either list may be empty). This lets the existing red-highlight path flag the offending node/edge for **every** validation error, not only cycles. `validate` reports the **first** violation found (deterministic: rules in table order, nodes/edges in file order).

Callers:
- `FlowService::save` calls `validate` instead of `topological_sort` (→ `DomainError::InvalidInput`, never persisted).
- `FlowExecutionService::load_ordered_nodes` calls `validate` instead of `topological_sort`, so a hand-edited invalid file fails to run with the same message.

Phase 1 files cannot violate V1–V8: the Phase 1 UI never creates edges into Input nodes or out of Output nodes, and has no If/Switch nodes.

## 8. Events and IPC

### 8.1 Domain events (`rocket-shared/src/events.rs`)

`FlowNodeStatus` is **unchanged** (`running | success | failed | skipped`). New:

```rust
#[serde(rename_all = "snake_case")]
pub enum FlowSkipReason { UpstreamFailed, BranchNotTaken }

FlowStepCompleted {
    run_id, node_id, status, status_code, duration_ms, error, value,   // existing
    #[serde(default, skip_serializing_if = "Option::is_none")] skip_reason: Option<FlowSkipReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")] branch: Option<String>,
}
FlowRunFinished {
    run_id, stopped_reason, node_count, failed_count, skipped_count,   // existing; skipped_count = total
    #[serde(default)] not_taken_count: usize,
}
```

### 8.2 Run summary (`rocket-app`)

`FlowStepResult` (camelCase IPC struct) gains `skip_reason: Option<FlowSkipReason>` and `branch: Option<String>`, both `#[serde(default, skip_serializing_if = "Option::is_none")]` → `skipReason`, `branch`.

### 8.3 DTOs (`src-tauri/src/commands/flow.rs`)

- `FlowNodeKindDto` gains `If { label, condition }` and `Switch { label, value, cases: Vec<SwitchCaseDto> }` (same `tag = "kind"`, `rename_all_fields = "camelCase"`), with `From` impls both ways.
- New `SwitchCaseDto { id, label, matches }` — `#[serde(rename_all = "camelCase")]`.
- `FlowEdgeDto` gains `source_handle: String` → `sourceHandle`, `#[serde(default = "…result")]`.
- No new commands.

### 8.4 Frontend types (`src/lib/tauri-api.ts`)

```ts
export interface SwitchCase { id: string; label: string; matches: string }
export type FlowNodeKind =
  | { kind: 'Request'; label: string; source: RequestSource }
  | { kind: 'Input'; label: string; value: unknown }
  | { kind: 'Output'; label: string }
  | { kind: 'If'; label: string; condition: string }
  | { kind: 'Switch'; label: string; value: string; cases: SwitchCase[] };
export interface FlowEdge { /* existing */ sourceHandle?: string }   // absent ⇒ 'result'
export type FlowSkipReason = 'upstream_failed' | 'branch_not_taken';
// FlowStepResult gains: skipReason?: FlowSkipReason; branch?: string
// FlowStepCompletedEvent gains: skip_reason?: FlowSkipReason; branch?: string
// FlowRunFinishedEvent gains: not_taken_count?: number
```

## 9. Frontend

### 9.1 New node components (`src/components/flow/nodes/`)

No node inspector exists (#26), so routing nodes are edited **inline**; edits call `updateFlowNodes` and mark the tab dirty. All controls are shadcn primitives; icons from `lucide-react` (`GitBranch` for If, `Split` for Switch); expression fields use `SingleLineEditor`.

**`IfNode.tsx`**
```
┌──── ⑂ If  Logged in? ─────────────────────┐
○ input                                      │
│  condition  [ response.status === 200  ]   │
│                                  true  ●  │ (green)
│                                  false ●  │ (muted)
└─────────────────────────────────────────────┘
```

**`SwitchNode.tsx`**
```
┌──── ⑃ Switch  Plan router ──────────────────┐
○ input                                        │
│  value  [ response.body.plan ]               │
│  [Free     ] = [free      ]  ✕     ●        │  case:<id>
│  [Pro plan ] = [pro       ]  ✕     ●        │  case:<id>
│  + Add case                                   │
│                                  default ●   │
└───────────────────────────────────────────────┘
```

- New case: `{ id: ulid(), label: "Case N", matches: "" }`. Deleting a case removes every edge whose `sourceHandle === 'case:<id>'` in the same store update.
- Defaults on creation: If `condition = "response.status === 200"`; Switch `value = "response.body.type"`, one empty case.

**Request and Output nodes** gain a small "Run when" target handle (`id="trigger"`) at the top of the left edge.

### 9.2 Wiring

- `buildEdgeFromConnection` keeps `connection.sourceHandle` (stored only when not `'result'`, mirroring the backend omission).
- `toRfEdges` maps `sourceHandle: e.sourceHandle ?? 'result'` (replacing the hard-coded `'result'`).
- Connections into `input` or `trigger` create the edge with `expression: ''` and **skip** `WireExpressionPopover`.
- `isValidConnection` mirrors V1–V5 client-side (e.g. rejects a second wire into an If/Switch `input`). The backend `validate` remains authoritative.
- Edges leaving If/Switch carry a React Flow `label`: `true`, `false`, the case's `label`, or `default`.
- `parseCycleErrorMessage` is renamed `parseGraphErrorMessage` (same tail format, §7) and now fires for any `save_flow` validation error.

### 9.3 Run visualization

- Extract `statusStyles` from `RequestNode.tsx` into a shared `nodeStatus.ts` helper and apply it to **all** node kinds (today only Request styles by status).
- Skipped nodes are styled by reason: `upstream_failed` keeps the current faded style with caption *Skipped — upstream failed*; `branch_not_taken` uses faded + dashed border with caption *Not taken*.
- A succeeded If/Switch shows a badge: `→ true`, `→ false`, `→ <case label>`, `→ default`.
- After (and during) a run, edges from a routing node whose `sourceHandle` equals that node's recorded `branch` are highlighted; its other exits' edges are dimmed and dashed. Plain edges are unchanged.
- `FlowTab.nodeDetail` type becomes `{ statusCode?; durationMs?; error?; value?; skipReason?; branch? }` — also fixing the existing omission of `value` from the type.
- `FlowToolbar` passes `skip_reason`/`branch` from `flow-step-completed` and `skipReason`/`branch` from the final summary into `patchFlowNodeStatus`.

### 9.4 Palette

`NodePalette` gains **If** and **Switch** entries (same `{x:100,y:100}` placement and id scheme as existing entries).

## 10. Error handling

- Invalid graphs (V1–V8, cycles) are rejected at save with a typed `DomainError::InvalidInput`; the canvas flags the named node/edge in red.
- A routing expression that throws fails the routing node; dependents are skipped as `upstream_failed`.
- Ambiguous live inputs fail the target node with a message naming the field.
- A hand-edited file that fails `validate` cannot run; `run_flow` returns the same message.
- No panicking unwraps in new production paths; all errors flow through `DomainResult`.

## 11. Testing

**`rocket-flow`** (pure unit tests)
- Serde: If/Switch round-trip; `source_handle` defaults to `result` when absent; omitted on serialize when `result`; present otherwise.
- `validate`: one passing and one failing test per rule V1–V8; error `Display` ends with the `node(s): …; edge(s): …` tail; first-violation determinism.

**`rocket-app` executor** (existing fakes: `FixedJsonqEngine`, `RecordingPublisher`, `UrlAwareExecutor`, `CancelAfterSteps`, …)
- If true → true-exit dependents run, false-exit dependents `branch_not_taken`; and vice versa.
- Switch: first matching case wins; no match → `default`; `null` value routes to a case matching `"null"`.
- Pass-through: a wire leaving an If exit evaluates against the If's input response.
- Per-field join: Case 1 merge runs; Case 2 skips as `branch_not_taken`.
- Two live edges into one field → target fails with the field name; several live triggers → runs.
- Strict failure through a join: failed node on one arm + not-taken other arm → join node `upstream_failed`.
- Routing expression error → routing node failed, dependents `upstream_failed`.
- Failure observation: a 401 Request wired into an If routes to `false` (If runs, false-arm runs), while the Request's plain dependents are `upstream_failed` and the Request itself is `failed`; a transport error (no response) still skips the If as `upstream_failed`.
- Two plain wires into the same Request field → the node fails with `field '<f>' has 2 live inputs` (intentional Phase 1 change).
- Transitive not-taken: a not-taken node's dependents are `branch_not_taken` (not `upstream_failed`).
- Cancellation before a routing node; `FlowStepStarted` never published for a skipped node.
- Events: `skip_reason`, `branch`, `not_taken_count`, `skipped_count` totals.
- A Phase 1 linear flow produces an identical summary to Phase 1 (regression), apart from `skip_reason` replacing the old `error` text on skipped steps.

**`rocket-infra`**: a Phase 1 fixture `.yml` loads and re-saves byte-identically; an If/Switch flow round-trips; no camelCase keys on disk.

**`src-tauri`**: DTO camelCase shape for If/Switch/`sourceHandle`/`skipReason`; DTO ↔ domain round-trip.

**`rocket-shared`**: exact JSON wire shape of `FlowStepCompleted` with and without the new fields; `FlowRunFinished.not_taken_count`.

**Frontend (Vitest)**
- `IfNode`/`SwitchNode` render handles, inline edits dispatch node updates, add/remove case (removal drops its edges).
- Status styling helper applied to every node kind; skip-reason captions; branch badge.
- `buildEdgeFromConnection` preserves `sourceHandle`; `input`/`trigger` connections skip the popover.
- `isValidConnection` rules.
- `toRfEdges` maps `sourceHandle`, labels, and taken/not-taken styling.
- `FlowToolbar` applies `skip_reason`/`branch` from events and summary.
- `parseGraphErrorMessage` highlights for a non-cycle validation error.

**Verification:** `cargo check -j4`, `cargo test -j4 -p rocket-flow -p rocket-app -p rocket-infra -p rocket-shared`, the `src-tauri` crate's tests, `yarn tsc --noEmit`, `yarn check`, `yarn test flow`.

## 12. Implementation plan split

Mirroring the Phase 1 series, under `docs/superpowers/plans/flow-phase2-branching/`:

1. **Domain model and validation** — `rocket-flow`: node kinds, `SwitchCase`, `source_handle`, `handle` module, `validate`, new error variants.
2. **Executor semantics** — `rocket-app`: `NodeOutcome`, liveness, per-field join, If/Switch execution, route evaluator, `FlowService`/load use `validate`.
3. **Events, DTOs and IPC** — `rocket-shared` events, `FlowStepResult`, `src-tauri` DTOs.
4. **Frontend types, store and wiring** — TS types, `FlowTab.nodeDetail`, `buildEdgeFromConnection`, `toRfEdges`, `isValidConnection`, trigger handles, `parseGraphErrorMessage`, toolbar event mapping.
5. **Routing node components and run visualization** — `IfNode`, `SwitchNode`, palette, shared status styling, skip captions, branch badge, edge highlighting.

Plans 1 → 2 → 3 are sequential (each depends on the previous types). Plan 4 depends on 3's wire shapes; plan 5 depends on 4.

## 13. Acceptance criteria

- A user can add an If node and a Switch node from the palette, edit the condition/value and cases inline, wire an upstream node into them, and wire their exits into downstream data fields or "Run when" inputs; the flow saves and reloads intact.
- Running a flow with an If node executes only the chosen exit's dependents; the others show *Not taken*, distinct from *Skipped — upstream failed*.
- A Switch routes to the first matching case, or `default` when none match.
- An If wired from a request that returned a non-2xx status still runs and can route on that status (e.g. 401 → refresh token), while the request itself shows as failed.
- if/else paths rejoining into one field of one node run that node once; a node needing a value from a not-taken branch in a different field is skipped as not taken.
- The canvas shows each routing node's chosen exit and highlights live vs. dimmed exit edges.
- Invalid graphs (e.g. two inputs into an If, a wire from a deleted case) cannot be saved; the offending node/edge is highlighted.
- Existing Phase 1 flows load, run with identical per-node statuses (skipped steps now carry `skip_reason: upstream_failed` instead of the old `error` text; a node with two wires into the same field now fails — §5.5), and re-save byte-identically.
