import { useState } from 'react';
import { MoreHorizontal, Copy, Trash2 } from 'lucide-react';
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
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { cn } from '@/lib/utils';
import { renameRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { mapApiRequestToState } from '@/lib/pane-utils';
import { isActiveRequest } from './tree-utils';
import type { CollectionItem, CollectionSummary } from '@/lib/tauri-api';
import type { RequestTab, RequestState } from '@/types/pane-types';
import type { DeleteTarget } from './tree-utils';

// Text color per HTTP method.
const METHOD_COLOR: Record<string, string> = {
  GET:     'text-emerald-500',
  POST:    'text-amber-500',
  PUT:     'text-blue-500',
  PATCH:   'text-violet-500',
  DELETE:  'text-red-500',
  OPTIONS: 'text-cyan-500',
  HEAD:    'text-pink-500',
};

interface RequestNodeProps {
  uid: string;
  name: string;
  method: string;
  collectionName: string;
  path: string;
  itemData: Extract<CollectionItem, { type: 'request' }>;
  summaries: CollectionSummary[];
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function RequestNode({
  uid, name, method, collectionName, path, itemData,
  summaries, onMove, onDelete, onDuplicate,
}: RequestNodeProps) {
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
    if (isRenaming) return;
    const request: RequestState = mapApiRequestToState(itemData, true);
    const tab: RequestTab = {
      id: uid, title: name, tabType: 'request',
      request, response: null, isDirty: false,
      source: { collection: collectionName, path },
    };
    usePaneStore.getState().openTab(tab);
  }

  const methodColor = METHOD_COLOR[method.toUpperCase()] ?? 'text-foreground';

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div className="group relative flex items-center">
          <TreeItem value={uid} active={active} className="flex-1">
            <TreeItemContent
              className="flex items-center gap-1 w-full px-2 py-0.5 text-xs rounded-sm cursor-pointer"
              onClick={handleClick}
              aria-label={`Open ${method} ${name}`}
            >
              <span className={cn('w-10 shrink-0 font-mono text-2xs font-bold', methodColor)}>
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
            </TreeItemContent>
          </TreeItem>

          {/* "..." action menu, visible on hover. */}
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
              <DropdownMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>
                <Copy className="h-3.5 w-3.5 mr-2" /> Duplicate
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>
                Rename
              </DropdownMenuItem>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>Move to...</DropdownMenuSubTrigger>
                <DropdownMenuSubContent className="w-48">
                  {summaries.map((s) => (
                    <DropdownMenuItem
                      key={s.name}
                      onClick={() => void onMove(collectionName, path, s.name, '')}
                      disabled={s.name === collectionName}
                    >
                      {s.name}
                    </DropdownMenuItem>
                  ))}
                  {summaries.length === 0 && <DropdownMenuItem disabled>No collections</DropdownMenuItem>}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                className="text-destructive"
                onClick={() => onDelete({ type: 'request', collection: collectionName, path, name })}
              >
                <Trash2 className="h-3.5 w-3.5 mr-2" /> Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>

      {/* Right-click context menu — same actions, power-user shortcut. */}
      <ContextMenuContent className="w-48">
        <ContextMenuItem onClick={() => void onDuplicate(collectionName, path, name)}>Duplicate</ContextMenuItem>
        <ContextMenuItem onClick={() => { setRenameValue(name); setIsRenaming(true); }}>Rename</ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>Move to...</ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-48">
            {summaries.map((s) => (
              <ContextMenuItem key={s.name} onClick={() => void onMove(collectionName, path, s.name, '')} disabled={s.name === collectionName}>
                {s.name}
              </ContextMenuItem>
            ))}
            {summaries.length === 0 && <ContextMenuItem disabled>No collections</ContextMenuItem>}
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
