import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { CheckFailureBanner } from '../CheckFailureBanner';
import { makeStatus } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, checkOpenApiSyncNow: vi.fn(), getOpenApiSyncStatus: vi.fn() };
});

const GITHUB = 'https://github.com/acme/api/blob/main/openapi.yaml';
const SENTENCE = 'The server could not be reached.';

function failed(code: string, over: Record<string, unknown> = {}) {
  return makeStatus({
    location: GITHUB,
    lastStatus: { state: 'failed', code },
    lastMessage: SENTENCE,
    ...over,
  });
}

function show(status: ReturnType<typeof makeStatus>) {
  vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(status);
  renderWithQuery(<CheckFailureBanner collection='c' status={status} />);
  return userEvent.setup();
}

describe('CheckFailureBanner', () => {
  beforeEach(() => vi.clearAllMocks());

  it('the_banner_shows_the_fixed_sentence', () => {
    show(failed('network', { lastMessage: SENTENCE }));
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Failed to check for spec updates');
    expect(alert).toHaveTextContent(SENTENCE);
    // The code is a machine word, not text for the user.
    expect(alert).not.toHaveTextContent('(network)');
  });

  it('the_banner_never_shows_a_raw_message_or_a_path', () => {
    show(failed('file_missing', { lastMessage: undefined, location: 'file:///home/me/secret/a.yaml' }));
    const text = screen.getByRole('alert').textContent ?? '';
    expect(text).toContain('The last check failed.');
    expect(text).not.toContain('secret');
  });

  it('no_banner_when_the_last_check_succeeded', () => {
    show(makeStatus({ lastStatus: { state: 'unchanged' } }));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('no_banner_without_consent', () => {
    show(failed('network', { consent: 'missing' }));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('a_token_failure_offers_add_access_token', () => {
    show(failed('not_found'));
    expect(screen.getByRole('button', { name: 'Add access token' })).toBeInTheDocument();
  });

  it('a_network_failure_offers_check_now', async () => {
    vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
    const user = show(failed('network'));
    await user.click(screen.getByRole('button', { name: 'Check now' }));
    await waitFor(() => expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('c'));
  });

  it('an_unknown_code_offers_connection_settings', async () => {
    const user = show(failed('something_new', { location: 'https://example.com/o.json' }));
    await user.click(screen.getByRole('button', { name: 'Connection settings' }));
    expect(await screen.findByRole('dialog')).toHaveTextContent('Connection settings');
  });

  it('the_button_opens_the_dialog_at_the_right_field', async () => {
    const user = show(failed('not_found'));
    await user.click(screen.getByRole('button', { name: 'Add access token' }));
    const input = await screen.findByLabelText('Access token');
    await waitFor(() => expect(input).toHaveFocus());
  });

  it('a_missing_file_opens_the_change_link_form', async () => {
    const user = show(failed('file_missing', { location: 'file:///home/me/specs/api.yaml' }));
    await user.click(screen.getByRole('button', { name: 'Choose another file' }));
    expect(await screen.findByRole('button', { name: /choose spec file/i })).toBeInTheDocument();
  });
});
