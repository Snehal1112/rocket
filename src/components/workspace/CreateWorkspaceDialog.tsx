import { FolderOpen } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useCreateWorkspace, useWorkspaces } from '@/lib/queries/workspace-queries';
import { getAppDataDir, openFolderPicker } from '@/lib/tauri-api';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CreateWorkspaceDialog({ open, onOpenChange }: Props) {
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [error, setError] = useState('');
  const createMutation = useCreateWorkspace();
  const { data: workspaces = [] } = useWorkspaces();

  // Pre-fill path with the default data directory when the dialog opens.
  // biome-ignore lint/correctness/useExhaustiveDependencies: path is an intentional guard condition, not a trigger
  useEffect(() => {
    if (open && !path) {
      getAppDataDir()
        .then(setPath)
        .catch(() => undefined);
    }
  }, [open]);

  const handleClose = () => {
    setName('');
    setPath('');
    setError('');
    onOpenChange(false);
  };

  const handlePickFolder = async () => {
    const picked = await openFolderPicker();
    if (!picked) return;
    setPath(picked);
    // Auto-fill name from the last path segment if name is still empty.
    if (!name.trim()) {
      const folderName = picked.split(/[\\/]/).pop() ?? picked;
      setName(folderName);
    }
  };

  const handleCreate = async () => {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError('Name is required');
      return;
    }
    if (!path) {
      setError('Please choose a folder');
      return;
    }
    if (workspaces.some((w) => w.name.toLowerCase() === trimmedName.toLowerCase())) {
      setError('A workspace with this name already exists');
      return;
    }
    try {
      // Append workspace name to the chosen directory so each workspace
      // lives in its own subfolder and can be deleted safely.
      const sep = path.includes('\\') ? '\\' : '/';
      const fullPath = path.endsWith(sep) ? path + trimmedName : path + sep + trimmedName;
      await createMutation.mutateAsync({ name: trimmedName, path: fullPath });
      handleClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to create workspace');
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) handleClose();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New workspace</DialogTitle>
          <DialogDescription className='sr-only'>
            Set up a new workspace to organise your collections and environments.
          </DialogDescription>
        </DialogHeader>

        <div className='space-y-4 py-2'>
          <div className='space-y-1.5'>
            <Label htmlFor='ws-name'>Name</Label>
            <Input
              id='ws-name'
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                setError('');
              }}
              placeholder='My Workspace'
              autoFocus
              onKeyDown={(e) => {
                if (e.key === 'Enter') void handleCreate();
              }}
            />
          </div>

          <div className='space-y-1.5'>
            <Label>Folder</Label>
            <div className='flex gap-2'>
              <Input
                value={path}
                readOnly
                placeholder='Choose a folder...'
                className='flex-1 text-sm text-muted-foreground cursor-default'
                onClick={() => void handlePickFolder()}
              />
              <Button variant='outline' size='sm' onClick={() => void handlePickFolder()}>
                <FolderOpen className='h-3.5 w-3.5 mr-1.5' />
                Browse
              </Button>
            </div>
          </div>

          {error && <p className='text-xs text-destructive'>{error}</p>}
        </div>

        <DialogFooter>
          <Button variant='ghost' onClick={handleClose}>
            Cancel
          </Button>
          <Button onClick={() => void handleCreate()} disabled={!name.trim() || !path}>
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
