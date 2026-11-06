import { useEffect } from 'react';
import { toast } from 'sonner';
import { createListenerScope } from '@/lib/listen-scope';
import {
  type NetworkChangedEvent,
  type NetworkPairingEvent,
  onNetworkChanged,
  onNetworkPairing,
} from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

// Changes carry no data worth keeping, so the store refetches from the commands.
export function handleNetworkChanged(event: NetworkChangedEvent): void {
  const store = useNetworkStore.getState();
  switch (event.type) {
    case 'nodesChanged':
      void store.refreshNodes();
      break;
    case 'statusChanged':
      void store.refresh();
      break;
    case 'pairedRemoved':
      void store.refreshNodes();
      if (event.by_peer) toast.info(`${event.display_name} removed the pairing.`);
      break;
  }
}

export function handleNetworkPairing(event: NetworkPairingEvent): void {
  const store = useNetworkStore.getState();
  if (event.type === 'pairingIncoming') {
    store.addIncoming({
      pairingId: event.pairing_id,
      deviceId: event.device_id,
      shortId: event.short_id,
      displayName: event.display_name,
      code: event.code,
      accepted: false,
    });
    return;
  }

  // Read the state before it changes below.
  const started = store.activePairing?.pairingId === event.pairing_id;
  store.removeIncoming(event.pairing_id);
  if (started) store.setActivePairing(null);
  if (store.dialogPairingId === event.pairing_id) store.closePairingDialog();

  const name = event.display_name;
  switch (event.outcome) {
    case 'accepted':
      toast.success(`Paired with ${name}.`);
      void store.refreshNodes();
      break;
    case 'declined':
      // The user who pressed Decline or Block already knows.
      if (started) toast.info(`${name} declined.`);
      break;
    case 'expired':
      toast.info('The request expired.');
      break;
    case 'failed':
      toast.error('Pairing failed. Try again.');
      break;
    default:
      // A cancelled pairing and any outcome added later stay quiet.
      break;
  }
}

/**
 * Subscribes once, for the app's lifetime, to the two network channels and loads the first
 * status and node list. Mount it once, next to the other event bridges.
 */
export function useNetworkEventBridge(): void {
  useEffect(() => {
    // A listener that resolves after disposal is unlistened at once.
    const scope = createListenerScope();
    scope.keep(
      onNetworkChanged((event) => {
        if (!scope.isDisposed()) handleNetworkChanged(event);
      }),
    );
    scope.keep(
      onNetworkPairing((event) => {
        if (!scope.isDisposed()) handleNetworkPairing(event);
      }),
    );

    void useNetworkStore.getState().refresh();
    void useNetworkStore.getState().refreshNodes();

    return () => scope.dispose();
  }, []);
}
