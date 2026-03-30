# Rename Default Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change the default workspace name from "Default Workspace" to "My Workspace" for new installations.

**Architecture:** One string literal in `rocket-workspace` controls the display name; one string in `rocket-infra` controls the filesystem directory created on first run. Both must change together. Existing users are unaffected — their workspace name and path are stored in `workspaces.yml` and never re-derived from these constants.

**Tech Stack:** Rust, serde/serde_yaml, tempfile (tests)

---

## File Structure

| Action | File | Change |
|--------|------|--------|
| Modify | `crates/rocket-workspace/src/workspace.rs:51` | Display name string literal |
| Modify | `crates/rocket-workspace/src/workspace.rs:110–111` | Update `name_exists` test that uses the old name |
| Modify | `crates/rocket-infra/src/fs_workspace_repo.rs:16` | Filesystem directory name on first install |
| Modify | `crates/rocket-infra/src/fs_workspace_repo.rs:75,77` | Two test assertions that check the old name/dir |
| Modify | `crates/rocket-app/src/workspace_service.rs:407` | Test that checks duplicate-name rejection uses old name |

---

### Task 1: Update the domain model and its tests

**Files:**
- Modify: `crates/rocket-workspace/src/workspace.rs`

- [ ] **Step 1: Update the display name literal in `new_with_default`**

In `crates/rocket-workspace/src/workspace.rs`, line 51, change:
```rust
            name: "Default Workspace".to_string(),
```
to:
```rust
            name: "My Workspace".to_string(),
```

- [ ] **Step 2: Update the `name_exists_case_insensitive` test**

In the same file, around line 110–111, the test checks the old name. Change:
```rust
        assert!(reg.name_exists("default workspace", None));
        assert!(!reg.name_exists("default workspace", Some("default")));
```
to:
```rust
        assert!(reg.name_exists("my workspace", None));
        assert!(!reg.name_exists("my workspace", Some("default")));
```

- [ ] **Step 3: Run the workspace crate tests to verify they pass**

```bash
cargo test -p rocket-workspace
```
Expected: all tests pass. If `name_exists_case_insensitive` still fails, double-check step 2.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-workspace/src/workspace.rs
git commit -m "feat(workspace): rename default workspace to My Workspace"
```

---

### Task 2: Update the infra layer and its tests

**Files:**
- Modify: `crates/rocket-infra/src/fs_workspace_repo.rs`

- [ ] **Step 1: Update the filesystem directory name in `FsWorkspaceRepo::new`**

In `crates/rocket-infra/src/fs_workspace_repo.rs`, line 16, change:
```rust
            default_workspace_path: app_data_dir.join("Default Workspace"),
```
to:
```rust
            default_workspace_path: app_data_dir.join("My Workspace"),
```

- [ ] **Step 2: Update the `first_load_creates_default_workspace` test assertions**

In the same file, around lines 75 and 77, change:
```rust
        assert_eq!(registry.workspaces[0].name, "Default Workspace");
        assert!(tmp.path().join("workspaces.yml").exists());
        assert!(tmp.path().join("Default Workspace").exists());
```
to:
```rust
        assert_eq!(registry.workspaces[0].name, "My Workspace");
        assert!(tmp.path().join("workspaces.yml").exists());
        assert!(tmp.path().join("My Workspace").exists());
```

- [ ] **Step 3: Run the infra crate tests to verify they pass**

```bash
cargo test -p rocket-infra
```
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra/src/fs_workspace_repo.rs
git commit -m "feat(infra): update default workspace directory to My Workspace"
```

---

### Task 3: Update the app-layer test

**Files:**
- Modify: `crates/rocket-app/src/workspace_service.rs`

**Context:** There is a test `rename_rejects_duplicate` that creates a workspace named "Alpha" then asserts that renaming it to "Default Workspace" is rejected (because "Default Workspace" already exists as the seeded workspace). The test still works with the new name — it just needs the string updated to "My Workspace" so it tests the right thing.

- [ ] **Step 1: Update the duplicate-name rejection test**

In `crates/rocket-app/src/workspace_service.rs`, around line 407, change:
```rust
        assert!(svc.rename(&ws.id, "Default Workspace").is_err());
```
to:
```rust
        assert!(svc.rename(&ws.id, "My Workspace").is_err());
```

- [ ] **Step 2: Run the app crate tests to verify they pass**

```bash
cargo test -p rocket-app
```
Expected: all tests pass.

- [ ] **Step 3: Run full cargo check to confirm no compilation errors**

```bash
cargo check
```
Expected: no errors or warnings related to this change.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/workspace_service.rs
git commit -m "test(workspace): update rename duplicate test to use My Workspace"
```
