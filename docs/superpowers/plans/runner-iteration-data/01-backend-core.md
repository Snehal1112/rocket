# Runner iteration data: backend core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Parse CSV and JSON data files in Rust, carry a row on every send through `ExecuteRequestInput.iteration`, expose a `load_iteration_data` command, and loop rows in `CollectionRunnerService`.

**Architecture:** DDD placement: the pure parser, the saved-path rule and the `DataFileReader` port live in `rocket-collection` (domain, no I/O). The filesystem implementation lives in `rocket-infra`. `IterationDataService` (orchestration) and the outer row loop live in `rocket-app`. The shared `IterationInfo` type lives in `rocket-scripting`. Scripts do not read the row until plan 02.

**Tech Stack:** Rust, the `csv` crate (new dependency of `rocket-collection`, a pure parser), `serde_yaml`, `tempfile`, `wiremock`.

**Spec:** `docs/superpowers/specs/2026-11-12-runner-iteration-data-design.md`

## Global Constraints

- Formats: CSV with a header row (RFC 4180, UTF-8, a leading BOM stripped) or a JSON array of objects. Format comes from the extension (`.csv`, `.json`), case-insensitive. Any other extension is an error.
- Values are strings: `null` becomes `""`, numbers and booleans become their JSON text, nested values become compact JSON text.
- CSV: a short row pads with `""`, a long row is an error naming the row, a blank line is skipped, a duplicate header is an error.
- Limits: `MAX_DATA_FILE_BYTES = 10 * 1024 * 1024`, `MAX_DATA_ROWS = 1000`. An empty file (no rows) is an error.
- Saved default key: `extensions.rocketapi.runner.dataFile`, a relative path with `/` separators inside the collection directory. No absolute path, no `..`, no symlink that leaves the collection root.
- Event wire format: the tag stays camelCase, new variant fields are snake_case (`total_iterations`, `iteration`). Update the `wire_contract_*` tests.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, never on persistence structs. Production paths return errors and never panic (no unwrap calls).
- A new Tauri command is also added to `src-tauri/app-commands.txt`.
- DDD: domain crates hold logic and traits only. `rocket-infra` implements ports and defines none. `rocket-app` holds `Box<dyn Trait>` and does no file reads. The `rocket-collection` crate must not depend on `rocket-app` or `rocket-infra`.
- Tasks that touch collection settings or `opencollection.yml` start with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Review Focus

- A CSV field with a quoted newline and an embedded comma stays one cell (task 1 test).
- A one-row file runs one pass but still sets `iteration` (`total` 1), unlike no file (task 3 test).
- A JSON row with a nested object or array arrives as compact JSON text (task 1 test).
- A saved default that is a symlink to a file outside the collection is rejected (task 2 test).
- A settings save keeps an invalid or non-string saved value unchanged (task 2 test).

---

### Task 1: Parser and the `iteration` field

**Files:**
- Create: `crates/rocket-collection/src/iteration_data.rs`
- Modify: `crates/rocket-collection/src/lib.rs` (module and re-exports), `crates/rocket-collection/Cargo.toml` (add `csv = "1"`)
- Modify: `crates/rocket-scripting/src/context.rs` (add `IterationInfo`), `crates/rocket-scripting/src/lib.rs` (re-export)
- Modify: `crates/rocket-app/src/execution_service.rs` (field on `ExecuteRequestInput`, struct literal at about line 4946 and the other literals the compiler flags), `crates/rocket-app/src/runner_sequence.rs` (`build_step_input` literal), `crates/rocket-app/src/execution_service/script_host.rs` (`run_request`)
- Test: unit tests in `iteration_data.rs`; one test in `script_host_tests.rs`

**Interfaces:**
- Produces, in `rocket-scripting`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct IterationInfo {
      pub index: usize,
      pub total: usize,
      pub data: std::collections::BTreeMap<String, String>,
  }
  ```
- Produces, in `rocket-collection` (`iteration_data.rs`):
  ```rust
  pub const MAX_DATA_FILE_BYTES: usize = 10 * 1024 * 1024;
  pub const MAX_DATA_ROWS: usize = 1000;
  pub type IterationRow = std::collections::BTreeMap<String, String>;
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct IterationData { pub columns: Vec<String>, pub rows: Vec<IterationRow> }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum DataFormat { Csv, Json }
  pub fn format_for_path(path: &str) -> DomainResult<DataFormat>;
  pub fn parse_iteration_data(format: DataFormat, text: &str) -> DomainResult<IterationData>;
  pub fn validate_data_file_path(path: &str) -> DomainResult<()>;  // saved default only: non-empty, relative, no `..`, `/` separators
  ```
  `rocket-collection` and `rocket-scripting` do not depend on each other, so the mapping to `IterationInfo` is not here. It is `iteration_info` in `rocket-app` (task 2).
- Produces, in `rocket-app`: `ExecuteRequestInput.iteration: Option<IterationInfo>` with `#[serde(default)]`, `None` in `build_step_input` and every existing literal.

- [ ] **Step 1: Write the failing parser tests** in `iteration_data.rs`, named: `csv_reads_header_and_rows`, `csv_strips_a_leading_bom`, `csv_keeps_quoted_newline_and_comma_in_one_cell`, `csv_pads_a_short_row_with_empty_strings`, `csv_rejects_a_long_row_and_names_the_row`, `csv_skips_blank_lines`, `csv_rejects_a_duplicate_header`, `json_stringifies_scalars_null_and_nested_values` (assert `3`, `true`, `""`, and `{"a":1}` text), `json_rejects_a_non_array_top_level`, `json_row_without_a_key_has_no_entry_for_it`, `empty_data_is_an_error`, `over_1000_rows_is_an_error`, `format_for_path_is_case_insensitive_and_rejects_other_extensions`, `validate_data_file_path_rejects_empty_absolute_dotdot_and_backslash_paths`, `validate_data_file_path_accepts_a_nested_relative_path`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-collection iteration_data` and confirm the failures are "not defined".
- [ ] **Step 3: Implement** the interfaces above. Use the `csv` crate with `has_headers(true)` and `flexible(true)`, then check cell counts yourself. Every error is `DomainError::InvalidInput` with a message that names the row number (1-based, header excluded) when there is one.
- [ ] **Step 4: Add `IterationInfo`** to `rocket-scripting` and re-export it from `rocket_scripting`.
- [ ] **Step 5: Add `iteration: Option<IterationInfo>`** to `ExecuteRequestInput` after `run_source`, with `#[serde(default)]`. Set `iteration: None` in `build_step_input` and fix every other literal the compiler flags (`cargo check -j4 -p rocket-app --tests`).
- [ ] **Step 6: Write the failing test** `run_request_e2e_the_nested_run_inherits_the_callers_iteration` in `script_host_tests.rs`: a caller input with `iteration: Some(IterationInfo { index: 1, total: 3, data })`, then assert the nested `ExecuteRequestInput` handed to the executor carries the same value.
- [ ] **Step 7: In `ExecutionScriptHost::run_request`**, set `nested.iteration = self.input.iteration.clone()` right after `build_step_input`.
- [ ] **Step 8: Run** `cargo test -j4 -p rocket-collection iteration_data` and `cargo test -j4 -p rocket-app run_request_e2e` and confirm all pass.
- [ ] **Step 9: Commit** with the git-commit skill, paths: the files listed above. Message type `feat(runner)`.

### Task 2: Saved default, reader and the command

**Files:**
- Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- Modify: `crates/rocket-collection/src/settings.rs` (field), `crates/rocket-infra/src/fs_collection/settings.rs` (read, write, round trip)
- Create: `crates/rocket-infra/src/fs_data_file_reader.rs`, `crates/rocket-app/src/iteration_data_service.rs`
- Modify: `crates/rocket-collection/src/iteration_data.rs` (the port and its types), `crates/rocket-collection/src/lib.rs`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-app/src/lib.rs`, `src-tauri/src/lib.rs` (wire the service and register the command), `src-tauri/src/commands/runner.rs` (new command), `src-tauri/app-commands.txt`
- Test: `crates/rocket-infra/src/fs_collection/settings.rs` tests, `crates/rocket-infra/src/fs_data_file_reader.rs` tests, `src-tauri/src/ipc_acl_tests.rs` (existing test covers the file)

**Interfaces:**
- Consumes (task 1): `IterationData`, `parse_iteration_data`, `format_for_path`, `validate_data_file_path`, `MAX_DATA_FILE_BYTES`.
- Produces:
  ```rust
  // rocket-collection, field on CollectionSettings (add after script_flow):
  pub runner_data_file: Option<String>,   // #[serde(default)]
  // rocket-collection, iteration_data.rs (the port is defined in the domain crate):
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum DataFileSource { PerRun, SavedDefault }
  pub struct DataFileText { pub file_name: String, pub text: String, pub source: DataFileSource }
  pub trait DataFileReader: Send + Sync {
      fn read(&self, collection: &str, per_run_path: Option<&str>) -> DomainResult<Option<DataFileText>>;
  }
  // rocket-app, iteration_data_service.rs (orchestration, no file reads):
  pub fn iteration_info(data: &IterationData, index: usize) -> rocket_scripting::IterationInfo;  // total = rows.len()
  pub struct LoadedIterationData { pub data: IterationData, pub file_name: String, pub source: DataFileSource }
  pub struct IterationDataService { /* Box<dyn DataFileReader> */ }
  impl IterationDataService {
      pub fn new(reader: Box<dyn DataFileReader>) -> Self;
      pub fn load(&self, collection: &str, per_run_path: Option<&str>) -> DomainResult<Option<LoadedIterationData>>;
  }
  // rocket-infra:
  pub struct FsDataFileReader { /* FsCollectionRepo handle */ }
  // src-tauri, IPC DTO (camelCase):
  pub struct IterationDataDto { pub columns: Vec<String>, pub rows: Vec<BTreeMap<String,String>>, pub source: String /* "perRun" | "savedDefault" */, pub file_name: String }
  // command: load_iteration_data(collection: String, per_run_path: Option<String>) -> Result<Option<IterationDataDto>, DomainError>
  ```

- [ ] **Step 1: Write failing settings tests** in `fs_collection/settings.rs`: `runner_data_file_reads_a_string_at_extensions_rocketapi_runner_datafile`, `runner_data_file_is_none_for_absent_or_non_string_values`, `set_runner_data_file_keeps_sibling_extension_keys`, `set_runner_data_file_none_removes_the_key_and_prunes_empty_mappings`, `a_non_string_saved_value_is_left_unchanged_when_settings_hold_none`. Model them on the `script_flow` tests (lines about 467 to 540 of that file).
- [ ] **Step 2: Implement** `runner_data_file_from_extensions(&Option<serde_yaml::Value>) -> Option<String>` and `set_runner_data_file_in_extensions(Option<serde_yaml::Value>, &Option<String>) -> Option<serde_yaml::Value>` mirroring `script_flow_from_extensions` and `set_script_flow_in_extensions`: return the input untouched when it already reads as the new value. Wire both into `get_settings` and the settings save path next to `script_flow`.
- [ ] **Step 2b: Write failing service tests** in `iteration_data_service.rs` with a fake `DataFileReader`: `per_run_path_is_passed_to_the_reader`, `a_reader_returning_none_gives_none`, `a_wrong_extension_is_rejected_after_reading`, `iteration_info_carries_index_total_and_the_row`. The service never touches the file system itself.
- [ ] **Step 3: Write failing reader tests** in `fs_data_file_reader.rs` using `tempfile`: `per_run_path_is_read_as_given`, `saved_default_is_read_inside_the_collection`, `saved_default_with_dotdot_is_rejected`, `saved_default_absolute_is_rejected`, `saved_default_symlink_out_of_the_collection_is_rejected` (unix only, `#[cfg(unix)]`), `neither_path_set_returns_none`, `a_file_over_the_size_limit_is_rejected_before_reading_it_all` (check metadata length first), `a_missing_file_names_the_path_in_the_error`.
- [ ] **Step 4: Implement** `FsDataFileReader` and `IterationDataService::load` (per-run path wins; otherwise the saved default; `format_for_path` then `parse_iteration_data`). The reader calls the pure `validate_data_file_path` first, then does only the I/O checks: canonicalize the saved path, require it to start with the canonical collection directory, and check the file size before reading. Build the reader on the workspace-following collection repository handle (the `SharedPath*` handle the runner's send pins use, see R-52), not a fixed-path `FsCollectionRepo`, so a workspace switch cannot make a run read another workspace's file.
- [ ] **Step 5: Add the command** `load_iteration_data` in `src-tauri/src/commands/runner.rs`: thin, calls the service from managed state, maps to `IterationDataDto`. Register it in `src-tauri/src/lib.rs`, build the service there with `FsDataFileReader`, and add `load_iteration_data` to `src-tauri/app-commands.txt`.
- [ ] **Step 6: Run** `cargo test -j4 -p rocket-infra settings`, `cargo test -j4 -p rocket-infra fs_data_file_reader`, `cargo test -j4 -p rocket-collection iteration_data`, `cargo test -j4 -p rocket-app iteration_data_service`, `cargo check -j4 -p <tauri package>` (use the real package name from `src-tauri/Cargo.toml`) and the ACL test in `src-tauri` (`cargo test -j4 -p <tauri package> ipc_acl`). All pass.
- [ ] **Step 7: Commit** with the git-commit skill. Type `feat(runner)`.

### Task 3: Row loop in `CollectionRunnerService`

**Files:**
- Modify: `crates/rocket-app/src/collection_runner_service.rs` (input field, builder, loop, result field), `crates/rocket-app/src/execution_events.rs` (two fields and the contract tests), `crates/rocket-app/src/runner_sequence.rs` (set `iteration` on the step input)
- Test: `collection_runner_service.rs` tests (use the existing `harness`, `three_step_collection`, `sample_run_input`, `ProgrammableEngine`, `RecordingExecutor`)

**Interfaces:**
- Consumes: `IterationDataService`, `LoadedIterationData`, `iteration_info` (task 2).
- Produces:
  ```rust
  // RunCollectionInput: #[serde(default)] pub data_file: Option<String>,
  // RunStepResult:      pub iteration: usize,   // zero-based pass
  // CollectionRunnerService:
  pub fn with_iteration_data(self, service: IterationDataService) -> Self;
  // ExecutionEvent::RunnerStarted gains `total_iterations: usize`
  // ExecutionEvent::RunnerStepCompleted gains `iteration: usize`
  ```

- [ ] **Step 1: Write failing tests**: `no_data_file_runs_one_pass_with_no_iteration_on_the_sends`, `a_one_row_file_runs_one_pass_with_iteration_total_1`, `three_rows_run_the_whole_set_three_times_in_order_with_the_right_iteration` (assert each send's `iteration.index` and `.data`), `set_next_request_jumps_stay_inside_a_pass`, `stop_execution_ends_all_passes`, `stop_on_failure_ends_all_passes`, `runtime_variables_carry_across_passes`, `the_step_cap_counts_steps_across_all_rows` (reports `StepLimitReached`), `a_bad_data_file_fails_before_runner_started_and_sends_nothing`, `cancelling_in_the_second_pass_ends_the_run`. Use a fake `DataFileReader` returning fixed text.
- [ ] **Step 2: Update the `wire_contract_*` test** cases in `execution_events.rs` for `RunnerStarted` (`"total_iterations":3`) and `RunnerStepCompleted` (`"iteration":1`), and confirm they fail.
- [ ] **Step 3: Implement.** Resolve the data once, after `flatten_run_set` and before `RunnerStarted`, using `input.data_file` then the saved default (the service does both). Wrap the existing `while cursor < items.len()` loop in `for iteration in 0..total_passes`, resetting `cursor` to 0 each pass and leaving `carried_runtime` and `steps` outside the loop. Break out of both loops for stop-by-script, stop-on-failure, cancel and the step cap. `build_step_input` callers set `step_input.iteration` from the pass; with no data file `total_passes` is 1 and `iteration` stays `None`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-app collection_runner_service` and `cargo test -j4 -p rocket-app execution_events`. All pass, including the old tests.
- [ ] **Step 5: Wire** `with_iteration_data` where `CollectionRunnerService` is built in `src-tauri/src/lib.rs`, reusing the service from task 2. Run `cargo check -j4 -p <tauri package>`.
- [ ] **Step 6: Commit** with the git-commit skill. Type `feat(runner)`.
