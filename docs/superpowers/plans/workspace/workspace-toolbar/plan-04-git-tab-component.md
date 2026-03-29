# Workspace Toolbar Plan 4: GitTab Full-Panel Component

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the full-panel `GitTab` component that renders as a tab in the editor area, and wire it into `EditorGroup` and `TabItem` so git tabs are recognized and rendered correctly.

**Architecture:** `GitTab` reuses all existing git sub-components (BranchSelector, GitRemoteActions, GitCommitForm, etc.) but lays them out in a full-panel view instead of the cramped sidebar. The top bar shows branch selector + remote actions. The body has a tabbed layout (Changes/Log/Stash) with a diff viewer panel. `EditorGroup` and `TabItem` get routing logic for the new `tabType: 'git'`.

**Tech Stack:** TypeScript, React, shadcn/ui (Tabs, ScrollArea, Separator), Zustand

**Spec:** `workspace-toolbar-design.md`

---

## Task 1: Build GitTab component

**Files:**
- Create: `src/components/git/GitTab.tsx`

**Depends on:** Plan 1 Task 1 (GitTab type exists), Plan 2 (old git sidebar removed)

- [ ] **Step 1: Create the GitTab component**

Create `src/components/git/GitTab.tsx`:

```typescript
import { useState, useEffect } from 'react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { Button } from '@/components/ui/button';
import { BranchSelector } from './BranchSelector';
import { GitRemoteActions } from './GitRemoteActions';
import { GitCommitForm } from './GitCommitForm';
import { GitStagedFiles } from './GitStagedFiles';
import { GitChangedFiles } from './GitChangedFiles';
import { GitCommitLog } from './GitCommitLog';
import { GitStashSection } from './GitStashSection';
import { GitCredentialsDialog } from './GitCredentialsDialog';
import { useGitStore } from '@/stores/git-store';
import { gitInit } from '@/lib/tauri-api';
import type { GitTab as GitTabType } from '@/types/pane-types';

interface GitTabProps {
  tab: GitTabType;
}

export function GitTab({ tab }: GitTabProps) {
  const {
    isRepo,
    loading,
    status,
    showCredentialsDialog,
    setCollection,
    refreshLog,
  } = useGitStore();

  const [activeSubTab, setActiveSubTab] = useState<string>('changes');

  // Set the git store collection when the tab mounts or collection changes.
  useEffect(() => {
    if (tab.collectionPath) {
      void setCollection(tab.collectionPath);
    }
  }, [tab.collectionPath, setCollection]);

  // Refresh log when switching to the log sub-tab.
  useEffect(() => {
    if (activeSubTab === 'log') void refreshLog();
  }, [activeSubTab, refreshLog]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        Loading git status...
      </div>
    );
  }

  if (!isRepo) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 h-full px-4 text-center">
        <p className="text-sm text-muted-foreground">
          This collection is not a git repository.
        </p>
        {tab.collectionPath && (
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              await gitInit(tab.collectionPath);
              await setCollection(tab.collectionPath);
            }}
          >
            Initialize Git
          </Button>
        )}
      </div>
    );
  }

  const changedCount = status?.files.filter((f) => f.status !== 'unchanged').length ?? 0;

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Top bar: branch selector + remote actions */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-border/70 shrink-0">
        <div className="flex items-center gap-2">
          <BranchSelector />
          {status && !status.isClean && (
            <span className="text-xs text-muted-foreground">
              {changedCount} changed {changedCount === 1 ? 'file' : 'files'}
            </span>
          )}
        </div>
        <GitRemoteActions />
      </div>

      {/* Body: tabbed layout */}
      <Tabs
        value={activeSubTab}
        onValueChange={setActiveSubTab}
        className="flex-1 flex flex-col overflow-hidden"
      >
        <TabsList className="w-full shrink-0 rounded-none border-b border-border/70 bg-card/60 h-9 px-3 justify-start">
          <TabsTrigger
            value="changes"
            className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
          >
            Changes
            {changedCount > 0 && (
              <span className="ml-1 text-2xs text-muted-foreground">({changedCount})</span>
            )}
          </TabsTrigger>
          <TabsTrigger
            value="log"
            className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
          >
            Log
          </TabsTrigger>
          <TabsTrigger
            value="stash"
            className="text-xs rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent"
          >
            Stash
          </TabsTrigger>
        </TabsList>

        <TabsContent value="changes" className="flex-1 overflow-hidden mt-0">
          <ScrollArea className="h-full">
            <div className="p-4 space-y-4 max-w-2xl">
              <GitCommitForm />
              <Separator />
              <GitStagedFiles />
              <GitChangedFiles />
            </div>
          </ScrollArea>
        </TabsContent>

        <TabsContent value="log" className="flex-1 overflow-hidden mt-0">
          <GitCommitLog />
        </TabsContent>

        <TabsContent value="stash" className="flex-1 overflow-hidden mt-0">
          <ScrollArea className="h-full">
            <div className="p-4">
              <GitStashSection />
            </div>
          </ScrollArea>
        </TabsContent>
      </Tabs>

      {showCredentialsDialog && <GitCredentialsDialog />}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitTab.tsx
git commit -m "feat: add full-panel GitTab component with changes/log/stash tabs"
```

---

## Task 2: Wire GitTab into EditorGroup routing

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`

- [ ] **Step 1: Add GitTab import and routing**

In `src/components/panes/EditorGroup.tsx`:

Add import at the top:
```typescript
import { GitTab } from '@/components/git/GitTab';
import { isGitTab } from '@/types/pane-types';
```

In the existing `isDiffTab`/`isRequestTab`/`isConflictTab` routing chain (inside the `activeTab ? (...)` ternary), add `isGitTab` check. The updated routing should be:

```typescript
{activeTab ? (
  isConflictTab(activeTab) ? (
    <ConflictResolver conflictState={activeTab.conflictState} />
  ) : isDiffTab(activeTab) ? (
    <DiffViewer diffState={activeTab.diffState} />
  ) : isGitTab(activeTab) ? (
    <GitTab tab={activeTab} />
  ) : isRequestTab(activeTab) ? (
    <RequestPanel tab={activeTab} groupId={node.groupId} />
  ) : (
    <CollectionOverviewTab tab={activeTab} />
  )
) : (
  <EmptyState />
)}
```

- [ ] **Step 2: Verify it compiles**

Run: `npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/panes/EditorGroup.tsx
git commit -m "feat: route GitTab in EditorGroup for tabType 'git'"
```

---

## Task 3: Update TabItem to render git icon for git tabs

**Files:**
- Modify: `src/components/panes/TabItem.tsx`

- [ ] **Step 1: Add git tab icon rendering**

In `src/components/panes/TabItem.tsx`:

Add import:
```typescript
import { isGitTab } from '@/types/pane-types';
import { GitBranch } from 'lucide-react';
```

Update the icon rendering logic inside the `TabItem` component. Currently it checks `isRequestTab(tab)` for the method badge, and falls back to `Folder` icon. Add a `isGitTab` check between them:

```typescript
{isRequestTab(tab) ? (
  <span className={`font-semibold text-2xs shrink-0 ${METHOD_TEXT_COLORS[tab.request.method]}`}>
    {tab.request.method}
  </span>
) : isGitTab(tab) ? (
  <GitBranch className="h-3 w-3 shrink-0 text-muted-foreground" />
) : (
  <Folder className="h-3 w-3 shrink-0 text-primary" />
)}
```

- [ ] **Step 2: Verify it compiles**

Run: `npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/panes/TabItem.tsx
git commit -m "feat: render git branch icon for git tabs in TabItem"
```
