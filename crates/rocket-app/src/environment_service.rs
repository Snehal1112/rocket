use rocket_audit::{
    event::AuditEventKind,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
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
        Self { repo, events, audit }
    }

    pub fn list(&self) -> DomainResult<Vec<Environment>> {
        self.repo.list()
    }

    pub fn get(&self, name: &str) -> DomainResult<Environment> {
        self.repo.get(name)
    }

    pub fn save(&self, env: &Environment) -> DomainResult<()> {
        // Snapshot previous state so we can detect which secret values actually changed.
        let previous = self.repo.get(&env.name).ok();
        self.repo.save(env)?;
        self.events.publish(DomainEvent::EnvironmentSaved { name: env.name.clone() });

        // Emit one SecretVariableWritten per secret whose value changed (or is new).
        for var in &env.variables {
            if !var.secret || var.value.is_empty() {
                continue;
            }
            let changed = match &previous {
                Some(prev) => prev
                    .variables
                    .iter()
                    .find(|v| v.key == var.key)
                    .map(|v| v.value != var.value || !v.secret)
                    .unwrap_or(true),
                None => true,
            };
            if changed {
                self.audit.publish(
                    "system".into(),
                    None,
                    AuditEventKind::SecretVariableWritten {
                        environment: env.name.clone(),
                        variable_key: var.key.clone(),
                    },
                );
            }
        }

        Ok(())
    }

    pub fn delete(&self, name: &str) -> DomainResult<()> {
        self.repo.delete(name)?;
        self.events.publish(DomainEvent::EnvironmentDeleted { name: name.to_string() });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::Variable;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;

    struct MockEnvRepo {
        envs: Mutex<Vec<Environment>>,
    }

    impl MockEnvRepo {
        fn new() -> Self {
            Self { envs: Mutex::new(Vec::new()) }
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
        let publisher = Arc::new(CapturingPublisher { captured: Mutex::new(vec![]) });
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
}
