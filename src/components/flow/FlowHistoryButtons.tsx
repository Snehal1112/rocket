import { Redo2, Undo2 } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface FlowHistoryButtonsProps {
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
}

// Undo and Redo for the flow canvas. The shortcuts live on the canvas itself.
export function FlowHistoryButtons({ canUndo, canRedo, onUndo, onRedo }: FlowHistoryButtonsProps) {
  return (
    <div className='flex items-center gap-1'>
      <Button
        type='button'
        size='icon'
        variant='outline'
        className='h-8 w-8'
        aria-label='Undo'
        title='Undo (Ctrl+Z)'
        disabled={!canUndo}
        onClick={onUndo}
      >
        <Undo2 className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
      <Button
        type='button'
        size='icon'
        variant='outline'
        className='h-8 w-8'
        aria-label='Redo'
        title='Redo (Ctrl+Shift+Z)'
        disabled={!canRedo}
        onClick={onRedo}
      >
        <Redo2 className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
    </div>
  );
}
