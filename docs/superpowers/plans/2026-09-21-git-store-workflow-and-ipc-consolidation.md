# Git Store Workflow and IPC Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the multi-step Git workflows and incidental Tauri IPC calls that still live inside Git presentation components into testable `git-store` actions, and give the commit-diff flow real loading/error feedback instead of silently swallowing failures.

**Architecture:** `GitLandingPanel`'s `handleStashAndPull`/`handleFetchAndPush` currently inline the entire stash→pull→pop and fetch→push sequencing (including the conflict-skip-pop and behind-check-abort-push logic) inside the component, which is why the review's own "focused tests" recommendation says these can only be tested by rendering the component. This plan extracts that sequencing into two new `git-store` actions (`stashThenPull`, `fetchThenPush`) that call the store's own existing actions via `get()`, so they can be unit-tested directly against the store. Separately, `GitCommitForm` (identity check/set) and `GitPanel` (commit-diff fetch) and `GitCloneDialog` (clone/detect-structure) import Tauri IPC functions directly instead of going through the store; this plan adds thin store actions for each and updates the three components to call them, and adds the missing loading/error state around the commit-diff fetch in `GitPanel`. `DiffViewer` is deliberately **not** touched — its existing tests (`src/components/git/__tests__/DiffViewer.test.tsx`) render it without a `GitStoreProvider`, which proves it is intentionally store-independent and reusable outside a loaded repository's panel; moving its already-correct, request-id-guarded diff-toggle logic into the store would remove that independence and break those tests for no behavioral gain.

**Tech Stack:** React 18 + TypeScript, Zustand (vanilla store via `createGitStore()` + React context via `GitStoreProvider`/`useGitStore`), Vitest + @testing-library/react + @testing-library/user-event.

**Spec:** docs/reports/git-integration-review/02-frontend-architecture.md (Finding F-13 "Component boundaries duplicate ownership and encode backend workflows in presentation components"; Finding F-07's commit-diff sub-item: "Commit diff failures are silently ignored"). All other findings in that document (F-01–F-06, F-08–F-12, F-14, F-15) are already implemented by the prior `docs/superpowers/plans/2026-09-20-git-*.md` remediation sequence and are out of scope here.

## Global Constraints

- Zustand: never fully destructure store state at component top level — use narrow selectors (CLAUDE.md `.claude/rules/frontend-component-guardrails.md`).
- Store actions must not throw for expected operational failures — follow the existing `clearError()`-before / `if (get().error)`-after idiom already used throughout `git-store.ts` for every mutation action. `loadCommitDiff`, `cloneRepository`, and `detectClonedRepoStructure` are the deliberate exceptions: they are read/one-shot operations whose callers already have their own try/catch (matching `refreshLog`'s sibling pattern is not applicable since those three have no natural "refresh" home and their callers need the thrown error's message directly).
- No raw `<button>`/`<input>`/`<select>` — this plan does not add new UI, only relocates existing IPC calls and adds a loading spinner (`lucide-react` `Loader2`, already used elsewhere in these files) and an existing `role='alert'` banner pattern.
- Rust backend is not touched by this plan — frontend-only.
- Preserve exact existing behavior for every relocated workflow; do not use this plan as an opportunity to change semantics (e.g. `fetchThenPush` must still record `lastFetched` purely based on whether the *fetch* step succeeded, independent of whether the subsequent push ran or succeeded — see Task 1).

---

### Task 1: Extract `stashThenPull` and `fetchThenPush` into `git-store` actions

**Files:**
- Modify: `src/stores/git-store.ts`
- Modify: `src/components/git/GitLandingPanel.tsx`
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Produces: `stashThenPull: () => Promise<void>` and `fetchThenPush: () => Promise<boolean>` added to `GitState`. `fetchThenPush` resolves `true` iff the fetch step itself succeeded (independent of whether push subsequently ran or succeeded) — this is the exact condition `GitLandingPanel` already uses to decide whether to record `lastFetched`.
- Consumes: existing `GitState` actions `saveStash`, `pull`, `popStash`, `fetch`, `push`, `clearError`, and the existing exported `selectHasConflicts(state: GitState): boolean`.

- [ ] **Step 1: Write the failing store tests**

Add to `src/stores/__tests__/git-store.test.ts` (place after the existing `describe('git-store pull refreshes the commit log', ...)` block):

```ts
describe('stashThenPull', () => {
  beforeEach(() => {
    store.setState({
      repositoryId: 'repository-test',
      isRepo: true,
      error: null,
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: false },
    });
  });

  it('stashes, pulls, and pops when every step succeeds', async () => {
    const { gitStashSave, gitPull, gitStashPop } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);

    await store.getState().stashThenPull();

    expect(gitStashSave).toHaveBeenCalledWith('repository-test', 'Auto-stash before pull');
    expect(gitPull).toHaveBeenCalled();
    expect(gitStashPop).toHaveBeenCalledWith('repository-test', 0);
  });

  it('does not pull when the stash save fails', async () => {
    const { gitStashSave, gitPull, gitStashPop } = await import('@/lib/tauri-api');
    vi.mocked(gitStashSave).mockRejectedValueOnce(new Error('nothing to stash'));

    await store.getState().stashThenPull();

    expect(gitPull).not.toHaveBeenCalled();
    expect(gitStashPop).not.toHaveBeenCalled();
    expect(store.getState().error).toContain('nothing to stash');
  });

  it('does not pop the stash when pull fails', async () => {
    const { gitPull, gitStashPop } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockRejectedValueOnce(new Error('authentication failed'));

    await store.getState().stashThenPull();

    expect(gitStashPop).not.toHaveBeenCalled();
  });

  it('does not pop the stash when the pull produces merge conflicts', async () => {
    const { gitPull, gitStashPop } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);
    vi.mocked(tauriApi.gitStatus).mockResolvedValueOnce({
      branch: 'main',
      files: [{ path: 'a.txt', staged: false, status: 'conflicted' }],
      ahead: 0,
      behind: 0,
      isClean: false,
    });

    await store.getState().stashThenPull();

    expect(gitStashPop).not.toHaveBeenCalled();
  });

  it('clears a stale pre-existing error before checking each step', async () => {
    const { gitStashSave, gitPull, gitStashPop } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);
    store.setState({ error: 'stale error from an earlier push' });

    await store.getState().stashThenPull();

    expect(gitStashSave).toHaveBeenCalled();
    expect(gitPull).toHaveBeenCalled();
    expect(gitStashPop).toHaveBeenCalledWith('repository-test', 0);
  });
});

describe('fetchThenPush', () => {
  beforeEach(() => {
    store.setState({
      repositoryId: 'repository-test',
      isRepo: true,
      error: null,
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
      status: { branch: 'main', files: [], ahead: 1, behind: 0, isClean: true },
    });
  });

  it('returns true and pushes after a successful fetch that leaves the branch not behind', async () => {
    const { gitFetch, gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitFetch).mockResolvedValueOnce({
      updatedRefs: [],
      receivedObjects: 0,
      receivedBytes: 0,
    });
    vi.mocked(tauriApi.gitStatus).mockResolvedValueOnce({
      branch: 'main',
      files: [],
      ahead: 1,
      behind: 0,
      isClean: true,
    });

    const result = await store.getState().fetchThenPush();

    expect(result).toBe(true);
    expect(gitPush).toHaveBeenCalled();
  });

  it('returns false and does not push when the fetch fails', async () => {
    const { gitFetch, gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitFetch).mockRejectedValueOnce(new Error('authentication failed'));

    const result = await store.getState().fetchThenPush();

    expect(result).toBe(false);
    expect(gitPush).not.toHaveBeenCalled();
  });

  it('returns true but does not push when the post-fetch status is now behind the remote', async () => {
    const { gitFetch, gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitFetch).mockResolvedValueOnce({
      updatedRefs: ['refs/heads/main'],
      receivedObjects: 3,
      receivedBytes: 900,
    });
    vi.mocked(tauriApi.gitStatus).mockResolvedValueOnce({
      branch: 'main',
      files: [],
      ahead: 1,
      behind: 2,
      isClean: true,
    });

    const result = await store.getState().fetchThenPush();

    expect(result).toBe(true);
    expect(gitPush).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "stashThenPull|fetchThenPush"`
Expected: FAIL — `store.getState().stashThenPull is not a function` (and same for `fetchThenPush`).

- [ ] **Step 3: Implement the actions in `git-store.ts`**

Add both to the `GitState` interface, directly below the existing `fetch: (remote?: string) => Promise<void>;` line:

```ts
  fetch: (remote?: string) => Promise<void>;
  /** Stash working-tree changes, pull, then restore the stash — stopping
   *  immediately if any step fails or the pull produces merge conflicts.
   *  Extracted from GitLandingPanel so the sequencing is unit-testable
   *  without rendering the component (see
   *  docs/reports/git-integration-review/02-frontend-architecture.md, F-13). */
  stashThenPull: () => Promise<void>;
  /** Fetch, then push only if the fetch succeeded and did not leave the
   *  branch behind the remote. Resolves whether the fetch step itself
   *  succeeded (independent of whether push ran or succeeded) — the caller
   *  uses that alone to decide whether to record a "last fetched" timestamp. */
  fetchThenPush: () => Promise<boolean>;
```

Add both implementations in `createGitStore()`, directly below the existing `fetch: async (remote) => { ... },` action and before `clearError: () => set({ error: null }),`:

```ts
    stashThenPull: async () => {
      get().clearError();
      await get().saveStash('Auto-stash before pull');
      if (get().error) {
        // Stash itself failed — nothing changed, nothing to pull or pop.
        return;
      }
      get().clearError();
      await get().pull();
      if (get().error) {
        // Pull failed outright (network/auth/etc.) — leave the stash in place
        // rather than popping it on top of an unknown working-tree state.
        return;
      }
      // After pull, check whether it produced merge conflicts.
      // If so, do NOT restore the stash — applying it on top of a conflicted
      // index would corrupt the working tree with doubled conflicts.
      if (selectHasConflicts(get())) {
        // Leave the stash in place; the user can pop it after resolving conflicts.
        return;
      }
      await get().popStash(0);
    },

    fetchThenPush: async () => {
      await get().fetch();
      if (get().error) return false;
      // Re-check status after fetch — if now behind, abort push.
      const { status } = get();
      if (status && status.behind > 0) return true;
      await get().push();
      return true;
    },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "stashThenPull|fetchThenPush"`
Expected: PASS (8 tests).

- [ ] **Step 5: Update `GitLandingPanel.tsx` to call the new actions**

Replace the `saveStash`/`popStash` selectors with `stashThenPull`/`fetchThenPush` selectors:

```ts
  const stashThenPull = useGitStore((s) => s.stashThenPull);
  const fetchThenPush = useGitStore((s) => s.fetchThenPush);
```

(removing `const saveStash = useGitStore((s) => s.saveStash);` and `const popStash = useGitStore((s) => s.popStash);` — they are no longer called directly from this component.)

Replace `handleStashAndPull`:

```ts
  const handleStashAndPull = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try {
      await stashThenPull();
      if (!gitStoreApi.getState().error) {
        setLastFetched(new Date().toLocaleTimeString());
      }
    } finally {
      setPulling(false);
    }
  };
```

Replace `handleFetchAndPush`:

```ts
  const handleFetchAndPush = async () => {
    setShowFetchFirstDialog(false);
    setPushing(true);
    try {
      const fetchSucceeded = await fetchThenPush();
      if (fetchSucceeded) {
        setLastFetched(new Date().toLocaleTimeString());
      }
    } finally {
      setPushing(false);
    }
  };
```

- [ ] **Step 6: Run the existing component suite to confirm no regressions**

Run: `yarn test src/components/git/__tests__/GitLandingPanel.test.tsx`
Expected: PASS — all existing tests (including "does not abort stash-then-pull due to a stale error left over from an earlier, unrelated failure" and "does not push after a fetch failure in the fetch-then-push flow") continue to pass unchanged, since they exercise the same DOM buttons and store-level mocks (`saveStash`/`pull`/`popStash`/`fetch`/`push` overridden via `store.setState`) that the new store actions still read through `get()`.

- [ ] **Step 7: Run the type checker**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts src/components/git/GitLandingPanel.tsx
git commit -m "refactor(git): extract stashThenPull/fetchThenPush into testable store actions"
```

---

### Task 2: Move identity check/set into `git-store` actions, replacing `GitCommitForm`'s direct IPC calls

**Files:**
- Modify: `src/stores/git-store.ts`
- Modify: `src/components/git/GitCommitForm.tsx`
- Test: `src/stores/__tests__/git-store.test.ts`
- Test: `src/components/git/__tests__/GitCommitForm.test.tsx`

**Interfaces:**
- Produces: `checkIdentity: () => Promise<GitIdentity | null>` (returns `null` when the identity is unset or the lookup fails — both are "identity unknown" to callers) and `setIdentity: (name: string, email: string) => Promise<void>` (sets `error` on failure, following the store's standard convention) added to `GitState`.
- Consumes: `gitGetIdentity`, `gitSetIdentity`, and `GitIdentity` from `@/lib/tauri-api`.

- [ ] **Step 1: Write the failing store tests**

Add to `src/stores/__tests__/git-store.test.ts`, after the `describe('stash', ...)` block:

```ts
describe('checkIdentity and setIdentity', () => {
  beforeEach(() => {
    store.setState({ repositoryId: 'repository-test', error: null });
  });

  it('checkIdentity returns the identity when name and email are set', async () => {
    vi.mocked(tauriApi.gitGetIdentity).mockResolvedValueOnce({
      name: 'Ada Lovelace',
      email: 'ada@example.com',
    });

    const identity = await store.getState().checkIdentity();

    expect(identity).toEqual({ name: 'Ada Lovelace', email: 'ada@example.com' });
  });

  it('checkIdentity returns null when the identity is blank', async () => {
    vi.mocked(tauriApi.gitGetIdentity).mockResolvedValueOnce({ name: '', email: '' });

    expect(await store.getState().checkIdentity()).toBeNull();
  });

  it('checkIdentity returns null when the lookup fails', async () => {
    vi.mocked(tauriApi.gitGetIdentity).mockRejectedValueOnce(new Error('git config unreadable'));

    expect(await store.getState().checkIdentity()).toBeNull();
  });

  it('setIdentity calls gitSetIdentity with the repository, name, and email', async () => {
    vi.mocked(tauriApi.gitSetIdentity).mockResolvedValueOnce(undefined);

    await store.getState().setIdentity('Ada Lovelace', 'ada@example.com');

    expect(tauriApi.gitSetIdentity).toHaveBeenCalledWith(
      'repository-test',
      'Ada Lovelace',
      'ada@example.com',
    );
    expect(store.getState().error).toBeNull();
  });

  it('setIdentity sets an error when the save fails', async () => {
    vi.mocked(tauriApi.gitSetIdentity).mockRejectedValueOnce(new Error('permission denied'));

    await store.getState().setIdentity('Ada Lovelace', 'ada@example.com');

    expect(store.getState().error).toContain('permission denied');
  });
});
```

This requires `gitSetIdentity` to be present in the file's top-level `vi.mock('@/lib/tauri-api', ...)` object — add `gitSetIdentity: vi.fn()` to it (`gitGetIdentity` is already there).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "checkIdentity and setIdentity"`
Expected: FAIL — `store.getState().checkIdentity is not a function`.

- [ ] **Step 3: Implement the actions**

In `git-store.ts`, add `type GitIdentity` to the `@/lib/tauri-api` import list (alongside the other `type` imports).

Add to `GitState`, directly below `checkoutRemoteBranch`:

```ts
  /** Return the repository's configured git identity, or null if it is
   *  unset or the lookup fails — both are treated as "identity unknown" by
   *  callers (see GitCommitForm). */
  checkIdentity: () => Promise<GitIdentity | null>;
  /** Save the repository's git identity (user.name/user.email). */
  setIdentity: (name: string, email: string) => Promise<void>;
```

Add to `createGitStore()`, directly below the `deleteBranch` action:

```ts
    checkIdentity: async () => {
      const { repositoryId } = get();
      if (!repositoryId) return null;
      try {
        const identity = await gitGetIdentity(repositoryId);
        if (!identity.name.trim() || !identity.email.trim()) return null;
        return identity;
      } catch {
        return null;
      }
    },

    setIdentity: async (name, email) => {
      const { repositoryId } = get();
      if (!repositoryId) return;
      try {
        await gitSetIdentity(repositoryId, name, email);
      } catch (e) {
        set({ error: `Failed to save git identity: ${String(e)}` });
      }
    },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "checkIdentity and setIdentity"`
Expected: PASS (5 tests).

- [ ] **Step 5: Update `GitCommitForm.tsx`**

Remove the `import { gitGetIdentity, gitSetIdentity } from '@/lib/tauri-api';` line.

Add two selectors alongside the existing ones:

```ts
  const checkIdentity = useGitStore((state) => state.checkIdentity);
  const setIdentity = useGitStore((state) => state.setIdentity);
```

Replace `handleCommit`:

```ts
  const handleCommit = async () => {
    if (!message.trim() || stagedCount === 0) return;
    if (!repositoryId) return;

    const identity = await checkIdentity();
    if (!identity) {
      setShowIdentityDialog(true);
      return;
    }

    await doCommit();
  };
```

Replace `handleIdentityConfirm`:

```ts
  const handleIdentityConfirm = async (name: string, email: string) => {
    setShowIdentityDialog(false);
    if (!repositoryId) return;
    gitStoreApi.setState({ error: null });
    await setIdentity(name, email);
    if (gitStoreApi.getState().error) return;
    await doCommit();
  };
```

- [ ] **Step 6: Write a new component test for the identity-missing flow**

Add to `src/components/git/__tests__/GitCommitForm.test.tsx`, as a new top-level `describe` block (this flow currently has zero coverage):

```ts
describe('GitCommitForm identity setup', () => {
  it('prompts for identity before committing when none is configured, then commits after it is saved', async () => {
    vi.mocked(tauriApi.gitGetIdentity).mockResolvedValueOnce({ name: '', email: '' });
    vi.mocked(tauriApi.gitSetIdentity).mockResolvedValueOnce(undefined);
    const commitChanges = vi.fn().mockResolvedValue(undefined);
    renderForm(commitChanges);
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Commit message'), 'fix: broken thing');
    await user.click(screen.getByRole('button', { name: /commit 1 file/i }));

    expect(await screen.findByText(/git identity/i)).toBeInTheDocument();
    expect(commitChanges).not.toHaveBeenCalled();

    await user.type(screen.getByLabelText(/name/i), 'Ada Lovelace');
    await user.type(screen.getByLabelText(/email/i), 'ada@example.com');
    await user.click(screen.getByRole('button', { name: /save identity/i }));

    await vi.waitFor(() => expect(commitChanges).toHaveBeenCalledWith('fix: broken thing'));
  });
});
```

(This test relies on `GitIdentityDialog`'s default confirm-button label; if it differs from "Save identity" or the field labels differ from "Name"/"Email", inspect `src/components/git/GitIdentityDialog.tsx` and adjust the queries to match its actual accessible names before running.)

- [ ] **Step 7: Run the component suite**

Run: `yarn test src/components/git/__tests__/GitCommitForm.test.tsx`
Expected: PASS — the three pre-existing tests continue to pass unchanged (the mocked `gitGetIdentity` in the file's top-level `vi.mock` still resolves a valid identity by default, so `checkIdentity()` returns non-null and `doCommit()` is reached exactly as before), plus the new test passes.

- [ ] **Step 8: Run the type checker**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts src/components/git/GitCommitForm.tsx src/components/git/__tests__/GitCommitForm.test.tsx
git commit -m "refactor(git): move identity check/set into git-store actions"
```

---

### Task 3: Add `loadCommitDiff` store action and fix the silently-swallowed commit-diff failure in `GitPanel`

**Files:**
- Modify: `src/stores/git-store.ts`
- Modify: `src/components/git/GitPanel.tsx`
- Test: `src/stores/__tests__/git-store.test.ts`
- Test: `src/components/git/__tests__/GitPanel.test.tsx`

**Interfaces:**
- Produces: `loadCommitDiff: (oid: string) => Promise<FileDiff[]>` on `GitState` — rethrows on failure (unlike other store actions) so the caller can render an actionable error message instead of the current silent no-op.
- Consumes: `gitDiffCommit` and `type FileDiff` from `@/lib/tauri-api`.

- [ ] **Step 1: Write the failing store tests**

Add to `src/stores/__tests__/git-store.test.ts`, after the `checkIdentity and setIdentity` block:

```ts
describe('loadCommitDiff', () => {
  beforeEach(() => {
    store.setState({ repositoryId: 'repository-test' });
  });

  it('resolves the diffs for the given commit', async () => {
    const diffs = [{ path: 'a.txt', oldContent: 'old', newContent: 'new', hunks: [] }];
    vi.mocked(tauriApi.gitDiffCommit).mockResolvedValueOnce(diffs);

    const result = await store.getState().loadCommitDiff('abc123');

    expect(tauriApi.gitDiffCommit).toHaveBeenCalledWith('repository-test', 'abc123');
    expect(result).toEqual(diffs);
  });

  it('rejects when the IPC call fails, without touching store error state', async () => {
    vi.mocked(tauriApi.gitDiffCommit).mockRejectedValueOnce(new Error('object not found'));

    await expect(store.getState().loadCommitDiff('abc123')).rejects.toThrow('object not found');
    expect(store.getState().error).toBeNull();
  });
});
```

This requires `gitDiffCommit` to be present in the file's top-level `vi.mock('@/lib/tauri-api', ...)` object — add `gitDiffCommit: vi.fn()` to it.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "loadCommitDiff"`
Expected: FAIL — `store.getState().loadCommitDiff is not a function`.

- [ ] **Step 3: Implement the action**

In `git-store.ts`, add `gitDiffCommit` and `type FileDiff` to the `@/lib/tauri-api` import list.

Add to `GitState`, directly below `refreshLog`:

```ts
  /** Fetch the full per-file diff for a single commit. Thin wrapper around
   *  the IPC call so components read `repositoryId` from the store instead
   *  of importing gitDiffCommit directly. Rethrows on failure so the caller
   *  can render an actionable error instead of discarding it. */
  loadCommitDiff: (oid: string) => Promise<FileDiff[]>;
```

Add to `createGitStore()`, directly below the `refreshLog` action:

```ts
    loadCommitDiff: async (oid) => {
      const { repositoryId } = get();
      if (!repositoryId) throw new Error('No repository loaded.');
      return gitDiffCommit(repositoryId, oid);
    },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "loadCommitDiff"`
Expected: PASS (2 tests).

- [ ] **Step 5: Update `GitPanel.tsx`**

Remove `gitDiffCommit` from the `@/lib/tauri-api` import (keep `gitSetIdentity`, `onCollectionChanged`, and the type imports).
Add `Loader2` to the `lucide-react` import.

Add a selector and two local state fields, near the other `useState`/`useStore` declarations:

```ts
  const [commitDiffError, setCommitDiffError] = useState<string | null>(null);
  const [loadingCommitDiff, setLoadingCommitDiff] = useState(false);
  const loadCommitDiff = useStore(store, (state) => state.loadCommitDiff);
```

Replace `handleCommitClick`:

```ts
  const handleCommitClick = async (commit: CommitInfo) => {
    setCommitDiffError(null);
    setLoadingCommitDiff(true);
    try {
      const diffs = await loadCommitDiff(commit.fullId);
      setRightPanel({ kind: 'commitDiff', commit, diffs });
    } catch (e) {
      setCommitDiffError(String(e));
    } finally {
      setLoadingCommitDiff(false);
    }
  };
```

Replace the `rightPanel.kind === 'commits'` render branch:

```tsx
              {rightPanel.kind === 'commits' && (
                <div className='flex flex-col h-full'>
                  {commitDiffError && (
                    <div
                      role='alert'
                      className='px-3 py-2 text-xs text-destructive border-b border-border/70 bg-destructive/10 shrink-0'
                    >
                      Failed to load commit diff: {commitDiffError}
                    </div>
                  )}
                  <div className='flex-1 overflow-hidden relative'>
                    <GitCommitLog onCommitClick={handleCommitClick} />
                    {loadingCommitDiff && (
                      <div className='absolute inset-0 flex items-center justify-center bg-background/60'>
                        <Loader2 className='h-5 w-5 animate-spin text-muted-foreground' />
                      </div>
                    )}
                  </div>
                </div>
              )}
```

- [ ] **Step 6: Write the failing component test**

Add to `src/components/git/__tests__/GitPanel.test.tsx`: add `gitLog: vi.fn().mockResolvedValue([])` and `gitDiffCommit: vi.fn()` to the top-level `vi.mock('@/lib/tauri-api', ...)` object, add `import type { CommitInfo } from '@/lib/tauri-api';` to the imports, and add:

```ts
describe('GitPanel commit diff loading', () => {
  const commit: CommitInfo = {
    id: 'abc1234',
    fullId: 'abc1234abc1234abc1234',
    message: 'test commit',
    author: 'Test User',
    authorEmail: 'test@example.com',
    timestamp: '2026-01-01T00:00:00Z',
    filesChanged: 1,
  };

  beforeEach(() => {
    vi.mocked(tauriApi.gitIsRepo).mockResolvedValue(true);
    vi.mocked(tauriApi.gitLog).mockResolvedValue([commit]);
  });

  async function renderReadyPanelOnCommitsView() {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <GitPanel repositoryId='repo-a' repositoryLabel='Repo A' />
      </QueryClientProvider>,
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /commits/i }));
    await screen.findByRole('button', { name: /test commit/i });
    return user;
  }

  it('shows an error and stays on the commits view when loading a commit diff fails', async () => {
    vi.mocked(tauriApi.gitDiffCommit).mockRejectedValueOnce(new Error('object not found'));
    const user = await renderReadyPanelOnCommitsView();

    await user.click(screen.getByRole('button', { name: /test commit/i }));

    expect(await screen.findByText(/object not found/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /test commit/i })).toBeInTheDocument();
  });

  it('opens the commit diff view when loading succeeds', async () => {
    vi.mocked(tauriApi.gitDiffCommit).mockResolvedValueOnce([
      { path: 'a.txt', oldContent: 'old', newContent: 'new', hunks: [] },
    ]);
    const user = await renderReadyPanelOnCommitsView();

    await user.click(screen.getByRole('button', { name: /test commit/i }));

    await waitFor(() => {
      expect(screen.queryByRole('button', { name: /test commit/i })).not.toBeInTheDocument();
    });
  });
});
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `yarn test src/components/git/__tests__/GitPanel.test.tsx`
Expected: PASS — the pre-existing "repository scoping" tests continue to pass, and the two new tests pass.

- [ ] **Step 8: Run the type checker**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts src/components/git/GitPanel.tsx src/components/git/__tests__/GitPanel.test.tsx
git commit -m "fix(git): stop silently swallowing commit-diff load failures"
```

---

### Task 4: Add `cloneRepository`/`detectClonedRepoStructure` store actions, replacing `GitCloneDialog`'s direct IPC calls

**Files:**
- Modify: `src/stores/git-store.ts`
- Modify: `src/components/git/GitCloneDialog.tsx`
- Test: `src/stores/__tests__/git-store.test.ts`

**Interfaces:**
- Produces: `cloneRepository: (url: string, capability: string, creds: GitCredentials) => Promise<void>` and `detectClonedRepoStructure: (path: string) => Promise<ClonedRepoStructure>` on `GitState`. Both are thin passthroughs that rethrow on failure — `GitCloneDialog` already owns all clone-flow sequencing, request-id guarding (`requestIdRef`), and error handling from the F-02 remediation; this task only relocates the two raw IPC calls so all Git IPC the panel initiates goes through the store, per F-13.
- Consumes: `gitClone`, `detectClonedStructure`, and `type ClonedRepoStructure` from `@/lib/tauri-api` (already imported in `git-store.ts` for other purposes except these two).

- [ ] **Step 1: Write the failing store tests**

Add to `src/stores/__tests__/git-store.test.ts`, after the `loadCommitDiff` block:

```ts
describe('cloneRepository and detectClonedRepoStructure', () => {
  it('cloneRepository calls gitClone with the given url, capability, and credentials', async () => {
    vi.mocked(tauriApi.gitClone).mockResolvedValueOnce(undefined);
    const creds: GitCredentials = { type: 'sshAgent' };

    await store.getState().cloneRepository('https://example.com/repo.git', 'cap-1', creds);

    expect(tauriApi.gitClone).toHaveBeenCalledWith(
      'https://example.com/repo.git',
      'cap-1',
      creds,
    );
  });

  it('detectClonedRepoStructure calls detectClonedStructure with the given path', async () => {
    const structure = { kind: 'unknown' as const, workspacePath: null, collections: [] };
    vi.mocked(tauriApi.detectClonedStructure).mockResolvedValueOnce(structure);

    const result = await store.getState().detectClonedRepoStructure('/tmp/cloned-repo');

    expect(tauriApi.detectClonedStructure).toHaveBeenCalledWith('/tmp/cloned-repo');
    expect(result).toEqual(structure);
  });
});
```

This requires `gitClone` and `detectClonedStructure` to be present in the file's top-level `vi.mock('@/lib/tauri-api', ...)` object — add `gitClone: vi.fn()` and `detectClonedStructure: vi.fn()` to it.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "cloneRepository and detectClonedRepoStructure"`
Expected: FAIL — `store.getState().cloneRepository is not a function`.

- [ ] **Step 3: Implement the actions**

In `git-store.ts`, add `gitClone`, `detectClonedStructure`, and `type ClonedRepoStructure` to the `@/lib/tauri-api` import list.

Add to `GitState`, directly below `initRepo`:

```ts
  /** Thin passthrough to the clone IPC call, used before a repository is
   *  loaded into this store (see GitCloneDialog). The component owns all
   *  clone-flow sequencing, request-id guarding, and error handling — this
   *  exists only so the panel's Git IPC calls all go through the store. */
  cloneRepository: (url: string, capability: string, creds: GitCredentials) => Promise<void>;
  /** Thin passthrough to the post-clone structure-detection IPC call. */
  detectClonedRepoStructure: (path: string) => Promise<ClonedRepoStructure>;
```

Add to `createGitStore()`, directly below `initRepo`:

```ts
    cloneRepository: (url, capability, creds) => gitClone(url, capability, creds),
    detectClonedRepoStructure: (path) => detectClonedStructure(path),
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/stores/__tests__/git-store.test.ts -t "cloneRepository and detectClonedRepoStructure"`
Expected: PASS (2 tests).

- [ ] **Step 5: Update `GitCloneDialog.tsx`**

Remove `detectClonedStructure` and `gitClone` from the `@/lib/tauri-api` import (keep `CloneDestinationGrant`, `ClonedRepoStructure`, `CollectionScanResult`, `GitCredentials`, `selectCloneDestination`).

Add two selectors alongside the existing `credentials`/`gitStoreApi` ones:

```ts
  const cloneRepository = useGitStore((s) => s.cloneRepository);
  const detectClonedRepoStructure = useGitStore((s) => s.detectClonedRepoStructure);
```

In `handlePostClone`, replace `const structure: ClonedRepoStructure = await detectClonedStructure(clonedPath);` with:

```ts
      const structure: ClonedRepoStructure = await detectClonedRepoStructure(clonedPath);
```

In `performClone`, replace `await gitClone(repoUrl.trim(), destination.capability, creds);` with:

```ts
        await cloneRepository(repoUrl.trim(), destination.capability, creds);
```

Add `detectClonedRepoStructure` and `cloneRepository` to the `useCallback` dependency arrays of `handlePostClone` and `performClone` respectively (replacing the now-absent direct-import references, which had no dependency-array entries since module-level imports are stable).

- [ ] **Step 6: Run the existing test suite to confirm no regressions**

Run: `yarn test src/components/git/__tests__/GitCloneDialog.test.tsx`
Expected: PASS — every existing assertion against `tauriApi.gitClone`/`tauriApi.detectClonedStructure` (e.g. `expect(tauriApi.gitClone).not.toHaveBeenCalled()`) continues to pass unchanged, since the new store actions call those same mocked functions under the hood.

- [ ] **Step 7: Run the type checker**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts src/components/git/GitCloneDialog.tsx
git commit -m "refactor(git): route clone/detect-structure IPC through the store"
```

---

## Final verification

- [ ] Run `yarn tsc --noEmit` — no errors.
- [ ] Run `yarn check` — no lint/format violations.
- [ ] Run `yarn test src/stores/__tests__/git-store.test.ts src/components/git/__tests__/GitLandingPanel.test.tsx src/components/git/__tests__/GitCommitForm.test.tsx src/components/git/__tests__/GitPanel.test.tsx src/components/git/__tests__/GitCloneDialog.test.tsx` — full green.
