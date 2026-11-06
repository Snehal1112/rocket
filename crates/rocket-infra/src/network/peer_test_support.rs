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
    let port = server
        .start(as_handler)
        .await
        .expect("start the peer server");
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
