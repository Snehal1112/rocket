//! An in-process gRPC server for the transport tests. It serves the `Greeter`
//! service from `test-fixtures/grpc/greeter.proto`, so the tests also prove that
//! a registry loaded from disk can drive real calls.
//!
//! Behaviour is picked by the request `name`:
//! - `fail`: status NOT_FOUND with the trailer `x-detail: gone`.
//! - `slow`: replies after 2 seconds.
//! - `trace`: replies with the value of the `x-trace` metadata.
//! - `whoami`: replies with the value of the `authorization` metadata.
//! - anything else: `hello <name>`.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use prost_reflect::{DynamicMessage, Value};
use rocket_grpc::{ProtoLoader, ProtoRegistry};
use tokio::sync::oneshot;
use tonic::codegen::{http, BoxFuture, Service};
use tonic::{Request, Response, Status};

use super::codec::DynCodec;
use super::proto_reader::FsProtoLoader;

const SERVICE: &str = "demo.greeter.v1.Greeter";

pub(crate) fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/grpc")
}

pub(crate) fn fixture_registry() -> ProtoRegistry {
    FsProtoLoader
        .load(&fixture_dir().join("greeter.proto"), &[])
        .expect("fixture proto compiles")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reflection {
    None,
    V1,
    V1Alpha,
}

pub(crate) struct TestServer {
    pub addr: SocketAddr,
    pub registry: ProtoRegistry,
    _shutdown: oneshot::Sender<()>,
}

impl TestServer {
    pub(crate) fn url(&self) -> String {
        format!("127.0.0.1:{}", self.addr.port())
    }
}

/// Starts the server on a free port. `tls` is `(certificate pem, key pem)`.
pub(crate) async fn start(tls: Option<(String, String)>, reflection: Reflection) -> TestServer {
    let registry = fixture_registry();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let (shutdown, stop) = oneshot::channel::<()>();

    let mut builder = tonic::transport::Server::builder();
    if let Some((cert, key)) = tls {
        let config = tonic::transport::ServerTlsConfig::new()
            .identity(tonic::transport::Identity::from_pem(cert, key));
        builder = builder.tls_config(config).expect("server tls");
    }
    let mut router = builder.add_service(Greeter {
        registry: registry.clone(),
    });
    let set = prost_types::FileDescriptorSet {
        file: registry.pool().file_descriptor_protos().cloned().collect(),
    };
    match reflection {
        Reflection::None => {}
        Reflection::V1 => {
            let svc = tonic_reflection::server::Builder::configure()
                .register_file_descriptor_set(set)
                .build_v1()
                .expect("reflection v1");
            router = router.add_service(svc);
        }
        Reflection::V1Alpha => {
            let svc = tonic_reflection::server::Builder::configure()
                .register_file_descriptor_set(set)
                .build_v1alpha()
                .expect("reflection v1alpha");
            router = router.add_service(svc);
        }
    }
    tokio::spawn(async move {
        let _ = router
            .serve_with_incoming_shutdown(incoming, async {
                let _ = stop.await;
            })
            .await;
    });
    TestServer {
        addr,
        registry,
        _shutdown: shutdown,
    }
}

#[derive(Clone)]
struct Greeter {
    registry: ProtoRegistry,
}

impl tonic::server::NamedService for Greeter {
    const NAME: &'static str = SERVICE;
}

fn reply(registry: &ProtoRegistry, text: &str, sequence: i32) -> DynamicMessage {
    let desc = registry
        .pool()
        .get_message_by_name("demo.greeter.v1.HelloReply")
        .expect("HelloReply");
    let mut message = DynamicMessage::new(desc);
    message.set_field_by_name("message", Value::String(text.to_string()));
    message.set_field_by_name("sequence", Value::I32(sequence));
    message
}

fn name_of(message: &DynamicMessage) -> String {
    message
        .get_field_by_name("name")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn meta(request_meta: &tonic::metadata::MetadataMap, key: &str, default: &str) -> String {
    request_meta
        .get(key)
        .and_then(|v| v.to_str().ok())
        .unwrap_or(default)
        .to_string()
}

async fn say_hello(
    registry: &ProtoRegistry,
    request: Request<DynamicMessage>,
) -> Result<Response<DynamicMessage>, Status> {
    let name = name_of(request.get_ref());
    match name.as_str() {
        "fail" => {
            let mut status = Status::not_found("no such user");
            status
                .metadata_mut()
                .insert("x-detail", "gone".parse().expect("ascii"));
            Err(status)
        }
        "slow" => {
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok(Response::new(reply(registry, "late", 0)))
        }
        "trace" => Ok(Response::new(reply(
            registry,
            &meta(request.metadata(), "x-trace", "none"),
            0,
        ))),
        "whoami" => Ok(Response::new(reply(
            registry,
            &meta(request.metadata(), "authorization", "anonymous"),
            0,
        ))),
        other => Ok(Response::new(reply(registry, &format!("hello {other}"), 0))),
    }
}

type ReplyStream = Pin<Box<dyn Stream<Item = Result<DynamicMessage, Status>> + Send>>;

impl<B> Service<http::Request<B>> for Greeter
where
    B: tonic::codegen::Body + Send + 'static,
    B::Error: Into<tonic::codegen::StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::Body>;
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let registry = self.registry.clone();
        let path = req.uri().path().to_string();
        Box::pin(async move {
            let method = match registry.method(&path) {
                Ok(m) => m,
                Err(_) => {
                    let mut response = http::Response::new(tonic::body::Body::default());
                    let headers = response.headers_mut();
                    headers.insert(
                        Status::GRPC_STATUS,
                        (tonic::Code::Unimplemented as i32).into(),
                    );
                    headers.insert(
                        http::header::CONTENT_TYPE,
                        tonic::metadata::GRPC_CONTENT_TYPE,
                    );
                    return Ok(response);
                }
            };
            let mut grpc = tonic::server::Grpc::new(DynCodec::new(method.input()));
            let response = match method.name() {
                "SayHello" => {
                    struct S(ProtoRegistry);
                    impl tonic::server::UnaryService<DynamicMessage> for S {
                        type Response = DynamicMessage;
                        type Future = BoxFuture<Response<DynamicMessage>, Status>;
                        fn call(&mut self, r: Request<DynamicMessage>) -> Self::Future {
                            let registry = self.0.clone();
                            Box::pin(async move { say_hello(&registry, r).await })
                        }
                    }
                    grpc.unary(S(registry), req).await
                }
                "ListGreetings" => {
                    struct S(ProtoRegistry);
                    impl tonic::server::ServerStreamingService<DynamicMessage> for S {
                        type Response = DynamicMessage;
                        type ResponseStream = ReplyStream;
                        type Future = BoxFuture<Response<ReplyStream>, Status>;
                        fn call(&mut self, r: Request<DynamicMessage>) -> Self::Future {
                            let registry = self.0.clone();
                            Box::pin(async move {
                                let name = name_of(r.get_ref());
                                let items: Vec<Result<DynamicMessage, Status>> = (0..3)
                                    .map(|i| Ok(reply(&registry, &format!("{name}-{i}"), i)))
                                    .collect();
                                let stream: ReplyStream =
                                    Box::pin(futures_util::stream::iter(items));
                                Ok(Response::new(stream))
                            })
                        }
                    }
                    grpc.server_streaming(S(registry), req).await
                }
                "CollectNames" => {
                    struct S(ProtoRegistry);
                    impl tonic::server::ClientStreamingService<DynamicMessage> for S {
                        type Response = DynamicMessage;
                        type Future = BoxFuture<Response<DynamicMessage>, Status>;
                        fn call(
                            &mut self,
                            r: Request<tonic::Streaming<DynamicMessage>>,
                        ) -> Self::Future {
                            let registry = self.0.clone();
                            Box::pin(async move {
                                let mut stream = r.into_inner();
                                let mut names = Vec::new();
                                while let Some(message) = stream.next().await {
                                    names.push(name_of(&message?));
                                }
                                Ok(Response::new(reply(
                                    &registry,
                                    &names.join(","),
                                    names.len() as i32,
                                )))
                            })
                        }
                    }
                    grpc.client_streaming(S(registry), req).await
                }
                _ => {
                    struct S(ProtoRegistry);
                    impl tonic::server::StreamingService<DynamicMessage> for S {
                        type Response = DynamicMessage;
                        type ResponseStream = ReplyStream;
                        type Future = BoxFuture<Response<ReplyStream>, Status>;
                        fn call(
                            &mut self,
                            r: Request<tonic::Streaming<DynamicMessage>>,
                        ) -> Self::Future {
                            let registry = self.0.clone();
                            Box::pin(async move {
                                let mut count = 0;
                                let out = r.into_inner().map(move |m| {
                                    count += 1;
                                    m.map(|m| {
                                        reply(&registry, &format!("echo {}", name_of(&m)), count)
                                    })
                                });
                                let stream: ReplyStream = Box::pin(out);
                                Ok(Response::new(stream))
                            })
                        }
                    }
                    grpc.streaming(S(registry), req).await
                }
            };
            Ok(response)
        })
    }
}
