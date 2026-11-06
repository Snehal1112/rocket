# Team Network — Plan 3: Transport and discovery

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 3 of 5.** Previous plan: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-02-identity-storage-tls.md` (must be merged and green).
**Next plan to run: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-04-service-tauri.md`**
**Recommended model: Opus.** This is the security-critical networking plan. The final review is also Opus (spec section 8.7).

**Goal:** Two Rocket nodes can talk. A TLS 1.3 peer server with client certificates serves `/hello`, the four pairing routes and the `/peer` WebSocket, with every limit of spec section 8.3. A connector makes the same calls from the other side. A pump turns each peer WebSocket into two `PeerMessage` channels. mDNS announces this node and reports others. A factory builds the whole transport from the device key. Nothing is wired into the app yet (Plan 04 does that).

**Architecture:** All code is in `crates/rocket-infra/src/network/`, on top of Plan 02's `TlsIdentity`, `server_config`, `client_config` and `peer_identity_from_certs`. The server runs its own accept loop (like `callback_server.rs`): TCP accept, a connection permit, a TLS handshake with a timeout, the client key read from the handshake, an extra permit for unpaired keys, then hyper's HTTP/1.1 connection (with upgrades) serving an axum `Router`. The connection's `PeerIdentity`, remote IP, permits and stop signal reach the routes as a request extension (`ConnInfo`). The `Origin` check and a per-request timeout run in a thin hyper `service_fn` in front of the router, so they cover every path. The client opens one TLS connection per REST call (hyper's low-level `http1` client, index deviation 5) and uses `tokio_tungstenite::client_async_with_config` over its own TLS stream for the socket (index deviation 6). One generic pump serves both WebSocket libraries through a three-method `WsFrame` trait. mDNS uses `mdns-sd` on a std thread that maps events through pure, tested functions.

**Tech Stack:** Rust, axum 0.8.9 (`ws`, `json`, `tokio`, `http1`, no default features), hyper 1.8.1 (`server`, `http1`, `client`), hyper-util 0.1.20 (`tokio`, `service`), tokio-rustls 0.26.4 and rustls 0.23.37 (from Plan 02), tokio-tungstenite 0.30, futures-util 0.3, mdns-sd 0.21.6 (new, pinned).

**Spec:** `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md` sections 3a (hub, 30 peers), 5.2 (`MdnsDiscovery`, `PeerServer`, `PeerClient`), 6.1 and 6.2 (channel and pairing routes), 7.1 (mDNS, port range), 7.3 (peer socket, Hello first), 8.1 to 8.3 (allow-list, TLS, limits), 9 (port taken, bind failure, mDNS unavailable), 10 (in-process tests). Index: `docs/superpowers/plans/team-network/00-plan-index.md` (binding names, deviations 5 and 6).

## Global Constraints

- `-j4` on every cargo invocation. Never `cargo test --workspace` or `--all`. Use `cargo test -j4 -p rocket-infra <filter>`.
- Use only names from the index, Plan 01 and Plan 02. Plan 01 additions used here: `NodeProfile::validated`, `commitment_from_base64`, `commitment_to_base64`, `PeerError::status_code`, `Nonce::parse_base64`. Plan 02 test helpers used here: `crate::network::tls_identity::test_support::{identity, key_material}`. Plan 02 helpers for production code: `crate::network::lock::lock_ignore_poison`. Shared constants: `crate::callback_server::{ACCEPT_ERROR_BACKOFF, HEADER_READ_TIMEOUT}` (Task 1 Step 1b) and `crate::network::client::REQUEST_TIMEOUT` (declared once, imported by `server.rs` and `socket.rs`).
- No unwrap or expect calls in production code. Tests use `expect("reason")`. Never write a bare unwrap call anywhere (a repo hook rejects Rust text that contains one, tests included). Mutex locks use `crate::network::lock::lock_ignore_poison(&mutex)` (Plan 02 Task 1). The `.lock().unwrap_or_else(|e| e.into_inner())` form is not repeated in this plan.
- The peer server calls only `PeerHandler`. It never touches collections, environments, scripts, history, RocketVault or `process.env` (spec 8.1).
- Never log or put into an error text: nonces, codes, keys, certificates or frame contents. Error texts toward peers are fixed words (`not-paired`, `busy`, ...).
- Peer wire types are snake_case JSON from `rocket-network`. No camelCase here.
- Code comments are short full sentences that end with a period.
- No commits during this plan. Every task ends with "no commit; reviewed at the plan end".
- Line numbers drift. Edits are anchored on quoted text.
- Every task touches `rocket-infra`, so per the root `CLAUDE.md` rule each task starts with the OpenCollection read step. These files are not collection files; the read is only the repo rule.

## Review Focus

1. **A TLS handshake that never completes must not hold a slot** (Task 1, `a_stalled_tls_handshake_frees_its_slot_after_the_timeout`). Two raw TCP connections that send nothing fill a 2-slot server. A third client is dropped. After the handshake timeout both slots are free again and `/hello` works. The real timeout is 10 s (`TLS_HANDSHAKE_TIMEOUT`); the test injects 1 s through `ServerOptions`.
2. **An unpaired key opening 33 connections** (Task 1, `unpaired_connections_stop_at_32_but_paired_devices_still_connect`). One stranger key holds 32 idle TLS connections. A 33rd unpaired connection is dropped right after the handshake, a paired device still gets in, and closing one stranger connection frees a slot. The unpaired cap is per server, not per key.
3. **`Origin` header on `/hello`** (Task 1, `an_origin_header_is_refused_before_any_route`). `GET /hello`, `POST /pair/request` and an unknown path with `Origin: http://evil.example` all get `403 forbidden`, and the handler is never called. The check runs in the `service_fn` in front of the router, so it covers every path, `/peer` included (Task 2, `an_origin_header_on_peer_is_refused_even_for_a_paired_key`).
4. **Hello is not the first frame** (Task 2, `a_frame_before_hello_closes_without_a_socket`). A paired raw client sends `{"type":"ping"}` first. The server closes the socket and `on_peer_socket` is never called. A second Hello after the first also closes (`a_second_hello_closes_the_socket`).
5. **The same peer dials twice** (Task 2, `the_same_device_dialling_twice_gets_two_sockets`). The transport delivers both sockets to `on_peer_socket` with the same `peer` and `dialed_by`. Keeping one is Plan 04's rule (`keep_dialed_by`); the transport must not silently drop either, or Plan 04 cannot apply the rule.
6. **A WebSocket keeps its connection slot after the HTTP connection ends** (Task 2, `an_open_socket_keeps_its_connection_slot`). After the 101 response hyper's connection future finishes; the permits live in `Arc<ConnSlot>` held by the pump task, so 256 open sockets really fill the 256 slots.
7. **Wrong expected id sends nothing** (Task 1, `a_wrong_expected_id_is_an_identity_mismatch_and_sends_nothing`). The client handshake fails inside Plan 02's verifier, so no request byte reaches the server.

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-infra/Cargo.toml` | modify | hyper `client` feature, hyper-util `service` feature, `axum`, `mdns-sd` |
| `crates/rocket-infra/src/network/mod.rs` | modify | New modules and re-exports |
| `crates/rocket-infra/src/callback_server.rs` | modify (Task 1 Step 1b) | `ACCEPT_ERROR_BACKOFF` and `HEADER_READ_TIMEOUT` become `pub(crate)` so the peer server imports them |
| `crates/rocket-infra/src/lib.rs` | modify | Re-export the four Plan 03 types (Task 3) |
| `crates/rocket-infra/src/network/server.rs` | create | `RustlsPeerServer`, `ServerOptions`, accept loop, routes, `FixedWindow` limiter |
| `crates/rocket-infra/src/network/server_tests.rs` | create | REST and limit tests (`network::server::tests`) |
| `crates/rocket-infra/src/network/client.rs` | create | `RustlsPeerConnector`, `tls_connect`, `send_request`, the one `REQUEST_TIMEOUT` |
| `crates/rocket-infra/src/network/peer_test_support.rs` | create | Test-only `FakeHandler`, `Node`, helpers |
| `crates/rocket-infra/src/network/socket.rs` | create | `/peer` route, Hello exchange, `WsFrame`, pump, client socket |
| `crates/rocket-infra/src/network/socket_tests.rs` | create | WebSocket tests (`network::socket::tests`) |
| `crates/rocket-infra/src/network/mdns.rs` | create | `MdnsDiscovery`, TXT encode and parse, `BrowseState` |
| `crates/rocket-infra/src/network/transport.rs` | create | `RustlsPeerTransportFactory`, plus the `local_test()` seam for Plan 04b |
| `crates/rocket-infra/src/network/tls_identity.rs` | modify (Task 3 Step 5b) | `test_support` becomes `pub` under `test` or the `test-support` feature, for Plan 04b |

## Dependency decisions

- `hyper` gains `client` (for `hyper::client::conn::http1::handshake`). `hyper-util` gains `service` (for `TowerToHyperService`, verified in hyper-util 0.1.20 `src/service/glue.rs`).
- `axum = { version = "0.8", default-features = false, features = ["ws", "json", "tokio", "http1"] }`. The lock already has 0.8.9 (src-tauri). The `ws` feature pulls axum's own `tokio-tungstenite` 0.29 and `tungstenite` 0.29 (axum 0.8.9 `Cargo.toml`), so the tree gets a second tungstenite next to our 0.30. That is the price of using axum's tested upgrade code instead of writing our own handshake checks. The pump is generic, so the two message types never meet.
- `mdns-sd = { version = "=0.21.6", default-features = false }`. 0.21.6 was published 2026-10-19 (18 days ago); 0.21.7 (2026-11-02) is too new, hence the exact pin (precedent: `deno_error = "=0.7.1"`). Default features (`async`, `logging`) are not needed. New transitive crates: `flume` 0.12.0 (2025-12), `if-addrs` 0.15.0 (2026-02), `socket-pktinfo` 0.4.x (2026-07 or older). `mdns-sd` needs `mio >= 1.2`, so the lock moves `mio` 1.1.1 to 1.2.x (1.2.4 is from 2026-10-03). All are older than two weeks.
- `tokio-rustls`, `rustls`, `rcgen` and `rocket-network` are already normal dependencies after Plan 02. `tokio-tungstenite` (`connect` includes `handshake`) and `futures-util` (`sink`, `std`, so `StreamExt::split` exists) need no change.

API facts checked in `~/.cargo/registry/src` (and, for mdns-sd, in the 0.21.6 crate source downloaded to the session scratchpad):
- axum 0.8.9: `WebSocketUpgrade::{max_message_size, max_frame_size, on_upgrade}`; the extractor removes `hyper::upgrade::OnUpgrade` from the request extensions (so hyper's connection must use `.with_upgrades()`); `Result<T, T::Rejection>` is an extractor (axum-core 0.5.6 `extract/mod.rs:107`); `axum::extract::ws::rejection::WebSocketUpgradeRejection`; a plain GET without upgrade headers is `400` (`InvalidConnectionHeader`); `Message::{Text(Utf8Bytes), Binary, Ping, Pong, Close}`, not `non_exhaustive`; `Router<()>` implements tower `Service<Request<B>>` with `Error = Infallible`.
- hyper 1.8.1: `server::conn::http1::Builder::{timer, header_read_timeout, keep_alive, max_buf_size, serve_connection}`, `Connection::with_upgrades`; the dispatcher hands over a pending upgrade before it honours `keep_alive(false)` (`proto/h1/dispatch.rs:151-159`); `client::conn::http1::handshake`.
- tokio-rustls 0.26.4: rustls errors surface as `io::Error::new(InvalidData, rustls::Error)` (`common/mod.rs:115`), so `get_ref()` plus `downcast_ref::<rustls::Error>()` finds them. `TlsStream::get_ref() -> (&IO, &Connection)`; `peer_certificates()` on both connection kinds.
- rustls-pki-types 1.14.0: `impl From<std::net::IpAddr> for ServerName<'_>`. Plan 02's client verifier ignores the server name and returns `Error::InvalidCertificate(CertificateError::ApplicationVerificationFailure)` on an id mismatch.
- tokio-tungstenite 0.30: `client_async_with_config(request, stream, Option<WebSocketConfig>)`, `client_async`; tungstenite 0.30 `WebSocketConfig::default().max_message_size(Some(n)).max_frame_size(Some(n))` (struct is `non_exhaustive`), `Error::Http(Box<Response<Option<Vec<u8>>>>)` whose body holds only bytes already read, `Message::Frame` exists. The client request carries no `Origin` header (`handshake/client.rs` `generate_request`).
- mdns-sd 0.21.6: `ServiceDaemon::new() -> Result<ServiceDaemon>` (opens sockets lazily), `Clone`; `register(ServiceInfo)`, `unregister(&str) -> Result<Receiver<UnregisterStatus>>`, `browse(&str) -> Result<Receiver<ServiceEvent>>`, `stop_browse(&str)`, `shutdown() -> Result<Receiver<DaemonStatus>>`; `ServiceInfo::new(ty, instance, host, ip: impl AsIpAddrs, port, props: impl IntoTxtProperties)`, `()` is an empty `AsIpAddrs`, `&[(K, V)]` is `IntoTxtProperties`, `.enable_addr_auto()`, `get_fullname()`; `ServiceEvent` is `non_exhaustive` with `ServiceResolved(Box<ResolvedService>)` and `ServiceRemoved(String, String)`; `ResolvedService { port, .. }` with `get_fullname()`, `get_addresses() -> &HashSet<ScopedIp>`, `get_property_val_str(&str) -> Option<&str>`; `ScopedIp::to_ip_addr()`; `mdns_sd::{Receiver, RecvTimeoutError}` re-export flume.

---

### Task 1: Peer server and REST connector

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`, `crates/rocket-infra/src/network/mod.rs`, `crates/rocket-infra/src/callback_server.rs`
- Create: `crates/rocket-infra/src/network/server.rs`, `crates/rocket-infra/src/network/server_tests.rs`, `crates/rocket-infra/src/network/client.rs`, `crates/rocket-infra/src/network/peer_test_support.rs`

**Interfaces:**
- Consumes: `rocket_network::{limits::*, DeviceId, NodeProfile, Nonce, PeerIdentity, PairingId, PairingStatus, HelloResponse, PairRequest, PairRequestResponse, NonceMessage, DecisionRequest, PairStatusResponse, PeerMessage, PeerSocket, PeerHandler, PeerConnector, PeerServerControl, PeerError, PeerCallError, commitment_from_base64, commitment_to_base64}`; `crate::network::{TlsIdentity, server_config, client_config, peer_identity_from_certs}`; `crate::network::tls_identity::test_support::identity` (tests).
- Produces (Task 2, Task 3, Plan 04):

```rust
// server.rs
pub struct RustlsPeerServer { /* identity, options, running */ }
impl RustlsPeerServer {
    pub fn new(identity: Arc<TlsIdentity>) -> Self;                                   // production options
    pub(crate) fn with_options(identity: Arc<TlsIdentity>, options: ServerOptions) -> Self; // test seam
}
#[async_trait] impl PeerServerControl for RustlsPeerServer {
    async fn start(&self, handler: Arc<dyn PeerHandler>) -> DomainResult<u16>; // stops a running instance first
    fn stop(&self);                                                            // idempotent
}
#[derive(Debug, Clone)]
pub(crate) struct ServerOptions {
    pub(crate) bind_ip: IpAddr,                 // 0.0.0.0
    pub(crate) ports: RangeInclusive<u16>,      // DEFAULT_PORT..=PORT_RANGE_END
    pub(crate) max_connections: usize,          // MAX_CONNECTIONS
    pub(crate) max_unpaired: usize,             // MAX_UNPAIRED_CONNECTIONS
    pub(crate) handshake_timeout: Duration,     // 10 s
    pub(crate) header_read_timeout: Duration,   // HEADER_READ_TIMEOUT
}
impl Default for ServerOptions;                  // the values above
#[cfg(any(test, feature = "test-support"))] impl ServerOptions { pub(crate) fn local_test() -> Self; } // 127.0.0.1, ports 0..=0 (gate widened in Task 3 Step 5b)
pub(crate) struct ConnSlot;                      // the two permits
#[derive(Clone)] pub(crate) struct ConnInfo { pub(crate) peer: PeerIdentity, pub(crate) remote_ip: IpAddr, pub(crate) slot: Arc<ConnSlot>, pub(crate) stop: watch::Receiver<()> }
#[derive(Clone)] pub(crate) struct RouteState { pub(crate) handler: Arc<dyn PeerHandler>, /* limiters */ }
pub(crate) fn text(status: StatusCode, body: &'static str) -> Response;
pub(crate) fn peer_error(error: PeerError) -> Response;
pub(crate) fn is_valid_pairing_id(raw: &str) -> bool;  // 1..=64 of [A-Za-z0-9-]
pub(crate) struct FixedWindow<K>;                       // new(limit, window, max_keys), allow(key, now) -> bool

// client.rs
pub struct RustlsPeerConnector { /* identity */ }
impl RustlsPeerConnector { pub fn new(identity: Arc<TlsIdentity>) -> Self; }
#[async_trait] impl PeerConnector for RustlsPeerConnector { /* all six methods; open_socket is finished in Task 2 */ }
pub(crate) const CONNECT_TIMEOUT: Duration;  // 5 s, TCP plus TLS
pub(crate) const REQUEST_TIMEOUT: Duration;  // 10 s, one request and its reply, or one socket open; the only declaration, server.rs and socket.rs import it
pub(crate) type ClientTls = tokio_rustls::client::TlsStream<TcpStream>;
pub(crate) struct RawReply { pub(crate) peer: PeerIdentity, pub(crate) status: u16, pub(crate) body: Bytes }
pub(crate) async fn tls_connect(identity: &TlsIdentity, addr: SocketAddr, expected: Option<DeviceId>) -> Result<(ClientTls, PeerIdentity), PeerCallError>;
pub(crate) async fn send_request(identity: &TlsIdentity, addr: SocketAddr, expected: Option<DeviceId>, method: Method, path: &str, body: Option<Vec<u8>>, headers: &[(&str, &str)]) -> Result<RawReply, PeerCallError>;
```

Wire behaviour of the server (the contract Plan 04 and the tests rely on):

| Request | Result |
|---|---|
| any request with an `Origin` header | `403 forbidden`, no handler call |
| `GET /hello` | `200` JSON `HelloResponse`; over 30 a minute from one IP: `429 slow-down` |
| `POST /pair/request` | over 5 a minute across all callers: `503 busy`; invalid profile: `400`; else the handler's answer |
| `POST /pair/{id}/nonce`, `POST /pair/{id}/decision`, `GET /pair/{id}` | id not 1..=64 of `[A-Za-z0-9-]`: `400`; decision success is `204` |
| handler `Err(PeerError)` | `status_code()` with body `not-found`, `not-paired`, `busy`, `forbidden`, `bad-request` or `internal` |
| body over 16 KiB | `413` (axum `DefaultBodyLimit`) |
| known path, wrong method | `405` (axum default) |
| any other path | `404 not-found` |
| request not answered within 10 s | `408 timeout` |

Client error mapping: TCP or I/O failure or timeout -> `Unreachable`; `rustls::Error::InvalidCertificate(ApplicationVerificationFailure)` or a hello body whose `device_id` differs from the certificate -> `IdentityMismatch`; any other TLS error or a malformed reply -> `Protocol`; `403` with body `not-paired` -> `NotPaired`; `503` -> `Busy`; any other non-2xx -> `Rejected(status)`.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 1: Add the dependencies**

In `crates/rocket-infra/Cargo.toml`, under `[dependencies]`, replace

```toml
hyper = { version = "1", features = ["server", "http1"] }
hyper-util = { version = "0.1", features = ["tokio"] }
```

with

```toml
# `client` is the peer connector's HTTPS client (team network).
hyper = { version = "1", features = ["server", "http1", "client"] }
# `service` adapts the peer server's axum router to a hyper connection.
hyper-util = { version = "0.1", features = ["tokio", "service"] }
```

and after the line `bytes = "1"` add

```toml
# The peer server routes of the team network. `ws` serves the `/peer` upgrade. No default
# features, so nothing else is pulled in.
axum = { version = "0.8", default-features = false, features = ["ws", "json", "tokio", "http1"] }
```

Run: `cargo check -j4 -p rocket-infra`
Expected: PASS (only the lock file changes: `tokio-tungstenite` 0.29 and `tungstenite` 0.29 appear next to 0.30).

- [ ] **Step 1b: Share two constants with the callback server**

`callback_server.rs` already has the accept-error pause (75 ms) and the header read timeout (10 s) that the peer server needs. Export them instead of declaring them a second time. `MAX_CONNECTIONS` stays separate: the peer server uses `rocket_network::limits::MAX_CONNECTIONS` (256), the callback server has its own 64.

In `crates/rocket-infra/src/callback_server.rs`, replace

```rust
/// Pause after a failed accept.
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(75);

/// A client must send its request headers within this time.
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
```

with

```rust
/// Pause after a failed accept. Also used by the team network peer server.
pub(crate) const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(75);

/// A client must send its request headers within this time. Also checked against the team
/// network limit of the same name in the peer server tests.
pub(crate) const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
```

Nothing else in that file changes. Run: `cargo test -j4 -p rocket-infra callback_server`
Expected: PASS, the same tests as before (a visibility change only).

- [ ] **Step 2: Register the modules**

Replace the whole content of `crates/rocket-infra/src/network/mod.rs` (Plan 02 Task 3 version) with:

```rust
//! Team network pieces that touch the machine: the device key, the local lists, the TLS
//! configs, the peer server and the peer connector. The domain rules live in `rocket-network`.

pub mod client;
pub mod identity_store;
pub(crate) mod lock;
#[cfg(test)]
pub(crate) mod peer_test_support;
pub mod repos;
pub mod server;
pub mod tls_config;
pub mod tls_identity;

pub use client::RustlsPeerConnector;
pub use identity_store::KeychainDeviceIdentity;
pub use repos::{
    FsBlockedDeviceRepo, FsManualAddressRepo, FsNetworkSettingsRepo, FsPairedDeviceRepo,
};
pub use server::RustlsPeerServer;
pub use tls_config::{client_config, peer_identity_from_certs, server_config};
pub use tls_identity::{build_tls_identity, spki_from_cert, TlsIdentity};
```

Create `crates/rocket-infra/src/network/server.rs` with only:

```rust
//! The peer server. TLS 1.3 with a client certificate on every connection, a fixed route
//! allow-list and the limits of spec section 8.3.

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
```

Create `crates/rocket-infra/src/network/client.rs` with only:

```rust
//! The peer client. Every REST call opens its own TLS 1.3 connection with our device
//! certificate, sends one HTTP/1.1 request and closes.
```

- [ ] **Step 3: Write the test support module**

Create `crates/rocket-infra/src/network/peer_test_support.rs`:

```rust
//! Helpers shared by the peer transport tests. Compiled only for tests.

use std::collections::HashSet;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use rocket_network::limits::PROTOCOL_VERSION;
use rocket_network::{
    commitment_to_base64, DecisionRequest, DeviceId, HelloResponse, NodeProfile, Nonce,
    NonceMessage, PairRequest, PairRequestResponse, PairStatusResponse, PairingId, PairingStatus,
    PeerCallError, PeerError, PeerHandler, PeerIdentity, PeerMessage, PeerServerControl,
    PeerSocket,
};
use tokio::sync::mpsc;

use crate::network::client::RustlsPeerConnector;
use crate::network::lock::lock_ignore_poison;
use crate::network::server::{RustlsPeerServer, ServerOptions};
use crate::network::tls_identity::test_support;
use crate::network::TlsIdentity;

/// A fresh device identity in an `Arc`, as the server and connector take it.
pub(crate) fn identity() -> Arc<TlsIdentity> {
    Arc::new(test_support::identity())
}

pub(crate) fn profile(name: &str) -> NodeProfile {
    NodeProfile::new(name).expect("a valid display name")
}

pub(crate) fn hello_for(name: &str) -> PeerMessage {
    PeerMessage::Hello {
        version: PROTOCOL_VERSION,
        profile: profile(name),
    }
}

/// The nonce the fake handler reveals.
pub(crate) fn reveal_nonce() -> String {
    Nonce([9u8; 32]).to_base64()
}

/// The commitment the fake handler sends.
pub(crate) fn commitment_text() -> String {
    commitment_to_base64(&[7u8; 32])
}

/// Records every call and hands over every socket through a channel.
pub(crate) struct FakeHandler {
    local: DeviceId,
    name: String,
    paired: Mutex<HashSet<DeviceId>>,
    calls: Mutex<Vec<String>>,
    sockets: mpsc::UnboundedSender<PeerSocket>,
}

impl FakeHandler {
    pub(crate) fn new(
        local: &TlsIdentity,
        name: &str,
    ) -> (Arc<FakeHandler>, mpsc::UnboundedReceiver<PeerSocket>) {
        let (sockets, received) = mpsc::unbounded_channel();
        let handler = FakeHandler {
            local: local.local.device_id,
            name: name.to_string(),
            paired: Mutex::new(HashSet::new()),
            calls: Mutex::new(Vec::new()),
            sockets,
        };
        (Arc::new(handler), received)
    }

    pub(crate) fn pair(&self, device: DeviceId) {
        lock_ignore_poison(&self.paired).insert(device);
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        lock_ignore_poison(&self.calls).clone()
    }

    fn record(&self, call: String) {
        lock_ignore_poison(&self.calls).push(call);
    }
}

/// Pairing ids that make the fake refuse, so the tests can check the status mapping.
fn scripted(id: &PairingId) -> Result<(), PeerError> {
    match id.0.as_str() {
        "missing" => Err(PeerError::NotFound),
        "not-paired" => Err(PeerError::NotPaired),
        "busy" => Err(PeerError::Busy),
        "forbidden" => Err(PeerError::Forbidden),
        _ => Ok(()),
    }
}

#[async_trait]
impl PeerHandler for FakeHandler {
    async fn hello(&self) -> HelloResponse {
        self.record("hello".to_string());
        HelloResponse {
            device_id: self.local,
            version: PROTOCOL_VERSION,
            profile: profile(&self.name),
        }
    }

    async fn pair_request(
        &self,
        peer: PeerIdentity,
        remote_ip: IpAddr,
        req: PairRequest,
    ) -> Result<PairRequestResponse, PeerError> {
        self.record(format!(
            "pair_request:{}:{remote_ip}:{}",
            peer.device_id.short(),
            req.profile.display_name
        ));
        Ok(PairRequestResponse {
            pairing_id: PairingId("p-1".to_string()),
            commitment: commitment_text(),
        })
    }

    async fn pair_nonce(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        _msg: NonceMessage,
    ) -> Result<NonceMessage, PeerError> {
        scripted(&id)?;
        self.record(format!("pair_nonce:{}:{}", peer.device_id.short(), id.0));
        Ok(NonceMessage {
            nonce: reveal_nonce(),
        })
    }

    async fn pair_decision(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        req: DecisionRequest,
    ) -> Result<(), PeerError> {
        scripted(&id)?;
        self.record(format!(
            "pair_decision:{}:{}:{}",
            peer.device_id.short(),
            id.0,
            req.accept
        ));
        Ok(())
    }

    async fn pair_status(
        &self,
        peer: PeerIdentity,
        id: PairingId,
    ) -> Result<PairStatusResponse, PeerError> {
        scripted(&id)?;
        self.record(format!("pair_status:{}:{}", peer.device_id.short(), id.0));
        Ok(PairStatusResponse {
            status: PairingStatus::Pending,
        })
    }

    async fn is_paired(&self, device: &DeviceId) -> bool {
        lock_ignore_poison(&self.paired).contains(device)
    }

    async fn on_peer_socket(&self, socket: PeerSocket) {
        self.record(format!("socket:{}", socket.peer.device_id.short()));
        let _ = self.sockets.send(socket);
    }

    fn local_hello(&self) -> PeerMessage {
        hello_for(&self.name)
    }
}

/// A running test server with its fake handler.
pub(crate) struct Node {
    pub(crate) tls: Arc<TlsIdentity>,
    pub(crate) handler: Arc<FakeHandler>,
    pub(crate) sockets: mpsc::UnboundedReceiver<PeerSocket>,
    pub(crate) server: RustlsPeerServer,
    pub(crate) addr: SocketAddr,
}

impl Node {
    pub(crate) fn id(&self) -> DeviceId {
        self.tls.local.device_id
    }
}

pub(crate) async fn start_node(name: &str) -> Node {
    start_node_with(name, ServerOptions::local_test()).await
}

pub(crate) async fn start_node_with(name: &str, options: ServerOptions) -> Node {
    let tls = identity();
    let (handler, sockets) = FakeHandler::new(&tls, name);
    let server = RustlsPeerServer::with_options(Arc::clone(&tls), options);
    let as_handler: Arc<dyn PeerHandler> = handler.clone();
    let port = server.start(as_handler).await.expect("start the peer server");
    Node {
        tls,
        handler,
        sockets,
        server,
        addr: SocketAddr::from(([127, 0, 0, 1], port)),
    }
}

pub(crate) fn connector(tls: &Arc<TlsIdentity>) -> RustlsPeerConnector {
    RustlsPeerConnector::new(Arc::clone(tls))
}

/// The error of a call that must fail. Works for results whose value has no `Debug`.
pub(crate) fn err_of<T>(result: Result<T, PeerCallError>) -> PeerCallError {
    match result {
        Ok(_) => panic!("expected the call to fail"),
        Err(error) => error,
    }
}

pub(crate) async fn within<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("finished within 5 seconds")
}
```

- [ ] **Step 4: Write the failing server and client tests**

Create `crates/rocket-infra/src/network/server_tests.rs`:

```rust
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hyper::Method;
use rocket_network::limits::{
    DEFAULT_PORT, HEADER_READ_TIMEOUT, HELLO_PER_IP_PER_MINUTE, MAX_CONNECTIONS,
    MAX_REST_BODY_BYTES, MAX_UNPAIRED_CONNECTIONS, PAIR_REQUESTS_PER_MINUTE, PORT_RANGE_END,
    PROTOCOL_VERSION,
};
use rocket_network::{
    DecisionRequest, Nonce, NonceMessage, PairRequest, PairingId, PairingStatus, PeerCallError,
    PeerConnector, PeerHandler, PeerServerControl,
};

use super::{is_valid_pairing_id, FixedWindow, RustlsPeerServer, ServerOptions};
use crate::network::client::{send_request, tls_connect};
use crate::network::peer_test_support::{
    commitment_text, connector, err_of, identity, profile, reveal_nonce, start_node,
    start_node_with, FakeHandler,
};

fn pair_request() -> PairRequest {
    PairRequest {
        version: PROTOCOL_VERSION,
        profile: profile("Bob"),
    }
}

fn pairing(id: &str) -> PairingId {
    PairingId(id.to_string())
}

#[tokio::test]
async fn hello_returns_the_server_identity_and_profile() {
    let node = start_node("Alice").await;
    let client = connector(&identity());

    let (peer, hello) = client.hello(node.addr).await.expect("hello");

    assert_eq!(peer.device_id, node.id(), "the key comes from the handshake");
    assert_eq!(hello.device_id, node.id());
    assert_eq!(hello.version, PROTOCOL_VERSION);
    assert_eq!(hello.profile, profile("Alice"));
    assert_eq!(node.handler.calls(), vec!["hello".to_string()]);
}

#[tokio::test]
async fn a_stranger_reaches_hello_and_every_pairing_route() {
    let node = start_node("Alice").await;
    let stranger = identity();
    let client = connector(&stranger);
    let id = pairing("p-1");

    client.hello(node.addr).await.expect("hello");
    let (peer, started) = client
        .pair_request(node.addr, &node.id(), &pair_request())
        .await
        .expect("pair request");
    assert_eq!(peer.device_id, node.id());
    assert_eq!(started.pairing_id, id);
    assert_eq!(started.commitment, commitment_text());

    let na = NonceMessage {
        nonce: Nonce([1u8; 32]).to_base64(),
    };
    let reveal = client
        .pair_nonce(node.addr, &node.id(), &id, &na)
        .await
        .expect("nonce");
    assert_eq!(reveal.nonce, reveal_nonce());
    client
        .pair_decision(node.addr, &node.id(), &id, &DecisionRequest { accept: true })
        .await
        .expect("decision");
    let status = client
        .pair_status(node.addr, &node.id(), &id)
        .await
        .expect("status");
    assert_eq!(status.status, PairingStatus::Pending);

    let short = stranger.local.device_id.short();
    assert_eq!(
        node.handler.calls(),
        vec![
            "hello".to_string(),
            format!("pair_request:{short}:127.0.0.1:Bob"),
            format!("pair_nonce:{short}:p-1"),
            format!("pair_decision:{short}:p-1:true"),
            format!("pair_status:{short}:p-1"),
        ],
        "every route sees the caller's key and address"
    );
}

#[tokio::test]
async fn handler_refusals_map_to_their_status() {
    let node = start_node("Alice").await;
    let client = connector(&identity());

    let missing = client.pair_status(node.addr, &node.id(), &pairing("missing")).await;
    assert_eq!(err_of(missing), PeerCallError::Rejected(404));
    let not_paired = client.pair_status(node.addr, &node.id(), &pairing("not-paired")).await;
    assert_eq!(err_of(not_paired), PeerCallError::NotPaired);
    let busy = client.pair_status(node.addr, &node.id(), &pairing("busy")).await;
    assert_eq!(err_of(busy), PeerCallError::Busy);
    let forbidden = client.pair_status(node.addr, &node.id(), &pairing("forbidden")).await;
    assert_eq!(err_of(forbidden), PeerCallError::Rejected(403));
}

#[tokio::test]
async fn a_bad_pairing_id_is_refused_on_both_sides() {
    let node = start_node("Alice").await;
    let client_tls = identity();

    // The connector never sends an id that could change the path.
    let result = connector(&client_tls)
        .pair_status(node.addr, &node.id(), &pairing("a/b"))
        .await;
    assert!(matches!(err_of(result), PeerCallError::Protocol(_)));

    // The server refuses a raw request with an over-long id before the handler runs.
    let long = "a".repeat(65);
    let reply = send_request(
        &client_tls,
        node.addr,
        Some(node.id()),
        Method::GET,
        &format!("/pair/{long}"),
        None,
        &[],
    )
    .await
    .expect("reply");
    assert_eq!(reply.status, 400);
    assert!(node.handler.calls().is_empty());
}

#[tokio::test]
async fn an_invalid_profile_in_a_pair_request_is_400() {
    let node = start_node("Alice").await;
    let client_tls = identity();
    let body = br#"{"version":1,"profile":{"display_name":"bad\u0007name","initials":"B"}}"#.to_vec();

    let reply = send_request(
        &client_tls,
        node.addr,
        Some(node.id()),
        Method::POST,
        "/pair/request",
        Some(body),
        &[],
    )
    .await
    .expect("reply");

    assert_eq!(reply.status, 400);
    assert!(node.handler.calls().is_empty());
}

#[tokio::test]
async fn an_unknown_path_is_404() {
    let node = start_node("Alice").await;
    let client_tls = identity();

    for path in ["/", "/pair", "/hello/extra", "/collections", "/pair/p-1/other"] {
        let reply = send_request(
            &client_tls,
            node.addr,
            Some(node.id()),
            Method::GET,
            path,
            None,
            &[],
        )
        .await
        .expect("reply");
        assert_eq!(reply.status, 404, "{path}");
        assert_eq!(reply.body.as_ref(), b"not-found");
    }
    assert!(node.handler.calls().is_empty());
}

#[tokio::test]
async fn an_origin_header_is_refused_before_any_route() {
    let node = start_node("Alice").await;
    let client_tls = identity();
    let body = serde_json::to_vec(&pair_request()).expect("encode");

    let cases = [
        (Method::GET, "/hello", None),
        (Method::POST, "/pair/request", Some(body)),
        (Method::GET, "/nope", None),
    ];
    for (method, path, body) in cases {
        let reply = send_request(
            &client_tls,
            node.addr,
            Some(node.id()),
            method,
            path,
            body,
            &[("origin", "http://evil.example")],
        )
        .await
        .expect("reply");
        assert_eq!(reply.status, 403, "{path}");
        assert_eq!(reply.body.as_ref(), b"forbidden");
    }
    assert!(
        node.handler.calls().is_empty(),
        "no handler runs for a browser request"
    );
}

#[tokio::test]
async fn the_sixth_pairing_request_in_a_minute_is_busy() {
    let node = start_node("Hub").await;
    let first = connector(&identity());

    for i in 0..PAIR_REQUESTS_PER_MINUTE {
        if let Err(e) = first.pair_request(node.addr, &node.id(), &pair_request()).await {
            panic!("request {i} must pass: {e:?}");
        }
    }
    let sixth = first.pair_request(node.addr, &node.id(), &pair_request()).await;
    assert_eq!(err_of(sixth), PeerCallError::Busy);

    // The limit is shared by every stranger.
    let second = connector(&identity());
    let other = second.pair_request(node.addr, &node.id(), &pair_request()).await;
    assert_eq!(err_of(other), PeerCallError::Busy);
    assert_eq!(
        node.handler.calls().len(),
        PAIR_REQUESTS_PER_MINUTE as usize,
        "a busy request never reaches the handler, so it shows no notice"
    );
}

#[tokio::test]
async fn the_thirty_first_hello_from_one_address_is_429() {
    let node = start_node("Hub").await;
    let client = connector(&identity());

    for i in 0..HELLO_PER_IP_PER_MINUTE {
        if let Err(e) = client.hello(node.addr).await {
            panic!("hello {i} must pass: {e:?}");
        }
    }
    assert_eq!(
        err_of(client.hello(node.addr).await),
        PeerCallError::Rejected(429)
    );
    // The limit is per IP, not per key.
    assert_eq!(
        err_of(connector(&identity()).hello(node.addr).await),
        PeerCallError::Rejected(429)
    );
    // Pairing has its own limit.
    client
        .pair_request(node.addr, &node.id(), &pair_request())
        .await
        .expect("pairing is not limited by hello");
}

#[tokio::test]
async fn a_body_over_16_kib_is_413_and_the_limit_itself_passes() {
    let node = start_node("Hub").await;
    let client_tls = identity();

    let mut big = serde_json::to_vec(&pair_request()).expect("encode");
    big.resize(MAX_REST_BODY_BYTES + 1, b' ');
    let result = send_request(
        &client_tls,
        node.addr,
        Some(node.id()),
        Method::POST,
        "/pair/request",
        Some(big),
        &[],
    )
    .await;
    match result {
        Ok(reply) => assert_eq!(reply.status, 413),
        // The server may close before it reads the whole body. The reset can then reach the
        // client before the 413 does.
        Err(PeerCallError::Unreachable(_)) => {}
        Err(other) => panic!("unexpected error: {other:?}"),
    }
    assert!(node.handler.calls().is_empty());

    // Trailing spaces are valid JSON, so a body of exactly the limit is parsed and served.
    let mut exact = serde_json::to_vec(&pair_request()).expect("encode");
    exact.resize(MAX_REST_BODY_BYTES, b' ');
    let reply = send_request(
        &client_tls,
        node.addr,
        Some(node.id()),
        Method::POST,
        "/pair/request",
        Some(exact),
        &[],
    )
    .await
    .expect("reply");
    assert_eq!(reply.status, 200);
}

#[tokio::test]
async fn a_wrong_expected_id_is_an_identity_mismatch_and_sends_nothing() {
    let node = start_node("Hub").await;
    let someone_else = identity().local.device_id;
    let client = connector(&identity());

    let request = client.pair_request(node.addr, &someone_else, &pair_request()).await;
    assert_eq!(err_of(request), PeerCallError::IdentityMismatch);
    let status = client.pair_status(node.addr, &someone_else, &pairing("p-1")).await;
    assert_eq!(err_of(status), PeerCallError::IdentityMismatch);
    assert!(node.handler.calls().is_empty());
}

#[tokio::test]
async fn a_closed_port_is_unreachable() {
    let free = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = free.local_addr().expect("addr");
    drop(free);

    let result = connector(&identity()).hello(addr).await;
    assert!(matches!(err_of(result), PeerCallError::Unreachable(_)));
}

#[tokio::test]
async fn stop_closes_the_port_and_start_serves_again() {
    let node = start_node("Hub").await;
    node.server.stop();
    node.server.stop();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = connector(&identity());
    assert!(matches!(
        err_of(client.hello(node.addr).await),
        PeerCallError::Unreachable(_)
    ));
    let rebound = tokio::net::TcpListener::bind(node.addr).await;
    assert!(rebound.is_ok(), "the port is free after stop");
    drop(rebound);

    let as_handler: Arc<dyn PeerHandler> = node.handler.clone();
    let port = node.server.start(as_handler).await.expect("restart");
    let (peer, _) = client
        .hello(SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .expect("hello after the restart");
    assert_eq!(peer.device_id, node.id());
}

/// A held port whose next port is free, so a two-port range must use the second.
fn held_port_with_a_free_neighbour() -> (std::net::TcpListener, u16) {
    for _ in 0..50 {
        let held = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = held.local_addr().expect("addr").port();
        if port < u16::MAX && std::net::TcpListener::bind(("127.0.0.1", port + 1)).is_ok() {
            return (held, port);
        }
    }
    panic!("no free neighbouring port found");
}

#[tokio::test]
async fn a_taken_first_port_falls_back_to_the_next() {
    let (_held, taken) = held_port_with_a_free_neighbour();
    let options = ServerOptions {
        ports: taken..=taken + 1,
        ..ServerOptions::local_test()
    };

    let node = start_node_with("Hub", options).await;

    assert_eq!(node.addr.port(), taken + 1);
}

#[tokio::test]
async fn a_range_with_no_free_port_is_an_error() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let taken = held.local_addr().expect("addr").port();
    let tls = identity();
    let (handler, _sockets) = FakeHandler::new(&tls, "Hub");
    let server = RustlsPeerServer::with_options(
        Arc::clone(&tls),
        ServerOptions {
            ports: taken..=taken,
            ..ServerOptions::local_test()
        },
    );

    let as_handler: Arc<dyn PeerHandler> = handler;
    assert!(server.start(as_handler).await.is_err());
}

#[tokio::test]
async fn the_default_range_skips_a_taken_default_port() {
    // Either this test holds 47821 or another program already does. Both mean it is taken.
    let _held = tokio::net::TcpListener::bind(("127.0.0.1", DEFAULT_PORT)).await;
    let options = ServerOptions {
        ports: DEFAULT_PORT..=PORT_RANGE_END,
        ..ServerOptions::local_test()
    };

    let node = start_node_with("Hub", options).await;

    let port = node.addr.port();
    assert!(port > DEFAULT_PORT && port <= PORT_RANGE_END, "port {port}");
}

#[test]
fn production_options_use_the_spec_values() {
    let options = ServerOptions::default();
    assert_eq!(options.bind_ip, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    assert_eq!(options.ports, DEFAULT_PORT..=PORT_RANGE_END);
    assert_eq!(options.max_connections, MAX_CONNECTIONS);
    assert_eq!(options.max_unpaired, MAX_UNPAIRED_CONNECTIONS);
    assert_eq!(options.handshake_timeout, Duration::from_secs(10));
    assert_eq!(options.header_read_timeout, HEADER_READ_TIMEOUT);
    // The peer server and the callback server share one header timeout value.
    assert_eq!(HEADER_READ_TIMEOUT, crate::callback_server::HEADER_READ_TIMEOUT);
}

#[tokio::test]
async fn a_stalled_tls_handshake_frees_its_slot_after_the_timeout() {
    let options = ServerOptions {
        max_connections: 2,
        handshake_timeout: Duration::from_secs(1),
        ..ServerOptions::local_test()
    };
    let node = start_node_with("Hub", options).await;

    // Two clients connect and never start TLS.
    let _a = tokio::net::TcpStream::connect(node.addr).await.expect("connect a");
    let _b = tokio::net::TcpStream::connect(node.addr).await.expect("connect b");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = connector(&identity());
    assert!(
        matches!(err_of(client.hello(node.addr).await), PeerCallError::Unreachable(_)),
        "both slots are held, so a third connection is dropped"
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    client
        .hello(node.addr)
        .await
        .expect("the stalled handshakes timed out and freed their slots");
}

#[tokio::test]
async fn a_connection_that_never_sends_headers_frees_its_slot_after_the_timeout() {
    let options = ServerOptions {
        max_connections: 1,
        header_read_timeout: Duration::from_secs(1),
        ..ServerOptions::local_test()
    };
    let node = start_node_with("Hub", options).await;

    // The TLS handshake finishes, then the client sends nothing.
    let (_silent, _) = tls_connect(&identity(), node.addr, Some(node.id()))
        .await
        .expect("handshake");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = connector(&identity());
    assert!(
        matches!(err_of(client.hello(node.addr).await), PeerCallError::Unreachable(_)),
        "the only slot is held by the silent connection"
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    client
        .hello(node.addr)
        .await
        .expect("the header read timeout closed the silent connection and freed its slot");
}

#[tokio::test]
async fn unpaired_connections_stop_at_32_but_paired_devices_still_connect() {
    let node = start_node("Hub").await;
    let friend = identity();
    node.handler.pair(friend.local.device_id);
    let stranger = identity();

    let mut held = Vec::new();
    for i in 0..MAX_UNPAIRED_CONNECTIONS {
        match tls_connect(&stranger, node.addr, Some(node.id())).await {
            Ok((tls, _)) => held.push(tls),
            Err(e) => panic!("stranger connection {i}: {e:?}"),
        }
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    let extra = connector(&identity()).hello(node.addr).await;
    assert!(
        matches!(err_of(extra), PeerCallError::Unreachable(_)),
        "the 33rd unpaired connection is dropped after the handshake"
    );
    connector(&friend)
        .hello(node.addr)
        .await
        .expect("a paired device still gets in");

    held.pop();
    tokio::time::sleep(Duration::from_millis(300)).await;
    connector(&identity())
        .hello(node.addr)
        .await
        .expect("a freed unpaired slot serves again");
}

#[test]
fn fixed_window_allows_the_limit_then_refuses_until_the_window_passes() {
    let start = Instant::now();
    let mut window = FixedWindow::new(3, Duration::from_secs(60), 16);

    for _ in 0..3 {
        assert!(window.allow("a", start));
    }
    assert!(!window.allow("a", start + Duration::from_secs(59)));
    assert!(window.allow("b", start), "keys are counted apart");
    assert!(
        window.allow("a", start + Duration::from_secs(60)),
        "a new window starts"
    );
}

#[test]
fn fixed_window_keeps_at_most_max_keys_and_fails_closed() {
    let start = Instant::now();
    let mut window = FixedWindow::new(1, Duration::from_secs(60), 2);

    assert!(window.allow(1, start));
    assert!(window.allow(2, start));
    assert!(!window.allow(3, start), "a full table refuses new keys");
    assert!(
        window.allow(3, start + Duration::from_secs(61)),
        "expired keys are pruned"
    );
    assert!(window.len() <= 2);
}

#[test]
fn pairing_ids_are_limited_to_safe_characters() {
    assert!(is_valid_pairing_id("0f8fad5b-d9cb-469f-a165-70867728950e"));
    let long = "a".repeat(65);
    for bad in ["", "a/b", "..", "a b", "%2e", "p?x=1", long.as_str()] {
        assert!(!is_valid_pairing_id(bad), "{bad:?}");
    }
}
```

- [ ] **Step 5: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-infra network::server`
Expected: FAIL to compile with `unresolved imports super::is_valid_pairing_id, super::FixedWindow, super::RustlsPeerServer, super::ServerOptions`, `unresolved imports crate::network::client::send_request, crate::network::client::tls_connect, crate::network::client::RustlsPeerConnector` and, from `mod.rs`, `unresolved import client::RustlsPeerConnector` and `unresolved import server::RustlsPeerServer`.

- [ ] **Step 6: Implement the server**

Replace the whole content of `crates/rocket-infra/src/network/server.rs` with:

```rust
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

#[cfg(test)]
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
                return Ok::<Response, Infallible>(text(StatusCode::FORBIDDEN, "forbidden"));
            }
            req.extensions_mut().insert(info);
            match tokio::time::timeout(REQUEST_TIMEOUT, router.call(req)).await {
                Ok(response) => response,
                Err(_) => Ok(text(StatusCode::REQUEST_TIMEOUT, "timeout")),
            }
        }
    });
    // One request per connection. Upgrades stay on for `/peer`.
    let connection = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(shared.options.header_read_timeout)
        .keep_alive(false)
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
    match state.handler.pair_request(conn.peer, conn.remote_ip, req).await {
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

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
```

- [ ] **Step 7: Implement the REST connector**

Replace the whole content of `crates/rocket-infra/src/network/client.rs` with:

```rust
//! The peer client. Every REST call opens its own TLS 1.3 connection with our device
//! certificate, sends one HTTP/1.1 request and closes.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::header::{CONTENT_TYPE, HOST};
use hyper::{Method, Request};
use hyper_util::rt::TokioIo;
use rocket_network::limits::MAX_REST_BODY_BYTES;
use rocket_network::{
    commitment_from_base64, DecisionRequest, DeviceId, HelloResponse, NodeProfile, Nonce,
    NonceMessage, PairRequest, PairRequestResponse, PairStatusResponse, PairingId, PeerCallError,
    PeerConnector, PeerIdentity, PeerMessage, PeerSocket,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{CertificateError, Error as TlsError};
use tokio_rustls::TlsConnector;

use crate::network::server::is_valid_pairing_id;
use crate::network::{client_config, peer_identity_from_certs, TlsIdentity};

/// TCP connect plus the TLS handshake.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// One request and its reply, or one socket upgrade with the Hello exchange. The server
/// imports this constant for its per-request timeout, so it is declared only here.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) type ClientTls = TlsStream<TcpStream>;

/// A reply as it came off the wire. The tests use it to check raw statuses.
pub(crate) struct RawReply {
    pub(crate) peer: PeerIdentity,
    pub(crate) status: u16,
    pub(crate) body: Bytes,
}

pub struct RustlsPeerConnector {
    identity: Arc<TlsIdentity>,
}

impl RustlsPeerConnector {
    pub fn new(identity: Arc<TlsIdentity>) -> Self {
        RustlsPeerConnector { identity }
    }

    async fn call_json<T: DeserializeOwned>(
        &self,
        addr: SocketAddr,
        expected: Option<DeviceId>,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<(PeerIdentity, T), PeerCallError> {
        let reply = send_request(&self.identity, addr, expected, method, path, body, &[]).await?;
        check_status(&reply)?;
        let value = serde_json::from_slice(&reply.body)
            .map_err(|_| PeerCallError::Protocol("the peer sent an unreadable reply".to_string()))?;
        Ok((reply.peer, value))
    }
}

#[async_trait]
impl PeerConnector for RustlsPeerConnector {
    async fn hello(&self, addr: SocketAddr) -> Result<(PeerIdentity, HelloResponse), PeerCallError> {
        // No expected id: this call is how a node's key is learned.
        let (peer, mut hello): (PeerIdentity, HelloResponse) = self
            .call_json(addr, None, Method::GET, "/hello", None)
            .await?;
        // The certificate decides. A body that claims another id is refused.
        if hello.device_id != peer.device_id {
            return Err(PeerCallError::IdentityMismatch);
        }
        hello.profile = validated_profile(&hello.profile)?;
        Ok((peer, hello))
    }

    async fn pair_request(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        req: &PairRequest,
    ) -> Result<(PeerIdentity, PairRequestResponse), PeerCallError> {
        let body = json_body(req)?;
        let (peer, reply): (PeerIdentity, PairRequestResponse) = self
            .call_json(addr, Some(*expected), Method::POST, "/pair/request", Some(body))
            .await?;
        if !is_valid_pairing_id(&reply.pairing_id.0)
            || commitment_from_base64(&reply.commitment).is_err()
        {
            return Err(PeerCallError::Protocol(
                "the peer sent a malformed pairing reply".to_string(),
            ));
        }
        Ok((peer, reply))
    }

    async fn pair_nonce(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        msg: &NonceMessage,
    ) -> Result<NonceMessage, PeerCallError> {
        let path = pairing_path(id, "/nonce")?;
        let (_, reply): (PeerIdentity, NonceMessage) = self
            .call_json(addr, Some(*expected), Method::POST, &path, Some(json_body(msg)?))
            .await?;
        if Nonce::parse_base64(&reply.nonce).is_err() {
            return Err(PeerCallError::Protocol(
                "the peer sent a malformed nonce".to_string(),
            ));
        }
        Ok(reply)
    }

    async fn pair_decision(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        req: &DecisionRequest,
    ) -> Result<(), PeerCallError> {
        let path = pairing_path(id, "/decision")?;
        let reply = send_request(
            &self.identity,
            addr,
            Some(*expected),
            Method::POST,
            &path,
            Some(json_body(req)?),
            &[],
        )
        .await?;
        check_status(&reply)
    }

    async fn pair_status(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
    ) -> Result<PairStatusResponse, PeerCallError> {
        let path = pairing_path(id, "")?;
        let (_, reply): (PeerIdentity, PairStatusResponse) = self
            .call_json(addr, Some(*expected), Method::GET, &path, None)
            .await?;
        Ok(reply)
    }

    async fn open_socket(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        hello: PeerMessage,
    ) -> Result<PeerSocket, PeerCallError> {
        // Task 2 of this plan replaces this body with the WebSocket client.
        let _ = (addr, expected, hello);
        Err(PeerCallError::Protocol(
            "peer sockets are not available yet".to_string(),
        ))
    }
}

fn validated_profile(profile: &NodeProfile) -> Result<NodeProfile, PeerCallError> {
    profile
        .validated()
        .map_err(|_| PeerCallError::Protocol("the peer sent an invalid profile".to_string()))
}

fn json_body<T: Serialize>(value: &T) -> Result<Vec<u8>, PeerCallError> {
    serde_json::to_vec(value)
        .map_err(|e| PeerCallError::Protocol(format!("encode the request: {e}")))
}

fn pairing_path(id: &PairingId, suffix: &str) -> Result<String, PeerCallError> {
    if !is_valid_pairing_id(&id.0) {
        return Err(PeerCallError::Protocol("invalid pairing id".to_string()));
    }
    Ok(format!("/pair/{}{suffix}", id.0))
}

/// Maps a reply status to the call result.
pub(crate) fn check_status(reply: &RawReply) -> Result<(), PeerCallError> {
    match reply.status {
        200..=299 => Ok(()),
        403 if reply.body.as_ref() == b"not-paired" => Err(PeerCallError::NotPaired),
        503 => Err(PeerCallError::Busy),
        status => Err(PeerCallError::Rejected(status)),
    }
}

/// Opens TCP and TLS with our certificate. With `expected`, Plan 02's verifier refuses any
/// other key inside the handshake, before a byte of the request is sent.
pub(crate) async fn tls_connect(
    identity: &TlsIdentity,
    addr: SocketAddr,
    expected: Option<DeviceId>,
) -> Result<(ClientTls, PeerIdentity), PeerCallError> {
    let config = client_config(identity, expected)
        .map_err(|_| PeerCallError::Protocol("the TLS client could not be built".to_string()))?;
    let connector = TlsConnector::from(config);
    let attempt = async {
        let tcp = TcpStream::connect(addr)
            .await
            .map_err(|e| PeerCallError::Unreachable(e.to_string()))?;
        let _ = tcp.set_nodelay(true);
        // The verifier ignores the name. An IP name sends no SNI.
        connector
            .connect(ServerName::from(addr.ip()), tcp)
            .await
            .map_err(map_tls_error)
    };
    let tls = tokio::time::timeout(CONNECT_TIMEOUT, attempt)
        .await
        .map_err(|_| PeerCallError::Unreachable("the connection timed out".to_string()))??;
    let peer = peer_identity_from_certs(tls.get_ref().1.peer_certificates())
        .map_err(|_| PeerCallError::IdentityMismatch)?;
    // A second check, so a verifier change can never let another key through.
    if expected.is_some_and(|id| id != peer.device_id) {
        return Err(PeerCallError::IdentityMismatch);
    }
    Ok((tls, peer))
}

fn map_tls_error(error: std::io::Error) -> PeerCallError {
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<TlsError>())
    {
        // The pinned key did not match: another node answers on this address.
        Some(TlsError::InvalidCertificate(CertificateError::ApplicationVerificationFailure)) => {
            PeerCallError::IdentityMismatch
        }
        Some(_) => PeerCallError::Protocol("the TLS handshake failed".to_string()),
        None => PeerCallError::Unreachable(error.to_string()),
    }
}

/// Sends one request on a new connection and reads the whole reply (at most 16 KiB).
pub(crate) async fn send_request(
    identity: &TlsIdentity,
    addr: SocketAddr,
    expected: Option<DeviceId>,
    method: Method,
    path: &str,
    body: Option<Vec<u8>>,
    headers: &[(&str, &str)],
) -> Result<RawReply, PeerCallError> {
    let (tls, peer) = tls_connect(identity, addr, expected).await?;
    let exchange = async {
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake::<_, Full<Bytes>>(TokioIo::new(tls))
                .await
                .map_err(|e| PeerCallError::Unreachable(e.to_string()))?;
        // The connection future drives the socket. It ends when the server closes.
        let driver = tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(HOST, addr.to_string());
        if body.is_some() {
            builder = builder.header(CONTENT_TYPE, "application/json");
        }
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder
            .body(Full::new(Bytes::from(body.unwrap_or_default())))
            .map_err(|e| PeerCallError::Protocol(e.to_string()))?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|e| PeerCallError::Unreachable(e.to_string()))?;
        let status = response.status().as_u16();
        let body = Limited::new(response.into_body(), MAX_REST_BODY_BYTES)
            .collect()
            .await
            .map_err(|_| PeerCallError::Protocol("the peer reply could not be read".to_string()))?
            .to_bytes();
        driver.abort();
        Ok(RawReply { peer, status, body })
    };
    tokio::time::timeout(REQUEST_TIMEOUT, exchange)
        .await
        .map_err(|_| PeerCallError::Unreachable("the request timed out".to_string()))?
}
```

- [ ] **Step 8: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-infra network::server`
Expected: PASS, 23 tests in `network::server::tests`. If `a_connection_that_never_sends_headers_frees_its_slot_after_the_timeout` fails, check that the hyper builder gets both `.timer(TokioTimer::new())` and `.header_read_timeout(options.header_read_timeout)`; hyper only starts that timer when a timer is set.

If `a_wrong_expected_id_is_an_identity_mismatch_and_sends_nothing` reports `Protocol(..)` instead of `IdentityMismatch`, Plan 02's verifier returns another rustls error for a key mismatch. Match that variant in `map_tls_error`; do not weaken the test.

- [ ] **Step 9: Check**

Run: `cargo check -j4 -p rocket-infra`
Expected: PASS. Two `field is never read` warnings (`ConnInfo::slot`, `ConnInfo::stop`) are expected until Task 2 reads both.

No commit; reviewed at the plan end.

---

### Task 2: Peer WebSocket (`/peer`, Hello exchange, pump, client socket)

**Files:**
- Modify: `crates/rocket-infra/src/network/mod.rs`, `crates/rocket-infra/src/network/server.rs`, `crates/rocket-infra/src/network/client.rs`
- Create: `crates/rocket-infra/src/network/socket.rs`, `crates/rocket-infra/src/network/socket_tests.rs`

**Interfaces:**
- Consumes: Task 1 (`RouteState`, `ConnInfo`, `ConnSlot`, `text`, `tls_connect`, `REQUEST_TIMEOUT`, `ClientTls`, `send_request`, test support); `rocket_network::{limits::MAX_FRAME_BYTES, PeerMessage, PeerSocket, PeerHandler, PeerCallError, DeviceId}`.
- Produces:

```rust
// socket.rs (crate-private)
pub(crate) enum FrameKind { Text(String), Control, Other }
pub(crate) trait WsFrame: Send + Sized + 'static {
    fn kind(self) -> FrameKind;
    fn text(body: String) -> Self;
    fn close() -> Self;
}
impl WsFrame for axum::extract::ws::Message;
impl WsFrame for tokio_tungstenite::tungstenite::Message;
pub(crate) async fn peer_upgrade(State<RouteState>, Extension<ConnInfo>, Result<WebSocketUpgrade, WebSocketUpgradeRejection>) -> Response;
pub(crate) async fn exchange_hello<W, M, E>(ws: &mut W, local: &PeerMessage) -> Result<PeerMessage, String>;
pub(crate) fn spawn_pump<W, M, E>(ws: W, stop: Option<watch::Receiver<()>>, slot: Option<Arc<ConnSlot>>) -> (mpsc::Sender<PeerMessage>, mpsc::Receiver<PeerMessage>);
pub(crate) async fn open_client_socket(identity: &TlsIdentity, addr: SocketAddr, expected: &DeviceId, hello: PeerMessage) -> Result<PeerSocket, PeerCallError>;
```

Socket rules:
- Server: `GET /peer` from a key that `is_paired` does not accept gets `403 not-paired` before any upgrade header is checked. A paired key then needs valid upgrade headers (`400`/`405`/`426` from axum otherwise).
- Both sides send their own Hello first, then wait at most 10 s (`HELLO_TIMEOUT`) for the peer's. WebSocket ping and pong control frames are skipped while waiting. Any other first frame closes the socket and nothing reaches the handler.
- After the Hello: text frames decode with `PeerMessage::decode`. A decode error, an unknown type, a second Hello, a binary frame, a close frame or a frame over 64 KiB (tungstenite's `max_frame_size`/`max_message_size`) ends the pump. Ending the pump sends a close frame (at most 3 s) and drops both channel ends, so `inbound` yields `None` and `outbound.send` fails.
- Dropping the `outbound` sender closes the socket. Dropping the `inbound` receiver closes it at the next received frame. Stopping the server closes every server-side socket.
- `dialed_by` is the remote key on the server side and the local key on the client side. Version checks and duplicate resolution are Plan 04's (it reads `remote_hello`).
- Pump design: one generic pump for both libraries. axum's `WebSocket` and tungstenite's `WebSocketStream` both implement `Stream<Item = Result<Message, E>> + Sink<Message>`, only with different `Message` types. `WsFrame` maps each message type to `FrameKind` and builds text and close frames, so the frame rules exist once and are tested once from either side. Two thin pumps would duplicate the security rules; a channel adapter per side would add a task and a buffer per socket.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 1: Register the module and a stub**

In `crates/rocket-infra/src/network/mod.rs`, replace

```rust
pub mod server;
```

with

```rust
pub mod server;
pub(crate) mod socket;
```

Create `crates/rocket-infra/src/network/socket.rs` with only:

```rust
//! The peer WebSocket. Both sides send `Hello` first, then a pump task moves `PeerMessage`s
//! between the socket and two channels.

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;
```

- [ ] **Step 2: Write the failing socket tests**

Create `crates/rocket-infra/src/network/socket_tests.rs`:

```rust
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hyper::Method;
use rocket_network::limits::MAX_FRAME_BYTES;
use rocket_network::{PeerCallError, PeerConnector, PeerMessage, PeerServerControl, PeerSocket};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::network::client::{send_request, tls_connect, ClientTls};
use crate::network::peer_test_support::{
    connector, err_of, hello_for, identity, start_node, start_node_with, within, Node,
};
use crate::network::server::ServerOptions;
use crate::network::TlsIdentity;

async fn paired_hub() -> (Node, Arc<TlsIdentity>) {
    let node = start_node("Hub").await;
    let mate = identity();
    node.handler.pair(mate.local.device_id);
    (node, mate)
}

async fn open(node: &Node, mate: &Arc<TlsIdentity>, name: &str) -> PeerSocket {
    match connector(mate)
        .open_socket(node.addr, &node.id(), hello_for(name))
        .await
    {
        Ok(socket) => socket,
        Err(e) => panic!("open the socket: {e:?}"),
    }
}

async fn server_socket(node: &mut Node) -> PeerSocket {
    within(node.sockets.recv())
        .await
        .expect("the server hands over its socket")
}

/// A raw client that can send frames the real pump never sends.
async fn raw_socket(node: &Node, mate: &TlsIdentity) -> WebSocketStream<ClientTls> {
    let (tls, _) = tls_connect(mate, node.addr, Some(node.id()))
        .await
        .expect("tls");
    let (ws, _) = tokio_tungstenite::client_async(format!("wss://{}/peer", node.addr), tls)
        .await
        .expect("upgrade");
    ws
}

async fn raw_hello(ws: &mut WebSocketStream<ClientTls>) -> PeerMessage {
    ws.send(Message::text(hello_for("Raw").encode()))
        .await
        .expect("send hello");
    let frame = within(ws.next())
        .await
        .expect("a frame")
        .expect("a good frame");
    PeerMessage::decode(frame.to_text().expect("a text frame")).expect("a peer message")
}

async fn assert_raw_closed(ws: &mut WebSocketStream<ClientTls>) {
    loop {
        match within(ws.next()).await {
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
            Some(Ok(_)) => continue,
        }
    }
}

/// A paired raw client after both Hello frames, plus the server's side of the socket.
async fn raw_paired() -> (Node, WebSocketStream<ClientTls>, PeerSocket) {
    let (mut node, mate) = paired_hub().await;
    let mut ws = raw_socket(&node, &mate).await;
    assert_eq!(raw_hello(&mut ws).await, hello_for("Hub"));
    let accepted = server_socket(&mut node).await;
    assert_eq!(accepted.remote_hello, hello_for("Raw"));
    (node, ws, accepted)
}

#[tokio::test]
async fn paired_sockets_exchange_hello_frames_both_ways() {
    let (mut node, mate) = paired_hub().await;

    let socket = open(&node, &mate, "Bob").await;
    assert_eq!(socket.peer.device_id, node.id());
    assert_eq!(socket.dialed_by, mate.local.device_id, "we dialled");
    assert_eq!(socket.remote_hello, hello_for("Hub"));

    let accepted = server_socket(&mut node).await;
    assert_eq!(accepted.peer.device_id, mate.local.device_id);
    assert_eq!(accepted.dialed_by, mate.local.device_id, "the remote side dialled");
    assert_eq!(accepted.remote_hello, hello_for("Bob"));
}

#[tokio::test]
async fn ping_and_pong_travel_through_the_channels() {
    let (mut node, mate) = paired_hub().await;
    let mut socket = open(&node, &mate, "Bob").await;
    let mut accepted = server_socket(&mut node).await;

    socket
        .outbound
        .send(PeerMessage::Ping)
        .await
        .expect("send ping");
    assert_eq!(within(accepted.inbound.recv()).await, Some(PeerMessage::Ping));
    accepted
        .outbound
        .send(PeerMessage::Pong)
        .await
        .expect("send pong");
    assert_eq!(within(socket.inbound.recv()).await, Some(PeerMessage::Pong));
}

#[tokio::test]
async fn a_stranger_gets_403_not_paired_before_the_upgrade() {
    let mut node = start_node("Hub").await;
    let stranger = identity();

    let result = connector(&stranger)
        .open_socket(node.addr, &node.id(), hello_for("Eve"))
        .await;
    assert_eq!(err_of(result), PeerCallError::NotPaired);

    // A plain GET shows the body: the check runs before any upgrade header is looked at.
    let reply = send_request(&stranger, node.addr, Some(node.id()), Method::GET, "/peer", None, &[])
        .await
        .expect("reply");
    assert_eq!(reply.status, 403);
    assert_eq!(reply.body.as_ref(), b"not-paired");
    assert!(node.sockets.try_recv().is_err(), "no socket reaches the handler");
}

#[tokio::test]
async fn a_paired_get_without_upgrade_headers_is_400() {
    let (node, mate) = paired_hub().await;

    let reply = send_request(&mate, node.addr, Some(node.id()), Method::GET, "/peer", None, &[])
        .await
        .expect("reply");

    assert_eq!(reply.status, 400);
}

#[tokio::test]
async fn an_origin_header_on_peer_is_refused_even_for_a_paired_key() {
    let (mut node, mate) = paired_hub().await;

    let reply = send_request(
        &mate,
        node.addr,
        Some(node.id()),
        Method::GET,
        "/peer",
        None,
        &[("origin", "http://evil.example")],
    )
    .await
    .expect("reply");

    // The Origin check runs before the paired check and before the upgrade.
    assert_eq!(reply.status, 403);
    assert_eq!(reply.body.as_ref(), b"forbidden");
    assert!(node.sockets.try_recv().is_err(), "no socket reaches the handler");
}

#[tokio::test]
async fn a_frame_at_the_limit_is_read() {
    let (_node, mut ws, mut accepted) = raw_paired().await;
    // Trailing spaces are valid JSON, so this ping is exactly 64 KiB.
    let mut frame = PeerMessage::Ping.encode();
    frame.push_str(&" ".repeat(MAX_FRAME_BYTES - frame.len()));

    ws.send(Message::text(frame)).await.expect("send");

    assert_eq!(within(accepted.inbound.recv()).await, Some(PeerMessage::Ping));
}

#[tokio::test]
async fn a_frame_over_64_kib_closes_the_socket() {
    let (_node, mut ws, mut accepted) = raw_paired().await;

    let _ = ws.send(Message::text("x".repeat(MAX_FRAME_BYTES + 1))).await;

    assert_eq!(within(accepted.inbound.recv()).await, None);
    assert_raw_closed(&mut ws).await;
}

#[tokio::test]
async fn a_binary_frame_closes_the_socket() {
    let (_node, mut ws, mut accepted) = raw_paired().await;

    let _ = ws.send(Message::binary(vec![1u8, 2, 3])).await;

    assert_eq!(within(accepted.inbound.recv()).await, None);
    assert_raw_closed(&mut ws).await;
}

#[tokio::test]
async fn an_unknown_message_type_closes_the_socket() {
    let (_node, mut ws, mut accepted) = raw_paired().await;

    let _ = ws
        .send(Message::text(r#"{"type":"send_collection"}"#.to_string()))
        .await;

    assert_eq!(within(accepted.inbound.recv()).await, None);
    assert_raw_closed(&mut ws).await;
}

#[tokio::test]
async fn a_second_hello_closes_the_socket() {
    let (_node, mut ws, mut accepted) = raw_paired().await;

    let _ = ws.send(Message::text(hello_for("Raw").encode())).await;

    assert_eq!(within(accepted.inbound.recv()).await, None);
    assert_raw_closed(&mut ws).await;
}

#[tokio::test]
async fn a_frame_before_hello_closes_without_a_socket() {
    let (mut node, mate) = paired_hub().await;
    let mut ws = raw_socket(&node, &mate).await;

    ws.send(Message::text(PeerMessage::Ping.encode()))
        .await
        .expect("send a ping first");

    // The server sends its own Hello, then closes.
    assert_raw_closed(&mut ws).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        node.sockets.try_recv().is_err(),
        "the handler never sees this socket"
    );
}

#[tokio::test]
async fn dropping_the_outbound_sender_closes_the_peer_inbound() {
    let (mut node, mate) = paired_hub().await;
    let socket = open(&node, &mate, "Bob").await;
    let mut accepted = server_socket(&mut node).await;

    let PeerSocket {
        outbound,
        mut inbound,
        ..
    } = socket;
    drop(outbound);

    assert_eq!(
        within(accepted.inbound.recv()).await,
        None,
        "the hub sees the socket close"
    );
    assert_eq!(within(inbound.recv()).await, None, "our own inbound closes too");
}

#[tokio::test]
async fn the_same_device_dialling_twice_gets_two_sockets() {
    let (mut node, mate) = paired_hub().await;
    let first = open(&node, &mate, "Bob").await;
    let second = open(&node, &mate, "Bob").await;
    let mut a = server_socket(&mut node).await;
    let mut b = server_socket(&mut node).await;

    for accepted in [&a, &b] {
        assert_eq!(accepted.peer.device_id, mate.local.device_id);
        assert_eq!(accepted.dialed_by, mate.local.device_id);
    }
    // Both work. Keeping one is Plan 04's rule (`keep_dialed_by`).
    first.outbound.send(PeerMessage::Ping).await.expect("ping 1");
    second.outbound.send(PeerMessage::Ping).await.expect("ping 2");
    assert_eq!(within(a.inbound.recv()).await, Some(PeerMessage::Ping));
    assert_eq!(within(b.inbound.recv()).await, Some(PeerMessage::Ping));
}

#[tokio::test]
async fn stop_closes_open_sockets() {
    let (mut node, mate) = paired_hub().await;
    let mut socket = open(&node, &mate, "Bob").await;
    let _accepted = server_socket(&mut node).await;

    node.server.stop();

    assert_eq!(within(socket.inbound.recv()).await, None);
}

#[tokio::test]
async fn an_open_socket_keeps_its_connection_slot() {
    let options = ServerOptions {
        max_connections: 2,
        ..ServerOptions::local_test()
    };
    let mut node = start_node_with("Hub", options).await;
    let mate = identity();
    node.handler.pair(mate.local.device_id);
    let first = open(&node, &mate, "Bob").await;
    let _second = open(&node, &mate, "Bob").await;
    let _a = server_socket(&mut node).await;
    let _b = server_socket(&mut node).await;

    // The HTTP connections of both upgrades are over, but each socket still holds its slot.
    let blocked = connector(&mate).hello(node.addr).await;
    assert!(matches!(err_of(blocked), PeerCallError::Unreachable(_)));

    drop(first);
    tokio::time::sleep(Duration::from_millis(300)).await;
    connector(&mate)
        .hello(node.addr)
        .await
        .expect("a closed socket frees its slot");
}

#[tokio::test]
async fn a_hub_holds_thirty_paired_sockets_at_once() {
    let mut node = start_node("Hub").await;
    let server_id = node.id();
    let mut mates = Vec::new();
    for _ in 0..30 {
        let mate = identity();
        node.handler.pair(mate.local.device_id);
        mates.push(mate);
    }

    let mut opening = JoinSet::new();
    for (i, mate) in mates.iter().enumerate() {
        let mate = Arc::clone(mate);
        let addr = node.addr;
        opening.spawn(async move {
            connector(&mate)
                .open_socket(addr, &server_id, hello_for(&format!("Mate {i}")))
                .await
        });
    }
    let mut sockets = Vec::new();
    while let Some(joined) = opening.join_next().await {
        match joined.expect("the task ran") {
            Ok(socket) => sockets.push(socket),
            Err(e) => panic!("a mate could not open its socket: {e:?}"),
        }
    }
    assert_eq!(sockets.len(), 30);

    let mut accepted = Vec::new();
    for _ in 0..30 {
        accepted.push(server_socket(&mut node).await);
    }
    for socket in &sockets {
        socket.outbound.send(PeerMessage::Ping).await.expect("ping");
    }
    for socket in &mut accepted {
        assert_eq!(within(socket.inbound.recv()).await, Some(PeerMessage::Ping));
    }
    let mut ids: Vec<_> = accepted.iter().map(|s| s.peer.device_id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 30, "one socket per teammate, all online at once");
}
```

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-infra network::socket`
Expected: FAIL. It compiles; every test that opens a socket fails with `open the socket: Protocol("peer sockets are not available yet")` or a raw upgrade that gets `404` (`/peer` is not routed yet), for example `a_stranger_gets_403_not_paired_before_the_upgrade` with `left: Protocol("peer sockets are not available yet"), right: NotPaired`.

- [ ] **Step 4: Implement the socket module**

Replace the whole content of `crates/rocket-infra/src/network/socket.rs` with:

```rust
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
    let (outbound, inbound) =
        spawn_pump(socket, Some(conn.stop.clone()), Some(Arc::clone(&conn.slot)));
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
                            if inbound.send(message).await.is_err() {
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
                if sink.send(M::text(message.encode())).await.is_err() {
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
```

- [ ] **Step 5: Route `/peer` and finish `open_socket`**

In `crates/rocket-infra/src/network/server.rs`, replace

```rust
        .route("/pair/{id}", get(pair_status))
```

with

```rust
        .route("/pair/{id}", get(pair_status))
        .route("/peer", get(crate::network::socket::peer_upgrade))
```

In `crates/rocket-infra/src/network/client.rs`, replace

```rust
        // Task 2 of this plan replaces this body with the WebSocket client.
        let _ = (addr, expected, hello);
        Err(PeerCallError::Protocol(
            "peer sockets are not available yet".to_string(),
        ))
```

with

```rust
        crate::network::socket::open_client_socket(&self.identity, addr, expected, hello).await
```

- [ ] **Step 6: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-infra network::socket`
Expected: PASS, 16 tests in `network::socket::tests`.

Run: `cargo test -j4 -p rocket-infra network::server`
Expected: PASS, 23 tests (`an_unknown_path_is_404` does not list `/peer`, so routing it changes nothing there).

- [ ] **Step 7: Check**

Run: `cargo check -j4 -p rocket-infra`
Expected: PASS with no warnings from `crates/rocket-infra/src/network/` (Task 1's two `never read` warnings are gone).

No commit; reviewed at the plan end.

---

### Task 3: mDNS discovery and the transport factory

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`, `crates/rocket-infra/src/network/mod.rs`, `crates/rocket-infra/src/lib.rs`, and for the Plan 04b test seams (Step 5b) `crates/rocket-infra/src/network/server.rs` and `crates/rocket-infra/src/network/tls_identity.rs`
- Create: `crates/rocket-infra/src/network/mdns.rs`, `crates/rocket-infra/src/network/transport.rs`
- Test: inline `#[cfg(test)] mod tests` in both new files.

**Interfaces:**
- Consumes: `rocket_network::{limits::MDNS_SERVICE_TYPE, Announcement, DeviceId, Discovery, DiscoveryEvent, DiscoverySink, NodeProfile, DeviceKeyMaterial, PeerTransport, PeerTransportFactory}`; Tasks 1 and 2; `crate::network::build_tls_identity`.
- Produces (Plan 04 wiring):

```rust
// mdns.rs
pub struct MdnsDiscovery { /* inner, own */ }
impl MdnsDiscovery { pub fn new() -> Self; }             // never fails; the daemon starts on first use
impl Default for MdnsDiscovery;
impl Discovery for MdnsDiscovery { /* start, announce, withdraw, stop */ }
pub(crate) fn instance_name(id: &DeviceId) -> String;    // id.short(), e.g. "K7QF-2M9X"
pub(crate) fn host_name(id: &DeviceId) -> String;        // "rocket-k7qf2m9x.local."
pub(crate) fn txt_record(a: &Announcement) -> Vec<(&'static str, String)>; // v, id, name, initials
pub(crate) fn parse_txt(lookup: &dyn Fn(&str) -> Option<String>) -> Option<(DeviceId, NodeProfile, u32)>;
pub(crate) fn dialable_addresses(ips: &[IpAddr], port: u16) -> Vec<SocketAddr>;
#[derive(Default)] pub(crate) struct BrowseState;        // resolved(..) -> Option<DiscoveryEvent>, removed(..) -> Option<DiscoveryEvent>

// transport.rs
#[derive(Debug, Default, Clone, Copy)]
pub struct RustlsPeerTransportFactory { /* local_test: bool, only with test or `test-support` */ }
impl RustlsPeerTransportFactory { pub fn new() -> Self; }
#[cfg(any(test, feature = "test-support"))]
impl RustlsPeerTransportFactory { pub fn local_test() -> Self; } // servers on 127.0.0.1 with an OS port (Plan 04b)

// Test seams opened in Step 5b for Plan 04b (only with test or the `test-support` feature):
// RustlsPeerServer::local_test(identity: Arc<TlsIdentity>) -> RustlsPeerServer
// rocket_infra::network::tls_identity::test_support::{key_material, identity} become `pub`
impl PeerTransportFactory for RustlsPeerTransportFactory {
    fn build(&self, key: &DeviceKeyMaterial) -> DomainResult<PeerTransport>; // one TlsIdentity for server and connector
}
```

mDNS rules:
- Service `_rocket._tcp.local.`, instance name = short id, host name derived from the short id, addresses from `enable_addr_auto()`, the server's real port.
- TXT holds exactly `v`, `id` (52-char base32), `name`, `initials`.
- A record is reported only when `v` parses as `u32`, `id` parses with `DeviceId::parse_base32`, `name` passes `NodeProfile::new`, the instance name equals the short form of `id`, the id is not our own announced id, the port is not 0 and at least one address is dialable. Initials are rebuilt from the name; the TXT `initials` is never trusted.
- A goodbye (`ServiceRemoved`) carries only the instance name, so `BrowseState` maps names to ids (at most 1024 names). An unknown name reports nothing.
- IPv6 link-local addresses are skipped: a `SocketAddr` from the record has no scope id, so it cannot be dialled.
- The id is a claim. Plan 04 lists a node only after `/hello` shows a key that hashes to it (spec 7.1).
- A silent node (no goodbye) drops out through mdns-sd itself: the SRV record carries the 120 s host TTL (`DNS_HOST_TTL` in mdns-sd 0.21.6, equal to `MDNS_RECORD_TTL`), and when it expires the daemon emits `ServiceRemoved`, which `BrowseState` turns into `Gone`. The TTL setters are crate-private in mdns-sd, so this plan does not set them. Manual check 5 in Plan 05 (network change) covers it.
- `start` errors mean "automatic discovery isn't available here" (spec 9). `stop` withdraws the record (waits at most 500 ms for the goodbye) and shuts the daemon down.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 1: Add mdns-sd and register the modules**

In `crates/rocket-infra/Cargo.toml`, under `[dependencies]`, after the `axum = ...` line add:

```toml
# mDNS announce and browse for the team network. Pinned: 0.21.7 is less than two weeks old.
mdns-sd = { version = "=0.21.6", default-features = false }
```

Replace the whole content of `crates/rocket-infra/src/network/mod.rs` with:

```rust
//! Team network pieces that touch the machine: the device key, the local lists, the TLS
//! configs, the peer server and connector, mDNS and the transport factory. The domain rules
//! live in `rocket-network`.

pub mod client;
pub mod identity_store;
pub(crate) mod lock;
pub mod mdns;
#[cfg(test)]
pub(crate) mod peer_test_support;
pub mod repos;
pub mod server;
pub(crate) mod socket;
pub mod tls_config;
pub mod tls_identity;
pub mod transport;

pub use client::RustlsPeerConnector;
pub use identity_store::KeychainDeviceIdentity;
pub use mdns::MdnsDiscovery;
pub use repos::{
    FsBlockedDeviceRepo, FsManualAddressRepo, FsNetworkSettingsRepo, FsPairedDeviceRepo,
};
pub use server::RustlsPeerServer;
pub use tls_config::{client_config, peer_identity_from_certs, server_config};
pub use tls_identity::{build_tls_identity, spki_from_cert, TlsIdentity};
pub use transport::RustlsPeerTransportFactory;
```

In `crates/rocket-infra/src/lib.rs`, replace

```rust
pub use network::{
    build_tls_identity, spki_from_cert, FsBlockedDeviceRepo, FsManualAddressRepo,
    FsNetworkSettingsRepo, FsPairedDeviceRepo, KeychainDeviceIdentity, TlsIdentity,
};
```

with

```rust
pub use network::{
    build_tls_identity, spki_from_cert, FsBlockedDeviceRepo, FsManualAddressRepo,
    FsNetworkSettingsRepo, FsPairedDeviceRepo, KeychainDeviceIdentity, MdnsDiscovery,
    RustlsPeerConnector, RustlsPeerServer, RustlsPeerTransportFactory, TlsIdentity,
};
```

Create `crates/rocket-infra/src/network/mdns.rs` and `crates/rocket-infra/src/network/transport.rs`, each with only its first doc line from Steps 4 and 5 (`//! mDNS discovery ...` and `//! Builds the peer transport ...`).

- [ ] **Step 2: Write the failing tests**

Append to `crates/rocket-infra/src/network/mdns.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use rocket_network::limits::MDNS_SERVICE_TYPE;
    use rocket_network::{Announcement, DeviceId, Discovery, DiscoveryEvent, DiscoverySink};

    use super::*;
    use crate::network::peer_test_support::profile;

    fn device(n: u8) -> DeviceId {
        DeviceId::from_bytes([n; 32])
    }

    fn announcement(id: DeviceId) -> Announcement {
        Announcement {
            device_id: id,
            profile: profile("Ada Lovelace"),
            port: 47821,
            version: 1,
        }
    }

    fn lookup_from(pairs: Vec<(&'static str, String)>) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        move |key: &str| map.get(key).cloned()
    }

    fn fullname(id: &DeviceId) -> String {
        format!("{}.{}", instance_name(id), MDNS_SERVICE_TYPE)
    }

    fn lan() -> Vec<IpAddr> {
        vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))]
    }

    #[test]
    fn txt_record_holds_exactly_v_id_name_and_initials() {
        let id = device(1);
        let txt = txt_record(&announcement(id));
        assert_eq!(
            txt,
            vec![
                ("v", "1".to_string()),
                ("id", id.to_base32()),
                ("name", "Ada Lovelace".to_string()),
                ("initials", "AL".to_string()),
            ]
        );
    }

    #[test]
    fn txt_record_round_trips_through_parse_txt() {
        let id = device(2);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let (parsed_id, parsed_profile, version) = parse_txt(&lookup).expect("parses");
        assert_eq!(parsed_id, id);
        assert_eq!(parsed_profile, profile("Ada Lovelace"));
        assert_eq!(version, 1);
    }

    #[test]
    fn parse_txt_refuses_a_bad_version_id_or_name() {
        let good = device(3).to_base32();
        let cases: Vec<Vec<(&'static str, String)>> = vec![
            vec![("id", good.clone()), ("name", "Ada".into())],
            vec![("v", "x".into()), ("id", good.clone()), ("name", "Ada".into())],
            vec![("v", "1".into()), ("id", "NOTANID".into()), ("name", "Ada".into())],
            vec![("v", "1".into()), ("id", good.to_lowercase()), ("name", "Ada".into())],
            vec![("v", "1".into()), ("name", "Ada".into())],
            vec![("v", "1".into()), ("id", good.clone()), ("name", "".into())],
            vec![("v", "1".into()), ("id", good.clone()), ("name", "bad\u{7}name".into())],
            vec![("v", "1".into()), ("id", good.clone()), ("name", "x".repeat(41))],
            vec![("v", "1".into()), ("id", good.clone())],
        ];
        for (i, pairs) in cases.into_iter().enumerate() {
            let lookup = lookup_from(pairs);
            assert!(parse_txt(&lookup).is_none(), "case {i}");
        }
    }

    #[test]
    fn forged_initials_are_rebuilt_from_the_name() {
        let id = device(4);
        let lookup = lookup_from(vec![
            ("v", "1".into()),
            ("id", id.to_base32()),
            ("name", "Ada Lovelace".into()),
            ("initials", "ZZ".into()),
        ]);
        let (_, parsed_profile, _) = parse_txt(&lookup).expect("parses");
        assert_eq!(parsed_profile.initials, "AL");
    }

    #[test]
    fn a_resolved_record_is_seen_with_its_dialable_addresses() {
        let id = device(5);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let ips = vec![
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6("fe80::1".parse::<Ipv6Addr>().expect("ipv6")),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)),
            IpAddr::V6("2001:db8::5".parse::<Ipv6Addr>().expect("ipv6")),
            IpAddr::V4(Ipv4Addr::new(224, 0, 0, 251)),
        ];
        let mut state = BrowseState::default();

        let event = state.resolved(&fullname(&id), 47822, &ips, &lookup, None);

        assert_eq!(
            event,
            Some(DiscoveryEvent::Seen {
                claimed_id: id,
                profile: profile("Ada Lovelace"),
                addresses: vec![
                    "192.168.1.20:47822".parse::<SocketAddr>().expect("addr"),
                    "[2001:db8::5]:47822".parse::<SocketAddr>().expect("addr"),
                ],
                version: 1,
            })
        );
    }

    #[test]
    fn an_instance_name_that_does_not_match_the_id_is_ignored() {
        let id = device(6);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let mut state = BrowseState::default();
        let spoofed = fullname(&device(7));
        assert_eq!(state.resolved(&spoofed, 47821, &lan(), &lookup, None), None);
    }

    #[test]
    fn our_own_record_is_ignored() {
        let id = device(8);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let mut state = BrowseState::default();
        assert_eq!(state.resolved(&fullname(&id), 47821, &lan(), &lookup, Some(id)), None);
    }

    #[test]
    fn port_zero_or_no_dialable_address_is_ignored() {
        let id = device(9);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let mut state = BrowseState::default();
        assert_eq!(state.resolved(&fullname(&id), 0, &lan(), &lookup, None), None);
        let link_local = vec![IpAddr::V6("fe80::2".parse::<Ipv6Addr>().expect("ipv6"))];
        assert_eq!(
            state.resolved(&fullname(&id), 47821, &link_local, &lookup, None),
            None
        );
    }

    #[test]
    fn a_goodbye_is_gone_only_for_a_known_name() {
        let id = device(10);
        let lookup = lookup_from(txt_record(&announcement(id)));
        let mut state = BrowseState::default();
        assert_eq!(state.removed(&fullname(&id)), None, "never seen");

        state.resolved(&fullname(&id), 47821, &lan(), &lookup, None);
        assert_eq!(
            state.removed(&fullname(&id)),
            Some(DiscoveryEvent::Gone { claimed_id: id })
        );
        assert_eq!(state.removed(&fullname(&id)), None, "reported once");
    }

    #[test]
    fn the_name_table_is_capped_but_known_names_still_update() {
        let mut state = BrowseState::default();
        let mut first = None;
        for i in 0..MAX_KNOWN_INSTANCES {
            let mut bytes = [0u8; 32];
            bytes[0] = (i & 0xff) as u8;
            bytes[1] = (i >> 8) as u8;
            let id = DeviceId::from_bytes(bytes);
            first.get_or_insert(id);
            let lookup = lookup_from(txt_record(&announcement(id)));
            assert!(state.resolved(&fullname(&id), 47821, &lan(), &lookup, None).is_some());
        }
        let extra = device(250);
        let lookup = lookup_from(txt_record(&announcement(extra)));
        assert_eq!(state.resolved(&fullname(&extra), 47821, &lan(), &lookup, None), None);

        let known = first.expect("one id");
        let lookup = lookup_from(txt_record(&announcement(known)));
        assert!(state.resolved(&fullname(&known), 47821, &lan(), &lookup, None).is_some());
    }

    #[test]
    fn instance_and_host_names_come_from_the_short_id() {
        let id = device(11);
        let short = id.short();
        assert_eq!(instance_name(&id), short);
        assert_eq!(
            host_name(&id),
            format!("rocket-{}.local.", short.replace('-', "").to_ascii_lowercase())
        );
    }

    #[test]
    fn withdraw_and_stop_before_start_are_harmless() {
        let discovery = MdnsDiscovery::new();
        discovery.withdraw().expect("nothing to withdraw");
        discovery.stop();
        discovery.stop();
    }

    struct ChannelSink(Mutex<std::sync::mpsc::Sender<DiscoveryEvent>>);

    impl DiscoverySink for ChannelSink {
        fn on_event(&self, event: DiscoveryEvent) {
            let _ = lock_ignore_poison(&self.0).send(event);
        }
    }

    fn wait_for(
        events: &std::sync::mpsc::Receiver<DiscoveryEvent>,
        wanted: impl Fn(&DiscoveryEvent) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match events.recv_timeout(left) {
                Ok(event) if wanted(&event) => return,
                Ok(_) => continue,
                Err(_) => panic!("the event did not arrive within 10 seconds"),
            }
        }
    }

    #[test]
    #[ignore = "needs a network with multicast; manual check 1 covers it where CI has none"]
    fn two_daemons_see_each_other_over_multicast() {
        let announcer = MdnsDiscovery::new();
        let browser = MdnsDiscovery::new();
        let (sender, events) = std::sync::mpsc::channel();
        browser
            .start(Arc::new(ChannelSink(Mutex::new(sender))))
            .expect("browse");
        let id = device(42);

        announcer.announce(&announcement(id)).expect("announce");
        wait_for(&events, |e| {
            matches!(e, DiscoveryEvent::Seen { claimed_id, .. } if *claimed_id == id)
        });

        announcer.withdraw().expect("withdraw");
        wait_for(&events, |e| {
            matches!(e, DiscoveryEvent::Gone { claimed_id } if *claimed_id == id)
        });

        announcer.stop();
        browser.stop();
    }
}
```

Append to `crates/rocket-infra/src/network/transport.rs`:

```rust
#[cfg(test)]
mod tests {
    use rocket_network::limits::PROTOCOL_VERSION;
    use rocket_network::{DeviceKeyMaterial, PairRequest, PeerConnector, PeerTransportFactory};

    use super::*;
    use crate::network::peer_test_support::{profile, start_node};
    use crate::network::tls_identity::test_support::key_material;

    #[test]
    fn the_same_key_gives_the_same_local_identity() {
        let factory = RustlsPeerTransportFactory::new();
        let key = key_material();

        let a = factory.build(&key).expect("build a");
        let b = factory.build(&key).expect("build b");
        let other = factory.build(&key_material()).expect("build other");

        assert_eq!(a.local, b.local);
        assert_ne!(a.local.device_id, other.local.device_id);
    }

    #[test]
    fn a_broken_key_is_an_error() {
        let broken = DeviceKeyMaterial {
            pkcs8_der: vec![1, 2, 3],
        };
        assert!(RustlsPeerTransportFactory::new().build(&broken).is_err());
    }

    #[tokio::test]
    async fn the_built_connector_presents_the_built_identity() {
        let node = start_node("Hub").await;
        let transport = RustlsPeerTransportFactory::new()
            .build(&key_material())
            .expect("build");

        let (peer, _) = transport.connector.hello(node.addr).await.expect("hello");
        assert_eq!(peer.device_id, node.id());
        let request = PairRequest {
            version: PROTOCOL_VERSION,
            profile: profile("Bob"),
        };
        transport
            .connector
            .pair_request(node.addr, &node.id(), &request)
            .await
            .expect("pair request");

        let expected = format!("pair_request:{}:127.0.0.1:Bob", transport.local.device_id.short());
        assert!(node.handler.calls().contains(&expected));
    }

    #[tokio::test]
    async fn the_local_test_factory_listens_on_loopback() {
        use rocket_network::{PeerHandler, PeerServerControl};
        use std::net::SocketAddr;

        use crate::network::peer_test_support::{connector, identity, FakeHandler};

        let key = key_material();
        let transport = RustlsPeerTransportFactory::local_test()
            .build(&key)
            .expect("build");
        // The handler answers hello with the same key the server shows.
        let tls = Arc::new(build_tls_identity(&key).expect("identity"));
        let (handler, _sockets) = FakeHandler::new(&tls, "Hub");
        let as_handler: Arc<dyn PeerHandler> = handler;

        let port = transport.server.start(as_handler).await.expect("start");
        assert_ne!(port, 0);
        let (peer, _) = connector(&identity())
            .hello(SocketAddr::from(([127, 0, 0, 1], port)))
            .await
            .expect("hello on loopback");
        assert_eq!(peer.device_id, transport.local.device_id);
        transport.server.stop();
    }
}
```

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-infra network::mdns`
Expected: FAIL to compile with `cannot find type MdnsDiscovery in this scope`, `cannot find function txt_record`, `cannot find struct BrowseState` and `unresolved import self::mdns::MdnsDiscovery` (from `mod.rs`).

- [ ] **Step 4: Implement mDNS discovery**

Put this above the `#[cfg(test)]` line in `crates/rocket-infra/src/network/mdns.rs`, replacing the doc line from Step 1:

```rust
//! mDNS discovery on `mdns-sd`. Announces this node as `_rocket._tcp.local.` and reports
//! other nodes. TXT ids are only claims: Plan 04 confirms each with `/hello` before listing it.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use mdns_sd::{RecvTimeoutError, ServiceDaemon, ServiceEvent, ServiceInfo};
use rocket_network::limits::MDNS_SERVICE_TYPE;
use rocket_network::{
    Announcement, DeviceId, Discovery, DiscoveryEvent, DiscoverySink, NodeProfile,
};
use rocket_shared::error::{DomainError, DomainResult};

use crate::network::lock::lock_ignore_poison;

const TXT_VERSION: &str = "v";
const TXT_ID: &str = "id";
const TXT_NAME: &str = "name";
const TXT_INITIALS: &str = "initials";
/// How often the browse thread checks its stop flag.
const BROWSE_POLL: Duration = Duration::from_millis(500);
/// How long `stop` waits for the goodbye packet to go out.
const GOODBYE_WAIT: Duration = Duration::from_millis(500);
/// A flood of fake records cannot grow the name table past this.
pub(crate) const MAX_KNOWN_INSTANCES: usize = 1024;

pub(crate) fn instance_name(id: &DeviceId) -> String {
    id.short()
}

pub(crate) fn host_name(id: &DeviceId) -> String {
    format!(
        "rocket-{}.local.",
        id.short().replace('-', "").to_ascii_lowercase()
    )
}

/// The TXT record of spec 7.1. Nothing else is announced.
pub(crate) fn txt_record(announcement: &Announcement) -> Vec<(&'static str, String)> {
    vec![
        (TXT_VERSION, announcement.version.to_string()),
        (TXT_ID, announcement.device_id.to_base32()),
        (TXT_NAME, announcement.profile.display_name.clone()),
        (TXT_INITIALS, announcement.profile.initials.clone()),
    ]
}

/// Reads a TXT record. `None` when the version or id does not parse or the name is not a
/// valid display name. The initials are rebuilt from the name, never taken from the record.
pub(crate) fn parse_txt(
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<(DeviceId, NodeProfile, u32)> {
    let version = lookup(TXT_VERSION)?.parse::<u32>().ok()?;
    let id = DeviceId::parse_base32(&lookup(TXT_ID)?).ok()?;
    let profile = NodeProfile::new(&lookup(TXT_NAME)?).ok()?;
    Some((id, profile, version))
}

/// Addresses a peer can be dialled on, sorted and without duplicates. IPv6 link-local needs a
/// scope id that the record does not give, so it is skipped.
pub(crate) fn dialable_addresses(ips: &[IpAddr], port: u16) -> Vec<SocketAddr> {
    if port == 0 {
        return Vec::new();
    }
    let mut addresses: Vec<SocketAddr> = ips
        .iter()
        .copied()
        .filter(|ip| !ip.is_unspecified() && !ip.is_multicast() && !is_ipv6_link_local(ip))
        .map(|ip| SocketAddr::new(ip, port))
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}

fn is_ipv6_link_local(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if (v6.segments()[0] & 0xffc0) == 0xfe80)
}

/// Remembers which instance name stands for which claimed id, because a goodbye packet only
/// carries the name.
#[derive(Default)]
pub(crate) struct BrowseState {
    known: HashMap<String, DeviceId>,
}

impl BrowseState {
    pub(crate) fn resolved(
        &mut self,
        fullname: &str,
        port: u16,
        ips: &[IpAddr],
        lookup: &dyn Fn(&str) -> Option<String>,
        own: Option<DeviceId>,
    ) -> Option<DiscoveryEvent> {
        let (claimed_id, profile, version) = parse_txt(lookup)?;
        // The instance name must be the short form of the claimed id.
        if !fullname.starts_with(&format!("{}.", instance_name(&claimed_id))) {
            return None;
        }
        if own == Some(claimed_id) {
            return None;
        }
        let addresses = dialable_addresses(ips, port);
        if addresses.is_empty() {
            return None;
        }
        if !self.known.contains_key(fullname) && self.known.len() >= MAX_KNOWN_INSTANCES {
            return None;
        }
        self.known.insert(fullname.to_string(), claimed_id);
        Some(DiscoveryEvent::Seen {
            claimed_id,
            profile,
            addresses,
            version,
        })
    }

    pub(crate) fn removed(&mut self, fullname: &str) -> Option<DiscoveryEvent> {
        self.known
            .remove(fullname)
            .map(|claimed_id| DiscoveryEvent::Gone { claimed_id })
    }
}

#[derive(Default)]
struct MdnsInner {
    daemon: Option<ServiceDaemon>,
    announced: Option<String>,
    browse_stop: Option<Arc<AtomicBool>>,
}

impl MdnsInner {
    /// The daemon is made on first use, so `MdnsDiscovery::new()` cannot fail.
    fn daemon(&mut self) -> DomainResult<ServiceDaemon> {
        if let Some(daemon) = &self.daemon {
            return Ok(daemon.clone());
        }
        let daemon = ServiceDaemon::new().map_err(mdns_error)?;
        self.daemon = Some(daemon.clone());
        Ok(daemon)
    }
}

pub struct MdnsDiscovery {
    inner: Mutex<MdnsInner>,
    /// The announced id, so our own record is never reported.
    own: Arc<Mutex<Option<DeviceId>>>,
}

impl MdnsDiscovery {
    pub fn new() -> Self {
        MdnsDiscovery {
            inner: Mutex::new(MdnsInner::default()),
            own: Arc::new(Mutex::new(None)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, MdnsInner> {
        lock_ignore_poison(&self.inner)
    }
}

impl Default for MdnsDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

fn mdns_error(error: mdns_sd::Error) -> DomainError {
    DomainError::Internal(format!("mDNS is not available: {error}"))
}

impl Discovery for MdnsDiscovery {
    fn start(&self, sink: Arc<dyn DiscoverySink>) -> DomainResult<()> {
        let mut inner = self.lock();
        let daemon = inner.daemon()?;
        if let Some(previous) = inner.browse_stop.take() {
            previous.store(true, Ordering::SeqCst);
            let _ = daemon.stop_browse(MDNS_SERVICE_TYPE);
        }
        let events = daemon.browse(MDNS_SERVICE_TYPE).map_err(mdns_error)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let own = Arc::clone(&self.own);
        std::thread::Builder::new()
            .name("rocket-mdns-browse".to_string())
            .spawn(move || browse_loop(events, sink, own, thread_stop))
            .map_err(|e| DomainError::Internal(format!("start the mDNS browser: {e}")))?;
        inner.browse_stop = Some(stop);
        Ok(())
    }

    fn announce(&self, announcement: &Announcement) -> DomainResult<()> {
        let mut inner = self.lock();
        let daemon = inner.daemon()?;
        if let Some(previous) = inner.announced.take() {
            let _ = daemon.unregister(&previous);
        }
        let properties = txt_record(announcement);
        let info = ServiceInfo::new(
            MDNS_SERVICE_TYPE,
            &instance_name(&announcement.device_id),
            &host_name(&announcement.device_id),
            (),
            announcement.port,
            &properties[..],
        )
        .map_err(mdns_error)?
        .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        daemon.register(info).map_err(mdns_error)?;
        inner.announced = Some(fullname);
        *lock_ignore_poison(&self.own) = Some(announcement.device_id);
        Ok(())
    }

    fn withdraw(&self) -> DomainResult<()> {
        let mut inner = self.lock();
        let Some(fullname) = inner.announced.take() else {
            return Ok(());
        };
        let Some(daemon) = inner.daemon.clone() else {
            return Ok(());
        };
        daemon.unregister(&fullname).map(|_| ()).map_err(mdns_error)
    }

    fn stop(&self) {
        let mut inner = self.lock();
        if let Some(stop) = inner.browse_stop.take() {
            stop.store(true, Ordering::SeqCst);
        }
        let Some(daemon) = inner.daemon.take() else {
            return;
        };
        if let Some(fullname) = inner.announced.take() {
            // Send the goodbye before the daemon exits, so peers drop us at once.
            if let Ok(done) = daemon.unregister(&fullname) {
                let _ = done.recv_timeout(GOODBYE_WAIT);
            }
        }
        let _ = daemon.shutdown();
    }
}

/// Runs on its own thread: maps daemon events and hands them to the sink.
fn browse_loop(
    events: mdns_sd::Receiver<ServiceEvent>,
    sink: Arc<dyn DiscoverySink>,
    own: Arc<Mutex<Option<DeviceId>>>,
    stop: Arc<AtomicBool>,
) {
    let mut state = BrowseState::default();
    while !stop.load(Ordering::SeqCst) {
        let event = match events.recv_timeout(BROWSE_POLL) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let own_id = *lock_ignore_poison(&own);
        let mapped = match event {
            ServiceEvent::ServiceResolved(service) => {
                let ips: Vec<IpAddr> = service
                    .get_addresses()
                    .iter()
                    .map(|ip| ip.to_ip_addr())
                    .collect();
                let lookup = |key: &str| service.get_property_val_str(key).map(str::to_string);
                state.resolved(service.get_fullname(), service.port, &ips, &lookup, own_id)
            }
            ServiceEvent::ServiceRemoved(_, fullname) => state.removed(&fullname),
            _ => None,
        };
        if let Some(event) = mapped {
            sink.on_event(event);
        }
    }
}
```

- [ ] **Step 5: Implement the transport factory**

Put this above the `#[cfg(test)]` line in `crates/rocket-infra/src/network/transport.rs`, replacing the doc line from Step 1:

```rust
//! Builds the peer transport from the device key. One TLS identity is shared by the server
//! and the connector, so both directions show the same certificate.

use std::sync::Arc;

use rocket_network::{DeviceKeyMaterial, PeerTransport, PeerTransportFactory};
use rocket_shared::error::DomainResult;

use crate::network::{build_tls_identity, RustlsPeerConnector, RustlsPeerServer, TlsIdentity};

#[derive(Debug, Default, Clone, Copy)]
pub struct RustlsPeerTransportFactory {
    /// Servers bind 127.0.0.1 on a port the OS picks. Only end-to-end tests set it.
    #[cfg(any(test, feature = "test-support"))]
    local_test: bool,
}

impl RustlsPeerTransportFactory {
    pub fn new() -> Self {
        RustlsPeerTransportFactory::default()
    }

    /// A factory whose servers listen on loopback with an OS port, so two nodes run side by
    /// side in one test without mDNS (Plan 04b).
    #[cfg(any(test, feature = "test-support"))]
    pub fn local_test() -> Self {
        RustlsPeerTransportFactory { local_test: true }
    }

    fn server_for(&self, identity: Arc<TlsIdentity>) -> RustlsPeerServer {
        #[cfg(any(test, feature = "test-support"))]
        if self.local_test {
            return RustlsPeerServer::local_test(identity);
        }
        RustlsPeerServer::new(identity)
    }
}

impl PeerTransportFactory for RustlsPeerTransportFactory {
    fn build(&self, key: &DeviceKeyMaterial) -> DomainResult<PeerTransport> {
        let identity = Arc::new(build_tls_identity(key)?);
        Ok(PeerTransport {
            local: identity.local.clone(),
            connector: Arc::new(RustlsPeerConnector::new(Arc::clone(&identity))),
            server: Arc::new(self.server_for(identity)),
        })
    }
}
```

- [ ] **Step 5b: Open the loopback seams for Plan 04b**

Plan 04b runs two real `NetworkService`s over this transport inside `rocket-app` tests. `rocket-app` already has `rocket-infra = { workspace = true, features = ["test-support"] }` as a dev-dependency, so these seams use that existing feature and never reach a release build.

In `crates/rocket-infra/src/network/server.rs`, replace

```rust
#[cfg(test)]
impl ServerOptions {
```

with

```rust
#[cfg(any(test, feature = "test-support"))]
impl ServerOptions {
```

and in the same file replace

```rust
    pub(crate) fn with_options(identity: Arc<TlsIdentity>, options: ServerOptions) -> Self {
```

with

```rust
    /// A server on 127.0.0.1 with a port the OS picks. For end-to-end tests in other crates.
    #[cfg(any(test, feature = "test-support"))]
    pub fn local_test(identity: Arc<TlsIdentity>) -> Self {
        Self::with_options(identity, ServerOptions::local_test())
    }

    pub(crate) fn with_options(identity: Arc<TlsIdentity>, options: ServerOptions) -> Self {
```

In `crates/rocket-infra/src/network/tls_identity.rs` (Plan 02), replace

```rust
/// Shared helpers for the tests of this module directory.
#[cfg(test)]
pub(crate) mod test_support {
```

with

```rust
/// Shared helpers for the tests of this module directory and, through the `test-support`
/// feature, for the end-to-end tests in `rocket-app` (Plan 04b).
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
```

and inside that module change `pub(crate) fn key_material()` to `pub fn key_material()` and `pub(crate) fn identity()` to `pub fn identity()`. The `use super::test_support::...` lines in the tests stay as they are.

- [ ] **Step 6: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-infra network::mdns`
Expected: PASS, 12 tests, 1 ignored (`two_daemons_see_each_other_over_multicast`).

Run: `cargo test -j4 -p rocket-infra network::transport`
Expected: PASS, 4 tests.

Run: `cargo check -j4 -p rocket-infra --features test-support`
Expected: PASS. The seams of Step 5b compile outside test builds when the feature is on.

On a machine with multicast, also run the ignored test once:
Run: `cargo test -j4 -p rocket-infra network::mdns -- --ignored`
Expected: PASS. If it fails only because the machine or CI has no multicast (container, VPN, client isolation), record that in the ledger; manual check 1 in `docs/manual-tests/team-network.md` (Plan 05) covers real discovery.

- [ ] **Step 7: Check**

Run: `cargo check -j4 -p rocket-infra`
Expected: PASS with no warnings from `crates/rocket-infra/src/network/`.

Run: `cargo clippy -j4 -p rocket-infra --all-targets -- -D warnings`
Expected: no new findings from `crates/rocket-infra/src/network/`. Findings that already exist in other files are not part of this plan.

- [ ] **Step 8: Update the crate guide**

In `crates/rocket-infra/CLAUDE.md`, add rows to its type table, in the table's own format, for the new `network/` module: the keychain device identity, the generic `YamlListRepo` behind the paired, blocked and address repos plus the settings repo, `network::lock`, the TLS identity and verifiers (Plan 02), `RustlsPeerServer`, `RustlsPeerConnector`, `MdnsDiscovery` and `RustlsPeerTransportFactory` (this plan). Each row names the `rocket-network` trait it implements. Add one line to its notes: the Team Network opens sockets and listens on a port, which is I/O and stays in this crate; `rocket-app` sees only the ports.

No commit; reviewed at the plan end.

---

End of Plan 3. Verify:

```bash
cargo check -j4 -p rocket-infra
cargo test -j4 -p rocket-infra network::
cargo clippy -j4 -p rocket-infra --all-targets -- -D warnings
cargo check -j4 --workspace
```

Expected: all green (`network::mdns::tests::two_daemons_see_each_other_over_multicast` ignored). Then run the Opus security review below on the plan's whole diff (spec 8.7). Stop and show the user the whole plan's change (`Cargo.toml`, `Cargo.lock`, `lib.rs`, `crates/rocket-infra/CLAUDE.md`, the new files in `network/`). After the user's OK, commit through the `dev-workflow-skills:1-git-commit` skill, staging explicit paths only, split for example as: `feat(network): add the TLS peer server and connector` (Task 1 files and Cargo files), `feat(network): add the peer WebSocket` (Task 2), `feat(network): add mDNS discovery and the transport factory` (Task 3).

Next plan to run: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-04-service-tauri.md`.

## Self-Review

**Spec mapping.**
- 3a hub with 30 teammates, 256 connections: Task 2 `a_hub_holds_thirty_paired_sockets_at_once`; `MAX_CONNECTIONS` slots held by sockets (`an_open_socket_keeps_its_connection_slot`).
- 5.2 `PeerServer` on axum with rustls and mutual TLS: Task 1 `RustlsPeerServer`; `PeerClient` on hyper over tokio-rustls and tokio-tungstenite: Task 1 and 2 `RustlsPeerConnector`; `MdnsDiscovery` on `mdns-sd`: Task 3.
- 6.1 every connection is TLS 1.3 with both certificates, also on open routes: Plan 02 configs, used for every accept and dial here; the key reaches every route as `ConnInfo.peer`, so pairing sessions are bound to the key that opened them (Plan 04 compares it).
- 6.2 routes `/pair/request`, `/pair/{id}/nonce`, `/pair/{id}/decision`, `GET /pair/{id}`: Task 1, with the body types of Plan 01; B never calls A (the server has no outgoing calls).
- 7.1 port 47821, else the next free up to 47830, the port in use returned: Task 1 `bind_first_free`, tests for fallback, the default range and an exhausted range. TXT `v`, `id`, `name`, `initials` and nothing else: Task 3. Goodbye removes at once: `BrowseState::removed` and `stop` waiting for the goodbye.
- 7.3 one WebSocket at `wss://<host>:<port>/peer` with mutual TLS, paired keys only, Hello first from both sides: Task 2.
- 8.1 allow-list, `403 not-paired` before the upgrade, 404 for the rest, text JSON frames of at most 64 KiB, binary, unknown variants and frames before Hello close: Tasks 1 and 2.
- 8.2 custom verifier on both sides: Plan 02; used through `server_config` and `client_config(identity, expected)`. "Any verification error counts as not paired": a certificate that cannot be read drops the connection.
- 8.3 256 connections, 32 unpaired, 10 s header timeout, 16 KiB body, 30 `/hello` per IP per minute, 5 pairing requests per minute with `503 busy` and no notice, `Origin` refused: Task 1, each with a test.
- 9 default port taken, listener cannot bind (`start` returns `Err`), mDNS unavailable (`Discovery::start` returns `Err`): Tasks 1 and 3. The banners are Plan 04 and 05.
- 10 infra tests: stranger upgrade 403, unknown path 404, `Origin` refused, 6th pairing request busy, body over 16 KiB refused, frame over 64 KiB, binary frame and unknown variant close, one node with 30 peers online, mDNS spoofed TXT id not listed (instance-name check here, key check in Plan 04), real mDNS ignored test plus manual check.

**Names for later plans (exactly as in the index, plus these).**
- `rocket_infra::{RustlsPeerServer, RustlsPeerConnector, MdnsDiscovery, RustlsPeerTransportFactory}` (also under `rocket_infra::network::`). Plan 04 wires only `MdnsDiscovery::new()` and `RustlsPeerTransportFactory::new()`; it never builds the server or connector itself.
- Added beyond the index: `RustlsPeerServer::new(Arc<TlsIdentity>)` and `RustlsPeerConnector::new(Arc<TlsIdentity>)` (the index listed the types without constructors); `Default` for `MdnsDiscovery` and `RustlsPeerTransportFactory`.
- Test seams for Plan 04b, compiled only with `test` or the existing `test-support` feature (Task 3 Step 5b): `RustlsPeerTransportFactory::local_test()`, `RustlsPeerServer::local_test(Arc<TlsIdentity>)`, `ServerOptions::local_test()` and a public `rocket_infra::network::tls_identity::test_support::{key_material, identity}`. Release builds keep the production bind (`0.0.0.0`, 47821 to 47830). Crate-private constants and helpers shared with other modules: `callback_server::{ACCEPT_ERROR_BACKOFF, HEADER_READ_TIMEOUT}` (now `pub(crate)`), `client::REQUEST_TIMEOUT` (imported by `server.rs` and `socket.rs`), `network::lock::lock_ignore_poison` (from Plan 02). Crate-private seams and helpers: `ServerOptions`, `RustlsPeerServer::with_options`, `ConnInfo`, `ConnSlot`, `RouteState`, `FixedWindow`, `is_valid_pairing_id`, `tls_connect`, `send_request`, `RawReply`, `WsFrame`, `spawn_pump`, `exchange_hello`, `open_client_socket`, `BrowseState`, `peer_test_support`.
- Behaviour Plan 04 must rely on: `start` stops a running instance first and returns the bound port; `stop` is idempotent and closes server-side sockets; `on_peer_socket` gets every Hello-checked socket, including duplicates and version mismatches; `PeerSocket.inbound` yields `None` when the socket ends; dropping `outbound` closes it; `DiscoveryEvent::Seen` ids are claims; `pair_request` arrives with a validated profile; the hello connector call already rejects a body id that differs from the certificate.

**Not covered here.**
- Protocol version mismatch handling ("Needs update"), duplicate socket resolution (`keep_dialed_by`), the 20 s ping schedule, missed pings, reconnect backoff, the 201st pairing, blocked and declined devices, hidden mode refusing pairing, one open session per device, manual address probing, merging nodes by id and the `/hello` key check before listing a discovered node: Plan 04 (through `PeerHandler`, `PeerSocket` and `DiscoveryEvent`).
- Closing an open socket when a device is removed or blocked: Plan 04 drops that `PeerSocket`. The transport does not re-check `is_paired` during a live socket.
- IPv6 listening: the server binds `0.0.0.0` only (spec 7.1). IPv6 link-local mDNS addresses are skipped.
- Graceful drain on stop: connections and sockets are cut at once (a close frame is attempted).
- Rate limits are per server instance and reset on restart; the hello limit is per source IP, so many teammates behind one NAT share 30 a minute.
- Real multicast: one ignored test plus manual check 1 (Plan 05).

**Placeholder scan:** every step has full code and a command with the expected result. The only interim body is Task 1's `open_socket`, which Task 2 Step 5 replaces with quoted text; nothing is committed in between.

## Security review checklist (Opus, spec sections 6 and 8)

1. Every accepted connection completes TLS 1.3 with a client certificate before any HTTP byte is parsed; nothing is served on a connection whose certificate could not be read (`serve_connection`).
2. The `PeerIdentity` passed to each handler call comes from that connection's handshake (`ConnInfo` per connection, `keep_alive(false)`), never from a request body, header or path.
3. `/peer` checks `is_paired` before the upgrade; strangers get only `403 not-paired`. The open routes are exactly the five in spec 8.1; every other path is 404 and `Origin` is refused on all of them, including `/peer`.
4. Limits hold under abuse: stalled handshakes time out (10 s), headers time out (10 s), whole requests time out (10 s), bodies stop at 16 KiB, frames stop at 64 KiB on both sides, 256 connections with sockets counted, 32 unpaired, limiter tables capped and failing closed.
5. Rate limits: 30 `/hello` per IP per minute (429), 5 pairing requests per minute across all strangers (503, handler not called, so no notice). Confirm the pair limiter counts before the handler and cannot be bypassed by other routes.
6. Client: with an expected id the key is pinned inside the handshake (Plan 02 verifier) and checked again after it; the hello body's id must equal the certificate's id; replies over 16 KiB are refused; commitment, nonce and pairing id in replies are format-checked; profiles from peers go through `validated()` (initials rebuilt).
7. WebSocket allow-list: first frame must be a Hello, a second Hello closes, binary, close, unknown variants and decode errors close, ping and pong control frames carry no data to the handler.
8. No logging or error text contains nonces, codes, keys, certificates or frame contents; responses to peers are fixed words.
9. mDNS: TXT carries only `v`, `id`, `name`, `initials`; parsed records are claims (Plan 04 must verify with `/hello` before listing); the own id is filtered; spoofed instance names and record floods are bounded.
10. The peer server reaches only `PeerHandler`; no route touches collections, environments, scripts, history, RocketVault, the keychain or `process.env`.
11. Spec 6.4: a node in the middle cannot pass because each TLS connection binds the key used in the pairing hashes; confirm Plan 04 uses `ConnInfo.peer.spki_der` (handshake key) for `PkA`/`PkB`, never a key from a body.
