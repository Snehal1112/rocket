import { useEffect } from 'react';
import { toast } from 'sonner';
import { IncomingPairingToast, incomingToastId } from '@/components/network/IncomingPairingToast';
import { INCOMING_TTL_MS, type PairingIncoming, useNetworkStore } from '@/stores/network-store';

function showToast(request: PairingIncoming): void {
  toast.custom(
    () => (
      <IncomingPairingToast
        pairingId={request.pairingId}
        displayName={request.displayName}
        shortId={request.shortId}
      />
    ),
    { id: incomingToastId(request.pairingId), duration: INCOMING_TTL_MS },
  );
}

// Shows one toast for each open pairing request and removes it when the request goes.
// The store drops a repeated event, so a duplicate never reaches this hook. Mount once.
export function useIncomingPairingNotice(): void {
  useEffect(() => {
    // A hot reload keeps the store, so requests that already exist get their toast too.
    for (const request of useNetworkStore.getState().incoming) showToast(request);

    return useNetworkStore.subscribe((state, prev) => {
      if (state.incoming === prev.incoming) return;
      for (const request of state.incoming) {
        if (!prev.incoming.some((p) => p.pairingId === request.pairingId)) showToast(request);
      }
      for (const request of prev.incoming) {
        if (!state.incoming.some((i) => i.pairingId === request.pairingId)) {
          toast.dismiss(incomingToastId(request.pairingId));
        }
      }
    });
  }, []);
}
