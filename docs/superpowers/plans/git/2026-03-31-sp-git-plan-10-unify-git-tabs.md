# Plan 10: Unify Git Tabs — Remove Collection-Level GitTab Pane

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the old collection-level `GitTab` pane type so there is one single Git UI in the app, accessed through the workspace's git section.

**Architecture:** The toolbar button currently opens a separate `GitTab` pane with the old Tabs-based layout. After this change, it navigates to the workspace's git section (`WorkspaceGitTab`) instead. The `GitTab` pane type, its component, and all orphaned components are deleted.

**Tech Stack:** React, TypeScript, Zustand

**Spec:** `docs/superpowers/specs/2026-03-31-sp-unify-git-tabs-design.md`

**Depends on:** Plans 7–9 (WorkspaceGitTab must exist)

---

## Chunk 1: Remove GitTab pane type and update routing

### Task 1: Always include git tab in workspace tabs

**Files:**
- Modify: `src/stores/pane-store.ts`

- [ ] **Step 1: Update `openWorkspaceTabs` to always include git**

In `src/stores/pane-store.ts`, find the `openWorkspaceTabs` method (~line 350). Change:

```typescript
const sections: WorkspaceTabSection[] = ['overview', 'environments'];
if (!isDefault) sections.push('git');
```

To:

```typescript
const sections: WorkspaceTabSection[] = ['overview', 'environments', 'git'];
```

Remove the `isDefault` parameter from the method signature since it's no longer used:

```typescript
// Old signature
openWorkspaceTabs: (workspaceId: string, isDefault: boolean) => void;

// New signature
openWorkspaceTabs: (workspaceId: string) => void;
```

And update the method implementation:

```typescript
openWorkspaceTabs(workspaceId) {
  get().closeAll();
  const sections: WorkspaceTabSection[] = ['overview', 'environments', 'git'];

  const tabs: WorkspaceTab[] = sections.map((section) => ({
    id: `workspace:${workspaceId}:${section}`,
    title: section.charAt(0).toUpperCase() + section.slice(1),
    isDirty: false,
    tabType: 'workspace',
    workspaceId,
    activeSection: section,
  }));

  const overviewId = tabs[0].id;
  const { root } = get();
  const rootLeaf = root as LeafNode;
  const newRoot = updateLeaf(root, rootLeaf.groupId, (leaf) => ({
    ...leaf,
    tabs,
    activeTabId: overviewId,
  }));
  set({ root: newRoot, activeGroupId: rootLeaf.groupId });
},
```

- [ ] **Step 2: Update all callers of `openWorkspaceTabs` to remove the `isDefault` argument**

There are 6 call sites. In each one, remove the second argument:

**`src/stores/workspace-store.ts` (~line 90):**
```typescript
// Old
usePaneStore.getState().openWorkspaceTabs(payload.id, payload.id === 'default')
// New
usePaneStore.getState().openWorkspaceTabs(payload.id)
```

**`src/stores/workspace-store.ts` (~line 112):**
```typescript
// Old
usePaneStore.getState().openWorkspaceTabs(activeWs.id, activeWs.id === 'default')
// New
usePaneStore.getState().openWorkspaceTabs(activeWs.id)
```

**`src/stores/workspace-store.ts` (~line 127):**
```typescript
// Old
usePaneStore.getState().openWorkspaceTabs(activeWs.id, activeWs.id === 'default')
// New
usePaneStore.getState().openWorkspaceTabs(activeWs.id)
```

**`src/components/layout/CollectionsSidebar.tsx` (~line 429):**
Search for `openWorkspaceTabs(` and remove the second argument.

**`src/components/layout/WorkspaceSection.tsx` (~line 28):**
```typescript
// Old
usePaneStore.getState().openWorkspaceTabs(workspace.id, workspace.id === 'default')
// New
usePaneStore.getState().openWorkspaceTabs(workspace.id)
```

**`src/App.tsx` (two call sites, ~lines 34 and 42):**
```typescript
// Old
usePaneStore.getState().openWorkspaceTabs(ws.id, ws.id === 'default')
// New
usePaneStore.getState().openWorkspaceTabs(ws.id)
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/stores/pane-store.ts src/stores/workspace-store.ts src/components/layout/CollectionsSidebar.tsx src/components/layout/WorkspaceSection.tsx src/App.tsx
git commit -m "refactor(frontend): always include git tab in workspace tabs"
```

### Task 2: Rewrite `GitToolbarButton` to activate workspace git tab

**Files:**
- Modify: `src/components/layout/GitToolbarButton.tsx`

- [ ] **Step 1: Read current `GitToolbarButton.tsx`**

Current implementation creates a `GitTab` pane:

```typescript
import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { usePaneStore } from '@/stores/pane-store';
import { useGitStore } from '@/stores/git-store';
import type { GitTab } from '@/types/pane-types';

export function GitToolbarButton() {
  const activeCollection = usePaneStore((s) => s.activeCollection);
  const openTab = usePaneStore((s) => s.openTab);
  const collectionPath = useGitStore((s) => s.collectionPath);

  const handleClick = () => {
    if (!activeCollection) return;
    const tab: GitTab = { ... };
    openTab(tab);
  };

  return (
    <Button ... disabled={!activeCollection} ...>
      <GitBranch className="h-4 w-4" />
    </Button>
  );
}
```

- [ ] **Step 2: Rewrite to activate the workspace git tab**

Replace the entire file content with:

```typescript
import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

export function GitToolbarButton() {
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const isWorkspaceMode = usePaneStore((s) => s.isWorkspaceMode);
  const setActiveTab = usePaneStore((s) => s.setActiveTab);
  const root = usePaneStore((s) => s.root);

  const handleClick = () => {
    if (!activeWorkspaceId) return;
    const gitTabId = `workspace:${activeWorkspaceId}:git`;

    // Find which editor group contains the git tab.
    const found = findGitTab(root, gitTabId);
    if (found) {
      setActiveTab(gitTabId, found);
    }
  };

  const enabled = isWorkspaceMode() && !!activeWorkspaceId;

  return (
    <Button
      variant="ghost"
      size="icon"
      className="h-7 w-7"
      onClick={handleClick}
      disabled={!enabled}
      title="Open Git panel"
    >
      <GitBranch className="h-4 w-4" />
    </Button>
  );
}

// Walk the pane tree to find which group contains the given tab id.
function findGitTab(node: import('@/types/pane-types').PaneNode, tabId: string): string | null {
  if (node.type === 'leaf') {
    return node.tabs.some((t) => t.id === tabId) ? node.groupId : null;
  }
  return findGitTab(node.children[0], tabId) ?? findGitTab(node.children[1], tabId);
}
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/GitToolbarButton.tsx
git commit -m "refactor(frontend): rewrite GitToolbarButton to activate workspace git tab"
```

### Task 3: Remove `GitTab` pane type and update consumers

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/components/panes/EditorGroup.tsx`
- Modify: `src/components/panes/TabItem.tsx`
- Modify: `src/stores/__tests__/pane-store.test.ts`

- [ ] **Step 1: Remove `GitTab` from `pane-types.ts`**

In `src/types/pane-types.ts`:

1. Delete the `GitTab` interface (lines 77–81):
```typescript
// DELETE this entire block:
export interface GitTab extends BaseTab {
  tabType: 'git';
  collectionName: string;
  collectionPath: string;
}
```

2. Remove `GitTab` from the `Tab` union (line 83):
```typescript
// Old
export type Tab = RequestTab | CollectionTab | WorkspaceTab | DiffTab | ConflictTab | GitTab;
// New
export type Tab = RequestTab | CollectionTab | WorkspaceTab | DiffTab | ConflictTab;
```

3. Delete the `isGitTab` function (lines 101–103):
```typescript
// DELETE this entire block:
export function isGitTab(tab: Tab): tab is GitTab {
  return tab.tabType === 'git';
}
```

- [ ] **Step 2: Remove `GitTab` from `EditorGroup.tsx`**

In `src/components/panes/EditorGroup.tsx`:

1. Remove the `GitTab` import (line 18):
```typescript
// DELETE this line:
import { GitTab } from '@/components/git/GitTab';
```

2. Remove `isGitTab` from the import (line 24):
```typescript
// Old
import { isDiffTab, isRequestTab, isConflictTab, isGitTab, isWorkspaceTab } from '@/types/pane-types';
// New
import { isDiffTab, isRequestTab, isConflictTab, isWorkspaceTab } from '@/types/pane-types';
```

3. Remove the `isGitTab` rendering branch (lines 79–80):
```typescript
// DELETE these two lines:
          ) : isGitTab(activeTab) ? (
            <GitTab tab={activeTab} />
```

The resulting render chain should flow directly from `isRequestTab` to `isWorkspaceTab`:
```tsx
          ) : isRequestTab(activeTab) ? (
            <RequestPanel tab={activeTab} groupId={node.groupId} />
          ) : isWorkspaceTab(activeTab) ? (
```

- [ ] **Step 3: Remove `isGitTab` from `TabItem.tsx`**

In `src/components/panes/TabItem.tsx`:

1. Remove `isGitTab` from the import (line 3):
```typescript
// Old
import { isRequestTab, isGitTab, isWorkspaceTab } from '@/types/pane-types';
// New
import { isRequestTab, isWorkspaceTab } from '@/types/pane-types';
```

2. Remove the `isGitTab` branch in the icon rendering (lines 46–47):
```typescript
// DELETE these two lines:
      ) : isGitTab(tab) ? (
        <GitBranch className="h-3 w-3 shrink-0 text-muted-foreground" />
```

The workspace git tab already renders a `GitBranch` icon (line 52), so this is covered.

3. Remove `GitBranch` from the lucide import if it's no longer used directly in the `isGitTab` branch. Check: the workspace tab section still uses `GitBranch` (line 52), so keep the import.

- [ ] **Step 4: Remove `isGitTab` test from pane-store test**

In `src/stores/__tests__/pane-store.test.ts`, find and delete the `isGitTab` test block:

```typescript
// DELETE the entire test:
  // ── isGitTab ──────────────────────────────────────────────────────────────

  it('isGitTab returns true for git tabs and false for others', () => {
    // ... entire test body
  });
```

Also remove the `isGitTab` import from the test file's imports.

- [ ] **Step 5: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 6: Commit**

```bash
git add src/types/pane-types.ts src/components/panes/EditorGroup.tsx src/components/panes/TabItem.tsx src/stores/__tests__/pane-store.test.ts
git commit -m "refactor(frontend): remove GitTab pane type and all references"
```

## Chunk 2: Delete old components

### Task 4: Delete `GitTab.tsx` and orphaned components

**Files:**
- Delete: `src/components/git/GitTab.tsx`
- Possibly delete: `src/components/git/GitRemoteActions.tsx`
- Possibly delete: `src/components/git/GitStagedFiles.tsx`
- Possibly delete: `src/components/git/GitChangedFiles.tsx`
- Possibly delete: `src/components/git/GitFileRow.tsx`
- Possibly delete: `src/components/git/GitStatusBadge.tsx`

- [ ] **Step 1: Delete `GitTab.tsx`**

```bash
git rm src/components/git/GitTab.tsx
```

- [ ] **Step 2: Check and delete `GitRemoteActions.tsx`**

```bash
grep -r "GitRemoteActions" src/ --include="*.tsx" --include="*.ts" -l
```

If the only result is `src/components/git/GitRemoteActions.tsx` itself (no other files import it), delete it:

```bash
git rm src/components/git/GitRemoteActions.tsx
```

- [ ] **Step 3: Check and delete `GitStagedFiles.tsx`**

```bash
grep -r "GitStagedFiles" src/ --include="*.tsx" --include="*.ts" -l
```

If only self-referencing, delete:

```bash
git rm src/components/git/GitStagedFiles.tsx
```

- [ ] **Step 4: Check and delete `GitChangedFiles.tsx`**

```bash
grep -r "GitChangedFiles" src/ --include="*.tsx" --include="*.ts" -l
```

If only self-referencing, delete:

```bash
git rm src/components/git/GitChangedFiles.tsx
```

- [ ] **Step 5: Check and delete `GitFileRow.tsx`**

```bash
grep -r "GitFileRow" src/ --include="*.tsx" --include="*.ts" -l
```

If only referenced by deleted files and itself, delete:

```bash
git rm src/components/git/GitFileRow.tsx
```

- [ ] **Step 6: Check and delete `GitStatusBadge.tsx`**

```bash
grep -r "GitStatusBadge" src/ --include="*.tsx" --include="*.ts" -l
```

If only referenced by deleted files and itself, delete:

```bash
git rm src/components/git/GitStatusBadge.tsx
```

- [ ] **Step 7: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors. If any broken imports remain, fix them.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(frontend): delete old GitTab component and orphaned git components"
```

## Chunk 3: Verification

### Task 5: Full integration verification

**Files:** None created — verification only

- [ ] **Step 1: Verify TypeScript compilation**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 2: Verify the app builds**

Run: `yarn build`
Expected: builds successfully with no errors

- [ ] **Step 3: Run lint**

Run: `yarn lint`
Expected: no new errors (warnings are acceptable)

- [ ] **Step 4: Run tests**

Run: `yarn test --run`
Expected: all tests pass

- [ ] **Step 5: Verify no references to deleted types remain**

Run:
```bash
grep -r "isGitTab\|GitTab" src/ --include="*.tsx" --include="*.ts" -l | grep -v WorkspaceGitTab | grep -v node_modules
```

Expected: no results (only `WorkspaceGitTab` references should exist)

- [ ] **Step 6: Verify no references to deleted components remain**

Run:
```bash
grep -r "GitRemoteActions\|GitStagedFiles\|GitChangedFiles\|GitSidebarPanel\|GitBottomBar" src/ --include="*.tsx" --include="*.ts" -l
```

Expected: no results

- [ ] **Step 7: Commit any fixes**

If any issues were found and fixed:

```bash
git add -A
git commit -m "fix(frontend): resolve issues from git tab unification"
```
