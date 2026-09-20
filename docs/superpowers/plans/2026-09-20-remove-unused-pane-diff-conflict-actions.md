# Remove Unused Pane Diff/Conflict Tab Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove `PaneState.openDiffTab`/`openConflictTab`, which have zero production callers now that Git renders diffs and conflicts inline inside `GitPanel`'s own right panel instead of as separate pane tabs.

**Architecture:** `src/stores/pane-store.ts` still defines `openDiffTab(diffState)` and `openConflictTab(conflictState)`, each of which builds a `DiffTab`/`ConflictTab` and calls `get().openTab(tab)`. `grep -rn "openDiffTab\|openConflictTab" src/` finds only the interface declaration and implementation in `pane-store.ts` itself — no component calls either one. This plan removes just those two actions and their `PaneState` interface entries. It deliberately leaves the `DiffTab`/`ConflictTab` types, the `isDiffTab`/`isConflictTab` type guards, and `EditorGroup.tsx`'s render branches for them alone: those are still referenced by `EditorGroup.tsx` (`isConflictTab(activeTab) ? <ConflictResolver .../> : isDiffTab(activeTab) ? <DiffViewer .../> : ...`), and deciding whether that whole route should also be removed is a separate, larger architectural call the spec itself frames as optional ("deliberately supported and tested, or removed") — not a plain dead-code deletion like the two actions this plan removes.

**Tech Stack:** TypeScript, Zustand.

**Spec:** `docs/reports/git-integration-review/02-frontend-architecture.md` ("Other unused or mismatched pane functionality": *"`PaneState.openDiffTab` and `PaneState.openConflictTab` are declared and implemented... but have no production callers. Git now renders diffs/conflicts inside `GitPanel`, so these actions and the corresponding `DiffTab`/`ConflictTab` route should either be deliberately supported and tested or removed after confirming no external use."*). Verified present in current `src/stores/pane-store.ts:101,104,363-385` with zero callers confirmed via `grep -rn "openDiffTab\|openConflictTab" src/`.

**Next Plan:** None — this is the final plan (22 of 22) in the git-frontend-architecture remediation sequence. Once it is implemented and verified, the sequence is complete.

## Global Constraints

- Commits use conventional commits format.
- `yarn tsc --noEmit` and the full pane-store test suite must pass before committing.

---

### Task 1: Remove the two unused actions

**Files:**
- Modify: `src/stores/pane-store.ts:100-105,363-385`
- Test: none — pure deletion of unreferenced code; `yarn tsc --noEmit` plus the existing `pane-store` test suite are the verification.

**Interfaces:**
- Removes: `PaneState.openDiffTab: (diffState: DiffState) => void` and `PaneState.openConflictTab: (conflictState: ConflictState) => void`, and their implementations.

- [ ] **Step 1: Confirm there are no callers**

Run: `grep -rn "openDiffTab\|openConflictTab" src/`
Expected: matches only inside `src/stores/pane-store.ts` (the interface declaration and the implementation) — no callers in any component or test.

- [ ] **Step 2: Remove the interface entries**

In `src/stores/pane-store.ts`, delete:

```ts
  // Diff tab action.
  openDiffTab: (diffState: DiffState) => void;

  // Conflict tab action.
  openConflictTab: (conflictState: ConflictState) => void;
```

- [ ] **Step 3: Remove the implementations**

Delete:

```ts
  openDiffTab(diffState) {
    const tabId = `diff:${diffState.repositoryId}/${diffState.filePath}:${diffState.isStaged ? 'staged' : 'working'}`;
    const tab: DiffTab = {
      id: tabId,
      title: `${diffState.filePath} (${diffState.isStaged ? 'Staged' : 'Working'})`,
      isDirty: false,
      tabType: 'diff',
      diffState,
    };
    get().openTab(tab);
  },

  openConflictTab(conflictState) {
    const tabId = `conflict:${conflictState.repositoryId}/${conflictState.filePath}`;
    const tab: ConflictTab = {
      id: tabId,
      title: `${conflictState.filePath} (Conflict)`,
      isDirty: false,
      tabType: 'conflict',
      conflictState,
    };
    get().openTab(tab);
  },
```

- [ ] **Step 4: Clean up now-unused imports, if any**

After the deletion, check whether `DiffState`, `DiffTab`, `ConflictState`, or `ConflictTab` are still imported/used elsewhere in `pane-store.ts` (e.g. by other actions, or re-exported). If any of those four names are no longer referenced anywhere in the file, remove them from the top-of-file `import type { ... } from '@/types/pane-types';` block; otherwise leave the import list unchanged.

- [ ] **Step 5: Verify nothing referenced the removed actions**

Run: `grep -rn "openDiffTab\|openConflictTab" src/`
Expected: no matches.

Run: `yarn tsc --noEmit`
Expected: no errors — this is the real safety net here: if any file still called either action, this step fails.

Run: `yarn test src/stores/__tests__/pane-store.test.ts`
Expected: PASS

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage:

```bash
git add src/stores/pane-store.ts
```

Commit message: `chore(panes): remove unused openDiffTab/openConflictTab actions`.
