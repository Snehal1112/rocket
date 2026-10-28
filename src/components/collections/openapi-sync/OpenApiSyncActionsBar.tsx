import { Search } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';

interface OpenApiSyncActionsBarProps {
  query: string;
  onQueryChange: (query: string) => void;
  /** Requests and variables the spec changed. */
  updateCount: number;
  /** Requests only. Variables cannot be skipped. */
  itemCount: number;
  /** Selected requests and variables. */
  selectedCount: number;
  /** Selected requests only. */
  itemSelectedCount: number;
  /** Requests and variables that match the search, removed ones left out. */
  filteredCount: number;
  busy: boolean;
  /** True while the plan is a placeholder, so selecting would act on stale rows. */
  locked: boolean;
  onSelectMatches: () => void;
  onSkipSelected: () => void;
  onConfirm: () => void;
  onAcceptAll: () => void;
  onSkipAll: () => void;
  onKeepCurrent: () => void;
  /** True when Accept all keeps what the user changed. */
  keepMine: boolean;
  onKeepMineChange: (on: boolean) => void;
  /** Changed requests where the spec and the user changed the same field. */
  conflictCount: number;
  conflictsOnly: boolean;
  onToggleConflictsOnly: () => void;
}

/** Search, the update summary and the bulk actions: Accept all, Skip all, Keep current, Confirm. */
export function OpenApiSyncActionsBar({
  query,
  onQueryChange,
  updateCount,
  itemCount,
  selectedCount,
  itemSelectedCount,
  filteredCount,
  busy,
  locked,
  onSelectMatches,
  onSkipSelected,
  onConfirm,
  onAcceptAll,
  onSkipAll,
  onKeepCurrent,
  keepMine,
  onKeepMineChange,
  conflictCount,
  conflictsOnly,
  onToggleConflictsOnly,
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
      <Button
        variant='outline'
        size='sm'
        onClick={onSelectMatches}
        disabled={busy || locked || filteredCount === 0}
      >
        {`Select matches (${filteredCount})`}
      </Button>
      <span className='text-xs text-muted-foreground'>
        {`${updateCount} ${updateCount === 1 ? 'update' : 'updates'} from the spec · ${selectedCount} selected`}
      </span>
      {conflictCount > 0 && (
        <Button
          variant={conflictsOnly ? 'secondary' : 'outline'}
          size='sm'
          aria-pressed={conflictsOnly}
          onClick={onToggleConflictsOnly}
        >
          {`${conflictCount} ${conflictCount === 1 ? 'conflict' : 'conflicts'}`}
        </Button>
      )}
      {conflictsOnly && <span className='text-xs text-muted-foreground'>Show conflicts only</span>}
      <div className='flex items-center gap-2'>
        <Switch
          id='openapi-sync-keep-mine'
          checked={keepMine}
          onCheckedChange={onKeepMineChange}
          aria-label='Keep my values'
          aria-describedby='openapi-sync-keep-mine-help'
          disabled={busy}
        />
        <div className='flex flex-col'>
          <Label htmlFor='openapi-sync-keep-mine' className='text-xs font-medium'>
            Keep my values
          </Label>
          <span id='openapi-sync-keep-mine-help' className='text-[11px] text-muted-foreground'>
            Accept all keeps what you changed. Turn off to take the spec's values.
          </span>
        </div>
      </div>
      <div className='ml-auto flex flex-wrap items-center gap-2'>
        <Button variant='outline' size='sm' onClick={onAcceptAll} disabled={bulkOff}>
          Accept all
        </Button>
        <Button
          variant='outline'
          size='sm'
          onClick={onSkipSelected}
          disabled={busy || itemSelectedCount === 0}
        >
          {`Skip selected (${itemSelectedCount})`}
        </Button>
        <Button
          variant='outline'
          size='sm'
          onClick={onSkipAll}
          disabled={bulkOff || itemCount === 0}
        >
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
