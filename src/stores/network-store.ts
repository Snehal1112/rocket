import { create } from 'zustand';
import { errorText } from '@/lib/error-text';
import {
  addNetworkAddress,
  getNetworkStatus,
  listNetworkNodes,
  markNetworkIntroSeen,
  type NetworkNode,
  type NetworkStatus,
  removeNetworkAddress,
  removeNetworkPaired,
  requestNetworkPairing,
  setNetworkProfile,
  setNetworkVisibility,
  type TransferDirection,
  unblockNetworkDevice,
} from '@/lib/tauri-api';

/** An incoming request lives this long. It matches the backend pairing lifetime. */
export const INCOMING_TTL_MS = 120_000;

/** A request another node sent to this node. */
export interface PairingIncoming {
  pairingId: string;
  deviceId: string;
  shortId: string;
  displayName: string;
  /** The six digits without a space. */
  code: string;
  /** The local user pressed Accept and now waits for the peer. */
  accepted: boolean;
}

/** The pairing this node started. */
export interface ActivePairing {
  pairingId: string;
  deviceId: string;
  shortId: string;
  displayName: string;
  code: string;
  /** `waiting`: no local decision yet. `done`: Accept was sent, the peer decides next. */
  phase: 'waiting' | 'done';
}

/** A collection another node offers. It waits for the local user's answer. */
export interface IncomingTransfer {
  transferId: string;
  deviceId: string;
  shortId: string;
  displayName: string;
  collectionName: string;
  sizeBytes: number;
  /** The local user pressed Accept. An accepted offer no longer expires. */
  accepted: boolean;
}

/** A transfer that moves bytes, in either direction. */
export interface ActiveTransfer {
  transferId: string;
  direction: TransferDirection;
  doneBytes: number;
  totalBytes: number;
}

export interface NetworkState {
  status: NetworkStatus | null;
  nodes: NetworkNode[];
  incoming: PairingIncoming[];
  activePairing: ActivePairing | null;
  panelOpen: boolean;
  /** The pairing whose dialog is open, for either role. */
  dialogPairingId: string | null;
  /** Offers from other nodes that wait for an answer or receive bytes. */
  incomingTransfers: IncomingTransfer[];
  /** Progress of transfers in both directions. */
  activeTransfers: ActiveTransfer[];
  /** The offer whose dialog is open. */
  dialogTransferId: string | null;
  /** Why the open offer ended before it was answered, for example "The other node went offline." */
  dialogEndedText: string | null;
  error: string | null;
  refresh: () => Promise<void>;
  refreshNodes: () => Promise<void>;
  /** The mutations below reject with the backend error and reload what they changed. */
  removePaired: (deviceId: string) => Promise<void>;
  addAddress: (host: string, port: number) => Promise<void>;
  removeAddress: (host: string, port: number) => Promise<void>;
  unblock: (deviceId: string) => Promise<void>;
  setHidden: (hidden: boolean) => Promise<void>;
  saveName: (displayName: string) => Promise<void>;
  markIntroSeen: () => Promise<void>;
  addIncoming: (incoming: PairingIncoming) => void;
  removeIncoming: (pairingId: string) => void;
  markIncomingAccepted: (pairingId: string) => void;
  setActivePairing: (pairing: ActivePairing | null) => void;
  setPanelOpen: (open: boolean) => void;
  openPairingDialog: (pairingId: string) => void;
  closePairingDialog: () => void;
  addIncomingTransfer: (transfer: IncomingTransfer) => void;
  removeIncomingTransfer: (transferId: string) => void;
  markTransferAccepted: (transferId: string) => void;
  setTransferProgress: (progress: ActiveTransfer) => void;
  /** Removes the transfer everywhere. It returns false when the transfer had finished before. */
  finishTransfer: (transferId: string) => boolean;
  openTransferDialog: (transferId: string) => void;
  closeTransferDialog: () => void;
  /** Keeps the dialog open on an ended offer and says why. */
  endTransferDialog: (text: string) => void;
  /** A send started here. The bridge updates its waiting toast. */
  /** Returns false when the transfer had already finished, so the caller shows no waiting toast. */
  trackOutgoing: (transferId: string, displayName: string) => boolean;
  /** Moves a tracked send forward. It returns the node name when the phase changed, else null. */
  advanceOutgoing: (transferId: string, phase: 'sending' | 'sent') => string | null;
  /** The local user pressed Cancel. The `cancelled` finish then shows no toast. */
  markCancelledHere: (transferId: string) => void;
  /** Undoes `markCancelledHere` when the cancel call was refused. */
  unmarkCancelledHere: (transferId: string) => void;
  wasCancelledHere: (transferId: string) => boolean;
  reset: () => void;
}

const initialState = {
  status: null as NetworkStatus | null,
  nodes: [] as NetworkNode[],
  incoming: [] as PairingIncoming[],
  activePairing: null as ActivePairing | null,
  panelOpen: false,
  dialogPairingId: null as string | null,
  incomingTransfers: [] as IncomingTransfer[],
  activeTransfers: [] as ActiveTransfer[],
  dialogTransferId: null as string | null,
  dialogEndedText: null as string | null,
  error: null as string | null,
};

// Expiry timers by pairing id, and the request counters that drop stale responses.
const expiryTimers = new Map<string, ReturnType<typeof setTimeout>>();
let statusSeq = 0;
let nodesSeq = 0;

// A transfer id shares the timer map with pairing ids, so its key has a prefix.
const transferTimerKey = (transferId: string): string => `transfer:${transferId}`;

// Ids of transfers that finished. A late progress or offer event for one of them is dropped.
const FINISHED_MEMORY = 100;
const finishedTransfers = new Set<string>();

function rememberFinished(transferId: string): void {
  finishedTransfers.add(transferId);
  if (finishedTransfers.size > FINISHED_MEMORY) {
    const oldest = finishedTransfers.values().next().value;
    if (oldest !== undefined) finishedTransfers.delete(oldest);
  }
}

// Sends started here, by transfer id. The phase only moves forward.
type OutgoingPhase = 'waiting' | 'sending' | 'sent';
const PHASE_ORDER: Record<OutgoingPhase, number> = { waiting: 0, sending: 1, sent: 2 };
const outgoing = new Map<string, { displayName: string; phase: OutgoingPhase }>();

// Transfers the local user cancelled. Their finish shows no "cancelled" toast.
const cancelledHere = new Set<string>();

function clearExpiryTimer(pairingId: string): void {
  const timer = expiryTimers.get(pairingId);
  if (timer === undefined) return;
  clearTimeout(timer);
  expiryTimers.delete(pairingId);
}

// A mirror of the backend network state. It is not saved.
export const useNetworkStore = create<NetworkState>()((set, get) => ({
  ...initialState,

  async refresh() {
    const seq = ++statusSeq;
    try {
      const status = await getNetworkStatus();
      if (seq === statusSeq) set({ status, error: null });
    } catch (err) {
      if (seq === statusSeq) set({ error: errorText(err) });
    }
  },

  async refreshNodes() {
    const seq = ++nodesSeq;
    try {
      const nodes = await listNetworkNodes();
      if (seq === nodesSeq) set({ nodes });
    } catch (err) {
      if (seq === nodesSeq) set({ error: errorText(err) });
    }
  },

  async removePaired(deviceId) {
    try {
      await removeNetworkPaired(deviceId);
    } finally {
      // The list is the truth, also after a failure. refreshNodes never throws.
      await get().refreshNodes();
    }
  },

  async addAddress(host, port) {
    await addNetworkAddress(host, port);
    await get().refreshNodes();
  },

  async removeAddress(host, port) {
    await removeNetworkAddress(host, port);
    await get().refreshNodes();
  },

  // A device that was blocked can show up as nearby again.
  async unblock(deviceId) {
    await unblockNetworkDevice(deviceId);
    await get().refreshNodes();
  },

  async setHidden(hidden) {
    await setNetworkVisibility(hidden);
    await get().refresh();
  },

  async saveName(displayName) {
    await setNetworkProfile(displayName);
    await get().refresh();
  },

  async markIntroSeen() {
    await markNetworkIntroSeen();
    await get().refresh();
  },

  addIncoming(incoming) {
    // A repeated event neither adds a row nor extends the expiry.
    if (get().incoming.some((i) => i.pairingId === incoming.pairingId)) return;
    set({ incoming: [...get().incoming, incoming] });
    expiryTimers.set(
      incoming.pairingId,
      setTimeout(() => get().removeIncoming(incoming.pairingId), INCOMING_TTL_MS),
    );
  },

  removeIncoming(pairingId) {
    clearExpiryTimer(pairingId);
    if (!get().incoming.some((i) => i.pairingId === pairingId)) return;
    set({ incoming: get().incoming.filter((i) => i.pairingId !== pairingId) });
  },

  markIncomingAccepted(pairingId) {
    if (!get().incoming.some((i) => i.pairingId === pairingId)) return;
    set({
      incoming: get().incoming.map((i) =>
        i.pairingId === pairingId ? { ...i, accepted: true } : i,
      ),
    });
  },

  setActivePairing: (activePairing) => set({ activePairing }),
  setPanelOpen: (panelOpen) => set({ panelOpen }),
  openPairingDialog: (dialogPairingId) => set({ dialogPairingId }),
  closePairingDialog: () => set({ dialogPairingId: null }),

  addIncomingTransfer(transfer) {
    if (finishedTransfers.has(transfer.transferId)) return;
    // A repeated event neither adds a row nor extends the expiry.
    if (get().incomingTransfers.some((t) => t.transferId === transfer.transferId)) return;
    set({ incomingTransfers: [...get().incomingTransfers, transfer] });
    expiryTimers.set(
      transferTimerKey(transfer.transferId),
      setTimeout(() => get().removeIncomingTransfer(transfer.transferId), INCOMING_TTL_MS),
    );
  },

  removeIncomingTransfer(transferId) {
    clearExpiryTimer(transferTimerKey(transferId));
    if (!get().incomingTransfers.some((t) => t.transferId === transferId)) return;
    set({ incomingTransfers: get().incomingTransfers.filter((t) => t.transferId !== transferId) });
  },

  markTransferAccepted(transferId) {
    if (!get().incomingTransfers.some((t) => t.transferId === transferId)) return;
    // An accepted transfer can run longer than the offer lifetime.
    clearExpiryTimer(transferTimerKey(transferId));
    set({
      incomingTransfers: get().incomingTransfers.map((t) =>
        t.transferId === transferId ? { ...t, accepted: true } : t,
      ),
    });
  },

  setTransferProgress(progress) {
    if (finishedTransfers.has(progress.transferId)) return;
    const known = get().activeTransfers.some((t) => t.transferId === progress.transferId);
    set({
      activeTransfers: known
        ? get().activeTransfers.map((t) => (t.transferId === progress.transferId ? progress : t))
        : [...get().activeTransfers, progress],
    });
  },

  finishTransfer(transferId) {
    if (finishedTransfers.has(transferId)) return false;
    rememberFinished(transferId);
    clearExpiryTimer(transferTimerKey(transferId));
    outgoing.delete(transferId);
    cancelledHere.delete(transferId);
    set({
      incomingTransfers: get().incomingTransfers.filter((t) => t.transferId !== transferId),
      activeTransfers: get().activeTransfers.filter((t) => t.transferId !== transferId),
    });
    return true;
  },

  openTransferDialog: (dialogTransferId) => set({ dialogTransferId, dialogEndedText: null }),
  closeTransferDialog: () => set({ dialogTransferId: null, dialogEndedText: null }),
  endTransferDialog: (dialogEndedText) => set({ dialogEndedText }),

  trackOutgoing(transferId, displayName) {
    if (finishedTransfers.has(transferId)) return false;
    outgoing.set(transferId, { displayName, phase: 'waiting' });
    return true;
  },

  advanceOutgoing(transferId, phase) {
    const entry = outgoing.get(transferId);
    if (!entry || PHASE_ORDER[phase] <= PHASE_ORDER[entry.phase]) return null;
    entry.phase = phase;
    return entry.displayName;
  },

  markCancelledHere: (transferId) => {
    cancelledHere.add(transferId);
  },
  unmarkCancelledHere: (transferId) => {
    cancelledHere.delete(transferId);
  },
  wasCancelledHere: (transferId) => cancelledHere.has(transferId),

  reset() {
    for (const timer of expiryTimers.values()) clearTimeout(timer);
    expiryTimers.clear();
    finishedTransfers.clear();
    outgoing.clear();
    cancelledHere.clear();
    // Bumping the counters makes any request still in flight a no-op.
    statusSeq++;
    nodesSeq++;
    set({ ...initialState });
  },
}));

/** Paired nodes that are online. The send dialog lists them. */
export const onlinePairedNodes = (nodes: NetworkNode[]): NetworkNode[] =>
  nodes.filter((n) => n.group === 'paired' && n.status === 'online');

/** Paired nodes that are online. It feeds the number in the status bar. */
export const selectOnlinePairedCount = (s: NetworkState): number =>
  onlinePairedNodes(s.nodes).length;

/**
 * Asks a nearby node to pair, keeps the code and opens the pairing dialog.
 * It rejects with the backend error and leaves the state alone.
 */
export async function startNetworkPairing(node: NetworkNode): Promise<void> {
  const started = await requestNetworkPairing(node.deviceId);
  const store = useNetworkStore.getState();
  store.setActivePairing({
    pairingId: started.pairingId,
    deviceId: node.deviceId,
    shortId: node.shortId,
    displayName: node.displayName,
    code: started.code,
    phase: 'waiting',
  });
  store.openPairingDialog(started.pairingId);
  store.setPanelOpen(false);
}
