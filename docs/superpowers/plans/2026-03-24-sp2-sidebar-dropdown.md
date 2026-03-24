# Sidebar Dropdown Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Collections/History tabs with a shadcn Select dropdown and add + (new collection) and import icon buttons to its right.

**Architecture:** Single-file edit to `CollectionsSidebar.tsx`. Swap Tabs for Select + view state, add action icons conditionally, wire import button to Tauri file dialog.

**Tech Stack:** React, shadcn/ui (Select), Tailwind CSS, Tauri plugin-dialog, lucide-react

**Spec:** `docs/superpowers/specs/2026-03-24-sidebar-dropdown-design.md`

---

### File Structure

- Modify: `src/components/layout/CollectionsSidebar.tsx`

No new files. No new dependencies (shadcn Select and @tauri-apps/plugin-dialog already in the project).

---

### Task 1: Replace Tabs imports with Select imports and add view state

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx:44,46-58,506-509`

- [ ] **Step 1: Update imports**

Replace line 44:
```tsx
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
```
with:
```tsx
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
```

Add `Upload` to the lucide-react import (line 46-58). Add it after `Settings`:
```tsx
  Settings,
  Upload,
```

Add the Tauri dialog import after the existing tauri imports (after line 18):
```tsx
import { open } from '@tauri-apps/plugin-dialog';
```

- [ ] **Step 2: Add view state and handleImport**

Inside `CollectionsSidebar()` (after line 509, `const filter = ...`), add:

```tsx
const [view, setView] = useState<'collections' | 'history'>('collections');

const handleImport = useCallback(async () => {
  const file = await open({
    multiple: false,
    filters: [{ name: 'Collection', extensions: ['json'] }],
  });
  if (file) {
    console.log('Import file selected:', file);
  }
}, []);
```

- [ ] **Step 3: Verify build**

Run: `npx tsc --noEmit`
Expected: Exit 0 (unused imports from Tabs are removed, new imports resolve)

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "refactor(sidebar): swap Tabs imports for Select, add view state and import handler"
```

---

### Task 2: Replace Tabs JSX with Select dropdown, action icons, and conditional rendering

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx:625-709`

- [ ] **Step 1: Replace the return JSX**

Replace the entire return block (from `return (` through the closing `);` before the final `}`) with:

```tsx
  return (
    <div className="h-full flex flex-col bg-card/50 backdrop-blur-sm border-r border-border/50">
      {/* View selector and action icons. */}
      <div className="flex items-center gap-1 px-2 pt-2 pb-1">
        <Select value={view} onValueChange={(v) => setView(v as 'collections' | 'history')}>
          <SelectTrigger className="h-8 flex-1 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="collections">Collections</SelectItem>
            <SelectItem value="history">History</SelectItem>
          </SelectContent>
        </Select>
        {view === 'collections' && (
          <>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
              onClick={() => setIsCreating(true)}
              title="New Collection"
            >
              <Plus className="h-4 w-4" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
              onClick={() => void handleImport()}
              title="Import Collection"
            >
              <Upload className="h-4 w-4" />
            </Button>
          </>
        )}
      </div>

      {view === 'collections' ? (
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Search and inline create. */}
          <div className="px-2 pb-2 space-y-1.5">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="h-7 pl-7 text-xs"
                placeholder="Search requests..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                aria-label="Search collections"
              />
            </div>
            {isCreating && (
              <div className="px-1">
                <Input
                  autoFocus
                  className="h-7 text-xs"
                  placeholder="Collection name"
                  value={newName}
                  onChange={(e) => { setNewName(e.target.value); setCreateError(''); }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') handleCreateCollection();
                    if (e.key === 'Escape') { setIsCreating(false); setNewName(''); setCreateError(''); }
                  }}
                  onBlur={() => { setIsCreating(false); setNewName(''); setCreateError(''); }}
                />
                {createError && (
                  <p className="text-[10px] text-destructive mt-0.5 px-1">{createError}</p>
                )}
              </div>
            )}
          </div>

          {/* Collection tree. */}
          <ScrollArea className="flex-1">
            <div className="px-1 pb-2">
              {summaries.length === 0 ? (
                <p className="px-3 py-4 text-center text-xs text-muted-foreground">
                  No collections yet.
                </p>
              ) : (
                summaries.map((s) => (
                  <CollectionNode
                    key={s.name}
                    summary={s}
                    filter={filter}
                    summaries={summaries}
                    onNewRequest={handleNewRequest}
                    onNewFolder={handleNewFolder}
                    onMove={handleMove}
                    onDelete={setDeleteTarget}
                  />
                ))
              )}
            </div>
          </ScrollArea>
        </div>
      ) : (
        <div className="flex-1 overflow-hidden">
          <HistoryPanel />
        </div>
      )}

      <AlertDialog open={!!deleteTarget} onOpenChange={(open) => { if (!open) setDeleteTarget(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Confirm Delete</AlertDialogTitle>
            <AlertDialogDescription>
              {deleteTarget?.type === 'collection'
                ? `Delete collection '${deleteTarget.name}'? This removes all requests inside it.`
                : deleteTarget?.type === 'folder'
                ? `Delete folder '${deleteTarget.name}' and all requests inside it?`
                : `Delete request '${deleteTarget?.name}'?`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>Delete</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
```

Key changes from original:
- `Tabs`/`TabsList`/`TabsTrigger`/`TabsContent` replaced with `Select` + conditional rendering
- `+ New Collection` ghost button removed — functionality moved to `+` icon in top bar
- The `isCreating` inline input still renders below search (only hidden when not creating, instead of wrapped in an else branch with the button)
- `Upload` icon button for import, only shown in collections view
- `HistoryPanel` rendered directly in else branch

- [ ] **Step 2: Verify build**

Run: `npx tsc --noEmit`
Expected: Exit 0, no errors

- [ ] **Step 3: Verify visually**

Run the dev server and confirm:
1. Dropdown shows "Collections" by default with a chevron
2. Clicking dropdown reveals "Collections" and "History" options
3. `+` and import icons appear to the right of the dropdown
4. Switching to "History" hides the icons and shows the history panel
5. Switching back to "Collections" restores icons and collection tree
6. Clicking `+` shows the inline collection name input
7. Clicking import opens a native file picker for `.json` files

- [ ] **Step 4: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "feat(sidebar): replace tabs with dropdown, add new/import collection icons"
```

---

### Final State

The complete import section at the top of the file after all changes:

```tsx
import { useEffect, useState, useCallback } from 'react';
import {
  listCollections,
  getCollection,
  onCollectionChanged,
  createCollection,
  saveRequest,
  createFolder,
  deleteCollection,
  deleteFolder,
  deleteRequest,
  renameCollection,
  moveItem,
  type CollectionSummary,
  type Collection,
  type CollectionItem,
} from '@/lib/tauri-api';
import { open } from '@tauri-apps/plugin-dialog';
import { usePaneStore } from '@/stores/pane-store';
import { createDefaultRequest } from '@/lib/pane-utils';
import type { Tab, RequestState, PaneNode } from '@/types/pane-types';
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
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import {
  ChevronRight,
  ChevronDown,
  Folder,
  FolderOpen,
  FileText,
  Search,
  Plus,
  Copy,
  Trash2,
  FolderPlus,
  Settings,
  Upload,
} from 'lucide-react';
import { HistoryPanel } from '@/components/history/HistoryPanel';
```
