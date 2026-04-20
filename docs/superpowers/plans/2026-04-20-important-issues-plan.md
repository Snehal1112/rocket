# Important Issues Implementation Plan

Addresses 7 "should fix" issues found during the backend review. All fixes are
surgical: minimal scope, no new abstractions beyond what each issue requires.

---

## Issue Index

| # | Short name | Affected crate(s) | Risk |
|---|---|---|---|
| I1 | rocket-import infra leak | rocket-import, rocket-infra | medium |
| I2 | workspace_service I/O | rocket-app | low |
| I3 | command-layer I/O + `.expect()` | src-tauri | low |
| I4 | workspace mutex `.unwrap()` | src-tauri | medium |
| I5 | `DomainError::Conflict` variant | rocket-shared, rocket-git | low |
| I6 | audit log O(n) latest() | rocket-infra | low |
| I7 | history search full-load | rocket-infra | low |

---

## Task 1 — I1: Remove direct infra dependency from rocket-import

**Problem:**
`rocket-import/Cargo.toml` depends directly on `rocket-infra`, and
`importer.rs:5` imports `FsCollectionRepo` and `FsEnvironmentRepo` as concrete
types. This violates the DDD boundary: `rocket-import` is a domain-layer crate
and must not know about filesystem implementations.

**Approach:**
Inject the two repository traits (`CollectionRepository`, `EnvironmentRepository`)
into `ImportService` instead of constructing `Fs*Repo` instances internally.
Remove the `rocket-infra` Cargo dependency entirely.

**Files to change:**

1. `crates/rocket-import/Cargo.toml`
   - Remove `rocket-infra = { path = "../rocket-infra" }` from `[dependencies]`
   - Keep `rocket-collection`, `rocket-environment` (already present)

2. `crates/rocket-import/src/importer.rs`
   - Remove `use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};`
   - Change `ImportService` struct:
     ```rust
     pub struct ImportService {
         workspace_path: PathBuf,
         collection_repo: Box<dyn CollectionRepository>,
         env_repo: Box<dyn EnvironmentRepository>,
     }
     ```
   - Update `new()` and `new_with_workspace_path()` — these can no longer
     construct concrete repos; remove them or make them private test helpers
   - Add a new primary constructor:
     ```rust
     pub fn new(
         workspace_path: PathBuf,
         collection_repo: Box<dyn CollectionRepository>,
         env_repo: Box<dyn EnvironmentRepository>,
     ) -> Self
     ```
   - Remove `make_collection_repo()` and `make_env_repo()` helper methods
     (lines 375–382); replace their call-sites with `&self.collection_repo` /
     `&self.env_repo`
   - The `default_workspace_path()` helper function can be deleted too, or kept
     as a module-private utility called only from tests if needed

3. `src-tauri/src/commands/import.rs`
   - Update `import_bruno` and `import_bruno_zip` command handlers to construct
     `FsCollectionRepo` and `FsEnvironmentRepo` from the workspace path and pass
     them into `ImportService::new(…)`

**Tests:**
- `cargo check -p rocket-import` must pass with no `rocket_infra` symbol in scope
- `cargo test -p rocket-import` (existing integration tests) must still pass
- Verify `rocket-infra` no longer appears in `rocket-import`'s `Cargo.lock`
  transitive deps

---

## Task 2 — I2: Remove raw I/O from workspace_service

**Problem:**
`crates/rocket-app/src/workspace_service.rs` calls `fs::create_dir_all`
(lines 41, 46, 49) and `fs::read_to_string` (line 266) directly. The app layer
must not perform raw I/O; all filesystem work goes through repository traits.

**Approach:**
Add two new methods to the `WorkspaceRepository` trait and move the I/O into
`FsWorkspaceRepo`. For `read_to_string`, identify which file is being read and
add a corresponding method to `WorkspaceConfigRepository` (or keep it on
`WorkspaceRepository`).

**Files to change:**

1. `crates/rocket-workspace/src/repository.rs`
   - Add to `WorkspaceRepository`:
     ```rust
     fn ensure_workspace_dirs(&self, path: &Path) -> DomainResult<()>;
     ```
   - This lets `WorkspaceService::create` and `WorkspaceService::open_workspace`
     delegate directory creation without raw I/O

2. `crates/rocket-workspace/src/config_repository.rs`
   - Add to `WorkspaceConfigRepository`:
     ```rust
     fn read_collection_name(&self, collection_dir: &Path) -> DomainResult<Option<String>>;
     ```
   - `FsWorkspaceConfigRepo` implements it by reading
     `collection_dir/opencollection.yml` (the file currently read on line 266)

3. `crates/rocket-infra/src/fs_workspace_repo.rs`
   - Implement `ensure_workspace_dirs(&self, path: &Path)` using
     `std::fs::create_dir_all`

4. `crates/rocket-infra/src/fs_workspace_config_repo.rs`
   - Implement `read_collection_name(&self, collection_dir: &Path)` by reading
     and parsing `opencollection.yml`

5. `crates/rocket-app/src/workspace_service.rs`
   - Replace the three `fs::create_dir_all` calls with `self.repo.ensure_workspace_dirs(&path)?`
   - Replace the `fs::read_to_string` call with `self.config_repo.read_collection_name(&dir)?`
   - Remove `use std::fs` import if it becomes unused

**Tests:**
- `cargo check -p rocket-app` and `cargo check -p rocket-workspace` pass
- `cargo check -p rocket-infra` passes
- Existing workspace service tests still pass

---

## Task 3 — I3: Remove raw I/O and fix `.expect()` in command handlers

**Problem:**
Two command modules do raw filesystem work that should be delegated to services:

- `commands/environments.rs` lines 28, 100: `std::fs::create_dir_all(&env_dir)`
- `commands/collections.rs` lines 164, 263: `fs::read_dir(dir)`
- `commands/environments.rs` lines 86, 94: `.expect("workspace service lock poisoned")`

**Approach:**

For `create_dir_all` in `environments.rs`: the environment service already
writes to the correct directory via `FsEnvironmentRepo`. The `create_dir_all`
calls are defensive no-ops that pre-create what the service will create anyway.
Remove them; `FsEnvironmentRepo` uses `atomic_write` which calls
`create_dir_all` on first write.

For `read_dir` in `collections.rs`: identify what is being read and confirm
whether the collection service already provides an equivalent operation. If yes,
call the service; if not, add a service method (e.g. `scan_dir`) and implement
it in `FsCollectionRepo`.

For `.expect()` on mutex lock: replace with
`.map_err(|_| "workspace service lock poisoned".to_string())?`

**Files to change:**

1. `src-tauri/src/commands/environments.rs`
   - Lines 28, 100: remove `std::fs::create_dir_all` calls
   - Lines 86, 94: replace `.expect("workspace service lock poisoned")` with
     `.map_err(|_| "workspace service lock poisoned".to_string())?`

2. `src-tauri/src/commands/collections.rs`
   - Lines 164, 263: check what `fs::read_dir` is doing; either remove it if a
     service call covers it, or move the logic to `CollectionService` + repo

**Tests:**
- `cargo check` (full workspace) passes
- Manually verify that listing environments and collections still works end-to-end
  (or run any existing Vitest/Rust tests that cover these paths)

---

## Task 4 — I4: Replace `.unwrap()` on workspace mutex in command handlers

**Problem:**
All 15 commands in `src-tauri/src/commands/workspaces.rs` call
`svc.lock().unwrap()`. A single poisoned lock crashes every subsequent workspace
command for the rest of the process lifetime.

**Approach:**
Replace every `.unwrap()` with `.map_err(|_| "workspace service lock poisoned".to_string())?`

This is a pure mechanical substitution — no logic changes.

**Files to change:**

1. `src-tauri/src/commands/workspaces.rs`
   - All 15 occurrences of `.lock().unwrap()` → `.lock().map_err(|_| "workspace service lock poisoned".to_string())?`

**Tests:**
- `cargo check` passes
- No functional change; existing tests pass

---

## Task 5 — I5: Add `DomainError::Conflict` variant for merge conflicts

**Problem:**
`crates/rocket-git/src/git2_service.rs` lines 676–678 return
`DomainError::Internal(format!("merge conflict: ..."))` for user-resolvable
merge conflicts. The `Internal` variant implies a bug; conflicts are an expected
domain outcome and should be surfaced as a distinct, handleable variant.

**Approach:**
Add `Conflict(String)` to `DomainError`. Update the single call-site in
`git2_service.rs`. Update `From<DomainError>` impls if any map exhaustively.

**Files to change:**

1. `crates/rocket-shared/src/error.rs`
   - Add variant: `Conflict(String)` with `#[error("conflict: {0}")]`

2. `crates/rocket-git/src/git2_service.rs` lines 676–678
   - Change `DomainError::Internal(format!("merge conflict: ..."))` →
     `DomainError::Conflict(format!("merge conflict: ..."))`

3. `src-tauri/src/commands/*.rs` — grep for exhaustive `DomainError` matches
   - Add `Conflict(msg)` arm if any match is exhaustive; map to a user-readable
     error string

**Tests:**
- `cargo check` passes
- `cargo test -p rocket-git` passes

---

## Task 6 — I6: Make `FsAuditLogRepo::latest()` O(1)

**Problem:**
`crates/rocket-infra/src/fs_audit_log_repo.rs` line 78:
```rust
Ok(self.read_lines()?.into_iter().last())
```
This reads the entire JSONL file to get only the last event. As the audit log
grows this becomes increasingly expensive.

**Approach:**
Read the file from the end using a reverse byte scan to find the last `\n`
terminated line, without loading the full file. Use `std::io::Seek` +
`std::io::Read` on the underlying file to scan backwards for the final newline.

Concrete steps:
1. Open the file (return `Ok(None)` if it doesn't exist)
2. Seek to the end; get file length
3. If length == 0, return `Ok(None)`
4. Scan backwards in small chunks (e.g. 512 bytes) to find the last `\n`
5. Seek to `last_newline_pos + 1`, read to EOF, parse as JSON

**Files to change:**

1. `crates/rocket-infra/src/fs_audit_log_repo.rs`
   - Replace the `latest()` method body with the O(1) reverse-scan implementation

**Tests:**
- Existing unit tests for `FsAuditLogRepo` must pass unchanged
- Add test: `latest_returns_last_line_without_reading_all()` — write 1000 events,
  verify `latest()` returns the 1000th without regressing

---

## Task 7 — I7: Bound history search to avoid full-table scan

**Problem:**
`crates/rocket-infra/src/fs_history_repo.rs` line 79:
```rust
let all = self.list(None)?
```
`search()` loads all history records before filtering. On a large history this
reads every file. There is no upper bound on `search` results.

**Approach:**
Pass a limit into the internal `list` call, or short-circuit loading once enough
matching entries have been accumulated. A pragmatic cap of 200 results is
sufficient for the UI's search use case.

Concrete steps:
1. Define a `SEARCH_LIMIT: usize = 200` constant
2. In `search()`, accumulate matches until `results.len() == SEARCH_LIMIT`, then
   stop loading more files (use `take_while`/early-return pattern)
3. History files should already be sorted by modification time (newest-first);
   confirm and document the sort order

**Files to change:**

1. `crates/rocket-infra/src/fs_history_repo.rs`
   - Add `const SEARCH_LIMIT: usize = 200;`
   - Rewrite the `search()` accumulation loop to stop after `SEARCH_LIMIT` matches

**Tests:**
- `cargo check -p rocket-infra` passes
- Add test: `search_respects_limit()` — populate 300 entries all matching the
  query, verify `search()` returns exactly 200

---

## Execution Order

Tasks 3, 4, 5, 6, 7 are independent and could run in parallel.
Task 1 must precede any changes to `import.rs` in `src-tauri`.
Task 2 must precede any changes that assume `workspace_service` is free of I/O.

Recommended sequential order (minimises cross-task conflicts):

1. Task 5 (shared error type — no dependencies, smallest diff)
2. Task 6 (audit repo — isolated)
3. Task 7 (history repo — isolated)
4. Task 4 (workspace commands — mechanical)
5. Task 3 (env/collection commands — slightly larger)
6. Task 2 (workspace_service I/O — trait changes)
7. Task 1 (import DI — largest refactor, depends on no other task)
