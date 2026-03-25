# Collection Tree — shadcn Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the hand-rolled collection tree to the shadcn Tree component with dnd-kit drag-and-drop reordering, "..." dropdown actions, and a split file structure.

**Architecture:** Install shadcn Tree + dnd-kit. Extract `CollectionNode`, `FolderNode`, `RequestNode` into separate files. Each node uses `TreeItem` for expand/collapse and `DropdownMenu` for actions. Each folder level (collection root + each folder) owns a `DndContext`+`SortableContext`; drag end calls a new `reorder_items` Tauri backend command that writes an `_order.json` sidecar file respected by `build_folder_tree`.

**Tech Stack:** React, TypeScript, shadcn/ui (tree, dropdown-menu, badge), @dnd-kit/core, @dnd-kit/sortable, Tauri (Rust backend via invoke), Zustand (pane-store)

**Spec:** `docs/superpowers/specs/2026-03-26-collection-tree-shadcn-design.md`

---

## File Map

| Action | Path |
|--------|------|
| Create | `src/components/collections/tree-utils.ts` |
| Create | `src/components/collections/RequestNode.tsx` |
| Create | `src/components/collections/FolderNode.tsx` |
| Create | `src/components/collections/CollectionNode.tsx` |
| Modify | `src/components/layout/CollectionsSidebar.tsx` |
| Modify | `src/lib/tauri-api.ts` |
| Modify | `crates/rocket-collection/src/repository.rs` |
| Modify | `crates/rocket-app/src/collection_service.rs` |
| Modify | `crates/rocket-infra/src/fs_collection_repo.rs` |
| Modify | `src-tauri/src/commands/collections.rs` |
| Modify | `src-tauri/src/lib.rs` |
| Auto-install | `src/components/ui/tree.tsx` (via `npx shadcn add tree`) |

---

## Task 1: Install Dependencies

**Files:** none modified yet

- [ ] **Step 1: Add dnd-kit packages**

```bash
cd /path/to/project
yarn add @dnd-kit/core @dnd-kit/sortable
```

Expected: packages added to `node_modules`, `yarn.lock` updated.

- [ ] **Step 2: Install shadcn tree component**

```bash
npx shadcn add tree
```

Expected: `src/components/ui/tree.tsx` created. If the CLI prompts, accept defaults.

- [ ] **Step 3: Read the installed tree.tsx to understand its API**

Open `src/components/ui/tree.tsx` and note the exported component names and props. The subsequent tasks reference `Tree`, `TreeItem`, and `TreeItemContent` — adjust names if the installed version differs.

- [ ] **Step 4: Verify the app still builds**

```bash
yarn build
```

Expected: build succeeds with no errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/tree.tsx package.json yarn.lock
git commit -m "chore: install shadcn tree and dnd-kit"
```

---

## Task 2: Create `tree-utils.ts`

Extract shared helpers and types out of `CollectionsSidebar.tsx` so the node files can import them without circular deps.

**Files:**
- Create: `src/components/collections/tree-utils.ts`

- [ ] **Step 1: Create the file**

```ts
import type { PaneNode } from '@/types/pane-types';

// Returns Tailwind text color class for an HTTP method.
export function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case 'GET':     return 'text-emerald-500';
    case 'POST':    return 'text-amber-500';
    case 'PUT':     return 'text-blue-500';
    case 'PATCH':   return 'text-violet-500';
    case 'DELETE':  return 'text-red-500';
    case 'OPTIONS': return 'text-cyan-500';
    case 'HEAD':    return 'text-pink-500';
    default:        return 'text-muted-foreground';
  }
}

// Returns true if any active tab in the pane tree matches the given tabId.
export function isActiveRequest(node: PaneNode, tabId: string): boolean {
  if (node.type === 'leaf') return node.activeTabId === tabId;
  return isActiveRequest(node.children[0], tabId) || isActiveRequest(node.children[1], tabId);
}

// Describes the item targeted for deletion in the shared confirmation dialog.
export type DeleteTarget = {
  type: 'collection' | 'folder' | 'request';
  collection: string;
  path?: string;
  name: string;
};
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/tree-utils.ts
git commit -m "refactor: extract tree-utils from CollectionsSidebar"
```

---

## Task 3: Create `RequestNode.tsx`

**Files:**
- Create: `src/components/collections/RequestNode.tsx`

This replaces the inline `RequestNode` in `CollectionsSidebar.tsx`. Key changes vs original:
- Uses `TreeItem` / `TreeItemContent` from shadcn tree.
- Replaces hover buttons with a single "..." `DropdownMenu`.
- Uses `Badge` for method label (consistent with `RequestList.tsx`).
- Registers with `useSortable` for dnd-kit (disabled when filter active).

- [ ] **Step 1: Create the file**

```tsx
import { useState } from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { MoreHorizontal, Copy, Trash2, GripVertical } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
// Import TreeItem and TreeItemContent from the installed tree component.
// Adjust the import path / names to match what npx shadcn add tree generated.
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { cn } from '@/lib/utils';
import { renameRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { mapApiRequestToState } from '@/lib/pane-utils';
import { methodColor, isActiveRequest } from './tree-utils';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import type { RequestTab, RequestState } from '@/types/pane-types';
import type { DeleteTarget } from './tree-utils';

// Badge color classes per HTTP method (matches RequestList.tsx).
const METHOD_BADGE: Record<string, string> = {
  GET:     'text-emerald-500 border-emerald-500/30 bg-emerald-500/10',
  POST:    'text-amber-500   border-amber-500/30   bg-amber-500/10',
  PUT:     'text-blue-500    border-blue-500/30    bg-blue-500/10',
  PATCH:   'text-violet-500  border-violet-500/30  bg-violet-500/10',
  DELETE:  'text-red-500     border-red-500/30     bg-red-500/10',
  OPTIONS: 'text-cyan-500    border-cyan-500/30    bg-cyan-500/10',
  HEAD:    'text-pink-500    border-pink-500/30    bg-pink-500/10',
};

interface RequestNodeProps {
  uid: string;
  name: string;
  method: string;
  collectionName: string;
  path: string;
  itemData: Extract<CollectionItem, { type: 'request' }>;
  summaries: CollectionSummary[];
  filter: string;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function RequestNode({
  uid, name, method, collectionName, path, itemData,
  summaries, filter, onMove, onDelete, onDuplicate,
}: RequestNodeProps) {
  const root = usePaneStore((s) => s.root);
  const active = isActiveRequest(root, uid);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);

  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: uid, disabled: !!filter });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) { setIsRenaming(false); return; }
    try {
      await renameRequest(collectionName, path, trimmed);
      setIsRenaming(false);
    } catch (err) {
      console.error('Rename request failed:', err);
    }
  };

  function handleClick() {
    const request: RequestState = mapApiRequestToState(itemData, true);
    const tab: RequestTab = {
      id: uid, title: name, tabType: 'request',
      request, response: null, isDirty: false,
      source: { collection: collectionName, path },
    };
    usePaneStore.getState().openTab(tab);
  }

  const badgeClass = METHOD_BADGE[method.toUpperCase()] ?? 'text-foreground border-border bg-muted';

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div ref={setNodeRef} style={style} className="group relative flex items-center">
          {/* Drag handle — grip icon, visible on hover. */}
          <button
            type="button"
            className="absolute left-0 h-full px-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing text-muted-foreground"
            {...attributes}
            {...listeners}
            tabIndex={-1}
          >
            <GripVertical className="h-3 w-3" />
          </button>

          {/* TreeItem wraps the row for accessible keyboard nav and selection. */}
          <TreeItem value={uid} className="flex-1">
            <TreeItemContent
              className={cn(
                'flex items-center gap-1.5 w-full px-2 pl-4 py-1 text-xs rounded-sm cursor-pointer',
                active && 'bg-accent/50 text-accent-foreground',
              )}
              onClick={handleClick}
              aria-label={`Open ${method} ${name}`}
            >
              <Badge variant="outline" className={cn('text-[10px] font-semibold w-14 justify-center shrink-0', badgeClass)}>
                {method}
              </Badge>
              {isRenaming ? (
                <Input
                  autoFocus
                  className="h-6 text-xs flex-1"
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void handleRename();
                    if (e.key === 'Escape') setIsRenaming(false);
                  }}
                  onBlur={() => void handleRename()}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <span className="truncate text-foreground">{name}</span>
              )}
            </TreeItemContent>
          </TreeItem>

          {/* "..." action menu, visible on hover. */}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground"
                onClick={(e) => e.stopPropagation()}
              >
                <MoreHorizontal className="h-3 w-3" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent className="w-48" onClick={(e) => e.stopPropagation()}>
              <DropdownMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>
                <Copy className="h-3.5 w-3.5 mr-2" /> Duplicate
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>
                Rename
              </DropdownMenuItem>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>Move to...</DropdownMenuSubTrigger>
                <DropdownMenuSubContent className="w-48">
                  {summaries.map((s) => (
                    <DropdownMenuItem
                      key={s.name}
                      onClick={() => void onMove(collectionName, path, s.name, '')}
                      disabled={s.name === collectionName}
                    >
                      {s.name}
                    </DropdownMenuItem>
                  ))}
                  {summaries.length === 0 && <DropdownMenuItem disabled>No collections</DropdownMenuItem>}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className="text-destructive"
                onClick={() => onDelete({ type: 'request', collection: collectionName, path, name })}
              >
                <Trash2 className="h-3.5 w-3.5 mr-2" /> Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>

      {/* Right-click context menu — same actions, power-user shortcut. */}
      <ContextMenuContent className="w-48">
        <ContextMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>Duplicate</ContextMenuItem>
        <ContextMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>Rename</ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>Move to...</ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-48">
            {summaries.map((s) => (
              <ContextMenuItem key={s.name} onClick={() => void onMove(collectionName, path, s.name, '')} disabled={s.name === collectionName}>
                {s.name}
              </ContextMenuItem>
            ))}
          </ContextMenuSubContent>
        </ContextMenuSub>
        <ContextMenuSeparator />
        <ContextMenuItem className="text-destructive" onClick={() => onDelete({ type: 'request', collection: collectionName, path, name })}>
          Delete
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Fix any import mismatches from the shadcn tree component API.

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/RequestNode.tsx
git commit -m "feat: add RequestNode with shadcn TreeItem and dropdown actions"
```

---

## Task 4: Create `FolderNode.tsx`

**Files:**
- Create: `src/components/collections/FolderNode.tsx`

Key changes vs original: uses `TreeItem` with controlled `open`, replaces hover buttons with "...", adds indentation guide, owns a `DndContext`+`SortableContext` for its children.

- [ ] **Step 1: Create the file**

```tsx
import { useState, useEffect } from 'react';
import { DndContext, DragOverlay, closestCenter, type DragEndEvent, type DragStartEvent } from '@dnd-kit/core';
import { SortableContext, verticalListSortingStrategy, arrayMove, useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Folder, FolderOpen, FolderPlus, Plus, Trash2, Pencil, GripVertical, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem,
  DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  ContextMenu, ContextMenuContent, ContextMenuItem,
  ContextMenuSeparator, ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { cn } from '@/lib/utils';
import { moveItem, reorderItems } from '@/lib/tauri-api';
import { RequestNode } from './RequestNode';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import type { DeleteTarget } from './tree-utils';

interface FolderNodeProps {
  name: string;
  items: CollectionItem[];
  collectionName: string;
  basePath: string;
  depth: number;
  filter: string;
  summaries: CollectionSummary[];
  onNewRequest: (collection: string, folderPath: string) => Promise<void>;
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function FolderNode({
  name, items, collectionName, basePath, depth, filter,
  summaries, onNewRequest, onNewFolder, onMove, onDelete, onDuplicate,
}: FolderNodeProps) {
  const [open, setOpen] = useState(depth < 2);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [localItems, setLocalItems] = useState(items);

  // Keep localItems in sync when parent refetches collection data.
  useEffect(() => { setLocalItems(items); }, [items]);

  // Auto-expand when filter is active.
  useEffect(() => { if (filter) setOpen(true); }, [filter]);

  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: basePath, disabled: !!filter });

  const style = { transform: CSS.Transform.toString(transform), transition, opacity: isDragging ? 0.5 : 1 };

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) { setIsRenaming(false); return; }
    // Folder rename is done by moving the folder to a new path (no rename_folder command).
    const parts = basePath.split('/');
    parts[parts.length - 1] = trimmed;
    const newPath = parts.join('/');
    try {
      await moveItem(collectionName, basePath, collectionName, newPath);
      setIsRenaming(false);
    } catch (err) {
      console.error('Rename folder failed:', err);
    }
  };

  const filteredItems = filter
    ? localItems.filter((item) => item.type !== 'request' || item.name.toLowerCase().includes(filter))
    : localItems;

  if (filter && filteredItems.length === 0) return null;

  // IDs for SortableContext — folders use basePath/name, requests use uid.
  const sortableIds = localItems.map((item) =>
    item.type === 'folder' ? `${basePath}/${item.name}` : item.uid
  );

  const handleDragStart = ({ active }: DragStartEvent) => setActiveId(String(active.id));
  const handleDragCancel = () => setActiveId(null);
  const handleDragEnd = async ({ active, over }: DragEndEvent) => {
    setActiveId(null);
    if (!over || active.id === over.id) return;
    const oldIdx = sortableIds.indexOf(String(active.id));
    const newIdx = sortableIds.indexOf(String(over.id));
    if (oldIdx === -1 || newIdx === -1) return;
    const reordered = arrayMove(localItems, oldIdx, newIdx);
    setLocalItems(reordered); // Optimistic update.
    const orderedNames = reordered.map((i) => i.type === 'request' ? (i.fileName ?? i.name) : i.name);
    try {
      await reorderItems(collectionName, basePath, orderedNames);
    } catch (err) {
      console.error('Reorder failed, reverting:', err);
      setLocalItems(items); // Revert on failure.
    }
  };

  const activeItem = activeId ? localItems.find((i) => (i.type === 'request' ? i.uid : `${basePath}/${i.name}`) === activeId) : null;

  return (
    <div ref={setNodeRef} style={style}>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="group relative flex items-center">
            <button
              type="button"
              className="absolute left-0 h-full px-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing text-muted-foreground"
              {...attributes} {...listeners} tabIndex={-1}
            >
              <GripVertical className="h-3 w-3" />
            </button>

            <TreeItem value={basePath} open={open} onOpenChange={setOpen} className="flex-1">
              <TreeItemContent className="flex items-center gap-1 w-full px-2 pl-4 py-1 text-xs rounded-sm cursor-pointer">
                {open ? <FolderOpen className="h-3.5 w-3.5 shrink-0 text-muted-foreground" /> : <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />}
                {isRenaming ? (
                  <Input
                    autoFocus className="h-6 text-xs flex-1"
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => { if (e.key === 'Enter') void handleRename(); if (e.key === 'Escape') setIsRenaming(false); }}
                    onBlur={() => void handleRename()}
                    onClick={(e) => e.stopPropagation()}
                  />
                ) : (
                  <span className="truncate font-medium text-foreground">{name}</span>
                )}
              </TreeItemContent>
            </TreeItem>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button type="button" className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground" onClick={(e) => e.stopPropagation()}>
                  <MoreHorizontal className="h-3 w-3" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent className="w-48" onClick={(e) => e.stopPropagation()}>
                <DropdownMenuItem onClick={() => void onNewRequest(collectionName, basePath)}><Plus className="h-3.5 w-3.5 mr-2" /> New Request</DropdownMenuItem>
                <DropdownMenuItem onClick={() => void onNewFolder(collectionName, basePath)}><FolderPlus className="h-3.5 w-3.5 mr-2" /> New Folder</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}><Pencil className="h-3.5 w-3.5 mr-2" /> Rename</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem className="text-destructive" onClick={() => onDelete({ type: 'folder', collection: collectionName, path: basePath, name })}><Trash2 className="h-3.5 w-3.5 mr-2" /> Delete</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuItem onClick={() => void onNewRequest(collectionName, basePath)}>New Request</ContextMenuItem>
          <ContextMenuItem onClick={() => void onNewFolder(collectionName, basePath)}>New Folder</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>Rename</ContextMenuItem>
          <ContextMenuItem className="text-destructive" onClick={() => onDelete({ type: 'folder', collection: collectionName, path: basePath, name })}>Delete</ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {open && (
        // Indentation guide line.
        <div className="pl-3 border-l border-border/30 ml-4">
          <DndContext
            collisionDetection={closestCenter}
            onDragStart={handleDragStart}
            onDragEnd={handleDragEnd}
            onDragCancel={handleDragCancel}
          >
            <SortableContext items={sortableIds} strategy={verticalListSortingStrategy}>
              {filteredItems.map((item, idx) => {
                if (item.type === 'folder') {
                  const folderPath = `${basePath}/${item.name}`;
                  return (
                    <FolderNode
                      key={`folder-${folderPath}`}
                      name={item.name} items={item.items}
                      collectionName={collectionName} basePath={folderPath}
                      depth={depth + 1} filter={filter} summaries={summaries}
                      onNewRequest={onNewRequest} onNewFolder={onNewFolder}
                      onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                    />
                  );
                }
                const fileName = item.fileName ?? item.name;
                const requestPath = `${basePath}/${fileName}`;
                return (
                  <RequestNode
                    key={`request-${requestPath}-${idx}`}
                    uid={item.uid} name={item.name} method={item.method}
                    collectionName={collectionName} path={requestPath}
                    itemData={item} summaries={summaries} filter={filter}
                    onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                  />
                );
              })}
            </SortableContext>
            <DragOverlay>
              {activeItem && activeItem.type === 'request' && (
                <div className="flex items-center gap-1.5 px-2 py-1 text-xs rounded-sm bg-card border border-border shadow-lg opacity-90">
                  <span className="text-muted-foreground">{activeItem.method}</span>
                  <span>{activeItem.name}</span>
                </div>
              )}
              {activeItem && activeItem.type === 'folder' && (
                <div className="flex items-center gap-1 px-2 py-1 text-xs rounded-sm bg-card border border-border shadow-lg opacity-90">
                  <Folder className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{activeItem.name}</span>
                </div>
              )}
            </DragOverlay>
          </DndContext>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Note: `reorderItems` does not exist in `tauri-api.ts` yet — add a temporary stub if needed to unblock compilation, or skip compilation until Task 7.

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/FolderNode.tsx
git commit -m "feat: add FolderNode with shadcn TreeItem, dropdown actions, and dnd-kit"
```

---

## Task 5: Create `CollectionNode.tsx`

**Files:**
- Create: `src/components/collections/CollectionNode.tsx`

Key behaviors to preserve: lazy `getCollection` fetch, `onCollectionChanged` subscription (debounced 300 ms), double-click → Overview tab (with click-delay pattern), `DndContext`+`SortableContext` for root items.

- [ ] **Step 1: Create the file**

```tsx
import { useState, useEffect, useCallback, useRef } from 'react';
import { DndContext, DragOverlay, closestCenter, type DragEndEvent, type DragStartEvent } from '@dnd-kit/core';
import { SortableContext, verticalListSortingStrategy, arrayMove } from '@dnd-kit/sortable';
import { Folder, FolderOpen, FolderPlus, Plus, Trash2, Settings, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem,
  DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  ContextMenu, ContextMenuContent, ContextMenuItem,
  ContextMenuSeparator, ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { cn } from '@/lib/utils';
import { getCollection, onCollectionChanged, renameCollection, reorderItems } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { FolderNode } from './FolderNode';
import { RequestNode } from './RequestNode';
import type { CollectionSummary, Collection, CollectionItem } from '@/lib/tauri-api';
import type { CollectionTab } from '@/types/pane-types';
import type { DeleteTarget } from './tree-utils';

interface CollectionNodeProps {
  summary: CollectionSummary;
  filter: string;
  summaries: CollectionSummary[];
  onNewRequest: (collection: string, folderPath: string) => Promise<void>;
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function CollectionNode({
  summary, filter, summaries,
  onNewRequest, onNewFolder, onMove, onDelete, onDuplicate,
}: CollectionNodeProps) {
  const [open, setOpen] = useState(false);
  const [collection, setCollection] = useState<Collection | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(summary.name);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [localItems, setLocalItems] = useState<CollectionItem[]>([]);
  const clickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const treeDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Keep localItems in sync when collection data is fetched/refreshed.
  useEffect(() => {
    if (collection) setLocalItems(collection.root.items);
  }, [collection]);

  const refreshTree = useCallback(() => {
    getCollection(summary.name)
      .then(setCollection)
      .catch((err) => console.error('[CollectionNode] fetch error', err));
  }, [summary.name]);

  // Fetch when first expanded.
  useEffect(() => {
    if (open && !collection) refreshTree();
  }, [open, collection, refreshTree]);

  // Per-collection change listener, active only when expanded.
  useEffect(() => {
    if (!open) return;
    let unlisten: (() => void) | undefined;
    onCollectionChanged((event) => {
      const affected = event.collection ?? event.name;
      if (!affected || affected === summary.name) {
        if (treeDebounce.current) clearTimeout(treeDebounce.current);
        treeDebounce.current = setTimeout(() => refreshTree(), 300);
      }
    }).then((fn) => { unlisten = fn; });
    return () => {
      unlisten?.();
      if (treeDebounce.current) clearTimeout(treeDebounce.current);
    };
  }, [open, refreshTree, summary.name]);

  // Auto-expand when filter is active.
  useEffect(() => { if (filter) setOpen(true); }, [filter]);

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === summary.name) { setIsRenaming(false); return; }
    try {
      await renameCollection(summary.name, trimmed);
      setIsRenaming(false);
    } catch (err) {
      console.error('Rename collection failed:', err);
    }
  };

  // Single click toggles expand after 250 ms (to allow double-click to fire).
  const handleClick = () => {
    if (clickTimer.current) { clearTimeout(clickTimer.current); clickTimer.current = null; return; }
    clickTimer.current = setTimeout(() => {
      clickTimer.current = null;
      setOpen((prev) => !prev);
    }, 250);
  };

  // Double click opens the collection Overview tab.
  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (clickTimer.current) { clearTimeout(clickTimer.current); clickTimer.current = null; }
    const tab: CollectionTab = {
      id: summary.uid, title: summary.name, tabType: 'collection',
      collectionName: summary.name, isDirty: false,
      source: { collection: summary.name, path: '' },
    };
    usePaneStore.getState().openTab(tab);
  };

  // DnD for root-level items.
  const sortableIds = localItems.map((item) => item.type === 'request' ? item.uid : item.name);

  const handleDragStart = ({ active }: DragStartEvent) => setActiveId(String(active.id));
  const handleDragCancel = () => setActiveId(null);
  const handleDragEnd = async ({ active, over }: DragEndEvent) => {
    setActiveId(null);
    if (!over || active.id === over.id) return;
    const oldIdx = sortableIds.indexOf(String(active.id));
    const newIdx = sortableIds.indexOf(String(over.id));
    if (oldIdx === -1 || newIdx === -1) return;
    const reordered = arrayMove(localItems, oldIdx, newIdx);
    setLocalItems(reordered);
    const orderedNames = reordered.map((i) => i.type === 'request' ? (i.fileName ?? i.name) : i.name);
    try {
      await reorderItems(summary.name, '', orderedNames);
    } catch (err) {
      console.error('Reorder failed, reverting:', err);
      if (collection) setLocalItems(collection.root.items);
    }
  };

  const activeItem = activeId ? localItems.find((i) => (i.type === 'request' ? i.uid : i.name) === activeId) : null;

  const filteredItems = filter
    ? localItems.filter((item) => item.type !== 'request' || item.name.toLowerCase().includes(filter))
    : localItems;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className="group relative flex items-center">
          <TreeItem value={summary.uid} open={open} onOpenChange={setOpen} className="flex-1">
            <TreeItemContent
              className="flex items-center gap-1.5 w-full px-2 py-1.5 text-xs rounded-sm cursor-pointer"
              onClick={handleClick}
              onDoubleClick={handleDoubleClick}
              aria-label={`${open ? 'Collapse' : 'Expand'} collection ${summary.name}`}
            >
              {open
                ? <FolderOpen className="h-4 w-4 shrink-0 text-primary" />
                : <Folder className="h-4 w-4 shrink-0 text-primary" />
              }
              {isRenaming ? (
                <Input
                  autoFocus className="h-6 text-xs flex-1"
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onKeyDown={(e) => { if (e.key === 'Enter') void handleRename(); if (e.key === 'Escape') setIsRenaming(false); }}
                  onBlur={() => void handleRename()}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <span className="truncate font-medium text-foreground">{summary.name}</span>
              )}
              <Badge variant="outline" className="ml-auto text-[10px] shrink-0">{summary.requestCount}</Badge>
            </TreeItemContent>
          </TreeItem>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button type="button" className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground" onClick={(e) => e.stopPropagation()}>
                <MoreHorizontal className="h-3 w-3" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent className="w-48" onClick={(e) => e.stopPropagation()}>
              <DropdownMenuItem onClick={handleDoubleClick as unknown as () => void}>Overview</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={async (e) => { await onNewRequest(summary.name, ''); setOpen(true); setCollection(null); }}>
                <Plus className="h-3.5 w-3.5 mr-2" /> New Request
              </DropdownMenuItem>
              <DropdownMenuItem onClick={async () => { await onNewFolder(summary.name, ''); setOpen(true); setCollection(null); }}>
                <FolderPlus className="h-3.5 w-3.5 mr-2" /> New Folder
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => setIsRenaming(true)}>Rename</DropdownMenuItem>
              <DropdownMenuItem className="text-destructive" onClick={() => onDelete({ type: 'collection', collection: summary.name, name: summary.name })}>
                <Trash2 className="h-3.5 w-3.5 mr-2" /> Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-48">
        <ContextMenuItem onClick={() => {
          const tab: CollectionTab = { id: summary.uid, title: summary.name, tabType: 'collection', collectionName: summary.name, isDirty: false, source: { collection: summary.name, path: '' } };
          usePaneStore.getState().openTab(tab);
        }}>Overview</ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => void onNewRequest(summary.name, '')}>New Request</ContextMenuItem>
        <ContextMenuItem onClick={() => void onNewFolder(summary.name, '')}>New Folder</ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => setIsRenaming(true)}>Rename</ContextMenuItem>
        <ContextMenuItem className="text-destructive" onClick={() => onDelete({ type: 'collection', collection: summary.name, name: summary.name })}>Delete</ContextMenuItem>
      </ContextMenuContent>

      {open && collection && (
        <div className="pl-2 border-l border-border/30 ml-3">
          <DndContext collisionDetection={closestCenter} onDragStart={handleDragStart} onDragEnd={handleDragEnd} onDragCancel={handleDragCancel}>
            <SortableContext items={sortableIds} strategy={verticalListSortingStrategy}>
              {filteredItems.map((item, idx) => {
                if (item.type === 'folder') {
                  return (
                    <FolderNode
                      key={`folder-${item.name}`}
                      name={item.name} items={item.items}
                      collectionName={summary.name} basePath={item.name}
                      depth={1} filter={filter} summaries={summaries}
                      onNewRequest={onNewRequest} onNewFolder={onNewFolder}
                      onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                    />
                  );
                }
                return (
                  <RequestNode
                    key={`request-${item.fileName ?? item.name}-${idx}`}
                    uid={item.uid} name={item.name} method={item.method}
                    collectionName={summary.name} path={item.fileName ?? item.name}
                    itemData={item} summaries={summaries} filter={filter}
                    onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                  />
                );
              })}
            </SortableContext>
            <DragOverlay>
              {activeItem && activeItem.type === 'request' && (
                <div className="flex items-center gap-1.5 px-2 py-1 text-xs rounded-sm bg-card border border-border shadow-lg opacity-90">
                  <span className="text-muted-foreground">{activeItem.method}</span>
                  <span>{activeItem.name}</span>
                </div>
              )}
              {activeItem && activeItem.type === 'folder' && (
                <div className="flex items-center gap-1 px-2 py-1 text-xs rounded-sm bg-card border border-border shadow-lg opacity-90">
                  <Folder className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{activeItem.name}</span>
                </div>
              )}
            </DragOverlay>
          </DndContext>
        </div>
      )}
    </ContextMenu>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/CollectionNode.tsx
git commit -m "feat: add CollectionNode with shadcn TreeItem, dropdown, and dnd-kit"
```

---

## Task 5b: Update `newDraftTab` to Accept `folderPath`

This must be done before Task 6 because Task 6 calls `newDraftTab` with a third argument.

**Files:**
- Modify: `src/stores/pane-store.ts`

- [ ] **Step 1: Update the `PaneState` interface**

Find line 59 in `src/stores/pane-store.ts`:
```ts
newDraftTab: (groupId?: string, defaultCollection?: string) => void;
```
Change to:
```ts
newDraftTab: (groupId?: string, defaultCollection?: string, defaultFolderPath?: string) => void;
```

- [ ] **Step 2: Update the implementation**

Find line 86:
```ts
newDraftTab(groupId, defaultCollection) {
  ...
  if (defaultCollection) {
    (tab as any).defaultCollection = defaultCollection;
  }
```
Change to:
```ts
newDraftTab(groupId, defaultCollection, defaultFolderPath) {
  ...
  if (defaultCollection) {
    (tab as any).defaultCollection = defaultCollection;
  }
  if (defaultFolderPath) {
    (tab as any).defaultFolderPath = defaultFolderPath;
  }
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

- [ ] **Step 4: Commit**

```bash
git add src/stores/pane-store.ts
git commit -m "feat: pass defaultFolderPath through newDraftTab"
```

---

## Task 6: Refactor `CollectionsSidebar.tsx`

Remove inline component definitions, add shadcn `Tree` wrapper, remove `handleTreeKeyDown`, update `handleNewRequest` to pass `folderPath`.

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Replace the file**

The new `CollectionsSidebar.tsx` has the same overall structure but:
1. Imports `CollectionNode` from `../collections/CollectionNode`.
2. Imports `DeleteTarget` from `../collections/tree-utils`.
3. Wraps the tree list in `<Tree>` from `@/components/ui/tree`.
4. Removes `methodColor`, `isActiveRequest`, `DeleteTarget`, `RequestNode`, `FolderNode`, `CollectionNode` inline definitions.
5. Removes `handleTreeKeyDown` and the `tabIndex`/`onKeyDown` on the scroll container.
6. Updates `handleNewRequest` to pass `folderPath` to `newDraftTab`.

Key changes as diffs:

**Remove** the `handleTreeKeyDown` callback (lines 875–922) entirely.

**Replace** `handleNewRequest`:
```tsx
// Before:
const handleNewRequest = useCallback(async (collection: string, _folderPath: string) => {
  usePaneStore.getState().newDraftTab(undefined, collection);
}, []);

// After:
const handleNewRequest = useCallback(async (collection: string, folderPath: string) => {
  usePaneStore.getState().newDraftTab(undefined, collection, folderPath || undefined);
}, []);
```
Note: update `newDraftTab` in `pane-store` to accept an optional third argument `folderPath` if it doesn't already. Check `src/stores/pane-store.ts` — if `newDraftTab` doesn't accept a folder path, add the parameter (it may be unused for now if the store doesn't use it yet, but the interface should accept it).

**Add `selectedId` state** near the top of the function (the shadcn `Tree` component needs a controlled selection value):
```tsx
const [selectedId, setSelectedId] = useState<string>('');
```

**Replace** the tree scroll area content:
```tsx
// Before:
<div className="px-1 pb-2" tabIndex={0} onKeyDown={handleTreeKeyDown}>
  {summaries.length === 0 ? (
    /* empty state */
  ) : (
    summaries.map((s) => (
      <CollectionNode key={s.name} ... />
    ))
  )}
</div>

// After — note the selectedId/onValueChange props on <Tree>:
<Tree value={selectedId} onValueChange={setSelectedId}>
  <div className="px-1 pb-2">
    {summaries.length === 0 ? (
      /* same empty state */
    ) : (
      summaries.map((s) => (
        <CollectionNode key={s.name} ... />
      ))
    )}
  </div>
</Tree>
```

If the installed `tree.tsx` does not use `value`/`onValueChange` props, check its actual API and adjust accordingly.

**Update imports** — remove all the inline component definitions and add:
```tsx
import { Tree } from '@/components/ui/tree';
import { CollectionNode } from '@/components/collections/CollectionNode';
import type { DeleteTarget } from '@/components/collections/tree-utils';
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

- [ ] **Step 3: Run the dev server and manually verify**

```bash
yarn tauri dev
```

Open the app. Verify: collections list shows, collections expand/collapse, requests open in tabs, context menus work, search filters, create/delete/rename collection dialogs work. DnD won't persist yet (Task 7).

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "refactor: split CollectionsSidebar into focused node components"
```

---

## Task 7: Add `reorder_items` Backend Command (Rust)

**Files:**
- Modify: `crates/rocket-collection/src/repository.rs`
- Modify: `crates/rocket-app/src/collection_service.rs`
- Modify: `crates/rocket-infra/src/fs_collection_repo.rs`
- Modify: `src-tauri/src/commands/collections.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/tauri-api.ts`

### Step 1: Add to `CollectionRepository` trait

In `crates/rocket-collection/src/repository.rs`, add after `move_item`:

```rust
/// Write an explicit ordering for items in a folder within a collection.
/// `folder_path` is relative to the collection root; pass `""` for the root.
/// `ordered_names` is the full ordered list of entry names (files include `.json`).
fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()>;
```

- [ ] **Step 1b: Verify Rust still compiles after trait change**

The trait now has a new required method — Rust will refuse to compile until the mock implements it. Run this before Step 2:

```bash
cargo check -p rocket-infra
```

Expected: error mentioning `reorder_items` not implemented on the mock. Proceed to Step 2 to add the stub.

- [ ] **Step 2: Add stub to test mock in `collection_service.rs`**

In `crates/rocket-app/src/collection_service.rs`, find the test mock that implements `CollectionRepository` (near the bottom, in `#[cfg(test)]`). Add:

```rust
fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> { Ok(()) }
```

### Step 3: Implement in `FsCollectionRepo`

In `crates/rocket-infra/src/fs_collection_repo.rs`:

**a) Update `is_request_file` to exclude `_order.json`:**

```rust
fn is_request_file(path: &Path) -> bool {
    if path.file_name().is_some_and(|n| n == "collection.json" || n == "_order.json") {
        return false;
    }
    path.extension().is_some_and(|ext| ext == "json" || ext == "bru")
}
```

**b) Update `build_folder_tree` to respect `_order.json`:**

Replace:
```rust
let mut entries: Vec<_> = fs::read_dir(current)?.filter_map(|e| e.ok()).collect();
entries.sort_by_key(|e| e.file_name());
```

With:
```rust
let mut entries: Vec<_> = fs::read_dir(current)?.filter_map(|e| e.ok()).collect();

// Apply explicit order from _order.json if present; fall back to alphabetical.
let order_path = current.join("_order.json");
if let Ok(content) = fs::read_to_string(&order_path) {
    if let Ok(ordered) = serde_json::from_str::<Vec<String>>(&content) {
        let pos: std::collections::HashMap<String, usize> = ordered
            .into_iter().enumerate().map(|(i, name)| (name, i)).collect();
        entries.sort_by(|a, b| {
            let ai = pos.get(&a.file_name().to_string_lossy().into_owned()).copied().unwrap_or(usize::MAX);
            let bi = pos.get(&b.file_name().to_string_lossy().into_owned()).copied().unwrap_or(usize::MAX);
            ai.cmp(&bi).then_with(|| a.file_name().cmp(&b.file_name()))
        });
    } else {
        entries.sort_by_key(|e| e.file_name());
    }
} else {
    entries.sort_by_key(|e| e.file_name());
}
```

**c) Add `reorder_items` implementation inside `impl CollectionRepository for FsCollectionRepo`:**

```rust
fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
    let collection_dir = self.collection_path(collection);
    let dir = if folder_path.is_empty() {
        collection_dir.clone()
    } else {
        self.validate_path(&collection_dir, std::path::Path::new(folder_path))?
    };
    if !dir.is_dir() {
        return Err(DomainError::NotFound(format!("{}/{}", collection, folder_path)));
    }
    let json = serde_json::to_string_pretty(ordered_names)?;
    fs::write(dir.join("_order.json"), json)?;
    Ok(())
}
```

- [ ] **Step 4: Add Tauri command to `collections.rs`**

```rust
#[tauri::command]
pub fn reorder_items(
    collection: String,
    folder_path: String,
    ordered_names: Vec<String>,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.reorder_items(&collection, &folder_path, &ordered_names)
}
```

Also add `reorder_items` to `CollectionService` in `crates/rocket-app/src/collection_service.rs`:

```rust
pub fn reorder_items(&self, collection: &str, folder_path: &str, ordered_names: &[String]) -> DomainResult<()> {
    self.repo.reorder_items(collection, folder_path, ordered_names)
}
```

- [ ] **Step 5: Register in `src-tauri/src/lib.rs`**

Find the `tauri::generate_handler![...]` block and add:

```rust
commands::collections::reorder_items,
```

- [ ] **Step 6: Add `reorderItems` to `src/lib/tauri-api.ts`**

After the `moveItem` function, add:

```ts
export const reorderItems = (
  collection: string,
  folderPath: string,
  orderedNames: string[],
): Promise<void> =>
  invoke<void>('reorder_items', { collection, folderPath, orderedNames });
```

- [ ] **Step 7: Add a Rust test for `reorder_items`**

In `crates/rocket-infra/src/fs_collection_repo.rs`, find the `#[cfg(test)]` block and add:

```rust
#[test]
fn reorder_items_writes_order_file_and_get_respects_it() {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsCollectionRepo::new(dir.path().to_path_buf());
    repo.create("test-col").unwrap();

    // Create two requests using the same constructor pattern as existing tests.
    let req = rocket_collection::Request::new("r", rocket_shared::types::HttpMethod::Get, "/r");
    repo.save_request("test-col", "b-request", &req).unwrap();
    repo.save_request("test-col", "a-request", &req).unwrap();

    // Default order is alphabetical: a-request, b-request.
    let col = repo.get("test-col").unwrap();
    let names: Vec<_> = col.root.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["a-request", "b-request"]);

    // Reorder: b first.
    repo.reorder_items("test-col", "", &["b-request.json".to_string(), "a-request.json".to_string()]).unwrap();
    let col2 = repo.get("test-col").unwrap();
    let names2: Vec<_> = col2.root.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names2, ["b-request", "a-request"]);
}
```

Note: check the `Request` struct's default values — use whatever fields are required, or look at existing tests in the file for the correct constructor pattern.

- [ ] **Step 8: Run Rust tests**

```bash
cargo test -p rocket-infra reorder_items
```

Expected: test passes.

- [ ] **Step 9: Build the full app**

```bash
yarn tauri build
```

or in dev mode:

```bash
yarn tauri dev
```

Expected: compiles with no errors.

- [ ] **Step 10: Commit**

```bash
git add crates/ src-tauri/src/commands/collections.rs src-tauri/src/lib.rs src/lib/tauri-api.ts
git commit -m "feat: add reorder_items backend command with _order.json sidecar"
```

---

## Task 8: Verify End-to-End

- [ ] **Step 1: Run the app**

```bash
yarn tauri dev
```

- [ ] **Step 2: Manually test drag-and-drop**

1. Open a collection with at least 3 requests.
2. Drag a request to a different position.
3. Verify the drop indicator line appears between items while dragging.
4. Verify the dragged item ghost follows the cursor.
5. Release the drag — verify the item lands in the new position.
6. Close and reopen the collection — verify the new order is persisted.

- [ ] **Step 3: Manually test UX improvements**

1. Hover over a request — verify "..." icon appears; click it — verify Duplicate, Rename, Move to..., Delete.
2. Hover over a folder — verify "..." icon; click it — verify New Request, New Folder, Rename, Delete.
3. Right-click a request — verify context menu still works.
4. Double-click a collection row — verify the Overview tab opens.
5. Type in the search box — verify all nodes auto-expand and filter; verify drag is disabled while filter is active.
6. Rename a request inline.
7. Rename a folder inline.
8. Delete a request — verify confirmation dialog and tab closes.

- [ ] **Step 4: Commit any fixes found during testing**

```bash
git add -p
git commit -m "fix: <describe any issues found>"
```

---

## Notes for Implementers

- **shadcn tree API:** The plan references `Tree`, `TreeItem`, `TreeItemContent` — match these to whatever `npx shadcn add tree` actually installs. The shadcn registry component may differ; read `src/components/ui/tree.tsx` before writing node code.
- **`_order.json` naming:** Item names in `orderedNames` must include the `.json` extension for files (e.g., `"my-request.json"`) and no extension for folders (e.g., `"my-folder"`). This matches how `build_folder_tree` reads `entry.file_name()`.
- **DnD and shadcn Tree:** If the shadcn `TreeItem` intercepts pointer events in a way that conflicts with dnd-kit's sensor, configure the sensor with an activation constraint (e.g., `useSensor(PointerSensor, { activationConstraint: { distance: 8 } })`) to prevent accidental drag on click.
- **Drop indicator between items:** Add `overId` state (`const [overId, setOverId] = useState<string | null>(null)`) to both `FolderNode` and `CollectionNode`. Add `onDragOver={({ over }) => setOverId(over ? String(over.id) : null)}` to each `DndContext`. Between items in the render list, insert `{overId === sortableIds[idx] && activeId !== sortableIds[idx] && <div className="h-0.5 bg-primary rounded mx-2 my-0.5" />}` to show the 2px accent line before the hovered target.
- **Stale `_order.json` after folder delete:** When a folder is deleted, `fs::remove_dir_all` cleans up `_order.json` files inside it. The parent folder's `_order.json` may still reference the deleted folder's name. This is harmless — `build_folder_tree` assigns `usize::MAX` to unknown names and they simply don't appear in directory entries. No cleanup is needed.
- **`filter` prop on `RequestNode`:** The plan's `RequestNodeProps` interface declares `filter: string` as required. Verify that every `RequestNode` call site in `FolderNode.tsx` and `CollectionNode.tsx` passes `filter={filter}`. If the TypeScript compiler reports a missing prop, add it to the call site.
