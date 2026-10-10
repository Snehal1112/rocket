import { create } from 'zustand';
import { listEnvironments } from '@/lib/tauri-api';

interface EnvState {
  activeEnvId: string | null;
  activeCollection: string | null;
  setActiveEnvId: (id: string | null) => void;
  setActiveCollection: (name: string | null) => void;
}

export const useEnvStore = create<EnvState>()((set) => ({
  activeEnvId: null,
  activeCollection: null,

  setActiveEnvId: (id) => {
    const { activeCollection } = useEnvStore.getState();
    set({ activeEnvId: id });
    const key = activeCollection ? `rocket-api:active-env:${activeCollection}` : null;
    if (key) {
      if (id) localStorage.setItem(key, id);
      else localStorage.removeItem(key);
    }
  },

  setActiveCollection: (name) => set({ activeCollection: name }),
}));

/**
 * Selects the environment last chosen for `collection`. The choice is stored by collection
 * name only, so it can name an environment of a same-named collection in another workspace.
 * It is checked against this workspace's list and dropped (in memory only) when missing.
 */
export function restoreActiveEnv(collection: string): void {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(`rocket-api:active-env:${collection}`);
  } catch {
    // Storage can be unavailable. Start with no environment selected.
  }
  useEnvStore.getState().setActiveEnvId(stored);
  if (!stored) return;
  const chosen = stored;
  listEnvironments(collection)
    .then((environments) => {
      const state = useEnvStore.getState();
      const stillChosen = state.activeCollection === collection && state.activeEnvId === chosen;
      if (stillChosen && !environments.some((env) => env.name === chosen)) {
        // The stored choice is kept for the workspace it came from.
        useEnvStore.setState({ activeEnvId: null });
      }
    })
    .catch(() => {
      // The list cannot be read. Keep the choice; a send reports a missing environment.
    });
}
