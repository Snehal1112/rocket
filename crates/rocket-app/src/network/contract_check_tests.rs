//! Tests of `ContractCheckService`: both sides, the limits and the hostile frames.

use std::time::{Duration, Instant};

use rocket_audit::{AuditActor, AuditEventKind};
use rocket_network::limits::{CONTRACT_CHECK_TTL, CONTRACT_VALUE_MAX};
use rocket_network::{
    ContractChange, ContractChangeType, ContractCheckEvent, ContractCheckId, ContractCheckOutcome,
    ContractCheckRow, ContractCheckRowDto, ContractPolicyWire, ContractVerdict, PeerMessage,
};
use rocket_shared::error::DomainError;

use super::contract_check::{NOT_WAITING, NO_CHANGES, TOO_LARGE};
use super::contract_check_fakes::{
    check_rig, header_change, pending, settle, user_request, CheckRig,
};
use super::sync_fakes::dev;
use super::transfer_fakes::wait_for;

const ANA: u8 = 1;
const BEN: u8 = 2;

fn two_changes() -> Vec<ContractChange> {
    vec![
        header_change(
            "X-Tenant",
            ContractChangeType::Added,
            None,
            Some("acme"),
            true,
        ),
        header_change(
            "Authorization",
            ContractChangeType::Changed,
            Some("Bearer old"),
            Some("Bearer new"),
            true,
        ),
    ]
}

fn offer(id: &ContractCheckId, changes: Vec<ContractChange>) -> PeerMessage {
    PeerMessage::ContractCheckOffer {
        check_id: id.as_str().to_string(),
        contract: "Orders API".into(),
        version: "1.2.0".into(),
        policy: ContractPolicyWire::Strict,
        changes,
    }
}

fn result(
    id: &str,
    outcome: ContractCheckOutcome,
    rows: Vec<(u32, ContractVerdict)>,
) -> PeerMessage {
    PeerMessage::ContractCheckResult {
        check_id: id.to_string(),
        outcome,
        rows: rows
            .into_iter()
            .map(|(index, verdict)| ContractCheckRow { index, verdict })
            .collect(),
    }
}

fn message_of(e: DomainError) -> String {
    match e {
        DomainError::InvalidInput(m)
        | DomainError::NotFound(m)
        | DomainError::Conflict(m)
        | DomainError::Internal(m) => m,
        other => format!("{other}"),
    }
}

/// The incoming offer events of the rig.
fn offers_received(rig: &CheckRig) -> Vec<String> {
    rig.events
        .all()
        .into_iter()
        .filter_map(|e| match e {
            ContractCheckEvent::OfferReceived { check_id, .. } => Some(check_id),
            _ => None,
        })
        .collect()
}

/// The provider started a check with ANA and BEN. Returns the check id per device.
async fn started(rig: &CheckRig) -> (String, String) {
    let out = rig
        .svc
        .start("/root", "contract-1", &[dev(ANA), dev(BEN)])
        .expect("started");
    let id_of = |seed: u8| {
        out.check_ids
            .iter()
            .find(|(d, _)| *d == dev(seed))
            .map(|(_, id)| id.as_str().to_string())
            .expect("id for device")
    };
    let ids = (id_of(ANA), id_of(BEN));
    wait_for("offers", || {
        rig.sent(ANA).len() == 1 && rig.sent(BEN).len() == 1
    })
    .await;
    ids
}

fn busy_sent(rig: &CheckRig, seed: u8, id: &ContractCheckId) -> bool {
    rig.sent(seed).iter().any(|m| {
        matches!(m, PeerMessage::ContractCheckResult { check_id, outcome: ContractCheckOutcome::Busy, rows }
            if check_id == id.as_str() && rows.is_empty())
    })
}

fn received_audits(rig: &CheckRig) -> Vec<String> {
    rig.audited()
        .into_iter()
        .filter_map(|k| match k {
            AuditEventKind::NetworkContractCheckReceived { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect()
}

fn sent_audits(rig: &CheckRig) -> Vec<(String, String, String)> {
    rig.audited()
        .into_iter()
        .filter_map(|k| match k {
            AuditEventKind::NetworkContractCheckSent {
                device_id,
                contract,
                outcome,
            } => Some((device_id, contract, outcome)),
            _ => None,
        })
        .collect()
}

fn results_received(rig: &CheckRig) -> Vec<(String, String, String, Vec<ContractCheckRowDto>)> {
    rig.events
        .all()
        .into_iter()
        .filter_map(|e| match e {
            ContractCheckEvent::ResultReceived {
                check_id,
                device_id,
                outcome,
                rows,
            } => Some((check_id, device_id, outcome, rows)),
            _ => None,
        })
        .collect()
}

// ----- provider: start -----

#[tokio::test]
async fn start_sends_one_offer_per_device_with_redacted_values() {
    let rig = check_rig(pending(two_changes()));
    let out = rig
        .svc
        .start("/root", "contract-1", &[dev(ANA), dev(BEN), dev(ANA)])
        .expect("started");
    assert_eq!(out.check_ids.len(), 2, "one id per distinct device");
    assert_ne!(out.check_ids[0].1, out.check_ids[1].1);
    assert_eq!(out.changes[1].old_value.as_deref(), Some("[hidden]"));
    assert_eq!(out.changes[1].new_value.as_deref(), Some("[hidden]"));
    assert_eq!(out.changes[0].new_value.as_deref(), Some("acme"));
    wait_for("offers", || {
        rig.sent(ANA).len() == 1 && rig.sent(BEN).len() == 1
    })
    .await;
    for (device, id) in &out.check_ids {
        let frames = rig.peers.sent_to(device);
        let PeerMessage::ContractCheckOffer {
            check_id,
            contract,
            version,
            changes,
            ..
        } = &frames[0]
        else {
            panic!("an offer");
        };
        assert_eq!(check_id, id.as_str());
        assert_eq!(contract, "Orders API");
        assert_eq!(version, "1.2.0");
        assert_eq!(changes, &out.changes);
        let text = frames[0].encode();
        assert!(!text.contains("Bearer"), "secrets never leave the node");
    }
    assert!(rig.audited().is_empty(), "start audits nothing");
}

#[tokio::test]
async fn start_refuses_too_large_offer_with_the_fixed_sentence() {
    let long = "v".repeat(CONTRACT_VALUE_MAX);
    let changes: Vec<ContractChange> = (0..200)
        .map(|i| {
            header_change(
                &format!("X-Long-{i}"),
                ContractChangeType::Changed,
                Some(&long),
                Some(&long),
                true,
            )
        })
        .collect();
    let rig = check_rig(pending(changes));
    let err = rig
        .svc
        .start("/root", "contract-1", &[dev(ANA)])
        .expect_err("too large");
    assert!(matches!(err, DomainError::InvalidInput(_)));
    assert_eq!(message_of(err), "Check fewer changes at a time.");
    assert_eq!(TOO_LARGE, "Check fewer changes at a time.");
    settle().await;
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn start_refuses_more_than_the_change_cap() {
    let changes: Vec<ContractChange> = (0..201)
        .map(|i| {
            header_change(
                &format!("X-{i}"),
                ContractChangeType::Added,
                None,
                None,
                true,
            )
        })
        .collect();
    let rig = check_rig(pending(changes));
    let err = rig
        .svc
        .start("/root", "contract-1", &[dev(ANA)])
        .expect_err("too many");
    assert_eq!(message_of(err), TOO_LARGE);
    settle().await;
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn start_refuses_when_there_are_no_changes() {
    let rig = check_rig(pending(Vec::new()));
    let err = rig
        .svc
        .start("/root", "contract-1", &[dev(ANA)])
        .expect_err("no changes");
    assert_eq!(message_of(err), NO_CHANGES);
    settle().await;
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn start_refuses_unpaired_offline_or_no_devices_before_reading() {
    let rig = check_rig(pending(two_changes()));
    rig.peers.forget(&dev(ANA));
    assert!(rig.svc.start("/root", "c", &[dev(ANA)]).is_err());
    rig.peers.set_online(&dev(BEN), false);
    assert!(rig.svc.start("/root", "c", &[dev(BEN)]).is_err());
    assert!(rig.svc.start("/root", "c", &[]).is_err());
    assert_eq!(rig.changes.calls(), 0);
    settle().await;
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn start_passes_a_port_error_and_sends_nothing() {
    let rig = check_rig(pending(two_changes()));
    rig.changes.set(None);
    assert!(rig.svc.start("/root", "c", &[dev(ANA)]).is_err());
    settle().await;
    assert!(rig.peers.sent().is_empty());
}

// ----- consumer -----

#[tokio::test]
async fn incoming_offer_emits_offer_received_and_waits_for_the_user() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    rig.push(ANA, offer(&id, two_changes()));
    let events = rig.events.all();
    assert_eq!(
        events,
        vec![ContractCheckEvent::OfferReceived {
            check_id: id.as_str().to_string(),
            device_id: dev(ANA).to_base32(),
            contract: "Orders API".into(),
            change_count: 2,
        }]
    );
    settle().await;
    assert!(
        rig.peers.sent().is_empty(),
        "nothing leaves without a click"
    );
    assert!(rig.audited().is_empty());
    assert_eq!(
        rig.locals.calls(),
        0,
        "the requests are read at review time"
    );
}

#[tokio::test]
async fn bad_offer_is_dropped_without_reply_or_audit() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    // No changes.
    rig.push(ANA, offer(&id, Vec::new()));
    // A control character in the field.
    let mut bad = two_changes();
    bad[0].field = "header:X\u{7}".into();
    rig.push(ANA, offer(&id, bad));
    // A bad check id.
    rig.push(
        ANA,
        PeerMessage::ContractCheckOffer {
            check_id: "nope".into(),
            contract: "Orders API".into(),
            version: "1".into(),
            policy: ContractPolicyWire::Strict,
            changes: two_changes(),
        },
    );
    // A contract name with a control character.
    rig.push(
        ANA,
        PeerMessage::ContractCheckOffer {
            check_id: id.as_str().to_string(),
            contract: "Orders\nAPI".into(),
            version: "1".into(),
            policy: ContractPolicyWire::Strict,
            changes: two_changes(),
        },
    );
    // A node that is not paired.
    rig.peers.forget(&dev(BEN));
    rig.push(BEN, offer(&ContractCheckId::random(), two_changes()));
    settle().await;
    assert!(rig.peers.sent().is_empty());
    assert!(rig.audited().is_empty());
    assert!(rig.events.all().is_empty());
    assert!(rig.svc.review(&id).is_err());
}

#[tokio::test]
async fn fourth_offer_gets_busy() {
    let rig = check_rig(pending(Vec::new()));
    for seed in 1..=3u8 {
        rig.push(seed, offer(&ContractCheckId::random(), two_changes()));
    }
    let fourth = ContractCheckId::random();
    rig.push(4, offer(&fourth, two_changes()));
    assert_eq!(offers_received(&rig).len(), 3, "the busy one is not shown");
    wait_for("busy", || busy_sent(&rig, 4, &fourth)).await;
    assert_eq!(rig.peers.sent().len(), 1);
    assert_eq!(received_audits(&rig), vec!["busy".to_string()]);
    assert!(rig.svc.review(&fourth).is_err());
}

#[tokio::test]
async fn second_offer_from_same_peer_gets_busy() {
    let rig = check_rig(pending(Vec::new()));
    let first = ContractCheckId::random();
    let second = ContractCheckId::random();
    rig.push(ANA, offer(&first, two_changes()));
    rig.push(ANA, offer(&second, two_changes()));
    assert_eq!(offers_received(&rig), vec![first.as_str().to_string()]);
    wait_for("busy", || busy_sent(&rig, ANA, &second)).await;
    assert_eq!(rig.sent(ANA).len(), 1);
    assert_eq!(received_audits(&rig), vec!["busy".to_string()]);
    // A replay of the pending offer's id is busy too, and the first check stays.
    rig.push(ANA, offer(&first, two_changes()));
    wait_for("busy again", || busy_sent(&rig, ANA, &first)).await;
    assert!(rig.svc.review(&first).is_ok());
    assert_eq!(offers_received(&rig).len(), 1, "no busy offer is shown");
    assert_eq!(received_audits(&rig).len(), 1, "one busy row per window");
}

#[tokio::test]
async fn busy_audit_is_written_once_per_peer_per_window() {
    let rig = check_rig(pending(Vec::new()));
    rig.push(ANA, offer(&ContractCheckId::random(), two_changes()));
    for _ in 0..5 {
        rig.push(ANA, offer(&ContractCheckId::random(), two_changes()));
    }
    wait_for("five busy frames", || rig.sent(ANA).len() == 5).await;
    assert_eq!(received_audits(&rig), vec!["busy".to_string()]);
    let actors: Vec<String> = rig.audit.records().into_iter().map(|r| r.actor).collect();
    assert_eq!(actors, vec![String::from(AuditActor::System)]);
    // Still inside the window, a sweep keeps the mark.
    rig.svc.sweep(Instant::now());
    rig.push(ANA, offer(&ContractCheckId::random(), two_changes()));
    wait_for("six busy frames", || rig.sent(ANA).len() == 6).await;
    assert_eq!(received_audits(&rig).len(), 1);
    // After the window another row is written. The pending check is still in time.
    rig.svc.advance(Duration::from_secs(61));
    rig.push(ANA, offer(&ContractCheckId::random(), two_changes()));
    wait_for("seven busy frames", || rig.sent(ANA).len() == 7).await;
    assert_eq!(received_audits(&rig).len(), 2);
}

#[tokio::test]
async fn decline_after_expiry_without_sweep_fails_and_emits_expired() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    rig.push(ANA, offer(&id, two_changes()));
    rig.svc.advance(CONTRACT_CHECK_TTL + Duration::from_secs(1));
    let err = rig.svc.decline(&id).expect_err("expired");
    assert_eq!(message_of(err), NOT_WAITING);
    let expired = ContractCheckEvent::Expired {
        check_id: id.as_str().to_string(),
    };
    assert_eq!(
        rig.events.all().iter().filter(|e| **e == expired).count(),
        1
    );
    // A later sweep does not expire it twice.
    rig.svc
        .sweep(Instant::now() + CONTRACT_CHECK_TTL + Duration::from_secs(2));
    assert_eq!(
        rig.events.all().iter().filter(|e| **e == expired).count(),
        1
    );
    settle().await;
    assert!(rig.peers.sent().is_empty());
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn review_lists_matches_and_verdicts_without_sending() {
    let rig = check_rig(pending(Vec::new()));
    rig.locals.set(vec![
        user_request("Get user", &[]),
        user_request("Get user as admin", &[("X-Tenant", "acme")]),
    ]);
    let id = ContractCheckId::random();
    let mut changes = two_changes();
    changes.push(ContractChange {
        method: "POST".into(),
        path_template: "/orders".into(),
        field: "header:X-Trace".into(),
        change_type: ContractChangeType::Added,
        old_value: None,
        new_value: Some("1".into()),
        breaking: true,
    });
    rig.push(ANA, offer(&id, changes.clone()));
    let review = rig.svc.review(&id).expect("review");
    assert_eq!(review.check_id, id.as_str());
    assert_eq!(review.device_id, dev(ANA).to_base32());
    assert_eq!(review.contract, "Orders API");
    assert_eq!(review.version, "1.2.0");
    assert_eq!(review.changes.len(), 3);
    assert_eq!(review.changes[0].change, changes[0]);
    // One of the two requests does not send the added breaking header.
    assert_eq!(review.changes[0].verdict, ContractVerdict::Break);
    let names: Vec<_> = review.changes[0]
        .matches
        .iter()
        .map(|m| (m.collection.as_str(), m.name.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![("Shop", "Get user"), ("Shop", "Get user as admin")]
    );
    assert_eq!(review.changes[2].verdict, ContractVerdict::NotUsed);
    assert!(review.changes[2].matches.is_empty());
    settle().await;
    assert!(rig.peers.sent().is_empty());
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn reply_sends_one_row_per_change_in_order_and_audits_once() {
    let rig = check_rig(pending(Vec::new()));
    rig.locals.set(vec![user_request("Get user", &[])]);
    let id = ContractCheckId::random();
    let mut changes = two_changes();
    changes.push(header_change(
        "X-Old",
        ContractChangeType::Removed,
        Some("1"),
        None,
        true,
    ));
    changes[1].path_template = "/other".into();
    rig.push(ANA, offer(&id, changes));
    let review = rig.svc.review(&id).expect("review");
    let verdicts: Vec<_> = review.changes.iter().map(|c| c.verdict).collect();
    rig.svc.reply(&id).expect("reply");
    wait_for("result", || rig.sent(ANA).len() == 1).await;
    let PeerMessage::ContractCheckResult {
        check_id,
        outcome,
        rows,
    } = &rig.sent(ANA)[0]
    else {
        panic!("a result");
    };
    assert_eq!(check_id, id.as_str());
    assert_eq!(*outcome, ContractCheckOutcome::Answered);
    let want: Vec<ContractCheckRow> = verdicts
        .iter()
        .enumerate()
        .map(|(i, v)| ContractCheckRow {
            index: i as u32,
            verdict: *v,
        })
        .collect();
    assert_eq!(rows, &want);
    assert_eq!(
        verdicts,
        vec![
            ContractVerdict::Break,
            ContractVerdict::NotUsed,
            ContractVerdict::Pass
        ]
    );
    let text = rig.sent(ANA)[0].encode();
    assert!(!text.contains("Get user") && !text.contains("Shop"));
    assert!(
        !text.contains("Orders"),
        "the result echoes no offer string"
    );
    let audits = rig.audited();
    assert_eq!(
        audits,
        vec![AuditEventKind::NetworkContractCheckReceived {
            device_id: dev(ANA).short(),
            contract: "Orders API".into(),
            outcome: "answered".into(),
        }]
    );
    let actors: Vec<String> = rig.audit.records().into_iter().map(|r| r.actor).collect();
    assert_eq!(actors, vec![String::from(AuditActor::User)]);
    // A second reply finds nothing to answer.
    let err = rig.svc.reply(&id).expect_err("answered once");
    assert_eq!(message_of(err), NOT_WAITING);
    settle().await;
    assert_eq!(rig.sent(ANA).len(), 1);
    assert_eq!(rig.audited().len(), 1);
}

#[tokio::test]
async fn reply_before_review_fails_and_sends_nothing() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    rig.push(ANA, offer(&id, two_changes()));
    assert!(rig.svc.reply(&id).is_err());
    settle().await;
    assert!(rig.peers.sent().is_empty());
    assert!(rig.audited().is_empty());
    // The check still waits, so a review and a reply work afterwards.
    rig.svc.review(&id).expect("review");
    rig.svc.reply(&id).expect("reply");
}

#[tokio::test]
async fn reply_and_decline_of_an_unknown_check_fail_with_the_fixed_sentence() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    assert_eq!(
        message_of(rig.svc.reply(&id).expect_err("unknown")),
        NOT_WAITING
    );
    assert_eq!(
        message_of(rig.svc.decline(&id).expect_err("unknown")),
        NOT_WAITING
    );
    assert_eq!(
        message_of(rig.svc.review(&id).expect_err("unknown")),
        NOT_WAITING
    );
    settle().await;
    assert!(rig.peers.sent().is_empty());
}

#[tokio::test]
async fn decline_sends_declined() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    rig.push(ANA, offer(&id, two_changes()));
    rig.svc.decline(&id).expect("decline");
    wait_for("declined", || rig.sent(ANA).len() == 1).await;
    assert_eq!(
        rig.sent(ANA)[0],
        PeerMessage::ContractCheckResult {
            check_id: id.as_str().to_string(),
            outcome: ContractCheckOutcome::Declined,
            rows: Vec::new(),
        }
    );
    assert_eq!(received_audits(&rig), vec!["declined".to_string()]);
    assert!(rig.svc.review(&id).is_err(), "a declined check is gone");
    assert_eq!(rig.locals.calls(), 0, "declining reads no request");
}

#[tokio::test]
async fn reply_after_expiry_fails_and_sends_nothing() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    rig.push(ANA, offer(&id, two_changes()));
    rig.svc.review(&id).expect("review");
    rig.svc
        .sweep(Instant::now() + CONTRACT_CHECK_TTL + Duration::from_secs(1));
    let err = rig.svc.reply(&id).expect_err("expired");
    assert!(matches!(err, DomainError::InvalidInput(_)));
    assert_eq!(message_of(err), NOT_WAITING);
    assert!(rig.svc.decline(&id).is_err());
    assert!(rig.events.all().contains(&ContractCheckEvent::Expired {
        check_id: id.as_str().to_string()
    }));
    settle().await;
    assert!(rig.peers.sent().is_empty());
    assert!(rig.audited().is_empty());
}

#[tokio::test]
async fn sweep_keeps_checks_that_are_not_due() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    rig.push(ANA, offer(&id, two_changes()));
    rig.svc.sweep(Instant::now() + Duration::from_secs(60));
    assert!(rig.svc.review(&id).is_ok());
    assert_eq!(offers_received(&rig).len(), rig.events.all().len());
}

#[tokio::test]
async fn reply_to_an_offline_node_fails_and_keeps_the_check() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    rig.push(ANA, offer(&id, two_changes()));
    rig.svc.review(&id).expect("review");
    rig.peers.set_online(&dev(ANA), false);
    assert!(rig.svc.reply(&id).is_err());
    assert!(rig.audited().is_empty());
    rig.peers.set_online(&dev(ANA), true);
    rig.svc.reply(&id).expect("reply once online");
    wait_for("result", || rig.sent(ANA).len() == 1).await;
}

// ----- provider: results -----

#[tokio::test]
async fn accepted_result_emits_result_received_and_audits_sent() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, _) = started(&rig).await;
    rig.push(
        ANA,
        result(
            &ana_id,
            ContractCheckOutcome::Answered,
            vec![(1, ContractVerdict::Pass), (0, ContractVerdict::Break)],
        ),
    );
    assert_eq!(
        results_received(&rig),
        vec![(
            ana_id.clone(),
            dev(ANA).to_base32(),
            "answered".to_string(),
            vec![
                ContractCheckRowDto {
                    index: 0,
                    verdict: ContractVerdict::Break
                },
                ContractCheckRowDto {
                    index: 1,
                    verdict: ContractVerdict::Pass
                },
            ],
        )]
    );
    assert_eq!(
        sent_audits(&rig),
        vec![(dev(ANA).short(), "Orders API".into(), "answered".into())]
    );
    let actors: Vec<String> = rig.audit.records().into_iter().map(|r| r.actor).collect();
    assert_eq!(actors, vec![String::from(AuditActor::System)]);
}

#[tokio::test]
async fn result_after_expiry_without_sweep_dropped() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, _) = started(&rig).await;
    rig.svc.advance(CONTRACT_CHECK_TTL + Duration::from_secs(1));
    rig.push(ANA, result(&ana_id, ContractCheckOutcome::Declined, vec![]));
    assert!(results_received(&rig).is_empty());
    assert!(rig.audited().is_empty());
    assert!(rig.events.all().contains(&ContractCheckEvent::Expired {
        check_id: ana_id.clone()
    }));
}

#[tokio::test]
async fn declined_and_busy_results_are_accepted_without_rows() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, ben_id) = started(&rig).await;
    rig.push(ANA, result(&ana_id, ContractCheckOutcome::Declined, vec![]));
    rig.push(BEN, result(&ben_id, ContractCheckOutcome::Busy, vec![]));
    let outcomes: Vec<_> = sent_audits(&rig).into_iter().map(|a| a.2).collect();
    assert_eq!(outcomes, vec!["declined".to_string(), "busy".to_string()]);
    assert_eq!(results_received(&rig).len(), 2);
}

#[tokio::test]
async fn declined_result_with_rows_is_dropped() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, _) = started(&rig).await;
    rig.push(
        ANA,
        result(
            &ana_id,
            ContractCheckOutcome::Declined,
            vec![(0, ContractVerdict::Pass), (1, ContractVerdict::Pass)],
        ),
    );
    assert!(results_received(&rig).is_empty());
    assert!(sent_audits(&rig).is_empty());
}

#[tokio::test]
async fn result_is_accepted_once() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, _) = started(&rig).await;
    let frame = result(
        &ana_id,
        ContractCheckOutcome::Answered,
        vec![(0, ContractVerdict::Pass), (1, ContractVerdict::Pass)],
    );
    rig.push(ANA, frame.clone());
    rig.push(ANA, frame);
    assert_eq!(results_received(&rig).len(), 1);
    assert_eq!(sent_audits(&rig).len(), 1, "no second audit row");
}

#[tokio::test]
async fn result_from_wrong_device_dropped() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, _) = started(&rig).await;
    let frame = result(
        &ana_id,
        ContractCheckOutcome::Answered,
        vec![(0, ContractVerdict::Pass), (1, ContractVerdict::Pass)],
    );
    rig.push(BEN, frame.clone());
    assert!(results_received(&rig).is_empty());
    assert!(sent_audits(&rig).is_empty());
    // The right node can still answer.
    rig.push(ANA, frame);
    assert_eq!(results_received(&rig).len(), 1);
}

#[tokio::test]
async fn result_with_duplicate_or_out_of_range_index_dropped() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, _) = started(&rig).await;
    for rows in [
        vec![(0, ContractVerdict::Pass), (0, ContractVerdict::Break)],
        vec![(0, ContractVerdict::Pass), (2, ContractVerdict::Break)],
        vec![
            (1, ContractVerdict::Pass),
            (u32::MAX, ContractVerdict::Pass),
        ],
    ] {
        rig.push(ANA, result(&ana_id, ContractCheckOutcome::Answered, rows));
    }
    assert!(results_received(&rig).is_empty());
    assert!(sent_audits(&rig).is_empty());
    // The check still waits for a good answer.
    rig.push(
        ANA,
        result(
            &ana_id,
            ContractCheckOutcome::Answered,
            vec![(0, ContractVerdict::Pass), (1, ContractVerdict::Pass)],
        ),
    );
    assert_eq!(results_received(&rig).len(), 1);
}

#[tokio::test]
async fn result_with_wrong_row_count_dropped() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, _) = started(&rig).await;
    for rows in [
        vec![],
        vec![(0, ContractVerdict::Pass)],
        vec![
            (0, ContractVerdict::Pass),
            (1, ContractVerdict::Pass),
            (2, ContractVerdict::Pass),
        ],
    ] {
        rig.push(ANA, result(&ana_id, ContractCheckOutcome::Answered, rows));
    }
    assert!(results_received(&rig).is_empty());
    assert!(sent_audits(&rig).is_empty());
}

#[tokio::test]
async fn unknown_check_id_dropped() {
    let rig = check_rig(pending(two_changes()));
    started(&rig).await;
    let other = ContractCheckId::random();
    rig.push(
        ANA,
        result(other.as_str(), ContractCheckOutcome::Declined, vec![]),
    );
    rig.push(
        ANA,
        result("not-an-id", ContractCheckOutcome::Declined, vec![]),
    );
    assert!(results_received(&rig).is_empty());
    assert!(sent_audits(&rig).is_empty());
}

#[tokio::test]
async fn provider_check_expires_on_sweep() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, ben_id) = started(&rig).await;
    rig.svc
        .sweep(Instant::now() + CONTRACT_CHECK_TTL + Duration::from_secs(1));
    let expired: Vec<_> = rig
        .events
        .all()
        .into_iter()
        .filter_map(|e| match e {
            ContractCheckEvent::Expired { check_id } => Some(check_id),
            _ => None,
        })
        .collect();
    assert_eq!(expired.len(), 2);
    assert!(expired.contains(&ana_id) && expired.contains(&ben_id));
    rig.push(ANA, result(&ana_id, ContractCheckOutcome::Declined, vec![]));
    assert!(results_received(&rig).is_empty());
}

#[tokio::test]
async fn peer_gone_drops_its_pending() {
    let rig = check_rig(pending(two_changes()));
    let (ana_id, ben_id) = started(&rig).await;
    let incoming = ContractCheckId::random();
    rig.push(ANA, offer(&incoming, two_changes()));
    rig.svc.on_peer_gone(dev(ANA));
    // The provider side: ANA's answer is no longer accepted, BEN's still is.
    rig.push(ANA, result(&ana_id, ContractCheckOutcome::Declined, vec![]));
    rig.push(BEN, result(&ben_id, ContractCheckOutcome::Declined, vec![]));
    let answered: Vec<_> = results_received(&rig).into_iter().map(|r| r.0).collect();
    assert_eq!(answered, vec![ben_id]);
    // The consumer side: ANA's offer is gone.
    assert_eq!(
        message_of(rig.svc.review(&incoming).expect_err("gone")),
        NOT_WAITING
    );
    assert!(rig.events.all().contains(&ContractCheckEvent::Expired {
        check_id: incoming.as_str().to_string()
    }));
    assert!(rig
        .events
        .all()
        .contains(&ContractCheckEvent::Expired { check_id: ana_id }));
    // ANA may offer again, and it is not busy.
    let again = ContractCheckId::random();
    rig.push(ANA, offer(&again, two_changes()));
    assert!(rig.svc.review(&again).is_ok());
}

#[test]
fn claims_only_the_two_frames() {
    let rig = check_rig(pending(Vec::new()));
    let id = ContractCheckId::random();
    assert!(rig.svc.claims(&offer(&id, two_changes())));
    assert!(rig
        .svc
        .claims(&result(id.as_str(), ContractCheckOutcome::Busy, vec![])));
    assert!(!rig.svc.claims(&PeerMessage::SyncResult {
        sync_id: id.as_str().to_string(),
        outcome: rocket_network::SyncOutcome::Applied,
    }));
}
