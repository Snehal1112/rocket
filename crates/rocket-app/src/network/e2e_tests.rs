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
    commitment, commitment_to_base64, pairing_code, DecisionRequest, DeviceId, DeviceIdentityStore,
    DeviceKeyMaterial, HelloResponse, NetworkEvent, NodeProfile, Nonce, NonceMessage, PairRequest,
    PairRequestResponse, PairStatusResponse, PairingId, PeerCallError, PeerConnector, PeerError,
    PeerHandler, PeerIdentity, PeerMessage, PeerServerControl, PeerSocket,
};
use rocket_shared::error::{DomainError, DomainResult};

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
pub(super) struct E2eNode {
    pub(super) name: String,
    pub(super) svc: Arc<NetworkService>,
    /// The PKCS#8 key, so a test can build a second connector that shows the same key.
    pub(super) key: Vec<u8>,
    /// The identity the TLS handshake shows for this node.
    pub(super) identity: PeerIdentity,
    pub(super) paired: Arc<MemPaired>,
    pub(super) blocked: Arc<MemBlocked>,
    pub(super) events: Arc<RecordingEvents>,
    pub(super) audit: Arc<RecordingSecurityAuditPublisher>,
}

impl E2eNode {
    pub(super) fn id(&self) -> DeviceId {
        self.identity.device_id
    }

    pub(super) fn port(&self) -> u16 {
        self.svc.status().port.expect("the node listens")
    }

    pub(super) fn addr(&self) -> SocketAddr {
        local(self.port())
    }

    pub(super) fn audited(&self) -> Vec<AuditEventKind> {
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

pub(super) fn e2e_node(name: &str) -> E2eNode {
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
pub(super) async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
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
pub(super) fn row(node: &E2eNode, id: &DeviceId) -> Option<NodeStatus> {
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
pub(super) async fn pair(a: &E2eNode, b: &E2eNode) -> String {
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
pub(super) async fn online_pair() -> (E2eNode, E2eNode, NetworkHandle, NetworkHandle) {
    let (a, b, ha, hb) = linked().await;
    pair(&a, &b).await;
    wait_until("both rows show Online", || {
        row(&a, &b.id()) == Some(NodeStatus::Online) && row(&b, &a.id()) == Some(NodeStatus::Online)
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
    /// Reveal a nonce other than the one committed to.
    lie: bool,
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
        let committed = log.nb_to_a.clone().ok_or(PeerError::NotFound)?;
        let nb = if self.lie { Nonce::random() } else { committed };
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

/// Two nodes, a relay with its own key, and A knowing only the relay's address.
struct RelaySetup {
    a: E2eNode,
    b: E2eNode,
    handles: (NetworkHandle, NetworkHandle),
    relay: Arc<Relay>,
    server: RustlsPeerServer,
    relay_id: DeviceId,
}

async fn start_relay(lie: bool) -> RelaySetup {
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
        lie,
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
    RelaySetup {
        a,
        b,
        handles: (ha, hb),
        relay,
        server: relay_server,
        relay_id,
    }
}

/// A thinks it pairs with "ben". It really pairs with the relay, which pairs with B.
async fn relayed_pairing() -> RelayedPairing {
    let RelaySetup {
        a,
        b,
        handles,
        relay,
        server,
        relay_id,
    } = start_relay(false).await;
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
        _handles: handles,
        relay,
        _relay_server: server,
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
    assert!(b
        .audited()
        .contains(&AuditEventKind::NetworkDeviceUnpaired {
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
    assert!(b
        .audited()
        .contains(&AuditEventKind::NetworkDeviceUnpaired {
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
    assert!(a
        .audited()
        .contains(&AuditEventKind::NetworkDeviceUnpaired {
            device_id: b.id().to_base32(),
            by_peer: false
        }));
    assert!(a.events.removed().contains(&("ben".to_string(), false)));
    assert!(b.events.removed().contains(&("ann".to_string(), true)));
    wait_until("the connection is closed on both sides", || {
        row(&a, &b.id()) != Some(NodeStatus::Online) && row(&b, &a.id()) != Some(NodeStatus::Online)
    })
    .await;

    // The blocked key is refused a new pairing request, not only a socket.
    let refused = connector
        .pair_request(a.addr(), &a.id(), &request)
        .await
        .expect_err("a blocked key may not ask again");
    assert!(
        matches!(refused, PeerCallError::Rejected(403)),
        "the answer for a blocked key is Forbidden: {refused:?}"
    );

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
    wait_until("both nodes finished the pairing", || {
        !run.a.events.finished().is_empty() && !run.b.events.finished().is_empty()
    })
    .await;
    assert_eq!(run.a.paired.count(), 0);
    assert_eq!(run.b.paired.count(), 0);
    assert!(!run.a.events.finished().contains(&"accepted".to_string()));
}

#[tokio::test]
async fn a_relay_that_reveals_another_nonce_than_it_committed_to_is_caught() {
    let setup = start_relay(true).await;
    let err = setup
        .a
        .svc
        .request_pairing(&setup.relay_id)
        .await
        .expect_err("the commitment check must fail");
    assert_eq!(
        err,
        DomainError::InvalidInput("Pairing failed. Try again.".into())
    );
    assert_eq!(setup.a.paired.count(), 0);
    assert_eq!(setup.b.paired.count(), 0);
    let _keep = (&setup.handles, &setup.relay, &setup.server);
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
