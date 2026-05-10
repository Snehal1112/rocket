import { Lock, Upload } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface ContractsEmptyStateProps {
  onStartFromCurrent: () => void;
}

export function ContractsEmptyState({ onStartFromCurrent }: ContractsEmptyStateProps) {
  return (
    <div className='flex flex-col items-center justify-center flex-1 py-16 px-6 text-center'>
      <div
        className='w-[120px] h-[120px] rounded-full border-2 border-dashed border-primary/30 bg-[hsl(var(--primary)/0.06)] flex items-center justify-center mb-6'
        aria-hidden='true'
      >
        <Lock className='h-10 w-10 text-primary' />
      </div>
      <h2 className='text-[22px] font-semibold text-foreground mb-2'>Lock the shape of this API</h2>
      <p className='text-sm text-muted-foreground max-w-[420px] mb-6 leading-relaxed'>
        Pin endpoint signatures so your consumer team builds against a known shape. Rocket tracks
        every change after — you'll see breaking diffs before they ship.
      </p>
      <div className='flex flex-col items-center gap-3'>
        <Button size='default' onClick={onStartFromCurrent}>
          <Lock className='h-4 w-4 mr-2' aria-hidden='true' />
          Start from current state
        </Button>
        <Button variant='outline' size='default' disabled>
          <Upload className='h-4 w-4 mr-2' aria-hidden='true' />
          Import OpenAPI…
        </Button>
        <p className='text-xs text-muted-foreground/60 mt-1'>
          or snapshot only a folder / single request
        </p>
      </div>
    </div>
  );
}
