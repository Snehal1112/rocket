//! Tauri IPC commands for the Contract Lock feature.
//!
//! Exposes `ContractService` to the frontend. The save-side audit hook
//! (`on_request_saved`) is wired into `commands::collections::save_request`,
//! not here — these commands only cover explicit user-driven CRUD.

use rocket_app::ContractService;
use rocket_collection::contract::{
    changelog::ContractChangelog,
    snapshot::RequestSignatureSnapshot,
    types::{Contract, ContractEnforcementMode, ContractScope},
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
    pub provider: String,
    pub consumer: String,
    pub project: String,
    pub version: String,
    pub effective_date: String,
    pub expiry_date: Option<String>,
    /// Absolute paths chosen by the file picker on the user's machine.
    /// The service validates, copies, and converts them to relative paths.
    pub document_paths: Vec<PathBuf>,
    pub scope: ContractScope,
    pub initial_snapshots: Vec<RequestSignatureSnapshot>,
}

#[tauri::command]
pub fn attach_contract(
    collection_root: String,
    input: AttachContractInput,
    svc: State<'_, ContractService>,
) -> Result<Contract, String> {
    let root = PathBuf::from(&collection_root);

    // The collection name is derived from the final path component of the
    // collection root. This works for embedded collections whose root is
    // `<workspace>/collections/<name>`. External collections (linked via
    // WorkspaceService::link_external_collection) have arbitrary roots, so
    // the stem may not match the name key used by CollectionRepository::get.
    // Full external-collection support is tracked in the workspace-mismatch
    // fix (collection_root_for on CollectionRepository).
    let collection_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "collectionRoot must have a final path component".to_string())?
        .to_string();

    let effective_date = chrono::NaiveDate::parse_from_str(&input.effective_date, "%Y-%m-%d")
        .map_err(|e| format!("invalid effectiveDate: {}", e))?;

    let expiry_date = input
        .expiry_date
        .as_deref()
        .map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("invalid expiryDate: {}", e))?;

    let contract = Contract {
        // Overwritten inside ContractService::attach_contract — placeholder only.
        id: Ulid::new(),
        title: input.title,
        provider: input.provider,
        consumer: input.consumer,
        project: input.project,
        version: input.version,
        effective_date,
        expiry_date,
        // Populated by the service after copying files; empty here.
        document_paths: vec![],
        // Forced to Informational inside the service; set here for shape only.
        enforcement_mode: ContractEnforcementMode::Informational,
        scope: input.scope,
    };

    svc.attach_contract(&root, &collection_name, contract, input.initial_snapshots, input.document_paths)
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
        provider: input.provider,
        consumer: input.consumer,
        project: input.project,
        version: input.version,
        effective_date,
        expiry_date,
        // Merged inside the service from kept_document_paths + new_document_paths.
        document_paths: vec![],
        enforcement_mode: existing.enforcement_mode,
        scope: existing.scope,
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
