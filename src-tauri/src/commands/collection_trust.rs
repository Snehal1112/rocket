use rocket_app::{Capability, CollectionTrustService, TrustStatus};
use rocket_collection::normalize_root;
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

/// Requested, granted and effective value of one boolean capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityStateDto {
    pub requested: bool,
    pub granted: bool,
    pub effective: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRootsStateDto {
    pub requested: Vec<String>,
    pub granted: Vec<String>,
    pub effective: Vec<String>,
    /// Requested roots that are not approved yet.
    pub pending: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessEnvStateDto {
    pub granted: bool,
}

/// What a collection asks for, what the user allowed and what actually applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionTrustDto {
    pub developer_mode: CapabilityStateDto,
    pub context_roots: ContextRootsStateDto,
    pub agent_run: CapabilityStateDto,
    pub process_env: ProcessEnvStateDto,
    /// Any capability is requested and not granted.
    pub pending: bool,
    /// Identifies the request that was shown, for `grant_requested_capabilities`.
    pub fingerprint: String,
    /// Set when the trust store could not be read.
    pub store_error: Option<String>,
}

impl From<TrustStatus> for CollectionTrustDto {
    fn from(status: TrustStatus) -> Self {
        let grant = status.grant.unwrap_or_default();
        let developer_mode = CapabilityStateDto {
            requested: status.requested.developer_mode,
            granted: grant.developer_mode,
            effective: status.effective.sandbox_mode
                == rocket_collection::settings::SandboxMode::Developer,
        };
        let agent_run = CapabilityStateDto {
            requested: status.requested.agent_run,
            granted: grant.agent_run,
            effective: status.effective.agent_run,
        };
        let requested_roots: Vec<String> = {
            let mut out: Vec<String> = Vec::new();
            for root in &status.requested.context_roots {
                let normalized = normalize_root(root);
                if !normalized.is_empty() && !out.contains(&normalized) {
                    out.push(normalized);
                }
            }
            out
        };
        let pending_roots: Vec<String> = requested_roots
            .iter()
            .filter(|r| !grant.context_roots.contains(r))
            .cloned()
            .collect();
        let pending = (developer_mode.requested && !developer_mode.granted)
            || (agent_run.requested && !agent_run.granted)
            || !pending_roots.is_empty();
        Self {
            developer_mode,
            context_roots: ContextRootsStateDto {
                requested: requested_roots,
                granted: grant.context_roots,
                effective: status.effective.context_roots,
                pending: pending_roots,
            },
            agent_run,
            process_env: ProcessEnvStateDto {
                granted: grant.process_env,
            },
            pending,
            fingerprint: status.fingerprint,
            store_error: status.store_error,
        }
    }
}

/// IPC name of a capability the banner can approve as requested.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestedCapabilityDto {
    DeveloperMode,
    ContextRoots,
    AgentRun,
}

impl From<RequestedCapabilityDto> for Capability {
    fn from(dto: RequestedCapabilityDto) -> Self {
        match dto {
            RequestedCapabilityDto::DeveloperMode => Capability::DeveloperMode,
            RequestedCapabilityDto::ContextRoots => Capability::ContextRoots,
            RequestedCapabilityDto::AgentRun => Capability::AgentRun,
        }
    }
}

/// The trust state of a collection on this computer.
#[tauri::command]
pub fn get_collection_trust(
    collection: String,
    svc: State<'_, CollectionTrustService>,
) -> Result<CollectionTrustDto, DomainError> {
    svc.status(&collection).map(Into::into)
}

/// Turns one capability on or off for a collection on this computer. This is the only
/// write path for capabilities. `save_collection_settings` ignores them.
#[tauri::command]
pub fn set_collection_capability(
    collection: String,
    capability: CapabilityDto,
    enabled: bool,
    svc: State<'_, CollectionTrustService>,
) -> Result<CollectionTrustDto, DomainError> {
    svc.set_capability(&collection, capability.into(), enabled)?;
    svc.status(&collection).map(Into::into)
}

/// Writes the extra script folders to the collection file and approves exactly those.
#[tauri::command]
pub fn set_collection_context_roots(
    collection: String,
    roots: Vec<String>,
    svc: State<'_, CollectionTrustService>,
) -> Result<CollectionTrustDto, DomainError> {
    svc.set_context_roots(&collection, roots)?;
    svc.status(&collection).map(Into::into)
}

/// Approves what the collection file asks for. Refused when the request changed since
/// the fingerprint was read.
#[tauri::command]
pub fn grant_requested_capabilities(
    collection: String,
    capabilities: Vec<RequestedCapabilityDto>,
    expected_fingerprint: String,
    svc: State<'_, CollectionTrustService>,
) -> Result<CollectionTrustDto, DomainError> {
    let caps: Vec<Capability> = capabilities.into_iter().map(Into::into).collect();
    svc.grant_requested(&collection, &caps, &expected_fingerprint)?;
    svc.status(&collection).map(Into::into)
}

/// Removes every grant of the collection. The collection file is not touched.
#[tauri::command]
pub fn revoke_collection_trust(
    collection: String,
    svc: State<'_, CollectionTrustService>,
) -> Result<CollectionTrustDto, DomainError> {
    svc.revoke(&collection)?;
    svc.status(&collection).map(Into::into)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::settings::SandboxMode;
    use rocket_collection::{
        CollectionGrant, EffectiveCapabilities, GrantSource, RequestedElevation,
    };

    fn status(requested: RequestedElevation, grant: Option<CollectionGrant>) -> TrustStatus {
        let effective = rocket_collection::resolve_effective(&requested, grant.as_ref());
        TrustStatus {
            fingerprint: rocket_collection::request_fingerprint(&requested),
            requested,
            grant,
            effective,
            store_error: None,
        }
    }

    fn asking_for_everything() -> RequestedElevation {
        RequestedElevation {
            developer_mode: true,
            context_roots: vec!["./shared/".into(), "other".into()],
            agent_run: true,
        }
    }

    #[test]
    fn a_request_without_a_grant_is_pending_and_not_effective() {
        let dto = CollectionTrustDto::from(status(asking_for_everything(), None));
        assert!(dto.pending);
        assert!(dto.developer_mode.requested && !dto.developer_mode.granted);
        assert!(!dto.developer_mode.effective && !dto.agent_run.effective);
        assert_eq!(dto.context_roots.requested, vec!["shared", "other"]);
        assert_eq!(dto.context_roots.pending, vec!["shared", "other"]);
        assert!(dto.context_roots.effective.is_empty());
        assert!(!dto.process_env.granted);
    }

    #[test]
    fn a_full_grant_has_nothing_pending() {
        let grant = CollectionGrant {
            developer_mode: true,
            context_roots: vec!["shared".into(), "other".into()],
            agent_run: true,
            process_env: true,
            source: GrantSource::User,
        };
        let dto = CollectionTrustDto::from(status(asking_for_everything(), Some(grant)));
        assert!(!dto.pending);
        assert!(dto.developer_mode.effective && dto.agent_run.effective);
        assert_eq!(dto.context_roots.effective, vec!["shared", "other"]);
        assert!(dto.process_env.granted);
    }

    #[test]
    fn a_new_root_is_pending_while_developer_mode_stays_effective() {
        let grant = CollectionGrant {
            developer_mode: true,
            context_roots: vec!["shared".into()],
            ..Default::default()
        };
        let requested = RequestedElevation {
            developer_mode: true,
            context_roots: vec!["shared".into(), "new".into()],
            agent_run: false,
        };
        let dto = CollectionTrustDto::from(status(requested, Some(grant)));
        assert!(dto.pending);
        assert_eq!(dto.context_roots.pending, vec!["new"]);
        assert_eq!(dto.context_roots.effective, vec!["shared"]);
        assert!(dto.developer_mode.effective);
    }

    #[test]
    fn nothing_requested_is_not_pending() {
        let dto = CollectionTrustDto::from(status(RequestedElevation::default(), None));
        assert!(!dto.pending);
        let untrusted = EffectiveCapabilities::untrusted();
        assert_eq!(untrusted.sandbox_mode, SandboxMode::Safe);
    }

    #[test]
    fn the_dto_uses_camel_case_keys() {
        let dto = CollectionTrustDto::from(status(RequestedElevation::default(), None));
        let value = serde_json::to_value(&dto).expect("serialize");
        assert!(value.get("developerMode").is_some());
        assert!(value.get("storeError").is_some());
        assert!(value["contextRoots"].get("pending").is_some());
    }
}
