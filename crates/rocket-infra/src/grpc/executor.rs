use std::str::FromStr;
use std::time::Instant;

use async_trait::async_trait;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use rocket_grpc::{
    json_to_message, message_to_json, GrpcCall, GrpcExecutor, GrpcStatus, GrpcUnaryResponse,
    ProtoRegistry,
};
use rocket_grpc::{GrpcStreamEvent, GrpcStreamHandle};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::codegen::http::uri::PathAndQuery;
use tonic::{Code, Request, Status};

use super::channel::{apply_metadata, connect, describe_error, pairs_from};
use super::codec::DynCodec;

/// Buffer between the network task and the UI for response events.
const EVENT_BUFFER: usize = 256;
/// Buffer for request messages the UI sends before the network takes them.
const OUTBOUND_BUFFER: usize = 64;

/// Runs gRPC calls over tonic with messages encoded at runtime.
pub struct TonicGrpcExecutor;

fn method_path(full_method: &str) -> DomainResult<PathAndQuery> {
    PathAndQuery::from_str(&format!("/{}", full_method.trim_start_matches('/'))).map_err(|e| {
        DomainError::InvalidInput(format!("'{full_method}' is not a valid method name: {e}"))
    })
}

fn status_of(status: &Status) -> GrpcStatus {
    GrpcStatus::new(status.code() as i32, status.message())
}

fn deadline_exceeded() -> GrpcStatus {
    GrpcStatus::new(
        Code::DeadlineExceeded as i32,
        "the deadline passed before the call finished",
    )
}

#[async_trait]
impl GrpcExecutor for TonicGrpcExecutor {
    async fn unary(
        &self,
        call: &GrpcCall,
        registry: &ProtoRegistry,
        request_json: &str,
    ) -> DomainResult<GrpcUnaryResponse> {
        let method = registry.method(&call.full_method)?;
        if method.is_client_streaming() || method.is_server_streaming() {
            return Err(DomainError::InvalidInput(format!(
                "{} is a streaming method, start a session instead",
                call.full_method
            )));
        }
        // Check the message before the network is touched.
        let request_message = json_to_message(&method.input(), request_json)?;
        let path = method_path(&call.full_method)?;
        let output = method.output();
        let channel = connect(call).await?;

        let started = Instant::now();
        let work = run_unary(channel, call, path, request_message, output);
        let (headers, message_json, status, trailers) = match call.timeout {
            Some(limit) => match tokio::time::timeout(limit, work).await {
                Ok(done) => done?,
                Err(_) => (vec![], None, deadline_exceeded(), vec![]),
            },
            None => work.await?,
        };
        Ok(GrpcUnaryResponse {
            headers,
            trailers,
            message_json,
            status,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    async fn open_stream(
        &self,
        call: &GrpcCall,
        registry: &ProtoRegistry,
        initial_json: Option<String>,
    ) -> DomainResult<GrpcStreamHandle> {
        let method = registry.method(&call.full_method)?;
        let client_streams = method.is_client_streaming();
        if !client_streams && !method.is_server_streaming() {
            return Err(DomainError::InvalidInput(format!(
                "{} is a unary method, send it as a normal call",
                call.full_method
            )));
        }
        let input = method.input();
        let initial = match (client_streams, initial_json) {
            (false, None) => {
                return Err(DomainError::InvalidInput(
                    "a server-streaming call needs one request message".into(),
                ))
            }
            (_, Some(json)) => Some(json_to_message(&input, &json)?),
            (true, None) => None,
        };
        let path = method_path(&call.full_method)?;
        let channel = connect(call).await?;

        let (events_tx, events_rx) = mpsc::channel(EVENT_BUFFER);
        let (outbound_tx, outbound_rx) = if client_streams {
            let (tx, rx) = mpsc::channel::<String>(OUTBOUND_BUFFER);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let driver = StreamDriver {
            channel,
            path,
            call: call.clone(),
            input,
            output: method.output(),
            initial,
            outbound: outbound_rx,
            events: events_tx,
        };
        let task = tokio::spawn(driver.run());
        Ok(GrpcStreamHandle {
            outbound: outbound_tx,
            events: events_rx,
            abort: task.abort_handle(),
        })
    }
}

type UnaryParts = (
    Vec<rocket_shared::grpc::GrpcMetadataPair>,
    Option<String>,
    GrpcStatus,
    Vec<rocket_shared::grpc::GrpcMetadataPair>,
);

/// Sends the request through the streaming API, which also exposes trailers.
async fn run_unary(
    channel: tonic::transport::Channel,
    call: &GrpcCall,
    path: PathAndQuery,
    message: DynamicMessage,
    output: MessageDescriptor,
) -> DomainResult<UnaryParts> {
    let mut client = tonic::client::Grpc::new(channel);
    client.ready().await.map_err(|e| {
        DomainError::Http(format!(
            "the connection is not ready: {}",
            describe_error(&e)
        ))
    })?;
    let mut request = Request::new(message);
    apply_metadata(request.metadata_mut(), &call.metadata)?;
    let response = match client
        .server_streaming(request, path, DynCodec::new(output))
        .await
    {
        Ok(response) => response,
        Err(status) => {
            return Ok((
                vec![],
                None,
                status_of(&status),
                pairs_from(status.metadata()),
            ));
        }
    };
    let (metadata, mut stream, _) = response.into_parts();
    let headers = pairs_from(&metadata);
    let mut message_json = None;
    match stream.message().await {
        Ok(Some(reply)) => message_json = Some(message_to_json(&reply)?),
        Ok(None) => {}
        Err(status) => {
            return Ok((
                headers,
                None,
                status_of(&status),
                pairs_from(status.metadata()),
            ));
        }
    }
    // Drain to the end so the trailers arrive and a second reply is noticed.
    match stream.message().await {
        Ok(None) => {}
        Ok(Some(_)) => {
            return Ok((
                headers,
                message_json,
                GrpcStatus::new(
                    Code::Internal as i32,
                    "the server sent more than one reply to a unary call",
                ),
                vec![],
            ));
        }
        Err(status) => {
            return Ok((
                headers,
                message_json,
                status_of(&status),
                pairs_from(status.metadata()),
            ));
        }
    }
    let trailers = match stream.trailers().await {
        Ok(Some(map)) => pairs_from(&map),
        _ => vec![],
    };
    Ok((headers, message_json, GrpcStatus::ok(), trailers))
}

struct StreamDriver {
    channel: tonic::transport::Channel,
    path: PathAndQuery,
    call: GrpcCall,
    input: MessageDescriptor,
    output: MessageDescriptor,
    initial: Option<DynamicMessage>,
    outbound: Option<mpsc::Receiver<String>>,
    events: mpsc::Sender<GrpcStreamEvent>,
}

impl StreamDriver {
    async fn run(self) {
        let events = self.events.clone();
        let limit = self.call.timeout;
        let work = self.drive();
        let finished = match limit {
            Some(limit) => tokio::time::timeout(limit, work).await.unwrap_or_else(|_| {
                Some(GrpcStreamEvent::Finished {
                    status: deadline_exceeded(),
                    trailers: vec![],
                })
            }),
            None => work.await,
        };
        if let Some(event) = finished {
            let _ = events.send(event).await;
        }
    }

    /// Returns the closing event, or `None` when the receiver went away first.
    async fn drive(self) -> Option<GrpcStreamEvent> {
        let StreamDriver {
            channel,
            path,
            call,
            input,
            output,
            initial,
            outbound,
            events,
        } = self;
        let mut client = tonic::client::Grpc::new(channel);
        if let Err(e) = client.ready().await {
            return Some(failed(Code::Unavailable, &describe_error(&e)));
        }
        // The request side is one stream for every shape. A server-streaming call
        // is a stream of one message, which is the same on the wire.
        let first = tokio_stream::iter(initial);
        let rest = outbound.map(|rx| {
            ReceiverStream::new(rx).map_while(move |json| json_to_message(&input, &json).ok())
        });
        let request_stream: std::pin::Pin<
            Box<dyn tokio_stream::Stream<Item = DynamicMessage> + Send>,
        > = match rest {
            Some(rest) => Box::pin(first.chain(rest)),
            None => Box::pin(first),
        };
        let mut request = Request::new(request_stream);
        if let Err(e) = apply_metadata(request.metadata_mut(), &call.metadata) {
            return Some(failed(Code::InvalidArgument, &e.to_string()));
        }
        let response = match client.streaming(request, path, DynCodec::new(output)).await {
            Ok(response) => response,
            Err(status) => {
                return Some(GrpcStreamEvent::Finished {
                    status: status_of(&status),
                    trailers: pairs_from(status.metadata()),
                })
            }
        };
        let (metadata, mut stream, _) = response.into_parts();
        if events
            .send(GrpcStreamEvent::Headers(pairs_from(&metadata)))
            .await
            .is_err()
        {
            return None;
        }
        loop {
            match stream.message().await {
                Ok(Some(message)) => {
                    let json = match message_to_json(&message) {
                        Ok(json) => json,
                        Err(e) => return Some(failed(Code::Internal, &e.to_string())),
                    };
                    if events.send(GrpcStreamEvent::Message(json)).await.is_err() {
                        return None;
                    }
                }
                Ok(None) => {
                    let trailers = match stream.trailers().await {
                        Ok(Some(map)) => pairs_from(&map),
                        _ => vec![],
                    };
                    return Some(GrpcStreamEvent::Finished {
                        status: GrpcStatus::ok(),
                        trailers,
                    });
                }
                Err(status) => {
                    return Some(GrpcStreamEvent::Finished {
                        status: status_of(&status),
                        trailers: pairs_from(status.metadata()),
                    })
                }
            }
        }
    }
}

fn failed(code: Code, message: &str) -> GrpcStreamEvent {
    GrpcStreamEvent::Finished {
        status: GrpcStatus::new(code as i32, message),
        trailers: vec![],
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rocket_shared::grpc::GrpcMetadataPair;

    use super::*;
    use crate::grpc::test_server::{start, Reflection, TestServer};

    const SAY_HELLO: &str = "demo.greeter.v1.Greeter/SayHello";
    const LIST: &str = "demo.greeter.v1.Greeter/ListGreetings";
    const COLLECT: &str = "demo.greeter.v1.Greeter/CollectNames";
    const CHAT: &str = "demo.greeter.v1.Greeter/Chat";

    fn call(server: &TestServer, method: &str) -> GrpcCall {
        GrpcCall {
            url: server.url(),
            full_method: method.to_string(),
            metadata: vec![],
            timeout: None,
            tls_ca_pem: None,
        }
    }

    fn name(value: &str) -> String {
        format!(r#"{{"name": "{value}"}}"#)
    }

    fn reply_message(response: &GrpcUnaryResponse) -> String {
        let json: serde_json::Value =
            serde_json::from_str(response.message_json.as_deref().expect("a reply")).expect("json");
        json["message"].as_str().expect("message field").to_string()
    }

    #[tokio::test]
    async fn a_unary_call_returns_the_reply_headers_and_an_ok_status() {
        let server = start(None, Reflection::None).await;
        let response = TonicGrpcExecutor
            .unary(&call(&server, SAY_HELLO), &server.registry, &name("ada"))
            .await
            .expect("call");
        assert_eq!(reply_message(&response), "hello ada");
        assert!(response.status.is_ok(), "{:?}", response.status);
        assert_eq!(response.status.code_name, "OK");
        assert!(
            response.headers.iter().any(|h| h.name == "content-type"),
            "{:?}",
            response.headers
        );
    }

    #[tokio::test]
    async fn metadata_reaches_the_server() {
        let server = start(None, Reflection::None).await;
        let mut c = call(&server, SAY_HELLO);
        c.metadata = vec![GrpcMetadataPair::new("X-Trace", "abc-123")];
        let response = TonicGrpcExecutor
            .unary(&c, &server.registry, &name("trace"))
            .await
            .expect("call");
        assert_eq!(reply_message(&response), "abc-123");
    }

    #[tokio::test]
    async fn an_error_status_is_a_response_with_its_trailers() {
        let server = start(None, Reflection::None).await;
        let response = TonicGrpcExecutor
            .unary(&call(&server, SAY_HELLO), &server.registry, &name("fail"))
            .await
            .expect("a failing status is not a transport error");
        assert_eq!(response.status.code, 5);
        assert_eq!(response.status.code_name, "NOT_FOUND");
        assert_eq!(response.status.message, "no such user");
        assert!(response.message_json.is_none());
        assert!(
            response
                .trailers
                .iter()
                .any(|t| t.name == "x-detail" && t.value == "gone"),
            "{:?}",
            response.trailers
        );
    }

    #[tokio::test]
    async fn the_deadline_turns_a_slow_call_into_deadline_exceeded() {
        let server = start(None, Reflection::None).await;
        let mut c = call(&server, SAY_HELLO);
        c.timeout = Some(Duration::from_millis(100));
        let response = TonicGrpcExecutor
            .unary(&c, &server.registry, &name("slow"))
            .await
            .expect("call");
        assert_eq!(response.status.code, 4);
        assert_eq!(response.status.code_name, "DEADLINE_EXCEEDED");
    }

    #[tokio::test]
    async fn a_bad_message_fails_before_any_connection_is_made() {
        let server = start(None, Reflection::None).await;
        let mut c = call(&server, SAY_HELLO);
        c.url = "127.0.0.1:1".into();
        let err = TonicGrpcExecutor
            .unary(&c, &server.registry, r#"{"nope": 1}"#)
            .await
            .expect_err("bad message");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
    }

    #[tokio::test]
    async fn an_unreachable_server_is_a_transport_error() {
        let server = start(None, Reflection::None).await;
        let mut c = call(&server, SAY_HELLO);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        c.url = format!("127.0.0.1:{}", listener.local_addr().expect("addr").port());
        drop(listener);
        let err = TonicGrpcExecutor
            .unary(&c, &server.registry, &name("x"))
            .await
            .expect_err("closed port");
        assert!(
            matches!(&err, DomainError::Http(m) if m.contains("could not connect")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn the_call_shape_must_match_the_entry_point() {
        let server = start(None, Reflection::None).await;
        let unary_on_stream = TonicGrpcExecutor
            .unary(&call(&server, LIST), &server.registry, &name("x"))
            .await
            .expect_err("streaming method");
        assert!(matches!(unary_on_stream, DomainError::InvalidInput(_)));
        let stream_on_unary = TonicGrpcExecutor
            .open_stream(&call(&server, SAY_HELLO), &server.registry, Some(name("x")))
            .await
            .err()
            .expect("unary method");
        assert!(matches!(stream_on_unary, DomainError::InvalidInput(_)));
        let unknown = TonicGrpcExecutor
            .unary(
                &call(&server, "demo.greeter.v1.Greeter/Nope"),
                &server.registry,
                "{}",
            )
            .await
            .expect_err("unknown method");
        assert!(matches!(unknown, DomainError::NotFound(_)));
    }

    fn self_signed() -> (String, String) {
        let key =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).expect("certificate");
        (key.cert.pem(), key.key_pair.serialize_pem())
    }

    #[tokio::test]
    async fn grpcs_works_with_a_trusted_ca_and_fails_without_one() {
        let (cert, key) = self_signed();
        let server = start(Some((cert.clone(), key)), Reflection::None).await;
        let mut c = call(&server, SAY_HELLO);
        c.url = format!("grpcs://localhost:{}", server.addr.port());

        c.tls_ca_pem = Some(cert);
        let response = TonicGrpcExecutor
            .unary(&c, &server.registry, &name("tls"))
            .await
            .expect("trusted call");
        assert_eq!(reply_message(&response), "hello tls");

        c.tls_ca_pem = None;
        let err = TonicGrpcExecutor
            .unary(&c, &server.registry, &name("tls"))
            .await
            .expect_err("untrusted certificate");
        assert!(
            matches!(&err, DomainError::Http(m) if m.contains("UnknownIssuer")),
            "{err:?}"
        );
    }

    async fn collect(handle: &mut GrpcStreamHandle) -> Vec<GrpcStreamEvent> {
        let mut out = Vec::new();
        while let Some(event) = handle.events.recv().await {
            let done = matches!(event, GrpcStreamEvent::Finished { .. });
            out.push(event);
            if done {
                break;
            }
        }
        out
    }

    fn messages(events: &[GrpcStreamEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                GrpcStreamEvent::Message(json) => {
                    let v: serde_json::Value = serde_json::from_str(json).expect("json");
                    v["message"].as_str().map(str::to_string)
                }
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_server_stream_delivers_headers_messages_then_finished() {
        let server = start(None, Reflection::None).await;
        let mut handle = TonicGrpcExecutor
            .open_stream(&call(&server, LIST), &server.registry, Some(name("n")))
            .await
            .expect("open");
        assert!(handle.outbound.is_none());
        let events = tokio::time::timeout(Duration::from_secs(5), collect(&mut handle))
            .await
            .expect("stream ends");
        assert!(
            matches!(events.first(), Some(GrpcStreamEvent::Headers(_))),
            "{events:?}"
        );
        assert_eq!(messages(&events), vec!["n-0", "n-1", "n-2"]);
        match events.last() {
            Some(GrpcStreamEvent::Finished { status, .. }) => assert!(status.is_ok()),
            other => panic!("expected Finished, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_server_stream_without_a_request_message_is_rejected() {
        let server = start(None, Reflection::None).await;
        let err = TonicGrpcExecutor
            .open_stream(&call(&server, LIST), &server.registry, None)
            .await
            .err()
            .expect("needs a message");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn a_client_stream_sends_every_message_and_ends_when_the_sender_drops() {
        let server = start(None, Reflection::None).await;
        let mut handle = TonicGrpcExecutor
            .open_stream(&call(&server, COLLECT), &server.registry, None)
            .await
            .expect("open");
        let outbound = handle
            .outbound
            .take()
            .expect("client streaming has a sender");
        for n in ["a", "b", "c"] {
            outbound.send(name(n)).await.expect("send");
        }
        drop(outbound);
        let events = tokio::time::timeout(Duration::from_secs(5), collect(&mut handle))
            .await
            .expect("stream ends");
        assert_eq!(messages(&events), vec!["a,b,c"]);
        assert!(
            matches!(events.last(), Some(GrpcStreamEvent::Finished { status, .. }) if status.is_ok())
        );
    }

    #[tokio::test]
    async fn a_bidi_stream_answers_each_message_while_it_stays_open() {
        let server = start(None, Reflection::None).await;
        let mut handle = TonicGrpcExecutor
            .open_stream(&call(&server, CHAT), &server.registry, None)
            .await
            .expect("open");
        let outbound = handle.outbound.clone().expect("bidi has a sender");
        let mut seen = Vec::new();
        for n in ["x", "y"] {
            outbound.send(name(n)).await.expect("send");
            loop {
                match tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
                    .await
                    .expect("event in time")
                {
                    Some(GrpcStreamEvent::Message(json)) => {
                        seen.push(json);
                        break;
                    }
                    Some(GrpcStreamEvent::Headers(_)) => continue,
                    other => panic!("unexpected {other:?}"),
                }
            }
        }
        assert!(
            seen[0].contains("echo x") && seen[1].contains("echo y"),
            "{seen:?}"
        );
        handle.outbound = None;
        drop(outbound);
        let tail = tokio::time::timeout(Duration::from_secs(5), collect(&mut handle))
            .await
            .expect("stream ends after half-close");
        assert!(
            matches!(tail.last(), Some(GrpcStreamEvent::Finished { status, .. }) if status.is_ok())
        );
    }

    #[tokio::test]
    async fn aborting_the_task_stops_the_events() {
        let server = start(None, Reflection::None).await;
        let mut handle = TonicGrpcExecutor
            .open_stream(&call(&server, CHAT), &server.registry, None)
            .await
            .expect("open");
        handle.abort.abort();
        let next = tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
            .await
            .expect("channel closes");
        assert!(
            next.is_none(),
            "an aborted driver sends nothing more: {next:?}"
        );
    }

    #[tokio::test]
    async fn a_stream_deadline_ends_with_deadline_exceeded() {
        let server = start(None, Reflection::None).await;
        let mut c = call(&server, CHAT);
        c.timeout = Some(Duration::from_millis(150));
        let mut handle = TonicGrpcExecutor
            .open_stream(&c, &server.registry, None)
            .await
            .expect("open");
        let events = tokio::time::timeout(Duration::from_secs(5), collect(&mut handle))
            .await
            .expect("deadline ends the stream");
        match events.last() {
            Some(GrpcStreamEvent::Finished { status, .. }) => assert_eq!(status.code, 4),
            other => panic!("expected Finished, got {other:?}"),
        }
    }
}
