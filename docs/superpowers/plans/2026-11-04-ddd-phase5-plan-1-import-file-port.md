# DDD Phase 5 Plan 1: import file port (D-51, the three writers)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `copy_collection_files`, `merge_root_defaults` and `copy_support_file` in `rocket-import` stop calling `std::fs` and go through a port implemented in `rocket-infra`.

**Architecture:** A small `ImportFiles` trait lives in `rocket-import`. `FsImportFiles` in `rocket-infra` implements it. `ImportService::new` takes an `Arc<dyn ImportFiles>` as a required argument (no fallback, as in D-44). Reads of the source tree outside the three writers (`read_dir` in scanners, zip, wsdl, openapi) stay for later plans.

**Tech Stack:** Rust, `tempfile` for fixtures.

**Spec:** `docs/roadmaps/ddd-roadmap.md` item D-51.

## Global Constraints

- Behavior is unchanged: same skip rules (`node_modules`, `workspace.yml`, `_order.yml`, non-yml), same report entries and texts.
- A symlink is never followed or copied (`copy_support_file`).
- No `unwrap` in production code. `cargo` commands take `-j4`.
- The trait lives in `rocket-import`; `rocket-infra` implements it (same shape as `CollectionFileSource`).

## Review Focus

- A source directory that cannot be listed still aborts the import with the same error as before.
- A dangling symlink named `x.proto` is reported, not copied, and does not abort.
- `opencollection.yml` with invalid YAML is reported as skipped, the import goes on.
- A destination that cannot be written reports `could not copy the .proto file: ...`.
- A non-Unicode file name is skipped like any non-yml file.

---

### Task 1: The `ImportFiles` port and `FsImportFiles`

**Files:**
- Create: `crates/rocket-import/src/files.rs`, `crates/rocket-infra/src/fs_import_files.rs`
- Modify: `crates/rocket-import/src/lib.rs`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-infra/Cargo.toml` (add `rocket-import.workspace = true` if the workspace has it, else a path dependency)

**Interfaces:**
- Produces, in `rocket_import`:
  ```rust
  pub enum EntryKind { File, Dir, Symlink, Other }
  pub struct DirEntryInfo { pub path: PathBuf, pub kind: EntryKind }
  pub trait ImportFiles: Send + Sync {
      fn list_dir(&self, dir: &Path) -> io::Result<Vec<DirEntryInfo>>;
      fn kind(&self, path: &Path) -> io::Result<EntryKind>; // does not follow symlinks
      fn create_dir_all(&self, dir: &Path) -> io::Result<()>;
      fn copy(&self, src: &Path, dest: &Path) -> io::Result<()>;
      fn read_to_string(&self, path: &Path) -> io::Result<String>;
      fn write(&self, path: &Path, text: &str) -> io::Result<()>;
  }
  ```
- Produces `rocket_infra::FsImportFiles` (unit struct) implementing it with `std::fs`. `list_dir` reports a symlink to a directory as `Symlink`.

- [ ] **Step 1: Write the failing tests** in `fs_import_files.rs`: `list_dir_reports_files_dirs_and_symlinks`, `kind_does_not_follow_a_symlink`, `copy_and_write_round_trip_through_read_to_string`, `create_dir_all_makes_nested_dirs`, `list_dir_of_a_missing_dir_errors`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-infra fs_import_files` — Expected: compile error (red).
- [ ] **Step 3: Implement** the trait in `files.rs` (re-exported from `lib.rs`) and `FsImportFiles` with `std::fs`.
- [ ] **Step 4: Run** the same command — Expected: 5 passed.
- [ ] **Step 5: Commit** (`refactor(import): add the ImportFiles port`).

### Task 2: Route the three writers through the port

**Files:**
- Modify: `crates/rocket-import/src/importer.rs`, `crates/rocket-import/tests/roundtrip_test.rs`, `crates/rocket-import/tests/wsdl_integration_test.rs`, `crates/rocket-app/tests/postman_round_trip.rs`, `crates/rocket-app/src/openapi_sync_apply_tests.rs`, `src-tauri/src/import_wiring.rs`

**Interfaces:**
- Consumes: `ImportFiles` (Task 1).
- Produces: `ImportService::new(workspace_path, collection_repo, env_factory, files: Arc<dyn ImportFiles>)`. `copy_support_file` and `merge_root_defaults` become methods taking `&dyn ImportFiles` (free functions with an extra `files` argument are fine).

- [ ] **Step 1: Write failing tests** in `importer.rs` with an in-memory `FakeFiles`: `a_support_file_that_cannot_be_copied_is_reported` (fake `copy` fails; report has `could not copy the .proto file: ...`), `a_symlinked_proto_is_reported_not_copied` (fake `kind` returns `Symlink`), `an_unparseable_root_file_is_reported_and_the_import_goes_on`, `an_unlistable_source_dir_aborts_the_import`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-import importer` — Expected: compile error (red).
- [ ] **Step 3: Implement**: add the `files` field and constructor argument; replace the `std::fs` calls at `importer.rs` lines ~1101-1250 and the `copy_support_file` call in `import_auto_from_zip` with the port. Update the six `ImportService::new` call sites and the `#[cfg(test)]` constructor (use `Arc::new(rocket_infra::FsImportFiles)`). `src-tauri/src/import_wiring.rs` passes `Arc::new(rocket_infra::FsImportFiles)`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-import`, `cargo test -j4 -p rocket-app openapi_sync`, `cargo test -j4 -p rocket-app --test postman_round_trip`, `cargo check -j4 --all-targets` — Expected: all pass.
- [ ] **Step 5: Commit** (`refactor(import): write imported files through a port`).

### Task 3: Docs and roadmap

**Files:**
- Modify: `crates/rocket-import/CLAUDE.md` (if present, else create a short one), `crates/rocket-infra/CLAUDE.md` (`FsImportFiles` row), `docs/roadmaps/ddd-roadmap.md` (D-51 partly done: the three writers, remaining sites listed).

- [ ] **Step 1: Count remaining `std::fs` production sites** in `rocket-import/src` with grep and list them in the roadmap row.
- [ ] **Step 2: Edit the docs.** Expected: `grep -n "std::fs" crates/rocket-import/src/importer.rs` shows no site in the three writers.
- [ ] **Step 3: Run** `cargo clippy -j4 -p rocket-import -p rocket-infra --all-targets` — Expected: no new warnings.
- [ ] **Step 4: Commit** (`docs: mark the D-51 writers done`).
