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

  it('shows the message of a failed check in an alert', () => {
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        waiting={0}
        status={makeStatus({
          lastStatus: { state: 'failed', code: 'timeout' },
          lastMessage: 'The spec host did not answer in time.',
        })}
      />,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('The spec host did not answer in time.');
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

    it('a_missing_file_offers_choose_another_file', async () => {
      vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ location: FILE }));
      renderWithQuery(
        <OpenApiSyncHeader
          collection='c'
          waiting={0}
          status={makeStatus({
            location: FILE,
            lastStatus: { state: 'failed', code: 'file_missing' },
            lastMessage: 'The spec file was not found on this computer.',
          })}
        />,
      );
      const alert = screen.getByRole('alert');
      await userEvent
        .setup()
        .click(within(alert).getByRole('button', { name: 'Choose another file' }));
      expect(await screen.findByLabelText('New spec link')).toBeInTheDocument();
    });
  });

  describe('next step in the failed-check alert', () => {
    const GITHUB = 'https://github.com/acme/api/blob/main/openapi.yaml';
    const failedWith = (code: string, over: Record<string, unknown> = {}) =>
      makeStatus({
        location: GITHUB,
        lastStatus: { state: 'failed', code },
        lastMessage: 'The check failed.',
        ...over,
      });

    it('a_not_found_failure_offers_add_access_token', () => {
      renderWithQuery(
        <OpenApiSyncHeader collection='c' waiting={0} status={failedWith('not_found')} />,
      );
      const alert = screen.getByRole('alert');
      expect(within(alert).getByRole('button', { name: 'Add access token' })).toBeInTheDocument();
      expect(within(alert).queryByRole('button', { name: 'Check now' })).toBeNull();
    });

    it('no_add_access_token_for_a_non_github_link', () => {
      renderWithQuery(
        <OpenApiSyncHeader
          collection='c'
          waiting={0}
          status={failedWith('not_found', { location: 'https://example.com/o.yaml' })}
        />,
      );
      expect(within(screen.getByRole('alert')).queryByRole('button')).toBeNull();
    });

    it('add_access_token_opens_the_dialog_at_the_token', async () => {
      vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(failedWith('not_found'));
      renderWithQuery(
        <OpenApiSyncHeader collection='c' waiting={0} status={failedWith('not_found')} />,
      );
      await userEvent
        .setup()
        .click(within(screen.getByRole('alert')).getByRole('button', { name: 'Add access token' }));
      const input = await screen.findByLabelText('Access token');
      await waitFor(() => expect(input).toHaveFocus());
    });

    it('a_network_failure_offers_check_now', async () => {
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
      renderWithQuery(
        <OpenApiSyncHeader collection='c' waiting={0} status={failedWith('network')} />,
      );
      const button = within(screen.getByRole('alert')).getByRole('button', { name: 'Check now' });
      await userEvent.setup().click(button);
      await waitFor(() => expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('c'));
    });

    it('check_now_in_the_alert_respects_consent', () => {
      renderWithQuery(
        <OpenApiSyncHeader
          collection='c'
          waiting={0}
          status={failedWith('network', { consent: 'missing' })}
        />,
      );
      expect(within(screen.getByRole('alert')).queryByRole('button')).toBeNull();
    });
  });

  it('hides the query of an Https link behind Show full link', async () => {
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        waiting={0}
        status={makeStatus({ location: 'https://api.example.com/v1/spec.json?token=abc' })}
      />,
    );
    expect(screen.getByText('api.example.com/v1/spec.json')).toBeInTheDocument();
    expect(screen.queryByText(/token=abc/)).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show full link' }));
    expect(screen.getByText('https://api.example.com/v1/spec.json?token=abc')).toBeInTheDocument();
  });

  it('shows_the_chip_location_and_interval', () => {
    renderWithQuery(
      <OpenApiSyncHeader
        collection='c'
        waiting={2}
        status={makeStatus({ lastStatus: { state: 'unchanged' } })}
      />,
    );
    expect(screen.getByText('2 updates')).toBeInTheDocument();
    expect(screen.getByText('https://example.com/openapi.json')).toBeInTheDocument();
    expect(screen.getByText('Check interval: Every day')).toBeInTheDocument();
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

  it('shows_updates_waiting_while_the_count_is_unknown', () => {
    renderWithQuery(<OpenApiSyncHeader collection='c' waiting={null} status={makeStatus()} />);
    expect(screen.getByText('Updates waiting')).toBeInTheDocument();
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

  it('check_now_is_disabled_without_consent', () => {
    renderWithQuery(
      <OpenApiSyncHeader collection='c' waiting={0} status={makeStatus({ consent: 'missing' })} />,
    );
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDisabled();
    expect(screen.getByText('Needs approval')).toBeInTheDocument();
  });

  describe('connection menu', () => {
    beforeAll(() => {
      HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
      HTMLElement.prototype.scrollIntoView = vi.fn();
    });

    it('the_menu_opens_connection_settings', async () => {
      const user = userEvent.setup();
      renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
      await user.click(screen.getByRole('button', { name: 'OpenAPI sync actions' }));
      await user.click(await screen.findByRole('menuitem', { name: 'Connection settings' }));
      expect(await screen.findByRole('dialog')).toHaveTextContent('Connection settings');
    });

    it('a_visible_button_beside_check_now_opens_the_connection_settings', async () => {
      const user = userEvent.setup();
      renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
      await user.click(screen.getByRole('button', { name: 'Connection settings' }));
      expect(await screen.findByRole('dialog')).toHaveTextContent('Connection settings');
    });

    it('the_menu_opens_the_disconnect_dialog', async () => {
      const user = userEvent.setup();
      renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
      await user.click(screen.getByRole('button', { name: 'OpenAPI sync actions' }));
      await user.click(await screen.findByRole('menuitem', { name: 'Disconnect' }));
      expect(await screen.findByRole('alertdialog')).toHaveTextContent('Disconnect from the spec?');
    });

    it('runs a check from the button', async () => {
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
      renderWithQuery(<OpenApiSyncHeader collection='c' status={makeStatus()} waiting={0} />);
      await userEvent.setup().click(screen.getByRole('button', { name: 'Check now' }));
      await waitFor(() => expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('c'));
    });
  });
});
