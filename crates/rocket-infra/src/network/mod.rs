//! Team network pieces that touch the machine: the device key, the local lists, the TLS
//! configs, the peer server and connector, mDNS and the transport factory. The domain rules
//! live in `rocket-network`.

pub mod client;
pub mod identity_store;
pub(crate) mod lock;
#[cfg(test)]
pub(crate) mod peer_test_support;
pub mod repos;
pub mod server;
pub(crate) mod socket;
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
