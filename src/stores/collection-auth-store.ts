import { create } from 'zustand';
import type { AuthState } from '@/types/pane-types';

// Stores the current auth state for each collection (keyed by collection name).
// This allows execute-request.ts to resolve inherited auth when a collection uses
// OAuth2 flows (e.g. authorization_code) whose access tokens are never persisted to disk.
interface CollectionAuthStore {
  auths: Map<string, AuthState>;
  setCollectionAuth: (collectionName: string, auth: AuthState) => void;
  getCollectionAuth: (collectionName: string) => AuthState | undefined;
  clearCollectionAuth: (collectionName: string) => void;
}

export const useCollectionAuthStore = create<CollectionAuthStore>()((set, get) => ({
  auths: new Map(),

  setCollectionAuth(collectionName, auth) {
    const next = new Map(get().auths);
    next.set(collectionName, auth);
    set({ auths: next });
  },

  getCollectionAuth(collectionName) {
    return get().auths.get(collectionName);
  },

  clearCollectionAuth(collectionName) {
    const next = new Map(get().auths);
    next.delete(collectionName);
    set({ auths: next });
  },
}));
