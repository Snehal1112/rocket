# Plan 2: Remote CRUD — App Service Wrappers & Domain Events

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `GitRemoteAdded` and `GitRemoteRemoved` domain events, and add 4 remote CRUD wrapper methods to `GitAppService`.

**Architecture:** Domain events are added to `DomainEvent` enum in `rocket-shared`. `GitAppService` wraps the `GitService` trait methods and publishes events on add/remove. List and set_remote_url are pass-through (no events needed).

**Tech Stack:** Rust, serde

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 1

**Depends on:** Plan 1 (RemoteInfo type, GitService trait methods, Git2Service implementation)

---

## Chunk 1: Domain Events & App Service

### Task 1: Add remote domain events to `DomainEvent`

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`

- [ ] **Step 1: Add two new variants to the `DomainEvent` enum**

Add after the existing `GitCloned { url: String, dest: String },` line in the `// Git events` section:

```rust
    GitRemoteAdded { collection: String, name: String, url: String },
    GitRemoteRemoved { collection: String, name: String },
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p rocket-shared`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-shared/src/events.rs
git commit -m "feat(rocket-shared): add GitRemoteAdded and GitRemoteRemoved domain events"
```

### Task 2: Add remote CRUD wrappers to `GitAppService`

**Files:**
- Modify: `crates/rocket-app/src/git_service.rs`

- [ ] **Step 1: Add `RemoteInfo` to the imports at top of `git_service.rs`**

Update the existing `use rocket_git::` block to include `RemoteInfo`:

```rust
use rocket_git::{
    BranchList, CommitInfo, ConflictFile, ConflictResolution,
    FileDiff, GitCredentials, RemoteInfo, RepoStatus, StashEntry,
};
```

- [ ] **Step 2: Add 4 remote methods to `GitAppService`**

Add these methods after the existing `clone_repo` method, before the `// Status + diff` comment:

```rust
    // Remotes
    pub fn list_remotes(&self, path: &str) -> DomainResult<Vec<RemoteInfo>> {
        self.git.list_remotes(path)
    }

    pub fn add_remote(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        self.git.add_remote(path, name, url)?;
        self.events.publish(DomainEvent::GitRemoteAdded {
            collection: path.to_string(),
            name: name.to_string(),
            url: url.to_string(),
        });
        Ok(())
    }

    pub fn remove_remote(&self, path: &str, name: &str) -> DomainResult<()> {
        self.git.remove_remote(path, name)?;
        self.events.publish(DomainEvent::GitRemoteRemoved {
            collection: path.to_string(),
            name: name.to_string(),
        });
        Ok(())
    }

    pub fn set_remote_url(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        self.git.set_remote_url(path, name, url)
    }
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rocket-app`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/git_service.rs
git commit -m "feat(rocket-app): add remote CRUD wrappers to GitAppService with events"
```

### Task 3: Update `GitAppService` mock in tests (if needed)

**Files:**
- Modify: `crates/rocket-app/src/git_service.rs` (test module)

- [ ] **Step 1: Check if `git_service.rs` has a `#[cfg(test)]` module with a mock `GitService`**

If there is a mock implementation of `GitService` in the test module, it needs the 4 new methods added. If there's no mock (tests use the real `Git2Service`), skip to Step 3.

- [ ] **Step 2: If a mock exists, add stub implementations for the 4 new methods**

Add to the mock `impl GitService for MockGitService` block:

```rust
    fn list_remotes(&self, _path: &str) -> DomainResult<Vec<RemoteInfo>> {
        Ok(vec![])
    }
    fn add_remote(&self, _path: &str, _name: &str, _url: &str) -> DomainResult<()> {
        Ok(())
    }
    fn remove_remote(&self, _path: &str, _name: &str) -> DomainResult<()> {
        Ok(())
    }
    fn set_remote_url(&self, _path: &str, _name: &str, _url: &str) -> DomainResult<()> {
        Ok(())
    }
```

Make sure to add `use rocket_git::RemoteInfo;` to the test module imports if not already present.

- [ ] **Step 3: Run all tests for rocket-app**

Run: `cargo test -p rocket-app`
Expected: all tests pass

- [ ] **Step 4: Commit (if changes were made)**

```bash
git add crates/rocket-app/src/git_service.rs
git commit -m "fix(rocket-app): update GitService mock with remote CRUD stubs"
```
