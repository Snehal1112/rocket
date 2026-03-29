# SP-W5: Sidebar Modes — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement single-workspace mode (default, current flat list) and multi-workspace mode (accordion sections with workspace-level actions) in the sidebar.

**Architecture:** New `WorkspaceSection` component. `CollectionsSidebar` conditionally renders flat or grouped layout based on `multiWorkspaceMode`. shadcn/ui only.

**Tech Stack:** TypeScript, React, Zustand, shadcn/ui, Lucide

**Spec:** `docs/superpowers/specs/2026-03-29-workspace-feature-design.md`

**Depends on:** SP-W4 complete

---

## Chunk 1: WorkspaceSection component

### Task 1: Create `WorkspaceSection` component — basic structure

**Files:**
- Create: `src/components/layout/WorkspaceSection.tsx`

- [ ] **Step 1:** Create the file. The component accepts props: `{ workspace: Workspace, children: React.ReactNode, collectionCount: number }`. It renders:
- A collapsible header row with: chevron (▼/▶), workspace icon (`LayoutDashboard`), workspace name (font-medium), collection count (muted)
- Clicking the chevron toggles expand/collapse (local `useState`)
- Clicking the workspace name calls `usePaneStore.getState().openWorkspaceTabs(workspace.id, workspace.id === 'default')`
- When expanded, renders `{children}` in a `<div className="pl-3">` wrapper
- On hover: show + button and ⋯ button (hidden by default, visible via `group-hover`)

Use imports: `useState` from react, `ChevronDown, ChevronRight, Plus, LayoutDashboard` from lucide-react, `Button` from `@/components/ui/button`, `useWorkspaceStore` and `usePaneStore`.

- [ ] **Step 2:** Commit: `git commit -m "feat(frontend): create WorkspaceSection component"`

---

### Task 2: Add context menu to `WorkspaceSection`

**Files:**
- Modify: `src/components/layout/WorkspaceSection.tsx`

- [ ] **Step 1:** Import `ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger` from `@/components/ui/context-menu` and `FolderOpen, Pencil, X` from `lucide-react`.

- [ ] **Step 2:** Wrap the header row `<div>` with `<ContextMenu><ContextMenuTrigger asChild>...<ContextMenuContent>`. Add these menu items:
- "Open workspace home" → calls `openWorkspaceTabs(workspace.id, workspace.id === 'default')`
- "Rename workspace" → calls store's `renameWorkspace` (or opens a rename dialog)
- "Show in folder" → invokes Tauri's `show_in_folder` with `workspace.path`
- separator
- "Close workspace" (destructive text color) → calls `closeWorkspace(workspace.id)`, disabled if `workspace.id === 'default'`

- [ ] **Step 3:** Commit: `git commit -m "feat(frontend): add context menu to WorkspaceSection"`

---

## Chunk 2: Sidebar conditional rendering

### Task 3: Update `CollectionsSidebar` header for multi-workspace mode

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1:** Read `multiWorkspaceMode` from the workspace store:

```typescript
const multiWorkspaceMode = useWorkspaceStore((s) => s.multiWorkspaceMode)
```

- [ ] **Step 2:** Find the sidebar "Collections" text label (the view tab button or header). When `multiWorkspaceMode` is true, change the text to "Workspaces".

- [ ] **Step 3:** Commit: `git commit -m "feat(sidebar): show 'Workspaces' header in multi-workspace mode"`

---

### Task 4: Render workspace sections in multi-workspace mode

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1:** Import `WorkspaceSection` from `./WorkspaceSection` and `useWorkspaceStore`.

- [ ] **Step 2:** Read `workspaces` from the workspace store:

```typescript
const workspaces = useWorkspaceStore((s) => s.workspaces)
```

- [ ] **Step 3:** Find the `ScrollArea` containing the collection tree (where `summaries.map((s) => <CollectionNode .../>)` is rendered). Wrap the entire tree rendering in a conditional:

```tsx
{multiWorkspaceMode ? (
  <div className="px-1 pb-2 space-y-1">
    {workspaces.map((ws) => (
      <WorkspaceSection
        key={ws.id}
        workspace={ws}
        collectionCount={ws.id === activeWorkspaceId ? summaries.length : 0}
      >
        {ws.id === activeWorkspaceId ? (
          summaries.map((s) => (
            <CollectionNode key={s.name} summary={s} ... />
          ))
        ) : (
          <div className="px-4 py-2 text-xs text-muted-foreground">
            Switch to this workspace to see collections
          </div>
        )}
      </WorkspaceSection>
    ))}
  </div>
) : (
  /* existing flat collection rendering — keep all existing JSX unchanged */
)}
```

Note: In v1, only the active workspace's collections are loaded. Other workspace sections show a "Switch to see collections" message. The `activeWorkspaceId` comes from `useWorkspaceStore((s) => s.activeWorkspaceId)`. Multi-workspace collection loading (loading collections per workspace independently) is a future enhancement.

- [ ] **Step 4:** Commit: `git commit -m "feat(sidebar): conditional rendering for single vs multi-workspace mode"`

---

## Chunk 3: Multi-workspace mode toggle

### Task 5: Add multi-workspace mode toggle to settings (placeholder)

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx` (or a settings component if one exists)

- [ ] **Step 1:** The subagent should find if a settings/preferences component exists. Search for files like `SettingsDialog.tsx`, `PreferencesDialog.tsx`, or similar in `src/components/`. If found, add a toggle switch for "Multi-workspace mode" that calls `useWorkspaceStore.getState().setMultiWorkspaceMode(enabled)`.

If no settings component exists, add a temporary toggle in the sidebar header area (as a small icon button) that toggles the mode:

```tsx
<Button
  variant="ghost"
  size="icon"
  className="h-6 w-6"
  onClick={() => {
    const store = useWorkspaceStore.getState()
    store.setMultiWorkspaceMode(!store.multiWorkspaceMode)
  }}
  title={multiWorkspaceMode ? 'Switch to single workspace mode' : 'Switch to multi-workspace mode'}
>
  <Layers className="h-3.5 w-3.5" />
</Button>
```

Import `Layers` from `lucide-react`.

- [ ] **Step 2:** Commit: `git commit -m "feat(frontend): add multi-workspace mode toggle"`
