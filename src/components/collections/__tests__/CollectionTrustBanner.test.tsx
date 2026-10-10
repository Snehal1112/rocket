import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { makeTrust, renderWithQuery, requestedOnly } from '@/test/trust-fixtures';
import { CollectionTrustBanner } from '../CollectionTrustBanner';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getCollectionTrust: vi.fn(), grantRequestedCapabilities: vi.fn() };
});

const pendingTrust = () =>
  makeTrust({
    developerMode: requestedOnly,
    agentRun: requestedOnly,
    pending: true,
    fingerprint: 'fp-seen',
  });

describe('CollectionTrustBanner', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders nothing when nothing is pending', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(makeTrust());
    const { container } = renderWithQuery(<CollectionTrustBanner collection='c' />);
    await waitFor(() => expect(tauriApi.getCollectionTrust).toHaveBeenCalledWith('c'));
    expect(container).toBeEmptyDOMElement();
  });

  it('lists what the collection asks for', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(pendingTrust());
    renderWithQuery(<CollectionTrustBanner collection='c' />);
    expect(
      await screen.findByText('This collection asks for more access than it has on this computer.'),
    ).toBeInTheDocument();
    expect(screen.getByText(/Developer mode, Agent request runs/)).toBeInTheDocument();
  });

  it('sends the fingerprint the user saw with the selected capabilities', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(pendingTrust());
    vi.mocked(tauriApi.grantRequestedCapabilities).mockResolvedValue(makeTrust());
    renderWithQuery(<CollectionTrustBanner collection='c' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Review...' }));
    const dialog = await screen.findByRole('alertdialog');
    const allow = within(dialog).getByRole('button', { name: 'Allow selected' });
    expect(allow).toBeDisabled();
    await user.click(within(dialog).getByRole('checkbox', { name: /Developer mode/ }));
    await user.click(allow);
    await waitFor(() =>
      expect(tauriApi.grantRequestedCapabilities).toHaveBeenCalledWith(
        'c',
        ['developerMode'],
        'fp-seen',
      ),
    );
  });

  it('shows the refusal when the request changed in between', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(pendingTrust());
    vi.mocked(tauriApi.grantRequestedCapabilities).mockRejectedValue(
      new Error("This collection's settings changed. Review them again."),
    );
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    renderWithQuery(<CollectionTrustBanner collection='c' />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Review...' }));
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('checkbox', { name: /Agent request runs/ }));
    await user.click(within(dialog).getByRole('button', { name: 'Allow selected' }));
    expect(await screen.findByText(/settings changed/)).toBeInTheDocument();
  });

  it('shows a destructive alert when the trust settings cannot be read', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(makeTrust({ storeError: 'corrupt' }));
    renderWithQuery(<CollectionTrustBanner collection='c' />);
    expect(
      await screen.findByText(/Rocket could not read its trust settings/),
    ).toBeInTheDocument();
  });
});
