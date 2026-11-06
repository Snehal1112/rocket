//! The peer WebSocket. Both sides send `Hello` first, then a pump task moves `PeerMessage`s
//! between the socket and two channels. Frames are text JSON of at most `MAX_FRAME_BYTES`.
//! Anything else closes the socket.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use rocket_network::limits::MAX_FRAME_BYTES;
use rocket_network::{DeviceId, PeerCallError, PeerHandler, PeerMessage, PeerSocket};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::client_async_with_config;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Error as WsError, Message as WsMessage};

use crate::network::client::{tls_connect, REQUEST_TIMEOUT};
use crate::network::server::{text, ConnInfo, ConnSlot, RouteState};
use crate::network::TlsIdentity;

/// Each side must send its Hello within this time.
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a closing side waits to send its close frame.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
/// How long one frame may take to leave. A peer that reads nothing is dropped after this.
/// The tests use a shorter time.
const SEND_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_secs(1)
} else {
    Duration::from_secs(10)
};
/// Messages buffered in each direction.
const CHANNEL_CAPACITY: usize = 64;

/// What the pump needs to know about a frame.
pub(crate) enum FrameKind {
    Text(String),
    /// WebSocket ping or pong. The library answers pings itself.
    Control,
    /// Binary, close or raw frames. All of them end the socket.
    Other,
}

/// The two WebSocket libraries have different message types. This is the small common shape
/// the Hello exchange and the pump need, so their rules exist once.
pub(crate) trait WsFrame: Send + Sized + 'static {
    fn kind(self) -> FrameKind;
    fn text(body: String) -> Self;
    fn close() -> Self;
}

impl WsFrame for AxumMessage {
    fn kind(self) -> FrameKind {
        match self {
            AxumMessage::Text(body) => FrameKind::Text(body.as_str().to_owned()),
            AxumMessage::Ping(_) | AxumMessage::Pong(_) => FrameKind::Control,
            AxumMessage::Binary(_) | AxumMessage::Close(_) => FrameKind::Other,
        }
    }

    fn text(body: String) -> Self {
        AxumMessage::Text(body.into())
    }

    fn close() -> Self {
        AxumMessage::Close(None)
    }
}

impl WsFrame for WsMessage {
    fn kind(self) -> FrameKind {
        match self {
            WsMessage::Text(body) => FrameKind::Text(body.as_str().to_owned()),
            WsMessage::Ping(_) | WsMessage::Pong(_) => FrameKind::Control,
            WsMessage::Binary(_) | WsMessage::Close(_) | WsMessage::Frame(_) => FrameKind::Other,
        }
    }

    fn text(body: String) -> Self {
        WsMessage::text(body)
    }

    fn close() -> Self {
        WsMessage::Close(None)
    }
}

/// `GET /peer`. Only a paired, unblocked key may upgrade. Everyone else gets `403 not-paired`
/// before any upgrade header is checked, so a stranger learns nothing about the socket.
pub(crate) async fn peer_upgrade(
    State(state): State<RouteState>,
    Extension(conn): Extension<ConnInfo>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    if !state.handler.is_paired(&conn.peer.device_id).await {
        return text(StatusCode::FORBIDDEN, "not-paired");
    }
    let upgrade = match upgrade {
        Ok(upgrade) => upgrade,
        Err(rejection) => return rejection.into_response(),
    };
    let handler = Arc::clone(&state.handler);
    upgrade
        .max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| serve_peer_socket(socket, handler, conn))
}

async fn serve_peer_socket(mut socket: WebSocket, handler: Arc<dyn PeerHandler>, conn: ConnInfo) {
    let local = handler.local_hello();
    let remote_hello = match exchange_hello(&mut socket, &local).await {
        Ok(hello) => hello,
        Err(_) => {
            close_quietly::<_, AxumMessage>(&mut socket).await;
            return;
        }
    };
    // The pump holds the connection permits for as long as the socket lives.
    let (outbound, inbound) = spawn_pump(
        socket,
        Some(conn.stop.clone()),
        Some(Arc::clone(&conn.slot)),
    );
    handler
        .on_peer_socket(PeerSocket {
            peer: conn.peer.clone(),
            dialed_by: conn.peer.device_id,
            remote_hello,
            outbound,
            inbound,
        })
        .await;
}

/// Sends our Hello, then waits for the peer's. The first data frame must be a valid Hello.
pub(crate) async fn exchange_hello<W, M, E>(
    ws: &mut W,
    local: &PeerMessage,
) -> Result<PeerMessage, String>
where
    W: Stream<Item = Result<M, E>> + Sink<M> + Unpin,
    M: WsFrame,
{
    ws.send(M::text(local.encode()))
        .await
        .map_err(|_| "the hello could not be sent".to_string())?;
    let first = tokio::time::timeout(HELLO_TIMEOUT, first_text(ws))
        .await
        .map_err(|_| "no hello in time".to_string())?
        .ok_or_else(|| "the socket closed before a hello".to_string())?;
    match PeerMessage::decode(&first) {
        Ok(hello @ PeerMessage::Hello { .. }) => Ok(hello),
        _ => Err("the first frame is not a hello".to_string()),
    }
}

/// The first text frame. Ping and pong are skipped; anything else ends the wait.
async fn first_text<W, M, E>(ws: &mut W) -> Option<String>
where
    W: Stream<Item = Result<M, E>> + Unpin,
    M: WsFrame,
{
    loop {
        match ws.next().await? {
            Ok(frame) => match frame.kind() {
                FrameKind::Text(body) => return Some(body),
                FrameKind::Control => continue,
                FrameKind::Other => return None,
            },
            Err(_) => return None,
        }
    }
}

async fn close_quietly<W, M>(ws: &mut W)
where
    W: Sink<M> + Unpin,
    M: WsFrame,
{
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, async {
        let _ = ws.send(M::close()).await;
        let _ = ws.close().await;
    })
    .await;
}

/// Starts the pump task and returns the two channel ends for the `PeerSocket`.
pub(crate) fn spawn_pump<W, M, E>(
    ws: W,
    stop: Option<watch::Receiver<()>>,
    slot: Option<Arc<ConnSlot>>,
) -> (mpsc::Sender<PeerMessage>, mpsc::Receiver<PeerMessage>)
where
    W: Stream<Item = Result<M, E>> + Sink<M> + Send + Unpin + 'static,
    M: WsFrame,
    E: Send + 'static,
{
    let (outbound, outbound_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let (inbound_tx, inbound) = mpsc::channel(CHANNEL_CAPACITY);
    tokio::spawn(pump(ws, outbound_rx, inbound_tx, stop, slot));
    (outbound, inbound)
}

async fn pump<W, M, E>(
    ws: W,
    mut outbound: mpsc::Receiver<PeerMessage>,
    inbound: mpsc::Sender<PeerMessage>,
    mut stop: Option<watch::Receiver<()>>,
    _slot: Option<Arc<ConnSlot>>,
) where
    W: Stream<Item = Result<M, E>> + Sink<M> + Send + Unpin + 'static,
    M: WsFrame,
    E: Send + 'static,
{
    let (mut sink, mut stream) = ws.split();
    loop {
        tokio::select! {
            _ = stopped(&mut stop) => break,
            frame = stream.next() => {
                // A read error covers frames over the size limit.
                let Some(Ok(frame)) = frame else { break };
                match frame.kind() {
                    FrameKind::Text(body) => match PeerMessage::decode(&body) {
                        // Hello is only the first frame. A second one is a protocol error.
                        Ok(PeerMessage::Hello { .. }) | Err(_) => break,
                        Ok(message) => {
                            // A slow consumer must not hold back the stop signal.
                            let delivered = tokio::select! {
                                result = inbound.send(message) => result.is_ok(),
                                _ = stopped(&mut stop) => false,
                            };
                            if !delivered {
                                break;
                            }
                        }
                    },
                    FrameKind::Control => {}
                    FrameKind::Other => break,
                }
            }
            message = outbound.recv() => {
                let Some(message) = message else { break };
                // A peer that stops reading must not pin this socket or its slot: the send
                // gives way to the stop signal and to a timeout.
                let sent = tokio::select! {
                    result = tokio::time::timeout(SEND_TIMEOUT, sink.send(M::text(message.encode()))) => {
                        matches!(result, Ok(Ok(())))
                    }
                    _ = stopped(&mut stop) => false,
                };
                if !sent {
                    break;
                }
            }
        }
    }
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, async {
        let _ = sink.send(M::close()).await;
        let _ = sink.close().await;
    })
    .await;
    // Returning drops `inbound`, `outbound` and `_slot`: the handler sees `None` and the
    // connection permits are free again.
}

/// Resolves when the server stops. A client socket has no server, so it never resolves.
async fn stopped(stop: &mut Option<watch::Receiver<()>>) {
    match stop {
        // An error means the server dropped its sender, which is also a stop.
        Some(stop) => {
            let _ = stop.changed().await;
        }
        None => std::future::pending::<()>().await,
    }
}

/// Dials `wss://addr/peer` with our certificate, pinned to `expected`, then exchanges Hello.
pub(crate) async fn open_client_socket(
    identity: &TlsIdentity,
    addr: SocketAddr,
    expected: &DeviceId,
    hello: PeerMessage,
) -> Result<PeerSocket, PeerCallError> {
    let (tls, peer) = tls_connect(identity, addr, Some(*expected)).await?;
    // The same limits as the server: a peer cannot make us buffer more than one frame.
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME_BYTES))
        .max_frame_size(Some(MAX_FRAME_BYTES));
    let opening = async {
        let (mut ws, _) = client_async_with_config(format!("wss://{addr}/peer"), tls, Some(config))
            .await
            .map_err(map_upgrade_error)?;
        let remote_hello = exchange_hello(&mut ws, &hello)
            .await
            .map_err(PeerCallError::Protocol)?;
        Ok::<_, PeerCallError>((ws, remote_hello))
    };
    let (ws, remote_hello) = tokio::time::timeout(REQUEST_TIMEOUT, opening)
        .await
        .map_err(|_| PeerCallError::Unreachable("the socket did not open in time".to_string()))??;
    let (outbound, inbound) = spawn_pump(ws, None, None);
    Ok(PeerSocket {
        peer,
        dialed_by: identity.local.device_id,
        remote_hello,
        outbound,
        inbound,
    })
}

fn map_upgrade_error(error: WsError) -> PeerCallError {
    match error {
        // The only 403 on `/peer` is "not-paired" (we never send Origin). The body may not
        // have arrived with the headers, so the status alone decides.
        WsError::Http(response) => match response.status().as_u16() {
            403 => PeerCallError::NotPaired,
            503 => PeerCallError::Busy,
            status => PeerCallError::Rejected(status),
        },
        WsError::Io(e) => PeerCallError::Unreachable(e.to_string()),
        _ => PeerCallError::Protocol("the WebSocket upgrade failed".to_string()),
    }
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
