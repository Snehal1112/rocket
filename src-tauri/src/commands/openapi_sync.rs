//! Commands for the Spec sync tab: the status and the plan (read-only), and the actions: check
//! now, interval, token, consent and apply. Each runs on a blocking task.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use std::collections::BTreeMap;

use rocket_app::{CheckOutcome, OpenApiSyncService, PlanView, SyncStatusView};
use rocket_collection::openapi_source::{SyncInterval, SyncStatus};
use rocket_collection::ConsentState;
use rocket_http::SpecFetcher;
use rocket_import::{
    AppliedKind, ApplyReport, ApprovedItem, Choice, FieldChange, FieldOutcome, Selection,
    SkipRequest, SyncItem, VariableApproval, VariableChange, VariableKind,
};
use rocket_shared::error::DomainError;
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
    /// `off`, `hourly`, `daily`, `weekly` or `monthly`.
    pub interval: &'static str,
    pub last_checked_at: Option<String>,
    pub last_status: Option<LastStatusDto>,
    pub has_pending: bool,
    pub has_token: bool,
    /// `granted`, `missing` or `locationChanged`.
    pub consent: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
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

/// Runs a blocking service call off the async runtime.
async fn run_blocking<T: Send + 'static>(
    workspace_path: Arc<Mutex<PathBuf>>,
    fetcher: Arc<dyn SpecFetcher>,
    call: impl FnOnce(OpenApiSyncService) -> Result<T, DomainError> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || call(make_sync_service(workspace_path, fetcher)))
        .await
        .map_err(|e| format!("the sync task failed: {e}"))?
        .map_err(|e| e.to_string())
}

/// The sync status of a collection, or `None` when it has no OpenAPI source.
#[tauri::command]
pub async fn openapi_sync_status(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<Option<SyncStatusDto>, String> {
    let result = run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| status_or_none(service.status_of(&collection)),
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
        CheckOutcome::Failed {
            code,
            message,
            retry_after_secs,
        } => CheckOutcomeDto {
            state: "failed",
            code: Some(code),
            message: Some(message),
            retry_after_secs,
        },
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

fn interval_from_name(name: &str) -> Result<SyncInterval, String> {
    match name {
        "off" => Ok(SyncInterval::Off),
        "hourly" => Ok(SyncInterval::Hourly),
        "daily" => Ok(SyncInterval::Daily),
        "weekly" => Ok(SyncInterval::Weekly),
        "monthly" => Ok(SyncInterval::Monthly),
        other => Err(format!(
            "Unknown interval \"{other}\". Use off, hourly, daily, weekly or monthly."
        )),
    }
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
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<CheckOutcomeDto, String> {
    let root = run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| service.root_of(&collection),
    )
    .await?;
    let service = make_sync_service(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
    );
    service
        .check(&root)
        .await
        .map(check_outcome_dto)
        .map_err(|e| e.to_string())
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
    .map_err(|e| format!("the sync task failed: {e}"))?;
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
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<ApplyReportDto, String> {
    let selection = selection_from_dto(selection)?;
    let service = make_sync_service(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
    );
    service
        .apply(&collection, &selection)
        .await
        .map(report_dto)
        .map_err(|e| e.to_string())
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
            has_token: true,
            consent: rocket_collection::ConsentState::Granted,
            last_message: Some("The server could not be reached.".to_string()),
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
            json!({"state": "failed", "code": "rate_limited", "message": "Slow down.",
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
    fn intervals_parse_and_unknown_names_are_refused() {
        assert_eq!(interval_from_name("weekly"), Ok(SyncInterval::Weekly));
        assert_eq!(interval_from_name("off"), Ok(SyncInterval::Off));
        assert!(interval_from_name("yearly").is_err());
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
}
