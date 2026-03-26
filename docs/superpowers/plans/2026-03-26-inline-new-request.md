# Inline New Request Creation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the buggy draft-tab-then-save-dialog flow with inline tree creation — user types a name in the tree, the request saves to disk immediately, and opens in a tab.

**Architecture:** Add inline `<Input>` creation to CollectionNode and FolderNode (reusing the existing rename input pattern), then remove the entire draft tab infrastructure: `newDraftTab` store action, `createDefaultTab` utility, `'draft'` tabType, `SaveToCollectionDialog`, non-tree + buttons, and Ctrl+N shortcut. Every tab becomes file-backed from birth.

**Tech Stack:** React, TypeScript, Tailwind CSS, Zustand, Tauri (`yarn tsc --noEmit` for verification, `yarn build` for final check)

**Spec:** `docs/superpowers/specs/2026-03-26-inline-new-request-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/components/collections/CollectionNode.tsx` | Add inline `<Input>` for new request; remove `onNewRequest` prop |
| `src/components/collections/FolderNode.tsx` | Add inline `<Input>` for new request; remove `onNewRequest` prop |
| `src/components/layout/CollectionsSidebar.tsx` | Remove `handleNewRequest`; remove `onNewRequest` from `CollectionNode` usage |
| `src/components/panes/TabBar.tsx` | Remove + button |
| `src/components/panes/EditorGroup.tsx` | Remove New Request button from empty state |
| `src/components/layout/StatusBar.tsx` | Remove + New button |
| `src/hooks/useKeyboardShortcuts.ts` | Remove Ctrl+N; update Ctrl+S |
| `src/stores/pane-store.ts` | Remove `newDraftTab` action |
| `src/lib/pane-utils.ts` | Remove `createDefaultTab` |
| `src/types/pane-types.ts` | Remove `'draft'` from tabType; remove `defaultCollection` |
| `src/components/request/SaveRequestButton.tsx` | Simplify to direct save only |
| `src/components/collections/SaveToCollectionDialog.tsx` | Delete |
| `src/stores/__tests__/pane-store.test.ts` | Update tests |
| `src/lib/__tests__/pane-utils.test.ts` | Update tests |

---

### Task 1: Add inline request creation to CollectionNode

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx`

**Context:** CollectionNode currently delegates new request creation to `onNewRequest` prop which calls `newDraftTab()`. Replace this with inline `<Input>` in the tree that saves directly to disk and opens a tab.

- [ ] **Step 1: Add `saveRequest` to tauri-api import**

Find (~line 13):
```tsx
import { getCollection, onCollectionChanged, renameCollection } from '@/lib/tauri-api';
```

Replace with:
```tsx
import { getCollection, onCollectionChanged, renameCollection, saveRequest } from '@/lib/tauri-api';
```

- [ ] **Step 2: Add `createDefaultRequest` import**

Find (~line 14):
```tsx
import { usePaneStore } from '@/stores/pane-store';
```

Add after it:
```tsx
import { createDefaultRequest } from '@/lib/pane-utils';
```

- [ ] **Step 3: Remove `onNewRequest` from the props interface**

Find (~lines 21-30):
```tsx
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
```

Replace with:
```tsx
interface CollectionNodeProps {
  summary: CollectionSummary;
  filter: string;
  summaries: CollectionSummary[];
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}
```

- [ ] **Step 4: Remove `onNewRequest` from the function destructuring**

Find (~lines 32-35):
```tsx
export function CollectionNode({
  summary, filter, summaries,
  onNewRequest, onNewFolder, onMove, onDelete, onDuplicate,
}: CollectionNodeProps) {
```

Replace with:
```tsx
export function CollectionNode({
  summary, filter, summaries,
  onNewFolder, onMove, onDelete, onDuplicate,
}: CollectionNodeProps) {
```

- [ ] **Step 5: Add inline request creation state**

Find (~line 41):
```tsx
  const treeDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
```

Add after it:
```tsx
  const [creatingRequest, setCreatingRequest] = useState(false);
  const [newRequestName, setNewRequestName] = useState('');
```

- [ ] **Step 6: Add the `handleNewRequestCreate` handler**

Find the `handleDoubleClick` function (~lines 104-116). Add after its closing `};`:

```tsx
  const handleNewRequestCreate = async () => {
    const name = newRequestName.trim();
    if (!name) { setCreatingRequest(false); return; }
    setCreatingRequest(false);
    try {
      const payload = { uid: '', name, method: 'GET' as const, url: '', headers: [], auth: { authType: 'none' as const } };
      const saved = await saveRequest(summary.name, name, payload);
      usePaneStore.getState().openTab({
        id: saved.uid,
        title: saved.name,
        tabType: 'request',
        request: createDefaultRequest(),
        response: null,
        isDirty: false,
        source: { collection: summary.name, path: saved.file_name ?? `${name}.json` },
      });
    } catch (err) {
      console.error('[CollectionNode] Failed to create request:', err);
    }
  };
```

- [ ] **Step 7: Update dropdown menu "New Request" item**

Find (~line 166):
```tsx
              <DropdownMenuItem onClick={async () => { await onNewRequest(summary.name, ''); setOpen(true); }}>
                <Plus className="h-3.5 w-3.5 mr-2" /> New Request
              </DropdownMenuItem>
```

Replace with:
```tsx
              <DropdownMenuItem onClick={() => { setOpen(true); setCreatingRequest(true); setNewRequestName(''); }}>
                <Plus className="h-3.5 w-3.5 mr-2" /> New Request
              </DropdownMenuItem>
```

- [ ] **Step 8: Update context menu "New Request" item**

Find (~line 198):
```tsx
        <ContextMenuItem onClick={() => void onNewRequest(summary.name, '')}>New Request</ContextMenuItem>
```

Replace with:
```tsx
        <ContextMenuItem onClick={() => { setOpen(true); setCreatingRequest(true); setNewRequestName(''); }}>New Request</ContextMenuItem>
```

- [ ] **Step 9: Add inline `<Input>` after children map**

Find (~lines 233-234):
```tsx
          })}
        </div>
```

Replace with:
```tsx
          })}
          {creatingRequest && (
            <div className="flex items-center gap-1 px-2 py-0.5 text-xs">
              <Input
                autoFocus
                className="h-5 text-xs flex-1"
                placeholder="Request name"
                value={newRequestName}
                onChange={(e) => setNewRequestName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleNewRequestCreate();
                  if (e.key === 'Escape') setCreatingRequest(false);
                }}
                onBlur={() => setCreatingRequest(false)}
                onClick={(e) => e.stopPropagation()}
              />
            </div>
          )}
        </div>
```

- [ ] **Step 10: Remove `onNewRequest` from FolderNode usage**

Find (~lines 214-221):
```tsx
                <FolderNode
                  key={`folder-${item.name}`}
                  name={item.name} items={item.items}
                  collectionName={summary.name} basePath={item.name}
                  depth={1} filter={filter} summaries={summaries}
                  onNewRequest={onNewRequest} onNewFolder={onNewFolder}
                  onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                />
```

Replace with:
```tsx
                <FolderNode
                  key={`folder-${item.name}`}
                  name={item.name} items={item.items}
                  collectionName={summary.name} basePath={item.name}
                  depth={1} filter={filter} summaries={summaries}
                  onNewFolder={onNewFolder}
                  onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                />
```

- [ ] **Step 11: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -30
```

Expected: errors about `onNewRequest` in `CollectionsSidebar.tsx` (still passes it). May also see errors about FolderNode still expecting the prop. Both fixed in Task 2.

- [ ] **Step 12: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/collections/CollectionNode.tsx
git commit -m "feat: add inline request creation to CollectionNode — replaces onNewRequest prop"
```

---

### Task 2: Add inline request creation to FolderNode

**Files:**
- Modify: `src/components/collections/FolderNode.tsx`

**Context:** Same pattern as Task 1 but for folders. FolderNode is recursive — it renders child FolderNodes. Each FolderNode handles its own inline creation using its `collectionName` and `basePath`.

- [ ] **Step 1: Add `saveRequest` to tauri-api import**

Find (~line 13):
```tsx
import { moveItem } from '@/lib/tauri-api';
```

Replace with:
```tsx
import { moveItem, saveRequest } from '@/lib/tauri-api';
```

- [ ] **Step 2: Add `usePaneStore` and `createDefaultRequest` imports**

Find (~line 14):
```tsx
import { RequestNode } from './RequestNode';
```

Add before it:
```tsx
import { usePaneStore } from '@/stores/pane-store';
import { createDefaultRequest } from '@/lib/pane-utils';
```

- [ ] **Step 3: Remove `onNewRequest` from the props interface**

Find (~lines 18-31):
```tsx
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
```

Replace with:
```tsx
interface FolderNodeProps {
  name: string;
  items: CollectionItem[];
  collectionName: string;
  basePath: string;
  depth: number;
  filter: string;
  summaries: CollectionSummary[];
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}
```

- [ ] **Step 4: Remove `onNewRequest` from function destructuring**

Find (~lines 33-36):
```tsx
export function FolderNode({
  name, items, collectionName, basePath, depth, filter,
  summaries, onNewRequest, onNewFolder, onMove, onDelete, onDuplicate,
}: FolderNodeProps) {
```

Replace with:
```tsx
export function FolderNode({
  name, items, collectionName, basePath, depth, filter,
  summaries, onNewFolder, onMove, onDelete, onDuplicate,
}: FolderNodeProps) {
```

- [ ] **Step 5: Add inline request creation state**

Find (~line 39):
```tsx
  const [renameValue, setRenameValue] = useState(name);
```

Add after it:
```tsx
  const [creatingRequest, setCreatingRequest] = useState(false);
  const [newRequestName, setNewRequestName] = useState('');
```

- [ ] **Step 6: Add `handleNewRequestCreate` handler**

Find the `handleRename` function closing `};` (~line 56). Add after it:

```tsx
  const handleNewRequestCreate = async () => {
    const reqName = newRequestName.trim();
    if (!reqName) { setCreatingRequest(false); return; }
    setCreatingRequest(false);
    try {
      const path = `${basePath}/${reqName}`;
      const payload = { uid: '', name: reqName, method: 'GET' as const, url: '', headers: [], auth: { authType: 'none' as const } };
      const saved = await saveRequest(collectionName, path, payload);
      usePaneStore.getState().openTab({
        id: saved.uid,
        title: saved.name,
        tabType: 'request',
        request: createDefaultRequest(),
        response: null,
        isDirty: false,
        source: { collection: collectionName, path: saved.file_name ?? `${path}.json` },
      });
    } catch (err) {
      console.error('[FolderNode] Failed to create request:', err);
    }
  };
```

- [ ] **Step 7: Update dropdown menu "New Request" item**

Find (~line 94):
```tsx
                <DropdownMenuItem onClick={() => void onNewRequest(collectionName, basePath)}><Plus className="h-3.5 w-3.5 mr-2" /> New Request</DropdownMenuItem>
```

Replace with:
```tsx
                <DropdownMenuItem onClick={() => { setOpen(true); setCreatingRequest(true); setNewRequestName(''); }}><Plus className="h-3.5 w-3.5 mr-2" /> New Request</DropdownMenuItem>
```

- [ ] **Step 8: Update context menu "New Request" item**

Find (~line 105):
```tsx
          <ContextMenuItem onClick={() => void onNewRequest(collectionName, basePath)}>New Request</ContextMenuItem>
```

Replace with:
```tsx
          <ContextMenuItem onClick={() => { setOpen(true); setCreatingRequest(true); setNewRequestName(''); }}>New Request</ContextMenuItem>
```

- [ ] **Step 9: Add inline `<Input>` after children map**

Find (~lines 142-143):
```tsx
          })}
        </div>
```

Replace with:
```tsx
          })}
          {creatingRequest && (
            <div className="flex items-center gap-1 px-2 py-0.5 text-xs">
              <Input
                autoFocus
                className="h-5 text-xs flex-1"
                placeholder="Request name"
                value={newRequestName}
                onChange={(e) => setNewRequestName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleNewRequestCreate();
                  if (e.key === 'Escape') setCreatingRequest(false);
                }}
                onBlur={() => setCreatingRequest(false)}
                onClick={(e) => e.stopPropagation()}
              />
            </div>
          )}
        </div>
```

- [ ] **Step 10: Remove `onNewRequest` from recursive FolderNode usage**

Find (~lines 121-128):
```tsx
                <FolderNode
                  key={`folder-${folderPath}`}
                  name={item.name} items={item.items}
                  collectionName={collectionName} basePath={folderPath}
                  depth={depth + 1} filter={filter} summaries={summaries}
                  onNewRequest={onNewRequest} onNewFolder={onNewFolder}
                  onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                />
```

Replace with:
```tsx
                <FolderNode
                  key={`folder-${folderPath}`}
                  name={item.name} items={item.items}
                  collectionName={collectionName} basePath={folderPath}
                  depth={depth + 1} filter={filter} summaries={summaries}
                  onNewFolder={onNewFolder}
                  onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                />
```

- [ ] **Step 11: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -30
```

Expected: error in `CollectionsSidebar.tsx` still passing `onNewRequest` to `CollectionNode`. Fixed in Task 3.

- [ ] **Step 12: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/collections/FolderNode.tsx
git commit -m "feat: add inline request creation to FolderNode — replaces onNewRequest prop"
```

---

### Task 3: Remove `onNewRequest` from CollectionsSidebar

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Remove `handleNewRequest` callback**

Find (~lines 141-145):
```tsx
  const handleNewRequest = useCallback(async (collection: string, folderPath: string) => {
    // Create a draft tab pre-linked to this collection so Save works
    // directly without prompting the "Save to Collection" dialog.
    usePaneStore.getState().newDraftTab(undefined, collection, folderPath || undefined);
  }, []);
```

Delete those 5 lines entirely.

- [ ] **Step 2: Remove `onNewRequest` prop from CollectionNode usage**

Find (~lines 340-350):
```tsx
                    <CollectionNode
                      key={s.name}
                      summary={s}
                      filter={filter}
                      summaries={summaries}
                      onNewRequest={handleNewRequest}
                      onNewFolder={handleNewFolder}
                      onMove={handleMove}
                      onDelete={setDeleteTarget}
                      onDuplicate={handleDuplicate}
                    />
```

Replace with:
```tsx
                    <CollectionNode
                      key={s.name}
                      summary={s}
                      filter={filter}
                      summaries={summaries}
                      onNewFolder={handleNewFolder}
                      onMove={handleMove}
                      onDelete={setDeleteTarget}
                      onDuplicate={handleDuplicate}
                    />
```

- [ ] **Step 3: Verify types — no errors**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "refactor: remove onNewRequest prop from CollectionsSidebar — inline creation handles it"
```

---

### Task 4: Remove non-tree entry points for new request

**Files:**
- Modify: `src/components/panes/TabBar.tsx`
- Modify: `src/components/panes/EditorGroup.tsx`
- Modify: `src/components/layout/StatusBar.tsx`
- Modify: `src/hooks/useKeyboardShortcuts.ts`

- [ ] **Step 1: Remove + button from TabBar**

In `src/components/panes/TabBar.tsx`, find (~line 20):
```tsx
  const newDraftTab = usePaneStore((s) => s.newDraftTab);
```
Delete that line.

Find (~lines 111-119):
```tsx
      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 shrink-0 rounded-none hover:bg-accent/60"
        onClick={() => newDraftTab(node.groupId)}
        aria-label="New tab"
      >
        <Plus className="h-3.5 w-3.5" />
      </Button>
```
Delete those 9 lines.

Remove `Plus` from the lucide-react import (~line 11):
```tsx
import { Plus, PanelRight, PanelBottom } from 'lucide-react';
```
Replace with:
```tsx
import { PanelRight, PanelBottom } from 'lucide-react';
```

- [ ] **Step 2: Simplify EditorGroup EmptyState**

In `src/components/panes/EditorGroup.tsx`, find the `EmptyState` component (~lines 12-57). Replace it with:

```tsx
function EmptyState() {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="flex flex-col items-center gap-6 text-center max-w-sm">
        <div className="flex flex-col items-center gap-3">
          <RocketLaunch className="w-32 h-32" />
          <div>
            <h2 className="text-lg font-semibold text-foreground">Rocket API</h2>
            <p className="text-sm text-muted-foreground mt-1">
              Open a request from the collections sidebar to get started.
            </p>
          </div>
        </div>
        <div className="flex gap-4 text-xs text-muted-foreground">
          <span>
            <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[10px]">Cmd+Enter</kbd>
            {' '}Send
          </span>
        </div>
      </div>
    </div>
  );
}
```

Update the usage in `EditorGroup` (~line 76):
```tsx
          <EmptyState groupId={node.groupId} />
```
Replace with:
```tsx
          <EmptyState />
```

Remove unused imports from EditorGroup: `Plus`, `Button`, `usePaneStore` (check each — `usePaneStore` may still be used elsewhere in the file; if not, remove it). Remove `Plus` and `Button` imports.

- [ ] **Step 3: Remove + New button from StatusBar**

In `src/components/layout/StatusBar.tsx`, replace the entire file with:

```tsx
import { EnvironmentSwitcher } from '@/components/layout/EnvironmentSwitcher';

export function StatusBar() {
  return (
    <div className="h-7 border-t border-border/70 bg-card/85 backdrop-blur-sm px-2 flex items-center gap-1.5 shrink-0">
      <div className="ml-auto">
        <EnvironmentSwitcher />
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Update keyboard shortcuts — remove Ctrl+N, update Ctrl+S**

In `src/hooks/useKeyboardShortcuts.ts`, find the Ctrl+S handler (~lines 28-36):
```tsx
      // Cmd/Ctrl+S — save draft to collection.
      if (e.key === 's') {
        e.preventDefault();
        const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
        if (tab && tab.tabType === 'draft') {
          window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: tab.id } }));
        }
        return;
      }
```

Replace with:
```tsx
      // Cmd/Ctrl+S — save active request.
      if (e.key === 's') {
        e.preventDefault();
        const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
        if (tab) {
          window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: tab.id } }));
        }
        return;
      }
```

Find the Ctrl+N handler (~lines 38-43):
```tsx
      // Cmd/Ctrl+N — open a new draft tab in the active group.
      if (e.key === 'n') {
        e.preventDefault();
        store.newDraftTab(activeGroupId);
        return;
      }
```

Delete those 6 lines.

- [ ] **Step 5: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -30
```

Expected: no errors (newDraftTab still exists in store, just no callers).

- [ ] **Step 6: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/panes/TabBar.tsx src/components/panes/EditorGroup.tsx src/components/layout/StatusBar.tsx src/hooks/useKeyboardShortcuts.ts
git commit -m "feat: remove non-tree entry points for new request — no + buttons, no Ctrl+N"
```

---

### Task 5: Remove draft tab infrastructure

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/stores/pane-store.ts`
- Modify: `src/lib/pane-utils.ts`
- Modify: `src/stores/__tests__/pane-store.test.ts`
- Modify: `src/lib/__tests__/pane-utils.test.ts`

- [ ] **Step 1: Remove `'draft'` from tabType and `defaultCollection` from RequestTab**

In `src/types/pane-types.ts`, find (~lines 28-34):
```tsx
export interface RequestTab extends BaseTab {
  tabType: 'request' | 'draft' | 'history';
  request: RequestState;
  response: ResponseState | null;
  /** Pre-set collection for drafts created from a collection's + button. */
  defaultCollection?: string;
}
```

Replace with:
```tsx
export interface RequestTab extends BaseTab {
  tabType: 'request' | 'history';
  request: RequestState;
  response: ResponseState | null;
}
```

- [ ] **Step 2: Remove `newDraftTab` from pane-store**

In `src/stores/pane-store.ts`, remove from imports (~line 6):
```tsx
  createDefaultTab,
```

Remove from `PaneState` interface (~line 59):
```tsx
  newDraftTab: (groupId?: string, defaultCollection?: string, defaultFolderPath?: string) => void;
```

Remove the implementation (~lines 86-102):
```tsx
  newDraftTab(groupId, defaultCollection, defaultFolderPath) {
    const { root, activeGroupId } = get();
    const targetGroupId = groupId ?? activeGroupId;
    const tab = createDefaultTab();
    if (defaultCollection) {
      (tab as any).defaultCollection = defaultCollection;
    }
    if (defaultFolderPath) {
      (tab as any).defaultFolderPath = defaultFolderPath;
    }
    const newRoot = updateLeaf(root, targetGroupId, (leaf) => ({
      ...leaf,
      tabs: [...leaf.tabs, tab],
      activeTabId: tab.id,
    }));
    set({ root: newRoot, activeGroupId: targetGroupId });
  },
```

- [ ] **Step 3: Remove `createDefaultTab` from pane-utils**

In `src/lib/pane-utils.ts`, delete (~lines 125-136):
```tsx
// Creates a new unsaved draft tab with a fresh UUID.
export function createDefaultTab(): RequestTab {
  const id = crypto.randomUUID();
  return {
    id,
    title: 'New Request',
    tabType: 'draft',
    request: createDefaultRequest(),
    response: null,
    isDirty: false,
  };
}
```

Also remove `RequestTab` from the import at the top of the file if it's no longer used (~line 6). Check if other exports still reference it.

- [ ] **Step 4: Update pane-store tests**

In `src/stores/__tests__/pane-store.test.ts`, remove or update all tests that reference `newDraftTab`. These tests verify draft tab creation behavior that no longer exists. Read the file first, then remove test cases that call `newDraftTab`. Replace them with equivalent tests using `openTab` if the test is testing something other than draft creation (like tab switching, split behavior, etc.).

- [ ] **Step 5: Update pane-utils tests**

In `src/lib/__tests__/pane-utils.test.ts`, remove the test for `createDefaultTab` (the one that asserts `tabType` is `'draft'`).

- [ ] **Step 6: Verify types and tests**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 7: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/types/pane-types.ts src/stores/pane-store.ts src/lib/pane-utils.ts src/stores/__tests__/pane-store.test.ts src/lib/__tests__/pane-utils.test.ts
git commit -m "refactor: remove draft tab infrastructure — no newDraftTab, createDefaultTab, or 'draft' tabType"
```

---

### Task 6: Simplify SaveRequestButton and delete SaveToCollectionDialog

**Files:**
- Modify: `src/components/request/SaveRequestButton.tsx`
- Delete: `src/components/collections/SaveToCollectionDialog.tsx`

- [ ] **Step 1: Rewrite SaveRequestButton**

Replace the entire contents of `src/components/request/SaveRequestButton.tsx` with:

```tsx
import { useEffect, useCallback } from 'react';
import { Save } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { saveRequest, type Auth, type Request as ApiRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';

interface SaveRequestButtonProps {
  tab: RequestTab;
  groupId: string;
}

// Maps AuthState to the flat Rust Auth shape for disk persistence.
function authForSave(auth: RequestTab['request']['auth']): Auth {
  switch (auth.authType) {
    case 'inherit':
    case 'none':
      return { authType: 'none' };
    case 'basic':
      return { authType: 'basic', username: auth.basic?.username ?? '', password: auth.basic?.password ?? '' };
    case 'bearer':
      return { authType: 'bearer', token: auth.bearer?.token ?? '' };
    case 'api-key':
      return { authType: 'api-key', key: auth.apiKey?.key ?? '', value: auth.apiKey?.value ?? '', addTo: auth.apiKey?.addTo ?? 'header' };
    case 'oauth2':
      return {
        authType: 'oauth2',
        grantType: auth.oauth2?.grantType ?? 'client_credentials',
        authorizationUrl: auth.oauth2?.authorizationUrl ?? '',
        tokenUrl: auth.oauth2?.tokenUrl ?? '',
        callbackUrl: auth.oauth2?.callbackUrl ?? '',
        clientId: auth.oauth2?.clientId ?? '',
        clientSecret: auth.oauth2?.clientSecret ?? '',
        scope: auth.oauth2?.scope ?? '',
        state: auth.oauth2?.state ?? '',
        username: auth.oauth2?.username ?? '',
        password: auth.oauth2?.password ?? '',
        clientAuthentication: auth.oauth2?.clientAuthentication ?? 'body',
        headerPrefix: auth.oauth2?.headerPrefix ?? 'Bearer',
        addTokenTo: auth.oauth2?.addTokenTo ?? 'header',
        verifySsl: auth.oauth2?.verifySsl ?? true,
        accessToken: auth.oauth2?.accessToken ?? '',
        refreshToken: auth.oauth2?.refreshToken ?? '',
        expiresIn: auth.oauth2?.expiresIn ?? null,
        tokenAcquiredAt: auth.oauth2?.tokenAcquiredAt ?? null,
      };
    case 'aws-sig-v4':
      return {
        authType: 'aws-sig-v4',
        accessKey: auth.awsSigV4?.accessKey ?? '',
        secretKey: auth.awsSigV4?.secretKey ?? '',
        region: auth.awsSigV4?.region ?? '',
        service: auth.awsSigV4?.service ?? '',
        sessionToken: auth.awsSigV4?.sessionToken ?? '',
      };
    default:
      return { authType: 'none' };
  }
}

function buildPayloadFromTab(tab: RequestTab): ApiRequest {
  const body = tab.request.body;
  return {
    uid: tab.id,
    name: tab.title,
    method: tab.request.method,
    url: tab.request.url,
    headers: tab.request.headers
      .filter((h) => h.key)
      .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
    body: body.mode !== 'none' ? { mode: body.mode, content: body.content } : undefined,
    auth: authForSave(tab.request.auth),
  };
}

export function SaveRequestButton({ tab }: SaveRequestButtonProps) {
  const markClean = usePaneStore((s) => s.markClean);

  const handleSave = useCallback(async () => {
    if (!tab.source) return;
    try {
      await saveRequest(tab.source.collection, tab.source.path, buildPayloadFromTab(tab));
      markClean(tab.id);
    } catch (err) {
      console.error('[SaveRequestButton] Save failed:', err);
    }
  }, [tab, markClean]);

  // Listen for Cmd+S keyboard shortcut.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId !== tab.id) return;
      void handleSave();
    };
    window.addEventListener('rocket:save-draft', handler);
    return () => window.removeEventListener('rocket:save-draft', handler);
  }, [tab.id, handleSave]);

  if (!tab.source) return null;

  return (
    <Button
      size="sm"
      variant="outline"
      className="h-8 px-3"
      disabled={!tab.isDirty}
      onClick={() => void handleSave()}
    >
      <Save className="mr-1 h-3.5 w-3.5" />
      Save
    </Button>
  );
}
```

- [ ] **Step 2: Delete SaveToCollectionDialog**

```bash
cd /home/numericlabs/data/Rust/Rocket && rm src/components/collections/SaveToCollectionDialog.tsx
```

- [ ] **Step 3: Verify no remaining references to SaveToCollectionDialog**

```bash
cd /home/numericlabs/data/Rust/Rocket && grep -r "SaveToCollectionDialog" src/
```

Expected: no output.

- [ ] **Step 4: Type check and build**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit && yarn build 2>&1 | tail -10
```

Expected: both succeed.

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/SaveRequestButton.tsx
git rm src/components/collections/SaveToCollectionDialog.tsx
git commit -m "feat: simplify SaveRequestButton to direct save only; delete SaveToCollectionDialog"
```

---

## Done

The new request creation flow is now:
- "New Request" in tree context menu shows inline `<Input>` in the tree
- Enter saves to disk immediately and opens the request in a tab
- Escape or blur cancels
- Every open tab is backed by a file on disk (`tabType: 'request'`)
- Ctrl+S always does direct save
- No draft tabs, no save dialog, no + buttons outside the tree
