# OpenAPI sync UX: read models Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** Give the OpenAPI sync UI the local-only data it needs: a waiting-update count, the last-synced facts, a list of what the user changed, an overview and a per-collection badge list.

**Architecture:** `OpenApiLocalState` caches `pending_count`, `last_synced_at` and `last_synced_updated`. `OpenApiSyncService` keeps them up to date in `check` and `apply`, and gains three read-only methods (`changes`, `overview`, `badges`) that read local files only. Thin Tauri commands map them to camelCase DTOs. Nothing here touches the network, the keychain or consent.

**Tech Stack:** Rust (`rocket-collection`, `rocket-app`, `src-tauri`).

**Spec:** `docs/superpowers/specs/2026-10-27-openapi-sync-ux-design.md`

## File Structure

| File | Change |
|---|---|
| `crates/rocket-collection/src/openapi_local.rs` | New fields `pending_count`, `last_synced_at`, `last_synced_updated` |
| `crates/rocket-app/src/openapi_sync_service.rs` | `SyncStatusView.pending_id`; `check` updates `pending_count` |
| `crates/rocket-app/src/openapi_sync_apply.rs` | `apply` updates `pending_count`, `last_synced_*`; `count_pending(root)` helper |
| `crates/rocket-app/src/openapi_sync_view.rs` | `ChangesView`, `ChangeRow`, `OverviewView`, `BadgeView` |
| `crates/rocket-app/src/openapi_sync_changes.rs` (new) | `changes`, `overview`, `badges` on `OpenApiSyncService` |
| `crates/rocket-app/src/openapi_sync_changes_tests.rs` (new) | Tests for Tasks 2 and 3 using the `world()` fixture of `openapi_sync_apply_tests.rs` |
| `src-tauri/src/commands/openapi_sync.rs`, `src-tauri/src/lib.rs` | DTOs and commands `openapi_sync_changes`, `openapi_sync_overview`, `openapi_sync_badges`; `pendingId` in `SyncStatusDto` |

## Global Notes

- Fixtures: reuse `world()`, `w.check_with(v2())`, `w.plan()`, `w.approve(&plan, &[..])`, `w.svc`, `w.name`, `w.root` from `crates/rocket-app/src/openapi_sync_apply_tests.rs` (see `plan_lists_new_changed_and_removed_items_after_a_changed_check` in `openapi_sync_plan_tests.rs` for the pattern). After `check_with(v2())` the plan holds `listPets` (changed), `createPet` and `getInventory` (new); `getPet` is unchanged.
- Tests use `.expect("...")` and `expect_err`, never the unwrap call.
- Commands: `cargo check -j4 -p <crate>`, `cargo test -j4 -p <crate> <filter>`.

---

### Task 1: Cached counts and status `pendingId`

**Interfaces:**
- Consumes: `OpenApiLocalState` (`crates/rocket-collection/src/openapi_local.rs:13`), `OpenApiSyncService::{check, status, apply, plan}`, `with_clock`.
- Produces:
  - `OpenApiLocalState { pending_count: u32, last_synced_at: Option<String>, last_synced_updated: u32 }`, all `#[serde(default)]` and skipped when default.
  - `SyncStatusView.pending_id: Option<String>` (first 12 hex characters of `pending_sha256`).
  - `OpenApiSyncService::count_pending(&self, collection: &str) -> DomainResult<u32>`: items + variable changes of the current plan, skipped items not counted, `0` without a pending copy.
  - `check` stores `pending_count` after a Changed outcome (via `count_pending`) and `0` after an Unchanged outcome. `apply` stores `pending_count = remaining`, and on a finalized apply also `last_synced_at` (clock, RFC 3339 UTC) and `last_synced_updated = report.applied.len()`.

- [ ] **Step 1: Read.** 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Then read `openapi_local.rs`, `openapi_sync_service.rs` (`status`, `check`), `openapi_sync_apply.rs` (`apply`, `plan`) and the tests `apply_keeps_the_timing_fields` and `plan_lists_new_changed_and_removed_items_after_a_changed_check`.
- [ ] **Step 2: Write the failing tests.**
  - In `openapi_local.rs`: `new_fields_are_omitted_at_their_defaults` (serialize `OpenApiLocalState::default()` and assert the JSON has none of the three keys) and `an_old_file_without_the_new_fields_loads_with_defaults` (parse `{"version":1}`; assert `pending_count == 0`, `last_synced_at.is_none()`, `last_synced_updated == 0`).
  - In `openapi_sync_changes_tests.rs` (new, add `#[cfg(test)] #[path = "openapi_sync_changes_tests.rs"] mod changes_tests;` to `openapi_sync_changes.rs`):
    - `status_reports_the_pending_id`: after `w.check_with(v2())`, `w.svc.status(&w.root, false).expect("status").pending_id` equals the first 12 characters of `w.source().pending_sha256.expect("hash")`; before the check it is `None`.
    - `check_stores_the_waiting_count`: after `check_with(v2())`, `w.svc.local_state(&w.name).expect("local").pending_count == 3`; after a check that returns the imported spec again (use `check_with(v1())`), it is `0`.
    - `a_partial_apply_stores_the_remaining_count`: approve only `listPets` (`w.approve(&plan, &["listPets"])`), apply, then `pending_count == 2` and `last_synced_at.is_none()`.
    - `a_finalized_apply_stores_the_sync_facts`: approve all keys, apply, then `pending_count == 0`, `last_synced_at.is_some()`, `last_synced_updated == 3`.
    - `skipped_items_are_not_counted_as_waiting`: skip `listPets` as `plan_unchanged_lists_operations_not_in_the_plan` does, then `w.svc.count_pending(&w.name).expect("count") == 2`.
    - `check_survives_a_failing_count`: corrupt `.rocket/openapi-baseline.json` with `{`, run `w.check_with(v2())`; the check still returns `CheckOutcome::Changed { .. }` and the stored `pending_count` keeps its old value (`0`).
- [ ] **Step 3: Run them to see them fail.** `cargo test -j4 -p rocket-collection openapi_local` and `cargo test -j4 -p rocket-app changes_tests`. Expected: compile errors such as `no field pending_count` and `no method count_pending`.
- [ ] **Step 4: Implement** the fields (keep the rule that fields at their default are not written), `pending_id`, `count_pending`, and the updates in `check` and `apply`. `check` must not fail when `count_pending` fails (log at debug and keep the old count). All writes use the existing locked local-state save.
- [ ] **Step 5: Run again.** Expected: the new tests pass and `cargo test -j4 -p rocket-app openapi_sync` stays green.

### Task 2: `changes` read model

**Interfaces:**
- Consumes: Task 1, `OpenApiSource.operations` (`SourceOperation { key, uid, request_path, baseline_hash }`), `OpenApiBaseline`, `merge_request(baseline, current, incoming, choices) -> MergeOutcome` (`merge.rs:107`), `baseline_hash` (`converter/openapi.rs:156`), the private `flatten()` of `openapi_sync_apply.rs:47`.
- Produces:
  - `ChangeRow { key: Option<String>, uid: String, name: String, method: String, url: String, request_path: Option<String>, fields: Vec<String> }`
  - `ChangesView { baseline_trusted: bool, modified: Vec<ChangeRow>, added: Vec<ChangeRow>, deleted: Vec<ChangeRow> }`
  - `OpenApiSyncService::changes(&self, collection: &str) -> DomainResult<ChangesView>`, read only, under the collection lock, no pending spec needed.
  - Rules: **modified** = a source operation whose request exists and differs from its baseline request in spec-owned fields; `fields` are the names from `merge_request(&baseline, &current, &baseline, &BTreeMap::new()).changes` (`url`, `header:Accept`, `body`, `name`, ...). Scripts, tests, assertions, docs and examples never count. **added** = an HTTP request whose uid is in no source operation. **deleted** = a source operation whose uid is in no request (`request_path` from the source, `fields` empty). Without a trusted baseline (missing file or other `converter_version`) `baseline_trusted` is false and modified falls back to `baseline_hash(current) != op.baseline_hash` with empty `fields`.

- [ ] **Step 1: Read.** 📖 Read `docs/superpowers/specs/opencollection-spec-reference.md`, then `merge.rs` (`merge_request`, `request_is_edited`), `openapi_baseline.rs` and `flatten` / `load_for_apply` in `openapi_sync_apply.rs`.
- [ ] **Step 2: Write the failing tests** in `openapi_sync_changes_tests.rs` (edit and delete requests through the collection repository the `world()` fixture builds, `save_request` / `delete_request`, as the apply tests do):
  - `changes_is_empty_for_a_fresh_import`: all three lists empty, `baseline_trusted` true.
  - `changes_lists_a_modified_request_with_its_field_names`: change the URL and add the header `Accept` on `getPet`; `modified` has one row, key `Some("getPet")`, `fields` contains `"url"` and `"header:Accept"`.
  - `changes_ignores_scripts_tests_and_docs`: set only a pre-request script and docs on `getPet`; `modified` stays empty.
  - `changes_lists_an_added_request`: save a new request; `added` has one row with `key == None`.
  - `changes_lists_a_deleted_request`: delete `listPets`; `deleted` has one row with key `Some("listPets")` and a `request_path`.
  - `changes_without_a_trusted_baseline_uses_hashes`: delete the baseline file, edit `getPet`; `baseline_trusted == false`, `modified` has one row with empty `fields`.
  - `changes_does_not_write_any_file`: compare `w.snapshot()` before and after.
  - `changes_fails_clearly_without_a_source`: a collection with no source gives `DomainError::NotFound`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app changes_tests`. Expected: `no method named changes`.
- [ ] **Step 4: Implement** `ChangeRow`, `ChangesView` (in `openapi_sync_view.rs`) and `changes` (in `openapi_sync_changes.rs`). Extract `flatten` use through a `pub(crate)` wrapper rather than copying it.
- [ ] **Step 5: Run again.** Expected: 8 pass.

### Task 3: `overview`, `badges` and the Tauri commands

**Interfaces:**
- Consumes: Tasks 1 and 2, `SyncStatusView`, `due_targets` style listing (`pinned()` repo, `collection_root_path`), `OpenApiLocalRepository`.
- Produces:
  - `OverviewView { endpoints: u32, in_sync: Option<u32>, changed_by_you: Option<u32>, updates_waiting: u32, spec_version: Option<String>, last_synced_at: Option<String>, last_synced_updated: u32 }`. `endpoints` = source operations. `changed_by_you` = `changes.modified.len()`. `in_sync` = `endpoints - changed_by_you - (operations with a waiting item)`, saturating at 0; both are `None` when `baseline_trusted` is false. `updates_waiting` = `pending_count`. `spec_version` = `info.version` of the pending copy when present (the stored applied copy arrives with a later series), else `None`.
  - `BadgeView { collection: String, waiting: u32 }`; `OpenApiSyncService::badges(&self) -> Vec<BadgeView>` lists collections of the pinned repo that have a source file and `pending_count > 0`, reading local state only, ignoring unreadable ones.
  - Tauri commands `openapi_sync_overview(collection) -> OverviewDto`, `openapi_sync_changes(collection) -> ChangesDto`, `openapi_sync_badges() -> Vec<BadgeDto>`, registered in `lib.rs`; `SyncStatusDto` gets `pending_id: Option<String>` (`pendingId`, skipped when `None`). DTOs are camelCase; errors use the existing fixed-sentence mapping of `openapi_sync.rs`; every command runs through `run_blocking`.

- [ ] **Step 1: Read.** 📖 Read `docs/superpowers/specs/opencollection-spec-reference.md`, `openapi_sync.rs` (`openapi_sync_status`, `run_blocking`, DTO tests at the bottom) and `due_targets` (`openapi_sync_service.rs:754`).
- [ ] **Step 2: Write the failing tests.**
  - Service (`openapi_sync_changes_tests.rs`): `overview_counts_after_a_changed_check` (after `check_with(v2())`: `endpoints == 4`, `updates_waiting == 3`, `changed_by_you == Some(0)`, `in_sync == Some(1)`); `overview_marks_counts_unknown_without_a_trusted_baseline` (`in_sync == None`, `changed_by_you == None`); `overview_reads_the_spec_version_of_the_pending_copy` (spec with `info.version` `"2.0.0"`, assert `Some("2.0.0")`; none before a check); `overview_makes_no_request_and_reads_no_token` (use the request recorder and `CountingSecrets` of the consent tests: zero requests, zero keychain reads); `badges_lists_only_collections_with_waiting_updates`; `badges_ignores_an_unreadable_state_file`.
  - Commands (`src-tauri/src/commands/openapi_sync.rs` tests): `overview_dto_uses_camel_case_and_nulls_unknown_counts` (JSON keys `inSync`, `changedByYou`, `updatesWaiting`, `specVersion`; `inSync: null` when unknown), `changes_dto_serializes_rows_with_field_names`, `badge_dto_shape`, `status_dto_includes_pending_id_only_when_present`.
- [ ] **Step 3: Run to see them fail.** `cargo test -j4 -p rocket-app changes_tests` and `cargo test -j4 -p rocket openapi_sync`. Expected: `no method named overview`, `cannot find type OverviewDto`.
- [ ] **Step 4: Implement** the views, service methods, DTOs and commands; register them in `lib.rs`.
- [ ] **Step 5: Run again.** Expected: all new tests pass; then `cargo check -j4 -p rocket-app`, `cargo check -j4 -p rocket`, `cargo test -j4 -p rocket-app openapi_sync`.

## Review Focus

1. A request the user edited only in scripts or docs must not appear in Modified. Pinned by `changes_ignores_scripts_tests_and_docs`.
2. A collection whose baseline file is missing or from an older converter must show unknown counts, not zeros. Pinned by `overview_marks_counts_unknown_without_a_trusted_baseline` and `changes_without_a_trusted_baseline_uses_hashes`.
3. Opening Overview or Changes must never fetch or read the keychain. Pinned by `overview_makes_no_request_and_reads_no_token`.
4. A skipped item must not inflate the waiting count or the badge. Pinned by `skipped_items_are_not_counted_as_waiting`.
5. A failed `count_pending` must not fail a check. Pinned by a test that makes the plan build fail (corrupt baseline) and asserts the check still returns its outcome: `check_survives_a_failing_count`.
6. A corrupt local state in one collection must not hide the badges of the others. Pinned by `badges_ignores_an_unreadable_state_file`.

Next plan: docs/superpowers/plans/2026-10-27-openapi-sync-ux/plan-02-openapi-sync-ux.md
