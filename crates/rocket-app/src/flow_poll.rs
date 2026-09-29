//! Repeat until: sends one Request node's request until its condition holds.

/// How a successful repeat-until poll went, for its step result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))] // Used by Task 3's poll loop.
pub(crate) struct PollStats {
    pub(crate) attempts: u32,
    /// Time from the first send to the attempt that met the condition.
    pub(crate) elapsed_ms: u64,
}
