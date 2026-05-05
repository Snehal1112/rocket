//! Collection top-level structs for the OpenCollection YAML format.

use rocket_shared::certificate::ClientCertificate as OcClientCertificate;
use serde::{Deserialize, Serialize};

use super::auth::OcProxy;
use super::defaults::OcRequestDefaults;
use super::environment::OcEnvironment;
use super::folder::OcItem;

/// Collection info metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<OcAuthor>>,
}

/// Author info.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Protobuf file item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcProtoFileItem {
    pub file_path: String,
}

/// Protobuf import path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcProtoFileImportPath {
    pub path: String,
}

/// Protobuf configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcProtobuf {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proto_files: Vec<OcProtoFileItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub import_paths: Vec<OcProtoFileImportPath>,
}

/// Collection config — environments, protobuf, proxy, client certificates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcCollectionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environments: Option<Vec<OcEnvironment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protobuf: Option<OcProtobuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<OcProxy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_certificates: Option<Vec<OcClientCertificate>>,
}

/// Top-level OpenCollection document (opencollection.yml).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcCollection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencollection: Option<String>,  // spec version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<OcInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<OcCollectionConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<OcItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<OcRequestDefaults>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_yaml::Value>,
}

