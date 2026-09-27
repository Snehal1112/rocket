//! `AcpAgentClient`: spawns an ACP agent process and speaks the Agent Client
//! Protocol over stdio using the real `agent-client-protocol` crate.
//!
//! See `RunningSession` below for why the connection is driven from a
//! background task rather than directly inside `start_session`.

use std::collections::HashMap;
use std::sync::{Arc, PoisonError};

use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, FileSystemCapabilities, Implementation, InitializeRequest,
    NewSessionRequest, PromptRequest, SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent as AgentRole, ByteStreams, Client, ConnectionTo,
};
use async_process::Child;
use rocket_acp::AcpSessionClient;
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::{mpsc::UnboundedSender, oneshot, Mutex};
use tokio::task::JoinHandle;

/// A running ACP agent process plus the connection handle used to talk to
/// it. Stored in the session map keyed by the ACP-provided session id.
///
/// `connection` is a `ConnectionTo<AgentRole>` clone rather than the crate's
/// higher-level `ActiveSession` wrapper. `Builder::connect_with`'s closure is
/// foreground-owned: the JSON-RPC dispatch loop that actually drives message
/// I/O only runs while that closure's future is being polled. Since
/// `start_session` must return long before `send_prompt`/`end_session`
/// are called, the whole `connect_with(...)` future is driven in a
/// background task that outlives this call (see `start_session`), and the
/// cheaply-clonable `ConnectionTo` handle is handed back out through a
/// channel. `ConnectionTo` is `Clone` and safe to use from any task as long
/// as the background task keeps driving the connection.
struct RunningSession {
    /// Connection handle for sending `session/prompt` and other requests to
    /// the agent.
    connection: ConnectionTo<AgentRole>,
    /// The agent process and its background dispatch task. Every path that
    /// removes a session from the map calls `terminate()` on it explicitly.
    process: std::sync::Mutex<AgentProcess>,
    /// Serializes prompts on one session. `current_chunk_tx` holds a single
    /// sender, so two overlapping prompts would otherwise steal or clear
    /// each other's chunk stream. ACP also allows only one turn at a time.
    prompt_lock: Mutex<()>,
    /// Set by `send_prompt` for the duration of one call, read by the
    /// notification handler registered at connect time -- `session/update`
    /// is a push notification uncorrelated with any specific request, so
    /// this indirection is how a fresh per-call `chunk_tx` receives it.
    current_chunk_tx: ChunkSlot,
}

type ChunkSlot = Arc<std::sync::Mutex<Option<UnboundedSender<String>>>>;

/// Sets the per-prompt chunk sender. A poisoned lock is recovered because
/// the guarded value is a plain `Option` that cannot be left half-written.
fn set_chunk_sender(slot: &ChunkSlot, sender: Option<UnboundedSender<String>>) {
    *slot.lock().unwrap_or_else(PoisonError::into_inner) = sender;
}

/// Owns a spawned agent process and the background task that drives its
/// ACP connection, and tears both down together.
///
/// Per `agent-client-protocol`'s own docs
/// (`agent-client-protocol-2.2.0/src/concepts/connections.rs`, "Clean
/// Incoming EOF" section): `connect_with`'s closure is foreground-owned, and
/// a transport EOF (e.g. the child crashing or being killed) "does not cancel
/// unrelated work in its closure" -- only pending requests fail. Our closure
/// parks in `std::future::pending::<()>()` after the handshake, so the task
/// never stops on its own and must be aborted.
///
/// `terminate()` is called explicitly on every normal exit path (end, crash,
/// failed handshake). `Drop` also calls it as a safety net for paths that
/// have no explicit hook. The main one is a caller dropping a pending
/// `start_session` future (e.g. on timeout) before the session is stored.
struct AgentProcess {
    child: Child,
    dispatch_task: JoinHandle<()>,
    terminated: bool,
}

impl AgentProcess {
    /// Aborts the dispatch task and kills the agent's process tree.
    /// Idempotent, so the `Drop` safety net never signals a second time.
    fn terminate(&mut self) -> std::io::Result<()> {
        if self.terminated {
            return Ok(());
        }
        self.terminated = true;
        self.dispatch_task.abort();
        // `spawn_process` makes the child its own process-group leader on
        // unix. Agents are often started through wrappers (`npx`, `uvx`), so
        // killing only the direct child would orphan the real agent. An error
        // here (e.g. `ESRCH`) just means the group is already gone.
        #[cfg(unix)]
        if let Some(pid) = i32::try_from(self.child.id())
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        self.child.kill()
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

/// Explicitly terminates a session that was removed from the map.
fn terminate_session(running: &RunningSession) -> std::io::Result<()> {
    running
        .process
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .terminate()
}

/// `AcpSessionClient` implementation backed by the real `agent-client-protocol`
/// crate (v2.2.0). Spawns one OS process per session and keeps its
/// connection alive in a background task for the life of the session.
pub struct AcpAgentClient {
    sessions: Mutex<HashMap<String, Arc<RunningSession>>>,
}

impl Default for AcpAgentClient {
    fn default() -> Self {
        Self::new()
    }
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
        // `AcpAgentConfig` is built directly (rather than using
        // `AcpAgent::from_args`) so `env` is applied through its dedicated
        // `.envs()` builder method. `from_args` instead parses leading
        // `KEY=value`-shaped tokens out of a single argv-style iterator,
        // which does not fit a pre-structured `&[(String, String)]` list and
        // is not what we want here.
        let config = AcpAgentConfig::new(command)
            .args(args.iter().cloned())
            .envs(env.iter().cloned());
        let agent = AcpAgent::new(config);

        // Low-level escape hatch (`AcpAgent::spawn_process`, documented as
        // such in the crate) used deliberately instead of passing `agent`
        // itself as the transport: passing `AcpAgent` directly (as the
        // crate's own examples and the fixture-binary test do) spawns and
        // owns the child process internally with no way to get it back out.
        // We need the raw `Child` handle so `end_session` can kill
        // it explicitly.
        //
        // This also means a nonexistent command fails synchronously right
        // here, before any connection/handshake machinery starts, and the
        // resulting error is built only from `command`'s OS-level spawn
        // failure (e.g. "No such file or directory") -- never from `env`.
        let (child_stdin, child_stdout, child_stderr, child) =
            agent.spawn_process().map_err(|e| {
                DomainError::InvalidInput(format!("failed to spawn agent command '{command}': {e}"))
            })?;

        let transport = ByteStreams::new(child_stdin, child_stdout);

        let current_chunk_tx: ChunkSlot = Arc::new(std::sync::Mutex::new(None));
        let notif_chunk_tx = Arc::clone(&current_chunk_tx);

        // See the `RunningSession` doc comment for why this whole connection
        // is driven from a background task instead of directly here. The
        // handshake result (session id + a cloned `ConnectionTo`) is handed
        // back through a oneshot as soon as it is ready; the closure then
        // waits forever so the background task keeps driving the dispatch
        // loop for the life of the session.
        let (ready_tx, ready_rx) =
            oneshot::channel::<Result<(String, ConnectionTo<AgentRole>), String>>();
        let ready_tx = Arc::new(std::sync::Mutex::new(Some(ready_tx)));
        let ready_tx_for_task = Arc::clone(&ready_tx);
        let cwd = cwd.to_string();
        let command_owned = command.to_string();

        let dispatch_task = tokio::spawn(async move {
            // The agent's stderr must be drained. Dropping the pipe would make
            // the agent's next stderr write fail with EPIPE/SIGPIPE, which
            // kills or breaks many real agents. The output is discarded, not
            // logged, because an agent may echo its environment (credentials).
            let drain_stderr = async move {
                let _ = futures_lite::io::copy(child_stderr, futures_lite::io::sink()).await;
            };
            let connect = Client
                .builder()
                .on_receive_notification(
                    move |notification: SessionNotification, _cx: ConnectionTo<AgentRole>| {
                        let notif_chunk_tx = Arc::clone(&notif_chunk_tx);
                        async move {
                            if let SessionUpdate::AgentMessageChunk(chunk) = notification.update {
                                if let ContentBlock::Text(text) = chunk.content {
                                    if let Ok(guard) = notif_chunk_tx.lock() {
                                        if let Some(tx) = guard.as_ref() {
                                            let _ = tx.send(text.text);
                                        }
                                    }
                                }
                            }
                            Ok(())
                        }
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(transport, move |connection: ConnectionTo<AgentRole>| {
                    let ready_tx = Arc::clone(&ready_tx_for_task);
                    async move {
                        let handshake = async {
                            connection
                                .send_request(
                                    InitializeRequest::new(ProtocolVersion::V1)
                                        .client_capabilities(
                                            ClientCapabilities::new().fs(
                                                FileSystemCapabilities::new()
                                                    .read_text_file(false)
                                                    .write_text_file(false),
                                            ),
                                        )
                                        .client_info(Implementation::new(
                                            "rocket",
                                            env!("CARGO_PKG_VERSION"),
                                        )),
                                )
                                .block_task()
                                .await?;
                            connection
                                .send_request(NewSessionRequest::new(cwd))
                                .block_task()
                                .await
                        };

                        match handshake.await {
                            Ok(response) => {
                                let session_id = response.session_id.to_string();
                                if let Ok(mut guard) = ready_tx.lock() {
                                    if let Some(tx) = guard.take() {
                                        let _ = tx.send(Ok((session_id, connection.clone())));
                                    }
                                }
                                // The session stays open until `end_session`
                                // aborts this task; this closure must
                                // keep the connection's dispatch loop alive
                                // until then.
                                std::future::pending::<()>().await;
                                Ok(())
                            }
                            Err(e) => {
                                if let Ok(mut guard) = ready_tx.lock() {
                                    if let Some(tx) = guard.take() {
                                        let _ = tx.send(Err(e.to_string()));
                                    }
                                }
                                Err(e)
                            }
                        }
                    }
                });
            let (outcome, ()) = tokio::join!(connect, drain_stderr);

            if let Err(e) = outcome {
                if let Ok(mut guard) = ready_tx.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(Err(e.to_string()));
                    }
                }
            }
        });

        // Owned from here on, so every early return below (and a caller
        // dropping this future mid-handshake) still tears the process down.
        let mut process = AgentProcess {
            child,
            dispatch_task,
            terminated: false,
        };

        let handshake = match ready_rx.await {
            Ok(Ok(ready)) => Ok(ready),
            Ok(Err(e)) => Err(DomainError::Internal(format!("ACP handshake failed: {e}"))),
            Err(_) => Err(DomainError::Internal(format!(
                "agent command '{command_owned}': ACP handshake task ended without a result"
            ))),
        };
        let (session_id, connection) = match handshake {
            Ok(ready) => ready,
            Err(e) => {
                let _ = process.terminate();
                return Err(e);
            }
        };

        let running = Arc::new(RunningSession {
            connection,
            process: std::sync::Mutex::new(process),
            prompt_lock: Mutex::new(()),
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

        let _turn = running.prompt_lock.lock().await;
        set_chunk_sender(&running.current_chunk_tx, Some(chunk_tx));

        // `connection.send_request(...)` takes `&self` and `ConnectionTo` is
        // cheaply `Clone` and safe to call concurrently, so no lock is needed
        // around the connection itself (see `RunningSession`'s doc comment).
        let result = running
            .connection
            .send_request(PromptRequest::new(
                session_id.to_string(),
                vec![ContentBlock::Text(TextContent::new(prompt))],
            ))
            .block_task()
            .await;

        set_chunk_sender(&running.current_chunk_tx, None);

        match result {
            Ok(response) => Ok(stop_reason_to_wire_string(response.stop_reason)),
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
        terminate_session(&running)
            .map_err(|e| DomainError::Internal(format!("failed to kill agent process: {e}")))
    }
}

impl AcpAgentClient {
    /// Removes a session that has failed (e.g. the agent process crashed
    /// mid-request) from the session map, killing its child process and
    /// aborting its background dispatch-loop task. Mirrors `end_session`'s
    /// cleanup so a crashed session never survives past its process's death
    /// on any exit path.
    async fn fail_and_remove(&self, session_id: &str) {
        // Bind the removal to a `let` statement (rather than the `sessions`
        // lock guard's temporary being extended across an `if let` body) so
        // the `sessions` lock is dropped here, before termination runs --
        // matching `end_session`'s locking discipline.
        let removed = self.sessions.lock().await.remove(session_id);
        if let Some(running) = removed {
            let _ = terminate_session(&running);
        }
    }
}

/// Maps the real, `#[non_exhaustive]` `StopReason` (agent-client-protocol-
/// schema-1.9.1, `src/v1/agent.rs:3178-3201`) to the lowercase `snake_case`
/// wire strings the spec and `DomainEvent::AcpSessionFinished` expect. The
/// enum's own `#[serde(rename_all = "snake_case")]` attribute confirms this
/// casing, but `{:?}` (Debug) still renders `PascalCase` variant names, so it
/// cannot be used directly here.
///
/// Unlike `agent-client-protocol-schema` v2's `StopReason`, the v1 variant
/// this crate uses (`agent_client_protocol::schema::v1::StopReason`, which is
/// what `start_session`/`send_prompt` are built against throughout this file)
/// has no `Other(String)` catch-all carrying a custom reason -- v1 only has
/// the five fixed variants below. It is still `#[non_exhaustive]`, so a
/// wildcard arm is required to compile against a future crate version that
/// adds a variant.
fn stop_reason_to_wire_string(reason: StopReason) -> String {
    match reason {
        StopReason::EndTurn => "end_turn".to_string(),
        StopReason::MaxTokens => "max_tokens".to_string(),
        StopReason::MaxTurnRequests => "max_turn_requests".to_string(),
        StopReason::Refusal => "refusal".to_string(),
        StopReason::Cancelled => "cancelled".to_string(),
        _ => "unknown".to_string(),
    }
}
