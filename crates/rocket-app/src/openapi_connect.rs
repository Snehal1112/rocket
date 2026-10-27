//! Connecting a hand-made collection to an OpenAPI spec. `preview` fetches the spec at the
//! link the user typed, matches its operations to the requests and writes nothing.

use std::collections::HashSet;

use rocket_collection::openapi_baseline::{
    BaselineOperation, OpenApiBaseline, BASELINE_FILE_VERSION,
};
use rocket_collection::openapi_local::OpenApiLocalState;
use rocket_collection::openapi_source::SyncInterval;
use rocket_collection::settings::CollectionVariable;
use rocket_collection::CollectionRepository;
use rocket_import::{
    baseline_hash, build_apply, fetch_spec_bundle_from, item_digest, match_requests,
    plan_with_links, source_record, variable_digest, ApplyInputs, ApprovedItem, FileChangeKind,
    GroupBy, ImportError, ImportReport, ImportedOperation, MatchRequest, OpenApiImportOutcome,
    ParsedSpec, Selection, SkipRequest, SpecOperationView, SyncInputs, SyncItem, VariableApproval,
    CONVERTER_VERSION,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::collection_lock::collection_lock;
use crate::openapi_sync_apply::{
    flatten_requests, to_json, to_yaml, APPLIED_REL, BASELINE_REL, LOCAL_REL, SOURCE_REL,
};
use crate::openapi_sync_service::{sha256_hex, OpenApiSyncService};

/// A request of the collection as the Connect screens show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectRequestRow {
    pub uid: String,
    pub request_path: String,
    pub method: String,
    pub url: String,
    pub name: String,
}

/// An operation of the spec as the Connect screens show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectOperationRow {
    pub key: String,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectPair {
    pub request: ConnectRequestRow,
    pub operation: ConnectOperationRow,
}

/// Requests and operations that all match each other, for the user to sort out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectAmbiguous {
    pub requests: Vec<ConnectRequestRow>,
    pub operations: Vec<ConnectOperationRow>,
}

/// What a spec link would connect to, before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectPreview {
    pub spec_title: String,
    pub spec_version: Option<String>,
    /// `request_digest` of the collection as the preview saw it. Confirm sends it back.
    pub digest: String,
    /// Hash of the bundled spec the preview matched. Confirm sends it back, so a spec that
    /// changed in between asks for a new preview.
    pub spec_sha256: String,
    pub matched: Vec<ConnectPair>,
    pub suggested: Vec<ConnectPair>,
    pub in_spec_only: Vec<ConnectOperationRow>,
    pub yours_only: Vec<ConnectRequestRow>,
    pub ambiguous: Vec<ConnectAmbiguous>,
}

const STALE_COLLECTION: &str = "The collection changed. Run the preview again.";
pub(crate) const STALE_SPEC: &str = "The spec changed. Run the preview again.";
const CHOSEN_TWICE: &str = "A request or an operation is chosen twice.";
pub(crate) const APPLY_RUNNING: &str = "A sync is running. Try again when it is done.";
const FAILED: &str = "Connecting failed. Nothing was changed.";
const WARN_CHECKS: &str = "Checks are not enabled yet.";
const WARN_TOKEN: &str = "The token was not saved.";
const BASE_URL_KEY: &str = "baseUrl";
const ALREADY_CONNECTED: &str = "This collection is already connected to an OpenAPI spec.";
pub(crate) const NEEDS_LINK: &str = "Enter the spec link.";
pub(crate) const NOT_A_SPEC: &str = "The file is not a supported OpenAPI spec.";
const COULD_NOT_LOAD: &str = "The spec could not be loaded.";

/// What the user decided for one request or operation in the Initial sync review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectRow {
    /// Tie the request to the operation. Its current content counts as the user's edit.
    Link { uid: String, key: String },
    /// Create a request for a spec-only operation.
    Add { key: String },
    /// Hide a spec-only operation, so it never comes back as new.
    Skip { key: String },
    /// Leave a request of the user's alone.
    Keep { uid: String },
}

/// What a connect did. The warnings are fixed sentences about steps that ran after the files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConnectOutcome {
    pub linked: u32,
    pub added: u32,
    pub skipped: u32,
    pub warnings: Vec<String>,
}

/// A fixed error for a failure of the collection files or the transaction, whose own message
/// can carry a path.
fn infra_error(error: DomainError) -> DomainError {
    match error {
        DomainError::NotFound(_) => {
            DomainError::NotFound("The collection was not found.".to_string())
        }
        DomainError::Conflict(_) => DomainError::Conflict(APPLY_RUNNING.to_string()),
        _ => DomainError::Internal(FAILED.to_string()),
    }
}

/// A fixed sentence for a failed fetch or bundle. The fetch messages are already fixed, and
/// anything else (a parse error with a path, say) becomes a general sentence.
pub(crate) fn safe_import_message(error: ImportError) -> String {
    match error {
        ImportError::SpecUnavailable(message) => message,
        ImportError::NotAnOpenApiSpec(_) => NOT_A_SPEC.to_string(),
        _ => COULD_NOT_LOAD.to_string(),
    }
}

/// A hash over the uid, path, method and URL of every HTTP request, in a fixed order. Confirm
/// compares it with the preview's, so a collection edited in between asks for a new preview.
pub fn request_digest(requests: &[MatchRequest]) -> String {
    let mut lines: Vec<String> = requests
        .iter()
        .filter(|r| r.http)
        .map(|r| format!("{}\0{}\0{}\0{}", r.uid, r.request_path, r.method, r.url))
        .collect();
    lines.sort();
    sha256_hex(lines.join("\n").as_bytes())
}

pub(crate) fn match_requests_of(
    repo: &dyn CollectionRepository,
    collection: &str,
) -> DomainResult<Vec<MatchRequest>> {
    Ok(
        flatten_requests(&repo.get(collection).map_err(infra_error)?.root)
            .into_iter()
            .map(|c| MatchRequest {
                uid: c.uid,
                request_path: c.request_path,
                method: c.request.method.to_string(),
                url: c.request.url,
                name: c.request.name,
                http: true,
            })
            .collect(),
    )
}

fn request_row(r: &MatchRequest) -> ConnectRequestRow {
    ConnectRequestRow {
        uid: r.uid.clone(),
        request_path: r.request_path.clone(),
        method: r.method.to_uppercase(),
        url: r.url.clone(),
        name: r.name.clone(),
    }
}

fn operation_row(o: &SpecOperationView) -> ConnectOperationRow {
    ConnectOperationRow {
        key: o.key.clone(),
        method: o.method.clone(),
        path: o.path.clone(),
        summary: o.summary.clone(),
        tag: o.tag.clone(),
    }
}

impl OpenApiSyncService {
    /// Fetches the spec at `link` and matches it to the requests of `collection`. The link and
    /// the optional token are the ones the user typed: no consent is granted, no token is
    /// stored and nothing is written.
    pub async fn preview(
        &self,
        collection: &str,
        link: &str,
        token: Option<&str>,
    ) -> DomainResult<ConnectPreview> {
        let link = link.trim();
        if link.is_empty() {
            return Err(DomainError::InvalidInput(NEEDS_LINK.to_string()));
        }
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection).map_err(infra_error)?;
        let requests = {
            let lock = collection_lock(&root);
            let _guard = lock.lock();
            if self.sources.load(&root).map_err(infra_error)?.is_some() {
                return Err(DomainError::Conflict(ALREADY_CONNECTED.to_string()));
            }
            match_requests_of(repo, collection).map_err(infra_error)?
        };
        let fetched = fetch_spec_bundle_from(&self.spec_sources(), &root, link, token)
            .await
            .map_err(|e| DomainError::InvalidInput(safe_import_message(e)))?;
        let spec = ParsedSpec::parse(&fetched.bundle.text)
            .map_err(|_| DomainError::InvalidInput(NOT_A_SPEC.to_string()))?;
        let views = spec.operation_views();
        let result = match_requests(&requests, &views, spec.servers());

        let req = |uid: &str| requests.iter().find(|r| r.uid == uid).map(request_row);
        let op = |key: &str| views.iter().find(|o| o.key == key).map(operation_row);
        let pairs = |list: &[rocket_import::MatchPair]| -> Vec<ConnectPair> {
            list.iter()
                .filter_map(|p| {
                    Some(ConnectPair {
                        request: req(&p.uid)?,
                        operation: op(&p.key)?,
                    })
                })
                .collect()
        };
        Ok(ConnectPreview {
            spec_title: spec.title().to_string(),
            spec_version: spec.version().map(str::to_string),
            digest: request_digest(&requests),
            spec_sha256: sha256_hex(fetched.bundle.text.as_bytes()),
            matched: pairs(&result.matched),
            suggested: pairs(&result.suggested),
            in_spec_only: result.in_spec_only.iter().filter_map(|k| op(k)).collect(),
            yours_only: result.yours_only.iter().filter_map(|u| req(u)).collect(),
            ambiguous: result
                .ambiguous
                .iter()
                .map(|g| ConnectAmbiguous {
                    requests: g.uids.iter().filter_map(|u| req(u)).collect(),
                    operations: g.keys.iter().filter_map(|k| op(k)).collect(),
                })
                .collect(),
        })
    }

    /// Connects the collection to the spec at `link` in one file transaction, then approves the
    /// typed link, sets the check interval and stores the token, in that order. A failure after
    /// the files were committed adds a warning and never undoes them. An operation no row
    /// mentions is skipped, like one the user chose to skip.
    #[allow(clippy::too_many_arguments)]
    pub async fn confirm(
        &self,
        collection: &str,
        link: &str,
        token: Option<&str>,
        rows: Vec<ConnectRow>,
        group_by: GroupBy,
        interval: Option<SyncInterval>,
        digest: &str,
        spec_sha256: &str,
    ) -> DomainResult<ConnectOutcome> {
        let link = link.trim();
        if link.is_empty() {
            return Err(DomainError::InvalidInput(NEEDS_LINK.to_string()));
        }
        let deps = self.deps()?;
        let pinned = deps.collections.pinned();
        let repo: &dyn CollectionRepository =
            pinned.as_deref().unwrap_or(deps.collections.as_ref());
        let root = repo.collection_root_path(collection).map_err(infra_error)?;
        let fetched = fetch_spec_bundle_from(&self.spec_sources(), &root, link, token)
            .await
            .map_err(|e| DomainError::InvalidInput(safe_import_message(e)))?;
        let spec = ParsedSpec::parse(&fetched.bundle.text)
            .map_err(|_| DomainError::InvalidInput(NOT_A_SPEC.to_string()))?;

        if sha256_hex(fetched.bundle.text.as_bytes()) != spec_sha256 {
            return Err(DomainError::Conflict(STALE_SPEC.to_string()));
        }

        let mut outcome = ConnectOutcome::default();
        {
            let lock = collection_lock(&root);
            let _guard = lock.lock();
            if self.sources.load(&root).map_err(infra_error)?.is_some() {
                return Err(DomainError::Conflict(ALREADY_CONNECTED.to_string()));
            }
            if deps
                .tx
                .recover_if_idle(&root)
                .map_err(infra_error)?
                .is_none()
            {
                return Err(DomainError::Conflict(APPLY_RUNNING.to_string()));
            }
            let requests = match_requests_of(repo, collection).map_err(infra_error)?;
            if request_digest(&requests) != digest {
                return Err(DomainError::Conflict(STALE_COLLECTION.to_string()));
            }
            let views = spec.operation_views();
            let spec_keys: HashSet<&str> = views.iter().map(|o| o.key.as_str()).collect();
            let known_uids: HashSet<&str> = requests.iter().map(|r| r.uid.as_str()).collect();

            let mut links: Vec<(String, String)> = Vec::new();
            let mut adds: HashSet<String> = HashSet::new();
            let mut skips: HashSet<String> = HashSet::new();
            let mut seen_keys: HashSet<String> = HashSet::new();
            let mut seen_uids: HashSet<String> = HashSet::new();
            for row in &rows {
                let (uid, key) = match row {
                    ConnectRow::Link { uid, key } => (Some(uid), Some(key)),
                    ConnectRow::Add { key } | ConnectRow::Skip { key } => (None, Some(key)),
                    ConnectRow::Keep { uid } => (Some(uid), None),
                };
                if key.is_some_and(|k| !spec_keys.contains(k.as_str()))
                    || uid.is_some_and(|u| !known_uids.contains(u.as_str()))
                {
                    return Err(DomainError::InvalidInput(STALE_SPEC.to_string()));
                }
                if key.is_some_and(|k| !seen_keys.insert(k.clone()))
                    || uid.is_some_and(|u| !seen_uids.insert(u.clone()))
                {
                    return Err(DomainError::InvalidInput(CHOSEN_TWICE.to_string()));
                }
                match row {
                    ConnectRow::Link { uid, key } => links.push((uid.clone(), key.clone())),
                    ConnectRow::Add { key } => {
                        adds.insert(key.clone());
                    }
                    ConnectRow::Skip { key } => {
                        skips.insert(key.clone());
                    }
                    ConnectRow::Keep { .. } => {}
                }
            }
            for key in &spec_keys {
                if !seen_keys.contains(*key) {
                    skips.insert((*key).to_string());
                }
            }

            let current = flatten_requests(&repo.get(collection).map_err(infra_error)?.root);
            let mut operations = Vec::new();
            for (uid, key) in &links {
                let (Some(cur), Some(request)) = (
                    current.iter().find(|c| c.uid == *uid),
                    spec.converted_request(key),
                ) else {
                    return Err(DomainError::InvalidInput(STALE_SPEC.to_string()));
                };
                operations.push(ImportedOperation {
                    key: key.clone(),
                    request_path: cur.request_path.clone(),
                    uid: uid.clone(),
                    baseline_hash: baseline_hash(&request),
                    baseline: request,
                });
            }
            let base_url = spec.servers().first().cloned().unwrap_or_default();
            let imported = OpenApiImportOutcome {
                report: ImportReport::default(),
                collection: collection.to_string(),
                operations,
                base_url: base_url.clone(),
            };
            let source = source_record(
                &fetched.location,
                group_by,
                fetched.bundle.text.as_bytes(),
                fetched.etag.clone(),
                fetched.last_modified.clone(),
                fetched.bundle.refs.clone(),
                fetched.bundle.fetch_failed,
                &imported,
            );
            let baseline = OpenApiBaseline {
                version: BASELINE_FILE_VERSION,
                converter_version: CONVERTER_VERSION,
                base_url: base_url.clone(),
                operations: imported
                    .operations
                    .iter()
                    .map(|o| BaselineOperation {
                        key: o.key.clone(),
                        request: o.baseline.clone(),
                    })
                    .collect(),
            };

            // Added requests and linked baselines point at {{baseUrl}}, so the variable has to exist.
            let mut settings = repo.get_settings(collection).map_err(infra_error)?;
            let mut settings_changed = false;
            if (!adds.is_empty() || !links.is_empty())
                && settings.variables.iter().all(|v| v.key != BASE_URL_KEY)
            {
                settings.variables.push(CollectionVariable {
                    key: BASE_URL_KEY.to_string(),
                    value: base_url.clone(),
                    initial_value: base_url,
                    enabled: true,
                    secret: false,
                });
                settings_changed = true;
            }
            let variable_names: Vec<String> =
                settings.variables.iter().map(|v| v.key.clone()).collect();
            let current_base = settings
                .variables
                .iter()
                .find(|v| v.key == BASE_URL_KEY)
                .map(|v| v.value.clone());
            let local = OpenApiLocalState::default();
            let inputs = SyncInputs {
                source: &source,
                baseline: Some(&baseline),
                incoming: &spec,
                current: &current,
                current_base_url: current_base.as_deref(),
                existing_variables: &variable_names,
                skips: &local.skips,
            };
            let plan = plan_with_links(&inputs, &[])
                .map_err(|_| DomainError::Internal(FAILED.to_string()))?;
            let mut selection = Selection::default();
            for item in &plan.items {
                if let SyncItem::New {
                    key, fingerprint, ..
                } = item
                {
                    if adds.contains(key) {
                        selection.items.push(ApprovedItem {
                            key: key.clone(),
                            digest: item_digest(item),
                            choices: Default::default(),
                        });
                    } else {
                        selection.skips.push(SkipRequest {
                            key: key.clone(),
                            fingerprint: fingerprint.clone(),
                        });
                    }
                }
            }
            selection.variables = plan
                .variable_changes
                .iter()
                .map(|c| VariableApproval {
                    name: c.name.clone(),
                    digest: variable_digest(c),
                })
                .collect();
            let renderer = (deps.renderer)(&root);
            let existing_paths = deps.tx.list_files(&root).map_err(infra_error)?;
            let out = build_apply(&ApplyInputs {
                plan_inputs: inputs,
                pending_sha256: sha256_hex(fetched.bundle.text.as_bytes()),
                selection: &selection,
                renderer: renderer.as_ref(),
                existing_paths: &existing_paths,
                settings: &settings,
            })
            .map_err(|_| DomainError::Internal(FAILED.to_string()))?;
            if !out.finalized || !out.report.stale.is_empty() {
                return Err(DomainError::Internal(FAILED.to_string()));
            }

            let mut tx = deps.tx.begin(&root).map_err(infra_error)?;
            let staged = (|| -> DomainResult<()> {
                for file in &out.files {
                    match &file.kind {
                        FileChangeKind::Put(bytes) => tx.stage_put(&file.rel_path, bytes)?,
                        FileChangeKind::Delete => tx.stage_delete(&file.rel_path)?,
                    }
                }
                if settings_changed {
                    let (rel, bytes) = renderer.render_settings(&settings)?;
                    if out.files.iter().all(|f| f.rel_path != rel) {
                        tx.stage_put(&rel, &bytes)?;
                    }
                }
                tx.stage_put(SOURCE_REL, &to_yaml(&out.source)?)?;
                tx.stage_put(BASELINE_REL, &to_json(&out.baseline)?)?;
                tx.stage_put(LOCAL_REL, &to_json(&out.local)?)?;
                tx.stage_put(APPLIED_REL, fetched.bundle.text.as_bytes())?;
                Ok(())
            })();
            if let Err(e) = staged {
                tx.abort();
                tracing::debug!(error = %e, "a connect could not be staged");
                return Err(DomainError::Internal(FAILED.to_string()));
            }
            if let Err(e) = tx.commit() {
                tracing::debug!(error = %e, "a connect could not be committed");
                return Err(DomainError::Internal(FAILED.to_string()));
            }
            outcome.linked = u32::try_from(links.len()).unwrap_or(u32::MAX);
            outcome.added = u32::try_from(out.report.applied.len()).unwrap_or(u32::MAX);
            outcome.skipped = u32::try_from(selection.skips.len()).unwrap_or(u32::MAX);
        };

        // The files are committed. What follows only adds warnings, and consent comes last of
        // the three that need it, so a failed step never leaves a half-enabled check.
        let mut consented = false;
        if self.grant_consent_for_link(&root, link).is_ok() {
            consented = true;
            if let Some(interval) = interval.filter(|i| *i != SyncInterval::Off) {
                if self.set_interval(&root, interval).is_err() {
                    outcome.warnings.push(WARN_CHECKS.to_string());
                }
            }
        } else if interval.is_some_and(|i| i != SyncInterval::Off) {
            outcome.warnings.push(WARN_CHECKS.to_string());
        }
        if let Some(token) = token.map(str::trim).filter(|t| !t.is_empty()) {
            if !consented || self.set_token(&root, token).is_err() {
                outcome.warnings.push(WARN_TOKEN.to_string());
            }
        }
        Ok(outcome)
    }
}
