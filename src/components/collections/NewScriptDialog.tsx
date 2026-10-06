import { useState } from 'react';
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
import { createScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

interface NewScriptDialogProps {
  open: boolean;
  collectionName: string;
  /** Folder path relative to the collection root. Empty for the root. */
  folderPath: string;
  onClose: () => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Prompts for a script name, creates the file and opens it in a tab. */
export function NewScriptDialog({
  open,
  collectionName,
  folderPath,
  onClose,
}: NewScriptDialogProps) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);

  const close = () => {
    setName('');
    setError('');
    onClose();
  };

  const create = async () => {
    const trimmed = name.trim();
    if (!trimmed || busy) return;
    setBusy(true);
    setError('');
    try {
      const path = await createScriptFile(collectionName, folderPath, trimmed);
      await usePaneStore.getState().openScriptTab(collectionName, path);
      close();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New script</DialogTitle>
          <DialogDescription>
            Creates a .js file you can load from any script with require().
          </DialogDescription>
        </DialogHeader>
        <div className='space-y-2'>
          <Label htmlFor='new-script-name'>Script name</Label>
          <Input
            id='new-script-name'
            autoFocus
            value={name}
            placeholder='utils'
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void create();
            }}
          />
          {error && (
            <p role='alert' className='text-xs text-destructive'>
              {error}
            </p>
          )}
        </div>
        <DialogFooter>
          <Button type='button' variant='ghost' onClick={close}>
            Cancel
          </Button>
          <Button type='button' disabled={!name.trim() || busy} onClick={() => void create()}>
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
