# Plan 07 — CreateWorkspaceDialog

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `CreateWorkspaceDialog` component — a shadcn Dialog with a name input, folder picker button, validation, and error display.

**Architecture:** Self-contained dialog component in `src/components/workspace/`. Calls `openFolderPicker()` from tauri-api, auto-fills name from the folder name if name is empty. Calls `createWorkspace` from the store on submit. Validates name uniqueness client-side before calling Tauri.

**Tech Stack:** React, TypeScript, shadcn/ui (Dialog, Button, Input, Label)

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-06-tauri-api-frontend.md`
**Next plan:** `plan-08-rename-workspace-dialog.md`

---

### Task 1: Ensure shadcn Dialog is installed

**Files:**
- Check: `src/components/ui/dialog.tsx`

- [ ] **Step 1: Check if Dialog component exists**

```bash
ls src/components/ui/dialog.tsx 2>/dev/null && echo "exists" || echo "missing"
```

- [ ] **Step 2: Install if missing**

```bash
yarn dlx shadcn@latest add dialog --preset b2CkJ2CsV
```

- [ ] **Step 3: Verify Label component exists (needed for the form)**

```bash
ls src/components/ui/label.tsx 2>/dev/null && echo "exists" || echo "missing"
```

If missing:
```bash
yarn dlx shadcn@latest add label --preset b2CkJ2CsV
```

- [ ] **Step 4: Commit any newly added components**

```bash
git add src/components/ui/
git commit -m "chore: install shadcn Dialog and Label components" --allow-empty
```

---

### Task 2: Create CreateWorkspaceDialog

**Files:**
- Create: `src/components/workspace/CreateWorkspaceDialog.tsx`

- [ ] **Step 1: Create `src/components/workspace/CreateWorkspaceDialog.tsx`**

```tsx
import { useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { FolderOpen } from 'lucide-react'
import { openFolderPicker } from '@/lib/tauri-api'
import { useWorkspaceStore } from '@/stores/workspace-store'

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function CreateWorkspaceDialog({ open, onOpenChange }: Props) {
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  const [error, setError] = useState('')
  const createWorkspace = useWorkspaceStore((s) => s.createWorkspace)
  const workspaces = useWorkspaceStore((s) => s.workspaces)

  const handleClose = () => {
    setName('')
    setPath('')
    setError('')
    onOpenChange(false)
  }

  const handlePickFolder = async () => {
    const picked = await openFolderPicker()
    if (!picked) return
    setPath(picked)
    // Auto-fill name from the last path segment if name is still empty.
    if (!name.trim()) {
      const folderName = picked.split(/[\\/]/).pop() ?? picked
      setName(folderName)
    }
  }

  const handleCreate = async () => {
    const trimmedName = name.trim()
    if (!trimmedName) {
      setError('Name is required')
      return
    }
    if (!path) {
      setError('Please choose a folder')
      return
    }
    if (
      workspaces.some(
        (w) => w.name.toLowerCase() === trimmedName.toLowerCase(),
      )
    ) {
      setError('A workspace with this name already exists')
      return
    }
    try {
      await createWorkspace(trimmedName, path)
      handleClose()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to create workspace')
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) handleClose() }}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New workspace</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="space-y-1.5">
            <Label htmlFor="ws-name">Name</Label>
            <Input
              id="ws-name"
              value={name}
              onChange={(e) => { setName(e.target.value); setError('') }}
              placeholder="My Workspace"
              autoFocus
              onKeyDown={(e) => { if (e.key === 'Enter') void handleCreate() }}
            />
          </div>

          <div className="space-y-1.5">
            <Label>Folder</Label>
            <div className="flex gap-2">
              <Input
                value={path}
                readOnly
                placeholder="Choose a folder..."
                className="flex-1 text-xs text-muted-foreground cursor-default"
                onClick={() => void handlePickFolder()}
              />
              <Button
                variant="outline"
                size="sm"
                onClick={() => void handlePickFolder()}
              >
                <FolderOpen className="h-4 w-4 mr-1.5" />
                Browse
              </Button>
            </div>
          </div>

          {error && (
            <p className="text-xs text-destructive">{error}</p>
          )}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={handleClose}>
            Cancel
          </Button>
          <Button
            onClick={() => void handleCreate()}
            disabled={!name.trim() || !path}
          >
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

- [ ] **Step 3: Commit**

```bash
git add src/components/workspace/CreateWorkspaceDialog.tsx
git commit -m "feat(workspace): add CreateWorkspaceDialog component"
```

---

### Task 3: Smoke test CreateWorkspaceDialog manually

- [ ] **Step 1: Temporarily wire the dialog in App.tsx for testing**

In `App.tsx`, add a temporary test button:

```tsx
import { useState } from 'react'
import { CreateWorkspaceDialog } from '@/components/workspace/CreateWorkspaceDialog'

// Inside App(), before the return:
const [testDialogOpen, setTestDialogOpen] = useState(false)

// Somewhere visible in the JSX (e.g. next to TitleBar):
<button onClick={() => setTestDialogOpen(true)} style={{position:'fixed',bottom:8,right:8,zIndex:9999}}>
  Test Dialog
</button>
<CreateWorkspaceDialog open={testDialogOpen} onOpenChange={setTestDialogOpen} />
```

- [ ] **Step 2: Run and manually test**

```bash
yarn tauri dev
```

Verify:
- Dialog opens on button click
- "Browse" button opens the OS folder picker
- Folder path appears in the input after picking
- Name auto-fills from folder name if name was empty
- "Create" button is disabled until both name and path are filled
- Duplicate name shows error message
- Successful creation closes the dialog
- New workspace appears in the workspace-store (check React devtools)

- [ ] **Step 3: Remove the test button from App.tsx**

```tsx
// Remove the test button and import
```

- [ ] **Step 4: Commit cleanup**

```bash
git add src/App.tsx
git commit -m "test(workspace): manual smoke test CreateWorkspaceDialog — remove test button"
```
