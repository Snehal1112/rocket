import { Button } from '@/components/ui/button';

interface ChangelogEmptyStateProps {
  onReset: () => void;
}

export function ChangelogEmptyState({ onReset }: ChangelogEmptyStateProps) {
  return (
    <div className='flex flex-col items-center justify-center py-12 gap-2 text-center'>
      <p className='text-sm font-medium text-foreground'>No changes match</p>
      <p className='text-[12px] text-muted-foreground'>Try clearing filters or search</p>
      <Button variant='outline' size='sm' className='mt-2' onClick={onReset}>
        Reset filters
      </Button>
    </div>
  );
}
