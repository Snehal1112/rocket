# Spec: Collection-Level Git Tab (Bruno-Style)

**Date:** 2026-03-31
**Status:** Draft
**Goal:** Make the Git toolbar button open a Git tab alongside collection tabs (like Bruno), instead of switching to workspace mode which destroys the user's open request tabs.

---

## Problem

Clicking the Git toolbar button calls `openWorkspaceTabs()`, which closes all open collection/request tabs and switches to workspace mode. This is destructive — the user loses their working context. Bruno keeps collection tabs open and adds the Git UI as a peer tab.

## Solution

1. **Re-add `GitTab` pane type** to `pane-types.ts` with `collectionPath` field.
2. **Extract a shared `GitPanel` component** from `WorkspaceGitTab` — accepts a `collectionPath` string prop and renders the two-panel Git layout. Both `WorkspaceGitTab` and `EditorGroup`'s GitTab branch use it.
3. **Revert `GitToolbarButton`** to create a `GitTab` pane via `openTab()`, which opens alongside collection tabs without mode switching.
4. **Update `EditorGroup`** to render `GitPanel` for `isGitTab` tabs.
5. **Update `TabItem`** to show `GitBranch` icon for git tabs, with close button (it's a collection-level tab, not a workspace tab).

---

## Changes

### 1. `src/types/pane-types.ts` — Re-add GitTab

```typescript
export interface GitTab extends BaseTab {
  tabType: 'git';
  collectionName: string;
  collectionPath: string;
}

export type Tab = RequestTab | CollectionTab | WorkspaceTab | DiffTab | ConflictTab | GitTab;

export function isGitTab(tab: Tab): tab is GitTab {
  return tab.tabType === 'git';
}
```

### 2. `src/components/git/GitPanel.tsx` — Extract shared component

Extract the core two-panel Git layout from `WorkspaceGitTab` into a new `GitPanel` component:

```typescript
interface GitPanelProps {
  collectionPath: string;
  collectionName: string;
}
```

This component contains:
- The `checkAndLoad` logic (check if path is a git repo, load git store)
- Loading / not-a-repo / clone states
- The two-panel layout (left: commit form + file list + links, right: landing/diff/commits/stashes)
- All dialog rendering (credentials, remotes, clone)

### 3. `src/components/workspace/WorkspaceGitTab.tsx` — Thin wrapper

Simplify to resolve `collectionPath` and `collectionName` from the workspace store, then render `<GitPanel>`:

```typescript
export function WorkspaceGitTab({ workspaceId }: WorkspaceGitTabProps) {
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const workspace = workspaces.find((w) => w.id === workspaceId);

  if (!workspace?.path) {
    return <div>No workspace path configured.</div>;
  }

  return <GitPanel collectionPath={workspace.path} collectionName={workspace.name ?? 'Collection'} />;
}
```

### 4. `src/components/layout/GitToolbarButton.tsx` — Open GitTab pane

Revert to creating a `GitTab` pane alongside collection tabs:

```typescript
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
```

The button is enabled when `activeCollection` exists (collection mode). In workspace mode, the Git tab is already available as a workspace tab.

### 5. `src/components/panes/EditorGroup.tsx` — Render GitPanel for GitTab

Add back the `isGitTab` branch:

```tsx
) : isGitTab(activeTab) ? (
  <GitPanel collectionPath={activeTab.collectionPath} collectionName={activeTab.collectionName} />
)
```

### 6. `src/components/panes/TabItem.tsx` — GitTab icon + close button

Add back `isGitTab` branch for the icon. Since `GitTab` is NOT a workspace tab, it automatically gets the close button (line 62: `!isWorkspaceTab(tab)`).

---

## What stays unchanged

- `GitPanel` internals (the two-panel layout, all git components)
- Workspace mode Git tab (still works, now delegates to GitPanel)
- Git store, Tauri commands, Rust backend
- All Plan 7 components (GitLandingPanel, GitLinksSection, GitFileList, etc.)

---

## Verification

1. `npx tsc --noEmit` — no type errors
2. `yarn build` — builds successfully
3. `yarn lint` — no new errors
4. Git toolbar button opens Git tab alongside collection tabs (no mode switching)
5. Workspace mode Git tab still works
6. Git tab is closable in collection mode
