import { create } from 'zustand';
import type { AuthState } from '@/types/pane-types';

// In-memory only. Holds each Auth node's full AuthState, including any fetched
// OAuth2 token, keyed by `flowAuthKey(collection, flow, node)`. Nothing here is
// persisted: no `persist` middleware, no storage, lost on reload by design.
interface FlowAuthStore {
  auths: Record<string, AuthState>;
  setAuth: (key: string, auth: AuthState) => void;
  getAuth: (key: string) => AuthState | undefined;
  clearAuth: (key: string) => void;
}

export const useFlowAuthStore = create<FlowAuthStore>()((set, get) => ({
  auths: {},

  setAuth(key, auth) {
    set({ auths: { ...get().auths, [key]: auth } });
  },

  getAuth(key) {
    return get().auths[key];
  },

  clearAuth(key) {
    const { [key]: _removed, ...rest } = get().auths;
    set({ auths: rest });
  },
}));
