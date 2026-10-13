# ACP Multi-Agent Support, Plan 4: Reasoning Visibility and Liveness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user see the agent's reasoning, and stop a long reasoning phase from killing the session.

**Architecture:** Today Rocket drops the agent's thought chunks in `rocket-infra`, so thinking never reaches the UI and never restarts the 120-second idle timer. This plan forwards thought chunks as a new `AcpUpdate::Thought` and a `DomainEvent::AcpThoughtChunk`, shows them in a collapsed "Reasoning" block, adds a "still working" notice for turns that send nothing, and makes the idle limit configurable per agent through a new `AgentConfig.tuning` struct (Plan 5 adds more fields to it).

**Tech Stack:** Rust (rocket-acp, rocket-shared, rocket-infra, rocket-app, src-tauri), React + TypeScript, Zustand, Vitest, Biome.

**Spec:** `.claude/acp-multi-agent-support.md`; the reasoning investigation of 2026-10-12 is summarised in `docs/superpowers/plans/2026-10-12-acp-multi-agent-index.md`. Depends on Plans 1 to 3 being merged.

## Global Constraints

- Run cargo with `-j4`. Never `cargo test --workspace` or `--all`.
- Never call unwrap in production paths (a project hook blocks it in Rust edits). `expect("...")` is fine in tests.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs, never on `AgentConfig` or `AgentTuning` (they persist as snake_case).
- Persistence stays backward compatible: every new `AgentConfig` field has `#[serde(default)]`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- UI uses shadcn/ui primitives only (`Collapsible`, `Button`, `Input`, `Label`), lucide-react icons only, no raw `<button>`/`<input>`.
- Never `git add -A`, `git add .` or `--all`. Explicit paths. Commit through the `dev-workflow-skills:1-git-commit` skill (conventional commits).
- Thought text is shown to the user only. It is not saved in stored conversations and not sent back to any agent.
- No `OpenCollection` read needed: this plan does not touch collection or environment YAML.

## Review Focus

- A turn that only thinks (no text, no tool call) must not be killed while thought chunks keep arriving, and a truly silent turn must still be killed after the idle limit (Task 9 tests).
- A very long reasoning stream must not grow the UI state without bound (Task 10 cap test).
- Reasoning from an old session or an old turn must never show in a new session or turn (Task 10 reset tests).
- A hand-edited `agent_configs.yml` with `idle_timeout_secs: 5` or `99999` must be refused, not applied (Task 11 tests).
- An agent config saved before this plan (no `tuning` key) must load with the 120-second default (Task 11 test).

---

### Task 9: Forward thought chunks through the backend

**Files:**
- Modify: `crates/rocket-acp/src/update.rs`
- Modify: `crates/rocket-infra/src/acp_agent_client.rs` (`session_update_to_acp` at line 740-773 and its tests near line 940-956)
- Modify: `crates/rocket-shared/src/events.rs` (variant after `AcpSessionChunk` at line 536, wire-shape test near line 1639)
- Modify: `src-tauri/src/tauri_event_bus.rs` (`channel_for` at line 31)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (`publish_update` at line 299)
- Test: the same files

**Interfaces:**
- Consumes: nothing from earlier plans.
- Produces:
  - `AcpUpdate::Thought { text: String }`.
  - `DomainEvent::AcpThoughtChunk { session_id: String, text: String }`, wire `{"type":"acpThoughtChunk","session_id":...,"text":...}`.
  - Frontend channel name `agent-session-thought`.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-acp/src/update.rs` add inside `tests`:

```rust
    #[test]
    fn thought_updates_compare_by_value() {
        assert_eq!(
            AcpUpdate::Thought {
                text: "a".to_string()
            },
            AcpUpdate::Thought {
                text: "a".to_string()
            }
        );
        assert_ne!(
            AcpUpdate::Thought {
                text: "a".to_string()
            },
            AcpUpdate::Text {
                text: "a".to_string()
            }
        );
    }
```

In `crates/rocket-infra/src/acp_agent_client.rs`, in the existing test `non_text_chunks_and_unmodelled_updates_are_dropped` delete the `AgentThoughtChunk` assertion (the block that asserts `session_update_to_acp(SessionUpdate::AgentThoughtChunk(text_chunk("thinking")))` is `None`), then add next to it:

```rust
    #[test]
    fn thought_chunks_become_thought_updates_and_non_text_thoughts_are_dropped() {
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentThoughtChunk(text_chunk("checking auth"))),
            Some(AcpUpdate::Thought {
                text: "checking auth".to_string()
            })
        );
        let image = ContentChunk::new(ContentBlock::Image(ImageContent::new("aGk=", "image/png")));
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentThoughtChunk(image)),
            None
        );
    }
```

In `crates/rocket-shared/src/events.rs` add inside `tests`, after `acp_session_chunk_wire_shape`:

```rust
    #[test]
    fn acp_thought_chunk_wire_shape() {
        let event = DomainEvent::AcpThoughtChunk {
            session_id: "sess-1".into(),
            text: "Checking the auth header. ".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpThoughtChunk","session_id":"sess-1","text":"Checking the auth header. "}"#
        );
    }
```

In `crates/rocket-app/src/acp_session_service.rs`, inside `tests` next to `send_prompt_publishes_every_update_kind_in_order_before_finished`, add:

```rust
    #[tokio::test]
    async fn thought_updates_are_published_in_order_before_the_reply_and_the_finish() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_updates: vec![
                AcpUpdate::Thought {
                    text: "checking auth ".to_string(),
                },
                AcpUpdate::Thought {
                    text: "then docs".to_string(),
                },
                AcpUpdate::Text {
                    text: "answer".to_string(),
                },
            ],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        service
            .send_prompt("session-1", hi())
            .await
            .expect("send_prompt");

        let events = publisher.events.lock().expect("lock");
        let thoughts: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::AcpThoughtChunk { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(thoughts, vec!["checking auth ", "then docs"]);
        let last_thought = events
            .iter()
            .rposition(|e| matches!(e, DomainEvent::AcpThoughtChunk { .. }))
            .expect("a thought event");
        let first_text = events
            .iter()
            .position(|e| matches!(e, DomainEvent::AcpSessionChunk { .. }))
            .expect("a text event");
        assert!(last_thought < first_text);
        assert!(matches!(
            events.last(),
            Some(DomainEvent::AcpSessionFinished { .. })
        ));
    }

    #[tokio::test]
    async fn a_turn_that_only_thinks_is_not_cut_off_while_thoughts_keep_arriving() {
        let publisher = Arc::new(FakeEventPublisher::new());
        // Six thoughts, 40 ms apart: 240 ms in total, longer than the 150 ms
        // idle limit, but no single gap reaches it.
        let client = FakeSessionClient {
            prompt_updates: (0..6)
                .map(|i| AcpUpdate::Thought {
                    text: format!("step {i} "),
                })
                .collect(),
            update_interval: Duration::from_millis(40),
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_millis(150));

        let stop = service
            .send_prompt("session-1", hi())
            .await
            .expect("a thinking turn must keep its session");

        assert_eq!(stop, "end_turn");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p rocket-acp -j4 thought_updates && cargo test -p rocket-shared -j4 acp_thought_chunk`
Expected: FAIL to compile (`no variant Thought`, `no variant AcpThoughtChunk`).

- [ ] **Step 3: Implement the update, the event, the mapping and the channel**

In `crates/rocket-acp/src/update.rs` add to `AcpUpdate` after `Text`:

```rust
    /// A chunk of the agent's reasoning text. It is shown apart from the
    /// reply, and like any update it restarts the idle limit.
    Thought { text: String },
```

In `crates/rocket-shared/src/events.rs` add after the `AcpSessionChunk` variant:

```rust
    /// Emitted per streamed reasoning chunk while a prompt is being answered.
    AcpThoughtChunk {
        session_id: String,
        text: String,
    },
```

In `src-tauri/src/tauri_event_bus.rs` add after the `AcpSessionChunk` line:

```rust
        DomainEvent::AcpThoughtChunk { .. } => "agent-session-thought",
```

In `acp_agent_client.rs` update the doc comment of `session_update_to_acp` to "Kinds Rocket does not model (user echoes, plans, modes, commands, session info) and non-text message chunks yield `None` and are dropped." and add this arm after the `AgentMessageChunk` arm:

```rust
        SessionUpdate::AgentThoughtChunk(chunk) => match chunk.content {
            ContentBlock::Text(text) => Some(AcpUpdate::Thought { text: text.text }),
            _ => None,
        },
```

In `acp_session_service.rs` `publish_update` add after the `AcpUpdate::Text` arm:

```rust
            AcpUpdate::Thought { text } => DomainEvent::AcpThoughtChunk { session_id, text },
```

- [ ] **Step 4: Run the focused tests**

Run: `cargo test -p rocket-acp -j4 && cargo test -p rocket-shared -j4 acp_ && cargo test -p rocket-infra -j4 acp_agent_client && cargo test -p rocket-app -j4 acp_session_service`
Expected: PASS. If a test names `AcpUpdate` in an exhaustive `match` elsewhere, the compiler points to it: add a `Thought` arm there with the same handling as `Text` unless it is a test double that ignores updates.

- [ ] **Step 5: Check the whole change compiles**

Run: `cargo check -j4`
Expected: no errors (the exhaustive `channel_for` match is satisfied).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-acp/src/update.rs crates/rocket-infra/src/acp_agent_client.rs \
  crates/rocket-shared/src/events.rs src-tauri/src/tauri_event_bus.rs \
  crates/rocket-app/src/acp_session_service.rs
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): forward agent thought chunks and count them as activity`.

---

### Task 10: Show the reasoning, and tell the user when the agent is quiet

**Files:**
- Modify: `src/lib/tauri-api.ts` (after `onAgentSessionChunk` at line 2783)
- Modify: `src/stores/assistant-store.ts` (state, actions, `reset`)
- Modify: `src/lib/assistant-event-bridge.ts` (registrations at line 74-105)
- Create: `src/components/assistant/ReasoningBlock.tsx`
- Create: `src/components/assistant/WaitingNotice.tsx`
- Modify: `src/components/assistant/AssistantChatView.tsx` (render both below the timeline)
- Test: `src/lib/queries/__tests__/agent-session-api.test.ts`, `src/stores/__tests__/assistant-store.test.ts`, `src/lib/__tests__/assistant-event-bridge.test.ts`, `src/components/assistant/__tests__/ReasoningBlock.test.tsx`, `src/components/assistant/__tests__/WaitingNotice.test.tsx`

**Interfaces:**
- Consumes: the `agent-session-thought` event (Task 9); `selectTurnRunning` and `isCurrent` in the store.
- Produces:
  - `onAgentSessionThought(handler)` and `AgentSessionThoughtEvent` in `tauri-api.ts`.
  - Store state `reasoning: string` (the current turn's reasoning) and `lastActivityAt: number | undefined`.
  - Store actions `appendReasoning(sessionId, text)` and `touchActivity(sessionId)`.
  - `MAX_REASONING_CHARS = 20_000` exported from the store.
  - Components `ReasoningBlock` and `WaitingNotice` (no props).

- [ ] **Step 1: Write the failing API test**

Add inside the `describe` of `src/lib/queries/__tests__/agent-session-api.test.ts` after the chunk test:

```ts
  it('onAgentSessionThought subscribes to agent-session-thought and unwraps the payload', async () => {
    const payload = { type: 'acpThoughtChunk', session_id: 'session-1', text: 'checking auth' };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onAgentSessionThought } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onAgentSessionThought(handler);
    expect(listen).toHaveBeenCalledWith('agent-session-thought', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });
```

- [ ] **Step 2: Write the failing store tests**

Add inside the `describe('assistant-store', ...)` of `src/stores/__tests__/assistant-store.test.ts` (and add `MAX_REASONING_CHARS` to the import from `../assistant-store`):

```ts
  it('collects the reasoning of the current turn and clears it at the next user message', () => {
    activate();
    store().appendUserMessage('why does login fail?');
    store().appendReasoning('s1', 'Checking the auth header. ');
    store().appendReasoning('s1', 'The token variable is empty.');
    expect(store().reasoning).toBe('Checking the auth header. The token variable is empty.');
    store().completeMessage('s1');
    expect(store().reasoning).not.toBe('');
    store().appendUserMessage('and now?');
    expect(store().reasoning).toBe('');
  });

  it('ignores reasoning for another session and empty chunks', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendReasoning('other', 'secret thoughts');
    store().appendReasoning('s1', '');
    expect(store().reasoning).toBe('');
  });

  it('keeps only the newest part of a very long reasoning stream', () => {
    activate();
    store().appendUserMessage('think hard');
    store().appendReasoning('s1', 'a'.repeat(MAX_REASONING_CHARS));
    store().appendReasoning('s1', 'END');
    const text = store().reasoning;
    expect(text.length).toBeLessThanOrEqual(MAX_REASONING_CHARS + 1);
    expect(text.startsWith('…')).toBe(true);
    expect(text.endsWith('END')).toBe(true);
  });

  it('clears the reasoning on reset and when a new session starts', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendReasoning('s1', 'thinking');
    store().reset();
    expect(store().reasoning).toBe('');
    activate('s2');
    expect(store().reasoning).toBe('');
  });

  it('records the time of the last activity of the active session only', () => {
    activate();
    store().appendUserMessage('hi');
    const started = store().lastActivityAt;
    expect(started).toBeTypeOf('number');
    useAssistantStore.setState({ lastActivityAt: 1 });
    store().touchActivity('other');
    expect(store().lastActivityAt).toBe(1);
    store().touchActivity('s1');
    expect(store().lastActivityAt).toBeGreaterThan(1);
  });
```

- [ ] **Step 3: Run them to verify they fail**

Run: `yarn test assistant-store agent-session-api`
Expected: FAIL (`appendReasoning is not a function`, no `onAgentSessionThought` export).

- [ ] **Step 4: Implement the API binding**

In `src/lib/tauri-api.ts` add after `onAgentSessionChunk`:

```ts
export interface AgentSessionThoughtEvent {
  type: 'acpThoughtChunk';
  session_id: string;
  text: string;
}

export const onAgentSessionThought = (
  handler: (event: AgentSessionThoughtEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionThoughtEvent>('agent-session-thought', (e) => handler(e.payload));
```

- [ ] **Step 5: Implement the store changes**

In `src/stores/assistant-store.ts`:

1. Export the cap above the interfaces: `export const MAX_REASONING_CHARS = 20_000;`
2. In `interface AssistantState` add after `usage?: AssistantUsage;`:

```ts
  /** The agent's reasoning for the current turn. Cleared at the next user message. */
  reasoning: string;
  /** When the active session last sent anything (text, thought, tool or usage). */
  lastActivityAt?: number;
```

   and after `appendChunk: ...;` add:

```ts
  /** Adds reasoning text. Keeps only the newest MAX_REASONING_CHARS characters. */
  appendReasoning: (sessionId: string, text: string) => void;
  /** Marks that the session sent something, for the quiet-agent notice. */
  touchActivity: (sessionId: string) => void;
```

3. In the initial state object add `reasoning: '',` next to the other initial values (for example after `messages: [],`), and add `reasoning: '',` and `lastActivityAt: undefined,` to the object that `reset()` sets.
4. In `appendUserMessage` add `reasoning: '', lastActivityAt: Date.now(),` to the object passed to `set` (next to `conversationId`).
5. In `activateSession` add `reasoning: ''` to the object it sets for the activated session (find the `set(` call that stores `status: 'active'`).
6. Add the two actions after `appendChunk`:

```ts
  appendReasoning(sessionId, text) {
    set((state) => {
      if (!text || !isCurrent(state, sessionId)) return state;
      const next = state.reasoning + text;
      const kept =
        next.length > MAX_REASONING_CHARS ? `…${next.slice(next.length - MAX_REASONING_CHARS)}` : next;
      return { reasoning: kept, lastActivityAt: Date.now() };
    });
  },

  touchActivity(sessionId) {
    set((state) => (isCurrent(state, sessionId) ? { lastActivityAt: Date.now() } : state));
  },
```

- [ ] **Step 6: Run the store and API tests**

Run: `yarn test assistant-store agent-session-api`
Expected: PASS.

- [ ] **Step 7: Write the failing bridge test**

In `src/lib/__tests__/assistant-event-bridge.test.ts`: in `mocks.api` add `onAgentSessionThought: listener('thought'),` after the chunk line, and add the test:

```ts
  it('shows thought chunks as reasoning and counts every agent event as activity', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    useAssistantStore.setState({ lastActivityAt: 1 });
    emit('thought', { session_id: 's1', text: 'Checking the folder auth. ' });
    expect(store().reasoning).toBe('Checking the folder auth. ');
    expect(store().lastActivityAt).toBeGreaterThan(1);

    useAssistantStore.setState({ lastActivityAt: 1 });
    emit('toolActivity', { session_id: 's1', call_id: 'c1', title: 'Read', status: 'in_progress' });
    expect(store().lastActivityAt).toBeGreaterThan(1);

    useAssistantStore.setState({ lastActivityAt: 1 });
    emit('chunk', { session_id: 's1', text: 'Hello' });
    expect(store().lastActivityAt).toBeGreaterThan(1);
  });
```

- [ ] **Step 8: Run it to verify it fails**

Run: `yarn test assistant-event-bridge`
Expected: FAIL (`no thought listener`).

- [ ] **Step 9: Wire the bridge**

In `src/lib/assistant-event-bridge.ts` add `onAgentSessionThought,` to the `@/lib/tauri-api` import (alphabetical, after `onAgentSessionFinished`). Inside the effect, after the `guard` helper add:

```ts
    // Every event that proves the agent is alive also refreshes the activity time.
    const alive =
      <T extends { session_id: string }>(handler: (event: T) => void) =>
      (event: T) => {
        store().touchActivity(event.session_id);
        handler(event);
      };
```

Then change the registrations so each of these is wrapped: replace `guard((e) => store().appendChunk(e.session_id, e.text))` with `guard(alive((e) => store().appendChunk(e.session_id, e.text)))`; wrap the `onAgentToolActivity` and `onAgentUsage` handler arguments the same way (`guard(alive((e) => ...))`); and add this registration right after the chunk one:

```ts
      onAgentSessionThought(guard(alive((e) => store().appendReasoning(e.session_id, e.text)))),
```

- [ ] **Step 10: Run the bridge tests**

Run: `yarn test assistant-event-bridge`
Expected: PASS (the new test and every existing one).

- [ ] **Step 11: Write the failing component tests**

Create `src/components/assistant/__tests__/ReasoningBlock.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { ReasoningBlock } from '@/components/assistant/ReasoningBlock';
import { useAssistantStore } from '@/stores/assistant-store';

describe('ReasoningBlock', () => {
  beforeEach(() => {
    useAssistantStore.getState().reset();
  });

  it('renders nothing when there is no reasoning', () => {
    const { container } = render(<ReasoningBlock />);
    expect(container).toBeEmptyDOMElement();
  });

  it('is collapsed by default and shows the reasoning when opened', async () => {
    useAssistantStore.setState({ reasoning: 'The token variable is empty.' });
    render(<ReasoningBlock />);
    expect(screen.queryByText('The token variable is empty.')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /reasoning/i }));
    expect(screen.getByText('The token variable is empty.')).toBeInTheDocument();
  });
});
```

Create `src/components/assistant/__tests__/WaitingNotice.test.tsx`:

```tsx
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WaitingNotice } from '@/components/assistant/WaitingNotice';
import { useAssistantStore } from '@/stores/assistant-store';

function startTurn() {
  const store = useAssistantStore.getState();
  const token = store.beginSession('agent-1', 'edit');
  store.activateSession(token, 's1', []);
  store.appendUserMessage('why does login fail?');
}

describe('WaitingNotice', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useAssistantStore.getState().reset();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('stays hidden while the agent is active', () => {
    startTurn();
    render(<WaitingNotice />);
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    expect(screen.queryByText(/still working/i)).not.toBeInTheDocument();
  });

  it('appears after 20 seconds without any update and says how long', () => {
    startTurn();
    render(<WaitingNotice />);
    act(() => {
      vi.advanceTimersByTime(25_000);
    });
    expect(screen.getByText(/still working/i)).toHaveTextContent('25');
  });

  it('is hidden when no turn is running', () => {
    render(<WaitingNotice />);
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(screen.queryByText(/still working/i)).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 12: Run them to verify they fail**

Run: `yarn test ReasoningBlock WaitingNotice`
Expected: FAIL (modules not found).

- [ ] **Step 13: Implement the components**

Create `src/components/assistant/ReasoningBlock.tsx`:

```tsx
import { Brain, ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/** The agent's reasoning for the current turn. Collapsed until the user opens it. */
export function ReasoningBlock() {
  const reasoning = useAssistantStore((s) => s.reasoning);
  const running = useAssistantStore(selectTurnRunning);
  const [open, setOpen] = useState(false);
  if (!reasoning) return null;
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <Collapsible open={open} onOpenChange={setOpen} className='rounded-md border bg-muted/30'>
      <CollapsibleTrigger asChild>
        <Button variant='ghost' size='sm' className='h-7 w-full justify-start gap-1 px-2 text-xs'>
          <Chevron className='h-3 w-3' />
          <Brain className='h-3 w-3' />
          {running ? 'Reasoning (live)' : 'Reasoning'}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <p className='max-h-48 overflow-auto whitespace-pre-wrap px-2 pb-2 text-xs text-muted-foreground'>
          {reasoning}
        </p>
      </CollapsibleContent>
    </Collapsible>
  );
}
```

Create `src/components/assistant/WaitingNotice.tsx`:

```tsx
import { Clock } from 'lucide-react';
import { useEffect, useState } from 'react';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/** Seconds without any agent update before the notice appears. */
const QUIET_AFTER_SECONDS = 20;

/** Tells the user the agent is still working when it has sent nothing for a while. */
export function WaitingNotice() {
  const running = useAssistantStore(selectTurnRunning);
  const lastActivityAt = useAssistantStore((s) => s.lastActivityAt);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [running]);

  if (!running || lastActivityAt === undefined) return null;
  const quietFor = Math.floor((now - lastActivityAt) / 1000);
  if (quietFor < QUIET_AFTER_SECONDS) return null;
  return (
    <p className='flex items-center gap-1 text-xs text-muted-foreground' role='status'>
      <Clock className='h-3 w-3' />
      Still working: {quietFor}s since the last update. Deep reasoning can take a few minutes. Use
      Stop to cancel.
    </p>
  );
}
```

In `src/components/assistant/AssistantChatView.tsx` import both components and render them where the timeline items end (after the mapped timeline entries and before the pending proposals): `<ReasoningBlock />` then `<WaitingNotice />`, each inside the same container that holds the timeline items.

- [ ] **Step 14: Run all checks for this task**

Run: `yarn test assistant-store agent-session-api assistant-event-bridge ReasoningBlock WaitingNotice AssistantChatView && yarn tsc --noEmit && yarn check`
Expected: PASS. If an existing `AssistantChatView` test counts child nodes, adjust it to ignore the two new elements, which render nothing when the store has no reasoning and no running turn.

- [ ] **Step 15: Commit**

```bash
git add src/lib/tauri-api.ts src/stores/assistant-store.ts src/lib/assistant-event-bridge.ts \
  src/components/assistant/ReasoningBlock.tsx src/components/assistant/WaitingNotice.tsx \
  src/components/assistant/AssistantChatView.tsx \
  src/lib/queries/__tests__/agent-session-api.test.ts src/stores/__tests__/assistant-store.test.ts \
  src/lib/__tests__/assistant-event-bridge.test.ts \
  src/components/assistant/__tests__/ReasoningBlock.test.tsx \
  src/components/assistant/__tests__/WaitingNotice.test.tsx
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(assistant): show agent reasoning and a quiet-agent notice`.

---

### Task 11: A configurable idle limit per agent

**Files:**
- Modify: `crates/rocket-acp/src/agent_config.rs` (`AgentTuning`, field on `AgentConfig`)
- Modify: `crates/rocket-acp/src/lib.rs` (re-export)
- Modify (compiler-guided, add `tuning: AgentTuning::default(),`): every `AgentConfig { ... }` literal, as in Plan 1 Task 1 Step 5
- Modify: `crates/rocket-app/src/agent_config_service.rs` (`validate_config`)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (struct, `start_workspace_session`, `send_prompt`, `end_idle_session`, `release`, `end_untracked`, `end_all_sessions`, tests)
- Modify: `src-tauri/src/commands/agent_configs.rs` (DTO)
- Modify: `src/lib/tauri-api.ts`, `src/components/settings/AgentConfigsDialog.tsx`
- Test: the same files plus `src/components/settings/__tests__/AgentConfigsDialog.test.tsx`

**Interfaces:**
- Consumes: `AgentConfig`, `AcpSessionService` (Plans 1 and 2).
- Produces:
  - `rocket_acp::AgentTuning { idle_timeout_secs: Option<u32> }` (`Default`, `Clone`, `PartialEq`, `Eq`, serde), with `AgentTuning::validate(&self) -> Result<(), String>`.
  - `rocket_acp::{MIN_IDLE_TIMEOUT_SECS (30), MAX_IDLE_TIMEOUT_SECS (1800)}`.
  - `AgentConfig.tuning: AgentTuning` with `#[serde(default)]`.
  - `AgentTuningDto { idle_timeout_secs: Option<u32> }` (camelCase) and `AgentConfigDto.tuning`.
  - TypeScript `AgentTuning { idleTimeoutSecs?: number }` and `AgentConfig.tuning?: AgentTuning`.
  - `AcpSessionService::idle_timeout_for(&self, session_id: &str) -> Duration` (private) and a `#[cfg(test)]` setter `set_session_idle_for_test`.

- [ ] **Step 1: Write the failing domain tests**

Add inside `tests` of `crates/rocket-acp/src/agent_config.rs`:

```rust
    #[test]
    fn a_config_without_tuning_loads_with_the_defaults() {
        let json = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "credential_env_var": "K",
            "vault_connection_id": "v", "vault_name": "n", "vault_secret_id": "s",
            "vault_secret_name": "sn"
        });
        let config: AgentConfig = serde_json::from_value(json).expect("old shape");
        assert_eq!(config.tuning, AgentTuning::default());
        assert_eq!(config.tuning.idle_timeout_secs, None);
    }

    #[test]
    fn the_idle_timeout_must_stay_between_30_and_1800_seconds() {
        let tuning = |secs: Option<u32>| AgentTuning {
            idle_timeout_secs: secs,
        };
        assert!(tuning(None).validate().is_ok());
        assert!(tuning(Some(MIN_IDLE_TIMEOUT_SECS)).validate().is_ok());
        assert!(tuning(Some(MAX_IDLE_TIMEOUT_SECS)).validate().is_ok());
        for bad in [0, 5, 29, 1801, 99_999] {
            let err = tuning(Some(bad)).validate().expect_err("out of range");
            assert!(err.contains("idle_timeout_secs"), "{err}");
        }
    }

    #[test]
    fn tuning_roundtrips_in_snake_case() {
        let mut c = sample();
        c.tuning.idle_timeout_secs = Some(300);
        let json = serde_json::to_string(&c).expect("serialize AgentConfig");
        assert!(json.contains("\"idle_timeout_secs\":300"), "got: {json}");
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert_eq!(back.tuning.idle_timeout_secs, Some(300));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p rocket-acp -j4 tuning`
Expected: FAIL to compile (`cannot find type AgentTuning`).

- [ ] **Step 3: Implement `AgentTuning` and the config field**

In `agent_config.rs` add above `AgentConfig`:

```rust
/// Shortest and longest accepted idle limit, in seconds.
pub const MIN_IDLE_TIMEOUT_SECS: u32 = 30;
pub const MAX_IDLE_TIMEOUT_SECS: u32 = 1800;

/// Per-agent knobs that tune how a session runs. Old files without this
/// struct load with every field at its default.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentTuning {
    /// Seconds a prompt turn may stay silent before the session is ended.
    /// `None` keeps the built-in 120 seconds. A deep reasoning turn on a
    /// slow model may need more.
    #[serde(default)]
    pub idle_timeout_secs: Option<u32>,
}

impl AgentTuning {
    /// Checks every field. The error is a short reason and never contains a value.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(secs) = self.idle_timeout_secs {
            if !(MIN_IDLE_TIMEOUT_SECS..=MAX_IDLE_TIMEOUT_SECS).contains(&secs) {
                return Err(format!(
                    "tuning idle_timeout_secs must be between {MIN_IDLE_TIMEOUT_SECS} and {MAX_IDLE_TIMEOUT_SECS}"
                ));
            }
        }
        Ok(())
    }
}
```

Add to `AgentConfig` after `kind`:

```rust
    /// Per-agent session tuning. Old files without it load with the defaults.
    #[serde(default)]
    pub tuning: AgentTuning,
```

Add `AgentTuning, MAX_IDLE_TIMEOUT_SECS, MIN_IDLE_TIMEOUT_SECS,` to the `pub use agent_config::{...}` list in `crates/rocket-acp/src/lib.rs`. In the `sample()` test helper add `tuning: AgentTuning::default(),`.

- [ ] **Step 4: Run the domain tests and fix the other literals**

Run: `cargo test -p rocket-acp -j4 agent_config`
Expected: PASS.

Run: `cargo check -j4 -p rocket-infra -p rocket-app -p rocket --tests`
Expected: one `E0063 missing field tuning` per `AgentConfig` literal (the same sites as Plan 1 Task 1 Step 5). Add `tuning: rocket_acp::AgentTuning::default(),` to each, then run the command again until clean.

- [ ] **Step 5: Write the failing service and validation tests**

In `crates/rocket-app/src/agent_config_service.rs` add inside `tests`:

```rust
    #[test]
    fn validate_config_refuses_an_idle_timeout_out_of_range() {
        let mut config = sample_config("a", "conn-1");
        config.tuning.idle_timeout_secs = Some(5);
        let err = validate_config(&config).expect_err("too short");
        assert!(err.to_string().contains("idle_timeout_secs"), "{err}");
        config.tuning.idle_timeout_secs = Some(300);
        assert!(validate_config(&config).is_ok());
    }
```

In `crates/rocket-app/src/acp_session_service.rs` add inside `tests`:

```rust
    fn service_with_config(config: AgentConfig, idle: Duration) -> AcpSessionService {
        let sm_repo = FakeSecretManagerRepo(Mutex::new(vec![sample_connection()]));
        let secret_manager = Arc::new(crate::secret_manager_service::SecretManagerService::new(
            Box::new(sm_repo),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher::default()),
        ));
        let cfg_service = Arc::new(AgentConfigService::new(
            Box::new(FakeAgentConfigRepo(Mutex::new(vec![config]))),
            secret_manager,
            Arc::new(rocket_environment::secret_store::NullSecretStore),
            crate::process_env_reader(),
        ));
        AcpSessionService::with_prompt_idle_timeout(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::new(FakeEventPublisher::new()))),
            noop_cleanup(),
            cfg_service,
            idle,
        )
    }

    #[tokio::test]
    async fn a_configured_idle_timeout_applies_to_the_session_it_started() {
        let mut config = sample_config();
        config.tuning.idle_timeout_secs = Some(300);
        let service = service_with_config(config, Duration::from_secs(120));

        service
            .start_workspace_session("agent-1", "/tmp", test_creds(), test_isolation())
            .await
            .expect("start");

        assert_eq!(service.idle_timeout_for("session-1"), Duration::from_secs(300));
        assert_eq!(
            service.idle_timeout_for("another-session"),
            Duration::from_secs(120),
            "other sessions keep the default"
        );
    }

    #[tokio::test]
    async fn an_agent_without_a_configured_idle_timeout_uses_the_default() {
        let service = service_with_config(sample_config(), Duration::from_secs(120));
        service
            .start_workspace_session("agent-1", "/tmp", test_creds(), test_isolation())
            .await
            .expect("start");
        assert_eq!(service.idle_timeout_for("session-1"), Duration::from_secs(120));
    }

    #[tokio::test]
    async fn a_session_idle_override_keeps_a_slow_turn_alive_and_the_default_still_kills_others() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let slow = FakeSessionClient {
            prompt_delay: Duration::from_millis(150),
            ..Default::default()
        };
        let service = service_with(slow, &publisher, Duration::from_millis(30));
        service.set_session_idle_for_test("session-1", Duration::from_millis(500));

        service
            .send_prompt("session-1", hi())
            .await
            .expect("the override lets the slow turn finish");

        let err = service
            .send_prompt("session-2", hi())
            .await
            .expect_err("the default limit still applies to another session");
        assert!(matches!(err, DomainError::Internal(_)));
    }

    #[tokio::test]
    async fn ending_a_session_forgets_its_idle_override() {
        let mut config = sample_config();
        config.tuning.idle_timeout_secs = Some(300);
        let service = service_with_config(config, Duration::from_secs(120));
        service
            .start_workspace_session("agent-1", "/tmp", test_creds(), test_isolation())
            .await
            .expect("start");
        assert!(service.track("session-1"));
        let _ = service.end_session("session-1").await;
        assert_eq!(service.idle_timeout_for("session-1"), Duration::from_secs(120));
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -p rocket-app -j4 idle_timeout`
Expected: FAIL to compile (`no method idle_timeout_for`, `set_session_idle_for_test`).

- [ ] **Step 7: Implement the per-session limit**

In `acp_session_service.rs`:

1. Add a field to `AcpSessionService` after `prompt_idle_timeout`: `session_idle: Mutex<HashMap<String, Duration>>,` and initialise it with `session_idle: Mutex::new(HashMap::new()),` in `with_prompt_idle_timeout`. Update the doc comment of `DEFAULT_PROMPT_IDLE_TIMEOUT` to say "An agent config can raise or lower it with `tuning.idle_timeout_secs`."
2. Add these methods inside `impl AcpSessionService`:

```rust
    /// The idle limit of one session: its agent's configured value, or the
    /// service default.
    fn idle_timeout_for(&self, session_id: &str) -> Duration {
        self.session_idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(session_id)
            .copied()
            .unwrap_or(self.prompt_idle_timeout)
    }

    fn forget_idle(&self, session_id: &str) {
        self.session_idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session_id);
    }

    #[cfg(test)]
    fn set_session_idle_for_test(&self, session_id: &str, idle: Duration) {
        self.session_idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id.to_string(), idle);
    }
```

3. In `start_workspace_session`, after the `start_session(...)` call succeeds and before the event is published, add:

```rust
        if let Some(secs) = config.tuning.idle_timeout_secs {
            self.session_idle
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(info.session_id.clone(), Duration::from_secs(u64::from(secs)));
        }
```

4. In `send_prompt` add `let idle = self.idle_timeout_for(session_id);` at the top, replace `tokio::time::sleep(self.prompt_idle_timeout)` with `tokio::time::sleep(idle)` and change the call to `self.end_idle_session(session_id, idle).await`. Change `end_idle_session` to `async fn end_idle_session(&self, session_id: &str, idle: Duration) -> DomainError` and use `idle.as_secs()` in its message instead of `self.prompt_idle_timeout.as_secs()`.
5. In `release` call `self.forget_idle(session_id);` first. In `end_untracked` call `self.forget_idle(session_id);` before ending. In `end_all_sessions` add `self.session_idle.lock().unwrap_or_else(PoisonError::into_inner).clear();` after the live set is drained.

In `agent_config_service.rs` `validate_config` add before the final `Ok(())`:

```rust
    config
        .tuning
        .validate()
        .map_err(|why| DomainError::InvalidInput(format!("agent config {why}")))?;
```

- [ ] **Step 8: Run the rocket-app tests**

Run: `cargo test -p rocket-app -j4 acp_session_service && cargo test -p rocket-app -j4 agent_config_service`
Expected: PASS (the existing timeout tests keep passing: no override means the default).

- [ ] **Step 9: Write the failing DTO test and implement the DTO**

In `src-tauri/src/commands/agent_configs.rs` add inside `tests`:

```rust
    #[test]
    fn the_dto_carries_tuning_in_camel_case_and_old_callers_get_the_defaults() {
        let mut domain = sample_domain();
        domain.tuning.idle_timeout_secs = Some(300);
        let json = serde_json::to_value(AgentConfigDto::from(domain)).expect("json");
        assert_eq!(json["tuning"]["idleTimeoutSecs"], 300);

        let old = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "args": [],
            "credentialEnvVar": "K", "vaultConnectionId": "", "vaultName": "",
            "vaultSecretId": "", "vaultSecretName": ""
        });
        let dto: AgentConfigDto = serde_json::from_value(old).expect("old caller");
        assert_eq!(AgentConfig::from(dto).tuning.idle_timeout_secs, None);
    }
```

Implement: add this DTO above `AgentConfigDto`:

```rust
/// Per-agent tuning on the wire.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTuningDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_timeout_secs: Option<u32>,
}
```

Add to `AgentConfigDto` after `kind`: `#[serde(default)] pub tuning: AgentTuningDto,`. In `From<AgentConfig> for AgentConfigDto` add `tuning: AgentTuningDto { idle_timeout_secs: c.tuning.idle_timeout_secs },` and in `From<AgentConfigDto> for AgentConfig` add `tuning: rocket_acp::AgentTuning { idle_timeout_secs: dto.tuning.idle_timeout_secs },`.

Run: `cargo test -p rocket -j4 --lib agent_configs`
Expected: PASS.

- [ ] **Step 10: Write the failing dialog tests**

Add inside `describe('AgentConfigsDialog', ...)`:

```tsx
  it('saves an idle timeout in seconds under tuning', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    await user.type(screen.getByLabelText(/Idle timeout/), '300');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0]).toMatchObject({
      tuning: { idleTimeoutSecs: 300 },
    });
  });

  it('sends no tuning when the idle timeout is left empty', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(api.saveAgentConfig).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.saveAgentConfig).mock.calls[0][0].tuning).toBeUndefined();
  });

  it('refuses an idle timeout outside 30 to 1800 seconds', async () => {
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    await screen.findByLabelText('Label');
    await user.type(screen.getByLabelText(/Idle timeout/), '5');
    expect(screen.getByRole('alert')).toHaveTextContent(/between 30 and 1800/i);
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('shows a stored idle timeout', async () => {
    vi.mocked(api.listAgentConfigs).mockResolvedValue([
      { ...claude, tuning: { idleTimeoutSecs: 600 } },
    ]);
    const user = userEvent.setup();
    renderDialog();
    await user.click(await screen.findByRole('button', { name: 'Edit agent' }));
    expect(await screen.findByLabelText(/Idle timeout/)).toHaveValue(600);
  });
```

- [ ] **Step 11: Run them to verify they fail**

Run: `yarn test AgentConfigsDialog`
Expected: FAIL (no field labelled "Idle timeout").

- [ ] **Step 12: Implement the TypeScript type and the dialog field**

In `src/lib/tauri-api.ts` add above `AgentConfig`:

```ts
// Per-agent session tuning. A missing value means the default.
export interface AgentTuning {
  // Seconds a prompt turn may stay silent before the session ends. 30 to 1800.
  idleTimeoutSecs?: number;
}
```

and inside `AgentConfig` after `kind?: AgentKind;` add `tuning?: AgentTuning;`.

In `AgentConfigsDialog.tsx`:

1. Add above the component:

```tsx
const MIN_IDLE_SECONDS = 30;
const MAX_IDLE_SECONDS = 1800;

// An empty field means the default. The backend stays the authority.
function validateIdleSeconds(text: string): string | null {
  const trimmed = text.trim();
  if (trimmed === '') return null;
  const value = Number(trimmed);
  if (!Number.isInteger(value) || value < MIN_IDLE_SECONDS || value > MAX_IDLE_SECONDS) {
    return `Use a whole number between ${MIN_IDLE_SECONDS} and ${MAX_IDLE_SECONDS}.`;
  }
  return null;
}
```

2. Add `idleTimeoutSecs: '',` to `emptyForm`; in `startEdit` add `idleTimeoutSecs: c.tuning?.idleTimeoutSecs ? String(c.tuning.idleTimeoutSecs) : '',`.
3. After `envInvalid` is computed add `const idleError = editing ? validateIdleSeconds(editing.idleTimeoutSecs) : null;` and include `|| idleError !== null` in the `envInvalid` expression.
4. In `handleSave`'s `config` object add `tuning: editing.idleTimeoutSecs.trim() === '' ? undefined : { idleTimeoutSecs: Number(editing.idleTimeoutSecs.trim()) },` after `kind`.
5. Add this field after the Working Directory `<div>`:

```tsx
            <div>
              <Label htmlFor='ac-idle-timeout' className='text-sm'>
                Idle timeout <span className='text-muted-foreground'>(seconds, optional)</span>
              </Label>
              <Input
                id='ac-idle-timeout'
                type='number'
                min={MIN_IDLE_SECONDS}
                max={MAX_IDLE_SECONDS}
                value={editing.idleTimeoutSecs}
                onChange={(e) => setEditing({ ...editing, idleTimeoutSecs: e.target.value })}
                placeholder='120'
                className='h-8 text-sm'
                aria-invalid={idleError !== null}
              />
              {idleError ? (
                <p role='alert' className='mt-1 text-xs text-destructive'>
                  {idleError}
                </p>
              ) : (
                <p className='mt-1 text-xs text-muted-foreground'>
                  How long a turn may stay silent before the session ends. Raise it for deep
                  reasoning on a slow model.
                </p>
              )}
            </div>
```

- [ ] **Step 13: Run all checks for this task**

Run: `yarn test AgentConfigsDialog && yarn tsc --noEmit && yarn check && cargo test -p rocket-acp -j4 && cargo test -p rocket-app -j4 && cargo check -j4`
Expected: PASS.

- [ ] **Step 14: Commit**

```bash
git add crates/rocket-acp/src/agent_config.rs crates/rocket-acp/src/lib.rs \
  crates/rocket-infra/src/fs_agent_config_repo.rs crates/rocket-app/src/agent_config_service.rs \
  crates/rocket-app/src/acp_session_service.rs src-tauri/tests/acp_mcp_start_workspace_assistant.rs \
  src-tauri/src/commands/agent_configs.rs src/lib/tauri-api.ts \
  src/components/settings/AgentConfigsDialog.tsx \
  src/components/settings/__tests__/AgentConfigsDialog.test.tsx
```

Then invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(acp): make the idle limit configurable per agent`.

---

## Next plan

**Next: Plan 5, reasoning level and Gemini model** (`docs/superpowers/plans/2026-10-12-acp-multi-agent-plan-05-reasoning-level.md`).

When Tasks 9 to 11 are committed with green checks, my Claude executes Plan 5 next, in the same worktree, without asking again. One plan at a time. Plan 5 adds `reasoning` and `model` to the `AgentTuning` struct made here, so do not start it before Task 11 is merged into the working branch. Ledger: `.superpowers/sdd/2026-10-12-acp-multi-agent-plan-04/progress.md`.
