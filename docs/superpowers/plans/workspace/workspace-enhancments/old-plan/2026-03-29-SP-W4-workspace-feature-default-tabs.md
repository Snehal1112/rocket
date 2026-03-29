# SP-W4: Workspace Default Tabs — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement workspace default tabs (Overview, Environments, Git) with mutual exclusion from collection tabs, non-closable behavior, and tab content components.

**Architecture:** New `WorkspaceTab` type. Pane store mutual exclusion. Three tab content components. TabBar non-closable support. Default Workspace = 2 tabs, custom = 3 tabs.

**Tech Stack:** TypeScript, React, Zustand, shadcn/ui, Lucide

**Spec:** `docs/superpowers/specs/2026-03-29-workspace-feature-design.md`

**Depends on:** SP-W3 complete

---

## Chunk 1: WorkspaceTab type

### Task 1: Add `WorkspaceTab` interface to `pane-types.ts`

**Files:**
- Modify: `src/types/pane-types.ts`

- [ ] **Step 1:** Add after the existing `CollectionTab` interface:

```typescript
export type WorkspaceTabSection = 'overview' | 'environments' | 'git';

export interface WorkspaceTab extends BaseTab {
  tabType: 'workspace';
  workspaceId: string;
  activeSection: WorkspaceTabSection;
}
```

- [ ] **Step 2:** Update the `Tab` union type to include `WorkspaceTab`. Keep all existing types.

- [ ] **Step 3:** Add type guard:

```typescript
export function isWorkspaceTab(tab: Tab): tab is WorkspaceTab {
  return tab.tabType === 'workspace';
}
```

- [ ] **Step 4:** Commit: `git commit -m "feat(types): add WorkspaceTab type and isWorkspaceTab guard"`

---

## Chunk 2: Pane store workspace actions

### Task 2: Add `openWorkspaceTabs` action to pane store

**Files:**
- Modify: `src/stores/pane-store.ts`

- [ ] **Step 1:** Add `openWorkspaceTabs: (workspaceId: string, isDefault: boolean) => void` to the store interface.

- [ ] **Step 2:** Import `WorkspaceTab` and `WorkspaceTabSection` from `@/types/pane-types`.

- [ ] **Step 3:** Implement: calls `closeAll()`, creates 2 or 3 `WorkspaceTab` objects (Overview + Environments + optionally Git if `!isDefault`), sets them on the root leaf node with Overview as active. If root is a split, collapse to a single leaf first. Full implementation provided in the spec section 4.5.

- [ ] **Step 4:** Commit: `git commit -m "feat(pane-store): add openWorkspaceTabs action"`

---

### Task 3: Add `isWorkspaceMode` helper and mutual exclusion in `openTab`

**Files:**
- Modify: `src/stores/pane-store.ts`

- [ ] **Step 1:** Add `isWorkspaceMode: () => boolean` to interface. Implement: recursively check if any tab in the tree has `tabType === 'workspace'`.

- [ ] **Step 2:** At the very start of the existing `openTab` method body (before the `findTabInTree` check), add:

```typescript
if (tab.tabType !== 'workspace' && get().isWorkspaceMode()) {
  get().closeAll()
}
```

- [ ] **Step 3:** Commit: `git commit -m "feat(pane-store): add isWorkspaceMode and mutual exclusion"`

---

## Chunk 3: TabBar non-closable tabs

### Task 4: Hide close button for workspace tabs

**Files:**
- Modify: `src/components/panes/TabBar.tsx`

- [ ] **Step 1:** Import `isWorkspaceTab` from `@/types/pane-types`.

- [ ] **Step 2:** Find the close button (× icon) for each tab. Wrap it with `{!isWorkspaceTab(tab) && (...)}`.

- [ ] **Step 3:** Find "Close" and "Close Others" `ContextMenuItem`s. Add `disabled={isWorkspaceTab(tab)}`.

- [ ] **Step 4:** Commit: `git commit -m "feat(tabbar): hide close for workspace tabs"`

---

### Task 5: Add workspace tab icons to TabBar

**Files:**
- Modify: `src/components/panes/TabBar.tsx`

- [ ] **Step 1:** Import `{ LayoutDashboard, Globe, GitBranch }` from `lucide-react`.

- [ ] **Step 2:** Before the tab title text, add icon rendering based on `isWorkspaceTab(tab) && tab.activeSection`. Use `LayoutDashboard` for overview, `Globe` for environments, `GitBranch` for git. Each icon: `className="h-3.5 w-3.5 shrink-0 text-muted-foreground"`.

- [ ] **Step 3:** Commit: `git commit -m "feat(tabbar): add icons for workspace tabs"`

---

## Chunk 4: Tab content components

### Task 6: Create `WorkspaceOverviewTab` component

**Files:**
- Create: `src/components/workspace/WorkspaceOverviewTab.tsx`

- [ ] **Step 1:** Create the component. Reference `CollectionOverviewTab.tsx` for layout patterns. Props: `{ workspaceId: string }`. Must show: workspace name (h2), editable description textarea (saves on blur via `updateWorkspaceDescription`), info bar with path in monospace, collections list from `getWorkspaceConfig()`, embedded/external badges, "Add collection" dashed button, quick actions (Rename, Show in Folder). All shadcn/ui primitives only.

- [ ] **Step 2:** Commit: `git commit -m "feat(frontend): create WorkspaceOverviewTab component"`

---

### Task 7: Create `WorkspaceEnvironmentsTab` component

**Files:**
- Create: `src/components/workspace/WorkspaceEnvironmentsTab.tsx`

- [ ] **Step 1:** Create the component. Reference `EnvironmentDialog.tsx` for the environment editor UI. Render it inline (not in a dialog). Props: `{ workspaceId: string }`. Left panel (200px): env list + add/delete. Right panel: variable key-value editor with enabled/secret/delete per row. Use existing Tauri API calls for environments. All shadcn/ui primitives only.

- [ ] **Step 2:** Commit: `git commit -m "feat(frontend): create WorkspaceEnvironmentsTab component"`

---

### Task 8: Create `WorkspaceGitTab` component

**Files:**
- Create: `src/components/workspace/WorkspaceGitTab.tsx`

- [ ] **Step 1:** Create the component. Reference existing Git components (`GitPanel.tsx`, `GitTab.tsx`). Props: `{ workspaceId: string }`. Get workspace path from store. Check if Git repo via `gitStatus`. If not: show "Not a Git repository" + "Initialize Git" button. If yes: render Git UI (status, changed files, stage/unstage, commit). All shadcn/ui primitives only.

- [ ] **Step 2:** Commit: `git commit -m "feat(frontend): create WorkspaceGitTab component"`

---

## Chunk 5: EditorGroup and sidebar wiring

### Task 9: Render workspace tab content in EditorGroup

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`

- [ ] **Step 1:** Import `WorkspaceOverviewTab`, `WorkspaceEnvironmentsTab`, `WorkspaceGitTab`, and `isWorkspaceTab`.

- [ ] **Step 2:** In the content rendering ternary chain, add a check for `isWorkspaceTab(activeTab)` BEFORE existing checks. Route to the correct component based on `activeTab.activeSection`.

- [ ] **Step 3:** Commit: `git commit -m "feat(editor): render workspace tab content in EditorGroup"`

---

### Task 10: Add workspace home button to sidebar

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1:** Import `LayoutDashboard` from lucide-react, `useWorkspaceStore`, and ensure `usePaneStore` is imported.

- [ ] **Step 2:** Add a clickable row before the search input showing workspace icon + name. On click: `openWorkspaceTabs(activeWorkspace.id, activeWorkspace.id === 'default')`.

- [ ] **Step 3:** Commit: `git commit -m "feat(sidebar): add workspace home button"`

---

### Task 11: Open workspace tabs on workspace switch

**Files:**
- Modify: `src/stores/workspace-store.ts`

- [ ] **Step 1:** In the `workspace-switched` event listener, after `closeAll()` and state update, add: `usePaneStore.getState().openWorkspaceTabs(payload.id, payload.id === 'default')`.

- [ ] **Step 2:** Commit: `git commit -m "feat(frontend): open workspace tabs on workspace switch"`
