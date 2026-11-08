//! Protocol constants for the Team Network.

use std::time::Duration;

/// The peer protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = 3;
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
/// The largest collection ZIP a node sends or accepts.
pub const MAX_TRANSFER_BYTES: u64 = 10 * 1024 * 1024;
/// Raw bytes in one chunk frame, before base64.
pub const CHUNK_BYTES: usize = 32 * 1024;
/// How long an offer waits for an answer, counted from its creation.
pub const TRANSFER_OFFER_TTL: Duration = Duration::from_secs(120);
/// Offers waiting for the user across all peers.
pub const MAX_PENDING_INCOMING: usize = 3;
/// Transfers per peer at a time, in each direction.
pub const MAX_ACTIVE_PER_PEER: usize = 1;
/// Offers accepted from one peer per minute.
pub const OFFERS_PER_PEER_PER_MINUTE: u32 = 5;
/// The most bytes a received ZIP may unzip to.
pub const MAX_UNZIPPED_BYTES: u64 = 50 * 1024 * 1024;
/// The most entries a received ZIP may hold.
pub const MAX_ZIP_ENTRIES: usize = 5000;
/// How long the sender waits for the receiver's `TransferResult` after the receiver's final
/// `ChunkAck`. When it passes, the sender reports `Unconfirmed`, not a failure.
pub const SENDER_RESULT_TIMEOUT: Duration = Duration::from_secs(60);
/// The receiver sends a `ChunkAck` after this many chunks accepted in order, and after the last.
pub const ACK_EVERY_CHUNKS: u32 = 16;
/// The most chunks the sender may have sent and not yet seen acknowledged.
pub const MAX_UNACKED_CHUNKS: u32 = 32;
/// The most files one manifest lists. Equals `MAX_ZIP_ENTRIES`.
pub const MAX_MANIFEST_ENTRIES: usize = 5000;
/// The longest name a sync payload transfer carries in its offer.
pub const SYNC_PAYLOAD_NAME_MAX: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn spec_values_are_pinned() {
        assert_eq!(PROTOCOL_VERSION, 3);
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
        assert_eq!(MAX_TRANSFER_BYTES, 10 * 1024 * 1024);
        assert_eq!(CHUNK_BYTES, 32 * 1024);
        assert_eq!(TRANSFER_OFFER_TTL, Duration::from_secs(120));
        assert_eq!(MAX_PENDING_INCOMING, 3);
        assert_eq!(MAX_ACTIVE_PER_PEER, 1);
        assert_eq!(OFFERS_PER_PEER_PER_MINUTE, 5);
        assert_eq!(MAX_UNZIPPED_BYTES, 50 * 1024 * 1024);
        assert_eq!(MAX_ZIP_ENTRIES, 5000);
        assert_eq!(SENDER_RESULT_TIMEOUT, Duration::from_secs(60));
        assert_eq!(ACK_EVERY_CHUNKS, 16);
        assert_eq!(MAX_UNACKED_CHUNKS, 32);
        assert_eq!(MAX_MANIFEST_ENTRIES, 5000);
        assert_eq!(SYNC_PAYLOAD_NAME_MAX, 32);
    }

    #[test]
    fn transfer_limits_are_consistent() {
        assert_eq!(TRANSFER_OFFER_TTL, PAIRING_TTL);
        assert_eq!(OFFERS_PER_PEER_PER_MINUTE, PAIR_REQUESTS_PER_MINUTE);
        const _: () = assert!(MAX_TRANSFER_BYTES < MAX_UNZIPPED_BYTES);
        // The sender gives up on the result sooner than an offer waits for its answer.
        const _: () = assert!(SENDER_RESULT_TIMEOUT.as_secs() < TRANSFER_OFFER_TTL.as_secs());
        // The receiver acks twice per window, so a sender never stalls on a healthy link.
        const _: () = assert!(ACK_EVERY_CHUNKS < MAX_UNACKED_CHUNKS);
        assert_eq!(MAX_UNACKED_CHUNKS % ACK_EVERY_CHUNKS, 0);
    }

    #[test]
    fn a_full_chunk_fits_a_frame_with_room_for_the_envelope() {
        // Base64 grows 3 raw bytes to 4 characters, padded.
        let encoded = CHUNK_BYTES.div_ceil(3) * 4;
        assert_eq!(encoded, 43_692);
        // The envelope is the type tag, a 32 character id and a sequence number.
        assert!(encoded + 200 < MAX_FRAME_BYTES);
    }

    #[test]
    fn sync_limits_follow_the_zip_limit() {
        // A manifest lists the files of one collection ZIP, so it can never hold more.
        assert_eq!(MAX_MANIFEST_ENTRIES, MAX_ZIP_ENTRIES);
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
