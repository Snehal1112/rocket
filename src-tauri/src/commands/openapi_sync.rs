//! Read-only commands for the Spec sync tab: the sync status of a collection and the plan of its
//! pending spec. Nothing here writes a file.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_app::{OpenApiSyncService, PlanView, SyncStatusView};
use rocket_collection::openapi_source::{SyncInterval, SyncStatus};
use rocket_http::SpecFetcher;
use rocket_import::{FieldChange, FieldOutcome, SyncItem, VariableChange, VariableKind};
use rocket_shared::error::DomainError;
use serde::Serialize;
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
        move |service| match service.status_of(&collection) {
            Ok(view) => Ok(Some(status_dto(view))),
            Err(DomainError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        },
    )
    .await?;
    Ok(result)
}

/// What applying the pending spec of a collection would do. Writes nothing.
#[tauri::command]
pub async fn openapi_sync_plan(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<SyncPlanDto, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| service.plan(&collection).map(plan_dto),
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
            has_token: true,
        });
        let json = serde_json::to_value(dto).expect("json");
        assert_eq!(
            json["lastStatus"],
            json!({"state": "failed", "code": "not_found"})
        );
        assert_eq!(json["interval"], "weekly");
        assert_eq!(json["hasToken"], true);
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
        assert!(json["items"][0]
            .as_object()
            .expect("object")
            .contains_key("edited"));
    }
}
