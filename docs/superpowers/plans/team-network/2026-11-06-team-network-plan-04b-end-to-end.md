# Team Network Plan 04b: End-to-End Tests over the Real Transport

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 4b of 5, runs after Plan 04 and before Plan 05.** Previous: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-04-service-tauri.md` (must be merged and green). Next: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-05-frontend.md`. Recommended model: Sonnet. Index (binding names): `docs/superpowers/plans/team-network/00-plan-index.md`.

**Goal:** Prove that the real pieces fit together. Two real `NetworkService` instances talk over the real `rocket-infra` transport (rustls TLS 1.3 with client certificates, the axum peer server, the hyper connector, the peer WebSocket) on `127.0.0.1`. The tests cover a full pairing with the same code on both screens, the presence socket, unpair online and offline, Block on a paired device, and a node in the middle. They also show that the pairing codes come from the TLS handshake keys. Plans 01 to 04 tested each layer against fakes. This plan closes the gap between them (spec section 10, rocket-infra list).

**Architecture:** One new test module, `crates/rocket-app/src/network/e2e_tests.rs`, compiled only in `rocket-app` test builds.
- It reuses Plan 04's in-memory repositories and `FakeDiscovery` from `network::test_support`, which are `#[cfg(test)]` inside `rocket-app`. So the test must be a unit-test module of `rocket-app`, not an integration test.
- It reaches the real transport through the dev-dependency that `rocket-app` already has: `rocket-infra = { workspace = true, features = ["test-support"] }`.
- There is no dependency cycle, because `rocket-infra` does not depend on `rocket-app`.
- `src-tauri/tests` was rejected for two reasons: it cannot see `rocket-app`'s test fakes, so they would be copied, and it would compile the whole Tauri app for a network test.
- Each node gets a real Ed25519 key (`rocket_infra::network::tls_identity::test_support::key_material`). It uses `RustlsPeerTransportFactory::local_test()`, which serves on `127.0.0.1` with a port the OS picks.
- Nodes find each other through manual addresses (`add_address("127.0.0.1", port)`), not mDNS. `FakeDiscovery` stands in for mDNS.
- For the node-in-the-middle cases a small `Relay` test handler runs on a real `RustlsPeerServer::local_test` with its own key. It relays the pairing calls to B with its own connector and records the nonces and the key A showed in the handshake.

**Tech Stack:** Rust, tokio (real time, no paused clock: real sockets need it), async-trait, `rocket-network`, `rocket-infra` (`test-support` feature), `rocket-audit`'s `RecordingSecurityAuditPublisher`.

**Spec:** `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md`, sections 6.1, 6.2, 6.4, 6.5, 7.3, 8.3 (Block) and 10 (rocket-infra list).

Prerequisites: Plans 01 to 04 are applied in the working tree. This plan uses these names from them:
- Plan 03 Task 3 Step 5b seams: `RustlsPeerTransportFactory::local_test()`, `RustlsPeerServer::local_test(Arc<TlsIdentity>)`, `rocket_infra::network::tls_identity::test_support::key_material`.
- Plan 04: `NetworkService`, `NetworkDeps`, `NetworkHandle`, `NodeStatus`, `PairingDecision`, `PairingStarted`, and from `network::test_support`: `MemPaired`, `MemBlocked`, `MemAddresses`, `MemSettings`, `FakeDiscovery`, `RecordingEvents` (with `first_incoming`) and `accept_both`.

## Global Constraints

- `-j4` on every cargo command. Never `cargo test --workspace` or `--all`. Use `cargo test -j4 -p rocket-app network::e2e_tests`.
- This plan adds test code only, plus one module line in `network/mod.rs`. If a test fails, the bug is in Plan 03 or Plan 04 code. Fix it there with superpowers:systematic-debugging, and record a `Ruling:` line in the ledger. Never weaken the test.
- No bare unwrap call in any Rust text. Tests use `expect("reason")`. A repo hook rejects a bare unwrap call, tests included. Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())`.
- Never hold a `std::sync::Mutex` guard across an `.await`.
- Code comments are short, full sentences that end with a period. No emojis.
- Real sockets mean real time. Every wait goes through `wait_until` with a 20 s limit, never a fixed sleep that hopes to be long enough. The one short sleep in the node-in-the-middle test only lets a best-effort cancel finish, and the asserts after it hold whether or not it finished.
- Logs and assertion messages never print nonces, codes, keys or certificates. Codes are compared inside `assert!`, never with `assert_eq!`, so a failing message does not print them.
- No commit during the plan. The plan ends with "reviewed at the plan end": the user reviews the change, then it is committed through `dev-workflow-skills:1-git-commit` with explicit paths (`test(network): ...`).
- Line numbers drift. Anchor every edit on quoted text.

## Review Focus

1. **The node in the middle** (`a_node_in_the_middle_gets_different_codes_on_each_side`). The relay holds a valid certificate of its own and relays every pairing call. The two users must still see different codes, and nothing is saved after they decline.
2. **The codes come from the handshake keys** (`pairing_codes_come_from_the_tls_handshake_keys`). A's code must equal `pairing_code(A's handshake key, relay's handshake key, Na, Nb')`, and B's must equal `pairing_code(relay's key, B's key, Na, Nb)`. The relay saw A's real handshake key. This is the property spec 6.4 rests on, and before this plan it was only checked by reading the code.
3. **Unpair while the peer is offline** (`a_peer_unpaired_while_offline_removes_itself_on_reconnect`). The peer learns of the removal only from `403 not-paired` on its next dial, after a restart, and removes its side once with `by_peer: true`.
4. **Block on a paired device** (`blocking_a_paired_device_closes_the_real_connection`). A real socket is open. Block must unpair both sides (the peer gets `Unpair`) and close the connection, and the blocked key cannot reconnect.
5. **No flaky timing.** Every wait is `wait_until` with a clear message. The test suite must pass ten runs in a row (Step 4).

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-app/src/network/mod.rs` | modify | Register `e2e_tests` (test builds only) |
| `crates/rocket-app/src/network/e2e_tests.rs` | create | Real two-node harness, `Relay`, seven tests |

No `Cargo.toml` change. `rocket-app` already has `rocket-infra = { workspace = true, features = ["test-support"] }` under `[dev-dependencies]` (checked in Step 1).

---

### Task 1: Two real nodes over TLS on loopback

**Files:**
- Modify: `crates/rocket-app/src/network/mod.rs`
- Create: `crates/rocket-app/src/network/e2e_tests.rs`

**Interfaces:**
- Consumes:
  - `rocket_infra::network::{build_tls_identity, RustlsPeerConnector, RustlsPeerServer, RustlsPeerTransportFactory, TlsIdentity}` and `rocket_infra::network::tls_identity::test_support::key_material`.
  - `rocket_network::{commitment, commitment_to_base64, pairing_code, DecisionRequest, DeviceId, DeviceIdentityStore, DeviceKeyMaterial, HelloResponse, NetworkEvent, NodeProfile, Nonce, NonceMessage, PairRequest, PairRequestResponse, PairStatusResponse, PairingId, PeerConnector, PeerError, PeerHandler, PeerIdentity, PeerMessage, PeerServerControl, PeerSocket}` and `rocket_network::limits::PROTOCOL_VERSION`.
  - Plan 04 names listed under Prerequisites.
- Produces: test-only items. These are `E2eNode`, `FixedKey`, `Relay`, `RelayLog`, `RelayedPairing`, `wait_until`, `linked`, `pair`, `online_pair`, `relayed_pairing`, `incoming` and `row`. Nothing outside the test module uses them.

Rules the tests pin:
- Both screens show the same 6 digits for a direct pairing. Both paired lists hold the other device. Both nodes publish `PairingFinished accepted` and audit `NetworkDevicePaired`.
- After pairing, a real `/peer` WebSocket opens and both rows show `Online`.
- Remove while online sends `Unpair`. The peer removes its side with `by_peer: true`.
- Remove while the peer is offline is silent. When the peer starts again and dials, it gets `403 not-paired`, removes its side and publishes `PairedRemoved { by_peer: true }`.
- Block on a paired device removes the pairing on both sides, closes the socket, records `NetworkDeviceBlocked` and `NetworkDeviceUnpaired { by_peer: false }`, and keeps the device from reconnecting.
- A relay with its own key gets two different codes on the two sides. Each code is the spec formula over the keys of the TLS connection it ran on.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 1: Check the dev-dependency and register the module**

Run: `grep -n 'rocket-infra' crates/rocket-app/Cargo.toml`
Expected: a line under `[dev-dependencies]` that reads `rocket-infra = { workspace = true, features = ["test-support"] }`. If the `test-support` feature is missing, add it to that line. Do not add `rocket-infra` to `[dependencies]`.

Run: `cargo check -j4 -p rocket-infra --features test-support`
Expected: PASS. The Plan 03 seams (`RustlsPeerTransportFactory::local_test`, `RustlsPeerServer::local_test`, the public `tls_identity::test_support`) exist. If this fails, Plan 03 Task 3 Step 5b was not applied. Apply it first.

In `crates/rocket-app/src/network/mod.rs`, replace

```rust
#[cfg(test)]
#[path = "connections_tests.rs"]
mod connections_tests;
```

with

```rust
#[cfg(test)]
#[path = "connections_tests.rs"]
mod connections_tests;
#[cfg(test)]
#[path = "e2e_tests.rs"]
mod e2e_tests;
```

Create `crates/rocket-app/src/network/e2e_tests.rs` with only:

```rust
//! End-to-end tests: two real services over the real TLS transport on 127.0.0.1.
```

Run: `cargo test -j4 -p rocket-app network::e2e_tests`
Expected: PASS with `0 passed` (the module is empty).

- [ ] **Step 2: Write the end-to-end tests**

Replace the whole content of `crates/rocket-app/src/network/e2e_tests.rs` with:

```rust
//! End-to-end tests: two real services over the real TLS transport on 127.0.0.1.
//!
//! The nodes find each other through manual addresses, so no multicast is needed. mDNS is
//! the in-memory `FakeDiscovery`. Everything else is the code that ships.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_audit::AuditEventKind;
use rocket_infra::network::tls_identity::test_support::key_material;
use rocket_infra::network::{
    build_tls_identity, RustlsPeerConnector, RustlsPeerServer, RustlsPeerTransportFactory,
    TlsIdentity,
};
use rocket_network::limits::PROTOCOL_VERSION;
use rocket_network::{
    commitment, commitment_to_base64, pairing_code, DecisionRequest, DeviceId,
    DeviceIdentityStore, DeviceKeyMaterial, HelloResponse, NetworkEvent, NodeProfile, Nonce,
    NonceMessage, PairRequest, PairRequestResponse, PairStatusResponse, PairingId, PeerConnector,
    PeerError, PeerHandler, PeerIdentity, PeerMessage, PeerServerControl, PeerSocket,
};
use rocket_shared::error::DomainResult;

use super::test_support::{
    accept_both, FakeDiscovery, MemAddresses, MemBlocked, MemPaired, MemSettings, RecordingEvents,
};
use super::{
    NetworkDeps, NetworkHandle, NetworkService, NodeStatus, PairingDecision, PairingStarted,
};

/// The longest any single wait may take. Pairing polls once a second and a reconnect waits
/// for the backoff, so a few seconds is normal and 20 s means something is broken.
const WAIT: Duration = Duration::from_secs(20);

// ----- the harness -----

/// A keychain that always returns the same real Ed25519 key.
struct FixedKey(Vec<u8>);

impl DeviceIdentityStore for FixedKey {
    fn load_or_create(&self) -> DomainResult<DeviceKeyMaterial> {
        Ok(DeviceKeyMaterial {
            pkcs8_der: self.0.clone(),
        })
    }
}

/// One real service with in-memory storage.
struct E2eNode {
    name: String,
    svc: Arc<NetworkService>,
    /// The PKCS#8 key, so a test can build a second connector that shows the same key.
    key: Vec<u8>,
    /// The identity the TLS handshake shows for this node.
    identity: PeerIdentity,
    paired: Arc<MemPaired>,
    blocked: Arc<MemBlocked>,
    events: Arc<RecordingEvents>,
    audit: Arc<RecordingSecurityAuditPublisher>,
}

impl E2eNode {
    fn id(&self) -> DeviceId {
        self.identity.device_id
    }

    fn port(&self) -> u16 {
        self.svc.status().port.expect("the node listens")
    }

    fn addr(&self) -> SocketAddr {
        local(self.port())
    }

    fn audited(&self) -> Vec<AuditEventKind> {
        self.audit.records().into_iter().map(|r| r.kind).collect()
    }

    /// A connector that shows this node's key, for calls the service never makes itself.
    fn own_connector(&self) -> RustlsPeerConnector {
        let key = DeviceKeyMaterial {
            pkcs8_der: self.key.clone(),
        };
        RustlsPeerConnector::new(Arc::new(build_tls_identity(&key).expect("identity")))
    }
}

fn local(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn e2e_node(name: &str) -> E2eNode {
    let key = key_material();
    let bytes = key.pkcs8_der.clone();
    let identity = build_tls_identity(&key).expect("identity").local;
    let paired = Arc::new(MemPaired::default());
    let blocked = Arc::new(MemBlocked::default());
    let events = Arc::new(RecordingEvents::default());
    let audit = Arc::new(RecordingSecurityAuditPublisher::new());
    let svc = NetworkService::new(NetworkDeps {
        identity: Arc::new(FixedKey(bytes.clone())),
        paired: paired.clone(),
        blocked: blocked.clone(),
        addresses: Arc::new(MemAddresses::default()),
        settings: Arc::new(MemSettings::default()),
        quarantine: Vec::new(),
        discovery: Arc::new(FakeDiscovery::default()),
        transport: Arc::new(RustlsPeerTransportFactory::local_test()),
        events: events.clone(),
        audit: audit.clone(),
        os_user_name: Some(name.to_string()),
    });
    E2eNode {
        name: name.to_string(),
        svc,
        key: bytes,
        identity,
        paired,
        blocked,
        events,
        audit,
    }
}

/// Polls `check` every 50 ms until it holds. Fails the test after `WAIT`.
async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !check() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for: {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The status of the row for `id` in this node's list, if there is a row.
fn row(node: &E2eNode, id: &DeviceId) -> Option<NodeStatus> {
    node.svc
        .list_nodes()
        .into_iter()
        .find(|r| r.device_id == id.to_base32())
        .map(|r| r.status)
}

/// The pairing id and code of the first incoming notice on this node.
fn incoming(node: &E2eNode) -> (String, String) {
    match node.events.first_incoming() {
        Some(notice) => notice,
        None => panic!("no pairing notice on {}", node.name),
    }
}

/// Two started nodes, each with the other's loopback address saved.
async fn linked() -> (E2eNode, E2eNode, NetworkHandle, NetworkHandle) {
    let a = e2e_node("ann");
    let b = e2e_node("ben");
    let ha = a.svc.start().await;
    let hb = b.svc.start().await;
    a.svc
        .add_address("127.0.0.1", b.port())
        .await
        .expect("A saves B's address");
    b.svc
        .add_address("127.0.0.1", a.port())
        .await
        .expect("B saves A's address");
    wait_until("each node lists the other", || {
        row(&a, &b.id()).is_some() && row(&b, &a.id()).is_some()
    })
    .await;
    (a, b, ha, hb)
}

/// A pairs with B, both accept, and both save. Returns the code both screens showed.
async fn pair(a: &E2eNode, b: &E2eNode) -> String {
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    wait_until("the notice on the responder", || {
        !b.events.incoming().is_empty()
    })
    .await;
    let (pairing_id, code) = incoming(b);
    assert_eq!(pairing_id, started.pairing_id.0);
    assert!(
        code == started.code.digits(),
        "both screens must show the same code"
    );

    accept_both(&b.svc, &a.svc, &started.pairing_id).await;
    wait_until("both sides saved the pairing", || {
        a.paired.contains(&b.id()) && b.paired.contains(&a.id())
    })
    .await;
    code
}

/// Two nodes that are paired and show each other online.
async fn online_pair() -> (E2eNode, E2eNode, NetworkHandle, NetworkHandle) {
    let (a, b, ha, hb) = linked().await;
    pair(&a, &b).await;
    wait_until("both rows show Online", || {
        row(&a, &b.id()) == Some(NodeStatus::Online)
            && row(&b, &a.id()) == Some(NodeStatus::Online)
    })
    .await;
    (a, b, ha, hb)
}

// ----- the relay, a node in the middle with its own valid key -----

/// What the relay saw. The nonces are kept so the test can recompute both codes.
#[derive(Default)]
struct RelayLog {
    /// The key A showed in the TLS handshake with the relay.
    initiator_spki: Option<Vec<u8>>,
    /// A's nonce, passed on to B unchanged.
    na: Option<Nonce>,
    /// The nonce the relay committed to and revealed to A.
    nb_to_a: Option<Nonce>,
    /// The nonce B revealed to the relay.
    nb_from_b: Option<Nonce>,
}

/// Answers A as if it were B, and calls B as if it were A. It cannot know B's nonce before
/// it must commit to A, so it commits to a nonce of its own.
struct Relay {
    me: Arc<TlsIdentity>,
    to_b: RustlsPeerConnector,
    b_addr: SocketAddr,
    b_id: DeviceId,
    log: Mutex<RelayLog>,
}

impl Relay {
    fn profile() -> NodeProfile {
        NodeProfile::new("ben").expect("valid name")
    }

    fn log(&self) -> MutexGuard<'_, RelayLog> {
        self.log.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl PeerHandler for Relay {
    async fn hello(&self) -> HelloResponse {
        HelloResponse {
            device_id: self.me.local.device_id,
            version: PROTOCOL_VERSION,
            profile: Relay::profile(),
        }
    }

    async fn pair_request(
        &self,
        peer: PeerIdentity,
        _remote_ip: IpAddr,
        req: PairRequest,
    ) -> Result<PairRequestResponse, PeerError> {
        let (_, from_b) = self
            .to_b
            .pair_request(self.b_addr, &self.b_id, &req)
            .await
            .map_err(|_| PeerError::Internal)?;
        let nb = Nonce::random();
        let commit = commitment(&self.me.local.spki_der, &peer.spki_der, &nb);
        {
            let mut log = self.log();
            log.initiator_spki = Some(peer.spki_der.clone());
            log.nb_to_a = Some(nb);
        }
        Ok(PairRequestResponse {
            pairing_id: from_b.pairing_id,
            commitment: commitment_to_base64(&commit),
        })
    }

    async fn pair_nonce(
        &self,
        _peer: PeerIdentity,
        id: PairingId,
        msg: NonceMessage,
    ) -> Result<NonceMessage, PeerError> {
        let from_b = self
            .to_b
            .pair_nonce(self.b_addr, &self.b_id, &id, &msg)
            .await
            .map_err(|_| PeerError::Internal)?;
        let na = Nonce::parse_base64(&msg.nonce).map_err(|_| PeerError::BadRequest)?;
        let nb_b = Nonce::parse_base64(&from_b.nonce).map_err(|_| PeerError::BadRequest)?;
        let mut log = self.log();
        log.na = Some(na);
        log.nb_from_b = Some(nb_b);
        let nb = log.nb_to_a.clone().ok_or(PeerError::NotFound)?;
        Ok(NonceMessage {
            nonce: nb.to_base64(),
        })
    }

    async fn pair_decision(
        &self,
        _peer: PeerIdentity,
        id: PairingId,
        req: DecisionRequest,
    ) -> Result<(), PeerError> {
        self.to_b
            .pair_decision(self.b_addr, &self.b_id, &id, &req)
            .await
            .map_err(|_| PeerError::Internal)
    }

    async fn pair_status(
        &self,
        _peer: PeerIdentity,
        id: PairingId,
    ) -> Result<PairStatusResponse, PeerError> {
        self.to_b
            .pair_status(self.b_addr, &self.b_id, &id)
            .await
            .map_err(|_| PeerError::Internal)
    }

    async fn is_paired(&self, _device: &DeviceId) -> bool {
        false
    }

    async fn on_peer_socket(&self, _socket: PeerSocket) {}

    fn local_hello(&self) -> PeerMessage {
        PeerMessage::Hello {
            version: PROTOCOL_VERSION,
            profile: Relay::profile(),
        }
    }
}

/// A relayed pairing that has reached the code on both sides.
struct RelayedPairing {
    a: E2eNode,
    b: E2eNode,
    _handles: (NetworkHandle, NetworkHandle),
    relay: Arc<Relay>,
    _relay_server: RustlsPeerServer,
    started: PairingStarted,
    code_b: String,
}

/// A thinks it pairs with "ben". It really pairs with the relay, which pairs with B.
async fn relayed_pairing() -> RelayedPairing {
    let a = e2e_node("ann");
    let b = e2e_node("ben");
    let ha = a.svc.start().await;
    let hb = b.svc.start().await;

    let me = Arc::new(build_tls_identity(&key_material()).expect("relay identity"));
    let relay = Arc::new(Relay {
        me: Arc::clone(&me),
        to_b: RustlsPeerConnector::new(Arc::clone(&me)),
        b_addr: b.addr(),
        b_id: b.id(),
        log: Mutex::new(RelayLog::default()),
    });
    let relay_server = RustlsPeerServer::local_test(me);
    let relay_port = relay_server
        .start(relay.clone())
        .await
        .expect("the relay listens");
    let relay_id = relay.me.local.device_id;

    a.svc
        .add_address("127.0.0.1", relay_port)
        .await
        .expect("A saves the relay's address");
    wait_until("A lists the relay", || row(&a, &relay_id).is_some()).await;
    let started = a
        .svc
        .request_pairing(&relay_id)
        .await
        .expect("A pairs with what it thinks is ben");
    wait_until("B shows a notice", || !b.events.incoming().is_empty()).await;
    let (_, code_b) = incoming(&b);

    RelayedPairing {
        a,
        b,
        _handles: (ha, hb),
        relay,
        _relay_server: relay_server,
        started,
        code_b,
    }
}

// ----- pairing and presence -----

#[tokio::test]
async fn two_real_nodes_pair_with_the_same_code_and_save_each_other() {
    let (a, b, _ha, _hb) = linked().await;
    let code = pair(&a, &b).await;
    assert_eq!(code.len(), 6);

    wait_until("both publish accepted", || {
        a.events.finished() == vec!["accepted"] && b.events.finished() == vec!["accepted"]
    })
    .await;
    // B bound the session to the key A showed in the handshake.
    assert_eq!(b.events.incoming().len(), 1, "one notice on the responder");
    match b.events.incoming().first() {
        Some(NetworkEvent::PairingIncoming { device_id, .. }) => {
            assert_eq!(device_id, &a.id().to_base32());
        }
        _ => panic!("no notice"),
    }
    assert!(a.audited().contains(&AuditEventKind::NetworkDevicePaired {
        device_id: b.id().to_base32()
    }));
    assert!(b.audited().contains(&AuditEventKind::NetworkDevicePaired {
        device_id: a.id().to_base32()
    }));
}

#[tokio::test]
async fn paired_nodes_open_a_real_socket_and_show_online() {
    let (a, b, _ha, _hb) = online_pair().await;
    assert_eq!(row(&a, &b.id()), Some(NodeStatus::Online));
    assert_eq!(row(&b, &a.id()), Some(NodeStatus::Online));
}

// ----- removing and blocking -----

#[tokio::test]
async fn unpairing_online_removes_both_sides() {
    let (a, b, _ha, _hb) = online_pair().await;

    a.svc.remove_paired(&b.id()).await.expect("remove");
    wait_until("B removes A after Unpair", || !b.paired.contains(&a.id())).await;

    assert!(!a.paired.contains(&b.id()));
    assert!(a.events.removed().contains(&("ben".to_string(), false)));
    assert!(b.events.removed().contains(&("ann".to_string(), true)));
    assert!(b.audited().contains(&AuditEventKind::NetworkDeviceUnpaired {
        device_id: a.id().to_base32(),
        by_peer: true
    }));
}

#[tokio::test]
async fn a_peer_unpaired_while_offline_removes_itself_on_reconnect() {
    let (a, b, _ha, hb) = online_pair().await;

    // B goes away. A notices the socket close.
    hb.stop();
    drop(hb);
    wait_until("A shows B as not online", || {
        row(&a, &b.id()) != Some(NodeStatus::Online)
    })
    .await;

    a.svc
        .remove_paired(&b.id())
        .await
        .expect("remove while the peer is away");
    assert!(b.paired.contains(&a.id()), "B was not told");

    // B comes back on a new port and dials A, which answers 403 not-paired.
    let _hb = b.svc.start().await;
    wait_until("B learns it was removed", || !b.paired.contains(&a.id())).await;
    assert!(b.events.removed().contains(&("ann".to_string(), true)));
    assert!(b.audited().contains(&AuditEventKind::NetworkDeviceUnpaired {
        device_id: a.id().to_base32(),
        by_peer: true
    }));
}

#[tokio::test]
async fn blocking_a_paired_device_closes_the_real_connection() {
    let (a, b, _ha, _hb) = online_pair().await;

    // B asks to pair again over a separate connection with its own key.
    let connector = b.own_connector();
    let request = PairRequest {
        version: PROTOCOL_VERSION,
        profile: NodeProfile::new("ben").expect("valid name"),
    };
    let (_, reply) = connector
        .pair_request(a.addr(), &a.id(), &request)
        .await
        .expect("a paired device may ask again");
    connector
        .pair_nonce(
            a.addr(),
            &a.id(),
            &reply.pairing_id,
            &NonceMessage {
                nonce: Nonce::random().to_base64(),
            },
        )
        .await
        .expect("nonce");

    a.svc
        .respond_pairing(&reply.pairing_id, PairingDecision::Block)
        .await
        .expect("A blocks B");

    wait_until("B got Unpair and removed A", || !b.paired.contains(&a.id())).await;
    assert!(a.blocked.has(&b.id()));
    assert!(!a.paired.contains(&b.id()), "blocking also unpairs");
    assert!(a.audited().contains(&AuditEventKind::NetworkDeviceBlocked {
        device_id: b.id().to_base32()
    }));
    assert!(a.audited().contains(&AuditEventKind::NetworkDeviceUnpaired {
        device_id: b.id().to_base32(),
        by_peer: false
    }));
    assert!(a.events.removed().contains(&("ben".to_string(), false)));
    assert!(b.events.removed().contains(&("ann".to_string(), true)));
    wait_until("the connection is closed on both sides", || {
        row(&a, &b.id()) != Some(NodeStatus::Online)
            && row(&b, &a.id()) != Some(NodeStatus::Online)
    })
    .await;

    // The blocked key cannot open a new socket.
    let hello = PeerMessage::Hello {
        version: PROTOCOL_VERSION,
        profile: NodeProfile::new("ben").expect("valid name"),
    };
    let again = connector.open_socket(a.addr(), &a.id(), hello).await;
    assert!(again.is_err(), "a blocked device gets no socket");
}

// ----- a node in the middle -----

#[tokio::test]
async fn a_node_in_the_middle_gets_different_codes_on_each_side() {
    let run = relayed_pairing().await;

    assert!(
        run.started.code.digits() != run.code_b,
        "a node in the middle must not be able to show both users the same code"
    );

    // The users compare, see different codes and say no. Nothing is saved.
    run.b
        .svc
        .respond_pairing(&run.started.pairing_id, PairingDecision::Decline)
        .await
        .expect("B declines");
    run.a
        .svc
        .cancel_pairing(&run.started.pairing_id)
        .expect("A cancels");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(run.a.paired.count(), 0);
    assert_eq!(run.b.paired.count(), 0);
}

#[tokio::test]
async fn pairing_codes_come_from_the_tls_handshake_keys() {
    let run = relayed_pairing().await;
    let a_spki = run.a.identity.spki_der.clone();
    let b_spki = run.b.identity.spki_der.clone();
    let m_spki = run.relay.me.local.spki_der.clone();

    let (seen_a, na, nb_to_a, nb_from_b) = {
        let log = run.relay.log();
        (
            log.initiator_spki.clone(),
            log.na.clone().expect("A's nonce passed the relay"),
            log.nb_to_a.clone().expect("the relay committed to a nonce"),
            log.nb_from_b.clone().expect("B revealed its nonce"),
        )
    };
    assert!(
        seen_a.as_deref() == Some(a_spki.as_slice()),
        "the relay saw A's real key in the handshake"
    );
    assert!(
        run.started.code == pairing_code(&a_spki, &m_spki, &na, &nb_to_a),
        "A's code uses A's key and the key the relay showed in its handshake"
    );
    assert!(
        run.code_b == pairing_code(&m_spki, &b_spki, &na, &nb_from_b).digits(),
        "B's code uses the key the relay showed to B and B's own key"
    );
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -j4 -p rocket-app network::e2e_tests`
Expected: PASS, 7 tests, 0 failed, in well under a minute.

These tests check code that Plans 03 and 04 already finished, so the first run should pass. A failure is a real defect. Read the `timed out waiting for: ...` message to see which step stalled, then:
- "each node lists the other": the probe after `add_address` did not verify `/hello`. Check `verify_hello`, and check that `local_test()` reports the real OS port through `status().port`.
- "both sides saved the pairing": look at the initiator's poll (`poll_initiator`) and the responder's `pair_decision`.
- "both rows show Online": look at `dial`, `accept_socket` and the duplicate rule in `run_socket`. Both sides dial here because both have a manual address.
- "B learns it was removed": `NOT_PAIRED_GRACE` must not apply after a restart (`prepare` clears `conns`), and B must have A's address after a restart (the manual address in `MemAddresses`).
- A code assertion: `initiate` must take `PkB` from the `pair_request` handshake identity, and the handler must take `PkA` from `ConnInfo.peer`.

Fix the owning file, record a `Ruling:` line in the ledger, and run the test again.

- [ ] **Step 4: Prove the tests are not flaky**

Run: `for i in 1 2 3 4 5 6 7 8 9 10; do cargo test -j4 -p rocket-app network::e2e_tests -q || break; done`
Expected: ten passing runs and no failure. One failure means a race. Treat it as a defect (Step 3), not as bad luck.

- [ ] **Step 5: Check the crate**

Run: `cargo test -j4 -p rocket-app network`
Expected: PASS. The Plan 04 tests and the 7 new tests are green.

Run: `cargo clippy -j4 -p rocket-app --all-targets -- -D warnings`
Expected: no findings in `crates/rocket-app/src/network/`.

---

## End of Plan 4b. Verify

Run, in order:
- `cargo check -j4 --workspace`, expected PASS.
- `cargo test -j4 -p rocket-infra network`, expected PASS (Plan 03 tests, including `the_local_test_factory_listens_on_loopback`).
- `cargo test -j4 -p rocket-app network`, expected PASS.

Stop and show the user the whole change (`crates/rocket-app/src/network/mod.rs`, `crates/rocket-app/src/network/e2e_tests.rs`, and any defect fixes in Plan 03 or 04 files). After the user's OK, commit through `dev-workflow-skills:1-git-commit`, staging explicit paths only, for example `test(network): add end-to-end tests over the real transport`. A defect fix gets its own `fix(network): ...` commit.

Next: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-05-frontend.md`.

## Self-Review

**Spec coverage**

| Spec item | Test |
|---|---|
| 10: full pairing with matching codes and saved lists | `two_real_nodes_pair_with_the_same_code_and_save_each_other` |
| 7.3: the WebSocket opens and both show Online | `paired_nodes_open_a_real_socket_and_show_online` |
| 6.5 and 10: unpair online | `unpairing_online_removes_both_sides` |
| 6.5 and 10: unpair offline, `403 not-paired` on the next dial | `a_peer_unpaired_while_offline_removes_itself_on_reconnect` |
| 6.5 and 8.3: Block on a paired device unpairs and closes (user decision 2026-11-06) | `blocking_a_paired_device_closes_the_real_connection` |
| 6.4 and 10: a node in the middle gets different codes | `a_node_in_the_middle_gets_different_codes_on_each_side` |
| 6.1 and 6.4: `PkA` and `PkB` are the TLS handshake keys | `pairing_codes_come_from_the_tls_handshake_keys` |

**Decisions**

1. The tests live in `rocket-app` as a unit-test module, because Plan 04's fakes are `#[cfg(test)]` there. The real transport comes through the existing dev-dependency on `rocket-infra` with `test-support`. There is no cycle, because `rocket-infra` does not depend on `rocket-app`.
2. Nodes meet through manual addresses on loopback. Real mDNS stays a manual check (Plan 05 manual check 1), as spec 10 allows.
3. The handshake-key check uses the relay as a recording tap. Only a node in the path sees both nonces, and the relay's two TLS legs show that each side's code uses the keys of its own connection.
4. The seams live in Plan 03 Task 3 Step 5b and compile only with `test` or `test-support`. Release builds keep the production bind.

5. The harness shares `accept_both` and `RecordingEvents::first_incoming` with Plan 04's `test_support`. `Relay` stays its own `PeerHandler` on purpose: it is a deliberate man in the middle that forwards over a real connector and keeps a log of the nonces, so it has nothing in common with the fake connector's routing. `linked`, `pair` and `online_pair` stay here because they wait with real time (`wait_until`), where Plan 04 waits with the paused clock.

**Known limit:** the node-in-the-middle test asserts that two independent 6-digit codes differ. They match by chance about once in a million runs. That is the spec's own bound (6.4).

**Out of scope here:** the 30-node hub over real TLS. Plan 03 tests 30 real sockets and Plan 04 tests 30 online rows with fakes, and manual check 9 covers the whole hub.

**Placeholders:** none. Every step has full code or an exact command with the expected result.

**Type consistency:** every name comes from the index, Plan 03's Step 5b seams or Plan 04's `network::test_support`. Codes are compared inside `assert!`, so a failing message never prints a code.
