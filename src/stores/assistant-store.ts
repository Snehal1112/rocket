import { create } from 'zustand';
import type {
  AgentPermissionOption,
  AgentProposal,
  AgentProposalStatus,
  AgentToolCallStatus,
  AssistantMode,
  ConfigOption,
} from '@/lib/tauri-api';

export type { AssistantMode };
export type AssistantSessionStatus = 'starting' | 'active' | 'ended' | 'error';
export type ToolActivityStatus = AgentToolCallStatus;
export type ProposalStatus = AgentProposalStatus;

export interface UserMessage {
  kind: 'user';
  id: string;
  text: string;
}

/** One segment of the assistant's reply. A turn that calls tools has several. */
export interface AgentMessage {
  kind: 'agent';
  id: string;
  text: string;
  streaming: boolean;
  error?: string;
}

export interface ToolActivityMessage {
  kind: 'tool';
  id: string;
  callId: string;
  title: string;
  status: ToolActivityStatus;
}

export interface NoticeMessage {
  kind: 'notice';
  id: string;
  text: string;
}

export type AssistantMessage = UserMessage | AgentMessage | ToolActivityMessage | NoticeMessage;

export interface AssistantSession {
  sessionId: string;
  agentConfigId: string;
  status: AssistantSessionStatus;
  configOptions: ConfigOption[];
  mode: AssistantMode;
  error?: string;
}

export interface AssistantUsage {
  used: number;
  size: number;
  costUsd?: number;
}

export interface AssistantFocus {
  collection: string;
  path: string;
}

/** A stored conversation carried into a new session (spec section 7, stage 2). */
export interface ReplaySeed {
  /** The id of the stored conversation, saved as `continuedFrom`. */
  fromId: string;
  /** The text put in front of the session's first prompt. */
  preface: string;
}

export interface ToolActivity {
  callId: string;
  title: string;
  status: ToolActivityStatus;
}

export const MAX_REASONING_CHARS = 20_000;

export interface AgentPermission {
  requestId: string;
  callId: string;
  title: string;
  kind: string;
  summary: string;
  options: AgentPermissionOption[];
  status: 'pending' | 'answered' | 'cancelled';
  /** The name of the option the user chose, once answered. */
  chosenName?: string;
}

/** A request that is still waiting becomes cancelled; answered ones stay. */
function cancelPending(permissions: AgentPermission[]): AgentPermission[] {
  return permissions.map((p) => (p.status === 'pending' ? { ...p, status: 'cancelled' } : p));
}

export interface AssistantState {
  session?: AssistantSession;
  messages: AssistantMessage[];
  proposals: AgentProposal[];
  /** Where each proposal sits in the chat: the id of the last message when it
   *  arrived, or null for the start. Kept apart from the wire DTO. */
  proposalAnchors: Record<string, string | null>;
  /** The agent's permission requests for this session, in arrival order. */
  permissions: AgentPermission[];
  usage?: AssistantUsage;
  /** The agent's reasoning for the current turn. Cleared at the next user message. */
  reasoning: string;
  /** When the active session last sent anything (text, thought, tool or usage). */
  lastActivityAt?: number;
  focus?: AssistantFocus;
  panelOpen: boolean;
  /** The stored conversation's id. Made at the first user message of a session. */
  conversationId?: string;
  /** The seed of a continued session. `sent` turns true once the first prompt carried it. */
  replay?: ReplaySeed & { sent: boolean };

  openPanel: () => void;
  closePanel: () => void;
  setFocus: (focus: AssistantFocus | undefined) => void;
  /** Puts a new session in 'starting' and returns the token of this start. */
  beginSession: (agentConfigId: string, mode: AssistantMode, replay?: ReplaySeed) => number;
  /** Activates the start with this token. Returns false when that start was
   *  abandoned, so the caller must end the backend session itself. */
  activateSession: (token: number, sessionId: string, configOptions: ConfigOption[]) => boolean;
  failStart: (token: number, error: string) => void;
  /** Adds the user's text and opens the streaming reply. Returns false when
   *  there is no active session or a turn is already running. */
  appendUserMessage: (text: string) => boolean;
  appendChunk: (sessionId: string, text: string) => void;
  /** Adds reasoning text. Keeps only the newest MAX_REASONING_CHARS characters. */
  appendReasoning: (sessionId: string, text: string) => void;
  /** Marks that the session sent something, for the quiet-agent notice. */
  touchActivity: (sessionId: string) => void;
  completeMessage: (sessionId: string) => void;
  /** A fatal failure ends the session. A non-fatal one only fails the turn. */
  failMessage: (sessionId: string, error: string, fatal?: boolean) => void;
  upsertToolActivity: (sessionId: string, activity: ToolActivity) => void;
  setConfigOptions: (sessionId: string, options: ConfigOption[]) => void;
  /** Records the mode the backend switched the current session to. */
  setMode: (sessionId: string, mode: AssistantMode) => void;
  /** Records that the first prompt of this session carried the replay preface. */
  markReplaySent: (sessionId: string) => void;
  setUsage: (sessionId: string, usage: AssistantUsage) => void;
  upsertProposal: (proposal: AgentProposal) => void;
  resolveProposal: (sessionId: string, proposalId: string, status: ProposalStatus) => void;
  addPermission: (
    sessionId: string,
    permission: Omit<AgentPermission, 'status' | 'chosenName'>,
  ) => void;
  resolvePermission: (sessionId: string, requestId: string, optionId: string | null) => void;
  /** Ends the session in the UI, discards its proposals and clears the focus. */
  endSession: (notice?: string) => void;
  /** Clears the session and conversation. Keeps the panel state and focus. */
  reset: () => void;
}

// Start tokens live outside the state. They only decide whether a finished
// start still has an owner, and nothing renders them. They stay at module level
// on purpose, because moving them into the state would re-render subscribers.
let startCounter = 0;
let currentStartToken = 0;

function isStreamingAgent(message: AssistantMessage): message is AgentMessage {
  return message.kind === 'agent' && message.streaming;
}

/** True while a turn runs. Its reply segments stream until the turn ends. */
export function selectTurnRunning(state: Pick<AssistantState, 'messages'>): boolean {
  return state.messages.some(isStreamingAgent);
}

// Events and results apply only to the session that is active right now.
function isCurrent(state: AssistantState, sessionId: string): boolean {
  return state.session?.status === 'active' && state.session.sessionId === sessionId;
}

// Stops every streaming reply segment. The error lands on the last one.
function settleStreaming(messages: AssistantMessage[], error?: string): AssistantMessage[] {
  let last = -1;
  for (let i = 0; i < messages.length; i += 1) {
    if (isStreamingAgent(messages[i])) last = i;
  }
  return messages.map((message, i) => {
    if (!isStreamingAgent(message)) return message;
    return i === last && error !== undefined
      ? { ...message, streaming: false, error }
      : { ...message, streaming: false };
  });
}

export const useAssistantStore = create<AssistantState>()((set, get) => ({
  session: undefined,
  messages: [],
  reasoning: '',
  proposals: [],
  proposalAnchors: {},
  permissions: [],
  usage: undefined,
  focus: undefined,
  panelOpen: false,
  conversationId: undefined,
  replay: undefined,

  openPanel: () => set({ panelOpen: true }),
  closePanel: () => set({ panelOpen: false }),
  setFocus: (focus) => set({ focus }),

  beginSession(agentConfigId, mode, replay) {
    startCounter += 1;
    currentStartToken = startCounter;
    set({
      session: { sessionId: '', agentConfigId, status: 'starting', configOptions: [], mode },
      messages: [],
      proposals: [],
      proposalAnchors: {},
      permissions: [],
      usage: undefined,
      conversationId: undefined,
      replay: replay ? { ...replay, sent: false } : undefined,
    });
    return currentStartToken;
  },

  activateSession(token, sessionId, configOptions) {
    const { session } = get();
    if (token !== currentStartToken || session?.status !== 'starting') return false;
    set({ session: { ...session, sessionId, configOptions, status: 'active' }, reasoning: '' });
    return true;
  },

  failStart(token, error) {
    const { session } = get();
    if (token !== currentStartToken || session?.status !== 'starting') return;
    set({ session: { ...session, status: 'error', error } });
  },

  appendUserMessage(text) {
    const state = get();
    if (state.session?.status !== 'active' || selectTurnRunning(state)) return false;
    const userId = crypto.randomUUID();
    const agentId = crypto.randomUUID();
    set({
      conversationId: state.conversationId ?? crypto.randomUUID(),
      reasoning: '',
      lastActivityAt: Date.now(),
      messages: [
        ...state.messages,
        { kind: 'user', id: userId, text },
        { kind: 'agent', id: agentId, text: '', streaming: true },
      ],
    });
    return true;
  },

  appendReasoning(sessionId, text) {
    set((state) => {
      if (!text || !isCurrent(state, sessionId)) return state;
      const next = state.reasoning + text;
      const kept =
        next.length > MAX_REASONING_CHARS
          ? `…${next.slice(next.length - MAX_REASONING_CHARS)}`
          : next;
      return { reasoning: kept, lastActivityAt: Date.now() };
    });
  },

  touchActivity(sessionId) {
    set((state) => (isCurrent(state, sessionId) ? { lastActivityAt: Date.now() } : state));
  },

  appendChunk(sessionId, text) {
    // Used only when a new segment starts. Unused ids are cheap to drop.
    const newSegmentId = crypto.randomUUID();
    set((state) => {
      if (!text || !isCurrent(state, sessionId)) return state;
      const last = state.messages[state.messages.length - 1];
      if (last && isStreamingAgent(last)) {
        return { messages: [...state.messages.slice(0, -1), { ...last, text: last.text + text }] };
      }
      // After a tool line, the reply goes on in a new segment of the same turn.
      if (!selectTurnRunning(state)) return state;
      return {
        messages: [...state.messages, { kind: 'agent', id: newSegmentId, text, streaming: true }],
      };
    });
  },

  completeMessage(sessionId) {
    set((state) =>
      isCurrent(state, sessionId) && selectTurnRunning(state)
        ? {
            messages: settleStreaming(state.messages),
            permissions: cancelPending(state.permissions),
          }
        : state,
    );
  },

  failMessage(sessionId, error, fatal = true) {
    set((state) => {
      if (!state.session || !isCurrent(state, sessionId)) return state;
      return {
        session: fatal ? { ...state.session, status: 'error', error } : state.session,
        messages: settleStreaming(state.messages, error),
        // A dead session cannot apply anything, so its proposals go too.
        proposals: fatal ? [] : state.proposals,
        proposalAnchors: fatal ? {} : state.proposalAnchors,
        permissions: fatal ? [] : cancelPending(state.permissions),
        // A fatal failure returns to the Start view, so the context goes too.
        ...(fatal ? { focus: undefined } : {}),
      };
    });
  },

  upsertToolActivity(sessionId, activity) {
    // Used only when a new tool line is added. Unused ids are cheap to drop.
    const newLineId = crypto.randomUUID();
    set((state) => {
      if (!isCurrent(state, sessionId)) return state;
      const index = state.messages.findIndex(
        (m) => m.kind === 'tool' && m.callId === activity.callId,
      );
      if (index !== -1) {
        const existing = state.messages[index];
        if (existing.kind !== 'tool') return state;
        const messages = state.messages.slice();
        messages[index] = {
          ...existing,
          title: activity.title || existing.title,
          status: activity.status,
        };
        return { messages };
      }
      // Updates that arrive between turns are dropped, like the backend does.
      if (!selectTurnRunning(state)) return state;
      return {
        messages: [...state.messages, { kind: 'tool', id: newLineId, ...activity }],
      };
    });
  },

  setConfigOptions(sessionId, options) {
    set((state) =>
      state.session && isCurrent(state, sessionId)
        ? { session: { ...state.session, configOptions: options } }
        : state,
    );
  },

  setMode(sessionId, mode) {
    set((state) =>
      state.session && isCurrent(state, sessionId)
        ? { session: { ...state.session, mode } }
        : state,
    );
  },

  markReplaySent(sessionId) {
    set((state) =>
      state.replay && isCurrent(state, sessionId)
        ? { replay: { ...state.replay, sent: true } }
        : state,
    );
  },

  setUsage(sessionId, usage) {
    set((state) => (isCurrent(state, sessionId) ? { usage } : state));
  },

  upsertProposal(proposal) {
    set((state) => {
      if (!isCurrent(state, proposal.sessionId)) return state;
      const index = state.proposals.findIndex((p) => p.id === proposal.id);
      if (index === -1) {
        const last = state.messages[state.messages.length - 1];
        return {
          proposals: [...state.proposals, proposal],
          proposalAnchors: { ...state.proposalAnchors, [proposal.id]: last ? last.id : null },
        };
      }
      // A late list result must not undo a status the resolved event set.
      if (proposal.status === 'pending' && state.proposals[index].status !== 'pending') {
        return state;
      }
      const proposals = state.proposals.slice();
      proposals[index] = proposal;
      return { proposals };
    });
  },

  resolveProposal(sessionId, proposalId, status) {
    set((state) => {
      if (!isCurrent(state, sessionId)) return state;
      if (!state.proposals.some((p) => p.id === proposalId)) return state;
      return {
        proposals: state.proposals.map((p) => (p.id === proposalId ? { ...p, status } : p)),
      };
    });
  },

  addPermission(sessionId, permission) {
    set((state) => {
      if (!isCurrent(state, sessionId)) return state;
      if (state.permissions.some((p) => p.requestId === permission.requestId)) return state;
      return { permissions: [...state.permissions, { ...permission, status: 'pending' }] };
    });
  },

  resolvePermission(sessionId, requestId, optionId) {
    set((state) => {
      if (!isCurrent(state, sessionId)) return state;
      return {
        permissions: state.permissions.map((p) => {
          if (p.requestId !== requestId || p.status !== 'pending') return p;
          if (optionId === null) return { ...p, status: 'cancelled' };
          const chosen = p.options.find((o) => o.id === optionId);
          return { ...p, status: 'answered', chosenName: chosen?.name ?? optionId };
        }),
      };
    });
  },

  endSession(notice) {
    const { session, messages } = get();
    if (!session || session.status === 'ended') return;
    currentStartToken = 0;
    const settled = settleStreaming(messages);
    set({
      session: { ...session, status: 'ended' },
      messages: notice
        ? [...settled, { kind: 'notice', id: crypto.randomUUID(), text: notice }]
        : settled,
      proposals: [],
      proposalAnchors: {},
      permissions: [],
      // A restarted agent must not inherit the stale context. The start itself
      // keeps a focus set just before it, so only the end clears it.
      focus: undefined,
    });
  },

  reset() {
    currentStartToken = 0;
    set({
      session: undefined,
      messages: [],
      proposals: [],
      proposalAnchors: {},
      permissions: [],
      usage: undefined,
      reasoning: '',
      lastActivityAt: undefined,
      conversationId: undefined,
      replay: undefined,
    });
  },
}));
