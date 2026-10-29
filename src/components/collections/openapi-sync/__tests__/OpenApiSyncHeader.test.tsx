import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncHeader } from '../OpenApiSyncHeader';
import { makeStatus } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, checkOpenApiSyncNow: vi.fn(), getOpenApiSyncStatus: vi.fn() };
});

describe('OpenApiSyncHeader', () => {
  it('shows the last check time', () => {
    renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
    expect(screen.getByText('https://example.com/openapi.json')).toBeInTheDocument();
    expect(screen.getByText('Last checked 2h ago')).toBeInTheDocument();
    expect(screen.getByText('Check interval: Every day')).toBeInTheDocument();
  });

  it('hides_credentials_of_the_link', () => {
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        status={makeStatus({ location: 'https://user:secret@example.com/openapi.json' })}
        waiting={0}
      />,
    );
    expect(screen.queryByText(/secret/)).toBeNull();
    expect(screen.getByText('example.com/openapi.json')).toBeInTheDocument();
  });

  it('view_spec_calls_the_handler', async () => {
    const onViewSpec = vi.fn();
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        status={makeStatus()}
        waiting={0}
        onViewSpec={onViewSpec}
      />,
    );
    await userEvent.setup().click(screen.getByRole('button', { name: 'View spec' }));
    expect(onViewSpec).toHaveBeenCalledTimes(1);
  });

  it('has a Check now button', () => {
    renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
    expect(screen.getByRole('button', { name: 'Check now' })).toBeEnabled();
  });

  it('disables Check now without consent', () => {
    renderWithQuery(
      <OpenApiSyncHeader collection='c' status={makeStatus({ consent: 'missing' })} waiting={0} />,
    );
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDisabled();
  });

  it('says so when it was never checked', () => {
    renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus({ lastCheckedAt: null })} waiting={0} />);
    expect(screen.getByText('Not checked yet')).toBeInTheDocument();
  });

  it('the_header_alert_is_gone', () => {
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        waiting={0}
        status={makeStatus({
          lastStatus: { state: 'failed', code: 'network' },
          lastMessage: 'The server could not be reached.',
        })}
      />,
    );
    // The failure shows in the page banner, so the header holds no second one.
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByText('The server could not be reached.')).toBeNull();
  });

  it('the_header_button_opens_connection_settings', async () => {
    const user = userEvent.setup();
    renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
    await user.click(screen.getByRole('button', { name: 'Connection settings' }));
    expect(
      within(await screen.findByRole('dialog')).getByRole('heading', { name: 'Connection settings' }),
    ).toBeInTheDocument();
  });

  describe('a spec file', () => {
    const FILE = 'file:///home/me/specs/api.yaml';

    it('a_file_source_shows_the_local_file_badge', () => {
      renderWithQuery(
        <OpenApiSyncHeader collection='c' waiting={0} status={makeStatus({ location: FILE })} />,
      );
      expect(screen.getByText('Local file')).toBeInTheDocument();
      expect(screen.getByText('api.yaml in /home/me/specs')).toBeInTheDocument();
    });

    it('a_link_source_has_no_local_file_badge', () => {
      renderWithQuery(<OpenApiSyncHeader collection='c' waiting={0} status={makeStatus()} />);
      expect(screen.queryByText('Local file')).toBeNull();
    });

    it('a_file_source_needs_approval_on_a_new_machine', () => {
      renderWithQuery(
        <OpenApiSyncHeader
          collection='c'
          waiting={0}
          status={makeStatus({ location: FILE, consent: 'missing' })}
        />,
      );
      expect(screen.getByText('Needs approval')).toBeInTheDocument();
    });
  });

  it('shows_a_credentials_link_as_host_and_path_until_show_full_link', async () => {
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        waiting={0}
        status={makeStatus({ location: 'https://user:pass@api.example.com/spec.json' })}
      />,
    );
    expect(screen.getByText('api.example.com/spec.json')).toBeInTheDocument();
    expect(screen.queryByText(/user:pass/)).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show full link' }));
    expect(screen.getByText('https://user:pass@api.example.com/spec.json')).toBeInTheDocument();
  });

  it('shows_the_query_only_after_show_full_link', async () => {
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        waiting={0}
        status={makeStatus({ location: 'https://api.example.com/v1/spec.json?token=abc' })}
      />,
    );
    expect(screen.queryByText(/token=abc/)).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show full link' }));
    expect(screen.getByText('https://api.example.com/v1/spec.json?token=abc')).toBeInTheDocument();
  });

  it('shows_the_chip_and_the_count', () => {
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        waiting={2}
        status={makeStatus({ lastStatus: { state: 'unchanged' } })}
      />,
    );
    expect(screen.getByText('2 updates')).toBeInTheDocument();
  });

  it('shows_updates_waiting_while_the_count_is_unknown', () => {
    renderWithQuery(<OpenApiSyncHeader collection='c' waiting={null} status={makeStatus()} />);
    expect(screen.getByText('Updates waiting')).toBeInTheDocument();
  });

  describe('actions', () => {
    beforeAll(() => {
      HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
      HTMLElement.prototype.scrollIntoView = vi.fn();
    });

    it('has_no_actions_menu_because_disconnect_is_in_the_connection_settings', async () => {
      const user = userEvent.setup();
      renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
      expect(screen.queryByRole('button', { name: 'OpenAPI sync actions' })).toBeNull();
      await user.click(screen.getByRole('button', { name: 'Connection settings' }));
      const dialog = await screen.findByRole('dialog');
      expect(within(dialog).getByRole('button', { name: 'Disconnect' })).toBeInTheDocument();
    });

    it('a_visible_button_beside_check_now_opens_the_connection_settings', async () => {
      const user = userEvent.setup();
      renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
      await user.click(screen.getByRole('button', { name: 'Connection settings' }));
      expect(await screen.findByRole('dialog')).toHaveTextContent('Connection settings');
    });

    it('runs a check from the button', async () => {
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
      renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
      await userEvent.setup().click(screen.getByRole('button', { name: 'Check now' }));
      await waitFor(() => expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('c'));
    });
  });
});
