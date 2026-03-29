# Plan 08 — RenameWorkspaceDialog

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `RenameWorkspaceDialog` component — a small shadcn Dialog with a single pre-filled name input and validation.

**Architecture:** Receives `workspaceId` and `currentName` as props. Pre-fills the input with `currentName`. Calls `renameWorkspace` from the store on confirm. Validates non-empty and catches duplicate errors from Tauri.

**Tech Stack:** React, TypeScript, shadcn/ui (Dialog, Button, Input)

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-07-create-workspace-dialog.md`
**Next plan:** `plan-09-workspace-switcher-ui.md`

---

### Task 1: Create RenameWorkspaceDialog

**Files:**
- Create: `src/components/workspace/RenameWorkspaceDialog.tsx`

- [ ] **Step 1: Create `src/components/workspace/RenameWorkspaceDialog.tsx`**

```tsx
import { useState, useEffect } from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useWorkspaceStore } from '@/stores/workspace-store'

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
  workspaceId: string
  currentName: string
}

export function RenameWorkspaceDialog({
  open,
  onOpenChange,
  workspaceId,
  currentName,
}: Props) {
  const [name, setName] = useState(currentName)
  const [error, setError] = useState('')
  const renameWorkspace = useWorkspaceStore((s) => s.renameWorkspace)

  // Sync when currentName changes (e.g. re-opened for a different workspace).
  useEffect(() => {
    setName(currentName)
    setError('')
  }, [currentName, open])

  const handleClose = () => {
    setError('')
    onOpenChange(false)
  }

  const handleRename = async () => {
    const trimmed = name.trim()
    if (!trimmed) {
      setError('Name is required')
      return
    }
    if (trimmed === currentName) {
      handleClose()
      return
    }
    try {
      await renameWorkspace(workspaceId, trimmed)
      handleClose()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to rename workspace')
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) handleClose() }}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename workspace</DialogTitle>
        </DialogHeader>

        <div className="py-2">
          <Input
            value={name}
            onChange={(e) => { setName(e.target.value); setError('') }}
            autoFocus
            onKeyDown={(e) => {
              if (e.key === 'Enter') void handleRename()
              if (e.key === 'Escape') handleClose()
            }}
          />
          {error && (
            <p className="text-xs text-destructive mt-1.5">{error}</p>
          )}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={handleClose}>
            Cancel
          </Button>
          <Button
            onClick={() => void handleRename()}
            disabled={!name.trim()}
          >
            Rename
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
git add src/components/workspace/RenameWorkspaceDialog.tsx
git commit -m "feat(workspace): add RenameWorkspaceDialog component"
```

---

### Task 2: Create workspace components index

**Files:**
- Create: `src/components/workspace/index.ts`

- [ ] **Step 1: Create barrel export**

```ts
export { CreateWorkspaceDialog } from './CreateWorkspaceDialog'
export { RenameWorkspaceDialog } from './RenameWorkspaceDialog'
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

- [ ] **Step 3: Commit**

```bash
git add src/components/workspace/index.ts
git commit -m "chore(workspace): add workspace components barrel export"
```

---

### Task 3: Smoke test RenameWorkspaceDialog manually

- [ ] **Step 1: Temporarily wire the dialog in App.tsx for testing**

```tsx
import { useState } from 'react'
import { RenameWorkspaceDialog } from '@/components/workspace/RenameWorkspaceDialog'
import { useWorkspaceStore } from '@/stores/workspace-store'

// Inside App():
const [renameOpen, setRenameOpen] = useState(false)
const workspaces = useWorkspaceStore((s) => s.workspaces)
const first = workspaces[0]

// Somewhere visible in JSX:
{first && (
  <>
    <button
      onClick={() => setRenameOpen(true)}
      style={{ position: 'fixed', bottom: 8, right: 8, zIndex: 9999 }}
    >
      Test Rename
    </button>
    <RenameWorkspaceDialog
      open={renameOpen}
      onOpenChange={setRenameOpen}
      workspaceId={first.id}
      currentName={first.name}
    />
  </>
)}
```

- [ ] **Step 2: Run and manually test**

```bash
yarn tauri dev
```

Verify:
- Dialog opens pre-filled with the current workspace name
- Empty name shows validation error
- Pressing Enter submits
- Pressing Escape closes
- Duplicate name shows error from Tauri
- Successful rename closes dialog and workspace name updates in store

- [ ] **Step 3: Remove test button from App.tsx**

- [ ] **Step 4: Commit cleanup**

```bash
git add src/App.tsx
git commit -m "test(workspace): manual smoke test RenameWorkspaceDialog — remove test button"
```
