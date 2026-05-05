//! gRPC request structs for the OpenCollection YAML format.

use rocket_shared::assertion::Assertion as OcAssertion;
use rocket_shared::description::Description as OcDescription;
use serde::{Deserialize, Serialize};

use super::auth::OcAuth;
use super::http::{OcScript};
use super::variables::OcVariable;

/// gRPC request info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGrpcRequestInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub request_type: Option<String>,  // "grpc"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// gRPC metadata entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGrpcMetadata {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// gRPC message variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcGrpcMessageVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub message: String,
}

/// gRPC message — string or array of variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcGrpcMessageOrVariants {
    Single(String),
    Variants(Vec<OcGrpcMessageVariant>),
}

/// gRPC request protocol details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcGrpcRequestDetails {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method_type: Option<String>,  // "unary" | "client-streaming" | "server-streaming" | "bidi-streaming"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proto_file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<OcGrpcMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<OcGrpcMessageOrVariants>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// gRPC request runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGrpcRequestRuntime {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<OcVariable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<OcScript>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<OcAssertion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// Complete gRPC request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGrpcRequest {
    pub info: OcGrpcRequestInfo,
    pub grpc: OcGrpcRequestDetails,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<OcGrpcRequestRuntime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}
