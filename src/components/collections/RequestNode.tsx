import {
  Copy,
  FolderInput,
  LayoutPanelLeft,
  MoreHorizontal,
  PanelBottom,
  PanelRight,
  Pencil,
  Trash2,
} from 'lucide-react';
import { useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
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
import {
  encodeFlowRequestDragPayload,
  FLOW_REQUEST_DRAG_MIME,
  FLOW_REQUEST_DRAG_TEXT_PREFIX,
} from '@/lib/flow-drag';
import {
  collectLeafGroupIds,
  findTabInTree,
  mapApiRequestToState,
  mapGraphQlToState,
} from '@/lib/pane-utils';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import { getGraphQlRequest, getRequest, getWebSocketRequest, renameRequest } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { mapWebSocketToState } from '@/lib/websocket-mapper';
import { useContractStore } from '@/stores/contract-store';
import { useContractsStore } from '@/stores/contracts/contractsSlice';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestState, RequestTab } from '@/types/pane-types';
import type { DeleteTarget } from './tree-utils';
import { isActiveRequest } from './tree-utils';

const EMPTY_CONTRACTS: import('@/lib/tauri-api').Contract[] = [];
const EMPTY_IDS: string[] = [];

// Module-level (not per-instance) counter shared by every RequestNode row, so a
// slow fetch from an earlier click on one row can detect that a later click on
// any row has superseded it, and skip stealing focus back when it resolves.
let latestOpenToken = 0;

interface RequestNodeProps {
  uid: string;
  name: string;
  method: string;
  collectionName: string;
  collectionRoot: string;
  path: string;
  itemData: Extract<CollectionItem, { type: 'request' } | { type: 'summary' }>;
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
  const kind = itemData.type === 'summary' ? (itemData.kind ?? 'http') : 'http';
  // The badge shows the protocol for GraphQL, the HTTP verb otherwise.
  const badge = kind === 'graphql' ? 'GQL' : kind === 'websocket' ? 'WS' : method;
  const root = usePaneStore((s) => s.root);
  const activeGroupId = usePaneStore((s) => s.activeGroupId);
  const openTab = usePaneStore((s) => s.openTab);
  const splitGroup = usePaneStore((s) => s.splitGroup);
  const active = isActiveRequest(root, uid);
  const contractsForScope = useContractStore((s) => s.contractsForScope);
  const scopedContracts = contractsForScope(collectionRoot, 'request', path) ?? EMPTY_CONTRACTS;

  // New store — contract status dot
  const newContractIds = useContractsStore((s) => s.byCollection[collectionRoot] ?? EMPTY_IDS);
  const newContractsById = useContractsStore((s) => s.byId);

  /**
   * Highest-severity contract status covering this request.
   * Priority: breach > drift > compliant > undefined (no contract covers it).
   */
  const contractDotStatus = useMemo(() => {
    let highestCompliant = false;
    for (const id of newContractIds) {
      const contract = newContractsById[id];
      if (!contract) continue;

      const covers =
        contract.scope.type === 'collection' ||
        (contract.scope.type === 'folder' && path.startsWith(contract.scope.rel_path)) ||
        (contract.scope.type === 'request' && contract.scope.rel_path === path);

      if (!covers) continue;

      if (contract.status === 'breach') return 'breach' as const;
      if (contract.status === 'drift') return 'drift' as const;
      if (contract.status === 'active' || contract.status === 'expiring_in_30_days') {
        highestCompliant = true;
      }
    }
    return highestCompliant ? ('compliant' as const) : undefined;
  }, [newContractIds, newContractsById, path]);

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

  // Builds a RequestTab, fetching full request data on demand if only a
  // lightweight summary (uid/name/method/fileName) was loaded from the sidebar.
  async function createTab(): Promise<RequestTab> {
    let request: RequestState;
    let tabId = uid;
    if (kind === 'graphql') {
      const loaded = await getGraphQlRequest(collectionName, path);
      // A file without a uid key has an empty summary uid; the loaded request carries a generated one.
      tabId = uid || loaded.uid;
      request = mapGraphQlToState(loaded);
    } else if (kind === 'websocket') {
      request = mapWebSocketToState(await getWebSocketRequest(collectionName, path));
    } else {
      const full = itemData.type === 'request' ? itemData : await getRequest(collectionName, path);
      request = mapApiRequestToState(full, true);
    }
    return {
      id: tabId,
      title: name,
      tabType: 'request',
      request,
      response: null,
      isDirty: false,
      source: { collection: collectionName, path },
    };
  }

  // Builds the tab and opens it in the given pane (or the active pane when omitted).
  // Swallows fetch failures so a malformed/deleted file can't crash the sidebar —
  // mirrors the existing error handling in CollectionNode's refreshTree.
  async function openInPane(groupId?: string) {
    // Claim the latest token before the await below, so a later click (on this row
    // or any other) can supersede this call while its fetch is still in flight.
    const token = ++latestOpenToken;
    // An already-open tab only needs focusing, so skip the IPC fetch entirely.
    // openTab recognises the existing tab by id and just activates it.
    const existing = findTabInTree(usePaneStore.getState().root, uid);
    if (existing) {
      openTab(existing.tab, groupId);
      return;
    }
    try {
      const tab = await createTab();
      // A newer click superseded this one while we were fetching. Drop the result
      // silently instead of stealing focus back to a request the user already left.
      if (token !== latestOpenToken) return;
      openTab(tab, groupId);
    } catch (err) {
      reportOpenFailure(err);
    }
  }

  // Logs a failed on-demand fetch and tells the user which request could not open.
  // Tauri rejects with a plain string, so non-Error values are stringified as-is.
  function reportOpenFailure(err: unknown) {
    console.error('[RequestNode] Failed to load request:', err);
    toast.error(`Could not open "${name}": ${err instanceof Error ? err.message : String(err)}`);
  }

  function handleClick() {
    if (isRenaming) return;
    void openInPane();
  }

  // Opens the tab in a new pane created by splitting in the given direction.
  // Fetches the tab data before splitting so a rejected on-demand fetch (deleted or
  // malformed file) never leaves the user with an empty split pane — split only happens
  // once we know we have a tab to put in it.
  async function openInSplit(direction: 'horizontal' | 'vertical') {
    // An already-open tab only needs focusing. Splitting first would leave the new
    // pane empty, since openTab re-activates the tab in its existing pane regardless.
    const existing = findTabInTree(usePaneStore.getState().root, uid);
    if (existing) {
      openTab(existing.tab);
      return;
    }
    try {
      const tab = await createTab();
      // Read panes fresh after the await, since they may have changed during the fetch.
      const current = usePaneStore.getState();
      const allCurrentIds = collectLeafGroupIds(current.root);
      splitGroup(current.activeGroupId, direction);
      const newRoot = usePaneStore.getState().root;
      const newIds = collectLeafGroupIds(newRoot);
      const newGroupId = newIds.find((id) => !allCurrentIds.includes(id));
      if (newGroupId) openTab(tab, newGroupId);
    } catch (err) {
      reportOpenFailure(err);
    }
  }

  const allLeafIds = collectLeafGroupIds(root);
  const otherGroupIds = allLeafIds.filter((id) => id !== activeGroupId);

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className='group relative flex items-center'>
          <TreeItem
            value={uid}
            active={active}
            className='flex-1'
            data-testid={`request-item-${badge}-${name}`}
            draggable={kind === 'http'}
            onDragStart={(e) => {
              const payload = encodeFlowRequestDragPayload({
                collection: collectionName,
                path,
                name,
                method,
              });
              e.dataTransfer.setData(FLOW_REQUEST_DRAG_MIME, payload);
              e.dataTransfer.setData('text/plain', `${FLOW_REQUEST_DRAG_TEXT_PREFIX}${payload}`);
              e.dataTransfer.effectAllowed = 'copy';
            }}
          >
            <TreeItemContent
              className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
              onClick={handleClick}
              aria-label={`Open ${badge} ${name}`}
            >
              <span
                className={cn(
                  'shrink-0 text-[10px] font-semibold uppercase px-1 py-0.5 rounded border',
                  METHOD_BADGE_COLOR[badge.toUpperCase()] ?? METHOD_BADGE_COLOR['GET'],
                )}
              >
                {badge}
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
              {contractDotStatus && (
                <span
                  role='img'
                  aria-label={`Contract status: ${contractDotStatus}`}
                  className={cn(
                    'ml-auto w-[7px] h-[7px] rounded-full shrink-0 inline-block',
                    contractDotStatus === 'breach' && 'bg-[hsl(var(--destructive))] animate-pulse',
                    contractDotStatus === 'drift' && 'bg-[hsl(var(--warning))]',
                    contractDotStatus === 'compliant' && 'bg-[hsl(var(--success)/0.8)]',
                  )}
                />
              )}
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
              {kind === 'http' && (
                <DropdownMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>
                  <Copy aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Duplicate
                </DropdownMenuItem>
              )}
              <DropdownMenuItem
                onClick={() => {
                  setRenameValue(name);
                  setIsRenaming(true);
                }}
              >
                <Pencil aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Rename
              </DropdownMenuItem>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>
                  <FolderInput aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Move to...
                </DropdownMenuSubTrigger>
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
                <DropdownMenuItem onClick={() => void openInPane(otherGroupIds[0])}>
                  <LayoutPanelLeft aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open in other
                  pane
                </DropdownMenuItem>
              )}
              {otherGroupIds.length > 1 && (
                <DropdownMenuSub>
                  <DropdownMenuSubTrigger>
                    <LayoutPanelLeft aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open in
                    other pane
                  </DropdownMenuSubTrigger>
                  <DropdownMenuSubContent className='w-48'>
                    {otherGroupIds.map((gid) => (
                      <DropdownMenuItem key={gid} onClick={() => void openInPane(gid)}>
                        Pane {allLeafIds.indexOf(gid) + 1}
                      </DropdownMenuItem>
                    ))}
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
              )}
              <DropdownMenuItem onClick={() => void openInSplit('horizontal')}>
                <PanelRight aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open to right
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => void openInSplit('vertical')}>
                <PanelBottom aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open below
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
        {kind === 'http' && (
          <ContextMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>
            <Copy aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Duplicate
          </ContextMenuItem>
        )}
        <ContextMenuItem
          onClick={() => {
            setRenameValue(name);
            setIsRenaming(true);
          }}
        >
          <Pencil aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Rename
        </ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>
            <FolderInput aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Move to...
          </ContextMenuSubTrigger>
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
          <ContextMenuItem onClick={() => void openInPane(otherGroupIds[0])}>
            <LayoutPanelLeft aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open in other pane
          </ContextMenuItem>
        )}
        {otherGroupIds.length > 1 && (
          <ContextMenuSub>
            <ContextMenuSubTrigger>
              <LayoutPanelLeft aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open in other pane
            </ContextMenuSubTrigger>
            <ContextMenuSubContent className='w-48'>
              {otherGroupIds.map((gid) => (
                <ContextMenuItem key={gid} onClick={() => void openInPane(gid)}>
                  Pane {allLeafIds.indexOf(gid) + 1}
                </ContextMenuItem>
              ))}
            </ContextMenuSubContent>
          </ContextMenuSub>
        )}
        <ContextMenuItem onClick={() => void openInSplit('horizontal')}>
          <PanelRight aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open to right
        </ContextMenuItem>
        <ContextMenuItem onClick={() => void openInSplit('vertical')}>
          <PanelBottom aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Open below
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem
          className='text-destructive'
          onClick={() => onDelete({ type: 'request', collection: collectionName, path, name })}
        >
          <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
