import { Search } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';

interface OpenApiSyncActionsBarProps {
  query: string;
  onQueryChange: (query: string) => void;
  /** Requests and variables that match the search, hidden rows included. */
  filteredCount: number;
  selectedCount: number;
  /** Selected requests only. Variables cannot be skipped. */
  itemSelectedCount: number;
  busy: boolean;
  /** True while the plan is a placeholder, so selecting would act on stale rows. */
  locked: boolean;
  onSelectAll: () => void;
  onSelectNone: () => void;
  onSkip: () => void;
  onApply: () => void;
}

/** Search, select all or none, and the Skip and Apply buttons. */
export function OpenApiSyncActionsBar({
  query,
  onQueryChange,
  filteredCount,
  selectedCount,
  itemSelectedCount,
  busy,
  locked,
  onSelectAll,
  onSelectNone,
  onSkip,
  onApply,
}: OpenApiSyncActionsBarProps) {
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
      <Button
        variant='outline'
        size='sm'
        onClick={onSelectAll}
        disabled={locked || filteredCount === 0}
      >
        Select all ({filteredCount})
      </Button>
      <Button
        variant='outline'
        size='sm'
        onClick={onSelectNone}
        disabled={locked || selectedCount === 0}
      >
        Select none
      </Button>
      <span className='text-xs text-muted-foreground'>{selectedCount} selected</span>
      <div className='ml-auto flex items-center gap-2'>
        <Button
          variant='outline'
          size='sm'
          onClick={onSkip}
          disabled={busy || itemSelectedCount === 0}
        >
          Skip selected ({itemSelectedCount})
        </Button>
        <Button size='sm' onClick={onApply} disabled={busy || selectedCount === 0}>
          Apply
        </Button>
      </div>
    </div>
  );
}
