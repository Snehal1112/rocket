import { MoreVertical } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useFlowNodeActions } from './FlowNodeActionsContext';

interface NodeMenuButtonProps {
  nodeId: string;
  label: string;
  /** When set, the button opens a menu with a debug mode toggle. */
  debug?: { enabled: boolean; onToggle: (enabled: boolean) => void };
}

// Without `debug`, opens the node's properties panel. With it, opens a menu.
// `nodrag nokey` keeps a click from dragging the node and keeps key presses on
// the button away from the canvas.
export function NodeMenuButton({ nodeId, label, debug }: NodeMenuButtonProps) {
  const { openProperties } = useFlowNodeActions();
  const trigger = (
    <Button
      type='button'
      variant='ghost'
      size='icon'
      aria-label={`Edit ${label}`}
      className='nodrag nokey ml-auto h-5 w-5 shrink-0 text-muted-foreground'
      onClick={debug ? undefined : () => openProperties(nodeId)}
    >
      <MoreVertical className='h-3.5 w-3.5' aria-hidden='true' />
    </Button>
  );

  if (!debug) return trigger;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
      {/* The menu is portalled out of the node, so without `nokey` a Backspace
          inside it would delete the selected node. */}
      <DropdownMenuContent className='nokey' align='end'>
        <DropdownMenuItem onSelect={() => openProperties(nodeId)}>Edit properties</DropdownMenuItem>
        <DropdownMenuCheckboxItem
          checked={debug.enabled}
          onCheckedChange={(value) => debug.onToggle(value === true)}
        >
          Debug mode
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
