# Relative-Time Formatter De-duplication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the two near-identical relative-time formatters in `GitCommitLog.tsx` and `GitStashSection.tsx` into one shared, tested utility, and resolve the behavior difference between them for timestamps older than 30 days.

**Architecture:** `GitCommitLog.tsx`'s `relativeTime` and `GitStashSection.tsx`'s `formatAge` are identical for anything under 30 days old (`just now` / `Xm ago` / `Xh ago` / `Xd ago`) but diverge past that: `GitCommitLog` shows a coarse `"Xmo ago"`, while `GitStashSection` shows a localized short date (`"Sep 20"`). This plan keeps the more informative behavior — a calendar date is more useful than a rounded "months ago" figure once something is over a month old, and it's the behavior already shown for old stashes today — and moves the single resulting implementation to `src/lib/relative-time.ts`, a new file alongside this codebase's other small formatting/utility modules (e.g. `src/lib/colors.ts`, `src/lib/utils.ts`).

**Tech Stack:** TypeScript, Vitest.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` ("Duplicate logic and boundary observations" item 1: *"Relative time formatting is duplicated in `GitCommitLog.tsx:8-21` and `GitStashSection.tsx:26-37`, with different behavior after 30 days. Extract one tested formatter if the product intends consistency."*). Verified present in current `src/components/git/GitCommitLog.tsx:8-21` and `src/components/git/GitStashSection.tsx:26-37`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-remove-unused-pane-diff-conflict-actions.md` next (plan 21 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- DRY — this plan exists specifically to remove this duplication.
- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Extract `formatRelativeTime` and use it in both call sites

**Files:**
- Create: `src/lib/relative-time.ts`
- Modify: `src/components/git/GitCommitLog.tsx:1-21`
- Modify: `src/components/git/GitStashSection.tsx:1-37`
- Test: `src/lib/__tests__/relative-time.test.ts` (new file)

**Interfaces:**
- Produces: `export function formatRelativeTime(timestamp: string): string` in `src/lib/relative-time.ts`.
- Consumes (both components): replaces their local `relativeTime`/`formatAge` function and every call site that used it.

- [ ] **Step 1: Write the failing test**

Create `src/lib/__tests__/relative-time.test.ts`:

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { formatRelativeTime } from '@/lib/relative-time';

describe('formatRelativeTime', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-09-20T12:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns "just now" for under a minute', () => {
    expect(formatRelativeTime(new Date('2026-09-20T11:59:30Z').toISOString())).toBe('just now');
  });

  it('returns minutes for under an hour', () => {
    expect(formatRelativeTime(new Date('2026-09-20T11:45:00Z').toISOString())).toBe('15m ago');
  });

  it('returns hours for under a day', () => {
    expect(formatRelativeTime(new Date('2026-09-20T09:00:00Z').toISOString())).toBe('3h ago');
  });

  it('returns days for under 30 days', () => {
    expect(formatRelativeTime(new Date('2026-09-15T12:00:00Z').toISOString())).toBe('5d ago');
  });

  it('returns a short calendar date for 30 days or more', () => {
    const timestamp = new Date('2026-08-01T12:00:00Z').toISOString();
    expect(formatRelativeTime(timestamp)).toBe(
      new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' }),
    );
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/lib/__tests__/relative-time.test.ts`
Expected: FAIL with a module-not-found error — `src/lib/relative-time.ts` doesn't exist yet.

- [ ] **Step 3: Create the shared formatter**

Create `src/lib/relative-time.ts`:

```ts
/** Format a UTC timestamp string into a concise relative label — "just now",
 *  "Xm/Xh/Xd ago" under 30 days, then a short calendar date. */
export function formatRelativeTime(timestamp: string): string {
  const diff = Date.now() - new Date(timestamp).getTime();
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/lib/__tests__/relative-time.test.ts`
Expected: PASS

- [ ] **Step 5: Replace both local implementations**

In `src/components/git/GitCommitLog.tsx`, delete the local `relativeTime` function and its now-unnecessary `useState`-adjacent placement:

```tsx
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { CommitInfo } from '@/lib/tauri-api';
import { formatRelativeTime } from '@/lib/relative-time';
import { useGitStore } from '@/stores/git-store-context';
```

Remove the `function relativeTime(timestamp: string): string { ... }` block entirely, and update its one call site:

```tsx
                {commit.author} · {formatRelativeTime(commit.timestamp)}
```

In `src/components/git/GitStashSection.tsx`, remove the local `formatAge` function and its import block header comment, add the shared import:

```tsx
import { formatRelativeTime } from '@/lib/relative-time';
```

and update its one call site:

```tsx
                  <span className='text-[10px] text-muted-foreground/50 shrink-0'>
                    {formatRelativeTime(stash.timestamp)}
                  </span>
```

- [ ] **Step 6: Run the full git component suite and typecheck**

Run: `yarn test src/components/git src/lib/__tests__/relative-time.test.ts`
Expected: PASS

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/lib/relative-time.ts src/lib/__tests__/relative-time.test.ts src/components/git/GitCommitLog.tsx src/components/git/GitStashSection.tsx
```

Commit message: `refactor(git): extract shared formatRelativeTime, unifying commit/stash age display past 30 days`.
