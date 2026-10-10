import { create } from 'zustand';
import { flowAuthKeyMatches } from '@/lib/flow-auth';
import type { AuthState } from '@/types/pane-types';

/**
 * One Auth node's in-memory state. `fingerprint` identifies the resolved
 * configuration (variables substituted) a held OAuth2 token was fetched for;
 * a token whose fingerprint no longer matches is not used.
 */
export interface FlowAuthEntry {
  auth: AuthState;
  fingerprint?: string;
}

// In-memory only. Holds each Auth node's full AuthState, including any fetched
// OAuth2 token, keyed by `flowAuthKey(collection, flow, node, environment,
// global environment)`. Nothing here is persisted: no `persist` middleware, no
// storage, lost on reload by design.
interface FlowAuthStore {
  auths: Record<string, FlowAuthEntry>;
  /** Per flow, raised by every clear of that flow. Read it through `epochOf`. */
  clearEpochs: Record<string, number>;
  /** Raised when everything is cleared at once (workspace switch). */
  allEpoch: number;
  /**
   * Changes whenever the flow's tokens are cleared, or all of them are. A
   * sign-in that started before a clear compares it and drops its token.
   */
  epochOf: (collection: string, flowName: string) => number;
  setAuth: (key: string, auth: AuthState, fingerprint?: string) => void;
  getAuth: (key: string) => AuthState | undefined;
  getEntry: (key: string) => FlowAuthEntry | undefined;
  clearAuth: (key: string) => void;
  /** Removes one node's entries in every environment variant. */
  clearNode: (collection: string, flowName: string, nodeId: string) => void;
  /** Removes every entry of every flow. */
  clearAll: () => void;
  /** Removes every entry of a flow. */
  clearFlow: (collection: string, flowName: string) => void;
}

export const useFlowAuthStore = create<FlowAuthStore>()((set, get) => ({
  auths: {},
  clearEpochs: {},
  allEpoch: 0,

  epochOf(collection, flowName) {
    const { allEpoch, clearEpochs } = get();
    return allEpoch * EPOCH_STRIDE + (clearEpochs[epochKey(collection, flowName)] ?? 0);
  },

  setAuth(key, auth, fingerprint) {
    set({ auths: { ...get().auths, [key]: { auth, fingerprint } } });
  },

  getAuth(key) {
    return get().auths[key]?.auth;
  },

  getEntry(key) {
    return get().auths[key];
  },

  clearAuth(key) {
    const { [key]: _removed, ...rest } = get().auths;
    set({ auths: rest });
    const flow = flowOfKey(key);
    if (flow) bumpEpoch(set, get, flow[0], flow[1]);
  },

  clearAll() {
    set({ auths: {}, allEpoch: get().allEpoch + 1 });
  },

  clearNode(collection, flowName, nodeId) {
    bumpEpoch(set, get, collection, flowName);
    removeWhere(set, get, (key) => flowAuthKeyMatches(key, collection, flowName, nodeId));
  },

  clearFlow(collection, flowName) {
    bumpEpoch(set, get, collection, flowName);
    removeWhere(set, get, (key) => flowAuthKeyMatches(key, collection, flowName));
  },
}));

// Per-flow epochs stay below this, so the sum with the global epoch never collides.
const EPOCH_STRIDE = 1_000_000;

const epochKey = (collection: string, flowName: string) => JSON.stringify([collection, flowName]);

// The collection and flow name of a store key, or undefined when it does not parse.
function flowOfKey(key: string): [string, string] | undefined {
  try {
    const parts: unknown = JSON.parse(key);
    if (Array.isArray(parts) && typeof parts[0] === 'string' && typeof parts[1] === 'string') {
      return [parts[0], parts[1]];
    }
  } catch {
    // A key that does not parse belongs to no flow.
  }
  return undefined;
}

function bumpEpoch(
  set: (partial: Partial<FlowAuthStore>) => void,
  get: () => FlowAuthStore,
  collection: string,
  flowName: string,
) {
  const id = epochKey(collection, flowName);
  const epochs = get().clearEpochs;
  set({ clearEpochs: { ...epochs, [id]: (epochs[id] ?? 0) + 1 } });
}

function removeWhere(
  set: (partial: Partial<FlowAuthStore>) => void,
  get: () => FlowAuthStore,
  match: (key: string) => boolean,
) {
  const current = get().auths;
  const keys = Object.keys(current);
  if (!keys.some(match)) return;
  set({ auths: Object.fromEntries(keys.filter((k) => !match(k)).map((k) => [k, current[k]])) });
}
