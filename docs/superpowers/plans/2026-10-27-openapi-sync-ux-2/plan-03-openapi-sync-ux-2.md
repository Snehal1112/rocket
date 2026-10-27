# OpenAPI sync UX 2: Reset backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The backend can reset requests to the last synced spec with a backup, restore or discard that backup, and drop a backup when a sync updates the endpoint.

**Architecture:** A pure function in `rocket-import` copies the spec-owned fields of the baseline request over the current one. `rocket-app` runs reset, restore and discard under the collection lock in one file transaction each, using the backups file of plan 01. `openapi_sync_changes` also returns the backups. Thin Tauri commands map results to fixed sentences.

**Tech Stack:** Rust (`rocket-import`, `rocket-app`, `src-tauri`).

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md` (section 6)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-import/src/openapi/sync/reset.rs (new)`, `sync/mod.rs` | `reset_spec_owned_fields` |
| `crates/rocket-app/src/openapi_sync_reset.rs (new)` | `reset`, `restore`, `discard` on `OpenApiSyncService`; `ResetReport` |
| `crates/rocket-app/src/openapi_sync_apply.rs` | A finalized or partial apply drops backups of the keys it applied |
| `crates/rocket-app/src/{openapi_sync_view.rs,openapi_sync_changes.rs}` | `ChangesView.backups`, `ChangesView.reset_available` |
| `src-tauri/src/commands/openapi_sync.rs`, `src-tauri/src/lib.rs` | Commands `openapi_sync_reset`, `openapi_sync_restore`, `openapi_sync_discard_backup`; DTOs |

## Global Notes

- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- **Spec-owned fields reset:** name, method, url, description, headers, query, path params, form, body, auth. **Kept:** scripts, tests, assertions, actions, docs, variables, examples, settings, tags. Reset targets the baseline request of the key (the last synced spec).
- Unavailable (fixed sentence "These requests cannot be reset because the sync data is from an older version.") when the baseline is missing or its `converter_version` differs from `CONVERTER_VERSION`.
- Reset, restore and discard never contact the network and never read the keychain.
- Fixtures: `world()` from `openapi_sync_apply_tests.rs`. To edit a request, load it with the collection repo, change `url`, and save through the same repo. Tests use `.expect("...")`.
- Commands: `cargo test -j4 -p <crate> <filter>`.

---

### Task 1: Spec-owned field reset

**Interfaces:**
- Consumes: `Request` (`rocket-collection`), `request_is_edited` (`merge.rs:207`) for the field list it already compares.
- Produces: `pub fn reset_spec_owned_fields(current: &Request, baseline: &Request) -> Request` and `pub const RESET_FIELD_NAMES: [&str; 10]` (the ten spec-owned names, used by `changes` rows and the confirm text). The result keeps `current`'s uid, file name, seq and every kept field.

- [ ] **Step 1: Read** `merge.rs` (`request_is_edited`, how it lists fields) and the `Request` struct.
- [ ] **Step 2: Write the failing tests** (`reset.rs`): `reset_restores_only_spec_owned_fields` (current has a changed url, header, body and also a script, a test, an assertion, docs and a variable; the result has the baseline url, header and body and the current script, test, assertion, docs and variable), `reset_keeps_uid_file_name_and_seq`, `reset_of_an_untouched_request_is_equal_to_it`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import reset_`.
- [ ] **Step 4: Implement** the function and constant. Keep it pure: no I/O.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`. Expected: green.

### Task 2: Reset, restore and discard

**Interfaces:**
- Consumes: Task 1, plan 01 `OpenApiBackupRepository` and `OpenApiBackups::without_keys`, `flatten_requests`, `ApplyDeps.tx`, `stage_all` pattern, `collection_lock`, `OpenApiSyncService::now()`.
- Produces: `OpenApiSyncService::reset(&self, collection: &str, keys: &[String]) -> DomainResult<ResetReport>`, `restore(&self, collection: &str, key: &str) -> DomainResult<()>`, `discard(&self, collection: &str, key: &str) -> DomainResult<()>`, `ResetReport { reset: Vec<String>, skipped: Vec<String> }`. Reset: under the lock, `recover_if_idle` (a running apply is `Conflict("another sync apply is running on this collection")`), then for every key with a source operation and a baseline request, stage a backup entry (replacing an older entry for the same key) and the reset request file in one transaction; a key without an operation or a request goes to `skipped`. A request that changed on disk between load and commit makes the transaction fail and nothing is written. Restore writes the backed-up request over the request at `request_path` (matched by uid first), recreates it at `request_path` when the uid is gone (a numeric suffix `-2`, `-3` when the path is taken), and removes the entry in the same transaction. Discard only removes the entry. The apply drops the backup entries of every key it applied (`report.applied` and `report.deleted`), in its own transaction, via the extra vector.

- [ ] **Step 1: Read** `apply` (`openapi_sync_apply.rs:215-305`), the disconnect function (`recover_if_idle`) and the writer helpers used by `stage_all`.
- [ ] **Step 2: Write the failing tests** (`openapi_sync_reset_tests.rs`, new):
  - `reset_restores_spec_fields_and_writes_a_backup` (edit `listPets` url, reset, the file has the baseline url and the backup holds the edited request).
  - `reset_keeps_scripts_and_docs`.
  - `resetting_twice_keeps_the_latest_backup_only`.
  - `reset_refuses_without_a_baseline` and `reset_refuses_when_the_converter_version_differs` (fixed sentence, nothing written).
  - `reset_is_a_conflict_while_an_apply_runs` (hold a journal as the lock tests do).
  - `reset_writes_nothing_when_the_file_changed_on_disk` (a tx double or a concurrent edit; assert both the request and the backups file are unchanged).
  - `restore_puts_the_backed_up_request_back_and_removes_the_entry`.
  - `restore_recreates_a_deleted_request_with_a_numeric_suffix_when_the_path_is_taken`.
  - `restore_works_after_the_spec_changed` (check v2, then restore returns the user's version).
  - `discard_removes_only_the_entry`.
  - `a_sync_that_updates_an_endpoint_drops_its_backup` and `a_partial_apply_keeps_backups_of_endpoints_it_did_not_touch`.
  - `reset_restore_and_discard_never_fetch` (fetcher double records zero calls).
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app openapi_sync_reset`.
- [ ] **Step 4: Implement** the three methods and the apply hook. All writes through `deps.tx`; map failures to the fixed sentences used by plan 01's commands.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi_sync`. Expected: green.

### Task 3: Backups in the changes view and the commands

**Interfaces:**
- Consumes: Task 2, `ChangesView`, `ChangeRow`, `openapi_sync_changes`.
- Produces: `ChangesView.backups: Vec<BackupRow>` (`BackupRow { key, name, method, url, request_path, saved_at, fields: Vec<String> }`, `fields` = spec-owned fields where the backup differs from the baseline) and `ChangesView.reset_available: bool` (= `baseline_trusted`). DTO camelCase `backups`, `resetAvailable`, `savedAt`. Commands `openapi_sync_reset(collection, keys: Vec<String>) -> ResetReportDto { reset, skipped }`, `openapi_sync_restore(collection, key)`, `openapi_sync_discard_backup(collection, key)`. A failure maps to a fixed sentence: "These requests cannot be reset because the sync data is from an older version.", "A sync is running. Try again when it is done.", "The request changed on disk. Reload and try again." or "Reset failed. Nothing was changed."

- [ ] **Step 1: Read** `changes` in `openapi_sync_changes.rs:35` and the existing command error mapping in `commands/openapi_sync.rs`.
- [ ] **Step 2: Write the failing tests:** in `openapi_sync_changes_tests.rs`: `changes_lists_backups_with_differing_fields`, `changes_reports_reset_unavailable_without_a_trusted_baseline`, `a_corrupt_backups_file_gives_an_error_not_an_empty_list`. In `openapi_sync.rs`: `changes_dto_has_backups_and_reset_available_keys`, `reset_errors_map_to_fixed_sentences` (the message of a conflict and of an internal error contain no path).
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app changes_lists_backups`.
- [ ] **Step 4: Implement** the view fields, the commands, and register them in `lib.rs`.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi_sync`, `cargo check -j4`. Expected: green.

## Review Focus

1. A reset must never touch scripts, tests, assertions or docs. Pinned by `reset_restores_only_spec_owned_fields` and `reset_keeps_scripts_and_docs`.
2. A second reset must not destroy the user's first saved version. Decide and pin: the newest backup replaces the older one (the older already equals a prior reset). Pinned by `resetting_twice_keeps_the_latest_backup_only`.
3. A failed write must leave neither a half-reset request nor a lost backup. Pinned by `reset_writes_nothing_when_the_file_changed_on_disk`.
4. Restore must still work after the spec changed or the request was deleted. Pinned by `restore_works_after_the_spec_changed` and the recreate test.
5. A corrupt backups file must fail loudly, never read as "no backups". Pinned by `a_corrupt_backups_file_gives_an_error_not_an_empty_list`.

Next plan: docs/superpowers/plans/2026-10-27-openapi-sync-ux-2/plan-04-openapi-sync-ux-2.md
