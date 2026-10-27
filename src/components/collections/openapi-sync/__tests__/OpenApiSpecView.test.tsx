import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import type { OpenApiSpecOperation, OpenApiSpecView as SpecViewData } from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSpecView } from '../OpenApiSpecView';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getOpenApiSyncSpecView: vi.fn(), getOpenApiSyncOverview: vi.fn() };
});

// The 500-row tests render many rows, so a busy machine needs more time.
vi.setConfig({ testTimeout: 30000 });

function op(over: Partial<OpenApiSpecOperation> = {}): OpenApiSpecOperation {
  return {
    key: 'listPets',
    method: 'GET',
    path: '/pets',
    summary: 'List pets',
    parameters: [{ name: 'limit', location: 'query', required: false }],
    responses: [
      { status: '200', description: 'A list' },
      { status: '404', description: 'Missing' },
    ],
    isNew: false,
    ...over,
  };
}

function view(over: Partial<SpecViewData> = {}): SpecViewData {
  return {
    title: 'Pets API',
    version: '2.1',
    fetchedAt: '2026-10-27T10:00:00+00:00',
    state: 'in_sync',
    groups: [
      { tag: 'pets', operations: [op(), op({ key: 'getPet', path: '/pets/{id}', summary: null })] },
      { tag: 'store', operations: [op({ key: 'getInventory', path: '/inventory', summary: 'Stock' })] },
    ],
    ...over,
  };
}

function show(data: SpecViewData, waiting = 0) {
  vi.mocked(tauriApi.getOpenApiSyncSpecView).mockResolvedValue(data);
  vi.mocked(tauriApi.getOpenApiSyncOverview).mockResolvedValue({
    endpoints: 3,
    inSync: 3,
    changedByYou: 0,
    updatesWaiting: waiting,
    specVersion: '2.1',
    lastSyncedAt: null,
    lastSyncedUpdated: 0,
  });
  const onBack = vi.fn();
  renderWithQuery(<OpenApiSpecView collection='c' onBack={onBack} />);
  return onBack;
}

describe('OpenApiSpecView', () => {
  beforeEach(() => vi.clearAllMocks());

  it('groups_operations_by_tag', async () => {
    show(view());
    expect(await screen.findByText('pets (2)')).toBeInTheDocument();
    expect(screen.getByText('store (1)')).toBeInTheDocument();
    expect(screen.getByText('/inventory')).toBeInTheDocument();
    expect(screen.getByText('Pets API')).toBeInTheDocument();
  });

  it('says_whether_updates_wait', async () => {
    show(view({ state: 'updates_waiting' }), 2);
    expect(await screen.findByText('Showing the latest spec Rocket fetched.')).toBeInTheDocument();
    expect(await screen.findByText('2 updates are waiting.')).toBeInTheDocument();
  });

  it('says_the_collection_is_in_sync', async () => {
    show(view());
    expect(await screen.findByText('Your collection is in sync with it.')).toBeInTheDocument();
  });

  it('marks_new_operations', async () => {
    show(
      view({
        state: 'updates_waiting',
        groups: [{ tag: 'pets', operations: [op({ key: 'createPet', isNew: true })] }],
      }),
      1,
    );
    expect(await screen.findByText('New')).toBeInTheDocument();
  });

  it('search_filters_the_rows', async () => {
    show(view());
    const user = userEvent.setup();
    await screen.findByText('pets (2)');
    await user.type(screen.getByRole('textbox', { name: 'Search operations' }), 'stock');
    expect(screen.getByText('store (1)')).toBeInTheDocument();
    expect(screen.queryByText(/^pets \(/)).toBeNull();
    expect(screen.queryByText('/pets')).toBeNull();
  });

  it('lists_500_per_tag_then_show_more', async () => {
    const many = Array.from({ length: 600 }, (_, i) =>
      op({ key: `op${i}`, path: `/p${i}`, summary: null }),
    );
    show(view({ groups: [{ tag: 'big', operations: many }] }));
    await screen.findByText('big (600)');
    expect(screen.getAllByRole('button', { name: /^GET \/p/ })).toHaveLength(500);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show more (100)' }));
    expect(screen.getAllByRole('button', { name: /^GET \/p/ })).toHaveLength(600);
  });

  it('one_budget_covers_all_tags', async () => {
    const groups = Array.from({ length: 30 }, (_, g) => ({
      tag: `tag${g}`,
      operations: Array.from({ length: 30 }, (_, i) =>
        op({ key: `op${g}-${i}`, path: `/t${g}/p${i}`, summary: null }),
      ),
    }));
    show(view({ groups }));
    await screen.findByText('tag0 (30)');
    // 900 operations in 30 tags: only the first 500 rows are in the page.
    expect(screen.getAllByRole('button', { name: /^GET \/t/ })).toHaveLength(500);
    expect(screen.queryByText('tag29 (30)')).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show more (400)' }));
    expect(screen.getAllByRole('button', { name: /^GET \/t/ })).toHaveLength(900);
    expect(screen.getByText('tag29 (30)')).toBeInTheDocument();
  });

  it('expanding_a_row_shows_parameters_and_responses', async () => {
    show(view());
    const user = userEvent.setup();
    await screen.findByText('pets (2)');
    expect(screen.queryByText('A list')).toBeNull();
    await user.click(screen.getByRole('button', { name: /GET \/pets List pets/ }));
    expect(screen.getByText('limit (query)')).toBeInTheDocument();
    const list = screen.getByRole('list', { name: 'Responses' });
    expect(within(list).getByText('200')).toBeInTheDocument();
    expect(within(list).getByText('A list')).toBeInTheDocument();
    expect(within(list).getByText('Missing')).toBeInTheDocument();
  });

  it('summary_text_is_rendered_as_plain_text', async () => {
    show(
      view({
        groups: [
          { tag: 'pets', operations: [op({ summary: '<script>alert(1)</script> **bold**' })] },
        ],
      }),
    );
    expect(await screen.findByText(/<script>alert\(1\)<\/script> \*\*bold\*\*/)).toBeInTheDocument();
    expect(document.querySelector('script')).toBeNull();
  });

  it('no_copy_shows_no_saved_copy_yet', async () => {
    show(view({ state: 'no_copy', groups: [], title: '', version: null, fetchedAt: null }));
    expect(
      await screen.findByText('No saved copy yet. The next check stores one.'),
    ).toBeInTheDocument();
  });

  it('shows_a_fixed_sentence_when_loading_fails', async () => {
    vi.mocked(tauriApi.getOpenApiSyncSpecView).mockRejectedValue(
      new Error('cannot read /home/me/secret/path'),
    );
    vi.mocked(tauriApi.getOpenApiSyncOverview).mockResolvedValue({
      endpoints: 0,
      inSync: 0,
      changedByYou: 0,
      updatesWaiting: 0,
      specVersion: null,
      lastSyncedAt: null,
      lastSyncedUpdated: 0,
    });
    renderWithQuery(<OpenApiSpecView collection='c' onBack={vi.fn()} />);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Something went wrong with the OpenAPI sync');
    expect(screen.queryByText(/secret/)).toBeNull();
  });

  it('back_calls_the_handler', async () => {
    const onBack = show(view());
    await screen.findByText('pets (2)');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Back' }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });
});
