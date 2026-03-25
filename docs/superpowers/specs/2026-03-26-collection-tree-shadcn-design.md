# Collection Tree — shadcn Tree Migration Design

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Status:** Approved

## Overview

Migrate the hand-rolled collection tree in `CollectionsSidebar.tsx` to the shadcn Tree registry component, with UX improvements to interactions and drag-and-drop reordering within collections.

## Goals

- Replace custom expand/collapse state and keyboard navigation with shadcn Tree primitives.
- Consolidate scattered hover action buttons into a single "..." `DropdownMenu` per node.
- Add within-collection drag-and-drop reordering using dnd-kit.
- Improve visual clarity: indentation guides, consistent `Badge` for HTTP method labels.
- Split the 1075-line `CollectionsSidebar.tsx` into focused, single-responsibility files.

## Non-Goals

- Cross-collection drag-and-drop (still handled via "Move to..." menu item).
- Cross-folder drag-and-drop within the same collection.
- Adding tests (no existing test infrastructure).
- Importing collections (existing stub, out of scope).

## Prerequisites — Install Steps

Before implementing, run:

```bash
yarn add @dnd-kit/core @dnd-kit/sortable
npx shadcn add tree
```

Neither package is currently in `package.json` and `src/components/ui/tree.tsx` does not exist yet.

## New Dependencies

| Package | Purpose |
|---------|---------|
| `@dnd-kit/core` | Drag-and-drop primitives |
| `@dnd-kit/sortable` | Sortable list strategy |
| shadcn `tree` | Tree/TreeItem primitives (via registry) |

## File Structure

```
src/components/collections/
  CollectionsSidebar.tsx     // top-level state, search, create/delete dialogs, handler callbacks
  CollectionNode.tsx         // collection row — expand/collapse, DnD root context, data fetching
  FolderNode.tsx             // folder row — sortable children
  RequestNode.tsx            // leaf request row
  tree-utils.ts              // methodColor(), isActiveRequest(), DeleteTarget type

src/components/ui/
  tree.tsx                   // installed via `npx shadcn add tree`
```

## Architecture

### CollectionsSidebar

Owns: `summaries` list, `searchQuery` state, `isCreating` / `newName` / `createError` state, `deleteTarget` state, the shared `AlertDialog`, and all handler callbacks (`handleNewRequest`, `handleNewFolder`, `handleMove`, `handleDuplicate`, `handleDelete`). Passes handlers as props to `CollectionNode`.

Removes: `handleTreeKeyDown` (replaced by Tree's built-in roving focus), inline `CollectionNode` / `FolderNode` / `RequestNode` definitions.

**Retains its own `onCollectionChanged` subscription** for refreshing the top-level `summaries` list (collection created, deleted, renamed). This is distinct from the per-collection listener in `CollectionNode.tsx` which refreshes the tree contents. Both listeners coexist; removing the sidebar-level one would break collection creation and deletion updates.

**`handleNewRequest` update required:** The current handler ignores `folderPath` and always creates a draft tab at the collection root. It must be updated to pass `folderPath` through to `newDraftTab` so that "New Request" from a `FolderNode` dropdown creates the draft pre-linked to the correct folder.

### shadcn Tree Integration

```
<Tree value={selectedId} onValueChange={setSelectedId}>
  <CollectionNode>               // <TreeItem value={summary.uid}>
    <FolderNode>                 //   <TreeItem value={folderPath}>
      <RequestNode />            //     <TreeItem value={uid}>
    </FolderNode>
  </CollectionNode>
</Tree>
```

- `TreeItem` manages expand/collapse state via `open`/`onOpenChange` props.
- `useState(expanded)` in collection and folder nodes is removed.
- When `filter` is active, all items are forced open via the controlled `open` prop; when cleared, nodes return to their last user-set open state.
- Built-in roving focus replaces the `data-sidebar-item` querySelector keyboard trick.

### CollectionNode — Data Fetching

`CollectionNode.tsx` owns the async data-fetching responsibility that currently lives inline in `CollectionsSidebar.tsx`:

- Calls `getCollection(summary.name)` when first expanded.
- Subscribes to `onCollectionChanged` for its collection (debounced 300 ms) and refreshes the tree on change.
- Cleans up the listener on unmount.

This is identical to the current behavior — ownership is simply moved to the dedicated file.

### Node UX

**"..." action menu** — each node renders a single `DropdownMenu` trigger (three-dot icon) on hover, replacing all current per-node hover button clusters.

| Node | Actions |
|------|---------|
| Collection | New Request, New Folder, Rename, Overview, Delete |
| Folder | New Request, New Folder, Rename, Delete |
| Request | Duplicate, Rename, Move to..., Delete |

**Right-click ContextMenu** is kept with identical items as a power-user shortcut.

**Double-click on CollectionNode:** The current double-click-to-open-Overview behavior (`CollectionTab`) is preserved. `TreeItemContent` receives an `onDoubleClick` handler that opens the `CollectionTab`, identical to today. The click-delay / `clickTimer` trick for separating single-click (expand) from double-click (overview) is retained inside `CollectionNode.tsx`.

**Inline rename** — Rename in the dropdown or context menu swaps the label for a focused `Input`, confirmed on Enter or blur. Behavior unchanged.

**Folder rename uses `moveItem`:** There is no `renameFolder` Tauri command. Renaming a folder is done by calling `moveItem(collection, oldPath, collection, newPath)` where `newPath` replaces the last path segment with the new name. This must be preserved in `FolderNode.tsx`.

**Active tab highlight** — `isActiveRequest()` drives a highlighted background on the active `RequestNode`. Unchanged.

**Visual clarity:**
- Folder children indented with a subtle `border-l border-border/30` guide line.
- Request method label uses the existing `Badge` component (consistent with `RequestList.tsx`).
- Collection request count rendered as a `Badge` instead of raw text.

### Drag-and-Drop

**Scope:** Within a single collection only. Root-level items sort among themselves; items inside a folder sort among that folder's children. No cross-folder or cross-collection drag.

**Stack:** `@dnd-kit/core` + `@dnd-kit/sortable` with `verticalListSortingStrategy`.

**Structure:**
- Each `CollectionNode` wraps its children in a `SortableContext`.
- Each `FolderNode` wraps its children in a nested `SortableContext`.
- `RequestNode` and `FolderNode` each call `useSortable(id)`.

**Persisting order — new Tauri command required:** The existing `moveItem` API only moves an item from one path to another; it cannot express an ordinal position within a sibling list. A new Tauri backend command `reorder_items(collection: str, folder_path: str, ordered_names: Vec<str>)` must be added. On `onDragEnd`, the full new sibling order is sent to this command, which persists order (e.g., via an `_order.json` sidecar in each folder, or by renaming files with numeric prefixes). The implementation plan must define the persistence strategy and implement the backend command before the frontend DnD can be wired up.

**Visual feedback:**
- Dragged item ghost: `DragOverlay` renders a semi-transparent clone of the item that follows the cursor (`opacity-50`).
- Drop position indicator: a conditional 2 px accent-colored `<div>` rendered between sibling items when `over?.id` matches that gap — distinct from `DragOverlay`.
- Drag handle: the existing chevron/folder icon area.

**Disabled when filter is active** — `useSortable` receives `disabled: !!filter` to prevent reordering a filtered view.

## Error Handling & Edge Cases

| Scenario | Handling |
|----------|---------|
| `reorderItems` fails after drag | File watcher triggers tree refresh; tree reverts to filesystem state automatically |
| Rename conflict | Rename input stays open; error logged to console |
| Filter active + drag | DnD disabled via `useSortable({ disabled: !!filter })` |
| Empty collection | Existing empty-state illustration unchanged |
