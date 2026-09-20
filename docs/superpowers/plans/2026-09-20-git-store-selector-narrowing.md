# Zustand Selector Narrowing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `BranchSelector`, `GitCommitLog`, `GitRemotesDialog`, and `GitStashSection` from subscribing to the entire Git store — a project hard rule violation (`CLAUDE.md:91`: *"Zustand: never fully destructure store state at component top level"*) that also means any unrelated store field change (a credential-dialog field, an error from a different view, a status refresh) rerenders these components even though they only read a handful of fields.

**Architecture:** Most Git components already follow the correct convention — `GitFileList`, `GitCommitForm`, `GitPanel`, `GitCredentialsDialog`, `GitLandingPanel`, and `ConflictResolver` all call `useGitStore((state) => state.someField)` once per field/action they need. Four files still call `useGitStore((state) => state)` and destructure the whole state object: `BranchSelector.tsx`, `GitCommitLog.tsx`, `GitRemotesDialog.tsx`, and `GitStashSection.tsx`. This plan converts each to the same one-selector-per-field style already used elsewhere, with no behavior change — action function references are stable across `set()` calls in this store (the vanilla store's action closures are defined once and never replaced by a partial `set()` patch), so splitting them into individual selectors changes nothing about when an action itself is "fresh"; it only stops the component from re-rendering when a field it doesn't use changes.

**Tech Stack:** React 18, TypeScript, Zustand, Vitest, React Testing Library, React's built-in `Profiler` for render-count assertions.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-10 "Full-store Zustand subscriptions violate project rules and amplify rerenders", `CLAUDE.md:91`). Verified present in current `src/components/git/BranchSelector.tsx:17-26`, `src/components/git/GitCommitLog.tsx:28`, `src/components/git/GitRemotesDialog.tsx:16-18`, `src/components/git/GitStashSection.tsx:47-59`. The other six files listed in the original review (`ConflictResolver.tsx`, `GitCommitForm.tsx`, `GitCredentialsDialog.tsx`, `GitFileList.tsx`, `GitLandingPanel.tsx`, `GitPanel.tsx`) already use narrow per-field selectors in the current codebase — this plan does not touch them.

**Depends on:** `docs/superpowers/plans/2026-09-20-branch-selector-result-handling.md` (adds `clearError` to `BranchSelector`'s destructure) and `docs/superpowers/plans/2026-09-20-git-remotes-dialog-failure-preserves-state.md` (adds `error`/`clearError` to `GitRemotesDialog`'s destructure) having already been applied, so the field lists below are complete. If either has not been applied yet, narrow whatever fields the current destructure actually contains — the mechanical conversion is the same either way.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-raw-controls-list-rows.md` next (plan 13 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- Zustand: never fully destructure store state at component top level (`CLAUDE.md:91`) — this is the rule this plan brings these four files into compliance with.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Narrow `BranchSelector` and `GitCommitLog`

**Files:**
- Modify: `src/components/git/BranchSelector.tsx:17-26`
- Modify: `src/components/git/GitCommitLog.tsx:28`
- Test: `src/components/git/__tests__/BranchSelector.test.tsx`, `src/components/git/__tests__/GitCommitLog.test.tsx` (new file)

**Interfaces:**
- Produces: no new exports, no behavior change. Purely a subscription-shape change.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitCommitLog.test.tsx` (new file — `GitCommitLog` has no existing test file):

```tsx
import { Profiler, type ProfilerOnRenderCallback } from 'react';
import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GitCommitLog } from '@/components/git/GitCommitLog';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

describe('GitCommitLog store subscription', () => {
  it('does not rerender when an unrelated store field changes', () => {
    const store = createGitStore();
    store.setState({
      commitLog: [
        {
          id: 'abc1234',
          fullId: 'abc1234full',
          message: 'initial commit',
          author: 'Test',
          authorEmail: 'test@test.com',
          timestamp: new Date().toISOString(),
          filesChanged: 1,
        },
      ],
    });
    const onRender = vi.fn<ProfilerOnRenderCallback>();

    render(
      <GitStoreProvider store={store}>
        <Profiler id='commit-log' onRender={onRender}>
          <GitCommitLog onCommitClick={() => {}} />
        </Profiler>
      </GitStoreProvider>,
    );
    expect(screen.getByText('initial commit')).toBeInTheDocument();
    const rendersAfterMount = onRender.mock.calls.length;

    act(() => {
      store.setState({ showCredentialsDialog: true });
    });

    expect(onRender.mock.calls.length).toBe(rendersAfterMount);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitCommitLog.test.tsx`
Expected: FAIL — `useGitStore((state) => state)` subscribes to every field, so `showCredentialsDialog` changing triggers a rerender of `GitCommitLog`.

- [ ] **Step 3: Narrow `GitCommitLog`'s subscription**

In `src/components/git/GitCommitLog.tsx`, replace:

```tsx
  const { commitLog, refreshLog } = useGitStore((state) => state);
```

with:

```tsx
  const commitLog = useGitStore((state) => state.commitLog);
  const refreshLog = useGitStore((state) => state.refreshLog);
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitCommitLog.test.tsx`
Expected: PASS

- [ ] **Step 5: Narrow `BranchSelector`'s subscription**

In `src/components/git/BranchSelector.tsx`, replace:

```tsx
  const {
    branches,
    switchBranch,
    createBranch,
    deleteBranch,
    mergeBranch,
    checkoutRemoteBranch,
    status,
    clearError,
  } = useGitStore((state) => state);
```

with:

```tsx
  const branches = useGitStore((state) => state.branches);
  const switchBranch = useGitStore((state) => state.switchBranch);
  const createBranch = useGitStore((state) => state.createBranch);
  const deleteBranch = useGitStore((state) => state.deleteBranch);
  const mergeBranch = useGitStore((state) => state.mergeBranch);
  const checkoutRemoteBranch = useGitStore((state) => state.checkoutRemoteBranch);
  const status = useGitStore((state) => state.status);
  const clearError = useGitStore((state) => state.clearError);
```

(If `docs/superpowers/plans/2026-09-20-branch-selector-result-handling.md` has not yet been applied, omit the `clearError`/`const clearError = ...` line — narrow only the fields actually present in the current destructure.)

- [ ] **Step 6: Run the full BranchSelector suite and typecheck**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx src/components/git/__tests__/GitCommitLog.test.tsx`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/BranchSelector.tsx src/components/git/GitCommitLog.tsx src/components/git/__tests__/GitCommitLog.test.tsx
```

Commit message: `refactor(git): narrow BranchSelector/GitCommitLog Zustand subscriptions to individual fields`.

---

### Task 2: Narrow `GitRemotesDialog` and `GitStashSection`

**Files:**
- Modify: `src/components/git/GitRemotesDialog.tsx`
- Modify: `src/components/git/GitStashSection.tsx:47-59`
- Test: `src/components/git/__tests__/GitStashSection.test.tsx` (new file)

**Interfaces:**
- Produces: no new exports, no behavior change.

- [ ] **Step 1: Write the failing test**

Create `src/components/git/__tests__/GitStashSection.test.tsx`:

```tsx
import { Profiler, type ProfilerOnRenderCallback } from 'react';
import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GitStashSection } from '@/components/git/GitStashSection';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

describe('GitStashSection store subscription', () => {
  it('does not rerender when an unrelated store field changes', () => {
    const store = createGitStore();
    store.setState({
      stashes: [
        {
          index: 0,
          message: 'wip',
          timestamp: new Date().toISOString(),
          filesChanged: 1,
          insertions: 1,
          deletions: 0,
          changedFiles: ['a.txt'],
          branch: 'main',
        },
      ],
    });
    const onRender = vi.fn<ProfilerOnRenderCallback>();

    render(
      <GitStoreProvider store={store}>
        <Profiler id='stash-section' onRender={onRender}>
          <GitStashSection />
        </Profiler>
      </GitStoreProvider>,
    );
    expect(screen.getByText('wip')).toBeInTheDocument();
    const rendersAfterMount = onRender.mock.calls.length;

    act(() => {
      store.setState({ showCredentialsDialog: true });
    });

    expect(onRender.mock.calls.length).toBe(rendersAfterMount);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx`
Expected: FAIL

- [ ] **Step 3: Narrow `GitStashSection`'s subscription**

In `src/components/git/GitStashSection.tsx`, replace:

```tsx
  const {
    stashes,
    saveStash,
    popStash,
    applyStash,
    dropStash,
    applyStashMany,
    popStashMany,
    dropStashMany,
    error,
    clearError,
  } = useGitStore((state) => state);
```

with:

```tsx
  const stashes = useGitStore((state) => state.stashes);
  const saveStash = useGitStore((state) => state.saveStash);
  const popStash = useGitStore((state) => state.popStash);
  const applyStash = useGitStore((state) => state.applyStash);
  const dropStash = useGitStore((state) => state.dropStash);
  const applyStashMany = useGitStore((state) => state.applyStashMany);
  const popStashMany = useGitStore((state) => state.popStashMany);
  const dropStashMany = useGitStore((state) => state.dropStashMany);
  const error = useGitStore((state) => state.error);
  const clearError = useGitStore((state) => state.clearError);
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx`
Expected: PASS

- [ ] **Step 5: Narrow `GitRemotesDialog`'s subscription**

In `src/components/git/GitRemotesDialog.tsx`, replace:

```tsx
  const { remotes, addRemote, removeRemote, setRemoteUrl, refreshRemotes, error, clearError } =
    useGitStore((state) => state);
```

with:

```tsx
  const remotes = useGitStore((state) => state.remotes);
  const addRemote = useGitStore((state) => state.addRemote);
  const removeRemote = useGitStore((state) => state.removeRemote);
  const setRemoteUrl = useGitStore((state) => state.setRemoteUrl);
  const refreshRemotes = useGitStore((state) => state.refreshRemotes);
  const error = useGitStore((state) => state.error);
  const clearError = useGitStore((state) => state.clearError);
```

(If `docs/superpowers/plans/2026-09-20-git-remotes-dialog-failure-preserves-state.md` has not yet been applied, omit the `error`/`clearError` lines — narrow only the fields actually present in the current destructure.)

- [ ] **Step 6: Run the full suite and typecheck**

Run: `yarn test src/components/git`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitRemotesDialog.tsx src/components/git/GitStashSection.tsx src/components/git/__tests__/GitStashSection.test.tsx
```

Commit message: `refactor(git): narrow GitRemotesDialog/GitStashSection Zustand subscriptions to individual fields`.
