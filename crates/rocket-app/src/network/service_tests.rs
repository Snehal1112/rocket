//! Tests for start, status, node listing, settings and addresses. All ports are fakes.

use std::sync::atomic::Ordering;
use std::time::Duration;

use rocket_network::limits::{MANUAL_PROBE_INTERVAL, PROTOCOL_VERSION};
use rocket_network::{DeviceId, DiscoveryEvent, DiscoverySource, NetworkEvent};
use rocket_shared::error::DomainError;

use super::test_support::*;
use super::{NetworkState, NodeGroup, NodeStatus};

#[tokio::test(start_paused = true)]
async fn start_runs_and_announces_with_the_port_in_use() {
    let n = node(1, "sam");
    let _handle = n.svc.start().await;

    let status = n.svc.status();
    assert_eq!(status.state, NetworkState::Running);
    assert_eq!(status.port, Some(47821));
    assert!(status.listener_available);
    assert!(status.discovery_available);
    assert_eq!(status.profile.display_name, "sam");
    assert_eq!(status.device_short_id, Some(n.id().short()));
    assert!(!status.hidden);
    assert!(!status.intro_seen);

    let announced = n.discovery.announcements();
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].port, 47821);
    assert_eq!(announced[0].device_id, n.id());
    assert_eq!(announced[0].version, PROTOCOL_VERSION);
    assert_eq!(announced[0].profile.display_name, "sam");
    assert!(n.events.all().contains(&NetworkEvent::StatusChanged));
}

#[tokio::test(start_paused = true)]
async fn keychain_unavailable_turns_networking_off_without_a_listener() {
    let n = node(1, "sam");
    n.id_store.fail.store(true, Ordering::SeqCst);
    let _handle = n.svc.start().await;

    assert_eq!(
        n.svc.status().state,
        NetworkState::Off {
            reason: "The keychain is not available, so networking is off.".to_string()
        }
    );
    assert_eq!(n.factory.builds.load(Ordering::SeqCst), 0, "no TLS setup");
    assert_eq!(n.server.starts.load(Ordering::SeqCst), 0, "no listener");
    assert_eq!(n.discovery.starts.load(Ordering::SeqCst), 0, "no discovery");
    assert!(n.discovery.announcements().is_empty(), "nothing announced");
    assert!(n.svc.list_nodes().is_empty());
    assert!(n.events.all().contains(&NetworkEvent::StatusChanged));
}

#[tokio::test(start_paused = true)]
async fn a_transport_setup_failure_also_turns_networking_off() {
    let n = node(1, "sam");
    n.factory.fail.store(true, Ordering::SeqCst);
    let _handle = n.svc.start().await;

    assert!(matches!(n.svc.status().state, NetworkState::Off { .. }));
    assert_eq!(n.server.starts.load(Ordering::SeqCst), 0);
    assert!(n.discovery.announcements().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_bind_failure_keeps_running_but_reports_no_listener() {
    let n = node(1, "sam");
    *n.server.port.lock().expect("port lock") = Err("address in use".to_string());
    let _handle = n.svc.start().await;

    let status = n.svc.status();
    assert_eq!(status.state, NetworkState::Running);
    assert!(!status.listener_available);
    assert_eq!(status.port, None);
    assert!(
        n.discovery.announcements().is_empty(),
        "no port to announce"
    );
}

#[tokio::test(start_paused = true)]
async fn a_discovery_failure_reports_it_and_manual_addresses_still_work() {
    let n = node(1, "sam");
    n.discovery.fail_start.store(true, Ordering::SeqCst);
    n.connector
        .add_route(addr(5), Route::fixed(identity(5), "Pat"));
    let _handle = n.svc.start().await;

    let status = n.svc.status();
    assert_eq!(status.state, NetworkState::Running);
    assert!(!status.discovery_available);
    assert!(n.discovery.announcements().is_empty());

    n.svc.add_address("127.0.0.1", 47005).await.expect("add");
    assert_eq!(n.svc.list_nodes().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_hidden_node_starts_without_announcing() {
    let n = node(1, "sam");
    n.settings.set_hidden(true);
    let _handle = n.svc.start().await;

    assert!(n.svc.status().hidden);
    assert!(n.discovery.announcements().is_empty());
}

#[tokio::test(start_paused = true)]
async fn hiding_withdraws_and_showing_announces_again() {
    let n = node(1, "sam");
    let _handle = n.svc.start().await;
    assert_eq!(n.discovery.announcements().len(), 1);

    n.svc.set_hidden(true).await.expect("hide");
    assert_eq!(n.discovery.withdraws.load(Ordering::SeqCst), 1);
    assert!(n.settings.get().hidden);
    assert!(n.svc.status().hidden);

    n.svc.set_hidden(false).await.expect("show");
    assert_eq!(n.discovery.announcements().len(), 2);
    assert!(!n.settings.get().hidden);
}

#[tokio::test(start_paused = true)]
async fn the_display_name_is_validated_saved_and_announced() {
    let n = node(1, "sam");
    let _handle = n.svc.start().await;

    let err = n.svc.set_display_name("   ").await.expect_err("empty name");
    assert!(matches!(err, DomainError::InvalidInput(_)));
    let err = n
        .svc
        .set_display_name(&"x".repeat(41))
        .await
        .expect_err("name too long");
    assert!(matches!(err, DomainError::InvalidInput(_)));

    n.svc
        .set_display_name("  Dana Reyes ")
        .await
        .expect("valid name");
    assert_eq!(n.settings.get().display_name.as_deref(), Some("Dana Reyes"));
    let status = n.svc.status();
    assert_eq!(status.profile.display_name, "Dana Reyes");
    assert_eq!(status.profile.initials, "DR");
    let announced = n.discovery.announcements();
    assert_eq!(announced.len(), 2);
    assert_eq!(announced[1].profile.display_name, "Dana Reyes");
}

#[tokio::test(start_paused = true)]
async fn the_saved_display_name_wins_over_the_os_user_name() {
    let n = node(1, "sam");
    n.settings.set_name("Chosen Name");
    let _handle = n.svc.start().await;
    assert_eq!(n.svc.status().profile.display_name, "Chosen Name");
}

#[tokio::test(start_paused = true)]
async fn the_intro_card_is_marked_seen_and_saved() {
    let n = node(1, "sam");
    let _handle = n.svc.start().await;
    assert!(!n.svc.status().intro_seen);

    n.svc.mark_intro_seen().expect("mark");
    assert!(n.svc.status().intro_seen);
    assert!(n.settings.get().intro_seen);
}

#[tokio::test(start_paused = true)]
async fn damaged_files_become_warnings_and_blocked_yml_has_its_own_text() {
    let n = node(1, "sam");
    n.quarantine.set("blocked.yml");
    let _handle = n.svc.start().await;
    assert_eq!(
        n.svc.status().warnings,
        vec!["Your blocked devices list was damaged and has been reset.".to_string()]
    );

    let m = node(2, "mia");
    m.quarantine.set("paired.yml.corrupt-1730000000");
    let _handle = m.svc.start().await;
    assert_eq!(
        m.svc.status().warnings,
        vec!["paired.yml was damaged and has been reset.".to_string()]
    );
}

#[tokio::test(start_paused = true)]
async fn a_file_that_is_damaged_only_when_first_read_still_becomes_a_warning() {
    let n = node(1, "sam");
    n.quarantine.set_after_first_read("blocked.yml");
    let _handle = n.svc.start().await;
    assert_eq!(
        n.svc.status().warnings,
        vec!["Your blocked devices list was damaged and has been reset.".to_string()]
    );
}

// ----- discovery and the node list -----

#[tokio::test(start_paused = true)]
async fn a_verified_nearby_node_is_listed_with_its_hello_profile() {
    let a = node(1, "ann");
    a.connector
        .add_route(addr(2), Route::fixed(identity(2), "Ben Ortiz"));
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(2).device_id,
        profile: profile("A name from the TXT record"),
        addresses: vec![addr(2)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].display_name, "Ben Ortiz", "the verified hello wins");
    assert_eq!(rows[0].initials, "BO");
    assert_eq!(rows[0].group, NodeGroup::Nearby);
    assert_eq!(rows[0].status, NodeStatus::Available);
    assert_eq!(rows[0].source, Some(DiscoverySource::Mdns));
    assert_eq!(rows[0].short_id, identity(2).device_id.short());
    assert!(a.events.all().contains(&NetworkEvent::NodesChanged));
}

#[tokio::test(start_paused = true)]
async fn a_stranger_spoofing_an_mdns_id_is_never_listed() {
    let a = node(1, "ann");
    let victim = identity(9);
    // The address answers with the stranger's own key, not the victim's.
    a.connector
        .add_route(addr(8), Route::fixed(identity(8), "Mallory"));
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: victim.device_id,
        profile: profile("Victim"),
        addresses: vec![addr(8)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    assert!(a.svc.list_nodes().is_empty());
    assert_eq!(a.connector.hello_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_lying_hello_body_is_never_listed() {
    let a = node(1, "ann");
    let victim = identity(9);
    // The certificate key is the victim's, but the body names someone else.
    let mut route = Route::fixed(victim.clone(), "Victim");
    route.claims = Some(identity(8).device_id);
    a.connector.add_route(addr(9), route);
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: victim.device_id,
        profile: profile("Victim"),
        addresses: vec![addr(9)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    assert!(a.svc.list_nodes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_own_device_id_is_filtered_out() {
    let a = node(1, "ann");
    a.connector
        .add_route(a.addr(), Route::fixed(a.identity.clone(), "ann"));
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: a.id(),
        profile: a.profile(),
        addresses: vec![a.addr()],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    assert!(a.svc.list_nodes().is_empty());
    assert_eq!(a.connector.hello_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn sightings_on_two_addresses_merge_into_one_row() {
    let a = node(1, "ann");
    // The first address does not answer. The second and third carry the same key.
    a.connector
        .add_route(addr(21), Route::fixed(identity(2), "Ben"));
    a.connector
        .add_route(addr(22), Route::fixed(identity(2), "Ben"));
    let _handle = a.svc.start().await;
    let ben = identity(2).device_id;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: ben,
        profile: profile("Ben"),
        addresses: vec![addr(20), addr(21)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: ben,
        profile: profile("Ben"),
        addresses: vec![addr(22)],
        version: PROTOCOL_VERSION,
    });
    settle().await;

    assert_eq!(a.svc.list_nodes().len(), 1);
    let addresses = a.svc.mdns_addresses(&ben);
    assert!(addresses.contains(&addr(21)) && addresses.contains(&addr(22)));
    assert!(
        !addresses.contains(&addr(20)),
        "the dead address is not kept"
    );
}

#[tokio::test(start_paused = true)]
async fn a_goodbye_removes_the_node_at_once() {
    let a = node(1, "ann");
    a.connector
        .add_route(addr(2), Route::fixed(identity(2), "Ben"));
    let _handle = a.svc.start().await;
    let ben = identity(2).device_id;
    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: ben,
        profile: profile("Ben"),
        addresses: vec![addr(2)],
        version: PROTOCOL_VERSION,
    });
    settle().await;
    assert_eq!(a.svc.list_nodes().len(), 1);

    a.discovery.play(DiscoveryEvent::Gone { claimed_id: ben });
    settle().await;
    assert!(a.svc.list_nodes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_node_with_another_protocol_version_needs_an_update() {
    let a = node(1, "ann");
    let mut route = Route::fixed(identity(2), "Ben");
    route.version = PROTOCOL_VERSION + 1;
    a.connector.add_route(addr(2), route);
    let _handle = a.svc.start().await;

    a.discovery.play(DiscoveryEvent::Seen {
        claimed_id: identity(2).device_id,
        profile: profile("Ben"),
        addresses: vec![addr(2)],
        version: PROTOCOL_VERSION + 1,
    });
    settle().await;

    let rows = a.svc.list_nodes();
    assert_eq!(rows[0].status, NodeStatus::NeedsUpdate);
}

#[tokio::test(start_paused = true)]
async fn a_paired_device_is_listed_under_paired_and_a_blocked_one_is_hidden() {
    let a = node(1, "ann");
    a.paired.seed(identity(2).device_id, "Old Name");
    a.connector
        .add_route(addr(2), Route::fixed(identity(2), "New Name"));
    a.connector
        .add_route(addr(3), Route::fixed(identity(3), "Cy"));
    a.blocked.put(identity(3).device_id, "Cy");
    let _handle = a.svc.start().await;

    // Before it is seen, the paired row uses the name saved at pairing time.
    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].group, NodeGroup::Paired);
    assert_eq!(rows[0].display_name, "Old Name");
    assert_eq!(rows[0].status, NodeStatus::Offline);

    for (seed, name) in [(2u8, "New Name"), (3u8, "Cy")] {
        a.discovery.play(DiscoveryEvent::Seen {
            claimed_id: identity(seed).device_id,
            profile: profile(name),
            addresses: vec![addr(seed)],
            version: PROTOCOL_VERSION,
        });
    }
    settle().await;

    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1, "the blocked device is not offered");
    assert_eq!(
        rows[0].display_name, "New Name",
        "the live name is preferred"
    );
    assert_eq!(rows[0].group, NodeGroup::Paired);
}

// ----- manual addresses -----

#[tokio::test(start_paused = true)]
async fn adding_an_address_probes_it_at_once() {
    let a = node(1, "ann");
    a.connector
        .add_route(addr(5), Route::fixed(identity(5), "Pat"));
    let _handle = a.svc.start().await;

    a.svc.add_address("127.0.0.1", 47005).await.expect("add");

    let rows = a.svc.list_nodes();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source, Some(DiscoverySource::Manual));
    assert_eq!(rows[0].status, NodeStatus::Available);
    assert_eq!(a.svc.list_addresses().expect("list").len(), 1);
}

#[tokio::test(start_paused = true)]
async fn an_unreachable_address_is_probed_again_every_interval() {
    let a = node(1, "ann");
    let _handle = a.svc.start().await;
    a.svc.add_address("127.0.0.1", 47005).await.expect("add");
    assert!(a.svc.list_nodes().is_empty());

    a.connector
        .add_route(addr(5), Route::fixed(identity(5), "Pat"));
    tokio::time::advance(MANUAL_PROBE_INTERVAL + Duration::from_secs(1)).await;
    settle().await;

    assert_eq!(a.svc.list_nodes().len(), 1);

    // When it goes away, the next round removes it.
    a.connector.remove_route(addr(5));
    tokio::time::advance(MANUAL_PROBE_INTERVAL + Duration::from_secs(1)).await;
    settle().await;
    assert!(a.svc.list_nodes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn bad_addresses_are_refused_and_removing_one_forgets_it() {
    let a = node(1, "ann");
    let _handle = a.svc.start().await;

    for (host, port) in [
        ("", 47821u16),
        ("   ", 47821),
        ("a b", 47821),
        ("host/x", 47821),
        ("h", 0),
    ] {
        let err = a
            .svc
            .add_address(host, port)
            .await
            .expect_err("bad address");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{host:?}");
    }
    assert!(a.svc.list_addresses().expect("list").is_empty());

    a.svc.add_address("Team.Example", 47821).await.expect("add");
    a.svc.remove_address("team.example", 47821).expect("remove");
    assert!(a.svc.list_addresses().expect("list").is_empty());
}

// ----- stopping -----

#[tokio::test(start_paused = true)]
async fn stopping_the_handle_stops_the_server_and_discovery_once() {
    let a = node(1, "ann");
    let handle = a.svc.start().await;

    handle.stop();
    handle.stop();
    assert_eq!(a.server.stops.load(Ordering::SeqCst), 1);
    assert_eq!(a.discovery.stops.load(Ordering::SeqCst), 1);

    drop(handle);
    assert_eq!(
        a.server.stops.load(Ordering::SeqCst),
        1,
        "drop does not stop twice"
    );
}

#[tokio::test(start_paused = true)]
async fn dropping_the_handle_stops_everything() {
    let a = node(1, "ann");
    let handle = a.svc.start().await;
    drop(handle);
    assert_eq!(a.server.stops.load(Ordering::SeqCst), 1);
    assert_eq!(a.discovery.stops.load(Ordering::SeqCst), 1);
}

#[test]
fn the_limit_message_is_the_spec_text() {
    assert_eq!(
        super::paired_limit_message(),
        "You've reached the limit of 200 paired devices. Remove one to pair."
    );
}

#[tokio::test(start_paused = true)]
async fn blocked_devices_can_be_listed_and_unblocked() {
    let a = node(1, "ann");
    let _handle = a.svc.start().await;
    let id = DeviceId::from_bytes([4; 32]);
    a.blocked.put(id, "Dee");

    assert_eq!(a.svc.list_blocked().expect("list").len(), 1);
    let before = a.events.all().len();
    a.svc.unblock(&id).expect("unblock");
    assert!(a.svc.list_blocked().expect("list").is_empty());
    let after = a.events.all()[before..].to_vec();
    assert!(
        after.contains(&NetworkEvent::NodesChanged),
        "the panel refetches"
    );
}
