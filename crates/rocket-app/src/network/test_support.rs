//! Fakes for every port, and a harness that links two services in memory.
//!
//! `FakeConnector` can route a call to another service's `PeerHandler`, so two real
//! `NetworkService`s pair and connect without sockets. A route without a handler answers
//! `/hello` from fixed data and hands out scripted sockets, which tests use to play a peer.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_network::limits::{MAX_MANUAL_ADDRESSES, MAX_PAIRED_DEVICES, PROTOCOL_VERSION};
use rocket_network::{
    Announcement, BlockedDevice, BlockedDeviceRepository, DecisionRequest, DeviceId,
    DeviceIdentityStore, DeviceKeyMaterial, Direction, Discovery, DiscoveryEvent, DiscoverySink,
    HelloResponse, ManualAddress, ManualAddressRepository, NetworkEvent, NetworkSettings,
    NetworkSettingsRepository, NodeProfile, NonceMessage, Outcome, PairRequest,
    PairRequestResponse, PairStatusResponse, PairedDevice, PairedDeviceRepository, PairingId,
    PeerCallError, PeerConnector, PeerError, PeerHandler, PeerIdentity, PeerMessage,
    PeerServerControl, PeerSocket, PeerTransport, PeerTransportFactory, QuarantineReport,
    TransferEvent,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{EventPublisher, PublishedEvent};
use tokio::sync::mpsc;

use super::service::paired_limit_message;
use super::{NetworkDeps, NetworkService, PairingDecision};

/// The address of the node with this seed.
pub(crate) fn addr(seed: u8) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 47000 + u16::from(seed)))
}

/// A peer identity whose key is eight bytes of `seed`.
pub(crate) fn identity(seed: u8) -> PeerIdentity {
    PeerIdentity::from_spki(vec![seed; 8])
}

pub(crate) fn profile(name: &str) -> NodeProfile {
    NodeProfile::new(name).expect("valid test profile")
}

/// Lets every ready task run. With paused time this also moves the clock 1 ms.
pub(crate) async fn settle() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

pub(crate) fn ip() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

pub(crate) fn pair_request(name: &str) -> PairRequest {
    PairRequest {
        version: PROTOCOL_VERSION,
        profile: profile(name),
    }
}

// ----- identity, discovery, server, transport -----

pub(crate) struct FakeIdentity {
    pub fail: AtomicBool,
}

impl DeviceIdentityStore for FakeIdentity {
    fn load_or_create(&self) -> DomainResult<DeviceKeyMaterial> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(DomainError::Internal(
                "the keychain could not be read".into(),
            ));
        }
        Ok(DeviceKeyMaterial {
            pkcs8_der: vec![7; 32],
        })
    }
}

#[derive(Default)]
pub(crate) struct FakeDiscovery {
    pub fail_start: AtomicBool,
    pub starts: AtomicUsize,
    pub stops: AtomicUsize,
    pub withdraws: AtomicUsize,
    pub announced: Mutex<Vec<Announcement>>,
    sink: Mutex<Option<Arc<dyn DiscoverySink>>>,
}

impl FakeDiscovery {
    /// Plays an mDNS event into the service, as the mDNS thread would. Not `emit`, which is the
    /// service's own event publisher.
    pub fn play(&self, event: DiscoveryEvent) {
        let sink = self.sink.lock().expect("sink lock").clone();
        if let Some(sink) = sink {
            sink.on_event(event);
        }
    }

    pub fn announcements(&self) -> Vec<Announcement> {
        self.announced.lock().expect("announced lock").clone()
    }
}

impl Discovery for FakeDiscovery {
    fn start(&self, sink: Arc<dyn DiscoverySink>) -> DomainResult<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        if self.fail_start.load(Ordering::SeqCst) {
            return Err(DomainError::Io("multicast is not available".into()));
        }
        *self.sink.lock().expect("sink lock") = Some(sink);
        Ok(())
    }

    fn announce(&self, announcement: &Announcement) -> DomainResult<()> {
        self.announced
            .lock()
            .expect("announced lock")
            .push(announcement.clone());
        Ok(())
    }

    fn withdraw(&self) -> DomainResult<()> {
        self.withdraws.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

pub(crate) struct FakeServer {
    pub port: Mutex<Result<u16, String>>,
    pub starts: AtomicUsize,
    pub stops: AtomicUsize,
}

impl FakeServer {
    pub fn new() -> Arc<FakeServer> {
        Arc::new(FakeServer {
            port: Mutex::new(Ok(47821)),
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl PeerServerControl for FakeServer {
    async fn start(&self, _handler: Arc<dyn PeerHandler>) -> DomainResult<u16> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        let outcome = self.port.lock().expect("port lock").clone();
        outcome.map_err(DomainError::Io)
    }

    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

/// One address the fake connector can reach.
#[derive(Clone)]
pub(crate) struct Route {
    /// The key the certificate on that address carries.
    pub identity: PeerIdentity,
    pub profile: NodeProfile,
    pub version: u32,
    /// When set, calls reach this handler. When not, `/hello` is answered from fixed data.
    pub handler: Option<Arc<dyn PeerHandler>>,
    /// What the `/hello` body claims as its device id, when it should differ from the key.
    pub claims: Option<DeviceId>,
}

impl Route {
    pub fn fixed(identity: PeerIdentity, name: &str) -> Route {
        Route {
            identity,
            profile: profile(name),
            version: PROTOCOL_VERSION,
            handler: None,
            claims: None,
        }
    }
}

pub(crate) struct FakeConnector {
    pub caller: PeerIdentity,
    routes: Mutex<HashMap<SocketAddr, Route>>,
    scripted: Mutex<VecDeque<PeerSocket>>,
    pub hello_calls: AtomicUsize,
    pub open_calls: AtomicUsize,
    pub pair_request_calls: AtomicUsize,
}

impl FakeConnector {
    pub fn new(caller: PeerIdentity) -> Arc<FakeConnector> {
        Arc::new(FakeConnector {
            caller,
            routes: Mutex::new(HashMap::new()),
            scripted: Mutex::new(VecDeque::new()),
            hello_calls: AtomicUsize::new(0),
            open_calls: AtomicUsize::new(0),
            pair_request_calls: AtomicUsize::new(0),
        })
    }

    pub fn add_route(&self, address: SocketAddr, route: Route) {
        self.routes
            .lock()
            .expect("routes lock")
            .insert(address, route);
    }

    pub fn remove_route(&self, address: SocketAddr) {
        self.routes.lock().expect("routes lock").remove(&address);
    }

    /// Queues a socket that `open_socket` returns for a route without a handler.
    pub fn push_socket(&self, socket: PeerSocket) {
        self.scripted
            .lock()
            .expect("scripted lock")
            .push_back(socket);
    }

    fn route(&self, address: SocketAddr) -> Result<Route, PeerCallError> {
        self.routes
            .lock()
            .expect("routes lock")
            .get(&address)
            .cloned()
            .ok_or_else(|| PeerCallError::Unreachable("no route".into()))
    }

    /// The route for `address`, checked against the key the caller expects there.
    fn checked(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
    ) -> Result<(Route, Arc<dyn PeerHandler>), PeerCallError> {
        let route = self.route(address)?;
        if route.identity.device_id != *expected {
            return Err(PeerCallError::IdentityMismatch);
        }
        let handler = route
            .handler
            .clone()
            .ok_or_else(|| PeerCallError::Unreachable("no handler".into()))?;
        Ok((route, handler))
    }
}

fn map_peer_error(error: PeerError) -> PeerCallError {
    match error {
        PeerError::Busy => PeerCallError::Busy,
        PeerError::NotPaired => PeerCallError::NotPaired,
        other => PeerCallError::Rejected(other.status_code()),
    }
}

#[async_trait]
impl PeerConnector for FakeConnector {
    async fn hello(
        &self,
        address: SocketAddr,
    ) -> Result<(PeerIdentity, HelloResponse), PeerCallError> {
        self.hello_calls.fetch_add(1, Ordering::SeqCst);
        let route = self.route(address)?;
        let response = match &route.handler {
            Some(handler) => handler.hello().await,
            None => HelloResponse {
                device_id: route.claims.unwrap_or(route.identity.device_id),
                version: route.version,
                profile: route.profile.clone(),
            },
        };
        Ok((route.identity, response))
    }

    async fn pair_request(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        req: &PairRequest,
    ) -> Result<(PeerIdentity, PairRequestResponse), PeerCallError> {
        self.pair_request_calls.fetch_add(1, Ordering::SeqCst);
        let (route, handler) = self.checked(address, expected)?;
        let reply = handler
            .pair_request(self.caller.clone(), ip(), req.clone())
            .await
            .map_err(map_peer_error)?;
        Ok((route.identity, reply))
    }

    async fn pair_nonce(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        msg: &NonceMessage,
    ) -> Result<NonceMessage, PeerCallError> {
        let (_, handler) = self.checked(address, expected)?;
        handler
            .pair_nonce(self.caller.clone(), id.clone(), msg.clone())
            .await
            .map_err(map_peer_error)
    }

    async fn pair_decision(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        req: &DecisionRequest,
    ) -> Result<(), PeerCallError> {
        let (_, handler) = self.checked(address, expected)?;
        handler
            .pair_decision(self.caller.clone(), id.clone(), req.clone())
            .await
            .map_err(map_peer_error)
    }

    async fn pair_status(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
    ) -> Result<PairStatusResponse, PeerCallError> {
        let (_, handler) = self.checked(address, expected)?;
        handler
            .pair_status(self.caller.clone(), id.clone())
            .await
            .map_err(map_peer_error)
    }

    async fn open_socket(
        &self,
        address: SocketAddr,
        expected: &DeviceId,
        hello: PeerMessage,
    ) -> Result<PeerSocket, PeerCallError> {
        self.open_calls.fetch_add(1, Ordering::SeqCst);
        let route = self.route(address)?;
        if route.identity.device_id != *expected {
            return Err(PeerCallError::IdentityMismatch);
        }
        let Some(handler) = route.handler.clone() else {
            return self
                .scripted
                .lock()
                .expect("scripted lock")
                .pop_front()
                .ok_or_else(|| PeerCallError::Unreachable("no scripted socket".into()));
        };
        if !handler.is_paired(&self.caller.device_id).await {
            return Err(PeerCallError::NotPaired);
        }
        let (to_them, their_inbound) = mpsc::channel(16);
        let (to_us, our_inbound) = mpsc::channel(16);
        let theirs = PeerSocket {
            peer: self.caller.clone(),
            dialed_by: self.caller.device_id,
            remote_hello: hello,
            outbound: to_us,
            inbound: their_inbound,
        };
        let ours = PeerSocket {
            peer: route.identity,
            dialed_by: self.caller.device_id,
            remote_hello: handler.local_hello(),
            outbound: to_them,
            inbound: our_inbound,
        };
        handler.on_peer_socket(theirs).await;
        Ok(ours)
    }
}

/// The test side of a scripted socket.
pub(crate) struct Remote {
    pub to_service: mpsc::Sender<PeerMessage>,
    pub from_service: mpsc::Receiver<PeerMessage>,
}

/// A socket the service sees as coming from `peer`, plus the handles to play the peer.
pub(crate) fn scripted_socket(
    peer: PeerIdentity,
    dialed_by: DeviceId,
    version: u32,
) -> (PeerSocket, Remote) {
    let (to_service, inbound) = mpsc::channel(16);
    let (outbound, from_service) = mpsc::channel(16);
    let socket = PeerSocket {
        peer,
        dialed_by,
        remote_hello: PeerMessage::Hello {
            version,
            profile: profile("Remote"),
        },
        outbound,
        inbound,
    };
    (
        socket,
        Remote {
            to_service,
            from_service,
        },
    )
}

pub(crate) struct FakeTransportFactory {
    local: PeerIdentity,
    connector: Arc<FakeConnector>,
    server: Arc<FakeServer>,
    pub builds: AtomicUsize,
    pub fail: AtomicBool,
}

impl PeerTransportFactory for FakeTransportFactory {
    fn build(&self, _key: &DeviceKeyMaterial) -> DomainResult<PeerTransport> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(DomainError::Internal("tls setup failed".into()));
        }
        Ok(PeerTransport {
            local: self.local.clone(),
            connector: self.connector.clone(),
            server: self.server.clone(),
        })
    }
}

// ----- repositories -----

#[derive(Default)]
pub(crate) struct MemPaired(Mutex<Vec<PairedDevice>>);

impl MemPaired {
    pub fn seed(&self, id: DeviceId, name: &str) {
        self.seed_with(id, name, Vec::new());
    }

    pub fn seed_with(&self, id: DeviceId, name: &str, last_addresses: Vec<SocketAddr>) {
        self.0.lock().expect("paired lock").push(PairedDevice {
            device_id: id,
            name_at_pairing: name.to_string(),
            paired_at: Utc::now(),
            last_addresses,
        });
    }

    /// Adds `count` unrelated paired devices.
    pub fn fill(&self, count: usize) {
        for index in 0..count {
            let mut bytes = [0u8; 32];
            bytes[0] = (index >> 8) as u8;
            bytes[1] = index as u8;
            bytes[31] = 0xEE;
            self.seed(DeviceId::from_bytes(bytes), "Filler");
        }
    }

    pub fn contains(&self, id: &DeviceId) -> bool {
        self.0
            .lock()
            .expect("paired lock")
            .iter()
            .any(|p| p.device_id == *id)
    }

    pub fn count(&self) -> usize {
        self.0.lock().expect("paired lock").len()
    }

    pub fn last_addresses(&self, id: &DeviceId) -> Vec<SocketAddr> {
        self.0
            .lock()
            .expect("paired lock")
            .iter()
            .find(|p| p.device_id == *id)
            .map(|p| p.last_addresses.clone())
            .unwrap_or_default()
    }
}

impl PairedDeviceRepository for MemPaired {
    fn list(&self) -> DomainResult<Vec<PairedDevice>> {
        Ok(self.0.lock().expect("paired lock").clone())
    }

    fn get(&self, id: &DeviceId) -> DomainResult<Option<PairedDevice>> {
        Ok(self
            .0
            .lock()
            .expect("paired lock")
            .iter()
            .find(|p| p.device_id == *id)
            .cloned())
    }

    fn upsert(&self, device: PairedDevice) -> DomainResult<()> {
        let mut list = self.0.lock().expect("paired lock");
        if let Some(existing) = list.iter_mut().find(|p| p.device_id == device.device_id) {
            *existing = device;
            return Ok(());
        }
        if list.len() >= MAX_PAIRED_DEVICES {
            return Err(DomainError::InvalidInput(paired_limit_message()));
        }
        list.push(device);
        Ok(())
    }

    fn remove(&self, id: &DeviceId) -> DomainResult<bool> {
        let mut list = self.0.lock().expect("paired lock");
        let before = list.len();
        list.retain(|p| p.device_id != *id);
        Ok(list.len() != before)
    }
}

#[derive(Default)]
pub(crate) struct MemBlocked(
    Mutex<Vec<BlockedDevice>>,
    Mutex<Option<Arc<FakeQuarantine>>>,
);

impl MemBlocked {
    /// A read of this list marks the linked quarantine fake as ready.
    pub fn link(&self, quarantine: Arc<FakeQuarantine>) {
        *self.1.lock().expect("link lock") = Some(quarantine);
    }

    fn touched(&self) {
        if let Some(q) = self.1.lock().expect("link lock").as_ref() {
            q.mark_read();
        }
    }

    pub fn put(&self, id: DeviceId, name: &str) {
        self.0.lock().expect("blocked lock").push(BlockedDevice {
            device_id: id,
            name: name.to_string(),
            blocked_at: Utc::now(),
        });
    }

    pub fn has(&self, id: &DeviceId) -> bool {
        self.0
            .lock()
            .expect("blocked lock")
            .iter()
            .any(|b| b.device_id == *id)
    }
}

impl BlockedDeviceRepository for MemBlocked {
    fn list(&self) -> DomainResult<Vec<BlockedDevice>> {
        self.touched();
        Ok(self.0.lock().expect("blocked lock").clone())
    }

    fn contains(&self, id: &DeviceId) -> DomainResult<bool> {
        self.touched();
        Ok(self
            .0
            .lock()
            .expect("blocked lock")
            .iter()
            .any(|b| b.device_id == *id))
    }

    fn add(&self, device: BlockedDevice) -> DomainResult<()> {
        let mut list = self.0.lock().expect("blocked lock");
        if !list.iter().any(|b| b.device_id == device.device_id) {
            list.push(device);
        }
        Ok(())
    }

    fn remove(&self, id: &DeviceId) -> DomainResult<bool> {
        let mut list = self.0.lock().expect("blocked lock");
        let before = list.len();
        list.retain(|b| b.device_id != *id);
        Ok(list.len() != before)
    }
}

#[derive(Default)]
pub(crate) struct MemAddresses(Mutex<Vec<ManualAddress>>);

impl ManualAddressRepository for MemAddresses {
    fn list(&self) -> DomainResult<Vec<ManualAddress>> {
        Ok(self.0.lock().expect("addresses lock").clone())
    }

    fn add(&self, address: ManualAddress) -> DomainResult<()> {
        let mut list = self.0.lock().expect("addresses lock");
        let host = address.host.to_lowercase();
        if list
            .iter()
            .any(|a| a.host.to_lowercase() == host && a.port == address.port)
        {
            return Ok(());
        }
        if list.len() >= MAX_MANUAL_ADDRESSES {
            return Err(DomainError::InvalidInput(
                "You already saved 50 addresses.".into(),
            ));
        }
        list.push(address);
        Ok(())
    }

    fn remove(&self, host: &str, port: u16) -> DomainResult<bool> {
        let mut list = self.0.lock().expect("addresses lock");
        let before = list.len();
        let host = host.to_lowercase();
        list.retain(|a| !(a.host.to_lowercase() == host && a.port == port));
        Ok(list.len() != before)
    }
}

#[derive(Default)]
pub(crate) struct MemSettings(Mutex<NetworkSettings>);

impl MemSettings {
    pub fn get(&self) -> NetworkSettings {
        self.0.lock().expect("settings lock").clone()
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.0.lock().expect("settings lock").hidden = hidden;
    }

    pub fn set_name(&self, name: &str) {
        self.0.lock().expect("settings lock").display_name = Some(name.to_string());
    }
}

impl NetworkSettingsRepository for MemSettings {
    fn load(&self) -> DomainResult<NetworkSettings> {
        Ok(self.get())
    }

    fn save(&self, settings: &NetworkSettings) -> DomainResult<()> {
        *self.0.lock().expect("settings lock") = settings.clone();
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct FakeQuarantine {
    file: Mutex<Option<String>>,
    /// The real repositories only quarantine while they read. False means not read yet.
    ready: AtomicBool,
}

impl FakeQuarantine {
    pub fn set(&self, file: &str) {
        *self.file.lock().expect("quarantine lock") = Some(file.to_string());
        self.ready.store(true, Ordering::SeqCst);
    }

    /// Reports the file only after the linked repository was read, like the real ones.
    pub fn set_after_first_read(&self, file: &str) {
        *self.file.lock().expect("quarantine lock") = Some(file.to_string());
        self.ready.store(false, Ordering::SeqCst);
    }

    pub fn mark_read(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }
}

impl QuarantineReport for FakeQuarantine {
    fn take_quarantined(&self) -> Option<String> {
        if !self.ready.load(Ordering::SeqCst) {
            return None;
        }
        self.file.lock().expect("quarantine lock").take()
    }
}

// ----- events -----

/// Kept concrete on purpose: its query helpers are specific to `NetworkEvent`, and the recorders
/// in other crates are private test doubles.
#[derive(Default)]
pub(crate) struct RecordingEvents(Mutex<Vec<NetworkEvent>>, Mutex<Vec<TransferEvent>>);

impl RecordingEvents {
    /// Every transfer event, in order.
    pub fn transfers(&self) -> Vec<TransferEvent> {
        self.1.lock().expect("events lock").clone()
    }

    /// The `(direction, outcome)` of every `Finished` transfer event, in order.
    pub fn finished_outcomes(&self) -> Vec<(Direction, Outcome)> {
        self.transfers()
            .into_iter()
            .filter_map(|e| match e {
                TransferEvent::Finished {
                    direction, outcome, ..
                } => Some((direction, outcome)),
                _ => None,
            })
            .collect()
    }

    pub fn all(&self) -> Vec<NetworkEvent> {
        self.0.lock().expect("events lock").clone()
    }

    pub fn incoming(&self) -> Vec<NetworkEvent> {
        self.all()
            .into_iter()
            .filter(|e| matches!(e, NetworkEvent::PairingIncoming { .. }))
            .collect()
    }

    /// The `(pairing_id, code)` of the first incoming notice, if any.
    pub fn first_incoming(&self) -> Option<(String, String)> {
        self.all().into_iter().find_map(|e| match e {
            NetworkEvent::PairingIncoming {
                pairing_id, code, ..
            } => Some((pairing_id, code)),
            _ => None,
        })
    }

    /// The outcome text of every `PairingFinished`, in order.
    pub fn finished(&self) -> Vec<String> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                NetworkEvent::PairingFinished { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect()
    }

    /// The `(display_name, by_peer)` of every `PairedRemoved`, in order.
    pub fn removed(&self) -> Vec<(String, bool)> {
        self.all()
            .into_iter()
            .filter_map(|e| match e {
                NetworkEvent::PairedRemoved {
                    display_name,
                    by_peer,
                    ..
                } => Some((display_name, by_peer)),
                _ => None,
            })
            .collect()
    }
}

impl EventPublisher for RecordingEvents {
    fn publish(&self, event: &dyn PublishedEvent) {
        if let Some(event) = event.downcast_ref::<NetworkEvent>() {
            self.0.lock().expect("events lock").push(event.clone());
        }
        if let Some(event) = event.downcast_ref::<TransferEvent>() {
            self.1.lock().expect("events lock").push(event.clone());
        }
    }
}

// ----- the harness -----

/// One service with all its fakes.
pub(crate) struct Node {
    pub seed: u8,
    pub name: String,
    pub svc: Arc<NetworkService>,
    pub identity: PeerIdentity,
    pub connector: Arc<FakeConnector>,
    pub server: Arc<FakeServer>,
    pub discovery: Arc<FakeDiscovery>,
    pub factory: Arc<FakeTransportFactory>,
    pub id_store: Arc<FakeIdentity>,
    pub paired: Arc<MemPaired>,
    pub blocked: Arc<MemBlocked>,
    pub addresses: Arc<MemAddresses>,
    pub settings: Arc<MemSettings>,
    pub quarantine: Arc<FakeQuarantine>,
    pub events: Arc<RecordingEvents>,
    pub audit: Arc<RecordingSecurityAuditPublisher>,
}

impl Node {
    pub fn id(&self) -> DeviceId {
        self.identity.device_id
    }

    pub fn addr(&self) -> SocketAddr {
        addr(self.seed)
    }

    pub fn profile(&self) -> NodeProfile {
        profile(&self.name)
    }

    pub fn handler(&self) -> Arc<dyn PeerHandler> {
        self.svc.clone()
    }
}

/// Builds a node. Its OS user name is `name`, so that is its default display name.
pub(crate) fn node(seed: u8, name: &str) -> Node {
    let identity = identity(seed);
    let connector = FakeConnector::new(identity.clone());
    let server = FakeServer::new();
    let discovery = Arc::new(FakeDiscovery::default());
    let factory = Arc::new(FakeTransportFactory {
        local: identity.clone(),
        connector: connector.clone(),
        server: server.clone(),
        builds: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
    });
    let id_store = Arc::new(FakeIdentity {
        fail: AtomicBool::new(false),
    });
    let paired = Arc::new(MemPaired::default());
    let blocked = Arc::new(MemBlocked::default());
    let addresses = Arc::new(MemAddresses::default());
    let settings = Arc::new(MemSettings::default());
    let quarantine = Arc::new(FakeQuarantine::default());
    blocked.link(quarantine.clone());
    let events = Arc::new(RecordingEvents::default());
    let audit = Arc::new(RecordingSecurityAuditPublisher::new());
    let svc = NetworkService::new(NetworkDeps {
        identity: id_store.clone(),
        paired: paired.clone(),
        blocked: blocked.clone(),
        addresses: addresses.clone(),
        settings: settings.clone(),
        quarantine: vec![quarantine.clone() as Arc<dyn QuarantineReport>],
        discovery: discovery.clone(),
        transport: factory.clone(),
        events: events.clone(),
        audit: audit.clone(),
        os_user_name: Some(name.to_string()),
    });
    Node {
        seed,
        name: name.to_string(),
        svc,
        identity,
        connector,
        server,
        discovery,
        factory,
        id_store,
        paired,
        blocked,
        addresses,
        settings,
        quarantine,
        events,
        audit,
    }
}

/// Lets each node reach the other's handler at the other's address.
pub(crate) fn link(a: &Node, b: &Node) {
    a.connector.add_route(
        b.addr(),
        Route {
            identity: b.identity.clone(),
            profile: b.profile(),
            version: PROTOCOL_VERSION,
            handler: Some(b.handler()),
            claims: None,
        },
    );
    b.connector.add_route(
        a.addr(),
        Route {
            identity: a.identity.clone(),
            profile: a.profile(),
            version: PROTOCOL_VERSION,
            handler: Some(a.handler()),
            claims: None,
        },
    );
}

/// Plays an mDNS sighting of `other` into `node`.
pub(crate) fn see(node: &Node, other: &Node) {
    node.discovery.play(DiscoveryEvent::Seen {
        claimed_id: other.id(),
        profile: other.profile(),
        addresses: vec![other.addr()],
        version: PROTOCOL_VERSION,
    });
}

/// The node with the lower device id first.
pub(crate) fn ordered<'a>(x: &'a Node, y: &'a Node) -> (&'a Node, &'a Node) {
    if x.id() < y.id() {
        (x, y)
    } else {
        (y, x)
    }
}

/// Both users accept: the responder first, then the initiator. It does not advance time, so the
/// caller waits for the save in its own way (`settle` here, `wait_until` in Plan 04b).
pub(crate) async fn accept_both(
    responder: &Arc<NetworkService>,
    initiator: &Arc<NetworkService>,
    id: &PairingId,
) {
    responder
        .respond_pairing(id, PairingDecision::Accept)
        .await
        .expect("responder accepts");
    initiator
        .respond_pairing(id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
}

/// The status of the row for `id`, if any.
pub(crate) fn row_status(node: &Node, id: &DeviceId) -> Option<super::NodeStatus> {
    node.svc
        .list_nodes()
        .into_iter()
        .find(|row| row.device_id == id.to_base32())
        .map(|row| row.status)
}
