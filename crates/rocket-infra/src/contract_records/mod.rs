//! YAML persistence records for contract types.
//!
//! Records live here (the adapter layer) so domain types in `rocket-collection`
//! stay free of serde wire-format concerns. Records use camelCase YAML for
//! on-disk compatibility with files written by previous versions; back-compat
//! custom Deserialize impls live with the Record types they target.

pub mod changelog;
pub mod snapshot;
pub mod types;
