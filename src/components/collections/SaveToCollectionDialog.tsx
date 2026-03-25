import { useState, useEffect, useCallback } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import {
  listCollections,
  saveRequest as saveReq,
  type CollectionSummary,
  type Request,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';

interface SaveToCollectionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tab: RequestTab;
  /** Build the save payload from the tab. Provided by SaveRequestButton. */
  buildPayload: (name: string) => Request;
  /** Pre-select this collection when the dialog opens. */
  defaultCollection?: string;
}

export function SaveToCollectionDialog({
  open,
  onOpenChange,
  tab,
  buildPayload,
  defaultCollection,
}: SaveToCollectionDialogProps) {
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [requestName, setRequestName] = useState('');
  const [saving, setSaving] = useState(false);

  // Reset state when dialog opens.
  useEffect(() => {
    if (open) {
      void listCollections().then(setCollections);
      setRequestName(tab.title || 'New Request');
      setSelectedCollection(defaultCollection ?? '');
      setSaving(false);
    }
  }, [open, tab.title, defaultCollection]);

  const handleSave = useCallback(async () => {
    const name = requestName.trim();
    if (!selectedCollection || !name) return;
    setSaving(true);
    try {
      // Build payload with empty UID for new requests (backend generates unique name).
      const payload = buildPayload(name);
      payload.uid = '';
      payload.name = name;

      const saved = await saveReq(selectedCollection, name, payload);

      // Transition the tab from draft to collection-owned.
      usePaneStore.setState((state) => {
        const updateNode = (node: any): any => {
          if (node.type === 'leaf') {
            const idx = node.tabs.findIndex((t: any) => t.id === tab.id);
            if (idx === -1) return node;
            const tabs = [...node.tabs];
            tabs[idx] = {
              ...tabs[idx],
              id: saved.uid,
              tabType: 'request',
              title: name,
              isDirty: false,
              defaultCollection: undefined,
              source: {
                collection: selectedCollection,
                path: saved.fileName ?? `${name}.json`,
              },
            };
            return {
              ...node,
              tabs,
              activeTabId: node.activeTabId === tab.id ? saved.uid : node.activeTabId,
            };
          }
          return {
            ...node,
            children: [updateNode(node.children[0]), updateNode(node.children[1])],
          };
        };
        return { root: updateNode(state.root) };
      });

      onOpenChange(false);
    } catch (err) {
      console.error('[SaveToCollection] Failed:', err);
      setSaving(false);
    }
  }, [selectedCollection, requestName, tab.id, buildPayload, onOpenChange]);

  const canSave = !!selectedCollection && !!requestName.trim() && !saving;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Save Request</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          {/* Request name input. */}
          <div>
            <label className="text-xs font-medium text-muted-foreground mb-1 block">
              Request Name
            </label>
            <Input
              className="text-xs h-8"
              value={requestName}
              onChange={(e) => setRequestName(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter' && canSave) void handleSave(); }}
              autoFocus
            />
          </div>

          {/* Collection selector. */}
          <div>
            <label className="text-xs font-medium text-muted-foreground mb-1 block">
              Collection
            </label>
            <ScrollArea className="h-[150px] border border-border rounded-md">
              <div className="p-1">
                {collections.map((c) => (
                  <button
                    key={c.name}
                    type="button"
                    onClick={() => setSelectedCollection(c.name)}
                    className={cn(
                      'w-full text-left px-2 py-1.5 text-xs rounded-sm',
                      selectedCollection === c.name
                        ? 'bg-accent text-accent-foreground'
                        : 'hover:bg-muted/60',
                    )}
                  >
                    {c.name}
                  </button>
                ))}
                {collections.length === 0 && (
                  <p className="text-xs text-muted-foreground text-center py-4">
                    No collections. Create one first.
                  </p>
                )}
              </div>
            </ScrollArea>
          </div>
        </div>
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button size="sm" onClick={handleSave} disabled={!canSave}>
            {saving ? 'Saving...' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
