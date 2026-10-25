# Plan audit, 2026-10-02

Scope: every plan under `docs/superpowers/plans/` except `environment-client-certificates` (current work).
Method: seven read-only agents checked each plan's named files and symbols against the code and `git log`.
Plan checkboxes were not used as evidence, because most are unticked even for shipped work.
Caveat: many SHIPPED rows rest on file or symbol presence, not on a behavior check.

## Result

About 240 plans and task files audited. The large majority are SHIPPED. The items below are the ones that need a decision or a fix.

## Needs a fix or a decision

| Item | Finding | Evidence |
|---|---|---|
| Executor client cache (FIXED in `fde5d074`) | `get_or_build_client` panicked on a poisoned mutex (a bare unwrap on the lock), which broke the no-unwrap rule for production paths. The guard is now recovered, with a poison test. | `crates/rocket-infra/src/reqwest_executor.rs:99` |
| Backend logs not shown | The `backend-log` event is emitted but nothing in `src/` listens, so backend tracing never reaches the Console panel. Verified. | `src-tauri/src/tauri_tracing_layer.rs:118`, plan `logging/structured-logging-plan` tasks 10 and 11 |
| ACP MCP tool server not merged | Plans 01 to 05 are complete only on branch `worktree-acp-mcp-tool-server` (38 commits ahead of main). Plan 06 frontend autonomy checkbox is not done on any branch. The worktree has uncommitted edits in 13 files. | `acp-mcp-tool-server/` |
| OpenCollection fixtures orphaned | The three fixtures exist but no contract test, `validate_oc` binary or CI step uses them. | `2026-04-04-opencollection-regression-prevention.md` |
| Collection runner entry points | No "Run folder" or "Run collection" entry in the sidebar. The Runner tab opens only from the tab bar menu. | `collection-runner` plan E task 3 |
| Raw button in git UI | A raw `<button>` remains, which breaks the shadcn-only rule. | `src/components/git/GitStashSection.tsx:244` |
| Theme playground never built | `vsocde-2026-theme` plan-02 and plan-03 have no code. | no `apps/` directory |
| Shipsmart | Only docs were committed. | `2026-04-26-shipsmart-saas.md` |
| Git bugs plan | B2, B4 and B8 were not confirmed individually. | `2026-04-25-git-bugs-plan.md` |
| Variable navigation | `VariablePopover` returns null for folder and process sources, so navigation there is incomplete. | `VariablePopover.tsx:42` |

## Partial or unclear, lower priority

- `oc-p14`, `oc-p15-known-gaps`, `oc-p16-minor-fixes`, `oc-p17-tracking-cleanup`: not verified item by item.
- `2026-04-30-workspace-environments-tab-visual-polish`: no Checkbox or grid in `WorkspaceEnvironmentsTab.tsx`.
- `2026-05-01-arch-plan-2-slim-stores`: consumer migration not confirmed.
- `2026-05-05-infra-phase1-remaining`: the poison-panic fix is incomplete.
- `sidebar-design-fixes`, `design-system-cleanup`, `theme-palette-update`, `collection-tree-spacing*`: later rework makes them hard to verify, treat as obsolete unless a manual UI check says otherwise.
- `git/plan-09` and `workspace/plan-10`: verification-only plans with no code artifact.
- `workspace-toolbar` plan-01: no sandbox store in `src/stores`.
- `src/lib/text-variables.ts` still exists though `cm6-sle-08` said to delete it.
- `imports/bruno/` is a stale partial copy.

## Superseded (no action)

Old tabs plan, the VariableAware input family (replaced by `SingleLineEditor`), Lora and Jakarta font plans, Linux window shadow plans (a 1px border replaced them), the Intel macOS and cross-compile CI plans (Intel Mac was dropped), `contract-tab-ui` 01 to 03, and the legacy variables plan.

## Corrections to older project notes

- Flow async, Flow Phase 2 branching and the node properties panel are all on `main`. No flow branch remains, and the async work is implemented, not just a spec.
- ACP A, B and C are on `main`. The MCP tool server (the D and E work) exists only on the branch above.
