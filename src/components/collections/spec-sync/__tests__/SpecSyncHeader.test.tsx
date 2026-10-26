import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { SpecSyncHeader } from '../SpecSyncHeader';
import { makeStatus } from './fixtures';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, checkOpenApiSyncNow: vi.fn() };
});

describe('SpecSyncHeader', () => {
  it('shows the location, last check time and interval', () => {
    renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
    expect(screen.getByText('https://example.com/openapi.json')).toBeInTheDocument();
    expect(screen.getByText('Last checked 2h ago')).toBeInTheDocument();
    expect(screen.getByText('Check interval: Every day')).toBeInTheDocument();
  });

  it('has a Check now button', () => {
    renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
    expect(screen.getByRole('button', { name: 'Check now' })).toBeEnabled();
  });

  it('disables Check now without consent', () => {
    renderWithQuery(
      <SpecSyncHeader collection='c' status={makeStatus({ consent: 'missing' })} />,
    );
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDisabled();
  });

  it('says so when it was never checked', () => {
    renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus({ lastCheckedAt: null })} />);
    expect(screen.getByText('Not checked yet')).toBeInTheDocument();
  });

  it('shows the message of a failed check in an alert', () => {
    renderWithQuery(
      <SpecSyncHeader
        collection='c'
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
      <SpecSyncHeader
        collection='c'
        status={makeStatus({ location: 'https://api.example.com/v1/spec.json?token=abc' })}
      />,
    );
    expect(screen.getByText('api.example.com/v1/spec.json')).toBeInTheDocument();
    expect(screen.queryByText(/token=abc/)).toBeNull();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Show full link' }));
    expect(screen.getByText('https://api.example.com/v1/spec.json?token=abc')).toBeInTheDocument();
  });

  describe('connection menu', () => {
    beforeAll(() => {
      HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
      HTMLElement.prototype.scrollIntoView = vi.fn();
    });

    it('has a title and a menu with the two entries', async () => {
      const user = userEvent.setup();
      renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
      expect(screen.getByText('Spec sync')).toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Spec sync actions' }));
      expect(await screen.findByRole('menuitem', { name: 'Edit connection settings' })).toBeInTheDocument();
      expect(screen.getByRole('menuitem', { name: 'Check now' })).toBeInTheDocument();
    });

    it('opens the connection dialog from the menu', async () => {
      const user = userEvent.setup();
      renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
      await user.click(screen.getByRole('button', { name: 'Spec sync actions' }));
      await user.click(await screen.findByRole('menuitem', { name: 'Edit connection settings' }));
      expect(await screen.findByRole('dialog')).toHaveTextContent('Edit connection settings');
    });

    it('runs a check from the menu', async () => {
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
      const user = userEvent.setup();
      renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
      await user.click(screen.getByRole('button', { name: 'Spec sync actions' }));
      await user.click(await screen.findByRole('menuitem', { name: 'Check now' }));
      expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('c');
    });

    it('shares the check state between the menu and the button', async () => {
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'unchanged' });
      const user = userEvent.setup();
      renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
      await user.click(screen.getByRole('button', { name: 'Spec sync actions' }));
      await user.click(await screen.findByRole('menuitem', { name: 'Check now' }));
      expect(await screen.findByText('No changes in the spec.')).toBeInTheDocument();
    });

    it('disables the button while a menu check runs', async () => {
      vi.mocked(tauriApi.checkOpenApiSyncNow).mockReturnValue(new Promise(() => undefined));
      const user = userEvent.setup();
      renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus()} />);
      await user.click(screen.getByRole('button', { name: 'Spec sync actions' }));
      await user.click(await screen.findByRole('menuitem', { name: 'Check now' }));
      await waitFor(() => expect(screen.getByRole('button', { name: 'Check now' })).toBeDisabled());
    });

    it('disables the menu check without consent', async () => {
      const user = userEvent.setup();
      renderWithQuery(<SpecSyncHeader collection='c' status={makeStatus({ consent: 'missing' })} />);
      await user.click(screen.getByRole('button', { name: 'Spec sync actions' }));
      expect(await screen.findByRole('menuitem', { name: 'Check now' })).toHaveAttribute('data-disabled');
    });
  });
});
