# Environment .env files as process.env Implementation Plan

> **Executor:** the factory driver runs this plan using superpowers:subagent-driven-development, with one fresh factory-builder implementer per task and no per-task review. The driver keeps the ledger, makes the commits and runs the final review.

**Goal:** The file named by `dotEnvFilePath` is read, layered along the `extends` chain, and offered as `{{process.env.NAME}}` under the host environment gate.

**Architecture:** `rocket-environment` gets a `DotEnvLoader` trait and a pure `collect_dot_env` that layers the chain. `rocket-infra` implements the loader with `dotenvy` and the path rules. `rocket-app` builds one `ProcessEnv` (OS environment plus `.env` values) behind `process_env_allowed`, and the three backend protocols use it.

**Tech Stack:** Rust, `dotenvy` (parse only), `rocket-environment`, `rocket-infra`, `rocket-app`, `src-tauri`.

**Spec:** `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`

## Global Constraints

- Values appear as `{{process.env.NAME}}` only, never as plain variables.
- A relative `dotEnvFilePath` resolves against the collection folder (a global environment's against the workspace folder). Absolute and `~/` paths are used as written. A path with a `..` segment is refused. A missing or unreadable file only warns.
- Layers run from the furthest ancestor's file to the child's file, and the child's value wins.
- The real OS environment wins over a `.env` value with the same name.
- When the host environment gate denies access, the `.env` file is not read at all.
- Use `dotenvy` to parse, without loading anything into the process environment.
- No panicking unwrap calls in production paths. No `git` CLI.
- Cargo commands are per crate: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`.

## Post-Implementation Review

Run after Task 3, on the finished `.env` backend, before the next plan starts.

- [ ] `cargo test -j4 -p rocket-environment dot_env`, `cargo test -j4 -p rocket-infra dot_env` and `cargo test -j4 -p rocket-app process_env` pass. Evidence: the test output.
- [ ] With the gate denied, no file is opened. Evidence: `a_denied_gate_returns_empty_vars_and_never_calls_the_loader`, plus a check that `FsDotEnvLoader::load` has no caller outside `process_env_for`.
- [ ] The loader never changes the process environment and `dotenvy`'s loading function is not called anywhere. Evidence: `loading_does_not_change_the_process_environment` and a grep for `dotenvy::dotenv` and `from_path`.
- [ ] A path with a `..` segment is refused, and a missing file only warns. Evidence: the named tests.
- [ ] Precedence is OS over collection file over global file, and parent file under child file. Evidence: the named tests.
- [ ] The `dotenvy` version in `Cargo.toml` is at least two weeks old and `cargo check -j4 -p rocket-infra` and `cargo check -j4 -p rocket-app` are clean. Evidence: the version and the command output.

## Review Focus

- A `.env` line with quotes, `#` inside quotes, an `export ` prefix or an empty value: parsed as `dotenvy` does (Task 2).
- A `.env` file that is not valid UTF-8 or is a directory: a warning, no failure (Task 2).
- A cycle in the chain while collecting `.env`: the walk stops and warns, never loops (Task 1).
- A key present in the OS environment and in two `.env` files: the OS value wins (Task 3).
- A denied gate with a `.env` file that does not exist and one that does: neither is read and no warning about the file appears (Task 3).

## File Structure

- Create `crates/rocket-environment/src/dot_env.rs`: `DotEnvLoader`, `DotEnvValues`, `collect_dot_env`.
- Modify `crates/rocket-environment/src/repository.rs`: `EnvironmentRepository::dot_env_base`.
- Modify `crates/rocket-environment/src/lib.rs`: exports.
- Create `crates/rocket-infra/src/fs_dot_env_loader.rs`: `FsDotEnvLoader`. Modify `crates/rocket-infra/Cargo.toml`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-infra/src/shared_path_environment_repo.rs` (`dot_env_base`).
- Modify `crates/rocket-app/src/execution_service.rs` (a `dot_env_loader` field, `process_env_for`, `build_variable_context_with_process_env`) and `src-tauri/src/lib.rs` (wire the loader).

---

### Task 1: Layering the chain in the domain crate

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-environment/src/dot_env.rs`
- Modify: `crates/rocket-environment/src/repository.rs`, `crates/rocket-environment/src/lib.rs`
- Test: inline tests in `dot_env.rs`

**Interfaces:**
- Consumes: `Environment` (`extends`, `dot_env_file_path`, `name`).
- Produces:
  - `pub trait DotEnvLoader: Send + Sync { fn load(&self, raw_path: &str, base: Option<&Path>) -> Result<Vec<(String, String)>, String>; }` (the `Err` text is a warning message).
  - `pub struct DotEnvValues { pub values: Vec<(String, String)>, pub warnings: Vec<String> }`
  - `pub fn collect_dot_env(env: &Environment, lookup: &dyn Fn(&str) -> Option<Environment>, loader: &dyn DotEnvLoader, base: Option<&Path>) -> DotEnvValues`
  - `EnvironmentRepository::dot_env_base(&self) -> Option<PathBuf>` with a default of `None`.

- [ ] **Step 1: Write the failing tests** with a fake loader backed by a `HashMap<String, Vec<(String, String)>>`:
  - `child_file_wins_over_parent_file`: parent `A=1`, child `A=2` gives `A=2` once.
  - `parent_file_is_read_through_the_chain`: parent `B=1`, child `A=2` gives both.
  - `environment_without_a_path_adds_nothing_but_its_parents_still_count`.
  - `a_loader_error_becomes_a_warning_and_other_files_still_load`.
  - `a_cycle_stops_the_walk_with_a_warning`.
  - `the_base_is_passed_to_the_loader_unchanged`.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-environment dot_env`.

- [ ] **Step 3: Implement** `collect_dot_env` with the same chain walk as `flatten_environment` (visited set, furthest ancestor first, later layers replace by key). Add the `dot_env_base` default method and the exports.

- [ ] **Step 4: Run to verify they pass:** `cargo test -j4 -p rocket-environment dot_env`, then `cargo check -j4 -p rocket-environment`. Expected: PASS.

---

### Task 2: The filesystem loader

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-infra/src/fs_dot_env_loader.rs`
- Modify: `crates/rocket-infra/Cargo.toml` (add `dotenvy`), `crates/rocket-infra/src/lib.rs`, `crates/rocket-infra/src/shared_path_environment_repo.rs`
- Test: inline tests using `tempfile`

**Interfaces:**
- Consumes: `DotEnvLoader` from Task 1, `rocket_shared::paths::expand_tilde(&str) -> PathBuf`.
- Produces:
  - `pub struct FsDotEnvLoader;` implementing `DotEnvLoader`.
  - `pub(crate) fn resolve_dot_env_path(raw: &str, base: Option<&Path>) -> Result<PathBuf, String>`.
  - `SharedPathEnvironmentRepo::dot_env_base` returns the workspace folder.

- [ ] **Step 1: Write the failing tests**
  - `a_relative_path_resolves_against_the_base`, `an_absolute_path_is_used_as_written`, `a_tilde_path_is_expanded`.
  - `a_path_with_a_parent_segment_is_refused`.
  - `a_relative_path_without_a_base_is_an_error`.
  - `quotes_export_prefix_and_empty_values_parse_like_dotenvy`.
  - `a_missing_file_is_an_error_text_naming_the_path`, `a_directory_is_an_error_text`, `a_file_that_is_not_utf8_is_an_error_text`.
  - `loading_does_not_change_the_process_environment`: a key read from the file is not in `std::env::var`.
  - `the_global_repository_reports_the_workspace_folder_as_its_base`.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-infra dot_env`.

- [ ] **Step 3: Implement** `FsDotEnvLoader::load`: resolve the path, read the file, parse with `dotenvy::Iter` over the bytes and collect the pairs without calling its `load`. Map every failure to an `Err(String)`. Add the `dotenvy` dependency using the version already pinned by Cargo.lock for the workspace if present, otherwise the latest published at least two weeks old.

- [ ] **Step 4: Run to verify they pass:** `cargo test -j4 -p rocket-infra dot_env`, then `cargo check -j4 -p rocket-infra`. Expected: PASS.

---

### Task 3: The gated process environment

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (struct field and builder, `build_variable_context_with_process_env` at about 988)
- Modify: `src-tauri/src/lib.rs` (wire `FsDotEnvLoader`)
- Test: inline tests in `execution_service.rs` with a fake `DotEnvLoader`

**Interfaces:**
- Consumes: `collect_dot_env`, `DotEnvLoader`, `process_env_allowed(&self, Option<&str>) -> bool`, `regular_env_repo`, `global_env_repo`, `pinned_collection_dir`.
- Produces:
  - `pub struct ProcessEnv { pub vars: HashMap<String, String>, pub dot_env_keys: Vec<String> }`
  - `pub fn process_env_for(&self, collection: Option<&str>, environment_name: Option<&str>, global_env_name: Option<&str>) -> ProcessEnv`
  - `RequestExecutionService::with_dot_env_loader(self, Box<dyn DotEnvLoader>) -> Self`. Without a loader no `.env` file is read.

- [ ] **Step 1: Write the failing tests**
  - `dot_env_values_appear_when_the_gate_allows`.
  - `the_os_environment_wins_over_a_dot_env_value`.
  - `a_denied_gate_returns_empty_vars_and_never_calls_the_loader` (the fake counts calls).
  - `the_collection_environment_file_wins_over_the_global_environment_file`.
  - `dot_env_keys_lists_only_keys_whose_value_came_from_a_file`.
  - `a_global_environment_relative_path_uses_the_repository_base`.
  - `without_a_loader_only_the_os_environment_is_returned`.
  - `build_variable_context_with_process_env_exposes_dot_env_values_as_process_env_keys`.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-app process_env`.

- [ ] **Step 3: Implement** `process_env_for`: return an empty `ProcessEnv` when `process_env_allowed(collection)` is false, before any file work. Otherwise collect the global environment's values (base `dot_env_base()` of the global repo), then the collection environment's (base `pinned_collection_dir(collection)`), the collection's winning, then fill the map with `.env` values and finally overwrite with `std::env::vars()`. Log each warning with the crate's existing logging macro. Make `build_variable_context_with_process_env` use `process_env_for(...).vars`. Wire `.with_dot_env_loader(Box::new(FsDotEnvLoader))` where the service is built in `src-tauri/src/lib.rs`.

- [ ] **Step 4: Run to verify they pass:** `cargo test -j4 -p rocket-app process_env`, `cargo check -j4 -p rocket-app`, `cargo check -j4 -p rocket-infra`. Expected: PASS.

Next plan: docs/superpowers/plans/2026-10-18-env-extends-dotenv/plan-03-env-extends-dotenv.md
