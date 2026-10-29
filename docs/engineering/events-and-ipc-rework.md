# Events and IPC rework (proposal, not started)

Status: idea only. Brainstormed on 2026-10-11 and parked by the user to think about later. No spec, no plan, no code.
Roadmap row: R-55 in `docs/roadmaps/roadmap.md`.

## The ask

Use events more across Rocket so that:

1. Screens do not go stale.
2. Frontend stores and components stop calling each other by hand.
3. Background work (agent actions, runs, git, file changes, errors) is visible.
4. Backend services can react to each other's events.
5. The frontend needs fewer backend calls to finish one piece of work.

It is a large change. It touches the backend event bus, the Tauri bridge and most frontend data flow. Treat it as several sub-projects, each with its own spec, plan and review.

## What exists today (verified 2026-10-11 at main `9666d1a1`)

- Backend bus: `EventPublisher` trait and a `DomainEvent` enum of 81 variants (`crates/rocket-shared/src/events.rs`). `TauriEventBus` (`src-tauri/src/tauri_event_bus.rs`) maps each variant to one channel name in a single `match`. 57 channels are emitted (56 from the event bus plus `backend-log`). Counted 2026-10-29. `let _ = app.emit` discards emit errors.
- Frontend: only 8 channels are listened to from components and `App.tsx` (the `workspace-*` set, `collection-changed`, `git-changed`, `environment-changed`, `collection-trust-changed`). The Flow, assistant, WebSocket and gRPC streams have their own listeners in their own modules.
- There is no central event-to-cache layer. About 57 `invalidateQueries`, `removeQueries` or `resetQueries` calls are spread over the code, 22 mutation `onSuccess` handlers, 9 synchronous `getQueryData` reads.
- No backend-internal subscribers. Services cannot react to each other. The audit path uses a separate trait (`SecurityAuditPublisher`) with direct calls.
- Payload naming is mixed: the enum tag is camelCase but field names inside variants stay snake_case.

## Findings

### Frontend multi-call sequences (round trips per user action)

1. Single request Send, `src/lib/execute-request.ts:633-696`: about 12 reads plus 2 per folder level before the real call (collection settings, folder chain, folder variables, request variables, process env, global env, workspace config for the guard policy; several are read twice). All serial `await`s. The frontend merges the variable layers itself and mirrors Rust logic (`inheritedHeaders` mirrors `rocket_collection::inherited_headers`).
2. Collection Runner, `src/stores/pane-store.ts:927` and `src/lib/runner-execute.ts:36`: the UI loops over requests and repeats the Send chain for each. The backend `run_collection` command and `CollectionRunnerService` exist (`src-tauri/src/commands/runner.rs:14`, emits `runner-*` events), but `src/lib/tauri-api.ts` has no wrapper and nothing in the UI listens to those events. A workspace switch mid-run can mis-scope the run.
3. Workspace switch, `src/lib/workspace-switch.ts` and the `workspace-switched` listener in `src/App.tsx`: flush autosaves, switch, close tabs, invalidate several queries, reload the global environment, wipe three auth stores by hand. A second `workspace-switched` listener lives in `CollectionsSidebar.tsx`. A Send right after a switch can read the old workspace's global environment from the cache.
4. Folder settings save, `src/hooks/useFolderSettings.ts:137-141`: get then save (read-modify-write across IPC, can lose updates).
5. Collection settings save, `CollectionOverviewTab.tsx:272-282`: get then full replace. `collection-settings-save.ts` exists only to carry fields over.
6. Git actions, `src/stores/git-store.ts`: each takes 2 to 3 calls (commit then status and log, and so on). The backend publishes `git-changed`, but the git store ignores it.
7. Flow run start, `src/lib/flow-auth-preflight.ts:98`: the frontend resolves the OAuth2 variable context (4 to 5 reads) before `runFlow`.
8. Import into a new workspace, `ImportCollectionDialog.tsx:190-215`: create, import, import, then switch.

### Manual refresh wiring

- Same state invalidated in several places: `workspaceKeys.all` by 7 App listeners and 9 mutations; `collectionKeys.all` in the sidebar, `WorkspaceOverviewTab` and `FlowPicker`; `environmentKeys.processAll` in 3 places.
- `collection-changed` has 6 consumers, each with its own 300 ms debounce (sidebar, `CollectionNode.tsx`, `GitPanel.tsx`, `saved-request-preview.ts`, `useContractDrift.ts`, `collection-trust-queries.ts`). `git-changed` has 3.
- Stores that must be wiped by hand on a workspace switch: `collection-auth-store`, `folder-auth-store`, `flow-auth-store`, and `clearSavedRequestPreviewCache`. Forgetting one leaks credentials across workspaces. The same class of leak was found earlier in ACP subproject C.

### Backend changes without an event

- Services that never publish: `cookie_service`, `template_service`, `agent_config_service`, `secret_manager_service`, `flow_service` (save, rename, delete), `oauth2_service`, the `ui_state` command, workspace config changes (`set_global_environment`, `update_request_guard_policy`).
- Built with `NullEventPublisher` in production (`src-tauri/src/lib.rs:405-430`): history, template and cookie services, and `global_env_service()` in `src-tauri/src/commands/environments.rs`. So `EnvironmentSaved` and `EnvironmentDeleted` for global environments never reach the UI, and `HistoryCleared` is swallowed.
- Published but never listened to in `src/`: `runner-*`, `history-changed`, `script-console`, `script-tests`, `script-error`, `script-variable-written`, `agent-tool-invoked`, `request-executed` (a wrapper exists, no caller).
- Many events are coarse: 22 or more variants collapse into `collection-changed`, 11 into `git-changed`.

### Plumbing risks

- Listeners attach asynchronously, so events from before they attach are lost. `init` fetches state manually to compensate.
- Event delivery can lag the command result (Flow toolbar already works around this).
- The file watcher is not debounced in Rust, only in the UI. It restarts on a workspace switch and its start result is discarded.
- High-frequency streams (`agent-session-chunk`, `ws:message`, `grpc-session-message`, `flow-step-progress`) have no batching or throttling. The bus has no sequence numbers or per-workspace scoping.

## Proposed decomposition

| Sub-project | What | Size | Depends on |
|---|---|---|---|
| A. Missing-event audit | Make the silent services publish; fix the null publishers; wire or drop dead channels | S to M | none |
| B. Frontend event-to-cache layer | One typed registry mapping backend events to cache updates, replacing the scattered invalidations and the hand-wiped stores | M | works best after A |
| C. Composite backend commands | See below | L | independent |
| D. Backend internal subscribers | A bus that services can subscribe to, with coalescing and throttling for watcher and stream events | M to L | optional |
| E. Activity and visibility surface | UI for the published but unused events (runner, script, history, agent tool calls) | S to M | A, B |

### C split

| Piece | What | Size |
|---|---|---|
| C1 | One backend command that prepares a request for sending (variables, folder chain, auth, guard policy). The backend becomes the single place that merges variable layers. | M |
| C2 | Runner driven by the existing backend `run_collection`; the UI follows its events | M |
| C3 | Atomic patch commands for folder and collection settings | S to M |
| C4 | Composite commands for workspace switch and git actions | M |

C1 first, because C2 and C4 reuse it. C1 interacts with the runtime-variable pass from G2.4 (`execution_service/runtime_fill.rs`): the backend fills runtime variables after the pre-request script, but the frontend still resolves variables before calling the backend. The two passes must line up.

## Open questions for when this resumes

1. Which sub-project goes first. Recommendation made in the session: A (smallest, fixes real gaps and unblocks B), or C1 if the many-calls complaint is the priority.
2. Whether behaviour must stay exactly as today (assumed yes: only fewer round trips).
3. Whether to move variable merging fully to the backend, or keep a thin frontend preview for the variable popover and editors.
4. Event contract cleanup (snake_case fields inside camelCase tagged enum) as part of A or separate.

## How to resume

Start with the `superpowers:brainstorming` skill and this file as context. The original inventory came from a read-only investigation (agent report in the 2026-10-11 session). Re-verify the counts above before relying on them, since the code moves quickly.
