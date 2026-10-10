//! Collection trust: the effective capabilities of a collection, and the one write path
//! for grants.
//!
//! A collection file only requests elevated capabilities. The trust store holds what the
//! user allowed on this computer. Consumers call `effective_capabilities` and never read
//! the raw request fields.

use std::sync::Arc;

use rocket_collection::{
    normalize_root, request_fingerprint, resolve_effective, CollectionGrant, CollectionIdentity,
    CollectionRepository, CollectionTrustStore, EffectiveCapabilities, GrantSource,
    LegacyCollection, MigrationNoticeEntry, RequestedElevation,
};
use rocket_collection::settings::SandboxMode;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

/// Capability names used in the migration notice.
pub const NOTICE_DEVELOPER_MODE: &str = "developerMode";
pub const NOTICE_CONTEXT_ROOTS: &str = "contextRoots";
pub const NOTICE_AGENT_RUN: &str = "agentRun";

/// The effective capabilities of a collection. Any error (unknown collection, unreadable
/// identity or settings, corrupt store) gives `EffectiveCapabilities::untrusted()`.
pub fn effective_capabilities(
    repo: &dyn CollectionRepository,
    store: &dyn CollectionTrustStore,
    collection: &str,
) -> EffectiveCapabilities {
    let Ok(identity) = repo.collection_identity(collection) else {
        return EffectiveCapabilities::untrusted();
    };
    let Ok(settings) = repo.get_settings(collection) else {
        return EffectiveCapabilities::untrusted();
    };
    let Ok(grant) = store.grant_for(&identity) else {
        return EffectiveCapabilities::untrusted();
    };
    resolve_effective(&RequestedElevation::from_settings(&settings), grant.as_ref())
}

/// A capability a grant can be given or taken away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    DeveloperMode,
    ContextRoots,
    AgentRun,
    ProcessEnv,
}

/// Requested, granted and effective values of one collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustStatus {
    pub requested: RequestedElevation,
    pub grant: Option<CollectionGrant>,
    pub effective: EffectiveCapabilities,
    pub fingerprint: String,
    /// Set when the trust store could not be read.
    pub store_error: Option<String>,
}

pub struct CollectionTrustService {
    repo: Arc<dyn CollectionRepository>,
    store: Arc<dyn CollectionTrustStore>,
    events: Arc<dyn EventPublisher>,
}

impl CollectionTrustService {
    pub fn new(
        repo: Arc<dyn CollectionRepository>,
        store: Arc<dyn CollectionTrustStore>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            repo,
            store,
            events,
        }
    }

    pub fn effective(&self, collection: &str) -> EffectiveCapabilities {
        effective_capabilities(self.repo.as_ref(), self.store.as_ref(), collection)
    }

    pub fn status(&self, collection: &str) -> DomainResult<TrustStatus> {
        let identity = self.repo.collection_identity(collection)?;
        let requested = RequestedElevation::from_settings(&self.repo.get_settings(collection)?);
        let (grant, store_error) = match self.store.grant_for(&identity) {
            Ok(grant) => (grant, None),
            Err(e) => (None, Some(e.to_string())),
        };
        let effective = resolve_effective(&requested, grant.as_ref());
        Ok(TrustStatus {
            fingerprint: request_fingerprint(&requested),
            requested,
            grant,
            effective,
            store_error,
        })
    }

    /// Turns a capability on or off for this computer, and updates the collection file to
    /// match where the capability has a file field. On: grant first, then file. Off: revoke
    /// first, then file. Either order fails closed.
    pub fn set_capability(
        &self,
        collection: &str,
        capability: Capability,
        enabled: bool,
    ) -> DomainResult<()> {
        let identity = self.repo.collection_identity(collection)?;
        let mut grant = self.current_grant(&identity);
        match capability {
            Capability::DeveloperMode => grant.developer_mode = enabled,
            Capability::AgentRun => grant.agent_run = enabled,
            Capability::ProcessEnv => grant.process_env = enabled,
            Capability::ContextRoots => {
                return Err(DomainError::InvalidInput(
                    "context roots are set with set_context_roots".into(),
                ))
            }
        }
        self.store.put(&identity, grant)?;
        match capability {
            Capability::DeveloperMode | Capability::AgentRun => {
                let mut settings = self.repo.get_settings(collection)?;
                if capability == Capability::DeveloperMode {
                    settings.sandbox_mode = if enabled {
                        SandboxMode::Developer
                    } else {
                        SandboxMode::Safe
                    };
                } else {
                    settings.agent_autonomy_enabled = enabled;
                }
                self.repo.save_settings(collection, &settings)?;
                self.publish_settings_saved(collection);
            }
            _ => {}
        }
        self.publish_trust_changed(collection);
        Ok(())
    }

    /// Writes the roots to the collection file and approves exactly those roots.
    pub fn set_context_roots(&self, collection: &str, roots: Vec<String>) -> DomainResult<()> {
        let identity = self.repo.collection_identity(collection)?;
        let normalized = dedup_roots(&roots);
        let mut grant = self.current_grant(&identity);
        grant.context_roots = normalized.clone();
        self.store.put(&identity, grant)?;
        let mut settings = self.repo.get_settings(collection)?;
        settings.script_context_roots = normalized;
        self.repo.save_settings(collection, &settings)?;
        self.publish_settings_saved(collection);
        self.publish_trust_changed(collection);
        Ok(())
    }

    /// Approves what the collection file currently asks for, without changing the file.
    /// Refuses when the request changed since `expected_fingerprint` was shown.
    pub fn grant_requested(
        &self,
        collection: &str,
        capabilities: &[Capability],
        expected_fingerprint: &str,
    ) -> DomainResult<()> {
        let identity = self.repo.collection_identity(collection)?;
        let requested = RequestedElevation::from_settings(&self.repo.get_settings(collection)?);
        if request_fingerprint(&requested) != expected_fingerprint {
            return Err(DomainError::InvalidInput(
                "This collection's settings changed. Review them again.".into(),
            ));
        }
        let mut grant = self.current_grant(&identity);
        for capability in capabilities {
            match capability {
                Capability::DeveloperMode => grant.developer_mode = requested.developer_mode,
                Capability::ContextRoots => {
                    grant.context_roots = dedup_roots(&requested.context_roots);
                }
                Capability::AgentRun => grant.agent_run = requested.agent_run,
                Capability::ProcessEnv => grant.process_env = true,
            }
        }
        self.store.put(&identity, grant)?;
        self.publish_trust_changed(collection);
        Ok(())
    }

    /// Removes every grant of the collection. The collection file is not touched.
    pub fn revoke(&self, collection: &str) -> DomainResult<()> {
        let identity = self.repo.collection_identity(collection)?;
        self.store.remove(&identity)?;
        self.publish_trust_changed(collection);
        Ok(())
    }

    /// One-time grandfathering. Every collection found keeps what its file asks for today,
    /// plus host environment access. Does nothing once the store is marked as migrated.
    pub fn migrate_legacy(&self, found: Vec<LegacyCollection>) -> DomainResult<()> {
        if self.store.migrated()? {
            return Ok(());
        }
        let mut grants = Vec::new();
        let mut notice = Vec::new();
        for item in found {
            let req = &item.requested;
            let mut capabilities = Vec::new();
            if req.developer_mode {
                capabilities.push(NOTICE_DEVELOPER_MODE.to_string());
            }
            if !req.context_roots.is_empty() {
                capabilities.push(NOTICE_CONTEXT_ROOTS.to_string());
            }
            if req.agent_run {
                capabilities.push(NOTICE_AGENT_RUN.to_string());
            }
            if !capabilities.is_empty() {
                notice.push(MigrationNoticeEntry {
                    root: item.identity.canonical_root.to_string_lossy().into_owned(),
                    capabilities,
                });
            }
            grants.push((
                item.identity,
                CollectionGrant {
                    developer_mode: req.developer_mode,
                    context_roots: dedup_roots(&req.context_roots),
                    agent_run: req.agent_run,
                    process_env: true,
                    source: GrantSource::Migrated,
                },
            ));
        }
        self.store.complete_migration(grants, notice)
    }

    /// Collections that were grandfathered with elevated capabilities, until dismissed.
    pub fn migration_notice(&self) -> DomainResult<Vec<MigrationNoticeEntry>> {
        self.store.migration_notice()
    }

    pub fn dismiss_migration_notice(&self) -> DomainResult<()> {
        self.store.dismiss_migration_notice()
    }

    /// The grant to modify. An unreadable store starts from nothing, and the next write
    /// moves the bad file aside.
    fn current_grant(&self, identity: &CollectionIdentity) -> CollectionGrant {
        self.store
            .grant_for(identity)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn publish_trust_changed(&self, collection: &str) {
        self.events.publish(DomainEvent::CollectionTrustChanged {
            collection: collection.to_string(),
        });
    }

    fn publish_settings_saved(&self, collection: &str) {
        self.events.publish(DomainEvent::CollectionSettingsSaved {
            collection: collection.to_string(),
        });
    }
}

fn dedup_roots(roots: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for root in roots {
        let normalized = normalize_root(root);
        if !normalized.is_empty() && !out.contains(&normalized) {
            out.push(normalized);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{ConfigurableCollectionRepo, InMemoryTrustStore};
    use rocket_collection::{CollectionSettings, DenyAllTrustStore};
    use rocket_shared::events::NullEventPublisher;

    fn service(
        repo: &Arc<ConfigurableCollectionRepo>,
        store: &Arc<InMemoryTrustStore>,
    ) -> CollectionTrustService {
        CollectionTrustService::new(
            Arc::clone(repo) as Arc<dyn CollectionRepository>,
            Arc::clone(store) as Arc<dyn CollectionTrustStore>,
            Arc::new(NullEventPublisher),
        )
    }

    fn requesting_everything() -> CollectionSettings {
        CollectionSettings {
            sandbox_mode: SandboxMode::Developer,
            script_context_roots: vec!["../shared".into()],
            agent_autonomy_enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_requested_capability_without_a_grant_is_not_effective() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("c", requesting_everything());
        let store = InMemoryTrustStore::new();
        let eff = effective_capabilities(repo.as_ref(), store.as_ref(), "c");
        assert_eq!(eff, EffectiveCapabilities::untrusted());
    }

    #[test]
    fn a_deny_all_store_is_untrusted() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("c", requesting_everything());
        let eff = effective_capabilities(repo.as_ref(), &DenyAllTrustStore, "c");
        assert_eq!(eff, EffectiveCapabilities::untrusted());
    }

    #[test]
    fn set_capability_on_grants_and_writes_the_file_and_off_clears_both() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("c", CollectionSettings::default());
        let store = InMemoryTrustStore::new();
        let svc = service(&repo, &store);

        svc.set_capability("c", Capability::DeveloperMode, true).expect("on");
        assert!(store.grant_of("c").expect("grant").developer_mode);
        assert_eq!(
            repo.get_settings("c").expect("settings").sandbox_mode,
            SandboxMode::Developer
        );
        assert_eq!(svc.effective("c").sandbox_mode, SandboxMode::Developer);

        svc.set_capability("c", Capability::DeveloperMode, false).expect("off");
        assert!(!store.grant_of("c").expect("grant").developer_mode);
        assert_eq!(
            repo.get_settings("c").expect("settings").sandbox_mode,
            SandboxMode::Safe
        );
    }

    #[test]
    fn agent_run_and_process_env_capabilities() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("c", CollectionSettings::default());
        let store = InMemoryTrustStore::new();
        let svc = service(&repo, &store);

        svc.set_capability("c", Capability::AgentRun, true).expect("agent");
        assert!(repo.get_settings("c").expect("settings").agent_autonomy_enabled);
        assert!(svc.effective("c").agent_run);

        // Process env has no file field, so only the store changes.
        svc.set_capability("c", Capability::ProcessEnv, true).expect("env");
        assert!(svc.effective("c").process_env);
        assert!(svc.set_capability("c", Capability::ContextRoots, true).is_err());
    }

    #[test]
    fn set_context_roots_writes_the_file_and_approves_the_same_roots() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("c", CollectionSettings::default());
        let store = InMemoryTrustStore::new();
        let svc = service(&repo, &store);
        svc.set_context_roots("c", vec!["./a/".into(), "a".into(), "b".into()])
            .expect("roots");
        assert_eq!(
            repo.get_settings("c").expect("settings").script_context_roots,
            vec!["a", "b"]
        );
        assert_eq!(store.grant_of("c").expect("grant").context_roots, vec!["a", "b"]);
    }

    #[test]
    fn grant_requested_approves_the_request_and_refuses_a_stale_fingerprint() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("c", requesting_everything());
        let store = InMemoryTrustStore::new();
        let svc = service(&repo, &store);
        let fingerprint = svc.status("c").expect("status").fingerprint;

        svc.grant_requested("c", &[Capability::DeveloperMode], "stale")
            .expect_err("stale fingerprint");
        assert!(store.grant_of("c").is_none());

        svc.grant_requested("c", &[Capability::DeveloperMode, Capability::ContextRoots], &fingerprint)
            .expect("grant");
        let eff = svc.effective("c");
        assert_eq!(eff.sandbox_mode, SandboxMode::Developer);
        assert_eq!(eff.context_roots, vec!["../shared"]);
        // Agent run was requested but not approved.
        assert!(!eff.agent_run);
        // The file was not touched.
        assert_eq!(repo.get_settings("c").expect("settings"), requesting_everything());
    }

    #[test]
    fn revoke_removes_the_grant_and_leaves_the_file_alone() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings("c", requesting_everything());
        let store = InMemoryTrustStore::new();
        let svc = service(&repo, &store);
        svc.set_capability("c", Capability::AgentRun, true).expect("on");
        svc.revoke("c").expect("revoke");
        assert!(store.grant_of("c").is_none());
        assert_eq!(svc.effective("c"), EffectiveCapabilities::untrusted());
        assert!(repo.get_settings("c").expect("settings").agent_autonomy_enabled);
    }

    fn legacy(name: &str, requested: RequestedElevation) -> LegacyCollection {
        LegacyCollection {
            name: name.into(),
            identity: crate::test_doubles::test_identity(name),
            requested,
        }
    }

    #[test]
    fn migration_grandfathers_current_requests_once_and_fills_the_notice() {
        let repo = ConfigurableCollectionRepo::new();
        let store = InMemoryTrustStore::new();
        let svc = service(&repo, &store);
        let elevated = RequestedElevation {
            developer_mode: true,
            context_roots: vec!["../shared".into()],
            agent_run: false,
        };
        svc.migrate_legacy(vec![
            legacy("plain", RequestedElevation::default()),
            legacy("dev", elevated),
        ])
        .expect("migrate");

        let plain = store.grant_of("plain").expect("plain grant");
        assert!(plain.process_env && !plain.developer_mode);
        assert_eq!(plain.source, GrantSource::Migrated);
        let dev = store.grant_of("dev").expect("dev grant");
        assert!(dev.developer_mode && dev.process_env);
        assert_eq!(dev.context_roots, vec!["../shared"]);

        let notice = svc.migration_notice().expect("notice");
        assert_eq!(notice.len(), 1);
        assert!(notice[0].root.ends_with("dev"));
        assert_eq!(notice[0].capabilities, vec![NOTICE_DEVELOPER_MODE, NOTICE_CONTEXT_ROOTS]);

        // A second run changes nothing.
        svc.migrate_legacy(vec![legacy("later", RequestedElevation::default())])
            .expect("again");
        assert!(store.grant_of("later").is_none());

        svc.dismiss_migration_notice().expect("dismiss");
        assert!(svc.migration_notice().expect("notice").is_empty());
    }

    /// Production code must read the effective capabilities, never the raw request fields.
    /// This keeps a new consumer from bypassing the trust gate.
    #[test]
    fn production_code_does_not_read_the_raw_capability_fields() {
        const FIELDS: [&str; 3] = [
            ".sandbox_mode",
            ".script_context_roots",
            ".agent_autonomy_enabled",
        ];
        // Reads of the script phase state and context, which are not collection settings.
        const IGNORED: [&str; 3] = ["state.sandbox_mode", "ctx.sandbox_mode", "self.sandbox_mode"];
        const ALLOWED_FILES: [&str; 3] = ["collection_trust.rs", "collection_service.rs", "test_doubles.rs"];

        fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    rust_files(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }

        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        let mut offenders = Vec::new();
        for path in files {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if ALLOWED_FILES.contains(&name) || name.ends_with("tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read file");
            // Test modules sit at the end of a file, so the production part ends there.
            let production = text.split("#[cfg(test)]").next().unwrap_or_default();
            for (number, line) in production.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                let mut checked = code.to_string();
                for ignored in IGNORED {
                    checked = checked.replace(ignored, "");
                }
                if FIELDS.iter().any(|f| checked.contains(f)) {
                    offenders.push(format!("{}:{}", path.display(), number + 1));
                }
            }
        }
        assert!(offenders.is_empty(), "raw capability reads: {offenders:?}");
    }
}
