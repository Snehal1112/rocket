# OpenAPI sync UX 2: View spec Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The user can open a read-only page that shows the latest stored spec offline, grouped by tag, with a header that says whether updates wait.

**Architecture:** `rocket-import` gains a public read model over `ParsedSpec`. `OpenApiSyncService::spec_view` picks the pending copy, else the applied copy, and marks new operations from the plan. A thin command returns a camelCase DTO. A viewer component replaces the inner tabs when the header's **View spec** button is pressed.

**Tech Stack:** Rust (`rocket-import`, `rocket-app`, `src-tauri`), React + TypeScript, react-query, shadcn/ui, lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md` (section 7)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-import/src/openapi/sync/spec.rs` | `ParsedSpec::title()`, `ParsedSpec::operation_views()`; types `SpecOperationView`, `SpecParamView`, `SpecResponseView` |
| `crates/rocket-app/src/{openapi_sync_view.rs,openapi_sync_spec_view.rs (new)}` | `SpecView`, `SpecTagGroup`, `OpenApiSyncService::spec_view` |
| `src-tauri/src/commands/openapi_sync.rs`, `src-tauri/src/lib.rs` | DTOs and command `openapi_sync_spec_view` |
| `src/lib/{tauri-api.ts,queries/openapi-sync-queries.ts}` | Types, wrapper, `useOpenApiSyncSpecView` |
| `src/components/collections/openapi-sync/{OpenApiSpecView.tsx (new),OpenApiSyncHeader.tsx,OpenApiSyncTab.tsx}` | Viewer, **View spec** button, view switch |

## Global Notes

- `doc()` stays `pub(crate)`; only the read model is public. The model holds names, paths, summaries, parameter names and response codes with their descriptions, never raw spec text blobs or examples.
- The command reads local files only (no network, no keychain, no consent check needed). A missing copy is not an error: it returns `state: 'no_copy'`.
- Fixtures: `world()` and `check_with(v2())` of `openapi_sync_apply_tests.rs`; after `check_with(v2())`, `createPet` and `getInventory` are new.
- Commands: `cargo test -j4 -p <crate> <filter>`, `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

---

### Task 1: ParsedSpec read model

**Interfaces:**
- Consumes: `ParsedSpec` (`spec.rs:9`), `Operation`, `Parameter`, `ParamLocation` (`model.rs`).
- Produces: `ParsedSpec::title(&self) -> &str`; `ParsedSpec::operation_views(&self) -> Vec<SpecOperationView>` in document order, with `SpecOperationView { key: String, method: String, path: String, tag: String, summary: Option<String>, parameters: Vec<SpecParamView>, responses: Vec<SpecResponseView> }`, `SpecParamView { name: String, location: String, required: bool }` (`location` is `path`, `query`, `header` or `cookie`), `SpecResponseView { status: String, description: String }`. `tag` is the first tag, or `"Untagged"`.

- [ ] **Step 1: Read** `model.rs` and how the reader keeps responses (if `Operation` drops them, extend the reader and `Operation` with a `responses: Vec<(String, String)>` field and update its tests).
- [ ] **Step 2: Write the failing tests** in `spec.rs`: `operation_views_list_method_path_tag_and_summary`, `an_operation_without_tags_goes_to_untagged`, `parameters_and_responses_are_listed` (a spec with a required path parameter and `200` / `404` responses), `title_returns_info_title`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import operation_views`. Expected: `no method named operation_views`.
- [ ] **Step 4: Implement** the accessors and types. Re-export the view types from the `rocket-import` public API like `ParsedSpec`.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`. Expected: green.

### Task 2: spec_view service and command

**Interfaces:**
- Consumes: Task 1, `load_pending` / `load_applied` (plan 01), `plan_locked` or `plan` for the new keys, `collection_lock`.
- Produces: `OpenApiSyncService::spec_view(&self, collection: &str) -> DomainResult<SpecView>` with `SpecView { title: String, version: Option<String>, fetched_at: Option<String>, state: SpecViewState, groups: Vec<SpecTagGroup> }`, `SpecViewState { InSync, UpdatesWaiting, NoCopy }`, `SpecTagGroup { tag: String, operations: Vec<SpecOpRow> }`, `SpecOpRow { key, method, path, summary, parameters, responses, is_new: bool }`. Command `openapi_sync_spec_view(collection) -> SpecViewDto` (camelCase: `fetchedAt`, `isNew`, state strings `in_sync`, `updates_waiting`, `no_copy`). `fetched_at` is the source's last successful fetch time. A copy that cannot be parsed returns a fixed `DomainError::InvalidInput("The saved spec cannot be shown.")`.

- [ ] **Step 1: Read** `openapi_sync_changes.rs` (`overview`, which already picks a spec version) and the DTO tests in `src-tauri/src/commands/openapi_sync.rs`.
- [ ] **Step 2: Write the failing tests** in `openapi_sync_spec_view_tests.rs` (new, `#[path]` module): `uses_the_applied_copy_when_nothing_waits` (state `InSync`, no operation `is_new`), `uses_the_pending_copy_when_present` (after `check_with(v2())`: `UpdatesWaiting`, `createPet` and `getInventory` are `is_new`, `listPets` is not), `reports_no_copy_for_an_old_import` (applied copy deleted, state `NoCopy`, empty groups), `never_touches_the_network` (the fetcher double records zero calls), `an_unparsable_copy_gives_a_fixed_sentence` (the message has no spec text). In `openapi_sync.rs`: `spec_view_dto_uses_camel_case_keys`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app spec_view`.
- [ ] **Step 4: Implement** `spec_view` (read under the collection lock so an apply never shows half a state) and the thin command; register it in `lib.rs`.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app spec_view`, `cargo check -j4`. Expected: green.

### Task 3: Viewer page

**Interfaces:**
- Consumes: Task 2 command, `useOpenApiSyncStatus`, `syncErrorMessage` (`syncErrors.ts`), `StateBlock`, `Badge`, `Input`, `Button`.
- Produces: types `OpenApiSpecView`, `OpenApiSpecOperation`; `getOpenApiSyncSpecView(collection)`; `openApiSyncKeys.specView(c)`; `useOpenApiSyncSpecView(collection)` (same options as the other hooks; the `useOpenApiSyncEvents` invalidation also covers it); `OpenApiSpecView({ collection, onBack }: { collection: string; onBack: () => void })`. Header text: "Showing the latest spec Rocket fetched." plus "Your collection is in sync with it." or "N updates are waiting." Search filters by method, path, key or summary. Each tag group lists the first 500 operations and a **Show more** button for the rest. Clicking an operation expands parameters and responses. The `OpenApiSyncHeader` **View spec** button (beside **Check now**) calls a new `onViewSpec` prop; `OpenApiSyncTab` shows the viewer instead of the inner tabs until **Back**.

- [ ] **Step 1: Read** `OpenApiSyncHeader.tsx`, `OpenApiSyncTab.tsx` (tab switching, `settled`) and `OpenApiSyncOverview.test.tsx` for the mock pattern.
- [ ] **Step 2: Write the failing tests** (`OpenApiSpecView.test.tsx`): `groups_operations_by_tag`, `says_whether_updates_wait`, `marks_new_operations`, `search_filters_the_rows`, `lists_500_per_tag_then_show_more` (600 rows: 500 visible, **Show more** reveals the rest), `expanding_a_row_shows_parameters_and_responses`, `no_copy_shows_no_saved_copy_yet`, `shows_a_fixed_sentence_when_loading_fails` (rejected text with a path is absent). In `OpenApiSyncHeader.test.tsx`: `view_spec_calls_the_handler`. In `OpenApiSyncTab.test.tsx`: `view_spec_opens_the_viewer_and_back_returns_to_the_same_tab`. In `tauri-api.openapi-sync.test.ts`: `spec_view_calls_openapi_sync_spec_view_with_the_collection`.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSpecView`. Expected: module not found.
- [ ] **Step 4: Implement** the types, wrapper, hook, viewer, header button and tab switch. Switching to the viewer must not unmount the connection dialog state when a dialog is open.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

## Review Focus

1. A very large spec must not render thousands of rows at once. Pinned by `lists_500_per_tag_then_show_more`.
2. An old import with no saved copy must say so, not show an empty list or an error. Pinned by `reports_no_copy_for_an_old_import` and `no_copy_shows_no_saved_copy_yet`.
3. Opening View spec must never fetch. Pinned by `never_touches_the_network`.
4. A broken saved copy must give a fixed sentence, never spec text. Pinned by `an_unparsable_copy_gives_a_fixed_sentence`.
5. Spec text must render as plain text (a summary holding `<script>` or markdown stays inert). Add `summary_text_is_rendered_as_plain_text` to the viewer tests.

Next plan: docs/superpowers/plans/2026-10-27-openapi-sync-ux-2/plan-03-openapi-sync-ux-2.md
