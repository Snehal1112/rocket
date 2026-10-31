import {
  Box,
  ExternalLink,
  FolderOpen,
  Globe,
  MoreHorizontal,
  Plus,
  Trash2,
  Upload,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
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
import { EmptyState } from '@/components/ui/empty-state';
import { Input } from '@/components/ui/input';
import { Skeleton } from '@/components/ui/skeleton';
import { useSaveButton } from '@/hooks/use-save-button';
import { collectionKeys, useCollections } from '@/lib/queries/collection-queries';
import { useGlobalEnvironments } from '@/lib/queries/environment-queries';
import { useUpdateWorkspaceDescription, useWorkspaces } from '@/lib/queries/workspace-queries';
import { getQueryClient } from '@/lib/query-client';
import { formatRelativeTime } from '@/lib/relative-time';
import {
  createCollection,
  deleteCollection,
  linkExternalCollection,
  openFolderPicker,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { CollectionTab } from '@/types/pane-types';
import { WorkspaceCollectionsSkeleton } from './WorkspaceCollectionsSkeleton';

interface WorkspaceOverviewTabProps {
  workspaceId: string;
}

function plural(n: number, singular: string, pluralForm: string): string {
  return `${n} ${n === 1 ? singular : pluralForm}`;
}

/** The backend sends the folder's modified time as Unix seconds in a string. */
function modifiedLabel(modifiedAt: string | undefined): string | null {
  const seconds = Number(modifiedAt);
  if (!modifiedAt || !Number.isFinite(seconds)) return null;
  return `Modified ${formatRelativeTime(new Date(seconds * 1000).toISOString())}`;
}

export function WorkspaceOverviewTab({ workspaceId }: WorkspaceOverviewTabProps) {
  const { data: workspaces = [] } = useWorkspaces();
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const workspace = workspaces.find((w) => w.id === (workspaceId || activeWorkspaceId));
  const updateDescriptionMutation = useUpdateWorkspaceDescription();
  const openTab = usePaneStore((s) => s.openTab);
  const { data: globalEnvironments = [], isPending: environmentsPending } = useGlobalEnvironments();

  // useCollections() reads the same shared query cache CollectionsSidebar
  // reads and invalidates. Freshness on collection-changed / git-changed
  // events is CollectionsSidebar's responsibility (it's always mounted in
  // the app shell); this component does not need its own listener because
  // invalidating the shared key anywhere refetches it for every consumer.
  const {
    data: summaries = [],
    isPending: collectionsPending,
    isError: collectionsFailed,
    refetch: refetchCollections,
  } = useCollections();
  const collectionsRef = useRef<HTMLDivElement>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState('');
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  const [docContent, setDocContent] = useState<string>(workspace?.description ?? '');

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
      await getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
    } catch (err) {
      console.error('[WorkspaceOverview] create failed:', err);
    }
  }

  async function handleDeleteCollection(name: string) {
    try {
      await deleteCollection(name);
      await getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
    } catch (err) {
      console.error('[WorkspaceOverview] delete failed:', err);
    }
  }

  const saveDocFn = useCallback(async () => {
    await updateDescriptionMutation.mutateAsync({
      id: workspaceId,
      description: docContent.trim() || null,
    });
  }, [workspaceId, docContent, updateDescriptionMutation]);

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
        await getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
      }
    } catch (err) {
      console.error('[WorkspaceOverview] link external failed:', err);
    }
  }

  // Switch to the Environments tab of this workspace, the same way the title bar does.
  function showEnvironments() {
    const tabId = `workspace:${workspaceId}:environments`;
    const activate = (node: ReturnType<typeof usePaneStore.getState>['root']): void => {
      if (node.type === 'leaf') {
        if (node.tabs.some((t) => t.id === tabId)) {
          usePaneStore.getState().setActiveTab(tabId, node.groupId);
        }
      } else {
        activate(node.children[0]);
        activate(node.children[1]);
      }
    };
    activate(usePaneStore.getState().root);
  }

  const collectionCount = summaries.length;

  return (
    <div className='flex h-full overflow-hidden bg-elevation-sunken'>
      {/* ── LEFT COLUMN ── */}
      <div className='flex-1 border-r border-border overflow-hidden flex flex-col'>
        <div className='flex-1 overflow-y-auto'>
          <div className='p-6 flex flex-col gap-6'>
            {/* Page header */}
            <h2 className='text-xl font-semibold leading-tight'>
              {workspace?.name ?? 'Workspace'}
            </h2>

            {/* Stats */}
            <div className='flex gap-3 pb-5 border-b border-border'>
              <Button
                variant='ghost'
                className='h-auto flex-col items-start gap-1 px-3 py-2 -ml-3'
                aria-label='Show collections'
                onClick={() =>
                  collectionsRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
                }
              >
                {collectionsPending ? (
                  <Skeleton className='h-9 w-8' />
                ) : (
                  <span className='text-3xl font-bold leading-tight tabular-nums'>
                    {collectionCount}
                  </span>
                )}
                <span className='flex items-center gap-1.5 text-xs font-normal text-muted-foreground'>
                  <Box className='h-3 w-3' aria-hidden='true' />
                  {collectionCount === 1 ? 'Collection' : 'Collections'}
                </span>
              </Button>
              <Button
                variant='ghost'
                className='h-auto flex-col items-start gap-1 px-3 py-2'
                aria-label='Show environments'
                onClick={showEnvironments}
              >
                {environmentsPending ? (
                  <Skeleton className='h-9 w-8' />
                ) : (
                  <span className='text-3xl font-bold leading-tight tabular-nums'>
                    {globalEnvironments.length}
                  </span>
                )}
                <span className='flex items-center gap-1.5 text-xs font-normal text-muted-foreground'>
                  <Globe className='h-3 w-3' aria-hidden='true' />
                  {globalEnvironments.length === 1 ? 'Environment' : 'Environments'}
                </span>
              </Button>
            </div>

            {/* Quick Actions */}
            <div>
              <p className='text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3'>
                Quick Actions
              </p>
              {isCreating ? (
                <div className='flex gap-2'>
                  <Input
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
                    className='flex-1 h-auto bg-transparent py-1.5 text-sm'
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
                <div className='flex flex-wrap gap-2'>
                  {summaries.length > 0 && (
                    <Button size='sm' className='text-xs h-8' onClick={() => setIsCreating(true)}>
                      <Plus className='h-3.5 w-3.5 mr-1.5' />
                      Create Collection
                    </Button>
                  )}
                  <Button
                    variant='ghost'
                    size='sm'
                    className='text-xs h-8 text-muted-foreground hover:text-foreground'
                    onClick={handleLinkExternal}
                  >
                    <FolderOpen className='h-3.5 w-3.5 mr-1.5' />
                    Open Collection
                  </Button>
                  <Button
                    variant='ghost'
                    size='sm'
                    className='text-xs h-8 text-muted-foreground hover:text-foreground'
                    onClick={() => setImportDialogOpen(true)}
                  >
                    <Upload className='h-3.5 w-3.5 mr-1.5' />
                    Import Collection
                  </Button>
                </div>
              )}
            </div>

            {/* Collections */}
            <div ref={collectionsRef} className='scroll-mt-6'>
              <p className='text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3'>
                Collections
              </p>
              {collectionsPending ? (
                <WorkspaceCollectionsSkeleton />
              ) : summaries.length > 0 ? (
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
                        className='group flex items-center gap-3 px-4 py-3 border-b border-border last:border-b-0 hover:bg-muted/50 cursor-pointer transition-colors'
                      >
                        <Box className='h-5 w-5 shrink-0' />
                        <div className='flex-1 min-w-0'>
                          <span className='text-sm font-medium truncate block'>{col.name}</span>
                          <span className='text-xs text-foreground/70 truncate block'>
                            {[
                              plural(col.requestCount, 'request', 'requests'),
                              modifiedLabel(col.modifiedAt),
                            ]
                              .filter(Boolean)
                              .join(' · ')}
                          </span>
                          {col.path && (
                            <span className='text-xs text-muted-foreground truncate block'>
                              {col.path}
                            </span>
                          )}
                        </div>
                        {col.refType === 'external' && (
                          <span className='shrink-0 rounded-full px-2 py-0.5 text-xs font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400'>
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
              ) : collectionsFailed ? (
                <EmptyState
                  size='compact'
                  cause='unavailable'
                  title="Couldn't load collections"
                  reason='Check the Console for details, then try again.'
                  action={{ label: 'Try again', onClick: () => void refetchCollections() }}
                />
              ) : (
                <div className='flex flex-col items-center justify-center py-12 gap-6 rounded-lg border border-dashed border-border/60 bg-muted/10 text-center px-8'>
                  <RocketBook className='w-32 h-32 opacity-90' />
                  <div className='space-y-2'>
                    <p className='text-base font-semibold text-foreground'>No collections yet</p>
                    <p className='text-sm text-muted-foreground leading-relaxed'>
                      Create a new collection or open an existing folder to get started.
                    </p>
                  </div>
                  <Button
                    variant='outline'
                    size='sm'
                    className='text-xs h-8'
                    onClick={() => setIsCreating(true)}
                  >
                    <Plus className='h-3.5 w-3.5 mr-1.5' />
                    <span className='prose prose-sm dark:prose-invert'>Create Collection</span>
                  </Button>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      <ImportCollectionDialog
        open={importDialogOpen}
        onOpenChange={setImportDialogOpen}
        workspaceId={workspaceId}
        onImportComplete={() =>
          void getQueryClient().invalidateQueries({ queryKey: collectionKeys.all })
        }
      />

      {/* ── RIGHT COLUMN — Documentation ── */}
      <div className='flex-1 p-4 flex flex-col overflow-hidden min-h-0'>
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
