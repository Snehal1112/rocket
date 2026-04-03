# Git Test Suite — Rust (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ~14 new `#[test]` functions to `crates/rocket-git/src/git2_service.rs` covering the 6 method groups that currently have no tests: `init`, `clone_repo`, `diff_staged`, `push`, `stash_drop`, `conflicts`/`resolve_conflict`, and `delete_branch` (failure path).

**Architecture:** All tests append to the existing `#[cfg(test)]` module at the bottom of `git2_service.rs`. They use the existing `setup_repo()` helper and follow the same TempDir + libgit2 patterns already in the file. No new files are created.

**Tech Stack:** Rust, libgit2 via `git2` crate, `tempfile::TempDir`, `std::fs`, `rocket-git` crate types.

---

## File Map

| File | Change |
|------|--------|
| `crates/rocket-git/src/git2_service.rs` | Append tests to the existing `#[cfg(test)]` module — do NOT reorganize existing tests |

---

## Task 1: init and clone tests

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs` (append to `#[cfg(test)]` module)

- [ ] **Step 1: Write the three tests**

Append these three functions inside the existing `#[cfg(test)] mod tests { ... }` block, just before the closing `}`:

```rust
    #[test]
    fn init_creates_git_repo() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let svc = Git2Service::new();
        svc.init(&path).unwrap();
        assert!(svc.is_repo(&path));
        assert!(svc.status(&path).is_ok());
    }

    #[test]
    fn init_on_existing_repo_succeeds() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        // Calling init on an already-initialised repo must be idempotent.
        assert!(svc.init(&path).is_ok());
    }

    #[test]
    fn clone_fails_on_invalid_url() {
        let dest_dir = TempDir::new().unwrap();
        let dest_path = dest_dir.path().to_string_lossy().to_string();
        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass {
            username: String::new(),
            password: String::new(),
        };
        let result = svc.clone_repo("not-a-valid-url", &dest_path, &creds);
        assert!(result.is_err(), "clone with invalid url must fail");
    }
```

- [ ] **Step 2: Run the tests to verify they pass**

```bash
cargo test -p rocket-git init_creates_git_repo init_on_existing_repo_succeeds clone_fails_on_invalid_url -- --nocapture 2>&1 | tail -20
```

Expected: `3 passed`

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "test(rocket-git): init and clone_repo contract tests"
```

---

## Task 2: diff_staged and diff_file_clean tests

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Write the two tests**

Append inside the existing test module:

```rust
    #[test]
    fn diff_staged_shows_staged_changes() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "modified content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        let diff = svc.diff_staged(&path, "test.bru").unwrap();
        assert_eq!(diff.path, "test.bru");
        assert!(diff.old_content.is_some());
        assert!(diff.new_content.is_some());
        assert_ne!(diff.old_content, diff.new_content);
    }

    #[test]
    fn diff_file_clean_returns_empty_hunks() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        // test.bru is tracked and unmodified — diff must be empty.
        let diff = svc.diff_file(&path, "test.bru").unwrap();
        assert!(diff.hunks.is_empty(), "clean file must have no diff hunks");
    }
```

- [ ] **Step 2: Run the tests**

```bash
cargo test -p rocket-git diff_staged_shows_staged_changes diff_file_clean_returns_empty_hunks -- --nocapture 2>&1 | tail -20
```

Expected: `2 passed`

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "test(rocket-git): diff_staged and diff_file_clean contract tests"
```

---

## Task 3: push tests

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Write the two tests**

Append inside the existing test module:

```rust
    #[test]
    fn push_advances_remote_head() {
        let (dir, path) = setup_repo();

        // Set up a bare remote and push the initial commit.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();
        let repo = Repository::open(&path).unwrap();
        let mut origin = repo.remote("origin", &remote_path).unwrap();
        origin.push(&["refs/heads/main:refs/heads/main"], None).unwrap();
        drop(origin);
        drop(repo);

        // Make a new local commit via the service.
        let svc = Git2Service::new();
        fs::write(dir.path().join("new.bru"), "pushed content").unwrap();
        svc.stage(&path, &["new.bru"]).unwrap();
        let commit_info = svc.commit(&path, "new commit").unwrap();

        // Push via the service.
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };
        svc.push(&path, "origin", &creds).unwrap();

        // Verify the bare remote HEAD now matches the new local commit.
        let remote_repo = Repository::open(&remote_path).unwrap();
        let remote_head = remote_repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(
            remote_head.id().to_string(),
            commit_info.full_id,
            "remote HEAD must match the pushed commit"
        );
    }

    #[test]
    fn push_fails_with_non_fast_forward() {
        let (dir_a, path_a) = setup_repo();

        // Set up a bare remote and push the initial state from repo A.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();
        let repo_a = Repository::open(&path_a).unwrap();
        let mut origin_a = repo_a.remote("origin", &remote_path).unwrap();
        origin_a.push(&["refs/heads/main:refs/heads/main"], None).unwrap();
        drop(origin_a);
        drop(repo_a);

        // Clone B from the bare remote (shares the base commit with A).
        let dir_b = TempDir::new().unwrap();
        let path_b = dir_b.path().to_string_lossy().to_string();
        let _repo_b = Repository::clone(&remote_path, &path_b).unwrap();

        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };

        // Repo A pushes a second commit — remote is now one ahead of B's base.
        fs::write(dir_a.path().join("a_extra.txt"), "from A").unwrap();
        svc.stage(&path_a, &["a_extra.txt"]).unwrap();
        svc.commit(&path_a, "A second commit").unwrap();
        svc.push(&path_a, "origin", &creds).unwrap();

        // Repo B also makes a commit on its stale base and tries to push — must fail.
        fs::write(dir_b.path().join("b_extra.txt"), "from B").unwrap();
        svc.stage(&path_b, &["b_extra.txt"]).unwrap();
        svc.commit(&path_b, "B commit on outdated base").unwrap();

        let result = svc.push(&path_b, "origin", &creds);
        assert!(result.is_err(), "non-fast-forward push must return Err, got Ok");
    }
```

- [ ] **Step 2: Run the tests**

```bash
cargo test -p rocket-git push_advances_remote_head push_fails_with_non_fast_forward -- --nocapture 2>&1 | tail -20
```

Expected: `2 passed`

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "test(rocket-git): push happy path and non-fast-forward failure"
```

---

## Task 4: stash_drop tests

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Write the two tests**

Append inside the existing test module:

```rust
    #[test]
    fn stash_drop_removes_entry_at_index() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "stash this").unwrap();
        svc.stash_save(&path, "drop me").unwrap();
        assert_eq!(svc.stash_list(&path).unwrap().len(), 1, "stash must exist before drop");
        svc.stash_drop(&path, 0).unwrap();
        assert!(
            svc.stash_list(&path).unwrap().is_empty(),
            "stash list must be empty after drop"
        );
    }

    #[test]
    fn stash_drop_out_of_range_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        // No stashes exist — index 99 must error.
        let result = svc.stash_drop(&path, 99);
        assert!(result.is_err(), "drop with out-of-range index must fail");
    }
```

- [ ] **Step 2: Run the tests**

```bash
cargo test -p rocket-git stash_drop_removes_entry_at_index stash_drop_out_of_range_fails -- --nocapture 2>&1 | tail -20
```

Expected: `2 passed`

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "test(rocket-git): stash_drop happy path and out-of-range failure"
```

---

## Task 5: conflict tests

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

Background: these tests use the same conflict-setup pattern as the existing `abort_merge_resets_to_head` test — create two branches with conflicting changes to `test.bru`, then start a merge to put the repo in a conflict state.

The `ConflictResolution` enum is `ConflictResolution::Ours` (keep current branch content) and `ConflictResolution::Theirs` (keep the merging branch content). It lives in `crates/rocket-git/src/conflict.rs`.

- [ ] **Step 1: Write the three tests**

Append inside the existing test module:

```rust
    #[test]
    fn conflicts_listed_after_merge_conflict() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Branch writes "branch content" to test.bru.
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "branch content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch commit").unwrap();

        // Main writes "main content" — guaranteed conflict.
        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "main content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "main commit").unwrap();

        // Start merge without aborting — leaves repo in conflict state.
        let _ = svc.merge_branch(&path, "conflict-branch");

        let conflicts = svc.conflicts(&path).unwrap();
        assert!(!conflicts.is_empty(), "conflicts must be non-empty after a conflicting merge");
        assert!(
            conflicts.iter().any(|c| c.path == "test.bru"),
            "test.bru must appear in the conflict list"
        );
    }

    #[test]
    fn resolve_conflict_ours_writes_local_content() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Branch content = "theirs content"; main content = "ours content".
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "theirs content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch commit").unwrap();

        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "ours content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "main commit").unwrap();

        let _ = svc.merge_branch(&path, "conflict-branch");

        svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Ours).unwrap();

        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "ours content", "Ours resolution must keep main branch content");
    }

    #[test]
    fn resolve_conflict_theirs_writes_remote_content() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Branch content = "theirs content"; main content = "ours content".
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "theirs content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch commit").unwrap();

        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "ours content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "main commit").unwrap();

        let _ = svc.merge_branch(&path, "conflict-branch");

        svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Theirs).unwrap();

        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "theirs content", "Theirs resolution must keep incoming branch content");
    }
```

- [ ] **Step 2: Run the tests**

```bash
cargo test -p rocket-git conflicts_listed_after_merge_conflict resolve_conflict_ours_writes_local_content resolve_conflict_theirs_writes_remote_content -- --nocapture 2>&1 | tail -30
```

Expected: `3 passed`

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "test(rocket-git): conflicts, resolve_conflict ours/theirs contract tests"
```

---

## Task 6: delete_branch failure test

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Write the test**

Append inside the existing test module:

```rust
    #[test]
    fn delete_checked_out_branch_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.create_branch(&path, "feature-x").unwrap();
        svc.switch_branch(&path, "feature-x").unwrap();
        // feature-x is now checked out — deleting it must fail.
        let result = svc.delete_branch(&path, "feature-x");
        assert!(result.is_err(), "deleting the currently checked-out branch must fail");
    }
```

- [ ] **Step 2: Run the test**

```bash
cargo test -p rocket-git delete_checked_out_branch_fails -- --nocapture 2>&1 | tail -10
```

Expected: `1 passed`

- [ ] **Step 3: Run the full rocket-git test suite to confirm no regressions**

```bash
cargo test -p rocket-git -- --nocapture 2>&1 | tail -20
```

Expected: all tests pass (previously 33 + the 14 new = 47 total)

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "test(rocket-git): delete checked-out branch failure test"
```
