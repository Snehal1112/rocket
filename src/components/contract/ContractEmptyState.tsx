import { Plus } from 'lucide-react';
import { RocketLock } from '@/components/illustrations';
import { Button } from '@/components/ui/button';

interface ContractEmptyStateProps {
  onNew: () => void;
}

export function ContractEmptyState({ onNew }: ContractEmptyStateProps) {
  return (
    <div className='flex flex-col items-center justify-center h-full gap-6 text-center px-8 bg-gradient-to-b from-background to-muted/20'>
      <RocketLock className='w-36 h-36 opacity-90' />
      <div className='space-y-2 max-w-xs'>
        <p className='text-base font-semibold tracking-tight text-foreground'>No contracts yet</p>
        <p className='text-sm text-muted-foreground leading-relaxed'>
          Attach a contract to lock this collection's API signature and automatically track any
          changes made after signing.
        </p>
      </div>
      <Button size='sm' onClick={onNew}>
        <Plus className='h-3.5 w-3.5 mr-1.5' />
        New contract
      </Button>
    </div>
  );
}
