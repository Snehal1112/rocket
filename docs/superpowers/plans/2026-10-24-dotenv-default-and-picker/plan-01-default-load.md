# .env Default Load Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read `<collection>/.env` (and `<workspace>/.env` for global environments) as `{{process.env.NAME}}` with no path set.

**Architecture:** A new `DotEnvLoader::load_default` reads `<base>/.env` and treats a missing file as empty. `collect_dot_env` stays as is. `RequestExecutionService::process_env_for` adds the default layers underneath the explicit ones, and no longer needs an environment name to read them. The trust gate is unchanged.

**Tech Stack:** Rust (`rocket-environment`, `rocket-infra`, `rocket-app`), `dotenvy`.

**Spec:** `docs/superpowers/specs/2026-10-18-env-extends-dotenv-design.md`, amended by [index.md](index.md).

## Global Constraints

- Rust: no panicking unwrap calls in production paths; `cargo check -j4`, `cargo test -j4 -p <crate> <name>`; no workspace-wide test run.
- A denied trust gate reads no file at all (`.claude/collection-trust.md`).
- A relative `.env` path with `..` stays refused. The OS environment wins over every `.env` value.
- `.env` keys are reported in `dot_env_keys` so the Console masks them. Keep that for default-layer keys.
- Conventional commits; commit through the `dev-workflow-skills:1-git-commit` skill; add explicit paths, never `git add -A`.

## Review Focus

- A `.env` that is a directory, or unreadable: a warning, other layers still apply, no crash.
- A malformed default `.env`: a warning, the explicit layers still apply.
- No environment selected: the default file is still read (and masked).
- Gate denied: the default file is not read.
- Default and explicit path name the same file: no duplicate keys, no error.

---

### Task 1: Default file loader

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-environment/src/dot_env.rs`
- Modify: `crates/rocket-infra/src/fs_dot_env_loader.rs`
- Test: both files' `tests` modules

**Interfaces:**
- Consumes: `DotEnvLoader::load(raw_path, base)` and `resolve_dot_env_path` as they are.
- Produces: `fn load_default(&self, base: Option<&Path>) -> Result<Vec<(String, String)>, String>` on `DotEnvLoader` (a required method, so update the `Fake` in `dot_env.rs` tests and any other implementor), and `pub fn collect_default_dot_env(loader: &dyn DotEnvLoader, base: Option<&Path>) -> DotEnvValues` in `dot_env.rs`, exported from the crate root next to `collect_dot_env`.

- [ ] **Step 1: Write the failing tests** in `fs_dot_env_loader.rs`:
  - `load_default_reads_dot_env_in_the_base` (tempdir with `.env` holding `A=1`, expect `[("A","1")]`).
  - `load_default_of_a_missing_file_is_empty_not_an_error`.
  - `load_default_with_no_base_is_empty`.
  - `load_default_of_a_directory_named_dot_env_is_an_error`.
  - `load_default_of_a_malformed_file_is_an_error`.

  And in `dot_env.rs`: `collect_default_dot_env_returns_the_loader_values` and `collect_default_dot_env_turns_an_error_into_a_warning` (using `Fake`).

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-infra load_default` and `cargo test -j4 -p rocket-environment collect_default`. Expected: FAIL, not compiling.

- [ ] **Step 3: Implement** `load_default` in `FsDotEnvLoader`: join `.env` to the base, return `Ok(vec![])` on `ErrorKind::NotFound` or a `None` base, otherwise reuse the same read and parse code as `load` (extract it into a private `read_pairs(path) -> Result<..., String>` so both share it). Implement `collect_default_dot_env`: call `load_default`, put `Ok` pairs in `values`, put `Err` text in `warnings`.

- [ ] **Step 4: Run to verify they pass:** the two commands above plus `cargo check -j4`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(env): read a default .env file from the base folder` with the paths of the two source files.

---

### Task 2: Wire the default layers into process_env_for

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`process_env_for`, around line 1060)
- Modify: `src/components/environments/EnvironmentDialog.tsx` (placeholder only, line 548)
- Modify: `.claude/collection-trust.md`, `.claude/env-extends-dotenv-spec-notes.md`, `crates/rocket-environment/CLAUDE.md`, `.claude/roadmap.md` (R-29 note)
- Test: `crates/rocket-app/src/execution_service.rs` tests module (near line 7187, uses the existing fake loader)

**Interfaces:**
- Consumes: `collect_default_dot_env(loader, base) -> DotEnvValues` from Task 1, `pinned_collection_dir(&str) -> Option<PathBuf>`, `global_env_repo().dot_env_base()`.
- Produces: no new public names. `process_env_for(collection, environment_name, global_env_name)` keeps its signature and now also reads default files.

- [ ] **Step 1: Write the failing tests** (extend the fake loader in that module so `load_default` returns configured pairs per base):
  - `process_env_reads_the_collection_default_file_without_an_environment`.
  - `process_env_default_file_is_overridden_by_an_explicit_dot_env_path`.
  - `process_env_workspace_default_is_under_the_collection_default`.
  - `process_env_denied_gate_reads_no_default_file` (assert the fake's call log is empty).
  - `process_env_default_file_keys_are_listed_in_dot_env_keys`.
  - `process_env_os_value_beats_the_default_file`.

- [ ] **Step 2: Run to verify they fail:** `cargo test -j4 -p rocket-app process_env_`. Expected: the new tests FAIL.

- [ ] **Step 3: Implement.** In `process_env_for`, after the gate check and before the explicit layers, insert default values into `file_values`: first the workspace base (`self.global_env_repo().dot_env_base()`), then the collection folder (`collection.and_then(|c| self.pinned_collection_dir(c))`). Then the existing explicit layers `extend` over them. Log each warning with `tracing::warn!`. The default read must not depend on `environment_name` or `global_env_name`.

- [ ] **Step 4: Update the placeholder** in `EnvironmentDialog.tsx` to `.env (default: collection .env)`, and the dialog test that asserts the old placeholder if any (`yarn test EnvironmentDialog`).

- [ ] **Step 5: Update the docs.** In `.claude/collection-trust.md` line about `.env` files: add that the default `<collection>/.env` is read under the same gate. In `crates/rocket-environment/CLAUDE.md` `dot_env.rs` row: mention `collect_default_dot_env`. Add the amendments to `.claude/env-extends-dotenv-spec-notes.md`.

- [ ] **Step 6: Run to verify they pass:** `cargo test -j4 -p rocket-app process_env_`, `cargo check -j4`, `yarn tsc --noEmit`, `yarn check`. Expected: PASS.

- [ ] **Step 7: Commit** `feat(env): load the default .env from the collection and workspace folders`.
