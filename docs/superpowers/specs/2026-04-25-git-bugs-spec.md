# Git Implementation — Bugs & Correctness Issues

**Date:** 2026-04-25  
**Scope:** Backend (`crates/rocket-git/`) and Frontend (`src/components/git/`, `src/stores/git-store.ts`)  
**Purpose:** Fix correctness bugs, silent failure paths, and dangerous UI behaviours identified during the full-stack git audit.

---

## Bug Inventory

### B1 — `switch_branch` uses `force` checkout, silently discarding uncommitted changes

**File:** `crates/rocket-git/src/git2_service.rs:844–851`

```rust
fn switch_branch(&self, path: &str, name: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;
    repo.set_head(&format!("refs/heads/{name}"))...;
    repo.checkout_head(Some(&mut CheckoutBuilder::new().force()))  // ← force
```

**Problem:** `CheckoutBuilder::force()` overwrites any uncommitted working-tree changes without warning. If a user has unsaved edits in requests and switches branches, those changes vanish permanently with no error or prompt. The CLI `git switch` and `git checkout` would refuse to proceed and ask the user to stash or commit first.

**Fix (backend):** Attempt a safe checkout first (no force flag). If libgit2 returns an error because of local modifications, propagate a new `DomainError::InvalidInput` with a human-readable message: _"You have uncommitted changes that would be overwritten. Please commit or stash them before switching branches."_ Only fall back to force if the caller explicitly requests it (not exposed in the API for now).

**Fix (frontend):** No change required — the error message propagates to `BranchSelector`'s `switchError` banner automatically.

---

### B2 — `merge_branch` conflict path returns wrong error variant, leaving repo in broken state

**File:** `crates/rocket-git/src/git2_service.rs:983–987`

```rust
if index.has_conflicts() {
    return Err(DomainError::Internal("merge resulted in conflicts".to_string()));
}
```

**Problem:** When `merge_branch` results in conflicts, it returns `DomainError::Internal` (not `DomainError::Conflict`), AND does NOT write the index to disk before returning. This leaves the repo with a MERGE_HEAD but an unstaged conflict index — `git_conflicts` will return an empty list because the conflicting entries were never persisted. The frontend then shows 0 conflicts, confusing the user who is stuck in a broken merge state.

**Compare to `pull` (lines 686–716):** pull correctly writes the index before returning `DomainError::Conflict`.

**Fix:** In `merge_branch`, before returning the error: write the index (`index.write()`), then return `DomainError::Conflict` with the list of conflicted filenames (same pattern as `pull`).

---

### B3 — `ConflictResolver` silently swallows resolution errors

**File:** `src/components/git/ConflictResolver.tsx:33–36`

```tsx
} catch {
  // Handle silently.
}
```

**Problem:** If `gitResolveConflict` fails (e.g. the file was deleted, the index is locked, or the path contains special characters), the error is discarded. The user clicks "Accept Ours" and nothing happens — the conflict appears to remain, with no feedback explaining why.

**Fix:** Remove the silent catch. Show an inline error banner (same pattern as `GitLandingPanel`'s error display). The store's `resolveConflict` action already sets `state.error`; read and display it.

---

### B4 — `handleConflictClick` race: `conflicts` state may be stale when `find()` runs

**File:** `src/components/git/GitFileList.tsx:45–53`

```tsx
const handleConflictClick = async (file: FileStatus) => {
  await refreshConflicts();
  const conflictFile = conflicts.find((c) => c.path === file.path);  // ← stale closure
```

**Problem:** `conflicts` is captured at render time from the Zustand store. `refreshConflicts()` mutates the store's `conflicts` array, but the local `conflicts` binding in this closure still refers to the old value. The `find()` will succeed most of the time (first load) but silently returns `undefined` on subsequent calls if the component didn't re-render between refreshes — causing the conflict view to never open.

**Fix:** Read fresh state from the store after the await: `const conflictFile = useGitStore.getState().conflicts.find(...)`.

---

### B5 — `commitChanges` does not refresh commit log after commit

**File:** `src/stores/git-store.ts:276–285`

```ts
commitChanges: async (message: string) => {
  ...
  await gitCommit(collectionPath, message);
  await get().refreshStatus();  // ← no refreshLog()
}
```

**Problem:** After committing, the Commits view (if already open) shows stale history — the new commit doesn't appear until the user navigates away and back, or manually refreshes. `refreshLog` is only called when the commits view is first opened (`GitPanel.tsx:101`).

**Fix:** Add `await get().refreshLog()` after `refreshStatus()` in `commitChanges`. Since `refreshLog` is a no-op when the log isn't currently being used (it just updates store state that no component is subscribed to), the cost is one extra IPC call on commit — acceptable.

---

### B6 — `files_changed` is always `0` in `CommitInfo` from `commit()` and `log()`

**File:** `crates/rocket-git/src/git2_service.rs:537–545` (commit) and `563–572` (log)

```rust
CommitInfo {
    ...
    files_changed: 0,  // ← hardcoded
}
```

**Problem:** The `files_changed` field is populated in `CommitLog` UI and `StashSection` stat display, but is always 0 for commits from `commit()` and `log()`. For stashes it is correct (lines 1062–1092 compute it via `diff_tree_to_tree`). Commits show "0 files" in the log view.

**Fix (commit):** After creating the commit, diff `HEAD~1..HEAD` (parent tree vs new tree) to count changed files. For the initial commit (no parent), diff against an empty tree.

**Fix (log):** For each commit in `revwalk`, diff the commit's tree against its first parent's tree (or empty tree if root commit) and count `stats.files_changed()`.

---

### B7 — `push` hardcodes `refs/heads/<branch>:refs/heads/<branch>` refspec; fails for repos where local and remote branch names differ

**File:** `crates/rocket-git/src/git2_service.rs:588–589`

```rust
let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
```

**Problem:** If the local branch `feature-x` tracks `origin/feat-x`, the push will create `refs/heads/feature-x` on the remote instead of updating `refs/heads/feat-x`. This is a subtle correctness bug — in most cases it passes, but for diverged branch name conventions (common in fork workflows) it silently pushes to the wrong ref.

**Fix:** Check for a configured upstream push refspec first: `repo.find_branch(branch, Local)?.upstream()`. If present, use its refname as the remote side of the refspec. Fall back to same-name if no upstream is configured.

---

### B8 — `handleStashAndPull` in `GitLandingPanel` does not handle partial failure cleanly

**File:** `src/components/git/GitLandingPanel.tsx:79–91`

```tsx
const handleStashAndPull = async () => {
  await saveStash('Auto-stash before pull');
  await pull();
  await popStash(0);  // ← no check for pull conflict
}
```

**Problem:** If `pull()` produces merge conflicts, `popStash(0)` is called immediately, which applies the stash on top of a conflicted working tree. This doubles the conflict count and corrupts the index state. The catch block only runs if `popStash` throws, not if `pull` produced conflicts.

**Fix:** After `pull()`, check `useGitStore.getState().hasConflicts()`. If conflicts exist, do NOT pop the stash — instead, show a banner: _"Pull produced conflicts. Resolve them first, then pop stash@{0} manually."_

---

## Summary Table

| ID | Severity | Area | Description |
|----|----------|------|-------------|
| B1 | High | Backend/UX | `switch_branch` force-discards uncommitted changes silently |
| B2 | High | Backend | `merge_branch` conflict path: wrong error variant + index not written |
| B3 | Medium | Frontend | `ConflictResolver` silently swallows resolution errors |
| B4 | Medium | Frontend | `handleConflictClick` reads stale Zustand `conflicts` closure |
| B5 | Low | Frontend | `commitChanges` doesn't refresh commit log after commit |
| B6 | Low | Backend | `files_changed` always 0 in `commit()` and `log()` |
| B7 | Medium | Backend | `push` uses hardcoded same-name refspec regardless of upstream config |
| B8 | High | Frontend | Stash-and-pull applies stash into conflicted state |

---

## Testing

For each bug fix, the following test approach applies:

- **B1:** `cargo test -p rocket-git` — add `switch_branch_refuses_when_dirty` test using `tempfile` repo, write uncommitted change, assert `DomainError::InvalidInput` is returned.
- **B2:** Add `merge_branch_with_conflict_returns_conflict_error` test. After the merge call, assert `DomainError::Conflict`, then call `conflicts()` and assert non-empty list.
- **B3–B5, B8:** Frontend integration testing — manually verify in the running app that errors surface and stash pop is blocked.
- **B6:** Add `commit_returns_files_changed_count` and `log_returns_files_changed_count` unit tests.
- **B7:** Add `push_uses_upstream_refspec_when_configured` test (requires a bare remote repo as fixture).
