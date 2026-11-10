//! Presence tests: dialing, pings, reconnects, unpairing and duplicate sockets.
//!
//! Single-node tests play the peer through a scripted socket. A manual address makes the node
//! dial at once (`DialHint::ManualAddress`), so those tests do not depend on id order.

use std::sync::atomic::Ordering;
use std::time::Duration;

use rocket_audit::AuditEventKind;
use rocket_network::limits::{DIAL_GRACE, PING_INTERVAL, PROTOCOL_VERSION};
use rocket_network::{
    DeviceId, DiscoveryEvent, NetworkEvent, Nonce, NonceMessage, PeerHandler, PeerMessage,
};
use rocket_shared::error::DomainError;

use super::test_support::*;
use super::{NetworkHandle, NodeGroup, NodeStatus, PairingDecision};

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
async fn go_online(a: &Node, peer_seed: u8, dialed_by: DeviceId) -> Remote {
    let (socket, remote) = scripted_socket(identity(peer_seed), dialed_by, PROTOCOL_VERSION);
    a.connector.push_socket(socket);
    a.svc
        .add_address("127.0.0.1", addr(peer_seed).port())
        .await
        .expect("add address");
    settle().await;
    remote
}

fn status_of(n: &Node, seed: u8) -> Option<NodeStatus> {
    row_status(n, &identity(seed).device_id)
}

fn audited(n: &Node) -> Vec<AuditEventKind> {
    n.audit.records().into_iter().map(|r| r.kind).collect()
}

// ----- one node against a scripted peer -----

#[tokio::test(start_paused = true)]
async fn a_hub_shows_thirty_paired_peers_online_at_once() {
    let a = node(1, "hub");
    for seed in 2..32u8 {
        a.paired.seed(identity(seed).device_id, "Peer");
        a.connector
            .add_route(addr(seed), Route::fixed(identity(seed), "Peer"));
    }
    let _h = a.svc.start().await;

    // Keep every remote end alive, or its socket closes.
    let mut remotes = Vec::new();
    for seed in 2..32u8 {
        remotes.push(go_online(&a, seed, a.id()).await);
    }

    let online = (2..32u8)
        .filter(|seed| status_of(&a, *seed) == Some(NodeStatus::Online))
        .count();
    assert_eq!(
        online, 30,
        "spec 3a: a hub with 30 teammates shows all of them online"
    );
    assert_eq!(remotes.len(), 30);
}

#[tokio::test(start_paused = true)]
async fn a_scripted_peer_goes_online_and_receives_pings() {
    let (a, _h) = solo(2).await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));

    let mut remote = go_online(&a, 2, a.id()).await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));

    tokio::time::advance(PING_INTERVAL + Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(remote.from_service.try_recv(), Ok(PeerMessage::Ping));
}

#[tokio::test(start_paused = true)]
async fn answering_pings_keeps_the_peer_online() {
    let (a, _h) = solo(2).await;
    let mut remote = go_online(&a, 2, a.id()).await;

    for _ in 0..6 {
        tokio::time::advance(PING_INTERVAL).await;
        settle().await;
        assert_eq!(remote.from_service.try_recv(), Ok(PeerMessage::Ping));
        remote
            .to_service
            .send(PeerMessage::Pong)
            .await
            .expect("pong");
        settle().await;
    }
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn the_third_unanswered_ping_goes_offline() {
    let (a, _h) = solo(2).await;
    let _remote = go_online(&a, 2, a.id()).await;

    for _ in 0..2 {
        tokio::time::advance(PING_INTERVAL).await;
        settle().await;
        assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
    }
    tokio::time::advance(PING_INTERVAL).await;
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));
}

#[tokio::test(start_paused = true)]
async fn a_ping_from_the_peer_is_answered_with_a_pong() {
    let (a, _h) = solo(2).await;
    let mut remote = go_online(&a, 2, a.id()).await;

    remote
        .to_service
        .send(PeerMessage::Ping)
        .await
        .expect("ping");
    settle().await;
    assert_eq!(remote.from_service.try_recv(), Ok(PeerMessage::Pong));
}

#[tokio::test(start_paused = true)]
async fn an_unpair_message_removes_the_pairing_and_tells_the_user() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2, a.id()).await;
    let mark = a.events.all().len();

    remote
        .to_service
        .send(PeerMessage::Unpair)
        .await
        .expect("unpair");
    settle().await;

    assert!(!a.paired.contains(&identity(2).device_id));
    assert_eq!(a.events.removed(), vec![("Peer".to_string(), true)]);
    assert!(a.events.all()[mark..].contains(&NetworkEvent::NodesChanged));
    assert!(
        audited(&a).contains(&AuditEventKind::NetworkDeviceUnpaired {
            device_id: identity(2).device_id.to_base32(),
            by_peer: true
        })
    );
    // The device is still on the network, so it is offered under Nearby.
    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].group, NodeGroup::Nearby);
}

#[tokio::test(start_paused = true)]
async fn it_reconnects_after_a_drop_with_growing_delays() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2, a.id()).await;
    let base = a.connector.open_calls.load(Ordering::SeqCst);
    assert_eq!(base, 1);

    drop(remote);
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));
    assert_eq!(
        a.connector.open_calls.load(Ordering::SeqCst),
        base,
        "waits first"
    );

    // Retries come after 1 s, 2 s and 4 s.
    tokio::time::advance(Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), base + 1);
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), base + 2);
    tokio::time::advance(Duration::from_secs(3)).await;
    settle().await;
    assert_eq!(
        a.connector.open_calls.load(Ordering::SeqCst),
        base + 2,
        "not yet"
    );
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), base + 3);
}

#[tokio::test(start_paused = true)]
async fn it_goes_online_again_when_a_socket_is_available() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2, a.id()).await;
    drop(remote);
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));

    let (socket, _second) = scripted_socket(identity(2), a.id(), PROTOCOL_VERSION);
    a.connector.push_socket(socket);
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn a_new_sighting_retries_at_once() {
    let (a, _h) = solo(2).await;
    let remote = go_online(&a, 2, a.id()).await;
    drop(remote);
    settle().await;
    // Back off to a long delay first.
    for seconds in [1u64, 2, 4] {
        tokio::time::advance(Duration::from_secs(seconds)).await;
        settle().await;
    }
    let (socket, _second) = scripted_socket(identity(2), a.id(), PROTOCOL_VERSION);
    a.connector.push_socket(socket);

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(2).device_id,
        profile: profile("Peer"),
        addresses: vec![addr(2)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    assert_eq!(
        status_of(&a, 2),
        Some(NodeStatus::Online),
        "no waiting for the backoff"
    );
}

#[tokio::test(start_paused = true)]
async fn a_socket_with_another_protocol_version_needs_an_update() {
    let (a, _h) = solo(2).await;
    let (socket, remote) = scripted_socket(identity(2), a.id(), PROTOCOL_VERSION + 1);
    a.connector.push_socket(socket);
    a.svc
        .add_address("127.0.0.1", addr(2).port())
        .await
        .expect("add address");
    settle().await;

    assert_eq!(status_of(&a, 2), Some(NodeStatus::NeedsUpdate));
    assert!(remote.to_service.is_closed(), "the socket was dropped");
}

#[tokio::test(start_paused = true)]
async fn a_peer_on_the_previous_protocol_version_needs_an_update() {
    // Version 2 is the build before keep-in-sync. It must never talk to version 3.
    let (a, _h) = solo(2).await;
    let (socket, remote) = scripted_socket(identity(2), a.id(), PROTOCOL_VERSION - 1);
    a.connector.push_socket(socket);
    a.svc
        .add_address("127.0.0.1", addr(2).port())
        .await
        .expect("add address");
    settle().await;

    assert_eq!(status_of(&a, 2), Some(NodeStatus::NeedsUpdate));
    assert!(remote.to_service.is_closed(), "the socket was dropped");
}

// ----- duplicate sockets -----

#[tokio::test(start_paused = true)]
async fn the_socket_dialled_by_the_lower_id_wins() {
    let (a, _h) = solo(2).await;
    let keep = a.id().min(identity(2).device_id);
    let other = a.id().max(identity(2).device_id);
    let first = go_online(&a, 2, keep).await;

    let (second_socket, second) = scripted_socket(identity(2), other, PROTOCOL_VERSION);
    a.svc.on_peer_socket(second_socket).await;
    settle().await;

    assert!(second.to_service.is_closed(), "the other socket is closed");
    assert!(!first.to_service.is_closed(), "the kept socket stays");
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn a_socket_dialled_by_the_lower_id_replaces_the_other() {
    let (a, _h) = solo(2).await;
    let keep = a.id().min(identity(2).device_id);
    let other = a.id().max(identity(2).device_id);
    let first = go_online(&a, 2, other).await;

    let (second_socket, second) = scripted_socket(identity(2), keep, PROTOCOL_VERSION);
    a.svc.on_peer_socket(second_socket).await;
    settle().await;

    assert!(first.to_service.is_closed(), "the old socket is closed");
    assert!(!second.to_service.is_closed());
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn a_socket_from_an_unpaired_or_blocked_device_is_refused() {
    let (a, _h) = solo(2).await;

    let (stranger, stranger_remote) =
        scripted_socket(identity(40), identity(40).device_id, PROTOCOL_VERSION);
    a.svc.on_peer_socket(stranger).await;
    settle().await;
    assert!(stranger_remote.to_service.is_closed());

    a.blocked.put(identity(2).device_id, "Peer");
    let (blocked, blocked_remote) =
        scripted_socket(identity(2), identity(2).device_id, PROTOCOL_VERSION);
    a.svc.on_peer_socket(blocked).await;
    settle().await;
    assert!(blocked_remote.to_service.is_closed());
    assert!(
        !a.svc.is_paired(&identity(2).device_id).await,
        "blocked counts as not paired"
    );
}

// ----- the dialer rule -----

#[tokio::test(start_paused = true)]
async fn paired_nodes_open_exactly_one_connection() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    see(&a, &b);
    see(&b, &a);
    settle().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(row_status(&a, &b.id()), Some(NodeStatus::Online));
    assert_eq!(row_status(&b, &a.id()), Some(NodeStatus::Online));
    let opens = a.connector.open_calls.load(Ordering::SeqCst)
        + b.connector.open_calls.load(Ordering::SeqCst);
    assert_eq!(opens, 1, "only the lower id dialled");
}

#[tokio::test(start_paused = true)]
async fn the_higher_id_waits_for_the_grace_period() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let (low, high) = ordered(&a, &b);
    let _hl = low.svc.start().await;
    let _hh = high.svc.start().await;

    // Only the higher side knows where the lower one is.
    see(high, low);
    settle().await;
    tokio::time::advance(DIAL_GRACE - Duration::from_secs(3)).await;
    settle().await;
    assert_eq!(
        high.connector.open_calls.load(Ordering::SeqCst),
        0,
        "still waiting"
    );
    assert_eq!(row_status(high, &low.id()), Some(NodeStatus::Offline));

    tokio::time::advance(Duration::from_secs(4)).await;
    settle().await;
    assert_eq!(
        high.connector.open_calls.load(Ordering::SeqCst),
        1,
        "the grace period ended"
    );
    assert_eq!(row_status(high, &low.id()), Some(NodeStatus::Online));
    assert_eq!(row_status(low, &high.id()), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn a_hidden_node_still_serves_paired_presence() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    b.svc.set_hidden(true).await.expect("hide");
    see(&a, &b);
    see(&b, &a);
    settle().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(row_status(&a, &b.id()), Some(NodeStatus::Online));
    assert_eq!(row_status(&b, &a.id()), Some(NodeStatus::Online));

    // Browsing still works while hidden (spec 7.4).
    b.connector
        .add_route(addr(3), Route::fixed(identity(3), "Cat Lee"));
    b.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(3).device_id,
        profile: profile("Cat Lee"),
        addresses: vec![addr(3)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    assert_eq!(
        row_status(&b, &identity(3).device_id),
        Some(NodeStatus::Available),
        "a hidden node still lists nearby nodes"
    );
}

// ----- spoofing and impostors -----

#[tokio::test(start_paused = true)]
async fn a_spoofed_id_of_a_paired_device_is_never_dialled() {
    let a = node(1, "ann");
    a.paired.seed(identity(2).device_id, "Peer");
    // A stranger answers at addr(8) with its own key.
    a.connector
        .add_route(addr(8), Route::fixed(identity(8), "Mallory"));
    let _ha = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(2).device_id,
        profile: profile("Peer"),
        addresses: vec![addr(8)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    tokio::time::advance(DIAL_GRACE * 3).await;
    settle().await;

    assert_eq!(a.connector.open_calls.load(Ordering::SeqCst), 0);
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Offline));
}

#[tokio::test(start_paused = true)]
async fn an_impostor_at_a_saved_address_does_not_remove_the_pairing() {
    let a = node(1, "ann");
    a.paired
        .seed_with(identity(2).device_id, "Peer", vec![addr(2)]);
    // The address now answers with another key. The connector reports an identity mismatch.
    a.connector
        .add_route(addr(2), Route::fixed(identity(77), "Impostor"));
    let _ha = a.svc.start().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;

    assert!(
        a.connector.open_calls.load(Ordering::SeqCst) >= 1,
        "it did try"
    );
    assert!(a.paired.contains(&identity(2).device_id));
    assert!(a.events.removed().is_empty());
}

// ----- unpairing between two real nodes -----

#[tokio::test(start_paused = true)]
async fn a_peer_that_unpaired_us_while_offline_is_removed_on_the_next_dial() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    // B no longer has A.
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    let mark = a.events.all().len();

    // A manual address makes A dial straight away. B answers "not paired".
    a.svc
        .add_address("127.0.0.1", b.addr().port())
        .await
        .expect("add address");
    settle().await;

    assert!(!a.paired.contains(&b.id()));
    assert_eq!(a.events.removed(), vec![("ben".to_string(), true)]);
    assert!(a.events.all()[mark..].contains(&NetworkEvent::NodesChanged));
    assert!(
        audited(&a).contains(&AuditEventKind::NetworkDeviceUnpaired {
            device_id: b.id().to_base32(),
            by_peer: true
        })
    );
}

#[tokio::test(start_paused = true)]
async fn removing_a_paired_device_online_tells_the_peer() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    see(&a, &b);
    see(&b, &a);
    settle().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(row_status(&a, &b.id()), Some(NodeStatus::Online));

    let mark_a = a.events.all().len();
    a.svc.remove_paired(&b.id()).await.expect("remove");
    settle().await;

    assert!(!a.paired.contains(&b.id()));
    assert!(!b.paired.contains(&a.id()), "the peer removed us too");
    assert_eq!(a.events.removed(), vec![("ben".to_string(), false)]);
    assert_eq!(b.events.removed(), vec![("ann".to_string(), true)]);
    assert!(a.events.all()[mark_a..].contains(&NetworkEvent::NodesChanged));
    assert!(
        audited(&a).contains(&AuditEventKind::NetworkDeviceUnpaired {
            device_id: b.id().to_base32(),
            by_peer: false
        })
    );
    assert!(
        audited(&b).contains(&AuditEventKind::NetworkDeviceUnpaired {
            device_id: a.id().to_base32(),
            by_peer: true
        })
    );
    assert!(a
        .svc
        .list_nodes()
        .iter()
        .all(|row| row.group != NodeGroup::Paired));
}

#[tokio::test(start_paused = true)]
async fn blocking_a_paired_device_unpairs_it_and_closes_its_socket() {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    a.paired.seed(b.id(), "ben");
    b.paired.seed(a.id(), "ann");
    let _ha = a.svc.start().await;
    let _hb = b.svc.start().await;
    see(&a, &b);
    see(&b, &a);
    settle().await;
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(row_status(&a, &b.id()), Some(NodeStatus::Online));

    // The paired device asks to pair again, and the user answers Block.
    let reply = a
        .svc
        .pair_request(b.identity.clone(), ip(), pair_request("ben"))
        .await
        .expect("a paired device may ask again");
    a.svc
        .pair_nonce(
            b.identity.clone(),
            reply.pairing_id.clone(),
            NonceMessage {
                nonce: Nonce::random().to_base64(),
            },
        )
        .await
        .expect("nonce");
    let mark_a = a.events.all().len();
    a.svc
        .respond_pairing(&reply.pairing_id, PairingDecision::Block)
        .await
        .expect("block");
    settle().await;

    assert!(a.blocked.has(&b.id()));
    assert!(
        !a.paired.contains(&b.id()),
        "blocking also removes the pairing"
    );
    assert!(
        !b.paired.contains(&a.id()),
        "the peer got Unpair and removed us"
    );
    assert_eq!(a.events.removed(), vec![("ben".to_string(), false)]);
    assert_eq!(b.events.removed(), vec![("ann".to_string(), true)]);
    assert!(a.events.all()[mark_a..].contains(&NetworkEvent::NodesChanged));
    assert!(audited(&a).contains(&AuditEventKind::NetworkDeviceBlocked {
        device_id: b.id().to_base32()
    }));
    assert!(
        audited(&a).contains(&AuditEventKind::NetworkDeviceUnpaired {
            device_id: b.id().to_base32(),
            by_peer: false
        })
    );
    assert_ne!(
        row_status(&a, &b.id()),
        Some(NodeStatus::Online),
        "the socket is closed"
    );

    // The blocked device cannot open a new socket.
    tokio::time::advance(DIAL_GRACE + Duration::from_secs(1)).await;
    settle().await;
    assert!(!a.svc.is_paired(&b.id()).await);
    assert_ne!(row_status(&b, &a.id()), Some(NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn frames_from_a_blocked_device_get_no_answer_and_close_the_socket() {
    let (a, _h) = solo(2).await;
    let mut remote = go_online(&a, 2, a.id()).await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));

    // The block lands while the socket is open.
    a.blocked.put(identity(2).device_id, "Peer");
    remote
        .to_service
        .send(PeerMessage::Ping)
        .await
        .expect("send ping");
    settle().await;

    let next = tokio::time::timeout(Duration::from_secs(1), remote.from_service.recv())
        .await
        .expect("the socket ends at once");
    assert_eq!(next, None, "no Pong, and the service closed the socket");
}

#[tokio::test(start_paused = true)]
async fn removing_a_device_that_is_not_paired_is_an_error() {
    let a = node(1, "ann");
    let _ha = a.svc.start().await;
    let err = a
        .svc
        .remove_paired(&identity(2).device_id)
        .await
        .expect_err("not paired");
    assert_eq!(
        err,
        DomainError::InvalidInput("That device is not paired.".into())
    );
}

#[tokio::test(start_paused = true)]
async fn the_last_good_address_is_remembered() {
    let (a, _h) = solo(2).await;
    let _remote = go_online(&a, 2, a.id()).await;
    assert_eq!(
        a.paired.last_addresses(&identity(2).device_id),
        vec![addr(2)]
    );
}

#[tokio::test(start_paused = true)]
async fn stopping_the_node_closes_the_open_socket() {
    let (a, handle) = solo(2).await;
    let mut remote = go_online(&a, 2, a.id()).await;
    assert_eq!(status_of(&a, 2), Some(NodeStatus::Online));

    handle.stop();
    settle().await;

    // Every sender into the socket is gone, so the peer sees the connection close.
    assert_eq!(
        remote.from_service.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected),
        "a stopped node must not keep a socket open"
    );
}

// ----- contract check frames -----

/// Records every frame the node hands to its sink.
#[derive(Default)]
struct RecordingSink(std::sync::Mutex<Vec<PeerMessage>>);

impl RecordingSink {
    fn frames(&self) -> Vec<PeerMessage> {
        self.0.lock().expect("sink lock").clone()
    }
}

impl rocket_network::TransferSink for RecordingSink {
    fn on_frame(&self, _from: DeviceId, message: PeerMessage) {
        self.0.lock().expect("sink lock").push(message);
    }

    fn on_peer_gone(&self, _device: DeviceId) {}
}

fn contract_frames() -> Vec<PeerMessage> {
    vec![
        PeerMessage::ContractCheckOffer {
            check_id: rocket_network::ContractCheckId::random()
                .as_str()
                .to_string(),
            contract: "Orders API".into(),
            version: "1.0.0".into(),
            policy: rocket_network::ContractPolicyWire::Strict,
            changes: Vec::new(),
        },
        PeerMessage::ContractCheckResult {
            check_id: rocket_network::ContractCheckId::random()
                .as_str()
                .to_string(),
            outcome: rocket_network::ContractCheckOutcome::Declined,
            rows: Vec::new(),
        },
    ]
}

#[tokio::test(start_paused = true)]
async fn contract_check_frames_from_a_paired_device_reach_the_sink() {
    let (a, _h) = solo(2).await;
    let sink = std::sync::Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let remote = go_online(&a, 2, a.id()).await;

    let frames = contract_frames();
    for frame in frames.clone() {
        remote.to_service.send(frame).await.expect("send frame");
    }
    settle().await;

    assert_eq!(sink.frames(), frames, "both frames are delivered as sent");
}

#[tokio::test(start_paused = true)]
async fn contract_check_frames_from_a_blocked_device_are_dropped_before_the_sink() {
    let (a, _h) = solo(2).await;
    let sink = std::sync::Arc::new(RecordingSink::default());
    a.svc.set_transfer_sink(sink.clone());
    let mut remote = go_online(&a, 2, a.id()).await;

    // The block lands while the socket is open.
    a.blocked.put(identity(2).device_id, "Peer");
    for frame in contract_frames() {
        // The second send can fail once the node closed the socket.
        let _ = remote.to_service.send(frame).await;
    }
    settle().await;

    assert!(
        sink.frames().is_empty(),
        "no contract frame reached the sink"
    );
    let next = tokio::time::timeout(Duration::from_secs(1), remote.from_service.recv())
        .await
        .expect("the socket ends at once");
    assert_eq!(next, None, "the service closed the socket");
}
