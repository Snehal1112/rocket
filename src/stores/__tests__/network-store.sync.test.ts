import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { INCOMING_TTL_MS, useNetworkStore } from '@/stores/network-store';
import { createDeferred } from '@/test/deferred';
import { makeLink, makeSyncReview } from '@/test/network-fixtures';

const mocks = vi.hoisted(() => ({
  api: {
    listNetworkLinks: vi.fn(),
    unlinkNetworkLink: vi.fn(),
    getNetworkStatus: vi.fn(),
    listNetworkNodes: vi.fn(),
  },
}));

vi.mock('@/lib/tauri-api', () => mocks.api);

const store = () => useNetworkStore.getState();

describe('network store sync slice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store().reset();
    mocks.api.listNetworkLinks.mockResolvedValue([]);
    mocks.api.unlinkNetworkLink.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('adds a review once', () => {
    expect(store().addSyncReview(makeSyncReview())).toBe(true);
    expect(store().syncReviews).toHaveLength(1);
  });

  it('dedupes by sync id and does not extend the expiry', () => {
    vi.useFakeTimers();
    store().addSyncReview(makeSyncReview());
    vi.advanceTimersByTime(INCOMING_TTL_MS - 1000);
    expect(store().addSyncReview(makeSyncReview())).toBe(false);
    expect(store().syncReviews).toHaveLength(1);
    vi.advanceTimersByTime(1000);
    expect(store().syncReviews).toHaveLength(0);
  });

  it('keeps two reviews, each with its own timer', () => {
    vi.useFakeTimers();
    store().addSyncReview(makeSyncReview({ syncId: 's1' }));
    vi.advanceTimersByTime(60_000);
    store().addSyncReview(makeSyncReview({ syncId: 's2' }));
    vi.advanceTimersByTime(60_000);
    expect(store().syncReviews.map((r) => r.syncId)).toEqual(['s2']);
  });

  it('does not expire an answered review', () => {
    vi.useFakeTimers();
    store().addSyncReview(makeSyncReview());
    store().markSyncAnswered('s1');
    vi.advanceTimersByTime(INCOMING_TTL_MS * 3);
    expect(store().syncReviews).toHaveLength(1);
    expect(store().syncReviews[0].answered).toBe(true);
  });

  it('markSyncAnswered ignores a sync that is gone', () => {
    store().addSyncReview(makeSyncReview());
    store().finishSync('s1');
    store().markSyncAnswered('s1');
    expect(store().syncReviews).toHaveLength(0);
  });

  it('finishSync returns false the second time', () => {
    store().addSyncReview(makeSyncReview());
    expect(store().finishSync('s1')).toBe(true);
    expect(store().finishSync('s1')).toBe(false);
  });

  it('ignores progress and reviews for a finished sync', () => {
    store().finishSync('s1');
    store().setSyncProgress({ syncId: 's1', doneBytes: 1, totalBytes: 2 });
    expect(store().addSyncReview(makeSyncReview())).toBe(false);
    expect(store().syncProgress).toHaveLength(0);
    expect(store().syncReviews).toHaveLength(0);
  });

  it('keeps progress per sync', () => {
    store().setSyncProgress({ syncId: 's1', doneBytes: 1, totalBytes: 10 });
    store().setSyncProgress({ syncId: 's2', doneBytes: 5, totalBytes: 10 });
    store().setSyncProgress({ syncId: 's1', doneBytes: 4, totalBytes: 10 });
    expect(store().syncProgress).toEqual([
      { syncId: 's1', doneBytes: 4, totalBytes: 10 },
      { syncId: 's2', doneBytes: 5, totalBytes: 10 },
    ]);
  });

  it('finishSync forgets the started sync and the local cancel', () => {
    expect(store().trackStartedSync('s1', { displayName: 'Ben', collectionName: 'Orders' })).toBe(
      true,
    );
    store().markSyncCancelledHere('s1');
    expect(store().startedSync('s1')).toEqual({ displayName: 'Ben', collectionName: 'Orders' });
    expect(store().wasSyncCancelledHere('s1')).toBe(true);
    store().finishSync('s1');
    expect(store().startedSync('s1')).toBeUndefined();
    expect(store().wasSyncCancelledHere('s1')).toBe(false);
  });

  it('remembers a local decline until the sync finishes', () => {
    store().markSyncDeclinedHere('s1');
    expect(store().wasSyncDeclinedHere('s1')).toBe(true);
    store().unmarkSyncDeclinedHere('s1');
    expect(store().wasSyncDeclinedHere('s1')).toBe(false);
    store().markSyncDeclinedHere('s1');
    store().finishSync('s1');
    expect(store().wasSyncDeclinedHere('s1')).toBe(false);
  });

  it('does not track a sync that already finished', () => {
    store().finishSync('s1');
    expect(store().trackStartedSync('s1', { displayName: 'Ben', collectionName: 'Orders' })).toBe(
      false,
    );
  });

  it('advances a started sync once', () => {
    store().trackStartedSync('s1', { displayName: 'Ben', collectionName: 'Orders' });
    expect(store().advanceStartedSync('s1')).toBe('Ben');
    expect(store().advanceStartedSync('s1')).toBeNull();
    expect(store().advanceStartedSync('other')).toBeNull();
  });

  it('opens and closes the dialog and clears the ended text', () => {
    store().openSyncDialog('s1');
    store().endSyncDialog('The other node went offline.');
    expect(store().dialogSyncEndedText).toBe('The other node went offline.');
    store().openSyncDialog('s2');
    expect(store().dialogSyncEndedText).toBeNull();
    store().closeSyncDialog();
    expect(store().dialogSyncId).toBeNull();
  });

  it('loads the links', async () => {
    mocks.api.listNetworkLinks.mockResolvedValue([makeLink()]);
    await store().refreshLinks();
    expect(store().links).toEqual([makeLink()]);
  });

  it('drops a stale links response', async () => {
    const first = createDeferred<ReturnType<typeof makeLink>[]>();
    mocks.api.listNetworkLinks
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce([makeLink({ linkId: 'NEW' })]);
    const slow = store().refreshLinks();
    await store().refreshLinks();
    first.resolve([makeLink({ linkId: 'OLD' })]);
    await slow;
    expect(store().links.map((l) => l.linkId)).toEqual(['NEW']);
  });

  it('keeps the old links when the load fails and sets no error', async () => {
    mocks.api.listNetworkLinks.mockResolvedValueOnce([makeLink()]);
    await store().refreshLinks();
    mocks.api.listNetworkLinks.mockRejectedValueOnce('boom');
    await store().refreshLinks();
    expect(store().links).toHaveLength(1);
    expect(store().error).toBeNull();
  });

  it('unlink calls the command and reloads the list', async () => {
    mocks.api.listNetworkLinks.mockResolvedValue([]);
    await store().unlink('L1');
    expect(mocks.api.unlinkNetworkLink).toHaveBeenCalledWith('L1');
    expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
  });

  it('unlink reloads the list also when the call fails', async () => {
    mocks.api.unlinkNetworkLink.mockRejectedValue('Could not unlink.');
    await expect(store().unlink('L1')).rejects.toBe('Could not unlink.');
    expect(mocks.api.listNetworkLinks).toHaveBeenCalledTimes(1);
  });

  it('reset clears the slice and the timers', () => {
    vi.useFakeTimers();
    store().addSyncReview(makeSyncReview());
    store().openSyncDialog('s1');
    store().reset();
    expect(store().syncReviews).toHaveLength(0);
    expect(store().dialogSyncId).toBeNull();
    expect(store().links).toHaveLength(0);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('keeps the same links array when a reload returns the same list', async () => {
    mocks.api.listNetworkLinks.mockResolvedValue([makeLink()]);
    await store().refreshLinks();
    const before = store().links;
    mocks.api.listNetworkLinks.mockResolvedValue([makeLink()]);
    await store().refreshLinks();
    expect(store().links).toBe(before);
  });

  it('lists the syncs started here for one link', () => {
    store().trackStartedSync('s1', { displayName: 'Ben', collectionName: 'Orders', linkId: 'L1' });
    store().trackStartedSync('s2', { displayName: 'Cara', collectionName: 'Billing', linkId: 'L2' });
    expect(store().startedSyncIdsForLink('L1')).toEqual(['s1']);
  });

  it('forgets the oldest local cancel marks after many syncs', () => {
    for (let i = 0; i < 150; i++) store().markSyncCancelledHere(`c${i}`);
    expect(store().wasSyncCancelledHere('c0')).toBe(false);
    expect(store().wasSyncCancelledHere('c149')).toBe(true);
    for (let i = 0; i < 150; i++) store().markSyncDeclinedHere(`d${i}`);
    expect(store().wasSyncDeclinedHere('d0')).toBe(false);
    expect(store().wasSyncDeclinedHere('d149')).toBe(true);
  });
});
