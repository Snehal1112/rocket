import { Copy, MoreHorizontal, Trash2 } from 'lucide-react';
import { useRef, useState } from 'react';
import { ContractBadge } from '@/components/contract/ContractBadge';
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
import { useContractStore } from '@/stores/contract-store';
import { usePaneStore } from '@/stores/pane-store';
import type { PaneNode, RequestState, RequestTab } from '@/types/pane-types';
import type { DeleteTarget } from './tree-utils';
import { isActiveRequest } from './tree-utils';

const EMPTY_CONTRACTS: import('@/lib/tauri-api').Contract[] = [];

// Collects all leaf groupIds from the pane tree.
function collectLeafGroupIds(node: PaneNode): string[] {
  if (node.type === 'leaf') return [node.groupId];
  return [...collectLeafGroupIds(node.children[0]), ...collectLeafGroupIds(node.children[1])];
}

interface RequestNodeProps {
  uid: string;
  name: string;
  method: string;
  collectionName: string;
  collectionRoot: string;
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
  collectionRoot,
  path,
  itemData,
  summaries,
  onMove,
  onDelete,
  onDuplicate,
}: RequestNodeProps) {
  const root = usePaneStore((s) => s.root);
  const activeGroupId = usePaneStore((s) => s.activeGroupId);
  const openTab = usePaneStore((s) => s.openTab);
  const splitGroup = usePaneStore((s) => s.splitGroup);
  const active = isActiveRequest(root, uid);
  const contractsForScope = useContractStore((s) => s.contractsForScope);
  const scopedContracts = contractsForScope(collectionRoot, 'request', path) ?? EMPTY_CONTRACTS;
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

  // Builds a RequestTab without opening it.
  function createTab(): RequestTab {
    const request: RequestState = mapApiRequestToState(itemData, true);
    return {
      id: uid,
      title: name,
      tabType: 'request',
      request,
      response: null,
      isDirty: false,
      source: { collection: collectionName, path },
    };
  }

  function handleClick() {
    if (isRenaming) return;
    openTab(createTab());
  }

  // Opens the tab in a new pane created by splitting in the given direction.
  function openInSplit(direction: 'horizontal' | 'vertical') {
    const allCurrentIds = collectLeafGroupIds(root);
    splitGroup(activeGroupId, direction);
    const newRoot = usePaneStore.getState().root;
    const newIds = collectLeafGroupIds(newRoot);
    const newGroupId = newIds.find((id) => !allCurrentIds.includes(id));
    if (newGroupId) openTab(createTab(), newGroupId);
  }

  const allLeafIds = collectLeafGroupIds(root);
  const otherGroupIds = allLeafIds.filter((id) => id !== activeGroupId);

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
              <ContractBadge
                contracts={scopedContracts}
                collectionName={collectionName}
                collectionRoot={collectionRoot}
              />
            </TreeItemContent>
          </TreeItem>

          {/* "..." action menu, visible on hover. */}
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
              <DropdownMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>
                <Copy aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Duplicate
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
              {/* Pane-targeting actions. */}
              {otherGroupIds.length === 1 && (
                <DropdownMenuItem onClick={() => openTab(createTab(), otherGroupIds[0])}>
                  Open in other pane
                </DropdownMenuItem>
              )}
              {otherGroupIds.length > 1 && (
                <DropdownMenuSub>
                  <DropdownMenuSubTrigger>Open in other pane</DropdownMenuSubTrigger>
                  <DropdownMenuSubContent className='w-48'>
                    {otherGroupIds.map((gid) => (
                      <DropdownMenuItem key={gid} onClick={() => openTab(createTab(), gid)}>
                        Pane {allLeafIds.indexOf(gid) + 1}
                      </DropdownMenuItem>
                    ))}
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
              )}
              <DropdownMenuItem onClick={() => openInSplit('horizontal')}>
                Open to right
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => openInSplit('vertical')}>
                Open below
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className='text-destructive'
                onClick={() =>
                  onDelete({ type: 'request', collection: collectionName, path, name })
                }
              >
                <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
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
        {/* Pane-targeting actions. */}
        {otherGroupIds.length === 1 && (
          <ContextMenuItem onClick={() => openTab(createTab(), otherGroupIds[0])}>
            Open in other pane
          </ContextMenuItem>
        )}
        {otherGroupIds.length > 1 && (
          <ContextMenuSub>
            <ContextMenuSubTrigger>Open in other pane</ContextMenuSubTrigger>
            <ContextMenuSubContent className='w-48'>
              {otherGroupIds.map((gid) => (
                <ContextMenuItem key={gid} onClick={() => openTab(createTab(), gid)}>
                  Pane {allLeafIds.indexOf(gid) + 1}
                </ContextMenuItem>
              ))}
            </ContextMenuSubContent>
          </ContextMenuSub>
        )}
        <ContextMenuItem onClick={() => openInSplit('horizontal')}>Open to right</ContextMenuItem>
        <ContextMenuItem onClick={() => openInSplit('vertical')}>Open below</ContextMenuItem>
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
