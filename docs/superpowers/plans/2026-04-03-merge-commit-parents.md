# Merge Commit Parent Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `commit()` in `Git2Service` to include `MERGE_HEAD` as a second parent when in merge-in-progress state, so a push after conflict resolution succeeds.

**Architecture:** Single method change in `git2_service.rs`. After building the parent list, check for `MERGE_HEAD` via `repo.find_reference("MERGE_HEAD")` and append it if found. After a successful merge commit, call `repo.cleanup_state()` to remove `.git/MERGE_HEAD` and related files.

**Tech Stack:** Rust, git2 (libgit2 bindings)

---

## File Map

| File | Change |
|------|--------|
| `crates/rocket-git/src/git2_service.rs` | Add test `commit_creates_merge_commit_when_merge_in_progress`; fix `commit()` parent logic and add `cleanup_state()` call |

---

## Task 1: Fix `commit()` to Handle Merge-in-Progress State

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs:430-455` (implementation)
- Modify: `crates/rocket-git/src/git2_service.rs:1441` (add test before closing `}`)

- [ ] **Step 1: Write the failing test**

Add the following test inside the `#[cfg(test)] mod tests { ... }` block at the end of `crates/rocket-git/src/git2_service.rs`, just before the closing `}` of the `tests` module (after the `abort_merge_resets_to_head` test, line 1441):

```rust
    #[test]
    fn commit_creates_merge_commit_when_merge_in_progress() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Create a branch with a change to the same file (will conflict with main).
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "branch content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch commit").unwrap();

        // Switch back to main and make a conflicting change.
        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "main content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        let main_tip = svc.commit(&path, "main commit").unwrap();

        // Start the merge — this leaves the repo in conflict state (MERGE_HEAD set).
        let _ = svc.merge_branch(&path, "conflict-branch");

        // Verify we are actually in a merge-in-progress state before proceeding.
        assert!(
            dir.path().join(".git/MERGE_HEAD").exists(),
            "MERGE_HEAD must exist to simulate merge-in-progress state"
        );

        // Resolve the conflict by staging a resolved version of the file.
        fs::write(dir.path().join("test.bru"), "resolved content").unwrap();
        let repo = Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.bru")).unwrap();
        index.write().unwrap();

        // Commit via the service — must produce a two-parent merge commit.
        let info = svc.commit(&path, "merge: resolve conflicts").unwrap();

        // The new commit must have exactly 2 parents.
        let oid = git2::Oid::from_str(&info.full_id).unwrap();
        let commit = Repository::open(&path).unwrap().find_commit(oid).unwrap();
        assert_eq!(commit.parent_count(), 2, "merge commit must have 2 parents");

        // First parent must be the main tip before the merge.
        assert_eq!(
            commit.parent(0).unwrap().id().to_string()[..7].to_string(),
            main_tip.id,
            "first parent must be the main branch tip"
        );

        // MERGE_HEAD must be cleaned up after the commit.
        assert!(
            !dir.path().join(".git/MERGE_HEAD").exists(),
            "MERGE_HEAD must be removed after a successful merge commit"
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p rocket-git commit_creates_merge_commit_when_merge_in_progress -- --nocapture
```

Expected: FAIL — the commit currently produces a single-parent commit, so `assert_eq!(commit.parent_count(), 2, ...)` fails.

- [ ] **Step 3: Implement the fix**

In `crates/rocket-git/src/git2_service.rs`, find the `commit()` method at line 430. Replace lines 440–444 (the parent resolution and `repo.commit` call) with:

```rust
        let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let merge_commit = repo
            .find_reference("MERGE_HEAD")
            .ok()
            .and_then(|r| r.peel_to_commit().ok());

        let parents: Vec<&git2::Commit> = head_commit.iter()
            .chain(merge_commit.iter())
            .collect();

        let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Remove merge state files after a successful merge commit.
        if merge_commit.is_some() {
            let _ = repo.cleanup_state();
        }
```

The full `commit()` method after the change:

```rust
    fn commit(&self, path: &str, message: &str) -> DomainResult<CommitInfo> {
        let repo = open_repo(path)?;
        let sig = repo.signature().or_else(|_|
            git2::Signature::now("RocketAPI User", "user@rocketapi.local")
        ).map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut index = repo.index().map_err(|e| DomainError::Internal(e.to_string()))?;
        let tree_id = index.write_tree().map_err(|e| DomainError::Internal(e.to_string()))?;
        let tree = repo.find_tree(tree_id).map_err(|e| DomainError::Internal(e.to_string()))?;

        let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let merge_commit = repo
            .find_reference("MERGE_HEAD")
            .ok()
            .and_then(|r| r.peel_to_commit().ok());

        let parents: Vec<&git2::Commit> = head_commit.iter()
            .chain(merge_commit.iter())
            .collect();

        let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Remove merge state files after a successful merge commit.
        if merge_commit.is_some() {
            let _ = repo.cleanup_state();
        }

        Ok(CommitInfo {
            id: oid.to_string()[..7].to_string(),
            full_id: oid.to_string(),
            message: message.to_string(),
            author: sig.name().unwrap_or("").to_string(),
            author_email: sig.email().unwrap_or("").to_string(),
            timestamp: chrono::Utc::now(),
            files_changed: 0,
        })
    }
```

- [ ] **Step 4: Run the new test to verify it passes**

```bash
cargo test -p rocket-git commit_creates_merge_commit_when_merge_in_progress -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Run the full test suite**

```bash
cargo test -p rocket-git
```

Expected: all tests pass. The existing `commit_and_log` test must still pass (normal single-parent path unchanged).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "fix: commit() includes MERGE_HEAD as second parent for merge commits"
```
