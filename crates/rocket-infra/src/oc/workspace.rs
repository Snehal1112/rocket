//! Workspace file format structs for the OpenCollection YAML format.

use serde::{Deserialize, Serialize};

/// workspace.yml — info block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcWorkspaceInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub workspace_type: Option<String>,
}

/// workspace.yml — single collection entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcWorkspaceCollectionRef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<std::path::PathBuf>,
}

/// workspace.yml — environments block (Rocket extension).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcWorkspaceEnvironments {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_environment: Option<String>,
}

/// workspace.yml — opt-in request-mutation host guard policy (Rocket extension).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OcRequestGuardPolicy {
    #[serde(default)]
    pub block_script_redirects_to_internal_hosts: bool,
    #[serde(default)]
    pub also_block_private_ranges: bool,
}

/// Top-level workspace.yml document.
/// Follows Bruno's OpenCollection workspace extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcWorkspaceConfig {
    /// Spec version — always "1.0.0" when written by Rocket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencollection: Option<String>,
    /// Required: workspace name and type.
    pub info: OcWorkspaceInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<OcWorkspaceCollectionRef>,
    /// Human-readable description (spec field name is `docs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// Active environment selection (Rocket extension).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environments: Option<OcWorkspaceEnvironments>,
    /// Global environment override (Rocket extension).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_environment: Option<String>,
    /// Opt-in request-mutation host guard policy (Rocket extension).
    #[serde(default)]
    pub request_guard_policy: OcRequestGuardPolicy,
}
