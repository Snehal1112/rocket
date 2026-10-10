import { create } from 'zustand';
import type {
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

export interface ToolActivity {
  callId: string;
  title: string;
  status: ToolActivityStatus;
}

export interface AssistantState {
  session?: AssistantSession;
  messages: AssistantMessage[];
  proposals: AgentProposal[];
  /** Where each proposal sits in the chat: the id of the last message when it
   *  arrived, or null for the start. Kept apart from the wire DTO. */
  proposalAnchors: Record<string, string | null>;
  usage?: AssistantUsage;
  focus?: AssistantFocus;
  panelOpen: boolean;

  openPanel: () => void;
  closePanel: () => void;
  setFocus: (focus: AssistantFocus | undefined) => void;
  /** Puts a new session in 'starting' and returns the token of this start. */
  beginSession: (agentConfigId: string, mode: AssistantMode) => number;
  /** Activates the start with this token. Returns false when that start was
   *  abandoned, so the caller must end the backend session itself. */
  activateSession: (token: number, sessionId: string, configOptions: ConfigOption[]) => boolean;
  failStart: (token: number, error: string) => void;
  /** Adds the user's text and opens the streaming reply. Returns false when
   *  there is no active session or a turn is already running. */
  appendUserMessage: (text: string) => boolean;
  appendChunk: (sessionId: string, text: string) => void;
  completeMessage: (sessionId: string) => void;
  /** A fatal failure ends the session. A non-fatal one only fails the turn. */
  failMessage: (sessionId: string, error: string, fatal?: boolean) => void;
  upsertToolActivity: (sessionId: string, activity: ToolActivity) => void;
  setConfigOptions: (sessionId: string, options: ConfigOption[]) => void;
  /** Records the mode the backend switched the current session to. */
  setMode: (sessionId: string, mode: AssistantMode) => void;
  setUsage: (sessionId: string, usage: AssistantUsage) => void;
  upsertProposal: (proposal: AgentProposal) => void;
  resolveProposal: (sessionId: string, proposalId: string, status: ProposalStatus) => void;
  /** Ends the session in the UI, discards its proposals and clears the focus. */
  endSession: (notice?: string) => void;
  /** Clears the session and conversation. Keeps the panel state and focus. */
  reset: () => void;
}

// Start tokens live outside the state. They only decide whether a finished
// start still has an owner, and nothing renders them.
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
  proposals: [],
  proposalAnchors: {},
  usage: undefined,
  focus: undefined,
  panelOpen: false,

  openPanel: () => set({ panelOpen: true }),
  closePanel: () => set({ panelOpen: false }),
  setFocus: (focus) => set({ focus }),

  beginSession(agentConfigId, mode) {
    startCounter += 1;
    currentStartToken = startCounter;
    set({
      session: { sessionId: '', agentConfigId, status: 'starting', configOptions: [], mode },
      messages: [],
      proposals: [],
      proposalAnchors: {},
      usage: undefined,
    });
    return currentStartToken;
  },

  activateSession(token, sessionId, configOptions) {
    const { session } = get();
    if (token !== currentStartToken || session?.status !== 'starting') return false;
    set({ session: { ...session, sessionId, configOptions, status: 'active' } });
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
    set({
      messages: [
        ...state.messages,
        { kind: 'user', id: crypto.randomUUID(), text },
        { kind: 'agent', id: crypto.randomUUID(), text: '', streaming: true },
      ],
    });
    return true;
  },

  appendChunk(sessionId, text) {
    set((state) => {
      if (!text || !isCurrent(state, sessionId)) return state;
      const last = state.messages[state.messages.length - 1];
      if (last && isStreamingAgent(last)) {
        return { messages: [...state.messages.slice(0, -1), { ...last, text: last.text + text }] };
      }
      // After a tool line, the reply goes on in a new segment of the same turn.
      if (!selectTurnRunning(state)) return state;
      return {
        messages: [
          ...state.messages,
          { kind: 'agent', id: crypto.randomUUID(), text, streaming: true },
        ],
      };
    });
  },

  completeMessage(sessionId) {
    set((state) =>
      isCurrent(state, sessionId) && selectTurnRunning(state)
        ? { messages: settleStreaming(state.messages) }
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
        // A fatal failure returns to the Start view, so the context goes too.
        ...(fatal ? { focus: undefined } : {}),
      };
    });
  },

  upsertToolActivity(sessionId, activity) {
    set((state) => {
      if (!isCurrent(state, sessionId)) return state;
      const index = state.messages.findIndex(
        (m) => m.kind === 'tool' && m.callId === activity.callId,
      );
      if (index !== -1) {
        const existing = state.messages[index] as ToolActivityMessage;
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
        messages: [...state.messages, { kind: 'tool', id: crypto.randomUUID(), ...activity }],
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
      usage: undefined,
    });
  },
}));
