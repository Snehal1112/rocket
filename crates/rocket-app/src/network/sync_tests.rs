//! Task 1 tests: the router, links, unlink, removal with the pairing, and the link at the
//! first linked send.

use std::io::{Cursor, Write};
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rocket_audit::publisher::RecordingSecurityAuditPublisher;
use rocket_audit::AuditEventKind;
use rocket_network::{
    LinkId, LinkState, PeerMessage, ResultOutcome, SyncAbortReason, SyncId, SyncPayloadKind,
    TransferEvent, TransferSink,
};
use rocket_shared::error::DomainError;

use super::sync::{SyncDeps, SyncService};
use super::sync_fakes::*;
use super::sync_ports::base_from_reader;
use super::transfer_fakes::FakePeers;
use rocket_network::LinkRepository;

fn base() -> Files {
    files(&[
        ("opencollection.yml", "name: orders\n"),
        ("a.yml", "url: a\n"),
    ])
}

/// A ZIP laid out as the Bruno export lays it out: one root folder.
fn rooted_zip(root: &str, files: &Files) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        for (path, bytes) in files {
            zip.start_file(format!("{root}/{path}"), opts)
                .expect("entry");
            zip.write_all(bytes).expect("bytes");
        }
        zip.finish().expect("zip");
    }
    out.into_inner()
}

fn collection_offer(id: &str, bytes: &[u8], link: Option<&LinkId>) -> PeerMessage {
    PeerMessage::CollectionOffer {
        transfer_id: id.to_string(),
        name: ORDERS.to_string(),
        size_bytes: bytes.len() as u64,
        chunk_count: rocket_network::chunk_count_for(bytes.len() as u64),
        sha256: sha(bytes),
        keep_in_sync: link.is_some(),
        link_id: link.map(|l| l.as_str().to_string()),
    }
}

// ----- the router -----

#[tokio::test]
async fn the_router_sends_sync_frames_to_sync_and_others_to_transfer() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let zip = rooted_zip(ORDERS, &base());

    rig.push(ANA, collection_offer(&ptid(1), &zip, None));
    rig.push(
        ANA,
        PeerMessage::SyncAbort {
            sync_id: SyncId::random().as_str().to_string(),
            reason: SyncAbortReason::Cancelled,
        },
    );
    wait_for("the incoming offer", || {
        rig.transfer_events
            .all()
            .iter()
            .any(|e| matches!(e, TransferEvent::OfferIncoming { .. }))
    })
    .await;
    wait_for("the sync frame", || rig.sync.frames_seen() == 1).await;
}

#[tokio::test]
async fn frames_of_a_sync_transfer_id_go_to_sync_only_from_its_peer() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    rig.peers.add(dev(BEN), "Ben");
    let id = rig.sync.claim_transfer_for_test(dev(ANA));
    let ack = PeerMessage::ChunkAck {
        transfer_id: id.clone(),
        next_seq: 1,
    };

    assert!(rig.sync.claims(&dev(ANA), &ack), "its own peer");
    assert!(!rig.sync.claims(&dev(BEN), &ack), "another peer");
    assert!(
        !rig.sync
            .claims(&dev(ANA), &collection_offer(&id, b"x", None)),
        "a collection offer is never a sync frame"
    );
    assert!(rig.sync.claims(
        &dev(BEN),
        &PeerMessage::SyncResult {
            sync_id: SyncId::random().as_str().to_string(),
            outcome: rocket_network::SyncOutcome::Applied,
        }
    ));
}

#[tokio::test]
async fn a_sync_offer_reusing_a_transfer_id_is_dropped() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());
    let zip = rooted_zip(ORDERS, &base());
    rig.push(ANA, collection_offer(&ptid(1), &zip, None));
    wait_for("the incoming offer", || {
        !rig.transfer_events.all().is_empty()
    })
    .await;
    let manifest = manifest_of(&base()).to_json().expect("json");
    let offer = offered(
        SyncId::random().as_str(),
        &lid(1),
        &manifest_of(&base()).base_hash(),
        SyncPayloadKind::Manifest,
        &ptid(1),
        &manifest,
    );

    rig.push(ANA, offer_frame(&offer));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert_eq!(rig.sync.frames_seen(), 0, "the router dropped it");
    assert!(rig.sent(ANA).is_empty(), "no answer of any kind");
}

#[tokio::test]
async fn peer_gone_reaches_both_services() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let zip = rooted_zip(ORDERS, &base());
    rig.push(ANA, collection_offer(&ptid(1), &zip, None));
    wait_for("the incoming offer", || {
        !rig.transfer_events.all().is_empty()
    })
    .await;

    rig.router.on_peer_gone(dev(ANA));

    wait_for("the pending offer ended", || {
        rig.transfer_events
            .all()
            .iter()
            .any(|e| matches!(e, TransferEvent::Finished { .. }))
    })
    .await;
    wait_for("the sync service saw it", || rig.sync.gone_seen() == 1).await;
}

#[tokio::test]
async fn sync_service_new_does_not_take_the_sink() {
    let peers = Arc::new(FakePeers::default());
    let _sync = SyncService::new(SyncDeps {
        network: peers.clone(),
        links: Arc::new(FakeLinks::default()),
        collections: Arc::new(FakeCollections::default()),
        events: Arc::new(RecordingSyncEvents::default()),
        audit: Arc::new(RecordingSecurityAuditPublisher::new()),
    });
    assert!(
        peers.sink().is_none(),
        "only the router registers as the sink"
    );

    let rig = sync_rig();
    let sink = rig.peers.sink().expect("a sink");
    let sink_ptr = Arc::as_ptr(&sink) as *const ();
    let router_ptr = Arc::as_ptr(&rig.router) as *const ();
    assert_eq!(sink_ptr, router_ptr, "the router stays the sink");
}

// ----- links -----

#[tokio::test]
async fn create_link_stores_the_record_and_announces_it() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let files: Vec<(String, Vec<u8>)> = base().into_iter().collect();

    rig.sync
        .create_link(record(&lid(1), ANA, &base()), files.clone())
        .expect("created");

    assert_eq!(
        rig.links.record(&lid(1)),
        Some(record(&lid(1), ANA, &base()))
    );
    assert_eq!(rig.links.base_files(&lid(1)), files);
    assert_eq!(rig.events.link_changes(), 1);
}

#[tokio::test]
async fn create_link_refuses_an_existing_link_id() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());

    let err = rig
        .sync
        .create_link(record(&lid(1), ANA, &base()), Vec::new())
        .expect_err("exists");

    assert!(matches!(&err, DomainError::AlreadyExists(m) if m == "That link already exists."));
    assert_eq!(rig.links.saves(), 1, "only the first save");
    assert_eq!(rig.events.link_changes(), 0);
}

#[tokio::test]
async fn links_show_the_peer_label_and_derived_state() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());

    let views = rig.sync.links().expect("links");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].link_id, lid(1).as_str());
    assert_eq!(views[0].device_id, dev(ANA).to_base32());
    assert_eq!(views[0].display_name, "Ana");
    assert_eq!(views[0].short_id, dev(ANA).short());
    assert_eq!(views[0].collection_name, ORDERS);
    assert_eq!(views[0].state, LinkState::InSync);
    assert_eq!(views[0].updated_at, "2026-11-08T10:00:00+00:00");

    rig.colls.put(ORDERS, "a.yml", b"url: changed\n");
    assert_eq!(
        rig.sync.links().expect("links")[0].state,
        LinkState::ChangesWaiting
    );

    rig.colls.set_uid(ORDERS, Some("another"));
    assert_eq!(rig.sync.links().expect("links")[0].state, LinkState::Broken);
    rig.colls.set_uid(ORDERS, Some(UID));
    rig.colls.set_root(ORDERS, "/elsewhere/orders");
    assert_eq!(rig.sync.links().expect("links")[0].state, LinkState::Broken);
    rig.colls.remove(ORDERS);
    assert_eq!(rig.sync.links().expect("links")[0].state, LinkState::Broken);
    assert_eq!(
        rig.links.record(&lid(1)).expect("kept").state,
        LinkState::InSync,
        "links() derives, it does not save"
    );

    rig.peers.forget(&dev(ANA));
    let view = &rig.sync.links().expect("links")[0];
    assert_eq!(view.display_name, "Unknown node");
    assert_eq!(view.short_id, dev(ANA).short());
}

#[tokio::test]
async fn links_keep_a_stored_out_of_date_state_and_sort_by_collection() {
    let rig = sync_rig();
    rig.linked(&lid(2), &base(), &base());
    let mut other = record(&lid(1), ANA, &base());
    other.collection_name = "zeta".to_string();
    other.collection_root = "/ws/collections/zeta".to_string();
    other.state = LinkState::OutOfDate;
    rig.colls.add("zeta", base());
    rig.links.save(&other, &[]).expect("saved");

    let views = rig.sync.links().expect("links");
    let names: Vec<&str> = views.iter().map(|v| v.collection_name.as_str()).collect();
    assert_eq!(names, vec![ORDERS, "zeta"]);
    assert_eq!(
        views[1].state,
        LinkState::OutOfDate,
        "not turned into in_sync"
    );
}

#[tokio::test]
async fn unlink_deletes_the_link_and_audits() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());

    rig.sync.unlink(&lid(1)).await.expect("unlinked");

    assert_eq!(rig.links.count(), 0);
    assert_eq!(rig.events.link_changes(), 1);
    assert_eq!(
        rig.audited(),
        vec![AuditEventKind::NetworkLinkRemoved {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
        }]
    );
    assert!(rig.sent(ANA).is_empty(), "the peer learns at its next sync");
    assert_eq!(
        rig.colls.files(ORDERS),
        base(),
        "the collection is untouched"
    );
}

#[tokio::test]
async fn unlinking_an_unknown_link_is_not_found() {
    let rig = sync_rig();
    let err = rig.sync.unlink(&lid(9)).await.expect_err("unknown");
    assert!(matches!(&err, DomainError::NotFound(m) if m == "That link does not exist."));
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn removing_the_pairing_deletes_the_peers_links() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());
    rig.colls.add("other", base());
    let mut bens = record(&lid(2), BEN, &base());
    bens.collection_name = "other".to_string();
    rig.links.save(&bens, &[]).expect("saved");
    rig.peers.forget(&dev(ANA));

    rig.router.on_peer_gone(dev(ANA));

    wait_for("the link is gone", || rig.links.count() == 1).await;
    assert!(
        rig.links.record(&lid(2)).is_some(),
        "another peer's link stays"
    );
    wait_for("the audit row", || {
        rig.audited().contains(&AuditEventKind::NetworkLinkRemoved {
            device_id: dev(ANA).short(),
            collection: ORDERS.to_string(),
        })
    })
    .await;
    wait_for("the link change", || rig.events.link_changes() == 1).await;
}

#[tokio::test]
async fn a_dropped_connection_keeps_the_links() {
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());

    rig.router.on_peer_gone(dev(ANA));
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    assert_eq!(rig.links.count(), 1);
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn a_pairing_back_within_the_recheck_keeps_the_links() {
    // One failed label read is not enough: the second read, 50 ms later, sees the pairing.
    let rig = sync_rig();
    rig.linked(&lid(1), &base(), &base());
    rig.peers.forget(&dev(ANA));

    rig.router.on_peer_gone(dev(ANA));
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    rig.peers.add(dev(ANA), "Ana");
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    assert_eq!(rig.links.count(), 1);
    assert!(rig.audited().is_empty());
}

// ----- the first linked send -----

/// Collects the chunks of the one collection transfer this node sent to Ana.
fn sent_zip(rig: &SyncRig) -> (String, Vec<u8>, u32) {
    let mut id = String::new();
    let mut bytes = Vec::new();
    let mut count = 0;
    for m in rig.sent(ANA) {
        match m {
            PeerMessage::CollectionOffer { transfer_id, .. } => id = transfer_id,
            PeerMessage::Chunk { data, .. } => {
                bytes.extend(STANDARD.decode(data).expect("base64"));
                count += 1;
            }
            _ => {}
        }
    }
    (id, bytes, count)
}

async fn linked_send_until_last_ack(rig: &SyncRig) -> (String, Vec<u8>) {
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());
    let started = rig
        .transfer
        .send(&dev(ANA), ORDERS, false, true)
        .await
        .expect("offered");
    rig.push(
        ANA,
        PeerMessage::OfferAnswer {
            transfer_id: started.transfer_id.clone(),
            accept: true,
        },
    );
    let mut sent = (String::new(), Vec::new(), 0);
    wait_for("every chunk", || {
        sent = sent_zip(rig);
        sent.2 == rocket_network::chunk_count_for(sent.1.len() as u64) && sent.2 > 0
    })
    .await;
    rig.push(
        ANA,
        PeerMessage::ChunkAck {
            transfer_id: started.transfer_id.clone(),
            next_seq: sent.2,
        },
    );
    (started.transfer_id, sent.1)
}

#[tokio::test]
async fn a_keep_in_sync_send_puts_the_link_on_the_offer() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());

    rig.transfer
        .send(&dev(ANA), ORDERS, false, true)
        .await
        .expect("offered");

    let offer = rig
        .sent(ANA)
        .into_iter()
        .find(|m| matches!(m, PeerMessage::CollectionOffer { .. }))
        .expect("an offer");
    match offer {
        PeerMessage::CollectionOffer {
            keep_in_sync,
            link_id,
            ..
        } => {
            assert!(keep_in_sync);
            LinkId::parse(&link_id.expect("a link id")).expect("a valid link id");
        }
        _ => unreachable!("filtered above"),
    }
    assert_eq!(rig.links.count(), 0, "no link before the result");
}

#[tokio::test]
async fn a_plain_send_puts_no_link_on_the_offer() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    rig.transfer
        .send(&dev(ANA), ORDERS, false, false)
        .await
        .expect("offered");
    let found = rig.sent(ANA).into_iter().any(|m| {
        matches!(
            m,
            PeerMessage::CollectionOffer {
                keep_in_sync: false,
                link_id: None,
                ..
            }
        )
    });
    assert!(found);
}

#[tokio::test]
async fn a_completed_keep_in_sync_send_creates_the_sender_link() {
    let rig = sync_rig();
    let (id, zip) = linked_send_until_last_ack(&rig).await;

    rig.push(
        ANA,
        PeerMessage::TransferResult {
            transfer_id: id,
            outcome: ResultOutcome::Completed,
        },
    );

    wait_for("the sender link", || rig.links.count() == 1).await;
    let expected = base_from_reader(Cursor::new(&zip)).expect("a base");
    let link = rig.links.list_for_test().remove(0);
    assert_eq!(link.base, expected.manifest);
    assert_eq!(link.peer_device_id, dev(ANA));
    assert_eq!(link.collection_name, ORDERS);
    assert_eq!(link.collection_root, ROOT);
    assert_eq!(link.collection_uid.as_deref(), Some(UID));
    assert_eq!(link.state, LinkState::InSync);
    let mut expected_files = expected.files;
    expected_files.sort();
    assert_eq!(rig.links.base_files(&link.link_id), expected_files);
    assert_eq!(rig.events.link_changes(), 1);
}

#[tokio::test]
async fn a_damaged_result_creates_no_link() {
    let rig = sync_rig();
    let (id, _) = linked_send_until_last_ack(&rig).await;

    rig.push(
        ANA,
        PeerMessage::TransferResult {
            transfer_id: id,
            outcome: ResultOutcome::Damaged,
        },
    );

    wait_for("the send ended", || {
        rig.transfer_events
            .all()
            .iter()
            .any(|e| matches!(e, TransferEvent::Finished { .. }))
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(rig.links.count(), 0);
}

#[tokio::test]
async fn an_unconfirmed_send_creates_no_link() {
    let rig = sync_rig();
    let _ = linked_send_until_last_ack(&rig).await;

    rig.router.on_peer_gone(dev(ANA));

    wait_for("the send ended", || {
        rig.transfer_events
            .all()
            .iter()
            .any(|e| matches!(e, TransferEvent::Finished { .. }))
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(rig.links.count(), 0);
}

#[tokio::test]
async fn keep_in_sync_with_environments_is_refused() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");

    let err = rig
        .transfer
        .send(&dev(ANA), ORDERS, true, true)
        .await
        .expect_err("refused");

    assert!(matches!(&err, DomainError::InvalidInput(m)
        if m == "Keep in sync does not send environments. Turn one of them off."));
    assert!(rig.sent(ANA).is_empty());
}

#[tokio::test]
async fn keep_in_sync_without_a_sync_service_is_refused() {
    let rig = super::transfer_fakes::rig();
    rig.peers.add(dev(ANA), "Ana");

    let err = rig
        .svc
        .send(&dev(ANA), "orders", false, true)
        .await
        .expect_err("refused");

    assert!(matches!(&err, DomainError::InvalidInput(m) if m == "Keep in sync is not available."));
    assert_eq!(rig.exports(), 0, "refused before any export");
    assert!(rig.peers.sent().is_empty());
}

async fn receive_linked(rig: &SyncRig, link: Option<&LinkId>) {
    rig.peers.add(dev(ANA), "Ana");
    rig.colls.add(ORDERS, base());
    let zip = rooted_zip(ORDERS, &base());
    rig.push(ANA, collection_offer(&ptid(1), &zip, link));
    wait_for("the incoming offer", || {
        !rig.transfer_events.all().is_empty()
    })
    .await;
    rig.transfer
        .respond(&ptid(1), true)
        .await
        .expect("accepted");
    for chunk in chunk_frames(&ptid(1), &zip) {
        rig.push(ANA, chunk);
    }
}

#[tokio::test]
async fn a_keep_in_sync_receive_creates_the_receiver_link_after_import() {
    let rig = sync_rig();
    receive_linked(&rig, Some(&lid(4))).await;

    wait_for("the receiver link", || rig.links.count() == 1).await;
    let link = rig.links.record(&lid(4)).expect("the offered link id");
    assert_eq!(
        link.base,
        manifest_of(&base()),
        "the root folder is stripped"
    );
    assert_eq!(link.peer_device_id, dev(ANA));
    assert_eq!(link.collection_name, ORDERS, "the imported name");
    assert_eq!(link.collection_root, ROOT);
    assert_eq!(link.state, LinkState::InSync);
    assert_eq!(
        rig.links.base_files(&lid(4)),
        base().into_iter().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_plain_receive_creates_no_link() {
    let rig = sync_rig();
    receive_linked(&rig, None).await;
    wait_for("the import", || !rig.importer.calls().is_empty()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(rig.links.count(), 0);
}

#[tokio::test]
async fn a_failed_import_creates_no_link() {
    let rig = sync_rig();
    rig.importer
        .set_mode(super::transfer_fakes::ImportMode::Fail);
    receive_linked(&rig, Some(&lid(4))).await;
    wait_for("the import", || !rig.importer.calls().is_empty()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(rig.links.count(), 0);
}

#[tokio::test]
async fn a_keep_in_sync_offer_with_a_bad_link_id_is_refused() {
    // A bad id, keep_in_sync without an id, and an id without keep_in_sync.
    let bad: [(bool, Option<String>); 3] = [
        (true, Some("not-hex".into())),
        (true, None),
        (false, Some(lid(4).as_str().to_string())),
    ];
    for (keep, id) in bad {
        let rig = sync_rig();
        rig.peers.add(dev(ANA), "Ana");
        let zip = rooted_zip(ORDERS, &base());
        let mut offer = collection_offer(&ptid(1), &zip, None);
        if let PeerMessage::CollectionOffer {
            keep_in_sync,
            link_id,
            ..
        } = &mut offer
        {
            *keep_in_sync = keep;
            *link_id = id.clone();
        }

        rig.push(ANA, offer);

        wait_for("the refusal", || {
            rig.sent(ANA).iter().any(|m| {
                matches!(
                    m,
                    PeerMessage::TransferAbort {
                        reason: rocket_network::AbortReason::Failed,
                        ..
                    }
                )
            })
        })
        .await;
        assert!(
            rig.transfer_events.all().is_empty(),
            "never shown ({keep}, {id:?})"
        );
        assert!(!rig
            .transfer
            .has_transfer(&rocket_network::TransferId::parse(&ptid(1)).expect("an id")));
    }
}

#[tokio::test]
async fn both_ends_of_a_linked_send_store_the_same_base_hash() {
    let mut pair = SyncPair::new();
    pair.a.colls.add(ORDERS, base());
    pair.b.colls.add(ORDERS, base());
    let started = pair
        .a
        .transfer
        .send(&dev(ANA), ORDERS, false, true)
        .await
        .expect("offered");
    pair.run_until(|p| !p.b.transfer_events.all().is_empty())
        .await;
    pair.b
        .transfer
        .respond(&started.transfer_id, true)
        .await
        .expect("accepted");

    pair.run_until(|p| p.a.links.count() == 1 && p.b.links.count() == 1)
        .await;

    let a = pair.a.links.list_for_test().remove(0);
    let b = pair.b.links.list_for_test().remove(0);
    assert_eq!(a.link_id, b.link_id);
    assert_eq!(a.base_hash(), b.base_hash());
    assert_eq!(a.peer_device_id, dev(ANA));
    assert_eq!(b.peer_device_id, dev(BO));
}

#[tokio::test]
async fn the_first_send_base_leaves_out_what_the_sync_does_not_carry() {
    let rig = sync_rig();
    rig.colls.add(ORDERS, base());
    let mut sent = base();
    sent.insert("environments/dev.yml".into(), b"name: dev\n".to_vec());
    sent.insert("users/opencollection.yml".into(), b"name: x\n".to_vec());
    let zip = rooted_zip(ORDERS, &sent);

    rig.sync
        .link_first_transfer(lid(5), dev(ANA), ORDERS, &zip)
        .expect("linked");

    let link = rig.links.record(&lid(5)).expect("a link");
    assert_eq!(link.base, manifest_of(&base()));
}

#[tokio::test]
async fn a_first_send_zip_that_fails_the_strict_validator_is_not_linked() {
    let rig = sync_rig();
    rig.colls.add(ORDERS, base());
    // The last two pass the plain ZIP reader, so only the strict validator refuses them.
    let mut trailing = rooted_zip(ORDERS, &base());
    trailing.push(0);
    for zip in [
        rooted_zip("../x", &base()),
        b"not a zip".to_vec(),
        trailing,
        rooted_zip("CON", &base()),
    ] {
        let err = rig
            .sync
            .link_first_transfer(lid(5), dev(ANA), ORDERS, &zip)
            .expect_err("refused");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m == "The sent collection cannot be linked.")
        );
    }
    assert_eq!(rig.links.count(), 0);
    assert_eq!(rig.events.link_changes(), 0);
}

#[test]
fn pack_and_unpack_round_trip_in_path_order() {
    let files = base();
    let zip = zip_of(&files);
    assert_eq!(
        super::sync_payload::unpack_zip(&zip).ok(),
        Some(files.clone())
    );
    assert_eq!(zip_of(&files), zip, "the same files give the same bytes");
}
