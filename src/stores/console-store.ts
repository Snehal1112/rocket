import { create } from 'zustand';

const MAX_ENTRIES = 200;

export interface ConsoleEntry {
  id: string;
  timestamp: string;
  method: string;
  url: string;
  status: number;
  statusText: string;
  durationMs: number;
  sizeBytes: number;
  requestHeaders: { key: string; value: string }[];
  requestBody: string;
  responseHeaders: { key: string; value: string }[];
  responseBody: string;
}

interface ConsoleState {
  entries: ConsoleEntry[];
  addEntry: (entry: Omit<ConsoleEntry, 'id' | 'timestamp'>) => void;
  clearEntries: () => void;
}

export const useConsoleStore = create<ConsoleState>((set) => ({
  entries: [],

  addEntry: (entry) => {
    const full: ConsoleEntry = {
      ...entry,
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    };
    set((state) => ({
      entries: [full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },

  clearEntries: () => set({ entries: [] }),
}));
