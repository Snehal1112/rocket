use std::sync::Arc;

use rocket_acp::{AgentConfig, AgentConfigRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::secret_manager_service::SecretManagerService;

pub struct AgentConfigService {
    repo: Box<dyn AgentConfigRepository>,
    secret_manager: Arc<SecretManagerService>,
}

impl AgentConfigService {
    pub fn new(
        repo: Box<dyn AgentConfigRepository>,
        secret_manager: Arc<SecretManagerService>,
    ) -> Self {
        Self {
            repo,
            secret_manager,
        }
    }

    pub fn list(&self) -> DomainResult<Vec<AgentConfig>> {
        self.repo.list()
    }

    pub fn save(&self, config: AgentConfig) -> DomainResult<()> {
        validate_config(&config)?;
        let known_connection = self
            .secret_manager
            .list()?
            .iter()
            .any(|c| c.id == config.vault_connection_id);
        if !known_connection {
            return Err(DomainError::InvalidInput(format!(
                "agent '{}': vault_connection_id '{}' does not match any configured RocketVault connection",
                config.label, config.vault_connection_id
            )));
        }
        self.repo.save(&config)
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        self.repo.delete(id)
    }

    /// Fetches one agent's full configuration by id — needed by `AcpSessionService`
    /// (subproject B), which requires `command`/`args`/`working_dir`/
    /// `credential_env_var`, not just the resolved credential value.
    pub fn get(&self, id: &str) -> DomainResult<AgentConfig> {
        self.get_config(id)
    }

    /// Resolves the agent's API key from RocketVault. A vault secret that no
    /// longer exists maps to `NotFound`; connection, keychain, and transport
    /// failures from `SecretManagerService` propagate unchanged.
    pub async fn resolve_credential(&self, id: &str) -> DomainResult<String> {
        let config = self.get_config(id)?;
        self.resolve_credential_for(&config).await
    }

    /// Checks that `command` resolves on PATH (no process is spawned) and
    /// that the credential resolves. Both checks must pass.
    pub async fn test_agent_config(&self, id: &str) -> DomainResult<()> {
        let config = self.get_config(id)?;
        which::which(&config.command).map_err(|e| {
            DomainError::InvalidInput(format!(
                "agent '{}': command '{}' not found: {e}",
                config.label, config.command
            ))
        })?;
        self.resolve_credential_for(&config).await?;
        Ok(())
    }

    fn get_config(&self, id: &str) -> DomainResult<AgentConfig> {
        self.repo
            .get(id)?
            .ok_or_else(|| DomainError::NotFound(format!("agent config '{id}'")))
    }

    async fn resolve_credential_for(&self, config: &AgentConfig) -> DomainResult<String> {
        let value = self
            .secret_manager
            .resolve_secret_value(
                &config.vault_connection_id,
                &config.vault_name,
                &config.vault_secret_id,
            )
            .await?;
        value.ok_or_else(|| {
            DomainError::NotFound(format!(
                "agent '{}': credential no longer exists in RocketVault — reconfigure this agent",
                config.label
            ))
        })
    }
}

fn validate_config(config: &AgentConfig) -> DomainResult<()> {
    let required = [
        ("id", &config.id),
        ("label", &config.label),
        ("command", &config.command),
        ("credential_env_var", &config.credential_env_var),
        ("vault_connection_id", &config.vault_connection_id),
        ("vault_name", &config.vault_name),
        ("vault_secret_id", &config.vault_secret_id),
    ];
    for (field, value) in required {
        if value.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "agent config {field} must not be empty"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::external_secret::ExternalSecretRef;
    use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
    use rocket_environment::secret_store::SecretStore;
    use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
    use std::sync::Mutex;

    struct FakeAgentConfigRepo(Mutex<Vec<AgentConfig>>);
    impl FakeAgentConfigRepo {
        fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }
    }
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

    struct FakeVaultFetcher {
        secret_value_result: DomainResult<Option<String>>,
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

    fn sample_connection(id: &str) -> SecretManagerConnection {
        SecretManagerConnection {
            id: id.to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    fn sample_config(id: &str, vault_connection_id: &str) -> AgentConfig {
        AgentConfig {
            id: id.to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: Vec::new(),
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            vault_connection_id: vault_connection_id.to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "secret-id-1".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    fn service_with(
        secret_value_result: DomainResult<Option<String>>,
        seed_connection: bool,
    ) -> AgentConfigService {
        let sm_repo = FakeSecretManagerRepo(Mutex::new(if seed_connection {
            vec![sample_connection("conn-1")]
        } else {
            Vec::new()
        }));
        let secret_manager = Arc::new(SecretManagerService::new(
            Box::new(sm_repo),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher {
                secret_value_result,
            }),
        ));
        AgentConfigService::new(Box::new(FakeAgentConfigRepo::new()), secret_manager)
    }

    #[test]
    fn list_delegates_to_repo() {
        let service = service_with(Ok(None), true);
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save");
        assert_eq!(service.list().expect("list").len(), 1);
    }

    #[test]
    fn save_rejects_blank_required_fields() {
        type Blanker = (&'static str, fn(&mut AgentConfig));
        let service = service_with(Ok(None), true);
        let blankers: [Blanker; 7] = [
            ("id", |c| c.id = " ".to_string()),
            ("label", |c| c.label = "  ".to_string()),
            ("command", |c| c.command = String::new()),
            ("credential_env_var", |c| {
                c.credential_env_var = " ".to_string()
            }),
            ("vault_connection_id", |c| {
                c.vault_connection_id = String::new()
            }),
            ("vault_name", |c| c.vault_name = String::new()),
            ("vault_secret_id", |c| c.vault_secret_id = "\t".to_string()),
        ];
        for (field, blank) in blankers {
            let mut config = sample_config("agent-1", "conn-1");
            blank(&mut config);
            let err = service
                .save(config)
                .expect_err("must reject a blank required field");
            assert!(
                matches!(err, DomainError::InvalidInput(_)),
                "blank {field} should be InvalidInput, got {err:?}"
            );
        }
        assert!(service.list().expect("list").is_empty());
    }

    #[test]
    fn save_rejects_unknown_vault_connection_id() {
        let service = service_with(Ok(None), false);
        let err = service
            .save(sample_config("agent-1", "no-such-conn"))
            .expect_err("must reject unknown vault_connection_id");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(
            service.list().expect("list").is_empty(),
            "a rejected save must not persist the config"
        );
    }

    #[test]
    fn delete_delegates_to_repo() {
        let service = service_with(Ok(None), true);
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save");
        service.delete("agent-1").expect("delete");
        assert!(service.list().expect("list").is_empty());
    }

    #[tokio::test]
    async fn resolve_credential_returns_value_when_secret_exists() {
        let service = service_with(Ok(Some("sk-abc123".to_string())), true);
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save");

        let value = service
            .resolve_credential("agent-1")
            .await
            .expect("resolve_credential should succeed");

        assert_eq!(value, "sk-abc123");
    }

    #[tokio::test]
    async fn resolve_credential_errors_when_secret_stale() {
        let service = service_with(Ok(None), true);
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save");

        let err = service
            .resolve_credential("agent-1")
            .await
            .expect_err("stale vault secret must be a hard error");

        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn resolve_credential_propagates_transport_failure_distinct_from_stale() {
        let service = service_with(
            Err(DomainError::Internal("network timeout".to_string())),
            true,
        );
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save");

        let err = service
            .resolve_credential("agent-1")
            .await
            .expect_err("a transport failure must be an error, not a panic");

        // A transport failure stays Internal so it is not confused with a
        // stale secret, which maps to NotFound.
        assert!(
            matches!(err, DomainError::Internal(_)),
            "expected DomainError::Internal, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_agent_config_propagates_transport_failure() {
        let service = service_with(
            Err(DomainError::Internal("network timeout".to_string())),
            true,
        );
        let mut config = sample_config("agent-1", "conn-1");
        config.command = env!("CARGO").to_string();
        service.save(config).expect("save");

        let err = service
            .test_agent_config("agent-1")
            .await
            .expect_err("a transport failure must fail the test action");
        assert!(matches!(err, DomainError::Internal(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn test_agent_config_unknown_config_id_errors() {
        let service = service_with(Ok(Some("sk-abc123".to_string())), true);
        let err = service
            .test_agent_config("no-such-agent")
            .await
            .expect_err("unknown agent config id must error");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn resolve_credential_unknown_config_id_errors() {
        let service = service_with(Ok(None), true);
        let err = service
            .resolve_credential("no-such-agent")
            .await
            .expect_err("unknown agent config id must error");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn resolve_credential_errors_when_vault_connection_deleted_after_save() {
        // Built directly (not via service_with) so the test keeps its own
        // handle to the SecretManagerService and can delete the connection
        // between save() and resolve_credential(), simulating a dangling
        // reference.
        let sm_repo = FakeSecretManagerRepo(Mutex::new(vec![sample_connection("conn-1")]));
        let secret_manager = Arc::new(SecretManagerService::new(
            Box::new(sm_repo),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher {
                secret_value_result: Ok(Some("sk-abc123".to_string())),
            }),
        ));
        let service = AgentConfigService::new(
            Box::new(FakeAgentConfigRepo::new()),
            Arc::clone(&secret_manager),
        );
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save while connection still exists");

        secret_manager.delete("conn-1").expect("delete connection");

        let err = service
            .resolve_credential("agent-1")
            .await
            .expect_err("a dangling vault_connection_id must error, not panic");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_agent_config_fails_when_command_not_found() {
        let service = service_with(Ok(Some("sk-abc123".to_string())), true);
        let mut config = sample_config("agent-1", "conn-1");
        config.command = "definitely-not-a-real-binary-xyz123".to_string();
        service.save(config).expect("save");

        let err = service
            .test_agent_config("agent-1")
            .await
            .expect_err("nonexistent command must fail the test");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_agent_config_fails_when_credential_stale_even_if_command_exists() {
        let service = service_with(Ok(None), true);
        let mut config = sample_config("agent-1", "conn-1");
        // env!("CARGO") is set by Cargo at build time to the exact cargo
        // binary running this test — a real, executable, cross-platform-safe
        // path.
        config.command = env!("CARGO").to_string();
        service.save(config).expect("save");

        let err = service
            .test_agent_config("agent-1")
            .await
            .expect_err("stale credential must fail the test even though the command resolves");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_agent_config_succeeds_when_command_and_credential_resolve() {
        let service = service_with(Ok(Some("sk-abc123".to_string())), true);
        let mut config = sample_config("agent-1", "conn-1");
        config.command = env!("CARGO").to_string();
        service.save(config).expect("save");

        service
            .test_agent_config("agent-1")
            .await
            .expect("test_agent_config should succeed");
    }

    #[test]
    fn get_returns_config_when_it_exists() {
        let service = service_with(Ok(None), true);
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save");
        let config = service.get("agent-1").expect("get should find the config");
        assert_eq!(config.id, "agent-1");
    }

    #[test]
    fn get_errors_when_unknown() {
        let service = service_with(Ok(None), true);
        let err = service
            .get("no-such-agent")
            .expect_err("unknown id must error");
        assert!(matches!(err, DomainError::NotFound(_)));
    }
}
