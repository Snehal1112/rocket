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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_publisher_does_not_panic() {
        let p = NullSecurityAuditPublisher;
        p.publish(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
        );
    }
}
