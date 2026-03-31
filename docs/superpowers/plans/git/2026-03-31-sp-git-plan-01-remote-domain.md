# Plan 1: Remote CRUD — Domain Type, Trait & Implementation

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `RemoteInfo` type, 4 remote CRUD methods to `GitService` trait, implement them in `Git2Service`, and test.

**Architecture:** New `remote.rs` module in `rocket-git` defines the `RemoteInfo` type. The `GitService` trait gets 4 new methods. `Git2Service` implements them using `git2` crate APIs. All tests use `tempfile::TempDir` like existing tests.

**Tech Stack:** Rust, git2 crate, serde, tempfile (tests)

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 1

**Constraints:** No `.json` files. All serde annotations are for Tauri IPC (in-memory), not file storage.

---

## Chunk 1: Remote CRUD Domain & Implementation

### Task 1: Create `RemoteInfo` type and `remote.rs` module

**Files:**
- Create: `crates/rocket-git/src/remote.rs`
- Modify: `crates/rocket-git/src/lib.rs`

- [ ] **Step 1: Create `crates/rocket-git/src/remote.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}
```

- [ ] **Step 2: Add module and re-export in `crates/rocket-git/src/lib.rs`**

Add after the existing `pub mod credentials;` line:

```rust
pub mod remote;
```

Add after the existing `pub use credentials::*;` line:

```rust
pub use remote::*;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p rocket-git`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-git/src/remote.rs crates/rocket-git/src/lib.rs
git commit -m "feat(rocket-git): add RemoteInfo domain type"
```

### Task 2: Add remote CRUD methods to `GitService` trait

**Files:**
- Modify: `crates/rocket-git/src/service.rs`

- [ ] **Step 1: Add import for `RemoteInfo` at top of `service.rs`**

Add `RemoteInfo` to the existing imports from `crate`:

```rust
use crate::{
    status::RepoStatus, diff::FileDiff, branch::BranchList,
    commit::CommitInfo, stash::StashEntry,
    conflict::{ConflictFile, ConflictResolution},
    credentials::GitCredentials,
    remote::RemoteInfo,
};
```

- [ ] **Step 2: Add 4 new methods to the `GitService` trait**

Add after the existing `fn clone_repo(...)` method, before the `// Status + diff` comment:

```rust
    // Remotes
    fn list_remotes(&self, path: &str) -> DomainResult<Vec<RemoteInfo>>;
    fn add_remote(&self, path: &str, name: &str, url: &str) -> DomainResult<()>;
    fn remove_remote(&self, path: &str, name: &str) -> DomainResult<()>;
    fn set_remote_url(&self, path: &str, name: &str, url: &str) -> DomainResult<()>;
```

- [ ] **Step 3: Verify compilation fails (Git2Service doesn't implement new methods yet)**

Run: `cargo check -p rocket-git`
Expected: compilation error — `Git2Service` doesn't implement `list_remotes`, `add_remote`, `remove_remote`, `set_remote_url`

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-git/src/service.rs
git commit -m "feat(rocket-git): add remote CRUD methods to GitService trait"
```

### Task 3: Implement remote CRUD in `Git2Service` and add tests

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Add `RemoteInfo` import at top of `git2_service.rs`**

Add to the existing `use crate::` block:

```rust
use crate::remote::RemoteInfo;
```

- [ ] **Step 2: Implement `list_remotes` in the `impl GitService for Git2Service` block**

Add after the `clone_repo` method implementation:

```rust
    fn list_remotes(&self, path: &str) -> DomainResult<Vec<RemoteInfo>> {
        let repo = open_repo(path)?;
        let remote_names = repo
            .remotes()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let mut remotes = Vec::new();
        for name in remote_names.iter().flatten() {
            let remote = repo
                .find_remote(name)
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            let url = remote.url().unwrap_or("").to_string();
            remotes.push(RemoteInfo {
                name: name.to_string(),
                url,
            });
        }
        Ok(remotes)
    }
```

- [ ] **Step 3: Implement `add_remote`**

Add after `list_remotes`:

```rust
    fn add_remote(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        repo.remote(name, url)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
```

- [ ] **Step 4: Implement `remove_remote`**

Add after `add_remote`:

```rust
    fn remove_remote(&self, path: &str, name: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        repo.remote_delete(name)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
```

- [ ] **Step 5: Implement `set_remote_url`**

Add after `remove_remote`:

```rust
    fn set_remote_url(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        repo.remote_set_url(name, url)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
```

- [ ] **Step 6: Verify compilation passes**

Run: `cargo check -p rocket-git`
Expected: compiles with no errors

- [ ] **Step 7: Add tests in the `#[cfg(test)] mod tests` block of `git2_service.rs`**

Add these tests at the end of the existing tests block:

```rust
    #[test]
    fn list_remotes_empty_for_fresh_repo() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        let remotes = svc.list_remotes(&path).unwrap();
        assert!(remotes.is_empty());
    }

    #[test]
    fn add_and_list_remote() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/repo.git").unwrap();
        let remotes = svc.list_remotes(&path).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[0].url, "https://github.com/user/repo.git");
    }

    #[test]
    fn add_multiple_remotes() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/repo.git").unwrap();
        svc.add_remote(&path, "upstream", "https://github.com/upstream/repo.git").unwrap();
        let remotes = svc.list_remotes(&path).unwrap();
        assert_eq!(remotes.len(), 2);
        let names: Vec<&str> = remotes.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"origin"));
        assert!(names.contains(&"upstream"));
    }

    #[test]
    fn remove_remote() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/repo.git").unwrap();
        svc.remove_remote(&path, "origin").unwrap();
        let remotes = svc.list_remotes(&path).unwrap();
        assert!(remotes.is_empty());
    }

    #[test]
    fn set_remote_url() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/old.git").unwrap();
        svc.set_remote_url(&path, "origin", "https://github.com/user/new.git").unwrap();
        let remotes = svc.list_remotes(&path).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].url, "https://github.com/user/new.git");
    }

    #[test]
    fn add_duplicate_remote_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/repo.git").unwrap();
        let result = svc.add_remote(&path, "origin", "https://github.com/user/other.git");
        assert!(result.is_err());
    }

    #[test]
    fn remove_nonexistent_remote_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        let result = svc.remove_remote(&path, "nonexistent");
        assert!(result.is_err());
    }
```

- [ ] **Step 8: Run all tests**

Run: `cargo test -p rocket-git`
Expected: all tests pass, including the 7 new remote tests

- [ ] **Step 9: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "feat(rocket-git): implement remote CRUD in Git2Service with tests"
```
