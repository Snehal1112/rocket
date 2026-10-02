import { create } from 'zustand';

const MAX_ENTRIES = 200;
/** The backend log buffer is separate so logs never push out other entries. */
export const MAX_LOG_ENTRIES = 500;

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
  /** Names the source of the entry, such as a Flow node. */
  requestName?: string;
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

export type BackendLogLevel = 'INFO' | 'WARN' | 'ERROR';

/** Payload of the "backend-log" Tauri event (camelCase JSON from Rust). */
export interface BackendLogPayload {
  timestamp: string;
  level: string;
  target: string;
  message: string;
  fields: Record<string, string>;
  spanFields: Record<string, string>;
}

export interface LogConsoleEntry {
  kind: 'log';
  id: string;
  timestamp: string;
  level: BackendLogLevel;
  target: string;
  message: string;
  fields: Record<string, string>;
  spanFields: Record<string, string>;
}

/** Maps tracing's level text (for example "WARN") to a known level. Unknown values become INFO. */
export function normalizeLogLevel(level: string): BackendLogLevel {
  const upper = level.trim().toUpperCase();
  if (upper === 'ERROR') return 'ERROR';
  if (upper === 'WARN' || upper === 'WARNING') return 'WARN';
  return 'INFO';
}

export type ConsoleEntry = HttpConsoleEntry | ScriptLogEntry | TestResultEntry;

interface ConsoleState {
  entries: ConsoleEntry[];
  /** Backend log entries, newest first, in their own capped buffer. */
  logEntries: LogConsoleEntry[];
  addLogEntry: (payload: BackendLogPayload) => void;
  addHttpEntry: (entry: Omit<HttpConsoleEntry, 'id' | 'timestamp' | 'kind'>) => void;
  addScriptEntry: (entry: Omit<ScriptLogEntry, 'id' | 'timestamp' | 'kind'>) => void;
  addTestEntry: (entry: Omit<TestResultEntry, 'id' | 'timestamp' | 'kind'>) => void;
  /** Adds all entries in one update, preserving their given order (earliest first). */
  addScriptEntries: (entries: Omit<ScriptLogEntry, 'id' | 'timestamp' | 'kind'>[]) => void;
  /** Adds all entries in one update, preserving their given order (earliest first). */
  addTestEntries: (entries: Omit<TestResultEntry, 'id' | 'timestamp' | 'kind'>[]) => void;
  clearEntries: () => void;
}

export const useConsoleStore = create<ConsoleState>((set) => ({
  entries: [],
  logEntries: [],

  addLogEntry: (payload) => {
    const full: LogConsoleEntry = {
      kind: 'log',
      id: crypto.randomUUID(),
      timestamp: payload.timestamp,
      level: normalizeLogLevel(payload.level),
      target: payload.target,
      message: payload.message,
      fields: payload.fields ?? {},
      spanFields: payload.spanFields ?? {},
    };
    set((state) => ({
      logEntries: [full, ...state.logEntries].slice(0, MAX_LOG_ENTRIES),
    }));
  },

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

  addScriptEntries: (entries) => {
    if (entries.length === 0) return;
    const full: ScriptLogEntry[] = entries.map((entry) => ({
      ...entry,
      kind: 'script',
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    }));
    set((state) => ({
      entries: [...full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },

  addTestEntries: (entries) => {
    if (entries.length === 0) return;
    const full: TestResultEntry[] = entries.map((entry) => ({
      ...entry,
      kind: 'test',
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    }));
    set((state) => ({
      entries: [...full, ...state.entries].slice(0, MAX_ENTRIES),
    }));
  },

  clearEntries: () => set({ entries: [], logEntries: [] }),
}));
