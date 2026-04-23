import { Box, ExternalLink, FolderOpen, MoreHorizontal, Plus, Trash2, Upload } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
import { RocketBook } from '@/components/illustrations';
import { ImportCollectionDialog } from '@/components/import/ImportCollectionDialog';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useSaveButton } from '@/hooks/use-save-button';
import {
  type CollectionSummary,
  createCollection,
  deleteCollection,
  linkExternalCollection,
  listCollections,
  onCollectionChanged,
  openFolderPicker,
} from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { CollectionTab } from '@/types/pane-types';

interface WorkspaceOverviewTabProps {
  workspaceId: string;
}

export function WorkspaceOverviewTab({ workspaceId }: WorkspaceOverviewTabProps) {
  const workspace = useWorkspaceStore((s) => s.workspaces.find((w) => w.id === workspaceId));
  const updateDescription = useWorkspaceStore((s) => s.updateDescription);
  const openTab = usePaneStore((s) => s.openTab);
  const globalEnvironments = useEnvStore((s) => s.globalEnvironments);
  const loadGlobalEnvironments = useEnvStore((s) => s.loadGlobalEnvironments);

  const [summaries, setSummaries] = useState<CollectionSummary[]>([]);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState('');
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  const [docContent, setDocContent] = useState<string>(workspace?.description ?? '');

  const refresh = useCallback(async () => {
    const cols = await listCollections();
    setSummaries(cols);
  }, []);

  // Both listCollections and loadGlobalEnvironments return data for the
  // active backend workspace. Refetch when workspaceId changes.
  // Also re-fetch when any collection-changed event fires (e.g. sidebar creates/deletes).
  // biome-ignore lint/correctness/useExhaustiveDependencies: workspaceId triggers backend context change
  useEffect(() => {
    refresh().catch(console.error);
    loadGlobalEnvironments().catch(console.error);

    let cancelled = false;
    const unlistenPromise = onCollectionChanged(() => {
      if (!cancelled) refresh().catch(console.error);
    });

    return () => {
      cancelled = true;
      unlistenPromise.then((fn) => fn());
    };
  }, [workspaceId, refresh, loadGlobalEnvironments]);

  useEffect(() => {
    setDocContent(workspace?.description ?? '');
  }, [workspace?.description]);

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

  const saveDocFn = useCallback(async () => {
    await updateDescription(workspaceId, docContent.trim() || null);
  }, [workspaceId, docContent, updateDescription]);

  const { state: saveDocState, trigger: triggerSaveDoc } = useSaveButton(
    saveDocFn,
    'Failed to save documentation',
  );

  // Only enable the save button when the doc has changed from the persisted value.
  const isDocDirty = docContent !== (workspace?.description ?? '');

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

  return (
    <div className='flex h-full overflow-hidden'>
      {/* ── LEFT COLUMN ── */}
      <div className='flex-1 border-r border-border overflow-hidden flex flex-col'>
        <ScrollArea className='flex-1'>
          <div className='p-5 flex flex-col gap-5'>
            {/* Page header */}
            <h2 className='text-base font-semibold leading-tight'>
              {workspace?.name ?? 'Workspace'}
            </h2>

            {/* Stats */}
            <div className='flex gap-7 pb-4 border-b border-border'>
              <div className='flex flex-col gap-0.5'>
                <span className='text-[22px] font-bold leading-tight tabular-nums'>
                  {collectionCount}
                </span>
                <span className='text-[11px] text-muted-foreground'>Collections</span>
              </div>
              <div className='flex flex-col gap-0.5'>
                <span className='text-[22px] font-bold leading-tight tabular-nums'>
                  {globalEnvironments.length}
                </span>
                <span className='text-[11px] text-muted-foreground'>Environments</span>
              </div>
            </div>

            {/* Quick Actions */}
            <div>
              <p className='text-[11px] font-medium text-muted-foreground mb-2'>Quick Actions</p>
              {isCreating ? (
                <div className='flex gap-2'>
                  <input
                    autoFocus
                    placeholder='Collection name'
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') void handleCreateCollection();
                      if (e.key === 'Escape') {
                        setIsCreating(false);
                        setNewName('');
                      }
                    }}
                    className='flex-1 rounded-md border border-input bg-transparent px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring'
                  />
                  <Button
                    size='sm'
                    onClick={() => void handleCreateCollection()}
                    disabled={!newName.trim()}
                  >
                    Create
                  </Button>
                  <Button
                    size='sm'
                    variant='ghost'
                    onClick={() => {
                      setIsCreating(false);
                      setNewName('');
                    }}
                  >
                    Cancel
                  </Button>
                </div>
              ) : (
                <div className='flex flex-wrap gap-1.5'>
                  {summaries.length > 0 && (
                    <Button
                      variant='outline'
                      size='sm'
                      className='text-xs h-7'
                      onClick={() => setIsCreating(true)}
                    >
                      <Plus className='h-3 w-3 mr-1.5' />
                      Create Collection
                    </Button>
                  )}
                  <Button
                    variant='outline'
                    size='sm'
                    className='text-xs h-7'
                    onClick={handleLinkExternal}
                  >
                    <FolderOpen className='h-3 w-3 mr-1.5' />
                    Open Collection
                  </Button>
                  <Button
                    variant='outline'
                    size='sm'
                    className='text-xs h-7'
                    onClick={() => setImportDialogOpen(true)}
                  >
                    <Upload className='h-3 w-3 mr-1.5' />
                    Import Collection
                  </Button>
                </div>
              )}
            </div>

            {/* Collections */}
            <div>
              <p className='text-[11px] font-medium text-muted-foreground mb-2'>Collections</p>
              {summaries.length > 0 ? (
                summaries.map((col) => (
                  <Card
                    key={col.name}
                    className='mb-2 last:mb-0 bg-card border rounded-md cursor-pointer hover:bg-muted/30 transition-colors'
                  >
                    <CardContent className='p-0'>
                      {/* biome-ignore lint/a11y/useSemanticElements: contains nested interactive elements (dropdown button), cannot be a <button> */}
                      <div
                        key={col.name}
                        role='button'
                        tabIndex={0}
                        onClick={() => handleOpenCollection(col.name)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            handleOpenCollection(col.name);
                          }
                        }}
                        className='group flex items-center gap-2.5 px-3 py-2.5 border-b border-border last:border-b-0 hover:bg-muted/50 cursor-pointer transition-colors'
                      >
                        <Box className='h-5 w-5 ' />
                        <div className='flex-1 min-w-0'>
                          <span className='text-sm font-medium truncate block'>{col.name}</span>
                          {col.path && (
                            <span className='text-[10px] text-muted-foreground truncate block'>
                              {col.path}
                            </span>
                          )}
                        </div>
                        <span className='text-[10px] text-muted-foreground shrink-0'>
                          {col.requestCount} req
                        </span>
                        {col.refType === 'external' && (
                          <span className='shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400'>
                            external
                          </span>
                        )}
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <button
                              type='button'
                              aria-label='Collection options'
                              className='h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:bg-muted text-muted-foreground'
                              onClick={(e) => e.stopPropagation()}
                            >
                              <MoreHorizontal className='h-3 w-3' />
                            </button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent onClick={(e) => e.stopPropagation()}>
                            <DropdownMenuItem onClick={() => handleOpenCollection(col.name)}>
                              <ExternalLink className='h-3.5 w-3.5 mr-2' /> Open
                            </DropdownMenuItem>
                            <DropdownMenuSeparator />
                            <DropdownMenuItem
                              className='text-destructive'
                              onClick={() => handleDeleteCollection(col.name)}
                            >
                              <Trash2 className='h-3.5 w-3.5 mr-2' /> Delete
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </div>
                    </CardContent>
                  </Card>
                ))
              ) : (
                <div className='flex flex-col items-center justify-center py-10 gap-5 rounded-lg border border-dashed border-border/60 bg-muted/10 text-center px-6'>
                  <RocketBook className='w-28 h-28 opacity-90' />
                  <div className='space-y-1.5'>
                    <p className='text-sm font-medium text-foreground'>No collections yet</p>
                    <p className='text-xs text-muted-foreground leading-relaxed'>
                      Create a new collection or open an existing folder to get started.
                    </p>
                  </div>
                  <Button
                    variant='outline'
                    size='sm'
                    className='text-xs h-7'
                    onClick={() => setIsCreating(true)}
                  >
                    <Plus className='h-3 w-3 mr-1.5' />
                    <span className='prose prose-sm dark:prose-invert'>Create Collection</span>
                  </Button>
                </div>
              )}
            </div>
          </div>
        </ScrollArea>
      </div>

      <ImportCollectionDialog
        open={importDialogOpen}
        onOpenChange={setImportDialogOpen}
        workspaceId={workspaceId}
        onImportComplete={() => void refresh()}
      />

      {/* ── RIGHT COLUMN — Documentation ── */}
      <div className='flex-1 p-4 flex flex-col overflow-hidden'>
        <MarkdownEditor
          value={docContent}
          onChange={setDocContent}
          mode={docMode}
          onModeChange={setDocMode}
          onSave={() => void triggerSaveDoc()}
          saveState={saveDocState}
          isDirty={isDocDirty}
          onBlur={() => {
            if (isDocDirty) void triggerSaveDoc();
          }}
        />
      </div>
    </div>
  );
}
