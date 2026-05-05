//! HTTP request structs for the OpenCollection YAML format.

use rocket_shared::assertion::Assertion as OcAssertion;
use rocket_shared::description::Description as OcDescription;
use serde::{Deserialize, Serialize};

use super::auth::{OcAuth, OcHttpRequestSettings};
use super::variables::OcVariable;

/// HTTP request metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequestInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub request_type: Option<String>,  // "http"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// HTTP request parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequestParam {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub param_type: Option<String>,  // "query" | "path"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// HTTP request header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequestHeader {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// HTTP response header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcHttpResponseHeader {
    pub name: String,
    pub value: String,
}

/// Form field for form-urlencoded body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcFormField {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// Multipart form part value — string or array of strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcMultipartValue {
    Single(String),
    Multiple(Vec<String>),
}

/// Multipart form part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcMultipartFormPart {
    pub name: String,
    #[serde(rename = "type")]
    pub part_type: String,  // "text" | "file"
    pub value: OcMultipartValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// File body variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcFileBodyVariant {
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default)]
    pub selected: bool,
}

/// HTTP request body — discriminated by `type` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OcHttpRequestBody {
    #[serde(rename = "json")]
    Json { data: String },
    #[serde(rename = "text")]
    Text { data: String },
    #[serde(rename = "xml")]
    Xml { data: String },
    #[serde(rename = "sparql")]
    Sparql { data: String },
    #[serde(rename = "form-urlencoded")]
    FormUrlEncoded { data: Vec<OcFormField> },
    #[serde(rename = "multipart-form")]
    MultipartForm { data: Vec<OcMultipartFormPart> },
    #[serde(rename = "file")]
    File { data: Vec<OcFileBodyVariant> },
}

/// HTTP request body variant (named variant with title + selected).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequestBodyVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub body: OcHttpRequestBody,
}

/// Script for a specific lifecycle stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcScript {
    #[serde(rename = "type")]
    pub script_type: String,  // "before-request" | "after-response" | "tests" | "hooks"
    pub code: String,
}

/// Type alias for a list of scripts.
#[allow(dead_code)]
pub type OcScripts = Vec<OcScript>;

/// External script file reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcScriptFile {
    #[serde(rename = "type")]
    pub script_file_type: String,  // "script"
    pub script: String,
}

/// Action selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcActionSelector {
    pub expression: String,
    pub method: String,  // "jsonq"
}

/// Action target variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcActionVariable {
    pub name: String,
    pub scope: String,  // "runtime" | "request" | "folder" | "collection" | "environment"
}

/// Action — currently only set-variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OcAction {
    #[serde(rename = "set-variable", rename_all = "camelCase")]
    SetVariable {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<OcDescription>,
        phase: String,  // "before-request" | "after-response"
        selector: OcActionSelector,
        variable: OcActionVariable,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disabled: Option<bool>,
    },
}

/// HTTP request runtime configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OcHttpRequestRuntime {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<OcVariable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<OcScript>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<OcAssertion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<OcAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// Response body in an example.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcExampleResponseBody {
    #[serde(rename = "type")]
    pub body_type: String,  // "json" | "text" | "xml" | "html" | "binary"
    pub data: String,
}

/// Example request snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcExampleRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<OcHttpRequestHeader>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<OcHttpRequestParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<OcHttpRequestBody>,
}

/// Example response snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcExampleResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<OcHttpResponseHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<OcExampleResponseBody>,
}

/// HTTP request/response example.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequestExample {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<OcExampleRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<OcExampleResponse>,
}

/// HTTP request protocol details — method, url, headers, params, body, auth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequestDetails {
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<OcHttpRequestHeader>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<OcHttpRequestParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<OcHttpRequestBody>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// Complete HTTP request — top-level YAML file struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequest {
    /// Stable identity for tab deduplication across reloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    pub info: OcHttpRequestInfo,
    pub http: OcHttpRequestDetails,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<OcHttpRequestRuntime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<OcHttpRequestSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<OcHttpRequestExample>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}
