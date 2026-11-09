import { useEffect } from 'react';
import { toast } from 'sonner';
import { IncomingSyncToast, incomingSyncToastId } from '@/components/network/IncomingSyncToast';
import { INCOMING_TTL_MS, type SyncReviewEntry, useNetworkStore } from '@/stores/network-store';

function showToast(entry: SyncReviewEntry): void {
  toast.custom(
    () => (
      <IncomingSyncToast
        syncId={entry.syncId}
        kind={entry.kind}
        displayName={entry.displayName}
        shortId={entry.shortId}
        collectionName={entry.collectionName}
      />
    ),
    { id: incomingSyncToastId(entry.syncId), duration: INCOMING_TTL_MS },
  );
}

// Shows one toast for each sync that waits for an answer and removes it when the entry goes. An
// answered entry needs no toast, and neither does the one whose dialog is open. Mount once.
export function useIncomingSyncNotice(): void {
  useEffect(() => {
    // A hot reload keeps the store, so pending entries that already exist get their toast too.
    const initial = useNetworkStore.getState();
    for (const entry of initial.syncReviews) {
      if (!entry.answered && entry.syncId !== initial.dialogSyncId) showToast(entry);
    }

    return useNetworkStore.subscribe((state, prev) => {
      if (state.syncReviews !== prev.syncReviews) {
        for (const entry of state.syncReviews) {
          const isNew = !prev.syncReviews.some((p) => p.syncId === entry.syncId);
          if (isNew && !entry.answered && entry.syncId !== state.dialogSyncId) showToast(entry);
        }
        for (const entry of prev.syncReviews) {
          if (!state.syncReviews.some((s) => s.syncId === entry.syncId)) {
            toast.dismiss(incomingSyncToastId(entry.syncId));
          }
        }
      }
      // A dialog closed without an answer leaves the entry pending, so its toast comes back.
      if (prev.dialogSyncId !== null && state.dialogSyncId !== prev.dialogSyncId) {
        const left = state.syncReviews.find((r) => r.syncId === prev.dialogSyncId);
        if (left && !left.answered) showToast(left);
      }
    });
  }, []);
}
