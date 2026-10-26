import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { renderWithQuery } from '@/test/trust-fixtures';
import { SpecSyncSettings } from '../SpecSyncSettings';
import { makeStatus } from './fixtures';

const GH = 'https://github.com/acme/orders/blob/main/openapi.yaml';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getOpenApiSyncStatus: vi.fn(),
    setOpenApiSyncInterval: vi.fn(),
    setOpenApiSyncToken: vi.fn(),
    clearOpenApiSyncToken: vi.fn(),
    revokeOpenApiSyncConsent: vi.fn(),
  };
});

async function open() {
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: /Settings/ }));
  return user;
}

describe('SpecSyncSettings', () => {
  // jsdom lacks the pointer and scroll APIs the Radix Select relies on.
  beforeAll(() => {
    HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
    HTMLElement.prototype.setPointerCapture = vi.fn();
    HTMLElement.prototype.releasePointerCapture = vi.fn();
    HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasToken: false }));
  });

  it('asks for hasToken only while the section is open', async () => {
    renderWithQuery(<SpecSyncSettings collection='s1' status={makeStatus()} />);
    expect(tauriApi.getOpenApiSyncStatus).not.toHaveBeenCalled();
    await open();
    await waitFor(() => expect(tauriApi.getOpenApiSyncStatus).toHaveBeenCalledWith('s1', true));
  });

  it('disables the interval until the link is approved', async () => {
    renderWithQuery(
      <SpecSyncSettings collection='s2' status={makeStatus({ consent: 'missing' })} />,
    );
    await open();
    expect(screen.getByRole('combobox', { name: 'Check interval' })).toBeDisabled();
  });

  it('sets the interval', async () => {
    vi.mocked(tauriApi.setOpenApiSyncInterval).mockResolvedValue(undefined);
    renderWithQuery(<SpecSyncSettings collection='s3' status={makeStatus()} />);
    const user = await open();
    await user.click(screen.getByRole('combobox', { name: 'Check interval' }));
    await user.click(await screen.findByRole('option', { name: 'Every week' }));
    expect(tauriApi.setOpenApiSyncInterval).toHaveBeenCalledWith('s3', 'weekly');
  });

  it('keeps the token field write-only, empty and cleared after save', async () => {
    vi.mocked(tauriApi.setOpenApiSyncToken).mockResolvedValue(undefined);
    renderWithQuery(<SpecSyncSettings collection='s4' status={makeStatus({ location: GH })} />);
    const user = await open();
    const input = await screen.findByLabelText('Access token');
    expect(input).toHaveAttribute('type', 'password');
    expect(input).toHaveValue('');
    await user.type(input, 'ghp_secret');
    await user.click(screen.getByRole('button', { name: 'Save token' }));
    expect(tauriApi.setOpenApiSyncToken).toHaveBeenCalledWith('s4', 'ghp_secret');
    await waitFor(() => expect(screen.getByLabelText('Access token')).toHaveValue(''));
  });

  it('shows the consent sentence when saving without approval', async () => {
    vi.mocked(tauriApi.setOpenApiSyncToken).mockRejectedValue('consent_required: no grant');
    renderWithQuery(
      <SpecSyncSettings
        collection='s5'
        status={makeStatus({ consent: 'missing', location: GH })}
      />,
    );
    const user = await open();
    await user.type(await screen.findByLabelText('Access token'), 'tok');
    await user.click(screen.getByRole('button', { name: 'Save token' }));
    expect(
      await screen.findByText(/Approve this spec link on this machine first/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/consent_required/)).toBeNull();
    expect(screen.getByLabelText('Access token')).toHaveValue('');
  });

  it('shows that a token is stored and clears it', async () => {
    vi.mocked(tauriApi.getOpenApiSyncStatus).mockResolvedValue(makeStatus({ hasToken: true }));
    vi.mocked(tauriApi.clearOpenApiSyncToken).mockResolvedValue(undefined);
    renderWithQuery(<SpecSyncSettings collection='s6' status={makeStatus({ location: GH })} />);
    const user = await open();
    expect(await screen.findByText('A token is stored')).toBeInTheDocument();
    expect(screen.getByLabelText('Access token')).toHaveValue('');
    await user.click(screen.getByRole('button', { name: 'Clear' }));
    expect(tauriApi.clearOpenApiSyncToken).toHaveBeenCalledWith('s6');
  });

  it('stops trusting the link', async () => {
    vi.mocked(tauriApi.revokeOpenApiSyncConsent).mockResolvedValue(undefined);
    renderWithQuery(<SpecSyncSettings collection='s7' status={makeStatus()} />);
    const user = await open();
    await user.click(screen.getByRole('button', { name: 'Stop trusting this link' }));
    expect(tauriApi.revokeOpenApiSyncConsent).toHaveBeenCalledWith('s7');
  });

  it('leaves no token in the mutation cache after the save settles', async () => {
    vi.mocked(tauriApi.setOpenApiSyncToken).mockResolvedValue(undefined);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={queryClient}>
        <SpecSyncSettings collection='s8' status={makeStatus({ location: GH })} />
      </QueryClientProvider>,
    );
    const user = await open();
    await user.type(await screen.findByLabelText('Access token'), 'ghp_secret');
    await user.click(screen.getByRole('button', { name: 'Save token' }));
    await waitFor(() => expect(tauriApi.setOpenApiSyncToken).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByLabelText('Access token')).toHaveValue(''));
    await waitFor(() =>
      expect(JSON.stringify(queryClient.getMutationCache().getAll().map((m) => m.state))).not.toContain(
        'ghp_secret',
      ),
    );
  });

  it('hides Clear when no token is stored', async () => {
    renderWithQuery(<SpecSyncSettings collection='s9' status={makeStatus({ location: GH })} />);
    await open();
    await screen.findByLabelText('Access token');
    expect(screen.queryByRole('button', { name: 'Clear' })).toBeNull();
    expect(screen.queryByText('A token is stored')).toBeNull();
  });

  it('hides the token field for a link that is not on GitHub', async () => {
    renderWithQuery(<SpecSyncSettings collection='s10' status={makeStatus()} />);
    await open();
    expect(screen.getByText('Tokens work with GitHub links only.')).toBeInTheDocument();
    expect(screen.queryByLabelText('Access token')).toBeNull();
  });

  it('uses new-password autocomplete on the token field', async () => {
    renderWithQuery(<SpecSyncSettings collection='s11' status={makeStatus({ location: GH })} />);
    await open();
    expect(await screen.findByLabelText('Access token')).toHaveAttribute('autocomplete', 'new-password');
  });

  it('hides Stop trusting after approval is gone and says the settings come back', async () => {
    renderWithQuery(
      <SpecSyncSettings collection='s12' status={makeStatus({ consent: 'missing' })} />,
    );
    await open();
    expect(screen.queryByRole('button', { name: 'Stop trusting this link' })).toBeNull();
    expect(
      screen.getByText(/stored interval and token come back when the link is approved again/),
    ).toBeInTheDocument();
  });
});
