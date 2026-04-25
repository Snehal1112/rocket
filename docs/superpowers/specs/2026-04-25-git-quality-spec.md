# Git Implementation — Code Quality & Maintainability

**Date:** 2026-04-25  
**Scope:** `crates/rocket-git/src/git2_service.rs`, `src/stores/git-store.ts`, `src/components/git/`  
**Purpose:** Reduce maintenance burden, improve test coverage for critical paths, and clarify inconsistent patterns identified during the audit.

---

## Quality Issue Inventory

### Q1 — `git2_service.rs` is a single 1,300+ line file; conflicts module is especially tangled

**Current state:** All 35 method implementations live in one file. The conflict resolution (`resolve_conflict`, lines 1201–1296) iterates the entire conflict list twice — once for `Ours` and once for `Theirs` — with duplicated index-opening boilerplate. The diff helpers (`build_simple_diff`, `get_head_content`, `get_index_content`) and the credential helpers (`build_callbacks`) are free functions mixed into the same module.

**Impact:** New contributors have to scan 1,300 lines to understand any one feature. The duplication in `resolve_conflict` is an error-waiting-to-happen (if the `Ours` loop is fixed, the `Theirs` loop may not be).

**Fix:** Extract into sub-modules within the crate (no new crates needed):

```
git2_service/
  mod.rs          ← Git2Service struct + impl GitService (delegates to sub-modules)
  credentials.rs  ← build_callbacks()
  diff.rs         ← build_simple_diff(), get_head_content(), get_index_content()
  conflict.rs     ← resolve_conflict(), abort_merge(), conflicts()
  stash.rs        ← stash_list(), stash_save(), stash_pop(), stash_apply(), stash_drop()
  branch.rs       ← branches(), switch_branch(), checkout_remote_branch(), create_branch(), delete_branch(), merge_branch()
  remote.rs       ← list_remotes(), add_remote(), remove_remote(), set_remote_url(), fetch(), push(), pull()
```

Each sub-module is `pub(super)` — the outer `mod.rs` re-exports nothing, keeping the public API unchanged. The trait boundary (`GitService`) remains in `service.rs` unchanged.

Also, **refactor `resolve_conflict`** to eliminate the duplicated loop: extract a helper `fn find_conflict_entry(index: &Index, file: &str) -> Option<IndexConflict>` that is reused by both `Ours` and `Theirs` branches.

---

### Q2 — No integration tests for push, pull, fetch, and conflict resolution

**Current state (from `git2_service.rs` test section):**

Tested: `is_repo`, `status`, `diff`, `stage`/`unstage`/`discard`, `commit`, `log`, `branch` ops, `stash` ops.

NOT tested: `push`, `pull`, `fetch`, `clone`, `resolve_conflict`, `abort_merge`, `merge_branch` (conflict path).

**Impact:** The most dangerous operations — those that modify remote state or handle merge conflicts — have zero automated coverage. Bugs B2, B7, and B8 (identified in the bugs spec) would have been caught by tests.

**Fix:** Add integration tests using `git2` to create bare repos as fake remotes (no network required):

```rust
fn setup_repo_with_remote() -> (TempDir, TempDir) {
    // Creates a bare "remote" and a local clone with one commit.
}

#[test]
fn push_and_pull_roundtrip() { ... }

#[test]
fn pull_fast_forward_updates_branch() { ... }

#[test]
fn pull_with_conflicts_returns_conflict_error_and_writes_index() { ... }

#[test]
fn resolve_conflict_ours_stages_local_version() { ... }

#[test]
fn resolve_conflict_theirs_stages_remote_version() { ... }

#[test]
fn abort_merge_resets_to_head() { ... }

#[test]
fn merge_branch_with_conflict_returns_conflict_error() { ... }  // covers B2
```

These tests must not use `wiremock` or real network calls — a bare `git2::Repository::init_bare()` acts as the "remote".

---

### Q3 — Inconsistent invocation pattern in frontend: some components call Tauri directly, others go through the store

**Current state:**

- `ConflictResolver.tsx:31` — calls `gitResolveConflict(...)` directly (bypasses store)
- `GitPanel.tsx:156` — calls `gitInit(...)` directly
- All other components — call store actions

**Impact:** Direct calls bypass the store's error-setting logic and post-operation refresh chain. `ConflictResolver` calls `gitResolveConflict` then calls `refreshStatus()` from the store — this duplicates the same sequence already in `store.resolveConflict`. If someone adds a `refreshConflicts()` call to the store action (logical thing to do), `ConflictResolver` won't benefit.

**Fix:**
- `ConflictResolver`: replace `gitResolveConflict(...)` with `store.resolveConflict(file, res)`. Remove the inline `refreshStatus()` call (the store action already does it).
- `GitPanel` (gitInit): this is a one-off init action, not in the store. Add a `initRepo()` store action that wraps `gitInit` + `setCollection` + resets `isRepo`. Call it from GitPanel.

---

### Q4 — `GitLandingPanel` holds ephemeral UI state (`lastFetched`, `pushing`, `pulling`, `fetching`) that is lost when the panel unmounts

**File:** `src/components/git/GitLandingPanel.tsx:33–38`

```tsx
const [pushing, setPushing] = useState(false);
const [pulling, setPulling] = useState(false);
const [fetching, setFetching] = useState(false);
const [lastFetched, setLastFetched] = useState<string | null>(null);
```

**Problem:** `lastFetched` is a string timestamp set after a fetch or pull. It is used to decide whether to show the "Fetch before push" dialog. If the user navigates to the Commits view and back to landing, `lastFetched` resets to `null`, meaning the fetch-before-push dialog will appear again even though a fetch was just performed.

Also, `pushing/pulling/fetching` loading states are local — if the tab is unmounted and remounted during an operation (unlikely but possible with pane splitting), the spinner is lost.

**Fix:**
- Move `lastFetched` to the git Zustand store as `lastFetchedAt: Date | null`. Persist it across landing panel mounts. The timestamp comparison in `handlePush` reads from the store instead of local state.
- Keep `pushing/pulling/fetching` as local state (they're true UI-only concerns and do not need cross-component visibility).

---

### Q5 — `stageAll` double-invokes status for no reason

**File:** `src/stores/git-store.ts:288–300`

```ts
stageAll: async () => {
  await get().refreshStatus();  // ← extra round-trip
  const { status } = get();
  ...
  await get().stageFiles(unstaged);  // ← stageFiles also calls refreshStatus()
}
```

**Problem:** `stageAll` calls `refreshStatus()` before reading the status (to avoid stale directory entries), then calls `stageFiles()` which calls `refreshStatus()` again after staging. This is 2 IPC calls to `git_status` for a single "Stage All" button press. The comment in the code says the first refresh is needed to avoid stale caches — which is valid — but a cleaner fix is to refresh only at the start, then call `gitStage` directly and do a single refresh at the end.

**Fix:** Inline the staging logic in `stageAll` instead of delegating to `stageFiles`:

```ts
stageAll: async () => {
  await get().refreshStatus();
  const { collectionPath, status } = get();
  if (!collectionPath || !status) return;
  const paths = status.files
    .filter(f => !f.staged && f.status !== 'unchanged')
    .map(f => f.path);
  if (paths.length === 0) return;
  try {
    await gitStage(collectionPath, paths);
    await get().refreshStatus();  // single refresh after staging
  } catch (e) {
    set({ error: String(e) });
  }
}
```

Result: 2 `git_status` calls instead of 3 for a normal "Stage All".

---

### Q6 — `build_simple_diff` produces a semantically misleading diff for large files

**File:** `crates/rocket-git/src/git2_service.rs:135–168`

**Current state:** All old lines appear first as removals, all new lines appear second as additions. For a 500-line file with one line changed, the diff shows 500 red lines followed by 500 green lines. Monaco's `DiffEditor` would show this correctly (it has its own diff algorithm), but the `DiffHunk` returned by the backend (used by `VisualDiffView` and anywhere consuming `hunks` directly) is structurally wrong.

**Impact:** `VisualDiffView` parses the `hunks` field to identify what changed. With all-removals-then-all-additions, it cannot tell which fields actually changed — it sees every field as "old" and every field as "new".

**Fix (short-term):** Keep the current approach but document it clearly in code: `DiffViewer` uses Monaco's own diff (which is correct), so it is not affected. `VisualDiffView` should NOT use `hunks` — it should parse `oldContent` and `newContent` directly (which it already does in most paths). No behavioral change, but remove the misleading comment about this being "intentionally simplistic."

**Fix (long-term, optional):** Replace `build_simple_diff` with a proper Myers diff (using the `similar` crate — already in the Rust ecosystem, MIT licensed, no new external deps philosophy needed). This would make the `hunks` field semantically correct. Flag this as a separate task because it is non-trivial.

---

### Q7 — Tracing instrumentation is inconsistent across git operations

**Current state:**

Instrumented with `#[tracing::instrument]`:
- `clone_repo`, `status`, `push`, `pull`, `fetch`, `commit`

NOT instrumented:
- All branch operations, stash operations, conflict resolution, remote management, staging, discarding, diffing

**Impact:** The operations that are most performance-sensitive for user experience (branch switch, stash pop, resolve conflict) produce no trace spans. Debugging latency issues requires guessing.

**Fix:** Add `#[tracing::instrument(name = "git_<op>", skip(self), fields(repo_path = %path))]` to all remaining `GitService` trait impl methods. For methods with sensitive params (credentials), use `skip(creds)`. This is mechanical and low-risk.

---

## Summary Table

| ID | Priority | Area | Description |
|----|----------|------|-------------|
| Q1 | High | Backend | `git2_service.rs` is 1,300+ lines; extract into sub-modules |
| Q2 | High | Backend | No tests for push/pull/fetch/conflict-resolution |
| Q3 | Medium | Frontend | Inconsistent direct-Tauri vs. store invocation pattern |
| Q4 | Low | Frontend | `lastFetched` lost on panel unmount; affects push dialog logic |
| Q5 | Low | Frontend | `stageAll` makes 3 `git_status` calls instead of 2 |
| Q6 | Medium | Backend | `build_simple_diff` misleading for `hunks` consumers |
| Q7 | Low | Backend | Tracing instrumentation incomplete across git operations |

---

## Implementation Order

Recommended order (dependencies listed):

1. **Q2** first — tests expose existing bugs and protect Q1 refactoring
2. **Q1** — safe to do once Q2 tests are in place (tests ensure refactoring doesn't break anything)
3. **Q3** — clean up frontend patterns independently of backend changes
4. **Q6 short-term** — documentation fix, zero risk
5. **Q5, Q4, Q7** — polish, any order

**Q6 long-term (Myers diff):** separate spec/plan, not part of this cycle.
