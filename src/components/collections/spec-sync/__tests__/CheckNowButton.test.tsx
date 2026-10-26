import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { CheckNowButton } from '../CheckNowButton';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, checkOpenApiSyncNow: vi.fn() };
});

describe('CheckNowButton', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('is disabled without consent and says why in a tooltip', async () => {
    renderWithQuery(<CheckNowButton collection='c' consent='missing' />);
    const button = screen.getByRole('button', { name: 'Check now' });
    expect(button).toBeDisabled();
    await userEvent.setup().hover(button.parentElement as HTMLElement);
    expect((await screen.findAllByText(/approve the spec link first/i)).length).toBeGreaterThan(0);
    expect(tauriApi.checkOpenApiSyncNow).not.toHaveBeenCalled();
  });

  it('sends one check on a double click', async () => {
    vi.mocked(tauriApi.checkOpenApiSyncNow).mockImplementation(
      () => new Promise((r) => setTimeout(() => r({ state: 'unchanged' }), 30)),
    );
    renderWithQuery(<CheckNowButton collection='c' consent='granted' />);
    await userEvent.setup().dblClick(screen.getByRole('button', { name: 'Check now' }));
    await waitFor(() => expect(screen.getByText('No changes in the spec.')).toBeInTheDocument());
    expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledTimes(1);
    expect(tauriApi.checkOpenApiSyncNow).toHaveBeenCalledWith('c');
  });

  it('says a new version was found', async () => {
    vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({ state: 'changed' });
    renderWithQuery(<CheckNowButton collection='c' consent='granted' />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Check now' }));
    expect(await screen.findByText('A new version of the spec was found.')).toBeInTheDocument();
  });

  it('shows the fixed message and a retry hint for a failed check', async () => {
    vi.mocked(tauriApi.checkOpenApiSyncNow).mockResolvedValue({
      state: 'failed',
      code: 'rate_limited',
      message: 'The host asked Rocket to slow down.',
      retryAfterSecs: 30,
    });
    renderWithQuery(<CheckNowButton collection='c' consent='granted' />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Check now' }));
    expect(await screen.findByText(/The host asked Rocket to slow down\./)).toBeInTheDocument();
    expect(screen.getByText(/Try again in 30 seconds\./)).toBeInTheDocument();
  });

  it('shows a fixed sentence when the command itself fails', async () => {
    vi.mocked(tauriApi.checkOpenApiSyncNow).mockRejectedValue('consent_required: x');
    renderWithQuery(<CheckNowButton collection='c' consent='granted' />);
    await userEvent.setup().click(screen.getByRole('button', { name: 'Check now' }));
    expect(
      await screen.findByText(/Approve this spec link on this machine first/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/consent_required/)).toBeNull();
  });
});
