//! Team network pieces that touch the machine: the device key, the local lists and the TLS
//! configs. The domain rules live in `rocket-network`.

pub mod identity_store;
pub(crate) mod lock;
pub mod repos;
pub mod tls_config;
pub mod tls_identity;

pub use identity_store::KeychainDeviceIdentity;
pub use repos::{
    FsBlockedDeviceRepo, FsManualAddressRepo, FsNetworkSettingsRepo, FsPairedDeviceRepo,
};
pub use tls_config::{client_config, peer_identity_from_certs, server_config};
pub use tls_identity::{build_tls_identity, spki_from_cert, TlsIdentity};
