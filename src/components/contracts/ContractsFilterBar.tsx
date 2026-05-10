import { ChevronDown, LayoutGrid, List, Search } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { statusLabel } from '@/lib/contracts/statusMachine';
import { cn } from '@/lib/utils';
import type {
  ContractCounts,
  ContractFilterStatus,
  ContractSortKey,
  ContractsFilterState,
  ContractViewMode,
} from '@/types/contracts';

const STATUS_CHIPS: ContractFilterStatus[] = [
  'all',
  'active',
  'drift',
  'breach',
  'draft',
  'paused',
  'expired',
  'archived',
];

const SORT_OPTIONS: Array<{ key: ContractSortKey; label: string }> = [
  { key: 'updated', label: 'Last updated' },
  { key: 'name', label: 'Name A→Z' },
  { key: 'effective', label: 'Effective date' },
  { key: 'drift', label: 'Drift count' },
];

function getChipCount(status: ContractFilterStatus, counts: ContractCounts): number {
  if (status === 'all') return counts.total;
  const map: Partial<Record<ContractFilterStatus, number>> = {
    active: counts.active,
    drift: counts.drift,
    breach: counts.breach,
    draft: counts.draft,
    paused: counts.paused,
    expired: counts.expired,
    archived: counts.archived,
  };
  return map[status] ?? 0;
}

interface ContractsFilterBarProps {
  filterState: ContractsFilterState;
  counts: ContractCounts;
  onSearch: (q: string) => void;
  onToggleStatus: (s: ContractFilterStatus) => void;
  onSetSort: (s: ContractSortKey) => void;
  onSetView: (v: ContractViewMode) => void;
}

export function ContractsFilterBar({
  filterState,
  counts,
  onSearch,
  onToggleStatus,
  onSetSort,
  onSetView,
}: ContractsFilterBarProps) {
  // Debounce search 200ms
  const [localSearch, setLocalSearch] = useState(filterState.search);
  useEffect(() => {
    const t = setTimeout(() => onSearch(localSearch), 200);
    return () => clearTimeout(t);
  }, [localSearch, onSearch]);

  const sortLabel = SORT_OPTIONS.find((o) => o.key === filterState.sort)?.label ?? 'Sort';

  return (
    <div className='flex items-center gap-2 px-6 py-3 border-b border-border flex-shrink-0 flex-wrap gap-y-2'>
      {/* Search */}
      <div className='relative max-w-[340px] flex-1 min-w-[180px]'>
        <Search
          className='absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground pointer-events-none'
          aria-hidden='true'
        />
        <Input
          placeholder='Search contracts…'
          value={localSearch}
          onChange={(e) => setLocalSearch(e.target.value)}
          className='pl-9 h-8 text-sm'
          aria-label='Search contracts'
        />
      </div>

      {/* Status chips */}
      <div className='flex items-center gap-1 flex-wrap'>
        {STATUS_CHIPS.map((status) => {
          const n = getChipCount(status, counts);
          if (status !== 'all' && n === 0) return null;
          const isActive = filterState.statuses.includes(status);
          return (
            <button
              key={status}
              type='button'
              onClick={() => onToggleStatus(status)}
              className={cn(
                'inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-[12px] font-medium border transition-colors',
                isActive
                  ? 'bg-[hsl(var(--primary)/0.12)] text-primary border-[hsl(var(--primary)/0.4)]'
                  : 'text-muted-foreground border-border hover:text-foreground hover:border-border/80',
              )}
            >
              {status === 'all' ? 'All' : statusLabel(status)}
              <span
                className={cn(
                  'inline-flex items-center justify-center rounded-full px-1 min-w-[16px] text-[10px] tabular-nums',
                  isActive ? 'bg-[hsl(var(--primary)/0.18)]' : 'bg-muted',
                )}
              >
                {n}
              </span>
            </button>
          );
        })}
      </div>

      <div className='ml-auto flex items-center gap-1'>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant='ghost' size='sm' className='h-8 text-xs gap-1.5 text-muted-foreground'>
              Sort: {sortLabel} <ChevronDown className='h-3 w-3' />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align='end'>
            {SORT_OPTIONS.map((o) => (
              <DropdownMenuItem
                key={o.key}
                onClick={() => onSetSort(o.key)}
                className={cn('text-sm', filterState.sort === o.key && 'font-medium text-primary')}
              >
                {o.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>

        <Button
          variant='ghost'
          size='icon'
          className='h-8 w-8'
          onClick={() => {
            if (filterState.view === 'cards') {
              toast('Table view coming soon');
            } else {
              onSetView('cards');
            }
          }}
          aria-label={filterState.view === 'cards' ? 'Switch to table view' : 'Switch to card view'}
        >
          {filterState.view === 'cards' ? (
            <List className='h-4 w-4' aria-hidden='true' />
          ) : (
            <LayoutGrid className='h-4 w-4' aria-hidden='true' />
          )}
        </Button>
      </div>
    </div>
  );
}
