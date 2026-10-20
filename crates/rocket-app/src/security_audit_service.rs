use rocket_audit::{
    actor::AuditActor,
    chain::{hash_event, verify_chain, ChainVerification},
    event::{AuditEventKind, SecurityAuditEvent},
    profile::{ComplianceProfile, EnforcementLevel, ProfileRepository},
    repository::AuditLogRepository,
};
use rocket_shared::error::{DomainError, DomainResult};
use std::sync::{Arc, Mutex};

/// Returns the id of the workspace that is active right now, if known.
pub type WorkspaceIdResolver = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// Events of an evidence export plus whether the whole log chain verified.
#[derive(Debug, Clone)]
pub struct EvidenceBundle {
    pub events: Vec<SecurityAuditEvent>,
    pub chain_verified: bool,
}

pub struct SecurityAuditService {
    log: Arc<dyn AuditLogRepository>,
    profile_repo: Arc<dyn ProfileRepository>,
    /// Cached head hash to avoid re-reading the whole log for each append.
    head: Mutex<Option<String>>,
    /// Fills the workspace id of an event that arrives without one.
    workspace_resolver: Option<WorkspaceIdResolver>,
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
            workspace_resolver: None,
        })
    }

    /// Attaches the source of the active workspace id, read at record time.
    pub fn with_workspace_resolver(mut self, resolver: WorkspaceIdResolver) -> Self {
        self.workspace_resolver = Some(resolver);
        self
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

        let workspace_id =
            workspace_id.or_else(|| self.workspace_resolver.as_ref().and_then(|r| r()));

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

    /// Verifies the whole log. A break is recorded once as `AuditChainBroken`
    /// (keyed by the broken event id), appended after the current head, so the
    /// original break stays the first one found and later checks do not add more.
    /// A failing record never changes the verification result.
    pub fn verify_log(&self, actor: AuditActor) -> DomainResult<ChainVerification> {
        let events = self.log.load_all()?;
        let result = verify_chain(&events);
        if let ChainVerification::Broken {
            index,
            expected,
            actual,
        } = &result
        {
            if let Some(broken) = events.get(*index) {
                let already = events.iter().any(|e| {
                    matches!(&e.event, AuditEventKind::AuditChainBroken { at_event_id, .. }
                        if *at_event_id == broken.id)
                });
                if !already {
                    let kind = AuditEventKind::AuditChainBroken {
                        at_event_id: broken.id,
                        expected_hash: expected.clone(),
                        actual_hash: actual.clone(),
                    };
                    if let Err(e) = self.record(actor.into(), None, kind) {
                        tracing::warn!(error = %e, "could not record a broken audit chain");
                    }
                }
            }
        }
        Ok(result)
    }

    /// Reads the events of a range for an evidence export. The chain is checked
    /// on the whole log, because a slice cannot start at the genesis hash. Only a
    /// successful read records `AuditEvidenceExported` (events carry no payload copy).
    pub fn export_evidence(
        &self,
        actor: AuditActor,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> DomainResult<EvidenceBundle> {
        let chain_verified = self.verify_log(actor)? == ChainVerification::Ok;
        let events = self.log.load_range(start, end)?;
        let kind = AuditEventKind::AuditEvidenceExported {
            range_start: start,
            range_end: end,
            count: events.len(),
        };
        if let Err(e) = self.record(actor.into(), None, kind) {
            tracing::warn!(error = %e, "could not record an evidence export");
        }
        Ok(EvidenceBundle {
            events,
            chain_verified,
        })
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

    fn svc_with_log() -> (SecurityAuditService, Arc<MemLog>) {
        let log = Arc::new(MemLog {
            events: std::sync::Mutex::new(vec![]),
        });
        let profile = Arc::new(MemProfile {
            p: Mutex::new(default_profile()),
        });
        (
            SecurityAuditService::new(log.clone(), profile).unwrap(),
            log,
        )
    }

    fn del(name: &str) -> AuditEventKind {
        AuditEventKind::CollectionDeleted {
            collection: name.into(),
        }
    }

    #[test]
    fn record_fills_the_workspace_id_from_the_resolver() {
        let (s, _) = svc_with_log();
        let s = s.with_workspace_resolver(Arc::new(|| Some("ws-1".to_string())));
        s.record("user".into(), None, del("x")).unwrap();
        s.record("user".into(), Some("explicit".into()), del("y"))
            .unwrap();
        let events = s.list().unwrap();
        assert_eq!(events[0].workspace_id.as_deref(), Some("ws-1"));
        assert_eq!(events[1].workspace_id.as_deref(), Some("explicit"));
        assert_eq!(verify_chain(&events), ChainVerification::Ok);
    }

    #[test]
    fn record_without_a_resolver_keeps_the_workspace_id_empty() {
        let s = svc();
        s.record("user".into(), None, del("x")).unwrap();
        assert_eq!(s.list().unwrap()[0].workspace_id, None);
    }

    #[test]
    fn verify_log_on_a_good_chain_records_nothing() {
        let (s, _) = svc_with_log();
        s.record("user".into(), None, del("x")).unwrap();
        assert_eq!(
            s.verify_log(AuditActor::User).unwrap(),
            ChainVerification::Ok
        );
        assert_eq!(s.list().unwrap().len(), 1);
    }

    #[test]
    fn verify_log_records_a_break_once_and_keeps_reporting_it() {
        let (s, log) = svc_with_log();
        s.record("user".into(), None, del("x")).unwrap();
        s.record("user".into(), None, del("y")).unwrap();
        let broken_id = {
            let mut events = log.events.lock().unwrap();
            events[0].actor = "attacker".into();
            events[0].id
        };

        let first = s.verify_log(AuditActor::System).unwrap();
        assert!(matches!(first, ChainVerification::Broken { index: 0, .. }));
        let events = s.list().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].actor, "system");
        assert!(matches!(
            &events[2].event,
            AuditEventKind::AuditChainBroken { at_event_id, .. } if *at_event_id == broken_id
        ));

        // A second check still reports the break and adds no second event.
        let second = s.verify_log(AuditActor::System).unwrap();
        assert!(matches!(second, ChainVerification::Broken { index: 0, .. }));
        assert_eq!(s.list().unwrap().len(), 3);
    }

    #[test]
    fn export_evidence_records_the_export_with_the_count() {
        let (s, _) = svc_with_log();
        s.record("user".into(), None, del("x")).unwrap();
        s.record("user".into(), None, del("y")).unwrap();
        let start = chrono::Utc::now() - chrono::Duration::hours(1);
        let end = chrono::Utc::now() + chrono::Duration::hours(1);
        let bundle = s.export_evidence(AuditActor::User, start, end).unwrap();
        assert_eq!(bundle.events.len(), 2);
        assert!(bundle.chain_verified);
        let events = s.list().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[2].actor, "user");
        assert!(matches!(
            &events[2].event,
            AuditEventKind::AuditEvidenceExported { count: 2, range_start, range_end }
                if *range_start == start && *range_end == end
        ));
    }

    #[test]
    fn export_evidence_of_a_slice_still_verifies_the_whole_chain() {
        let (s, _) = svc_with_log();
        s.record("user".into(), None, del("x")).unwrap();
        let start = chrono::Utc::now() + chrono::Duration::hours(1);
        let end = start + chrono::Duration::hours(1);
        let bundle = s.export_evidence(AuditActor::User, start, end).unwrap();
        assert!(bundle.events.is_empty());
        assert!(bundle.chain_verified);
    }

    #[test]
    fn export_evidence_records_nothing_when_the_read_fails() {
        struct FailingRange(MemLog);
        impl AuditLogRepository for FailingRange {
            fn append(&self, e: &SecurityAuditEvent) -> DomainResult<()> {
                self.0.append(e)
            }
            fn load_all(&self) -> DomainResult<Vec<SecurityAuditEvent>> {
                self.0.load_all()
            }
            fn load_range(
                &self,
                _: chrono::DateTime<chrono::Utc>,
                _: chrono::DateTime<chrono::Utc>,
            ) -> DomainResult<Vec<SecurityAuditEvent>> {
                Err(DomainError::Internal("read failed".into()))
            }
            fn latest(&self) -> DomainResult<Option<SecurityAuditEvent>> {
                self.0.latest()
            }
        }
        let log = Arc::new(FailingRange(MemLog {
            events: std::sync::Mutex::new(vec![]),
        }));
        let profile = Arc::new(MemProfile {
            p: Mutex::new(default_profile()),
        });
        let s = SecurityAuditService::new(log.clone(), profile).unwrap();
        s.record("user".into(), None, del("x")).unwrap();
        let now = chrono::Utc::now();
        assert!(s.export_evidence(AuditActor::User, now, now).is_err());
        assert_eq!(log.load_all().unwrap().len(), 1);
    }

    fn svc() -> SecurityAuditService {
        let log = Arc::new(MemLog {
            events: std::sync::Mutex::new(vec![]),
        });
        let profile = Arc::new(MemProfile {
            p: Mutex::new(default_profile()),
        });
        SecurityAuditService::new(log, profile).unwrap()
    }

    #[test]
    fn record_appends_and_chains() {
        let s = svc();
        s.record(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted {
                collection: "x".into(),
            },
        )
        .unwrap();
        s.record(
            "a".into(),
            None,
            AuditEventKind::CollectionDeleted {
                collection: "y".into(),
            },
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
                AuditEventKind::CollectionDeleted {
                    collection: "x".into(),
                },
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
            AuditEventKind::CollectionDeleted {
                collection: "x".into(),
            },
        );
        assert!(matches!(result, Err(DomainError::InvalidInput(_))));
        // Event was still recorded — this is the audit trail.
        assert_eq!(s.list().unwrap().len(), 1);
    }
}
