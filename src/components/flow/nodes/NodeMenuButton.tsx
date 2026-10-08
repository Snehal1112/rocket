import { FastForward, MoreVertical, Play } from 'lucide-react';
import { useRef } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useFlowNodeActions } from './FlowNodeActionsContext';

interface NodeMenuButtonProps {
  nodeId: string;
  label: string;
  /** When set, the menu has a debug mode toggle. */
  debug?: { enabled: boolean; onToggle: (enabled: boolean) => void };
  /** A Wait for callback node cannot run on its own, so "Run this node" is disabled. */
  isWait?: boolean;
}

// Opens a menu when it has more than Edit properties: a debug toggle, or run
// items once the tab has a run to build on. Otherwise a click opens the
// node's properties panel directly. `nodrag nokey` keeps a click from
// dragging the node and keeps key presses on the button away from the canvas.
export function NodeMenuButton({ nodeId, label, debug, isWait = false }: NodeMenuButtonProps) {
  const { openProperties, duplicateNode, runNode, runBusy = false } = useFlowNodeActions();
  const hasMenu = debug !== undefined || runNode !== undefined;
  const openedProperties = useRef(false);
  const trigger = (
    <Button
      type='button'
      variant='ghost'
      size='icon'
      aria-label={`Edit ${label}`}
      className='nodrag nokey ml-auto h-5 w-5 shrink-0 text-muted-foreground'
      onClick={hasMenu ? undefined : () => openProperties(nodeId)}
    >
      <MoreVertical className='h-3.5 w-3.5' aria-hidden='true' />
    </Button>
  );

  if (!hasMenu) return trigger;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
      {/* The menu is portalled out of the node, so without `nokey` a Backspace
          inside it would delete the selected node. */}
      <DropdownMenuContent
        className='nokey'
        align='end'
        onCloseAutoFocus={(event) => {
          // Radix returns focus to the trigger after this. Open the panel only
          // now, so its focus request runs last and the trigger cannot steal it.
          if (!openedProperties.current) return;
          openedProperties.current = false;
          event.preventDefault();
          openProperties(nodeId);
        }}
      >
        <DropdownMenuItem
          onSelect={() => {
            openedProperties.current = true;
          }}
        >
          Edit properties
        </DropdownMenuItem>
        {duplicateNode && (
          <DropdownMenuItem onSelect={() => duplicateNode(nodeId)}>Duplicate</DropdownMenuItem>
        )}
        {runNode && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={runBusy || isWait} onSelect={() => runNode(nodeId, 'node')}>
              <Play className='h-3.5 w-3.5' aria-hidden='true' />
              Run this node
            </DropdownMenuItem>
            <DropdownMenuItem disabled={runBusy} onSelect={() => runNode(nodeId, 'fromHere')}>
              <FastForward className='h-3.5 w-3.5' aria-hidden='true' />
              Run from here
            </DropdownMenuItem>
          </>
        )}
        {debug && (
          <DropdownMenuCheckboxItem
            checked={debug.enabled}
            onCheckedChange={(value) => debug.onToggle(value === true)}
          >
            Debug mode
          </DropdownMenuCheckboxItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
