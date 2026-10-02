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
  setAuth: (key: string, auth: AuthState, fingerprint?: string) => void;
  getAuth: (key: string) => AuthState | undefined;
  getEntry: (key: string) => FlowAuthEntry | undefined;
  clearAuth: (key: string) => void;
  /** Removes one node's entries in every environment variant. */
  clearNode: (collection: string, flowName: string, nodeId: string) => void;
  /** Removes every entry of a flow. */
  clearFlow: (collection: string, flowName: string) => void;
}

export const useFlowAuthStore = create<FlowAuthStore>()((set, get) => ({
  auths: {},

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
  },

  clearNode(collection, flowName, nodeId) {
    removeWhere(set, get, (key) => flowAuthKeyMatches(key, collection, flowName, nodeId));
  },

  clearFlow(collection, flowName) {
    removeWhere(set, get, (key) => flowAuthKeyMatches(key, collection, flowName));
  },
}));

function removeWhere(
  set: (partial: { auths: Record<string, FlowAuthEntry> }) => void,
  get: () => FlowAuthStore,
  match: (key: string) => boolean,
) {
  const current = get().auths;
  const keys = Object.keys(current);
  if (!keys.some(match)) return;
  set({ auths: Object.fromEntries(keys.filter((k) => !match(k)).map((k) => [k, current[k]])) });
}
