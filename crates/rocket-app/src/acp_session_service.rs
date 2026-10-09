use std::sync::Arc;
use std::time::Duration;

use rocket_acp::{AcpSessionClient, AcpUpdate, PromptPart};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::agent_config_service::AgentConfigService;

/// Orchestrates ACP agent sessions. It resolves an agent's command and
/// credential through `AgentConfigService`, then drives the injected
/// `AcpSessionClient`. It publishes `AcpSession*` domain events for the UI.
///
/// This service keeps no session map of its own. The session client owns
/// session state and process lifecycle.
pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    prompt_timeout: Duration,
}

/// Fixed per-prompt timeout from the spec. It is not user-configurable.
const DEFAULT_PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

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
    /// Production constructor. Uses the fixed 120-second prompt timeout.
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    ) -> Self {
        Self::with_prompt_timeout(
            session_client,
            event_publisher,
            agent_config_service,
            collection_repo,
            DEFAULT_PROMPT_TIMEOUT,
        )
    }

    /// Test seam only — production wiring (Plan 05) always uses `new`, which
    /// fixes this at the spec's 120-second constant. This constructor does
    /// not add end-user configurability.
    pub fn with_prompt_timeout(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        prompt_timeout: Duration,
    ) -> Self {
        Self {
            session_client,
            event_publisher,
            agent_config_service,
            collection_repo,
            prompt_timeout,
        }
    }

    /// Spawns the configured agent and opens an ACP session in `cwd`.
    ///
    /// `cwd` comes from the caller, not from `AgentConfig::working_dir`, as
    /// the spec requires. Config lookup and credential resolution errors
    /// propagate unchanged. No event is published on failure, because no
    /// session id exists yet. On success, `AcpSessionStarted` is published
    /// and the new session id is returned.
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
    ) -> DomainResult<String> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = vec![(config.credential_env_var.clone(), credential)];

        let autonomy_enabled = self
            .collection_repo
            .get_settings(collection)?
            .agent_autonomy_enabled;
        let mcp_servers: Vec<rocket_acp::McpServerSpec> = match (autonomy_enabled, mcp_http) {
            (true, Some(creds)) => {
                let exe = std::env::current_exe().map_err(|e| {
                    DomainError::Internal(format!("could not resolve current executable: {e}"))
                })?;
                vec![
                    rocket_acp::McpServerSpec::Http {
                        name: "rocket".to_string(),
                        // The path literal ("/mcp") must match
                        // `src_tauri::mcp::tool_server::MCP_HTTP_PATH`
                        // exactly — the bare origin 404s (Plan 04's Final
                        // Review). This crate cannot import that constant
                        // (`rocket-app` never depends on `src-tauri` — this
                        // repo's DDD boundary runs the other way), so it is
                        // duplicated here as a literal; if that constant's
                        // value ever changes, this literal must change with
                        // it.
                        url: format!("http://127.0.0.1:{}/mcp", creds.port),
                        token: creds.token.clone(),
                    },
                    rocket_acp::McpServerSpec::Stdio {
                        name: "rocket".to_string(),
                        command: exe.to_string_lossy().into_owned(),
                        args: vec!["--acp-mcp-stdio-bridge".to_string()],
                        env: vec![
                            ("ROCKET_MCP_PORT".to_string(), creds.port.to_string()),
                            ("ROCKET_MCP_TOKEN".to_string(), creds.token),
                        ],
                    },
                ]
            }
            // Autonomy is off, or the caller couldn't spawn the HTTP server
            // (fails open to chat-only mode rather than failing the whole
            // session start over a tool-server hiccup) — no MCP servers.
            _ => Vec::new(),
        };

        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &mcp_servers, None)
            .await?
            .session_id;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: session_id.clone(),
            });
        Ok(session_id)
    }

    /// Sends one prompt turn and returns the agent's stop reason string.
    ///
    /// Each streamed chunk is published as `AcpSessionChunk`. Then exactly one
    /// terminal event follows: `AcpSessionFinished` on success, or
    /// `AcpSessionFailed` on error or timeout. The error is still returned to
    /// the caller in both failure cases.
    ///
    /// Ordering: every `AcpSessionChunk` is published before the terminal
    /// event. `tokio::join!` only completes once the chunk channel closes,
    /// and it closes only when the client's `send_prompt` future has resolved
    /// and dropped its sender. This holds for any client implementation.
    ///
    /// Completeness is a separate client-side contract: the client must
    /// forward all of a turn's chunks before its `send_prompt` resolves.
    /// `AcpAgentClient` relies on the ACP connection dispatching a turn's
    /// `session/update` notifications before its `PromptResponse`. A chunk
    /// arriving later would be dropped, never reordered.
    ///
    /// On timeout, the session is force-killed via `end_session`, because a
    /// hung agent process is still running.
    pub async fn send_prompt(&self, session_id: &str, prompt: String) -> DomainResult<String> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AcpUpdate>();
        let session_id_owned = session_id.to_string();

        let drain_chunks = async {
            while let Some(update) = rx.recv().await {
                // Task 2 of the ACP client upgrade publishes the other update kinds.
                if let AcpUpdate::Text { text } = update {
                    self.event_publisher.publish(DomainEvent::AcpSessionChunk {
                        session_id: session_id_owned.clone(),
                        text,
                    });
                }
            }
        };
        let send = self
            .session_client
            .send_prompt(session_id, vec![PromptPart::Text(prompt)], tx);

        let joined = tokio::time::timeout(self.prompt_timeout, async {
            tokio::join!(drain_chunks, send)
        })
        .await;

        match joined {
            Ok((_, Ok(stop_reason))) => {
                self.event_publisher
                    .publish(DomainEvent::AcpSessionFinished {
                        session_id: session_id.to_string(),
                        stop_reason: stop_reason.clone(),
                    });
                Ok(stop_reason)
            }
            Ok((_, Err(e))) => {
                self.event_publisher.publish(DomainEvent::AcpSessionFailed {
                    session_id: session_id.to_string(),
                    error: e.to_string(),
                });
                Err(e)
            }
            Err(_elapsed) => {
                // The kill result is ignored on purpose. The timeout is the
                // error the caller must see. A kill failure, for example when
                // the session already crashed and was removed, changes nothing.
                let _ = self.session_client.end_session(session_id).await;
                let message = format!(
                    "agent did not respond within {}s",
                    self.prompt_timeout.as_secs()
                );
                self.event_publisher.publish(DomainEvent::AcpSessionFailed {
                    session_id: session_id.to_string(),
                    error: message.clone(),
                });
                Err(DomainError::Internal(message))
            }
        }
    }

    /// Ends the session and kills its agent process. No event is published.
    /// An unknown or already-ended session id returns the client's error.
    pub async fn end_session(&self, session_id: &str) -> DomainResult<()> {
        self.session_client.end_session(session_id).await
    }

    /// Kills every currently-tracked agent session's process. Intended for
    /// app-exit cleanup — the caller does not know individual session ids at
    /// that point, so this delegates straight to the session client, which
    /// owns the session map. No event is published.
    pub async fn end_all_sessions(&self) -> DomainResult<()> {
        self.session_client.end_all_sessions().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use rocket_acp::{AgentConfig, AgentConfigRepository};
    use rocket_acp::{ConfigOption, PromptCapabilities, SessionInfo};
    use rocket_environment::external_secret::ExternalSecretRef;
    use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
    use rocket_environment::secret_store::SecretStore;
    use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
    use tokio::sync::mpsc::UnboundedSender;

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
            }
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
                    session_id: "session-1".to_string(),
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

    #[tokio::test]
    async fn start_session_resolves_config_and_credential_and_publishes_started() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let session_id = service
            .start_session("agent-1", "/tmp", "demo", None)
            .await
            .expect("start_session should succeed");
        assert_eq!(session_id, "session-1");

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
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let stop_reason = service
            .send_prompt("session-1", "hi".to_string())
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
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let err = service
            .start_session("no-such-agent", "/tmp", "demo", None)
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
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let err = service
            .start_session("agent-1", "/tmp", "demo", None)
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
            agent_config_service_with(FakeVaultFetcher {
                secret_value_result: Ok(None),
            }),
            ConfigurableCollectionRepo::new(),
        );

        let err = service
            .start_session("agent-1", "/tmp", "demo", None)
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
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let err = service
            .send_prompt("session-1", "hi".to_string())
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
        let service = AcpSessionService::with_prompt_timeout(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
            Duration::from_millis(20),
        );

        let err = service
            .send_prompt("session-1", "hi".to_string())
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
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let session_id = service
            .start_session("agent-1", "/tmp", "unconfigured-collection", None)
            .await
            .expect("an unconfigured collection must still start a chat-only session");
        assert_eq!(session_id, "session-1");
    }

    #[tokio::test]
    async fn start_session_with_autonomy_disabled_ignores_provided_mcp_http_credentials() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>> =
            Arc::new(Mutex::new(Vec::new()));
        let client = CapturingSessionClient {
            captured_servers: Arc::clone(&captured_servers),
        };
        let collection_repo = ConfigurableCollectionRepo::new();
        collection_repo.set_autonomy("my-api", false);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
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
            )
            .await
            .expect("a disabled collection must still be able to start a chat-only session");
        assert_eq!(session_id, "session-1");
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
        };
        let collection_repo = ConfigurableCollectionRepo::with_autonomy_enabled("my-api", true);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            collection_repo,
        );

        let session_id = service
            .start_session("agent-1", "/tmp", "my-api", None)
            .await
            .expect("a missing MCP HTTP server must fail open to a chat-only session");
        assert_eq!(session_id, "session-1");
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
            agent_config_service(),
            collection_repo,
        );

        let err = service
            .start_session("agent-1", "/tmp", "broken-collection", None)
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

    struct CapturingSessionClient {
        captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>>,
    }
    #[async_trait::async_trait]
    impl AcpSessionClient for CapturingSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            mcp_servers: &[rocket_acp::McpServerSpec],
            _meta: Option<serde_json::Value>,
        ) -> DomainResult<SessionInfo> {
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
        };
        let collection_repo = ConfigurableCollectionRepo::with_autonomy_enabled("demo", true);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
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
}
