import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { TrustMigrationNotice } from '../TrustMigrationNotice';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, getTrustMigrationNotice: vi.fn(), dismissTrustMigrationNotice: vi.fn() };
});

const notice = {
  collections: [
    { name: 'payments-api', path: '/w/payments-api', capabilities: ['developerMode', 'agentRun'] },
    { name: 'orders', path: '/w/orders', capabilities: ['contextRoots'] },
  ],
};

describe('TrustMigrationNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriApi.dismissTrustMigrationNotice).mockResolvedValue(undefined);
  });

  it('shows nothing when no collection kept extra access', async () => {
    vi.mocked(tauriApi.getTrustMigrationNotice).mockResolvedValue({ collections: [] });
    render(<TrustMigrationNotice />);
    await waitFor(() => expect(tauriApi.getTrustMigrationNotice).toHaveBeenCalled());
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });

  it('lists the collections and what they kept', async () => {
    vi.mocked(tauriApi.getTrustMigrationNotice).mockResolvedValue(notice);
    render(<TrustMigrationNotice />);
    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('payments-api (Developer mode, agent runs)');
    expect(dialog).toHaveTextContent('orders (extra script folders)');
  });

  it('OK dismisses the notice for good', async () => {
    vi.mocked(tauriApi.getTrustMigrationNotice).mockResolvedValue(notice);
    render(<TrustMigrationNotice />);
    await userEvent.click(await screen.findByRole('button', { name: 'OK' }));
    await waitFor(() => expect(tauriApi.dismissTrustMigrationNotice).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument());
  });

  it('Review opens the first collection, closes the dialog and does not dismiss', async () => {
    vi.mocked(tauriApi.getTrustMigrationNotice).mockResolvedValue(notice);
    const open = vi.spyOn(usePaneStore.getState(), 'openCollectionTab').mockReturnValue(true);
    render(<TrustMigrationNotice />);
    await userEvent.click(await screen.findByRole('button', { name: 'Review' }));
    expect(open).toHaveBeenCalledWith('payments-api', 'overview');
    expect(tauriApi.dismissTrustMigrationNotice).not.toHaveBeenCalled();
  });
});
