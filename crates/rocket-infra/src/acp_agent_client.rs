//! `AcpAgentClient`: spawns an ACP agent process and speaks the Agent Client
//! Protocol over stdio using the real `agent-client-protocol` crate.
//!
//! See `RunningSession` below for why the connection is driven from a
//! background task rather than directly inside `start_session`.

use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, FileSystemCapabilities, Implementation, InitializeRequest,
    NewSessionRequest, SessionNotification, SessionUpdate,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent as AgentRole, ByteStreams, Client, ConnectionTo,
};
use async_process::Child;
use rocket_acp::AcpSessionClient;
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::{mpsc::UnboundedSender, oneshot, Mutex};

/// A running ACP agent process plus the connection handle used to talk to
/// it. Stored in the session map keyed by the ACP-provided session id.
///
/// `connection` is a `ConnectionTo<AgentRole>` clone rather than the crate's
/// higher-level `ActiveSession` wrapper. `Builder::connect_with`'s closure is
/// foreground-owned: the JSON-RPC dispatch loop that actually drives message
/// I/O only runs while that closure's future is being polled. Since
/// `start_session` must return long before `send_prompt`/`end_session`
/// (Task 3) are called, the whole `connect_with(...)` future is driven in a
/// background task that outlives this call (see `start_session`), and the
/// cheaply-clonable `ConnectionTo` handle is handed back out through a
/// channel. `ConnectionTo` is `Clone` and safe to use from any task as long
/// as the background task keeps driving the connection.
struct RunningSession {
    /// Connection handle for sending `session/prompt` and other requests to
    /// the agent.
    connection: ConnectionTo<AgentRole>,
    /// The spawned agent process. `end_session` (Task 3) must kill it
    /// explicitly -- dropping a connection handle does not kill the
    /// underlying child process by default in either `std` or `tokio`, and
    /// there is no ACP-level shutdown handshake.
    child: Mutex<Child>,
    /// Set by `send_prompt` for the duration of one call, read by the
    /// notification handler registered at connect time -- `session/update`
    /// is a push notification uncorrelated with any specific request, so
    /// this indirection is how a fresh per-call `chunk_tx` receives it.
    current_chunk_tx: Arc<std::sync::Mutex<Option<UnboundedSender<String>>>>,
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
        // We need the raw `Child` handle so `end_session` (Task 3) can kill
        // it explicitly.
        //
        // This also means a nonexistent command fails synchronously right
        // here, before any connection/handshake machinery starts, and the
        // resulting error is built only from `command`'s OS-level spawn
        // failure (e.g. "No such file or directory") -- never from `env`.
        let (child_stdin, child_stdout, _child_stderr, child) =
            agent.spawn_process().map_err(|e| {
                DomainError::InvalidInput(format!("failed to spawn agent command '{command}': {e}"))
            })?;

        let transport = ByteStreams::new(child_stdin, child_stdout);

        let current_chunk_tx: Arc<std::sync::Mutex<Option<UnboundedSender<String>>>> =
            Arc::new(std::sync::Mutex::new(None));
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

        tokio::spawn(async move {
            let outcome = Client
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
                                // (Task 3) kills `child`; this closure must
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
                })
                .await;

            if let Err(e) = outcome {
                if let Ok(mut guard) = ready_tx.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(Err(e.to_string()));
                    }
                }
            }
        });

        let (session_id, connection) = ready_rx
            .await
            .map_err(|_| {
                DomainError::Internal(format!(
                    "agent command '{command_owned}': ACP handshake task ended without a result"
                ))
            })?
            .map_err(|e| DomainError::Internal(format!("ACP handshake failed: {e}")))?;

        let running = Arc::new(RunningSession {
            connection,
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
