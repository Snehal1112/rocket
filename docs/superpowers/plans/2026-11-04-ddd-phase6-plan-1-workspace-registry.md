# DDD Phase 6, Plan 1: WorkspaceRegistry owns add and rename (D-60)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `WorkspaceRegistry` guards its own rules (unique name, unique path, valid name), so `WorkspaceService` cannot skip them with a direct `workspaces.push` or `workspace.name = ..`.

**Architecture:** Add `ensure_can_add`, `add` and `rename` to `WorkspaceRegistry` in `rocket-workspace`, with tests first. `WorkspaceService::create`, `open_workspace` and `rename` call them. Error kinds and texts stay the same. `close`, `delete` and `switch` are out of scope.

**Tech Stack:** Rust, `DomainError`/`DomainResult`.

**Spec:** `docs/roadmaps/ddd-roadmap.md` item D-60, `docs/engineering/ddd-decisions.md` D-04 (narrow model, `WorkspaceRegistry` first).

## Global Constraints

- Behavior is unchanged except one ruled change: `create` now also refuses a path that is already in the registry (before, two entries could share one folder).
- Error texts stay: `AlreadyExists(<name>)`, `AlreadyExists("This workspace is already open")`, `NotFound(<id>)`, and the `validate_name` text.
- The name check still runs before any file is written (a duplicate leaves no files on disk).
- No `unwrap` in production code. `cargo` commands take `-j4`.

## Review Focus

- A name that differs only in case is a duplicate (`name_exists` is case-insensitive).
- Renaming a workspace to its own name in another case is allowed (the id is excluded).
- Renaming to a duplicate name leaves the old name in place.
- A missing id on rename is `NotFound`, and the duplicate-name check still comes first.
- `open_workspace` of an already-open path reports the path error, not the name error.

---

### Task 1: Registry methods (domain, tests first)

**Files:**
- Modify: `crates/rocket-workspace/src/workspace.rs`

**Interfaces:**
- Produces:
  ```rust
  impl WorkspaceRegistry {
      pub fn ensure_can_add(&self, name: &str, path: &Path) -> DomainResult<()>; // path first, then name
      pub fn add(&mut self, workspace: Workspace) -> DomainResult<()>;           // ensure_can_add, then push
      pub fn rename(&mut self, id: &str, new_name: &str) -> DomainResult<String>; // returns the old name
  }
  ```

- [ ] **Step 1: Write the failing tests**: `ensure_can_add_refuses_a_path_already_open`, `ensure_can_add_refuses_a_name_in_another_case`, `ensure_can_add_checks_the_path_before_the_name`, `add_pushes_after_the_checks`, `add_of_a_duplicate_leaves_the_registry_unchanged`, `rename_changes_the_name_and_returns_the_old_one`, `rename_to_a_duplicate_keeps_the_old_name`, `rename_to_the_same_name_in_another_case_is_allowed`, `rename_of_a_missing_id_is_not_found`, `rename_rejects_an_empty_name`, `rename_checks_the_duplicate_before_the_id`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-workspace` — Expected: compile error (red).
- [ ] **Step 3: Implement** the three methods.
- [ ] **Step 4: Run** the same command — Expected: all pass.

### Task 2: The service uses them

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

**Interfaces:**
- Consumes: Task 1.

- [ ] **Step 1: Write the failing tests** in the service tests: `create_refuses_a_path_that_is_already_registered` (nothing written to disk), `open_workspace_reports_the_path_before_the_name`, `rename_of_a_missing_workspace_is_not_found`.
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-app workspace_service` — Expected: the new `create` test fails.
- [ ] **Step 3: Replace** the checks and the `push` in `create` and `open_workspace`, and the name change in `rename`, with the registry methods. `create` calls `ensure_can_add` before `ensure_workspace_dirs`.
- [ ] **Step 4: Run** `cargo test -j4 -p rocket-app workspace_service` and `cargo check -j4 --all-targets` — Expected: pass.

### Task 3: Docs

**Files:**
- Modify: `crates/rocket-workspace/CLAUDE.md` (if present), `docs/roadmaps/ddd-roadmap.md` (D-60 done).

- [ ] **Step 1: Edit the docs and run** `cargo clippy -j4 -p rocket-workspace -p rocket-app --all-targets` — Expected: no new warnings.
