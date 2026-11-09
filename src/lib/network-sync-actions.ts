import { toast } from 'sonner';
import { toastError } from '@/lib/error-text';
import { syncWaitingToastId } from '@/lib/network-sync-utils';
import { peerName } from '@/lib/network-utils';
import { cancelNetworkSync, type NetworkLink, startNetworkSync } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

/**
 * Starts a sync for one link. It rejects with the backend error, so the caller decides how to show
 * it. On success the waiting toast stays until the bridge dismisses it on the first finish.
 */
export async function startCollectionSync(link: NetworkLink): Promise<void> {
  const { syncId } = await startNetworkSync(link.linkId);
  const store = useNetworkStore.getState();
  // False when the sync finished before this call returned, so no toast would ever be dismissed.
  if (
    !store.trackStartedSync(syncId, {
      displayName: link.displayName,
      collectionName: link.collectionName,
    })
  ) {
    return;
  }
  toast.info(`Waiting for ${peerName(link.displayName)} to review the sync.`, {
    id: syncWaitingToastId(syncId),
    duration: Number.POSITIVE_INFINITY,
    action: {
      label: 'Cancel',
      onClick: () => {
        // The finish then shows no toast about the user's own cancel.
        store.markSyncCancelledHere(syncId);
        cancelNetworkSync(syncId).catch((err: unknown) => {
          store.unmarkSyncCancelledHere(syncId);
          toastError(err);
        });
      },
    },
  });
}
