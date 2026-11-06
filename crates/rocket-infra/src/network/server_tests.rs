use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hyper::Method;
use rocket_network::limits::{
    DEFAULT_PORT, HEADER_READ_TIMEOUT, HELLO_PER_IP_PER_MINUTE, MAX_CONNECTIONS,
    MAX_REST_BODY_BYTES, MAX_UNPAIRED_CONNECTIONS, PAIR_REQUESTS_PER_MINUTE, PORT_RANGE_END,
    PROTOCOL_VERSION,
};
use rocket_network::{
    DecisionRequest, Nonce, NonceMessage, PairRequest, PairingId, PairingStatus, PeerCallError,
    PeerConnector, PeerHandler, PeerServerControl,
};

use super::{is_valid_pairing_id, FixedWindow, RustlsPeerServer, ServerOptions};
use crate::network::client::{send_request, tls_connect};
use crate::network::peer_test_support::{
    commitment_text, connector, err_of, identity, profile, reveal_nonce, start_node,
    start_node_with, FakeHandler,
};

fn pair_request() -> PairRequest {
    PairRequest {
        version: PROTOCOL_VERSION,
        profile: profile("Bob"),
    }
}

fn pairing(id: &str) -> PairingId {
    PairingId(id.to_string())
}

#[tokio::test]
async fn hello_returns_the_server_identity_and_profile() {
    let node = start_node("Alice").await;
    let client = connector(&identity());

    let (peer, hello) = client.hello(node.addr).await.expect("hello");

    assert_eq!(
        peer.device_id,
        node.id(),
        "the key comes from the handshake"
    );
    assert_eq!(hello.device_id, node.id());
    assert_eq!(hello.version, PROTOCOL_VERSION);
    assert_eq!(hello.profile, profile("Alice"));
    assert_eq!(node.handler.calls(), vec!["hello".to_string()]);
}

#[tokio::test]
async fn a_stranger_reaches_hello_and_every_pairing_route() {
    let node = start_node("Alice").await;
    let stranger = identity();
    let client = connector(&stranger);
    let id = pairing("p-1");

    client.hello(node.addr).await.expect("hello");
    let (peer, started) = client
        .pair_request(node.addr, &node.id(), &pair_request())
        .await
        .expect("pair request");
    assert_eq!(peer.device_id, node.id());
    assert_eq!(started.pairing_id, id);
    assert_eq!(started.commitment, commitment_text());

    let na = NonceMessage {
        nonce: Nonce([1u8; 32]).to_base64(),
    };
    let reveal = client
        .pair_nonce(node.addr, &node.id(), &id, &na)
        .await
        .expect("nonce");
    assert_eq!(reveal.nonce, reveal_nonce());
    client
        .pair_decision(
            node.addr,
            &node.id(),
            &id,
            &DecisionRequest { accept: true },
        )
        .await
        .expect("decision");
    let status = client
        .pair_status(node.addr, &node.id(), &id)
        .await
        .expect("status");
    assert_eq!(status.status, PairingStatus::Pending);

    let short = stranger.local.device_id.short();
    assert_eq!(
        node.handler.calls(),
        vec![
            "hello".to_string(),
            format!("pair_request:{short}:127.0.0.1:Bob"),
            format!("pair_nonce:{short}:p-1"),
            format!("pair_decision:{short}:p-1:true"),
            format!("pair_status:{short}:p-1"),
        ],
        "every route sees the caller's key and address"
    );
}

#[tokio::test]
async fn handler_refusals_map_to_their_status() {
    let node = start_node("Alice").await;
    let client = connector(&identity());

    let missing = client
        .pair_status(node.addr, &node.id(), &pairing("missing"))
        .await;
    assert_eq!(err_of(missing), PeerCallError::Rejected(404));
    let not_paired = client
        .pair_status(node.addr, &node.id(), &pairing("not-paired"))
        .await;
    assert_eq!(err_of(not_paired), PeerCallError::NotPaired);
    let busy = client
        .pair_status(node.addr, &node.id(), &pairing("busy"))
        .await;
    assert_eq!(err_of(busy), PeerCallError::Busy);
    let forbidden = client
        .pair_status(node.addr, &node.id(), &pairing("forbidden"))
        .await;
    assert_eq!(err_of(forbidden), PeerCallError::Rejected(403));
}

#[tokio::test]
async fn a_bad_pairing_id_is_refused_on_both_sides() {
    let node = start_node("Alice").await;
    let client_tls = identity();

    // The connector never sends an id that could change the path.
    let result = connector(&client_tls)
        .pair_status(node.addr, &node.id(), &pairing("a/b"))
        .await;
    assert!(matches!(err_of(result), PeerCallError::Protocol(_)));

    // The server refuses a raw request with an over-long id before the handler runs.
    let long = "a".repeat(65);
    let reply = send_request(
        &client_tls,
        node.addr,
        Some(node.id()),
        Method::GET,
        &format!("/pair/{long}"),
        None,
        &[],
    )
    .await
    .expect("reply");
    assert_eq!(reply.status, 400);
    assert!(node.handler.calls().is_empty());
}

#[tokio::test]
async fn an_invalid_profile_in_a_pair_request_is_400() {
    let node = start_node("Alice").await;
    let client_tls = identity();
    let body =
        br#"{"version":1,"profile":{"display_name":"bad\u0007name","initials":"B"}}"#.to_vec();

    let reply = send_request(
        &client_tls,
        node.addr,
        Some(node.id()),
        Method::POST,
        "/pair/request",
        Some(body),
        &[],
    )
    .await
    .expect("reply");

    assert_eq!(reply.status, 400);
    assert!(node.handler.calls().is_empty());
}

#[tokio::test]
async fn an_unknown_path_is_404() {
    let node = start_node("Alice").await;
    let client_tls = identity();

    for path in [
        "/",
        "/pair",
        "/hello/extra",
        "/collections",
        "/pair/p-1/other",
    ] {
        let reply = send_request(
            &client_tls,
            node.addr,
            Some(node.id()),
            Method::GET,
            path,
            None,
            &[],
        )
        .await
        .expect("reply");
        assert_eq!(reply.status, 404, "{path}");
        assert_eq!(reply.body.as_ref(), b"not-found");
    }
    assert!(node.handler.calls().is_empty());
}

#[tokio::test]
async fn an_origin_header_is_refused_before_any_route() {
    let node = start_node("Alice").await;
    let client_tls = identity();
    let body = serde_json::to_vec(&pair_request()).expect("encode");

    let cases = [
        (Method::GET, "/hello", None),
        (Method::POST, "/pair/request", Some(body)),
        (Method::GET, "/nope", None),
    ];
    for (method, path, body) in cases {
        let reply = send_request(
            &client_tls,
            node.addr,
            Some(node.id()),
            method,
            path,
            body,
            &[("origin", "http://evil.example")],
        )
        .await
        .expect("reply");
        assert_eq!(reply.status, 403, "{path}");
        assert_eq!(reply.body.as_ref(), b"forbidden");
    }
    assert!(
        node.handler.calls().is_empty(),
        "no handler runs for a browser request"
    );
}

#[tokio::test]
async fn the_sixth_pairing_request_in_a_minute_is_busy() {
    let node = start_node("Hub").await;
    let first = connector(&identity());

    for i in 0..PAIR_REQUESTS_PER_MINUTE {
        if let Err(e) = first
            .pair_request(node.addr, &node.id(), &pair_request())
            .await
        {
            panic!("request {i} must pass: {e:?}");
        }
    }
    let sixth = first
        .pair_request(node.addr, &node.id(), &pair_request())
        .await;
    assert_eq!(err_of(sixth), PeerCallError::Busy);

    // The limit is shared by every stranger.
    let second = connector(&identity());
    let other = second
        .pair_request(node.addr, &node.id(), &pair_request())
        .await;
    assert_eq!(err_of(other), PeerCallError::Busy);
    assert_eq!(
        node.handler.calls().len(),
        PAIR_REQUESTS_PER_MINUTE as usize,
        "a busy request never reaches the handler, so it shows no notice"
    );
}

#[tokio::test]
async fn the_thirty_first_hello_from_one_address_is_429() {
    let node = start_node("Hub").await;
    let client = connector(&identity());

    for i in 0..HELLO_PER_IP_PER_MINUTE {
        if let Err(e) = client.hello(node.addr).await {
            panic!("hello {i} must pass: {e:?}");
        }
    }
    assert_eq!(
        err_of(client.hello(node.addr).await),
        PeerCallError::Rejected(429)
    );
    // The limit is per IP, not per key.
    assert_eq!(
        err_of(connector(&identity()).hello(node.addr).await),
        PeerCallError::Rejected(429)
    );
    // Pairing has its own limit.
    client
        .pair_request(node.addr, &node.id(), &pair_request())
        .await
        .expect("pairing is not limited by hello");
}

#[tokio::test]
async fn a_body_over_16_kib_is_413_and_the_limit_itself_passes() {
    let node = start_node("Hub").await;
    let client_tls = identity();

    let mut big = serde_json::to_vec(&pair_request()).expect("encode");
    big.resize(MAX_REST_BODY_BYTES + 1, b' ');
    let result = send_request(
        &client_tls,
        node.addr,
        Some(node.id()),
        Method::POST,
        "/pair/request",
        Some(big),
        &[],
    )
    .await;
    match result {
        Ok(reply) => assert_eq!(reply.status, 413),
        // The server may close before it reads the whole body. The reset can then reach the
        // client before the 413 does.
        Err(PeerCallError::Unreachable(_)) => {}
        Err(other) => panic!("unexpected error: {other:?}"),
    }
    assert!(node.handler.calls().is_empty());

    // Trailing spaces are valid JSON, so a body of exactly the limit is parsed and served.
    let mut exact = serde_json::to_vec(&pair_request()).expect("encode");
    exact.resize(MAX_REST_BODY_BYTES, b' ');
    let reply = send_request(
        &client_tls,
        node.addr,
        Some(node.id()),
        Method::POST,
        "/pair/request",
        Some(exact),
        &[],
    )
    .await
    .expect("reply");
    assert_eq!(reply.status, 200);
}

#[tokio::test]
async fn a_wrong_expected_id_is_an_identity_mismatch_and_sends_nothing() {
    let node = start_node("Hub").await;
    let someone_else = identity().local.device_id;
    let client = connector(&identity());

    let request = client
        .pair_request(node.addr, &someone_else, &pair_request())
        .await;
    assert_eq!(err_of(request), PeerCallError::IdentityMismatch);
    let status = client
        .pair_status(node.addr, &someone_else, &pairing("p-1"))
        .await;
    assert_eq!(err_of(status), PeerCallError::IdentityMismatch);
    assert!(node.handler.calls().is_empty());
}

#[tokio::test]
async fn a_closed_port_is_unreachable() {
    let free = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = free.local_addr().expect("addr");
    drop(free);

    let result = connector(&identity()).hello(addr).await;
    assert!(matches!(err_of(result), PeerCallError::Unreachable(_)));
}

#[tokio::test]
async fn stop_closes_the_port_and_start_serves_again() {
    let node = start_node("Hub").await;
    node.server.stop();
    node.server.stop();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = connector(&identity());
    assert!(matches!(
        err_of(client.hello(node.addr).await),
        PeerCallError::Unreachable(_)
    ));
    let rebound = tokio::net::TcpListener::bind(node.addr).await;
    assert!(rebound.is_ok(), "the port is free after stop");
    drop(rebound);

    let as_handler: Arc<dyn PeerHandler> = node.handler.clone();
    let port = node.server.start(as_handler).await.expect("restart");
    let (peer, _) = client
        .hello(SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .expect("hello after the restart");
    assert_eq!(peer.device_id, node.id());
}

/// A held port whose next port is free, so a two-port range must use the second.
fn held_port_with_a_free_neighbour() -> (std::net::TcpListener, u16) {
    for _ in 0..50 {
        let held = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = held.local_addr().expect("addr").port();
        if port < u16::MAX && std::net::TcpListener::bind(("127.0.0.1", port + 1)).is_ok() {
            return (held, port);
        }
    }
    panic!("no free neighbouring port found");
}

#[tokio::test]
async fn a_taken_first_port_falls_back_to_the_next() {
    let (_held, taken) = held_port_with_a_free_neighbour();
    let options = ServerOptions {
        ports: taken..=taken + 1,
        ..ServerOptions::local_test()
    };

    let node = start_node_with("Hub", options).await;

    assert_eq!(node.addr.port(), taken + 1);
}

#[tokio::test]
async fn a_range_with_no_free_port_is_an_error() {
    let held = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let taken = held.local_addr().expect("addr").port();
    let tls = identity();
    let (handler, _sockets) = FakeHandler::new(&tls, "Hub");
    let server = RustlsPeerServer::with_options(
        Arc::clone(&tls),
        ServerOptions {
            ports: taken..=taken,
            ..ServerOptions::local_test()
        },
    );

    let as_handler: Arc<dyn PeerHandler> = handler;
    assert!(server.start(as_handler).await.is_err());
}

#[tokio::test]
async fn the_default_range_skips_a_taken_default_port() {
    // Either this test holds 47821 or another program already does. Both mean it is taken.
    let _held = tokio::net::TcpListener::bind(("127.0.0.1", DEFAULT_PORT)).await;
    let options = ServerOptions {
        ports: DEFAULT_PORT..=PORT_RANGE_END,
        ..ServerOptions::local_test()
    };

    let node = start_node_with("Hub", options).await;

    let port = node.addr.port();
    assert!(port > DEFAULT_PORT && port <= PORT_RANGE_END, "port {port}");
}

#[test]
fn production_options_use_the_spec_values() {
    let options = ServerOptions::default();
    assert_eq!(options.bind_ip, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    assert_eq!(options.ports, DEFAULT_PORT..=PORT_RANGE_END);
    assert_eq!(options.max_connections, MAX_CONNECTIONS);
    assert_eq!(options.max_unpaired, MAX_UNPAIRED_CONNECTIONS);
    assert_eq!(options.handshake_timeout, Duration::from_secs(10));
    assert_eq!(options.header_read_timeout, HEADER_READ_TIMEOUT);
    // The peer server and the callback server share one header timeout value.
    assert_eq!(
        HEADER_READ_TIMEOUT,
        crate::callback_server::HEADER_READ_TIMEOUT
    );
}

#[tokio::test]
async fn a_stalled_tls_handshake_frees_its_slot_after_the_timeout() {
    let options = ServerOptions {
        max_connections: 2,
        handshake_timeout: Duration::from_secs(1),
        ..ServerOptions::local_test()
    };
    let node = start_node_with("Hub", options).await;

    // Two clients connect and never start TLS.
    let _a = tokio::net::TcpStream::connect(node.addr)
        .await
        .expect("connect a");
    let _b = tokio::net::TcpStream::connect(node.addr)
        .await
        .expect("connect b");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = connector(&identity());
    assert!(
        matches!(
            err_of(client.hello(node.addr).await),
            PeerCallError::Unreachable(_)
        ),
        "both slots are held, so a third connection is dropped"
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    client
        .hello(node.addr)
        .await
        .expect("the stalled handshakes timed out and freed their slots");
}

#[tokio::test]
async fn a_connection_that_never_sends_headers_frees_its_slot_after_the_timeout() {
    let options = ServerOptions {
        max_connections: 1,
        header_read_timeout: Duration::from_secs(1),
        ..ServerOptions::local_test()
    };
    let node = start_node_with("Hub", options).await;

    // The TLS handshake finishes, then the client sends nothing.
    let (_silent, _) = tls_connect(&identity(), node.addr, Some(node.id()))
        .await
        .expect("handshake");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = connector(&identity());
    assert!(
        matches!(
            err_of(client.hello(node.addr).await),
            PeerCallError::Unreachable(_)
        ),
        "the only slot is held by the silent connection"
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    client
        .hello(node.addr)
        .await
        .expect("the header read timeout closed the silent connection and freed its slot");
}

#[tokio::test]
async fn unpaired_connections_stop_at_32_but_paired_devices_still_connect() {
    let node = start_node("Hub").await;
    let friend = identity();
    node.handler.pair(friend.local.device_id);
    let stranger = identity();

    let mut held = Vec::new();
    for i in 0..MAX_UNPAIRED_CONNECTIONS {
        match tls_connect(&stranger, node.addr, Some(node.id())).await {
            Ok((tls, _)) => held.push(tls),
            Err(e) => panic!("stranger connection {i}: {e:?}"),
        }
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    let extra = connector(&identity()).hello(node.addr).await;
    assert!(
        matches!(err_of(extra), PeerCallError::Unreachable(_)),
        "the 33rd unpaired connection is dropped after the handshake"
    );
    connector(&friend)
        .hello(node.addr)
        .await
        .expect("a paired device still gets in");

    held.pop();
    tokio::time::sleep(Duration::from_millis(300)).await;
    connector(&identity())
        .hello(node.addr)
        .await
        .expect("a freed unpaired slot serves again");
}

#[test]
fn fixed_window_allows_the_limit_then_refuses_until_the_window_passes() {
    let start = Instant::now();
    let mut window = FixedWindow::new(3, Duration::from_secs(60), 16);

    for _ in 0..3 {
        assert!(window.allow("a", start));
    }
    assert!(!window.allow("a", start + Duration::from_secs(59)));
    assert!(window.allow("b", start), "keys are counted apart");
    assert!(
        window.allow("a", start + Duration::from_secs(60)),
        "a new window starts"
    );
}

#[test]
fn fixed_window_keeps_at_most_max_keys_and_fails_closed() {
    let start = Instant::now();
    let mut window = FixedWindow::new(1, Duration::from_secs(60), 2);

    assert!(window.allow(1, start));
    assert!(window.allow(2, start));
    assert!(!window.allow(3, start), "a full table refuses new keys");
    assert!(
        window.allow(3, start + Duration::from_secs(61)),
        "expired keys are pruned"
    );
    assert!(window.len() <= 2);
}

#[test]
fn pairing_ids_are_limited_to_safe_characters() {
    assert!(is_valid_pairing_id("0f8fad5b-d9cb-469f-a165-70867728950e"));
    let long = "a".repeat(65);
    for bad in ["", "a/b", "..", "a b", "%2e", "p?x=1", long.as_str()] {
        assert!(!is_valid_pairing_id(bad), "{bad:?}");
    }
}
