# Request Editor UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve request tab usability with a shared KeyValueEditor, pre-send URL validation, a sending spinner, save feedback, and an unsaved changes warning dialog.

**Architecture:** Task 1 creates the shared KeyValueEditor component. Tasks 2-3 replace the 4 duplicate editors with it. Task 4 adds URL validation to RequestPanel. Task 5 adds a sending spinner. Task 6 adds save feedback. Task 7 adds the unsaved changes warning. Tasks 1→2→3 are sequential. Tasks 4-7 are independent of each other (all modify different files/sections).

**Tech Stack:** React, TypeScript, Tailwind CSS, Zustand (`yarn tsc --noEmit` for verification)

**Spec:** `docs/superpowers/specs/2026-03-26-request-editor-ux-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/components/request/KeyValueEditor.tsx` | Create — shared key-value editor component |
| `src/components/request/HeadersEditor.tsx` | Simplify — thin wrapper around KeyValueEditor |
| `src/components/request/QueryParamsEditor.tsx` | Simplify — thin wrapper around KeyValueEditor |
| `src/components/request/PathParamsPanel.tsx` | Simplify — thin wrapper around KeyValueEditor |
| `src/components/request/BodyEditor.tsx` | Simplify FormDataEditor — use KeyValueEditor |
| `src/components/request/RequestPanel.tsx` | Add URL validation + sending spinner |
| `src/components/request/SaveRequestButton.tsx` | Add success/error inline feedback |
| `src/components/panes/EditorGroup.tsx` | Add unsaved changes warning dialog |

---

### Task 1: Create KeyValueEditor component

**Files:**
- Create: `src/components/request/KeyValueEditor.tsx`

- [ ] **Step 1: Create the shared component**

Create `src/components/request/KeyValueEditor.tsx` with the following content:

```tsx
import { useCallback } from 'react';
import { Check, X, Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { KeyValueEntry } from '@/types/pane-types';

interface KeyValueEditorProps {
  entries: KeyValueEntry[];
  onChange: (entries: KeyValueEntry[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  addLabel?: string;
  label?: string;
}

export function KeyValueEditor({
  entries,
  onChange,
  keyPlaceholder = 'Key',
  valuePlaceholder = 'Value',
  addLabel = 'Add Entry',
  label,
}: KeyValueEditorProps) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) => {
      onChange(entries.map((e) => (e.id === id ? { ...e, ...patch } : e)));
    },
    [entries, onChange],
  );

  const removeEntry = useCallback(
    (id: string) => {
      onChange(entries.filter((e) => e.id !== id));
    },
    [entries, onChange],
  );

  const addEntry = useCallback(() => {
    onChange([
      ...entries,
      { id: crypto.randomUUID(), key: '', value: '', enabled: true },
    ]);
  }, [entries, onChange]);

  return (
    <div className="space-y-2">
      {label && <div className="text-sm font-medium text-muted-foreground">{label}</div>}
      {entries.map((entry) => (
        <div key={entry.id} className="flex gap-2 items-center">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => updateEntry(entry.id, { enabled: !entry.enabled })}
            className={`w-4 h-4 rounded border p-0 ${
              entry.enabled
                ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
                : 'border-gray-300 hover:bg-muted'
            }`}
            aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key || 'unnamed'}`}
          >
            {entry.enabled && <Check className="h-3 w-3" />}
          </Button>
          <Input
            placeholder={keyPlaceholder}
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
            className="flex-1 text-xs h-8"
          />
          <Input
            placeholder={valuePlaceholder}
            value={entry.value}
            onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
            className="flex-1 text-xs h-8"
          />
          <Button
            variant="ghost"
            size="icon"
            onClick={() => removeEntry(entry.id)}
            className="h-7 w-7"
            aria-label={`Remove ${entry.key || 'unnamed'}`}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      ))}
      <Button variant="ghost" size="sm" onClick={addEntry} className="text-xs">
        <Plus className="h-3 w-3 mr-1" />
        {addLabel}
      </Button>
    </div>
  );
}
```

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/KeyValueEditor.tsx
git commit -m "feat: create shared KeyValueEditor component"
```

---

### Task 2: Replace HeadersEditor, QueryParamsEditor, PathParamsPanel with KeyValueEditor

**Files:**
- Modify: `src/components/request/HeadersEditor.tsx`
- Modify: `src/components/request/QueryParamsEditor.tsx`
- Modify: `src/components/request/PathParamsPanel.tsx`

**Depends on:** Task 1

- [ ] **Step 1: Replace HeadersEditor**

Replace the entire contents of `src/components/request/HeadersEditor.tsx` with:

```tsx
import { KeyValueEditor } from './KeyValueEditor';
import type { KeyValueEntry } from '@/types/pane-types';

interface HeadersEditorProps {
  headers: KeyValueEntry[];
  onChange: (headers: KeyValueEntry[]) => void;
}

export function HeadersEditor({ headers, onChange }: HeadersEditorProps) {
  return (
    <KeyValueEditor
      entries={headers}
      onChange={onChange}
      keyPlaceholder="Header name"
      valuePlaceholder="Value"
      addLabel="Add Header"
    />
  );
}
```

- [ ] **Step 2: Replace QueryParamsEditor**

Replace the entire contents of `src/components/request/QueryParamsEditor.tsx` with:

```tsx
import { KeyValueEditor } from './KeyValueEditor';
import type { KeyValueEntry } from '@/types/pane-types';

interface QueryParamsEditorProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
}

export function QueryParamsEditor({ params, onChange }: QueryParamsEditorProps) {
  return (
    <KeyValueEditor
      entries={params}
      onChange={onChange}
      keyPlaceholder="Param name"
      valuePlaceholder="Value"
      addLabel="Add Query Param"
      label="Query Params"
    />
  );
}
```

- [ ] **Step 3: Replace PathParamsPanel**

Replace the entire contents of `src/components/request/PathParamsPanel.tsx` with:

```tsx
import { KeyValueEditor } from './KeyValueEditor';
import type { KeyValueEntry } from '@/types/pane-types';

interface PathParamsPanelProps {
  params: KeyValueEntry[];
  onChange: (params: KeyValueEntry[]) => void;
}

export function PathParamsPanel({ params, onChange }: PathParamsPanelProps) {
  return (
    <KeyValueEditor
      entries={params}
      onChange={onChange}
      keyPlaceholder="Path key (e.g. customerId)"
      valuePlaceholder="Value"
      addLabel="Add Path Param"
      label="Path Params"
    />
  );
}
```

- [ ] **Step 4: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/HeadersEditor.tsx src/components/request/QueryParamsEditor.tsx src/components/request/PathParamsPanel.tsx
git commit -m "refactor: replace Headers/QueryParams/PathParams editors with KeyValueEditor"
```

---

### Task 3: Replace FormDataEditor in BodyEditor with KeyValueEditor

**Files:**
- Modify: `src/components/request/BodyEditor.tsx`

**Depends on:** Task 1

- [ ] **Step 1: Replace FormDataEditor with KeyValueEditor**

In `src/components/request/BodyEditor.tsx`:

1. Add import at the top (after existing imports):
```tsx
import { KeyValueEditor } from './KeyValueEditor';
```

2. Find the usage of `FormDataEditor` (~line 118-120):
```tsx
      {body.mode === 'formdata' && (
        <FormDataEditor formData={body.formData} onChange={setFormData} />
      )}
```

Replace with:
```tsx
      {body.mode === 'formdata' && (
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder="Field name"
          valuePlaceholder="Value"
          addLabel="Add Field"
        />
      )}
```

3. Delete the entire `FormDataEditor` sub-component (lines 144-219, from `// Sub-component for form data entries` through the closing `}`).

4. Remove unused imports that were only used by FormDataEditor: `Check`, `X`, `Plus` from lucide-react (check if `Plus` is still needed — it's not used elsewhere in this file, so remove it; `Check` and `X` are also not used elsewhere). Also remove the `Button` and `Input` imports if they're only used by FormDataEditor (check: `Button` is used for binary file picker, keep it; `Input` is not used elsewhere, remove it).

After cleanup, the lucide-react import should be:
```tsx
import { FileUp } from 'lucide-react';
```

And remove the `Input` import line.

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/BodyEditor.tsx
git commit -m "refactor: replace FormDataEditor with KeyValueEditor in BodyEditor"
```

---

### Task 4: Add URL validation to RequestPanel

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

- [ ] **Step 1: Add urlError state**

Find (~line 69-70):
```tsx
  const [activeSection, setActiveSection] = useState<SectionTab>('params');
  const [unsavedDialogOpen, setUnsavedDialogOpen] = useState(false);
```

Add after:
```tsx
  const [urlError, setUrlError] = useState('');
```

- [ ] **Step 2: Add validation to send handler**

Find the Send button (~line 172):
```tsx
          <Button size="sm" className="h-8 px-3" disabled={sending} onClick={() => send(request)}>
```

Replace with:
```tsx
          <Button size="sm" className="h-8 px-3" disabled={sending} onClick={() => {
            const url = request.url.trim();
            if (!url) { setUrlError('URL is required'); return; }
            try { new URL(url); } catch { setUrlError('Invalid URL — include http:// or https://'); return; }
            setUrlError('');
            send(request);
          }}>
```

- [ ] **Step 3: Clear error on URL change**

Find the URL Input onChange (~line 168):
```tsx
            onChange={(e) => handleUrlChange(e.target.value)}
```

Replace with:
```tsx
            onChange={(e) => { setUrlError(''); handleUrlChange(e.target.value); }}
```

- [ ] **Step 4: Display error message below URL bar**

Find the closing `</div>` of the URL bar container (the `</div>` after `<SaveRequestButton>`, ~line 178):
```tsx
          <SaveRequestButton tab={tab} groupId={_groupId} />
        </div>
```

Replace with:
```tsx
          <SaveRequestButton tab={tab} groupId={_groupId} />
        </div>
        {urlError && (
          <p className="text-2xs text-destructive px-3 py-1">{urlError}</p>
        )}
```

- [ ] **Step 5: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/RequestPanel.tsx
git commit -m "feat: add URL validation before sending — shows inline error for empty or invalid URLs"
```

---

### Task 5: Add sending spinner to response area

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

- [ ] **Step 1: Add Loader2 import**

Find (~line 2):
```tsx
import { Send } from 'lucide-react';
```

Replace with:
```tsx
import { Send, Loader2 } from 'lucide-react';
```

- [ ] **Step 2: Add spinner state to response area**

Find the response area section (~lines 263-281):
```tsx
      <div className="flex-1 flex flex-col overflow-hidden bg-card/65 min-h-0">
        {response ? (
          <ResponseBodyViewer response={response} />
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <RocketLiftOff className="w-24 h-24" />
            <p className="text-sm font-medium text-foreground">Ready for liftoff</p>
            <p className="text-xs text-muted-foreground">
              Send a request to see the response here
            </p>
            <p className="text-xs text-muted-foreground mt-1">
              Press{' '}
              <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-2xs">
                Ctrl+Enter
              </kbd>
              {' '}to send
            </p>
          </div>
        )}
      </div>
```

Replace with:
```tsx
      <div className="flex-1 flex flex-col overflow-hidden bg-card/65 min-h-0">
        {sending ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
            <p className="text-sm text-muted-foreground">Sending request...</p>
          </div>
        ) : response ? (
          <ResponseBodyViewer response={response} />
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <RocketLiftOff className="w-24 h-24" />
            <p className="text-sm font-medium text-foreground">Ready for liftoff</p>
            <p className="text-xs text-muted-foreground">
              Send a request to see the response here
            </p>
            <p className="text-xs text-muted-foreground mt-1">
              Press{' '}
              <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-2xs">
                Ctrl+Enter
              </kbd>
              {' '}to send
            </p>
          </div>
        )}
      </div>
```

- [ ] **Step 3: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 4: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/RequestPanel.tsx
git commit -m "feat: show spinner in response area while request is in-flight"
```

---

### Task 6: Add save success/failure feedback to SaveRequestButton

**Files:**
- Modify: `src/components/request/SaveRequestButton.tsx`

- [ ] **Step 1: Add state and imports**

Find (~line 1-2):
```tsx
import { useEffect, useCallback } from 'react';
import { Save } from 'lucide-react';
```

Replace with:
```tsx
import { useState, useEffect, useCallback } from 'react';
import { Save, Check } from 'lucide-react';
```

- [ ] **Step 2: Add saveStatus state and update handleSave**

Find (~lines 76-87):
```tsx
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
```

Replace with:
```tsx
export function SaveRequestButton({ tab }: SaveRequestButtonProps) {
  const markClean = usePaneStore((s) => s.markClean);
  const [saveStatus, setSaveStatus] = useState<'idle' | 'success' | 'error'>('idle');

  const handleSave = useCallback(async () => {
    if (!tab.source) return;
    try {
      await saveRequest(tab.source.collection, tab.source.path, buildPayloadFromTab(tab));
      markClean(tab.id);
      setSaveStatus('success');
      setTimeout(() => setSaveStatus('idle'), 2000);
    } catch (err) {
      console.error('[SaveRequestButton] Save failed:', err);
      setSaveStatus('error');
      setTimeout(() => setSaveStatus('idle'), 3000);
    }
  }, [tab, markClean]);
```

- [ ] **Step 3: Update the button JSX to show feedback**

Find (~lines 102-113):
```tsx
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
```

Replace with:
```tsx
  return (
    <div className="flex items-center gap-1.5">
      <Button
        size="sm"
        variant="outline"
        className="h-8 px-3"
        disabled={!tab.isDirty}
        onClick={() => void handleSave()}
      >
        {saveStatus === 'success' ? (
          <Check className="mr-1 h-3.5 w-3.5 text-green-500" />
        ) : (
          <Save className="mr-1 h-3.5 w-3.5" />
        )}
        Save
      </Button>
      {saveStatus === 'error' && (
        <span className="text-2xs text-destructive">Save failed</span>
      )}
    </div>
  );
```

- [ ] **Step 4: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/SaveRequestButton.tsx
git commit -m "feat: show inline success/error feedback on save"
```

---

### Task 7: Add unsaved changes warning on tab close

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`
- Modify: `src/stores/pane-store.ts`

- [ ] **Step 1: Add pending close state and dialog to EditorGroup**

In `src/components/panes/EditorGroup.tsx`, add imports at the top:

```tsx
import { useState } from 'react';
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
import { usePaneStore } from '@/stores/pane-store';
```

- [ ] **Step 2: Add close interception logic to EditorGroup**

Find the `EditorGroup` function (~line 34):
```tsx
export function EditorGroup({ node }: { node: LeafNode }) {
  const activeTab = node.tabs.find((t) => t.id === node.activeTabId);
  const hasTabs = node.tabs.length > 0;
```

Replace with:
```tsx
export function EditorGroup({ node }: { node: LeafNode }) {
  const activeTab = node.tabs.find((t) => t.id === node.activeTabId);
  const hasTabs = node.tabs.length > 0;
  const closeTab = usePaneStore((s) => s.closeTab);
  const [pendingCloseTabId, setPendingCloseTabId] = useState<string | null>(null);

  const handleCloseTab = (tabId: string) => {
    const tab = node.tabs.find((t) => t.id === tabId);
    if (tab && tab.isDirty && !tab.source) {
      setPendingCloseTabId(tabId);
    } else {
      closeTab(tabId, node.groupId);
    }
  };
```

- [ ] **Step 3: Pass handleCloseTab to TabBar**

Find:
```tsx
      {hasTabs && <TabBar node={node} />}
```

Replace with:
```tsx
      {hasTabs && <TabBar node={node} onCloseTab={handleCloseTab} />}
```

- [ ] **Step 4: Add the AlertDialog JSX**

Find the closing `</div>` of the EditorGroup return (the last `</div>` before the function closes):

Add BEFORE that closing `</div>`:
```tsx
      <AlertDialog open={!!pendingCloseTabId} onOpenChange={(open) => { if (!open) setPendingCloseTabId(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Unsaved Changes</AlertDialogTitle>
            <AlertDialogDescription>
              This request has never been saved to a collection. Close anyway?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => {
              if (pendingCloseTabId) closeTab(pendingCloseTabId, node.groupId);
              setPendingCloseTabId(null);
            }}>
              Close
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
```

- [ ] **Step 5: Update TabBar to accept onCloseTab prop**

In `src/components/panes/TabBar.tsx`, find (~line 17):
```tsx
export function TabBar({ node }: { node: LeafNode }) {
```

Replace with:
```tsx
export function TabBar({ node, onCloseTab }: { node: LeafNode; onCloseTab?: (tabId: string) => void }) {
```

Find the close tab usage in the context menu (~line 88):
```tsx
            <ContextMenuItem onClick={() => closeTab(tab.id, node.groupId)}>
              Close
            </ContextMenuItem>
```

Replace with:
```tsx
            <ContextMenuItem onClick={() => (onCloseTab ?? closeTab)(tab.id, onCloseTab ? undefined as any : node.groupId)}>
              Close
            </ContextMenuItem>
```

Actually, a cleaner approach: use the `onCloseTab` if provided, otherwise fall back to the store's `closeTab`:

```tsx
            <ContextMenuItem onClick={() => onCloseTab ? onCloseTab(tab.id) : closeTab(tab.id, node.groupId)}>
              Close
            </ContextMenuItem>
```

Also update the TabItem close button. Find where `onClose` is passed to TabItem (~line 61):
```tsx
                  onClose={() => closeTab(tab.id, node.groupId)}
```

Replace with:
```tsx
                  onClose={() => onCloseTab ? onCloseTab(tab.id) : closeTab(tab.id, node.groupId)}
```

And update "Close Others" (~lines 91-98):
```tsx
            <ContextMenuItem
              onClick={() => {
                node.tabs
                  .filter((t) => t.id !== tab.id)
                  .forEach((t) => closeTab(t.id, node.groupId));
              }}
            >
              Close Others
            </ContextMenuItem>
```

Replace with:
```tsx
            <ContextMenuItem
              onClick={() => {
                node.tabs
                  .filter((t) => t.id !== tab.id)
                  .forEach((t) => onCloseTab ? onCloseTab(t.id) : closeTab(t.id, node.groupId));
              }}
            >
              Close Others
            </ContextMenuItem>
```

- [ ] **Step 6: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 7: Build check**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn build 2>&1 | tail -10
```

- [ ] **Step 8: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/panes/EditorGroup.tsx src/components/panes/TabBar.tsx
git commit -m "feat: warn before closing tabs with unsaved changes that have no collection source"
```

---

## Done

Request Editor UX improvements complete:
- Shared `KeyValueEditor` replaces ~240 lines of duplicate code across 4 editors
- URL validation prevents sending requests with empty or invalid URLs
- Spinner shown in response area while request is in-flight
- Save button shows green checkmark on success, red error text on failure
- Unsaved changes warning dialog when closing a tab that was never saved to a collection
