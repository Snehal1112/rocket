import { beforeEach, describe, expect, it, vi } from 'vitest';
import { handleNetworkSync } from '@/lib/network-event-bridge';
import { syncResultToastId, syncWaitingToastId } from '@/lib/network-sync-utils';
import type { NetworkSyncEvent } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';
import { makeSyncSummary } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  toast: { info: vi.fn(), success: vi.fn(), error: vi.fn(), dismiss: vi.fn() },
  api: {
    listNetworkLinks: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);
vi.mock('sonner', () => ({ toast: mocks.toast }));

const store = () => useNetworkStore.getState();

type Finished = Extract<NetworkSyncEvent, { type: 'finished' }>;

const reviewReady = (patch: Record<string, unknown> = {}): NetworkSyncEvent =>
  ({
    type: 'reviewReady',
    sync_id: 's1',
    link_id: 'L1',
    device_id: 'DEV1',
    short_id: 'AAAA-1111',
    display_name: 'Ben Ortiz',
    collection_name: 'Orders',
    summary: makeSyncSummary(),
    conflicts: [],
    role: 'planner',
    ...patch,
  }) as NetworkSyncEvent;

const confirmNeeded = (patch: Record<string, unknown> = {}): NetworkSyncEvent =>
  ({
    type: 'confirmNeeded',
    sync_id: 's1',
    link_id: 'L1',
    device_id: 'DEV1',
    short_id: 'AAAA-1111',
    display_name: 'Ben Ortiz',
    collection_name: 'Orders',
    summary: makeSyncSummary(),
    ...patch,
  }) as NetworkSyncEvent;

const finished = (patch: Partial<Finished> = {}): Finished => ({
  type: 'finished',
  sync_id: 's1',
  link_id: 'L1',
  display_name: 'Ben Ortiz',
  collection_name: 'Orders',
  outcome: 'applied',
  message: null,
  ...patch,
});

describe('handleNetworkSync', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.listNetworkLinks.mockResolvedValue([]);
  });

  describe('reviews', () => {
    it('adds a review from the planner event', () => {
      handleNetworkSync(
        reviewReady({
          conflicts: [{ path: 'a.yml', kind: 'both_changed', mineHash: 'h1', theirsHash: 'h2' }],
        }),
      );
      const [entry] = store().syncReviews;
      expect(entry).toMatchObject({
        syncId: 's1',
        linkId: 'L1',
        deviceId: 'DEV1',
        shortId: 'AAAA-1111',
        displayName: 'Ben Ortiz',
        collectionName: 'Orders',
        kind: 'review',
        answered: false,
      });
      expect(entry.conflicts).toHaveLength(1);
    });

    it('adds one row for a repeated review', () => {
      handleNetworkSync(reviewReady());
      handleNetworkSync(reviewReady());
      expect(store().syncReviews).toHaveLength(1);
    });

    it('does not open a planner review by itself', () => {
      handleNetworkSync(reviewReady());
      expect(store().dialogSyncId).toBeNull();
    });

    it('treats a review for the starter role as a confirm', () => {
      handleNetworkSync(reviewReady({ role: 'starter' }));
      expect(store().syncReviews[0].kind).toBe('confirm');
    });

    it('opens the confirm dialog when no sync dialog is open', () => {
      handleNetworkSync(confirmNeeded());
      expect(store().syncReviews[0]).toMatchObject({ kind: 'confirm', conflicts: [] });
      expect(store().dialogSyncId).toBe('s1');
    });

    it('does not take over another open sync dialog', () => {
      handleNetworkSync(reviewReady({ sync_id: 'other' }));
      store().openSyncDialog('other');
      handleNetworkSync(confirmNeeded());
      expect(store().dialogSyncId).toBe('other');
      expect(store().syncReviews).toHaveLength(2);
    });

    it('keeps a double quote in the collection name', () => {
      handleNetworkSync(reviewReady({ collection_name: 'Orders "v2"' }));
      expect(store().syncReviews[0].collectionName).toBe('Orders "v2"');
    });
  });

  describe('progress and links', () => {
    it('stores progress', () => {
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 5, total_bytes: 10 });
      expect(store().syncProgress).toEqual([{ syncId: 's1', doneBytes: 5, totalBytes: 10 }]);
    });

    it('switches the waiting toast of a started sync to running once', () => {
      store().trackStartedSync('s1', { displayName: 'Ben Ortiz', collectionName: 'Orders' });
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 1, total_bytes: 10 });
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 2, total_bytes: 10 });
      expect(mocks.toast.info).toHaveBeenCalledTimes(1);
      expect(mocks.toast.info).toHaveBeenCalledWith('Syncing with Ben Ortiz...', {
        id: syncWaitingToastId('s1'),
        duration: Number.POSITIVE_INFINITY,
      });
    });

    it('leaves the toasts alone for a sync it did not start', () => {
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 1, total_bytes: 10 });
      expect(mocks.toast.info).not.toHaveBeenCalled();
    });

    it('reloads the links on linkChanged', () => {
      handleNetworkSync({ type: 'linkChanged', link_id: 'L1' });
      expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
    });
  });

  describe('late events', () => {
    it('ignores progress and reviews that arrive after the finish', () => {
      handleNetworkSync(finished());
      handleNetworkSync({ type: 'progress', sync_id: 's1', done_bytes: 1, total_bytes: 2 });
      handleNetworkSync(reviewReady());
      expect(store().syncProgress).toHaveLength(0);
      expect(store().syncReviews).toHaveLength(0);
    });

    it('a finish for another sync leaves the open dialog alone', () => {
      handleNetworkSync(reviewReady({ sync_id: 's1' }));
      handleNetworkSync(reviewReady({ sync_id: 's2' }));
      store().openSyncDialog('s1');
      handleNetworkSync(finished({ sync_id: 's2' }));
      expect(store().dialogSyncId).toBe('s1');
      expect(store().syncReviews.map((r) => r.syncId)).toEqual(['s1']);
    });

    it('closes the dialog of the sync that finished', () => {
      handleNetworkSync(reviewReady());
      store().openSyncDialog('s1');
      handleNetworkSync(finished());
      expect(store().dialogSyncId).toBeNull();
    });

    it('shows the result once, also when the finish event repeats', () => {
      handleNetworkSync(finished());
      handleNetworkSync(finished());
      expect(mocks.toast.success).toHaveBeenCalledTimes(1);
    });

    it('dismisses the waiting toast on the first finish', () => {
      handleNetworkSync(finished({ outcome: 'declined' }));
      expect(mocks.toast.dismiss).toHaveBeenCalledWith(syncWaitingToastId('s1'));
    });
  });

  describe('results', () => {
    it('announces applied', () => {
      handleNetworkSync(finished({ collection_name: 'Orders "v2"' }));
      expect(mocks.toast.success).toHaveBeenCalledWith('Synced "Orders "v2"" with Ben Ortiz.', {
        id: syncResultToastId('s1'),
      });
    });

    it('names a node without a display name', () => {
      handleNetworkSync(finished({ display_name: '  ' }));
      expect(mocks.toast.success).toHaveBeenCalledWith('Synced "Orders" with Unnamed node.', {
        id: syncResultToastId('s1'),
      });
    });

    it('announces a decline by the other node to the starter', () => {
      store().trackStartedSync('s1', { displayName: 'Ben Ortiz', collectionName: 'Orders' });
      handleNetworkSync(finished({ outcome: 'declined' }));
      expect(mocks.toast.info).toHaveBeenCalledWith('Ben Ortiz declined the sync.', {
        id: syncResultToastId('s1'),
      });
    });

    it('announces a decline by the other node to a planner that accepted', () => {
      handleNetworkSync(reviewReady());
      store().markSyncAnswered('s1');
      handleNetworkSync(finished({ outcome: 'declined' }));
      expect(mocks.toast.info).toHaveBeenCalledWith('Ben Ortiz declined the sync.', {
        id: syncResultToastId('s1'),
      });
    });

    it('says nothing when the local user declined', () => {
      handleNetworkSync(confirmNeeded());
      store().markSyncDeclinedHere('s1');
      handleNetworkSync(finished({ outcome: 'declined' }));
      expect(mocks.toast.info).not.toHaveBeenCalled();
    });

    it('says nothing when the local user cancelled the waiting toast and it ended declined', () => {
      store().trackStartedSync('s1', { displayName: 'Ben Ortiz', collectionName: 'Orders' });
      store().markSyncCancelledHere('s1');
      handleNetworkSync(finished({ outcome: 'declined' }));
      expect(mocks.toast.info).not.toHaveBeenCalled();
    });

    it('announces expired', () => {
      handleNetworkSync(finished({ outcome: 'expired' }));
      expect(mocks.toast.info).toHaveBeenCalledWith('The request expired.', {
        id: syncResultToastId('s1'),
      });
    });

    it('announces a cancel by the other node', () => {
      handleNetworkSync(finished({ outcome: 'cancelled' }));
      expect(mocks.toast.info).toHaveBeenCalledWith('Ben Ortiz cancelled the sync.', {
        id: syncResultToastId('s1'),
      });
    });

    it('says nothing when the local user cancelled', () => {
      store().markSyncCancelledHere('s1');
      handleNetworkSync(finished({ outcome: 'cancelled' }));
      expect(mocks.toast.info).not.toHaveBeenCalled();
    });

    it('announces a base mismatch with the fixed text, or the backend message', () => {
      handleNetworkSync(finished({ outcome: 'base_mismatch' }));
      expect(mocks.toast.error).toHaveBeenLastCalledWith(
        'The two copies disagree about the last agreed version. Unlink and send the collection again.',
        { id: syncResultToastId('s1') },
      );
      handleNetworkSync(finished({ sync_id: 's2', outcome: 'base_mismatch', message: 'Backend text.' }));
      expect(mocks.toast.error).toHaveBeenLastCalledWith('Backend text.', {
        id: syncResultToastId('s2'),
      });
    });

    it('announces a missing link with the backend message', () => {
      handleNetworkSync(
        finished({
          outcome: 'link_missing',
          message: 'The link is missing on the other side. Unlink and send the collection again.',
        }),
      );
      expect(mocks.toast.error).toHaveBeenCalledWith(
        'The link is missing on the other side. Unlink and send the collection again.',
        { id: syncResultToastId('s1') },
      );
    });

    it('announces a missing link with a fixed text when there is no message', () => {
      handleNetworkSync(finished({ outcome: 'link_missing' }));
      expect(mocks.toast.error).toHaveBeenCalledWith(
        'The link is missing on the other side. Unlink and send the collection again.',
        { id: syncResultToastId('s1') },
      );
    });

    it.each([
      ['Applied here, but the other node did not confirm.'],
      ['Applied here, but the other node did not get the result. Sync again.'],
    ])('announces unconfirmed with the backend message: %s', (message) => {
      handleNetworkSync(finished({ outcome: 'unconfirmed', message }));
      expect(mocks.toast.info).toHaveBeenCalledWith(message, { id: syncResultToastId('s1') });
    });

    it('announces unconfirmed with a fixed text when there is no message', () => {
      handleNetworkSync(finished({ outcome: 'unconfirmed' }));
      expect(mocks.toast.info).toHaveBeenCalledWith(
        'Applied here, but the other node did not confirm.',
        { id: syncResultToastId('s1') },
      );
    });

    it('shows the backend message of an applied sync as info', () => {
      handleNetworkSync(finished({ message: 'Already in sync.' }));
      expect(mocks.toast.info).toHaveBeenCalledWith('Already in sync.', {
        id: syncResultToastId('s1'),
      });
      expect(mocks.toast.success).not.toHaveBeenCalled();
    });

    it('uses the backend message on failed, else a fixed text', () => {
      handleNetworkSync(finished({ outcome: 'failed', message: 'A file was not safe to apply.' }));
      expect(mocks.toast.error).toHaveBeenLastCalledWith('A file was not safe to apply.', {
        id: syncResultToastId('s1'),
      });
      handleNetworkSync(finished({ sync_id: 's2', outcome: 'failed', message: null }));
      expect(mocks.toast.error).toHaveBeenLastCalledWith('Sync failed. Try again.', {
        id: syncResultToastId('s2'),
      });
      handleNetworkSync(finished({ sync_id: 's3', outcome: 'failed', message: '   ' }));
      expect(mocks.toast.error).toHaveBeenLastCalledWith('Sync failed. Try again.', {
        id: syncResultToastId('s3'),
      });
    });

    it('says nothing for an outcome added later', () => {
      handleNetworkSync(finished({ outcome: 'future' as never }));
      expect(mocks.toast.info).not.toHaveBeenCalled();
      expect(mocks.toast.error).not.toHaveBeenCalled();
      expect(mocks.toast.success).not.toHaveBeenCalled();
    });

    it('reloads the links after every finish', () => {
      handleNetworkSync(finished());
      expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
    });
  });

  describe('a dialog that is open while the sync ends', () => {
    it('keeps an unanswered dialog open and says why', () => {
      handleNetworkSync(reviewReady());
      store().openSyncDialog('s1');
      handleNetworkSync(
        finished({ outcome: 'failed', message: 'The other node went offline.' }),
      );
      expect(store().dialogSyncId).toBe('s1');
      expect(store().dialogSyncEndedText).toBe('The other node went offline.');
      expect(store().syncReviews).toHaveLength(0);
    });

    it('keeps the dialog open on an expired finish', () => {
      handleNetworkSync(reviewReady());
      store().openSyncDialog('s1');
      handleNetworkSync(finished({ outcome: 'expired' }));
      expect(store().dialogSyncId).toBe('s1');
    });

    it('closes an answered dialog on a failure', () => {
      handleNetworkSync(reviewReady());
      store().markSyncAnswered('s1');
      store().openSyncDialog('s1');
      handleNetworkSync(finished({ outcome: 'failed', message: 'Something broke.' }));
      expect(store().dialogSyncId).toBeNull();
    });
  });
});
