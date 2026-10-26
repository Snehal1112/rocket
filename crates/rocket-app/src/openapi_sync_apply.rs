//! Applies approved spec changes to a collection through one file transaction, and finishes or
//! undoes an apply that a crash cut short. The pure work is `rocket_import::build_apply`; this
//! file loads the inputs, stages the result and commits.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use rocket_collection::file_tx::{CollectionFileTx, CollectionFileTxFactory, RecoveryOutcome};
use rocket_collection::openapi_baseline::{OpenApiBaseline, OpenApiBaselineRepository};
use rocket_collection::openapi_local::{OpenApiLocalRepository, OpenApiLocalState};
use rocket_collection::openapi_source::OpenApiSource;
use rocket_collection::{CollectionItem, CollectionRepository, Folder};
use rocket_import::{
    build_apply, ApplyInputs, ApplyReport, CurrentRequest, FileChangeKind, ParsedSpec,
    RequestRenderer, Selection, SyncInputs,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::openapi_sync_service::{sha256_hex, OpenApiSyncService};

const SOURCE_REL: &str = ".rocket/openapi-source.yml";
const BASELINE_REL: &str = ".rocket/openapi-baseline.json";
const LOCAL_REL: &str = ".rocket/openapi-local.json";
const PENDING_REL: &str = ".rocket/openapi-pending.spec";
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
                out.push(CurrentRequest {
                    uid: request.uid.clone(),
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

fn to_yaml(source: &OpenApiSource) -> DomainResult<Vec<u8>> {
    serde_yaml::to_string(source)
        .map(String::into_bytes)
        .map_err(|e| DomainError::Internal(format!("the source file could not be written: {e}")))
}

fn to_json<T: serde::Serialize>(value: &T) -> DomainResult<Vec<u8>> {
    serde_json::to_vec_pretty(value)
        .map_err(|e| DomainError::Internal(format!("a state file could not be written: {e}")))
}

/// Everything `apply` reads before it builds the changes.
struct Loaded {
    source: OpenApiSource,
    baseline: Option<OpenApiBaseline>,
    local: OpenApiLocalState,
    pending_text: String,
    current: Vec<CurrentRequest>,
    existing_paths: HashSet<String>,
    settings: rocket_collection::CollectionSettings,
}

impl OpenApiSyncService {
    fn deps(&self) -> DomainResult<&ApplyDeps> {
        self.apply_deps.as_ref().ok_or_else(not_set_up)
    }

    fn load_for_apply(&self, deps: &ApplyDeps, name: &str, root: &Path) -> DomainResult<Loaded> {
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
        let collection = deps.collections.get(name)?;
        let mut current = Vec::new();
        flatten(&collection.root, "", &mut current);
        Ok(Loaded {
            baseline: deps.baselines.load(root)?,
            local: deps.locals.load(root)?.unwrap_or_default(),
            existing_paths: deps.tx.list_files(root)?,
            settings: deps.collections.get_settings(name)?,
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
        let root = deps.collections.collection_root_path(collection)?;
        // An earlier apply that was cut short is settled first.
        deps.tx.recover(&root).map_err(|_| {
            DomainError::Conflict("an earlier apply is not finished yet".to_string())
        })?;
        let loaded = self.load_for_apply(deps, collection, &root)?;
        let incoming = ParsedSpec::parse(&loaded.pending_text)
            .map_err(|e| DomainError::InvalidInput(e.to_string()))?;
        let variables: Vec<String> = loaded
            .settings
            .variables
            .iter()
            .map(|v| v.key.clone())
            .collect();
        let base_url = loaded
            .settings
            .variables
            .iter()
            .find(|v| v.key == BASE_URL_KEY)
            .map(|v| v.value.clone());
        let renderer = (deps.renderer)(&root);
        let output = build_apply(&ApplyInputs {
            plan_inputs: SyncInputs {
                source: &loaded.source,
                baseline: loaded.baseline.as_ref(),
                incoming: &incoming,
                current: &loaded.current,
                current_base_url: base_url.as_deref(),
                existing_variables: &variables,
                skips: &loaded.local.skips,
            },
            pending_sha256: sha256_hex(loaded.pending_text.as_bytes()),
            selection,
            renderer: renderer.as_ref(),
            existing_paths: &loaded.existing_paths,
            settings: &loaded.settings,
        })
        .map_err(|e| DomainError::InvalidInput(e.to_string()))?;

        let extra = vec![
            (SOURCE_REL, Some(to_yaml(&output.source)?)),
            (BASELINE_REL, Some(to_json(&output.baseline)?)),
            (LOCAL_REL, Some(to_json(&output.local)?)),
        ];
        let mut extra = extra;
        if output.finalized {
            extra.push((PENDING_REL, None));
        }
        let mut tx = deps.tx.begin(&root)?;
        if let Err(e) = stage_all(tx.as_mut(), &output.files, extra) {
            tx.abort();
            return Err(e);
        }
        if let Err(e) = tx.commit() {
            // A failed commit can leave a journal that recovery finishes or undoes.
            return match deps.tx.recover(&root) {
                Ok(RecoveryOutcome::Finished { .. }) => Ok(output.report),
                Ok(_) => Err(DomainError::Internal(format!("{e}; nothing was changed"))),
                Err(re) => {
                    tracing::warn!(error = %re, "recovery after a failed apply did not finish");
                    Err(DomainError::Internal(format!(
                        "{e}; the apply will be finished or undone the next time Rocket starts"
                    )))
                }
            };
        }
        Ok(output.report)
    }

    /// Finishes or undoes an interrupted apply of one collection.
    pub fn recover(&self, collection: &str) -> DomainResult<RecoveryOutcome> {
        let deps = self.deps()?;
        let root = deps.collections.collection_root_path(collection)?;
        deps.tx.recover(&root)
    }

    /// Runs `recover` for every collection of the active workspace. Collections with nothing to
    /// recover are left out, and a failure is logged and skipped.
    pub fn recover_all(&self) -> Vec<(String, RecoveryOutcome)> {
        let Ok(deps) = self.deps() else {
            return Vec::new();
        };
        let names = match deps.collections.list() {
            Ok(list) => list,
            Err(e) => {
                tracing::warn!(error = %e, "collections could not be listed for sync recovery");
                return Vec::new();
            }
        };
        let mut out = Vec::new();
        for summary in names {
            match self.recover(&summary.name) {
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
        let root = deps.collections.collection_root_path(collection)?;
        Ok(deps.locals.load(&root)?.unwrap_or_default())
    }
}

#[cfg(test)]
#[path = "openapi_sync_apply_tests.rs"]
mod tests;
