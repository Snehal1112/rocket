import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ContractCounts, ContractsFilterState } from '@/types/contracts';
import { ContractsFilterBar } from './ContractsFilterBar';

const defaultFilter: ContractsFilterState = {
  search: '',
  statuses: ['all'],
  sort: 'updated',
  sortDir: 'desc',
  view: 'cards',
};

const counts: ContractCounts = {
  total: 5,
  active: 2,
  drift: 1,
  breach: 1,
  inReview: 0,
  draft: 1,
  paused: 0,
  expired: 0,
  totalChanges: 4,
  changesAdded: 1,
  changesRemoved: 2,
  changesModified: 1,
};

function setup(
  overrides: Partial<{
    onSearch: (q: string) => void;
    onToggleStatus: (s: unknown) => void;
    onSetSort: (s: unknown) => void;
  }> = {},
) {
  const onSearch = overrides.onSearch ?? vi.fn();
  const onToggleStatus = overrides.onToggleStatus ?? vi.fn();
  const onSetSort = overrides.onSetSort ?? vi.fn();
  const onSetView = vi.fn();

  render(
    <ContractsFilterBar
      filterState={defaultFilter}
      counts={counts}
      onSearch={onSearch}
      onToggleStatus={onToggleStatus}
      onSetSort={onSetSort}
      onSetView={onSetView}
    />,
  );
  return { onSearch, onToggleStatus, onSetSort, onSetView };
}

describe('ContractsFilterBar', () => {
  it('renders search input', () => {
    setup();
    expect(screen.getByPlaceholderText('Search contracts…')).toBeInTheDocument();
  });

  it('shows "All" chip with total count', () => {
    setup();
    expect(screen.getByText('All')).toBeInTheDocument();
    expect(screen.getByText('5')).toBeInTheDocument();
  });

  it('hides status chip when count is zero', () => {
    setup();
    // 'paused' count is 0, should not appear
    expect(screen.queryByText('Paused')).not.toBeInTheDocument();
  });

  it('calls onToggleStatus when a chip is clicked', () => {
    const { onToggleStatus } = setup();
    fireEvent.click(screen.getByText('Active'));
    expect(onToggleStatus).toHaveBeenCalledWith('active');
  });

  it('calls onSearch after debounce (200ms)', async () => {
    const onSearch = vi.fn();
    setup({ onSearch });
    const input = screen.getByPlaceholderText('Search contracts…');
    await userEvent.type(input, 'Billing');
    await waitFor(() => expect(onSearch).toHaveBeenCalledWith('Billing'), { timeout: 500 });
  });
});
