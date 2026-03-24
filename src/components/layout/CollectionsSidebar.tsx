import { useEffect, useState, useCallback } from 'react';
import {
  listCollections,
  getCollection,
  onCollectionChanged,
  createCollection,
  saveRequest,
  createFolder,
  deleteCollection,
  deleteFolder,
  deleteRequest,
  renameCollection,
  moveItem,
  type CollectionSummary,
  type Collection,
  type CollectionItem,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { createDefaultRequest } from '@/lib/pane-utils';
import type { Tab, RequestState, PaneNode } from '@/types/pane-types';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { cn } from '@/lib/utils';
import {
  ChevronRight,
  ChevronDown,
  Folder,
  FolderOpen,
  FileText,
  Search,
  Plus,
  Copy,
  Trash2,
  FolderPlus,
  Settings,
  Upload,
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { HistoryPanel } from '@/components/history/HistoryPanel';

// Returns Tailwind text color for an HTTP method.
function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case 'GET':    return 'text-emerald-500';
    case 'POST':   return 'text-amber-500';
    case 'PUT':    return 'text-blue-500';
    case 'PATCH':  return 'text-violet-500';
    case 'DELETE': return 'text-red-500';
    case 'OPTIONS': return 'text-cyan-500';
    case 'HEAD':   return 'text-pink-500';
    default:       return 'text-muted-foreground';
  }
}

// Delete target descriptor used by the shared confirmation dialog.
type DeleteTarget = {
  type: 'collection' | 'folder' | 'request';
  collection: string;
  path?: string;
  name: string;
};

// Renders a single request item in the collection tree.
function RequestNode({
  name,
  method,
  collectionName,
  path,
  summaries,
  onMove,
  onDelete,
}: {
  name: string;
  method: string;
  collectionName: string;
  path: string;
  summaries: CollectionSummary[];
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
}) {
  function handleClick() {
    const tabId = `${collectionName}/${path}`;
    const request: RequestState = {
      ...createDefaultRequest(),
      method: method as RequestState['method'],
    };
    const tab: Tab = {
      id: tabId,
      title: name,
      tabType: 'request',
      request,
      response: null,
      isDirty: false,
      source: { collection: collectionName, path },
    };
    usePaneStore.getState().openTab(tab);
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className="group relative flex items-center">
          <button
            type="button"
            className="flex items-center gap-1.5 w-full px-2 py-1 text-left text-xs rounded-sm hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer"
            onClick={handleClick}
            aria-label={`Open ${method} ${name}`}
          >
            <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <span className={cn('w-9 shrink-0 font-semibold text-[10px]', methodColor(method))}>
              {method}
            </span>
            <span className="truncate text-foreground">{name}</span>
          </button>
          <div className="absolute right-1 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
            <button
              type="button"
              className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
              onClick={(e) => { e.stopPropagation(); }}
              title="Duplicate"
            >
              <Copy className="h-3 w-3" />
            </button>
            <button
              type="button"
              className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-destructive"
              onClick={(e) => { e.stopPropagation(); onDelete({ type: 'request', collection: collectionName, path, name }); }}
              title="Delete"
            >
              <Trash2 className="h-3 w-3" />
            </button>
          </div>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-48">
        <ContextMenuItem onClick={() => {}}>
          Duplicate
        </ContextMenuItem>
        <ContextMenuItem onClick={() => {}}>
          Rename
        </ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>Move to...</ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-48">
            {summaries.map((s) => (
              <ContextMenuItem
                key={s.name}
                onClick={() => onMove(collectionName, path, s.name, '')}
                disabled={s.name === collectionName}
              >
                {s.name}
              </ContextMenuItem>
            ))}
            {summaries.length === 0 && (
              <ContextMenuItem disabled>No collections</ContextMenuItem>
            )}
          </ContextMenuSubContent>
        </ContextMenuSub>
        <ContextMenuSeparator />
        <ContextMenuItem className="text-destructive" onClick={() => onDelete({ type: 'request', collection: collectionName, path, name })}>
          Delete
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

// Renders a folder node with expandable children.
function FolderNode({
  name,
  items,
  collectionName,
  basePath,
  depth,
  filter,
  summaries,
  onNewRequest,
  onNewFolder,
  onMove,
  onDelete,
}: {
  name: string;
  items: CollectionItem[];
  collectionName: string;
  basePath: string;
  depth: number;
  filter: string;
  summaries: CollectionSummary[];
  onNewRequest: (collection: string, folderPath: string) => Promise<void>;
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
}) {
  const [expanded, setExpanded] = useState(depth < 2);

  // Auto-expand when a search filter is active.
  useEffect(() => {
    if (filter) setExpanded(true);
  }, [filter]);

  const filteredItems = filter
    ? items.filter((item) => {
        if (item.type === 'request') {
          return item.name.toLowerCase().includes(filter);
        }
        return true;
      })
    : items;

  if (filter && filteredItems.length === 0) return null;

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="group relative flex items-center">
            <button
              type="button"
              className="flex items-center gap-1 w-full px-2 py-1 text-xs rounded-sm hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer"
              onClick={() => setExpanded((prev) => !prev)}
              aria-expanded={expanded}
              aria-label={`${expanded ? 'Collapse' : 'Expand'} folder ${name}`}
            >
              {expanded ? (
                <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              )}
              {expanded ? (
                <FolderOpen className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              ) : (
                <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              )}
              <span className="truncate font-medium text-foreground">{name}</span>
            </button>
            <div className="absolute right-1 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
              <button
                type="button"
                className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
                onClick={(e) => { e.stopPropagation(); void onNewRequest(collectionName, basePath); }}
                title="New Request"
              >
                <Plus className="h-3 w-3" />
              </button>
              <button
                type="button"
                className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
                onClick={(e) => { e.stopPropagation(); void onNewFolder(collectionName, basePath); }}
                title="New Folder"
              >
                <FolderPlus className="h-3 w-3" />
              </button>
            </div>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuItem onClick={() => void onNewRequest(collectionName, basePath)}>
            New Request
          </ContextMenuItem>
          <ContextMenuItem onClick={() => void onNewFolder(collectionName, basePath)}>
            New Folder
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem className="text-destructive" onClick={() => onDelete({ type: 'folder', collection: collectionName, path: basePath, name })}>
            Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {expanded && (
        <div className="pl-3">
          {filteredItems.map((item, idx) => {
            if (item.type === 'folder') {
              const folderPath = basePath ? `${basePath}/${item.name}` : item.name;
              return (
                <FolderNode
                  key={`folder-${folderPath}`}
                  name={item.name}
                  items={item.items}
                  collectionName={collectionName}
                  basePath={folderPath}
                  depth={depth + 1}
                  filter={filter}
                  summaries={summaries}
                  onNewRequest={onNewRequest}
                  onNewFolder={onNewFolder}
                  onMove={onMove}
                  onDelete={onDelete}
                />
              );
            }
            const requestPath = basePath
              ? `${basePath}/${item.name}`
              : item.name;
            return (
              <RequestNode
                key={`request-${requestPath}-${idx}`}
                name={item.name}
                method={item.method}
                collectionName={collectionName}
                path={requestPath}
                summaries={summaries}
                onMove={onMove}
                onDelete={onDelete}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

// Renders a top-level collection as an expandable tree node.
function CollectionNode({
  summary,
  filter,
  summaries,
  onNewRequest,
  onNewFolder,
  onMove,
  onDelete,
}: {
  summary: CollectionSummary;
  filter: string;
  summaries: CollectionSummary[];
  onNewRequest: (collection: string, folderPath: string) => Promise<void>;
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [collection, setCollection] = useState<Collection | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(summary.name);

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === summary.name) { setIsRenaming(false); return; }
    try {
      await renameCollection(summary.name, trimmed);
      setIsRenaming(false);
    } catch (err) {
      console.error('Rename failed:', err);
    }
  };

  // Fetch full collection data when expanded.
  useEffect(() => {
    if (expanded && !collection) {
      getCollection(summary.name)
        .then(setCollection)
        .catch((err) => console.error('[CollectionsSidebar] fetch error', err));
    }
  }, [expanded, collection, summary.name]);

  // Auto-expand when a filter is active.
  useEffect(() => {
    if (filter) setExpanded(true);
  }, [filter]);

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="group relative flex items-center">
            <button
              type="button"
              className="flex items-center gap-1.5 w-full px-2 py-1.5 text-xs rounded-sm hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer"
              onClick={() => setExpanded((prev) => !prev)}
              aria-expanded={expanded}
              aria-label={`${expanded ? 'Collapse' : 'Expand'} collection ${summary.name}`}
            >
              {expanded ? (
                <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              )}
              {expanded ? (
                <FolderOpen className="h-4 w-4 shrink-0 text-primary" />
              ) : (
                <Folder className="h-4 w-4 shrink-0 text-primary" />
              )}
              {isRenaming ? (
                <Input
                  autoFocus
                  className="h-6 text-xs flex-1"
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
                <span className="truncate font-medium text-foreground">{summary.name}</span>
              )}
              <span className="ml-auto text-[10px] text-muted-foreground">{summary.requestCount}</span>
            </button>
            <div className="absolute right-1 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
              <button
                type="button"
                className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
                onClick={(e) => { e.stopPropagation(); void onNewRequest(summary.name, ''); }}
                title="New Request"
              >
                <Plus className="h-3 w-3" />
              </button>
              <button
                type="button"
                className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
                onClick={(e) => { e.stopPropagation(); void onNewFolder(summary.name, ''); }}
                title="New Folder"
              >
                <FolderPlus className="h-3 w-3" />
              </button>
              <button
                type="button"
                className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
                onClick={(e) => { e.stopPropagation(); }}
                title="Settings"
              >
                <Settings className="h-3 w-3" />
              </button>
            </div>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuItem onClick={() => void onNewRequest(summary.name, '')}>
            New Request
          </ContextMenuItem>
          <ContextMenuItem onClick={() => void onNewFolder(summary.name, '')}>
            New Folder
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => setIsRenaming(true)}>
            Rename
          </ContextMenuItem>
          <ContextMenuItem className="text-destructive" onClick={() => onDelete({ type: 'collection', collection: summary.name, name: summary.name })}>
            Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {expanded && collection && (
        <div className="pl-2">
          {collection.root.items.map((item, idx) => {
            if (item.type === 'folder') {
              return (
                <FolderNode
                  key={`folder-${item.name}`}
                  name={item.name}
                  items={item.items}
                  collectionName={summary.name}
                  basePath={item.name}
                  depth={1}
                  filter={filter}
                  summaries={summaries}
                  onNewRequest={onNewRequest}
                  onNewFolder={onNewFolder}
                  onMove={onMove}
                  onDelete={onDelete}
                />
              );
            }
            return (
              <RequestNode
                key={`request-${item.name}-${idx}`}
                name={item.name}
                method={item.method}
                collectionName={summary.name}
                path={item.name}
                summaries={summaries}
                onMove={onMove}
                onDelete={onDelete}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

// Sidebar panel with Collections tree and History tabs.
export function CollectionsSidebar() {
  const [summaries, setSummaries] = useState<CollectionSummary[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const filter = searchQuery.toLowerCase().trim();

  const [view, setView] = useState<'collections' | 'history'>('collections');

  const handleImport = useCallback(async () => {
    const file = await open({
      multiple: false,
      filters: [{ name: 'Collection', extensions: ['json'] }],
    });
    if (file) {
      console.log('Import file selected:', file);
    }
  }, []);

  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState('');
  const [createError, setCreateError] = useState('');

  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null);

  const confirmDelete = useCallback(async () => {
    if (!deleteTarget) return;
    try {
      if (deleteTarget.type === 'collection') {
        await deleteCollection(deleteTarget.collection);
      } else if (deleteTarget.type === 'folder') {
        await deleteFolder(deleteTarget.collection, deleteTarget.path!);
      } else {
        await deleteRequest(deleteTarget.collection, deleteTarget.path!);
      }
      // Close open tabs for deleted items.
      const store = usePaneStore.getState();
      const closeTabs = (node: PaneNode): void => {
        if (node.type === 'leaf') {
          for (const tab of node.tabs) {
            if (!tab.source) continue;
            const matches =
              (deleteTarget.type === 'collection' && tab.source.collection === deleteTarget.collection) ||
              (deleteTarget.type === 'request' && tab.source.collection === deleteTarget.collection && tab.source.path === deleteTarget.path) ||
              (deleteTarget.type === 'folder' && tab.source.collection === deleteTarget.collection && tab.source.path.startsWith(deleteTarget.path!));
            if (matches) store.closeTab(tab.id, node.groupId);
          }
        } else {
          closeTabs(node.children[0]);
          closeTabs(node.children[1]);
        }
      };
      closeTabs(store.root);
    } catch (err) {
      console.error('Delete failed:', err);
    }
    setDeleteTarget(null);
  }, [deleteTarget]);

  const INVALID_CHARS = /[/\\:*?"<>|]/;

  const handleCreateCollection = useCallback(async () => {
    const trimmed = newName.trim();
    if (!trimmed) {
      setIsCreating(false);
      setNewName('');
      return;
    }
    if (INVALID_CHARS.test(trimmed)) {
      setCreateError('Name contains invalid characters.');
      return;
    }
    try {
      await createCollection(trimmed);
      setIsCreating(false);
      setNewName('');
      setCreateError('');
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : 'Failed to create collection.');
    }
  }, [newName]);

  const handleNewRequest = useCallback(async (collection: string, folderPath: string) => {
    const name = 'New Request';
    const path = folderPath ? `${folderPath}/${name}` : name;
    await saveRequest(collection, path, {
      name,
      method: 'GET',
      url: '',
      headers: [],
      auth: { authType: 'none' },
    });
    const tab: Tab = {
      id: `${collection}/${path}`,
      title: name,
      tabType: 'request',
      request: createDefaultRequest(),
      response: null,
      isDirty: false,
      source: { collection, path },
    };
    usePaneStore.getState().openTab(tab);
  }, []);

  const handleNewFolder = useCallback(async (collection: string, folderPath: string) => {
    const name = 'New Folder';
    const path = folderPath ? `${folderPath}/${name}` : name;
    await createFolder(collection, path);
  }, []);

  const handleMove = useCallback(async (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => {
    await moveItem(srcCollection, srcPath, dstCollection, dstPath);
  }, []);

  const fetchCollections = useCallback(async () => {
    try {
      const results = await listCollections();
      setSummaries(results);
    } catch (err) {
      console.error('[CollectionsSidebar] list error', err);
    }
  }, []);

  // Load collections on mount and subscribe to changes.
  useEffect(() => {
    void fetchCollections();
    let unlisten: (() => void) | undefined;
    onCollectionChanged(() => void fetchCollections()).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [fetchCollections]);

  return (
    <div className="h-full flex flex-col bg-card/50 backdrop-blur-sm border-r border-border/50">
      {/* View selector and action icons. */}
      <div className="flex items-center gap-1 px-2 pt-2 pb-1">
        <Select value={view} onValueChange={(v) => setView(v as 'collections' | 'history')}>
          <SelectTrigger className="h-8 flex-1 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="collections">Collections</SelectItem>
            <SelectItem value="history">History</SelectItem>
          </SelectContent>
        </Select>
        {view === 'collections' && (
          <>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
              onClick={() => setIsCreating(true)}
              title="New Collection"
            >
              <Plus className="h-4 w-4" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
              onClick={() => void handleImport()}
              title="Import Collection"
            >
              <Upload className="h-4 w-4" />
            </Button>
          </>
        )}
      </div>

      {view === 'collections' ? (
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Search and inline create. */}
          <div className="px-2 pb-2 space-y-1.5">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="h-7 pl-7 text-xs"
                placeholder="Search requests..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                aria-label="Search collections"
              />
            </div>
            {isCreating && (
              <div className="px-1">
                <Input
                  autoFocus
                  className="h-7 text-xs"
                  placeholder="Collection name"
                  value={newName}
                  onChange={(e) => { setNewName(e.target.value); setCreateError(''); }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') handleCreateCollection();
                    if (e.key === 'Escape') { setIsCreating(false); setNewName(''); setCreateError(''); }
                  }}
                  onBlur={() => { setIsCreating(false); setNewName(''); setCreateError(''); }}
                />
                {createError && (
                  <p className="text-[10px] text-destructive mt-0.5 px-1">{createError}</p>
                )}
              </div>
            )}
          </div>

          {/* Collection tree. */}
          <ScrollArea className="flex-1">
            <div className="px-1 pb-2">
              {summaries.length === 0 ? (
                <p className="px-3 py-4 text-center text-xs text-muted-foreground">
                  No collections yet.
                </p>
              ) : (
                summaries.map((s) => (
                  <CollectionNode
                    key={s.name}
                    summary={s}
                    filter={filter}
                    summaries={summaries}
                    onNewRequest={handleNewRequest}
                    onNewFolder={handleNewFolder}
                    onMove={handleMove}
                    onDelete={setDeleteTarget}
                  />
                ))
              )}
            </div>
          </ScrollArea>
        </div>
      ) : (
        <div className="flex-1 overflow-hidden">
          <HistoryPanel />
        </div>
      )}

      <AlertDialog open={!!deleteTarget} onOpenChange={(open) => { if (!open) setDeleteTarget(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Confirm Delete</AlertDialogTitle>
            <AlertDialogDescription>
              {deleteTarget?.type === 'collection'
                ? `Delete collection '${deleteTarget.name}'? This removes all requests inside it.`
                : deleteTarget?.type === 'folder'
                ? `Delete folder '${deleteTarget.name}' and all requests inside it?`
                : `Delete request '${deleteTarget?.name}'?`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>Delete</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
