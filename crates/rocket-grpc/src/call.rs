use std::time::Duration;

use async_trait::async_trait;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::grpc::GrpcMetadataPair;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::registry::ProtoRegistry;

/// Everything the transport needs for one call. Variables are already resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct GrpcCall {
    /// `host:port`, `grpc://host:port`, `grpcs://host:port`, `http://` or `https://`.
    pub url: String,
    /// `package.Service/Method`.
    pub full_method: String,
    pub metadata: Vec<GrpcMetadataPair>,
    /// Deadline for the whole call. `None` means no deadline.
    pub timeout: Option<Duration>,
    /// Extra PEM CA certificate to trust for `grpcs://`. The system roots are always trusted.
    pub tls_ca_pem: Option<String>,
}

/// A gRPC status. `code` follows the canonical numbering, 0 is OK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcStatus {
    pub code: i32,
    pub code_name: String,
    pub message: String,
}

impl GrpcStatus {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            code_name: grpc_code_name(code).to_string(),
            message: message.into(),
        }
    }

    pub fn ok() -> Self {
        Self::new(0, "")
    }

    pub fn is_ok(&self) -> bool {
        self.code == 0
    }
}

/// The canonical name of a gRPC status code, `UNKNOWN` for numbers outside 0..=16.
pub fn grpc_code_name(code: i32) -> &'static str {
    match code {
        0 => "OK",
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => "UNKNOWN",
    }
}

/// The outcome of a unary call. A non-OK `status` is a normal outcome and is
/// returned here, not as an error, so the UI can show it with its trailers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcUnaryResponse {
    pub headers: Vec<GrpcMetadataPair>,
    pub trailers: Vec<GrpcMetadataPair>,
    pub message_json: Option<String>,
    pub status: GrpcStatus,
    pub duration_ms: u64,
}

/// What a running stream reports back, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum GrpcStreamEvent {
    Headers(Vec<GrpcMetadataPair>),
    Message(String),
    /// Always the last event of a stream that ends by itself.
    Finished {
        status: GrpcStatus,
        trailers: Vec<GrpcMetadataPair>,
    },
}

/// A running streaming call.
pub struct GrpcStreamHandle {
    /// JSON messages to send. `None` for a server-streaming call. Dropping the
    /// sender ends the request side of the call (half-close).
    pub outbound: Option<mpsc::Sender<String>>,
    pub events: mpsc::Receiver<GrpcStreamEvent>,
    /// Aborting the task cancels the call.
    pub abort: AbortHandle,
}

/// Runs gRPC calls. Implemented by `TonicGrpcExecutor` in `rocket-infra`.
#[async_trait]
pub trait GrpcExecutor: Send + Sync {
    /// Runs a unary call. Fails before connecting when the method is not unary
    /// or `request_json` does not fit the request message.
    async fn unary(
        &self,
        call: &GrpcCall,
        registry: &ProtoRegistry,
        request_json: &str,
    ) -> DomainResult<GrpcUnaryResponse>;

    /// Starts a client-streaming, server-streaming or bidirectional call.
    /// `initial_json` is the one request of a server-streaming call, and an
    /// optional first message of the other two.
    async fn open_stream(
        &self,
        call: &GrpcCall,
        registry: &ProtoRegistry,
        initial_json: Option<String>,
    ) -> DomainResult<GrpcStreamHandle>;

    /// Reads the descriptors a live server publishes through server reflection.
    async fn reflect(&self, _call: &GrpcCall) -> DomainResult<ProtoRegistry> {
        Err(DomainError::InvalidInput(
            "this executor does not support server reflection".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_names_cover_the_canonical_range() {
        assert_eq!(grpc_code_name(0), "OK");
        assert_eq!(grpc_code_name(4), "DEADLINE_EXCEEDED");
        assert_eq!(grpc_code_name(5), "NOT_FOUND");
        assert_eq!(grpc_code_name(16), "UNAUTHENTICATED");
        assert_eq!(grpc_code_name(99), "UNKNOWN");
        assert_eq!(grpc_code_name(-1), "UNKNOWN");
    }

    #[test]
    fn status_new_fills_the_name_and_ok_is_ok() {
        let s = GrpcStatus::new(5, "no such user");
        assert_eq!(s.code_name, "NOT_FOUND");
        assert!(!s.is_ok());
        assert!(GrpcStatus::ok().is_ok());
    }

    #[test]
    fn executor_trait_is_object_safe() {
        fn _assert(_: Box<dyn GrpcExecutor>) {}
    }
}
