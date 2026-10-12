# Flow Roadmap

Living roadmap for the Flow feature (visual workflow builder). Source: the multi-agent review of 2026-10-08.
Update this file whenever an item changes state. Do not let it drift from the code.
Statuses re-verified against `main` (commit `9666d1a1`) on 2026-10-11 by reading the code. Items marked "Partly done" in Notes are still `open`. Real-app checks (F-80, F-84, F-89, F-94, F-101, F-112, F-116, F-121) were not run and stay pending.

## How to maintain this file

- Every item has a stable ID (`F-xx`). Never renumber. Add new items at the end of their tier.
- Status values: `open`, `planned` (in a plan file), `in-progress`, `done`, `dropped`.
- When an item ships, set status to `done` and add the commit or PR in the Notes column.
- When an item is dropped, keep the row and give the reason.
- Evidence lines were read from the code on 2026-10-08 and were not all run. Re-verify before you rely on a line number.
- Plans derived from this file must hold at most 3 tasks each, with an index file. Link each plan in the Batches section.
- Any task that touches `.yml`, collection or environment models, auth, or variable resolution must start with:
  "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."
- Related notes: `.claude/plan-audit-2026-10-02.md`, `docs/superpowers/specs/*flow*`, `crates/rocket-flow/CLAUDE.md`.

Sizes: S under half a day, M one to two days, L more than two days or needs a backend and frontend change.

---

## Tier 1: Bugs and risks

| ID | Item | Size | Status | Evidence | Notes |
|---|---|---|---|---|---|
| F-01 | Renaming, moving or deleting a collection request silently breaks flows that use it. Add a rewrite hook in the collection service, plus a warning (see F-20). | L | open | `collection_service.rs:179,281,344`; `node.rs:152-155` | Needs the OpenCollection read. |
| F-02 | Redact step error text. A failed send can quote a query-string API key. Auth follow-up F2. | S | open | `flow_execution_service.rs:1642,1663`; contrast `flow_debug.rs:36` | Reuse `redact_url_secrets`. |
| F-03 | Let the client choose the run ID. Runs are matched to tabs by collection and flow name, so two tabs can cross-attach and Stop can cancel the wrong run. | M | done | `FlowToolbar.tsx` `handleRun`; `flow_execution_service.rs` `run_with_options` | Done in P22: client UUID in `RunFlowOptions.runId`, backend checks and reserves it first, tab keeps `pendingRunId`. F-05 can build on the early reservation. |
| F-04 | Stop must interrupt an in-flight HTTP send. Race `execute_capturing` against the cancel token. Also cap each poll attempt at the deadline. | M | open | `flow_execution_service.rs:1361-1363`; `flow_poll.rs:85-87` | A collection timeout of 0 means no timeout. |
| F-05 | Make the pre-run phase cancellable and visible. Covers vault fetch, Auth token fetches (30 s each, sequential) and callback listener setup. Emit a "preparing" event. | M | open | `flow_execution_service.rs:930-977`; `FlowToolbar.tsx:298` | Needs F-03 first. |
| F-06 | Surface Request script errors and test results in `FlowStepResult` and the step event. Optional per-node "fail on test failure". | M | open | `result_to_step` `flow_execution_service.rs:1605-1625`; test near line 7052 | |
| F-07 | Ctrl+S saves a flow. Warn when closing a dirty flow tab and show a dirty marker. Toast when Run auto-saves. | S | done | Only ScriptFilePane, SaveRequestButton and FolderSettingsTab listen for `rocket:save-draft`. | Verified 2026-10-08. |
| F-08 | The Auth node's token overrides a request's explicit "No auth", for any host. Add an opt-in per request or a host scope, and show a warning in the editor. | M | open | `flow_auth.rs:107-115`; `flow_execution_service.rs:1271` | Needs a design decision first. |
| F-09 | The callback server binds `0.0.0.0` regardless of `callback_host`. Bind loopback when the host is loopback, add body and idle timeouts, report the resolved URL, and warn when LAN detection falls back to `127.0.0.1`. | M | open | `callback_server.rs:33,66,133,159-161` | Verified the bind on 2026-10-08. Partly done (2026-10-11): a header-read timeout exists (`callback_server.rs:33,186`). Still binds `0.0.0.0` (`:66`), no body or idle timeout, no LAN-fallback warning. |
| F-10 | OAuth2 sign-in race. A token from an in-flight sign-in can be filed under a different environment. Capture the store key and resolver at sign-in start. | M | open | `OAuth2AuthEditor.tsx:110-148`; `AuthNodeEditor.tsx:142-150`; `flow-auth.ts:241` | Same class as the earlier ACP credential leak. |
| F-11 | Report Cancelled as its own status. Send Skipped(Cancelled) for nodes not reached, and keep the real error in its own field. | M | open | `flow_execution_service.rs:1058-1059`; `flow_wait.rs:93`; `flow_poll.rs:72,181` | Needs a UI state in `nodeStatus.ts`. |
| F-12 | Drop a callback endpoint when its Wait node finishes or is skipped. Count rejected calls (404, 413, 503) and show them in progress. | M | open | `flow_callbacks.rs`; `callback_server.rs:202-249` | |
| F-13 | A Wait should drain queued calls before it reports a timeout. Race `accept_when` against cancel. | S | done | `flow_wait.rs:90-118` | Partly done (2026-10-11): the wait loop races cancel (`flow_wait.rs:110`), but the biased select checks the deadline before `calls.recv()` (`:114`), so queued calls are not drained, and `accept_when` is not raced against cancel. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-14 | Save the pending history entry when a poll ends with a send error. | S | done | `flow_poll.rs:105-113` | Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-15 | Cap `debug_request` size, or leave it out of the step event. Today the body goes over IPC twice. | S | done | `flow_execution_service.rs:1372-1374`; `flow_poll.rs:96-98` | Partly done (2026-10-11): `exchange` is capped (`cap_exchange`, `flow_debug.rs:148`). `debug_request` is still an uncapped second clone (`flow_execution_service.rs:~1763`, `flow_poll.rs:~109`). Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-16 | Mask `RequestExecuted` event URLs the way history does. | S | done | `execution_service.rs:1970-1975` | Predates Flow. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-17 | Make the token key separator unambiguous. Clear tokens on rename and after late sign-in writes. Auth follow-up F7. | S | done | `flow-auth.ts:42,56`; `flow-auth-preflight.ts:102,117` | Partly done (2026-10-11): tokens are cleared on rename (`FlowListItem.tsx:108-116`). The key separator is still `::` and late sign-in writes are not cleared. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-18 | Connection-slot exhaustion and memory amplification in the callback server (64 slots, about 100 MiB per Wait node). Lower the channel capacity or body limit. | S | done | `callback_server.rs`; `flow_wait.rs:116-118` | Fold into F-09 if convenient. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-57 | Secret environment values reach the variable popover unmasked. `buildScopedContext` adds environment and global entries with `secret=false`, and callers build `envVars` from `v.value` without the secret flag. Fix before the F-43 hover ships. | M | done | `url-variables.ts:46-87`; `RequestPanel.tsx:538-541`; `AuthNodeEditor.tsx:93-97`; `VariablePopover.tsx:129`; `fs_environment_repo.rs:83-126` | Found 2026-10-08. Partly verified: the flag is dropped in the callers. Whether secrets are hydrated into `Variable.value` on load was not re-checked. Plan P3, fixed on worktree-flow-p3 (b6c878c0), merged to local main 2026-10-08, not pushed. |
| F-58 | The callback token (`/cb/<token>`, a bearer capability) already appears in step data: `flow_debug.rs:155-160` puts the call path in the Wait exchange URL, and any Request that sends `{{callback.x}}` records the URL in its exchange and in history. Show the path as `/cb/…` there. | S | done | `flow_debug.rs:181-184`; tests `flow_debug.rs:669-727` | Found 2026-10-08 by the P9 plan writer. Done in P9: the Wait exchange URL shows `/cb/…`. Verified 2026-10-11. The Request exchange and history path (`flow_execution_service.rs`) was not re-checked. |
| F-59 | Script tab "Save and close" closes the dialog after a failed save (`EditorGroup.tsx` `.then(() => setPendingCloseTabId(null))`). The flow version is correct. | S | done | `EditorGroup.tsx` script branch | Found in P1 review. Not Flow-specific. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-60 | Tab context menu on a flow tab says "Save to collection..." and stays enabled when clean; on a non-active flow tab it does nothing (only the active tab's pane is mounted). | S | done | `TabBar.tsx:135-152` | Found in P1 final review. Script tabs have the same gap. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-61 | `handleSave` marks the tab clean after the await even if the user edited while the save was in flight, so that edit is neither saved nor guarded on close. Compare the saved snapshot before `markClean`. | S | done | `FlowPane.tsx` `handleSave` (`markClean` after the await) | Pre-existing, easier to hit now that Ctrl+S exists. Plan P4 records a saved snapshot in `markClean`, so fix them together. |
| F-62 | `process.env.*` entries are never masked in the popover and hover, so values such as `AWS_SECRET_ACCESS_KEY` show in clear text. | S | done | `url-variables.ts` process loop | Found in the P3 final review. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-63 | A secret variable with an empty value shows a read-only mask in the popover and cannot be set there. Add a hint to open the environment editor. | S | done | `VariablePopover.tsx:96` | Found in the P3 final review. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-64 | A second wire connected while an uncommitted `headers` popover is open is keyed `wire:B`, so one Ctrl+Z restores the bare `headers` edge A that the backend rejects. Key the preempt write with `wire:<pendingEdge.id>`. | S | done | `FlowPane.tsx` ~419 | Found in the P4 re-review. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-65 | Duplicating an inline Request that sends `{{callback.<name>}}` together with its Wait renames the Wait copy (`pay_2`) but the Request copy still sends the old name, so save fails V15. Rewrite the variable when both are in the same clip. | M | open | `flow-clipboard.ts:135-142` | Found in the P5 final review. Partly done (2026-10-11): a notice about the rename is now pushed (`flow-clipboard.ts:139`), but the Request copy's variable is still not rewritten. |
| F-66 | Pasting an If, Switch or Transform without its upstream node drops its input wire and the save fails V1 with no notice. Add a paste notice. | S | done | `flow-clipboard.ts:31-41` | P5 final review. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-67 | The clipboard cross-collection check compares names only, and the clip survives a workspace switch, so a Saved request path can paste into another workspace's same-named collection. Also check the workspace id or clear the clip on switch. Same weakness exists in drag-and-drop. | S | open | `flow-clipboard.ts:52-60` | P5 final review. Partly done 2026-10-11 on branch worktree-flow-tier1-small: the clip is stamped with the workspace and cleared on a switch; drag-and-drop still checks names only. |
| F-68 | Ctrl+C with a node selected always calls preventDefault, so text selected inside a non-editable node area cannot be copied. Skip when `window.getSelection()` is not empty. | S | done | `FlowCanvas.tsx:493-497` | P5 final review. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-69 | Trace wire recording masks the whole input before cutting to 16 KB, so every If, Switch and Transform copies a large captured body once per mask. Cut the raw text to `limit + longest_mask_len - 1` at a char boundary before masking, so a secret straddling the cut still falls in the window. | M | open | `flow_trace.rs` `record_wire`, `mask_then_cap`; `record_input_wire` | Found in the P7 final review. |
| F-70 | Step `error` text is masked less than the trace (`wire_err` wraps script and capture errors; masked only by what the scripting layer does with `secret_values`). Same family as F-02. | M | open | `flow_execution_service.rs` `wire_err` | P7 final review. |
| F-71 | A turned-down callback call has no cap on header count or header bytes, and every one publishes a live event. A token holder can flood the webview IPC. Cap headers in `rejected_call` or batch live turn-down events onto the 1 Hz tick. | S | done | `flow_wait.rs` ~152-172; `flow_debug.rs` `rejected_call` | Found in the P9 final review. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-72 | Step trace UI polish from the P8 final review: (1) "Cut at 16 KB." can be wrong when the 64 KB per-step budget cut a wire below 16 KB, say "Value was cut."; (2) a failed credential wire shows no error reason although the wire error text holds no value; (3) a wire's script edited after a run shows the old "Last value" next to the new script; (4) `DurationChip` has no screen reader context ("Last run took 12ms"); (5) every Wires tab trigger is named "Last value", include the field name. Items 4 and 5 fit P16. | S | done | `LastRunTab.tsx`, `WiresTab.tsx`, `DurationChip.tsx` | P8 final review. Checked per item 2026-10-11: all five still OPEN. (1) `WiresTab.tsx:87` and `LastRunTab.tsx:291` still say "Cut at 16 KB." unconditionally. (2) `WireValueText` and `ResolvedValue` return "Credential (hidden)" before the error branch. (3) `wireRows.ts:127` builds the value from the trace with no check against the current `edge.expression`. (4) `DurationChip.tsx:11-28` has no aria text. (5) `WiresTab.tsx:77` trigger is the fixed text "Last value". Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-73 | P10 follow-ups: the Wait node's own "Copy variable" button still uses `navigator.clipboard` (use `copyTextAsync`); the countdown interval keeps ticking after it reaches 0 until the next event; a Wait node that has not started shows its URL on the node but not in the Last run tab (idle early return); the countdown shows the old number for one render after a new event; the copy result is not announced to screen readers (P16). | S | open | `WaitForCallbackNode.tsx`, `LastRunTab.tsx` `useSecondsLeft`, `CallbackUrlField.tsx` | P10 reviews. Checked per item 2026-10-11: all five still OPEN. (1) `WaitForCallbackNode.tsx:51` uses `navigator.clipboard`. (2) `useSecondsLeft` interval depends only on `deadline` and keeps ticking at 0 (`LastRunTab.tsx:389-405`). (3) `LastRunTab.tsx:477` returns "Not run yet" for idle before the URL section. (4) `deadline` and `now` are set inside `useEffect([live])`, so the first render uses the old value. (5) `CallbackUrlField.tsx:36-40` only swaps icons, no live region. Partly done 2026-10-11 on branch worktree-flow-tier1-small: items 1-5 fixed; the Wait node Copy variable is now announced too. Open: none known, status kept open until the branch merges. |
| F-74 | A flow tab parked by `switchCollection` while a run is in progress still misses `setFlowRunResult`, `patchFlowNodeStatus` and other run-activity updates (they use `updateTabInTree`), so its final `lastRun` and run-history record are never written. P10 fixed `setFlowRunState` and `setFlowCallbackUrls` only. Move the run-activity actions to `updateTabEverywhere`. | S | done | `pane-store.ts` `setFlowRunResult`, `patchFlowNodeStatus`, `patchFlowNodeProgress` | Found in the P10 and P11 reviews. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |
| F-75 | P11 follow-ups: (1) memory: five full-detail runs can hold about 0.8 MB per node per record, so strip `exchange` and `logs` from records older than the newest two if it matters; (2) layout at the 40% minimum panel width: the fixed `w-48` run selector, the viewed-run banner (`top-14 left-3 max-w-[45%]`) and the RunResultStrip (`top-12 right-2 max-w-[60%]`) can collide; let the banner text wrap instead of truncate; (3) accessibility: the banner `role=note` is not announced, and focus is lost when "Back to latest" unmounts it, return focus to the selector; (4) `FlowToolbar` catch can send a second error result for a run that already reported (skip it once a result was sent); (5) re-recording the same run refreshes `finishedAt`. Item 2 and 3 fit P16. | S | open | `FlowPane.tsx`, `ViewedRunBanner.tsx`, `RunHistorySelect.tsx`, `FlowToolbar.tsx` | P11 reviews. Checked per item 2026-10-11: (1) OPEN, `flow-run-history.ts:15` only slices to 5, nothing strips older records. (2) real-app check pending, code unchanged (`ViewedRunBanner.tsx:~21` truncates, `RunHistorySelect.tsx:40` `w-48`). (3) OPEN, `role=note` only, no focus return. (4) OPEN, `FlowToolbar.tsx:430-438` has no "result already sent" guard. (5) OPEN, `flow-run-history.ts:12-13` replaces the record and `FlowPane.tsx:326` passes `Date.now()`. Partly done 2026-10-11 on branch worktree-flow-tier1-small: items 1, 3 (focus return; the banner may not be announced on mount), 4 and 5 fixed. Item 2 (visual layout) not done. |
| F-76 | P12 follow-ups: (1) the top-right row now wraps at the 40% minimum panel width, so the second line can sit under `RunResultStrip` (`top-12 right-2`), `ViewedRunBanner` (`top-14 left-3`) and the top-center Tidy/search Panel; give the overlays a shared layout or offset (needs a visual check in the app); (2) a stale "save" issue and ring stay after the user fixes a cycle by hand until the next save (clear on edit); (3) a rejected save can list the same problem twice, as the save issue and as the client issue; (4) `computeFlowIssues` runs every drag frame (under 1 ms at 100 nodes, memoize on a key without positions before P21 adds the backend lint feed); (5) switching tabs clears `saveErrorMessage` but not `cycleNodeIds`/`cycleEdgeIds`; (6) `cleanSaveMessage` passes backend reasons through (they can contain a Switch match value, same as the toast and panel today). | S | open | `FlowPane.tsx` ~557/651/660, `flow-issues.ts`, `FlowIssuesButton.tsx` | P12 reviews. Checked per item 2026-10-11: (1) real-app check pending (`FlowPane.tsx:476,594,603` still offset independently). (2) OPEN, cycle and save state are cleared only on save success, run start and the clear handlers (`FlowPane.tsx:390-392,524,240-248`). (3) OPEN, `flow-issues.ts:273-293` adds `saveIssues` with no dedupe. (4) OPEN, `clientIssues` memo depends on `tab.nodes` and `tab.edges` (`FlowPane.tsx:134-145`). (5) OPEN, the `tab.id` effect clears `saveErrorMessage` only (`:108-115`). (6) OPEN, `cleanSaveMessage` strips only the "Invalid input:" prefix and suffix (`flow-issues.ts:241-251`). Partly done 2026-10-11 on branch worktree-flow-tier1-small: items 2-6 fixed. Item 1 (visual layout) not done. |
| F-19 | Replace the 8 `.expect()` calls in `graph.rs` with non-panicking code, to follow the project rule. | S | done | `graph.rs:68-154` | Each is guarded by an invariant today. Done 2026-10-11 on branch worktree-flow-tier1-small (not yet merged). |

## Tier 2: Silent failures and detection gaps

| ID | Item | Size | Status | Evidence | Notes |
|---|---|---|---|---|---|
| F-20 | Add a non-blocking lint tier: `validate_with_warnings(flow, ctx)`. The app layer implements `ctx` (saved-request lookup, known variables), so `rocket-flow` stays free of I/O. Every lint carries node ID, label and a fix hint. | L | done | Review of `validate.rs` | Parent of F-21 to F-27. |
| F-21 | Lint: dangling saved-request path. Run at save and when a flow opens. | M | done | `validate.rs:391-392`; `flow_execution_service.rs:484` | Done 2026-10-11 on branch worktree-flow-tier2 (`lint.rs` `dangling_request_lint`, `flow_lint_context.rs`). Re-lints on open, edit, environment change and save. Follow-up: a variable edit alone does not re-lint. |
| F-22 | Lint: unknown `{{var}}` in inline fields. Today it is sent as literal text. | M | done | `flow_execution_service.rs:1204` | Done 2026-10-11 on branch worktree-flow-tier2 (`lint.rs` `unknown_variable_lint`). Silent when a Saved Request is in the flow, no environment is selected, or the global name is not loaded. |
| F-23 | Lint: Switch case or If exit with no outgoing edge, a Switch with no `default`, and nodes with no path to an Output. | M | done | `lint.rs:171-205` `exit_lints`; `lint.rs:207,238` `nodes_reaching_output`, `no_path_lint`; wired at `lint.rs:84-85` | Done in the lint tier (349ad959, 0ae789a1). Verified 2026-10-11. |
| F-24 | Lint: Auth node with no effect, or an `auth` wire into a Request that has explicit auth. | S | done | `flow_auth.rs:107-108` | Needs F-20. Done 2026-10-11 on branch worktree-flow-tier2: `auth_no_effect`, `auth_wire_overrides_auth` in `lint.rs`, facts in `flow_lint_facts.rs`. Unknown saved request never warns. |
| F-25 | Lint: callback ordering for saved requests (extend V15). The engine can also check it at run start. | M | done | `validate.rs:388-431` | Needs F-20. Done 2026-10-11 on branch worktree-flow-tier2: `callback_not_wired`, saved requests only (inline stays V15). Silent when a wire replaces url, headers or body. Run-start engine check not built. |
| F-26 | Hard error: unknown Request wire `target_field` (whitelist `url`, `body`, `auth`, `headers[...]`). Also reject a `body` wire into form or binary bodies. | S | done | `flow_execution_service.rs:564-606` Done 2026-10-11 on branch worktree-flow-tier2: V16 `is_request_wire_field`. Form/binary body of saved requests stays a run-time error (lint later). Flows with a bad Request wire no longer save or run; release note needed. |
| F-27 | Hard errors: duplicate edge IDs, and an Output node with other than one incoming wire (Phase 1 spec section 6). | S | done | `graph.rs:44-55`; `flow_execution_service.rs:1221-1228` Done 2026-10-11 on branch worktree-flow-tier2: `DuplicateEdge` and V17 (exactly one non-trigger wire on an Output; zero is an error). Release note needed. Partial runs validate the whole flow, so an unwired Output blocks Run-from-node. |
| F-28 | Return all validation errors at once, with node labels. Improve the weak messages (unknown exit should list valid exits, `auth` wire error should name both ends). | M | done | `graph.rs:6-25`; `validate.rs:54-59,74` | Changes the error type. Pairs with F-33. Partly addressed (2026-10-11): the lint layer reports per item with labels (`graph_error_lints`, `lint.rs:278`, 5b3059ec), but `validate()` still returns on the first error (`validate.rs:12`). Done 2026-10-11 on branch worktree-flow-tier2: `validate_all`, `describe.rs`, multi-error save text capped at 50, client maps each problem to its node or wire. String format kept until F-50. Known gap: wire problems can show twice (save plus lint). |
| F-29 | Unparseable flow files must show in the list as "corrupt" with the parse message, not vanish. | S | done | `fs_flow_repo.rs:86-96` Done 2026-10-11 on branch worktree-flow-tier2: `FlowListEntry`/`list_entries`, corrupt rows in sidebar and picker (rename disabled, delete kept); parse text redacts quoted and backtick values; `FsFlowRepo::save` refuses to overwrite an unparseable file. |
| F-30 | Add a `MAX_RUN_STEPS`-style guard (Phase 1 spec section 7), or record why it was dropped. | S | done | spec versus `flow_execution_service.rs` | Low risk, the graph is acyclic. Done 2026-10-11 on branch worktree-flow-tier2: `MAX_RUN_STEPS` 10,000 checked against node count in `load_ordered_nodes` before any event. No per-step counter (no loop node yet). |

## Tier 3: Transparency and productivity

Design is done for every item here. Plans P1 to P21 are listed in `docs/superpowers/plans/flow-tier3/00-plan-index.md`, with order, dependencies and open decisions D1 to D6. Plan files are not written yet, so item status stays `open` until a plan file exists. Notes from design: F-07 is mostly built already, F-37 splits into client rules now and a backend feed after F-20, F-46 needs a new sidebar Flows group.

| ID | Item | Size | Status | Evidence | Notes |
|---|---|---|---|---|---|
| F-31 | Run-result strip: total time, `stoppedReason`, "failed at X" with click-to-select. Subscribe to `onFlowRunFinished` as a recovery path. | S | done | `tauri-api.ts:2244,2357`; `FlowToolbar.tsx:280` | Depends on F-11 for a clean Cancelled label. |
| F-32 | Show a wire-value block and a duration chip on every node kind. | S | done | `LastRunTab.tsx:306-309`; `RequestNode.tsx:86` | Backend `FlowStepResult.value` exists. |
| F-33 | Ctrl+Enter to run, a Re-run button, and Stop disabled when idle. | S | done | `useKeyboardShortcuts.ts`; `FlowToolbar.tsx:308` | |
| F-34 | Undo and redo for graph edits (snapshot stack in the pane store). Addresses issue #45 (Ctrl+A then Backspace deletes the flow). | M | done | `FlowPane.tsx:156-170` | |
| F-35 | Copy, paste and duplicate nodes, with fresh IDs, offset positions and edges between copied nodes. | M | done | `FlowCanvas.tsx:400` | |
| F-36 | Run history: keep the last N run summaries per tab with a selector. Persisting needs a backend command (L). | M | done | `pane-store.ts:976` | |
| F-37 | Pre-run issue badges on nodes and an issue count next to Run. Client-side first, then fed by the lint tier. | M | done | `FlowPane.tsx:318-334`; `FlowCanvas.tsx:115` | Pairs with F-20. F-37a (client rules, badges, issue list) built in P12 and merged to local main 2026-10-08; F-37b (backend lint feed) still needs F-20 and P21. |
| F-38 | Show the evaluated If and Switch value and the condition result in the Last run tab. | M | done | `LastRunTab.tsx:297-301`; `IfNode.tsx:61-67` | Backend field needed. |
| F-39 | Show what each wire resolved to, in the Wires tab. Name the edge, source node and target field in wire errors. | L | done | `WiresTab.tsx`; `flow_execution_service.rs:258-262` | Backend support needed. |
| F-40 | Richer repeat-until progress: last status code, condition result, time left. Show callback URLs and Wait detail (last rejected call). | M | done | `flow_poll.rs`; `flow_callbacks.rs` | |
| F-41 | Run this node and Run from here, using cached upstream values. | L | done | `tauri-api.ts:2267`; `NodeMenuButton.tsx:58-70` | Backend change in `run_flow`. |
| F-42 | Auto-layout ("Tidy"), node search (Ctrl+F) and a minimap. | M | done | `FlowCanvas.tsx:455-461` | |
| F-43 | Resolved `{{var}}` hover preview in `SingleLineEditor`. | M | done | `FlowPane.tsx:60` | P13 merged to local main 2026-10-08 (4b519ad4..694ce3e3), not pushed. Real-app hover check still pending. |
| F-44 | Copy flow JSON and export a run report. | S | done | `tauri-api.ts:2170-2267` | |
| F-45 | Accessibility: node status labels, an `aria-live` region for run progress, a canvas Tab stop. | M | done | `FlowCanvas.tsx:411` | |
| F-46 | Delete and rename flows from the UI. Call `clearFlow` on both (Auth follow-up F7). | M | done | `lib.rs:913`; `tauri-api.ts:2284,2287` | Rename shipped in P17 (`rename_flow`, 37189100); delete is in the sidebar (7a939a28). |
| F-47 | Authenticate button and a plaintext-secret warning in the Auth node editor, as the Auth spec promised. | M | done | `AuthNodeEditor.tsx`; Auth spec "Components touched" | |

## Tier 4: Communication gaps and prevention

| ID | Item | Size | Status | Evidence | Notes |
|---|---|---|---|---|---|
| F-48 | Generate TypeScript types from Rust (`ts-rs` or `tauri-specta`) for flow DTOs, events and errors. Fail CI when the generated file is stale. | L | open | `tauri-api.ts:2200,2357` | Needs a tool decision. |
| F-49 | Shared golden-fixture contract test: Rust writes sample payloads, vitest checks them against the TS types. Include omitted optional fields. | M | open | `flow-api.test.ts` mocks by hand | Can land before F-48. |
| F-50 | Structured IPC error `{code, message, nodeIds, edgeIds}` replacing the regex over `node(s): ...; edge(s): ...`. Make `stopped_reason` a serde enum. | M | open | `flow-wiring.ts:151`; `flow_service.rs:34-51` | Pairs with F-28. Verified 2026-10-11: still open. The error is a string (`graph_error_message`, `flow_service.rs:68`), the UI parses it with a regex, and `stopped_reason` is a `String` (`rocket-shared` `events.rs:427,516`). |
| F-51 | Add a `version` field to `Flow`. Clean up stale `*.tmp` files. Detect conflicting saves with a content hash or mtime. | M | open | `flow.rs:61-69`; `fs_flow_repo.rs:120-132` | |
| F-52 | Document validation rules V9 to V15 and V-T in a spec. V15 has no design doc. Update the header comment in `validate.rs:2`. | S | open | `validate.rs:2,184,269,310,344,393` | Partly done (2026-10-11): V-T is documented in `2026-09-30-flow-phase3-transform-node-design.md:74`. V9 to V15 appear in no spec, and `validate.rs:2` still says "rules V1-V15". |
| F-53 | Fix stale spec headers (status, branch names), the V2 and V3 wording, the `list_flows` and `run_flow` IPC description, and the "out of scope" items later delivered. | S | open | Phase 1, 2, async, panel, tabs, debug and wire-script specs | |
| F-54 | Triage GitHub issues: close or update #32 and #40, verify and close #37, and keep #42 to #45 linked to items here. | S | open | `tree.rs:241` | Checked on GitHub 2026-10-11 (read only): #32, #37, #40, #42, #43, #44, #45 are all still open. Candidates to close: #45 (Ctrl+A then Backspace deletes the flow; F-34 undo and redo shipped) and #32 (Phase 3 transform node is on main). #37 needs verifying first. #42 to #45 still need a link to the roadmap items. No issue mentions an `F-` ID. |
| F-55 | Ticket the untracked Auth follow-ups F3, F4, F6, F8 and the documented limits (public callback tunnel, aborting in-flight requests, binary body viewer, debug limits). F6 is the backend check that a supplied token matches the node's current config. | S | open | Auth spec "Follow-ups"; async and debug specs | Checked on GitHub 2026-10-11 (read only): no issue exists for Auth follow-ups F3, F4, F6, F8, the public callback tunnel, aborting in-flight requests, the binary body viewer, or the debug limits. |
| F-56 | Add tests for `flow_poll.rs` and the untested frontend files listed below. | M | open | Name-match scan only, treat as candidates | `InputNodeEditor`, `LabelOnlyEditor`, `WaitForCallbackEditor`, `NodeDetails`, `useSavedRequestPreview`, `RequestPicker`, `LabelField`, `NodeMenuButton`. Partly done (2026-10-11): only `NodeMenuButton` has a test (`nodes/__tests__/NodeMenuButton.test.tsx`). The other seven files and `flow_poll.rs` (no test module) are still untested. |

---

## Batches (implementation order)

Each batch is split into plan files of at most 3 tasks. Link the plans here when they exist.

| Batch | Scope | Items | Plan |
|---|---|---|---|
| B1 Safety | Redaction, auth scope, callback exposure, sign-in race | F-02, F-08, F-09, F-10, F-16, F-17, F-18 | not written |
| B2 Run lifecycle | Run ID, cancel, Cancelled status, script errors, result strip, small run fixes | F-03, F-04, F-05, F-06, F-11, F-12, F-13, F-14, F-15, F-31, F-33 | not written |
| B3 Pre-run checks | Lint tier, rename hook, validation hardening | F-20 to F-30, F-01, F-19, F-37 | not written |
| B4 Contract | Fixtures, generated types, structured errors, schema version | F-49, F-48, F-50, F-51, F-28 | not written |
| B5 Editing quality | Save and dirty, undo, copy and paste, history, display of values | F-07, F-34, F-35, F-36, F-32, F-38, F-39, F-40 | not written |
| B6 Longer term | Run from node, layout, search, preview, export, a11y, flow delete and rename, Auth UX | F-41 to F-47 | not written |
| B7 Docs and hygiene | Spec fixes, issue triage, tickets, tests | F-52 to F-56 | not written |

Notes on order:
- F-07 is a quick win and may ship ahead of B5.
- B7 is cheap and can run in parallel with any batch, since it only touches docs and tests.
- F-03 must land before F-05.
- F-20 must land before F-21 to F-25 and F-37.
- Do not run two batches at once in one worktree. Follow `.claude/rules/harness.md` for orchestration and ledgers.

## Decisions log

Add `Ruling:` lines here as choices are made.

- Ruling (2026-10-08): this roadmap covers all four tiers from the review, including contract hardening and doc fixes.
- Open: F-08 needs a design choice (per-request opt-in, host scope, or a warning only).
- Open: F-48 needs a tool choice (`ts-rs` or `tauri-specta`).

## Follow-ups from P13 (2026-10-08)

| ID | Item | Notes |
|---|---|---|
| F-77 | `FlowVariableScope` remounts per node selection, so collection-scope tokens briefly look unresolved; hoist it above the panel. | `NodePropertiesPanel.tsx:37,51,234`. |
| F-78 | Monaco body editor keeps stale highlighting when the flow context arrives late. | `MonacoWrapper.tsx:82-85,200-204`. |
| F-79 | `AuthEditor` fields in `AuthNodeEditor` still have an editable popover that can save to the wrong collection; pass `readOnlyVariables`. | Excluded from P13. |
| F-80 | Hover card closing on popover open is only unit-tested via the `hideOnPopoverOpen` predicate. Check in the real app. | |

## Follow-ups from P14 (2026-10-08)

| ID | Item | Notes |
|---|---|---|
| F-81 | `shouldWrite` compares raw `JSON.stringify` of auth, not `canonicalAuth`; a key-order round trip can discard a finished sign-in (fails safe). | `AuthenticateButton.tsx`. |
| F-82 | `AuthEditor`'s own "Get New Access Token" is not blocked while Authenticate is pending. | |
| F-83 | Button shows for the implicit grant but the backend says "not yet implemented". Gate it or implement the grant. | `oauth2.rs:420`. |
| F-84 | Real-app OAuth check of the Authenticate button is pending. | |

## Follow-ups from P15 (2026-10-08)

| ID | Item | Notes |
|---|---|---|
| F-85 | Export redaction misses: escaped JSON (`{\"password\":...}`), single-quoted JSON, percent-encoded names, `name: value` colon form outside six header names. | `flow-export.ts` `KEY_NAME`, `JSON_PAIR`, `PARAM`. |
| F-86 | `HEADER_LINE` redacts the rest of the line, so prose or minified JSON after `cookie:` is lost. | |
| F-87 | Not masked in a copied flow: Transform `script`, edge `expression`, Switch/If values, OAuth2 `additionalParameters` values. | `maskNode` default branch. |
| F-88 | Input node with a credential under a non-credential label (for example `User`) is not masked. Known limit of the label heuristic. | |
| F-89 | Real-app check of the Export menu (native save dialog, WebKit clipboard) pending. | |

## Follow-ups from P16 (2026-10-09)

| ID | Item | Notes |
|---|---|---|
| F-90 | Identical consecutive live-region text is dropped by screen readers (loop node succeeding twice, repeated identical failure). | `useFlowRunAnnouncer.ts`. |
| F-91 | A run stopped by an error is announced as "Run finished"; say "Run stopped". | `flowAnnounce.ts`. |
| F-92 | Wire names read raw field paths (`headers[Authorization].value`) and do not mention cycles. | `flowA11y.ts`. |
| F-93 | Step error text reaches the aria-label and alert region unmasked until F-70 is fixed in the backend. | Depends on F-70. |
| F-94 | Manual screen-reader and visual check of P16 (labels, announcements, icons, dark mode contrast) pending. | |

## Follow-ups from P17 (2026-10-09)

| ID | Item | Notes |
|---|---|---|
| F-95 | When both the old-file removal and the rollback fail, the error does not say a copy remains under the new name. Add that to the message. | `fs_flow_repo.rs:85-91`. |
| F-96 | Missing rename tests: unparsable target file gives Conflict and is untouched; in-place rename keeps nodes and edges. | |
| F-97 | A crash between write and remove leaves two flows (accepted, no data lost). | Accepted risk, no work planned (2026-10-11). |

## Follow-ups from P18 (2026-10-09)

| ID | Item | Notes |
|---|---|---|
| F-98 | Rename/delete guard misses the pre-run window (save, OAuth prepare); a rename during an interactive sign-in orphans in-memory tokens under the old name. Expose a "starting" state on the tab. | `FlowToolbar.tsx:228-299`. |
| F-99 | A Save sent after `rename_flow` but before `renameFlowTabs` recreates the old flow. | Single IPC round trip. |
| F-100 | Row clicked between rename success and list refetch opens a picker instead of an error. | |
| F-101 | Manual check: rename input accepts Space (TreeItem `onKeyDown` preventDefault); Flows group, rename, delete in the real app. | |
| F-102 | Rename/validation failure closes the input and discards typed text. | |

## Follow-ups from P19 (2026-10-09)

| ID | Item | Notes |
|---|---|---|
| F-103 | P20 must call `clear_run_cache` on workspace switch (the cache is not scoped by workspace yet). | DONE (verified 2026-10-11): `src-tauri/src/commands/workspaces.rs:86,128` call `clear_run_cache()`; service fn at `flow_execution_service.rs:975`. A Rust test through close/delete workspace is still missing (see F-115). |
| F-104 | A partial run opens a callback endpoint for every Wait in the flow. Open only Waits in the run plus Waits whose sender is in the run. P20 must not show other URLs as "waiting". | `open_all`. |
| F-105 | Fingerprint hashes the whole node kind including `label` and `debug`; renaming an upstream node refuses partial runs as changed. | `flow_run_cache.rs`. |
| F-106 | Seed outputs are deep-cloned on each partial run (up to 64 MiB transient). Use `Arc`. | DONE (verified 2026-10-11): `flow_run_cache.rs:33` holds `Option<Arc<CapturedOutput>>`; `Arc::ptr_eq` test at `:848`; commit 6b3a5a32. |
| F-107 | "Too large to keep" wording also appears when a node simply captured nothing. | `flow_partial.rs`. |

## Follow-ups from P22 / F-03 (2026-10-09)

| ID | Item | Notes |
|---|---|---|
| F-108 | A poisoned `run_cache` lock lets a kept id through (fails open); return Internal like `in_flight`. | `flow_execution_service.rs` `reserve`. |
| F-109 | No end-to-end test that Stop during the secret/token/callback setup ends the run before its first node. | |
| F-110 | A rejection from the `on*` subscription awaits in `handleRun` leaves `isStartingRef` stuck. | `FlowToolbar.tsx`. |
| F-111 | Closing a tab does not cancel its run; `patchFlowNodeStatus/Progress` and `setFlowRunResult` skip parked tabs. | pane-store. |
| F-112 | Manual check: two tabs of one flow, Stop, hide tab during start, switch tabs in a group. | |

## Follow-ups from P20 (2026-10-09)

| ID | Item | Notes |
|---|---|---|
| F-113 | Result strip counts only the partial run's nodes; add a "partial run" label. | |
| F-114 | Partial runs build on `tab.runId`, not on a run the user is viewing in history. | |
| F-115 | Missing tests: FlowPane-level `onRunNode` absence without a base run and `runBusy` with `pendingRunId`; Rust test for the run-cache clear on close/delete workspace; dedicated tests for the M3/M4 run-error paths. | Partly done (2026-10-11): `clear_run_cache` itself is tested (`flow_partial_run_tests.rs:629`), `runBusy` only in `NodeMenuButton.test.tsx`. The other tests are still missing. |
| F-116 | Real-app check of Run this node / Run from here: menu, refusal highlight, kept-result marks, Wait callback URLs, workspace close. | |

## Follow-ups from P23 / F-20 (2026-10-09)

| ID | Item | Notes |
|---|---|---|
| F-117 | `FlowService::lint` runs `validate` first, which is super-linear on very large flows (about 3.5 s at 20k If nodes). Fine at drawn sizes. | `validate.rs` `check_routing_inputs`. |
| F-118 | rustfmt drift in lint.rs and flow_service.rs (17 hunks). | |
| F-119 | `flow-issues.ts` still has the old "result is never shown" wording for the client no-path check (removed by P21). | DONE (verified 2026-10-11): the wording is gone from `flow-issues.ts`. |

## Follow-ups from P21 (2026-10-09)

| ID | Item | Notes |
|---|---|---|
| F-120 | The merge-rule tests (client-error-wins over `invalid_graph`, errors first, ghost filter) were written with the code, not seen failing first; the unmount hook test asserts only that console.error is not called. | `flow-lint.ts`, `useBackendFlowLints`. |
| F-121 | Real-app check of the issue count and popover with backend lint: new unwired If node counts once, opening a past run keeps warnings, deleting a node drops its lints. | |
| F-122 | F-37b done; F-23 done (2026-10-11, `lint.rs`). F-21, F-22, F-24 and F-25 done on branch worktree-flow-tier2 (2026-10-11); F-26 to F-30 still open. The lint context (`saved_request_exists`, `variable_is_known`) is still a stub that `validate_with_warnings` ignores (`lint.rs:52-72`). | |

## Follow-ups from the Tier 1 small-bug branch (2026-10-11)

Branch `worktree-flow-tier1-small`. Reviews logged in `.superpowers/sdd/flow-tier1-small/progress.md` in that worktree.

| ID | Item | Notes |
|---|---|---|
| F-123 | `redact_secrets` misses percent-encoded secrets (path parameters go through `urlencoding`), and secrets under `MIN_REDACTION_LEN` (6) are never masked. Feed `secret_forms()` into `redact_secrets` for history and the `RequestExecuted` event. | Found in the F-16 review. |
| F-124 | At the Wait deadline only the adapter's 1-slot channel is drained. Up to 16 calls queued in the server channel are dropped. Drain with a bounded `recv` and a short timeout. | Found in the final review of F-13. |
| F-125 | The per-flow auth epoch interrupts all of a flow's sign-ins during a multi-node preflight when any Auth node of that flow is deleted. | Accepted for F-17. |
| F-126 | The viewed-run banner is `role=status` but mounts with its text, so some screen readers do not announce it. Send the text through an always-mounted live region. | F-75(3). |
| F-127 | Input wires into If, Switch and Transform are editable in the Wires tab although their script is never run. | Found in the F-72(3) review. |
| F-128 | Drag-and-drop of a saved request checks the collection name only, not the workspace. | F-67 remainder. |
| F-129 | A run record older than the newest two loses its request and logs with no note in the Last run tab. | F-75(1). |
| F-130 | Console entries for a parked flow tab (`onStepLogs`, `onStepDebug`) still use `latestFlowTab()` and are labelled with the node id. Use `findFlowTabAnywhere`. | F-74 remainder. |
| F-131 | `cleanSaveMessage` masks only quoted text after match/case/value. Mirror the backend lint `sentence()` table for wait names, request paths and handle names. | F-76(6) remainder. |
| F-132 | Two tests are timing-sensitive: `wait_accepts_a_call_queued_when_the_deadline_has_passed` (1.3 s real sleep) and `a_steady_stream_of_calls_does_not_hold_the_wait_open_past_its_timeout` (3 s bound). FlowPane suites can time out at 5 s under heavy machine load. | Consider `--no-file-parallelism` or a larger `testTimeout` in CI. |
