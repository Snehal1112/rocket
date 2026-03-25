import { useState, useEffect } from 'react';
import { DndContext, DragOverlay, closestCenter, type DragEndEvent, type DragStartEvent } from '@dnd-kit/core';
import { SortableContext, verticalListSortingStrategy, arrayMove, useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Folder, FolderOpen, FolderPlus, Plus, Trash2, Pencil, GripVertical, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem,
  DropdownMenuSeparator, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  ContextMenu, ContextMenuContent, ContextMenuItem,
  ContextMenuSeparator, ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { moveItem, reorderItems } from '@/lib/tauri-api';
import { RequestNode } from './RequestNode';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import type { DeleteTarget } from './tree-utils';

interface FolderNodeProps {
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
}

export function FolderNode({
  name, items, collectionName, basePath, depth, filter,
  summaries, onNewRequest, onNewFolder, onMove, onDelete, onDuplicate,
}: FolderNodeProps) {
  const [open, setOpen] = useState(depth < 2);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [localItems, setLocalItems] = useState(items);

  // Keep localItems in sync when parent refetches collection data.
  useEffect(() => { setLocalItems(items); }, [items]);

  // Auto-expand when filter is active.
  useEffect(() => { if (filter) setOpen(true); }, [filter]);

  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: basePath, disabled: !!filter });

  const style = { transform: CSS.Transform.toString(transform), transition, opacity: isDragging ? 0.5 : 1 };

  const handleRename = async () => {
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) { setIsRenaming(false); return; }
    // Folder rename is done by moving the folder to a new path (no rename_folder command).
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

  const filteredItems = filter
    ? localItems.filter((item) => item.type !== 'request' || item.name.toLowerCase().includes(filter.toLowerCase()))
    : localItems;

  if (filter && filteredItems.length === 0) return null;

  // IDs for SortableContext — folders use basePath/name, requests use uid.
  const sortableIds = filteredItems.map((item) =>
    item.type === 'folder' ? `${basePath}/${item.name}` : item.uid
  );

  const handleDragStart = ({ active }: DragStartEvent) => setActiveId(String(active.id));
  const handleDragCancel = () => setActiveId(null);
  const handleDragEnd = async ({ active, over }: DragEndEvent) => {
    setActiveId(null);
    if (!over || active.id === over.id) return;
    const oldIdx = sortableIds.indexOf(String(active.id));
    const newIdx = sortableIds.indexOf(String(over.id));
    if (oldIdx === -1 || newIdx === -1) return;
    const reordered = arrayMove(localItems, oldIdx, newIdx);
    setLocalItems(reordered); // Optimistic update.
    const orderedNames = reordered.map((i) => i.type === 'request' ? (i.fileName ?? i.name) : i.name);
    try {
      await reorderItems(collectionName, basePath, orderedNames);
    } catch (err) {
      console.error('Reorder failed, reverting:', err);
      setLocalItems(items); // Revert on failure.
    }
  };

  const activeItem = activeId ? localItems.find((i) => (i.type === 'request' ? i.uid : `${basePath}/${i.name}`) === activeId) : null;

  return (
    <div ref={setNodeRef} style={style} {...attributes}>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="group relative flex items-center">
            <button
              type="button"
              className="absolute left-0 h-full px-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing text-muted-foreground"
              {...listeners} tabIndex={-1}
            >
              <GripVertical className="h-3 w-3" />
            </button>

            <TreeItem value={basePath} open={open} onOpenChange={setOpen} className="flex-1">
              <TreeItemContent className="flex items-center gap-1 w-full px-2 pl-4 py-1 text-xs rounded-sm cursor-pointer">
                {open ? <FolderOpen className="h-3.5 w-3.5 shrink-0 text-muted-foreground" /> : <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />}
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
                  <span className="truncate font-medium text-foreground">{name}</span>
                )}
              </TreeItemContent>
            </TreeItem>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button type="button" className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground" onClick={(e) => e.stopPropagation()}>
                  <MoreHorizontal className="h-3 w-3" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent className="w-48" onClick={(e) => e.stopPropagation()}>
                <DropdownMenuItem onClick={() => void onNewRequest(collectionName, basePath)}><Plus className="h-3.5 w-3.5 mr-2" /> New Request</DropdownMenuItem>
                <DropdownMenuItem onClick={() => void onNewFolder(collectionName, basePath)}><FolderPlus className="h-3.5 w-3.5 mr-2" /> New Folder</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}><Pencil className="h-3.5 w-3.5 mr-2" /> Rename</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem className="text-destructive" onClick={() => onDelete({ type: 'folder', collection: collectionName, path: basePath, name })}><Trash2 className="h-3.5 w-3.5 mr-2" /> Delete</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuItem onClick={() => void onNewRequest(collectionName, basePath)}>New Request</ContextMenuItem>
          <ContextMenuItem onClick={() => void onNewFolder(collectionName, basePath)}>New Folder</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>Rename</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem className="text-destructive" onClick={() => onDelete({ type: 'folder', collection: collectionName, path: basePath, name })}>Delete</ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {open && (
        // Indentation guide line.
        <div className="pl-3 border-l border-border/30 ml-4">
          <DndContext
            collisionDetection={closestCenter}
            onDragStart={handleDragStart}
            onDragEnd={handleDragEnd}
            onDragCancel={handleDragCancel}
          >
            <SortableContext items={sortableIds} strategy={verticalListSortingStrategy}>
              {filteredItems.map((item) => {
                if (item.type === 'folder') {
                  const folderPath = `${basePath}/${item.name}`;
                  return (
                    <FolderNode
                      key={`folder-${folderPath}`}
                      name={item.name} items={item.items}
                      collectionName={collectionName} basePath={folderPath}
                      depth={depth + 1} filter={filter} summaries={summaries}
                      onNewRequest={onNewRequest} onNewFolder={onNewFolder}
                      onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
                    />
                  );
                }
                const fileName = item.fileName ?? item.name;
                const requestPath = `${basePath}/${fileName}`;
                return (
                  <RequestNode
                    key={item.uid}
                    uid={item.uid} name={item.name} method={item.method}
                    collectionName={collectionName} path={requestPath}
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
    </div>
  );
}
