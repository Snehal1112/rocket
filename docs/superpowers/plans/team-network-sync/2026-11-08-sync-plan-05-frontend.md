# Team Network Sync — Plan 5: Frontend, docs and manual checks

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 5 of 5.** Previous plan: `docs/superpowers/plans/team-network-sync/2026-11-08-sync-plan-04-tauri-e2e.md` (must be done and green; Plans 1 to 3 come before it). The commands `network_send_collection` (with `keep_in_sync`), `network_sync_start`, `network_sync_review`, `network_sync_confirm`, `network_sync_cancel`, `network_list_links`, `network_unlink` and the event channel `network-sync` exist.
**Next plan to run: none. Stop and show the user the whole change.**
**Recommended model: Sonnet.**

**Goal:** A Rocket user ticks **Keep in sync** when sending a collection. Later, the collection menu of either side shows **Sync with Ana**, **Unlink** and a status line. Pressing Sync asks the other user, who sees a review dialog (counts, script and auth marks, conflicts with a read-only side-by-side diff and the choices keep mine, keep theirs, keep both). The user who pressed Sync sees a short confirm dialog before anything is written. Both sides get a clear result toast. The Network panel and the Settings section list the links with Unlink. Audit rows describe a sent sync, a received sync and a removed link. The feature doc and manual-test section 12 say what was built.

**Architecture:** The backend (Plans 1 to 4) owns every sync. The frontend keeps a thin mirror in `network-store`: a `syncs` slice with `syncReviews` (entries waiting for an answer, each with the same 120 s expiry timer as pairing requests and transfer offers; `kind` is `review` for the planner and `confirm` for the starter), `syncProgress`, `dialogSyncId`, `dialogSyncEndedText`, and `links` (the `LinkDto` list). The slice is fed only by the `network-sync` event through `handleNetworkSync` in the existing `useNetworkEventBridge`, so it uses the same listener scope as the three other channels. Late events for another sync are ignored the way the transfer slice does: a finished-sync memory, `dialogSyncId === id` checks after every await, and dialog state keyed by sync id. The incoming side copies the transfer pattern: `useIncomingSyncNotice` shows one toast per entry with a fixed id, `SyncReviewDialog` is mounted once in `App.tsx`. The starting side is a pure function `startCollectionSync(link)` called from two new menu item groups in `CollectionNode.tsx` (dropdown and context menu, rendered by one small file so the two stay equal). The conflict diff is `SyncConflictDiff`, a thin read-only wrapper over the existing `src/components/git/DiffViewer.tsx`.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui, lucide-react, sonner, Monaco (through the existing `DiffViewer`), Vitest + Testing Library, Bun.

## Spec

`docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md`, sections 6 (flow), 8 (errors), 9 (interface), 12 (testing: frontend and manual). Binding names: `docs/superpowers/plans/team-network-sync/00-plan-index.md` ("Plan 1 produces" for `SyncEvent`, "Plan 4 produces / Plan 5 consumes"). Previous frontend plan, whose patterns this plan copies: `docs/superpowers/plans/team-network-send/2026-11-06-send-plan-05-frontend.md`.

## Global Constraints

- shadcn/ui primitives only. No raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`. Choosing keep mine, keep theirs or keep both uses `RadioGroup` and `RadioGroupItem` with `Label`. The checkbox uses `Checkbox` and `Label`. Unlink asks with `AlertDialog`. There is no progress primitive, so progress is text.
- Icons from `lucide-react` only (`RefreshCw`, `Unlink`, `ShieldAlert`, `KeyRound`, `GitCompare`, `Loader2`). No inline SVG.
- The conflict diff is Monaco, through `DiffViewer` (`hideStageToggle`). It is read-only (`DiffViewer` sets `readOnly: true`). No CodeMirror is added. No `type='search'` anywhere.
- Zustand: narrow selectors only. A selector returns a primitive or a reference that already lives in the store (`find` on a store array, a whole store array). A selector never builds a new array: filter in the component with `useMemo`. Never destructure the whole store at the top of a component.
- Forms follow the design system map: `FormField` for every labelled control (group mode for the checkbox row), `EmptyState`, `StateBlock` for inline errors (`tone='error'`), text sizes only `text-2xs`, `text-xs`, `text-sm` (dialog title `text-lg` from the primitive).
- Bun only: `bun run tsc --noEmit`, `bun run check`, `bun run lint` (auto-fix), `bun run test <paths>`. No Yarn or npm.
- Event payload fields are snake_case as sent (`sync_id`, `done_bytes`). Helper DTOs inside the event (`summary`, `conflicts`) and IPC DTOs are camelCase. Invoke arguments are camelCase. The values of `ConflictKind`, `Resolution`, `LinkState`, `SyncFinish` and `SyncRole` are snake_case words as in the index.
- Code comments are short full sentences that end with a period. No emojis.
- No commit during the plan. The user reviews the whole plan at the end. Then commit through `dev-workflow-skills:1-git-commit`, staging explicit paths only, conventional subjects (`feat(network): ...`, `test(network): ...`, `docs(network): ...`).
- Never show the full device id. The UI shows the short id only. A collection name and a file path are data: they are always rendered as text and never as HTML, and the collection name is passed to commands unchanged (no trim, no quoting, no normalisation). Errors never quote file contents or secret values. The peer's text is never shown: only fixed texts of this plan or the `message` the local backend put into `finished`.
- Verification per task: `bun run tsc --noEmit`, `bun run check` (if it reports format or import-order findings, run `bun run lint`, then `bun run check` again), and the task's `bun run test <paths>`.

## Review Focus

Each line names the test that pins it.

1. **A late event for another sync** (spec section 12): Task 1 bridge tests `a finish for another sync leaves the open dialog alone` and `ignores progress and reviews that arrive after the finish`; store tests `ignores progress and reviews for a finished sync`; Task 2 dialog tests `keeps the dialog of one sync when another sync finishes` and `does not show the progress of another sync`.
2. **Cancel is disabled once all bytes are in** (spec section 9): Task 2 dialog tests `disables Cancel once all bytes are in`, `keeps Cancel enabled while bytes are missing` and `Cancel cancels the sync and remembers it was local`.
3. **Every conflict needs a choice, and the decisions go out exactly** (index `resolve`): Task 2 dialog tests `keeps Accept off until every conflict has a choice` and `sends one decision per conflict with the wire resolution`; `hides keep both when one side has no file`.
4. **Conflict diff is read-only Monaco through DiffViewer, mine left and theirs right**: Task 2 `SyncConflictDiff` tests `shows mine on the left and theirs on the right through the git diff viewer`, `hides the stage toggle`, `shows an added file against an empty left side`; dialog test `still lets the user choose when the files cannot be loaded`.
5. **Script and auth marks and the sandbox warning on both dialogs** (spec section 6 step 5 and 8): Task 2 dialog tests `marks the changed scripts and auth files by path`, `warns that changed scripts run in the Safe sandbox` and `shows the same marks in the confirm dialog`.
6. **Duplicate `reviewReady`**: Task 1 store test `dedupes by sync id and does not extend the expiry`; bridge test `adds one row for a repeated review`; Task 2 hook test `shows no second toast for a duplicate review`.
7. **Late answer after the sync finished**: Task 2 dialog test `a late Accept answer does not bring a finished sync back`; store test `markSyncAnswered ignores a sync that is gone`.
8. **A repeated `finished` does not double the toast**: Task 1 bridge test `shows the result once, also when the finish event repeats`.
9. **Who is told what**: Task 1 bridge table (`announces applied`, `announces declined to the starter only`, `says nothing when the local user cancelled`, `announces a base mismatch`, `announces a missing link`, `announces unconfirmed`, `uses the backend message on failed or a fixed text`).
10. **The dialog is open while the peer goes away** (a `failed` with a message on an unanswered review): Task 1 bridge test `keeps an unanswered dialog open and says why`; Task 2 dialog test `shows why an unanswered review ended`; `expired` has `keeps the dialog open on an expired finish`.
11. **Unlink**: Task 1 store tests `unlink reloads the list also when the call fails`; Task 2 menu tests `shows Unlink and the status line when the peer is offline` and `asks before it unlinks`; Task 3 `LinkList` tests.
12. **The Sync item is shown only while the peer is paired and online** (spec section 6 step 1): Task 2 menu tests `hides Sync with ... when the peer is offline` and `shows one group per linked peer`.
13. **Names are data**: Task 1 bridge test `keeps a double quote in the collection name`; Task 2 dialog tests `shows a collection name with a double quote as given` and `names a node without a display name`; Task 3 audit test `shows a collection name with a double quote`.
14. **Keep in sync is off by default and reaches the command** (spec section 9): Task 2 `SendCollectionDialog` tests `sends keepInSync false by default`, `sends keepInSync true when ticked` and `resets the box when the dialog closes`; Task 1 wrapper test `sends keepInSync with the collection`.
15. **Listener lifecycle**: Task 1 bridge tests `ignores sync events after unmount` and `stops a sync listener that finishes registering after unmount`.
16. **Inline errors use `StateBlock`**: Task 2 dialog test `shows the error in a state block and keeps the buttons when the answer fails` (checks `data-slot='state-block'` and `data-tone='error'`).
17. **Audit rows with a short id, the outcome and no file names** (spec section 10): Task 3 audit tests for the three kinds.
18. **Only the planner decides conflicts** (spec section 6 step 6): Task 2 dialog test `shows no conflict choices in the confirm dialog`.

## Rulings and doubts (recorded for the ledger)

- **Doubt A (blocks nothing, needs a backend ruling): the content behind a conflict.** `ConflictView` carries only `path`, `kind` and two hashes. A "side-by-side diff" (spec section 6 step 6, section 9) needs the text of both versions, and the index flow exchanges only manifests before the review, so the planner holds its own file but not the peer's. This plan consumes one more read command and records it as a request to Plans 3 and 4: `network_sync_conflict_files(sync_id: String, path: String) -> ConflictFilesDto { mine: Option<String>, theirs: Option<String> }`, camelCase, text of the exported (scrubbed) file, `None` for a missing side. The planner must have the peer's version before `ReviewReady` (for example a `Files` payload that holds only the conflicting paths, fetched before the review). If the backend cannot provide `theirs`, it returns `theirs: None` with an `Err("unavailable")` and the dialog falls back (Ruling 4). This plan builds the wrapper `getNetworkSyncConflictFiles` and tests it with a mock. Cost if the command never exists: the dialog shows "The files could not be loaded." for every conflict and the user decides from the kind and the paths only. The final review must check that Plans 3 and 4 added the command to `src-tauri/app-commands.txt`.
- **Doubt B, progress totals.** `Progress { sync_id, done_bytes, total_bytes }` has no payload tag. A sync moves three payloads (manifest, plan, files). The dialog shows the latest snapshot and disables Cancel when `total_bytes > 0 && done_bytes >= total_bytes`. If the backend emits `progress` for the small manifest and plan payloads, Cancel flickers off for a moment. Proposed to Plan 3: emit `Progress` for the `Files` payloads only, or make the totals cover the whole sync. This plan works either way.
- **Doubt C, helper DTO casing.** The index writes `ConflictView { path, kind, mine_hash, theirs_hash }` and `SyncSummary { added_here, ... }` in Rust field style, and its rule says helper DTOs inside the events are camelCase. This plan reads `mineHash`, `theirsHash`, `addedHere`, `changedHere`, `deletedHere`, `sentAdded`, `sentChanged`, `sentDeleted`, `scriptsChanged`, `authChanged` (camelCase). If Plan 1 pinned snake_case, change the TS interfaces only; every component reads the camelCase names through `SyncSummary` and `ConflictView`.
- **Ruling 1, the two dialogs are one component.** `SyncReviewDialog` renders by `entry.kind`: `review` (planner: counts, marks, conflicts, Accept and Decline) and `confirm` (starter: counts and marks, Apply and Decline). `ReviewReady` with `role: 'starter'` is treated as `confirm` (the index gives `role` only on `ReviewReady`; a planner always decides conflicts, a starter never does).
- **Ruling 2, no default choice.** A conflict starts with no radio chosen and Accept stays disabled until each conflict has one. Data is never decided by a default.
- **Ruling 3, keep both is hidden when one side has no file.** A delete against an edit (`mineHash` or `theirsHash` is `null`) offers only keep mine and keep theirs, because "keep both" has no second copy to name. The index does not forbid it. If Plan 1 `resolve` accepts `KeepBoth` there, the radio can be added back by deleting one condition.
- **Ruling 4, diff fallback.** If `getNetworkSyncConflictFiles` rejects, the row shows `StateBlock tone='error'` "The files could not be loaded. You can still choose." and the radios keep working.
- **Ruling 5, the starter confirm opens at once.** `confirmNeeded` follows the user's own action, so the bridge opens the dialog when no other sync dialog is open, and the hook toasts only entries that the open dialog does not show. A planner `reviewReady` never opens by itself (the user may be typing): it gets a toast with Review.
- **Ruling 6, accepted entries do not expire locally.** `markSyncAnswered` clears the 120 s timer, as `markTransferAccepted` does. The row stays until `finished`.
- **Ruling 7, finished memory.** The store remembers the last 100 finished sync ids and ignores progress or reviews for them, and a second `finished`. The memory is cleared by `reset()`.
- **Ruling 8, the waiting toast of the starter.** `startCollectionSync` shows "Waiting for <name> to review the sync." with id `network-sync-waiting-<id>`, no auto close and a Cancel action (`cancelNetworkSync`). The first `progress` for that sync switches the text to "Syncing with <name>..." (the action stays, sonner merges options for a reused id). The bridge dismisses the toast on the first `finished`. The result toast has its own id `network-sync-result-<id>` so it does not keep the Cancel action.
- **Ruling 9, who cancelled.** The store keeps `cancelledHere` (sync ids the local user cancelled) and `startedSyncs` (syncs started here, with the node and collection name) outside the state. A `cancelled` finish shows "<name> cancelled the sync." unless the id is in `cancelledHere`. `declined` shows "<name> declined the sync." only for a sync started here.
- **Ruling 10, links are matched by collection name.** `LinkDto` has no collection root, so the menu picks the links whose `collectionName` equals the node's `summary.name`. A collection renamed after the link was made loses its menu items until the link record follows (the backend marks it broken by uid or root; that is a Plan 3 matter). The Settings and panel lists show every link and so still offer Unlink.
- **Ruling 11, status text.** `in_sync` "In sync with <name>", `changes_waiting` "Changes waiting to sync with <name>", `out_of_date` "Out of date with <name>. Sync again.", `broken` "Link to <name> is broken. Unlink and send the collection again." The line is a menu label, not an item.
- **Ruling 12, audit outcome.** The two sync kinds carry `outcome` (wire name, free text in the index). A completed sync is `applied` by the finish names, so `outcomeSuffix` hides `completed` and `applied`; every other outcome shows in brackets, as for transfers. If Plan 3 uses another word for success the row shows it in brackets, which is harmless.
- **Ruling 13, links refresh.** `refreshLinks` uses a sequence counter like `refreshNodes` and never throws: a failed load keeps the old list (no store `error` is set, so the panel and Settings keep working). `linkChanged` and every `finished` trigger it. The bridge also loads the list at startup.
- **Ruling 14, docs index.** `.claude/docs-index.md` gets a pointer change only if its team-network line does not already mention sync (the line at "`docs/features/team-network.md` for the team network (...)"). Stage only that hunk with `git add -p`.
- **Ruling 15, no design-system page edit.** As in the send plan, this plan does not touch the design system map page.

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src/lib/tauri-api.ts` (Team network section) | modify | `keepInSync` on `sendNetworkCollection`; types `LinkState`, `NetworkLink`, `SyncStarted`, `SyncRole`, `ConflictKind`, `SyncResolution`, `SyncDecision`, `SyncSummary`, `ConflictView`, `SyncFinish`, `ConflictFiles`, `NetworkSyncEvent`; 7 wrappers and `onNetworkSync` |
| `src/lib/tauri-api.ts` (`AuditEventKind` union) | modify | Three kinds with wire names |
| `src/lib/__tests__/tauri-api.network.test.ts` | modify | Wrapper and listener tests, existing send assertion gains `keepInSync` |
| `src/lib/network-sync-utils.ts` | create | `linkStatusText`, `syncCountsText`, `conflictKindText`, `syncResultToastId`, `syncWaitingToastId`, `SAFE_SANDBOX_NOTE` |
| `src/lib/__tests__/network-sync-utils.test.ts` | create | Tests for the helpers |
| `src/stores/network-store.ts` | modify | `syncs` slice and `links` |
| `src/stores/__tests__/network-store.sync.test.ts` | create | Slice tests |
| `src/test/network-fixtures.ts` | modify | `makeSyncSummary`, `makeSyncReview`, `makeLink` |
| `src/lib/network-event-bridge.ts` | modify | `handleNetworkSync`, fourth listener, startup link load |
| `src/lib/__tests__/network-event-bridge.sync.test.ts` | create | Handler tests |
| `src/lib/__tests__/network-event-bridge.test.ts` | modify | Mock gains `onNetworkSync` and `listNetworkLinks`, listener lifecycle tests |
| `src/lib/network-sync-actions.ts` | create | `startCollectionSync(link)` and the waiting toast |
| `src/lib/__tests__/network-sync-actions.test.ts` | create | Tests |
| `src/components/network/SendCollectionDialog.tsx` | modify | "Keep in sync" checkbox |
| `src/components/network/__tests__/SendCollectionDialog.test.tsx` | modify | Existing call assertions gain `false`, new tests |
| `src/components/network/SyncConflictDiff.tsx` | create | Loads both versions and renders the read-only `DiffViewer` |
| `src/components/network/SyncReviewDialog.tsx` | create | Review (planner) and confirm (starter) dialog, running view, ended view |
| `src/components/network/IncomingSyncToast.tsx` | create | Body of the incoming sync toast (Review only) |
| `src/hooks/useIncomingSyncNotice.tsx` | create | One toast per unanswered entry |
| `src/components/network/UnlinkDialog.tsx` | create | `AlertDialog` that confirms an unlink |
| `src/components/network/__tests__/SyncConflictDiff.test.tsx`, `SyncReviewDialog.test.tsx`, `IncomingSyncToast.test.tsx`, `UnlinkDialog.test.tsx` | create | Component tests |
| `src/hooks/__tests__/useIncomingSyncNotice.test.tsx` | create | Hook tests |
| `src/App.tsx` | modify | Mount the hook and `SyncReviewDialog` |
| `src/hooks/useCollectionSyncLinks.ts` | create | Links of one collection with the peer's online flag |
| `src/components/collections/CollectionSyncMenuItems.tsx` | create | Status line, Sync and Unlink items for the dropdown and the context menu |
| `src/components/collections/CollectionNode.tsx` | modify | Render the items in both menus, mount `UnlinkDialog` |
| `src/components/collections/__tests__/CollectionNode.sync.test.tsx` | create | Menu tests |
| `src/components/network/LinkList.tsx` | create | List of links with Unlink |
| `src/components/network/NetworkPanel.tsx` | modify | "Linked collections" section, `onUnlink` prop |
| `src/components/status-bar/NetworkStatusItem.tsx` | modify | Host `UnlinkDialog` outside the popover |
| `src/components/settings/sections/NetworkSection.tsx` | modify | "Linked collections" block |
| `src/components/network/__tests__/LinkList.test.tsx`, `NetworkPanel.links.test.tsx`; `src/components/settings/sections/__tests__/NetworkSection.links.test.tsx` | create | List tests |
| `src/components/audit/AuditEventRow.tsx` | modify | Three summary cases |
| `src/components/audit/__tests__/AuditEventRow.network.test.tsx` | modify | Row tests |
| `docs/features/team-network.md` | modify | "Keep a collection in sync" section, code map row, limits, test list |
| `docs/manual-tests/team-network.md` | modify | Section 12 |
| `docs/roadmaps/roadmap.md` | modify | R-83 row text |
| `.claude/docs-index.md` | modify (only if needed) | Pointer text mentions sync (stage only this hunk, ruling 14) |

---

## Task 1: Types, wrappers, helpers, store slice and bridge

**Files:**
- Modify: `src/lib/tauri-api.ts`, `src/lib/__tests__/tauri-api.network.test.ts`, `src/stores/network-store.ts`, `src/test/network-fixtures.ts`, `src/lib/network-event-bridge.ts`, `src/lib/__tests__/network-event-bridge.test.ts`
- Create: `src/lib/network-sync-utils.ts`, `src/lib/__tests__/network-sync-utils.test.ts`, `src/stores/__tests__/network-store.sync.test.ts`, `src/lib/__tests__/network-event-bridge.sync.test.ts`, `src/lib/network-sync-actions.ts`, `src/lib/__tests__/network-sync-actions.test.ts`

**Interfaces:**
- Consumes: commands `network_send_collection(deviceId, collection, includeEnvironments, keepInSync)`, `network_sync_start(linkId) -> { syncId }`, `network_sync_review(syncId, accept, decisions)`, `network_sync_confirm(syncId, accept)`, `network_sync_cancel(syncId)`, `network_list_links() -> LinkDto[]`, `network_unlink(linkId)`, and (Doubt A) `network_sync_conflict_files(syncId, path) -> { mine, theirs }`; channel `network-sync`; `createListenerScope`, `errorText`, `toastError`, `peerName`, `formatBytes`.
- Produces: the types and wrappers in the File Structure table (`startNetworkSync`, `reviewNetworkSync`, `confirmNetworkSync`, `cancelNetworkSync`, `listNetworkLinks`, `unlinkNetworkLink`, `getNetworkSyncConflictFiles`, `onNetworkSync`); helpers `linkStatusText`, `syncCountsText`, `conflictKindText`, `syncResultToastId`, `syncWaitingToastId`, `SAFE_SANDBOX_NOTE`; store `SyncReviewEntry`, `SyncProgress`, `syncReviews`, `syncProgress`, `dialogSyncId`, `dialogSyncEndedText`, `links`, `addSyncReview`, `removeSyncReview`, `markSyncAnswered`, `setSyncProgress`, `finishSync`, `openSyncDialog`, `closeSyncDialog`, `endSyncDialog`, `trackStartedSync`, `startedSync`, `advanceStartedSync`, `markSyncCancelledHere`, `unmarkSyncCancelledHere`, `wasSyncCancelledHere`, `refreshLinks`, `unlink`; `handleNetworkSync`; `startCollectionSync`.

### 1a. Types and wrappers

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. (This plan does not change collection files, but the sync moves them. The note is here so the reader knows the wire names come from that format.)

- [ ] **Step 2: Write the failing wrapper tests**

In `src/lib/__tests__/tauri-api.network.test.ts`, extend the import list (keep it alphabetical) with `cancelNetworkSync`, `confirmNetworkSync`, `getNetworkSyncConflictFiles`, `listNetworkLinks`, `onNetworkSync`, `reviewNetworkSync`, `startNetworkSync`, `unlinkNetworkLink`. Change the existing send assertion: in the test that contains `sendNetworkCollection('DEV1', 'Orders "v2" café', true)` the call becomes `sendNetworkCollection('DEV1', 'Orders "v2" café', true, false)` and the expected invoke arguments gain `keepInSync: false`; in the test that contains `sendNetworkCollection('DEV1', 'Orders', false)` the call becomes `sendNetworkCollection('DEV1', 'Orders', false, false)`. Append this block at the end of the file:

```ts
describe('team network sync commands', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('sends keepInSync with the collection', async () => {
    vi.mocked(invoke).mockResolvedValue({ transferId: 't1', notes: [] });
    await sendNetworkCollection('DEV1', 'Orders', false, true);
    expect(invoke).toHaveBeenLastCalledWith('network_send_collection', {
      deviceId: 'DEV1',
      collection: 'Orders',
      includeEnvironments: false,
      keepInSync: true,
    });
  });

  it('starts a sync and returns its id', async () => {
    vi.mocked(invoke).mockResolvedValue({ syncId: 's1' });
    await expect(startNetworkSync('L1')).resolves.toEqual({ syncId: 's1' });
    expect(invoke).toHaveBeenLastCalledWith('network_sync_start', { linkId: 'L1' });
  });

  it('sends the decisions of a review unchanged', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const decisions = [
      { path: 'Orders "v2"/a.yml', resolution: 'keep_both' as const },
      { path: 'b.yml', resolution: 'keep_mine' as const },
    ];
    await reviewNetworkSync('s1', true, decisions);
    expect(invoke).toHaveBeenLastCalledWith('network_sync_review', {
      syncId: 's1',
      accept: true,
      decisions,
    });
    await reviewNetworkSync('s1', false, []);
    expect(invoke).toHaveBeenLastCalledWith('network_sync_review', {
      syncId: 's1',
      accept: false,
      decisions: [],
    });
  });

  it('confirms, cancels, lists and unlinks', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await confirmNetworkSync('s1', true);
    expect(invoke).toHaveBeenLastCalledWith('network_sync_confirm', { syncId: 's1', accept: true });
    await cancelNetworkSync('s1');
    expect(invoke).toHaveBeenLastCalledWith('network_sync_cancel', { syncId: 's1' });
    await listNetworkLinks();
    expect(invoke).toHaveBeenLastCalledWith('network_list_links');
    await unlinkNetworkLink('L1');
    expect(invoke).toHaveBeenLastCalledWith('network_unlink', { linkId: 'L1' });
  });

  it('reads the two versions of a conflict', async () => {
    vi.mocked(invoke).mockResolvedValue({ mine: 'a', theirs: null });
    await expect(getNetworkSyncConflictFiles('s1', 'a.yml')).resolves.toEqual({
      mine: 'a',
      theirs: null,
    });
    expect(invoke).toHaveBeenLastCalledWith('network_sync_conflict_files', {
      syncId: 's1',
      path: 'a.yml',
    });
  });

  it('passes a rejected command through unchanged', async () => {
    vi.mocked(invoke).mockRejectedValue('Another sync is already running for this link.');
    await expect(startNetworkSync('L1')).rejects.toBe(
      'Another sync is already running for this link.',
    );
  });

  it('listens on network-sync and passes the payload', async () => {
    const unlisten = vi.fn();
    vi.mocked(listen).mockResolvedValue(unlisten);
    const handler = vi.fn();
    await expect(onNetworkSync(handler)).resolves.toBe(unlisten);
    expect(listen).toHaveBeenCalledWith('network-sync', expect.any(Function));
    const callback = vi.mocked(listen).mock.calls[0][1] as (e: { payload: unknown }) => void;
    callback({ payload: { type: 'linkChanged', link_id: 'L1' } });
    expect(handler).toHaveBeenCalledWith({ type: 'linkChanged', link_id: 'L1' });
  });
});
```

- [ ] **Step 3: Run the wrapper tests and see them fail**

Run: `bun run test src/lib/__tests__/tauri-api.network.test.ts`
Expected: FAIL. The new names are not exported (`startNetworkSync is not a function` and the like).

- [ ] **Step 4: Add the types and wrappers**

In `src/lib/tauri-api.ts`, in the Team network section, directly after `onNetworkTransfer`:

```ts
/** The wire value of `LinkState` in `rocket-network`. */
export type LinkState = 'in_sync' | 'changes_waiting' | 'out_of_date' | 'broken';

/** One link to a collection on a paired node (`LinkDto`). */
export interface NetworkLink {
  linkId: string;
  deviceId: string;
  displayName: string;
  shortId: string;
  collectionName: string;
  state: LinkState;
  /** RFC 3339. */
  updatedAt: string;
}

export interface SyncStarted {
  syncId: string;
}

export type SyncRole = 'planner' | 'starter';
export type ConflictKind = 'both_changed' | 'delete_vs_edit' | 'both_added' | 'rename_vs_edit';
export type SyncResolution = 'keep_mine' | 'keep_theirs' | 'keep_both';

/** One answer for one conflict (`DecisionDto`). */
export interface SyncDecision {
  path: string;
  resolution: SyncResolution;
}

/** Counts and marks of a planned sync. "Here" is the node that shows the dialog. */
export interface SyncSummary {
  addedHere: number;
  changedHere: number;
  deletedHere: number;
  sentAdded: number;
  sentChanged: number;
  sentDeleted: number;
  /** Paths of changed files that contain scripts. */
  scriptsChanged: string[];
  /** Paths of changed files that contain auth. */
  authChanged: string[];
}

/** A path changed on both sides. A hash is null when that side has no file. */
export interface ConflictView {
  path: string;
  kind: ConflictKind;
  mineHash: string | null;
  theirsHash: string | null;
}

/** How a sync ended. The values are the snake_case `SyncFinish` of `rocket-network`. */
export type SyncFinish =
  | 'applied'
  | 'declined'
  | 'expired'
  | 'cancelled'
  | 'failed'
  | 'base_mismatch'
  | 'link_missing'
  | 'unconfirmed';

/** The two versions of a conflicting file, as exported text. A side is null when it has no file. */
export interface ConflictFiles {
  mine: string | null;
  theirs: string | null;
}

/** Channel `network-sync`. Fields are snake_case as sent. */
export type NetworkSyncEvent =
  | {
      type: 'reviewReady';
      sync_id: string;
      link_id: string;
      device_id: string;
      short_id: string;
      display_name: string;
      collection_name: string;
      summary: SyncSummary;
      conflicts: ConflictView[];
      role: SyncRole;
    }
  | {
      type: 'confirmNeeded';
      sync_id: string;
      link_id: string;
      device_id: string;
      short_id: string;
      display_name: string;
      collection_name: string;
      summary: SyncSummary;
    }
  | { type: 'progress'; sync_id: string; done_bytes: number; total_bytes: number }
  | {
      type: 'finished';
      sync_id: string;
      link_id: string;
      display_name: string;
      collection_name: string;
      outcome: SyncFinish;
      /** A fixed text from the local backend, else null. */
      message: string | null;
    }
  | { type: 'linkChanged'; link_id: string };

export const startNetworkSync = (linkId: string): Promise<SyncStarted> =>
  invoke<SyncStarted>('network_sync_start', { linkId });

/** The planner answers. Decisions are empty on a decline. */
export const reviewNetworkSync = (
  syncId: string,
  accept: boolean,
  decisions: SyncDecision[],
): Promise<void> => invoke<void>('network_sync_review', { syncId, accept, decisions });

/** The starter answers the confirm dialog. */
export const confirmNetworkSync = (syncId: string, accept: boolean): Promise<void> =>
  invoke<void>('network_sync_confirm', { syncId, accept });

export const cancelNetworkSync = (syncId: string): Promise<void> =>
  invoke<void>('network_sync_cancel', { syncId });

export const listNetworkLinks = (): Promise<NetworkLink[]> =>
  invoke<NetworkLink[]>('network_list_links');

export const unlinkNetworkLink = (linkId: string): Promise<void> =>
  invoke<void>('network_unlink', { linkId });

/** Proposed command (plan doubt A). Rejects when the backend cannot supply the files. */
export const getNetworkSyncConflictFiles = (
  syncId: string,
  path: string,
): Promise<ConflictFiles> =>
  invoke<ConflictFiles>('network_sync_conflict_files', { syncId, path });

export const onNetworkSync = (handler: (event: NetworkSyncEvent) => void): Promise<UnlistenFn> =>
  listen<NetworkSyncEvent>('network-sync', (e) => handler(e.payload));
```

Then change `sendNetworkCollection` (quoted anchor `export const sendNetworkCollection = (`) to take a fourth parameter `keepInSync: boolean` and invoke `'network_send_collection', { deviceId, collection, includeEnvironments, keepInSync }`. Update its doc comment: "Resolves once the offer is on its way. `keepInSync` asks for a link to be made when the transfer completes."

- [ ] **Step 5: Run the wrapper tests and the type check**

Run: `bun run test src/lib/__tests__/tauri-api.network.test.ts`
Expected: PASS, all tests in the file.
Run: `bun run tsc --noEmit`
Expected: errors only in `SendCollectionDialog.tsx` (the call has three arguments) and in test files that call `sendNetworkCollection` with three arguments. Task 2 fixes the dialog; Step 6 fixes the test calls.

- [ ] **Step 6: Fix the callers of `sendNetworkCollection` that are not the dialog**

In `src/components/collections/__tests__/CollectionNode.send.test.tsx` the assertions `toHaveBeenCalledWith('DEV1', 'my-collection', false)` and `toHaveBeenCalledWith('DEV1', name, false)` become `('DEV1', 'my-collection', false, false)` and `('DEV1', name, false, false)`. In `src/components/network/__tests__/SendCollectionDialog.test.tsx` the three assertions at `toHaveBeenCalledWith('DEV1', 'Orders', false)`, `('DEV1', 'Orders', true)` and `('DEV1', name, false)` gain a trailing `false`. The component change is Task 2; these tests stay red until then. That is expected: do not run them yet.

### 1b. Helpers

- [ ] **Step 7: Write the failing helper tests**

Create `src/lib/__tests__/network-sync-utils.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  conflictKindText,
  linkStatusText,
  SAFE_SANDBOX_NOTE,
  syncCountsText,
  syncResultToastId,
  syncWaitingToastId,
} from '@/lib/network-sync-utils';

describe('linkStatusText', () => {
  it.each([
    ['in_sync', 'In sync with Ana Lima'],
    ['changes_waiting', 'Changes waiting to sync with Ana Lima'],
    ['out_of_date', 'Out of date with Ana Lima. Sync again.'],
    ['broken', 'Link to Ana Lima is broken. Unlink and send the collection again.'],
  ] as const)('describes %s', (state, text) => {
    expect(linkStatusText(state, 'Ana Lima')).toBe(text);
  });

  it('names a node without a display name', () => {
    expect(linkStatusText('in_sync', '  ')).toBe('In sync with Unnamed node');
  });

  it('says nothing wrong about a state added later', () => {
    expect(linkStatusText('future' as never, 'Ana Lima')).toBe('Linked with Ana Lima');
  });
});

describe('syncCountsText', () => {
  it('lists only the counts above zero', () => {
    expect(syncCountsText(2, 0, 1)).toBe('2 added, 1 deleted');
    expect(syncCountsText(0, 3, 0)).toBe('3 changed');
  });

  it('says no changes for all zero', () => {
    expect(syncCountsText(0, 0, 0)).toBe('no changes');
  });
});

describe('conflictKindText', () => {
  it.each([
    ['both_changed', 'Changed on both nodes'],
    ['delete_vs_edit', 'Deleted on one node, changed on the other'],
    ['both_added', 'Added on both nodes with different content'],
    ['rename_vs_edit', 'Renamed on one node, changed on the other'],
  ] as const)('describes %s', (kind, text) => {
    expect(conflictKindText(kind)).toBe(text);
  });

  it('has a fallback for a kind added later', () => {
    expect(conflictKindText('future' as never)).toBe('Changed on both nodes');
  });
});

describe('ids and notes', () => {
  it('gives the waiting toast and the result toast different ids', () => {
    expect(syncWaitingToastId('s1')).toBe('network-sync-waiting-s1');
    expect(syncResultToastId('s1')).toBe('network-sync-result-s1');
  });

  it('says changed scripts run in the Safe sandbox', () => {
    expect(SAFE_SANDBOX_NOTE).toBe(
      'Changed scripts run in the Safe sandbox. Developer mode, host environment and vault access stay off until you allow them.',
    );
  });
});
```

- [ ] **Step 8: Run and see it fail**

Run: `bun run test src/lib/__tests__/network-sync-utils.test.ts`
Expected: FAIL, the module does not exist.

- [ ] **Step 9: Create `src/lib/network-sync-utils.ts`**

Signatures and pinned values:

```ts
import { peerName } from '@/lib/network-utils';
import type { ConflictKind, LinkState } from '@/lib/tauri-api';

export function linkStatusText(state: LinkState, displayName: string): string;
// "<n> added, <n> changed, <n> deleted", zero counts left out, "no changes" when all are zero.
export function syncCountsText(added: number, changed: number, deleted: number): string;
export function conflictKindText(kind: ConflictKind): string;
export const syncResultToastId = (syncId: string): string => `network-sync-result-${syncId}`;
export const syncWaitingToastId = (syncId: string): string => `network-sync-waiting-${syncId}`;
export const SAFE_SANDBOX_NOTE: string;
```

The texts are the ones in the test. `linkStatusText` uses `peerName`, with a `default` branch returning `Linked with <name>`. `conflictKindText` has a `default` branch returning the `both_changed` text.

- [ ] **Step 10: Run the helper tests**

Run: `bun run test src/lib/__tests__/network-sync-utils.test.ts`
Expected: PASS, 11 tests.

### 1c. Store slice

- [ ] **Step 11: Add the fixtures**

In `src/test/network-fixtures.ts` add (and import the types `NetworkLink`, `SyncSummary` from `@/lib/tauri-api` and `SyncReviewEntry` from `@/stores/network-store`):

```ts
/** A summary with a few counts and no marks. */
export function makeSyncSummary(patch: Partial<SyncSummary> = {}): SyncSummary {
  return {
    addedHere: 1,
    changedHere: 2,
    deletedHere: 0,
    sentAdded: 0,
    sentChanged: 1,
    sentDeleted: 0,
    scriptsChanged: [],
    authChanged: [],
    ...patch,
  };
}

/** A review the planner has not answered yet, with no conflicts. */
export function makeSyncReview(patch: Partial<SyncReviewEntry> = {}): SyncReviewEntry {
  return {
    syncId: 's1',
    linkId: 'L1',
    deviceId: 'DEV1',
    shortId: 'AAAA-1111',
    displayName: 'Ben Ortiz',
    collectionName: 'Orders',
    summary: makeSyncSummary(),
    conflicts: [],
    kind: 'review',
    answered: false,
    ...patch,
  };
}

/** A link in sync with an online node. */
export function makeLink(patch: Partial<NetworkLink> = {}): NetworkLink {
  return {
    linkId: 'L1',
    deviceId: 'DEV1',
    displayName: 'Ben Ortiz',
    shortId: 'AAAA-1111',
    collectionName: 'Orders',
    state: 'in_sync',
    updatedAt: '2026-11-08T10:00:00Z',
    ...patch,
  };
}
```

- [ ] **Step 12: Write the failing store tests**

Create `src/stores/__tests__/network-store.sync.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { INCOMING_TTL_MS, useNetworkStore } from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeLink, makeSyncReview } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  api: {
    listNetworkLinks: vi.fn(),
    unlinkNetworkLink: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);

const store = () => useNetworkStore.getState();

describe('network store sync slice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.listNetworkLinks.mockResolvedValue([]);
    mocks.api.unlinkNetworkLink.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('adds a review once', () => {
    expect(store().addSyncReview(makeSyncReview())).toBe(true);
    expect(store().syncReviews).toHaveLength(1);
  });

  it('dedupes by sync id and does not extend the expiry', () => {
    vi.useFakeTimers();
    store().addSyncReview(makeSyncReview());
    vi.advanceTimersByTime(INCOMING_TTL_MS - 1000);
    expect(store().addSyncReview(makeSyncReview())).toBe(false);
    expect(store().syncReviews).toHaveLength(1);
    vi.advanceTimersByTime(1000);
    expect(store().syncReviews).toHaveLength(0);
  });

  it('keeps two reviews, each with its own timer', () => {
    vi.useFakeTimers();
    store().addSyncReview(makeSyncReview({ syncId: 's1' }));
    vi.advanceTimersByTime(60_000);
    store().addSyncReview(makeSyncReview({ syncId: 's2' }));
    vi.advanceTimersByTime(60_000);
    expect(store().syncReviews.map((r) => r.syncId)).toEqual(['s2']);
  });

  it('does not expire an answered review', () => {
    vi.useFakeTimers();
    store().addSyncReview(makeSyncReview());
    store().markSyncAnswered('s1');
    vi.advanceTimersByTime(INCOMING_TTL_MS * 3);
    expect(store().syncReviews).toHaveLength(1);
    expect(store().syncReviews[0].answered).toBe(true);
  });

  it('markSyncAnswered ignores a sync that is gone', () => {
    store().addSyncReview(makeSyncReview());
    store().finishSync('s1');
    store().markSyncAnswered('s1');
    expect(store().syncReviews).toHaveLength(0);
  });

  it('finishSync returns false the second time', () => {
    store().addSyncReview(makeSyncReview());
    expect(store().finishSync('s1')).toBe(true);
    expect(store().finishSync('s1')).toBe(false);
  });

  it('ignores progress and reviews for a finished sync', () => {
    store().finishSync('s1');
    store().setSyncProgress({ syncId: 's1', doneBytes: 1, totalBytes: 2 });
    expect(store().addSyncReview(makeSyncReview())).toBe(false);
    expect(store().syncProgress).toHaveLength(0);
    expect(store().syncReviews).toHaveLength(0);
  });

  it('keeps progress per sync', () => {
    store().setSyncProgress({ syncId: 's1', doneBytes: 1, totalBytes: 10 });
    store().setSyncProgress({ syncId: 's2', doneBytes: 5, totalBytes: 10 });
    store().setSyncProgress({ syncId: 's1', doneBytes: 4, totalBytes: 10 });
    expect(store().syncProgress).toEqual([
      { syncId: 's1', doneBytes: 4, totalBytes: 10 },
      { syncId: 's2', doneBytes: 5, totalBytes: 10 },
    ]);
  });

  it('finishSync forgets the started sync and the local cancel', () => {
    expect(store().trackStartedSync('s1', { displayName: 'Ben', collectionName: 'Orders' })).toBe(
      true,
    );
    store().markSyncCancelledHere('s1');
    expect(store().startedSync('s1')).toEqual({ displayName: 'Ben', collectionName: 'Orders' });
    expect(store().wasSyncCancelledHere('s1')).toBe(true);
    store().finishSync('s1');
    expect(store().startedSync('s1')).toBeUndefined();
    expect(store().wasSyncCancelledHere('s1')).toBe(false);
  });

  it('does not track a sync that already finished', () => {
    store().finishSync('s1');
    expect(store().trackStartedSync('s1', { displayName: 'Ben', collectionName: 'Orders' })).toBe(
      false,
    );
  });

  it('advances a started sync once', () => {
    store().trackStartedSync('s1', { displayName: 'Ben', collectionName: 'Orders' });
    expect(store().advanceStartedSync('s1')).toBe('Ben');
    expect(store().advanceStartedSync('s1')).toBeNull();
    expect(store().advanceStartedSync('other')).toBeNull();
  });

  it('opens and closes the dialog and clears the ended text', () => {
    store().openSyncDialog('s1');
    store().endSyncDialog('The other node went offline.');
    expect(store().dialogSyncEndedText).toBe('The other node went offline.');
    store().openSyncDialog('s2');
    expect(store().dialogSyncEndedText).toBeNull();
    store().closeSyncDialog();
    expect(store().dialogSyncId).toBeNull();
  });

  it('loads the links', async () => {
    mocks.api.listNetworkLinks.mockResolvedValue([makeLink()]);
    await store().refreshLinks();
    expect(store().links).toEqual([makeLink()]);
  });

  it('drops a stale links response', async () => {
    const first = createDeferred<ReturnType<typeof makeLink>[]>();
    mocks.api.listNetworkLinks
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce([makeLink({ linkId: 'NEW' })]);
    const slow = store().refreshLinks();
    await store().refreshLinks();
    first.resolve([makeLink({ linkId: 'OLD' })]);
    await slow;
    expect(store().links.map((l) => l.linkId)).toEqual(['NEW']);
  });

  it('keeps the old links when the load fails and sets no error', async () => {
    mocks.api.listNetworkLinks.mockResolvedValueOnce([makeLink()]);
    await store().refreshLinks();
    mocks.api.listNetworkLinks.mockRejectedValueOnce('boom');
    await store().refreshLinks();
    expect(store().links).toHaveLength(1);
    expect(store().error).toBeNull();
  });

  it('unlink calls the command and reloads the list', async () => {
    mocks.api.listNetworkLinks.mockResolvedValue([]);
    await store().unlink('L1');
    expect(mocks.api.unlinkNetworkLink).toHaveBeenCalledWith('L1');
    expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
  });

  it('unlink reloads the list also when the call fails', async () => {
    mocks.api.unlinkNetworkLink.mockRejectedValue('Could not unlink.');
    await expect(store().unlink('L1')).rejects.toBe('Could not unlink.');
    expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
  });

  it('reset clears the slice and the timers', () => {
    vi.useFakeTimers();
    store().addSyncReview(makeSyncReview());
    store().openSyncDialog('s1');
    store().reset();
    expect(store().syncReviews).toHaveLength(0);
    expect(store().dialogSyncId).toBeNull();
    expect(store().links).toHaveLength(0);
    expect(vi.getTimerCount()).toBe(0);
  });
});
```

- [ ] **Step 13: Run and see it fail**

Run: `bun run test src/stores/__tests__/network-store.sync.test.ts`
Expected: FAIL, `store().addSyncReview is not a function` and the like.

- [ ] **Step 14: Extend `src/stores/network-store.ts`**

Import `listNetworkLinks`, `unlinkNetworkLink`, `type NetworkLink`, `type ConflictView`, `type SyncSummary` from `@/lib/tauri-api`. Add the exported types:

```ts
/** A sync another node started (`review`, this node plans) or one this node started and must confirm (`confirm`). */
export interface SyncReviewEntry {
  syncId: string;
  linkId: string;
  deviceId: string;
  shortId: string;
  displayName: string;
  collectionName: string;
  summary: SyncSummary;
  /** Empty for `confirm`. */
  conflicts: ConflictView[];
  kind: 'review' | 'confirm';
  /** The local user answered. An answered entry no longer expires. */
  answered: boolean;
}

export interface SyncProgress {
  syncId: string;
  doneBytes: number;
  totalBytes: number;
}
```

Add to `NetworkState` and `initialState`: `syncReviews: SyncReviewEntry[]` (`[]`), `syncProgress: SyncProgress[]` (`[]`), `dialogSyncId: string | null` (`null`), `dialogSyncEndedText: string | null` (`null`), `links: NetworkLink[]` (`[]`). Add the actions with these signatures:

```ts
/** Returns false for a repeated or finished sync, so the caller shows nothing twice. */
addSyncReview: (entry: SyncReviewEntry) => boolean;
removeSyncReview: (syncId: string) => void;
markSyncAnswered: (syncId: string) => void;
setSyncProgress: (progress: SyncProgress) => void;
/** Removes the sync everywhere. It returns false when the sync had finished before. */
finishSync: (syncId: string) => boolean;
openSyncDialog: (syncId: string) => void;
closeSyncDialog: () => void;
endSyncDialog: (text: string) => void;
/** A sync started here. Returns false when it had already finished, so no waiting toast is shown. */
trackStartedSync: (syncId: string, info: { displayName: string; collectionName: string }) => boolean;
startedSync: (syncId: string) => { displayName: string; collectionName: string } | undefined;
/** Moves a started sync from waiting to running. It returns the node name the first time, else null. */
advanceStartedSync: (syncId: string) => string | null;
markSyncCancelledHere: (syncId: string) => void;
unmarkSyncCancelledHere: (syncId: string) => void;
wasSyncCancelledHere: (syncId: string) => boolean;
/** Reloads the links. It never rejects and keeps the old list on a failure. */
refreshLinks: () => Promise<void>;
/** Rejects with the backend error and reloads the list either way. */
unlink: (linkId: string) => Promise<void>;
```

Module state next to the transfer state, copying its patterns: `syncTimerKey = (id) => \`sync:${id}\`` into the shared `expiryTimers` map; `finishedSyncs` Set with the same 100 memory (generalise `rememberFinished` into a helper that takes the set, or add `rememberFinishedSync`); `startedSyncs = new Map<string, { displayName: string; collectionName: string; running: boolean }>()`; `cancelledSyncsHere = new Set<string>()`; `let linksSeq = 0`. Behaviour pinned by the tests: `addSyncReview` refuses a finished or already known id and arms a `INCOMING_TTL_MS` timer that calls `removeSyncReview`; `markSyncAnswered` does nothing for an unknown id, clears the timer and sets `answered`; `setSyncProgress` ignores finished ids and replaces or appends by id; `finishSync` clears the timer, deletes the `startedSyncs` and `cancelledSyncsHere` entries and filters `syncReviews` and `syncProgress`; `openSyncDialog` sets `dialogSyncId` and clears `dialogSyncEndedText`; `reset` clears timers, both memories, bumps `linksSeq` and restores `initialState`. `startedSync` returns the `{ displayName, collectionName }` part only (build a new object without `running`).

- [ ] **Step 15: Run the store tests**

Run: `bun run test src/stores/__tests__/network-store.sync.test.ts src/stores/__tests__/network-store.test.ts`
Expected: PASS for both files (the old file is unchanged and must stay green).

### 1d. Bridge

- [ ] **Step 16: Write the failing handler tests**

Create `src/lib/__tests__/network-event-bridge.sync.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { handleNetworkSync } from '@/lib/network-event-bridge';
import { syncResultToastId, syncWaitingToastId } from '@/lib/network-sync-utils';
import type { NetworkSyncEvent } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';
import { makeSyncReview, makeSyncSummary } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { info: vi.fn(), success: vi.fn(), error: vi.fn(), dismiss: vi.fn() },
  api: {
    listNetworkLinks: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

type Finished = Extract<NetworkSyncEvent, { type: 'finished' }>;

const reviewReady = (patch: Record<string, unknown> = {}): NetworkSyncEvent =>
  ({
    type: 'reviewReady',
    sync_id: 's1',
    link_id: 'L1',
    device_id: 'DEV1',
    short_id: 'AAAA-1111',
    display_name: 'Ben Ortiz',
    collection_name: 'Orders',
    summary: makeSyncSummary(),
    conflicts: [],
    role: 'planner',
    ...patch,
  }) as NetworkSyncEvent;

const confirmNeeded = (patch: Record<string, unknown> = {}): NetworkSyncEvent =>
  ({
    type: 'confirmNeeded',
    sync_id: 's1',
    link_id: 'L1',
    device_id: 'DEV1',
    short_id: 'AAAA-1111',
    display_name: 'Ben Ortiz',
    collection_name: 'Orders',
    summary: makeSyncSummary(),
    ...patch,
  }) as NetworkSyncEvent;

const finished = (patch: Partial<Finished> = {}): Finished => ({
  type: 'finished',
  sync_id: 's1',
  link_id: 'L1',
  display_name: 'Ben Ortiz',
  collection_name: 'Orders',
  outcome: 'applied',
  message: null,
  ...patch,
});

describe('handleNetworkSync', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.listNetworkLinks.mockResolvedValue([]);
  });

  describe('reviews', () => {
    it('adds a review from the planner event', () => {
      handleNetworkSync(
        reviewReady({
          conflicts: [{ path: 'a.yml', kind: 'both_changed', mineHash: 'h1', theirsHash: 'h2' }],
        }),
      );
      const [entry] = store().syncReviews;
      expect(entry).toMatchObject({
        syncId: 's1',
        linkId: 'L1',
        deviceId: 'DEV1',
        shortId: 'AAAA-1111',
        displayName: 'Ben Ortiz',
        collectionName: 'Orders',
        kind: 'review',
        answered: false,
      });
      expect(entry.conflicts).toHaveLength(1);
    });

    it('adds one row for a repeated review', () => {
      handleNetworkSync(reviewReady());
      handleNetworkSync(reviewReady());
      expect(store().syncReviews).toHaveLength(1);
    });

    it('does not open a planner review by itself', () => {
      handleNetworkSync(reviewReady());
      expect(store().dialogSyncId).toBeNull();
    });

    it('treats a review for the starter role as a confirm', () => {
      handleNetworkSync(reviewReady({ role: 'starter' }));
      expect(store().syncReviews[0].kind).toBe('confirm');
    });

    it('opens the confirm dialog when no sync dialog is open', () => {
      handleNetworkSync(confirmNeeded());
      expect(store().syncReviews[0]).toMatchObject({ kind: 'confirm', conflicts: [] });
      expect(store().dialogSyncId).toBe('s1');
    });

    it('does not take over another open sync dialog', () => {
      handleNetworkSync(reviewReady({ sync_id: 'other' }));
      store().openSyncDialog('other');
      handleNetworkSync(confirmNeeded());
      expect(store().dialogSyncId).toBe('other');
      expect(store().syncReviews).toHaveLength(2);
    });

    it('keeps a double quote in the collection name', () => {
      handleNetworkSync(reviewReady({ collection_name: 'Orders "v2"' }));
      expect(store().syncReviews[0].collectionName).toBe('Orders "v2"');
    });
  });

  describe('progress and links', () => {
    it('stores progress', () => {
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 5, total_bytes: 10 });
      expect(store().syncProgress).toEqual([{ syncId: 's1', doneBytes: 5, totalBytes: 10 }]);
    });

    it('switches the waiting toast of a started sync to running once', () => {
      store().trackStartedSync('s1', { displayName: 'Ben Ortiz', collectionName: 'Orders' });
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 1, total_bytes: 10 });
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 2, total_bytes: 10 });
      expect(mocks.toast.info).toHaveBeenCalledTimes(1);
      expect(mocks.toast.info).toHaveBeenCalledWith('Syncing with Ben Ortiz...', {
        id: syncWaitingToastId('s1'),
        duration: Number.POSITIVE_INFINITY,
      });
    });

    it('leaves the toasts alone for a sync it did not start', () => {
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 1, total_bytes: 10 });
      expect(mocks.toast.info).not.toHaveBeenCalled();
    });

    it('reloads the links on linkChanged', () => {
      handleNetworkSync({ type: 'linkChanged', link_id: 'L1' });
      expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
    });
  });

  describe('late events', () => {
    it('ignores progress and reviews that arrive after the finish', () => {
      handleNetworkSync(finished());
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 1, total_bytes: 2 });
      handleNetworkSync(reviewReady());
      expect(store().syncProgress).toHaveLength(0);
      expect(store().syncReviews).toHaveLength(0);
    });

    it('a finish for another sync leaves the open dialog alone', () => {
      handleNetworkSync(reviewReady({ sync_id: 's1' }));
      handleNetworkSync(reviewReady({ sync_id: 's2' }));
      store().openSyncDialog('s1');
      handleNetworkSync(finished({ sync_id: 's2' }));
      expect(store().dialogSyncId).toBe('s1');
      expect(store().syncReviews.map((r) => r.syncId)).toEqual(['s1']);
    });

    it('closes the dialog of the sync that finished', () => {
      handleNetworkSync(reviewReady());
      store().openSyncDialog('s1');
      handleNetworkSync(finished());
      expect(store().dialogSyncId).toBeNull();
    });

    it('shows the result once, also when the finish event repeats', () => {
      handleNetworkSync(finished());
      handleNetworkSync(finished());
      expect(mocks.toast.success).toHaveBeenCalledTimes(1);
    });

    it('dismisses the waiting toast on the first finish', () => {
      handleNetworkSync(finished({ outcome: 'declined' }));
      expect(mocks.toast.dismiss).toHaveBeenCalledWith(syncWaitingToastId('s1'));
    });
  });

  describe('results', () => {
    it('announces applied', () => {
      handleNetworkSync(finished({ collection_name: 'Orders "v2"' }));
      expect(mocks.toast.success).toHaveBeenCalledWith('Synced "Orders "v2"" with Ben Ortiz.', {
        id: syncResultToastId('s1'),
      });
    });

    it('names a node without a display name', () => {
      handleNetworkSync(finished({ display_name: '  ' }));
      expect(mocks.toast.success).toHaveBeenCalledWith('Synced "Orders" with Unnamed node.', {
        id: syncResultToastId('s1'),
      });
    });

    it('announces declined to the starter only', () => {
      store().trackStartedSync('s1', { displayName: 'Ben Ortiz', collectionName: 'Orders' });
      handleNetworkSync(finished({ outcome: 'declined' }));
      expect(mocks.toast.info).toHaveBeenCalledWith('Ben Ortiz declined the sync.', {
        id: syncResultToastId('s1'),
      });
      handleNetworkSync(finished({ sync_id: 's2', outcome: 'declined' }));
      expect(mocks.toast.info).toHaveBeenCalledTimes(1);
    });

    it('announces expired', () => {
      handleNetworkSync(finished({ outcome: 'expired' }));
      expect(mocks.toast.info).toHaveBeenCalledWith('The request expired.', {
        id: syncResultToastId('s1'),
      });
    });

    it('announces a cancel by the other node', () => {
      handleNetworkSync(finished({ outcome: 'cancelled' }));
      expect(mocks.toast.info).toHaveBeenCalledWith('Ben Ortiz cancelled the sync.', {
        id: syncResultToastId('s1'),
      });
    });

    it('says nothing when the local user cancelled', () => {
      store().markSyncCancelledHere('s1');
      handleNetworkSync(finished({ outcome: 'cancelled' }));
      expect(mocks.toast.info).not.toHaveBeenCalled();
    });

    it('announces a base mismatch', () => {
      handleNetworkSync(finished({ outcome: 'base_mismatch' }));
      expect(mocks.toast.error).toHaveBeenCalledWith(
        'The two copies disagree about the last agreed version. Unlink and send the collection again.',
        { id: syncResultToastId('s1') },
      );
    });

    it('announces a missing link', () => {
      handleNetworkSync(finished({ outcome: 'link_missing' }));
      expect(mocks.toast.error).toHaveBeenCalledWith(
        'The other node no longer has this link. Unlink and send the collection again.',
        { id: syncResultToastId('s1') },
      );
    });

    it('announces unconfirmed', () => {
      handleNetworkSync(finished({ outcome: 'unconfirmed' }));
      expect(mocks.toast.info).toHaveBeenCalledWith(
        'Synced here, but the other node did not confirm.',
        { id: syncResultToastId('s1') },
      );
    });

    it('uses the backend message on failed, else a fixed text', () => {
      handleNetworkSync(finished({ outcome: 'failed', message: 'A file was not safe to apply.' }));
      expect(mocks.toast.error).toHaveBeenLastCalledWith('A file was not safe to apply.', {
        id: syncResultToastId('s1'),
      });
      handleNetworkSync(finished({ sync_id: 's2', outcome: 'failed', message: null }));
      expect(mocks.toast.error).toHaveBeenLastCalledWith('Sync failed. Try again.', {
        id: syncResultToastId('s2'),
      });
      handleNetworkSync(finished({ sync_id: 's3', outcome: 'failed', message: '   ' }));
      expect(mocks.toast.error).toHaveBeenLastCalledWith('Sync failed. Try again.', {
        id: syncResultToastId('s3'),
      });
    });

    it('says nothing for an outcome added later', () => {
      handleNetworkSync(finished({ outcome: 'future' as never }));
      expect(mocks.toast.info).not.toHaveBeenCalled();
      expect(mocks.toast.error).not.toHaveBeenCalled();
      expect(mocks.toast.success).not.toHaveBeenCalled();
    });

    it('reloads the links after every finish', () => {
      handleNetworkSync(finished());
      expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
    });
  });

  describe('a dialog that is open while the sync ends', () => {
    it('keeps an unanswered dialog open and says why', () => {
      handleNetworkSync(reviewReady());
      store().openSyncDialog('s1');
      handleNetworkSync(
        finished({ outcome: 'failed', message: 'The other node went offline.' }),
      );
      expect(store().dialogSyncId).toBe('s1');
      expect(store().dialogSyncEndedText).toBe('The other node went offline.');
      expect(store().syncReviews).toHaveLength(0);
    });

    it('keeps the dialog open on an expired finish', () => {
      handleNetworkSync(reviewReady());
      store().openSyncDialog('s1');
      handleNetworkSync(finished({ outcome: 'expired' }));
      expect(store().dialogSyncId).toBe('s1');
    });

    it('closes an answered dialog on a failure', () => {
      handleNetworkSync(reviewReady());
      store().markSyncAnswered('s1');
      store().openSyncDialog('s1');
      handleNetworkSync(finished({ outcome: 'failed', message: 'Something broke.' }));
      expect(store().dialogSyncId).toBeNull();
    });
  });
});
```

(`makeSyncReview` is imported for later reuse; remove the import if unused so `bun run check` stays clean.)

- [ ] **Step 17: Run and see it fail**

Run: `bun run test src/lib/__tests__/network-event-bridge.sync.test.ts`
Expected: FAIL, `handleNetworkSync` is not exported.

- [ ] **Step 18: Extend `src/lib/network-event-bridge.ts`**

Add the imports (`syncResultToastId`, `syncWaitingToastId` from `@/lib/network-sync-utils`; `NetworkSyncEvent`, `onNetworkSync` from `@/lib/tauri-api`). Add and export `handleNetworkSync(event: NetworkSyncEvent): void`:

```ts
// Fixed texts. A missing row means the user already knows.
function describeSyncOutcome(
  event: FinishedSync,
  startedHere: boolean,
  cancelledHere: boolean,
): TransferMessage | null;
```

Reuse the `TransferMessage` type. Texts, exactly as the tests pin them: `applied` success `Synced "<collection>" with <name>.`; `declined` info `<name> declined the sync.` only when `startedHere`, else null; `expired` info `The request expired.`; `cancelled` info `<name> cancelled the sync.` unless `cancelledHere`; `failed` error `event.message?.trim() || 'Sync failed. Try again.'`; `base_mismatch` error `The two copies disagree about the last agreed version. Unlink and send the collection again.`; `link_missing` error `The other node no longer has this link. Unlink and send the collection again.`; `unconfirmed` info `Synced here, but the other node did not confirm.`; `default` null. `<name>` is `peerName(event.display_name)`.

Handler behaviour (the store reads happen before `finishSync`, which forgets them):

- `reviewReady`: `addSyncReview` with `kind: event.role === 'starter' ? 'confirm' : 'review'`, `answered: false`.
- `confirmNeeded`: `const added = addSyncReview({... conflicts: [], kind: 'confirm'})`; if `added && store.dialogSyncId === null` call `openSyncDialog(id)`.
- `progress`: `setSyncProgress`, then `const name = store.advanceStartedSync(id)`; when it is not null `toast.info(\`Syncing with ${peerName(name)}...\`, { id: syncWaitingToastId(id), duration: Number.POSITIVE_INFINITY })`. The action is not passed: sonner merges options for a reused id (send plan ruling 9).
- `finished`: read `startedHere = store.startedSync(id) !== undefined`, `cancelledHere = store.wasSyncCancelledHere(id)`, `wasPending = store.syncReviews.some(r => r.syncId === id && !r.answered)`; `if (!store.finishSync(id)) return;` build the message; `keepDialog = event.outcome === 'expired' || (wasPending && event.outcome === 'failed' && Boolean(event.message?.trim()))`; when `store.dialogSyncId === id`: close unless `keepDialog`, and when `failed` with a message call `endSyncDialog(message.text)`; `toast.dismiss(syncWaitingToastId(id))`; `if (message) toast[message.kind](message.text, { id: syncResultToastId(id) })`; `void store.refreshLinks()`.
- `linkChanged`: `void store.refreshLinks()`.

In `useNetworkEventBridge` add a fourth `scope.keep(onNetworkSync(...))` guarded by `scope.isDisposed()`, and `void useNetworkStore.getState().refreshLinks();` next to the other two startup loads.

- [ ] **Step 19: Update the existing bridge test and add the lifecycle tests**

In `src/lib/__tests__/network-event-bridge.test.ts`: in the hoisted `api` object add `onNetworkSync: listener('sync'),` and `listNetworkLinks: vi.fn(),`; in the `beforeEach` that sets the status and node mocks add `mocks.api.listNetworkLinks.mockResolvedValue([]);`. Then find every other test file that mounts the bridge or mocks the same names: run `grep -rln "onNetworkTransfer" src --include='*.test.ts*'` and add `onNetworkSync: vi.fn().mockResolvedValue(() => undefined)` to each tauri-api mock that is missing it. Add two tests next to the existing "ignores transfer events after unmount" tests (copy their shape):

```ts
it('ignores sync events after unmount', async () => {
  const view = await mountBridge();
  view.unmount();
  mocks.handlers.sync({
    type: 'progress',
    sync_id: 's1',
    done_bytes: 1,
    total_bytes: 2,
  });
  expect(store().syncProgress).toHaveLength(0);
});

it('stops a sync listener that finishes registering after unmount', async () => {
  const late = createDeferred<() => void>();
  mocks.api.onNetworkSync.mockReturnValueOnce(late.promise);
  const view = renderHook(() => useNetworkEventBridge());
  view.unmount();
  const stop = vi.fn();
  late.resolve(stop);
  await waitFor(() => expect(stop).toHaveBeenCalled());
});
```

If the existing file builds the second test differently (look at the existing "stops a transfer listener that finishes registering after unmount"), mirror that test exactly with `onNetworkSync` in place of `onNetworkTransfer`.

- [ ] **Step 20: Run the bridge tests**

Run: `bun run test src/lib/__tests__/network-event-bridge.sync.test.ts src/lib/__tests__/network-event-bridge.test.ts`
Expected: PASS for both files.

### 1e. Starting a sync

- [ ] **Step 21: Write the failing action tests**

Create `src/lib/__tests__/network-sync-actions.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { startCollectionSync } from '@/lib/network-sync-actions';
import { syncWaitingToastId } from '@/lib/network-sync-utils';
import { useNetworkStore } from '@/stores/network-store';
import { makeLink } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { info: vi.fn(), error: vi.fn(), success: vi.fn(), dismiss: vi.fn() },
  api: {
    startNetworkSync: vi.fn(),
    cancelNetworkSync: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

const waitingOptions = () =>
  mocks.toast.info.mock.calls[0]?.[1] as {
    id: string;
    duration: number;
    action: { label: string; onClick: () => void };
  };

describe('startCollectionSync', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.startNetworkSync.mockResolvedValue({ syncId: 's1' });
    mocks.api.cancelNetworkSync.mockResolvedValue(undefined);
  });

  it('starts the sync and shows a waiting toast with Cancel', async () => {
    await startCollectionSync(makeLink());
    expect(mocks.api.startNetworkSync).toHaveBeenCalledWith('L1');
    expect(mocks.toast.info).toHaveBeenCalledWith(
      'Waiting for Ben Ortiz to review the sync.',
      expect.objectContaining({ id: syncWaitingToastId('s1'), duration: Number.POSITIVE_INFINITY }),
    );
    expect(waitingOptions().action.label).toBe('Cancel');
    expect(store().startedSync('s1')).toEqual({
      displayName: 'Ben Ortiz',
      collectionName: 'Orders',
    });
  });

  it('shows no waiting toast when the sync finished before the call returned', async () => {
    store().finishSync('s1');
    await startCollectionSync(makeLink());
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it('Cancel cancels that sync and remembers it was local', async () => {
    await startCollectionSync(makeLink());
    waitingOptions().action.onClick();
    expect(mocks.api.cancelNetworkSync).toHaveBeenCalledWith('s1');
    expect(store().wasSyncCancelledHere('s1')).toBe(true);
  });

  it('shows the error and forgets the local cancel when the cancel is refused', async () => {
    mocks.api.cancelNetworkSync.mockRejectedValue('Already syncing.');
    await startCollectionSync(makeLink());
    waitingOptions().action.onClick();
    await vi.waitFor(() => expect(mocks.toast.error).toHaveBeenCalled());
    expect(store().wasSyncCancelledHere('s1')).toBe(false);
  });

  it('rejects with the backend error and shows no toast', async () => {
    mocks.api.startNetworkSync.mockRejectedValue('Another sync is already running for this link.');
    await expect(startCollectionSync(makeLink())).rejects.toBe(
      'Another sync is already running for this link.',
    );
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 22: Run and see it fail**

Run: `bun run test src/lib/__tests__/network-sync-actions.test.ts`
Expected: FAIL, the module does not exist.

- [ ] **Step 23: Create `src/lib/network-sync-actions.ts`**

```ts
import { toast } from 'sonner';
import { toastError } from '@/lib/error-text';
import { peerName } from '@/lib/network-utils';
import { syncWaitingToastId } from '@/lib/network-sync-utils';
import { cancelNetworkSync, type NetworkLink, startNetworkSync } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

/**
 * Starts a sync for one link. It rejects with the backend error, so the caller decides how to show
 * it. On success the waiting toast stays until the bridge dismisses it on the first finish.
 */
export async function startCollectionSync(link: NetworkLink): Promise<void>;
```

Body: `const { syncId } = await startNetworkSync(link.linkId)`; `trackStartedSync(syncId, { displayName: link.displayName, collectionName: link.collectionName })`; if it returned false return; else `toast.info(\`Waiting for ${peerName(link.displayName)} to review the sync.\`, { id: syncWaitingToastId(syncId), duration: Number.POSITIVE_INFINITY, action: { label: 'Cancel', onClick } })`. `onClick` calls `markSyncCancelledHere(syncId)` then `cancelNetworkSync(syncId).catch((err) => { unmarkSyncCancelledHere(syncId); toastError(err); })`, the same shape as `showWaitingToast` in `SendCollectionDialog.tsx`.

- [ ] **Step 24: Run the task checks**

Run: `bun run test src/lib/__tests__/network-sync-actions.test.ts src/lib/__tests__/network-sync-utils.test.ts src/lib/__tests__/tauri-api.network.test.ts src/stores/__tests__ src/lib/__tests__/network-event-bridge.test.ts src/lib/__tests__/network-event-bridge.sync.test.ts`
Expected: PASS for all listed files.
Run: `bun run tsc --noEmit`
Expected: errors only in `SendCollectionDialog.tsx` and the two test files from Step 6 (Task 2 fixes them). No other error.
Run: `bun run check`
Expected: no findings in the new and changed files (if it reports format or import order, run `bun run lint`, then `bun run check` again).

---

## Task 2: Dialogs, conflict diff, toast and menu items

**Files:**
- Modify: `src/components/network/SendCollectionDialog.tsx`, `src/components/network/__tests__/SendCollectionDialog.test.tsx`, `src/components/collections/CollectionNode.tsx`, `src/App.tsx`
- Create: `src/components/network/SyncConflictDiff.tsx`, `src/components/network/SyncReviewDialog.tsx`, `src/components/network/IncomingSyncToast.tsx`, `src/components/network/UnlinkDialog.tsx`, `src/hooks/useIncomingSyncNotice.tsx`, `src/hooks/useCollectionSyncLinks.ts`, `src/components/collections/CollectionSyncMenuItems.tsx`, and the test files `SyncConflictDiff.test.tsx`, `SyncReviewDialog.test.tsx`, `IncomingSyncToast.test.tsx`, `UnlinkDialog.test.tsx`, `useIncomingSyncNotice.test.tsx`, `CollectionNode.sync.test.tsx`

**Interfaces:**
- Consumes: everything Task 1 produces; `DiffViewer` (`diffState`, `hideStageToggle`); `useAsyncAction`, `StateBlock`, `FormField`, `Checkbox`, `RadioGroup`, `AlertDialog`; the existing `selectOnlinePairedCount`.
- Produces: `SendCollectionDialog` with a `keepInSync` box; `SyncConflictDiff`, `SyncReviewDialog`, `IncomingSyncToast`, `incomingSyncToastId`, `useIncomingSyncNotice`, `UnlinkDialog`, `useCollectionSyncLinks`, `CollectionSyncMenuItems` (`DropdownSyncItems`, `ContextSyncItems`).

### 2a. Keep in sync in the Send dialog

- [ ] **Step 1: Write the failing dialog tests**

In `src/components/network/__tests__/SendCollectionDialog.test.tsx`, make sure the three existing assertions already carry the trailing `false` from Task 1 Step 6. Add inside the main `describe` (use the file's own `setup`, `userEvent` and `waitFor` imports as the neighbouring tests do):

```tsx
it('sends keepInSync false by default', async () => {
  mocks.api.sendNetworkCollection.mockResolvedValue(started());
  setup();
  await userEvent.click(screen.getByRole('button', { name: 'Send' }));
  await waitFor(() =>
    expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'Orders', false, false),
  );
});

it('sends keepInSync true when ticked', async () => {
  mocks.api.sendNetworkCollection.mockResolvedValue(started());
  setup();
  await userEvent.click(screen.getByRole('checkbox', { name: 'Keep in sync' }));
  await userEvent.click(screen.getByRole('button', { name: 'Send' }));
  await waitFor(() =>
    expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'Orders', false, true),
  );
});

it('keeps the two boxes independent', async () => {
  mocks.api.sendNetworkCollection.mockResolvedValue(started());
  setup();
  await userEvent.click(screen.getByRole('checkbox', { name: 'Include environments' }));
  await userEvent.click(screen.getByRole('button', { name: 'Send' }));
  await waitFor(() =>
    expect(mocks.api.sendNetworkCollection).toHaveBeenCalledWith('DEV1', 'Orders', true, false),
  );
});

it('says that only a collection sent now can be linked', () => {
  setup();
  expect(screen.getByText(/Only a collection you send now can be linked/)).toBeInTheDocument();
});

it('resets the box when the dialog closes', async () => {
  const { view, onClose } = setup();
  await userEvent.click(screen.getByRole('checkbox', { name: 'Keep in sync' }));
  view.rerender(<SendCollectionDialog open={false} collectionName='Orders' onClose={onClose} />);
  view.rerender(<SendCollectionDialog open collectionName='Orders' onClose={onClose} />);
  expect(screen.getByRole('checkbox', { name: 'Keep in sync' })).not.toBeChecked();
});
```

- [ ] **Step 2: Run and see it fail**

Run: `bun run test src/components/network/__tests__/SendCollectionDialog.test.tsx`
Expected: FAIL (no checkbox named "Keep in sync"; the old assertions now expect four arguments).

- [ ] **Step 3: Edit `SendCollectionDialog.tsx`**

Add `const [keepInSync, setKeepInSync] = useState(false);`, reset it in the existing `useEffect` that resets on close, call `sendNetworkCollection(chosen.deviceId, collectionName, includeEnvironments, keepInSync)`, and add a second group after the Environments `FormField`:

```tsx
<FormField
  id='send-sync'
  group
  label='Sync'
  help='Off by default. Lets you and the other user sync later changes, each one reviewed. Only a collection you send now can be linked.'
>
  {(a11y) => (
    <div className='flex items-center gap-2'>
      <Checkbox
        id='send-sync-box'
        aria-describedby={a11y['aria-describedby']}
        checked={keepInSync}
        disabled={busy}
        onCheckedChange={(v) => setKeepInSync(v === true)}
      />
      <Label htmlFor='send-sync-box' className='text-sm font-normal'>
        Keep in sync
      </Label>
    </div>
  )}
</FormField>
```

- [ ] **Step 4: Run the dialog tests**

Run: `bun run test src/components/network/__tests__/SendCollectionDialog.test.tsx src/components/collections/__tests__/CollectionNode.send.test.tsx`
Expected: PASS for both (the Task 1 Step 6 edits turn green here).

### 2b. The conflict diff

- [ ] **Step 5: Write the failing diff tests**

Create `src/components/network/__tests__/SyncConflictDiff.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SyncConflictDiff } from '@/components/network/SyncConflictDiff';
import { createDeferred } from '@/test/deferred';

const mocks = vi.hoisted(() => ({
  api: { getNetworkSyncConflictFiles: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('@/components/git/DiffViewer', () => ({
  DiffViewer: (props: {
    diffState: { filePath: string; oldContent: string; newContent: string; status: string };
    hideStageToggle?: boolean;
  }) => (
    <div
      data-testid='diff'
      data-path={props.diffState.filePath}
      data-status={props.diffState.status}
      data-hide-stage={String(props.hideStageToggle)}
    >
      <pre data-testid='diff-old'>{props.diffState.oldContent}</pre>
      <pre data-testid='diff-new'>{props.diffState.newContent}</pre>
    </div>
  ),
}));

const conflict = { path: 'a.yml', kind: 'both_changed' as const, mineHash: 'h1', theirsHash: 'h2' };

describe('SyncConflictDiff', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows mine on the left and theirs on the right through the git diff viewer', async () => {
    mocks.api.getNetworkSyncConflictFiles.mockResolvedValue({ mine: 'url: a', theirs: 'url: b' });
    render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben Ortiz' />);
    expect(await screen.findByTestId('diff')).toHaveAttribute('data-path', 'a.yml');
    expect(screen.getByTestId('diff-old')).toHaveTextContent('url: a');
    expect(screen.getByTestId('diff-new')).toHaveTextContent('url: b');
    expect(mocks.api.getNetworkSyncConflictFiles).toHaveBeenCalledWith('s1', 'a.yml');
    expect(screen.getByText('Left: your copy. Right: the copy of Ben Ortiz.')).toBeInTheDocument();
  });

  it('hides the stage toggle', async () => {
    mocks.api.getNetworkSyncConflictFiles.mockResolvedValue({ mine: 'a', theirs: 'b' });
    render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben Ortiz' />);
    expect(await screen.findByTestId('diff')).toHaveAttribute('data-hide-stage', 'true');
  });

  it('shows an added file against an empty left side', async () => {
    mocks.api.getNetworkSyncConflictFiles.mockResolvedValue({ mine: null, theirs: 'new' });
    render(
      <SyncConflictDiff
        syncId='s1'
        conflict={{ ...conflict, kind: 'delete_vs_edit', mineHash: null }}
        peerName='Ben Ortiz'
      />,
    );
    expect(await screen.findByTestId('diff-old')).toHaveTextContent('');
    expect(screen.getByTestId('diff-new')).toHaveTextContent('new');
  });

  it('shows a loading state first', async () => {
    const files = createDeferred<{ mine: string; theirs: string }>();
    mocks.api.getNetworkSyncConflictFiles.mockReturnValue(files.promise);
    render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben Ortiz' />);
    expect(screen.getByText('Loading the files...')).toBeInTheDocument();
    files.resolve({ mine: 'a', theirs: 'b' });
    expect(await screen.findByTestId('diff')).toBeInTheDocument();
  });

  it('shows the error in a state block when the files cannot be loaded', async () => {
    mocks.api.getNetworkSyncConflictFiles.mockRejectedValue('unavailable');
    render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben Ortiz' />);
    const block = await screen.findByText('The files could not be loaded. You can still choose.');
    expect(block.closest('[data-slot="state-block"]')).toHaveAttribute('data-tone', 'error');
    expect(screen.queryByTestId('diff')).toBeNull();
  });

  it('does not show the result of a request for another conflict', async () => {
    const first = createDeferred<{ mine: string; theirs: string }>();
    mocks.api.getNetworkSyncConflictFiles
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce({ mine: 'second', theirs: 'second' });
    const view = render(<SyncConflictDiff syncId='s1' conflict={conflict} peerName='Ben' />);
    view.rerender(
      <SyncConflictDiff syncId='s1' conflict={{ ...conflict, path: 'b.yml' }} peerName='Ben' />,
    );
    expect(await screen.findByTestId('diff-new')).toHaveTextContent('second');
    first.resolve({ mine: 'first', theirs: 'first' });
    await Promise.resolve();
    expect(screen.getByTestId('diff-new')).toHaveTextContent('second');
  });
});
```

- [ ] **Step 6: Run and see it fail**

Run: `bun run test src/components/network/__tests__/SyncConflictDiff.test.tsx`
Expected: FAIL, the module does not exist.

- [ ] **Step 7: Create `src/components/network/SyncConflictDiff.tsx`**

```tsx
interface SyncConflictDiffProps {
  syncId: string;
  conflict: ConflictView;
  /** The display name of the other node, already passed through `peerName`. */
  peerName: string;
}

/**
 * Loads both versions of one conflicting file and shows them in the read-only git diff viewer,
 * mine on the left, theirs on the right. It changes nothing: the three choices live in the dialog.
 */
export function SyncConflictDiff({ syncId, conflict, peerName }: SyncConflictDiffProps);
```

Behaviour: a state `{ status: 'loading' } | { status: 'ready'; files: ConflictFiles } | { status: 'error' }`. An effect keyed on `syncId` and `conflict.path` calls `getNetworkSyncConflictFiles` with a `cancelled` flag in its cleanup, so a late answer for another conflict is dropped (test above). Loading renders `StateBlock tone='loading'` "Loading the files...". Error renders `StateBlock tone='error'` "The files could not be loaded. You can still choose." (never the rejection text: it may name a path or a file). Ready renders a caption `<p className='text-2xs text-muted-foreground'>Left: your copy. Right: the copy of {peerName}.</p>` and, in a `div className='h-72 overflow-hidden rounded-md border'`, `<DiffViewer key={\`${syncId}:${conflict.path}\`} hideStageToggle diffState={{ filePath: conflict.path, repositoryId: '', repositoryLabel: 'Sync', oldContent: files.mine ?? '', newContent: files.theirs ?? '', status: 'modified', isStaged: false }} />`. The `key` is needed because `DiffViewer` copies its prop into state once.

- [ ] **Step 8: Run the diff tests**

Run: `bun run test src/components/network/__tests__/SyncConflictDiff.test.tsx`
Expected: PASS, 6 tests.

### 2c. The review and confirm dialog

- [ ] **Step 9: Write the failing dialog tests**

Create `src/components/network/__tests__/SyncReviewDialog.test.tsx`:

```tsx
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SyncReviewDialog } from '@/components/network/SyncReviewDialog';
import { handleNetworkSync } from '@/lib/network-event-bridge';
import type { ConflictView } from '@/lib/tauri-api';
import { type SyncReviewEntry, useNetworkStore } from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeSyncReview, makeSyncSummary } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn(), dismiss: vi.fn() },
  api: {
    reviewNetworkSync: vi.fn(),
    confirmNetworkSync: vi.fn(),
    cancelNetworkSync: vi.fn(),
    getNetworkSyncConflictFiles: vi.fn(),
    listNetworkLinks: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));
vi.mock('@/components/git/DiffViewer', () => ({
  DiffViewer: (props: { diffState: { filePath: string; oldContent: string; newContent: string } }) => (
    <div data-testid='diff' data-path={props.diffState.filePath}>
      <pre data-testid='diff-old'>{props.diffState.oldContent}</pre>
      <pre data-testid='diff-new'>{props.diffState.newContent}</pre>
    </div>
  ),
}));

const store = () => useNetworkStore.getState();

const both: ConflictView = { path: 'a.yml', kind: 'both_changed', mineHash: 'h1', theirsHash: 'h2' };
const gone: ConflictView = {
  path: 'b.yml',
  kind: 'delete_vs_edit',
  mineHash: null,
  theirsHash: 'h3',
};

function openEntry(patch: Partial<SyncReviewEntry> = {}) {
  const entry = makeSyncReview(patch);
  store().addSyncReview(entry);
  useNetworkStore.setState({ dialogSyncId: entry.syncId });
  return render(<SyncReviewDialog />);
}

const group = (path: string) => screen.getByRole('radiogroup', { name: `Resolution for ${path}` });
const choose = (path: string, label: string) =>
  userEvent.click(within(group(path)).getByRole('radio', { name: label }));

describe('SyncReviewDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.reviewNetworkSync.mockResolvedValue(undefined);
    mocks.api.confirmNetworkSync.mockResolvedValue(undefined);
    mocks.api.cancelNetworkSync.mockResolvedValue(undefined);
    mocks.api.listNetworkLinks.mockResolvedValue([]);
    mocks.api.getNetworkSyncConflictFiles.mockResolvedValue({ mine: 'mine text', theirs: 'their text' });
  });

  it('renders nothing without an open sync', () => {
    const { container } = render(<SyncReviewDialog />);
    expect(container).toBeEmptyDOMElement();
  });

  describe('review', () => {
    it('shows who, what and the counts on both sides', () => {
      openEntry({
        summary: makeSyncSummary({
          addedHere: 1,
          changedHere: 2,
          deletedHere: 0,
          sentAdded: 0,
          sentChanged: 3,
          sentDeleted: 1,
        }),
      });
      expect(
        screen.getByRole('dialog', { name: 'Ben Ortiz wants to sync a collection' }),
      ).toBeInTheDocument();
      expect(screen.getByText('Orders')).toBeInTheDocument();
      expect(screen.getByText('Device ID AAAA-1111')).toBeInTheDocument();
      expect(screen.getByText('Coming to you: 1 added, 2 changed')).toBeInTheDocument();
      expect(screen.getByText('Going to Ben Ortiz: 3 changed, 1 deleted')).toBeInTheDocument();
    });

    it('shows a collection name with a double quote as given', () => {
      openEntry({ collectionName: 'Orders "v2"' });
      expect(screen.getByText('Orders "v2"')).toBeInTheDocument();
    });

    it('names a node without a display name', () => {
      openEntry({ displayName: '  ' });
      expect(
        screen.getByRole('dialog', { name: 'Unnamed node wants to sync a collection' }),
      ).toBeInTheDocument();
    });

    it('marks the changed scripts and auth files by path', () => {
      openEntry({
        summary: makeSyncSummary({
          scriptsChanged: ['Auth/login.yml'],
          authChanged: ['Auth/login.yml', 'Billing/pay.yml'],
        }),
      });
      expect(screen.getByText('Scripts changed (1)')).toBeInTheDocument();
      expect(screen.getByText('Auth changed (2)')).toBeInTheDocument();
      expect(screen.getAllByText('Auth/login.yml')).toHaveLength(2);
      expect(screen.getByText('Billing/pay.yml')).toBeInTheDocument();
    });

    it('shows no mark section when nothing changed in scripts or auth', () => {
      openEntry();
      expect(screen.queryByText(/Scripts changed/)).toBeNull();
      expect(screen.queryByText(/Auth changed/)).toBeNull();
    });

    it('warns that changed scripts run in the Safe sandbox', () => {
      openEntry();
      expect(
        screen.getByText(
          'Changed scripts run in the Safe sandbox. Developer mode, host environment and vault access stay off until you allow them.',
        ),
      ).toBeInTheDocument();
    });

    it('accepts at once when there is no conflict and sends no decision', async () => {
      openEntry();
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      await waitFor(() => expect(mocks.api.reviewNetworkSync).toHaveBeenCalledWith('s1', true, []));
    });

    it('keeps Accept off until every conflict has a choice', async () => {
      openEntry({ conflicts: [both, gone] });
      const accept = screen.getByRole('button', { name: 'Accept' });
      expect(accept).toBeDisabled();
      await choose('a.yml', 'Keep mine');
      expect(accept).toBeDisabled();
      await choose('b.yml', 'Keep theirs');
      expect(accept).toBeEnabled();
    });

    it('starts every conflict with no choice', () => {
      openEntry({ conflicts: [both] });
      for (const radio of within(group('a.yml')).getAllByRole('radio')) {
        expect(radio).not.toBeChecked();
      }
    });

    it('sends one decision per conflict with the wire resolution', async () => {
      openEntry({ conflicts: [both, gone] });
      await choose('a.yml', 'Keep both');
      await choose('b.yml', 'Keep theirs');
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      await waitFor(() =>
        expect(mocks.api.reviewNetworkSync).toHaveBeenCalledWith('s1', true, [
          { path: 'a.yml', resolution: 'keep_both' },
          { path: 'b.yml', resolution: 'keep_theirs' },
        ]),
      );
    });

    it('lets the user change a choice', async () => {
      openEntry({ conflicts: [both] });
      await choose('a.yml', 'Keep mine');
      await choose('a.yml', 'Keep theirs');
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      await waitFor(() =>
        expect(mocks.api.reviewNetworkSync).toHaveBeenCalledWith('s1', true, [
          { path: 'a.yml', resolution: 'keep_theirs' },
        ]),
      );
    });

    it('hides keep both when one side has no file', () => {
      openEntry({ conflicts: [gone] });
      expect(within(group('b.yml')).queryByRole('radio', { name: 'Keep both' })).toBeNull();
      expect(within(group('b.yml')).getAllByRole('radio')).toHaveLength(2);
    });

    it('lists each conflict with its kind', () => {
      openEntry({ conflicts: [both, gone] });
      expect(screen.getByText('Conflicts (2)')).toBeInTheDocument();
      expect(screen.getByText('Changed on both nodes')).toBeInTheDocument();
      expect(screen.getByText('Deleted on one node, changed on the other')).toBeInTheDocument();
    });

    it('opens the diff for one conflict and closes it again', async () => {
      openEntry({ conflicts: [both] });
      expect(screen.queryByTestId('diff')).toBeNull();
      await userEvent.click(screen.getByRole('button', { name: 'Compare a.yml' }));
      expect(await screen.findByTestId('diff-old')).toHaveTextContent('mine text');
      expect(screen.getByTestId('diff-new')).toHaveTextContent('their text');
      await userEvent.click(screen.getByRole('button', { name: 'Hide a.yml' }));
      expect(screen.queryByTestId('diff')).toBeNull();
    });

    it('still lets the user choose when the files cannot be loaded', async () => {
      mocks.api.getNetworkSyncConflictFiles.mockRejectedValue('unavailable');
      openEntry({ conflicts: [both] });
      await userEvent.click(screen.getByRole('button', { name: 'Compare a.yml' }));
      expect(
        await screen.findByText('The files could not be loaded. You can still choose.'),
      ).toBeInTheDocument();
      await choose('a.yml', 'Keep mine');
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
    });

    it('declines with no decisions, removes the entry and closes', async () => {
      openEntry({ conflicts: [both] });
      await choose('a.yml', 'Keep mine');
      await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
      await waitFor(() => expect(mocks.api.reviewNetworkSync).toHaveBeenCalledWith('s1', false, []));
      await waitFor(() => expect(store().dialogSyncId).toBeNull());
      expect(store().syncReviews).toHaveLength(0);
    });

    it('closes without an answer and leaves the review pending', async () => {
      openEntry();
      await userEvent.keyboard('{Escape}');
      expect(store().dialogSyncId).toBeNull();
      expect(store().syncReviews).toHaveLength(1);
      expect(mocks.api.reviewNetworkSync).not.toHaveBeenCalled();
    });

    it('shows the error in a state block and keeps the buttons when the answer fails', async () => {
      mocks.api.reviewNetworkSync.mockRejectedValue('The sync is no longer running.');
      openEntry();
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      const block = await screen.findByText('The sync is no longer running.');
      expect(block.closest('[data-slot="state-block"]')).toHaveAttribute('data-tone', 'error');
      expect(screen.getByRole('button', { name: 'Accept' })).toBeEnabled();
      expect(screen.getByRole('button', { name: 'Decline' })).toBeEnabled();
    });
  });

  describe('after Accept', () => {
    async function accepted(patch: Partial<SyncReviewEntry> = {}) {
      openEntry(patch);
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      await screen.findByText('Syncing with Ben Ortiz...');
    }

    it('shows that the sync is running, with Cancel and Done', async () => {
      await accepted();
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
    });

    it('keeps Cancel enabled while bytes are missing', async () => {
      await accepted();
      act(() => store().setSyncProgress({ syncId: 's1', doneBytes: 50, totalBytes: 100 }));
      expect(screen.getByText('50 bytes of 100 bytes')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
    });

    it('disables Cancel once all bytes are in', async () => {
      await accepted();
      act(() => store().setSyncProgress({ syncId: 's1', doneBytes: 100, totalBytes: 100 }));
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    });

    it('does not disable Cancel for a total of zero', async () => {
      await accepted();
      act(() => store().setSyncProgress({ syncId: 's1', doneBytes: 0, totalBytes: 0 }));
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
    });

    it('Cancel cancels the sync and remembers it was local', async () => {
      await accepted();
      await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
      await waitFor(() => expect(mocks.api.cancelNetworkSync).toHaveBeenCalledWith('s1'));
      expect(store().wasSyncCancelledHere('s1')).toBe(true);
    });

    it('forgets the local cancel when the cancel is refused and shows why', async () => {
      mocks.api.cancelNetworkSync.mockRejectedValue('Already applying.');
      await accepted();
      await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
      expect(await screen.findByText('Already applying.')).toBeInTheDocument();
      expect(store().wasSyncCancelledHere('s1')).toBe(false);
    });

    it('Done closes the dialog and the sync goes on', async () => {
      await accepted();
      await userEvent.click(screen.getByRole('button', { name: 'Done' }));
      expect(store().dialogSyncId).toBeNull();
      expect(store().syncReviews).toHaveLength(1);
    });

    it('does not show the progress of another sync', async () => {
      await accepted();
      act(() => store().setSyncProgress({ syncId: 'other', doneBytes: 100, totalBytes: 100 }));
      expect(screen.queryByText('100 bytes of 100 bytes')).toBeNull();
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
    });
  });

  describe('late events', () => {
    it('keeps the dialog of one sync when another sync finishes', () => {
      store().addSyncReview(makeSyncReview({ syncId: 's2', displayName: 'Cara Lund' }));
      openEntry();
      act(() =>
        handleNetworkSync({
          type: 'finished',
          sync_id: 's2',
          link_id: 'L2',
          display_name: 'Cara Lund',
          collection_name: 'Other',
          outcome: 'applied',
          message: null,
        }),
      );
      expect(
        screen.getByRole('dialog', { name: 'Ben Ortiz wants to sync a collection' }),
      ).toBeInTheDocument();
    });

    it('a late Accept answer does not bring a finished sync back', async () => {
      const answer = createDeferred<undefined>();
      mocks.api.reviewNetworkSync.mockReturnValue(answer.promise);
      openEntry();
      await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
      act(() =>
        handleNetworkSync({
          type: 'finished',
          sync_id: 's1',
          link_id: 'L1',
          display_name: 'Ben Ortiz',
          collection_name: 'Orders',
          outcome: 'cancelled',
          message: null,
        }),
      );
      await act(async () => answer.resolve(undefined));
      expect(store().syncReviews).toHaveLength(0);
      expect(store().dialogSyncId).toBeNull();
    });

    it('a late Decline answer does not close the dialog of another sync', async () => {
      const answer = createDeferred<undefined>();
      mocks.api.reviewNetworkSync.mockReturnValue(answer.promise);
      store().addSyncReview(makeSyncReview({ syncId: 's2' }));
      openEntry();
      await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
      act(() => store().openSyncDialog('s2'));
      await act(async () => answer.resolve(undefined));
      expect(store().dialogSyncId).toBe('s2');
    });

    it('shows why an unanswered review ended', () => {
      openEntry();
      act(() =>
        handleNetworkSync({
          type: 'finished',
          sync_id: 's1',
          link_id: 'L1',
          display_name: 'Ben Ortiz',
          collection_name: 'Orders',
          outcome: 'failed',
          message: 'The other node went offline.',
        }),
      );
      expect(screen.getByText('The other node went offline.')).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
      expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
    });

    it('switches to the expired view when the review expires', () => {
      openEntry();
      act(() =>
        handleNetworkSync({
          type: 'finished',
          sync_id: 's1',
          link_id: 'L1',
          display_name: 'Ben Ortiz',
          collection_name: 'Orders',
          outcome: 'expired',
          message: null,
        }),
      );
      expect(screen.getByText('The request expired.')).toBeInTheDocument();
    });
  });

  describe('confirm', () => {
    const confirm = (patch: Partial<SyncReviewEntry> = {}) =>
      openEntry({
        kind: 'confirm',
        summary: makeSyncSummary({ addedHere: 1, changedHere: 1, deletedHere: 1 }),
        ...patch,
      });

    it('asks to apply the changes of the other node', () => {
      confirm();
      expect(screen.getByRole('dialog', { name: 'Apply 3 changes from Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByText('Coming to you: 1 added, 1 changed, 1 deleted')).toBeInTheDocument();
    });

    it('says one change in the singular', () => {
      confirm({ summary: makeSyncSummary({ addedHere: 1, changedHere: 0, deletedHere: 0 }) });
      expect(screen.getByRole('dialog', { name: 'Apply 1 change from Ben Ortiz' })).toBeInTheDocument();
    });

    it('shows the same marks and the sandbox warning', () => {
      confirm({ summary: makeSyncSummary({ scriptsChanged: ['s.yml'], authChanged: ['t.yml'] }) });
      expect(screen.getByText('Scripts changed (1)')).toBeInTheDocument();
      expect(screen.getByText('Auth changed (1)')).toBeInTheDocument();
      expect(screen.getByText(/Changed scripts run in the Safe sandbox/)).toBeInTheDocument();
    });

    it('shows no conflict choices in the confirm dialog', () => {
      confirm({ conflicts: [both] });
      expect(screen.queryByText(/Conflicts/)).toBeNull();
      expect(screen.queryByRole('radio')).toBeNull();
    });

    it('applies', async () => {
      confirm();
      await userEvent.click(screen.getByRole('button', { name: 'Apply' }));
      await waitFor(() => expect(mocks.api.confirmNetworkSync).toHaveBeenCalledWith('s1', true));
      await screen.findByText('Syncing with Ben Ortiz...');
    });

    it('declines', async () => {
      confirm();
      await userEvent.click(screen.getByRole('button', { name: 'Decline' }));
      await waitFor(() => expect(mocks.api.confirmNetworkSync).toHaveBeenCalledWith('s1', false));
      await waitFor(() => expect(store().dialogSyncId).toBeNull());
    });
  });
});
```

- [ ] **Step 10: Run and see it fail**

Run: `bun run test src/components/network/__tests__/SyncReviewDialog.test.tsx`
Expected: FAIL, the module does not exist.

- [ ] **Step 11: Create `src/components/network/SyncReviewDialog.tsx`**

Signature: `export function SyncReviewDialog()`. It reads the store with narrow selectors, as `IncomingTransferDialog` does: `id = s.dialogSyncId`, `entry = s.syncReviews.find(r => r.syncId === s.dialogSyncId)`, `progress = s.syncProgress.find(p => p.syncId === s.dialogSyncId)`, `endedText = s.dialogSyncEndedText`. Local state: `choices: Record<string, SyncResolution>` and `comparing: string | null`, both reset in an effect keyed on `id`. `useAsyncAction('inline')` gives `busy`, `error`, `run`, `reset` (reset on `id` change, with the same biome-ignore comment as `IncomingTransferDialog`). `const store = () => useNetworkStore.getState();` for the after-await checks. Layout and pinned texts:

- `if (!id) return null;`. A `Dialog open` with `onOpenChange` closing through `store().closeSyncDialog()` when not busy (an unanswered entry stays pending, its toast comes back through the hook). `DialogContent className='sm:max-w-2xl'` (wide for the diff), body scrolls (`max-h-[80vh] overflow-y-auto`).
- Titles: no entry: `Sync` with description `endedText ?? 'The request expired.'` and a `Done` button. `review` and not answered: `${peerName(displayName)} wants to sync a collection`, description `Nothing is saved until you accept.`. `confirm` and not answered: `Apply ${n} ${n === 1 ? 'change' : 'changes'} from ${name}` where `n = addedHere + changedHere + deletedHere`, description `Nothing is saved until you apply.`. Answered: `Syncing with ${name}`, description `You can close this window. The sync goes on.`.
- Detail block (not answered): `<p className='break-words text-sm font-medium'>{collectionName}</p>`, `<p className='font-mono text-xs text-muted-foreground'>{\`Device ID ${shortId}\`}</p>`, `<p className='text-xs'>{\`Coming to you: ${syncCountsText(addedHere, changedHere, deletedHere)}\`}</p>`, `<p className='text-xs'>{\`Going to ${name}: ${syncCountsText(sentAdded, sentChanged, sentDeleted)}\`}</p>`. A node's name is rendered as text only.
- Marks: when `scriptsChanged.length > 0` a section with a `ShieldAlert` icon, the heading `Scripts changed (${n})` and a `ul` of the paths in `font-mono text-xs break-all`; when `authChanged.length > 0` the same with `KeyRound` and `Auth changed (${n})`. Always (not answered) the sandbox note, `role='note'`, `ShieldAlert`, `SAFE_SANDBOX_NOTE`, in a plain bordered paragraph (as `IncomingTransferDialog`, `StateBlock` has no warning tone).
- Conflicts (only `kind === 'review'`, not answered, and `conflicts.length > 0`): heading `Conflicts (${n})` and the help `Choose what to keep for each one. Nothing changes until you press Accept.`. One list item per conflict: the path in `font-mono text-xs break-all`, `conflictKindText(kind)` in `text-2xs text-muted-foreground`, a `Button variant='ghost' size='sm'` with `aria-label={\`Compare ${path}\`}` (or `Hide ${path}` while that conflict is open) and a `GitCompare` icon, and `RadioGroup aria-label={\`Resolution for ${path}\`} value={choices[path] ?? ''} onValueChange` with `RadioGroupItem` + `Label` pairs `Keep mine`, `Keep theirs` and, only when `mineHash !== null && theirsHash !== null`, `Keep both` (ids `sync-${path}-mine` and so on; a path can hold spaces, so build ids with `encodeURIComponent(path)`). When `comparing === path`, render `<SyncConflictDiff syncId={id} conflict={conflict} peerName={name} />` under that row. The decisions array is built in the order of `entry.conflicts`: `entry.conflicts.map(c => ({ path: c.path, resolution: choices[c.path] }))`, sent only when every conflict has a choice (`allChosen`).
- Running view (answered): `<p role='status'>Syncing with {name}...</p>` and, when `progress` exists, `<p role='status' className='text-xs ...'>{\`${formatBytes(done)} of ${formatBytes(total)}\`}</p>`.
- Error: `{error && <StateBlock tone='error'>{error}</StateBlock>}` above the footer.
- Footer: no entry: `Done`. Answered: `Cancel` (`variant='outline'`, `disabled={busy || allBytesIn}` with `allBytesIn = progress !== undefined && progress.totalBytes > 0 && progress.doneBytes >= progress.totalBytes`) and `Done`. `review` open: `Decline` (`disabled={busy}`) and `Accept` (`disabled={busy || !allChosen}`). `confirm` open: `Decline` and `Apply` (`disabled={busy}`).
- Actions, each through `run`: `accept` calls `reviewNetworkSync(id, true, decisions)` (or `confirmNetworkSync(id, true)` for `confirm`) and then `store().markSyncAnswered(id)`; `markSyncAnswered` ignores a sync that finished meanwhile. `decline` calls `reviewNetworkSync(id, false, [])` (or `confirmNetworkSync(id, false)`), then `store().removeSyncReview(id)` and, only `if (store().dialogSyncId === id)`, `store().closeSyncDialog()`. `cancel` calls `markSyncCancelledHere(id)`, then `cancelNetworkSync(id)`; on a rejection it calls `unmarkSyncCancelledHere(id)` and rethrows so `run` shows the text.

- [ ] **Step 12: Run the dialog tests**

Run: `bun run test src/components/network/__tests__/SyncReviewDialog.test.tsx src/components/network/__tests__/SyncConflictDiff.test.tsx`
Expected: PASS for both files.

### 2d. Toast, hook and mounting

- [ ] **Step 13: Write the failing toast and hook tests**

Create `src/components/network/__tests__/IncomingSyncToast.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IncomingSyncToast, incomingSyncToastId } from '@/components/network/IncomingSyncToast';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({ toast: { dismiss: vi.fn() } }));
vi.mock('sonner', () => ({ toast: mocks.toast }));
vi.mock('@/lib/tauri-api', () => ({}));

const props = {
  syncId: 's1',
  kind: 'review' as const,
  displayName: 'Ben Ortiz',
  shortId: 'AAAA-1111',
  collectionName: 'Orders "v2"',
};

describe('IncomingSyncToast', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('names the sender, the collection and the short id', () => {
    render(<IncomingSyncToast {...props} />);
    expect(screen.getByText('Ben Ortiz wants to sync a collection')).toBeInTheDocument();
    expect(screen.getByText('Orders "v2"')).toBeInTheDocument();
    expect(screen.getByText('AAAA-1111')).toBeInTheDocument();
  });

  it('falls back when the display name is empty', () => {
    render(<IncomingSyncToast {...props} displayName='' />);
    expect(screen.getByText('Unnamed node wants to sync a collection')).toBeInTheDocument();
  });

  it('words a confirm differently', () => {
    render(<IncomingSyncToast {...props} kind='confirm' />);
    expect(screen.getByText('Ben Ortiz has changes ready to apply')).toBeInTheDocument();
  });

  it('has only a Review action that opens the dialog and dismisses the toast', async () => {
    render(<IncomingSyncToast {...props} />);
    expect(screen.getAllByRole('button')).toHaveLength(1);
    await userEvent.click(screen.getByRole('button', { name: 'Review' }));
    expect(useNetworkStore.getState().dialogSyncId).toBe('s1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith(incomingSyncToastId('s1'));
  });
});
```

Create `src/hooks/__tests__/useIncomingSyncNotice.test.tsx`:

```tsx
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { incomingSyncToastId } from '@/components/network/IncomingSyncToast';
import { useIncomingSyncNotice } from '@/hooks/useIncomingSyncNotice';
import { useNetworkStore } from '@/stores/network-store';
import { makeSyncReview } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({ toast: { custom: vi.fn(), dismiss: vi.fn() } }));
vi.mock('sonner', () => ({ toast: mocks.toast }));
vi.mock('@/lib/tauri-api', () => ({}));

const store = () => useNetworkStore.getState();
const customIds = () => mocks.toast.custom.mock.calls.map((c) => c[1].id);

describe('useIncomingSyncNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  it('shows one toast per entry, with different ids', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview({ syncId: 's1' }));
      store().addSyncReview(makeSyncReview({ syncId: 's2' }));
    });
    expect(customIds()).toEqual([incomingSyncToastId('s1'), incomingSyncToastId('s2')]);
  });

  it('shows no second toast for a duplicate review', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview());
      store().addSyncReview(makeSyncReview());
    });
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('shows the toast of an entry that exists at mount', () => {
    store().addSyncReview(makeSyncReview());
    renderHook(() => useIncomingSyncNotice());
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('shows no toast for an answered entry', () => {
    store().addSyncReview(makeSyncReview({ answered: true }));
    renderHook(() => useIncomingSyncNotice());
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });

  it('shows no toast for the entry whose dialog is already open', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      useNetworkStore.setState({ dialogSyncId: 's1' });
      store().addSyncReview(makeSyncReview({ kind: 'confirm' }));
    });
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });

  it('dismisses the toast when the entry goes away', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview());
      store().finishSync('s1');
    });
    expect(mocks.toast.dismiss).toHaveBeenCalledWith(incomingSyncToastId('s1'));
  });

  it('brings the toast back when the dialog closes without an answer', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview());
      store().openSyncDialog('s1');
    });
    mocks.toast.custom.mockClear();
    act(() => store().closeSyncDialog());
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('does not bring a toast back for an answered entry', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview());
      store().markSyncAnswered('s1');
      store().openSyncDialog('s1');
    });
    mocks.toast.custom.mockClear();
    act(() => store().closeSyncDialog());
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });
});
```

Note: `markSyncAnswered` replaces the entry, which changes `syncReviews`; the hook must not toast an entry that was only updated (it compares by id, as the transfer hook does).

- [ ] **Step 14: Run and see them fail**

Run: `bun run test src/components/network/__tests__/IncomingSyncToast.test.tsx src/hooks/__tests__/useIncomingSyncNotice.test.tsx`
Expected: FAIL, the modules do not exist.

- [ ] **Step 15: Create the toast and the hook**

`src/components/network/IncomingSyncToast.tsx` copies `IncomingTransferToast.tsx`: `export const incomingSyncToastId = (syncId: string): string => \`network-sync-offer-${syncId}\`;` and `IncomingSyncToast({ syncId, kind, displayName, shortId, collectionName })`. Title: `kind === 'review'` `${peerName(displayName)} wants to sync a collection`, else `${peerName(displayName)} has changes ready to apply`. The collection name is its own `<p className='break-words text-xs text-muted-foreground'>`, the short id is `font-mono text-xs`. One `Button size='sm'` "Review" that calls `useNetworkStore.getState().openSyncDialog(syncId)` and `toast.dismiss(incomingSyncToastId(syncId))`.

`src/hooks/useIncomingSyncNotice.tsx` copies `useIncomingTransferNotice.tsx`: `showToast(entry)` with `toast.custom(() => <IncomingSyncToast .../>, { id: incomingSyncToastId(entry.syncId), duration: INCOMING_TTL_MS })`. A toast is shown for an entry that is not `answered` and whose `syncId !== dialogSyncId`. The subscription compares `syncReviews` by id (new ids get a toast, ids that left get `toast.dismiss`), and a closed dialog (`prev.dialogSyncId !== null && state.dialogSyncId !== prev.dialogSyncId`) brings the toast back for an unanswered entry that is still there.

- [ ] **Step 16: Mount in `App.tsx`**

Add `import { SyncReviewDialog } from '@/components/network/SyncReviewDialog';` and `import { useIncomingSyncNotice } from '@/hooks/useIncomingSyncNotice';`. Add `useIncomingSyncNotice();` after the line `useIncomingTransferNotice();` and `<SyncReviewDialog />` after `<IncomingTransferDialog />`.

- [ ] **Step 17: Run the toast and hook tests**

Run: `bun run test src/components/network/__tests__/IncomingSyncToast.test.tsx src/hooks/__tests__/useIncomingSyncNotice.test.tsx`
Expected: PASS, 12 tests.

### 2e. Unlink dialog and the collection menu

- [ ] **Step 18: Write the failing unlink and menu tests**

Create `src/components/network/__tests__/UnlinkDialog.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { UnlinkDialog } from '@/components/network/UnlinkDialog';
import { useNetworkStore } from '@/stores/network-store';
import { makeLink } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  api: {
    unlinkNetworkLink: vi.fn(),
    listNetworkLinks: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

describe('UnlinkDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
    mocks.api.unlinkNetworkLink.mockResolvedValue(undefined);
    mocks.api.listNetworkLinks.mockResolvedValue([]);
  });

  it('stays closed without a link', () => {
    render(<UnlinkDialog link={null} onClose={vi.fn()} />);
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('says what an unlink does and does not do', () => {
    render(<UnlinkDialog link={makeLink({ collectionName: 'Orders "v2"' })} onClose={vi.fn()} />);
    expect(screen.getByRole('alertdialog')).toHaveTextContent('Unlink Orders "v2" from Ben Ortiz?');
    expect(screen.getByRole('alertdialog')).toHaveTextContent(
      'Your collection and its files stay as they are. Ben Ortiz (AAAA-1111) learns of it at the next sync.',
    );
  });

  it('unlinks and closes', async () => {
    const onClose = vi.fn();
    render(<UnlinkDialog link={makeLink()} onClose={onClose} />);
    await userEvent.click(screen.getByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(mocks.api.unlinkNetworkLink).toHaveBeenCalledWith('L1'));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it('shows a refused unlink as a toast', async () => {
    mocks.api.unlinkNetworkLink.mockRejectedValue('Could not unlink.');
    render(<UnlinkDialog link={makeLink()} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(mocks.toast.error).toHaveBeenCalled());
  });

  it('does nothing on Cancel', async () => {
    const onClose = vi.fn();
    render(<UnlinkDialog link={makeLink()} onClose={onClose} />);
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mocks.api.unlinkNetworkLink).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });
});
```

Create `src/components/collections/__tests__/CollectionNode.sync.test.tsx`. Copy the header of `CollectionNode.send.test.tsx` (the `QueryClientProvider` imports, the `@tauri-apps/api/event` mock, `makeSummary`, `renderNode`, the `Menu` type and `openMenu`), with this `tauri-api` mock and an added `sonner` mock:

```tsx
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSummaries: vi.fn(),
    getOpenApiSyncStatus: vi.fn(),
    getOpenApiSyncBadges: vi.fn(),
    startNetworkSync: vi.fn(),
    cancelNetworkSync: vi.fn(),
    unlinkNetworkLink: vi.fn(),
    listNetworkLinks: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

vi.mock('sonner', async (original) => {
  const actual = await original<typeof import('sonner')>();
  return {
    ...actual,
    toast: Object.assign(vi.fn(), actual.toast, {
      info: vi.fn(),
      error: vi.fn(),
      success: vi.fn(),
      dismiss: vi.fn(),
    }),
  };
});
```

Then the tests:

```tsx
const ben = makeNode({ deviceId: 'DEV1', displayName: 'Ben Ortiz', group: 'paired', status: 'online' });
const caraOffline = makeNode({
  deviceId: 'DEV2',
  shortId: 'BBBB-2222',
  displayName: 'Cara Lund',
  group: 'paired',
  status: 'offline',
});

describe('CollectionNode sync entries', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useNetworkStore.getState().reset();
    vi.mocked(tauriApi.getOpenApiSyncBadges).mockResolvedValue([]);
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(null);
    vi.mocked(tauriApi.startNetworkSync).mockResolvedValue({ syncId: 's1' });
    vi.mocked(tauriApi.unlinkNetworkLink).mockResolvedValue(undefined);
    vi.mocked(tauriApi.listNetworkLinks).mockResolvedValue([]);
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'my-collection',
      root: { uid: 'root', name: 'my-collection', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
  });

  describe.each<Menu>(['dropdown', 'context'])('in the %s menu', (menu) => {
    it('shows nothing for a collection without a link', async () => {
      useNetworkStore.setState({ nodes: [ben], links: [makeLink({ collectionName: 'other' })] });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.queryByRole('menuitem', { name: /Sync with/ })).toBeNull();
      expect(screen.queryByRole('menuitem', { name: /Unlink/ })).toBeNull();
    });

    it('shows the status line, Sync with and Unlink for a linked, online peer', async () => {
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection', state: 'changes_waiting' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.getByText('Changes waiting to sync with Ben Ortiz')).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Sync with Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Unlink' })).toBeInTheDocument();
    });

    it('hides Sync with ... when the peer is offline', async () => {
      useNetworkStore.setState({
        nodes: [caraOffline],
        links: [
          makeLink({
            collectionName: 'my-collection',
            deviceId: 'DEV2',
            displayName: 'Cara Lund',
            shortId: 'BBBB-2222',
          }),
        ],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.queryByRole('menuitem', { name: /Sync with/ })).toBeNull();
    });

    it('shows Unlink and the status line when the peer is offline', async () => {
      useNetworkStore.setState({
        nodes: [caraOffline],
        links: [
          makeLink({
            collectionName: 'my-collection',
            deviceId: 'DEV2',
            displayName: 'Cara Lund',
            state: 'broken',
          }),
        ],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(
        screen.getByText('Link to Cara Lund is broken. Unlink and send the collection again.'),
      ).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Unlink' })).toBeInTheDocument();
    });

    it('shows one group per linked peer', async () => {
      useNetworkStore.setState({
        nodes: [ben, { ...caraOffline, status: 'online' }],
        links: [
          makeLink({ collectionName: 'my-collection' }),
          makeLink({
            linkId: 'L2',
            deviceId: 'DEV2',
            displayName: 'Cara Lund',
            shortId: 'BBBB-2222',
            collectionName: 'my-collection',
          }),
        ],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      expect(screen.getByRole('menuitem', { name: 'Sync with Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Sync with Cara Lund' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Unlink from Ben Ortiz' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Unlink from Cara Lund' })).toBeInTheDocument();
    });

    it('starts the sync for that link', async () => {
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await user.click(screen.getByRole('menuitem', { name: 'Sync with Ben Ortiz' }));
      await waitFor(() => expect(tauriApi.startNetworkSync).toHaveBeenCalledWith('L1'));
    });

    it('shows the refusal of a start as an error toast', async () => {
      vi.mocked(tauriApi.startNetworkSync).mockRejectedValue('Another sync is already running for this link.');
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await user.click(screen.getByRole('menuitem', { name: 'Sync with Ben Ortiz' }));
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
    });

    it('asks before it unlinks', async () => {
      useNetworkStore.setState({
        nodes: [ben],
        links: [makeLink({ collectionName: 'my-collection' })],
      });
      const user = userEvent.setup();
      renderNode(makeSummary('my-collection'));
      await openMenu(menu, 'my-collection', user);
      await user.click(screen.getByRole('menuitem', { name: 'Unlink' }));
      expect(tauriApi.unlinkNetworkLink).not.toHaveBeenCalled();
      await user.click(await screen.findByRole('button', { name: 'Unlink' }));
      await waitFor(() => expect(tauriApi.unlinkNetworkLink).toHaveBeenCalledWith('L1'));
    });
  });
});
```

Add `import { toast } from 'sonner';` and `import { makeLink, makeNode } from '@/test/network-fixtures';` to the imports.

- [ ] **Step 19: Run and see them fail**

Run: `bun run test src/components/network/__tests__/UnlinkDialog.test.tsx src/components/collections/__tests__/CollectionNode.sync.test.tsx`
Expected: FAIL, `UnlinkDialog` does not exist and the menu has no sync entries.

- [ ] **Step 20: Create the dialog, the hook and the menu items**

`src/components/network/UnlinkDialog.tsx`: copies `RemovePairedDialog.tsx`. Props `{ link: NetworkLink | null; onClose: () => void }`. Title `Unlink ${link.collectionName} from ${peerName(link.displayName)}?`; description `Your collection and its files stay as they are. ${peerName(link.displayName)} (${link.shortId}) learns of it at the next sync. To sync again, send the collection again with Keep in sync.`. The test above checks the two sentences it quotes, so keep them verbatim and put the third after them. `AlertDialogAction` "Unlink" calls `run(async () => { await useNetworkStore.getState().unlink(link.linkId); onClose(); })` with a toast `useAsyncAction()`; `AlertDialogCancel` "Cancel" keeps Radix's close, whose `onOpenChange` calls `onClose`. Use `e.preventDefault()` in the action's `onClick` so the dialog stays open on a failure (Radix closes an alert dialog on action press otherwise); on success `onClose()` closes it.

`src/hooks/useCollectionSyncLinks.ts`:

```ts
export interface CollectionSyncEntry {
  link: NetworkLink;
  /** The peer is paired and online now. */
  online: boolean;
}

/** The links of one collection, matched by collection name (ruling 10), with the peer's online flag. */
export function useCollectionSyncLinks(collectionName: string): CollectionSyncEntry[];
```

Body: two narrow selectors (`s.links`, `s.nodes`), then `useMemo` building the entries: `links.filter(l => l.collectionName === collectionName).map(link => ({ link, online: nodes.some(n => n.deviceId === link.deviceId && n.group === 'paired' && n.status === 'online') }))`. It returns a stable empty array constant when there is no match so a row without links does not re-render.

`src/components/collections/CollectionSyncMenuItems.tsx`: exports `DropdownSyncItems` and `ContextSyncItems`, both with props `{ entries: CollectionSyncEntry[]; onSync: (link: NetworkLink) => void; onUnlink: (link: NetworkLink) => void }`, returning `null` for an empty list. For each entry (key `link.linkId`): a separator (`DropdownMenuSeparator` / `ContextMenuSeparator`) before the first group; the status line as `DropdownMenuLabel` / `ContextMenuLabel` with `className='text-2xs font-normal text-muted-foreground'` and `linkStatusText(link.state, link.displayName)`; when `online` an item with the `RefreshCw` icon (`aria-hidden`, `h-3.5 w-3.5 mr-2`) and the text `Sync with ${peerName(link.displayName)}` calling `onSync(link)`; always an item with the `Unlink` icon and the text `Unlink` when `entries.length === 1`, else `Unlink from ${peerName(link.displayName)}`, calling `onUnlink(link)`. Both components share one small pure helper `unlinkLabel(count, name)` in the same file.

- [ ] **Step 21: Edit `CollectionNode.tsx`**

Add imports (`useCollectionSyncLinks`, `DropdownSyncItems`, `ContextSyncItems`, `UnlinkDialog`, `startCollectionSync`, `toastError`, `type NetworkLink`). Next to the line `const [sendOpen, setSendOpen] = useState(false);` add:

```tsx
const syncEntries = useCollectionSyncLinks(summary.name);
const [unlinkTarget, setUnlinkTarget] = useState<NetworkLink | null>(null);
const syncWith = (link: NetworkLink) => void startCollectionSync(link).catch(toastError);
```

Directly after the closing `)}` of the `{canSend && (...)}` block in the dropdown menu (the one containing `<DropdownMenuItem onClick={() => setSendOpen(true)}>`) add `<DropdownSyncItems entries={syncEntries} onSync={syncWith} onUnlink={setUnlinkTarget} />`, and the same with `ContextSyncItems` after the `ContextMenuItem` Send block. After the `SendCollectionDialog` block add `<UnlinkDialog link={unlinkTarget} onClose={() => setUnlinkTarget(null)} />`. `toastError` is the helper from `@/lib/error-text` that `SendCollectionDialog` already imports.

- [ ] **Step 22: Run the task checks**

Run: `bun run test src/components/network src/components/collections/__tests__ src/hooks/__tests__/useIncomingSyncNotice.test.tsx src/hooks/__tests__/useIncomingTransferNotice.test.tsx`
Expected: PASS for every file, the old send tests included.
Run: `bun run tsc --noEmit`
Expected: no errors.
Run: `bun run check`
Expected: no findings in the new and changed files (if it reports format or import order, run `bun run lint`, then `bun run check` again).

---

## Task 3: Link list, audit rows, docs and manual checks

**Files:**
- Modify: `src/components/network/NetworkPanel.tsx`, `src/components/status-bar/NetworkStatusItem.tsx`, `src/components/settings/sections/NetworkSection.tsx`, `src/lib/tauri-api.ts` (`AuditEventKind` union), `src/components/audit/AuditEventRow.tsx`, `src/components/audit/__tests__/AuditEventRow.network.test.tsx`, `docs/features/team-network.md`, `docs/manual-tests/team-network.md`, `docs/roadmaps/roadmap.md`, and `.claude/docs-index.md` only if needed
- Create: `src/components/network/LinkList.tsx`, `src/components/network/__tests__/LinkList.test.tsx`, `src/components/network/__tests__/NetworkPanel.links.test.tsx`, `src/components/settings/sections/__tests__/NetworkSection.links.test.tsx`

**Interfaces:**
- Consumes: `links` in the store, `UnlinkDialog`, `linkStatusText`, `ListRow`, `NetworkIdentity`, `Badge`.
- Produces: `LinkList`, the panel section and Settings block, three audit summaries.

### 3a. The link list

- [ ] **Step 1: Write the failing list tests**

Create `src/components/network/__tests__/LinkList.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { LinkList } from '@/components/network/LinkList';
import { makeLink } from '@/test/network-fixtures';

describe('LinkList', () => {
  it('shows each link with its collection, node, short id and state', () => {
    render(
      <LinkList
        links={[
          makeLink({ collectionName: 'Orders "v2"', state: 'changes_waiting' }),
          makeLink({
            linkId: 'L2',
            collectionName: 'Billing',
            displayName: 'Cara Lund',
            shortId: 'BBBB-2222',
            state: 'broken',
          }),
        ]}
        onUnlink={vi.fn()}
      />,
    );
    const items = screen.getAllByRole('listitem');
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent('Orders "v2"');
    expect(items[0]).toHaveTextContent('Ben Ortiz');
    expect(items[0]).toHaveTextContent('AAAA-1111');
    expect(items[0]).toHaveTextContent('Changes waiting');
    expect(items[1]).toHaveTextContent('Broken');
  });

  it('never shows a full device id', () => {
    render(<LinkList links={[makeLink({ deviceId: 'K7QF2M9XPYE7HWE7ABCDEFGHIJ' })]} onUnlink={vi.fn()} />);
    expect(screen.queryByText(/K7QF2M9XPYE7HWE7ABCDEFGHIJ/)).toBeNull();
  });

  it('names a node without a display name', () => {
    render(<LinkList links={[makeLink({ displayName: '' })]} onUnlink={vi.fn()} />);
    expect(screen.getByRole('listitem')).toHaveTextContent('Unnamed node');
  });

  it('calls onUnlink with the link', async () => {
    const onUnlink = vi.fn();
    const link = makeLink();
    render(<LinkList links={[link]} onUnlink={onUnlink} />);
    await userEvent.click(
      screen.getByRole('button', { name: 'Unlink Orders from Ben Ortiz (AAAA-1111)' }),
    );
    expect(onUnlink).toHaveBeenCalledWith(link);
  });

  it('renders nothing for an empty list', () => {
    const { container } = render(<LinkList links={[]} onUnlink={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });
});
```

Create `src/components/network/__tests__/NetworkPanel.links.test.tsx`. Look at the existing `NetworkPanel.test.tsx` for its `vi.hoisted` mock and its way to put the store into a running status (it uses `makeStatus`), copy that harness, and add:

```tsx
it('shows the linked collections and passes Unlink to the host', async () => {
  useNetworkStore.setState({
    status: makeStatus(),
    nodes: [makeNode({ group: 'paired', status: 'online' })],
    links: [makeLink()],
  });
  const onUnlink = vi.fn();
  render(<NetworkPanel onAddAddress={vi.fn()} onRemove={vi.fn()} onUnlink={onUnlink} />);
  expect(screen.getByText('Linked collections (1)')).toBeInTheDocument();
  await userEvent.click(
    screen.getByRole('button', { name: 'Unlink Orders from Ben Ortiz (AAAA-1111)' }),
  );
  expect(onUnlink).toHaveBeenCalledWith(makeLink());
});

it('shows no linked section without links', () => {
  useNetworkStore.setState({ status: makeStatus(), nodes: [makeNode()], links: [] });
  render(<NetworkPanel onAddAddress={vi.fn()} onRemove={vi.fn()} onUnlink={vi.fn()} />);
  expect(screen.queryByText(/Linked collections/)).toBeNull();
});
```

Create `src/components/settings/sections/__tests__/NetworkSection.links.test.tsx`. Copy the hoisted mock and the `beforeEach` of `NetworkSection.test.tsx`, add `listNetworkLinks`, `unlinkNetworkLink` to the mock, and:

```tsx
it('lists the linked collections and unlinks after a confirmation', async () => {
  mocks.api.unlinkNetworkLink.mockResolvedValue(undefined);
  mocks.api.listNetworkLinks.mockResolvedValue([]);
  useNetworkStore.setState({ status: makeStatus(), links: [makeLink()] });
  render(<NetworkSection />);
  expect(screen.getByRole('heading', { name: 'Linked collections' })).toBeInTheDocument();
  await userEvent.click(
    screen.getByRole('button', { name: 'Unlink Orders from Ben Ortiz (AAAA-1111)' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Unlink' }));
  await waitFor(() => expect(mocks.api.unlinkNetworkLink).toHaveBeenCalledWith('L1'));
});

it('says there are no linked collections', () => {
  useNetworkStore.setState({ status: makeStatus(), links: [] });
  render(<NetworkSection />);
  expect(screen.getByText('No linked collections.')).toBeInTheDocument();
});
```

(The panel and section tests must import `makeLink`, `makeStatus`, `makeNode` from `@/test/network-fixtures`, `userEvent`, `render`, `screen`, `waitFor`, and `useNetworkStore`; follow the neighbouring file for the rest.)

- [ ] **Step 2: Run and see them fail**

Run: `bun run test src/components/network/__tests__/LinkList.test.tsx src/components/network/__tests__/NetworkPanel.links.test.tsx src/components/settings/sections/__tests__/NetworkSection.links.test.tsx`
Expected: FAIL, `LinkList` does not exist and neither screen shows links.

- [ ] **Step 3: Create `LinkList` and use it**

`src/components/network/LinkList.tsx`:

```tsx
interface LinkListProps {
  links: NetworkLink[];
  onUnlink: (link: NetworkLink) => void;
}

/** Rows of linked collections: collection name, node with short id, state, and an Unlink button. */
export function LinkList({ links, onUnlink }: LinkListProps);
```

It returns `null` for an empty list, else `<ul aria-label='Linked collections'>` of `ListRow className='px-3 py-1.5'` rows (the panel gives the padding; Settings passes the same list inside a `Block`, the extra padding is harmless). Each row: a `min-w-0 flex-1` column with the collection name (`break-words text-sm`), `NetworkIdentity name={link.displayName} shortId={link.shortId}` (it already handles the short id and an empty name through `peerName`; check its props in `NetworkIdentity.tsx` and pass what it takes), a `Badge variant='secondary' className='px-1.5 py-0 text-2xs'` with the state label (`in_sync` "In sync", `changes_waiting` "Changes waiting", `out_of_date` "Out of date", `broken` "Broken", anything else "Linked"), and `Button size='sm' variant='outline' className='h-6 px-2 text-xs'` with `aria-label={\`Unlink ${link.collectionName} from ${peerName(link.displayName)} (${link.shortId})\`}` and the text "Unlink", with an `Unlink` icon.

`NetworkPanel.tsx`: add an optional prop `onUnlink?: (link: NetworkLink) => void`, a narrow selector `const links = useNetworkStore((s) => s.links);`, and under the Paired and Nearby groups (inside the scroll area, before the closing fragment of the node list branch) a section shown only when `links.length > 0 && onUnlink`:

```tsx
<section className='border-t border-border py-1'>
  <SectionLabel className='px-3 py-1'>{`Linked collections (${links.length})`}</SectionLabel>
  <LinkList links={links} onUnlink={onUnlink} />
</section>
```

Place it so it also shows when the search matches no nodes (put it after the `nodes.length === 0 ? ... : ...` ternary, not inside it).

`NetworkStatusItem.tsx`: add `const [unlinkTarget, setUnlinkTarget] = useState<NetworkLink | null>(null);`, pass `onUnlink={(link) => { setOpen(false); setUnlinkTarget(link); }}` to `NetworkPanel`, and mount `<UnlinkDialog link={unlinkTarget} onClose={() => setUnlinkTarget(null)} />` next to `RemovePairedDialog` (outside the popover, for the same reason).

`NetworkSection.tsx`: add a narrow selector for `links`, state `const [unlinkTarget, setUnlinkTarget] = useState<NetworkLink | null>(null);`, and a `Block title='Linked collections'` before `Blocked devices`: `{links.length === 0 ? <StateBlock tone='empty'>No linked collections.</StateBlock> : <LinkList links={links} onUnlink={setUnlinkTarget} />}`; mount `<UnlinkDialog link={unlinkTarget} onClose={() => setUnlinkTarget(null)} />` at the end of the section's root `div`. The section does not call `refreshLinks` (the bridge keeps the list fresh, ruling 13).

- [ ] **Step 4: Run the list tests and the neighbours**

Run: `bun run test src/components/network src/components/status-bar src/components/settings`
Expected: PASS for every file in those folders (the existing panel, status item and section tests must stay green: they never set `links`, so the new parts render nothing).

### 3b. Audit rows

- [ ] **Step 5: Write the failing audit tests**

In `src/components/audit/__tests__/AuditEventRow.network.test.tsx`, add inside the main `describe` (the file already has `show`, `FULL_ID` and `cleanup`):

```tsx
it('describes a sync that was sent, with the short id and no word for success', () => {
  show({ kind: 'network_sync_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'applied' });
  expect(screen.getByText('Sent sync changes for Orders to device K7QF-2M9X')).toBeInTheDocument();
});

it('describes a sync that was received, with the short id and no word for success', () => {
  show({
    kind: 'network_sync_received',
    device_id: FULL_ID,
    collection: 'Orders',
    outcome: 'applied',
  });
  expect(
    screen.getByText('Received sync changes for Orders from device K7QF-2M9X'),
  ).toBeInTheDocument();
});

it('shows the outcome of a sync that did not complete', () => {
  show({ kind: 'network_sync_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'failed' });
  expect(
    screen.getByText('Sent sync changes for Orders to device K7QF-2M9X (failed)'),
  ).toBeInTheDocument();
  cleanup();
  show({
    kind: 'network_sync_received',
    device_id: FULL_ID,
    collection: 'Orders',
    outcome: 'unconfirmed',
  });
  expect(
    screen.getByText('Received sync changes for Orders from device K7QF-2M9X (unconfirmed)'),
  ).toBeInTheDocument();
});

it('describes a removed link', () => {
  show({ kind: 'network_link_removed', device_id: FULL_ID, collection: 'Orders' });
  expect(screen.getByText('Unlinked collection Orders from device K7QF-2M9X')).toBeInTheDocument();
});

it('shows a collection name with a double quote as given', () => {
  show({ kind: 'network_link_removed', device_id: FULL_ID, collection: 'Orders "v2"' });
  expect(
    screen.getByText('Unlinked collection Orders "v2" from device K7QF-2M9X'),
  ).toBeInTheDocument();
});

it('does not show the full device id for the sync kinds', () => {
  show({ kind: 'network_sync_sent', device_id: FULL_ID, collection: 'Orders', outcome: 'applied' });
  expect(screen.queryByText(new RegExp(FULL_ID))).toBeNull();
});
```

- [ ] **Step 6: Run and see them fail**

Run: `bun run test src/components/audit/__tests__/AuditEventRow.network.test.tsx`
Expected: FAIL (type errors are not checked by vitest; the rows render nothing for the new kinds, so the text queries fail).

- [ ] **Step 7: Add the kinds and the summaries**

In `src/lib/tauri-api.ts`, in the `AuditEventKind` union directly after the `network_collection_received` line, add:

```ts
  | { kind: 'network_sync_sent'; device_id: string; collection: string; outcome: string }
  | { kind: 'network_sync_received'; device_id: string; collection: string; outcome: string }
  | { kind: 'network_link_removed'; device_id: string; collection: string };
```

In `src/components/audit/AuditEventRow.tsx`: change `outcomeSuffix` so that `outcome === 'completed' || outcome === 'applied'` gives `''` (ruling 12), and add after the `network_collection_received` case:

```ts
    case 'network_sync_sent':
      return `Sent sync changes for ${k.collection} to device ${shortDeviceId(k.device_id)}${outcomeSuffix(k.outcome)}`;
    case 'network_sync_received':
      return `Received sync changes for ${k.collection} from device ${shortDeviceId(k.device_id)}${outcomeSuffix(k.outcome)}`;
    case 'network_link_removed':
      return `Unlinked collection ${k.collection} from device ${shortDeviceId(k.device_id)}`;
```

- [ ] **Step 8: Run the audit tests**

Run: `bun run test src/components/audit`
Expected: PASS for every file in the folder.

### 3c. Docs, manual checks and roadmap

- [ ] **Step 9: Update `docs/features/team-network.md`**

Four edits, each anchored on quoted text:

1. In the intro paragraph (the one that starts "Rocket nodes on the same network find each other"), after "Paired nodes can send each other a collection (see Send a collection)." add " A sent collection can be kept in sync (see Keep a collection in sync)." and after the send spec reference add "Keep in sync: `docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md`, plans in `docs/superpowers/plans/team-network-sync/`."
2. In the "Where the code lives" table, after the "Send a collection" row add a row "Keep in sync": the protocol and planner (`crates/rocket-network`: `link.rs`, `planner.rs`, `events.rs` `SyncEvent`), `SyncService` and the frame router in `crates/rocket-app/src/network/`, the file merge in `crates/rocket-export/src/sync_merge.rs`, `FsLinkRepo` in `crates/rocket-infra`, commands in `src-tauri/src/commands/network.rs`. Frontend: `src/components/network/SyncReviewDialog.tsx`, `SyncConflictDiff.tsx`, `IncomingSyncToast.tsx`, `UnlinkDialog.tsx`, `LinkList.tsx`, `src/hooks/useIncomingSyncNotice.tsx`, `src/hooks/useCollectionSyncLinks.ts`, `src/components/collections/CollectionSyncMenuItems.tsx`, `src/lib/network-sync-actions.ts`, `src/lib/network-sync-utils.ts`, the `syncs` slice and `links` in `src/stores/network-store.ts`, `handleNetworkSync` in `src/lib/network-event-bridge.ts`. Data row: add "`~/.rocket-api/network/links/<link_id>/` (`link.yml` and `base/`)".
3. Add the section "## Keep a collection in sync" before "## Decisions", with these parts: **Make a link** (the **Keep in sync** box in the Send dialog, off by default; only a collection sent now can be linked; both sides store the link when the transfer completes). **Sync** (the collection menu shows a status line, **Sync with <name>** while the peer is paired and online, and **Unlink**; the flow in the numbered steps of spec section 6: the starter sends a manifest, the other node plans and shows the review dialog with counts, script and auth marks, conflicts with a read-only Monaco diff and keep mine, keep theirs, keep both; the starter then sees a confirm dialog; each node applies in one file transaction; "Synced" shows only after the peer's result). **Frontend rules**: the backend owns the state; `syncReviews` is deduped by sync id and expires after 120 seconds unless answered; late events for another sync are ignored; the planner decides conflicts, no default choice, keep both is hidden when a side has no file; Cancel is disabled once all bytes are in; the diff uses `DiffViewer` through `SyncConflictDiff` and loads both versions with `network_sync_conflict_files`; if that fails the choices still work; `refreshLinks` never throws; the links are matched to a collection by name. **Errors** (spec section 8): the texts in `describeSyncOutcome`. **Audit**: `network_sync_sent`, `network_sync_received` (fields `device_id`, `collection`, `outcome`) and `network_link_removed` (`device_id`, `collection`); never file names or contents.
4. In "## Limits" add: "A link joins two copies. A third copy links to either node separately." "Sync is manual and reviewed: no automatic or background sync." "Collections sent before this feature, and collections that exist on both sides, cannot be linked." "Environments, flows, gRPC and WebSocket requests, client certificates and proxy settings are not synced." In "## How to test" add `src/stores/__tests__/network-store.sync.test.ts src/lib/__tests__/network-event-bridge.sync.test.ts src/lib/__tests__/network-sync-utils.test.ts src/lib/__tests__/network-sync-actions.test.ts src/hooks/__tests__/useIncomingSyncNotice.test.tsx` to the vitest command (the folders `src/components/network`, `src/components/collections/__tests__`, `src/components/audit`, `src/components/settings` are already in it).

Also update the line "No sending of contracts yet. A collection can be sent as described in Send a collection." to end with " A sent collection can be kept in sync as described in Keep a collection in sync."

- [ ] **Step 10: Add manual-test section 12**

Append to `docs/manual-tests/team-network.md` (after check 22 of section 11), keeping the style of section 11:

```markdown
## 12: keep a collection in sync

Needs two paired machines, A and B, ideally one Linux and one Mac, both Online. Both must run the build with protocol version 3. Collection names below are examples. Open the Audit log on both machines before you start.

1. Make a link. On A create `Orders` with a folder and 3 requests. Right-click it, **Send to...**, pick B, tick **Keep in sync**, press Send. Accept on B. Expect the collection on B, and on both machines the collection menu now shows a status line "In sync with <peer>", **Sync with <peer>** and **Unlink**. The Network panel and Settings, Team network, list `Orders` under Linked collections on both. Send a second collection without ticking the box: its menu has none of the three entries.
2. Item visibility. Quit Rocket on B. On A open the `...` menu and the right-click menu of `Orders`. Expect the status line and **Unlink** but no **Sync with ...**. Start B again and wait for Online: **Sync with ...** is back in both menus.
3. Clean two-way sync. On A edit the URL of request R1. On B edit the name of request R2 and add a request R4. On A press **Sync with <B>**. A shows "Waiting for <B> to review the sync." with Cancel. B gets a toast, press Review: the dialog lists the counts for both sides, no conflicts, and the Safe sandbox note. Press Accept. A gets a confirm dialog "Apply N changes from <B>": press Apply. Expect "Synced "Orders" with <peer>." on both. Both copies hold all three edits. The status line says "In sync".
4. Conflict, three choices. On A and B edit the same request R1 differently, three times in a row (reset the pair with a fresh send between rounds if needed). Sync from A. On B open the dialog, press **Compare R1.yml**: expect a read-only side-by-side diff, your copy on the left, A's copy on the right, with no staging toggle, and no way to type in it. Accept stays off until every conflict has a choice. Round 1: **Keep mine**, expect B's version on both nodes. Round 2: **Keep theirs**, expect A's version on both. Round 3: **Keep both**, expect both versions on both nodes, the second with the name suffix "(from peer)". After each round both status lines say "In sync".
5. Delete against edit. On A delete request R3, on B edit R3. Sync from A. Expect a conflict "Deleted on one node, changed on the other" with only **Keep mine** and **Keep theirs**. Check both choices in two rounds.
6. Rename. On A move request R2 to another folder without editing it. Sync. Expect a rename on B (same content at the new path), not a delete plus an add: the counts show 0 added and 0 deleted for it.
7. Secrets and local settings survive. On B put a token in R1's auth and a saved example on R1. On A edit R1's URL and sync. Expect B to have the new URL, its own token and its example. Open the collection folder on B and check that no token of A's is there.
8. Script and auth marks. On A change a pre-request script in R1 and the auth type of R2. Sync. Expect the review dialog on B to list both files under "Scripts changed (1)" and "Auth changed (1)", and the Safe sandbox note. After Accept, the confirm dialog on A shows the same marks. After the sync no trust record exists for the collection on either side, and Developer mode is still off.
9. Decline. Sync from A and press Decline on B. Expect "<B> declined the sync." on A, no toast about its own choice on B, and no file changed on either side.
10. Expiry. Sync from A and leave B alone for 2 minutes. Expect "The request expired." on A. Repeat but open the dialog on B and leave it: the dialog switches to "The request expired." with only Done.
11. Cancel. Sync from A and press Cancel on A's waiting toast before B answers: B's toast and dialog go away and B shows "<A> cancelled the sync." and A shows nothing about it. Then start a sync that moves a large payload, and press Cancel in the dialog while bytes flow. Try once more and watch the Cancel button in the dialog: it is disabled once all bytes are in. Known open item: if the backend also emits progress for the small manifest, Cancel flickers off for a moment.
12. Offline during the review. Sync from A, open the dialog on B, quit Rocket on A. Expect B's dialog to switch to "The other node went offline." with only Done.
13. Base mismatch and unlink on one side. On B choose **Unlink** and confirm (the collection stays). Sync from A. Expect "The other node no longer has this link. Unlink and send the collection again." on A, and A's status line to read "Link to <B> is broken. Unlink and send the collection again." Unlink on A, send again with **Keep in sync**, and sync works again.
14. Pairing removed during a sync. Start a sync from A and, while the review dialog is open on B, remove the pairing on A. Expect the sync to end with no file changed, and the links of both sides to be unusable until a new pairing and a new send.
15. Size limit. Make changed files add up to just under 10 MiB and sync: it works in both directions. Add 1 byte more and expect the refusal before any dialog on B.
16. Crash during apply. Start a sync, accept on both sides, and kill Rocket on B during the apply (use a large change set). Start Rocket again. Expect the collection on B to be as before the sync or fully synced, never half merged, and the link to say "Out of date with <A>. Sync again." or "In sync". A second sync ends in "In sync".
17. Odd names. Link a collection named `Orders "v2"` and one with spaces and an accented letter. Expect the names to show unchanged in the menu, the toasts, the dialogs and the Audit log.
18. Audit. After the checks, expect on A one `Sent sync changes for ...` row per sync that moved a chunk, on B one `Received sync changes for ...` row, `Unlinked collection ...` rows for the unlink checks, outcomes in brackets only when the sync did not end with applied, and no file name or content in any row.
```

- [ ] **Step 11: Update the roadmap row and the docs index**

In `docs/roadmaps/roadmap.md`, in the R-83 row, after "manual checks open in section 11 of the manual test file." add " Keep a collection in sync is built (spec `docs/superpowers/specs/2026-11-08-team-network-sync-collection-design.md`), manual checks open in section 12." Do not change the status column text.

In `.claude/docs-index.md`, the line "`docs/features/team-network.md` for the team network (discovery, pairing, presence, Network panel, Settings section, send a collection, limits, manual checks)." gets ", keep in sync" after "send a collection". Run `git diff .claude/docs-index.md` first: if the file already has unrelated uncommitted edits, the commit must stage only this hunk with `git add -p` (ruling 14).

- [ ] **Step 12: Whole-plan verification**

Run:

```bash
bun run tsc --noEmit && bun run check && bun run test \
  src/lib/__tests__/tauri-api.network.test.ts src/lib/__tests__/network-utils.test.ts \
  src/lib/__tests__/network-sync-utils.test.ts src/lib/__tests__/network-sync-actions.test.ts \
  src/lib/__tests__/network-event-bridge.test.ts src/lib/__tests__/network-event-bridge.sync.test.ts \
  src/stores/__tests__/network-store.test.ts src/stores/__tests__/network-store.sync.test.ts \
  src/components/network src/components/status-bar src/components/settings \
  src/components/collections/__tests__ src/components/audit src/hooks/__tests__
```

Expected: `tsc` prints nothing, `check` reports no findings, the tests end with every listed file passed and 0 failed. If `check` reports format or import-order findings, run `bun run lint`, then rerun the command.

- [ ] **Step 13: No commit; reviewed at the plan end.**

Do not run `git add` or `git commit`. The user reviews the whole plan first. Then the commits go through `dev-workflow-skills:1-git-commit` with explicit paths, for example `feat(network): add the keep in sync checkbox, sync dialogs and link list`, `test(network): ...` and `docs(network): add keep in sync docs and manual checks`.

---

## Self-review

Spec sections covered:

- **Section 6 (flow):** the menu item and its online rule (Task 2e); the review dialog with counts, script and auth marks and the Safe sandbox warning (Task 2c); conflicts decided once by the planner with a diff and three choices (Task 2b, 2c); the confirm dialog of the starter with the same marks and no conflict choices (Task 2c); "Synced" only from the `applied` finish, which the backend sends after the peer's result (Task 1d).
- **Section 8 (errors):** every row has a fixed text in `describeSyncOutcome` and a table test (declined, expired, peer left through the `failed` message, base mismatch, link missing, unconfirmed, cancelled); no peer text and no file content is shown; the user cancelling the conflict dialog applies nothing (closing leaves the review pending, Decline sends `accept: false`).
- **Section 9 (interface):** Keep in sync checkbox off by default (Task 2a); Sync with, Unlink and the status line (Task 2e); incoming toast and dialog, starter confirm dialog (Task 2c, 2d); Cancel disabled once all bytes are in (Task 2c); conflict diff through the existing `DiffViewer`, read-only, only three choices change anything (Task 2b); links in the Network panel and Settings with Unlink (Task 3a); shadcn and lucide only.
- **Section 10 (audit):** three kinds with the wire fields, short id, outcome in brackets, no file names (Task 3b).
- **Section 12 (testing):** store, bridge and dialog tests with a late event for another sync (Tasks 1c, 1d, 2c); manual checks as section 12 of the manual file (Task 3c).
- **Index, Plan 5 consumes:** `startNetworkSync`, `reviewNetworkSync`, `confirmNetworkSync`, `cancelNetworkSync`, `listNetworkLinks`, `unlinkNetworkLink`, `onNetworkSync` and `sendNetworkCollection(deviceId, collection, includeEnvironments, keepInSync)` are all built with the exact names.

Open items for the final review (also in the rulings): Doubt A needs Plans 3 and 4 to add `network_sync_conflict_files` (and make the peer's version available before `ReviewReady`); Doubt B asks for progress of the `Files` payloads only; Doubt C asks Plan 1 to confirm camelCase helper DTOs.
