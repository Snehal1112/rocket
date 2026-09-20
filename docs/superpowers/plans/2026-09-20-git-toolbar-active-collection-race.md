# GitToolbarButton Active-Collection Race Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `openGitPanel` from opening a Git tab for a collection the user is no longer viewing, if they switch the active collection while the toolbar button's `listCollections()` lookup is still in flight.

**Architecture:** `GitToolbarButton.tsx`'s `openGitPanel` captures `activeCollection` once at the start, `await`s `listCollections()`, and then unconditionally opens/repairs a Git tab for that captured collection name — it never re-checks whether `activeCollection` is still the same after the await. This is a narrower, still-real remnant of the class of bug the review's F-04 describes; the main part of F-04 (the toolbar reusing a stale global `collectionPath`) is already fixed in the current codebase — `openGitPanel` now always re-resolves the repository id from a fresh `listCollections()` call and repairs a mismatched existing tab, rather than trusting a cached path. What's left is this one unguarded await.

**Tech Stack:** TypeScript, Zustand, Vitest.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` (F-04 "Toolbar can open a collection Git tab with another collection's path" — its third focused test: *"Switch active collection while `listCollections()` is pending; assert no stale tab is opened for the wrong context."*). Verified present in current `src/components/layout/GitToolbarButton.tsx:11-49`.

**Next Plan:** After this plan is fully implemented and verified, execute `docs/superpowers/plans/2026-09-20-git-panel-load-state-unification.md` next (plan 2 of 22 in the git-frontend-architecture remediation sequence).

## Global Constraints

- Commits use conventional commits format.
- `yarn tsc --noEmit` and `yarn test <pattern>` must pass before each commit.

---

### Task 1: Re-check the active collection after `listCollections()` resolves

**Files:**
- Modify: `src/components/layout/GitToolbarButton.tsx`
- Test: `src/components/layout/__tests__/GitToolbarButton.test.tsx` (check first with `ls src/components/layout/__tests__/ | grep GitToolbarButton` — extend it if it exists)

**Interfaces:**
- Produces: no new exports, no behavior change to the happy path. `openGitPanel` returns early (opening no tab) if the active collection changed while `listCollections()` was pending.

- [ ] **Step 1: Write the failing test**

Create (or extend) `src/components/layout/__tests__/GitToolbarButton.test.tsx`:

```tsx
import { describe, expect, it, vi } from 'vitest';
import { openGitPanel } from '@/components/layout/GitToolbarButton';
import * as tauriApi from '@/lib/tauri-api';
import { createDeferred } from '@/test/deferred';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn() };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

describe('openGitPanel active-collection race', () => {
  it('does not open a tab for a collection the user has since navigated away from', async () => {
    usePaneStore.getState().reset();
    usePaneStore.setState({ activeCollection: 'collection-a' });

    const deferred = createDeferred<tauriApi.CollectionSummary[]>();
    vi.mocked(tauriApi.listCollections).mockReturnValue(deferred.promise);

    const openPromise = openGitPanel();

    // User switches to a different collection while listCollections() is pending.
    usePaneStore.setState({ activeCollection: 'collection-b' });

    deferred.resolve([
      { name: 'collection-a', repositoryId: '/repos/a' } as tauriApi.CollectionSummary,
    ]);
    await openPromise;

    const { root } = usePaneStore.getState();
    const hasGitTabForA =
      root.type === 'leaf' && root.tabs.some((t) => t.id === 'git:collection-a');
    expect(hasGitTabForA).toBe(false);
  });
});
```

Adjust the `CollectionSummary` fixture fields to match whatever that type actually requires (check `src/lib/tauri-api.ts`'s `CollectionSummary` interface — add any other required fields with plausible values).

- [ ] **Step 2: Run the test to verify it fails**

Run: `yarn test src/components/layout/__tests__/GitToolbarButton.test.tsx`
Expected: FAIL — `openGitPanel` opens `git:collection-a` regardless of the active-collection change that happened during the await.

- [ ] **Step 3: Add the re-check**

In `src/components/layout/GitToolbarButton.tsx`, update `openGitPanel`:

```tsx
export async function openGitPanel(): Promise<void> {
  const { activeCollection, openTab, root, closeTab } = usePaneStore.getState();
  if (!activeCollection) return;

  let summary: CollectionSummary | undefined;
  try {
    const summaries = await listCollections();
    summary = summaries.find((candidate) => candidate.name === activeCollection);
  } catch {
    toast.error('Failed to open Git panel: could not load collections.');
    return;
  }

  // The active collection may have changed while listCollections() was in
  // flight — don't open a Git tab for a collection the user has since
  // navigated away from.
  if (usePaneStore.getState().activeCollection !== activeCollection) return;

  if (!summary) {
    toast.error('Failed to open Git panel: collection not found.');
    return;
  }

  const tabId = `git:${activeCollection}`;
  const found = findTabInTree(root, tabId);
  if (found) {
    const existingTab = found.tab as Partial<GitTab>;
    if (
      existingTab.repositoryId !== summary.repositoryId ||
      existingTab.repositoryLabel !== summary.name
    ) {
      closeTab(tabId, found.leaf.groupId);
    }
  }

  const tab: GitTab = {
    id: tabId,
    title: 'Git UI',
    tabType: 'git',
    repositoryId: summary.repositoryId,
    repositoryLabel: summary.name,
    isDirty: false,
  };
  openTab(tab);
}
```

(Only the new `if (usePaneStore.getState().activeCollection !== activeCollection) return;` line is added — everything else in the function is unchanged from the current source.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `yarn test src/components/layout/__tests__/GitToolbarButton.test.tsx`
Expected: PASS

- [ ] **Step 5: Run typecheck**

Run: `yarn tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/components/layout/GitToolbarButton.tsx src/components/layout/__tests__/GitToolbarButton.test.tsx
```

Commit message: `fix(git): ignore a stale collections lookup if the active collection changed while it was pending`.
