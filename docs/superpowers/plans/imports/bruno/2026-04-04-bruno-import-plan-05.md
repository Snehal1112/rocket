# Bruno Import — Plan 05: Frontend UI — `ImportBrunoDialog`

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `ImportBrunoDialog` React component (three internal states: picking → importing → complete), wire up the three UI entry points (File menu, Workspace overview, Collections toolbar), and connect to the Tauri commands from plan-01.

**Architecture:** Single shadcn/ui `Dialog` with internal state machine. All interactive elements use shadcn/ui primitives exclusively — no raw `<button>`, `<input>`, `<select>`, or `<dialog>` tags. Lucide icons only. State is local to the dialog (no Zustand store needed).

**Tech Stack:** React 18, TypeScript, shadcn/ui, Lucide React, Tauri v2 `@tauri-apps/api`

**Prerequisite:** Plans 01–04 complete (Tauri commands registered and functional).

**Spec:** `docs/superpowers/specs/2026-04-04-bruno-import-design.md`

---

## Task 1: Tauri IPC types + hook

**Files:**
- Create: `src/features/import/types.ts`
- Create: `src/features/import/useImport.ts`

- [ ] **Step 1: Create `src/features/import/types.ts`**

```typescript
export interface ImportReport {
  totalFiles: number;
  imported: number;
  skipped: SkippedItem[];
  createdWorkspace: string | null;
  createdCollections: string[];
}

export interface SkippedItem {
  path: string;
  reason: SkipReason;
}

export type SkipReason =
  | { type: 'unsupportedRequestType'; detail: string }
  | { type: 'unsupportedAuthType'; detail: string }
  | { type: 'parseError'; detail: string };

export interface ImportCollectionOptions {
  path: string;
  targetWorkspaceId: string;
}

export interface ImportWorkspaceOptions {
  path: string;
  createNewWorkspace: boolean;
  targetWorkspaceId?: string;
}
```

- [ ] **Step 2: Create `src/features/import/useImport.ts`**

```typescript
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { ImportReport, ImportCollectionOptions, ImportWorkspaceOptions } from './types';

export function useImport() {
  async function pickDirectory(): Promise<string | null> {
    const selected = await open({ directory: true, multiple: false });
    if (!selected || Array.isArray(selected)) return null;
    return selected;
  }

  async function importCollection(opts: ImportCollectionOptions): Promise<ImportReport> {
    return invoke<ImportReport>('import_bruno_collection', {
      path: opts.path,
      targetWorkspaceId: opts.targetWorkspaceId,
    });
  }

  async function importWorkspace(opts: ImportWorkspaceOptions): Promise<ImportReport> {
    return invoke<ImportReport>('import_bruno_workspace', {
      path: opts.path,
      createNewWorkspace: opts.createNewWorkspace,
      targetWorkspaceId: opts.targetWorkspaceId ?? null,
    });
  }

  return { pickDirectory, importCollection, importWorkspace };
}
```

- [ ] **Step 3: Verify TypeScript types compile**

```bash
yarn tsc --noEmit
```
Expected: no type errors in new files.

- [ ] **Step 4: Commit**

```bash
git add src/features/import/
git commit -m "feat(import): Tauri IPC types and useImport hook"
```

---

## Task 2: `ImportBrunoDialog` component

**Files:**
- Create: `src/features/import/ImportBrunoDialog.tsx`

The dialog has three internal states:

| State | Content |
|---|---|
| `picking` | Radio group (workspace choice) + directory path display + Import button |
| `importing` | Spinner + "Importing…" label. Non-dismissable. |
| `complete` | Summary: imported count, skipped list (collapsible), action buttons |

**shadcn/ui components needed** (add via `yarn dlx shadcn@latest add` if not already present):
- `dialog`
- `radio-group`
- `button`
- `collapsible`
- `badge`

- [ ] **Step 1: Scaffold the component**

```tsx
import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { Label } from '@/components/ui/label';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { Badge } from '@/components/ui/badge';
import { Loader2, FolderOpen, ChevronDown, CheckCircle2, AlertTriangle } from 'lucide-react';
import { useImport } from './useImport';
import { ImportReport, SkippedItem } from './types';

type DialogState = 'picking' | 'importing' | 'complete';
type WorkspaceMode = 'current' | 'new';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 'collection' = import a single Bruno collection.
   *  'workspace'  = import a Bruno workspace (may contain multiple collections). */
  mode: 'collection' | 'workspace';
  /** The active workspace ID — used when mode === 'collection' or workspaceMode === 'current'. */
  activeWorkspaceId: string;
}

export function ImportBrunoDialog({ open, onOpenChange, mode, activeWorkspaceId }: Props) {
  const [state, setState] = useState<DialogState>('picking');
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [workspaceMode, setWorkspaceMode] = useState<WorkspaceMode>('current');
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [skippedOpen, setSkippedOpen] = useState(false);

  const { pickDirectory, importCollection, importWorkspace } = useImport();

  function handleClose() {
    if (state === 'importing') return; // Non-dismissable while importing.
    onOpenChange(false);
    // Reset state after close animation.
    setTimeout(() => {
      setState('picking');
      setSelectedPath(null);
      setWorkspaceMode('current');
      setReport(null);
      setError(null);
      setSkippedOpen(false);
    }, 200);
  }

  async function handlePickDirectory() {
    const path = await pickDirectory();
    if (path) setSelectedPath(path);
  }

  async function handleImport() {
    if (!selectedPath) return;
    setState('importing');
    setError(null);

    try {
      let result: ImportReport;
      if (mode === 'collection') {
        result = await importCollection({
          path: selectedPath,
          targetWorkspaceId: activeWorkspaceId,
        });
      } else {
        result = await importWorkspace({
          path: selectedPath,
          createNewWorkspace: workspaceMode === 'new',
          targetWorkspaceId: workspaceMode === 'current' ? activeWorkspaceId : undefined,
        });
      }
      setReport(result);
      setState('complete');
    } catch (e) {
      setError(String(e));
      setState('picking');
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {mode === 'collection' ? 'Import Bruno Collection' : 'Import Bruno Workspace'}
          </DialogTitle>
        </DialogHeader>

        {state === 'picking' && (
          <PickingState
            mode={mode}
            selectedPath={selectedPath}
            workspaceMode={workspaceMode}
            error={error}
            onPickDirectory={handlePickDirectory}
            onWorkspaceModeChange={setWorkspaceMode}
            onImport={handleImport}
            onCancel={handleClose}
          />
        )}

        {state === 'importing' && <ImportingState />}

        {state === 'complete' && report && (
          <CompleteState
            report={report}
            skippedOpen={skippedOpen}
            onSkippedOpenChange={setSkippedOpen}
            onClose={handleClose}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Implement sub-components**

```tsx
// ─── PickingState ─────────────────────────────────────────────────────────────

interface PickingStateProps {
  mode: 'collection' | 'workspace';
  selectedPath: string | null;
  workspaceMode: WorkspaceMode;
  error: string | null;
  onPickDirectory: () => void;
  onWorkspaceModeChange: (mode: WorkspaceMode) => void;
  onImport: () => void;
  onCancel: () => void;
}

function PickingState({
  mode,
  selectedPath,
  workspaceMode,
  error,
  onPickDirectory,
  onWorkspaceModeChange,
  onImport,
  onCancel,
}: PickingStateProps) {
  return (
    <div className="space-y-4">
      {/* Directory picker */}
      <div className="space-y-2">
        <Label>Bruno directory</Label>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={onPickDirectory}>
            <FolderOpen className="mr-2 h-4 w-4" />
            Browse…
          </Button>
          {selectedPath && (
            <span className="text-sm text-muted-foreground truncate max-w-[240px]">
              {selectedPath}
            </span>
          )}
        </div>
      </div>

      {/* Workspace mode — only shown for workspace imports */}
      {mode === 'workspace' && (
        <div className="space-y-2">
          <Label>Import into</Label>
          <RadioGroup
            value={workspaceMode}
            onValueChange={(v) => onWorkspaceModeChange(v as WorkspaceMode)}
            className="space-y-1"
          >
            <div className="flex items-center space-x-2">
              <RadioGroupItem value="current" id="ws-current" />
              <Label htmlFor="ws-current">Current workspace</Label>
            </div>
            <div className="flex items-center space-x-2">
              <RadioGroupItem value="new" id="ws-new" />
              <Label htmlFor="ws-new">Create new workspace</Label>
            </div>
          </RadioGroup>
        </div>
      )}

      {error && (
        <p className="text-sm text-destructive">{error}</p>
      )}

      <DialogFooter>
        <Button variant="outline" onClick={onCancel}>Cancel</Button>
        <Button onClick={onImport} disabled={!selectedPath}>Import</Button>
      </DialogFooter>
    </div>
  );
}

// ─── ImportingState ────────────────────────────────────────────────────────────

function ImportingState() {
  return (
    <div className="flex flex-col items-center justify-center py-8 gap-3">
      <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      <p className="text-sm text-muted-foreground">Importing…</p>
    </div>
  );
}

// ─── CompleteState ─────────────────────────────────────────────────────────────

interface CompleteStateProps {
  report: ImportReport;
  skippedOpen: boolean;
  onSkippedOpenChange: (open: boolean) => void;
  onClose: () => void;
}

function CompleteState({ report, skippedOpen, onSkippedOpenChange, onClose }: CompleteStateProps) {
  return (
    <div className="space-y-4">
      {/* Success summary */}
      <div className="flex items-start gap-3">
        <CheckCircle2 className="h-5 w-5 text-green-500 mt-0.5 shrink-0" />
        <div>
          <p className="text-sm font-medium">
            {report.imported} {report.imported === 1 ? 'request' : 'requests'} imported
            {report.createdCollections.length > 1
              ? ` across ${report.createdCollections.length} collections`
              : ''}
          </p>
          {report.createdWorkspace && (
            <p className="text-xs text-muted-foreground mt-0.5">
              New workspace: {report.createdWorkspace}
            </p>
          )}
        </div>
      </div>

      {/* Skipped items */}
      {report.skipped.length > 0 && (
        <Collapsible open={skippedOpen} onOpenChange={onSkippedOpenChange}>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" size="sm" className="flex items-center gap-2 px-0 h-auto">
              <AlertTriangle className="h-4 w-4 text-yellow-500" />
              <span className="text-sm text-muted-foreground">
                {report.skipped.length} {report.skipped.length === 1 ? 'item' : 'items'} skipped
              </span>
              <ChevronDown
                className={`h-4 w-4 text-muted-foreground transition-transform ${
                  skippedOpen ? 'rotate-180' : ''
                }`}
              />
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="mt-2 space-y-1 max-h-40 overflow-y-auto rounded border p-2">
              {report.skipped.map((item, i) => (
                <SkippedItemRow key={i} item={item} />
              ))}
            </div>
          </CollapsibleContent>
        </Collapsible>
      )}

      <DialogFooter>
        <Button onClick={onClose}>Close</Button>
      </DialogFooter>
    </div>
  );
}

function SkippedItemRow({ item }: { item: SkippedItem }) {
  const label =
    item.reason.type === 'unsupportedRequestType' ? `Unsupported type: ${item.reason.detail}`
    : item.reason.type === 'unsupportedAuthType' ? `Unsupported auth: ${item.reason.detail}`
    : `Parse error: ${item.reason.detail}`;

  return (
    <div className="flex items-start gap-2 text-xs">
      <Badge variant="outline" className="shrink-0 mt-0.5">{item.path}</Badge>
      <span className="text-muted-foreground">{label}</span>
    </div>
  );
}
```

- [ ] **Step 3: Verify TypeScript**

```bash
yarn tsc --noEmit
```
Expected: no type errors.

- [ ] **Step 4: Commit**

```bash
git add src/features/import/ImportBrunoDialog.tsx
git commit -m "feat(import): ImportBrunoDialog — picking/importing/complete state machine"
```

---

## Task 3: Wire up three entry points

**Files:**
- Modify: file menu component (locate in `src/` — search for existing File menu)
- Modify: workspace overview component (locate in `src/` — search for WorkspaceOverview)
- Modify: collections toolbar component (locate in `src/` — search for CollectionsToolbar or similar)

- [ ] **Step 1: Add import trigger to the File menu**

Find the File menu component. Add an "Import" submenu item with two options:

```tsx
import { ImportBrunoDialog } from '@/features/import/ImportBrunoDialog';

// Inside the File menu component:
const [importDialogOpen, setImportDialogOpen] = useState(false);
const [importMode, setImportMode] = useState<'collection' | 'workspace'>('collection');

function openImportCollection() {
  setImportMode('collection');
  setImportDialogOpen(true);
}

function openImportWorkspace() {
  setImportMode('workspace');
  setImportDialogOpen(true);
}

// In the JSX, inside the DropdownMenu:
<DropdownMenuSub>
  <DropdownMenuSubTrigger>
    <Download className="mr-2 h-4 w-4" />
    Import
  </DropdownMenuSubTrigger>
  <DropdownMenuSubContent>
    <DropdownMenuItem onClick={openImportCollection}>
      Bruno Collection…
    </DropdownMenuItem>
    <DropdownMenuItem onClick={openImportWorkspace}>
      Bruno Workspace…
    </DropdownMenuItem>
  </DropdownMenuSubContent>
</DropdownMenuSub>

// Outside the DropdownMenu (sibling):
<ImportBrunoDialog
  open={importDialogOpen}
  onOpenChange={setImportDialogOpen}
  mode={importMode}
  activeWorkspaceId={activeWorkspaceId}
/>
```

Use `Download` from `lucide-react`. No inline SVGs.

- [ ] **Step 2: Add import button to Workspace Overview**

Find the workspace overview component. Add an "Import from Bruno" `Button` with `variant="outline"`:

```tsx
import { Download } from 'lucide-react';
import { ImportBrunoDialog } from '@/features/import/ImportBrunoDialog';

// In component state:
const [importOpen, setImportOpen] = useState(false);
const [importMode, setImportMode] = useState<'collection' | 'workspace'>('collection');

// In JSX (alongside existing "New Collection" button or similar):
<Button variant="outline" size="sm" onClick={() => { setImportMode('workspace'); setImportOpen(true); }}>
  <Download className="mr-2 h-4 w-4" />
  Import from Bruno
</Button>

<ImportBrunoDialog
  open={importOpen}
  onOpenChange={setImportOpen}
  mode={importMode}
  activeWorkspaceId={activeWorkspaceId}
/>
```

- [ ] **Step 3: Add import icon to Collections toolbar**

Find the Collections panel toolbar component. Add an import icon `Button` with `variant="ghost"` and `size="icon"` alongside the existing New Collection button:

```tsx
import { Download } from 'lucide-react';
import { ImportBrunoDialog } from '@/features/import/ImportBrunoDialog';

// In component state:
const [importOpen, setImportOpen] = useState(false);

// In JSX (alongside New Collection button):
<Button
  variant="ghost"
  size="icon"
  title="Import Bruno collection"
  onClick={() => setImportOpen(true)}
>
  <Download className="h-4 w-4" />
</Button>

<ImportBrunoDialog
  open={importOpen}
  onOpenChange={setImportOpen}
  mode="collection"
  activeWorkspaceId={activeWorkspaceId}
/>
```

- [ ] **Step 4: Verify TypeScript**

```bash
yarn tsc --noEmit
```
Expected: no type errors.

- [ ] **Step 5: Run full build**

```bash
yarn build
```
Expected: builds cleanly with no errors.

- [ ] **Step 6: Commit**

```bash
git add src/
git commit -m "feat(import): wire ImportBrunoDialog into File menu, Workspace Overview, and Collections toolbar"
```
