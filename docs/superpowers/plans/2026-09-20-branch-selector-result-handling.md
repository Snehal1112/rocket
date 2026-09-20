# BranchSelector Result Handling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `BranchSelector`'s fragile "compare the store's `error` string before and after the call" success check with the reliable clear-before/check-after pattern already used elsewhere in the Git panel, and give `deleteBranch` visible error feedback.

**Architecture:** `git-store.ts`'s `createBranch`, `switchBranch`, `checkoutRemoteBranch`, `mergeBranch`, and `deleteBranch` never throw — each catches its own errors and writes them into the shared `error` field. `BranchSelector.tsx` currently infers success by capturing `gitStoreApi.getState().error` *before* the call and comparing it to the value *after* the call (`nextError !== prevError`). This has two real bugs: (1) if the operation fails twice in a row with the identical error message (e.g. the same "branch already exists" error on two consecutive attempts), the second failure is indistinguishable from success and the UI closes/clears as if it worked; (2) an unrelated concurrent error (e.g. a background `refreshStatus` failure) can be misattributed to the branch operation. This plan replaces that comparison with `clearError()` before the call and a plain `if (gitStoreApi.getState().error)` check after — the same idiom already used successfully in `GitStashSection.tsx` — and adds the same treatment to `deleteBranch`, which today has no result handling at all.

**Tech Stack:** React 18, TypeScript, Zustand, Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-03 — specifically: *"Branch handlers compare `nextError !== prevError`; repeating the same failure string is interpreted as success and closes/clears UI. An unrelated concurrent error can be misattributed to the branch operation."*, and the store-action-to-caller trace's `deleteBranch` row: *"Fire-and-forget, no confirmation, no inline result handling."*). Verified present in current `src/components/git/BranchSelector.tsx:43-103,186-190`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-invalidation-fixes.md` next (plan 10 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: `createBranch` — clear-before/check-after

**Files:**
- Modify: `src/components/git/BranchSelector.tsx`
- Test: `src/components/git/__tests__/BranchSelector.test.tsx`

**Interfaces:**
- Consumes: `useGitStore((state) => state)` destructure already present in this file — this task adds `clearError` to it. (A later, separate plan converts this file's full-store subscription to narrow selectors — see `docs/superpowers/plans/2026-09-20-git-store-selector-narrowing.md`.)
- Produces: no new exports.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/BranchSelector.test.tsx` (it already has a `branches` fixture and a `renderWithStore` helper — extend rather than duplicate; change `renderWithStore` to accept a partial state patch instead of only `checkoutRemoteBranch` so this and later tasks can inject different action mocks):

```tsx
function renderWithStore(patch: Partial<Parameters<typeof createGitStore>[0]> & Record<string, unknown>) {
  const store = createGitStore();
  store.setState({ branches, ...patch });
  render(
    <GitStoreProvider store={store}>
      <BranchSelector />
    </GitStoreProvider>,
  );
  return store;
}
```

(This changes the existing helper's signature — update the existing "shows a busy spinner..." test's call site from `renderWithStore(checkoutRemoteBranch)` to `renderWithStore({ checkoutRemoteBranch })` as part of this step.)

Then add:

```tsx
describe('BranchSelector createBranch result handling', () => {
  it('treats a repeated identical error as a failure, not success', async () => {
    const store = renderWithStore({
      createBranch: async () => {
        store.setState({ error: 'branch already exists' });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.type(screen.getByLabelText('New branch name'), 'feature-x');
    await user.click(screen.getByLabelText('Create branch'));

    expect(await screen.findByText('branch already exists')).toBeInTheDocument();
    expect(screen.getByLabelText('New branch name')).toHaveValue('feature-x');

    // Second attempt fails with the exact same message — must still be treated
    // as a failure (this is what the old prevError/nextError comparison got wrong).
    await user.click(screen.getByLabelText('Create branch'));
    expect(await screen.findByText('branch already exists')).toBeInTheDocument();
    expect(screen.getByLabelText('New branch name')).toHaveValue('feature-x');
  });

  it('clears the input on success', async () => {
    const store = renderWithStore({
      createBranch: async () => {
        store.setState({ error: null });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.type(screen.getByLabelText('New branch name'), 'feature-x');
    await user.click(screen.getByLabelText('Create branch'));

    expect(await screen.findByLabelText('New branch name')).toHaveValue('');
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx -t "repeated identical error"`
Expected: FAIL — on the second click, `nextError === prevError` (both `'branch already exists'`), so the current code treats it as success and clears the input.

- [ ] **Step 3: Implement the fix**

In `src/components/git/BranchSelector.tsx`, add `clearError` to the destructure:

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

Replace `handleCreate`:

```tsx
  const handleCreate = async () => {
    if (!newBranchName.trim()) return;
    setCreateError(null);
    clearError();
    await createBranch(newBranchName.trim());
    const nextError = gitStoreApi.getState().error;
    if (nextError) {
      setCreateError(nextError);
    } else {
      setNewBranchName('');
    }
  };
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx`
Expected: PASS (all tests in the file, including the pre-existing remote-checkout test updated in Step 1)

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/BranchSelector.tsx src/components/git/__tests__/BranchSelector.test.tsx
```

Commit message: `fix(git): stop treating a repeated branch-create error as success`.

---

### Task 2: `switchBranch` and `checkoutRemoteBranch` — clear-before/check-after

**Files:**
- Modify: `src/components/git/BranchSelector.tsx`
- Test: `src/components/git/__tests__/BranchSelector.test.tsx`

**Interfaces:**
- Consumes: `clearError` wired up in Task 1.
- Produces: no new exports.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/BranchSelector.test.tsx`:

```tsx
describe('BranchSelector switchBranch result handling', () => {
  it('treats a repeated identical switch error as a failure', async () => {
    const store = renderWithStore({
      switchBranch: async () => {
        store.setState({ error: 'uncommitted changes would be overwritten' });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    // `main` is head; click a non-head local branch row instead — the fixture
    // only has `main` as local, so extend the fixture's local branches for
    // this test via a second store patch.
    store.setState({
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
    });

    await user.click(await screen.findByText('develop'));
    expect(await screen.findByText('uncommitted changes would be overwritten')).toBeInTheDocument();

    await user.click(screen.getByText('develop'));
    expect(await screen.findByText('uncommitted changes would be overwritten')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx -t "repeated identical switch error"`
Expected: FAIL — the second click's `nextError === prevError`, so the popover closes (`setOpen(false)`) instead of showing the error again.

- [ ] **Step 3: Implement the fix**

Replace `handleSwitch`:

```tsx
  const handleSwitch = async (name: string) => {
    setSwitchError(null);
    clearError();
    await switchBranch(name);
    const nextError = gitStoreApi.getState().error;
    if (nextError) {
      setSwitchError(nextError);
    } else {
      setOpen(false);
    }
  };
```

Replace `handleCheckoutRemote`:

```tsx
  const handleCheckoutRemote = async (name: string) => {
    setSwitchError(null);
    setCheckingOutRemote(name);
    try {
      clearError();
      await checkoutRemoteBranch(name);
      const nextError = gitStoreApi.getState().error;
      if (nextError) {
        setSwitchError(nextError);
      } else {
        setOpen(false);
      }
    } finally {
      setCheckingOutRemote(null);
    }
  };
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx`
Expected: PASS

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/BranchSelector.tsx src/components/git/__tests__/BranchSelector.test.tsx
```

Commit message: `fix(git): stop treating a repeated branch-switch error as success`.

---

### Task 3: `mergeBranch`/`deleteBranch` — clear-before/check-after, and give delete visible feedback

**Files:**
- Modify: `src/components/git/BranchSelector.tsx`
- Test: `src/components/git/__tests__/BranchSelector.test.tsx`

**Interfaces:**
- Consumes: `clearError` wired up in Task 1.
- Produces: no new exports. `deleteBranch` now awaits its result and surfaces a failure via the existing `switchError` banner instead of being fire-and-forget with no feedback.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/BranchSelector.test.tsx`:

```tsx
describe('BranchSelector mergeBranch/deleteBranch result handling', () => {
  it('treats a repeated identical merge error as a failure and keeps the popover open', async () => {
    const store = renderWithStore({
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
      mergeBranch: async () => {
        store.setState({ error: 'not something we can merge (unrelated histories)' });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: /merge into current/i }));

    expect(
      await screen.findByText('not something we can merge (unrelated histories)'),
    ).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /merge into current/i }));
    expect(
      await screen.findByText('not something we can merge (unrelated histories)'),
    ).toBeInTheDocument();
  });

  it('shows an error when deleting a branch fails', async () => {
    const store = renderWithStore({
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
      deleteBranch: async () => {
        store.setState({ error: 'cannot delete the currently checked-out branch' });
      },
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByRole('button', { name: /delete branch/i }));

    expect(
      await screen.findByText('cannot delete the currently checked-out branch'),
    ).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx -t "mergeBranch/deleteBranch"`
Expected: FAIL — the repeated-merge-error test fails the same way as Task 2's; the delete test fails because `deleteBranch` is currently invoked without `await` or any error handling (nothing renders the error).

- [ ] **Step 3: Implement the fix**

Replace `handleMerge`:

```tsx
  const handleMerge = async (name: string) => {
    setSwitchError(null);
    clearError();
    await mergeBranch(name);
    const nextError = gitStoreApi.getState().error;
    if (nextError) {
      if (nextError.toLowerCase().includes('conflict')) {
        setOpen(false);
      } else {
        setSwitchError(nextError);
      }
    } else {
      setOpen(false);
    }
  };
```

Add a `handleDelete` handler and use it in place of the inline fire-and-forget call:

```tsx
  const handleDelete = async (name: string) => {
    setSwitchError(null);
    clearError();
    await deleteBranch(name);
    const nextError = gitStoreApi.getState().error;
    if (nextError) {
      setSwitchError(nextError);
    }
  };
```

Update the delete button's `onClick`:

```tsx
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5 text-destructive'
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleDelete(branch.name);
                          }}
                        >
                          <Trash2 className='h-3.5 w-3.5' />
                        </Button>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx`
Expected: PASS (all tests in the file)

- [ ] **Step 5: Run typecheck and the broader git component suite**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn test src/components/git`
Expected: PASS

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/BranchSelector.tsx src/components/git/__tests__/BranchSelector.test.tsx
```

Commit message: `fix(git): stop misreading repeated merge errors as success and surface delete failures`.
