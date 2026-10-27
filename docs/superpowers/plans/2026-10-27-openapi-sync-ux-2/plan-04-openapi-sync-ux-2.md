# OpenAPI sync UX 2: Reset UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Your changes lets the user reset modified requests to the spec, and restore or discard the backup in a Reset by you group.

**Architecture:** TypeScript wrappers and mutations for the three commands of plan 03 (each invalidates the same keys as the apply mutation). `OpenApiSyncChanges` gains per-row **Reset to spec**, a **Reset all to spec** button, a confirm dialog with the exact spec text, and the Reset by you group.

**Tech Stack:** React + TypeScript, react-query, shadcn/ui (Button, Badge, AlertDialog, StateBlock), lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md` (section 6)

## File Structure

| File | Change |
|---|---|
| `src/lib/tauri-api.ts` | `OpenApiBackupRow`; `backups`, `resetAvailable` on `OpenApiChanges`; `resetOpenApiSync`, `restoreOpenApiSync`, `discardOpenApiSyncBackup` |
| `src/lib/queries/openapi-sync-queries.ts` | `useResetOpenApiSync`, `useRestoreOpenApiSync`, `useDiscardOpenApiBackup` |
| `src/components/collections/openapi-sync/{OpenApiSyncChanges.tsx,ResetConfirmDialog.tsx (new),openApiSyncView.ts}` | Buttons, dialog, group, `resetConfirmText` |
| tests | `tauri-api.openapi-sync.test.ts`, `openapi-sync-queries.test.tsx`, `OpenApiSyncChanges.test.tsx`, `ResetConfirmDialog.test.tsx`, `openApiSyncView.test.ts` |

## Global Notes

- DTO mirror (plan 03): `OpenApiChanges.backups: BackupRowDto[]` with `{ key, name, method, url, requestPath, savedAt, fields: string[] }`, `resetAvailable: boolean`; `ResetReportDto { reset: string[]; skipped: string[] }`. Commands `openapi_sync_reset { collection, keys }`, `openapi_sync_restore { collection, key }`, `openapi_sync_discard_backup { collection, key }`.
- Plan 03 of series 1 (the earlier series) pinned `has_no_buttons_in_this_version`; this plan replaces it with `added_and_deleted_rows_have_no_buttons`.
- shadcn/ui and lucide-react only; no raw backend text on screen (use `syncErrorMessage`; add the four reset sentences of plan 03 to its allow list).
- Commands: `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

---

### Task 1: API wrappers and mutations

**Interfaces:**
- Consumes: `useApplyOpenApiSync` (invalidation pattern), `openApiSyncKeys`.
- Produces: the three wrappers (exact command names and args above) and mutation hooks. On settle each invalidates `changes(c)`, `overview(c)`, `plan(c)`, `status(c)` and `badges`; `useResetOpenApiSync` takes `{ collection, keys }`.

- [ ] **Step 1: Read** `useApplyOpenApiSync` and its test.
- [ ] **Step 2: Write the failing tests:** `reset_calls_openapi_sync_reset_with_collection_and_keys`, `restore_calls_openapi_sync_restore`, `discard_calls_openapi_sync_discard_backup`; `a_reset_refreshes_changes_overview_plan_status_and_badges`; `a_failed_reset_still_refreshes`.
- [ ] **Step 3: Run to see them fail.** `yarn test openapi-sync`.
- [ ] **Step 4: Implement** types, wrappers, hooks.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`. Expected: green.

### Task 2: Reset buttons and confirm

**Interfaces:**
- Consumes: Task 1, `fieldLabel`, `AlertDialog` primitives as used by `ApplyConfirmDialog.tsx`.
- Produces: `resetConfirmText(row: { method: string; url: string; fields: string[] }): string` in `openApiSyncView.ts` returning exactly: "Reset GET /orders to the spec? URL and header Accept go back to the spec's version. Your scripts, tests, assertions and docs are kept. A backup is kept until the next sync." (field labels joined with commas and "and", from `fieldLabel`); `ResetConfirmDialog({ open, title, body, count, busy, onConfirm, onCancel })`. Each Modified row gets a **Reset to spec** button; the group heading gets **Reset all to spec (n)**, whose confirm names the count ("Reset 3 requests to the spec? ...") and the same kept-fields and backup sentences. When `resetAvailable` is false the buttons are disabled and the note "These requests cannot be reset because the sync data is from an older version." shows. Added and deleted rows keep no button. While a reset runs, the buttons are disabled; an error shows a fixed sentence in an `Alert`.

- [ ] **Step 1: Read** `OpenApiSyncChanges.tsx`, `ApplyConfirmDialog.tsx` and `fieldLabel`.
- [ ] **Step 2: Write the failing tests:** `openApiSyncView.test.ts`: `reset_confirm_text_names_the_method_url_and_fields` (exact string above). `OpenApiSyncChanges.test.tsx`: `modified_rows_have_a_reset_button`, `added_and_deleted_rows_have_no_buttons`, `reset_asks_first_and_cancel_sends_nothing`, `confirming_a_reset_sends_that_one_key`, `reset_all_asks_once_with_the_count_and_sends_all_keys`, `reset_is_disabled_with_a_note_when_unavailable`, `shows_a_fixed_sentence_when_the_reset_fails`, `the_buttons_are_disabled_while_a_reset_runs`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncChanges`.
- [ ] **Step 4: Implement** the dialog, helper, buttons and error handling.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 3: Reset by you group

**Interfaces:**
- Consumes: Tasks 1 and 2.
- Produces: a group **Reset by you (n)** under Modified, shown only when `backups` is not empty. A row shows the method, URL or path, the backed-up field names, "Saved <relative time>", and two buttons: **Restore my version** and **Discard backup**. Restore runs at once and shows "Your version is back." in a toast; Discard asks "Discard the backup of GET /orders? You cannot restore it afterwards." first. The tab label `Your changes (n)` counts modified + added + deleted + backups.

- [ ] **Step 1: Read** the tab label code in `OpenApiSyncTab.tsx` and the toast usage in the sync files.
- [ ] **Step 2: Write the failing tests:** `shows_the_reset_by_you_group_only_with_backups`, `restore_my_version_sends_the_key_and_toasts`, `discard_asks_first`, `discard_confirm_sends_the_key`, `backups_show_when_the_request_is_no_longer_modified` (the common case right after a reset), `the_tab_label_counts_backups` (in `OpenApiSyncTab.test.tsx`).
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncChanges`.
- [ ] **Step 4: Implement** the group, buttons and label.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

## Review Focus

1. A reset must always ask first, and the exact confirm text must name the fields that go back. Pinned by `reset_asks_first_and_cancel_sends_nothing` and the text test.
2. Discard is permanent, so it must ask; Restore is safe and must not. Pinned by `discard_asks_first` and `restore_my_version_sends_the_key_and_toasts`.
3. After a reset the request is no longer "modified", but its backup must stay reachable. Pinned by `backups_show_when_the_request_is_no_longer_modified`.
4. Reset must be impossible when the sync data is old. Pinned by `reset_is_disabled_with_a_note_when_unavailable`.
5. Added and deleted rows must offer no destructive action. Pinned by `added_and_deleted_rows_have_no_buttons`.

Next plan: docs/superpowers/plans/2026-10-27-openapi-sync-ux-2/plan-05-openapi-sync-ux-2.md
