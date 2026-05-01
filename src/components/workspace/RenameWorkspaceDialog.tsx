import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { useRenameWorkspace } from '@/lib/queries/workspace-queries';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspaceId: string;
  currentName: string;
}

export function RenameWorkspaceDialog({ open, onOpenChange, workspaceId, currentName }: Props) {
  const [name, setName] = useState(currentName);
  const [error, setError] = useState('');
  const renameMutation = useRenameWorkspace();

  // Sync when currentName changes (e.g. re-opened for a different workspace).
  // biome-ignore lint/correctness/useExhaustiveDependencies: open is intentionally included as a reset trigger
  useEffect(() => {
    setName(currentName);
    setError('');
  }, [currentName, open]);

  const handleClose = () => {
    setError('');
    onOpenChange(false);
  };

  const handleRename = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError('Name is required');
      return;
    }
    if (trimmed === currentName) {
      handleClose();
      return;
    }
    try {
      await renameMutation.mutateAsync({ id: workspaceId, newName: trimmed });
      handleClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to rename workspace');
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
          <DialogTitle>Rename workspace</DialogTitle>
        </DialogHeader>

        <div className='py-2'>
          <Input
            value={name}
            onChange={(e) => {
              setName(e.target.value);
              setError('');
            }}
            autoFocus
            onKeyDown={(e) => {
              if (e.key === 'Enter') void handleRename();
              if (e.key === 'Escape') handleClose();
            }}
          />
          {error && <p className='text-xs text-destructive mt-1.5'>{error}</p>}
        </div>

        <DialogFooter>
          <Button variant='ghost' onClick={handleClose}>
            Cancel
          </Button>
          <Button onClick={() => void handleRename()} disabled={!name.trim()}>
            Rename
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
