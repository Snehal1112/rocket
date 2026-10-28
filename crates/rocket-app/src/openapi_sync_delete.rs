//! Deleting requests from the sync page: one the user added, or one the spec removed that the
//! user had edited. Every call runs under the collection lock and in one file transaction, and
//! nothing here touches the network or the keychain.

use std::collections::{HashMap, HashSet};

use rocket_collection::{request_filename_for, CollectionRepository};
use rocket_import::SyncItem;
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_change_link::UNFINISHED;
use crate::openapi_connect::APPLY_RUNNING;
use crate::openapi_sync_apply::{
    flatten_requests, to_json, to_yaml, BACKUPS_REL, BASELINE_REL, SOURCE_REL,
};
use crate::openapi_sync_service::OpenApiSyncService;

const NOT_DELETABLE: &str =
    "This request is still part of the spec. Remove it from the spec first.";
const REQUEST_CHANGED: &str = "The request changed on disk. Reload the page.";
const BAD_PATH: &str = "That request is not in this collection.";
const FAILED: &str = "Deleting failed. Nothing was changed.";

/// A request to delete, as the page knew it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteTarget {
    pub uid: String,
    pub request_path: String,
}

/// True for a path that stays inside the collection: relative, no `.` or `..` part, no NUL.
fn path_is_inside(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\0')
        && !path.starts_with(['/', '\\'])
        && !path.chars().nth(1).is_some_and(|c| c == ':')
        && path
            .split(['/', '\\'])
            .all(|part| !matches!(part, "" | "." | ".."))
}

impl OpenApiSyncService {
    /// Deletes the requests behind `targets`. A request is deletable when the user added it
    /// (no spec operation behind it), or when the spec removed its operation and the user had
    /// edited it. Each target is checked against the file on disk by uid and path first. The
    /// source operation, the baseline entry and the reset backup of a removed request go with
    /// it, so the removal is not reported again. It returns how many requests were deleted.
    pub fn delete_requests(
        &self,
        collection: &str,
        targets: &[DeleteTarget],
    ) -> DomainResult<usize> {
        if targets.is_empty() {
            return Ok(0);
        }
        if targets.iter().any(|t| !path_is_inside(&t.request_path)) {
            return Err(DomainError::InvalidInput(BAD_PATH.to_string()));
        }
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        match deps.tx.recover_if_idle(&root) {
            Ok(Some(_)) => {}
            Ok(None) => return Err(DomainError::Conflict(APPLY_RUNNING.to_string())),
            Err(e) => {
                // An interrupted apply that cannot be finished blocks every other write.
                tracing::debug!(error = %e, "recovery failed before a delete");
                return Err(DomainError::Conflict(UNFINISHED.to_string()));
            }
        }
        let mut source = self.load(&root)?;
        let current = flatten_requests(&repo.get(collection)?.root);
        // The operations the pending spec removes, with whether the plan counts them as edited.
        // That is the rule the page shows, so the button and this check agree.
        let removed: HashMap<String, Option<bool>> = self
            .plan_locked(deps, repo, collection, &root, &[])
            .map(|view| {
                view.plan
                    .items
                    .iter()
                    .filter_map(|i| match i {
                        SyncItem::Removed { key, edited, .. } => Some((key.clone(), *edited)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        let mut dropped_keys: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        for target in targets {
            if !seen.insert((target.uid.as_str(), target.request_path.as_str())) {
                continue;
            }
            let Some(cur) = current
                .iter()
                .find(|c| c.uid == target.uid && c.request_path == target.request_path)
            else {
                return Err(DomainError::Conflict(REQUEST_CHANGED.to_string()));
            };
            match source.operations.iter().find(|o| o.uid == target.uid) {
                None => {}
                Some(op) => {
                    if removed.get(&op.key) != Some(&Some(true)) {
                        return Err(DomainError::InvalidInput(NOT_DELETABLE.to_string()));
                    }
                    dropped_keys.push(op.key.clone());
                }
            }
            let dir = cur
                .request_path
                .rsplit_once('/')
                .map(|(dir, _)| format!("{dir}/"))
                .unwrap_or_default();
            let file = cur
                .request
                .file_name
                .clone()
                .unwrap_or_else(|| request_filename_for(&cur.request_path));
            files.push(format!("{dir}{file}"));
        }

        let mut tx = deps.tx.begin(&root)?;
        let staged = (|| -> DomainResult<()> {
            for file in &files {
                tx.stage_delete(file)?;
            }
            if !dropped_keys.is_empty() {
                source.operations.retain(|o| !dropped_keys.contains(&o.key));
                tx.stage_put(SOURCE_REL, &to_yaml(&source)?)?;
                if let Some(mut baseline) = deps.baselines.load(&root)? {
                    baseline
                        .operations
                        .retain(|o| !dropped_keys.contains(&o.key));
                    tx.stage_put(BASELINE_REL, &to_json(&baseline)?)?;
                }
                if let Some(backups) = self
                    .backup_repo()
                    .ok()
                    .map(|r| r.load(&root))
                    .transpose()?
                    .flatten()
                {
                    if backups
                        .entries
                        .iter()
                        .any(|e| dropped_keys.contains(&e.key))
                    {
                        tx.stage_put(BACKUPS_REL, &to_json(&backups.without_keys(&dropped_keys))?)?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = staged {
            tx.abort();
            return Err(e);
        }
        match tx.commit() {
            Ok(()) => {}
            Err(e @ DomainError::Conflict(_)) => return Err(e),
            Err(e) => {
                tracing::debug!(error = %e, "a delete could not be committed");
                return Err(DomainError::Internal(FAILED.to_string()));
            }
        }
        if !dropped_keys.is_empty() {
            self.refresh_count(deps, repo, collection, &root);
        }
        Ok(files.len())
    }
}
