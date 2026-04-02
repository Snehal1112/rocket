import { useCallback, useEffect, useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { CollectionVariablesEditor } from './CollectionVariablesEditor';
import { getFolderVariables, saveFolderVariables } from '@/lib/tauri-api';
import type { CollectionVariable } from '@/lib/tauri-api';

interface FolderVariablesPopoverProps {
  open: boolean;
  onClose: () => void;
  collection: string;
  /** Folder path relative to the collection root, e.g. "auth" or "auth/oauth". */
  folderPath: string;
  folderName: string;
}

export function FolderVariablesPopover({
  open,
  onClose,
  collection,
  folderPath,
  folderName,
}: FolderVariablesPopoverProps) {
  const [variables, setVariables] = useState<CollectionVariable[]>([]);
  const [saving, setSaving] = useState(false);

  // Load only this folder's own variables when the dialog opens.
  useEffect(() => {
    if (!open) return;
    void getFolderVariables(collection, folderPath)
      .then(setVariables)
      .catch((err) => console.error('[FolderVariablesPopover] load failed:', err));
  }, [open, collection, folderPath]);

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      await saveFolderVariables(collection, folderPath, variables);
      onClose();
    } catch (err) {
      console.error('[FolderVariablesPopover] Failed to save variables:', err);
    } finally {
      setSaving(false);
    }
  }, [collection, folderPath, variables, onClose]);

  return (
    <Dialog open={open} onOpenChange={(isOpen) => { if (!isOpen) onClose(); }}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>{folderName} — Variables</DialogTitle>
          <DialogDescription>
            Variables defined here are available to all requests in this folder.
            Inner folders inherit and can override these variables.
          </DialogDescription>
        </DialogHeader>

        {/* Editable variable table. */}
        <div className="overflow-y-auto max-h-[60vh]">
          <CollectionVariablesEditor
            variables={variables}
            onChange={setVariables}
          />
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={saving}>
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
