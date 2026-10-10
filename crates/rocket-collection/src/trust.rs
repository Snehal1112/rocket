//! Collection trust: what a collection asks for versus what this computer grants.
//!
//! A collection file only holds a request. The trust store, kept in the user's own app
//! data, holds the grant. What runs is the effective value, computed on every use.

use std::path::PathBuf;

use rocket_shared::error::{DomainError, DomainResult};

use crate::settings::{CollectionSettings, SandboxMode};

/// The elevated capabilities a collection file asks for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestedElevation {
    pub developer_mode: bool,
    pub context_roots: Vec<String>,
    pub agent_run: bool,
}

impl RequestedElevation {
    pub fn from_settings(settings: &CollectionSettings) -> Self {
        Self {
            developer_mode: settings.sandbox_mode == SandboxMode::Developer,
            context_roots: settings
                .script_context_roots
                .iter()
                .map(|r| normalize_root(r))
                .collect(),
            agent_run: settings.agent_autonomy_enabled,
        }
    }
}

/// Where a grant came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GrantSource {
    /// The user confirmed it in the app.
    #[default]
    User,
    /// The collection was created in Rocket.
    Created,
    /// Granted once at the first start after the trust gate shipped.
    Migrated,
}

/// What the user allowed for one collection on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollectionGrant {
    pub developer_mode: bool,
    /// Approved, normalised roots.
    pub context_roots: Vec<String>,
    pub agent_run: bool,
    pub process_env: bool,
    pub source: GrantSource,
}

/// The capabilities that actually apply to a collection right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveCapabilities {
    pub sandbox_mode: SandboxMode,
    pub context_roots: Vec<String>,
    pub agent_run: bool,
    pub process_env: bool,
}

impl EffectiveCapabilities {
    /// Safe mode, no extra roots, no agent run and no host environment.
    pub fn untrusted() -> Self {
        Self {
            sandbox_mode: SandboxMode::Safe,
            context_roots: Vec::new(),
            agent_run: false,
            process_env: false,
        }
    }
}

/// Combines a request with a grant. The file can always turn power down, but it can
/// only turn power up when the grant allows it.
pub fn resolve_effective(
    req: &RequestedElevation,
    grant: Option<&CollectionGrant>,
) -> EffectiveCapabilities {
    let Some(grant) = grant else {
        return EffectiveCapabilities::untrusted();
    };
    let developer = req.developer_mode && grant.developer_mode;
    let context_roots = if developer {
        let approved: Vec<String> = grant.context_roots.iter().map(|r| normalize_root(r)).collect();
        req.context_roots
            .iter()
            .map(|r| normalize_root(r))
            .filter(|r| approved.contains(r))
            .collect()
    } else {
        Vec::new()
    };
    EffectiveCapabilities {
        sandbox_mode: if developer {
            SandboxMode::Developer
        } else {
            SandboxMode::Safe
        },
        context_roots,
        agent_run: req.agent_run && grant.agent_run,
        process_env: grant.process_env,
    }
}

/// Trims, strips trailing slashes and drops leading `./` so equal roots compare equal.
pub fn normalize_root(raw: &str) -> String {
    let mut s = raw.trim();
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.trim_start_matches('/');
    }
    let stripped = s.trim_end_matches('/');
    if stripped.is_empty() && s.starts_with('/') {
        return "/".to_string();
    }
    stripped.to_string()
}

/// Canonical text of a request, so a change between showing and approving is detectable.
/// Roots are sorted and deduplicated, so their order does not matter.
pub fn request_fingerprint(req: &RequestedElevation) -> String {
    let mut roots: Vec<String> = req.context_roots.iter().map(|r| normalize_root(r)).collect();
    roots.sort();
    roots.dedup();
    format!(
        "dev={};agent={};roots={}",
        u8::from(req.developer_mode),
        u8::from(req.agent_run),
        roots.join("|")
    )
}

/// Which collection a grant belongs to: the canonical folder plus the file's uid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionIdentity {
    pub canonical_root: PathBuf,
    pub uid: Option<String>,
}

/// A collection found on disk, with what its file asks for. Used by the one-time migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyCollection {
    pub name: String,
    pub identity: CollectionIdentity,
    pub requested: RequestedElevation,
}

/// One collection listed in the one-time upgrade notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationNoticeEntry {
    pub root: String,
    /// Names of the elevated capabilities the collection kept, for example `developerMode`.
    pub capabilities: Vec<String>,
}

/// Persistence for grants. Implementations live in the infrastructure crate.
///
/// A read that cannot be trusted (corrupt file) returns an error, and callers treat that
/// as "nothing is granted".
pub trait CollectionTrustStore: Send + Sync {
    /// The grant for an identity, if one matches both path and uid.
    fn grant_for(&self, id: &CollectionIdentity) -> DomainResult<Option<CollectionGrant>>;
    /// Creates or replaces the grant for an identity.
    fn put(&self, id: &CollectionIdentity, grant: CollectionGrant) -> DomainResult<()>;
    /// Removes the grant for an identity. Missing is not an error.
    fn remove(&self, id: &CollectionIdentity) -> DomainResult<()>;
    /// Moves a grant from one identity to another. Missing is not an error.
    fn rekey(&self, old: &CollectionIdentity, new: &CollectionIdentity) -> DomainResult<()>;
    /// True once the one-time grandfathering has run.
    fn migrated(&self) -> DomainResult<bool>;
    /// Writes all migrated grants and the notice, and marks the migration as done.
    fn complete_migration(
        &self,
        grants: Vec<(CollectionIdentity, CollectionGrant)>,
        notice: Vec<MigrationNoticeEntry>,
    ) -> DomainResult<()>;
    /// Collections to list in the upgrade notice, until dismissed.
    fn migration_notice(&self) -> DomainResult<Vec<MigrationNoticeEntry>>;
    /// Clears the upgrade notice.
    fn dismiss_migration_notice(&self) -> DomainResult<()>;
}

/// A store that grants nothing. It is the default when no store is wired, so a missed
/// wiring fails closed.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAllTrustStore;

impl CollectionTrustStore for DenyAllTrustStore {
    fn grant_for(&self, _: &CollectionIdentity) -> DomainResult<Option<CollectionGrant>> {
        Ok(None)
    }
    fn put(&self, _: &CollectionIdentity, _: CollectionGrant) -> DomainResult<()> {
        Err(DomainError::Internal("no trust store is configured".into()))
    }
    fn remove(&self, _: &CollectionIdentity) -> DomainResult<()> {
        Ok(())
    }
    fn rekey(&self, _: &CollectionIdentity, _: &CollectionIdentity) -> DomainResult<()> {
        Ok(())
    }
    fn migrated(&self) -> DomainResult<bool> {
        Ok(true)
    }
    fn complete_migration(
        &self,
        _: Vec<(CollectionIdentity, CollectionGrant)>,
        _: Vec<MigrationNoticeEntry>,
    ) -> DomainResult<()> {
        Err(DomainError::Internal("no trust store is configured".into()))
    }
    fn migration_notice(&self) -> DomainResult<Vec<MigrationNoticeEntry>> {
        Ok(Vec::new())
    }
    fn dismiss_migration_notice(&self) -> DomainResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(dev: bool, roots: &[&str], agent: bool) -> RequestedElevation {
        RequestedElevation {
            developer_mode: dev,
            context_roots: roots.iter().map(|r| r.to_string()).collect(),
            agent_run: agent,
        }
    }

    fn grant(dev: bool, roots: &[&str], agent: bool, env: bool) -> CollectionGrant {
        CollectionGrant {
            developer_mode: dev,
            context_roots: roots.iter().map(|r| r.to_string()).collect(),
            agent_run: agent,
            process_env: env,
            source: GrantSource::User,
        }
    }

    #[test]
    fn no_grant_is_untrusted() {
        let eff = resolve_effective(&req(true, &["a"], true), None);
        assert_eq!(eff, EffectiveCapabilities::untrusted());
    }

    #[test]
    fn safe_request_stays_safe_with_any_grant() {
        let eff = resolve_effective(&req(false, &[], false), Some(&grant(true, &[], true, true)));
        assert_eq!(eff.sandbox_mode, SandboxMode::Safe);
        assert!(!eff.agent_run);
        assert!(eff.process_env);
    }

    #[test]
    fn developer_needs_request_and_grant() {
        let granted = grant(true, &[], false, false);
        let denied = grant(false, &[], false, false);
        assert_eq!(
            resolve_effective(&req(true, &[], false), Some(&granted)).sandbox_mode,
            SandboxMode::Developer
        );
        assert_eq!(
            resolve_effective(&req(true, &[], false), Some(&denied)).sandbox_mode,
            SandboxMode::Safe
        );
        assert_eq!(
            resolve_effective(&req(false, &[], false), Some(&granted)).sandbox_mode,
            SandboxMode::Safe
        );
    }

    #[test]
    fn roots_are_the_intersection_with_approved() {
        let eff = resolve_effective(
            &req(true, &["../shared", "../new"], false),
            Some(&grant(true, &["../shared"], false, false)),
        );
        assert_eq!(eff.context_roots, vec!["../shared"]);
    }

    #[test]
    fn roots_are_dropped_when_effective_mode_is_safe() {
        let eff = resolve_effective(
            &req(true, &["../shared"], false),
            Some(&grant(false, &["../shared"], false, false)),
        );
        assert!(eff.context_roots.is_empty());
        let eff = resolve_effective(
            &req(false, &["../shared"], false),
            Some(&grant(true, &["../shared"], false, false)),
        );
        assert!(eff.context_roots.is_empty());
    }

    #[test]
    fn roots_compare_after_normalisation() {
        let eff = resolve_effective(
            &req(true, &["./a/"], false),
            Some(&grant(true, &["a"], false, false)),
        );
        assert_eq!(eff.context_roots, vec!["a"]);
    }

    #[test]
    fn agent_run_needs_request_and_grant() {
        let granted = grant(false, &[], true, false);
        let denied = grant(false, &[], false, false);
        assert!(resolve_effective(&req(false, &[], true), Some(&granted)).agent_run);
        assert!(!resolve_effective(&req(false, &[], true), Some(&denied)).agent_run);
        // The file turning the switch off wins over a grant.
        assert!(!resolve_effective(&req(false, &[], false), Some(&granted)).agent_run);
    }

    #[test]
    fn process_env_follows_the_grant_only() {
        let on = grant(false, &[], false, true);
        let off = grant(false, &[], false, false);
        assert!(resolve_effective(&req(false, &[], false), Some(&on)).process_env);
        assert!(!resolve_effective(&req(false, &[], false), Some(&off)).process_env);
    }

    #[test]
    fn normalize_root_cases() {
        assert_eq!(normalize_root("./a/"), "a");
        assert_eq!(normalize_root("a"), "a");
        assert_eq!(normalize_root("  ../x// "), "../x");
        assert_eq!(normalize_root("/"), "/");
        assert_eq!(normalize_root("././b"), "b");
    }

    #[test]
    fn fingerprint_changes_with_any_requested_value() {
        let base = request_fingerprint(&req(false, &["a"], false));
        assert_ne!(base, request_fingerprint(&req(true, &["a"], false)));
        assert_ne!(base, request_fingerprint(&req(false, &["a"], true)));
        assert_ne!(base, request_fingerprint(&req(false, &["a", "b"], false)));
    }

    #[test]
    fn fingerprint_ignores_root_order_and_spelling() {
        assert_eq!(
            request_fingerprint(&req(false, &["b", "./a/"], false)),
            request_fingerprint(&req(false, &["a", "b"], false))
        );
    }

    #[test]
    fn deny_all_store_grants_nothing() {
        let id = CollectionIdentity {
            canonical_root: PathBuf::from("/x"),
            uid: None,
        };
        assert_eq!(DenyAllTrustStore.grant_for(&id).expect("ok"), None);
        assert!(DenyAllTrustStore.put(&id, CollectionGrant::default()).is_err());
    }
}
