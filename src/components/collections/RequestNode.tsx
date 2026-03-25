import { useState } from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { MoreHorizontal, Copy, Trash2, GripVertical } from 'lucide-react';
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
import { Badge } from '@/components/ui/badge';
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

// Badge color classes per HTTP method (matches RequestList.tsx).
const METHOD_BADGE: Record<string, string> = {
  GET:     'text-emerald-500 border-emerald-500/30 bg-emerald-500/10',
  POST:    'text-amber-500   border-amber-500/30   bg-amber-500/10',
  PUT:     'text-blue-500    border-blue-500/30    bg-blue-500/10',
  PATCH:   'text-violet-500  border-violet-500/30  bg-violet-500/10',
  DELETE:  'text-red-500     border-red-500/30     bg-red-500/10',
  OPTIONS: 'text-cyan-500    border-cyan-500/30    bg-cyan-500/10',
  HEAD:    'text-pink-500    border-pink-500/30    bg-pink-500/10',
};

interface RequestNodeProps {
  uid: string;
  name: string;
  method: string;
  collectionName: string;
  path: string;
  itemData: Extract<CollectionItem, { type: 'request' }>;
  summaries: CollectionSummary[];
  filter: string;
  onMove: (srcCollection: string, srcPath: string, dstCollection: string, dstPath: string) => Promise<void>;
  onDelete: (target: DeleteTarget) => void;
  onDuplicate: (collection: string, path: string, name: string) => Promise<void>;
}

export function RequestNode({
  uid, name, method, collectionName, path, itemData,
  summaries, filter, onMove, onDelete, onDuplicate,
}: RequestNodeProps) {
  const root = usePaneStore((s) => s.root);
  const active = isActiveRequest(root, uid);
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);

  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: uid, disabled: !!filter });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

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
    const request: RequestState = mapApiRequestToState(itemData, true);
    const tab: RequestTab = {
      id: uid, title: name, tabType: 'request',
      request, response: null, isDirty: false,
      source: { collection: collectionName, path },
    };
    usePaneStore.getState().openTab(tab);
  }

  const badgeClass = METHOD_BADGE[method.toUpperCase()] ?? 'text-foreground border-border bg-muted';

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div ref={setNodeRef} style={style} className="group relative flex items-center">
          {/* Drag handle — grip icon, visible on hover. */}
          <button
            type="button"
            className="absolute left-0 h-full px-0.5 opacity-0 group-hover:opacity-100 cursor-grab active:cursor-grabbing text-muted-foreground"
            {...attributes}
            {...listeners}
            tabIndex={-1}
          >
            <GripVertical className="h-3 w-3" />
          </button>

          <TreeItem value={uid} className="flex-1">
            <TreeItemContent
              className={cn(
                'flex items-center gap-1.5 w-full px-2 pl-4 py-1 text-xs rounded-sm cursor-pointer',
                active && 'bg-accent/50 text-accent-foreground',
              )}
              onClick={handleClick}
              aria-label={`Open ${method} ${name}`}
            >
              <Badge variant="outline" className={cn('text-[10px] font-semibold w-14 justify-center shrink-0', badgeClass)}>
                {method}
              </Badge>
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
