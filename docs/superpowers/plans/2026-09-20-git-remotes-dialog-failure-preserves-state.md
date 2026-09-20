# GitRemotesDialog Failure Handling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A failed add/edit/delete remote operation must not silently clear the user's input or exit edit/delete-confirmation mode — and the dialog currently shows no error at all, so a failure is invisible.

**Architecture:** `git-store.ts`'s `addRemote`, `setRemoteUrl`, and `removeRemote` never throw — each catches its own errors and writes them into the shared `error` field, clearing `error` only implicitly (they don't clear it before the call, unlike the network actions). `GitRemotesDialog.tsx`'s `handleAdd`/`handleSaveEdit`/`handleConfirmDelete` all `await` their store action and then unconditionally reset the local UI (clear the name/url fields, exit edit mode, exit delete-confirmation) with no error ever rendered. This plan adds an `error`/`clearError` read to the dialog, clears the store's error explicitly before each mutating call (since, unlike the network actions, these three don't clear it themselves), and only resets local UI state when that specific call left no error — the same clear-before/check-after idiom already used in `GitStashSection.tsx`.

**Tech Stack:** React 18, TypeScript, Zustand, Vitest, React Testing Library.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-03 — specifically: *"A failed remote add clears both fields; a failed edit/delete exits the editing/confirmation state."*, refs `GitRemotesDialog.tsx:37-53`). Verified present in current `src/components/git/GitRemotesDialog.tsx:39-55`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-branch-selector-result-handling.md` next (plan 9 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only. The new error banner uses a `<span role='alert'>` plus shadcn `Button` for dismiss, matching the pattern used in `GitCommitForm`/`GitStashSection`.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: `addRemote` failure preserves the name/url fields and shows the error

**Files:**
- Modify: `src/components/git/GitRemotesDialog.tsx`
- Test: `src/components/git/__tests__/GitRemotesDialog.test.tsx` (new file)

**Interfaces:**
- Consumes: `useGitStore((state) => state)` destructure already present in this file (`remotes, addRemote, removeRemote, setRemoteUrl, refreshRemotes`) — this task adds `error` and `clearError` to that same destructure. (A later, separate plan converts this file's full-store subscription to narrow selectors — see `docs/superpowers/plans/2026-09-20-git-store-selector-narrowing.md` — so this task intentionally keeps the existing destructure style rather than partially narrowing it.)
- Produces: no new exports. Dialog renders an inline error banner; `handleAdd` only clears `newName`/`newUrl` on success.

- [ ] **Step 1: Write the failing test**

Create `src/components/git/__tests__/GitRemotesDialog.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitRemotesDialog } from '@/components/git/GitRemotesDialog';
import { createGitStore } from '@/stores/git-store';
import { GitStoreProvider } from '@/stores/git-store-context';

function renderDialog(store: ReturnType<typeof createGitStore>) {
  return render(
    <GitStoreProvider store={store}>
      <GitRemotesDialog open onOpenChange={() => {}} />
    </GitStoreProvider>,
  );
}

describe('GitRemotesDialog failure handling', () => {
  it('keeps the name/url fields and shows the error when adding a remote fails', async () => {
    const store = createGitStore();
    store.setState({
      remotes: [],
      refreshRemotes: vi.fn().mockResolvedValue(undefined),
      addRemote: async () => {
        store.setState({ error: 'remote origin already exists' });
      },
    });
    renderDialog(store);
    const user = userEvent.setup();

    await user.type(screen.getByPlaceholderText('name'), 'origin');
    await user.type(screen.getByPlaceholderText('https://github.com/...'), 'https://example.com/repo.git');
    await user.click(screen.getByRole('button', { name: /add/i }));

    expect(await screen.findByText('remote origin already exists')).toBeInTheDocument();
    expect(screen.getByPlaceholderText('name')).toHaveValue('origin');
    expect(screen.getByPlaceholderText('https://github.com/...')).toHaveValue('https://example.com/repo.git');
  });

  it('clears the fields when adding a remote succeeds', async () => {
    const store = createGitStore();
    store.setState({
      remotes: [],
      refreshRemotes: vi.fn().mockResolvedValue(undefined),
      addRemote: async () => {
        store.setState({ error: null });
      },
    });
    renderDialog(store);
    const user = userEvent.setup();

    await user.type(screen.getByPlaceholderText('name'), 'origin');
    await user.type(screen.getByPlaceholderText('https://github.com/...'), 'https://example.com/repo.git');
    await user.click(screen.getByRole('button', { name: /add/i }));

    expect(await screen.findByPlaceholderText('name')).toHaveValue('');
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitRemotesDialog.test.tsx`
Expected: FAIL — no error text is rendered anywhere, and `handleAdd` clears the fields unconditionally.

- [ ] **Step 3: Implement the fix**

In `src/components/git/GitRemotesDialog.tsx`, add `X` to the lucide-react import and `error`/`clearError` to the destructure:

```tsx
import { Check, Pencil, Plus, Trash2, X } from 'lucide-react';
```

```tsx
  const { remotes, addRemote, removeRemote, setRemoteUrl, refreshRemotes, error, clearError } =
    useGitStore((state) => state);
```

`GitRemotesDialog` currently only imports the reactive `useGitStore` hook, not the imperative store api needed to read the post-call error without re-rendering on every store change. Add `useGitStoreApi`:

```tsx
import { useGitStore, useGitStoreApi } from '@/stores/git-store-context';
```

```tsx
export function GitRemotesDialog({ open, onOpenChange }: Props) {
  const { remotes, addRemote, removeRemote, setRemoteUrl, refreshRemotes, error, clearError } =
    useGitStore((state) => state);
  const gitStoreApi = useGitStoreApi();
```

Update `handleAdd` to clear the error before the call and only reset the fields when that call left no error:

```tsx
  const handleAdd = async () => {
    clearError();
    await addRemote(newName.trim(), newUrl.trim());
    if (!gitStoreApi.getState().error) {
      setNewName('');
      setNewUrl('');
    }
  };
```

Add the error banner inside the dialog content, above the remotes list:

```tsx
        <TooltipProvider delayDuration={300}>
          <div className='space-y-3 min-w-0'>
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
            {remotes.length === 0 ? (
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitRemotesDialog.test.tsx`
Expected: PASS

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitRemotesDialog.tsx src/components/git/__tests__/GitRemotesDialog.test.tsx
```

Commit message: `fix(git): preserve remote-add fields and surface the error on failure`.

---

### Task 2: `setRemoteUrl` failure preserves edit mode and shows the error

**Files:**
- Modify: `src/components/git/GitRemotesDialog.tsx`
- Test: `src/components/git/__tests__/GitRemotesDialog.test.tsx`

**Interfaces:**
- Consumes: same `error`/`clearError`/`gitStoreApi` wired up in Task 1.
- Produces: no new exports.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitRemotesDialog.test.tsx`:

```tsx
  it('stays in edit mode and shows the error when saving a remote URL fails', async () => {
    const store = createGitStore();
    store.setState({
      remotes: [{ name: 'origin', url: 'https://old.example.com/repo.git' }],
      refreshRemotes: vi.fn().mockResolvedValue(undefined),
      setRemoteUrl: async () => {
        store.setState({ error: 'invalid remote URL' });
      },
    });
    renderDialog(store);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /pencil|edit/i }));
    const urlInput = screen.getByDisplayValue('https://old.example.com/repo.git');
    await user.clear(urlInput);
    await user.type(urlInput, 'not-a-url');
    await user.keyboard('{Enter}');

    expect(await screen.findByText('invalid remote URL')).toBeInTheDocument();
    expect(screen.getByDisplayValue('not-a-url')).toBeInTheDocument();
  });
```

The pencil/edit button in `GitRemotesDialog` currently has no `aria-label` — if `getByRole('button', { name: /pencil|edit/i })` cannot find it, use `screen.getAllByRole('button')` and select the one whose child is the `Pencil` icon (e.g. query by `container.querySelector` as a fallback), or — preferably — add `aria-label='Edit remote'` to that button as part of this task's implementation step below, then query by that label directly: `screen.getByRole('button', { name: 'Edit remote' })`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitRemotesDialog.test.tsx -t "stays in edit mode"`
Expected: FAIL — `handleSaveEdit` exits edit mode unconditionally, so `urlInput` (an `<Input>` in edit mode) is gone and the assertions fail.

- [ ] **Step 3: Implement the fix**

Update `handleSaveEdit`:

```tsx
  const handleSaveEdit = async () => {
    if (!editingRemote) return;
    clearError();
    await setRemoteUrl(editingRemote, editUrl.trim());
    if (!gitStoreApi.getState().error) {
      setEditingRemote(null);
    }
  };
```

Add an accessible label to the edit trigger button so the test (and screen-reader users) can address it directly:

```tsx
                        <Button
                          size='sm'
                          variant='ghost'
                          className='h-7 w-7 p-0'
                          aria-label='Edit remote'
                          onClick={() => {
                            setEditingRemote(remote.name);
                            setEditUrl(remote.url);
                          }}
                        >
                          <Pencil className='h-3.5 w-3.5' />
                        </Button>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitRemotesDialog.test.tsx`
Expected: PASS (all tests so far)

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitRemotesDialog.tsx src/components/git/__tests__/GitRemotesDialog.test.tsx
```

Commit message: `fix(git): preserve remote edit state and surface the error on failure`.

---

### Task 3: `removeRemote` failure preserves the confirmation prompt and shows the error

**Files:**
- Modify: `src/components/git/GitRemotesDialog.tsx`
- Test: `src/components/git/__tests__/GitRemotesDialog.test.tsx`

**Interfaces:**
- Consumes: same `error`/`clearError`/`gitStoreApi` wired up in Task 1.
- Produces: no new exports.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/GitRemotesDialog.test.tsx`:

```tsx
  it('stays in delete-confirmation mode and shows the error when removing a remote fails', async () => {
    const store = createGitStore();
    store.setState({
      remotes: [{ name: 'origin', url: 'https://example.com/repo.git' }],
      refreshRemotes: vi.fn().mockResolvedValue(undefined),
      removeRemote: async () => {
        store.setState({ error: 'could not remove remote' });
      },
    });
    renderDialog(store);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Delete remote' }));
    await user.click(screen.getByRole('button', { name: /^remove$/i }));

    expect(await screen.findByText('could not remove remote')).toBeInTheDocument();
    expect(screen.getByText(/Remove/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^remove$/i })).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitRemotesDialog.test.tsx -t "stays in delete-confirmation mode"`
Expected: FAIL — the delete trigger button currently has no accessible name (`getByRole('button', { name: 'Delete remote' })` won't find it), and `handleConfirmDelete` exits confirmation mode unconditionally.

- [ ] **Step 3: Implement the fix**

Add an accessible label to the delete trigger button:

```tsx
                        <Button
                          size='sm'
                          variant='ghost'
                          className='h-7 w-7 p-0 text-destructive hover:text-destructive'
                          aria-label='Delete remote'
                          onClick={() => setDeletingRemote(remote.name)}
                        >
                          <Trash2 className='h-3.5 w-3.5' />
                        </Button>
```

Update `handleConfirmDelete`:

```tsx
  const handleConfirmDelete = async () => {
    if (!deletingRemote) return;
    clearError();
    await removeRemote(deletingRemote);
    if (!gitStoreApi.getState().error) {
      setDeletingRemote(null);
    }
  };
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitRemotesDialog.test.tsx`
Expected: PASS (all tests in the file)

- [ ] **Step 5: Run typecheck and the broader git component suite**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn test src/components/git`
Expected: PASS

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/GitRemotesDialog.tsx src/components/git/__tests__/GitRemotesDialog.test.tsx
```

Commit message: `fix(git): preserve remote delete confirmation and surface the error on failure`.
