import { useState, useEffect } from 'react';
import { Folder, FolderOpen, FolderPlus, Plus, Trash2, Pencil, MoreHorizontal } from 'lucide-react';
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
import { moveItem, saveRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { createDefaultRequest } from '@/lib/pane-utils';
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
  onNewFolder: (collection: string, folderPath: string) => Promise<void>;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function FolderNode({
  name, items, collectionName, basePath, depth, filter,
  summaries, onNewFolder, onMove, onDelete, onDuplicate,
}: FolderNodeProps) {
  const [open, setOpen] = useState(depth < 2);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const [creatingRequest, setCreatingRequest] = useState(false);
  const [newRequestName, setNewRequestName] = useState('');
  // Auto-expand when filter is active.
  useEffect(() => { if (filter) setOpen(true); }, [filter]);

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

  const handleNewRequestCreate = async () => {
    const reqName = newRequestName.trim();
    if (!reqName) { setCreatingRequest(false); return; }
    setCreatingRequest(false);
    try {
      const path = `${basePath}/${reqName}`;
      const uid = crypto.randomUUID();
      const payload = { uid, name: reqName, method: 'GET' as const, url: '', headers: [], auth: { authType: 'none' as const } };
      const saved = await saveRequest(collectionName, path, payload);
      usePaneStore.getState().openTab({
        id: uid,
        title: saved.name,
        tabType: 'request',
        request: createDefaultRequest(),
        response: null,
        isDirty: false,
        source: { collection: collectionName, path: saved.fileName ?? `${path}.yml` },
      });
    } catch (err) {
      console.error('[FolderNode] Failed to create request:', err);
    }
  };

  const filteredItems = filter
    ? items.filter((item) => item.type !== 'request' || item.name.toLowerCase().includes(filter.toLowerCase()))
    : items;

  if (filter && filteredItems.length === 0) return null;

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="group relative flex items-center">
            <TreeItem value={basePath} open={open} onOpenChange={setOpen} className="flex-1">
              <TreeItemContent
                className="flex items-center gap-1 w-full px-2 py-0.5 text-xs rounded-sm cursor-pointer"
                onClick={() => setOpen((prev) => !prev)}
              >
                {open ? <FolderOpen className="h-4 w-4 shrink-0 text-muted-foreground" /> : <Folder className="h-4 w-4 shrink-0 text-muted-foreground" />}
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
                <DropdownMenuItem onClick={() => { setOpen(true); setCreatingRequest(true); setNewRequestName(''); }}><Plus className="h-3.5 w-3.5 mr-2" /> New Request</DropdownMenuItem>
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
          <ContextMenuItem onClick={() => { setOpen(true); setCreatingRequest(true); setNewRequestName(''); }}>New Request</ContextMenuItem>
          <ContextMenuItem onClick={() => void onNewFolder(collectionName, basePath)}>New Folder</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>Rename</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem className="text-destructive" onClick={() => onDelete({ type: 'folder', collection: collectionName, path: basePath, name })}>Delete</ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {open && (
        // Indentation guide line.
        <div className="pl-1.5 border-l border-border/50 ml-2">
          {filteredItems.map((item) => {
            if (item.type === 'folder') {
              const folderPath = `${basePath}/${item.name}`;
              return (
                <FolderNode
                  key={`folder-${folderPath}`}
                  name={item.name} items={item.items}
                  collectionName={collectionName} basePath={folderPath}
                  depth={depth + 1} filter={filter} summaries={summaries}
                  onNewFolder={onNewFolder}
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
                itemData={item} summaries={summaries}
                onMove={onMove} onDelete={onDelete} onDuplicate={onDuplicate}
              />
            );
          })}
          {creatingRequest && (
            <div className="flex items-center gap-1 px-2 py-0.5 text-xs">
              <Input
                autoFocus
                className="h-5 text-xs flex-1"
                placeholder="Request name"
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
    </div>
  );
}
