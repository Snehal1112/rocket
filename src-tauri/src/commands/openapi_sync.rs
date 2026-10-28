//! Commands for the OpenAPI sync tab: the status and the plan (read-only), and the actions: check
//! now, interval, token, consent and apply. Each runs on a blocking task.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use std::collections::BTreeMap;

use rocket_app::{
    safe_message, BackupRow, BadgeView, ChangeLinkOutcome, ChangeLinkPreview, ChangeRow,
    ChangeValue, ChangesView, CheckOutcome, ConnectOperationRow, ConnectOutcome, ConnectPreview,
    ConnectRequestRow, ConnectRow, OpenApiSyncService, OverviewView, PlanView, ResetReport,
    DeleteTarget, SkippedRow, SpecSource, SpecSourceFormat, SpecView, SpecViewState, SyncStatusView,
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
        preview: NewPreviewDto,
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
        uid: String,
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

/// What a new request looks like, without any value of a header or a query parameter, because an
/// example in a spec can hold a secret.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPreviewDto {
    pub method: String,
    pub url: String,
    pub headers: Vec<String>,
    pub query: Vec<String>,
    pub has_body: bool,
    pub auth_kind: Option<String>,
}

fn new_preview_dto(request: &rocket_collection::Request) -> NewPreviewDto {
    let names = |entries: &[rocket_shared::types::Header]| {
        entries
            .iter()
            .filter(|h| h.enabled)
            .map(|h| h.key.clone())
            .collect::<Vec<_>>()
    };
    let has_body = request.body.as_ref().is_some_and(|b| {
        !matches!(b.mode, rocket_shared::types::BodyMode::None)
            && (b.content.as_deref().is_some_and(|c| !c.is_empty())
                || b.form_data.as_ref().is_some_and(|f| !f.is_empty())
                || b.file_path.is_some())
    });
    // The tag of the serialized auth is the kind. None and inherit mean the request has none.
    let auth_kind = serde_json::to_value(&request.auth)
        .ok()
        .and_then(|v| {
            v.get("authType")
                .and_then(|t| t.as_str().map(str::to_string))
        })
        .filter(|kind| kind != "none" && kind != "inherit");
    NewPreviewDto {
        method: request.method.to_string(),
        url: request.url.clone(),
        headers: names(&request.headers),
        query: names(&request.query_params),
        has_body,
        auth_kind,
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
            request,
        } => SyncItemDto::New {
            key: key.clone(),
            name: name.clone(),
            digest,
            fingerprint: fingerprint.clone(),
            folder: folder.clone(),
            possibly_renamed_from: possibly_renamed_from.clone(),
            preview: new_preview_dto(request),
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
            uid,
            name,
            request_path,
            edited,
            possibly_renamed_to,
        } => SyncItemDto::Removed {
            key: key.clone(),
            uid: uid.clone(),
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

/// One parameter of an operation in the spec viewer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecParamDto {
    pub name: String,
    /// `path`, `query`, `header` or `cookie`.
    pub location: String,
    pub required: bool,
}

/// One property of an object schema.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaPropertyDto {
    pub name: String,
    pub schema: SchemaViewDto,
}

/// A bounded schema for the spec viewer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaViewDto {
    pub kind: String,
    pub format: Option<String>,
    pub description: Option<String>,
    pub required: bool,
    pub nullable: bool,
    pub enum_values: Vec<String>,
    pub ref_name: Option<String>,
    pub properties: Vec<SchemaPropertyDto>,
    pub items: Option<Box<SchemaViewDto>>,
    pub variants: Vec<SchemaViewDto>,
    pub cut: bool,
}

/// One media type of a body with its schema.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyViewDto {
    pub content_type: String,
    pub required: bool,
    pub schema: Option<SchemaViewDto>,
}

fn schema_view_dto(schema: rocket_import::SchemaView) -> SchemaViewDto {
    SchemaViewDto {
        kind: schema.kind,
        format: schema.format,
        description: schema.description,
        required: schema.required,
        nullable: schema.nullable,
        enum_values: schema.enum_values,
        ref_name: schema.ref_name,
        properties: schema
            .properties
            .into_iter()
            .map(|(name, schema)| SchemaPropertyDto {
                name,
                schema: schema_view_dto(schema),
            })
            .collect(),
        items: schema.items.map(|items| Box::new(schema_view_dto(*items))),
        variants: schema.variants.into_iter().map(schema_view_dto).collect(),
        cut: schema.cut,
    }
}

fn body_view_dto(body: rocket_import::BodyView) -> BodyViewDto {
    BodyViewDto {
        content_type: body.content_type,
        required: body.required,
        schema: body.schema.map(schema_view_dto),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecResponseDto {
    pub status: String,
    pub description: String,
    pub bodies: Vec<BodyViewDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecOperationDto {
    pub key: String,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub parameters: Vec<SpecParamDto>,
    pub responses: Vec<SpecResponseDto>,
    pub request_body: Vec<BodyViewDto>,
    pub security: Vec<String>,
    pub is_new: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecGroupDto {
    pub tag: String,
    pub operations: Vec<SpecOperationDto>,
}

/// The latest stored spec for the read-only viewer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecViewDto {
    pub title: String,
    pub version: Option<String>,
    pub fetched_at: Option<String>,
    /// `in_sync`, `updates_waiting` or `no_copy`.
    pub state: &'static str,
    pub groups: Vec<SpecGroupDto>,
    pub servers: Vec<String>,
}

fn spec_view_dto(view: SpecView) -> SpecViewDto {
    SpecViewDto {
        title: view.title,
        version: view.version,
        fetched_at: view.fetched_at,
        state: match view.state {
            SpecViewState::InSync => "in_sync",
            SpecViewState::UpdatesWaiting => "updates_waiting",
            SpecViewState::NoCopy => "no_copy",
        },
        groups: view
            .groups
            .into_iter()
            .map(|g| SpecGroupDto {
                tag: g.tag,
                operations: g
                    .operations
                    .into_iter()
                    .map(|o| SpecOperationDto {
                        key: o.key,
                        method: o.method,
                        path: o.path,
                        summary: o.summary,
                        parameters: o
                            .parameters
                            .into_iter()
                            .map(|p| SpecParamDto {
                                name: p.name,
                                location: p.location,
                                required: p.required,
                            })
                            .collect(),
                        responses: o
                            .responses
                            .into_iter()
                            .map(|r| SpecResponseDto {
                                status: r.status,
                                description: r.description,
                                bodies: r.bodies.into_iter().map(body_view_dto).collect(),
                            })
                            .collect(),
                        request_body: o.request_body.into_iter().map(body_view_dto).collect(),
                        security: o.security,
                        is_new: o.is_new,
                    })
                    .collect(),
            })
            .collect(),
        servers: view.servers,
    }
}

/// The latest stored spec, grouped by tag. Reads local files only.
#[tauri::command]
pub async fn openapi_sync_spec_view(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<SpecViewDto, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| service.spec_view(&collection).map(spec_view_dto),
    )
    .await
}

/// The raw text of the latest stored spec.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecSourceDto {
    pub text: Option<String>,
    /// `yaml` or `json`.
    pub format: &'static str,
    pub size_bytes: u64,
    pub too_large: bool,
}

fn spec_source_dto(source: SpecSource) -> SpecSourceDto {
    SpecSourceDto {
        text: source.text,
        format: match source.format {
            SpecSourceFormat::Yaml => "yaml",
            SpecSourceFormat::Json => "json",
        },
        size_bytes: source.size_bytes,
        too_large: source.too_large,
    }
}

/// The raw text of the latest stored spec. Reads local files only.
#[tauri::command]
pub async fn openapi_sync_spec_source(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<SpecSourceDto, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| service.spec_source(&collection).map(spec_source_dto),
    )
    .await
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
    pub changes: Vec<ChangeValueDto>,
}

/// One changed field with the version last synced and the user's version.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeValueDto {
    pub field: String,
    pub baseline: Option<String>,
    pub yours: Option<String>,
}

fn change_value_dto(value: &ChangeValue) -> ChangeValueDto {
    ChangeValueDto {
        field: value.field.clone(),
        baseline: value.baseline.clone(),
        yours: value.yours.clone(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequestDto {
    pub uid: String,
    pub request_path: String,
    pub method: String,
    pub url: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOperationDto {
    pub key: String,
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub tag: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPairDto {
    pub request: ConnectRequestDto,
    pub operation: ConnectOperationDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectAmbiguousDto {
    pub requests: Vec<ConnectRequestDto>,
    pub operations: Vec<ConnectOperationDto>,
}

/// What a spec link would connect to, before anything is written.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPreviewDto {
    pub spec_title: String,
    pub spec_version: Option<String>,
    pub digest: String,
    pub spec_sha256: String,
    pub matched: Vec<ConnectPairDto>,
    pub suggested: Vec<ConnectPairDto>,
    pub in_spec_only: Vec<ConnectOperationDto>,
    pub yours_only: Vec<ConnectRequestDto>,
    pub ambiguous: Vec<ConnectAmbiguousDto>,
}

fn connect_request_dto(r: ConnectRequestRow) -> ConnectRequestDto {
    ConnectRequestDto {
        uid: r.uid,
        request_path: r.request_path,
        method: r.method,
        url: r.url,
        name: r.name,
    }
}

fn connect_operation_dto(o: ConnectOperationRow) -> ConnectOperationDto {
    ConnectOperationDto {
        key: o.key,
        method: o.method,
        path: o.path,
        summary: o.summary,
        tag: o.tag,
    }
}

fn connect_pair_dto(p: rocket_app::ConnectPair) -> ConnectPairDto {
    ConnectPairDto {
        request: connect_request_dto(p.request),
        operation: connect_operation_dto(p.operation),
    }
}

fn connect_preview_dto(p: ConnectPreview) -> ConnectPreviewDto {
    ConnectPreviewDto {
        spec_title: p.spec_title,
        spec_version: p.spec_version,
        digest: p.digest,
        spec_sha256: p.spec_sha256,
        matched: p.matched.into_iter().map(connect_pair_dto).collect(),
        suggested: p.suggested.into_iter().map(connect_pair_dto).collect(),
        in_spec_only: p.in_spec_only.into_iter().map(connect_operation_dto).collect(),
        yours_only: p.yours_only.into_iter().map(connect_request_dto).collect(),
        ambiguous: p
            .ambiguous
            .into_iter()
            .map(|g| ConnectAmbiguousDto {
                requests: g.requests.into_iter().map(connect_request_dto).collect(),
                operations: g.operations.into_iter().map(connect_operation_dto).collect(),
            })
            .collect(),
    }
}

/// One decision of the Initial sync review.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRowDto {
    /// `link`, `add`, `skip` or `keep`.
    pub kind: String,
    pub uid: Option<String>,
    pub key: Option<String>,
}

const BAD_CONNECT_ROW: &str = "A row of the review is not complete. Run the preview again.";

fn connect_row_from_dto(dto: ConnectRowDto) -> Result<ConnectRow, String> {
    let bad = || BAD_CONNECT_ROW.to_string();
    Ok(match dto.kind.as_str() {
        "link" => ConnectRow::Link {
            uid: dto.uid.ok_or_else(bad)?,
            key: dto.key.ok_or_else(bad)?,
        },
        "add" => ConnectRow::Add {
            key: dto.key.ok_or_else(bad)?,
        },
        "skip" => ConnectRow::Skip {
            key: dto.key.ok_or_else(bad)?,
        },
        "keep" => ConnectRow::Keep {
            uid: dto.uid.ok_or_else(bad)?,
        },
        _ => return Err(bad()),
    })
}

/// What a connect did. The warnings are fixed sentences.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOutcomeDto {
    pub linked: u32,
    pub added: u32,
    pub skipped: u32,
    pub warnings: Vec<String>,
}

fn connect_outcome_dto(o: ConnectOutcome) -> ConnectOutcomeDto {
    ConnectOutcomeDto {
        linked: o.linked,
        added: o.added,
        skipped: o.skipped,
        warnings: o.warnings,
    }
}

/// Connects a collection to the spec at the link the user typed, in one transaction. Approval,
/// interval and token follow the files and only add warnings when they fail.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn openapi_connect_confirm(
    collection: String,
    link: String,
    token: Option<String>,
    rows: Vec<ConnectRowDto>,
    group_by: String,
    interval: Option<String>,
    digest: String,
    spec_sha256: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<ConnectOutcomeDto, String> {
    let rows = rows
        .into_iter()
        .map(connect_row_from_dto)
        .collect::<Result<Vec<_>, _>>()?;
    let group_by = rocket_import::GroupBy::parse(&group_by)
        .ok_or_else(|| "The folder layout must be tags or paths.".to_string())?;
    let interval = interval.as_deref().map(interval_from_name).transpose()?;
    let workspace_path = Arc::clone(workspace_path.inner());
    let fetcher = Arc::clone(fetcher.inner());
    tokio::task::spawn_blocking(move || {
        let service = make_sync_service(workspace_path, fetcher);
        tauri::async_runtime::block_on(service.confirm(
            &collection,
            &link,
            token.as_deref(),
            rows,
            group_by,
            interval,
            &digest,
            &spec_sha256,
        ))
    })
    .await
    .map_err(|_| TASK_FAILED.to_string())?
    .map(connect_outcome_dto)
    .map_err(connect_error)
}

/// A fixed sentence for a failed connect. The service builds its input and conflict messages
/// from fixed text, so those pass through. Anything else becomes a general sentence.
fn connect_error(error: DomainError) -> String {
    match error {
        DomainError::InvalidInput(m) | DomainError::Conflict(m) => m,
        DomainError::NotFound(_) => "The collection was not found.".to_string(),
        _ => "Connecting failed. Nothing was changed.".to_string(),
    }
}

/// Fetches the spec at the link the user typed and matches it to the requests of a collection
/// without a source. Writes nothing, grants no approval and stores no token.
#[tauri::command]
pub async fn openapi_connect_preview(
    collection: String,
    link: String,
    token: Option<String>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<ConnectPreviewDto, String> {
    let workspace_path = Arc::clone(workspace_path.inner());
    let fetcher = Arc::clone(fetcher.inner());
    tokio::task::spawn_blocking(move || {
        let service = make_sync_service(workspace_path, fetcher);
        tauri::async_runtime::block_on(service.preview(&collection, &link, token.as_deref()))
    })
    .await
    .map_err(|_| TASK_FAILED.to_string())?
    .map(connect_preview_dto)
    .map_err(connect_error)
}

/// What moving to another link would do, for the user to confirm.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeLinkPreviewDto {
    pub spec_sha256: String,
    pub title: String,
    pub version: Option<String>,
    pub matched: usize,
    pub added: usize,
    pub removed: usize,
    pub same_spec: bool,
}

fn change_link_preview_dto(p: ChangeLinkPreview) -> ChangeLinkPreviewDto {
    ChangeLinkPreviewDto {
        spec_sha256: p.spec_sha256,
        title: p.title,
        version: p.version,
        matched: p.matched,
        added: p.added,
        removed: p.removed,
        same_spec: p.same_spec,
    }
}

/// What a move to another link did. The warnings are fixed sentences.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeLinkOutcomeDto {
    pub pending: bool,
    pub token_kept: bool,
    pub warnings: Vec<String>,
}

fn change_link_outcome_dto(o: ChangeLinkOutcome) -> ChangeLinkOutcomeDto {
    ChangeLinkOutcomeDto {
        pending: o.pending,
        token_kept: o.token_kept,
        warnings: o.warnings,
    }
}

/// A fixed sentence for a failed link change. The service builds its input and conflict
/// messages from fixed text, so those pass through. Anything else becomes a general sentence.
fn change_link_error(error: DomainError) -> String {
    match error {
        DomainError::InvalidInput(m) | DomainError::Conflict(m) => m,
        DomainError::NotFound(_) => "The collection was not found.".to_string(),
        _ => "Changing the link failed. Nothing was changed.".to_string(),
    }
}

/// Fetches the spec at the new link and counts how it lines up with the connected operations.
/// Writes nothing, grants no approval and stores no token.
#[tauri::command]
pub async fn openapi_sync_change_link_preview(
    collection: String,
    link: String,
    token: Option<String>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<ChangeLinkPreviewDto, String> {
    let workspace_path = Arc::clone(workspace_path.inner());
    let fetcher = Arc::clone(fetcher.inner());
    tokio::task::spawn_blocking(move || {
        let service = make_sync_service(workspace_path, fetcher);
        tauri::async_runtime::block_on(service.change_link_preview(
            &collection,
            &link,
            token.as_deref(),
        ))
    })
    .await
    .map_err(|_| TASK_FAILED.to_string())?
    .map(change_link_preview_dto)
    .map_err(change_link_error)
}

/// Moves the collection to the new link in one transaction. Approval and the token follow the
/// files and only add warnings when they fail.
#[tauri::command]
pub async fn openapi_sync_change_link_confirm(
    collection: String,
    link: String,
    token: Option<String>,
    spec_sha256: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<ChangeLinkOutcomeDto, String> {
    let workspace_path = Arc::clone(workspace_path.inner());
    let fetcher = Arc::clone(fetcher.inner());
    tokio::task::spawn_blocking(move || {
        let service = make_sync_service(workspace_path, fetcher);
        tauri::async_runtime::block_on(service.change_link_confirm(
            &collection,
            &link,
            token.as_deref(),
            &spec_sha256,
        ))
    })
    .await
    .map_err(|_| TASK_FAILED.to_string())?
    .map(change_link_outcome_dto)
    .map_err(change_link_error)
}

/// A request the user reset, with the version they can restore.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRowDto {
    pub key: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub request_path: String,
    pub saved_at: String,
    pub fields: Vec<String>,
}

fn backup_row_dto(row: BackupRow) -> BackupRowDto {
    BackupRowDto {
        key: row.key,
        name: row.name,
        method: row.method,
        url: row.url,
        request_path: row.request_path,
        saved_at: row.saved_at,
        fields: row.fields,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesDto {
    pub baseline_trusted: bool,
    pub modified: Vec<ChangeRowDto>,
    pub added: Vec<ChangeRowDto>,
    pub deleted: Vec<ChangeRowDto>,
    pub backups: Vec<BackupRowDto>,
    pub reset_available: bool,
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
        changes: row.changes.iter().map(change_value_dto).collect(),
    }
}

fn changes_dto(view: ChangesView) -> ChangesDto {
    ChangesDto {
        baseline_trusted: view.baseline_trusted,
        modified: view.modified.into_iter().map(change_row_dto).collect(),
        added: view.added.into_iter().map(change_row_dto).collect(),
        deleted: view.deleted.into_iter().map(change_row_dto).collect(),
        backups: view.backups.into_iter().map(backup_row_dto).collect(),
        reset_available: view.reset_available,
    }
}

/// Which keys a reset changed and which it left alone.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetReportDto {
    pub reset: Vec<String>,
    pub skipped: Vec<String>,
}

/// A fixed sentence for a failed reset, restore or discard. The raw error never reaches the UI.
fn reset_error(error: DomainError) -> String {
    match error {
        DomainError::InvalidInput(m) if m.contains("older version") => {
            "These requests cannot be reset because the sync data is from an older version."
                .to_string()
        }
        DomainError::Conflict(m) if m.contains("changed on disk") => {
            "The request changed on disk. Reload and try again.".to_string()
        }
        DomainError::Conflict(_) => "A sync is running. Try again when it is done.".to_string(),
        DomainError::NotFound(_) => "There is no backup for this request.".to_string(),
        _ => "Reset failed. Nothing was changed.".to_string(),
    }
}

async fn run_reset<T: Send + 'static>(
    workspace_path: Arc<Mutex<PathBuf>>,
    fetcher: Arc<dyn SpecFetcher>,
    call: impl FnOnce(OpenApiSyncService) -> Result<T, DomainError> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || call(make_sync_service(workspace_path, fetcher)))
        .await
        .map_err(|_| TASK_FAILED.to_string())?
        .map_err(reset_error)
}

/// Puts the spec-owned fields of the given requests back to the last synced spec and keeps the
/// user's version as a backup. Never contacts the network.
#[tauri::command]
pub async fn openapi_sync_reset(
    collection: String,
    keys: Vec<String>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<ResetReportDto, String> {
    run_reset(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| {
            service
                .reset(&collection, &keys)
                .map(|ResetReport { reset, skipped }| ResetReportDto { reset, skipped })
        },
    )
    .await
}

/// Writes the backed-up version of a request back and removes the backup.
#[tauri::command]
pub async fn openapi_sync_restore(
    collection: String,
    key: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<(), String> {
    run_reset(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| service.restore(&collection, &key),
    )
    .await
}

/// Removes a backup without touching any request.
#[tauri::command]
pub async fn openapi_sync_discard_backup(
    collection: String,
    key: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<(), String> {
    run_reset(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| service.discard(&collection, &key),
    )
    .await
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

/// One skipped item, for the Skipped list.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRowDto {
    pub key: String,
    pub name: String,
    pub method: String,
    pub path: String,
}

fn skipped_row_dto(row: SkippedRow) -> SkippedRowDto {
    SkippedRowDto {
        key: row.key,
        name: row.name,
        method: row.method,
        path: row.path,
    }
}

fn unskip_error(error: DomainError) -> String {
    match error {
        DomainError::Conflict(m) if m.contains("not finished") => m,
        DomainError::Conflict(_) => "A sync is running. Try again when it is done.".to_string(),
        DomainError::NotFound(_) => "The collection was not found.".to_string(),
        _ => "Taking the skip back failed. Nothing was changed.".to_string(),
    }
}

/// A request to delete, as the page knew it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTargetDto {
    pub uid: String,
    pub request_path: String,
}

fn delete_error(error: DomainError) -> String {
    match error {
        DomainError::InvalidInput(m)
            if m.contains("still part of the spec") || m.contains("not in this collection") =>
        {
            m
        }
        DomainError::Conflict(m) if m.contains("changed on disk") || m.contains("not finished") => m,
        DomainError::Conflict(_) => "A sync is running. Try again when it is done.".to_string(),
        DomainError::NotFound(_) => "The collection was not found.".to_string(),
        _ => "Deleting failed. Nothing was changed.".to_string(),
    }
}

/// Deletes requests the user added, or that the spec removed and the user had edited. Never
/// contacts the network.
#[tauri::command]
pub async fn openapi_sync_delete_requests(
    collection: String,
    targets: Vec<DeleteTargetDto>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<usize, String> {
    let workspace_path = Arc::clone(workspace_path.inner());
    let fetcher = Arc::clone(fetcher.inner());
    tokio::task::spawn_blocking(move || {
        let targets: Vec<DeleteTarget> = targets
            .into_iter()
            .map(|t| DeleteTarget {
                uid: t.uid,
                request_path: t.request_path,
            })
            .collect();
        make_sync_service(workspace_path, fetcher).delete_requests(&collection, &targets)
    })
    .await
    .map_err(|_| TASK_FAILED.to_string())?
    .map_err(delete_error)
}

/// The items the user skipped. Never contacts the network.
#[tauri::command]
pub async fn openapi_sync_skipped(
    collection: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<Vec<SkippedRowDto>, String> {
    run_blocking(
        Arc::clone(workspace_path.inner()),
        Arc::clone(fetcher.inner()),
        move |service| {
            service
                .skipped(&collection)
                .map(|rows| rows.into_iter().map(skipped_row_dto).collect())
        },
    )
    .await
}

/// Takes the named skips back. Returns how many were removed. Never contacts the network.
#[tauri::command]
pub async fn openapi_sync_unskip(
    collection: String,
    keys: Vec<String>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
    fetcher: State<'_, Arc<dyn SpecFetcher>>,
) -> Result<usize, String> {
    let workspace_path = Arc::clone(workspace_path.inner());
    let fetcher = Arc::clone(fetcher.inner());
    tokio::task::spawn_blocking(move || {
        make_sync_service(workspace_path, fetcher).unskip(&collection, &keys)
    })
    .await
    .map_err(|_| TASK_FAILED.to_string())?
    .map_err(unskip_error)
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
        assert_eq!(json["items"][0]["uid"], "u2");
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
    fn spec_view_dto_uses_camel_case_keys() {
        let dto = spec_view_dto(SpecView {
            title: "Pets".to_string(),
            version: Some("2".to_string()),
            fetched_at: Some("2026-10-27T10:00:00+00:00".to_string()),
            state: SpecViewState::UpdatesWaiting,
            groups: vec![rocket_app::SpecTagGroup {
                tag: "pets".to_string(),
                operations: vec![rocket_app::SpecOpRow {
                    key: "listPets".to_string(),
                    method: "GET".to_string(),
                    path: "/pets".to_string(),
                    summary: None,
                    parameters: Vec::new(),
                    responses: Vec::new(),
                    request_body: Vec::new(),
                    security: Vec::new(),
                    is_new: true,
                }],
            }],
            servers: Vec::new(),
        });
        let json = serde_json::to_value(dto).expect("json");
        assert_eq!(json["fetchedAt"], "2026-10-27T10:00:00+00:00");
        assert_eq!(json["state"], "updates_waiting");
        assert_eq!(json["groups"][0]["operations"][0]["isNew"], true);
        assert_eq!(json["groups"][0]["operations"][0]["method"], "GET");
    }

    #[test]
    fn spec_source_dto_uses_camel_case_keys() {
        let dto = spec_source_dto(SpecSource {
            text: Some("{}".to_string()),
            format: SpecSourceFormat::Json,
            size_bytes: 2,
            too_large: false,
        });
        let json = serde_json::to_value(dto).expect("json");
        assert_eq!(json["text"], "{}");
        assert_eq!(json["format"], "json");
        assert_eq!(json["sizeBytes"], 2);
        assert_eq!(json["tooLarge"], false);
    }

    #[test]
    fn schema_dto_uses_camel_case_keys() {
        use rocket_import::{BodyView, SchemaView};
        let node = |kind: &str| SchemaView {
            kind: kind.to_string(),
            format: None,
            description: None,
            required: false,
            nullable: false,
            enum_values: vec!["a".to_string()],
            ref_name: Some("Node".to_string()),
            properties: Vec::new(),
            items: None,
            variants: Vec::new(),
            cut: true,
        };
        let mut root = node("object");
        root.properties.push(("name".to_string(), node("string")));
        let dto = body_view_dto(BodyView {
            content_type: "application/json".to_string(),
            required: true,
            schema: Some(root),
        });
        let json = serde_json::to_value(dto).expect("json");
        assert_eq!(json["contentType"], "application/json");
        assert_eq!(json["required"], true);
        let schema = &json["schema"];
        assert_eq!(schema["properties"][0]["name"], "name");
        assert_eq!(schema["properties"][0]["schema"]["kind"], "string");
        assert_eq!(schema["refName"], "Node");
        assert_eq!(schema["enumValues"][0], "a");
        assert_eq!(schema["cut"], true);
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
            changes: Vec::new(),
        };
        let json = serde_json::to_value(changes_dto(ChangesView {
            baseline_trusted: true,
            modified: vec![row(Some("getPet"))],
            added: vec![row(None)],
            deleted: Vec::new(),
            backups: vec![BackupRow {
                key: "getPet".to_string(),
                name: "getPet".to_string(),
                method: "GET".to_string(),
                url: "{{baseUrl}}/mine".to_string(),
                request_path: "pets/getPet".to_string(),
                saved_at: "2026-10-27T10:00:00+00:00".to_string(),
                fields: vec!["url".to_string()],
            }],
            reset_available: true,
        }))
        .expect("json");
        assert_eq!(json["baselineTrusted"], true);
        assert_eq!(json["modified"][0]["key"], "getPet");
        assert_eq!(json["modified"][0]["requestPath"], "pets/getPet");
        assert_eq!(json["modified"][0]["fields"], json!(["url"]));
        assert_eq!(json["added"][0]["key"], serde_json::Value::Null);
        assert_eq!(json["deleted"], json!([]));
        assert_eq!(json["resetAvailable"], true);
        assert_eq!(json["backups"][0]["savedAt"], "2026-10-27T10:00:00+00:00");
        assert_eq!(json["backups"][0]["requestPath"], "pets/getPet");
        assert_eq!(json["backups"][0]["fields"], json!(["url"]));
    }

    #[test]
    fn connect_preview_dto_uses_camel_case_keys() {
        let request = ConnectRequestRow {
            uid: "u1".to_string(),
            request_path: "pets/list".to_string(),
            method: "GET".to_string(),
            url: "{{baseUrl}}/pets".to_string(),
            name: "List".to_string(),
        };
        let operation = ConnectOperationRow {
            key: "listPets".to_string(),
            method: "GET".to_string(),
            path: "/pets".to_string(),
            summary: None,
            tag: "pets".to_string(),
        };
        let json = serde_json::to_value(connect_preview_dto(ConnectPreview {
            spec_title: "Pets".to_string(),
            spec_version: Some("1".to_string()),
            digest: "abc".to_string(),
            spec_sha256: "def".to_string(),
            matched: vec![rocket_app::ConnectPair {
                request: request.clone(),
                operation: operation.clone(),
            }],
            suggested: Vec::new(),
            in_spec_only: vec![operation],
            yours_only: vec![request],
            ambiguous: Vec::new(),
        }))
        .expect("json");
        assert_eq!(json["specTitle"], "Pets");
        assert_eq!(json["specSha256"], "def");
        assert_eq!(json["matched"][0]["request"]["requestPath"], "pets/list");
        assert_eq!(json["matched"][0]["operation"]["key"], "listPets");
        assert_eq!(json["inSpecOnly"][0]["tag"], "pets");
        assert_eq!(json["yoursOnly"][0]["uid"], "u1");
        assert_eq!(json["ambiguous"], json!([]));
    }

    #[test]
    fn connect_rows_parse_every_kind_and_reject_incomplete_ones() {
        let row = |kind: &str, uid: Option<&str>, key: Option<&str>| ConnectRowDto {
            kind: kind.to_string(),
            uid: uid.map(str::to_string),
            key: key.map(str::to_string),
        };
        assert_eq!(
            connect_row_from_dto(row("link", Some("u"), Some("k"))),
            Ok(ConnectRow::Link { uid: "u".into(), key: "k".into() })
        );
        assert_eq!(
            connect_row_from_dto(row("add", None, Some("k"))),
            Ok(ConnectRow::Add { key: "k".into() })
        );
        assert_eq!(
            connect_row_from_dto(row("skip", None, Some("k"))),
            Ok(ConnectRow::Skip { key: "k".into() })
        );
        assert_eq!(
            connect_row_from_dto(row("keep", Some("u"), None)),
            Ok(ConnectRow::Keep { uid: "u".into() })
        );
        assert!(connect_row_from_dto(row("link", Some("u"), None)).is_err());
        assert!(connect_row_from_dto(row("nope", None, None)).is_err());
        let parsed: ConnectRowDto =
            serde_json::from_value(json!({"kind": "link", "uid": "u", "key": "k"})).expect("json");
        assert_eq!(parsed.kind, "link");
    }

    #[test]
    fn connect_outcome_dto_uses_camel_case_keys() {
        let json = serde_json::to_value(connect_outcome_dto(ConnectOutcome {
            linked: 2,
            added: 1,
            skipped: 3,
            warnings: vec!["The token was not saved.".to_string()],
        }))
        .expect("json");
        assert_eq!(json["linked"], 2);
        assert_eq!(json["warnings"][0], "The token was not saved.");
    }

    fn new_request() -> rocket_collection::Request {
        let mut r = rocket_collection::Request::new(
            "Create pet",
            rocket_shared::types::HttpMethod::Post,
            "{{baseUrl}}/pets",
        );
        r.headers.push(rocket_shared::types::Header::new(
            "X-Api-Key",
            "super-secret",
        ));
        r.headers
            .push(rocket_shared::types::Header::disabled("X-Off", "off"));
        r.query_params
            .push(rocket_shared::types::Header::new("limit", "10"));
        r
    }

    #[test]
    fn new_preview_lists_names_not_values() {
        let json = serde_json::to_value(new_preview_dto(&new_request())).expect("json");
        assert_eq!(json["method"], "POST");
        assert_eq!(json["url"], "{{baseUrl}}/pets");
        assert_eq!(json["headers"], serde_json::json!(["X-Api-Key"]));
        assert_eq!(json["query"], serde_json::json!(["limit"]));
        let text = json.to_string();
        assert!(!text.contains("super-secret"), "{text}");
    }

    #[test]
    fn new_preview_has_body_and_auth_kind() {
        let mut r = new_request();
        let none = serde_json::to_value(new_preview_dto(&r)).expect("json");
        assert_eq!(none["hasBody"], false);
        assert!(none["authKind"].is_null());
        r.body = Some(rocket_shared::types::Body {
            mode: rocket_shared::types::BodyMode::Json,
            content: Some("{\"a\":1}".to_string()),
            form_data: None,
            file_path: None,
        });
        r.auth = rocket_shared::types::Auth::Bearer {
            token: "secret-token".to_string(),
        };
        let json = serde_json::to_value(new_preview_dto(&r)).expect("json");
        assert_eq!(json["hasBody"], true);
        assert_eq!(json["authKind"], "bearer");
        assert!(!json.to_string().contains("secret-token"));
    }

    #[test]
    fn change_value_dto_uses_camel_case_keys() {
        let json = serde_json::to_value(change_value_dto(&ChangeValue {
            field: "url".to_string(),
            baseline: Some("a".to_string()),
            yours: None,
        }))
        .expect("json");
        assert_eq!(json["field"], "url");
        assert_eq!(json["baseline"], "a");
        assert!(json["yours"].is_null());
    }

    #[test]
    fn change_link_preview_dto_uses_camel_case_keys() {
        let json = serde_json::to_value(change_link_preview_dto(ChangeLinkPreview {
            spec_sha256: "abc".to_string(),
            title: "Pets".to_string(),
            version: Some("2".to_string()),
            matched: 3,
            added: 1,
            removed: 2,
            same_spec: false,
        }))
        .expect("json");
        assert_eq!(json["specSha256"], "abc");
        assert_eq!(json["sameSpec"], false);
        assert_eq!(json["matched"], 3);
        assert_eq!(json["removed"], 2);
    }

    #[test]
    fn change_link_outcome_dto_uses_camel_case_keys() {
        let json = serde_json::to_value(change_link_outcome_dto(ChangeLinkOutcome {
            pending: true,
            token_kept: false,
            warnings: vec!["The token was not saved.".to_string()],
        }))
        .expect("json");
        assert_eq!(json["pending"], true);
        assert_eq!(json["tokenKept"], false);
        assert_eq!(json["warnings"][0], "The token was not saved.");
    }

    #[test]
    fn change_link_errors_pass_only_fixed_sentences() {
        assert_eq!(
            change_link_error(DomainError::Conflict("This is already the spec link.".to_string())),
            "This is already the spec link."
        );
        let other = change_link_error(DomainError::Internal("cannot write /home/me/secret".to_string()));
        assert_eq!(other, "Changing the link failed. Nothing was changed.");
        assert!(!other.contains("secret"));
        assert_eq!(
            change_link_error(DomainError::NotFound("x".to_string())),
            "The collection was not found."
        );
    }

    #[test]
    fn skipped_dto_uses_camel_case_keys() {
        let json = serde_json::to_value(skipped_row_dto(SkippedRow {
            key: "listPets".to_string(),
            name: "List pets".to_string(),
            method: "GET".to_string(),
            path: "{{baseUrl}}/pets".to_string(),
        }))
        .expect("json");
        assert_eq!(json["key"], "listPets");
        assert_eq!(json["name"], "List pets");
        assert_eq!(json["method"], "GET");
        assert_eq!(json["path"], "{{baseUrl}}/pets");
    }

    #[test]
    fn delete_dto_uses_camel_case_keys() {
        let target: DeleteTargetDto =
            serde_json::from_value(serde_json::json!({"uid": "u1", "requestPath": "pets/x"}))
                .expect("target");
        assert_eq!(target.uid, "u1");
        assert_eq!(target.request_path, "pets/x");
    }

    #[test]
    fn delete_errors_pass_only_fixed_sentences() {
        let not_deletable =
            "This request is still part of the spec. Remove it from the spec first.";
        assert_eq!(
            delete_error(DomainError::InvalidInput(not_deletable.to_string())),
            not_deletable
        );
        let changed = "The request changed on disk. Reload the page.";
        assert_eq!(
            delete_error(DomainError::Conflict(changed.to_string())),
            changed
        );
        assert_eq!(
            delete_error(DomainError::Conflict(
                "another sync apply is running".to_string()
            )),
            "A sync is running. Try again when it is done."
        );
        let other = delete_error(DomainError::Internal(
            "cannot remove /home/me/secret".to_string(),
        ));
        assert_eq!(other, "Deleting failed. Nothing was changed.");
        assert!(!other.contains("secret"));
    }

    #[test]
    fn unskip_errors_pass_only_fixed_sentences() {
        let busy = unskip_error(DomainError::Conflict("An apply is running.".to_string()));
        assert_eq!(busy, "A sync is running. Try again when it is done.");
        let other = unskip_error(DomainError::Internal(
            "cannot write /home/me/secret".to_string(),
        ));
        assert_eq!(other, "Taking the skip back failed. Nothing was changed.");
        assert!(!other.contains("secret"));
    }

    #[test]
    fn connect_errors_are_fixed_sentences() {
        assert_eq!(
            connect_error(DomainError::InvalidInput("Enter the spec link.".to_string())),
            "Enter the spec link."
        );
        let other = connect_error(DomainError::Internal("cannot write /home/me/secret".to_string()));
        assert_eq!(other, "Connecting failed. Nothing was changed.");
        assert!(!other.contains("secret"));
    }

    #[test]
    fn reset_errors_map_to_fixed_sentences() {
        let older = reset_error(DomainError::InvalidInput(
            "These requests cannot be reset because the sync data is from an older version."
                .to_string(),
        ));
        assert!(older.contains("older version"));
        let busy = reset_error(DomainError::Conflict(
            "another sync apply is running on this collection".to_string(),
        ));
        assert_eq!(busy, "A sync is running. Try again when it is done.");
        let changed = reset_error(DomainError::Conflict(
            "The request changed on disk. Reload and try again.".to_string(),
        ));
        assert_eq!(changed, "The request changed on disk. Reload and try again.");
        let missing = reset_error(DomainError::NotFound("x".to_string()));
        assert_eq!(missing, "There is no backup for this request.");
        let other = reset_error(DomainError::Internal("cannot write /home/me/secret".to_string()));
        assert_eq!(other, "Reset failed. Nothing was changed.");
        assert!(!other.contains("secret"));
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
