import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { OpenApiSyncHeader } from '../OpenApiSyncHeader';
import { makeStatus } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, checkOpenApiSyncNow: vi.fn() };
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
      await user.click(await screen.findByRole('menuitem', { name: 'Edit connection settings' }));
      expect(await screen.findByRole('dialog')).toHaveTextContent('Edit connection settings');
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
