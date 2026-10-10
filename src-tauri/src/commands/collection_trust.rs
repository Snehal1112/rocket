use rocket_app::{Capability, CollectionTrustService};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

/// IPC name of a capability a user can switch for a collection on this computer.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityDto {
    DeveloperMode,
    AgentRun,
    ProcessEnv,
}

impl From<CapabilityDto> for Capability {
    fn from(dto: CapabilityDto) -> Self {
        match dto {
            CapabilityDto::DeveloperMode => Capability::DeveloperMode,
            CapabilityDto::AgentRun => Capability::AgentRun,
            CapabilityDto::ProcessEnv => Capability::ProcessEnv,
        }
    }
}

/// Turns one capability on or off for a collection on this computer. This is the only
/// write path for capabilities. `save_collection_settings` ignores them.
#[tauri::command]
pub fn set_collection_capability(
    collection: String,
    capability: CapabilityDto,
    enabled: bool,
    svc: State<'_, CollectionTrustService>,
) -> Result<(), DomainError> {
    svc.set_capability(&collection, capability.into(), enabled)
}

/// One collection that was grandfathered with elevated capabilities.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustMigrationEntryDto {
    pub name: String,
    pub path: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustMigrationNoticeDto {
    pub collections: Vec<TrustMigrationEntryDto>,
}

/// Collections that kept elevated capabilities at the upgrade, until dismissed.
#[tauri::command]
pub fn get_trust_migration_notice(
    svc: State<'_, CollectionTrustService>,
) -> Result<TrustMigrationNoticeDto, DomainError> {
    let collections = svc
        .migration_notice()?
        .into_iter()
        .map(|entry| {
            let name = std::path::Path::new(&entry.root)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.root.clone());
            TrustMigrationEntryDto {
                name,
                path: entry.root,
                capabilities: entry.capabilities,
            }
        })
        .collect();
    Ok(TrustMigrationNoticeDto { collections })
}

#[tauri::command]
pub fn dismiss_trust_migration_notice(
    svc: State<'_, CollectionTrustService>,
) -> Result<(), DomainError> {
    svc.dismiss_migration_notice()
}
