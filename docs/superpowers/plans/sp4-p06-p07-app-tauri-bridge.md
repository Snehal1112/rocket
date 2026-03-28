# SP4-P06: Domain Events + GitAppService

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Git domain events to `rocket-shared` and create `GitAppService` in `rocket-app` that delegates to `GitService` + publishes events.

**Tech Stack:** Rust, rocket-shared, rocket-app, rocket-git

**Prerequisite:** SP4-P05 complete.

---

## Task 1: Add Git domain events

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`

- [ ] **Step 1: Add Git event variants to DomainEvent**

```rust
GitStatusChanged { collection: String },
GitCommit { collection: String, message: String, sha: String },
GitPush { collection: String, remote: String },
GitPull { collection: String, remote: String },
BranchSwitched { collection: String, branch: String },
BranchMerged { collection: String, branch: String },
GitStashChanged { collection: String },
GitConflictDetected { collection: String, files: Vec<String> },
GitCloned { url: String, dest: String },
```

- [ ] **Step 2: Test + commit**

```bash
cargo test -p rocket-shared
git commit -am "feat(shared): Git domain events"
```

---

## Task 2: Create GitAppService

**Files:**
- Create: `crates/rocket-app/src/git_service.rs`
- Modify: `crates/rocket-app/Cargo.toml` (add rocket-git dep)
- Modify: `crates/rocket-app/src/lib.rs`

- [ ] **Step 1: Add dependency**

```toml
rocket-git.workspace = true
```

- [ ] **Step 2: Implement GitAppService — thin delegate + event publishing**

Every method: call `self.git.method()`, then publish the relevant event. ~27 methods, each 3-5 lines. (See the full implementation in the previous SP4 Plan 2 — copy that pattern exactly.)

- [ ] **Step 3: Export from lib.rs**

```rust
pub mod git_service;
pub use git_service::GitAppService;
```

- [ ] **Step 4: Test + commit**

```bash
cargo check -p rocket-app
git commit -am "feat(app): GitAppService — delegates + domain events"
```

---

## Milestone Checklist — P06

- [ ] 9 Git domain events added
- [ ] `GitAppService` wraps all 27 `GitService` methods
- [ ] Events published for: commit, push, pull, branch switch, merge, stash, clone
- [ ] `cargo check --workspace` passes

---

---

# SP4-P07: Tauri Commands + TypeScript Bridge

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Register ~24 Tauri commands for all git operations, wire `GitAppService` into Tauri managed state, create TypeScript types and invoke wrappers.

**Tech Stack:** Rust (Tauri), TypeScript

**Prerequisite:** SP4-P06 complete.

---

## Task 1: Tauri git commands

**Files:**
- Create: `src-tauri/src/commands/git.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml` (add rocket-git dep if needed)

- [ ] **Step 1: Implement all git commands (each 1-3 lines)**

24 commands: `git_is_repo`, `git_init`, `git_clone`, `git_status`, `git_diff`, `git_diff_staged`, `git_stage`, `git_unstage`, `git_discard`, `git_commit`, `git_log`, `git_push`, `git_pull`, `git_fetch`, `git_branches`, `git_switch_branch`, `git_create_branch`, `git_delete_branch`, `git_merge_branch`, `git_stash_list`, `git_stash_save`, `git_stash_pop`, `git_stash_apply`, `git_stash_drop`, `git_conflicts`, `git_resolve_conflict`

Each follows the pattern:
```rust
#[tauri::command]
pub fn git_status(collection_path: String, svc: State<'_, GitAppService>) -> Result<RepoStatus, DomainError> {
    svc.status(&collection_path)
}
```

- [ ] **Step 2: Register in lib.rs + manage state**

Add `GitAppService` to Tauri managed state in `setup()`:
```rust
let git_svc = GitAppService::new(
    Box::new(rocket_git::Git2Service::new()),
    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
);
app.manage(git_svc);
```

Add all 24 commands to `generate_handler![]`.

- [ ] **Step 3: Verify compilation**

```bash
cargo check --workspace
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(tauri): 24 git commands registered"
```

---

## Task 2: TypeScript types + invoke wrappers

**Files:**
- Modify: `frontend/src/lib/tauri-api.ts`
- Modify: `frontend/src/lib/api.ts`

- [ ] **Step 1: Add all git TypeScript types**

`RepoStatus`, `FileStatus`, `FileDiff`, `DiffHunk`, `DiffLine`, `CommitInfo`, `BranchList`, `Branch`, `StashEntry`, `ConflictFile`, `ConflictResolution`, `GitCredentials`

- [ ] **Step 2: Add 24 invoke wrapper functions**

`gitIsRepo`, `gitInit`, `gitClone`, `gitStatus`, `gitDiff`, `gitDiffStaged`, `gitStage`, `gitUnstage`, `gitDiscard`, `gitCommit`, `gitLog`, `gitPush`, `gitPull`, `gitFetch`, `gitBranches`, `gitSwitchBranch`, `gitCreateBranch`, `gitDeleteBranch`, `gitMergeBranch`, `gitStashList`, `gitStashSave`, `gitStashPop`, `gitStashApply`, `gitStashDrop`, `gitConflicts`, `gitResolveConflict`

- [ ] **Step 3: Re-export from api.ts**

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(frontend): Git TS types + 24 invoke wrappers"
```

---

## Milestone Checklist — P07

- [ ] 24 Tauri commands implemented and registered
- [ ] `GitAppService` managed in Tauri state
- [ ] 12 TypeScript types defined
- [ ] 24 invoke wrapper functions
- [ ] Re-exported from `api.ts`
- [ ] `cargo check --workspace` passes
