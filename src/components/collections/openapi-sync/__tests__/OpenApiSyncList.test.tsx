import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { OpenApiSyncItem } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { OpenApiSyncList } from '../OpenApiSyncList';
import { renderWithQuery } from '@/test/trust-fixtures';
import { changedItem, makePlan } from './fixtures';

function props(overrides: Partial<React.ComponentProps<typeof OpenApiSyncList>> = {}) {
  return {
    collection: 'c',
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

// The big-plan tests render hundreds of rows, which is slow when the machine is busy.
vi.setConfig({ testTimeout: 30000 });

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getOpenApiSyncSkipped: vi.fn(), unskipOpenApiSync: vi.fn() };
});

describe('OpenApiSyncList', () => {
  it('shows the groups with counts and calls back on select', async () => {
    const p = props();
    render(<OpenApiSyncList {...p} />);
    expect(screen.getByText('New (1)')).toBeInTheDocument();
    expect(screen.getByText('Changed (1)')).toBeInTheDocument();
    expect(screen.getByText('Removed (1)')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByText('Add pet'));
    expect(p.onSelect).toHaveBeenCalledWith('changed:POST /pets');
  });

  it('collapses a group', async () => {
    render(<OpenApiSyncList {...props()} />);
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
    render(<OpenApiSyncList {...props({ plan })} />);
    expect(screen.queryByText('Same one')).toBeNull();
    await userEvent.setup().click(screen.getByText('Stays as you have it (1)'));
    expect(screen.getByText('Same one')).toBeInTheDocument();
    expect(screen.getByText('Variables (1)')).toBeInTheDocument();
    expect(screen.getByText('Base URL baseUrl')).toBeInTheDocument();
    expect(screen.getByText(/2 skipped/)).toBeInTheDocument();
    expect(screen.getByText(/Old request/)).toBeInTheDocument();
  });

  it('caps a very large plan at 500 rows per group and offers Show more', async () => {
    const plan = makePlan({ items: manyItems(600) });
    render(<OpenApiSyncList {...props({ plan })} />);
    expect(screen.getByText('Changed (600)')).toBeInTheDocument();
    expect(screen.getByText('Request 499')).toBeInTheDocument();
    expect(screen.queryByText('Request 500')).toBeNull();
    expect(screen.getByText('100 more not shown')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Show more' }));
    expect(screen.getByText('Request 599')).toBeInTheDocument();
    expect(screen.queryByText(/more not shown/)).toBeNull();
  });

  it('toggles an approval with the row checkbox and with the Space key', async () => {
    const p = props();
    render(<OpenApiSyncList {...p} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('checkbox', { name: 'Include Add pet in apply' }));
    expect(p.onToggle).toHaveBeenCalledWith('POST /pets');
    screen.getByRole('checkbox', { name: 'Include List pets in apply' }).focus();
    await user.keyboard(' ');
    expect(p.onToggle).toHaveBeenCalledWith('GET /pets');
    // The checkbox does not select the row.
    expect(p.onSelect).not.toHaveBeenCalled();
  });

  it('shows checked and skipped state from the sets', () => {
    const p = props({ approved: new Set(['POST /pets']), skips: new Set(['GET /pets']) });
    render(<OpenApiSyncList {...p} />);
    expect(screen.getByRole('checkbox', { name: 'Include Add pet in apply' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Include List pets in apply' })).not.toBeChecked();
    expect(screen.getByText('Skipped')).toBeInTheDocument();
  });

  it('toggles a variable with its own callback', async () => {
    const plan = makePlan({
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'v' }],
    });
    const p = props({ plan });
    render(<OpenApiSyncList {...p} />);
    await userEvent.setup().click(screen.getByRole('checkbox', { name: 'Include Base URL baseUrl in apply' }));
    expect(p.onToggleVariable).toHaveBeenCalledWith('baseUrl');
    expect(p.onToggle).not.toHaveBeenCalled();
  });

  it('filters every group by the query and lifts the cap for the matches', () => {
    const plan = makePlan({ items: [...manyItems(650), ...makePlan().items] });
    render(<OpenApiSyncList {...props({ plan, query: 'request' })} />);
    // All 650 match, which is past the cap of 500.
    expect(screen.getByText('Changed (650)')).toBeInTheDocument();
    expect(screen.getByText('Request 649')).toBeInTheDocument();
    expect(screen.queryByText(/more not shown/)).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show more' })).toBeNull();
    expect(screen.getByText('New (0)')).toBeInTheDocument();
  });

  it('says when nothing matches the query', () => {
    render(<OpenApiSyncList {...props({ query: 'zzz' })} />);
    expect(screen.getAllByText('Nothing here.').length).toBeGreaterThan(0);
  });

  it('the_skipped_line_opens_the_list', async () => {
    vi.mocked(tauriApi.getOpenApiSyncSkipped).mockResolvedValue([
      { key: 'listPets', name: 'Hidden pets', method: 'GET', path: '/pets' },
    ]);
    renderWithQuery(<OpenApiSyncList {...props({ plan: makePlan({ skippedHidden: 1 }) })} />);
    expect(tauriApi.getOpenApiSyncSkipped).not.toHaveBeenCalled();
    const line = screen.getByRole('button', { name: '1 skipped, hidden from this list.' });
    await userEvent.setup().click(line);
    expect(await screen.findByText('Hidden pets')).toBeInTheDocument();
    await userEvent.setup().click(line);
    expect(screen.queryByText('Hidden pets')).toBeNull();
  });
});
