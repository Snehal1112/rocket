# Plan 9: Git Tab Redesign — Cleanup & Integration Verification

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean up components that are no longer used after the redesign, verify the full integration works, and ensure no dead code remains.

**Architecture:** The old Tabs-based layout has been replaced. Components like `GitRemoteActions` may now be redundant (its functionality moved to `GitLandingPanel`). Old components `GitStagedFiles` and `GitChangedFiles` may be replaced by `GitFileList`. This plan audits and cleans up.

**Tech Stack:** React, TypeScript

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 4

**Depends on:** Plan 8 (restructured WorkspaceGitTab)

---

## Chunk 1: Cleanup & Verification

### Task 1: Audit and remove unused components

**Files:**
- Possibly delete or deprecate: `src/components/git/GitRemoteActions.tsx`
- Possibly delete or deprecate: `src/components/git/GitStagedFiles.tsx`, `src/components/git/GitChangedFiles.tsx`
- Modify: any files that still import the removed components

- [ ] **Step 1: Check if `GitRemoteActions` is imported anywhere**

Search the codebase for imports of `GitRemoteActions`:

```bash
grep -r "GitRemoteActions" src/ --include="*.tsx" --include="*.ts" -l
```

If it's only imported by `WorkspaceGitTab.tsx` and that import was removed in Plan 8, it's safe to delete. If other files import it, leave it.

- [ ] **Step 2: Check if `GitStagedFiles` and `GitChangedFiles` are imported anywhere**

```bash
grep -r "GitStagedFiles\|GitChangedFiles" src/ --include="*.tsx" --include="*.ts" -l
```

If they are only imported by `WorkspaceGitTab.tsx` and those imports were removed (replaced by `GitFileList`), they are safe to delete. If `GitFileList` wraps them internally, leave them.

- [ ] **Step 3: Delete unused component files**

For each component confirmed as unused:
```bash
git rm src/components/git/GitRemoteActions.tsx
git rm src/components/git/GitStagedFiles.tsx
git rm src/components/git/GitChangedFiles.tsx
```

Only delete files that are truly unused. If unsure, keep them.

- [ ] **Step 4: Check for any remaining imports of deleted files**

```bash
npx tsc --noEmit
```

Fix any broken imports. If TypeScript compilation passes, the cleanup is correct.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(frontend): remove unused git components after layout redesign"
```

### Task 2: Verify full integration

**Files:** None created — verification only

- [ ] **Step 1: Verify TypeScript compilation**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 2: Verify the app builds**

Run: `npm run build` (or `yarn build` — check `package.json`)
Expected: builds successfully with no errors

- [ ] **Step 3: Verify no unused imports or dead code warnings**

Run the linter if configured:
```bash
npm run lint
```

Fix any warnings about unused imports or variables introduced during the restructure.

- [ ] **Step 4: Commit any lint fixes**

```bash
git add -A
git commit -m "fix(frontend): resolve lint warnings from git tab redesign"
```

### Task 3: Update `GitToolbarButton` if needed

**Files:**
- Modify: `src/components/layout/GitToolbarButton.tsx` (if needed)

- [ ] **Step 1: Read `GitToolbarButton.tsx`**

This component opens the Git tab as a pane tab. Check if the tab it opens still works correctly with the new `WorkspaceGitTab` layout.

- [ ] **Step 2: Verify the Git toolbar button still opens the Git tab correctly**

The button creates a `GitTab` pane and calls `openTab`. The new layout is inside `WorkspaceGitTab` which is what renders for that tab type. As long as the pane type hasn't changed, this should work without modifications.

If the toolbar button references any of the deleted components or uses the old `activeSubTab` prop, update it.

- [ ] **Step 3: Verify `BranchSelector` placement**

The `BranchSelector` currently lives in the Git tab header. In the new layout, it should still be accessible — either in the top toolbar area or within the left panel. Check where it's rendered:

```bash
grep -r "BranchSelector" src/ --include="*.tsx" -l
```

If it was removed from `WorkspaceGitTab` during Plan 8, ensure it's still accessible somewhere (e.g., in the collection toolbar or the Git toolbar button area). If it's missing, add it back to the left panel header (next to the collection name) or the right panel landing state.

- [ ] **Step 4: Commit if any changes were made**

```bash
git add -A
git commit -m "fix(frontend): ensure GitToolbarButton and BranchSelector work with new layout"
```
