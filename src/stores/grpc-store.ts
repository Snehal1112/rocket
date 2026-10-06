import { create } from 'zustand';
import {
  type GrpcPair,
  type GrpcStatus,
  type GrpcUnaryResponse,
  grpcCancelSession,
  onGrpcSessionFinished,
  onGrpcSessionHeaders,
  onGrpcSessionMessage,
  onGrpcSessionStarted,
} from '@/lib/tauri-api';

/** A stream that runs for hours must not grow the log without limit. */
export const MAX_GRPC_LOG_ENTRIES = 1000;

export type GrpcLogDirection = 'in' | 'out';

export interface GrpcLogEntry {
  id: number;
  direction: GrpcLogDirection;
  /** Protobuf JSON text. */
  json: string;
  at: number;
}

export interface GrpcSessionView {
  id: string;
  methodType: string;
  status: 'running' | 'finished';
  headers: GrpcPair[];
  log: GrpcLogEntry[];
  nextLogId: number;
  finished?: { status: GrpcStatus; trailers: GrpcPair[]; durationMs: number };
}

export interface GrpcUnaryView {
  status: 'sending' | 'done' | 'error';
  response?: GrpcUnaryResponse;
  error?: string;
}

interface GrpcStoreState {
  /** Every session the backend has told us about, by session id. */
  sessions: Record<string, GrpcSessionView>;
  /** The session a tab started most recently. */
  sessionByTab: Record<string, string>;
  unaryByTab: Record<string, GrpcUnaryView>;
  attachSession: (tabId: string, sessionId: string) => void;
  /** Forgets a session that never opened. Does nothing once the tab shows a newer one. */
  detachSession: (tabId: string, sessionId: string) => void;
  recordOutbound: (sessionId: string, json: string) => void;
  setUnary: (tabId: string, view: GrpcUnaryView | undefined) => void;
  cancelTabSession: (tabId: string) => Promise<void>;
  /** Forgets everything about a closed tab. Cancels its session first. */
  dropTab: (tabId: string) => Promise<void>;
}

function newSession(id: string): GrpcSessionView {
  return { id, methodType: '', status: 'running', headers: [], log: [], nextLogId: 0 };
}

function appended(
  session: GrpcSessionView,
  direction: GrpcLogDirection,
  json: string,
): GrpcSessionView {
  const entry: GrpcLogEntry = { id: session.nextLogId, direction, json, at: Date.now() };
  const log = [...session.log, entry];
  return {
    ...session,
    log: log.length > MAX_GRPC_LOG_ENTRIES ? log.slice(log.length - MAX_GRPC_LOG_ENTRIES) : log,
    nextLogId: session.nextLogId + 1,
  };
}

export const useGrpcStore = create<GrpcStoreState>()((set, get) => ({
  sessions: {},
  sessionByTab: {},
  unaryByTab: {},

  attachSession(tabId, sessionId) {
    set((state) => {
      const previous = state.sessionByTab[tabId];
      const sessions = { ...state.sessions };
      // The tab shows one session at a time, so the older one can go.
      if (previous && previous !== sessionId && sessions[previous]?.status === 'finished') {
        delete sessions[previous];
      }
      // The call is already running, whether or not an event has arrived for it yet.
      sessions[sessionId] = sessions[sessionId] ?? newSession(sessionId);
      return { sessions, sessionByTab: { ...state.sessionByTab, [tabId]: sessionId } };
    });
  },

  detachSession(tabId, sessionId) {
    set((state) => {
      if (state.sessionByTab[tabId] !== sessionId) return state;
      const sessions = { ...state.sessions };
      delete sessions[sessionId];
      const sessionByTab = { ...state.sessionByTab };
      delete sessionByTab[tabId];
      return { sessions, sessionByTab };
    });
  },

  recordOutbound(sessionId, json) {
    set((state) => {
      const session = state.sessions[sessionId] ?? newSession(sessionId);
      return { sessions: { ...state.sessions, [sessionId]: appended(session, 'out', json) } };
    });
  },

  setUnary(tabId, view) {
    set((state) => {
      const unaryByTab = { ...state.unaryByTab };
      if (view) unaryByTab[tabId] = view;
      else delete unaryByTab[tabId];
      return { unaryByTab };
    });
  },

  async cancelTabSession(tabId) {
    const id = get().sessionByTab[tabId];
    if (!id || get().sessions[id]?.status === 'finished') return;
    try {
      await grpcCancelSession(id);
    } catch (err) {
      // The call may have finished on its own a moment ago. Nothing is left to cancel.
      console.warn('[grpc] cancel failed:', err);
    }
  },

  async dropTab(tabId) {
    await get().cancelTabSession(tabId);
    set((state) => {
      const id = state.sessionByTab[tabId];
      const sessions = { ...state.sessions };
      if (id) delete sessions[id];
      const sessionByTab = { ...state.sessionByTab };
      delete sessionByTab[tabId];
      const unaryByTab = { ...state.unaryByTab };
      delete unaryByTab[tabId];
      return { sessions, sessionByTab, unaryByTab };
    });
  },
}));

function update(sessionId: string, change: (session: GrpcSessionView) => GrpcSessionView) {
  useGrpcStore.setState((state) => ({
    sessions: {
      ...state.sessions,
      [sessionId]: change(state.sessions[sessionId] ?? newSession(sessionId)),
    },
  }));
}

let listening: Promise<void> | null = null;

/**
 * Subscribes to the session events once for the life of the app. Events can arrive
 * before the command that started the session has returned, so a session is created
 * by whichever event reaches the store first.
 */
export function ensureGrpcListeners(): Promise<void> {
  if (!listening) {
    listening = Promise.all([
      onGrpcSessionStarted((e) =>
        update(e.session_id, (s) => ({ ...s, methodType: e.method_type })),
      ),
      onGrpcSessionHeaders((e) => update(e.session_id, (s) => ({ ...s, headers: e.headers }))),
      onGrpcSessionMessage((e) => update(e.session_id, (s) => appended(s, 'in', e.json))),
      onGrpcSessionFinished((e) =>
        update(e.session_id, (s) => ({
          ...s,
          status: 'finished',
          finished: {
            status: { code: e.code, codeName: e.code_name, message: e.message },
            trailers: e.trailers,
            durationMs: e.duration_ms,
          },
        })),
      ),
    ])
      .then(() => undefined)
      .catch((err) => {
        listening = null;
        console.error('[grpc] could not listen for session events:', err);
      });
  }
  return listening;
}
