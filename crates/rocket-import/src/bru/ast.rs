/// The root AST node produced by parsing a single `.bru` file
/// (request file or environment file).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BruDocument {
    pub meta: Option<BruMeta>,
    pub method: Option<BruMethod>,
    pub url: Option<String>,
    pub headers: Vec<BruKeyValue>,
    pub body: Option<BruBody>,
    pub auth: Option<BruAuth>,
    /// Variables from `vars {}` block (environment files).
    pub vars: Vec<BruKeyValue>,
    /// Variables from `vars:secret {}` block (environment files).
    pub secret_vars: Vec<String>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    /// Unrecognised or unsupported blocks — fed into ImportReport.
    pub unknown_blocks: Vec<BruRawBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BruMeta {
    pub name: String,
    pub request_type: String,  // "http", "graphql", "grpc", "websocket"
    pub seq: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BruKeyValue {
    pub key: String,
    pub value: String,
    pub disabled: bool,  // true when line starts with `~`
}

#[derive(Debug, Clone, PartialEq)]
pub enum BruMethod {
    Get, Post, Put, Patch, Delete, Head, Options,
}

impl BruMethod {
    /// Parse from lowercase block name ("get", "post", …).
    pub fn from_block_name(s: &str) -> Option<Self> {
        match s {
            "get"     => Some(Self::Get),
            "post"    => Some(Self::Post),
            "put"     => Some(Self::Put),
            "patch"   => Some(Self::Patch),
            "delete"  => Some(Self::Delete),
            "head"    => Some(Self::Head),
            "options" => Some(Self::Options),
            _         => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BruBody {
    Json(String),
    Text(String),
    Xml(String),
    FormUrlEncoded(Vec<BruKeyValue>),
    Multipart(Vec<BruKeyValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BruAuth {
    Bearer { token: String },
    Basic { username: String, password: String },
    AwsV4 {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        service: Option<String>,
        region: Option<String>,
        profile_name: Option<String>,
    },
    ApiKey { key: String, value: String, placement: String },
    Digest { username: String, password: String },
    /// Any auth type not listed above — lands in unknown_blocks instead.
    /// The parser never constructs this; it is kept for completeness.
    #[allow(dead_code)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BruRawBlock {
    pub name: String,
    pub subtype: Option<String>,
    pub content: String,
}
