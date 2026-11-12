//! Limits for script-driven gRPC calls.

use std::time::Duration;

/// Most response messages a script call keeps.
pub const MAX_RESPONSE_MESSAGES: usize = 1000;
/// Most response bytes a script call keeps.
pub const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
/// Most input messages a script call may send.
pub const MAX_INPUT_MESSAGES: usize = 1000;
/// Timeout used when a call asks for none.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest timeout a call may ask for.
pub const MAX_CALL_TIMEOUT: Duration = Duration::from_secs(300);

/// Turns a requested timeout in milliseconds into the one the call uses.
pub fn clamp_call_timeout(requested_ms: Option<u64>) -> Duration {
    match requested_ms {
        None | Some(0) => DEFAULT_CALL_TIMEOUT,
        Some(ms) => Duration::from_millis(ms).min(MAX_CALL_TIMEOUT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_call_timeout_uses_the_default_for_none_and_zero() {
        assert_eq!(clamp_call_timeout(None), DEFAULT_CALL_TIMEOUT);
        assert_eq!(clamp_call_timeout(Some(0)), DEFAULT_CALL_TIMEOUT);
    }

    #[test]
    fn clamp_call_timeout_caps_at_five_minutes() {
        assert_eq!(clamp_call_timeout(Some(300_001)), MAX_CALL_TIMEOUT);
        assert_eq!(clamp_call_timeout(Some(u64::MAX)), MAX_CALL_TIMEOUT);
    }

    #[test]
    fn clamp_call_timeout_keeps_a_valid_value() {
        assert_eq!(clamp_call_timeout(Some(1500)), Duration::from_millis(1500));
        assert_eq!(clamp_call_timeout(Some(300_000)), MAX_CALL_TIMEOUT);
    }
}
