//! Commands for the Spec sync tab: the status and the plan (read-only), and the actions: check
//! now, interval, token, consent and apply. Each runs on a blocking task.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use std::collections::BTreeMap;

use rocket_app::{
    safe_message, BadgeView, ChangeRow, ChangesView, CheckOutcome, OpenApiSyncService,
    OverviewView, PlanView, SyncStatusView,
};
use rocket_collection::openapi_source::{SyncInterval, SyncStatus};
use rocket_collection::ConsentState;
use rocket_http::SpecFetcher;
use rocket_import::{
    AppliedKind, ApplyReport, ApprovedItem, Choice, FieldChange, FieldOutcome, Selection,
    SkipRequest, SyncItem, VariableApproval, VariableChange, VariableKind,
};
use rocket_shared::error::DomainError;
use rocket_shared::events::EventPublisher;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::import::make_sync_service;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastStatusDto {
    /// `unchanged`, `changed` or `failed`.
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusDto {
    pub location: String,
    /// `off`, `every_5_minutes`, `every_15_minutes`, `every_30_minutes`, `hourly`, `daily`,
    /// `weekly` or `monthly`.
    pub interval: &'static str,
    /// True when the stored interval was not understood. `interval` is `off` then.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub interval_unknown: bool,
    pub last_checked_at: Option<String>,
    pub last_status: Option<LastStatusDto>,
    pub has_pending: bool,
    /// Left out unless the caller asked for it with `includeToken`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_token: Option<bool>,
    /// `granted`, `missing` or `locationChanged`.
    pub consent: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    /// The first 12 hex characters of the pending spec's hash. Left out without a pending copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChangeDto {
    pub field: String,
    /// `takeSpec`, `keepYours` or `conflict`.
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yours: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncItemDto {
    New {
        key: String,
        name: String,
        digest: String,
        fingerprint: String,
        folder: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        possibly_renamed_from: Option<String>,
    },
    Changed {
        key: String,
        name: String,
        digest: String,
        fingerprint: String,
        request_path: String,
        cautious: bool,
        changes: Vec<FieldChangeDto>,
    },
    Removed {
        key: String,
        name: String,
        digest: String,
        fingerprint: String,
        request_path: String,
        /// `null` when it cannot be known, which the apply treats as edited.
        edited: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        possibly_renamed_to: Option<String>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableChangeDto {
    pub name: String,
    /// `baseUrl` or `addSecurity`.
    pub kind: &'static str,
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yours: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnchangedDto {
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlanDto {
    pub pending: bool,
    pub items: Vec<SyncItemDto>,
    pub variable_changes: Vec<VariableChangeDto>,
    pub unchanged: Vec<UnchangedDto>,
    pub skipped_hidden: usize,
    pub deleted_by_user: Vec<String>,
    pub cautious: bool,
    pub warnings: Vec<String>,
}

fn interval_name(interval: SyncInterval) -> &'static str {
    match interval {
        SyncInterval::Off => "off",
        SyncInterval::Min5 => "every_5_minutes",
        SyncInterval::Min15 => "every_15_minutes",
        SyncInterval::Min30 => "every_30_minutes",
        SyncInterval::Hourly => "hourly",
        SyncInterval::Daily => "daily",
        SyncInterval::Weekly => "weekly",
        SyncInterval::Monthly => "monthly",
    }
}

fn status_dto(view: SyncStatusView) -> SyncStatusDto {
    SyncStatusDto {
        location: view.location,
        interval: interval_name(view.interval),
        interval_unknown: view.interval_unknown,
        last_checked_at: view.last_checked_at,
        last_status: view.last_status.map(|s| match s {
            SyncStatus::Unchanged => LastStatusDto {
                state: "unchanged",
                code: None,
            },
            SyncStatus::Changed => LastStatusDto {
                state: "changed",
                code: None,
            },
            SyncStatus::Failed { code } => LastStatusDto {
                state: "failed",
                code: Some(code),
            },
        }),
        has_pending: view.has_pending,
        has_token: view.has_token,
        consent: match view.consent {
            ConsentState::Granted => "granted",
            ConsentState::Missing => "missing",
            ConsentState::LocationChanged => "locationChanged",
        },
        last_message: view.last_message,
        pending_id: view.pending_id,
    }
}

fn field_change_dto(change: &FieldChange) -> FieldChangeDto {
    FieldChangeDto {
        field: change.field.clone(),
        outcome: match change.outcome {
            FieldOutcome::TakeSpec => "takeSpec",
            FieldOutcome::KeepYours => "keepYours",
            FieldOutcome::Conflict => "conflict",
        },
        baseline: change.baseline.clone(),
        yours: change.yours.clone(),
        spec: change.spec.clone(),
    }
}

fn item_dto(item: &SyncItem, digest: &str) -> SyncItemDto {
    let digest = digest.to_string();
    match item {
        SyncItem::New {
            key,
            name,
            folder,
            fingerprint,
            possibly_renamed_from,
            ..
        } => SyncItemDto::New {
            key: key.clone(),
            name: name.clone(),
            digest,
            fingerprint: fingerprint.clone(),
            folder: folder.clone(),
            possibly_renamed_from: possibly_renamed_from.clone(),
        },
        SyncItem::Changed {
            key,
            name,
            request_path,
            merge,
            fingerprint,
            ..
        } => SyncItemDto::Changed {
            key: key.clone(),
            name: name.clone(),
            digest,
            fingerprint: fingerprint.clone(),
            request_path: request_path.clone(),
            cautious: merge.cautious,
            changes: merge.changes.iter().map(field_change_dto).collect(),
        },
        SyncItem::Removed {
            key,
            name,
            request_path,
            edited,
            possibly_renamed_to,
            ..
        } => SyncItemDto::Removed {
            key: key.clone(),
            name: name.clone(),
            digest,
            fingerprint: item.fingerprint().to_string(),
            request_path: request_path.clone(),
            edited: *edited,
            possibly_renamed_to: possibly_renamed_to.clone(),
        },
    }
}

fn variable_dto(change: &VariableChange, digest: &str) -> VariableChangeDto {
    VariableChangeDto {
        name: change.name.clone(),
        kind: match change.kind {
            VariableKind::BaseUrl => "baseUrl",
            VariableKind::AddSecurity => "addSecurity",
        },
        digest: digest.to_string(),
        baseline: change.baseline.clone(),
        yours: change.yours.clone(),
        spec: change.spec.clone(),
    }
}

fn plan_dto(view: PlanView) -> SyncPlanDto {
    let items = view
        .plan
        .items
        .iter()
        .zip(&view.item_digests)
        .map(|(item, digest)| item_dto(item, digest))
        .collect();
    let variable_changes = view
        .plan
        .variable_changes
        .iter()
        .zip(&view.variable_digests)
        .map(|(change, digest)| variable_dto(change, digest))
        .collect();
    SyncPlanDto {
        pending: view.pending,
        items,
        variable_changes,
        unchanged: view
            .unchanged
            .into_iter()
            .map(|u| UnchangedDto {
                key: u.key,
                name: u.name,
                request_path: u.request_path,
            })
            .collect(),
        skipped_hidden: view.plan.skipped_hidden,
        deleted_by_user: view.plan.deleted_by_user,
        cautious: view.plan.cautious,
        warnings: view.plan.warnings,
    }
}

/// A collection without an OpenAPI source gives `None`. An unknown collection name is NotFound
/// too, so it gives `None` on purpose. Any other error is kept.
fn status_or_none(
    result: Result<SyncStatusView, DomainError>,
) -> Result<Option<SyncStatusDto>, DomainError> {
    match result {
        Ok(view) => Ok(Some(status_dto(view))),
        Err(DomainError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// What a caller sees when a blocking task panics or is cancelled. The panic payload stays out.
const TASK_FAILED: &str = "The sync task failed. Try again.";

/// Runs a blocking service call off the async runtime.
async fn run_blocking<T: Send + 'static>(
    workspace_path: Arc<Mutex<PathBuf>>,
    fetcher: Arc<dyn SpecFetcher>,
    call: impl FnOnce(OpenApiSyncService) -> Result<T, DomainError> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || call(make_sync_service(workspace_path, fetcher)))
        .await
        .map_err(|_| TASK_FAILED.to_string())?
        .map_err(|e| e.to_string())
}

/// The counts of the Overview tab. `inSync` and `changedByYou` are null when unknown.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewDto {
    pub endpoints: u32,
    pub in_sync: Option<u32>,
    pub changed_by_you: Option<u32>,
    pub updates_waiting: u32,
    pub spec_version: Option<String>,
    pub last_synced_at: Option<String>,
    pub last_synced_updated: u32,
}

fn overview_dto(view: OverviewView) -> OverviewDto {
    OverviewDto {
        endpoints: view.endpoints,
        in_sync: view.in_sync,
        changed_by_you: view.changed_by_you,
        updates_waiting: view.updates_waiting,
        spec_version: view.spec_version,
        last_synced_at: view.last_synced_at,
        last_synced_updated: view.last_synced_updated,
    }
}

/// One request that differs from what the spec produced.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRowDto {
    /// The operation key, or null for a request the user added.
    pub key: Option<String>,
    pub uid: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub request_path: Option<String>,
    pub fields: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesDto {
    pub baseline_trusted: bool,
    pub modified: Vec<ChangeRowDto>,
    pub added: Vec<ChangeRowDto>,
    pub deleted: Vec<ChangeRowDto>,
}

fn change_row_dto(row: ChangeRow) -> ChangeRowDto {
    ChangeRowDto {
        key: row.key,
        uid: row.uid,
        name: row.name,
        method: row.method,
        url: row.url,
        request_path: row.request_path,
        fields: row.fields,
    }
}

fn changes_dto(view: ChangesView) -> ChangesDto {
    ChangesDto {
        baseline_trusted: view.baseline_trusted,
        modified: view.modified.into_iter().map(change_row_dto).collect(),
        added: view.added.into_iter().map(change_row_dto).collect(),
        deleted: view.deleted.into_iter().map(change_row_dto).collect(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadgeDto {
    pub collection: String,
    pub waiting: u32,
}

fn badge_dto(view: BadgeView) -> BadgeDto {
    BadgeDto {
        collection: view.collection,
        waiting: view.waiting,
    }
}

/// The Overview counts of a collection. Reads local state only.
#[tauri::command]
pub async fn openapi_sync_overview(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<OverviewDto, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| service.overview(&collection).map(overview_dto),
    )
    .await
}

/// The requests the user changed, added or deleted since the last import or sync.
#[tauri::command]
pub async fn openapi_sync_changes(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<ChangesDto, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| service.changes(&collection).map(changes_dto),
    )
    .await
}

/// The collections with updates waiting, for the sidebar badges.
#[tauri::command]
pub async fn openapi_sync_badges(
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<Vec<BadgeDto>, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| Ok(service.badges().into_iter().map(badge_dto).collect()),
    )
    .await
}

/// The sync status of a collection, or `None` when it has no OpenAPI source. `hasToken` is
/// filled only when `includeToken` is true.
#[tauri::command]
pub async fn openapi_sync_status(
    collection: String,
    include_token: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<Option<SyncStatusDto>, String> {
    // The keychain is read only when the caller asks for the token state.
    let include_token = include_token.unwrap_or(false);
    let result = run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| status_or_none(service.status_of(&collection, include_token)),
    )
    .await?;
    Ok(result)
}

/// What applying the pending spec of a collection would do. Writes nothing.
#[tauri::command]
pub async fn openapi_sync_plan(
    collection: String,
    links: Option<Vec<[String; 2]>>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<SyncPlanDto, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| {
            let links = links_from_dto(links.unwrap_or_default());
            service.plan_linked(&collection, &links).map(plan_dto)
        },
    )
    .await
}

/// The result of a check, for the tab.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcomeDto {
    /// `unchanged`, `changed` or `failed`.
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
}

fn check_outcome_dto(outcome: CheckOutcome) -> CheckOutcomeDto {
    match outcome {
        CheckOutcome::Unchanged => CheckOutcomeDto {
            state: "unchanged",
            code: None,
            message: None,
            retry_after_secs: None,
        },
        CheckOutcome::Changed { .. } => CheckOutcomeDto {
            state: "changed",
            code: None,
            message: None,
            retry_after_secs: None,
        },
        // The message is a fixed sentence for the code. The outcome's own text may come from
        // the network, so it is never forwarded.
        CheckOutcome::Failed {
            code,
            retry_after_secs,
            ..
        } => CheckOutcomeDto {
            state: "failed",
            message: Some(safe_message(&code).to_string()),
            code: Some(code),
            retry_after_secs,
        },
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovedItemDto {
    pub key: String,
    pub digest: String,
    /// Field name to `spec` or `yours`.
    #[serde(default)]
    pub choices: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkipDto {
    pub key: String,
    pub fingerprint: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableApprovalDto {
    pub name: String,
    pub digest: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionDto {
    #[serde(default)]
    pub items: Vec<ApprovedItemDto>,
    #[serde(default)]
    pub skips: Vec<SkipDto>,
    #[serde(default)]
    pub variables: Vec<VariableApprovalDto>,
    /// `[oldKey, newKey]` pairs, in the order the user linked them.
    #[serde(default)]
    pub rename_links: Vec<[String; 2]>,
}

fn links_from_dto(links: Vec<[String; 2]>) -> Vec<(String, String)> {
    links.into_iter().map(|[old, new]| (old, new)).collect()
}

fn choice_from_text(field: &str, text: &str) -> Result<Choice, String> {
    match text {
        "spec" => Ok(Choice::Spec),
        "yours" => Ok(Choice::Yours),
        other => Err(format!(
            "Unknown choice \"{other}\" for the field \"{field}\". Use \"spec\" or \"yours\"."
        )),
    }
}

fn selection_from_dto(dto: SelectionDto) -> Result<Selection, String> {
    let mut items = Vec::with_capacity(dto.items.len());
    for item in dto.items {
        let mut choices = BTreeMap::new();
        for (field, text) in &item.choices {
            choices.insert(field.clone(), choice_from_text(field, text)?);
        }
        items.push(ApprovedItem {
            key: item.key,
            digest: item.digest,
            choices,
        });
    }
    Ok(Selection {
        items,
        skips: dto
            .skips
            .into_iter()
            .map(|s| SkipRequest {
                key: s.key,
                fingerprint: s.fingerprint,
            })
            .collect(),
        variables: dto
            .variables
            .into_iter()
            .map(|v| VariableApproval {
                name: v.name,
                digest: v.digest,
            })
            .collect(),
        rename_links: links_from_dto(dto.rename_links),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedItemDto {
    pub key: String,
    /// `created` or `updated`.
    pub kind: &'static str,
    pub request_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReportDto {
    pub applied: Vec<AppliedItemDto>,
    pub stale: Vec<String>,
    pub not_rewritable: Vec<String>,
    pub deleted: Vec<String>,
    pub detached: Vec<String>,
    pub skipped: usize,
    pub remaining: usize,
}

fn report_dto(report: ApplyReport) -> ApplyReportDto {
    ApplyReportDto {
        applied: report
            .applied
            .into_iter()
            .map(|a| AppliedItemDto {
                key: a.key,
                kind: match a.kind {
                    AppliedKind::Created => "created",
                    AppliedKind::Updated => "updated",
                },
                request_path: a.request_path,
            })
            .collect(),
        stale: report.stale,
        not_rewritable: report.not_rewritable,
        deleted: report.deleted,
        detached: report.detached,
        skipped: report.skipped,
        remaining: report.remaining,
    }
}

const UNKNOWN_INTERVAL: &str = "Unknown interval. Use off, every_5_minutes, every_15_minutes, every_30_minutes, hourly, daily, weekly or monthly.";

fn interval_from_name(name: &str) -> Result<SyncInterval, String> {
    SyncInterval::parse(name).ok_or_else(|| UNKNOWN_INTERVAL.to_string())
}

/// Clear sentences for the two ways an approval can be refused.
fn grant_error_text(error: DomainError) -> String {
    match error {
        DomainError::Conflict(_) => {
            "The spec link changed since it was shown. Check the link and approve it again."
                .to_string()
        }
        DomainError::InvalidInput(_) => {
            "This spec location cannot be approved because its link is ambiguous.".to_string()
        }
        other => other.to_string(),
    }
}

/// Checks the spec of a collection now. A collection that is not approved on this machine
/// answers `failed` with the code `consent_required`.
#[tauri::command]
pub async fn openapi_sync_check_now(
    collection: String,
    app: tauri::AppHandle,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<CheckOutcomeDto, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| {
            let root = service.root_of(&collection)?;
            // The whole check runs on a blocking thread. It holds no std guard across an
            // await, and its fetches still run on the runtime.
            let result = tauri::async_runtime::block_on(service.check(&root));
            // The same refresh signal the background check sends.
            let status = match &result {
                Ok(CheckOutcome::Unchanged) => "unchanged",
                Ok(CheckOutcome::Changed { .. }) => "changed",
                _ => "failed",
            };
            crate::tauri_event_bus::TauriEventBus::new(app).publish(
                rocket_shared::events::DomainEvent::OpenApiSyncChecked {
                    collection,
                    status: status.to_string(),
                },
            );
            result.map(check_outcome_dto)
        },
    )
    .await
}

/// Publishes `OpenApiSyncApplied` once for a finished apply, whatever its report holds. A call
/// that failed before doing anything publishes nothing.
fn publish_applied(
    events: &dyn EventPublisher,
    collection: &str,
    result: &Result<ApplyReport, DomainError>,
) {
    if let Ok(report) = result {
        events.publish(rocket_shared::events::DomainEvent::OpenApiSyncApplied {
            collection: collection.to_string(),
            applied: report.applied.len(),
            stale: report.stale.len(),
        });
    }
}

/// Sets how often this machine checks the spec. Anything but `off` needs an approval.
#[tauri::command]
pub async fn openapi_sync_set_interval(
    collection: String,
    interval: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<(), String> {
    let interval = interval_from_name(&interval)?;
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| {
            let root = service.root_of(&collection)?;
            service.set_interval(&root, interval)
        },
    )
    .await
}

/// Fixed sentences for a failed disconnect, so no path or error text reaches the UI.
fn disconnect_error(error: DomainError) -> String {
    match error {
        DomainError::Conflict(_) => {
            "An apply is running on this collection. Try again when it finishes.".to_string()
        }
        _ => "The sync settings could not be removed. Try again.".to_string(),
    }
}

/// Removes the sync state of a collection. Requests and folders stay. `removeToken` also
/// deletes the stored access token.
#[tauri::command]
pub async fn openapi_sync_disconnect(
    collection: String,
    remove_token: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<(), String> {
    let remove_token = remove_token.unwrap_or(false);
    let result = tokio::task::spawn_blocking({
        let workspace_path = Arc::clone(workspace_path.inner());
        let fetcher = Arc::clone(fetcher.inner());
        move || {
            let service = make_sync_service(workspace_path, fetcher);
            let root = service.root_of(&collection)?;
            service.disconnect(&root, remove_token)
        }
    })
    .await
    .map_err(|_| TASK_FAILED.to_string())?;
    result.map_err(disconnect_error)
}

/// Stores the access token of a GitHub source in the keychain.
#[tauri::command]
pub async fn openapi_sync_set_token(
    collection: String,
    token: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<(), String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| {
            let root = service.root_of(&collection)?;
            service.set_token(&root, &token)
        },
    )
    .await
}

/// Removes the stored access token of a GitHub source.
#[tauri::command]
pub async fn openapi_sync_clear_token(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<(), String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| {
            let root = service.root_of(&collection)?;
            service.clear_token(&root)
        },
    )
    .await
}

/// Approves the spec location the UI showed (`location`, the display text). It refuses when
/// the source file shows another location now.
#[tauri::command]
pub async fn openapi_sync_grant_consent(
    collection: String,
    location: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<(), String> {
    let workspace_path = Arc::clone(workspace_path.inner());
    let fetcher = Arc::clone(fetcher.inner());
    let result = tokio::task::spawn_blocking(move || {
        let service = make_sync_service(workspace_path, fetcher);
        let root = service.root_of(&collection)?;
        service.grant_consent(&root, &location)
    })
    .await
    .map_err(|_| TASK_FAILED.to_string())?;
    result.map_err(grant_error_text)
}

/// Withdraws the approval of this collection on this machine.
#[tauri::command]
pub async fn openapi_sync_revoke_consent(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<(), String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| {
            let root = service.root_of(&collection)?;
            service.revoke_consent(&root)
        },
    )
    .await
}

/// Applies the approved items of the pending spec. A selection with only skips records them.
#[tauri::command]
pub async fn openapi_sync_apply(
    collection: String,
    selection: SelectionDto,
    app: tauri::AppHandle,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<ApplyReportDto, String> {
    let selection = selection_from_dto(selection)?;
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        // The apply never awaits, so driving it here only waits on the collection lock
        // and the files.
        move |service| {
            let result = tauri::async_runtime::block_on(service.apply(&collection, &selection));
            let events = crate::tauri_event_bus::TauriEventBus::new(app);
            publish_applied(&events, &collection, &result);
            result.map(report_dto)
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_app::UnchangedView;
    use rocket_collection::Request;
    use rocket_import::{MergeOutcome, SyncPlan};
    use rocket_shared::types::HttpMethod;
    use serde_json::json;

    fn request(name: &str) -> Request {
        Request::new(name, HttpMethod::Get, "{{baseUrl}}/pets")
    }

    fn view(items: Vec<SyncItem>) -> PlanView {
        PlanView {
            pending: true,
            item_digests: items.iter().map(|i| format!("d-{}", i.key())).collect(),
            variable_digests: vec!["vd".to_string()],
            plan: SyncPlan {
                items,
                variable_changes: vec![VariableChange {
                    name: "baseUrl".to_string(),
                    kind: VariableKind::BaseUrl,
                    baseline: Some("a".to_string()),
                    yours: None,
                    spec: Some("b".to_string()),
                }],
                skipped_hidden: 2,
                deleted_by_user: vec!["gone".to_string()],
                cautious: true,
                warnings: vec!["careful".to_string()],
            },
            unchanged: vec![UnchangedView {
                key: "getPet".to_string(),
                name: "getPet".to_string(),
                request_path: Some("pets/getPet".to_string()),
            }],
        }
    }

    #[test]
    fn a_changed_item_serializes_with_its_kind_and_changes() {
        let item = SyncItem::Changed {
            key: "listPets".to_string(),
            uid: "u1".to_string(),
            request_path: "pets/listPets".to_string(),
            name: "listPets".to_string(),
            merge: MergeOutcome {
                merged: request("listPets"),
                changes: vec![FieldChange {
                    field: "query:limit".to_string(),
                    outcome: FieldOutcome::TakeSpec,
                    baseline: None,
                    yours: None,
                    spec: Some("1".to_string()),
                }],
                cautious: false,
            },
            fingerprint: "fp".to_string(),
            spec: request("listPets"),
        };
        let json = serde_json::to_value(plan_dto(view(vec![item]))).expect("json");
        assert_eq!(
            json["items"][0],
            json!({
                "kind": "changed",
                "key": "listPets",
                "name": "listPets",
                "digest": "d-listPets",
                "fingerprint": "fp",
                "requestPath": "pets/listPets",
                "cautious": false,
                "changes": [{"field": "query:limit", "outcome": "takeSpec", "spec": "1"}],
            })
        );
        assert_eq!(json["skippedHidden"], 2);
        assert_eq!(json["deletedByUser"], json!(["gone"]));
        assert_eq!(json["unchanged"][0]["requestPath"], "pets/getPet");
        assert_eq!(
            json["variableChanges"][0],
            json!({"name": "baseUrl", "kind": "baseUrl", "digest": "vd",
                "baseline": "a", "spec": "b"})
        );
    }

    #[test]
    fn a_failed_status_serializes_its_code() {
        let dto = status_dto(SyncStatusView {
            location: "https://example.com/spec.json".to_string(),
            interval: SyncInterval::Weekly,
            last_checked_at: Some("2026-10-26T10:00:00+00:00".to_string()),
            last_status: Some(SyncStatus::Failed {
                code: "not_found".to_string(),
            }),
            has_pending: false,
            has_token: Some(true),
            consent: rocket_collection::ConsentState::Granted,
            last_message: Some("The server could not be reached.".to_string()),
            interval_unknown: false,
            pending_id: None,
        });
        let json = serde_json::to_value(dto).expect("json");
        assert_eq!(
            json["lastStatus"],
            json!({"state": "failed", "code": "not_found"})
        );
        assert_eq!(json["interval"], "weekly");
        assert_eq!(json["hasToken"], true);
        assert_eq!(json["consent"], "granted");
        assert_eq!(json["lastMessage"], "The server could not be reached.");
        assert_eq!(json["lastCheckedAt"], "2026-10-26T10:00:00+00:00");
    }

    #[test]
    fn a_removed_item_with_unknown_edit_state_serializes_null() {
        let item = SyncItem::Removed {
            key: "old".to_string(),
            uid: "u2".to_string(),
            request_path: "pets/old".to_string(),
            name: "old".to_string(),
            edited: None,
            possibly_renamed_to: None,
        };
        let json = serde_json::to_value(plan_dto(view(vec![item]))).expect("json");
        assert_eq!(json["items"][0]["kind"], "removed");
        assert!(json["items"][0]["edited"].is_null());
        assert_eq!(json["items"][0]["fingerprint"], "removed");
        assert!(json["items"][0]
            .as_object()
            .expect("object")
            .contains_key("edited"));
    }

    #[test]
    fn a_selection_maps_choices_and_keeps_the_link_order() {
        let dto: SelectionDto = serde_json::from_value(json!({
            "items": [{"key": "a", "digest": "d1", "choices": {"summary": "yours", "url": "spec"}}],
            "skips": [{"key": "b", "fingerprint": "fp"}],
            "variables": [{"name": "baseUrl", "digest": "vd"}],
            "renameLinks": [["z", "y"], ["a", "b"]],
        }))
        .expect("dto");
        let selection = selection_from_dto(dto).expect("selection");
        assert_eq!(selection.items[0].choices["summary"], Choice::Yours);
        assert_eq!(selection.items[0].choices["url"], Choice::Spec);
        assert_eq!(selection.skips[0].fingerprint, "fp");
        assert_eq!(selection.variables[0].name, "baseUrl");
        assert_eq!(
            selection.rename_links,
            vec![
                ("z".to_string(), "y".to_string()),
                ("a".to_string(), "b".to_string())
            ]
        );
    }

    #[test]
    fn a_selection_with_only_skips_parses_and_an_empty_one_too() {
        let dto: SelectionDto =
            serde_json::from_value(json!({"skips": [{"key": "b", "fingerprint": "fp"}]}))
                .expect("dto");
        let selection = selection_from_dto(dto).expect("selection");
        assert!(selection.items.is_empty());
        assert_eq!(selection.skips.len(), 1);
        let empty: SelectionDto = serde_json::from_value(json!({})).expect("dto");
        assert!(selection_from_dto(empty)
            .expect("selection")
            .items
            .is_empty());
    }

    #[test]
    fn an_unknown_choice_is_refused_with_a_clear_error() {
        let dto: SelectionDto = serde_json::from_value(json!({
            "items": [{"key": "a", "digest": "d", "choices": {"summary": "mine"}}]
        }))
        .expect("dto");
        let err = selection_from_dto(dto).expect_err("refused");
        assert!(err.contains("\"mine\"") && err.contains("summary"), "{err}");
        assert!(err.contains("spec") && err.contains("yours"), "{err}");
    }

    #[test]
    fn an_apply_report_maps_every_list() {
        let report = ApplyReport {
            applied: vec![
                rocket_import::AppliedItem {
                    key: "a".to_string(),
                    kind: AppliedKind::Created,
                    request_path: "pets/a".to_string(),
                },
                rocket_import::AppliedItem {
                    key: "b".to_string(),
                    kind: AppliedKind::Updated,
                    request_path: "pets/b".to_string(),
                },
            ],
            stale: vec!["s".to_string()],
            not_rewritable: vec!["n".to_string()],
            deleted: vec!["d".to_string()],
            detached: vec!["t".to_string()],
            skipped: 2,
            remaining: 3,
        };
        let json = serde_json::to_value(report_dto(report)).expect("json");
        assert_eq!(
            json,
            json!({
                "applied": [
                    {"key": "a", "kind": "created", "requestPath": "pets/a"},
                    {"key": "b", "kind": "updated", "requestPath": "pets/b"},
                ],
                "stale": ["s"],
                "notRewritable": ["n"],
                "deleted": ["d"],
                "detached": ["t"],
                "skipped": 2,
                "remaining": 3,
            })
        );
    }

    #[test]
    fn a_check_outcome_maps_to_its_state() {
        let failed = check_outcome_dto(CheckOutcome::Failed {
            code: "rate_limited".to_string(),
            message: "Slow down.".to_string(),
            retry_after_secs: Some(60),
        });
        let json = serde_json::to_value(failed).expect("json");
        assert_eq!(
            json,
            json!({"state": "failed", "code": "rate_limited", "message": safe_message("rate_limited"),
                "retryAfterSecs": 60})
        );
        let changed = check_outcome_dto(CheckOutcome::Changed {
            spec_sha256: "x".to_string(),
        });
        assert_eq!(
            serde_json::to_value(changed).expect("json"),
            json!({"state": "changed"})
        );
    }

    #[test]
    fn a_failed_outcome_never_forwards_raw_error_text() {
        let failed = check_outcome_dto(CheckOutcome::Failed {
            code: "network".to_string(),
            message: "error sending https://h/x?token=ghp_secret123".to_string(),
            retry_after_secs: None,
        });
        let text = serde_json::to_string(&failed).expect("json");
        assert!(!text.contains("https://"), "{text}");
        assert!(!text.contains("ghp_"), "{text}");
        assert!(!text.contains("token="), "{text}");
        assert_eq!(failed.code.as_deref(), Some("network"));
        assert!(failed.message.is_some());
    }

    #[test]
    fn a_misspelled_selection_key_is_refused() {
        let top: Result<SelectionDto, _> =
            serde_json::from_value(json!({"renamelinks": [["a", "b"]]}));
        assert!(top.is_err());
        let item: Result<SelectionDto, _> = serde_json::from_value(json!({
            "items": [{"key": "a", "digest": "d", "choice": {}}]
        }));
        assert!(item.is_err());
    }

    #[test]
    fn has_token_is_left_out_when_not_asked() {
        let view = |has_token| SyncStatusView {
            location: "https://example.com/spec.json".to_string(),
            interval: SyncInterval::Off,
            last_checked_at: None,
            last_status: None,
            has_pending: false,
            has_token,
            consent: ConsentState::Granted,
            last_message: None,
            interval_unknown: false,
            pending_id: None,
        };
        let json = serde_json::to_value(status_dto(view(None))).expect("json");
        assert!(json.get("hasToken").is_none());
        let json = serde_json::to_value(status_dto(view(Some(false)))).expect("json");
        assert_eq!(json["hasToken"], false);
    }

    #[derive(Default)]
    struct Recorded(Mutex<Vec<rocket_shared::events::DomainEvent>>);

    impl EventPublisher for Recorded {
        fn publish(&self, event: rocket_shared::events::DomainEvent) {
            self.0.lock().expect("lock").push(event);
        }
    }

    fn report(applied: usize, stale: usize) -> ApplyReport {
        ApplyReport {
            applied: (0..applied)
                .map(|i| rocket_import::AppliedItem {
                    key: format!("k{i}"),
                    kind: AppliedKind::Created,
                    request_path: format!("p{i}"),
                })
                .collect(),
            stale: (0..stale).map(|i| format!("s{i}")).collect(),
            not_rewritable: vec![],
            deleted: vec![],
            detached: vec![],
            skipped: 0,
            remaining: 0,
        }
    }

    #[test]
    fn apply_publishes_one_event() {
        use rocket_shared::events::DomainEvent;
        let events = Recorded::default();
        publish_applied(&events, "api", &Ok(report(3, 1)));
        let seen = events.0.lock().expect("lock");
        assert_eq!(seen.len(), 1);
        assert!(matches!(
            &seen[0],
            DomainEvent::OpenApiSyncApplied { collection, applied: 3, stale: 1 } if collection == "api"
        ));
    }

    #[test]
    fn a_failed_apply_publishes_nothing() {
        let events = Recorded::default();
        publish_applied(&events, "api", &Err(DomainError::NotFound("x".to_string())));
        assert!(events.0.lock().expect("lock").is_empty());
    }

    #[test]
    fn the_status_dto_names_every_consent_state() {
        for (state, name) in [
            (ConsentState::Granted, "granted"),
            (ConsentState::Missing, "missing"),
            (ConsentState::LocationChanged, "locationChanged"),
        ] {
            let dto = status_dto(SyncStatusView {
                location: "https://example.com/spec.json".to_string(),
                interval: SyncInterval::Off,
                last_checked_at: None,
                last_status: None,
                has_pending: false,
                has_token: None,
                consent: state,
                last_message: None,
                interval_unknown: false,
                pending_id: None,
            });
            let json = serde_json::to_value(dto).expect("json");
            assert_eq!(json["consent"], name);
            assert!(json.get("lastMessage").is_none());
            assert!(json.get("intervalUnknown").is_none());
        }
    }

    #[test]
    fn intervals_parse_and_unknown_names_are_refused() {
        assert_eq!(interval_from_name("weekly"), Ok(SyncInterval::Weekly));
        assert_eq!(interval_from_name("off"), Ok(SyncInterval::Off));
        assert!(interval_from_name("yearly").is_err());
        for (name, interval) in [
            ("every_5_minutes", SyncInterval::Min5),
            ("every_15_minutes", SyncInterval::Min15),
            ("every_30_minutes", SyncInterval::Min30),
        ] {
            assert_eq!(interval_from_name(name), Ok(interval));
            assert_eq!(interval_name(interval), name);
        }
        let error = interval_from_name("every_2_hours").expect_err("unknown");
        assert!(error.contains("every_5_minutes"), "{error}");
        assert!(!error.contains("every_2_hours"), "{error}");
    }

    #[test]
    fn an_unknown_stored_interval_is_flagged_in_the_dto() {
        let dto = status_dto(SyncStatusView {
            location: "https://example.com/spec.json".to_string(),
            interval: SyncInterval::Off,
            last_checked_at: None,
            last_status: None,
            has_pending: false,
            has_token: None,
            consent: ConsentState::Granted,
            last_message: None,
            interval_unknown: true,
            pending_id: None,
        });
        let json = serde_json::to_value(dto).expect("json");
        assert_eq!(json["interval"], "off");
        assert_eq!(json["intervalUnknown"], true);
    }

    #[test]
    fn disconnect_errors_are_fixed_sentences() {
        let busy = disconnect_error(DomainError::Conflict("path /x/y".to_string()));
        assert!(busy.contains("apply is running"), "{busy}");
        let other = disconnect_error(DomainError::Internal("secret /home/u".to_string()));
        assert!(!other.contains("/home"), "{other}");
        assert!(!other.contains("secret"), "{other}");
    }

    #[test]
    fn grant_errors_become_clear_sentences() {
        let changed = grant_error_text(DomainError::Conflict("x".to_string()));
        assert!(changed.contains("changed"), "{changed}");
        let ambiguous = grant_error_text(DomainError::InvalidInput("x".to_string()));
        assert!(ambiguous.contains("ambiguous"), "{ambiguous}");
    }

    #[test]
    fn a_missing_source_maps_to_none_and_other_errors_stay() {
        let none = status_or_none(Err(DomainError::NotFound("x".to_string())));
        assert!(matches!(none, Ok(None)));
        let err = status_or_none(Err(DomainError::Internal("x".to_string())));
        assert!(err.is_err());
    }

    #[test]
    fn overview_dto_uses_camel_case_and_nulls_unknown_counts() {
        let dto = overview_dto(OverviewView {
            endpoints: 4,
            in_sync: None,
            changed_by_you: None,
            updates_waiting: 3,
            spec_version: Some("2.0.0".to_string()),
            last_synced_at: Some("2026-10-27T10:00:00+00:00".to_string()),
            last_synced_updated: 2,
        });
        let json = serde_json::to_value(dto).expect("json");
        assert_eq!(json["endpoints"], 4);
        assert_eq!(json["inSync"], serde_json::Value::Null);
        assert_eq!(json["changedByYou"], serde_json::Value::Null);
        assert_eq!(json["updatesWaiting"], 3);
        assert_eq!(json["specVersion"], "2.0.0");
        assert_eq!(json["lastSyncedAt"], "2026-10-27T10:00:00+00:00");
        assert_eq!(json["lastSyncedUpdated"], 2);
    }

    #[test]
    fn changes_dto_serializes_rows_with_field_names() {
        let row = |key: Option<&str>| ChangeRow {
            key: key.map(str::to_string),
            uid: "u1".to_string(),
            name: "getPet".to_string(),
            method: "GET".to_string(),
            url: "{{baseUrl}}/pets".to_string(),
            request_path: Some("pets/getPet".to_string()),
            fields: vec!["url".to_string()],
        };
        let json = serde_json::to_value(changes_dto(ChangesView {
            baseline_trusted: true,
            modified: vec![row(Some("getPet"))],
            added: vec![row(None)],
            deleted: Vec::new(),
        }))
        .expect("json");
        assert_eq!(json["baselineTrusted"], true);
        assert_eq!(json["modified"][0]["key"], "getPet");
        assert_eq!(json["modified"][0]["requestPath"], "pets/getPet");
        assert_eq!(json["modified"][0]["fields"], json!(["url"]));
        assert_eq!(json["added"][0]["key"], serde_json::Value::Null);
        assert_eq!(json["deleted"], json!([]));
    }

    #[test]
    fn badge_dto_shape() {
        let json = serde_json::to_value(badge_dto(BadgeView {
            collection: "Pets".to_string(),
            waiting: 3,
        }))
        .expect("json");
        assert_eq!(json, json!({"collection": "Pets", "waiting": 3}));
    }

    #[test]
    fn status_dto_includes_pending_id_only_when_present() {
        let view = |pending_id: Option<String>| SyncStatusView {
            location: "https://example.com/spec.json".to_string(),
            interval: SyncInterval::Off,
            last_checked_at: None,
            last_status: None,
            has_pending: pending_id.is_some(),
            has_token: None,
            consent: ConsentState::Granted,
            last_message: None,
            interval_unknown: false,
            pending_id,
        };
        let json = serde_json::to_value(status_dto(view(None))).expect("json");
        assert!(json.get("pendingId").is_none());
        let json =
            serde_json::to_value(status_dto(view(Some("abc123def456".to_string())))).expect("json");
        assert_eq!(json["pendingId"], "abc123def456");
    }
}
