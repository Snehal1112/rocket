# Fix: Save Request UI — Missing Save Button + Save Flow

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY:** Every interactive UI element MUST use a shadcn/ui component. No raw HTML elements.

**Goal:** Add the complete "save request" flow to the UI — save button in the request panel, Cmd+S keyboard shortcut, save-as dialog for draft tabs, auto-detection of dirty state, and proper dirty dot clearing after save.

**Architecture:** Save delegates to the existing Tauri `save_request` command via `invoke()`. Draft tabs (unsaved new requests) show a "Save As" dialog to pick collection + path. Tabs linked to a collection request save directly.

**Tech Stack:** React, TypeScript, Zustand, shadcn/ui, Tauri invoke

---

## File Structure

```
frontend/src/
  components/
    request/
      SaveRequestButton.tsx       # Save / Save As button (shadcn Button + DropdownMenu)
      SaveAsDialog.tsx             # Dialog to pick collection + folder + filename
    panes/
      TabBar.tsx                   # Modified: add save action to context menu
  stores/
    pane-store.ts                  # Modified: add saveRequest action
  hooks/
    useKeyboardShortcuts.ts        # Modified: wire Cmd+S to save active tab
  lib/
    tauri-api.ts                   # Verify: saveRequest function exists
```

---

## Task 1: Verify Rust backend save_request command exists

- [ ] **Step 1: Check the Tauri command is registered**

```bash
grep -rn "save_request" src-tauri/src/ --include="*.rs"
```

Expected: find `save_request` in `commands/collections.rs` and registered in `lib.rs` `generate_handler![]`.

If missing, add:
```rust
// src-tauri/src/commands/collections.rs
#[tauri::command]
pub fn save_request(
    collection: String,
    request_path: String,
    data: rocket_collection::Request,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.save_request(&collection, &request_path, &data)
}
```

And register in `lib.rs`:
```rust
commands::collections::save_request,
```

- [ ] **Step 2: Verify tauri-api.ts has saveRequest**

```bash
grep -n "saveRequest\|save_request" frontend/src/lib/tauri-api.ts
```

If missing, add:
```typescript
export async function saveRequest(
  collection: string,
  requestPath: string,
  data: RequestFile,
): Promise<void> {
  return invoke('save_request', { collection, requestPath, data });
}
```

And ensure it's re-exported from `api.ts`.

- [ ] **Step 3: Commit if changes were needed**

```bash
git add -A && git commit -m "fix: ensure save_request command exists in backend + API bridge"
```

---

## Task 2: Create SaveAsDialog component

**Files:**
- Create: `frontend/src/components/request/SaveAsDialog.tsx`

This dialog appears when saving a draft tab (no `source` collection) for the first time, or when explicitly choosing "Save As".

- [ ] **Step 1: Implement SaveAsDialog using shadcn components**

```tsx
import { useState, useEffect } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { listCollections } from '@/lib/api';
import type { CollectionInfo } from '@/lib/tauri-api';

interface SaveAsDialogProps {
  open: boolean;
  onClose: () => void;
  onSave: (collection: string, folderPath: string, fileName: string) => void;
  defaultName?: string;
}

export function SaveAsDialog({ open, onClose, onSave, defaultName = '' }: SaveAsDialogProps) {
  const [collections, setCollections] = useState<CollectionInfo[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [folderPath, setFolderPath] = useState('');
  const [fileName, setFileName] = useState(defaultName);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (open) {
      listCollections().then(setCollections).catch(console.error);
    }
  }, [open]);

  useEffect(() => {
    if (defaultName) {
      // Sanitize: lowercase, replace spaces with hyphens, add .json extension
      const sanitized = defaultName
        .toLowerCase()
        .replace(/[^a-z0-9-_]/g, '-')
        .replace(/-+/g, '-');
      setFileName(sanitized.endsWith('.json') ? sanitized : `${sanitized}.json`);
    }
  }, [defaultName]);

  const canSave = selectedCollection && fileName.trim();

  const handleSave = () => {
    if (!canSave) return;
    setLoading(true);
    const path = folderPath
      ? `${folderPath.replace(/^\/|\/$/g, '')}/${fileName}`
      : fileName;
    onSave(selectedCollection, path, fileName);
    setLoading(false);
  };

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Save request</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="space-y-2">
            <Label>Collection</Label>
            <Select value={selectedCollection} onValueChange={setSelectedCollection}>
              <SelectTrigger>
                <SelectValue placeholder="Select a collection" />
              </SelectTrigger>
              <SelectContent>
                {collections.map((c) => (
                  <SelectItem key={c.name} value={c.name}>
                    {c.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label>Folder path (optional)</Label>
            <Input
              value={folderPath}
              onChange={(e) => setFolderPath(e.target.value)}
              placeholder="e.g. auth/login"
            />
          </div>
          <div className="space-y-2">
            <Label>File name</Label>
            <Input
              value={fileName}
              onChange={(e) => setFileName(e.target.value)}
              placeholder="e.g. get-users.json"
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={!canSave || loading}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/request/SaveAsDialog.tsx
git commit -m "feat: SaveAsDialog — pick collection + path for new requests"
```

---

## Task 3: Create SaveRequestButton component

**Files:**
- Create: `frontend/src/components/request/SaveRequestButton.tsx`

- [ ] **Step 1: Implement SaveRequestButton**

```tsx
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Save, ChevronDown } from 'lucide-react';
import { SaveAsDialog } from './SaveAsDialog';
import { saveRequest } from '@/lib/api';
import { usePaneStore } from '@/stores/pane-store';
import type { Tab } from '@/types/pane-types';

interface SaveRequestButtonProps {
  tab: Tab;
  groupId: string;
}

export function SaveRequestButton({ tab, groupId }: SaveRequestButtonProps) {
  const [saveAsOpen, setSaveAsOpen] = useState(false);
  const { markClean, updateTabSource } = usePaneStore();

  const handleSave = async () => {
    if (tab.source) {
      // Save directly to existing collection/path
      try {
        await saveRequest(tab.source.collection, tab.source.path, {
          name: tab.title,
          method: tab.request.method,
          url: tab.request.url,
          headers: tab.request.headers.filter((h) => h.key), // filter empty rows
          body: tab.request.body.mode !== 'none' ? {
            mode: tab.request.body.mode,
            content: tab.request.body.content || undefined,
            formData: tab.request.body.formData.length > 0 ? tab.request.body.formData : undefined,
          } : undefined,
          auth: tab.request.auth,
        });
        markClean(tab.id);
      } catch (err) {
        console.error('Save failed:', err);
      }
    } else {
      // No source — open Save As dialog
      setSaveAsOpen(true);
    }
  };

  const handleSaveAs = async (collection: string, path: string, fileName: string) => {
    try {
      await saveRequest(collection, path, {
        name: tab.title || fileName.replace('.json', ''),
        method: tab.request.method,
        url: tab.request.url,
        headers: tab.request.headers.filter((h) => h.key),
        body: tab.request.body.mode !== 'none' ? {
          mode: tab.request.body.mode,
          content: tab.request.body.content || undefined,
          formData: tab.request.body.formData.length > 0 ? tab.request.body.formData : undefined,
        } : undefined,
        auth: tab.request.auth,
      });
      // Update tab to point to saved location
      if (updateTabSource) {
        updateTabSource(tab.id, { collection, path });
      }
      markClean(tab.id);
      setSaveAsOpen(false);
    } catch (err) {
      console.error('Save As failed:', err);
    }
  };

  const hasDrop = tab.source != null; // Show dropdown only if already saved (to offer Save As)

  return (
    <>
      {hasDrop ? (
        <DropdownMenu>
          <div className="flex">
            <Button
              size="sm"
              variant="outline"
              className="rounded-r-none"
              onClick={handleSave}
              disabled={!tab.isDirty}
            >
              <Save className="h-3.5 w-3.5 mr-1.5" />
              Save
            </Button>
            <DropdownMenuTrigger asChild>
              <Button size="sm" variant="outline" className="rounded-l-none border-l-0 px-1.5">
                <ChevronDown className="h-3.5 w-3.5" />
              </Button>
            </DropdownMenuTrigger>
          </div>
          <DropdownMenuContent>
            <DropdownMenuItem onClick={handleSave} disabled={!tab.isDirty}>
              Save
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSaveAsOpen(true)}>
              Save as...
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      ) : (
        <Button size="sm" variant="outline" onClick={handleSave}>
          <Save className="h-3.5 w-3.5 mr-1.5" />
          Save
        </Button>
      )}

      <SaveAsDialog
        open={saveAsOpen}
        onClose={() => setSaveAsOpen(false)}
        onSave={handleSaveAs}
        defaultName={tab.title}
      />
    </>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/request/SaveRequestButton.tsx
git commit -m "feat: SaveRequestButton — save + save-as with dropdown"
```

---

## Task 4: Add updateTabSource action to pane store

**Files:**
- Modify: `frontend/src/stores/pane-store.ts`

- [ ] **Step 1: Add updateTabSource to the store**

Add to the PaneState interface:
```typescript
updateTabSource: (tabId: string, source: { collection: string; path: string }) => void;
```

Implementation:
```typescript
updateTabSource: (tabId, source) => {
  set((state) => ({
    root: updateTabInTree(state.root, tabId, (tab) => ({
      ...tab,
      source,
      tabType: 'request', // promote from draft to saved request
    })),
  }));
},
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/stores/pane-store.ts
git commit -m "feat: add updateTabSource action to pane store"
```

---

## Task 5: Wire SaveRequestButton into RequestPanel

**Files:**
- Modify: `frontend/src/components/request/RequestPanel.tsx`

- [ ] **Step 1: Add SaveRequestButton next to Send button in the URL bar**

Find the URL bar section (where the method Select, URL Input, and Send Button live) and add SaveRequestButton:

```tsx
import { SaveRequestButton } from './SaveRequestButton';

// In the URL bar area, after the Send button:
<div className="flex items-center gap-2">
  <SaveRequestButton tab={tab} groupId={groupId} />
  <Button onClick={handleSend} disabled={isLoading}>
    <Send className="h-4 w-4 mr-2" />
    Send
  </Button>
</div>
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/request/RequestPanel.tsx
git commit -m "feat: wire SaveRequestButton into RequestPanel URL bar"
```

---

## Task 6: Wire Cmd+S keyboard shortcut

**Files:**
- Modify: `frontend/src/hooks/useKeyboardShortcuts.ts`

- [ ] **Step 1: Add Cmd+S handler**

Find the existing keyboard shortcuts hook and add/verify Cmd+S:

```typescript
// Inside the keydown handler:
if ((e.metaKey || e.ctrlKey) && e.key === 's') {
  e.preventDefault();
  const { root, activeGroupId } = usePaneStore.getState();
  const leaf = findLeaf(root, activeGroupId);
  if (!leaf) return;
  const activeTab = leaf.tabs.find((t) => t.id === leaf.activeTabId);
  if (!activeTab || !activeTab.isDirty) return;

  if (activeTab.source) {
    // Direct save
    saveRequest(activeTab.source.collection, activeTab.source.path, {
      name: activeTab.title,
      method: activeTab.request.method,
      url: activeTab.request.url,
      headers: activeTab.request.headers.filter((h) => h.key),
      body: activeTab.request.body.mode !== 'none' ? {
        mode: activeTab.request.body.mode,
        content: activeTab.request.body.content || undefined,
        formData: activeTab.request.body.formData.length > 0 ? activeTab.request.body.formData : undefined,
      } : undefined,
      auth: activeTab.request.auth,
    }).then(() => {
      usePaneStore.getState().markClean(activeTab.id);
    }).catch(console.error);
  } else {
    // Draft — need to trigger Save As dialog
    // Dispatch a custom event that SaveRequestButton listens to
    window.dispatchEvent(new CustomEvent('rocket:save-as', { detail: { tabId: activeTab.id } }));
  }
}
```

- [ ] **Step 2: In SaveRequestButton, listen for the custom event**

```typescript
useEffect(() => {
  const handler = (e: CustomEvent) => {
    if (e.detail.tabId === tab.id) {
      setSaveAsOpen(true);
    }
  };
  window.addEventListener('rocket:save-as', handler as EventListener);
  return () => window.removeEventListener('rocket:save-as', handler as EventListener);
}, [tab.id]);
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/hooks/useKeyboardShortcuts.ts frontend/src/components/request/SaveRequestButton.tsx
git commit -m "feat: Cmd+S saves active tab (direct or Save As)"
```

---

## Task 7: Add save to tab context menu

**Files:**
- Modify: `frontend/src/components/panes/TabBar.tsx`

- [ ] **Step 1: Add "Save" to the context menu**

In `TabBar.tsx`, add a Save item to the ContextMenu:

```tsx
<ContextMenuItem
  onClick={() => {
    // Trigger save for this specific tab
    if (tab.source && tab.isDirty) {
      saveRequest(tab.source.collection, tab.source.path, { /* ... */ })
        .then(() => markClean(tab.id));
    } else if (!tab.source) {
      window.dispatchEvent(new CustomEvent('rocket:save-as', { detail: { tabId: tab.id } }));
    }
  }}
  disabled={!tab.isDirty && !!tab.source}
>
  Save {tab.source ? '' : 'as...'}
</ContextMenuItem>
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/panes/TabBar.tsx
git commit -m "feat: add Save to tab context menu"
```

---

## Task 8: Verify end-to-end

- [ ] **Step 1: Test save flow**

```bash
cargo tauri dev
```

Verify:
- [ ] Save button visible next to Send button
- [ ] New draft tab: clicking Save opens Save As dialog with collection picker
- [ ] Save As dialog: select collection, enter path, save → file appears on disk
- [ ] After save: dirty dot clears, tab type changes from draft to request
- [ ] Edit saved request → dirty dot appears → click Save → saves directly (no dialog)
- [ ] Save dropdown: "Save" + "Save as..." options for already-saved requests
- [ ] Cmd+S: saves active tab (direct save or opens Save As for drafts)
- [ ] Right-click tab → "Save" in context menu
- [ ] Close unsaved tab → AlertDialog still works

- [ ] **Step 2: Commit any fixes**

```bash
git add -A && git commit -m "fix: save request end-to-end verification"
```

---

## Milestone Checklist

- [ ] Save button exists in RequestPanel (next to Send)
- [ ] Save As dialog with collection picker + folder path + filename
- [ ] Direct save for requests with existing source
- [ ] Draft tabs get Save As flow
- [ ] Cmd+S keyboard shortcut works
- [ ] Tab context menu has Save option
- [ ] Dirty dot clears after successful save
- [ ] File persists on disk at ~/.rocket-api/collections/{collection}/{path}
- [ ] All UI uses shadcn components (Button, Dialog, Select, Input, Label, DropdownMenu)
