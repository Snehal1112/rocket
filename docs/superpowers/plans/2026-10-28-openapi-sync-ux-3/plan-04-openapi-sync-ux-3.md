# OpenAPI sync UX 3: Skipped items Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users can see which items they skipped, un-skip them, and learn in the import dialog that a file import does not sync.

**Architecture:** Two offline service methods over `local.skips` (names found by planning without skips, as `openapi_sync_plan` already does for `hidden`), two commands, a Skipped list in the Spec updates tab, and one sentence in the import dialog.

**Tech Stack:** Rust (`rocket-app`, `src-tauri`), React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 4)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-app/src/{openapi_sync_skips.rs (new),openapi_sync_skips_tests.rs (new),lib.rs}` | `SkippedRow`, `skipped`, `unskip` |
| `src-tauri/src/commands/openapi_sync.rs`, `src-tauri/src/lib.rs` | Commands, DTO |
| `src/lib/{tauri-api.ts,queries/openapi-sync-queries.ts}` | Wrappers, hooks |
| `src/components/collections/openapi-sync/{OpenApiSkippedList.tsx (new),OpenApiSyncList.tsx}`, `src/components/import/ImportCollectionDialog.tsx` | UI and note |

## Review Focus

1. Skipped and un-skip never contact the network. Pinned by `skipped_never_touches_the_network`.
2. Un-skip must raise `pending_count` so the badge matches. Pinned by `unskip_refreshes_the_pending_count`.
3. A skip whose operation left the spec must not break the list. Pinned by `a_skip_for_a_removed_operation_is_listed_by_key`.
4. Un-skip during an apply must refuse. Pinned by `unskip_refuses_while_an_apply_runs`.
5. The un-skipped item must reappear in the plan with its selection state fresh. Pinned by `an_unskipped_item_returns_to_the_list`.

---

### Task 1: Skipped list and unskip in the service

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Interfaces:**
- Consumes: `OpenApiLocalState.skips: BTreeMap<String, String>`, `plan_with_links(&sync_inputs(&prepared, &BTreeMap::new()), ..)` as in `openapi_sync_plan`, `FsOpenApiLocalRepo`, `pending_count` refresh, `collection_lock`, `recover_if_idle`.
- Produces: `SkippedRow { key: String, name: String, method: String, path: String }`; `OpenApiSyncService::skipped(&self, collection: &str) -> DomainResult<Vec<SkippedRow>>` (empty when nothing is pending; a key no longer in the spec is listed with `name = key` and empty method and path); `unskip(&self, collection: &str, keys: &[String]) -> DomainResult<usize>` returning the number removed.

- [ ] **Step 1: Read** `openapi_sync_plan` (the `hidden` computation), the local repo and how `pending_count` is written.
- [ ] **Step 2: Write the failing tests** (`openapi_sync_skips_tests.rs`): `skipped_lists_the_hidden_items_with_names`, `skipped_is_empty_when_nothing_is_pending`, `a_skip_for_a_removed_operation_is_listed_by_key`, `skipped_never_touches_the_network`, `unskip_removes_only_the_named_keys`, `unskip_refreshes_the_pending_count`, `unskip_refuses_while_an_apply_runs`, `an_unskipped_item_returns_to_the_list`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app skips`.
- [ ] **Step 4: Implement** both methods; `unskip` writes through the file transaction.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi_sync`, `cargo check -j4`. Expected: green.

### Task 2: Commands, wrappers and hooks

**Interfaces:**
- Consumes: Task 1; the pattern of `openapi_sync_changes` and `useOpenApiSyncChanges`.
- Produces: commands `openapi_sync_skipped(collection) -> Vec<SkippedRowDto>` and `openapi_sync_unskip(collection, keys) -> usize` (camelCase); TS `OpenApiSkippedRow`; `getOpenApiSyncSkipped(collection)`, `unskipOpenApiSync(collection, keys)`; `openApiSyncKeys.skipped(c)`; `useOpenApiSyncSkipped(collection, enabled)` and `useUnskip(collection)` (calls `invalidateSync` and the skipped key).

- [ ] **Step 1: Read** the changes command, DTO test and hook.
- [ ] **Step 2: Write the failing tests:** `skipped_dto_uses_camel_case_keys` (Rust); `skipped_and_unskip_call_the_commands`, `unskip_refreshes_plan_and_skipped_queries`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket skipped_dto`, `yarn test openapi-sync`.
- [ ] **Step 4: Implement** and register the commands in `lib.rs`.
- [ ] **Step 5: Run again.** `cargo check -j4`, `yarn tsc --noEmit`, `yarn check`. Expected: green.

### Task 3: Skipped list UI and the import-dialog note

**Interfaces:**
- Consumes: Task 2 hooks; `plan.skippedHidden`; `OpenApiSyncList.tsx`; the file-mode branch of `ImportCollectionDialog.tsx`.
- Produces: `OpenApiSkippedList({ collection }: { collection: string })` with rows (method, path or key, name) each with **Un-skip**, and **Un-skip all** with a count; the "N skipped, hidden from this list." line becomes a button that toggles it; in file mode of the import dialog, the line "A spec imported from a file is not kept in sync. Import from a link to get updates." (series 4 replaces it).

- [ ] **Step 1: Read** `OpenApiSyncList.tsx` (the skipped line) and the file-mode block of `ImportCollectionDialog.tsx`.
- [ ] **Step 2: Write the failing tests:** `the_skipped_line_opens_the_list`, `unskip_calls_the_hook_with_the_key`, `unskip_all_sends_every_key`, `the_list_shows_a_fixed_sentence_when_loading_fails`, `an_empty_list_shows_nothing_skipped`; in `ImportCollectionDialog.test.tsx` `file_mode_says_the_import_is_not_kept_in_sync` and `link_mode_does_not_show_the_note`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSkippedList`.
- [ ] **Step 4: Implement** the list, the toggle and the note.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync ImportCollectionDialog`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-3/plan-05-openapi-sync-ux-3.md
