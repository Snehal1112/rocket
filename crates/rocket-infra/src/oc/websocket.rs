//! WebSocket request structs for the OpenCollection YAML format.

use rocket_shared::description::Description as OcDescription;
use serde::{Deserialize, Serialize};

use super::auth::OcAuth;
use super::http::{OcHttpRequestHeader, OcScript};
use super::variables::OcVariable;

/// WebSocket request info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcWebSocketRequestInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub request_type: Option<String>,  // "websocket"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// WebSocket message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcWebSocketMessage {
    #[serde(rename = "type")]
    pub message_type: String,  // "text" | "json" | "xml" | "binary"
    pub data: String,
}

/// WebSocket message variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcWebSocketMessageVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub message: OcWebSocketMessage,
}

/// WebSocket message — single or array of variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcWebSocketMessageOrVariants {
    Single(OcWebSocketMessage),
    Variants(Vec<OcWebSocketMessageVariant>),
}

/// WebSocket request protocol details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcWebSocketRequestDetails {
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<OcHttpRequestHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<OcWebSocketMessageOrVariants>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// WebSocket request runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcWebSocketRequestRuntime {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<OcVariable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<OcScript>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// Complete WebSocket request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcWebSocketRequest {
    pub info: OcWebSocketRequestInfo,
    pub websocket: OcWebSocketRequestDetails,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<OcWebSocketRequestRuntime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}
