import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { SpecSyncActionsBar } from '../SpecSyncActionsBar';

function props(overrides: Partial<React.ComponentProps<typeof SpecSyncActionsBar>> = {}) {
  return {
    query: '',
    onQueryChange: vi.fn(),
    filteredCount: 3000,
    selectedCount: 0,
    itemSelectedCount: 0,
    busy: false,
    onSelectAll: vi.fn(),
    onSelectNone: vi.fn(),
    onSkip: vi.fn(),
    onApply: vi.fn(),
    ...overrides,
  };
}

describe('SpecSyncActionsBar', () => {
  it('shows the count of the whole filtered set on Select all', async () => {
    const p = props();
    render(<SpecSyncActionsBar {...p} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Select all (3000)' }));
    expect(p.onSelectAll).toHaveBeenCalled();
  });

  it('disables Skip and Apply without a selection and shows the selected count', () => {
    render(<SpecSyncActionsBar {...props()} />);
    expect(screen.getByRole('button', { name: /^Skip selected/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
    expect(screen.getByText('0 selected')).toBeInTheDocument();
  });

  it('enables Skip and Apply and calls back', async () => {
    const p = props({ selectedCount: 2, itemSelectedCount: 2 });
    render(<SpecSyncActionsBar {...p} />);
    const user = userEvent.setup();
    expect(screen.getByText('2 selected')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /^Skip selected/ }));
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await user.click(screen.getByRole('button', { name: 'Select none' }));
    expect(p.onSkip).toHaveBeenCalled();
    expect(p.onApply).toHaveBeenCalled();
    expect(p.onSelectNone).toHaveBeenCalled();
  });

  it('shows the number of requests the Skip button acts on', () => {
    render(<SpecSyncActionsBar {...props({ selectedCount: 5, itemSelectedCount: 3 })} />);
    expect(screen.getByRole('button', { name: 'Skip selected (3)' })).toBeInTheDocument();
  });

  it('cannot skip variables, only requests', () => {
    render(<SpecSyncActionsBar {...props({ selectedCount: 1, itemSelectedCount: 0 })} />);
    expect(screen.getByRole('button', { name: /^Skip selected/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
  });

  it('disables Skip and Apply while busy', () => {
    render(
      <SpecSyncActionsBar {...props({ selectedCount: 2, itemSelectedCount: 2, busy: true })} />,
    );
    expect(screen.getByRole('button', { name: /^Skip selected/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
  });

  it('reports the search text', async () => {
    const p = props();
    render(<SpecSyncActionsBar {...p} />);
    await userEvent.setup().type(screen.getByRole('searchbox', { name: 'Search changes' }), 'p');
    expect(p.onQueryChange).toHaveBeenCalledWith('p');
  });
});
