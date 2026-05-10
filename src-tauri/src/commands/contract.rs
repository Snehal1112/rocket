//! Tauri IPC commands for the Contract Lock feature.
//!
//! Exposes `ContractService` to the frontend. The save-side audit hook
//! (`on_request_saved`) is wired into `commands::collections::save_request`,
//! not here — these commands only cover explicit user-driven CRUD.

use crate::commands::contract_dtos::{
    changelog::ContractChangelogDto,
    snapshot::RequestSignatureSnapshotDto,
    summary::{ContractDriftSummaryDto, ContractSummaryDto},
    types::{ContractDto, ContractPartyDto, ContractPolicyDto, ContractScopeDto},
};
use rocket_app::ContractService;
use rocket_collection::contract::{
    types::{Contract, ContractEnforcementMode, ContractStatus},
};
use std::path::PathBuf;
use tauri::State;
use ulid::Ulid;

/// Input DTO for attaching a contract.
/// Dates come from the frontend as `YYYY-MM-DD` strings and are parsed here.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachContractInput {
    pub title: String,
    pub provider: ContractPartyDto,
    pub consumers: Vec<ContractPartyDto>,
    pub version: String,
    pub effective_date: String,
    pub expiry_date: Option<String>,
    /// Absolute paths chosen by the file picker on the user's machine.
    /// The service validates, copies, and converts them to relative paths.
    pub document_paths: Vec<PathBuf>,
    pub scope: ContractScopeDto,
    pub policy: ContractPolicyDto,
    pub initial_snapshots: Vec<RequestSignatureSnapshotDto>,
    /// If true, status is set to Active and snapshot taken on creation.
    /// If false, status is Draft and no snapshot is taken.
    pub publish_immediately: bool,
}

#[tauri::command]
pub fn attach_contract(
    collection_root: String,
    input: AttachContractInput,
    svc: State<'_, ContractService>,
) -> Result<ContractDto, String> {
    use chrono::NaiveDate;

    let root = PathBuf::from(&collection_root);

    let effective_date = NaiveDate::parse_from_str(&input.effective_date, "%Y-%m-%d")
        .map_err(|e| format!("invalid effectiveDate: {e}"))?;

    let expiry_date = input.expiry_date.as_deref()
        .map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("invalid expiryDate: {e}"))?;

    let status = if input.publish_immediately {
        ContractStatus::Active
    } else {
        ContractStatus::Draft
    };

    let contract = Contract {
        id: Ulid::new(),
        title: input.title,
        provider: input.provider.into(),
        consumers: input.consumers.into_iter().map(Into::into).collect(),
        project: String::new(), // project field superseded by ContractParty identities; retained for backward compat
        version: input.version,
        status,
        effective_date,
        expiry_date,
        document_paths: vec![],
        enforcement_mode: ContractEnforcementMode::Informational,
        scope: input.scope.into(),
        policy: input.policy.into(),
        drift_count: 0,
        breach_count: 0,
        endpoint_count: 0, // overwritten by the service after the snapshot walk
        created_by: None,
        created_at: None, // set by the service
        updated_at: None, // set by the service
    };

    let snapshots: Vec<rocket_collection::contract::snapshot::RequestSignatureSnapshot> =
        if input.publish_immediately {
            input.initial_snapshots.into_iter().map(Into::into).collect()
        } else {
            vec![]
        };

    svc.attach_contract(&root, contract, snapshots, input.document_paths)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}

/// Input DTO for updating contract metadata. Does not touch scope, snapshots,
/// or changelog — those are immutable once a contract is attached.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContractInput {
    pub contract_id: String,
    pub title: String,
    pub provider: ContractPartyDto,
    pub consumers: Vec<ContractPartyDto>,
    pub version: String,
    pub effective_date: String,
    pub expiry_date: Option<String>,
    pub policy: ContractPolicyDto,
    /// Absolute paths for newly added attachments (not yet copied).
    pub new_document_paths: Vec<PathBuf>,
    /// Relative paths of existing attachments the user wants to keep.
    pub kept_document_paths: Vec<PathBuf>,
}

#[tauri::command]
pub fn update_contract(
    collection_root: String,
    input: UpdateContractInput,
    svc: State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let root = PathBuf::from(&collection_root);

    let id = Ulid::from_string(&input.contract_id).map_err(|e| e.to_string())?;

    let effective_date = chrono::NaiveDate::parse_from_str(&input.effective_date, "%Y-%m-%d")
        .map_err(|e| format!("invalid effectiveDate: {}", e))?;

    let expiry_date = input
        .expiry_date
        .as_deref()
        .map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("invalid expiryDate: {}", e))?;

    // Load existing contract to preserve scope and enforcement_mode.
    let existing = svc.get_contract(&root, id).map_err(|e| e.to_string())?;

    let updated = Contract {
        id,
        title: input.title,
        provider: input.provider.into(),
        consumers: input.consumers.into_iter().map(Into::into).collect(),
        project: existing.project,  // preserve existing value; field superseded by ContractParty
        version: input.version,
        status: existing.status,
        effective_date,
        expiry_date,
        // Merged inside the service from kept_document_paths + new_document_paths.
        document_paths: vec![],
        enforcement_mode: existing.enforcement_mode,
        scope: existing.scope,
        policy: input.policy.into(),
        drift_count: existing.drift_count,
        breach_count: existing.breach_count,
        endpoint_count: existing.endpoint_count,
        created_by: existing.created_by,
        created_at: existing.created_at,
        updated_at: existing.updated_at,  // refreshed by the service
    };

    svc.update_contract(&root, updated, input.new_document_paths, input.kept_document_paths)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_contracts(
    collection_root: String,
    svc: State<'_, ContractService>,
) -> Result<Vec<ContractDto>, String> {
    svc.list_contracts(&PathBuf::from(&collection_root))
        .map(|v| v.iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_contract(
    collection_root: String,
    contract_id: String,
    svc: State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.get_contract(&PathBuf::from(&collection_root), id)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_contract(
    collection_root: String,
    contract_id: String,
    svc: State<'_, ContractService>,
) -> Result<(), String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.delete_contract(&PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_contract_changelog(
    collection_root: String,
    contract_id: String,
    svc: State<'_, ContractService>,
) -> Result<ContractChangelogDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.get_changelog(&PathBuf::from(&collection_root), id)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn publish_contract(
    collection_root: String,
    contract_id: String,
    snapshots: Vec<RequestSignatureSnapshotDto>,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    let domain_snapshots: Vec<rocket_collection::contract::snapshot::RequestSignatureSnapshot> =
        snapshots.into_iter().map(Into::into).collect();
    svc.publish_contract(&PathBuf::from(&collection_root), id, domain_snapshots)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn accept_drift(
    collection_root: String,
    contract_id: String,
    new_version: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.accept_drift(&PathBuf::from(&collection_root), id, new_version)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pause_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Pause,
    )
    .map(|c| (&c).into())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resume_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Resume,
    )
    .map(|c| (&c).into())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn renew_contract(
    collection_root: String,
    contract_id: String,
    new_expires_at: Option<String>,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    let expiry = new_expires_at
        .as_deref()
        .map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("invalid expiresAt: {e}"))?;
    svc.renew_contract(&PathBuf::from(&collection_root), id, expiry)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn send_for_review(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::SendForReview,
    )
    .map(|c| (&c).into())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn approve_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Approve,
    )
    .map(|c| (&c).into())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reject_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Reject,
    )
    .map(|c| (&c).into())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn archive_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Archive,
    )
    .map(|c| (&c).into())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn unarchive_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Unarchive,
    )
    .map(|c| (&c).into())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duplicate_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<ContractDto, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.duplicate_contract(&PathBuf::from(&collection_root), id)
        .map(|c| (&c).into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn recompute_drift(
    collection_root: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Vec<ContractDriftSummaryDto>, String> {
    svc.recompute_drift_for_collection(&std::path::PathBuf::from(&collection_root))
        .map(|v| v.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_contract_summary(
    collection_root: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Vec<ContractSummaryDto>, String> {
    svc.list_summaries(&PathBuf::from(&collection_root))
        .map(|v| v.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

/// Returns an OpenAPI 3.0 YAML stub for a contract as a String.
/// The frontend is responsible for triggering the save dialog.
#[tauri::command]
pub fn export_contract_openapi(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<String, String> {
    let id = ulid::Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.export_as_openapi_yaml(&std::path::PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}
