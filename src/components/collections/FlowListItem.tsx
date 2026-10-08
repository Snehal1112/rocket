import { useQueryClient } from '@tanstack/react-query';
import { MoreHorizontal, Pencil, Trash2, Workflow } from 'lucide-react';
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
import { flowAuthKeyMatches } from '@/lib/flow-auth';
import { validateFlowName } from '@/lib/flow-name';
import { isFlowRunning } from '@/lib/flow-tabs';
import { flowKeys } from '@/lib/queries/flow-queries';
import { renameFlow } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import { usePaneStore } from '@/stores/pane-store';
import type { DeleteTarget } from './tree-utils';

interface FlowListItemProps {
  name: string;
  collectionName: string;
  onDelete: (target: DeleteTarget) => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

// True while any tab of this flow is running, also one parked after a collection switch.
function flowIsRunning(collectionName: string, name: string): boolean {
  const state = usePaneStore.getState();
  return isFlowRunning(state.root, state.collectionTabState, collectionName, name);
}

// True when the flow still holds in-memory Auth tokens.
function holdsAuthTokens(collectionName: string, name: string): boolean {
  return Object.keys(useFlowAuthStore.getState().auths).some((key) =>
    flowAuthKeyMatches(key, collectionName, name),
  );
}

export function FlowListItem({ name, collectionName, onDelete }: FlowListItemProps) {
  const queryClient = useQueryClient();
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const renameInFlight = useRef(false);
  // Set on Escape to block the blur that fires when the Input unmounts.
  const renameCancelled = useRef(false);

  const open = async () => {
    if (isRenaming) return;
    try {
      await usePaneStore.getState().openFlowTab(collectionName, name);
    } catch (err) {
      toast.error(`Could not open "${name}": ${errorMessage(err)}`);
    }
  };

  const stopRunMessage = (verb: string) => `Stop the run of "${name}" before ${verb} it.`;

  const requestDelete = () => {
    if (flowIsRunning(collectionName, name)) {
      toast.error(stopRunMessage('deleting'));
      return;
    }
    onDelete({ type: 'flow', collection: collectionName, name });
  };

  const startRename = () => {
    if (flowIsRunning(collectionName, name)) {
      toast.error(stopRunMessage('renaming'));
      return;
    }
    setRenameValue(name);
    // Wait for the menu to release focus, or the input blurs at once.
    setTimeout(() => setIsRenaming(true), 0);
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
    const problem = validateFlowName(trimmed);
    if (problem) {
      toast.error(problem);
      setIsRenaming(false);
      return;
    }
    // A run can start while the name is typed, so check again before touching anything.
    if (flowIsRunning(collectionName, name)) {
      toast.error(stopRunMessage('renaming'));
      setIsRenaming(false);
      return;
    }
    renameInFlight.current = true;
    const hadTokens = holdsAuthTokens(collectionName, name);
    try {
      await renameFlow(collectionName, name, trimmed);
      // Only after the backend succeeded: open tabs follow the new name and the old tokens go.
      usePaneStore.getState().renameFlowTabs(collectionName, name, trimmed);
      void queryClient.invalidateQueries({ queryKey: flowKeys.collection(collectionName) });
      if (hadTokens) {
        toast.info(
          `Sign-in tokens for "${name}" were cleared. Authenticate again before the next run.`,
        );
      }
    } catch (err) {
      toast.error(`Could not rename "${name}": ${errorMessage(err)}`);
    } finally {
      renameInFlight.current = false;
      setIsRenaming(false);
    }
  };

  return (
    <div className='group relative flex items-center'>
      <TreeItem value={JSON.stringify(['flow', collectionName, name])} className='w-full'>
        <TreeItemContent
          className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
          onClick={() => void open()}
          aria-label={`Open flow ${name}`}
        >
          <Workflow aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
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
          <DropdownMenuItem onSelect={startRename}>
            <Pencil aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Rename
          </DropdownMenuItem>
          <DropdownMenuItem className='text-destructive' onClick={requestDelete}>
            <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
