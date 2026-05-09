//! Tauri IPC commands for the Contract Lock feature.
//!
//! Exposes `ContractService` to the frontend. The save-side audit hook
//! (`on_request_saved`) is wired into `commands::collections::save_request`,
//! not here — these commands only cover explicit user-driven CRUD.

use rocket_app::ContractService;
use rocket_collection::contract::{
    changelog::ContractChangelog,
    snapshot::RequestSignatureSnapshot,
    types::{Contract, ContractEnforcementMode, ContractParty, ContractStatus},
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
    pub provider: rocket_collection::contract::types::ContractParty,
    pub consumers: Vec<rocket_collection::contract::types::ContractParty>,
    pub version: String,
    pub effective_date: String,
    pub expiry_date: Option<String>,
    /// Absolute paths chosen by the file picker on the user's machine.
    /// The service validates, copies, and converts them to relative paths.
    pub document_paths: Vec<PathBuf>,
    pub scope: rocket_collection::contract::types::ContractScope,
    pub policy: rocket_collection::contract::types::ContractPolicy,
    pub initial_snapshots: Vec<RequestSignatureSnapshot>,
    /// If true, status is set to Active and snapshot taken on creation.
    /// If false, status is Draft and no snapshot is taken.
    pub publish_immediately: bool,
}

#[tauri::command]
pub fn attach_contract(
    collection_root: String,
    input: AttachContractInput,
    svc: State<'_, ContractService>,
) -> Result<Contract, String> {
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
        provider: input.provider,
        consumers: input.consumers,
        project: String::new(), // project field superseded by ContractParty identities; retained for backward compat
        version: input.version,
        status,
        effective_date,
        expiry_date,
        document_paths: vec![],
        enforcement_mode: ContractEnforcementMode::Informational,
        scope: input.scope,
        policy: input.policy,
        drift_count: 0,
        breach_count: 0,
        endpoint_count: 0, // overwritten by the service after the snapshot walk
        created_by: None,
        created_at: None, // set by the service
        updated_at: None, // set by the service
    };

    let snapshots = if input.publish_immediately { input.initial_snapshots } else { vec![] };

    svc.attach_contract(&root, contract, snapshots, input.document_paths)
        .map_err(|e| e.to_string())
}

/// Input DTO for updating contract metadata. Does not touch scope, snapshots,
/// or changelog — those are immutable once a contract is attached.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContractInput {
    pub contract_id: String,
    pub title: String,
    pub provider: String,
    pub consumer: String,
    pub project: String,
    pub version: String,
    pub effective_date: String,
    pub expiry_date: Option<String>,
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
) -> Result<Contract, String> {
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
        provider: ContractParty::from_name(&input.provider),
        consumers: vec![ContractParty::from_name(&input.consumer)],
        project: input.project,
        version: input.version,
        status: existing.status,
        effective_date,
        expiry_date,
        // Merged inside the service from kept_document_paths + new_document_paths.
        document_paths: vec![],
        enforcement_mode: existing.enforcement_mode,
        scope: existing.scope,
        policy: existing.policy,
        drift_count: existing.drift_count,
        breach_count: existing.breach_count,
        endpoint_count: existing.endpoint_count,
        created_by: existing.created_by,
        created_at: existing.created_at,
        updated_at: existing.updated_at,
    };

    svc.update_contract(&root, updated, input.new_document_paths, input.kept_document_paths)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_contracts(
    collection_root: String,
    svc: State<'_, ContractService>,
) -> Result<Vec<Contract>, String> {
    svc.list_contracts(&PathBuf::from(&collection_root))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_contract(
    collection_root: String,
    contract_id: String,
    svc: State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.get_contract(&PathBuf::from(&collection_root), id)
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
) -> Result<ContractChangelog, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.get_changelog(&PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn publish_contract(
    collection_root: String,
    contract_id: String,
    snapshots: Vec<rocket_collection::contract::snapshot::RequestSignatureSnapshot>,
    svc: tauri::State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.publish_contract(&PathBuf::from(&collection_root), id, snapshots)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pause_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Pause,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resume_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Resume,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn renew_contract(
    collection_root: String,
    contract_id: String,
    new_expires_at: Option<String>,
    svc: tauri::State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    let expiry = new_expires_at
        .as_deref()
        .map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("invalid expiresAt: {e}"))?;
    svc.renew_contract(&PathBuf::from(&collection_root), id, expiry)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn send_for_review(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::SendForReview,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn approve_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Approve,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reject_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.transition_contract_status(
        &PathBuf::from(&collection_root),
        id,
        rocket_collection::contract::StatusEvent::Reject,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duplicate_contract(
    collection_root: String,
    contract_id: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Contract, String> {
    let id = Ulid::from_string(&contract_id).map_err(|e| e.to_string())?;
    svc.duplicate_contract(&PathBuf::from(&collection_root), id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn recompute_drift(
    collection_root: String,
    current_snapshots: Vec<rocket_collection::contract::snapshot::RequestSignatureSnapshot>,
    svc: tauri::State<'_, ContractService>,
) -> Result<Vec<rocket_app::contract_service::ContractDriftSummary>, String> {
    svc.recompute_drift_for_collection(
        &std::path::PathBuf::from(&collection_root),
        &current_snapshots,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_contract_summary(
    collection_root: String,
    svc: tauri::State<'_, ContractService>,
) -> Result<Vec<rocket_app::contract_service::ContractSummary>, String> {
    svc.list_summaries(&PathBuf::from(&collection_root))
        .map_err(|e| e.to_string())
}
