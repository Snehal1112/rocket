# GitLandingPanel Enhancement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reshape `GitLandingPanel` into a compact Status Card layout using shadcn `Card` and `Badge` components with a Bruno-inspired monospace-accented aesthetic.

**Architecture:** Single-file JSX restructure. All handlers, state vars, and dialogs are unchanged. The return value is replaced: the large decorative icon + subtitle text + plain ahead/behind line are removed; the remaining elements are wrapped in a `Card` with a header row showing branch name and ahead/behind badges.

**Tech Stack:** React, TypeScript, Tailwind CSS, shadcn/ui (`Card`, `Badge`), lucide-react

**Spec:** `docs/superpowers/specs/2026-04-02-git-landing-panel-enhancement-design.md`

---

## File Map

| Action | File |
|---|---|
| Modify | `src/components/git/GitLandingPanel.tsx` |

---

### Task 1: Add new imports

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx:1-13`

- [ ] **Step 1: Add `Card`/`CardContent`/`CardHeader` and `Badge` imports**

Open `src/components/git/GitLandingPanel.tsx`. The current import block looks like:

```tsx
import { useState } from 'react';
import {
  GitBranch,
  RefreshCw,
  ArrowDown,
  ArrowUp,
  Loader2,
  Clock,
  Check,
  AlertCircle,
  GitCommit,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { useGitStore } from '@/stores/git-store';
```

Replace with:

```tsx
import { useState } from 'react';
import {
  GitBranch,
  RefreshCw,
  ArrowDown,
  ArrowUp,
  Loader2,
  Clock,
  Check,
  AlertCircle,
  GitCommit,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { useGitStore } from '@/stores/git-store';
```

- [ ] **Step 2: TypeScript check (imports only)**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors (the new imports exist at `src/components/ui/card.tsx` and `src/components/ui/badge.tsx`).

---

### Task 2: Reshape the return JSX

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx` — the `return (...)` block (approximately lines 133–247)

- [ ] **Step 1: Replace the return block**

The current return block starts with:

```tsx
  return (
    <div className="flex flex-col items-center justify-center h-full px-6">
      <GitBranch className="h-12 w-12 text-muted-foreground/30" />

      <p className="text-sm text-muted-foreground text-center max-w-[280px] mt-4 mb-6">
        Perform git actions or open files from sidebar to view
      </p>

      {/* Fetch / Pull / Push button group. */}
      <div className="flex gap-2 mb-6">
```

Replace the entire `return (...)` expression — from `return (` through the closing `);` at the end of the function — with the following. The two `AlertDialog` blocks and all handler logic above the return are **not touched**.

```tsx
  return (
    <div className="flex flex-col items-center justify-center h-full px-6">
      <Card className="w-full max-w-[320px]">
        <CardHeader className="flex flex-row items-center px-4 py-3 space-y-0">
          <GitBranch className="h-3.5 w-3.5 text-muted-foreground mr-2" />
          <span className="font-mono text-sm font-medium">
            {status?.branch ?? 'no branch'}
          </span>
          <div className="ml-auto flex gap-1.5">
            <Badge
              variant="outline"
              className={ahead > 0 ? 'text-amber-500' : 'text-emerald-500'}
            >
              ↑{ahead}
            </Badge>
            <Badge
              variant="outline"
              className={behind > 0 ? 'text-amber-500' : 'text-emerald-500'}
            >
              ↓{behind}
            </Badge>
          </div>
        </CardHeader>

        <CardContent className="px-4 pb-4 pt-0">
          {/* Fetch / Pull / Push button group. */}
          <div className="flex gap-2 mb-3">
            <Button variant="outline" size="sm" onClick={handleFetch} disabled={fetching}>
              {fetching ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
              Fetch
            </Button>
            <Button variant="outline" size="sm" onClick={handlePull} disabled={pulling}>
              {pulling ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <ArrowDown className="h-3.5 w-3.5" />
              )}
              Pull{behind > 0 ? ` ↓${behind}` : ''}
            </Button>
            <Button variant="outline" size="sm" onClick={handlePush} disabled={pushing}>
              {pushing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <ArrowUp className="h-3.5 w-3.5" />
              )}
              Push{ahead > 0 ? ` ↑${ahead}` : ''}
            </Button>
          </div>

          {/* Last fetched timestamp. */}
          <p className="text-xs text-muted-foreground flex items-center gap-1.5">
            <Clock className="h-3.5 w-3.5 text-muted-foreground" />
            Last fetched:{' '}
            <span className="font-medium text-foreground">{lastFetched ?? 'Never'}</span>
          </p>
        </CardContent>
      </Card>

      {/* Branch status badge. */}
      <div className="flex items-center gap-1.5 text-xs border rounded-md px-3 py-1.5 mt-3">
        {isUpToDate ? (
          <>
            <Check className="h-3.5 w-3.5 text-emerald-500" />
            Your branch is up to date
          </>
        ) : behind > 0 ? (
          <>
            <AlertCircle className="h-3.5 w-3.5 text-amber-500" />
            {behind} commits behind
          </>
        ) : (
          <>
            <GitCommit className="h-3.5 w-3.5 text-muted-foreground" />
            {ahead} commits ahead
          </>
        )}
      </div>

      {/* Auto-stash confirmation dialog. */}
      <AlertDialog open={showStashDialog} onOpenChange={setShowStashDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Uncommitted Changes</AlertDialogTitle>
            <AlertDialogDescription>
              You have uncommitted changes. Pulling may cause conflicts or data loss. Would you like to stash your changes first?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="flex-wrap gap-2">
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePullAnyway}>Pull Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleStashAndPull}>Stash & Pull</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Fetch-before-push confirmation dialog. */}
      <AlertDialog open={showFetchFirstDialog} onOpenChange={setShowFetchFirstDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Fetch Before Push</AlertDialogTitle>
            <AlertDialogDescription>
              {(status?.behind ?? 0) > 0
                ? `Your branch is ${status?.behind} commits behind the remote. Fetching first ensures you have the latest changes and reduces the risk of conflicts.`
                : 'You have not fetched from the remote yet. Fetching first ensures you have the latest changes and reduces the risk of conflicts.'}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="flex-wrap gap-2">
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePushAnyway}>Push Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleFetchAndPush}>Fetch & Push</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
```

**Note on `CardHeader` override:** The default `CardHeader` applies `flex flex-col space-y-1.5`. This plan overrides it with `flex flex-row items-center space-y-0` via `className` prop, which is supported — `CardHeader` passes `className` through `cn()`.

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "feat: reshape GitLandingPanel into Status Card layout"
```

---

### Task 3: Visual smoke test

- [ ] **Step 1: Start the app and navigate to the Git panel**

```bash
yarn dev
```

Open the app, navigate to a workspace with a git repo. Verify:

1. The large `GitBranch` icon is gone.
2. The "Perform git actions..." subtitle is gone.
3. The plain `↑N Ahead | ↓N Behind` text line is gone.
4. A `Card` is visible with: small branch icon + branch name in monospace on the left, `↑N` and `↓N` badges on the right.
5. Fetch/Pull/Push buttons appear inside the card body.
6. "Last fetched: Never" appears below the buttons inside the card.
7. The status badge (up to date / N commits behind / N commits ahead) appears below the card with `mt-3` spacing.
8. When `ahead > 0`, the `↑N` badge is amber; when `ahead === 0`, it is green. Same logic for behind.
9. Both AlertDialogs still work (stash confirmation on pull with dirty tree; fetch-first confirmation on push).

- [ ] **Step 2: Test with no git repo (status is null)**

Navigate to a workspace with no git repo. Verify the branch name shows `no branch` (not a crash or undefined).

---

## Self-Review

- **Spec coverage:** All removals accounted for (large icon, subtitle, plain ahead/behind text). All additions accounted for (Card, CardHeader with branch name + badges, CardContent with buttons + timestamp, status badge below). Both dialogs preserved. ✓
- **Placeholder scan:** No TBDs. Every step has exact code. ✓
- **Type consistency:** `ahead`, `behind`, `isUpToDate` derived values are defined earlier in the component and reused in Task 2 without redefinition. `status?.branch` is optional-chained to `'no branch'` fallback. ✓
- **Import completeness:** `Card`, `CardContent`, `CardHeader` from `@/components/ui/card`; `Badge` from `@/components/ui/badge`. Both files confirmed to exist. `CardHeader` confirmed to accept `className` via `cn()`. ✓
