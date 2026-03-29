import { useState, useEffect } from 'react';
import { Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { getWorkspaceConfig, linkExternalCollection, openFolderPicker, type WorkspaceConfig } from '@/lib/tauri-api';
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

  const [config, setConfig] = useState<WorkspaceConfig | null>(null);

  // Load workspace config on mount.
  useEffect(() => {
    getWorkspaceConfig(workspaceId)
      .then(setConfig)
      .catch((err) => {
        console.error('[WorkspaceOverviewTab] failed to load config', err);
      });
  }, [workspaceId]);

  // Open a collection tab when clicking a collection row.
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

  // Link an external collection by picking a folder from disk.
  async function handleLinkExternal() {
    try {
      const path = await openFolderPicker();
      if (path) {
        await linkExternalCollection(workspaceId, path);
        // Refresh config after linking.
        const updated = await getWorkspaceConfig(workspaceId);
        setConfig(updated);
      }
    } catch (err) {
      console.error('[WorkspaceOverviewTab] link external failed', err);
    }
  }

  return (
    <ScrollArea className="flex-1">
      <div className="p-6 max-w-3xl mx-auto space-y-6">

        {/* Workspace header. */}
        <div className="space-y-1">
          <h2 className="text-xl font-semibold leading-tight">
            {workspace?.name ?? 'Workspace'}
          </h2>
          {workspace?.path && (
            <p className="text-xs text-muted-foreground truncate">{workspace.path}</p>
          )}
        </div>

        {/* Description. */}
        <div className="space-y-1.5">
          <label className="text-sm font-medium text-muted-foreground" htmlFor="ws-description">
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

        {/* Collections section. */}
        <div className="space-y-2">
          <h3 className="text-sm font-medium text-muted-foreground">Collections</h3>

          {config?.collections && config.collections.length > 0 ? (
            <div className="space-y-1.5">
              {config.collections.map((col) => (
                <div
                  key={col.name}
                  onClick={() => handleOpenCollection(col.name)}
                  className="flex items-center gap-3 p-3 rounded-lg border border-border hover:bg-muted/50 cursor-pointer transition-colors"
                >
                  <span className="flex-1 text-sm font-medium truncate">{col.name}</span>
                  {col.type === 'external' ? (
                    <span className="shrink-0 rounded-full px-2 py-0.5 text-xs font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">
                      external
                    </span>
                  ) : (
                    <span className="shrink-0 rounded-full px-2 py-0.5 text-xs font-medium text-muted-foreground bg-muted">
                      embedded
                    </span>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">No collections yet.</p>
          )}

          {/* Link external collection button. */}
          <Button
            variant="outline"
            className="w-full border-dashed"
            onClick={handleLinkExternal}
          >
            <Plus className="mr-2 h-4 w-4" />
            Link external collection
          </Button>
        </div>
      </div>
    </ScrollArea>
  );
}
