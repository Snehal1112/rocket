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
}
