# OpenAPI sync UX 5: Merge parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the merge-rule gaps found against Bruno: urlencoded bodies merge per field, Reset covers the same fields as Bruno, and the intentional limits are documented.

**Architecture:** `merge_body` gets a urlencoded branch that reuses `merge_entries("form")`; `reset_request` and `RESET_FIELD_NAMES` drop three fields; docs and a reserved-folder-name check close the rest.

**Tech Stack:** Rust (`rocket-import`, `rocket-app`), React + TypeScript (one string), Vitest.

**Spec:** `docs/superpowers/specs/2026-10-28-openapi-sync-ux-3-design.md` (section 8); parity table section 4.

## File Structure

| File | Change |
|---|---|
| `crates/rocket-import/src/openapi/sync/{merge.rs,reset.rs}` | Urlencoded branch, Reset field list |
| `crates/rocket-app/src/openapi_sync_reset.rs`, `openapi_sync_reset_tests.rs` | `RESET_FIELD_NAMES` |
| `src/components/collections/openapi-sync/{openApiSyncView.ts,OpenApiSyncChanges.tsx}` | Field labels, Reset confirm text |
| `docs/features/openapi-sync-ux.md` | Known limits, manual checks |

## Review Focus

1. A urlencoded value containing `&`, `=` or `%26` must round-trip without changing. Pinned by `urlencoded_pairs_round_trip_exactly`.
2. A user edit to one urlencoded field and a spec change to another must both survive. Pinned by `urlencoded_fields_merge_independently`.
3. Reset must keep a user's rename, method and description. Pinned by `reset_keeps_name_method_and_description`.
4. Backups taken before this change (with the old field list) must still restore. Pinned by `an_old_backup_still_restores`.
5. Reset must still keep scripts, tests, assertions, docs and examples. Pinned by the existing reset tests, kept green.

---

### Task 1: Urlencoded body merge

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Interfaces:**
- Consumes: `merge_body` (`merge.rs:574`), `merge_entries("form")` and its `Entry` trait, the converter's urlencoded text (`converter/openapi.rs:357`, `a=b&c=d`), the `FormUrlEncoded` body mode.
- Produces: in `merge_body`, mode `FormUrlEncoded` on all three sides splits each text on `&` into entries (name and value percent-decoded as needed, original text kept for the rejoin), merges them by name with `merge_entries`, and rejoins with `&`; any side that fails to split falls back to today's scalar merge. Field names for changes: `form:<name>` (same labels as form-data).

- [ ] **Step 1: Read** `merge_body`, `merge_entries`, the `Entry` impls and the form-data tests.
- [ ] **Step 2: Write the failing tests** (`merge.rs`): `urlencoded_fields_merge_independently`, `urlencoded_pairs_round_trip_exactly`, `a_removed_urlencoded_field_is_dropped_when_untouched`, `a_user_edited_urlencoded_field_survives_a_spec_removal_as_a_conflict`, `an_unparsable_urlencoded_body_falls_back_to_the_scalar_merge`, `urlencoded_changes_are_named_form_colon_name`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import urlencoded`.
- [ ] **Step 4: Implement** the branch.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`. Expected: green.

### Task 2: Reset scope

**Interfaces:**
- Consumes: `reset_request` (`reset.rs:17-27`), `RESET_FIELD_NAMES` and the reset service (`openapi_sync_reset.rs`), the backup entry format (`BackupEntry.request` is a full `Request`), `OpenApiSyncChanges.tsx` Reset confirm.
- Produces: Reset replaces URL, query, path params, headers, body and auth with the baseline's, and keeps name, method, description, scripts, tests, assertions, docs, examples, variables and tags; `RESET_FIELD_NAMES` lists those six groups only; a row whose only differences are name, method or description is no longer reset-eligible; the Reset confirm reads "Your changes to the URL, parameters, headers, body and auth of this request will be replaced with the last synced version. A backup is kept so you can restore it."

- [ ] **Step 1: Read** `reset.rs`, the reset service and its tests, the Restore code (to confirm it stores the whole request).
- [ ] **Step 2: Write the failing tests:** `reset_keeps_name_method_and_description`, `reset_still_keeps_scripts_tests_and_assertions`, `a_name_only_change_is_not_reset_eligible`, `an_old_backup_still_restores`, `reset_all_skips_rows_that_only_changed_the_name`; frontend `the_reset_confirm_names_the_replaced_fields`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import reset`, `cargo test -j4 -p rocket-app reset`, `yarn test OpenApiSyncChanges`.
- [ ] **Step 4: Implement** the narrower reset and text.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`, `cargo test -j4 -p rocket-app openapi_sync`, `yarn tsc --noEmit`, `yarn check`. Expected: green.

### Task 3: Known limits, reserved folder names and docs

**Interfaces:**
- Consumes: `folder_names` (`plan.rs:245`), `docs/features/openapi-sync-ux.md`.
- Produces: tag or path folder names `node_modules`, `.git`, `environments`, `mocks` are not used as folder names (they fall back to the collection root, like Bruno); the doc section "Known limits" lists: JSON arrays and top-level array bodies are merged as one value, numbers that are not canonical are merged as text, Reset restores the last synced version and a waiting spec change stays in the review list, regrouping existing folders is not supported; manual checks 27 to 30.

- [ ] **Step 1: Read** `folder_names` and its tests.
- [ ] **Step 2: Write the failing tests:** `a_reserved_tag_name_falls_back_to_the_root`, `a_reserved_path_segment_falls_back_to_the_root`, `a_normal_tag_is_unchanged`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import folder_names`.
- [ ] **Step 4: Implement** the rule and write the docs section.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`, `cargo check -j4`. Expected: green.

Next plan: docs/superpowers/plans/2026-10-28-openapi-sync-ux-5/plan-04-openapi-sync-ux-5.md
