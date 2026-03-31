# Plan 11: Collection-Level Git Tab (Bruno-Style)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Git toolbar button open a Git tab alongside collection tabs (like Bruno), instead of switching to workspace mode which destroys open request tabs.

**Architecture:** Extract the two-panel Git layout from `WorkspaceGitTab` into a shared `GitPanel` component that accepts a `collectionPath`. Re-add the `GitTab` pane type so the toolbar button opens a closable Git tab in collection mode. Both `WorkspaceGitTab` and `EditorGroup`'s GitTab branch delegate to `GitPanel`.

**Tech Stack:** React, TypeScript, Zustand

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-tab-collection-level-design.md`

**Depends on:** Plans 7–10

---

## Chunk 1: Extract GitPanel and re-add GitTab pane type

### Task 1: Re-add `GitTab` pane type

**Files:**
- Modify: `src/types/pane-types.ts`

- [ ] **Step 1: Add GitTab interface and isGitTab guard**

In `src/types/pane-types.ts`, add the `GitTab` interface just before the `Tab` union type (before line 77):

```typescript
export interface GitTab extends BaseTab {
  tabType: 'git';
  collectionName: string;
  collectionPath: string;
}
```

Update the `Tab` union to include `GitTab`:

```typescript
export type Tab = RequestTab | CollectionTab | WorkspaceTab | DiffTab | ConflictTab | GitTab;
```

Add the `isGitTab` guard function after the `isConflictTab` function:

```typescript
export function isGitTab(tab: Tab): tab is GitTab {
  return tab.tabType === 'git';
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/types/pane-types.ts
git commit -m "feat(frontend): re-add GitTab pane type for collection-level git"
```

### Task 2: Extract `GitPanel` from `WorkspaceGitTab`

**Files:**
- Create: `src/components/git/GitPanel.tsx`
- Modify: `src/components/workspace/WorkspaceGitTab.tsx`

- [ ] **Step 1: Create `src/components/git/GitPanel.tsx`**

Move all the logic from `WorkspaceGitTab` into a new `GitPanel` component. The only difference is that `GitPanel` accepts `collectionPath` and `collectionName` as props instead of resolving them from the workspace store.

```typescript
import { useState, useEffect, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { GitCommitForm } from '@/components/git/GitCommitForm';
import { GitCommitLog } from '@/components/git/GitCommitLog';
import { GitStashSection } from '@/components/git/GitStashSection';
import { GitCredentialsDialog } from '@/components/git/GitCredentialsDialog';
import { GitRemotesDialog } from '@/components/git/GitRemotesDialog';
import { GitCloneDialog } from '@/components/git/GitCloneDialog';
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { GitLinksSection } from '@/components/git/GitLinksSection';
import { GitFileList } from '@/components/git/GitFileList';
import { DiffViewForFile } from '@/components/git/DiffViewForFile';
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible';
import { useGitStore } from '@/stores/git-store';
import { gitInit, gitIsRepo } from '@/lib/tauri-api';
import { Package, ChevronDown } from 'lucide-react';
import type { FileStatus } from '@/lib/tauri-api';

type RightPanelView =
  | { kind: 'landing' }
  | { kind: 'diff'; file: FileStatus }
  | { kind: 'commits' }
  | { kind: 'stashes' };

interface GitPanelProps {
  collectionPath: string;
  collectionName: string;
}

export function GitPanel({ collectionPath, collectionName }: GitPanelProps) {
  // null = loading, false = not a repo, true = is a repo.
  const [isRepo, setIsRepo] = useState<boolean | null>(null);
  const [rightPanel, setRightPanel] = useState<RightPanelView>({ kind: 'landing' });
  const [showRemotesDialog, setShowRemotesDialog] = useState(false);
  const [showCloneDialog, setShowCloneDialog] = useState(false);
  const [changesOpen, setChangesOpen] = useState(true);

  const { showCredentialsDialog, setCollection, refreshLog } = useGitStore();

  // Check git repo status and initialize the git store when the path is known.
  const checkAndLoad = useCallback(async (path: string) => {
    setIsRepo(null);
    try {
      const repo = await gitIsRepo(path);
      setIsRepo(repo);
      if (repo) {
        await setCollection(path);
      }
    } catch {
      setIsRepo(false);
    }
  }, [setCollection]);

  useEffect(() => {
    void checkAndLoad(collectionPath);
  }, [collectionPath, checkAndLoad]);

  // Load the commit log when the commits view is opened.
  useEffect(() => {
    if (rightPanel.kind === 'commits') void refreshLog();
  }, [rightPanel.kind, refreshLog]);

  if (isRepo === null) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        Loading...
      </div>
    );
  }

  if (!isRepo) {
    return (
      <div className="flex flex-col items-center justify-center gap-3 h-full px-4 text-center">
        <p className="text-sm text-muted-foreground">
          This collection is not a Git repository.
        </p>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              await gitInit(collectionPath);
              await checkAndLoad(collectionPath);
            }}
          >
            Initialize Git
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowCloneDialog(true)}
          >
            Clone Repository
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex overflow-hidden">

        {/* LEFT PANEL */}
        <div className="w-80 border-r border-border/70 flex flex-col overflow-hidden">

          {/* Collection name header */}
          <div className="flex items-center gap-2 px-3 py-2.5 border-b border-border/70 shrink-0">
            <Package className="h-3.5 w-3.5 text-muted-foreground" />
            <span className="text-sm font-medium truncate">
              {collectionName}
            </span>
          </div>

          {/* Changes section with commit form */}
          <div className="shrink-0 px-3 pt-3 pb-2 space-y-2 border-b border-border/70">
            <Collapsible open={changesOpen} onOpenChange={setChangesOpen}>
              <CollapsibleTrigger className="flex items-center gap-1 text-sm font-medium text-primary">
                <ChevronDown className={`h-3 w-3 transition-transform ${!changesOpen ? '-rotate-90' : ''}`} />
                Changes
              </CollapsibleTrigger>
              <CollapsibleContent className="pt-2 space-y-2">
                <GitCommitForm />
              </CollapsibleContent>
            </Collapsible>
          </div>

          {/* File list */}
          <GitFileList
            onFileClick={(file) => setRightPanel({ kind: 'diff', file })}
          />

          {/* Links section */}
          <div className="shrink-0 border-t border-border/70">
            <GitLinksSection
              onNavigate={(view) => setRightPanel({ kind: view })}
              onOpenRemotes={() => setShowRemotesDialog(true)}
            />
          </div>
        </div>

        {/* RIGHT PANEL */}
        <div className="flex-1 overflow-hidden">
          {rightPanel.kind === 'landing' && <GitLandingPanel />}
          {rightPanel.kind === 'diff' && (
            <DiffViewForFile file={rightPanel.file} collectionPath={collectionPath} />
          )}
          {rightPanel.kind === 'commits' && <GitCommitLog />}
          {rightPanel.kind === 'stashes' && (
            <ScrollArea className="h-full">
              <div className="p-4">
                <GitStashSection />
              </div>
            </ScrollArea>
          )}
        </div>

      </div>

      {/* Dialogs */}
      {showCredentialsDialog && <GitCredentialsDialog />}
      <GitRemotesDialog open={showRemotesDialog} onOpenChange={setShowRemotesDialog} />
      <GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
    </div>
  );
}
```

- [ ] **Step 2: Simplify `src/components/workspace/WorkspaceGitTab.tsx`**

Replace the entire file with a thin wrapper that resolves the workspace path and delegates to `GitPanel`:

```typescript
import { GitPanel } from '@/components/git/GitPanel';
import { useWorkspaceStore } from '@/stores/workspace-store';

interface WorkspaceGitTabProps {
  workspaceId: string;
}

export function WorkspaceGitTab({ workspaceId }: WorkspaceGitTabProps) {
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const workspace = workspaces.find((w) => w.id === workspaceId);
  const workspacePath = workspace?.path ?? null;

  if (!workspacePath) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        No workspace path configured.
      </div>
    );
  }

  return <GitPanel collectionPath={workspacePath} collectionName={workspace?.name ?? 'Collection'} />;
}
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitPanel.tsx src/components/workspace/WorkspaceGitTab.tsx
git commit -m "refactor(frontend): extract GitPanel from WorkspaceGitTab into shared component"
```

## Chunk 2: Wire up GitToolbarButton and EditorGroup

### Task 3: Rewrite `GitToolbarButton` to open GitTab pane

**Files:**
- Modify: `src/components/layout/GitToolbarButton.tsx`

- [ ] **Step 1: Replace file content**

Replace the entire file with:

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
    const tab: GitTab = {
      id: `git:${activeCollection}`,
      title: 'Git',
      tabType: 'git',
      collectionName: activeCollection,
      collectionPath: collectionPath ?? '',
      isDirty: false,
    };
    openTab(tab);
  };

  return (
    <Button
      variant="ghost"
      size="icon"
      className="h-7 w-7"
      onClick={handleClick}
      disabled={!activeCollection}
      title="Open Git panel"
    >
      <GitBranch className="h-4 w-4" />
    </Button>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/components/layout/GitToolbarButton.tsx
git commit -m "refactor(frontend): revert GitToolbarButton to open GitTab pane in collection mode"
```

### Task 4: Update `EditorGroup` and `TabItem` for GitTab

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`
- Modify: `src/components/panes/TabItem.tsx`

- [ ] **Step 1: Update `EditorGroup.tsx`**

Add the `GitPanel` import and `isGitTab` guard:

```typescript
// Add these imports:
import { GitPanel } from '@/components/git/GitPanel';
import { isDiffTab, isRequestTab, isConflictTab, isGitTab, isWorkspaceTab } from '@/types/pane-types';
```

(Replace the existing `isDiffTab, isRequestTab, isConflictTab, isWorkspaceTab` import to add `isGitTab`.)

Add the `isGitTab` rendering branch in the tab content chain, between `isRequestTab` and `isWorkspaceTab`:

```tsx
          ) : isRequestTab(activeTab) ? (
            <RequestPanel tab={activeTab} groupId={node.groupId} />
          ) : isGitTab(activeTab) ? (
            <GitPanel collectionPath={activeTab.collectionPath} collectionName={activeTab.collectionName} />
          ) : isWorkspaceTab(activeTab) ? (
```

- [ ] **Step 2: Update `TabItem.tsx`**

Add `isGitTab` to the import:

```typescript
import { isRequestTab, isGitTab, isWorkspaceTab } from '@/types/pane-types';
```

Add the `isGitTab` icon branch between the `isRequestTab` and `isWorkspaceTab` branches:

```tsx
      {isRequestTab(tab) ? (
        <span className={`font-semibold text-2xs shrink-0 ${METHOD_TEXT_COLOR[tab.request.method] ?? ''}`}>
          {tab.request.method}
        </span>
      ) : isGitTab(tab) ? (
        <GitBranch className="h-3 w-3 shrink-0 text-muted-foreground" />
      ) : isWorkspaceTab(tab) ? (
```

Note: `GitBranch` is already imported from lucide-react in this file. The close button is automatic — `GitTab` is not a workspace tab, so `!isWorkspaceTab(tab)` is true and the X button renders.

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Verify the app builds**

Run: `yarn build`
Expected: builds successfully

- [ ] **Step 5: Verify lint**

Run: `yarn lint`
Expected: no new errors

- [ ] **Step 6: Commit**

```bash
git add src/components/panes/EditorGroup.tsx src/components/panes/TabItem.tsx
git commit -m "feat(frontend): wire GitPanel into EditorGroup and TabItem for collection-level git tab"
```
