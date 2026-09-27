# ACP Transport Plan 03: AcpAgentClient + Fixture — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `AcpSessionClient` (Plan 01) as `AcpAgentClient` in `rocket-infra`, built on the `agent-client-protocol` crate, plus a fixture ACP-speaking test-agent binary to test it against without needing a real LLM-backed agent installed.

**Architecture:** Confirmed 2026-09-27 against the `agent-client-protocol` crate's own source (`agentclientprotocol/rust-sdk`, `main` branch, v2.2.0):

- `ConnectionTo<Counterpart>` is cheaply `Clone` — "all clones refer to the same underlying connection... easy to share across async tasks" (doc comment, `src/agent-client-protocol/src/jsonrpc.rs`). It stays usable as long as the `connect_with(...)`/`connect_to(...)` future it came from hasn't finished — in practice, that future is `tokio::spawn`ed and never returns until the session ends.
- `SessionBuilder::start_session()` returns `ActiveSession<'static, Counterpart>` — an **owned**, non-scoped handle (`session_id` + `connection` fields, `pub fn send_prompt(&mut self, ...)`) that can be moved out of the connecting closure (e.g. via a channel) and reused later, from a different call, for multi-turn prompting. This is exactly what `start_session`/`send_prompt` being separate `AcpSessionClient` trait calls needs.
- **No process-kill method exists on the connection or the default spawn path** — the crate's default `AcpAgent`-as-`ConnectTo` path installs a private `ChildGuard` that terminates the process (and its process group, on Unix) only when the connecting future itself is dropped, which is too indirect for this plan's explicit, on-demand kill requirement (end_session, timeout). Instead, use the documented low-level escape hatch: `AcpAgent::spawn_process(&self) -> Result<(ChildStdin, ChildStdout, ChildStderr, async_process::Child), Error>` — spawns the process and hands back raw stdio plus the **raw `Child`**, which this plan's code keeps for itself and calls `.kill()` on directly, instead of using the crate's own `ConnectTo` implementation for `AcpAgent`.

**Tech Stack:** Rust, `agent-client-protocol` v2.2.0, `tokio`.

**Spec:** `docs/superpowers/specs/2026-09-27-acp-transport-design.md` (Session lifecycle & data flow, Credential handling & security). Plan index: `docs/superpowers/plans/acp-transport/00-plan-index.md` (has the full protocol grounding and locked interface contract, including the fixture agent's exact source).

## Global Constraints

- `AcpAgentClient` (this crate) is the **only** place `agent-client-protocol` is a dependency — `rocket-acp`'s trait (Plan 01) knows nothing about it.
- The spawned child's environment (`env` parameter to `start_session`) must never appear in any `tracing`/error-message output this plan's code produces — error messages may name `command`/`args`/`cwd`, never `env`. This needs a dedicated test (see Task 2).
- Every session's process must be explicitly killed via the retained `Child` handle on `end_session`, on a crash/protocol-error, and on the Plan 04 timeout path — never left to implicit drop-based cleanup. This is the central correctness property of this plan; every task touching process lifecycle carries its own explicit test for it.
- **Before writing Task 2's transport-wiring code**, confirm the exact call shape for feeding `spawn_process()`'s returned stdio into a `.connect_with(...)`-compatible connection against `docs.rs/agent-client-protocol/2.2.0` (or `cargo doc -p rocket-infra --open` once the dependency is added) — the crate exposes `crate::ByteStreams`/`crate::Lines` for this (confirmed to exist, used internally in `src/agent-client-protocol/src/stdio.rs`), but their exact constructor signatures were not independently re-verified during this plan's research. This is the one piece of this plan's code most likely to need a small adjustment against the installed version.
- Test code uses `.expect("message")` for fallible calls, never the bare panicking shorthand — production code must never reach for that shorthand either; every fallible call maps its error into `DomainResult` explicitly.

## Review Focus

- A crashed/killed agent process must never leave a `RunningSession` entry in the map — every exit path (explicit end, timeout, crash detection) must remove it.
- Two sequential `send_prompt` calls on the same session (simulating a real multi-turn chat) must both succeed and both receive their own chunks — this is the entire reason `ActiveSession` is stored owned rather than re-created per call; a test must actually exercise two prompts against the fixture, not just one.
- The credential-in-`env` non-leakage rule above needs a test that deliberately fails a spawn (bad command) with a real credential-shaped value in `env` and asserts the resulting error string doesn't contain it.

---

## Task 1: Fixture test-agent binary

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`
- Create: `crates/rocket-infra/src/bin/test_acp_agent.rs`
- Create: `crates/rocket-infra/src/acp_agent_client.rs` (empty module stub, registered but not yet implemented — this task only needs the fixture binary; the client module is built in Task 2)

**Interfaces:**
- Produces: a spawnable binary at `env!("CARGO_BIN_EXE_test_acp_agent")` (Cargo's own mechanism for locating a sibling `[[bin]]` target from tests — the same technique subproject A's tests already use via `env!("CARGO")`) that speaks real ACP: responds to `initialize`, `session/new`, and `session/prompt` (echoing a canned `session/update` chunk, then `StopReason::EndTurn`) — consumed by Task 2 and Task 3's integration tests.

- [ ] **Step 1: Add the dependency and the `[[bin]]` target**

In `crates/rocket-infra/Cargo.toml`, add to `[dependencies]`:

```toml
rocket-acp.workspace = true
agent-client-protocol = "2.2"
```

(`rocket-acp.workspace = true` was already added in subproject A's Plan 02 for `FsAgentConfigRepo` — confirm it's present rather than adding a duplicate line.)

Add a new `[[bin]]` section (this crate currently has none — add it after `[dependencies]`, before `[target.'cfg(windows)'.dependencies]`):

```toml
[[bin]]
name = "test_acp_agent"
path = "src/bin/test_acp_agent.rs"
test = false
doc = false
```

- [ ] **Step 2: Write the fixture agent**

```rust
// crates/rocket-infra/src/bin/test_acp_agent.rs
use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Responder, Result, Stdio};

#[tokio::main]
async fn main() -> Result<()> {
    Agent
        .builder()
        .on_receive_request(
            async move |req: InitializeRequest, responder, _conn: ConnectionTo<Client>| {
                responder.respond(
                    InitializeResponse::new(req.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: NewSessionRequest,
                        responder: Responder<NewSessionResponse>,
                        _conn: ConnectionTo<Client>| {
                responder.respond(NewSessionResponse::new(SessionId::new("fixture-session")))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: PromptRequest,
                        responder: Responder<PromptResponse>,
                        conn: ConnectionTo<Client>| {
                // Test hook: a sentinel prompt text triggers an abrupt,
                // uncooperative exit (no response, connection just drops) so
                // Task 3's crash-handling path can be exercised against a
                // real broken connection rather than only a fake.
                if let Some(ContentBlock::Text(text)) = req.prompt.first() {
                    if text.text == "__CRASH__" {
                        std::process::exit(1);
                    }
                }
                conn.send_notification(SessionNotification::new(
                    req.session_id,
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new("fixture reply"),
                    ))),
                ))?;
                responder.respond(PromptResponse::new(StopReason::EndTurn))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(Stdio::new())
        .await
}
```

- [ ] **Step 3: Create the (empty) client module stub**

```rust
// crates/rocket-infra/src/acp_agent_client.rs
// Implemented in Plan 03 Task 2/3. This stub exists so the module is
// registered and the crate compiles before those tasks land.
```

Register it in `crates/rocket-infra/src/lib.rs` alongside the other modules:

```rust
pub mod acp_agent_client;
```

- [ ] **Step 4: Write the failing test proving the fixture works**

```rust
// crates/rocket-infra/src/acp_agent_client.rs (append)
#[cfg(test)]
mod tests {
    use agent_client_protocol::schema::v1::{InitializeRequest, ProtocolVersion};
    use agent_client_protocol::{Agent as AgentRole, Client, ConnectionTo};

    #[tokio::test]
    async fn fixture_agent_completes_initialize_handshake() {
        // AcpAgent::from_args's exact name/signature is re-verified as part
        // of Task 2 (see this plan's Global Constraints) — this test uses
        // it minimally, just to prove the fixture binary itself is a valid
        // ACP agent, before AcpAgentClient exists.
        let agent = agent_client_protocol::AcpAgent::from_args([
            env!("CARGO_BIN_EXE_test_acp_agent"),
        ])
        .expect("build spawn config for fixture agent");

        let result = Client
            .builder()
            .connect_with(agent, |connection: ConnectionTo<AgentRole>| async move {
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await
            })
            .await;

        result.expect("initialize handshake should succeed against the fixture agent");
    }
}
```

- [ ] **Step 5: Run tests to verify they fail**

Run: `cargo test -p rocket-infra acp_agent_client -j4`
Expected: FAIL — either a compile error if `AcpAgent::from_args` doesn't match the installed crate's real API (see Global Constraints — fix the call to match before proceeding), or the fixture binary not yet built. Resolve any naming mismatch against `docs.rs/agent-client-protocol/2.2.0` here, in this step, before continuing — this is the cheapest point to catch it.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-infra acp_agent_client -j4`
Expected: PASS — 1 test.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/Cargo.toml crates/rocket-infra/src/bin/test_acp_agent.rs crates/rocket-infra/src/acp_agent_client.rs crates/rocket-infra/src/lib.rs
git commit -m "feat(infra): add fixture ACP test agent"
```

---

## Task 2: `AcpAgentClient::start_session`

**Files:**
- Modify: `crates/rocket-infra/src/acp_agent_client.rs`

**Interfaces:**
- Consumes: `AcpSessionClient` (Plan 01), the fixture binary (Task 1).
- Produces: `AcpAgentClient::new()` and its `start_session` implementation — consumed by Task 3 of this plan (`send_prompt`/`end_session` extend the same struct) and Plan 04 (`AcpSessionService` holds `Box<dyn AcpSessionClient>` backed by this type).

- [ ] **Step 1: Write the failing test**

```rust
// crates/rocket-infra/src/acp_agent_client.rs (add to the existing tests module)
use rocket_acp::AcpSessionClient;
use rocket_shared::error::DomainError;

fn fixture_command() -> String {
    env!("CARGO_BIN_EXE_test_acp_agent").to_string()
}

#[tokio::test]
async fn start_session_returns_a_session_id() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session should succeed against the fixture agent");
    assert!(!session_id.is_empty());
}

#[tokio::test]
async fn start_session_fails_clearly_for_a_nonexistent_command() {
    let client = AcpAgentClient::new();
    let err = client
        .start_session("definitely-not-a-real-binary-xyz123", &[], "/tmp", &[])
        .await
        .expect_err("nonexistent command must fail, not panic");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test]
async fn start_session_error_never_contains_the_credential_value() {
    let client = AcpAgentClient::new();
    let err = client
        .start_session(
            "definitely-not-a-real-binary-xyz123",
            &[],
            "/tmp",
            &[("ANTHROPIC_API_KEY".to_string(), "sk-super-secret-test-value".to_string())],
        )
        .await
        .expect_err("nonexistent command must fail");
    let message = err.to_string();
    assert!(
        !message.contains("sk-super-secret-test-value"),
        "error message must never contain the credential value, got: {message}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra acp_agent_client -j4`
Expected: FAIL with "cannot find struct `AcpAgentClient`" (compile error — it doesn't exist yet).

- [ ] **Step 3: Implement the struct and `start_session`**

First, confirm the exact `ByteStreams`/`Lines` transport-construction call against `docs.rs/agent-client-protocol/2.2.0` per this plan's Global Constraints, then implement:

```rust
// crates/rocket-infra/src/acp_agent_client.rs (add above the tests module)
use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    ClientCapabilities, ClientInfo, FileSystemCapability, InitializeRequest, NewSessionRequest,
    ProtocolVersion,
};
use agent_client_protocol::{AcpAgent, Agent as AgentRole, Client, ConnectionTo};
use async_process::Child;
use rocket_acp::AcpSessionClient;
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::{mpsc::UnboundedSender, Mutex};

struct RunningSession {
    active_session: Mutex<agent_client_protocol::ActiveSession<'static, AgentRole>>,
    child: Mutex<Child>,
    /// Set by `send_prompt` for the duration of one call, read by the
    /// notification handler registered at connect time — `session/update`
    /// is a push notification uncorrelated with any specific request, so
    /// this indirection is how a fresh per-call `chunk_tx` receives it.
    current_chunk_tx: Arc<std::sync::Mutex<Option<UnboundedSender<String>>>>,
}

pub struct AcpAgentClient {
    sessions: Mutex<HashMap<String, Arc<RunningSession>>>,
}

impl AcpAgentClient {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl AcpSessionClient for AcpAgentClient {
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
    ) -> DomainResult<String> {
        let agent = AcpAgent::from_args(std::iter::once(command).chain(args.iter().map(String::as_str)))
            .map_err(|e| DomainError::InvalidInput(format!("agent command '{command}': {e}")))?;
        // env is applied on the spawn-config object here (exact method name
        // to confirm against docs.rs alongside the transport-wiring check
        // above) — never logged or included in any error path.
        let (_stdin, _stdout, _stderr, child) = agent
            .spawn_process()
            .map_err(|e| DomainError::InvalidInput(format!("agent command '{command}': {e}")))?;

        let current_chunk_tx: Arc<std::sync::Mutex<Option<UnboundedSender<String>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let notif_chunk_tx = Arc::clone(&current_chunk_tx);

        // Wire the retained stdio into the connection via the crate's
        // transport helpers (ByteStreams/Lines — confirm exact constructor
        // per this plan's Global Constraints), registering the notification
        // handler that forwards agent_message_chunk text into whichever
        // chunk_tx is currently active.
        let connection_result = Client
            .builder()
            .on_receive_notification(
                {
                    let notif_chunk_tx = Arc::clone(&notif_chunk_tx);
                    async move |n: agent_client_protocol::schema::v1::SessionNotification, _cx| {
                        if let agent_client_protocol::schema::v1::SessionUpdate::AgentMessageChunk(chunk) =
                            n.update
                        {
                            if let agent_client_protocol::schema::v1::ContentBlock::Text(text) = chunk.content {
                                let guard = notif_chunk_tx.lock().expect("lock chunk sender");
                                if let Some(tx) = guard.as_ref() {
                                    let _ = tx.send(text.text);
                                }
                            }
                        }
                    }
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(/* transport built from _stdin/_stdout above */, |connection: ConnectionTo<AgentRole>| async move {
                connection
                    .send_request(
                        InitializeRequest::new(ProtocolVersion::V1)
                            .client_capabilities(
                                ClientCapabilities::new()
                                    .fs(FileSystemCapability::new().read_text_file(false).write_text_file(false)),
                            )
                            .client_info(ClientInfo::new("rocket", "Rocket")),
                    )
                    .block_task()
                    .await?;
                let active_session = connection
                    .start_session(NewSessionRequest::new(cwd.to_string()))
                    .await?;
                Ok(active_session)
            })
            .await;

        let active_session = connection_result
            .map_err(|e| DomainError::Internal(format!("ACP handshake failed: {e}")))?;
        let session_id = active_session.session_id.to_string();

        let running = Arc::new(RunningSession {
            active_session: Mutex::new(active_session),
            child: Mutex::new(child),
            current_chunk_tx,
        });
        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), running);
        Ok(session_id)
    }

    async fn send_prompt(
        &self,
        _session_id: &str,
        _prompt: String,
        _chunk_tx: UnboundedSender<String>,
    ) -> DomainResult<String> {
        todo!("implemented in Task 3")
    }

    async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
        todo!("implemented in Task 3")
    }
}
```

This step's code has two explicitly-flagged spots (`env` application on the spawn-config, and the transport-wiring `connect_with` call) that need confirming against the installed crate version's docs before this compiles — resolve those now, adjusting names as needed; the surrounding structure (session map, `RunningSession`, the notification-handler indirection) does not depend on getting those two names exactly right on the first try.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-infra acp_agent_client -j4`
Expected: PASS — 3 new tests (from Step 1) plus the 1 from Task 1.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/acp_agent_client.rs
git commit -m "feat(infra): add AcpAgentClient::start_session"
```

---

## Task 3: `send_prompt`, `end_session`, and process lifecycle

**Files:**
- Modify: `crates/rocket-infra/src/acp_agent_client.rs`

**Interfaces:**
- Consumes: `RunningSession`/`AcpAgentClient` (Task 2).
- Produces: the completed `AcpSessionClient` implementation — consumed by Plan 04's `AcpSessionService`.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-infra/src/acp_agent_client.rs (add to the existing tests module)
use tokio::sync::mpsc;

#[tokio::test]
async fn send_prompt_streams_a_chunk_and_returns_a_stop_reason() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = client
        .send_prompt(&session_id, "hello".to_string(), tx)
        .await
        .expect("send_prompt should succeed against the fixture agent");

    assert_eq!(stop_reason, "end_turn");
    assert_eq!(rx.recv().await, Some("fixture reply".to_string()));
}

#[tokio::test]
async fn send_prompt_works_twice_on_the_same_session_for_multi_turn_chat() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    for _ in 0..2 {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let stop_reason = client
            .send_prompt(&session_id, "hello again".to_string(), tx)
            .await
            .expect("send_prompt should succeed on a reused session");
        assert_eq!(stop_reason, "end_turn");
        assert_eq!(rx.recv().await, Some("fixture reply".to_string()));
    }
}

#[tokio::test]
async fn send_prompt_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt("no-such-session", "hi".to_string(), tx)
        .await
        .expect_err("unknown session id must error, not panic");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn end_session_kills_the_process_and_removes_the_session() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    client
        .end_session(&session_id)
        .await
        .expect("end_session should succeed");

    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(&session_id, "hi".to_string(), tx)
        .await
        .expect_err("session must be gone after end_session");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn end_session_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let err = client
        .end_session("no-such-session")
        .await
        .expect_err("unknown session id must error, not panic");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn a_crashed_agent_is_removed_from_the_session_map() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(&session_id, "__CRASH__".to_string(), tx)
        .await
        .expect_err("an abrupt process exit must surface as an error, not panic or hang");
    assert!(matches!(err, DomainError::Internal(_)));

    // The crashed session must be gone — proven the same way end_session's
    // cleanup is proven: a follow-up call on the same id is NotFound, not a
    // second crash-shaped error.
    let (tx2, _rx2) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(&session_id, "hi".to_string(), tx2)
        .await
        .expect_err("a crashed session must be removed from the map, not left dangling");
    assert!(matches!(err, DomainError::NotFound(_)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra acp_agent_client -j4`
Expected: FAIL — the `send_prompt`/`end_session` stubs panic via `todo!`.

- [ ] **Step 3: Implement `send_prompt` and `end_session`**

```rust
// crates/rocket-infra/src/acp_agent_client.rs — replace the two todo!() bodies

async fn send_prompt(
    &self,
    session_id: &str,
    prompt: String,
    chunk_tx: UnboundedSender<String>,
) -> DomainResult<String> {
    let running = self
        .sessions
        .lock()
        .await
        .get(session_id)
        .cloned()
        .ok_or_else(|| DomainError::NotFound(format!("acp session '{session_id}'")))?;

    {
        let mut guard = running.current_chunk_tx.lock().expect("lock chunk sender");
        *guard = Some(chunk_tx);
    }

    let mut active_session = running.active_session.lock().await;
    let result = active_session
        .send_prompt(/* real PromptRequest built from `prompt` per the locked contract's ContentBlock::Text shape */)
        .await;
    drop(active_session);
    {
        let mut guard = running.current_chunk_tx.lock().expect("lock chunk sender");
        *guard = None;
    }

    match result {
        Ok(stop_reason) => Ok(stop_reason_to_wire_string(stop_reason)),
        Err(e) => {
            self.fail_and_remove(session_id).await;
            Err(DomainError::Internal(format!("agent session failed: {e}")))
        }
    }
}

async fn end_session(&self, session_id: &str) -> DomainResult<()> {
    let running = self
        .sessions
        .lock()
        .await
        .remove(session_id)
        .ok_or_else(|| DomainError::NotFound(format!("acp session '{session_id}'")))?;
    running
        .child
        .lock()
        .await
        .kill()
        .map_err(|e| DomainError::Internal(format!("failed to kill agent process: {e}")))?;
    Ok(())
}
```

Add a small private helper used by the crash path, plus the explicit `StopReason` → wire-string mapping (do not rely on `{:?}` formatting for this — its casing may not match the `snake_case` values the spec and `DomainEvent::AcpSessionFinished` expect; write the match explicitly against `docs.rs/agent-client-protocol-schema/1.9.1`'s real `StopReason` variants):

```rust
impl AcpAgentClient {
    async fn fail_and_remove(&self, session_id: &str) {
        if let Some(running) = self.sessions.lock().await.remove(session_id) {
            let _ = running.child.lock().await.kill();
        }
    }
}

fn stop_reason_to_wire_string(reason: agent_client_protocol::schema::v1::StopReason) -> String {
    use agent_client_protocol::schema::v1::StopReason;
    match reason {
        StopReason::EndTurn => "end_turn".to_string(),
        StopReason::MaxTokens => "max_tokens".to_string(),
        StopReason::MaxTurnRequests => "max_turn_requests".to_string(),
        StopReason::Refusal => "refusal".to_string(),
        StopReason::Cancelled => "cancelled".to_string(),
        // #[non_exhaustive] — an agent-specific custom reason falls through
        // here; confirm the real variant shape (likely a named field
        // carrying the custom string) against the installed schema crate
        // and forward it verbatim rather than losing it to a fixed label.
        other => format!("{other:?}"),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-infra acp_agent_client -j4`
Expected: PASS — all tests in this module (10 total across the three tasks).

- [ ] **Step 5: Run the full crate suite**

Run: `cargo test -p rocket-infra -j4`
Expected: PASS — confirms no regression to any other `rocket-infra` test.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/acp_agent_client.rs
git commit -m "feat(infra): add AcpAgentClient send_prompt and end_session"
```

---

## Next Plan

[Plan 04: AgentConfigService::get + AcpSessionService](2026-09-27-acp-transport-plan-04-app-service.md) — the `rocket-app` orchestration layer that resolves credentials via `AgentConfigService` and drives this plan's `AcpAgentClient` through the `AcpSessionClient` trait, translating results into `DomainEvent`s (Plan 02).

## Post-Implementation Review

Before starting Plan 04, dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-infra/Cargo.toml`, `crates/rocket-infra/src/lib.rs`,
> `crates/rocket-infra/src/bin/test_acp_agent.rs`,
> `crates/rocket-infra/src/acp_agent_client.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interface — does `AcpAgentClient` fully
>    and correctly implement `AcpSessionClient` (Plan 01) as the plan index's
>    locked interface contract promises Plan 04 will consume? Pay particular
>    attention to the places this plan explicitly flagged as needing
>    confirmation against the real installed crate version (the transport
>    wiring in `start_session`, the `env` application on the spawn-config,
>    and the `StopReason` → wire-string conversion in `send_prompt`) — verify
>    these were actually resolved correctly against real crate docs, not
>    left as a guess that happened to compile.
> 2. Code quality and correctness versus this plan's Review Focus section —
>    no `RunningSession` ever survives past its process's death on any exit
>    path (end/timeout/crash), two sequential prompts on one session both
>    work, and no credential value ever reaches an error message or log line.
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    `agent-client-protocol` is a dependency of this crate only, not
>    `rocket-acp`; no bare panicking shorthand in production code paths
>    (test code may use `.expect(...)`).
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-infra -j4` and
> `cargo check -p rocket-infra -j4`, and confirm they still pass. Report what
> you found and fixed.

Only proceed to Plan 04 once this review comes back clean (or its fixes are applied and re-verified).
