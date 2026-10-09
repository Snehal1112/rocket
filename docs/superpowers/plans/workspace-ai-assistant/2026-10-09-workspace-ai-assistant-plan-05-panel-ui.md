# Workspace AI Assistant — Plan 05: Panel UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the per-request-tab AI Assist chat with one workspace-level assistant: a Zustand `assistant-store`, a single app-lifetime event bridge, a docked right-hand `AssistantPanel` toggled from the title bar, a permissions popover, tool activity lines, proposal cards with Monaco diffs, and a Scripts-tab shortcut that opens the panel with the request in focus.

**Architecture:** All assistant state lives in `src/stores/assistant-store.ts` (one session per webview, narrow selectors). `src/lib/assistant/assistant-session.ts` holds the start, send, stop and end flows that call the Plans 01–04 IPC wrappers, and `src/lib/assistant-event-bridge.ts` is the only subscriber to the agent events: it routes them into the store by session id, ends the session when the active workspace changes, and runs the stale-session sweep once per webview load. The panel components under `src/components/assistant/` only read the store and call those flows; the old tab-keyed chat (`AgentChatPanel`, `tab.agentSession`, the pane-store agent actions and `agent-session-event-bridge.ts`) is deleted in Task 3.

**Tech Stack:** React 19, TypeScript, Zustand 5, `@tanstack/react-query`, shadcn/ui (Radix) primitives, `lucide-react`, `@monaco-editor/react` `DiffEditor`, Vitest 4 + Testing Library + `@testing-library/user-event`.

**Spec:** `docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md` (sections 2 "Session model" and 5 "UI"). Plan index with the locked interface contracts: `docs/superpowers/plans/workspace-ai-assistant/00-plan-index.md`.

## Verified facts

Read in this worktree on 2026-10-09 before writing the plan. Line numbers are from that read; re-check them if an earlier plan moved the code.

1. `src/App.tsx:15` imports `useAgentSessionEventBridge` and `src/App.tsx:41` mounts it once for the app's lifetime. The main row is `src/App.tsx:177-233`: sidebar, a hand-made pointer `role='separator'` resize handle (`:186-224`, with a `biome-ignore lint/a11y/useSemanticElements` comment), then `<main>` (`:225-232`). There is no right-hand panel today.
2. `src/lib/agent-session-event-bridge.ts:1-59` is tab-keyed in full: `findStreamingTarget` scans every request tab in the pane tree and the collection snapshots for a matching `agentSession.sessionId`. Its only test is `src/lib/__tests__/agent-session-event-bridge.test.ts`.
3. `src/stores/pane-store.ts`: `endAgentSession` import `:34`; `ChatMessage` type import `:48`; `findTabInSnapshots` `:136-142`, used only by `activateAgentSession` (`:673`); `updateTabEverywhere` `:144-157`, still used by non-agent actions (for example `:818`, `:1198`); `endSessionIfActive` and `endActiveSessions` `:159-193`; agent action types `:305-316`; `closeTab` calls `endSessionIfActive` at `:483-484`; agent actions `:658-773`; `endActiveSessions` callers `:1331-1333` (switchCollection), `:1392-1395` (openWorkspaceTabs), `:1466-1474` (reset); the comment at `:1342-1345` mentions agent activation. `closeAll` (`:1479`) ends in `get().reset()`.
4. `src/stores/pane-store.ts:619-626` `updateRequest` marks the tab dirty and `:642` `markClean` clears it. `scheduleAutoSave` is only called from `closeTab` (`:474`), `:543`, `openWorkspaceTabs` (`:1406`) and `closeAll` (`:1485`), so a dirty tab writes its stale copy back when it closes or the user switches.
5. `src/types/pane-types.ts:28-41` defines `ChatMessage` and `AgentChatSession`; `:43-48` `RequestTab` has `agentSession?: AgentChatSession` at `:47`. `BaseTab` (`:21-26`) has `source?: { collection: string; path: string }` at `:25`. `isRequestTab` is at `:298-300`.
6. `src/components/request/ScriptsTab.tsx`: imports `AgentChatSession` and `AgentChatPanel` at `:13-14`; prop `agentSession` at `:28`; destructures `tabId` and `agentSession` at `:71-73`; `showAgentChat` state at `:100`; the AI Assist button at `:165-178`; renders `AgentChatPanel` at `:271-278`. `tabId` is used by nothing else in the component.
7. `src/components/request/RequestPanel.tsx:1188-1198` renders `ScriptsTab` with `tabId`, `collectionName={tab.source?.collection}` and `agentSession={tab.agentSession}` (`:1191`), but no request path.
8. `src/components/collections/folder-settings/ScriptSection.tsx:26-38` and `TestSection.tsx:26-38` render `ScriptsTab` with `tabId={editorKey}` and `agentAssist={false}`. `src/components/collections/folder-settings/__tests__/ScriptTestSections.test.tsx:8-25` and `:144-164` assert on the `tabId` prop, so `ScriptsTab` keeps `tabId` in its props interface.
9. `src/components/request/__tests__/ScriptsTab.test.tsx:39-124` mocks `../AgentChatPanel` and tests the per-tab panel; `:126-216` tests phases and `agentAssist`.
10. `src/components/title-bar/TitleBar.tsx:2` imports `Bot, Globe, Settings`; the Bot button that opens `AgentConfigsDialog` is `:52-60`, followed by `{!isMac && <WindowControls />}` at `:61`. There are no TitleBar tests.
11. `src/components/request/AgentAutonomyToggle.tsx:30-128` does the read-modify-write save (`:56-71`), asks before turning on (`:73-77`), uses the fixed element id `agent-autonomy-switch` (`:83`, `:88`) and the label "Allow this agent to run requests and edit files" (`:89`). Its test `src/components/request/__tests__/AgentAutonomyToggle.test.tsx` pins that label at `:14` and the dialog title at `:55`.
12. `src/components/git/DiffViewer.tsx` is the repo's only Monaco diff: `import '@/components/editor/monaco-setup'` (`:1`), `DiffEditor` from `@monaco-editor/react` (`:2`), `MONACO_FONT_FAMILY` (`:5`), `acquireJsWorker`/`releaseJsWorker` (`:6`, `:112-117`), `useMonacoTheme().themeName` (`:7`, `:44`), dispose-on-unmount through a ref (`:46-60`), options at `:154-162`. It is bound to the git `DiffState`, so this plan adds a small sibling component that copies its setup. `DiffViewer` is always loaded with `React.lazy` (`src/components/git/DiffViewForFile.tsx:7`, `src/components/panes/EditorGroup.tsx:14`).
13. `src/lib/tauri-api.ts`: `Request` `:166-185` stores scripts as `preRequestScript`, `postResponseScript` and `tests`; `getRequest(collection, path)` `:873-874`; `listCollections` `:866`; `getCollectionSettings`/`saveCollectionSettings` are what the toggle uses; the AI Assist wrappers `:2519-2526`; the session events `:2528-2560` and `:2627-2636`, with snake_case payload fields. `HttpMethod` is `string` (`:15`); `Auth` starts `{ authType: 'none' }` (`:48-49`).
14. `src/lib/queries/collection-queries.ts:4-13` exports `collectionKeys` and `useCollections()`. The backing `list_collections` command lists only the active workspace's collections (`src-tauri/src/commands/collections.rs:44-51`).
15. `src/stores/workspace-store.ts` starts with `activeWorkspaceId: ''`. `src/App.tsx:58` sets it during startup and `src/App.tsx:120-123` sets it on `workspace-switched`. That id is the frontend's signal that the active workspace changed.
16. `src/lib/pane-utils.ts:24` `mapApiRequestToState(req, fromCollection)` (sidebar tabs pass `true`, for example `src/components/collections/RequestNode.tsx:192`), `:237` `isPathWithin`, `:332` `collectAllTabs`.
17. `src/components/collections/MarkdownRenderer.tsx:9-15` takes `children: string` and an optional `renderCodeActions`.
18. `src/stores/layout-store.ts` holds `sidebarWidth`/`consoleHeight` and their setters; `src/stores/__tests__/layout-store.test.ts` sets only those fields, so a new field does not break it.
19. `src/main.tsx:49` wraps the app in `React.StrictMode`, so effects run twice in development.
20. `tsconfig.json` includes all of `src`, so test files are type-checked by `yarn tsc --noEmit`. `biome.json` excludes `**/__tests__/**` and `*.test.*` and enforces single-quote JSX, sorted imports and `useExhaustiveDependencies`. `tsconfig.json` has `lib: ["ES2020", ...]`: no `Array.prototype.at` or `findLast`.
21. `vite.config.ts:7-11`: Vitest runs in jsdom with `src/test-setup.ts` (jest-dom plus a `ResizeObserver` polyfill). `src/test/deferred.ts` exports `createDeferred<T>()`.
22. Plan 02 (`2026-10-09-workspace-ai-assistant-plan-02-isolation-and-lifecycle.md:1771`, `:1900`) adds `export const endStaleAssistantSessions = () => invoke<number>('end_stale_assistant_sessions');` and asks Plan 05 to call it once before any `start_workspace_assistant`.
23. `.claude/frontend.md`, referenced by `CLAUDE.md`, does not exist in this worktree. The Zustand and UI rules come from `CLAUDE.md` and `.claude/rules/frontend-component-guardrails.md`.
24. The TypeScript surface Plans 01–04 add to `src/lib/tauri-api.ts`, as written in their plan files in this directory:
    - Plan 01 (`…-plan-01-acp-client-upgrade.md:3663-3770`): `ConfigChoice` (`description: string | null`), `ConfigOption` (`category: string | null`, `currentValue`), `AgentSessionStarted`, `PromptResourceDto`, `sendAgentPrompt(sessionId, prompt, resources?)`, `cancelAgentPrompt`, `setAgentConfigOption`, `AgentToolCallStatus = 'pending' | 'in_progress' | 'completed' | 'failed'`, `onAgentToolActivity`, `AgentConfigOptionPayload` (snake_case `current_value`), `onAgentConfigOptions`, `onAgentUsage` (`cost_usd: number | null`), and `configOptionsFromEvent(options): ConfigOption[]` (`:3761`). Plan 01 also adapts `AgentChatPanel.tsx:57`, which Task 3 deletes.
    - Plan 02 (`…-plan-02-isolation-and-lifecycle.md:1900`): `endStaleAssistantSessions = () => invoke<number>(...)`.
    - Plan 03 (`…-plan-03-workspace-read-tools-and-modes.md:3402-3405`, `:4111-4119`): `AssistantMode`, `setAssistantMode`, `startWorkspaceAssistant(agentConfigId, mode, model?)`. Its Task 2 Step 8 (`:3408-3420`) already changes the `AgentAutonomyToggle` label (`:89`) and its test's `LABEL` (`:14`) to "Allow the agent to run requests in this collection".
    - Plan 04 (`…-plan-04-proposals-backend.md:3413-3495`): `AgentProposalStatus`, `AgentProposedRequest` (`name, method, url, headers, queryParams` required), `AgentRequestPatch` (`method, url, headers, queryParams, body, docs`, all optional), `AgentProposedChange` with camelCase `op` values (`createFolder`, `createRequest`, `updateRequest`, `editScript`, `moveItem`, `renameItem`, `setEnvVar`), `editScript.phase: 'preRequest' | 'postResponse' | 'tests'`, no `baseFingerprint`, `AgentProposal.statusMessage?`, `list/accept/rejectAgentProposal`, and the two proposal events, whose resolved `status` is `Exclude<AgentProposalStatus, 'pending'>`.

## Global Constraints

- Plans 01–04 have landed before this plan runs. Every name in the index's "Locked interface contracts" is fact, and so is the TypeScript surface in fact 24. Task 1 Step 1 confirms it; if anything is missing, stop and report instead of adding wrappers here.
- Frontend only. No Rust file changes. If a step seems to need one, stop and report it.
- The agent runs only `yarn tsc --noEmit` and `yarn check`. Tests are written in every task; the `yarn test <pattern>` commands are for the user to run.
- Each task leaves `yarn tsc --noEmit` and `yarn check` green. If `yarn check` reports only import order or formatting, run `yarn biome check --write <the task's files>` and re-check.
- Commits go through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths (`git add <paths>`, `git rm <paths>`), never `git add -A`, `--all` or `.`.
- UI: shadcn/ui primitives from `src/components/ui/` and `lucide-react` icons only. No raw `<button>`, `<input>`, `<select>`, `<form>`, `<dialog>`, no inline SVG. Single-quoted JSX attributes.
- Zustand: narrow selectors, one value or action per `useAssistantStore((s) => ...)` call. Never destructure a whole store at component top level.
- Proposal diffs use Monaco's `DiffEditor` (`@monaco-editor/react`), set up like `src/components/git/DiffViewer.tsx` and loaded with `React.lazy`. No CodeMirror in this plan; `PromptEditor` is Plan 06.
- Event payload fields stay snake_case (`session_id`, `call_id`, `cost_usd`); command arguments stay camelCase.
- One assistant session per webview, owned by `assistant-store`. No tab, pane or collection holds session state after Task 3.
- The default mode is `'edit'` until Plan 06 adds the mode picker. No model is passed at start until Plan 06 adds the model picker.
- The focus is stored (`setFocus`) but never sent to the agent in this plan. Plan 06 sends it as a masked resource. Never send raw request text, because it can hold credentials.
- No `.at()` or `findLast` (ES2020 lib). No literal unwrap call text anywhere.

## Review Focus

- **A start that resolves after it was abandoned.** `start_workspace_assistant` can resolve after End session, after a workspace switch, or after a newer start. That session must not become active, and its backend process must be ended, or a credentialed agent is orphaned. Task 1 tests: `refuses to activate a start that a newer start replaced`, `refuses to activate a start that was ended while starting` (store) and `ends the backend session when the start was abandoned` (assistant-session).
- **The startup sweep racing a start.** `end_stale_assistant_sessions` ends every session the backend tracks. A start issued while the sweep is in flight would be killed by it. Task 1 test: `waits for the stale-session sweep before starting`. Also, StrictMode mounts the bridge twice: `sweeps stale backend sessions once per webview load`.
- **Late or foreign events.** A chunk, tool update, usage update or proposal list that arrives for another session id, or after the session ended, must be dropped. Task 1 tests: `ignores events for another session` (store and bridge) and `drops proposals that arrive after the session ended` (bridge).
- **One switch per collection in a popover.** `AgentAutonomyToggle` used a fixed element id. Rendered once per collection, every label would point at the first switch, and turning one on must not close the popover through its confirm dialog. Task 2 tests: `lists every collection with its own run switch` and `asks before letting the agent run requests in one collection`.
- **Accepting a change to a request that has unsaved edits.** The open dirty tab would later autosave its stale copy over the accepted change (fact 4). Accept is disabled while any live or parked tab of that request (or of a moved or renamed folder) is dirty, and clean open tabs are reloaded after Accept. Task 3 tests: `disables Accept while the request has unsaved edits in an open tab`, `finds unsaved edits in a tab parked after a collection switch`, `finds unsaved edits inside a folder that is being moved` and `accept stores the result and reloads a clean open tab of that request`.

## Interface deviations

Names in the index are unchanged. These are additions and signature details the index left open; the index should be updated to match.

1. `beginSession(agentConfigId, mode)` returns a start token (`number`), and `activateSession(token, sessionId, configOptions)` returns `boolean`. The token is what makes an abandoned start detectable (Review Focus 1).
2. New action `failStart(token, error)`. A failed start needs its own transition from `'starting'` to `'error'`.
3. Event-driven actions take the session id first and drop anything for another or ended session: `appendChunk(sessionId, text)`, `completeMessage(sessionId)`, `failMessage(sessionId, error)`, `upsertToolActivity(sessionId, activity)`, `setConfigOptions(sessionId, options)`, `setUsage(sessionId, usage)`, `resolveProposal(sessionId, proposalId, status)`. `upsertProposal(proposal)` uses `proposal.sessionId`.
4. `appendUserMessage(text)` also opens the streaming reply and returns `false` while a turn runs, which is the double-send guard. `endSession(notice?)` adds an optional notice line.
5. `AssistantMessage` is a union of `user`, `agent`, `tool` and `notice` items. The `notice` kind carries the "workspace changed" message.
6. `end_stale_assistant_sessions` is called once per webview load from the bridge (mounted in `App.tsx`), not on the first panel mount, and every start awaits it. This still meets Plan 02's requirement that it runs before any `start_workspace_assistant`.
7. `layout-store` (not `assistant-store`) gains `assistantPanelWidth` and `setAssistantPanelWidth`.
8. The TypeScript names for Plans 01–04, which the index does not lock, are taken from those plan files (fact 24). Notable: `AssistantMode` comes from Plan 03's `tauri-api.ts` and the store re-exports it; Plan 04's DTO carries no before text, so the diff's before side is fetched with `getRequest`.

---

## Task 1: `assistant-store`, session flows and the app-lifetime event bridge

**Files:**
- Create: `src/stores/assistant-store.ts`
- Create: `src/lib/assistant/assistant-session.ts`
- Create: `src/lib/assistant-event-bridge.ts`
- Create: `src/test/assistant-fixtures.ts`
- Create: `src/stores/__tests__/assistant-store.test.ts`
- Create: `src/lib/assistant/__tests__/assistant-session.test.ts`
- Create: `src/lib/__tests__/assistant-event-bridge.test.ts`
- Modify: `src/App.tsx:15-16` (import) and `:41` (mount)

**Interfaces:**
- Consumes (from Plans 01–04, in `src/lib/tauri-api.ts`; names as written in those plan files, see fact 24):
  ```ts
  // Plan 01
  interface ConfigChoice { value: string; name: string; description: string | null }
  interface ConfigOption { id: string; name: string; category: string | null; currentValue: string; choices: ConfigChoice[] }
  interface AgentSessionStarted { sessionId: string; configOptions: ConfigOption[] }
  sendAgentPrompt(sessionId: string, prompt: string, resources?: PromptResourceDto[]): Promise<string>;
  cancelAgentPrompt(sessionId: string): Promise<void>;
  type AgentToolCallStatus = 'pending' | 'in_progress' | 'completed' | 'failed';
  onAgentToolActivity(h: (e: { session_id; call_id; title; status: AgentToolCallStatus }) => void): Promise<UnlistenFn>;
  onAgentConfigOptions(h: (e: { session_id; options: AgentConfigOptionPayload[] }) => void): Promise<UnlistenFn>;
  configOptionsFromEvent(options: AgentConfigOptionPayload[]): ConfigOption[];
  onAgentUsage(h: (e: { session_id; used; size; cost_usd: number | null }) => void): Promise<UnlistenFn>;
  // Plan 02
  endStaleAssistantSessions(): Promise<number>;
  // Plan 03
  type AssistantMode = 'ask' | 'edit' | 'agent';
  startWorkspaceAssistant(agentConfigId: string, mode: AssistantMode, model?: string): Promise<AgentSessionStarted>;
  // Plan 04
  type AgentProposalStatus = 'pending' | 'accepted' | 'rejected' | 'stale' | 'failed';
  type AgentProposedChange = { op: 'createFolder' | 'createRequest' | 'updateRequest' | 'editScript' | 'moveItem' | 'renameItem' | 'setEnvVar'; ... };
  interface AgentProposal { id; sessionId; change: AgentProposedChange; summary; status: AgentProposalStatus; statusMessage?: string; createdAtMs: number }
  listAgentProposals(sessionId: string): Promise<AgentProposal[]>;
  onAgentProposalCreated(h: (e: { session_id; proposal_id; summary }) => void): Promise<UnlistenFn>;
  onAgentProposalResolved(h: (e: { session_id; proposal_id; status: Exclude<AgentProposalStatus, 'pending'> }) => void): Promise<UnlistenFn>;
  // existing
  endAgentSession(sessionId: string): Promise<void>;
  onAgentSessionChunk / onAgentSessionFinished / onAgentSessionFailed;
  ```
  `useWorkspaceStore` (`activeWorkspaceId`), `createDeferred` (`src/test/deferred.ts`).
- Produces:
  ```ts
  // src/stores/assistant-store.ts
  export type { AssistantMode };                                  // re-exported from tauri-api
  export type ToolActivityStatus = AgentToolCallStatus;
  export type ProposalStatus = AgentProposalStatus;
  export type AssistantMessage = UserMessage | AgentMessage | ToolActivityMessage | NoticeMessage;
  export const useAssistantStore;                                // state and actions exactly as in Step 5
  export function selectTurnRunning(state: Pick<AssistantState, 'messages'>): boolean;
  // src/lib/assistant/assistant-session.ts
  export const DEFAULT_ASSISTANT_MODE: AssistantMode;          // 'edit'
  export const WORKSPACE_SWITCH_NOTICE: string;
  export function sweepStaleAssistantSessions(): Promise<void>;
  export function resetStaleSweepForTests(): void;
  export function startAssistant(agentConfigId: string, mode?: AssistantMode): Promise<void>;
  export function sendAssistantMessage(text: string): Promise<void>;
  export function stopAssistantTurn(): Promise<void>;
  export function endAssistantSession(notice?: string): Promise<void>;
  // src/lib/assistant-event-bridge.ts
  export function useAssistantEventBridge(): void;
  // src/test/assistant-fixtures.ts
  export function makeProposal(overrides?: Partial<AgentProposal>): AgentProposal;
  export function makeRequest(overrides?: Partial<Request>): Request;
  ```

- [ ] **Step 1: Confirm the TypeScript surface from Plans 01–04**

Run:

```bash
grep -nE "export (const|interface|type|function) (ConfigChoice|ConfigOption|AgentSessionStarted|sendAgentPrompt|cancelAgentPrompt|AgentToolCallStatus|onAgentToolActivity|onAgentConfigOptions|configOptionsFromEvent|onAgentUsage|endStaleAssistantSessions|AssistantMode|startWorkspaceAssistant|AgentProposalStatus|AgentProposedChange|AgentProposal|listAgentProposals|acceptAgentProposal|rejectAgentProposal|onAgentProposalCreated|onAgentProposalResolved|endAgentSession)\b" src/lib/tauri-api.ts
```

Expected: one line for each of the 22 names. Then read `AgentProposedChange` and `AgentProposal` and confirm: `op` values are camelCase (`editScript`, `updateRequest`, ...), `editScript.phase` is `'preRequest' | 'postResponse' | 'tests'`, there is no `baseFingerprint` in the DTO, and the failure text is `statusMessage`.

If a name is missing or a shape differs, stop and report it: it means an earlier plan did not land as written, and this plan must not paper over that by adding its own wrappers. The only Plan 05 modules that read `change` fields are `src/lib/assistant/proposal-view.ts` and `src/lib/assistant/proposal-actions.ts` (Task 3), so a later rename touches only them and the test literals.

- [ ] **Step 2: Write the shared test fixtures**

Create `src/test/assistant-fixtures.ts`:

```ts
import type { AgentProposal, Request } from '@/lib/tauri-api';

// Shared fixtures for the AI Assistant tests.
export function makeProposal(overrides: Partial<AgentProposal> = {}): AgentProposal {
  return {
    id: 'p1',
    sessionId: 's1',
    summary: 'Add a status test',
    status: 'pending',
    createdAtMs: 1,
    change: {
      op: 'editScript',
      collection: 'orders',
      requestPath: 'get.yml',
      phase: 'tests',
      body: "rok.test('status', () => {});",
    },
    ...overrides,
  };
}

export function makeRequest(overrides: Partial<Request> = {}): Request {
  return {
    uid: 'u1',
    name: 'Get order',
    method: 'GET',
    url: 'https://api.test/orders/1',
    headers: [],
    auth: { authType: 'none' },
    ...overrides,
  };
}
```

- [ ] **Step 3: Write the failing store test**

Create `src/stores/__tests__/assistant-store.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { makeProposal } from '@/test/assistant-fixtures';
import { type AssistantMessage, selectTurnRunning, useAssistantStore } from '../assistant-store';

const store = () => useAssistantStore.getState();

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

function lastMessage(): AssistantMessage | undefined {
  const { messages } = store();
  return messages[messages.length - 1];
}

describe('assistant-store', () => {
  beforeEach(() => {
    store().reset();
    useAssistantStore.setState({ panelOpen: false, focus: undefined });
  });

  it('opens and closes the panel and keeps the focus', () => {
    store().openPanel();
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    expect(store().panelOpen).toBe(true);
    store().closePanel();
    expect(store().panelOpen).toBe(false);
    expect(store().focus).toEqual({ collection: 'orders', path: 'get.yml' });
  });

  it('activates the session started with the current token', () => {
    const token = store().beginSession('agent-1', 'ask');
    expect(store().session).toMatchObject({ status: 'starting', sessionId: '', mode: 'ask' });
    expect(store().activateSession(token, 's1', [])).toBe(true);
    expect(store().session).toMatchObject({ status: 'active', sessionId: 's1' });
  });

  it('refuses to activate a start that a newer start replaced', () => {
    const first = store().beginSession('agent-1', 'edit');
    const second = store().beginSession('agent-2', 'edit');
    expect(store().activateSession(first, 'old', [])).toBe(false);
    expect(store().activateSession(second, 'new', [])).toBe(true);
    expect(store().session?.sessionId).toBe('new');
  });

  it('refuses to activate a start that was ended while starting', () => {
    const token = store().beginSession('agent-1', 'edit');
    store().endSession();
    expect(store().activateSession(token, 's1', [])).toBe(false);
    expect(store().session?.status).toBe('ended');
  });

  it('records a failed start', () => {
    const token = store().beginSession('agent-1', 'edit');
    store().failStart(token, 'command not found');
    expect(store().session).toMatchObject({ status: 'error', error: 'command not found' });
  });

  it('opens one turn at a time', () => {
    activate();
    expect(store().appendUserMessage('one')).toBe(true);
    expect(store().appendUserMessage('two')).toBe(false);
    expect(store().messages.map((m) => m.kind)).toEqual(['user', 'agent']);
    expect(selectTurnRunning(store())).toBe(true);
  });

  it('does not open a turn without an active session', () => {
    expect(store().appendUserMessage('hi')).toBe(false);
    expect(store().messages).toEqual([]);
  });

  it('streams chunks into the reply and continues after a tool line', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendChunk('s1', 'Let me look. ');
    store().upsertToolActivity('s1', {
      callId: 'c1',
      title: 'Reading GET /orders',
      status: 'in_progress',
    });
    store().appendChunk('s1', 'Found it.');
    expect(store().messages.map((m) => m.kind)).toEqual(['user', 'agent', 'tool', 'agent']);
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: 'Found it.', streaming: true });
  });

  it('updates a tool line by call id and keeps its title', () => {
    activate();
    store().appendUserMessage('hi');
    store().upsertToolActivity('s1', { callId: 'c1', title: 'Running Login', status: 'pending' });
    store().upsertToolActivity('s1', { callId: 'c1', title: '', status: 'completed' });
    const tools = store().messages.filter((m) => m.kind === 'tool');
    expect(tools).toHaveLength(1);
    expect(tools[0]).toMatchObject({ title: 'Running Login', status: 'completed' });
  });

  it('ignores events for another session', () => {
    activate('s1');
    store().appendUserMessage('hi');
    store().appendChunk('other', 'x');
    store().upsertToolActivity('other', { callId: 'c1', title: 'Reading', status: 'pending' });
    store().setUsage('other', { used: 1, size: 2 });
    store().upsertProposal(makeProposal({ sessionId: 'other' }));
    store().completeMessage('other');
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: '', streaming: true });
    expect(store().usage).toBeUndefined();
    expect(store().proposals).toEqual([]);
  });

  it('completes the turn', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendChunk('s1', 'Done.');
    store().completeMessage('s1');
    expect(selectTurnRunning(store())).toBe(false);
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: 'Done.', streaming: false });
  });

  it('fails the turn with the error on the last reply segment', () => {
    activate();
    store().appendUserMessage('hi');
    store().appendChunk('s1', 'partial');
    store().failMessage('s1', 'agent crashed');
    expect(store().session).toMatchObject({ status: 'error', error: 'agent crashed' });
    expect(lastMessage()).toMatchObject({
      kind: 'agent',
      text: 'partial',
      streaming: false,
      error: 'agent crashed',
    });
  });

  it('ends the session, discards proposals and adds the notice', () => {
    activate();
    store().appendUserMessage('hi');
    store().upsertProposal(makeProposal());
    store().endSession('Workspace changed.');
    expect(store().session?.status).toBe('ended');
    expect(store().proposals).toEqual([]);
    expect(selectTurnRunning(store())).toBe(false);
    expect(lastMessage()).toMatchObject({ kind: 'notice', text: 'Workspace changed.' });
  });

  it('upserts proposals by id and resolves their status', () => {
    activate();
    store().upsertProposal(makeProposal());
    store().upsertProposal(makeProposal({ summary: 'Changed summary' }));
    expect(store().proposals).toHaveLength(1);
    expect(store().proposals[0].summary).toBe('Changed summary');
    store().resolveProposal('s1', 'p1', 'rejected');
    expect(store().proposals[0].status).toBe('rejected');
  });

  it('stores config options and usage for the active session', () => {
    activate();
    store().setConfigOptions('s1', [
      { id: 'model', name: 'Model', category: 'model', currentValue: 'opus', choices: [] },
    ]);
    store().setUsage('s1', { used: 1200, size: 200000, costUsd: 0.02 });
    expect(store().session?.configOptions[0].currentValue).toBe('opus');
    expect(store().usage).toEqual({ used: 1200, size: 200000, costUsd: 0.02 });
  });

  it('starts a new session with an empty conversation', () => {
    activate();
    store().appendUserMessage('hi');
    store().endSession();
    store().beginSession('agent-1', 'edit');
    expect(store().messages).toEqual([]);
    expect(store().usage).toBeUndefined();
  });
});
```

- [ ] **Step 4: Run it to see it fail (for the user)**

For the user to run: `yarn test assistant-store`
Expected: FAIL, `Failed to resolve import "../assistant-store"`.

- [ ] **Step 5: Implement the store**

Create `src/stores/assistant-store.ts`:

```ts
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
  failMessage: (sessionId: string, error: string) => void;
  upsertToolActivity: (sessionId: string, activity: ToolActivity) => void;
  setConfigOptions: (sessionId: string, options: ConfigOption[]) => void;
  setUsage: (sessionId: string, usage: AssistantUsage) => void;
  upsertProposal: (proposal: AgentProposal) => void;
  resolveProposal: (sessionId: string, proposalId: string, status: ProposalStatus) => void;
  /** Ends the session in the UI and discards its proposals. */
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

  failMessage(sessionId, error) {
    set((state) => {
      if (!state.session || !isCurrent(state, sessionId)) return state;
      return {
        session: { ...state.session, status: 'error', error },
        messages: settleStreaming(state.messages, error),
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

  setUsage(sessionId, usage) {
    set((state) => (isCurrent(state, sessionId) ? { usage } : state));
  },

  upsertProposal(proposal) {
    set((state) => {
      if (!isCurrent(state, proposal.sessionId)) return state;
      const index = state.proposals.findIndex((p) => p.id === proposal.id);
      if (index === -1) return { proposals: [...state.proposals, proposal] };
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
    });
  },

  reset() {
    currentStartToken = 0;
    set({ session: undefined, messages: [], proposals: [], usage: undefined });
  },
}));
```

- [ ] **Step 6: Write the failing session-flow test**

Create `src/lib/assistant/__tests__/assistant-session.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import type { AgentSessionStarted } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { createDeferred } from '@/test/deferred';
import {
  endAssistantSession,
  resetStaleSweepForTests,
  sendAssistantMessage,
  startAssistant,
  stopAssistantTurn,
} from '../assistant-session';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  startWorkspaceAssistant: vi.fn(),
  sendAgentPrompt: vi.fn(),
  cancelAgentPrompt: vi.fn(),
  endAgentSession: vi.fn(),
  endStaleAssistantSessions: vi.fn(),
}));

const store = () => useAssistantStore.getState();

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

describe('assistant session flows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStaleSweepForTests();
    store().reset();
    vi.mocked(api.endStaleAssistantSessions).mockResolvedValue(0);
    vi.mocked(api.endAgentSession).mockResolvedValue(undefined);
    vi.mocked(api.cancelAgentPrompt).mockResolvedValue(undefined);
  });

  it('starts a session and activates it with the reported options', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({
      sessionId: 's1',
      configOptions: [
        { id: 'model', name: 'Model', category: 'model', currentValue: 'opus', choices: [] },
      ],
    });
    await startAssistant('agent-1');
    expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'edit');
    expect(store().session).toMatchObject({ status: 'active', sessionId: 's1', mode: 'edit' });
    expect(store().session?.configOptions[0].currentValue).toBe('opus');
  });

  it('waits for the stale-session sweep before starting', async () => {
    const sweep = createDeferred<number>();
    vi.mocked(api.endStaleAssistantSessions).mockReturnValue(sweep.promise);
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's1', configOptions: [] });
    const started = startAssistant('agent-1');
    await Promise.resolve();
    await Promise.resolve();
    expect(api.startWorkspaceAssistant).not.toHaveBeenCalled();
    sweep.resolve(2);
    await started;
    expect(api.startWorkspaceAssistant).toHaveBeenCalledTimes(1);
    expect(store().session?.status).toBe('active');
  });

  it('still starts when the sweep fails', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.mocked(api.endStaleAssistantSessions).mockRejectedValue(new Error('no backend'));
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's1', configOptions: [] });
    await startAssistant('agent-1');
    expect(store().session?.status).toBe('active');
  });

  it('ends the backend session when the start was abandoned', async () => {
    const start = createDeferred<AgentSessionStarted>();
    vi.mocked(api.startWorkspaceAssistant).mockReturnValue(start.promise);
    const started = startAssistant('agent-1');
    await endAssistantSession();
    expect(api.endAgentSession).not.toHaveBeenCalled();
    start.resolve({ sessionId: 'orphan', configOptions: [] });
    await started;
    expect(api.endAgentSession).toHaveBeenCalledWith('orphan');
    expect(store().session?.status).toBe('ended');
  });

  it('shows the error when the start fails', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockRejectedValue('agent not found');
    await startAssistant('agent-1');
    expect(store().session).toMatchObject({ status: 'error', error: 'agent not found' });
  });

  it('sends one prompt per turn', async () => {
    activate();
    vi.mocked(api.sendAgentPrompt).mockReturnValue(new Promise<string>(() => undefined));
    void sendAssistantMessage('  hi  ');
    void sendAssistantMessage('again');
    await Promise.resolve();
    expect(api.sendAgentPrompt).toHaveBeenCalledTimes(1);
    expect(api.sendAgentPrompt).toHaveBeenCalledWith('s1', 'hi');
  });

  it('ignores an empty message', async () => {
    activate();
    await sendAssistantMessage('   ');
    expect(api.sendAgentPrompt).not.toHaveBeenCalled();
    expect(store().messages).toEqual([]);
  });

  it('fails the reply when the prompt call throws', async () => {
    activate();
    vi.mocked(api.sendAgentPrompt).mockRejectedValue('agent exited');
    await sendAssistantMessage('hi');
    const { messages } = store();
    expect(messages[messages.length - 1]).toMatchObject({
      kind: 'agent',
      streaming: false,
      error: 'agent exited',
    });
    expect(store().session?.status).toBe('error');
  });

  it('stops a running turn', async () => {
    activate();
    store().appendUserMessage('hi');
    await stopAssistantTurn();
    expect(api.cancelAgentPrompt).toHaveBeenCalledWith('s1');
  });

  it('does not stop when no turn runs', async () => {
    activate();
    await stopAssistantTurn();
    expect(api.cancelAgentPrompt).not.toHaveBeenCalled();
  });

  it('ends an active session', async () => {
    activate();
    await endAssistantSession('Bye.');
    expect(api.endAgentSession).toHaveBeenCalledWith('s1');
    expect(store().session?.status).toBe('ended');
  });
});
```

- [ ] **Step 7: Implement the session flows**

Create `src/lib/assistant/assistant-session.ts`:

```ts
import {
  cancelAgentPrompt,
  endAgentSession,
  endStaleAssistantSessions,
  sendAgentPrompt,
  startWorkspaceAssistant,
} from '@/lib/tauri-api';
import { type AssistantMode, selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/** Mode for a new session until the composer (Plan 06) adds the picker. */
export const DEFAULT_ASSISTANT_MODE: AssistantMode = 'edit';

export const WORKSPACE_SWITCH_NOTICE =
  'The workspace changed, so the assistant session ended and its pending proposals were discarded.';

let staleSweep: Promise<void> | null = null;

/**
 * Ends backend assistant sessions left over from an earlier webview load.
 * Runs once per load, before any new session starts.
 */
export function sweepStaleAssistantSessions(): Promise<void> {
  if (!staleSweep) {
    staleSweep = Promise.resolve()
      .then(async () => {
        await endStaleAssistantSessions();
      })
      .catch((err) => {
        console.error('[assistant] stale session sweep failed', err);
      });
  }
  return staleSweep;
}

/** Lets a test run the once-per-load sweep again. */
export function resetStaleSweepForTests(): void {
  staleSweep = null;
}

function endInBackground(sessionId: string, what: string): void {
  Promise.resolve(endAgentSession(sessionId)).catch((err) => {
    console.error(`[assistant] failed to end ${what}`, err);
  });
}

export async function startAssistant(
  agentConfigId: string,
  mode: AssistantMode = DEFAULT_ASSISTANT_MODE,
): Promise<void> {
  const token = useAssistantStore.getState().beginSession(agentConfigId, mode);
  try {
    // A sweep still in flight would end the session started below.
    await sweepStaleAssistantSessions();
    const started = await startWorkspaceAssistant(agentConfigId, mode);
    const applied = useAssistantStore
      .getState()
      .activateSession(token, started.sessionId, started.configOptions);
    // The start was abandoned (End session, a workspace switch or a newer
    // start), so nothing else would ever end this agent process.
    if (!applied) endInBackground(started.sessionId, 'an abandoned session');
  } catch (err) {
    useAssistantStore.getState().failStart(token, String(err));
  }
}

export async function sendAssistantMessage(text: string): Promise<void> {
  const trimmed = text.trim();
  const store = useAssistantStore.getState();
  const session = store.session;
  if (!trimmed || session?.status !== 'active') return;
  // The store refuses a second turn while one runs, so a double send stops here.
  if (!store.appendUserMessage(trimmed)) return;
  try {
    await sendAgentPrompt(session.sessionId, trimmed);
  } catch (err) {
    useAssistantStore.getState().failMessage(session.sessionId, String(err));
  }
}

export async function stopAssistantTurn(): Promise<void> {
  const state = useAssistantStore.getState();
  const session = state.session;
  if (session?.status !== 'active' || !selectTurnRunning(state)) return;
  try {
    // The turn then finishes with a 'cancelled' stop reason, through the bridge.
    await cancelAgentPrompt(session.sessionId);
  } catch (err) {
    console.error('[assistant] failed to stop the turn', err);
  }
}

export async function endAssistantSession(notice?: string): Promise<void> {
  const session = useAssistantStore.getState().session;
  if (!session || session.status === 'ended') return;
  useAssistantStore.getState().endSession(notice);
  // A session still starting has no id yet. startAssistant ends it when the start resolves.
  if (!session.sessionId) return;
  try {
    await endAgentSession(session.sessionId);
  } catch (err) {
    console.error('[assistant] failed to end the session', err);
  }
}
```

- [ ] **Step 8: Write the failing bridge test**

Create `src/lib/__tests__/assistant-event-bridge.test.ts`:

```ts
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  resetStaleSweepForTests,
  WORKSPACE_SWITCH_NOTICE,
} from '@/lib/assistant/assistant-session';
import type { AgentProposal } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { makeProposal } from '@/test/assistant-fixtures';
import { createDeferred } from '@/test/deferred';
import { useAssistantEventBridge } from '../assistant-event-bridge';

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
      onAgentSessionChunk: listener('chunk'),
      onAgentSessionFinished: listener('finished'),
      onAgentSessionFailed: listener('failed'),
      onAgentToolActivity: listener('toolActivity'),
      onAgentConfigOptions: listener('configOptions'),
      onAgentUsage: listener('usage'),
      onAgentProposalCreated: listener('proposalCreated'),
      onAgentProposalResolved: listener('proposalResolved'),
      listAgentProposals: vi.fn(),
      endAgentSession: vi.fn(),
      endStaleAssistantSessions: vi.fn(),
    },
  };
});

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  ...mocks.api,
}));

const store = () => useAssistantStore.getState();

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

async function mountBridge(): Promise<void> {
  renderHook(() => useAssistantEventBridge());
  await waitFor(() => expect(mocks.handlers.proposalResolved).toBeDefined());
}

function emit(name: string, payload: unknown): void {
  const handler = mocks.handlers[name];
  if (!handler) throw new Error(`no ${name} listener`);
  handler(payload);
}

function lastMessage() {
  const { messages } = store();
  return messages[messages.length - 1];
}

describe('useAssistantEventBridge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const key of Object.keys(mocks.handlers)) delete mocks.handlers[key];
    resetStaleSweepForTests();
    store().reset();
    useAssistantStore.setState({ panelOpen: false, focus: undefined });
    useWorkspaceStore.setState({ activeWorkspaceId: '' });
    mocks.api.endStaleAssistantSessions.mockResolvedValue(0);
    mocks.api.endAgentSession.mockResolvedValue(undefined);
  });

  it('streams chunks and finishes the turn of the active session', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    emit('chunk', { session_id: 's1', text: 'Hello' });
    emit('finished', { session_id: 's1', stop_reason: 'end_turn' });
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: 'Hello', streaming: false });
  });

  it('ignores events for another session', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    emit('chunk', { session_id: 'other', text: 'Hello' });
    emit('failed', { session_id: 'other', error: 'boom' });
    expect(lastMessage()).toMatchObject({ kind: 'agent', text: '', streaming: true });
    expect(store().session?.status).toBe('active');
  });

  it('fails the turn on a failed event', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    emit('failed', { session_id: 's1', error: 'idle timeout' });
    expect(store().session).toMatchObject({ status: 'error', error: 'idle timeout' });
  });

  it('adds and updates a tool activity line', async () => {
    activate();
    store().appendUserMessage('hi');
    await mountBridge();
    emit('toolActivity', {
      session_id: 's1',
      call_id: 'c1',
      title: 'Running Login',
      status: 'in_progress',
    });
    emit('toolActivity', { session_id: 's1', call_id: 'c1', title: '', status: 'completed' });
    const tools = store().messages.filter((m) => m.kind === 'tool');
    expect(tools).toEqual([
      expect.objectContaining({ callId: 'c1', title: 'Running Login', status: 'completed' }),
    ]);
  });

  it('converts the snake_case config options of the event', async () => {
    activate();
    await mountBridge();
    emit('configOptions', {
      session_id: 's1',
      options: [
        {
          id: 'model',
          name: 'Model',
          category: 'model',
          current_value: 'opus',
          choices: [{ value: 'opus', name: 'Opus', description: null }],
        },
        { id: 'effort', name: 'Effort', category: 'thought_level', current_value: 'high', choices: [] },
      ],
    });
    expect(store().session?.configOptions.map((o) => o.currentValue)).toEqual(['opus', 'high']);
  });

  it('stores usage', async () => {
    activate();
    await mountBridge();
    emit('usage', { session_id: 's1', used: 1200, size: 200000, cost_usd: null });
    expect(store().usage).toEqual({ used: 1200, size: 200000, costUsd: undefined });
  });

  it('loads the session proposals when one is created', async () => {
    activate();
    mocks.api.listAgentProposals.mockResolvedValue([makeProposal()]);
    await mountBridge();
    emit('proposalCreated', { session_id: 's1', proposal_id: 'p1', summary: 'Add a status test' });
    await waitFor(() => expect(store().proposals).toHaveLength(1));
    expect(mocks.api.listAgentProposals).toHaveBeenCalledWith('s1');
  });

  it('drops proposals that arrive after the session ended', async () => {
    activate();
    const pending = createDeferred<AgentProposal[]>();
    mocks.api.listAgentProposals.mockReturnValue(pending.promise);
    await mountBridge();
    emit('proposalCreated', { session_id: 's1', proposal_id: 'p1', summary: 'Add a status test' });
    store().endSession();
    pending.resolve([makeProposal()]);
    await pending.promise;
    await Promise.resolve();
    expect(store().proposals).toEqual([]);
  });

  it('applies a resolved status', async () => {
    activate();
    store().upsertProposal(makeProposal());
    await mountBridge();
    emit('proposalResolved', { session_id: 's1', proposal_id: 'p1', status: 'stale' });
    expect(store().proposals[0].status).toBe('stale');
  });

  it('ends the session and clears the focus when the workspace changes', async () => {
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws-1' });
    activate();
    store().setFocus({ collection: 'orders', path: 'get.yml' });
    await mountBridge();
    useWorkspaceStore.getState().setActiveWorkspaceId('ws-2');
    await waitFor(() => expect(mocks.api.endAgentSession).toHaveBeenCalledWith('s1'));
    expect(store().session?.status).toBe('ended');
    expect(store().focus).toBeUndefined();
    expect(lastMessage()).toMatchObject({ kind: 'notice', text: WORKSPACE_SWITCH_NOTICE });
  });

  it('keeps the session when the first workspace id is set at startup', async () => {
    activate();
    await mountBridge();
    useWorkspaceStore.getState().setActiveWorkspaceId('ws-1');
    expect(store().session?.status).toBe('active');
    expect(mocks.api.endAgentSession).not.toHaveBeenCalled();
  });

  it('sweeps stale backend sessions once per webview load', async () => {
    const first = renderHook(() => useAssistantEventBridge());
    first.unmount();
    renderHook(() => useAssistantEventBridge());
    await waitFor(() => expect(mocks.api.endStaleAssistantSessions).toHaveBeenCalledTimes(1));
  });
});
```

- [ ] **Step 9: Implement the bridge**

Create `src/lib/assistant-event-bridge.ts`:

```ts
import { useEffect } from 'react';
import {
  endAssistantSession,
  sweepStaleAssistantSessions,
  WORKSPACE_SWITCH_NOTICE,
} from '@/lib/assistant/assistant-session';
import {
  configOptionsFromEvent,
  listAgentProposals,
  onAgentConfigOptions,
  onAgentProposalCreated,
  onAgentProposalResolved,
  onAgentSessionChunk,
  onAgentSessionFailed,
  onAgentSessionFinished,
  onAgentToolActivity,
  onAgentUsage,
} from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

// The created event names one proposal. The list carries the full DTOs.
async function refreshProposals(sessionId: string): Promise<void> {
  if (useAssistantStore.getState().session?.sessionId !== sessionId) return;
  try {
    const proposals = await listAgentProposals(sessionId);
    // upsertProposal drops them when the session ended in the meantime.
    for (const proposal of proposals) useAssistantStore.getState().upsertProposal(proposal);
  } catch (err) {
    console.error('[assistant] failed to load proposals', err);
  }
}

/**
 * Subscribes once, for the app's lifetime, to every assistant event and
 * routes it into assistant-store by session id. It also ends the session when
 * the active workspace changes, and sweeps sessions left by an earlier load.
 */
export function useAssistantEventBridge(): void {
  useEffect(() => {
    void sweepStaleAssistantSessions();
    const store = () => useAssistantStore.getState();

    const unsubs = Promise.all([
      onAgentSessionChunk((e) => store().appendChunk(e.session_id, e.text)),
      onAgentSessionFinished((e) => store().completeMessage(e.session_id)),
      onAgentSessionFailed((e) => store().failMessage(e.session_id, e.error)),
      onAgentToolActivity((e) =>
        store().upsertToolActivity(e.session_id, {
          callId: e.call_id,
          title: e.title,
          status: e.status,
        }),
      ),
      // Event options are snake_case. Plan 01's helper converts them.
      onAgentConfigOptions((e) =>
        store().setConfigOptions(e.session_id, configOptionsFromEvent(e.options)),
      ),
      onAgentUsage((e) =>
        store().setUsage(e.session_id, {
          used: e.used,
          size: e.size,
          costUsd: e.cost_usd ?? undefined,
        }),
      ),
      onAgentProposalCreated((e) => {
        void refreshProposals(e.session_id);
      }),
      onAgentProposalResolved((e) =>
        store().resolveProposal(e.session_id, e.proposal_id, e.status),
      ),
    ]);

    // The first id is set during startup. Only a change from one workspace
    // to another ends the session.
    const unsubWorkspace = useWorkspaceStore.subscribe((state, prev) => {
      if (!prev.activeWorkspaceId || state.activeWorkspaceId === prev.activeWorkspaceId) return;
      store().setFocus(undefined);
      void endAssistantSession(WORKSPACE_SWITCH_NOTICE);
    });

    return () => {
      unsubWorkspace();
      void unsubs.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);
}
```

- [ ] **Step 10: Mount the bridge next to the old one**

In `src/App.tsx`, add the import right after the existing `useAgentSessionEventBridge` import (`:15`):

```ts
import { useAgentSessionEventBridge } from '@/lib/agent-session-event-bridge';
import { useAssistantEventBridge } from '@/lib/assistant-event-bridge';
```

and the call right after `useAgentSessionEventBridge();` (`:41`):

```ts
  useAgentSessionEventBridge();
  useAssistantEventBridge();
```

Both bridges coexist until Task 3: the old one routes by tab session ids and the new one by the assistant session id, so no event reaches both.

- [ ] **Step 11: Verify**

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn check`
Expected: PASS. Import order or formatting only: `yarn biome check --write src/stores/assistant-store.ts src/lib/assistant/assistant-session.ts src/lib/assistant-event-bridge.ts src/test/assistant-fixtures.ts src/App.tsx`, then re-run.

For the user to run: `yarn test assistant-store assistant-session assistant-event-bridge`
Expected: PASS, 16 + 11 + 12 tests.

- [ ] **Step 12: Commit**

```bash
git add src/stores/assistant-store.ts src/lib/assistant/assistant-session.ts \
  src/lib/assistant-event-bridge.ts src/test/assistant-fixtures.ts \
  src/stores/__tests__/assistant-store.test.ts \
  src/lib/assistant/__tests__/assistant-session.test.ts \
  src/lib/__tests__/assistant-event-bridge.test.ts src/App.tsx
```

Invoke the `dev-workflow-skills:1-git-commit` skill (Skill tool, skill name `dev-workflow-skills:1-git-commit`) for the staged changes, with the conventional-commit subject `feat: add workspace assistant store and event bridge`. Do not write a freeform `git commit -m`.

---

## Task 2: Docked `AssistantPanel`, title bar toggle and permissions popover

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src/stores/layout-store.ts` (whole file, 28 lines)
- Create: `src/components/assistant/AssistantToggleButton.tsx`
- Create: `src/components/assistant/AssistantPanel.tsx`
- Create: `src/components/assistant/AssistantStartView.tsx`
- Create: `src/components/assistant/AssistantChatView.tsx`
- Create: `src/components/assistant/AssistantInputStub.tsx`
- Create: `src/components/assistant/AssistantPermissionsPopover.tsx`
- Modify: `src/components/request/AgentAutonomyToggle.tsx:1-2`, `:25-30`, `:80-94`, `:100-109`
- Modify: `src/components/request/__tests__/AgentAutonomyToggle.test.tsx:14`, `:55`
- Modify: `src/lib/tauri-api.ts:96-100` (doc comment of `agentAutonomyEnabled`)
- Modify: `src/components/title-bar/TitleBar.tsx:4` (import) and `:52-61` (button)
- Modify: `src/App.tsx` imports (`:4`, `:25`), the `App` body (`:30`) and the main row (`:232-233`)
- Create: `src/components/assistant/__tests__/AssistantPanel.test.tsx`
- Create: `src/components/assistant/__tests__/AssistantPermissionsPopover.test.tsx`

**Interfaces:**
- Consumes: Task 1's `useAssistantStore`, `selectTurnRunning`, `startAssistant`, `sendAssistantMessage`, `stopAssistantTurn`, `endAssistantSession`, `resetStaleSweepForTests`; `useAgentConfigs()` (`src/lib/queries/agent-config-queries.ts:14-19`); `useCollections()` (`src/lib/queries/collection-queries.ts:8-13`); `MarkdownRenderer`; shadcn `Button`, `Select*`, `Textarea`, `ScrollArea`, `Popover*`, `Switch`, `Label`, `AlertDialog*`.
- Produces:
  ```ts
  export function AssistantPanel(): JSX.Element;               // <aside id='assistant-panel'>
  export function AssistantToggleButton(): JSX.Element;        // aria-label 'AI Assistant'
  export function AssistantStartView(): JSX.Element;
  export function AssistantChatView(): JSX.Element;            // Task 3 adds the proposal cards
  export function AssistantInputStub(): JSX.Element;           // Plan 06 replaces it
  export function AssistantPermissionsPopover(): JSX.Element;  // trigger aria-label 'Agent permissions'
  // layout-store additions
  assistantPanelWidth: number;                                  // default 400
  setAssistantPanelWidth: (w: number) => void;
  ```

- [ ] **Step 1: Write the failing panel test**

Create `src/components/assistant/__tests__/AssistantPanel.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { resetStaleSweepForTests } from '@/lib/assistant/assistant-session';
import * as api from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { AssistantPanel } from '../AssistantPanel';
import { AssistantToggleButton } from '../AssistantToggleButton';

// Radix Select calls pointer-capture APIs that jsdom lacks.
if (!Element.prototype.hasPointerCapture) {
  Element.prototype.hasPointerCapture = () => false;
}
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => undefined;
}

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  startWorkspaceAssistant: vi.fn(),
  endStaleAssistantSessions: vi.fn(),
  sendAgentPrompt: vi.fn(),
  cancelAgentPrompt: vi.fn(),
  endAgentSession: vi.fn(),
  getCollectionSettings: vi.fn(),
  saveCollectionSettings: vi.fn(),
}));

vi.mock('@/lib/queries/agent-config-queries', () => ({
  useAgentConfigs: () => ({ data: [{ id: 'agent-1', label: 'Claude' }] }),
}));

vi.mock('@/lib/queries/collection-queries', () => ({
  collectionKeys: { all: ['collections'] },
  useCollections: () => ({ data: [] }),
}));

vi.mock('@/components/collections/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ children }: { children: string }) => <div>{children}</div>,
}));

const store = () => useAssistantStore.getState();

function activate(sessionId = 's1'): void {
  const token = store().beginSession('agent-1', 'edit');
  store().activateSession(token, sessionId, []);
}

describe('AssistantPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetStaleSweepForTests();
    store().reset();
    useAssistantStore.setState({ panelOpen: true, focus: undefined });
    vi.mocked(api.endStaleAssistantSessions).mockResolvedValue(0);
    vi.mocked(api.endAgentSession).mockResolvedValue(undefined);
    vi.mocked(api.cancelAgentPrompt).mockResolvedValue(undefined);
  });

  it('starts a session with the first agent and shows the message box', async () => {
    vi.mocked(api.startWorkspaceAssistant).mockResolvedValue({ sessionId: 's1', configOptions: [] });
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'Start' }));
    await waitFor(() =>
      expect(api.startWorkspaceAssistant).toHaveBeenCalledWith('agent-1', 'edit'),
    );
    expect(
      await screen.findByRole('textbox', { name: 'Message the assistant' }),
    ).toBeInTheDocument();
  });

  it('shows a start error', () => {
    const token = store().beginSession('agent-1', 'edit');
    store().failStart(token, 'agent not found');
    render(<AssistantPanel />);
    expect(screen.getByText('agent not found')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start' })).toBeInTheDocument();
  });

  it('renders replies and one muted line per tool call', () => {
    activate();
    store().appendUserMessage('Why does login fail?');
    store().appendChunk('s1', 'Checking.');
    store().upsertToolActivity('s1', {
      callId: 'c1',
      title: 'Reading GET /orders',
      status: 'in_progress',
    });
    render(<AssistantPanel />);
    expect(screen.getByText('Why does login fail?')).toBeInTheDocument();
    expect(screen.getByText('Checking.')).toBeInTheDocument();
    expect(screen.getByText('Reading GET /orders')).toBeInTheDocument();
    expect(screen.getByText('running')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeInTheDocument();
  });

  it('sends the typed message and clears the box', async () => {
    activate();
    vi.mocked(api.sendAgentPrompt).mockReturnValue(new Promise<string>(() => undefined));
    render(<AssistantPanel />);
    const box = screen.getByRole('textbox', { name: 'Message the assistant' });
    await userEvent.type(box, '  hello  ');
    await userEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect(api.sendAgentPrompt).toHaveBeenCalledWith('s1', 'hello');
    expect(box).toHaveValue('');
  });

  it('sends on Enter and keeps Shift+Enter for a new line', async () => {
    activate();
    vi.mocked(api.sendAgentPrompt).mockReturnValue(new Promise<string>(() => undefined));
    render(<AssistantPanel />);
    const box = screen.getByRole('textbox', { name: 'Message the assistant' });
    await userEvent.type(box, 'line one{Shift>}{Enter}{/Shift}line two{Enter}');
    expect(api.sendAgentPrompt).toHaveBeenCalledWith('s1', 'line one\nline two');
  });

  it('stops a running turn', async () => {
    activate();
    store().appendUserMessage('hi');
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(api.cancelAgentPrompt).toHaveBeenCalledWith('s1');
  });

  it('ends the session and keeps the transcript', async () => {
    activate();
    store().appendUserMessage('keep me');
    store().completeMessage('s1');
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'End session' }));
    expect(api.endAgentSession).toHaveBeenCalledWith('s1');
    expect(screen.getByText('keep me')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start a new session' })).toBeInTheDocument();
  });

  it('shows the notice of an ended session', () => {
    activate();
    store().endSession('The workspace changed.');
    render(<AssistantPanel />);
    expect(screen.getByText('The workspace changed.')).toBeInTheDocument();
  });

  it('closes from its header', async () => {
    render(<AssistantPanel />);
    await userEvent.click(screen.getByRole('button', { name: 'Close AI Assistant' }));
    expect(store().panelOpen).toBe(false);
  });
});

describe('AssistantToggleButton', () => {
  it('opens and closes the panel', async () => {
    useAssistantStore.setState({ panelOpen: false });
    render(<AssistantToggleButton />);
    const button = screen.getByRole('button', { name: 'AI Assistant' });
    expect(button).toHaveAttribute('aria-pressed', 'false');
    await userEvent.click(button);
    expect(useAssistantStore.getState().panelOpen).toBe(true);
    expect(button).toHaveAttribute('aria-pressed', 'true');
    await userEvent.click(button);
    expect(useAssistantStore.getState().panelOpen).toBe(false);
  });
});
```

- [ ] **Step 2: Write the failing popover test and update the toggle test**

Create `src/components/assistant/__tests__/AssistantPermissionsPopover.test.tsx`:

```tsx
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import { AssistantPermissionsPopover } from '../AssistantPermissionsPopover';

const collections = vi.hoisted(() => ({ data: [] as Array<{ name: string }> }));

vi.mock('@/lib/queries/collection-queries', () => ({
  collectionKeys: { all: ['collections'] },
  useCollections: () => collections,
}));

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getCollectionSettings: vi.fn(),
  saveCollectionSettings: vi.fn(),
}));

const LABEL = 'Allow the agent to run requests in this collection';

async function openPopover(): Promise<void> {
  await userEvent.click(screen.getByRole('button', { name: 'Agent permissions' }));
}

describe('AssistantPermissionsPopover', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collections.data = [{ name: 'orders' }, { name: 'billing' }];
    vi.mocked(api.getCollectionSettings).mockResolvedValue({
      headers: [],
      variables: [],
      sandboxMode: 'safe',
    });
    vi.mocked(api.saveCollectionSettings).mockResolvedValue(undefined);
  });

  it('lists every collection with its own run switch', async () => {
    render(<AssistantPermissionsPopover />);
    await openPopover();
    const switches = await screen.findAllByRole('switch', { name: LABEL });
    expect(switches).toHaveLength(2);
    expect(new Set(switches.map((s) => s.id)).size).toBe(2);
    expect(screen.getByRole('region', { name: 'orders' })).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'billing' })).toBeInTheDocument();
  });

  it('asks before letting the agent run requests in one collection', async () => {
    render(<AssistantPermissionsPopover />);
    await openPopover();
    const billing = await screen.findByRole('region', { name: 'billing' });
    const toggle = within(billing).getByRole('switch');
    await waitFor(() => expect(toggle).toBeEnabled());
    await userEvent.click(toggle);
    await userEvent.click(await screen.findByRole('button', { name: 'Allow' }));
    await waitFor(() =>
      expect(api.saveCollectionSettings).toHaveBeenCalledWith(
        'billing',
        expect.objectContaining({ agentAutonomyEnabled: true }),
      ),
    );
    // The confirm dialog must not have closed the popover.
    await waitFor(() =>
      expect(screen.getByRole('region', { name: 'orders' })).toBeInTheDocument(),
    );
  });

  it('says so when the workspace has no collections', async () => {
    collections.data = [];
    render(<AssistantPermissionsPopover />);
    await openPopover();
    expect(await screen.findByText('No collections in this workspace.')).toBeInTheDocument();
  });
});
```

In `src/components/request/__tests__/AgentAutonomyToggle.test.tsx`, Plan 03 (Task 2, Step 8c) already set line 14 to:

```ts
const LABEL = 'Allow the agent to run requests in this collection';
```

Confirm it, and set it to that text if it is not. Then change line 55:

```ts
    expect(
      await screen.findByText('Let the agent run requests in this collection?'),
    ).toBeInTheDocument();
```

- [ ] **Step 3: Run them to see them fail (for the user)**

For the user to run: `yarn test AssistantPanel AssistantPermissionsPopover AgentAutonomyToggle`
Expected: FAIL. The new files fail to resolve `../AssistantPanel` and `../AssistantPermissionsPopover`; `AgentAutonomyToggle` fails to find the new dialog title.

- [ ] **Step 4: Add the panel width to the layout store**

Replace `src/stores/layout-store.ts` with:

```ts
import { create } from 'zustand';

type RequestLayout = 'stacked' | 'side-by-side';

interface LayoutStore {
  requestLayout: RequestLayout;
  sidebarWidth: number;
  isConsoleOpen: boolean;
  consoleHeight: number;
  assistantPanelWidth: number;

  setRequestLayout: (dir: RequestLayout) => void;
  setSidebarWidth: (w: number) => void;
  setConsoleOpen: (open: boolean) => void;
  setConsoleHeight: (h: number) => void;
  setAssistantPanelWidth: (w: number) => void;
}

export const useLayoutStore = create<LayoutStore>()((set) => ({
  requestLayout: 'stacked',
  sidebarWidth: 280,
  isConsoleOpen: false,
  consoleHeight: 280,
  assistantPanelWidth: 400,

  setRequestLayout: (dir) => set({ requestLayout: dir }),
  setSidebarWidth: (w) => set({ sidebarWidth: w }),
  setConsoleOpen: (open) => set({ isConsoleOpen: open }),
  setConsoleHeight: (h) => set({ consoleHeight: h }),
  setAssistantPanelWidth: (w) => set({ assistantPanelWidth: w }),
}));
```

- [ ] **Step 5: Reword and de-duplicate `AgentAutonomyToggle`**

In `src/components/request/AgentAutonomyToggle.tsx`:

Line 2, add `useId`:

```ts
import { useEffect, useId, useState } from 'react';
```

Replace the doc comment and the start of the component (`:25-30`):

```tsx
/**
 * Per-collection switch that lets the AI Assistant send this collection's
 * requests. Reading the workspace and proposing changes is always allowed.
 * Rendered once per collection, so every instance needs its own element id.
 */
export function AgentAutonomyToggle({ collectionName }: AgentAutonomyToggleProps) {
  const switchId = useId();
```

Replace the switch block and the hint (`:80-94`). Plan 03 (Task 2, Step 8b) already changed the label text; this step keeps that text and replaces the fixed id and the hint:

```tsx
    <div className='flex flex-col gap-1.5'>
      <div className='flex items-start gap-2'>
        <Switch
          id={switchId}
          checked={enabled === true}
          disabled={enabled === null || saving}
          onCheckedChange={handleCheckedChange}
        />
        <Label htmlFor={switchId} className='text-xs leading-snug'>
          Allow the agent to run requests in this collection
        </Label>
      </div>
      <p className='text-xs text-muted-foreground'>
        Reading and proposing changes is always allowed.
      </p>
```

Replace the dialog title and description text (`:100-109`):

```tsx
            <AlertDialogTitle className='flex items-center gap-2'>
              <ShieldAlert className='h-4 w-4' aria-hidden='true' />
              Let the agent run requests in this collection?
            </AlertDialogTitle>
            <AlertDialogDescription>
              The agent will be able to send this collection&apos;s requests without asking each
              time. A request it sends can reach any public host, so turn this on only for agents
              and collections you trust.
            </AlertDialogDescription>
```

In `src/lib/tauri-api.ts:96-100`, update the field's doc comment:

```ts
  /**
   * Lets the AI Assistant send this collection's requests. Off by default.
   * Persisted at extensions.rocketapi.agentAutonomyEnabled in opencollection.yml.
   */
```

- [ ] **Step 6: Create the title bar toggle**

Create `src/components/assistant/AssistantToggleButton.tsx`:

```tsx
import { Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useAssistantStore } from '@/stores/assistant-store';

/** Title bar button that shows and hides the docked AI Assistant. */
export function AssistantToggleButton() {
  const panelOpen = useAssistantStore((s) => s.panelOpen);
  const openPanel = useAssistantStore((s) => s.openPanel);
  const closePanel = useAssistantStore((s) => s.closePanel);

  return (
    <Button
      variant='ghost'
      size='icon'
      className='h-7 w-7'
      aria-label='AI Assistant'
      aria-pressed={panelOpen}
      aria-controls='assistant-panel'
      title={panelOpen ? 'Hide AI Assistant' : 'Show AI Assistant'}
      onClick={() => (panelOpen ? closePanel() : openPanel())}
    >
      <Sparkles className='h-4 w-4' aria-hidden='true' />
    </Button>
  );
}
```

In `src/components/title-bar/TitleBar.tsx`, add the import before the `AgentConfigsDialog` import (`:4`):

```ts
import { AssistantToggleButton } from '@/components/assistant/AssistantToggleButton';
import { AgentConfigsDialog } from '@/components/settings/AgentConfigsDialog';
```

and render it right after the Bot button (after `:60`, before `{!isMac && <WindowControls />}`). The Bot button stays and still opens `AgentConfigsDialog`:

```tsx
          <Bot className='h-4 w-4' aria-hidden='true' />
        </Button>
        <AssistantToggleButton />
        {!isMac && <WindowControls />}
```

- [ ] **Step 7: Create the permissions popover**

Create `src/components/assistant/AssistantPermissionsPopover.tsx`:

```tsx
import { ShieldCheck } from 'lucide-react';
import { AgentAutonomyToggle } from '@/components/request/AgentAutonomyToggle';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { useCollections } from '@/lib/queries/collection-queries';

/** Lists the workspace's collections, each with its run switch. */
export function AssistantPermissionsPopover() {
  const { data: collections = [] } = useCollections();

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Agent permissions'
          title='Agent permissions'
        >
          <ShieldCheck className='h-4 w-4' aria-hidden='true' />
        </Button>
      </PopoverTrigger>
      <PopoverContent align='end' className='w-80'>
        <div className='flex flex-col gap-3'>
          <div>
            <p className='text-sm font-medium'>Agent permissions</p>
            <p className='text-xs text-muted-foreground'>
              The assistant can read every collection in this workspace and propose changes.
              Running requests needs the switch below.
            </p>
          </div>
          {collections.length === 0 ? (
            <p className='text-xs text-muted-foreground'>No collections in this workspace.</p>
          ) : (
            <div className='flex max-h-72 flex-col gap-2 overflow-y-auto'>
              {collections.map((c) => (
                <section
                  key={c.name}
                  aria-label={c.name}
                  className='flex flex-col gap-1.5 rounded-md border p-2'
                >
                  <span className='text-xs font-medium'>{c.name}</span>
                  <AgentAutonomyToggle collectionName={c.name} />
                </section>
              ))}
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
```

- [ ] **Step 8: Create the chat view, start view and input stub**

Create `src/components/assistant/AssistantChatView.tsx`:

```tsx
import { Check, Loader2, X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import {
  type AssistantMessage,
  type ToolActivityStatus,
  useAssistantStore,
} from '@/stores/assistant-store';

const TOOL_STATUS_LABEL: Record<ToolActivityStatus, string> = {
  pending: 'waiting',
  in_progress: 'running',
  completed: 'done',
  failed: 'failed',
};

function ToolActivityLine({ title, status }: { title: string; status: ToolActivityStatus }) {
  const Icon = status === 'completed' ? Check : status === 'failed' ? X : Loader2;
  const busy = status === 'pending' || status === 'in_progress';
  return (
    <div className='flex items-center gap-1.5 text-xs text-muted-foreground'>
      <Icon
        className={cn(
          'h-3 w-3 shrink-0',
          busy && 'animate-spin',
          status === 'failed' && 'text-destructive',
        )}
        aria-hidden='true'
      />
      <span className='truncate'>{title}</span>
      <span className='sr-only'>{TOOL_STATUS_LABEL[status]}</span>
    </div>
  );
}

function ChatItem({ message, isLast }: { message: AssistantMessage; isLast: boolean }) {
  switch (message.kind) {
    case 'user':
      return (
        <div className='text-sm'>
          <div className='mb-1 text-xs font-semibold text-muted-foreground'>You</div>
          <p className='whitespace-pre-wrap'>{message.text}</p>
        </div>
      );
    case 'agent': {
      const waiting = message.streaming && isLast;
      // A reply segment that got no text before a tool call adds nothing.
      if (!message.text && !message.error && !waiting) return null;
      return (
        <div className='text-sm'>
          <div className='mb-1 text-xs font-semibold text-muted-foreground'>Assistant</div>
          {message.text && <MarkdownRenderer>{message.text}</MarkdownRenderer>}
          {waiting && (
            <Loader2
              className='h-3 w-3 animate-spin text-muted-foreground'
              aria-label='Assistant is replying'
            />
          )}
          {message.error && <p className='text-xs text-destructive'>{message.error}</p>}
        </div>
      );
    }
    case 'tool':
      return <ToolActivityLine title={message.title} status={message.status} />;
    case 'notice':
      return <p className='text-xs italic text-muted-foreground'>{message.text}</p>;
  }
}

/** The conversation: messages, tool activity lines and notices. */
export function AssistantChatView() {
  const messages = useAssistantStore((s) => s.messages);
  const endRef = useRef<HTMLDivElement>(null);
  const itemCount = messages.length;

  // Keeps the newest item in view.
  useEffect(() => {
    if (itemCount === 0) return;
    endRef.current?.scrollIntoView?.({ block: 'end' });
  }, [itemCount]);

  const lastId = messages[messages.length - 1]?.id;
  return (
    <ScrollArea className='min-h-0 flex-1'>
      <div className='flex flex-col gap-3 p-3'>
        {messages.map((m) => (
          <ChatItem key={m.id} message={m} isLast={m.id === lastId} />
        ))}
        <div ref={endRef} />
      </div>
    </ScrollArea>
  );
}
```

Create `src/components/assistant/AssistantStartView.tsx`:

```tsx
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { startAssistant } from '@/lib/assistant/assistant-session';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
import { cn } from '@/lib/utils';
import { useAssistantStore } from '@/stores/assistant-store';

/** Agent picker and Start button. Shown before a session and after one ends. */
export function AssistantStartView() {
  const { data: agentConfigs = [] } = useAgentConfigs();
  const error = useAssistantStore((s) =>
    s.session?.status === 'error' ? s.session.error : undefined,
  );
  const hasMessages = useAssistantStore((s) => s.messages.length > 0);
  const [selectedId, setSelectedId] = useState('');
  const agentConfigId = selectedId || agentConfigs[0]?.id || '';

  return (
    <div className={cn('flex shrink-0 flex-col gap-3 p-3', hasMessages && 'border-t')}>
      {!hasMessages && (
        <p className='text-xs text-muted-foreground'>
          Ask about this workspace, or let the assistant write scripts and tests and organize
          requests. Every change is a proposal you review before anything is written.
        </p>
      )}
      {error && <p className='text-xs text-destructive'>{error}</p>}
      {agentConfigs.length === 0 ? (
        <p className='text-xs text-muted-foreground'>
          No agents are configured yet. Add one with the agent button in the title bar.
        </p>
      ) : (
        <>
          <Select value={agentConfigId} onValueChange={setSelectedId}>
            <SelectTrigger className='h-8 text-sm' aria-label='Agent'>
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
          <Button
            size='sm'
            disabled={!agentConfigId}
            onClick={() => void startAssistant(agentConfigId)}
          >
            {hasMessages ? 'Start a new session' : 'Start'}
          </Button>
        </>
      )}
    </div>
  );
}
```

Create `src/components/assistant/AssistantInputStub.tsx`:

```tsx
import { Send, Square } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { sendAssistantMessage, stopAssistantTurn } from '@/lib/assistant/assistant-session';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/**
 * Temporary prompt input for the AI Assistant. Plan 06 replaces it with the
 * composer (PromptEditor, chips, pickers). Keep it this small until then.
 */
export function AssistantInputStub() {
  const running = useAssistantStore(selectTurnRunning);
  const [text, setText] = useState('');

  const send = () => {
    if (!text.trim() || running) return;
    const value = text;
    setText('');
    void sendAssistantMessage(value);
  };

  return (
    <div className='flex shrink-0 items-end gap-2 border-t p-2'>
      <Textarea
        aria-label='Message the assistant'
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key !== 'Enter' || e.shiftKey) return;
          // Enter sends here, and must not reach the global send-request shortcut.
          e.preventDefault();
          e.stopPropagation();
          send();
        }}
        placeholder='Ask about this workspace…'
        className='min-h-8 flex-1 resize-none text-sm'
      />
      {running ? (
        <Button
          size='sm'
          variant='outline'
          aria-label='Stop'
          onClick={() => void stopAssistantTurn()}
        >
          <Square className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      ) : (
        <Button size='sm' aria-label='Send' disabled={!text.trim()} onClick={send}>
          <Send className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      )}
    </div>
  );
}
```

- [ ] **Step 9: Create the panel shell**

Create `src/components/assistant/AssistantPanel.tsx`:

```tsx
import { Loader2, Sparkles, X } from 'lucide-react';
import type { CSSProperties } from 'react';
import { Button } from '@/components/ui/button';
import { endAssistantSession } from '@/lib/assistant/assistant-session';
import { useAssistantStore } from '@/stores/assistant-store';
import { useLayoutStore } from '@/stores/layout-store';
import { AssistantChatView } from './AssistantChatView';
import { AssistantInputStub } from './AssistantInputStub';
import { AssistantPermissionsPopover } from './AssistantPermissionsPopover';
import { AssistantStartView } from './AssistantStartView';

const MIN_WIDTH = 320;
const MAX_WIDTH = 720;

function clampWidth(width: number): number {
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width));
}

/** The workspace AI Assistant, docked on the right of the main layout. */
export function AssistantPanel() {
  const status = useAssistantStore((s) => s.session?.status);
  const hasMessages = useAssistantStore((s) => s.messages.length > 0);
  const closePanel = useAssistantStore((s) => s.closePanel);
  const width = useLayoutStore((s) => s.assistantPanelWidth);
  const setWidth = useLayoutStore((s) => s.setAssistantPanelWidth);

  return (
    <aside
      id='assistant-panel'
      aria-label='AI Assistant'
      style={{ '--assistant-w': `${width}px` } as CSSProperties}
      className='relative flex w-(--assistant-w) shrink-0 flex-col border-l bg-background'
    >
      {/* biome-ignore lint/a11y/useSemanticElements: <hr role="separator"> is a horizontal rule and cannot be a draggable, focusable resize handle */}
      <div
        role='separator'
        aria-orientation='vertical'
        aria-valuenow={width}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={MAX_WIDTH}
        aria-label='Resize AI Assistant'
        tabIndex={0}
        className='absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize hover:bg-primary/30 focus-visible:bg-primary/60 focus-visible:outline-none'
        onPointerDown={(e) => {
          e.preventDefault();
          const startX = e.clientX;
          const startWidth = width;
          // The panel grows as the handle moves left.
          const onMove = (ev: PointerEvent) =>
            setWidth(clampWidth(startWidth - (ev.clientX - startX)));
          const onUp = () => {
            window.removeEventListener('pointermove', onMove);
            window.removeEventListener('pointerup', onUp);
          };
          window.addEventListener('pointermove', onMove);
          window.addEventListener('pointerup', onUp);
        }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft') {
            e.preventDefault();
            setWidth(clampWidth(width + 16));
          } else if (e.key === 'ArrowRight') {
            e.preventDefault();
            setWidth(clampWidth(width - 16));
          }
        }}
      />
      <header className='flex h-10 shrink-0 items-center gap-1 border-b px-3'>
        <Sparkles className='h-4 w-4 text-primary' aria-hidden='true' />
        <span className='flex-1 text-sm font-medium'>AI Assistant</span>
        <AssistantPermissionsPopover />
        {(status === 'active' || status === 'starting') && (
          <Button
            variant='ghost'
            size='sm'
            className='h-7 text-xs'
            onClick={() => void endAssistantSession()}
          >
            End session
          </Button>
        )}
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Close AI Assistant'
          onClick={closePanel}
        >
          <X className='h-4 w-4' aria-hidden='true' />
        </Button>
      </header>
      {status === 'starting' ? (
        <div className='flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground'>
          <Loader2 className='h-4 w-4 animate-spin' aria-hidden='true' />
          Starting the assistant…
        </div>
      ) : status === 'active' ? (
        <>
          <AssistantChatView />
          <AssistantInputStub />
        </>
      ) : (
        <>
          {hasMessages && <AssistantChatView />}
          <AssistantStartView />
        </>
      )}
    </aside>
  );
}
```

- [ ] **Step 10: Dock the panel in the main layout**

In `src/App.tsx`, add the panel import as the first `@/components` import (before `ErrorBoundary`, `:4`) and the store import as the first `@/stores` import (before `env-store`, `:23`):

```ts
import { AssistantPanel } from '@/components/assistant/AssistantPanel';
import { ErrorBoundary } from '@/components/ErrorBoundary';
```

```ts
import { useAssistantStore } from '@/stores/assistant-store';
import { useEnvStore } from '@/stores/env-store';
```

In `App()`, after `const root = usePaneStore((s) => s.root);` (`:30`):

```ts
  const assistantPanelOpen = useAssistantStore((s) => s.panelOpen);
```

and after `</main>` (`:232`), inside the main row and before its closing `</div>` (`:233`):

```tsx
        </main>
        {assistantPanelOpen && (
          <ErrorBoundary>
            <AssistantPanel />
          </ErrorBoundary>
        )}
      </div>
```

The session lives in the store, so closing the panel unmounts the view but keeps the session and the event routing.

- [ ] **Step 11: Verify**

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn check`
Expected: PASS. Import order or formatting only: `yarn biome check --write src/components/assistant src/stores/layout-store.ts src/components/request/AgentAutonomyToggle.tsx src/components/title-bar/TitleBar.tsx src/App.tsx src/lib/tauri-api.ts`, then re-run.

For the user to run: `yarn test AssistantPanel AssistantPermissionsPopover AgentAutonomyToggle layout-store`
Expected: PASS.

- [ ] **Step 12: Commit**

```bash
git add src/stores/layout-store.ts src/components/assistant/AssistantToggleButton.tsx \
  src/components/assistant/AssistantPanel.tsx src/components/assistant/AssistantStartView.tsx \
  src/components/assistant/AssistantChatView.tsx src/components/assistant/AssistantInputStub.tsx \
  src/components/assistant/AssistantPermissionsPopover.tsx \
  src/components/assistant/__tests__/AssistantPanel.test.tsx \
  src/components/assistant/__tests__/AssistantPermissionsPopover.test.tsx \
  src/components/request/AgentAutonomyToggle.tsx \
  src/components/request/__tests__/AgentAutonomyToggle.test.tsx \
  src/components/title-bar/TitleBar.tsx src/App.tsx src/lib/tauri-api.ts
```

Invoke the `dev-workflow-skills:1-git-commit` skill for the staged changes, with the conventional-commit subject `feat: add docked AI Assistant panel`. Do not write a freeform `git commit -m`.

---

## Task 3: Proposal cards, the Scripts-tab shortcut and removal of the per-tab chat

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src/lib/assistant/proposal-view.ts`
- Create: `src/lib/assistant/proposal-actions.ts`
- Create: `src/components/assistant/ProposalDiffEditor.tsx`
- Create: `src/components/assistant/AssistantProposalCard.tsx`
- Modify: `src/components/assistant/AssistantChatView.tsx` (from Task 2)
- Create: `src/lib/assistant/__tests__/proposal-view.test.ts`
- Create: `src/lib/assistant/__tests__/proposal-actions.test.ts`
- Create: `src/components/assistant/__tests__/AssistantProposalCard.test.tsx`
- Modify: `src/components/request/ScriptsTab.tsx:1`, `:13-14`, `:25-32`, `:70-82`, `:100`, `:165-178`, `:271-278`
- Modify: `src/components/request/RequestPanel.tsx:1191`
- Replace: `src/components/request/__tests__/ScriptsTab.test.tsx` (whole file)
- Delete: `src/components/request/AgentChatPanel.tsx`, `src/components/request/__tests__/AgentChatPanel.test.tsx`, `src/lib/agent-session-event-bridge.ts`, `src/lib/__tests__/agent-session-event-bridge.test.ts`
- Modify: `src/App.tsx` (remove the old bridge import and call)
- Modify: `src/types/pane-types.ts:28-41`, `:47`
- Modify: `src/stores/pane-store.ts:34`, `:48`, `:136-142`, `:144-147`, `:159-193`, `:305-316`, `:483-484`, `:658-773`, `:1331-1333`, `:1342-1345`, `:1392-1395`, `:1466-1474`
- Modify: `src/stores/__tests__/pane-store.test.ts:500-528`, `:1200-1493`

**Interfaces:**
- Consumes: Task 1's store and fixtures; `acceptAgentProposal(sessionId, proposalId): Promise<AgentProposal>`, `rejectAgentProposal(sessionId, proposalId): Promise<AgentProposal>` (Plan 04); `getRequest(collection, path): Promise<Request>` (`src/lib/tauri-api.ts:873-874`); `mapApiRequestToState`, `collectAllTabs`, `isPathWithin` (`src/lib/pane-utils.ts`); `usePaneStore` actions `updateRequest`, `markClean`; `PaneState` (`src/stores/pane-store.ts:281`); `collectionKeys` and `getQueryClient`.
- Produces:
  ```ts
  // src/lib/assistant/proposal-view.ts
  export interface ProposalTarget { collection: string; path?: string }
  export type ProposalPreview = { kind: 'diff' } | { kind: 'definition'; text: string } | { kind: 'line'; text: string };
  export interface ProposalDiff { before: string; after: string; language: string }
  export function proposalTarget(change: AgentProposal['change']): ProposalTarget;
  export function proposalPreview(change: AgentProposal['change']): ProposalPreview;
  export function proposalFailure(proposal: AgentProposal): string | undefined;
  export function loadProposalDiff(proposal: AgentProposal): Promise<ProposalDiff | null>;
  // src/lib/assistant/proposal-actions.ts
  export function findAffectedRequestTabs(state: Pick<PaneState, 'root' | 'collectionTabState'>, change: AgentProposal['change']): RequestTab[];
  export function hasDirtyAffectedTab(state: Pick<PaneState, 'root' | 'collectionTabState'>, change: AgentProposal['change']): boolean;
  export function acceptProposal(proposal: AgentProposal): Promise<void>;
  export function rejectProposal(proposal: AgentProposal): Promise<void>;
  // components
  export function ProposalDiffEditor(props: { original: string; modified: string; language: string }): JSX.Element;
  export function AssistantProposalCard(props: { proposal: AgentProposal }): JSX.Element;
  // ScriptsTab gains `requestPath?: string` and loses `agentSession`
  ```

- [ ] **Step 1: Write the failing proposal-view and proposal-actions tests, and run them to see them fail**

Create `src/lib/assistant/__tests__/proposal-view.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as api from '@/lib/tauri-api';
import { makeProposal, makeRequest } from '@/test/assistant-fixtures';
import {
  loadProposalDiff,
  proposalFailure,
  proposalPreview,
  proposalTarget,
} from '../proposal-view';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  getRequest: vi.fn(),
}));

describe('proposal-view', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('diffs a script edit against the stored script of that phase', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(
      makeRequest({ postResponseScript: 'old();', tests: 'other();' }),
    );
    const proposal = makeProposal({
      change: {
        op: 'editScript',
        collection: 'orders',
        requestPath: 'get.yml',
        phase: 'postResponse',
        body: 'new();',
      },
    });
    expect(await loadProposalDiff(proposal)).toEqual({
      before: 'old();',
      after: 'new();',
      language: 'javascript',
    });
    expect(api.getRequest).toHaveBeenCalledWith('orders', 'get.yml');
  });

  it('diffs only the patched fields of a request update', async () => {
    vi.mocked(api.getRequest).mockResolvedValue(
      makeRequest({ url: 'https://old.test', name: 'Same' }),
    );
    const proposal = makeProposal({
      change: {
        op: 'updateRequest',
        collection: 'orders',
        requestPath: 'get.yml',
        patch: { url: 'https://new.test' },
      },
    });
    const diff = await loadProposalDiff(proposal);
    expect(diff?.language).toBe('json');
    expect(JSON.parse(diff?.before ?? '{}')).toEqual({ url: 'https://old.test' });
    expect(JSON.parse(diff?.after ?? '{}')).toEqual({ url: 'https://new.test' });
  });

  it('has no diff for a create', async () => {
    const proposal = makeProposal({
      change: { op: 'createFolder', collection: 'orders', parentPath: '', name: 'admin' },
    });
    expect(await loadProposalDiff(proposal)).toBeNull();
    expect(api.getRequest).not.toHaveBeenCalled();
  });

  it('describes creates with the new definition', () => {
    const preview = proposalPreview({
      op: 'createRequest',
      collection: 'orders',
      folderPath: 'admin',
      request: {
        name: 'List orders',
        method: 'GET',
        url: 'https://api.test/orders',
        headers: [],
        queryParams: [],
      },
    });
    expect(preview.kind).toBe('definition');
    expect(preview.kind === 'definition' && preview.text).toContain('"name": "List orders"');
  });

  it('describes moves, renames, folders and environment variables in one line', () => {
    expect(
      proposalPreview({
        op: 'moveItem',
        collection: 'orders',
        fromPath: 'get.yml',
        toFolder: 'admin',
      }),
    ).toEqual({ kind: 'line', text: 'Move get.yml to admin' });
    expect(
      proposalPreview({
        op: 'renameItem',
        collection: 'orders',
        path: 'get.yml',
        newName: 'Get one',
      }),
    ).toEqual({ kind: 'line', text: 'Rename get.yml to Get one' });
    expect(
      proposalPreview({ op: 'createFolder', collection: 'orders', parentPath: 'a', name: 'b' }),
    ).toEqual({ kind: 'line', text: 'New folder a/b' });
    expect(
      proposalPreview({
        op: 'setEnvVar',
        collection: 'orders',
        environment: 'dev',
        key: 'BASE_URL',
        value: 'https://dev.test',
      }),
    ).toEqual({ kind: 'line', text: 'Set BASE_URL = https://dev.test in environment dev' });
  });

  it('names the target of each change', () => {
    expect(proposalTarget(makeProposal().change)).toEqual({
      collection: 'orders',
      path: 'get.yml',
    });
    expect(
      proposalTarget({
        op: 'setEnvVar',
        collection: 'orders',
        environment: 'dev',
        key: 'K',
        value: 'v',
      }),
    ).toEqual({ collection: 'orders' });
  });

  it('reads the failure message', () => {
    expect(
      proposalFailure(makeProposal({ status: 'failed', statusMessage: 'name taken' })),
    ).toBe('name taken');
    expect(proposalFailure(makeProposal())).toBeUndefined();
  });
});
```

Create `src/lib/assistant/__tests__/proposal-actions.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequest } from '@/lib/pane-utils';
import * as api from '@/lib/tauri-api';
import type { AgentProposal } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { makeProposal, makeRequest } from '@/test/assistant-fixtures';
import { isRequestTab, type RequestTab } from '@/types/pane-types';
import { acceptProposal, hasDirtyAffectedTab, rejectProposal } from '../proposal-actions';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  acceptAgentProposal: vi.fn(),
  rejectAgentProposal: vi.fn(),
  getRequest: vi.fn(),
}));

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));

function requestTab(path: string, overrides: Partial<RequestTab> = {}): RequestTab {
  return {
    id: `tab:${path}`,
    title: path,
    tabType: 'request',
    request: { ...createDefaultRequest(), testsScript: 'old();' },
    response: null,
    isDirty: false,
    source: { collection: 'orders', path },
    ...overrides,
  };
}

function firstTab(): RequestTab | undefined {
  const { root } = usePaneStore.getState();
  const tab = root.type === 'leaf' ? root.tabs[0] : undefined;
  return tab && isRequestTab(tab) ? tab : undefined;
}

describe('proposal actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useAssistantStore.getState().reset();
    const token = useAssistantStore.getState().beginSession('agent-1', 'edit');
    useAssistantStore.getState().activateSession(token, 's1', []);
  });

  it('accept stores the result and reloads a clean open tab of that request', async () => {
    usePaneStore.getState().openTab(requestTab('get.yml'));
    const proposal = makeProposal();
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'accepted' });
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ tests: 'new();' }));

    await acceptProposal(proposal);

    expect(api.acceptAgentProposal).toHaveBeenCalledWith('s1', 'p1');
    expect(useAssistantStore.getState().proposals[0].status).toBe('accepted');
    expect(firstTab()?.request.testsScript).toBe('new();');
    expect(firstTab()?.isDirty).toBe(false);
  });

  it('reloads nothing when the change was not applied', async () => {
    usePaneStore.getState().openTab(requestTab('get.yml'));
    const proposal = makeProposal();
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'stale' });

    await acceptProposal(proposal);

    expect(useAssistantStore.getState().proposals[0].status).toBe('stale');
    expect(api.getRequest).not.toHaveBeenCalled();
    expect(firstTab()?.request.testsScript).toBe('old();');
  });

  it('reject stores the result', async () => {
    const proposal = makeProposal();
    useAssistantStore.getState().upsertProposal(proposal);
    vi.mocked(api.rejectAgentProposal).mockResolvedValue({ ...proposal, status: 'rejected' });

    await rejectProposal(proposal);

    expect(api.rejectAgentProposal).toHaveBeenCalledWith('s1', 'p1');
    expect(useAssistantStore.getState().proposals[0].status).toBe('rejected');
  });

  it('finds unsaved edits in an open tab of the changed request', () => {
    usePaneStore.getState().openTab(requestTab('get.yml', { isDirty: true }));
    expect(hasDirtyAffectedTab(usePaneStore.getState(), makeProposal().change)).toBe(true);
  });

  it('finds unsaved edits in a tab parked after a collection switch', () => {
    usePaneStore.setState({
      collectionTabState: {
        orders: { tabs: [requestTab('get.yml', { isDirty: true })], activeTabId: 'tab:get.yml' },
      },
    });
    expect(hasDirtyAffectedTab(usePaneStore.getState(), makeProposal().change)).toBe(true);
  });

  it('finds unsaved edits inside a folder that is being moved', () => {
    usePaneStore.getState().openTab(requestTab('users/get.yml', { isDirty: true }));
    const change: AgentProposal['change'] = {
      op: 'moveItem',
      collection: 'orders',
      fromPath: 'users',
      toFolder: 'archive',
    };
    expect(hasDirtyAffectedTab(usePaneStore.getState(), change)).toBe(true);
  });

  it('ignores clean tabs, other requests and other collections', () => {
    usePaneStore.getState().openTab(requestTab('get.yml'));
    usePaneStore.getState().openTab(requestTab('other.yml', { isDirty: true }));
    usePaneStore.getState().openTab(
      requestTab('get.yml', {
        id: 'tab:billing',
        isDirty: true,
        source: { collection: 'billing', path: 'get.yml' },
      }),
    );
    expect(hasDirtyAffectedTab(usePaneStore.getState(), makeProposal().change)).toBe(false);
  });
});
```

For the user to run: `yarn test proposal-view proposal-actions`
Expected: FAIL, `Failed to resolve import "../proposal-view"` and `"../proposal-actions"`.

- [ ] **Step 2: Implement `proposal-view.ts`**

Create `src/lib/assistant/proposal-view.ts`. Together with `proposal-actions.ts` it is the only code that reads `change` fields, so a later DTO change is fixed in these two files:

```ts
import { type AgentProposal, getRequest } from '@/lib/tauri-api';

type Change = AgentProposal['change'];

export interface ProposalTarget {
  collection: string;
  path?: string;
}

export type ProposalPreview =
  | { kind: 'diff' }
  | { kind: 'definition'; text: string }
  | { kind: 'line'; text: string };

export interface ProposalDiff {
  before: string;
  after: string;
  language: string;
}

type ScriptPhase = Extract<Change, { op: 'editScript' }>['phase'];

// Where each proposal phase is stored on a saved request.
const SCRIPT_FIELDS: Record<ScriptPhase, 'preRequestScript' | 'postResponseScript' | 'tests'> = {
  preRequest: 'preRequestScript',
  postResponse: 'postResponseScript',
  tests: 'tests',
};

function joinPath(parent: string, name: string): string {
  return parent ? `${parent}/${name}` : name;
}

// Lets a typed DTO be read key by key.
function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
}

function stringifyPicked(source: Record<string, unknown>, keys: string[]): string {
  const picked: Record<string, unknown> = {};
  for (const key of keys) picked[key] = source[key] ?? null;
  return JSON.stringify(picked, null, 2);
}

/** Where the change lands, for the card header and the open-tab checks. */
export function proposalTarget(change: Change): ProposalTarget {
  switch (change.op) {
    case 'createFolder':
      return { collection: change.collection, path: joinPath(change.parentPath, change.name) };
    case 'createRequest':
      return { collection: change.collection, path: change.folderPath || undefined };
    case 'updateRequest':
    case 'editScript':
      return { collection: change.collection, path: change.requestPath };
    case 'moveItem':
      return { collection: change.collection, path: change.fromPath };
    case 'renameItem':
      return { collection: change.collection, path: change.path };
    case 'setEnvVar':
      return { collection: change.collection };
  }
}

/** How the card previews the change. */
export function proposalPreview(change: Change): ProposalPreview {
  switch (change.op) {
    case 'updateRequest':
    case 'editScript':
      return { kind: 'diff' };
    case 'createRequest':
      return { kind: 'definition', text: JSON.stringify(change.request, null, 2) };
    case 'createFolder':
      return { kind: 'line', text: `New folder ${joinPath(change.parentPath, change.name)}` };
    case 'moveItem':
      return {
        kind: 'line',
        text: `Move ${change.fromPath} to ${change.toFolder || 'the collection root'}`,
      };
    case 'renameItem':
      return { kind: 'line', text: `Rename ${change.path} to ${change.newName}` };
    case 'setEnvVar':
      return {
        kind: 'line',
        text: `Set ${change.key} = ${change.value} in environment ${change.environment}`,
      };
  }
}

/** Why a failed proposal could not apply, when the backend said. */
export function proposalFailure(proposal: AgentProposal): string | undefined {
  return proposal.statusMessage || undefined;
}

/**
 * Before and after texts for a script edit or request update. The proposal
 * DTO carries only the new values, so the before side is the stored request.
 */
export async function loadProposalDiff(proposal: AgentProposal): Promise<ProposalDiff | null> {
  const { change } = proposal;
  if (change.op !== 'updateRequest' && change.op !== 'editScript') return null;
  const language = change.op === 'editScript' ? 'javascript' : 'json';

  const current = await getRequest(change.collection, change.requestPath);
  if (change.op === 'editScript') {
    const before = current[SCRIPT_FIELDS[change.phase]] ?? '';
    return { before, after: change.body, language };
  }

  const patch = asRecord(change.patch);
  const keys = Object.keys(patch).filter((key) => patch[key] !== undefined);
  return {
    before: stringifyPicked(asRecord(current), keys),
    after: stringifyPicked(patch, keys),
    language,
  };
}
```

- [ ] **Step 3: Implement `proposal-actions.ts`**

Create `src/lib/assistant/proposal-actions.ts`:

```ts
import { collectAllTabs, isPathWithin, mapApiRequestToState } from '@/lib/pane-utils';
import { collectionKeys } from '@/lib/queries/collection-queries';
import { getQueryClient } from '@/lib/query-client';
import {
  type AgentProposal,
  acceptAgentProposal,
  getRequest,
  rejectAgentProposal,
} from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { type PaneState, usePaneStore } from '@/stores/pane-store';
import { isRequestTab, type RequestTab } from '@/types/pane-types';
import { proposalTarget } from './proposal-view';

type Change = AgentProposal['change'];
type TabSource = Pick<PaneState, 'root' | 'collectionTabState'>;

/**
 * Request tabs, live or parked after a collection switch, that show an item
 * this change rewrites, moves or renames. Creates and env vars touch none.
 */
export function findAffectedRequestTabs(state: TabSource, change: Change): RequestTab[] {
  if (change.op === 'createFolder' || change.op === 'createRequest') return [];
  if (change.op === 'setEnvVar') return [];
  const target = proposalTarget(change);
  const targetPath = target.path;
  if (!targetPath) return [];
  const tabs = [
    ...collectAllTabs(state.root),
    ...Object.values(state.collectionTabState).flatMap((entry) => entry.tabs),
  ];
  return tabs.filter(
    (tab): tab is RequestTab =>
      isRequestTab(tab) &&
      tab.source?.collection === target.collection &&
      isPathWithin(tab.source.path, targetPath),
  );
}

/** A dirty tab would later save its stale copy over the accepted change. */
export function hasDirtyAffectedTab(state: TabSource, change: Change): boolean {
  return findAffectedRequestTabs(state, change).some((tab) => tab.isDirty);
}

// Shows the accepted version in clean HTTP tabs of the changed request. Tabs
// with edits are never touched, and Accept is disabled while one exists.
async function refreshCleanOpenTabs(change: Change): Promise<void> {
  if (change.op !== 'updateRequest' && change.op !== 'editScript') return;
  const tabs = collectAllTabs(usePaneStore.getState().root).filter(
    (tab): tab is RequestTab =>
      isRequestTab(tab) &&
      tab.tabType === 'request' &&
      !tab.isDirty &&
      tab.request.requestType === 'http' &&
      tab.source?.collection === change.collection &&
      tab.source.path === change.requestPath,
  );
  if (tabs.length === 0) return;
  try {
    const fresh = mapApiRequestToState(
      await getRequest(change.collection, change.requestPath),
      true,
    );
    for (const tab of tabs) {
      usePaneStore.getState().updateRequest(tab.id, fresh);
      usePaneStore.getState().markClean(tab.id);
    }
  } catch (err) {
    console.error('[assistant] failed to refresh an open tab', err);
  }
}

export async function acceptProposal(proposal: AgentProposal): Promise<void> {
  const result = await acceptAgentProposal(proposal.sessionId, proposal.id);
  useAssistantStore.getState().upsertProposal(result);
  if (result.status !== 'accepted') return;
  void getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
  await refreshCleanOpenTabs(proposal.change);
}

export async function rejectProposal(proposal: AgentProposal): Promise<void> {
  const result = await rejectAgentProposal(proposal.sessionId, proposal.id);
  useAssistantStore.getState().upsertProposal(result);
}
```

- [ ] **Step 4: Write the failing card test**

Create `src/components/assistant/__tests__/AssistantProposalCard.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createDefaultRequest } from '@/lib/pane-utils';
import * as api from '@/lib/tauri-api';
import type { AgentProposal } from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { makeProposal, makeRequest } from '@/test/assistant-fixtures';
import { AssistantProposalCard } from '../AssistantProposalCard';

vi.mock('@/lib/tauri-api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-api')>()),
  acceptAgentProposal: vi.fn(),
  rejectAgentProposal: vi.fn(),
  getRequest: vi.fn(),
}));

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));

vi.mock('../ProposalDiffEditor', () => ({
  ProposalDiffEditor: ({
    original,
    modified,
    language,
  }: {
    original: string;
    modified: string;
    language: string;
  }) => (
    <div
      data-testid='proposal-diff'
      data-original={original}
      data-modified={modified}
      data-language={language}
    />
  ),
}));

vi.mock('@/components/collections/MarkdownRenderer', () => ({
  MarkdownRenderer: ({ children }: { children: string }) => <pre>{children}</pre>,
}));

// Renders the card from the store, the way the chat view does.
function StoreCard() {
  const proposal = useAssistantStore((s) => s.proposals[0]);
  return proposal ? <AssistantProposalCard proposal={proposal} /> : null;
}

function showProposal(proposal: AgentProposal): void {
  useAssistantStore.getState().upsertProposal(proposal);
  render(<StoreCard />);
}

describe('AssistantProposalCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    useAssistantStore.getState().reset();
    const token = useAssistantStore.getState().beginSession('agent-1', 'edit');
    useAssistantStore.getState().activateSession(token, 's1', []);
    vi.mocked(api.getRequest).mockResolvedValue(makeRequest({ tests: 'old();' }));
  });

  it('shows a Monaco diff of the script edit', async () => {
    showProposal(makeProposal());
    const diff = await screen.findByTestId('proposal-diff');
    expect(diff.dataset.original).toBe('old();');
    expect(diff.dataset.modified).toBe("rok.test('status', () => {});");
    expect(diff.dataset.language).toBe('javascript');
    expect(screen.getByText('orders / get.yml')).toBeInTheDocument();
  });

  it('accepts and shows the accepted state', async () => {
    const proposal = makeProposal();
    vi.mocked(api.acceptAgentProposal).mockResolvedValue({ ...proposal, status: 'accepted' });
    showProposal(proposal);
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(api.acceptAgentProposal).toHaveBeenCalledWith('s1', 'p1');
    expect(await screen.findByText('Accepted')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
  });

  it('rejects', async () => {
    const proposal = makeProposal();
    vi.mocked(api.rejectAgentProposal).mockResolvedValue({ ...proposal, status: 'rejected' });
    showProposal(proposal);
    await userEvent.click(screen.getByRole('button', { name: 'Reject' }));
    expect(api.rejectAgentProposal).toHaveBeenCalledWith('s1', 'p1');
    expect(await screen.findByText('Rejected')).toBeInTheDocument();
  });

  it('explains a stale proposal', () => {
    showProposal(makeProposal({ status: 'stale' }));
    expect(screen.getByText(/changed after the proposal was made/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Accept' })).not.toBeInTheDocument();
  });

  it('explains a failed proposal with its message', () => {
    showProposal(makeProposal({ status: 'failed', statusMessage: 'name taken' }));
    expect(screen.getByText('Could not apply this change: name taken')).toBeInTheDocument();
  });

  it('shows the definition of a new request', () => {
    showProposal(
      makeProposal({
        change: {
          op: 'createRequest',
          collection: 'orders',
          folderPath: 'admin',
          request: {
            name: 'List orders',
            method: 'GET',
            url: 'https://api.test/orders',
            headers: [],
            queryParams: [],
          },
        },
      }),
    );
    expect(screen.getByText(/"name": "List orders"/)).toBeInTheDocument();
    expect(screen.queryByTestId('proposal-diff')).not.toBeInTheDocument();
  });

  it('disables Accept while the request has unsaved edits in an open tab', () => {
    usePaneStore.getState().openTab({
      id: 'tab-1',
      title: 'get.yml',
      tabType: 'request',
      request: createDefaultRequest(),
      response: null,
      isDirty: true,
      source: { collection: 'orders', path: 'get.yml' },
    });
    showProposal(makeProposal());
    expect(screen.getByRole('button', { name: 'Accept' })).toBeDisabled();
    expect(screen.getByText(/unsaved edits in an open tab/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reject' })).toBeEnabled();
  });

  it('shows the error when Accept fails', async () => {
    vi.mocked(api.acceptAgentProposal).mockRejectedValue('session ended');
    showProposal(makeProposal());
    await userEvent.click(screen.getByRole('button', { name: 'Accept' }));
    expect(await screen.findByText('session ended')).toBeInTheDocument();
  });
});
```

- [ ] **Step 5: Run the new tests to see them fail (for the user)**

For the user to run: `yarn test proposal-view proposal-actions AssistantProposalCard`
Expected: `proposal-view` and `proposal-actions` PASS once Steps 2–3 are in; `AssistantProposalCard` FAILS to resolve `../AssistantProposalCard`.

- [ ] **Step 6: Create the diff editor and the card**

Create `src/components/assistant/ProposalDiffEditor.tsx` (same Monaco setup and teardown as `src/components/git/DiffViewer.tsx`):

```tsx
import '@/components/editor/monaco-setup';
import { DiffEditor, type DiffOnMount } from '@monaco-editor/react';
import type * as monacoNs from 'monaco-editor';
import { useEffect, useRef } from 'react';
import { MONACO_FONT_FAMILY } from '@/components/editor/monaco-config';
import { acquireJsWorker, releaseJsWorker } from '@/components/editor/monaco-js-worker-lifecycle';
import { useMonacoTheme } from '@/components/editor/useMonacoTheme';

interface ProposalDiffEditorProps {
  original: string;
  modified: string;
  language: string;
}

/** Read-only inline diff of one proposal. Mirrors DiffViewer's Monaco setup. */
export function ProposalDiffEditor({ original, modified, language }: ProposalDiffEditorProps) {
  const { themeName } = useMonacoTheme();
  // Dispose before React removes the DOM, as DiffViewer does, to avoid
  // "TextModel disposed before DiffEditorWidget model got reset".
  const editorRef = useRef<monacoNs.editor.IDiffEditor | null>(null);

  useEffect(() => {
    return () => {
      editorRef.current?.dispose();
      editorRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (language !== 'javascript' && language !== 'typescript') return;
    acquireJsWorker();
    return () => releaseJsWorker();
  }, [language]);

  const handleMount: DiffOnMount = (editor) => {
    editorRef.current = editor;
  };

  return (
    <div className='h-56 overflow-hidden rounded-md border'>
      <DiffEditor
        original={original}
        modified={modified}
        language={language}
        theme={themeName}
        onMount={handleMount}
        options={{
          readOnly: true,
          renderSideBySide: false,
          minimap: { enabled: false },
          scrollBeyondLastLine: false,
          fontSize: 13,
          fontFamily: MONACO_FONT_FAMILY,
          hideUnchangedRegions: { enabled: true },
        }}
      />
    </div>
  );
}
```

Create `src/components/assistant/AssistantProposalCard.tsx`:

```tsx
import { Check, Loader2, X } from 'lucide-react';
import { lazy, Suspense, useEffect, useState } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  acceptProposal,
  hasDirtyAffectedTab,
  rejectProposal,
} from '@/lib/assistant/proposal-actions';
import {
  loadProposalDiff,
  type ProposalDiff,
  proposalFailure,
  proposalPreview,
  proposalTarget,
} from '@/lib/assistant/proposal-view';
import type { AgentProposal } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

const ProposalDiffEditor = lazy(() =>
  import('./ProposalDiffEditor').then((m) => ({ default: m.ProposalDiffEditor })),
);

type BadgeVariant = 'default' | 'secondary' | 'outline' | 'warning' | 'destructive';

const STATUS_BADGE: Record<AgentProposal['status'], { label: string; variant: BadgeVariant }> = {
  pending: { label: 'Pending', variant: 'secondary' },
  accepted: { label: 'Accepted', variant: 'default' },
  rejected: { label: 'Rejected', variant: 'outline' },
  stale: { label: 'Stale', variant: 'warning' },
  failed: { label: 'Failed', variant: 'destructive' },
};

/** One proposed change with its preview and Accept and Reject. */
export function AssistantProposalCard({ proposal }: { proposal: AgentProposal }) {
  const target = proposalTarget(proposal.change);
  const preview = proposalPreview(proposal.change);
  const blockedByEdits = usePaneStore(
    (s) => proposal.status === 'pending' && hasDirtyAffectedTab(s, proposal.change),
  );
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [diff, setDiff] = useState<ProposalDiff | null>(null);
  const [diffError, setDiffError] = useState<string | null>(null);

  // Loads the diff while the proposal is pending. After Accept the stored
  // version already holds the change, so the earlier diff is kept.
  useEffect(() => {
    if (preview.kind !== 'diff' || proposal.status !== 'pending') return;
    let cancelled = false;
    loadProposalDiff(proposal)
      .then((loaded) => {
        if (!cancelled) setDiff(loaded);
      })
      .catch((err) => {
        if (!cancelled) setDiffError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [proposal, preview.kind]);

  const run = async (action: (p: AgentProposal) => Promise<void>) => {
    setBusy(true);
    setActionError(null);
    try {
      await action(proposal);
    } catch (err) {
      setActionError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const badge = STATUS_BADGE[proposal.status];
  const failure = proposalFailure(proposal);

  return (
    <article
      aria-label={proposal.summary}
      className='flex flex-col gap-2 rounded-md border bg-card p-3'
    >
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0'>
          <p className='text-sm font-medium'>{proposal.summary}</p>
          <p className='truncate text-xs text-muted-foreground'>
            {target.path ? `${target.collection} / ${target.path}` : target.collection}
          </p>
        </div>
        <Badge variant={badge.variant}>{badge.label}</Badge>
      </div>

      {preview.kind === 'diff' &&
        (diffError ? (
          <p className='text-xs text-destructive'>
            Could not load the current version: {diffError}
          </p>
        ) : diff ? (
          <Suspense fallback={<EditorSkeleton />}>
            <ProposalDiffEditor
              original={diff.before}
              modified={diff.after}
              language={diff.language}
            />
          </Suspense>
        ) : (
          <EditorSkeleton />
        ))}
      {preview.kind === 'definition' && (
        <MarkdownRenderer>{`\`\`\`json\n${preview.text}\n\`\`\``}</MarkdownRenderer>
      )}
      {preview.kind === 'line' && <p className='text-xs'>{preview.text}</p>}

      {proposal.status === 'stale' && (
        <p className='text-xs text-muted-foreground'>
          This item changed after the proposal was made, so nothing was written. Ask the
          assistant to propose it again.
        </p>
      )}
      {proposal.status === 'failed' && (
        <p className='text-xs text-destructive'>
          {failure ? `Could not apply this change: ${failure}` : 'Could not apply this change.'}
        </p>
      )}
      {actionError && <p className='text-xs text-destructive'>{actionError}</p>}

      {proposal.status === 'pending' && (
        <>
          {blockedByEdits && (
            <p className='text-xs text-muted-foreground'>
              This request has unsaved edits in an open tab. Save or discard them before
              accepting.
            </p>
          )}
          <div className='flex justify-end gap-2'>
            <Button
              size='sm'
              variant='outline'
              disabled={busy}
              onClick={() => void run(rejectProposal)}
            >
              <X className='h-3.5 w-3.5' aria-hidden='true' />
              Reject
            </Button>
            <Button
              size='sm'
              disabled={busy || blockedByEdits}
              onClick={() => void run(acceptProposal)}
            >
              {busy ? (
                <Loader2 className='h-3.5 w-3.5 animate-spin' aria-hidden='true' />
              ) : (
                <Check className='h-3.5 w-3.5' aria-hidden='true' />
              )}
              Accept
            </Button>
          </div>
        </>
      )}
    </article>
  );
}
```

- [ ] **Step 7: Show the proposals in the chat flow**

In `src/components/assistant/AssistantChatView.tsx` (Task 2), add the import after the `ScrollArea` import:

```ts
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import {
  type AssistantMessage,
  type ToolActivityStatus,
  useAssistantStore,
} from '@/stores/assistant-store';
import { AssistantProposalCard } from './AssistantProposalCard';
```

and replace the `AssistantChatView` function with:

```tsx
/** The conversation: messages, tool activity lines, notices and proposals. */
export function AssistantChatView() {
  const messages = useAssistantStore((s) => s.messages);
  const proposals = useAssistantStore((s) => s.proposals);
  const endRef = useRef<HTMLDivElement>(null);
  const itemCount = messages.length + proposals.length;

  // Keeps the newest item in view.
  useEffect(() => {
    if (itemCount === 0) return;
    endRef.current?.scrollIntoView?.({ block: 'end' });
  }, [itemCount]);

  const lastId = messages[messages.length - 1]?.id;
  return (
    <ScrollArea className='min-h-0 flex-1'>
      <div className='flex flex-col gap-3 p-3'>
        {messages.map((m) => (
          <ChatItem key={m.id} message={m} isLast={m.id === lastId} />
        ))}
        {proposals.length > 0 && (
          <section aria-label='Proposals' className='flex flex-col gap-2'>
            {proposals.map((p) => (
              <AssistantProposalCard key={p.id} proposal={p} />
            ))}
          </section>
        )}
        <div ref={endRef} />
      </div>
    </ScrollArea>
  );
}
```

- [ ] **Step 8: Rewrite the ScriptsTab test for the shortcut**

Replace `src/components/request/__tests__/ScriptsTab.test.tsx` with:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useAssistantStore } from '@/stores/assistant-store';
import { ScriptsTab } from '../ScriptsTab';

vi.mock('@/components/editor/MonacoWrapper', () => ({
  MonacoWrapper: ({ phase }: { phase: string }) => <div data-testid={`monaco-${phase}`} />,
}));

function renderWith(extra: Partial<ComponentProps<typeof ScriptsTab>> = {}) {
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
      {...extra}
    />,
  );
}

describe('ScriptsTab — AI Assist shortcut', () => {
  beforeEach(() => {
    useAssistantStore.setState({ panelOpen: false, focus: undefined });
  });

  it('opens the assistant panel with the saved request in focus', async () => {
    renderWith({ requestPath: 'orders/get.yml' });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    expect(useAssistantStore.getState().panelOpen).toBe(true);
    expect(useAssistantStore.getState().focus).toEqual({
      collection: 'my-collection',
      path: 'orders/get.yml',
    });
  });

  it('clears the focus for a request that is not saved yet', async () => {
    useAssistantStore.setState({ focus: { collection: 'other', path: 'x.yml' } });
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: 'AI Assist' }));
    expect(useAssistantStore.getState().panelOpen).toBe(true);
    expect(useAssistantStore.getState().focus).toBeUndefined();
  });

  it('keeps AI Assist visible by default', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'AI Assist' })).toBeInTheDocument();
  });

  it('agentAssist false hides AI Assist', async () => {
    renderWith({ agentAssist: false });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: 'AI Assist' })).not.toBeInTheDocument();
  });
});

describe('ScriptsTab phases', () => {
  it('renders all three phase tabs by default', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: 'Pre Request' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Post Response' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Tests' })).toBeInTheDocument();
  });

  it('phases limits the visible phase tabs', async () => {
    renderWith({ phases: ['pre-request', 'post-response'] });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: 'Pre Request' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Post Response' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Tests' })).not.toBeInTheDocument();
  });

  it('starts on the first allowed phase', async () => {
    renderWith({ phases: ['tests'] });
    await waitFor(() => expect(screen.getByTestId('monaco-tests')).toBeInTheDocument());
    expect(screen.queryByTestId('monaco-pre-request')).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Pre Request' })).not.toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Tests' })).toBeInTheDocument();
  });

  it('passes the phase to every editor', async () => {
    renderWith({ phases: ['pre-request', 'post-response'] });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Post Response' }));
    await waitFor(() => expect(screen.getByTestId('monaco-post-response')).toBeInTheDocument());
  });

  it('right-aligns Snippets when AI Assist is hidden', async () => {
    renderWith({ agentAssist: false });
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Snippets' })).toHaveClass('ml-auto');
  });

  it('leaves Snippets unshifted when AI Assist is shown', async () => {
    renderWith();
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Snippets' })).not.toHaveClass('ml-auto');
  });

  it('falls back to a visible phase when phases change on rerender', async () => {
    const props = {
      tabId: 'tab-1',
      preRequestScript: '',
      postResponseScript: '',
      testsScript: '',
      onChangePreRequest: vi.fn(),
      onChangePostResponse: vi.fn(),
      onChangeTests: vi.fn(),
    };
    const { rerender } = render(<ScriptsTab {...props} />);
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Tests' }));
    await waitFor(() => expect(screen.getByTestId('monaco-tests')).toBeInTheDocument());
    rerender(<ScriptsTab {...props} phases={['pre-request', 'post-response']} />);
    await waitFor(() => expect(screen.getByTestId('monaco-pre-request')).toBeInTheDocument());
    expect(screen.queryByTestId('monaco-tests')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 9: Point the Scripts-tab button at the docked panel**

In `src/components/request/ScriptsTab.tsx`:

Line 1:

```ts
import { PanelRight, Sparkles } from 'lucide-react';
```

Replace lines 13-14 (`AgentChatSession` and `AgentChatPanel` imports) with:

```ts
import { useAssistantStore } from '@/stores/assistant-store';
```

so the import block ends `import { cn } from '@/lib/utils';`, `import { useAssistantStore } from '@/stores/assistant-store';`, `import { ScriptSnippetSidebar } from './ScriptSnippetSidebar';`.

Replace the props interface head (`:25-32`) with:

```ts
interface ScriptsTabProps {
  /** Identifies the owning tab or folder editor. Callers key editors by it. */
  tabId: string;
  collectionName?: string;
  /** Path of the saved request, so AI Assist can focus it. */
  requestPath?: string;
  /** Phase tabs to show. Defaults to all three. */
  phases?: ScriptPhase[];
  /** Shows the AI Assist button, which opens the docked assistant. Defaults to true. */
  agentAssist?: boolean;
```

(the remaining fields from `preRequestScript` on stay as they are).

Replace the destructuring head (`:70-75`):

```ts
export function ScriptsTab({
  collectionName,
  requestPath,
  phases = ALL_PHASES,
  agentAssist = true,
```

Replace `const [showAgentChat, setShowAgentChat] = useState(false);` (`:100`) with:

```ts
  const openAssistantPanel = useAssistantStore((s) => s.openPanel);
  const setAssistantFocus = useAssistantStore((s) => s.setFocus);

  // Opens the docked assistant with this request in focus. An unsaved request
  // has no path, so the focus is cleared rather than left on another request.
  const openAssistant = () => {
    setAssistantFocus(
      collectionName && requestPath ? { collection: collectionName, path: requestPath } : undefined,
    );
    openAssistantPanel();
  };
```

Replace the AI Assist button (`:165-178`) with:

```tsx
          {agentAssist && (
            <Button
              variant='ghost'
              size='sm'
              className='ml-auto h-7 gap-1 text-xs'
              onClick={openAssistant}
              aria-controls='assistant-panel'
              title='Open the AI Assistant with this request in focus'
            >
              <Sparkles className='h-3.5 w-3.5' />
              AI Assist
            </Button>
          )}
```

Delete the per-tab panel (`:271-278`):

```tsx
      {agentAssist && showAgentChat && (
        <AgentChatPanel
          tabId={tabId}
          collectionName={collectionName}
          agentSession={agentSession}
          onInsertCode={(code) => insertSnippet(editorRefs.current[activeTab], code)}
        />
      )}
```

In `src/components/request/RequestPanel.tsx:1191`, replace `agentSession={tab.agentSession}` with:

```tsx
              requestPath={tab.source?.path}
```

- [ ] **Step 10: Remove the per-tab chat files and the old bridge**

```bash
git rm src/components/request/AgentChatPanel.tsx \
  src/components/request/__tests__/AgentChatPanel.test.tsx \
  src/lib/agent-session-event-bridge.ts \
  src/lib/__tests__/agent-session-event-bridge.test.ts
```

In `src/App.tsx`, delete the line `import { useAgentSessionEventBridge } from '@/lib/agent-session-event-bridge';` and the line `useAgentSessionEventBridge();`.

- [ ] **Step 11: Remove the per-tab session types**

In `src/types/pane-types.ts`, delete `ChatMessage` and `AgentChatSession` (`:28-41`, with the blank line after them) and the `agentSession?: AgentChatSession;` field (`:47`), so `RequestTab` reads:

```ts
export interface RequestTab extends BaseTab {
  tabType: 'request' | 'history';
  request: RequestState;
  response: ResponseState | null;
}
```

- [ ] **Step 12: Remove the pane-store session state and keep the stream release**

In `src/stores/pane-store.ts`:

1. Delete `  endAgentSession,` from the `@/lib/tauri-api` import (`:34`) and `  ChatMessage,` from the `@/types/pane-types` import (`:48`).
2. Delete `findTabInSnapshots` (`:136-142`), whose only caller was `activateAgentSession`.
3. Replace the comment above `updateTabEverywhere` (`:144-147`) with:

```ts
// Applies an updater to one tab by id in the live pane tree and in every
// collection snapshot, so an update also reaches a tab that is parked after a
// collection switch.
```

4. Replace `endSessionIfActive` and `endActiveSessions` (`:159-193`) with:

```ts
// Releases the streaming connections of tabs that are about to be discarded.
// The same tab can appear twice (a live tab plus a stale snapshot copy), so
// each tab id is released once. Stream sessions are keyed by tab id.
function releaseDroppedTabs(tabs: Tab[]): void {
  const seen = new Set<string>();
  for (const tab of tabs) {
    if (
      isRequestTab(tab) &&
      (tab.request.requestType === 'websocket' ||
        tab.request.requestType === 'graphql' ||
        tab.request.requestType === 'grpc') &&
      !seen.has(tab.id)
    ) {
      seen.add(tab.id);
      releaseStreamingTab(tab);
    }
  }
}
```

5. Delete the agent action types (`:305-316`, from `  // Agent chat session actions.` through `  clearAgentSession: (tabId: string) => void;`, plus the blank line after).
6. In `closeTab`, replace (`:483-484`):

```ts
    // Best-effort session cleanup for the tab being closed.
    if (found) endSessionIfActive(found.tab);
```

with:

```ts
    // Best-effort release of the closed tab's streaming connection.
    if (found) releaseStreamingTab(found.tab);
```

7. Delete the agent actions `beginAgentSession` through `clearAgentSession` (`:658-773`, ending at the `},` before `openContractTab`).
8. In `switchCollection`, replace (`:1331-1333`):

```ts
      // With no active collection there is no snapshot to keep the active
      // leaf's tabs, so they are dropped. End their agent sessions first.
      endActiveSessions(activeLeaf.tabs);
```

with:

```ts
      // With no active collection there is no snapshot to keep the active
      // leaf's tabs, so they are dropped. Release their streams first.
      releaseDroppedTabs(activeLeaf.tabs);
```

9. Replace the comment at `:1342-1345` with:

```ts
    // The restored snapshot is now redundant: its tabs are about to become
    // live in `root`, so keeping it around would leave a stale duplicate
    // that `updateTabEverywhere` would keep updating after the tab is
    // closed. Drop it.
```

10. In `openWorkspaceTabs`, replace (`:1392-1395`):

```ts
    // Every other tab in the pane tree is dropped below. End its agent
    // session so no credentialed backend process is left orphaned.
    const droppedTabs = collectAllTabs(root).filter((tab) => !preservedTabIds.has(tab.id));
    endActiveSessions(droppedTabs);
```

with:

```ts
    // Every other tab in the pane tree is dropped below. Release its stream.
    const droppedTabs = collectAllTabs(root).filter((tab) => !preservedTabIds.has(tab.id));
    releaseDroppedTabs(droppedTabs);
```

11. In `reset`, replace `endActiveSessions([...collectAllTabs(root), ...snapshotTabs]);` (`:1474`) with `releaseDroppedTabs([...collectAllTabs(root), ...snapshotTabs]);`.

The assistant session is no longer tied to tabs, so closing or dropping a tab never ends it. A workspace switch ends it through the Task 1 bridge.

- [ ] **Step 13: Remove the pane-store session tests**

In `src/stores/__tests__/pane-store.test.ts`, delete:

- the test `regression: a tab closed after switching away and back has no stale snapshot copy to "activate"` (`:500-528`). The snapshot deletion it relied on is still covered by `switchCollection deletes the stale collectionTabState entry after restoring it` (`:485-498`).
- the describe blocks `Agent chat session actions`, `closeTab — ends the agent session for an active chat` and `closeAll/openWorkspaceTabs — end agent sessions of dropped tabs` (`:1200-1493`, up to the blank line before `describe('websocket tab cleanup'`).

Leave the `endAgentSession: vi.fn()` entry in the `vi.mock('@/lib/tauri-api', ...)` factory at `:24`; it is harmless, and other test files mock it the same way.

- [ ] **Step 14: Verify**

Run:

```bash
grep -rnE "agentSession|AgentChatPanel|AgentChatSession|agent-session-event-bridge|useAgentSessionEventBridge|beginAgentSession|activateAgentSession|appendAgentChat|completeAgentChatMessage|failAgentChatMessage|markAgentSessionEnded|clearAgentSession|endSessionIfActive|endActiveSessions|findTabInSnapshots" src
```

Expected: no output.

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn check`
Expected: PASS. Import order or formatting only: `yarn biome check --write src/lib/assistant src/components/assistant src/components/request/ScriptsTab.tsx src/components/request/RequestPanel.tsx src/App.tsx src/types/pane-types.ts src/stores/pane-store.ts`, then re-run.

For the user to run: `yarn test proposal-view proposal-actions AssistantProposalCard AssistantPanel ScriptsTab ScriptTestSections pane-store`
Expected: PASS.

- [ ] **Step 15: Commit**

```bash
git add src/lib/assistant/proposal-view.ts src/lib/assistant/proposal-actions.ts \
  src/components/assistant/ProposalDiffEditor.tsx \
  src/components/assistant/AssistantProposalCard.tsx \
  src/components/assistant/AssistantChatView.tsx \
  src/lib/assistant/__tests__/proposal-view.test.ts \
  src/lib/assistant/__tests__/proposal-actions.test.ts \
  src/components/assistant/__tests__/AssistantProposalCard.test.tsx \
  src/components/request/ScriptsTab.tsx src/components/request/RequestPanel.tsx \
  src/components/request/__tests__/ScriptsTab.test.tsx \
  src/App.tsx src/types/pane-types.ts src/stores/pane-store.ts \
  src/stores/__tests__/pane-store.test.ts
```

The four deletions are already staged by `git rm` in Step 10. Invoke the `dev-workflow-skills:1-git-commit` skill for the staged changes, with the conventional-commit subject `feat: add proposal cards and remove per-tab AI chat`. Do not write a freeform `git commit -m`.

---

## Manual check (for the user, in `yarn tauri dev`)

1. The Sparkles button in the title bar shows and hides the panel. The Bot button still opens the agent configurations.
2. Start a session, ask a question, and watch tool lines ("Reading …") appear and turn to done. Stop ends a running turn and the session stays usable.
3. Ask for a script change. The proposal card shows a Monaco diff. Accept writes it and an open clean tab shows the new script; with unsaved edits in that tab, Accept is disabled. Reject leaves the file alone. Edit the request by hand, then accept an older proposal: it turns Stale and nothing is written.
4. Open the permissions popover: one switch per collection, turning one on asks first, the popover stays open.
5. In the Scripts tab, AI Assist opens the docked panel. No chat appears inside the tab any more.
6. Switch workspace: the session ends with the notice, and pending proposals are gone.
7. Reload the webview (Ctrl+R) with a session running, then check with `ps` that the old agent process is gone once the app has loaded again (the bridge sweeps on every webview load, before any start).

## Next Plan

**Plan 06 — Composer** (`docs/superpowers/plans/workspace-ai-assistant/2026-10-09-workspace-ai-assistant-plan-06-composer.md`). It replaces `src/components/assistant/AssistantInputStub.tsx` (used only in `AssistantPanel.tsx`) with the composer and `PromptEditor`. What this plan leaves for it:

- `assistant-store` has no `setMode` action yet. Plan 06 adds `setMode(mode)` next to a `setAssistantMode(sessionId, mode)` call.
- The only `startWorkspaceAssistant(...)` call is in `src/lib/assistant/assistant-session.ts` (`startAssistant`), not under `src/components` or `src/stores`. Plan 06's remembered-model step edits it there.
- The send flow is `sendAssistantMessage(text)` in the same file: `appendUserMessage` (which also opens the streaming reply and refuses a second turn), then `sendAgentPrompt`, then `failMessage(sessionId, error)` on a thrown send. The stub calls only `sendAssistantMessage` and `stopAssistantTurn`. The stub's textbox label is `Message the assistant`, which `AssistantPanel.test.tsx` queries.
- `session.configOptions` is kept current by the bridge (`setConfigOptions`), and `usage` by `setUsage`. The model and effort pickers and the context indicator read them.
- `focus` is set by the Scripts-tab shortcut and cleared on workspace switch, but nothing sends it. Plan 06 turns it into the default chip and sends it through `chipToResource` (masked, 8 KB cap) as `resources` on `sendAgentPrompt`; give `sendAssistantMessage` a `resources` argument for that rather than calling `sendAgentPrompt` from the composer directly, so the double-send guard and the failure path stay in one place.
- Enter-to-send and the Stop button live in the stub. The composer takes them over, including stopping key events from reaching the global send shortcut.

Follow-ups not owned by Plan 06: the Rust `start_agent_session` command and the TypeScript `startAgentSession` wrapper have no frontend caller after this plan and can be removed; clean tabs parked in a collection snapshot are not reloaded after Accept (they show the old version until reopened, and they are clean, so they never write it back).

## Post-Implementation Review

Before starting Plan 06, dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created, modified or deleted: `src/stores/assistant-store.ts`, `src/lib/assistant/assistant-session.ts`, `src/lib/assistant-event-bridge.ts`, `src/lib/assistant/proposal-view.ts`, `src/lib/assistant/proposal-actions.ts`, `src/test/assistant-fixtures.ts`, everything under `src/components/assistant/`, `src/components/request/AgentAutonomyToggle.tsx`, `src/components/request/ScriptsTab.tsx`, `src/components/request/RequestPanel.tsx`, `src/components/title-bar/TitleBar.tsx`, `src/App.tsx`, `src/stores/layout-store.ts`, `src/types/pane-types.ts`, `src/stores/pane-store.ts`, `src/lib/tauri-api.ts`, their tests, and the deletions of `AgentChatPanel.tsx`, `agent-session-event-bridge.ts` and their tests. Use `git diff <commit before Task 1>..HEAD`.
>
> Check for:
> 1. **Interface conformance.** Compare `assistant-store` with the "Plan 05" block of `docs/superpowers/plans/workspace-ai-assistant/00-plan-index.md` and with this plan's "Interface deviations". Every name must be present; any drift beyond the listed deviations is a finding. Confirm the TypeScript wrappers from Plans 01–04 (fact 24) were used as they are and that this plan added none of its own to `tauri-api.ts`.
> 2. **Spec fidelity** against sections 2 and 5 of `docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md`: one session per workspace owned by the store, workspace switch ends it with a notice and discards proposals, the stale sweep runs before any start, the panel is docked right and toggled from the title bar with the Bot button kept, proposals render in the chat flow with Monaco diffs for script and request edits, Stale and Failed cards say why, the popover label reads "Allow the agent to run requests in this collection", and no per-tab chat or per-tab session state is left.
> 3. **The five Review Focus items** of this plan, by reading the code paths, not only the tests: abandoned starts end their backend session; the sweep cannot end a session the user just started; late and foreign events are dropped; every collection switch has its own id; Accept is blocked by any dirty live or parked tab of the target and clean tabs are reloaded.
> 4. **Frontend rules** (`CLAUDE.md`, `.claude/rules/frontend-component-guardrails.md`): shadcn primitives and `lucide-react` only, no raw `<button>`/`<input>`/`<select>`/`<form>`/`<dialog>`, no inline SVG, narrow Zustand selectors with no whole-store destructuring, Monaco for the diff, no CodeMirror, no `.at()`/`findLast`.
> 5. **Code quality and duplication**: `ProposalDiffEditor` versus `DiffViewer` (shared setup worth extracting?), the resize handle in `AssistantPanel` versus the sidebar handle in `App.tsx`, and whether `proposal-view.ts` is still the only reader of `change` fields.
>
> You may fix what you find directly. After fixing, run `yarn tsc --noEmit` and `yarn check` and confirm they pass. Do not run the test suite; list the `yarn test <pattern>` commands the user should run for the files you changed. Report what you found and fixed, and flag anything that needs a change to an earlier plan's interface instead of forcing it.

Plan 05 is done when this review is clean (or its fixes pass `yarn tsc --noEmit` and `yarn check`) and the user has run the tests listed in each task.
