# AI Assist Chat Panel (Subproject C) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `AgentChatPanel` — a per-request-tab chat UI in the Scripts tab that lets a user converse with a configured ACP agent and insert code it suggests into the active script editor — consuming subproject B's existing Tauri IPC surface exactly as it stands today.

**Architecture:** Pure frontend work, no new Rust code. State lives on the request tab object in `pane-store.ts` (`agentSession?: AgentChatSession`), following the same pattern `FlowTab.runState`/`runId` already established. A new `tauri-api.ts` section wraps subproject B's 3 commands + 4 events. `AgentChatPanel` mounts as a flex sibling of `ScriptsTab`'s whole `<Tabs>` element so it survives phase switches, subscribing to session events the same way `FlowToolbar` subscribes to flow-run events (subscribe-on-mount, ref-captured latest-state read inside the handler, clean up on unmount).

**Tech Stack:** React, TypeScript, Zustand, `@tanstack/react-query`, `react-markdown` (already a dependency), Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-09-27-acp-chat-panel-design.md`. Also see `docs/superpowers/plans/acp-transport/00-plan-index.md` for subproject B's locked IPC surface this plan consumes unchanged.

## Global Constraints

- Frontend-only — no Rust changes. This plan is a pure consumer of subproject B's 3 Tauri commands (`start_agent_session`, `send_agent_prompt`, `end_agent_session`) and 4 events (`agent-session-started`, `agent-session-chunk`, `agent-session-finished`, `agent-session-failed`), all already implemented and merged.
- One agent chat session per open request tab, not per script phase and not app-wide. Each session is tied to exactly one `AgentConfig`, chosen at start.
- No persistence — `agentSession` is in-memory only on the tab object. Closing the tab or restarting the app loses the conversation; this is deliberate.
- Event payload fields stay snake_case (`session_id`, `stop_reason`, `error`, `text`) matching every other `DomainEvent`-derived event already in `tauri-api.ts`; Tauri command args stay camelCase, matching every existing command wrapper there.
- Closing a single tab with an active session (`agentSession.status === 'active'`) must fire `endAgentSession(sessionId)` best-effort (fire-and-forget) from the `pane-store` tab-removal path — subproject B only sweeps sessions on whole-app exit, not on a single tab closing while the app keeps running.
- Out of scope: tool-calling/MCP (subproject D), the autonomous safety valve (subproject E), any change to subprojects A/B's surface, and persisting chat history across tab close or app restart.

## Review Focus

- **Double-send while streaming:** clicking Send (or hitting its keyboard trigger) twice in quick succession while a message is still `streaming: true` must fire exactly one `sendAgentPrompt` call, not two — Task 5 tests this directly.
- **Closing a tab mid-handshake:** a tab closed while `agentSession.status === 'starting'` (before a real `sessionId` exists) must not call `endAgentSession` at all — Task 2 tests the `closeTab` guard checks `status === 'active'` specifically, not just presence of `agentSession`.
- **Phase switching mid-conversation:** switching between Pre Request/Post Response/Tests must not unmount, reset, or duplicate the chat panel or its session — Task 6 tests the panel survives a phase switch.
- **Stale/cross-session events:** a chunk/finished/failed event carrying a `session_id` that doesn't match the panel's current session (e.g. a lingering listener from a session that just ended) must be ignored, not misapplied to the current message — Task 5 tests this directly.
- **External collection path resolution:** an `external`-type `CollectionReference` must resolve to its own `path` field, not the workspace-relative `collections/<name>` concatenation used for embedded collections — Task 3 tests this directly.

---

## Task 1: `tauri-api.ts` wrapper layer for subproject B's IPC surface

**Files:**
- Modify: `src/lib/tauri-api.ts` (append after line 1863, end of file)
- Test: `src/lib/queries/__tests__/agent-session-api.test.ts` (new)

**Interfaces:**
- Produces: `startAgentSession(agentConfigId: string, cwd: string): Promise<string>`, `sendAgentPrompt(sessionId: string, prompt: string): Promise<string>`, `endAgentSession(sessionId: string): Promise<void>`, `onAgentSessionStarted/Chunk/Finished/Failed(handler): Promise<UnlistenFn>` — consumed by Task 2 (`closeTab`) and Task 5 (`AgentChatPanel`).

- [ ] **Step 1: Write the failing tests**

Create `src/lib/queries/__tests__/agent-session-api.test.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('ACP chat session tauri-api bindings', () => {
  it('startAgentSession invokes start_agent_session with camelCase args', async () => {
    vi.mocked(invoke).mockResolvedValue('session-1');
    const { startAgentSession } = await import('@/lib/tauri-api');
    const result = await startAgentSession('agent-1', '/collections/my-collection');
    expect(invoke).toHaveBeenCalledWith('start_agent_session', {
      agentConfigId: 'agent-1',
      cwd: '/collections/my-collection',
    });
    expect(result).toBe('session-1');
  });

  it('sendAgentPrompt invokes send_agent_prompt with camelCase args', async () => {
    vi.mocked(invoke).mockResolvedValue('end_turn');
    const { sendAgentPrompt } = await import('@/lib/tauri-api');
    const result = await sendAgentPrompt('session-1', 'hello');
    expect(invoke).toHaveBeenCalledWith('send_agent_prompt', {
      sessionId: 'session-1',
      prompt: 'hello',
    });
    expect(result).toBe('end_turn');
  });

  it('endAgentSession invokes end_agent_session with the session id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { endAgentSession } = await import('@/lib/tauri-api');
    await endAgentSession('session-1');
    expect(invoke).toHaveBeenCalledWith('end_agent_session', { sessionId: 'session-1' });
  });

  it('onAgentSessionStarted subscribes to agent-session-started and unwraps the payload', async () => {
    const payload = { type: 'acpSessionStarted', session_id: 'session-1' };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionStarted } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionStarted(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-started', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onAgentSessionChunk subscribes to agent-session-chunk and unwraps the payload', async () => {
    const payload = { type: 'acpSessionChunk', session_id: 'session-1', text: 'hello' };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionChunk } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionChunk(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-chunk', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onAgentSessionFinished subscribes to agent-session-finished and unwraps the payload', async () => {
    const payload = {
      type: 'acpSessionFinished',
      session_id: 'session-1',
      stop_reason: 'end_turn',
    };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionFinished } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionFinished(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-finished', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onAgentSessionFailed subscribes to agent-session-failed and unwraps the payload', async () => {
    const payload = { type: 'acpSessionFailed', session_id: 'session-1', error: 'boom' };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionFailed } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionFailed(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-failed', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn vitest run src/lib/queries/__tests__/agent-session-api.test.ts`
Expected: FAIL — `startAgentSession`/`sendAgentPrompt`/`endAgentSession`/`onAgentSession*` are not exported by `@/lib/tauri-api` yet (import errors / `undefined` is not a function).

- [ ] **Step 3: Implement the wrapper layer**

Append to the end of `src/lib/tauri-api.ts` (after the existing `onFlowRunFinished` export at line 1863):

```ts

// ==== AI Assist (ACP chat sessions) ====
export const startAgentSession = (agentConfigId: string, cwd: string) =>
  invoke<string>('start_agent_session', { agentConfigId, cwd });

export const sendAgentPrompt = (sessionId: string, prompt: string) =>
  invoke<string>('send_agent_prompt', { sessionId, prompt });

export const endAgentSession = (sessionId: string) =>
  invoke<void>('end_agent_session', { sessionId });

// Event payloads are DomainEvent JSON. Their fields are snake_case, like
// every other DomainEvent. Do not camelCase them here.
export interface AgentSessionStartedEvent {
  type: 'acpSessionStarted';
  session_id: string;
}

export const onAgentSessionStarted = (
  handler: (event: AgentSessionStartedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionStartedEvent>('agent-session-started', (e) => handler(e.payload));

export interface AgentSessionChunkEvent {
  type: 'acpSessionChunk';
  session_id: string;
  text: string;
}

export const onAgentSessionChunk = (
  handler: (event: AgentSessionChunkEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionChunkEvent>('agent-session-chunk', (e) => handler(e.payload));

export interface AgentSessionFinishedEvent {
  type: 'acpSessionFinished';
  session_id: string;
  stop_reason: string;
}

export const onAgentSessionFinished = (
  handler: (event: AgentSessionFinishedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionFinishedEvent>('agent-session-finished', (e) => handler(e.payload));

export interface AgentSessionFailedEvent {
  type: 'acpSessionFailed';
  session_id: string;
  error: string;
}

export const onAgentSessionFailed = (
  handler: (event: AgentSessionFailedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionFailedEvent>('agent-session-failed', (e) => handler(e.payload));
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn vitest run src/lib/queries/__tests__/agent-session-api.test.ts`
Expected: PASS — 7 tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/queries/__tests__/agent-session-api.test.ts
git commit -m "feat(chat-panel): add tauri-api wrappers for ACP chat sessions"
```

---

## Task 2: Chat session state on the request tab + `pane-store` actions

**Files:**
- Modify: `src/types/pane-types.ts` (add `ChatMessage`, `AgentChatSession` types; add `agentSession?` to `RequestTab`)
- Modify: `src/stores/pane-store.ts` (add 8 actions; modify `closeTab` to fire `endAgentSession` for an active session)
- Modify: `src/stores/__tests__/pane-store.test.ts` (add mock for `endAgentSession`; append new tests)

**Interfaces:**
- Consumes: `endAgentSession(sessionId: string): Promise<void>` (Task 1).
- Produces: `ChatMessage { id, role, text, streaming? }`, `AgentChatSession { agentConfigId, sessionId, status, messages, error? }`, `RequestTab.agentSession?: AgentChatSession`; store actions `beginAgentSession(tabId, agentConfigId)`, `activateAgentSession(tabId, sessionId)`, `appendAgentChatMessage(tabId, message)`, `appendAgentChatChunk(tabId, messageId, text)`, `completeAgentChatMessage(tabId, messageId)`, `failAgentChatMessage(tabId, messageId, error)`, `markAgentSessionEnded(tabId)`, `clearAgentSession(tabId)` — consumed by Task 5 (`AgentChatPanel`).

- [ ] **Step 1: Write the failing tests**

In `src/stores/__tests__/pane-store.test.ts`, change the existing tauri-api mock (near the top of the file) to also mock `endAgentSession`:

```ts
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});
```

Then append this to the end of the file:

```ts
describe('Agent chat session actions', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  function getRequestTab(): RequestTab {
    const leaf = getLeaf();
    const tab = leaf.tabs[0];
    if (!isRequestTab(tab)) throw new Error('Expected a request tab');
    return tab;
  }

  it('beginAgentSession sets status starting with an empty session id', () => {
    const leaf = setupWithTab();
    usePaneStore.getState().beginAgentSession(leaf.tabs[0].id, 'agent-1');
    const tab = getRequestTab();
    expect(tab.agentSession).toEqual({
      agentConfigId: 'agent-1',
      sessionId: '',
      status: 'starting',
      messages: [],
    });
  });

  it('activateAgentSession sets the real session id and status active', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    const tab = getRequestTab();
    expect(tab.agentSession?.sessionId).toBe('session-1');
    expect(tab.agentSession?.status).toBe('active');
  });

  it('appendAgentChatMessage appends to the messages list', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore
      .getState()
      .appendAgentChatMessage(tabId, { id: 'm1', role: 'user', text: 'hi' });
    const tab = getRequestTab();
    expect(tab.agentSession?.messages).toEqual([{ id: 'm1', role: 'user', text: 'hi' }]);
  });

  it('appendAgentChatChunk appends text onto the matching message only', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore
      .getState()
      .appendAgentChatMessage(tabId, { id: 'm1', role: 'user', text: 'hi' });
    usePaneStore
      .getState()
      .appendAgentChatMessage(tabId, { id: 'm2', role: 'agent', text: '', streaming: true });
    usePaneStore.getState().appendAgentChatChunk(tabId, 'm2', 'Hello');
    usePaneStore.getState().appendAgentChatChunk(tabId, 'm2', ' there');
    const tab = getRequestTab();
    expect(tab.agentSession?.messages).toEqual([
      { id: 'm1', role: 'user', text: 'hi' },
      { id: 'm2', role: 'agent', text: 'Hello there', streaming: true },
    ]);
  });

  it('completeAgentChatMessage marks the message not streaming', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore
      .getState()
      .appendAgentChatMessage(tabId, { id: 'm2', role: 'agent', text: 'done', streaming: true });
    usePaneStore.getState().completeAgentChatMessage(tabId, 'm2');
    const tab = getRequestTab();
    expect(tab.agentSession?.messages[0].streaming).toBe(false);
  });

  it('failAgentChatMessage marks status error, sets session error, and appends the error to the message', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore
      .getState()
      .appendAgentChatMessage(tabId, { id: 'm2', role: 'agent', text: 'partial', streaming: true });
    usePaneStore.getState().failAgentChatMessage(tabId, 'm2', 'agent crashed');
    const tab = getRequestTab();
    expect(tab.agentSession?.status).toBe('error');
    expect(tab.agentSession?.error).toBe('agent crashed');
    expect(tab.agentSession?.messages[0]).toEqual({
      id: 'm2',
      role: 'agent',
      text: 'partial\n\nError: agent crashed',
      streaming: false,
    });
  });

  it('markAgentSessionEnded sets status ended', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore.getState().markAgentSessionEnded(tabId);
    expect(getRequestTab().agentSession?.status).toBe('ended');
  });

  it('clearAgentSession removes the agent session entirely', () => {
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().clearAgentSession(tabId);
    expect(getRequestTab().agentSession).toBeUndefined();
  });
});

describe('closeTab — ends the agent session for an active chat', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  it('calls endAgentSession when the closed tab has an active session', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().activateAgentSession(tabId, 'session-1');
    usePaneStore.getState().closeTab(tabId, leaf.groupId);
    expect(endAgentSession).toHaveBeenCalledWith('session-1');
  });

  it('does not call endAgentSession when the session is still starting (no real session id yet)', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const leaf = setupWithTab();
    const tabId = leaf.tabs[0].id;
    usePaneStore.getState().beginAgentSession(tabId, 'agent-1');
    usePaneStore.getState().closeTab(tabId, leaf.groupId);
    expect(endAgentSession).not.toHaveBeenCalled();
  });

  it('does not call endAgentSession when there is no agent session at all', async () => {
    const { endAgentSession } = await import('@/lib/tauri-api');
    const leaf = setupWithTab();
    usePaneStore.getState().closeTab(leaf.tabs[0].id, leaf.groupId);
    expect(endAgentSession).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts`
Expected: FAIL — `beginAgentSession`/`activateAgentSession`/etc. are not functions on the store yet.

- [ ] **Step 3: Add the types**

In `src/types/pane-types.ts`, add after the `Environment` interface (before `AgentConfig`, near line 206):

```ts
export interface ChatMessage {
  id: string;
  role: 'user' | 'agent';
  text: string;
  streaming?: boolean;
}

export interface AgentChatSession {
  agentConfigId: string;
  sessionId: string;
  status: 'starting' | 'active' | 'ended' | 'error';
  messages: ChatMessage[];
  error?: string;
}
```

Then extend `RequestTab` (currently at line 28):

```ts
export interface RequestTab extends BaseTab {
  tabType: 'request' | 'history';
  request: RequestState;
  response: ResponseState | null;
  agentSession?: AgentChatSession;
}
```

- [ ] **Step 4: Add the store actions**

In `src/stores/pane-store.ts`, add `endAgentSession` to the existing `@/lib/tauri-api` import list at the top, and add `AgentChatSession`/`ChatMessage` to the `@/types/pane-types` type import list.

Add to the `PaneState` interface, near `updateRequest`:

```ts
  beginAgentSession: (tabId: string, agentConfigId: string) => void;
  activateAgentSession: (tabId: string, sessionId: string) => void;
  appendAgentChatMessage: (tabId: string, message: ChatMessage) => void;
  appendAgentChatChunk: (tabId: string, messageId: string, text: string) => void;
  completeAgentChatMessage: (tabId: string, messageId: string) => void;
  failAgentChatMessage: (tabId: string, messageId: string, error: string) => void;
  markAgentSessionEnded: (tabId: string) => void;
  clearAgentSession: (tabId: string) => void;
```

Add the implementations, next to `updateRequest`/`setResponse`:

```ts
  beginAgentSession(tabId, agentConfigId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRequestTab(tab)) return tab;
        return {
          ...tab,
          agentSession: { agentConfigId, sessionId: '', status: 'starting', messages: [] },
        };
      }),
    });
  },

  activateAgentSession(tabId, sessionId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return { ...tab, agentSession: { ...tab.agentSession, sessionId, status: 'active' } };
      }),
    });
  },

  appendAgentChatMessage(tabId, message) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return {
          ...tab,
          agentSession: {
            ...tab.agentSession,
            messages: [...tab.agentSession.messages, message],
          },
        };
      }),
    });
  },

  appendAgentChatChunk(tabId, messageId, text) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return {
          ...tab,
          agentSession: {
            ...tab.agentSession,
            messages: tab.agentSession.messages.map((m) =>
              m.id === messageId ? { ...m, text: m.text + text } : m,
            ),
          },
        };
      }),
    });
  },

  completeAgentChatMessage(tabId, messageId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return {
          ...tab,
          agentSession: {
            ...tab.agentSession,
            messages: tab.agentSession.messages.map((m) =>
              m.id === messageId ? { ...m, streaming: false } : m,
            ),
          },
        };
      }),
    });
  },

  failAgentChatMessage(tabId, messageId, error) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return {
          ...tab,
          agentSession: {
            ...tab.agentSession,
            status: 'error',
            error,
            messages: tab.agentSession.messages.map((m) =>
              m.id === messageId
                ? { ...m, text: `${m.text}\n\nError: ${error}`, streaming: false }
                : m,
            ),
          },
        };
      }),
    });
  },

  markAgentSessionEnded(tabId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRequestTab(tab) || !tab.agentSession) return tab;
        return { ...tab, agentSession: { ...tab.agentSession, status: 'ended' } };
      }),
    });
  },

  clearAgentSession(tabId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRequestTab(tab)) return tab;
        return { ...tab, agentSession: undefined };
      }),
    });
  },
```

- [ ] **Step 5: Wire the tab-close cleanup**

In `closeTab` (currently starting at line 209), add the agent-session check right after the existing dirty-tab autosave block:

```ts
  closeTab(tabId, groupId) {
    // Save the tab before closing if it's dirty.
    const { root } = get();
    const found = findTabInTree(root, tabId);
    if (found?.tab.isDirty && found.tab.source && isRequestTab(found.tab)) {
      scheduleAutoSave(
        tabId,
        found.tab.source.collection,
        found.tab.source.path,
        found.tab.title,
        found.tab.request,
      );
    }

    // Best-effort session cleanup — subproject B only sweeps sessions on
    // whole-app exit, not on a single tab closing while the app keeps
    // running. A session still mid-handshake (no real session id yet) has
    // nothing on the backend to end.
    if (found && isRequestTab(found.tab) && found.tab.agentSession?.status === 'active') {
      endAgentSession(found.tab.agentSession.sessionId).catch((err) => {
        console.error('[pane-store] closeTab: failed to end agent session', err);
      });
    }

    const leaf = (() => {
```

(The rest of `closeTab` is unchanged — this only inserts the new block between the existing autosave check and the existing `const leaf = (() => {` line.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts`
Expected: PASS — all existing tests plus the new ones (11 new tests across the two new `describe` blocks).

- [ ] **Step 7: Commit**

```bash
git add src/types/pane-types.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat(chat-panel): add agent chat session state and pane-store actions"
```

---

## Task 3: `resolveCollectionPath` helper + `useCollectionPath` hook

**Files:**
- Create: `src/lib/collection-path.ts`
- Test: `src/lib/__tests__/collection-path.test.ts` (new)

**Interfaces:**
- Consumes: `CollectionReference { name, type, path? }` (already defined in `@/lib/tauri-api`), `useWorkspaceStore((s) => s.activeWorkspaceId)`, `useWorkspaces()`, `useWorkspaceConfig(workspaceId)` (all existing).
- Produces: `resolveCollectionPath(collectionName: string, workspacePath: string, collections: CollectionReference[]): string`, `useCollectionPath(collectionName: string | undefined): string | undefined` — consumed by Task 5 (`AgentChatPanel`).

- [ ] **Step 1: Write the failing tests**

Create `src/lib/__tests__/collection-path.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { CollectionReference } from '@/lib/tauri-api';
import { resolveCollectionPath } from '../collection-path';

describe('resolveCollectionPath', () => {
  it('concatenates the workspace path for an embedded collection', () => {
    const collections: CollectionReference[] = [{ name: 'my-collection', type: 'embedded' }];
    const result = resolveCollectionPath('my-collection', '/ws/root', collections);
    expect(result).toBe('/ws/root/collections/my-collection');
  });

  it('uses the CollectionReference path directly for an external collection', () => {
    const collections: CollectionReference[] = [
      { name: 'ext-collection', type: 'external', path: '/somewhere/else' },
    ];
    const result = resolveCollectionPath('ext-collection', '/ws/root', collections);
    expect(result).toBe('/somewhere/else');
  });

  it('falls back to the workspace concatenation when no matching reference is found', () => {
    const result = resolveCollectionPath('unknown-collection', '/ws/root', []);
    expect(result).toBe('/ws/root/collections/unknown-collection');
  });

  it('falls back to the workspace concatenation when an external reference has no path', () => {
    const collections: CollectionReference[] = [{ name: 'ext-collection', type: 'external' }];
    const result = resolveCollectionPath('ext-collection', '/ws/root', collections);
    expect(result).toBe('/ws/root/collections/ext-collection');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/__tests__/collection-path.test.ts`
Expected: FAIL — `../collection-path` module does not exist.

- [ ] **Step 3: Implement the helper and hook**

Create `src/lib/collection-path.ts`:

```ts
import { useWorkspaceConfig, useWorkspaces } from '@/lib/queries/workspace-queries';
import type { CollectionReference } from '@/lib/tauri-api';
import { useWorkspaceStore } from '@/stores/workspace-store';

// Resolves a collection name to its filesystem path. Mirrors the embedded-
// collection concatenation already used by useKeyboardShortcuts.ts and
// CollectionNode.tsx (`${workspacePath}/collections/${name}`); an
// external-type collection's own `path` is used directly instead, since it
// isn't necessarily under the workspace's `collections/` folder.
export function resolveCollectionPath(
  collectionName: string,
  workspacePath: string,
  collections: CollectionReference[],
): string {
  const ref = collections.find((c) => c.name === collectionName);
  if (ref?.type === 'external' && ref.path) return ref.path;
  return `${workspacePath}/collections/${collectionName}`;
}

// Resolves the active workspace's collection list and the given collection
// name into a filesystem path, or undefined while either is still loading /
// unavailable (no active workspace, or the name doesn't matter yet).
export function useCollectionPath(collectionName: string | undefined): string | undefined {
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const { data: workspaces = [] } = useWorkspaces();
  const { data: workspaceConfig } = useWorkspaceConfig(activeWorkspaceId);
  const workspace = workspaces.find((w) => w.id === activeWorkspaceId);
  if (!collectionName || !workspace) return undefined;
  return resolveCollectionPath(collectionName, workspace.path, workspaceConfig?.collections ?? []);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/lib/__tests__/collection-path.test.ts`
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/collection-path.ts src/lib/__tests__/collection-path.test.ts
git commit -m "feat(chat-panel): add resolveCollectionPath helper for agent session cwd"
```

---

## Task 4: `MarkdownRenderer` per-code-block actions

**Files:**
- Modify: `src/components/collections/MarkdownRenderer.tsx`
- Test: `src/components/collections/__tests__/MarkdownRenderer.test.tsx` (new)

**Interfaces:**
- Produces: `MarkdownRendererProps.renderCodeActions?: (code: string, language?: string) => React.ReactNode` — an optional prop; when given, its return value is rendered over each fenced (language-tagged) code block. Consumed by Task 5 (`AgentChatPanel`, for the "Insert" button).

- [ ] **Step 1: Write the failing tests**

Create `src/components/collections/__tests__/MarkdownRenderer.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { MarkdownRenderer } from '../MarkdownRenderer';

describe('MarkdownRenderer — code block actions', () => {
  it('renders no action UI when renderCodeActions is not given', () => {
    render(<MarkdownRenderer>{'```js\nconst x = 1;\n```'}</MarkdownRenderer>);
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('renders the caller-provided action for each fenced code block', async () => {
    const onInsert = vi.fn();
    render(
      <MarkdownRenderer
        renderCodeActions={(code) => (
          <button type='button' onClick={() => onInsert(code)}>
            Insert
          </button>
        )}
      >
        {'```js\nconst x = 1;\n```'}
      </MarkdownRenderer>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Insert' }));
    expect(onInsert).toHaveBeenCalledWith('const x = 1;');
  });

  it('does not render an action for inline code (no language fence)', () => {
    render(
      <MarkdownRenderer renderCodeActions={() => <button type='button'>Insert</button>}>
        {'Use `const x = 1` inline.'}
      </MarkdownRenderer>,
    );
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn vitest run src/components/collections/__tests__/MarkdownRenderer.test.tsx`
Expected: FAIL — `renderCodeActions` prop is not read/rendered yet, so the "Insert" button never appears (test 2 fails on the `getByRole` lookup).

- [ ] **Step 3: Add the prop**

Replace `src/components/collections/MarkdownRenderer.tsx` in full:

```tsx
import { useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import oneDark from 'react-syntax-highlighter/dist/esm/styles/prism/one-dark';
import oneLight from 'react-syntax-highlighter/dist/esm/styles/prism/one-light';
import remarkGfm from 'remark-gfm';
import { cn } from '@/lib/utils';

interface MarkdownRendererProps {
  children: string;
  className?: string;
  // When given, rendered over each fenced (language-tagged) code block —
  // e.g. an "Insert" button in the AI Assist chat panel.
  renderCodeActions?: (code: string, language?: string) => React.ReactNode;
}

function useIsDark() {
  const [isDark, setIsDark] = useState(() => document.documentElement.classList.contains('dark'));
  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
    return () => observer.disconnect();
  }, []);
  return isDark;
}

export function MarkdownRenderer({ children, className, renderCodeActions }: MarkdownRendererProps) {
  const isDark = useIsDark();

  return (
    <div className={cn('prose-doc', className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          code({ className: cls, children: ch, ...rest }) {
            // language-* className signals a fenced code block in react-markdown v10.
            const match = /language-(\w+)/.exec(cls ?? '');
            const code = String(ch).replace(/\n$/, '');

            if (match) {
              return (
                <div className='relative'>
                  <SyntaxHighlighter
                    style={(isDark ? oneDark : oneLight) as Record<string, React.CSSProperties>}
                    language={match[1]}
                    PreTag='div'
                    customStyle={{
                      margin: '0 0 1rem',
                      borderRadius: '8px',
                      fontSize: '0.8125rem',
                      ...(isDark ? {} : { background: 'hsl(var(--muted))' }),
                    }}
                    codeTagProps={{ style: { fontFamily: 'var(--font-mono)' } }}
                  >
                    {code}
                  </SyntaxHighlighter>
                  {renderCodeActions && (
                    <div className='absolute right-2 top-2'>
                      {renderCodeActions(code, match[1])}
                    </div>
                  )}
                </div>
              );
            }

            return (
              <code className={cn(cls)} {...rest}>
                {ch}
              </code>
            );
          },
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn vitest run src/components/collections/__tests__/MarkdownRenderer.test.tsx`
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add src/components/collections/MarkdownRenderer.tsx src/components/collections/__tests__/MarkdownRenderer.test.tsx
git commit -m "feat(chat-panel): add per-code-block action slot to MarkdownRenderer"
```

---

## Task 5: `AgentChatPanel` component

**Files:**
- Create: `src/components/request/AgentChatPanel.tsx`
- Test: `src/components/request/__tests__/AgentChatPanel.test.tsx` (new)

**Interfaces:**
- Consumes: `AgentChatSession`/`ChatMessage` (Task 2 types), `beginAgentSession`/`activateAgentSession`/`appendAgentChatMessage`/`appendAgentChatChunk`/`completeAgentChatMessage`/`failAgentChatMessage`/`markAgentSessionEnded`/`clearAgentSession` (Task 2 store actions), `startAgentSession`/`sendAgentPrompt`/`endAgentSession`/`onAgentSessionChunk`/`onAgentSessionFinished`/`onAgentSessionFailed` (Task 1), `useCollectionPath` (Task 3), `MarkdownRenderer` with `renderCodeActions` (Task 4), `useAgentConfigs()` (existing, `src/lib/queries/agent-config-queries.ts`).
- Produces: `AgentChatPanel({ tabId, collectionName, agentSession, onInsertCode })` — consumed by Task 6 (`ScriptsTab`).

- [ ] **Step 1: Write the failing tests**

Create `src/components/request/__tests__/AgentChatPanel.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentChatSession } from '@/types/pane-types';
import { AgentChatPanel } from '../AgentChatPanel';

// Radix Select (used for the agent picker) calls pointer-capture and
// scrollIntoView APIs jsdom doesn't implement — polyfill them so
// userEvent can open the dropdown and pick an option.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {
    // No-op for test polyfill.
  };
}

const mockActions = vi.hoisted(() => ({
  beginAgentSession: vi.fn(),
  activateAgentSession: vi.fn(),
  appendAgentChatMessage: vi.fn(),
  appendAgentChatChunk: vi.fn(),
  completeAgentChatMessage: vi.fn(),
  failAgentChatMessage: vi.fn(),
  markAgentSessionEnded: vi.fn(),
  clearAgentSession: vi.fn(),
}));

vi.mock('@/stores/pane-store', () => ({
  usePaneStore: (selector: (s: typeof mockActions) => unknown) => selector(mockActions),
}));

vi.mock('@/lib/collection-path', () => ({
  useCollectionPath: () => '/ws/root/collections/my-collection',
}));

vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({
    data: [
      { id: 'agent-1', label: 'Claude', command: 'claude-acp', args: [] },
      { id: 'agent-2', label: 'Gemini', command: 'gemini-acp', args: [] },
    ],
  }),
}));

type ChunkHandler = (e: { session_id: string; text: string }) => void;
type FinishedHandler = (e: { session_id: string; stop_reason: string }) => void;
type FailedHandler = (e: { session_id: string; error: string }) => void;

let chunkHandler: ChunkHandler | undefined;
let finishedHandler: FinishedHandler | undefined;
let failedHandler: FailedHandler | undefined;

vi.mock('@/lib/tauri-api', () => ({
  startAgentSession: vi.fn(),
  sendAgentPrompt: vi.fn(),
  endAgentSession: vi.fn(),
  onAgentSessionChunk: vi.fn((h: ChunkHandler) => {
    chunkHandler = h;
    return Promise.resolve(() => undefined);
  }),
  onAgentSessionFinished: vi.fn((h: FinishedHandler) => {
    finishedHandler = h;
    return Promise.resolve(() => undefined);
  }),
  onAgentSessionFailed: vi.fn((h: FailedHandler) => {
    failedHandler = h;
    return Promise.resolve(() => undefined);
  }),
}));

import * as tauriApi from '@/lib/tauri-api';

function activeSession(overrides: Partial<AgentChatSession> = {}): AgentChatSession {
  return {
    agentConfigId: 'agent-1',
    sessionId: 'session-1',
    status: 'active',
    messages: [],
    ...overrides,
  };
}

describe('AgentChatPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    chunkHandler = undefined;
    finishedHandler = undefined;
    failedHandler = undefined;
  });

  it('shows the agent picker and a disabled Start button with no session', () => {
    render(<AgentChatPanel tabId='tab-1' collectionName='my-collection' onInsertCode={vi.fn()} />);
    expect(screen.getByText('Select an agent…')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start' })).toBeDisabled();
  });

  it('starting a session calls startAgentSession with the resolved cwd, then begins and activates it', async () => {
    vi.mocked(tauriApi.startAgentSession).mockResolvedValue('session-1');
    render(<AgentChatPanel tabId='tab-1' collectionName='my-collection' onInsertCode={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox'));
    await userEvent.click(await screen.findByText('Claude'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));

    expect(mockActions.beginAgentSession).toHaveBeenCalledWith('tab-1', 'agent-1');
    await waitFor(() =>
      expect(tauriApi.startAgentSession).toHaveBeenCalledWith(
        'agent-1',
        '/ws/root/collections/my-collection',
      ),
    );
    await waitFor(() =>
      expect(mockActions.activateAgentSession).toHaveBeenCalledWith('tab-1', 'session-1'),
    );
  });

  it('a start failure shows an inline error and clears the session', async () => {
    vi.mocked(tauriApi.startAgentSession).mockRejectedValue(new Error('no credential'));
    render(<AgentChatPanel tabId='tab-1' collectionName='my-collection' onInsertCode={vi.fn()} />);

    await userEvent.click(screen.getByRole('combobox'));
    await userEvent.click(await screen.findByText('Claude'));
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));

    await waitFor(() => expect(mockActions.clearAgentSession).toHaveBeenCalledWith('tab-1'));
    expect(screen.getByText(/no credential/)).toBeInTheDocument();
  });

  it('sending a message appends a user message and a streaming placeholder, then calls sendAgentPrompt', async () => {
    vi.mocked(tauriApi.sendAgentPrompt).mockResolvedValue('end_turn');
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession()}
        onInsertCode={vi.fn()}
      />,
    );

    await userEvent.type(screen.getByPlaceholderText('Ask the agent…'), 'hello');
    await userEvent.click(screen.getByRole('button', { name: /send/i }));

    expect(mockActions.appendAgentChatMessage).toHaveBeenNthCalledWith(1, 'tab-1', {
      id: expect.any(String),
      role: 'user',
      text: 'hello',
    });
    expect(mockActions.appendAgentChatMessage).toHaveBeenNthCalledWith(2, 'tab-1', {
      id: expect.any(String),
      role: 'agent',
      text: '',
      streaming: true,
    });
    await waitFor(() =>
      expect(tauriApi.sendAgentPrompt).toHaveBeenCalledWith('session-1', 'hello'),
    );
  });

  it('does not send a second prompt while a message is still streaming', async () => {
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [{ id: 'm1', role: 'agent', text: 'partial', streaming: true }],
        })}
        onInsertCode={vi.fn()}
      />,
    );
    await userEvent.type(screen.getByPlaceholderText('Ask the agent…'), 'hello');
    expect(screen.getByRole('button', { name: /send/i })).toBeDisabled();
  });

  it('a chunk event for the current session appends to the streaming message', async () => {
    const { rerender } = render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [{ id: 'm1', role: 'agent', text: '', streaming: true }],
        })}
        onInsertCode={vi.fn()}
      />,
    );
    await waitFor(() => expect(chunkHandler).toBeDefined());
    chunkHandler?.({ session_id: 'session-1', text: 'Hello' });
    expect(mockActions.appendAgentChatChunk).toHaveBeenCalledWith('tab-1', 'm1', 'Hello');
    rerender(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [{ id: 'm1', role: 'agent', text: '', streaming: true }],
        })}
        onInsertCode={vi.fn()}
      />,
    );
  });

  it('ignores a chunk event for a different session id', async () => {
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [{ id: 'm1', role: 'agent', text: '', streaming: true }],
        })}
        onInsertCode={vi.fn()}
      />,
    );
    await waitFor(() => expect(chunkHandler).toBeDefined());
    chunkHandler?.({ session_id: 'some-other-session', text: 'Hello' });
    expect(mockActions.appendAgentChatChunk).not.toHaveBeenCalled();
  });

  it('a finished event completes the streaming message', async () => {
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [{ id: 'm1', role: 'agent', text: 'done', streaming: true }],
        })}
        onInsertCode={vi.fn()}
      />,
    );
    await waitFor(() => expect(finishedHandler).toBeDefined());
    finishedHandler?.({ session_id: 'session-1', stop_reason: 'end_turn' });
    expect(mockActions.completeAgentChatMessage).toHaveBeenCalledWith('tab-1', 'm1');
  });

  it('a failed event fails the streaming message', async () => {
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [{ id: 'm1', role: 'agent', text: 'partial', streaming: true }],
        })}
        onInsertCode={vi.fn()}
      />,
    );
    await waitFor(() => expect(failedHandler).toBeDefined());
    failedHandler?.({ session_id: 'session-1', error: 'crashed' });
    expect(mockActions.failAgentChatMessage).toHaveBeenCalledWith('tab-1', 'm1', 'crashed');
  });

  it('clicking Insert on a rendered code block calls onInsertCode with the code', async () => {
    const onInsertCode = vi.fn();
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession({
          messages: [
            { id: 'm1', role: 'agent', text: '```js\nconst x = 1;\n```', streaming: false },
          ],
        })}
        onInsertCode={onInsertCode}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Insert' }));
    expect(onInsertCode).toHaveBeenCalledWith('const x = 1;');
  });

  it('End session calls endAgentSession then marks the session ended', async () => {
    vi.mocked(tauriApi.endAgentSession).mockResolvedValue(undefined);
    render(
      <AgentChatPanel
        tabId='tab-1'
        collectionName='my-collection'
        agentSession={activeSession()}
        onInsertCode={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'End session' }));
    await waitFor(() => expect(tauriApi.endAgentSession).toHaveBeenCalledWith('session-1'));
    expect(mockActions.markAgentSessionEnded).toHaveBeenCalledWith('tab-1');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn vitest run src/components/request/__tests__/AgentChatPanel.test.tsx`
Expected: FAIL — `../AgentChatPanel` module does not exist.

- [ ] **Step 3: Implement `AgentChatPanel`**

Create `src/components/request/AgentChatPanel.tsx`:

```tsx
import type { UnlistenFn } from '@tauri-apps/api/event';
import { Loader2, Send } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { useCollectionPath } from '@/lib/collection-path';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
import {
  endAgentSession,
  onAgentSessionChunk,
  onAgentSessionFailed,
  onAgentSessionFinished,
  sendAgentPrompt,
  startAgentSession,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { AgentChatSession } from '@/types/pane-types';

interface AgentChatPanelProps {
  tabId: string;
  collectionName?: string;
  agentSession?: AgentChatSession;
  onInsertCode: (code: string) => void;
}

export function AgentChatPanel({
  tabId,
  collectionName,
  agentSession,
  onInsertCode,
}: AgentChatPanelProps) {
  const { data: agentConfigs = [] } = useAgentConfigs();
  const cwd = useCollectionPath(collectionName);

  const beginAgentSession = usePaneStore((s) => s.beginAgentSession);
  const activateAgentSession = usePaneStore((s) => s.activateAgentSession);
  const appendAgentChatMessage = usePaneStore((s) => s.appendAgentChatMessage);
  const appendAgentChatChunk = usePaneStore((s) => s.appendAgentChatChunk);
  const completeAgentChatMessage = usePaneStore((s) => s.completeAgentChatMessage);
  const failAgentChatMessage = usePaneStore((s) => s.failAgentChatMessage);
  const markAgentSessionEnded = usePaneStore((s) => s.markAgentSessionEnded);
  const clearAgentSession = usePaneStore((s) => s.clearAgentSession);

  const [selectedAgentConfigId, setSelectedAgentConfigId] = useState('');
  const [promptText, setPromptText] = useState('');
  const [startError, setStartError] = useState<string | null>(null);

  // Kept in sync every render so the event handlers below (subscribed only
  // when the session id/status actually changes) always read the latest
  // message list without needing to resubscribe on every chunk.
  const agentSessionRef = useRef(agentSession);
  agentSessionRef.current = agentSession;

  const sessionId = agentSession?.status === 'active' ? agentSession.sessionId : undefined;

  useEffect(() => {
    if (!sessionId) return;
    let disposed = false;
    const unlistens: UnlistenFn[] = [];

    const findStreamingMessageId = () =>
      agentSessionRef.current?.messages.find((m) => m.streaming)?.id;

    Promise.all([
      onAgentSessionChunk((e) => {
        if (e.session_id !== sessionId) return;
        const messageId = findStreamingMessageId();
        if (messageId) appendAgentChatChunk(tabId, messageId, e.text);
      }),
      onAgentSessionFinished((e) => {
        if (e.session_id !== sessionId) return;
        const messageId = findStreamingMessageId();
        if (messageId) completeAgentChatMessage(tabId, messageId);
      }),
      onAgentSessionFailed((e) => {
        if (e.session_id !== sessionId) return;
        const messageId = findStreamingMessageId();
        if (messageId) failAgentChatMessage(tabId, messageId, e.error);
      }),
    ]).then((fns) => {
      if (disposed) {
        for (const fn of fns) fn();
      } else {
        unlistens.push(...fns);
      }
    });

    return () => {
      disposed = true;
      for (const fn of unlistens) fn();
    };
  }, [sessionId, tabId, appendAgentChatChunk, completeAgentChatMessage, failAgentChatMessage]);

  const handleStart = async () => {
    if (!selectedAgentConfigId || !cwd) return;
    setStartError(null);
    beginAgentSession(tabId, selectedAgentConfigId);
    try {
      const newSessionId = await startAgentSession(selectedAgentConfigId, cwd);
      activateAgentSession(tabId, newSessionId);
    } catch (err) {
      clearAgentSession(tabId);
      setStartError(String(err));
    }
  };

  const handleSend = async () => {
    if (!agentSession || agentSession.status !== 'active') return;
    const text = promptText.trim();
    if (!text) return;
    if (agentSession.messages.some((m) => m.streaming)) return;
    setPromptText('');
    appendAgentChatMessage(tabId, { id: crypto.randomUUID(), role: 'user', text });
    const agentMessageId = crypto.randomUUID();
    appendAgentChatMessage(tabId, {
      id: agentMessageId,
      role: 'agent',
      text: '',
      streaming: true,
    });
    try {
      await sendAgentPrompt(agentSession.sessionId, text);
    } catch (err) {
      failAgentChatMessage(tabId, agentMessageId, String(err));
    }
  };

  const handleEnd = async () => {
    if (!agentSession) return;
    try {
      await endAgentSession(agentSession.sessionId);
    } catch (err) {
      console.error('[AgentChatPanel] end_agent_session failed', err);
    } finally {
      markAgentSessionEnded(tabId);
    }
  };

  const isStreaming = agentSession?.messages.some((m) => m.streaming) ?? false;

  if (!agentSession || agentSession.status === 'ended' || agentSession.status === 'error') {
    return (
      <div className='flex w-72 shrink-0 flex-col gap-3 border-l p-3'>
        <div className='text-xs font-semibold text-muted-foreground uppercase tracking-wide'>
          AI Assist
        </div>
        {agentSession?.status === 'error' && agentSession.error && (
          <p className='text-xs text-destructive'>{agentSession.error}</p>
        )}
        <Select value={selectedAgentConfigId} onValueChange={setSelectedAgentConfigId}>
          <SelectTrigger className='h-8 text-sm'>
            <SelectValue placeholder='Select an agent…' />
          </SelectTrigger>
          <SelectContent>
            {agentConfigs.map((c) => (
              <SelectItem key={c.id} value={c.id}>
                {c.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {startError && <p className='text-xs text-destructive'>{startError}</p>}
        <Button
          size='sm'
          onClick={() => void handleStart()}
          disabled={!selectedAgentConfigId || !cwd}
        >
          Start
        </Button>
      </div>
    );
  }

  if (agentSession.status === 'starting') {
    return (
      <div className='flex w-72 shrink-0 items-center justify-center gap-2 border-l p-3 text-sm text-muted-foreground'>
        <Loader2 className='h-4 w-4 animate-spin' />
        Starting agent…
      </div>
    );
  }

  return (
    <div className='flex w-96 shrink-0 flex-col border-l' id='agent-chat-panel'>
      <div className='flex items-center justify-between border-b px-3 py-2'>
        <span className='text-xs font-semibold text-muted-foreground uppercase tracking-wide'>
          AI Assist
        </span>
        <Button variant='ghost' size='sm' onClick={() => void handleEnd()}>
          End session
        </Button>
      </div>
      <ScrollArea className='flex-1'>
        <div className='flex flex-col gap-3 p-3'>
          {agentSession.messages.map((m) => (
            <div key={m.id} className='text-sm'>
              <div className='mb-1 text-xs font-semibold text-muted-foreground'>
                {m.role === 'user' ? 'You' : 'Agent'}
              </div>
              <MarkdownRenderer
                renderCodeActions={(code) => (
                  <Button size='sm' variant='outline' onClick={() => onInsertCode(code)}>
                    Insert
                  </Button>
                )}
              >
                {m.text}
              </MarkdownRenderer>
              {m.streaming && <Loader2 className='h-3 w-3 animate-spin' />}
            </div>
          ))}
        </div>
      </ScrollArea>
      <div className='flex items-end gap-2 border-t p-2'>
        <Textarea
          value={promptText}
          onChange={(e) => setPromptText(e.target.value)}
          placeholder='Ask the agent…'
          className='min-h-8 flex-1 resize-none text-sm'
          disabled={isStreaming}
        />
        <Button
          size='sm'
          aria-label='Send'
          onClick={() => void handleSend()}
          disabled={isStreaming || !promptText.trim()}
        >
          <Send className='h-3.5 w-3.5' />
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn vitest run src/components/request/__tests__/AgentChatPanel.test.tsx`
Expected: PASS — 11 tests.

- [ ] **Step 5: Commit**

```bash
git add src/components/request/AgentChatPanel.tsx src/components/request/__tests__/AgentChatPanel.test.tsx
git commit -m "feat(chat-panel): add AgentChatPanel component"
```

---

## Task 6: Wire `AgentChatPanel` into `ScriptsTab` and `RequestPanel`

**Files:**
- Modify: `src/components/request/ScriptsTab.tsx` (full-file replacement — the changes touch imports, props, layout, and the toggle button)
- Modify: `src/components/request/RequestPanel.tsx:1059-1066` (thread `tabId`/`collectionName`/`agentSession` into `ScriptsTab`)
- Test: `src/components/request/__tests__/ScriptsTab.test.tsx` (new)

**Interfaces:**
- Consumes: `AgentChatPanel({ tabId, collectionName, agentSession, onInsertCode })` (Task 5), `RequestTab.agentSession` (Task 2).
- Produces: `ScriptsTabProps` gains `tabId: string`, `collectionName?: string`, `agentSession?: AgentChatSession`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/request/__tests__/ScriptsTab.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ScriptsTab } from '../ScriptsTab';

type EditorStub = {
  getModel: () => { getLineCount: () => number; getLineMaxColumn: () => number };
  getPosition: () => null;
  executeEdits: ReturnType<typeof vi.fn>;
  focus: ReturnType<typeof vi.fn>;
};

const editorStubs: Record<string, EditorStub> = {};

function makeEditorStub(): EditorStub {
  return {
    getModel: () => ({ getLineCount: () => 1, getLineMaxColumn: () => 1 }),
    getPosition: () => null,
    executeEdits: vi.fn(),
    focus: vi.fn(),
  };
}

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({
    phase,
    onEditorReady,
  }: {
    phase: string;
    onEditorReady: (editor: EditorStub) => void;
  }) => {
    const editor = makeEditorStub();
    editorStubs[phase] = editor;
    onEditorReady(editor);
    return <div data-testid={`monaco-${phase}`} />;
  },
}));

let latestOnInsertCode: ((code: string) => void) | undefined;
vi.mock('../AgentChatPanel', () => ({
  AgentChatPanel: ({
    tabId,
    collectionName,
    onInsertCode,
  }: {
    tabId: string;
    collectionName?: string;
    onInsertCode: (code: string) => void;
  }) => {
    latestOnInsertCode = onInsertCode;
    return (
      <div data-testid='agent-chat-panel' data-tab-id={tabId} data-collection={collectionName} />
    );
  },
}));

function renderScriptsTab() {
  return render(
    <ScriptsTab
      tabId='tab-1'
      collectionName='my-collection'
      preRequestScript=''
      postResponseScript=''
      testsScript=''
      onChangePreRequest={vi.fn()}
      onChangePostResponse={vi.fn()}
      onChangeTests={vi.fn()}
    />,
  );
}

describe('ScriptsTab — AI Assist panel wiring', () => {
  beforeEach(() => {
    latestOnInsertCode = undefined;
    for (const key of Object.keys(editorStubs)) delete editorStubs[key];
  });

  it('hides the AI Assist panel until its toggle is clicked', async () => {
    renderScriptsTab();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.queryByTestId('agent-chat-panel')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    expect(screen.getByTestId('agent-chat-panel')).toBeInTheDocument();
  });

  it('passes tabId and collectionName through to AgentChatPanel', async () => {
    renderScriptsTab();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    const panel = screen.getByTestId('agent-chat-panel');
    expect(panel.dataset.tabId).toBe('tab-1');
    expect(panel.dataset.collection).toBe('my-collection');
  });

  it('keeps the AI Assist panel mounted across phase switches', async () => {
    renderScriptsTab();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    expect(screen.getByTestId('agent-chat-panel')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: 'Post Response' }));
    await waitFor(() => expect(screen.getByTestId('monaco-post-response')).toBeInTheDocument());
    expect(screen.getByTestId('agent-chat-panel')).toBeInTheDocument();
  });

  it('inserts code from the agent into whichever phase editor is currently active', async () => {
    renderScriptsTab();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));

    latestOnInsertCode?.('pm.test("ok", () => {});');
    expect(editorStubs['pre-request'].executeEdits).toHaveBeenCalledWith('snippet-insert', [
      expect.objectContaining({ text: '\npm.test("ok", () => {});\n' }),
    ]);

    fireEvent.click(screen.getByRole('tab', { name: 'Post Response' }));
    await waitFor(() => expect(screen.getByTestId('monaco-post-response')).toBeInTheDocument());
    latestOnInsertCode?.('pm.test("second", () => {});');
    expect(editorStubs['post-response'].executeEdits).toHaveBeenCalledWith('snippet-insert', [
      expect.objectContaining({ text: '\npm.test("second", () => {});\n' }),
    ]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn vitest run src/components/request/__tests__/ScriptsTab.test.tsx`
Expected: FAIL — `ScriptsTab` doesn't accept `tabId`/`collectionName` props yet and renders no "AI Assist" button (TypeScript prop-type errors and a failed `getByRole('button', { name: 'AI Assist' })` lookup).

- [ ] **Step 3: Update `ScriptsTab`**

Replace `src/components/request/ScriptsTab.tsx` in full:

```tsx
import { MessageSquare, PanelRight } from 'lucide-react';
import type * as monacoNs from 'monaco-editor';
import { lazy, Suspense, useEffect, useRef, useState } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import {
  POST_RESPONSE_SNIPPETS,
  PRE_REQUEST_SNIPPETS,
  type ScriptPhase,
} from '@/components/editor/rok-types';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import type { AgentChatSession } from '@/types/pane-types';
import { AgentChatPanel } from './AgentChatPanel';
import { ScriptSnippetSidebar } from './ScriptSnippetSidebar';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

const MIN_SIDEBAR_WIDTH = 160;
const MIN_EDITOR_WIDTH = 320;

interface ScriptsTabProps {
  tabId: string;
  collectionName?: string;
  agentSession?: AgentChatSession;
  preRequestScript: string;
  postResponseScript: string;
  testsScript: string;
  onChangePreRequest: (value: string) => void;
  onChangePostResponse: (value: string) => void;
  onChangeTests: (value: string) => void;
}

// Inserts a snippet at the cursor (or appends at the end with no cursor).
// A no-op when `editor` is undefined — e.g. the target tab's Monaco instance
// hasn't finished mounting yet after a fast tab switch.
function insertSnippet(editor: monacoNs.editor.IStandaloneCodeEditor | undefined, code: string) {
  if (!editor) return;
  const model = editor.getModel();
  if (!model) return;
  const position = editor.getPosition();
  const range = position
    ? {
        startLineNumber: position.lineNumber,
        startColumn: position.column,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      }
    : (() => {
        const lastLine = model.getLineCount();
        const lastCol = model.getLineMaxColumn(lastLine);
        return {
          startLineNumber: lastLine,
          startColumn: lastCol,
          endLineNumber: lastLine,
          endColumn: lastCol,
        };
      })();
  editor.executeEdits('snippet-insert', [{ range, text: `\n${code}\n`, forceMoveMarkers: true }]);
  editor.focus();
}

export function ScriptsTab({
  tabId,
  collectionName,
  agentSession,
  preRequestScript,
  postResponseScript,
  testsScript,
  onChangePreRequest,
  onChangePostResponse,
  onChangeTests,
}: ScriptsTabProps) {
  // Keyed per phase (not a single shared ref) — each tab's Monaco instance is
  // unmounted when its TabsContent goes inactive, so a shared ref could point
  // at a disposed editor from a different tab right after switching.
  const editorRefs = useRef<Partial<Record<ScriptPhase, monacoNs.editor.IStandaloneCodeEditor>>>(
    {},
  );

  const [activeTab, setActiveTab] = useState<ScriptPhase>('pre-request');
  const [snippetSidebars, setSnippetSidebars] = useState<Record<ScriptPhase, boolean>>({
    'pre-request': false,
    'post-response': false,
    tests: false,
  });
  const [showAgentChat, setShowAgentChat] = useState(false);
  const scriptsContainerRef = useRef<HTMLDivElement>(null);
  const [scriptsContainerWidth, setScriptsContainerWidth] = useState(0);
  const sidebarMaxWidth = Math.max(
    0,
    Math.min(scriptsContainerWidth * 0.5, scriptsContainerWidth - MIN_EDITOR_WIDTH),
  );
  const canShowSidebar = sidebarMaxWidth >= MIN_SIDEBAR_WIDTH;
  const showSidebar = canShowSidebar && snippetSidebars[activeTab];

  useEffect(() => {
    const container = scriptsContainerRef.current;
    if (!container) return;

    const updateWidth = () => setScriptsContainerWidth(container.getBoundingClientRect().width);
    updateWidth();

    const observer =
      typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(updateWidth);
    observer?.observe(container);
    window.addEventListener('resize', updateWidth);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', updateWidth);
    };
  }, []);

  useEffect(() => {
    if (!canShowSidebar) {
      setSnippetSidebars((current) =>
        Object.values(current).some(Boolean)
          ? { 'pre-request': false, 'post-response': false, tests: false }
          : current,
      );
    }
  }, [canShowSidebar]);

  const toggleSidebar = () => {
    setSnippetSidebars((current) => ({ ...current, [activeTab]: !current[activeTab] }));
  };

  return (
    <div className='flex h-full min-h-0'>
      <Tabs
        ref={scriptsContainerRef}
        value={activeTab}
        onValueChange={(v) => setActiveTab(v as ScriptPhase)}
        className='flex h-full min-h-0 min-w-0 flex-1 flex-col'
      >
        <TabsList className='shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2'>
          <TabsTrigger value='pre-request' className='text-xs'>
            Pre Request
          </TabsTrigger>
          <TabsTrigger value='post-response' className='text-xs'>
            Post Response
          </TabsTrigger>
          <TabsTrigger value='tests' className='text-xs'>
            Tests
          </TabsTrigger>
          <Button
            variant='ghost'
            size='sm'
            className='ml-auto h-7 gap-1 text-xs'
            onClick={() => setShowAgentChat((v) => !v)}
            aria-pressed={showAgentChat}
            aria-controls='agent-chat-panel'
            title={showAgentChat ? 'Hide AI assist' : 'Show AI assist'}
          >
            <MessageSquare className='h-3.5 w-3.5' />
            AI Assist
          </Button>
          <Button
            variant='ghost'
            size='sm'
            className='h-7 gap-1 text-xs'
            onClick={toggleSidebar}
            disabled={!canShowSidebar}
            aria-pressed={showSidebar}
            aria-controls='script-snippet-sidebar'
            title={
              canShowSidebar
                ? showSidebar
                  ? 'Hide snippets'
                  : 'Show snippets'
                : 'Not enough space to show snippets'
            }
          >
            <PanelRight className='h-3.5 w-3.5' />
            {showSidebar ? 'Hide snippets' : 'Snippets'}
          </Button>
        </TabsList>

        <TabsContent value='pre-request' className='flex min-h-0 flex-1 m-0 overflow-hidden p-0'>
          <div className='min-h-0 min-w-0 flex-1'>
            <Suspense fallback={<EditorSkeleton />}>
              <MonacoWrapper
                language='javascript'
                value={preRequestScript}
                onChange={onChangePreRequest}
                height='100%'
                phase='pre-request'
                onEditorReady={(editor) => {
                  editorRefs.current['pre-request'] = editor;
                }}
              />
            </Suspense>
          </div>
          {showSidebar && (
            <ScriptSnippetSidebar
              maxWidth={sidebarMaxWidth}
              snippets={PRE_REQUEST_SNIPPETS}
              onInsert={(code) => insertSnippet(editorRefs.current['pre-request'], code)}
            />
          )}
        </TabsContent>

        <TabsContent value='post-response' className='flex min-h-0 flex-1 m-0 overflow-hidden p-0'>
          <div className='min-h-0 min-w-0 flex-1'>
            <Suspense fallback={<EditorSkeleton />}>
              <MonacoWrapper
                language='javascript'
                value={postResponseScript}
                onChange={onChangePostResponse}
                height='100%'
                phase='post-response'
                onEditorReady={(editor) => {
                  editorRefs.current['post-response'] = editor;
                }}
              />
            </Suspense>
          </div>
          {showSidebar && (
            <ScriptSnippetSidebar
              maxWidth={sidebarMaxWidth}
              snippets={POST_RESPONSE_SNIPPETS}
              onInsert={(code) => insertSnippet(editorRefs.current['post-response'], code)}
            />
          )}
        </TabsContent>

        <TabsContent value='tests' className='flex min-h-0 flex-1 m-0 overflow-hidden p-0'>
          <div className='min-h-0 min-w-0 flex-1'>
            <Suspense fallback={<EditorSkeleton />}>
              <MonacoWrapper
                language='javascript'
                value={testsScript}
                onChange={onChangeTests}
                height='100%'
                phase='tests'
                onEditorReady={(editor) => {
                  editorRefs.current.tests = editor;
                }}
              />
            </Suspense>
          </div>
          {showSidebar && (
            <ScriptSnippetSidebar
              maxWidth={sidebarMaxWidth}
              onInsert={(code) => insertSnippet(editorRefs.current.tests, code)}
            />
          )}
        </TabsContent>
      </Tabs>
      {showAgentChat && (
        <AgentChatPanel
          tabId={tabId}
          collectionName={collectionName}
          agentSession={agentSession}
          onInsertCode={(code) => insertSnippet(editorRefs.current[activeTab], code)}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 4: Thread the new props from `RequestPanel`**

In `src/components/request/RequestPanel.tsx`, replace the `ScriptsTab` call (currently lines 1059-1066):

```tsx
            <ScriptsTab
              tabId={tab.id}
              collectionName={tab.source?.collection}
              agentSession={tab.agentSession}
              preRequestScript={request.preRequestScript ?? ''}
              postResponseScript={request.postResponseScript ?? ''}
              testsScript={request.testsScript ?? ''}
              onChangePreRequest={(v) => updateRequest(tab.id, { preRequestScript: v })}
              onChangePostResponse={(v) => updateRequest(tab.id, { postResponseScript: v })}
              onChangeTests={(v) => updateRequest(tab.id, { testsScript: v })}
            />
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `yarn vitest run src/components/request/__tests__/ScriptsTab.test.tsx`
Expected: PASS — 4 tests.

- [ ] **Step 6: Full verification**

Run: `yarn tsc --noEmit && yarn check && yarn vitest run`
Expected: no TypeScript errors, Biome clean, full frontend suite green (including every test from Tasks 1-6).

- [ ] **Step 7: Commit**

```bash
git add src/components/request/ScriptsTab.tsx src/components/request/RequestPanel.tsx src/components/request/__tests__/ScriptsTab.test.tsx
git commit -m "feat(chat-panel): wire AgentChatPanel into ScriptsTab"
```

---

## Post-Implementation Review

Once all 6 tasks are complete, dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, most capable available model) with this brief:

> Review every file this plan created or modified:
> `src/lib/tauri-api.ts`, `src/lib/queries/__tests__/agent-session-api.test.ts`,
> `src/types/pane-types.ts`, `src/stores/pane-store.ts`,
> `src/stores/__tests__/pane-store.test.ts`, `src/lib/collection-path.ts`,
> `src/lib/__tests__/collection-path.test.ts`,
> `src/components/collections/MarkdownRenderer.tsx`,
> `src/components/collections/__tests__/MarkdownRenderer.test.tsx`,
> `src/components/request/AgentChatPanel.tsx`,
> `src/components/request/__tests__/AgentChatPanel.test.tsx`,
> `src/components/request/ScriptsTab.tsx`, `src/components/request/RequestPanel.tsx`,
> `src/components/request/__tests__/ScriptsTab.test.tsx`.
>
> Check for:
> 1. Fidelity to `docs/superpowers/specs/2026-09-27-acp-chat-panel-design.md` — session
>    scope (one per request tab, tied to one `AgentConfig`), layout (chat panel survives
>    phase switches, only the whole-panel toggle unmounts it), the five Review Focus
>    items in this plan's own header, and the explicit out-of-scope boundaries
>    (no tool-calling/MCP, no safety valve, no persistence).
> 2. Code quality — Zustand selector discipline (no full-state destructuring at
>    component top level, per `.claude/rules/frontend-component-guardrails.md`),
>    shadcn/lucide-only UI primitives, no raw `<button>`/`<input>`/`<select>`.
> 3. Correctness of the event-subscription lifecycle in `AgentChatPanel` — that a
>    chunk/finished/failed event for a stale or foreign `session_id` is provably
>    ignored, and that the effect only resubscribes on session id/status change,
>    not on every message update.
>
> You have explicit authority to apply fixes directly for anything you find. After
> fixing, re-run `yarn tsc --noEmit && yarn check && yarn vitest run` and confirm
> they still pass. Report what you found and fixed.

Only consider subproject C done once this review comes back clean (or its fixes are applied and re-verified).
