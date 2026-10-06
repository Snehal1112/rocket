use rocket_shared::assertion::Assertion;
use rocket_shared::description::Description;
use rocket_shared::types::Auth;
use serde::{Deserialize, Serialize};

use crate::settings::CollectionVariable;

/// The four gRPC call shapes. The serialized strings match the OpenCollection
/// `methodType` values, so the IPC payload and the YAML file agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum GrpcMethodType {
    #[default]
    #[serde(rename = "unary")]
    Unary,
    #[serde(rename = "client-streaming")]
    ClientStreaming,
    #[serde(rename = "server-streaming")]
    ServerStreaming,
    #[serde(rename = "bidi-streaming")]
    BidiStreaming,
}

impl GrpcMethodType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unary => "unary",
            Self::ClientStreaming => "client-streaming",
            Self::ServerStreaming => "server-streaming",
            Self::BidiStreaming => "bidi-streaming",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "unary" => Some(Self::Unary),
            "client-streaming" => Some(Self::ClientStreaming),
            "server-streaming" => Some(Self::ServerStreaming),
            "bidi-streaming" => Some(Self::BidiStreaming),
            _ => None,
        }
    }

    /// Builds the type from the two streaming flags of a method descriptor.
    pub fn from_streaming_flags(client_streaming: bool, server_streaming: bool) -> Self {
        match (client_streaming, server_streaming) {
            (false, false) => Self::Unary,
            (true, false) => Self::ClientStreaming,
            (false, true) => Self::ServerStreaming,
            (true, true) => Self::BidiStreaming,
        }
    }

    /// True when the client sends more than one message.
    pub fn client_streams(&self) -> bool {
        matches!(self, Self::ClientStreaming | Self::BidiStreaming)
    }

    /// True when the server sends more than one message.
    pub fn server_streams(&self) -> bool {
        matches!(self, Self::ServerStreaming | Self::BidiStreaming)
    }
}

/// One metadata (header) line sent with a gRPC call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcMetadataEntry {
    pub key: String,
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
}

fn default_true() -> bool {
    true
}

impl GrpcMetadataEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            description: None,
        }
    }
}

/// One saved message body. A request with a single untitled message is written
/// as a plain string in the file; every other shape is written as variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcMessage {
    #[serde(default)]
    pub title: String,
    /// The message the editor shows first and a unary or server-streaming call sends.
    #[serde(default)]
    pub selected: bool,
    pub content: String,
}

/// A script stored under `runtime.scripts`. Kept as-is so a file round-trips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcScript {
    #[serde(rename = "type")]
    pub script_type: String,
    pub code: String,
}

/// A saved gRPC request (OpenCollection `GrpcRequest`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcRequest {
    #[serde(default = "crate::generate_uid")]
    pub uid: String,
    pub name: String,
    /// On-disk file name. Filled in when the collection is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    pub url: String,
    /// Full RPC name, `package.Service/Method`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default)]
    pub method_type: GrpcMethodType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proto_file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<GrpcMetadataEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<GrpcMessage>,
    /// The OpenCollection schema keeps gRPC auth in the `grpc` block, not in `runtime`.
    #[serde(default)]
    pub auth: Auth,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<CollectionVariable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<GrpcScript>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<Assertion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

impl GrpcRequest {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            uid: crate::generate_uid(),
            name: name.into(),
            file_name: None,
            seq: None,
            tags: Vec::new(),
            description: None,
            url: url.into(),
            method: None,
            method_type: GrpcMethodType::Unary,
            proto_file_path: None,
            metadata: Vec::new(),
            messages: Vec::new(),
            auth: Auth::None,
            variables: Vec::new(),
            scripts: Vec::new(),
            assertions: Vec::new(),
            docs: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_request_has_unary_defaults() {
        let r = GrpcRequest::new("Say Hello", "localhost:50051");
        assert!(!r.uid.is_empty());
        assert_eq!(r.method_type, GrpcMethodType::Unary);
        assert!(r.messages.is_empty());
        assert_eq!(r.auth, Auth::None);
    }

    #[test]
    fn method_type_uses_the_spec_strings() {
        assert_eq!(
            serde_json::to_string(&GrpcMethodType::BidiStreaming).expect("serialize"),
            "\"bidi-streaming\""
        );
        assert_eq!(
            GrpcMethodType::parse("client-streaming"),
            Some(GrpcMethodType::ClientStreaming)
        );
        assert_eq!(GrpcMethodType::parse("nope"), None);
        assert_eq!(GrpcMethodType::ServerStreaming.as_str(), "server-streaming");
    }

    #[test]
    fn streaming_flags_map_to_all_four_types() {
        assert_eq!(
            GrpcMethodType::from_streaming_flags(false, false),
            GrpcMethodType::Unary
        );
        assert_eq!(
            GrpcMethodType::from_streaming_flags(true, false),
            GrpcMethodType::ClientStreaming
        );
        assert_eq!(
            GrpcMethodType::from_streaming_flags(false, true),
            GrpcMethodType::ServerStreaming
        );
        assert_eq!(
            GrpcMethodType::from_streaming_flags(true, true),
            GrpcMethodType::BidiStreaming
        );
        assert!(GrpcMethodType::BidiStreaming.client_streams());
        assert!(GrpcMethodType::BidiStreaming.server_streams());
        assert!(!GrpcMethodType::Unary.client_streams());
        assert!(!GrpcMethodType::ClientStreaming.server_streams());
    }

    #[test]
    fn json_shape_is_camel_case_and_skips_empty_fields() {
        let mut r = GrpcRequest::new("Say Hello", "localhost:50051");
        r.method = Some("demo.Greeter/SayHello".into());
        r.method_type = GrpcMethodType::ServerStreaming;
        r.proto_file_path = Some("protos/greeter.proto".into());
        let v = serde_json::to_value(&r).expect("serialize");
        assert_eq!(v["methodType"], "server-streaming");
        assert_eq!(v["protoFilePath"], "protos/greeter.proto");
        assert!(v.get("metadata").is_none(), "empty lists are skipped: {v}");
    }

    #[test]
    fn minimal_payload_deserializes_with_defaults() {
        let r: GrpcRequest =
            serde_json::from_str(r#"{"name":"A","url":"h:1"}"#).expect("minimal payload");
        assert!(!r.uid.is_empty());
        assert_eq!(r.method_type, GrpcMethodType::Unary);
        assert!(r.metadata.is_empty());
    }

    #[test]
    fn metadata_entry_defaults_to_enabled() {
        let e: GrpcMetadataEntry =
            serde_json::from_str(r#"{"key":"k","value":"v"}"#).expect("entry");
        assert!(e.enabled);
    }
}
