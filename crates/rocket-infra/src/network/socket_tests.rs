use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hyper::Method;
use rocket_network::limits::MAX_FRAME_BYTES;
use rocket_network::{PeerCallError, PeerConnector, PeerMessage, PeerServerControl, PeerSocket};
use tokio::task::JoinSet;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::network::client::{send_request, tls_connect, ClientTls};
use crate::network::peer_test_support::{
    connector, err_of, hello_for, identity, start_node, start_node_with, within, Node,
};
use crate::network::server::ServerOptions;
use crate::network::TlsIdentity;

async fn paired_hub() -> (Node, Arc<TlsIdentity>) {
    let node = start_node("Hub").await;
    let mate = identity();
    node.handler.pair(mate.local.device_id);
    (node, mate)
}

async fn open(node: &Node, mate: &Arc<TlsIdentity>, name: &str) -> PeerSocket {
    match connector(mate)
        .open_socket(node.addr, &node.id(), hello_for(name))
        .await
    {
        Ok(socket) => socket,
        Err(e) => panic!("open the socket: {e:?}"),
    }
}

async fn server_socket(node: &mut Node) -> PeerSocket {
    within(node.sockets.recv())
        .await
        .expect("the server hands over its socket")
}

/// A raw client that can send frames the real pump never sends.
async fn raw_socket(node: &Node, mate: &TlsIdentity) -> WebSocketStream<ClientTls> {
    let (tls, _) = tls_connect(mate, node.addr, Some(node.id()))
        .await
        .expect("tls");
    let (ws, _) = tokio_tungstenite::client_async(format!("wss://{}/peer", node.addr), tls)
        .await
        .expect("upgrade");
    ws
}

async fn raw_hello(ws: &mut WebSocketStream<ClientTls>) -> PeerMessage {
    ws.send(Message::text(hello_for("Raw").encode()))
        .await
        .expect("send hello");
    let frame = within(ws.next())
        .await
        .expect("a frame")
        .expect("a good frame");
    PeerMessage::decode(frame.to_text().expect("a text frame")).expect("a peer message")
}

async fn assert_raw_closed(ws: &mut WebSocketStream<ClientTls>) {
    loop {
        match within(ws.next()).await {
            None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
            Some(Ok(_)) => continue,
        }
    }
}

/// A paired raw client after both Hello frames, plus the server's side of the socket.
async fn raw_paired() -> (Node, WebSocketStream<ClientTls>, PeerSocket) {
    let (mut node, mate) = paired_hub().await;
    let mut ws = raw_socket(&node, &mate).await;
    assert_eq!(raw_hello(&mut ws).await, hello_for("Hub"));
    let accepted = server_socket(&mut node).await;
    assert_eq!(accepted.remote_hello, hello_for("Raw"));
    (node, ws, accepted)
}

#[tokio::test]
async fn paired_sockets_exchange_hello_frames_both_ways() {
    let (mut node, mate) = paired_hub().await;

    let socket = open(&node, &mate, "Bob").await;
    assert_eq!(socket.peer.device_id, node.id());
    assert_eq!(socket.dialed_by, mate.local.device_id, "we dialled");
    assert_eq!(socket.remote_hello, hello_for("Hub"));

    let accepted = server_socket(&mut node).await;
    assert_eq!(accepted.peer.device_id, mate.local.device_id);
    assert_eq!(
        accepted.dialed_by, mate.local.device_id,
        "the remote side dialled"
    );
    assert_eq!(accepted.remote_hello, hello_for("Bob"));
}

#[tokio::test]
async fn ping_and_pong_travel_through_the_channels() {
    let (mut node, mate) = paired_hub().await;
    let mut socket = open(&node, &mate, "Bob").await;
    let mut accepted = server_socket(&mut node).await;

    socket
        .outbound
        .send(PeerMessage::Ping)
        .await
        .expect("send ping");
    assert_eq!(
        within(accepted.inbound.recv()).await,
        Some(PeerMessage::Ping)
    );
    accepted
        .outbound
        .send(PeerMessage::Pong)
        .await
        .expect("send pong");
    assert_eq!(within(socket.inbound.recv()).await, Some(PeerMessage::Pong));
}

#[tokio::test]
async fn a_stranger_gets_403_not_paired_before_the_upgrade() {
    let mut node = start_node("Hub").await;
    let stranger = identity();

    let result = connector(&stranger)
        .open_socket(node.addr, &node.id(), hello_for("Eve"))
        .await;
    assert_eq!(err_of(result), PeerCallError::NotPaired);

    // A plain GET shows the body: the check runs before any upgrade header is looked at.
    let reply = send_request(
        &stranger,
        node.addr,
        Some(node.id()),
        Method::GET,
        "/peer",
        None,
        &[],
    )
    .await
    .expect("reply");
    assert_eq!(reply.status, 403);
    assert_eq!(reply.body.as_ref(), b"not-paired");
    assert!(
        node.sockets.try_recv().is_err(),
        "no socket reaches the handler"
    );
}

#[tokio::test]
async fn a_paired_get_without_upgrade_headers_is_400() {
    let (node, mate) = paired_hub().await;

    let reply = send_request(
        &mate,
        node.addr,
        Some(node.id()),
        Method::GET,
        "/peer",
        None,
        &[],
    )
    .await
    .expect("reply");

    assert_eq!(reply.status, 400);
}

#[tokio::test]
async fn an_origin_header_on_peer_is_refused_even_for_a_paired_key() {
    let (mut node, mate) = paired_hub().await;

    let reply = send_request(
        &mate,
        node.addr,
        Some(node.id()),
        Method::GET,
        "/peer",
        None,
        &[("origin", "http://evil.example")],
    )
    .await
    .expect("reply");

    // The Origin check runs before the paired check and before the upgrade.
    assert_eq!(reply.status, 403);
    assert_eq!(reply.body.as_ref(), b"forbidden");
    assert!(
        node.sockets.try_recv().is_err(),
        "no socket reaches the handler"
    );
}

#[tokio::test]
async fn a_frame_at_the_limit_is_read() {
    let (_node, mut ws, mut accepted) = raw_paired().await;
    // Trailing spaces are valid JSON, so this ping is exactly 64 KiB.
    let mut frame = PeerMessage::Ping.encode();
    frame.push_str(&" ".repeat(MAX_FRAME_BYTES - frame.len()));

    ws.send(Message::text(frame)).await.expect("send");

    assert_eq!(
        within(accepted.inbound.recv()).await,
        Some(PeerMessage::Ping)
    );
}

#[tokio::test]
async fn a_frame_over_64_kib_closes_the_socket() {
    let (_node, mut ws, mut accepted) = raw_paired().await;

    let _ = ws
        .send(Message::text("x".repeat(MAX_FRAME_BYTES + 1)))
        .await;

    assert_eq!(within(accepted.inbound.recv()).await, None);
    assert_raw_closed(&mut ws).await;
}

#[tokio::test]
async fn a_binary_frame_closes_the_socket() {
    let (_node, mut ws, mut accepted) = raw_paired().await;

    let _ = ws.send(Message::binary(vec![1u8, 2, 3])).await;

    assert_eq!(within(accepted.inbound.recv()).await, None);
    assert_raw_closed(&mut ws).await;
}

#[tokio::test]
async fn an_unknown_message_type_closes_the_socket() {
    let (_node, mut ws, mut accepted) = raw_paired().await;

    let _ = ws
        .send(Message::text(r#"{"type":"send_collection"}"#.to_string()))
        .await;

    assert_eq!(within(accepted.inbound.recv()).await, None);
    assert_raw_closed(&mut ws).await;
}

#[tokio::test]
async fn a_second_hello_closes_the_socket() {
    let (_node, mut ws, mut accepted) = raw_paired().await;

    let _ = ws.send(Message::text(hello_for("Raw").encode())).await;

    assert_eq!(within(accepted.inbound.recv()).await, None);
    assert_raw_closed(&mut ws).await;
}

#[tokio::test]
async fn a_frame_before_hello_closes_without_a_socket() {
    let (mut node, mate) = paired_hub().await;
    let mut ws = raw_socket(&node, &mate).await;

    ws.send(Message::text(PeerMessage::Ping.encode()))
        .await
        .expect("send a ping first");

    // The server sends its own Hello, then closes.
    assert_raw_closed(&mut ws).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        node.sockets.try_recv().is_err(),
        "the handler never sees this socket"
    );
}

#[tokio::test]
async fn dropping_the_outbound_sender_closes_the_peer_inbound() {
    let (mut node, mate) = paired_hub().await;
    let socket = open(&node, &mate, "Bob").await;
    let mut accepted = server_socket(&mut node).await;

    let PeerSocket {
        outbound,
        mut inbound,
        ..
    } = socket;
    drop(outbound);

    assert_eq!(
        within(accepted.inbound.recv()).await,
        None,
        "the hub sees the socket close"
    );
    assert_eq!(
        within(inbound.recv()).await,
        None,
        "our own inbound closes too"
    );
}

#[tokio::test]
async fn the_same_device_dialling_twice_gets_two_sockets() {
    let (mut node, mate) = paired_hub().await;
    let first = open(&node, &mate, "Bob").await;
    let second = open(&node, &mate, "Bob").await;
    let mut a = server_socket(&mut node).await;
    let mut b = server_socket(&mut node).await;

    for accepted in [&a, &b] {
        assert_eq!(accepted.peer.device_id, mate.local.device_id);
        assert_eq!(accepted.dialed_by, mate.local.device_id);
    }
    // Both work. Keeping one is Plan 04's rule (`keep_dialed_by`).
    first
        .outbound
        .send(PeerMessage::Ping)
        .await
        .expect("ping 1");
    second
        .outbound
        .send(PeerMessage::Ping)
        .await
        .expect("ping 2");
    assert_eq!(within(a.inbound.recv()).await, Some(PeerMessage::Ping));
    assert_eq!(within(b.inbound.recv()).await, Some(PeerMessage::Ping));
}

#[tokio::test]
async fn stop_closes_open_sockets() {
    let (mut node, mate) = paired_hub().await;
    let mut socket = open(&node, &mate, "Bob").await;
    let _accepted = server_socket(&mut node).await;

    node.server.stop();

    assert_eq!(within(socket.inbound.recv()).await, None);
}

#[tokio::test]
async fn an_open_socket_keeps_its_connection_slot() {
    let options = ServerOptions {
        max_connections: 2,
        ..ServerOptions::local_test()
    };
    let mut node = start_node_with("Hub", options).await;
    let mate = identity();
    node.handler.pair(mate.local.device_id);
    let first = open(&node, &mate, "Bob").await;
    let _second = open(&node, &mate, "Bob").await;
    let _a = server_socket(&mut node).await;
    let _b = server_socket(&mut node).await;

    // The HTTP connections of both upgrades are over, but each socket still holds its slot.
    let blocked = connector(&mate).hello(node.addr).await;
    assert!(matches!(err_of(blocked), PeerCallError::Unreachable(_)));

    drop(first);
    tokio::time::sleep(Duration::from_millis(300)).await;
    connector(&mate)
        .hello(node.addr)
        .await
        .expect("a closed socket frees its slot");
}

#[tokio::test]
async fn a_hub_holds_thirty_paired_sockets_at_once() {
    let mut node = start_node("Hub").await;
    let server_id = node.id();
    let mut mates = Vec::new();
    for _ in 0..30 {
        let mate = identity();
        node.handler.pair(mate.local.device_id);
        mates.push(mate);
    }

    let mut opening = JoinSet::new();
    for (i, mate) in mates.iter().enumerate() {
        let mate = Arc::clone(mate);
        let addr = node.addr;
        opening.spawn(async move {
            connector(&mate)
                .open_socket(addr, &server_id, hello_for(&format!("Mate {i}")))
                .await
        });
    }
    let mut sockets = Vec::new();
    while let Some(joined) = opening.join_next().await {
        match joined.expect("the task ran") {
            Ok(socket) => sockets.push(socket),
            Err(e) => panic!("a mate could not open its socket: {e:?}"),
        }
    }
    assert_eq!(sockets.len(), 30);

    let mut accepted = Vec::new();
    for _ in 0..30 {
        accepted.push(server_socket(&mut node).await);
    }
    for socket in &sockets {
        socket.outbound.send(PeerMessage::Ping).await.expect("ping");
    }
    for socket in &mut accepted {
        assert_eq!(within(socket.inbound.recv()).await, Some(PeerMessage::Ping));
    }
    let mut ids: Vec<_> = accepted.iter().map(|s| s.peer.device_id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 30, "one socket per teammate, all online at once");
}

/// A paired raw client that reads nothing after its Hello, and the server side of its socket
/// with the send path filled until the pump is stuck in a send.
async fn stuck_pump(
    node: &mut Node,
    mate: &TlsIdentity,
) -> (WebSocketStream<ClientTls>, PeerSocket) {
    let mut raw = raw_socket(node, mate).await;
    let _ = raw_hello(&mut raw).await;
    let accepted = server_socket(node).await;
    let mut full_in_a_row = 0;
    for _ in 0..5_000_000 {
        match accepted.outbound.try_send(PeerMessage::Ping) {
            Ok(()) => full_in_a_row = 0,
            Err(_) => {
                full_in_a_row += 1;
                if full_in_a_row > 20 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
    assert!(full_in_a_row > 20, "the pump should be stuck in a send");
    (raw, accepted)
}

#[tokio::test]
async fn a_peer_that_stops_reading_cannot_outlive_stop() {
    let (mut node, mate) = paired_hub().await;
    let (_raw, mut accepted) = stuck_pump(&mut node, &mate).await;

    node.server.stop();

    // The stuck send gives way to the stop signal, so the socket ends.
    assert_eq!(within(accepted.inbound.recv()).await, None);
}

#[tokio::test]
async fn a_stuck_send_frees_the_slot_after_the_send_timeout() {
    let options = ServerOptions {
        max_connections: 1,
        ..ServerOptions::local_test()
    };
    let mut node = start_node_with("Hub", options).await;
    let mate = identity();
    node.handler.pair(mate.local.device_id);
    let mate = Arc::new(mate);
    let (_raw, accepted) = stuck_pump(&mut node, &mate).await;
    // Plan 04 drops the socket when the peer misses its pings.
    drop(accepted);

    let mut freed = false;
    for _ in 0..40 {
        if connector(&mate).hello(node.addr).await.is_ok() {
            freed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(
        freed,
        "the stuck pump should give up and free the only slot"
    );
}

#[tokio::test]
async fn rest_replies_end_the_connection_and_a_pipelined_request_gets_nothing() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (node, mate) = paired_hub().await;
    let (mut tls, _) = tls_connect(&mate, node.addr, Some(node.id()))
        .await
        .expect("tls");
    let one = format!("GET /hello HTTP/1.1\r\nHost: {}\r\n\r\n", node.addr);
    tls.write_all(format!("{one}{one}").as_bytes())
        .await
        .expect("write both requests");

    let mut reply = Vec::new();
    // The server closes after the first reply, so the read reaches the end of the stream.
    let _ = within(tls.read_to_end(&mut reply)).await;
    let text = String::from_utf8_lossy(&reply).to_lowercase();
    assert_eq!(text.matches("http/1.1 200").count(), 1, "{text}");
    assert!(text.contains("connection: close"), "{text}");
}
