import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import type { OpenApiOverview } from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncOverview } from '../OpenApiSyncOverview';
import { makeStatus } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getOpenApiSyncOverview: vi.fn() };
});

function overview(over: Partial<OpenApiOverview> = {}): OpenApiOverview {
  return {
    endpoints: 42,
    inSync: 36,
    changedByYou: 3,
    updatesWaiting: 5,
    specVersion: '1.2.0',
    lastSyncedAt: null,
    lastSyncedUpdated: 0,
    ...over,
  };
}

function show(over: Partial<OpenApiOverview> = {}, status = makeStatus(), onReview = vi.fn()) {
  vi.mocked(tauriApi.getOpenApiSyncOverview).mockResolvedValue(overview(over));
  renderWithQuery(<OpenApiSyncOverview collection='c' status={status} onReview={onReview} />);
  return onReview;
}

function card(label: string) {
  return screen.getByText(label).parentElement as HTMLElement;
}

describe('OpenApiSyncOverview', () => {
  beforeEach(() => vi.clearAllMocks());

  it('shows_the_four_counts', async () => {
    show();
    await screen.findByText('Endpoints');
    expect(within(card('Endpoints')).getByText('42')).toBeInTheDocument();
    expect(within(card('In sync with spec')).getByText('36')).toBeInTheDocument();
    expect(within(card('Changed by you')).getByText('3')).toBeInTheDocument();
    expect(within(card('Spec updates waiting')).getByText('5')).toBeInTheDocument();
  });

  it('shows_a_dash_for_unknown_counts', async () => {
    show({ inSync: null, changedByYou: null });
    await screen.findByText('Endpoints');
    expect(within(card('In sync with spec')).getByText('–')).toBeInTheDocument();
    expect(within(card('Changed by you')).getByText('–')).toBeInTheDocument();
    expect(
      screen.getByText('Sync data is from an older version, so these counts are unknown.'),
    ).toBeInTheDocument();
  });

  it('the_banner_appears_only_when_updates_wait', async () => {
    show({ updatesWaiting: 0 });
    await screen.findByText('Endpoints');
    expect(screen.queryByText(/updates are waiting/)).toBeNull();
    expect(screen.queryByRole('button', { name: 'Review updates' })).toBeNull();
  });

  it('review_updates_calls_onReview', async () => {
    const onReview = show();
    expect(await screen.findByText('5 updates are waiting.')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Review updates' }));
    expect(onReview).toHaveBeenCalledTimes(1);
  });

  it('hides_the_query_until_show_full_link', async () => {
    show({}, makeStatus({ location: 'https://example.com/openapi.json?token=abc' }));
    await screen.findByText('Endpoints');
    expect(screen.queryByText(/abc/)).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show full link' }));
    expect(screen.getByText(/abc/)).toBeInTheDocument();
  });

  it('edit_connection_settings_opens_the_dialog', async () => {
    show();
    await screen.findByText('Endpoints');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Connection settings' }));
    await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
  });

  it('hides_credentials_until_show_full_link', async () => {
    show({}, makeStatus({ location: 'https://user:secret@example.com/openapi.json' }));
    await screen.findByText('Endpoints');
    expect(screen.queryByText(/secret/)).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show full link' }));
    expect(screen.getByText(/secret/)).toBeInTheDocument();
  });

  it('last_checked_never_checked', async () => {
    show({}, makeStatus({ lastCheckedAt: null }));
    await screen.findByText('Endpoints');
    expect(screen.getByText('Not checked yet')).toBeInTheDocument();
  });

  it('last_checked_with_no_problems', async () => {
    show({}, makeStatus({ lastStatus: { state: 'unchanged' } }));
    expect(await screen.findByText('2h ago. No problems.')).toBeInTheDocument();
  });

  it('last_checked_with_a_failure_leaves_the_message_to_the_banner', async () => {
    show(
      {},
      makeStatus({
        lastStatus: { state: 'failed' },
        lastMessage: 'The server could not be reached.',
      }),
    );
    expect(await screen.findByText('2h ago.')).toBeInTheDocument();
    expect(screen.queryByText(/The server could not be reached/)).toBeNull();
  });

  it('says_one_request_updated_in_the_singular', async () => {
    show({
      lastSyncedAt: new Date(Date.now() - 3 * 3_600_000).toISOString(),
      lastSyncedUpdated: 1,
    });
    expect(await screen.findByText(/1 request updated/)).toBeInTheDocument();
  });

  it('shows_never_when_nothing_was_synced', async () => {
    show();
    await screen.findByText('Endpoints');
    expect(screen.getByText('Never')).toBeInTheDocument();
  });

  it('shows_the_sync_time_and_count', async () => {
    show({
      lastSyncedAt: new Date(Date.now() - 3 * 3_600_000).toISOString(),
      lastSyncedUpdated: 4,
    });
    expect(await screen.findByText(/3h ago/)).toBeInTheDocument();
    expect(screen.getByText(/4 requests updated/)).toBeInTheDocument();
  });

  it('shows_a_fixed_sentence_when_the_overview_fails', async () => {
    vi.mocked(tauriApi.getOpenApiSyncOverview).mockRejectedValue(
      new Error('cannot read /home/me/secret/sync.yml'),
    );
    renderWithQuery(<OpenApiSyncOverview collection='c' status={makeStatus()} onReview={vi.fn()} />);
    expect(await screen.findByText(/Something went wrong with the OpenAPI sync/)).toBeInTheDocument();
    expect(screen.queryByText(/secret/)).toBeNull();
  });

  it('a_file_source_shows_the_local_file_badge', async () => {
    show({}, makeStatus({ location: 'file:///home/me/specs/api.yaml' }));
    expect(await screen.findByText('Local file')).toBeInTheDocument();
  });
});
