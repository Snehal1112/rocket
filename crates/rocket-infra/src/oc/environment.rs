//! Environment struct for the OpenCollection YAML format.

use rocket_shared::certificate::ClientCertificate as OcClientCertificate;
use rocket_shared::description::Description as OcDescription;
use serde::{Deserialize, Serialize};

use super::variables::OcEnvVariableEntry;

/// Mirrors `rocket_environment::external_secret::ExternalSecretRef` for the
/// OpenCollection YAML format. Never carries a secret *value* — only a
/// captured name and RocketVault's own UUID (see spec §4.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcExternalSecretRef {
    pub name: String,
    pub secret_id: String,
}

/// Mirrors `rocket_environment::external_secret::ExternalSecretBinding` for
/// the OpenCollection YAML format. A new top-level section on
/// `OcEnvironment` (`externalSecrets`), not routed through
/// `OcVariable`/`OcSecretVariable` — this is not a variable entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcExternalSecretBinding {
    pub alias: String,
    pub connection_id: String,
    pub vault_name: String,
    #[serde(default)]
    pub secret_names: Vec<OcExternalSecretRef>,
}

/// Environment for collection config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcEnvironment {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<OcEnvVariableEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_secrets: Vec<OcExternalSecretBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_certificates: Vec<OcClientCertificate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dot_env_file_path: Option<String>,
}
