import { Copy, MoreHorizontal, Trash2 } from 'lucide-react';
import { useRef, useState } from 'react';
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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { METHOD_BADGE_COLOR } from '@/lib/colors';
import { mapApiRequestToState } from '@/lib/pane-utils';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import { renameRequest } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestState, RequestTab } from '@/types/pane-types';
import type { DeleteTarget } from './tree-utils';
import { isActiveRequest } from './tree-utils';

interface RequestNodeProps {
  uid: string;
  name: string;
  method: string;
  collectionName: string;
  path: string;
  itemData: Extract<CollectionItem, { type: 'request' }>;
  summaries: CollectionSummary[];
  onMove: (
    srcCollection: string,
    srcPath: string,
    dstCollection: string,
    dstPath: string,
  ) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function RequestNode({
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
}: RequestNodeProps) {
  const root = usePaneStore((s) => s.root);
  const active = isActiveRequest(root, uid);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const renameInFlight = useRef(false);

  const handleRename = async () => {
    if (renameInFlight.current) return;
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) {
      setIsRenaming(false);
      return;
    }
    renameInFlight.current = true;
    try {
      await renameRequest(collectionName, path, trimmed);
    } catch (err) {
      console.error('Rename request failed:', err);
    } finally {
      renameInFlight.current = false;
      setIsRenaming(false);
    }
  };

  function handleClick() {
    if (isRenaming) return;
    const request: RequestState = mapApiRequestToState(itemData, true);
    const tab: RequestTab = {
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
        <div className='group relative flex items-center'>
          <TreeItem value={uid} active={active} className='flex-1'>
            <TreeItemContent
              className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
              onClick={handleClick}
              aria-label={`Open ${method} ${name}`}
            >
              <span
                className={cn(
                  'shrink-0 text-[10px] font-semibold uppercase px-1 py-0.5 rounded border',
                  METHOD_BADGE_COLOR[method.toUpperCase()] ?? METHOD_BADGE_COLOR['GET'],
                )}
              >
                {method}
              </span>
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
                <span className='truncate text-foreground'>{name}</span>
              )}
            </TreeItemContent>
          </TreeItem>

          {/* "..." action menu, visible on hover. */}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type='button'
                aria-label={`Actions for ${name}`}
                className='absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-muted text-muted-foreground'
                onClick={(e) => e.stopPropagation()}
              >
                <MoreHorizontal className='h-3 w-3' />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent className='w-48' onClick={(e) => e.stopPropagation()}>
              <DropdownMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>
                <Copy className='h-3.5 w-3.5 mr-2' /> Duplicate
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => {
                  setRenameValue(name);
                  setIsRenaming(true);
                }}
              >
                Rename
              </DropdownMenuItem>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>Move to...</DropdownMenuSubTrigger>
                <DropdownMenuSubContent className='w-48'>
                  {summaries.map((s) => (
                    <DropdownMenuItem
                      key={s.name}
                      onClick={() => void onMove(collectionName, path, s.name, '')}
                      disabled={s.name === collectionName}
                    >
                      {s.name}
                    </DropdownMenuItem>
                  ))}
                  {summaries.length === 0 && (
                    <DropdownMenuItem disabled>No collections</DropdownMenuItem>
                  )}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className='text-destructive'
                onClick={() =>
                  onDelete({ type: 'request', collection: collectionName, path, name })
                }
              >
                <Trash2 className='h-3.5 w-3.5 mr-2' /> Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>

      {/* Right-click context menu — same actions, power-user shortcut. */}
      <ContextMenuContent className='w-48'>
        <ContextMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>
          Duplicate
        </ContextMenuItem>
        <ContextMenuItem
          onClick={() => {
            setRenameValue(name);
            setIsRenaming(true);
          }}
        >
          Rename
        </ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>Move to...</ContextMenuSubTrigger>
          <ContextMenuSubContent className='w-48'>
            {summaries.map((s) => (
              <ContextMenuItem
                key={s.name}
                onClick={() => void onMove(collectionName, path, s.name, '')}
                disabled={s.name === collectionName}
              >
                {s.name}
              </ContextMenuItem>
            ))}
            {summaries.length === 0 && <ContextMenuItem disabled>No collections</ContextMenuItem>}
          </ContextMenuSubContent>
        </ContextMenuSub>
        <ContextMenuSeparator />
        <ContextMenuItem
          className='text-destructive'
          onClick={() => onDelete({ type: 'request', collection: collectionName, path, name })}
        >
          Delete
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
