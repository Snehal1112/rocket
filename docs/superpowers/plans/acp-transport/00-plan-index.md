# ACP Transport (Subproject B) — Plan Index

**Spec:** [../../specs/2026-09-27-acp-transport-design.md](../../specs/2026-09-27-acp-transport-design.md)

**Context:** subproject B of the ACP AI-assist feature (see project memory `project_acp_ai_assist_feature.md`). Depends on subproject A (`rocket-acp`'s `AgentConfig`/`AgentConfigRepository`, `AgentConfigService`, the `agent_configs` Tauri commands — all implemented and reviewed already). Delivers process lifecycle, the ACP JSON-RPC handshake, and chat-only streamed turns, plus three Tauri commands so it's independently exercisable before subproject C's chat UI exists.

## Plan breakdown — 5 plans, 11 tasks (max 3 per plan)

| # | Plan | Tasks | Crate/area | Depends on |
|---|---|---|---|---|
| 01 | [AcpSessionClient trait](2026-09-27-acp-transport-plan-01-domain-trait.md) | 1 | `rocket-acp` | — |
| 02 | [AcpSession DomainEvent variants](2026-09-27-acp-transport-plan-02-domain-events.md) | 1 | `rocket-shared` | — |
| 03 | [AcpAgentClient + fixture test agent](2026-09-27-acp-transport-plan-03-infra-client.md) | 3 | `rocket-infra` | 01 |
| 04 | [AgentConfigService::get + AcpSessionService](2026-09-27-acp-transport-plan-04-app-service.md) | 3 | `rocket-app` | 01, 02, 03 |
| 05 | [Tauri commands + event bus + wiring](2026-09-27-acp-transport-plan-05-tauri-commands.md) | 3 | `src-tauri` | 02, 04 |

Plans 01 and 02 are single-task plans — each is one cohesive, self-contained addition (a trait with no internal state to split, and four `DomainEvent` variants added the same way the just-landed `FlowRunStarted`/`FlowStepCompleted`/`FlowRunFinished` trio was, in one commit) with no natural second slice to carve out without artificial splitting.

Each plan file ends with a **Next Plan** section and a **Post-Implementation Review** section (an Opus-model subagent reviewing that plan's own diff for interface gaps, code quality, and DDD boundary conformance, with authority to fix what it finds) — same process this project used for subproject A.

## Protocol grounding this series is written against

Verified 2026-09-27 against agentclientprotocol.com and the `agentclientprotocol/rust-sdk` GitHub repo (`main` branch, crate `agent-client-protocol` v2.2.0, Apache-2.0). Re-verify against the exact installed version if it has moved since.

- Transport: newline-delimited JSON-RPC 2.0 over stdio, no `Content-Length` headers. Agent stdout carries only ACP messages; stderr is free for agent logging.
- Handshake: `initialize` (`protocolVersion`, `clientCapabilities`, `clientInfo`) → `session/new` (`cwd`, `mcpServers`) → `sessionId`.
- Prompting: `session/prompt` (`sessionId`, `prompt: ContentBlock[]`) resolves with `stopReason` (`StopReason::EndTurn`/`MaxTokens`/`MaxTurnRequests`/`Refusal`/`Cancelled`, `#[non_exhaustive]`). Streaming arrives as `session/update` notifications (`SessionUpdate::AgentMessageChunk`) while the prompt call is pending.
- No ACP-level shutdown/heartbeat method — process supervision (crash/hang detection, killing on end/timeout) is entirely this codebase's responsibility.
- Client-role (spawning a subprocess agent) API: `Client.builder().on_receive_notification(handler, on_receive_notification!()).connect_with(agent, |connection: ConnectionTo<Agent>| async move { connection.send_request(...).block_task().await })`, where `agent` is built via a spawn-config type referred to in research as `AcpAgent::from_args(command, args)` — **re-confirm this exact type name against `docs.rs/agent-client-protocol/2.2.0` as the first step of Plan 03**, since it wasn't independently re-verified in the second research pass (which focused on the agent role).
- Agent-role (the fixture test binary implements this) API, verified directly from `examples/simple_agent.rs` and `tests/session_ordering.rs` in the crate's own repo:
  ```rust
  use agent_client_protocol::{Agent, Client, ConnectionTo, Responder, Result, Stdio};
  use agent_client_protocol::schema::v1::{
      AgentCapabilities, InitializeRequest, InitializeResponse,
      NewSessionRequest, NewSessionResponse, SessionId,
      PromptRequest, PromptResponse,
      SessionNotification, SessionUpdate, ContentChunk, ContentBlock, TextContent, StopReason,
  };

  Agent
      .builder()
      .on_receive_request(
          async move |req: InitializeRequest, responder, _conn: ConnectionTo<Client>| {
              responder.respond(InitializeResponse::new(req.protocol_version).agent_capabilities(AgentCapabilities::new()))
          },
          agent_client_protocol::on_receive_request!(),
      )
      .on_receive_request(
          async move |_req: NewSessionRequest, responder: Responder<NewSessionResponse>, _conn: ConnectionTo<Client>| {
              responder.respond(NewSessionResponse::new(SessionId::new("fixture-session")))
          },
          agent_client_protocol::on_receive_request!(),
      )
      .on_receive_request(
          async move |req: PromptRequest, responder: Responder<PromptResponse>, conn: ConnectionTo<Client>| {
              conn.send_notification(SessionNotification::new(
                  req.session_id,
                  SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new("fixture reply")))),
              ))?;
              responder.respond(PromptResponse::new(StopReason::EndTurn))
          },
          agent_client_protocol::on_receive_request!(),
      )
      .connect_to(Stdio::new())
      .await
  ```
  No feature flags needed for either role in v1 (only v2/`unstable_*` extras are gated).

## Locked interface contract

### `rocket-acp` (new, Plan 01)

```rust
// crates/rocket-acp/src/session.rs
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
        chunk_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> DomainResult<String>; // -> stop_reason

    async fn end_session(&self, session_id: &str) -> DomainResult<()>;
}
```

### `rocket-shared` (new, Plan 02)

```rust
// crates/rocket-shared/src/events.rs — new DomainEvent variants
AcpSessionStarted { session_id: String },
AcpSessionChunk { session_id: String, text: String },
AcpSessionFinished { session_id: String, stop_reason: String },
AcpSessionFailed { session_id: String, error: String },
```

### `rocket-infra` (new, Plan 03)

```rust
// crates/rocket-infra/src/acp_agent_client.rs
pub struct AcpAgentClient { /* Arc<Mutex<HashMap<String, RunningSession>>> */ }
impl AcpAgentClient {
    pub fn new() -> Self;
}
impl AcpSessionClient for AcpAgentClient { /* ... */ }
```

Fixture binary: `crates/rocket-infra/src/bin/test_acp_agent.rs` (a `[[bin]]` target in `rocket-infra/Cargo.toml`, resolved in tests via `env!("CARGO_BIN_EXE_test_acp_agent")` — the same Cargo-provided mechanism subproject A's `test_agent_config` tests already use via `env!("CARGO")`).

### `rocket-app` (modified/new, Plan 04)

```rust
// crates/rocket-app/src/agent_config_service.rs — new method
impl AgentConfigService {
    pub fn get(&self, id: &str) -> DomainResult<AgentConfig>;
}
```

```rust
// crates/rocket-app/src/acp_session_service.rs
pub struct AcpSessionService { /* session_client, event_publisher, agent_config_service */ }
impl AcpSessionService {
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
    ) -> Self;
    pub async fn start_session(&self, agent_config_id: &str, cwd: &str) -> DomainResult<String>;
    pub async fn send_prompt(&self, session_id: &str, prompt: String) -> DomainResult<String>;
    pub async fn end_session(&self, session_id: &str) -> DomainResult<()>;
}
```

### `src-tauri` (new, Plan 05)

```rust
// src-tauri/src/commands/acp_sessions.rs
#[tauri::command]
pub async fn start_agent_session(agent_config_id: String, cwd: String, svc: State<'_, AcpSessionService>) -> Result<String, DomainError>;
#[tauri::command]
pub async fn send_agent_prompt(session_id: String, prompt: String, svc: State<'_, AcpSessionService>) -> Result<String, DomainError>;
#[tauri::command]
pub async fn end_agent_session(session_id: String, svc: State<'_, AcpSessionService>) -> Result<(), DomainError>;
```

`TauriEventBus` (`src-tauri/src/tauri_event_bus.rs`) gains match arms: `AcpSessionStarted` → `"agent-session-started"`, `AcpSessionChunk` → `"agent-session-chunk"`, `AcpSessionFinished` → `"agent-session-finished"`, `AcpSessionFailed` → `"agent-session-failed"`.

## Execution note for whoever runs these plans

Run in numeric order. Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` per plan, per each file's own header. After each plan's tasks are done, run that plan's Post-Implementation Review step before starting the next plan.
