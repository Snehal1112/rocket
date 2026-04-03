# Git Test Suite — Full Contract Coverage

**Date:** 2026-04-04
**Status:** Approved
**Scope:** `crates/rocket-git/src/git2_service.rs` · `src/stores/__tests__/git-store.test.ts`

## Overview

Expand the existing git test files to achieve full contract coverage for every method on `GitService` (Rust) and every action on `useGitStore` (TypeScript). Work is split into two independent phases:

- **Phase 1 — Rust:** append new test functions to the existing `#[cfg(test)]` module in `git2_service.rs`
- **Phase 2 — Frontend:** add new `describe` blocks to the existing `git-store.test.ts`

Both phases target the same goal: happy path + failure path for every method, with no mocking of the system under test.

---

## Affected Files

| File | Change |
|------|--------|
| `crates/rocket-git/src/git2_service.rs` | Append ~20 new `#[test]` functions to the existing test module |
| `src/stores/__tests__/git-store.test.ts` | Add new `describe` blocks for all store action groups; update `vi.mock` factory |

---

## Section 1: Rust Test Gaps

### Currently covered (33 tests)

| Method | Test(s) |
|--------|---------|
| `is_repo` | `is_repo_true`, `is_repo_false` |
| `status` | `status_modified_file`, `status_untracked_file`, `status_ahead_behind_with_remote` |
| `diff_file` | `diff_file_shows_changes` |
| `stage / unstage` | `stage_and_unstage_file`, `stage_deleted_file` |
| `discard` | `discard_reverts_changes` |
| `commit / log` | `commit_and_log`, `log_respects_limit`, `commit_creates_merge_commit_when_merge_in_progress` |
| `create/switch/delete_branch` | `branch_create_switch_delete` |
| `merge_branch` | `merge_branch_fast_forward` |
| `stash_save / stash_pop` | `stash_save_and_pop` |
| `stash_apply` | `stash_apply_keeps_stash` |
| `list_remotes / add_remote` | `list_remotes_empty_for_fresh_repo`, `add_and_list_remote`, `add_multiple_remotes` |
| `remove_remote` | `remove_remote`, `remove_nonexistent_remote_fails`, `remove_and_readd_remote_leaves_stale_tracking_refs` |
| `set_remote_url` | `set_remote_url` |
| `add_remote` duplicate | `add_duplicate_remote_fails` |
| `pull` | `pull_fast_forward_updates_status`, `pull_fast_forward_without_prior_fetch`, `pull_with_diverged_history_merges_and_clears_behind` |
| `checkout_remote_branch` | `checkout_remote_branch_creates_local_tracking` |
| `abort_merge` | `abort_merge_resets_to_head` |

### Gaps → new tests to add

#### Group: `init_clone`

**`init_creates_git_repo`**
- Call `svc.init(&path)` on a fresh empty TempDir
- Assert `svc.is_repo(&path)` returns true
- Assert `svc.status(&path)` succeeds

**`init_on_existing_repo_succeeds`**
- Call `init` on a path that is already a git repo (idempotent)
- Must return `Ok(())`

**`clone_fails_on_invalid_url`**
- Call `svc.clone_repo("not-a-url", dest_path, &creds)`
- Assert `result.is_err()`

#### Group: `diff`

**`diff_staged_shows_staged_changes`**
- Modify `test.bru`, stage it
- Call `svc.diff_staged(&path, "test.bru")`
- Assert `old_content != new_content` and path is correct

**`diff_file_clean_returns_empty_hunks`**
- On an unmodified file, call `svc.diff_file(&path, "test.bru")`
- Assert `diff.hunks.is_empty()`

#### Group: `push`

**`push_advances_remote_head`**
- Create local repo, commit, push to local bare remote
- Make one more local commit
- Call `svc.push(&path, "origin", &creds)`
- Verify from the bare remote that its HEAD OID matches the new local HEAD

**`push_fails_with_non_fast_forward`**
- Two clones of the same remote
- Clone A pushes a commit
- Clone B (without pulling) pushes a commit on the same branch
- Assert clone B's push returns `Err`

#### Group: `stash_drop`

**`stash_drop_removes_entry_at_index`**
- Stash one change
- Call `svc.stash_drop(&path, 0)`
- Assert `svc.stash_list(&path).unwrap().is_empty()`

**`stash_drop_out_of_range_fails`**
- Call `svc.stash_drop(&path, 99)` on a repo with no stashes
- Assert `result.is_err()`

#### Group: `conflict`

**`conflicts_listed_after_merge_conflict`**
- Set up same-file conflict (same pattern as `abort_merge_resets_to_head`)
- Start merge, do NOT abort
- Call `svc.conflicts(&path)`
- Assert at least one `ConflictFile` is returned

**`resolve_conflict_ours_writes_local_content`**
- Set up conflict, call `svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Ours)`
- Read file content — must equal the local (ours) branch content

**`resolve_conflict_theirs_writes_remote_content`**
- Same setup, use `ConflictResolution::Theirs`
- File content must equal the incoming (theirs) branch content

#### Group: `delete_branch`

**`delete_checked_out_branch_fails`**
- Create and check out `feature-x`
- Attempt `svc.delete_branch(&path, "feature-x")` while on that branch
- Assert `result.is_err()`

---

## Section 2: Frontend Test Gaps

### Currently covered (5 tests in `describe('git-store clearError')`)

- `clearError sets error to null`
- `push clears stale error before executing`
- `pull clears stale error before executing`
- `fetch clears stale error before executing`
- `push sets error when operation fails`

### `vi.mock` factory update

The existing mock factory is missing several API functions used by the new tests. Add these to the factory:

```ts
gitStashList: vi.fn().mockResolvedValue([]),
gitStashSave: vi.fn().mockResolvedValue(undefined),
gitStashPop: vi.fn().mockResolvedValue(undefined),
gitStashApply: vi.fn().mockResolvedValue(undefined),
gitStashDrop: vi.fn().mockResolvedValue(undefined),
gitStage: vi.fn().mockResolvedValue(undefined),
gitUnstage: vi.fn().mockResolvedValue(undefined),
gitDiscard: vi.fn().mockResolvedValue(undefined),
gitCommit: vi.fn().mockResolvedValue({ id: 'abc1234', message: 'test commit' }),
gitLog: vi.fn().mockResolvedValue([]),
gitSwitchBranch: vi.fn().mockResolvedValue(undefined),
gitCreateBranch: vi.fn().mockResolvedValue(undefined),
gitDeleteBranch: vi.fn().mockResolvedValue(undefined),
gitMergeBranch: vi.fn().mockResolvedValue(undefined),
gitConflicts: vi.fn().mockResolvedValue([]),
gitResolveConflict: vi.fn().mockResolvedValue(undefined),
gitAbortMerge: vi.fn().mockResolvedValue(undefined),
gitListRemotes: vi.fn().mockResolvedValue([]),
gitAddRemote: vi.fn().mockResolvedValue(undefined),
gitRemoveRemote: vi.fn().mockResolvedValue(undefined),
gitSetRemoteUrl: vi.fn().mockResolvedValue(undefined),
```

Note: rename `gitRemotes` and `gitStashes` (currently mocked incorrectly) to `gitListRemotes` and `gitStashList` respectively, to match the actual function names exported from `tauri-api.ts`.

### `beforeEach` convention

Each `describe` block uses the same `beforeEach`:

```ts
beforeEach(() => {
  useGitStore.setState({
    collectionPath: '/test/repo',
    isRepo: true,
    error: null,
    credentials: null,
    remotes: [],
    pendingNetworkOp: null,
    status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
    branches: { local: [], remote: [] },
    stashes: [],
    conflicts: [],
  });
  vi.clearAllMocks();
});
```

### New `describe` blocks

#### `describe('setCollection')`

**`non-repo path sets isRepo=false and status=null`**
- Mock `gitIsRepo` to return `false`
- Call `setCollection('/not/a/repo')`
- Assert `isRepo === false`, `status === null`

**`valid repo path loads status, branches, remotes, and stashes`**
- Mock `gitIsRepo` to return `true`
- Call `setCollection('/test/repo')`
- Assert `isRepo === true`, `gitStatus` called, `gitBranches` called, `gitListRemotes` called, `gitStashList` called

**`gitIsRepo throwing sets error state`**
- Mock `gitIsRepo` to reject with `new Error('disk error')`
- Call `setCollection('/test/repo')`
- Assert `error` contains `'disk error'`

---

#### `describe('pendingNetworkOp and setCredentials')`

**`pull without credentials opens dialog and sets pendingNetworkOp=pull`**
- Store has `credentials: null`
- Call `pull()`
- Assert `showCredentialsDialog === true`, `pendingNetworkOp === 'pull'`

**`setCredentials auto-retries pull and clears pendingNetworkOp`**
- Store has `pendingNetworkOp: 'pull'`, no credentials
- Call `setCredentials({ type: 'sshAgent' })`
- Assert `gitPull` was called, `pendingNetworkOp === null`, `showCredentialsDialog === false`

**`push without credentials sets pendingNetworkOp=push`**
- Same pattern as pull, but for `push()`

**`fetch without credentials sets pendingNetworkOp=fetch`**
- Same pattern, for `fetch()`

**`dismissing dialog (setShowCredentialsDialog false) clears pendingNetworkOp`**
- Set `pendingNetworkOp: 'push'`
- Call `setShowCredentialsDialog(false)`
- Assert `pendingNetworkOp === null`, `gitPush` was NOT called

**`reset clears pendingNetworkOp`**
- Set `pendingNetworkOp: 'fetch'`
- Call `reset()`
- Assert `pendingNetworkOp === null`

---

#### `describe('staging')`

**`stageFiles calls gitStage and refreshes status`**
- Call `stageFiles(['foo.bru'])`
- Assert `gitStage` called with `('/test/repo', ['foo.bru'])`
- Assert `gitStatus` called (refresh)

**`unstageFiles calls gitUnstage and refreshes status`**
- Call `unstageFiles(['foo.bru'])`
- Assert `gitUnstage` called, `gitStatus` called

**`stageAll stages only unstaged non-unchanged files`**
- Set `status.files` with a mix of staged, unstaged-modified, and unchanged files
- Call `stageAll()`
- Assert `gitStage` called with only the unstaged modified paths (not unchanged, not already staged)

**`discardFiles calls gitDiscard and refreshes status`**
- Call `discardFiles(['foo.bru'])`
- Assert `gitDiscard` called, `gitStatus` called

**`commitChanges calls gitCommit and refreshes status and log`**
- Call `commitChanges('initial commit')`
- Assert `gitCommit` called with `('/test/repo', 'initial commit')`
- Assert `gitStatus` called

**`stageFiles sets error on failure`**
- Mock `gitStage` to reject
- Call `stageFiles(['foo.bru'])`
- Assert `error` is non-null

---

#### `describe('branches')`

**`switchBranch calls api and refreshes status and branches`**
- Call `switchBranch('feature')`
- Assert `gitSwitchBranch` called with `('/test/repo', 'feature')`
- Assert `gitStatus` and `gitBranches` called

**`createBranch calls api and refreshes branches`**
- Call `createBranch('new-branch')`
- Assert `gitCreateBranch` called, `gitBranches` called

**`deleteBranch calls api and refreshes branches`**
- Call `deleteBranch('old-branch')`
- Assert `gitDeleteBranch` called, `gitBranches` called

**`mergeBranch calls api and refreshes status and branches`**
- Call `mergeBranch('feature')`
- Assert `gitMergeBranch` called, `gitStatus` called, `gitBranches` called

**`switchBranch sets error on failure`**
- Mock `gitSwitchBranch` to reject
- Assert `error` is non-null

---

#### `describe('stash')`

**`saveStash calls api and refreshes status and stashes`**
- Call `saveStash('WIP')`
- Assert `gitStashSave` called, `gitStatus` called, `gitStashList` called

**`popStash calls api and refreshes status and stashes`**
- Call `popStash(0)`
- Assert `gitStashPop` called with `('/test/repo', 0)`, `gitStatus` called, `gitStashList` called

**`applyStash calls api and refreshes stashes`**
- Call `applyStash(0)`
- Assert `gitStashApply` called, `gitStatus` called, `gitStashList` called

**`dropStash calls api and refreshes stashes`**
- Call `dropStash(0)`
- Assert `gitStashDrop` called with `('/test/repo', 0)`, `gitStashList` called
- `gitStatus` must NOT be called (dropStash doesn't touch working tree)

---

#### `describe('remotes')`

**`addRemote calls api and refreshes remotes`**
- Call `addRemote('upstream', 'https://github.com/org/repo.git')`
- Assert `gitAddRemote` called with correct args, `gitListRemotes` called

**`removeRemote calls api and refreshes remotes`**
- Call `removeRemote('upstream')`
- Assert `gitRemoveRemote` called, `gitListRemotes` called

**`setRemoteUrl calls api and refreshes remotes`**
- Call `setRemoteUrl('origin', 'https://github.com/org/new.git')`
- Assert `gitSetRemoteUrl` called, `gitListRemotes` called

**`addRemote sets error on failure`**
- Mock `gitAddRemote` to reject
- Assert `error` is non-null

---

#### `describe('conflicts')`

**`resolveConflict calls api and refreshes status and conflicts`**
- Call `resolveConflict('foo.bru', 'ours')`
- Assert `gitResolveConflict` called with `('/test/repo', 'foo.bru', 'ours')`
- Assert `gitStatus` called, `gitConflicts` called

**`abortMerge calls api and refreshes status and conflicts`**
- Call `abortMerge()`
- Assert `gitAbortMerge` called, `gitStatus` called, `gitConflicts` called

---

#### `describe('reset')`

**`reset clears all state to initial values`**
- Seed store with arbitrary non-default values for all fields
- Call `reset()`
- Assert: `isRepo === false`, `collectionPath === null`, `status === null`, `conflicts === []`, `stashes === []`, `branches === null`, `remotes === []`, `commitLog === []`, `error === null`, `credentials === null`, `showCredentialsDialog === false`, `pendingNetworkOp === null`

---

## Section 3: Conventions

### Rust

- All tests use the existing `setup_repo()` helper (creates a TempDir + bare repo with one initial commit on `main`)
- Tests that need a network target (push/pull) construct a local bare `Repository::init_bare(TempDir)` — no actual network
- Failure-path tests use `.unwrap_err()` or `assert!(result.is_err())`
- Naming: `<group>_<scenario>` (e.g. `stash_drop_out_of_range_fails`)
- Each test is standalone: no shared state between tests

### Frontend

- All new `describe` blocks share the same `beforeEach` pattern: `useGitStore.setState(defaults) + vi.clearAllMocks()`
- Per-test overrides: `vi.mocked(gitX).mockResolvedValueOnce(...)` or `mockRejectedValueOnce(...)`
- Test names are plain English, read as "it [name]"
- The `vi.mock` factory mock at the top of the file provides safe defaults for all functions; no test imports mocks at the top level — they import inside the test using `const { gitX } = await import('@/lib/tauri-api')`

### Error contract

- Rust: all failure paths return `DomainError::Internal(msg)` or `DomainError::NotFound(msg)` — tests assert `result.is_err()` and may inspect the message
- Frontend: store catches all thrown errors and sets `state.error = String(e)` — tests assert `error` is non-null and optionally check the message substring

### What we are NOT testing

- Rust `fetch` directly (it is exercised as a precondition in pull/push tests; a dedicated test would require a real network call that is out of scope)
- Frontend `checkoutRemoteBranch` (same contract as `switchBranch`; covered by pattern, not worth a separate describe block)
- `unstageAll` (delegates entirely to `unstageFiles`; the latter is tested)
- `refreshStatus`, `refreshBranches`, etc. in isolation (they are tested as side effects of the mutating actions)
