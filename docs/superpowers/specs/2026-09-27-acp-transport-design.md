# ACP Transport (Subproject B)

## Context

Rocket is gaining an AI assist feature in the Scripts tab, built on the Agent Client Protocol (ACP). The full feature was decomposed into five subprojects (build order A → B → {C, D in parallel} → E); see the project memory `project_acp_ai_assist_feature.md` for the complete decomposition and the decisions that apply across all of them. Subproject A (Agent Configuration & Credentials) is complete: it added the `rocket-acp` crate's `AgentConfig`/`AgentConfigRepository`, `FsAgentConfigRepo` persistence, `AgentConfigService` (with `resolve_credential`/`test_agent_config`), the four `agent_configs` Tauri commands, and the `AgentConfigsDialog` settings UI.

This spec covers **only subproject B**: extending `rocket-acp` with the ACP protocol transport itself — process lifecycle, the JSON-RPC handshake, and chat-only streamed turns — plus three Tauri commands so the capability is independently exercisable (e.g. via devtools) before subproject C's chat UI exists.

**Out of scope for this subproject:** tool-calling and the MCP server (subproject D), the chat UI (subproject C — B's three Tauri commands exist so C can consume them without B needing to guess C's interaction shape), and the autonomous safety valve / opt-in gating (subproject E — there is no "run a request" tool-call capability yet for that gate to apply to).

## Protocol grounding

The following was confirmed by a live fetch of agentclientprotocol.com and the `agentclientprotocol/rust-sdk` GitHub repo (September 2026), not inferred:

- **Transport:** newline-delimited JSON-RPC 2.0 over stdio. One JSON object per line on stdin/stdout, no LSP-style `Content-Length` headers. Agent stdout must carry only valid ACP messages; stderr is free for the agent's own logging and may be captured, forwarded, or ignored by the client.
- **Handshake:** `initialize` (client → agent) with `protocolVersion` (integer, currently `1`), `clientCapabilities: {fs: {readTextFile, writeTextFile}, terminal}`, and `clientInfo`; the agent responds with `agentCapabilities`, `agentInfo`, `authMethods`. Then `session/new` with `cwd` (absolute path) and `mcpServers` (array), returning `sessionId`.
- **Prompting:** `session/prompt` with `{sessionId, prompt: ContentBlock[]}` (a text block is `{"type":"text","text":"..."}`). The agent streams `session/update` *notifications* back while the request is pending — the chat-relevant kind is `session_update: "agent_message_chunk"`, carrying `messageId` and a `content` block that appends to that message. The original `session/prompt` call finally resolves with `stopReason`: `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`, or an agent-specific `_`-prefixed custom reason.
- **No ACP-level shutdown or heartbeat method exists.** Process supervision (detecting a crash or hang) is entirely the client's own responsibility via normal subprocess exit-code/stream-closed handling — not something the protocol provides a construct for.
- **Confirmed gap, not a blocker:** whether `session/cancel` is a notification or an id-bearing request could not be confirmed from the fetched docs. B does not use `session/cancel` (there is no cancellation UI yet), so this doesn't affect this subproject — it's a forward note for whichever later subproject adds cancellation.

## Dependency decision

B depends on the official **`agent-client-protocol`** crate (crates.io, v2.2.0 at time of writing, Apache-2.0) rather than hand-rolling the JSON-RPC framing and message types.

Why: it is a full client/agent/proxy *runtime*, not just wire-format schema types — subprocess spawning, newline-delimited JSON-RPC framing, request/response correlation, and notification dispatch are all built in. Its async runtime is `async-io`/`async-process` (tokio appears only as a dev-dependency), so it doesn't force a conflicting async runtime into a codebase that already uses tokio throughout. It's actively maintained (commits on the day this was researched) and permissively licensed. Its MSRV is 1.88.0; this machine's installed `rustc` is 1.94.0 (verified via `rustc --version`), so there is no toolchain blocker.

Example client-side API shape found during research (for the plan to verify exactly against the crate's current docs.rs page — this spec fixes the *decision* to depend on this crate, not the plan's exact code):

```rust
let agent = AcpAgent::from_args([...])?;
Client::builder()
    .on_receive_notification(async move |n: SessionNotification, _cx| { /* handle session/update */ })
    .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
        connection.send_request(InitializeRequest::new(ProtocolVersion::V1)).block_task().await?;
        connection.send_request(NewSessionRequest::new(cwd)).block_task().await?;
        connection.send_request(PromptRequest::new(session_id, prompt)).block_task().await?;
        Ok(())
    }).await
```

## Architecture & crate placement

Following the same "trait in the domain crate + infra impl" split subproject A already established (and that mirrors `rocket-git`):

```rust
// crates/rocket-acp/src/session.rs (new module)
#[async_trait]
pub trait AcpSessionClient: Send + Sync {
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
    ) -> DomainResult<String>; // -> session_id

    async fn send_prompt(
        &self,
        session_id: &str,
        prompt: String,
        chunk_tx: UnboundedSender<String>,
    ) -> DomainResult<String>; // -> stop_reason

    async fn end_session(&self, session_id: &str) -> DomainResult<()>;
}
```

This trait stays protocol-focused — it has no knowledge of `DomainEvent`, Tauri, or the `agent-client-protocol` crate's own types, using only primitives, `DomainResult`, and `UnboundedSender<String>`. `rocket-acp` therefore gains `async-trait` (for `#[async_trait]`) and `tokio` (for `UnboundedSender`) as new dependencies — **not** `agent-client-protocol` itself, which is needed only by the concrete implementation below. Streaming comes back through a plain channel, not a publisher.

`rocket-infra` gets the concrete implementation, `AcpAgentClient`, built on `agent-client-protocol`. It holds a session map (`Arc<Mutex<HashMap<String, RunningSession>>>`, keyed by the ACP-provided `sessionId` directly — no separate Rocket-side id translation layer), spawns the process, and forwards `session/update` `agent_message_chunk` notifications into the `chunk_tx` channel passed to `send_prompt`. Other update kinds (`tool_call`, `plan`, `usage_update`, etc.) are ignored — out of scope until subproject D.

`rocket-app` gets a new `AcpSessionService`:

```rust
pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
}
```

It resolves the agent's command/args/credential via `AgentConfigService`, calls the trait, and translates the raw channel chunks into `DomainEvent` publishes — the same shape `CollectionRunnerService` already uses (holding its own publisher and emitting events as it goes) rather than pushing that responsibility down into a domain trait.

**One small interface addition to subproject A:** `AgentConfigService` currently exposes only `list`/`save`/`delete`/`resolve_credential`/`test_agent_config` — no way to fetch a single config's `command`/`args`/`working_dir`/`credential_env_var`. B adds:

```rust
pub fn get(&self, id: &str) -> DomainResult<AgentConfig> {
    self.repo.get(id)?.ok_or_else(|| DomainError::NotFound(id.to_string()))
}
```

This is a normal small extension for a new consumer's sake, not scope creep — the same way subproject A itself added `SecretManagerService::resolve_secret_value` purely for `AgentConfigService`'s own use.

## Session lifecycle & data flow

**`start_session(agent_config_id, cwd)`** — `cwd` is supplied by the caller; B has no collection/workspace context of its own, so it doesn't invent one. Subproject C will pass the real folder path once it exists.
1. `AcpSessionService` calls `AgentConfigService::get(agent_config_id)` and `resolve_credential(agent_config_id)`.
2. Builds `env = [(config.credential_env_var, credential_value)]` and calls `AcpSessionClient::start_session(config.command, config.args, cwd, env)`.
3. The infra impl spawns the process via `agent-client-protocol`, sends `initialize` declaring `clientCapabilities: {fs: false, terminal: false}` (no tool support in this subproject), then `session/new` with `mcpServers: []`, and stores the resulting `sessionId` in its map.
4. `AcpSessionService` publishes `DomainEvent::AcpSessionStarted` and returns the session id.

**`send_prompt(session_id, text)`**
1. `AcpSessionService` creates an `UnboundedSender`/`Receiver` pair, spawns a task reading the receiver and publishing `DomainEvent::AcpSessionChunk` per chunk, then calls `AcpSessionClient::send_prompt(session_id, text, tx)`.
2. The infra impl sends `session/prompt`; as `agent_message_chunk` updates arrive, their text is forwarded through `tx`.
3. When `session/prompt` resolves with `stopReason`, `AcpSessionService` **drops its `tx` handle and awaits the chunk-reading task's completion** (it exits once the channel closes and drains) before publishing `DomainEvent::AcpSessionFinished { session_id, stop_reason }`. This ordering is required: `AcpSessionChunk` events for every chunk the agent sent must reach `TauriEventBus` before `AcpSessionFinished` does, or a consumer (subproject C's UI) could observe "finished" while chunk text is still arriving. Needs an explicit test pinning this ordering, not just an assumption that `tokio::spawn` scheduling happens to cooperate.

**`end_session(session_id)`** — infra impl **explicitly kills the child process** (not merely dropping the connection handle/stdin), then removes the map entry. This matters because neither `std::process::Child` nor `tokio::process::Child` kills the child on drop by default — that's opt-in behavior (e.g. `kill_on_drop`), never automatic — so "drop the connection" alone would leak the process. No ACP-level shutdown handshake exists (see Protocol grounding), so an explicit kill is the only teardown mechanism available.

**Crash/hang handling:** if the child's stdout closes or a request errors mid-call, the infra impl surfaces this as a `DomainError`; `AcpSessionService` publishes `DomainEvent::AcpSessionFailed` and removes the session from its map (the process is already gone in this case — no kill needed). `send_prompt` has a bounded timeout — a fixed 120-second constant for this subproject, not user-configurable — on expiry it is treated identically to a crash, **plus an explicit process kill**: the process is genuinely still running (hung, not crashed), so skipping the kill here would leak it indefinitely, with the injected credential still present in its environment for as long as it lives. Sequence on timeout: kill the process, then `DomainError::Internal` + `AcpSessionFailed` + map cleanup.

## Credential handling & security

- The credential is resolved fresh via `AgentConfigService::resolve_credential` on every `start_session` call — never cached — so a rotated RocketVault secret takes effect on the next session with no invalidation logic needed.
- It is injected only into the spawned child's environment, scoped to that one process — never into Rocket's own process environment, never logged.
- **Rule:** no `Debug`/`Display`/`tracing` instrumentation anywhere in the new code may include the raw credential value or the full `env` list passed to `start_session`. Error messages referencing a failed spawn may name `command`/`args`/`cwd`, never `env`. This needs a dedicated test asserting a deliberately-failing spawn's error string does not contain a fake test credential value used in that test.
- The `agent-client-protocol` crate's own `tracing` usage logs protocol-level JSON-RPC traffic, not process environment, so credential leakage through its instrumentation isn't expected — worth a quick confirmation against its source during planning rather than an assumption.

## Error handling / DomainError mapping

| Condition | Mapping |
|---|---|
| Unknown `agent_config_id` | `DomainError::NotFound` (from `AgentConfigService::get`/`resolve_credential`, established in subproject A) |
| Spawn failure (bad `command`) | `DomainError::InvalidInput` — same class `test_agent_config` already uses for this exact failure mode |
| Unknown or already-ended `session_id` | `DomainError::NotFound` |
| Agent crash or protocol-level error | `DomainError::Internal`, session removed from the map, `AcpSessionFailed` published |
| `send_prompt` timeout (fixed 120s) | `DomainError::Internal` with a message like `"agent did not respond within 120s"`, same cleanup as a crash |

**Review Focus for the eventual plan:**
- Calling `send_prompt`/`end_session` on an already-removed session must return `NotFound` cleanly, not panic — this falls out naturally from the map lookup as long as failure/end always removes the entry, but needs an explicit test pinning it.
- Every `AcpSessionChunk` event for a prompt must be published before that prompt's `AcpSessionFinished` event (see Session lifecycle) — needs an explicit test, not an assumption about task-scheduling order.
- `end_session` and the timeout path must actually terminate the child process, not merely stop tracking it (see Session lifecycle) — needs an explicit test that the process is gone (e.g. asserting its PID no longer exists, or that a wait/exit-status call resolves) after each path, not just that the map entry was removed.

## Tauri IPC surface

```rust
#[tauri::command]
pub async fn start_agent_session(agent_config_id: String, cwd: String, svc: State<'_, AcpSessionService>) -> Result<String, DomainError>;

#[tauri::command]
pub async fn send_agent_prompt(session_id: String, prompt: String, svc: State<'_, AcpSessionService>) -> Result<String, DomainError>;

#[tauri::command]
pub async fn end_agent_session(session_id: String, svc: State<'_, AcpSessionService>) -> Result<(), DomainError>;
```

Named consistently with subproject A's `agent`-prefixed style, not `acp`-prefixed — ACP stays an internal implementation detail, the same way "RocketVault" doesn't leak into the secret-manager command names. `stop_reason` stays a plain `String`, not a modeled enum, because ACP allows agent-specific `_`-prefixed custom reasons — a fixed Rust enum would need an escape hatch anyway, so it isn't worth modeling yet.

Streaming reaches the frontend the same way `CollectionRunnerService` already does: `TauriEventBus` maps the new `DomainEvent` variants (added in `rocket-shared`: `AcpSessionStarted`, `AcpSessionChunk { session_id, text }`, `AcpSessionFinished { session_id, stop_reason }`, `AcpSessionFailed { session_id, error }`) to named events — `agent-session-started`, `agent-session-chunk`, `agent-session-finished`, `agent-session-failed` — that subproject C's chat UI (or, for B's own end-to-end verification, a devtools `listen(...)` call) can subscribe to.

## Testing strategy

- **Unit tests** (`rocket-app`): `AcpSessionService` tested against a fake `AcpSessionClient` (in-memory, no real process), the same inline-mock convention every service in this crate already uses — covering credential resolution, event publishing, and the error-mapping table above.
- **Integration tests** (`rocket-infra`): a fixture "test agent" binary that speaks real ACP over stdio, built using `agent-client-protocol`'s own Agent-side builder API so it's protocol-correct without hand-writing raw JSON-RPC — it responds to `initialize`/`session/new` and echoes a canned `session/update` chunk plus a `stopReason` for `session/prompt`. This proves the real framing/spawning/streaming code works end-to-end without needing an LLM-backed agent installed in CI. The exact fixture API needs confirming against the crate's current docs during planning — flagged here as a planning-time task, not assumed.
- **Manual verification:** exercising `start_agent_session`/`send_agent_prompt` against a real installed agent binary via devtools isn't possible in this headless sandbox — same disclosed limitation as subproject A. The plan should state this explicitly rather than claim it was done.

## Out of scope for this subproject

- Tool-calling and the MCP server — subproject D.
- The chat UI — subproject C. B's three Tauri commands exist specifically so C can consume them without B needing to guess C's interaction shape, and so B is independently exercisable via devtools before C exists.
- The autonomous safety valve / per-collection opt-in gating — subproject E. B's sessions aren't gated by any toggle yet, since there is no "run a request" tool-call capability yet for that gate to apply to.
- `session/cancel` and any cancellation UI — no cancellation capability exists yet; the protocol ambiguity noted above is a forward note only.
