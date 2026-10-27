# OpenAPI sync UX 3: Values and Open request Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users see what they changed and what a new endpoint looks like, and can open the request from the sync page.

**Architecture:** `changes()` keeps the `FieldChange` values it already computes; the plan DTO keeps a name-only preview of `SyncItem::New.request`. The UI shows them in expandable rows and a shared helper opens request tabs.

**Tech Stack:** Rust (`rocket-app`, `src-tauri`), React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 2)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-app/src/{openapi_sync_view.rs,openapi_sync_changes.rs,openapi_sync_changes_tests.rs}` | `ChangeValue`, `ChangeRow.changes` |
| `src-tauri/src/commands/openapi_sync.rs` | `ChangeValueDto`, `NewPreviewDto`, `SyncItemDto::New.preview` |
| `src/lib/tauri-api.ts` | Types |
| `src/components/collections/openapi-sync/{openSyncRequest.ts (new),OpenApiSyncChanges.tsx,OpenApiSyncDetail.tsx,OpenApiSyncList.tsx}` | Values, preview, Open request |

## Review Focus

1. Header and query values from the spec must never reach the UI preview (names only). Pinned by `new_preview_lists_names_not_values`.
2. Values stay clipped at 500 characters. Pinned by `long_values_are_clipped`.
3. Without a trusted baseline no values show. Pinned by `no_values_without_a_trusted_baseline`.
4. Open request on a request deleted from disk shows a fixed sentence, not a crash. Pinned by `open_request_reports_a_missing_file`.
5. Values render as plain text. Pinned by `values_render_as_plain_text`.

---

### Task 1: Values and preview in the read models

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Interfaces:**
- Consumes: `changes()` (`openapi_sync_changes.rs`), `merge_request` and `FieldChange { field, baseline, yours, spec }` (`merge.rs`), `item_dto` and `SyncItem::New { request, .. }`.
- Produces: `ChangeValue { field: String, baseline: Option<String>, yours: Option<String> }` and `ChangeRow.changes: Vec<ChangeValue>` (empty when the baseline is untrusted; `fields` stays). `NewPreviewDto { method: String, url: String, headers: Vec<String>, query: Vec<String>, has_body: bool, auth_kind: Option<String> }` on `SyncItemDto::New.preview`. `yours` is the user's request, passed to `merge_request` as the incoming side, so read it from the `spec` slot of the change.

- [ ] **Step 1: Read** `changes()` and the `merge_request` call that produces `fields`, plus `item_dto`.
- [ ] **Step 2: Write the failing tests:** in `openapi_sync_changes_tests.rs` `modified_rows_carry_baseline_and_yours_values`, `long_values_are_clipped`, `no_values_without_a_trusted_baseline`; in `openapi_sync.rs` `new_preview_lists_names_not_values`, `new_preview_has_body_and_auth_kind`, `change_value_dto_uses_camel_case_keys`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app modified_rows_carry`, `cargo test -j4 -p rocket new_preview`.
- [ ] **Step 4: Implement** the fields and DTOs.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi_sync`, `cargo check -j4`. Expected: green.

### Task 2: Show values and the preview

**Interfaces:**
- Consumes: Task 1 DTOs, `fieldLabel`, the `Value` and `FieldBlock` components in `OpenApiSyncDetail.tsx`.
- Produces: TS types `OpenApiChangeValue`, `OpenApiNewPreview`; rows in Your changes expand to show each changed field as Baseline / Yours (reusing `FieldBlock`); a New item's detail shows method, URL, header names, query names, "Has a body" and the auth kind instead of only the key and folder.

- [ ] **Step 1: Read** `OpenApiSyncChanges.tsx`, `OpenApiSyncDetail.tsx` (`FieldBlock`, `Value`) and the fixtures.
- [ ] **Step 2: Write the failing tests:** `modified_rows_expand_to_show_baseline_and_yours`, `values_render_as_plain_text` (a value holding `<script>`), `rows_without_values_do_not_expand`, `a_new_item_shows_its_preview` (`OpenApiSyncDetail.test.tsx`); extend `makeChanges` and `newItem` fixtures.
- [ ] **Step 3: Run to see them fail.** `yarn test OpenApiSyncChanges`.
- [ ] **Step 4: Implement** the types, expansion and preview.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 3: Open request

**Interfaces:**
- Consumes: `getRequest(collection, path)`, `mapApiRequestToState(full, true)`, `usePaneStore.getState().openTab`, `findTabInTree`, `RequestNode.createTab`/`openInPane` as the pattern.
- Produces: `openSyncRequest(args: { collection: string; uid: string; requestPath: string }): Promise<'opened' | 'missing'>` in `openSyncRequest.ts`; an **Open request** button on modified rows and on "Added by you" rows. A focused existing tab is reused. A failed `getRequest` returns `'missing'` and the row shows "The request file was not found."

- [ ] **Step 1: Read** `RequestNode.tsx:175-232`, `pane-store.ts:467` and confirm that `get_request` accepts the extension-less `request_path` (if not, add the extension in the helper and pin it with a test).
- [ ] **Step 2: Write the failing tests:** `openSyncRequest.test.ts` (`opens_a_new_tab_for_the_request`, `focuses_an_open_tab_instead_of_duplicating`, `open_request_reports_a_missing_file`); component test `open_request_button_calls_the_helper`.
- [ ] **Step 3: Run to see them fail.** `yarn test openSyncRequest`.
- [ ] **Step 4: Implement** the helper and buttons.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-3/plan-03-openapi-sync-ux-3.md
