import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { OpenApiSyncActionsBar } from '../OpenApiSyncActionsBar';

function props(overrides: Partial<React.ComponentProps<typeof OpenApiSyncActionsBar>> = {}) {
  return {
    query: '',
    onQueryChange: vi.fn(),
    updateCount: 5,
    selectedCount: 2,
    busy: false,
    locked: false,
    onConfirm: vi.fn(),
    onAcceptAll: vi.fn(),
    onSkipAll: vi.fn(),
    onKeepCurrent: vi.fn(),
    ...overrides,
  };
}

describe('OpenApiSyncActionsBar', () => {
  it('shows_the_summary_and_the_four_buttons', async () => {
    const p = props();
    render(<OpenApiSyncActionsBar {...p} />);
    const user = userEvent.setup();
    expect(screen.getByText('5 updates from the spec · 2 selected')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    await user.click(screen.getByRole('button', { name: 'Skip all' }));
    await user.click(screen.getByRole('button', { name: 'Keep current' }));
    await user.click(screen.getByRole('button', { name: 'Confirm and sync (2)' }));
    expect(p.onAcceptAll).toHaveBeenCalled();
    expect(p.onSkipAll).toHaveBeenCalled();
    expect(p.onKeepCurrent).toHaveBeenCalled();
    expect(p.onConfirm).toHaveBeenCalled();
  });

  it('confirm_is_disabled_with_nothing_selected', () => {
    render(<OpenApiSyncActionsBar {...props({ selectedCount: 0 })} />);
    expect(screen.getByRole('button', { name: 'Confirm and sync (0)' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Accept all' })).toBeEnabled();
  });

  it('confirm_shows_the_selected_count', () => {
    render(<OpenApiSyncActionsBar {...props({ selectedCount: 7 })} />);
    expect(screen.getByRole('button', { name: 'Confirm and sync (7)' })).toBeEnabled();
  });

  it('uses_the_singular_for_one_update', () => {
    render(<OpenApiSyncActionsBar {...props({ updateCount: 1, selectedCount: 1 })} />);
    expect(screen.getByText('1 update from the spec · 1 selected')).toBeInTheDocument();
  });

  it('disables_every_action_while_busy', () => {
    render(<OpenApiSyncActionsBar {...props({ busy: true })} />);
    for (const name of ['Accept all', 'Skip all', 'Keep current', 'Confirm and sync (2)']) {
      expect(screen.getByRole('button', { name })).toBeDisabled();
    }
  });

  it('disables_the_bulk_buttons_while_locked', () => {
    render(<OpenApiSyncActionsBar {...props({ locked: true })} />);
    expect(screen.getByRole('button', { name: 'Accept all' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Skip all' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Keep current' })).toBeDisabled();
  });

  it('reports the search text', async () => {
    const p = props();
    render(<OpenApiSyncActionsBar {...p} />);
    await userEvent.setup().type(screen.getByRole('searchbox', { name: 'Search changes' }), 'p');
    expect(p.onQueryChange).toHaveBeenCalledWith('p');
  });
});
