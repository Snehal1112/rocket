//! Ports: the traits that infra implements and the service consumes.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use rocket_shared::error::DomainResult;
use tokio::sync::mpsc;

use crate::device::DeviceId;
use crate::identity::{DeviceKeyMaterial, PeerIdentity};
use crate::messages::PeerMessage;
use crate::pairing::PairingId;
use crate::profile::NodeProfile;
use crate::records::{BlockedDevice, ManualAddress, NetworkSettings, PairedDevice};
use crate::wire::{
    DecisionRequest, HelloResponse, NonceMessage, PairRequest, PairRequestResponse,
    PairStatusResponse,
};

pub trait DeviceIdentityStore: Send + Sync {
    /// Loads the device key or creates it. Errors when the keychain is
    /// unavailable. It never falls back to a file.
    fn load_or_create(&self) -> DomainResult<DeviceKeyMaterial>;
}

pub trait PairedDeviceRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<PairedDevice>>;
    fn get(&self, id: &DeviceId) -> DomainResult<Option<PairedDevice>>;
    /// `InvalidInput` when a new entry would exceed `MAX_PAIRED_DEVICES`.
    fn upsert(&self, device: PairedDevice) -> DomainResult<()>;
    fn remove(&self, id: &DeviceId) -> DomainResult<bool>;
}

pub trait BlockedDeviceRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<BlockedDevice>>;
    fn contains(&self, id: &DeviceId) -> DomainResult<bool>;
    fn add(&self, device: BlockedDevice) -> DomainResult<()>;
    fn remove(&self, id: &DeviceId) -> DomainResult<bool>;
}

pub trait ManualAddressRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<ManualAddress>>;
    /// Dedupes on (host lowercased, port). `InvalidInput` over `MAX_MANUAL_ADDRESSES`.
    fn add(&self, address: ManualAddress) -> DomainResult<()>;
    fn remove(&self, host: &str, port: u16) -> DomainResult<bool>;
}

pub trait NetworkSettingsRepository: Send + Sync {
    fn load(&self) -> DomainResult<NetworkSettings>;
    fn save(&self, settings: &NetworkSettings) -> DomainResult<()>;
}

/// Each Fs repo also implements this, so the service can warn about a quarantined file once.
pub trait QuarantineReport: Send + Sync {
    /// The name of a file that was set aside, once. `None` afterwards.
    fn take_quarantined(&self) -> Option<String>;
}

/// What the local node announces on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub device_id: DeviceId,
    pub profile: NodeProfile,
    pub port: u16,
    pub version: u32,
}

/// What discovery reports. The id is only claimed until `/hello` confirms it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    Seen {
        claimed_id: DeviceId,
        profile: NodeProfile,
        addresses: Vec<SocketAddr>,
        version: u32,
    },
    Gone {
        claimed_id: DeviceId,
    },
}

pub trait DiscoverySink: Send + Sync {
    fn on_event(&self, event: DiscoveryEvent);
}

pub trait Discovery: Send + Sync {
    /// Starts browsing. An error means "discovery unavailable".
    fn start(&self, sink: Arc<dyn DiscoverySink>) -> DomainResult<()>;
    fn announce(&self, announcement: &Announcement) -> DomainResult<()>;
    fn withdraw(&self) -> DomainResult<()>;
    fn stop(&self);
}

/// Why an outgoing call to a peer failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerCallError {
    Unreachable(String),
    NotPaired,
    Busy,
    Rejected(u16),
    IdentityMismatch,
    Protocol(String),
}

/// Why the local handler refuses a peer's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerError {
    NotFound,
    NotPaired,
    Busy,
    Forbidden,
    BadRequest,
    Internal,
}

impl PeerError {
    /// The HTTP status the server sends: 404, 403, 503, 403, 400, 500.
    pub fn status_code(&self) -> u16 {
        match self {
            PeerError::NotFound => 404,
            PeerError::NotPaired => 403,
            PeerError::Busy => 503,
            PeerError::Forbidden => 403,
            PeerError::BadRequest => 400,
            PeerError::Internal => 500,
        }
    }
}

/// An open peer WebSocket, after both Hello frames were exchanged.
pub struct PeerSocket {
    pub peer: PeerIdentity,
    pub dialed_by: DeviceId,
    /// The peer's Hello frame.
    pub remote_hello: PeerMessage,
    pub outbound: mpsc::Sender<PeerMessage>,
    /// Closes when the socket closes.
    pub inbound: mpsc::Receiver<PeerMessage>,
}

/// What the peer server calls. Implemented by the service.
#[async_trait]
pub trait PeerHandler: Send + Sync {
    async fn hello(&self) -> HelloResponse;
    async fn pair_request(
        &self,
        peer: PeerIdentity,
        remote_ip: IpAddr,
        req: PairRequest,
    ) -> Result<PairRequestResponse, PeerError>;
    async fn pair_nonce(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        msg: NonceMessage,
    ) -> Result<NonceMessage, PeerError>;
    async fn pair_decision(
        &self,
        peer: PeerIdentity,
        id: PairingId,
        req: DecisionRequest,
    ) -> Result<(), PeerError>;
    async fn pair_status(
        &self,
        peer: PeerIdentity,
        id: PairingId,
    ) -> Result<PairStatusResponse, PeerError>;
    /// True when the device is paired and not blocked.
    async fn is_paired(&self, device: &DeviceId) -> bool;
    /// Takes ownership of an open socket, after the Hello frames were exchanged.
    async fn on_peer_socket(&self, socket: PeerSocket);
    /// The `PeerMessage::Hello` for the first frame.
    fn local_hello(&self) -> PeerMessage;
}

/// What the service calls to reach a peer. Implemented by infra.
#[async_trait]
pub trait PeerConnector: Send + Sync {
    async fn hello(&self, addr: SocketAddr)
        -> Result<(PeerIdentity, HelloResponse), PeerCallError>;
    async fn pair_request(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        req: &PairRequest,
    ) -> Result<(PeerIdentity, PairRequestResponse), PeerCallError>;
    async fn pair_nonce(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        msg: &NonceMessage,
    ) -> Result<NonceMessage, PeerCallError>;
    async fn pair_decision(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
        req: &DecisionRequest,
    ) -> Result<(), PeerCallError>;
    async fn pair_status(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        id: &PairingId,
    ) -> Result<PairStatusResponse, PeerCallError>;
    async fn open_socket(
        &self,
        addr: SocketAddr,
        expected: &DeviceId,
        hello: PeerMessage,
    ) -> Result<PeerSocket, PeerCallError>;
}

#[async_trait]
pub trait PeerServerControl: Send + Sync {
    /// Binds `0.0.0.0` on `DEFAULT_PORT..=PORT_RANGE_END` and returns the port.
    /// Errors when no port in the range is free.
    async fn start(&self, handler: Arc<dyn PeerHandler>) -> DomainResult<u16>;
    fn stop(&self);
}

/// The pieces the service needs once the device key is known.
pub struct PeerTransport {
    pub local: PeerIdentity,
    pub connector: Arc<dyn PeerConnector>,
    pub server: Arc<dyn PeerServerControl>,
}

pub trait PeerTransportFactory: Send + Sync {
    fn build(&self, key: &DeviceKeyMaterial) -> DomainResult<PeerTransport>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn assert_dyn<T: ?Sized>() {}

    #[test]
    fn every_port_is_object_safe() {
        assert_dyn::<dyn DeviceIdentityStore>();
        assert_dyn::<dyn PairedDeviceRepository>();
        assert_dyn::<dyn BlockedDeviceRepository>();
        assert_dyn::<dyn ManualAddressRepository>();
        assert_dyn::<dyn NetworkSettingsRepository>();
        assert_dyn::<dyn QuarantineReport>();
        assert_dyn::<dyn DiscoverySink>();
        assert_dyn::<dyn Discovery>();
        assert_dyn::<dyn PeerHandler>();
        assert_dyn::<dyn PeerConnector>();
        assert_dyn::<dyn PeerServerControl>();
        assert_dyn::<dyn PeerTransportFactory>();
    }

    #[test]
    fn peer_errors_map_to_http_status_codes() {
        assert_eq!(PeerError::NotFound.status_code(), 404);
        assert_eq!(PeerError::NotPaired.status_code(), 403);
        assert_eq!(PeerError::Busy.status_code(), 503);
        assert_eq!(PeerError::Forbidden.status_code(), 403);
        assert_eq!(PeerError::BadRequest.status_code(), 400);
        assert_eq!(PeerError::Internal.status_code(), 500);
    }

    struct Collecting(Mutex<Vec<DiscoveryEvent>>);

    impl DiscoverySink for Collecting {
        fn on_event(&self, event: DiscoveryEvent) {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).push(event);
        }
    }

    #[test]
    fn a_sink_receives_events_through_the_trait_object() {
        let sink = Arc::new(Collecting(Mutex::new(Vec::new())));
        let dynamic: Arc<dyn DiscoverySink> = sink.clone();
        let id = DeviceId::from_bytes([1; 32]);
        dynamic.on_event(DiscoveryEvent::Gone { claimed_id: id });
        dynamic.on_event(DiscoveryEvent::Seen {
            claimed_id: id,
            profile: NodeProfile::new("Ada").expect("valid"),
            addresses: vec!["127.0.0.1:47821".parse().expect("addr")],
            version: 1,
        });
        let seen = sink.0.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], DiscoveryEvent::Gone { claimed_id: id });
    }

    #[test]
    fn announcements_compare_by_value() {
        let a = Announcement {
            device_id: DeviceId::from_bytes([1; 32]),
            profile: NodeProfile::new("Ada").expect("valid"),
            port: 47821,
            version: 1,
        };
        assert_eq!(a.clone(), a);
    }

    #[tokio::test]
    async fn peer_socket_channels_carry_messages_both_ways() {
        let (out_tx, mut out_rx) = mpsc::channel::<PeerMessage>(4);
        let (in_tx, in_rx) = mpsc::channel::<PeerMessage>(4);
        let mut socket = PeerSocket {
            peer: PeerIdentity::from_spki(vec![1; 44]),
            dialed_by: DeviceId::from_bytes([1; 32]),
            remote_hello: PeerMessage::Hello {
                version: 1,
                profile: NodeProfile::new("Ada").expect("valid"),
            },
            outbound: out_tx,
            inbound: in_rx,
        };

        socket
            .outbound
            .send(PeerMessage::Ping)
            .await
            .expect("outbound open");
        assert_eq!(out_rx.recv().await, Some(PeerMessage::Ping));

        in_tx.send(PeerMessage::Pong).await.expect("inbound open");
        assert_eq!(socket.inbound.recv().await, Some(PeerMessage::Pong));

        drop(in_tx);
        assert_eq!(socket.inbound.recv().await, None);
    }

    #[test]
    fn peer_call_errors_compare_by_value() {
        assert_eq!(PeerCallError::Rejected(403), PeerCallError::Rejected(403));
        assert_ne!(PeerCallError::NotPaired, PeerCallError::Busy);
        assert_ne!(
            PeerCallError::Unreachable("a".to_string()),
            PeerCallError::Unreachable("b".to_string())
        );
    }
}
