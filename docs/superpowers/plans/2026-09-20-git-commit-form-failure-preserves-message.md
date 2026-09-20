# GitCommitForm Failure Handling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A failed commit must not silently clear the commit message the user typed, and the user must be told the commit failed.

**Architecture:** `git-store.ts`'s `commitChanges` never throws — on failure it catches the error internally and writes it to the shared `error` field, then resolves normally. `GitCommitForm.tsx`'s `doCommit` awaits `commitChanges(...)` and unconditionally calls `setMessage('')` afterward, so a failed commit clears the user's message exactly as if it had succeeded, with no error shown anywhere in the form. This plan applies the same "clear the shared error before the call, check it after" idiom already used successfully in `GitStashSection.tsx` (`handleSave`/`handleApplyMany`/etc.) — that pattern is the established convention in this codebase for turning the store's shared `error` field into a reliable per-call result without changing every store action's return type. It also adds a minimal inline error banner so the user actually sees why the commit didn't go through, matching the existing banner style used elsewhere in the Git panel (`GitStashSection`, `GitLandingPanel`, `ConflictResolver`).

**Tech Stack:** React 18, TypeScript, Zustand, Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-03 "`Promise<void>` actions swallow failures, so callers cannot know whether work succeeded" — specifically: *"A failed commit still clears the user's commit message."*, ref `GitCommitForm.tsx:17-24`). Verified present in current `src/components/git/GitCommitForm.tsx:20-28`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-landing-panel-workflow-guards.md` next (plan 7 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only — no raw `<button>`/`<input>`/etc. This plan only touches existing `Textarea`/`Button` usage plus a text/icon error banner matching the existing pattern (a `<span>` + shadcn `Button` for dismiss, not a raw `<button>`).
- Zustand: never fully destructure store state at component top level — this plan adds two more narrow `useGitStore((state) => state.x)` selectors, consistent with `GitCommitForm`'s existing style.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Preserve the commit message and surface the error on a failed commit

**Files:**
- Modify: `src/components/git/GitCommitForm.tsx`
- Test: `src/components/git/__tests__/GitCommitForm.test.tsx` (new file)

**Interfaces:**
- Consumes: `useGitStore((s) => s.error)`, `useGitStore((s) => s.clearError)` (already exported on `GitState`, unused by this file today).
- Produces: no new exports. `GitCommitForm` renders an inline destructive error banner when `store.error` is set, using shadcn `Button` (`variant='ghost' size='icon'`) with a `lucide-react` `X` icon for dismiss, matching `GitStashSection`'s banner markup.

- [ ] **Step 1: Write the failing test**

Create `src/components/git/__tests__/GitCommitForm.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitCommitForm } from '@/components/git/GitCommitForm';
import * as tauriApi from '@/lib/tauri-api';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    gitGetIdentity: vi.fn().mockResolvedValue({ name: 'Test', email: 'test@example.com' }),
  };
});

function renderForm(commitChanges: (message: string) => Promise<void>) {
  const store = createGitStore();
  store.setState({
    repositoryId: 'repo-1',
    status: { branch: 'main', files: [{ path: 'a.txt', staged: true, status: 'modified' }], ahead: 0, behind: 0, isClean: false },
    commitChanges,
  });
  render(
    <GitStoreProvider store={store}>
      <GitCommitForm />
    </GitStoreProvider>,
  );
  return store;
}

describe('GitCommitForm failure handling', () => {
  it('keeps the typed message and shows the error when commit fails', async () => {
    const store = renderForm(async () => {
      store.setState({ error: 'commit failed: nothing to commit' });
    });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Commit message'), 'fix: broken thing');
    await user.click(screen.getByRole('button', { name: /commit 1 file/i }));

    expect(await screen.findByText('commit failed: nothing to commit')).toBeInTheDocument();
    expect(screen.getByLabelText('Commit message')).toHaveValue('fix: broken thing');
  });

  it('clears the message on a successful commit', async () => {
    const store = renderForm(async () => {
      store.setState({ error: null });
    });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText('Commit message'), 'fix: broken thing');
    await user.click(screen.getByRole('button', { name: /commit 1 file/i }));

    expect(await screen.findByLabelText('Commit message')).toHaveValue('');
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitCommitForm.test.tsx`
Expected: FAIL — the first test fails both on the missing error text (nothing renders it today) and because the message is cleared unconditionally.

- [ ] **Step 3: Implement the fix**

In `src/components/git/GitCommitForm.tsx`, add the two selectors and an `X` icon import:

```tsx
import { Check, Loader2, X } from 'lucide-react';
```

```tsx
  const status = useGitStore((state) => state.status);
  const commitChanges = useGitStore((state) => state.commitChanges);
  const repositoryId = useGitStore((state) => state.repositoryId);
  const error = useGitStore((state) => state.error);
  const clearError = useGitStore((state) => state.clearError);
  const gitStoreApi = useGitStoreApi();
```

Change `doCommit` to clear the shared error before committing and only clear the message when the commit actually succeeded:

```tsx
  const doCommit = async () => {
    setCommitting(true);
    gitStoreApi.setState({ error: null });
    try {
      await commitChanges(message.trim());
      if (!gitStoreApi.getState().error) {
        setMessage('');
      }
    } finally {
      setCommitting(false);
    }
  };
```

Add an error banner above the `Textarea`, matching `GitStashSection`'s banner:

```tsx
      <div className='space-y-2'>
        {error && (
          <div className='flex items-start gap-1.5 rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-1.5 text-xs text-destructive'>
            <span role='alert' className='flex-1 wrap-break-word'>
              {error}
            </span>
            <Button
              variant='ghost'
              size='icon'
              className='h-4 w-4 shrink-0'
              onClick={clearError}
              aria-label='Dismiss error'
            >
              <X className='h-3 w-3' />
            </Button>
          </div>
        )}
        <Textarea
          placeholder='Commit message... (Ctrl+Enter to commit)'
          ...
```

(Keep the rest of the `Textarea`/`Button`/staged-count markup unchanged — only the new banner block is inserted before it.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitCommitForm.test.tsx`
Expected: PASS

- [ ] **Step 5: Typecheck and lint**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn check`
Expected: no new lint findings in `GitCommitForm.tsx`

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitCommitForm.tsx src/components/git/__tests__/GitCommitForm.test.tsx
```

Commit message: `fix(git): preserve commit message and surface the error on a failed commit`.
