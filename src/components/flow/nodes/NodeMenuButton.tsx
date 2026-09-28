import { MoreVertical } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useFlowNodeActions } from './FlowNodeActionsContext';

// Opens the node's properties panel. `nodrag nokey` keeps a click from
// dragging the node and keeps key presses on the button away from the canvas.
export function NodeMenuButton({ nodeId, label }: { nodeId: string; label: string }) {
  const { openProperties } = useFlowNodeActions();
  return (
    <Button
      type='button'
      variant='ghost'
      size='icon'
      aria-label={`Edit ${label}`}
      className='nodrag nokey ml-auto h-5 w-5 shrink-0 text-muted-foreground'
      onClick={() => openProperties(nodeId)}
    >
      <MoreVertical className='h-3.5 w-3.5' aria-hidden='true' />
    </Button>
  );
}
