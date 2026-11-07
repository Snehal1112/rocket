import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IncomingTransferToast } from '@/components/network/IncomingTransferToast';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { dismiss: vi.fn(), custom: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), requestNetworkPairing: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

function setup(patch: Partial<React.ComponentProps<typeof IncomingTransferToast>> = {}) {
  render(
    <IncomingTransferToast
      transferId='t1'
      displayName='Dana Roy'
      shortId='DDDD-4444'
      collectionName='Orders'
      sizeBytes={2048}
      {...patch}
    />,
  );
}

describe('IncomingTransferToast', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('names the sender, the short id, the collection and the size', () => {
    setup();
    expect(screen.getByText('Dana Roy wants to send you a collection')).toBeInTheDocument();
    expect(screen.getByText('DDDD-4444')).toBeInTheDocument();
    expect(screen.getByText('Orders (2 KiB)')).toBeInTheDocument();
  });

  it('falls back when the display name is empty', () => {
    setup({ displayName: '' });
    expect(screen.getByText('Unnamed node wants to send you a collection')).toBeInTheDocument();
  });

  it('shows a zero byte size', () => {
    setup({ sizeBytes: 0 });
    expect(screen.getByText('Orders (0 bytes)')).toBeInTheDocument();
  });

  it('shows a collection name with a double quote as given', () => {
    setup({ collectionName: 'Orders "v2"' });
    expect(screen.getByText('Orders "v2" (2 KiB)')).toBeInTheDocument();
  });

  it('Review opens the transfer dialog and dismisses the toast', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Review' }));
    expect(useNetworkStore.getState().dialogTransferId).toBe('t1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-offer-t1');
  });

  it('has only a Review action', () => {
    setup();
    expect(screen.getAllByRole('button').map((b) => b.textContent)).toEqual(['Review']);
    expect(screen.queryByRole('button', { name: 'Later' })).not.toBeInTheDocument();
  });
});
