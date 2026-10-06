import { FileCode, MoreHorizontal, Pencil, Trash2 } from 'lucide-react';
import { useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { renameScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { DeleteTarget } from './tree-utils';

interface ScriptNodeProps {
  name: string;
  collectionName: string;
  /** Collection-relative path, for example `lib/utils.js`. */
  path: string;
  onDelete: (target: DeleteTarget) => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function ScriptNode({ name, collectionName, path, onDelete }: ScriptNodeProps) {
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const renameInFlight = useRef(false);
  // Set on Escape to block the blur that fires when the Input unmounts.
  const renameCancelled = useRef(false);

  const open = async () => {
    if (isRenaming) return;
    try {
      await usePaneStore.getState().openScriptTab(collectionName, path);
    } catch (err) {
      toast.error(`Could not open "${name}": ${errorMessage(err)}`);
    }
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
    renameInFlight.current = true;
    try {
      const newPath = await renameScriptFile(collectionName, path, trimmed);
      usePaneStore.getState().renameScriptTabs(collectionName, path, newPath);
    } catch (err) {
      toast.error(`Could not rename "${name}": ${errorMessage(err)}`);
    } finally {
      renameInFlight.current = false;
      setIsRenaming(false);
    }
  };

  return (
    <div className='group relative flex items-center'>
      <TreeItem value={`script-${collectionName}-${path}`} className='w-full'>
        <TreeItemContent
          className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
          onClick={() => void open()}
          aria-label={`Open script ${name}`}
        >
          <FileCode aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          {isRenaming ? (
            <Input
              autoFocus
              className='h-6 text-sm flex-1'
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
        </TreeItemContent>
      </TreeItem>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            aria-label={`Actions for ${name}`}
            className='absolute right-1 h-5 w-5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal aria-hidden='true' className='h-3 w-3' />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          className='w-48'
          onClick={(e) => e.stopPropagation()}
          // Keeps the rename input focused instead of returning focus to the trigger.
          onCloseAutoFocus={(e) => e.preventDefault()}
        >
          <DropdownMenuItem
            onSelect={() => {
              setRenameValue(name);
              // Wait for the menu to release focus, or the input blurs at once.
              setTimeout(() => setIsRenaming(true), 0);
            }}
          >
            <Pencil aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Rename
          </DropdownMenuItem>
          <DropdownMenuItem
            className='text-destructive'
            onClick={() => onDelete({ type: 'script', collection: collectionName, path, name })}
          >
            <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
