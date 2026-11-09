import { beforeEach, describe, expect, it, vi } from 'vitest';
import { startCollectionSync } from '@/lib/network-sync-actions';
import { syncWaitingToastId } from '@/lib/network-sync-utils';
import { useNetworkStore } from '@/stores/network-store';
import { makeLink } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { info: vi.fn(), error: vi.fn(), success: vi.fn(), dismiss: vi.fn() },
  api: {
    startNetworkSync: vi.fn(),
    cancelNetworkSync: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

const waitingOptions = () =>
  mocks.toast.info.mock.calls[0]?.[1] as {
    id: string;
    duration: number;
    action: { label: string; onClick: () => void };
  };

describe('startCollectionSync', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.startNetworkSync.mockResolvedValue({ syncId: 's1' });
    mocks.api.cancelNetworkSync.mockResolvedValue(undefined);
  });

  it('starts the sync and shows a waiting toast with Cancel', async () => {
    await startCollectionSync(makeLink());
    expect(mocks.api.startNetworkSync).toHaveBeenCalledWith('L1');
    expect(mocks.toast.info).toHaveBeenCalledWith(
      'Waiting for Ben Ortiz to review the sync.',
      expect.objectContaining({ id: syncWaitingToastId('s1'), duration: Number.POSITIVE_INFINITY }),
    );
    expect(waitingOptions().action.label).toBe('Cancel');
    expect(store().startedSync('s1')).toEqual({
      displayName: 'Ben Ortiz',
      collectionName: 'Orders',
    });
  });

  it('shows no waiting toast when the sync finished before the call returned', async () => {
    store().finishSync('s1');
    await startCollectionSync(makeLink());
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });

  it('Cancel cancels that sync and remembers it was local', async () => {
    await startCollectionSync(makeLink());
    waitingOptions().action.onClick();
    expect(mocks.api.cancelNetworkSync).toHaveBeenCalledWith('s1');
    expect(store().wasSyncCancelledHere('s1')).toBe(true);
  });

  it('shows the error and forgets the local cancel when the cancel is refused', async () => {
    mocks.api.cancelNetworkSync.mockRejectedValue('The sync is already being applied.');
    await startCollectionSync(makeLink());
    waitingOptions().action.onClick();
    await vi.waitFor(() => expect(mocks.toast.error).toHaveBeenCalled());
    expect(store().wasSyncCancelledHere('s1')).toBe(false);
  });

  it('rejects with the backend error and shows no toast', async () => {
    mocks.api.startNetworkSync.mockRejectedValue('A sync with that node is already running.');
    await expect(startCollectionSync(makeLink())).rejects.toBe(
      'A sync with that node is already running.',
    );
    expect(mocks.toast.info).not.toHaveBeenCalled();
  });
});
