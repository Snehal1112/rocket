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

export interface NetworkState {
  status: NetworkStatus | null;
  nodes: NetworkNode[];
  incoming: PairingIncoming[];
  activePairing: ActivePairing | null;
  panelOpen: boolean;
  /** The pairing whose dialog is open, for either role. */
  dialogPairingId: string | null;
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
  reset: () => void;
}

const initialState = {
  status: null as NetworkStatus | null,
  nodes: [] as NetworkNode[],
  incoming: [] as PairingIncoming[],
  activePairing: null as ActivePairing | null,
  panelOpen: false,
  dialogPairingId: null as string | null,
  error: null as string | null,
};

// Expiry timers by pairing id, and the request counters that drop stale responses.
const expiryTimers = new Map<string, ReturnType<typeof setTimeout>>();
let statusSeq = 0;
let nodesSeq = 0;

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

  reset() {
    for (const timer of expiryTimers.values()) clearTimeout(timer);
    expiryTimers.clear();
    // Bumping the counters makes any request still in flight a no-op.
    statusSeq++;
    nodesSeq++;
    set({ ...initialState });
  },
}));

/** Paired nodes that are online. It feeds the number in the status bar. */
export const selectOnlinePairedCount = (s: NetworkState): number =>
  s.nodes.filter((n) => n.group === 'paired' && n.status === 'online').length;

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
