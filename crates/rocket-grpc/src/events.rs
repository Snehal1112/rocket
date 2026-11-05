//! Events the gRPC context publishes, and the metadata line type they carry.

use rocket_shared::published_event::PublishedEvent;
use serde::{Deserialize, Serialize};

/// One metadata (header or trailer) line of a gRPC call, as shown to the user.
/// Binary values (`*-bin` names) are carried as base64 text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrpcMetadataPair {
    pub name: String,
    pub value: String,
}

impl GrpcMetadataPair {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// Something happened on a streaming gRPC call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GrpcEvent {
    /// Emitted once a streaming gRPC call has been opened.
    GrpcSessionStarted {
        session_id: String,
        /// `client-streaming`, `server-streaming` or `bidi-streaming`.
        method_type: String,
    },
    /// Emitted when the response headers arrive.
    GrpcSessionHeaders {
        session_id: String,
        headers: Vec<GrpcMetadataPair>,
    },
    /// Emitted for every response message, as protobuf JSON. `index` counts from 0.
    GrpcSessionMessage {
        session_id: String,
        index: u64,
        json: String,
    },
    /// Emitted once when the call ends, for any reason. `code` is the gRPC status
    /// code, and 1 (CANCELLED) when the user cancelled.
    GrpcSessionFinished {
        session_id: String,
        code: i32,
        code_name: String,
        message: String,
        trailers: Vec<GrpcMetadataPair>,
        duration_ms: u64,
    },
}

impl PublishedEvent for GrpcEvent {
    fn channel(&self) -> &'static str {
        match self {
            GrpcEvent::GrpcSessionStarted { .. } => "grpc-session-started",
            GrpcEvent::GrpcSessionHeaders { .. } => "grpc-session-headers",
            GrpcEvent::GrpcSessionMessage { .. } => "grpc-session-message",
            GrpcEvent::GrpcSessionFinished { .. } => "grpc-session-finished",
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::published_event::PublishedEvent;
    use serde_json::json;

    fn pair() -> GrpcMetadataPair {
        GrpcMetadataPair::new("x-a", "b")
    }

    #[test]
    fn grpc_events_json_are_unchanged() {
        let started = GrpcEvent::GrpcSessionStarted {
            session_id: "s1".into(),
            method_type: "bidi-streaming".into(),
        };
        assert_eq!(
            started.to_json().expect("serializes"),
            json!({"type":"grpcSessionStarted","session_id":"s1","method_type":"bidi-streaming"})
        );
        let headers = GrpcEvent::GrpcSessionHeaders {
            session_id: "s1".into(),
            headers: vec![pair()],
        };
        assert_eq!(
            headers.to_json().expect("serializes"),
            json!({"type":"grpcSessionHeaders","session_id":"s1","headers":[{"name":"x-a","value":"b"}]})
        );
        let message = GrpcEvent::GrpcSessionMessage {
            session_id: "s1".into(),
            index: 2,
            json: "{\"a\":1}".into(),
        };
        assert_eq!(
            message.to_json().expect("serializes"),
            json!({"type":"grpcSessionMessage","session_id":"s1","index":2,"json":"{\"a\":1}"})
        );
        let finished = GrpcEvent::GrpcSessionFinished {
            session_id: "s1".into(),
            code: 1,
            code_name: "CANCELLED".into(),
            message: "m".into(),
            trailers: vec![pair()],
            duration_ms: 42,
        };
        assert_eq!(
            finished.to_json().expect("serializes"),
            json!({"type":"grpcSessionFinished","session_id":"s1","code":1,"code_name":"CANCELLED","message":"m","trailers":[{"name":"x-a","value":"b"}],"duration_ms":42})
        );
    }

    #[test]
    fn grpc_events_keep_their_channels() {
        let s = || "s".to_string();
        let events = [
            (
                GrpcEvent::GrpcSessionStarted {
                    session_id: s(),
                    method_type: s(),
                },
                "grpc-session-started",
            ),
            (
                GrpcEvent::GrpcSessionHeaders {
                    session_id: s(),
                    headers: vec![],
                },
                "grpc-session-headers",
            ),
            (
                GrpcEvent::GrpcSessionMessage {
                    session_id: s(),
                    index: 0,
                    json: s(),
                },
                "grpc-session-message",
            ),
            (
                GrpcEvent::GrpcSessionFinished {
                    session_id: s(),
                    code: 0,
                    code_name: s(),
                    message: s(),
                    trailers: vec![],
                    duration_ms: 0,
                },
                "grpc-session-finished",
            ),
        ];
        for (event, channel) in events {
            assert_eq!(event.channel(), channel);
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }

    #[test]
    fn metadata_pair_keeps_its_two_fields() {
        assert_eq!(
            serde_json::to_value(pair()).expect("serializes"),
            json!({"name": "x-a", "value": "b"})
        );
    }
}
