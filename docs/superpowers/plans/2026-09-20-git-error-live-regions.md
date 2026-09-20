# Error Live-Regions and Loading Announcements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make error banners announce themselves to assistive technology when they appear, and give in-flight operation buttons a programmatic busy state.

**Architecture:** Only `GitCredentialsDialog`'s save-error paragraph currently has `role='alert'` (`src/components/git/GitCredentialsDialog.tsx:145`); every other destructive-error banner in the Git panel (`GitStashSection`, `GitLandingPanel`, `ConflictResolver`'s two banners, `BranchSelector`'s `switchError`/`createError`) is a plain `<div>` that a screen reader has no reason to announce when it appears mid-session, since it isn't part of the initial render the user already navigated to. Adding `role='alert'` (an implicit ARIA live region with `assertive` politeness) is the standard fix and matches the one banner in this codebase that already does it correctly. Separately, buttons that show a spinner and swap their label to a present-participle ("Committing...", "Pushing...") don't reliably get that change announced — `aria-busy='true'` on the button while the operation is in flight is the minimal, additive fix (no new visually-hidden live-region component, consistent with this codebase not having one already).

**Tech Stack:** React 18, TypeScript, Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-12 — specifically: *"Error banners generally lack `role=\"alert\"`/live-region behavior; only the credential save error does this correctly."*, *"Loading indicators often lack a programmatic status message."*). Verified present in current `src/components/git/GitStashSection.tsx:160-165`, `src/components/git/GitLandingPanel.tsx:290-304`, `src/components/git/ConflictResolver.tsx:143-156,189-202`, `src/components/git/BranchSelector.tsx:119-125,254-259`.

**Depends on:** `docs/superpowers/plans/2026-09-20-git-raw-controls-list-rows.md` (Task 3, dismiss-button conversion) — the exact JSX for `GitLandingPanel`'s and `ConflictResolver`'s error banners shown below assumes that plan's `Button`-based dismiss control. If it hasn't been applied yet, add `role='alert'` to the same outer `<div>` regardless — the dismiss control's markup is unrelated to this change.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-accessible-names-and-keyboard.md` next (plan 16 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: `role='alert'` on every destructive error banner

**Files:**
- Modify: `src/components/git/GitStashSection.tsx:160-165`
- Modify: `src/components/git/GitLandingPanel.tsx:290-304`
- Modify: `src/components/git/ConflictResolver.tsx:143-156,189-202`
- Modify: `src/components/git/BranchSelector.tsx:119-125,254-259`
- Test: `src/components/git/__tests__/GitStashSection.test.tsx`, `src/components/git/__tests__/BranchSelector.test.tsx`

**Interfaces:**
- Produces: no new exports, no visual change — `role='alert'` is announced by assistive technology only.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitStashSection.test.tsx`:

```tsx
  it('announces the error banner as an alert', () => {
    const store = createGitStore();
    store.setState({ error: 'could not save stash' });
    render(
      <GitStoreProvider store={store}>
        <GitStashSection />
      </GitStoreProvider>,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('could not save stash');
  });
```

Add to `src/components/git/__tests__/BranchSelector.test.tsx`:

```tsx
  it('announces the branch-switch error banner as an alert', async () => {
    const store = renderWithStore({
      switchBranch: async () => {
        store.setState({ error: 'checkout failed' });
      },
      branches: {
        current: 'main',
        local: [
          { name: 'main', isHead: true, isRemote: false },
          { name: 'develop', isHead: false, isRemote: false },
        ],
        remote: [],
      },
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /main/ }));
    await user.click(screen.getByText('develop'));

    expect(await screen.findByRole('alert')).toHaveTextContent('checkout failed');
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx src/components/git/__tests__/BranchSelector.test.tsx`
Expected: FAIL — `screen.getByRole('alert')` finds nothing (a plain `<div>` has no implicit `alert` role).

- [ ] **Step 3: Add `role='alert'` to each banner**

In `src/components/git/GitStashSection.tsx`:

```tsx
      {error && (
        <div
          role='alert'
          className='mx-3 mb-2 flex items-start gap-1.5 rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-1.5 text-xs text-destructive'
        >
          <AlertCircle className='mt-px h-3 w-3 shrink-0' />
          <span className='break-all leading-relaxed'>{error}</span>
        </div>
      )}
```

In `src/components/git/GitLandingPanel.tsx`:

```tsx
          {error && (
            <div
              role='alert'
              className='flex items-start gap-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'
            >
              <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
              <span className='flex-1 wrap-break-word'>{error}</span>
```

(Leave the dismiss control after the `<span>` exactly as-is — this task only adds `role='alert'` to the outer `<div>`.)

In `src/components/git/ConflictResolver.tsx`, both occurrences:

```tsx
        {error && (
          <div
            role='alert'
            className='flex items-start gap-2 mx-3 mt-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'
          >
            <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
            <span className='flex-1 wrap-break-word'>{error}</span>
```

In `src/components/git/BranchSelector.tsx`:

```tsx
        {switchError && (
          <div
            role='alert'
            className='flex items-start gap-1.5 px-2 py-1.5 text-xs text-destructive border-b border-border/70'
          >
            <AlertCircle className='h-3 w-3 shrink-0 mt-0.5' />
            <span className='wrap-break-word'>{switchError}</span>
          </div>
        )}
```

```tsx
          {createError && (
            <div role='alert' className='flex items-start gap-1.5 text-xs text-destructive'>
              <AlertCircle className='h-3 w-3 shrink-0 mt-0.5' />
              <span className='wrap-break-word'>{createError}</span>
            </div>
          )}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx src/components/git/__tests__/BranchSelector.test.tsx`
Expected: PASS

- [ ] **Step 5: Run the full git component suite and typecheck**

Run: `yarn test src/components/git`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitStashSection.tsx src/components/git/GitLandingPanel.tsx src/components/git/ConflictResolver.tsx src/components/git/BranchSelector.tsx src/components/git/__tests__/GitStashSection.test.tsx src/components/git/__tests__/BranchSelector.test.tsx
```

Commit message: `fix(git): announce error banners as alerts to assistive technology`.

---

### Task 2: `aria-busy` on in-flight operation buttons

**Files:**
- Modify: `src/components/git/GitCommitForm.tsx`
- Modify: `src/components/git/GitLandingPanel.tsx`
- Modify: `src/components/git/GitStashSection.tsx`
- Test: `src/components/git/__tests__/GitCommitForm.test.tsx`, `src/components/git/__tests__/GitLandingPanel.test.tsx`, `src/components/git/__tests__/GitStashSection.test.tsx`

**Interfaces:**
- Produces: no new exports, no new state — `aria-busy` is bound directly to each button's existing local busy boolean (`committing`, `fetching`/`pulling`/`pushing`, `isSaving`).

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitCommitForm.test.tsx`:

```tsx
  it('marks the commit button as busy while committing', async () => {
    const deferred = createDeferred<void>();
    renderForm(() => deferred.promise);
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Commit message'), 'fix: broken thing');
    const commitButton = screen.getByRole('button', { name: /commit 1 file/i });
    await user.click(commitButton);

    expect(commitButton).toHaveAttribute('aria-busy', 'true');
    deferred.resolve();
    await vi.waitFor(() => expect(commitButton).toHaveAttribute('aria-busy', 'false'));
  });
```

Add `import { createDeferred } from '@/test/deferred';` to the test file.

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitCommitForm.test.tsx -t "marks the commit button as busy"`
Expected: FAIL — the Commit button has no `aria-busy` attribute.

- [ ] **Step 3: Add `aria-busy` to the Commit button**

In `src/components/git/GitCommitForm.tsx`:

```tsx
        <Button
          onClick={handleCommit}
          disabled={!message.trim() || stagedCount === 0 || committing}
          aria-busy={committing}
          className='w-full'
          size='sm'
        >
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitCommitForm.test.tsx`
Expected: PASS

- [ ] **Step 5: Add `aria-busy` to Fetch/Pull/Push in `GitLandingPanel.tsx`**

Add a test to `src/components/git/__tests__/GitLandingPanel.test.tsx`:

```tsx
  it('marks Fetch as busy while a fetch is in flight', async () => {
    const deferred = createDeferred<void>();
    const store = createGitStore();
    store.setState({
      credentials: { type: 'token', token: 'tok' },
      status: { branch: 'main', files: [], ahead: 0, behind: 0, isClean: true },
      fetch: () => deferred.promise,
    });
    render(
      <GitStoreProvider store={store}>
        <GitLandingPanel />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();
    const fetchButton = screen.getByRole('button', { name: /^fetch$/i });
    await user.click(fetchButton);

    expect(fetchButton).toHaveAttribute('aria-busy', 'true');
    deferred.resolve();
    await vi.waitFor(() => expect(fetchButton).toHaveAttribute('aria-busy', 'false'));
  });
```

Add `import { createDeferred } from '@/test/deferred';` to that test file too, if not already present from an earlier plan.

Implement in `src/components/git/GitLandingPanel.tsx`:

```tsx
            <Button
              variant='outline'
              size='sm'
              className='flex-1'
              onClick={handleFetch}
              disabled={fetching}
              aria-busy={fetching}
            >
```

```tsx
            <Button
              variant='outline'
              size='sm'
              className='flex-1'
              onClick={handlePull}
              disabled={pulling}
              aria-busy={pulling}
            >
```

```tsx
            <Button
              variant={ahead > 0 ? 'default' : 'outline'}
              size='sm'
              className='flex-1'
              onClick={handlePush}
              disabled={pushing || hasConflicts}
              aria-busy={pushing}
            >
```

Run: `yarn test src/components/git/__tests__/GitLandingPanel.test.tsx`
Expected: PASS

- [ ] **Step 6: Add `aria-busy` to the Stash save button in `GitStashSection.tsx`**

Add a test to `src/components/git/__tests__/GitStashSection.test.tsx`:

```tsx
  it('marks the Stash button as busy while saving', async () => {
    const deferred = createDeferred<void>();
    const store = createGitStore();
    store.setState({ saveStash: () => deferred.promise });
    render(
      <GitStoreProvider store={store}>
        <GitStashSection />
      </GitStoreProvider>,
    );
    const user = userEvent.setup();
    await user.type(screen.getByLabelText('Stash message'), 'wip');
    const stashButton = screen.getByRole('button', { name: /stash/i });
    await user.click(stashButton);

    expect(stashButton).toHaveAttribute('aria-busy', 'true');
    deferred.resolve();
    await vi.waitFor(() => expect(stashButton).toHaveAttribute('aria-busy', 'false'));
  });
```

Implement in `src/components/git/GitStashSection.tsx`:

```tsx
        <Button
          variant='outline'
          size='sm'
          className='h-7 px-2.5 text-xs shrink-0 gap-1'
          onClick={() => void handleSave()}
          disabled={!message.trim() || isSaving}
          aria-busy={isSaving}
        >
```

- [ ] **Step 7: Run the full git component suite, typecheck, and lint**

Run: `yarn test src/components/git`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn check`
Expected: no new findings

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitCommitForm.tsx src/components/git/GitLandingPanel.tsx src/components/git/GitStashSection.tsx src/components/git/__tests__/GitCommitForm.test.tsx src/components/git/__tests__/GitLandingPanel.test.tsx src/components/git/__tests__/GitStashSection.test.tsx
```

Commit message: `fix(git): mark commit/fetch/pull/push/stash buttons as aria-busy while in flight`.
