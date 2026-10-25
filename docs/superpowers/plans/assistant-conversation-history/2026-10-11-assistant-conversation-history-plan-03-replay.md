# Assistant Conversation History, Plan 03: Replay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user continue a stored conversation: a new session in Ask mode whose first prompt carries a bounded, read-only replay of the old transcript, saved as a new conversation that records `continued_from`.

**Architecture:** A pure `buildReplayPreface(record, limits)` turns a `ConversationRecord` into a framed text block of at most 12,000 characters (newest messages kept, oldest first, a marker when older ones were dropped, no tool results). `continueConversation` ends any live session and calls `startAssistant(agentConfigId, 'ask', { fromId, preface })`; the store keeps the seed, `sendAssistantMessage` puts the preface in front of the first prompt only, and the chat (and so the saved conversation) keeps only what the user typed. The saver sends `continuedFrom`, which Plan 01 already stores and lists; the composer and the History list show it.

**Tech Stack:** React 19 + TypeScript, Zustand, shadcn/ui (`Button`, `Select`), lucide-react, Vitest + Testing Library.

**Spec:** [`docs/superpowers/specs/2026-10-11-assistant-conversation-history-design.md`](../../specs/2026-10-11-assistant-conversation-history-design.md), section 7 (stage 2), sections 8, 9 and 10. Plan index: [`00-index.md`](00-index.md). Depends on Plans 01 and 02.

## Verified facts

Line numbers are from `worktree-acp-mcp-tool-server` at `e9f01191`; Plan 02 shifts some of them, so anchor edits by content.

- `startAssistant(agentConfigId, mode = DEFAULT_ASSISTANT_MODE)` (`src/lib/assistant/assistant-session.ts:48-76`) calls `beginSession(agentConfigId, mode)` and `startWorkspaceAssistant(agentConfigId, mode, loadRememberedModel(agentConfigId))`. `DEFAULT_ASSISTANT_MODE` is `'edit'` (`:13`), so Continue must pass `'ask'` explicitly.
- `sendAssistantMessage(text, resources?)` (`:78-94`) appends the trimmed text with `appendUserMessage` and sends it with `sendAgentPrompt(sessionId, trimmed, resources)`. Its tests expect the plain text when nothing is seeded (`src/lib/assistant/__tests__/assistant-session.test.ts:123-156`).
- `endAssistantSession(notice?)` (`:108-119`) ends the store session first (which the Plan 02 saver turns into a final save) and then the backend session.
- `send_agent_prompt` has no limit on the prompt text; only resources are capped (8 of 8 KB, `src-tauri/src/commands/acp_session_dto.rs:11-13`, `:106-117`). The preface therefore goes into the prompt text, and the size hint is informational.
- `ConversationRecord`, `StoredMessage`, `ConversationSummary.continuedFrom` and `ConversationInput.continuedFrom` exist after Plan 02 Task 1. The backend validates `continued_from` as a lowercase UUID and stores it (Plan 01 Task 3).
- `startConversationSaver` builds the snapshot from `SaveSource` (Plan 02 Task 2); `buildConversationInput` already copies `continuedFrom`.
- `AssistantStartView` (`src/components/assistant/AssistantStartView.tsx:54-72`) is the precedent for the agent `Select` with an empty value and a placeholder. `Composer` renders `ComposerChips`, `PromptEditor` and `ComposerToolbar` (`src/components/assistant/composer/Composer.tsx:212-243`); its test is `src/components/assistant/composer/__tests__/Composer.test.tsx`.

## Global Constraints

- OC-1: Location is `<workspace>/assistant/conversations/<id>.yml` only, never inside `<workspace>/collections/`, a collection, a folder or a linked collection's directory.
- OC-2: No transcript data and no new keys go into `opencollection.yml`, `folder.yml`, request files, environment files or `workspace.yml`; no `extensions.*` keys.
- OC-3: Files are `.yml`, named `<id>.yml` with a generated lowercase UUID, never a reserved name; writes use `atomic_write`.
- OC-4: `<workspace>/assistant/.gitignore` holds `*`, is created with `create_new` before the first write, never overwritten, and a failure is only logged.
- OC-5: The path is resolved on every call from the active workspace (or the session's pinned one), directories are created lazily, and the folder is outside the collections watcher.
- OC-6: A collection is referenced only by its `opencollection.yml` uid (plus name) as metadata, never by a path.
- OC-7: Reads tolerate missing and unknown keys and skip an unparseable or other-version file with a warning.
- Persistence keys are snake_case (`continued_from`); camelCase only on the IPC DTOs (`continuedFrom`).
- Caps: 50 conversations, 500 messages, 16 KB per message; the replay preface is at most 12,000 characters.
- Continue always starts in Ask mode; a stored Agent or Edit mode is never re-applied. The user raises it with the mode control.
- Events carry ids only.
- No vault scrubbing in v1; stage 2 sends stored (already masked) text to the agent, the same exposure as the original chat.
- Always pass `-j4` to `cargo`; never `cargo test --workspace`.
- No unwrap calls in Rust production code (none is written here).
- shadcn/ui primitives and lucide-react icons only; no raw `<button>`, `<input>`, `<select>`, no inline SVG.
- Narrow Zustand selectors; never destructure the whole store in a component.
- Harness: no bare `git stash`/`git stash pop`; no `git add -A`, `--all` or `.`; staged `.ts/.tsx` run `yarn tsc --noEmit` and `yarn check` before commit.
- Commits: every commit goes through the `dev-workflow-skills:1-git-commit` skill, conventional format, explicit `git add <paths>` and a pathspec commit, never `git add -A`.
- Comments are short full sentences ending with a period.

## Review Focus

1. **The preface goes out once and is never stored.** Only the first prompt of the continued session carries it, a failed first send keeps it for the retry, and the chat (so the saved conversation) holds only the typed text, so continuing a continued conversation never nests prefaces. Tests: `continues in Ask mode and sends the preface with the first message only`, `keeps the preface when the first send fails` (Task 2).
2. **The 12,000-character limit.** The whole preface stays within the limit, keeps the newest messages oldest-first, marks dropped ones, and cuts an oversized newest message from the front. Tests: `keeps the newest messages that fit and marks the dropped ones`, `keeps the end of a newest message longer than the limit`, `uses a 12,000 character limit by default` (Task 1).
3. **No tool results.** Only tool titles and statuses appear, even if a stored message carries extra fields. Test: `never includes tool results` (Task 1).
4. **A stored Agent mode is never re-applied, and a live session is ended first.** Test: `continues in Ask mode and sends the preface with the first message only` (Task 2).
5. **A deleted agent config.** The stored conversation still opens, and Continue asks for an agent and stays disabled until one is picked. Test: `asks for an agent when the stored one is gone` (Task 2).

---

### Task 1: `buildReplayPreface`

**Files:**
- Create: `src/lib/assistant/replay-preface.ts`
- Test: `src/lib/assistant/__tests__/replay-preface.test.ts`

**Interfaces:**
- Consumes: `ConversationRecord`, `StoredMessage` (Plan 02 Task 1).
- Produces (exact):
  ```ts
  export interface ReplayLimits { maxChars: number }
  export const DEFAULT_REPLAY_LIMITS: ReplayLimits;            // { maxChars: 12_000 }
  export const REPLAY_PREFACE_HEADER =
    'This is a previous conversation with the user, for context only. Do not repeat its actions.';
  export const REPLAY_DROPPED_MARKER = '[Older messages of this conversation were left out.]';
  export const REPLAY_OPEN = '<previous-conversation>';
  export const REPLAY_CLOSE = '</previous-conversation>';
  export function replayLine(message: StoredMessage): string | undefined;
  export function buildReplayPreface(record: ConversationRecord, limits?: ReplayLimits): string;
  export function replaySizeHint(chars: number): string;
  ```

- [ ] **Step 1: Write the failing tests**

Create `src/lib/assistant/__tests__/replay-preface.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { ConversationRecord, StoredMessage } from '@/lib/tauri-api';
import {
  buildReplayPreface,
  DEFAULT_REPLAY_LIMITS,
  REPLAY_CLOSE,
  REPLAY_DROPPED_MARKER,
  REPLAY_OPEN,
  REPLAY_PREFACE_HEADER,
  replaySizeHint,
} from '../replay-preface';

function record(messages: StoredMessage[]): ConversationRecord {
  return {
    id: '0b6f3f4e-0000-4000-8000-000000000001',
    title: 'Why 401?',
    agentConfigId: 'agent-1',
    mode: 'agent',
    workspaceId: 'ws-1',
    createdAt: '2026-10-11T09:12:00Z',
    updatedAt: '2026-10-11T09:20:41Z',
    collections: [],
    messages,
  };
}

describe('buildReplayPreface', () => {
  it('lists the conversation oldest first, one line per tool or proposal, without notices', () => {
    const preface = buildReplayPreface(
      record([
        { kind: 'user', text: 'Why 401?' },
        { kind: 'tool', title: 'get_request:\n  login', status: 'completed' },
        { kind: 'notice', text: 'Session ended.' },
        { kind: 'proposal', summary: 'Refresh  the token', status: 'rejected' },
        { kind: 'agent', text: 'The token expired.' },
      ]),
    );
    expect(preface).toBe(
      [
        REPLAY_PREFACE_HEADER,
        REPLAY_OPEN,
        'User: Why 401?',
        'Tool call: get_request: login (completed)',
        'Proposed change: Refresh the token (rejected)',
        'Assistant: The token expired.',
        REPLAY_CLOSE,
      ].join('\n'),
    );
  });

  it('keeps the newest messages that fit and marks the dropped ones', () => {
    const messages: StoredMessage[] = Array.from({ length: 50 }, (_, i) => ({
      kind: 'user',
      text: `message ${i} ${'x'.repeat(40)}`,
    }));
    const preface = buildReplayPreface(record(messages), { maxChars: 1_000 });
    expect(preface.length).toBeLessThanOrEqual(1_000);
    expect(preface).toContain(REPLAY_DROPPED_MARKER);
    expect(preface).not.toContain('message 0 ');
    const numbers = preface
      .split('\n')
      .filter((line) => line.startsWith('User: '))
      .map((line) => Number(line.split(' ')[2]));
    expect(numbers.at(-1)).toBe(49);
    expect(numbers).toEqual([...numbers].sort((a, b) => a - b));
  });

  it('adds no marker when everything fits', () => {
    expect(buildReplayPreface(record([{ kind: 'user', text: 'hi' }]))).not.toContain(
      REPLAY_DROPPED_MARKER,
    );
  });

  it('keeps the end of a newest message longer than the limit', () => {
    const preface = buildReplayPreface(
      record([{ kind: 'agent', text: `${'a'.repeat(5_000)}END` }]),
      { maxChars: 500 },
    );
    expect(preface.length).toBeLessThanOrEqual(500);
    expect(preface).toContain('…');
    expect(preface).toContain('END');
  });

  it('never includes tool results', () => {
    const withResult = {
      kind: 'tool',
      title: 'run_request: login',
      status: 'completed',
      result: 'SECRET-BODY',
    } as unknown as StoredMessage;
    const preface = buildReplayPreface(record([withResult]));
    expect(preface).toContain('Tool call: run_request: login (completed)');
    expect(preface).not.toContain('SECRET-BODY');
  });

  it('uses a 12,000 character limit by default', () => {
    expect(DEFAULT_REPLAY_LIMITS.maxChars).toBe(12_000);
    const messages: StoredMessage[] = Array.from({ length: 400 }, (_, i) => ({
      kind: 'agent',
      text: `reply ${i} ${'y'.repeat(100)}`,
    }));
    expect(buildReplayPreface(record(messages)).length).toBeLessThanOrEqual(12_000);
  });
});

describe('replaySizeHint', () => {
  it('names the size in characters', () => {
    expect(replaySizeHint(8412)).toBe(
      'Earlier conversation attached as context: 8,412 characters. It goes out with your first message.',
    );
  });
});
```

- [ ] **Step 2: Run them to see them fail**

Run: `yarn test src/lib/assistant/__tests__/replay-preface.test.ts`
Expected: FAIL with `Failed to resolve import "../replay-preface"`.

- [ ] **Step 3: Implement the preface**

Create `src/lib/assistant/replay-preface.ts`:

```ts
import type { ConversationRecord, StoredMessage } from '@/lib/tauri-api';

export interface ReplayLimits {
  /** Longest preface, in characters, header and frame included. */
  maxChars: number;
}

export const DEFAULT_REPLAY_LIMITS: ReplayLimits = { maxChars: 12_000 };

export const REPLAY_PREFACE_HEADER =
  'This is a previous conversation with the user, for context only. Do not repeat its actions.';

export const REPLAY_DROPPED_MARKER = '[Older messages of this conversation were left out.]';

export const REPLAY_OPEN = '<previous-conversation>';

export const REPLAY_CLOSE = '</previous-conversation>';

function oneLine(text: string): string {
  return text.replace(/\s+/g, ' ').trim();
}

/**
 * One stored message as a preface line. Tools and proposals become one-line
 * summaries built from their title or summary and status only, so a tool
 * result can never appear. Notices are left out.
 */
export function replayLine(message: StoredMessage): string | undefined {
  switch (message.kind) {
    case 'user':
      return `User: ${message.text}`;
    case 'agent':
      return `Assistant: ${message.text}`;
    case 'tool':
      return `Tool call: ${oneLine(message.title)} (${message.status})`;
    case 'proposal':
      return `Proposed change: ${oneLine(message.summary)} (${message.status})`;
    case 'notice':
      return undefined;
  }
}

function frame(lines: string[], dropped: boolean): string {
  return [
    REPLAY_PREFACE_HEADER,
    REPLAY_OPEN,
    ...(dropped ? [REPLAY_DROPPED_MARKER] : []),
    ...lines,
    REPLAY_CLOSE,
  ].join('\n');
}

/**
 * The context block put in front of the first prompt of a continued
 * conversation. Keeps the newest messages that fit in `limits.maxChars`
 * (the whole block counts), oldest first, with a marker when older ones
 * were left out. A newest message that alone is too long keeps its end.
 */
export function buildReplayPreface(
  record: ConversationRecord,
  limits: ReplayLimits = DEFAULT_REPLAY_LIMITS,
): string {
  const lines = record.messages
    .map(replayLine)
    .filter((line): line is string => line !== undefined);
  const kept: string[] = [];
  for (let i = lines.length - 1; i >= 0; i -= 1) {
    if (frame([lines[i], ...kept], i > 0).length > limits.maxChars) break;
    kept.unshift(lines[i]);
  }
  if (kept.length === 0 && lines.length > 0) {
    const newest = lines[lines.length - 1];
    const room = Math.max(0, limits.maxChars - frame([''], true).length - 1);
    kept.push(`…${newest.slice(Math.max(0, newest.length - room))}`);
  }
  return frame(kept, kept.length < lines.length);
}

/** The composer's note about an attached preface. */
export function replaySizeHint(chars: number): string {
  return `Earlier conversation attached as context: ${chars.toLocaleString('en-US')} characters. It goes out with your first message.`;
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `yarn test src/lib/assistant/__tests__/replay-preface.test.ts`
Expected: PASS, 7 tests.

- [ ] **Step 5: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.

Commit through the `dev-workflow-skills:1-git-commit` skill:

```bash
git add src/lib/assistant/replay-preface.ts src/lib/assistant/__tests__/replay-preface.test.ts
git commit --only -m "feat(assistant): build a bounded replay preface" -- src/lib/assistant/replay-preface.ts src/lib/assistant/__tests__/replay-preface.test.ts
```

---

### Task 2: Continue action

**Files:**
- Modify: `src/stores/assistant-store.ts` (`ReplaySeed` type; `replay` state; `beginSession` signature and body; `markReplaySent`; `reset`)
- Modify: `src/lib/assistant/assistant-session.ts` (`startAssistant` `:48-76`, `sendAssistantMessage` `:78-94`, new `continueConversation`)
- Modify: `src/lib/assistant/conversation-saver.ts` (`SaveSource` and the `buildConversationInput` call)
- Modify: `src/components/assistant/AssistantConversationView.tsx` (Continue footer, `onContinued` prop)
- Modify: `src/components/assistant/AssistantHistoryView.tsx` (`onContinued` prop passed through)
- Modify: `src/components/assistant/AssistantPanel.tsx` (`<AssistantHistoryView onContinued={() => setShowHistory(false)} />`)
- Test: `src/lib/assistant/__tests__/assistant-session.test.ts`, `src/lib/assistant/__tests__/conversation-saver.test.ts`, `src/components/assistant/__tests__/AssistantConversationView.test.tsx`

**Interfaces:**
- Consumes: Task 1 `buildReplayPreface`; Plan 02 `startConversationSaver`, `AssistantConversationView`, `AssistantHistoryView`.
- Produces (exact):
  ```ts
  // src/stores/assistant-store.ts
  export interface ReplaySeed { fromId: string; preface: string }
  interface AssistantState {
    replay?: ReplaySeed & { sent: boolean };
    beginSession: (agentConfigId: string, mode: AssistantMode, replay?: ReplaySeed) => number;
    markReplaySent: (sessionId: string) => void;
  }
  // src/lib/assistant/assistant-session.ts
  export async function startAssistant(agentConfigId: string, mode?: AssistantMode, replay?: ReplaySeed): Promise<void>;
  export async function continueConversation(record: ConversationRecord, agentConfigId: string): Promise<void>;
  // src/components/assistant/AssistantConversationView.tsx
  export const CONTINUE_HINT: string;
  export const MISSING_AGENT_TEXT: string;
  export function AssistantConversationView(props: { record: ConversationRecord; agentLabel: string;
    onBack: () => void; onContinued?: () => void }): JSX.Element;
  // src/components/assistant/AssistantHistoryView.tsx
  export function AssistantHistoryView(props: { onContinued?: () => void }): JSX.Element;
  ```

- [ ] **Step 1: Write the failing session and saver tests**

In `src/lib/assistant/__tests__/assistant-session.test.ts`, add `continueConversation` to the import from `'../assistant-session'`, add `import type { ConversationRecord } from '@/lib/tauri-api';` and `import { REPLAY_PREFACE_HEADER } from '../replay-preface';`, then add inside `describe('assistant session flows', ...)`:

```ts
  const stored: ConversationRecord = {
    id: '0b6f3f4e-0000-4000-8000-000000000001',
    title: 'Why 401?',
    agentConfigId: 'agent-1',
    mode: 'agent',
    workspaceId: 'ws-1',
    createdAt: '2026-10-11T09:12:00Z',
    updatedAt: '2026-10-11T09:20:41Z',
    collections: [],
    messages: [
      { kind: 'user', text: 'Why 401?' },
      { kind: 'agent', text: 'The token expired.' },
    ],
  };

  it('continues in Ask mode and sends the preface with the first message only', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's2', configOptions: [] });
    vi.mocked(api.sendAgentPrompt).mockResolvedValue('end_turn');
    activate('s1');
    await continueConversation(stored, 'agent-1');
    expect(api.endAgentSession).toHaveBeenCalledWith('s1');
    expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'ask', undefined);
    expect(store().session).toMatchObject({ sessionId: 's2', status: 'active', mode: 'ask' });
    expect(store().replay).toMatchObject({ fromId: stored.id, sent: false });

    await sendAssistantMessage('  And now?  ');
    const first = vi.mocked(api.sendAgentPrompt).mock.calls[0][1];
    expect(first.startsWith(REPLAY_PREFACE_HEADER)).toBe(true);
    expect(first).toContain('Assistant: The token expired.');
    expect(first.endsWith('\n\nAnd now?')).toBe(true);
    expect(store().messages[0]).toMatchObject({ kind: 'user', text: 'And now?' });
    expect(store().replay?.sent).toBe(true);

    store().completeMessage('s2');
    await sendAssistantMessage('Next');
    expect(vi.mocked(api.sendAgentPrompt).mock.calls[1][1]).toBe('Next');
  });

  it('keeps the preface when the first send fails', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's2', configOptions: [] });
    await continueConversation(stored, 'agent-1');
    vi.mocked(api.sendAgentPrompt).mockRejectedValueOnce('agent busy');
    await sendAssistantMessage('First try');
    expect(store().replay?.sent).toBe(false);
    vi.mocked(api.sendAgentPrompt).mockResolvedValueOnce('end_turn');
    await sendAssistantMessage('Second try');
    expect(
      vi.mocked(api.sendAgentPrompt).mock.calls[1][1].startsWith(REPLAY_PREFACE_HEADER),
    ).toBe(true);
  });
```

In `src/lib/assistant/__tests__/conversation-saver.test.ts`, add inside `describe('startConversationSaver', ...)`:

```ts
  it('records the conversation a continued session came from', async () => {
    const token = store().beginSession('agent-1', 'ask', { fromId: 'c0', preface: 'P' });
    store().activateSession(token, 's1', []);
    turn('Why 401?', 'Hello');
    await vi.advanceTimersByTimeAsync(CONVERSATION_SAVE_DEBOUNCE_MS);
    expect(save.mock.calls[0][1]).toMatchObject({
      continuedFrom: 'c0',
      mode: 'ask',
      messages: [
        { kind: 'user', text: 'Why 401?' },
        { kind: 'agent', text: 'Hello' },
      ],
    });
  });
```

- [ ] **Step 2: Write the failing Continue view tests**

Create `src/components/assistant/__tests__/AssistantConversationView.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { continueConversation } from '@/lib/assistant/assistant-session';
import type { ConversationRecord } from '@/lib/tauri-api';
import {
  AssistantConversationView,
  CONTINUE_HINT,
  MISSING_AGENT_TEXT,
} from '../AssistantConversationView';

// Radix Select calls pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

vi.mock('@/lib/assistant/assistant-session', () => ({ continueConversation: vi.fn() }));
vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [{ id: 'agent-1', label: 'Claude' }] }),
}));
vi.mock('@/components/collections/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ children }: { children: string }) => <div>{children}</div>,
}));

const record: ConversationRecord = {
  id: '0b6f3f4e-0000-4000-8000-000000000001',
  title: 'Why 401?',
  agentConfigId: 'agent-1',
  mode: 'agent',
  workspaceId: 'ws-1',
  createdAt: '2026-10-11T09:12:00Z',
  updatedAt: '2026-10-11T09:20:41Z',
  collections: [],
  messages: [
    { kind: 'user', text: 'Why 401?' },
    { kind: 'agent', text: 'The token expired.' },
  ],
};

describe('AssistantConversationView Continue', () => {
  beforeEach(() => vi.clearAllMocks());

  it('continues with the stored agent and leaves the history', async () => {
    const onContinued = vi.fn();
    render(
      <AssistantConversationView
        record={record}
        agentLabel='Claude'
        onBack={vi.fn()}
        onContinued={onContinued}
      />,
    );
    expect(screen.getByText(CONTINUE_HINT)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    expect(onContinued).toHaveBeenCalledTimes(1);
    expect(continueConversation).toHaveBeenCalledWith(record, 'agent-1');
  });

  it('asks for an agent when the stored one is gone', () => {
    render(
      <AssistantConversationView
        record={{ ...record, agentConfigId: 'deleted-agent' }}
        agentLabel='deleted-agent'
        onBack={vi.fn()}
        onContinued={vi.fn()}
      />,
    );
    expect(screen.getByText(MISSING_AGENT_TEXT)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled();
  });

  it('has no Continue without onContinued', () => {
    render(<AssistantConversationView record={record} agentLabel='Claude' onBack={vi.fn()} />);
    expect(screen.queryByRole('button', { name: 'Continue' })).toBeNull();
  });
});
```

- [ ] **Step 3: Run them to see them fail**

Run: `yarn test src/lib/assistant/__tests__/assistant-session.test.ts src/lib/assistant/__tests__/conversation-saver.test.ts src/components/assistant/__tests__/AssistantConversationView.test.tsx`
Expected: FAIL with `does not provide an export named 'continueConversation'`, a missing `continuedFrom` in the saver assertion, and `does not provide an export named 'CONTINUE_HINT'`.

- [ ] **Step 4: Seed the store**

In `src/stores/assistant-store.ts`:
- After `AssistantFocus`, add:

```ts
/** A stored conversation carried into a new session (spec section 7, stage 2). */
export interface ReplaySeed {
  /** The id of the stored conversation, saved as `continuedFrom`. */
  fromId: string;
  /** The text put in front of the session's first prompt. */
  preface: string;
}
```

- In `AssistantState`, after `conversationId?: string;`, add:

```ts
  /** The seed of a continued session. `sent` turns true once the first prompt carried it. */
  replay?: ReplaySeed & { sent: boolean };
```

  change `beginSession: (agentConfigId: string, mode: AssistantMode) => number;` to `beginSession: (agentConfigId: string, mode: AssistantMode, replay?: ReplaySeed) => number;`, and after `setMode` add:

```ts
  /** Records that the first prompt of this session carried the replay preface. */
  markReplaySent: (sessionId: string) => void;
```

- In the initial state add `replay: undefined,`. In `beginSession(agentConfigId, mode, replay)` add `replay: replay ? { ...replay, sent: false } : undefined,` to its `set({...})`. In `reset` add `replay: undefined,`.
- After `setMode(...) {...},` add:

```ts
  markReplaySent(sessionId) {
    set((state) =>
      state.replay && isCurrent(state, sessionId)
        ? { replay: { ...state.replay, sent: true } }
        : state,
    );
  },
```

- [ ] **Step 5: Continue and send with the preface**

In `src/lib/assistant/assistant-session.ts`:
- Add imports: `import { buildReplayPreface } from '@/lib/assistant/replay-preface';`, `type ConversationRecord` to the `@/lib/tauri-api` import, and `type ReplaySeed` to the `@/stores/assistant-store` import.
- Change `startAssistant`'s signature and its `beginSession` call:

```ts
export async function startAssistant(
  agentConfigId: string,
  mode: AssistantMode = DEFAULT_ASSISTANT_MODE,
  replay?: ReplaySeed,
): Promise<void> {
  // (unchanged lines)
  const token = useAssistantStore.getState().beginSession(agentConfigId, mode, replay);
```

- Replace the body of `sendAssistantMessage` after the `if (!trimmed || session?.status !== 'active') return;` line with:

```ts
  // A continued session sends its preface once, with the first message that goes out.
  const replay = store.replay && !store.replay.sent ? store.replay : undefined;
  // The store refuses a second turn while one runs, so a double send stops here.
  // The chat, and so the saved conversation, keeps only what the user typed.
  if (!store.appendUserMessage(trimmed)) return;
  const prompt = replay ? `${replay.preface}\n\n${trimmed}` : trimmed;
  try {
    await sendAgentPrompt(session.sessionId, prompt, resources);
    if (replay) useAssistantStore.getState().markReplaySent(session.sessionId);
  } catch (err) {
    // A rejected prompt fails the turn only. A dead session arrives as its own event.
    useAssistantStore.getState().failMessage(session.sessionId, String(err), false);
  }
```

- Append:

```ts
/**
 * Starts a new session that carries a stored conversation as context. It
 * always starts in Ask mode, so a stored Agent mode is never re-applied. A
 * live session ends first, which saves it.
 */
export async function continueConversation(
  record: ConversationRecord,
  agentConfigId: string,
): Promise<void> {
  const current = useAssistantStore.getState().session;
  if (current?.status === 'active' || current?.status === 'starting') {
    await endAssistantSession();
  }
  await startAssistant(agentConfigId, 'ask', {
    fromId: record.id,
    preface: buildReplayPreface(record),
  });
}
```

In `src/lib/assistant/conversation-saver.ts`, add `'replay'` to the `SaveSource` `Pick<...>` keys and `continuedFrom: source.replay?.fromId,` to the `buildConversationInput({...})` argument.

- [ ] **Step 6: Add the Continue footer**

In `src/components/assistant/AssistantConversationView.tsx`:
- Add imports:

```tsx
import { useState } from 'react';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { continueConversation } from '@/lib/assistant/assistant-session';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
```

- Add before `AssistantConversationView`:

```tsx
export const CONTINUE_HINT =
  'Continue starts a new session in Ask mode with this conversation as context. The stored conversation is not changed.';

export const MISSING_AGENT_TEXT =
  'The agent of this conversation is no longer configured. Pick an agent to continue.';

/** Agent picker and Continue. Preselects the stored agent when it still exists. */
function ContinueConversation({
  record,
  onContinued,
}: {
  record: ConversationRecord;
  onContinued: () => void;
}) {
  const { data: agentConfigs = [] } = useAgentConfigs();
  const storedAgentExists = agentConfigs.some((c) => c.id === record.agentConfigId);
  const [picked, setPicked] = useState('');
  const agentConfigId = picked || (storedAgentExists ? record.agentConfigId : '');

  const handleContinue = () => {
    if (!agentConfigId) return;
    onContinued();
    // A failed start shows its error in the Start view, like any start.
    void continueConversation(record, agentConfigId);
  };

  return (
    <div className='flex shrink-0 flex-col gap-2 border-t p-3'>
      {!storedAgentExists && <p className='text-xs text-muted-foreground'>{MISSING_AGENT_TEXT}</p>}
      <Select value={agentConfigId} onValueChange={setPicked}>
        <SelectTrigger className='h-8 text-sm' aria-label='Agent'>
          <SelectValue placeholder='Pick an agent' />
        </SelectTrigger>
        <SelectContent>
          {agentConfigs.map((c) => (
            <SelectItem key={c.id} value={c.id}>
              {c.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button size='sm' disabled={!agentConfigId} onClick={handleContinue}>
        Continue
      </Button>
      <p className='text-xs text-muted-foreground'>{CONTINUE_HINT}</p>
    </div>
  );
}
```

- Add `onContinued?: () => void` to the props of `AssistantConversationView` (destructure it with the others) and, after the closing `</ScrollArea>`, add:

```tsx
      {onContinued && <ContinueConversation record={record} onContinued={onContinued} />}
```

In `src/components/assistant/AssistantHistoryView.tsx`, change the signature to `export function AssistantHistoryView({ onContinued }: { onContinued?: () => void }) {` and pass `onContinued={onContinued}` to `<AssistantConversationView ... />`. The Plan 02 tests render `<AssistantHistoryView />`, which stays valid because the prop is optional.

In `src/components/assistant/AssistantPanel.tsx`, render `<AssistantHistoryView onContinued={() => setShowHistory(false)} />`.

- [ ] **Step 7: Run the tests to see them pass**

Run: `yarn test src/lib/assistant/__tests__/assistant-session.test.ts src/lib/assistant/__tests__/conversation-saver.test.ts src/components/assistant/__tests__/AssistantConversationView.test.tsx src/components/assistant/__tests__/AssistantHistoryView.test.tsx src/stores/__tests__/assistant-store.test.ts`
Expected: PASS (2 new session tests, 1 new saver test, 3 view tests, earlier tests unchanged).

- [ ] **Step 8: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.

Commit through the `dev-workflow-skills:1-git-commit` skill:

```bash
git add src/stores/assistant-store.ts src/lib/assistant/assistant-session.ts src/lib/assistant/conversation-saver.ts src/components/assistant/AssistantConversationView.tsx src/components/assistant/AssistantHistoryView.tsx src/components/assistant/AssistantPanel.tsx src/lib/assistant/__tests__/assistant-session.test.ts src/lib/assistant/__tests__/conversation-saver.test.ts src/components/assistant/__tests__/AssistantConversationView.test.tsx
git commit --only -m "feat(assistant): continue a stored conversation in a new session" -- src/stores/assistant-store.ts src/lib/assistant/assistant-session.ts src/lib/assistant/conversation-saver.ts src/components/assistant/AssistantConversationView.tsx src/components/assistant/AssistantHistoryView.tsx src/components/assistant/AssistantPanel.tsx src/lib/assistant/__tests__/assistant-session.test.ts src/lib/assistant/__tests__/conversation-saver.test.ts src/components/assistant/__tests__/AssistantConversationView.test.tsx
```

---

### Task 3: Composer size hint and `continuedFrom` in the list

**Files:**
- Modify: `src/components/assistant/composer/Composer.tsx` (selector near `:43-53`; hint above `<ComposerChips ... />` `:214`)
- Modify: `src/components/assistant/AssistantHistoryView.tsx` (list item, under the meta line)
- Modify: `docs/features/assistant-conversations.md` (add a "Continue" section)
- Modify: `docs/features/assistant-composer.md` (one line under "Toolbar")
- Test: `src/components/assistant/composer/__tests__/Composer.test.tsx`, `src/components/assistant/__tests__/AssistantHistoryView.test.tsx`

**Interfaces:**
- Consumes: Task 1 `replaySizeHint`; Task 2 `replay` state and `markReplaySent`; `ConversationSummary.continuedFrom`.
- Produces: no new exports. The composer shows `replaySizeHint(preface.length)` while `replay.sent` is false; a History row with `continuedFrom` shows `Continued from "<title>"`, or `Continued from an earlier conversation` when that id is not in the list.

- [ ] **Step 1: Write the failing tests**

In `src/components/assistant/composer/__tests__/Composer.test.tsx`, add `import { replaySizeHint } from '@/lib/assistant/replay-preface';` and inside the top-level `describe`:

```tsx
  it('shows the size of an attached earlier conversation until the first message goes out', () => {
    const token = useAssistantStore
      .getState()
      .beginSession('agent-1', 'ask', { fromId: 'c0', preface: 'x'.repeat(8412) });
    useAssistantStore.getState().activateSession(token, 's1', []);
    renderComposer();
    expect(screen.getByText(replaySizeHint(8412))).toBeInTheDocument();
    act(() => useAssistantStore.getState().markReplaySent('s1'));
    expect(screen.queryByText(replaySizeHint(8412))).toBeNull();
  });
```

In `src/components/assistant/__tests__/AssistantHistoryView.test.tsx`, add inside `describe('AssistantHistoryView', ...)`:

```tsx
  it('names the conversation a continued one came from', async () => {
    vi.mocked(api.listConversations).mockResolvedValue([
      { ...summary, id: 'c2', title: 'Follow-up', continuedFrom: 'c1' },
      { ...summary, id: 'c3', title: 'Older follow-up', continuedFrom: 'gone' },
      summary,
    ]);
    render(<AssistantHistoryView />);
    expect(
      await screen.findByText('Continued from "Why does login return 401?"'),
    ).toBeInTheDocument();
    expect(screen.getByText('Continued from an earlier conversation')).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run them to see them fail**

Run: `yarn test src/components/assistant/composer/__tests__/Composer.test.tsx src/components/assistant/__tests__/AssistantHistoryView.test.tsx`
Expected: FAIL with `Unable to find an element with the text: Earlier conversation attached as context: 8,412 characters...` and `Unable to find an element with the text: Continued from "Why does login return 401?"`.

- [ ] **Step 3: Show the size hint**

In `src/components/assistant/composer/Composer.tsx`:
- Add `import { replaySizeHint } from '@/lib/assistant/replay-preface';`.
- After `const workspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);` add:

```tsx
  // The preface of a continued session goes out with the first prompt, so its size is shown.
  const replayChars = useAssistantStore((s) =>
    s.replay && !s.replay.sent ? s.replay.preface.length : 0,
  );
```

- Before `<ComposerChips chips={chips} onRemove={handleRemoveChip} />` add:

```tsx
      {replayChars > 0 && (
        <p className='px-3 pt-2 text-xs text-muted-foreground'>{replaySizeHint(replayChars)}</p>
      )}
```

- [ ] **Step 4: Show `continuedFrom` in the list**

In `src/components/assistant/AssistantHistoryView.tsx`, next to `agentLabel`, add:

```tsx
  const continuedFromLabel = (id: string) => {
    const from = items.find((i) => i.id === id);
    return from
      ? `Continued from "${from.title || UNTITLED}"`
      : 'Continued from an earlier conversation';
  };
```

and, after the meta `<span>` inside the open `Button` of each list item, add:

```tsx
                {item.continuedFrom && (
                  <span className='text-xs text-muted-foreground'>
                    {continuedFromLabel(item.continuedFrom)}
                  </span>
                )}
```

- [ ] **Step 5: Run the tests to see them pass**

Run: `yarn test src/components/assistant/composer/__tests__/Composer.test.tsx src/components/assistant/__tests__/AssistantHistoryView.test.tsx`
Expected: PASS (1 new composer test, 1 new History view test, earlier tests unchanged).

- [ ] **Step 6: Document Continue**

Append to `docs/features/assistant-conversations.md`:

```markdown
## Continue (stage 2)

- Continue (read-only view footer) calls `continueConversation(record, agentConfigId)`: a live session ends first (and saves), then `startAssistant(agentConfigId, 'ask', { fromId, preface })`. Always Ask mode. A deleted agent config leaves the picker empty and Continue disabled.
- `buildReplayPreface` (`src/lib/assistant/replay-preface.ts`): header, `<previous-conversation>` frame, newest messages that fit in 12,000 characters (whole block), oldest first, a marker when older ones were left out, the end of an oversized newest message. Tools and proposals are one-line title or summary plus status; notices and tool results never appear.
- `sendAssistantMessage` puts the preface in front of the first prompt only (`replay.sent`, set after a successful send). The chat and the saved conversation keep only the typed text, so prefaces never nest. The new conversation saves with `continued_from`; the old one is not changed. There is no backend limit on prompt text; the composer only shows the size.
```

In `docs/features/assistant-composer.md`, under "Toolbar", add:

```markdown
- A continued session shows "Earlier conversation attached as context: N characters" above the chips until the first message goes out (`replaySizeHint`).
```

- [ ] **Step 7: Check and commit**

Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.

Commit through the `dev-workflow-skills:1-git-commit` skill:

```bash
git add src/components/assistant/composer/Composer.tsx src/components/assistant/AssistantHistoryView.tsx src/components/assistant/composer/__tests__/Composer.test.tsx src/components/assistant/__tests__/AssistantHistoryView.test.tsx docs/features/assistant-conversations.md docs/features/assistant-composer.md
git commit --only -m "feat(assistant): show replay size and continued-from in history" -- src/components/assistant/composer/Composer.tsx src/components/assistant/AssistantHistoryView.tsx src/components/assistant/composer/__tests__/Composer.test.tsx src/components/assistant/__tests__/AssistantHistoryView.test.tsx docs/features/assistant-conversations.md docs/features/assistant-composer.md
```

---

## Post-Implementation Review

A reviewer (Sonnet) reads the diff against the Review Focus list, with extra attention on `sendAssistantMessage` (preface once, retry after a failure, typed text only in the chat) and on `continueConversation` (Ask mode, live session ended first). Then a final whole-branch review (Opus) covers Plans 01 to 03 before merge. Manual check: restart the app, open History, Continue a conversation, confirm the size hint, the Ask mode and that the new conversation lists "Continued from".
