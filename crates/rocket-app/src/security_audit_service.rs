use rocket_audit::{
    chain::hash_event,
    event::{AuditEventKind, SecurityAuditEvent},
    profile::{ComplianceProfile, EnforcementLevel, ProfileRepository},
    repository::AuditLogRepository,
};
use rocket_shared::error::{DomainError, DomainResult};
use std::sync::{Arc, Mutex};

pub struct SecurityAuditService {
    log: Arc<dyn AuditLogRepository>,
    profile_repo: Arc<dyn ProfileRepository>,
    /// Cached head hash to avoid re-reading the whole log for each append.
    head: Mutex<Option<String>>,
}

impl SecurityAuditService {
    pub fn new(
        log: Arc<dyn AuditLogRepository>,
        profile_repo: Arc<dyn ProfileRepository>,
    ) -> DomainResult<Self> {
        let latest = log.latest()?;
        Ok(Self {
            log,
            profile_repo,
            head: Mutex::new(latest.map(|e| e.hash)),
        })
    }

    /// Records a security event. Returns `Ok(None)` when the current profile
    /// mutes this kind, `Ok(Some(event))` when recorded, `Err(DomainError::InvalidInput(..))`
    /// under `EnforcementLevel::Block` so the caller can abort.
    pub fn record(
        &self,
        actor: String,
        workspace_id: Option<String>,
        kind: AuditEventKind,
    ) -> DomainResult<Option<SecurityAuditEvent>> {
        let profile = self.profile_repo.load()?;
        if !profile.records(kind.tag()) {
            return Ok(None);
        }

        let prev_hash = {
            let guard = self.head.lock().expect("head mutex poisoned");
            guard.clone().unwrap_or_default()
        };

        let mut event = SecurityAuditEvent::new(actor, workspace_id, kind, prev_hash);
        event.hash = hash_event(&event);
        self.log.append(&event)?;

        {
            let mut guard = self.head.lock().expect("head mutex poisoned");
            *guard = Some(event.hash.clone());
        }

        if profile.enforcement == EnforcementLevel::Block {
            return Err(DomainError::InvalidInput(format!(
                "blocked by compliance profile: {}",
                event.event.tag()
            )));
        }

        Ok(Some(event))
    }

    pub fn load_profile(&self) -> DomainResult<ComplianceProfile> {
        self.profile_repo.load()
    }

    pub fn save_profile(&self, profile: &ComplianceProfile) -> DomainResult<()> {
        self.profile_repo.save(profile)
    }

    pub fn list(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
        self.log.load_all()
    }

    pub fn list_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> DomainResult<Vec<SecurityAuditEvent>> {
        self.log.load_range(start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_audit::{
        chain::{verify_chain, ChainVerification},
        profile::default_profile,
    };

    struct MemLog {
        events: std::sync::Mutex<Vec<SecurityAuditEvent>>,
    }

    impl AuditLogRepository for MemLog {
        fn append(&self, event: &SecurityAuditEvent) -> DomainResult<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
        fn load_all(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
            Ok(self.events.lock().unwrap().clone())
        }
        fn load_range(
            &self,
            start: chrono::DateTime<chrono::Utc>,
            end: chrono::DateTime<chrono::Utc>,
        ) -> DomainResult<Vec<SecurityAuditEvent>> {
            Ok(self
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.occurred_at >= start && e.occurred_at <= end)
                .cloned()
                .collect())
        }
        fn latest(&self) -> DomainResult<Option<SecurityAuditEvent>> {
            Ok(self.events.lock().unwrap().last().cloned())
        }
    }

    struct MemProfile {
        p: Mutex<ComplianceProfile>,
    }

    impl ProfileRepository for MemProfile {
        fn load(&self) -> DomainResult<ComplianceProfile> {
            Ok(self.p.lock().unwrap().clone())
        }
        fn save(&self, profile: &ComplianceProfile) -> DomainResult<()> {
            *self.p.lock().unwrap() = profile.clone();
            Ok(())
        }
    }

    fn svc() -> SecurityAuditService {
        let log = Arc::new(MemLog { events: std::sync::Mutex::new(vec![]) });
        let profile = Arc::new(MemProfile { p: Mutex::new(default_profile()) });
        SecurityAuditService::new(log, profile).unwrap()
    }

    #[test]
    fn record_appends_and_chains() {
        let s = svc();
        s.record(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
        )
        .unwrap();
        s.record(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: "y".into() },
        )
        .unwrap();
        let events = s.list().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].prev_hash, events[0].hash);
        assert_eq!(verify_chain(&events), ChainVerification::Ok);
    }

    #[test]
    fn record_skips_muted_kinds() {
        let s = svc();
        let mut p = default_profile();
        p.muted_kinds.insert("collection_deleted".into());
        s.save_profile(&p).unwrap();
        let result = s
            .record(
                "a".into(),
                None,
                AuditEventKind::CollectionDeleted { collection: "x".into() },
            )
            .unwrap();
        assert!(result.is_none());
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn block_enforcement_errors_after_recording() {
        let s = svc();
        let mut p = default_profile();
        p.enforcement = EnforcementLevel::Block;
        s.save_profile(&p).unwrap();
        let result = s.record(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted { collection: "x".into() },
        );
        assert!(matches!(result, Err(DomainError::InvalidInput(_))));
        // Event was still recorded — this is the audit trail.
        assert_eq!(s.list().unwrap().len(), 1);
    }
}
