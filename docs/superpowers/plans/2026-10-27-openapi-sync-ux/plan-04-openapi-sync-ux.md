# OpenAPI sync UX: bulk review and notifications Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The Spec updates tab works bulk-first (safe items preselected, Accept all, Skip all, Keep current, Confirm and sync), and users learn about waiting updates through a sidebar badge and a one-time toast.

**Architecture:** The existing selection hook gains default selection rules and three bulk actions; the existing confirm and result dialogs are reused. A global listener (mounted once in `App.tsx`) keeps the badge list fresh and shows the toast, deduplicated by `pendingId` in `localStorage`.

**Tech Stack:** React + TypeScript, react-query, Zustand, sonner, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md`

## File Structure

| File | Change |
|---|---|
| `src/components/collections/openapi-sync/useOpenApiSyncSelection.ts` (was `useSyncSelection.ts`) | Default selection, `acceptAll`, `keepCurrent`, `skipAll` |
| `.../OpenApiSyncActionsBar.tsx` | Buttons Accept all, Skip all, Keep current, Confirm and sync (N); summary line |
| `.../BulkConfirmDialog.tsx` (new) | Confirms Accept all (overwrite warning) and Skip all |
| `.../OpenApiSyncTab.tsx` | Wire the bar and dialogs |
| `src/components/collections/openapi-sync/notifyUpdates.ts` (new) | Pure `shouldNotify` + storage helpers |
| `src/components/collections/openapi-sync/OpenApiSyncNotifier.tsx` (new) | Global listener, badge refresh, toast |
| `src/components/collections/CollectionNode.tsx` | Count badge on the collection row |
| `src/App.tsx` | Mount `OpenApiSyncNotifier` |

## Global Notes

- shadcn/ui and lucide-react only; narrow Zustand selectors; strings inline English; badge is a `span role="button"` (not a nested `<button>`), as `ContractBadge` does in `CollectionNode.tsx:424-431`.
- Rules fixed by the spec: preselect every New item and every Changed item without a Conflict field; Changed items with a Conflict start unselected with "keep yours"; a Removed item is never preselected (an edited one is never deletable; an unedited one needs a deliberate click and is named in the confirm); **Accept all** selects every New and Changed item and takes the spec on every Conflict field after a warning, and never selects an edited Removed item; **Skip all** skips every item after a confirm; **Keep current** clears the selection and sends nothing.
- Tests: `renderWithQuery`, `vi.mock('@/lib/tauri-api', ...importActual...)`, fixtures `makeStatus`, `newItem`, plus `changedItem`, `conflictItem`, `removedItem` (add what is missing to `fixtures.ts`), `clearApplyState()` in `afterEach`, `vi.mock('sonner')` for toasts. No 3000-row fixtures. Commands: `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

---

### Task 1: Default selection and bulk actions

**Interfaces:**
- Consumes: `useOpenApiSyncSelection(plan, links, setLinks)` (`useSyncSelection.ts:67`), `buildSelection`, `ApplyConfirmDialog`, `ApplyResultDialog`, `useApplySync`, `OpenApiSyncActionsBar`.
- Produces: the hook also returns `acceptAll(): { overwritten: number }`, `keepCurrent(): void`, `skipAll(): void` and `selectedCount: number`; on every new plan signature it resets to the default selection (same moment it clears approvals today). `OpenApiSyncActionsBar` shows "N updates from the spec · M selected" and the four buttons; **Confirm and sync (M)** is disabled at 0. `BulkConfirmDialog({ kind: 'accept' | 'skip'; count: number; overwritten?: number; onConfirm; onCancel })`.

- [ ] **Step 1: Read** `useSyncSelection.ts` (now `useOpenApiSyncSelection.ts`), `buildSelection.ts`, `OpenApiSyncActionsBar.tsx`, `OpenApiSyncTab.tsx` (`confirmApply`, `skipSelected`) and `useSyncSelection.test.ts`, `SpecSyncTab.test.tsx` (now `OpenApiSyncTab.test.tsx`).
- [ ] **Step 2: Write the failing tests.**
  - `useOpenApiSyncSelection.test.ts`: `preselects_new_and_changed_items_without_conflicts` (plan: 1 new, 1 changed, 1 changed with a conflict field, 1 removed edited false, 1 removed edited true; selected keys are the first two), `a_conflict_item_starts_unselected_and_keeps_yours` (its `choices` map holds `'yours'` for the conflict field), `removed_items_are_never_preselected`, `accept_all_selects_new_and_changed_and_takes_the_spec_on_conflicts` (returns `overwritten: 1`, the conflict field choice becomes `'spec'`), `accept_all_never_selects_a_removed_item_the_user_edited`, `keep_current_clears_the_selection`, `skip_all_marks_every_item_skipped_and_selects_nothing`, `a_new_plan_resets_to_the_defaults` (change a digest; defaults return even after manual toggles), `toggling_inside_one_plan_is_kept` (re-render with the same signature keeps the manual toggle).
  - `OpenApiSyncActionsBar.test.tsx`: `shows_the_summary_and_the_four_buttons`, `confirm_is_disabled_with_nothing_selected`, `confirm_shows_the_selected_count`.
  - `OpenApiSyncTab.test.tsx` additions: `confirm_and_sync_opens_the_confirm_dialog_with_counts` (2 selected: counts match), `accept_all_warns_before_overwriting_edits` (dialog text "1 of your edits will be overwritten"; Cancel changes nothing), `accept_all_without_conflicts_needs_no_warning` (selects directly), `skip_all_asks_first_and_then_sends_skips_with_fingerprints` (one `applyOpenApiSync` call whose selection has `items: []` and `skips` for every item, `renameLinks: []`), `keep_current_sends_nothing` (no `applyOpenApiSync` call).
- [ ] **Step 3: Run to see them fail.** `yarn test useOpenApiSyncSelection` and `yarn test OpenApiSyncTab`. Expected: `acceptAll is not a function` and missing button errors.
- [ ] **Step 4: Implement** the default rules inside the existing signature-reset branch, the three actions, the bar, `BulkConfirmDialog`, and the wiring. Skip all reuses the existing skips-only apply path (`skipSelected`) with every item.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 2: Global listener and sidebar badge

**Interfaces:**
- Consumes: Plan 03 `useOpenApiSyncBadges`, `onOpenApiSyncChecked`, `onOpenApiSyncApplied`, `openApiSyncKeys`, `openOpenApiSyncTab`, `CollectionNode.tsx`.
- Produces: `useOpenApiSyncGlobalEvents(): void` (in `openapi-sync-queries.ts`): one pair of listeners that invalidates `openApiSyncKeys.badges` on every event and the status and plan keys of the named collection; `OpenApiSyncNotifier` mounts it. A count badge `N` next to the collection name in `CollectionNode` (hidden at 0), tooltip "N OpenAPI spec updates", `span role="button"` with `aria-label="Open OpenAPI sync, N updates"` that opens the tab and does not toggle the row.

- [ ] **Step 1: Read** `CollectionNode.tsx:420-440` (`ContractBadge`), `useOpenApiSyncEvents` and `App.tsx` near `<Toaster>` (`:271`).
- [ ] **Step 2: Write the failing tests.**
  - `openapi-sync-queries.test.tsx`: `global_events_refresh_the_badge_list_for_any_collection`, `global_events_refresh_status_and_plan_only_for_the_named_collection`, `the_listeners_are_removed_on_unmount` (both unlisten functions called, also when unmount happens before the listen promises resolve).
  - `CollectionNode.openapi-sync.test.tsx`: `shows_the_waiting_count_badge` (badges `[{ collection: 'Orders', waiting: 5 }]` -> "5" visible), `hides_the_badge_at_zero_and_for_other_collections`, `clicking_the_badge_opens_the_tab_and_keeps_the_row_closed` (`openOpenApiSyncTab('Orders')` called; the row's expanded state unchanged), `the_badge_is_keyboard_reachable` (Enter on the badge opens the tab).
  - `OpenApiSyncNotifier.test.tsx`: `mounts_the_global_listeners_once`.
- [ ] **Step 3: Run to see them fail.** `yarn test CollectionNode` and `yarn test OpenApiSyncNotifier`. Expected: missing hook and component errors.
- [ ] **Step 4: Implement** the global hook, the notifier component (renders `null`), mount it in `App.tsx`, and the badge in `CollectionNode`.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`, `yarn test CollectionNode`. Expected: green.

### Task 3: One-time toast

**Interfaces:**
- Consumes: Task 2 notifier, `getOpenApiSyncStatus`, `pendingId` on the status, `toast` from `sonner`, `openOpenApiSyncTab`.
- Produces: `shouldNotify(store: Pick<Storage, 'getItem' | 'setItem'>, collection: string, pendingId: string): boolean` (true once per `collection:pendingId`; the set is stored in `localStorage` under `rocket.openapi-sync.notified`, capped at 200 entries; a storage error returns false and never throws); in `OpenApiSyncNotifier`, on an `openapi-sync-checked` event with `status === 'changed'`: fetch the status (without token), and when `consent === 'granted'`, `hasPending`, `pendingId` is set, `shouldNotify` is true and the OpenAPI sync tab for that collection is not the active tab, show `toast.info("{Collection} has {N} spec updates", { action: { label: 'Review', onClick: () => openOpenApiSyncTab(collection) } })`, where N comes from the refreshed badge list.

- [ ] **Step 1: Read** how `toast` is used (`ScriptNode.tsx:44`, `CollectionsSidebar.tsx:155`) and the tests that mock `sonner`.
- [ ] **Step 2: Write the failing tests.**
  - `notifyUpdates.test.ts`: `notifies_once_per_pending_id`, `a_new_pending_id_notifies_again`, `different_collections_do_not_share_ids`, `survives_a_throwing_storage` (getItem throws -> false, no exception), `caps_the_stored_ids` (201 ids keep 200, the oldest dropped).
  - `OpenApiSyncNotifier.test.tsx`: `shows_a_toast_for_a_changed_check` (text "Orders has 5 spec updates", action "Review"), `does_not_toast_twice_for_the_same_pending_id`, `does_not_toast_for_unchanged_or_failed_events`, `does_not_toast_without_consent` (status consent `missing`), `does_not_toast_when_the_tab_is_active`, `the_review_action_opens_the_tab`.
- [ ] **Step 3: Run to see them fail.** `yarn test notifyUpdates` and `yarn test OpenApiSyncNotifier`. Expected: module not found.
- [ ] **Step 4: Implement** `notifyUpdates.ts` and the toast logic in the notifier.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

## Review Focus

1. Accept all must never delete a Removed request the user edited. Pinned by `accept_all_never_selects_a_removed_item_the_user_edited`.
2. Accept all must warn with an exact overwrite count before taking the spec on conflicts. Pinned by `accept_all_warns_before_overwriting_edits`.
3. Skip all and Keep current must not be confusable: Skip all hides items until the spec changes (asks first), Keep current changes nothing. Pinned by `skip_all_asks_first_and_then_sends_skips_with_fingerprints` and `keep_current_sends_nothing`.
4. A manual toggle must survive a re-render of the same plan but not a new plan. Pinned by `toggling_inside_one_plan_is_kept` and `a_new_plan_resets_to_the_defaults`.
5. A broken `localStorage` must not break notifications or throw. Pinned by `survives_a_throwing_storage`.
6. The toast must not appear for a collection the user is already looking at, and never twice for one spec version. Pinned by `does_not_toast_when_the_tab_is_active` and `does_not_toast_twice_for_the_same_pending_id`.
7. The badge must not toggle the collection row or nest a button. Pinned by `clicking_the_badge_opens_the_tab_and_keeps_the_row_closed`.

Next plan: none
