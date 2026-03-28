# SP4-P03: GitService Trait + Git2 Status/Diff Implementation

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define the `GitService` trait (full interface — ~25 methods) and implement `Git2Service` for `is_repo`, `init`, `status`, `diff_file`, and `diff_staged` using the `git2` crate.

**Architecture:** `GitService` trait in `service.rs`. `Git2Service` struct in `git2_service.rs` implements it. Remaining methods start as `todo!()` — filled in by P04 and P05.

**Tech Stack:** Rust, git2, rocket-shared

**Prerequisite:** SP4-P02 complete.

---

## Task 1: Define GitService trait

**Files:**
- Create: `crates/rocket-git/src/service.rs`
- Modify: `crates/rocket-git/src/lib.rs`

- [ ] **Step 1: Implement the full trait**

`crates/rocket-git/src/service.rs`:
```rust
use rocket_shared::error::DomainResult;
use crate::{
    status::RepoStatus, diff::FileDiff, branch::BranchList,
    commit::CommitInfo, stash::StashEntry,
    conflict::{ConflictFile, ConflictResolution},
    credentials::GitCredentials,
};

pub trait GitService: Send + Sync {
    // Repository
    fn is_repo(&self, path: &str) -> bool;
    fn init(&self, path: &str) -> DomainResult<()>;
    fn clone_repo(&self, url: &str, dest_path: &str, creds: &GitCredentials) -> DomainResult<()>;

    // Status + diff
    fn status(&self, path: &str) -> DomainResult<RepoStatus>;
    fn diff_file(&self, path: &str, file: &str) -> DomainResult<FileDiff>;
    fn diff_staged(&self, path: &str, file: &str) -> DomainResult<FileDiff>;

    // Staging
    fn stage(&self, path: &str, files: &[&str]) -> DomainResult<()>;
    fn unstage(&self, path: &str, files: &[&str]) -> DomainResult<()>;
    fn discard(&self, path: &str, files: &[&str]) -> DomainResult<()>;

    // Commit
    fn commit(&self, path: &str, message: &str) -> DomainResult<CommitInfo>;
    fn log(&self, path: &str, limit: usize) -> DomainResult<Vec<CommitInfo>>;

    // Remote
    fn push(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()>;
    fn pull(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()>;
    fn fetch(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()>;

    // Branches
    fn branches(&self, path: &str) -> DomainResult<BranchList>;
    fn switch_branch(&self, path: &str, name: &str) -> DomainResult<()>;
    fn create_branch(&self, path: &str, name: &str) -> DomainResult<()>;
    fn delete_branch(&self, path: &str, name: &str) -> DomainResult<()>;
    fn merge_branch(&self, path: &str, name: &str) -> DomainResult<()>;

    // Stash
    fn stash_list(&self, path: &str) -> DomainResult<Vec<StashEntry>>;
    fn stash_save(&self, path: &str, message: &str) -> DomainResult<()>;
    fn stash_pop(&self, path: &str, index: usize) -> DomainResult<()>;
    fn stash_apply(&self, path: &str, index: usize) -> DomainResult<()>;
    fn stash_drop(&self, path: &str, index: usize) -> DomainResult<()>;

    // Conflicts
    fn conflicts(&self, path: &str) -> DomainResult<Vec<ConflictFile>>;
    fn resolve_conflict(&self, path: &str, file: &str, resolution: &ConflictResolution) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trait_is_object_safe() { fn _assert(_: Box<dyn GitService>) {} }
}
```

- [ ] **Step 2: Add to lib.rs**

```rust
pub mod service;
pub use service::GitService;
```

- [ ] **Step 3: Run test, commit**

```bash
cargo test -p rocket-git -- service::tests
git add crates/rocket-git/src/service.rs crates/rocket-git/src/lib.rs
git commit -m "feat(git): GitService trait — 27 methods, object safe"
```

---

## Task 2: Git2Service — is_repo, init, status, diff_file, diff_staged

**Files:**
- Create: `crates/rocket-git/src/git2_service.rs`
- Modify: `crates/rocket-git/src/lib.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::GitService;
    use crate::status::GitStatus;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn setup_repo() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let repo = git2::Repository::init(&path).unwrap();
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();
        fs::write(dir.path().join("test.bru"), "meta { name: Test }").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.bru")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        (dir, path)
    }

    #[test]
    fn is_repo_true() {
        let (_dir, path) = setup_repo();
        assert!(Git2Service::new().is_repo(&path));
    }

    #[test]
    fn is_repo_false() {
        let dir = TempDir::new().unwrap();
        assert!(!Git2Service::new().is_repo(&dir.path().to_string_lossy()));
    }

    #[test]
    fn status_modified_file() {
        let (dir, path) = setup_repo();
        fs::write(dir.path().join("test.bru"), "meta { name: Changed }").unwrap();
        let status = Git2Service::new().status(&path).unwrap();
        assert_eq!(status.branch, "main");
        assert!(status.files.iter().any(|f| f.path == "test.bru" && f.status == GitStatus::Modified));
    }

    #[test]
    fn status_untracked_file() {
        let (dir, path) = setup_repo();
        fs::write(dir.path().join("new.bru"), "new").unwrap();
        let status = Git2Service::new().status(&path).unwrap();
        assert!(status.files.iter().any(|f| f.path == "new.bru" && f.status == GitStatus::Untracked));
    }

    #[test]
    fn diff_file_shows_changes() {
        let (dir, path) = setup_repo();
        fs::write(dir.path().join("test.bru"), "meta { name: Changed }").unwrap();
        let diff = Git2Service::new().diff_file(&path, "test.bru").unwrap();
        assert_eq!(diff.path, "test.bru");
        assert!(diff.old_content.is_some());
        assert!(diff.new_content.is_some());
        assert_ne!(diff.old_content, diff.new_content);
    }
}
```

- [ ] **Step 2: Implement Git2Service (status + diff only, rest as todo!())**

`crates/rocket-git/src/git2_service.rs` — implement `is_repo`, `init`, `status`, `diff_file`, `diff_staged`. All other trait methods: `todo!()`.

Include helper functions: `open_repo`, `map_git2_status`, `get_ahead_behind`, `get_head_content`, `build_simple_diff`.

Key detail for `diff_staged`: diff the index against HEAD (not working tree against HEAD):
```rust
fn diff_staged(&self, path: &str, file: &str) -> DomainResult<FileDiff> {
    let repo = Self::open_repo(path)?;
    let head_content = get_head_content(&repo, file);
    let index_content = get_index_content(&repo, file);
    // Build diff between head and index
    Ok(FileDiff {
        path: file.to_string(),
        old_content: head_content,
        new_content: index_content,
        hunks: build_simple_diff(&head_content, &index_content),
    })
}
```

- [ ] **Step 3: Add to lib.rs**

```rust
pub mod git2_service;
pub use git2_service::Git2Service;
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p rocket-git -- git2_service::tests
```
Expected: PASS — 5 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs crates/rocket-git/src/lib.rs
git commit -m "feat(git): Git2Service — is_repo, init, status, diff_file, diff_staged"
```

---

## Milestone Checklist — P03

- [ ] `GitService` trait with 27 methods — object safe
- [ ] `Git2Service` implements: `is_repo`, `init`, `status`, `diff_file`, `diff_staged`
- [ ] Remaining methods are `todo!()` (filled in P04, P05)
- [ ] 6 tests pass (1 trait + 5 integration)
