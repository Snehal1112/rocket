import { useEffect } from 'react';
import { toast } from 'sonner';
import { createListenerScope } from '@/lib/listen-scope';
import {
  PAIRING_FAILED_TOAST_ID,
  peerName,
  transferResultToastId,
  transferWaitingToastId,
} from '@/lib/network-utils';
import {
  type NetworkChangedEvent,
  type NetworkPairingEvent,
  type NetworkTransferEvent,
  onNetworkChanged,
  onNetworkPairing,
  onNetworkTransfer,
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
  const acceptedHere = store.incoming.some((i) => i.pairingId === event.pairing_id && i.accepted);
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
      else if (acceptedHere) toast.info(`${name} did not pair.`);
      break;
    case 'expired':
      toast.info('The request expired.');
      break;
    case 'failed':
      toast.error('Pairing failed. Try again.', { id: PAIRING_FAILED_TOAST_ID });
      break;
    case 'cancelled':
      // The other side backed out after this user accepted.
      if (acceptedHere) toast.info(`${name} did not pair.`);
      break;
    default:
      // Any outcome added later stays quiet.
      break;
  }
}

type FinishedTransfer = Extract<NetworkTransferEvent, { type: 'finished' }>;
type ProgressTransfer = Extract<NetworkTransferEvent, { type: 'progress' }>;

interface TransferMessage {
  kind: 'success' | 'info' | 'error';
  text: string;
}

// The texts come from spec section 10. A missing row means the user already knows.
// `cancelledHere` is true when the local user pressed Cancel on this transfer.
function describeTransferOutcome(
  event: FinishedTransfer,
  cancelledHere: boolean,
): TransferMessage | null {
  const name = peerName(event.display_name);
  const sending = event.direction === 'send';
  switch (event.outcome) {
    case 'completed':
      return sending
        ? { kind: 'success', text: `Sent "${event.collection_name}" to ${name}.` }
        : { kind: 'success', text: `Received "${event.collection_name}" from ${name}.` };
    case 'unconfirmed':
      // The bytes left, but no result came back. Not an error: the import may have worked.
      return sending ? { kind: 'info', text: 'Sent, but the other node did not confirm.' } : null;
    case 'declined':
      // The user who pressed Decline already knows.
      return sending ? { kind: 'info', text: `${name} declined.` } : null;
    case 'expired':
      return { kind: 'info', text: 'The request expired.' };
    case 'busy':
      return sending ? { kind: 'error', text: `${name} is busy. Try again in a minute.` } : null;
    case 'too_big':
      // A refused offer emits nothing on the receiver, so only the sender has a text.
      return sending
        ? { kind: 'error', text: 'This collection is too big to send (limit 10 MiB).' }
        : null;
    case 'failed':
      return {
        kind: 'error',
        // The receiver shows the importer's text when there is one. The sender never does.
        text: sending ? 'Sending failed. Try again.' : event.message?.trim() || 'Receiving failed.',
      };
    case 'damaged':
      return {
        kind: 'error',
        text: sending
          ? 'Sending failed. Try again.'
          : 'The collection arrived damaged and was not imported.',
      };
    case 'rejected':
      return {
        kind: 'error',
        text: sending
          ? 'Sending failed. Try again.'
          : 'The collection was rejected because it is not safe to import.',
      };
    case 'cancelled':
      // The user who pressed Cancel already knows. Otherwise the other side cancelled.
      return cancelledHere ? null : { kind: 'info', text: `${name} cancelled the transfer.` };
    default:
      // Any outcome added later stays quiet.
      return null;
  }
}

// The waiting toast follows a send started here: "Sending to <name>..." once bytes move, and no
// Cancel once the last chunk is out, because the backend refuses a cancel then.
function updateWaitingToast(event: ProgressTransfer): void {
  const last = event.done_bytes >= event.total_bytes;
  const name = useNetworkStore
    .getState()
    .advanceOutgoing(event.transfer_id, last ? 'sent' : 'sending');
  if (name === null) return;
  const options = {
    id: transferWaitingToastId(event.transfer_id),
    duration: Number.POSITIVE_INFINITY,
  };
  // Sonner merges options for a reused id, so an explicit undefined removes the Cancel action.
  toast.info(`Sending to ${peerName(name)}...`, last ? { ...options, action: undefined } : options);
}

export function handleNetworkTransfer(event: NetworkTransferEvent): void {
  const store = useNetworkStore.getState();
  switch (event.type) {
    case 'offerIncoming':
      store.addIncomingTransfer({
        transferId: event.transfer_id,
        deviceId: event.device_id,
        shortId: event.short_id,
        displayName: event.display_name,
        collectionName: event.collection_name,
        sizeBytes: event.size_bytes,
        accepted: false,
      });
      return;
    case 'progress':
      store.setTransferProgress({
        transferId: event.transfer_id,
        direction: event.direction,
        doneBytes: event.done_bytes,
        totalBytes: event.total_bytes,
      });
      if (event.direction === 'send') updateWaitingToast(event);
      return;
    case 'finished': {
      const id = event.transfer_id;
      // Read these before the finish, which forgets both.
      const cancelledHere = store.wasCancelledHere(id);
      const wasPending = store.incomingTransfers.some((t) => t.transferId === id && !t.accepted);
      // A repeated finish event changes nothing and shows nothing.
      if (!store.finishTransfer(id)) return;
      const message = describeTransferOutcome(event, cancelledHere);
      // An offer that ended before it was answered keeps its dialog, so the dialog can say why:
      // it expired, or the sender went offline (a `failed` with a message).
      const keepDialog =
        event.outcome === 'expired' ||
        (wasPending && event.outcome === 'failed' && Boolean(event.message?.trim()));
      if (store.dialogTransferId === id) {
        if (!keepDialog) store.closeTransferDialog();
        else if (event.outcome === 'failed' && message) store.endTransferDialog(message.text);
      }
      // The waiting toast with the Cancel action must not outlive the transfer.
      toast.dismiss(transferWaitingToastId(id));
      if (message) toast[message.kind](message.text, { id: transferResultToastId(id) });
      return;
    }
  }
}

/**
 * Subscribes once, for the app's lifetime, to the three network channels and loads the first
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
    scope.keep(
      onNetworkTransfer((event) => {
        if (!scope.isDisposed()) handleNetworkTransfer(event);
      }),
    );

    void useNetworkStore.getState().refresh();
    void useNetworkStore.getState().refreshNodes();

    return () => scope.dispose();
  }, []);
}
