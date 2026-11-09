//! Task 2 tests: one sync run row by row (the scripted peer plays the other node), then over
//! a relay between two real nodes.

use rocket_audit::AuditEventKind;
use rocket_network::{
    ConflictKind, Decision, LinkState, Manifest, PeerMessage, PlanFile, PlanRename, Resolution,
    SyncEvent, SyncFinish, SyncOutcome, SyncPayloadKind, SyncPlanPayload, SyncRole,
};
use rocket_network::{LinkRepository, TransferSink};
use rocket_shared::error::DomainError;

use super::sync::{ConflictFiles, Phase};
use super::sync_checks::{ALREADY_IN_SYNC, FOLDER_CHANGED, RESULT_NOT_SENT, UNCONFIRMED};
use super::sync_fakes::*;
use super::sync_payload::unpack_zip;
use super::sync_ports::FileChange;

fn put(path: &str, text: &str) -> FileChange {
    FileChange::Put {
        path: path.to_string(),
        bytes: text.as_bytes().to_vec(),
    }
}

// ----- starter, row by row -----

#[tokio::test]
async fn start_offers_the_manifest_with_the_base_hash() {
    let rig = sync_rig();
    let mine = with(&base(), "a.yml", "url: a2\n");
    rig.linked(&lid(L), &base(), &mine);

    let (sid, offer, bytes) = bo_starts(&rig).await;

    assert_eq!(offer.sync_id, sid.as_str());
    assert_eq!(offer.link_id, lid(L).as_str());
    assert_eq!(offer.base_hash, bh());
    assert_eq!(
        Manifest::from_json(&bytes).expect("a manifest"),
        manifest_of(&mine)
    );
    wait_phase(&rig, &sid, Phase::AwaitPlan).await;
    assert!(rig.events.all().is_empty(), "no dialog on the starter yet");
    assert!(rig.colls.applied().is_empty());
}

async fn assert_refused(rig: &SyncRig, text: &str) {
    let err = rig.sync.start(&lid(L)).await.expect_err("refused");
    assert!(
        matches!(&err, DomainError::InvalidInput(m) if m == text),
        "{err:?} is not {text}"
    );
}

#[tokio::test]
async fn start_refuses_with_the_reason_and_sends_nothing() {
    let rig = sync_rig();
    assert_refused(&rig, "That link does not exist.").await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let mut broken = rig.links.record(&lid(L)).expect("link");
    broken.state = LinkState::Broken;
    rig.links.save(&broken, &[]).expect("saved");
    assert_refused(
        &rig,
        "This link is broken. Unlink and send the collection again.",
    )
    .await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.peers.forget(&dev(ANA));
    assert_refused(&rig, "That node is not paired.").await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.peers.set_online(&dev(ANA), false);
    assert_refused(&rig, "That node is offline.").await;

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.colls.set_uid(ORDERS, Some("another"));
    assert_refused(&rig, FOLDER_CHANGED).await;
    wait_link(&rig, |r| r.state == LinkState::Broken).await;
    assert!(rig.sent(ANA).is_empty(), "no refused start sends a frame");

    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    rig.sync.start(&lid(L)).await.expect("the first");
    assert_refused(&rig, "A sync with that node is already running.").await;
    let offers = rig.sent(ANA).iter().filter_map(as_offer).count();
    assert_eq!(offers, 1, "only the first start offered");
    assert!(rig.audited().is_empty(), "a refused start writes no audit");
}

#[tokio::test]
async fn a_request_plan_is_answered_with_the_wanted_files() {
    let rig = sync_rig();
    let mine = with(&base(), "a.yml", "url: a2\n");
    rig.linked(&lid(L), &base(), &mine);
    let (sid, ..) = bo_starts(&rig).await;

    ana_sends_plan(&rig, sid.as_str(), 2, &request(&["a.yml"])).await;
    let (offer, zip) = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;

    assert_eq!(offer.sync_id, sid.as_str());
    assert_eq!(offer.base_hash, bh());
    assert_eq!(
        unpack_zip(&zip).expect("a valid zip"),
        files(&[("a.yml", "url: a2\n")])
    );
    wait_phase(&rig, &sid, Phase::AwaitPlan).await;
    assert!(rig
        .events
        .all()
        .iter()
        .any(|e| matches!(e, SyncEvent::Progress { sync_id, .. } if sync_id == sid.as_str())));
    assert!(rig.colls.applied().is_empty());
}

#[tokio::test]
async fn a_final_plan_with_files_asks_for_confirmation() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let sends = [("b.yml", "url: b2\n"), ("n.yml", "url: n\n")];
    let (plan, _) = final_plan(&base(), &[], &[], &sends);

    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitFiles).await;
    assert!(rig.events.confirms().is_empty(), "not before the files");
    ana_sends_files(&rig, sid.as_str(), 3, &zip_of(&files(&sends))).await;

    wait_for("the confirm", || rig.events.confirms().len() == 1).await;
    match &rig.events.confirms()[0] {
        SyncEvent::ConfirmNeeded {
            sync_id,
            link_id,
            device_id,
            short_id,
            display_name,
            collection_name,
            summary,
        } => {
            assert_eq!(sync_id, sid.as_str());
            assert_eq!(link_id, lid(L).as_str());
            assert_eq!(device_id, &dev(ANA).to_base32());
            assert_eq!(short_id, &dev(ANA).short());
            assert_eq!(display_name, "Ana");
            assert_eq!(collection_name, ORDERS);
            assert_eq!(
                (
                    summary.added_here,
                    summary.changed_here,
                    summary.deleted_here
                ),
                (1, 1, 0)
            );
        }
        _ => unreachable!("filtered by confirms()"),
    }
    assert_eq!(rig.sync.phase_of(&sid), Some(Phase::AwaitConfirm));
    assert!(
        rig.colls.applied().is_empty(),
        "nothing is written before the confirm"
    );
}

#[tokio::test]
async fn a_final_plan_with_only_deletes_asks_for_confirmation() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);

    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;

    wait_for("the confirm", || rig.events.confirms().len() == 1).await;
    match &rig.events.confirms()[0] {
        SyncEvent::ConfirmNeeded { summary, .. } => assert_eq!(summary.deleted_here, 1),
        _ => unreachable!("filtered by confirms()"),
    }
    assert!(
        rig.sent(ANA)
            .iter()
            .filter_map(as_offer)
            .all(|o| o.kind == SyncPayloadKind::Manifest),
        "the starter sent no files"
    );
}

#[tokio::test]
async fn the_starter_applies_then_reports_and_stores_the_base_on_applied() {
    let rig = sync_rig();
    let (sid, after) = starter_confirmed(&rig).await;

    wait_for("our result", || {
        rig.results(ANA) == vec![SyncOutcome::Applied]
    })
    .await;
    let applied = rig.colls.applied();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].0, ORDERS);
    assert_eq!(
        applied[0].1,
        vec![put("b.yml", "url: b2\n"), put("n.yml", "url: n\n")]
    );
    assert_eq!(rig.sync.phase_of(&sid), Some(Phase::AwaitResult));
    assert_eq!(
        rig.links.record(&lid(L)).expect("link").base,
        manifest_of(&base()),
        "the base waits for the peer's result"
    );

    rig.push(
        ANA,
        PeerMessage::SyncResult {
            sync_id: sid.as_str().to_string(),
            outcome: SyncOutcome::Applied,
        },
    );

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Applied, None));
    let link = wait_link(&rig, |r| r.base == manifest_of(&after)).await;
    assert_eq!(link.state, LinkState::InSync);
    assert_eq!(
        rig.links.base_files(&lid(L)),
        after.clone().into_iter().collect::<Vec<_>>()
    );
    assert_eq!(rig.colls.files(ORDERS), after);
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkSyncSent {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
            outcome: "applied".to_string(),
        }]
    );
    wait_for("link changed", || rig.events.link_changes() == 1).await;
    assert_eq!(rig.sync.phase_of(&sid), None, "the session is gone");
}

#[tokio::test]
async fn a_peer_gone_after_our_result_is_unconfirmed_with_the_new_base() {
    let rig = sync_rig();
    let (_sid, after) = starter_confirmed(&rig).await;
    wait_for("our result", || {
        rig.results(ANA) == vec![SyncOutcome::Applied]
    })
    .await;

    rig.router.on_peer_gone(dev(ANA));

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Unconfirmed, Some(UNCONFIRMED.to_string()))
    );
    let link = wait_link(&rig, |r| r.base == manifest_of(&after)).await;
    assert_eq!(link.state, LinkState::OutOfDate);
    assert_eq!(rig.links.count(), 1, "still paired, so the link stays");
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkSyncSent {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
            outcome: "unconfirmed".to_string(),
        }]
    );
}

#[tokio::test]
async fn no_result_within_the_timeout_is_unconfirmed() {
    let rig = sync_rig();
    let (_sid, after) = starter_confirmed(&rig).await;

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Unconfirmed, Some(UNCONFIRMED.to_string()))
    );
    let link = wait_link(&rig, |r| r.base == manifest_of(&after)).await;
    assert_eq!(link.state, LinkState::OutOfDate);
}

#[tokio::test]
async fn a_result_that_cannot_be_sent_keeps_the_old_base() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let (sid, ..) = bo_starts(&rig).await;
    let (plan, _) = final_plan(&base(), &["b.yml"], &[], &[]);
    ana_sends_plan(&rig, sid.as_str(), 2, &plan).await;
    wait_phase(&rig, &sid, Phase::AwaitConfirm).await;
    rig.peers.set_online(&dev(ANA), false);

    rig.sync.confirm(&sid, true).await.expect("confirmed");

    assert_eq!(
        wait_finished(&rig).await,
        (SyncFinish::Unconfirmed, Some(RESULT_NOT_SENT.to_string()))
    );
    assert_eq!(rig.colls.applied().len(), 1, "this side applied");
    let link = wait_link(&rig, |r| r.state == LinkState::OutOfDate).await;
    assert_eq!(
        link.base,
        manifest_of(&base()),
        "the planner surely did not apply"
    );
}

// ----- planner, row by row -----

#[tokio::test]
async fn a_matching_manifest_is_planned_and_reviewed() {
    let rig = sync_rig();
    let mine = with(&base(), "b.yml", "url: b-bo\n");
    rig.linked(&lid(L), &base(), &mine);
    let theirs = with(&with(&base(), "a.yml", "url: a-ana\n"), "c.yml", "url: c\n");

    let sid = ana_starts(&rig, &theirs).await;
    let (offer, bytes) = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let req = SyncPlanPayload::from_json(&bytes).expect("a plan");
    assert_eq!(offer.sync_id, sid.as_str());
    assert_eq!(offer.base_hash, bh());
    assert_eq!(req.wanted, vec!["a.yml".to_string(), "c.yml".to_string()]);
    assert!(req.new_base_hash.is_none() && req.deletes.is_empty() && req.sends.is_empty());
    assert!(rig.events.reviews().is_empty(), "not before the files");

    let incoming = files(&[("a.yml", "url: a-ana\n"), ("c.yml", "url: c\n")]);
    ana_sends_files(&rig, sid.as_str(), 2, &zip_of(&incoming)).await;

    wait_for("the review", || rig.events.reviews().len() == 1).await;
    match &rig.events.reviews()[0] {
        SyncEvent::ReviewReady {
            sync_id,
            link_id,
            device_id,
            display_name,
            collection_name,
            summary,
            conflicts,
            role,
            ..
        } => {
            assert_eq!(sync_id, sid.as_str());
            assert_eq!(link_id, lid(L).as_str());
            assert_eq!(device_id, &dev(ANA).to_base32());
            assert_eq!(display_name, "Ana");
            assert_eq!(collection_name, ORDERS);
            assert_eq!(*role, SyncRole::Planner);
            assert!(conflicts.is_empty());
            assert_eq!(
                (
                    summary.added_here,
                    summary.changed_here,
                    summary.deleted_here
                ),
                (1, 1, 0)
            );
            assert_eq!(
                (
                    summary.sent_added,
                    summary.sent_changed,
                    summary.sent_deleted
                ),
                (0, 1, 0)
            );
        }
        _ => unreachable!("filtered by reviews()"),
    }
    assert!(rig.colls.applied().is_empty());

    rig.sync.review(&sid, true, vec![]).await.expect("accepted");
    let (_, fin) = take_payload(&rig, ANA, SyncPayloadKind::Plan, 1).await;
    let fin = SyncPlanPayload::from_json(&fin).expect("the final plan");
    assert_eq!(
        fin.sends,
        vec![PlanFile {
            path: "b.yml".into(),
            sha256: sha(b"url: b-bo\n"),
        }]
    );
    assert!(fin.wanted.is_empty() && fin.deletes.is_empty() && fin.renames.is_empty());
    let after = with(&with(&mine, "a.yml", "url: a-ana\n"), "c.yml", "url: c\n");
    assert_eq!(fin.new_base_hash, Some(manifest_of(&after).base_hash()));
    assert!(
        rig.colls.applied().is_empty(),
        "the planner waits for the starter"
    );
}

#[tokio::test]
async fn a_delete_only_plan_is_reviewed_without_a_request() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());

    let sid = ana_starts(&rig, &without(&base(), "a.yml")).await;

    wait_for("the review", || rig.events.reviews().len() == 1).await;
    match &rig.events.reviews()[0] {
        SyncEvent::ReviewReady { summary, .. } => assert_eq!(summary.deleted_here, 1),
        _ => unreachable!("filtered by reviews()"),
    }
    assert!(
        rig.sent(ANA).iter().filter_map(as_offer).next().is_none(),
        "nothing was requested"
    );
    assert_eq!(rig.sync.phase_of(&sid), Some(Phase::AwaitReview));
}

#[tokio::test]
async fn the_final_plan_lists_the_peer_changes_and_the_new_base_hash() {
    let rig = sync_rig();
    let (sid, after) = planner_waiting(&rig).await;

    let (_, fin) = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let fin = SyncPlanPayload::from_json(&fin).expect("the final plan");
    assert_eq!(
        fin.renames,
        vec![PlanRename {
            from: "b.yml".into(),
            to: "b2.yml".into(),
        }],
        "a rename by hash travels as a rename"
    );
    assert_eq!(
        fin.sends,
        vec![PlanFile {
            path: "n.yml".into(),
            sha256: sha(b"url: n\n"),
        }]
    );
    assert!(fin.deletes.is_empty());
    assert_eq!(fin.new_base_hash, Some(manifest_of(&after).base_hash()));

    let (_, zip) = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    assert_eq!(
        unpack_zip(&zip).expect("a zip"),
        files(&[("n.yml", "url: n\n")])
    );
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;
    assert!(rig.colls.applied().is_empty());
}

#[tokio::test]
async fn the_planner_applies_only_after_the_starter_applied() {
    let rig = sync_rig();
    let (sid, after) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;
    assert!(rig.colls.applied().is_empty());

    rig.push(
        ANA,
        PeerMessage::SyncResult {
            sync_id: sid.as_str().to_string(),
            outcome: SyncOutcome::Applied,
        },
    );

    wait_for("our result", || {
        rig.results(ANA) == vec![SyncOutcome::Applied]
    })
    .await;
    assert_eq!(
        rig.colls.applied()[0].1,
        vec![FileChange::Delete {
            path: "a.yml".into()
        }]
    );
    assert_eq!(wait_finished(&rig).await, (SyncFinish::Applied, None));
    let link = wait_link(&rig, |r| r.base == manifest_of(&after)).await;
    assert_eq!(link.state, LinkState::InSync);
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkSyncReceived {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
            outcome: "applied".to_string(),
        }]
    );
}

#[tokio::test]
async fn a_peer_gone_during_the_planner_apply_does_not_stop_it() {
    let rig = sync_rig();
    let (sid, after) = planner_waiting(&rig).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Plan, 0).await;
    let _ = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    wait_phase(&rig, &sid, Phase::AwaitStarter).await;
    let gate = rig.colls.hold_apply();
    rig.push(
        ANA,
        PeerMessage::SyncResult {
            sync_id: sid.as_str().to_string(),
            outcome: SyncOutcome::Applied,
        },
    );
    wait_phase(&rig, &sid, Phase::Applying).await;

    rig.router.on_peer_gone(dev(ANA));
    drop(gate);

    assert_eq!(wait_finished(&rig).await, (SyncFinish::Applied, None));
    assert_eq!(rig.colls.files(ORDERS), after);
    wait_link(&rig, |r| r.base == manifest_of(&after)).await;
}

// ----- two nodes -----

/// After the review: confirms on the starter when it asks (a plan that changes nothing
/// there skips the confirm), then runs until both sides finished.
async fn confirm_if_asked(pair: &mut SyncPair, sid: &rocket_network::SyncId) {
    pair.run_until(|p| p.a.events.confirms().len() == 1 || !p.a.events.finished().is_empty())
        .await;
    if pair.a.events.confirms().len() == 1 {
        assert!(
            pair.a.colls.applied().is_empty() && pair.b.colls.applied().is_empty(),
            "nothing is written before the confirm"
        );
        pair.a.sync.confirm(sid, true).await.expect("confirmed");
    }
    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;
}

/// Bo starts, Ana reviews with `decisions`, Bo confirms when asked; runs until both finished.
async fn full_sync(pair: &mut SyncPair, decisions: Vec<Decision>) -> rocket_network::SyncId {
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");
    pair.run_until(|p| p.b.events.reviews().len() == 1).await;
    assert!(pair.a.colls.applied().is_empty() && pair.b.colls.applied().is_empty());
    pair.b
        .sync
        .review(&sid, true, decisions)
        .await
        .expect("reviewed");
    confirm_if_asked(pair, &sid).await;
    sid
}

async fn both_bases(pair: &mut SyncPair, a_files: &Files, b_files: &Files) {
    let (ma, mb) = (manifest_of(a_files), manifest_of(b_files));
    pair.run_until(|p| {
        p.a.links.record(&lid(L)).is_some_and(|r| r.base == ma)
            && p.b.links.record(&lid(L)).is_some_and(|r| r.base == mb)
    })
    .await;
}

#[tokio::test]
async fn a_two_way_sync_merges_both_edits_and_updates_both_bases() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);

    let sid = full_sync(&mut pair, vec![]).await;

    let both = with(&bo, "b.yml", "url: b-ana\n");
    assert_eq!(pair.a.colls.files(ORDERS), both);
    assert_eq!(pair.b.colls.files(ORDERS), both);
    assert_eq!(
        pair.a.events.finished(),
        vec![(sid.as_str().to_string(), SyncFinish::Applied, None)]
    );
    assert_eq!(
        pair.b.events.finished(),
        vec![(sid.as_str().to_string(), SyncFinish::Applied, None)]
    );
    both_bases(&mut pair, &both, &both).await;
    let (a, b) = (
        pair.a.links.record(&lid(L)).expect("a"),
        pair.b.links.record(&lid(L)).expect("b"),
    );
    assert_eq!(a.base_hash(), b.base_hash());
    assert_eq!((a.state, b.state), (LinkState::InSync, LinkState::InSync));
    assert_eq!(
        pair.a.audited(),
        vec![AuditEventKind::NetworkSyncSent {
            device_id: dev(ANA).short(),
            collection: ORDERS.into(),
            outcome: "applied".into(),
        }]
    );
    assert_eq!(
        pair.b.audited(),
        vec![AuditEventKind::NetworkSyncReceived {
            device_id: dev(BO).short(),
            collection: ORDERS.into(),
            outcome: "applied".into(),
        }]
    );
}

#[tokio::test]
async fn the_stored_base_is_computed_locally_on_both_sides() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "b.yml", "url: b-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    full_sync(&mut pair, vec![]).await;
    let (a_files, b_files) = (pair.a.colls.files(ORDERS), pair.b.colls.files(ORDERS));

    both_bases(&mut pair, &a_files, &b_files).await;

    assert_eq!(
        pair.a.links.base_files(&lid(L)),
        a_files.into_iter().collect::<Vec<_>>(),
        "every base file is a byte copy of the agreed file"
    );
    assert_eq!(
        pair.b.links.base_files(&lid(L)),
        b_files.into_iter().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_conflict_resolves_with_each_choice() {
    let bo = with(&base(), "a.yml", "url: a-bo\n");
    let ana = with(&base(), "a.yml", "url: a-ana\n");
    let cases = [
        (Resolution::KeepMine, with(&base(), "a.yml", "url: a-ana\n")),
        (
            Resolution::KeepTheirs,
            with(&base(), "a.yml", "url: a-bo\n"),
        ),
        (
            Resolution::KeepBoth,
            with(
                &with(&base(), "a.yml", "url: a-ana\n"),
                "a (from peer).yml",
                "url: a-bo\n",
            ),
        ),
    ];
    for (resolution, expected) in cases {
        let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
        let sid = pair.a.sync.start(&lid(L)).await.expect("started");
        pair.run_until(|p| p.b.events.reviews().len() == 1).await;
        match &pair.b.events.reviews()[0] {
            SyncEvent::ReviewReady { conflicts, .. } => {
                assert_eq!(conflicts.len(), 1);
                assert_eq!(conflicts[0].path, "a.yml");
                assert_eq!(conflicts[0].kind, ConflictKind::BothChanged);
                assert_eq!(conflicts[0].mine_hash, Some(sha(b"url: a-ana\n")));
                assert_eq!(conflicts[0].theirs_hash, Some(sha(b"url: a-bo\n")));
            }
            _ => unreachable!("filtered by reviews()"),
        }
        assert_eq!(
            pair.b
                .sync
                .conflict_files(&sid, "a.yml")
                .expect("both texts"),
            ConflictFiles {
                mine: Some("url: a-ana\n".into()),
                theirs: Some("url: a-bo\n".into()),
            }
        );
        pair.b
            .sync
            .review(
                &sid,
                true,
                vec![Decision {
                    path: "a.yml".into(),
                    resolution,
                }],
            )
            .await
            .expect("decided");
        confirm_if_asked(&mut pair, &sid).await;

        assert_eq!(
            pair.a.colls.files(ORDERS),
            expected,
            "{resolution:?} on the starter"
        );
        assert_eq!(
            pair.b.colls.files(ORDERS),
            expected,
            "{resolution:?} on the planner"
        );
        both_bases(&mut pair, &expected, &expected).await;
    }
}

#[tokio::test]
async fn a_rename_travels_as_a_rename() {
    let mut bo = without(&base(), "b.yml");
    bo.insert("f/c.yml".into(), b"url: b\n".to_vec());
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());

    full_sync(&mut pair, vec![]).await;

    assert_eq!(
        pair.b.colls.applied()[0].1,
        vec![FileChange::Rename {
            from: "b.yml".into(),
            to: "f/c.yml".into()
        }],
        "not a delete plus an add"
    );
    assert_eq!(pair.b.colls.files(ORDERS), bo);
}

#[tokio::test]
async fn a_delete_of_an_untouched_file_is_taken() {
    let bo = without(&base(), "a.yml");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &base());

    full_sync(&mut pair, vec![]).await;

    assert_eq!(
        pair.b.colls.applied()[0].1,
        vec![FileChange::Delete {
            path: "a.yml".into()
        }]
    );
    assert_eq!(pair.b.colls.files(ORDERS), bo);
}

#[tokio::test]
async fn a_delete_against_an_edit_is_a_conflict() {
    let bo = without(&base(), "a.yml");
    let ana = with(&base(), "a.yml", "url: a-ana\n");
    let mut pair = SyncPair::linked(&lid(L), &base(), &bo, &ana);
    pair.a.sync.start(&lid(L)).await.expect("started");

    pair.run_until(|p| p.b.events.reviews().len() == 1).await;

    match &pair.b.events.reviews()[0] {
        SyncEvent::ReviewReady { conflicts, .. } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].kind, ConflictKind::DeleteVsEdit);
        }
        _ => unreachable!("filtered by reviews()"),
    }
}

#[tokio::test]
async fn an_empty_sync_ends_applied_without_dialogs() {
    let mut pair = SyncPair::linked(&lid(L), &base(), &base(), &base());
    let sid = pair.a.sync.start(&lid(L)).await.expect("started");

    pair.run_until(|p| !p.a.events.finished().is_empty() && !p.b.events.finished().is_empty())
        .await;

    assert_eq!(
        pair.a.events.finished(),
        vec![(sid.as_str().to_string(), SyncFinish::Applied, None)]
    );
    assert_eq!(
        pair.b.events.finished(),
        vec![(
            sid.as_str().to_string(),
            SyncFinish::Applied,
            Some(ALREADY_IN_SYNC.to_string())
        )]
    );
    assert!(pair.a.events.reviews().is_empty() && pair.b.events.reviews().is_empty());
    assert!(pair.a.events.confirms().is_empty());
    assert!(pair.a.colls.applied().is_empty() && pair.b.colls.applied().is_empty());
    both_bases(&mut pair, &base(), &base()).await;
}

// ----- extra rows: case clash, malformed ids, an early follow-up offer -----

#[tokio::test]
async fn names_that_differ_only_by_case_end_the_sync_with_the_fixed_text() {
    let rig = sync_rig();
    let mine = with(&base(), "Login.yml", "url: l\n");
    rig.linked(&lid(L), &base(), &mine);

    let sid = ana_starts(&rig, &with(&base(), "login.yml", "url: l\n")).await;

    assert_eq!(
        wait_finished(&rig).await,
        (
            SyncFinish::Failed,
            Some(super::sync_checks::CASE_CLASH.to_string())
        )
    );
    wait_for("the abort", || {
        rig.aborts(ANA) == vec![rocket_network::SyncAbortReason::Failed]
    })
    .await;
    assert_eq!(rig.sync.phase_of(&sid), None);
    assert!(rig.events.reviews().is_empty());
    assert!(rig.colls.applied().is_empty());
}

#[tokio::test]
async fn a_manifest_offer_with_a_malformed_id_is_ignored() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let bytes = manifest_of(&base()).to_json().expect("json");
    let good = offered(
        rocket_network::SyncId::random().as_str(),
        &lid(L),
        &bh(),
        SyncPayloadKind::Manifest,
        &ptid(1),
        &bytes,
    );
    let mut bad_sync = good.clone();
    bad_sync.sync_id = "not-hex".into();
    let mut bad_link = good.clone();
    bad_link.link_id = "LINK".into();
    let mut bad_transfer = good;
    bad_transfer.transfer_id = "../x".into();

    for offer in [bad_sync, bad_link, bad_transfer] {
        rig.push(ANA, offer_frame(&offer));
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(rig.sent(ANA).is_empty(), "no answer at all");
    assert!(rig.events.all().is_empty());
}

#[tokio::test]
async fn a_plan_offer_that_overtakes_the_final_manifest_ack_is_kept() {
    let rig = sync_rig();
    rig.linked(&lid(L), &base(), &base());
    let sid = rig.sync.start(&lid(L)).await.expect("started");
    let manifest = nth_offer(&rig, ANA, SyncPayloadKind::Manifest, 0).await;
    rig.push(
        ANA,
        PeerMessage::OfferAnswer {
            transfer_id: manifest.transfer_id.clone(),
            accept: true,
        },
    );
    wait_for("every manifest chunk", || {
        rig.sent(ANA)
            .iter()
            .filter(|m| matches!(m, PeerMessage::Chunk { .. }))
            .count() as u32
            == manifest.chunk_count
    })
    .await;

    // The plan comes before the final ack of the manifest.
    let plan = request(&["a.yml"]).to_json().expect("json");
    let offer = offered(
        sid.as_str(),
        &lid(L),
        &bh(),
        SyncPayloadKind::Plan,
        &ptid(2),
        &plan,
    );
    rig.push(ANA, offer_frame(&offer));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        rig.sync.phase_of(&sid),
        Some(Phase::SendManifest),
        "kept, not refused"
    );
    assert!(rig.events.finished().is_empty());

    rig.push(
        ANA,
        PeerMessage::ChunkAck {
            transfer_id: manifest.transfer_id,
            next_seq: manifest.chunk_count,
        },
    );
    wait_for("the answer to the kept plan", || {
        rig.sent(ANA).iter().any(|m| {
            matches!(m, PeerMessage::OfferAnswer { transfer_id, accept: true } if *transfer_id == ptid(2))
        })
    })
    .await;
    for chunk in chunk_frames(&ptid(2), &plan) {
        rig.push(ANA, chunk);
    }
    let (_, zip) = take_payload(&rig, ANA, SyncPayloadKind::Files, 0).await;
    assert_eq!(
        unpack_zip(&zip).expect("a zip"),
        files(&[("a.yml", "url: a\n")])
    );
}
