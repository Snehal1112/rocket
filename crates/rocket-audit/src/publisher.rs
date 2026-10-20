use crate::event::AuditEventKind;

/// Non-failing publisher for security events. Services call this fire-and-forget;
/// a failing write must never break the caller's operation.
pub trait SecurityAuditPublisher: Send + Sync {
    fn publish(&self, actor: String, workspace_id: Option<String>, kind: AuditEventKind);
}

pub struct NullSecurityAuditPublisher;

impl SecurityAuditPublisher for NullSecurityAuditPublisher {
    fn publish(&self, _actor: String, _workspace_id: Option<String>, _kind: AuditEventKind) {
        // Intentionally empty for tests.
    }
}

/// One call recorded by `RecordingSecurityAuditPublisher`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedAudit {
    pub actor: String,
    pub workspace_id: Option<String>,
    pub kind: AuditEventKind,
}

/// Test double that keeps every published event with its actor and workspace id.
#[derive(Default)]
pub struct RecordingSecurityAuditPublisher {
    records: std::sync::Mutex<Vec<RecordedAudit>>,
}

impl RecordingSecurityAuditPublisher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn records(&self) -> Vec<RecordedAudit> {
        self.records.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

impl SecurityAuditPublisher for RecordingSecurityAuditPublisher {
    fn publish(&self, actor: String, workspace_id: Option<String>, kind: AuditEventKind) {
        if let Ok(mut records) = self.records.lock() {
            records.push(RecordedAudit {
                actor,
                workspace_id,
                kind,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_publisher_does_not_panic() {
        let p = NullSecurityAuditPublisher;
        p.publish(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted {
                collection: "x".into(),
            },
        );
    }
}
