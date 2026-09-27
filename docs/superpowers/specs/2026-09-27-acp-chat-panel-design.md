# AI Assist Chat Panel (Subproject C)

## Context

Rocket is gaining an AI assist feature in the Scripts tab, built on the Agent Client Protocol (ACP). The full feature was decomposed into five subprojects (build order A → B → {C, D in parallel} → E); see the project memory `project_acp_ai_assist_feature.md` for the complete decomposition and the decisions that apply across all of them. Subprojects A (Agent Configuration & Credentials) and B (ACP Transport) are complete and merged to `main`. A delivered `AgentConfig`/`AgentConfigRepository`/`AgentConfigService` and the `AgentConfigsDialog` settings UI. B delivered the transport itself: three Tauri commands (`start_agent_session`, `send_agent_prompt`, `end_agent_session`) and four event channels (`agent-session-started`, `agent-session-chunk`, `agent-session-finished`, `agent-session-failed`) that stream chat-only agent turns, with a guaranteed ordering (every chunk event publishes before the finished/failed event for that prompt).

This spec covers **only subproject C**: the frontend chat panel that lets a user converse with a configured agent from inside the Scripts tab, and insert code the agent suggests into the active script editor. It is pure frontend work — no new Rust code — consuming B's existing IPC surface exactly as B left it.

**Out of scope for this subproject:** tool-calling / MCP (subproject D — the agent cannot run requests, read collection data, or edit scripts on its own yet; it can only reply with text/markdown the user chooses to insert manually), and the autonomous safety valve (subproject E — there is no agent-initiated action for that gate to apply to yet).

## Session scope

One agent chat session per open request tab, not one per script phase (Pre Request/Post Response/Tests) and not a single app-wide session. This mirrors how Flow scopes an in-flight run to its own tab (`FlowTab.runState`/`runId` in `src/types/pane-types.ts`), rather than the module-level singleton pattern `load-test-store.ts` uses for its single global run. A user can have independent conversations open in several request tabs at once, each backed by its own ACP session id and, on the backend, its own spawned agent process (B's `AcpAgentClient` already supports multiple concurrent sessions via its session map).

Each session is also tied to one specific `AgentConfig`, chosen when the session starts; different tabs may use different configured agents.

## Layout

`ScriptsTab.tsx` currently renders `ScriptSnippetSidebar` as a flex sibling of the Monaco editor **inside each phase's `TabsContent`** — the sidebar is deliberately re-mounted per phase because its insert target (`editorRefs.current[phase]`) is phase-specific.

The chat panel is different: the conversation is scoped to the whole request tab, not to whichever phase happens to be active, so it must not reset when the user switches between Pre Request/Post Response/Tests. `AgentChatPanel` therefore mounts as a flex sibling of the entire `<Tabs>` element (one level higher than `ScriptSnippetSidebar`), toggled by a second button placed next to the existing "Snippets" toggle in the `TabsList` row. "Insert to editor" still targets whichever phase is currently active, by reading `editorRefs.current[activeTab]` — `ScriptsTab` already tracks this today, so no new plumbing is needed for that part.

`ScriptsTab` needs a `tabId: string` prop added, the same way `LoadTestTab` already receives `tabId={tab.id}` from `RequestPanel.tsx` — today `ScriptsTab` is the only phase-tab component that doesn't get one, since it previously had no reason to look up per-tab state.

## State shape

Following the same "state lives on the tab object" convention `FlowTab` established in `pane-store.ts`, extend the request tab's type (`pane-types.ts`) with an optional field:

```ts
interface ChatMessage {
  id: string;
  role: 'user' | 'agent';
  text: string;
  streaming?: boolean; // true while more chunks for this message are still arriving
}

interface AgentChatSession {
  agentConfigId: string;
  sessionId: string;
  status: 'starting' | 'active' | 'ended' | 'error';
  messages: ChatMessage[];
  error?: string;
}

// on the request tab type:
agentSession?: AgentChatSession;
```

No persistence: this state is in-memory only, held in `pane-store.ts` like the rest of a tab's transient UI state. Closing the tab or restarting the app loses the conversation — deliberately, to avoid a new persistence path and any interaction with the OpenCollection spec for this milestone.

## Tauri wrapper layer

`src/lib/tauri-api.ts` has no wrappers for B's three commands or four events yet (only Agent *config* CRUD exists there today). Add a new banner section following the exact convention the Flow section already established (`runFlow`/`onFlowRunStarted`/`onFlowStepCompleted`/`onFlowRunFinished`, `tauri-api.ts:1810-1863`):

```ts
// ==== AI Assist (ACP chat sessions) ====
export const startAgentSession = (agentConfigId: string, cwd: string) =>
  invoke<string>('start_agent_session', { agentConfigId, cwd });
export const sendAgentPrompt = (sessionId: string, prompt: string) =>
  invoke<string>('send_agent_prompt', { sessionId, prompt });
export const endAgentSession = (sessionId: string) =>
  invoke<void>('end_agent_session', { sessionId });

export const onAgentSessionStarted = (handler: (p: { session_id: string }) => void) =>
  listen<{ session_id: string }>('agent-session-started', (e) => handler(e.payload));
export const onAgentSessionChunk = (handler: (p: { session_id: string; text: string }) => void) =>
  listen<{ session_id: string; text: string }>('agent-session-chunk', (e) => handler(e.payload));
export const onAgentSessionFinished = (
  handler: (p: { session_id: string; stop_reason: string }) => void,
) => listen<{ session_id: string; stop_reason: string }>('agent-session-finished', (e) => handler(e.payload));
export const onAgentSessionFailed = (handler: (p: { session_id: string; error: string }) => void) =>
  listen<{ session_id: string; error: string }>('agent-session-failed', (e) => handler(e.payload));
```

Event payload fields stay snake_case (`session_id`, `stop_reason`), matching every other `DomainEvent`-derived event in this file — command args stay camelCase, matching every existing Tauri command wrapper.

## Data flow

1. **No session yet.** `AgentChatPanel` shows an agent picker built from `useAgentConfigs()` (the existing React Query hook from subproject A) plus a Start button.
2. **Start.** Calls `startAgentSession(agentConfigId, cwd)` (`cwd` is the collection's working directory — the same value subproject B's spec already deferred to the caller). On success, sets `agentSession = { agentConfigId, sessionId, status: 'active', messages: [] }` on the tab and subscribes to the three streaming events, guarded by `sessionId` exactly like Flow guards by `runId` (`useRef<UnlistenFn[]>`, cleaned up on unmount via `useEffect` and re-subscribed if the tab remounts mid-session with a stored `sessionId`).

   **Resolving `cwd`:** a `RequestTab` does not carry a collection filesystem path today — `tab.source.collection` (`pane-types.ts:28-32`, via `BaseTab`) is a collection *name*, not a path. There is an existing precedent for turning a name into a path, used by `useKeyboardShortcuts.ts:70` and `CollectionNode.tsx:113` for embedded collections: look up `activeWorkspace.path` from the workspace React Query cache and concatenate `` `${activeWorkspace.path}/collections/${tab.source.collection}` ``. For an `external`-type collection, `CollectionReference.path` (`tauri-api.ts:599`) already holds the real path directly and must be used instead of the concatenation — this needs a small helper (e.g. `resolveCollectionPath(collection: CollectionReference | { workspacePath, name })`) so `AgentChatPanel` doesn't duplicate the branch inline. This is new plumbing this subproject must add; it doesn't exist anywhere as a single reusable function yet.
3. **Send.** Appends a `{ role: 'user', text }` message, then a placeholder `{ role: 'agent', text: '', streaming: true }` message, then calls `sendAgentPrompt(sessionId, text)`. While the call is pending:
   - `agent-session-chunk` events (filtered to this `sessionId`) append their `text` onto the in-progress agent message.
   - `agent-session-finished` sets that message's `streaming` to `false`.
   - `agent-session-failed` sets that message's `streaming` to `false`, appends the error text, and sets `agentSession.status = 'error'`.
   The `sendAgentPrompt` invoke promise itself is only used to catch an immediate call-level rejection (e.g. an already-ended session); the event stream, not the promise resolution, drives message rendering — this is safe because of B's guaranteed chunk-before-finished/failed ordering.
   The Send button is disabled while a message with `streaming: true` exists, purely for UX clarity — B's backend already serializes concurrent prompts per session via its `prompt_lock`, so this isn't a correctness requirement, just avoids a confusing "did my second message do anything?" moment.
4. **Insert to editor.** Each fenced code block in a rendered agent message gets an "Insert" button that calls the existing `insertSnippet(editorRefs.current[activeTab], code)` already defined in `ScriptsTab.tsx` — no changes needed to that function.
5. **End.** An explicit "End session" control calls `endAgentSession(sessionId)` and sets `status: 'ended'`.

## Error handling

- **Session start fails** (bad command, credential resolution error surfaced as a rejected `startAgentSession` call) — show the error inline where the picker was, `status` stays unset (no session created), retry is just re-clicking Start.
- **Mid-prompt failure** (`agent-session-failed`) — per B's design the underlying process is already killed by the backend on failure, so the panel does not offer "retry this session"; it offers "Start new session," which re-shows the picker.
- **Tab closed with an active session** — not covered by anything in subproject B, which only sweeps sessions on whole-app exit (`RunEvent::Exit`), not on a single tab closing while the app keeps running. This is a real gap the same shape as the one B's own review process caught for app exit: closing a tab must fire `endAgentSession(sessionId)` (best-effort, fire-and-forget — B's `end_session` is already idempotent and safe to call while a prompt is in flight, confirmed during B's Plan 06 work) from the `pane-store` tab-removal path, whenever the removed tab's `agentSession.status === 'active'`.
- **App exits with chats open** — already fully handled by subproject B (Plan 06's exit sweep and signal handling); C requires no new backend work for this case.

## Testing strategy

Frontend-only; no new Rust tests. Vitest component tests for `AgentChatPanel` covering: the start-session flow (picker → active state), chunk accumulation into a streaming message, finished/failed finalization, the insert-to-editor button invoking `insertSnippet` with the correct editor ref, and the tab-close cleanup calling `endAgentSession`. `yarn tsc --noEmit` and `yarn check` gate the new `tauri-api.ts` wrappers and event payload typing. Manual verification against a real running agent isn't possible in this headless sandbox (same disclosed limitation as subprojects A and B) — the eventual plan should state this explicitly rather than claim it was done.

## Out of scope for this subproject

- Tool-calling and the MCP server — subproject D. Until D exists, the agent can only produce text/code the user chooses to insert; it cannot run requests, read collection/environment data, or edit scripts on its own.
- The autonomous safety valve / per-collection opt-in gating — subproject E. There is no agent-initiated action yet for that gate to apply to.
- Persisting chat history across tab close or app restart — deliberately deferred; sessions are in-memory/ephemeral for this milestone.
- Any change to `AgentConfig`, credential resolution, or the ACP transport itself — those are final as B left them; C is a pure consumer of B's existing IPC surface.
