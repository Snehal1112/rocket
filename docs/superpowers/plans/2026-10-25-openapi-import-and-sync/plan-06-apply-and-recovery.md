# OpenAPI Apply and Recovery Implementation Plan (Plan 6 of the OpenAPI series)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply the changes a user approved from a sync plan to the collection: all of them as approved, or none. An interrupted apply is finished or undone on the next launch. Skipped changes move nothing and stay hidden until the spec changes that endpoint again.

**Architecture:** A small file-transaction port (`rocket-collection`) with a filesystem implementation (`rocket-infra`) stages every file change under `.rocket/staging/`, records a journal, then moves the files in and rolls back on any error. A pure apply builder (`rocket-import`) turns an approved selection into the exact file changes, the new source file, baseline snapshot and local state. `OpenApiSyncService::apply` (`rocket-app`) loads the current state, runs the builder, and commits through the transaction. Recovery runs from the app at launch and at the start of any sync call.

**Tech Stack:** Rust (`rocket-collection`, `rocket-infra`, `rocket-import`, `rocket-app`, `src-tauri` startup hook), `serde`, `sha2`.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md` ("Merge and identity rules": applying, skipping, removed endpoints). Intent: `docs/superpowers/specs/2026-10-26-openapi-sync-diff-intent.md`. Series index: `00-plan-index.md`, including "Carry-forward from the Plan 5 final review" (the list of what this plan owes).

## Global Constraints

- "Applying is safe, not a database transaction. Write the approved changes to a staging area, then move them in; if any step fails, restore what was changed. A small journal lets the next launch finish or undo an interrupted apply."
- "A half-applied update after a failed fetch or a crash" is an anti-goal. An apply either completes as approved or leaves every file as it was.
- "Removed endpoints. If the user never edited the request, approving the removal deletes it. If the user edited it, approving keeps it as a normal request that no longer follows the spec, and the review says so." A removal is deletable only when `Removed.edited == Some(false)`; `None` counts as edited.
- "New endpoints go into the folder for their tag (or path); that folder is recreated if the user deleted it."
- "A skipped change does not move the baseline." Skips are stored with the fingerprint of the spec's version of that endpoint. Skips are local to the user (not committed): a teammate's skip must not hide changes for others.
- Scripts, tests, assertions, actions, docs, examples, settings, tags and sequence are never touched by an apply. A request file Rocket cannot rewrite without losing content is not rewritten.
- Spec-derived names (folders, files) can never leave the collection folder, hit a reserved name, or collide with an existing path.
- Persisted structs use snake_case keys with no serde renaming. Never unwrap errors in production paths. Cargo commands use `-j4` and `-p <crate>`, never `--workspace`.
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths. This plan is Rust only, so the commit gate (`cargo check`) works.
- Tasks that touch `rocket-import`, `rocket-collection`, `rocket-infra` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."

## Rulings made while planning (open to your objection at plan review)

1. **A stale approval is never applied.** Each plan item has a digest (key, kind, fingerprint and the listed changes). The selection carries the digest the user saw. Apply recomputes the plan from the files as they are now and applies only items whose digest still matches; the rest are reported as stale. This covers both "the spec changed since the review" and "the user edited the request since the review".
2. **Roll forward after a crash, roll back after an error.** Staging is complete before the first file moves, so recovery of an interrupted commit finishes the remaining moves. A failure while moving (disk full, permission) restores the backups instead. A crash while still staging discards the staging area.
3. **Skips live in a local, git-ignored state file** (`.rocket/openapi-local.json`). Plan 7 will move the check timing there too (the Plan 4 carry-forward about git churn).
4. **The baseline's `converter_version` is never bumped by an apply.** Entries written by an apply come from the current converter, but untouched entries do not. Keeping the old value is safe and only noisy. Re-baselining after a converter change is a separate feature.
5. **The pending spec is kept while anything is still undecided.** After an apply, if every item is applied or skipped, `spec_sha256` becomes the pending hash and the pending copy and status are cleared. If something is left undecided, the pending copy and `pending_sha256` stay so the review can show it again.
6. **A request file is rewritten only if a round trip loses nothing.** Before staging an update, the current file is compared (as YAML values, not bytes) with what the repository would write for the loaded request. A difference (a key Rocket does not model) means the item is reported as `not_rewritable` and left alone.
7. **Rename links are an input of apply.** The review's "Link as rename" (Plan 9) passes `(old_key, new_key)`. Apply re-points the stored key (source and baseline) before planning, so the endpoint merges as Changed and the user's edits stay.

## Review Focus

- An error or a crash at any point leaves every file byte for byte as before, or all of them applied (Tasks 1 and 3).
- A request edited, or a spec changed, after the review is not overwritten by the old approval (Tasks 2 and 3).
- A request file with keys Rocket does not model is never rewritten (Task 2).
- A partial apply keeps the unapplied changes visible, and a skipped one moves neither baseline nor source entry (Task 2).
- Only `edited == Some(false)` is ever deleted; a spec-derived path like `../x`, `.rocket/x` or a reserved name cannot be written (Tasks 1 and 2).

---

### Task 1: File transaction, journal and local state

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-collection/src/file_tx.rs` (port types); declare and re-export in `lib.rs`.
- Create: `crates/rocket-collection/src/openapi_local.rs` (local state types and repository trait).
- Create: `crates/rocket-infra/src/fs_file_tx.rs` (transaction, journal, recovery) and `crates/rocket-infra/src/fs_openapi_local_repo.rs`; declare and re-export in `lib.rs`.
- Modify: `crates/rocket-infra/src/rocket_dir.rs` (`IGNORED_NAMES` gains `openapi-local.json`, `apply-journal.json`, `staging/`).
- Modify: `crates/rocket-import/CLAUDE.md` or `crates/rocket-infra/CLAUDE.md` (a short section on the transaction and recovery rules).

**Interfaces:**
- Produces:
  - `pub trait CollectionFileTx: Send { fn stage_put(&mut self, rel_path: &str, bytes: &[u8]) -> DomainResult<()>; fn stage_delete(&mut self, rel_path: &str) -> DomainResult<()>; fn commit(self: Box<Self>) -> DomainResult<()>; fn abort(self: Box<Self>); }`. `rel_path` is relative to the collection root, uses `/`, and is validated: no empty path, no `..`, not absolute, no backslash or NUL, and nothing under `.rocket/staging`. Paths under `.rocket/` are allowed only for the files this feature owns (`openapi-source.yml`, `openapi-baseline.json`, `openapi-pending.spec`, `openapi-local.json`).
  - `pub trait CollectionFileTxFactory: Send + Sync { fn begin(&self, collection_root: &Path) -> DomainResult<Box<dyn CollectionFileTx>>; fn recover(&self, collection_root: &Path) -> DomainResult<RecoveryOutcome>; }` and `pub enum RecoveryOutcome { Nothing, Finished { files: usize }, RolledBack { files: usize }, Discarded }`.
  - Journal `<collection>/.rocket/apply-journal.json`: `{ version, id, phase: "staging" | "committing", ops: [{ op: "put" | "delete", path, backup: bool, done: bool }] }`, rewritten with `atomic_write` as ops complete.
  - Commit order: write the journal as `committing` (staging is complete and flushed), then for each op in order: back up the existing file to `staging/<id>/backup/<path>` (rename), move the staged file into place (rename; create parent folders), or delete, then mark `done`. On any error roll back the done ops in reverse from the backups and remove files that did not exist before. On success remove the journal, then the staging folder.
  - Recovery: no journal gives `Nothing`; phase `staging` discards the staging folder (`Discarded`); phase `committing` rolls forward the ops that are not `done` (`Finished`), and if that fails rolls back (`RolledBack`).
  - Only one transaction per collection at a time: `begin` fails with `Conflict` when a journal exists (the caller runs `recover` first).
  - `pub struct OpenApiLocalState { pub version: u32, pub skips: BTreeMap<String, String> }` (snake_case, `skips` maps operation key to fingerprint); `pub trait OpenApiLocalRepository: Send + Sync { fn load(&self, root: &Path) -> DomainResult<Option<OpenApiLocalState>>; fn save(&self, root: &Path, state: &OpenApiLocalState) -> DomainResult<()>; }` writing `.rocket/openapi-local.json` (calls `ensure_gitignore` first, higher version or unreadable file is an error).

- [ ] **Step 1: Write the failing tests** (`tempfile` folders; a `FaultyFs` hook inside `fs_file_tx.rs` tests that fails the k-th rename or delete):
  - Transaction: `commit_applies_puts_and_deletes`; `a_put_over_an_existing_file_keeps_nothing_behind` (no staging folder and no journal left); `new_files_get_parent_folders`; `delete_of_a_missing_file_is_ok`; `failure_at_each_step_restores_every_file_byte_for_byte` (loop k over all steps of a 6-op transaction, assert the tree equals the original, including files that did not exist before); `abort_removes_the_staging_area`; `begin_fails_when_a_journal_exists`.
  - Paths: `rejected_paths` (`""`, `/etc/x`, `../x`, `a/../../x`, `a\\b`, `.rocket/staging/x`, `.rocket/other.txt`, a NUL byte); `allowed_state_files_under_dot_rocket`.
  - Recovery: `crash_while_staging_is_discarded` (journal with phase `staging`); `crash_mid_commit_rolls_forward` (stop after 2 of 5 ops, then `recover` gives `Finished` and the tree equals the full result); `roll_forward_failure_rolls_back` (fault during recovery); `recover_without_a_journal_is_nothing`; `recover_is_idempotent`.
  - Local state: `skips_round_trip`; `missing_file_is_none`; `newer_version_is_an_error`; `saving_writes_the_gitignore_first`; `gitignore_lists_the_new_names`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-infra fs_file_tx` and `cargo test -j4 -p rocket-infra openapi_local`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block. Keep the fault hook behind `#[cfg(test)]`.
- [ ] **Step 4: Run to verify they pass.** Run both commands, then `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): add a safe file transaction for sync`.

---

### Task 2: Build an apply from an approved selection

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-import/src/openapi/sync/apply.rs` and `crates/rocket-import/src/openapi/sync/spec.rs`; export from `sync/mod.rs` and `lib.rs`.
- Modify: `crates/rocket-import/src/openapi/sync/plan.rs` (`item_digest`, store the carried-name incoming request on `Changed`/`New` items if needed by apply).
- Modify: `crates/rocket-import/CLAUDE.md` (an "Apply" section: inputs, outputs, the rulings above).

**Interfaces:**
- Consumes: Plan 5 `build_sync_plan`, `SyncInputs`, `SyncPlan`, `SyncItem`, `merge_item`, `Choice`, `fingerprint`; Plan 4 `OpenApiSource`, `SourceOperation`; Task 1 types for the local state; `layout::folder_names`, `GroupBy`.
- Produces:
  - `pub struct ParsedSpec` (opaque, wraps the neutral document) with `pub fn parse(text: &str) -> ImportResult<ParsedSpec>`; `SyncInputs.incoming` becomes `&ParsedSpec`. This is the public entry point Plan 5 left open.
  - `pub fn item_digest(item: &SyncItem) -> String`: hex SHA-256 over key, kind, fingerprint and each change (`field`, outcome, `yours`, `spec`).
  - `pub trait RequestRenderer { fn render(&self, request_path: &str, request: &Request) -> DomainResult<Vec<u8>>; fn render_settings(&self, settings: &CollectionSettings) -> DomainResult<(String, Vec<u8>)>; }` (`render` gives the exact bytes the repository would write for a request at that path; `render_settings` gives the settings file's relative path and bytes; the real one comes from `rocket-infra` in Task 3).
  - `pub struct Selection { pub items: Vec<ApprovedItem>, pub skips: Vec<SkipRequest>, pub variables: Vec<String>, pub rename_links: Vec<(String, String)> }`, `pub struct ApprovedItem { pub key: String, pub digest: String, pub choices: BTreeMap<String, Choice> }`, `pub struct SkipRequest { pub key: String, pub fingerprint: String }`.
  - `pub struct ApplyInputs<'a> { pub plan_inputs: SyncInputs<'a>, pub pending_sha256: String, pub selection: &'a Selection, pub renderer: &'a dyn RequestRenderer, pub existing_paths: &'a HashSet<String>, pub settings: &'a CollectionSettings }`.
  - `pub struct FileChange { pub rel_path: String, pub kind: FileChangeKind }` with `enum FileChangeKind { Put(Vec<u8>), Delete }`.
  - `pub struct ApplyOutput { pub files: Vec<FileChange>, pub source: OpenApiSource, pub baseline: OpenApiBaseline, pub local: OpenApiLocalState, pub finalized: bool, pub report: ApplyReport }` and `pub struct ApplyReport { pub applied: Vec<AppliedItem>, pub stale: Vec<String>, pub not_rewritable: Vec<String>, pub deleted: Vec<String>, pub detached: Vec<String>, pub skipped: usize, pub remaining: usize }`.
  - `pub fn build_apply(inputs: &ApplyInputs) -> ImportResult<ApplyOutput>`. `files` includes request files, `folder.yml` for new folders, collection settings when variables change, but not the source, baseline, pending or local files (Task 3 stages those from the returned structs).

Apply rules:
1. Apply `rename_links` first: for each `(old, new)` the old key must be in the source and not in the incoming spec, and the new key in the incoming spec and not in the source; otherwise it is ignored and reported stale. Re-point the key in the source and the baseline.
2. Build the plan with `build_sync_plan`. For each approved item find the plan item by key; if it is missing or its `item_digest` differs, add the key to `stale` and change nothing for it. Items not approved, not skipped and not stale count in `remaining`.
3. New: choose the folder from `layout::folder_names` and the source `group_by`, create missing folders (with `folder.yml`), pick a unique file stem against `existing_paths` and the other new requests (same rules and reserved names as the importer, a name that sanitizes to empty becomes a default), render the request, and add a source entry (key, uid, request_path, `baseline_hash` of the converted incoming request) plus a baseline entry (the request as rendered, name suffix included).
4. Changed: `merge_item` with the user's choices. Run the round-trip guard (decision 6) on the current request file; on failure add the key to `not_rewritable` and leave it. Otherwise render the merged request at the item's `request_path`, put it, set the source op's `baseline_hash` and `request_path` (the current one), and replace the baseline entry with the converted incoming request (carried name).
5. Removed: with `edited == Some(false)` delete the request file and drop the source and baseline entries (`deleted`). Otherwise keep the file, drop the source and baseline entries (`detached`), and the request stays as a normal request.
6. Variables: for each name in `selection.variables` that the plan lists, a `BaseUrl` change updates `baseUrl` only when `VariableChange.baseline` is `Some` and equals the current value; `AddSecurity` adds the variable (secret, empty). Stage the settings file from `RequestRenderer::render_settings`.
7. Skips: for each `SkipRequest` whose key and fingerprint match a plan item, add it to `local.skips` and count it in `skipped`. A skipped key moves nothing else. An applied key removes its entry from `local.skips`.
8. Finalize: `finalized` is true when `remaining == 0` and `stale` is empty. Then `source.spec_sha256 = pending_sha256`, `pending_sha256 = None`, `last_status = Unchanged`, and the caller removes the pending copy. Otherwise the pending copy and hash stay. `baseline.converter_version` is never changed.
9. Path safety: every `rel_path` produced here is checked with the same rules as Task 1 before it is returned; a name that fails is replaced by a safe default, never passed through.

- [ ] **Step 1: Write the failing tests** (small inline specs through `ParsedSpec::parse`, a `MemoryRenderer` that serializes a request as JSON bytes, a baseline built by converting the first spec):
  - `parsed_spec_rejects_a_non_spec`; `item_digest_changes_with_any_listed_change`.
  - `new_endpoint_is_put_in_its_tag_folder_with_a_unique_name` (an existing request with the same stem forces a suffix); `deleted_tag_folder_is_recreated`; `new_endpoint_gets_source_and_baseline_entries`; `hostile_names_cannot_escape` (a tag `../../x`, an operation named `con`, an empty summary).
  - `changed_item_merges_and_advances_the_baseline` (user header kept, spec body taken, baseline entry equals the incoming converted request, `request_path` updated for a moved request); `choice_spec_applies_a_conflict_resolution`; `cautious_item_with_one_choice_keeps_the_rest`.
  - `unmodeled_yaml_key_makes_the_item_not_rewritable` (current file has an extra key; nothing is staged for it).
  - `removed_unedited_is_deleted`; `removed_edited_is_detached_and_kept`; `removed_with_unknown_edit_state_is_detached`.
  - `stale_digest_is_not_applied` (the user edits the request after the plan was shown); `spec_change_after_review_is_stale`.
  - `skip_moves_nothing_and_is_recorded`; `applied_key_clears_its_skip`.
  - `partial_apply_keeps_the_pending_copy` and `full_apply_finalizes_the_spec_hash`; `converter_version_is_unchanged`.
  - `rename_link_merges_as_changed_and_keeps_user_edits`; `invalid_rename_link_is_stale`.
  - `base_url_is_updated_only_when_untouched`; `security_variable_is_added_once`.
  - `apply_of_5000_items_is_linear` (a 5,000 operation spec with all items changed; completes in under 2 seconds, loose bound).
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-import openapi::sync::apply`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** `build_apply` following the rules in order, one private function per rule. Build the key maps once (carry-forward from Plan 5: no per-item scan); add `merge_item_with` or an indexed form if the linear scan shows in the 5,000 test.
- [ ] **Step 4: Run to verify they pass.** Run: `cargo test -j4 -p rocket-import openapi`, `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): build an apply from an approved selection`.

---

### Task 3: Apply through the transaction, and recovery at launch

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/openapi_sync_service.rs` (new dependencies, `apply`, `recover`, `local_state`); a split into `openapi_sync_apply.rs` is fine if the file grows past a few hundred lines.
- Create: `crates/rocket-infra/src/fs_request_renderer.rs` (`RequestRenderer` over a throwaway `FsCollectionRepo`); declare and re-export in `lib.rs`.
- Modify: `src-tauri/src/lib.rs` (wire the new service dependencies; startup and workspace-switch recovery hook).
- Modify: `crates/rocket-import/CLAUDE.md` and `docs/roadmaps/roadmap.md` R-70.

**Interfaces:**
- Consumes: Tasks 1 and 2; `CollectionRepository` (`get`, `get_settings`, `collection_root_path`); `OpenApiSourceRepository`, `OpenApiBaselineRepository`, `OpenApiLocalRepository`; `CollectionFileTxFactory`.
- Produces:
  - `OpenApiSyncService` gains constructor arguments for the collection repository, the baseline and local repositories, the transaction factory and the renderer (use a builder method so existing call sites keep compiling).
  - `async fn apply(&self, collection: &str, selection: &Selection) -> DomainResult<ApplyReport>`:
    1. `recover` first; refuse (`Conflict`) if a journal remains.
    2. Load the source, baseline, local state, pending copy; verify `sha256(pending) == pending_sha256` (Plan 4 carry-forward) and return `Conflict("the pending spec changed, check again")` otherwise.
    3. Load the collection (flatten HTTP requests to `CurrentRequest` with uid and path), `existing_paths`, settings, `current_base_url`, `existing_variables`; parse the pending text with `ParsedSpec::parse`.
    4. `build_apply`; stage every `FileChange` plus the new source file (`.rocket/openapi-source.yml`, rendered with the same serializer the repository uses), baseline, local state and, when `finalized`, a delete of the pending copy; `commit`.
    5. Return the report. Any error aborts the transaction and returns it; no file changed.
  - `fn recover(&self, collection: &str) -> DomainResult<RecoveryOutcome>` and `fn recover_all(&self) -> Vec<(String, RecoveryOutcome)>` for the active workspace's collections.
  - `fn local_state(&self, collection: &str) -> DomainResult<OpenApiLocalState>`.
  - `FsRequestRenderer::new() -> Self` implementing `RequestRenderer` (`render` and `render_settings`) by saving into a temporary `FsCollectionRepo` and reading the bytes back; the temporary folder is removed.
  - `src-tauri` startup: after the workspace loads and after a workspace switch, call `recover_all` on a background task and log the outcomes (`tracing::info`/`warn`, no user data in the message).

- [ ] **Step 1: Write the failing tests** (`#[tokio::test]` in `rocket-app` with the real `Fs*` repos on a temp workspace, built by running `import_openapi_url` with a fake fetcher so the collection, source and baseline are real):
  - `apply_changes_the_files_and_the_state_together`: after a changed spec is checked, approve all items; request files, source, baseline, pending and local state reflect it; no staging folder or journal remains.
  - `apply_with_a_failing_step_changes_nothing`: inject a failure through a wrapping `CollectionFileTxFactory`; every file under the collection is byte for byte as before.
  - `crash_mid_commit_is_finished_on_recovery`: a factory that stops after two ops; `recover` completes the apply and the state is consistent; `apply` itself refuses to start while the journal exists.
  - `stale_approval_after_a_user_edit_is_reported_and_not_applied`.
  - `changed_pending_hash_refuses_to_apply`.
  - `partial_apply_then_check_keeps_the_rest_visible`: apply one of two items; the next plan still lists the other; a skipped item is hidden and comes back when the spec changes that endpoint.
  - `moved_request_is_found_by_uid_and_the_source_path_is_updated`.
  - `renderer_output_equals_a_normal_save` (the bytes `FsRequestRenderer` returns equal the file `FsCollectionRepo::save_request` writes for the same request).
  - `recover_all_finds_journals_in_every_collection`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-app openapi_sync`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block and the startup hook. Keep the service methods thin: load, call `build_apply`, stage, commit.
- [ ] **Step 4: Run to verify they pass.** Run: `cargo test -j4 -p rocket-app openapi_sync`, `cargo test -j4 -p rocket-infra`, `cargo test -j4 -p rocket-import openapi`, then `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): apply approved spec changes safely`. Then update `00-plan-index.md` (P6 row done) and `docs/roadmaps/roadmap.md` R-70.

---

## Self-review notes

- Spec coverage: staging area, journal, finish-or-undo at launch (Tasks 1 and 3); approvals applied as approved or not at all (Task 3 step 5); removed endpoints deleted only when unedited (Task 2 rule 5); new endpoints in their folder, folder recreated (rule 3); skips that move nothing (rule 7); user moves handled by uid (Tasks 2 and 3). Not here: commands and events for the UI (Plan 9), the scheduler (Plan 7), the review screen (Plans 8 and 9), "Link as rename" UI (Plan 9; the engine support is rule 1).
- The Plan 5 carry-forward is covered: public parse entry (`ParsedSpec`), `merge_item` for choices, source `baseline_hash` and snapshot updates, entries for created requests, moved `request_path`, `edited == None` as edited, unique names at write time, the `converter_version` decision (ruling 4), skips store, the pending-hash check.
- Known limits: file watching (the app's own atomic renames will look like external changes to the file watcher; open tabs get the usual external-change refresh); flows that use a deleted request are reported by the Plan 9 dialog through the existing lookup, not by apply.
- Deferred: re-baselining after a converter change; moving check timing into the local state (Plan 7).
