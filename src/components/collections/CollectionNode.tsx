import {
  BoxIcon,
  ChevronDown,
  ChevronRight,
  FolderPlus,
  LayoutGrid,
  Lock,
  MoreHorizontal,
  Pencil,
  Plus,
  Trash2,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ContractBadge } from '@/components/contract/ContractBadge';
import { CreateRequestDialog } from '@/components/request/CreateRequestDialog';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { sortItemsFoldersFirst } from '@/lib/collection-utils';
import { createDefaultRequest } from '@/lib/pane-utils';
import type { Collection, CollectionSummary, Contract } from '@/lib/tauri-api';
import { getCollection, onCollectionChanged, renameCollection, saveRequest } from '@/lib/tauri-api';
import { useContractStore } from '@/stores/contract-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import type { CollectionTab } from '@/types/pane-types';
import { FolderNode } from './FolderNode';
import { RequestNode } from './RequestNode';
import type { DeleteTarget } from './tree-utils';

// Stable empty reference to keep the Zustand selector output identity-stable
// for collections that have no contracts. Without this, the `?? []` fallback
// would produce a fresh array on every store update and force re-renders.
const EMPTY_CONTRACTS: Contract[] = [];

interface CollectionNodeProps {
  summary: CollectionSummary;
  filter: string;
  summaries: CollectionSummary[];
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (
    srcCollection: string,
    srcPath: string,
    dstCollection: string,
    dstPath: string,
  ) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function CollectionNode({
  summary,
  filter,
  summaries,
  onNewFolder,
  onMove,
  onDelete,
  onDuplicate,
}: CollectionNodeProps) {
  const [open, setOpen] = useState(false);
  const [collection, setCollection] = useState<Collection | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(summary.name);
  const clickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const treeDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  const renameInFlight = useRef(false);
  const [creatingRequest, setCreatingRequest] = useState(false);
  const [newRequestName, setNewRequestName] = useState('');
  const [createRequestOpen, setCreateRequestOpen] = useState(false);

  // Derive the filesystem path of this collection so we can talk to the
  // contract IPC commands. The backend stores contract metadata under
  // `<workspace>/collections/<name>/.rocket/contracts/` — we mirror the
  // same construction used by `save_request` on the Rust side.
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);
  const collectionRoot = activeWorkspace
    ? `${activeWorkspace.path}/collections/${summary.name}`
    : '';

  const loadContracts = useContractStore((s) => s.loadContracts);
  const contractsForRoot = useContractStore(
    (s) => s.contractsByRoot[collectionRoot] ?? EMPTY_CONTRACTS,
  );
  const collectionScopedContracts = contractsForRoot.filter((c) => c.scope.type === 'collection');
  const openContractTab = usePaneStore((s) => s.openContractTab);

  // Load contracts for this collection once the workspace path is known.
  // Cheap when the collection has none — backend returns an empty list.
  useEffect(() => {
    if (!collectionRoot) return;
    void loadContracts(collectionRoot);
  }, [collectionRoot, loadContracts]);

  const refreshTree = useCallback(() => {
    getCollection(summary.name)
      .then(setCollection)
      .catch((err) => console.error('[CollectionNode] fetch error', err));
  }, [summary.name]);

  // Fetch when first expanded.
  useEffect(() => {
    if (open && !collection) refreshTree();
  }, [open, collection, refreshTree]);

  // Per-collection change listener, active only when expanded.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onCollectionChanged((event) => {
      // Collection-level lifecycle events (delete/rename) are handled by the
      // sidebar's own fetchCollections listener. Refreshing the tree here
      // would call getCollection with a name that no longer exists on disk.
      if (event.type === 'collectionDeleted' || event.type === 'collectionRenamed') return;

      // event.collection may be a full filesystem path (e.g. from git events).
      // Extract just the last segment so it can be compared against the name.
      const raw = event.collection ?? event.name;
      const parts = raw?.includes('/') ? raw.split('/') : null;
      const affected = parts ? parts[parts.length - 1] : raw;
      if (!affected || affected === summary.name) {
        if (treeDebounce.current) clearTimeout(treeDebounce.current);
        treeDebounce.current = setTimeout(() => refreshTree(), 300);
      }
    }).then((fn) => {
      if (cancelled)
        fn(); // Already unmounted — immediately unsubscribe.
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      if (treeDebounce.current) clearTimeout(treeDebounce.current);
    };
  }, [open, refreshTree, summary.name]);

  // Auto-expand when filter is active.
  useEffect(() => {
    if (filter) setOpen(true);
  }, [filter]);

  // Auto-expand when this collection becomes the active one in the pane store.
  const activeCollection = usePaneStore((s) => s.activeCollection);
  useEffect(() => {
    if (activeCollection === summary.name) setOpen(true);
  }, [activeCollection, summary.name]);

  // Clear pending click timer on unmount.
  useEffect(() => {
    return () => {
      if (clickTimer.current) clearTimeout(clickTimer.current);
    };
  }, []);

  const handleRename = async () => {
    if (renameInFlight.current) return;
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === summary.name) {
      setIsRenaming(false);
      return;
    }
    renameInFlight.current = true;
    try {
      await renameCollection(summary.name, trimmed);
    } catch (err) {
      console.error('Rename collection failed:', err);
    } finally {
      renameInFlight.current = false;
      setIsRenaming(false);
    }
  };

  // Single click toggles expand after 250 ms (to allow double-click to fire first).
  const handleClick = () => {
    if (clickTimer.current) {
      clearTimeout(clickTimer.current);
      clickTimer.current = null;
      return;
    }
    clickTimer.current = setTimeout(() => {
      clickTimer.current = null;
      setOpen((prev) => !prev);
    }, 250);
  };

  // Double click opens the collection Overview tab.
  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (clickTimer.current) {
      clearTimeout(clickTimer.current);
      clickTimer.current = null;
    }
    const tab: CollectionTab = {
      id: summary.uid,
      title: summary.name,
      tabType: 'collection',
      collectionName: summary.name,
      isDirty: false,
    };
    usePaneStore.getState().openTab(tab);
  };

  const handleNewRequestCreate = async () => {
    const name = newRequestName.trim();
    if (!name) {
      setCreatingRequest(false);
      return;
    }
    setCreatingRequest(false);
    try {
      const uid = crypto.randomUUID();
      const payload = {
        uid,
        name,
        method: 'GET' as const,
        url: '',
        headers: [],
        auth: { authType: 'none' as const },
      };
      const saved = await saveRequest(summary.name, name, payload);
      usePaneStore.getState().openTab({
        id: uid,
        title: saved.name,
        tabType: 'request',
        request: createDefaultRequest(),
        response: null,
        isDirty: false,
        source: {
          collection: summary.name,
          path: saved.fileName ?? `${name}.yml`,
        },
      });
    } catch (err) {
      console.error('[CollectionNode] Failed to create request:', err);
    }
  };

  const rawItems = collection?.root.items ?? [];
  const filteredItems = sortItemsFoldersFirst(
    filter
      ? rawItems.filter(
          (item) =>
            item.type !== 'request' || item.name.toLowerCase().includes(filter.toLowerCase()),
        )
      : rawItems,
  );

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className='group relative flex items-center'>
          <TreeItem value={summary.uid} open={open} onOpenChange={setOpen} className='flex-1'>
            <TreeItemContent
              className='flex gap-2 w-full  py-1 cursor-pointer'
              onClick={handleClick}
              onDoubleClick={handleDoubleClick}
              aria-label={`${open ? 'Collapse' : 'Expand'} collection ${summary.name}`}
            >
              {open ? (
                <ChevronDown
                  aria-hidden='true'
                  className='h-5 w-5 flex-none text-muted-foreground'
                  strokeWidth={1.5}
                />
              ) : (
                <ChevronRight
                  aria-hidden='true'
                  className='h-5 w-5 flex-none text-muted-foreground'
                  strokeWidth={1.5}
                />
              )}
              <BoxIcon aria-hidden='true' className='h-5 w-5 shrink-0' />
              {isRenaming ? (
                <Input
                  autoFocus
                  className='h-6 text-sm flex-1'
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') void handleRename();
                    if (e.key === 'Escape') setIsRenaming(false);
                  }}
                  onBlur={() => void handleRename()}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <>
                  <span className='truncate text-foreground'>{summary.name}</span>
                  {collectionRoot && (
                    <ContractBadge
                      contracts={collectionScopedContracts}
                      collectionName={summary.name}
                      collectionRoot={collectionRoot}
                    />
                  )}
                  {summary.refType === 'external' && (
                    <span className='ml-auto shrink-0 text-2xs text-foreground bg-muted px-1.5 py-0.5 rounded'>
                      ext
                    </span>
                  )}
                </>
              )}
            </TreeItemContent>
          </TreeItem>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type='button'
                aria-label={`Actions for ${summary.name}`}
                className='absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-muted text-foreground'
                onClick={(e) => e.stopPropagation()}
              >
                <MoreHorizontal aria-hidden='true' className='h-3 w-3' />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              className='w-48'
              onClick={(e) => e.stopPropagation()}
            >
              <DropdownMenuItem
                onClick={(e) => handleDoubleClick(e as unknown as React.MouseEvent)}
              >
                <LayoutGrid className='h-3.5 w-3.5 mr-2' /> Overview
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => setCreateRequestOpen(true)}>
                <Plus className='h-3.5 w-3.5 mr-2' /> New Request
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={async () => {
                  await onNewFolder(summary.name, '');
                  setOpen(true);
                }}
              >
                <FolderPlus className='h-3.5 w-3.5 mr-2' /> New Folder
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                onClick={() => {
                  setRenameValue(summary.name);
                  setIsRenaming(true);
                }}
              >
                <Pencil className='h-3.5 w-3.5 mr-2' /> Rename
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                disabled={!collectionRoot}
                onClick={() => openContractTab(summary.name, collectionRoot)}
              >
                <Lock className='h-3.5 w-3.5 mr-2' /> Manage contracts
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className='text-destructive'
                onClick={() =>
                  onDelete({
                    type: 'collection',
                    collection: summary.name,
                    name: summary.name,
                  })
                }
              >
                <Trash2 className='h-3.5 w-3.5 mr-2' /> Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>

      <ContextMenuContent className='w-48'>
        <ContextMenuItem
          onClick={() => {
            const tab: CollectionTab = {
              id: summary.uid,
              title: summary.name,
              tabType: 'collection',
              collectionName: summary.name,
              isDirty: false,
            };
            usePaneStore.getState().openTab(tab);
          }}
        >
          <LayoutGrid className='h-3.5 w-3.5 mr-2' /> Overview
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => setCreateRequestOpen(true)}>
          <Plus className='h-3.5 w-3.5 mr-2' /> New Request
        </ContextMenuItem>
        <ContextMenuItem onClick={() => void onNewFolder(summary.name, '')}>
          <FolderPlus className='h-3.5 w-3.5 mr-2' /> New Folder
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          onClick={() => {
            setRenameValue(summary.name);
            setIsRenaming(true);
          }}
        >
          <Pencil className='h-3.5 w-3.5 mr-2' /> Rename
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          disabled={!collectionRoot}
          onClick={() => openContractTab(summary.name, collectionRoot)}
        >
          <Lock className='h-3.5 w-3.5 mr-2 text-muted-foreground' />
          Manage contracts
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          className='text-destructive'
          onClick={() =>
            onDelete({
              type: 'collection',
              collection: summary.name,
              name: summary.name,
            })
          }
        >
          <Trash2 className='h-3.5 w-3.5 mr-2' /> Delete
        </ContextMenuItem>
      </ContextMenuContent>

      {open && collection && (
        <div className='border-l border-border/70 ml-8'>
          {filteredItems.map((item) => {
            if (item.type === 'folder') {
              const folderDirName = item.dirName ?? item.name;
              return (
                <FolderNode
                  key={`folder-${folderDirName}`}
                  name={item.name}
                  items={item.items}
                  collectionName={summary.name}
                  collectionRoot={collectionRoot}
                  basePath={folderDirName}
                  depth={1}
                  filter={filter}
                  summaries={summaries}
                  onNewFolder={onNewFolder}
                  onMove={onMove}
                  onDelete={onDelete}
                  onDuplicate={onDuplicate}
                />
              );
            }
            return (
              <RequestNode
                key={`request-${item.uid}`}
                uid={item.uid}
                name={item.name}
                method={item.method}
                collectionName={summary.name}
                collectionRoot={collectionRoot}
                path={item.fileName ?? item.name}
                itemData={item}
                summaries={summaries}
                onMove={onMove}
                onDelete={onDelete}
                onDuplicate={onDuplicate}
              />
            );
          })}
          {creatingRequest && (
            <div className='flex items-center gap-1 px-2 py-1 text-sm'>
              <Input
                autoFocus
                className='h-5 text-sm flex-1'
                placeholder='Request name'
                value={newRequestName}
                onChange={(e) => setNewRequestName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void handleNewRequestCreate();
                  if (e.key === 'Escape') setCreatingRequest(false);
                }}
                onBlur={() => setCreatingRequest(false)}
                onClick={(e) => e.stopPropagation()}
              />
            </div>
          )}
        </div>
      )}
      <CreateRequestDialog
        open={createRequestOpen}
        collectionName={summary.name}
        onClose={() => setCreateRequestOpen(false)}
      />
    </ContextMenu>
  );
}
