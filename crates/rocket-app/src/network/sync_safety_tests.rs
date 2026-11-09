//! Task 3 tests: every failure row of the state tables, the spec section 7 safety rules and
//! the spec section 8 errors. Each one checks that no file changed where the spec says so.

use std::io::Write;

use rocket_audit::AuditEventKind;
use rocket_network::limits::MAX_TRANSFER_BYTES;
use rocket_network::{
    Decision, LinkState, PeerMessage, Resolution, SyncAbortReason, SyncFinish, SyncId, SyncOutcome,
    SyncPayloadKind,
};
use rocket_network::{LinkRepository, TransferSink};
use rocket_shared::error::DomainError;

use super::sync::Phase;
use super::sync_checks::*;
use super::sync_fakes::*;
use super::sync_ports::FileChange;
use super::transfer_fakes::noise;

fn raw_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, opts).expect("entry");
            zip.write_all(bytes).expect("bytes");
        }
        zip.finish().expect("zip");
    }
    out.into_inner()
}

fn manifest_json(entries: &[(&str, &str)]) -> Vec<u8> {
    let list: Vec<serde_json::Value> = entries
        .iter()
        .map(|(path, hash)| serde_json::json!({ "path": path, "sha256": hash }))
        .collect();
    serde_json::to_vec(&serde_json::json!({ "entries": list })).expect("json")
}

/// The scripted Ana offers raw manifest bytes for link `L` with `base_hash`.
async fn ana_offers_manifest(rig: &SyncRig, bytes: &[u8], base_hash: &str) -> SyncId {
    let sid = SyncId::random();
    let offer = offered(
        sid.as_str(),
        &lid(L),
        base_hash,
        SyncPayloadKind::Manifest,
        &ptid(1),
        bytes,
    );
    give_payload(rig, ANA, &offer, bytes).await;
    sid
}

/// Pushes a manifest offer from `seed` without waiting for an answer.
fn manifest_offer_from(rig: &SyncRig, seed: u8, link: u8, base_hash: &str, n: u8) {
    let bytes = manifest_of(&base()).to_json().expect("json");
    let sid = SyncId::random();
    let offer = offered(
        sid.as_str(),
        &lid(link),
        base_hash,
        SyncPayloadKind::Manifest,
        &ptid(n),
        &bytes,
    );
    rig.push(seed, offer_frame(&offer));
}

fn assert_unchanged(rig: &SyncRig, files: &Files) {
    assert!(rig.colls.applied().is_empty(), "apply was not called");
    assert_eq!(&rig.colls.files(ORDERS), files);
}

fn audit_row(starter: bool, peer: u8, outcome: &str) -> AuditEventKind {
    if starter {
        AuditEventKind::NetworkSyncSent {
            device_id: dev(peer).short(),
            collection: ORDERS.into(),
            outcome: outcome.into(),
        }
    } else {
        AuditEventKind::NetworkSyncReceived {
            device_id: dev(peer).short(),
            collection: ORDERS.into(),
            outcome: outcome.into(),
        }
    }
}

/// Start, accept the review, confirm when asked, run to the end.
async fn full_sync_simple(pair: &mut SyncPair) {
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;
    pair.b
        .sync
        .review(&sid, true, vec![])
        .await
        .expect("accepted");
    pair.run_until(|p| p.a.events.confirms().len() == 1 || !p.a.events.finished().is_empty())
        .await;
    if pair.a.events.confirms().len() == 1 {
        pair.a.sync.confirm(&sid, true).await.expect("confirmed");
    }
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;
}

// ----- starter failure rows -----

#[tokio::test]
async fn a_peer_apply_failure_keeps_the_old_base_out_of_date() {
    for use_abort in [false, true] {
        let rig = sync_rig();
        let (sid, _after) = starter_confirmed(&rig).await;
        wait_for("our result", || {
            rig.results(ANA) == vec![SyncOutcome::Applied]
        })
        .await;
        let answer = if use_abort {
            PeerMessage::SyncAbort {
                sync_id: sid.as_str().into(),
                reason: SyncAbortReason::Failed,
            }
        } else {
            PeerMessage::SyncResult {
                sync_id: sid.as_str().into(),
                outcome: SyncOutcome::Failed,
            }
        };

        rig.push(ANA, answer);

        assert_eq!(
            wait_finished(&rig).await,
            (SyncFinish::Failed, Some(PEER_APPLY_FAILED.to_string()))
        );
        let link = wait_link(&rig, |r| r.state == LinkState::OutOfDate).await;
        assert_eq!(link.base, manifest_of(&base()), "the old base is kept");
        assert_eq!(rig.audited(), vec![audit_row(true, ANA, "failed")]);
    }
}

#[tokio::test]
async fn an_apply_failure_rolls_back_and_goes_out_of_date() {
    let rig = sync_rig();
    rig.colls
        .fail_apply
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let _ = starter_confirmed(&rig).await;

    wait_for("our result", || {
        rig.results(ANA) == vec![SyncOutcome::Failed]
    })
    .await;
    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(APPLY_FAILED.to_string()))
    );
    assert_eq!(rig.colls.files(ORDERS), base(), "rolled back");
    let link = wait_link(&rig, |r| r.state == LinkState::OutOfDate).await;
    assert_eq!(link.base, manifest_of(&base()));
}

#[tokio::test]
async fn a_local_edit_before_the_apply_stops_it() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitConfirm).await;
    // The save lands between the confirm and the apply's check inside the collection lock.
    rig.colls.edit_before_apply("a.yml", b"url: edited\n");

    rig.sync.confirm(&sid, true).await.expect("confirmed");

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(APPLY_CONFLICT.to_string()))
    );
    assert!(rig.colls.applied().is_empty(), "apply was never called");
    wait_for("the result", || {
        rig.results(ANA) == vec![SyncOutcome::Failed]
    })
    .await;
    wait_link(&rig, |r| r.state == LinkState::OutOfDate).await;
}

#[tokio::test]
async fn an_unanswered_confirm_expires() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitConfirm).await;

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Expired, None));
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Expired]
    })
    .await;
    assert_unchanged(&rig, &base());
    let err = rig.sync.confirm(&sid, true).await.expect_err("too late");
    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "The sync is no longer waiting for an answer.")
    );
}

#[tokio::test]
async fn cancel_after_confirm_is_refused() {
    let rig = sync_rig();
    let gate = rig.colls.hold_apply();
    let (sid, _) = starter_confirmed(&rig).await;
    wait_phase(&rig, &sid, Phase::Applying).await;
    let err = rig.sync.cancel(&sid).await.expect_err("refused");
    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "The sync is already being applied.")
    );
    drop(gate);

    let rig = sync_rig();
    let (sid, _) = planner_waiting(&rig).await;
    let err = rig.sync.cancel(&sid).await.expect_err("refused");
    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "The sync is already being applied.")
    );

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;
    rig.sync
        .cancel(&sid)
        .await
        .expect("cancel in the dialog declines");
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Declined, None));
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Declined]
    })
    .await;
    assert!(rig.sync.cancel(&sid).await.is_ok(), "a finished id is fine");
}

#[tokio::test]
async fn each_abort_reason_ends_the_starter_with_its_outcome() {
    use SyncAbortReason as R;
    let rows = [
        (R::Declined, SyncFinish::Declined, None),
        (R::Expired, SyncFinish::Expired, None),
        (R::Cancelled, SyncFinish::Cancelled, None),
        (
            R::BaseMismatch,
            SyncFinish::BaseMismatch,
            Some(BASE_MISMATCH),
        ),
        (R::LinkMissing, SyncFinish::LinkMissing, Some(LINK_MISSING)),
        (R::Busy, SyncFinish::Failed, Some(BUSY)),
        (R::TooBig, SyncFinish::Failed, Some(PEER_TOO_BIG)),
        (R::Failed, SyncFinish::Failed, Some(PEER_FAILED)),
    ];
    for (reason, finish, text) in rows {
        let rig = sync_rig();
        rig.linked(&lid(L), &base(), &base());
        let (sid, ..) = bo_starts(&rig).await;

        rig.push(
            ANA,
            PeerMessage::SyncAbort {
                sync_id: sid.as_str().into(),
                reason,
            },
        );

        assert_eq!(
            wait_finished(&rig).await,
            (finish, text.map(str::to_string)),
            "{reason:?}"
        );
        assert_unchanged(&rig, &base());
        assert!(rig.aborts(ANA).is_empty(), "nothing is sent back");
        assert_eq!(
            rig.audited(),
            vec![audit_row(true, ANA, finish_name(finish))]
        );
        let want = if reason == R::LinkMissing {
            LinkState::Broken
        } else {
            LinkState::InSync
        };
        wait_link(&rig, |r| r.state == want).await;
    }
}

#[tokio::test]
async fn a_peer_gone_before_the_apply_changes_nothing() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitConfirm).await;

    rig.router.on_peer_gone(dev(ANA));

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(WENT_OFFLINE.to_string()))
    );
    assert!(rig.sync.confirm(&sid, true).await.is_err());
    assert_unchanged(&rig, &base());

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;

    rig.router.on_peer_gone(dev(ANA));

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(WENT_OFFLINE.to_string()))
    );
    assert!(rig.sync.review(&sid, true, vec![]).await.is_err());
    assert_unchanged(&rig, &base());
    assert_eq!(rig.links.count(), 1, "a dropped connection keeps the link");
}

#[tokio::test]
async fn a_silent_planner_fails_the_starter_after_the_wait() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let _ = bo_starts(&rig).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(STALLED.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;

    // A peer that answers the manifest offer and never acks stalls the stream the same way.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.sync.start(&lid(L)).await.expect("started");
    let offer = nth_offer(&rig, ANA, SyncPayloadKind::Manifest, 0).await;
    rig.push(
        ANA,
        PeerMessage::OfferAnswer {
            transfer_id: offer.transfer_id,
            accept: true,
        },
    );

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(STALLED.to_string()))
    );
}

#[tokio::test]
async fn a_local_edit_before_the_export_stops_the_sync() {
    let rig = sync_rig();
    let mine = with(&base(), "a.yml", "url: a2\n");
    rig.linked(&lid(L), &base(), &mine);
    let (sid, ..) = bo_starts(&rig).await;
    rig.colls.put(ORDERS, "a.yml", b"url: a3\n");

    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["a.yml"])).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(CHANGED.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;
    assert!(
        rig.sent(ANA)
            .iter()
            .filter_map(as_offer)
            .all(|o| o.kind != SyncPayloadKind::Files),
        "no files were offered"
    );
}

#[tokio::test]
async fn too_big_changes_are_refused_before_any_offer() {
    // About 20 MiB pass through the fakes in a debug build, so the clocks are long and the
    // waits are 30 s: a loaded machine must not fail the test.
    let rig = sync_rig_timed(slow());
    let mut mine = base();
    mine.insert("big.yml".into(), noise(MAX_TRANSFER_BYTES as usize));
    rig.linked(&lid(L), &base(), &mine);
    let (sid, ..) = bo_starts(&rig).await;

    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["big.yml"])).await;

    wait_long("the finished event", || !rig.events.finished().is_empty()).await;
    let (finish, message) = wait_finished(&rig).await;
    assert_eq!(finish, SyncFinish::Failed);
    let message = message.expect("a text");
    assert!(
        message.starts_with("The changes are too big to sync ("),
        "{message}"
    );
    assert!(message.ends_with(" MiB, limit 10)."), "{message}");
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::TooBig]
    })
    .await;
    assert!(rig
        .sent(ANA)
        .iter()
        .filter_map(as_offer)
        .all(|o| o.kind != SyncPayloadKind::Files));

    // Just under the cap passes: the files payload is offered and arrives whole.
    let rig = sync_rig_timed(slow());
    let mut mine = base();
    mine.insert(
        "big.yml".into(),
        noise(MAX_TRANSFER_BYTES as usize - 64 * 1024),
    );
    rig.linked(&lid(L), &base(), &mine);
    let (sid, ..) = bo_starts(&rig).await;
    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["big.yml"])).await;
    wait_long("the files offer", || {
        rig.sent(ANA)
            .iter()
            .filter_map(as_offer)
            .any(|o| o.kind == SyncPayloadKind::Files)
    })
    .await;
    let (offer, zip) = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    assert!(offer.size_bytes <= MAX_TRANSFER_BYTES);
    assert_eq!(zip.len() as u64, offer.size_bytes);
}

#[tokio::test]
async fn a_sync_offer_out_of_phase_ends_the_session() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let zip = zip_of(&files(&[("b.yml", "x")]));
    rig.push(
        ANA,
        offer_frame(&offered(
            sid.as_str(),
            &lid(L),
            &bh(),
            SyncPayloadKind::Files,
            &ptid(2),
            &zip,
        )),
    );
    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(BAD_PLAN.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let plan = request(&["a.yml"]).to_json().expect("json");
    let other_base = "e".repeat(64);
    rig.push(
        ANA,
        offer_frame(&offered(
            sid.as_str(),
            &lid(L),
            &other_base,
            SyncPayloadKind::Plan,
            &ptid(2),
            &plan,
        )),
    );
    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(BAD_PLAN.to_string()))
    );

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let _ = bo_starts(&rig).await;
    let stray = SyncId::random();
    rig.push(
        ANA,
        offer_frame(&offered(
            stray.as_str(),
            &lid(L),
            &bh(),
            SyncPayloadKind::Plan,
            &ptid(2),
            &plan,
        )),
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        rig.events.finished().is_empty(),
        "an unknown sync id is ignored"
    );
}

// ----- planner failure rows and caps -----

/// Pairs Ana, Ben, Cy and Di, each with its own link (1 to 4) to `orders`.
fn hub() -> SyncRig {
    let rig = sync_rig();
    rig.colls.add(ORDERS, base());
    let base_files: Vec<(String, Vec<u8>)> = base().into_iter().collect();
    for (n, (seed, name)) in [(ANA, "Ana"), (BEN, "Ben"), (4, "Cy"), (5, "Di")]
        .into_iter()
        .enumerate()
    {
        rig.peers.add(dev(seed), name);
        rig.links
            .save(&record(&lid(n as u8 + 1), seed, &base()), &base_files)
            .expect("link");
    }
    rig
}

#[tokio::test]
async fn a_fourth_waiting_sync_is_busy() {
    let rig = hub();
    let theirs = manifest_of(&without(&base(), "a.yml"))
        .to_json()
        .expect("json");
    for (n, seed) in [(1u8, ANA), (2, BEN), (3, 4)] {
        let sid = SyncId::random();
        let offer = offered(
            sid.as_str(),
            &lid(n),
            &bh(),
            SyncPayloadKind::Manifest,
            &ptid(n),
            &theirs,
        );
        give_payload(&rig, seed, &offer, &theirs).await;
    }
    wait_for("three reviews", || rig.events.reviews().len() == 3).await;

    let sid = SyncId::random();
    let offer = offered(
        sid.as_str(),
        &lid(4),
        &bh(),
        SyncPayloadKind::Manifest,
        &ptid(4),
        &theirs,
    );
    rig.push(5, offer_frame(&offer));

    wait_for("the busy answer", || {
        rig.aborts(5) == vec![SyncAbortReason::Busy]
    })
    .await;
    assert_eq!(rig.events.reviews().len(), 3);
}

#[tokio::test]
async fn a_second_session_with_one_peer_is_busy() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.sync.start(&lid(L)).await.expect("Bo starts");

    manifest_offer_from(&rig, ANA, L, &bh(), 9);

    wait_for("the busy answer", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Busy]
    })
    .await;
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn the_sync_rate_window_refuses_the_sixth_start_in_a_minute() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let wrong = "e".repeat(64);
    for n in 1..=5u8 {
        manifest_offer_from(&rig, ANA, L, &wrong, n);
        wait_for("the base mismatch", || rig.aborts(ANA).len() == n as usize).await;
    }

    manifest_offer_from(&rig, ANA, L, &bh(), 6);

    wait_for("the sixth answer", || rig.aborts(ANA).len() == 6).await;
    assert_eq!(rig.aborts(ANA)[5], SyncAbortReason::Busy);
    assert!(rig.aborts(ANA)[..5]
        .iter()
        .all(|r| *r == SyncAbortReason::BaseMismatch));
}

#[tokio::test]
async fn follow_up_sync_offers_do_not_count_against_the_rate_window() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let wrong = "e".repeat(64);
    for n in 1..=4u8 {
        manifest_offer_from(&rig, ANA, L, &wrong, n);
        wait_for("the base mismatch", || rig.aborts(ANA).len() == n as usize).await;
    }
    let theirs = with(&base(), "a.yml", "url: a-ana\n");

    // The fifth start uses the last hit; its files offer must still be admitted.
    let sid = ana_starts(&rig, &theirs).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    ana_sends_files(
        &rig,
        sid.as_str(),
        7,
        &zip_of(&files(&[("a.yml", "url: a-ana\n")])),
    )
    .await;

    wait_for("the review", || rig.events.reviews().len() == 1).await;
    assert_eq!(rig.aborts(ANA).len(), 4, "no busy answer");
}

#[tokio::test]
async fn a_sync_does_not_use_the_collection_transfer_slots() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;

    let zip = zip_of(&base());
    rig.push(
        ANA,
        PeerMessage::CollectionOffer {
            transfer_id: ptid(40),
            name: ORDERS.into(),
            size_bytes: zip.len() as u64,
            chunk_count: rocket_network::chunk_count_for(zip.len() as u64),
            sha256: sha(&zip),
            keep_in_sync: false,
            link_id: None,
        },
    );
    wait_for("the collection offer is shown", || {
        !rig.transfer_events.all().is_empty()
    })
    .await;
    rig.transfer
        .send(&dev(ANA), ORDERS, false, false)
        .await
        .expect("a collection send to the same peer is not blocked by the sync");
}

#[tokio::test]
async fn an_unknown_or_broken_link_is_link_missing() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());
    manifest_offer_from(&rig, ANA, L, &bh(), 1);
    wait_for("link missing", || {
        rig.aborts(ANA) == vec![SyncAbortReason::LinkMissing]
    })
    .await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let mut broken = rig.links.record(&lid(L)).expect("link");
    broken.state = LinkState::Broken;
    rig.links.save(&broken, &[]).expect("saved");
    manifest_offer_from(&rig, ANA, L, &bh(), 1);
    wait_for("link missing", || {
        rig.aborts(ANA) == vec![SyncAbortReason::LinkMissing]
    })
    .await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.peers.add(dev(BEN), "Ben");
    manifest_offer_from(&rig, BEN, L, &bh(), 1);
    wait_for("link missing", || {
        rig.aborts(BEN) == vec![SyncAbortReason::LinkMissing]
    })
    .await;
    assert!(
        rig.events.all().is_empty(),
        "the user never sees a refused offer"
    );
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn a_moved_collection_is_link_missing_and_broken() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.colls.set_root(ORDERS, "/elsewhere/orders");

    manifest_offer_from(&rig, ANA, L, &bh(), 1);

    wait_for("link missing", || {
        rig.aborts(ANA) == vec![SyncAbortReason::LinkMissing]
    })
    .await;
    wait_link(&rig, |r| r.state == LinkState::Broken).await;
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_base_mismatch_is_answered_and_changes_nothing() {
    let rig = sync_rig();
    let other_base = with(&base(), "a.yml", "url: other\n");
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());
    let base_files: Vec<(String, Vec<u8>)> = other_base.clone().into_iter().collect();
    rig.links
        .save(&record(&lid(L), ANA, &other_base), &base_files)
        .expect("link");

    manifest_offer_from(&rig, ANA, L, &bh(), 1);

    wait_for("the mismatch", || {
        rig.aborts(ANA) == vec![SyncAbortReason::BaseMismatch]
    })
    .await;
    assert!(rig.events.all().is_empty());
    assert!(rig.audited().is_empty(), "no chunk left this node");
    assert_eq!(
        rig.links.record(&lid(L)).expect("link").base,
        manifest_of(&other_base)
    );
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn review_needs_one_valid_decision_per_conflict() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "a.yml", "url: a-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    let err = pair
        .b
        .sync
        .review(&sid, true, vec![])
        .await
        .expect_err("no decision");
    assert!(matches!(err, DomainError::InvalidInput(_)));
    let err = pair
        .b
        .sync
        .review(
            &sid,
            true,
            vec![Decision {
                path: "b.yml".into(),
                resolution: Resolution::KeepMine,
            }],
        )
        .await
        .expect_err("not a conflict");
    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "That file is not a conflict."));
    assert_eq!(
        pair.b.sync.phase_of(&sid),
        Some(Phase::AwaitReview),
        "the review stays open"
    );

    pair.b
        .sync
        .review(
            &sid,
            true,
            vec![Decision {
                path: "a.yml".into(),
                resolution: Resolution::KeepMine,
            }],
        )
        .await
        .expect("now valid");
}

#[tokio::test]
async fn declining_the_review_changes_nothing_on_both_sides() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    pair.b
        .sync
        .review(&sid, false, vec![])
        .await
        .expect("declined");
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;

    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Declined);
    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Declined);
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &base());
    assert_eq!(
        pair.a.links.record(&lid(L)).expect("a").base,
        manifest_of(&base())
    );
    assert_eq!(
        pair.b.links.record(&lid(L)).expect("b").base,
        manifest_of(&base())
    );
    assert_eq!(pair.a.audited(), vec![audit_row(true, ANA, "declined")]);
    assert_eq!(
        pair.b.audited(),
        vec![audit_row(false, BO, "declined")],
        "the request left Ana"
    );
}

#[tokio::test]
async fn declining_the_confirm_changes_nothing() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;
    pair.b
        .sync
        .review(&sid, true, vec![])
        .await
        .expect("accepted");
    pair.run_until(|p| p.a.events.confirms().len() == 1).await;

    pair.a.sync.confirm(&sid, false).await.expect("declined");
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;

    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Declined);
    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Declined);
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &ana);
}

#[tokio::test]
async fn an_unanswered_review_expires_on_both_sides() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());
    pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;

    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Expired);
    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Expired);
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &base());
}

#[tokio::test]
async fn cancel_before_the_dialog_ends_the_sync_on_both_sides() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    pair.a.sync.cancel(&sid).await.expect("cancelled");
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;

    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Cancelled);
    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Cancelled);
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &base());

    // The planner cancels while it waits for the starter's files.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &with(&base(), "a.yml", "url: a-ana\n")).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitFiles).await;
    rig.sync.cancel(&sid).await.expect("cancelled");
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Cancelled, None));
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Cancelled]
    })
    .await;
    assert_eq!(rig.audited(), vec![audit_row(false, ANA, "cancelled")]);
}

#[tokio::test]
async fn a_planner_apply_failure_tells_the_starter_and_goes_out_of_date() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    pair.b
        .colls
        .fail_apply
        .store(true, std::sync::atomic::Ordering::SeqCst);

    full_sync_simple(&mut pair).await;

    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Failed);
    assert_eq!(pair.b.events.finished()[0].2.as_deref(), Some(APPLY_FAILED));
    assert_eq!(
        pair.a.events.finished()[0].2.as_deref(),
        Some(PEER_APPLY_FAILED)
    );
    assert_eq!(pair.b.colls.files(ORDERS), ana, "the planner rolled back");
    let both = with(&bo, "b.yml", "url: b-ana\n");
    assert_eq!(pair.a.colls.files(ORDERS), both, "the starter had applied");
    pair.run_until(|p| {
        p.a.links
            .record(&lid(L))
            .is_some_and(|r| r.state == LinkState::OutOfDate)
            && p.b
                .links
                .record(&lid(L))
                .is_some_and(|r| r.state == LinkState::OutOfDate)
    })
    .await;
    assert_eq!(
        pair.a.links.record(&lid(L)).expect("a").base,
        manifest_of(&base())
    );
    assert_eq!(
        pair.b.links.record(&lid(L)).expect("b").base,
        manifest_of(&base())
    );

    // The next sync starts from the old bases and finds the starter's files already equal.
    pair.b
        .colls
        .fail_apply
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started again");
    pair.run_until(|p| p.b.events.reviews().len() == 2).await;
    pair.b
        .sync
        .review(&sid, true, vec![])
        .await
        .expect("accepted");
    pair.run_until(|p| p.b.events.finished().len() == 2 && p.a.events.finished().len() == 2)
        .await;
    assert_eq!(pair.b.events.finished()[1].1, SyncFinish::Applied);
    assert_eq!(pair.b.colls.files(ORDERS), both);
    assert_eq!(
        pair.a.events.confirms().len(),
        1,
        "the second sync asks the starter nothing"
    );
}

#[tokio::test]
async fn a_starter_apply_failure_marks_the_planner_out_of_date() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    pair.a
        .colls
        .fail_apply
        .store(true, std::sync::atomic::Ordering::SeqCst);

    full_sync_simple(&mut pair).await;

    assert_eq!(pair.a.events.finished()[0].2.as_deref(), Some(APPLY_FAILED));
    assert_eq!(
        pair.b.events.finished()[0].2.as_deref(),
        Some(PEER_APPLY_FAILED)
    );
    assert_unchanged(&pair.b, &ana);
    assert_eq!(pair.a.colls.files(ORDERS), bo);
    pair.run_until(|p| {
        p.b.links
            .record(&lid(L))
            .is_some_and(|r| r.state == LinkState::OutOfDate)
    })
    .await;
}

#[tokio::test]
async fn a_silent_starter_fails_the_planner_after_the_wait() {
    let rig = sync_rig();
    let (sid, _) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(STALLED.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;
    assert!(rig.colls.applied().is_empty());
}

#[tokio::test]
async fn a_local_edit_during_the_review_stops_the_sync() {
    let rig = sync_rig();
    let mine = with(&base(), "n.yml", "url: n\n");
    rig.linked(&lid(L), &base(), &mine);
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;
    rig.colls.put(ORDERS, "n.yml", b"url: n-edited\n");

    rig.sync
        .review(&sid, true, vec![])
        .await
        .expect("the reason goes to Finished");

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(CHANGED.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;
    assert!(
        rig.sent(ANA).iter().filter_map(as_offer).next().is_none(),
        "no final plan"
    );
    assert!(rig.colls.applied().is_empty());
}

// ----- spec section 7 safety rules -----

#[tokio::test]
async fn a_manifest_with_an_unsafe_path_is_refused() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let bytes = manifest_json(&[("../SECRET-TOKEN.yml", &"a".repeat(64))]);

    ana_offers_manifest(&rig, &bytes, &bh()).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(UNSAFE.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;
    assert!(rig.events.reviews().is_empty());
    for event in rig.events.all() {
        let json = serde_json::to_string(&event).expect("json");
        assert!(!json.contains("SECRET-TOKEN"), "no peer text in any event");
    }
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_nested_collection_file_is_refused() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let mut theirs = manifest_of(&base());
    theirs
        .insert("f/opencollection.yml", &"a".repeat(64))
        .expect("Plan 1 allows the path");
    let bytes = theirs.to_json().expect("json");

    ana_offers_manifest(&rig, &bytes, &bh()).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(UNSAFE.to_string()))
    );
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_request_for_a_path_not_offered_is_refused() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;

    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["environments/prod.yml"])).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(UNSAFE.to_string()))
    );
    assert!(rig
        .sent(ANA)
        .iter()
        .filter_map(as_offer)
        .all(|o| o.kind != SyncPayloadKind::Files));
}

/// The deletes and the renames `(from, to)` of one refused final plan.
type PlanCase = (Vec<&'static str>, Vec<(&'static str, &'static str)>);

#[tokio::test]
async fn a_final_plan_with_an_unsafe_or_unknown_path_is_refused() {
    let cases: Vec<PlanCase> = vec![
        (vec!["../x.yml"], vec![]),
        (vec!["x.yml"], vec![]),
        (vec![], vec![("a.yml", "f/opencollection.yml")]),
        (vec![], vec![("a.yml", ".rocket/a.yml")]),
        // The root collection file is never deleted or moved by a sync.
        (vec!["opencollection.yml"], vec![]),
        (vec![], vec![("opencollection.yml", "orders.yml")]),
        (vec!["a.yml"], vec![("b.yml", "OpenCollection.yml")]),
    ];
    for (deletes, renames) in cases {
        let rig = sync_rig();
        rig.linked(&lid(L), &base(), &base());
        let (sid, ..) = bo_starts(&rig).await;
        let mut plan = empty_plan();
        plan.deletes = deletes.iter().map(|s| s.to_string()).collect();
        plan.renames = renames
            .iter()
            .map(|(f, t)| rocket_network::PlanRename {
                from: f.to_string(),
                to: t.to_string(),
            })
            .collect();
        plan.new_base_hash = Some(bh());

        ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

        assert_eq!(
            wait_finished(&rig).await,
            (SyncFinish::Failed, Some(UNSAFE.to_string())),
            "{deletes:?} {renames:?}"
        );
        assert!(rig.events.confirms().is_empty());
        assert_unchanged(&rig, &base());
    }
}

#[tokio::test]
async fn a_final_plan_whose_base_hash_disagrees_is_refused() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (mut plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    plan.new_base_hash = Some(manifest_of(&base()).base_hash());

    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(BAD_PLAN.to_string()))
    );
    assert!(rig.events.confirms().is_empty());
    assert_unchanged(&rig, &base());
    assert_eq!(
        rig.links.record(&lid(L)).expect("link").base,
        manifest_of(&base()),
        "the peer's idea of the base is never stored"
    );
}

#[tokio::test]
async fn a_files_zip_with_an_extra_or_missing_entry_is_unsafe() {
    // Starter: the plan sends b.yml, the ZIP also holds x.yml.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &[], &[], &[("b.yml", "url: b2\n")]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    let zip = zip_of(&files(&[("b.yml", "url: b2\n"), ("x.yml", "url: x\n")]));
    ana_sends_files(&rig, sid.as_str(), 3, &zip).await;
    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(UNSAFE.to_string()))
    );
    assert!(rig.events.confirms().is_empty());
    assert_unchanged(&rig, &base());

    // Planner: it asked for a.yml and c.yml, the ZIP holds only a.yml.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let theirs = with(&with(&base(), "a.yml", "url: a2\n"), "c.yml", "url: c\n");
    let sid = ana_starts(&rig, &theirs).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    ana_sends_files(
        &rig,
        sid.as_str(),
        2,
        &zip_of(&files(&[("a.yml", "url: a2\n")])),
    )
    .await;
    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(UNSAFE.to_string()))
    );
    assert!(rig.events.reviews().is_empty());
}

#[tokio::test]
async fn a_files_zip_that_fails_the_strict_validator_is_unsafe() {
    let mut trailing = zip_of(&files(&[("b.yml", "url: b2\n")]));
    trailing.push(0);
    for zip in [
        raw_zip(&[("B.yml", b"url: b2\n"), ("b.yml", b"url: b2\n")]),
        raw_zip(&[("../b.yml", b"url: b2\n")]),
        trailing,
    ] {
        let rig = sync_rig();
        rig.linked(&lid(L), &base(), &base());
        let (sid, ..) = bo_starts(&rig).await;
        let (plan, _) = final_plan(&base(), &[], &[], &[("b.yml", "url: b2\n")]);
        ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

        ana_sends_files(&rig, sid.as_str(), 3, &zip).await;

        assert_eq!(
            wait_finished(&rig).await,
            (SyncFinish::Failed, Some(UNSAFE.to_string()))
        );
        assert_unchanged(&rig, &base());
    }
}

#[tokio::test]
async fn a_damaged_payload_is_refused_before_any_dialog() {
    // The offer's hash does not match the bytes that arrive (same length).
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let theirs = with(&base(), "a.yml", "url: a2\n");
    let sid = ana_starts(&rig, &theirs).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let zip = zip_of(&files(&[("a.yml", "url: a2\n")]));
    let mut offer = offered(
        sid.as_str(),
        &lid(L),
        &bh(),
        SyncPayloadKind::Files,
        &ptid(2),
        &zip,
    );
    offer.sha256 = sha(b"something else");
    give_payload(&rig, ANA, &offer, &zip).await;
    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(DAMAGED.to_string()))
    );
    assert!(rig.events.reviews().is_empty());

    // A chunk out of order fails at once.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let bytes = manifest_of(&base()).to_json().expect("json");
    let sid = SyncId::random();
    let offer = offered(
        sid.as_str(),
        &lid(L),
        &bh(),
        SyncPayloadKind::Manifest,
        &ptid(1),
        &bytes,
    );
    rig.push(ANA, offer_frame(&offer));
    wait_for("the answer", || !rig.sent(ANA).is_empty()).await;
    let mut chunk = chunk_frames(&ptid(1), &bytes).remove(0);
    if let PeerMessage::Chunk { seq, .. } = &mut chunk {
        *seq = 1;
    }
    rig.push(ANA, chunk);
    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(DAMAGED.to_string()))
    );
}

#[tokio::test]
async fn a_file_whose_hash_differs_from_the_plan_is_damaged() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &[], &[], &[("b.yml", "url: b2\n")]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

    ana_sends_files(
        &rig,
        sid.as_str(),
        3,
        &zip_of(&files(&[("b.yml", "url: evil\n")])),
    )
    .await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(DAMAGED.to_string()))
    );
    assert!(rig.events.confirms().is_empty());
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_sync_writes_only_through_the_collection_port() {
    let bo = with(&without(&base(), "b.yml"), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "c.yml", "url: c\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let saves_before = (pair.a.links.saves(), pair.b.links.saves());

    full_sync_simple(&mut pair).await;
    pair.run_until(|p| p.a.links.saves() > saves_before.0 && p.b.links.saves() > saves_before.1)
        .await;

    // SyncDeps has no trust port: the collection port and the link repository are the only
    // writers, and every write names a path inside the collection.
    for rig in [&pair.a, &pair.b] {
        for (collection, changes) in rig.colls.applied() {
            assert_eq!(collection, ORDERS);
            for change in changes {
                let paths = match &change {
                    FileChange::Put { path, .. }
                    | FileChange::PutAs { path, .. }
                    | FileChange::Delete { path } => vec![path.clone()],
                    FileChange::Rename { from, to } => vec![from.clone(), to.clone()],
                };
                for path in paths {
                    assert!(path_allowed(&path), "{path}");
                    assert_ne!(path, "trust.yml");
                }
            }
        }
    }
    assert_eq!(
        pair.a.links.saves(),
        saves_before.0 + 1,
        "one link save per side"
    );
    assert_eq!(pair.b.links.saves(), saves_before.1 + 1);
}

#[tokio::test]
async fn a_synced_collection_file_is_always_merged_never_written_raw() {
    // A hostile peer whose collection file asks for Developer mode. The real export strips
    // `extensions.rocketapi` and Plan 2's merge_file keeps the local one (its own test); here
    // the service must hand the file to the merge (Put), never write it as received (PutAs),
    // and trust lives in ~/.rocket-api/trust.yml, which no sync can reach.
    let hostile = "name: orders\nextensions:\n  rocketapi:\n    sandboxMode: developer\n";
    let bo = with(&base(), "opencollection.yml", hostile);
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());

    full_sync_simple(&mut pair).await;

    let changes: Vec<FileChange> = pair
        .b
        .colls
        .applied()
        .into_iter()
        .flat_map(|(_, c)| c)
        .collect();
    assert!(changes.contains(&FileChange::Put {
        path: "opencollection.yml".into(),
        bytes: hostile.as_bytes().to_vec(),
    }));
    assert!(!changes
        .iter()
        .any(|c| matches!(c, FileChange::PutAs { .. })));
}

#[tokio::test]
async fn keep_both_is_refused_for_settings_files() {
    let bo = with(&base(), "opencollection.yml", "name: orders-bo\n");
    let ana = with(&base(), "opencollection.yml", "name: orders-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    let err = pair
        .b
        .sync
        .review(
            &sid,
            true,
            vec![Decision {
                path: "opencollection.yml".into(),
                resolution: Resolution::KeepBoth,
            }],
        )
        .await
        .expect_err("refused");

    assert!(matches!(&err, DomainError::InvalidInput(m)
        if m == "Keep both is not possible for collection or folder settings."));
    pair.b
        .sync
        .review(
            &sid,
            true,
            vec![Decision {
                path: "opencollection.yml".into(),
                resolution: Resolution::KeepTheirs,
            }],
        )
        .await
        .expect("keep theirs is fine");
}

#[tokio::test]
async fn removing_the_pairing_mid_sync_ends_it_with_no_change() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());
    pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    // Ana removes the pairing; Bo hears `Unpair`, which removes it there too.
    pair.b.peers.forget(&dev(BO));
    pair.b.router.on_peer_gone(dev(BO));
    pair.a.peers.forget(&dev(ANA));
    pair.a.router.on_peer_gone(dev(ANA));

    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;
    assert_eq!(pair.a.events.finished()[0].2.as_deref(), Some(WENT_OFFLINE));
    assert_eq!(pair.b.events.finished()[0].2.as_deref(), Some(WENT_OFFLINE));
    assert_unchanged(&pair.a, &bo);
    assert_unchanged(&pair.b, &base());
    pair.run_until(|p| p.a.links.count() == 0 && p.b.links.count() == 0)
        .await;
}

#[tokio::test]
async fn unlinking_during_a_sync_cancels_it() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    wait_phase(&rig, &sid, Phase::AwaitPlan).await;

    rig.sync.unlink(&lid(L)).await.expect("unlinked");

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Cancelled, None));
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Cancelled]
    })
    .await;
    assert_eq!(rig.links.count(), 0);
}

// ----- worker, texts, audit -----

#[tokio::test]
async fn a_slow_apply_does_not_hold_back_another_peer() {
    let rig = sync_rig();
    let (sid, _) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;
    rig.peers.add(dev(BEN), "Ben");
    let base_files: Vec<(String, Vec<u8>)> = base().into_iter().collect();
    rig.links
        .save(&record(&lid(2), BEN, &base()), &base_files)
        .expect("Ben's link");
    let gate = rig.colls.hold_apply();
    rig.push(
        ANA,
        PeerMessage::SyncResult {
            sync_id: sid.as_str().into(),
            outcome: SyncOutcome::Applied,
        },
    );
    wait_phase(&rig, &sid, Phase::Applying).await;

    let ben = SyncId::random();
    let bytes = manifest_of(&base()).to_json().expect("json");
    let offer = offered(
        ben.as_str(),
        &lid(2),
        &bh(),
        SyncPayloadKind::Manifest,
        &ptid(30),
        &bytes,
    );
    give_payload(&rig, BEN, &offer, &bytes).await;

    wait_for("Ben's review while Ana's apply is held", || {
        rig.events.reviews().len() == 2
    })
    .await;
    drop(gate);
    wait_for("Ana's sync ended", || !rig.events.finished().is_empty()).await;
}

#[test]
fn every_finished_message_is_one_of_the_fixed_texts() {
    let fixed = [
        WENT_OFFLINE,
        DAMAGED,
        UNSAFE,
        BAD_PLAN,
        BASE_MISMATCH,
        LINK_MISSING,
        CHANGED,
        APPLY_FAILED,
        PEER_APPLY_FAILED,
        PEER_FAILED,
        PEER_TOO_BIG,
        BUSY,
        STALLED,
        INTERNAL,
        APPLY_CONFLICT,
        CASE_CLASH,
    ];
    let mut stops = vec![
        Stop::Damaged,
        Stop::Unsafe,
        Stop::BadPlan,
        Stop::Changed,
        Stop::ApplyFailed,
        Stop::PeerApplyFailed,
        Stop::ApplyConflict,
        Stop::CaseClash,
        Stop::WentOffline,
        Stop::Stalled,
        Stop::Internal,
        Stop::Declined,
        Stop::Expired,
        Stop::Cancelled,
    ];
    for r in [
        SyncAbortReason::BaseMismatch,
        SyncAbortReason::LinkMissing,
        SyncAbortReason::Busy,
        SyncAbortReason::Cancelled,
        SyncAbortReason::Declined,
        SyncAbortReason::Expired,
        SyncAbortReason::Failed,
        SyncAbortReason::TooBig,
    ] {
        stops.push(Stop::Peer(r));
    }
    for stop in stops {
        if let Some(text) = stop.message() {
            assert!(fixed.contains(&text.as_str()), "{stop:?}: {text}");
        }
    }
}

#[tokio::test]
async fn a_refusal_before_any_chunk_writes_no_audit() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    manifest_offer_from(&rig, ANA, L, &"e".repeat(64), 1);
    wait_for("the mismatch", || rig.aborts(ANA).len() == 1).await;
    manifest_offer_from(&rig, ANA, 9, &bh(), 2);
    wait_for("link missing", || rig.aborts(ANA).len() == 2).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(rig.audited().is_empty());
    assert!(rig.events.finished().is_empty());
}

// ----- review fixes from Task 2 (N1 to N4) and the leftovers -----

#[tokio::test]
async fn a_payload_step_slower_than_the_idle_limit_is_not_stalled() {
    // N1: once the last chunk is in, the step that checks the payload owns the phase. A step
    // that takes longer than the idle limit must not end the sync "stopped answering".
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    *rig.sync.processing_delay.lock().expect("delay lock") = std::time::Duration::from_millis(700);
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);

    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

    wait_for("the confirm", || rig.events.confirms().len() == 1).await;
    assert!(rig.events.finished().is_empty(), "not stalled");
    assert_eq!(rig.sync.phase_of(&sid), Some(Phase::AwaitConfirm));
}

#[tokio::test]
async fn a_slow_manifest_build_does_not_count_against_the_answer() {
    // A large collection takes a while to export. The idle and offer clocks start once the
    // manifest is built, not when `start` was called.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let gate = rig.colls.hold_manifest();
    let sync = rig.sync.clone();
    let start = tokio::spawn(async move { sync.start(&lid(L)).await });
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    drop(gate);
    let sid = start.await.expect("joined").expect("started");

    let _ = take_payload(&rig, ANA, SyncPayloadKind::Manifest, 0).await;

    wait_phase(&rig, &sid, Phase::AwaitPlan).await;
    assert!(rig.events.finished().is_empty(), "not stalled");
}

#[tokio::test]
async fn a_collection_offer_reusing_the_manifest_id_during_the_check_is_refused() {
    // N2: the planner holds the manifest's transfer id from the moment its session opens.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let gate = rig.links.hold_get();
    let sid = SyncId::random();
    let bytes = manifest_of(&without(&base(), "a.yml"))
        .to_json()
        .expect("json");
    let offer = offered(
        sid.as_str(),
        &lid(L),
        &bh(),
        SyncPayloadKind::Manifest,
        &ptid(1),
        &bytes,
    );
    rig.push(ANA, offer_frame(&offer));
    wait_phase(&rig, &sid, Phase::Checking).await;

    let zip = zip_of(&base());
    rig.push(
        ANA,
        PeerMessage::CollectionOffer {
            transfer_id: ptid(1),
            name: ORDERS.into(),
            size_bytes: zip.len() as u64,
            chunk_count: rocket_network::chunk_count_for(zip.len() as u64),
            sha256: sha(&zip),
            keep_in_sync: false,
            link_id: None,
        },
    );

    wait_for("the refusal", || {
        rig.sent(ANA).iter().any(|m| {
            matches!(m, PeerMessage::TransferAbort { transfer_id, .. } if *transfer_id == ptid(1))
        })
    })
    .await;
    assert!(rig.transfer_events.all().is_empty(), "never shown");
    drop(gate);
    wait_for("the answer", || {
        rig.sent(ANA).iter().any(|m| {
            matches!(m, PeerMessage::OfferAnswer { transfer_id, accept: true } if *transfer_id == ptid(1))
        })
    })
    .await;
    for chunk in chunk_frames(&ptid(1), &bytes) {
        rig.push(ANA, chunk);
    }
    wait_for("the review", || rig.events.reviews().len() == 1).await;
}

#[tokio::test]
async fn a_dropped_start_frees_the_peer() {
    // N3: a `start` future dropped while it builds the manifest leaves no claim behind.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let gate = rig.colls.hold_manifest();
    let dropped = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        rig.sync.start(&lid(L)),
    )
    .await;
    assert!(dropped.is_err(), "the start was still running");
    drop(gate);

    let sid = rig
        .sync
        .start(&lid(L))
        .await
        .expect("a new start is not busy");
    assert_eq!(rig.sync.phase_of(&sid), Some(Phase::SendManifest));
}

#[tokio::test]
async fn a_dropped_review_keeps_the_review_open() {
    // N3: a `review` future dropped while its answer is checked leaves the review open.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;
    let gate = std::sync::mpsc::channel::<()>();
    *rig.sync.review_gate.lock().expect("gate lock") = Some(gate.1);
    let dropped = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        rig.sync.review(&sid, true, vec![]),
    )
    .await;
    assert!(dropped.is_err(), "the review was still checking");
    drop(gate.0);

    rig.sync
        .review(&sid, true, vec![])
        .await
        .expect("answered again");
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
}

#[tokio::test]
async fn cancel_while_the_review_answer_is_checked_has_its_own_text() {
    // N4: the review answer is neither applied nor open while it is checked.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;
    let (release, gate) = std::sync::mpsc::channel::<()>();
    *rig.sync.review_gate.lock().expect("gate lock") = Some(gate);
    let sync = rig.sync.clone();
    let review_sid = sid.clone();
    let review = tokio::spawn(async move { sync.review(&review_sid, true, vec![]).await });
    wait_for("the check", || {
        rig.sync.review_gate.lock().expect("gate lock").is_none()
    })
    .await;

    let err = rig.sync.cancel(&sid).await.expect_err("refused");

    assert!(matches!(&err, DomainError::InvalidInput(m)
        if m == "The answer to the review is being checked. Try again in a moment."));
    drop(release);
    review.await.expect("joined").expect("accepted");
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
}

#[tokio::test]
async fn a_rename_against_an_edit_syncs_between_two_nodes() {
    // N4: Bo renamed a.yml while Ana edited it. Both diff texts are shown, and the sync ends
    // with the same files and the same base on both sides.
    let bo = with(&without(&base(), "a.yml"), "a2.yml", "url: a\n");
    let ana = with(&base(), "a.yml", "url: a-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;
    let texts = pair
        .b
        .sync
        .conflict_files(&sid, "a.yml")
        .expect("a conflict");
    assert_eq!(texts.mine.as_deref(), Some("url: a-ana\n"));
    assert_eq!(texts.theirs.as_deref(), Some("url: a\n"));

    let keep = Decision {
        path: "a.yml".into(),
        resolution: Resolution::KeepMine,
    };
    pair.b
        .sync
        .review(&sid, true, vec![keep])
        .await
        .expect("accepted");
    pair.run_until(|p| p.a.events.confirms().len() == 1 || !p.a.events.finished().is_empty())
        .await;
    if pair.a.events.confirms().len() == 1 {
        pair.a.sync.confirm(&sid, true).await.expect("confirmed");
    }
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;

    assert_eq!(pair.a.events.finished()[0].1, SyncFinish::Applied);
    assert_eq!(pair.b.events.finished()[0].1, SyncFinish::Applied);
    let a_files = pair.a.colls.files(ORDERS);
    assert_eq!(
        a_files,
        pair.b.colls.files(ORDERS),
        "the same files on both sides"
    );
    pair.run_until(|p| {
        p.a.links
            .record(&lid(L))
            .is_some_and(|r| r.base == manifest_of(&a_files))
            && p.b
                .links
                .record(&lid(L))
                .is_some_and(|r| r.base == manifest_of(&a_files))
    })
    .await;
}

#[tokio::test]
async fn a_result_that_overtakes_the_final_ack_of_the_files_is_taken() {
    // N4: the starter acks the last chunk of the files before it applies, so its `applied`
    // can come before that ack reaches the planner.
    let rig = sync_rig();
    let (sid, after) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let offer = nth_offer(&rig, ANA, SyncPayloadKind::Files, 0).await;
    rig.push(
        ANA,
        PeerMessage::OfferAnswer {
            transfer_id: offer.transfer_id.clone(),
            accept: true,
        },
    );
    wait_for("every chunk", || {
        rig.sent(ANA)
            .iter()
            .filter(|m| matches!(m, PeerMessage::Chunk { transfer_id, .. } if *transfer_id == offer.transfer_id))
            .count() as u32
            == offer.chunk_count
    })
    .await;

    rig.push(
        ANA,
        PeerMessage::SyncResult {
            sync_id: sid.as_str().into(),
            outcome: SyncOutcome::Applied,
        },
    );

    assert_eq!(wait_finished(&rig).await.0, SyncFinish::Applied);
    assert_eq!(rig.colls.files(ORDERS), after);
}

#[tokio::test]
async fn a_starter_manifest_without_the_collection_file_is_unsafe() {
    // A peer whose manifest lacks the root collection file would make the planner delete its
    // own. The final step refuses it.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "opencollection.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;

    rig.sync
        .review(&sid, true, vec![])
        .await
        .expect("the reason goes to Finished");

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(UNSAFE.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;
    assert!(
        rig.sent(ANA).iter().filter_map(as_offer).next().is_none(),
        "no final plan"
    );
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_zero_device_link_is_an_unknown_broken_link() {
    // R9: a quarantine stub has a zero device id. It shows as an unknown node, cannot start,
    // and no peer can match it.
    let rig = sync_rig();
    rig.colls.add(ORDERS, base());
    let mut stub = record(&lid(L), ANA, &base());
    stub.peer_device_id = rocket_network::DeviceId::from_bytes([0u8; 32]);
    stub.state = LinkState::Broken;
    rig.links.save(&stub, &[]).expect("saved");
    rig.peers.add(dev(ANA), "Ana");

    let view = rig.sync.links().expect("links");
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].display_name, "Unknown node");
    assert_eq!(view[0].state, LinkState::Broken);
    let err = rig.sync.start(&lid(L)).await.expect_err("refused");
    assert!(matches!(&err, DomainError::InvalidInput(m)
        if m == "This link is broken. Unlink and send the collection again."));
    manifest_offer_from(&rig, ANA, L, &manifest_of(&base()).base_hash(), 1);
    wait_for("link missing", || {
        rig.aborts(ANA) == vec![SyncAbortReason::LinkMissing]
    })
    .await;
}

#[tokio::test]
async fn unlinking_during_the_apply_is_refused() {
    // The apply's link save must not bring a deleted link back, so the unlink waits.
    let rig = sync_rig();
    let gate = rig.colls.hold_apply();
    let (sid, _) = starter_confirmed(&rig).await;
    wait_phase(&rig, &sid, Phase::Applying).await;

    let err = rig.sync.unlink(&lid(L)).await.expect_err("refused");

    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "The sync is already being applied.")
    );
    assert_eq!(rig.links.count(), 1);
    drop(gate);
}

#[tokio::test]
async fn a_refused_offer_frees_its_transfer_id() {
    // The planner holds the manifest id while it checks; a refusal gives it back.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    manifest_offer_from(&rig, ANA, L, &"e".repeat(64), 1);
    wait_for("the mismatch", || {
        rig.aborts(ANA) == vec![SyncAbortReason::BaseMismatch]
    })
    .await;

    manifest_offer_from(&rig, ANA, L, &bh(), 1);

    wait_for("the answer", || {
        rig.sent(ANA)
            .iter()
            .any(|m| matches!(m, PeerMessage::OfferAnswer { accept: true, .. }))
    })
    .await;
    assert_eq!(
        rig.aborts(ANA),
        vec![SyncAbortReason::BaseMismatch],
        "not busy"
    );
}

#[tokio::test]
async fn unlinking_during_a_start_is_refused() {
    // M1: `unlink` reads `starting` under the lock that would end the sync.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let gate = rig.colls.hold_manifest();
    let sync = rig.sync.clone();
    let start = tokio::spawn(async move { sync.start(&lid(L)).await });
    // The manifest call is parked on the gate, so `starting` holds the peer.
    wait_for("the start to claim the peer", || {
        rig.sync.is_starting(&dev(ANA))
    })
    .await;

    let err = rig.sync.unlink(&lid(L)).await.expect_err("refused");

    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == "The sync is already being applied.")
    );
    assert_eq!(rig.links.count(), 1);
    drop(gate);
    start.await.expect("joined").expect("started");
}

#[tokio::test]
async fn unlinking_during_the_check_ends_it_quietly() {
    // M3: a sync still in `Checking` was never shown, so it ends with the abort only.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let gate = rig.links.hold_get();
    let sid = SyncId::random();
    let bytes = manifest_of(&without(&base(), "a.yml"))
        .to_json()
        .expect("json");
    let offer = offered(
        sid.as_str(),
        &lid(L),
        &bh(),
        SyncPayloadKind::Manifest,
        &ptid(1),
        &bytes,
    );
    rig.push(ANA, offer_frame(&offer));
    wait_phase(&rig, &sid, Phase::Checking).await;

    rig.sync.unlink(&lid(L)).await.expect("unlinked");

    assert_eq!(rig.sync.phase_of(&sid), None);
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Cancelled]
    })
    .await;
    drop(gate);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        rig.events.finished().is_empty(),
        "no Finished for a sync never shown"
    );
    assert!(rig
        .audited()
        .iter()
        .all(|k| !matches!(k, AuditEventKind::NetworkSyncReceived { .. })));
    assert_eq!(rig.links.count(), 0);
}

#[tokio::test]
async fn a_final_step_that_panics_ends_the_sync() {
    // M4, M5: `finalize` runs owned, so a panic ends the session instead of leaving it in a
    // phase with no deadline.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;
    wait_phase(&rig, &sid, Phase::AwaitReview).await;
    rig.sync
        .panic_next_step
        .store(true, std::sync::atomic::Ordering::SeqCst);

    rig.sync.review(&sid, true, vec![]).await.expect("accepted");

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(INTERNAL.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;
    assert_unchanged(&rig, &base());
}

#[tokio::test]
async fn a_payload_step_that_panics_ends_the_sync() {
    // M5: the step after a payload's last chunk runs owned too.
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    rig.sync
        .panic_next_step
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);

    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Failed, Some(INTERNAL.to_string()))
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![SyncAbortReason::Failed]
    })
    .await;
    assert!(rig.events.confirms().is_empty());
}
