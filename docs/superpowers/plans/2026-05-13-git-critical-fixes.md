# Git Critical Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two critical issues found in code review: (1) the hardcoded `"RocketAPI User"` identity fallback that survived in the merge-commit path of `pull()` in `remote.rs`, and (2) a TypeScript type error in the git store tests where `status` literals are not narrowed to `GitStatusKind`.

**Architecture:** The Rust fix lives entirely inside `crates/rocket-git/src/git2_service/remote.rs` — the `pull()` function's non-fast-forward merge path. The change removes the `.or_else` fallback and propagates `DomainError` the same way `staging::commit()` already does. The TypeScript fix is a one-line cast in the test file. Neither change touches any IPC contract, frontend component, or Zustand store.

**Tech Stack:** Rust / git2 crate (`rocket-git` crate), TypeScript / Vitest (`git-store.test.ts`)

---

## Files

| Action | Path | Purpose |
|--------|------|---------|
| Modify | `crates/rocket-git/src/git2_service/remote.rs` | Remove fallback signature in `pull()` merge-commit path |
| Modify | `crates/rocket-git/src/git2_service/mod.rs` | Add hermetic test: `pull_merge_commit_fails_without_identity` |
| Modify | `src/stores/__tests__/git-store.test.ts` | Fix `FileStatus` type narrowing on `status` literal |

---

## Task 1: Fix the hardcoded identity fallback in `remote.rs`

The `pull()` function in `crates/rocket-git/src/git2_service/remote.rs` creates a merge commit when the history has diverged (non-fast-forward). Lines 281-283 contain:

```rust
let sig = repo
    .signature()
    .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
    .map_err(|e| DomainError::Internal(e.to_string()))?;
```

This silently stamps `RocketAPI User` as the merge commit author when git config has no identity. The fix mirrors what `staging::commit()` already does: let `repo.signature()` fail and surface a `DomainError::Internal`.

**Files:**
- Modify: `crates/rocket-git/src/git2_service/remote.rs:280-283`

- [ ] **Step 1: Replace the signature block**

Open `crates/rocket-git/src/git2_service/remote.rs`. Find this block (around line 280):

```rust
    let sig = repo
        .signature()
        .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
```

Replace it with:

```rust
    let sig = repo
        .signature()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
```

- [ ] **Step 2: Verify the crate compiles**

```bash
cargo check -p rocket-git
```

Expected: zero errors, zero warnings about this change.

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-git/src/git2_service/remote.rs
git commit -m "fix(git): remove hardcoded identity fallback from pull merge-commit path"
```

---

## Task 2: Add a hermetic test for the pull merge-commit identity guard

The existing `commit_fails_without_identity` test (in `mod.rs`) verifies that `staging::commit()` rejects a missing identity. We need an equivalent test for the `pull()` merge-commit path added in Task 1.

The test must be **hermetic**: it must not pass on a CI machine that has a global `~/.gitconfig` with `user.name` set. We achieve this by writing an empty string for `user.name` and `user.email` into the **local** repo config, which takes priority over any global config under `git2`.

**Files:**
- Modify: `crates/rocket-git/src/git2_service/mod.rs` — add test inside the existing `#[cfg(test)]` block

- [ ] **Step 1: Understand the existing diverged-history pull test**

Read the test `pull_with_diverged_history_merges_and_clears_behind` (around line 975 in `mod.rs`). This test already sets up two repos with diverged history so that `pull()` must create a merge commit. We need the same topology but **without** a local identity set in the pulling repo's config.

- [ ] **Step 2: Write the failing test**

Append this test inside the `#[cfg(test)]` module in `crates/rocket-git/src/git2_service/mod.rs`, after the existing `commit_succeeds_with_identity` test:

```rust
#[test]
fn pull_merge_commit_fails_without_identity() {
    // Set up a bare remote.
    let remote_dir = TempDir::new().unwrap();
    let remote_path = remote_dir.path().to_string_lossy().to_string();
    Repository::init_bare(&remote_path).unwrap();

    // Create a local repo with a shared base commit but NO identity in local config.
    // Note: git2 reads local → global → system config. Writing empty strings to
    // local config overrides any identity the CI runner may have in ~/.gitconfig.
    let local_dir = TempDir::new().unwrap();
    let local_path = local_dir.path().to_string_lossy().to_string();
    let local_repo = Repository::init(&local_path).unwrap();
    local_repo.set_head("refs/heads/main").ok();
    {
        let cfg = local_repo.config().unwrap();
        let mut local_cfg = cfg.open_level(git2::ConfigLevel::Local).unwrap();
        local_cfg.set_str("user.name", "").unwrap();
        local_cfg.set_str("user.email", "").unwrap();
    }

    // Create the shared base commit using an explicit signature (bypasses repo.signature()).
    let setup_sig = git2::Signature::now("Setup", "setup@test.com").unwrap();
    fs::write(local_dir.path().join("base.txt"), "base").unwrap();
    let mut idx = local_repo.index().unwrap();
    idx.add_path(Path::new("base.txt")).unwrap();
    idx.write().unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = local_repo.find_tree(tree_id).unwrap();
    local_repo
        .commit(Some("refs/heads/main"), &setup_sig, &setup_sig, "base", &tree, &[])
        .unwrap();

    // Push the base commit to the bare remote.
    let mut remote_obj = local_repo.remote("origin", &remote_path).unwrap();
    remote_obj
        .push(&["refs/heads/main:refs/heads/main"], None)
        .unwrap();
    drop(remote_obj);

    // Add a LOCAL commit so histories diverge (local is ahead by 1).
    fs::write(local_dir.path().join("local.txt"), "local change").unwrap();
    let mut idx2 = local_repo.index().unwrap();
    idx2.add_path(Path::new("local.txt")).unwrap();
    idx2.write().unwrap();
    let tree_id2 = idx2.write_tree().unwrap();
    let tree2 = local_repo.find_tree(tree_id2).unwrap();
    let head = local_repo.head().unwrap().peel_to_commit().unwrap();
    local_repo
        .commit(Some("refs/heads/main"), &setup_sig, &setup_sig, "local", &tree2, &[&head])
        .unwrap();
    drop(local_repo);

    // Add a REMOTE commit via a second repo so remote is also ahead of base by 1.
    let other_dir = TempDir::new().unwrap();
    let other_repo = Repository::clone(&remote_path, other_dir.path()).unwrap();
    {
        let cfg = other_repo.config().unwrap();
        let mut local_cfg = cfg.open_level(git2::ConfigLevel::Local).unwrap();
        local_cfg.set_str("user.name", "Other").unwrap();
        local_cfg.set_str("user.email", "other@test.com").unwrap();
    }
    fs::write(other_dir.path().join("remote.txt"), "remote change").unwrap();
    let mut oi = other_repo.index().unwrap();
    oi.add_path(Path::new("remote.txt")).unwrap();
    oi.write().unwrap();
    let otid = oi.write_tree().unwrap();
    let otree = other_repo.find_tree(otid).unwrap();
    let ohead = other_repo.head().unwrap().peel_to_commit().unwrap();
    let other_sig = git2::Signature::now("Other", "other@test.com").unwrap();
    other_repo
        .commit(Some("refs/heads/main"), &other_sig, &other_sig, "remote", &otree, &[&ohead])
        .unwrap();
    other_repo
        .find_remote("origin")
        .unwrap()
        .push(&["refs/heads/main:refs/heads/main"], None)
        .unwrap();

    // Pull must fail with a DomainError — identity is missing in local config.
    let svc = Git2Service::new();
    let creds = GitCredentials::UserPass {
        username: String::new(),
        password: String::new(),
    };
    let result = svc.pull(&local_path, "origin", &creds);
    assert!(
        result.is_err(),
        "pull merge commit must fail when git identity is missing, got: {:?}",
        result
    );
}
```

- [ ] **Step 3: Run the test to confirm it fails before the fix**

> **Note:** Task 1 must be completed first — this test should already pass after Task 1. Run it now to verify:

```bash
cargo test -p rocket-git pull_merge_commit_fails_without_identity -- --nocapture
```

Expected: **PASS** (the fallback was already removed in Task 1).

- [ ] **Step 4: Run the full `rocket-git` test suite to check for regressions**

```bash
cargo test -p rocket-git
```

Expected: all tests pass. The existing `pull_with_diverged_history_merges_and_clears_behind` test must still pass — it uses `setup_repo()` which sets a real identity, so the merge commit succeeds.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-git/src/git2_service/mod.rs
git commit -m "test(git): verify pull merge commit fails without git identity"
```

---

## Task 3: Fix the TypeScript type error in `git-store.test.ts`

The test `stageAll stages only unstaged non-unchanged files` (around line 306 in `src/stores/__tests__/git-store.test.ts`) passes a `files` array to `vi.mocked(gitStatus).mockResolvedValueOnce`. The `status` field on each element is inferred as `string` instead of `GitStatusKind`, which causes a TypeScript error because `RepoStatus.files` is typed as `FileStatus[]` where `FileStatus.status: GitStatusKind`.

**Files:**
- Modify: `src/stores/__tests__/git-store.test.ts:308-318`

- [ ] **Step 1: Confirm the current TypeScript error**

```bash
yarn tsc --noEmit 2>&1 | grep git-store.test
```

Expected output contains something like:
```
src/stores/__tests__/git-store.test.ts(319,7): error TS2345: Argument of type ... is not assignable ...
```

- [ ] **Step 2: Fix the type — add `as const` to the status literals**

Find this block in `src/stores/__tests__/git-store.test.ts`:

```typescript
    const files = [
      { path: 'already-staged.bru', status: 'modified', staged: true },
      { path: 'unstaged-modified.bru', status: 'modified', staged: false },
      { path: 'unchanged.bru', status: 'unchanged', staged: false },
    ];
```

Replace it with:

```typescript
    const files: import('@/lib/tauri-api').FileStatus[] = [
      { path: 'already-staged.bru', status: 'modified', staged: true },
      { path: 'unstaged-modified.bru', status: 'modified', staged: false },
      { path: 'unchanged.bru', status: 'unchanged', staged: false },
    ];
```

- [ ] **Step 3: Verify TypeScript passes**

```bash
yarn tsc --noEmit
```

Expected: zero errors.

- [ ] **Step 4: Run the tests to confirm no regressions**

```bash
yarn test git-store
```

Expected: all tests pass (53 tests).

- [ ] **Step 5: Commit**

```bash
git add src/stores/__tests__/git-store.test.ts
git commit -m "fix(test): narrow FileStatus.status to GitStatusKind in git-store test"
```

---

## Self-Review

**Spec coverage:**
- Critical #1 (hardcoded fallback in `remote.rs`) → Task 1 removes it, Task 2 adds regression test. ✓
- Critical #2 (TypeScript type error) → Task 3 fixes it. ✓

**Placeholder scan:** No TBDs, no vague steps. All code blocks are complete and exact.

**Type consistency:**
- `DomainError::Internal` — used identically to `staging::commit()`. ✓
- `FileStatus` — imported from `@/lib/tauri-api`, same module where it is defined. ✓
- `GitCredentials::UserPass` — same pattern as all other pull tests in `mod.rs`. ✓
- `Git2Service::new()` — same pattern as all other service tests. ✓

**Regression safety check:**
- The `pull_with_diverged_history_merges_and_clears_behind` test uses `setup_repo_with_remote()` which calls `setup_repo()` — which sets `user.name = "Test"` and `user.email = "test@test.com"` in local config. That test will still pass because identity is present.
- All fast-forward pull tests never reach the merge-commit code path, so they are unaffected.
- The `stageAll` test logic is unchanged; only the type annotation is added.
