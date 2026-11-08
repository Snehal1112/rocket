// The async_trait macro triggers this lint on newer toolchains.
#![allow(clippy::double_must_use)]
pub mod device;
pub mod dialer;
pub mod events;
pub mod identity;
pub mod limits;
pub mod link;
pub mod messages;
pub mod pairing;
pub mod ports;
pub mod presence;
pub mod profile;
pub mod rate;
pub mod records;
pub mod staging;
pub mod transfer;
pub mod wire;

pub use device::DeviceId;
pub use dialer::{keep_dialed_by, should_dial, DialHint};
pub use events::{NetworkEvent, TransferEvent};
pub use identity::{DeviceKeyMaterial, PeerIdentity};
pub use link::{is_sha256_hex, path_is_valid, LinkId, LinkRecord, LinkState, Manifest, SyncId};
pub use messages::{AbortReason, PeerMessage, ResultOutcome};
pub use pairing::{
    commitment, commitment_from_base64, commitment_to_base64, pairing_code, Nonce, PairingCode,
    PairingId, PairingOutcome, PairingRole, PairingSession, PairingState, PairingStatus,
};
pub use ports::{
    Announcement, BlockedDeviceRepository, DeviceIdentityStore, Discovery, DiscoveryEvent,
    DiscoverySink, ManualAddressRepository, NetworkSettingsRepository, PairedDeviceRepository,
    PeerCallError, PeerConnector, PeerError, PeerHandler, PeerServerControl, PeerSocket,
    PeerTransport, PeerTransportFactory, QuarantineReport,
};
pub use presence::{backoff, Presence, PresenceTracker};
pub use profile::{default_display_name, initials_for, NodeProfile};
pub use rate::SlidingWindow;
pub use records::{
    BlockedDevice, DiscoveredNode, DiscoverySource, ManualAddress, NetworkSettings, PairedDevice,
};
pub use staging::{StagedFile, StagedZip, TransferStaging};
pub use transfer::{
    chunk_count_for, offer_name_is_valid, Admission, ChunkStep, Direction, IncomingState,
    IncomingTransfer, OfferError, Outcome, OutgoingState, OutgoingTransfer, TransferId,
    TransferLimits, TransferSink,
};
pub use wire::{
    DecisionRequest, HelloResponse, NonceMessage, PairRequest, PairRequestResponse,
    PairStatusResponse,
};
