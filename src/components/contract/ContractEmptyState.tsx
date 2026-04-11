import { Lock, Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface ContractEmptyStateProps {
  onNew: () => void;
}

export function ContractEmptyState({ onNew }: ContractEmptyStateProps) {
  return (
    <div className='flex flex-col items-center justify-center py-24 gap-4 text-center'>
      <div className='w-12 h-12 rounded-xl bg-primary/10 flex items-center justify-center'>
        <Lock className='h-6 w-6 text-primary/50' />
      </div>
      <div className='space-y-1.5 max-w-xs'>
        <p className='text-sm font-medium text-foreground'>No contracts yet</p>
        <p className='text-xs text-muted-foreground'>
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
