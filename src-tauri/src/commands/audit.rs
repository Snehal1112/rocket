use chrono::{DateTime, Utc};
use rocket_app::SecurityAuditService;
use rocket_audit::{event::SecurityAuditEvent, profile::ComplianceProfile};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn list_audit_events(
    svc: State<'_, Arc<SecurityAuditService>>,
) -> Result<Vec<SecurityAuditEvent>, String> {
    svc.list().map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeInput {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[tauri::command]
pub fn list_audit_events_range(
    svc: State<'_, Arc<SecurityAuditService>>,
    input: RangeInput,
) -> Result<Vec<SecurityAuditEvent>, String> {
    svc.list_range(input.start, input.end)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_compliance_profile(
    svc: State<'_, Arc<SecurityAuditService>>,
) -> Result<ComplianceProfile, String> {
    svc.load_profile().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_compliance_profile(
    svc: State<'_, Arc<SecurityAuditService>>,
    profile: ComplianceProfile,
) -> Result<(), String> {
    svc.save_profile(&profile).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceExport {
    pub exported_at: DateTime<Utc>,
    pub range_start: DateTime<Utc>,
    pub range_end: DateTime<Utc>,
    pub events: Vec<SecurityAuditEvent>,
    pub chain_verified: bool,
}

#[tauri::command]
pub fn export_audit_evidence(
    svc: State<'_, Arc<SecurityAuditService>>,
    input: RangeInput,
) -> Result<EvidenceExport, String> {
    let events = svc
        .list_range(input.start, input.end)
        .map_err(|e| e.to_string())?;
    let chain_verified = matches!(
        rocket_audit::chain::verify_chain(&events),
        rocket_audit::chain::ChainVerification::Ok
    );
    Ok(EvidenceExport {
        exported_at: Utc::now(),
        range_start: input.start,
        range_end: input.end,
        events,
        chain_verified,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEvidenceInput {
    pub path: String,
    pub content: String,
}

/// Writes the evidence JSON to a user-chosen path on the native filesystem.
/// Using a Rust command sidesteps the plugin-fs scope restrictions that block
/// writes to arbitrary paths chosen via the save dialog.
#[tauri::command]
pub fn save_audit_evidence_file(input: SaveEvidenceInput) -> Result<(), String> {
    std::fs::write(&input.path, &input.content).map_err(|e| e.to_string())
}
