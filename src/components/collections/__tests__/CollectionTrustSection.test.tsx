import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { allowed, makeTrust, renderWithQuery, requestedOnly } from '@/test/trust-fixtures';
import { CollectionTrustSection } from '../CollectionTrustSection';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionTrust: vi.fn(),
    setCollectionCapability: vi.fn(),
    grantRequestedCapabilities: vi.fn(),
    revokeCollectionTrust: vi.fn(),
  };
});

function row(name: RegExp): HTMLElement {
  return screen.getByRole('row', { name });
}

describe('CollectionTrustSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.setCollectionCapability).mockResolvedValue(makeTrust());
    vi.mocked(tauriApi.grantRequestedCapabilities).mockResolvedValue(makeTrust());
    vi.mocked(tauriApi.revokeCollectionTrust).mockResolvedValue(makeTrust());
  });

  it('shows requested and allowed values per capability', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ developerMode: requestedOnly, agentRun: allowed, pending: true }),
    );
    renderWithQuery(<CollectionTrustSection collection='c' />);
    const dev = await screen.findByRole('row', { name: /Developer mode/ });
    expect(within(dev).getAllByRole('cell').map((c) => c.textContent)).toEqual([
      'Developer mode (scripts get file and command access)',
      'Yes',
      'No',
      'Allow...',
    ]);
    expect(within(row(/Agent may run requests/)).getByRole('button', { name: 'Revoke' })).toBeInTheDocument();
  });

  it('allows a requested capability with its fingerprint after confirming', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({ developerMode: requestedOnly, pending: true, fingerprint: 'fp-9' }),
    );
    renderWithQuery(<CollectionTrustSection collection='c' />);
    const user = userEvent.setup();
    await user.click(
      within(await screen.findByRole('row', { name: /Developer mode/ })).getByRole('button', {
        name: 'Allow...',
      }),
    );
    expect(tauriApi.grantRequestedCapabilities).not.toHaveBeenCalled();
    await user.click(await screen.findByRole('button', { name: 'Allow' }));
    await waitFor(() =>
      expect(tauriApi.grantRequestedCapabilities).toHaveBeenCalledWith(
        'c',
        ['developerMode'],
        'fp-9',
      ),
    );
  });

  it('revokes an allowed capability and forgets all permissions', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(makeTrust({ agentRun: allowed }));
    renderWithQuery(<CollectionTrustSection collection='c' />);
    const user = userEvent.setup();
    await user.click(
      within(await screen.findByRole('row', { name: /Agent may run requests/ })).getByRole(
        'button',
        { name: 'Revoke' },
      ),
    );
    await waitFor(() =>
      expect(tauriApi.setCollectionCapability).toHaveBeenCalledWith('c', 'agentRun', false),
    );
    await user.click(screen.getByRole('button', { name: /Forget this collection/ }));
    await waitFor(() => expect(tauriApi.revokeCollectionTrust).toHaveBeenCalledWith('c'));
  });

  it('marks a pending root', async () => {
    vi.mocked(tauriApi.getCollectionTrust).mockResolvedValue(
      makeTrust({
        contextRoots: { requested: ['new'], granted: [], effective: [], pending: ['new'] },
        pending: true,
      }),
    );
    renderWithQuery(<CollectionTrustSection collection='c' />);
    expect(await screen.findByText('Pending')).toBeInTheDocument();
  });
});
