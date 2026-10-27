//! Read model of the user's own changes to the imported requests. It reads the source, the
//! baseline and the requests, and writes nothing.

use std::collections::{BTreeMap, HashMap, HashSet};

use rocket_collection::CollectionRepository;
use rocket_import::{baseline_hash, merge_request, CurrentRequest, ParsedSpec, CONVERTER_VERSION};
use rocket_shared::error::DomainResult;

use crate::collection_lock::collection_lock;
use crate::openapi_sync_apply::flatten_requests;
use crate::openapi_sync_service::OpenApiSyncService;
use crate::openapi_sync_view::{BadgeView, ChangeRow, ChangesView, OverviewView};

fn row(
    key: Option<&str>,
    cur: &CurrentRequest,
    path: Option<String>,
    fields: Vec<String>,
) -> ChangeRow {
    ChangeRow {
        key: key.map(str::to_string),
        uid: cur.uid.clone(),
        name: cur.request.name.clone(),
        method: cur.request.method.to_string(),
        url: cur.request.url.clone(),
        request_path: path,
        fields,
    }
}

impl OpenApiSyncService {
    /// The requests the user edited, added or deleted since the last import or sync. It takes
    /// the collection lock like the other reads and needs no pending spec.
    pub fn changes(&self, collection: &str) -> DomainResult<ChangesView> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        let source = self.load(&root)?;
        let baseline = deps
            .baselines
            .load(&root)?
            .filter(|b| b.converter_version == CONVERTER_VERSION);
        let current = flatten_requests(&repo.get(collection)?.root);
        let by_uid: HashMap<&str, &CurrentRequest> =
            current.iter().map(|c| (c.uid.as_str(), c)).collect();
        let known: HashSet<&str> = source.operations.iter().map(|o| o.uid.as_str()).collect();
        let base_by_key: HashMap<&str, _> = baseline
            .as_ref()
            .map(|b| {
                b.operations
                    .iter()
                    .map(|o| (o.key.as_str(), &o.request))
                    .collect()
            })
            .unwrap_or_default();

        let mut modified = Vec::new();
        let mut deleted = Vec::new();
        for op in &source.operations {
            let Some(cur) = by_uid.get(op.uid.as_str()) else {
                deleted.push(ChangeRow {
                    key: Some(op.key.clone()),
                    uid: op.uid.clone(),
                    name: op.key.clone(),
                    method: String::new(),
                    url: String::new(),
                    request_path: Some(op.request_path.clone()),
                    fields: Vec::new(),
                });
                continue;
            };
            let fields = match (&baseline, base_by_key.get(op.key.as_str())) {
                (Some(_), Some(base)) => {
                    let none = BTreeMap::new();
                    // The user's request plays the incoming side, so the changes are the
                    // fields where it differs from the baseline.
                    merge_request(base, base, &cur.request, &none)
                        .changes
                        .into_iter()
                        .map(|c| c.field)
                        .collect::<Vec<_>>()
                }
                (Some(_), None) => Vec::new(),
                (None, _) => {
                    if baseline_hash(&cur.request) == op.baseline_hash {
                        continue;
                    }
                    Vec::new()
                }
            };
            if baseline.is_some() && fields.is_empty() {
                continue;
            }
            modified.push(row(
                Some(&op.key),
                cur,
                Some(cur.request_path.clone()),
                fields,
            ));
        }
        let added = current
            .iter()
            .filter(|c| !known.contains(c.uid.as_str()))
            .map(|c| row(None, c, Some(c.request_path.clone()), Vec::new()))
            .collect();
        Ok(ChangesView {
            baseline_trusted: baseline.is_some(),
            modified,
            added,
            deleted,
        })
    }

    /// The counts of the Overview tab. It reads local state only: no request, no keychain and
    /// no consent. The three reads each take the collection lock in turn.
    pub fn overview(&self, collection: &str) -> DomainResult<OverviewView> {
        let changes = self.changes(collection)?;
        let plan = self.plan(collection)?;
        let local = self.local_state(collection)?;
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let source = self.load(&root)?;
        // The pending copy when one waits, else the applied copy.
        let latest = match self.sources.load_pending(&root)? {
            Some(bytes) => Some(bytes),
            None => self.sources.load_applied(&root)?,
        };
        let spec_version = latest
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|text| ParsedSpec::parse(&text).ok())
            .and_then(|spec| spec.version().map(str::to_string));
        let endpoints = u32::try_from(source.operations.len()).unwrap_or(u32::MAX);
        let waiting = plan.plan.items.len() + plan.plan.variable_changes.len();
        let changed = u32::try_from(changes.modified.len()).unwrap_or(u32::MAX);
        // An operation that is both modified and waiting counts once. Skipped items are not in
        // the plan, so they do not count as waiting.
        let touched: HashSet<&str> = changes
            .modified
            .iter()
            .filter_map(|r| r.key.as_deref())
            .chain(
                source
                    .operations
                    .iter()
                    .map(|o| o.key.as_str())
                    .filter(|k| plan.plan.items.iter().any(|i| i.key() == *k)),
            )
            .collect();
        let touched = u32::try_from(touched.len()).unwrap_or(u32::MAX);
        let (in_sync, changed_by_you) = if changes.baseline_trusted {
            (Some(endpoints.saturating_sub(touched)), Some(changed))
        } else {
            (None, None)
        };
        Ok(OverviewView {
            endpoints,
            in_sync,
            changed_by_you,
            updates_waiting: u32::try_from(waiting).unwrap_or(u32::MAX),
            spec_version,
            last_synced_at: local.last_synced_at,
            last_synced_updated: local.last_synced_updated,
        })
    }

    /// The collections with updates waiting, from the stored counts. A collection whose state
    /// cannot be read is left out, so it does not hide the others.
    pub fn badges(&self) -> Vec<BadgeView> {
        let (Some(deps), Ok(locals)) = (&self.apply_deps, self.local_repo()) else {
            return Vec::new();
        };
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let Ok(names) = repo.list() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for summary in names {
            let Ok(root) = repo.collection_root_path(&summary.name) else {
                continue;
            };
            if !matches!(self.sources.load(&root), Ok(Some(_))) {
                continue;
            }
            match locals.load(&root) {
                Ok(Some(local)) if local.pending_count > 0 => out.push(BadgeView {
                    collection: summary.name,
                    waiting: local.pending_count,
                }),
                _ => {}
            }
        }
        out
    }
}
