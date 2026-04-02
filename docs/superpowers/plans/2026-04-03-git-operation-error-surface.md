# Git Operation Error Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface push/pull/fetch errors to the user, disable Push during merge conflicts, and set lastFetched after pull.

**Architecture:** Two files. The git store gets a `clearError` action and clears stale errors before each operation. `GitLandingPanel` reads `error` and `clearError` from the store, shows an inline dismissible alert, disables Push when `hasConflicts`, and marks `lastFetched` after every pull path.

**Tech Stack:** React, TypeScript, Zustand, Tailwind CSS, Shadcn UI

---

## File Map

| File | Change |
|------|--------|
| `src/stores/git-store.ts` | Add `clearError` action; clear `error` before push/pull/fetch |
| `src/components/git/GitLandingPanel.tsx` | Show inline error alert; disable Push on `hasConflicts`; set `lastFetched` after pull |

---

## Task 1: git-store — clearError and Pre-operation Error Clear

**Files:**
- Modify: `src/stores/git-store.ts`
- Test: `src/stores/__tests__/git-store.test.ts` (create)

- [ ] **Step 1: Write the failing test**

Create `src/stores/__tests__/git-store.test.ts`:

```ts
import { vi, describe, it, expect, beforeEach } from 'vitest';
import { useGitStore } from '../git-store';

vi.mock('@/lib/tauri-api', () => ({
  gitIsRepo: vi.fn(),
  gitStatus: vi.fn().mockResolvedValue({
    branch: 'main', files: [], ahead: 0, behind: 0, isClean: true,
  }),
  gitBranches: vi.fn().mockResolvedValue({ local: [], remote: [] }),
  gitRemotes: vi.fn().mockResolvedValue([]),
  gitStashes: vi.fn().mockResolvedValue([]),
  gitLog: vi.fn().mockResolvedValue([]),
  gitConflicts: vi.fn().mockResolvedValue([]),
  gitPush: vi.fn().mockResolvedValue(undefined),
  gitPull: vi.fn().mockResolvedValue(undefined),
  gitFetch: vi.fn().mockResolvedValue(undefined),
}));

describe('git-store clearError', () => {
  beforeEach(() => {
    useGitStore.setState({
      error: null,
      collectionPath: null,
      credentials: null,
      remotes: [],
    });
    vi.clearAllMocks();
  });

  it('clearError sets error to null', () => {
    useGitStore.setState({ error: 'previous error' });
    useGitStore.getState().clearError();
    expect(useGitStore.getState().error).toBeNull();
  });

  it('push clears stale error before executing', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockResolvedValueOnce(undefined);

    useGitStore.setState({
      error: 'stale error',
      collectionPath: '/test/repo',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().push();

    expect(useGitStore.getState().error).toBeNull();
  });

  it('pull clears stale error before executing', async () => {
    const { gitPull } = await import('@/lib/tauri-api');
    vi.mocked(gitPull).mockResolvedValueOnce(undefined);

    useGitStore.setState({
      error: 'stale error',
      collectionPath: '/test/repo',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().pull();

    expect(useGitStore.getState().error).toBeNull();
  });

  it('fetch clears stale error before executing', async () => {
    const { gitFetch } = await import('@/lib/tauri-api');
    vi.mocked(gitFetch).mockResolvedValueOnce(undefined);

    useGitStore.setState({
      error: 'stale error',
      collectionPath: '/test/repo',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().fetch();

    expect(useGitStore.getState().error).toBeNull();
  });

  it('push sets error when operation fails', async () => {
    const { gitPush } = await import('@/lib/tauri-api');
    vi.mocked(gitPush).mockRejectedValueOnce(new Error('NotFastForward'));

    useGitStore.setState({
      collectionPath: '/test/repo',
      credentials: { type: 'sshAgent' },
      remotes: [{ name: 'origin', url: 'git@github.com:test/repo.git' }],
    });

    await useGitStore.getState().push();

    expect(useGitStore.getState().error).toContain('NotFastForward');
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
yarn test src/stores/__tests__/git-store.test.ts --run
```

Expected: FAIL — `clearError is not a function` (the action doesn't exist yet).

- [ ] **Step 3: Add clearError to the GitState interface**

In `src/stores/git-store.ts`, find the interface block that ends with:

```ts
  push: (remote?: string) => Promise<void>;
  pull: (remote?: string) => Promise<void>;
  fetch: (remote?: string) => Promise<void>;
  reset: () => void;
}
```

Add `clearError` before `reset`:

```ts
  push: (remote?: string) => Promise<void>;
  pull: (remote?: string) => Promise<void>;
  fetch: (remote?: string) => Promise<void>;
  clearError: () => void;
  reset: () => void;
}
```

- [ ] **Step 4: Add clearError implementation and update push/pull/fetch**

In `src/stores/git-store.ts`, find the `push` implementation and replace the three operations (`push`, `pull`, `fetch`) with:

```ts
  // Push local commits to the remote, prompting for credentials if needed.
  push: async (remote) => {
    const { collectionPath, credentials } = get();
    if (!collectionPath) return;
    if (!credentials) { set({ showCredentialsDialog: true }); return; }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitPush(collectionPath, resolvedRemote, credentials);
      await get().refreshStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Pull remote commits into the current branch, prompting for credentials if needed.
  pull: async (remote) => {
    const { collectionPath, credentials } = get();
    if (!collectionPath) return;
    if (!credentials) { set({ showCredentialsDialog: true }); return; }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitPull(collectionPath, resolvedRemote, credentials);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  // Fetch remote refs without merging, prompting for credentials if needed.
  fetch: async (remote) => {
    const { collectionPath, credentials } = get();
    if (!collectionPath) return;
    if (!credentials) { set({ showCredentialsDialog: true }); return; }
    const resolvedRemote = remote ?? get().remotes[0]?.name;
    set({ error: null });
    try {
      await gitFetch(collectionPath, resolvedRemote, credentials);
      await get().refreshStatus();
      await get().refreshBranches();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearError: () => set({ error: null }),
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
yarn test src/stores/__tests__/git-store.test.ts --run
```

Expected: all 5 tests pass.

- [ ] **Step 6: Run the full test suite**

```bash
yarn test --run
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/stores/git-store.ts src/stores/__tests__/git-store.test.ts
git commit -m "feat: add clearError action and clear stale error before push/pull/fetch"
```

---

## Task 2: GitLandingPanel — Error Alert, Conflict Push Guard, lastFetched After Pull

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx`

- [ ] **Step 1: Update the store destructure**

In `src/components/git/GitLandingPanel.tsx`, line 30, change:

```tsx
  const { status, push, pull, fetch, saveStash, popStash } = useGitStore();
```

to:

```tsx
  const { status, push, pull, fetch, saveStash, popStash, error, clearError } = useGitStore();
```

- [ ] **Step 2: Add hasConflicts derived value**

After the existing derived values at line 151–153:

```ts
  const ahead = status?.ahead ?? 0;
  const behind = status?.behind ?? 0;
  const isUpToDate = (status?.isClean ?? false) && ahead === 0 && behind === 0;
```

Add:

```ts
  const hasConflicts = (status?.files.some((f) => f.status === "conflicted")) ?? false;
```

- [ ] **Step 3: Set lastFetched after every pull path**

There are three handlers that call `pull()`: `handlePull`, `handlePullAnyway`, and `handleStashAndPull`. Add `setLastFetched(new Date().toLocaleTimeString())` after each `await pull()` call.

**`handlePull`** — change:
```ts
    setPulling(true);
    try {
      await pull();
    } finally {
      setPulling(false);
    }
```
to:
```ts
    setPulling(true);
    try {
      await pull();
      setLastFetched(new Date().toLocaleTimeString());
    } finally {
      setPulling(false);
    }
```

**`handlePullAnyway`** — change:
```ts
    setPulling(true);
    try {
      await pull();
    } finally {
      setPulling(false);
    }
```
to:
```ts
    setPulling(true);
    try {
      await pull();
      setLastFetched(new Date().toLocaleTimeString());
    } finally {
      setPulling(false);
    }
```

**`handleStashAndPull`** — change:
```ts
    try {
      await saveStash("Auto-stash before pull");
      await pull();
      await popStash(0);
    } catch {
```
to:
```ts
    try {
      await saveStash("Auto-stash before pull");
      await pull();
      setLastFetched(new Date().toLocaleTimeString());
      await popStash(0);
    } catch {
```

- [ ] **Step 4: Disable Push when conflicts exist**

Find the Push button (around line 236):

```tsx
            <Button
              variant={ahead > 0 ? "default" : "outline"}
              size="sm"
              className="flex-1"
              onClick={handlePush}
              disabled={pushing}
            >
```

Change `disabled={pushing}` to `disabled={pushing || hasConflicts}`:

```tsx
            <Button
              variant={ahead > 0 ? "default" : "outline"}
              size="sm"
              className="flex-1"
              onClick={handlePush}
              disabled={pushing || hasConflicts}
            >
```

- [ ] **Step 5: Add the inline error alert**

After the closing `</div>` of the Fetch/Pull/Push button row (around line 250, after the `</div>` that wraps the three buttons), add the error alert inside `<CardContent>`:

```tsx
          {/* Inline error alert for failed push/pull/fetch operations. */}
          {error && (
            <div className="flex items-start gap-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive">
              <AlertCircle className="h-3.5 w-3.5 shrink-0 mt-0.5" />
              <span className="flex-1 break-words">{error}</span>
              <button
                className="shrink-0 hover:opacity-70 leading-none"
                onClick={clearError}
              >
                ×
              </button>
            </div>
          )}
```

`AlertCircle` is already imported at line 10 of the file.

- [ ] **Step 6: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 7: Run the full test suite**

```bash
yarn test --run
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "feat: show push/pull/fetch errors inline, disable push during merge conflict"
```
