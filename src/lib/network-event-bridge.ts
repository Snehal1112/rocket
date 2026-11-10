import { useEffect } from 'react';
import { toast } from 'sonner';
import { createListenerScope } from '@/lib/listen-scope';
import { contractCheckToastId, offerToastText } from '@/lib/network-contract-check-utils';
import { syncResultToastId, syncWaitingToastId } from '@/lib/network-sync-utils';
import {
  PAIRING_FAILED_TOAST_ID,
  peerName,
  transferResultToastId,
  transferWaitingToastId,
} from '@/lib/network-utils';
import {
  type NetworkChangedEvent,
  type NetworkContractCheckEvent,
  type NetworkPairingEvent,
  type NetworkSyncEvent,
  type NetworkTransferEvent,
  onNetworkChanged,
  onNetworkContractCheck,
  onNetworkPairing,
  onNetworkSync,
  onNetworkTransfer,
} from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

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

type FinishedSync = Extract<NetworkSyncEvent, { type: 'finished' }>;

// Fixed texts, or the local backend message where it has one. A missing row means the user
// already knows. `declinedHere` and `cancelledHere` are true when the local user did it.
function describeSyncOutcome(
  event: FinishedSync,
  declinedHere: boolean,
  cancelledHere: boolean,
): TransferMessage | null {
  const name = peerName(event.display_name);
  const backendText = event.message?.trim() || null;
  switch (event.outcome) {
    case 'applied':
      // An empty plan ends applied with "Already in sync.".
      return backendText
        ? { kind: 'info', text: backendText }
        : { kind: 'success', text: `Synced "${event.collection_name}" with ${name}.` };
    case 'declined':
      return declinedHere || cancelledHere
        ? null
        : { kind: 'info', text: `${name} declined the sync.` };
    case 'expired':
      return { kind: 'info', text: 'The request expired.' };
    case 'cancelled':
      return cancelledHere ? null : { kind: 'info', text: `${name} cancelled the sync.` };
    case 'failed':
      return { kind: 'error', text: backendText ?? 'Sync failed. Try again.' };
    case 'base_mismatch':
      return {
        kind: 'error',
        text:
          backendText ??
          'The two copies disagree about the last agreed version. Unlink and send the collection again.',
      };
    case 'link_missing':
      return {
        kind: 'error',
        text:
          backendText ??
          'The link is missing on the other side. Unlink and send the collection again.',
      };
    case 'unconfirmed':
      return {
        kind: 'info',
        text: backendText ?? 'Applied here, but the other node did not confirm.',
      };
    default:
      // Any outcome added later stays quiet.
      return null;
  }
}

export function handleNetworkSync(event: NetworkSyncEvent): void {
  const store = useNetworkStore.getState();
  switch (event.type) {
    case 'reviewReady':
      store.addSyncReview({
        syncId: event.sync_id,
        linkId: event.link_id,
        deviceId: event.device_id,
        shortId: event.short_id,
        displayName: event.display_name,
        collectionName: event.collection_name,
        summary: event.summary,
        conflicts: event.conflicts,
        kind: event.role === 'starter' ? 'confirm' : 'review',
        answered: false,
      });
      return;
    case 'confirmNeeded': {
      const added = store.addSyncReview({
        syncId: event.sync_id,
        linkId: event.link_id,
        deviceId: event.device_id,
        shortId: event.short_id,
        displayName: event.display_name,
        collectionName: event.collection_name,
        summary: event.summary,
        conflicts: [],
        kind: 'confirm',
        answered: false,
      });
      // The confirm follows the user's own action, so it opens unless another sync dialog is open.
      if (added && store.dialogSyncId === null) store.openSyncDialog(event.sync_id);
      return;
    }
    case 'progress': {
      store.setSyncProgress({
        syncId: event.sync_id,
        doneBytes: event.done_bytes,
        totalBytes: event.total_bytes,
      });
      const name = store.advanceStartedSync(event.sync_id);
      // Sonner merges options for a reused id, so the Cancel action stays on the toast.
      if (name !== null) {
        toast.info(`Syncing with ${peerName(name)}...`, {
          id: syncWaitingToastId(event.sync_id),
          duration: Number.POSITIVE_INFINITY,
        });
      }
      return;
    }
    case 'finished': {
      const id = event.sync_id;
      // Read these before the finish, which forgets them.
      const declinedHere = store.wasSyncDeclinedHere(id);
      const cancelledHere = store.wasSyncCancelledHere(id);
      const wasPending = store.syncReviews.some((r) => r.syncId === id && !r.answered);
      // A repeated finish event changes nothing and shows nothing.
      if (!store.finishSync(id)) return;
      let message = describeSyncOutcome(event, declinedHere, cancelledHere);
      // An open dialog already says "The request expired.", so the toast would repeat it.
      if (event.outcome === 'expired' && store.dialogSyncId === id) message = null;
      // A review that ended before it was answered keeps its dialog, so the dialog can say why.
      const keepDialog =
        event.outcome === 'expired' ||
        (wasPending && event.outcome === 'failed' && Boolean(event.message?.trim()));
      if (store.dialogSyncId === id) {
        if (!keepDialog) store.closeSyncDialog();
        else if (event.outcome === 'failed' && message) store.endSyncDialog(message.text);
      }
      // The waiting toast with the Cancel action must not outlive the sync.
      toast.dismiss(syncWaitingToastId(id));
      if (message) toast[message.kind](message.text, { id: syncResultToastId(id) });
      void store.refreshLinks();
      return;
    }
    case 'linkChanged':
      void store.refreshLinks();
      return;
  }
}

export function handleNetworkContractCheck(event: NetworkContractCheckEvent): void {
  const store = useNetworkStore.getState();
  switch (event.type) {
    case 'offerReceived': {
      store.addIncomingContractCheck({
        checkId: event.check_id,
        deviceId: event.device_id,
        contract: event.contract,
        changeCount: event.change_count,
      });
      const known = store.nodes.find((n) => n.deviceId === event.device_id)?.displayName.trim();
      toast.info(offerToastText(known || null, event.contract, event.change_count), {
        id: contractCheckToastId(event.check_id),
        duration: Number.POSITIVE_INFINITY,
        action: {
          label: 'Review',
          onClick: () => useNetworkStore.getState().openContractCheckDialog(event.check_id),
        },
      });
      return;
    }
    case 'resultReceived':
      store.setContractCheckResult(event.check_id, event.device_id, {
        outcome: event.outcome,
        rows: event.rows,
      });
      return;
    case 'expired':
      store.expireContractCheck(event.check_id);
      toast.dismiss(contractCheckToastId(event.check_id));
      return;
  }
}

/**
 * Subscribes once, for the app's lifetime, to the network channels and loads the first
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
    scope.keep(
      onNetworkSync((event) => {
        if (!scope.isDisposed()) handleNetworkSync(event);
      }),
    );
    scope.keep(
      onNetworkContractCheck((event) => {
        if (!scope.isDisposed()) handleNetworkContractCheck(event);
      }),
    );

    void useNetworkStore.getState().refresh();
    void useNetworkStore.getState().refreshNodes();
    void useNetworkStore.getState().refreshLinks();

    // Links are matched to collections of the active workspace, so a switch reloads their state.
    const stopWorkspaceWatch = useWorkspaceStore.subscribe((state, prev) => {
      if (state.activeWorkspaceId !== prev.activeWorkspaceId) {
        void useNetworkStore.getState().refreshLinks();
      }
    });

    return () => {
      stopWorkspaceWatch();
      scope.dispose();
    };
  }, []);
}
