//! Moving a connected collection to another spec link. The baseline, the skips, the backups and
//! the interval stay. A spec that differs from the applied one becomes the pending copy, so the
//! user reviews it in Spec updates like any other update.

use std::collections::HashSet;

use rocket_collection::openapi_source::{SpecLocation, SyncStatus};
use rocket_collection::CollectionRepository;
use rocket_import::{fetch_spec_bundle, ParsedSpec};
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_connect::{
    safe_import_message, APPLY_RUNNING, NEEDS_LINK, NOT_A_SPEC, STALE_SPEC,
};
use crate::openapi_sync_apply::{to_yaml, PENDING_REL, SOURCE_REL};
use crate::openapi_sync_service::{sha256_hex, OpenApiSyncService, TOKEN_KEY};

const SAME_LINK: &str = "This is already the spec link.";
const NOT_CONNECTED: &str = "This collection is not connected to an OpenAPI spec.";
pub(crate) const UNFINISHED: &str =
    "An earlier sync is not finished. Try again after Rocket restarts.";
const FAILED: &str = "Changing the link failed. Nothing was changed.";
const WARN_CONSENT: &str = "The new link needs your approval before checks run.";
const WARN_TOKEN: &str = "The token was not saved.";
const WARN_OLD_TOKEN: &str = "The old token could not be removed.";

/// What a move to another link would do, from a fetch that writes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeLinkPreview {
    pub spec_sha256: String,
    pub title: String,
    pub version: Option<String>,
    /// Linked operations of the collection that the new spec has too.
    pub matched: usize,
    /// Operations only the new spec has.
    pub added: usize,
    /// Linked operations the new spec no longer has.
    pub removed: usize,
    /// The new spec is the one already applied.
    pub same_spec: bool,
}

/// What a move did. The warnings are fixed sentences about steps that ran after the files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeLinkOutcome {
    pub pending: bool,
    pub token_kept: bool,
    pub warnings: Vec<String>,
}

/// A fixed error for a failure of the collection files, whose own message can carry a path.
fn infra_error(error: DomainError) -> DomainError {
    match error {
        DomainError::NotFound(_) => {
            DomainError::NotFound("The collection was not found.".to_string())
        }
        DomainError::Conflict(_) => DomainError::Conflict(APPLY_RUNNING.to_string()),
        _ => DomainError::Internal(FAILED.to_string()),
    }
}

/// True when both are Github links to the same repository, so a stored token still fits.
fn same_repo(old: &SpecLocation, new: &SpecLocation) -> bool {
    match (old, new) {
        (
            SpecLocation::Github {
                owner: o1,
                repo: r1,
                ..
            },
            SpecLocation::Github {
                owner: o2,
                repo: r2,
                ..
            },
        ) => o1.eq_ignore_ascii_case(o2) && r1.eq_ignore_ascii_case(r2),
        _ => false,
    }
}

impl OpenApiSyncService {
    /// Fetches the spec at `link` and counts how it lines up with the connected operations.
    /// Nothing is written, no consent is granted and no token is stored.
    pub async fn change_link_preview(
        &self,
        collection: &str,
        link: &str,
        token: Option<&str>,
    ) -> DomainResult<ChangeLinkPreview> {
        let link = link.trim();
        if link.is_empty() {
            return Err(DomainError::InvalidInput(NEEDS_LINK.to_string()));
        }
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection).map_err(infra_error)?;
        let source = {
            let lock = collection_lock(&root);
            let _guard = lock.lock();
            self.sources
                .load(&root)
                .map_err(infra_error)?
                .ok_or_else(|| DomainError::Conflict(NOT_CONNECTED.to_string()))?
        };
        let fetched = fetch_spec_bundle(self.fetcher.as_ref(), link, token)
            .await
            .map_err(|e| DomainError::InvalidInput(safe_import_message(e)))?;
        if fetched.location == source.location {
            return Err(DomainError::Conflict(SAME_LINK.to_string()));
        }
        let spec = ParsedSpec::parse(&fetched.bundle.text)
            .map_err(|_| DomainError::InvalidInput(NOT_A_SPEC.to_string()))?;
        let views = spec.operation_views();
        let new_keys: HashSet<&str> = views.iter().map(|o| o.key.as_str()).collect();
        let old_keys: HashSet<&str> = source.operations.iter().map(|o| o.key.as_str()).collect();
        Ok(ChangeLinkPreview {
            spec_sha256: sha256_hex(fetched.bundle.text.as_bytes()),
            title: spec.title().to_string(),
            version: spec.version().map(str::to_string),
            matched: old_keys.intersection(&new_keys).count(),
            added: new_keys.difference(&old_keys).count(),
            removed: old_keys.difference(&new_keys).count(),
            same_spec: sha256_hex(fetched.bundle.text.as_bytes()) == source.spec_sha256,
        })
    }

    /// Moves the collection to `link` in one file transaction, then approves the typed link and
    /// handles the token. A failure after the files were committed adds a warning and never
    /// undoes them.
    pub async fn change_link_confirm(
        &self,
        collection: &str,
        link: &str,
        token: Option<&str>,
        spec_sha256: &str,
    ) -> DomainResult<ChangeLinkOutcome> {
        let link = link.trim();
        if link.is_empty() {
            return Err(DomainError::InvalidInput(NEEDS_LINK.to_string()));
        }
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection).map_err(infra_error)?;
        let fetched = fetch_spec_bundle(self.fetcher.as_ref(), link, token)
            .await
            .map_err(|e| DomainError::InvalidInput(safe_import_message(e)))?;
        ParsedSpec::parse(&fetched.bundle.text)
            .map_err(|_| DomainError::InvalidInput(NOT_A_SPEC.to_string()))?;
        let hash = sha256_hex(fetched.bundle.text.as_bytes());
        if hash != spec_sha256 {
            return Err(DomainError::Conflict(STALE_SPEC.to_string()));
        }

        let mut outcome = ChangeLinkOutcome::default();
        let old_location;
        let source_id;
        {
            let lock = collection_lock(&root);
            let _guard = lock.lock();
            let mut source = self
                .sources
                .load(&root)
                .map_err(infra_error)?
                .ok_or_else(|| DomainError::Conflict(NOT_CONNECTED.to_string()))?;
            if fetched.location == source.location {
                return Err(DomainError::Conflict(SAME_LINK.to_string()));
            }
            match deps.tx.recover_if_idle(&root) {
                Ok(Some(_)) => {}
                Ok(None) => return Err(DomainError::Conflict(APPLY_RUNNING.to_string())),
                Err(e) => {
                    // An interrupted apply that cannot be finished blocks every other write.
                    tracing::debug!(error = %e, "recovery failed before a link change");
                    return Err(DomainError::Conflict(UNFINISHED.to_string()));
                }
            }
            old_location = source.location.clone();
            source_id = source.source_id.clone();

            let differs = hash != source.spec_sha256;
            source.location = fetched.location.clone();
            source.etag = fetched.etag.clone();
            source.last_modified = fetched.last_modified.clone();
            source.remote_refs = fetched.bundle.refs.clone();
            source.remote_refs_incomplete = fetched.bundle.fetch_failed;
            source.pending_sha256 = differs.then(|| hash.clone());

            let mut tx = deps.tx.begin(&root).map_err(infra_error)?;
            let staged = (|| -> DomainResult<()> {
                tx.stage_put(SOURCE_REL, &to_yaml(&source)?)?;
                if differs {
                    tx.stage_put(PENDING_REL, fetched.bundle.text.as_bytes())?;
                } else {
                    tx.stage_delete(PENDING_REL)?;
                }
                Ok(())
            })();
            if let Err(e) = staged {
                tx.abort();
                tracing::debug!(error = %e, "a link change could not be staged");
                return Err(DomainError::Internal(FAILED.to_string()));
            }
            if let Err(e) = tx.commit() {
                tracing::debug!(error = %e, "a link change could not be committed");
                return Err(DomainError::Internal(FAILED.to_string()));
            }
            outcome.pending = differs;

            // The fetch worked, so the local state records a good check. A count that cannot be
            // made keeps the old one.
            let count = self.count_pending_locked(&root).ok();
            let status = if differs {
                SyncStatus::Changed
            } else {
                SyncStatus::Unchanged
            };
            if let Err(e) = self.record_success(&root, &self.now(), status, count) {
                tracing::debug!(error = %e, "the local state could not be updated after a link change");
            }
        }

        // The files are committed. What follows only adds warnings.
        let consented = self.grant_consent_for_link(&root, link).is_ok();
        if !consented {
            outcome.warnings.push(WARN_CONSENT.to_string());
        }
        let keeps = same_repo(&old_location, &fetched.location);
        if !keeps && !source_id.is_empty() {
            if let Err(e) = self.secrets.delete(&source_id, TOKEN_KEY) {
                tracing::debug!(error = %e, "the old token could not be removed");
                outcome.warnings.push(WARN_OLD_TOKEN.to_string());
            }
        }
        outcome.token_kept = keeps;
        if let Some(token) = token.map(str::trim).filter(|t| !t.is_empty()) {
            if consented && self.set_token(&root, token).is_ok() {
                outcome.token_kept = true;
            } else {
                outcome.warnings.push(WARN_TOKEN.to_string());
            }
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(owner: &str, repo: &str, path: &str) -> SpecLocation {
        SpecLocation::Github {
            owner: owner.into(),
            repo: repo.into(),
            git_ref: "main".into(),
            path: path.into(),
        }
    }

    #[test]
    fn the_same_repo_ignores_case_and_path() {
        assert!(same_repo(
            &gh("Acme", "Api", "a.json"),
            &gh("acme", "api", "b.json")
        ));
    }

    #[test]
    fn another_repo_or_a_plain_link_is_not_the_same_repo() {
        assert!(!same_repo(
            &gh("acme", "api", "a"),
            &gh("acme", "other", "a")
        ));
        let https = SpecLocation::Https {
            url: "https://example.com/x.json".into(),
        };
        assert!(!same_repo(&gh("acme", "api", "a"), &https));
        assert!(!same_repo(&https, &gh("acme", "api", "a")));
    }
}
