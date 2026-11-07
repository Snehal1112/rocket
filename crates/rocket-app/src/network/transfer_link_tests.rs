//! Tests for `send_to`, the transfer sink hook and `peer_label`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rocket_network::limits::{PING_INTERVAL, PROTOCOL_VERSION};
use rocket_network::{AbortReason, DeviceId, PeerMessage, ResultOutcome, TransferSink};
use rocket_shared::error::DomainError;

use super::test_support::*;
use super::{NetworkHandle, NodeStatus};

#[derive(Default)]
struct RecordingSink {
    frames: Mutex<Vec<(DeviceId, PeerMessage)>>,
    gone: Mutex<Vec<DeviceId>>,
}

impl RecordingSink {
    fn frames(&self) -> Vec<(DeviceId, PeerMessage)> {
        self.frames.lock().expect("sink lock").clone()
    }

    fn gone(&self) -> Vec<DeviceId> {
        self.gone.lock().expect("sink lock").clone()
    }

    /// The sink was told at least once, and only about `device`.
    fn gone_only(&self, device: &DeviceId) -> bool {
        let gone = self.gone();
        !gone.is_empty() && gone.iter().all(|d| d == device)
    }
}

impl TransferSink for RecordingSink {
    fn on_frame(&self, from: DeviceId, message: PeerMessage) {
        self.frames.lock().expect("sink lock").push((from, message));
    }

    fn on_peer_gone(&self, device: DeviceId) {
        self.gone.lock().expect("sink lock").push(device);
    }
}

/// A node whose only paired device is `peer_seed`, reachable at its fixed route.
async fn solo(peer_seed: u8) -> (Node, NetworkHandle) {
    let a = node(1, "ann");
    a.paired.seed(identity(peer_seed).device_id, "Peer");
    a.connector
        .add_route(addr(peer_seed), Route::fixed(identity(peer_seed), "Peer"));
    let handle = a.svc.start().await;
    (a, handle)
}

/// Queues a scripted socket and adds the manual address, so the node dials and gets it.
async fn go_online(a: &Node, peer_seed: u8) -> Remote {
    let (socket, remote) = scripted_socket(identity(peer_seed), a.id(), PROTOCOL_VERSION);
    a.connector.push_socket(socket);
    a.svc
        .add_address("127.0.0.1", addr(peer_seed).port())
        .await
        .expect("add address");
    settle().await;
    remote
}

fn peer() -> DeviceId {
    identity(2).device_id
}

fn transfer_id() -> String {
    "0".repeat(32)
}

/// One frame of each of the six transfer variants (the chunk is at index 2, the abort at 3,
/// the ack at 5).
fn frames() -> Vec<PeerMessage> {
    vec![
        PeerMessage::CollectionOffer {
            transfer_id: transfer_id(),
            name: "Orders".into(),
            size_bytes: 10,
            chunk_count: 1,
            sha256: "0".repeat(64),
        },
        PeerMessage::OfferAnswer {
            transfer_id: transfer_id(),
            accept: true,
        },
        PeerMessage::Chunk {
            transfer_id: transfer_id(),
            seq: 0,
            data: "AAAA".into(),
        },
        PeerMessage::TransferAbort {
            transfer_id: transfer_id(),
            reason: AbortReason::Cancelled,
        },
        PeerMessage::TransferResult {
            transfer_id: transfer_id(),
            outcome: ResultOutcome::Completed,
        },
        PeerMessage::ChunkAck {
            transfer_id: transfer_id(),
            next_seq: 16,
        },
    ]
}

#[tokio::test(start_paused = true)]
async fn the_six_transfer_frames_reach_the_sink_with_the_sender_id() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;

    for frame in frames() {
        remote.to_service.send(frame).await.expect("frame sent");
    }
    settle().await;

    let got = sink.frames();
    assert!(got.iter().all(|(from, _)| *from == peer()));
    let messages: Vec<PeerMessage> = got.into_iter().map(|(_, m)| m).collect();
    assert_eq!(messages, frames(), "all six arrive, in order");
}

#[tokio::test(start_paused = true)]
async fn a_frame_from_a_blocked_device_never_reaches_the_sink() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;
    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));

    a.blocked.put(peer(), "Peer");
    // The offer is checked against the block list (chunks and acks are not, see below).
    remote
        .to_service
        .send(frames().remove(0))
        .await
        .expect("frame sent");
    settle().await;

    assert!(sink.frames().is_empty(), "the sink saw nothing");
    assert_eq!(
        row_status(&a, &peer()),
        Some(NodeStatus::Offline),
        "the connection was removed first"
    );
    let err = a
        .svc
        .send_to(&peer(), frames().remove(3))
        .await
        .expect_err("no connection is left");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test(start_paused = true)]
async fn frames_without_a_sink_are_dropped_quietly() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2).await;

    remote
        .to_service
        .send(frames().remove(0))
        .await
        .expect("frame sent");
    settle().await;

    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn send_to_an_offline_peer_is_refused() {
    let (a, _h) = solo(2).await;

    let err = a
        .svc
        .send_to(&peer(), frames().remove(3))
        .await
        .expect_err("offline");

    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is offline."));
}

#[tokio::test(start_paused = true)]
async fn send_to_an_online_peer_delivers_the_frame() {
    let (a, _h) = solo(2).await;
    let mut remote = go_online(&a, 2).await;
    let message = frames().remove(1);

    a.svc
        .send_to(&peer(), message.clone())
        .await
        .expect("delivered");

    assert_eq!(remote.from_service.try_recv(), Ok(message));
}

#[tokio::test(start_paused = true)]
async fn send_to_gives_up_when_the_peer_does_not_read() {
    let (a, _h) = solo(2).await;
    // The remote end is kept but never read, so the 16-slot channel fills up.
    let _remote = go_online(&a, 2).await;
    for _ in 0..16 {
        a.svc
            .send_to(&peer(), frames().remove(1))
            .await
            .expect("room left");
    }

    let started = tokio::time::Instant::now();
    let err = a
        .svc
        .send_to(&peer(), frames().remove(1))
        .await
        .expect_err("channel full");

    assert!(matches!(err, DomainError::Io(_)));
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn send_to_a_closed_socket_reads_as_offline() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2).await;
    drop(remote);
    settle().await;

    let err = a
        .svc
        .send_to(&peer(), frames().remove(1))
        .await
        .expect_err("closed");

    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is offline."));
}

#[tokio::test(start_paused = true)]
async fn peer_label_names_a_paired_unblocked_device_only() {
    let (a, _h) = solo(2).await;

    assert_eq!(
        a.svc.peer_label(&peer()),
        Some(("Peer".to_string(), peer().short()))
    );
    assert_eq!(a.svc.peer_label(&identity(9).device_id), None, "not paired");

    a.blocked.put(peer(), "Peer");
    assert_eq!(a.svc.peer_label(&peer()), None, "blocked");
}

#[tokio::test(start_paused = true)]
async fn a_socket_that_ends_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;
    assert!(sink.gone().is_empty(), "a live socket is not gone");

    drop(remote);
    settle().await;

    assert!(sink.gone_only(&peer()), "the sink was told about the peer");
}

#[tokio::test(start_paused = true)]
async fn a_peer_unpair_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;

    remote
        .to_service
        .send(PeerMessage::Unpair)
        .await
        .expect("frame sent");
    settle().await;

    assert!(sink.gone_only(&peer()));
}

#[tokio::test(start_paused = true)]
async fn removing_the_pairing_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let _remote = go_online(&a, 2).await;

    a.svc.remove_paired(&peer()).await.expect("removed");
    settle().await;

    assert!(sink.gone_only(&peer()));
}

#[tokio::test(start_paused = true)]
async fn removing_an_offline_pairing_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());

    a.svc.remove_paired(&peer()).await.expect("removed");
    settle().await;

    assert!(
        sink.gone_only(&peer()),
        "no socket was open, the pairing went"
    );
}

#[tokio::test(start_paused = true)]
async fn transfer_frames_keep_a_peer_online_without_pongs() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;

    // The remote never answers a ping. Three unanswered pings would mark it offline, but a
    // chunk after every ping counts as the answer.
    for _ in 0..4 {
        tokio::time::advance(PING_INTERVAL).await;
        settle().await;
        remote
            .to_service
            .send(frames().remove(2))
            .await
            .expect("frame sent");
        settle().await;
    }

    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));
    assert_eq!(sink.frames().len(), 4);
}

#[tokio::test(start_paused = true)]
async fn chunk_and_ack_frames_skip_the_block_list_read() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2).await;
    // Only the list changes here, so the socket stays open. A real block also unpairs and
    // closes the socket, which is the next test.
    a.blocked.put(peer(), "Peer");

    let all = frames();
    for frame in [all[2].clone(), all[5].clone()] {
        remote.to_service.send(frame).await.expect("frame sent");
    }
    settle().await;
    assert_eq!(
        sink.frames().len(),
        2,
        "the chunk and the ack were not checked"
    );
    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Online));

    remote
        .to_service
        .send(all[3].clone())
        .await
        .expect("frame sent");
    settle().await;
    assert_eq!(sink.frames().len(), 2, "the abort was checked and refused");
    assert_eq!(row_status(&a, &peer()), Some(NodeStatus::Offline));
}

#[tokio::test(start_paused = true)]
async fn blocking_a_paired_device_reports_the_peer_gone() {
    let (a, _h) = solo(2).await;
    let sink = Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let _remote = go_online(&a, 2).await;

    // `block_peer` is `pub(super)` in `pairing.rs`, so this test module can call it. If
    // `identity` does not return a `PeerIdentity`, build one the way `pairing_tests.rs` does.
    a.svc.block_peer(&identity(2), "Peer").await;
    settle().await;

    assert!(
        sink.gone_only(&peer()),
        "a block ends the open transfers at once"
    );
}

#[tokio::test(start_paused = true)]
async fn is_online_follows_the_connection() {
    let (a, _h) = solo(2).await;
    assert!(!a.svc.is_online(&peer()), "not connected yet");
    assert!(!a.svc.is_online(&identity(9).device_id), "unknown device");

    let remote = go_online(&a, 2).await;
    assert!(a.svc.is_online(&peer()));

    drop(remote);
    settle().await;
    assert!(!a.svc.is_online(&peer()), "the socket closed");
}
