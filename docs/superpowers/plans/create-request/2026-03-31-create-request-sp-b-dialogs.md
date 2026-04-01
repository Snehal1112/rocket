# Create Request — SP-B: Creation Dialogs

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Depends on:** SP-A must be merged first (`sanitizeFilename` and `openEphemeralTab` must exist).

**Goal:** Build the two dialogs that handle named, intentional request creation — `CreateRequestDialog` (within-collection flow from the `···` menu) and `SaveToCollectionDialog` (anchor an unsaved tab to a collection).

**Architecture:** Both dialogs are self-contained shadcn/ui Dialog components. `CreateRequestDialog` calls `saveRequest` immediately and opens a tab with a full `source` binding. `SaveToCollectionDialog` re-uses an existing tab's data, calls `saveRequest`, then updates the tab's `source` + title in the store. A "Save to Collection" button appears in `RequestPanel` only when the active tab has no `source`. `Cmd+S` on a sourceless tab dispatches `rocket:save-to-collection` instead of `rocket:save-draft`.

**Tech Stack:** React 18, TypeScript, shadcn/ui (Dialog, Select, Input, Label, Button), Zustand (`usePaneStore`), `sanitizeFilename` from SP-A, `saveRequest` + `listCollections` + `createCollection` from `src/lib/tauri-api.ts`.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src/components/request/CreateRequestDialog.tsx` | **Create** | New-request form: type + name + method + URL → saves file + opens tab |
| `src/components/request/SaveToCollectionDialog.tsx` | **Create** | Assign unsaved tab to existing or new collection |
| `src/components/collections/CollectionNode.tsx` | **Modify** | Wire `···` → "New Request" to open `CreateRequestDialog` |
| `src/components/request/RequestPanel.tsx` | **Modify** | Show "Save to Collection" button + dialog when tab has no `source`; hook `Cmd+S` |
| `src/hooks/useKeyboardShortcuts.ts` | **Modify** | `Cmd+S` on sourceless tab → `rocket:save-to-collection` event |

---

## Chunk 1: `CreateRequestDialog`

### Task 1: Build `CreateRequestDialog` component

**Files:**
- Create: `src/components/request/CreateRequestDialog.tsx`

- [ ] **Step 1: Verify `saveRequest` signature**

```bash
grep -n "saveRequest" src/lib/tauri-api.ts | head -10
```

Note the exact parameter order and types. The plan assumes `saveRequest(collectionName: string, path: string, request: Request): Promise<Request>`. Adjust if different.

- [ ] **Step 2: Create the component**

```tsx
// src/components/request/CreateRequestDialog.tsx
import { useState } from 'react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import { saveRequest } from '@/lib/tauri-api';
import { sanitizeFilename } from '@/lib/filename-utils';
import { createDefaultRequest } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import type { HttpMethod, RequestTab } from '@/types/pane-types';

type RequestType = 'http' | 'graphql' | 'grpc' | 'websocket' | 'curl';

const REQUEST_TYPES: { label: string; value: RequestType }[] = [
  { label: 'HTTP',      value: 'http' },
  { label: 'GraphQL',   value: 'graphql' },
  { label: 'gRPC',      value: 'grpc' },
  { label: 'WebSocket', value: 'websocket' },
  { label: 'From cURL', value: 'curl' },
];

const HTTP_METHODS: HttpMethod[] = [
  'GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD',
];

export interface CreateRequestDialogProps {
  open: boolean;
  collectionName: string;
  /** Optional sub-folder path within the collection (e.g. "auth/tokens"). */
  folderPath?: string;
  onClose: () => void;
}

export function CreateRequestDialog({
  open,
  collectionName,
  folderPath,
  onClose,
}: CreateRequestDialogProps) {
  const [requestType, setRequestType] = useState<RequestType>('http');
  const [name, setName]               = useState('');
  const [method, setMethod]           = useState<HttpMethod>('GET');
  const [url, setUrl]                 = useState('');
  const [saving, setSaving]           = useState(false);
  const [error, setError]             = useState('');

  const trimmedName = name.trim();
  const fsName      = trimmedName ? sanitizeFilename(trimmedName) : '';
  // Show filesystem name hint only when special chars changed the name.
  const showFsHint  = fsName !== '' && fsName !== `${trimmedName}.yml`;

  function reset() {
    setName('');
    setUrl('');
    setMethod('GET');
    setRequestType('http');
    setError('');
  }

  async function handleCreate() {
    if (!trimmedName) { setError('Request name is required.'); return; }
    setSaving(true);
    setError('');
    try {
      const uid      = crypto.randomUUID();
      const filePath = folderPath ? `${folderPath}/${fsName}` : fsName;
      const payload  = {
        uid,
        name: trimmedName,
        method,
        url,
        headers: [],
        auth: { authType: 'none' as const },
        fileName: filePath,
      };
      const saved = await saveRequest(collectionName, filePath, payload);
      const tab: RequestTab = {
        id:       uid,
        title:    trimmedName,
        tabType:  'request',
        request:  {
          ...createDefaultRequest(),
          method,
          url,
          requestType: requestType === 'curl' ? 'http' : requestType,
        },
        response: null,
        isDirty:  false,
        source:   { collection: collectionName, path: saved.fileName ?? filePath },
      };
      usePaneStore.getState().openTab(tab);
      reset();
      onClose();
    } catch (err) {
      console.error('[CreateRequestDialog] failed:', err);
      setError('Failed to create request. Please try again.');
    } finally {
      setSaving(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter' && !saving) void handleCreate();
  }

  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) { reset(); onClose(); } }}>
      <DialogContent className="sm:max-w-md" onKeyDown={handleKeyDown}>
        <DialogHeader>
          <DialogTitle>New Request</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4 py-2">
          {/* Request Type */}
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="crd-type" className="text-xs font-medium">
              Request Type
            </Label>
            <Select
              value={requestType}
              onValueChange={(v) => setRequestType(v as RequestType)}
            >
              <SelectTrigger id="crd-type" className="h-9">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {REQUEST_TYPES.map((t) => (
                  <SelectItem key={t.value} value={t.value}>{t.label}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Request Name */}
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="crd-name" className="text-xs font-medium">
              Request Name
            </Label>
            <Input
              id="crd-name"
              autoFocus
              placeholder="e.g. GET /users/:id"
              value={name}
              onChange={(e) => { setName(e.target.value); setError(''); }}
              className="h-9"
            />
            {showFsHint && (
              <p className="text-xs text-muted-foreground">
                Saved as:{' '}
                <span className="font-mono">{fsName}</span>
              </p>
            )}
          </div>

          {/* HTTP Method — only for http / curl */}
          {(requestType === 'http' || requestType === 'curl') && (
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="crd-method" className="text-xs font-medium">
                HTTP Method
              </Label>
              <Select
                value={method}
                onValueChange={(v) => setMethod(v as HttpMethod)}
              >
                <SelectTrigger id="crd-method" className="h-9">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {HTTP_METHODS.map((m) => (
                    <SelectItem key={m} value={m}>{m}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          {/* URL */}
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="crd-url" className="text-xs font-medium">URL</Label>
            <Input
              id="crd-url"
              placeholder="https://api.example.com/users"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              className="h-9 font-mono text-sm"
            />
          </div>

          {error && (
            <p className="text-xs text-destructive">{error}</p>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            size="sm"
            onClick={() => { reset(); onClose(); }}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={() => void handleCreate()}
            disabled={saving || !trimmedName}
          >
            {saving ? 'Creating…' : 'Create'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Wire into `CollectionNode`**

In `src/components/collections/CollectionNode.tsx`:

1. Add import:
```tsx
import { CreateRequestDialog } from '@/components/request/CreateRequestDialog';
```

2. Add state near other `useState` calls:
```tsx
const [createRequestOpen, setCreateRequestOpen] = useState(false);
```

3. Find the `···` DropdownMenu. Locate the existing "New Request" `DropdownMenuItem` (it currently calls `setCreatingRequest(true)` for an inline input). Replace its `onClick` handler:
```tsx
<DropdownMenuItem onClick={() => setCreateRequestOpen(true)}>
  New Request
</DropdownMenuItem>
```

4. Render the dialog inside the component's JSX (after the ContextMenu/TreeItem structure):
```tsx
<CreateRequestDialog
  open={createRequestOpen}
  collectionName={summary.name}
  onClose={() => setCreateRequestOpen(false)}
/>
```

- [ ] **Step 4: Manual smoke test**

```bash
yarn tauri dev
```

1. Click `···` on any collection → select "New Request"
2. Dialog opens with Request Type, Name, Method, URL fields
3. Type name `GET /users/:id [v2]` → filesystem hint appears: `GET -users--id -v2-.yml`
4. Set Method GET, URL `https://api.example.com`
5. Click Create → tab opens with correct title and URL pre-filled
6. Check collection folder on disk → `.yml` file exists

- [ ] **Step 5: Commit**

```bash
git add src/components/request/CreateRequestDialog.tsx \
        src/components/collections/CollectionNode.tsx
git commit -m "feat: add CreateRequestDialog for within-collection request creation"
```

---

## Chunk 2: `SaveToCollectionDialog`

### Task 2: Build `SaveToCollectionDialog` and wire into `RequestPanel`

**Files:**
- Create: `src/components/request/SaveToCollectionDialog.tsx`
- Modify: `src/components/request/RequestPanel.tsx`
- Modify: `src/hooks/useKeyboardShortcuts.ts`

- [ ] **Step 1: Verify `createCollection` exists in tauri-api**

```bash
grep -n "createCollection\|create_collection" src/lib/tauri-api.ts | head -5
```

If `createCollection` does not exist, use `listCollections` only and omit the "New Collection" option for now — add a `TODO` comment. Adjust the dialog accordingly.

- [ ] **Step 2: Verify `updateTabSource` and `updateTabTitle` exist on pane-store**

```bash
grep -n "updateTabSource\|updateTabTitle" src/stores/pane-store.ts | head -5
```

If either is missing, add it to `PaneState` and implement it before proceeding:

```ts
// PaneState interface additions:
updateTabSource: (tabId: string, source: { collection: string; path: string }) => void;
updateTabTitle:  (tabId: string, title: string) => void;

// Implementation:
updateTabSource(tabId, source) {
  const { root } = get();
  const newRoot = updateTabInTree(root, tabId, (tab) => ({ ...tab, source }));
  set({ root: newRoot });
},

updateTabTitle(tabId, title) {
  const { root } = get();
  const newRoot = updateTabInTree(root, tabId, (tab) => ({ ...tab, title }));
  set({ root: newRoot });
},
```

- [ ] **Step 3: Create `SaveToCollectionDialog`**

```tsx
// src/components/request/SaveToCollectionDialog.tsx
import { useState, useEffect } from 'react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from '@/components/ui/dialog';
import { Button }   from '@/components/ui/button';
import { Input }    from '@/components/ui/input';
import { Label }    from '@/components/ui/label';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import { listCollections, saveRequest } from '@/lib/tauri-api';
import { sanitizeFilename } from '@/lib/filename-utils';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';
import type { CollectionSummary } from '@/lib/tauri-api';

// Sentinel value used in the Select to mean "create a new collection".
const NEW_COLLECTION = '__new__';

export interface SaveToCollectionDialogProps {
  open: boolean;
  tab: RequestTab;
  onClose: () => void;
}

export function SaveToCollectionDialog({
  open,
  tab,
  onClose,
}: SaveToCollectionDialogProps) {
  const [collections,        setCollections]        = useState<CollectionSummary[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [newCollectionName,  setNewCollectionName]  = useState('');
  const [requestName,        setRequestName]        = useState(
    tab.title === 'Untitled' ? '' : tab.title,
  );
  const [saving, setSaving] = useState(false);
  const [error,  setError]  = useState('');

  useEffect(() => {
    if (!open) return;
    listCollections()
      .then((cols) => {
        setCollections(cols);
        setSelectedCollection(cols.length > 0 ? cols[0].name : NEW_COLLECTION);
      })
      .catch(console.error);
  }, [open]);

  const isCreatingNew = selectedCollection === NEW_COLLECTION || collections.length === 0;
  const trimmedName   = requestName.trim();
  const fsName        = trimmedName ? sanitizeFilename(trimmedName) : '';
  const showFsHint    = fsName !== '' && fsName !== `${trimmedName}.yml`;

  async function handleSave() {
    if (!trimmedName) { setError('Request name is required.'); return; }
    if (isCreatingNew && !newCollectionName.trim()) {
      setError('Collection name is required.');
      return;
    }
    setSaving(true);
    setError('');
    try {
      let collectionName = selectedCollection;

      if (isCreatingNew) {
        // Import createCollection if it exists; otherwise surface a clear error.
        const { createCollection } = await import('@/lib/tauri-api');
        await createCollection(newCollectionName.trim());
        collectionName = newCollectionName.trim();
      }

      const payload = {
        uid:     tab.id,
        name:    trimmedName,
        method:  tab.request.method,
        url:     tab.request.url,
        headers: tab.request.headers
          .filter((h) => h.key)
          .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
        body:
          tab.request.body.mode !== 'none'
            ? { mode: tab.request.body.mode, content: tab.request.body.content }
            : undefined,
        auth:     { authType: 'none' as const },
        fileName: fsName,
      };

      const saved = await saveRequest(collectionName, fsName, payload);

      const store = usePaneStore.getState();
      store.updateTabSource(tab.id, {
        collection: collectionName,
        path:       saved.fileName ?? fsName,
      });
      store.updateTabTitle(tab.id, trimmedName);
      store.markClean(tab.id);

      onClose();
    } catch (err) {
      console.error('[SaveToCollectionDialog]', err);
      setError('Failed to save. Please try again.');
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) onClose(); }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Save Request</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4 py-2">
          {/* Request name */}
          <div className="flex flex-col gap-1.5">
            <Label className="text-xs font-medium">Request Name</Label>
            <Input
              autoFocus
              placeholder="My Request"
              value={requestName}
              onChange={(e) => { setRequestName(e.target.value); setError(''); }}
              className="h-9"
            />
            {showFsHint && (
              <p className="text-xs text-muted-foreground">
                Saved as: <span className="font-mono">{fsName}</span>
              </p>
            )}
          </div>

          {/* Collection selector */}
          <div className="flex flex-col gap-1.5">
            <Label className="text-xs font-medium">Save to Collection</Label>
            {collections.length > 0 ? (
              <Select
                value={selectedCollection}
                onValueChange={setSelectedCollection}
              >
                <SelectTrigger className="h-9">
                  <SelectValue placeholder="Select collection" />
                </SelectTrigger>
                <SelectContent>
                  {collections.map((c) => (
                    <SelectItem key={c.name} value={c.name}>{c.name}</SelectItem>
                  ))}
                  <SelectItem value={NEW_COLLECTION}>+ New Collection</SelectItem>
                </SelectContent>
              </Select>
            ) : (
              <p className="text-xs text-muted-foreground">
                No collections found — a new one will be created.
              </p>
            )}
          </div>

          {/* New collection name input — only when creating new */}
          {isCreatingNew && (
            <div className="flex flex-col gap-1.5">
              <Label className="text-xs font-medium">New Collection Name</Label>
              <Input
                placeholder="My Collection"
                value={newCollectionName}
                onChange={(e) => { setNewCollectionName(e.target.value); setError(''); }}
                className="h-9"
              />
            </div>
          )}

          {error && <p className="text-xs text-destructive">{error}</p>}
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={() => void handleSave()}
            disabled={saving || !trimmedName}
          >
            {saving ? 'Saving…' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 4: Add "Save to Collection" button to `RequestPanel`**

In `src/components/request/RequestPanel.tsx`:

1. Add imports:
```tsx
import { SaveToCollectionDialog } from './SaveToCollectionDialog';
```

2. Add state near other `useState` calls:
```tsx
const [saveToCollectionOpen, setSaveToCollectionOpen] = useState(false);
```

3. In the toolbar area (near the existing `<SaveRequestButton>` render), add the button and dialog — shown only when the tab has no `source`:
```tsx
{!tab.source && (
  <>
    <Button
      size="sm"
      variant="outline"
      className="h-8 px-3"
      onClick={() => setSaveToCollectionOpen(true)}
    >
      Save to Collection
    </Button>
    <SaveToCollectionDialog
      open={saveToCollectionOpen}
      tab={tab}
      onClose={() => setSaveToCollectionOpen(false)}
    />
  </>
)}
```

4. Listen for the `rocket:save-to-collection` custom event (dispatched by `Cmd+S` when sourceless):
```tsx
useEffect(() => {
  const handler = (e: Event) => {
    const detail = (e as CustomEvent<{ tabId: string }>).detail;
    if (detail?.tabId === tab.id) setSaveToCollectionOpen(true);
  };
  window.addEventListener('rocket:save-to-collection', handler);
  return () => window.removeEventListener('rocket:save-to-collection', handler);
}, [tab.id]);
```

- [ ] **Step 5: Update `useKeyboardShortcuts` for sourceless `Cmd+S`**

In `src/hooks/useKeyboardShortcuts.ts`, find the `Cmd+S` handler block and replace it:

```ts
if (e.key === 's') {
  e.preventDefault();
  const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
  if (!tab) return;
  if (isRequestTab(tab) && !tab.source) {
    // Sourceless tab — open the Save to Collection dialog.
    window.dispatchEvent(
      new CustomEvent('rocket:save-to-collection', { detail: { tabId: tab.id } }),
    );
  } else {
    // Normal save — auto-save to existing source.
    window.dispatchEvent(
      new CustomEvent('rocket:save-draft', { detail: { tabId: tab.id } }),
    );
  }
  return;
}
```

- [ ] **Step 6: Manual smoke test**

```bash
yarn tauri dev
```

1. Click `+` tab button (from SP-C, or manually call `openEphemeralTab` from devtools) → "Untitled" tab opens
2. Set method POST, URL `https://api.example.com/login`
3. "Save to Collection" button visible in toolbar
4. Click it → dialog opens, collections list populated
5. Select existing collection, name the request "POST Login", click Save
6. Tab title updates to "POST Login", source is now bound, dirty indicator clears
7. Check disk → `.yml` file in the chosen collection

Also test `Cmd+S` on sourceless tab → same dialog opens.
Also test `Cmd+S` on a tab WITH source → normal save (no dialog).

- [ ] **Step 7: Commit**

```bash
git add src/components/request/SaveToCollectionDialog.tsx \
        src/components/request/RequestPanel.tsx \
        src/hooks/useKeyboardShortcuts.ts
git commit -m "feat: add SaveToCollectionDialog and Cmd+S routing for ephemeral tabs"
```

---

## Definition of Done

- [ ] `CreateRequestDialog` renders from `CollectionNode` `···` menu
- [ ] Dialog fields: Request Type, Name (with filesystem hint), Method (conditional), URL
- [ ] On Create: file saved to disk, tab opens with `source` bound
- [ ] `SaveToCollectionDialog` renders from "Save to Collection" button in `RequestPanel` when `!tab.source`
- [ ] `Cmd+S` on sourceless tab → opens `SaveToCollectionDialog` (not auto-save)
- [ ] `Cmd+S` on sourced tab → normal auto-save (unchanged behaviour)
- [ ] After saving: tab title, source, and dirty state all update correctly
- [ ] `yarn vitest run` → no regressions
