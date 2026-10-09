use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rocket_acp::{
    AcpSessionClient, AcpUpdate, ConfigOption, PromptPart, SessionInfo, ToolCallStatus,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::agent_config_service::AgentConfigService;
use crate::agent_isolation::{SessionIsolation, ROCKET_MCP_SERVER_NAME};

/// Releases per-session resources that live outside this crate, such as the
/// MCP tool server, its caches and the session's scratch directories.
pub trait SessionCleanup: Send + Sync {
    /// Called exactly once per session on EVERY end path: end_session, idle
    /// timeout, failed prompt, end_all_sessions. Idempotent.
    fn on_session_ended(&self, session_id: &str);
}

/// A cleanup that does nothing. Used by tests and by callers that own no
/// per-session resources.
pub struct NoopSessionCleanup;

impl SessionCleanup for NoopSessionCleanup {
    fn on_session_ended(&self, _session_id: &str) {}
}

/// Orchestrates ACP agent sessions. It resolves an agent's command and
/// credential through `AgentConfigService`, then drives the injected
/// `AcpSessionClient`. It publishes `AcpSession*`, `AcpToolActivity`,
/// `AcpConfigOptionsChanged` and `AcpUsage` domain events for the UI.
///
/// This service keeps only the set of live session ids, so it can run
/// `SessionCleanup` exactly once per session. The session client still owns
/// session state and process lifecycle.
pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    cleanup: Arc<dyn SessionCleanup>,
    live_sessions: Mutex<HashSet<String>>,
    /// Set once `end_all_sessions` ran. Written and read under the
    /// `live_sessions` lock, so `track` and the drain never interleave.
    shutting_down: AtomicBool,
    agent_config_service: Arc<AgentConfigService>,
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    prompt_idle_timeout: Duration,
}

/// Fixed idle limit for one prompt turn, from the spec. Every update from the
/// agent restarts it, so a long turn that keeps making progress is never cut
/// off. It is not user-configurable.
const DEFAULT_PROMPT_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Last known title and status of each tool call in one prompt turn. A tool
/// call update may leave either out, and the event always carries both.
type ToolCallStates = HashMap<String, (String, ToolCallStatus)>;

/// Plain-data credentials for the per-session MCP HTTP tool server, built by
/// the Tauri command layer (which owns the `AppHandle` needed to spawn the
/// real HTTP server — see this crate's `CLAUDE.md`/the plan's Design Note for
/// why that spawn does not happen in this crate) and passed into
/// `start_session` as plain values, keeping this crate free of a `tauri`
/// dependency for this feature.
#[derive(Debug, Clone)]
pub struct McpHttpServerCredentials {
    pub port: u16,
    pub token: String,
}

impl AcpSessionService {
    /// Production constructor. Uses the fixed 120-second idle limit.
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        cleanup: Arc<dyn SessionCleanup>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    ) -> Self {
        Self::with_prompt_idle_timeout(
            session_client,
            event_publisher,
            cleanup,
            agent_config_service,
            collection_repo,
            DEFAULT_PROMPT_IDLE_TIMEOUT,
        )
    }

    /// Test seam only. Production wiring always uses `new`, which fixes the
    /// idle limit at the spec's 120-second constant.
    pub fn with_prompt_idle_timeout(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        cleanup: Arc<dyn SessionCleanup>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        prompt_idle_timeout: Duration,
    ) -> Self {
        Self {
            session_client,
            event_publisher,
            cleanup,
            live_sessions: Mutex::new(HashSet::new()),
            shutting_down: AtomicBool::new(false),
            agent_config_service,
            collection_repo,
            prompt_idle_timeout,
        }
    }

    /// Starts owning a started session, so every end path runs its cleanup.
    /// Idempotent. The caller registers the session's resources first and
    /// calls this last, so a sweep never sees a session whose resources are
    /// not yet in place. Returns false when `end_all_sessions` already ran.
    /// The session is then not tracked, and the caller must release its own
    /// resources and end the session.
    pub fn track(&self, session_id: &str) -> bool {
        let mut live = self
            .live_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.shutting_down.load(Ordering::SeqCst) {
            return false;
        }
        live.insert(session_id.to_string());
        true
    }

    /// Forgets a tracked session and runs its cleanup. Only the first caller
    /// for an id finds it tracked, so cleanup runs exactly once per session.
    fn release(&self, session_id: &str) {
        let was_tracked = self
            .live_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session_id);
        if was_tracked {
            self.cleanup.on_session_ended(session_id);
        }
    }

    /// Spawns the configured agent and opens an ACP session in `cwd`.
    ///
    /// `cwd` comes from the caller, not from `AgentConfig::working_dir`, as
    /// the spec requires. Config lookup and credential resolution errors
    /// propagate unchanged. No event is published on failure, because no
    /// session id exists yet. On success, `AcpSessionStarted` is published
    /// and the session info is returned.
    ///
    /// The session is not tracked yet. The command layer is the only caller,
    /// and it must register the session's resources and then call `track`,
    /// so a sweep cannot run between the two.
    ///
    /// `isolation`, when present, starts the agent isolated: its `_meta`
    /// comes from `SessionIsolation::meta`, and `CLAUDE_CONFIG_DIR` points at
    /// the caller's empty scratch directory. `cwd` should then be an empty
    /// scratch directory too.
    ///
    /// `collection` gates whether any MCP servers are attached at all: a
    /// collection that has not opted into agent autonomy gets none —
    /// chat-only mode. This re-checks `get_settings` on every call (not just
    /// once), matching the design spec's "checked on every call" rule for
    /// the tools themselves, and propagates a lookup failure instead of
    /// silently falling back to chat-only mode, so a broken collection name
    /// surfaces loudly rather than silently degrading.
    ///
    /// `mcp_http` carries the port/token of an already-running MCP HTTP
    /// server (see `McpHttpServerCredentials`'s doc comment for why this
    /// crate never spawns that server itself). When autonomy is enabled and
    /// credentials are present, both an `Http` and a `Stdio` spec are
    /// offered to the agent — the `Stdio` spec points back at this same
    /// server through the hidden `--acp-mcp-stdio-bridge` CLI mode, for
    /// agents that only support MCP over stdio. When autonomy is disabled,
    /// or the caller could not spawn the HTTP server (`mcp_http` is `None`),
    /// no MCP servers are attached — the latter fails open to chat-only mode
    /// rather than failing the whole session start over a tool-server hiccup.
    pub async fn start_session(
        &self,
        agent_config_id: &str,
        cwd: &str,
        collection: &str,
        mcp_http: Option<McpHttpServerCredentials>,
        isolation: Option<SessionIsolation>,
    ) -> DomainResult<SessionInfo> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let mut env = vec![(config.credential_env_var.clone(), credential)];
        let meta = isolation.as_ref().map(|isolation| {
            env.push(isolation.env_entry());
            isolation.meta()
        });

        let autonomy_enabled = self
            .collection_repo
            .get_settings(collection)?
            .agent_autonomy_enabled;
        let mcp_servers: Vec<rocket_acp::McpServerSpec> = match (autonomy_enabled, mcp_http) {
            (true, Some(creds)) => mcp_server_specs(creds)?,
            // Autonomy is off, or the caller couldn't spawn the HTTP server
            // (fails open to chat-only mode rather than failing the whole
            // session start over a tool-server hiccup) — no MCP servers.
            _ => Vec::new(),
        };

        let info = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &mcp_servers, meta)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: info.session_id.clone(),
            });
        Ok(info)
    }

    /// Starts a workspace assistant session. Unlike `start_session`, the
    /// tool server is always attached: the workspace assistant's tools
    /// check the workspace scope, the mode and the run switch on every
    /// call. `isolation` adds `CLAUDE_CONFIG_DIR` to the environment and
    /// supplies the `_meta`. Like `start_session`, the session is not
    /// tracked yet: the command layer registers the session's resources
    /// and then calls `track`.
    /// Publishes `AcpSessionStarted` on success, nothing on failure.
    pub async fn start_workspace_session(
        &self,
        agent_config_id: &str,
        cwd: &str,
        mcp_http: McpHttpServerCredentials,
        isolation: SessionIsolation,
    ) -> DomainResult<SessionInfo> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = vec![
            (config.credential_env_var.clone(), credential),
            isolation.env_entry(),
        ];
        let mcp_servers = mcp_server_specs(mcp_http)?;
        let info = self
            .session_client
            .start_session(
                &config.command,
                &config.args,
                cwd,
                &env,
                &mcp_servers,
                Some(isolation.meta()),
            )
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: info.session_id.clone(),
            });
        Ok(info)
    }

    /// Sends one prompt turn and returns the agent's stop reason string.
    ///
    /// Each update is published as it arrives: text as `AcpSessionChunk`, tool
    /// calls as `AcpToolActivity`, options as `AcpConfigOptionsChanged`, usage
    /// as `AcpUsage`. Then exactly one terminal event follows:
    /// `AcpSessionFinished` on success (including the `cancelled` stop reason,
    /// which is a normal finish), or `AcpSessionFailed` on error or idle
    /// timeout. The error is still returned to the caller.
    ///
    /// Ordering: the loop ends only once the client's future has resolved
    /// and the update channel has closed, so every update event is published
    /// before the terminal event. The client must forward a turn's updates
    /// before its `send_prompt` resolves; a later update is dropped, never
    /// reordered.
    ///
    /// Idle timeout: the limit restarts after every update. When it runs out,
    /// the pending prompt is dropped and the session is force-killed via
    /// `end_session`, because a hung agent process is still running.
    ///
    /// A failed prompt also ends the session, unless the error is InvalidInput.
    pub async fn send_prompt(
        &self,
        session_id: &str,
        parts: Vec<PromptPart>,
    ) -> DomainResult<String> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AcpUpdate>();
        let mut send = self.session_client.send_prompt(session_id, parts, tx);
        let mut outcome: Option<DomainResult<String>> = None;
        let mut channel_open = true;
        let mut tool_calls = ToolCallStates::new();

        while outcome.is_none() || channel_open {
            tokio::select! {
                biased;
                update = rx.recv(), if channel_open => match update {
                    Some(update) => self.publish_update(session_id, update, &mut tool_calls),
                    None => channel_open = false,
                },
                result = &mut send, if outcome.is_none() => outcome = Some(result),
                () = tokio::time::sleep(self.prompt_idle_timeout) => {
                    // The client already finished, but another sender clone
                    // keeps the channel open. The turn is done, so finish
                    // with its outcome instead of killing the session.
                    if outcome.is_some() {
                        break;
                    }
                    // Drop the pending prompt first, so the client releases
                    // the turn before the session is killed.
                    drop(send);
                    return Err(self.end_idle_session(session_id).await);
                }
            }
        }

        match outcome {
            Some(Ok(stop_reason)) => {
                self.event_publisher
                    .publish(DomainEvent::AcpSessionFinished {
                        session_id: session_id.to_string(),
                        stop_reason: stop_reason.clone(),
                    });
                Ok(stop_reason)
            }
            Some(Err(e)) => {
                // A prompt the agent could not take (InvalidInput) leaves the
                // session usable. Any other failure means the session is gone
                // or broken, so end it and release its resources.
                if !matches!(e, DomainError::InvalidInput(_)) {
                    let _ = self.session_client.end_session(session_id).await;
                    self.release(session_id);
                }
                self.event_publisher.publish(DomainEvent::AcpSessionFailed {
                    session_id: session_id.to_string(),
                    error: e.to_string(),
                });
                Err(e)
            }
            None => Err(DomainError::Internal(
                "agent prompt ended without a result".to_string(),
            )),
        }
    }

    /// Asks the agent to stop the running turn and returns at once. The
    /// pending `send_prompt` then finishes with the `cancelled` stop reason,
    /// and the session stays open. No event is published here.
    pub async fn cancel(&self, session_id: &str) -> DomainResult<()> {
        self.session_client.cancel(session_id).await
    }

    /// Changes one session option, such as the model, and returns the
    /// agent's new option list. The list is also published as
    /// `AcpConfigOptionsChanged`, because a model change can add or remove
    /// the effort option.
    pub async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: &str,
    ) -> DomainResult<Vec<ConfigOption>> {
        let options = self
            .session_client
            .set_config_option(session_id, config_id, value)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpConfigOptionsChanged {
                session_id: session_id.to_string(),
                options: options.clone(),
            });
        Ok(options)
    }

    /// Publishes one update as its domain event. Tool call updates are merged
    /// with the last known title and status of the same call.
    fn publish_update(&self, session_id: &str, update: AcpUpdate, tool_calls: &mut ToolCallStates) {
        let session_id = session_id.to_string();
        let event = match update {
            AcpUpdate::Text { text } => DomainEvent::AcpSessionChunk { session_id, text },
            AcpUpdate::ToolCall {
                call_id,
                title,
                kind: _,
                status,
            } => {
                tool_calls.insert(call_id.clone(), (title.clone(), status));
                DomainEvent::AcpToolActivity {
                    session_id,
                    call_id,
                    title,
                    status: status.as_str().to_string(),
                }
            }
            AcpUpdate::ToolCallUpdate {
                call_id,
                title,
                status,
            } => {
                let entry = tool_calls
                    .entry(call_id.clone())
                    .or_insert_with(|| (String::new(), ToolCallStatus::Pending));
                if let Some(title) = title {
                    entry.0 = title;
                }
                if let Some(status) = status {
                    entry.1 = status;
                }
                DomainEvent::AcpToolActivity {
                    session_id,
                    call_id,
                    title: entry.0.clone(),
                    status: entry.1.as_str().to_string(),
                }
            }
            AcpUpdate::ConfigOptions { options } => DomainEvent::AcpConfigOptionsChanged {
                session_id,
                options,
            },
            AcpUpdate::Usage {
                used,
                size,
                cost_usd,
            } => DomainEvent::AcpUsage {
                session_id,
                used,
                size,
                cost_usd,
            },
        };
        self.event_publisher.publish(event);
    }

    /// Kills a session whose turn sent nothing for the idle limit, publishes
    /// `AcpSessionFailed`, and returns the error for the caller. The kill
    /// result is ignored on purpose: the timeout is the error the caller must
    /// see, and a session that already crashed changes nothing.
    async fn end_idle_session(&self, session_id: &str) -> DomainError {
        let _ = self.session_client.end_session(session_id).await;
        self.release(session_id);
        let message = format!(
            "agent sent no update for {}s",
            self.prompt_idle_timeout.as_secs()
        );
        self.event_publisher.publish(DomainEvent::AcpSessionFailed {
            session_id: session_id.to_string(),
            error: message.clone(),
        });
        DomainError::Internal(message)
    }

    /// Kills the agent of a session that `track` refused, and runs its
    /// cleanup once. For an id that was never tracked. No event is published.
    pub async fn end_untracked(&self, session_id: &str) -> DomainResult<()> {
        let result = self.session_client.end_session(session_id).await;
        self.cleanup.on_session_ended(session_id);
        result
    }

    /// Ends the session and kills its agent process, then releases its
    /// resources once. No event is published. An unknown or already-ended
    /// session id returns the client's error and runs no cleanup.
    pub async fn end_session(&self, session_id: &str) -> DomainResult<()> {
        let result = self.session_client.end_session(session_id).await;
        self.release(session_id);
        result
    }

    /// Kills every agent session's process for app exit, then releases each
    /// tracked session's resources. The client refuses new sessions
    /// afterwards. No event is published.
    pub async fn end_all_sessions(&self) -> DomainResult<()> {
        let ended: Vec<String> = {
            let mut live = self
                .live_sessions
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            self.shutting_down.store(true, Ordering::SeqCst);
            live.drain().collect()
        };
        let result = self.session_client.end_all_sessions().await;
        for session_id in &ended {
            self.cleanup.on_session_ended(session_id);
        }
        result
    }

    /// Ends every session this service still tracks, one at a time, and
    /// returns how many it ended. Each one runs its cleanup once. Unlike
    /// `end_all_sessions`, the client keeps accepting new sessions, so the
    /// webview can start a fresh one right after this sweep.
    pub async fn end_tracked_sessions(&self) -> usize {
        let tracked: Vec<String> = self
            .live_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect();
        for session_id in &tracked {
            let _ = self.session_client.end_session(session_id).await;
            self.release(session_id);
        }
        tracked.len()
    }
}

/// The two MCP server specs offered for one session's tool server: `Http`
/// for agents that speak MCP over HTTP, and `Stdio`, which points back at
/// the same server through the hidden `--acp-mcp-stdio-bridge` mode. The
/// token travels only in the stdio spec's environment, never in argv.
fn mcp_server_specs(
    creds: McpHttpServerCredentials,
) -> DomainResult<Vec<rocket_acp::McpServerSpec>> {
    let exe = std::env::current_exe()
        .map_err(|e| DomainError::Internal(format!("could not resolve current executable: {e}")))?;
    Ok(vec![
        rocket_acp::McpServerSpec::Http {
            name: ROCKET_MCP_SERVER_NAME.to_string(),
            // Must match `src_tauri::mcp::tool_server::MCP_HTTP_PATH`
            // ("/mcp"); this crate cannot import it, because `rocket-app`
            // never depends on `src-tauri`.
            url: format!("http://127.0.0.1:{}/mcp", creds.port),
            token: creds.token.clone(),
        },
        rocket_acp::McpServerSpec::Stdio {
            name: ROCKET_MCP_SERVER_NAME.to_string(),
            command: exe.to_string_lossy().into_owned(),
            args: vec!["--acp-mcp-stdio-bridge".to_string()],
            env: vec![
                ("ROCKET_MCP_PORT".to_string(), creds.port.to_string()),
                ("ROCKET_MCP_TOKEN".to_string(), creds.token),
            ],
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use rocket_acp::{AgentConfig, AgentConfigRepository};
    use rocket_acp::{ConfigChoice, ConfigOption, PromptCapabilities, SessionInfo, ToolCallStatus};
    use rocket_environment::external_secret::ExternalSecretRef;
    use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
    use rocket_environment::secret_store::SecretStore;
    use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
    use rocket_shared::events::NullEventPublisher;
    use tokio::sync::mpsc::UnboundedSender;

    use crate::agent_isolation::{
        isolation_meta, SessionIsolation, ROCKET_ASSISTANT_SYSTEM_PROMPT,
    };
    use crate::test_doubles::ConfigurableCollectionRepo;

    struct FakeAgentConfigRepo(Mutex<Vec<AgentConfig>>);
    impl AgentConfigRepository for FakeAgentConfigRepo {
        fn list(&self) -> DomainResult<Vec<AgentConfig>> {
            Ok(self.0.lock().expect("lock FakeAgentConfigRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeAgentConfigRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, config: &AgentConfig) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeAgentConfigRepo");
            guard.retain(|c| c.id != config.id);
            guard.push(config.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeAgentConfigRepo")
                .retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretManagerRepo(Mutex<Vec<SecretManagerConnection>>);
    impl SecretManagerRepository for FakeSecretManagerRepo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            Ok(self.0.lock().expect("lock FakeSecretManagerRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeSecretManagerRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeSecretManagerRepo");
            guard.retain(|c| c.id != connection.id);
            guard.push(connection.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeSecretManagerRepo")
                .retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretStore;
    impl SecretStore for FakeSecretStore {
        fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
            Ok(Some("shh-its-a-secret".to_string()))
        }
        fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    /// `secret_value_result: Ok(None)` simulates a stale/deleted vault
    /// secret (the credential-resolution-failure case this plan's Review
    /// Focus requires a test for) without needing a second fake type.
    struct FakeVaultFetcher {
        secret_value_result: DomainResult<Option<String>>,
    }
    impl Default for FakeVaultFetcher {
        fn default() -> Self {
            Self {
                secret_value_result: Ok(Some("sk-abc123".to_string())),
            }
        }
    }
    #[async_trait::async_trait]
    impl VaultSecretFetcher for FakeVaultFetcher {
        async fn list_secrets(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(Vec::new())
        }
        async fn get_secret_value(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            _secret_id: &str,
        ) -> DomainResult<Option<String>> {
            match &self.secret_value_result {
                Ok(v) => Ok(v.clone()),
                Err(DomainError::Internal(msg)) => Err(DomainError::Internal(msg.clone())),
                Err(_) => Err(DomainError::Internal("fake fetcher error".to_string())),
            }
        }
        async fn test_connection(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    fn sample_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: Default::default(),
            config: None,
        }
    }

    fn sample_config() -> AgentConfig {
        AgentConfig {
            id: "agent-1".to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: Vec::new(),
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "secret-id-1".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    fn agent_config_service() -> Arc<AgentConfigService> {
        agent_config_service_with(FakeVaultFetcher::default())
    }

    fn agent_config_service_with(fetcher: FakeVaultFetcher) -> Arc<AgentConfigService> {
        let sm_repo = FakeSecretManagerRepo(Mutex::new(vec![sample_connection()]));
        let secret_manager = Arc::new(crate::secret_manager_service::SecretManagerService::new(
            Box::new(sm_repo),
            Arc::new(FakeSecretStore),
            Arc::new(fetcher),
        ));
        let repo = FakeAgentConfigRepo(Mutex::new(vec![sample_config()]));
        Arc::new(AgentConfigService::new(Box::new(repo), secret_manager))
    }

    struct FakeSessionClient {
        start_should_fail: bool,
        start_config_options: Vec<ConfigOption>,
        prompt_updates: Vec<AcpUpdate>,
        update_interval: Duration,
        prompt_stop_reason: String,
        prompt_should_fail: bool,
        prompt_delay: Duration,
        end_session_called: Arc<AtomicBool>,
        end_all_sessions_called: Arc<AtomicBool>,
        cancel_called: Arc<AtomicBool>,
        options_after_set: Vec<ConfigOption>,
        prompt_invalid_input: bool,
        start_ids: Arc<Mutex<VecDeque<String>>>,
        /// When set, each prompt parks a clone of its update sender here.
        held_update_tx: Option<Arc<Mutex<Vec<UnboundedSender<AcpUpdate>>>>>,
    }
    impl Default for FakeSessionClient {
        fn default() -> Self {
            Self {
                start_should_fail: false,
                start_config_options: Vec::new(),
                prompt_updates: vec![AcpUpdate::Text {
                    text: "hello".to_string(),
                }],
                update_interval: Duration::ZERO,
                prompt_stop_reason: "end_turn".to_string(),
                prompt_should_fail: false,
                prompt_delay: Duration::ZERO,
                end_session_called: Arc::new(AtomicBool::new(false)),
                end_all_sessions_called: Arc::new(AtomicBool::new(false)),
                cancel_called: Arc::new(AtomicBool::new(false)),
                options_after_set: Vec::new(),
                prompt_invalid_input: false,
                start_ids: Arc::new(Mutex::new(VecDeque::new())),
                held_update_tx: None,
            }
        }
    }

    impl FakeSessionClient {
        /// Returns the next queued session id, or "session-1" when none is queued.
        fn next_session_id(&self) -> String {
            self.start_ids
                .lock()
                .expect("lock start_ids")
                .pop_front()
                .unwrap_or_else(|| "session-1".to_string())
        }
    }
    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            _mcp_servers: &[rocket_acp::McpServerSpec],
            _meta: Option<serde_json::Value>,
        ) -> DomainResult<SessionInfo> {
            if self.start_should_fail {
                Err(DomainError::InvalidInput("command not found".to_string()))
            } else {
                Ok(SessionInfo {
                    session_id: self.next_session_id(),
                    config_options: self.start_config_options.clone(),
                    prompt_capabilities: PromptCapabilities {
                        embedded_context: true,
                        image: false,
                    },
                })
            }
        }
        async fn send_prompt(
            &self,
            _session_id: &str,
            _parts: Vec<PromptPart>,
            update_tx: UnboundedSender<AcpUpdate>,
        ) -> DomainResult<String> {
            if self.prompt_invalid_input {
                return Err(DomainError::InvalidInput(
                    "unsupported prompt part".to_string(),
                ));
            }
            if let Some(held) = &self.held_update_tx {
                held.lock()
                    .expect("lock held_update_tx")
                    .push(update_tx.clone());
            }
            tokio::time::sleep(self.prompt_delay).await;
            for update in &self.prompt_updates {
                tokio::time::sleep(self.update_interval).await;
                let _ = update_tx.send(update.clone());
            }
            if self.prompt_should_fail {
                Err(DomainError::Internal("agent crashed".to_string()))
            } else {
                Ok(self.prompt_stop_reason.clone())
            }
        }
        async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
            self.cancel_called.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn set_config_option(
            &self,
            _session_id: &str,
            _config_id: &str,
            _value: &str,
        ) -> DomainResult<Vec<ConfigOption>> {
            Ok(self.options_after_set.clone())
        }
        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            self.end_session_called.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn end_all_sessions(&self) -> DomainResult<()> {
            self.end_all_sessions_called.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FakeEventPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }
    impl FakeEventPublisher {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }
    impl EventPublisher for FakeEventPublisher {
        fn publish(&self, event: DomainEvent) {
            self.events
                .lock()
                .expect("lock FakeEventPublisher")
                .push(event);
        }
    }

    /// `Arc<FakeEventPublisher>` can't implement the foreign `EventPublisher`
    /// trait directly (orphan rule — neither `Arc` nor `EventPublisher` is
    /// local to this crate). This local newtype wrapper, delegating to the
    /// shared inner publisher, is the same pattern already used by
    /// `collection_service.rs`'s `SharedEventPublisher`.
    struct SharedEventPublisher(Arc<FakeEventPublisher>);
    impl EventPublisher for SharedEventPublisher {
        fn publish(&self, event: DomainEvent) {
            self.0.publish(event);
        }
    }

    #[derive(Default)]
    struct RecordingCleanup {
        ended: Mutex<Vec<String>>,
    }
    impl RecordingCleanup {
        fn ended(&self) -> Vec<String> {
            self.ended.lock().expect("lock RecordingCleanup").clone()
        }
    }
    impl SessionCleanup for RecordingCleanup {
        fn on_session_ended(&self, session_id: &str) {
            self.ended
                .lock()
                .expect("lock RecordingCleanup")
                .push(session_id.to_string());
        }
    }

    fn noop_cleanup() -> Arc<dyn SessionCleanup> {
        Arc::new(NoopSessionCleanup)
    }

    fn service_with_cleanup(
        client: FakeSessionClient,
        cleanup: Arc<dyn SessionCleanup>,
    ) -> AcpSessionService {
        AcpSessionService::new(
            Box::new(client),
            Box::new(NullEventPublisher),
            cleanup,
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        )
    }

    /// Starts a session and tracks it, as the command layer does once its
    /// resources are registered.
    async fn start(service: &AcpSessionService) {
        let info = service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");
        assert!(service.track(&info.session_id));
    }

    /// Sends one text prompt. Keep the argument form identical to the
    /// existing send_prompt tests in this module.
    async fn send_hi(service: &AcpSessionService, session_id: &str) -> DomainResult<String> {
        service.send_prompt(session_id, hi()).await
    }

    #[tokio::test]
    async fn start_session_resolves_config_and_credential_and_publishes_started() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let session_id = service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");
        assert_eq!(session_id.session_id, "session-1");

        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DomainEvent::AcpSessionStarted { .. }));
    }

    #[tokio::test]
    async fn send_prompt_publishes_every_chunk_before_finished_in_order() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_updates: vec![
                AcpUpdate::Text {
                    text: "Hello, ".to_string(),
                },
                AcpUpdate::Text {
                    text: "world!".to_string(),
                },
            ],
            ..Default::default()
        };
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let stop_reason = service
            .send_prompt("session-1", hi())
            .await
            .expect("send_prompt should succeed");
        assert_eq!(stop_reason, "end_turn");

        let events = publisher.events.lock().expect("lock");
        assert_eq!(
            events.len(),
            3,
            "expected 2 chunks then 1 finished, got {events:?}"
        );
        match (&events[0], &events[1], &events[2]) {
            (
                DomainEvent::AcpSessionChunk { text: t0, .. },
                DomainEvent::AcpSessionChunk { text: t1, .. },
                DomainEvent::AcpSessionFinished { stop_reason, .. },
            ) => {
                assert_eq!(t0, "Hello, ");
                assert_eq!(t1, "world!");
                assert_eq!(stop_reason, "end_turn");
            }
            other => panic!("expected [Chunk, Chunk, Finished] in order, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn end_session_delegates_to_session_client() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        service
            .end_session("session-1")
            .await
            .expect("end_session should succeed");
        assert!(end_session_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn start_session_unknown_agent_config_id_errors() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let err = service
            .start_session("no-such-agent", "/tmp", "demo", None, None)
            .await
            .expect_err("unknown agent_config_id must error");
        assert!(matches!(err, DomainError::NotFound(_)));
        assert!(
            publisher.events.lock().expect("lock").is_empty(),
            "no event should publish when config resolution fails before any session starts"
        );
    }

    #[tokio::test]
    async fn start_session_propagates_spawn_failure() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            start_should_fail: true,
            ..Default::default()
        };
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let err = service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect_err("spawn failure must propagate");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn start_session_propagates_credential_resolution_failure_unchanged() {
        // Ok(None) simulates a stale/deleted vault secret — AgentConfigService
        // maps this to NotFound (proven by its own tests in subproject A);
        // this test's job is narrower: prove AcpSessionService::start_session
        // doesn't re-wrap or swallow that result on its way through.
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service_with(FakeVaultFetcher {
                secret_value_result: Ok(None),
            }),
            ConfigurableCollectionRepo::new(),
        );

        let err = service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect_err("a stale vault secret must fail start_session, not silently proceed");
        assert!(matches!(err, DomainError::NotFound(_)));
        assert!(
            publisher.events.lock().expect("lock").is_empty(),
            "no event should publish when credential resolution fails before any session starts"
        );
    }

    #[tokio::test]
    async fn send_prompt_failure_publishes_failed_and_returns_the_error() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_should_fail: true,
            ..Default::default()
        };
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let err = service
            .send_prompt("session-1", hi())
            .await
            .expect_err("a crashed/errored prompt must return an error");
        assert!(matches!(err, DomainError::Internal(_)));

        let events = publisher.events.lock().expect("lock");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })),
            "AcpSessionFailed must be published, got {events:?}"
        );
    }

    #[tokio::test]
    async fn send_prompt_timeout_kills_the_session_and_publishes_failed() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_delay: Duration::from_millis(200),
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = AcpSessionService::with_prompt_idle_timeout(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
            Duration::from_millis(20),
        );

        let err = service
            .send_prompt("session-1", hi())
            .await
            .expect_err("a hung prompt must time out as an error");
        assert!(matches!(err, DomainError::Internal(_)));
        assert!(
            end_session_called.load(Ordering::SeqCst),
            "timeout must force-kill the session via end_session"
        );

        let events = publisher.events.lock().expect("lock");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })),
            "AcpSessionFailed must be published on timeout, got {events:?}"
        );
    }

    #[tokio::test]
    async fn end_all_sessions_delegates_to_session_client() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_all_sessions_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            end_all_sessions_called: Arc::clone(&end_all_sessions_called),
            ..Default::default()
        };
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        service
            .end_all_sessions()
            .await
            .expect("end_all_sessions should succeed");
        assert!(end_all_sessions_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn start_session_with_default_unconfigured_settings_still_works() {
        // A collection that was never explicitly configured (no
        // `set_autonomy` call) falls back to `CollectionSettings::default()`
        // (autonomy off), matching the real repos' "missing settings file"
        // behavior — this must not error.
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let session_id = service
            .start_session("agent-1", "/tmp", "unconfigured-collection", None, None)
            .await
            .expect("an unconfigured collection must still start a chat-only session");
        assert_eq!(session_id.session_id, "session-1");
    }

    #[tokio::test]
    async fn start_session_with_autonomy_disabled_ignores_provided_mcp_http_credentials() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>> =
            Arc::new(Mutex::new(Vec::new()));
        let client = CapturingSessionClient {
            captured_servers: Arc::clone(&captured_servers),
            ..Default::default()
        };
        let collection_repo = ConfigurableCollectionRepo::new();
        collection_repo.set_autonomy("my-api", false);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            collection_repo,
        );

        let session_id = service
            .start_session(
                "agent-1",
                "/tmp",
                "my-api",
                Some(McpHttpServerCredentials {
                    port: 1234,
                    token: "unused-token".to_string(),
                }),
                None,
            )
            .await
            .expect("a disabled collection must still be able to start a chat-only session");
        assert_eq!(session_id.session_id, "session-1");
        assert!(
            captured_servers.lock().expect("lock").is_empty(),
            "a disabled collection must get no MCP servers even when credentials were provided"
        );
    }

    #[tokio::test]
    async fn start_session_with_autonomy_enabled_but_no_mcp_http_credentials_fails_open_to_chat_only(
    ) {
        // The HTTP tool server could not be spawned (or autonomy was enabled
        // after the caller already decided not to try) — this must not fail
        // the whole session start, only skip attaching any MCP servers.
        let publisher = Arc::new(FakeEventPublisher::new());
        let captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>> =
            Arc::new(Mutex::new(Vec::new()));
        let client = CapturingSessionClient {
            captured_servers: Arc::clone(&captured_servers),
            ..Default::default()
        };
        let collection_repo = ConfigurableCollectionRepo::with_autonomy_enabled("my-api", true);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            collection_repo,
        );

        let session_id = service
            .start_session("agent-1", "/tmp", "my-api", None, None)
            .await
            .expect("a missing MCP HTTP server must fail open to a chat-only session");
        assert_eq!(session_id.session_id, "session-1");
        assert!(
            captured_servers.lock().expect("lock").is_empty(),
            "no MCP servers should be attached without credentials, even with autonomy enabled"
        );
    }

    #[tokio::test]
    async fn start_session_propagates_a_collection_settings_lookup_failure() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let collection_repo = ConfigurableCollectionRepo::new();
        collection_repo.fail_settings_for("broken-collection");
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            collection_repo,
        );

        let err = service
            .start_session("agent-1", "/tmp", "broken-collection", None, None)
            .await
            .expect_err(
                "a broken collection settings read must fail start_session, not silently degrade to chat-only",
            );
        assert!(matches!(err, DomainError::Internal(_)));
        assert!(
            publisher.events.lock().expect("lock").is_empty(),
            "no event should publish when the settings lookup fails before any session starts"
        );
    }

    #[derive(Default)]
    struct CapturingSessionClient {
        captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>>,
        captured_env: Arc<Mutex<Vec<(String, String)>>>,
        captured_meta: Arc<Mutex<Option<serde_json::Value>>>,
    }
    #[async_trait::async_trait]
    impl AcpSessionClient for CapturingSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            env: &[(String, String)],
            mcp_servers: &[rocket_acp::McpServerSpec],
            meta: Option<serde_json::Value>,
        ) -> DomainResult<SessionInfo> {
            *self.captured_env.lock().expect("lock") = env.to_vec();
            *self.captured_meta.lock().expect("lock") = meta;
            *self.captured_servers.lock().expect("lock") = mcp_servers.to_vec();
            Ok(SessionInfo {
                session_id: "session-1".to_string(),
                config_options: Vec::new(),
                prompt_capabilities: PromptCapabilities::default(),
            })
        }
        async fn send_prompt(
            &self,
            _session_id: &str,
            _parts: Vec<PromptPart>,
            _update_tx: UnboundedSender<AcpUpdate>,
        ) -> DomainResult<String> {
            unreachable!("not exercised by this test")
        }
        async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
            unreachable!("not exercised by this test")
        }
        async fn set_config_option(
            &self,
            _session_id: &str,
            _config_id: &str,
            _value: &str,
        ) -> DomainResult<Vec<ConfigOption>> {
            unreachable!("not exercised by this test")
        }
        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            unreachable!("not exercised by this test")
        }
        async fn end_all_sessions(&self) -> DomainResult<()> {
            unreachable!("not exercised by this test")
        }
    }

    #[tokio::test]
    async fn start_session_with_autonomy_enabled_builds_http_and_stdio_specs_with_token_only_in_env(
    ) {
        let publisher = Arc::new(FakeEventPublisher::new());
        let captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>> =
            Arc::new(Mutex::new(Vec::new()));
        let client = CapturingSessionClient {
            captured_servers: Arc::clone(&captured_servers),
            ..Default::default()
        };
        let collection_repo = ConfigurableCollectionRepo::with_autonomy_enabled("demo", true);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            collection_repo,
        );

        service
            .start_session(
                "agent-1",
                "/tmp",
                "demo",
                Some(McpHttpServerCredentials {
                    port: 54321,
                    token: "s3cr3t-token".to_string(),
                }),
                None,
            )
            .await
            .expect("start_session should succeed");

        let servers = captured_servers.lock().expect("lock").clone();
        assert_eq!(
            servers.len(),
            2,
            "expected one Http and one Stdio spec, got {servers:?}"
        );
        for server in &servers {
            if let rocket_acp::McpServerSpec::Stdio { args, env, .. } = server {
                assert!(
                    !args.iter().any(|a| a.contains("s3cr3t-token")),
                    "the token must never appear in argv, got args {args:?}"
                );
                assert!(
                    env.iter()
                        .any(|(k, v)| k == "ROCKET_MCP_TOKEN" && v == "s3cr3t-token"),
                    "the token must be passed via the ROCKET_MCP_TOKEN env var, got {env:?}"
                );
            }
        }
    }

    fn hi() -> Vec<PromptPart> {
        vec![PromptPart::Text("hi".to_string())]
    }

    fn sample_option(id: &str, current: &str) -> ConfigOption {
        ConfigOption {
            id: id.to_string(),
            name: id.to_string(),
            category: None,
            current_value: current.to_string(),
            choices: vec![ConfigChoice {
                value: current.to_string(),
                name: current.to_string(),
                description: None,
            }],
        }
    }

    fn service_with(
        client: FakeSessionClient,
        publisher: &Arc<FakeEventPublisher>,
        idle: Duration,
    ) -> AcpSessionService {
        AcpSessionService::with_prompt_idle_timeout(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
            idle,
        )
    }

    #[tokio::test]
    async fn start_session_returns_the_client_session_info() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            start_config_options: vec![sample_option("model", "default")],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        let info = service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");
        assert_eq!(info.session_id, "session-1");
        assert_eq!(info.config_options, vec![sample_option("model", "default")]);
        assert!(info.prompt_capabilities.embedded_context);
    }

    #[tokio::test]
    async fn send_prompt_publishes_every_update_kind_in_order_before_finished() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_updates: vec![
                AcpUpdate::Text {
                    text: "Reading".to_string(),
                },
                AcpUpdate::ToolCall {
                    call_id: "call-1".to_string(),
                    title: "Read GET /orders".to_string(),
                    kind: "read".to_string(),
                    status: ToolCallStatus::Pending,
                },
                AcpUpdate::ToolCallUpdate {
                    call_id: "call-1".to_string(),
                    title: None,
                    status: Some(ToolCallStatus::Completed),
                },
                AcpUpdate::ConfigOptions {
                    options: vec![sample_option("model", "opus")],
                },
                AcpUpdate::Usage {
                    used: 1_200,
                    size: 200_000,
                    cost_usd: Some(0.01),
                },
            ],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        let stop_reason = service
            .send_prompt("session-1", hi())
            .await
            .expect("send_prompt should succeed");
        assert_eq!(stop_reason, "end_turn");

        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 6, "got {events:?}");
        assert!(
            matches!(&events[0], DomainEvent::AcpSessionChunk { text, .. } if text == "Reading")
        );
        assert!(matches!(
            &events[1],
            DomainEvent::AcpToolActivity { call_id, title, status, .. }
                if call_id == "call-1" && title == "Read GET /orders" && status == "pending"
        ));
        assert!(
            matches!(
                &events[2],
                DomainEvent::AcpToolActivity { title, status, .. }
                    if title == "Read GET /orders" && status == "completed"
            ),
            "an update without a title keeps the last known title, got {:?}",
            events[2]
        );
        assert!(matches!(
            &events[3],
            DomainEvent::AcpConfigOptionsChanged { options, .. }
                if options.len() == 1 && options[0].current_value == "opus"
        ));
        assert!(matches!(
            &events[4],
            DomainEvent::AcpUsage {
                used: 1_200,
                size: 200_000,
                cost_usd: Some(_),
                ..
            }
        ));
        assert!(matches!(
            &events[5],
            DomainEvent::AcpSessionFinished { stop_reason, .. } if stop_reason == "end_turn"
        ));
    }

    #[tokio::test]
    async fn tool_call_update_for_an_unknown_call_publishes_an_empty_title_and_pending() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_updates: vec![AcpUpdate::ToolCallUpdate {
                call_id: "call-9".to_string(),
                title: None,
                status: None,
            }],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        service
            .send_prompt("session-1", hi())
            .await
            .expect("send_prompt should succeed");

        let events = publisher.events.lock().expect("lock");
        assert!(
            matches!(
                &events[0],
                DomainEvent::AcpToolActivity { call_id, title, status, .. }
                    if call_id == "call-9" && title.is_empty() && status == "pending"
            ),
            "got {events:?}"
        );
    }

    #[tokio::test]
    async fn idle_timeout_restarts_on_every_update() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_updates: (0..5)
                .map(|i| AcpUpdate::Text {
                    text: format!("chunk {i}"),
                })
                .collect(),
            update_interval: Duration::from_millis(40),
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        // Five updates 40 ms apart take about 200 ms, longer than the 150 ms
        // idle limit, but no single gap reaches it.
        let service = service_with(client, &publisher, Duration::from_millis(150));

        let stop_reason = service
            .send_prompt("session-1", hi())
            .await
            .expect("a turn that keeps sending updates must not time out");
        assert_eq!(stop_reason, "end_turn");
        assert!(!end_session_called.load(Ordering::SeqCst));
        let events = publisher.events.lock().expect("lock");
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })));
    }

    #[tokio::test]
    async fn a_lingering_sender_clone_after_the_client_finished_does_not_kill_the_session() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let held = Arc::new(Mutex::new(Vec::new()));
        let cleanup = Arc::new(RecordingCleanup::default());
        let client = FakeSessionClient {
            end_session_called: Arc::clone(&end_session_called),
            held_update_tx: Some(Arc::clone(&held)),
            ..Default::default()
        };
        let service = AcpSessionService::with_prompt_idle_timeout(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            cleanup.clone(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
            Duration::from_millis(50),
        );
        start(&service).await;

        let stop_reason = send_hi(&service, "session-1")
            .await
            .expect("a finished turn must succeed despite the open channel");
        assert_eq!(stop_reason, "end_turn");
        assert_eq!(held.lock().expect("lock held").len(), 1);
        assert!(!end_session_called.load(Ordering::SeqCst));
        assert!(cleanup.ended().is_empty());

        let events = publisher.events.lock().expect("lock");
        assert!(events
            .iter()
            .any(|e| matches!(e, DomainEvent::AcpSessionFinished { .. })));
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })));
    }

    #[tokio::test]
    async fn cancelled_stop_reason_is_a_normal_finish_and_keeps_the_session() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_stop_reason: "cancelled".to_string(),
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        let stop_reason = service
            .send_prompt("session-1", hi())
            .await
            .expect("a cancelled turn is a normal finish");
        assert_eq!(stop_reason, "cancelled");
        assert!(!end_session_called.load(Ordering::SeqCst));

        let events = publisher.events.lock().expect("lock");
        assert!(matches!(
            events.last(),
            Some(DomainEvent::AcpSessionFinished { stop_reason, .. }) if stop_reason == "cancelled"
        ));
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })));
    }

    #[tokio::test]
    async fn cancel_delegates_to_the_session_client_without_events() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let cancel_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            cancel_called: Arc::clone(&cancel_called),
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        service
            .cancel("session-1")
            .await
            .expect("cancel should succeed");
        assert!(cancel_called.load(Ordering::SeqCst));
        assert!(publisher.events.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn set_config_option_returns_the_new_options_and_publishes_them() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            options_after_set: vec![
                sample_option("model", "opus"),
                sample_option("effort", "high"),
            ],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        let options = service
            .set_config_option("session-1", "model", "opus")
            .await
            .expect("set_config_option should succeed");
        assert_eq!(options.len(), 2);

        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            DomainEvent::AcpConfigOptionsChanged { session_id, options }
                if session_id == "session-1" && options.len() == 2
        ));
    }

    #[tokio::test]
    async fn start_session_with_isolation_passes_meta_and_the_config_dir_env() {
        let client = CapturingSessionClient::default();
        let captured_env = Arc::clone(&client.captured_env);
        let captured_meta = Arc::clone(&client.captured_meta);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(NullEventPublisher),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        service
            .start_session(
                "agent-1",
                "/scratch/cwd",
                "demo",
                None,
                Some(SessionIsolation::new("/scratch/config")),
            )
            .await
            .expect("start_session should succeed");

        let env = captured_env.lock().expect("lock").clone();
        assert!(
            env.iter()
                .any(|(k, v)| k == "CLAUDE_CONFIG_DIR" && v == "/scratch/config"),
            "CLAUDE_CONFIG_DIR must point at the scratch config dir, got {env:?}"
        );
        assert!(
            env.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"),
            "the credential env var must still be passed, got {env:?}"
        );
        assert_eq!(
            *captured_meta.lock().expect("lock"),
            Some(isolation_meta(ROCKET_ASSISTANT_SYSTEM_PROMPT))
        );
    }

    #[tokio::test]
    async fn start_session_without_isolation_passes_no_meta_and_no_config_dir() {
        let client = CapturingSessionClient::default();
        let captured_env = Arc::clone(&client.captured_env);
        let captured_meta = Arc::clone(&client.captured_meta);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(NullEventPublisher),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");

        assert!(captured_meta.lock().expect("lock").is_none());
        assert!(!captured_env
            .lock()
            .expect("lock")
            .iter()
            .any(|(k, _)| k == "CLAUDE_CONFIG_DIR"));
    }

    #[tokio::test]
    async fn end_session_runs_cleanup_once_for_a_started_session() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let service = service_with_cleanup(FakeSessionClient::default(), cleanup.clone());
        start(&service).await;

        service.end_session("session-1").await.expect("first end");
        let _ = service.end_session("session-1").await;

        assert_eq!(cleanup.ended(), vec!["session-1".to_string()]);
    }

    #[tokio::test]
    async fn end_session_on_an_untracked_id_still_calls_the_client_but_runs_no_cleanup() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());

        let _ = service.end_session("never-started").await;

        assert!(end_session_called.load(Ordering::SeqCst));
        assert!(cleanup.ended().is_empty());
    }

    #[tokio::test]
    async fn idle_timeout_runs_cleanup_once() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let client = FakeSessionClient {
            prompt_delay: Duration::from_millis(200),
            ..Default::default()
        };
        let service = AcpSessionService::with_prompt_idle_timeout(
            Box::new(client),
            Box::new(NullEventPublisher),
            cleanup.clone(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
            Duration::from_millis(20),
        );
        start(&service).await;

        send_hi(&service, "session-1")
            .await
            .expect_err("a hung prompt must time out");
        let _ = service.end_session("session-1").await;

        assert_eq!(cleanup.ended(), vec!["session-1".to_string()]);
    }

    #[tokio::test]
    async fn failed_prompt_runs_cleanup_once_and_kills_the_session() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_should_fail: true,
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        start(&service).await;

        send_hi(&service, "session-1")
            .await
            .expect_err("a crashed prompt must fail");
        let _ = send_hi(&service, "session-1").await;
        let _ = service.end_session("session-1").await;

        assert!(end_session_called.load(Ordering::SeqCst));
        assert_eq!(cleanup.ended(), vec!["session-1".to_string()]);
    }

    #[tokio::test]
    async fn invalid_input_prompt_error_keeps_the_session_and_runs_no_cleanup() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_invalid_input: true,
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        start(&service).await;

        let err = send_hi(&service, "session-1")
            .await
            .expect_err("an invalid prompt must fail");
        assert!(matches!(err, DomainError::InvalidInput(_)));

        assert!(!end_session_called.load(Ordering::SeqCst));
        assert!(cleanup.ended().is_empty());
    }

    #[tokio::test]
    async fn end_all_sessions_runs_cleanup_for_every_tracked_session() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let client = FakeSessionClient {
            start_ids: Arc::new(Mutex::new(VecDeque::from(vec![
                "s-a".to_string(),
                "s-b".to_string(),
            ]))),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        start(&service).await;
        start(&service).await;

        service.end_all_sessions().await.expect("end_all_sessions");
        let _ = service.end_session("s-a").await;

        let mut ended = cleanup.ended();
        ended.sort();
        assert_eq!(ended, vec!["s-a".to_string(), "s-b".to_string()]);
    }

    #[tokio::test]
    async fn a_failed_start_tracks_nothing() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let client = FakeSessionClient {
            start_should_fail: true,
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());

        service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect_err("spawn failure must propagate");
        service.end_all_sessions().await.expect("end_all_sessions");

        assert!(cleanup.ended().is_empty());
    }

    #[tokio::test]
    async fn end_tracked_sessions_ends_every_tracked_session_without_shutting_the_client_down() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let end_all_sessions_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            start_ids: Arc::new(Mutex::new(VecDeque::from(vec![
                "s-a".to_string(),
                "s-b".to_string(),
            ]))),
            end_session_called: Arc::clone(&end_session_called),
            end_all_sessions_called: Arc::clone(&end_all_sessions_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        start(&service).await;
        start(&service).await;

        assert_eq!(service.end_tracked_sessions().await, 2);

        let mut ended = cleanup.ended();
        ended.sort();
        assert_eq!(ended, vec!["s-a".to_string(), "s-b".to_string()]);
        assert!(end_session_called.load(Ordering::SeqCst));
        assert!(
            !end_all_sessions_called.load(Ordering::SeqCst),
            "end_all_sessions would make the client refuse every later session"
        );

        start(&service).await;
        assert_eq!(service.end_tracked_sessions().await, 1);
    }

    #[tokio::test]
    async fn end_tracked_sessions_with_nothing_tracked_ends_nothing() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());

        assert_eq!(service.end_tracked_sessions().await, 0);
        assert!(!end_session_called.load(Ordering::SeqCst));
        assert!(cleanup.ended().is_empty());
    }

    #[tokio::test]
    async fn a_sweep_between_start_session_and_track_neither_ends_nor_leaks_the_session() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let service = service_with_cleanup(FakeSessionClient::default(), cleanup.clone());
        let info = service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");

        // The command has not registered its resources yet, so the sweep
        // must not see the session.
        assert_eq!(service.end_tracked_sessions().await, 0);
        assert!(cleanup.ended().is_empty());

        assert!(service.track(&info.session_id));
        assert!(service.track(&info.session_id), "track is idempotent");
        assert_eq!(service.end_tracked_sessions().await, 1);
        assert_eq!(cleanup.ended(), vec!["session-1".to_string()]);
    }

    #[tokio::test]
    async fn track_after_end_all_sessions_is_refused_and_runs_no_cleanup() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let service = service_with_cleanup(FakeSessionClient::default(), cleanup.clone());
        let info = service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");

        service.end_all_sessions().await.expect("end_all_sessions");

        assert!(!service.track(&info.session_id));
        assert_eq!(service.end_tracked_sessions().await, 0);
        assert!(cleanup.ended().is_empty());
    }

    #[tokio::test]
    async fn end_untracked_kills_the_agent_and_runs_cleanup_once() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        let info = service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");
        service.end_all_sessions().await.expect("end_all_sessions");
        assert!(!service.track(&info.session_id));
        end_session_called.store(false, Ordering::SeqCst);

        service
            .end_untracked(&info.session_id)
            .await
            .expect("end_untracked should succeed");

        assert!(end_session_called.load(Ordering::SeqCst));
        assert_eq!(cleanup.ended(), vec!["session-1".to_string()]);
    }

    #[derive(Default)]
    struct WorkspaceStartCapture {
        env: Vec<(String, String)>,
        servers: Vec<rocket_acp::McpServerSpec>,
        meta: Option<serde_json::Value>,
    }

    struct WorkspaceCapturingClient {
        capture: Arc<Mutex<WorkspaceStartCapture>>,
    }

    #[async_trait::async_trait]
    impl AcpSessionClient for WorkspaceCapturingClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            env: &[(String, String)],
            mcp_servers: &[rocket_acp::McpServerSpec],
            meta: Option<serde_json::Value>,
        ) -> DomainResult<rocket_acp::SessionInfo> {
            let mut capture = self.capture.lock().expect("lock capture");
            capture.env = env.to_vec();
            capture.servers = mcp_servers.to_vec();
            capture.meta = meta;
            Ok(rocket_acp::SessionInfo {
                session_id: "assistant-1".to_string(),
                config_options: Vec::new(),
                prompt_capabilities: rocket_acp::PromptCapabilities {
                    embedded_context: true,
                    image: false,
                },
            })
        }
        async fn send_prompt(
            &self,
            _session_id: &str,
            _parts: Vec<rocket_acp::PromptPart>,
            _update_tx: UnboundedSender<rocket_acp::AcpUpdate>,
        ) -> DomainResult<String> {
            unreachable!("not exercised by this test")
        }
        async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
            unreachable!("not exercised by this test")
        }
        async fn set_config_option(
            &self,
            _session_id: &str,
            _config_id: &str,
            _value: &str,
        ) -> DomainResult<Vec<rocket_acp::ConfigOption>> {
            unreachable!("not exercised by this test")
        }
        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            Ok(())
        }
        async fn end_all_sessions(&self) -> DomainResult<()> {
            unreachable!("not exercised by this test")
        }
    }

    #[tokio::test]
    async fn start_workspace_session_always_attaches_the_tool_server_and_passes_isolation_settings()
    {
        let capture = Arc::new(Mutex::new(WorkspaceStartCapture::default()));
        let publisher = Arc::new(FakeEventPublisher::new());
        // Plan 02's recording double: proves the session is tracked, so
        // SessionCleanup runs when it ends.
        let cleanup = Arc::new(RecordingCleanup::default());
        // The run switch is off for every collection: the workspace
        // assistant attaches its tools anyway, because each tool checks
        // scope, mode and the switch on every call.
        let service = AcpSessionService::new(
            Box::new(WorkspaceCapturingClient {
                capture: Arc::clone(&capture),
            }),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            cleanup.clone(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );
        let isolation = SessionIsolation {
            config_dir: "/tmp/scratch-config".to_string(),
            system_prompt_append: "rocket".to_string(),
        };

        let info = service
            .start_workspace_session(
                "agent-1",
                "/tmp/scratch-cwd",
                McpHttpServerCredentials {
                    port: 4321,
                    token: "tok-123".to_string(),
                },
                isolation.clone(),
            )
            .await
            .expect("start_workspace_session");

        assert_eq!(info.session_id, "assistant-1");
        {
            let capture = capture.lock().expect("lock capture");
            assert_eq!(capture.servers.len(), 2, "Http and Stdio specs");
            assert!(capture
                .env
                .iter()
                .any(|(k, v)| k == "CLAUDE_CONFIG_DIR" && v == "/tmp/scratch-config"));
            assert!(capture.env.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"));
            assert_eq!(capture.meta, Some(isolation.meta()));
        }
        {
            let events = publisher.events.lock().expect("lock");
            assert!(matches!(
                events.as_slice(),
                [DomainEvent::AcpSessionStarted { .. }]
            ));
        }

        // The command layer tracks the session once its resources exist.
        assert!(service.track("assistant-1"));
        service.end_session("assistant-1").await.expect("end_session");
        assert_eq!(cleanup.ended(), vec!["assistant-1".to_string()]);
    }
}
