import { create } from 'zustand';
import { errorText } from '@/lib/error-text';
import {
  addNetworkAddress,
  type ConflictView,
  type ContractCheckOutcome,
  type ContractCheckRowDto,
  getNetworkStatus,
  listNetworkLinks,
  listNetworkNodes,
  markNetworkIntroSeen,
  type NetworkLink,
  type NetworkNode,
  type NetworkStatus,
  removeNetworkAddress,
  removeNetworkPaired,
  requestNetworkPairing,
  type SyncSummary,
  setNetworkProfile,
  setNetworkVisibility,
  type TransferDirection,
  unblockNetworkDevice,
  unlinkNetworkLink,
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

/** A sync another node started (`review`, this node plans) or one this node started and must confirm (`confirm`). */
export interface SyncReviewEntry {
  syncId: string;
  linkId: string;
  deviceId: string;
  shortId: string;
  displayName: string;
  collectionName: string;
  summary: SyncSummary;
  /** Empty for `confirm`. */
  conflicts: ConflictView[];
  kind: 'review' | 'confirm';
  /** The local user answered. An answered entry no longer expires. */
  answered: boolean;
}

export interface SyncProgress {
  syncId: string;
  doneBytes: number;
  totalBytes: number;
}

/** A contract check another node offered. It waits for the local user. */
export interface IncomingContractCheck {
  checkId: string;
  deviceId: string;
  /** Peer text. Render it as plain text only. */
  contract: string;
  changeCount: number;
}

/** What a node answered. A node that has not answered has no entry. */
export interface ContractCheckResult {
  outcome: ContractCheckOutcome | 'expired';
  rows: ContractCheckRowDto[];
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
  /** Syncs that wait for a local answer, or were answered and still run. */
  syncReviews: SyncReviewEntry[];
  syncProgress: SyncProgress[];
  /** The sync whose dialog is open. */
  dialogSyncId: string | null;
  /** Why the open sync ended before it was answered. */
  dialogSyncEndedText: string | null;
  /** Contract checks other nodes offered that wait for a local answer. */
  contractChecksIncoming: IncomingContractCheck[];
  /** Results the provider got, by check id and then device id. */
  contractCheckResults: Record<string, Record<string, ContractCheckResult>>;
  /** The incoming check whose dialog is open. */
  dialogContractCheckId: string | null;
  links: NetworkLink[];
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
  /** Returns false for a repeated or finished sync, so the caller shows nothing twice. */
  addSyncReview: (entry: SyncReviewEntry) => boolean;
  removeSyncReview: (syncId: string) => void;
  markSyncAnswered: (syncId: string) => void;
  setSyncProgress: (progress: SyncProgress) => void;
  /** Removes the sync everywhere. It returns false when the sync had finished before. */
  finishSync: (syncId: string) => boolean;
  openSyncDialog: (syncId: string) => void;
  closeSyncDialog: () => void;
  endSyncDialog: (text: string) => void;
  /** A sync started here. Returns false when it had already finished, so no waiting toast is shown. */
  trackStartedSync: (
    syncId: string,
    info: { displayName: string; collectionName: string; linkId?: string },
  ) => boolean;
  /** Ids of the syncs started here for one link that are still running. */
  startedSyncIdsForLink: (linkId: string) => string[];
  startedSync: (syncId: string) => { displayName: string; collectionName: string } | undefined;
  /** Moves a started sync from waiting to running. It returns the node name the first time, else null. */
  advanceStartedSync: (syncId: string) => string | null;
  /** The local user pressed Cancel. The `cancelled` finish then shows no toast. */
  markSyncCancelledHere: (syncId: string) => void;
  /** Undoes `markSyncCancelledHere` when the cancel call was refused. */
  unmarkSyncCancelledHere: (syncId: string) => void;
  wasSyncCancelledHere: (syncId: string) => boolean;
  /** The local user pressed Decline. The `declined` finish then shows no toast. */
  markSyncDeclinedHere: (syncId: string) => void;
  /** Undoes `markSyncDeclinedHere` when the answer was refused. */
  unmarkSyncDeclinedHere: (syncId: string) => void;
  wasSyncDeclinedHere: (syncId: string) => boolean;
  /** Reloads the links. It never rejects and keeps the old list on a failure. */
  refreshLinks: () => Promise<void>;
  /** Rejects with the backend error and reloads the list either way. */
  unlink: (linkId: string) => Promise<void>;
  /** A repeated offer changes nothing. */
  addIncomingContractCheck: (check: IncomingContractCheck) => void;
  removeIncomingContractCheck: (checkId: string) => void;
  setContractCheckResult: (checkId: string, deviceId: string, result: ContractCheckResult) => void;
  /** Remembers the node a started check went to, so an expiry can mark its result. */
  trackContractCheck: (checkId: string, deviceId: string) => void;
  /** Drops the incoming check and marks an unanswered result as expired. */
  expireContractCheck: (checkId: string) => void;
  openContractCheckDialog: (checkId: string) => void;
  closeContractCheckDialog: () => void;
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
  syncReviews: [] as SyncReviewEntry[],
  syncProgress: [] as SyncProgress[],
  dialogSyncId: null as string | null,
  dialogSyncEndedText: null as string | null,
  contractChecksIncoming: [] as IncomingContractCheck[],
  contractCheckResults: {} as Record<string, Record<string, ContractCheckResult>>,
  dialogContractCheckId: null as string | null,
  links: [] as NetworkLink[],
  error: null as string | null,
};

// The node each started contract check went to, by check id.
const startedContractChecks = new Map<string, string>();

// Expiry timers by pairing id, and the request counters that drop stale responses.
const expiryTimers = new Map<string, ReturnType<typeof setTimeout>>();
let statusSeq = 0;
let nodesSeq = 0;
let linksSeq = 0;

// A transfer id shares the timer map with pairing ids, so its key has a prefix.
const transferTimerKey = (transferId: string): string => `transfer:${transferId}`;

// Ids of transfers that finished. A late progress or offer event for one of them is dropped.
const FINISHED_MEMORY = 100;
const finishedTransfers = new Set<string>();

// Keeps the newest ids only, so the memory cannot grow without bound.
function remember(set: Set<string>, id: string): void {
  set.add(id);
  if (set.size > FINISHED_MEMORY) {
    const oldest = set.values().next().value;
    if (oldest !== undefined) set.delete(oldest);
  }
}

// A sync id shares the timer map too.
const syncTimerKey = (syncId: string): string => `sync:${syncId}`;
const finishedSyncs = new Set<string>();

// Syncs started here, by sync id. `running` turns true on the first progress.
const startedSyncs = new Map<
  string,
  { displayName: string; collectionName: string; linkId?: string; running: boolean }
>();

// Syncs the local user cancelled or declined. Their finish shows no toast about it.
const cancelledSyncsHere = new Set<string>();
const declinedSyncsHere = new Set<string>();

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
    remember(finishedTransfers, transferId);
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

  addSyncReview(entry) {
    if (finishedSyncs.has(entry.syncId)) return false;
    // A repeated event neither adds a row nor extends the expiry.
    if (get().syncReviews.some((r) => r.syncId === entry.syncId)) return false;
    set({ syncReviews: [...get().syncReviews, entry] });
    expiryTimers.set(
      syncTimerKey(entry.syncId),
      setTimeout(() => get().removeSyncReview(entry.syncId), INCOMING_TTL_MS),
    );
    return true;
  },

  removeSyncReview(syncId) {
    clearExpiryTimer(syncTimerKey(syncId));
    if (!get().syncReviews.some((r) => r.syncId === syncId)) return;
    set({ syncReviews: get().syncReviews.filter((r) => r.syncId !== syncId) });
  },

  markSyncAnswered(syncId) {
    if (!get().syncReviews.some((r) => r.syncId === syncId)) return;
    // An answered sync can run longer than the request lifetime.
    clearExpiryTimer(syncTimerKey(syncId));
    set({
      syncReviews: get().syncReviews.map((r) =>
        r.syncId === syncId ? { ...r, answered: true } : r,
      ),
    });
  },

  setSyncProgress(progress) {
    if (finishedSyncs.has(progress.syncId)) return;
    const known = get().syncProgress.some((p) => p.syncId === progress.syncId);
    set({
      syncProgress: known
        ? get().syncProgress.map((p) => (p.syncId === progress.syncId ? progress : p))
        : [...get().syncProgress, progress],
    });
  },

  finishSync(syncId) {
    if (finishedSyncs.has(syncId)) return false;
    remember(finishedSyncs, syncId);
    clearExpiryTimer(syncTimerKey(syncId));
    startedSyncs.delete(syncId);
    cancelledSyncsHere.delete(syncId);
    declinedSyncsHere.delete(syncId);
    set({
      syncReviews: get().syncReviews.filter((r) => r.syncId !== syncId),
      syncProgress: get().syncProgress.filter((p) => p.syncId !== syncId),
    });
    return true;
  },

  openSyncDialog: (dialogSyncId) => set({ dialogSyncId, dialogSyncEndedText: null }),
  closeSyncDialog: () => set({ dialogSyncId: null, dialogSyncEndedText: null }),
  endSyncDialog: (dialogSyncEndedText) => set({ dialogSyncEndedText }),

  trackStartedSync(syncId, info) {
    if (finishedSyncs.has(syncId)) return false;
    startedSyncs.set(syncId, { ...info, running: false });
    return true;
  },

  startedSync(syncId) {
    const entry = startedSyncs.get(syncId);
    return entry
      ? { displayName: entry.displayName, collectionName: entry.collectionName }
      : undefined;
  },

  advanceStartedSync(syncId) {
    const entry = startedSyncs.get(syncId);
    if (!entry || entry.running) return null;
    entry.running = true;
    return entry.displayName;
  },

  startedSyncIdsForLink: (linkId) =>
    [...startedSyncs.entries()].filter(([, e]) => e.linkId === linkId).map(([id]) => id),

  markSyncCancelledHere: (syncId) => {
    remember(cancelledSyncsHere, syncId);
  },
  unmarkSyncCancelledHere: (syncId) => {
    cancelledSyncsHere.delete(syncId);
  },
  wasSyncCancelledHere: (syncId) => cancelledSyncsHere.has(syncId),

  markSyncDeclinedHere: (syncId) => {
    remember(declinedSyncsHere, syncId);
  },
  unmarkSyncDeclinedHere: (syncId) => {
    declinedSyncsHere.delete(syncId);
  },
  wasSyncDeclinedHere: (syncId) => declinedSyncsHere.has(syncId),

  addIncomingContractCheck(check) {
    if (get().contractChecksIncoming.some((c) => c.checkId === check.checkId)) return;
    set({ contractChecksIncoming: [...get().contractChecksIncoming, check] });
  },

  removeIncomingContractCheck(checkId) {
    if (!get().contractChecksIncoming.some((c) => c.checkId === checkId)) return;
    set({
      contractChecksIncoming: get().contractChecksIncoming.filter((c) => c.checkId !== checkId),
    });
  },

  setContractCheckResult(checkId, deviceId, result) {
    const current = get().contractCheckResults;
    set({
      contractCheckResults: { ...current, [checkId]: { ...current[checkId], [deviceId]: result } },
    });
  },

  trackContractCheck(checkId, deviceId) {
    startedContractChecks.set(checkId, deviceId);
  },

  expireContractCheck(checkId) {
    get().removeIncomingContractCheck(checkId);
    const deviceId = startedContractChecks.get(checkId);
    if (deviceId === undefined) return;
    // An expired check is over, so it is no longer tracked.
    startedContractChecks.delete(checkId);
    // An answered node keeps its answer.
    if (get().contractCheckResults[checkId]?.[deviceId]) return;
    get().setContractCheckResult(checkId, deviceId, { outcome: 'expired', rows: [] });
  },

  openContractCheckDialog: (dialogContractCheckId) => set({ dialogContractCheckId }),
  closeContractCheckDialog: () => set({ dialogContractCheckId: null }),

  async refreshLinks() {
    const seq = ++linksSeq;
    try {
      const links = await listNetworkLinks();
      // An unchanged list keeps the old array, so nothing re-renders.
      if (seq === linksSeq && JSON.stringify(links) !== JSON.stringify(get().links)) set({ links });
    } catch {
      // The old list stays. A failed load is not a panel error.
    }
  },

  async unlink(linkId) {
    try {
      await unlinkNetworkLink(linkId);
    } finally {
      // The list is the truth, also after a failure. refreshLinks never throws.
      await get().refreshLinks();
    }
  },

  reset() {
    for (const timer of expiryTimers.values()) clearTimeout(timer);
    expiryTimers.clear();
    finishedTransfers.clear();
    outgoing.clear();
    cancelledHere.clear();
    finishedSyncs.clear();
    startedSyncs.clear();
    cancelledSyncsHere.clear();
    declinedSyncsHere.clear();
    startedContractChecks.clear();
    // Bumping the counters makes any request still in flight a no-op.
    statusSeq++;
    nodesSeq++;
    linksSeq++;
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
