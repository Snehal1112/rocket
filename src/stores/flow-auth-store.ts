import { create } from 'zustand';
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
}));
