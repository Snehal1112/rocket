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
} from '@/lib/tauri-api';
import { toApiAuth } from '@/lib/execute-request';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestState } from '@/types/pane-types';

interface SaveToCollectionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tabId: string;
  title: string;
  request: RequestState;
}

export function SaveToCollectionDialog({
  open,
  onOpenChange,
  tabId,
  title,
  request,
}: SaveToCollectionDialogProps) {
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [requestName, setRequestName] = useState(title || 'New Request');

  useEffect(() => {
    if (open) {
      void listCollections().then(setCollections);
      setRequestName(title || 'New Request');
    }
  }, [open, title]);

  const handleSave = useCallback(async () => {
    if (!selectedCollection || !requestName.trim()) return;
    try {
      await saveReq(selectedCollection, requestName.trim(), {
        name: requestName.trim(),
        method: request.method,
        url: request.url,
        headers: request.headers
          .filter((h) => h.enabled)
          .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
        body: request.body.mode !== 'none'
          ? { mode: request.body.mode, content: request.body.content }
          : undefined,
        auth: toApiAuth(request.auth),
      });
      // Update the tab to be collection-owned after a successful save.
      usePaneStore.setState((state) => {
        const updateTab = (node: any): any => {
          if (node.type === 'leaf') {
            const idx = node.tabs.findIndex((t: any) => t.id === tabId);
            if (idx === -1) return node;
            const tabs = [...node.tabs];
            tabs[idx] = {
              ...tabs[idx],
              tabType: 'request',
              title: requestName.trim(),
              isDirty: false,
              source: { collection: selectedCollection, path: requestName.trim() },
            };
            return { ...node, tabs };
          }
          return {
            ...node,
            children: [updateTab(node.children[0]), updateTab(node.children[1])],
          };
        };
        return { root: updateTab(state.root) };
      });
      onOpenChange(false);
      // Signal sidebar to refresh immediately.
      window.dispatchEvent(new CustomEvent('rocket:collections-changed'));
    } catch (err) {
      console.error('[SaveToCollection] Failed:', err);
    }
  }, [selectedCollection, requestName, request, tabId, onOpenChange]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>Save to Collection</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <div>
            <label className="text-xs font-medium text-muted-foreground mb-1 block">
              Request Name
            </label>
            <Input
              className="text-xs h-8"
              value={requestName}
              onChange={(e) => setRequestName(e.target.value)}
            />
          </div>
          <div>
            {/* v1: flat collection list — folder selection deferred. */}
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
          <Button
            size="sm"
            onClick={handleSave}
            disabled={!selectedCollection || !requestName.trim()}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
