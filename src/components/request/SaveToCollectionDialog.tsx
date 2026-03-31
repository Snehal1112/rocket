import { useState, useEffect } from 'react';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from '@/components/ui/select';
import { listCollections, saveRequest, createCollection } from '@/lib/tauri-api';
import { sanitizeFilename } from '@/lib/filename-utils';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';
import type { CollectionSummary } from '@/lib/tauri-api';

const NEW_COLLECTION = '__new__';

export interface SaveToCollectionDialogProps {
  open: boolean;
  tab: RequestTab;
  onClose: () => void;
}

export function SaveToCollectionDialog({ open, tab, onClose }: SaveToCollectionDialogProps) {
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [newCollectionName, setNewCollectionName] = useState('');
  const [requestName, setRequestName] = useState(tab.title === 'Untitled' ? '' : tab.title);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    if (!open) return;
    listCollections()
      .then((cols) => {
        setCollections(cols);
        setSelectedCollection(cols.length > 0 ? cols[0].name : NEW_COLLECTION);
      })
      .catch(console.error);
  }, [open]);

  const isCreatingNew = selectedCollection === NEW_COLLECTION || collections.length === 0;
  const trimmedName = requestName.trim();
  const fsName = trimmedName ? sanitizeFilename(trimmedName) : '';
  const showFsHint = fsName !== '' && fsName !== `${trimmedName}.yml`;

  async function handleSave() {
    if (!trimmedName) { setError('Request name is required.'); return; }
    if (isCreatingNew && !newCollectionName.trim()) {
      setError('Collection name is required.'); return;
    }
    setSaving(true); setError('');
    try {
      let collectionName = selectedCollection;
      if (isCreatingNew) {
        await createCollection(newCollectionName.trim());
        collectionName = newCollectionName.trim();
      }

      const payload = {
        uid: tab.id,
        name: trimmedName,
        method: tab.request.method,
        url: tab.request.url,
        headers: tab.request.headers
          .filter((h) => h.key)
          .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
        body: tab.request.body.mode !== 'none'
          ? { mode: tab.request.body.mode, content: tab.request.body.content }
          : undefined,
        auth: { authType: 'none' as const },
        fileName: fsName,
      };

      const saved = await saveRequest(collectionName, fsName, payload);

      const store = usePaneStore.getState();
      store.updateTabSource(tab.id, {
        collection: collectionName,
        path: saved.fileName ?? fsName,
      });
      store.updateTabTitle(tab.id, trimmedName);
      store.markClean(tab.id);
      onClose();
    } catch (err) {
      console.error('[SaveToCollectionDialog]', err);
      setError('Failed to save. Please try again.');
    } finally { setSaving(false); }
  }

  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) onClose(); }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader><DialogTitle>Save Request</DialogTitle></DialogHeader>
        <div className="flex flex-col gap-4 py-2">
          <div className="flex flex-col gap-1.5">
            <Label className="text-xs font-medium">Request Name</Label>
            <Input autoFocus placeholder="My Request"
              value={requestName} onChange={(e) => { setRequestName(e.target.value); setError(''); }}
              className="h-9" />
            {showFsHint && (
              <p className="text-xs text-muted-foreground">
                Saved as: <span className="font-mono">{fsName}</span>
              </p>
            )}
          </div>
          <div className="flex flex-col gap-1.5">
            <Label className="text-xs font-medium">Save to Collection</Label>
            {collections.length > 0 ? (
              <Select value={selectedCollection} onValueChange={setSelectedCollection}>
                <SelectTrigger className="h-9">
                  <SelectValue placeholder="Select collection" />
                </SelectTrigger>
                <SelectContent>
                  {collections.map((c) => (
                    <SelectItem key={c.name} value={c.name}>{c.name}</SelectItem>
                  ))}
                  <SelectItem value={NEW_COLLECTION}>+ New Collection</SelectItem>
                </SelectContent>
              </Select>
            ) : (
              <p className="text-xs text-muted-foreground">
                No collections found — a new one will be created.
              </p>
            )}
          </div>
          {isCreatingNew && (
            <div className="flex flex-col gap-1.5">
              <Label className="text-xs font-medium">New Collection Name</Label>
              <Input placeholder="My Collection"
                value={newCollectionName}
                onChange={(e) => { setNewCollectionName(e.target.value); setError(''); }}
                className="h-9" />
            </div>
          )}
          {error && <p className="text-xs text-destructive">{error}</p>}
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose} disabled={saving}>Cancel</Button>
          <Button size="sm" onClick={() => void handleSave()} disabled={saving || !trimmedName}>
            {saving ? 'Saving...' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
