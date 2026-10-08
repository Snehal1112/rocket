import { MoreHorizontal, Trash2, Workflow } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { isFlowRunning } from '@/lib/flow-tabs';
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

export function FlowListItem({ name, collectionName, onDelete }: FlowListItemProps) {
  const open = async () => {
    try {
      await usePaneStore.getState().openFlowTab(collectionName, name);
    } catch (err) {
      toast.error(`Could not open "${name}": ${errorMessage(err)}`);
    }
  };

  const requestDelete = () => {
    if (flowIsRunning(collectionName, name)) {
      toast.error(`Stop the run of "${name}" before deleting it.`);
      return;
    }
    onDelete({ type: 'flow', collection: collectionName, name });
  };

  return (
    <div className='group relative flex items-center'>
      <TreeItem value={`flow-${collectionName}-${name}`} className='w-full'>
        <TreeItemContent
          className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
          onClick={() => void open()}
          aria-label={`Open flow ${name}`}
        >
          <Workflow aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          <span className='truncate text-foreground'>{name}</span>
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
        <DropdownMenuContent className='w-48' onClick={(e) => e.stopPropagation()}>
          <DropdownMenuItem className='text-destructive' onClick={requestDelete}>
            <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
