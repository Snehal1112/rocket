//! Presence bookkeeping and the reconnect backoff. Pure; the caller owns the timers.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::limits::{BACKOFF_MAX, MISSED_PINGS_OFFLINE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Unknown,
    Online,
    Offline,
}

/// Tracks one peer WebSocket: the state, the missed pings and the reconnect attempt.
#[derive(Debug, Clone)]
pub struct PresenceTracker {
    state: Presence,
    attempt: u32,
    missed: u32,
}

impl PresenceTracker {
    pub fn new() -> PresenceTracker {
        PresenceTracker {
            state: Presence::Unknown,
            attempt: 0,
            missed: 0,
        }
    }

    pub fn state(&self) -> Presence {
        self.state
    }

    /// The socket is open.
    pub fn on_connected(&mut self) {
        self.state = Presence::Online;
        self.attempt = 0;
        self.missed = 0;
    }

    /// A pong (or any frame the caller counts as alive) arrived.
    pub fn on_pong(&mut self) {
        self.missed = 0;
    }

    /// A ping was sent. More unanswered pings than `MISSED_PINGS_OFFLINE` means offline.
    pub fn on_ping_sent(&mut self) -> Presence {
        self.missed = self.missed.saturating_add(1);
        if self.state == Presence::Online && self.missed > MISSED_PINGS_OFFLINE {
            self.state = Presence::Offline;
        }
        self.state
    }

    /// The socket closed or a dial failed.
    pub fn on_disconnected(&mut self) {
        self.state = Presence::Offline;
        self.attempt = self.attempt.saturating_add(1);
        self.missed = 0;
    }

    pub fn next_retry_delay(&self) -> Duration {
        backoff(self.attempt)
    }
}

impl Default for PresenceTracker {
    fn default() -> Self {
        PresenceTracker::new()
    }
}

/// Reconnect delay: attempts 0 and 1 wait 1 s, then it doubles up to `BACKOFF_MAX`.
pub fn backoff(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(10);
    Duration::from_secs(1u64 << exponent).min(BACKOFF_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_tracker_is_unknown_with_a_one_second_retry() {
        let tracker = PresenceTracker::new();
        assert_eq!(tracker.state(), Presence::Unknown);
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(1));
        assert_eq!(PresenceTracker::default().state(), Presence::Unknown);
    }

    #[test]
    fn connecting_goes_online_and_resets_the_counters() {
        let mut tracker = PresenceTracker::new();
        tracker.on_disconnected();
        tracker.on_disconnected();
        tracker.on_disconnected();
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(4));
        tracker.on_connected();
        assert_eq!(tracker.state(), Presence::Online);
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(1));
    }

    #[test]
    fn the_third_unanswered_ping_goes_offline() {
        let mut tracker = PresenceTracker::new();
        tracker.on_connected();
        assert_eq!(tracker.on_ping_sent(), Presence::Online);
        assert_eq!(tracker.on_ping_sent(), Presence::Online);
        assert_eq!(tracker.on_ping_sent(), Presence::Offline);
        assert_eq!(tracker.state(), Presence::Offline);
    }

    #[test]
    fn a_pong_resets_the_missed_count() {
        let mut tracker = PresenceTracker::new();
        tracker.on_connected();
        tracker.on_ping_sent();
        tracker.on_ping_sent();
        tracker.on_pong();
        assert_eq!(tracker.on_ping_sent(), Presence::Online);
        assert_eq!(tracker.on_ping_sent(), Presence::Online);
        assert_eq!(tracker.on_ping_sent(), Presence::Offline);
    }

    #[test]
    fn pings_do_not_change_an_unknown_tracker() {
        let mut tracker = PresenceTracker::new();
        for _ in 0..5 {
            assert_eq!(tracker.on_ping_sent(), Presence::Unknown);
        }
    }

    #[test]
    fn disconnecting_goes_offline_and_grows_the_backoff() {
        let mut tracker = PresenceTracker::new();
        tracker.on_connected();
        tracker.on_disconnected();
        assert_eq!(tracker.state(), Presence::Offline);
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(1));
        tracker.on_disconnected();
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(2));
        tracker.on_disconnected();
        assert_eq!(tracker.next_retry_delay(), Duration::from_secs(4));
    }

    #[test]
    fn backoff_sequence_doubles_and_caps() {
        let seconds: Vec<u64> = (0..10).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(seconds, [1, 1, 2, 4, 8, 16, 32, 64, 120, 120]);
        assert_eq!(backoff(u32::MAX), BACKOFF_MAX);
    }

    #[test]
    fn presence_serializes_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&Presence::Online).expect("serializes"),
            "\"online\""
        );
        assert_eq!(
            serde_json::to_string(&Presence::Unknown).expect("serializes"),
            "\"unknown\""
        );
    }
}
