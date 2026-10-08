import {
  ChevronRight,
  FileCode,
  Folder,
  FolderOpen,
  FolderPlus,
  MoreHorizontal,
  Pencil,
  Plus,
  Settings,
  Trash2,
  Variable,
} from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { NewScriptDialog } from '@/components/collections/NewScriptDialog';
import { ScriptNode } from '@/components/collections/ScriptNode';
import { ContractBadge } from '@/components/contract/ContractBadge';
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
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import { moveItem, saveRequest } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { useContractStore } from '@/stores/contract-store';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderSection } from '@/types/pane-types';
import { RequestNode } from './RequestNode';
import type { DeleteTarget } from './tree-utils';

const EMPTY_CONTRACTS: import('@/lib/tauri-api').Contract[] = [];

interface FolderNodeProps {
  name: string;
  items: CollectionItem[];
  collectionName: string;
  collectionRoot: string;
  basePath: string;
  depth: number;
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

export function FolderNode({
  name,
  items,
  collectionName,
  collectionRoot,
  basePath,
  depth,
  filter,
  summaries,
  onNewFolder,
  onMove,
  onDelete,
  onDuplicate,
}: FolderNodeProps) {
  const contractsForScope = useContractStore((s) => s.contractsForScope);
  const scopedContracts = contractsForScope(collectionRoot, 'folder', basePath) ?? EMPTY_CONTRACTS;

  const [open, setOpen] = useState(false);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const [creatingRequest, setCreatingRequest] = useState(false);
  const [newRequestName, setNewRequestName] = useState('');
  const [newScriptOpen, setNewScriptOpen] = useState(false);
  const renameInFlight = useRef(false);
  // Set to true on Escape or after a successful rename to block the
  // blur event that fires when the Input unmounts.
  const renameCancelled = useRef(false);

  // Auto-expand when filter is active.
  useEffect(() => {
    if (filter) setOpen(true);
  }, [filter]);

  // Opens or focuses this folder's settings tab. A given section switches the tab to it.
  const openSettings = (section?: FolderSection) => {
    usePaneStore.getState().openFolderTab(collectionName, basePath, section);
  };

  const handleRename = async () => {
    if (renameInFlight.current) return;
    if (renameCancelled.current) {
      renameCancelled.current = false;
      return;
    }
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) {
      setIsRenaming(false);
      return;
    }
    // Folder rename is done by moving the folder to a new path (no rename_folder command).
    const parts = basePath.split('/');
    parts[parts.length - 1] = trimmed;
    const newPath = parts.join('/');
    renameInFlight.current = true;
    try {
      await moveItem(collectionName, basePath, collectionName, newPath);
      usePaneStore.getState().renameScriptTabs(collectionName, basePath, newPath);
      usePaneStore.getState().renameFolderTabs(collectionName, basePath, newPath);
      // Prevent the blur (fired when Input unmounts) from triggering a second rename.
      renameCancelled.current = true;
    } catch (err) {
      console.error('Rename folder failed:', err);
    } finally {
      renameInFlight.current = false;
      setIsRenaming(false);
    }
  };

  const handleNewRequestCreate = async () => {
    const reqName = newRequestName.trim();
    if (!reqName) {
      setCreatingRequest(false);
      return;
    }
    setCreatingRequest(false);
    try {
      const path = `${basePath}/${reqName}`;
      const uid = crypto.randomUUID();
      const payload = {
        uid,
        name: reqName,
        method: 'GET' as const,
        url: '',
        headers: [],
        auth: { authType: 'none' as const },
      };
      const saved = await saveRequest(collectionName, path, payload);
      usePaneStore.getState().openTab({
        id: uid,
        title: saved.name,
        tabType: 'request',
        request: createDefaultRequest(),
        response: null,
        isDirty: false,
        source: {
          collection: collectionName,
          path: saved.fileName ?? `${path}.yml`,
        },
      });
    } catch (err) {
      console.error('[FolderNode] Failed to create request:', err);
    }
  };

  // Opaque and typed GraphQL/WebSocket/gRPC full-tree items never render here: the
  // sidebar loads summaries, where GraphQL arrives as a `summary` with
  // `kind: 'graphql'`. They must never keep an otherwise-empty folder visible under an
  // active filter. Script files do render, so they stay.
  const filterableItems = items.filter(
    (item) =>
      item.type !== 'opaque' &&
      item.type !== 'graphql' &&
      item.type !== 'websocket' &&
      item.type !== 'grpc',
  );
  const filteredItems = sortItemsFoldersFirst(
    filter
      ? filterableItems.filter(
          (item) =>
            (item.type !== 'request' && item.type !== 'summary' && item.type !== 'scriptFile') ||
            item.name.toLowerCase().includes(filter.toLowerCase()),
        )
      : filterableItems,
  );

  if (filter && filteredItems.length === 0) return null;

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className='group relative flex items-center'>
            <TreeItem value={basePath} open={open} onOpenChange={setOpen} className='flex-1'>
              <TreeItemContent
                className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
                onClick={() => {
                  // A row click opens the settings and expands. Only the chevron collapses.
                  openSettings();
                  setOpen(true);
                }}
              >
                {/* biome-ignore lint/a11y/useSemanticElements: nested inside TreeItem's <button> row, HTML forbids button-in-button */}
                <span
                  role='button'
                  tabIndex={0}
                  aria-label={`${open ? 'Collapse' : 'Expand'} ${name}`}
                  aria-expanded={open}
                  className='flex h-4 w-4 shrink-0 items-center justify-center rounded-sm hover:bg-accent'
                  onClick={(e) => {
                    e.stopPropagation();
                    setOpen((prev) => !prev);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      e.stopPropagation();
                      setOpen((prev) => !prev);
                    }
                  }}
                >
                  <ChevronRight
                    aria-hidden='true'
                    className={cn('h-3 w-3 transition-transform', open && 'rotate-90')}
                  />
                </span>
                {open ? (
                  <FolderOpen aria-hidden='true' strokeWidth={10} className='h-5 w-5 shrink-0' />
                ) : (
                  <Folder aria-hidden='true' strokeWidth={10} className='h-5 w-5 shrink-0 ' />
                )}
                {isRenaming ? (
                  <Input
                    autoFocus
                    className='h-6 text-xs flex-1'
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') void handleRename();
                      if (e.key === 'Escape') {
                        renameCancelled.current = true;
                        setIsRenaming(false);
                      }
                    }}
                    onBlur={() => void handleRename()}
                    onClick={(e) => e.stopPropagation()}
                  />
                ) : (
                  <span className='truncate text-foreground'>{name}</span>
                )}
                <ContractBadge
                  contracts={scopedContracts}
                  collectionName={collectionName}
                  collectionRoot={collectionRoot}
                />
              </TreeItemContent>
            </TreeItem>

            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  type='button'
                  aria-label={`Actions for ${name}`}
                  className='absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-muted text-foreground'
                  onClick={(e) => e.stopPropagation()}
                >
                  <MoreHorizontal aria-hidden='true' className='h-3 w-3' />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent className='w-48' onClick={(e) => e.stopPropagation()}>
                <DropdownMenuItem
                  onClick={() => {
                    setOpen(true);
                    setCreatingRequest(true);
                    setNewRequestName('');
                  }}
                >
                  <Plus className='h-3.5 w-3.5 mr-2' /> New Request
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => void onNewFolder(collectionName, basePath)}>
                  <FolderPlus className='h-3.5 w-3.5 mr-2' /> New Folder
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => setNewScriptOpen(true)}>
                  <FileCode aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> New Script
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => openSettings()}>
                  <Settings aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Settings
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => openSettings('vars')}>
                  <Variable className='h-3.5 w-3.5 mr-2' /> Variables
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={() => {
                    renameCancelled.current = false;
                    setRenameValue(name);
                    setIsRenaming(true);
                  }}
                >
                  <Pencil className='h-3.5 w-3.5 mr-2' /> Rename
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  className='text-destructive'
                  onClick={() =>
                    onDelete({
                      type: 'folder',
                      collection: collectionName,
                      path: basePath,
                      name,
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
              setOpen(true);
              setCreatingRequest(true);
              setNewRequestName('');
            }}
          >
            <Plus className='h-3.5 w-3.5 mr-2' /> New Request
          </ContextMenuItem>
          <ContextMenuItem onClick={() => void onNewFolder(collectionName, basePath)}>
            <FolderPlus className='h-3.5 w-3.5 mr-2' /> New Folder
          </ContextMenuItem>
          <ContextMenuItem onClick={() => setNewScriptOpen(true)}>
            <FileCode aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> New Script
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onClick={() => openSettings()}>
            <Settings aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Settings
          </ContextMenuItem>
          <ContextMenuItem onClick={() => openSettings('vars')}>
            <Variable className='h-3.5 w-3.5 mr-2' /> Variables
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            onClick={() => {
              renameCancelled.current = false;
              setRenameValue(name);
              setIsRenaming(true);
            }}
          >
            <Pencil className='h-3.5 w-3.5 mr-2' /> Rename
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            className='text-destructive'
            onClick={() =>
              onDelete({
                type: 'folder',
                collection: collectionName,
                path: basePath,
                name,
              })
            }
          >
            <Trash2 className='h-3.5 w-3.5 mr-2' /> Delete
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {open && (
        // Indentation guide line.
        <div className='pl-1.5 border-l border-border/70 ml-2'>
          {filteredItems.map((item) => {
            if (item.type === 'folder') {
              const folderDirName = item.dirName ?? item.name;
              const folderPath = `${basePath}/${folderDirName}`;
              return (
                <FolderNode
                  key={`folder-${folderPath}`}
                  name={item.name}
                  items={item.items}
                  collectionName={collectionName}
                  collectionRoot={collectionRoot}
                  basePath={folderPath}
                  depth={depth + 1}
                  filter={filter}
                  summaries={summaries}
                  onNewFolder={onNewFolder}
                  onMove={onMove}
                  onDelete={onDelete}
                  onDuplicate={onDuplicate}
                />
              );
            }
            if (item.type === 'scriptFile') {
              return (
                <ScriptNode
                  key={`script-${basePath}/${item.fileName}`}
                  name={item.name}
                  collectionName={collectionName}
                  path={`${basePath}/${item.fileName}`}
                  onDelete={onDelete}
                />
              );
            }
            if (
              item.type === 'opaque' ||
              item.type === 'graphql' ||
              item.type === 'websocket' ||
              item.type === 'grpc'
            )
              return null;
            const fileName = item.fileName ?? item.name;
            const requestPath = `${basePath}/${fileName}`;
            return (
              <RequestNode
                key={`request-${requestPath}`}
                uid={item.uid}
                name={item.name}
                method={item.method}
                collectionName={collectionName}
                collectionRoot={collectionRoot}
                path={requestPath}
                itemData={item}
                summaries={summaries}
                onMove={onMove}
                onDelete={onDelete}
                onDuplicate={onDuplicate}
              />
            );
          })}
          {creatingRequest && (
            <div className='flex items-center gap-1 px-2 py-1 text-xs'>
              <Input
                autoFocus
                className='h-5 text-xs flex-1'
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
      <NewScriptDialog
        open={newScriptOpen}
        collectionName={collectionName}
        folderPath={basePath}
        onClose={() => setNewScriptOpen(false)}
      />
    </div>
  );
}
