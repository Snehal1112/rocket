# Spec: Unify Git Tabs — Remove Collection-Level GitTab Pane

**Date:** 2026-03-31
**Status:** Draft
**Goal:** Remove the separate collection-level `GitTab` pane type and make the Git toolbar button navigate to the workspace's git section, so there is one single Git UI in the app.

---

## Problem

There are two independent Git UI implementations:

1. **`WorkspaceGitTab`** (`src/components/workspace/WorkspaceGitTab.tsx`) — the new Bruno-style two-panel layout (left: file list + commit form + links, right: landing/diff/commits/stashes). Opened by clicking the "Git" workspace tab.

2. **`GitTab`** (`src/components/git/GitTab.tsx`) — the old Tabs-based layout (Changes/Log/Stash sub-tabs with top bar). Opened by clicking "Open Git Panel" in the toolbar via `GitToolbarButton`.

These two UIs look completely different, have different feature sets (the old one lacks clone/remotes dialogs), and confuse users.

## Solution

Remove the old `GitTab` pane type entirely. The toolbar button navigates to the workspace's git section instead of opening a separate pane. This matches Bruno's approach: one Git UI, accessed through the workspace view.

---

## Changes

### 1. Always include the git tab in workspace tabs

**File:** `src/stores/pane-store.ts`

In `openWorkspaceTabs`, the git section is currently only added for non-default workspaces:

```typescript
const sections: WorkspaceTabSection[] = ['overview', 'environments'];
if (!isDefault) sections.push('git');
```

Change to always include git:

```typescript
const sections: WorkspaceTabSection[] = ['overview', 'environments', 'git'];
```

This ensures every workspace (including the default) has a git tab that the toolbar button can target.

### 2. Rewrite `GitToolbarButton` to activate the workspace git tab

**File:** `src/components/layout/GitToolbarButton.tsx`

Instead of creating a `GitTab` pane, the button should:

1. Find the workspace git tab by id pattern `workspace:{workspaceId}:git`
2. Activate it via `setActiveTab`

The button needs to know the active workspace. It currently reads `activeCollection` and `collectionPath` from the stores. Replace this with reading the active workspace from `useWorkspaceStore`.

If in workspace mode:
- Compute the git tab id: `workspace:${activeWorkspace.id}:git`
- Find which editor group contains it
- Call `setActiveTab(gitTabId, groupId)`

If not in workspace mode (collection mode):
- The button should be disabled or hidden, since there is no workspace git tab to navigate to

### 3. Remove `GitTab` pane type

**File:** `src/types/pane-types.ts`

- Delete the `GitTab` interface
- Remove `GitTab` from the `Tab` union type
- Delete the `isGitTab` guard function

### 4. Remove `GitTab` rendering from `EditorGroup`

**File:** `src/components/panes/EditorGroup.tsx`

- Remove the `import { GitTab } from '@/components/git/GitTab'` import
- Remove the `isGitTab` import
- Remove the `isGitTab(activeTab) ? <GitTab tab={activeTab} />` branch from the render logic

### 5. Delete old `GitTab` component

**File:** `src/components/git/GitTab.tsx`

Delete this file entirely. Its functionality is fully replaced by `WorkspaceGitTab`.

### 6. Delete orphaned components

After deleting `GitTab.tsx`, these components may have no remaining consumers:

- `src/components/git/GitRemoteActions.tsx` — check if anything still imports it
- `src/components/git/GitStagedFiles.tsx` — check if anything still imports it
- `src/components/git/GitChangedFiles.tsx` — check if anything still imports it
- `src/components/git/GitFileRow.tsx` — check if anything still imports it (used by GitStagedFiles/GitChangedFiles)
- `src/components/git/GitStatusBadge.tsx` — check if anything still imports it (used by GitFileRow)

For each: grep for imports. If only self-referencing or referenced by other deleted files, delete it.

### 7. Clean up `openDiffTab` and `openConflictTab` in pane store

**File:** `src/stores/pane-store.ts`

Check if `openDiffTab` and `openConflictTab` still reference `GitTab` or collection-path logic that assumed the old Git tab. If so, update them. These are used by the existing diff/conflict tab flows, which should continue to work independently.

---

## What stays unchanged

- `WorkspaceGitTab` component — no modifications needed
- All Plan 7 components (`GitLandingPanel`, `GitLinksSection`, `GitFileList`, `DiffViewForFile`)
- `BranchSelector` component — kept for potential future use
- Git store (`git-store.ts`) — unchanged
- All Tauri commands and Rust backend — unchanged
- `GitCommitForm`, `GitCommitLog`, `GitStashSection` — unchanged (used by WorkspaceGitTab)
- `DiffViewer`, `DiffHeader`, `ConflictResolver` — unchanged (used by DiffTab/ConflictTab panes)
- `GitCredentialsDialog`, `GitRemotesDialog`, `GitCloneDialog` — unchanged

---

## Verification

1. `npx tsc --noEmit` — no type errors
2. `yarn build` — builds successfully
3. `yarn lint` — no new errors
4. Grep for `GitTab` (as a type/component name, not `WorkspaceGitTab`) confirms zero references remain
5. Grep for deleted component names confirms zero remaining imports
