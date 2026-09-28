use std::sync::Arc;
use std::time::Duration;

use rocket_acp::AcpSessionClient;
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
    pub async fn start_session(
        &self,
        agent_config_id: &str,
        cwd: &str,
        collection: Option<&str>,
    ) -> DomainResult<String> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = vec![(config.credential_env_var.clone(), credential)];
        let mcp_servers = self.mcp_server_specs_for(collection)?;
        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &mcp_servers)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: session_id.clone(),
            });
        Ok(session_id)
    }

    /// Resolves the MCP servers to attach to a new session. A session with
    /// no target collection, or whose collection has not opted into agent
    /// autonomy, gets none — chat-only mode, identical to subproject C's
    /// existing behavior. No MCP server implementation exists yet (the HTTP
    /// backend lands in Plan 04, the Stdio shim in Plan 05), so an opted-in
    /// collection also gets an empty list today — there is nothing yet to
    /// attach. This still re-checks `get_settings` on every call (not just
    /// once), matching the design spec's "checked on every call" rule for
    /// the tools themselves, and still propagates a lookup failure instead
    /// of silently falling back to chat-only mode, so a broken collection
    /// name surfaces loudly rather than silently degrading.
    fn mcp_server_specs_for(
        &self,
        collection: Option<&str>,
    ) -> DomainResult<Vec<rocket_acp::McpServerSpec>> {
        let Some(collection) = collection else {
            return Ok(Vec::new());
        };
        let settings = self.collection_repo.get_settings(collection)?;
        if !settings.agent_autonomy_enabled {
            return Ok(Vec::new());
        }
        Ok(Vec::new())
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
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let session_id_owned = session_id.to_string();

        let drain_chunks = async {
            while let Some(text) = rx.recv().await {
                self.event_publisher.publish(DomainEvent::AcpSessionChunk {
                    session_id: session_id_owned.clone(),
                    text,
                });
            }
        };
        let send = self.session_client.send_prompt(session_id, prompt, tx);

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
        prompt_chunks: Vec<String>,
        prompt_stop_reason: String,
        prompt_should_fail: bool,
        prompt_delay: Duration,
        end_session_called: Arc<AtomicBool>,
        end_all_sessions_called: Arc<AtomicBool>,
    }
    impl Default for FakeSessionClient {
        fn default() -> Self {
            Self {
                start_should_fail: false,
                prompt_chunks: vec!["hello".to_string()],
                prompt_stop_reason: "end_turn".to_string(),
                prompt_should_fail: false,
                prompt_delay: Duration::ZERO,
                end_session_called: Arc::new(AtomicBool::new(false)),
                end_all_sessions_called: Arc::new(AtomicBool::new(false)),
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
        ) -> DomainResult<String> {
            if self.start_should_fail {
                Err(DomainError::InvalidInput("command not found".to_string()))
            } else {
                Ok("session-1".to_string())
            }
        }
        async fn send_prompt(
            &self,
            _session_id: &str,
            _prompt: String,
            chunk_tx: UnboundedSender<String>,
        ) -> DomainResult<String> {
            tokio::time::sleep(self.prompt_delay).await;
            for chunk in &self.prompt_chunks {
                let _ = chunk_tx.send(chunk.clone());
            }
            if self.prompt_should_fail {
                Err(DomainError::Internal("agent crashed".to_string()))
            } else {
                Ok(self.prompt_stop_reason.clone())
            }
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
            .start_session("agent-1", "/tmp", None)
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
            prompt_chunks: vec!["Hello, ".to_string(), "world!".to_string()],
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
            .start_session("no-such-agent", "/tmp", None)
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
            .start_session("agent-1", "/tmp", None)
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
            .start_session("agent-1", "/tmp", None)
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
    async fn start_session_with_no_collection_still_works_exactly_as_before() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        let session_id = service
            .start_session("agent-1", "/tmp", None)
            .await
            .expect("start_session with no collection must keep working");
        assert_eq!(session_id, "session-1");
    }

    #[tokio::test]
    async fn start_session_with_autonomy_disabled_still_succeeds_with_no_mcp_servers() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let collection_repo = ConfigurableCollectionRepo::new();
        collection_repo.set_autonomy("my-api", false);
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            collection_repo,
        );

        let session_id = service
            .start_session("agent-1", "/tmp", Some("my-api"))
            .await
            .expect("a disabled collection must still be able to start a chat-only session");
        assert_eq!(session_id, "session-1");
    }

    #[tokio::test]
    async fn start_session_with_autonomy_enabled_still_succeeds_since_no_mcp_backend_exists_yet() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let collection_repo = ConfigurableCollectionRepo::with_autonomy_enabled("my-api", true);
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            agent_config_service(),
            collection_repo,
        );

        // Plan 03 has no HTTP backend or Stdio shim to attach yet, so an
        // opted-in collection behaves identically to a disabled one today —
        // this is this plan's complete, correct behavior (see this plan's
        // "Deviations from the Plan Index" item 4), not a bug to fix later.
        let session_id = service
            .start_session("agent-1", "/tmp", Some("my-api"))
            .await
            .expect("an opted-in collection must still start a session");
        assert_eq!(session_id, "session-1");
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
            .start_session("agent-1", "/tmp", Some("broken-collection"))
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
}
