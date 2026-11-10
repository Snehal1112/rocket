//! End-to-end contract check tests: two real nodes over real TLS on 127.0.0.1, each with the
//! stack of `e2e_sync_tests` (the real `FrameRouter` with its three services). Only the
//! contract check ports are fakes: the provider's pending contract and the consumer's requests.

use rocket_audit::AuditEventKind;
use rocket_network::{
    ContractChange, ContractChangeType, ContractCheckEvent, ContractCheckRowDto, ContractVerdict,
};

use super::contract_check_fakes::{header_change, pending, user_request};
use super::e2e_sync_tests::{attach_sync, Peer};
use super::e2e_tests::{online_pair, row, wait_until};
use super::{NetworkHandle, NodeStatus};

/// A paired, online provider (`a`) and consumer (`b`). The handles keep the nodes running.
struct CheckPair {
    a: Peer,
    b: Peer,
    _ha: NetworkHandle,
    _hb: NetworkHandle,
}

async fn check_pair() -> CheckPair {
    let (a, b, ha, hb) = online_pair().await;
    CheckPair {
        a: attach_sync(a, Vec::new()),
        b: attach_sync(b, Vec::new()),
        _ha: ha,
        _hb: hb,
    }
}

/// Three changes: a header the consumer does not send, a header it still sends and a path it
/// does not call.
fn three_changes() -> Vec<ContractChange> {
    let mut unused = header_change("X-Trace", ContractChangeType::Added, None, Some("on"), true);
    unused.path_template = "/orders/{id}".into();
    vec![
        header_change("X-Trace", ContractChangeType::Added, None, Some("on"), true),
        header_change("X-Old", ContractChangeType::Removed, Some("1"), None, true),
        unused,
    ]
}

const CONTRACT_ID: &str = "01JCONTRACT0000000000000000";

/// The provider starts one check with the consumer and returns its id.
fn start(p: &CheckPair) -> String {
    let started =
        p.a.checks
            .start("/mem/Orders", CONTRACT_ID, &[p.b.id()])
            .expect("the check starts");
    assert_eq!(started.check_ids.len(), 1);
    assert_eq!(started.check_ids[0].0, p.b.id());
    started.check_ids[0].1.as_str().to_string()
}

fn offer_seen(peer: &Peer, id: &str) -> bool {
    peer.check_events
        .all()
        .iter()
        .any(|e| matches!(e, ContractCheckEvent::OfferReceived { check_id, .. } if check_id == id))
}

fn result_of(peer: &Peer, id: &str) -> Option<(String, String, Vec<ContractCheckRowDto>)> {
    peer.check_events.all().into_iter().find_map(|e| match e {
        ContractCheckEvent::ResultReceived {
            check_id,
            device_id,
            outcome,
            rows,
        } if check_id == id => Some((device_id, outcome, rows)),
        _ => None,
    })
}

fn check_id_of(id: &str) -> rocket_network::ContractCheckId {
    rocket_network::ContractCheckId::parse(id).expect("a check id")
}

#[tokio::test]
async fn offer_review_and_reply_reach_the_provider_with_the_rows() {
    let p = check_pair().await;
    p.a.check_changes.set(Some(pending(three_changes())));
    p.b.check_locals
        .set(vec![user_request("Get user", &[("X-Old", "1")])]);

    let id = start(&p);
    wait_until("the consumer shows the offer", || offer_seen(&p.b, &id)).await;
    match p.b.check_events.all().first() {
        Some(ContractCheckEvent::OfferReceived {
            device_id,
            contract,
            change_count,
            ..
        }) => {
            assert_eq!(device_id, &p.a.id().to_base32());
            assert_eq!(contract, "Orders API");
            assert_eq!(*change_count, 3);
        }
        other => panic!("the first event is the offer: {other:?}"),
    }
    assert!(
        result_of(&p.a, &id).is_none(),
        "nothing is answered before the user's click"
    );

    let review = p.b.checks.review(&check_id_of(&id)).expect("the review");
    let verdicts: Vec<ContractVerdict> = review.changes.iter().map(|c| c.verdict).collect();
    assert_eq!(
        verdicts,
        vec![
            ContractVerdict::Break,
            ContractVerdict::Break,
            ContractVerdict::NotUsed
        ]
    );
    assert_eq!(review.changes[0].matches[0].name, "Get user");

    p.b.checks.reply(&check_id_of(&id)).expect("the reply");
    wait_until("the provider gets the result", || {
        result_of(&p.a, &id).is_some()
    })
    .await;
    let (device_id, outcome, rows) = result_of(&p.a, &id).expect("a result");
    assert_eq!(device_id, p.b.id().to_base32());
    assert_eq!(outcome, "answered");
    let got: Vec<(u32, ContractVerdict)> = rows.iter().map(|r| (r.index, r.verdict)).collect();
    assert_eq!(
        got,
        vec![
            (0, ContractVerdict::Break),
            (1, ContractVerdict::Break),
            (2, ContractVerdict::NotUsed)
        ]
    );
    assert!(p.a.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkContractCheckSent { outcome, .. } if outcome == "answered"
    )));
    assert!(p.b.node.audited().iter().any(|k| matches!(
        k,
        AuditEventKind::NetworkContractCheckReceived { outcome, .. } if outcome == "answered"
    )));
}

#[tokio::test]
async fn a_declined_check_reaches_the_provider_without_rows() {
    let p = check_pair().await;
    p.a.check_changes.set(Some(pending(three_changes())));

    let id = start(&p);
    wait_until("the consumer shows the offer", || offer_seen(&p.b, &id)).await;
    p.b.checks.decline(&check_id_of(&id)).expect("the decline");

    wait_until("the provider gets the result", || {
        result_of(&p.a, &id).is_some()
    })
    .await;
    let (_, outcome, rows) = result_of(&p.a, &id).expect("a result");
    assert_eq!(outcome, "declined");
    assert!(rows.is_empty());
}

#[tokio::test]
async fn a_second_check_while_one_waits_comes_back_busy() {
    let p = check_pair().await;
    p.a.check_changes.set(Some(pending(three_changes())));

    let first = start(&p);
    wait_until("the consumer shows the first offer", || {
        offer_seen(&p.b, &first)
    })
    .await;
    let second = start(&p);

    wait_until("the provider gets busy", || {
        result_of(&p.a, &second).is_some()
    })
    .await;
    let (_, outcome, rows) = result_of(&p.a, &second).expect("a result");
    assert_eq!(outcome, "busy");
    assert!(rows.is_empty());
    assert!(!offer_seen(&p.b, &second), "the busy offer is not shown");
    assert!(
        result_of(&p.a, &first).is_none(),
        "the first check still waits for the user"
    );
}

#[tokio::test]
async fn an_offer_from_a_blocked_device_is_dropped_before_the_service() {
    let p = check_pair().await;
    p.a.check_changes.set(Some(pending(three_changes())));
    // The consumer blocks the provider while the socket is still open.
    p.b.node.blocked.put(p.a.id(), "ann");

    let id = start(&p);
    wait_until("the consumer closes the connection", || {
        row(&p.b.node, &p.a.id()) != Some(NodeStatus::Online)
    })
    .await;

    assert!(
        p.b.check_events.all().is_empty(),
        "the service never saw the offer"
    );
    assert!(!p
        .b
        .node
        .audited()
        .iter()
        .any(|k| matches!(k, AuditEventKind::NetworkContractCheckReceived { .. })));
    wait_until("the provider's check ends", || {
        p.a.check_events
            .all()
            .iter()
            .any(|e| matches!(e, ContractCheckEvent::Expired { check_id } if *check_id == id))
    })
    .await;
    assert!(result_of(&p.a, &id).is_none());
}
