import { create } from 'zustand';

const STORAGE_KEY = 'rocket-sandbox-mode';

type SandboxMode = 'safe' | 'developer';

function readPersistedMode(): SandboxMode {
  if (typeof window === 'undefined' || !window.localStorage) {
    return 'safe';
  }
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored === 'developer' ? 'developer' : 'safe';
}

interface SandboxState {
  mode: SandboxMode;
  setMode: (mode: SandboxMode) => void;
}

export const useSandboxStore = create<SandboxState>((set) => ({
  mode: readPersistedMode(),

  setMode(mode) {
    if (typeof window !== 'undefined' && window.localStorage) {
      localStorage.setItem(STORAGE_KEY, mode);
    }
    set({ mode });
  },
}));
