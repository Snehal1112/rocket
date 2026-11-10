//! Router tests for the third service: contract check frames go to `ContractCheckService` only,
//! and it gets every peer-gone notice.

use std::time::Duration;

use rocket_network::{
    ContractChangeType, ContractCheckEvent, ContractCheckId, ContractCheckOutcome,
    ContractPolicyWire, PeerMessage, TransferSink,
};

use super::contract_check_fakes::header_change;
use super::sync_fakes::*;

fn offer(id: &ContractCheckId) -> PeerMessage {
    PeerMessage::ContractCheckOffer {
        check_id: id.as_str().to_string(),
        contract: "Orders API".into(),
        version: "1.2.0".into(),
        policy: ContractPolicyWire::Strict,
        changes: vec![header_change(
            "X-Trace",
            ContractChangeType::Added,
            None,
            Some("on"),
            true,
        )],
    }
}

fn offers_received(rig: &SyncRig) -> Vec<String> {
    rig.check_events
        .all()
        .into_iter()
        .filter_map(|e| match e {
            ContractCheckEvent::OfferReceived { check_id, .. } => Some(check_id),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn contract_check_frames_go_to_the_contract_check_service_only() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let id = ContractCheckId::random();

    rig.push(ANA, offer(&id));
    rig.push(
        ANA,
        PeerMessage::ContractCheckResult {
            check_id: ContractCheckId::random().as_str().to_string(),
            outcome: ContractCheckOutcome::Declined,
            rows: Vec::new(),
        },
    );
    wait_for("the offer reached the contract check service", || {
        offers_received(&rig) == vec![id.as_str().to_string()]
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(rig.sync.frames_seen(), 0, "the sync service saw nothing");
    assert!(
        rig.transfer_events.all().is_empty(),
        "the transfer service saw nothing"
    );
    assert!(
        rig.sent(ANA).is_empty(),
        "no answer before the user's click"
    );
}

#[tokio::test]
async fn peer_gone_reaches_the_contract_check_service() {
    let rig = sync_rig();
    rig.peers.add(dev(ANA), "Ana");
    let id = ContractCheckId::random();
    rig.push(ANA, offer(&id));
    wait_for("the offer", || !offers_received(&rig).is_empty()).await;

    rig.router.on_peer_gone(dev(ANA));

    wait_for("the check expired", || {
        rig.check_events.all().iter().any(
            |e| matches!(e, ContractCheckEvent::Expired { check_id } if *check_id == id.as_str()),
        )
    })
    .await;
    wait_for("the sync service saw it", || rig.sync.gone_seen() == 1).await;
}
