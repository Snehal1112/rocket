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
    /// Messages from a `body:ws` block (WebSocket requests only).
    pub ws_messages: Vec<BruWsMessage>,
    /// The `auth:` mode named inside a `ws {}` block, such as `inherit`, `none` or `bearer`.
    pub ws_auth_mode: Option<String>,
    /// The query and variables of a GraphQL request (`body:graphql` and `body:graphql:vars`).
    pub graphql: Option<BruGraphQl>,
    /// The `grpc {}` block of a gRPC request.
    pub grpc: Option<BruGrpc>,
    /// Entries of the `metadata {}` block of a gRPC request.
    pub grpc_metadata: Vec<BruKeyValue>,
    /// One entry per `body:grpc {}` block, in file order.
    pub grpc_messages: Vec<BruGrpcMessage>,
    /// Unrecognised or unsupported blocks — fed into ImportReport.
    pub unknown_blocks: Vec<BruRawBlock>,
}

impl BruDocument {
    /// True for a Bruno WebSocket request (`meta.type` is `ws` or `websocket`).
    pub fn is_websocket(&self) -> bool {
        self.meta
            .as_ref()
            .is_some_and(|m| matches!(m.request_type.as_str(), "ws" | "websocket"))
    }
}

/// The `grpc {}` block: where to call and which method.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BruGrpc {
    pub url: Option<String>,
    /// `/package.Service/Method` in Bruno files.
    pub method: Option<String>,
    /// `unary`, `client-streaming`, `server-streaming` or `bidi-streaming`.
    pub method_type: Option<String>,
    /// `protoPath` in `.bru` files, `protoFilePath` in OpenCollection YAML.
    pub proto_path: Option<String>,
    /// The `auth:` mode named in the block, such as `none` or `inherit`.
    pub auth_mode: Option<String>,
}

/// One saved gRPC message (`body:grpc {}` block).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BruGrpcMessage {
    pub title: String,
    pub content: String,
}

/// One saved WebSocket message from a `body:ws` block.
#[derive(Debug, Clone, PartialEq)]
pub struct BruWsMessage {
    pub name: String,
    /// Bruno's format tag: `json`, `text` or `xml`. Anything else is passed through.
    pub kind: String,
    pub content: String,
}

/// A GraphQL body: the query text and the optional variables JSON.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BruGraphQl {
    pub query: String,
    pub variables: Option<String>,
    /// Every stored body when the file lists several. `query` and `variables` hold the selected one.
    pub variants: Vec<rocket_collection::GraphQlBodyVariant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BruMeta {
    pub name: String,
    pub request_type: String, // "http", "graphql", "grpc", "websocket"
    pub seq: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BruKeyValue {
    pub key: String,
    pub value: String,
    pub disabled: bool, // true when line starts with `~`
}

#[derive(Debug, Clone, PartialEq)]
pub enum BruMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl BruMethod {
    /// Parse from lowercase block name ("get", "post", …).
    pub fn from_block_name(s: &str) -> Option<Self> {
        match s {
            "get" => Some(Self::Get),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "patch" => Some(Self::Patch),
            "delete" => Some(Self::Delete),
            "head" => Some(Self::Head),
            "options" => Some(Self::Options),
            _ => None,
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
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    AwsV4 {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        service: Option<String>,
        region: Option<String>,
        profile_name: Option<String>,
    },
    ApiKey {
        key: String,
        value: String,
        placement: String,
    },
    Digest {
        username: String,
        password: String,
    },
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
