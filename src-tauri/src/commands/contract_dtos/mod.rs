//! IPC DTOs for contract commands.
//!
//! DTOs sit at the IPC adapter layer. Domain types in `rocket-collection` and
//! `rocket-app` carry no serde — all wire-format concerns live here. Each DTO
//! has `From` impls bridging it to its domain counterpart.

pub mod changelog;
pub mod snapshot;
pub mod summary;
pub mod types;
