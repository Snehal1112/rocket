# Git Code Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve git implementation quality through integration tests for untested critical paths, frontend consistency fixes, reduced IPC calls in stageAll, and complete tracing instrumentation.

**Architecture:** Four independent tasks: Q2 adds Rust integration tests for push/pull/conflict-resolution using bare repo fixtures (no network). Q3 adds `initRepo` store action and wires GitPanel to use it. Q5 inlines stageAll to eliminate one redundant git_status call. Q7 adds `#[tracing::instrument]` to all uninstrumented GitService impl methods. Q6 updates the doc comment on `build_simple_diff`.

**Tech Stack:** Rust/libgit2 (`git2` crate), React/TypeScript, Zustand, `tracing` crate.

---

## File Map

| File | Change |
|------|--------|
| `crates/rocket-git/src/git2_service.rs` | Q2 (new tests), Q6 (doc comment), Q7 (tracing attributes) |
| `src/stores/git-store.ts` | Q3 (add `initRepo` action), Q5 (inline stageAll) |
| `src/components/git/GitPanel.tsx` | Q3 (use `initRepo` instead of direct `gitInit`) |

---

## Task 1 — Q2: Integration tests for push, pull, fetch, conflict resolution, and abort merge

**Spec ref:** Q2 — No integration tests for push/pull/fetch/conflict-resolution/abort-merge.

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs` — add tests to the `#[cfg(test)]` block

### Background

`setup_repo_with_remote()` already exists (added during B7 fix) at line ~1463. It creates a bare remote and clones it locally with one commit on `main`. All new tests reuse it. All tests use local file-path remotes — no network required. Credentials use `GitCredentials::SshAgent` as a no-op placeholder (local paths don't invoke credential callbacks).

**Run tests with:** `cargo test -p rocket-git <test_name> -- --nocapture`

- [ ] **Step 1: Add `pull_fast_forward_updates_branch` test**

Append to the `#[cfg(test)]` block (before closing `}`):

```rust
#[test]
fn pull_fast_forward_updates_branch() {
    let (local_dir, local_path, _remote_dir, remote_path) = setup_repo_with_remote();
    let svc = Git2Service::new();
    let creds = crate::credentials::GitCredentials::SshAgent;

    // First push local main to remote so remote has a commit.
    svc.push(&local_path, "origin", &creds).unwrap();

    // Now add a commit directly to the bare remote via a second clone.
    let clone2_dir = TempDir::new().unwrap();
    let clone2_path = clone2_dir.path().to_string_lossy().to_string();
    git2::build::RepoBuilder::new()
        .clone(&remote_path, clone2_dir.path())
        .unwrap();
    let clone2 = git2::Repository::open(&clone2_path).unwrap();
    let sig = git2::Signature::now("T", "t@t.com").unwrap();
    std::fs::write(clone2_dir.path().join("remote_change.bru"), "from remote").unwrap();
    let mut idx = clone2.index().unwrap();
    idx.add_path(std::path::Path::new("remote_change.bru")).unwrap();
    idx.write().unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = clone2.find_tree(tree_id).unwrap();
    let head = clone2.head().unwrap().peel_to_commit().unwrap();
    clone2.commit(Some("HEAD"), &sig, &sig, "remote commit", &tree, &[&head]).unwrap();
    clone2.find_remote("origin").unwrap()
        .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

    // Pull into the original local repo — should fast-forward.
    let result = svc.pull(&local_path, "origin", &creds);
    assert!(result.is_ok(), "fast-forward pull failed: {:?}", result);

    // The new file from the remote commit must now exist locally.
    assert!(
        local_dir.path().join("remote_change.bru").exists(),
        "pulled file not present after fast-forward pull"
    );

    drop(local_dir);
    drop(clone2_dir);
}
```

- [ ] **Step 2: Run test — must PASS**

```bash
cargo test -p rocket-git pull_fast_forward_updates_branch -- --nocapture
```

Expected: PASS.

- [ ] **Step 3: Add `push_and_pull_roundtrip` test**

```rust
#[test]
fn push_and_pull_roundtrip() {
    let (local_dir, local_path, _remote_dir, remote_path) = setup_repo_with_remote();
    let svc = Git2Service::new();
    let creds = crate::credentials::GitCredentials::SshAgent;

    // Push local main to remote.
    svc.push(&local_path, "origin", &creds).unwrap();

    // Clone the remote into a second local dir.
    let clone2_dir = TempDir::new().unwrap();
    git2::build::RepoBuilder::new()
        .clone(&remote_path, clone2_dir.path())
        .unwrap();

    // Add a commit in clone2 and push it.
    let clone2 = git2::Repository::open(clone2_dir.path()).unwrap();
    let sig = git2::Signature::now("T", "t@t.com").unwrap();
    std::fs::write(clone2_dir.path().join("roundtrip.bru"), "roundtrip").unwrap();
    let mut idx = clone2.index().unwrap();
    idx.add_path(std::path::Path::new("roundtrip.bru")).unwrap();
    idx.write().unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = clone2.find_tree(tree_id).unwrap();
    let head = clone2.head().unwrap().peel_to_commit().unwrap();
    clone2.commit(Some("HEAD"), &sig, &sig, "roundtrip commit", &tree, &[&head]).unwrap();
    clone2.find_remote("origin").unwrap()
        .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

    // Pull in local1 and verify the file arrived.
    svc.pull(&local_path, "origin", &creds).unwrap();
    assert!(
        local_dir.path().join("roundtrip.bru").exists(),
        "roundtrip file not present after pull"
    );

    drop(local_dir);
    drop(clone2_dir);
}
```

- [ ] **Step 4: Run test — must PASS**

```bash
cargo test -p rocket-git push_and_pull_roundtrip -- --nocapture
```

- [ ] **Step 5: Add `resolve_conflict_ours_stages_local_version` test**

```rust
#[test]
fn resolve_conflict_ours_stages_local_version() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();

    // Create conflicting state via merge_branch (B2 fix ensures index is written).
    svc.create_branch(&path, "feature").unwrap();
    std::fs::write(dir.path().join("test.bru"), "feature version").unwrap();
    let repo = git2::Repository::open(&path).unwrap();
    let mut idx = repo.index().unwrap();
    idx.add_path(std::path::Path::new("test.bru")).unwrap();
    idx.write().unwrap();
    let sig = git2::Signature::now("T", "t@t.com").unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "feature", &tree, &[&head]).unwrap();

    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
    std::fs::write(dir.path().join("test.bru"), "main version").unwrap();
    let mut idx2 = repo.index().unwrap();
    idx2.add_path(std::path::Path::new("test.bru")).unwrap();
    idx2.write().unwrap();
    let tree_id2 = idx2.write_tree().unwrap();
    let tree2 = repo.find_tree(tree_id2).unwrap();
    let head2 = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "main", &tree2, &[&head2]).unwrap();
    svc.merge_branch(&path, "feature").unwrap_err(); // produces conflict

    // Resolve using Ours strategy.
    svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Ours).unwrap();

    // File on disk must contain the local (main) version.
    let content = std::fs::read_to_string(dir.path().join("test.bru")).unwrap();
    assert_eq!(content.trim(), "main version", "expected 'main version', got: {content}");

    // File must be staged (no longer in conflict list).
    let conflicts = svc.conflicts(&path).unwrap();
    assert!(
        !conflicts.iter().any(|c| c.path == "test.bru"),
        "test.bru still in conflicts after Ours resolution"
    );
}
```

- [ ] **Step 6: Run test — must PASS**

```bash
cargo test -p rocket-git resolve_conflict_ours_stages_local_version -- --nocapture
```

- [ ] **Step 7: Add `resolve_conflict_theirs_stages_remote_version` test**

```rust
#[test]
fn resolve_conflict_theirs_stages_remote_version() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();

    // Same conflict setup as above.
    svc.create_branch(&path, "feature").unwrap();
    std::fs::write(dir.path().join("test.bru"), "feature version").unwrap();
    let repo = git2::Repository::open(&path).unwrap();
    let mut idx = repo.index().unwrap();
    idx.add_path(std::path::Path::new("test.bru")).unwrap();
    idx.write().unwrap();
    let sig = git2::Signature::now("T", "t@t.com").unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "feature", &tree, &[&head]).unwrap();

    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
    std::fs::write(dir.path().join("test.bru"), "main version").unwrap();
    let mut idx2 = repo.index().unwrap();
    idx2.add_path(std::path::Path::new("test.bru")).unwrap();
    idx2.write().unwrap();
    let tree_id2 = idx2.write_tree().unwrap();
    let tree2 = repo.find_tree(tree_id2).unwrap();
    let head2 = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "main", &tree2, &[&head2]).unwrap();
    svc.merge_branch(&path, "feature").unwrap_err();

    // Resolve using Theirs strategy.
    svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Theirs).unwrap();

    // File on disk must contain the incoming (feature) version.
    let content = std::fs::read_to_string(dir.path().join("test.bru")).unwrap();
    assert_eq!(content.trim(), "feature version", "expected 'feature version', got: {content}");

    let conflicts = svc.conflicts(&path).unwrap();
    assert!(
        !conflicts.iter().any(|c| c.path == "test.bru"),
        "test.bru still in conflicts after Theirs resolution"
    );
}
```

- [ ] **Step 8: Run test — must PASS**

```bash
cargo test -p rocket-git resolve_conflict_theirs_stages_remote_version -- --nocapture
```

- [ ] **Step 9: Add `abort_merge_resets_to_head` test**

```rust
#[test]
fn abort_merge_resets_to_head() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();

    // Create conflict state.
    svc.create_branch(&path, "feature").unwrap();
    std::fs::write(dir.path().join("test.bru"), "feature version").unwrap();
    let repo = git2::Repository::open(&path).unwrap();
    let mut idx = repo.index().unwrap();
    idx.add_path(std::path::Path::new("test.bru")).unwrap();
    idx.write().unwrap();
    let sig = git2::Signature::now("T", "t@t.com").unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "feature", &tree, &[&head]).unwrap();

    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
    std::fs::write(dir.path().join("test.bru"), "main version").unwrap();
    let mut idx2 = repo.index().unwrap();
    idx2.add_path(std::path::Path::new("test.bru")).unwrap();
    idx2.write().unwrap();
    let tree_id2 = idx2.write_tree().unwrap();
    let tree2 = repo.find_tree(tree_id2).unwrap();
    let head2 = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "main", &tree2, &[&head2]).unwrap();
    svc.merge_branch(&path, "feature").unwrap_err();

    // Abort the merge.
    svc.abort_merge(&path).unwrap();

    // After abort: no conflicts, file restored to HEAD state ("main version").
    let conflicts = svc.conflicts(&path).unwrap();
    assert!(conflicts.is_empty(), "conflicts should be empty after abort");

    let content = std::fs::read_to_string(dir.path().join("test.bru")).unwrap();
    assert_eq!(content.trim(), "main version", "file not restored after abort");
}
```

- [ ] **Step 10: Run test — must PASS**

```bash
cargo test -p rocket-git abort_merge_resets_to_head -- --nocapture
```

- [ ] **Step 11: Run full test suite**

```bash
cargo test -p rocket-git -- --nocapture 2>&1 | tail -5
```

Expected: all new tests PASS, no new failures.

- [ ] **Step 12: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "test(git): add integration tests for push/pull roundtrip and conflict resolution"
```

---

## Task 2 — Q3: Add `initRepo` store action; wire GitPanel to use it

**Spec ref:** Q3 — `GitPanel` calls `gitInit` directly, bypassing the store.

**Files:**
- Modify: `src/stores/git-store.ts` — add `initRepo` action
- Modify: `src/components/git/GitPanel.tsx` — use `initRepo` instead of `gitInit`

### Background

`GitPanel.tsx` lines 155–158:
```tsx
onClick={async () => {
  await gitInit(collectionPath);
  await checkAndLoad(collectionPath);
}}
```

This directly calls the Tauri API and then manually re-runs `checkAndLoad`. The fix: add `initRepo(path)` to the git store that calls `gitInit` then `setCollection`. `GitPanel` then just calls `store.initRepo(collectionPath)` and reads `storeIsRepo` to update its local state.

- [ ] **Step 1: Add `gitInit` import and `initRepo` action to `git-store.ts`**

In `src/stores/git-store.ts`, add `gitInit` to the import from `@/lib/tauri-api`:

```ts
import {
  // ... existing imports ...
  gitInit,
  // ...
} from '@/lib/tauri-api';
```

Add `initRepo` to the `GitState` interface (after the `reset` line):

```ts
initRepo: (path: string) => Promise<void>;
```

Add the implementation inside the `create<GitState>` call (after `reset`):

```ts
initRepo: async (path: string) => {
  try {
    await gitInit(path);
    await get().setCollection(path);
  } catch (e) {
    set({ error: String(e) });
  }
},
```

- [ ] **Step 2: Update `GitPanel.tsx` to use the store action**

In `src/components/git/GitPanel.tsx`:

1. Remove the `gitInit` import from `@/lib/tauri-api` (find the line `import { gitInit, onCollectionChanged } from '@/lib/tauri-api'` and remove `gitInit` from it).

2. Destructure `initRepo` from `useGitStore` in the component. Find the `useGitStore()` destructure block and add `initRepo`:

```tsx
const {
  showCredentialsDialog,
  setCollection,
  refreshLog,
  refreshStashes,
  refreshStatus,
  status,
  collectionPath: loadedPath,
  isRepo: storeIsRepo,
  initRepo,
} = useGitStore();
```

3. Replace the "Initialize Git" button's `onClick` handler (lines 155–158):

```tsx
onClick={async () => {
  await initRepo(collectionPath);
  setIsRepo(useGitStore.getState().isRepo);
}}
```

- [ ] **Step 3: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/stores/git-store.ts src/components/git/GitPanel.tsx
git commit -m "refactor(git): add initRepo store action; GitPanel no longer calls gitInit directly"
```

---

## Task 3 — Q5: Eliminate redundant `git_status` call in `stageAll`

**Spec ref:** Q5 — `stageAll` makes 3 `git_status` IPC calls (should be 2).

**Files:**
- Modify: `src/stores/git-store.ts:288–302`

### Background

Current `stageAll`:
```ts
stageAll: async () => {
  await get().refreshStatus();          // call 1
  const { status } = get();
  ...
  await get().stageFiles(unstaged);    // stageFiles → gitStage → refreshStatus() = call 2 + call 3
}
```

The fix: inline the gitStage call directly in `stageAll`, doing only 2 `git_status` calls (one before, one after staging).

- [ ] **Step 1: Replace `stageAll` in `git-store.ts`**

Find the `stageAll` action (lines 288–302) and replace it:

```ts
// Stage every modified file that is not yet staged.
stageAll: async () => {
  // Always refresh before reading so we never stage from a stale cache
  // that could contain directory-level entries (trailing '/') from an
  // older status response.
  await get().refreshStatus();
  const { collectionPath, status } = get();
  if (!collectionPath || !status) return;
  const paths = status.files
    .filter((f: FileStatus) => !f.staged && f.status !== 'unchanged')
    .map((f: FileStatus) => f.path);
  if (paths.length === 0) return;
  try {
    await gitStage(collectionPath, paths);
    await get().refreshStatus();
  } catch (e) {
    set({ error: String(e) });
  }
},
```

Make sure `gitStage` is already imported (it is — it's used in `stageFiles`). No new imports needed.

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/stores/git-store.ts
git commit -m "perf(git): stageAll inlines gitStage to avoid redundant git_status call"
```

---

## Task 4 — Q6 + Q7: Update `build_simple_diff` doc comment + add tracing to all uninstrumented methods

**Spec ref:** Q6 short-term (doc comment), Q7 (tracing instrumentation).

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

### Background

**Q6:** The doc comment on `build_simple_diff` says "intentionally simplistic" without explaining the consumer implications. Update it to be precise about what works correctly (`DiffEditor` via Monaco) and what should not rely on `hunks` (`VisualDiffView` uses `oldContent`/`newContent` directly — correct).

**Q7:** Methods that currently lack `#[tracing::instrument]`:
- `is_repo`, `init`, `list_remotes`, `add_remote`, `remove_remote`, `set_remote_url`
- `diff_file`, `diff_staged`, `stage`, `unstage`, `discard`, `log`
- `branches`, `switch_branch`, `checkout_remote_branch`, `create_branch`, `delete_branch`, `merge_branch`
- `stash_list`, `stash_save`, `stash_pop`, `stash_apply`, `stash_drop`
- `conflicts`, `resolve_conflict`, `abort_merge`

Pattern for regular methods (no credentials):
```rust
#[tracing::instrument(name = "git_<method_name>", skip(self), fields(repo_path = %path))]
fn method_name(&self, path: &str, ...) -> DomainResult<...> {
```

For `is_repo` (returns bool, not DomainResult, no path field):
```rust
#[tracing::instrument(name = "git_is_repo", skip(self), fields(path = %path))]
fn is_repo(&self, path: &str) -> bool {
```

For methods with `index: usize` instead of `path` as second arg (stash_pop/apply/drop):
```rust
#[tracing::instrument(name = "git_stash_pop", skip(self), fields(repo_path = %path, index = %index))]
fn stash_pop(&self, path: &str, index: usize) -> DomainResult<()> {
```

For `merge_branch` and `switch_branch` (name param):
```rust
#[tracing::instrument(name = "git_switch_branch", skip(self), fields(repo_path = %path, branch = %name))]
fn switch_branch(&self, path: &str, name: &str) -> DomainResult<()> {
```

- [ ] **Step 1: Update `build_simple_diff` doc comment (lines 135–139)**

Replace the existing doc comment:

```rust
/// Build a simple line-by-line diff producing hunks.
///
/// Produces a single hunk with all old lines as removals followed by all new
/// lines as additions. This is structurally correct for Monaco's DiffEditor,
/// which applies its own Myers diff on `oldContent`/`newContent` and ignores
/// the hunk structure. VisualDiffView also parses `oldContent`/`newContent`
/// directly and does not rely on hunks, so both consumers are unaffected.
///
/// Do NOT use `hunks` for semantic diff consumers — replace with the `similar`
/// crate for a proper Myers diff when hunk-level accuracy is needed.
fn build_simple_diff(old: &Option<String>, new: &Option<String>) -> Vec<DiffHunk> {
```

- [ ] **Step 2: Add tracing to remote management methods**

Add `#[tracing::instrument]` before each of these (find by searching for `fn <name>` in the file):

```rust
// Before fn is_repo:
#[tracing::instrument(name = "git_is_repo", skip(self), fields(path = %path))]

// Before fn init:
#[tracing::instrument(name = "git_init", skip(self), fields(repo_path = %path))]

// Before fn list_remotes:
#[tracing::instrument(name = "git_list_remotes", skip(self), fields(repo_path = %path))]

// Before fn add_remote:
#[tracing::instrument(name = "git_add_remote", skip(self), fields(repo_path = %path, name = %name))]

// Before fn remove_remote:
#[tracing::instrument(name = "git_remove_remote", skip(self), fields(repo_path = %path, name = %name))]

// Before fn set_remote_url:
#[tracing::instrument(name = "git_set_remote_url", skip(self), fields(repo_path = %path, name = %name))]
```

- [ ] **Step 3: Add tracing to diff and staging methods**

```rust
// Before fn diff_file:
#[tracing::instrument(name = "git_diff_file", skip(self), fields(repo_path = %path, file = %file))]

// Before fn diff_staged:
#[tracing::instrument(name = "git_diff_staged", skip(self), fields(repo_path = %path, file = %file))]

// Before fn stage:
#[tracing::instrument(name = "git_stage", skip(self, files), fields(repo_path = %path, count = files.len()))]

// Before fn unstage:
#[tracing::instrument(name = "git_unstage", skip(self, files), fields(repo_path = %path, count = files.len()))]

// Before fn discard:
#[tracing::instrument(name = "git_discard", skip(self, files), fields(repo_path = %path, count = files.len()))]

// Before fn log:
#[tracing::instrument(name = "git_log", skip(self), fields(repo_path = %path, limit = %limit))]
```

- [ ] **Step 4: Add tracing to branch methods**

```rust
// Before fn branches:
#[tracing::instrument(name = "git_branches", skip(self), fields(repo_path = %path))]

// Before fn switch_branch:
#[tracing::instrument(name = "git_switch_branch", skip(self), fields(repo_path = %path, branch = %name))]

// Before fn checkout_remote_branch:
#[tracing::instrument(name = "git_checkout_remote_branch", skip(self), fields(repo_path = %path, remote_branch = %remote_branch))]

// Before fn create_branch:
#[tracing::instrument(name = "git_create_branch", skip(self), fields(repo_path = %path, name = %name))]

// Before fn delete_branch:
#[tracing::instrument(name = "git_delete_branch", skip(self), fields(repo_path = %path, name = %name))]

// Before fn merge_branch:
#[tracing::instrument(name = "git_merge_branch", skip(self), fields(repo_path = %path, name = %name))]
```

- [ ] **Step 5: Add tracing to stash methods**

```rust
// Before fn stash_list:
#[tracing::instrument(name = "git_stash_list", skip(self), fields(repo_path = %path))]

// Before fn stash_save:
#[tracing::instrument(name = "git_stash_save", skip(self), fields(repo_path = %path))]

// Before fn stash_pop:
#[tracing::instrument(name = "git_stash_pop", skip(self), fields(repo_path = %path, index = %index))]

// Before fn stash_apply:
#[tracing::instrument(name = "git_stash_apply", skip(self), fields(repo_path = %path, index = %index))]

// Before fn stash_drop:
#[tracing::instrument(name = "git_stash_drop", skip(self), fields(repo_path = %path, index = %index))]
```

- [ ] **Step 6: Add tracing to conflict methods**

```rust
// Before fn conflicts:
#[tracing::instrument(name = "git_conflicts", skip(self), fields(repo_path = %path))]

// Before fn resolve_conflict:
#[tracing::instrument(name = "git_resolve_conflict", skip(self, resolution), fields(repo_path = %path, file = %file))]

// Before fn abort_merge:
#[tracing::instrument(name = "git_abort_merge", skip(self), fields(repo_path = %path))]
```

- [ ] **Step 7: Verify compilation**

```bash
cargo check -p rocket-git
```

Expected: no errors or warnings.

- [ ] **Step 8: Run full test suite**

```bash
cargo test -p rocket-git -- 2>&1 | tail -5
```

Expected: same pass/fail counts as before (tracing changes are non-functional).

- [ ] **Step 9: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "chore(git): add tracing instrumentation to all uninstrumented git methods; clarify build_simple_diff doc"
```

---

## Final Verification

- [ ] **Rust tests**

```bash
cargo test -p rocket-git -- 2>&1 | tail -5
```

- [ ] **TypeScript check**

```bash
yarn tsc --noEmit
```

- [ ] **Biome lint**

```bash
yarn check
```
