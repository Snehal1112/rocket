//! Environment struct for the OpenCollection YAML format.

use rocket_shared::certificate::ClientCertificate as OcClientCertificate;
use rocket_shared::description::Description as OcDescription;
use serde::{Deserialize, Serialize};

use super::variables::OcVariable;

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
    pub variables: Vec<OcVariable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_certificates: Vec<OcClientCertificate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dot_env_file_path: Option<String>,
}
