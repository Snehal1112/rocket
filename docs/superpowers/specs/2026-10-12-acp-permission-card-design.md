# ACP permission card: design

Date: 2026-10-12. Status: draft for review.

## Problem

Gemini CLI (`--experimental-acp`) calls `session/request_permission` before it runs a tool that needs confirmation. This includes every MCP tool from a server it does not trust, such as `propose_changes`.

`acp_agent_client.rs` answers every such request at once with a reject option ("Rocket never grants a permission"). Gemini then returns `Tool "propose_changes" was canceled by the user.` to the model. The tool never runs, so no proposal exists and no approval card appears.

The auto-deny rests on one assumption: Rocket tools are allowed in advance, so no request is expected. That holds only for Claude, through `_meta.claudeCode.options.allowedTools` in `agent_isolation.rs`. Gemini ignores that `_meta`.

## Evidence (Gemini CLI 0.46.0)

- `gemini-LOO67E54.js` `runTool`: if `shouldConfirmExecute` returns details, it calls `connection.requestPermission` and cancels the tool when the outcome is cancel.
- MCP confirmation options are `proceed_always_server` ("Allow all server tools for this session") and `proceed_always_tool`. The allowlist is a static set keyed by server name, so one approval covers every later call to that server in the process.
- The request carries no server name. The title is `displayName || toolName`, and a string `command` param replaces it. The call id is the model's `fc.id` when present.
- Gemini also loads `~/.gemini/settings.json` MCP servers (for example `tolaria`), and their requests look the same as Rocket's.

## Decision

Rocket shows every permission request as a card in the assistant chat and returns the user's choice to the agent. Rocket never auto-approves and never tries to tell "a Rocket tool" apart from other tools, because the request has no reliable signal for it (see Evidence). The user approves once per session with the agent's own "Allow all server tools for this session" option. Gemini then stops asking for `rocket` tools.

Chosen over: auto-approving Rocket tools by matching (spoofable through `command` and through a same-named tool in a user-level MCP server), and a per-session Gemini settings file with `trust: true` (fragile, Gemini-only).

## Backend

- **`rocket-acp`**:
  - New `AcpUpdate::PermissionRequest { request_id, call_id, title, kind, summary, options }` and `AcpUpdate::PermissionResolved { request_id, option_id: Option<String> }`.
  - `PermissionOption { id, name, kind }`, where `kind` is `allow_once | allow_always | reject_once | reject_always`.
  - `summary` is a short text of the tool arguments, masked and capped at 500 characters.
  - New trait method `AcpSessionClient::resolve_permission(session_id, request_id, option_id: Option<&str>) -> DomainResult<()>`, with a default body that returns `NotFound`, so the four existing test doubles need no change. `None` means cancelled.
- **`rocket-infra`**:
  - A `PermissionBroker` (new file `acp_permissions.rs`) owns the pending map: `register`, `resolve`, `drain_cancelled`. It is unit-testable without a process.
  - The `RequestPermissionRequest` handler **must not wait inside the handler**: `on_receive_request` callbacks run inside the connection's dispatch loop, so waiting there would freeze every other message, including the prompt response and the cancel. The handler registers the request, sends `PermissionRequest`, then calls `connection.spawn(...)` for a task that awaits the answer and calls `responder.respond(...)`, and returns at once.
  - With no active turn (no update sender) it denies at once, as today.
- **Draining**: pending requests are answered `Cancelled` by `cancel()` (the ACP rule: on cancel, pending permission requests are answered cancelled), when a turn ends, and in `end_session`. A drained or answered request sends `PermissionResolved` on the turn's channel when it still exists.
- **`resolve_permission` validation**: the request must exist and not be answered yet, and `option_id` must be one the agent offered. Otherwise it returns a `DomainError` without touching the agent.
- **`rocket-app`**:
  - `AcpSessionService::respond_permission` forwards to the client.
  - `publish_update` maps the two updates to `DomainEvent::AcpPermissionRequested` and `DomainEvent::AcpPermissionResolved` (`TauriEventBus` delivers them; `NullEventPublisher` in tests).
  - **Idle clock**: `send_prompt` kills a session after 120 s without an update. The turn loop tracks pending request ids from those two updates and does not count idle time while any is pending, so an unanswered card never ends the session. The clock restarts when the answer arrives.
- **`src-tauri`**: a thin command `respond_agent_permission(session_id, request_id, option_id: Option<String>)`. The IPC DTO is camelCase. Persistence structs are unchanged.

No timeout. The agent waits for the user, and stopping the turn cancels the request.

## Frontend

- `assistant-store`: `permissions: AgentPermission[]`, each `pending` or answered. Cleared with the session, like `proposals`.
- `AssistantPermissionCard` (shadcn `Card`, `Button`, `Badge`; lucide icons): tool title, summary, one button per offered option. Reject kinds use the outline or destructive style. `execute` and `edit` kinds get a caution badge. After an answer it becomes a one-line row showing the chosen option, or "Cancelled".
- Cards live in their own "Permissions" section above the proposals section, not in the message timeline. An answered card stays there as a one-line row until the session ends. The card text says the agent is waiting for the answer.
- The event bridge in `assistant-session.ts` maps the two events to store actions.

## Edge cases

| Case | Behavior |
|---|---|
| Stop turn while a card is open | Drained as cancelled; card shows "Cancelled". |
| Session ends, agent exits, webview reload | Drained; `end_stale_assistant_sessions` already ends sessions on load. |
| Several requests at once (parallel tool calls) | Each has its own card and `request_id`. |
| Unknown or repeated `request_id`, or `option_id` not offered | `DomainError`; the agent is untouched. |
| Secrets in arguments | `summary` uses the existing masking helper (the plan names it); the full arguments are not sent. |
| Claude / Codex agents | Same path. Claude is pre-approved and should send none; a request that does arrive now shows a card instead of an auto-deny. |
| Permission cards in saved conversations | Not saved (transient). Out of scope for v1. |

## Security note

Today the auto-deny is the only thing stopping Gemini's built-in shell and file tools. After this change the user can approve them. This is intended, but it is a behavior change: the card must show the real command or path for `execute` and `edit` requests. Disabling Gemini's built-in tools per session, as `tools: []` does for Claude, is a separate follow-up and is not part of this work.

## Testing

- **Rust unit tests**: handler parks and replies; no active turn denies at once; drain on cancel, end and exit; `resolve_permission` validation; the event is published.
- **Integration test**: extend `crates/rocket-infra/tests/acp_agent_client.rs` with a fake agent that sends `session/request_permission` mid-turn. Cases: answered with an allow option, answered with a reject option, cancelled by stop.
- **Frontend**: store tests (add, resolve, clear on fatal), card test (buttons, styles, inline collapse), event bridge test.
- **Manual**: a real Gemini session. Ask for a change, click "Allow all server tools for this session", see the proposal card. A second tool call must not prompt again.
- **Verification** (the project rules): `cargo check -j4`, focused `cargo test -j4`, `yarn tsc --noEmit`, `yarn check`.

## Docs to update when shipped

`.claude/tauri-commands.md` (new command), `docs/features/assistant-followups.md` (the Gemini built-in tools follow-up), `.claude/frontend.md` (the store field).

## Out of scope

Persisting permission cards in saved conversations; disabling Gemini built-in tools; remembering approvals across sessions; per-agent auto-approve settings.
