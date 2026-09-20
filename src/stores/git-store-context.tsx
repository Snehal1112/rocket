import { createContext, type ReactNode, useContext } from 'react';
import { useStore } from 'zustand';
import type { StoreApi } from 'zustand/vanilla';
import type { GitState } from './git-store';

const GitStoreContext = createContext<StoreApi<GitState> | null>(null);

interface GitStoreProviderProps {
  store: StoreApi<GitState>;
  children: ReactNode;
}

/** Provides one GitPanel's own isolated git-store instance to its subtree. */
export function GitStoreProvider({ store, children }: GitStoreProviderProps) {
  return <GitStoreContext.Provider value={store}>{children}</GitStoreContext.Provider>;
}

function useStoreApi(): StoreApi<GitState> {
  const api = useContext(GitStoreContext);
  if (!api) {
    throw new Error('useGitStore must be used within a GitStoreProvider');
  }
  return api;
}

/** Reactive selector hook, scoped to the nearest GitStoreProvider's store instance. */
export function useGitStore<T>(selector: (state: GitState) => T): T {
  return useStore(useStoreApi(), selector);
}

/** Imperative access (`.getState()`/`.setState()`) to the nearest GitStoreProvider's store instance. */
export function useGitStoreApi(): StoreApi<GitState> {
  return useStoreApi();
}
