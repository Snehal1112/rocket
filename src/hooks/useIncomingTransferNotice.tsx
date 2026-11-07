import { useEffect } from 'react';
import { toast } from 'sonner';
import {
  IncomingTransferToast,
  incomingTransferToastId,
} from '@/components/network/IncomingTransferToast';
import { INCOMING_TTL_MS, type IncomingTransfer, useNetworkStore } from '@/stores/network-store';

function showToast(offer: IncomingTransfer): void {
  toast.custom(
    () => (
      <IncomingTransferToast
        transferId={offer.transferId}
        displayName={offer.displayName}
        shortId={offer.shortId}
        collectionName={offer.collectionName}
        sizeBytes={offer.sizeBytes}
      />
    ),
    { id: incomingTransferToastId(offer.transferId), duration: INCOMING_TTL_MS },
  );
}

// Shows one toast for each pending offer and removes it when the offer goes. An accepted offer
// needs no toast. The store drops a repeated event, so a duplicate never reaches this hook.
// Mount once.
export function useIncomingTransferNotice(): void {
  useEffect(() => {
    // A hot reload keeps the store, so pending offers that already exist get their toast too.
    for (const offer of useNetworkStore.getState().incomingTransfers) {
      if (!offer.accepted) showToast(offer);
    }

    return useNetworkStore.subscribe((state, prev) => {
      if (state.incomingTransfers !== prev.incomingTransfers) {
        for (const offer of state.incomingTransfers) {
          if (!prev.incomingTransfers.some((p) => p.transferId === offer.transferId)) {
            showToast(offer);
          }
        }
        for (const offer of prev.incomingTransfers) {
          if (!state.incomingTransfers.some((i) => i.transferId === offer.transferId)) {
            toast.dismiss(incomingTransferToastId(offer.transferId));
          }
        }
      }
      // A dialog closed without an answer leaves the offer pending, so its toast comes back.
      if (prev.dialogTransferId !== null && state.dialogTransferId !== prev.dialogTransferId) {
        const left = state.incomingTransfers.find((i) => i.transferId === prev.dialogTransferId);
        if (left && !left.accepted) showToast(left);
      }
    });
  }, []);
}
