//! WebSocket client port. Pure types and one trait; the socket code lives in
//! `rocket-infra`. A connection is a pair of channels so a caller can read
//! events and send commands at the same time without sharing a `&mut`.

use std::time::Duration;

use async_trait::async_trait;
use rocket_shared::error::DomainResult;
use tokio::sync::mpsc;

/// Connect and handshake timeout used when a request does not set one.
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 30_000;

/// A fully resolved connect request: variables already substituted and auth
/// already turned into headers or a query parameter.
#[derive(Clone)]
pub struct WebSocketConnectRequest {
    pub url: String,
    /// Header name and value pairs sent on the handshake.
    pub headers: Vec<(String, String)>,
    /// Subprotocols offered in `Sec-WebSocket-Protocol`, in preference order.
    pub subprotocols: Vec<String>,
    /// `None` waits forever.
    pub connect_timeout: Option<Duration>,
    /// `None` sends no pings.
    pub keep_alive_interval: Option<Duration>,
    pub verify_ssl: bool,
}

// Header values commonly hold credentials, and the URL can carry a token in its
// query, so Debug prints header names only and omits the URL.
impl std::fmt::Debug for WebSocketConnectRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("WebSocketConnectRequest")
            .field("headers", &names)
            .field("subprotocols", &self.subprotocols)
            .field("connect_timeout", &self.connect_timeout)
            .field("keep_alive_interval", &self.keep_alive_interval)
            .field("verify_ssl", &self.verify_ssl)
            .finish_non_exhaustive()
    }
}

/// One data frame, in either direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketFrame {
    Text(String),
    Binary(Vec<u8>),
}

impl WebSocketFrame {
    /// Payload size in bytes.
    pub fn size(&self) -> usize {
        match self {
            Self::Text(t) => t.len(),
            Self::Binary(b) => b.len(),
        }
    }
}

/// How a connection ended. `clean` is true when both sides finished the
/// WebSocket close handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketClose {
    pub code: Option<u16>,
    pub reason: String,
    pub clean: bool,
}

/// What a connection reports. A stream of `Frame`s ends with exactly one `Closed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketEvent {
    Frame(WebSocketFrame),
    Closed(WebSocketClose),
}

/// What a caller asks a connection to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketCommand {
    Send(WebSocketFrame),
    Close { code: u16, reason: String },
}

/// A live connection. Dropping every clone of `outbound` closes the socket.
#[derive(Debug)]
pub struct WebSocketHandle {
    /// The subprotocol the server selected, if any.
    pub subprotocol: Option<String>,
    pub outbound: mpsc::Sender<WebSocketCommand>,
    pub events: mpsc::Receiver<WebSocketEvent>,
}

/// Opens WebSocket connections. Implemented by `TungsteniteWebSocketClient`
/// in `rocket-infra`.
#[async_trait]
pub trait WebSocketClient: Send + Sync {
    async fn connect(&self, request: WebSocketConnectRequest) -> DomainResult<WebSocketHandle>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_contains_header_values() {
        let request = WebSocketConnectRequest {
            url: "wss://chat.example.com/ws".into(),
            headers: vec![("Authorization".into(), "Bearer super-secret".into())],
            subprotocols: vec![],
            connect_timeout: None,
            keep_alive_interval: None,
            verify_ssl: true,
        };
        let shown = format!("{request:?}");
        assert!(shown.contains("Authorization"), "{shown}");
        assert!(!shown.contains("super-secret"), "{shown}");
    }

    #[test]
    fn frame_helpers_report_payload_size_in_bytes() {
        assert_eq!(WebSocketFrame::Text("héllo".into()).size(), 6);
        assert_eq!(WebSocketFrame::Binary(vec![1, 2, 3]).size(), 3);
    }

    #[test]
    fn client_trait_is_object_safe() {
        fn _assert(_: std::sync::Arc<dyn WebSocketClient>) {}
    }
}
