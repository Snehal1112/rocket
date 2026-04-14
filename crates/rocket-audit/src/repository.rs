use crate::event::SecurityAuditEvent;
use chrono::{DateTime, Utc};
use rocket_shared::error::DomainResult;

pub trait AuditLogRepository: Send + Sync {
    /// Append a sealed event (hash already populated) to the log.
    fn append(&self, event: &SecurityAuditEvent) -> DomainResult<()>;

    /// Load every event in chronological order.
    fn load_all(&self) -> DomainResult<Vec<SecurityAuditEvent>>;

    /// Load events whose `occurred_at` falls within [start, end].
    fn load_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DomainResult<Vec<SecurityAuditEvent>>;

    /// Return the most recently appended event, if any.
    fn latest(&self) -> DomainResult<Option<SecurityAuditEvent>>;
}
