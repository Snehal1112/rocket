# Flow Phase 2 — Branching — Plan Index

**Spec:** [../../specs/2026-09-28-flow-phase2-branching-design.md](../../specs/2026-09-28-flow-phase2-branching-design.md)
**Issue:** #31

**Context:** Adds If/Switch routing nodes and non-linear execution to Flow
(Phase 1 spec: `../../specs/2026-09-27-flow-visual-workflow-builder-design.md`).
Loops, a Merge node, a general node inspector (#26), parallel execution and
Transform nodes are out of scope.

## Plan breakdown

| # | Plan | Crate/area | Depends on |
|---|---|---|---|
| 01 | [Domain model and validation](2026-09-28-flow-phase2-branching-plan-01-domain-model-and-validation.md) | `rocket-flow` (+ compile-keeping edits in `rocket-app`, `src-tauri`) | — |
| 02 | [Events and step results](2026-09-28-flow-phase2-branching-plan-02-events-and-step-results.md) | `rocket-shared`, `rocket-app` | 01 |
| 03 | [Executor routing semantics](2026-09-28-flow-phase2-branching-plan-03-executor-routing.md) | `rocket-app` | 01, 02 |
| 04 | [Frontend types, store and wiring](2026-09-28-flow-phase2-branching-plan-04-frontend-types-store-wiring.md) | frontend | 01, 02 |
| 05 | [Routing nodes and run visualization](2026-09-28-flow-phase2-branching-plan-05-routing-nodes-and-visualization.md) | frontend | 04 |

Run the plans in numeric order. **Deviation from spec §12:** events and
step-result fields (spec plan 3) move ahead of executor semantics (spec
plan 2), because the executor must emit `skip_reason`/`branch`, so those
fields have to exist first. The IPC DTO mirror of the new node kinds and
`sourceHandle` lands in plan 01, so the workspace compiles after every plan.

**Decisions made during planning (2026-09-28, reflected in the spec):**
- **Failure observation** (spec §6.3.1): a Request that failed only on a non-2xx status keeps its response, and an If/Switch `input` wired from it is live, so flows can route on error statuses. Plan 03 owns this.
- **Duplicate plain wires** into one field now fail the node instead of last-wins (spec §5.5). This is an intentional Phase 1 change.
- `FlowService::save` → `validate` is owned by plan 01 Task 4. `load_ordered_nodes` → `validate` is owned by plan 03.

## Cross-plan interface contract

Every plan uses exactly these names. A plan's own "Interfaces" blocks
repeat the parts it consumes or produces.

**`rocket-flow` (plan 01)**
- `pub mod handle` with `RESULT = "result"`, `TRUE = "true"`, `FALSE = "false"`, `DEFAULT = "default"`, `INPUT = "input"`, `TRIGGER = "trigger"`, `CASE_PREFIX = "case:"`, `pub fn case_handle(case_id: &str) -> String`, `pub fn case_id_from_handle(handle: &str) -> Option<&str>`.
- `FlowNodeKind::If { label: String, condition: String }` and `FlowNodeKind::Switch { label: String, value: String, cases: Vec<SwitchCase> }`.
- `pub struct SwitchCase { pub id: String, pub label: String, pub matches: String }`, re-exported from the crate root.
- `FlowEdge.source_handle: String`, with `#[serde(default = "default_source_handle", skip_serializing_if = "is_result_handle")]`.
- `pub fn validate(flow: &Flow) -> Result<Vec<String>, FlowGraphError>` in `validate.rs`, re-exported from the crate root.
- `FlowGraphError::InvalidNode { node_id: String, reason: String }` and `FlowGraphError::InvalidEdge { edge_id: String, reason: String }`.

**`rocket-shared` / `rocket-app` (plan 02)**
- `pub enum FlowSkipReason { UpstreamFailed, BranchNotTaken }` in `events.rs`, with `#[serde(rename_all = "snake_case")]`.
- `DomainEvent::FlowStepCompleted { …, skip_reason: Option<FlowSkipReason>, branch: Option<String> }`.
- `DomainEvent::FlowRunFinished { …, not_taken_count: usize }`.
- `FlowStepResult { …, skip_reason: Option<FlowSkipReason>, branch: Option<String> }`, serialized as camelCase `skipReason`/`branch`.

**`rocket-app` (plan 03)**
- `RequestExecutionService::evaluate_flow_route_expression(&self, collection: &str, output: &CapturedOutput, wrapped_expression: &str) -> DomainResult<String>`.

**Frontend (plan 04)**
- `tauri-api.ts` types as in spec §8.4.
- `src/lib/flow-handles.ts` exports `RESULT_HANDLE`, `TRUE_HANDLE`, `FALSE_HANDLE`, `DEFAULT_HANDLE`, `INPUT_HANDLE`, `TRIGGER_HANDLE`, `caseHandle(id)`, `caseIdFromHandle(handle)` and `isRoutingKind(kind)`.
- `src/lib/flow-wiring.ts` exports `parseGraphErrorMessage` (replacing `parseCycleErrorMessage`), `isDataLessTarget(targetHandle)` and `isValidFlowConnection(connection, nodes, edges)`.
- `FlowNodeDetail` type: `{ statusCode?; durationMs?; error?; value?; skipReason?; branch? }`.

**Frontend (plan 05)**
- `src/components/flow/nodes/nodeStatus.ts` exports `nodeStatusClassName(status, skipReason?)` and `nodeStatusCaption(status, detail?)`.

## Per-plan conventions

- Every task that touches Rust crates or flow `.yml` files starts with:
  📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- Always pass `-j4` to cargo.
- Commit steps use the `dev-workflow-skills:1-git-commit` skill, with conventional-commit subjects.
- Each plan ends with a **Next Plan** link and a **Post-Implementation Review** section. Before the next plan starts, an Opus-model subagent reviews everything the plan changed and is allowed to fix what it finds.
