import { useEffect, useState } from 'react';
import { toast } from 'sonner';
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
import type { ExampleSnapshot } from '@/lib/example-snapshot';
import { addRequestExample } from '@/lib/tauri-api';

interface SaveExampleDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  collection: string;
  requestPath: string;
  snapshot: ExampleSnapshot;
  defaultName: string;
  existingNames: string[];
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return typeof err === 'string' ? err : 'Could not save the example.';
}

export function SaveExampleDialog({
  open,
  onOpenChange,
  collection,
  requestPath,
  snapshot,
  defaultName,
  existingNames,
}: SaveExampleDialogProps) {
  const [name, setName] = useState(defaultName);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Start from the suggested name every time the dialog opens.
  useEffect(() => {
    if (open) {
      setName(defaultName);
      setError(null);
    }
  }, [open, defaultName]);

  const trimmed = name.trim();
  const duplicate = existingNames.some((n) => n.toLowerCase() === trimmed.toLowerCase());
  const canSave = trimmed !== '' && !duplicate && !saving;

  const handleSave = async () => {
    if (!canSave) return;
    setSaving(true);
    setError(null);
    try {
      await addRequestExample(collection, requestPath, {
        name: trimmed,
        request: snapshot.request,
        response: snapshot.response,
      });
      toast.success('Example saved');
      onOpenChange(false);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='max-w-md'>
        <DialogHeader>
          <DialogTitle>Save as example</DialogTitle>
          <DialogDescription>
            Store this request and response under the saved request.
          </DialogDescription>
        </DialogHeader>

        <div className='space-y-2 py-2'>
          <Label htmlFor='example-name'>Example name</Label>
          <Input
            id='example-name'
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void handleSave();
              }
            }}
          />
          {duplicate && (
            <p className='text-xs text-destructive'>An example with this name already exists.</p>
          )}
          {error && (
            <p role='alert' className='text-xs text-destructive'>
              {error}
            </p>
          )}
          {snapshot.notes.length > 0 && (
            <ul className='space-y-1 text-xs text-muted-foreground'>
              {snapshot.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          )}
        </div>

        <DialogFooter>
          <Button variant='ghost' onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={() => void handleSave()} disabled={!canSave}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
