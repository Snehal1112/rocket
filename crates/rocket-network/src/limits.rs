//! Protocol constants for the Team Network.

use std::time::Duration;

/// The peer protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;
/// The mDNS service type.
pub const MDNS_SERVICE_TYPE: &str = "_rocket._tcp.local.";
/// The first port the peer server tries.
pub const DEFAULT_PORT: u16 = 47821;
/// The last port the peer server tries.
pub const PORT_RANGE_END: u16 = 47830;
/// The most paired devices one node keeps.
pub const MAX_PAIRED_DEVICES: usize = 200;
/// The most manual addresses one node keeps.
pub const MAX_MANUAL_ADDRESSES: usize = 50;
/// The most concurrent connections the peer server accepts.
pub const MAX_CONNECTIONS: usize = 256;
/// The most concurrent connections from unpaired keys.
pub const MAX_UNPAIRED_CONNECTIONS: usize = 32;
/// How long a client may take to send its request headers.
pub const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// The largest REST request body.
pub const MAX_REST_BODY_BYTES: usize = 16 * 1024;
/// The largest WebSocket text frame.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// `/hello` requests allowed per IP per minute.
pub const HELLO_PER_IP_PER_MINUTE: u32 = 30;
/// Pairing requests allowed per minute across all strangers.
pub const PAIR_REQUESTS_PER_MINUTE: u32 = 5;
/// How long a pairing session lives, counted from its creation.
pub const PAIRING_TTL: Duration = Duration::from_secs(120);
/// How long a declined device must wait before it may ask again.
pub const DECLINE_COOLDOWN: Duration = Duration::from_secs(60);
/// How often each side sends a ping.
pub const PING_INTERVAL: Duration = Duration::from_secs(20);
/// Unanswered pings that are tolerated before a peer counts as offline.
pub const MISSED_PINGS_OFFLINE: u32 = 2;
/// The longest reconnect delay.
pub const BACKOFF_MAX: Duration = Duration::from_secs(120);
/// How long the higher device id waits before it dials too.
pub const DIAL_GRACE: Duration = Duration::from_secs(10);
/// How often a manual address is probed.
pub const MANUAL_PROBE_INTERVAL: Duration = Duration::from_secs(60);
/// How long an mDNS record lives without a refresh.
pub const MDNS_RECORD_TTL: Duration = Duration::from_secs(120);
/// The longest display name, in characters.
pub const DISPLAY_NAME_MAX_CHARS: usize = 40;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn spec_values_are_pinned() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert_eq!(MDNS_SERVICE_TYPE, "_rocket._tcp.local.");
        assert_eq!(MAX_PAIRED_DEVICES, 200);
        assert_eq!(MAX_MANUAL_ADDRESSES, 50);
        assert_eq!(MAX_CONNECTIONS, 256);
        assert_eq!(MAX_UNPAIRED_CONNECTIONS, 32);
        assert_eq!(MAX_REST_BODY_BYTES, 16 * 1024);
        assert_eq!(MAX_FRAME_BYTES, 64 * 1024);
        assert_eq!(PAIR_REQUESTS_PER_MINUTE, 5);
        assert_eq!(HELLO_PER_IP_PER_MINUTE, 30);
        assert_eq!(PAIRING_TTL, Duration::from_secs(120));
        assert_eq!(DECLINE_COOLDOWN, Duration::from_secs(60));
        assert_eq!(PING_INTERVAL, Duration::from_secs(20));
        assert_eq!(MISSED_PINGS_OFFLINE, 2);
        assert_eq!(BACKOFF_MAX, Duration::from_secs(120));
        assert_eq!(DIAL_GRACE, Duration::from_secs(10));
        assert_eq!(MDNS_RECORD_TTL, Duration::from_secs(120));
        assert_eq!(DISPLAY_NAME_MAX_CHARS, 40);
    }

    #[test]
    fn port_range_holds_ten_ports() {
        let (start, end) = (DEFAULT_PORT, PORT_RANGE_END);
        assert_eq!(start, 47821);
        assert_eq!(end - start + 1, 10);
    }

    #[test]
    fn unpaired_connections_stay_below_the_total() {
        let (unpaired, total) = (MAX_UNPAIRED_CONNECTIONS, MAX_CONNECTIONS);
        assert!(unpaired < total);
        let (rest, frame) = (MAX_REST_BODY_BYTES, MAX_FRAME_BYTES);
        assert!(rest < frame);
    }
}
