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
  - New `AcpUpdate::PermissionRequest { request_id, call_id, title, kind, summary, options }`.
  - `PermissionOption { id, name, kind }`, where `kind` is `allow_once | allow_always | reject_once | reject_always`.
  - `summary` is a short, masked text of the tool arguments, capped in length.
  - New trait method `AcpSessionClient::resolve_permission(session_id, request_id, option_id: Option<&str>) -> DomainResult<()>`. `None` means cancelled.
- **`rocket-infra` (`acp_agent_client.rs`)**: the `RequestPermissionRequest` handler:
  1. Looks up the turn's current `update_tx`. With no active turn it denies at once, exactly as today.
  2. Creates a `request_id` and stores a `oneshot::Sender` in a per-session pending map.
  3. Sends `AcpUpdate::PermissionRequest`.
  4. Awaits the oneshot and replies `Selected(option_id)` or `Cancelled`.
- **Draining**: the pending map answers `Cancelled` and clears on turn stop (`cancel`), session end, agent process exit and app shutdown. A request can never outlive its turn.
- **`resolve_permission` validation**: the request must exist and not be answered yet, and `option_id` must be one the agent offered. Otherwise it returns a `DomainError` without touching the agent. Double answers are rejected.
- **`rocket-app`**: `AcpSessionService::respond_permission` forwards to the client. It publishes a `DomainEvent::AcpPermissionRequested` for the UI (`TauriEventBus` delivers it; `NullEventPublisher` in tests) and an `AcpPermissionResolved` when the request is answered or drained.
- **`src-tauri`**: a thin command `respond_agent_permission(session_id, request_id, option_id: Option<String>)`. The IPC DTO is camelCase. Persistence structs are unchanged.

No timeout. The agent waits for the user, and stopping the turn cancels the request.

## Frontend

- `assistant-store`: `pendingPermissions: AgentPermissionRequest[]` plus an anchor to the last chat message, like `proposalAnchors`. Cleared with the session on fatal errors and on `resolve`.
- `AssistantPermissionCard` (shadcn `Card`, `Button`, `Badge`; lucide icons): tool title, summary, one button per offered option. Reject kinds use the outline or destructive style. `execute` and `edit` kinds get a caution badge. After an answer it collapses to a one-line inline row.
- The turn indicator reads "Waiting for your approval" while a request is pending.
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

`.claude/tauri-commands.md` (new command), `.claude/assistant-followups.md` (the Gemini built-in tools follow-up), `.claude/frontend.md` (the store field).

## Out of scope

Persisting permission cards in saved conversations; disabling Gemini built-in tools; remembering approvals across sessions; per-agent auto-approve settings.
