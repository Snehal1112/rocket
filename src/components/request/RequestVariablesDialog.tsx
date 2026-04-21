import { useCallback, useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { CollectionVariable } from '@/lib/tauri-api';
import { getRequestVariables, saveRequestVariables } from '@/lib/tauri-api';
import { CollectionVariablesEditor } from '@/components/collections/CollectionVariablesEditor';

interface RequestVariablesDialogProps {
  open: boolean;
  onClose: () => void;
  collection: string;
  requestPath: string;
  requestName: string;
  onVarCountChange?: (count: number) => void;
}

export function RequestVariablesDialog({
  open,
  onClose,
  collection,
  requestPath,
  requestName,
  onVarCountChange,
}: RequestVariablesDialogProps) {
  const [variables, setVariables] = useState<CollectionVariable[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    void getRequestVariables(collection, requestPath)
      .then((loaded) => {
        setVariables(loaded);
        onVarCountChange?.(loaded.length);
      })
      .catch((err) => console.error('[RequestVariablesDialog] load failed:', err));
  }, [open, collection, requestPath, onVarCountChange]);

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      await saveRequestVariables(collection, requestPath, variables);
      onVarCountChange?.(variables.length);
      onClose();
    } catch (err) {
      console.error('[RequestVariablesDialog] save failed:', err);
    } finally {
      setSaving(false);
    }
  }, [collection, requestPath, variables, onClose, onVarCountChange]);

  return (
    <Dialog open={open} onOpenChange={(isOpen) => { if (!isOpen) onClose(); }}>
      <DialogContent className='max-w-3xl'>
        <DialogHeader>
          <DialogTitle>{requestName} — Variables</DialogTitle>
          <DialogDescription>
            Request variables are scoped to this request only and take priority over folder,
            environment, and collection variables.
          </DialogDescription>
        </DialogHeader>

        <div className='overflow-y-auto max-h-[60vh]'>
          <CollectionVariablesEditor
            variables={variables}
            onChange={(updated) => {
              setVariables(updated);
              onVarCountChange?.(updated.length);
            }}
            showDescription={false}
          />
        </div>

        <DialogFooter>
          <Button variant='outline' onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button onClick={() => void handleSave()} disabled={saving}>
            {saving ? 'Saving…' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
