//! Shared audit/event-publishing logic for environment writes. Extracted so that
//! both a manual environment save (`EnvironmentService::save`) and a script-driven
//! write (`RequestExecutionService::apply_env_writes`) produce an identical audit
//! trail — see docs/superpowers/specs/2026-09-16-env-var-write-audit-spec.md §3.2.

use rocket_audit::{event::AuditEventKind, publisher::SecurityAuditPublisher};
use rocket_environment::Environment;
use rocket_shared::events::{DomainEvent, EventPublisher};

/// Publishes `DomainEvent::EnvironmentSaved` unconditionally, then one
/// `AuditEventKind::SecretVariableWritten` per secret variable in `after` whose
/// value changed (or is new) relative to `before`. Non-secret variables and
/// empty-valued secrets never emit the audit event.
pub fn publish_env_write_events(
    events: &dyn EventPublisher,
    audit: &dyn SecurityAuditPublisher,
    env_name: &str,
    before: &Environment,
    after: &Environment,
) {
    events.publish(DomainEvent::EnvironmentSaved { name: env_name.to_string() });

    for var in &after.variables {
        if !var.secret || var.value.is_empty() {
            continue;
        }
        let changed = before
            .variables
            .iter()
            .find(|v| v.key == var.key)
            .map(|v| v.value != var.value || !v.secret)
            .unwrap_or(true);
        if changed {
            audit.publish(
                "system".into(),
                None,
                AuditEventKind::SecretVariableWritten {
                    environment: env_name.to_string(),
                    variable_key: var.key.clone(),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::Variable;
    use std::sync::Mutex;

    struct RecordingPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }
    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: DomainEvent) {
            self.events.lock().expect("lock").push(event);
        }
    }

    struct CapturingAuditPublisher {
        captured: Mutex<Vec<AuditEventKind>>,
    }
    impl SecurityAuditPublisher for CapturingAuditPublisher {
        fn publish(&self, _actor: String, _workspace_id: Option<String>, kind: AuditEventKind) {
            self.captured.lock().expect("lock").push(kind);
        }
    }

    #[test]
    fn publishes_environment_saved_unconditionally() {
        let events = RecordingPublisher { events: Mutex::new(vec![]) };
        let audit = CapturingAuditPublisher { captured: Mutex::new(vec![]) };
        let before = Environment::new("prod");
        let after = Environment::new("prod");

        publish_env_write_events(&events, &audit, "prod", &before, &after);

        let published = events.events.lock().expect("lock");
        assert!(published
            .iter()
            .any(|e| matches!(e, DomainEvent::EnvironmentSaved { name } if name == "prod")));
    }

    #[test]
    fn publishes_secret_variable_written_for_new_secret() {
        let events = RecordingPublisher { events: Mutex::new(vec![]) };
        let audit = CapturingAuditPublisher { captured: Mutex::new(vec![]) };
        let before = Environment::new("prod");
        let mut after = Environment::new("prod");
        after.set_variable(Variable::secret("API_KEY", "sk-12345"));

        publish_env_write_events(&events, &audit, "prod", &before, &after);

        let captured = audit.captured.lock().expect("lock");
        assert!(captured.iter().any(|k| matches!(
            k,
            AuditEventKind::SecretVariableWritten { environment, variable_key }
                if environment == "prod" && variable_key == "API_KEY"
        )));
    }

    #[test]
    fn does_not_publish_secret_variable_written_for_non_secret() {
        let events = RecordingPublisher { events: Mutex::new(vec![]) };
        let audit = CapturingAuditPublisher { captured: Mutex::new(vec![]) };
        let before = Environment::new("prod");
        let mut after = Environment::new("prod");
        after.set_variable(Variable::new("HOST", "api.example.com"));

        publish_env_write_events(&events, &audit, "prod", &before, &after);

        let captured = audit.captured.lock().expect("lock");
        assert!(captured.is_empty());
    }

    #[test]
    fn does_not_publish_secret_variable_written_when_value_unchanged() {
        let events = RecordingPublisher { events: Mutex::new(vec![]) };
        let audit = CapturingAuditPublisher { captured: Mutex::new(vec![]) };
        let mut before = Environment::new("prod");
        before.set_variable(Variable::secret("API_KEY", "sk-12345"));
        let after = before.clone();

        publish_env_write_events(&events, &audit, "prod", &before, &after);

        let captured = audit.captured.lock().expect("lock");
        assert!(captured.is_empty());
    }
}
