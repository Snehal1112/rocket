# OpenAPI sync UX 5: Delete actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From the sync page a user can delete a request they added locally and a request the spec removed but they had edited, like Bruno's Delete and Delete All Local Endpoints, with a confirm that names the request.

**Architecture:** One service method `delete_requests` removes request files through the file transaction under the collection lock, re-checking each file's uid first, and drops the matching source operation when there is one. A thin command, a hook, and confirm dialogs in Your changes and in the Spec updates detail.

**Tech Stack:** Rust (`rocket-app`, `src-tauri`), React + TypeScript, shadcn `AlertDialog`, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 8); parity rows "Local-only requests" and "Removed from spec".

## File Structure

| File | Change |
|---|---|
| `crates/rocket-app/src/{openapi_sync_delete.rs (new),openapi_sync_delete_tests.rs (new),lib.rs}` | `DeleteTarget`, `delete_requests` |
| `src-tauri/src/commands/openapi_sync.rs`, `src-tauri/src/lib.rs` | `openapi_sync_delete_requests` |
| `src/lib/{tauri-api.ts,queries/openapi-sync-queries.ts}`, `syncErrors.ts` | Wrapper, `useDeleteRequests`, sentences |
| `src/components/collections/openapi-sync/{OpenApiSyncChanges.tsx,DeleteRequestsDialog.tsx (new),OpenApiSyncDetail.tsx}` | Buttons and confirm |

## Review Focus

1. A file replaced after the page loaded (different uid) must not be deleted. Pinned by `a_changed_uid_is_refused`.
2. A path that escapes the collection (`../x`, absolute) must be refused. Pinned by `a_path_outside_the_collection_is_refused`.
3. A request that is still synced and unedited must not be deletable here. Pinned by `a_synced_unedited_request_is_refused`.
4. During an apply the command must refuse. Pinned by `delete_refuses_while_an_apply_runs`.
5. Deleting an edited spec-removed request must also drop its source operation so it is not reported again. Pinned by `deleting_a_removed_request_drops_its_source_operation`.
6. A failed transaction must leave every file. Pinned by `a_failed_delete_leaves_all_files`.

---

### Task 1: Service method

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Interfaces:**
- Consumes: `changes()` (added rows have `key: None`), `OpenApiSource.operations`, `CollectionFileTx` (`deps.tx`), `collection_lock`, `recover_if_idle`, the request reader that returns the uid of a file.
- Produces: `DeleteTarget { uid: String, request_path: String }`; `OpenApiSyncService::delete_requests(&self, collection: &str, targets: &[DeleteTarget]) -> DomainResult<usize>`. Allowed targets: a request whose uid is not in `source.operations` (added by you), or whose operation is no longer in the pending/applied spec and whose request was edited (hash differs from `baseline_hash`). Anything else gives the fixed error `NOT_DELETABLE` ("This request is still part of the spec. Remove it from the spec first."). A uid mismatch gives `REQUEST_CHANGED` ("The request changed on disk. Reload the page."). The whole list is one transaction.

- [ ] **Step 1: Read** `changes()`, the reset service (for the uid re-check and the transaction style) and `apply`'s removal code (`apply.rs:409-440`).
- [ ] **Step 2: Write the failing tests** (`openapi_sync_delete_tests.rs`): `deleting_a_request_added_by_you_removes_the_file`, `deleting_an_edited_removed_request_removes_the_file`, `deleting_a_removed_request_drops_its_source_operation`, `a_synced_unedited_request_is_refused`, `a_changed_uid_is_refused`, `a_path_outside_the_collection_is_refused`, `delete_refuses_while_an_apply_runs`, `a_failed_delete_leaves_all_files`, `delete_never_touches_the_network`, `the_baseline_entry_is_dropped_with_the_operation`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app openapi_sync_delete`. Expected: module not found.
- [ ] **Step 4: Implement** the method.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi_sync`, `cargo check -j4`. Expected: green.

### Task 2: Command, wrapper and hook

**Interfaces:**
- Consumes: Task 1; the reset command and its hook (`useResetRequests`, `refreshAfterReset`) as the pattern.
- Produces: command `openapi_sync_delete_requests(collection, targets: Vec<DeleteTargetDto { uid, requestPath }>) -> usize`; `deleteOpenApiRequests(collection, targets)`; `useDeleteRequests(collection)` that calls `refreshAfterReset` (collection tree plus sync queries); `KNOWN` entries for the two sentences.

- [ ] **Step 1: Read** the reset command, hook and `syncErrors.test.ts`.
- [ ] **Step 2: Write the failing tests:** `delete_dto_uses_camel_case_keys`, `delete_errors_pass_only_fixed_sentences` (Rust); `delete_wrapper_calls_the_command`, `delete_refreshes_the_collection_and_sync_queries`, `syncerrors_maps_the_delete_sentences`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket delete_dto`, `yarn test openapi-sync`.
- [ ] **Step 4: Implement** and register the command.
- [ ] **Step 5: Run again.** `cargo check -j4`, `yarn tsc --noEmit`, `yarn check`. Expected: green.

### Task 3: Buttons and confirm

**Interfaces:**
- Consumes: Task 2 hook; "Added by you" group in `OpenApiSyncChanges.tsx`; the removed-item detail in `OpenApiSyncDetail.tsx` (item kind `removed` with `edited: true`); shadcn `AlertDialog`; series 3 plan 02's `openSyncRequest` for the Open button beside Delete.
- Produces: `DeleteRequestsDialog({ requests, open, onConfirm, onCancel, busy })` with the text `Delete "<METHOD> <path or name>"? This removes the file from the collection. The sync page cannot undo it, git can.` (for several: `Delete N requests?`); **Delete** on each added row and **Delete all (N)** on the group; **Delete this request** in the detail of an edited removed request; after success a toast "Deleted." and the lists refresh.

- [ ] **Step 1: Read** both components and the Reset confirm for the dialog pattern.
- [ ] **Step 2: Write the failing tests:** `delete_on_an_added_row_asks_before_deleting`, `confirm_calls_the_hook_with_uid_and_path`, `cancel_deletes_nothing`, `delete_all_sends_every_added_row`, `a_removed_edited_request_offers_delete_in_its_detail`, `an_unedited_removed_request_offers_no_delete`, `a_failed_delete_shows_a_fixed_sentence`, `the_buttons_are_disabled_while_deleting`.
- [ ] **Step 3: Run to see them fail.** `yarn test DeleteRequestsDialog OpenApiSyncChanges`.
- [ ] **Step 4: Implement** the dialog and buttons.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync CollectionNode`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-5/plan-03-openapi-sync-ux-5.md
