import { create } from 'zustand';

const MAX_ENTRIES = 200;

export interface HttpConsoleEntry {
  kind: 'http';
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

export interface ScriptLogEntry {
  kind: 'script';
  id: string;
  timestamp: string;
  level: 'log' | 'warn' | 'error';
  message: string;
  requestName: string;
}

export interface TestResultEntry {
  kind: 'test';
  id: string;
  timestamp: string;
  name: string;
  status: 'passed' | 'failed';
  error: string | null;
  requestName: string;
}

export type ConsoleEntry = HttpConsoleEntry | ScriptLogEntry | TestResultEntry;

interface ConsoleState {
  entries: ConsoleEntry[];
  addHttpEntry: (entry: Omit<HttpConsoleEntry, 'id' | 'timestamp' | 'kind'>) => void;
  addScriptEntry: (entry: Omit<ScriptLogEntry, 'id' | 'timestamp' | 'kind'>) => void;
  addTestEntry: (entry: Omit<TestResultEntry, 'id' | 'timestamp' | 'kind'>) => void;
  clearEntries: () => void;
}

export const useConsoleStore = create<ConsoleState>((set) => ({
  entries: [],

  addHttpEntry: (entry) => {
    const full: HttpConsoleEntry = {
      ...entry,
      kind: 'http',
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    };
    set((state) => ({
      entries: [full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },

  addScriptEntry: (entry) => {
    const full: ScriptLogEntry = {
      ...entry,
      kind: 'script',
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    };
    set((state) => ({
      entries: [full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },

  addTestEntry: (entry) => {
    const full: TestResultEntry = {
      ...entry,
      kind: 'test',
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    };
    set((state) => ({
      entries: [full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },

  clearEntries: () => set({ entries: [] }),
}));
