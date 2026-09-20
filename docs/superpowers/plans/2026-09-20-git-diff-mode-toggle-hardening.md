# Diff Mode and Toggle Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Validate the persisted diff-mode preference instead of blindly casting it, fix a stale comment that misdescribes the visual-mode file-type gate, and make the working/staged toggle resilient to rapid clicking (disable while in flight, ignore a stale response that resolves after a newer toggle started, and surface a failed toggle instead of silently keeping old content with no feedback).

**Architecture:** `DiffViewer.tsx` reads `localStorage.getItem('git-diff-mode')` and casts it straight to `'text' | 'visual'` with no runtime check — a corrupted or manually-edited value silently becomes an invalid `mode` the rest of the component doesn't expect. Its `canShowVisual` gate is `diffState.filePath.endsWith('.yml')`, but the comment above it says "Visual mode is only available for JSON request files," which no longer matches (collections moved to YAML). `handleToggleStaged` has no loading state, no disabling of the Working/Staged control, and no way to detect that a slower response is stale — clicking Working → Staged → Working quickly can let the first (now-stale) Staged response overwrite the second, newer Working response if it resolves later, and any fetch failure is silently swallowed with the old content kept and no error shown.

**Tech Stack:** React 18, TypeScript, Vitest, React Testing Library, `@/test/deferred`.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-15 "Diff mode and toggle requests have weak validation/error semantics"). Verified present in current `src/components/git/DiffViewer.tsx:60-90`, `src/components/git/DiffHeader.tsx:29-59`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-store-network-and-cleanup.md` next (plan 19 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- All UI components use shadcn/ui primitives only — this plan adds a `disabled` prop to the existing shadcn `Tabs`/`TabsTrigger` usage in `DiffHeader`, not a new control.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Validate the persisted diff mode and fix the stale visual-mode comment

**Files:**
- Modify: `src/components/git/DiffViewer.tsx:60-63,89-90`
- Test: `src/components/git/__tests__/DiffViewer.test.tsx` (check first with `ls src/components/git/__tests__/ | grep DiffViewer` — create it if it doesn't exist)

**Interfaces:**
- Produces: no new exports, no behavior change for a valid stored value — only changes what happens when `localStorage['git-diff-mode']` holds anything other than exactly `'text'` or `'visual'`.

- [ ] **Step 1: Write the failing test**

Create (or extend) `src/components/git/__tests__/DiffViewer.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { DiffViewer } from '@/components/git/DiffViewer';
import type { DiffState } from '@/types/pane-types';

const diffState: DiffState = {
  filePath: 'collection.yml',
  repositoryId: 'repo-1',
  repositoryLabel: 'Repo',
  oldContent: 'old',
  newContent: 'new',
  status: 'modified',
  isStaged: false,
};

describe('DiffViewer persisted mode validation', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('falls back to text mode when the stored value is invalid', () => {
    localStorage.setItem('git-diff-mode', 'not-a-real-mode');
    render(<DiffViewer diffState={diffState} />);
    expect(screen.getByRole('tab', { name: 'Text' })).toHaveAttribute('data-state', 'active');
  });

  it('honors a valid stored value', () => {
    localStorage.setItem('git-diff-mode', 'visual');
    render(<DiffViewer diffState={diffState} />);
    expect(screen.getByRole('tab', { name: 'Visual' })).toHaveAttribute('data-state', 'active');
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/DiffViewer.test.tsx -t "falls back to text mode"`
Expected: FAIL — `mode` becomes the literal string `'not-a-real-mode'`, so neither the `'text'` nor `'visual'` branch renders as active and `canShowVisual` (true for `.yml`) means the mode-switch `Tabs` exists but its `value` prop doesn't match either `TabsTrigger`, leaving Radix Tabs with no active tab.

- [ ] **Step 3: Implement the fix**

In `src/components/git/DiffViewer.tsx`, replace:

```tsx
  // Persist mode preference across sessions.
  const [mode, setMode] = useState<'text' | 'visual'>(() => {
    return (localStorage.getItem('git-diff-mode') as 'text' | 'visual') ?? 'text';
  });
```

with:

```tsx
  // Persist mode preference across sessions. Validate the stored value —
  // it's user/session-editable localStorage, not a value this code controls.
  const [mode, setMode] = useState<'text' | 'visual'>(() => {
    const stored = localStorage.getItem('git-diff-mode');
    return stored === 'text' || stored === 'visual' ? stored : 'text';
  });
```

Fix the stale comment:

```tsx
  // Visual mode is only available for .yml collection files.
  const canShowVisual = diffState.filePath.endsWith('.yml');
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/DiffViewer.test.tsx`
Expected: PASS

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/DiffViewer.tsx src/components/git/__tests__/DiffViewer.test.tsx
```

Commit message: `fix(git): validate the persisted diff-mode preference and fix a stale comment`.

---

### Task 2: Guard the working/staged toggle against stale responses and surface failures

**Files:**
- Modify: `src/components/git/DiffViewer.tsx`
- Modify: `src/components/git/DiffHeader.tsx`
- Test: `src/components/git/__tests__/DiffViewer.test.tsx`

**Interfaces:**
- Produces: `DiffHeaderProps` gains an optional `stageToggleDisabled?: boolean`, applied to both `TabsTrigger`s in the Working/Staged `Tabs`. `DiffViewer` gains local state `toggling: boolean` and `toggleError: string | null`, and a `toggleRequestIdRef` to discard a response that resolves after a newer toggle has already started.

- [ ] **Step 1: Write the failing test**

Add to `src/components/git/__tests__/DiffViewer.test.tsx`:

```tsx
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, gitDiff: vi.fn(), gitDiffStaged: vi.fn() };
});

describe('DiffViewer toggle resilience', () => {
  it('keeps the newer response when an older toggle resolves out of order', async () => {
    const deferredStaged = createDeferred<{ oldContent: string; newContent: string }>();
    const deferredWorking = createDeferred<{ oldContent: string; newContent: string }>();
    vi.mocked(tauriApi.gitDiffStaged).mockReturnValue(deferredStaged.promise);
    vi.mocked(tauriApi.gitDiff).mockReturnValue(deferredWorking.promise);

    render(<DiffViewer diffState={{ ...diffState, filePath: 'plain.ts' }} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: 'Staged' }));
    await user.click(screen.getByRole('tab', { name: 'Working' }));

    // The newer (working) request resolves first, the stale (staged) request
    // resolves after — the stale one must be discarded.
    deferredWorking.resolve({ oldContent: 'old-working', newContent: 'new-working' });
    await vi.waitFor(() => expect(screen.getByRole('tab', { name: 'Working' })).toHaveAttribute('data-state', 'active'));
    deferredStaged.resolve({ oldContent: 'old-staged', newContent: 'new-staged' });
    await Promise.resolve();

    expect(screen.getByRole('tab', { name: 'Working' })).toHaveAttribute('data-state', 'active');
  });

  it('disables the toggle while a request is in flight and shows an error on failure', async () => {
    const deferred = createDeferred<{ oldContent: string; newContent: string }>();
    vi.mocked(tauriApi.gitDiffStaged).mockReturnValue(deferred.promise);

    render(<DiffViewer diffState={{ ...diffState, filePath: 'plain.ts' }} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole('tab', { name: 'Staged' }));
    expect(screen.getByRole('tab', { name: 'Working' })).toBeDisabled();

    deferred.reject(new Error('diff unavailable'));
    expect(await screen.findByText(/diff unavailable/)).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Working' })).not.toBeDisabled();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/DiffViewer.test.tsx -t "toggle resilience"`
Expected: FAIL — there is no request-id guard, so the stale staged response overwrites the newer working one; there is no `disabled` state and no rendered error text.

- [ ] **Step 3: Implement the guard**

In `src/components/git/DiffHeader.tsx`, add the new prop:

```tsx
interface DiffHeaderProps {
  diffState: DiffState;
  onToggleStaged: (isStaged: boolean) => void;
  mode: 'text' | 'visual';
  onModeChange: (mode: 'text' | 'visual') => void;
  canShowVisual: boolean;
  hideStageToggle?: boolean;
  stageToggleDisabled?: boolean;
}
```

```tsx
export function DiffHeader({
  diffState,
  onToggleStaged,
  mode,
  onModeChange,
  canShowVisual,
  hideStageToggle = false,
  stageToggleDisabled = false,
}: DiffHeaderProps) {
```

```tsx
        {!hideStageToggle && (
          <Tabs
            value={diffState.isStaged ? 'staged' : 'working'}
            onValueChange={(v) => onToggleStaged(v === 'staged')}
          >
            <TabsList className='h-6'>
              <TabsTrigger value='working' className='text-xs px-2 py-0.5' disabled={stageToggleDisabled}>
                Working
              </TabsTrigger>
              <TabsTrigger value='staged' className='text-xs px-2 py-0.5' disabled={stageToggleDisabled}>
                Staged
              </TabsTrigger>
            </TabsList>
          </Tabs>
        )}
```

In `src/components/git/DiffViewer.tsx`, add the guard state (`useRef`/`useState` are already imported):

```tsx
  const [toggling, setToggling] = useState(false);
  const [toggleError, setToggleError] = useState<string | null>(null);
  const toggleRequestIdRef = useRef(0);

  const handleToggleStaged = useCallback(
    async (isStaged: boolean) => {
      const myRequestId = ++toggleRequestIdRef.current;
      setToggling(true);
      setToggleError(null);
      try {
        const diff = isStaged
          ? await gitDiffStaged(diffState.repositoryId, diffState.filePath)
          : await gitDiff(diffState.repositoryId, diffState.filePath);
        if (toggleRequestIdRef.current !== myRequestId) return;
        setDiffState((prev) => ({
          ...prev,
          oldContent: diff.oldContent ?? '',
          newContent: diff.newContent ?? '',
          isStaged,
        }));
      } catch (e) {
        if (toggleRequestIdRef.current !== myRequestId) return;
        setToggleError(String(e));
      } finally {
        if (toggleRequestIdRef.current === myRequestId) setToggling(false);
      }
    },
    [diffState.repositoryId, diffState.filePath],
  );
```

Pass the new prop and render the error banner:

```tsx
      <DiffHeader
        diffState={diffState}
        onToggleStaged={handleToggleStaged}
        mode={mode}
        onModeChange={handleModeChange}
        canShowVisual={canShowVisual}
        hideStageToggle={hideStageToggle}
        stageToggleDisabled={toggling}
      />
      {toggleError && (
        <div role='alert' className='px-3 py-1.5 text-xs text-destructive border-b bg-destructive/10'>
          {toggleError}
        </div>
      )}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/DiffViewer.test.tsx`
Expected: PASS (all tests in the file)

- [ ] **Step 5: Run the full git component suite and typecheck**

Run: `yarn test src/components/git`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/git/DiffViewer.tsx src/components/git/DiffHeader.tsx src/components/git/__tests__/DiffViewer.test.tsx
```

Commit message: `fix(git): discard stale diff-toggle responses and surface toggle failures`.
