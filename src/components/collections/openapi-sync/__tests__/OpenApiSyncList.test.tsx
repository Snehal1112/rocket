import { fireEvent, render, screen, within } from '@testing-library/react';
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
    query: '',
    approved: new Set<string>(),
    skips: new Set<string>(),
    variables: new Set<string>(),
    onToggle: vi.fn(),
    onToggleVariable: vi.fn(),
    detail: {
      collection: 'c',
      plan: overrides.plan ?? makePlan(),
      choices: new Map(),
      links: [],
      onChoose: vi.fn(),
      onLink: vi.fn(),
      onUnlink: vi.fn(),
      linksLocked: false,
    },
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
  it('shows the sections in order with their counts', () => {
    render(<OpenApiSyncList {...props()} />);
    const titles = screen.getAllByRole('heading', { level: 3 }).map((h) => h.textContent);
    expect(titles).toEqual(['Updated in spec', 'New in spec', 'Removed from spec']);
    expect(screen.getAllByText('1')).toHaveLength(3);
  });

  it('shows the method, the path and the name of a row', () => {
    render(<OpenApiSyncList {...props()} />);
    // The leading variable is dropped, so the path reads first.
    expect(screen.getAllByText('/pets')).toHaveLength(2);
    expect(screen.getByText('Add pet')).toBeInTheDocument();
    expect(screen.getAllByText('POST').length).toBeGreaterThan(0);
  });

  it('marks a request that has a conflict', () => {
    render(<OpenApiSyncList {...props()} />);
    expect(screen.getByText('Conflict')).toBeInTheDocument();
  });

  it('opens a row in place and closes it again', async () => {
    render(<OpenApiSyncList {...props()} />);
    const user = userEvent.setup();
    const toggle = screen.getByRole('button', { name: 'Show details of Add pet' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await user.click(toggle);
    expect(screen.getByRole('button', { name: 'Hide details of Add pet' })).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    expect(screen.getByText('Header Accept')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Hide details of Add pet' }));
    expect(screen.queryByText('Header Accept')).toBeNull();
  });

  it('collapses a section', async () => {
    render(<OpenApiSyncList {...props()} />);
    const user = userEvent.setup();
    expect(screen.getByText('Add pet')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Updated in spec/ }));
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
    expect(screen.getByRole('heading', { level: 3, name: 'Variables' })).toBeInTheDocument();
    expect(screen.getByText('Base URL baseUrl')).toBeInTheDocument();
    expect(screen.getByText(/2 skipped/)).toBeInTheDocument();
    expect(screen.getByText(/Old request/)).toBeInTheDocument();
  });

  it('caps a very large plan at 500 rows per group and offers Show more', async () => {
    const plan = makePlan({ items: manyItems(600) });
    render(<OpenApiSyncList {...props({ plan })} />);
    expect(screen.getByText('500 of 600')).toBeInTheDocument();
    expect(screen.getByText('Request 499')).toBeInTheDocument();
    expect(screen.queryByText('Request 500')).toBeNull();
    expect(screen.getByText('100 more not shown')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Show more' }));
    expect(screen.getByText('Request 599')).toBeInTheDocument();
    expect(screen.queryByText(/more not shown/)).toBeNull();
  });

  it('flips a row with its choice and leaves the row closed', async () => {
    const p = props();
    render(<OpenApiSyncList {...p} />);
    const user = userEvent.setup();
    await user.click(
      within(screen.getByRole('radiogroup', { name: 'Choice for Add pet' })).getByRole('radio', {
        name: 'Update',
      }),
    );
    expect(p.onToggle).toHaveBeenCalledWith('POST /pets');
    // The choice is not the row, so nothing opened.
    expect(screen.queryByText('Header Accept')).toBeNull();
  });

  it('names the choice by the kind of the row', () => {
    render(<OpenApiSyncList {...props()} />);
    const names = (label: string) =>
      within(screen.getByRole('radiogroup', { name: label }))
        .getAllByRole('radio')
        .map((r) => r.textContent);
    expect(names('Choice for Add pet')).toEqual(['Keep current', 'Update']);
    expect(names('Choice for List pets')).toEqual(["Don't add", 'Add']);
    expect(names('Choice for Delete pet')).toEqual(['Keep', 'Delete']);
  });

  it('shows the choice and the skipped state from the sets', () => {
    const p = props({ approved: new Set(['POST /pets']), skips: new Set(['GET /pets']) });
    render(<OpenApiSyncList {...p} />);
    expect(
      within(screen.getByRole('radiogroup', { name: 'Choice for Add pet' })).getByRole('radio', {
        name: 'Update',
      }),
    ).toBeChecked();
    expect(
      within(screen.getByRole('radiogroup', { name: 'Choice for List pets' })).getByRole('radio', {
        name: "Don't add",
      }),
    ).toBeChecked();
    expect(screen.getByText('Skipped')).toBeInTheDocument();
  });

  it('flips a variable with its own callback', async () => {
    const plan = makePlan({
      variableChanges: [{ name: 'baseUrl', kind: 'baseUrl', digest: 'v' }],
    });
    const p = props({ plan });
    render(<OpenApiSyncList {...p} />);
    await userEvent.setup().click(
      within(screen.getByRole('radiogroup', { name: 'Choice for Base URL baseUrl' })).getByRole(
        'radio',
        { name: 'Update' },
      ),
    );
    expect(p.onToggleVariable).toHaveBeenCalledWith('baseUrl');
    expect(p.onToggle).not.toHaveBeenCalled();
  });

  it('filters every group by the query and lifts the cap for the matches', () => {
    const plan = makePlan({ items: [...manyItems(650), ...makePlan().items] });
    render(<OpenApiSyncList {...props({ plan, query: 'request' })} />);
    // All 650 match, which is past the cap of 500.
    expect(screen.getByText('650')).toBeInTheDocument();
    expect(screen.getByText('Request 649')).toBeInTheDocument();
    expect(screen.queryByText(/more not shown/)).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show more' })).toBeNull();
    expect(screen.queryByRole('heading', { level: 3, name: 'New in spec' })).toBeNull();
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
