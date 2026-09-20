# git-store Network Guard and Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `push`/`pull`/`fetch` from calling the Tauri IPC layer with an `undefined` remote name when no remote is configured, remove the dead `clearPendingNetworkOp` action, and de-duplicate the "does this repo have a conflicted file" check that's currently computed independently in two components.

**Architecture:** `push`, `pull`, and `fetch` all accept an optional `remote?: string` and fall back to `get().remotes[0]?.name` when the caller omits it — but no production caller ever supplies `remote` explicitly, and `remotes[0]?.name` is `string | undefined` when there are zero remotes configured, which then gets passed to `gitPush(repositoryId, remote: string, ...)` — a Tauri command whose Rust side expects a real string. Today that only "works" because nothing exercises a truly remote-less repository in the existing tests; this plan makes it an explicit, user-visible "No remote configured." error instead of an IPC call with a `remote` argument that violates the function's own type. `clearPendingNetworkOp` has zero callers anywhere in `src/` (verified by `grep -rn "clearPendingNetworkOp" src/`) — it's dead public API surface, kept alive only by its own definition. `GitState.hasConflicts: () => boolean` is a method-shaped piece of derived state (`get().status?.files.some(...)`) that `GitLandingPanel.tsx` calls via `state.hasConflicts?.()` while `GitPanel.tsx` independently recomputes the identical `status.files.some((f) => f.status === 'conflicted')` inline — this plan replaces both with one exported selector function.

**Tech Stack:** TypeScript, Zustand vanilla store, Vitest.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` ("Other unused or mismatched pane functionality": *"`push(remote?)`, `pull(remote?)`, and `fetch(remote?)` expose remote selection, but all callers omit it. The fallback `get().remotes[0]?.name` can be `undefined` even though the Tauri API requires `remote: string`."*, *"`clearPendingNetworkOp` — No production caller — Unused public action."*, *"`hasConflicts` — Method-style derived state is awkward; use a selector over path-scoped status."*; "Duplicate logic and boundary observations" item 6: *"Conflict detection is duplicated: `GitPanel` directly scans `status.files`... while `GitLandingPanel` calls the store method."*). Verified present in current `src/stores/git-store.ts:103,149-152,657-717`, `src/components/git/GitPanel.tsx:69-70`, `src/components/git/GitLandingPanel.tsx:177`.

**Depends on:** `docs/superpowers/plans/2026-09-20-git-landing-panel-workflow-guards.md` (Task 2) having already been applied — Task 3 below updates the `gitStoreApi.getState().hasConflicts()` call inside `handleStashAndPull` that plan introduces. If it hasn't been applied yet, update the pre-existing `handleStashAndPull` (which calls `gitStoreApi.getState().hasConflicts()` at the same point) instead — the selector swap is identical either way.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-relative-time-formatter-dedup.md` next (plan 20 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- Rust: not touched by this plan (frontend-only) — no `src-tauri`/`crates` changes.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: `push`/`pull`/`fetch` refuse to call IPC with no resolved remote

**Files:**
- Modify: `src/stores/git-store.ts:656-717`
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Produces: no new exports, no signature change to `push`/`pull`/`fetch`. When neither an explicit `remote` argument nor any configured remote is available, each sets `error: 'No remote configured.'` and returns without calling `gitPush`/`gitPull`/`gitFetch`.

- [ ] **Step 1: Write the failing test**

Add to `src/stores/__tests__/git-store.test.ts`:

```ts
describe('git-store network actions with no remote configured', () => {
  beforeEach(() => {
    store.setState({
      repositoryId: 'repo-1',
      isRepo: true,
      credentials: { type: 'token', token: 'tok' },
      remotes: [],
    });
  });

  it('push sets an error and never calls gitPush', async () => {
    await store.getState().push();
    expect(tauriApi.gitPush).not.toHaveBeenCalled();
    expect(store.getState().error).toBe('No remote configured.');
  });

  it('pull sets an error and never calls gitPull', async () => {
    await store.getState().pull();
    expect(tauriApi.gitPull).not.toHaveBeenCalled();
    expect(store.getState().error).toBe('No remote configured.');
  });

  it('fetch sets an error and never calls gitFetch', async () => {
    await store.getState().fetch();
    expect(tauriApi.gitFetch).not.toHaveBeenCalled();
    expect(store.getState().error).toBe('No remote configured.');
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "with no remote configured"`
Expected: FAIL — today each action calls `gitPush`/`gitPull`/`gitFetch` with `remote: undefined` and no distinct "No remote configured." error.

- [ ] **Step 3: Implement the guard**

In `src/stores/git-store.ts`, update `push`:

```ts
    push: async (remote) => {
      const { repositoryId, credentials } = get();
      if (!repositoryId) return;
      if (!credentials) {
        set({ showCredentialsDialog: true, pendingNetworkOp: 'push' });
        return;
      }
      const resolvedRemote = remote ?? get().remotes[0]?.name;
      if (!resolvedRemote) {
        set({ error: 'No remote configured.' });
        return;
      }
      set({ error: null });
      try {
        await gitPush(repositoryId, resolvedRemote, credentials);
        await get().refreshStatus();
        set({ trustFailure: null });
      } catch (e) {
        set(networkErrorPatch(e, 'push'));
      }
    },
```

Update `pull`:

```ts
    pull: async (remote) => {
      const { repositoryId, credentials } = get();
      if (!repositoryId) return;
      if (!credentials) {
        set({ showCredentialsDialog: true, pendingNetworkOp: 'pull' });
        return;
      }
      const resolvedRemote = remote ?? get().remotes[0]?.name;
      if (!resolvedRemote) {
        set({ error: 'No remote configured.' });
        return;
      }
      set({ error: null });
      try {
        await gitPull(repositoryId, resolvedRemote, credentials);
        set({ trustFailure: null });
      } catch (e) {
        set(networkErrorPatch(e, 'pull'));
      }
      // Always refresh status, conflicts, branches, and the commit log after a
      // pull attempt — whether it succeeded or produced merge conflicts — so
      // the UI reflects the real repo state (behind count, conflict files,
      // incoming commits, etc.).
      await get().refreshStatus();
      await get().refreshConflicts();
      await get().refreshBranches();
      await get().refreshLog();
    },
```

Update `fetch`:

```ts
    fetch: async (remote) => {
      const { repositoryId, credentials } = get();
      if (!repositoryId) return;
      if (!credentials) {
        set({ showCredentialsDialog: true, pendingNetworkOp: 'fetch' });
        return;
      }
      const resolvedRemote = remote ?? get().remotes[0]?.name;
      if (!resolvedRemote) {
        set({ error: 'No remote configured.' });
        return;
      }
      set({ error: null });
      try {
        await gitFetch(repositoryId, resolvedRemote, credentials);
        await get().refreshStatus();
        await get().refreshBranches();
        set({ trustFailure: null });
      } catch (e) {
        set(networkErrorPatch(e, 'fetch'));
      }
    },
```

(`pull`'s body above already includes the `refreshLog()` call from `docs/superpowers/plans/2026-09-20-git-invalidation-fixes.md` Task 2 — if that plan hasn't been applied yet, omit the `await get().refreshLog();` line; the no-remote guard is otherwise identical.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "with no remote configured"`
Expected: PASS

- [ ] **Step 5: Run the full store suite**

Run: `yarn test src/stores/__tests__/git-store.test.ts`
Expected: PASS

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts
```

Commit message: `fix(git): refuse to push/pull/fetch when no remote is configured`.

---

### Task 2: Remove the unused `clearPendingNetworkOp` action

**Files:**
- Modify: `src/stores/git-store.ts:103,637`
- Test: none — this is a pure deletion of unreferenced code; `yarn tsc --noEmit` is the verification.

**Interfaces:**
- Produces: `clearPendingNetworkOp` removed from `GitState` and from the store implementation.

- [ ] **Step 1: Confirm there are no callers**

Run: `grep -rn "clearPendingNetworkOp" src/`
Expected: two matches, both in `src/stores/git-store.ts` itself (the interface declaration and the implementation) — no callers in any component or test.

- [ ] **Step 2: Remove the action**

In `src/stores/git-store.ts`, delete the interface line:

```ts
  clearPendingNetworkOp: () => void;
```

and the implementation line:

```ts
    clearPendingNetworkOp: () => set({ pendingNetworkOp: null }),
```

- [ ] **Step 3: Verify nothing referenced it**

Run: `grep -rn "clearPendingNetworkOp" src/`
Expected: no matches.

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn test src/stores/__tests__/git-store.test.ts`
Expected: PASS

- [ ] **Step 4: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/stores/git-store.ts
```

Commit message: `chore(git): remove unused clearPendingNetworkOp action`.

---

### Task 3: Replace `hasConflicts()` with one shared selector

**Files:**
- Modify: `src/stores/git-store.ts` (interface, initial state/body, add exported `selectHasConflicts`)
- Modify: `src/components/git/GitPanel.tsx:67-70`
- Modify: `src/components/git/GitLandingPanel.tsx:100-101,177`
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Produces: `export function selectHasConflicts(state: GitState): boolean` in `src/stores/git-store.ts`, replacing `GitState.hasConflicts: () => boolean`.
- Consumes (`GitPanel.tsx`): `useStore(store, selectHasConflicts)` in place of the inline `status?.files.some(...)` computation.
- Consumes (`GitLandingPanel.tsx`): `useGitStore(selectHasConflicts)` in place of `useGitStore((state) => state.hasConflicts?.())`, and `selectHasConflicts(gitStoreApi.getState())` in place of `gitStoreApi.getState().hasConflicts()`.

- [ ] **Step 1: Write the failing test**

Add to `src/stores/__tests__/git-store.test.ts`:

```ts
import { createGitStore, type GitState, selectHasConflicts } from '../git-store';
```

(Merge this into the file's existing top-of-file import from `../git-store` rather than adding a second import line.)

```ts
describe('selectHasConflicts', () => {
  it('is true when any status file is conflicted', () => {
    const state = {
      status: { branch: 'main', ahead: 0, behind: 0, isClean: false, files: [{ path: 'a.txt', staged: false, status: 'conflicted' }] },
    } as GitState;
    expect(selectHasConflicts(state)).toBe(true);
  });

  it('is false with no status or no conflicted files', () => {
    expect(selectHasConflicts({ status: null } as GitState)).toBe(false);
    const clean = { status: { branch: 'main', ahead: 0, behind: 0, isClean: true, files: [] } } as GitState;
    expect(selectHasConflicts(clean)).toBe(false);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "selectHasConflicts"`
Expected: FAIL — `selectHasConflicts` doesn't exist yet.

- [ ] **Step 3: Add the selector and remove the method**

In `src/stores/git-store.ts`, remove from `GitState`:

```ts
  hasConflicts: () => boolean;
```

Remove from the store body (the `hasConflicts: () => { const { status } = get(); return status?.files.some(...) ?? false; },` field near the top of the returned state object).

Add, near the bottom of the file (after `createGitStore`, as a standalone export — not part of the store body):

```ts
/** True when the repository's current status has any conflicted file. Shared
 *  by GitPanel and GitLandingPanel so conflict detection isn't computed
 *  independently in two places. */
export function selectHasConflicts(state: GitState): boolean {
  return state.status?.files.some((f) => f.status === 'conflicted') ?? false;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "selectHasConflicts"`
Expected: PASS

- [ ] **Step 5: Update `GitPanel.tsx` to use the shared selector**

In `src/components/git/GitPanel.tsx`, add the import:

```tsx
import { createGitStore, selectHasConflicts } from '@/stores/git-store';
```

Replace:

```tsx
  const currentBranch = status?.branch ?? null;
  const hasConflicts = status?.files.some((f) => f.status === 'conflicted') ?? false;
  const conflictCount = status?.files.filter((f) => f.status === 'conflicted').length ?? 0;
```

with:

```tsx
  const currentBranch = status?.branch ?? null;
  const hasConflicts = useStore(store, selectHasConflicts);
  const conflictCount = status?.files.filter((f) => f.status === 'conflicted').length ?? 0;
```

- [ ] **Step 6: Update `GitLandingPanel.tsx` to use the shared selector**

In `src/components/git/GitLandingPanel.tsx`, add the import:

```tsx
import { selectHasConflicts } from '@/stores/git-store';
```

Replace:

```tsx
  const hasConflicts = useGitStore((state) => state.hasConflicts?.()) ?? false;
```

with:

```tsx
  const hasConflicts = useGitStore(selectHasConflicts);
```

Replace, inside `handleStashAndPull`:

```tsx
      if (gitStoreApi.getState().hasConflicts()) {
```

with:

```tsx
      if (selectHasConflicts(gitStoreApi.getState())) {
```

- [ ] **Step 7: Run the full git component suite and typecheck**

Run: `yarn test src/components/git src/stores/__tests__/git-store.test.ts`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/stores/git-store.ts src/components/git/GitPanel.tsx src/components/git/GitLandingPanel.tsx src/stores/__tests__/git-store.test.ts
```

Commit message: `refactor(git): replace the hasConflicts() method with one shared selectHasConflicts selector`.
