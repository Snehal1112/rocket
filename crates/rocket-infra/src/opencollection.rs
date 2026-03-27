//! OpenCollection YAML file-format structs.
//! These mirror the OpenCollection JSON schema for on-disk YAML serialization.
//! Domain types from rocket-shared are re-used where field names match.

use serde::{Deserialize, Serialize};

// Re-export domain types that map directly to schema types.
pub use rocket_shared::description::{Description as OcDescription, Documentation as OcDocumentation};
pub use rocket_shared::variable_value::{VariableValue as OcVariableValue, VariableValueVariant as OcVariableValueVariant};

/// OpenCollection Variable — schema field names: name, value, description, disabled.
/// Our domain Variable uses `key` instead of `name` and `enabled` instead of `disabled`,
/// so we need a separate YAML struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcVariable {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<OcVariableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// OpenCollection SecretVariable — schema: { secret: true, name, description, disabled, type }.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcSecretVariable {
    pub secret: bool,  // always true
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub secret_type: Option<String>,  // "string"|"number"|"boolean"|"null"|"object"
}

/// OpenCollection Auth — discriminated by `type` field. String "inherit" for inheritance.
/// Uses custom serde since it's a oneOf with a string shorthand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcAuth {
    /// String shorthand: "inherit".
    Inherit(String),
    /// Object form: dispatched by `type` field.
    Typed(OcAuthTyped),
}

/// Typed auth — discriminated by `type` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OcAuthTyped {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "basic")]
    Basic { username: String, password: String },
    #[serde(rename = "bearer")]
    Bearer { token: String },
    #[serde(rename = "apikey", rename_all = "camelCase")]
    ApiKey {
        key: String,
        value: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placement: Option<String>,
    },
    #[serde(rename = "digest")]
    Digest { username: String, password: String },
    #[serde(rename = "ntlm")]
    Ntlm {
        username: String,
        password: String,
        domain: String,
    },
    #[serde(rename = "wsse")]
    Wsse { username: String, password: String },
    #[serde(rename = "awsv4", rename_all = "camelCase")]
    AwsV4 {
        access_key_id: String,
        secret_access_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_token: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        service: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile_name: Option<String>,
    },
    #[serde(rename = "oauth2", rename_all = "camelCase")]
    OAuth2 {
        flow: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        access_token_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refresh_token_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authorization_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        callback_url: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credentials: Option<OcOAuth2Credentials>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource_owner: Option<OcOAuth2ResourceOwner>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkce: Option<OcOAuth2PKCE>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        additional_parameters: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token_config: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<serde_json::Value>,
    },
}

/// OAuth2 client credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcOAuth2Credentials {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
}

/// OAuth2 resource owner credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcOAuth2ResourceOwner {
    pub username: String,
    pub password: String,
}

/// OAuth2 PKCE configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcOAuth2PKCE {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

/// A value that can be a boolean or the string "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InheritableBoolean {
    Value(bool),
    Inherit(String),  // "inherit"
}

/// A value that can be a number or the string "inherit".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InheritableNumber {
    Value(f64),
    Inherit(String),  // "inherit"
}

/// HTTP request execution settings.
/// Schema: { encodeUrl, timeout, followRedirects, maxRedirects }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcHttpRequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_url: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<InheritableNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<InheritableNumber>,
}

/// GraphQL request execution settings (same fields as HTTP settings).
/// Schema: { encodeUrl, timeout, followRedirects, maxRedirects }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcGraphQLRequestSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_url: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<InheritableNumber>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<InheritableBoolean>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<InheritableNumber>,
}

// ============================================================
// HTTP Request — Detail Structs
// ============================================================

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcHttpRequestBodyVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub body: OcHttpRequestBody,
}

// ============================================================
// HTTP Request — Runtime Structs
// ============================================================

/// Script for a specific lifecycle stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcScript {
    #[serde(rename = "type")]
    pub script_type: String,  // "before-request" | "after-response" | "tests" | "hooks"
    pub code: String,
}

/// Type alias for a list of scripts.
pub type OcScripts = Vec<OcScript>;

/// External script file reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcScriptFile {
    #[serde(rename = "type")]
    pub script_file_type: String,  // "script"
    pub script: String,
}

/// Assertion for response validation.
/// Re-uses domain Assertion which already has the correct serde.
pub use rocket_shared::assertion::Assertion as OcAssertion;

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

// ============================================================
// HTTP Request — Top-Level
// ============================================================

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

// ============================================================
// GraphQL Request
// ============================================================

/// GraphQL request info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGraphQLRequestInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub request_type: Option<String>,  // "graphql"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// GraphQL body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcGraphQLBody {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<String>,
}

/// GraphQL body variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcGraphQLBodyVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub body: OcGraphQLBody,
}

/// GraphQL body — either a single body or array of variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcGraphQLBodyOrVariants {
    Single(OcGraphQLBody),
    Variants(Vec<OcGraphQLBodyVariant>),
}

/// GraphQL request protocol details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGraphQLRequestDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<OcHttpRequestHeader>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<OcHttpRequestParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<OcGraphQLBodyOrVariants>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<OcAuth>,
}

/// GraphQL request runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGraphQLRequestRuntime {
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

/// Complete GraphQL request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcGraphQLRequest {
    pub info: OcGraphQLRequestInfo,
    pub graphql: OcGraphQLRequestDetails,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<OcGraphQLRequestRuntime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<OcGraphQLRequestSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

// ============================================================
// gRPC Request
// ============================================================

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

// ============================================================
// WebSocket Request
// ============================================================

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

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::description::Description;
    use rocket_shared::variable_value::VariableValue;

    #[test]
    fn oc_description_yaml_string() {
        let yaml = "\"A simple description\"";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), Some("A simple description"));
    }

    #[test]
    fn oc_description_yaml_object() {
        let yaml = "content: \"# Docs\"\ntype: text/markdown";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), Some("# Docs"));
        assert_eq!(desc.content_type(), Some("text/markdown"));
    }

    #[test]
    fn oc_description_yaml_null() {
        let yaml = "null";
        let desc: OcDescription = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(desc.content(), None);
    }

    #[test]
    fn oc_variable_yaml_simple() {
        let yaml = "name: BASE_URL\nvalue: https://api.example.com";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.name, "BASE_URL");
        assert_eq!(var.value.as_ref().unwrap().data(), "https://api.example.com");
    }

    #[test]
    fn oc_variable_yaml_typed_value() {
        let yaml = "name: COUNT\nvalue:\n  type: number\n  data: \"42\"";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.value.as_ref().unwrap().value_type(), Some("number"));
        assert_eq!(var.value.as_ref().unwrap().data(), "42");
    }

    #[test]
    fn oc_variable_yaml_with_description_and_disabled() {
        let yaml = "name: HOST\nvalue: localhost\ndescription: The API host\ndisabled: true";
        let var: OcVariable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(var.disabled, Some(true));
        assert!(var.description.is_some());
    }

    #[test]
    fn oc_secret_variable_yaml() {
        let yaml = "secret: true\nname: API_KEY\ntype: string\ndisabled: false";
        let sv: OcSecretVariable = serde_yaml::from_str(yaml).unwrap();
        assert!(sv.secret);
        assert_eq!(sv.name, "API_KEY");
        assert_eq!(sv.secret_type, Some("string".into()));
    }

    #[test]
    fn oc_variable_yaml_roundtrip() {
        let var = OcVariable {
            name: "HOST".into(),
            value: Some(VariableValue::simple("localhost")),
            description: Some(Description::text("Server host")),
            disabled: Some(false),
        };
        let yaml = serde_yaml::to_string(&var).unwrap();
        let back: OcVariable = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(var, back);
    }

    #[test]
    fn oc_variable_value_variant_yaml() {
        let yaml = "title: Production\nselected: true\nvalue: https://prod.example.com";
        let variant: OcVariableValueVariant = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(variant.title, "Production");
        assert!(variant.selected);
    }

    #[test]
    fn oc_auth_inherit_yaml() {
        let yaml = "inherit";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Inherit(s) if s == "inherit"));
    }

    #[test]
    fn oc_auth_basic_yaml() {
        let yaml = "type: basic\nusername: user\npassword: pass";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::Basic { username, password }) => {
                assert_eq!(username, "user");
                assert_eq!(password, "pass");
            }
            _ => panic!("expected Basic"),
        }
    }

    #[test]
    fn oc_auth_bearer_yaml() {
        let yaml = "type: bearer\ntoken: my-token-123";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::Bearer { .. })));
    }

    #[test]
    fn oc_auth_apikey_yaml() {
        let yaml = "type: apikey\nkey: X-API-Key\nvalue: abc123\nplacement: header";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::ApiKey {
                key,
                value,
                placement,
            }) => {
                assert_eq!(key, "X-API-Key");
                assert_eq!(value, "abc123");
                assert_eq!(placement, Some("header".into()));
            }
            _ => panic!("expected ApiKey"),
        }
    }

    #[test]
    fn oc_auth_awsv4_yaml() {
        let yaml =
            "type: awsv4\naccessKeyId: AKIA...\nsecretAccessKey: secret\nregion: us-east-1\nservice: s3";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::AwsV4 { .. })));
    }

    #[test]
    fn oc_auth_digest_yaml() {
        let yaml = "type: digest\nusername: admin\npassword: secret";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(auth, OcAuth::Typed(OcAuthTyped::Digest { .. })));
    }

    #[test]
    fn oc_auth_oauth2_client_credentials_yaml() {
        let yaml = "type: oauth2\nflow: client_credentials\naccessTokenUrl: https://auth.example.com/token\ncredentials:\n  clientId: my-id\n  clientSecret: my-secret";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::OAuth2 {
                flow,
                access_token_url,
                credentials,
                ..
            }) => {
                assert_eq!(flow, "client_credentials");
                assert_eq!(
                    access_token_url,
                    Some("https://auth.example.com/token".into())
                );
                assert!(credentials.is_some());
            }
            _ => panic!("expected OAuth2"),
        }
    }

    #[test]
    fn oc_auth_oauth2_authorization_code_yaml() {
        let yaml = "type: oauth2\nflow: authorization_code\nauthorizationUrl: https://auth.example.com/authorize\naccessTokenUrl: https://auth.example.com/token\ncredentials:\n  clientId: id\n  clientSecret: secret\npkce:\n  enabled: true\n  method: S256";
        let auth: OcAuth = serde_yaml::from_str(yaml).unwrap();
        match auth {
            OcAuth::Typed(OcAuthTyped::OAuth2 { flow, pkce, .. }) => {
                assert_eq!(flow, "authorization_code");
                assert!(pkce.is_some());
                assert_eq!(pkce.unwrap().method, Some("S256".into()));
            }
            _ => panic!("expected OAuth2"),
        }
    }

    #[test]
    fn inheritable_boolean_value() {
        let yaml = "true";
        let v: InheritableBoolean = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableBoolean::Value(true));
    }

    #[test]
    fn inheritable_boolean_inherit() {
        let yaml = "inherit";
        let v: InheritableBoolean = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableBoolean::Inherit("inherit".into()));
    }

    #[test]
    fn inheritable_number_value() {
        let yaml = "5000";
        let v: InheritableNumber = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableNumber::Value(5000.0));
    }

    #[test]
    fn inheritable_number_inherit() {
        let yaml = "inherit";
        let v: InheritableNumber = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(v, InheritableNumber::Inherit("inherit".into()));
    }

    #[test]
    fn oc_http_request_settings_yaml() {
        let yaml = "encodeUrl: true\ntimeout: 30000\nfollowRedirects: inherit\nmaxRedirects: 5";
        let settings: OcHttpRequestSettings = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(settings.encode_url, Some(InheritableBoolean::Value(true)));
        assert_eq!(settings.timeout, Some(InheritableNumber::Value(30000.0)));
        assert_eq!(settings.follow_redirects, Some(InheritableBoolean::Inherit("inherit".into())));
        assert_eq!(settings.max_redirects, Some(InheritableNumber::Value(5.0)));
    }

    #[test]
    fn oc_http_request_settings_roundtrip() {
        let settings = OcHttpRequestSettings {
            encode_url: Some(InheritableBoolean::Value(false)),
            timeout: Some(InheritableNumber::Inherit("inherit".into())),
            follow_redirects: None,
            max_redirects: Some(InheritableNumber::Value(10.0)),
        };
        let yaml = serde_yaml::to_string(&settings).unwrap();
        let back: OcHttpRequestSettings = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(settings, back);
    }

    #[test]
    fn oc_graphql_request_settings_yaml() {
        let yaml = "encodeUrl: false\ntimeout: inherit";
        let settings: OcGraphQLRequestSettings = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(settings.encode_url, Some(InheritableBoolean::Value(false)));
        assert_eq!(settings.timeout, Some(InheritableNumber::Inherit("inherit".into())));
    }

    #[test]
    fn oc_http_request_info_yaml() {
        let yaml = "name: Get Users\ntype: http\nseq: 1\ntags:\n  - api\n  - users";
        let info: OcHttpRequestInfo = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(info.name, "Get Users");
        assert_eq!(info.request_type, Some("http".into()));
        assert_eq!(info.seq, Some(1));
        assert_eq!(info.tags, vec!["api", "users"]);
    }

    #[test]
    fn oc_http_request_header_yaml() {
        let yaml = "name: Content-Type\nvalue: application/json\ndisabled: false";
        let header: OcHttpRequestHeader = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(header.name, "Content-Type");
        assert_eq!(header.disabled, Some(false));
    }

    #[test]
    fn oc_http_request_param_yaml() {
        let yaml = "name: page\nvalue: \"1\"\ntype: query";
        let param: OcHttpRequestParam = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(param.name, "page");
        assert_eq!(param.param_type, Some("query".into()));
    }

    #[test]
    fn oc_body_json_yaml() {
        let yaml = "type: json\ndata: '{\"key\": \"value\"}'";
        let body: OcHttpRequestBody = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(body, OcHttpRequestBody::Json { .. }));
    }

    #[test]
    fn oc_body_form_urlencoded_yaml() {
        let yaml = "type: form-urlencoded\ndata:\n  - name: username\n    value: admin\n  - name: password\n    value: secret\n    disabled: true";
        let body: OcHttpRequestBody = serde_yaml::from_str(yaml).unwrap();
        match body {
            OcHttpRequestBody::FormUrlEncoded { data } => {
                assert_eq!(data.len(), 2);
                assert_eq!(data[1].disabled, Some(true));
            }
            _ => panic!("expected FormUrlEncoded"),
        }
    }

    #[test]
    fn oc_body_multipart_yaml() {
        let yaml = "type: multipart-form\ndata:\n  - name: file\n    type: file\n    value:\n      - /path/to/file.txt\n      - /path/to/file2.txt";
        let body: OcHttpRequestBody = serde_yaml::from_str(yaml).unwrap();
        match body {
            OcHttpRequestBody::MultipartForm { data } => {
                assert_eq!(data.len(), 1);
                assert!(matches!(data[0].value, OcMultipartValue::Multiple(_)));
            }
            _ => panic!("expected MultipartForm"),
        }
    }

    #[test]
    fn oc_body_file_yaml() {
        let yaml = "type: file\ndata:\n  - filePath: /uploads/doc.pdf\n    contentType: application/pdf\n    selected: true";
        let body: OcHttpRequestBody = serde_yaml::from_str(yaml).unwrap();
        match body {
            OcHttpRequestBody::File { data } => {
                assert_eq!(data[0].file_path, "/uploads/doc.pdf");
                assert!(data[0].selected);
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn oc_body_variant_yaml() {
        let yaml = "title: Default\nselected: true\nbody:\n  type: json\n  data: '{}'";
        let variant: OcHttpRequestBodyVariant = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(variant.title, "Default");
        assert!(variant.selected);
    }

    #[test]
    fn oc_script_yaml() {
        let yaml = "type: before-request\ncode: \"console.log('hello')\"";
        let script: OcScript = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(script.script_type, "before-request");
        assert_eq!(script.code, "console.log('hello')");
    }

    #[test]
    fn oc_action_set_variable_yaml() {
        let yaml = "type: set-variable\nphase: after-response\nselector:\n  expression: res.body.token\n  method: jsonq\nvariable:\n  name: authToken\n  scope: collection";
        let action: OcAction = serde_yaml::from_str(yaml).unwrap();
        match action {
            OcAction::SetVariable { phase, selector, variable, .. } => {
                assert_eq!(phase, "after-response");
                assert_eq!(selector.expression, "res.body.token");
                assert_eq!(variable.scope, "collection");
            }
        }
    }

    #[test]
    fn oc_runtime_yaml() {
        let yaml = "scripts:\n  - type: before-request\n    code: \"let x = 1;\"\nassertions:\n  - expression: res.status\n    operator: eq\n    value: \"200\"";
        let runtime: OcHttpRequestRuntime = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(runtime.scripts.len(), 1);
        assert_eq!(runtime.assertions.len(), 1);
    }

    #[test]
    fn oc_http_request_example_yaml() {
        let yaml = "name: Success\nrequest:\n  url: https://api.example.com/users\n  method: GET\nresponse:\n  status: 200\n  statusText: OK\n  body:\n    type: json\n    data: '{\"users\": []}'";
        let example: OcHttpRequestExample = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(example.name, "Success");
        assert!(example.request.is_some());
        let resp = example.response.unwrap();
        assert_eq!(resp.status, Some(200));
        assert_eq!(resp.body.unwrap().body_type, "json");
    }

    #[test]
    fn oc_http_request_full_yaml() {
        let yaml = r#"
info:
  name: Create User
  type: http
  seq: 1
  tags:
    - users
    - api
http:
  method: POST
  url: "https://api.example.com/users"
  headers:
    - name: Content-Type
      value: application/json
    - name: Authorization
      value: "Bearer {{token}}"
  params:
    - name: version
      value: "2"
      type: query
  body:
    type: json
    data: '{"name": "John", "email": "john@example.com"}'
  auth:
    type: bearer
    token: "{{authToken}}"
runtime:
  scripts:
    - type: before-request
      code: "bru.setVar('timestamp', Date.now())"
  assertions:
    - expression: res.status
      operator: eq
      value: "201"
  actions:
    - type: set-variable
      phase: after-response
      selector:
        expression: res.body.id
        method: jsonq
      variable:
        name: userId
        scope: collection
settings:
  encodeUrl: true
  timeout: 30000
examples:
  - name: Success
    response:
      status: 201
      body:
        type: json
        data: '{"id": "123", "name": "John"}'
docs: "Creates a new user in the system."
"#;
        let request: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.info.name, "Create User");
        assert_eq!(request.info.seq, Some(1));
        assert_eq!(request.info.tags, vec!["users", "api"]);
        assert_eq!(request.http.method, "POST");
        assert_eq!(request.http.headers.len(), 2);
        assert_eq!(request.http.params.len(), 1);
        assert!(request.http.body.is_some());
        assert!(request.http.auth.is_some());
        let runtime = request.runtime.unwrap();
        assert_eq!(runtime.scripts.len(), 1);
        assert_eq!(runtime.assertions.len(), 1);
        assert_eq!(runtime.actions.len(), 1);
        let settings = request.settings.unwrap();
        assert_eq!(settings.encode_url, Some(InheritableBoolean::Value(true)));
        assert_eq!(request.examples.as_ref().unwrap().len(), 1);
        assert_eq!(request.docs, Some("Creates a new user in the system.".into()));
    }

    #[test]
    fn oc_http_request_minimal_yaml() {
        let yaml = "info:\n  name: Simple GET\nhttp:\n  method: GET\n  url: https://example.com";
        let request: OcHttpRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(request.info.name, "Simple GET");
        assert_eq!(request.http.method, "GET");
        assert!(request.runtime.is_none());
        assert!(request.settings.is_none());
        assert!(request.examples.is_none());
    }

    #[test]
    fn oc_graphql_request_yaml() {
        let yaml = r#"
info:
  name: Get Users
  type: graphql
graphql:
  url: "https://api.example.com/graphql"
  body:
    query: "query { users { id name } }"
"#;
        let req: OcGraphQLRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.info.name, "Get Users");
        assert_eq!(req.graphql.url, "https://api.example.com/graphql");
    }

    #[test]
    fn oc_grpc_request_yaml() {
        let yaml = r#"
info:
  name: Get User
  type: grpc
grpc:
  url: "localhost:50051"
  method: "users.UserService/GetUser"
  methodType: unary
  protoFilePath: "./protos/users.proto"
  metadata:
    - name: authorization
      value: "Bearer token"
  message: '{"id": "123"}'
"#;
        let req: OcGrpcRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.info.name, "Get User");
        assert_eq!(req.grpc.method_type, Some("unary".into()));
        assert_eq!(req.grpc.metadata.len(), 1);
    }

    #[test]
    fn oc_websocket_request_yaml() {
        let yaml = r#"
info:
  name: Chat
  type: websocket
websocket:
  url: "wss://chat.example.com/ws"
  headers:
    - name: Authorization
      value: "Bearer token"
  message:
    type: json
    data: '{"action": "subscribe", "channel": "general"}'
"#;
        let req: OcWebSocketRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.info.name, "Chat");
        assert_eq!(req.websocket.url, "wss://chat.example.com/ws");
        assert!(req.websocket.message.is_some());
    }

    #[test]
    fn oc_script_file_yaml() {
        let yaml = "type: script\nscript: ./scripts/auth.js";
        let sf: OcScriptFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(sf.script_file_type, "script");
        assert_eq!(sf.script, "./scripts/auth.js");
    }

    #[test]
    fn oc_runtime_with_auth_yaml() {
        let yaml = "scripts:\n  - type: before-request\n    code: \"let x = 1;\"\nauth:\n  type: bearer\n  token: my-token";
        let runtime: OcHttpRequestRuntime = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(runtime.scripts.len(), 1);
        assert!(runtime.auth.is_some());
    }
}
