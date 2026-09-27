use std::sync::Arc;

use rocket_acp::{AgentConfig, AgentConfigRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::secret_manager_service::SecretManagerService;

pub struct AgentConfigService {
    repo: Box<dyn AgentConfigRepository>,
    secret_manager: Arc<SecretManagerService>,
}

impl AgentConfigService {
    pub fn new(repo: Box<dyn AgentConfigRepository>, secret_manager: Arc<SecretManagerService>) -> Self {
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
}

fn validate_config(config: &AgentConfig) -> DomainResult<()> {
    let required = [
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
        let service = service_with(Ok(None), true);
        let mut blank_label = sample_config("agent-1", "conn-1");
        blank_label.label = "  ".to_string();
        let err = service.save(blank_label).expect_err("must reject blank label");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn save_rejects_unknown_vault_connection_id() {
        let service = service_with(Ok(None), false);
        let err = service
            .save(sample_config("agent-1", "no-such-conn"))
            .expect_err("must reject unknown vault_connection_id");
        assert!(matches!(err, DomainError::InvalidInput(_)));
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
}
