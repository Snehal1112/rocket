import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from 'sonner';
import * as tauriApi from '@/lib/tauri-api';
import type { OpenApiBackupRow, OpenApiChangeRow, OpenApiChanges } from '@/lib/tauri-api';
import { collectAllTabs } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncChanges } from '../OpenApiSyncChanges';
import { openSyncRequest } from '../openSyncRequest';

vi.mock('sonner', () => ({ toast: { success: vi.fn(), info: vi.fn() } }));

vi.mock('../openSyncRequest', () => ({ openSyncRequest: vi.fn() }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getOpenApiSyncChanges: vi.fn(),
    resetOpenApiSync: vi.fn(),
    restoreOpenApiSync: vi.fn(),
    discardOpenApiSyncBackup: vi.fn(),
    deleteOpenApiRequests: vi.fn(),
  };
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
    changes: [],
    ...over,
  };
}

function backup(over: Partial<OpenApiBackupRow> = {}): OpenApiBackupRow {
  return {
    key: 'GET /pets',
    name: 'List pets',
    method: 'GET',
    url: '{{baseUrl}}/mine',
    requestPath: 'pets/list-pets',
    savedAt: new Date(Date.now() - 3_600_000).toISOString(),
    fields: ['url'],
    ...over,
  };
}

function changes(over: Partial<OpenApiChanges> = {}): OpenApiChanges {
  return {
    baselineTrusted: true,
    modified: [],
    added: [],
    deleted: [],
    backups: [],
    resetAvailable: true,
    ...over,
  };
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
    expect(await screen.findByText('You have not changed anything. Edit a synced request and it shows up here with the values you changed.')).toBeInTheDocument();
    expect(screen.queryByText('Modified (0)')).toBeNull();
  });

  it('modified_rows_have_a_reset_button', async () => {
    show(changes({ modified: [row(), row({ uid: 'u2', key: 'GET /b' })] }));
    await screen.findByText('Modified (2)');
    expect(screen.getAllByRole('button', { name: 'Reset to spec' })).toHaveLength(2);
  });

  it('added_rows_open_and_delete_and_deleted_rows_have_no_buttons', async () => {
    show(changes({ added: [row({ uid: 'u2' })], deleted: [row({ uid: 'u3' })] }));
    await screen.findByText('Added by you (1)');
    const names = screen.getAllByRole('button').map((b) => b.textContent);
    expect(names).toEqual(['Delete all (1)', 'Open request', 'Delete']);
  });

  describe('reset scope', () => {
    it('a_name_only_row_has_no_reset_button_and_is_not_counted', async () => {
      show(
        changes({
          modified: [
            row({ uid: 'u1', key: 'GET /a', fields: ['name'] }),
            row({ uid: 'u2', key: 'GET /b', fields: ['url', 'name'] }),
            row({ uid: 'u3', key: 'GET /c', fields: ['header:Accept'] }),
          ],
        }),
      );
      await screen.findByText('Modified (3)');
      expect(screen.getAllByRole('button', { name: 'Reset to spec' })).toHaveLength(2);
      expect(screen.getByRole('button', { name: 'Reset all to spec (2)' })).toBeInTheDocument();
    });

    it('reset_all_sends_only_the_eligible_keys', async () => {
      vi.mocked(tauriApi.resetOpenApiSync).mockResolvedValue({ reset: ['GET /b'], skipped: [] });
      show(
        changes({
          modified: [
            row({ uid: 'u1', key: 'GET /a', fields: ['name'] }),
            row({ uid: 'u2', key: 'GET /b', fields: ['url'] }),
          ],
        }),
      );
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Reset all to spec (1)' }));
      await user.click(
        within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Reset' }),
      );
      await waitFor(() =>
        expect(tauriApi.resetOpenApiSync).toHaveBeenCalledWith('c', ['GET /b']),
      );
    });

    it('the_reset_confirm_names_the_replaced_fields', async () => {
      show(changes({ modified: [row({ fields: ['url'] })] }));
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Reset to spec' }));
      expect(await screen.findByRole('alertdialog')).toHaveTextContent(
        'Your changes to the URL, parameters, headers, body and auth of this request will be replaced with the last synced version.',
      );
    });
  });

  describe('delete', () => {
    const mine = () =>
      row({
        uid: 'u2',
        key: null,
        name: 'Mine',
        method: 'POST',
        url: '{{baseUrl}}/mine',
        requestPath: 'mine',
      });

    it('delete_on_an_added_row_asks_before_deleting', async () => {
      show(changes({ added: [mine()] }));
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Delete' }));
      const dialog = await screen.findByRole('alertdialog');
      expect(dialog).toHaveTextContent('Delete "POST {{baseUrl}}/mine"?');
      expect(dialog).toHaveTextContent(
        'This removes the file from the collection. The sync page cannot undo it, git can.',
      );
      expect(tauriApi.deleteOpenApiRequests).not.toHaveBeenCalled();
    });

    it('confirm_calls_the_hook_with_uid_and_path', async () => {
      vi.mocked(tauriApi.deleteOpenApiRequests).mockResolvedValue(1);
      show(changes({ added: [mine()] }));
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Delete' }));
      await user.click(
        within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
      );
      await waitFor(() =>
        expect(tauriApi.deleteOpenApiRequests).toHaveBeenCalledWith('c', [
          { uid: 'u2', requestPath: 'mine' },
        ]),
      );
      await waitFor(() => expect(toast.success).toHaveBeenCalledWith('Deleted.'));
    });

    it('deleting_closes_the_open_tab_of_the_request', async () => {
      vi.mocked(tauriApi.deleteOpenApiRequests).mockResolvedValue(1);
      usePaneStore.getState().reset();
      usePaneStore.getState().openTab({
        id: 'tab-mine',
        title: 'Mine',
        tabType: 'request',
        request: {} as never,
        response: null,
        isDirty: true,
        source: { collection: 'c', path: 'mine' },
      } as never);
      show(changes({ added: [mine()] }));
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Delete' }));
      await user.click(
        within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
      );
      await waitFor(() =>
        expect(
          collectAllTabs(usePaneStore.getState().root).some((t) => t.id === 'tab-mine'),
        ).toBe(false),
      );
    });

    it('cancel_deletes_nothing', async () => {
      show(changes({ added: [mine()] }));
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Delete' }));
      await user.click(
        within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Cancel' }),
      );
      expect(tauriApi.deleteOpenApiRequests).not.toHaveBeenCalled();
    });

    it('delete_all_sends_every_added_row', async () => {
      vi.mocked(tauriApi.deleteOpenApiRequests).mockResolvedValue(2);
      show(
        changes({
          added: [mine(), { ...mine(), uid: 'u3', name: 'Two', requestPath: 'two' }],
        }),
      );
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Delete all (2)' }));
      const dialog = await screen.findByRole('alertdialog');
      expect(dialog).toHaveTextContent('Delete 2 requests?');
      await user.click(within(dialog).getByRole('button', { name: 'Delete' }));
      await waitFor(() =>
        expect(tauriApi.deleteOpenApiRequests).toHaveBeenCalledWith('c', [
          { uid: 'u2', requestPath: 'mine' },
          { uid: 'u3', requestPath: 'two' },
        ]),
      );
    });

    it('a_failed_delete_shows_a_fixed_sentence', async () => {
      vi.mocked(tauriApi.deleteOpenApiRequests).mockRejectedValue(
        'cannot remove /home/me/secret/mine.yml',
      );
      show(changes({ added: [mine()] }));
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Delete' }));
      await user.click(
        within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
      );
      const alert = await screen.findByText(/could not be completed|Nothing was changed|try again/i);
      expect(alert.textContent ?? '').not.toContain('secret');
    });

    it('the_buttons_are_disabled_while_deleting', async () => {
      vi.mocked(tauriApi.deleteOpenApiRequests).mockReturnValue(new Promise(() => undefined));
      show(changes({ added: [mine()] }));
      const user = userEvent.setup();
      await user.click(await screen.findByRole('button', { name: 'Delete' }));
      await user.click(
        within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }),
      );
      await waitFor(() => expect(screen.getByRole('button', { name: 'Delete' })).toBeDisabled());
      expect(screen.getByRole('button', { name: 'Delete all (1)' })).toBeDisabled();
    });
  });

  it('open_request_button_calls_the_helper', async () => {
    vi.mocked(openSyncRequest).mockResolvedValue('opened');
    show(changes({ modified: [row()] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Open request' }));
    expect(openSyncRequest).toHaveBeenCalledWith({
      collection: 'c',
      uid: 'u1',
      requestPath: 'pets/list-pets.yml',
    });
  });

  it('a_missing_request_shows_a_fixed_sentence', async () => {
    vi.mocked(openSyncRequest).mockResolvedValue('missing');
    show(changes({ modified: [row()] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Open request' }));
    expect(await screen.findByText('The request file was not found.')).toBeInTheDocument();
  });

  it('a_deleted_row_has_no_open_button', async () => {
    show(changes({ deleted: [row({ uid: 'u3' })] }));
    await screen.findByText('Deleted by you (1)');
    expect(screen.queryByRole('button', { name: 'Open request' })).not.toBeInTheDocument();
  });

  it('reset_asks_first_and_cancel_sends_nothing', async () => {
    show(changes({ modified: [row({ method: 'GET', url: '/orders', fields: ['url', 'header:Accept'] })] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Reset to spec' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(
      within(dialog).getByText(
        'Reset GET /orders to the spec? Your changes to the URL, parameters, headers, body and auth of this request will be replaced with the last synced version. Your name, method, description, scripts, tests, assertions and docs are kept. A backup is kept so you can restore it.',
      ),
    ).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(tauriApi.resetOpenApiSync).not.toHaveBeenCalled();
  });

  it('confirming_a_reset_sends_that_one_key', async () => {
    vi.mocked(tauriApi.resetOpenApiSync).mockResolvedValue({ reset: ['GET /pets'], skipped: [] });
    show(changes({ modified: [row(), row({ uid: 'u2', key: 'GET /b', name: 'B' })] }));
    const user = userEvent.setup();
    await user.click((await screen.findAllByRole('button', { name: 'Reset to spec' }))[0]);
    await user.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Reset' }));
    await waitFor(() =>
      expect(tauriApi.resetOpenApiSync).toHaveBeenCalledWith('c', ['GET /pets']),
    );
  });

  it('reset_all_asks_once_with_the_count_and_sends_all_keys', async () => {
    vi.mocked(tauriApi.resetOpenApiSync).mockResolvedValue({ reset: [], skipped: [] });
    show(
      changes({
        modified: [
          row(),
          row({ uid: 'u2', key: 'GET /b' }),
          row({ uid: 'u3', key: 'GET /c' }),
        ],
      }),
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Reset all to spec (3)' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(within(dialog).getByText(/^Reset 3 requests to the spec\?/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Reset' }));
    await waitFor(() =>
      expect(tauriApi.resetOpenApiSync).toHaveBeenCalledWith('c', ['GET /pets', 'GET /b', 'GET /c']),
    );
    expect(tauriApi.resetOpenApiSync).toHaveBeenCalledTimes(1);
  });

  it('reset_is_disabled_with_a_note_when_unavailable', async () => {
    show(changes({ resetAvailable: false, baselineTrusted: false, modified: [row()] }));
    expect(await screen.findByRole('button', { name: 'Reset to spec' })).toBeDisabled();
    expect(
      screen.getByText(
        'These requests cannot be reset because the sync data is from an older version.',
      ),
    ).toBeInTheDocument();
  });

  it('shows_a_fixed_sentence_when_the_reset_fails', async () => {
    vi.mocked(tauriApi.resetOpenApiSync).mockRejectedValue(
      'The request changed on disk. Reload and try again.',
    );
    show(changes({ modified: [row()] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Reset to spec' }));
    await user.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Reset' }));
    expect(
      await screen.findByText('The request changed on disk. Reload and try again.'),
    ).toBeInTheDocument();
  });

  it('an_unknown_reset_error_is_not_shown_raw', async () => {
    vi.mocked(tauriApi.resetOpenApiSync).mockRejectedValue('failed at /home/me/secret.yml');
    show(changes({ modified: [row()] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Reset to spec' }));
    await user.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Reset' }));
    await screen.findByRole('alert');
    expect(screen.queryByText(/secret\.yml/)).toBeNull();
  });

  it('the_buttons_are_disabled_while_a_reset_runs', async () => {
    vi.mocked(tauriApi.resetOpenApiSync).mockReturnValue(new Promise(() => undefined));
    show(changes({ modified: [row(), row({ uid: 'u2', key: 'GET /b' })] }));
    const user = userEvent.setup();
    await user.click((await screen.findAllByRole('button', { name: 'Reset to spec' }))[0]);
    await user.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Reset' }));
    await waitFor(() =>
      expect(screen.getAllByRole('button', { name: 'Reset to spec' })[1]).toBeDisabled(),
    );
    expect(screen.getByRole('button', { name: 'Reset all to spec (2)' })).toBeDisabled();
  });

  it('shows_a_fixed_sentence_when_loading_fails', async () => {
    vi.mocked(tauriApi.getOpenApiSyncChanges).mockRejectedValue('failed at /home/me/secret.yml');
    renderWithQuery(<OpenApiSyncChanges collection='c' />);
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.queryByText(/secret\.yml/)).toBeNull();
  });

  it('shows_the_reset_by_you_group_only_with_backups', async () => {
    show(changes({ modified: [row()] }));
    await screen.findByText('Modified (1)');
    expect(screen.queryByText(/^Reset by you/)).toBeNull();
  });

  it('backups_show_when_the_request_is_no_longer_modified', async () => {
    show(changes({ backups: [backup()] }));
    expect(await screen.findByText('Reset by you (1)')).toBeInTheDocument();
    expect(screen.queryByText('You have not changed anything. Edit a synced request and it shows up here with the values you changed.')).toBeNull();
    expect(screen.getByText('{{baseUrl}}/mine')).toBeInTheDocument();
    expect(screen.getByText('URL')).toBeInTheDocument();
    expect(screen.getByText('Saved 1h ago')).toBeInTheDocument();
  });

  it('restore_my_version_sends_the_key_and_toasts', async () => {
    vi.mocked(tauriApi.restoreOpenApiSync).mockResolvedValue(undefined);
    show(changes({ backups: [backup()] }));
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Restore my version' }));
    await waitFor(() => expect(tauriApi.restoreOpenApiSync).toHaveBeenCalledWith('c', 'GET /pets'));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('Your version is back.'));
  });

  it('a_failed_restore_shows_a_fixed_sentence', async () => {
    vi.mocked(tauriApi.restoreOpenApiSync).mockRejectedValue('failed at /home/me/secret.yml');
    show(changes({ backups: [backup()] }));
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Restore my version' }));
    await screen.findByRole('alert');
    expect(screen.queryByText(/secret\.yml/)).toBeNull();
    expect(toast.success).not.toHaveBeenCalled();
  });

  it('discard_asks_first', async () => {
    show(changes({ backups: [backup()] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Discard backup' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(
      within(dialog).getByText(
        'Discard the backup of GET {{baseUrl}}/mine? You cannot restore it afterwards.',
      ),
    ).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(tauriApi.discardOpenApiSyncBackup).not.toHaveBeenCalled();
  });

  it('discard_confirm_sends_the_key', async () => {
    vi.mocked(tauriApi.discardOpenApiSyncBackup).mockResolvedValue(undefined);
    show(changes({ backups: [backup()] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Discard backup' }));
    await user.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Discard' }));
    await waitFor(() =>
      expect(tauriApi.discardOpenApiSyncBackup).toHaveBeenCalledWith('c', 'GET /pets'),
    );
  });

  it('a_reset_tells_how_many_requests_were_reset', async () => {
    vi.mocked(tauriApi.resetOpenApiSync).mockResolvedValue({ reset: ['GET /pets'], skipped: [] });
    show(changes({ modified: [row()] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Reset to spec' }));
    await user.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Reset' }));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('Reset 1 request.'));
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('a_reset_that_skips_requests_says_so', async () => {
    vi.mocked(tauriApi.resetOpenApiSync).mockResolvedValue({
      reset: [],
      skipped: ['GET /pets', 'GET /b'],
    });
    show(changes({ modified: [row(), row({ uid: 'u2', key: 'GET /b' })] }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Reset all to spec (2)' }));
    await user.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Reset' }));
    await waitFor(() =>
      expect(toast.info).toHaveBeenCalledWith(
        '2 requests were already like the spec or could not be reset.',
      ),
    );
    expect(toast.success).not.toHaveBeenCalled();
  });

  it('a_damaged_backups_file_gets_its_own_sentence', async () => {
    vi.mocked(tauriApi.getOpenApiSyncChanges).mockRejectedValue(
      'Internal error: openapi-reset-backups.json cannot be parsed: expected value at line 1',
    );
    renderWithQuery(<OpenApiSyncChanges collection='c' />);
    expect(
      await screen.findByText(/The reset backups file is damaged/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/expected value/)).toBeNull();
  });
  it('modified_rows_expand_to_show_baseline_and_yours', async () => {
    show(
      changes({
        modified: [
          row({
            fields: ['url'],
            changes: [{ field: 'url', baseline: '{{baseUrl}}/old', yours: '{{baseUrl}}/mine' }],
          }),
        ],
      }),
    );
    const user = userEvent.setup();
    await screen.findByText('Modified (1)');
    expect(screen.queryByText('Baseline')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Show changes' }));
    expect(screen.getByText('Baseline')).toBeInTheDocument();
    expect(screen.getByText('Yours')).toBeInTheDocument();
    expect(screen.getByText('{{baseUrl}}/old')).toBeInTheDocument();
    expect(screen.getByText('{{baseUrl}}/mine')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Hide changes' }));
    expect(screen.queryByText('Baseline')).not.toBeInTheDocument();
  });

  it('a_missing_value_shows_none', async () => {
    show(
      changes({
        modified: [
          row({
            fields: ['header:Accept'],
            changes: [{ field: 'header:Accept', baseline: null, yours: 'text/plain' }],
          }),
        ],
      }),
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Show changes' }));
    expect(screen.getByText('(none)')).toBeInTheDocument();
    expect(screen.getByText('text/plain')).toBeInTheDocument();
  });

  it('values_render_as_plain_text', async () => {
    show(
      changes({
        modified: [
          row({
            fields: ['url'],
            changes: [{ field: 'url', baseline: 'a', yours: '<script>alert(1)</script>' }],
          }),
        ],
      }),
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Show changes' }));
    expect(screen.getByText('<script>alert(1)</script>')).toBeInTheDocument();
    expect(document.querySelector('script')).toBeNull();
  });

  it('rows_without_values_do_not_expand', async () => {
    show(changes({ modified: [row({ fields: ['url'], changes: [] })] }));
    await screen.findByText('Modified (1)');
    expect(screen.queryByRole('button', { name: 'Show changes' })).not.toBeInTheDocument();
  });

  it('an_untrusted_baseline_shows_no_expand_button', async () => {
    show(
      changes({
        baselineTrusted: false,
        modified: [
          row({
            changes: [{ field: 'url', baseline: 'a', yours: 'b' }],
          }),
        ],
      }),
    );
    await screen.findByText('Modified (1)');
    expect(screen.queryByRole('button', { name: 'Show changes' })).not.toBeInTheDocument();
  });
});
