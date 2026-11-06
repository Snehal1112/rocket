//! The peer server. TLS 1.3 with a client certificate on every connection, a fixed route
//! allow-list and the limits of spec section 8.3.

use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::{service_fn, Service as _};
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use rocket_network::limits::{
    DEFAULT_PORT, HEADER_READ_TIMEOUT, HELLO_PER_IP_PER_MINUTE, MAX_CONNECTIONS,
    MAX_REST_BODY_BYTES, MAX_UNPAIRED_CONNECTIONS, PAIR_REQUESTS_PER_MINUTE, PORT_RANGE_END,
};
use rocket_network::{
    DecisionRequest, NonceMessage, PairRequest, PairingId, PeerError, PeerHandler, PeerIdentity,
    PeerServerControl,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};
use tokio_rustls::TlsAcceptor;

use crate::callback_server::ACCEPT_ERROR_BACKOFF;
use crate::network::client::REQUEST_TIMEOUT;
use crate::network::lock::lock_ignore_poison;
use crate::network::{peer_identity_from_certs, server_config, TlsIdentity};

/// A TLS handshake must finish within this time, or the connection is dropped.
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// The fixed window of both rate limits.
const RATE_WINDOW: Duration = Duration::from_secs(60);
/// The per-IP limiter remembers each address for a minute, so its table is capped.
const MAX_LIMITER_KEYS: usize = 4096;
/// The HTTP read buffer. Headers and a 16 KiB body fit with room to spare.
const MAX_HTTP_BUF: usize = 64 * 1024;
/// Pairing ids are UUID strings. Anything longer is refused before the handler runs.
const MAX_PAIRING_ID_LEN: usize = 64;

/// Where and how the server listens. Production uses `Default`; tests narrow it.
#[derive(Debug, Clone)]
pub(crate) struct ServerOptions {
    pub(crate) bind_ip: IpAddr,
    pub(crate) ports: RangeInclusive<u16>,
    pub(crate) max_connections: usize,
    pub(crate) max_unpaired: usize,
    pub(crate) handshake_timeout: Duration,
    pub(crate) header_read_timeout: Duration,
}

impl Default for ServerOptions {
    fn default() -> Self {
        ServerOptions {
            bind_ip: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            ports: DEFAULT_PORT..=PORT_RANGE_END,
            max_connections: MAX_CONNECTIONS,
            max_unpaired: MAX_UNPAIRED_CONNECTIONS,
            handshake_timeout: TLS_HANDSHAKE_TIMEOUT,
            header_read_timeout: HEADER_READ_TIMEOUT,
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ServerOptions {
    /// Loopback and a port the OS picks, so tests never touch the real range.
    pub(crate) fn local_test() -> Self {
        ServerOptions {
            bind_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            ports: 0..=0,
            ..ServerOptions::default()
        }
    }
}

/// The permits of one connection. They are released when the last holder drops this. For a
/// WebSocket that is the pump task, not the finished HTTP connection.
pub(crate) struct ConnSlot {
    _all: OwnedSemaphorePermit,
    _unpaired: Option<OwnedSemaphorePermit>,
}

/// What a route knows about its connection. Added to every request as an extension.
#[derive(Clone)]
pub(crate) struct ConnInfo {
    pub(crate) peer: PeerIdentity,
    pub(crate) remote_ip: IpAddr,
    pub(crate) slot: Arc<ConnSlot>,
    pub(crate) stop: watch::Receiver<()>,
}

/// Shared by every route of one running server.
#[derive(Clone)]
pub(crate) struct RouteState {
    pub(crate) handler: Arc<dyn PeerHandler>,
    hello_limit: Arc<Mutex<FixedWindow<IpAddr>>>,
    pair_limit: Arc<Mutex<FixedWindow<()>>>,
}

struct ServerShared {
    acceptor: TlsAcceptor,
    handler: Arc<dyn PeerHandler>,
    router: Router,
    options: ServerOptions,
    slots: Arc<Semaphore>,
    unpaired: Arc<Semaphore>,
}

pub struct RustlsPeerServer {
    identity: Arc<TlsIdentity>,
    options: ServerOptions,
    /// Dropping the sender stops the accept loop, every connection and every peer socket.
    running: Mutex<Option<watch::Sender<()>>>,
}

impl RustlsPeerServer {
    pub fn new(identity: Arc<TlsIdentity>) -> Self {
        Self::with_options(identity, ServerOptions::default())
    }

    /// A server on 127.0.0.1 with a port the OS picks. For end-to-end tests in other crates.
    #[cfg(any(test, feature = "test-support"))]
    pub fn local_test(identity: Arc<TlsIdentity>) -> Self {
        Self::with_options(identity, ServerOptions::local_test())
    }

    pub(crate) fn with_options(identity: Arc<TlsIdentity>, options: ServerOptions) -> Self {
        RustlsPeerServer {
            identity,
            options,
            running: Mutex::new(None),
        }
    }
}

#[async_trait]
impl PeerServerControl for RustlsPeerServer {
    async fn start(&self, handler: Arc<dyn PeerHandler>) -> DomainResult<u16> {
        // A restart replaces the running listener.
        self.stop();
        let acceptor = TlsAcceptor::from(server_config(&self.identity)?);
        let listener = bind_first_free(self.options.bind_ip, self.options.ports.clone()).await?;
        let port = listener
            .local_addr()
            .map_err(|e| DomainError::Io(format!("read the peer port: {e}")))?
            .port();
        let state = RouteState {
            handler: Arc::clone(&handler),
            hello_limit: Arc::new(Mutex::new(FixedWindow::new(
                HELLO_PER_IP_PER_MINUTE,
                RATE_WINDOW,
                MAX_LIMITER_KEYS,
            ))),
            pair_limit: Arc::new(Mutex::new(FixedWindow::new(
                PAIR_REQUESTS_PER_MINUTE,
                RATE_WINDOW,
                1,
            ))),
        };
        let shared = Arc::new(ServerShared {
            acceptor,
            handler,
            router: build_router(state),
            options: self.options.clone(),
            slots: Arc::new(Semaphore::new(self.options.max_connections)),
            unpaired: Arc::new(Semaphore::new(self.options.max_unpaired)),
        });
        let (stop_tx, stop_rx) = watch::channel(());
        tokio::spawn(accept_loop(listener, shared, stop_rx));
        *lock_ignore_poison(&self.running) = Some(stop_tx);
        Ok(port)
    }

    fn stop(&self) {
        // Dropping the sender makes every `changed()` on its receivers return an error.
        drop(lock_ignore_poison(&self.running).take());
    }
}

/// Binds the first free port of the range. Spec 7.1: 47821, else the next free up to 47830.
async fn bind_first_free(ip: IpAddr, ports: RangeInclusive<u16>) -> DomainResult<TcpListener> {
    let mut last_error = String::from("the port range is empty");
    for port in ports {
        match TcpListener::bind(SocketAddr::new(ip, port)).await {
            Ok(listener) => return Ok(listener),
            Err(e) => last_error = e.to_string(),
        }
    }
    Err(DomainError::Io(format!("no free peer port: {last_error}")))
}

async fn accept_loop(
    listener: TcpListener,
    shared: Arc<ServerShared>,
    mut stop: watch::Receiver<()>,
) {
    loop {
        tokio::select! {
            // The server was stopped. Dropping `listener` at the end closes the port.
            _ = stop.changed() => break,
            accepted = listener.accept() => {
                let Ok((tcp, remote)) = accepted else {
                    tokio::select! {
                        _ = stop.changed() => break,
                        _ = tokio::time::sleep(ACCEPT_ERROR_BACKOFF) => {}
                    }
                    continue;
                };
                // Over the cap: dropping the stream closes it.
                let Ok(slot) = Arc::clone(&shared.slots).try_acquire_owned() else {
                    continue;
                };
                tokio::spawn(serve_connection(tcp, remote, Arc::clone(&shared), stop.clone(), slot));
            }
        }
    }
}

async fn serve_connection(
    tcp: TcpStream,
    remote: SocketAddr,
    shared: Arc<ServerShared>,
    mut stop: watch::Receiver<()>,
    slot: OwnedSemaphorePermit,
) {
    let _ = tcp.set_nodelay(true);
    // A handshake that never finishes must not hold a slot.
    let handshake = tokio::time::timeout(
        shared.options.handshake_timeout,
        shared.acceptor.accept(tcp),
    );
    let tls = tokio::select! {
        _ = stop.changed() => return,
        result = handshake => match result {
            Ok(Ok(tls)) => tls,
            _ => return,
        },
    };
    // Plan 02's config requires a client certificate, so this only fails on a broken one.
    let Ok(peer) = peer_identity_from_certs(tls.get_ref().1.peer_certificates()) else {
        return;
    };
    let unpaired = if shared.handler.is_paired(&peer.device_id).await {
        None
    } else {
        match Arc::clone(&shared.unpaired).try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => return,
        }
    };
    let info = ConnInfo {
        peer,
        remote_ip: remote.ip(),
        slot: Arc::new(ConnSlot {
            _all: slot,
            _unpaired: unpaired,
        }),
        stop: stop.clone(),
    };
    let router = TowerToHyperService::new(shared.router.clone());
    let service = service_fn(move |mut req: hyper::Request<Incoming>| {
        let router = router.clone();
        let info = info.clone();
        async move {
            // Browsers send Origin on cross-site requests. Refusing it stops web pages and
            // DNS rebinding from reaching any route.
            if req.headers().contains_key(header::ORIGIN) {
                return Ok::<Response, Infallible>(closing(text(
                    StatusCode::FORBIDDEN,
                    "forbidden",
                )));
            }
            req.extensions_mut().insert(info);
            let reply = match tokio::time::timeout(REQUEST_TIMEOUT, router.call(req)).await {
                Ok(response) => response,
                Err(_) => Ok(text(StatusCode::REQUEST_TIMEOUT, "timeout")),
            };
            reply.map(closing)
        }
    });
    // One request per connection: `closing` marks every reply except the upgrade. The hyper
    // `keep_alive(false)` switch is not used, because it also rewrites the `Connection: upgrade`
    // header of the 101 reply and no WebSocket client accepts that.
    let connection = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(shared.options.header_read_timeout)
        .max_buf_size(MAX_HTTP_BUF)
        .serve_connection(TokioIo::new(tls), service)
        .with_upgrades();
    tokio::pin!(connection);
    tokio::select! {
        _ = connection.as_mut() => {}
        // The server was stopped: end this connection too.
        _ = stop.changed() => {}
    }
}

/// The route allow-list of spec 8.1. Everything else is 404.
fn build_router(state: RouteState) -> Router {
    Router::new()
        .route("/hello", get(hello))
        .route("/pair/request", post(pair_request))
        .route("/pair/{id}/nonce", post(pair_nonce))
        .route("/pair/{id}/decision", post(pair_decision))
        .route("/pair/{id}", get(pair_status))
        .route("/peer", get(crate::network::socket::peer_upgrade))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_REST_BODY_BYTES))
        .with_state(state)
}

async fn hello(State(state): State<RouteState>, Extension(conn): Extension<ConnInfo>) -> Response {
    if !allow(&state.hello_limit, conn.remote_ip) {
        return text(StatusCode::TOO_MANY_REQUESTS, "slow-down");
    }
    Json(state.handler.hello().await).into_response()
}

async fn pair_request(
    State(state): State<RouteState>,
    Extension(conn): Extension<ConnInfo>,
    Json(req): Json<PairRequest>,
) -> Response {
    // One budget for all strangers. A busy request gets no notice on this node.
    if !allow(&state.pair_limit, ()) {
        return text(StatusCode::SERVICE_UNAVAILABLE, "busy");
    }
    let Ok(profile) = req.profile.validated() else {
        return peer_error(PeerError::BadRequest);
    };
    let req = PairRequest { profile, ..req };
    match state
        .handler
        .pair_request(conn.peer, conn.remote_ip, req)
        .await
    {
        Ok(reply) => Json(reply).into_response(),
        Err(e) => peer_error(e),
    }
}

async fn pair_nonce(
    State(state): State<RouteState>,
    Extension(conn): Extension<ConnInfo>,
    Path(id): Path<String>,
    Json(msg): Json<NonceMessage>,
) -> Response {
    let Some(id) = pairing_id(id) else {
        return peer_error(PeerError::BadRequest);
    };
    match state.handler.pair_nonce(conn.peer, id, msg).await {
        Ok(reply) => Json(reply).into_response(),
        Err(e) => peer_error(e),
    }
}

async fn pair_decision(
    State(state): State<RouteState>,
    Extension(conn): Extension<ConnInfo>,
    Path(id): Path<String>,
    Json(req): Json<DecisionRequest>,
) -> Response {
    let Some(id) = pairing_id(id) else {
        return peer_error(PeerError::BadRequest);
    };
    match state.handler.pair_decision(conn.peer, id, req).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => peer_error(e),
    }
}

async fn pair_status(
    State(state): State<RouteState>,
    Extension(conn): Extension<ConnInfo>,
    Path(id): Path<String>,
) -> Response {
    let Some(id) = pairing_id(id) else {
        return peer_error(PeerError::BadRequest);
    };
    match state.handler.pair_status(conn.peer, id).await {
        Ok(reply) => Json(reply).into_response(),
        Err(e) => peer_error(e),
    }
}

async fn not_found() -> Response {
    text(StatusCode::NOT_FOUND, "not-found")
}

fn pairing_id(raw: String) -> Option<PairingId> {
    is_valid_pairing_id(&raw).then_some(PairingId(raw))
}

/// A pairing id may only hold letters, digits and dashes, so it can never change a path.
pub(crate) fn is_valid_pairing_id(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= MAX_PAIRING_ID_LEN
        && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Maps a handler refusal to its status. The bodies are fixed words, never details.
pub(crate) fn peer_error(error: PeerError) -> Response {
    let body = match error {
        PeerError::NotFound => "not-found",
        PeerError::NotPaired => "not-paired",
        PeerError::Busy => "busy",
        PeerError::Forbidden => "forbidden",
        PeerError::BadRequest => "bad-request",
        PeerError::Internal => "internal",
    };
    let status =
        StatusCode::from_u16(error.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    text(status, body)
}

pub(crate) fn text(status: StatusCode, body: &'static str) -> Response {
    (status, body).into_response()
}

fn allow<K: Eq + Hash>(limiter: &Mutex<FixedWindow<K>>, key: K) -> bool {
    lock_ignore_poison(limiter).allow(key, Instant::now())
}

/// Counts calls per key in fixed windows. When the table is full of live keys, new keys are
/// refused, so a flood of addresses can neither grow memory nor get around the limit.
pub(crate) struct FixedWindow<K> {
    limit: u32,
    window: Duration,
    max_keys: usize,
    entries: HashMap<K, (Instant, u32)>,
}

impl<K: Eq + Hash> FixedWindow<K> {
    pub(crate) fn new(limit: u32, window: Duration, max_keys: usize) -> Self {
        FixedWindow {
            limit,
            window,
            max_keys,
            entries: HashMap::new(),
        }
    }

    pub(crate) fn allow(&mut self, key: K, now: Instant) -> bool {
        if !self.entries.contains_key(&key) && self.entries.len() >= self.max_keys {
            let window = self.window;
            self.entries
                .retain(|_, (start, _)| now.saturating_duration_since(*start) < window);
            if self.entries.len() >= self.max_keys {
                return false;
            }
        }
        let entry = self.entries.entry(key).or_insert((now, 0));
        if now.saturating_duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        if entry.1 >= self.limit {
            return false;
        }
        entry.1 += 1;
        true
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Ends the connection after this reply, except for the WebSocket upgrade.
fn closing(mut response: Response) -> Response {
    if response.status() != StatusCode::SWITCHING_PROTOCOLS {
        response.headers_mut().insert(
            header::CONNECTION,
            header::HeaderValue::from_static("close"),
        );
    }
    response
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
