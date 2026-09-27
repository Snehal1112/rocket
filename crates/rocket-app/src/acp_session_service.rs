use std::sync::Arc;
use std::time::Duration;

use rocket_acp::AcpSessionClient;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::agent_config_service::AgentConfigService;

pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
    prompt_timeout: Duration,
}

const DEFAULT_PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

impl AcpSessionService {
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
    ) -> Self {
        Self::with_prompt_timeout(
            session_client,
            event_publisher,
            agent_config_service,
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
        prompt_timeout: Duration,
    ) -> Self {
        Self {
            session_client,
            event_publisher,
            agent_config_service,
            prompt_timeout,
        }
    }

    pub async fn start_session(&self, agent_config_id: &str, cwd: &str) -> DomainResult<String> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = vec![(config.credential_env_var.clone(), credential)];
        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env)
            .await?;
        self.event_publisher.publish(DomainEvent::AcpSessionStarted {
            session_id: session_id.clone(),
        });
        Ok(session_id)
    }

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
                self.event_publisher.publish(DomainEvent::AcpSessionFinished {
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
            self.events.lock().expect("lock FakeEventPublisher").push(event);
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
        );

        let session_id = service
            .start_session("agent-1", "/tmp")
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
        );

        let stop_reason = service
            .send_prompt("session-1", "hi".to_string())
            .await
            .expect("send_prompt should succeed");
        assert_eq!(stop_reason, "end_turn");

        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 3, "expected 2 chunks then 1 finished, got {events:?}");
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
}
