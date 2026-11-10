//! Team network: discovery, pairing and presence for Rocket nodes.
//!
//! `NetworkService` owns the lifecycle. It talks to the outside only through the ports in
//! `rocket-network`, so tests run with fakes and no sockets.

mod connections;
pub mod contract_check_match;
mod frame_router;
mod handler;
mod nodes;
mod pairing;
mod service;
mod status;
mod sync;
mod sync_checks;
pub mod sync_collections;
mod sync_payload;
mod sync_planner;
pub mod sync_ports;
mod sync_starter;
mod transfer;
mod transfer_link;
pub mod transfer_ports;
mod transfer_receive;
mod transfer_send;
// Plan 04b reuses these fakes, so some helpers are unused here.
#[cfg(test)]
#[allow(dead_code, clippy::type_complexity)]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "connections_tests.rs"]
mod connections_tests;
#[cfg(test)]
#[path = "contract_check_match_tests.rs"]
mod contract_check_match_tests;
#[cfg(test)]
#[path = "e2e_sync_tests.rs"]
mod e2e_sync_tests;
#[cfg(test)]
#[path = "e2e_tests.rs"]
mod e2e_tests;
#[cfg(test)]
#[path = "e2e_transfer_tests.rs"]
mod e2e_transfer_tests;
#[cfg(test)]
#[path = "pairing_tests.rs"]
mod pairing_tests;
#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;
#[cfg(test)]
#[path = "sync_collections_tests.rs"]
mod sync_collections_tests;
#[cfg(test)]
#[path = "transfer_link_tests.rs"]
mod transfer_link_tests;
// The sync tests of later tasks reuse these fakes, so some helpers are unused here.
#[cfg(test)]
#[allow(dead_code, clippy::type_complexity)]
pub(crate) mod sync_fakes;
#[cfg(test)]
#[path = "sync_first_link_tests.rs"]
mod sync_first_link_tests;
#[cfg(test)]
#[path = "sync_run_tests.rs"]
mod sync_run_tests;
#[cfg(test)]
#[path = "sync_safety_tests.rs"]
mod sync_safety_tests;
#[cfg(test)]
#[path = "sync_tests.rs"]
mod sync_tests;
// The transfer fakes are shared by the send and receive tests.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) mod transfer_fakes;
#[cfg(test)]
#[path = "transfer_receive_tests.rs"]
mod transfer_receive_tests;
#[cfg(test)]
#[path = "transfer_send_tests.rs"]
mod transfer_send_tests;

pub use frame_router::FrameRouter;
pub use nodes::{NodeGroup, NodeStatus, NodeView};
pub use pairing::{PairingDecision, PairingStarted};
pub use service::{paired_limit_message, NetworkDeps, NetworkHandle, NetworkService};
pub use status::{NetworkState, NetworkStatus};
pub use sync::{ConflictFiles, LinkView, SyncDeps, SyncService};
pub use sync_collections::ExportBackedSyncCollections;
pub use sync_ports::{
    base_from_reader, base_from_zip, is_sync_entry, manifest_from_zip, sha256_hex, FileChange,
    SyncCollectionPort, SyncFile, ZipBase,
};
pub use transfer::{PeerSender, Timing, TransferDeps, TransferService};
pub use transfer_ports::{
    receiver_display_name, CollectionImportPort, ImportedCollection, StagedFile, StagedZip,
    TransferStaging, UNSAFE_ZIP_MESSAGE,
};
pub use transfer_send::SendStarted;
