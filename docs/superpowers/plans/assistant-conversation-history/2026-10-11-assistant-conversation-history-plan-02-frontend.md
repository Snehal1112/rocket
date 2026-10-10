# Assistant Conversation History, Plan 02: Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Save the assistant conversation automatically (after each turn, at session end, at panel close) and add a History view in the panel that lists, opens read-only, deletes and clears stored conversations of the active workspace.

**Architecture:** `src/lib/tauri-api.ts` gets typed wrappers for Plan 01's five commands and three channels. A pure snapshot module turns the store's messages and proposals into `StoredMessage[]`; a saver subscribed to `assistant-store` (started by the app-lifetime assistant event bridge) debounces and serializes `saveConversation(sessionId, input)` calls, so the backend's session pin picks the workspace. `AssistantHistoryView` and `AssistantConversationView` render the list and a read-only conversation with the existing `ChatItem`, never with proposal actions.

**Tech Stack:** React 19 + TypeScript, Zustand, shadcn/ui (`Button`, `ScrollArea`, `AlertDialog`), lucide-react, sonner, Vitest + Testing Library.

**Spec:** [`docs/superpowers/specs/2026-10-11-assistant-conversation-history-design.md`](../../specs/2026-10-11-assistant-conversation-history-design.md), sections 7 (stage 1), 8 and 10. Plan index: [`00-index.md`](00-index.md). Depends on Plan 01.

## Verified facts

Line numbers are from `worktree-acp-mcp-tool-server` at `e9f01191`.

- The store: `src/stores/assistant-store.ts`. `AssistantState` `:72-110`; `beginSession` `:159-170` clears messages and proposals; `appendUserMessage` `:185-198`; `endSession` `:318-334` and a fatal `failMessage` `:225-238` clear `proposals` and `proposalAnchors` **in the same update** that ends the session, so a session-end save must take proposals from the previous state; `reset` `:336-345`; `selectTurnRunning` `:123-125`.
- The app-lifetime bridge is `src/lib/assistant-event-bridge.ts` (not under `src/lib/assistant/`). Its effect `:58-159` registers listeners, and ends the session on a workspace change (`:133-137`) **after** the backend switched, which is why Plan 01 pins the workspace per session. Its test `src/lib/__tests__/assistant-event-bridge.test.ts` mocks a fixed list of `tauri-api` functions (`:17-49`).
- `buildTimeline(messages, resolved, anchors)` is exported from `src/components/assistant/AssistantChatView.tsx:88-115` and used only there. `ChatItem` (`:43-82`) is not exported.
- Panel: `src/components/assistant/AssistantPanel.tsx`, header `:83-106`, body `:107-122`. Panel test: `src/components/assistant/__tests__/AssistantPanel.test.tsx` (partial `tauri-api` mock `:21-30`).
- Wrapper conventions: `invoke` and `listen` (`src/lib/tauri-api.ts:6-7`); proposal wrappers and DomainEvent payload types `:2858-2955` (DTO fields camelCase, event fields snake_case); `AgentToolCallStatus` `:2957`; `AssistantMode` `:2723`. Wrapper test precedent: `src/lib/__tests__/tauri-api.agent-proposals.test.ts`.
- Helpers: `formatRelativeTime(iso)` (`src/lib/relative-time.ts:3`), `useAgentConfigs()` (`src/lib/queries/agent-config-queries.ts:14-19`), `makeProposal` (`src/test/assistant-fixtures.ts`), `createDeferred` (`src/test/deferred.ts`), `useWorkspaceStore((s) => s.activeWorkspaceId)` (`src/stores/workspace-store.ts:4`). shadcn `AlertDialog*` exports: `src/components/ui/alert-dialog.tsx:117-128`.

## Global Constraints

- OC-1: Location is `<workspace>/assistant/conversations/<id>.yml` only, never inside `<workspace>/collections/`, a collection, a folder or a linked collection's directory.
- OC-2: No transcript data and no new keys go into `opencollection.yml`, `folder.yml`, request files, environment files or `workspace.yml`; no `extensions.*` keys.
- OC-3: Files are `.yml`, named `<id>.yml` with a generated lowercase UUID (`crypto.randomUUID()`), never a reserved name; writes use `atomic_write`.
- OC-4: `<workspace>/assistant/.gitignore` holds `*`, is created with `create_new` before the first write, never overwritten, and a failure is only logged.
- OC-5: The path is resolved on every call from the active workspace (or the session's pinned one), directories are created lazily, and the folder is outside the collections watcher.
- OC-6: A collection is referenced only by its `opencollection.yml` uid (plus name) as metadata, never by a path.
- OC-7: Reads tolerate missing and unknown keys and skip an unparseable or other-version file with a warning.
- Persistence keys are snake_case; camelCase only on the IPC DTOs. The TypeScript types mirror the DTOs (camelCase) and the DomainEvent payloads (snake_case).
- Caps: 50 conversations, 500 messages, 16 KB per message (enforced by the backend).
- A continued conversation always starts in Ask mode (Plan 03).
- Events carry ids only; the History view reloads the list instead of reading data from them.
- No vault scrubbing in v1; the History notice says so.
- Always pass `-j4` to `cargo`; never `cargo test --workspace`.
- No `unwrap()` in Rust production code (none is written here).
- shadcn/ui primitives and lucide-react icons only: no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`, no inline SVG.
- Narrow Zustand selectors; never destructure the whole store in a component.
- Harness: no bare `git stash`/`git stash pop`; no `git add -A`, `--all` or `.`; staged `.ts/.tsx` run `yarn tsc --noEmit` and `yarn check` before commit.
- Commits: every commit goes through the `dev-workflow-skills:1-git-commit` skill, conventional format, explicit `git add <paths>` and a pathspec commit, never `git add -A`.
- Comments are short full sentences ending with a period.

## Review Focus

1. **Session end drops the proposals in the same update.** The final save must include the proposals `endSession` or a fatal `failMessage` removed. Test: `saves the proposals that ending the session drops` (Task 2).
2. **A reply still streaming.** No save ever carries a streaming segment, and a chat whose only reply streams is not saved. Tests: `never saves a reply that is still streaming or a chat without a reply`, `saves at once when the panel closes, without the streaming segment` (Task 2).
3. **Two saves in flight.** A debounced save and the session-end save must reach the backend in order, so an older snapshot never lands last. Test: `saves one after another, so an older snapshot never lands last` (Task 2).
4. **Save failures.** A failing save shows one quiet notice per conversation and never touches the chat or the session. Test: `shows one quiet notice per conversation when saves fail` (Task 2).
5. **Read-only rendering and workspace switch.** A stored proposal shows no Accept or Reject, and a workspace switch closes the open conversation and reloads the list. Tests: `opens a conversation read-only, with proposals as plain lines`, `a workspace switch closes the open conversation and reloads` (Task 3).

---

### Task 1: API wrappers, types, timeline module and snapshot

**Files:**
- Modify: `src/lib/tauri-api.ts` (new section after `onAgentProposalResolved`, `:2940-2943`)
- Create: `src/lib/assistant/timeline.ts`
- Modify: `src/components/assistant/AssistantChatView.tsx` (remove `TimelineEntry` and `buildTimeline` `:84-115`, import them)
- Create: `src/lib/assistant/conversation-snapshot.ts`
- Test: `src/lib/__tests__/tauri-api.assistant-conversations.test.ts`, `src/lib/assistant/__tests__/conversation-snapshot.test.ts`

**Interfaces:**
- Consumes: Plan 01 commands `list_conversations`, `get_conversation`, `save_conversation(session_id, conversation)`, `delete_conversation(id)`, `clear_conversations`; channels `assistant-conversation-saved`, `assistant-conversation-deleted`, `assistant-conversations-cleared`.
- Produces (exact):
  ```ts
  // src/lib/tauri-api.ts
  export type StoredMessage =
    | { kind: 'user'; text: string }
    | { kind: 'agent'; text: string }
    | { kind: 'tool'; title: string; status: AgentToolCallStatus }
    | { kind: 'notice'; text: string }
    | { kind: 'proposal'; summary: string; status: AgentProposalStatus };
  export interface ConversationSummary { id: string; title: string; agentConfigId: string; mode: AssistantMode;
    createdAt: string; updatedAt: string; messageCount: number; continuedFrom?: string }
  export interface ConversationCollectionRef { uid: string; name: string }
  export interface ConversationRecord { id: string; title: string; agentConfigId: string; mode: AssistantMode;
    workspaceId: string; createdAt: string; updatedAt: string; collections: ConversationCollectionRef[];
    messages: StoredMessage[]; continuedFrom?: string }
  export interface ConversationInput { id: string; agentConfigId: string; mode: AssistantMode; workspaceId: string;
    collections: string[]; messages: StoredMessage[]; continuedFrom?: string }
  export const listConversations: () => Promise<ConversationSummary[]>;
  export const getConversation: (id: string) => Promise<ConversationRecord>;
  export const saveConversation: (sessionId: string, conversation: ConversationInput) => Promise<ConversationSummary>;
  export const deleteConversation: (id: string) => Promise<void>;
  export const clearConversations: () => Promise<void>;
  export interface AssistantConversationSavedEvent { type: 'assistantConversationSaved'; conversation_id: string }
  export interface AssistantConversationDeletedEvent { type: 'assistantConversationDeleted'; conversation_id: string }
  export interface AssistantConversationsClearedEvent { type: 'assistantConversationsCleared' }
  export const onAssistantConversationSaved: (h: (e: AssistantConversationSavedEvent) => void) => Promise<UnlistenFn>;
  export const onAssistantConversationDeleted: (h: (e: AssistantConversationDeletedEvent) => void) => Promise<UnlistenFn>;
  export const onAssistantConversationsCleared: (h: (e: AssistantConversationsClearedEvent) => void) => Promise<UnlistenFn>;

  // src/lib/assistant/timeline.ts (moved from AssistantChatView)
  export type TimelineEntry = { type: 'message'; message: AssistantMessage } | { type: 'proposal'; proposal: AgentProposal };
  export function buildTimeline(messages: AssistantMessage[], resolved: AgentProposal[],
    anchors: Record<string, string | null>): TimelineEntry[];

  // src/lib/assistant/conversation-snapshot.ts
  export interface ConversationSnapshot { conversationId: string; agentConfigId: string; mode: AssistantMode;
    workspaceId: string; collections: string[]; messages: AssistantMessage[]; proposals: AgentProposal[];
    proposalAnchors: Record<string, string | null>; continuedFrom?: string }
  export function toStoredMessages(messages: AssistantMessage[], proposals: AgentProposal[],
    anchors: Record<string, string | null>): StoredMessage[];
  export function hasAgentReply(messages: StoredMessage[]): boolean;
  export function buildConversationInput(snapshot: ConversationSnapshot): ConversationInput | null;
  ```

- [ ] **Step 1: Write the failing wrapper and snapshot tests**

Create `src/lib/__tests__/tauri-api.assistant-conversations.test.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  type ConversationInput,
  clearConversations,
  deleteConversation,
  getConversation,
  listConversations,
  onAssistantConversationDeleted,
  onAssistantConversationSaved,
  onAssistantConversationsCleared,
  saveConversation,
} from '../tauri-api';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

const input: ConversationInput = {
  id: '0b6f3f4e-0000-4000-8000-000000000001',
  agentConfigId: 'agent-1',
  mode: 'ask',
  workspaceId: 'ws-1',
  collections: ['orders'],
  messages: [
    { kind: 'user', text: 'Why 401?' },
    { kind: 'agent', text: 'The token expired.' },
  ],
};

describe('assistant conversation commands', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it('calls the five commands with camelCase arguments', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await listConversations();
    expect(invoke).toHaveBeenLastCalledWith('list_conversations');
    await getConversation('c1');
    expect(invoke).toHaveBeenLastCalledWith('get_conversation', { id: 'c1' });
    await saveConversation('s1', input);
    expect(invoke).toHaveBeenLastCalledWith('save_conversation', {
      sessionId: 's1',
      conversation: input,
    });
    await deleteConversation('c1');
    expect(invoke).toHaveBeenLastCalledWith('delete_conversation', { id: 'c1' });
    await clearConversations();
    expect(invoke).toHaveBeenLastCalledWith('clear_conversations');
  });

  it('listens on the three channels and passes the payload through', async () => {
    vi.mocked(listen).mockResolvedValue(vi.fn());
    const handler = vi.fn();
    await onAssistantConversationSaved(handler);
    await onAssistantConversationDeleted(handler);
    await onAssistantConversationsCleared(handler);
    expect(vi.mocked(listen).mock.calls.map((call) => call[0])).toEqual([
      'assistant-conversation-saved',
      'assistant-conversation-deleted',
      'assistant-conversations-cleared',
    ]);
    const deliver = vi.mocked(listen).mock.calls[0][1];
    deliver({ payload: { type: 'assistantConversationSaved', conversation_id: 'c1' } } as never);
    expect(handler).toHaveBeenCalledWith({
      type: 'assistantConversationSaved',
      conversation_id: 'c1',
    });
  });
});
```

Create `src/lib/assistant/__tests__/conversation-snapshot.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { AssistantMessage } from '@/stores/assistant-store';
import { makeProposal } from '@/test/assistant-fixtures';
import { buildConversationInput, toStoredMessages } from '../conversation-snapshot';

const messages: AssistantMessage[] = [
  { kind: 'user', id: 'u1', text: 'Why 401?' },
  { kind: 'agent', id: 'a1', text: '', streaming: false },
  { kind: 'tool', id: 't1', callId: 'c1', title: 'get_request: login', status: 'completed' },
  {
    kind: 'agent',
    id: 'a2',
    text: 'The token expired.',
    streaming: false,
    error: 'stopped early',
  },
  { kind: 'notice', id: 'n1', text: 'Session ended.' },
];

describe('toStoredMessages', () => {
  it('maps every kind, drops empty and streaming segments and places proposals at their anchor', () => {
    const proposal = makeProposal({ id: 'p1', summary: 'Refresh the token', status: 'rejected' });
    const withStreaming: AssistantMessage[] = [
      ...messages,
      { kind: 'agent', id: 'a3', text: 'partial', streaming: true },
    ];
    expect(toStoredMessages(withStreaming, [proposal], { p1: 't1' })).toEqual([
      { kind: 'user', text: 'Why 401?' },
      { kind: 'tool', title: 'get_request: login', status: 'completed' },
      { kind: 'proposal', summary: 'Refresh the token', status: 'rejected' },
      { kind: 'agent', text: 'The token expired.' },
      { kind: 'notice', text: 'stopped early' },
      { kind: 'notice', text: 'Session ended.' },
    ]);
  });
});

describe('buildConversationInput', () => {
  const base = {
    conversationId: 'c1',
    agentConfigId: 'agent-1',
    mode: 'ask' as const,
    workspaceId: 'ws-1',
    collections: ['orders'],
    proposals: [],
    proposalAnchors: {},
  };

  it('returns null until the agent replied with text', () => {
    expect(
      buildConversationInput({ ...base, messages: [{ kind: 'user', id: 'u', text: 'hi' }] }),
    ).toBeNull();
  });

  it('builds the save payload, with continuedFrom only when set', () => {
    const chat: AssistantMessage[] = [
      { kind: 'user', id: 'u', text: 'hi' },
      { kind: 'agent', id: 'a', text: 'hello', streaming: false },
    ];
    expect(buildConversationInput({ ...base, messages: chat })).toEqual({
      id: 'c1',
      agentConfigId: 'agent-1',
      mode: 'ask',
      workspaceId: 'ws-1',
      collections: ['orders'],
      messages: [
        { kind: 'user', text: 'hi' },
        { kind: 'agent', text: 'hello' },
      ],
    });
    expect(
      buildConversationInput({ ...base, messages: chat, continuedFrom: 'c0' })?.continuedFrom,
    ).toBe('c0');
  });
});
```

- [ ] **Step 2: Run them to see them fail**

Run: `yarn test src/lib/__tests__/tauri-api.assistant-conversations.test.ts src/lib/assistant/__tests__/conversation-snapshot.test.ts`
Expected: FAIL with `does not provide an export named 'listConversations'` and `Failed to resolve import "../conversation-snapshot"`.

- [ ] **Step 3: Add the wrappers and types**

In `src/lib/tauri-api.ts`, after `onAgentProposalResolved` (`:2940-2943`), add:

```ts
// Stored assistant conversations. Records are DTOs, so their fields are
// camelCase. Message kinds and statuses are the same words as on disk. The
// three events are DomainEvent JSON, so their fields stay snake_case, and
// they carry ids only.

export type StoredMessage =
  | { kind: 'user'; text: string }
  | { kind: 'agent'; text: string }
  | { kind: 'tool'; title: string; status: AgentToolCallStatus }
  | { kind: 'notice'; text: string }
  | { kind: 'proposal'; summary: string; status: AgentProposalStatus };

export interface ConversationSummary {
  id: string;
  title: string;
  agentConfigId: string;
  mode: AssistantMode;
  createdAt: string;
  updatedAt: string;
  messageCount: number;
  continuedFrom?: string;
}

export interface ConversationCollectionRef {
  uid: string;
  name: string;
}

export interface ConversationRecord {
  id: string;
  title: string;
  agentConfigId: string;
  mode: AssistantMode;
  workspaceId: string;
  createdAt: string;
  updatedAt: string;
  collections: ConversationCollectionRef[];
  messages: StoredMessage[];
  continuedFrom?: string;
}

/** What the panel saves. `collections` are names; the backend stores their uid. */
export interface ConversationInput {
  id: string;
  agentConfigId: string;
  mode: AssistantMode;
  workspaceId: string;
  collections: string[];
  messages: StoredMessage[];
  continuedFrom?: string;
}

export const listConversations = () => invoke<ConversationSummary[]>('list_conversations');

export const getConversation = (id: string) =>
  invoke<ConversationRecord>('get_conversation', { id });

/** Saves into the workspace the session started in, masked by the backend. */
export const saveConversation = (sessionId: string, conversation: ConversationInput) =>
  invoke<ConversationSummary>('save_conversation', { sessionId, conversation });

export const deleteConversation = (id: string) => invoke<void>('delete_conversation', { id });

export const clearConversations = () => invoke<void>('clear_conversations');

export interface AssistantConversationSavedEvent {
  type: 'assistantConversationSaved';
  conversation_id: string;
}

export interface AssistantConversationDeletedEvent {
  type: 'assistantConversationDeleted';
  conversation_id: string;
}

export interface AssistantConversationsClearedEvent {
  type: 'assistantConversationsCleared';
}

export const onAssistantConversationSaved = (
  handler: (event: AssistantConversationSavedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AssistantConversationSavedEvent>('assistant-conversation-saved', (e) =>
    handler(e.payload),
  );

export const onAssistantConversationDeleted = (
  handler: (event: AssistantConversationDeletedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AssistantConversationDeletedEvent>('assistant-conversation-deleted', (e) =>
    handler(e.payload),
  );

export const onAssistantConversationsCleared = (
  handler: (event: AssistantConversationsClearedEvent) => void,
): Promise<UnlistenFn> =>
  listen<AssistantConversationsClearedEvent>('assistant-conversations-cleared', (e) =>
    handler(e.payload),
  );
```

- [ ] **Step 4: Move the timeline builder**

Create `src/lib/assistant/timeline.ts` with the `TimelineEntry` type and the `buildTimeline` function cut from `src/components/assistant/AssistantChatView.tsx:84-115`, unchanged, plus these imports:

```ts
import type { AgentProposal } from '@/lib/tauri-api';
import type { AssistantMessage } from '@/stores/assistant-store';
```

and `export` on `TimelineEntry`. In `AssistantChatView.tsx` delete those lines and add `import { buildTimeline } from '@/lib/assistant/timeline';`. Remove the `AgentProposal` import there if it is now unused.

- [ ] **Step 5: Implement the snapshot module**

Create `src/lib/assistant/conversation-snapshot.ts`:

```ts
import { buildTimeline } from '@/lib/assistant/timeline';
import type {
  AgentProposal,
  AssistantMode,
  ConversationInput,
  StoredMessage,
} from '@/lib/tauri-api';
import type { AssistantMessage } from '@/stores/assistant-store';

/** Everything one save needs, taken from the store and the saver. */
export interface ConversationSnapshot {
  conversationId: string;
  agentConfigId: string;
  mode: AssistantMode;
  workspaceId: string;
  collections: string[];
  messages: AssistantMessage[];
  proposals: AgentProposal[];
  proposalAnchors: Record<string, string | null>;
  continuedFrom?: string;
}

/**
 * The chat as stored messages, in reading order. Proposals sit after their
 * anchor message. Streaming and empty reply segments are left out, and a
 * reply's error becomes a notice.
 */
export function toStoredMessages(
  messages: AssistantMessage[],
  proposals: AgentProposal[],
  anchors: Record<string, string | null>,
): StoredMessage[] {
  const out: StoredMessage[] = [];
  for (const entry of buildTimeline(messages, proposals, anchors)) {
    if (entry.type === 'proposal') {
      out.push({
        kind: 'proposal',
        summary: entry.proposal.summary,
        status: entry.proposal.status,
      });
      continue;
    }
    const message = entry.message;
    switch (message.kind) {
      case 'user':
        out.push({ kind: 'user', text: message.text });
        break;
      case 'agent':
        if (message.streaming) break;
        if (message.text) out.push({ kind: 'agent', text: message.text });
        if (message.error) out.push({ kind: 'notice', text: message.error });
        break;
      case 'tool':
        out.push({ kind: 'tool', title: message.title, status: message.status });
        break;
      case 'notice':
        out.push({ kind: 'notice', text: message.text });
        break;
    }
  }
  return out;
}

/** True when the agent has replied with some text. */
export function hasAgentReply(messages: StoredMessage[]): boolean {
  return messages.some((m) => m.kind === 'agent' && m.text.trim() !== '');
}

/** The save payload, or null while there is no agent reply to keep. */
export function buildConversationInput(snapshot: ConversationSnapshot): ConversationInput | null {
  const messages = toStoredMessages(
    snapshot.messages,
    snapshot.proposals,
    snapshot.proposalAnchors,
  );
  if (!hasAgentReply(messages)) return null;
  return {
    id: snapshot.conversationId,
    agentConfigId: snapshot.agentConfigId,
    mode: snapshot.mode,
    workspaceId: snapshot.workspaceId,
    collections: snapshot.collections,
    messages,
    ...(snapshot.continuedFrom ? { continuedFrom: snapshot.continuedFrom } : {}),
  };
}
```

- [ ] **Step 6: Run the tests to see them pass**

Run: `yarn test src/lib/__tests__/tauri-api.assistant-conversations.test.ts src/lib/assistant/__tests__/conversation-snapshot.test.ts src/components/assistant/__tests__/AssistantChatView.test.tsx`
Expected: PASS, 5 new tests, and `AssistantChatView` tests still pass after the move.

- [ ] **Step 7: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.

Commit through the `dev-workflow-skills:1-git-commit` skill:

```bash
git add src/lib/tauri-api.ts src/lib/assistant/timeline.ts src/components/assistant/AssistantChatView.tsx src/lib/assistant/conversation-snapshot.ts src/lib/__tests__/tauri-api.assistant-conversations.test.ts src/lib/assistant/__tests__/conversation-snapshot.test.ts
git commit --only -m "feat(assistant): add conversation API wrappers and snapshot" -- src/lib/tauri-api.ts src/lib/assistant/timeline.ts src/components/assistant/AssistantChatView.tsx src/lib/assistant/conversation-snapshot.ts src/lib/__tests__/tauri-api.assistant-conversations.test.ts src/lib/assistant/__tests__/conversation-snapshot.test.ts
```

---

### Task 2: Conversation id in the store and the save routine

**Files:**
- Modify: `src/stores/assistant-store.ts` (`AssistantState` `:72-81`, initial state `:146-153`, `beginSession` `:159-170`, `appendUserMessage` `:185-198`, `reset` `:336-345`)
- Create: `src/lib/assistant/conversation-saver.ts`
- Modify: `src/lib/assistant-event-bridge.ts` (import; start in the effect `:59-61`; stop in the cleanup `:151-158`)
- Modify: `src/lib/__tests__/assistant-event-bridge.test.ts` (add `saveConversation` to `mocks.api` `:32-46`)
- Test: `src/stores/__tests__/assistant-store.test.ts`, `src/lib/assistant/__tests__/conversation-saver.test.ts`

**Interfaces:**
- Consumes: Task 1 `saveConversation`, `buildConversationInput`, `ConversationSummary`; store actions and `selectTurnRunning`.
- Produces (exact):
  ```ts
  // src/stores/assistant-store.ts
  interface AssistantState { /* ... */ conversationId?: string; }
  // beginSession and reset set conversationId to undefined; appendUserMessage sets it once
  // per session with crypto.randomUUID().

  // src/lib/assistant/conversation-saver.ts
  export const CONVERSATION_SAVE_DEBOUNCE_MS = 500;
  export const SAVE_FAILED_NOTICE: string;
  export function startConversationSaver(): () => void;   // returns stop
  ```

- [ ] **Step 1: Write the failing store test**

In `src/stores/__tests__/assistant-store.test.ts`, inside `describe('assistant-store', ...)`, add:

```ts
  it('gives the conversation an id at the first user message and a new one per session', () => {
    activate();
    expect(store().conversationId).toBeUndefined();
    store().appendUserMessage('hi');
    const first = store().conversationId;
    expect(first).toMatch(/^[0-9a-f-]{36}$/);
    store().completeMessage('s1');
    store().appendUserMessage('again');
    expect(store().conversationId).toBe(first);
    store().endSession();
    activate('s2');
    expect(store().conversationId).toBeUndefined();
  });
```

- [ ] **Step 2: Write the failing saver tests**

Create `src/lib/assistant/__tests__/conversation-saver.test.ts`:

```ts
import { toast } from 'sonner';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { type ConversationSummary, saveConversation } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { makeProposal } from '@/test/assistant-fixtures';
import { createDeferred } from '@/test/deferred';
import {
  CONVERSATION_SAVE_DEBOUNCE_MS,
  SAVE_FAILED_NOTICE,
  startConversationSaver,
} from '../conversation-saver';

vi.mock('sonner', () => ({ toast: { warning: vi.fn() } }));
vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  saveConversation: vi.fn(),
}));

const store = () => useAssistantStore.getState();
const save = vi.mocked(saveConversation);
const summary: ConversationSummary = {
  id: 'c1',
  title: 'Why 401?',
  agentConfigId: 'agent-1',
  mode: 'edit',
  createdAt: '2026-10-11T09:00:00Z',
  updatedAt: '2026-10-11T09:00:00Z',
  messageCount: 2,
};

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

function turn(question: string, reply: string, sessionId = 's1'): void {
  store().appendUserMessage(question);
  store().appendChunk(sessionId, reply);
  store().completeMessage(sessionId);
}

describe('startConversationSaver', () => {
  let stop: () => void;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    save.mockResolvedValue(summary);
    store().reset();
    useAssistantStore.setState({ panelOpen: true, focus: undefined });
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    stop = startConversationSaver();
  });

  afterEach(() => {
    stop();
    vi.useRealTimers();
  });

  it('saves 500 ms after a turn finishes', async () => {
    activate();
    turn('Why 401?', 'Hello');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS - 1);
    expect(save).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(save).toHaveBeenCalledTimes(1);
    const [sessionId, input] = save.mock.calls[0];
    expect(sessionId).toBe('s1');
    expect(input).toMatchObject({
      id: store().conversationId,
      agentConfigId: 'agent-1',
      mode: 'edit',
      workspaceId: 'ws-1',
      messages: [
        { kind: 'user', text: 'Why 401?' },
        { kind: 'agent', text: 'Hello' },
      ],
    });
  });

  it('never saves a reply that is still streaming or a chat without a reply', async () => {
    activate();
    store().appendUserMessage('Why 401?');
    store().appendChunk('s1', 'Partial');
    store().closePanel();
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save).not.toHaveBeenCalled();
  });

  it('saves at once when the panel closes, without the streaming segment', async () => {
    activate();
    turn('Why 401?', 'First');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    store().appendUserMessage('Again');
    store().appendChunk('s1', 'Partial');
    store().closePanel();
    await vi.advanceTimersByTimeAsync(0);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][1].messages).toEqual([
      { kind: 'user', text: 'Why 401?' },
      { kind: 'agent', text: 'First' },
      { kind: 'user', text: 'Again' },
    ]);
  });

  it('saves the proposals that ending the session drops', async () => {
    activate();
    turn('Why 401?', 'Hello');
    store().upsertProposal(
      makeProposal({ id: 'p1', sessionId: 's1', summary: 'Add a status test', status: 'accepted' }),
    );
    store().endSession('Session ended.');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save).toHaveBeenCalledTimes(1);
    expect(save.mock.calls[0][1].messages).toEqual([
      { kind: 'user', text: 'Why 401?' },
      { kind: 'agent', text: 'Hello' },
      { kind: 'proposal', summary: 'Add a status test', status: 'accepted' },
      { kind: 'notice', text: 'Session ended.' },
    ]);
  });

  it('saves one after another, so an older snapshot never lands last', async () => {
    const first = createDeferred<ConversationSummary>();
    save.mockImplementationOnce(() => first.promise);
    activate();
    turn('Why 401?', 'Hello');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save).toHaveBeenCalledTimes(1);
    store().endSession('Session ended.');
    await vi.advanceTimersByTimeAsync(0);
    expect(save).toHaveBeenCalledTimes(1);
    first.resolve(summary);
    await vi.advanceTimersByTimeAsync(0);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][1].messages.at(-1)).toEqual({
      kind: 'notice',
      text: 'Session ended.',
    });
  });

  it('shows one quiet notice per conversation when saves fail', async () => {
    save.mockRejectedValue(new Error('disk full'));
    activate();
    turn('Why 401?', 'One');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    turn('More', 'Two');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save).toHaveBeenCalledTimes(2);
    expect(toast.warning).toHaveBeenCalledTimes(1);
    expect(toast.warning).toHaveBeenCalledWith(SAVE_FAILED_NOTICE);
    expect(store().session?.status).toBe('active');
  });

  it('records the workspace the session started in and the focused collections', async () => {
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    activate();
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' });
    turn('Why 401?', 'Hello');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save.mock.calls[0][1]).toMatchObject({ workspaceId: 'ws-1', collections: ['orders'] });
  });
});
```

- [ ] **Step 3: Run them to see them fail**

Run: `yarn test src/stores/__tests__/assistant-store.test.ts src/lib/assistant/__tests__/conversation-saver.test.ts`
Expected: FAIL: `expected undefined to match /^[0-9a-f-]{36}$/` in the store test, and `Failed to resolve import "../conversation-saver"`.

- [ ] **Step 4: Add `conversationId` to the store**

In `src/stores/assistant-store.ts`:
- In `AssistantState`, after `panelOpen: boolean;` add:

```ts
  /** The stored conversation's id. Made at the first user message of a session. */
  conversationId?: string;
```

- In the initial state add `conversationId: undefined,` after `panelOpen: false,`.
- In `beginSession`'s `set({...})` and in `reset`'s `set({...})` add `conversationId: undefined,`.
- In `appendUserMessage` replace the `set({ messages: [...] })` call with:

```ts
    set({
      conversationId: state.conversationId ?? crypto.randomUUID(),
      messages: [
        ...state.messages,
        { kind: 'user', id: userId, text },
        { kind: 'agent', id: agentId, text: '', streaming: true },
      ],
    });
```

- [ ] **Step 5: Implement the saver**

Create `src/lib/assistant/conversation-saver.ts`:

```ts
import { toast } from 'sonner';
import { buildConversationInput } from '@/lib/assistant/conversation-snapshot';
import { saveConversation } from '@/lib/tauri-api';
import { type AssistantState, selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

/** Wait after a turn ends, so a burst of late updates makes one save. */
export const CONVERSATION_SAVE_DEBOUNCE_MS = 500;

export const SAVE_FAILED_NOTICE =
  'This conversation could not be saved to the history. The chat itself is not affected.';

type SaveSource = Pick<
  AssistantState,
  'session' | 'conversationId' | 'messages' | 'proposals' | 'proposalAnchors'
>;

/** What the saver keeps per session: where it started and what it focused on. */
interface TrackedSession {
  sessionId: string;
  workspaceId: string;
  collections: Set<string>;
}

/**
 * Saves the assistant conversation 500 ms after a turn ends, and at once
 * when the session ends or the panel closes. Saves run one after another,
 * so an older snapshot never lands after a newer one. A failed save shows
 * one quiet notice per conversation and never touches the chat. The backend
 * picks the workspace from the session id. Returns a stop function.
 */
export function startConversationSaver(): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let queue: Promise<void> = Promise.resolve();
  const warned = new Set<string>();
  let tracked: TrackedSession | undefined;

  const cancelTimer = () => {
    if (timer !== undefined) {
      clearTimeout(timer);
      timer = undefined;
    }
  };

  const save = (source: SaveSource) => {
    const { session, conversationId } = source;
    if (!session?.sessionId || !conversationId || tracked?.sessionId !== session.sessionId) return;
    const input = buildConversationInput({
      conversationId,
      agentConfigId: session.agentConfigId,
      mode: session.mode,
      workspaceId: tracked.workspaceId,
      collections: [...tracked.collections],
      messages: source.messages,
      proposals: source.proposals,
      proposalAnchors: source.proposalAnchors,
    });
    if (!input) return;
    const sessionId = session.sessionId;
    queue = queue.then(async () => {
      try {
        await saveConversation(sessionId, input);
      } catch (err) {
        console.error('[assistant] failed to save the conversation', err);
        if (!warned.has(conversationId)) {
          warned.add(conversationId);
          toast.warning(SAVE_FAILED_NOTICE);
        }
      }
    });
  };

  const unsubscribe = useAssistantStore.subscribe((state, prev) => {
    const session = state.session;
    if (session?.status === 'active' && tracked?.sessionId !== session.sessionId) {
      tracked = {
        sessionId: session.sessionId,
        workspaceId: useWorkspaceStore.getState().activeWorkspaceId,
        collections: new Set(),
      };
    }
    if (tracked && session?.sessionId === tracked.sessionId && state.focus?.collection) {
      tracked.collections.add(state.focus.collection);
    }

    const wasActive = prev.session?.status === 'active';
    const sameSession = session?.sessionId === prev.session?.sessionId;
    if (wasActive && !(session?.status === 'active' && sameSession)) {
      // The session ended. endSession and a fatal failure drop the proposals
      // in this same update, so they come from the previous state.
      cancelTimer();
      save(
        sameSession
          ? { ...state, proposals: prev.proposals, proposalAnchors: prev.proposalAnchors }
          : prev,
      );
      return;
    }
    if (prev.panelOpen && !state.panelOpen && session?.status === 'active') {
      cancelTimer();
      save(state);
      return;
    }
    if (session?.status === 'active' && selectTurnRunning(prev) && !selectTurnRunning(state)) {
      cancelTimer();
      timer = setTimeout(() => {
        timer = undefined;
        save(useAssistantStore.getState());
      }, CONVERSATION_SAVE_DEBOUNCE_MS);
    }
  });

  return () => {
    cancelTimer();
    unsubscribe();
  };
}
```

- [ ] **Step 6: Start the saver from the bridge**

In `src/lib/assistant-event-bridge.ts`:
- Add `import { startConversationSaver } from '@/lib/assistant/conversation-saver';`.
- At the top of the effect, after `void sweepStaleAssistantSessions();`, add:

```ts
    // Saves the conversation after each turn, at session end and at panel close.
    const stopSaver = startConversationSaver();
```

- In the cleanup, after `disposed = true;`, add `stopSaver();`.

In `src/lib/__tests__/assistant-event-bridge.test.ts`, add to `mocks.api`:

```ts
      saveConversation: vi.fn().mockResolvedValue(undefined),
```

- [ ] **Step 7: Run the tests to see them pass**

Run: `yarn test src/stores/__tests__/assistant-store.test.ts src/lib/assistant/__tests__/conversation-saver.test.ts src/lib/__tests__/assistant-event-bridge.test.ts`
Expected: PASS (1 new store test, 7 saver tests, the bridge tests unchanged).

- [ ] **Step 8: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.

Commit through the `dev-workflow-skills:1-git-commit` skill:

```bash
git add src/stores/assistant-store.ts src/lib/assistant/conversation-saver.ts src/lib/assistant-event-bridge.ts src/lib/__tests__/assistant-event-bridge.test.ts src/stores/__tests__/assistant-store.test.ts src/lib/assistant/__tests__/conversation-saver.test.ts
git commit --only -m "feat(assistant): save conversations after turns and at session end" -- src/stores/assistant-store.ts src/lib/assistant/conversation-saver.ts src/lib/assistant-event-bridge.ts src/lib/__tests__/assistant-event-bridge.test.ts src/stores/__tests__/assistant-store.test.ts src/lib/assistant/__tests__/conversation-saver.test.ts
```

---

### Task 3: History view, read-only conversation, delete and clear

**Files:**
- Modify: `src/components/assistant/AssistantChatView.tsx` (export `ChatItem`, `:43`)
- Create: `src/components/assistant/AssistantConversationView.tsx`
- Create: `src/components/assistant/AssistantHistoryView.tsx`
- Modify: `src/components/assistant/AssistantPanel.tsx` (imports `:1-10`, header `:83-106`, body `:107-122`)
- Modify: `src/components/assistant/__tests__/AssistantPanel.test.tsx` (mock list `:21-30`, one new test)
- Modify: `.claude/assistant-conversations.md` (add a "Frontend" section)
- Test: `src/components/assistant/__tests__/AssistantHistoryView.test.tsx`

**Interfaces:**
- Consumes: Task 1 wrappers and types; `useAgentConfigs`, `formatRelativeTime`, `useWorkspaceStore`.
- Produces (exact):
  ```ts
  // src/components/assistant/AssistantChatView.tsx
  export function ChatItem(props: { message: AssistantMessage; isLastStreaming: boolean }): JSX.Element | null;
  // src/components/assistant/AssistantConversationView.tsx
  export function toChatMessage(message: StoredMessage, index: number): AssistantMessage | undefined;
  export function AssistantConversationView(props: { record: ConversationRecord; agentLabel: string;
    onBack: () => void }): JSX.Element;
  // src/components/assistant/AssistantHistoryView.tsx
  export const HISTORY_PRIVACY_NOTICE: string;
  export const HISTORY_EMPTY_TEXT = 'No saved conversations in this workspace yet.';
  export function AssistantHistoryView(): JSX.Element;
  ```

- [ ] **Step 1: Write the failing History view tests**

Create `src/components/assistant/__tests__/AssistantHistoryView.test.tsx`:

```tsx
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import type { ConversationRecord, ConversationSummary } from '@/lib/tauri-api';
import { useWorkspaceStore } from '@/stores/workspace-store';
import {
  AssistantHistoryView,
  HISTORY_EMPTY_TEXT,
  HISTORY_PRIVACY_NOTICE,
} from '../AssistantHistoryView';

const mocks = vi.hoisted(() => {
  const handlers: Record<string, (payload: unknown) => void> = {};
  const listener = (name: string) =>
    vi.fn((handler: (payload: unknown) => void) => {
      handlers[name] = handler;
      return Promise.resolve(() => undefined);
    });
  return {
    handlers,
    api: {
      listConversations: vi.fn(),
      getConversation: vi.fn(),
      deleteConversation: vi.fn(),
      clearConversations: vi.fn(),
      onAssistantConversationSaved: listener('saved'),
      onAssistantConversationDeleted: listener('deleted'),
      onAssistantConversationsCleared: listener('cleared'),
    },
  };
});

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  ...mocks.api,
}));
vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [{ id: 'agent-1', label: 'Claude' }] }),
}));
vi.mock('@/components/collections/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ children }: { children: string }) => <div>{children}</div>,
}));

const summary: ConversationSummary = {
  id: 'c1',
  title: 'Why does login return 401?',
  agentConfigId: 'agent-1',
  mode: 'ask',
  createdAt: '2026-10-11T09:12:00Z',
  updatedAt: '2026-10-11T09:20:41Z',
  messageCount: 4,
};

const record: ConversationRecord = {
  id: 'c1',
  title: 'Why does login return 401?',
  agentConfigId: 'agent-1',
  mode: 'ask',
  workspaceId: 'ws-1',
  createdAt: '2026-10-11T09:12:00Z',
  updatedAt: '2026-10-11T09:20:41Z',
  collections: [],
  messages: [
    { kind: 'user', text: 'Why does login return 401?' },
    { kind: 'agent', text: 'The token expired.' },
    { kind: 'tool', title: 'get_request: login', status: 'completed' },
    { kind: 'proposal', summary: 'Refresh the token', status: 'pending' },
  ],
};

describe('AssistantHistoryView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    vi.mocked(api.listConversations).mockResolvedValue([summary]);
    vi.mocked(api.getConversation).mockResolvedValue(record);
    vi.mocked(api.deleteConversation).mockResolvedValue(undefined);
    vi.mocked(api.clearConversations).mockResolvedValue(undefined);
  });

  it('lists the conversations with their agent and message count, under the privacy notice', async () => {
    render(<AssistantHistoryView />);
    expect(await screen.findByText('Why does login return 401?')).toBeInTheDocument();
    expect(screen.getByText(/Claude · 4 messages/)).toBeInTheDocument();
    expect(screen.getByText(HISTORY_PRIVACY_NOTICE)).toBeInTheDocument();
  });

  it('says so when the workspace has no conversations', async () => {
    vi.mocked(api.listConversations).mockResolvedValue([]);
    render(<AssistantHistoryView />);
    expect(await screen.findByText(HISTORY_EMPTY_TEXT)).toBeInTheDocument();
  });

  it('opens a conversation read-only, with proposals as plain lines', async () => {
    render(<AssistantHistoryView />);
    await userEvent.click(await screen.findByText('Why does login return 401?'));
    expect(await screen.findByText('The token expired.')).toBeInTheDocument();
    expect(screen.getByText('get_request: login')).toBeInTheDocument();
    expect(screen.getByText('Proposal: Refresh the token (not decided)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /accept|reject/i })).toBeNull();
    expect(api.getConversation).toHaveBeenCalledWith('c1');
    await userEvent.click(screen.getByRole('button', { name: 'Back to the history' }));
    expect(await screen.findByText(HISTORY_PRIVACY_NOTICE)).toBeInTheDocument();
  });

  it('deletes one conversation and reloads the list', async () => {
    render(<AssistantHistoryView />);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Delete "Why does login return 401?"' }),
    );
    expect(api.deleteConversation).toHaveBeenCalledWith('c1');
    await waitFor(() => expect(api.listConversations).toHaveBeenCalledTimes(2));
  });

  it('clears all only after the confirm step', async () => {
    render(<AssistantHistoryView />);
    await screen.findByText('Why does login return 401?');
    await userEvent.click(screen.getByRole('button', { name: 'Clear all' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(api.clearConversations).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Clear all' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Delete all' }));
    await waitFor(() => expect(api.clearConversations).toHaveBeenCalledTimes(1));
  });

  it('reloads when a conversation is saved elsewhere', async () => {
    render(<AssistantHistoryView />);
    await waitFor(() => expect(mocks.handlers.saved).toBeDefined());
    await screen.findByText('Why does login return 401?');
    act(() => mocks.handlers.saved({ type: 'assistantConversationSaved', conversation_id: 'c2' }));
    await waitFor(() => expect(api.listConversations).toHaveBeenCalledTimes(2));
  });

  it('a workspace switch closes the open conversation and reloads', async () => {
    render(<AssistantHistoryView />);
    await userEvent.click(await screen.findByText('Why does login return 401?'));
    expect(await screen.findByRole('button', { name: 'Back to the history' })).toBeInTheDocument();
    act(() => useWorkspaceStore.setState({ activeWorkspaceId: 'ws-2' }));
    expect(await screen.findByText(HISTORY_PRIVACY_NOTICE)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Back to the history' })).toBeNull();
    expect(api.listConversations).toHaveBeenCalledTimes(2);
  });
});
```

In `src/components/assistant/__tests__/AssistantPanel.test.tsx`, add to the `vi.mock('@/lib/tauri-api', ...)` object:

```ts
  listConversations: vi.fn().mockResolvedValue([]),
  onAssistantConversationSaved: vi.fn().mockResolvedValue(() => undefined),
  onAssistantConversationDeleted: vi.fn().mockResolvedValue(() => undefined),
  onAssistantConversationsCleared: vi.fn().mockResolvedValue(() => undefined),
```

and this test in `describe('AssistantPanel', ...)`:

```tsx
  it('switches between the chat and the conversation history', async () => {
    vi.mocked(api.listConversations).mockResolvedValue([]);
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'Conversation history' }));
    expect(
      await screen.findByText('No saved conversations in this workspace yet.'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Back to the chat' }));
    expect(screen.getByRole('button', { name: 'Start' })).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run them to see them fail**

Run: `yarn test src/components/assistant/__tests__/AssistantHistoryView.test.tsx src/components/assistant/__tests__/AssistantPanel.test.tsx`
Expected: FAIL with `Failed to resolve import "../AssistantHistoryView"` and, in the panel test, `Unable to find an accessible element with the role "button" and name "Conversation history"`.

- [ ] **Step 3: Export `ChatItem`**

In `src/components/assistant/AssistantChatView.tsx`, change `function ChatItem({` to `export function ChatItem({`.

- [ ] **Step 4: Implement the read-only conversation view**

Create `src/components/assistant/AssistantConversationView.tsx`:

```tsx
import { ArrowLeft } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { AgentProposalStatus, ConversationRecord, StoredMessage } from '@/lib/tauri-api';
import type { AssistantMessage } from '@/stores/assistant-store';
import { ChatItem } from './AssistantChatView';

const PROPOSAL_STATUS_LABEL: Record<AgentProposalStatus, string> = {
  pending: 'not decided',
  accepted: 'accepted',
  rejected: 'rejected',
  stale: 'stale',
  failed: 'failed',
};

/** A stored message as a chat message for `ChatItem`. Stored messages have no id, so the index is used. */
export function toChatMessage(message: StoredMessage, index: number): AssistantMessage | undefined {
  const id = `stored-${index}`;
  switch (message.kind) {
    case 'user':
      return { kind: 'user', id, text: message.text };
    case 'agent':
      return { kind: 'agent', id, text: message.text, streaming: false };
    case 'tool':
      return { kind: 'tool', id, callId: id, title: message.title, status: message.status };
    case 'notice':
      return { kind: 'notice', id, text: message.text };
    case 'proposal':
      return undefined;
  }
}

/** One stored conversation, read-only. Proposals are history lines without actions. */
export function AssistantConversationView({
  record,
  agentLabel,
  onBack,
}: {
  record: ConversationRecord;
  agentLabel: string;
  onBack: () => void;
}) {
  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='flex items-center gap-2 border-b px-3 py-2'>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Back to the history'
          onClick={onBack}
        >
          <ArrowLeft className='h-4 w-4' aria-hidden='true' />
        </Button>
        <div className='min-w-0 flex-1'>
          <p className='truncate text-sm font-medium'>{record.title || 'Untitled conversation'}</p>
          <p className='text-xs text-muted-foreground'>{`${agentLabel} · read-only`}</p>
        </div>
      </div>
      <ScrollArea className='min-h-0 flex-1'>
        <div className='flex flex-col gap-3 p-3'>
          {record.messages.map((message, index) => {
            const key = `stored-${index}`;
            if (message.kind === 'proposal') {
              return (
                // biome-ignore lint/suspicious/noArrayIndexKey: stored messages have no id and never reorder.
                <p key={key} className='text-xs text-muted-foreground'>
                  {`Proposal: ${message.summary} (${PROPOSAL_STATUS_LABEL[message.status]})`}
                </p>
              );
            }
            const chat = toChatMessage(message, index);
            // biome-ignore lint/suspicious/noArrayIndexKey: stored messages have no id and never reorder.
            return chat ? <ChatItem key={key} message={chat} isLastStreaming={false} /> : null;
          })}
        </div>
      </ScrollArea>
    </div>
  );
}
```

If `yarn check` reports the `biome-ignore` comments as unused (the key is a template, not the bare index), delete them.

- [ ] **Step 5: Implement the History view**

Create `src/components/assistant/AssistantHistoryView.tsx`:

```tsx
import { Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import {
  type ConversationRecord,
  type ConversationSummary,
  clearConversations,
  deleteConversation,
  getConversation,
  listConversations,
  onAssistantConversationDeleted,
  onAssistantConversationSaved,
  onAssistantConversationsCleared,
} from '@/lib/tauri-api';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { AssistantConversationView } from './AssistantConversationView';

export const HISTORY_PRIVACY_NOTICE =
  'Conversations are stored on this computer, inside this workspace, and are never committed to git. Secret variable values are masked. A secret you paste that is not a variable, such as a vault secret, is stored as typed. You can delete a conversation or clear all at any time.';

export const HISTORY_EMPTY_TEXT = 'No saved conversations in this workspace yet.';

const UNTITLED = 'Untitled conversation';

/** The stored conversations of the active workspace. */
export function AssistantHistoryView() {
  const workspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const { data: agentConfigs = [] } = useAgentConfigs();
  const [items, setItems] = useState<ConversationSummary[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string>();
  const [open, setOpen] = useState<ConversationRecord>();
  const [confirmClear, setConfirmClear] = useState(false);

  const reload = useCallback(async () => {
    try {
      setItems(await listConversations());
      setError(undefined);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoaded(true);
    }
  }, []);

  // A workspace switch closes the open conversation and loads the other list.
  // biome-ignore lint/correctness/useExhaustiveDependencies: workspaceId is a change trigger only.
  useEffect(() => {
    setOpen(undefined);
    void reload();
  }, [workspaceId, reload]);

  // A finished save or another window changes the list without a reopen.
  useEffect(() => {
    let disposed = false;
    const guard =
      <T,>(handler: (event: T) => void) =>
      (event: T) => {
        if (!disposed) handler(event);
      };
    const registrations = [
      onAssistantConversationSaved(guard(() => void reload())),
      onAssistantConversationDeleted(
        guard((e) => {
          setOpen((current) => (current?.id === e.conversation_id ? undefined : current));
          void reload();
        }),
      ),
      onAssistantConversationsCleared(
        guard(() => {
          setOpen(undefined);
          void reload();
        }),
      ),
    ];
    const settled = Promise.allSettled(registrations).then((results) =>
      results.flatMap((result) => (result.status === 'fulfilled' ? [result.value] : [])),
    );
    return () => {
      disposed = true;
      void settled.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, [reload]);

  const agentLabel = (id: string) => agentConfigs.find((c) => c.id === id)?.label ?? id;

  const openConversation = async (id: string) => {
    try {
      setOpen(await getConversation(id));
    } catch (err) {
      toast.error(`Could not open the conversation: ${String(err)}`);
    }
  };

  const remove = async (id: string) => {
    try {
      await deleteConversation(id);
    } catch (err) {
      toast.error(`Could not delete the conversation: ${String(err)}`);
    } finally {
      void reload();
    }
  };

  const clearAll = async () => {
    setConfirmClear(false);
    try {
      await clearConversations();
      setOpen(undefined);
    } catch (err) {
      toast.error(`Could not clear the history: ${String(err)}`);
    } finally {
      void reload();
    }
  };

  if (open) {
    return (
      <AssistantConversationView
        record={open}
        agentLabel={agentLabel(open.agentConfigId)}
        onBack={() => setOpen(undefined)}
      />
    );
  }

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='flex items-center gap-2 border-b px-3 py-2'>
        <span className='flex-1 text-sm font-medium'>History</span>
        <Button
          variant='ghost'
          size='sm'
          className='h-7 text-xs'
          disabled={items.length === 0}
          onClick={() => setConfirmClear(true)}
        >
          Clear all
        </Button>
      </div>
      <p className='border-b px-3 py-2 text-xs text-muted-foreground'>{HISTORY_PRIVACY_NOTICE}</p>
      <ScrollArea className='min-h-0 flex-1'>
        <ul className='flex flex-col gap-1 p-2'>
          {loaded && error && (
            <li role='alert' className='px-1 text-xs text-destructive'>
              {error}
            </li>
          )}
          {loaded && !error && items.length === 0 && (
            <li className='px-1 text-xs text-muted-foreground'>{HISTORY_EMPTY_TEXT}</li>
          )}
          {items.map((item) => (
            <li key={item.id} className='flex items-start gap-1'>
              <Button
                variant='ghost'
                className='h-auto min-w-0 flex-1 flex-col items-start gap-0.5 px-2 py-1.5 text-left'
                onClick={() => void openConversation(item.id)}
              >
                <span className='w-full truncate text-sm'>{item.title || UNTITLED}</span>
                <span className='text-xs text-muted-foreground'>
                  {`${agentLabel(item.agentConfigId)} · ${item.messageCount} messages · ${formatRelativeTime(item.updatedAt)}`}
                </span>
              </Button>
              <Button
                variant='ghost'
                size='icon'
                className='h-7 w-7 shrink-0'
                aria-label={`Delete "${item.title || UNTITLED}"`}
                onClick={() => void remove(item.id)}
              >
                <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
              </Button>
            </li>
          ))}
        </ul>
      </ScrollArea>
      <AlertDialog open={confirmClear} onOpenChange={setConfirmClear}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete all conversations?</AlertDialogTitle>
            <AlertDialogDescription>
              Every stored conversation of this workspace is deleted. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void clearAll()}>Delete all</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
```

- [ ] **Step 6: Add the History toggle to the panel**

In `src/components/assistant/AssistantPanel.tsx`:
- Change the icon import to `import { History, Loader2, Sparkles, X } from 'lucide-react';` and add `import { AssistantHistoryView } from './AssistantHistoryView';`.
- After `const [windowWidth, setWindowWidth] = ...` add `const [showHistory, setShowHistory] = useState(false);`.
- In the header, before `<AssistantPermissionsPopover />`, add:

```tsx
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label={showHistory ? 'Back to the chat' : 'Conversation history'}
          aria-pressed={showHistory}
          onClick={() => setShowHistory((shown) => !shown)}
        >
          <History className='h-4 w-4' aria-hidden='true' />
        </Button>
```

- Wrap the body: replace `{status === 'starting' ? (` with `{showHistory ? (\n        <AssistantHistoryView />\n      ) : status === 'starting' ? (`, keeping the rest of the chain unchanged.

- [ ] **Step 7: Run the tests to see them pass**

Run: `yarn test src/components/assistant/__tests__/AssistantHistoryView.test.tsx src/components/assistant/__tests__/AssistantPanel.test.tsx src/components/assistant/__tests__/AssistantChatView.test.tsx`
Expected: PASS (7 History view tests, 1 new panel test, existing tests unchanged).

- [ ] **Step 8: Document the frontend**

Append to `.claude/assistant-conversations.md`:

```markdown
## Frontend

- `assistant-store.conversationId` is made at the first user message of a session (`crypto.randomUUID()`), cleared by `beginSession` and `reset`.
- `startConversationSaver` (`src/lib/assistant/conversation-saver.ts`, started by `useAssistantEventBridge`) saves 500 ms after a turn ends, and at once at session end and panel close. Saves are chained, so they reach the backend in order. Streaming segments are never saved, and a chat without an agent reply is not saved. The session-end save takes proposals from the previous state, because `endSession` and a fatal `failMessage` drop them in the same update. A failed save shows one toast per conversation.
- `toStoredMessages` (`conversation-snapshot.ts`) places proposals by their anchor (`src/lib/assistant/timeline.ts`).
- History: the panel's History button shows `AssistantHistoryView` (list, delete, Clear all with a confirm). `AssistantConversationView` renders stored messages with `ChatItem`; stored proposals are plain lines with no actions. The view reloads on the three events and on a workspace switch, which also closes the open conversation.
```

- [ ] **Step 9: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.

Commit through the `dev-workflow-skills:1-git-commit` skill:

```bash
git add src/components/assistant/AssistantChatView.tsx src/components/assistant/AssistantConversationView.tsx src/components/assistant/AssistantHistoryView.tsx src/components/assistant/AssistantPanel.tsx src/components/assistant/__tests__/AssistantPanel.test.tsx src/components/assistant/__tests__/AssistantHistoryView.test.tsx .claude/assistant-conversations.md
git commit --only -m "feat(assistant): add the conversation history view" -- src/components/assistant/AssistantChatView.tsx src/components/assistant/AssistantConversationView.tsx src/components/assistant/AssistantHistoryView.tsx src/components/assistant/AssistantPanel.tsx src/components/assistant/__tests__/AssistantPanel.test.tsx src/components/assistant/__tests__/AssistantHistoryView.test.tsx .claude/assistant-conversations.md
```

---

## Next Plan

Plan 03 (`2026-10-11-assistant-conversation-history-plan-03-replay.md`) adds `buildReplayPreface`, the Continue action and the composer size hint.

## Post-Implementation Review

A reviewer (Sonnet is enough for this plan) reads the diff against the Review Focus list, with extra attention on the saver's transition logic (session end vs. a new session, panel close while streaming) and on the History view's listener cleanup. Findings go into the ledger as `Ruling:` lines. Manual check after this plan: chat two turns, restart the app, open History, open the conversation, delete it, clear all.
