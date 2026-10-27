import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { OpenApiSyncActionsBar } from '../OpenApiSyncActionsBar';

function props(overrides: Partial<React.ComponentProps<typeof OpenApiSyncActionsBar>> = {}) {
  return {
    query: '',
    onQueryChange: vi.fn(),
    filteredCount: 3000,
    selectedCount: 0,
    itemSelectedCount: 0,
    busy: false,
    locked: false,
    onSelectAll: vi.fn(),
    onSelectNone: vi.fn(),
    onSkip: vi.fn(),
    onApply: vi.fn(),
    ...overrides,
  };
}

describe('OpenApiSyncActionsBar', () => {
  it('shows the count of the whole filtered set on Select all', async () => {
    const p = props();
    render(<OpenApiSyncActionsBar {...p} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Select all (3000)' }));
    expect(p.onSelectAll).toHaveBeenCalled();
  });

  it('disables Skip and Apply without a selection and shows the selected count', () => {
    render(<OpenApiSyncActionsBar {...props()} />);
    expect(screen.getByRole('button', { name: /^Skip selected/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
    expect(screen.getByText('0 selected')).toBeInTheDocument();
  });

  it('enables Skip and Apply and calls back', async () => {
    const p = props({ selectedCount: 2, itemSelectedCount: 2 });
    render(<OpenApiSyncActionsBar {...p} />);
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
    render(<OpenApiSyncActionsBar {...props({ selectedCount: 5, itemSelectedCount: 3 })} />);
    expect(screen.getByRole('button', { name: 'Skip selected (3)' })).toBeInTheDocument();
  });

  it('cannot skip variables, only requests', () => {
    render(<OpenApiSyncActionsBar {...props({ selectedCount: 1, itemSelectedCount: 0 })} />);
    expect(screen.getByRole('button', { name: /^Skip selected/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
  });

  it('disables Skip and Apply while busy', () => {
    render(
      <OpenApiSyncActionsBar {...props({ selectedCount: 2, itemSelectedCount: 2, busy: true })} />,
    );
    expect(screen.getByRole('button', { name: /^Skip selected/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
  });

  it('disables Select all and Select none while locked', () => {
    render(<OpenApiSyncActionsBar {...props({ locked: true, selectedCount: 1 })} />);
    expect(screen.getByRole('button', { name: /^Select all/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Select none' })).toBeDisabled();
  });

  it('reports the search text', async () => {
    const p = props();
    render(<OpenApiSyncActionsBar {...p} />);
    await userEvent.setup().type(screen.getByRole('searchbox', { name: 'Search changes' }), 'p');
    expect(p.onQueryChange).toHaveBeenCalledWith('p');
  });
});
