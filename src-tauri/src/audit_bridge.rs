use std::sync::Arc;

use rocket_app::SecurityAuditService;
use rocket_audit::event::AuditEventKind;
use rocket_audit::publisher::SecurityAuditPublisher;

/// Adapts `SecurityAuditService::record` to the fire-and-forget
/// `SecurityAuditPublisher` contract. A failing record is logged but never
/// propagated to the caller.
pub struct ServiceBackedAuditPublisher {
    svc: Arc<SecurityAuditService>,
}

impl ServiceBackedAuditPublisher {
    pub fn new(svc: Arc<SecurityAuditService>) -> Self {
        Self { svc }
    }
}

impl SecurityAuditPublisher for ServiceBackedAuditPublisher {
    fn publish(&self, actor: String, workspace_id: Option<String>, kind: AuditEventKind) {
        if let Err(e) = self.svc.record(actor, workspace_id, kind) {
            tracing::warn!(error = %e, "audit record failed");
        }
    }
}
