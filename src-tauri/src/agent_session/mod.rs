//! Per-session resources of isolated agent sessions: scratch directories and
//! the `SessionCleanup` implementation that releases them.

pub mod cleanup;
pub mod scratch;
