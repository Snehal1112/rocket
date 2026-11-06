//! Pairing flow and peer-handler tests. Two real services talk through in-memory fakes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use rocket_audit::AuditEventKind;
use rocket_network::limits::{DECLINE_COOLDOWN, PAIRING_TTL, PROTOCOL_VERSION};
use rocket_network::{
    commitment, commitment_to_base64, pairing_code, DecisionRequest, DiscoveryEvent, HelloResponse,
    NetworkEvent, NodeProfile, Nonce, NonceMessage, PairRequest, PairRequestResponse,
    PairStatusResponse, PairingId, PairingStatus, PeerError, PeerHandler, PeerIdentity,
    PeerMessage, PeerSocket,
};
use rocket_shared::error::DomainError;
use std::net::IpAddr;
use std::sync::Arc;

use super::test_support::*;
use super::{paired_limit_message, NetworkHandle, NodeGroup, PairingDecision};

/// Two started nodes that see each other.
async fn ready() -> (Node, Node, NetworkHandle, NetworkHandle) {
    let a = node(1, "ann");
    let b = node(2, "ben");
    link(&a, &b);
    let ha = a.svc.start().await;
    let hb = b.svc.start().await;
    see(&a, &b);
    see(&b, &a);
    settle().await;
    (a, b, ha, hb)
}

fn audited(n: &Node) -> Vec<AuditEventKind> {
    n.audit.records().into_iter().map(|r| r.kind).collect()
}

fn nodes_changed_since(n: &Node, mark: usize) -> bool {
    n.events.all()[mark..].contains(&NetworkEvent::NodesChanged)
}

// ----- the happy paths -----

#[tokio::test(start_paused = true)]
async fn both_accept_and_each_side_saves_the_other() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;

    let incoming = b.events.incoming();
    assert_eq!(incoming.len(), 1, "one notice on the responder");
    let NetworkEvent::PairingIncoming {
        pairing_id,
        device_id,
        short_id,
        display_name,
        code,
    } = &incoming[0]
    else {
        panic!("not an incoming notice");
    };
    assert_eq!(pairing_id, &started.pairing_id.0);
    assert_eq!(device_id, &a.id().to_base32());
    assert_eq!(short_id, &a.id().short());
    assert_eq!(display_name, "ann");
    assert_eq!(
        code,
        &started.code.digits(),
        "both screens show the same code"
    );
    assert_eq!(code.len(), 6);

    // The responder accepts first. Nothing is saved until the initiator accepts too.
    let mark_b = b.events.all().len();
    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("responder accepts");
    assert!(!b.paired.contains(&a.id()));
    assert!(b.events.finished().is_empty());

    let mark_a = a.events.all().len();
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
    settle().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;

    assert!(a.paired.contains(&b.id()));
    assert!(b.paired.contains(&a.id()));
    assert_eq!(a.events.finished(), vec!["accepted"]);
    assert_eq!(b.events.finished(), vec!["accepted"]);
    assert!(
        nodes_changed_since(&a, mark_a),
        "the panel refetches on the initiator"
    );
    assert!(
        nodes_changed_since(&b, mark_b),
        "the panel refetches on the responder"
    );
    assert!(audited(&a).contains(&AuditEventKind::NetworkDevicePaired {
        device_id: b.id().to_base32()
    }));
    assert!(audited(&b).contains(&AuditEventKind::NetworkDevicePaired {
        device_id: a.id().to_base32()
    }));
    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].group, NodeGroup::Paired);
    assert_eq!(rows[0].display_name, "ben");

    // The pair then connects on its own.
    tokio::time::advance(Duration::from_secs(15)).await;
    settle().await;
    assert_eq!(row_status(&a, &b.id()), Some(super::NodeStatus::Online));
    assert_eq!(row_status(&b, &a.id()), Some(super::NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn the_responder_keeps_the_pairing_while_the_initiator_is_still_saving() {
    let x = node(1, "ann");
    let y = node(2, "ben");
    link(&x, &y);
    let (low, high) = ordered(&x, &y);
    let _hl = low.svc.start().await;
    let _hh = high.svc.start().await;
    see(low, high);
    see(high, low);
    settle().await;

    // The higher node starts, so the lower node is the responder and dials at once.
    let started = high.svc.request_pairing(&low.id()).await.expect("request");
    settle().await;
    accept_both(&low.svc, &high.svc, &started.pairing_id).await;
    settle().await;

    // The initiator has not saved yet, so it answers "not paired". That is not a removal.
    assert!(low.paired.contains(&high.id()));
    assert!(low.events.removed().is_empty());

    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert!(
        high.paired.contains(&low.id()),
        "the initiator's poll saved it"
    );
    tokio::time::advance(Duration::from_secs(15)).await;
    settle().await;
    assert_eq!(row_status(low, &high.id()), Some(super::NodeStatus::Online));
    assert_eq!(row_status(high, &low.id()), Some(super::NodeStatus::Online));
}

#[tokio::test(start_paused = true)]
async fn the_initiator_can_accept_before_the_responder() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;

    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert!(
        !a.paired.contains(&b.id()),
        "still waiting for the other side"
    );
    assert_eq!(
        row_status(&a, &b.id()),
        Some(super::NodeStatus::Pairing),
        "the row shows Pairing while the session is open"
    );

    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("responder accepts");
    assert!(b.paired.contains(&a.id()), "the responder saves at once");
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;

    assert!(a.paired.contains(&b.id()));
    assert_eq!(a.events.finished(), vec!["accepted"]);
    assert_eq!(b.events.finished(), vec!["accepted"]);
}

#[tokio::test(start_paused = true)]
async fn a_decline_ends_both_sides_and_starts_a_cooldown() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");

    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Decline)
        .await
        .expect("responder declines");
    assert_eq!(
        b.events.finished(),
        vec!["declined"],
        "the decliner is told too"
    );

    settle().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(a.events.finished(), vec!["declined"]);
    assert!(!a.paired.contains(&b.id()));
    assert!(!b.paired.contains(&a.id()));

    // Inside the cooldown the responder says busy.
    let err = a.svc.request_pairing(&b.id()).await.expect_err("cooldown");
    assert_eq!(
        err,
        DomainError::InvalidInput("The other device is busy. Try again in a minute.".into())
    );

    // After the cooldown a new request works.
    tokio::time::advance(DECLINE_COOLDOWN + Duration::from_secs(1)).await;
    settle().await;
    a.svc
        .request_pairing(&b.id())
        .await
        .expect("allowed after the cooldown");
}

#[tokio::test(start_paused = true)]
async fn blocking_refuses_the_device_until_it_is_unblocked() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");

    let mark = b.events.all().len();
    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Block)
        .await
        .expect("responder blocks");
    assert_eq!(b.events.finished(), vec!["declined"]);
    assert!(nodes_changed_since(&b, mark));
    assert!(b.blocked.has(&a.id()));
    assert!(audited(&b).contains(&AuditEventKind::NetworkDeviceBlocked {
        device_id: a.id().to_base32()
    }));

    // The sender sees the same "declined" as for a plain decline.
    settle().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(a.events.finished(), vec!["declined"]);

    // A new request from the blocked device is refused outright.
    let refused = b
        .svc
        .pair_request(a.identity.clone(), ip(), pair_request("ann"))
        .await
        .expect_err("blocked");
    assert_eq!(refused, PeerError::Forbidden);

    // Unblocking lifts it (after the cooldown, which is separate).
    b.svc.unblock(&a.id()).expect("unblock");
    tokio::time::advance(DECLINE_COOLDOWN + Duration::from_secs(1)).await;
    settle().await;
    b.svc
        .pair_request(a.identity.clone(), ip(), pair_request("ann"))
        .await
        .expect("allowed again");
}

#[tokio::test(start_paused = true)]
async fn cancelling_as_the_initiator_tells_the_peer() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    assert_eq!(b.events.incoming().len(), 1);

    a.svc.cancel_pairing(&started.pairing_id).expect("cancel");
    settle().await;

    assert_eq!(a.events.finished(), vec!["cancelled"]);
    assert_eq!(
        b.events.finished(),
        vec!["declined"],
        "the peer's notice closes"
    );
    let err = a
        .svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect_err("session is over");
    assert_eq!(
        err,
        DomainError::InvalidInput("That pairing request is no longer open.".into())
    );
    a.svc
        .cancel_pairing(&started.pairing_id)
        .expect("cancelling twice is fine");
    assert_eq!(a.events.finished(), vec!["cancelled"], "published once");

    let unknown = a
        .svc
        .cancel_pairing(&PairingId::new())
        .expect_err("unknown session");
    assert!(matches!(unknown, DomainError::InvalidInput(_)));
}

#[tokio::test(start_paused = true)]
async fn the_responder_can_cancel_too() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    b.svc.cancel_pairing(&started.pairing_id).expect("cancel");
    assert_eq!(b.events.finished(), vec!["cancelled"]);
}

// ----- failures partway end as failed, not cancelled -----

#[tokio::test(start_paused = true)]
async fn a_decision_that_cannot_be_sent_ends_the_pairing_as_failed() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;

    // The responder drops off the network before the initiator accepts.
    a.connector.remove_route(b.addr());
    let err = a
        .svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect_err("the decision cannot reach the peer");
    assert_eq!(
        err,
        DomainError::InvalidInput("Pairing failed. Try again.".into())
    );
    settle().await;

    assert_eq!(
        a.events.finished(),
        vec!["failed"],
        "a failure, not a cancel"
    );
    assert!(!a.paired.contains(&b.id()), "nothing is saved");
}

#[tokio::test(start_paused = true)]
async fn a_responder_that_stops_answering_while_polled_ends_the_pairing_as_failed() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
    settle().await;

    // The responder vanishes while the initiator polls for its answer.
    a.connector.remove_route(b.addr());
    for _ in 0..12 {
        tokio::time::advance(Duration::from_secs(1)).await;
        settle().await;
    }

    assert_eq!(
        a.events.finished(),
        vec!["failed"],
        "a failure, not a cancel"
    );
    assert!(!a.paired.contains(&b.id()), "nothing is saved");
}

// ----- refusals on the initiator side -----

#[tokio::test(start_paused = true)]
async fn pairing_is_refused_while_networking_is_off() {
    let a = node(1, "ann");
    a.id_store.fail.store(true, Ordering::SeqCst);
    let _ha = a.svc.start().await;

    let err = a
        .svc
        .request_pairing(&identity(2).device_id)
        .await
        .expect_err("off");
    assert_eq!(err, DomainError::InvalidInput("Networking is off.".into()));
    let refused = a
        .svc
        .pair_request(identity(2), ip(), pair_request("Ben"))
        .await
        .expect_err("off");
    assert_eq!(refused, PeerError::Forbidden);
}

#[tokio::test(start_paused = true)]
async fn a_device_that_is_not_nearby_cannot_be_paired() {
    let a = node(1, "ann");
    let _ha = a.svc.start().await;
    let err = a
        .svc
        .request_pairing(&identity(2).device_id)
        .await
        .expect_err("unknown");
    assert_eq!(
        err,
        DomainError::InvalidInput("That device is no longer nearby.".into())
    );
}

#[tokio::test(start_paused = true)]
async fn a_paired_or_blocked_device_cannot_be_requested_again() {
    let (a, b, _ha, _hb) = ready().await;
    a.paired.seed(b.id(), "ben");
    let err = a.svc.request_pairing(&b.id()).await.expect_err("paired");
    assert_eq!(
        err,
        DomainError::InvalidInput("This device is already paired.".into())
    );

    let (c, d, _hc, _hd) = {
        let c = node(3, "cat");
        let d = node(4, "dan");
        link(&c, &d);
        let hc = c.svc.start().await;
        let hd = d.svc.start().await;
        see(&c, &d);
        settle().await;
        (c, d, hc, hd)
    };
    c.blocked.put(d.id(), "dan");
    let err = c.svc.request_pairing(&d.id()).await.expect_err("blocked");
    assert_eq!(
        err,
        DomainError::InvalidInput("This device is blocked. Unblock it in Settings to pair.".into())
    );
}

#[tokio::test(start_paused = true)]
async fn the_201st_outgoing_request_shows_the_limit_message() {
    let (a, b, _ha, _hb) = ready().await;
    a.paired.fill(200);

    let err = a.svc.request_pairing(&b.id()).await.expect_err("full");
    assert_eq!(err, DomainError::InvalidInput(paired_limit_message()));
    assert_eq!(
        paired_limit_message(),
        "You've reached the limit of 200 paired devices. Remove one to pair."
    );
    assert_eq!(a.connector.pair_request_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn a_hidden_responder_refuses_new_requests_but_still_answers_hello() {
    let (a, b, _ha, _hb) = ready().await;
    b.svc.set_hidden(true).await.expect("hide");

    let err = a.svc.request_pairing(&b.id()).await.expect_err("hidden");
    assert_eq!(
        err,
        DomainError::InvalidInput("Pairing failed. Try again.".into())
    );
    assert!(b.events.incoming().is_empty());
    assert_eq!(b.svc.hello().await.device_id, b.id());
}

#[tokio::test(start_paused = true)]
async fn a_responder_that_lies_about_its_nonce_fails_the_pairing() {
    // A responder that commits to one value and reveals another.
    struct Liar {
        identity: PeerIdentity,
        declines: AtomicUsize,
    }

    #[async_trait]
    impl PeerHandler for Liar {
        async fn hello(&self) -> HelloResponse {
            HelloResponse {
                device_id: self.identity.device_id,
                version: PROTOCOL_VERSION,
                profile: profile("Liar"),
            }
        }
        async fn pair_request(
            &self,
            _peer: PeerIdentity,
            _ip: IpAddr,
            _req: PairRequest,
        ) -> Result<PairRequestResponse, PeerError> {
            Ok(PairRequestResponse {
                pairing_id: PairingId::new(),
                commitment: commitment_to_base64(&[1u8; 32]),
            })
        }
        async fn pair_nonce(
            &self,
            _peer: PeerIdentity,
            _id: PairingId,
            _msg: NonceMessage,
        ) -> Result<NonceMessage, PeerError> {
            Ok(NonceMessage {
                nonce: Nonce::random().to_base64(),
            })
        }
        async fn pair_decision(
            &self,
            _peer: PeerIdentity,
            _id: PairingId,
            req: DecisionRequest,
        ) -> Result<(), PeerError> {
            assert!(!req.accept, "the initiator must decline");
            self.declines.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn pair_status(
            &self,
            _peer: PeerIdentity,
            _id: PairingId,
        ) -> Result<PairStatusResponse, PeerError> {
            Ok(PairStatusResponse {
                status: PairingStatus::Declined,
            })
        }
        async fn is_paired(&self, _device: &rocket_network::DeviceId) -> bool {
            false
        }
        async fn on_peer_socket(&self, _socket: PeerSocket) {}
        fn local_hello(&self) -> PeerMessage {
            PeerMessage::Hello {
                version: PROTOCOL_VERSION,
                profile: profile("Liar"),
            }
        }
    }

    let a = node(1, "ann");
    let liar = Arc::new(Liar {
        identity: identity(7),
        declines: AtomicUsize::new(0),
    });
    a.connector.add_route(
        addr(7),
        Route {
            identity: liar.identity.clone(),
            profile: profile("Liar"),
            version: PROTOCOL_VERSION,
            handler: Some(liar.clone()),
            claims: None,
        },
    );
    let _ha = a.svc.start().await;
    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: liar.identity.device_id,
        profile: profile("Liar"),
        addresses: vec![addr(7)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    let err = a
        .svc
        .request_pairing(&liar.identity.device_id)
        .await
        .expect_err("commitment mismatch");
    assert_eq!(
        err,
        DomainError::InvalidInput("Pairing failed. Try again.".into())
    );
    assert_eq!(a.events.finished(), vec!["failed"]);
    assert_eq!(a.paired.count(), 0, "nothing is saved");
    assert_eq!(
        liar.declines.load(Ordering::SeqCst),
        1,
        "the peer is told to drop it"
    );
}

// ----- the handler, called as the peer server would call it -----

#[tokio::test(start_paused = true)]
async fn the_notice_appears_only_after_the_nonce_and_shows_the_right_code() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    let stranger = identity(10);

    let reply = b
        .svc
        .pair_request(stranger.clone(), ip(), pair_request("Mallory"))
        .await
        .expect("request");
    assert!(
        b.events.incoming().is_empty(),
        "no notice after the request alone"
    );

    let na = Nonce::random();
    let revealed = b
        .svc
        .pair_nonce(
            stranger.clone(),
            reply.pairing_id.clone(),
            NonceMessage {
                nonce: na.to_base64(),
            },
        )
        .await
        .expect("nonce");
    let nb = Nonce::parse_base64(&revealed.nonce).expect("nb");

    // The commitment from the first reply matches the revealed nonce.
    let expected_commitment = commitment(&b.identity.spki_der, &stranger.spki_der, &nb);
    assert_eq!(commitment_to_base64(&expected_commitment), reply.commitment);

    let incoming = b.events.incoming();
    assert_eq!(incoming.len(), 1);
    let NetworkEvent::PairingIncoming {
        code, display_name, ..
    } = &incoming[0]
    else {
        panic!("not an incoming notice");
    };
    let expected = pairing_code(&stranger.spki_der, &b.identity.spki_der, &na, &nb);
    assert_eq!(
        code,
        &expected.digits(),
        "initiator key first, responder key second"
    );
    assert_eq!(display_name, "Mallory");
}

#[tokio::test(start_paused = true)]
async fn only_the_key_that_opened_a_session_can_use_it() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    let intruder = identity(30);
    let id = started.pairing_id.clone();

    let nonce = b
        .svc
        .pair_nonce(
            intruder.clone(),
            id.clone(),
            NonceMessage {
                nonce: Nonce::random().to_base64(),
            },
        )
        .await
        .expect_err("wrong key");
    assert_eq!(nonce, PeerError::Forbidden);
    let decision = b
        .svc
        .pair_decision(
            intruder.clone(),
            id.clone(),
            DecisionRequest { accept: true },
        )
        .await
        .expect_err("wrong key");
    assert_eq!(decision, PeerError::Forbidden);
    let status = b
        .svc
        .pair_status(intruder, id.clone())
        .await
        .expect_err("wrong key");
    assert_eq!(status, PeerError::Forbidden);

    let unknown = b
        .svc
        .pair_status(a.identity.clone(), PairingId::new())
        .await
        .expect_err("unknown id");
    assert_eq!(unknown, PeerError::NotFound);
}

#[tokio::test(start_paused = true)]
async fn the_201st_incoming_request_is_forbidden() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    b.paired.fill(199);
    b.paired.seed(identity(11).device_id, "Known");
    assert_eq!(b.paired.count(), 200);

    let refused = b
        .svc
        .pair_request(identity(10), ip(), pair_request("Newcomer"))
        .await
        .expect_err("list is full");
    assert_eq!(refused, PeerError::Forbidden);
    assert!(b.events.incoming().is_empty());

    // A device that is already paired may pair again.
    b.svc
        .pair_request(identity(11), ip(), pair_request("Known"))
        .await
        .expect("a known device is not a 201st");
}

#[tokio::test(start_paused = true)]
async fn the_sixth_request_in_a_minute_is_busy_and_the_window_slides() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    for seed in 10..15u8 {
        b.svc
            .pair_request(identity(seed), ip(), pair_request("Stranger"))
            .await
            .expect("within the limit");
    }
    let busy = b
        .svc
        .pair_request(identity(15), ip(), pair_request("Stranger"))
        .await
        .expect_err("sixth");
    assert_eq!(busy, PeerError::Busy);
    assert!(
        b.events.incoming().is_empty(),
        "a refused request shows no notice"
    );

    tokio::time::advance(Duration::from_secs(61)).await;
    settle().await;
    b.svc
        .pair_request(identity(15), ip(), pair_request("Stranger"))
        .await
        .expect("a minute later");
}

#[tokio::test(start_paused = true)]
async fn one_open_session_per_remote_device() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    b.svc
        .pair_request(identity(10), ip(), pair_request("Stranger"))
        .await
        .expect("first");
    let second = b
        .svc
        .pair_request(identity(10), ip(), pair_request("Stranger"))
        .await
        .expect_err("second");
    assert_eq!(second, PeerError::Busy);
}

#[tokio::test(start_paused = true)]
async fn a_bad_version_or_profile_is_a_bad_request() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;

    let version = b
        .svc
        .pair_request(
            identity(10),
            ip(),
            PairRequest {
                version: PROTOCOL_VERSION + 1,
                profile: profile("Old"),
            },
        )
        .await
        .expect_err("version");
    assert_eq!(version, PeerError::BadRequest);

    for name in ["", "bad\u{7}name", &"x".repeat(41)] {
        let bad = b
            .svc
            .pair_request(
                identity(11),
                ip(),
                PairRequest {
                    version: PROTOCOL_VERSION,
                    profile: NodeProfile {
                        display_name: name.to_string(),
                        initials: "ZZZ".to_string(),
                    },
                },
            )
            .await
            .expect_err("profile");
        assert_eq!(bad, PeerError::BadRequest, "{name:?}");
    }
}

// ----- expiry -----

#[tokio::test(start_paused = true)]
async fn a_decision_after_expiry_saves_nothing_and_says_expired() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;

    tokio::time::advance(PAIRING_TTL + Duration::from_secs(1)).await;

    let err = b
        .svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect_err("expired");
    assert_eq!(
        err,
        DomainError::InvalidInput("The request expired.".into())
    );
    assert!(!b.paired.contains(&a.id()));
    assert_eq!(b.events.finished(), vec!["expired"], "announced once");

    let err = a
        .svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect_err("expired");
    assert_eq!(
        err,
        DomainError::InvalidInput("The request expired.".into())
    );
    assert!(!a.paired.contains(&b.id()));
    assert_eq!(a.events.finished(), vec!["expired"]);
}

#[tokio::test(start_paused = true)]
async fn a_remote_decision_after_expiry_is_ignored() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    let stranger = identity(10);
    let reply = b
        .svc
        .pair_request(stranger.clone(), ip(), pair_request("Mallory"))
        .await
        .expect("request");
    b.svc
        .pair_nonce(
            stranger.clone(),
            reply.pairing_id.clone(),
            NonceMessage {
                nonce: Nonce::random().to_base64(),
            },
        )
        .await
        .expect("nonce");

    tokio::time::advance(PAIRING_TTL + Duration::from_secs(1)).await;

    b.svc
        .pair_decision(
            stranger.clone(),
            reply.pairing_id.clone(),
            DecisionRequest { accept: true },
        )
        .await
        .expect("a late decision is acknowledged and ignored");
    let status = b
        .svc
        .pair_status(stranger.clone(), reply.pairing_id.clone())
        .await
        .expect("status");
    assert_eq!(status.status, PairingStatus::Expired);
    assert!(!b.paired.contains(&stranger.device_id));
    assert_eq!(b.events.finished(), vec!["expired"]);
}

#[tokio::test(start_paused = true)]
async fn old_finished_sessions_are_purged() {
    let b = node(2, "ben");
    let _hb = b.svc.start().await;
    let stranger = identity(10);
    let reply = b
        .svc
        .pair_request(stranger.clone(), ip(), pair_request("Mallory"))
        .await
        .expect("request");

    settle().await;
    tokio::time::advance(PAIRING_TTL * 3).await;
    settle().await;

    let gone = b
        .svc
        .pair_status(stranger, reply.pairing_id)
        .await
        .expect_err("purged");
    assert_eq!(gone, PeerError::NotFound);
}

#[tokio::test(start_paused = true)]
async fn an_accept_just_before_the_responder_expires_still_pairs_both_sides() {
    let (a, b, _ha, _hb) = ready().await;
    let started = a.svc.request_pairing(&b.id()).await.expect("request");
    settle().await;
    a.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("initiator accepts");
    settle().await;

    // The responder accepts half a second before its request expires.
    tokio::time::advance(PAIRING_TTL - Duration::from_millis(500)).await;
    settle().await;
    b.svc
        .respond_pairing(&started.pairing_id, PairingDecision::Accept)
        .await
        .expect("responder accepts in time");
    assert!(
        b.paired.contains(&a.id()),
        "the responder saved the pairing"
    );

    // The initiator's next poll lands after its own timer. It must still ask the responder.
    tokio::time::advance(Duration::from_secs(2)).await;
    settle().await;
    assert!(a.paired.contains(&b.id()), "the initiator saved it too");
    assert_eq!(a.events.finished(), vec!["accepted"]);
    assert_eq!(b.events.finished(), vec!["accepted"]);
}
