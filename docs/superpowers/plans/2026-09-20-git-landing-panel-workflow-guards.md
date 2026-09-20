# GitLandingPanel Workflow Guards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `GitLandingPanel`'s multi-step workflows (fetch-then-push, stash-then-pull) from proceeding to their next step after an earlier step failed, and stop it from recording a "last fetched"/"last synced" timestamp when the underlying fetch or pull actually failed.

**Architecture:** `git-store.ts`'s `fetch`, `pull`, and `push` never throw — each catches its own errors and writes them into the shared `error` field, then resolves normally. Every network action in `git-store.ts` clears `error: null` synchronously before making its IPC call (see `push`/`pull`/`fetch` at `src/stores/git-store.ts:657-717`), so a component can reliably read `store.error` immediately after `await`ing one of these calls to learn whether *that specific call* failed, without needing to change any store action's signature. `GitLandingPanel.tsx` doesn't do this today: `handleFetch`/`handlePull`/`handlePullAnyway` set `lastFetched` unconditionally after `await fetch()`/`await pull()`, and `handleFetchAndPush`/`handleStashAndPull` proceed to their second step (`push()`/`pull()`) even when the first step (`fetch()`/`saveStash()`) failed. This plan applies the store's own clear-before/check-after guarantee at each call site, the same convention already used in `GitStashSection.tsx`.

**Tech Stack:** React 18, TypeScript, Zustand, Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-03 — specifically: *"Fetch and pull update `lastFetched` even when IPC failed."*, *"`handleFetchAndPush` can fetch unsuccessfully and then push anyway because `fetch()` resolves normally."*, *"`handleStashAndPull` can continue to pull after stash save fails, and can pop stash 0 after pull fails."*). Verified present in current `src/components/git/GitLandingPanel.tsx:52-172`.

**Known limitation (documented, not fixed by this plan):** `pull()` and `fetch()` each run follow-up refreshes (`refreshStatus`/`refreshConflicts`/`refreshBranches`) after their own try/catch; a refresh that fails after an otherwise-successful pull/fetch will also leave `store.error` set, which these guards will (correctly, if conservatively) treat as "the operation failed" and halt the workflow. This mirrors the same shared-error-channel tradeoff already accepted by `GitStashSection`'s existing pattern and is out of scope for this plan.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-remotes-dialog-failure-preserves-state.md` next (plan 8 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Only record `lastFetched` on success, and stop fetch-then-push on a failed fetch

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx:52-66` (`handleFetch`), `:68-89` (`handlePull`), `:113-122` (`handlePullAnyway`), `:146-162` (`handleFetchAndPush`)
- Test: `src/components/git/__tests__/GitLandingPanel.test.tsx` (new file)

**Interfaces:**
- Consumes: `gitStoreApi.getState().error` (already exported on `GitState`), `fetch: (remote?: string) => Promise<void>`, `pull: (remote?: string) => Promise<void>`, `push: (remote?: string) => Promise<void>` (unchanged signatures).
- Produces: no new exports — behavior-only change to the four handlers listed above.

- [ ] **Step 1: Write the failing test**

Create `src/components/git/__tests__/GitLandingPanel.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

describe('GitLandingPanel workflow guards', () => {
  it('does not record a fetch timestamp when fetch fails', async () => {
    const store = createGitStore();
    // Mirrors the store's own contract: fetch never throws, it sets `error`.
    const fetch = async () => {
      store.setState({ error: 'network unreachable' });
    };
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 1, behind: 2, isClean: true },
      fetch,
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /^fetch$/i }));

    expect(await screen.findByText('network unreachable')).toBeInTheDocument();
    expect(screen.getByText('Never fetched')).toBeInTheDocument();
  });

  it('does not push after a fetch failure in the fetch-then-push flow', async () => {
    const push = vi.fn().mockResolvedValue(undefined);
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 1, behind: 1, isClean: true },
      push,
      fetch: async () => {
        store.setState({ error: 'auth failed' });
      },
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    // lastFetched is null and behind > 0, so Push opens the fetch-first dialog.
    await user.click(screen.getByRole('button', { name: /^push/i }));
    await user.click(screen.getByRole('button', { name: /fetch & push/i }));

    expect(await screen.findByText('auth failed')).toBeInTheDocument();
    expect(push).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitLandingPanel.test.tsx`
Expected: FAIL — `lastFetched` shows a timestamp instead of `'Never fetched'` in the first test, and `push` is called in the second.

- [ ] **Step 3: Implement the guard**

In `src/components/git/GitLandingPanel.tsx`, update `handleFetch`:

```tsx
  const handleFetch = async () => {
    const { credentials } = gitStoreApi.getState();
    if (!credentials) {
      // Store will open the credentials dialog; skip timestamp update.
      fetch();
      return;
    }
    setFetching(true);
    try {
      await fetch();
      if (!gitStoreApi.getState().error) {
        setLastFetched(new Date().toLocaleTimeString());
      }
    } finally {
      setFetching(false);
    }
  };
```

Update `handlePull`:

```tsx
  const handlePull = async () => {
    const { credentials } = gitStoreApi.getState();
    if (!credentials) {
      pull();
      return;
    }

    // Check if working tree has uncommitted changes.
    const { status: currentStatus } = gitStoreApi.getState();
    if (currentStatus && !currentStatus.isClean) {
      setShowStashDialog(true);
      return;
    }

    setPulling(true);
    try {
      await pull();
      if (!gitStoreApi.getState().error) {
        setLastFetched(new Date().toLocaleTimeString());
      }
    } finally {
      setPulling(false);
    }
  };
```

Update `handlePullAnyway`:

```tsx
  const handlePullAnyway = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try {
      await pull();
      if (!gitStoreApi.getState().error) {
        setLastFetched(new Date().toLocaleTimeString());
      }
    } finally {
      setPulling(false);
    }
  };
```

Update `handleFetchAndPush`:

```tsx
  const handleFetchAndPush = async () => {
    setShowFetchFirstDialog(false);
    setPushing(true);
    try {
      await fetch();
      if (gitStoreApi.getState().error) return;
      setLastFetched(new Date().toLocaleTimeString());
      // Re-check status after fetch — if now behind, abort push.
      const { status: freshStatus } = gitStoreApi.getState();
      if (freshStatus && freshStatus.behind > 0) {
        return;
      }
      await push();
    } finally {
      setPushing(false);
    }
  };
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitLandingPanel.test.tsx`
Expected: PASS

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitLandingPanel.tsx src/components/git/__tests__/GitLandingPanel.test.tsx
```

Commit message: `fix(git): stop fetch-then-push and timestamp updates after a failed fetch/pull`.

---

### Task 2: Stop stash-then-pull from pulling after a failed stash or popping after a failed pull

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx:91-111` (`handleStashAndPull`)
- Test: `src/components/git/__tests__/GitLandingPanel.test.tsx`

**Interfaces:**
- Consumes: `gitStoreApi.getState().error`, `gitStoreApi.getState().hasConflicts()` (unchanged), `saveStash: (message: string) => Promise<void>`, `pull: (remote?: string) => Promise<void>`, `popStash: (index: number) => Promise<void>` (unchanged signatures).
- Produces: no new exports.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitLandingPanel.test.tsx`:

```tsx
  it('does not pull after a failed auto-stash, and does not pop after a failed pull', async () => {
    const pull = vi.fn().mockResolvedValue(undefined);
    const popStash = vi.fn().mockResolvedValue(undefined);
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: false },
      saveStash: async () => {
        store.setState({ error: 'could not create stash' });
      },
      pull,
      popStash,
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^pull/i }));
    await user.click(screen.getByRole('button', { name: /stash & pull/i }));

    expect(await screen.findByText('could not create stash')).toBeInTheDocument();
    expect(pull).not.toHaveBeenCalled();
    expect(popStash).not.toHaveBeenCalled();
  });

  it('does not pop the auto-stash after an outright pull failure', async () => {
    const popStash = vi.fn().mockResolvedValue(undefined);
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: false },
      saveStash: vi.fn().mockResolvedValue(undefined),
      pull: async () => {
        store.setState({ error: 'authentication failed' });
      },
      popStash,
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /^pull/i }));
    await user.click(screen.getByRole('button', { name: /stash & pull/i }));

    expect(await screen.findByText('authentication failed')).toBeInTheDocument();
    expect(popStash).not.toHaveBeenCalled();
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitLandingPanel.test.tsx`
Expected: FAIL — `pull`/`popStash` are called in cases where they must not be.

- [ ] **Step 3: Implement the guard**

In `src/components/git/GitLandingPanel.tsx`, replace `handleStashAndPull`:

```tsx
  const handleStashAndPull = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try {
      await saveStash('Auto-stash before pull');
      if (gitStoreApi.getState().error) {
        // Stash itself failed — nothing changed, nothing to pull or pop.
        return;
      }
      await pull();
      if (gitStoreApi.getState().error) {
        // Pull failed outright (network/auth/etc.) — leave the stash in place
        // rather than popping it on top of an unknown working-tree state.
        return;
      }
      // After pull, check whether it produced merge conflicts.
      // If so, do NOT restore the stash — applying it on top of a conflicted
      // index would corrupt the working tree with doubled conflicts.
      if (gitStoreApi.getState().hasConflicts()) {
        // Leave the stash in place; the user can pop it after resolving conflicts.
        return;
      }
      await popStash(0);
      if (!gitStoreApi.getState().error) {
        setLastFetched(new Date().toLocaleTimeString());
      }
    } finally {
      setPulling(false);
    }
  };
```

Note the removed `catch { ... }` block: `saveStash`/`pull`/`popStash` never throw (they catch internally and write to `error`), so that `catch` was unreachable dead code — the explicit `error` checks above are the real guard.

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitLandingPanel.test.tsx`
Expected: PASS

- [ ] **Step 5: Run the full suite and typecheck**

Run: `yarn test src/components/git`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitLandingPanel.tsx src/components/git/__tests__/GitLandingPanel.test.tsx
```

Commit message: `fix(git): stop stash-then-pull from pulling or popping after a failed step`.
