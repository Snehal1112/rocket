# Raw Control Cleanup — List Rows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace remaining raw `<button>` elements in Git list rows and dismiss controls with the shadcn `Button` primitive, per the project's hard rule (`CLAUDE.md:87`: *"no raw `<button>`, `<input>`, `<dialog>`, `<select>`, or `<form>`"*).

**Architecture:** This is a source-convention violation, not a runtime-observable bug — a raw `<button>` and a shadcn `Button` both render an actual `<button>` DOM element with identical accessibility semantics, so there is no jsdom assertion that would fail before this change and pass after it. Several places in this same codebase already nest a shadcn `Button` inside a `role='button'` row (e.g. `BranchSelector`'s per-row Merge/Delete buttons, with a `biome-ignore` comment explaining why the *outer* element can't also be a real `<button>` — WebKitGTK breaks hover tracking when a `<button>` nests inside another `<button>`), so nesting `Button` here is a proven-safe pattern, not a new risk. Each task below converts a specific raw `<button>` to `Button` with matching Tailwind classes (`variant='ghost'` and a `font-normal h-auto justify-start` combination reproduces the original unstyled-row look), verified by grep (confirming the tag is gone) and by the file's existing functional test suite (confirming the click behavior is unchanged).

**Tech Stack:** React 18, TypeScript, shadcn/ui `Button`, Biome (`yarn check`).

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-11 "Raw interactive controls violate the shadcn-only hard rule", `CLAUDE.md:87`). Verified present in current `src/components/git/BranchSelector.tsx:208`, `src/components/git/GitCloneDialog.tsx:206`, `src/components/git/CommitDiffView.tsx:62`, `src/components/git/GitCommitLog.tsx:63`, `src/components/git/ConflictResolver.tsx:147,193`, `src/components/git/GitLandingPanel.tsx:295`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-raw-controls-checkbox-close.md` next (plan 14 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only — no raw `<button>`/`<input>`/`<dialog>`/`<select>`/`<form>`. This plan exists to bring the files below into compliance.
- Icons: `lucide-react` only.
- Commits use conventional commits format.
- `yarn tsc --noEmit`, `yarn check`, and `yarn test <pattern>` must pass before each commit.

---

### Task 1: `BranchSelector` remote-branch row and `GitCloneDialog` collection-picker row

**Files:**
- Modify: `src/components/git/BranchSelector.tsx:207-224`
- Modify: `src/components/git/GitCloneDialog.tsx:204-220`
- Test: `src/components/git/__tests__/BranchSelector.test.tsx` (existing test already exercises the remote row)

**Interfaces:**
- Produces: no new exports, no behavior change. `Button` is already imported in both files.

- [ ] **Step 1: Confirm the current violation**

Run: `grep -n "<button" src/components/git/BranchSelector.tsx src/components/git/GitCloneDialog.tsx`
Expected: matches at `BranchSelector.tsx:208` and `GitCloneDialog.tsx:206`.

- [ ] **Step 2: Convert the remote-branch row in `BranchSelector.tsx`**

Replace:

```tsx
                return (
                  <button
                    key={branch.name}
                    type='button'
                    disabled={checkingOutRemote !== null}
                    className='flex w-full items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm text-left disabled:opacity-50 disabled:cursor-not-allowed'
                    onClick={() => {
                      void handleCheckoutRemote(branch.name);
                    }}
                  >
                    {isCheckingOutThis ? (
                      <Loader2 className='w-3.5 h-3.5 animate-spin shrink-0' />
                    ) : (
                      <span className='w-3.5' />
                    )}
                    <span className='truncate flex-1 text-muted-foreground'>{localName}</span>
                  </button>
                );
```

with:

```tsx
                return (
                  <Button
                    key={branch.name}
                    type='button'
                    variant='ghost'
                    disabled={checkingOutRemote !== null}
                    className='flex w-full h-auto items-center gap-1.5 rounded px-2 py-1 justify-start font-normal hover:bg-muted/50 text-sm text-left disabled:opacity-50 disabled:cursor-not-allowed'
                    onClick={() => {
                      void handleCheckoutRemote(branch.name);
                    }}
                  >
                    {isCheckingOutThis ? (
                      <Loader2 className='w-3.5 h-3.5 animate-spin shrink-0' />
                    ) : (
                      <span className='w-3.5' />
                    )}
                    <span className='truncate flex-1 text-muted-foreground'>{localName}</span>
                  </Button>
                );
```

- [ ] **Step 3: Convert the collection-picker row in `GitCloneDialog.tsx`**

Replace:

```tsx
                {collections.map((col) => (
                  <button
                    key={col.path}
                    type='button'
                    className='flex items-center gap-2 px-2 py-1.5 rounded hover:bg-muted/50 cursor-pointer text-sm w-full text-left'
                    onClick={() => setSelectedCollection(col.path)}
                  >
                    <Check
                      className='h-3 w-3 shrink-0'
                      style={{
                        opacity: selectedCollection === col.path ? 1 : 0,
                      }}
                    />
                    <span className='truncate'>{col.name}</span>
                  </button>
                ))}
```

with:

```tsx
                {collections.map((col) => (
                  <Button
                    key={col.path}
                    type='button'
                    variant='ghost'
                    className='flex items-center gap-2 px-2 py-1.5 h-auto rounded justify-start font-normal hover:bg-muted/50 text-sm w-full text-left'
                    onClick={() => setSelectedCollection(col.path)}
                  >
                    <Check
                      className='h-3 w-3 shrink-0'
                      style={{
                        opacity: selectedCollection === col.path ? 1 : 0,
                      }}
                    />
                    <span className='truncate'>{col.name}</span>
                  </Button>
                ))}
```

- [ ] **Step 4: Verify the tag is gone and behavior is unchanged**

Run: `grep -n "<button" src/components/git/BranchSelector.tsx src/components/git/GitCloneDialog.tsx`
Expected: no matches.

Run: `yarn test src/components/git/__tests__/BranchSelector.test.tsx`
Expected: PASS — the existing "shows a busy spinner and disables the row while checkout is in flight" test still passes unchanged, confirming the click/disabled behavior is preserved.

- [ ] **Step 5: Typecheck and lint**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn check`
Expected: no new findings in either file

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/BranchSelector.tsx src/components/git/GitCloneDialog.tsx
```

Commit message: `refactor(git): replace raw list-row buttons with shadcn Button`.

---

### Task 2: `CommitDiffView` file row and `GitCommitLog` SHA-copy button

**Files:**
- Modify: `src/components/git/CommitDiffView.tsx:61-73`
- Modify: `src/components/git/GitCommitLog.tsx:62-77`
- Test: none of these currently have a component test exercising the click; add a minimal one for `CommitDiffView` and rely on manual verification for `GitCommitLog`'s copy button (its only effect is a clipboard write, already outside this plan's scope to test).

**Interfaces:**
- Produces: no new exports, no behavior change. `CommitDiffView.tsx` does not currently import `Button` — add the import as part of this task.

- [ ] **Step 1: Confirm the current violation**

Run: `grep -n "<button" src/components/git/CommitDiffView.tsx src/components/git/GitCommitLog.tsx`
Expected: matches at `CommitDiffView.tsx:62` and `GitCommitLog.tsx:63`.

- [ ] **Step 2: Convert the file row in `CommitDiffView.tsx`**

Add the import:

```tsx
import { Button } from '@/components/ui/button';
```

Replace:

```tsx
            {diffs.map((diff) => (
              <button
                key={diff.path}
                type='button'
                className={`w-full flex items-center gap-1.5 px-2 py-1 rounded text-left text-sm hover:bg-muted/50 ${
                  selectedPath === diff.path ? 'bg-muted/70' : ''
                }`}
                onClick={() => setSelectedPath(diff.path)}
              >
                <GitStatusBadge status={fileStatus(diff)} />
                <span className='truncate flex-1 text-xs font-mono'>{diff.path}</span>
              </button>
            ))}
```

with:

```tsx
            {diffs.map((diff) => (
              <Button
                key={diff.path}
                type='button'
                variant='ghost'
                className={`w-full h-auto flex items-center gap-1.5 px-2 py-1 rounded justify-start font-normal text-left text-sm hover:bg-muted/50 ${
                  selectedPath === diff.path ? 'bg-muted/70' : ''
                }`}
                onClick={() => setSelectedPath(diff.path)}
              >
                <GitStatusBadge status={fileStatus(diff)} />
                <span className='truncate flex-1 text-xs font-mono'>{diff.path}</span>
              </Button>
            ))}
```

- [ ] **Step 3: Convert the SHA-copy button in `GitCommitLog.tsx`**

Replace:

```tsx
                  <button
                    type='button'
                    className='shrink-0 cursor-pointer font-mono text-[10px] px-1 py-0.5 bg-muted rounded text-muted-foreground hover:text-foreground transition-colors'
                    onClick={(e) => {
                      e.stopPropagation();
                      navigator.clipboard.writeText(commit.fullId);
                    }}
                  >
                    {commit.id}
                  </button>
```

with:

```tsx
                  <Button
                    type='button'
                    variant='ghost'
                    size='sm'
                    className='shrink-0 h-auto px-1 py-0.5 font-mono text-[10px] bg-muted rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors'
                    onClick={(e) => {
                      e.stopPropagation();
                      navigator.clipboard.writeText(commit.fullId);
                    }}
                  >
                    {commit.id}
                  </Button>
```

- [ ] **Step 4: Add a minimal click-behavior test for `CommitDiffView`**

Create `src/components/git/__tests__/CommitDiffView.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { CommitDiffView } from '@/components/git/CommitDiffView';

describe('CommitDiffView file row', () => {
  it('selects a file and shows its diff when clicked', async () => {
    render(
      <CommitDiffView
        diffs={[
          { path: 'a.txt', oldContent: 'old-a', newContent: 'new-a' },
          { path: 'b.txt', oldContent: 'old-b', newContent: 'new-b' },
        ]}
        repositoryId='repo-1'
        repositoryLabel='Repo'
      />,
    );
    const user = userEvent.setup();

    expect(screen.getByRole('button', { name: /a\.txt/ })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /b\.txt/ }));
    expect(screen.getByRole('button', { name: /b\.txt/ })).toHaveClass('bg-muted/70');
  });
});
```

- [ ] **Step 5: Run the tests and verify the tags are gone**

Run: `yarn test src/components/git/__tests__/CommitDiffView.test.tsx`
Expected: PASS

Run: `grep -n "<button" src/components/git/CommitDiffView.tsx src/components/git/GitCommitLog.tsx`
Expected: no matches.

- [ ] **Step 6: Typecheck and lint**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn check`
Expected: no new findings

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/CommitDiffView.tsx src/components/git/GitCommitLog.tsx src/components/git/__tests__/CommitDiffView.test.tsx
```

Commit message: `refactor(git): replace raw commit-diff row and SHA-copy buttons with shadcn Button`.

---

### Task 3: Dismiss-error buttons in `ConflictResolver` and `GitLandingPanel`

**Files:**
- Modify: `src/components/git/ConflictResolver.tsx:143-155,189-201`
- Modify: `src/components/git/GitLandingPanel.tsx:290-304`
- Test: `src/components/git/__tests__/ConflictResolver.test.tsx`

**Interfaces:**
- Produces: no new exports, no behavior change. Both files add `X` to their existing `lucide-react` import; both already import `Button`.

- [ ] **Step 1: Confirm the current violation**

Run: `grep -n "shrink-0 hover:opacity-70 leading-none" src/components/git/ConflictResolver.tsx src/components/git/GitLandingPanel.tsx`
Expected: two matches in `ConflictResolver.tsx` (manual-mode and normal-mode error banners) and one in `GitLandingPanel.tsx`.

- [ ] **Step 2: Convert both dismiss buttons in `ConflictResolver.tsx`**

Update the import:

```tsx
import { AlertCircle, X } from 'lucide-react';
```

Replace each occurrence of (there are two, identical, one in the manual-mode branch and one in the normal-mode branch):

```tsx
            <button
              type='button'
              className='shrink-0 hover:opacity-70 leading-none'
              onClick={clearError}
              aria-label='Dismiss error'
            >
              ×
            </button>
```

with:

```tsx
            <Button
              variant='ghost'
              size='icon'
              className='h-4 w-4 shrink-0'
              onClick={clearError}
              aria-label='Dismiss error'
            >
              <X className='h-3 w-3' />
            </Button>
```

- [ ] **Step 3: Convert the dismiss button in `GitLandingPanel.tsx`**

Update the import:

```tsx
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  Check,
  Clock,
  GitBranch,
  GitCommit,
  KeyRound,
  Loader2,
  RefreshCw,
  X,
} from 'lucide-react';
```

Replace:

```tsx
              <button
                type='button'
                className='shrink-0 hover:opacity-70 leading-none'
                onClick={clearError}
                aria-label='Dismiss error'
              >
                ×
              </button>
```

with:

```tsx
              <Button
                variant='ghost'
                size='icon'
                className='h-4 w-4 shrink-0'
                onClick={clearError}
                aria-label='Dismiss error'
              >
                <X className='h-3 w-3' />
              </Button>
```

- [ ] **Step 4: Run the existing ConflictResolver suite and verify the tags are gone**

Run: `yarn test src/components/git/__tests__/ConflictResolver.test.tsx`
Expected: PASS — its dismiss-button interactions (queried by `aria-label='Dismiss error'`, unchanged) still work.

Run: `grep -n "shrink-0 hover:opacity-70 leading-none" src/components/git/ConflictResolver.tsx src/components/git/GitLandingPanel.tsx`
Expected: no matches.

- [ ] **Step 5: Typecheck, lint, and the full git component suite**

Run: `yarn tsc --noEmit`
Expected: no errors

Run: `yarn check`
Expected: no new findings

Run: `yarn test src/components/git`
Expected: PASS

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/ConflictResolver.tsx src/components/git/GitLandingPanel.tsx
```

Commit message: `refactor(git): replace raw dismiss-error buttons with shadcn Button`.
