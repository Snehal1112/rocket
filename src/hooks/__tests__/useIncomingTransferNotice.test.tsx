import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useIncomingTransferNotice } from '@/hooks/useIncomingTransferNotice';
import { INCOMING_TTL_MS, useNetworkStore } from '@/stores/network-store';
import { makeIncomingTransfer } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { custom: vi.fn(), dismiss: vi.fn() },
  api: { getNetworkStatus: vi.fn(), listNetworkNodes: vi.fn(), requestNetworkPairing: vi.fn() },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();
const offer = (transferId: string) => makeIncomingTransfer({ transferId });
const toastIds = () => mocks.toast.custom.mock.calls.map((call) => (call[1] as { id: string }).id);

describe('useIncomingTransferNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  it('shows one toast with a fixed id and the offer lifetime', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
    expect(mocks.toast.custom).toHaveBeenCalledWith(expect.any(Function), {
      id: 'network-transfer-offer-t1',
      duration: INCOMING_TTL_MS,
    });
  });

  it('shows no second toast for a duplicate offer', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => {
      store().addIncomingTransfer(offer('t1'));
      store().addIncomingTransfer(offer('t1'));
    });
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('shows one toast per offer, with different ids', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => {
      store().addIncomingTransfer(offer('t1'));
      store().addIncomingTransfer(offer('t2'));
    });
    expect(toastIds()).toEqual(['network-transfer-offer-t1', 'network-transfer-offer-t2']);
  });

  it('dismisses the toast when the offer goes away', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    act(() => store().removeIncomingTransfer('t1'));
    expect(mocks.toast.dismiss).toHaveBeenCalledWith('network-transfer-offer-t1');
  });

  it('does not react to changes that leave the offers alone', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => useNetworkStore.setState({ panelOpen: true }));
    expect(mocks.toast.custom).not.toHaveBeenCalled();
    expect(mocks.toast.dismiss).not.toHaveBeenCalled();
  });

  it('shows a toast for a pending offer that exists at mount, but not for an accepted one', () => {
    store().addIncomingTransfer(offer('t1'));
    store().addIncomingTransfer(offer('t2'));
    store().markTransferAccepted('t2');
    renderHook(() => useIncomingTransferNotice());
    expect(toastIds()).toEqual(['network-transfer-offer-t1']);
  });

  it('brings the toast back when the dialog closes without an answer', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    act(() => store().openTransferDialog('t1'));
    mocks.toast.custom.mockClear();
    act(() => store().closeTransferDialog());
    expect(toastIds()).toEqual(['network-transfer-offer-t1']);
  });

  it('does not bring the toast back after an answer', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    act(() => store().openTransferDialog('t1'));
    mocks.toast.custom.mockClear();
    act(() => {
      store().removeIncomingTransfer('t1');
      store().closeTransferDialog();
    });
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });

  it('does not bring the toast back for an accepted offer', () => {
    renderHook(() => useIncomingTransferNotice());
    act(() => store().addIncomingTransfer(offer('t1')));
    act(() => store().openTransferDialog('t1'));
    mocks.toast.custom.mockClear();
    act(() => {
      store().markTransferAccepted('t1');
      store().closeTransferDialog();
    });
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });

  it('stops listening after unmount', () => {
    const view = renderHook(() => useIncomingTransferNotice());
    view.unmount();
    act(() => store().addIncomingTransfer(offer('t1')));
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });
});
