import { useState, useEffect, useCallback } from 'react';
import {
  Plus, FolderOpen, Layers, MoreHorizontal, Trash2, ExternalLink,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem,
  DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  listCollections,
  linkExternalCollection, openFolderPicker, createCollection, deleteCollection,
  type CollectionSummary,
} from '@/lib/tauri-api';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { usePaneStore } from '@/stores/pane-store';
import type { CollectionTab } from '@/types/pane-types';

interface WorkspaceOverviewTabProps {
  workspaceId: string;
}

export function WorkspaceOverviewTab({ workspaceId }: WorkspaceOverviewTabProps) {
  const workspace = useWorkspaceStore((s) => s.workspaces.find((w) => w.id === workspaceId));
  const updateDescription = useWorkspaceStore((s) => s.updateDescription);
  const openTab = usePaneStore((s) => s.openTab);

  const [summaries, setSummaries] = useState<CollectionSummary[]>([]);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState('');

  const refresh = useCallback(async () => {
    const cols = await listCollections();
    setSummaries(cols);
  }, [workspaceId]);

  useEffect(() => {
    refresh().catch(console.error);
  }, [refresh]);

  function handleOpenCollection(collectionName: string) {
    const tab: CollectionTab = {
      id: `collection-${collectionName}`,
      title: collectionName,
      tabType: 'collection',
      collectionName,
      isDirty: false,
    };
    openTab(tab);
  }

  async function handleCreateCollection() {
    const name = newName.trim();
    if (!name) return;
    try {
      await createCollection(name);
      setNewName('');
      setIsCreating(false);
      await refresh();
    } catch (err) {
      console.error('[WorkspaceOverview] create failed:', err);
    }
  }

  async function handleDeleteCollection(name: string) {
    try {
      await deleteCollection(name);
      await refresh();
    } catch (err) {
      console.error('[WorkspaceOverview] delete failed:', err);
    }
  }

  async function handleLinkExternal() {
    try {
      const path = await openFolderPicker();
      if (path) {
        await linkExternalCollection(workspaceId, path);
        await refresh();
      }
    } catch (err) {
      console.error('[WorkspaceOverview] link external failed:', err);
    }
  }

  const collectionCount = summaries.length;
  const totalRequests = summaries.reduce((sum, c) => sum + (c.requestCount ?? 0), 0);

  return (
    <ScrollArea className="flex-1">
      <div className="p-6 max-w-3xl mx-auto space-y-6">

        {/* Header. */}
        <div className="space-y-1">
          <h2 className="text-xl font-semibold leading-tight">
            {workspace?.name ?? 'Workspace'}
          </h2>
        </div>

        {/* Stats row. */}
        <div className="grid grid-cols-3 gap-3">
          <div className="rounded-lg border border-border p-3">
            <p className="text-2xl font-semibold">{collectionCount}</p>
            <p className="text-xs text-muted-foreground">Collections</p>
          </div>
          <div className="rounded-lg border border-border p-3">
            <p className="text-2xl font-semibold">—</p>
            <p className="text-xs text-muted-foreground">Environments</p>
          </div>
          <div className="rounded-lg border border-border p-3">
            <p className="text-2xl font-semibold">{totalRequests}</p>
            <p className="text-xs text-muted-foreground">Requests</p>
          </div>
        </div>

        {/* Quick actions. */}
        <div className="flex gap-2">
          {isCreating ? (
            <div className="flex gap-2 flex-1">
              <input
                autoFocus
                placeholder="Collection name"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleCreateCollection();
                  if (e.key === 'Escape') { setIsCreating(false); setNewName(''); }
                }}
                className="flex-1 rounded-md border border-input bg-transparent px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
              <Button size="sm" onClick={() => void handleCreateCollection()} disabled={!newName.trim()}>
                Create
              </Button>
              <Button size="sm" variant="ghost" onClick={() => { setIsCreating(false); setNewName(''); }}>
                Cancel
              </Button>
            </div>
          ) : (
            <>
              <Button variant="outline" size="sm" onClick={() => setIsCreating(true)}>
                <Plus className="h-3.5 w-3.5 mr-1.5" />
                Create Collection
              </Button>
              <Button variant="outline" size="sm" onClick={handleLinkExternal}>
                <FolderOpen className="h-3.5 w-3.5 mr-1.5" />
                Open Collection
              </Button>
            </>
          )}
        </div>

        {/* Description. */}
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground" htmlFor="ws-description">
            Description
          </label>
          <textarea
            id="ws-description"
            rows={3}
            placeholder="Add a description..."
            defaultValue={workspace?.description ?? ''}
            onBlur={(e) => updateDescription(workspaceId, e.target.value || null)}
            className="w-full resize-none rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          />
        </div>

        {/* Collections list. */}
        <div className="space-y-2">
          <h3 className="text-xs font-medium text-muted-foreground">Collections</h3>

          {summaries.length > 0 ? (
            <div className="space-y-1.5">
              {summaries.map((col) => (
                <div
                  key={col.name}
                  onClick={() => handleOpenCollection(col.name)}
                  className="group flex items-center gap-3 p-3 rounded-lg border border-border hover:bg-muted/50 cursor-pointer transition-colors"
                >
                  <Layers className="h-3.5 w-3.5 shrink-0 text-primary" />
                  <div className="flex-1 min-w-0">
                    <span className="text-sm font-medium truncate block">{col.name}</span>
                    {col.path && (
                      <span className="text-[10px] text-muted-foreground truncate block">{col.path}</span>
                    )}
                  </div>
                  <span className="text-xs text-muted-foreground shrink-0">
                    {col.requestCount} {col.requestCount === 1 ? 'request' : 'requests'}
                  </span>
                  {col.refType === 'external' && (
                    <span className="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">
                      external
                    </span>
                  )}
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <button
                        type="button"
                        className="h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <MoreHorizontal className="h-3 w-3" />
                      </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent onClick={(e) => e.stopPropagation()}>
                      <DropdownMenuItem onClick={() => handleOpenCollection(col.name)}>
                        <ExternalLink className="h-3.5 w-3.5 mr-2" /> Open
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        className="text-destructive"
                        onClick={() => handleDeleteCollection(col.name)}
                      >
                        <Trash2 className="h-3.5 w-3.5 mr-2" /> Delete
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">No collections yet.</p>
          )}
        </div>
      </div>
    </ScrollArea>
  );
}
