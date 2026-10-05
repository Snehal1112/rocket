//! `tokio-tungstenite` implementation of the `WebSocketClient` port.
//!
//! `connect` performs the handshake and then spawns a pump task that owns the socket. The caller
//! talks to the pump through the channels in `WebSocketHandle`. Errors name header names, never
//! values, and never include the URL.

use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rocket_http::websocket::{
    WebSocketClient, WebSocketClose, WebSocketCommand, WebSocketConnectRequest, WebSocketEvent,
    WebSocketFrame, WebSocketHandle,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{connect_async_tls_with_config, Connector};

/// How long to wait for the server's close reply after sending our close frame.
const CLOSE_REPLY_TIMEOUT: Duration = Duration::from_secs(3);
const COMMAND_BUFFER: usize = 64;
const EVENT_BUFFER: usize = 256;

/// Headers the handshake itself owns. A caller cannot override them.
const RESERVED_HEADERS: [&str; 6] = [
    "host",
    "connection",
    "upgrade",
    "sec-websocket-key",
    "sec-websocket-version",
    "sec-websocket-extensions",
];

#[derive(Debug, Default, Clone, Copy)]
pub struct TungsteniteWebSocketClient;

impl TungsteniteWebSocketClient {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WebSocketClient for TungsteniteWebSocketClient {
    async fn connect(&self, request: WebSocketConnectRequest) -> DomainResult<WebSocketHandle> {
        let client_request = build_client_request(&request)?;
        let connector = tls_connector(request.verify_ssl)?;
        let attempt = connect_async_tls_with_config(client_request, None, false, connector);

        let result = match request.connect_timeout {
            Some(limit) => tokio::time::timeout(limit, attempt)
                .await
                .map_err(|_| DomainError::Http("WebSocket connection timed out".into()))?,
            None => attempt.await,
        };
        let (stream, response) = result.map_err(|e| DomainError::Http(describe_error(&e)))?;

        let subprotocol = response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        let (command_tx, command_rx) = mpsc::channel(COMMAND_BUFFER);
        let (event_tx, event_rx) = mpsc::channel(EVENT_BUFFER);
        tokio::spawn(pump(
            stream,
            command_rx,
            event_tx,
            request.keep_alive_interval,
        ));

        Ok(WebSocketHandle {
            subprotocol,
            outbound: command_tx,
            events: event_rx,
        })
    }
}

/// Builds the handshake request: URL, caller headers and the subprotocol offer.
fn build_client_request(
    request: &WebSocketConnectRequest,
) -> DomainResult<tokio_tungstenite::tungstenite::http::Request<()>> {
    let scheme_ok = request.url.starts_with("ws://") || request.url.starts_with("wss://");
    if !scheme_ok {
        return Err(DomainError::InvalidInput(
            "WebSocket URL must start with ws:// or wss://".into(),
        ));
    }
    let mut built = request
        .url
        .as_str()
        .into_client_request()
        .map_err(|e| DomainError::InvalidInput(format!("invalid WebSocket URL: {e}")))?;

    let mut subprotocols: Vec<String> = Vec::new();
    for offered in &request.subprotocols {
        push_unique(&mut subprotocols, offered.trim());
    }
    for (name, value) in &request.headers {
        let lower = name.trim().to_ascii_lowercase();
        if lower == "sec-websocket-protocol" {
            for offered in value.split(',') {
                push_unique(&mut subprotocols, offered.trim());
            }
            continue;
        }
        if RESERVED_HEADERS.contains(&lower.as_str()) {
            return Err(DomainError::InvalidInput(format!(
                "header '{name}' is managed by the WebSocket handshake and cannot be set"
            )));
        }
        let header_name = HeaderName::from_bytes(lower.as_bytes())
            .map_err(|_| DomainError::InvalidInput(format!("invalid header name '{name}'")))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|_| DomainError::InvalidInput(format!("invalid value for header '{name}'")))?;
        built.headers_mut().append(header_name, header_value);
    }
    if !subprotocols.is_empty() {
        let joined = HeaderValue::from_str(&subprotocols.join(", "))
            .map_err(|_| DomainError::InvalidInput("invalid WebSocket subprotocol".into()))?;
        built.headers_mut().insert("sec-websocket-protocol", joined);
    }
    Ok(built)
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !list.iter().any(|existing| existing == value) {
        list.push(value.to_string());
    }
}

/// The default connector verifies certificates against the OS store, exactly like reqwest.
/// Only an explicit `verify_ssl: false` builds the permissive one.
fn tls_connector(verify_ssl: bool) -> DomainResult<Option<Connector>> {
    if verify_ssl {
        return Ok(None);
    }
    let connector = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .map_err(|e| DomainError::Internal(format!("TLS setup failed: {e}")))?;
    Ok(Some(Connector::NativeTls(connector)))
}

/// A user-facing message that never includes header values or the request URL.
fn describe_error(error: &WsError) -> String {
    match error {
        WsError::Http(response) => {
            format!(
                "handshake rejected with HTTP {}",
                response.status().as_u16()
            )
        }
        WsError::Tls(e) => format!("TLS handshake failed: {e}"),
        WsError::Io(e) => format!("connection failed: {e}"),
        WsError::Url(e) => format!("invalid WebSocket URL: {e}"),
        other => format!("WebSocket error: {other}"),
    }
}

fn to_message(frame: WebSocketFrame) -> Message {
    match frame {
        WebSocketFrame::Text(text) => Message::text(text),
        WebSocketFrame::Binary(bytes) => Message::binary(bytes),
    }
}

fn unclean(reason: String) -> WebSocketClose {
    WebSocketClose {
        code: None,
        reason,
        clean: false,
    }
}

/// Owns the socket. Sends exactly one `Closed` event, then returns.
async fn pump(
    mut stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut commands: mpsc::Receiver<WebSocketCommand>,
    events: mpsc::Sender<WebSocketEvent>,
    keep_alive: Option<Duration>,
) {
    let mut ticker = keep_alive.filter(|d| !d.is_zero()).map(|period| {
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval
    });
    let mut closing_deadline: Option<tokio::time::Instant> = None;
    let mut commands_open = true;

    let outcome: WebSocketClose = loop {
        tokio::select! {
            command = commands.recv(), if commands_open => match command {
                Some(WebSocketCommand::Send(frame)) => {
                    if let Err(e) = stream.send(to_message(frame)).await {
                        break unclean(describe_error(&e));
                    }
                }
                Some(WebSocketCommand::Close { code, reason }) => {
                    let frame = CloseFrame { code: CloseCode::from(code), reason: reason.into() };
                    if let Err(e) = stream.close(Some(frame)).await {
                        break unclean(describe_error(&e));
                    }
                    closing_deadline = Some(tokio::time::Instant::now() + CLOSE_REPLY_TIMEOUT);
                    commands_open = false;
                }
                None => {
                    // Every sender is gone: close politely and wait for the reply.
                    let frame = CloseFrame { code: CloseCode::Normal, reason: "".into() };
                    let _ = stream.close(Some(frame)).await;
                    closing_deadline = Some(tokio::time::Instant::now() + CLOSE_REPLY_TIMEOUT);
                    commands_open = false;
                }
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let event = WebSocketEvent::Frame(WebSocketFrame::Text(text.as_str().to_string()));
                    if events.send(event).await.is_err() {
                        break unclean("listener dropped".into());
                    }
                }
                Some(Ok(Message::Binary(bytes))) => {
                    let event = WebSocketEvent::Frame(WebSocketFrame::Binary(bytes.to_vec()));
                    if events.send(event).await.is_err() {
                        break unclean("listener dropped".into());
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    // When the peer started the close, the library only queued our reply. Flush
                    // it so the peer sees a close frame instead of a dropped connection.
                    if closing_deadline.is_none() {
                        let _ = tokio::time::timeout(CLOSE_REPLY_TIMEOUT, stream.flush()).await;
                    }
                    break WebSocketClose {
                        code: frame.as_ref().map(|f| u16::from(f.code)),
                        reason: frame.map(|f| f.reason.as_str().to_string()).unwrap_or_default(),
                        clean: true,
                    };
                }
                // Pings are answered by the library and pongs need no handling.
                Some(Ok(_)) => {}
                Some(Err(WsError::ConnectionClosed)) => {
                    break WebSocketClose { code: None, reason: String::new(), clean: closing_deadline.is_some() };
                }
                Some(Err(e)) => {
                    break unclean(describe_error(&e));
                }
                None => {
                    break WebSocketClose {
                        code: None,
                        reason: "connection ended".into(),
                        clean: closing_deadline.is_some(),
                    };
                }
            },
            _ = async {
                match ticker.as_mut() {
                    Some(t) => { t.tick().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                if let Err(e) = stream.send(Message::Ping(Vec::<u8>::new().into())).await {
                    break unclean(describe_error(&e));
                }
            }
            _ = async {
                match closing_deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                break unclean("close reply timed out".into());
            }
        }
    };
    // The receiver may already be gone; then there is nobody left to tell.
    let _ = events.send(WebSocketEvent::Closed(outcome)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    use tokio_tungstenite::tungstenite::http::StatusCode;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::Message;

    #[derive(Default)]
    struct ServerState {
        handshake_headers: Mutex<Vec<(String, String)>>,
        pings: AtomicUsize,
    }

    #[derive(Clone, Copy)]
    enum Mode {
        Echo,
        RejectUnauthorized,
    }

    /// Local server. In `Echo` mode it echoes text and binary frames, offers back the first
    /// subprotocol it was sent, closes with code 4001 when it receives the text `close-me`,
    /// and counts pings. In `RejectUnauthorized` mode it answers the handshake with 401.
    async fn spawn_server(mode: Mode) -> (u16, Arc<ServerState>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let state = Arc::new(ServerState::default());
        let shared = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    return;
                };
                let state = Arc::clone(&shared);
                tokio::spawn(async move {
                    let for_callback = Arc::clone(&state);
                    let callback = move |req: &Request,
                                         mut resp: Response|
                          -> Result<Response, ErrorResponse> {
                        for (k, v) in req.headers() {
                            for_callback.handshake_headers.lock().expect("lock").push((
                                k.as_str().to_string(),
                                v.to_str().unwrap_or("").to_string(),
                            ));
                        }
                        if matches!(mode, Mode::RejectUnauthorized) {
                            let mut rejection = ErrorResponse::new(Some("denied".to_string()));
                            *rejection.status_mut() = StatusCode::UNAUTHORIZED;
                            return Err(rejection);
                        }
                        let offered = req
                            .headers()
                            .get("sec-websocket-protocol")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.split(',').next())
                            .map(str::trim)
                            .filter(|s| !s.is_empty());
                        if let Some(first) = offered {
                            if let Ok(value) = first.parse() {
                                resp.headers_mut().insert("sec-websocket-protocol", value);
                            }
                        }
                        Ok(resp)
                    };
                    let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await
                    else {
                        return;
                    };
                    while let Some(Ok(msg)) = ws.next().await {
                        match msg {
                            Message::Text(t) if t.as_str() == "close-me" => {
                                let _ = ws
                                    .close(Some(CloseFrame {
                                        code: CloseCode::from(4001),
                                        reason: "bye".into(),
                                    }))
                                    .await;
                            }
                            Message::Text(_) | Message::Binary(_) => {
                                if ws.send(msg).await.is_err() {
                                    break;
                                }
                            }
                            Message::Ping(_) => {
                                state.pings.fetch_add(1, Ordering::SeqCst);
                            }
                            _ => {}
                        }
                    }
                });
            }
        });
        (port, state)
    }

    fn request_for(port: u16) -> WebSocketConnectRequest {
        WebSocketConnectRequest {
            url: format!("ws://127.0.0.1:{port}/socket"),
            headers: Vec::new(),
            subprotocols: Vec::new(),
            connect_timeout: Some(Duration::from_secs(5)),
            keep_alive_interval: None,
            verify_ssl: true,
        }
    }

    async fn next_event(handle: &mut WebSocketHandle) -> WebSocketEvent {
        tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
            .await
            .expect("an event within 5s")
            .expect("event channel still open")
    }

    #[tokio::test]
    async fn text_and_binary_frames_round_trip_through_an_echo_server() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let mut handle = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");

        handle
            .outbound
            .send(WebSocketCommand::Send(WebSocketFrame::Text("hello".into())))
            .await
            .expect("send text");
        assert_eq!(
            next_event(&mut handle).await,
            WebSocketEvent::Frame(WebSocketFrame::Text("hello".into()))
        );

        handle
            .outbound
            .send(WebSocketCommand::Send(WebSocketFrame::Binary(vec![
                1, 2, 3,
            ])))
            .await
            .expect("send binary");
        assert_eq!(
            next_event(&mut handle).await,
            WebSocketEvent::Frame(WebSocketFrame::Binary(vec![1, 2, 3]))
        );
    }

    #[tokio::test]
    async fn headers_are_sent_on_the_handshake() {
        let (port, state) = spawn_server(Mode::Echo).await;
        let mut request = request_for(port);
        request.headers = vec![
            ("Authorization".into(), "Bearer t0k".into()),
            ("X-Trace".into(), "abc".into()),
        ];
        let _handle = TungsteniteWebSocketClient::new()
            .connect(request)
            .await
            .expect("connect");

        let seen = state.handshake_headers.lock().expect("lock").clone();
        assert!(
            seen.contains(&("authorization".to_string(), "Bearer t0k".to_string())),
            "{seen:?}"
        );
        assert!(
            seen.contains(&("x-trace".to_string(), "abc".to_string())),
            "{seen:?}"
        );
    }

    #[tokio::test]
    async fn the_negotiated_subprotocol_is_reported() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let mut request = request_for(port);
        request.subprotocols = vec!["graphql-transport-ws".into(), "graphql-ws".into()];
        let handle = TungsteniteWebSocketClient::new()
            .connect(request)
            .await
            .expect("connect");
        assert_eq!(handle.subprotocol.as_deref(), Some("graphql-transport-ws"));
    }

    #[tokio::test]
    async fn a_server_close_yields_one_closed_event_with_its_code_then_ends() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let mut handle = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");
        handle
            .outbound
            .send(WebSocketCommand::Send(WebSocketFrame::Text(
                "close-me".into(),
            )))
            .await
            .expect("send");

        match next_event(&mut handle).await {
            WebSocketEvent::Closed(close) => {
                assert_eq!(close.code, Some(4001));
                assert_eq!(close.reason, "bye");
                assert!(close.clean);
            }
            other => panic!("expected Closed, got {other:?}"),
        }
        let after = tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
            .await
            .expect("channel ends");
        assert!(after.is_none(), "no event may follow Closed, got {after:?}");
    }

    #[tokio::test]
    async fn a_peer_close_is_answered_with_a_close_frame() {
        use tokio_tungstenite::tungstenite::error::ProtocolError;

        // A server that starts the close, then reports whether the client replied with a close
        // frame (clean) or just dropped the TCP connection (reset without closing handshake).
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (verdict_tx, verdict_rx) = tokio::sync::oneshot::channel::<bool>();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept");
            let mut ws = tokio_tungstenite::accept_async(tcp)
                .await
                .expect("handshake");
            let _ = ws
                .close(Some(CloseFrame {
                    code: CloseCode::Normal,
                    reason: "bye".into(),
                }))
                .await;
            let mut clean = true;
            while let Some(item) = ws.next().await {
                if let Err(WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake)) = item {
                    clean = false;
                    break;
                }
                if item.is_err() {
                    break;
                }
            }
            let _ = verdict_tx.send(clean);
        });

        let mut handle = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");
        match next_event(&mut handle).await {
            WebSocketEvent::Closed(close) => assert!(close.clean, "{close:?}"),
            other => panic!("expected Closed, got {other:?}"),
        }
        let clean = tokio::time::timeout(Duration::from_secs(5), verdict_rx)
            .await
            .expect("the server finishes")
            .expect("verdict");
        assert!(
            clean,
            "the client must echo the close frame before dropping the socket"
        );
    }

    #[tokio::test]
    async fn a_client_close_command_ends_with_a_clean_close() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let mut handle = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");
        handle
            .outbound
            .send(WebSocketCommand::Close {
                code: 1000,
                reason: "done".into(),
            })
            .await
            .expect("close");

        match next_event(&mut handle).await {
            WebSocketEvent::Closed(close) => assert!(close.clean, "{close:?}"),
            other => panic!("expected Closed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dropping_the_outbound_sender_closes_the_socket() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let WebSocketHandle {
            outbound,
            mut events,
            ..
        } = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");
        drop(outbound);

        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("the pump must finish once every sender is gone")
            .expect("one Closed event");
        assert!(matches!(event, WebSocketEvent::Closed(_)), "{event:?}");
    }

    #[tokio::test]
    async fn keep_alive_pings_reach_the_server() {
        let (port, state) = spawn_server(Mode::Echo).await;
        let mut request = request_for(port);
        request.keep_alive_interval = Some(Duration::from_millis(50));
        let _handle = TungsteniteWebSocketClient::new()
            .connect(request)
            .await
            .expect("connect");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while state.pings.load(Ordering::SeqCst) < 2 {
            assert!(
                tokio::time::Instant::now() < deadline,
                "fewer than 2 pings in 5s"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_rejected_handshake_reports_the_status() {
        let (port, _state) = spawn_server(Mode::RejectUnauthorized).await;
        let err = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect_err("401 must fail the connect");
        assert!(err.to_string().contains("401"), "{err}");
    }

    #[tokio::test]
    async fn connection_errors_never_contain_header_values() {
        // A rejected handshake.
        let (port, _state) = spawn_server(Mode::RejectUnauthorized).await;
        let mut request = request_for(port);
        request.headers = vec![("Authorization".into(), "Bearer super-secret".into())];
        request.url = format!("{}?token=query-secret", request.url);
        let err = TungsteniteWebSocketClient::new()
            .connect(request)
            .await
            .expect_err("rejected");
        let text = err.to_string();
        assert!(!text.contains("super-secret"), "{text}");
        assert!(!text.contains("query-secret"), "{text}");

        // A refused connection: bind a port, then free it.
        let probe = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let dead_port = probe.local_addr().expect("addr").port();
        drop(probe);
        let mut refused = request_for(dead_port);
        refused.headers = vec![("Authorization".into(), "Bearer super-secret".into())];
        refused.url = format!("{}?token=query-secret", refused.url);
        let err = TungsteniteWebSocketClient::new()
            .connect(refused)
            .await
            .expect_err("refused");
        let text = err.to_string();
        assert!(!text.contains("super-secret"), "{text}");
        assert!(!text.contains("query-secret"), "{text}");
    }

    #[tokio::test]
    async fn a_silent_server_hits_the_connect_timeout() {
        // The kernel completes the TCP connect from its backlog, but nothing ever answers
        // the handshake.
        let silent = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = silent.local_addr().expect("addr").port();
        let mut request = request_for(port);
        request.connect_timeout = Some(Duration::from_millis(200));
        let err = TungsteniteWebSocketClient::new()
            .connect(request)
            .await
            .expect_err("must time out");
        assert!(err.to_string().contains("timed out"), "{err}");
        drop(silent);
    }

    #[test]
    fn reserved_handshake_headers_are_rejected_by_name() {
        for name in [
            "Host",
            "Upgrade",
            "Connection",
            "Sec-WebSocket-Key",
            "sec-websocket-version",
        ] {
            let mut request = request_for(1);
            request.headers = vec![(name.into(), "x".into())];
            let err = build_client_request(&request).expect_err("reserved header");
            assert!(err.to_string().contains(name), "{err}");
        }
    }

    #[test]
    fn a_protocol_header_is_merged_into_the_subprotocol_offer() {
        let mut request = request_for(1);
        request.subprotocols = vec!["a".into()];
        request.headers = vec![("Sec-WebSocket-Protocol".into(), "b, a".into())];
        let built = build_client_request(&request).expect("build");
        assert_eq!(
            built
                .headers()
                .get("sec-websocket-protocol")
                .and_then(|v| v.to_str().ok()),
            Some("a, b")
        );
    }

    #[test]
    fn a_non_websocket_scheme_is_rejected_before_any_network_use() {
        let mut request = request_for(1);
        request.url = "https://example.com/socket".into();
        let err = build_client_request(&request).expect_err("https is not a websocket scheme");
        assert!(err.to_string().contains("ws://"), "{err}");
    }

    #[test]
    fn the_tls_connector_is_only_replaced_when_verification_is_off() {
        assert!(tls_connector(true).expect("default").is_none());
        assert!(tls_connector(false).expect("permissive").is_some());
    }
}
