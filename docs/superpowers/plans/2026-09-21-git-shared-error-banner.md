# Git Shared Error Banner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the duplicated inline error-banner markup in `ConflictResolver` (two instances), `GitLandingPanel`, and `GitStashSection` with one shared, tested `GitErrorBanner` component.

**Architecture:** Add a minimal shadcn-style `Alert` primitive (`src/components/ui/alert.tsx`, `cva`-based like `button.tsx`/`badge.tsx`) that owns the `role='alert'` + destructive-color box styling. On top of it, add a Git-domain `GitErrorBanner` (`src/components/git/GitErrorBanner.tsx`) that renders the icon + message + optional dismiss button — the exact shape every duplicated banner already uses. Replace the four cited instances with it. This plan intentionally does not touch `GitCommitForm`, `BranchSelector`, or `GitRemotesDialog`: the source spec's dup-obs #2 finding cites only `ConflictResolver.tsx` (both banners), `GitLandingPanel.tsx`, and `GitStashSection.tsx`; `BranchSelector`'s two banners are a structurally different, non-dismissible top-divider style rather than the boxed-card pattern being consolidated here, and widening scope to components the finding didn't name would be an undocumented visual/behavioral change outside this plan's traceable basis.

**Tech Stack:** React 18 + TypeScript, `class-variance-authority` (matching existing `src/components/ui/*` primitives), Vitest + @testing-library/react + @testing-library/user-event.

**Spec:** docs/reports/git-integration-review/02-frontend-architecture.md ("Duplicate logic and boundary observations", item #2: "Global error banners and dismiss controls are duplicated in ConflictResolver.tsx:67-80, ConflictResolver.tsx:111-124, GitLandingPanel.tsx:291-305, and GitStashSection.tsx:158-164. A shared alert component would also eliminate raw dismiss buttons and normalize live-region behavior.")

## Global Constraints

- All UI components use shadcn/ui primitives only — this plan adds one (`Alert`), following the exact `cva` + `data-slot` pattern already used by `src/components/ui/button.tsx` and `src/components/ui/badge.tsx` (CLAUDE.md).
- Icons: `lucide-react` only (CLAUDE.md) — reuse the existing `AlertCircle`/`X` icons already used by every banner being replaced.
- Preserve the `role='alert'` semantics and the `aria-label='Dismiss error'` on every dismiss button exactly as today, so no accessibility regression is introduced.
- `GitStashSection`'s banner has no dismiss button today — do not add one; only consolidate the markup.
- Minor visual normalization is expected and acceptable: `GitStashSection`'s banner currently uses slightly smaller padding/gap/icon size (`px-2.5 py-1.5`, `gap-1.5`, `h-3 w-3` icon) than the other three (`px-3 py-2`, `gap-2`, `h-3.5 w-3.5` icon); after this plan all four render with the same padding/gap/icon size as `ConflictResolver`/`GitLandingPanel`. This is the intended effect of "normalize... behavior" from the finding, not an oversight.
- No `ui/*` primitive in this codebase currently has its own test file (`button.tsx`, `badge.tsx` have none) — follow that convention and do not add one for `alert.tsx`; test the domain-level `GitErrorBanner` instead, matching how other `src/components/git/*` components are tested.

---

### Task 1: Create the `Alert` primitive and the `GitErrorBanner` component

**Files:**
- Create: `src/components/ui/alert.tsx`
- Create: `src/components/git/GitErrorBanner.tsx`
- Test: `src/components/git/__tests__/GitErrorBanner.test.tsx`

**Interfaces:**
- Produces: `Alert` component (`{ variant?: 'default' | 'destructive' } & React.ComponentProps<'div'>`, renders `role='alert'`) from `src/components/ui/alert.tsx`. `GitErrorBanner` component (`{ message: string; onDismiss?: () => void; className?: string }`) from `src/components/git/GitErrorBanner.tsx`, used by later tasks in this plan.
- Consumes: `cva`, `cn` from `@/lib/utils`, `AlertCircle`/`X` from `lucide-react`, `Button` from `@/components/ui/button`.

- [ ] **Step 1: Write the failing test**

Create `src/components/git/__tests__/GitErrorBanner.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitErrorBanner } from '@/components/git/GitErrorBanner';

describe('GitErrorBanner', () => {
  it('renders the message in an alert region', () => {
    render(<GitErrorBanner message='something failed' />);

    expect(screen.getByRole('alert')).toHaveTextContent('something failed');
  });

  it('has no dismiss button when onDismiss is not provided', () => {
    render(<GitErrorBanner message='something failed' />);

    expect(screen.queryByRole('button', { name: /dismiss error/i })).not.toBeInTheDocument();
  });

  it('calls onDismiss when the dismiss button is clicked', async () => {
    const onDismiss = vi.fn();
    render(<GitErrorBanner message='something failed' onDismiss={onDismiss} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /dismiss error/i }));

    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/git/__tests__/GitErrorBanner.test.tsx`
Expected: FAIL — cannot find module `@/components/git/GitErrorBanner`.

- [ ] **Step 3: Create the `Alert` primitive**

Create `src/components/ui/alert.tsx`:

```tsx
import { cva, type VariantProps } from 'class-variance-authority';
import type * as React from 'react';
import { cn } from '@/lib/utils';

const alertVariants = cva('relative w-full rounded-md border px-3 py-2 text-xs flex items-start gap-2', {
  variants: {
    variant: {
      default: 'bg-card text-card-foreground border-border',
      destructive: 'border-destructive/30 bg-destructive/10 text-destructive',
    },
  },
  defaultVariants: { variant: 'default' },
});

function Alert({
  className,
  variant,
  ...props
}: React.ComponentProps<'div'> & VariantProps<typeof alertVariants>) {
  return (
    <div
      data-slot='alert'
      role='alert'
      className={cn(alertVariants({ variant, className }))}
      {...props}
    />
  );
}

export { Alert, alertVariants };
```

- [ ] **Step 4: Create `GitErrorBanner`**

Create `src/components/git/GitErrorBanner.tsx`:

```tsx
import { AlertCircle, X } from 'lucide-react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';

interface GitErrorBannerProps {
  message: string;
  onDismiss?: () => void;
  className?: string;
}

// Shared error banner for Git panel views — replaces the near-identical
// inline alert markup previously duplicated across ConflictResolver,
// GitLandingPanel, and GitStashSection (see
// docs/reports/git-integration-review/02-frontend-architecture.md, dup-obs #2).
export function GitErrorBanner({ message, onDismiss, className }: GitErrorBannerProps) {
  return (
    <Alert variant='destructive' className={className}>
      <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
      <span className='flex-1 wrap-break-word'>{message}</span>
      {onDismiss && (
        <Button
          variant='ghost'
          size='icon'
          className='h-4 w-4 shrink-0'
          onClick={onDismiss}
          aria-label='Dismiss error'
        >
          <X className='h-3 w-3' />
        </Button>
      )}
    </Alert>
  );
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `yarn test src/components/git/__tests__/GitErrorBanner.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 6: Run the type checker**

Run: `yarn tsc --noEmit`
Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/ui/alert.tsx src/components/git/GitErrorBanner.tsx src/components/git/__tests__/GitErrorBanner.test.tsx
git commit -m "feat(git): add shared GitErrorBanner component"
```

---

### Task 2: Replace both duplicated banners in `ConflictResolver`

**Files:**
- Modify: `src/components/git/ConflictResolver.tsx`
- Test: `src/components/git/__tests__/ConflictResolver.test.tsx` (verify only, no changes expected)

**Interfaces:**
- Consumes: `GitErrorBanner` from Task 1.

- [ ] **Step 1: Replace the first banner (manual-resolution view, around line 147)**

Replace:

```tsx
        {error && (
          <div
            role='alert'
            className='flex items-start gap-2 mx-3 mt-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'
          >
            <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
            <span className='flex-1 wrap-break-word'>{error}</span>
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
```

with:

```tsx
        {error && <GitErrorBanner message={error} onDismiss={clearError} className='mx-3 mt-2' />}
```

- [ ] **Step 2: Replace the second banner (default conflict view, around line 197)**

Replace the second, identical block (same JSX, no `manual-resolution` wrapper differences) with the same one-line replacement:

```tsx
      {error && <GitErrorBanner message={error} onDismiss={clearError} className='mx-3 mt-2' />}
```

- [ ] **Step 3: Update imports**

Add `import { GitErrorBanner } from './GitErrorBanner';` to the imports.
Remove `AlertCircle` and `X` from the `lucide-react` import — grep the file first to confirm neither is referenced anywhere else (`grep -n "AlertCircle\|<X " src/components/git/ConflictResolver.tsx` should return no matches after the two replacements above).

- [ ] **Step 4: Run the existing test suite**

Run: `yarn test src/components/git/__tests__/ConflictResolver.test.tsx`
Expected: PASS with no changes to the test file — it queries by `role='alert'` text content and the "Dismiss error" accessible name, both of which `GitErrorBanner` preserves exactly.

- [ ] **Step 5: Run the type checker and linter**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/git/ConflictResolver.tsx
git commit -m "refactor(git): use GitErrorBanner in ConflictResolver"
```

---

### Task 3: Replace the duplicated banner in `GitLandingPanel`

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx`
- Test: `src/components/git/__tests__/GitLandingPanel.test.tsx` (verify only, no changes expected)

**Interfaces:**
- Consumes: `GitErrorBanner` from Task 1.

- [ ] **Step 1: Replace the banner**

Replace:

```tsx
          {error && (
            <div
              role='alert'
              className='flex items-start gap-2 rounded-md bg-destructive/10 border border-destructive/30 px-3 py-2 text-xs text-destructive'
            >
              <AlertCircle className='h-3.5 w-3.5 shrink-0 mt-0.5' />
              <span className='flex-1 wrap-break-word'>{error}</span>
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
```

with:

```tsx
          {error && <GitErrorBanner message={error} onDismiss={clearError} />}
```

- [ ] **Step 2: Update imports**

Add `import { GitErrorBanner } from './GitErrorBanner';` to the imports.
Remove `X` from the `lucide-react` import list (it is only used in the removed banner). Keep `AlertCircle` — it is also used at the unrelated "behind the remote" status icon later in the file (`<AlertCircle className='h-3.5 w-3.5 text-amber-500' />`).

- [ ] **Step 3: Run the existing test suite**

Run: `yarn test src/components/git/__tests__/GitLandingPanel.test.tsx`
Expected: PASS with no changes to the test file.

- [ ] **Step 4: Run the type checker and linter**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "refactor(git): use GitErrorBanner in GitLandingPanel"
```

---

### Task 4: Replace the duplicated banner in `GitStashSection`

**Files:**
- Modify: `src/components/git/GitStashSection.tsx`
- Test: `src/components/git/__tests__/GitStashSection.test.tsx` (verify only, no changes expected)

**Interfaces:**
- Consumes: `GitErrorBanner` from Task 1.

- [ ] **Step 1: Replace the banner**

Replace:

```tsx
      {/* Error banner */}
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

with:

```tsx
      {/* Error banner */}
      {error && <GitErrorBanner message={error} className='mx-3 mb-2' />}
```

- [ ] **Step 2: Update imports**

Add `import { GitErrorBanner } from './GitErrorBanner';` to the imports.
Remove `AlertCircle` from the `lucide-react` import list if it is not used elsewhere in the file (confirm via `grep -n "AlertCircle" src/components/git/GitStashSection.tsx` returning no remaining matches).

- [ ] **Step 3: Run the existing test suite**

Run: `yarn test src/components/git/__tests__/GitStashSection.test.tsx`
Expected: PASS with no changes to the test file.

- [ ] **Step 4: Run the type checker and linter**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/git/GitStashSection.tsx
git commit -m "refactor(git): use GitErrorBanner in GitStashSection"
```

---

## Final verification

- [ ] Run `yarn tsc --noEmit` — no errors.
- [ ] Run `yarn check` — no lint/format violations.
- [ ] Run `yarn test src/components/git/__tests__/GitErrorBanner.test.tsx src/components/git/__tests__/ConflictResolver.test.tsx src/components/git/__tests__/GitLandingPanel.test.tsx src/components/git/__tests__/GitStashSection.test.tsx` — full green.
- [ ] Manually eyeball `GitStashSection`'s error banner in `yarn tauri dev` (trigger a stash failure) to confirm the slightly larger padding/icon from the shared component still reads well in that panel's narrower left column.
