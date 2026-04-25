# Git Bugs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 8 correctness bugs in the Rocket git implementation — covering data-loss risks, broken merge state, stale UI closures, and missing state refreshes.

**Architecture:** Bugs are fixed at the layer they originate: backend fixes go in `crates/rocket-git/src/git2_service.rs`, frontend fixes go in `src/components/git/` and `src/stores/git-store.ts`. Each task is self-contained and commits independently. Backend fixes are tested with `cargo test -p rocket-git`; frontend fixes are verified manually.

**Tech Stack:** Rust/libgit2 (`git2` crate), React/TypeScript, Zustand, Tauri IPC, Vitest (frontend tests not required for these bugs — behavior is verified by backend unit tests and manual app testing).

---

## File Map

| File | Change |
|------|--------|
| `crates/rocket-git/src/git2_service.rs` | B1 (switch_branch), B2 (merge_branch), B6 (files_changed), B7 (push refspec) |
| `src/components/git/ConflictResolver.tsx` | B3 (surface errors), B4 is in GitFileList |
| `src/components/git/GitFileList.tsx` | B4 (stale closure fix) |
| `src/stores/git-store.ts` | B5 (refreshLog after commit) |
| `src/components/git/GitLandingPanel.tsx` | B8 (stash-and-pull conflict guard) |

---

## Task 1 — B1: `switch_branch` must refuse when working tree has uncommitted changes

**Spec ref:** B1 — `switch_branch` uses `force` checkout, silently discarding uncommitted changes.

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs:844–851`

### Background
`switch_branch` currently calls `repo.checkout_head(Some(&mut CheckoutBuilder::new().force()))`. The `force()` flag discards any uncommitted working-tree changes without warning — equivalent to `git checkout -f`. The fix: attempt a safe checkout first; if libgit2 reports a conflict (local modifications would be overwritten), return a descriptive `DomainError::InvalidInput`.

- [ ] **Step 1: Write the failing test**

Add this test to the `#[cfg(test)]` block at the bottom of `crates/rocket-git/src/git2_service.rs` (after the last `#[test]` fn, before the closing `}`):

```rust
#[test]
fn switch_branch_refuses_when_dirty() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();

    // Create a second branch to switch to.
    svc.create_branch(&path, "other").unwrap();
    // Switch back to main first (create_branch switches HEAD).
    let repo = git2::Repository::open(&path).unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();

    // Now dirty the working tree on main.
    std::fs::write(dir.path().join("test.bru"), "dirty content").unwrap();

    // Attempting to switch to 'other' must fail with InvalidInput, not silently discard the change.
    let result = svc.switch_branch(&path, "other");
    assert!(
        matches!(result, Err(rocket_shared::error::DomainError::InvalidInput(_))),
        "expected InvalidInput when dirty, got: {:?}", result
    );

    // The dirty file must still be there — not discarded.
    let content = std::fs::read_to_string(dir.path().join("test.bru")).unwrap();
    assert_eq!(content, "dirty content");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rocket-git switch_branch_refuses_when_dirty -- --nocapture
```

Expected: FAIL — test currently panics or asserts `Ok(())` because force checkout succeeds silently.

- [ ] **Step 3: Fix `switch_branch` in `git2_service.rs`**

Replace lines 844–851:

```rust
fn switch_branch(&self, path: &str, name: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;
    repo.set_head(&format!("refs/heads/{name}"))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    // Safe checkout — will fail if local modifications would be overwritten.
    // This matches `git switch` / `git checkout` default behaviour.
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().safe()))
        .map_err(|e| {
            if e.message().contains("conflict") || e.message().contains("overwritten") {
                DomainError::InvalidInput(
                    "You have uncommitted changes that would be overwritten by switching branches. \
                     Please commit or stash your changes first.".to_string()
                )
            } else {
                DomainError::Internal(e.to_string())
            }
        })?;
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p rocket-git switch_branch_refuses_when_dirty -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Verify existing branch tests still pass**

```bash
cargo test -p rocket-git branch -- --nocapture
```

Expected: all branch-related tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "fix(git): switch_branch refuses when working tree has uncommitted changes"
```

---

## Task 2 — B2: `merge_branch` conflict path must write index and return `DomainError::Conflict`

**Spec ref:** B2 — `merge_branch` returns wrong error variant and never writes the index, leaving the repo in a broken state.

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs:983–987`

### Background
When `merge_branch` detects conflicts, it currently:
1. Returns `DomainError::Internal("merge resulted in conflicts")` — wrong variant.
2. Never calls `index.write()` — so the conflicted index is never persisted to `.git/index`.

The result: the repo has a MERGE_HEAD but `git_conflicts` returns `[]` because the index is clean on disk. The fix mirrors what `pull` does correctly at lines 686–716: write the index first, collect conflicted paths, return `DomainError::Conflict`.

- [ ] **Step 1: Write the failing test**

Add after the `merge_branch_fast_forward` test in `crates/rocket-git/src/git2_service.rs`:

```rust
#[test]
fn merge_branch_with_conflicts_returns_conflict_error_and_writes_index() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();

    // Create 'feature' branch and commit a change to test.bru.
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
    repo.commit(Some("HEAD"), &sig, &sig, "feature commit", &tree, &[&head]).unwrap();

    // Switch back to main and make a conflicting change to the same file.
    repo.set_head("refs/heads/main").unwrap();
    repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
    std::fs::write(dir.path().join("test.bru"), "main version").unwrap();
    let mut idx2 = repo.index().unwrap();
    idx2.add_path(std::path::Path::new("test.bru")).unwrap();
    idx2.write().unwrap();
    let tree_id2 = idx2.write_tree().unwrap();
    let tree2 = repo.find_tree(tree_id2).unwrap();
    let head2 = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "main conflicting commit", &tree2, &[&head2]).unwrap();

    // Now try to merge 'feature' into main — must conflict.
    let result = svc.merge_branch(&path, "feature");
    assert!(
        matches!(result, Err(rocket_shared::error::DomainError::Conflict(_))),
        "expected Conflict error, got: {:?}", result
    );

    // Conflicts must be readable after the call (index was written).
    let conflicts = svc.conflicts(&path).unwrap();
    assert!(!conflicts.is_empty(), "expected at least one conflict file in index");
    assert!(conflicts.iter().any(|c| c.path == "test.bru"));
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rocket-git merge_branch_with_conflicts -- --nocapture
```

Expected: FAIL — `result` is `Err(Internal(...))` and conflicts list is empty.

- [ ] **Step 3: Fix `merge_branch` conflict path in `git2_service.rs`**

Replace lines 983–987 (the `if index.has_conflicts()` block inside `merge_branch`):

```rust
if index.has_conflicts() {
    // Write the conflicted index so git_conflicts() can enumerate the files.
    index
        .write()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let conflicted: Vec<String> = index
        .conflicts()
        .map(|iter| {
            iter.flatten()
                .filter_map(|c| {
                    c.our
                        .or(c.their)
                        .or(c.ancestor)
                        .and_then(|e| String::from_utf8(e.path).ok())
                })
                .collect()
        })
        .unwrap_or_default();
    let file_list = if conflicted.is_empty() {
        "unknown files".to_string()
    } else {
        conflicted.join(", ")
    };
    return Err(DomainError::Conflict(format!(
        "merge conflict: resolve conflicts in {file_list} and commit to complete the merge"
    )));
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p rocket-git merge_branch_with_conflicts -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Run full test suite**

```bash
cargo test -p rocket-git -- --nocapture
```

Expected: all tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "fix(git): merge_branch writes index and returns Conflict error on merge conflicts"
```

---

## Task 3 — B3: `ConflictResolver` must surface resolution errors to the user

**Spec ref:** B3 — `ConflictResolver` silently swallows resolution errors.

**Files:**
- Modify: `src/components/git/ConflictResolver.tsx`

### Background
`handleResolve` has a bare `catch { // Handle silently. }`. If `gitResolveConflict` fails, the user sees nothing. The fix: replace the direct `gitResolveConflict` call with `store.resolveConflict` (which sets `state.error`), then read and display the error inline. Also applies to the manual-edit "Save Resolution" path.

- [ ] **Step 1: Replace the `handleResolve` implementation and add error display**

Replace the entire `ConflictResolver.tsx` file content:

```tsx
import '@/components/editor/monaco-setup';
import Editor from '@monaco-editor/react';
import { AlertCircle } from 'lucide-react';
import { useState } from 'react';
import { useMonacoTheme } from '@/components/editor/useMonacoTheme';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';
import type { ConflictState } from '@/types/pane-types';

interface ConflictResolverProps {
  conflictState: ConflictState;
}

export function ConflictResolver({ conflictState }: ConflictResolverProps) {
  const [manualMode, setManualMode] = useState(false);
  const [manualContent, setManualContent] = useState(conflictState.ours);
  const { resolveConflict, abortMerge, error, clearError } = useGitStore();
  const { themeName } = useMonacoTheme();

  const handleAbort = async () => {
    await abortMerge();
  };

  const handleResolve = async (resolution: 'ours' | 'theirs' | 'custom', content?: string) => {
    const res =
      resolution === 'custom'
        ? { resolution: 'custom' as const, content: content ?? '' }
        : { resolution };
    await resolveConflict(conflictState.filePath, res);
  };

  if (manualMode) {
    return (
      <div className='flex flex-col h-full'>
        <div className='flex items-center gap-2 border-b px-3 py-1.5'>
          <Badge variant='destructive' className='text-[9px]'>
            Conflict
          </Badge>
          <span className='font-mono text-sm truncate'>{conflictState.filePath}</span>
          <div className='ml-auto flex gap-1'>
            <Button
              variant='outline'
              size='sm'
              className='h-6 text-sm text-destructive'
              onClick={handleAbort}
            >
              Abort Merge
            </Button>
            <Button
              variant='outline'
              size='sm'
              className='h-6 text-sm'
              onClick={() => setManualMode(false)}
            >
              Back
            </Button>
            <Button
              size='sm'
              className='h-6 text-sm'
              onClick={() => handleResolve('custom', manualContent)}
            >
              Save Resolution
            </Button>
          </div>
        </div>
        {error && (
          <div className='flex items-start gap-2 mx-3 mt-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'>
            <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
            <span className='flex-1 wrap-break-word'>{error}</span>
            <button type='button' className='shrink-0 hover:opacity-70 leading-none' onClick={clearError} aria-label='Dismiss error'>×</button>
          </div>
        )}
        <div className='flex-1'>
          <Editor
            value={manualContent}
            onChange={(v) => setManualContent(v ?? '')}
            theme={themeName}
            options={{ minimap: { enabled: false }, fontSize: 12, scrollBeyondLastLine: false }}
          />
        </div>
      </div>
    );
  }

  return (
    <div className='flex flex-col h-full'>
      <div className='flex items-center gap-2 border-b px-3 py-1.5'>
        <Badge variant='destructive' className='text-[9px]'>
          Conflict
        </Badge>
        <span className='font-mono text-sm truncate'>{conflictState.filePath}</span>
        <div className='ml-auto'>
          <Button
            variant='outline'
            size='sm'
            className='h-6 text-sm text-destructive'
            onClick={handleAbort}
          >
            Abort Merge
          </Button>
        </div>
      </div>
      {error && (
        <div className='flex items-start gap-2 mx-3 mt-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'>
          <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
          <span className='flex-1 wrap-break-word'>{error}</span>
          <button type='button' className='shrink-0 hover:opacity-70 leading-none' onClick={clearError} aria-label='Dismiss error'>×</button>
        </div>
      )}
      <div className='flex flex-1 min-h-0'>
        <div className='flex-1 flex flex-col border-r'>
          <div className='px-2 py-1 text-sm font-medium text-muted-foreground border-b'>Ours</div>
          <div className='flex-1'>
            <Editor
              value={conflictState.ours}
              theme={themeName}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
              }}
            />
          </div>
        </div>
        <div className='flex-1 flex flex-col'>
          <div className='px-2 py-1 text-sm font-medium text-muted-foreground border-b'>Theirs</div>
          <div className='flex-1'>
            <Editor
              value={conflictState.theirs}
              theme={themeName}
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 12,
                scrollBeyondLastLine: false,
              }}
            />
          </div>
        </div>
      </div>
      <div className='flex items-center gap-2 border-t px-3 py-2'>
        <Button variant='outline' size='sm' onClick={() => handleResolve('ours')}>
          Accept Ours
        </Button>
        <Button variant='outline' size='sm' onClick={() => handleResolve('theirs')}>
          Accept Theirs
        </Button>
        <Button variant='secondary' size='sm' onClick={() => setManualMode(true)}>
          Edit Manually
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/ConflictResolver.tsx
git commit -m "fix(git): surface conflict resolution errors in ConflictResolver instead of swallowing them"
```

---

## Task 4 — B4: Fix stale Zustand closure in `handleConflictClick`

**Spec ref:** B4 — `handleConflictClick` reads stale `conflicts` from render-time closure instead of fresh store state.

**Files:**
- Modify: `src/components/git/GitFileList.tsx:45–53`

### Background
`conflicts` is destructured from `useGitStore()` at render time. After `await refreshConflicts()`, the store's `conflicts` array is updated but the local binding still points to the old array. Fix: read from `useGitStore.getState()` after the await.

- [ ] **Step 1: Fix the stale closure in `handleConflictClick`**

In `src/components/git/GitFileList.tsx`, replace lines 45–53:

```tsx
const handleConflictClick = async (file: FileStatus) => {
  await refreshConflicts();
  // Read fresh state — the `conflicts` binding captured at render-time is stale after the await.
  const conflictFile = useGitStore.getState().conflicts.find((c) => c.path === file.path);
  if (conflictFile) {
    onConflictClick(conflictFile);
  } else {
    await refreshStatus();
  }
};
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitFileList.tsx
git commit -m "fix(git): read fresh conflicts from store after refreshConflicts in handleConflictClick"
```

---

## Task 5 — B5: `commitChanges` must refresh commit log after a commit

**Spec ref:** B5 — The Commits view shows stale history after a commit because `commitChanges` only calls `refreshStatus`, not `refreshLog`.

**Files:**
- Modify: `src/stores/git-store.ts:276–285`

- [ ] **Step 1: Add `refreshLog` call in `commitChanges`**

In `src/stores/git-store.ts`, replace the `commitChanges` action (lines 276–285):

```ts
commitChanges: async (message: string) => {
  const { collectionPath } = get();
  if (!collectionPath) return;
  try {
    await gitCommit(collectionPath, message);
    await get().refreshStatus();
    await get().refreshLog();
  } catch (e) {
    set({ error: String(e) });
  }
},
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/stores/git-store.ts
git commit -m "fix(git): refresh commit log after commitChanges so Commits view stays current"
```

---

## Task 6 — B6: Populate `files_changed` in `commit()` and `log()`

**Spec ref:** B6 — `files_changed` is always `0` in `CommitInfo` returned by `commit()` and `log()`.

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs` — `commit()` (lines 509–546) and `log()` (lines 548–574)

### Background
For stashes, `files_changed` is computed via `diff_tree_to_tree` comparing the stash commit to its parent. The same approach works for commits. For the initial commit (no parent), diff against an empty tree using `repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap())`.

- [ ] **Step 1: Write the failing tests**

Add two tests to the `#[cfg(test)]` block:

```rust
#[test]
fn commit_returns_files_changed_count() {
    let (dir, path) = setup_repo();
    let svc = Git2Service::new();

    // Stage a new file (second commit, so has a parent).
    std::fs::write(dir.path().join("new.bru"), "new request").unwrap();
    let repo = git2::Repository::open(&path).unwrap();
    let mut idx = repo.index().unwrap();
    idx.add_path(std::path::Path::new("new.bru")).unwrap();
    idx.write().unwrap();

    let info = svc.commit(&path, "add new.bru").unwrap();
    assert_eq!(info.files_changed, 1, "expected 1 file changed, got {}", info.files_changed);
}

#[test]
fn log_returns_files_changed_count() {
    let (_dir, path) = setup_repo();
    let svc = Git2Service::new();
    let log = svc.log(&path, 10).unwrap();
    // The initial commit in setup_repo() adds test.bru — files_changed should be 1.
    assert!(!log.is_empty());
    assert_eq!(log[0].files_changed, 1, "expected 1 file in initial commit, got {}", log[0].files_changed);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-git commit_returns_files_changed_count log_returns_files_changed_count -- --nocapture
```

Expected: FAIL — both assertions report `files_changed == 0`.

- [ ] **Step 3: Add `count_commit_files` helper and fix `commit()`**

Add a private helper function after `build_simple_diff` (around line 170):

```rust
/// Count the number of files changed in a commit relative to its first parent.
/// For the initial commit (no parent), diffs against an empty tree.
fn count_commit_files(repo: &Repository, commit: &git2::Commit) -> usize {
    let new_tree = match commit.tree() {
        Ok(t) => t,
        Err(_) => return 0,
    };
    let old_tree: Option<git2::Tree> = commit
        .parent(0)
        .ok()
        .and_then(|p| p.tree().ok());

    repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)
        .ok()
        .and_then(|d| d.stats().ok())
        .map(|s| s.files_changed())
        .unwrap_or(0)
}
```

Then in `commit()`, replace `files_changed: 0` with the computed value. The full `commit()` return block (lines 537–545) becomes:

```rust
let commit_obj = repo.find_commit(oid)
    .map_err(|e| DomainError::Internal(e.to_string()))?;
let files_changed = count_commit_files(&repo, &commit_obj);

Ok(CommitInfo {
    id: oid.to_string()[..7].to_string(),
    full_id: oid.to_string(),
    message: message.to_string(),
    author: sig.name().unwrap_or("").to_string(),
    author_email: sig.email().unwrap_or("").to_string(),
    timestamp: chrono::Utc::now(),
    files_changed,
})
```

- [ ] **Step 4: Fix `log()` to populate `files_changed`**

In `log()`, replace the `commits.push(CommitInfo { ... })` block (lines 563–572) with:

```rust
let files_changed = count_commit_files(&repo, &commit);
commits.push(CommitInfo {
    id: oid.to_string()[..7].to_string(),
    full_id: oid.to_string(),
    message: commit.message().unwrap_or("").to_string(),
    author: commit.author().name().unwrap_or("").to_string(),
    author_email: commit.author().email().unwrap_or("").to_string(),
    timestamp,
    files_changed,
});
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p rocket-git commit_returns_files_changed_count log_returns_files_changed_count -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Run full test suite**

```bash
cargo test -p rocket-git -- --nocapture
```

Expected: all tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "fix(git): populate files_changed in commit() and log() via diff_tree_to_tree"
```

---

## Task 7 — B7: `push` must use the configured upstream refspec instead of hardcoding same-name

**Spec ref:** B7 — `push` hardcodes `refs/heads/<branch>:refs/heads/<branch>`, which fails when local and remote branch names differ.

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs:577–599` (`push()`)

### Background
The fix: before building the refspec, look up whether the local branch has a configured upstream. If it does, use the remote branch name from the upstream as the right-hand side of the refspec. If not (no upstream configured), fall back to the current same-name behaviour.

- [ ] **Step 1: Write the failing test**

This test requires a bare repo as a fake remote. Add to the `#[cfg(test)]` block:

```rust
fn setup_repo_with_remote() -> (TempDir, String, TempDir, String) {
    // Create a bare "remote" repo.
    let remote_dir = TempDir::new().unwrap();
    let remote_path = remote_dir.path().to_string_lossy().to_string();
    git2::Repository::init_bare(&remote_path).unwrap();

    // Clone it locally.
    let local_dir = TempDir::new().unwrap();
    let local_path = local_dir.path().to_string_lossy().to_string();
    // Use git2 directly for the clone — local file-path remotes need no credentials.
    git2::build::RepoBuilder::new()
        .clone(&remote_path, local_dir.path())
        .expect("clone failed");

    // Make an initial commit so the repo is non-empty.
    let repo = git2::Repository::open(&local_path).unwrap();
    let sig = git2::Signature::now("T", "t@t.com").unwrap();
    std::fs::write(local_dir.path().join("a.bru"), "content").unwrap();
    let mut idx = repo.index().unwrap();
    idx.add_path(std::path::Path::new("a.bru")).unwrap();
    idx.write().unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[]).unwrap();
    repo.set_head("refs/heads/main").unwrap();

    (local_dir, local_path, remote_dir, remote_path)
}

#[test]
fn push_uses_upstream_refspec_when_local_remote_names_match() {
    let (local_dir, local_path, _remote_dir, remote_path) = setup_repo_with_remote();
    let svc = Git2Service::new();

    // Local file-path remotes don't invoke the credential callback.
    // SshAgent is a safe no-op placeholder for the required parameter.
    let result = svc.push(&local_path, "origin", &crate::credentials::GitCredentials::SshAgent);
    // For a local bare remote, push should succeed.
    assert!(result.is_ok(), "push failed: {:?}", result);

    // Verify the ref landed in the bare remote.
    let bare = git2::Repository::open_bare(&remote_path).unwrap();
    assert!(bare.find_reference("refs/heads/main").is_ok());

    drop(local_dir);
}
```

- [ ] **Step 2: Run test to verify it passes as-is (baseline)**

```bash
cargo test -p rocket-git push_uses_upstream_refspec -- --nocapture
```

Expected: PASS (existing behaviour works for same-name case). This establishes a baseline.

- [ ] **Step 3: Fix `push()` to use upstream refspec when configured**

Replace the refspec-building block in `push()` (lines 585–590):

```rust
let branch_name_str = head.shorthand().unwrap_or("main");

// Prefer the configured upstream's remote branch name as the push target.
// Falls back to same-name if no upstream is configured.
let remote_branch = repo
    .find_branch(branch_name_str, git2::BranchType::Local)
    .ok()
    .and_then(|b| b.upstream().ok())
    .and_then(|u| {
        u.name().ok().flatten().map(|full| {
            // upstream name is "origin/feat-x" — strip the "origin/" prefix.
            full.splitn(2, '/').nth(1).map(String::from)
        })
    })
    .flatten()
    .unwrap_or_else(|| branch_name_str.to_string());

let refspec = format!("refs/heads/{branch_name_str}:refs/heads/{remote_branch}");
```

- [ ] **Step 4: Run test and full suite**

```bash
cargo test -p rocket-git push_uses_upstream_refspec -- --nocapture
cargo test -p rocket-git -- --nocapture
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "fix(git): push uses configured upstream refspec instead of hardcoded same-name"
```

---

## Task 8 — B8: `handleStashAndPull` must not pop stash when pull produces conflicts

**Spec ref:** B8 — `handleStashAndPull` calls `popStash(0)` even when pull left the repo in a conflicted state.

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx:79–91`

### Background
After `await pull()`, the store's status is refreshed (done inside `store.pull`). So after the await returns, `useGitStore.getState().hasConflicts()` reflects the real repo state. If conflicts exist, skip the pop and show an informational banner instead.

- [ ] **Step 1: Fix `handleStashAndPull` in `GitLandingPanel.tsx`**

Replace lines 79–91 (the `handleStashAndPull` function):

```tsx
const handleStashAndPull = async () => {
  setShowStashDialog(false);
  setPulling(true);
  try {
    await saveStash('Auto-stash before pull');
    await pull();
    // After pull, check whether it produced merge conflicts.
    // If so, do NOT restore the stash — applying it on top of a conflicted
    // index would corrupt the working tree with doubled conflicts.
    if (useGitStore.getState().hasConflicts()) {
      // Leave the stash in place; the user can pop it after resolving conflicts.
      return;
    }
    await popStash(0);
    setLastFetched(new Date().toLocaleTimeString());
  } catch {
    // If pop fails (e.g. stash itself conflicts), stash is preserved for manual resolution.
  } finally {
    setPulling(false);
  }
};
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Lint check**

```bash
yarn check
```

Expected: no lint errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "fix(git): skip stash pop when pull produces merge conflicts in stash-and-pull flow"
```

---

## Final Verification

- [ ] **Run full Rust test suite**

```bash
cargo test -p rocket-git -- --nocapture
```

Expected: all tests PASS.

- [ ] **Run TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Run Biome lint**

```bash
yarn check
```

Expected: clean.
