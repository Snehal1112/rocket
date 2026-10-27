import { Search } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';

interface OpenApiSyncActionsBarProps {
  query: string;
  onQueryChange: (query: string) => void;
  /** Requests and variables the spec changed. */
  updateCount: number;
  /** Selected requests and variables. */
  selectedCount: number;
  busy: boolean;
  /** True while the plan is a placeholder, so selecting would act on stale rows. */
  locked: boolean;
  onConfirm: () => void;
  onAcceptAll: () => void;
  onSkipAll: () => void;
  onKeepCurrent: () => void;
}

/** Search, the update summary and the bulk actions: Accept all, Skip all, Keep current, Confirm. */
export function OpenApiSyncActionsBar({
  query,
  onQueryChange,
  updateCount,
  selectedCount,
  busy,
  locked,
  onConfirm,
  onAcceptAll,
  onSkipAll,
  onKeepCurrent,
}: OpenApiSyncActionsBarProps) {
  const bulkOff = busy || locked || updateCount === 0;
  return (
    <div className='flex flex-wrap items-center gap-2 border-b border-border/70 px-4 py-2'>
      <div className='relative w-56'>
        <Search className='pointer-events-none absolute left-2 top-2.5 size-4 text-muted-foreground' />
        <Input
          type='search'
          aria-label='Search changes'
          placeholder='Search changes'
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          className='h-9 pl-8'
        />
      </div>
      <span className='text-xs text-muted-foreground'>
        {`${updateCount} ${updateCount === 1 ? 'update' : 'updates'} from the spec · ${selectedCount} selected`}
      </span>
      <div className='ml-auto flex flex-wrap items-center gap-2'>
        <Button variant='outline' size='sm' onClick={onAcceptAll} disabled={bulkOff}>
          Accept all
        </Button>
        <Button variant='outline' size='sm' onClick={onSkipAll} disabled={bulkOff}>
          Skip all
        </Button>
        <Button variant='outline' size='sm' onClick={onKeepCurrent} disabled={busy || locked}>
          Keep current
        </Button>
        <Button size='sm' onClick={onConfirm} disabled={busy || selectedCount === 0}>
          {`Confirm and sync (${selectedCount})`}
        </Button>
      </div>
    </div>
  );
}
