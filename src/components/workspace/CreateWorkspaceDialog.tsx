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
import { Label } from '@/components/ui/label'
import { FolderOpen } from 'lucide-react'
import { openFolderPicker, getAppDataDir } from '@/lib/tauri-api'
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

  // Pre-fill path with the default data directory when the dialog opens.
  useEffect(() => {
    if (open && !path) {
      getAppDataDir().then(setPath).catch(() => {});
    }
  }, [open]);

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
                className="flex-1 text-sm text-muted-foreground cursor-default"
                onClick={() => void handlePickFolder()}
              />
              <Button
                variant="outline"
                size="sm"
                onClick={() => void handlePickFolder()}
              >
                <FolderOpen className="h-3.5 w-3.5 mr-1.5" />
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
