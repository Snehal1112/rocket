import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IncomingPairingToast } from '@/components/network/IncomingPairingToast';
import { useNetworkStore } from '@/stores/network-store';

const mocks = vi.hoisted(() => ({
  toast: { dismiss: vi.fn(), custom: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), requestNetworkPairing: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

function setup() {
  render(<IncomingPairingToast pairingId='p1' displayName='Dana Roy' shortId='DDDD-4444' />);
}

describe('IncomingPairingToast', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('names the sender and its short id, but not the code', () => {
    setup();
    expect(screen.getByText('Dana Roy wants to pair')).toBeInTheDocument();
    expect(screen.getByText('DDDD-4444')).toBeInTheDocument();
  });

  it('Review opens the pairing dialog and dismisses the toast', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Review' }));
    expect(useNetworkStore.getState().dialogPairingId).toBe('p1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-pairing-p1');
  });

  it('Later only dismisses the toast', async () => {
    setup();
    await userEvent.click(screen.getByRole('button', { name: 'Later' }));
    expect(useNetworkStore.getState().dialogPairingId).toBeNull();
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-pairing-p1');
  });
});
