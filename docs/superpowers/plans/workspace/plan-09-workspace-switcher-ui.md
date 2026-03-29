# Plan 09 — WorkspaceSwitcher full UI

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the stub `WorkspaceSwitcher` in the title bar with the full implementation — dropdown list, per-workspace context menu, close/delete confirmation dialogs, and "New workspace" action.

**Architecture:** `WorkspaceSwitcher` reads from `useWorkspaceStore`. Each workspace row has a hoverable `⋯` button that opens a nested `DropdownMenu` with Rename / Close / Delete actions. Rename opens `RenameWorkspaceDialog`. Close and Delete open shadcn `AlertDialog` confirmations. "New workspace" at the bottom opens `CreateWorkspaceDialog`.

**Tech Stack:** React, TypeScript, shadcn/ui (DropdownMenu, AlertDialog, Button), lucide-react

**Spec:** `docs/superpowers/specs/2026-03-28-workspace-feature-design.md`

**Previous plan:** `plan-08-rename-workspace-dialog.md`
**Next plan:** `plan-10-e2e-verification.md`

---

### Task 1: Ensure AlertDialog is installed

**Files:**
- Check: `src/components/ui/alert-dialog.tsx`

- [ ] **Step 1: Check if AlertDialog component exists**

```bash
ls src/components/ui/alert-dialog.tsx 2>/dev/null && echo "exists" || echo "missing"
```

It already exists — it's used in `CollectionsSidebar.tsx`. If for any reason it's missing:

```bash
yarn dlx shadcn@latest add alert-dialog --preset b2CkJ2CsV
```

- [ ] **Step 2: No commit needed if already exists**

---

### Task 2: Build WorkspaceSwitcher

**Files:**
- Modify: `src/components/title-bar/WorkspaceSwitcher.tsx`

- [ ] **Step 1: Replace the entire contents of `src/components/title-bar/WorkspaceSwitcher.tsx`**

```tsx
import { useState } from 'react'
import { Check, ChevronDown, MoreHorizontal, Plus } from 'lucide-react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { useWorkspaceStore } from '@/stores/workspace-store'
import { CreateWorkspaceDialog } from '@/components/workspace/CreateWorkspaceDialog'
import { RenameWorkspaceDialog } from '@/components/workspace/RenameWorkspaceDialog'

type DialogTarget = { id: string; name: string }

export function WorkspaceSwitcher() {
  const workspaces = useWorkspaceStore((s) => s.workspaces)
  const activeId = useWorkspaceStore((s) => s.activeWorkspaceId)
  const switchWorkspace = useWorkspaceStore((s) => s.switchWorkspace)
  const closeWorkspace = useWorkspaceStore((s) => s.closeWorkspace)
  const deleteWorkspace = useWorkspaceStore((s) => s.deleteWorkspace)

  const [createOpen, setCreateOpen] = useState(false)
  const [renameTarget, setRenameTarget] = useState<DialogTarget | null>(null)
  const [closeTarget, setCloseTarget] = useState<DialogTarget | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<DialogTarget | null>(null)

  const active = workspaces.find((w) => w.id === activeId)
  const canCloseOrDelete = workspaces.length > 1

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            className="gap-1.5 font-medium max-w-[200px]"
          >
            <span className="truncate">{active?.name ?? 'Select workspace'}</span>
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent align="center" className="min-w-[220px]">
          {workspaces.map((ws) => (
            <div key={ws.id} className="flex items-center group">
              <DropdownMenuItem
                className="flex-1 gap-2"
                onSelect={() => {
                  if (ws.id !== activeId) void switchWorkspace(ws.id)
                }}
              >
                <Check
                  className="h-3.5 w-3.5 shrink-0"
                  style={{ opacity: ws.id === activeId ? 1 : 0 }}
                />
                <span className="flex-1 truncate">{ws.name}</span>
              </DropdownMenuItem>

              {/* Per-workspace context menu */}
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 shrink-0 opacity-0 group-hover:opacity-100 mr-1"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <MoreHorizontal className="h-3.5 w-3.5" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-36">
                  <DropdownMenuItem
                    onSelect={() =>
                      setRenameTarget({ id: ws.id, name: ws.name })
                    }
                  >
                    Rename
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={() =>
                      setCloseTarget({ id: ws.id, name: ws.name })
                    }
                    disabled={!canCloseOrDelete}
                  >
                    Close
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    className="text-destructive focus:text-destructive"
                    onSelect={() =>
                      setDeleteTarget({ id: ws.id, name: ws.name })
                    }
                    disabled={ws.id === 'default' || !canCloseOrDelete}
                  >
                    Delete
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          ))}

          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => setCreateOpen(true)}>
            <Plus className="h-3.5 w-3.5 mr-2" />
            New workspace
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Create dialog */}
      <CreateWorkspaceDialog open={createOpen} onOpenChange={setCreateOpen} />

      {/* Rename dialog */}
      {renameTarget && (
        <RenameWorkspaceDialog
          open={!!renameTarget}
          onOpenChange={(o) => { if (!o) setRenameTarget(null) }}
          workspaceId={renameTarget.id}
          currentName={renameTarget.name}
        />
      )}

      {/* Close confirmation */}
      <AlertDialog
        open={!!closeTarget}
        onOpenChange={(o) => { if (!o) setCloseTarget(null) }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Close workspace</AlertDialogTitle>
            <AlertDialogDescription>
              Remove &quot;{closeTarget?.name}&quot; from Rocket? The files on disk will not be affected.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                void closeWorkspace(closeTarget!.id)
                setCloseTarget(null)
              }}
            >
              Close workspace
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Delete confirmation */}
      <AlertDialog
        open={!!deleteTarget}
        onOpenChange={(o) => { if (!o) setDeleteTarget(null) }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete workspace</AlertDialogTitle>
            <AlertDialogDescription>
              Permanently delete &quot;{deleteTarget?.name}&quot; and all its files from disk? This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
              onClick={() => {
                void deleteWorkspace(deleteTarget!.id)
                setDeleteTarget(null)
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

- [ ] **Step 3: Commit**

```bash
git add src/components/title-bar/WorkspaceSwitcher.tsx
git commit -m "feat(workspace): implement full WorkspaceSwitcher with create/switch/rename/close/delete"
```

---

### Task 3: Verify WorkspaceSwitcher renders correctly

- [ ] **Step 1: Run the app**

```bash
yarn tauri dev
```

- [ ] **Step 2: Manual verification checklist**

- [ ] Title bar shows the active workspace name with a chevron
- [ ] Clicking opens a dropdown listing all workspaces
- [ ] Active workspace has a checkmark
- [ ] Hovering a workspace row shows the `⋯` button
- [ ] `⋯` → Rename opens `RenameWorkspaceDialog` pre-filled with current name
- [ ] `⋯` → Close shows confirmation dialog (disabled when only 1 workspace)
- [ ] `⋯` → Delete is disabled for the default workspace and when only 1 workspace
- [ ] "New workspace" at the bottom opens `CreateWorkspaceDialog`
- [ ] Clicking a workspace (not active) calls `switchWorkspace` — collections sidebar reloads

- [ ] **Step 3: Commit any fixes found during verification**

```bash
git add -A
git commit -m "fix(workspace): WorkspaceSwitcher verification fixes" --allow-empty
```
