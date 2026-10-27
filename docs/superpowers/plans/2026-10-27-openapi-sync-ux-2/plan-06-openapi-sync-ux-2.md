# OpenAPI sync UX 2: Connect UI and docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A user can connect a hand-made collection to an OpenAPI spec from the sidebar menu through a link step and an Initial sync review, and the feature docs match what shipped.

**Architecture:** Wrappers for the two connect commands of plan 05. A two-step `ConnectOpenApiDialog` (link, then review rows with a select per row). The collection menu shows **Connect to an OpenAPI spec** for collections without a source. A final task updates the docs.

**Tech Stack:** React + TypeScript, react-query, shadcn/ui (Dialog, Select, Input, Checkbox, Button, Alert, Badge), lucide-react, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md` (section 9, Files changed)

## File Structure

| File | Change |
|---|---|
| `src/lib/tauri-api.ts` | Types `ConnectPreview`, `ConnectRow`, `ConnectOutcome`; `previewOpenApiConnect`, `confirmOpenApiConnect` |
| `src/lib/queries/openapi-sync-queries.ts` | `usePreviewOpenApiConnect`, `useConfirmOpenApiConnect` (invalidate status, overview, changes, plan, badges and the collection list) |
| `src/components/collections/openapi-sync/{ConnectOpenApiDialog.tsx,ConnectReviewStep.tsx,connectRows.ts}` (new) | Dialog, step 2, default row choices |
| `src/components/collections/CollectionNode.tsx` | Menu item and dialog mount |
| docs | `docs/features/openapi-sync-ux.md`, `crates/rocket-import/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `docs/roadmaps/roadmap.md`, `CLAUDE.md` |
| tests | `tauri-api.openapi-sync.test.ts`, `openapi-sync/__tests__/{connectRows.test.ts,ConnectOpenApiDialog.test.tsx,ConnectReviewStep.test.tsx}`, `CollectionNode` test |

## Global Notes

- Commands from plan 05: `openapi_connect_preview { collection, link, token? }` and `openapi_connect_confirm { collection, link, token?, rows, groupBy, interval?, digest }`. Row kinds `link | add | skip | keep`. Mirror the camelCase DTOs of `src-tauri/src/commands/openapi_sync.rs` exactly.
- Default choices: Matched -> Link; Suggested -> Keep (not preselected as Link); In spec only -> Add; Yours only -> Keep; Ambiguous -> no default, **Continue** is disabled until the user picks.
- shadcn/ui and lucide-react only; fixed sentences only (use `syncErrorMessage`; add the connect sentences "The collection changed. Run the preview again." and the two warnings to it).
- Commands: `yarn tsc --noEmit`, `yarn check`, `yarn test <name>`.

---

### Task 1: Wrappers, hooks and default rows

**Interfaces:**
- Consumes: plan 05 DTOs, `openApiSyncKeys`.
- Produces: the two wrappers and mutation hooks; `defaultRows(preview: ConnectPreview): ConnectRowChoice[]` and `rowsToRequest(choices): ConnectRow[]` in `connectRows.ts`; `hasUnresolvedAmbiguity(choices): boolean`.

- [ ] **Step 1: Read** `useApplyOpenApiSync`, the import dialog's group-by control (`ImportCollectionDialog.tsx`) and its token rules.
- [ ] **Step 2: Write the failing tests:** `tauri-api.openapi-sync.test.ts`: `preview_calls_openapi_connect_preview`, `confirm_calls_openapi_connect_confirm_with_rows_and_digest`. `connectRows.test.ts`: `matched_rows_default_to_link`, `suggested_rows_default_to_keep`, `spec_only_rows_default_to_add`, `yours_only_rows_default_to_keep`, `ambiguous_rows_have_no_default_and_block_continue`, `rows_to_request_sends_the_choice_key_and_uid`. Hooks: `a_confirmed_connect_refreshes_status_overview_changes_plan_and_badges`.
- [ ] **Step 3: Run to see them fail.** `yarn test connectRows`.
- [ ] **Step 4: Implement** types, wrappers, hooks and helpers.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn test openapi-sync`. Expected: green.

### Task 2: Connect dialog and menu item

**Interfaces:**
- Consumes: Task 1, `openOpenApiSyncTab`, the collection source status (the same signal the sidebar uses for the sync entry).
- Produces: `ConnectOpenApiDialog({ collection, open, onOpenChange })`. Step 1: spec link (GitHub or https), optional token (password input, shown for GitHub links only), group-by select, **Check for spec updates** checkbox (default on, picks the default interval), **Preview** button. Step 2 (`ConnectReviewStep`): four sections (Matched, Suggested, In spec only, Yours only, plus Ambiguous at the top), each row with method, path, name and a select (Link / Add / Skip / Keep as allowed for the section), and a summary line "N linked, N added, N skipped". **Back** returns to step 1 with the link kept and the token cleared from state on close. **Connect** confirms; on success the dialog closes, the sync tab opens (`openOpenApiSyncTab(collection)`) and warnings show in a toast. A stale-digest error returns to step 1 with the fixed sentence. The menu item **Connect to an OpenAPI spec** shows only for collections without a source and only when the collection has HTTP requests or none at all (a pure GraphQL/gRPC collection is hidden).

- [ ] **Step 1: Read** the sync entry in `CollectionNode.tsx` (the `openOpenApiSync` handler and its visibility rule) and an existing two-step dialog if any.
- [ ] **Step 2: Write the failing tests** (`ConnectOpenApiDialog.test.tsx`, `ConnectReviewStep.test.tsx`, CollectionNode test): `preview_is_disabled_without_a_link`, `the_token_field_shows_for_github_links_only`, `preview_failure_shows_a_fixed_sentence`, `step_two_groups_rows_into_sections`, `changing_a_select_changes_the_summary`, `continue_is_blocked_until_ambiguous_rows_are_picked`, `connect_sends_the_rows_digest_link_and_group_by`, `success_opens_the_sync_tab_and_closes`, `a_stale_digest_returns_to_step_one_with_the_sentence`, `the_token_is_not_kept_after_close`, `warnings_are_toasted_as_fixed_sentences`, `the_menu_item_shows_only_without_a_source`, `the_menu_item_is_hidden_for_a_graphql_only_collection`.
- [ ] **Step 3: Run to see them fail.** `yarn test ConnectOpenApiDialog`.
- [ ] **Step 4: Implement** the dialog, the review step and the menu item. No raw `<select>` or `<input>`.
- [ ] **Step 5: Run again.** `yarn tsc --noEmit`, `yarn check`, `yarn test openapi-sync`, `yarn test CollectionNode`. Expected: green.

### Task 3: Docs and roadmap

**Interfaces:**
- Consumes: the shipped behaviour of plans 01 to 06.
- Produces: updated docs (short, in the style of the file).

- [ ] **Step 1: Read** `docs/features/openapi-sync-ux.md`, `crates/rocket-import/CLAUDE.md`, `crates/rocket-app/CLAUDE.md` and the OpenAPI section of `docs/roadmaps/roadmap.md`.
- [ ] **Step 2: Update** `docs/features/openapi-sync-ux.md` with: the applied copy and backups file (names, when written and removed), View spec, Reset and Restore (field list, confirm text, known limits), Connect (matching rules, commit order, warnings), and a manual-check list (reset then restore; sync drops a backup; connect a hand-made collection, then check; open View spec offline). Add one line each to the two crate CLAUDE.md files (matching, reset, spec view in `rocket-import`; connect, reset, spec view in `rocket-app`). Mark the roadmap item shipped.
- [ ] **Step 3: Check** that every command, file and test name in the docs exists: `grep` each against the tree.
- [ ] **Step 4: Run** `yarn check` (docs are not linted, but the pointer in `CLAUDE.md` must stay one line) and `cargo check -j4`. Expected: clean.

## Review Focus

1. Connect must never preselect a risky link: Suggested rows start as Keep and Ambiguous rows block Continue. Pinned by `suggested_rows_default_to_keep` and `continue_is_blocked_until_ambiguous_rows_are_picked`.
2. The token must not stay in React state or logs after the dialog closes. Pinned by `the_token_is_not_kept_after_close`.
3. Errors and warnings must be fixed sentences. Pinned by `preview_failure_shows_a_fixed_sentence` and `warnings_are_toasted_as_fixed_sentences`.
4. A collection with no HTTP requests or only GraphQL must not offer a pointless connect. Pinned by `the_menu_item_is_hidden_for_a_graphql_only_collection`.
5. A double-click on **Connect** must not send two confirms. Add `connect_ignores_a_second_click_while_running` to the dialog tests.

Next plan: none
