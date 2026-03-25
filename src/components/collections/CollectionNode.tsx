import { useState, useEffect, useCallback, useRef } from 'react';
import { DndContext, DragOverlay, closestCenter, type DragEndEvent, type DragStartEvent } from '@dnd-kit/core';
import { SortableContext, verticalListSortingStrategy, arrayMove } from '@dnd-kit/sortable';
import { Folder, FolderOpen, FolderPlus, Plus, Trash2, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem,
  DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  ContextMenu, ContextMenuContent, ContextMenuItem,
  ContextMenuSeparator, ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { getCollection, onCollectionChanged, renameCollection, reorderItems } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { FolderNode } from './FolderNode';
import { RequestNode } from './RequestNode';
import type { CollectionSummary, Collection, CollectionItem } from '@/lib/tauri-api';
import type { CollectionTab } from '@/types/pane-types';
import type { DeleteTarget } from './tree-utils';

interface CollectionNodeProps {
  summary: CollectionSummary;
  filter: string;
  summaries: CollectionSummary[];
  onNewRequest: (collection: string, folderPath: string) => Promise<void>;
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function CollectionNode({
  summary, filter, summaries,
  onNewRequest, onNewFolder, onMove, onDelete, onDuplicate,
}: CollectionNodeProps) {
  const [open, setOpen] = useState(false);
  const [collection, setCollection] = useState<Collection | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(summary.name);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [localItems, setLocalItems] = useState<CollectionItem[]>([]);
  const clickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const treeDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Keep localItems in sync when collection data is fetched/refreshed.
  useEffect(() => {
    if (collection) setLocalItems(collection.root.items);
  }, [collection]);

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
    let unlisten: (() => void) | undefined;
    onCollectionChanged((event) => {
      const affected = event.collection ?? event.name;
      if (!affected || affected === summary.name) {
        if (treeDebounce.current) clearTimeout(treeDebounce.current);
        treeDebounce.current = setTimeout(() => refreshTree(), 300);
      }
    }).then((fn) => { unlisten = fn; });
    return () => {
      unlisten?.();
      if (treeDebounce.current) clearTimeout(treeDebounce.current);
    };
  }, [open, refreshTree, summary.name]);

  // Auto-expand when filter is active.
  useEffect(() => { if (filter) setOpen(true); }, [filter]);

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === summary.name) { setIsRenaming(false); return; }
    try {
      await renameCollection(summary.name, trimmed);
      setIsRenaming(false);
    } catch (err) {
      console.error('Rename collection failed:', err);
    }
  };

  // Single click toggles expand after 250 ms (to allow double-click to fire first).
  const handleClick = () => {
    if (clickTimer.current) { clearTimeout(clickTimer.current); clickTimer.current = null; return; }
    clickTimer.current = setTimeout(() => {
      clickTimer.current = null;
      setOpen((prev) => !prev);
    }, 250);
  };

  // Double click opens the collection Overview tab.
  const handleDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (clickTimer.current) { clearTimeout(clickTimer.current); clickTimer.current = null; }
    const tab: CollectionTab = {
      id: summary.uid,
      title: summary.name,
      tabType: 'collection',
      collectionName: summary.name,
      isDirty: false,
    };
    usePaneStore.getState().openTab(tab);
  };

  const filteredItems = filter
    ? localItems.filter((item) => item.type !== 'request' || item.name.toLowerCase().includes(filter.toLowerCase()))
    : localItems;

  // sortableIds must match filteredItems exactly (Fix A).
  const sortableIds = filteredItems.map((item) => item.type === 'request' ? item.uid : item.name);

  const handleDragStart = ({ active }: DragStartEvent) => setActiveId(String(active.id));
  const handleDragCancel = () => setActiveId(null);
  const handleDragEnd = async ({ active, over }: DragEndEvent) => {
    setActiveId(null);
    if (!over || active.id === over.id) return;
    const oldIdx = sortableIds.indexOf(String(active.id));
    const newIdx = sortableIds.indexOf(String(over.id));
    if (oldIdx === -1 || newIdx === -1) return;
    const reordered = arrayMove(localItems, oldIdx, newIdx);
    setLocalItems(reordered);
    const orderedNames = reordered.map((i) => i.type === 'request' ? (i.fileName ?? i.name) : i.name);
    try {
      await reorderItems(summary.name, '', orderedNames);
    } catch (err) {
      console.error('Reorder failed, reverting:', err);
      if (collection) setLocalItems(collection.root.items);
    }
  };

  const activeItem = activeId ? localItems.find((i) => (i.type === 'request' ? i.uid : i.name) === activeId) : null;

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className="group relative flex items-center">
          <TreeItem value={summary.uid} open={open} onOpenChange={setOpen} className="flex-1">
            <TreeItemContent
              className="flex items-center gap-1.5 w-full px-2 py-1.5 text-xs rounded-sm cursor-pointer"
              onClick={handleClick}
              onDoubleClick={handleDoubleClick}
              aria-label={`${open ? 'Collapse' : 'Expand'} collection ${summary.name}`}
            >
              {open
                ? <FolderOpen className="h-4 w-4 shrink-0 text-primary" />
                : <Folder className="h-4 w-4 shrink-0 text-primary" />
              }
              {isRenaming ? (
                <Input
                  autoFocus className="h-6 text-xs flex-1"
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onKeyDown={(e) => { if (e.key === 'Enter') void handleRename(); if (e.key === 'Escape') setIsRenaming(false); }}
                  onBlur={() => void handleRename()}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <span className="truncate font-medium text-foreground">{summary.name}</span>
              )}
              <Badge variant="outline" className="ml-auto text-[10px] shrink-0">{summary.requestCount}</Badge>
            </TreeItemContent>
          </TreeItem>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground"
                onClick={(e) => e.stopPropagation()}
              >
                <MoreHorizontal className="h-3 w-3" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent className="w-48" onClick={(e) => e.stopPropagation()}>
              <DropdownMenuItem onClick={(e) => handleDoubleClick(e as unknown as React.MouseEvent)}>Overview</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={async () => { await onNewRequest(summary.name, ''); setOpen(true); setCollection(null); }}>
                <Plus className="h-3.5 w-3.5 mr-2" /> New Request
              </DropdownMenuItem>
              <DropdownMenuItem onClick={async () => { await onNewFolder(summary.name, ''); setOpen(true); setCollection(null); }}>
                <FolderPlus className="h-3.5 w-3.5 mr-2" /> New Folder
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => { setRenameValue(summary.name); setIsRenaming(true); }}>Rename</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className="text-destructive"
                onClick={() => onDelete({ type: 'collection', collection: summary.name, name: summary.name })}
              >
                <Trash2 className="h-3.5 w-3.5 mr-2" /> Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>

      <ContextMenuContent className="w-48">
        <ContextMenuItem onClick={() => {
          const tab: CollectionTab = {
            id: summary.uid,
            title: summary.name,
            tabType: 'collection',
            collectionName: summary.name,
            isDirty: false,
          };
          usePaneStore.getState().openTab(tab);
        }}>Overview</ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => void onNewRequest(summary.name, '')}>New Request</ContextMenuItem>
        <ContextMenuItem onClick={() => void onNewFolder(summary.name, '')}>New Folder</ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => { setRenameValue(summary.name); setIsRenaming(true); }}>Rename</ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          className="text-destructive"
          onClick={() => onDelete({ type: 'collection', collection: summary.name, name: summary.name })}
        >Delete</ContextMenuItem>
      </ContextMenuContent>

      {open && collection && (
        <div className="pl-2 border-l border-border/30 ml-3">
          <DndContext
            collisionDetection={closestCenter}
            onDragStart={handleDragStart}
            onDragEnd={handleDragEnd}
            onDragCancel={handleDragCancel}
          >
            <SortableContext items={sortableIds} strategy={verticalListSortingStrategy}>
              {filteredItems.map((item) => {
                if (item.type === 'folder') {
                  return (
                    <FolderNode
                      key={`folder-${item.name}`}
                      name={item.name} items={item.items}
                      collectionName={summary.name} basePath={item.name}
                      depth={1} filter={filter} summaries={summaries}
                      onNewRequest={onNewRequest} onNewFolder={onNewFolder}
                      onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                    />
                  );
                }
                return (
                  <RequestNode
                    key={item.uid}
                    uid={item.uid} name={item.name} method={item.method}
                    collectionName={summary.name} path={item.fileName ?? item.name}
                    itemData={item} summaries={summaries} dragDisabled={!!filter}
                    onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                  />
                );
              })}
            </SortableContext>
            <DragOverlay>
              {activeItem && activeItem.type === 'request' && (
                <div className="flex items-center gap-1.5 px-2 py-1 text-xs rounded-sm bg-card border border-border shadow-lg opacity-90">
                  <span className="text-muted-foreground">{activeItem.method}</span>
                  <span>{activeItem.name}</span>
                </div>
              )}
              {activeItem && activeItem.type === 'folder' && (
                <div className="flex items-center gap-1 px-2 py-1 text-xs rounded-sm bg-card border border-border shadow-lg opacity-90">
                  <Folder className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{activeItem.name}</span>
                </div>
              )}
            </DragOverlay>
          </DndContext>
        </div>
      )}
    </ContextMenu>
  );
}
