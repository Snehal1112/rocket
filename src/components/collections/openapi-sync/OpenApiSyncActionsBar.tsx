import { MoreHorizontal, Search } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
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
  /** Selected requests only. */
  itemSelectedCount: number;
  /** Requests and variables that match the search, removed ones left out. */
  filteredCount: number;
  busy: boolean;
  /** True while the plan is a placeholder, so selecting would act on stale rows. */
  locked: boolean;
  onSelectMatches: () => void;
  onSkipSelected: () => void;
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

/** The title, the bulk actions and the search of the review. The sync button is in the footer. */
export function OpenApiSyncActionsBar({
  query,
  onQueryChange,
  updateCount,
  itemCount,
  itemSelectedCount,
  filteredCount,
  busy,
  locked,
  onSelectMatches,
  onSkipSelected,
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
  const searching = query.trim().length > 0;
  return (
    <div className='flex flex-col gap-3 border-b border-border px-4 py-3'>
      <div className='flex flex-wrap items-start justify-between gap-3'>
        <div className='min-w-0'>
          <h2 className='text-sm font-semibold'>Review changes</h2>
          <p className='text-xs text-muted-foreground'>
            {`${updateCount} ${updateCount === 1 ? 'update' : 'updates'} from the spec. Choose what to take. Whatever stays on Keep is left as it is.`}
          </p>
        </div>
        <div className='flex flex-wrap items-center gap-2'>
          <Button variant='outline' size='sm' onClick={onAcceptAll} disabled={bulkOff}>
            Accept all
          </Button>
          <Button variant='outline' size='sm' onClick={onKeepCurrent} disabled={busy || locked}>
            Keep current
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant='outline' size='sm' aria-label='More actions'>
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align='end'>
              <DropdownMenuItem
                disabled={busy || itemSelectedCount === 0}
                onSelect={onSkipSelected}
              >
                {`Skip selected (${itemSelectedCount})`}
              </DropdownMenuItem>
              <DropdownMenuItem disabled={bulkOff || itemCount === 0} onSelect={onSkipAll}>
                Skip all
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
      <div className='flex flex-wrap items-center gap-3'>
        <div className='relative w-64 max-w-full'>
          <Search
            aria-hidden='true'
            className='pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground'
          />
          <Input
            type='search'
            aria-label='Search changes'
            placeholder='Search changes'
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            className='h-9 appearance-none bg-transparent pl-8 [&::-webkit-search-cancel-button]:appearance-none'
          />
        </div>
        {searching && (
          <Button
            variant='outline'
            size='sm'
            onClick={onSelectMatches}
            disabled={busy || locked || filteredCount === 0}
          >
            {`Select matches (${filteredCount})`}
          </Button>
        )}
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
        {conflictsOnly && conflictCount > 0 && (
          <span className='text-xs text-muted-foreground'>Show conflicts only</span>
        )}
        <div className='flex items-center gap-2 sm:ml-auto'>
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
      </div>
    </div>
  );
}

interface OpenApiSyncFooterProps {
  updateCount: number;
  selectedCount: number;
  busy: boolean;
  onConfirm: () => void;
}

/** The bar at the bottom: what will happen, and the one button that does it. */
export function OpenApiSyncFooter({
  updateCount,
  selectedCount,
  busy,
  onConfirm,
}: OpenApiSyncFooterProps) {
  const left = updateCount - selectedCount;
  return (
    <div className='flex flex-wrap items-center justify-between gap-3 border-t border-border bg-card px-4 py-3'>
      <p className='text-sm text-muted-foreground'>
        {selectedCount === 0
          ? 'Nothing is selected, so a sync would change nothing.'
          : `${selectedCount} of ${updateCount} will be applied${left > 0 ? `, ${left} stay as they are` : ''}.`}
      </p>
      <Button size='sm' onClick={onConfirm} disabled={busy || selectedCount === 0}>
        {`Confirm and sync (${selectedCount})`}
      </Button>
    </div>
  );
}
