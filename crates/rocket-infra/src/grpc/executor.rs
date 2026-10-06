use std::str::FromStr;
use std::time::Instant;

use async_trait::async_trait;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use rocket_grpc::{
    json_to_message, message_to_json, GrpcCall, GrpcExecutor, GrpcStatus, GrpcUnaryResponse,
    ProtoRegistry,
};
use rocket_shared::error::{DomainError, DomainResult};
use tonic::codegen::http::uri::PathAndQuery;
use tonic::{Code, Request, Status};

use super::channel::{apply_metadata, connect, describe_error, pairs_from};
use super::codec::DynCodec;

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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rocket_shared::grpc::GrpcMetadataPair;

    use super::*;
    use crate::grpc::test_server::{start, Reflection, TestServer};

    const SAY_HELLO: &str = "demo.greeter.v1.Greeter/SayHello";
    const LIST: &str = "demo.greeter.v1.Greeter/ListGreetings";

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
}
