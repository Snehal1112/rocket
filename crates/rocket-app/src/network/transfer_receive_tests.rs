//! Receiver-side tests for `TransferService`, with fake peers, staging and importer.
//!
//! The rule tables (limits, chunk bounds, offer shape) are tested in `rocket-network`. These
//! tests prove that the service wires the machines: a frame in, the right event or frame out,
//! and the staged file freed on every exit.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use rocket_audit::AuditEventKind;
use rocket_network::limits::ACK_EVERY_CHUNKS;
use rocket_network::limits::{MAX_TRANSFER_BYTES, TRANSFER_OFFER_TTL};
use rocket_network::{
    chunk_count_for, AbortReason, DeviceId, Direction, Outcome, PeerMessage, ResultOutcome,
    TransferSink,
};

use super::test_support::{identity, settle};
use super::transfer::Timing;
use super::transfer_fakes::*;
use super::transfer_ports::UNSAFE_ZIP_MESSAGE;

const ANA: u8 = 2;
const BEN: u8 = 3;

fn dev(seed: u8) -> DeviceId {
    identity(seed).device_id
}

/// Four paired, online peers: Ana, Ben, Cy and Di.
fn rig_peers() -> Rig {
    let rig = rig();
    for (seed, name) in [(2u8, "Ana"), (3, "Ben"), (4, "Cy"), (5, "Di")] {
        rig.peers.add(dev(seed), name);
    }
    rig
}

async fn push(rig: &Rig, seed: u8, message: PeerMessage) {
    rig.svc.on_frame(dev(seed), message);
    settle().await;
}

/// Sends the offer and waits until it shows up as incoming.
async fn offer(rig: &Rig, seed: u8, id: &str, payload: &Payload) {
    let before = rig.events.incoming().len();
    rig.svc.on_frame(dev(seed), payload.offer(id, "orders"));
    wait_for("the incoming offer", || {
        rig.events.incoming().len() > before
    })
    .await;
}

fn feed(rig: &Rig, seed: u8, id: &str, payload: &Payload) {
    for seq in 0..payload.count() {
        rig.svc.on_frame(dev(seed), payload.chunk(id, seq));
    }
}

fn reasons(rig: &Rig, seed: u8) -> Vec<AbortReason> {
    rig.peers
        .sent_to(&dev(seed))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::TransferAbort { reason, .. } => Some(reason),
            _ => None,
        })
        .collect()
}

async fn wait_for_finish(rig: &Rig) {
    wait_for("a finished event", || !rig.events.finished().is_empty()).await;
}

/// The `TransferResult` outcomes sent to a peer.
fn results(rig: &Rig, seed: u8) -> Vec<ResultOutcome> {
    rig.peers
        .sent_to(&dev(seed))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::TransferResult { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

/// The result frame leaves from its own task, so it can trail the `Finished` event.
async fn wait_for_result(rig: &Rig, seed: u8) {
    wait_for("the result frame", || !results(rig, seed).is_empty()).await;
}

/// The `ChunkAck` numbers sent to a peer, in order.
fn acks(rig: &Rig, seed: u8) -> Vec<u32> {
    rig.peers
        .sent_to(&dev(seed))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::ChunkAck { next_seq, .. } => Some(next_seq),
            _ => None,
        })
        .collect()
}

/// The audit row a receive from Ana writes.
fn received_row(collection: &str, outcome: &str) -> AuditEventKind {
    AuditEventKind::NetworkCollectionReceived {
        device_id: dev(ANA).short(),
        collection: collection.into(),
        outcome: outcome.into(),
    }
}

fn manual_offer(id: &str, name: &str, size: u64, count: u32, sha: &str) -> PeerMessage {
    PeerMessage::CollectionOffer {
        transfer_id: id.to_string(),
        name: name.to_string(),
        size_bytes: size,
        chunk_count: count,
        sha256: sha.to_string(),
        keep_in_sync: false,
        link_id: None,
    }
}

#[tokio::test(start_paused = true)]
async fn an_offer_shows_up_once() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);

    offer(&rig, ANA, &id, &payload).await;
    push(&rig, ANA, payload.offer(&id, "orders")).await;

    assert_eq!(
        rig.events.incoming(),
        vec![(
            id.clone(),
            "Ana".to_string(),
            dev(ANA).short(),
            "orders".to_string(),
            payload.bytes.len() as u64
        )],
        "a duplicate offer adds no second row"
    );
    assert_eq!(rig.svc.counts(), (1, 0));
    assert_eq!(
        rig.staging.counters.created(),
        0,
        "nothing staged before Accept"
    );
    assert!(
        rig.importer.calls().is_empty(),
        "nothing imported before Accept"
    );
}

#[tokio::test]
async fn accept_stages_imports_with_the_display_name_and_cleans_up() {
    let rig = rig_peers();
    *rig.importer.existing.lock().expect("existing lock") = vec!["orders".to_string()];
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    assert!(rig.peers.sent_to(&dev(ANA)).contains(&answer(&id, true)));
    assert_eq!(rig.staging.counters.created(), 1);
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Completed)]
    );
    let calls = rig.importer.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, PathBuf::from(format!("/staged/{id}.zip")));
    assert_eq!(calls[0].1, "orders (from Ana)");
    assert_eq!(
        rig.audited(),
        vec![received_row("orders (from Ana)", "completed")]
    );
    wait_for_result(&rig, ANA).await;
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Completed]);
    assert!(reasons(&rig, ANA).is_empty(), "a result, not an abort");
    assert_eq!(rig.events.finished_messages(), vec![None]);
    let size = payload.bytes.len() as u64;
    assert_eq!(
        rig.events.progress().last().copied(),
        Some((Direction::Receive, size, size))
    );
    assert_eq!(rig.staging.counters.finished(), 1);
    assert_eq!(
        rig.staging.counters.released(),
        1,
        "the finished zip is released"
    );
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test]
async fn a_free_name_is_imported_as_offered() {
    let rig = rig_peers();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;

    assert_eq!(rig.importer.calls()[0].1, "orders");
}

#[tokio::test(start_paused = true)]
async fn decline_answers_false_and_finishes_declined() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, false).await.expect("declines");

    assert!(rig.peers.sent_to(&dev(ANA)).contains(&answer(&id, false)));
    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Declined)]
    );
    assert_eq!(rig.staging.counters.created(), 0);
    assert!(rig.svc.respond(&id, true).await.is_err(), "answered once");
}

#[tokio::test(start_paused = true)]
async fn an_unanswered_offer_expires() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    tokio::time::advance(TRANSFER_OFFER_TTL + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Expired)]
    );
    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Expired]);
    assert_eq!(rig.svc.counts(), (0, 0));
    assert!(rig.svc.respond(&id, true).await.is_err());
    assert_eq!(rig.staging.counters.created(), 0);
}

#[tokio::test(start_paused = true)]
async fn accepting_after_expiry_sends_nothing_and_reports_expired() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    tokio::time::advance(TRANSFER_OFFER_TTL).await;
    let err = rig
        .svc
        .respond(&id, true)
        .await
        .expect_err("the offer is over");
    settle().await;

    assert!(
        matches!(&err, rocket_shared::error::DomainError::InvalidInput(m) if m == "The request expired.")
    );
    assert!(
        !rig.peers.sent_to(&dev(ANA)).contains(&answer(&id, true)),
        "no accept goes out"
    );
    assert_eq!(rig.staging.counters.created(), 0);
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Expired)],
        "one result, whichever side saw the deadline first"
    );
}

#[tokio::test(start_paused = true)]
async fn the_fourth_pending_offer_is_busy() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    offer(&rig, 2, &tid(1), &payload).await;
    offer(&rig, 3, &tid(2), &payload).await;
    offer(&rig, 4, &tid(3), &payload).await;

    push(&rig, 5, payload.offer(&tid(4), "orders")).await;

    assert_eq!(reasons(&rig, 5), vec![AbortReason::Busy]);
    assert_eq!(rig.events.incoming().len(), 3);
    assert_eq!(rig.svc.counts(), (3, 0));
}

#[tokio::test(start_paused = true)]
async fn a_second_offer_from_the_same_peer_is_busy() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    offer(&rig, ANA, &tid(1), &payload).await;

    push(&rig, ANA, payload.offer(&tid(2), "orders")).await;
    assert_eq!(
        reasons(&rig, ANA),
        vec![AbortReason::Busy],
        "first still pending"
    );

    rig.svc.respond(&tid(1), true).await.expect("accepts");
    push(&rig, ANA, payload.offer(&tid(3), "orders")).await;
    assert_eq!(
        reasons(&rig, ANA),
        vec![AbortReason::Busy, AbortReason::Busy],
        "first is receiving"
    );
    assert_eq!(rig.events.incoming().len(), 1, "only the first was shown");
}

#[tokio::test(start_paused = true)]
async fn refused_offers_are_answered_with_the_reason_of_the_check() {
    let rig = rig_peers();
    let sha = "a".repeat(64);
    let too_big = MAX_TRANSFER_BYTES + 1;

    push(
        &rig,
        2,
        manual_offer(&tid(1), "orders", too_big, chunk_count_for(too_big), &sha),
    )
    .await;
    push(&rig, 3, manual_offer(&tid(2), "orders", 100, 5, &sha)).await;
    push(&rig, 4, manual_offer("not-an-id", "orders", 100, 1, &sha)).await;

    assert_eq!(reasons(&rig, 2), vec![AbortReason::TooBig]);
    assert_eq!(
        reasons(&rig, 3),
        vec![AbortReason::Failed],
        "a lying chunk count"
    );
    assert!(
        rig.peers.sent_to(&dev(4)).is_empty(),
        "a bad id cannot be named in a reply"
    );
    assert!(rig.events.incoming().is_empty());
    assert_eq!(
        rig.svc.counts(),
        (0, 0),
        "a refused offer leaves nothing behind"
    );
}

#[tokio::test(start_paused = true)]
async fn offers_from_unpaired_peers_are_ignored() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);

    push(&rig, 9, payload.offer(&tid(1), "orders")).await;
    rig.peers.forget(&dev(ANA));
    push(&rig, ANA, payload.offer(&tid(2), "orders")).await;

    assert!(rig.events.all().is_empty());
    assert!(rig.peers.sent().is_empty(), "not even an abort goes out");
}

#[tokio::test(start_paused = true)]
async fn chunks_before_accept_abort_the_transfer() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    push(&rig, ANA, payload.chunk(&id, 0)).await;

    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Failed]);
    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(rig.staging.counters.created(), 0);
    assert!(rig.svc.respond(&id, true).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_refused_chunk_fails_the_transfer_and_cleans_up() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");

    // Chunk 1 before chunk 0: the machine refuses it, the service must clean up.
    push(&rig, ANA, payload.chunk(&id, 1)).await;

    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Failed]);
    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));

    // Garbage that is not base64 ends the next transfer the same way.
    let next = tid(2);
    offer(&rig, ANA, &next, &payload).await;
    rig.svc.respond(&next, true).await.expect("accepts");
    push(
        &rig,
        ANA,
        PeerMessage::Chunk {
            transfer_id: next.clone(),
            seq: 0,
            data: "not base64!".to_string(),
        },
    )
    .await;
    assert_eq!(rig.events.finished().len(), 2);
    assert_eq!(rig.staging.counters.discarded(), 2);
    rig.staging.assert_clean();
}

#[tokio::test]
async fn a_tampered_payload_is_damaged() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let mut tampered = payload.clone();
    tampered.bytes[50] ^= 0xFF;
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &tampered);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Damaged)]
    );
    assert!(
        rig.importer.calls().is_empty(),
        "a damaged file is never imported"
    );
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Damaged]);
    assert!(
        reasons(&rig, ANA).is_empty(),
        "the verdict is a result, not an abort"
    );
    assert_eq!(
        rig.audited(),
        vec![received_row("orders", "damaged")],
        "a tampered payload is evidence"
    );
    assert_eq!(
        rig.staging.counters.released(),
        1,
        "the damaged zip is released"
    );
    rig.staging.assert_clean();
}

#[tokio::test]
async fn a_rejected_zip_is_reported_rejected() {
    let rig = rig_peers();
    rig.importer.set_mode(ImportMode::Reject);
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Rejected)]
    );
    assert_eq!(
        rig.events.finished_messages(),
        vec![Some(UNSAFE_ZIP_MESSAGE.to_string())]
    );
    assert_eq!(rig.importer.calls().len(), 1);
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Rejected]);
    assert_eq!(
        rig.audited(),
        vec![received_row("orders", "rejected")],
        "a hostile ZIP is evidence"
    );
    rig.staging.assert_clean();
}

#[tokio::test]
async fn an_import_failure_is_reported_failed() {
    let rig = rig_peers();
    rig.importer.set_mode(ImportMode::Fail);
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    let messages = rig.events.finished_messages();
    assert_eq!(
        messages,
        vec![Some("The collection could not be imported.".to_string())],
        "a fixed text, never the importer's own"
    );
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Failed]);
    assert!(rig.audited().is_empty());
    rig.staging.assert_clean();
}

#[tokio::test]
async fn invalid_input_with_another_text_is_a_failure_not_a_rejection() {
    let rig = rig_peers();
    rig.importer.set_mode(ImportMode::OtherInvalid);
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)],
        "only the exact unsafe-ZIP text is a rejection"
    );
    let messages = rig.events.finished_messages();
    assert_eq!(
        messages,
        vec![Some("The collection could not be imported.".to_string())]
    );
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Failed]);
    rig.staging.assert_clean();
}

#[tokio::test]
async fn a_failing_release_does_not_change_the_result() {
    let rig = rig_peers();
    rig.staging.fail_release.store(true, Ordering::SeqCst);
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Completed)],
        "the import succeeded, a stuck temp file is only logged"
    );
    assert_eq!(rig.audited().len(), 1);
    assert_eq!(rig.staging.counters.released(), 1, "release was tried once");
}

#[tokio::test]
async fn a_chunk_after_the_transfer_finished_is_ignored() {
    let rig = rig_peers();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;
    let events = rig.events.all().len();
    let sent = rig.peers.sent().len();

    rig.svc.on_frame(dev(ANA), payload.chunk(&id, 0));
    rig.svc.on_frame(dev(ANA), payload.chunk(&tid(9), 0));
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(rig.events.all().len(), events, "no new event");
    assert_eq!(rig.peers.sent().len(), sent, "no new frame");
    assert_eq!(rig.staging.counters.created(), 1, "no new staged file");
    assert_eq!(rig.importer.calls().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_peer_abort_while_receiving_cleans_up() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    push(&rig, ANA, abort(&id, AbortReason::Cancelled)).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Cancelled)]
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn cancel_while_receiving_aborts_and_cleans_up() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    rig.svc.cancel(&id).await.expect("cancels");
    push(&rig, ANA, payload.chunk(&id, 1)).await;

    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Cancelled]);
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Cancelled)]
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
}

#[tokio::test(start_paused = true)]
async fn a_silent_peer_drops_the_receive_after_the_idle_limit() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;
    let almost = Timing::default().idle_limit - Duration::from_secs(5);

    tokio::time::advance(almost).await;
    settle().await;
    assert!(rig.events.finished().is_empty(), "still within the limit");
    push(&rig, ANA, payload.chunk(&id, 1)).await;

    tokio::time::advance(almost).await;
    settle().await;
    assert!(
        rig.events.finished().is_empty(),
        "a chunk restarts the clock"
    );

    tokio::time::advance(Duration::from_secs(10)).await;
    settle().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Failed]);
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn an_abort_from_another_device_is_ignored() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    push(&rig, BEN, abort(&id, AbortReason::Cancelled)).await;
    push(&rig, BEN, payload.chunk(&id, 0)).await;

    assert!(rig.events.finished().is_empty());
    assert_eq!(rig.svc.counts(), (1, 0), "Ana's offer is untouched");
}

#[tokio::test(start_paused = true)]
async fn a_peer_can_offer_again_after_its_transfer_ended() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    offer(&rig, ANA, &tid(1), &payload).await;
    rig.svc.respond(&tid(1), false).await.expect("declines");

    offer(&rig, ANA, &tid(2), &payload).await;

    assert_eq!(
        rig.events.incoming().len(),
        2,
        "the first transfer freed the peer's slot"
    );
    assert!(reasons(&rig, ANA).is_empty(), "no busy answer");
}

#[tokio::test(start_paused = true)]
async fn peer_gone_drops_a_pending_offer() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(
        rig.events.finished_messages(),
        vec![Some("The other node went offline.".to_string())],
        "the receiver never accepted, so it is not a receive failure"
    );
    assert_eq!(rig.svc.counts(), (0, 0));
    assert!(
        rig.peers.sent().is_empty(),
        "nothing is told to a gone peer"
    );
    assert_eq!(rig.staging.counters.created(), 0);
}

#[tokio::test(start_paused = true)]
async fn peer_gone_drops_a_receiving_transfer_at_once() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    // No advance of the clock: the idle limit is only the fallback.
    rig.svc.on_peer_gone(dev(ANA));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
    assert!(
        reasons(&rig, ANA).is_empty(),
        "nothing is told to a gone peer"
    );
    assert_eq!(
        rig.events.finished_messages(),
        vec![None],
        "an accepted receive just failed"
    );
}

#[tokio::test(start_paused = true)]
async fn peer_gone_for_another_device_changes_nothing() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    offer(&rig, ANA, &tid(1), &payload).await;

    rig.svc.on_peer_gone(dev(BEN));
    settle().await;

    assert!(rig.events.finished().is_empty());
    assert_eq!(rig.svc.counts(), (1, 0), "Ana's offer is untouched");
}

#[tokio::test]
async fn an_abort_during_the_finish_stops_the_import() {
    let rig = rig_peers();
    let release = rig.staging.hold_finish();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for("the finish to start", || {
        rig.staging.counters.finished() == 1
    })
    .await;

    rig.svc
        .on_frame(dev(ANA), abort(&id, AbortReason::Cancelled));
    tokio::time::sleep(Duration::from_millis(50)).await;
    release.send(()).expect("the finish waits");
    wait_for_finish(&rig).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Cancelled)]
    );
    assert!(
        rig.importer.calls().is_empty(),
        "an aborted transfer is never imported"
    );
    assert!(rig.audited().is_empty());
    assert!(
        results(&rig, ANA).is_empty(),
        "the sender aborted, it needs no result"
    );
    assert_eq!(rig.staging.counters.released(), 1);
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test]
async fn a_non_cancel_abort_during_the_finish_is_a_failure() {
    // Expired, busy and too_big mean nothing for a transfer that is already received.
    for reason in [
        AbortReason::Expired,
        AbortReason::Busy,
        AbortReason::TooBig,
        AbortReason::Failed,
    ] {
        let rig = rig_peers();
        let release = rig.staging.hold_finish();
        let payload = Payload::new(40 * 1024);
        let id = tid(1);
        offer(&rig, ANA, &id, &payload).await;
        rig.svc.respond(&id, true).await.expect("accepts");
        feed(&rig, ANA, &id, &payload);
        wait_for("the finish to start", || {
            rig.staging.counters.finished() == 1
        })
        .await;

        rig.svc.on_frame(dev(ANA), abort(&id, reason));
        tokio::time::sleep(Duration::from_millis(50)).await;
        release.send(()).expect("the finish waits");
        wait_for_finish(&rig).await;

        assert_eq!(
            rig.events.finished(),
            vec![(id, Direction::Receive, Outcome::Failed)],
            "{reason:?}"
        );
        assert!(rig.importer.calls().is_empty());
    }
}

#[tokio::test]
async fn peer_gone_during_the_finish_stops_the_import() {
    let rig = rig_peers();
    let release = rig.staging.hold_finish();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for("the finish to start", || {
        rig.staging.counters.finished() == 1
    })
    .await;

    rig.svc.on_peer_gone(dev(ANA));
    tokio::time::sleep(Duration::from_millis(50)).await;
    release.send(()).expect("the finish waits");
    wait_for_finish(&rig).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert!(rig.importer.calls().is_empty());
    assert!(results(&rig, ANA).is_empty(), "nobody is left to tell");
    rig.staging.assert_clean();
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test]
async fn a_slow_import_does_not_block_other_peers() {
    let rig = rig_peers();
    let release = rig.importer.hold_import();
    let payload = Payload::new(40 * 1024);
    offer(&rig, ANA, &tid(1), &payload).await;
    rig.svc.respond(&tid(1), true).await.expect("accepts");
    feed(&rig, ANA, &tid(1), &payload);
    wait_for("the import to start", || !rig.importer.calls().is_empty()).await;

    // The import is parked. Another peer's offer must still be handled.
    offer(&rig, BEN, &tid(2), &payload).await;
    assert_eq!(rig.events.incoming().len(), 2);
    assert!(rig.events.finished().is_empty());

    release.send(()).expect("the import waits");
    wait_for_finish(&rig).await;
    assert_eq!(
        rig.events.finished(),
        vec![(tid(1), Direction::Receive, Outcome::Completed)]
    );
}

#[tokio::test(start_paused = true)]
async fn a_short_offer_ttl_expires_a_pending_offer_at_that_ttl() {
    let timing = Timing {
        offer_ttl: Duration::from_secs(5),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    tokio::time::advance(Duration::from_secs(6)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Expired)],
        "the machine and the timer both follow the short ttl"
    );
    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Expired]);
}

#[tokio::test(start_paused = true)]
async fn a_short_idle_limit_drops_the_receive_at_that_limit() {
    let timing = Timing {
        idle_limit: Duration::from_secs(10),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    tokio::time::advance(Duration::from_secs(11)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    rig.staging.assert_clean();
}

#[tokio::test]
async fn the_receiver_acks_every_16_chunks_and_the_last() {
    let rig = rig_peers();
    // 40 chunks: acks at 16, 32 and 40.
    let payload = Payload::new(40 * 32 * 1024 - 100);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");

    for seq in 0..15 {
        rig.svc.on_frame(dev(ANA), payload.chunk(&id, seq));
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(acks(&rig, ANA).is_empty(), "no ack before the 16th chunk");

    for seq in 15..payload.count() {
        rig.svc.on_frame(dev(ANA), payload.chunk(&id, seq));
    }
    wait_for_finish(&rig).await;
    wait_for("the last ack", || acks(&rig, ANA).len() == 3).await;

    assert_eq!(payload.count(), 40);
    assert_eq!(
        acks(&rig, ANA),
        vec![ACK_EVERY_CHUNKS, 2 * ACK_EVERY_CHUNKS, 40],
        "in order, one every 16 chunks and one for the last"
    );
}

#[tokio::test]
async fn a_wrong_staged_length_is_damaged() {
    let rig = rig_peers();
    rig.staging.wrong_len.store(true, Ordering::SeqCst);
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.respond(&id, true).await.expect("accepts");
    feed(&rig, ANA, &id, &payload);
    wait_for_finish(&rig).await;
    wait_for_result(&rig, ANA).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Damaged)],
        "the hash fits, the size does not"
    );
    assert!(rig.importer.calls().is_empty());
    assert_eq!(results(&rig, ANA), vec![ResultOutcome::Damaged]);
    rig.staging.assert_clean();
}

#[tokio::test(start_paused = true)]
async fn a_sender_cancel_of_a_pending_offer_closes_it() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    push(&rig, ANA, abort(&id, AbortReason::Cancelled)).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Cancelled)]
    );
    assert_eq!(rig.svc.counts(), (0, 0));
    assert!(
        rig.peers.sent().is_empty(),
        "the sender ended it, nothing goes back"
    );
    let err = rig
        .svc
        .respond(&id, true)
        .await
        .expect_err("the offer is gone");
    assert!(
        matches!(&err, rocket_shared::error::DomainError::InvalidInput(m) if m == "The request expired.")
    );
    offer(&rig, ANA, &tid(2), &payload).await;
    assert!(
        reasons(&rig, ANA).is_empty(),
        "the peer's slot was freed, no busy"
    );
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_pending_offer_declines_it() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;

    rig.svc.cancel(&id).await.expect("cancels");

    assert!(rig.peers.sent_to(&dev(ANA)).contains(&answer(&id, false)));
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Declined)]
    );
    assert_eq!(rig.svc.counts(), (0, 0));
    assert_eq!(rig.staging.counters.created(), 0);
}

#[tokio::test]
async fn a_dropped_chunk_fails_both_sides_at_the_next_chunk() {
    let sender = rig();
    let receiver = rig();
    let mut link = Link::new(&sender, dev(ANA), "Ana", &receiver, dev(BEN), "Ben");
    let id = sender
        .svc
        .send(&dev(BEN), "orders", false, false)
        .await
        .expect("the send starts")
        .transfer_id;
    let no_loss = |_: &PeerMessage| false;
    link.run_until(&no_loss, || !receiver.events.incoming().is_empty())
        .await;
    receiver.svc.respond(&id, true).await.expect("accepts");

    // Chunk 1 is lost on the way; chunk 2 then breaks the sequence rule.
    let lose_one = |m: &PeerMessage| matches!(m, PeerMessage::Chunk { seq: 1, .. });
    link.run_until(&lose_one, || {
        !sender.events.finished().is_empty() && !receiver.events.finished().is_empty()
    })
    .await;

    assert_eq!(
        receiver.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(
        sender.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert!(receiver.importer.calls().is_empty());
    receiver.staging.assert_clean();
    assert_eq!(
        sender.audited(),
        vec![AuditEventKind::NetworkCollectionSent {
            device_id: dev(BEN).short(),
            collection: "orders".into(),
            outcome: "failed".into(),
        }],
        "chunks left the node, so the failed send is audited"
    );
    assert!(receiver.audited().is_empty());
}

#[tokio::test]
async fn a_stalled_stream_fails_both_sides_within_the_idle_limit() {
    // Real clock, so the limit is short.
    let timing = Timing {
        idle_limit: Duration::from_millis(500),
        ..Timing::default()
    };
    let sender = rig_timed(default_files(), timing);
    let receiver = rig_timed(default_files(), timing);
    let mut link = Link::new(&sender, dev(ANA), "Ana", &receiver, dev(BEN), "Ben");
    let id = sender
        .svc
        .send(&dev(BEN), "orders", false, false)
        .await
        .expect("the send starts")
        .transfer_id;
    let no_loss = |_: &PeerMessage| false;
    link.run_until(&no_loss, || !receiver.events.incoming().is_empty())
        .await;
    receiver.svc.respond(&id, true).await.expect("accepts");

    // Every chunk after the first is lost: a lost tail, or a socket swapped by the duplicate
    // rule. Nothing more arrives, so only the idle limit can end it.
    let lose_tail = |m: &PeerMessage| matches!(m, PeerMessage::Chunk { seq, .. } if *seq >= 1);
    let started = std::time::Instant::now();
    link.run_until(&lose_tail, || {
        !sender.events.finished().is_empty() && !receiver.events.finished().is_empty()
    })
    .await;

    assert!(
        started.elapsed() >= Duration::from_millis(500),
        "the idle limit ended it"
    );
    assert_eq!(
        receiver.events.finished(),
        vec![(id.clone(), Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(
        sender.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)],
        "the receiver's failed reached the sender before its 60 s result wait"
    );
    receiver.staging.assert_clean();
}

#[tokio::test(start_paused = true)]
async fn peer_gone_twice_ends_the_transfer_once() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    push(&rig, ANA, payload.chunk(&id, 0)).await;

    rig.svc.on_peer_gone(dev(ANA));
    rig.svc.on_peer_gone(dev(ANA));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)],
        "one Finished for two notices"
    );
    assert_eq!(rig.staging.counters.discarded(), 1);
    rig.staging.assert_clean();
}

#[tokio::test(start_paused = true)]
async fn frames_after_peer_gone_are_ignored() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    rig.svc.on_peer_gone(dev(ANA));
    settle().await;
    let events = rig.events.all().len();
    let sent = rig.peers.sent().len();

    push(&rig, ANA, payload.chunk(&id, 0)).await;
    push(&rig, ANA, abort(&id, AbortReason::Cancelled)).await;

    assert_eq!(rig.events.all().len(), events, "no new event");
    assert_eq!(rig.peers.sent().len(), sent, "no new frame");
    assert_eq!(rig.staging.counters.created(), 1);
    rig.staging.assert_clean();
}

#[tokio::test(start_paused = true)]
async fn an_offer_after_the_pairing_was_removed_shows_nothing() {
    let rig = rig_peers();
    let payload = Payload::new(100 * 1024);
    rig.peers.forget(&dev(ANA));

    push(&rig, ANA, payload.offer(&tid(1), "orders")).await;

    assert!(rig.events.incoming().is_empty());
    assert_eq!(rig.svc.counts(), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn a_chunk_with_surplus_bytes_fails_the_transfer() {
    let rig = rig_peers();
    let payload = Payload::new(40 * 1024);
    let id = tid(1);
    offer(&rig, ANA, &id, &payload).await;
    rig.svc.respond(&id, true).await.expect("accepts");
    // The first chunk must be 32 KiB; a longer one is refused by the machine.
    let data = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        vec![7u8; 33 * 1024],
    );

    push(
        &rig,
        ANA,
        PeerMessage::Chunk {
            transfer_id: id.clone(),
            seq: 0,
            data,
        },
    )
    .await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Receive, Outcome::Failed)]
    );
    assert_eq!(reasons(&rig, ANA), vec![AbortReason::Failed]);
    rig.staging.assert_clean();
}
