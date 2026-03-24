import { useEffect, useState, useCallback, useRef } from 'react';
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
  renameRequest,
  moveItem,
  type CollectionSummary,
  type Collection,
  type CollectionItem,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { createDefaultRequest, mapApiRequestToState } from '@/lib/pane-utils';
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

// Returns true if any active tab in the pane tree matches the given tabId.
function isActiveRequest(node: PaneNode, tabId: string): boolean {
  if (node.type === 'leaf') return node.activeTabId === tabId;
  return isActiveRequest(node.children[0], tabId) || isActiveRequest(node.children[1], tabId);
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
  uid,
  name,
  method,
  collectionName,
  path,
  itemData,
  summaries,
  onMove,
  onDelete,
  onDuplicate,
}: {
  uid: string;
  name: string;
  method: string;
  collectionName: string;
  path: string;
  // Full request data from the collection tree, used to populate the new tab.
  itemData: Extract<CollectionItem, { type: 'request' }>;
  summaries: CollectionSummary[];
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}) {
  const root = usePaneStore((s) => s.root);
  const active = isActiveRequest(root, uid);

  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) { setIsRenaming(false); return; }
    try {
      await renameRequest(collectionName, path, trimmed);
      setIsRenaming(false);
    } catch (err) {
      console.error('Rename request failed:', err);
    }
  };

  function handleClick() {
    const request: RequestState = mapApiRequestToState(itemData);
    const tab: Tab = {
      id: uid,
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
            className={cn(
              'flex items-center gap-1.5 w-full px-2 py-1 text-left text-xs rounded-sm hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring cursor-pointer',
              active && 'bg-accent/50 text-accent-foreground',
            )}
            onClick={handleClick}
            aria-label={`Open ${method} ${name}`}
          >
            <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <span className={cn('w-9 shrink-0 font-semibold text-[10px]', methodColor(method))}>
              {method}
            </span>
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
              <span className="truncate text-foreground">{name}</span>
            )}
          </button>
          <div className="absolute right-1 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
            <button
              type="button"
              className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
              onClick={(e) => { e.stopPropagation(); void onDuplicate(collectionName, path, name); }}
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
        <ContextMenuItem onClick={(e) => { e.stopPropagation(); void onDuplicate(collectionName, path, name); }}>
          Duplicate
        </ContextMenuItem>
        <ContextMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>
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
  onDuplicate,
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
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}) {
  const [expanded, setExpanded] = useState(depth < 2);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) { setIsRenaming(false); return; }
    // Build the new path by replacing the last segment of basePath with the new name.
    const parts = basePath.split('/');
    parts[parts.length - 1] = trimmed;
    const newPath = parts.join('/');
    try {
      await moveItem(collectionName, basePath, collectionName, newPath);
      setIsRenaming(false);
    } catch (err) {
      console.error('Rename folder failed:', err);
    }
  };

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
                <span className="truncate font-medium text-foreground">{name}</span>
              )}
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
          <ContextMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>
            Rename
          </ContextMenuItem>
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
                  onDuplicate={onDuplicate}
                />
              );
            }
            const fileName = item.fileName ?? item.name;
            const requestPath = basePath
              ? `${basePath}/${fileName}`
              : fileName;
            return (
              <RequestNode
                key={`request-${requestPath}-${idx}`}
                uid={item.uid}
                name={item.name}
                method={item.method}
                collectionName={collectionName}
                path={requestPath}
                itemData={item}
                summaries={summaries}
                onMove={onMove}
                onDelete={onDelete}
                onDuplicate={onDuplicate}
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
  onDuplicate,
}: {
  summary: CollectionSummary;
  filter: string;
  summaries: CollectionSummary[];
  onNewRequest: (collection: string, folderPath: string) => Promise<void>;
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
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

  // Fetch full collection tree.
  const refreshTree = useCallback(() => {
    if (!expanded) return;
    getCollection(summary.name)
      .then(setCollection)
      .catch((err) => console.error('[CollectionsSidebar] fetch error', err));
  }, [expanded, summary.name]);

  // Fetch when first expanded.
  useEffect(() => {
    if (expanded && !collection) {
      refreshTree();
    }
  }, [expanded, collection, refreshTree]);

  // Refresh the expanded tree only when THIS collection is affected.
  // Debounced to collapse rapid filesystem events into one refresh.
  const treeDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!expanded) return;
    let unlisten: (() => void) | undefined;
    onCollectionChanged((event) => {
      const affected = event.collection ?? event.name;
      if (!affected || affected === summary.name) {
        if (treeDebounce.current) clearTimeout(treeDebounce.current);
        treeDebounce.current = setTimeout(() => refreshTree(), 300);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
      if (treeDebounce.current) clearTimeout(treeDebounce.current);
    };
  }, [expanded, refreshTree, summary.name]);

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
                onClick={async (e) => {
                  e.stopPropagation();
                  await onNewRequest(summary.name, '');
                  setExpanded(true);
                  setCollection(null);
                }}
                title="New Request"
              >
                <Plus className="h-3 w-3" />
              </button>
              <button
                type="button"
                className="h-5 w-5 flex items-center justify-center rounded-sm hover:bg-muted text-muted-foreground"
                onClick={async (e) => {
                  e.stopPropagation();
                  await onNewFolder(summary.name, '');
                  setExpanded(true);
                  setCollection(null);
                }}
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
                  onDuplicate={onDuplicate}
                />
              );
            }
            return (
              <RequestNode
                key={`request-${item.fileName ?? item.name}-${idx}`}
                uid={item.uid}
                name={item.name}
                method={item.method}
                collectionName={summary.name}
                path={item.fileName ?? item.name}
                itemData={item}
                summaries={summaries}
                onMove={onMove}
                onDelete={onDelete}
                onDuplicate={onDuplicate}
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

  const fetchCollections = useCallback(async () => {
    try {
      const results = await listCollections();
      setSummaries(results);
    } catch (err) {
      console.error('[CollectionsSidebar] list error', err);
    }
  }, []);

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
    // Find next available name (New Request, New Request 2, New Request 3...).
    let name = 'New Request';
    try {
      const col = await getCollection(collection);
      const items = col.root.items;
      const existing = new Set(
        items.filter((i: CollectionItem) => i.type === 'request').map((i: CollectionItem) => i.name),
      );
      let counter = 1;
      while (existing.has(name)) {
        counter++;
        name = `New Request ${counter}`;
      }
    } catch { /* Use default name if fetch fails. */ }
    const path = folderPath ? `${folderPath}/${name}` : name;
    const saved = await saveRequest(collection, path, {
      uid: '',
      name,
      method: 'GET',
      url: '',
      headers: [],
      auth: { authType: 'none' },
    });
    const tab: Tab = {
      id: saved.uid,
      title: name,
      tabType: 'request',
      request: createDefaultRequest(),
      response: null,
      isDirty: false,
      source: { collection, path: saved.fileName ?? (path.endsWith('.json') ? path : `${path}.json`) },
    };
    usePaneStore.getState().openTab(tab);
  }, []);

  const handleNewFolder = useCallback(async (collection: string, folderPath: string) => {
    // Find next available name (New Folder, New Folder 2, New Folder 3...).
    let name = 'New Folder';
    try {
      const col = await getCollection(collection);
      const items = col.root.items;
      const existing = new Set(
        items.filter((i: CollectionItem) => i.type === 'folder').map((i: CollectionItem) => i.name),
      );
      let counter = 1;
      while (existing.has(name)) {
        counter++;
        name = `New Folder ${counter}`;
      }
    } catch { /* Use default name if fetch fails. */ }
    const path = folderPath ? `${folderPath}/${name}` : name;
    try {
      await createFolder(collection, path);
    } catch (err) {
      console.error('[CollectionsSidebar] create folder failed:', err);
    }
  }, []);

  const handleMove = useCallback(async (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => {
    await moveItem(srcCollection, srcPath, dstCollection, dstPath);
  }, []);

  const handleDuplicate = useCallback(async (collection: string, path: string, name: string) => {
    try {
      // Read the existing collection to locate the source request.
      const col = await getCollection(collection);
      const items = col.root.items;

      // Recursively find the request by fileName or name.
      const findRequest = (nodes: CollectionItem[], targetPath: string): CollectionItem | undefined => {
        for (const item of nodes) {
          if (item.type === 'request') {
            const fn = item.fileName ?? item.name;
            if (fn === targetPath || item.name === name) return item;
          }
          if (item.type === 'folder') {
            const found = findRequest(item.items, targetPath);
            if (found) return found;
          }
        }
        return undefined;
      };

      const source = findRequest(items, path.split('/').pop() ?? path);
      if (!source || source.type !== 'request') return;

      // Collect existing names at the top level to find a unique copy name.
      const existing = new Set(
        items.filter((i: CollectionItem) => i.type === 'request').map((i: CollectionItem) => i.name),
      );
      let copyName = `${name} copy`;
      let counter = 1;
      while (existing.has(copyName)) {
        counter++;
        copyName = `${name} copy ${counter}`;
      }

      // Preserve the folder prefix from the original path.
      const pathParts = path.split('/');
      pathParts.pop();
      const folderPath = pathParts.join('/');
      const newPath = folderPath ? `${folderPath}/${copyName}` : copyName;

      // Save the duplicate with all source data. New uid and name, rest copied.
      const { type: _t, uid: _u, name: _n, fileName: _f, ...requestData } = source;
      await saveRequest(collection, newPath, {
        ...requestData,
        uid: '',
        name: copyName,
      });
    } catch (err) {
      console.error('[CollectionsSidebar] duplicate failed:', err);
    }
  }, []);

  // Load collections on mount. Debounce file watcher events so rapid
  // filesystem changes collapse into one refresh.
  const listDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    void fetchCollections();
    let unlisten: (() => void) | undefined;
    onCollectionChanged(() => {
      if (listDebounce.current) clearTimeout(listDebounce.current);
      listDebounce.current = setTimeout(() => void fetchCollections(), 300);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (listDebounce.current) clearTimeout(listDebounce.current);
      unlisten?.();
    };
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
                    onDuplicate={handleDuplicate}
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
