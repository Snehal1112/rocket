import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { incomingSyncToastId } from '@/components/network/IncomingSyncToast';
import { useIncomingSyncNotice } from '@/hooks/useIncomingSyncNotice';
import { useNetworkStore } from '@/stores/network-store';
import { makeSyncReview } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({ toast: { custom: vi.fn(), dismiss: vi.fn() } }));
vi.mock('sonner', () => ({ toast: mocks.toast }));
vi.mock('@/lib/tauri-api', () => ({}));

const store = () => useNetworkStore.getState();
const customIds = () => mocks.toast.custom.mock.calls.map((c) => c[1].id);

describe('useIncomingSyncNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
  });

  it('shows one toast per entry, with different ids', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview({ syncId: 's1' }));
      store().addSyncReview(makeSyncReview({ syncId: 's2' }));
    });
    expect(customIds()).toEqual([incomingSyncToastId('s1'), incomingSyncToastId('s2')]);
  });

  it('shows no second toast for a duplicate review', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview());
      store().addSyncReview(makeSyncReview());
    });
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('shows the toast of an entry that exists at mount', () => {
    store().addSyncReview(makeSyncReview());
    renderHook(() => useIncomingSyncNotice());
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('shows no toast for an answered entry', () => {
    store().addSyncReview(makeSyncReview({ answered: true }));
    renderHook(() => useIncomingSyncNotice());
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });

  it('shows no toast for the entry whose dialog is already open', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      useNetworkStore.setState({ dialogSyncId: 's1' });
      store().addSyncReview(makeSyncReview({ kind: 'confirm' }));
    });
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });

  it('dismisses the toast when the entry goes away', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview());
      store().finishSync('s1');
    });
    expect(mocks.toast.dismiss).toHaveBeenCalledWith(incomingSyncToastId('s1'));
  });

  it('brings the toast back when the dialog closes without an answer', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview());
      store().openSyncDialog('s1');
    });
    mocks.toast.custom.mockClear();
    act(() => store().closeSyncDialog());
    expect(mocks.toast.custom).toHaveBeenCalledTimes(1);
  });

  it('does not bring a toast back for an answered entry', () => {
    renderHook(() => useIncomingSyncNotice());
    act(() => {
      store().addSyncReview(makeSyncReview());
      store().markSyncAnswered('s1');
      store().openSyncDialog('s1');
    });
    mocks.toast.custom.mockClear();
    act(() => store().closeSyncDialog());
    expect(mocks.toast.custom).not.toHaveBeenCalled();
  });
});
