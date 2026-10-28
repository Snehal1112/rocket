//! Applies approved spec changes to a collection through one file transaction, and finishes or
//! undoes an apply that a crash cut short. The pure work is `rocket_import::build_apply`; this
//! file loads the inputs, stages the result and commits.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use rocket_collection::file_tx::{CollectionFileTx, CollectionFileTxFactory, RecoveryOutcome};
use rocket_collection::openapi_baseline::{OpenApiBaseline, OpenApiBaselineRepository};
use rocket_collection::openapi_local::{OpenApiLocalRepository, OpenApiLocalState};
use rocket_collection::openapi_source::{OpenApiSource, SyncStatus};
use rocket_collection::{CollectionItem, CollectionRepository, Folder};
use rocket_import::{
    build_apply, item_digest, plan_with_links, variable_digest, ApplyInputs, ApplyReport,
    CurrentRequest, FileChangeKind, ParsedSpec, RequestRenderer, Selection, SyncInputs,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_sync_service::{sha256_hex, OpenApiSyncService};
use crate::openapi_sync_view::{PlanView, UnchangedView};

pub(crate) const SOURCE_REL: &str = ".rocket/openapi-source.yml";
pub(crate) const BASELINE_REL: &str = ".rocket/openapi-baseline.json";
pub(crate) const LOCAL_REL: &str = ".rocket/openapi-local.json";
pub(crate) const PENDING_REL: &str = ".rocket/openapi-pending.spec";
pub(crate) const APPLIED_REL: &str = ".rocket/openapi-applied.spec";
pub(crate) const BACKUPS_REL: &str = ".rocket/openapi-reset-backups.json";
const BASE_URL_KEY: &str = "baseUrl";

/// Builds the renderer for the collection folder at the given path.
pub type RendererFactory = Arc<dyn Fn(&Path) -> Box<dyn RequestRenderer> + Send + Sync>;

/// What `apply` and `recover` need beyond a check.
pub struct ApplyDeps {
    pub collections: Box<dyn CollectionRepository>,
    pub baselines: Box<dyn OpenApiBaselineRepository>,
    pub locals: Box<dyn OpenApiLocalRepository>,
    pub tx: Arc<dyn CollectionFileTxFactory>,
    pub renderer: RendererFactory,
}

fn not_set_up() -> DomainError {
    DomainError::Internal("Applying spec changes is not set up.".to_string())
}

/// Walks the tree and returns each HTTP request with its path (no extension).
fn flatten(folder: &Folder, prefix: &str, out: &mut Vec<CurrentRequest>) {
    for item in &folder.items {
        match item {
            CollectionItem::Request(request) => {
                let Some(file) = request.file_name.as_deref() else {
                    continue;
                };
                let stem = file
                    .strip_suffix(".yml")
                    .or_else(|| file.strip_suffix(".yaml"))
                    .unwrap_or(file);
                // A file with no `uid:` key (a Bruno import) reads as an empty uid. Give it the
                // uid the sidebar derives from its path, so two such files never share one.
                let uid = if request.uid.is_empty() {
                    format!("http-{prefix}{file}")
                } else {
                    request.uid.clone()
                };
                out.push(CurrentRequest {
                    uid,
                    request_path: format!("{prefix}{stem}"),
                    request: (**request).clone(),
                });
            }
            CollectionItem::Folder(sub) => {
                let name = sub.dir_name.as_deref().unwrap_or(&sub.name);
                flatten(sub, &format!("{prefix}{name}/"), out);
            }
            _ => {}
        }
    }
}

/// Every HTTP request of the collection with its path, for the read models.
pub(crate) fn flatten_requests(folder: &Folder) -> Vec<CurrentRequest> {
    let mut out = Vec::new();
    flatten(folder, "", &mut out);
    out
}

fn count_of(view: &PlanView) -> u32 {
    let total = view.plan.items.len() + view.plan.variable_changes.len();
    u32::try_from(total).unwrap_or(u32::MAX)
}

fn stage_all(
    tx: &mut dyn CollectionFileTx,
    files: &[rocket_import::FileChange],
    extra: Vec<(&str, Option<Vec<u8>>)>,
) -> DomainResult<()> {
    for file in files {
        match &file.kind {
            FileChangeKind::Put(bytes) => tx.stage_put(&file.rel_path, bytes)?,
            FileChangeKind::Delete => tx.stage_delete(&file.rel_path)?,
        }
    }
    for (rel, bytes) in extra {
        match bytes {
            Some(bytes) => tx.stage_put(rel, &bytes)?,
            None => tx.stage_delete(rel)?,
        }
    }
    Ok(())
}

pub(crate) fn to_yaml(source: &OpenApiSource) -> DomainResult<Vec<u8>> {
    // A local file source is written as version 2, so an older build refuses it.
    let mut source = source.clone();
    source.version = source.write_version();
    serde_yaml::to_string(&source)
        .map(String::into_bytes)
        .map_err(|e| DomainError::Internal(format!("the source file could not be written: {e}")))
}

pub(crate) fn to_json<T: serde::Serialize>(value: &T) -> DomainResult<Vec<u8>> {
    serde_json::to_vec_pretty(value)
        .map_err(|e| DomainError::Internal(format!("a state file could not be written: {e}")))
}

/// Everything `apply` reads before it builds the changes.
pub(crate) struct Loaded {
    pub(crate) source: OpenApiSource,
    pub(crate) baseline: Option<OpenApiBaseline>,
    pub(crate) local: OpenApiLocalState,
    pending_text: String,
    pub(crate) current: Vec<CurrentRequest>,
    existing_paths: HashSet<String>,
    settings: rocket_collection::CollectionSettings,
}

/// What the plan needs besides `Loaded`: the parsed pending spec and the collection facts.
pub(crate) struct Prepared {
    incoming: ParsedSpec,
    variables: Vec<String>,
    base_url: Option<String>,
}

impl Loaded {
    pub(crate) fn prepare(&self) -> DomainResult<Prepared> {
        let incoming = ParsedSpec::parse(&self.pending_text)
            .map_err(|e| DomainError::InvalidInput(e.to_string()))?;
        let variables = self
            .settings
            .variables
            .iter()
            .map(|v| v.key.clone())
            .collect();
        let base_url = self
            .settings
            .variables
            .iter()
            .find(|v| v.key == BASE_URL_KEY)
            .map(|v| v.value.clone());
        Ok(Prepared {
            incoming,
            variables,
            base_url,
        })
    }

    /// The one place the plan inputs are put together, for `apply` and `plan` alike.
    pub(crate) fn sync_inputs<'a>(
        &'a self,
        prepared: &'a Prepared,
        skips: &'a BTreeMap<String, String>,
    ) -> SyncInputs<'a> {
        SyncInputs {
            source: &self.source,
            baseline: self.baseline.as_ref(),
            incoming: &prepared.incoming,
            current: &self.current,
            current_base_url: prepared.base_url.as_deref(),
            existing_variables: &prepared.variables,
            skips,
        }
    }
}

impl OpenApiSyncService {
    pub(crate) fn deps(&self) -> DomainResult<&ApplyDeps> {
        self.apply_deps.as_ref().ok_or_else(not_set_up)
    }

    pub(crate) fn load_for_apply(
        &self,
        deps: &ApplyDeps,
        repo: &dyn CollectionRepository,
        name: &str,
        root: &Path,
    ) -> DomainResult<Loaded> {
        let source = self.load(root)?;
        let pending = self.sources.load_pending(root)?;
        let pending = match (pending, source.pending_sha256.as_deref()) {
            (Some(bytes), Some(hash)) if sha256_hex(&bytes) == hash => bytes,
            _ => {
                return Err(DomainError::Conflict(
                    "the pending spec changed, check again".to_string(),
                ))
            }
        };
        let pending_text = String::from_utf8(pending)
            .map_err(|_| DomainError::InvalidInput("the pending spec is not text".to_string()))?;
        let collection = repo.get(name)?;
        let mut current = Vec::new();
        flatten(&collection.root, "", &mut current);
        Ok(Loaded {
            baseline: deps.baselines.load(root)?,
            local: self.local_repo()?.load(root)?.unwrap_or_default(),
            existing_paths: deps.tx.list_files(root)?,
            settings: repo.get_settings(name)?,
            source,
            pending_text,
            current,
        })
    }

    /// Applies the items the user approved in one transaction. Either every file and state
    /// change lands, or none does.
    pub async fn apply(
        &self,
        collection: &str,
        selection: &Selection,
    ) -> DomainResult<ApplyReport> {
        let deps = self.deps()?;
        // One repository for the whole apply, so a workspace switch cannot mix two workspaces.
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        // Held from the recovery to the end, so a check, another apply or a settings change
        // never works on state this apply is about to replace.
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        // An earlier apply that was cut short is settled first.
        deps.tx.recover(&root).map_err(|_| {
            DomainError::Conflict("an earlier apply is not finished yet".to_string())
        })?;
        let loaded = self.load_for_apply(deps, repo, collection, &root)?;
        let prepared = loaded.prepare()?;
        let renderer = (deps.renderer)(&root);
        let mut output = build_apply(&ApplyInputs {
            plan_inputs: loaded.sync_inputs(&prepared, &loaded.local.skips),
            pending_sha256: sha256_hex(loaded.pending_text.as_bytes()),
            selection,
            renderer: renderer.as_ref(),
            existing_paths: &loaded.existing_paths,
            settings: &loaded.settings,
        })
        .map_err(|e| DomainError::InvalidInput(e.to_string()))?;

        // The apply owns the skips only. The interval and the check timing stay as loaded.
        let skips = std::mem::take(&mut output.local.skips);
        output.local = OpenApiLocalState {
            skips,
            ..loaded.local.clone()
        };
        output.local.pending_count = u32::try_from(output.report.remaining).unwrap_or(u32::MAX);
        if output.finalized {
            output.local.last_synced_at = Some(self.now().to_rfc3339());
            output.local.last_synced_updated =
                u32::try_from(output.report.applied.len() + output.report.deleted.len())
                    .unwrap_or(u32::MAX);
            output.local.last_status = Some(SyncStatus::Unchanged);
            output.local.consecutive_failures = 0;
            output.local.not_before = None;
            output.local.last_message = None;
        }

        let extra = vec![
            (SOURCE_REL, Some(to_yaml(&output.source)?)),
            (BASELINE_REL, Some(to_json(&output.baseline)?)),
            (LOCAL_REL, Some(to_json(&output.local)?)),
        ];
        let mut extra = extra;
        if output.finalized {
            // The spec that was just applied becomes the saved copy.
            extra.push((APPLIED_REL, Some(loaded.pending_text.into_bytes())));
            extra.push((PENDING_REL, None));
        }
        // A sync that updates an endpoint drops its reset backup in the same transaction.
        if let Ok(repo) = self.backup_repo() {
            let touched: Vec<String> = output
                .report
                .applied
                .iter()
                .map(|a| a.key.clone())
                .chain(output.report.deleted.iter().cloned())
                .collect();
            match repo.load(&root) {
                Ok(Some(backups)) if backups.entries.iter().any(|e| touched.contains(&e.key)) => {
                    extra.push((BACKUPS_REL, Some(to_json(&backups.without_keys(&touched))?)));
                }
                Ok(_) => {}
                Err(e) => tracing::debug!(error = %e, "the reset backups could not be read"),
            }
        }
        let mut tx = deps.tx.begin(&root)?;
        if let Err(e) = stage_all(tx.as_mut(), &output.files, extra) {
            tx.abort();
            return Err(e);
        }
        if let Err(e) = tx.commit() {
            // The staging area was lost or taken over. A journal may be live in another
            // process, so it is left alone.
            if matches!(e, DomainError::Conflict(_)) {
                return Err(e);
            }
            // A failed commit can leave a journal that recovery finishes or undoes.
            return match deps.tx.recover(&root) {
                Ok(RecoveryOutcome::Finished { .. }) => {
                    self.refresh_count(deps, repo, collection, &root);
                    Ok(output.report)
                }
                Ok(_) => Err(DomainError::Internal(format!("{e}; nothing was changed"))),
                Err(re) => {
                    tracing::warn!(error = %re, "recovery after a failed apply did not finish");
                    Err(DomainError::Internal(format!(
                        "{e}; the apply will be finished or undone the next time Rocket starts"
                    )))
                }
            };
        }
        self.refresh_count(deps, repo, collection, &root);
        Ok(output.report)
    }

    /// Stores the waiting count of the plan as it stands after an apply, with the same logic
    /// as `count_pending`. The caller holds the collection lock. A failure keeps the count the
    /// apply staged from its report.
    pub(crate) fn refresh_count(
        &self,
        deps: &ApplyDeps,
        repo: &dyn CollectionRepository,
        collection: &str,
        root: &Path,
    ) {
        let result = self
            .plan_locked(deps, repo, collection, root, &[])
            .map(|view| count_of(&view))
            .and_then(|count| {
                let locals = self.local_repo()?;
                let mut local = locals.load(root)?.unwrap_or_default();
                local.pending_count = count;
                locals.save(root, &local)
            });
        if let Err(e) = result {
            tracing::debug!(error = %e, "the waiting count could not be refreshed");
        }
    }

    /// What an apply would do with the pending spec, with the digests an approval carries. It
    /// takes the collection lock like `apply` and writes nothing, not even a recovery.
    pub fn plan(&self, collection: &str) -> DomainResult<PlanView> {
        self.plan_linked(collection, &[])
    }

    /// Like `plan`, with the rename links the user chose. The digests match what `apply`
    /// checks when it gets the same links.
    pub fn plan_linked(
        &self,
        collection: &str,
        links: &[(String, String)],
    ) -> DomainResult<PlanView> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        self.plan_locked(deps, repo, collection, &root, links)
    }

    /// The items and variable changes of the current plan that wait for a decision. Skipped
    /// items are not counted, and no pending copy gives `0`.
    pub fn count_pending(&self, collection: &str) -> DomainResult<u32> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        Ok(count_of(&self.plan_locked(
            deps,
            repo,
            collection,
            &root,
            &[],
        )?))
    }

    /// Like `count_pending` for a caller that holds the collection lock and knows the root.
    pub(crate) fn count_pending_locked(&self, root: &Path) -> DomainResult<u32> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        // The plan reads the collection by name, so the name of this root is looked up.
        let name = repo
            .list()?
            .into_iter()
            .map(|s| s.name)
            .find(|n| repo.collection_root_path(n).is_ok_and(|r| r == root))
            .ok_or_else(|| DomainError::NotFound("the collection was not found".to_string()))?;
        Ok(count_of(&self.plan_locked(deps, repo, &name, root, &[])?))
    }

    /// The plan work for a caller that already holds the collection lock.
    pub(crate) fn plan_locked(
        &self,
        deps: &ApplyDeps,
        repo: &dyn CollectionRepository,
        collection: &str,
        root: &Path,
        links: &[(String, String)],
    ) -> DomainResult<PlanView> {
        if self.load(root)?.pending_sha256.is_none() {
            return Ok(PlanView::empty());
        }
        let loaded = self.load_for_apply(deps, repo, collection, root)?;
        let prepared = loaded.prepare()?;
        let inputs = loaded.sync_inputs(&prepared, &loaded.local.skips);
        let plan = plan_with_links(&inputs, links)
            .map_err(|e| DomainError::InvalidInput(e.to_string()))?;
        // A plan without the skips tells which items the user's skips hid.
        let hidden: HashSet<String> = if plan.skipped_hidden == 0 {
            HashSet::new()
        } else {
            let none = BTreeMap::new();
            let full = plan_with_links(&loaded.sync_inputs(&prepared, &none), links)
                .map_err(|e| DomainError::InvalidInput(e.to_string()))?;
            full.items.iter().map(|i| i.key().to_string()).collect()
        };
        let planned: HashSet<&str> = plan.items.iter().map(|i| i.key()).collect();
        let deleted: HashSet<&str> = plan.deleted_by_user.iter().map(String::as_str).collect();
        let by_uid: std::collections::HashMap<&str, &CurrentRequest> =
            loaded.current.iter().map(|c| (c.uid.as_str(), c)).collect();
        let unchanged = loaded
            .source
            .operations
            .iter()
            .filter(|o| {
                !planned.contains(o.key.as_str())
                    && !deleted.contains(o.key.as_str())
                    && !hidden.contains(&o.key)
            })
            .map(|o| match by_uid.get(o.uid.as_str()) {
                Some(cur) => UnchangedView {
                    key: o.key.clone(),
                    name: cur.request.name.clone(),
                    request_path: Some(cur.request_path.clone()),
                },
                None => UnchangedView {
                    key: o.key.clone(),
                    name: o.key.clone(),
                    request_path: Some(o.request_path.clone()),
                },
            })
            .collect();
        Ok(PlanView {
            pending: true,
            item_digests: plan.items.iter().map(item_digest).collect(),
            variable_digests: plan.variable_changes.iter().map(variable_digest).collect(),
            plan,
            unchanged,
        })
    }

    /// The folder of a collection by name, resolved in the pinned active workspace.
    pub fn root_of(&self, collection: &str) -> DomainResult<std::path::PathBuf> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        repo.collection_root_path(collection)
    }

    /// The sync state of a collection by name, resolved in the pinned active workspace.
    pub fn status_of(
        &self,
        collection: &str,
        include_token: bool,
    ) -> DomainResult<crate::SyncStatusView> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        self.status(&repo.collection_root_path(collection)?, include_token)
    }

    /// Finishes or undoes an interrupted apply of one collection.
    pub fn recover(&self, collection: &str) -> DomainResult<RecoveryOutcome> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        self.recover_in(deps, repo, collection)
    }

    fn recover_in(
        &self,
        deps: &ApplyDeps,
        repo: &dyn CollectionRepository,
        collection: &str,
    ) -> DomainResult<RecoveryOutcome> {
        let root = repo.collection_root_path(collection)?;
        let lock = collection_lock(&root);
        let _guard = lock.lock();
        deps.tx.recover(&root)
    }

    /// Runs `recover` for every collection of the active workspace. Collections with nothing to
    /// recover are left out, and a failure is logged and skipped.
    pub fn recover_all(&self) -> Vec<(String, RecoveryOutcome)> {
        let Ok(deps) = self.deps() else {
            return Vec::new();
        };
        // One repository for the whole pass, so a workspace switch cannot mix two workspaces.
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let names = match repo.list() {
            Ok(list) => list,
            Err(e) => {
                tracing::warn!(error = %e, "collections could not be listed for sync recovery");
                return Vec::new();
            }
        };
        let mut out = Vec::new();
        for summary in names {
            match self.recover_in(deps, repo, &summary.name) {
                Ok(RecoveryOutcome::Nothing) => {}
                Ok(outcome) => out.push((summary.name, outcome)),
                Err(e) => tracing::warn!(error = %e, "a sync recovery failed"),
            }
        }
        out
    }

    /// The local choices of a collection. No file means the defaults.
    pub fn local_state(&self, collection: &str) -> DomainResult<OpenApiLocalState> {
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection)?;
        Ok(self.local_repo()?.load(&root)?.unwrap_or_default())
    }
}

#[cfg(test)]
#[path = "openapi_sync_apply_tests.rs"]
mod tests;
