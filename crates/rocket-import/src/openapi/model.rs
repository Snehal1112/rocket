//! A small neutral model of an OpenAPI document. All three spec versions
//! (Swagger 2.0, OpenAPI 3.0 and 3.1) are read into these types.

use rocket_shared::types::HttpMethod;

#[derive(Debug, Clone, PartialEq)]
pub struct OpenApiDoc {
    pub title: String,
    pub version: String,
    /// Server base URLs in document order. Server variables are already replaced by their defaults.
    pub servers: Vec<String>,
    pub operations: Vec<Operation>,
    pub security_schemes: Vec<SecurityScheme>,
    /// Things the reader left out, such as remote `$ref` values.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    /// `operationId` when present, else `"{METHOD} {path}"`. Unique within a document.
    pub key: String,
    pub method: HttpMethod,
    pub path: String,
    pub summary: Option<String>,
    pub operation_id: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBodySpec>,
    /// Names of the security schemes this operation uses.
    pub security: Vec<String>,
    pub deprecated: bool,
    /// Status code and description of each documented response, in document order.
    pub responses: Vec<(String, String)>,
    /// Every media type of the request body with its schema, for the spec viewer.
    pub request_media: Vec<MediaSpec>,
    /// True when the request body is marked `required`.
    pub request_body_required: bool,
    /// The media types of each response, by status code, for the spec viewer.
    pub response_media: Vec<(String, Vec<MediaSpec>)>,
}

/// One media type of a body and its schema, with local references already inlined.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaSpec {
    pub content_type: String,
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamLocation {
    Path,
    Query,
    Header,
    Cookie,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub location: ParamLocation,
    pub required: bool,
    pub example: Option<serde_json::Value>,
    pub schema: Option<serde_json::Value>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestBodySpec {
    pub content_type: String,
    pub schema: Option<serde_json::Value>,
    pub example: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SecurityScheme {
    pub name: String,
    pub kind: SecuritySchemeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SecuritySchemeKind {
    Bearer,
    Basic,
    ApiKey { param_name: String, in_query: bool },
    OAuth2,
    Unsupported(String),
}
