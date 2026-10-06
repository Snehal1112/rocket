import { create } from 'zustand';
import type { WebSocketMessageEvent, WebSocketStatusEvent } from '@/lib/tauri-api';
import type { MessageLogEntry } from '@/types/message-log';

/** The log keeps this many entries per tab; older ones are dropped. */
export const MAX_LOG_ENTRIES = 1000;

export type ConnectionStatus = 'idle' | 'connecting' | 'open' | 'closed' | 'failed';

export interface TabSession {
  /** Id of the live session, or null when none is connecting or open. */
  sessionId: string | null;
  status: ConnectionStatus;
  subprotocol: string | null;
  error: string | null;
  log: MessageLogEntry[];
}

export const IDLE_SESSION: TabSession = {
  sessionId: null,
  status: 'idle',
  subprotocol: null,
  error: null,
  log: [],
};

/** Appends and keeps only the newest `max` items. */
export function appendCapped<T>(list: T[], entry: T, max = MAX_LOG_ENTRIES): T[] {
  const next = [...list, entry];
  return next.length > max ? next.slice(next.length - max) : next;
}

let entrySeq = 0;
const nextEntryId = () => `ws-log-${++entrySeq}`;

function systemEntry(text: string, timestampMs = Date.now()): MessageLogEntry {
  return {
    id: nextEntryId(),
    direction: 'system',
    kind: 'text',
    data: text,
    size: 0,
    timestampMs,
  };
}

function closeText(code: number | null, reason: string | null): string {
  const head = code === null ? 'Closed' : `Closed ${code}`;
  return reason ? `${head}: ${reason}` : head;
}

interface WebSocketStoreState {
  byTab: Record<string, TabSession>;
  tabBySession: Record<string, string>;
  /** Registers a session for a tab before `ws_connect` is invoked, so no early event is lost. */
  beginSession: (tabId: string, sessionId: string) => void;
  applyMessage: (event: WebSocketMessageEvent) => void;
  applyStatus: (event: WebSocketStatusEvent) => void;
  /** A rejected connect call. A no-op when a status event already ended the session. */
  failSession: (tabId: string, error: string) => void;
  clearLog: (tabId: string) => void;
  forgetTab: (tabId: string) => void;
}

function withoutKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  const { [key]: _removed, ...rest } = record;
  return rest;
}

export const useWebSocketStore = create<WebSocketStoreState>((set, get) => ({
  byTab: {},
  tabBySession: {},

  beginSession(tabId, sessionId) {
    const { byTab, tabBySession } = get();
    const previous = byTab[tabId] ?? IDLE_SESSION;
    const mapping = previous.sessionId
      ? withoutKey(tabBySession, previous.sessionId)
      : tabBySession;
    set({
      byTab: {
        ...byTab,
        [tabId]: { ...previous, sessionId, status: 'connecting', subprotocol: null, error: null },
      },
      tabBySession: { ...mapping, [sessionId]: tabId },
    });
  },

  applyMessage(event) {
    const { byTab, tabBySession } = get();
    const tabId = tabBySession[event.session_id];
    if (!tabId) return;
    const session = byTab[tabId];
    if (!session) return;
    const entry: MessageLogEntry = {
      id: nextEntryId(),
      direction: event.direction,
      kind: event.kind,
      data: event.data,
      size: event.size,
      timestampMs: event.timestamp_ms,
    };
    set({ byTab: { ...byTab, [tabId]: { ...session, log: appendCapped(session.log, entry) } } });
  },

  applyStatus(event) {
    const { byTab, tabBySession } = get();
    const tabId = tabBySession[event.session_id];
    if (!tabId) return;
    const session = byTab[tabId];
    if (!session) return;

    if (event.state === 'connecting') {
      set({ byTab: { ...byTab, [tabId]: { ...session, status: 'connecting' } } });
      return;
    }
    if (event.state === 'open') {
      const label = event.subprotocol ? `Connected (${event.subprotocol})` : 'Connected';
      set({
        byTab: {
          ...byTab,
          [tabId]: {
            ...session,
            status: 'open',
            subprotocol: event.subprotocol,
            error: null,
            log: appendCapped(session.log, systemEntry(label)),
          },
        },
      });
      return;
    }

    // closed or failed: the session is over, so later events for this id are ignored.
    const failed = event.state === 'failed';
    const text = failed
      ? `Failed: ${event.reason ?? 'connection lost'}`
      : closeText(event.code, event.reason);
    set({
      byTab: {
        ...byTab,
        [tabId]: {
          ...session,
          sessionId: null,
          status: failed ? 'failed' : 'closed',
          error: failed ? (event.reason ?? 'connection lost') : null,
          log: appendCapped(session.log, systemEntry(text)),
        },
      },
      tabBySession: withoutKey(tabBySession, event.session_id),
    });
  },

  failSession(tabId, error) {
    const { byTab, tabBySession } = get();
    const session = byTab[tabId];
    if (!session || session.status !== 'connecting') return;
    set({
      byTab: {
        ...byTab,
        [tabId]: {
          ...session,
          sessionId: null,
          status: 'failed',
          error,
          log: appendCapped(session.log, systemEntry(`Failed: ${error}`)),
        },
      },
      tabBySession: session.sessionId ? withoutKey(tabBySession, session.sessionId) : tabBySession,
    });
  },

  clearLog(tabId) {
    const { byTab } = get();
    const session = byTab[tabId];
    if (!session) return;
    set({ byTab: { ...byTab, [tabId]: { ...session, log: [] } } });
  },

  forgetTab(tabId) {
    const { byTab, tabBySession } = get();
    const session = byTab[tabId];
    set({
      byTab: withoutKey(byTab, tabId),
      tabBySession: session?.sessionId ? withoutKey(tabBySession, session.sessionId) : tabBySession,
    });
  },
}));
