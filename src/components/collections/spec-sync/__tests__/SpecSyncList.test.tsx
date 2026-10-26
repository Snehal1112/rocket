import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { OpenApiSyncItem } from '@/lib/tauri-api';
import { SpecSyncList } from '../SpecSyncList';
import { changedItem, makePlan } from './fixtures';

function props(overrides: Partial<React.ComponentProps<typeof SpecSyncList>> = {}) {
  return {
    plan: makePlan(),
    selectedId: null,
    onSelect: vi.fn(),
    query: '',
    approved: new Set<string>(),
    skips: new Set<string>(),
    variables: new Set<string>(),
    onToggle: vi.fn(),
    onToggleVariable: vi.fn(),
    ...overrides,
  };
}

function manyItems(count: number): OpenApiSyncItem[] {
  return Array.from({ length: count }, (_, i) => ({
    ...changedItem,
    key: `GET /p${i}`,
    name: `Request ${i}`,
  })) as OpenApiSyncItem[];
}

describe('SpecSyncList', () => {
  it('shows the groups with counts and calls back on select', async () => {
    const p = props();
    render(<SpecSyncList {...p} />);
    expect(screen.getByText('New (1)')).toBeInTheDocument();
    expect(screen.getByText('Changed (1)')).toBeInTheDocument();
    expect(screen.getByText('Removed (1)')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByText('Add pet'));
    expect(p.onSelect).toHaveBeenCalledWith('changed:POST /pets');
  });

  it('collapses a group', async () => {
    render(<SpecSyncList {...props()} />);
    const user = userEvent.setup();
    expect(screen.getByText('Add pet')).toBeInTheDocument();
    await user.click(screen.getByText('Changed (1)'));
    expect(screen.queryByText('Add pet')).toBeNull();
  });

  it('keeps the unchanged section collapsed and lists variables and notes', async () => {
    const plan = makePlan({
      unchanged: [{ key: 'GET /a', name: 'Same one' }],
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'v', spec: 'https://x' }],
      skippedHidden: 2,
      deletedByUser: ['Old request'],
    });
    render(<SpecSyncList {...props({ plan })} />);
    expect(screen.queryByText('Same one')).toBeNull();
    await userEvent.setup().click(screen.getByText('Stays as you have it (1)'));
    expect(screen.getByText('Same one')).toBeInTheDocument();
    expect(screen.getByText('Variables (1)')).toBeInTheDocument();
    expect(screen.getByText('Base URL baseUrl')).toBeInTheDocument();
    expect(screen.getByText(/2 skipped/)).toBeInTheDocument();
    expect(screen.getByText(/Old request/)).toBeInTheDocument();
  });

  it('caps a very large plan at 500 rows per group and offers Show more', async () => {
    const plan = makePlan({ items: manyItems(3000) });
    render(<SpecSyncList {...props({ plan })} />);
    expect(screen.getByText('Changed (3000)')).toBeInTheDocument();
    expect(screen.getByText('Request 499')).toBeInTheDocument();
    expect(screen.queryByText('Request 500')).toBeNull();
    expect(screen.getByText('2500 more not shown')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show more' }));
    expect(screen.getByText('Request 999')).toBeInTheDocument();
    expect(screen.queryByText('Request 1000')).toBeNull();
    expect(screen.getByText('2000 more not shown')).toBeInTheDocument();
  });

  it('toggles an approval with the row checkbox and with the Space key', async () => {
    const p = props();
    render(<SpecSyncList {...p} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('checkbox', { name: 'Select Add pet' }));
    expect(p.onToggle).toHaveBeenCalledWith('POST /pets');
    screen.getByRole('checkbox', { name: 'Select List pets' }).focus();
    await user.keyboard(' ');
    expect(p.onToggle).toHaveBeenCalledWith('GET /pets');
    // The checkbox does not select the row.
    expect(p.onSelect).not.toHaveBeenCalled();
  });

  it('shows checked and skipped state from the sets', () => {
    const p = props({ approved: new Set(['POST /pets']), skips: new Set(['GET /pets']) });
    render(<SpecSyncList {...p} />);
    expect(screen.getByRole('checkbox', { name: 'Select Add pet' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Select List pets' })).not.toBeChecked();
    expect(screen.getByText('Skipped')).toBeInTheDocument();
  });

  it('toggles a variable with its own callback', async () => {
    const plan = makePlan({
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'v' }],
    });
    const p = props({ plan });
    render(<SpecSyncList {...p} />);
    await userEvent.setup().click(screen.getByRole('checkbox', { name: 'Select Base URL baseUrl' }));
    expect(p.onToggleVariable).toHaveBeenCalledWith('baseUrl');
    expect(p.onToggle).not.toHaveBeenCalled();
  });

  it('filters every group by the query and lifts the cap for the matches', () => {
    const plan = makePlan({ items: [...manyItems(1200), ...makePlan().items] });
    render(<SpecSyncList {...props({ plan, query: 'request 1' })} />);
    // Request 1, 10-19, 100-199 and 1000-1199: 1 + 10 + 100 + 200 matches.
    expect(screen.getByText('Changed (311)')).toBeInTheDocument();
    expect(screen.getByText('Request 1199')).toBeInTheDocument();
    expect(screen.queryByText(/more not shown/)).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show more' })).toBeNull();
    expect(screen.getByText('New (0)')).toBeInTheDocument();
  });

  it('says when nothing matches the query', () => {
    render(<SpecSyncList {...props({ query: 'zzz' })} />);
    expect(screen.getAllByText('Nothing here.').length).toBeGreaterThan(0);
  });
});
