//! Folder and item structs for the OpenCollection YAML format.

use rocket_shared::description::Description as OcDescription;
use serde::{Deserialize, Serialize};

use super::defaults::OcRequestDefaults;
use super::graphql::OcGraphQLRequest;
use super::grpc::OcGrpcRequest;
use super::http::OcScriptFile;
use super::http::OcHttpRequest;
use super::websocket::OcWebSocketRequest;

/// Folder info metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcFolderInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub folder_type: Option<String>,  // "folder"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Folder-level request defaults (variables, auth, headers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<OcRequestDefaults>,
}

impl Default for OcFolderInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            uid: None,
            description: None,
            folder_type: Some("folder".into()),
            seq: None,
            tags: Vec::new(),
            request: None,
        }
    }
}

/// Folder — contains nested items and optional request defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcFolder {
    pub info: OcFolderInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<OcItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<OcRequestDefaults>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

/// Item — dispatches to any request type, folder, or script file.
/// Order matters: serde tries variants top-to-bottom with untagged.
/// More specific types (with unique required fields) should come first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcItem {
    Http(OcHttpRequest),
    GraphQL(OcGraphQLRequest),
    Grpc(OcGrpcRequest),
    WebSocket(OcWebSocketRequest),
    Folder(OcFolder),
    ScriptFile(OcScriptFile),
}
