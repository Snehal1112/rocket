# Inline New Request Creation — Design Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Replace the buggy draft-tab-then-save-dialog flow with inline tree creation: user types a name in the tree, request is saved to disk immediately, and opens in a tab.

## Problems with Current Design

1. **No collection/folder pre-selection:** Draft tab stores `defaultCollection`/`defaultFolderPath` via `(tab as any)` — type-unsafe, and the SaveToCollectionDialog doesn't always pick them up.
2. **Tab ID mutation on first save:** Tab starts with a local UUID, then changes to the backend UID after save. Any reference to the old ID becomes stale.
3. **Duplicate "New Request" confusion:** Multiple drafts all named "New Request" create ambiguity in the tree. Selection gets confused when multiple requests share the same name.
4. **Save dialog complexity:** SaveToCollectionDialog manages collection picker, name input, inline collection creation, and error handling — too much surface area for bugs.
5. **`isDirty` semantics unclear:** Draft starts with `isDirty: false` even though it was never saved to disk.

## New Flow

### Tree-based creation (primary path)

1. User clicks "New Request" in collection/folder context menu or dropdown menu
2. The parent node auto-expands if closed
3. An inline `<Input>` appears at the bottom of the node's children list
4. User types a name, presses Enter
5. Frontend calls `saveRequest(collection, path, emptyGetRequest)` — file is written to disk
6. Backend returns the saved `Request` with UID and `file_name`
7. Tree refreshes via `collection-changed` event
8. Frontend calls `openTab()` with a fully-formed `RequestTab` (tabType: `'request'`, source set, ID = backend UID)
9. Tab opens showing the empty GET request, ready to edit

**Cancel:** Escape or blur without Enter removes the inline input. No file created.

**Collision handling:** Backend auto-suffixes if a file with the same name exists (e.g., "login 1.json"). The tree shows the actual saved name.

### Non-tree entry points (TabBar +, StatusBar +, Ctrl+N)

These currently call `newDraftTab()`. For this change, they will be **removed or disabled**:
- **TabBar + button:** Remove. New requests should always be contextual to a collection.
- **StatusBar + button:** Remove.
- **Ctrl+N keyboard shortcut:** Remove the `newDraftTab` call. The shortcut can be repurposed later if needed.

This eliminates the draft tab concept entirely — every open tab is backed by a file on disk.

### Save button (Ctrl+S)

Since every tab is now `tabType: 'request'` with a `source`, Ctrl+S always does a direct save to the existing file. No dialog branching needed.

---

## Section 1 — Type Changes (`src/types/pane-types.ts`)

### `RequestTab.tabType`

| Before | After |
|---|---|
| `tabType: 'request' \| 'draft' \| 'history'` | `tabType: 'request' \| 'history'` |

Remove `'draft'` from the union. Remove the `defaultCollection` optional property.

---

## Section 2 — Pane Store Changes (`src/stores/pane-store.ts`, `src/lib/pane-utils.ts`)

### Remove `newDraftTab`

Remove the `newDraftTab` action from `PaneState` interface and store implementation.

### Remove `createDefaultTab`

Remove from `pane-utils.ts`. `createDefaultRequest` stays — it's useful for building the empty GET payload when creating a new request.

---

## Section 3 — Inline Input in CollectionNode (`src/components/collections/CollectionNode.tsx`)

### New state

```tsx
const [creatingRequest, setCreatingRequest] = useState(false);
const [newRequestName, setNewRequestName] = useState('');
```

### Menu item change

"New Request" menu items currently call `onNewRequest(summary.name, '')`. Change to:
```tsx
onClick={() => { setOpen(true); setCreatingRequest(true); setNewRequestName(''); }}
```

### Inline input JSX

After the `filteredItems.map(...)` block, inside the indent guide div, add:

```tsx
{creatingRequest && (
  <div className="flex items-center gap-1 px-2 py-0.5 text-xs">
    <Input
      autoFocus
      value={newRequestName}
      onChange={(e) => setNewRequestName(e.target.value)}
      onKeyDown={handleNewRequestKeyDown}
      onBlur={() => setCreatingRequest(false)}
      placeholder="Request name"
      className="h-5 text-xs"
    />
  </div>
)}
```

### Handler

```tsx
const handleNewRequestKeyDown = async (e: React.KeyboardEvent<HTMLInputElement>) => {
  if (e.key === 'Escape') {
    setCreatingRequest(false);
    return;
  }
  if (e.key === 'Enter') {
    const name = newRequestName.trim();
    if (!name) return;
    setCreatingRequest(false);
    try {
      const payload = { uid: '', name, method: 'GET', url: '', headers: [], auth: { authType: 'none' } };
      const saved = await saveRequest(summary.name, name, payload);
      // Open tab with the saved request data.
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
  }
};
```

### Remove `onNewRequest` prop

This prop is no longer needed — creation is handled locally within each node.

---

## Section 4 — Inline Input in FolderNode (`src/components/collections/FolderNode.tsx`)

Same pattern as CollectionNode:
- Add `creatingRequest` / `newRequestName` state
- Change "New Request" menu items to set state instead of calling `onNewRequest`
- Add inline `<Input>` after children map
- Handler calls `saveRequest(collectionName, \`${basePath}/${name}\`, payload)`
- Remove `onNewRequest` prop

---

## Section 5 — CollectionsSidebar Changes (`src/components/layout/CollectionsSidebar.tsx`)

- Remove `handleNewRequest` callback (the `newDraftTab` call)
- Remove `onNewRequest` prop from `CollectionNode` usage
- Keep `handleMove`, `handleNewFolder`, `onDelete`, `onDuplicate` — those are unchanged

---

## Section 6 — Remove Non-Tree Entry Points

### `src/components/panes/TabBar.tsx`

Remove the + button that calls `newDraftTab(node.groupId)`.

### `src/components/panes/EditorGroup.tsx`

Remove the empty-state + button that calls `newDraftTab(groupId)`.

### `src/components/layout/StatusBar.tsx`

Remove the + button that calls `newDraftTab(activeGroupId)`.

### `src/hooks/useKeyboardShortcuts.ts`

Remove the Ctrl+N handler that calls `newDraftTab`.

---

## Section 7 — Simplify SaveRequestButton (`src/components/request/SaveRequestButton.tsx`)

- Remove the `SaveToCollectionDialog` import and usage
- Remove all dialog state (`dialogOpen`, etc.)
- The button always does direct save: call `saveRequest(tab.source.collection, tab.source.path, payload)` then `markClean(tab.id)`
- If `!tab.source`, the button is disabled (should never happen since drafts are gone)

---

## Section 8 — Delete SaveToCollectionDialog

Delete `src/components/collections/SaveToCollectionDialog.tsx` entirely.

---

## Files Changed

| File | Changes |
|---|---|
| `src/types/pane-types.ts` | Remove `'draft'` from tabType union, remove `defaultCollection` |
| `src/stores/pane-store.ts` | Remove `newDraftTab` action |
| `src/lib/pane-utils.ts` | Remove `createDefaultTab()` |
| `src/components/collections/CollectionNode.tsx` | Add inline input for new request, remove `onNewRequest` prop |
| `src/components/collections/FolderNode.tsx` | Add inline input for new request, remove `onNewRequest` prop |
| `src/components/layout/CollectionsSidebar.tsx` | Remove `handleNewRequest`, remove `onNewRequest` prop |
| `src/components/panes/TabBar.tsx` | Remove + button |
| `src/components/panes/EditorGroup.tsx` | Remove + new tab button |
| `src/components/layout/StatusBar.tsx` | Remove + button |
| `src/hooks/useKeyboardShortcuts.ts` | Remove Ctrl+N handler |
| `src/components/request/SaveRequestButton.tsx` | Simplify to direct save only |
| `src/components/collections/SaveToCollectionDialog.tsx` | Delete |

## Out of Scope

- "New Folder" flow (already works via backend, no draft concept)
- "Save As" / copy-to-another-collection feature (can be added later)
- Request templates or method selection during creation (user edits method after opening)
- `reorderItems` / `_order.json` updates — new request appears in default alphabetical order or at the end
