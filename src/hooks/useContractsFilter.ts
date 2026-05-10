import { useCallback, useMemo, useState } from 'react';
import type {
  Contract,
  ContractFilterStatus,
  ContractSortKey,
  ContractsFilterState,
  ContractViewMode,
} from '@/types/contracts';

const DEFAULT: ContractsFilterState = {
  search: '',
  statuses: ['all'],
  sort: 'updated',
  sortDir: 'desc',
  view: 'cards',
};

/** Pure function — exported for unit testing */
export function applyFilter(contracts: Contract[], state: ContractsFilterState): Contract[] {
  let result = contracts;

  if (state.search.trim()) {
    const q = state.search.toLowerCase();
    result = result.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.provider.name.toLowerCase().includes(q) ||
        c.consumers.some((p) => p.name.toLowerCase().includes(q)) ||
        c.version.toLowerCase().includes(q) ||
        (c.scope.type === 'folder' && c.scope.rel_path.toLowerCase().includes(q)),
    );
  }

  if (!state.statuses.includes('all')) {
    result = result.filter((c) => state.statuses.includes(c.status as ContractFilterStatus));
  } else {
    // 'all' excludes archived — archived only shows when explicitly filtered.
    result = result.filter((c) => c.status !== 'archived');
  }

  return applySort(result, state.sort, state.sortDir);
}

function applySort(contracts: Contract[], sort: ContractSortKey, dir: 'asc' | 'desc'): Contract[] {
  return [...contracts].sort((a, b) => {
    let cmp = 0;
    switch (sort) {
      case 'name':
        cmp = a.name.localeCompare(b.name);
        break;
      case 'effective':
        cmp = a.effectiveAt.localeCompare(b.effectiveAt);
        break;
      case 'drift':
        cmp = b.driftCount - a.driftCount;
        break;
      default:
        cmp = b.updatedAt.localeCompare(a.updatedAt);
        break;
    }
    return dir === 'asc' ? -cmp : cmp;
  });
}

export function useContractsFilter(contracts: Contract[]) {
  const [state, setState] = useState<ContractsFilterState>(DEFAULT);

  const filtered = useMemo(() => applyFilter(contracts, state), [contracts, state]);

  const setSearch = useCallback((s: string) => setState((prev) => ({ ...prev, search: s })), []);

  const toggleStatus = useCallback((status: ContractFilterStatus) => {
    setState((prev) => {
      if (status === 'all') return { ...prev, statuses: ['all'] };
      const without = prev.statuses.filter((s) => s !== 'all' && s !== status);
      const next = prev.statuses.includes(status) ? without : [...without, status];
      return { ...prev, statuses: next.length === 0 ? ['all'] : next };
    });
  }, []);

  const setSort = useCallback((sort: ContractSortKey) => setState((p) => ({ ...p, sort })), []);
  const setView = useCallback((view: ContractViewMode) => setState((p) => ({ ...p, view })), []);

  return { filtered, filterState: state, setSearch, toggleStatus, setSort, setView };
}
