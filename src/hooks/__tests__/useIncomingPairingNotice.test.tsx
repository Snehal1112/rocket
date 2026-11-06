import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useIncomingPairingNotice } from '@/hooks/useIncomingPairingNotice';
import { INCOMING_TTL_MS, useNetworkStore } from '@/stores/network-store';
import { makeIncoming } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { custom: vi.fn(), dismiss: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), requestNetworkPairing: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const request = (pairingId: string) => makeIncoming({ pairingId });

describe('useIncomingPairingNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useNetworkStore.getState().reset();
  });

  it('shows one toast with a fixed id and the request lifetime', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.getState().addIncoming(request('p1'));
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
    expect(mocks.toast.custom).toHaveBeenCalledWith(expect.any(Function), {
      id: 'network-pairing-p1',
      duration: INCOMING_TTL_MS,
    });
  });

  it('shows no second toast for a duplicate event', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.getState().addIncoming(request('p1'));
    useNetworkStore.getState().addIncoming(request('p1'));
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('shows one toast per request, with different ids', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.getState().addIncoming(request('p1'));
    useNetworkStore.getState().addIncoming(request('p2'));
    const ids = mocks.toast.custom.mock.calls.map((call) => (call[1] as { id: string }).id);
    expect(ids).toEqual(['network-pairing-p1', 'network-pairing-p2']);
  });

  it('dismisses the toast when the request goes away', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.getState().addIncoming(request('p1'));
    useNetworkStore.getState().removeIncoming('p1');
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-pairing-p1');
  });

  it('does not react to changes that leave the requests alone', () => {
    renderHook(() => useIncomingPairingNotice());
    useNetworkStore.setState({ panelOpen: true });
    expect(mocks.toast.custom).not.toHaveBeenCalled();
    expect(mocks.toast.dismiss).not.toHaveBeenCalled();
  });

  it('shows a toast for a request that exists at mount', () => {
    useNetworkStore.getState().addIncoming(request('p1'));
    renderHook(() => useIncomingPairingNotice());
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('stops listening after unmount', () => {
    const view = renderHook(() => useIncomingPairingNotice());
    view.unmount();
    useNetworkStore.getState().addIncoming(request('p1'));
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });
});
