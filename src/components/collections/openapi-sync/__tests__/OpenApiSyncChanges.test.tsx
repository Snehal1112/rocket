import { screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import type { OpenApiChangeRow, OpenApiChanges } from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncChanges } from '../OpenApiSyncChanges';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getOpenApiSyncChanges: vi.fn() };
});

function row(over: Partial<OpenApiChangeRow> = {}): OpenApiChangeRow {
  return {
    key: 'GET /pets',
    uid: 'u1',
    name: 'List pets',
    method: 'GET',
    url: '{{baseUrl}}/pets',
    requestPath: 'pets/list-pets.yml',
    fields: [],
    ...over,
  };
}

function changes(over: Partial<OpenApiChanges> = {}): OpenApiChanges {
  return { baselineTrusted: true, modified: [], added: [], deleted: [], ...over };
}

function show(data: OpenApiChanges) {
  vi.mocked(tauriApi.getOpenApiSyncChanges).mockResolvedValue(data);
  renderWithQuery(<OpenApiSyncChanges collection='c' />);
}

describe('OpenApiSyncChanges', () => {
  beforeEach(() => vi.clearAllMocks());

  it('groups_modified_added_and_deleted_with_counts', async () => {
    show(
      changes({
        modified: [row({ fields: ['url'] })],
        added: [row({ uid: 'u2', name: 'Mine', method: 'POST', url: '{{baseUrl}}/mine' })],
        deleted: [
          row({ uid: 'u3', name: 'DELETE /pets/{id}', method: '', url: '', requestPath: null }),
        ],
      }),
    );
    expect(await screen.findByText('Modified (1)')).toBeInTheDocument();
    expect(screen.getByText('Added by you (1)')).toBeInTheDocument();
    expect(screen.getByText('Deleted by you (1)')).toBeInTheDocument();
    expect(screen.getByText('{{baseUrl}}/mine')).toBeInTheDocument();
    expect(screen.getByText('DELETE /pets/{id}')).toBeInTheDocument();
    expect(
      screen.getByText(
        'Differences between your collection and the last synced spec. Sync never overwrites these without asking.',
      ),
    ).toBeInTheDocument();
    expect(screen.getByText('Scripts, tests and assertions are not tracked here.')).toBeInTheDocument();
  });

  it('deleted_rows_show_the_request_path_when_there_is_one', async () => {
    show(
      changes({
        deleted: [row({ method: '', url: '', requestPath: 'pets/gone.yml', name: 'GET /gone' })],
      }),
    );
    expect(await screen.findByText('pets/gone.yml')).toBeInTheDocument();
  });

  it('modified_rows_name_the_changed_fields', async () => {
    show(changes({ modified: [row({ fields: ['url', 'header:Accept'] })] }));
    expect(await screen.findByText('URL')).toBeInTheDocument();
    expect(screen.getByText('Header Accept')).toBeInTheDocument();
  });

  it('shows_the_approximate_note_without_a_trusted_baseline_and_no_field_names', async () => {
    show(changes({ baselineTrusted: false, modified: [row({ fields: ['url'] })] }));
    expect(
      await screen.findByText(
        'These differences are approximate because the sync data is from an older version.',
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText('URL')).toBeNull();
  });

  it('shows_the_empty_state', async () => {
    show(changes());
    expect(await screen.findByText('You have not changed anything.')).toBeInTheDocument();
    expect(screen.queryByText('Modified (0)')).toBeNull();
  });

  it('has_no_buttons_in_this_version', async () => {
    show(changes({ modified: [row()], added: [row({ uid: 'u2' })], deleted: [row({ uid: 'u3' })] }));
    await screen.findByText('Modified (1)');
    const lists = screen.getAllByRole('list');
    for (const list of lists) expect(within(list).queryAllByRole('button')).toHaveLength(0);
    expect(screen.queryAllByRole('button')).toHaveLength(0);
  });

  it('shows_a_fixed_sentence_when_loading_fails', async () => {
    vi.mocked(tauriApi.getOpenApiSyncChanges).mockRejectedValue('failed at /home/me/secret.yml');
    renderWithQuery(<OpenApiSyncChanges collection='c' />);
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.queryByText(/secret\.yml/)).toBeNull();
  });
});
