# OpenAPI Diff and Merge Engine Implementation Plan (Plan 5 of the OpenAPI series)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Given the spec a collection was last synced to, the user's current requests and the new spec, produce a reviewable plan of what would change: New, Changed and Removed endpoints, with a field-by-field merge that keeps the user's work. Pure code, nothing is written to the user's requests.

**Architecture:** A baseline snapshot (the converted requests as last synced) is stored next to the source file and written at link import. A pure merge function does the three-way merge of one request. A plan builder matches operations by their stored key, classifies them, hints at renames, hides skipped changes and returns a `SyncPlan`. Apply (Plan 6) and the review UI (Plans 8 and 9) consume the plan.

**Tech Stack:** Rust (`rocket-collection`, `rocket-infra`, `rocket-import`), `serde_json`, `sha2`.

**Spec:** `docs/superpowers/specs/2026-10-25-openapi-import-and-sync-design.md` (sections "Merge and identity rules", "Facts the builder needs"). Series index: `00-plan-index.md`, including "Series 2 decisions" and "Carry-forward".

## Global Constraints

- "User wins per field, not per request." JSON bodies merge by key path; form fields, params and headers merge by name, keeping the user's value and enabled state; `{{var}}` references are kept. A user's change to a param value while the spec removes that param is a conflict the user chooses.
- "Same auth type keeps the user's auth values; a different type takes the spec's."
- "Scripts, tests, assertions, actions, docs, examples, settings, tags and sequence are never touched by a sync." A merge never changes those fields.
- "Edited by the user" means any difference between the current request and its baseline, in any field. "When in doubt, treat the request as edited."
- Identity is the stored operation key and the request `uid`, not the file path. A request the user moved or renamed still syncs. A request the user deleted is not recreated.
- A skipped change does not move the baseline. The skip is stored with a hash of the spec's version of that endpoint and stays hidden until that version differs.
- "Possibly renamed" pairing is a hint only and never applied automatically.
- Design for specs up to 5,000 operations: matching and planning are linear, no per-operation scan of all requests.
- Persisted structs use snake_case keys with no serde renaming (`Request` keeps its own existing serde shape). Never unwrap errors in production paths. Cargo commands use `-j4` and `-p <crate>`, never `--workspace`.
- Spec copies and the baseline snapshot are git-ignored through `.rocket/.gitignore` (Series 2 decision).
- Commits go through `dev-workflow-skills:1-git-commit` with explicit paths. This plan is Rust only, so the commit gate (`cargo check`) works.
- Tasks that touch `rocket-import`, `rocket-collection`, `rocket-infra` or collection files start with: "Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`."

## Rulings made while planning (open to your objection at plan review)

1. **The applied copy is a baseline snapshot of converted requests, not the spec text.** The decision "keep a copy of the last applied spec" cannot hold the rule "a skipped change does not move the baseline": one spec file cannot say that operation A moved to the new version while operation B stayed. The snapshot stores one converted `Request` per operation, so apply (Plan 6) advances only the approved operations. The pending spec text copy from Plan 4 stays.
2. **A skipped change is hidden, not listed.** The spec says both "stays in the list" and "stays hidden until the spec's version differs". The second sits in the later, more specific "Merge and identity rules" section, so it wins. The plan reports a count of hidden skips so the UI can show "3 skipped".
3. **The hint rules are two, not three.** A changed `operationId` on the same method and path, and a changed path on the same method and summary. "Same operationId with a new method or path" needs no hint, because the stored key is the `operationId`, so it already matches as Changed.
4. **`seq` is not part of "edited".** It cannot be rebuilt (the importer numbers requests per folder), and reordering is not editing. `uid` and `file_name` are identity, not content. Every other field of `Request` counts.
5. **A converter change must not fake a mass edit.** The snapshot stores a `converter_version`. When it differs from the running code, the plan falls back to the cautious mode described in Task 3.
6. **A merged JSON body is re-serialized with sorted keys** when a spec change is taken (the workspace `serde_json` has no `preserve_order`). When nothing from the spec is taken the user's text is left exactly as it is.

## Review Focus

- A header, param, form field or JSON key the user added is never removed or overwritten by a merge (Task 2).
- The spec removes something the user changed: a conflict the user chooses, never a silent delete and never silently ignored (Task 2).
- A collection with no snapshot (imported before this plan), a snapshot from another `converter_version` or a missing operation in the snapshot: nothing of the user's is overwritten and no removal is offered as safe to delete (Task 3).
- A request the user moved to another folder, renamed or deleted: matched by `uid`; a deleted one is not recreated (Task 3).
- 5,000 operations plan in linear time, and a JSON body nested 200 levels deep or an array of 100,000 numbers does not overflow the stack or hang (Tasks 2 and 3).

---

### Task 1: Baseline snapshot, converter version, import writes it

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-collection/src/openapi_baseline.rs` (types and trait); declare and re-export in `crates/rocket-collection/src/lib.rs`.
- Create: `crates/rocket-infra/src/fs_openapi_baseline_repo.rs` and `crates/rocket-infra/src/rocket_dir.rs` (shared `ensure_gitignore`); declare and re-export in `crates/rocket-infra/src/lib.rs`.
- Modify: `crates/rocket-infra/src/fs_openapi_source_repo.rs` (`save_pending` also calls `ensure_gitignore`).
- Modify: `crates/rocket-import/src/converter/openapi.rs` (`pub const CONVERTER_VERSION: u32 = 1;` with a doc comment saying to bump it on any change to `convert_operation` output).
- Modify: `crates/rocket-import/src/importer.rs` (`ImportedOperation` gains `baseline: Request`, the request exactly as saved).
- Modify: `crates/rocket-import/src/openapi/entry.rs` and `src-tauri/src/commands/import.rs` (`import_openapi_url` takes `baselines: &dyn OpenApiBaselineRepository` and writes the snapshot).
- Modify: `crates/rocket-import/CLAUDE.md` (the new file and the gitignore).

**Interfaces:**
- Consumes: `Request`, `atomic_write`, `OpenApiSourceRepository`, `ImportedOperation`, `OpenApiImportOutcome`.
- Produces:
  - `pub const BASELINE_FILE_VERSION: u32 = 1;`
  - `pub struct OpenApiBaseline { pub version: u32, pub converter_version: u32, pub base_url: String, pub operations: Vec<BaselineOperation> }` and `pub struct BaselineOperation { pub key: String, pub request: Request }` (serde, snake_case keys for the struct fields).
  - `pub trait OpenApiBaselineRepository: Send + Sync { fn load(&self, collection_root: &Path) -> DomainResult<Option<OpenApiBaseline>>; fn save(&self, collection_root: &Path, baseline: &OpenApiBaseline) -> DomainResult<()>; fn delete(&self, collection_root: &Path) -> DomainResult<()>; }`. A file with a higher `version`, or one that cannot be read, is an error and never replaced by defaults.
  - `FsOpenApiBaselineRepo` writes `<collection>/.rocket/openapi-baseline.json` with `atomic_write`.
  - `pub fn ensure_gitignore(collection_root: &Path) -> DomainResult<()>` in `rocket-infra`: creates `<collection>/.rocket/.gitignore` containing `openapi-baseline.json` and `openapi-pending.spec`, or appends only the missing lines; never removes or reorders lines the user wrote.
  - `import_openapi_url(..., baselines: &dyn OpenApiBaselineRepository, ...)`: after the source file, saves a baseline with `converter_version: CONVERTER_VERSION`, `base_url` from `servers[0]` and one `BaselineOperation` per imported operation. A baseline write failure adds a warning ("The review will be cautious until the next sync.") and never fails the import.

- [ ] **Step 1: Write the failing tests:**
  - `rocket-infra`: `baseline_round_trips`; `missing_file_is_none`; `newer_version_is_an_error`; `corrupt_file_is_an_error`; `delete_ignores_a_missing_file`; `gitignore_is_created_with_both_names`; `gitignore_keeps_user_lines_and_adds_only_missing_ones` (a file with a user line and one of the two names ends with the user line first, each name once); `saving_a_pending_copy_also_writes_the_gitignore`; `a_collection_with_a_baseline_still_loads_in_strict_mode`.
  - `rocket-import` (`entry.rs`): `link_import_writes_a_baseline_matching_the_saved_requests` (operation count equals the imported count; each snapshot request equals the request loaded from the collection, ignoring `file_name`); `baseline_records_the_converter_version_and_base_url`; `baseline_write_failure_is_a_warning_not_an_error` (a failing fake repository); `file_import_writes_no_baseline`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-infra openapi_baseline` and `cargo test -j4 -p rocket-import openapi::entry`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block. Update every caller of `import_openapi_url` (grep) and the Plan 3 and 4 tests.
- [ ] **Step 4: Run to verify they pass.** Run the two commands, then `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): keep a baseline snapshot of the synced requests`.

---

### Task 2: Merge one request (three-way, per field)

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-import/src/openapi/sync/mod.rs`, `crates/rocket-import/src/openapi/sync/merge.rs`, `crates/rocket-import/src/openapi/sync/json_merge.rs`; declare `sync` in `openapi/mod.rs` and re-export the public items from `lib.rs`.
- Test: in the same files.

**Interfaces:**
- Consumes: `Request` and its field types (`Header`, `QueryParam`, `PathParam`, `Body`, `FormDataEntry`, `Auth`, `Description`).
- Produces:
  - `pub enum FieldOutcome { TakeSpec, KeepYours, Conflict }` (`Conflict` only when the spec removed something the user changed; default resolution is yours).
  - `pub struct FieldChange { pub field: String, pub outcome: FieldOutcome, pub baseline: Option<String>, pub yours: Option<String>, pub spec: Option<String> }`. `field` names: `name`, `method`, `url`, `description`, `header:<name>`, `query:<name>`, `path:<name>`, `form:<key>`, `body` (whole body), `body:$.a.b` (JSON key path), `auth`.
  - `pub enum Choice { Spec, Yours }`.
  - `pub struct MergeOutcome { pub merged: Request, pub changes: Vec<FieldChange>, pub cautious: bool }`. `changes` lists only fields where the spec's value differs from the baseline.
  - `pub fn merge_request(baseline: &Request, current: &Request, incoming: &Request, choices: &BTreeMap<String, Choice>) -> MergeOutcome`. Fields not in the synced set (see below) are copied from `current` untouched.
  - `pub fn request_is_edited(baseline: &Request, current: &Request) -> bool`: true when any field differs, ignoring `uid`, `file_name` and `seq`.
  - Synced fields: `name`, `method`, `url`, `description`, `headers`, `query_params`, `path_params`, `body`, `auth`. Everything else is never touched.

Merge rules (three-way with b = baseline, c = current, s = incoming):
1. Scalar field: `s == b` keeps c (no change listed); else `c == b` takes s (`TakeSpec`); else `c == s` keeps c; else keeps c (`KeepYours`).
2. Entries by name (headers case-insensitive, others exact), `value` and `enabled` merged separately by rule 1: an entry only in s is added (`TakeSpec`); an entry in b and s but not in c stays deleted (`KeepYours`); an entry in b but not in s is removed if c's entry equals b's (`TakeSpec`), or is a `Conflict` if the user changed it, and stays in c until a `Choice::Spec` removes it; an entry only in c is the user's and is kept untouched.
3. JSON body (all three bodies `Json` mode and valid JSON): merge objects by key path with the same rule per leaf; arrays and scalars are leaves; nesting deeper than 64 levels is a leaf. Add and remove of a key follow rule 2. When nothing from the spec is taken, `merged.body` is exactly `current.body`. A non-JSON body or any parse failure falls back to rule 1 on the whole body (`field: "body"`).
4. Form-data entries merge by key like rule 2.
5. Auth: `s == b` keeps c; if c's variant equals s's variant keep c (the user's values win); otherwise take s's auth (`TakeSpec`, marked so the UI can say the type changed).
6. `choices` overrides the default for the named `field`: `Choice::Spec` takes the spec's value for that field (including a removal), `Choice::Yours` keeps c. An unknown field name is ignored.
7. `cautious` is false here; Task 3 sets it.

- [ ] **Step 1: Write the failing tests** (table-driven where it reads better; each builds small `Request` values):
  - Scalars: `spec_change_is_taken_when_the_user_did_not_touch_it`; `user_value_wins_when_both_changed`; `same_change_on_both_sides_is_no_conflict`; `unchanged_spec_lists_nothing`.
  - Entries: `new_spec_header_is_added`; `user_added_header_survives`; `user_disabled_param_stays_disabled_when_the_spec_changes_its_value` (value taken, `enabled` kept); `spec_removes_an_untouched_param_so_it_is_removed`; `spec_removes_a_param_the_user_changed_is_a_conflict_and_kept_by_default`; `choice_spec_resolves_the_conflict_by_removing_it`; `header_names_match_case_insensitively`; `path_param_value_change_by_the_spec_is_taken_if_untouched`.
  - JSON: `new_key_in_the_spec_is_added_to_the_users_body`; `key_the_user_added_is_kept`; `user_edit_to_one_key_and_spec_edit_to_another_both_apply`; `spec_removes_a_key_the_user_changed_is_a_conflict`; `body_text_is_untouched_when_nothing_is_taken` (user's whitespace and key order preserved byte for byte); `invalid_user_json_falls_back_to_whole_body_rule`; `nesting_deeper_than_64_levels_does_not_overflow` (200 nested objects); `a_large_array_is_a_leaf` (100,000 numbers, finishes quickly).
  - Auth: `same_type_keeps_the_users_values`; `different_type_takes_the_specs_auth`; `spec_unchanged_keeps_the_users_auth`.
  - Never touched: `scripts_tests_docs_examples_settings_tags_and_variables_are_copied_from_current` (set every one on `current`, assert equal on `merged`).
  - Edited: `request_is_edited_ignores_uid_file_name_and_seq`; `request_is_edited_sees_a_script`; `request_is_edited_sees_an_example`; `identical_requests_are_not_edited`.
  - Choices: `unknown_choice_field_is_ignored`; `choice_spec_on_a_keep_yours_field_takes_the_specs_value`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-import openapi::sync`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** the Interfaces block and merge rules. Keep one small function per rule and one `fn merge_entries<T>` shared by headers, query params, path params and form data (they differ only in the key and the compared attributes).
- [ ] **Step 4: Run to verify they pass.** Run: `cargo test -j4 -p rocket-import openapi::sync` and `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): merge a synced request field by field`.

---

### Task 3: Build the sync plan

> Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-import/src/openapi/sync/plan.rs` and `crates/rocket-import/src/openapi/sync/rename.rs`; export from `sync/mod.rs` and `lib.rs`.
- Modify: `crates/rocket-import/CLAUDE.md` (a "Sync plan" section: inputs, outputs, the cautious mode, the rulings above).

**Interfaces:**
- Consumes: Task 1 `OpenApiBaseline`, `CONVERTER_VERSION`; Task 2 `merge_request`, `request_is_edited`, `MergeOutcome`; `OpenApiSource`, `OpenApiDoc`, `convert_operation`, `baseline_hash`, `security_variable_names`, `layout::folder_names`, `GroupBy`.
- Produces:
  - `pub struct CurrentRequest { pub uid: String, pub request_path: String, pub request: Request }` (the caller loads the collection's requests).
  - `pub struct SyncInputs<'a> { pub source: &'a OpenApiSource, pub baseline: Option<&'a OpenApiBaseline>, pub incoming: &'a OpenApiDoc, pub current: &'a [CurrentRequest], pub current_base_url: Option<&'a str>, pub existing_variables: &'a [String], pub skips: &'a BTreeMap<String, String> }` (`skips` maps operation key to the fingerprint it was skipped at).
  - `pub fn fingerprint(request: &Request) -> String`: hex SHA-256 over the synced fields (those of `baseline_hash` plus `name` and `description`).
  - `pub enum SyncItem { New { key, name, folder: Vec<String>, request: Request, fingerprint: String, possibly_renamed_from: Option<String> }, Changed { key, uid, request_path, name, merge: MergeOutcome, fingerprint: String }, Removed { key, uid, request_path, name, edited: Option<bool>, possibly_renamed_to: Option<String> } }`. For `Removed`, `edited` is `None` when it cannot be known (cautious mode), and the apply step must then treat it as edited.
  - `pub struct VariableChange { pub name: String, pub kind: VariableKind, pub yours: Option<String>, pub spec: Option<String> }` with `enum VariableKind { BaseUrl, AddSecurity }`.
  - `pub struct SyncPlan { pub items: Vec<SyncItem>, pub variable_changes: Vec<VariableChange>, pub skipped_hidden: usize, pub deleted_by_user: Vec<String>, pub cautious: bool, pub warnings: Vec<String> }`.
  - `pub fn build_sync_plan(inputs: &SyncInputs) -> SyncPlan`.

Plan rules:
1. Index `source.operations` by key and by `uid`, `current` by `uid`, the baseline by key, and the incoming operations by key, each once (hash maps); no nested scans.
2. Each incoming operation is converted with `convert_operation`. A key in the source and incoming: if the request's `uid` is not in `current`, the user deleted it, add the key to `deleted_by_user` and skip; otherwise merge (below). A key only incoming is `New`; the folder comes from `layout::folder_names` with the source's `group_by`. A key only in the source, with its request present, is `Removed`; with its request gone, it goes to `deleted_by_user`.
3. Merge, normal mode (baseline present, `converter_version` equals `CONVERTER_VERSION`, key present in the baseline): `merge_request(baseline_request, current, incoming, &{})`. An item is produced only when `changes` is not empty. `Removed.edited = Some(request_is_edited(baseline_request, current))`.
4. Cautious mode (no baseline, another `converter_version`, or the key missing from the baseline): the request counts as untouched on the synced fields only when `baseline_hash(current)` equals the source operation's stored `baseline_hash`. If untouched, use `current` as the baseline for the merge (every difference to the spec is a `TakeSpec`). If not, run the same merge, then set `merged` to `current` and mark every change `KeepYours`; a field the user then picks as `Choice::Spec` is applied by Plan 6 calling `merge_item(baseline, source_op, current, incoming, choices)`, which repeats the plan's trust and cautious decisions and applies only the chosen fields (calling `merge_request(current, current, ..)` directly would take every unchosen field from the spec). `MergeOutcome.cautious = true` and `Removed.edited = None`. Non-synced fields are never touched in either case. `plan.cautious` is true when any item used this mode.
5. Skips: an item whose key is in `skips` with an equal `fingerprint` is dropped and counted in `skipped_hidden`. A `Removed` item's fingerprint is the fixed string `"removed"`.
6. Rename hints (`rename.rs`): among `Removed` and `New` items, pair a Removed and a New item only when it is the single candidate on both sides and either (a) the method and the path template match but the keys differ, or (b) the method and the request name match but the path differs. Both items then carry the other's key. Anything ambiguous gets no hint. Linear time: group by (method, path template) and by (method, name) with hash maps.
7. Variables: compare the baseline `base_url`, `current_base_url` and the incoming `servers[0]` with the scalar rule (no baseline means treat the baseline as `current_base_url`); a security scheme whose variable names are not in `existing_variables` gives `AddSecurity` items.
8. Item order follows the incoming document order for `New` and `Changed`, then `Removed` in source order, so the output is stable.

- [ ] **Step 1: Write the failing tests** (build specs with `parse_openapi` from small inline YAML and requests with `convert_operation`, so the fixtures match what an import produces):
  - `unchanged_spec_gives_an_empty_plan`.
  - `new_operation_is_new_with_its_tag_folder_and_a_converted_request`; same with `GroupBy::Paths`.
  - `changed_summary_url_or_body_gives_a_changed_item_with_those_fields`.
  - `removed_operation_unedited_is_removed_with_edited_false`; `removed_operation_with_a_user_script_is_edited_true`.
  - `request_moved_to_another_folder_is_still_matched_by_uid`.
  - `request_the_user_deleted_is_listed_in_deleted_by_user_and_not_recreated`.
  - `skipped_change_is_hidden_until_the_fingerprint_changes` (hidden count 1; after the spec changes that endpoint again it shows).
  - `rename_hint_pairs_a_single_operation_id_change` and `rename_hint_pairs_a_single_path_change_with_the_same_summary`; `ambiguous_candidates_get_no_hint` (two removed, one new).
  - `no_baseline_is_cautious_and_never_overwrites_an_edited_request` (current hash differs from the stored one: every change is `KeepYours`, `Removed.edited == None`); `no_baseline_and_an_untouched_request_takes_the_specs_changes`.
  - `another_converter_version_is_cautious`; `key_missing_from_the_baseline_is_cautious_for_that_request_only`.
  - `base_url_change_is_listed_and_the_users_edit_wins`; `a_new_security_scheme_adds_its_variables`.
  - `plan_for_5000_operations_is_linear` (build 5,000 operations and requests; assert it completes in well under a second and the item count is right; no timing assertion tighter than 2 s).
  - `items_come_in_a_stable_order`.
- [ ] **Step 2: Run to verify they fail.** Run: `cargo test -j4 -p rocket-import openapi::sync::plan`. Expected: compile errors for the missing items.
- [ ] **Step 3: Implement** `build_sync_plan` following the rules in order, with one private function per rule (index, classify, merge-mode choice, skips, renames, variables).
- [ ] **Step 4: Run to verify they pass.** Run: `cargo test -j4 -p rocket-import openapi`, `cargo test -j4 -p rocket-infra openapi`, `cargo check -j4`. Expected: all pass, no warnings.
- [ ] **Step 5: Commit** with `dev-workflow-skills:1-git-commit`. Message: `feat(import): build a sync plan from a new spec`. Then update `00-plan-index.md` (P5 row done, rulings) and `docs/roadmaps/roadmap.md` R-70.

---

## Self-review notes

- Spec coverage: per-field merge and user-wins (Task 2); identity by key and uid, moves and renames (Task 3 rules 1 to 2); edited detection (Task 2 `request_is_edited`); removed endpoints edited or not (Task 3 rule 3); renames as a hint (rule 6); `baseUrl` change as a Variables item (rule 7); skips (rule 5); 5,000 operations (rule 1, the linear test). Not here: writing anything to requests, the staging area and recovery (Plan 6), listing the plan in the UI (Plans 8 and 9), the `Link as rename` action and `Choice` UI (Plan 9), where skips are stored (Plan 6).
- Open for Plan 6: apply a user's `Choice` with `merge_item`, not `merge_request`. Apply must advance the snapshot only for approved operations (add the new request, replace the baseline request, drop the removed key), and write `converter_version`. The snapshot of a partly applied sync therefore holds old and new baselines side by side, which is the point.
- Carry-over from the Plan 4 review that this plan touches: Plan 6 must verify `sha256(load_pending()) == pending_sha256` before applying.
