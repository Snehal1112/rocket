# OpenAPI sync UX 4: Local file UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users can pick a spec file in the import dialog, Connect and Change link, see where the spec comes from, and get clear messages when the file is missing.

**Architecture:** Frontend on top of plan 02. A shared `SpecSourceField` (Link/File switch plus file picker) replaces the link-only inputs; display helpers learn the `file://` form; the interim "not kept in sync" note from series 3 is replaced by a checkbox.

**Tech Stack:** React + TypeScript, shadcn/ui, lucide-react, Tauri file dialog (`openFilePicker`), Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 7)

## File Structure

| File | Change |
|---|---|
| `src/components/collections/openapi-sync/{SpecSourceField.tsx (new),ConnectOpenApiDialog.tsx,ChangeLinkForm.tsx,openApiSyncView.ts,syncErrors.ts}` | Field, display helpers, sentence |
| `src/components/import/ImportCollectionDialog.tsx` | Keep-in-sync checkbox for file mode |
| `src/lib/tauri-api.ts`, `src/components/collections/openapi-sync/{OpenApiSyncHeader.tsx,OpenApiSyncOverview.tsx,OpenApiSyncConnectionDialog.tsx}` | `keepSynced`, local-file display |
| `docs/features/openapi-sync-ux.md`, `docs/roadmaps/roadmap.md`, `crates/rocket-import/CLAUDE.md` | Docs, manual checks 22 to 26 |

## Review Focus

1. The picked path must be shown, but a file path must never appear in an error text. Pinned by `errors_for_a_file_never_show_the_path`.
2. A token field must not show for a file. Pinned by `no_token_field_for_a_file`.
3. A cloned collection must show "Needs approval" before any read. Pinned by `a_file_source_needs_approval_on_a_new_machine`.
4. A missing file must offer **Choose another file** (Change link), not only Check now. Pinned by `a_missing_file_offers_choose_another_file`.
5. Link mode of the import dialog must behave exactly as before. Pinned by `link_mode_is_unchanged`.

---

### Task 1: Import dialog checkbox and API

**Interfaces:**
- Consumes: `ImportCollectionDialog.tsx` (file mode, the "not kept in sync" note from series 3 plan 04), `importOpenApiFile` wrapper, the link mode's "Check for spec updates" checkbox as the pattern.
- Produces: `importOpenApiFile(path, groupBy, workspaceId, keepSynced?)`; in file mode a checkbox **Keep in sync with this file** (default on) with the line "Rocket checks the file every 5 minutes while the app is open."; the series 3 note is removed; the result screen shows the warnings from the command.

- [ ] **Step 1: Read** the dialog's file-mode block and `importOpenApiFile`.
- [ ] **Step 2: Write the failing tests:** `file_mode_shows_the_keep_in_sync_checkbox_on_by_default`, `unchecking_it_sends_keep_synced_false`, `the_old_not_kept_in_sync_note_is_gone`, `link_mode_is_unchanged`, `the_wrapper_passes_keep_synced`.
- [ ] **Step 3: Run to see them fail.** `yarn test ImportCollectionDialog tauri-api.import`.
- [ ] **Step 4: Implement** the checkbox and wrapper.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test ImportCollectionDialog`. Expected: green.

### Task 2: Connect and Change link accept a file

**Interfaces:**
- Consumes: `ConnectOpenApiDialog.tsx`, `ChangeLinkForm.tsx` (series 3 plan 01), `openFilePicker` (json, yaml, yml filter), `isGithubLocation`.
- Produces: `SpecSourceField({ value, onChange, disabled }: { value: string; onChange: (v: string) => void; disabled?: boolean })` with a Link/File switch; in File mode a **Choose spec file** button sets `value` to the picked path; both dialogs use it and send the value as `link`; the token field shows only for Github links; the Connect and Change link previews work with the Rust side from plan 02 (`file://` or a path string).

- [ ] **Step 1: Read** both dialogs' link inputs and `ImportCollectionDialog`'s `handleChooseOpenApi`.
- [ ] **Step 2: Write the failing tests** (`SpecSourceField.test.tsx` and both dialog tests): `the_field_switches_between_link_and_file`, `choosing_a_file_sets_the_value`, `no_token_field_for_a_file`, `connect_previews_a_chosen_file`, `change_link_previews_a_chosen_file`, `errors_for_a_file_never_show_the_path`.
- [ ] **Step 3: Run to see them fail.** `yarn test SpecSourceField`.
- [ ] **Step 4: Implement** the field and use it.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`. Expected: green.

### Task 3: Display, messages and docs

**Interfaces:**
- Consumes: `splitLocation`, `chipState`, header, Overview and connection dialog "Spec link" rows, `syncErrors.ts`, `nextStep` (series 3 plan 03).
- Produces: `splitLocation("file:///a/b/api.yaml")` gives `{ short: 'api.yaml in /a/b', full: null }` and a `Local file` badge in the header, Overview and dialog; `isFileLocation(location)` helper; `syncErrors` maps "The spec file was not found on this computer."; `nextStep` returns `{ kind: 'change_link', label: 'Choose another file' }` for `file_missing` (opens the connection dialog's Change link form); `docs/features/openapi-sync-ux.md` documents file sources, relative-path storage and manual checks 22 to 26; the roadmap and `crates/rocket-import/CLAUDE.md` are updated; the Bruno parity row "Source" is marked done.

- [ ] **Step 1: Read** the helpers and the three "Spec link" renderings.
- [ ] **Step 2: Write the failing tests:** `split_location_shows_the_file_name_and_folder`, `a_file_source_shows_the_local_file_badge`, `a_missing_file_offers_choose_another_file`, `a_file_source_needs_approval_on_a_new_machine`, `syncerrors_maps_the_file_missing_sentence`.
- [ ] **Step 3: Run to see them fail.** `yarn test openApiSyncView nextStep OpenApiSyncHeader`.
- [ ] **Step 4: Implement** the helpers, badge, action and docs.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync CollectionNode ImportCollectionDialog`, `cargo check -j4`. Expected: green.

Series 4 is complete here. Whole-feature review and manual checks 13 to 26 follow.
