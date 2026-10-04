use rocket_audit::publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher};
use rocket_environment::{Environment, EnvironmentRepository};
use rocket_shared::error::DomainResult;
use rocket_shared::events::{DomainEvent, EventPublisher};
use std::sync::Arc;

pub struct EnvironmentService {
    repo: Box<dyn EnvironmentRepository>,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
}

impl EnvironmentService {
    pub fn new(repo: Box<dyn EnvironmentRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self {
            repo,
            events,
            audit: Arc::new(NullSecurityAuditPublisher),
        }
    }

    pub fn new_with_audit(
        repo: Box<dyn EnvironmentRepository>,
        events: Box<dyn EventPublisher>,
        audit: Arc<dyn SecurityAuditPublisher>,
    ) -> Self {
        Self {
            repo,
            events,
            audit,
        }
    }

    pub fn list(&self) -> DomainResult<Vec<Environment>> {
        self.repo.list()
    }

    pub fn get(&self, name: &str) -> DomainResult<Environment> {
        self.repo.get(name)
    }

    pub fn save(&self, env: &Environment) -> DomainResult<()> {
        rocket_environment::external_secret::validate_external_secret_bindings(
            &env.external_secrets,
        )?;
        rocket_environment::validate_client_certificates(
            &env.client_certificates,
            &env.external_secrets,
        )?;
        // Snapshot previous state so we can detect which secret values actually changed.
        let previous = self.repo.get(&env.name).ok();
        self.repo.save(env)?;

        // `before` is an empty environment of the same name when there was no prior
        // save — `publish_env_write_events` treats every secret in `after` as "changed"
        // in that case, matching the pre-refactor behavior (`previous: None` branch).
        let before = previous.unwrap_or_else(|| Environment::new(env.name.as_str()));
        crate::env_audit::publish_env_write_events(
            self.events.as_ref(),
            self.audit.as_ref(),
            &env.name,
            &before,
            env,
        );

        Ok(())
    }

    /// Like `save`, but also rejects a `vault` client certificate whose
    /// binding points at a provider that cannot supply certificates. The
    /// lookup is passed in because this service holds no connections.
    pub fn save_with_capabilities(
        &self,
        env: &Environment,
        lookup: &dyn rocket_environment::ProviderCapabilityLookup,
    ) -> DomainResult<()> {
        rocket_environment::validate_vault_certificate_providers(
            &env.client_certificates,
            &env.external_secrets,
            lookup,
        )?;
        self.save(env)
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.events.publish(DomainEvent::EnvironmentDeleted {
            name: name.to_string(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_audit::event::AuditEventKind;
    use rocket_environment::Variable;
    use rocket_environment::{ExternalSecretBinding, ExternalSecretRef};
    use rocket_shared::certificate::{ClientCertificate, VaultCertificateFormat};
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;

    struct MockEnvRepo {
        envs: Mutex<Vec<Environment>>,
    }

    impl MockEnvRepo {
        fn new() -> Self {
            Self {
                envs: Mutex::new(Vec::new()),
            }
        }
    }

    impl EnvironmentRepository for MockEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.envs.lock().unwrap().clone())
        }

        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.envs
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }

        fn save(&self, env: &Environment) -> DomainResult<()> {
            let mut envs = self.envs.lock().unwrap();
            if let Some(existing) = envs.iter_mut().find(|e| e.name == env.name) {
                *existing = env.clone();
            } else {
                envs.push(env.clone());
            }
            Ok(())
        }

        fn delete(&self, name: &str) -> DomainResult<()> {
            self.envs.lock().unwrap().retain(|e| e.name != name);
            Ok(())
        }
    }

    struct CapturingPublisher {
        captured: Mutex<Vec<AuditEventKind>>,
    }
    impl SecurityAuditPublisher for CapturingPublisher {
        fn publish(&self, _actor: String, _workspace_id: Option<String>, kind: AuditEventKind) {
            self.captured.lock().unwrap().push(kind);
        }
    }

    fn make_service() -> EnvironmentService {
        EnvironmentService::new(Box::new(MockEnvRepo::new()), Box::new(NullEventPublisher))
    }

    use rocket_environment::{
        ConnectionProvider, ProviderCapabilities, ProviderCapabilityLookup, SecretProviderKind,
    };

    struct FixedLookup(Option<ConnectionProvider>);
    impl ProviderCapabilityLookup for FixedLookup {
        fn provider_of(&self, _id: &str) -> DomainResult<Option<ConnectionProvider>> {
            Ok(self.0.clone())
        }
    }

    fn env_with_vault_certificate() -> Environment {
        let mut env = Environment::new("prod");
        env.external_secrets.push(ExternalSecretBinding {
            alias: "vault".to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "v".to_string(),
            secret_names: vec![],
        });
        env.client_certificates.push(ClientCertificate::Vault {
            domain: "api.example.com".to_string(),
            binding: "vault".to_string(),
            certificate: "client".to_string(),
            format: VaultCertificateFormat::default(),
        });
        env
    }

    #[test]
    fn save_with_capabilities_rejects_a_vault_certificate_on_azure() {
        let svc = make_service();
        let lookup = FixedLookup(Some(ConnectionProvider {
            kind: SecretProviderKind::Azure,
            capabilities: ProviderCapabilities::default(),
        }));

        let err = svc
            .save_with_capabilities(&env_with_vault_certificate(), &lookup)
            .expect_err("azure cannot supply certificates");

        assert!(err.to_string().contains("Azure Key Vault"), "got: {err}");
    }

    #[test]
    fn save_with_capabilities_saves_when_the_provider_supports_certificates() {
        let svc = make_service();
        let lookup = FixedLookup(Some(ConnectionProvider {
            kind: SecretProviderKind::RocketVault,
            capabilities: ProviderCapabilities {
                certificates: true,
                ..ProviderCapabilities::default()
            },
        }));

        svc.save_with_capabilities(&env_with_vault_certificate(), &lookup)
            .expect("rocketvault supplies certificates");
    }

    #[test]
    fn save_with_capabilities_saves_when_the_connection_was_deleted() {
        let svc = make_service();
        svc.save_with_capabilities(&env_with_vault_certificate(), &FixedLookup(None))
            .expect("a deleted connection is not a save error");
    }

    #[test]
    fn save_and_list() {
        let svc = make_service();
        svc.save(&Environment::new("production")).unwrap();
        let list = svc.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "production");
    }

    #[test]
    fn get_by_name() {
        let svc = make_service();
        svc.save(&Environment::new("staging")).unwrap();
        let env = svc.get("staging").unwrap();
        assert_eq!(env.name, "staging");
    }

    #[test]
    fn delete_removes_environment() {
        let svc = make_service();
        svc.save(&Environment::new("temp")).unwrap();
        svc.delete("temp").unwrap();
        assert!(svc.list().unwrap().is_empty());
    }

    #[test]
    fn save_emits_security_audit_event() {
        let publisher = Arc::new(CapturingPublisher {
            captured: Mutex::new(vec![]),
        });
        let svc = EnvironmentService::new_with_audit(
            Box::new(MockEnvRepo::new()),
            Box::new(NullEventPublisher),
            publisher.clone(),
        );

        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-12345"));
        env.set_variable(Variable::new("HOST", "api.example.com"));
        svc.save(&env).unwrap();

        let captured = publisher.captured.lock().unwrap();
        assert!(
            captured.iter().any(|k| matches!(
                k,
                AuditEventKind::SecretVariableWritten { environment, variable_key }
                    if environment == "prod" && variable_key == "API_KEY"
            )),
            "expected SecretVariableWritten for API_KEY, got {:?}",
            *captured
        );
        // Non-secret variables must not emit the event.
        assert!(
            !captured
                .iter()
                .any(|k| matches!(k, AuditEventKind::SecretVariableWritten { variable_key, .. } if variable_key == "HOST")),
            "non-secret variables must not emit SecretVariableWritten"
        );
    }

    // Review Focus 5.
    #[test]
    fn save_rejects_pasted_key_text_in_a_certificate() {
        let svc = make_service();
        let mut env = Environment::new("prod");
        env.client_certificates = vec![ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: "/certs/client.pem".into(),
            private_key_file_path:
                "-----BEGIN PRIVATE KEY-----\nMIIEvQsecret\n-----END PRIVATE KEY-----".into(),
            certificate_secret: None,
            private_key_secret: None,
            passphrase: None,
        }];
        let err = svc
            .save(&env)
            .expect_err("pasted key text must be rejected");
        assert!(err.to_string().contains("privateKeyFilePath"), "{err}");
        assert!(
            svc.list().expect("list").is_empty(),
            "nothing may be written"
        );
    }

    #[test]
    fn save_accepts_a_certificate_that_references_a_bound_secret() {
        let svc = make_service();
        let mut env = Environment::new("prod");
        env.external_secrets = vec![ExternalSecretBinding {
            alias: "vault".into(),
            connection_id: "conn-1".into(),
            vault_name: "prod-vault".into(),
            secret_names: vec![
                ExternalSecretRef {
                    name: "clientCertPem".into(),
                    secret_id: "id-1".into(),
                },
                ExternalSecretRef {
                    name: "clientKeyPem".into(),
                    secret_id: "id-2".into(),
                },
            ],
        }];
        env.client_certificates = vec![ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: Some("{{vault.clientKeyPass}}".into()),
        }];
        svc.save(&env).expect("a bound reference is valid");
        assert_eq!(svc.get("prod").expect("saved").client_certificates.len(), 1);
    }

    #[test]
    fn save_rejects_a_vault_certificate_whose_binding_is_missing() {
        let svc = make_service();
        let mut env = Environment::new("prod");
        env.client_certificates = vec![ClientCertificate::Vault {
            domain: "api.example.com".into(),
            binding: "prod".into(),
            certificate: "client-a".into(),
            format: VaultCertificateFormat::Pem,
        }];
        let err = svc
            .save(&env)
            .expect_err("a vault entry needs a bound alias");
        assert!(err.to_string().contains("binding prod"), "{err}");
        assert!(
            svc.list().expect("list").is_empty(),
            "nothing may be written"
        );
    }
}
