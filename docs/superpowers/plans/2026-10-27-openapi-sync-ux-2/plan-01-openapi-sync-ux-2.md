# OpenAPI sync UX 2: Stored files Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The collection keeps a restorable-backup file and a copy of the last applied spec, so Reset, Restore and View spec have data to work from.

**Architecture:** A new backups model and repository sit next to the other `.rocket` state files and join the owned-files list, the git-ignore text and disconnect. The applied spec copy is written at import and in the same transaction as a finalized apply (it becomes the old pending text), and a consented check heals a missing copy.

**Tech Stack:** Rust (`rocket-collection`, `rocket-infra`, `rocket-import`, `rocket-app`).

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md` (sections 6 and 7)

## File Structure

| File | Change |
|---|---|
| `crates/rocket-collection/src/openapi_backup.rs` (new) | `OpenApiBackups`, `BackupEntry`, `OpenApiBackupRepository` |
| `crates/rocket-collection/src/{lib.rs,file_tx.rs,openapi_source.rs}` | Export the module; `OWNED_STATE_FILES` 5 -> 6; `save_applied` / `load_applied` on `OpenApiSourceRepository` |
| `crates/rocket-infra/src/{fs_openapi_backup_repo.rs (new),fs_openapi_source_repo.rs,rocket_dir.rs,lib.rs}` | Filesystem impls, git-ignore line |
| `crates/rocket-import/src/openapi/entry.rs` | `import_openapi_url` writes the applied copy |
| `crates/rocket-app/src/{openapi_sync_apply.rs,openapi_sync_service.rs}` | Finalized apply stages the applied copy; check self-heals; disconnect removes the backups file |

## Global Notes

- Fixtures: `world()` and `check_with` of `crates/rocket-app/src/openapi_sync_apply_tests.rs`; the existing disconnect tests are in `openapi_sync_disconnect_tests.rs`. Tests use `.expect("...")`, never the unwrap call.
- `OWNED_STATE_FILES` already lists `openapi-applied.spec`; this plan adds `openapi-reset-backups.json`.
- Commands: `cargo check -j4 -p <crate>`, `cargo test -j4 -p <crate> <filter>`.

---

### Task 1: Backups model, repository and owned file

**Interfaces:**
- Consumes: the pattern of `openapi_local.rs` and `fs_openapi_local_repo.rs` (version check, `atomic_write`), `ensure_gitignore` (`rocket_dir.rs`).
- Produces: `BACKUP_FILE_VERSION: u32 = 1`; `BackupEntry { key: String, uid: String, request_path: String, saved_at: String, request: Request }` and `OpenApiBackups { version: u32, entries: Vec<BackupEntry> }` (snake_case, `#[serde(default)]` on every field, `Default` gives version 1 and no entries); trait `OpenApiBackupRepository { load(&self, root: &Path) -> DomainResult<Option<OpenApiBackups>>; save(&self, root: &Path, b: &OpenApiBackups) -> DomainResult<()> }`; `FsOpenApiBackupRepo`; the file name `openapi-reset-backups.json`; helper `OpenApiBackups::without_keys(&self, keys: &[String]) -> OpenApiBackups`.

- [ ] **Step 1: Read.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Then read `openapi_local.rs`, `fs_openapi_local_repo.rs`, `rocket_dir.rs`, and the disconnect tests that assert the list of removed files.
- [ ] **Step 2: Write the failing tests.**
  - `openapi_backup.rs`: `a_backup_file_round_trips`, `an_old_file_without_entries_loads_with_defaults` (parse `{"version":1}`), `without_keys_drops_only_the_named_entries`.
  - `fs_openapi_backup_repo.rs`: `load_returns_none_for_a_missing_file`, `a_newer_version_is_an_error_not_defaults`, `save_then_load_round_trips`.
  - `rocket_dir.rs`: `the_gitignore_lists_the_backups_file`.
  - `file_tx.rs`: `owned_state_files_lists_the_backups_file` (length 6).
  - `openapi_sync_disconnect_tests.rs`: `disconnect_removes_the_backups_file` (write a file, disconnect, assert it is gone) and update the tests that list the owned files to the six.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-collection openapi_backup`. Expected: module not found.
- [ ] **Step 4: Implement** the model, the trait, `FsOpenApiBackupRepo` (a corrupt or newer file is an error, never replaced by defaults), the `OWNED_STATE_FILES` entry, the git-ignore name, and let `remove_sync_state` stage the removal of the backups file with the other owned files.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-collection`, `-p rocket-infra`, `cargo test -j4 -p rocket-app disconnect`. Expected: green.

### Task 2: Applied copy at import and on a finalized apply

**Interfaces:**
- Consumes: Task 1 constant pattern, `OpenApiSourceRepository::{save_pending, load_pending}`, `import_openapi_url` (`entry.rs:147`), the `extra` staging vector and `PENDING_REL` in `OpenApiSyncService::apply`.
- Produces: `OpenApiSourceRepository::save_applied(&self, root, spec: &[u8]) -> DomainResult<()>` and `load_applied(&self, root) -> DomainResult<Option<Vec<u8>>>` (file `.rocket/openapi-applied.spec`); constant `APPLIED_REL` beside `PENDING_REL`; `import_openapi_url` saves `bundle.text` as the applied copy right after `sources.save` and adds the warning "The saved copy of the spec could not be written." on failure (never an error); a finalized apply stages `(APPLIED_REL, Some(pending_text))` in the same transaction that clears the pending copy.

- [ ] **Step 1: Read** `entry.rs:147-237`, `fs_openapi_source_repo.rs` (`save_pending`), and `apply` (`openapi_sync_apply.rs:215-305`).
- [ ] **Step 2: Write the failing tests.**
  - `fs_openapi_source_repo.rs`: `applied_copy_round_trips_and_a_missing_one_is_none`.
  - `entry.rs`: `import_writes_the_applied_copy` (the file equals the bundled text) and `import_warns_when_the_applied_copy_cannot_be_written` (a source repo double whose `save_applied` fails; the import still succeeds).
  - `openapi_sync_changes_tests.rs`: `a_finalized_apply_replaces_the_applied_copy_with_the_applied_spec` (after `check_with(v2())` and an approve-all apply, the applied copy equals the v2 text and the pending copy is gone) and `a_partial_apply_leaves_the_applied_and_pending_copies_alone`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-import applied`. Expected: `no method save_applied`.
- [ ] **Step 4: Implement** the two trait methods and the writes. `save_applied` uses `atomic_write`. The staged write in `apply` happens only when `output.finalized`.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-import openapi`, `cargo test -j4 -p rocket-app openapi_sync`. Expected: green.

### Task 3: Check heals a missing applied copy

**Interfaces:**
- Consumes: Task 2 methods, `OpenApiSyncService::check` (`openapi_sync_service.rs:446`), `OpenApiSource.spec_sha256`, the fetch path of `check` (consent and token rules stay as they are).
- Produces: `check` behaviour. When the check is consented, finds no pending copy and `load_applied` is `None`, it fetches once without validators (no `If-None-Match`, no `If-Modified-Since`). If the sha256 of the fetched (bundled) text equals `source.spec_sha256`, it stores the text as the applied copy and returns the unchanged outcome. If the hash differs, the normal pending rules apply and no applied copy is written.

- [ ] **Step 1: Read** `check` and its tests (`check_writes_timing_to_the_local_state_not_the_source`), and how it builds the fetch request and `bundle_spec` text.
- [ ] **Step 2: Write the failing tests** (same file as Task 2):
  - `check_stores_the_applied_copy_when_the_hash_matches` (delete the applied copy, run `check_with(v1())`, the copy is back).
  - `check_does_not_store_an_applied_copy_when_the_hash_differs` (`check_with(v2())` with the copy missing: the pending copy is written, the applied copy stays missing).
  - `check_sends_no_validators_when_the_copy_is_missing` (the fetcher double records the request; both validators are `None` although the source holds an etag).
  - `check_without_consent_never_fetches_to_heal` (no consent: no fetch call, no copy).
  - `check_leaves_an_existing_applied_copy_alone`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app check_stores_the_applied_copy`.
- [ ] **Step 4: Implement** the heal inside `check`, under the lock it already holds. A failed save of the copy is logged at debug and never fails the check.
- [ ] **Step 5: Run again.** `cargo test -j4 -p rocket-app openapi_sync`; `cargo check -j4`. Expected: green.

## Review Focus

1. A corrupt or newer backups file must be an error, never silently replaced by an empty one (it would drop the user's saved versions). Pinned by `a_newer_version_is_an_error_not_defaults`.
2. A failed applied-copy write at import must not fail the import. Pinned by `import_warns_when_the_applied_copy_cannot_be_written`.
3. A partial apply must not move the applied copy. Pinned by `a_partial_apply_leaves_the_applied_and_pending_copies_alone`.
4. Healing must never fetch without consent. Pinned by `check_without_consent_never_fetches_to_heal`.
5. Disconnect must leave no backups file behind. Pinned by `disconnect_removes_the_backups_file`.

Next plan: docs/superpowers/plans/2026-10-27-openapi-sync-ux-2/plan-02-openapi-sync-ux-2.md
