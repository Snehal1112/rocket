//! `AcpAgentClient`: spawns an ACP agent process and speaks the Agent Client
//! Protocol over stdio using the real `agent-client-protocol` crate.
//!
//! See `RunningSession` below for why the connection is driven from a
//! background task rather than directly inside `start_session`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, Weak};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, EmbeddedResource,
    EmbeddedResourceResource, EnvVariable, FileSystemCapabilities, HttpHeader, Implementation,
    InitializeRequest, McpServer, McpServerHttp, McpServerStdio, Meta, NewSessionRequest,
    PermissionOption, PermissionOptionKind, PromptCapabilities as WirePromptCapabilities,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionConfigSelectOptions, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, StopReason, TextContent, TextResourceContents, ToolCallContent,
    ToolCallStatus as WireToolCallStatus, ToolCallUpdateFields, ToolKind,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent as AgentRole, ByteStreams, Client, ConnectionTo, Responder,
};
use async_process::Child;
use futures_lite::io::AsyncReadExt;
use rocket_acp::{
    AcpSessionClient, AcpUpdate, ConfigChoice, ConfigOption, McpServerSpec,
    PermissionOption as AcpPermissionOption, PermissionOptionKind as AcpPermissionOptionKind,
    PromptCapabilities, PromptPart, SessionInfo, ToolCallStatus,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::{mpsc::UnboundedSender, oneshot, Mutex};
use tokio::task::JoinHandle;

use crate::acp_permissions::PermissionBroker;
use crate::acp_stderr_tail::{mask_secrets, with_agent_output, StderrTail};

/// How long the startup handshake (`initialize` and `session/new`) may take.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// Shared (as a `Weak`) with the in-flight registry while the handshake
    /// runs, so `end_all_sessions` can reach it before it is in the map.
    process: SharedProcess,
    /// Serializes prompts on one session. `current_update_tx` holds a single
    /// sender, so two overlapping prompts would otherwise steal or clear
    /// each other's update stream. ACP also allows only one turn at a time.
    /// `cancel` and `set_config_option` never take this lock.
    prompt_lock: Mutex<()>,
    /// Set by `send_prompt` for the duration of one call, read by the
    /// notification handler registered at connect time -- `session/update`
    /// is a push notification uncorrelated with any specific request, so
    /// this indirection is how a fresh per-call `update_tx` receives it.
    /// Updates that arrive between turns find no sender and are dropped.
    current_update_tx: UpdateSlot,
    /// What the agent accepts in a prompt, from its `initialize` answer.
    prompt_capabilities: PromptCapabilities,
    /// The permission requests the agent has open. Answered by
    /// `resolve_permission`, or as cancelled by `cancel` and at turn end.
    permissions: Arc<PermissionBroker>,
}

type UpdateSlot = Arc<std::sync::Mutex<Option<UnboundedSender<AcpUpdate>>>>;

type SharedProcess = Arc<std::sync::Mutex<AgentProcess>>;

/// Terminates a shared agent process. A poisoned lock is recovered because
/// `terminate()` is idempotent and safe to call on any state.
fn terminate_process(process: &SharedProcess) -> std::io::Result<()> {
    process
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .terminate()
}

/// Stores a session. Session ids are only unique per agent process, so an id
/// that is already taken is refused: the new process is terminated and the
/// old session stays untouched.
fn insert_unique_session<T>(
    sessions: &mut HashMap<String, T>,
    session_id: &str,
    running: T,
    on_reject: impl FnOnce(&T),
) -> DomainResult<()> {
    if sessions.contains_key(session_id) {
        on_reject(&running);
        return Err(DomainError::Internal(
            "the agent returned a session id that is already in use".to_string(),
        ));
    }
    sessions.insert(session_id.to_string(), running);
    Ok(())
}

/// True when the error is a JSON-RPC error response from a live agent. A
/// broken or closed connection surfaces from the crate as an internal error,
/// so that code is treated as a transport failure.
fn is_agent_error_response(error: &agent_client_protocol::Error) -> bool {
    !matches!(error.code, agent_client_protocol::ErrorCode::InternalError)
}

/// Sets the per-prompt update sender. A poisoned lock is recovered because
/// the guarded value is a plain `Option` that cannot be left half-written.
fn set_update_sender(slot: &UpdateSlot, sender: Option<UnboundedSender<AcpUpdate>>) {
    *slot.lock().unwrap_or_else(PoisonError::into_inner) = sender;
}

/// Ends a turn: clears the update sender and answers every open permission
/// request as cancelled. Both happen under the slot lock, so a permission
/// request that arrives meanwhile is either drained here or sees no turn
/// (see `open_permission_request`).
fn end_turn(slot: &UpdateSlot, permissions: &PermissionBroker) {
    let mut guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
    let drained = permissions.drain_cancelled();
    // The wait tasks find no sender once the slot is cleared, so the cards
    // are closed here, while the turn's sender is still known.
    if let Some(tx) = guard.as_ref() {
        for request_id in drained {
            let _ = tx.send(AcpUpdate::PermissionResolved {
                request_id,
                option_id: None,
            });
        }
    }
    *guard = None;
}

/// Ends the turn when dropped, so a `send_prompt` future that is dropped
/// mid-turn cannot leave its update sender installed or a permission request
/// open. `end_turn` is idempotent, so an explicit drop and the end of scope
/// do not conflict.
struct TurnGuard {
    slot: UpdateSlot,
    permissions: Arc<PermissionBroker>,
}

impl TurnGuard {
    fn new(slot: UpdateSlot, permissions: Arc<PermissionBroker>) -> Self {
        Self { slot, permissions }
    }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        end_turn(&self.slot, &self.permissions);
    }
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
    terminate_process(&running.process)
}

/// Removes one entry from the in-flight registry when `start_session`
/// finishes, fails, or is dropped mid-handshake.
struct InFlightGuard<'a> {
    in_flight: &'a std::sync::Mutex<HashMap<u64, Weak<std::sync::Mutex<AgentProcess>>>>,
    id: u64,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

/// `AcpSessionClient` implementation backed by the real `agent-client-protocol`
/// crate (v2.2.0). Spawns one OS process per session and keeps its
/// connection alive in a background task for the life of the session.
pub struct AcpAgentClient {
    sessions: Mutex<HashMap<String, Arc<RunningSession>>>,
    /// Processes spawned by a `start_session` call whose handshake has not
    /// finished yet, so they are not in `sessions`. Held as `Weak` so the
    /// `AgentProcess` drop safety net still fires when that call is dropped.
    in_flight: std::sync::Mutex<HashMap<u64, Weak<std::sync::Mutex<AgentProcess>>>>,
    next_in_flight_id: AtomicU64,
    /// Set once by `end_all_sessions`. After that, no new session may be
    /// stored, because the app is exiting and nothing would kill it.
    shutting_down: AtomicBool,
    handshake_timeout: Duration,
}

impl Default for AcpAgentClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AcpAgentClient {
    /// Looks up a running session without holding the map lock afterwards.
    async fn running(&self, session_id: &str) -> DomainResult<Arc<RunningSession>> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(format!("acp session '{session_id}'")))
    }

    pub fn new() -> Self {
        Self::with_handshake_timeout(HANDSHAKE_TIMEOUT)
    }

    /// Like `new`, with another handshake limit. Used by tests.
    pub fn with_handshake_timeout(handshake_timeout: Duration) -> Self {
        Self {
            handshake_timeout,
            sessions: Mutex::new(HashMap::new()),
            in_flight: std::sync::Mutex::new(HashMap::new()),
            next_in_flight_id: AtomicU64::new(0),
            shutting_down: AtomicBool::new(false),
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
        mcp_servers: &[McpServerSpec],
        meta: Option<serde_json::Value>,
    ) -> DomainResult<SessionInfo> {
        // `_meta` must be a JSON object. Checked before anything is spawned.
        let meta: Option<Meta> = match meta {
            None => None,
            Some(serde_json::Value::Object(map)) => Some(map),
            Some(_) => {
                return Err(DomainError::InvalidInput(
                    "session meta must be a JSON object".to_string(),
                ))
            }
        };
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

        // Every value handed to the process, plus the MCP server secrets. The
        // stderr tail is masked with these before it joins a start error.
        let mut secret_values: Vec<String> = env.iter().map(|(_, v)| v.clone()).collect();
        for server in mcp_servers {
            match server {
                McpServerSpec::Http { token, .. } => secret_values.push(token.clone()),
                McpServerSpec::Stdio { env, .. } => {
                    secret_values.extend(env.iter().map(|(_, v)| v.clone()));
                }
            }
        }
        let stderr_tail = Arc::new(StderrTail::new());
        let drain_tail = Arc::clone(&stderr_tail);

        let current_update_tx: UpdateSlot = Arc::new(std::sync::Mutex::new(None));
        let notif_update_tx = Arc::clone(&current_update_tx);
        let permission_broker = Arc::new(PermissionBroker::default());
        let handler_broker = Arc::clone(&permission_broker);
        let handler_update_tx = Arc::clone(&current_update_tx);
        let handler_secrets: Arc<[String]> = secret_values.clone().into();

        // See the `RunningSession` doc comment for why this whole connection
        // is driven from a background task instead of directly here. The
        // handshake result (session id + a cloned `ConnectionTo`) is handed
        // back through a oneshot as soon as it is ready; the closure then
        // waits forever so the background task keeps driving the dispatch
        // loop for the life of the session.
        let (ready_tx, ready_rx) =
            oneshot::channel::<Result<(SessionInfo, ConnectionTo<AgentRole>), String>>();
        let ready_tx = Arc::new(std::sync::Mutex::new(Some(ready_tx)));
        let ready_tx_for_task = Arc::clone(&ready_tx);
        let cwd = cwd.to_string();
        let command_owned = command.to_string();
        let mcp_servers_owned = mcp_servers.to_vec();
        let meta_owned = meta;

        let dispatch_task = tokio::spawn(async move {
            // The agent's stderr must be drained. Dropping the pipe would make
            // the agent's next stderr write fail with EPIPE/SIGPIPE, which
            // kills or breaks many real agents. A bounded tail is kept only
            // while the session starts, to explain a failed start. It is
            // masked first and is never logged, because an agent may echo
            // its environment (credentials).
            let drain_stderr = async move {
                let mut child_stderr = child_stderr;
                let mut chunk = [0u8; 1024];
                loop {
                    match child_stderr.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => drain_tail.push(&chunk[..n]),
                    }
                }
                drain_tail.mark_eof();
            };
            let connect = Client
                .builder()
                .on_receive_notification(
                    move |notification: SessionNotification, _cx: ConnectionTo<AgentRole>| {
                        let notif_update_tx = Arc::clone(&notif_update_tx);
                        async move {
                            if let Some(update) = session_update_to_acp(notification.update) {
                                // A poisoned lock is recovered, so no update is lost.
                                if let Some(tx) = current_update_sender(&notif_update_tx) {
                                    let _ = tx.send(update);
                                }
                            }
                            Ok(())
                        }
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                // A permission request becomes a card the user answers. The
                // wait runs in a spawned task: this callback runs inside the
                // dispatch loop, and waiting here would block every other
                // message, including the prompt response and the cancel.
                .on_receive_request(
                    move |request: RequestPermissionRequest,
                          responder: Responder<RequestPermissionResponse>,
                          cx: ConnectionTo<AgentRole>| {
                        let broker = Arc::clone(&handler_broker);
                        let slot = Arc::clone(&handler_update_tx);
                        let secrets = Arc::clone(&handler_secrets);
                        async move {
                            handle_permission_request(
                                request, responder, &cx, broker, slot, &secrets,
                            )
                        }
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(transport, move |connection: ConnectionTo<AgentRole>| {
                    let ready_tx = Arc::clone(&ready_tx_for_task);
                    async move {
                        let handshake = async {
                            let init_response = connection
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
                            let prompt_capabilities = prompt_capabilities_from_wire(
                                &init_response.agent_capabilities.prompt_capabilities,
                            );
                            let selected_mcp_servers = select_mcp_servers_for_agent(
                                &mcp_servers_owned,
                                init_response.agent_capabilities.mcp_capabilities.http,
                            );
                            let response = connection
                                .send_request(
                                    NewSessionRequest::new(cwd)
                                        .mcp_servers(mcp_server_specs_to_wire(
                                            &selected_mcp_servers,
                                        ))
                                        .meta(meta_owned),
                                )
                                .block_task()
                                .await?;
                            Ok::<SessionInfo, agent_client_protocol::Error>(SessionInfo {
                                session_id: response.session_id.to_string(),
                                config_options: config_options_from_wire(
                                    response.config_options.unwrap_or_default(),
                                ),
                                prompt_capabilities,
                            })
                        };

                        match handshake.await {
                            Ok(info) => {
                                if let Ok(mut guard) = ready_tx.lock() {
                                    if let Some(tx) = guard.take() {
                                        let _ = tx.send(Ok((info, connection.clone())));
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
        let process: SharedProcess = Arc::new(std::sync::Mutex::new(AgentProcess {
            child,
            dispatch_task,
            terminated: false,
        }));

        // Register the process before the handshake so an app-exit sweep
        // that runs meanwhile can still kill it. The flag is checked after
        // registering: either the sweep sees this entry, or this call sees
        // the flag. Both use `SeqCst`, and the sweep sets the flag before it
        // drains this registry.
        let in_flight_id = self.next_in_flight_id.fetch_add(1, Ordering::SeqCst);
        self.in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(in_flight_id, Arc::downgrade(&process));
        let _in_flight_guard = InFlightGuard {
            in_flight: &self.in_flight,
            id: in_flight_id,
        };
        if self.shutting_down.load(Ordering::SeqCst) {
            let _ = terminate_process(&process);
            return Err(shutting_down_error());
        }

        let waited = tokio::time::timeout(self.handshake_timeout, ready_rx).await;
        let handshake = match waited {
            Ok(Ok(Ok(ready))) => Ok(ready),
            Ok(Ok(Err(e))) => Err(format!("ACP handshake failed: {e}")),
            Ok(Err(_)) => Err(format!(
                "agent command '{command_owned}': ACP handshake task ended without a result"
            )),
            Err(_) => Err(format!(
                "agent did not finish starting within {}; a first start may be downloading the agent, try again",
                describe_duration(self.handshake_timeout)
            )),
        };
        let (info, connection) = match handshake {
            Ok(ready) => ready,
            Err(message) => {
                // Let a process that just exited finish its last words, then
                // kill it and add the masked tail to the error.
                stderr_tail.wait_for_eof().await;
                let _ = terminate_process(&process);
                let secrets: Vec<&str> = secret_values.iter().map(String::as_str).collect();
                let tail = stderr_tail.take_masked(&secrets);
                // The whole text is masked, including the protocol error in
                // `message`, which may echo a value.
                let full = with_agent_output(message, &tail);
                return Err(DomainError::Internal(mask_secrets(&full, &secrets)));
            }
        };
        // The start worked. Free the tail; the drain task keeps reading.
        stderr_tail.discard();

        let running = Arc::new(RunningSession {
            connection,
            process,
            prompt_lock: Mutex::new(()),
            current_update_tx,
            prompt_capabilities: info.prompt_capabilities,
            permissions: permission_broker,
        });
        // The flag is read under the `sessions` lock, which the sweep also
        // takes after setting it. So a session is either stored before the
        // sweep drains the map, or it is refused and killed here.
        let mut sessions = self.sessions.lock().await;
        if self.shutting_down.load(Ordering::SeqCst) {
            drop(sessions);
            let _ = terminate_session(&running);
            return Err(shutting_down_error());
        }
        insert_unique_session(&mut sessions, &info.session_id, running, |new| {
            let _ = terminate_session(new);
        })?;
        Ok(info)
    }

    async fn send_prompt(
        &self,
        session_id: &str,
        parts: Vec<PromptPart>,
        update_tx: UnboundedSender<AcpUpdate>,
    ) -> DomainResult<String> {
        if parts.is_empty() {
            return Err(DomainError::InvalidInput(
                "a prompt needs at least one part".to_string(),
            ));
        }
        let running = self.running(session_id).await?;
        let prompt = prompt_parts_to_wire(parts, running.prompt_capabilities.embedded_context);

        // A queued prompt must not wait behind a running turn, because the
        // service's idle clock would kill the healthy session.
        let Ok(_turn) = running.prompt_lock.try_lock() else {
            return Err(DomainError::InvalidInput(
                "a turn is already running".to_string(),
            ));
        };
        set_update_sender(&running.current_update_tx, Some(update_tx));
        let turn_guard = TurnGuard::new(
            Arc::clone(&running.current_update_tx),
            Arc::clone(&running.permissions),
        );

        // `connection.send_request(...)` takes `&self` and `ConnectionTo` is
        // cheaply `Clone` and safe to call concurrently, so no lock is needed
        // around the connection itself (see `RunningSession`'s doc comment).
        let result = running
            .connection
            .send_request(PromptRequest::new(session_id.to_string(), prompt))
            .block_task()
            .await;

        // A request the agent left open cannot be answered after the turn. The
        // guard also does this when the future is dropped before this point.
        drop(turn_guard);

        match result {
            Ok(response) => Ok(stop_reason_to_wire_string(response.stop_reason)),
            Err(e) if is_agent_error_response(&e) => {
                // The agent refused this prompt but the session still works.
                // Only the code is reported, as the message may echo a value.
                Err(DomainError::InvalidInput(format!(
                    "the agent rejected the prompt (error code {})",
                    i32::from(e.code)
                )))
            }
            Err(e) => {
                self.fail_and_remove(session_id).await;
                Err(DomainError::Internal(format!("agent session failed: {e}")))
            }
        }
    }

    async fn cancel(&self, session_id: &str) -> DomainResult<()> {
        // A notification, sent without the prompt lock: the running turn
        // holds that lock until the agent answers it with `cancelled`.
        let running = self.running(session_id).await?;
        // ACP: on cancel, open permission requests are answered cancelled.
        running.permissions.drain_cancelled();
        running
            .connection
            .send_notification(CancelNotification::new(session_id.to_string()))
            .map_err(|e| DomainError::Internal(format!("failed to send cancel: {e}")))
    }

    async fn resolve_permission(
        &self,
        session_id: &str,
        request_id: &str,
        option_id: Option<&str>,
    ) -> DomainResult<()> {
        self.running(session_id)
            .await?
            .permissions
            .resolve(request_id, option_id)
    }

    async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: &str,
    ) -> DomainResult<Vec<ConfigOption>> {
        let running = self.running(session_id).await?;
        let response = running
            .connection
            .send_request(SetSessionConfigOptionRequest::new(
                session_id.to_string(),
                config_id.to_string(),
                SessionConfigOptionValue::value_id(value.to_string()),
            ))
            .block_task()
            .await
            .map_err(|e| DomainError::Internal(format!("failed to change option: {e}")))?;
        Ok(config_options_from_wire(response.config_options))
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

    async fn end_all_sessions(&self) -> DomainResult<()> {
        // Refuse new sessions first. See `start_session` for how this flag
        // pairs with the in-flight registry and the `sessions` lock.
        self.shutting_down.store(true, Ordering::SeqCst);

        // Kill processes still in their handshake. They are not in the map
        // yet, and the app exits right after this call returns.
        let in_flight: Vec<_> = self
            .in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        for process in &in_flight {
            let _ = terminate_process(process);
        }

        // Drain the whole map (rather than iterating a snapshot and removing
        // one-by-one) so a session that finishes naturally mid-sweep can't be
        // double-terminated, and so the lock is held only for the swap itself.
        let sessions = std::mem::take(&mut *self.sessions.lock().await);
        for running in sessions.values() {
            let _ = terminate_session(running);
        }
        Ok(())
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

/// Formats a limit as "30 s", or in milliseconds when under a second.
fn describe_duration(d: Duration) -> String {
    if d.as_secs() >= 1 {
        format!("{} s", d.as_secs())
    } else {
        format!("{} ms", d.as_millis())
    }
}

fn shutting_down_error() -> DomainError {
    DomainError::Internal("agent sessions are shutting down".to_string())
}

/// Maps Rocket's transport-agnostic `McpServerSpec` (owned by `rocket-acp`,
/// which must not depend on `agent-client-protocol` -- see that crate's DDD
/// boundary) to the real `agent_client_protocol::schema::v1::McpServer` wire
/// type. `select_mcp_servers_for_agent` (below, in this file) decides *which*
/// variants to keep for a given agent; this function only translates that
/// already-selected list, it never chooses Http vs Stdio itself.
fn mcp_server_specs_to_wire(specs: &[McpServerSpec]) -> Vec<McpServer> {
    specs
        .iter()
        .map(|spec| match spec {
            McpServerSpec::Http { name, url, token } => {
                McpServer::Http(McpServerHttp::new(name.clone(), url.clone()).headers(vec![
                    HttpHeader::new("Authorization", format!("Bearer {token}")),
                ]))
            }
            McpServerSpec::Stdio {
                name,
                command,
                args,
                env,
            } => McpServer::Stdio(
                McpServerStdio::new(name.clone(), command.clone())
                    .args(args.clone())
                    .env(
                        env.iter()
                            .map(|(k, v)| EnvVariable::new(k.clone(), v.clone()))
                            .collect(),
                    ),
            ),
        })
        .collect()
}

/// Picks which of the caller's `McpServerSpec`s the agent actually receives,
/// based on its negotiated `mcp_capabilities.http` (design spec, "Capability
/// negotiation"; plan index, `AcpSessionService` section). `rocket-app` offers
/// an `Http` and a `Stdio` spec under the same name; this crate chooses:
/// - agent supports HTTP: keep every `Http` spec, and drop a `Stdio` spec
///   only when an `Http` spec with the same name already covers it;
/// - agent lacks HTTP: drop every `Http` spec (ACP clients must not send an
///   HTTP server the agent never advertised) and keep the `Stdio` fallback,
///   which every agent must support.
///
/// Never blocks the session; a dropped `Http` spec with no `Stdio` fallback is
/// only logged. The log line never includes the spec itself or its token.
fn select_mcp_servers_for_agent(
    specs: &[McpServerSpec],
    agent_supports_http: bool,
) -> Vec<McpServerSpec> {
    let http_names: Vec<&str> = specs
        .iter()
        .filter_map(|spec| match spec {
            McpServerSpec::Http { name, .. } => Some(name.as_str()),
            McpServerSpec::Stdio { .. } => None,
        })
        .collect();
    let stdio_names: Vec<&str> = specs
        .iter()
        .filter_map(|spec| match spec {
            McpServerSpec::Stdio { name, .. } => Some(name.as_str()),
            McpServerSpec::Http { .. } => None,
        })
        .collect();

    if !agent_supports_http && http_names.iter().any(|name| !stdio_names.contains(name)) {
        tracing::warn!(
            "requested an HTTP MCP server with no stdio fallback, but the agent's \
             InitializeResponse did not advertise mcp_capabilities.http; not attaching it"
        );
    }

    specs
        .iter()
        .filter(|spec| match spec {
            McpServerSpec::Http { .. } => agent_supports_http,
            McpServerSpec::Stdio { name, .. } => {
                !agent_supports_http || !http_names.contains(&name.as_str())
            }
        })
        .cloned()
        .collect()
}

/// Maps one `session/update` to Rocket's typed update. Kinds Rocket does not
/// model (user echoes, plans, modes, commands, session info) and
/// non-text message chunks yield `None` and are dropped.
fn session_update_to_acp(update: SessionUpdate) -> Option<AcpUpdate> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
            ContentBlock::Text(text) => Some(AcpUpdate::Text { text: text.text }),
            _ => None,
        },
        SessionUpdate::AgentThoughtChunk(chunk) => match chunk.content {
            ContentBlock::Text(text) => Some(AcpUpdate::Thought { text: text.text }),
            _ => None,
        },
        SessionUpdate::ToolCall(call) => Some(AcpUpdate::ToolCall {
            call_id: call.tool_call_id.to_string(),
            title: call.title,
            kind: tool_kind_to_wire(call.kind).to_string(),
            status: tool_status_from_wire(call.status),
        }),
        SessionUpdate::ToolCallUpdate(update) => Some(AcpUpdate::ToolCallUpdate {
            call_id: update.tool_call_id.to_string(),
            title: update.fields.title,
            status: update.fields.status.map(tool_status_from_wire),
        }),
        SessionUpdate::ConfigOptionUpdate(update) => Some(AcpUpdate::ConfigOptions {
            options: config_options_from_wire(update.config_options),
        }),
        SessionUpdate::UsageUpdate(usage) => Some(AcpUpdate::Usage {
            used: usage.used,
            size: usage.size,
            cost_usd: usage
                .cost
                .filter(|cost| {
                    let usd = cost.currency == "USD";
                    if !usd {
                        tracing::debug!(currency = %cost.currency, "dropped a non-USD usage cost");
                    }
                    usd
                })
                .map(|cost| cost.amount),
        }),
        _ => {
            tracing::debug!("dropped a session update kind Rocket does not model");
            None
        }
    }
}

fn tool_status_from_wire(status: WireToolCallStatus) -> ToolCallStatus {
    match status {
        WireToolCallStatus::Pending => ToolCallStatus::Pending,
        WireToolCallStatus::InProgress => ToolCallStatus::InProgress,
        WireToolCallStatus::Completed => ToolCallStatus::Completed,
        WireToolCallStatus::Failed => ToolCallStatus::Failed,
        _ => ToolCallStatus::InProgress,
    }
}

/// The ACP tool kind in its snake_case wire spelling.
fn tool_kind_to_wire(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "read",
        ToolKind::Edit => "edit",
        ToolKind::Delete => "delete",
        ToolKind::Move => "move",
        ToolKind::Search => "search",
        ToolKind::Execute => "execute",
        ToolKind::Think => "think",
        ToolKind::Fetch => "fetch",
        ToolKind::SwitchMode => "switch_mode",
        _ => "other",
    }
}

fn prompt_capabilities_from_wire(caps: &WirePromptCapabilities) -> PromptCapabilities {
    PromptCapabilities {
        embedded_context: caps.embedded_context,
        image: caps.image,
    }
}

/// Maps the agent's options. Boolean options are skipped: Rocket does not
/// advertise boolean config support, and v1 shows no toggles.
fn config_options_from_wire(options: Vec<SessionConfigOption>) -> Vec<ConfigOption> {
    options
        .into_iter()
        .filter_map(config_option_from_wire)
        .collect()
}

fn config_option_from_wire(option: SessionConfigOption) -> Option<ConfigOption> {
    let SessionConfigKind::Select(select) = option.kind else {
        return None;
    };
    let choices = match select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options,
        SessionConfigSelectOptions::Grouped(groups) => {
            groups.into_iter().flat_map(|group| group.options).collect()
        }
        _ => Vec::new(),
    };
    Some(ConfigOption {
        id: option.id.to_string(),
        name: option.name,
        category: option.category.and_then(category_to_wire),
        current_value: select.current_value.to_string(),
        choices: choices
            .into_iter()
            .map(|choice| ConfigChoice {
                value: choice.value.to_string(),
                name: choice.name,
                description: choice.description,
            })
            .collect(),
    })
}

fn category_to_wire(category: SessionConfigOptionCategory) -> Option<String> {
    match category {
        SessionConfigOptionCategory::Mode => Some("mode".to_string()),
        SessionConfigOptionCategory::Model => Some("model".to_string()),
        SessionConfigOptionCategory::ModelConfig => Some("model_config".to_string()),
        SessionConfigOptionCategory::ThoughtLevel => Some("thought_level".to_string()),
        SessionConfigOptionCategory::Other(other) => Some(other),
        _ => None,
    }
}

/// Picks a reject option, so the agent hears a clear "no". Without one, the
/// only other answer that grants nothing is `Cancelled`.
fn deny_outcome(options: &[PermissionOption]) -> RequestPermissionOutcome {
    let reject = options
        .iter()
        .find(|option| matches!(option.kind, PermissionOptionKind::RejectOnce))
        .or_else(|| {
            options
                .iter()
                .find(|option| matches!(option.kind, PermissionOptionKind::RejectAlways))
        });
    match reject {
        Some(option) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
            option.option_id.clone(),
        )),
        None => RequestPermissionOutcome::Cancelled,
    }
}

/// Longest `summary` sent for a permission request.
const PERMISSION_SUMMARY_MAX_CHARS: usize = 500;

fn current_update_sender(slot: &UpdateSlot) -> Option<UnboundedSender<AcpUpdate>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Turns a permission request into a card update, then answers the agent
/// when the user has chosen. With no turn running nobody could answer, so
/// the request is denied at once.
fn handle_permission_request(
    request: RequestPermissionRequest,
    responder: Responder<RequestPermissionResponse>,
    cx: &ConnectionTo<AgentRole>,
    broker: Arc<PermissionBroker>,
    slot: UpdateSlot,
    secrets: &[String],
) -> Result<(), agent_client_protocol::Error> {
    let Some((request_id, answer)) = open_permission_request(&request, &broker, &slot, secrets)
    else {
        tracing::warn!("the agent asked for a permission outside a turn; Rocket denied it");
        return responder.respond(RequestPermissionResponse::new(deny_outcome(
            &request.options,
        )));
    };
    let task_request_id = request_id.clone();
    let spawned = cx.spawn(async move {
        // A dropped sender never grants anything: it reads as cancelled.
        let chosen = answer.await.unwrap_or(None);
        // The task reads the slot instead of keeping a sender clone, because
        // a held clone would keep the turn's update channel open.
        // The UI hears the answer before the agent does, so the card is
        // closed before the agent can end the turn.
        if let Some(tx) = current_update_sender(&slot) {
            let _ = tx.send(AcpUpdate::PermissionResolved {
                request_id: task_request_id,
                option_id: chosen.clone(),
            });
        }
        let outcome = match chosen {
            Some(option_id) => {
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id))
            }
            None => RequestPermissionOutcome::Cancelled,
        };
        // An error here means the connection is closing. It is not returned,
        // because a failed spawned task shuts the whole connection down.
        if let Err(e) = responder.respond(RequestPermissionResponse::new(outcome)) {
            tracing::debug!("could not answer a permission request: {e}");
        }
        Ok(())
    });
    if let Err(e) = spawned {
        // The connection is closing, so the request can no longer be answered.
        let _ = broker.resolve(&request_id, None);
        tracing::debug!("could not wait for a permission answer: {e}");
    }
    Ok(())
}

/// Registers a permission request and sends its card update to the running
/// turn. Returns `None` when no turn can show it. The slot lock is held for
/// the whole step, so `end_turn` either drains this request or runs first.
fn open_permission_request(
    request: &RequestPermissionRequest,
    broker: &PermissionBroker,
    slot: &UpdateSlot,
    secrets: &[String],
) -> Option<(String, oneshot::Receiver<Option<String>>)> {
    let guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
    let update_tx = guard.as_ref()?;
    let options = permission_options_from_wire(&request.options);
    let (request_id, answer) =
        broker.register(options.iter().map(|option| option.id.clone()).collect());
    let secret_refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
    let fields = &request.tool_call.fields;
    let update = AcpUpdate::PermissionRequest {
        request_id: request_id.clone(),
        call_id: request.tool_call.tool_call_id.to_string(),
        title: mask_secrets(fields.title.as_deref().unwrap_or(""), &secret_refs),
        kind: fields
            .kind
            .map(tool_kind_to_wire)
            .unwrap_or("other")
            .to_string(),
        summary: permission_summary(fields, &secret_refs),
        options,
    };
    if update_tx.send(update).is_err() {
        // The turn's receiver is gone, so no card can be shown.
        let _ = broker.resolve(&request_id, None);
        return None;
    }
    Some((request_id, answer))
}

fn permission_options_from_wire(options: &[PermissionOption]) -> Vec<AcpPermissionOption> {
    options
        .iter()
        .map(|option| AcpPermissionOption {
            id: option.option_id.to_string(),
            name: option.name.clone(),
            kind: match option.kind {
                PermissionOptionKind::AllowOnce => AcpPermissionOptionKind::AllowOnce,
                PermissionOptionKind::AllowAlways => AcpPermissionOptionKind::AllowAlways,
                PermissionOptionKind::RejectAlways => AcpPermissionOptionKind::RejectAlways,
                // Reject is the safe reading of a kind Rocket does not know.
                _ => AcpPermissionOptionKind::RejectOnce,
            },
        })
        .collect()
}

/// Short, masked text about the call: the agent's own content text, a diff's
/// path, or the raw input as a last resort.
fn permission_summary(fields: &ToolCallUpdateFields, secrets: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for item in fields.content.iter().flatten() {
        match item {
            ToolCallContent::Content(content) => {
                if let ContentBlock::Text(text) = &content.content {
                    parts.push(text.text.clone());
                }
            }
            ToolCallContent::Diff(diff) => parts.push(format!("Edit {}", diff.path.display())),
            _ => {}
        }
    }
    if parts.is_empty() {
        if let Some(raw) = &fields.raw_input {
            parts.push(raw.to_string());
        }
    }
    // The raw input is JSON text, where a secret with a quote, a backslash
    // or a control character shows up escaped. Those forms are masked too.
    let escaped: Vec<String> = secrets
        .iter()
        .filter_map(|secret| {
            let json = serde_json::to_string(secret).ok()?;
            let inner = json.strip_prefix('"')?.strip_suffix('"')?;
            (inner != *secret).then(|| inner.to_string())
        })
        .collect();
    let mut forms: Vec<&str> = secrets.to_vec();
    forms.extend(escaped.iter().map(String::as_str));
    let masked = mask_secrets(&parts.join("\n"), &forms);
    if masked.chars().count() > PERMISSION_SUMMARY_MAX_CHARS {
        let cut: String = masked.chars().take(PERMISSION_SUMMARY_MAX_CHARS).collect();
        format!("{cut}…")
    } else {
        masked
    }
}

/// Builds the prompt blocks. A resource is embedded when the agent accepts
/// embedded context, and sent as labelled plain text otherwise.
fn prompt_parts_to_wire(parts: Vec<PromptPart>, embedded_context: bool) -> Vec<ContentBlock> {
    parts
        .into_iter()
        .map(|part| match part {
            PromptPart::Text(text) => ContentBlock::Text(TextContent::new(text)),
            PromptPart::Resource {
                uri,
                mime_type,
                text,
            } if embedded_context => ContentBlock::Resource(EmbeddedResource::new(
                EmbeddedResourceResource::TextResourceContents(
                    TextResourceContents::new(text, uri).mime_type(mime_type),
                ),
            )),
            PromptPart::Resource { uri, text, .. } => {
                ContentBlock::Text(TextContent::new(format!("Context from {uri}:\n{text}")))
            }
        })
        .collect()
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
        other => {
            tracing::warn!(reason = ?other, "the agent ended a turn with an unknown stop reason");
            "unknown".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        ConfigOptionUpdate, ContentChunk, Cost, ImageContent, SessionConfigSelectGroup,
        SessionConfigSelectOption, ToolCall, ToolCallUpdate, UsageUpdate,
    };
    use tokio::sync::mpsc;

    fn text_chunk(text: &str) -> ContentChunk {
        ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
    }

    #[test]
    fn text_chunks_become_text_updates() {
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentMessageChunk(text_chunk("hi"))),
            Some(AcpUpdate::Text {
                text: "hi".to_string()
            })
        );
    }

    #[test]
    fn thought_chunks_become_thought_updates_and_non_text_thoughts_are_dropped() {
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentThoughtChunk(text_chunk(
                "checking auth"
            ))),
            Some(AcpUpdate::Thought {
                text: "checking auth".to_string()
            })
        );
        let image = ContentChunk::new(ContentBlock::Image(ImageContent::new("aGk=", "image/png")));
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentThoughtChunk(image)),
            None
        );
    }

    #[test]
    fn non_text_chunks_and_unmodelled_updates_are_dropped() {
        let image = ContentChunk::new(ContentBlock::Image(ImageContent::new("aGk=", "image/png")));
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentMessageChunk(image)),
            None
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::UserMessageChunk(text_chunk("echo"))),
            None
        );
    }

    #[test]
    fn tool_calls_map_id_title_kind_and_status() {
        let call = ToolCall::new("call-1", "Run tests")
            .kind(ToolKind::Execute)
            .status(WireToolCallStatus::InProgress);
        assert_eq!(
            session_update_to_acp(SessionUpdate::ToolCall(call)),
            Some(AcpUpdate::ToolCall {
                call_id: "call-1".to_string(),
                title: "Run tests".to_string(),
                kind: "execute".to_string(),
                status: ToolCallStatus::InProgress,
            })
        );
    }

    #[test]
    fn tool_call_updates_keep_missing_fields_as_none() {
        let update = ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new().status(WireToolCallStatus::Failed),
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::ToolCallUpdate(update)),
            Some(AcpUpdate::ToolCallUpdate {
                call_id: "call-1".to_string(),
                title: None,
                status: Some(ToolCallStatus::Failed),
            })
        );
    }

    #[test]
    fn usage_cost_is_kept_only_in_usd() {
        let usd = UsageUpdate::new(10, 100).cost(Cost::new(1.5, "USD"));
        let eur = UsageUpdate::new(10, 100).cost(Cost::new(1.5, "EUR"));
        let none = UsageUpdate::new(10, 100);
        assert_eq!(
            session_update_to_acp(SessionUpdate::UsageUpdate(usd)),
            Some(AcpUpdate::Usage {
                used: 10,
                size: 100,
                cost_usd: Some(1.5)
            })
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::UsageUpdate(eur)),
            Some(AcpUpdate::Usage {
                used: 10,
                size: 100,
                cost_usd: None
            })
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::UsageUpdate(none)),
            Some(AcpUpdate::Usage {
                used: 10,
                size: 100,
                cost_usd: None
            })
        );
    }

    #[test]
    fn config_options_flatten_groups_and_skip_boolean_options() {
        let grouped = SessionConfigOption::select(
            "model",
            "Model",
            "b",
            vec![
                SessionConfigSelectGroup::new(
                    "g1",
                    "Group 1",
                    vec![SessionConfigSelectOption::new("a", "A")],
                ),
                SessionConfigSelectGroup::new(
                    "g2",
                    "Group 2",
                    vec![SessionConfigSelectOption::new("b", "B").description("Bee")],
                ),
            ],
        )
        .category(SessionConfigOptionCategory::Other("custom".to_string()));
        let boolean = SessionConfigOption::boolean("fast", "Fast", true);

        let options = config_options_from_wire(vec![grouped, boolean]);
        assert_eq!(
            options,
            vec![ConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                category: Some("custom".to_string()),
                current_value: "b".to_string(),
                choices: vec![
                    ConfigChoice {
                        value: "a".to_string(),
                        name: "A".to_string(),
                        description: None,
                    },
                    ConfigChoice {
                        value: "b".to_string(),
                        name: "B".to_string(),
                        description: Some("Bee".to_string()),
                    },
                ],
            }]
        );
    }

    #[test]
    fn config_option_updates_map_to_config_options() {
        let option = SessionConfigOption::select(
            "effort",
            "Effort",
            "high",
            vec![SessionConfigSelectOption::new("high", "High")],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel);
        match session_update_to_acp(SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(
            vec![option],
        ))) {
            Some(AcpUpdate::ConfigOptions { options }) => {
                assert_eq!(options.len(), 1);
                assert_eq!(options[0].category.as_deref(), Some("thought_level"));
            }
            other => panic!("expected ConfigOptions, got {other:?}"),
        }
    }

    #[test]
    fn deny_outcome_prefers_reject_once_then_reject_always_then_cancelled() {
        let allow = PermissionOption::new("allow", "Allow", PermissionOptionKind::AllowOnce);
        let reject_always =
            PermissionOption::new("never", "Never", PermissionOptionKind::RejectAlways);
        let reject_once = PermissionOption::new("no", "No", PermissionOptionKind::RejectOnce);

        assert_eq!(
            deny_outcome(&[allow.clone(), reject_always.clone(), reject_once]),
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new("no"))
        );
        assert_eq!(
            deny_outcome(&[allow.clone(), reject_always]),
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new("never"))
        );
        assert_eq!(deny_outcome(&[allow]), RequestPermissionOutcome::Cancelled);
    }

    #[test]
    fn permission_summary_masks_secrets_and_is_capped() {
        let fields = ToolCallUpdateFields::new().raw_input(serde_json::json!({
            "command": "curl -H 'Authorization: tok-secret-123' https://example.com"
        }));
        let summary = permission_summary(&fields, &["tok-secret-123"]);
        assert!(!summary.contains("tok-secret-123"), "{summary}");

        let long = ToolCallUpdateFields::new().raw_input(serde_json::json!("x".repeat(900)));
        let capped = permission_summary(&long, &[]);
        assert_eq!(capped.chars().count(), PERMISSION_SUMMARY_MAX_CHARS + 1);
        assert!(capped.ends_with('…'));
    }

    #[test]
    fn permission_summary_masks_a_secret_that_json_escapes() {
        let secret = r#"pa"ss\word"#;
        let fields = ToolCallUpdateFields::new().raw_input(serde_json::json!({ "token": secret }));
        let summary = permission_summary(&fields, &[secret]);
        assert!(!summary.contains(r#"pa\"ss\\word"#), "{summary}");
        assert!(summary.contains("***"), "{summary}");
    }

    #[test]
    fn permission_summary_prefers_content_text_and_diff_paths() {
        let fields = ToolCallUpdateFields::new()
            .content(vec![
                ToolCallContent::from("run the tests"),
                ToolCallContent::Diff(agent_client_protocol::schema::v1::Diff::new(
                    "/tmp/a.txt",
                    "new",
                )),
            ])
            .raw_input(serde_json::json!({ "ignored": true }));
        assert_eq!(
            permission_summary(&fields, &[]),
            "run the tests\nEdit /tmp/a.txt"
        );
    }

    #[test]
    fn an_unknown_permission_kind_becomes_a_reject_option() {
        // `PermissionOptionKind` is non-exhaustive, so Rocket reads an
        // unknown kind as the safe one. This pins the known mappings.
        let wire = vec![
            PermissionOption::new("a", "A", PermissionOptionKind::AllowOnce),
            PermissionOption::new("b", "B", PermissionOptionKind::RejectAlways),
        ];
        let mapped = permission_options_from_wire(&wire);
        assert_eq!(mapped[0].kind, AcpPermissionOptionKind::AllowOnce);
        assert_eq!(mapped[1].kind, AcpPermissionOptionKind::RejectAlways);
    }

    fn permission_request(title: &str) -> RequestPermissionRequest {
        RequestPermissionRequest::new(
            "session-1",
            ToolCallUpdate::new(
                "call-1",
                ToolCallUpdateFields::new()
                    .title(title.to_string())
                    .kind(ToolKind::Execute),
            ),
            vec![
                PermissionOption::new("allow-once", "Allow", PermissionOptionKind::AllowOnce),
                PermissionOption::new("reject-once", "Reject", PermissionOptionKind::RejectOnce),
            ],
        )
    }

    #[test]
    fn a_permission_request_outside_a_turn_is_not_registered() {
        let broker = PermissionBroker::default();
        let slot: UpdateSlot = Arc::new(std::sync::Mutex::new(None));
        assert!(open_permission_request(&permission_request("ls"), &broker, &slot, &[]).is_none());
        let err = broker
            .resolve("perm-1", Some("allow-once"))
            .expect_err("nothing was registered");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn a_permission_request_whose_turn_receiver_is_gone_is_not_left_open() {
        let broker = PermissionBroker::default();
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx);
        let slot: UpdateSlot = Arc::new(std::sync::Mutex::new(Some(tx)));
        assert!(open_permission_request(&permission_request("ls"), &broker, &slot, &[]).is_none());
        let err = broker
            .resolve("perm-1", Some("allow-once"))
            .expect_err("the request was closed");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn a_permission_request_in_a_turn_sends_a_masked_card_and_turn_end_cancels_it() {
        let broker = PermissionBroker::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let slot: UpdateSlot = Arc::new(std::sync::Mutex::new(Some(tx)));
        let secrets = vec!["tok-secret-123".to_string()];
        let (request_id, answer) = open_permission_request(
            &permission_request("curl -H tok-secret-123"),
            &broker,
            &slot,
            &secrets,
        )
        .expect("a turn is running");

        match rx.try_recv().expect("the card update") {
            AcpUpdate::PermissionRequest {
                request_id: sent_id,
                call_id,
                title,
                kind,
                options,
                ..
            } => {
                assert_eq!(sent_id, request_id);
                assert_eq!(call_id, "call-1");
                assert_eq!(title, "curl -H ***");
                assert_eq!(kind, "execute");
                assert_eq!(options.len(), 2);
            }
            other => panic!("expected a permission request, got {other:?}"),
        }

        end_turn(&slot, &broker);
        assert_eq!(answer.await.expect("answer"), None);
        assert!(current_update_sender(&slot).is_none());
        let err = broker
            .resolve(&request_id, Some("allow-once"))
            .expect_err("a request from an ended turn");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn dropping_the_turn_guard_clears_the_slot_and_cancels_open_permissions() {
        let broker = Arc::new(PermissionBroker::default());
        let (tx, _rx) = mpsc::unbounded_channel();
        let slot: UpdateSlot = Arc::new(std::sync::Mutex::new(Some(tx)));
        let (request_id, answer) =
            open_permission_request(&permission_request("ls"), &broker, &slot, &[])
                .expect("a turn is running");

        let guard = TurnGuard::new(Arc::clone(&slot), Arc::clone(&broker));
        drop(guard);

        assert_eq!(answer.await.expect("answer"), None);
        assert!(current_update_sender(&slot).is_none());
        let err = broker
            .resolve(&request_id, Some("allow-once"))
            .expect_err("a request from an ended turn");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn turn_guard_is_a_no_op_after_end_turn() {
        let broker = Arc::new(PermissionBroker::default());
        let slot: UpdateSlot = Arc::new(std::sync::Mutex::new(None));
        let guard = TurnGuard::new(Arc::clone(&slot), Arc::clone(&broker));
        end_turn(&slot, &broker);
        drop(guard);
        assert!(current_update_sender(&slot).is_none());
    }

    #[test]
    fn resources_are_embedded_when_the_agent_supports_it() {
        let blocks = prompt_parts_to_wire(
            vec![PromptPart::Resource {
                uri: "rocket://x".to_string(),
                mime_type: Some("text/plain".to_string()),
                text: "body".to_string(),
            }],
            true,
        );
        match &blocks[..] {
            [ContentBlock::Resource(resource)] => match &resource.resource {
                EmbeddedResourceResource::TextResourceContents(contents) => {
                    assert_eq!(contents.uri, "rocket://x");
                    assert_eq!(contents.text, "body");
                    assert_eq!(contents.mime_type.as_deref(), Some("text/plain"));
                }
                other => panic!("expected text contents, got {other:?}"),
            },
            other => panic!("expected one resource block, got {other:?}"),
        }
    }

    #[test]
    fn resources_fall_back_to_text_without_embedded_context() {
        let blocks = prompt_parts_to_wire(
            vec![PromptPart::Resource {
                uri: "rocket://x".to_string(),
                mime_type: None,
                text: "body".to_string(),
            }],
            false,
        );
        match &blocks[..] {
            [ContentBlock::Text(text)] => assert_eq!(text.text, "Context from rocket://x:\nbody"),
            other => panic!("expected one text block, got {other:?}"),
        }
    }

    #[test]
    fn a_json_rpc_error_response_is_not_a_transport_failure() {
        use agent_client_protocol::Error;
        assert!(is_agent_error_response(&Error::invalid_params()));
        assert!(is_agent_error_response(&Error::auth_required()));
        assert!(is_agent_error_response(&Error::new(-32001, "custom")));
        assert!(!is_agent_error_response(&Error::internal_error()));
    }

    #[test]
    fn a_duplicate_session_id_is_refused_and_the_old_session_kept() {
        let mut sessions: HashMap<String, u32> = HashMap::new();
        insert_unique_session(&mut sessions, "s1", 1, |_| panic!("not a duplicate"))
            .expect("first insert");
        let mut rejected = None;
        let err = insert_unique_session(&mut sessions, "s1", 2, |new| rejected = Some(*new))
            .expect_err("duplicate id");
        assert!(matches!(err, DomainError::Internal(_)));
        assert_eq!(rejected, Some(2));
        assert_eq!(sessions.get("s1"), Some(&1));
    }

    #[tokio::test]
    async fn end_turn_reports_drained_permissions_as_resolved_before_clearing_the_slot() {
        let broker = PermissionBroker::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let slot: UpdateSlot = Arc::new(std::sync::Mutex::new(Some(tx)));
        let (request_id, _answer) =
            open_permission_request(&permission_request("ls"), &broker, &slot, &[])
                .expect("a turn is running");
        assert!(matches!(
            rx.try_recv().expect("card"),
            AcpUpdate::PermissionRequest { .. }
        ));

        end_turn(&slot, &broker);

        match rx.try_recv().expect("resolved update") {
            AcpUpdate::PermissionResolved {
                request_id: id,
                option_id,
            } => {
                assert_eq!(id, request_id);
                assert_eq!(option_id, None);
            }
            other => panic!("expected PermissionResolved, got {other:?}"),
        }
        assert!(current_update_sender(&slot).is_none());
    }
}
