use std::sync::Arc;

use rocket_acp::{
    validate_credential_source, AgentConfig, AgentConfigRepository, CredentialSource,
};
use rocket_environment::secret_store::SecretStore;
use rocket_shared::error::{DomainError, DomainResult};

use crate::secret_manager_service::SecretManagerService;

/// Reads one variable from Rocket's own process environment. Injected so the
/// service holds no process state and tests need no real environment.
pub type EnvReader = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// The reader used in production. A variable that is not valid Unicode counts
/// as not set.
pub fn process_env_reader() -> EnvReader {
    Arc::new(|name| std::env::var(name).ok())
}

/// The keychain scope that holds agent credentials, keyed by config id.
const LOCAL_CREDENTIAL_SCOPE: &str = "agent-credential";

/// Longest accepted keychain credential, in bytes.
const MAX_LOCAL_CREDENTIAL_BYTES: usize = 4096;

pub struct AgentConfigService {
    repo: Box<dyn AgentConfigRepository>,
    secret_manager: Arc<SecretManagerService>,
    local_secrets: Arc<dyn SecretStore>,
    env_reader: EnvReader,
}

impl AgentConfigService {
    pub fn new(
        repo: Box<dyn AgentConfigRepository>,
        secret_manager: Arc<SecretManagerService>,
        local_secrets: Arc<dyn SecretStore>,
        env_reader: EnvReader,
    ) -> Self {
        Self {
            repo,
            secret_manager,
            local_secrets,
            env_reader,
        }
    }

    pub fn list(&self) -> DomainResult<Vec<AgentConfig>> {
        self.repo.list()
    }

    pub fn save(&self, config: AgentConfig) -> DomainResult<()> {
        validate_config(&config)?;
        if config.credential_source == CredentialSource::Vault {
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
        }
        let previous = self.repo.get(&config.id)?;
        self.repo.save(&config)?;
        let left_keychain = previous.is_some_and(|p| {
            p.credential_source == CredentialSource::Keychain
                && config.credential_source != CredentialSource::Keychain
        });
        if left_keychain {
            self.forget_local_credential(&config.id);
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        let previous = self.repo.get(id)?;
        self.repo.delete(id)?;
        if previous.is_some_and(|p| p.credential_source == CredentialSource::Keychain) {
            self.forget_local_credential(id);
        }
        Ok(())
    }

    /// Removes the keychain entry of a config. A failure is logged without any
    /// value and does not fail the caller, because the config change already
    /// happened.
    fn forget_local_credential(&self, id: &str) {
        if let Err(e) = self.local_secrets.delete(LOCAL_CREDENTIAL_SCOPE, id) {
            tracing::warn!(error = %e, "could not remove an agent credential from the keychain");
        }
    }

    /// Fetches one agent's full configuration by id — needed by `AcpSessionService`
    /// (subproject B), which requires `command`/`args`/`working_dir`/
    /// `credential_env_var`, not just the resolved credential value.
    pub fn get(&self, id: &str) -> DomainResult<AgentConfig> {
        self.get_config(id)
    }

    /// Resolves the agent's credential from its source: RocketVault, a
    /// variable of Rocket's own environment, or the OS keychain. A missing
    /// credential maps to `NotFound`; connection, keychain, and transport
    /// failures propagate unchanged.
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

    /// Saves the credential of a `keychain` config in the OS keychain.
    pub fn set_local_credential(&self, config_id: &str, value: &str) -> DomainResult<()> {
        let config = self.get_config(config_id)?;
        require_keychain(&config)?;
        let value = value.trim();
        validate_local_credential(value)?;
        self.local_secrets
            .set(LOCAL_CREDENTIAL_SCOPE, config_id, value)
    }

    /// Removes the keychain entry of a config. Missing is fine.
    pub fn clear_local_credential(&self, config_id: &str) -> DomainResult<()> {
        self.get_config(config_id)?;
        self.local_secrets.delete(LOCAL_CREDENTIAL_SCOPE, config_id)
    }

    /// True when a `keychain` config has a saved entry. Other sources are false.
    pub fn has_local_credential(&self, config_id: &str) -> DomainResult<bool> {
        let config = self.get_config(config_id)?;
        if config.credential_source != CredentialSource::Keychain {
            return Ok(false);
        }
        Ok(self
            .local_secrets
            .get_strict(LOCAL_CREDENTIAL_SCOPE, config_id)?
            .is_some())
    }

    fn get_config(&self, id: &str) -> DomainResult<AgentConfig> {
        self.repo
            .get(id)?
            .ok_or_else(|| DomainError::NotFound(format!("agent config '{id}'")))
    }

    async fn resolve_credential_for(&self, config: &AgentConfig) -> DomainResult<String> {
        match config.credential_source {
            CredentialSource::Vault => self.resolve_vault_credential(config).await,
            CredentialSource::Env => {
                let name = config.source_env_var.as_deref().unwrap_or_default();
                (self.env_reader)(name)
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        DomainError::NotFound(format!(
                            "agent '{}': environment variable '{name}' is not set in Rocket's environment",
                            config.label
                        ))
                    })
            }
            CredentialSource::Keychain => self
                .local_secrets
                .get_strict(LOCAL_CREDENTIAL_SCOPE, &config.id)?
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    DomainError::NotFound(format!(
                        "agent '{}': no credential saved in the keychain; open the agent settings and save one",
                        config.label
                    ))
                }),
        }
    }

    async fn resolve_vault_credential(&self, config: &AgentConfig) -> DomainResult<String> {
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

fn require_keychain(config: &AgentConfig) -> DomainResult<()> {
    if config.credential_source != CredentialSource::Keychain {
        return Err(DomainError::InvalidInput(format!(
            "agent '{}': its credential source is not the keychain",
            config.label
        )));
    }
    Ok(())
}

/// The error text never contains the value.
fn validate_local_credential(value: &str) -> DomainResult<()> {
    if value.is_empty() {
        return Err(DomainError::InvalidInput(
            "credential must not be empty".to_string(),
        ));
    }
    if value.len() > MAX_LOCAL_CREDENTIAL_BYTES {
        return Err(DomainError::InvalidInput(format!(
            "credential is too long (at most {MAX_LOCAL_CREDENTIAL_BYTES} bytes)"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(DomainError::InvalidInput(
            "credential must not contain control characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_config(config: &AgentConfig) -> DomainResult<()> {
    let required = [
        ("id", &config.id),
        ("label", &config.label),
        ("command", &config.command),
        ("credential_env_var", &config.credential_env_var),
    ];
    for (field, value) in required {
        if value.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "agent config {field} must not be empty"
            )));
        }
    }
    validate_credential_source(config)
        .map_err(|why| DomainError::InvalidInput(format!("agent config {why}")))?;
    rocket_acp::validate_env_config(&config.credential_env_var, &config.extra_env)
        .map_err(|why| DomainError::InvalidInput(format!("agent config {why}")))?;
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
            provider: Default::default(),
            config: None,
        }
    }

    fn sample_config(id: &str, vault_connection_id: &str) -> AgentConfig {
        AgentConfig {
            id: id.to_string(),
            label: "Claude Agent".to_string(),
            kind: rocket_acp::AgentKind::default(),
            command: "claude-agent-acp".to_string(),
            args: Vec::new(),
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            extra_env: Vec::new(),
            credential_source: rocket_acp::CredentialSource::Vault,
            source_env_var: None,
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
        AgentConfigService::new(
            Box::new(FakeAgentConfigRepo::new()),
            secret_manager,
            Arc::new(MapStore::default()),
            env_reader(&[]),
        )
    }

    /// An in-memory keychain. `fail` makes every call an error, like a locked keychain.
    #[derive(Default)]
    struct MapStore {
        entries: Mutex<std::collections::HashMap<(String, String), String>>,
        fail: std::sync::atomic::AtomicBool,
    }
    impl SecretStore for MapStore {
        fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            self.get_strict(scope, key)
        }
        fn get_strict(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(DomainError::Internal(
                    "the keychain could not be read".to_string(),
                ));
            }
            Ok(self
                .entries
                .lock()
                .expect("lock")
                .get(&(scope.into(), key.into()))
                .cloned())
        }
        fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
            self.entries
                .lock()
                .expect("lock")
                .insert((scope.into(), key.into()), value.into());
            Ok(())
        }
        fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
            self.entries
                .lock()
                .expect("lock")
                .remove(&(scope.to_string(), key.to_string()));
            Ok(())
        }
    }

    fn env_reader(pairs: &[(&str, &str)]) -> EnvReader {
        let map: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Arc::new(move |name| map.get(name).cloned())
    }

    fn local_service(store: Arc<MapStore>, env: EnvReader) -> AgentConfigService {
        let secret_manager = Arc::new(SecretManagerService::new(
            Box::new(FakeSecretManagerRepo(Mutex::new(Vec::new()))),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher {
                secret_value_result: Ok(None),
            }),
        ));
        AgentConfigService::new(
            Box::new(FakeAgentConfigRepo::new()),
            secret_manager,
            store,
            env,
        )
    }

    fn env_config(id: &str, var: &str) -> AgentConfig {
        let mut config = sample_config(id, "");
        config.credential_source = CredentialSource::Env;
        config.source_env_var = Some(var.to_string());
        config.vault_connection_id = String::new();
        config.vault_name = String::new();
        config.vault_secret_id = String::new();
        config
    }

    fn keychain_config(id: &str) -> AgentConfig {
        let mut config = env_config(id, "UNUSED");
        config.credential_source = CredentialSource::Keychain;
        config.source_env_var = None;
        config
    }

    const FAKE_TOKEN: &str = "sk-ant-oat01-FAKE-VALUE-FOR-TESTS";

    #[tokio::test]
    async fn an_env_config_reads_the_named_variable_trimmed() {
        let service = local_service(
            Arc::default(),
            env_reader(&[("MY_TOKEN", &format!("  {FAKE_TOKEN}\n"))]),
        );
        service
            .save(env_config("a", "MY_TOKEN"))
            .expect("save without a vault connection");
        assert_eq!(
            service.resolve_credential("a").await.expect("resolve"),
            FAKE_TOKEN
        );
    }

    #[tokio::test]
    async fn an_env_config_with_a_missing_or_blank_variable_names_it_and_never_a_value() {
        let service = local_service(Arc::default(), env_reader(&[("BLANK", "   ")]));
        service.save(env_config("a", "NOT_SET")).expect("save");
        service.save(env_config("b", "BLANK")).expect("save");
        for id in ["a", "b"] {
            let err = service.resolve_credential(id).await.expect_err("not set");
            assert!(matches!(err, DomainError::NotFound(_)), "{err:?}");
        }
        let message = service
            .resolve_credential("a")
            .await
            .expect_err("not set")
            .to_string();
        assert!(message.contains("NOT_SET"), "{message}");
    }

    #[tokio::test]
    async fn a_keychain_config_reads_the_saved_entry_and_separates_missing_from_locked() {
        let store = Arc::new(MapStore::default());
        let service = local_service(Arc::clone(&store), env_reader(&[]));
        service.save(keychain_config("a")).expect("save");

        let missing = service
            .resolve_credential("a")
            .await
            .expect_err("no entry yet");
        assert!(matches!(missing, DomainError::NotFound(_)), "{missing:?}");

        service
            .set_local_credential("a", &format!("  {FAKE_TOKEN}  "))
            .expect("set");
        assert_eq!(
            service.resolve_credential("a").await.expect("resolve"),
            FAKE_TOKEN
        );

        store.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        let locked = service
            .resolve_credential("a")
            .await
            .expect_err("locked keychain");
        assert!(
            matches!(locked, DomainError::Internal(_)),
            "locked is not a miss: {locked:?}"
        );
        assert!(!locked.to_string().contains(FAKE_TOKEN));
    }

    #[test]
    fn set_local_credential_enforces_the_rules_and_never_echoes_the_value() {
        let service = local_service(Arc::default(), env_reader(&[]));
        service.save(keychain_config("a")).expect("save");
        service.save(env_config("e", "X")).expect("save env");
        let too_long = "x".repeat(4097);
        let with_control = format!("{FAKE_TOKEN}\u{0007}");
        for (label, value) in [
            ("empty", ""),
            ("blank", "  \n"),
            ("control", with_control.as_str()),
            ("too long", too_long.as_str()),
        ] {
            let err = service.set_local_credential("a", value).expect_err(label);
            assert!(matches!(err, DomainError::InvalidInput(_)), "{label}");
            assert!(!err.to_string().contains(FAKE_TOKEN), "{label}");
        }
        assert!(
            service.set_local_credential("e", FAKE_TOKEN).is_err(),
            "only a keychain config"
        );
        assert!(matches!(
            service.set_local_credential("nope", FAKE_TOKEN),
            Err(DomainError::NotFound(_))
        ));
        service
            .set_local_credential("a", &"x".repeat(4096))
            .expect("exactly the limit");
    }

    #[test]
    fn has_and_clear_follow_the_entry() {
        let service = local_service(Arc::default(), env_reader(&[]));
        service.save(keychain_config("a")).expect("save");
        service.save(env_config("e", "X")).expect("save env");
        assert!(!service.has_local_credential("a").expect("has"));
        service.set_local_credential("a", FAKE_TOKEN).expect("set");
        assert!(service.has_local_credential("a").expect("has"));
        service.clear_local_credential("a").expect("clear");
        assert!(!service.has_local_credential("a").expect("has"));
        assert!(!service
            .has_local_credential("e")
            .expect("not a keychain config"));
    }

    #[test]
    fn deleting_or_switching_away_removes_the_keychain_entry() {
        let store = Arc::new(MapStore::default());
        let service = local_service(Arc::clone(&store), env_reader(&[]));
        service.save(keychain_config("a")).expect("save");
        service.set_local_credential("a", FAKE_TOKEN).expect("set");
        service.delete("a").expect("delete");
        assert!(
            store.entries.lock().expect("lock").is_empty(),
            "delete clears the entry"
        );

        service.save(keychain_config("b")).expect("save");
        service.set_local_credential("b", FAKE_TOKEN).expect("set");
        service
            .save(env_config("b", "MY_TOKEN"))
            .expect("switch to env");
        assert!(
            store.entries.lock().expect("lock").is_empty(),
            "switching clears the entry"
        );
    }

    #[test]
    fn save_validates_the_source_fields() {
        let service = local_service(Arc::default(), env_reader(&[]));
        assert!(service.save(env_config("a", "")).is_err());
        assert!(service.save(env_config("a", "sk-ant-oat01-abc")).is_err());
        let mut vault = sample_config("a", "conn-1");
        vault.vault_secret_id = String::new();
        assert!(service.save(vault).is_err());
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
    fn save_validates_env_var_names_and_hides_values() {
        use rocket_acp::EnvPair;
        let service = service_with(Ok(None), true);
        let pair = |n: &str| EnvPair {
            name: n.to_string(),
            value: "top-secret-value".to_string(),
        };
        let cases: Vec<(&str, Vec<EnvPair>)> = vec![
            ("leading digit", vec![pair("1A")]),
            ("equals sign", vec![pair("A=B")]),
            ("space", vec![pair("A B")]),
            ("empty", vec![pair("")]),
            ("reserved", vec![pair("CLAUDE_CONFIG_DIR")]),
            ("credential clash", vec![pair("ANTHROPIC_API_KEY")]),
            ("duplicate", vec![pair("A"), pair("A")]),
            ("too long", vec![pair(&"A".repeat(129))]),
        ];
        for (label, extra) in cases {
            let mut config = sample_config("agent-1", "conn-1");
            config.extra_env = extra;
            let err = service.save(config).expect_err(label);
            assert!(matches!(err, DomainError::InvalidInput(_)), "{label}");
            assert!(!err.to_string().contains("top-secret-value"), "{label}");
        }
        let mut bad_credential = sample_config("agent-1", "conn-1");
        bad_credential.credential_env_var = "CLAUDE_CONFIG_DIR".to_string();
        assert!(service.save(bad_credential).is_err());
        let mut good = sample_config("agent-1", "conn-1");
        good.extra_env = vec![pair("HTTPS_PROXY"), pair("NO_PROXY")];
        service.save(good).expect("valid extra_env saves");
        assert_eq!(service.list().expect("list")[0].extra_env.len(), 2);
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
            Arc::new(MapStore::default()),
            env_reader(&[]),
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
