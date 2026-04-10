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
    pub document_path: Option<PathBuf>,
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
        document_path: input.document_path,
        // Forced to Informational inside the service; set here for shape only.
        enforcement_mode: ContractEnforcementMode::Informational,
        scope: input.scope,
    };

    svc.attach_contract(&root, contract, input.initial_snapshots)
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
