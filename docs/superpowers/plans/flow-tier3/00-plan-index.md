# Flow Tier 3 Plan Index

Source: `.claude/flow-roadmap.md` Tier 3 (F-31 to F-47) plus F-57. Design facts come from five read-only investigations on 2026-10-08.
Each plan holds at most 3 tasks. Plan files are written one at a time just before they run. Status here is the source of truth for plan progress.

Rules for every plan:
- shadcn primitives and lucide icons only, narrow Zustand selectors, `SingleLineEditor` for single-line fields (see `CLAUDE.md`).
- Cargo commands use `-j4` and `-p <crate>`. Never `--workspace`.
- Plans touching `rocket-infra`, auth, variable resolution or collection and environment models start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."
- One implementer per worktree. Commits go through `dev-workflow-skills:1-git-commit`. Add explicit paths, never `git add -A`.
- Gates before commit: `yarn tsc --noEmit`, `yarn check`, targeted `yarn test`, and `cargo check -j4` when Rust changes.

## Plans

| ID | Plan | Items | Tasks | Needs | Models | Status |
|---|---|---|---|---|---|---|
| P1 | Flow save and shortcuts | F-07, F-33 | 1 save listener and auto-save toast; 2 close guard (Ctrl+W, dialog wording, optional Save and close); 3 Ctrl+Enter and Stop disabled when idle | none | sonnet | done: merged to local main 2026-10-08 (ab2ca7ef..cfc0de1a), not pushed |
| P2 | Run-result strip | F-31 | 1 `FlowTab.lastRun` and store action; 2 toolbar result, `onFlowRunFinished` recovery; 3 strip UI with click-to-select | P1 merged (same files) | sonnet | done: merged to local main 2026-10-08 (1bb17677..f69bac20), not pushed |
| P3 | Secret foundation | F-57, F-47 warning, F-43 prerequisite | 1 `flow-secrets.ts`; 2 carry secret flag through `buildScopedContext` and its callers; 3 `useCollectionVariableContext` hook and plaintext warning | none | sonnet, review step 2 in main loop | done on branch worktree-flow-p3 (e8711e3f, b6c878c0, ae5823c3), final review clean, merged to local main 2026-10-08 (e8711e3f..ae5823c3), not pushed |
| P4 | Undo and redo | F-34 | 1 pure history helpers; 2 store history, coalescing, drag gesture; 3 keys, toolbar buttons, selection pruning, issue #45 test | none | sonnet, review step 2 in main loop | done on branch worktree-flow-p4 (93b633a3, 8458eb5f, 5c103191, 42e5d084), final review fixed and re-reviewed, merged to local main 2026-10-08, not pushed |
| P5 | Copy, paste, duplicate | F-35 | 1 clipboard and id helpers with per-kind rules; 2 key handlers and wiring; 3 component tests and node menu entry | P4 | sonnet | done on branch worktree-flow-p5 (5f4b6272, 06dac16d, 0f282c41), final review clean, merged to local main 2026-10-08 (5f4b6272..0f282c41), not pushed |
| P6 | Layout, search, minimap | F-42 | 1 minimap; 2 search bar and `flow-search.ts`; 3 Tidy with undo step | P4; decision D2 | sonnet | done on branch worktree-flow-p6 (08d164cb, 5aa18bfd, 356f64d8, 4c423207), final review fixed and re-reviewed, merged to local main 2026-10-08 (08d164cb..4c423207), not pushed |
| P7 | Step trace backend | F-32, F-38, F-39 (backend) | 1 shared types, `NodeTrace`, durations, cap `value`; 2 wire capture with masking, route eval; 3 edge-named errors and `failed_edge_id` | none | sonnet, review masking in main loop | done on branch worktree-flow-p7 (88171043, 8061ca6c, 2a45e9ac, 97164b09), final review fixed and re-reviewed, merged to local main 2026-10-08 (88171043..97164b09), not pushed |
| P8 | Step trace frontend | F-32, F-38, F-39 (frontend) | 1 TS types and detail mapping; 2 Last run tab and duration chips; 3 Wires tab resolved values | P7 | sonnet | done: merged to local main 2026-10-08 (29e08ced..ef38ee00), not pushed |
| P9 | Live progress backend | F-40 (backend) | 1 poll live progress; 2 Wait detail and rejected call; 3 callback URLs on run-started | P7 | sonnet | done: merged to local main 2026-10-08 (dc43827a..c067eb87), not pushed |
| P10 | Live progress frontend | F-40 (frontend) | 1 types, toolbar and store; 2 Last run panels; 3 callback URL display and copy | P8, P9 | sonnet | done: merged to local main 2026-10-08 (9a728a20..699a2fb9), not pushed |
| P11 | Run history | F-36 | 1 store types and `recordFlowRun`; 2 selector and viewed-run banner; 3 edge cases | P2; decision D3 | sonnet | done: merged to local main 2026-10-08 (164d5a2e..41306861), not pushed |
| P12 | Issue badges (client) | F-37a | 1 `flow-issues.ts` rules; 2 `NodeIssueBadge` replacing `hasCycleError`; 3 issue count and popover next to Run | none | sonnet | done: merged to local main 2026-10-08 (e028bc8f..4bfffb7e), not pushed |
| P13 | Variable preview | F-43 | 1 pass context into flow editors and make click popover read-only there; 2 `variable-hover.ts`; 3 scope docs and checks | P3 | sonnet | done: merged to local main 2026-10-08 (4b519ad4..694ce3e3), not pushed |
| P14 | Authenticate button | F-47 | 1 extract `authenticateAuthNode`; 2 button and token status in `AuthNodeEditor`; 3 tests | P3; decision D4 | sonnet | done: merged to local main 2026-10-08 (2af1b436..c138aba2), not pushed |
| P15 | Export | F-44 | 1 `flow-export.ts`; 2 save helper and `FlowExportMenu`; 3 defensive redaction and bodies toggle | P3, ideally F-02 | sonnet | done: merged to local main 2026-10-08 (287ecdc9..1f6331ba), not pushed |
| P16 | Accessibility | F-45 | 1 status labels and aria labels; 2 run announcer live regions; 3 canvas role and tests | none | sonnet | done: merged to local main 2026-10-09 (1efea462..b3487374), not pushed |
| P17 | Flow rename and delete, backend | F-46 | 1 repo `rename` with rollback; 2 service and `rename_flow` command; 3 checks | none | opus for step 1 | done: merged to local main 2026-10-09 (37189100..3b995136), not pushed |
| P18 | Flow rename and delete, frontend | F-46 | 1 `useFlows` and Flows sidebar group; 2 delete target and tab handling; 3 `renameFlowTabs` and auth clearing | P17 | sonnet, review step 3 | done: merged to local main 2026-10-09 (44f7f603..0c0514cb), not pushed |
| P19 | Run from node, backend | F-41 | 1 pure `flow_partial.rs`; 2 `flow_run_cache.rs`; 3 wire into `run_with_auth` | decisions D1 and D5 | opus | done: merged to local main 2026-10-09 (d8a3570e..e2bd8487), not pushed. D1 and D5 taken at recommended defaults. |
| P20 | Run from node, IPC and UI | F-41 | 1 DTO and TS types; 2 `useFlowRun` hook and node menu; 3 canvas states and refusal highlighting | P19, F-03 | sonnet | done: merged to local main 2026-10-09 (69b57636..18b22468), not pushed |
| P21 | Backend lint feed | F-37b | 1 `lint_flow` command; 2 debounced hook and merge; 3 remove duplicate client rules | P23 (F-20), P12 | sonnet | done: merged to local main 2026-10-09 (420fd3a3..6873e0d9), not pushed |
| P22 | Client-chosen run id | F-03 | 1 backend id rule, reserve and `run_with_options`; 2 toolbar mints, stores and matches the id; 3 two-tab test and docs | none | sonnet, review task 1 in main loop | done: see `2026-10-09-p22-client-run-id.md` |

## Suggested order

1. P1, P3, P12 (small, independent, safety first).
2. P2, P4, P16.
3. P5, P6.
4. P7, P8, P9, P10.
5. P11, P13, P14, P15.
6. P17, P18.
7. P19, P20 once F-03 and the decisions below are settled.
8. P21 after the backend lint tier (F-20).

Plans on separate files can proceed in separate worktrees only when they touch different files. P1, P2, P4, P5 and P6 all edit `FlowCanvas.tsx`, `FlowPane.tsx` or `FlowToolbar.tsx`, so run those one at a time. P7, P9 and P19 all edit `execute_node` in `flow_execution_service.rs`, so run those one at a time too.

## Corrections to the roadmap found during design

- F-07 is mostly built. The dirty dot and the X-button close confirm exist. Missing: a flow save listener, a Ctrl+W branch for dirty flow tabs, flow wording in the dialog, and the auto-save toast.
- F-43: the "null for folder and process" at `VariablePopover.tsx:42` only means no "Navigate to source" link. The popover works for those sources. Flow editors have no variable context at all today.
- F-46: flows are not shown in the sidebar. They are listed only in the Flow picker tab. Delete and rename need a new Flows group.
- F-37 is split into F-37a (client rules, no dependency) and F-37b (backend lint feed, needs F-20).
- F-32: wire value already exists for Input, Output and Transform. The real gap is duration on non-HTTP nodes.
- F-38: the If node can only show true or false, because the condition is coerced with `!!(` and tests match on it.
- New item F-57: secret environment values reach the variable popover unmasked (see the roadmap).

## Open decisions

| ID | Question | Recommended | Affects |
|---|---|---|---|
| D1 | "Run this node": ignore trigger wires and refuse when a data input's cached source was skipped or failed, or fall back to re-running ancestors? | Refuse with a clear message. Add "run to here" later on the same machinery. | P19 |
| D2 | Tidy layout: add `@dagrejs/dagre` (about 30 to 40 KB) or hand-roll about 80 lines? | dagre (confirmed by user 2026-10-08) | P6 |
| D3 | Run history: in-memory, last 5, full detail, or persisted metadata only? | In-memory now. | P11 |
| D4 | Authenticate button: interactive OAuth2 grants only, or all OAuth2 grants ("Test sign-in")? | Interactive grants only. | P14 |
| D5 | Run-from-node cache freshness: refuse when any upstream node changed, and accept that environment value changes are not detected? | Refuse on upstream change. | P19 |
| D6 | Re-run: relabel Run as "Re-run" after a finished run, or add a separate button? | Relabel. | P1 |

## Decisions log

- Ruling (2026-10-08): Tier 3 comes first. Plans are written one at a time.

## Written 2026-10-08: notes from the plan writers

All 21 plans are written, 3 tasks each. Where the plan differs from this index, the plan wins:
- Task order and content differ in P16 (labels, then announcer, then status icons), P17 and P18 (P18 task 1 is the picker, query and name validation; task 2 the sidebar group and delete; task 3 rename and token clearing).
- P20 does not extract a `useFlowRun` hook (P1's `rocket:flow-run` event already gives one run lifecycle) and the node button becomes a menu only once the tab has a base run.
- P2 uses `role="group"` for the strip, not `role="status"`, so P16's announcer does not duplicate it.
- P11 puts the history selector and P12 the issue button in `FlowPane`'s top-right container, not in `FlowToolbar.tsx`.
- Blocked plans: P20 needs P19 (done) and F-03 (done in P22); P21 needs roadmap F-20 and P12. F-03 and F-20 have no plan yet.
- Roadmap F-58 (callback token already visible in exchange URLs) is fixed inside P9 task 2.
- The corrections the writers found to `01-design-notes.md` are listed in their reports and are reflected in the plans; the plans, not the notes, are now the source of truth.
