import { useState, useEffect, useCallback } from 'react';
import { Folder, Check, FolderPlus } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import {
  listCollections,
  createCollection,
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
  buildPayload: (name: string) => Request;
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
  const [newCollectionMode, setNewCollectionMode] = useState(false);
  const [newCollectionName, setNewCollectionName] = useState('');

  // Reset state when dialog opens.
  useEffect(() => {
    if (open) {
      void listCollections().then(setCollections);
      setRequestName(tab.title || 'New Request');
      setSelectedCollection(defaultCollection ?? '');
      setSaving(false);
      setNewCollectionMode(false);
      setNewCollectionName('');
    }
  }, [open, tab.title, defaultCollection]);

  // Create a new collection inline.
  const handleCreateCollection = useCallback(async () => {
    const name = newCollectionName.trim();
    if (!name) return;
    try {
      await createCollection(name);
      const updated = await listCollections();
      setCollections(updated);
      setSelectedCollection(name);
      setNewCollectionMode(false);
      setNewCollectionName('');
    } catch (err) {
      console.error('[SaveToCollection] Create collection failed:', err);
    }
  }, [newCollectionName]);

  // Save request to selected collection.
  const handleSave = useCallback(async () => {
    const name = requestName.trim();
    if (!selectedCollection || !name) return;
    setSaving(true);
    try {
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
      <DialogContent className="max-w-md p-0 gap-0 overflow-hidden">
        {/* Header. */}
        <DialogHeader className="px-5 pt-5 pb-3">
          <DialogTitle className="text-base">Save Request</DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            Choose a name and collection for this request.
          </DialogDescription>
        </DialogHeader>

        <div className="px-5 pb-4 space-y-4">
          {/* Request name. */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-foreground" htmlFor="save-req-name">
              Request Name
            </label>
            <Input
              id="save-req-name"
              className="h-9 text-sm"
              placeholder="e.g., Get Users, Create Order"
              value={requestName}
              onChange={(e) => setRequestName(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter' && canSave) void handleSave(); }}
              autoFocus
            />
          </div>

          {/* Collection selector. */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label className="text-xs font-medium text-foreground">
                Save to Collection
              </label>
              {!newCollectionMode && (
                <button
                  type="button"
                  onClick={() => setNewCollectionMode(true)}
                  className="text-[11px] text-primary hover:underline font-medium"
                >
                  + New Collection
                </button>
              )}
            </div>

            {/* Inline new collection creation. */}
            {newCollectionMode && (
              <div className="flex gap-2 mb-2">
                <Input
                  className="h-8 text-xs flex-1"
                  placeholder="Collection name"
                  value={newCollectionName}
                  onChange={(e) => setNewCollectionName(e.target.value)}
                  onKeyDown={(e) => { if (e.key === 'Enter') void handleCreateCollection(); }}
                  autoFocus
                />
                <Button size="sm" className="h-8 px-3 text-xs" onClick={handleCreateCollection} disabled={!newCollectionName.trim()}>
                  Create
                </Button>
                <Button size="sm" variant="ghost" className="h-8 px-2 text-xs" onClick={() => setNewCollectionMode(false)}>
                  Cancel
                </Button>
              </div>
            )}

            <ScrollArea className="h-[180px] rounded-lg border border-border bg-muted/20">
              <div className="p-1.5 space-y-0.5">
                {collections.map((c) => (
                  <button
                    key={c.name}
                    type="button"
                    onClick={() => setSelectedCollection(c.name)}
                    className={cn(
                      'w-full flex items-center gap-2.5 px-3 py-2 text-xs rounded-md transition-colors',
                      selectedCollection === c.name
                        ? 'bg-primary/10 text-primary font-medium'
                        : 'text-foreground hover:bg-muted/60',
                    )}
                  >
                    <Folder className={cn(
                      'h-3.5 w-3.5 shrink-0',
                      selectedCollection === c.name ? 'text-primary' : 'text-muted-foreground',
                    )} />
                    <span className="flex-1 text-left truncate">{c.name}</span>
                    {c.requestCount > 0 && (
                      <span className="text-[10px] text-muted-foreground">
                        {c.requestCount}
                      </span>
                    )}
                    {selectedCollection === c.name && (
                      <Check className="h-3.5 w-3.5 text-primary shrink-0" />
                    )}
                  </button>
                ))}
                {collections.length === 0 && !newCollectionMode && (
                  <div className="flex flex-col items-center justify-center py-8 gap-2">
                    <FolderPlus className="h-8 w-8 text-muted-foreground/40" />
                    <p className="text-xs text-muted-foreground">No collections yet.</p>
                    <Button
                      size="sm"
                      variant="outline"
                      className="text-xs h-7"
                      onClick={() => setNewCollectionMode(true)}
                    >
                      Create Collection
                    </Button>
                  </div>
                )}
              </div>
            </ScrollArea>
          </div>
        </div>

        {/* Footer. */}
        <DialogFooter className="px-5 py-3 border-t border-border bg-muted/30">
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
