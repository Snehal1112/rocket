//! Team network: discovery, pairing and presence for Rocket nodes.
//!
//! `NetworkService` owns the lifecycle. It talks to the outside only through the ports in
//! `rocket-network`, so tests run with fakes and no sockets.

mod connections;
mod handler;
mod nodes;
mod pairing;
mod service;
mod status;
// Plan 04b reuses these fakes, so some helpers are unused here.
#[cfg(test)]
#[allow(dead_code, clippy::type_complexity)]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "connections_tests.rs"]
mod connections_tests;
#[cfg(test)]
#[path = "e2e_tests.rs"]
mod e2e_tests;
#[cfg(test)]
#[path = "pairing_tests.rs"]
mod pairing_tests;
#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;

pub use nodes::{NodeGroup, NodeStatus, NodeView};
pub use pairing::{PairingDecision, PairingStarted};
pub use service::{paired_limit_message, NetworkDeps, NetworkHandle, NetworkService};
pub use status::{NetworkState, NetworkStatus};
