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

    /// Return the hash of the most recently appended event, if any.
    ///
    /// A log written by a newer build can end with an event kind this build does not
    /// know. Such an event cannot be parsed, but the next event must still chain to
    /// its hash, so an implementation should read the hash without parsing the kind.
    fn head_hash(&self) -> DomainResult<Option<String>> {
        Ok(self.latest()?.map(|e| e.hash))
    }
}
