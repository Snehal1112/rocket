# Flow Phase 3 — Transform Node — Plan Index

**Spec:** [../../specs/2026-09-30-flow-phase3-transform-node-design.md](../../specs/2026-09-30-flow-phase3-transform-node-design.md)
**Issue:** #32

**Context:** Adds a first-class Transform node to Flow. It has one `input` and one `result` handle, runs a script body over its upstream value through the existing sandbox, and exposes the returned text to any number of downstream nodes. Multiple inputs or outputs, the `rok` API and extra sandbox limits are out of scope.

## How to run these plans (read this first)

- **Execute exactly one plan at a time, in numeric order.** Never start plan N+1 in the same run as plan N. When a plan finishes and its Post-Implementation Review is done, stop and report to the user. The user starts the next plan.
- Every plan ends with a **Next Plan** section that names the one file to run next. The last plan says there is none.
- Each plan has at most three tasks. Work the tasks in order, and commit at the end of each task.
- If a plan's first task finds that the previous plan is not merged or its tests fail, stop and report instead of continuing.

## Plan breakdown

| # | Plan | Area | Depends on |
|---|---|---|---|
| 01 | [Domain model and validation](2026-09-30-flow-phase3-transform-plan-01-domain-model-and-validation.md) | `rocket-flow` (+ compile-keeping edits in `rocket-app`, `src-tauri`) | — |
| 02 | [Executor and step reporting](2026-09-30-flow-phase3-transform-plan-02-executor-and-step-reporting.md) | `rocket-app` | 01 |
| 03 | [Frontend types and wiring rules](2026-09-30-flow-phase3-transform-plan-03-frontend-types-and-wiring.md) | frontend `src/lib` | 01 |
| 04 | [Canvas node and palette](2026-09-30-flow-phase3-transform-plan-04-canvas-node-and-palette.md) | frontend `src/components/flow` | 03 |
| 05 | [Properties panel, Last run and final verification](2026-09-30-flow-phase3-transform-plan-05-panel-last-run-and-verification.md) | frontend, whole feature | 02, 04 |

Plans 02 and 03 do not depend on each other, but they still run one after the other, in the order above.

## Decisions made during planning (2026-09-30)

- **No `trigger` input on Transform.** A Transform node has exactly one incoming edge and it targets `input`, the same as If and Switch. This settles the open point in spec §6 item 4. Plan 01 Task 2 updates the spec text.
- **"Script returned no value" is enforced inside the generated JS**, not in Rust. The script engine reports `undefined` and `null` the same way (`unwrap_or(Null)` in `evaluate_expression_with_logs`), so a new `FlowCoercion::Required` wraps the call in a guard that throws on `undefined`. Plan 02 owns this.
- **The reported value is masked.** A Transform's step value goes through `redact_secrets`, like an Input node's. Downstream wires still get the raw text. Plan 02 owns this.
- **Failed upstream Requests are not observed.** Only If/Switch observe a failed Request's response (Phase 2 §6.3.1). A Transform after a failed Request is skipped as "upstream failed", so `decide_fate` is unchanged.

## Cross-plan interface contract

Every plan uses exactly these names.

**`rocket-flow` (plan 01)**
- `FlowNodeKind::Transform { label: String, script: String }`, serialized with `kind: Transform` and snake_case fields.
- `pub const TRANSFORM_DEFAULT_SCRIPT: &str = "return response.body;"`, re-exported from the crate root.

**`src-tauri` (plan 01)**
- `FlowNodeKindDto::Transform { label: String, script: String }` (camelCase fields, no rename needed for these names).

**`rocket-app` (plan 02)**
- `FlowCoercion::Required`, which throws `script returned no value` on `undefined`.
- `RequestExecutionService::evaluate_flow_transform_script(&self, collection: &str, output: &CapturedOutput, source: &str, secret_values: &HashSet<String>) -> FlowScriptOutcome`.
- `fn single_input(node, data_edges, captured)`, the renamed `single_route_input`.

**Frontend (plans 03 to 05)**
- `FlowNodeKind` gains `{ kind: 'Transform'; label: string; script: string }` in `src/lib/tauri-api.ts`.
- `src/lib/flow-transform.ts` exports `DEFAULT_TRANSFORM_SCRIPT`.
- `src/lib/flow-handles.ts` exports `takesSingleInput(kind)`.
- `src/components/flow/nodes/TransformNode.tsx` exports `TransformNode` and `TransformNodeData`.
- `src/components/flow/properties/TransformNodeEditor.tsx` exports `TransformNodeEditor`.

## Per-plan conventions

- Every task that touches Rust crates or flow `.yml` files starts with:
  📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`. Use targeted crate tests plus `cargo check -j4 --workspace`.
- Commit steps use the `dev-workflow-skills:1-git-commit` skill, with conventional-commit subjects. Stage only the paths the task changed, because other sessions may share this repo.
- Frontend tasks follow `.claude/rules/frontend-component-guardrails.md`: shadcn/ui primitives, `lucide-react` icons, Monaco for multi-line editors, and no full destructuring of Zustand state.
- Code comments are short full sentences that end with a punctuation mark.
- Each plan ends with a **Next Plan** section and a **Post-Implementation Review** section. Before the next plan starts, an Opus-model subagent reviews everything the plan changed and may fix what it finds.
