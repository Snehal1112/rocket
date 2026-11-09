//! Sender-side tests for `TransferService`, with fake peers and a real export over memory files.
//!
//! The rules (offer shape, expiry, chunk order, limits) are tested in `rocket-network`. These
//! tests prove the wiring: a frame in, the right event or frame out, and one `Finished`.

use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rocket_audit::AuditEventKind;
use rocket_export::{ExportAction, ExportNote, ExportReport};
use rocket_network::limits::{
    MAX_FRAME_BYTES, MAX_TRANSFER_BYTES, MAX_UNACKED_CHUNKS, TRANSFER_OFFER_TTL,
};
use rocket_network::{
    chunk_count_for, AbortReason, DeviceId, Direction, OfferError, Outcome, PeerMessage,
    ResultOutcome, TransferEvent, TransferId, TransferSink,
};
use rocket_shared::error::DomainError;
use sha2::{Digest, Sha256};

use super::test_support::{identity, node, settle};
use super::transfer::{outcome_name, PeerSender, Timing};
use super::transfer_fakes::*;
use super::transfer_send::{export_notes, offer_error};

const ANA: u8 = 2;

fn dev(seed: u8) -> DeviceId {
    identity(seed).device_id
}

fn rig_with_ana() -> Rig {
    let rig = rig();
    rig.peers.add(dev(ANA), "Ana");
    rig
}

/// Sends "orders" to Ana, then freezes the clock so the test can move it.
async fn start(rig: &Rig) -> String {
    let started = rig
        .svc
        .send(&dev(ANA), "orders", false, false)
        .await
        .expect("the send starts");
    tokio::time::pause();
    settle().await;
    started.transfer_id
}

/// `(size, chunk count, sha256)` of the first offer sent to Ana.
fn offer_of(rig: &Rig) -> (u64, u32, String) {
    match rig.peers.sent_to(&dev(ANA)).first() {
        Some(PeerMessage::CollectionOffer {
            size_bytes,
            chunk_count,
            sha256,
            ..
        }) => (*size_bytes, *chunk_count, sha256.clone()),
        other => panic!("expected an offer first, got {other:?}"),
    }
}

fn chunks_sent(rig: &Rig) -> Vec<(u32, String)> {
    rig.peers
        .sent_to(&dev(ANA))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::Chunk { seq, data, .. } => Some((seq, data)),
            _ => None,
        })
        .collect()
}

fn aborts_sent(rig: &Rig) -> Vec<AbortReason> {
    rig.peers
        .sent_to(&dev(ANA))
        .into_iter()
        .filter_map(|m| match m {
            PeerMessage::TransferAbort { reason, .. } => Some(reason),
            _ => None,
        })
        .collect()
}

/// Moves the clock past the wait for the receiver's result.
async fn past_the_result_wait() {
    tokio::time::advance(Timing::default().result_timeout + Duration::from_millis(1)).await;
    settle().await;
}

/// Starts a send, lets Ana accept and waits until every chunk is out.
async fn delivered(rig: &Rig) -> String {
    let id = start(rig).await;
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    id
}

/// A delivered send whose receiver also sent the final `ChunkAck`, so the sender now waits
/// for the result (and only now may a lost peer or the result timeout give `Unconfirmed`).
async fn delivered_and_acked(rig: &Rig) -> String {
    let id = delivered(rig).await;
    let (_, count, _) = offer_of(rig);
    rig.svc.on_frame(dev(ANA), ack(&id, count));
    settle().await;
    id
}

/// A delivered send whose receiver answered with `outcome`.
async fn finish_with(outcome: ResultOutcome) -> (Rig, String) {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;
    rig.svc.on_frame(dev(ANA), result(&id, outcome));
    settle().await;
    (rig, id)
}

#[tokio::test]
async fn new_registers_the_service_as_the_sink() {
    let rig = rig();
    assert!(rig.peers.sink().is_some());
}

#[test]
fn the_network_service_is_a_peer_sender() {
    let a = node(1, "ann");
    let _sender: Arc<dyn PeerSender> = a.svc.clone();
}

#[tokio::test]
async fn send_offers_the_collection_with_size_and_hash() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    assert!(TransferId::parse(&id).is_ok());
    let sent = rig.peers.sent_to(&dev(ANA));
    assert_eq!(sent.len(), 1, "only the offer, no chunk before the answer");
    let PeerMessage::CollectionOffer {
        transfer_id,
        name,
        size_bytes,
        chunk_count,
        sha256,
        ..
    } = &sent[0]
    else {
        panic!("expected an offer, got {:?}", sent[0]);
    };
    assert_eq!(transfer_id, &id);
    assert_eq!(name, "orders");
    assert!(*size_bytes > 100 * 1024 && *size_bytes < MAX_TRANSFER_BYTES);
    assert_eq!(*chunk_count, chunk_count_for(*size_bytes));
    assert_eq!(sha256.len(), 64);
    assert!(sha256
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[tokio::test]
async fn an_accepted_offer_streams_every_chunk_and_completes_on_the_result() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (size, count, sha) = offer_of(&rig);

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    let chunks = chunks_sent(&rig);
    assert_eq!(chunks.len() as u32, count);
    for (index, (seq, _)) in chunks.iter().enumerate() {
        assert_eq!(*seq, index as u32, "in order");
    }
    let mut joined = Vec::new();
    for (_, data) in &chunks {
        joined.extend(STANDARD.decode(data).expect("base64"));
    }
    assert_eq!(joined.len() as u64, size);
    assert_eq!(format!("{:x}", Sha256::digest(&joined)), sha);
    for (_, message) in rig.peers.sent() {
        assert!(message.encode().len() <= MAX_FRAME_BYTES, "frame fits");
    }
    assert_eq!(
        rig.events.progress().last().copied(),
        Some((Direction::Send, size, size))
    );
    assert!(
        rig.events.finished().is_empty(),
        "sent is not reported before the receiver confirms the import"
    );
    assert!(rig.audited().is_empty());

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Send, Outcome::Completed)]
    );
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkCollectionSent {
            device_id: dev(ANA).short(),
            collection: "orders".into(),
            outcome: "completed".into(),
        }]
    );
}

#[tokio::test]
async fn a_decline_ends_the_send_as_declined() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    rig.svc.on_frame(dev(ANA), answer(&id, false));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Declined)]
    );
    assert!(chunks_sent(&rig).is_empty());
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn an_unanswered_offer_expires_and_tells_the_peer() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    tokio::time::advance(TRANSFER_OFFER_TTL + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Expired)]
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Expired]);
}

#[tokio::test]
async fn an_answer_after_the_offer_expired_is_ignored() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    tokio::time::advance(TRANSFER_OFFER_TTL + Duration::from_secs(1)).await;
    settle().await;

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    assert!(chunks_sent(&rig).is_empty(), "no chunk after expiry");
    assert_eq!(rig.events.finished().len(), 1, "still one result");
}

#[tokio::test]
async fn a_peer_abort_ends_the_send_with_the_mapped_outcome() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    rig.svc.on_frame(dev(ANA), abort(&id, AbortReason::Busy));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Busy)]
    );
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn frames_from_another_device_are_ignored() {
    let rig = rig_with_ana();
    rig.peers.add(dev(3), "Ben");
    let id = start(&rig).await;

    rig.svc.on_frame(dev(3), answer(&id, true));
    rig.svc.on_frame(dev(3), abort(&id, AbortReason::Failed));
    settle().await;
    assert!(chunks_sent(&rig).is_empty());
    assert!(rig.events.finished().is_empty());

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    rig.svc
        .on_frame(dev(3), result(&id, ResultOutcome::Completed));
    settle().await;

    assert!(
        rig.events.finished().is_empty(),
        "only the peer itself can confirm the import"
    );
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn cancel_before_the_answer_aborts_and_finishes_cancelled() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    rig.svc.cancel(&id).await.expect("cancels");

    assert_eq!(aborts_sent(&rig), vec![AbortReason::Cancelled]);
    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Send, Outcome::Cancelled)]
    );
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    assert!(chunks_sent(&rig).is_empty(), "a late accept does nothing");
}

#[tokio::test]
async fn cancel_while_streaming_ends_once_as_cancelled() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (reached, release) = rig.peers.hold_chunk(1);

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    reached.notified().await;
    rig.svc.cancel(&id).await.expect("cancels");
    release.notify_one();
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Cancelled)],
        "exactly one result"
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Cancelled]);
    assert_eq!(
        rig.audited(),
        vec![sent_row("cancelled")],
        "a chunk left, so it is recorded"
    );
    assert!(
        matches!(
            rig.events.all().last(),
            Some(TransferEvent::Finished { .. })
        ),
        "no progress after the end"
    );
}

#[tokio::test]
async fn cancel_after_the_last_chunk_is_refused_and_the_send_still_completes() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (_, count, _) = offer_of(&rig);
    let (reached, release) = rig.peers.hold_chunk(count - 1);

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    reached.notified().await;
    let err = rig.svc.cancel(&id).await.expect_err("too late to cancel");
    release.notify_one();
    settle().await;

    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "Already sent. Waiting for the result.")
    );
    assert!(aborts_sent(&rig).is_empty(), "nothing was aborted");
    assert!(rig.events.finished().is_empty(), "still waiting");

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Completed)]
    );
    assert_eq!(rig.audited().len(), 1);
}

#[tokio::test]
async fn a_peer_going_offline_mid_send_fails_the_send() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (reached, release) = rig.peers.hold_chunk(1);

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    reached.notified().await;
    rig.peers.set_online(&dev(ANA), false);
    release.notify_one();
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert_eq!(chunks_sent(&rig).len(), 1, "only chunk 0 got through");
    assert_eq!(rig.audited(), vec![sent_row("failed")]);
}

#[tokio::test]
async fn a_failure_report_after_the_last_chunk_fails_the_send() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;
    assert!(rig.events.finished().is_empty(), "delivered, waiting");

    rig.svc.on_frame(dev(ANA), abort(&id, AbortReason::Failed));
    settle().await;
    past_the_result_wait().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)],
        "one result, and the end of the wait adds none"
    );
    assert_eq!(rig.audited(), vec![sent_row("failed")]);
}

#[tokio::test]
async fn cancel_after_completion_is_a_no_op() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;
    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert_eq!(rig.events.finished().len(), 1);

    rig.svc.cancel(&id).await.expect("a no-op");

    assert_eq!(rig.events.finished().len(), 1);
    assert!(aborts_sent(&rig).is_empty());
}

#[tokio::test]
async fn a_second_send_to_the_same_peer_is_refused() {
    let rig = rig_with_ana();
    let _first = start(&rig).await;

    let err = rig
        .svc
        .send(&dev(ANA), "orders", false, false)
        .await
        .expect_err("one at a time");

    assert!(matches!(&err, DomainError::InvalidInput(m) if m.contains("already running")));
    let offers = rig
        .peers
        .sent_to(&dev(ANA))
        .into_iter()
        .filter(|m| matches!(m, PeerMessage::CollectionOffer { .. }))
        .count();
    assert_eq!(offers, 1);
    assert_eq!(rig.exports(), 1, "the refused send did not export");
}

#[tokio::test]
async fn an_empty_collection_is_refused_before_any_offer() {
    let rig = rig_with(Vec::new());
    rig.peers.add(dev(ANA), "Ana");

    let err = rig
        .svc
        .send(&dev(ANA), "orders", false, false)
        .await
        .expect_err("nothing to send");

    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "There is nothing to send in that collection.")
    );
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn a_collection_over_the_limit_is_refused_before_any_offer() {
    let rig = rig_with(vec![
        (
            "opencollection.yml".to_string(),
            b"opencollection: \"1.0.0\"\ninfo:\n  name: orders\n".to_vec(),
        ),
        (
            "scripts/big.js".to_string(),
            noise(MAX_TRANSFER_BYTES as usize + 512 * 1024),
        ),
    ]);
    rig.peers.add(dev(ANA), "Ana");

    let err = rig
        .svc
        .send(&dev(ANA), "orders", false, false)
        .await
        .expect_err("too big");

    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m.starts_with("This collection is too big to send (") && m.ends_with(" MiB, limit 10)."))
    );
    assert!(rig.peers.sent().is_empty(), "no offer for a big collection");
}

#[test]
fn the_too_big_refusal_names_the_size_rounded_up() {
    let err = offer_error(OfferError::TooBig, MAX_TRANSFER_BYTES as usize + 1);

    assert!(matches!(
        &err,
        DomainError::InvalidInput(m)
            if m == "This collection is too big to send (10.1 MiB, limit 10)."
    ));
}

#[tokio::test]
async fn sending_to_an_unpaired_or_offline_node_is_refused() {
    let rig = rig_with_ana();

    let err = rig
        .svc
        .send(&dev(9), "orders", false, false)
        .await
        .expect_err("not paired");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is not paired."));

    rig.peers.set_online(&dev(ANA), false);
    let err = rig
        .svc
        .send(&dev(ANA), "orders", false, false)
        .await
        .expect_err("offline");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is offline."));
    assert!(rig.peers.sent().is_empty());

    assert_eq!(rig.exports(), 0, "a refused send never reaches the export");

    // The refused sends left nothing behind, so a retry works.
    rig.peers.set_online(&dev(ANA), true);
    start(&rig).await;
}

#[tokio::test]
async fn a_blank_collection_name_is_refused() {
    let rig = rig_with_ana();

    let err = rig
        .svc
        .send(&dev(ANA), "   ", false, false)
        .await
        .expect_err("blank");

    assert!(matches!(err, DomainError::InvalidInput(_)));
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn a_peer_can_be_sent_to_again_after_the_send_ended() {
    let rig = rig_with_ana();
    let first = rig
        .svc
        .send(&dev(ANA), "orders", false, false)
        .await
        .expect("the first send starts")
        .transfer_id;
    rig.svc.cancel(&first).await.expect("cancels");

    let second = rig.svc.send(&dev(ANA), "orders", false, false).await;

    assert!(second.is_ok(), "the first send freed the peer's slot");
}

#[tokio::test]
async fn a_damaged_result_ends_the_send_as_damaged() {
    let (rig, id) = finish_with(ResultOutcome::Damaged).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Damaged)]
    );
    assert_eq!(rig.audited(), vec![sent_row("damaged")]);
}

#[tokio::test]
async fn a_rejected_result_ends_the_send_as_rejected() {
    let (rig, id) = finish_with(ResultOutcome::Rejected).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Rejected)]
    );
    assert_eq!(rig.audited(), vec![sent_row("rejected")]);
}

#[tokio::test]
async fn a_failed_result_ends_the_send_as_failed() {
    let (rig, id) = finish_with(ResultOutcome::Failed).await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert_eq!(rig.audited(), vec![sent_row("failed")]);
}

/// The audit row a send to Ana writes, with `outcome`.
fn sent_row(outcome: &str) -> AuditEventKind {
    AuditEventKind::NetworkCollectionSent {
        device_id: dev(ANA).short(),
        collection: "orders".into(),
        outcome: outcome.into(),
    }
}

#[tokio::test]
async fn a_missing_result_times_out_as_unconfirmed() {
    let rig = rig_with_ana();
    let id = delivered_and_acked(&rig).await;

    tokio::time::advance(Timing::default().result_timeout - Duration::from_secs(1)).await;
    settle().await;
    assert!(rig.events.finished().is_empty(), "still waiting");

    past_the_result_wait().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Send, Outcome::Unconfirmed)],
        "every byte left, so this is not a failure"
    );
    assert_eq!(
        rig.audited(),
        vec![sent_row("unconfirmed")],
        "the egress is recorded"
    );
    assert!(matches!(
        rig.events.all().last(),
        Some(TransferEvent::Finished { .. })
    ));

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert_eq!(rig.events.finished().len(), 1, "a late result is ignored");
    assert_eq!(rig.audited().len(), 1);
}

#[tokio::test]
async fn a_result_before_the_last_chunk_is_ignored() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (_, count, _) = offer_of(&rig);

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert!(rig.events.finished().is_empty());
    assert!(rig.audited().is_empty());

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    assert_eq!(chunks_sent(&rig).len() as u32, count, "the send went on");
}

#[tokio::test]
async fn a_peer_gone_after_the_final_ack_is_unconfirmed() {
    let rig = rig_with_ana();
    let id = delivered_and_acked(&rig).await;

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;
    past_the_result_wait().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Unconfirmed)],
        "one result, no wait for the timeout, and not a failure"
    );
    assert_eq!(rig.audited(), vec![sent_row("unconfirmed")]);
}

#[tokio::test]
async fn a_peer_gone_after_the_last_chunk_but_before_the_final_ack_fails_the_send() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;
    assert!(
        rig.events.finished().is_empty(),
        "delivered, no final ack yet"
    );

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;
    past_the_result_wait().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id.clone(), Direction::Send, Outcome::Failed)],
        "the receiver may never have got the last chunk, so this is a failure"
    );
    assert_eq!(
        rig.audited(),
        vec![sent_row("failed")],
        "the egress is recorded"
    );
    assert!(
        aborts_sent(&rig).is_empty(),
        "nothing is told to a gone peer"
    );

    // A late final ack or result changes nothing.
    let (_, count, _) = offer_of(&rig);
    rig.svc.on_frame(dev(ANA), ack(&id, count));
    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert_eq!(rig.events.finished().len(), 1);
    assert_eq!(rig.audited().len(), 1);
}

#[tokio::test]
async fn no_final_ack_for_the_idle_limit_fails_the_send_even_after_the_last_chunk() {
    let timing = Timing {
        idle_limit: Duration::from_secs(10),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = delivered(&rig).await;

    tokio::time::advance(Duration::from_secs(9)).await;
    settle().await;
    assert!(
        rig.events.finished().is_empty(),
        "still within the idle limit"
    );

    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)],
        "no final ack means failed, not unconfirmed"
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Failed]);
    assert_eq!(rig.audited(), vec![sent_row("failed")]);
}

#[tokio::test]
async fn the_result_timer_starts_only_with_the_final_ack() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;

    // The 60 s result timeout passes with no final ack: it must not end the send.
    past_the_result_wait().await;
    assert!(
        rig.events.finished().is_empty(),
        "no result timer before the final ack"
    );

    // The final ack starts the wait; now the result timeout gives `Unconfirmed`.
    let (_, count, _) = offer_of(&rig);
    rig.svc.on_frame(dev(ANA), ack(&id, count));
    settle().await;
    assert!(rig.events.finished().is_empty());
    past_the_result_wait().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Unconfirmed)]
    );
    assert_eq!(rig.audited(), vec![sent_row("unconfirmed")]);
}

#[tokio::test]
async fn a_peer_gone_fails_an_unanswered_offer() {
    let rig = rig_with_ana();
    let id = start(&rig).await;

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert!(
        aborts_sent(&rig).is_empty(),
        "nothing is told to a gone peer"
    );
    let again = rig.svc.send(&dev(ANA), "orders", false, false).await;
    assert!(again.is_ok(), "the peer's slot is free again");
}

#[tokio::test]
async fn a_peer_gone_for_another_device_changes_nothing() {
    let rig = rig_with_ana();
    rig.peers.add(dev(3), "Ben");
    let _id = start(&rig).await;

    rig.svc.on_peer_gone(dev(3));
    settle().await;

    assert!(rig.events.finished().is_empty());
}

#[tokio::test]
async fn offer_raw_offers_the_given_bytes_and_returns_the_id() {
    let rig = rig_with_ana();
    let payload = Payload::new(40 * 1024);

    let id = rig
        .svc
        .offer_raw(&dev(ANA), "raw", payload.bytes.clone())
        .await
        .expect("offered");

    assert!(TransferId::parse(&id).is_ok());
    match rig.peers.sent_to(&dev(ANA)).first() {
        Some(PeerMessage::CollectionOffer {
            transfer_id,
            name,
            size_bytes,
            sha256,
            ..
        }) => {
            assert_eq!(transfer_id, &id);
            assert_eq!(name, "raw");
            assert_eq!(*size_bytes, 40 * 1024);
            assert_eq!(sha256, &payload.sha());
        }
        other => panic!("expected an offer, got {other:?}"),
    }
    assert_eq!(rig.exports(), 0, "no export for raw bytes");
    let again = rig
        .svc
        .offer_raw(&dev(ANA), "raw", payload.bytes)
        .await
        .expect_err("one at a time");
    assert!(matches!(&again, DomainError::InvalidInput(m) if m.contains("already running")));
}

#[tokio::test]
async fn offer_raw_frees_the_slot_when_the_offer_fails() {
    let rig = rig_with_ana();

    let err = rig
        .svc
        .offer_raw(&dev(9), "raw", vec![1; 10])
        .await
        .expect_err("not paired");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That node is not paired."));

    rig.peers.add(dev(9), "Zed");
    let ok = rig.svc.offer_raw(&dev(9), "raw", vec![1; 10]).await;
    assert!(ok.is_ok(), "the failed offer released the slot");
}

#[tokio::test]
async fn the_chunk_delay_hook_waits_before_each_chunk() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    rig.svc.set_chunk_delay(Duration::from_secs(5));

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    assert!(chunks_sent(&rig).is_empty(), "the first chunk waits too");

    tokio::time::advance(Duration::from_secs(5) + Duration::from_millis(1)).await;
    settle().await;
    assert_eq!(chunks_sent(&rig).len(), 1, "one chunk per delay");
}

#[tokio::test]
async fn the_tamper_hook_flips_a_byte_of_the_chosen_chunk() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    let (_, count, sha) = offer_of(&rig);
    rig.svc.set_tamper(Some(1));

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    let chunks = chunks_sent(&rig);
    assert_eq!(chunks.len() as u32, count, "every chunk is still sent");
    let mut joined = Vec::new();
    for (_, data) in &chunks {
        joined.extend(STANDARD.decode(data).expect("base64"));
    }
    assert_ne!(
        format!("{:x}", Sha256::digest(&joined)),
        sha,
        "the hash no longer fits"
    );
}

#[test]
fn notes_list_skipped_and_blanked_items_only() {
    let note = |path: &str, feature: &str, action: ExportAction, detail: Option<&str>| ExportNote {
        path: path.to_string(),
        feature: feature.to_string(),
        action,
        detail: detail.map(str::to_string),
    };
    let report = ExportReport {
        exported: 5,
        items: vec![
            note("flows/a.yml", "flow", ExportAction::Skipped, None),
            note("flows/b.yml", "flow", ExportAction::Skipped, None),
            note(
                "auth",
                "bearer token",
                ExportAction::Blanked,
                Some("value removed"),
            ),
            note("x.yml", "digest auth", ExportAction::Blanked, None),
            note("scripts/a.js", "rok script", ExportAction::Preserved, None),
            note(
                "req.yml",
                "oauth2",
                ExportAction::Translated,
                Some("converted"),
            ),
        ],
    };

    assert_eq!(
        export_notes(&report),
        vec![
            "flow: skipped".to_string(),
            "bearer token: value removed".to_string(),
            "digest auth: blanked".to_string(),
        ],
        "one line per kind of item, kept and translated items left out"
    );
    assert!(export_notes(&ExportReport::default()).is_empty());
}

#[tokio::test]
async fn a_short_offer_ttl_expires_the_offer_at_that_ttl() {
    let timing = Timing {
        offer_ttl: Duration::from_secs(5),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;

    tokio::time::advance(Duration::from_secs(6)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Expired)],
        "the machine and the timer both follow the short ttl"
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Expired]);
}

#[tokio::test]
async fn a_short_result_timeout_ends_the_send_unconfirmed_at_that_timeout() {
    let timing = Timing {
        result_timeout: Duration::from_secs(5),
        ..Timing::default()
    };
    let rig = rig_timed(default_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = delivered_and_acked(&rig).await;

    tokio::time::advance(Duration::from_secs(6)).await;
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Unconfirmed)]
    );
}

/// A collection whose ZIP needs about 96 chunks, three times the ack window.
fn big_files() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "opencollection.yml".to_string(),
            b"opencollection: \"1.0.0\"\ninfo:\n  name: orders\n".to_vec(),
        ),
        ("scripts/big.js".to_string(), noise(3 * 1024 * 1024)),
    ]
}

#[tokio::test]
async fn the_sender_never_has_more_than_32_chunks_unacked() {
    // A slow link: the receiver acks only when the test says so.
    let rig = rig_with(big_files());
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;
    let (_, count, _) = offer_of(&rig);
    assert!(
        count > 2 * MAX_UNACKED_CHUNKS,
        "the payload must outgrow the window"
    );

    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    assert_eq!(
        chunks_sent(&rig).len() as u32,
        MAX_UNACKED_CHUNKS,
        "the stream stops at the window without an ack"
    );

    let mut acked = 0u32;
    while acked < count {
        acked = (acked + 16).min(count);
        rig.svc.on_frame(dev(ANA), ack(&id, acked));
        settle().await;
        let sent = chunks_sent(&rig).len() as u32;
        assert!(
            sent - acked <= MAX_UNACKED_CHUNKS,
            "sent {sent}, acked {acked}"
        );
        assert!(sent <= count);
    }
    assert_eq!(
        chunks_sent(&rig).len() as u32,
        count,
        "every chunk went out in the end"
    );

    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Completed)]
    );
}

#[tokio::test]
async fn no_ack_for_the_idle_limit_fails_the_send() {
    let timing = Timing {
        idle_limit: Duration::from_secs(10),
        ..Timing::default()
    };
    let rig = rig_timed(big_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    tokio::time::advance(Duration::from_secs(9)).await;
    settle().await;
    assert!(
        rig.events.finished().is_empty(),
        "still within the idle limit"
    );

    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Failed]);
    assert_eq!(rig.audited(), vec![sent_row("failed")]);
}

#[tokio::test]
async fn an_ack_past_the_chunks_sent_fails_the_send() {
    let rig = rig_with(big_files());
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;

    rig.svc.on_frame(dev(ANA), ack(&id, 1000));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert_eq!(aborts_sent(&rig), vec![AbortReason::Failed]);
    assert_eq!(
        chunks_sent(&rig).len() as u32,
        MAX_UNACKED_CHUNKS,
        "no chunk after it"
    );
}

#[tokio::test]
async fn a_bad_collection_name_is_refused_with_the_rename_text() {
    let rig = rig_with_ana();

    // U+202E turns the rest of the name around on screen.
    let err = rig
        .svc
        .send(&dev(ANA), "orders\u{202E}gpj", false, false)
        .await
        .expect_err("a spoofing name");

    assert!(matches!(
        &err,
        DomainError::InvalidInput(m)
            if m == "This collection name cannot be sent. Rename it and try again."
    ));
    assert!(rig.peers.sent().is_empty());
    assert_eq!(rig.exports(), 0, "the name is checked before the export");
    assert!(matches!(
        offer_error(OfferError::BadName, 10),
        DomainError::InvalidInput(m)
            if m == "This collection name cannot be sent. Rename it and try again."
    ));
    start(&rig).await;
}

#[tokio::test]
async fn a_send_without_environments_carries_no_environment_files() {
    let mut files = default_files();
    files.push((
        "environments/prod.yml".to_string(),
        b"name: prod\nvariables:\n  - name: host\n    value: example.org\n".to_vec(),
    ));
    for (include, expect_env) in [(false, false), (true, true)] {
        let rig = rig_with(files.clone());
        rig.peers.add(dev(ANA), "Ana");
        let id = rig
            .svc
            .send(&dev(ANA), "orders", include, false)
            .await
            .expect("the send starts")
            .transfer_id;
        tokio::time::pause();
        rig.svc.on_frame(dev(ANA), answer(&id, true));
        settle().await;

        let mut zip = Vec::new();
        for (_, data) in chunks_sent(&rig) {
            zip.extend(STANDARD.decode(data).expect("base64"));
        }
        let has_env = zip_entry_names(&zip)
            .iter()
            .any(|name| name.contains("environments/"));
        assert_eq!(has_env, expect_env, "include_environments = {include}");
        tokio::time::resume();
    }
}

#[test]
fn outcome_names_match_the_wire_names() {
    for outcome in [
        Outcome::Completed,
        Outcome::Unconfirmed,
        Outcome::Declined,
        Outcome::Expired,
        Outcome::Cancelled,
        Outcome::Failed,
        Outcome::TooBig,
        Outcome::Busy,
        Outcome::Damaged,
        Outcome::Rejected,
    ] {
        let wire = serde_json::to_string(&outcome).expect("serializes");
        assert_eq!(wire, format!("\"{}\"", outcome_name(outcome)));
    }
}

#[tokio::test]
async fn an_end_before_the_first_chunk_writes_no_audit_row() {
    let rig = rig_with_ana();
    let id = start(&rig).await;
    rig.svc.on_frame(dev(ANA), abort(&id, AbortReason::Failed));
    settle().await;
    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
    assert!(rig.audited().is_empty());
    tokio::time::resume();

    let rig = rig_with_ana();
    let _id = start(&rig).await;
    rig.svc.on_peer_gone(dev(ANA));
    settle().await;
    assert_eq!(rig.events.finished().len(), 1);
    assert!(rig.audited().is_empty(), "no byte left the node");
}

#[tokio::test]
async fn a_peer_gone_during_the_offer_write_leaves_no_event_for_the_refused_send() {
    let rig = rig_with_ana();
    let (reached, release) = rig.peers.hold_offer();
    let svc = rig.svc.clone();
    let task = tokio::spawn(async move { svc.send(&dev(ANA), "orders", false, false).await });
    reached.notified().await;

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;
    release.notify_one();
    let result = task.await.expect("joins");

    assert!(result.is_err(), "the caller gets an error");
    assert!(rig.events.finished().is_empty(), "and no id ever finished");
    assert!(rig.audited().is_empty());
    let again = rig.svc.send(&dev(ANA), "orders", false, false).await;
    assert!(again.is_ok(), "the slot is free");
}

#[tokio::test]
async fn an_abort_after_the_final_ack_is_a_failure_with_an_audit_row() {
    for reason in [
        AbortReason::Failed,
        AbortReason::Cancelled,
        AbortReason::Busy,
    ] {
        let rig = rig_with_ana();
        let id = delivered_and_acked(&rig).await;

        rig.svc.on_frame(dev(ANA), abort(&id, reason));
        settle().await;

        assert_eq!(
            rig.events.finished(),
            vec![(id, Direction::Send, Outcome::Failed)],
            "{reason:?}"
        );
        assert_eq!(rig.audited(), vec![sent_row("failed")], "{reason:?}");
        tokio::time::resume();
    }
}

#[tokio::test]
async fn an_abort_after_the_last_chunk_before_the_final_ack_is_a_failure() {
    for reason in [AbortReason::Failed, AbortReason::Cancelled] {
        let rig = rig_with_ana();
        let id = delivered(&rig).await;

        rig.svc.on_frame(dev(ANA), abort(&id, reason));
        settle().await;

        assert_eq!(
            rig.events.finished(),
            vec![(id, Direction::Send, Outcome::Failed)],
            "{reason:?}"
        );
        assert_eq!(rig.audited(), vec![sent_row("failed")], "{reason:?}");
        tokio::time::resume();
    }
}

#[tokio::test]
async fn a_failed_send_carries_a_fixed_message() {
    let rig = rig_with_ana();
    let id = delivered(&rig).await;

    rig.svc.on_frame(dev(ANA), abort(&id, AbortReason::Failed));
    settle().await;

    assert_eq!(
        rig.events.finished_messages(),
        vec![Some("The transfer failed.".to_string())]
    );
}

#[tokio::test]
async fn a_peer_gone_twice_ends_the_send_once() {
    let rig = rig_with_ana();
    let id = delivered_and_acked(&rig).await;

    rig.svc.on_peer_gone(dev(ANA));
    rig.svc.on_peer_gone(dev(ANA));
    settle().await;
    rig.svc
        .on_frame(dev(ANA), result(&id, ResultOutcome::Completed));
    settle().await;

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Unconfirmed)]
    );
    assert_eq!(rig.audited(), vec![sent_row("unconfirmed")]);
}

#[tokio::test]
async fn a_peer_gone_ends_the_send_while_the_node_still_reads_online() {
    let rig = rig_with_ana();
    let _id = start(&rig).await;
    assert!(rig.peers.is_online(&dev(ANA)), "the state is stale");

    rig.svc.on_peer_gone(dev(ANA));
    settle().await;

    assert_eq!(rig.events.finished().len(), 1);
}

#[tokio::test]
async fn repeated_acks_do_not_restart_the_idle_clock() {
    let timing = Timing {
        idle_limit: Duration::from_secs(10),
        ..Timing::default()
    };
    let rig = rig_timed(big_files(), timing);
    rig.peers.add(dev(ANA), "Ana");
    let id = start(&rig).await;
    rig.svc.on_frame(dev(ANA), answer(&id, true));
    settle().await;
    rig.svc.on_frame(dev(ANA), ack(&id, 1));
    settle().await;

    // The same ack again and again must not keep the send alive.
    for _ in 0..4 {
        tokio::time::advance(Duration::from_secs(4)).await;
        rig.svc.on_frame(dev(ANA), ack(&id, 1));
        settle().await;
    }

    assert_eq!(
        rig.events.finished(),
        vec![(id, Direction::Send, Outcome::Failed)]
    );
}
