//! Reset to the spec, with a backup the user can restore or discard. Every call runs under the
//! collection lock and in one file transaction. Nothing here touches the network or the
//! keychain.

use rocket_collection::file_tx::CollectionFileTx;
use rocket_collection::openapi_backup::{BackupEntry, OpenApiBackups};
use rocket_collection::{request_filename_for, CollectionRepository, Request};
use rocket_import::{
    request_is_edited, reset_spec_owned_fields, CurrentRequest, CONVERTER_VERSION,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_sync_apply::{flatten_requests, to_json, to_yaml, BACKUPS_REL};
use crate::openapi_sync_service::OpenApiSyncService;

const OLDER_VERSION: &str =
    "These requests cannot be reset because the sync data is from an older version.";
const APPLY_RUNNING: &str = "another sync apply is running on this collection";
const CHANGED_ON_DISK: &str = "The request changed on disk. Reload and try again.";
const NO_BACKUP: &str = "There is no backup for this request.";
const FAILED: &str = "Reset failed. Nothing was changed.";
const SOURCE_REL: &str = ".rocket/openapi-source.yml";

/// What a reset did. A key lands in `skipped` when it has no request, no spec version to go
/// back to, nothing to reset, or a file that holds more than Rocket models.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResetReport {
    pub reset: Vec<String>,
    pub skipped: Vec<String>,
}

fn commit(tx: Box<dyn CollectionFileTx>) -> DomainResult<()> {
    match tx.commit() {
        Ok(()) => Ok(()),
        Err(e @ DomainError::Conflict(_)) => Err(e),
        Err(e) => {
            tracing::debug!(error = %e, "a reset could not be committed");
            Err(DomainError::Internal(FAILED.to_string()))
        }
    }
}

/// The request with this uid. A request at the same path with another uid is somebody else's.
fn find_current<'a>(current: &'a [CurrentRequest], uid: &str) -> Option<&'a CurrentRequest> {
    current.iter().find(|c| c.uid == uid)
}

impl OpenApiSyncService {
    /// Resets the spec-owned fields of the requests behind `keys` to the last synced spec and
    /// keeps the version the user had as a backup. Scripts, tests, assertions and docs stay.
    pub fn reset(&self, collection: &str, keys: &[String]) -> DomainResult<ResetReport> {
        let deps = self.deps()?;
        let backup_repo = self.backup_repo()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        if deps.tx.recover_if_idle(&root)?.is_none() {
            return Err(DomainError::Conflict(APPLY_RUNNING.to_string()));
        }
        let source = self.load(&root)?;
        let baseline = deps
            .baselines
            .load(&root)?
            .filter(|b| b.converter_version == CONVERTER_VERSION)
            .ok_or_else(|| DomainError::InvalidInput(OLDER_VERSION.to_string()))?;
        let mut backups = backup_repo.load(&root)?.unwrap_or_default();
        let current = flatten_requests(&repo.get(collection)?.root);
        let renderer = (deps.renderer)(&root);
        let saved_at = self.now().to_rfc3339();

        let mut tx = deps.tx.begin(&root)?;
        let mut report = ResetReport::default();
        // The requests as they were read, to be compared again just before the commit.
        let mut seen: Vec<(String, Request)> = Vec::new();
        let staged = (|| -> DomainResult<()> {
            for key in keys {
                let found = source
                    .operations
                    .iter()
                    .find(|o| o.key == *key)
                    .and_then(|op| {
                        let base = baseline.operations.iter().find(|b| b.key == *key)?;
                        let cur = find_current(&current, &op.uid)?;
                        Some((op, base, cur))
                    });
                let Some((op, base, cur)) = found else {
                    report.skipped.push(key.clone());
                    continue;
                };
                let reset = reset_spec_owned_fields(&cur.request, &base.request);
                if !request_is_edited(&cur.request, &reset)
                    || !renderer.can_rewrite(&cur.request_path, &cur.request)?
                {
                    report.skipped.push(key.clone());
                    continue;
                }
                let bytes = renderer.render(&cur.request_path, &reset)?;
                tx.stage_put(&request_filename_for(&cur.request_path), &bytes)?;
                backups.entries.retain(|e| e.key != *key);
                backups.entries.push(BackupEntry {
                    key: key.clone(),
                    uid: cur.uid.clone(),
                    request_path: op.request_path.clone(),
                    saved_at: saved_at.clone(),
                    request: cur.request.clone(),
                });
                seen.push((cur.request_path.clone(), cur.request.clone()));
                report.reset.push(key.clone());
            }
            if !report.reset.is_empty() {
                tx.stage_put(BACKUPS_REL, &to_json(&backups)?)?;
            }
            Ok(())
        })();
        if let Err(e) = staged {
            tx.abort();
            return Err(e);
        }
        if report.reset.is_empty() {
            tx.abort();
            return Ok(report);
        }
        // An editor save between the read and now would be overwritten, so it stops the reset.
        for (path, before) in &seen {
            match repo.get_request(collection, path) {
                Ok(now) if !request_is_edited(before, &now) => {}
                _ => {
                    tx.abort();
                    return Err(DomainError::Conflict(CHANGED_ON_DISK.to_string()));
                }
            }
        }
        commit(tx)?;
        Ok(report)
    }

    /// Puts the backed-up request back and removes the entry. A request that was deleted is
    /// recreated at its old path, or at a numbered path when that one is taken.
    pub fn restore(&self, collection: &str, key: &str) -> DomainResult<()> {
        let deps = self.deps()?;
        let backup_repo = self.backup_repo()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        if deps.tx.recover_if_idle(&root)?.is_none() {
            return Err(DomainError::Conflict(APPLY_RUNNING.to_string()));
        }
        let backups = backup_repo.load(&root)?.unwrap_or_default();
        let entry = backups
            .entries
            .iter()
            .find(|e| e.key == key)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(NO_BACKUP.to_string()))?;
        let current = flatten_requests(&repo.get(collection)?.root);
        let renderer = (deps.renderer)(&root);
        let mut tx = deps.tx.begin(&root)?;
        let staged = (|| -> DomainResult<Option<(String, Request)>> {
            let mut watch = None;
            if let Some(cur) = find_current(&current, &entry.uid) {
                // Only the spec-owned fields come back, so scripts, examples and other edits
                // made after the reset stay.
                let request = reset_spec_owned_fields(&cur.request, &entry.request);
                let bytes = renderer.render(&cur.request_path, &request)?;
                tx.stage_put(&request_filename_for(&cur.request_path), &bytes)?;
                watch = Some((cur.request_path.clone(), cur.request.clone()));
            } else {
                let files = deps.tx.list_files(&root)?;
                let taken = |path: &str| {
                    files.contains(&format!("{path}.yml"))
                        || files.contains(&format!("{path}.yaml"))
                };
                let mut path = entry.request_path.clone();
                let mut n = 2;
                while taken(&path) {
                    path = format!("{}-{n}", entry.request_path);
                    n += 1;
                }
                let mut request = entry.request.clone();
                request.file_name = None;
                let bytes = renderer.render(&path, &request)?;
                tx.stage_put(&request_filename_for(&path), &bytes)?;
                if path != entry.request_path {
                    let mut source = self.load(&root)?;
                    if let Some(op) = source.operations.iter_mut().find(|o| o.key == key) {
                        op.request_path = path;
                        tx.stage_put(SOURCE_REL, &to_yaml(&source)?)?;
                    }
                }
            }
            tx.stage_put(
                BACKUPS_REL,
                &to_json(&backups.without_keys(&[key.to_string()]))?,
            )?;
            Ok(watch)
        })();
        let watch = match staged {
            Ok(watch) => watch,
            Err(e) => {
                tx.abort();
                return Err(e);
            }
        };
        if let Some((path, before)) = watch {
            match repo.get_request(collection, &path) {
                Ok(now) if !request_is_edited(&before, &now) => {}
                _ => {
                    tx.abort();
                    return Err(DomainError::Conflict(CHANGED_ON_DISK.to_string()));
                }
            }
        }
        commit(tx)
    }

    /// Removes a backup without touching any request.
    pub fn discard(&self, collection: &str, key: &str) -> DomainResult<()> {
        let deps = self.deps()?;
        let backup_repo = self.backup_repo()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        if deps.tx.recover_if_idle(&root)?.is_none() {
            return Err(DomainError::Conflict(APPLY_RUNNING.to_string()));
        }
        let backups: OpenApiBackups = backup_repo.load(&root)?.unwrap_or_default();
        if !backups.entries.iter().any(|e| e.key == key) {
            return Err(DomainError::NotFound(NO_BACKUP.to_string()));
        }
        let mut tx = deps.tx.begin(&root)?;
        let rest = backups.without_keys(&[key.to_string()]);
        if let Err(e) = to_json(&rest).and_then(|bytes| tx.stage_put(BACKUPS_REL, &bytes)) {
            tx.abort();
            return Err(e);
        }
        commit(tx)
    }
}
