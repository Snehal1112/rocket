import { create } from 'zustand';

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
