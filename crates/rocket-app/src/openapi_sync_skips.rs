//! The items the user skipped, and taking a skip back. Both work offline on the pending copy.

use std::collections::{BTreeMap, HashMap};

use rocket_collection::CollectionRepository;
use rocket_import::{plan_with_links, CurrentRequest, SyncItem};
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_change_link::UNFINISHED;
use crate::openapi_connect::APPLY_RUNNING;
use crate::openapi_sync_service::OpenApiSyncService;

/// One skipped item. A key the spec no longer has shows the key as its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRow {
    pub key: String,
    pub name: String,
    pub method: String,
    pub path: String,
}

impl OpenApiSyncService {
    /// The skips the user made, with what each one is. Nothing pending gives an empty list.
    pub fn skipped(&self, collection: &str) -> DomainResult<Vec<SkippedRow>> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        if self.load(&root)?.pending_sha256.is_none() {
            return Ok(Vec::new());
        }
        let loaded = self.load_for_apply(deps, repo, collection, &root)?;
        if loaded.local.skips.is_empty() {
            return Ok(Vec::new());
        }
        let prepared = loaded.prepare()?;
        // A plan without the skips knows every skipped item that is still in the spec.
        let none = BTreeMap::new();
        let full = plan_with_links(&loaded.sync_inputs(&prepared, &none), &[])
            .map_err(|e| DomainError::InvalidInput(e.to_string()))?;
        let items: HashMap<&str, &SyncItem> = full.items.iter().map(|i| (i.key(), i)).collect();
        let by_uid: HashMap<&str, &CurrentRequest> =
            loaded.current.iter().map(|c| (c.uid.as_str(), c)).collect();
        Ok(loaded
            .local
            .skips
            .iter()
            // A skip hides an item only while the item still has the fingerprint that was
            // skipped. A changed item is back in the plan, so it is not listed here.
            .filter(|(key, fp)| {
                items
                    .get(key.as_str())
                    .is_none_or(|item| item.fingerprint() == fp.as_str())
            })
            .map(|(key, _)| match items.get(key.as_str()) {
                Some(SyncItem::New { name, request, .. }) => SkippedRow {
                    key: key.clone(),
                    name: name.clone(),
                    method: request.method.to_string(),
                    path: request.url.clone(),
                },
                Some(SyncItem::Changed { name, spec, .. }) => SkippedRow {
                    key: key.clone(),
                    name: name.clone(),
                    method: spec.method.to_string(),
                    path: spec.url.clone(),
                },
                Some(SyncItem::Removed {
                    name,
                    uid,
                    request_path,
                    ..
                }) => SkippedRow {
                    key: key.clone(),
                    name: name.clone(),
                    method: by_uid
                        .get(uid.as_str())
                        .map(|c| c.request.method.to_string())
                        .unwrap_or_default(),
                    path: request_path.clone(),
                },
                None => SkippedRow {
                    key: key.clone(),
                    name: key.clone(),
                    method: String::new(),
                    path: String::new(),
                },
            })
            .collect())
    }

    /// Takes the named skips back and refreshes the waiting count. It refuses while an apply
    /// runs. It returns how many skips were removed.
    pub fn unskip(&self, collection: &str, keys: &[String]) -> DomainResult<usize> {
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
                tracing::debug!(error = %e, "recovery failed before an un-skip");
                return Err(DomainError::Conflict(UNFINISHED.to_string()));
            }
        }
        let locals = self.local_repo()?;
        let mut local = locals.load(&root)?.unwrap_or_default();
        let before = local.skips.len();
        for key in keys {
            local.skips.remove(key);
        }
        let removed = before - local.skips.len();
        if removed > 0 {
            locals.save(&root, &local)?;
            self.refresh_count(deps, repo, collection, &root);
        }
        Ok(removed)
    }
}
