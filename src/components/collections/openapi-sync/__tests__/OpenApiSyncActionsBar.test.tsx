import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { OpenApiSyncActionsBar, OpenApiSyncFooter } from '../OpenApiSyncActionsBar';

function props(overrides: Partial<React.ComponentProps<typeof OpenApiSyncActionsBar>> = {}) {
  return {
    query: '',
    onQueryChange: vi.fn(),
    updateCount: 5,
    itemCount: 5,
    itemSelectedCount: 2,
    filteredCount: 4,
    busy: false,
    locked: false,
    onSelectMatches: vi.fn(),
    onSkipSelected: vi.fn(),
    onAcceptAll: vi.fn(),
    onSkipAll: vi.fn(),
    onKeepCurrent: vi.fn(),
    keepMine: true,
    onKeepMineChange: vi.fn(),
    conflictCount: 0,
    conflictsOnly: false,
    onToggleConflictsOnly: vi.fn(),
    ...overrides,
  };
}

async function openMore() {
  await userEvent.setup().click(screen.getByRole('button', { name: 'More actions' }));
}

describe('OpenApiSyncActionsBar', () => {
  it('shows_the_title_the_summary_and_the_main_buttons', async () => {
    const p = props();
    render(<OpenApiSyncActionsBar {...p} />);
    const user = userEvent.setup();
    expect(screen.getByRole('heading', { name: 'Review changes' })).toBeInTheDocument();
    expect(screen.getByText(/5 updates from the spec/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Accept all' }));
    await user.click(screen.getByRole('button', { name: 'Keep current' }));
    expect(p.onAcceptAll).toHaveBeenCalled();
    expect(p.onKeepCurrent).toHaveBeenCalled();
  });

  it('the_skip_actions_live_in_the_more_menu', async () => {
    const p = props();
    render(<OpenApiSyncActionsBar {...p} />);
    expect(screen.queryByRole('menuitem', { name: 'Skip all' })).toBeNull();
    await openMore();
    await userEvent.setup().click(screen.getByRole('menuitem', { name: 'Skip all' }));
    expect(p.onSkipAll).toHaveBeenCalled();
    await openMore();
    await userEvent.setup().click(screen.getByRole('menuitem', { name: 'Skip selected (2)' }));
    expect(p.onSkipSelected).toHaveBeenCalled();
  });

  it('uses_the_singular_for_one_update', () => {
    render(<OpenApiSyncActionsBar {...props({ updateCount: 1 })} />);
    expect(screen.getByText(/1 update from the spec/)).toBeInTheDocument();
  });

  it('disables_every_action_while_busy', async () => {
    render(<OpenApiSyncActionsBar {...props({ busy: true, query: 'p' })} />);
    for (const name of ['Accept all', 'Keep current', 'Select matches (4)']) {
      expect(screen.getByRole('button', { name })).toBeDisabled();
    }
    await openMore();
    expect(screen.getByRole('menuitem', { name: 'Skip all' })).toHaveAttribute('data-disabled');
    expect(screen.getByRole('menuitem', { name: 'Skip selected (2)' })).toHaveAttribute(
      'data-disabled',
    );
  });

  it('disables_the_bulk_buttons_while_locked', () => {
    render(<OpenApiSyncActionsBar {...props({ locked: true })} />);
    expect(screen.getByRole('button', { name: 'Accept all' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Keep current' })).toBeDisabled();
  });

  it('reports the search text', async () => {
    const p = props();
    render(<OpenApiSyncActionsBar {...p} />);
    await userEvent.setup().type(screen.getByRole('searchbox', { name: 'Search changes' }), 'p');
    expect(p.onQueryChange).toHaveBeenCalledWith('p');
  });

  it('skip_all_is_disabled_without_request_items', async () => {
    render(<OpenApiSyncActionsBar {...props({ itemCount: 0, updateCount: 2 })} />);
    expect(screen.getByRole('button', { name: 'Accept all' })).toBeEnabled();
    await openMore();
    expect(screen.getByRole('menuitem', { name: 'Skip all' })).toHaveAttribute('data-disabled');
  });

  it('select_matches_shows_only_while_searching', async () => {
    const p = props({ query: 'pet' });
    render(<OpenApiSyncActionsBar {...p} />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Select matches (4)' }));
    expect(p.onSelectMatches).toHaveBeenCalled();
  });

  it('no_select_matches_without_a_search', () => {
    render(<OpenApiSyncActionsBar {...props()} />);
    expect(screen.queryByRole('button', { name: /Select matches/ })).toBeNull();
  });

  it('the_search_field_drops_the_native_search_look', () => {
    render(<OpenApiSyncActionsBar {...props()} />);
    expect(screen.getByRole('searchbox', { name: 'Search changes' })).toHaveClass('appearance-none');
  });

  it('the_switch_shows_on_by_default', () => {
    render(<OpenApiSyncActionsBar {...props()} />);
    const sw = screen.getByRole('switch', { name: 'Keep my values' });
    expect(sw).toBeChecked();
    expect(
      screen.getByText("Accept all keeps what you changed. Turn off to take the spec's values."),
    ).toBeInTheDocument();
  });

  it('the_switch_has_an_accessible_name_and_calls_back', async () => {
    const p = props();
    render(<OpenApiSyncActionsBar {...p} />);
    await userEvent.setup().click(screen.getByRole('switch', { name: 'Keep my values' }));
    expect(p.onKeepMineChange).toHaveBeenCalledWith(false);
  });

  it('no_conflict_badge_at_zero', () => {
    render(<OpenApiSyncActionsBar {...props({ conflictCount: 0 })} />);
    expect(screen.queryByRole('button', { name: /conflict/ })).toBeNull();
  });

  it('the_filter_label_goes_when_no_conflict_is_left', () => {
    render(<OpenApiSyncActionsBar {...props({ conflictCount: 0, conflictsOnly: true })} />);
    expect(screen.queryByText('Show conflicts only')).toBeNull();
  });

  it('the_badge_shows_the_count_and_toggles_the_filter', async () => {
    const p = props({ conflictCount: 3 });
    render(<OpenApiSyncActionsBar {...p} />);
    const badge = screen.getByRole('button', { name: '3 conflicts' });
    expect(badge).toHaveAttribute('aria-pressed', 'false');
    await userEvent.setup().click(badge);
    expect(p.onToggleConflictsOnly).toHaveBeenCalled();
  });

  it('one_conflict_reads_in_the_singular_and_the_filter_state_shows', () => {
    render(<OpenApiSyncActionsBar {...props({ conflictCount: 1, conflictsOnly: true })} />);
    const badge = screen.getByRole('button', { name: '1 conflict' });
    expect(badge).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('Show conflicts only')).toBeInTheDocument();
  });
});

describe('OpenApiSyncFooter', () => {
  it('says_what_will_be_applied_and_confirms', async () => {
    const onConfirm = vi.fn();
    render(<OpenApiSyncFooter updateCount={5} selectedCount={2} busy={false} onConfirm={onConfirm} />);
    expect(screen.getByText('2 of 5 will be applied, 3 stay as they are.')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Confirm and sync (2)' }));
    expect(onConfirm).toHaveBeenCalled();
  });

  it('is_disabled_with_nothing_selected', () => {
    render(<OpenApiSyncFooter updateCount={5} selectedCount={0} busy={false} onConfirm={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Confirm and sync (0)' })).toBeDisabled();
    expect(screen.getByText('Nothing is selected, so a sync would change nothing.')).toBeInTheDocument();
  });

  it('is_disabled_while_busy', () => {
    render(<OpenApiSyncFooter updateCount={5} selectedCount={5} busy onConfirm={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Confirm and sync (5)' })).toBeDisabled();
  });
});
