# Protocol parity, Plan 12: gRPC execution and streaming

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rocket can call a gRPC server. A unary call runs with metadata, TLS (`grpcs://`), a deadline, `{{variable}}` resolution and auth, and returns its reply, headers, trailers and status. Server-streaming, client-streaming and bidirectional calls run as sessions that push their messages and lifecycle to the frontend as Tauri events. A server that publishes gRPC reflection can be listed and called without a `.proto` file.

**Architecture:** `rocket-grpc` gains the `GrpcExecutor` trait and the call, status and stream types, like `rocket-http` has `HttpExecutor`. `rocket-infra` implements it as `TonicGrpcExecutor` over tonic with a custom codec that encodes and decodes `DynamicMessage` from descriptors known only at runtime, so there is no code generation. `rocket-app` gains `GrpcService`: it resolves variables and auth, finds descriptors (from the request's `.proto` file or from reflection, cached), runs unary calls and keeps the table of streaming sessions. Session output goes out as `DomainEvent::GrpcSession*` through the `EventPublisher` that `TauriEventBus` implements in production. Commands in `src-tauri` stay thin.

**Tech Stack:** Rust, `tonic 0.14.6` (client channel, rustls with `ring`, OS root certificates), `tonic-reflection 0.14.6` (client messages only), `prost`, `prost-reflect`, `tokio`, `tokio-stream`, `rcgen` for the TLS test certificate. Tests run an in-process tonic server that serves the fixture `greeter.proto` through the same dynamic codec. Run Rust tests with `cargo test -j4 -p <crate> <name>`.

**Spec:** `docs/superpowers/specs/opencollection-spec-reference.md` section 2.5 (with Plan 11's correction). Behaviour reference: https://docs.usebruno.com/send-requests/grpc/overview.

**Depends on:** Plan 11 (`GrpcRequest`, `ProtoRegistry`, `ProtoLoader`, `FsProtoLoader`, the codec and the fixture protos). The crates and versions are justified in Plan 11.

## Facts found while building this plan

The transport, codec, service and reflection code below was compiled and its tests were run in a scratch workspace that held copies of the real crates' sources. The Tauri wiring in `src-tauri` was written against the existing commands and was not built there, so `cargo check -j4 -p rocket` is the first real check of it.

- A tonic `Channel` is a buffered service. `tonic::client::Grpc::ready().await` must run before **every** call, or the call panics with `send_item called without first calling poll_reserve`. `run_unary` and the stream driver call it.
- `Request::set_timeout` only writes the `grpc-timeout` header. It does not stop the client from waiting. Rocket wraps the call in `tokio::time::timeout` and reports `DEADLINE_EXCEEDED` itself.
- tonic accepts bytes above `0x7e` in an ASCII metadata value. gRPC does not, so `apply_metadata` checks for printable ASCII itself.
- `Response<M>` from `Grpc::unary` has no trailers. Rocket sends unary calls through the streaming API instead (one request, one reply) and reads `Streaming::trailers()` after the last message. On the wire this is the same call.
- A reflection server may return only the file that defines a symbol and not its imports (tonic's own server does this). The client therefore asks for each missing import by file name until none is missing.
- `tonic-reflection`'s generated client (`pb::v1`, `pb::v1alpha`) is available without its `server` feature.
- tonic's default limit for a decoded message is 4 MiB.

## Behaviour decisions baked in

- URL schemes: `grpcs://` and `https://` use TLS, `grpc://`, `http://` and a bare `host:port` do not. Anything after the host is ignored, because the method is chosen separately.
- A non-OK gRPC status is a normal outcome. `unary` returns it in `GrpcUnaryResponse.status` with the trailers, so the UI can show it. Only a failure to connect, a bad message or a bad URL is an `Err`.
- The deadline covers the whole call, for unary and for streams. No deadline is set unless the caller passes one (Plan 13's tab uses 30 seconds for unary).
- TLS trusts the operating system's roots. `GrpcCall.tls_ca_pem` adds one more CA and is used by tests. Turning certificate verification off, and mutual TLS, are not supported (see Known limits).
- Auth becomes metadata: bearer and basic go in `authorization`, an API key placed in a header goes in its named header. Every other type fails the call with a message that names it. A call is never sent unauthenticated because its auth type is unsupported. A metadata line the user wrote with the same name wins over auth.
- An undefined `{{variable}}` in the URL, a metadata name or value, the message, an auth field or the proto path fails the call and names the variable. It is never sent as literal text. The request's own variables are the innermost scope.
- A relative proto path is resolved inside the collection and may not contain `..`. An absolute path or `~/` is used as given.
- Descriptors are cached: a `.proto` by path and modification time, reflection by URL and the hash of the call's metadata, so two credentials never share a result. `refresh` bypasses the cache.
- Streaming sessions are keyed by a generated id. Events: `grpc-session-started`, `grpc-session-headers`, `grpc-session-message`, `grpc-session-finished`. Event fields stay snake_case on the wire, like the agent session events.

## Global Constraints

- Never `unwrap()` in production paths. Tests may use `.expect("reason")`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill, conventional subjects, and stage by explicit path only. Peer sessions share this repo's git index. `crates/rocket-app/src/execution_service.rs` may carry unrelated local edits. This plan never edits or stages it.
- Commands stay thin: validate, call the service, map the error. `camelCase` serde only on IPC DTOs. The response types in `rocket-grpc` (`GrpcUnaryResponse`, `GrpcStatus`) are IPC view types and are never persisted.
- Do not log message bodies, metadata values or credentials. Errors name the metadata key, never its value.
- Only `rocket-infra` touches the network and the filesystem.

## How to read the diffs

Where a task changes a file an earlier task created, the change is shown as unified-diff hunks. The line numbers in the `@@` headers belong to the finished file, so match each hunk by its context lines, not its numbers. Hunks inside a `tests` module belong to the failing-test step. The rest belong to the implementation step.

## Review Focus

1. An undefined variable must fail the call by name and nothing may reach the server (Task 1 test `an_undefined_variable_fails_by_name_and_nothing_is_sent`). An unsupported auth type must fail instead of sending the call unauthenticated (Task 1 test `an_unsupported_auth_type_fails_instead_of_going_out_unauthenticated`).
2. A failing status is a response with its trailers, not an error, and a slow call becomes `DEADLINE_EXCEEDED` (Task 1 tests `an_error_status_is_a_response_with_its_trailers` and `the_deadline_turns_a_slow_call_into_deadline_exceeded`). A bad message must fail before any connection is made (`a_bad_message_fails_before_any_connection_is_made`).
3. Cancelling a session publishes `CANCELLED` exactly once even if the call ends at the same moment, and a message the descriptor rejects never reaches the call (Task 2 tests `cancelling_aborts_the_call_and_reports_cancelled_exactly_once` and `a_client_stream_message_is_resolved_validated_and_forwarded`).
4. Dropping the outbound sender must half-close a client or bidi call, and a call that stops without a closing event must still publish a final event (Task 2 tests `a_client_stream_sends_every_message_and_ends_when_the_sender_drops` and `a_call_that_stops_without_a_closing_event_is_reported_as_unknown`).
5. Reflection must fetch imports the server does not return, fall back to v1alpha, and never share a cached result between different credentials (Task 3 tests `a_reflected_registry_can_drive_a_real_call`, `a_v1alpha_only_server_is_reached_by_the_fallback` and `different_credentials_do_not_share_reflected_descriptors`).

---

## Task 1: Unary calls

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/src/lib.rs`; create `crates/rocket-shared/src/grpc.rs`
- Modify: `crates/rocket-grpc/Cargo.toml`, `src/lib.rs`; create `src/call.rs`
- Modify: `crates/rocket-infra/Cargo.toml`, `src/lib.rs`, `src/grpc/mod.rs`
- Create: `crates/rocket-infra/src/grpc/codec.rs`, `channel.rs`, `executor.rs`, `test_server.rs`
- Modify: `crates/rocket-app/Cargo.toml`, `src/lib.rs`; create `src/grpc_service.rs`
- Modify: `src-tauri/Cargo.toml`, `src/lib.rs`, `src/commands/mod.rs`; create `src/commands/grpc.rs`

**Interfaces:**
- Consumes: `ProtoRegistry`, `ProtoLoader`, `json_to_message`, `message_to_json` (Plan 11), `GrpcRequest`, `Auth`, `CollectionRepository::get_settings`, `rocket_environment::resolve`, `RequestExecutionService::{build_variable_context, resolve_external_secrets}`.
- Produces:
  - `rocket_shared::grpc::GrpcMetadataPair { name, value }`.
  - `rocket_grpc::{GrpcCall, GrpcStatus, GrpcUnaryResponse, GrpcExecutor, grpc_code_name}` with `GrpcExecutor::unary(&self, call: &GrpcCall, registry: &ProtoRegistry, request_json: &str) -> DomainResult<GrpcUnaryResponse>`.
  - `rocket_infra::TonicGrpcExecutor`.
  - `rocket_app::{GrpcService, GrpcExecuteInput}`: `GrpcService::new(executor: Arc<dyn GrpcExecutor>, proto_loader: Arc<dyn ProtoLoader>, collection_repo: Arc<dyn CollectionRepository>, workspace_path: Arc<Mutex<PathBuf>>) -> Self` and `async fn call_unary(&self, input: GrpcExecuteInput) -> DomainResult<GrpcUnaryResponse>`.
  - Tauri command `grpc_unary_call(input: GrpcExecuteDto) -> GrpcUnaryResponse`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Dependencies and the shared metadata pair**

`crates/rocket-grpc/Cargo.toml`, under `[dependencies]`:

```toml
async-trait.workspace = true
```

`crates/rocket-infra/Cargo.toml`, under `[dependencies]` (the TLS features give rustls with the `ring` provider and the OS root certificates):

```toml
prost = "0.14"
prost-reflect = { version = "0.16", features = ["serde"] }
prost-types = "0.14"
tonic = { version = "0.14.6", default-features = false, features = ["codegen", "channel", "tls-ring", "tls-native-roots"] }
```

and under `[dev-dependencies]` (the `server` and `router` features are only for the in-test server):

```toml
futures-util = "0.3"
rcgen = { version = "0.13", default-features = false, features = ["crypto", "ring", "pem"] }
tonic = { version = "0.14.6", default-features = false, features = ["codegen", "channel", "server", "router", "tls-ring"] }
tonic-reflection = { version = "0.14.6", default-features = false, features = ["server"] }
tokio-stream = { version = "0.1", features = ["net"] }
```

`crates/rocket-app/Cargo.toml` and `src-tauri/Cargo.toml`, under `[dependencies]`: `rocket-grpc.workspace = true`.

Create `crates/rocket-shared/src/grpc.rs` and add `pub mod grpc;` after `pub mod error;` in `crates/rocket-shared/src/lib.rs`:

```rust
use serde::{Deserialize, Serialize};

/// One metadata (header or trailer) line of a gRPC call, as shown to the user.
/// Binary values (`*-bin` names) are carried as base64 text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrpcMetadataPair {
    pub name: String,
    pub value: String,
}

impl GrpcMetadataPair {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}
```

- [ ] **Step 3: Write the failing `rocket-grpc` tests**

Create `crates/rocket-grpc/src/call.rs` with only this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_names_cover_the_canonical_range() {
        assert_eq!(grpc_code_name(0), "OK");
        assert_eq!(grpc_code_name(4), "DEADLINE_EXCEEDED");
        assert_eq!(grpc_code_name(5), "NOT_FOUND");
        assert_eq!(grpc_code_name(16), "UNAUTHENTICATED");
        assert_eq!(grpc_code_name(99), "UNKNOWN");
        assert_eq!(grpc_code_name(-1), "UNKNOWN");
    }

    #[test]
    fn status_new_fills_the_name_and_ok_is_ok() {
        let s = GrpcStatus::new(5, "no such user");
        assert_eq!(s.code_name, "NOT_FOUND");
        assert!(!s.is_ok());
        assert!(GrpcStatus::ok().is_ok());
    }

    #[test]
    fn executor_trait_is_object_safe() {
        fn _assert(_: Box<dyn GrpcExecutor>) {}
    }
}
```

In `crates/rocket-grpc/src/lib.rs`, make the module list and exports read:

```diff
@@ -2,12 +2,14 @@
 //! JSON to and from protobuf at runtime. It holds no network or file I/O; the
 //! concrete transport and file reader live in `rocket-infra`.
 
+pub mod call;
 pub mod codec;
 pub mod registry;
 
 #[cfg(test)]
 mod test_support;
 
+pub use call::{grpc_code_name, GrpcCall, GrpcExecutor, GrpcStatus, GrpcUnaryResponse};
 pub use codec::{empty_message_json, json_to_message, message_to_json};
 pub use prost_reflect::{DynamicMessage, MessageDescriptor, MethodDescriptor};
 pub use registry::{GrpcMethodInfo, GrpcServiceInfo, ProtoFileReader, ProtoLoader, ProtoRegistry};
```

- [ ] **Step 4: Run the tests to verify they fail, then implement the call types**

Run: `cargo test -j4 -p rocket-grpc call`
Expected: FAIL to compile (`GrpcStatus`, `grpc_code_name`, `GrpcExecutor` not defined).

Put this above the test module in `crates/rocket-grpc/src/call.rs`:

```rust
use std::time::Duration;

use async_trait::async_trait;
use rocket_shared::error::DomainResult;
use rocket_shared::grpc::GrpcMetadataPair;
use serde::Serialize;

use crate::registry::ProtoRegistry;

/// Everything the transport needs for one call. Variables are already resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct GrpcCall {
    /// `host:port`, `grpc://host:port`, `grpcs://host:port`, `http://` or `https://`.
    pub url: String,
    /// `package.Service/Method`.
    pub full_method: String,
    pub metadata: Vec<GrpcMetadataPair>,
    /// Deadline for the whole call. `None` means no deadline.
    pub timeout: Option<Duration>,
    /// Extra PEM CA certificate to trust for `grpcs://`. The system roots are always trusted.
    pub tls_ca_pem: Option<String>,
}

/// A gRPC status. `code` follows the canonical numbering, 0 is OK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcStatus {
    pub code: i32,
    pub code_name: String,
    pub message: String,
}

impl GrpcStatus {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            code_name: grpc_code_name(code).to_string(),
            message: message.into(),
        }
    }

    pub fn ok() -> Self {
        Self::new(0, "")
    }

    pub fn is_ok(&self) -> bool {
        self.code == 0
    }
}

/// The canonical name of a gRPC status code, `UNKNOWN` for numbers outside 0..=16.
pub fn grpc_code_name(code: i32) -> &'static str {
    match code {
        0 => "OK",
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => "UNKNOWN",
    }
}

/// The outcome of a unary call. A non-OK `status` is a normal outcome and is
/// returned here, not as an error, so the UI can show it with its trailers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcUnaryResponse {
    pub headers: Vec<GrpcMetadataPair>,
    pub trailers: Vec<GrpcMetadataPair>,
    pub message_json: Option<String>,
    pub status: GrpcStatus,
    pub duration_ms: u64,
}

/// Runs gRPC calls. Implemented by `TonicGrpcExecutor` in `rocket-infra`.
#[async_trait]
pub trait GrpcExecutor: Send + Sync {
    /// Runs a unary call. Fails before connecting when the method is not unary
    /// or `request_json` does not fit the request message.
    async fn unary(
        &self,
        call: &GrpcCall,
        registry: &ProtoRegistry,
        request_json: &str,
    ) -> DomainResult<GrpcUnaryResponse>;
}
```

Run: `cargo test -j4 -p rocket-grpc call`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the failing transport tests**

The tests run against a real in-process server, so the test support comes first. Create `crates/rocket-infra/src/grpc/codec.rs`. It is a tonic codec for messages known only at runtime, and the test server uses it as well as the client:

```rust
use prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::Status;

/// A tonic codec for protobuf messages known only at runtime. The encoder writes
/// any `DynamicMessage`. The decoder builds messages of one descriptor.
#[derive(Clone)]
pub(crate) struct DynCodec {
    decode: MessageDescriptor,
}

impl DynCodec {
    pub(crate) fn new(decode: MessageDescriptor) -> Self {
        Self { decode }
    }
}

pub(crate) struct DynEncoder;
pub(crate) struct DynDecoder(MessageDescriptor);

impl Codec for DynCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynEncoder;
    type Decoder = DynDecoder;

    fn encoder(&mut self) -> DynEncoder {
        DynEncoder
    }

    fn decoder(&mut self) -> DynDecoder {
        DynDecoder(self.decode.clone())
    }
}

impl Encoder for DynEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(&mut self, item: DynamicMessage, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        item.encode(dst)
            .map_err(|e| Status::internal(format!("could not encode message: {e}")))
    }
}

impl Decoder for DynDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<DynamicMessage>, Status> {
        DynamicMessage::decode(self.0.clone(), src)
            .map(Some)
            .map_err(|e| Status::internal(format!("could not decode message: {e}")))
    }
}
```

Create `crates/rocket-infra/src/grpc/test_server.rs`. It serves the fixture `greeter.proto` from `test-fixtures/grpc` (Plan 11), so these tests also prove that a registry loaded from disk drives real calls. It already contains the streaming and reflection behaviour that Tasks 2 and 3 test, so those tasks add no server code:

```rust
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

#[allow(dead_code)] // V1 and V1Alpha are used by the reflection tests.
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
```

Replace `crates/rocket-infra/src/grpc/mod.rs` with:

```diff
@@ -1,6 +1,13 @@
 //! gRPC transport and `.proto` file access.
 
+mod channel;
+mod codec;
+mod executor;
 mod proto_reader;
 
+#[cfg(test)]
+mod test_server;
+
+pub use executor::TonicGrpcExecutor;
 pub use proto_reader::{FsProtoFileReader, FsProtoLoader};
 
```

and make the re-export in `crates/rocket-infra/src/lib.rs` read:

```rust
pub use grpc::{FsProtoFileReader, FsProtoLoader, TonicGrpcExecutor};
```

Create `crates/rocket-infra/src/grpc/channel.rs` with only this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_decide_tls() {
        let cases = [
            ("localhost:50051", "http://localhost:50051", false),
            ("grpc://localhost:50051", "http://localhost:50051", false),
            ("http://localhost:50051", "http://localhost:50051", false),
            ("grpcs://api.example.com", "https://api.example.com", true),
            (
                "HTTPS://api.example.com:8443/x/y",
                "https://api.example.com:8443",
                true,
            ),
            ("  grpc://h:1/  ", "http://h:1", false),
        ];
        for (input, uri, tls) in cases {
            let t = parse_target(input).unwrap_or_else(|e| panic!("{input}: {e}"));
            assert_eq!((t.uri.as_str(), t.tls), (uri, tls), "{input}");
        }
    }

    #[test]
    fn bad_urls_are_invalid_input() {
        for input in ["", "   ", "ftp://host:1", "grpc://", "grpc:// host:1"] {
            assert!(
                matches!(parse_target(input), Err(DomainError::InvalidInput(_))),
                "{input:?}"
            );
        }
    }

    #[test]
    fn metadata_names_are_lowercased_and_empty_names_skipped() {
        let mut map = MetadataMap::new();
        apply_metadata(
            &mut map,
            &[
                GrpcMetadataPair::new("X-Trace-Id", "abc"),
                GrpcMetadataPair::new("  ", "ignored"),
            ],
        )
        .expect("apply");
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get("x-trace-id").and_then(|v| v.to_str().ok()),
            Some("abc")
        );
    }

    #[test]
    fn reserved_names_are_rejected() {
        for name in ["content-type", "TE", "grpc-timeout", "user-agent"] {
            let mut map = MetadataMap::new();
            let err =
                apply_metadata(&mut map, &[GrpcMetadataPair::new(name, "x")]).expect_err(name);
            assert!(matches!(err, DomainError::InvalidInput(_)), "{name}");
        }
    }

    #[test]
    fn binary_metadata_takes_base64_and_reads_back_as_base64() {
        let mut map = MetadataMap::new();
        apply_metadata(&mut map, &[GrpcMetadataPair::new("trace-bin", "aGk=")]).expect("apply");
        let pairs = pairs_from(&map);
        assert_eq!(pairs, vec![GrpcMetadataPair::new("trace-bin", "aGk=")]);
        let err = apply_metadata(&mut map, &[GrpcMetadataPair::new("x-bin", "!!")])
            .expect_err("bad base64");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn a_non_ascii_value_is_rejected_by_name() {
        let mut map = MetadataMap::new();
        let err = apply_metadata(&mut map, &[GrpcMetadataPair::new("x-name", "caf\u{e9}")])
            .expect_err("non ascii");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("x-name")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn connecting_to_a_closed_port_names_the_address() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let call = GrpcCall {
            url: format!("127.0.0.1:{port}"),
            full_method: "a.B/C".into(),
            metadata: vec![],
            timeout: None,
            tls_ca_pem: None,
        };
        let err = connect(&call).await.expect_err("closed port");
        assert!(
            matches!(&err, DomainError::Http(m) if m.contains(&format!("127.0.0.1:{port}"))),
            "{err:?}"
        );
    }
}
```

Create `crates/rocket-infra/src/grpc/executor.rs` with only this test module first:

```rust
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
```

- [ ] **Step 6: Run the tests to verify they fail, then implement the transport**

Run: `cargo test -j4 -p rocket-infra grpc::`
Expected: FAIL to compile (`TonicGrpcExecutor`, `parse_target`, `connect` not defined). The first build compiles tonic and takes a few minutes.

Put this above the test module in `crates/rocket-infra/src/grpc/channel.rs`:

```rust
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rocket_grpc::GrpcCall;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::grpc::GrpcMetadataPair;
use tonic::metadata::{Ascii, Binary, KeyAndValueRef, MetadataKey, MetadataMap, MetadataValue};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Where to connect and whether to use TLS.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Target {
    /// `http://authority` or `https://authority`, which is what tonic expects.
    pub uri: String,
    pub tls: bool,
}

/// `grpcs://` and `https://` use TLS. `grpc://`, `http://` and a bare `host:port`
/// do not. Anything after the authority is ignored, since the method is chosen separately.
pub(crate) fn parse_target(url: &str) -> DomainResult<Target> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidInput("the gRPC URL is empty".into()));
    }
    let (scheme, rest) = match trimmed.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => ("grpc".to_string(), trimmed),
    };
    let tls = match scheme.as_str() {
        "grpcs" | "https" => true,
        "grpc" | "http" => false,
        other => {
            return Err(DomainError::InvalidInput(format!(
                "unsupported URL scheme '{other}'; use grpc://, grpcs://, http:// or https://"
            )))
        }
    };
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.chars().any(char::is_whitespace) {
        return Err(DomainError::InvalidInput(format!(
            "'{trimmed}' has no valid host"
        )));
    }
    Ok(Target {
        uri: format!("{}://{authority}", if tls { "https" } else { "http" }),
        tls,
    })
}

/// Opens a channel. The connect step has its own 10 second limit. The deadline of
/// the call itself is applied by the executor.
pub(crate) async fn connect(call: &GrpcCall) -> DomainResult<Channel> {
    let target = parse_target(&call.url)?;
    let mut endpoint = Endpoint::from_shared(target.uri.clone())
        .map_err(|e| DomainError::InvalidInput(format!("invalid gRPC URL '{}': {e}", call.url)))?
        .connect_timeout(CONNECT_TIMEOUT);
    if target.tls {
        let mut tls = ClientTlsConfig::new().with_native_roots();
        if let Some(pem) = &call.tls_ca_pem {
            tls = tls.ca_certificate(Certificate::from_pem(pem));
        }
        endpoint = endpoint
            .tls_config(tls)
            .map_err(|e| DomainError::InvalidInput(format!("invalid TLS setup: {e}")))?;
    }
    endpoint.connect().await.map_err(|e| {
        DomainError::Http(format!(
            "could not connect to {}: {}",
            target.uri,
            describe_error(&e)
        ))
    })
}

/// Joins an error and its sources, so "transport error" comes with the real cause.
pub(crate) fn describe_error(error: &(dyn std::error::Error + 'static)) -> String {
    let mut parts: Vec<String> = vec![error.to_string()];
    let mut source = error.source();
    while let Some(inner) = source {
        let text = inner.to_string();
        if parts.last() != Some(&text) {
            parts.push(text);
        }
        source = inner.source();
    }
    parts.join(": ")
}

/// Names the client sets itself. A user value for one of them would break the call.
fn is_reserved(name: &str) -> bool {
    name.starts_with("grpc-") || matches!(name, "content-type" | "te" | "user-agent")
}

/// Copies `pairs` into request metadata. Names are lower-cased. A `-bin` name takes
/// base64 text. Entries with an empty name are skipped.
pub(crate) fn apply_metadata(
    map: &mut MetadataMap,
    pairs: &[GrpcMetadataPair],
) -> DomainResult<()> {
    for pair in pairs {
        let name = pair.name.trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if is_reserved(&name) {
            return Err(DomainError::InvalidInput(format!(
                "metadata '{name}' is set by the gRPC client and cannot be overridden"
            )));
        }
        if name.ends_with("-bin") {
            let bytes = STANDARD.decode(pair.value.trim()).map_err(|e| {
                DomainError::InvalidInput(format!("metadata '{name}' must be base64: {e}"))
            })?;
            let key = MetadataKey::<Binary>::from_bytes(name.as_bytes()).map_err(|e| {
                DomainError::InvalidInput(format!("invalid metadata name '{name}': {e}"))
            })?;
            map.append_bin(key, MetadataValue::from_bytes(&bytes));
        } else {
            let key = MetadataKey::<Ascii>::from_bytes(name.as_bytes()).map_err(|e| {
                DomainError::InvalidInput(format!("invalid metadata name '{name}': {e}"))
            })?;
            // gRPC allows printable ASCII only. tonic would also pass bytes above 0x7e.
            let printable = pair.value.bytes().all(|b| (0x20..=0x7e).contains(&b));
            let value = MetadataValue::<Ascii>::try_from(pair.value.as_str())
                .ok()
                .filter(|_| printable)
                .ok_or_else(|| {
                    DomainError::InvalidInput(format!(
                        "metadata '{name}' has a value that is not printable ASCII"
                    ))
                })?;
            map.append(key, value);
        }
    }
    Ok(())
}

/// Reads response metadata. Binary values are shown as base64.
pub(crate) fn pairs_from(map: &MetadataMap) -> Vec<GrpcMetadataPair> {
    map.iter()
        .map(|entry| match entry {
            KeyAndValueRef::Ascii(key, value) => {
                GrpcMetadataPair::new(key.as_str(), value.to_str().unwrap_or("<not printable>"))
            }
            KeyAndValueRef::Binary(key, value) => GrpcMetadataPair::new(
                key.as_str(),
                value
                    .to_bytes()
                    .map(|b| STANDARD.encode(b))
                    .unwrap_or_else(|_| "<invalid base64>".into()),
            ),
        })
        .collect()
}
```

Put this above the test module in `crates/rocket-infra/src/grpc/executor.rs`:

```rust
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
```

Run: `cargo test -j4 -p rocket-infra grpc::`
Expected: PASS (20 tests, 5 of them the file loader tests from Plan 11). If a TLS test fails with a message about a crypto provider, `tls-ring` is missing from the `tonic` features.

- [ ] **Step 7: Write the failing service tests**

Create `crates/rocket-app/src/grpc_service.rs` with only this test module first. It uses fakes for the executor and the proto loader and the existing `InMemoryCollectionRepo` from `test_doubles.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use rocket_collection::{Collection, GrpcMessage, GrpcMetadataEntry};
    use rocket_grpc::{GrpcStatus, ProtoFileReader};

    use super::*;
    use crate::test_doubles::InMemoryCollectionRepo;

    const PROTO: &str = r#"syntax = "proto3";
package demo.v1;
service Greeter {
  rpc SayHello (Req) returns (Rep);
  rpc List (Req) returns (stream Rep);
  rpc Collect (stream Req) returns (Rep);
  rpc Chat (stream Req) returns (stream Rep);
}
message Req { string name = 1; }
message Rep { string message = 1; }
"#;
    const SAY_HELLO: &str = "demo.v1.Greeter/SayHello";

    struct MemReader;
    impl ProtoFileReader for MemReader {
        fn read(&self, name: &str) -> Option<String> {
            (name == "greeter.proto").then(|| PROTO.to_string())
        }
    }

    fn registry() -> ProtoRegistry {
        ProtoRegistry::compile("greeter.proto", Arc::new(MemReader)).expect("compile")
    }

    #[derive(Default)]
    struct FakeLoader {
        loads: Mutex<Vec<(PathBuf, Vec<PathBuf>)>>,
    }

    impl ProtoLoader for FakeLoader {
        fn load(&self, proto_file: &Path, extra: &[PathBuf]) -> DomainResult<ProtoRegistry> {
            lock(&self.loads).push((proto_file.to_path_buf(), extra.to_vec()));
            Ok(registry())
        }
    }

    #[derive(Default)]
    struct FakeExecutor {
        unary: Mutex<Vec<(GrpcCall, String)>>,
    }

    #[async_trait]
    impl GrpcExecutor for FakeExecutor {
        async fn unary(
            &self,
            call: &GrpcCall,
            _registry: &ProtoRegistry,
            request_json: &str,
        ) -> DomainResult<GrpcUnaryResponse> {
            lock(&self.unary).push((call.clone(), request_json.to_string()));
            Ok(GrpcUnaryResponse {
                headers: vec![],
                trailers: vec![],
                message_json: Some("{}".into()),
                status: GrpcStatus::ok(),
                duration_ms: 1,
            })
        }
    }

    struct Harness {
        svc: GrpcService,
        exec: Arc<FakeExecutor>,
        loader: Arc<FakeLoader>,
        dir: tempfile::TempDir,
    }

    fn harness(collection_auth: Option<Auth>) -> Harness {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let proto_dir = dir.path().join("collections/api/protos");
        std::fs::create_dir_all(&proto_dir).expect("mkdir");
        std::fs::write(proto_dir.join("greeter.proto"), PROTO).expect("write");
        let mut collection = Collection::new("api");
        collection.settings.auth = collection_auth;
        let exec = Arc::new(FakeExecutor::default());
        let loader = Arc::new(FakeLoader::default());
        let svc = GrpcService::new(
            exec.clone(),
            loader.clone(),
            InMemoryCollectionRepo::new(collection),
            Arc::new(Mutex::new(dir.path().to_path_buf())),
        );
        Harness {
            svc,
            exec,
            loader,
            dir,
        }
    }

    fn input(request: GrpcRequest) -> GrpcExecuteInput {
        GrpcExecuteInput {
            collection: Some("api".into()),
            request,
            message: None,
            variables: HashMap::new(),
            timeout: None,
        }
    }

    fn request(method: &str) -> GrpcRequest {
        let mut r = GrpcRequest::new("Say Hello", "localhost:50051");
        r.method = Some(method.to_string());
        r.proto_file_path = Some("protos/greeter.proto".into());
        r
    }

    fn sent(h: &Harness) -> (GrpcCall, String) {
        lock(&h.exec.unary)
            .last()
            .cloned()
            .expect("a unary call was made")
    }

    // ---- unary preparation -------------------------------------------------

    #[tokio::test]
    async fn variables_resolve_in_the_url_metadata_and_message() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.url = "{{host}}".into();
        r.metadata = vec![GrpcMetadataEntry::new("x-key", "{{tok}}")];
        let mut i = input(r);
        i.message = Some(r#"{"name": "{{tok}}"}"#.into());
        i.variables = HashMap::from([
            ("host".into(), "api:9".into()),
            ("tok".into(), "abc".into()),
        ]);
        h.svc.call_unary(i).await.expect("call");
        let (call, message) = sent(&h);
        assert_eq!(call.url, "api:9");
        assert_eq!(call.metadata, vec![GrpcMetadataPair::new("x-key", "abc")]);
        assert_eq!(message, r#"{"name": "abc"}"#);
        assert_eq!(call.full_method, SAY_HELLO);
    }

    #[tokio::test]
    async fn a_request_variable_overrides_the_same_name_from_the_environment() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.url = "{{host}}".into();
        r.variables = vec![
            rocket_collection::CollectionVariable {
                key: "host".into(),
                value: "request-host:1".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            },
            rocket_collection::CollectionVariable {
                key: "off".into(),
                value: "ignored".into(),
                initial_value: String::new(),
                enabled: false,
                secret: false,
            },
        ];
        let mut i = input(r);
        i.variables = HashMap::from([("host".into(), "env-host:2".into())]);
        h.svc.call_unary(i).await.expect("call");
        assert_eq!(sent(&h).0.url, "request-host:1");
    }

    #[tokio::test]
    async fn an_undefined_variable_fails_by_name_and_nothing_is_sent() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.url = "{{missing_host}}".into();
        let err = h
            .svc
            .call_unary(input(r))
            .await
            .expect_err("undefined variable");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("missing_host") && m.contains("URL")),
            "{err:?}"
        );
        assert!(lock(&h.exec.unary).is_empty());
    }

    #[tokio::test]
    async fn disabled_metadata_and_blank_names_are_not_sent() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        let mut off = GrpcMetadataEntry::new("x-off", "1");
        off.enabled = false;
        r.metadata = vec![
            off,
            GrpcMetadataEntry::new("  ", "2"),
            GrpcMetadataEntry::new("x-on", "3"),
        ];
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("x-on", "3")]
        );
    }

    #[tokio::test]
    async fn bearer_auth_becomes_the_authorization_header() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Bearer {
            token: "{{tok}}".into(),
        };
        let mut i = input(r);
        i.variables = HashMap::from([("tok".into(), "secret".into())]);
        h.svc.call_unary(i).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("authorization", "Bearer secret")]
        );
    }

    #[tokio::test]
    async fn basic_auth_is_base64_encoded() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Basic {
            username: "ada".into(),
            password: "pw".into(),
        };
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("authorization", "Basic YWRhOnB3")]
        );
    }

    #[tokio::test]
    async fn inherit_takes_the_collection_auth() {
        let h = harness(Some(Auth::Bearer {
            token: "from-collection".into(),
        }));
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Inherit;
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new(
                "authorization",
                "Bearer from-collection"
            )]
        );
    }

    #[tokio::test]
    async fn the_request_auth_wins_over_the_collection_auth() {
        let h = harness(Some(Auth::Bearer {
            token: "collection".into(),
        }));
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Bearer {
            token: "request".into(),
        };
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("authorization", "Bearer request")]
        );
    }

    #[tokio::test]
    async fn a_metadata_line_for_authorization_beats_the_auth_setting() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Bearer {
            token: "from-auth".into(),
        };
        r.metadata = vec![GrpcMetadataEntry::new("Authorization", "Custom x")];
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("Authorization", "Custom x")]
        );
    }

    #[tokio::test]
    async fn an_unsupported_auth_type_fails_instead_of_going_out_unauthenticated() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Digest {
            username: "u".into(),
            password: "p".into(),
        };
        let err = h
            .svc
            .call_unary(input(r))
            .await
            .expect_err("unsupported auth");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("Digest")),
            "{err:?}"
        );
        assert!(lock(&h.exec.unary).is_empty());
    }

    #[tokio::test]
    async fn an_api_key_in_the_query_is_rejected_and_one_in_a_header_is_sent() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::ApiKey {
            key: "x-api-key".into(),
            value: "k".into(),
            placement: "query".into(),
        };
        assert!(h.svc.call_unary(input(r.clone())).await.is_err());
        r.auth = Auth::ApiKey {
            key: "x-api-key".into(),
            value: "k".into(),
            placement: "header".into(),
        };
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("x-api-key", "k")]
        );
    }

    #[tokio::test]
    async fn a_missing_method_is_an_error() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.method = None;
        let err = h.svc.call_unary(input(r)).await.expect_err("no method");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn the_selected_saved_message_is_sent_when_none_is_given() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.messages = vec![
            GrpcMessage {
                title: "a".into(),
                selected: false,
                content: r#"{"name":"a"}"#.into(),
            },
            GrpcMessage {
                title: "b".into(),
                selected: true,
                content: r#"{"name":"b"}"#.into(),
            },
        ];
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(sent(&h).1, r#"{"name":"b"}"#);
    }

    #[tokio::test]
    async fn no_message_at_all_sends_an_empty_object() {
        let h = harness(None);
        h.svc
            .call_unary(input(request(SAY_HELLO)))
            .await
            .expect("call");
        assert_eq!(sent(&h).1, "{}");
    }

    #[tokio::test]
    async fn a_relative_proto_path_resolves_inside_the_collection() {
        let h = harness(None);
        h.svc
            .call_unary(input(request(SAY_HELLO)))
            .await
            .expect("call");
        let loads = lock(&h.loader.loads);
        let collection_dir = h.dir.path().join("collections/api");
        assert_eq!(loads[0].0, collection_dir.join("protos/greeter.proto"));
        assert_eq!(loads[0].1, vec![collection_dir]);
    }

    #[tokio::test]
    async fn a_relative_proto_path_cannot_climb_out_of_the_collection() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.proto_file_path = Some("../other/secret.proto".into());
        let err = h.svc.call_unary(input(r)).await.expect_err("climbing path");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("..")),
            "{err:?}"
        );
        assert!(lock(&h.loader.loads).is_empty());
    }
}
```

In `crates/rocket-app/src/lib.rs` add `pub mod grpc_service;` after `pub mod git_service;` and these re-exports with the others:

```rust
pub use grpc_service::{GrpcExecuteInput, GrpcService};
```

- [ ] **Step 8: Run the tests to verify they fail, then implement the service**

Run: `cargo test -j4 -p rocket-app grpc_service`
Expected: FAIL to compile (`GrpcService`, `GrpcExecuteInput` not defined).

Put this above the test module in `crates/rocket-app/src/grpc_service.rs`:

```rust
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rocket_collection::{CollectionRepository, GrpcRequest};
use rocket_environment::resolve;
use rocket_grpc::{GrpcCall, GrpcExecutor, GrpcUnaryResponse, ProtoLoader, ProtoRegistry};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::grpc::GrpcMetadataPair;
use rocket_shared::types::Auth;

/// What the UI sends for one call. `request` is the editor state, which may be unsaved.
#[derive(Debug, Clone)]
pub struct GrpcExecuteInput {
    /// The collection the request belongs to. Needed for relative proto paths and inherited auth.
    pub collection: Option<String>,
    pub request: GrpcRequest,
    /// The message to send. `None` uses the selected saved message.
    pub message: Option<String>,
    /// Every variable in scope, already merged by precedence.
    pub variables: HashMap<String, String>,
    /// Deadline for the whole call.
    pub timeout: Option<Duration>,
}

impl GrpcExecuteInput {
    /// The request's own variables are the innermost scope, so they win over
    /// the values the caller collected from the environment and collection.
    fn with_request_variables(mut self) -> Self {
        for variable in self.request.variables.iter().filter(|v| v.enabled) {
            self.variables
                .insert(variable.key.clone(), variable.value.clone());
        }
        self
    }
}

/// Runs gRPC calls: resolves variables and auth, finds the descriptors, and keeps
/// the table of running streaming sessions.
pub struct GrpcService {
    executor: Arc<dyn GrpcExecutor>,
    proto_loader: Arc<dyn ProtoLoader>,
    collection_repo: Arc<dyn CollectionRepository>,
    workspace_path: Arc<Mutex<PathBuf>>,
}

impl GrpcService {
    pub fn new(
        executor: Arc<dyn GrpcExecutor>,
        proto_loader: Arc<dyn ProtoLoader>,
        collection_repo: Arc<dyn CollectionRepository>,
        workspace_path: Arc<Mutex<PathBuf>>,
    ) -> Self {
        Self {
            executor,
            proto_loader,
            collection_repo,
            workspace_path,
        }
    }

    /// Runs a unary call and returns its reply, headers, trailers and status.
    pub async fn call_unary(&self, input: GrpcExecuteInput) -> DomainResult<GrpcUnaryResponse> {
        let input = input.with_request_variables();
        let call = self.prepare_call(&input, true)?;
        let message = self.prepare_message(&input)?;
        let registry = self.registry_for(&input).await?;
        self.executor.unary(&call, &registry, &message).await
    }

    /// Builds the transport call: resolved URL, metadata and auth.
    fn prepare_call(
        &self,
        input: &GrpcExecuteInput,
        require_method: bool,
    ) -> DomainResult<GrpcCall> {
        let request = &input.request;
        let vars = &input.variables;
        let full_method = match request.method.as_deref().map(str::trim) {
            Some(m) if !m.is_empty() => m.trim_start_matches('/').to_string(),
            _ if require_method => {
                return Err(DomainError::InvalidInput(
                    "choose a method for this request".into(),
                ))
            }
            _ => String::new(),
        };
        let url = resolve_text(&request.url, vars, "the URL")?;
        if url.trim().is_empty() {
            return Err(DomainError::InvalidInput("the gRPC URL is empty".into()));
        }

        let mut metadata = Vec::new();
        for entry in request
            .metadata
            .iter()
            .filter(|e| e.enabled && !e.key.trim().is_empty())
        {
            metadata.push(GrpcMetadataPair::new(
                resolve_text(&entry.key, vars, "a metadata name")?,
                resolve_text(&entry.value, vars, &format!("metadata '{}'", entry.key))?,
            ));
        }
        for pair in self.auth_metadata(input)? {
            let taken = metadata
                .iter()
                .any(|m| m.name.eq_ignore_ascii_case(&pair.name));
            if !taken {
                metadata.push(pair);
            }
        }
        Ok(GrpcCall {
            url,
            full_method,
            metadata,
            timeout: input.timeout,
            tls_ca_pem: None,
        })
    }

    /// The message to send: the explicit one, else the selected saved one, else `{}`.
    fn prepare_message(&self, input: &GrpcExecuteInput) -> DomainResult<String> {
        let text = match &input.message {
            Some(text) => text.clone(),
            None => input
                .request
                .messages
                .iter()
                .find(|m| m.selected)
                .or_else(|| input.request.messages.first())
                .map(|m| m.content.clone())
                .unwrap_or_else(|| "{}".to_string()),
        };
        resolve_text(&text, &input.variables, "the message")
    }

    /// Request auth, or the collection's when the request inherits, as call metadata.
    fn auth_metadata(&self, input: &GrpcExecuteInput) -> DomainResult<Vec<GrpcMetadataPair>> {
        let request = &input.request;
        let own = request.auth.clone();
        let effective = match own {
            Auth::None | Auth::Inherit => input
                .collection
                .as_deref()
                .and_then(|c| self.collection_repo.get_settings(c).ok())
                .and_then(|s| s.auth)
                .unwrap_or(Auth::None),
            explicit => explicit,
        };
        let vars = &input.variables;
        match effective {
            Auth::None | Auth::Inherit => Ok(vec![]),
            Auth::Bearer { token } => Ok(vec![GrpcMetadataPair::new(
                "authorization",
                format!("Bearer {}", resolve_text(&token, vars, "the bearer token")?),
            )]),
            Auth::Basic { username, password } => {
                let user = resolve_text(&username, vars, "the basic auth user")?;
                let pass = resolve_text(&password, vars, "the basic auth password")?;
                Ok(vec![GrpcMetadataPair::new(
                    "authorization",
                    format!("Basic {}", STANDARD.encode(format!("{user}:{pass}"))),
                )])
            }
            Auth::ApiKey { key, value, placement } => {
                if placement.eq_ignore_ascii_case("query") {
                    return Err(DomainError::InvalidInput(
                        "an API key cannot be sent in the query of a gRPC call; place it in a header".into(),
                    ));
                }
                Ok(vec![GrpcMetadataPair::new(
                    resolve_text(&key, vars, "the API key name")?,
                    resolve_text(&value, vars, "the API key value")?,
                )])
            }
            other => Err(DomainError::InvalidInput(format!(
                "{} auth is not supported for gRPC calls yet; use a bearer token or a metadata header",
                auth_name(&other)
            ))),
        }
    }

    /// Finds the descriptors from the request's `.proto` file.
    async fn registry_for(&self, input: &GrpcExecuteInput) -> DomainResult<ProtoRegistry> {
        let raw = input
            .request
            .proto_file_path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .ok_or_else(|| {
                DomainError::InvalidInput("set a .proto file for this request".into())
            })?;
        let resolved = resolve_text(raw, &input.variables, "the proto file path")?;
        let (path, include_dirs) =
            self.resolve_proto_path(input.collection.as_deref(), &resolved)?;
        let loader = Arc::clone(&self.proto_loader);
        tokio::task::spawn_blocking(move || loader.load(&path, &include_dirs))
            .await
            .map_err(|e| DomainError::Internal(format!("proto loading stopped: {e}")))?
    }

    /// A relative path is inside the collection and may not climb out of it. An
    /// absolute path or `~/` is used as given. Returns the file and the extra
    /// directories searched for imports.
    fn resolve_proto_path(
        &self,
        collection: Option<&str>,
        raw: &str,
    ) -> DomainResult<(PathBuf, Vec<PathBuf>)> {
        let collection_dir = collection.map(|name| {
            lock_path(&self.workspace_path)
                .join("collections")
                .join(name)
        });
        let extra: Vec<PathBuf> = collection_dir.iter().cloned().collect();
        if let Some(rest) = raw.strip_prefix("~/") {
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .ok_or_else(|| {
                    DomainError::InvalidInput("cannot find the home directory".into())
                })?;
            return Ok((PathBuf::from(home).join(rest), extra));
        }
        let path = Path::new(raw);
        if path.is_absolute() {
            return Ok((path.to_path_buf(), extra));
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(DomainError::InvalidInput(
                "a relative proto path cannot contain '..'".into(),
            ));
        }
        let base = collection_dir.ok_or_else(|| {
            DomainError::InvalidInput("a relative proto path needs a collection".into())
        })?;
        Ok((base.join(path), extra))
    }
}

/// Resolves `{{name}}` placeholders. A placeholder with no value is an error,
/// because sending the literal text to a server is never what the user wants.
fn resolve_text(text: &str, vars: &HashMap<String, String>, what: &str) -> DomainResult<String> {
    let result = resolve(text, vars);
    if result.unresolved.is_empty() {
        Ok(result.output)
    } else {
        Err(DomainError::InvalidInput(format!(
            "{what} uses undefined variable(s): {}",
            result.unresolved.join(", ")
        )))
    }
}

fn auth_name(auth: &Auth) -> &'static str {
    match auth {
        Auth::None => "no",
        Auth::Basic { .. } => "Basic",
        Auth::Bearer { .. } => "Bearer",
        Auth::ApiKey { .. } => "API key",
        Auth::OAuth2(_) => "OAuth2",
        Auth::AwsSigV4 { .. } => "AWS Signature",
        Auth::Inherit => "inherited",
        Auth::Wsse { .. } => "WSSE",
        Auth::Digest { .. } => "Digest",
        Auth::Ntlm { .. } => "NTLM",
        Auth::OAuth1(_) => "OAuth1",
    }
}

/// A poisoned lock still holds consistent data here, so keep going.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_path(m: &Mutex<PathBuf>) -> PathBuf {
    lock(m).clone()
}
```

The service builds the call (resolved URL, enabled metadata, auth as metadata), picks the message (the one passed in, else the selected saved message, else `{}`), finds the descriptors from the request's `.proto` file, and runs the call. Auth and variables are resolved here rather than by `RequestExecutionService` on purpose: that file stays untouched, and the resolution can be tested with fakes.

Run: `cargo test -j4 -p rocket-app grpc_service`
Expected: PASS (16 tests).

- [ ] **Step 9: IPC command and wiring**

Create `src-tauri/src/commands/grpc.rs`. The DTO is the only camelCase struct. The command gathers the variables in scope the way the OAuth2 commands do, and everything else is the service's job:

```rust
use std::collections::HashMap;
use std::time::Duration;

use rocket_app::{GrpcExecuteInput, GrpcService, RequestExecutionService};
use rocket_collection::GrpcRequest;
use rocket_grpc::GrpcUnaryResponse;
use rocket_shared::error::DomainError;
use serde::Deserialize;
use tauri::State;

/// What the gRPC tab sends for one call. `request` is the editor state, which may be unsaved.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcExecuteDto {
    pub collection: Option<String>,
    pub request: GrpcRequest,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
    #[serde(default)]
    pub global_env_name: Option<String>,
    /// Path of the request file, so folder-level variables apply.
    #[serde(default)]
    pub request_path: Option<String>,
    /// Deadline in milliseconds. 0 or absent means none.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl GrpcExecuteDto {
    fn into_input(self, variables: HashMap<String, String>) -> GrpcExecuteInput {
        GrpcExecuteInput {
            collection: self.collection,
            request: self.request,
            message: self.message,
            variables,
            timeout: self
                .timeout_ms
                .filter(|ms| *ms > 0)
                .map(Duration::from_millis),
        }
    }
}

/// Collects the variables in scope for the call: global, collection, environment, folders,
/// and RocketVault values. The request's own variables are added by the service.
async fn resolve_input(
    dto: GrpcExecuteDto,
    exec: &RequestExecutionService,
) -> Result<GrpcExecuteInput, DomainError> {
    // RocketVault values for the environment's bindings, so `{{alias.secretName}}` resolves.
    // An environment without bindings needs no vault access.
    let secrets = exec
        .resolve_external_secrets(dto.collection.as_deref(), dto.environment_name.as_deref())
        .await?;
    let variables = exec.build_variable_context(
        dto.global_env_name.as_deref(),
        dto.collection.as_deref(),
        dto.environment_name.as_deref(),
        dto.request_path.as_deref(),
        &secrets,
    );
    Ok(dto.into_input(variables))
}

#[tauri::command]
pub async fn grpc_unary_call(
    input: GrpcExecuteDto,
    svc: State<'_, GrpcService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<GrpcUnaryResponse, DomainError> {
    let input = resolve_input(input, &exec).await?;
    svc.call_unary(input).await
}
```

Add its tests at the bottom of the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn dto_json(extra: &str) -> String {
        format!(
            r#"{{"collection": "api", "request": {{"name": "Say", "url": "localhost:50051", "methodType": "unary"}}{extra}}}"#
        )
    }

    #[test]
    fn the_dto_reads_camel_case_fields_from_the_frontend() {
        let json = dto_json(
            r#", "message": "{}", "environmentName": "dev", "globalEnvName": "g", "requestPath": "a/b.yml", "timeoutMs": 1500"#,
        );
        let dto: GrpcExecuteDto = serde_json::from_str(&json).expect("dto");
        assert_eq!(dto.environment_name.as_deref(), Some("dev"));
        assert_eq!(dto.global_env_name.as_deref(), Some("g"));
        assert_eq!(dto.request_path.as_deref(), Some("a/b.yml"));
        assert_eq!(dto.request.url, "localhost:50051");
    }

    #[test]
    fn a_zero_or_missing_timeout_means_no_deadline() {
        let zero: GrpcExecuteDto =
            serde_json::from_str(&dto_json(r#", "timeoutMs": 0"#)).expect("dto");
        assert_eq!(zero.into_input(HashMap::new()).timeout, None);
        let none: GrpcExecuteDto = serde_json::from_str(&dto_json("")).expect("dto");
        assert_eq!(none.into_input(HashMap::new()).timeout, None);
        let some: GrpcExecuteDto =
            serde_json::from_str(&dto_json(r#", "timeoutMs": 1500"#)).expect("dto");
        assert_eq!(
            some.into_input(HashMap::new()).timeout,
            Some(Duration::from_millis(1500))
        );
    }

    #[test]
    fn variables_and_message_reach_the_service_input() {
        let dto: GrpcExecuteDto =
            serde_json::from_str(&dto_json(r#", "message": "{\"a\":1}""#)).expect("dto");
        let input = dto.into_input(HashMap::from([("k".to_string(), "v".to_string())]));
        assert_eq!(input.message.as_deref(), Some("{\"a\":1}"));
        assert_eq!(input.variables.get("k").map(String::as_str), Some("v"));
        assert_eq!(input.collection.as_deref(), Some("api"));
    }
}
```

In `src-tauri/src/commands/mod.rs` add `pub mod grpc;` after `pub mod git;`.

In `src-tauri/src/lib.rs`:
- build the service after `contract_svc` is created and before the block that registers managed state:

```rust
            // gRPC calls. The workspace path is shared with the collection repo, so relative
            // proto paths follow a workspace switch.
            let grpc_svc = rocket_app::GrpcService::new(
                Arc::new(rocket_infra::TonicGrpcExecutor),
                Arc::new(rocket_infra::FsProtoLoader),
                Arc::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Arc::clone(&active_workspace_path),
            );
```

- add `app.manage(grpc_svc);` after `app.manage(contract_svc);`;
- add `commands::grpc::grpc_unary_call,` to the `generate_handler!` list after `commands::git::` entries (anywhere in the list is fine, keep it grouped).

Run: `cargo check -j4 -p rocket`
Expected: PASS.
Run: `cargo test -j4 -p rocket commands::grpc`
Expected: PASS (3 tests).

- [ ] **Step 10: Run the checks**

Run:
- `cargo check -j4 -p rocket-shared -p rocket-grpc -p rocket-infra -p rocket-app -p rocket`
- `cargo test -j4 -p rocket-grpc`
- `cargo test -j4 -p rocket-infra grpc::`
- `cargo test -j4 -p rocket-app grpc_service`
- `cargo test -j4 -p rocket commands::grpc`
- `git diff Cargo.lock` and confirm it only adds packages.

Expected: PASS.

- [ ] **Step 11: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add Cargo.lock crates/rocket-shared/src crates/rocket-grpc crates/rocket-infra/Cargo.toml \
  crates/rocket-infra/src/lib.rs crates/rocket-infra/src/grpc \
  crates/rocket-app/Cargo.toml crates/rocket-app/src/lib.rs crates/rocket-app/src/grpc_service.rs \
  src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/src/commands/mod.rs src-tauri/src/commands/grpc.rs
```

Suggested subject: `feat(grpc): run unary gRPC calls over tonic with dynamic messages`.

---

## Task 2: Streaming sessions and events

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/src/events.rs`
- Modify: `crates/rocket-grpc/Cargo.toml`, `src/call.rs`, `src/lib.rs`
- Modify: `crates/rocket-infra/Cargo.toml`, `src/grpc/executor.rs`
- Modify: `crates/rocket-app/src/grpc_service.rs`
- Modify: `src-tauri/src/tauri_event_bus.rs`, `src/lib.rs`, `src/commands/grpc.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - `DomainEvent::{GrpcSessionStarted { session_id, method_type }, GrpcSessionHeaders { session_id, headers }, GrpcSessionMessage { session_id, index, json }, GrpcSessionFinished { session_id, code, code_name, message, trailers, duration_ms }}`. Tauri event names `grpc-session-started`, `grpc-session-headers`, `grpc-session-message`, `grpc-session-finished`.
  - `rocket_grpc::{GrpcStreamEvent, GrpcStreamHandle}` and `GrpcExecutor::open_stream(&self, call: &GrpcCall, registry: &ProtoRegistry, initial_json: Option<String>) -> DomainResult<GrpcStreamHandle>`.
  - `GrpcService::new(..., events: Arc<dyn EventPublisher>)`, `start_session(input) -> DomainResult<String>`, `send_message(&self, session_id: &str, json: &str) -> DomainResult<()>`, `end_requests(&self, session_id: &str) -> DomainResult<()>`, `cancel(&self, session_id: &str) -> DomainResult<()>`, `end_all(&self)`.
  - Commands `grpc_start_session`, `grpc_send_message`, `grpc_end_requests`, `grpc_cancel_session`.

How the session model works. `open_stream` returns a handle with an optional `outbound` sender (client-streaming and bidirectional calls), an `events` receiver and an `abort` handle. A server-streaming call sends its one request message when it opens. The service keeps the sender and the abort handle in a table keyed by session id and forwards every event as a `DomainEvent`. Dropping the sender half-closes the call. Aborting the task cancels the call. Whoever removes a session from the table first publishes its `Finished` event, so a cancel that races with the call ending still produces exactly one.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing transport tests**

In `crates/rocket-grpc/Cargo.toml` add `tokio.workspace = true` under `[dependencies]`. In `crates/rocket-infra/Cargo.toml` add `tokio-stream = "0.1"` under `[dependencies]`.

Add the stream tests to `crates/rocket-infra/src/grpc/executor.rs`. These hunks are inside its `tests` module (the new constants and the stream tests at the end):

```diff
@@ -168,6 +348,8 @@
 
     const SAY_HELLO: &str = "demo.greeter.v1.Greeter/SayHello";
     const LIST: &str = "demo.greeter.v1.Greeter/ListGreetings";
+    const COLLECT: &str = "demo.greeter.v1.Greeter/CollectNames";
+    const CHAT: &str = "demo.greeter.v1.Greeter/Chat";
 
     fn call(server: &TestServer, method: &str) -> GrpcCall {
         GrpcCall {
```

```diff
@@ -291,6 +473,12 @@
             .await
             .expect_err("streaming method");
         assert!(matches!(unary_on_stream, DomainError::InvalidInput(_)));
+        let stream_on_unary = TonicGrpcExecutor
+            .open_stream(&call(&server, SAY_HELLO), &server.registry, Some(name("x")))
+            .await
+            .err()
+            .expect("unary method");
+        assert!(matches!(stream_on_unary, DomainError::InvalidInput(_)));
         let unknown = TonicGrpcExecutor
             .unary(
                 &call(&server, "demo.greeter.v1.Greeter/Nope"),
```

```diff
@@ -332,5 +520,161 @@
             "{err:?}"
         );
     }
+
+    async fn collect(handle: &mut GrpcStreamHandle) -> Vec<GrpcStreamEvent> {
+        let mut out = Vec::new();
+        while let Some(event) = handle.events.recv().await {
+            let done = matches!(event, GrpcStreamEvent::Finished { .. });
+            out.push(event);
+            if done {
+                break;
+            }
+        }
+        out
+    }
+
+    fn messages(events: &[GrpcStreamEvent]) -> Vec<String> {
+        events
+            .iter()
+            .filter_map(|e| match e {
+                GrpcStreamEvent::Message(json) => {
+                    let v: serde_json::Value = serde_json::from_str(json).expect("json");
+                    v["message"].as_str().map(str::to_string)
+                }
+                _ => None,
+            })
+            .collect()
+    }
+
+    #[tokio::test]
+    async fn a_server_stream_delivers_headers_messages_then_finished() {
+        let server = start(None, Reflection::None).await;
+        let mut handle = TonicGrpcExecutor
+            .open_stream(&call(&server, LIST), &server.registry, Some(name("n")))
+            .await
+            .expect("open");
+        assert!(handle.outbound.is_none());
+        let events = tokio::time::timeout(Duration::from_secs(5), collect(&mut handle))
+            .await
+            .expect("stream ends");
+        assert!(
+            matches!(events.first(), Some(GrpcStreamEvent::Headers(_))),
+            "{events:?}"
+        );
+        assert_eq!(messages(&events), vec!["n-0", "n-1", "n-2"]);
+        match events.last() {
+            Some(GrpcStreamEvent::Finished { status, .. }) => assert!(status.is_ok()),
+            other => panic!("expected Finished, got {other:?}"),
+        }
+    }
+
+    #[tokio::test]
+    async fn a_server_stream_without_a_request_message_is_rejected() {
+        let server = start(None, Reflection::None).await;
+        let err = TonicGrpcExecutor
+            .open_stream(&call(&server, LIST), &server.registry, None)
+            .await
+            .err()
+            .expect("needs a message");
+        assert!(matches!(err, DomainError::InvalidInput(_)));
+    }
+
+    #[tokio::test]
+    async fn a_client_stream_sends_every_message_and_ends_when_the_sender_drops() {
+        let server = start(None, Reflection::None).await;
+        let mut handle = TonicGrpcExecutor
+            .open_stream(&call(&server, COLLECT), &server.registry, None)
+            .await
+            .expect("open");
+        let outbound = handle
+            .outbound
+            .take()
+            .expect("client streaming has a sender");
+        for n in ["a", "b", "c"] {
+            outbound.send(name(n)).await.expect("send");
+        }
+        drop(outbound);
+        let events = tokio::time::timeout(Duration::from_secs(5), collect(&mut handle))
+            .await
+            .expect("stream ends");
+        assert_eq!(messages(&events), vec!["a,b,c"]);
+        assert!(
+            matches!(events.last(), Some(GrpcStreamEvent::Finished { status, .. }) if status.is_ok())
+        );
+    }
+
+    #[tokio::test]
+    async fn a_bidi_stream_answers_each_message_while_it_stays_open() {
+        let server = start(None, Reflection::None).await;
+        let mut handle = TonicGrpcExecutor
+            .open_stream(&call(&server, CHAT), &server.registry, None)
+            .await
+            .expect("open");
+        let outbound = handle.outbound.clone().expect("bidi has a sender");
+        let mut seen = Vec::new();
+        for n in ["x", "y"] {
+            outbound.send(name(n)).await.expect("send");
+            loop {
+                match tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
+                    .await
+                    .expect("event in time")
+                {
+                    Some(GrpcStreamEvent::Message(json)) => {
+                        seen.push(json);
+                        break;
+                    }
+                    Some(GrpcStreamEvent::Headers(_)) => continue,
+                    other => panic!("unexpected {other:?}"),
+                }
+            }
+        }
+        assert!(
+            seen[0].contains("echo x") && seen[1].contains("echo y"),
+            "{seen:?}"
+        );
+        handle.outbound = None;
+        drop(outbound);
+        let tail = tokio::time::timeout(Duration::from_secs(5), collect(&mut handle))
+            .await
+            .expect("stream ends after half-close");
+        assert!(
+            matches!(tail.last(), Some(GrpcStreamEvent::Finished { status, .. }) if status.is_ok())
+        );
+    }
+
+    #[tokio::test]
+    async fn aborting_the_task_stops_the_events() {
+        let server = start(None, Reflection::None).await;
+        let mut handle = TonicGrpcExecutor
+            .open_stream(&call(&server, CHAT), &server.registry, None)
+            .await
+            .expect("open");
+        handle.abort.abort();
+        let next = tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
+            .await
+            .expect("channel closes");
+        assert!(
+            next.is_none(),
+            "an aborted driver sends nothing more: {next:?}"
+        );
+    }
+
+    #[tokio::test]
+    async fn a_stream_deadline_ends_with_deadline_exceeded() {
+        let server = start(None, Reflection::None).await;
+        let mut c = call(&server, CHAT);
+        c.timeout = Some(Duration::from_millis(150));
+        let mut handle = TonicGrpcExecutor
+            .open_stream(&c, &server.registry, None)
+            .await
+            .expect("open");
+        let events = tokio::time::timeout(Duration::from_secs(5), collect(&mut handle))
+            .await
+            .expect("deadline ends the stream");
+        match events.last() {
+            Some(GrpcStreamEvent::Finished { status, .. }) => assert_eq!(status.code, 4),
+            other => panic!("expected Finished, got {other:?}"),
+        }
+    }
 }
 
```

Add the stream types and the `open_stream` method to the trait and the exports (`call.rs` and `lib.rs`):

```diff
@@ -4,6 +4,8 @@
 use rocket_shared::error::DomainResult;
 use rocket_shared::grpc::GrpcMetadataPair;
 use serde::Serialize;
+use tokio::sync::mpsc;
+use tokio::task::AbortHandle;
 
 use crate::registry::ProtoRegistry;
 
```

```diff
@@ -84,6 +86,28 @@
     pub duration_ms: u64,
 }
 
+/// What a running stream reports back, in order.
+#[derive(Debug, Clone, PartialEq)]
+pub enum GrpcStreamEvent {
+    Headers(Vec<GrpcMetadataPair>),
+    Message(String),
+    /// Always the last event of a stream that ends by itself.
+    Finished {
+        status: GrpcStatus,
+        trailers: Vec<GrpcMetadataPair>,
+    },
+}
+
+/// A running streaming call.
+pub struct GrpcStreamHandle {
+    /// JSON messages to send. `None` for a server-streaming call. Dropping the
+    /// sender ends the request side of the call (half-close).
+    pub outbound: Option<mpsc::Sender<String>>,
+    pub events: mpsc::Receiver<GrpcStreamEvent>,
+    /// Aborting the task cancels the call.
+    pub abort: AbortHandle,
+}
+
 /// Runs gRPC calls. Implemented by `TonicGrpcExecutor` in `rocket-infra`.
 #[async_trait]
 pub trait GrpcExecutor: Send + Sync {
```

```diff
@@ -95,6 +119,16 @@
         registry: &ProtoRegistry,
         request_json: &str,
     ) -> DomainResult<GrpcUnaryResponse>;
+
+    /// Starts a client-streaming, server-streaming or bidirectional call.
+    /// `initial_json` is the one request of a server-streaming call, and an
+    /// optional first message of the other two.
+    async fn open_stream(
+        &self,
+        call: &GrpcCall,
+        registry: &ProtoRegistry,
+        initial_json: Option<String>,
+    ) -> DomainResult<GrpcStreamHandle>;
 }
 
 #[cfg(test)]
```

```diff
@@ -10,6 +10,7 @@
 mod test_support;
 
 pub use call::{grpc_code_name, GrpcCall, GrpcExecutor, GrpcStatus, GrpcUnaryResponse};
+pub use call::{GrpcStreamEvent, GrpcStreamHandle};
 pub use codec::{empty_message_json, json_to_message, message_to_json};
 pub use prost_reflect::{DynamicMessage, MessageDescriptor, MethodDescriptor};
 pub use registry::{GrpcMethodInfo, GrpcServiceInfo, ProtoFileReader, ProtoLoader, ProtoRegistry};
```

- [ ] **Step 3: Run the tests to verify they fail, then implement the stream driver**

Run: `cargo test -j4 -p rocket-infra grpc::executor`
Expected: FAIL to compile (`open_stream` is not a method of `TonicGrpcExecutor`).

Apply these hunks to `crates/rocket-infra/src/grpc/executor.rs` (everything outside the `tests` module):

```diff
@@ -7,12 +7,21 @@
     json_to_message, message_to_json, GrpcCall, GrpcExecutor, GrpcStatus, GrpcUnaryResponse,
     ProtoRegistry,
 };
+use rocket_grpc::{GrpcStreamEvent, GrpcStreamHandle};
 use rocket_shared::error::{DomainError, DomainResult};
+use tokio::sync::mpsc;
+use tokio_stream::wrappers::ReceiverStream;
+use tokio_stream::StreamExt;
 use tonic::codegen::http::uri::PathAndQuery;
 use tonic::{Code, Request, Status};
 
 use super::channel::{apply_metadata, connect, describe_error, pairs_from};
 use super::codec::DynCodec;
+
+/// Buffer between the network task and the UI for response events.
+const EVENT_BUFFER: usize = 256;
+/// Buffer for request messages the UI sends before the network takes them.
+const OUTBOUND_BUFFER: usize = 64;
 
 /// Runs gRPC calls over tonic with messages encoded at runtime.
 pub struct TonicGrpcExecutor;
```

```diff
@@ -70,6 +79,59 @@
             message_json,
             status,
             duration_ms: started.elapsed().as_millis() as u64,
+        })
+    }
+
+    async fn open_stream(
+        &self,
+        call: &GrpcCall,
+        registry: &ProtoRegistry,
+        initial_json: Option<String>,
+    ) -> DomainResult<GrpcStreamHandle> {
+        let method = registry.method(&call.full_method)?;
+        let client_streams = method.is_client_streaming();
+        if !client_streams && !method.is_server_streaming() {
+            return Err(DomainError::InvalidInput(format!(
+                "{} is a unary method, send it as a normal call",
+                call.full_method
+            )));
+        }
+        let input = method.input();
+        let initial = match (client_streams, initial_json) {
+            (false, None) => {
+                return Err(DomainError::InvalidInput(
+                    "a server-streaming call needs one request message".into(),
+                ))
+            }
+            (_, Some(json)) => Some(json_to_message(&input, &json)?),
+            (true, None) => None,
+        };
+        let path = method_path(&call.full_method)?;
+        let channel = connect(call).await?;
+
+        let (events_tx, events_rx) = mpsc::channel(EVENT_BUFFER);
+        let (outbound_tx, outbound_rx) = if client_streams {
+            let (tx, rx) = mpsc::channel::<String>(OUTBOUND_BUFFER);
+            (Some(tx), Some(rx))
+        } else {
+            (None, None)
+        };
+
+        let driver = StreamDriver {
+            channel,
+            path,
+            call: call.clone(),
+            input,
+            output: method.output(),
+            initial,
+            outbound: outbound_rx,
+            events: events_tx,
+        };
+        let task = tokio::spawn(driver.run());
+        Ok(GrpcStreamHandle {
+            outbound: outbound_tx,
+            events: events_rx,
+            abort: task.abort_handle(),
         })
     }
 }
```

```diff
@@ -157,6 +219,124 @@
     Ok((headers, message_json, GrpcStatus::ok(), trailers))
 }
 
+struct StreamDriver {
+    channel: tonic::transport::Channel,
+    path: PathAndQuery,
+    call: GrpcCall,
+    input: MessageDescriptor,
+    output: MessageDescriptor,
+    initial: Option<DynamicMessage>,
+    outbound: Option<mpsc::Receiver<String>>,
+    events: mpsc::Sender<GrpcStreamEvent>,
+}
+
+impl StreamDriver {
+    async fn run(self) {
+        let events = self.events.clone();
+        let limit = self.call.timeout;
+        let work = self.drive();
+        let finished = match limit {
+            Some(limit) => tokio::time::timeout(limit, work).await.unwrap_or_else(|_| {
+                Some(GrpcStreamEvent::Finished {
+                    status: deadline_exceeded(),
+                    trailers: vec![],
+                })
+            }),
+            None => work.await,
+        };
+        if let Some(event) = finished {
+            let _ = events.send(event).await;
+        }
+    }
+
+    /// Returns the closing event, or `None` when the receiver went away first.
+    async fn drive(self) -> Option<GrpcStreamEvent> {
+        let StreamDriver {
+            channel,
+            path,
+            call,
+            input,
+            output,
+            initial,
+            outbound,
+            events,
+        } = self;
+        let mut client = tonic::client::Grpc::new(channel);
+        if let Err(e) = client.ready().await {
+            return Some(failed(Code::Unavailable, &describe_error(&e)));
+        }
+        // The request side is one stream for every shape. A server-streaming call
+        // is a stream of one message, which is the same on the wire.
+        let first = tokio_stream::iter(initial);
+        let rest = outbound.map(|rx| {
+            ReceiverStream::new(rx).map_while(move |json| json_to_message(&input, &json).ok())
+        });
+        let request_stream: std::pin::Pin<
+            Box<dyn tokio_stream::Stream<Item = DynamicMessage> + Send>,
+        > = match rest {
+            Some(rest) => Box::pin(first.chain(rest)),
+            None => Box::pin(first),
+        };
+        let mut request = Request::new(request_stream);
+        if let Err(e) = apply_metadata(request.metadata_mut(), &call.metadata) {
+            return Some(failed(Code::InvalidArgument, &e.to_string()));
+        }
+        let response = match client.streaming(request, path, DynCodec::new(output)).await {
+            Ok(response) => response,
+            Err(status) => {
+                return Some(GrpcStreamEvent::Finished {
+                    status: status_of(&status),
+                    trailers: pairs_from(status.metadata()),
+                })
+            }
+        };
+        let (metadata, mut stream, _) = response.into_parts();
+        if events
+            .send(GrpcStreamEvent::Headers(pairs_from(&metadata)))
+            .await
+            .is_err()
+        {
+            return None;
+        }
+        loop {
+            match stream.message().await {
+                Ok(Some(message)) => {
+                    let json = match message_to_json(&message) {
+                        Ok(json) => json,
+                        Err(e) => return Some(failed(Code::Internal, &e.to_string())),
+                    };
+                    if events.send(GrpcStreamEvent::Message(json)).await.is_err() {
+                        return None;
+                    }
+                }
+                Ok(None) => {
+                    let trailers = match stream.trailers().await {
+                        Ok(Some(map)) => pairs_from(&map),
+                        _ => vec![],
+                    };
+                    return Some(GrpcStreamEvent::Finished {
+                        status: GrpcStatus::ok(),
+                        trailers,
+                    });
+                }
+                Err(status) => {
+                    return Some(GrpcStreamEvent::Finished {
+                        status: status_of(&status),
+                        trailers: pairs_from(status.metadata()),
+                    })
+                }
+            }
+        }
+    }
+}
+
+fn failed(code: Code, message: &str) -> GrpcStreamEvent {
+    GrpcStreamEvent::Finished {
+        status: GrpcStatus::new(code as i32, message),
+        trailers: vec![],
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use std::time::Duration;
```

Run: `cargo test -j4 -p rocket-infra grpc::executor`
Expected: PASS (26 tests: the 20 from Task 1 plus 6 stream tests).

A server-streaming call and a unary call both go through one request stream of one message, which is the same on the wire as a plain request. The driver sends the first message, then drains the receiver the service holds the sender of. `StreamExt::map_while` ends the request stream if a message fails to encode, which cannot happen for a message the service already validated.

- [ ] **Step 4: Write the failing service tests**

Add the session tests and the fakes they need to `crates/rocket-app/src/grpc_service.rs`. These hunks are inside its `tests` module:

```diff
@@ -287,6 +490,7 @@
 
     use async_trait::async_trait;
     use rocket_collection::{Collection, GrpcMessage, GrpcMetadataEntry};
+    use rocket_grpc::GrpcStreamHandle;
     use rocket_grpc::{GrpcStatus, ProtoFileReader};
 
     use super::*;
```

```diff
@@ -331,6 +535,8 @@
     #[derive(Default)]
     struct FakeExecutor {
         unary: Mutex<Vec<(GrpcCall, String)>>,
+        streams: Mutex<Vec<(GrpcCall, Option<String>)>>,
+        next_stream: Mutex<Option<GrpcStreamHandle>>,
     }
 
     #[async_trait]
```

```diff
@@ -350,12 +556,53 @@
                 duration_ms: 1,
             })
         }
+
+        async fn open_stream(
+            &self,
+            call: &GrpcCall,
+            _registry: &ProtoRegistry,
+            initial_json: Option<String>,
+        ) -> DomainResult<GrpcStreamHandle> {
+            lock(&self.streams).push((call.clone(), initial_json));
+            lock(&self.next_stream)
+                .take()
+                .ok_or_else(|| DomainError::Internal("no stream prepared".into()))
+        }
+    }
+
+    #[derive(Default)]
+    struct Recorder(Mutex<Vec<DomainEvent>>);
+
+    impl EventPublisher for Recorder {
+        fn publish(&self, event: DomainEvent) {
+            lock(&self.0).push(event);
+        }
+    }
+
+    impl Recorder {
+        fn tags(&self) -> Vec<String> {
+            lock(&self.0)
+                .iter()
+                .map(|e| match e {
+                    DomainEvent::GrpcSessionStarted { method_type, .. } => {
+                        format!("started:{method_type}")
+                    }
+                    DomainEvent::GrpcSessionHeaders { .. } => "headers".to_string(),
+                    DomainEvent::GrpcSessionMessage { index, .. } => format!("message:{index}"),
+                    DomainEvent::GrpcSessionFinished { code_name, .. } => {
+                        format!("finished:{code_name}")
+                    }
+                    _ => "other".to_string(),
+                })
+                .collect()
+        }
     }
 
     struct Harness {
         svc: GrpcService,
         exec: Arc<FakeExecutor>,
         loader: Arc<FakeLoader>,
+        events: Arc<Recorder>,
         dir: tempfile::TempDir,
     }
 
```

```diff
@@ -368,16 +615,19 @@
         collection.settings.auth = collection_auth;
         let exec = Arc::new(FakeExecutor::default());
         let loader = Arc::new(FakeLoader::default());
+        let events = Arc::new(Recorder::default());
         let svc = GrpcService::new(
             exec.clone(),
             loader.clone(),
             InMemoryCollectionRepo::new(collection),
             Arc::new(Mutex::new(dir.path().to_path_buf())),
+            events.clone(),
         );
         Harness {
             svc,
             exec,
             loader,
+            events,
             dir,
         }
     }
```

```diff
@@ -675,5 +925,215 @@
         );
         assert!(lock(&h.loader.loads).is_empty());
     }
+
+    // ---- sessions ----------------------------------------------------------
+
+    struct StreamFixture {
+        events_tx: mpsc::Sender<GrpcStreamEvent>,
+        outbound_rx: Option<mpsc::Receiver<String>>,
+        task: tokio::task::JoinHandle<()>,
+    }
+
+    /// Prepares the handle the fake executor hands out for the next stream.
+    fn prepare_stream(h: &Harness, with_outbound: bool) -> StreamFixture {
+        let (events_tx, events_rx) = mpsc::channel(16);
+        let (outbound, outbound_rx) = if with_outbound {
+            let (tx, rx) = mpsc::channel(16);
+            (Some(tx), Some(rx))
+        } else {
+            (None, None)
+        };
+        let task = tokio::spawn(std::future::pending::<()>());
+        *lock(&h.exec.next_stream) = Some(GrpcStreamHandle {
+            outbound,
+            events: events_rx,
+            abort: task.abort_handle(),
+        });
+        StreamFixture {
+            events_tx,
+            outbound_rx,
+            task,
+        }
+    }
+
+    async fn wait_for_tags(h: &Harness, expected: &[&str]) {
+        for _ in 0..200 {
+            if h.events.tags() == expected {
+                return;
+            }
+            tokio::time::sleep(Duration::from_millis(5)).await;
+        }
+        assert_eq!(h.events.tags(), expected);
+    }
+
+    #[tokio::test]
+    async fn a_server_stream_session_publishes_its_events_in_order_and_then_forgets_the_session() {
+        let h = harness(None);
+        let fx = prepare_stream(&h, false);
+        let mut i = input(request("demo.v1.Greeter/List"));
+        i.message = Some(r#"{"name":"n"}"#.into());
+        let id = h.svc.start_session(i).await.expect("start");
+        assert_eq!(
+            lock(&h.exec.streams)[0].1.as_deref(),
+            Some(r#"{"name":"n"}"#)
+        );
+
+        fx.events_tx
+            .send(GrpcStreamEvent::Headers(vec![]))
+            .await
+            .expect("send");
+        fx.events_tx
+            .send(GrpcStreamEvent::Message("{}".into()))
+            .await
+            .expect("send");
+        fx.events_tx
+            .send(GrpcStreamEvent::Message("{}".into()))
+            .await
+            .expect("send");
+        fx.events_tx
+            .send(GrpcStreamEvent::Finished {
+                status: GrpcStatus::ok(),
+                trailers: vec![],
+            })
+            .await
+            .expect("send");
+        wait_for_tags(
+            &h,
+            &[
+                "started:server-streaming",
+                "headers",
+                "message:0",
+                "message:1",
+                "finished:OK",
+            ],
+        )
+        .await;
+
+        let err = h
+            .svc
+            .send_message(&id, "{}")
+            .await
+            .expect_err("session is gone");
+        assert!(matches!(err, DomainError::NotFound(_)));
+    }
+
+    #[tokio::test]
+    async fn a_client_stream_message_is_resolved_validated_and_forwarded() {
+        let h = harness(None);
+        let mut fx = prepare_stream(&h, true);
+        let mut i = input(request("demo.v1.Greeter/Collect"));
+        i.variables = HashMap::from([("who".into(), "ada".into())]);
+        let id = h.svc.start_session(i).await.expect("start");
+        assert_eq!(
+            lock(&h.exec.streams)[0].1,
+            None,
+            "a client stream starts with no message"
+        );
+
+        h.svc
+            .send_message(&id, r#"{"name": "{{who}}"}"#)
+            .await
+            .expect("send");
+        let rx = fx.outbound_rx.as_mut().expect("receiver");
+        assert_eq!(rx.recv().await.as_deref(), Some(r#"{"name": "ada"}"#));
+
+        let err = h
+            .svc
+            .send_message(&id, r#"{"nope": 1}"#)
+            .await
+            .expect_err("bad message");
+        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
+        assert!(
+            rx.try_recv().is_err(),
+            "a rejected message never reaches the call"
+        );
+    }
+
+    #[tokio::test]
+    async fn ending_the_requests_closes_the_outbound_side() {
+        let h = harness(None);
+        let mut fx = prepare_stream(&h, true);
+        let id = h
+            .svc
+            .start_session(input(request("demo.v1.Greeter/Chat")))
+            .await
+            .expect("start");
+        h.svc.end_requests(&id).expect("end");
+        assert_eq!(
+            fx.outbound_rx.as_mut().expect("receiver").recv().await,
+            None
+        );
+        let err = h.svc.send_message(&id, "{}").await.expect_err("closed");
+        assert!(matches!(err, DomainError::InvalidInput(_)));
+        // The server can still answer: the session stays until it finishes.
+        fx.events_tx
+            .send(GrpcStreamEvent::Message("{}".into()))
+            .await
+            .expect("send");
+        wait_for_tags(&h, &["started:bidi-streaming", "message:0"]).await;
+    }
+
+    #[tokio::test]
+    async fn cancelling_aborts_the_call_and_reports_cancelled_exactly_once() {
+        let h = harness(None);
+        let fx = prepare_stream(&h, true);
+        let id = h
+            .svc
+            .start_session(input(request("demo.v1.Greeter/Chat")))
+            .await
+            .expect("start");
+        h.svc.cancel(&id).expect("cancel");
+        assert!(fx.task.await.expect_err("aborted").is_cancelled());
+        // A late closing event from the call must not produce a second Finished.
+        let _ = fx
+            .events_tx
+            .send(GrpcStreamEvent::Finished {
+                status: GrpcStatus::ok(),
+                trailers: vec![],
+            })
+            .await;
+        tokio::time::sleep(Duration::from_millis(50)).await;
+        assert_eq!(
+            h.events.tags(),
+            vec!["started:bidi-streaming", "finished:CANCELLED"]
+        );
+        assert!(matches!(h.svc.cancel(&id), Err(DomainError::NotFound(_))));
+    }
+
+    #[tokio::test]
+    async fn a_call_that_stops_without_a_closing_event_is_reported_as_unknown() {
+        let h = harness(None);
+        let fx = prepare_stream(&h, true);
+        h.svc
+            .start_session(input(request("demo.v1.Greeter/Chat")))
+            .await
+            .expect("start");
+        drop(fx.events_tx);
+        wait_for_tags(&h, &["started:bidi-streaming", "finished:UNKNOWN"]).await;
+    }
+
+    #[tokio::test]
+    async fn the_wrong_entry_point_for_the_call_shape_is_rejected() {
+        let h = harness(None);
+        let err = h
+            .svc
+            .start_session(input(request(SAY_HELLO)))
+            .await
+            .expect_err("unary method");
+        assert!(matches!(err, DomainError::InvalidInput(_)));
+
+        let _fx = prepare_stream(&h, false);
+        let id = h
+            .svc
+            .start_session(input(request("demo.v1.Greeter/List")))
+            .await
+            .expect("start");
+        let err = h
+            .svc
+            .send_message(&id, "{}")
+            .await
+            .expect_err("server stream takes no messages");
+        assert!(matches!(err, DomainError::InvalidInput(_)));
+    }
 }
 
```

- [ ] **Step 5: Run the tests to verify they fail, then implement the sessions**

Add the four events to `DomainEvent` in `crates/rocket-shared/src/events.rs`:

```diff
@@ -326,6 +326,35 @@
     AcpSessionFailed {
         session_id: String,
         error: String,
+    },
+
+    // gRPC session events
+    /// Emitted once a streaming gRPC call has been opened.
+    GrpcSessionStarted {
+        session_id: String,
+        /// `client-streaming`, `server-streaming` or `bidi-streaming`.
+        method_type: String,
+    },
+    /// Emitted when the response headers arrive.
+    GrpcSessionHeaders {
+        session_id: String,
+        headers: Vec<crate::grpc::GrpcMetadataPair>,
+    },
+    /// Emitted for every response message, as protobuf JSON. `index` counts from 0.
+    GrpcSessionMessage {
+        session_id: String,
+        index: u64,
+        json: String,
+    },
+    /// Emitted once when the call ends, for any reason. `code` is the gRPC status
+    /// code, and 1 (CANCELLED) when the user cancelled.
+    GrpcSessionFinished {
+        session_id: String,
+        code: i32,
+        code_name: String,
+        message: String,
+        trailers: Vec<crate::grpc::GrpcMetadataPair>,
+        duration_ms: u64,
     },
 
     // File system events
```

Run: `cargo test -j4 -p rocket-app grpc_service`
Expected: FAIL to compile (`GrpcService::new` takes four arguments, and `start_session` and the other session methods do not exist).

Apply these hunks to `crates/rocket-app/src/grpc_service.rs` (everything outside the `tests` module):

```diff
@@ -2,15 +2,21 @@
 use std::path::{Component, Path, PathBuf};
 use std::sync::{Arc, Mutex};
 use std::time::Duration;
+use std::time::Instant;
 
 use base64::engine::general_purpose::STANDARD;
 use base64::Engine;
+use rocket_collection::GrpcMethodType;
 use rocket_collection::{CollectionRepository, GrpcRequest};
 use rocket_environment::resolve;
+use rocket_grpc::{json_to_message, GrpcStreamEvent, MessageDescriptor};
 use rocket_grpc::{GrpcCall, GrpcExecutor, GrpcUnaryResponse, ProtoLoader, ProtoRegistry};
 use rocket_shared::error::{DomainError, DomainResult};
+use rocket_shared::events::{DomainEvent, EventPublisher};
 use rocket_shared::grpc::GrpcMetadataPair;
 use rocket_shared::types::Auth;
+use tokio::sync::mpsc;
+use tokio::task::AbortHandle;
 
 /// What the UI sends for one call. `request` is the editor state, which may be unsaved.
 #[derive(Debug, Clone)]
```

```diff
@@ -38,6 +44,16 @@
     }
 }
 
+struct SessionEntry {
+    outbound: Option<mpsc::Sender<String>>,
+    input: MessageDescriptor,
+    variables: HashMap<String, String>,
+    abort: AbortHandle,
+    started: Instant,
+}
+
+type Sessions = Arc<Mutex<HashMap<String, SessionEntry>>>;
+
 /// Runs gRPC calls: resolves variables and auth, finds the descriptors, and keeps
 /// the table of running streaming sessions.
 pub struct GrpcService {
```

```diff
@@ -45,6 +61,8 @@
     proto_loader: Arc<dyn ProtoLoader>,
     collection_repo: Arc<dyn CollectionRepository>,
     workspace_path: Arc<Mutex<PathBuf>>,
+    events: Arc<dyn EventPublisher>,
+    sessions: Sessions,
 }
 
 impl GrpcService {
```

```diff
@@ -53,12 +71,15 @@
         proto_loader: Arc<dyn ProtoLoader>,
         collection_repo: Arc<dyn CollectionRepository>,
         workspace_path: Arc<Mutex<PathBuf>>,
+        events: Arc<dyn EventPublisher>,
     ) -> Self {
         Self {
             executor,
             proto_loader,
             collection_repo,
             workspace_path,
+            events,
+            sessions: Arc::new(Mutex::new(HashMap::new())),
         }
     }
 
```

```diff
@@ -69,6 +90,117 @@
         let message = self.prepare_message(&input)?;
         let registry = self.registry_for(&input).await?;
         self.executor.unary(&call, &registry, &message).await
+    }
+
+    /// Opens a client-streaming, server-streaming or bidirectional call and
+    /// returns its session id. Everything the server sends arrives as
+    /// `GrpcSession*` domain events.
+    pub async fn start_session(&self, input: GrpcExecuteInput) -> DomainResult<String> {
+        let input = input.with_request_variables();
+        let call = self.prepare_call(&input, true)?;
+        let registry = self.registry_for(&input).await?;
+        let method = registry.method(&call.full_method)?;
+        let method_type = GrpcMethodType::from_streaming_flags(
+            method.is_client_streaming(),
+            method.is_server_streaming(),
+        );
+        if method_type == GrpcMethodType::Unary {
+            return Err(DomainError::InvalidInput(format!(
+                "{} is a unary method, send it as a normal call",
+                call.full_method
+            )));
+        }
+        // A server-streaming call has exactly one request. The other shapes take
+        // their messages one by one through `send_message`.
+        let initial = if method_type == GrpcMethodType::ServerStreaming {
+            Some(self.prepare_message(&input)?)
+        } else {
+            None
+        };
+        let handle = self.executor.open_stream(&call, &registry, initial).await?;
+
+        let session_id = ulid::Ulid::new().to_string();
+        let started = Instant::now();
+        lock(&self.sessions).insert(
+            session_id.clone(),
+            SessionEntry {
+                outbound: handle.outbound,
+                input: method.input(),
+                variables: input.variables.clone(),
+                abort: handle.abort,
+                started,
+            },
+        );
+        self.events.publish(DomainEvent::GrpcSessionStarted {
+            session_id: session_id.clone(),
+            method_type: method_type.as_str().to_string(),
+        });
+        tokio::spawn(forward_events(
+            Arc::clone(&self.sessions),
+            Arc::clone(&self.events),
+            session_id.clone(),
+            handle.events,
+        ));
+        Ok(session_id)
+    }
+
+    /// Sends one message on a client-streaming or bidirectional call. Variables
+    /// resolve with the values the session started with. A message that does not
+    /// fit the request type is rejected here and never reaches the server.
+    pub async fn send_message(&self, session_id: &str, json: &str) -> DomainResult<()> {
+        let (sender, input, variables) = {
+            let sessions = lock(&self.sessions);
+            let entry = sessions
+                .get(session_id)
+                .ok_or_else(|| DomainError::NotFound(format!("gRPC session '{session_id}'")))?;
+            let sender = entry.outbound.clone().ok_or_else(|| {
+                DomainError::InvalidInput(
+                    "this call does not take messages after it starts, or its request side is closed"
+                        .into(),
+                )
+            })?;
+            (sender, entry.input.clone(), entry.variables.clone())
+        };
+        let text = resolve_text(json, &variables, "the message")?;
+        json_to_message(&input, &text)?;
+        sender
+            .send(text)
+            .await
+            .map_err(|_| DomainError::Conflict("the call has already finished".into()))
+    }
+
+    /// Ends the request side of the call (half-close). The server may keep sending.
+    pub fn end_requests(&self, session_id: &str) -> DomainResult<()> {
+        let mut sessions = lock(&self.sessions);
+        let entry = sessions
+            .get_mut(session_id)
+            .ok_or_else(|| DomainError::NotFound(format!("gRPC session '{session_id}'")))?;
+        entry.outbound = None;
+        Ok(())
+    }
+
+    /// Cancels the call and reports it as finished with CANCELLED.
+    pub fn cancel(&self, session_id: &str) -> DomainResult<()> {
+        let entry = lock(&self.sessions)
+            .remove(session_id)
+            .ok_or_else(|| DomainError::NotFound(format!("gRPC session '{session_id}'")))?;
+        entry.abort.abort();
+        self.events.publish(DomainEvent::GrpcSessionFinished {
+            session_id: session_id.to_string(),
+            code: 1,
+            code_name: "CANCELLED".into(),
+            message: "cancelled by the user".into(),
+            trailers: vec![],
+            duration_ms: entry.started.elapsed().as_millis() as u64,
+        });
+        Ok(())
+    }
+
+    /// Aborts every running call. Used when the app exits.
+    pub fn end_all(&self) {
+        for (_, entry) in lock(&self.sessions).drain() {
+            entry.abort.abort();
+        }
     }
 
     /// Builds the transport call: resolved URL, metadata and auth.
```

```diff
@@ -241,6 +373,77 @@
     }
 }
 
+/// Publishes what a session's call reports until it finishes.
+async fn forward_events(
+    sessions: Sessions,
+    events: Arc<dyn EventPublisher>,
+    session_id: String,
+    mut stream: mpsc::Receiver<GrpcStreamEvent>,
+) {
+    let mut index: u64 = 0;
+    while let Some(event) = stream.recv().await {
+        match event {
+            GrpcStreamEvent::Headers(headers) => events.publish(DomainEvent::GrpcSessionHeaders {
+                session_id: session_id.clone(),
+                headers,
+            }),
+            GrpcStreamEvent::Message(json) => {
+                events.publish(DomainEvent::GrpcSessionMessage {
+                    session_id: session_id.clone(),
+                    index,
+                    json,
+                });
+                index += 1;
+            }
+            GrpcStreamEvent::Finished { status, trailers } => {
+                finish(
+                    &sessions,
+                    &events,
+                    &session_id,
+                    status.code,
+                    &status.code_name,
+                    &status.message,
+                    trailers,
+                );
+                return;
+            }
+        }
+    }
+    // The call stopped without a closing event. Report it unless a cancel already did.
+    finish(
+        &sessions,
+        &events,
+        &session_id,
+        2,
+        "UNKNOWN",
+        "the call ended unexpectedly",
+        vec![],
+    );
+}
+
+/// Publishes `GrpcSessionFinished` once. Whoever removes the session first wins.
+fn finish(
+    sessions: &Sessions,
+    events: &Arc<dyn EventPublisher>,
+    session_id: &str,
+    code: i32,
+    code_name: &str,
+    message: &str,
+    trailers: Vec<GrpcMetadataPair>,
+) {
+    let Some(entry) = lock(sessions).remove(session_id) else {
+        return;
+    };
+    events.publish(DomainEvent::GrpcSessionFinished {
+        session_id: session_id.to_string(),
+        code,
+        code_name: code_name.to_string(),
+        message: message.to_string(),
+        trailers,
+        duration_ms: entry.started.elapsed().as_millis() as u64,
+    });
+}
+
 /// Resolves `{{name}}` placeholders. A placeholder with no value is an error,
 /// because sending the literal text to a server is never what the user wants.
 fn resolve_text(text: &str, vars: &HashMap<String, String>, what: &str) -> DomainResult<String> {
```

Run: `cargo test -j4 -p rocket-app grpc_service`
Expected: PASS (22 tests).

- [ ] **Step 6: Commands, event mapping and wiring**

In `src-tauri/src/tauri_event_bus.rs`, add these arms to the `match` in `publish`, after the ACP session arms:

```rust
            // gRPC session events — each variant gets its own channel, like the ACP and Flow events.
            DomainEvent::GrpcSessionStarted { .. } => "grpc-session-started",
            DomainEvent::GrpcSessionHeaders { .. } => "grpc-session-headers",
            DomainEvent::GrpcSessionMessage { .. } => "grpc-session-message",
            DomainEvent::GrpcSessionFinished { .. } => "grpc-session-finished",
```

In `src-tauri/src/commands/grpc.rs` add the four commands:

```diff
@@ -74,6 +74,43 @@
     svc.call_unary(input).await
 }
 
+/// Opens a client-streaming, server-streaming or bidirectional call. The id it returns
+/// names the session in the `grpc-session-*` events.
+#[tauri::command]
+pub async fn grpc_start_session(
+    input: GrpcExecuteDto,
+    svc: State<'_, GrpcService>,
+    exec: State<'_, RequestExecutionService>,
+) -> Result<String, DomainError> {
+    let input = resolve_input(input, &exec).await?;
+    svc.start_session(input).await
+}
+
+#[tauri::command]
+pub async fn grpc_send_message(
+    session_id: String,
+    message: String,
+    svc: State<'_, GrpcService>,
+) -> Result<(), DomainError> {
+    svc.send_message(&session_id, &message).await
+}
+
+#[tauri::command]
+pub fn grpc_end_requests(
+    session_id: String,
+    svc: State<'_, GrpcService>,
+) -> Result<(), DomainError> {
+    svc.end_requests(&session_id)
+}
+
+#[tauri::command]
+pub fn grpc_cancel_session(
+    session_id: String,
+    svc: State<'_, GrpcService>,
+) -> Result<(), DomainError> {
+    svc.cancel(&session_id)
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
```

In `src-tauri/src/lib.rs`:
- pass the event bus as the new last argument of `GrpcService::new`:

```rust
                Arc::clone(&active_workspace_path),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );
```

- register the four commands in `generate_handler!`: `commands::grpc::grpc_start_session`, `commands::grpc::grpc_send_message`, `commands::grpc::grpc_end_requests`, `commands::grpc::grpc_cancel_session`;
- end the running calls when the app exits. In the `RunEvent::Exit` branch at the bottom of `run()`, next to the ACP cleanup, add:

```rust
                if let Some(grpc_svc) = app_handle.try_state::<rocket_app::GrpcService>() {
                    // Best-effort, like the ACP cleanup: the process is about to end.
                    grpc_svc.end_all();
                }
```

Run: `cargo check -j4 -p rocket`
Expected: PASS.
Run: `cargo test -j4 -p rocket commands::grpc`
Expected: PASS.

- [ ] **Step 7: Run the checks**

Run:
- `cargo check -j4 -p rocket-shared -p rocket-grpc -p rocket-infra -p rocket-app -p rocket`
- `cargo test -j4 -p rocket-shared`
- `cargo test -j4 -p rocket-grpc`
- `cargo test -j4 -p rocket-infra grpc::`
- `cargo test -j4 -p rocket-app grpc_service`

Expected: PASS. `TauriEventBus::publish` has an exhaustive `match`, so a missed arm fails the build.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add Cargo.lock crates/rocket-shared/src/events.rs crates/rocket-grpc crates/rocket-infra/Cargo.toml \
  crates/rocket-infra/src/grpc crates/rocket-app/src/grpc_service.rs \
  src-tauri/src/tauri_event_bus.rs src-tauri/src/lib.rs src-tauri/src/commands/grpc.rs
```

Suggested subject: `feat(grpc): run streaming gRPC calls as sessions that emit events`.

---

## Task 3: Server reflection and the descriptor cache

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-grpc/src/call.rs`
- Modify: `crates/rocket-infra/Cargo.toml`, `src/grpc/mod.rs`, `src/grpc/executor.rs`; create `src/grpc/reflection.rs`
- Modify: `crates/rocket-app/src/grpc_service.rs`
- Modify: `src-tauri/src/commands/grpc.rs`, `src/lib.rs`
- Modify: `crates/rocket-infra/CLAUDE.md`, `crates/rocket-grpc/CLAUDE.md`

**Interfaces:**
- Consumes: Tasks 1 and 2, `ProtoRegistry::from_file_descriptors` (Plan 11).
- Produces:
  - `GrpcExecutor::reflect(&self, call: &GrpcCall) -> DomainResult<ProtoRegistry>` with a default that returns `InvalidInput`. `TonicGrpcExecutor` implements it with reflection v1 and a v1alpha fallback.
  - `GrpcService::list_services(&self, input: GrpcExecuteInput, refresh: bool) -> DomainResult<Vec<GrpcServiceInfo>>`.
  - A request with no `proto_file_path` now takes its descriptors from reflection.
  - Command `grpc_list_services(input: GrpcExecuteDto, refresh: bool) -> Vec<GrpcServiceInfo>`.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

- [ ] **Step 2: Write the failing reflection tests**

In `crates/rocket-infra/Cargo.toml`, under `[dependencies]`:

```toml
tonic-reflection = { version = "0.14.6", default-features = false }
```

Create `crates/rocket-infra/src/grpc/reflection.rs` with only this test module first (the test server already serves reflection v1, v1alpha or none):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::test_server::{start, Reflection, TestServer};
    use rocket_grpc::GrpcExecutor;

    fn call(server: &TestServer) -> GrpcCall {
        GrpcCall {
            url: server.url(),
            full_method: String::new(),
            metadata: vec![],
            timeout: Some(Duration::from_secs(5)),
            tls_ca_pem: None,
        }
    }

    #[tokio::test]
    async fn reflection_v1_returns_the_same_services_as_the_proto_file() {
        let server = start(None, Reflection::V1).await;
        let registry = reflect(&call(&server)).await.expect("reflect");
        assert_eq!(registry.services(), server.registry.services());
    }

    #[tokio::test]
    async fn a_v1alpha_only_server_is_reached_by_the_fallback() {
        let server = start(None, Reflection::V1Alpha).await;
        let registry = reflect(&call(&server)).await.expect("reflect");
        assert_eq!(registry.services(), server.registry.services());
    }

    #[tokio::test]
    async fn a_server_without_reflection_gives_a_clear_not_found() {
        let server = start(None, Reflection::None).await;
        let err = reflect(&call(&server)).await.err().expect("no reflection");
        assert!(
            matches!(&err, DomainError::NotFound(m) if m.contains("reflection")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn a_reflected_registry_can_drive_a_real_call() {
        let server = start(None, Reflection::V1).await;
        let c = call(&server);
        let registry = crate::grpc::TonicGrpcExecutor
            .reflect(&c)
            .await
            .expect("reflect");
        let mut say = c.clone();
        say.full_method = "demo.greeter.v1.Greeter/SayHello".into();
        let response = crate::grpc::TonicGrpcExecutor
            .unary(&say, &registry, r#"{"name": "reflected"}"#)
            .await
            .expect("call");
        assert!(response
            .message_json
            .expect("reply")
            .contains("hello reflected"));
    }

    #[tokio::test]
    async fn an_unreachable_server_is_a_transport_error_not_a_missing_reflection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let c = GrpcCall {
            url: format!("127.0.0.1:{port}"),
            full_method: String::new(),
            metadata: vec![],
            timeout: Some(Duration::from_secs(5)),
            tls_ca_pem: None,
        };
        let err = reflect(&c).await.err().expect("closed port");
        assert!(matches!(err, DomainError::Http(_)), "{err:?}");
    }
}
```

Now that the reflection variants of the test server are used, remove the dead-code allowance in `crates/rocket-infra/src/grpc/test_server.rs`:

```diff
@@ -38,7 +38,6 @@
         .expect("fixture proto compiles")
 }
 
-#[allow(dead_code)] // V1 and V1Alpha are used by the reflection tests.
 #[derive(Clone, Copy, PartialEq, Eq)]
 pub(crate) enum Reflection {
     None,
```

- [ ] **Step 3: Run the tests to verify they fail, then implement reflection**

Add `mod reflection;` to `crates/rocket-infra/src/grpc/mod.rs`:

```diff
@@ -4,6 +4,7 @@
 mod codec;
 mod executor;
 mod proto_reader;
+mod reflection;
 
 #[cfg(test)]
 mod test_server;
```

and add the trait method to `call.rs`:

```diff
@@ -1,7 +1,7 @@
 use std::time::Duration;
 
 use async_trait::async_trait;
-use rocket_shared::error::DomainResult;
+use rocket_shared::error::{DomainError, DomainResult};
 use rocket_shared::grpc::GrpcMetadataPair;
 use serde::Serialize;
 use tokio::sync::mpsc;
```

```diff
@@ -129,6 +129,13 @@
         registry: &ProtoRegistry,
         initial_json: Option<String>,
     ) -> DomainResult<GrpcStreamHandle>;
+
+    /// Reads the descriptors a live server publishes through server reflection.
+    async fn reflect(&self, _call: &GrpcCall) -> DomainResult<ProtoRegistry> {
+        Err(DomainError::InvalidInput(
+            "this executor does not support server reflection".into(),
+        ))
+    }
 }
 
 #[cfg(test)]
```

Run: `cargo test -j4 -p rocket-infra grpc::reflection`
Expected: FAIL to compile (`reflect` is not defined in `reflection.rs`).

Put this above the test module in `crates/rocket-infra/src/grpc/reflection.rs`:

```rust
use std::collections::HashSet;
use std::time::Duration;

use prost::Message;
use prost_types::FileDescriptorProto;
use rocket_grpc::{GrpcCall, ProtoRegistry};
use rocket_shared::error::{DomainError, DomainResult};
use tonic::transport::Channel;
use tonic::{Code, Request, Status};

use super::channel::{apply_metadata, connect};

/// Reflection must answer within this time when the call sets no deadline.
const DEFAULT_REFLECTION_TIMEOUT: Duration = Duration::from_secs(15);
/// How many times to ask for imports that are still missing.
const MAX_IMPORT_ROUNDS: usize = 16;

enum Want {
    ListServices,
    Symbol(String),
    File(String),
}

enum Answer {
    Services(Vec<String>),
    Files(Vec<Vec<u8>>),
}

/// The v1 and v1alpha reflection protocols have the same messages in different
/// packages, so one macro writes the client code for both.
macro_rules! reflection_client {
    ($name:ident, $version:ident) => {
        async fn $name(
            channel: Channel,
            call: &GrpcCall,
            wants: &[Want],
        ) -> DomainResult<Result<Vec<Answer>, Status>> {
            use tonic_reflection::pb::$version::{
                server_reflection_client::ServerReflectionClient,
                server_reflection_request::MessageRequest,
                server_reflection_response::MessageResponse, ServerReflectionRequest,
            };
            let requests: Vec<ServerReflectionRequest> = wants
                .iter()
                .map(|w| ServerReflectionRequest {
                    host: String::new(),
                    message_request: Some(match w {
                        Want::ListServices => MessageRequest::ListServices(String::new()),
                        Want::Symbol(s) => MessageRequest::FileContainingSymbol(s.clone()),
                        Want::File(f) => MessageRequest::FileByFilename(f.clone()),
                    }),
                })
                .collect();
            let expected = requests.len();
            let mut request = Request::new(tokio_stream::iter(requests));
            apply_metadata(request.metadata_mut(), &call.metadata)?;
            let mut client = ServerReflectionClient::new(channel);
            let mut stream = match client.server_reflection_info(request).await {
                Ok(response) => response.into_inner(),
                Err(status) => return Ok(Err(status)),
            };
            let mut answers = Vec::new();
            while answers.len() < expected {
                match stream.message().await {
                    Ok(Some(response)) => match response.message_response {
                        Some(MessageResponse::ListServicesResponse(list)) => {
                            answers.push(Answer::Services(
                                list.service.into_iter().map(|s| s.name).collect(),
                            ));
                        }
                        Some(MessageResponse::FileDescriptorResponse(files)) => {
                            answers.push(Answer::Files(files.file_descriptor_proto));
                        }
                        Some(MessageResponse::ErrorResponse(e)) => {
                            return Ok(Err(Status::new(
                                Code::from_i32(e.error_code),
                                e.error_message,
                            )));
                        }
                        _ => {}
                    },
                    Ok(None) => break,
                    Err(status) => return Ok(Err(status)),
                }
            }
            Ok(Ok(answers))
        }
    };
}

reflection_client!(ask_v1, v1);
reflection_client!(ask_v1alpha, v1alpha);

/// Reads the service descriptors a live server publishes. Tries reflection v1,
/// then v1alpha when the server does not know v1.
pub(crate) async fn reflect(call: &GrpcCall) -> DomainResult<ProtoRegistry> {
    let limit = call.timeout.unwrap_or(DEFAULT_REFLECTION_TIMEOUT);
    tokio::time::timeout(limit, reflect_inner(call))
        .await
        .map_err(|_| DomainError::Http("server reflection timed out".into()))?
}

async fn reflect_inner(call: &GrpcCall) -> DomainResult<ProtoRegistry> {
    let channel = connect(call).await?;
    let listing = [Want::ListServices];
    let (use_v1alpha, listed) = match ask_v1(channel.clone(), call, &listing).await? {
        Ok(answers) => (false, answers),
        Err(status) if status.code() == Code::Unimplemented => {
            match ask_v1alpha(channel.clone(), call, &listing).await? {
                Ok(answers) => (true, answers),
                Err(status) => return Err(reflection_error(&status)),
            }
        }
        Err(status) => return Err(reflection_error(&status)),
    };
    let services: Vec<String> = listed
        .into_iter()
        .filter_map(|a| match a {
            Answer::Services(names) => Some(names),
            Answer::Files(_) => None,
        })
        .flatten()
        .filter(|name| !name.starts_with("grpc.reflection."))
        .collect();
    if services.is_empty() {
        return Err(DomainError::NotFound(
            "the server publishes no services through reflection".into(),
        ));
    }
    let wants: Vec<Want> = services.into_iter().map(Want::Symbol).collect();
    let mut files: Vec<FileDescriptorProto> = Vec::new();
    let mut have: HashSet<String> = HashSet::new();
    let mut batch = wants;
    // A server may send only the file that holds a symbol and not its imports, so
    // keep asking for the imports we are still missing.
    for _ in 0..MAX_IMPORT_ROUNDS {
        let answers = if use_v1alpha {
            ask_v1alpha(channel.clone(), call, &batch).await?
        } else {
            ask_v1(channel.clone(), call, &batch).await?
        }
        .map_err(|status| reflection_error(&status))?;
        for answer in answers {
            if let Answer::Files(encoded) = answer {
                for bytes in encoded {
                    let file = FileDescriptorProto::decode(bytes.as_slice()).map_err(|e| {
                        DomainError::Serialization(format!("the server sent a bad descriptor: {e}"))
                    })?;
                    if have.insert(file.name().to_string()) {
                        files.push(file);
                    }
                }
            }
        }
        let mut missing: Vec<String> = files
            .iter()
            .flat_map(|f| f.dependency.iter().cloned())
            .filter(|d| !have.contains(d) && !d.starts_with("google/protobuf/"))
            .collect();
        missing.sort();
        missing.dedup();
        if missing.is_empty() {
            break;
        }
        batch = missing.into_iter().map(Want::File).collect();
    }
    ProtoRegistry::from_file_descriptors(files)
}

fn reflection_error(status: &Status) -> DomainError {
    if status.code() == Code::Unimplemented {
        DomainError::NotFound(
            "the server does not support gRPC server reflection; choose a .proto file instead"
                .into(),
        )
    } else {
        DomainError::Http(format!(
            "server reflection failed: {:?}: {}",
            status.code(),
            status.message()
        ))
    }
}
```

and add the method to `impl GrpcExecutor for TonicGrpcExecutor` in `executor.rs`:

```diff
@@ -133,6 +133,10 @@
             events: events_rx,
             abort: task.abort_handle(),
         })
+    }
+
+    async fn reflect(&self, call: &GrpcCall) -> DomainResult<ProtoRegistry> {
+        super::reflection::reflect(call).await
     }
 }
 
```

Run: `cargo test -j4 -p rocket-infra grpc::`
Expected: PASS (31 tests, including the 5 reflection tests).

Three details are easy to get wrong and are pinned by the tests. A server may answer a symbol with only the file that defines it, so `reflect_inner` keeps asking for missing imports by file name (`a_reflected_registry_can_drive_a_real_call` fails without that loop). The listing request fails with `UNIMPLEMENTED` on a v1alpha-only server, which is the signal to retry on v1alpha. A server with no reflection at all gives a `NotFound` that tells the user to choose a `.proto` file, not a transport error.

- [ ] **Step 4: Write the failing service tests**

Add the reflection and cache tests to `crates/rocket-app/src/grpc_service.rs`. These hunks are inside its `tests` module:

```diff
@@ -486,7 +565,9 @@
 #[cfg(test)]
 mod tests {
     use std::collections::HashMap;
+    use std::sync::atomic::{AtomicUsize, Ordering};
     use std::sync::Mutex;
+    use std::time::SystemTime;
 
     use async_trait::async_trait;
     use rocket_collection::{Collection, GrpcMessage, GrpcMetadataEntry};
```

```diff
@@ -537,6 +618,7 @@
         unary: Mutex<Vec<(GrpcCall, String)>>,
         streams: Mutex<Vec<(GrpcCall, Option<String>)>>,
         next_stream: Mutex<Option<GrpcStreamHandle>>,
+        reflects: AtomicUsize,
     }
 
     #[async_trait]
```

```diff
@@ -567,6 +649,11 @@
             lock(&self.next_stream)
                 .take()
                 .ok_or_else(|| DomainError::Internal("no stream prepared".into()))
+        }
+
+        async fn reflect(&self, _call: &GrpcCall) -> DomainResult<ProtoRegistry> {
+            self.reflects.fetch_add(1, Ordering::SeqCst);
+            Ok(registry())
         }
     }
 
```

```diff
@@ -1135,5 +1222,92 @@
             .expect_err("server stream takes no messages");
         assert!(matches!(err, DomainError::InvalidInput(_)));
     }
+
+    // ---- descriptors -------------------------------------------------------
+
+    #[tokio::test]
+    async fn a_request_without_a_proto_file_uses_reflection_and_caches_the_result() {
+        let h = harness(None);
+        let mut r = request(SAY_HELLO);
+        r.proto_file_path = None;
+        h.svc.call_unary(input(r.clone())).await.expect("first");
+        h.svc.call_unary(input(r.clone())).await.expect("second");
+        assert_eq!(
+            h.exec.reflects.load(Ordering::SeqCst),
+            1,
+            "the second call reuses the descriptors"
+        );
+        assert!(lock(&h.loader.loads).is_empty());
+
+        let services = h.svc.list_services(input(r), true).await.expect("refresh");
+        assert_eq!(
+            h.exec.reflects.load(Ordering::SeqCst),
+            2,
+            "refresh asks the server again"
+        );
+        assert_eq!(services[0].name, "demo.v1.Greeter");
+    }
+
+    #[tokio::test]
+    async fn different_credentials_do_not_share_reflected_descriptors() {
+        let h = harness(None);
+        let mut r = request(SAY_HELLO);
+        r.proto_file_path = None;
+        r.auth = Auth::Bearer {
+            token: "one".into(),
+        };
+        h.svc.call_unary(input(r.clone())).await.expect("first");
+        r.auth = Auth::Bearer {
+            token: "two".into(),
+        };
+        h.svc.call_unary(input(r)).await.expect("second");
+        assert_eq!(h.exec.reflects.load(Ordering::SeqCst), 2);
+    }
+
+    #[tokio::test]
+    async fn a_changed_proto_file_is_compiled_again_but_an_unchanged_one_is_not() {
+        let h = harness(None);
+        h.svc
+            .call_unary(input(request(SAY_HELLO)))
+            .await
+            .expect("first");
+        h.svc
+            .call_unary(input(request(SAY_HELLO)))
+            .await
+            .expect("second");
+        assert_eq!(
+            lock(&h.loader.loads).len(),
+            1,
+            "an unchanged file is compiled once"
+        );
+
+        let file = h.dir.path().join("collections/api/protos/greeter.proto");
+        let handle = std::fs::OpenOptions::new()
+            .write(true)
+            .open(&file)
+            .expect("open");
+        handle
+            .set_modified(SystemTime::now() + Duration::from_secs(60))
+            .expect("touch");
+        h.svc
+            .call_unary(input(request(SAY_HELLO)))
+            .await
+            .expect("third");
+        assert_eq!(
+            lock(&h.loader.loads).len(),
+            2,
+            "a new modification time forces a new compile"
+        );
+
+        h.svc
+            .list_services(input(request(SAY_HELLO)), true)
+            .await
+            .expect("refresh");
+        assert_eq!(
+            lock(&h.loader.loads).len(),
+            3,
+            "refresh always compiles again"
+        );
+    }
 }
 
```

- [ ] **Step 5: Run the tests to verify they fail, then implement the cache and `list_services`**

Run: `cargo test -j4 -p rocket-app grpc_service`
Expected: FAIL to compile (`list_services` does not exist, and `registry_for` takes one argument).

Apply these hunks to `crates/rocket-app/src/grpc_service.rs` (everything outside the `tests` module):

```diff
@@ -1,14 +1,15 @@
 use std::collections::HashMap;
 use std::path::{Component, Path, PathBuf};
 use std::sync::{Arc, Mutex};
-use std::time::Duration;
 use std::time::Instant;
+use std::time::{Duration, SystemTime};
 
 use base64::engine::general_purpose::STANDARD;
 use base64::Engine;
 use rocket_collection::GrpcMethodType;
 use rocket_collection::{CollectionRepository, GrpcRequest};
 use rocket_environment::resolve;
+use rocket_grpc::GrpcServiceInfo;
 use rocket_grpc::{json_to_message, GrpcStreamEvent, MessageDescriptor};
 use rocket_grpc::{GrpcCall, GrpcExecutor, GrpcUnaryResponse, ProtoLoader, ProtoRegistry};
 use rocket_shared::error::{DomainError, DomainResult};
```

```diff
@@ -17,6 +18,9 @@
 use rocket_shared::types::Auth;
 use tokio::sync::mpsc;
 use tokio::task::AbortHandle;
+
+/// Descriptor caches stop growing at this many entries and start over.
+const MAX_CACHED_REGISTRIES: usize = 32;
 
 /// What the UI sends for one call. `request` is the editor state, which may be unsaved.
 #[derive(Debug, Clone)]
```

```diff
@@ -63,6 +67,7 @@
     workspace_path: Arc<Mutex<PathBuf>>,
     events: Arc<dyn EventPublisher>,
     sessions: Sessions,
+    registries: Mutex<HashMap<String, ProtoRegistry>>,
 }
 
 impl GrpcService {
```

```diff
@@ -80,6 +85,7 @@
             workspace_path,
             events,
             sessions: Arc::new(Mutex::new(HashMap::new())),
+            registries: Mutex::new(HashMap::new()),
         }
     }
 
```

```diff
@@ -88,8 +94,19 @@
         let input = input.with_request_variables();
         let call = self.prepare_call(&input, true)?;
         let message = self.prepare_message(&input)?;
-        let registry = self.registry_for(&input).await?;
+        let registry = self.registry_for(&input, false).await?;
         self.executor.unary(&call, &registry, &message).await
+    }
+
+    /// Lists the services and methods of the request's `.proto` file, or, when it
+    /// has none, of the live server through reflection. `refresh` skips the cache.
+    pub async fn list_services(
+        &self,
+        input: GrpcExecuteInput,
+        refresh: bool,
+    ) -> DomainResult<Vec<GrpcServiceInfo>> {
+        let input = input.with_request_variables();
+        Ok(self.registry_for(&input, refresh).await?.services())
     }
 
     /// Opens a client-streaming, server-streaming or bidirectional call and
```

```diff
@@ -98,7 +115,7 @@
     pub async fn start_session(&self, input: GrpcExecuteInput) -> DomainResult<String> {
         let input = input.with_request_variables();
         let call = self.prepare_call(&input, true)?;
-        let registry = self.registry_for(&input).await?;
+        let registry = self.registry_for(&input, false).await?;
         let method = registry.method(&call.full_method)?;
         let method_type = GrpcMethodType::from_streaming_flags(
             method.is_client_streaming(),
```

```diff
@@ -315,24 +332,86 @@
         }
     }
 
-    /// Finds the descriptors from the request's `.proto` file.
-    async fn registry_for(&self, input: &GrpcExecuteInput) -> DomainResult<ProtoRegistry> {
-        let raw = input
+    /// Finds the descriptors: from the `.proto` file when the request has one,
+    /// else from server reflection.
+    async fn registry_for(
+        &self,
+        input: &GrpcExecuteInput,
+        refresh: bool,
+    ) -> DomainResult<ProtoRegistry> {
+        let raw_path = input
             .request
             .proto_file_path
             .as_deref()
             .map(str::trim)
-            .filter(|p| !p.is_empty())
-            .ok_or_else(|| {
-                DomainError::InvalidInput("set a .proto file for this request".into())
-            })?;
+            .filter(|p| !p.is_empty());
+        match raw_path {
+            Some(raw) => self.registry_from_file(input, raw, refresh).await,
+            None => self.registry_from_reflection(input, refresh).await,
+        }
+    }
+
+    async fn registry_from_file(
+        &self,
+        input: &GrpcExecuteInput,
+        raw: &str,
+        refresh: bool,
+    ) -> DomainResult<ProtoRegistry> {
         let resolved = resolve_text(raw, &input.variables, "the proto file path")?;
         let (path, include_dirs) =
             self.resolve_proto_path(input.collection.as_deref(), &resolved)?;
+        let modified = std::fs::metadata(&path)
+            .and_then(|m| m.modified())
+            .ok()
+            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
+            .map(|d| d.as_nanos());
+        let key = modified.map(|nanos| format!("proto:{}:{nanos}", path.display()));
+        if let (Some(key), false) = (&key, refresh) {
+            if let Some(hit) = lock(&self.registries).get(key) {
+                return Ok(hit.clone());
+            }
+        }
         let loader = Arc::clone(&self.proto_loader);
-        tokio::task::spawn_blocking(move || loader.load(&path, &include_dirs))
+        let registry = tokio::task::spawn_blocking(move || loader.load(&path, &include_dirs))
             .await
-            .map_err(|e| DomainError::Internal(format!("proto loading stopped: {e}")))?
+            .map_err(|e| DomainError::Internal(format!("proto loading stopped: {e}")))??;
+        if let Some(key) = key {
+            self.cache(key, registry.clone());
+        }
+        Ok(registry)
+    }
+
+    async fn registry_from_reflection(
+        &self,
+        input: &GrpcExecuteInput,
+        refresh: bool,
+    ) -> DomainResult<ProtoRegistry> {
+        let call = self.prepare_call(input, false)?;
+        let key = {
+            use std::hash::{Hash, Hasher};
+            let mut hasher = std::collections::hash_map::DefaultHasher::new();
+            for pair in &call.metadata {
+                pair.name.hash(&mut hasher);
+                pair.value.hash(&mut hasher);
+            }
+            format!("reflect:{}:{:x}", call.url, hasher.finish())
+        };
+        if !refresh {
+            if let Some(hit) = lock(&self.registries).get(&key) {
+                return Ok(hit.clone());
+            }
+        }
+        let registry = self.executor.reflect(&call).await?;
+        self.cache(key, registry.clone());
+        Ok(registry)
+    }
+
+    fn cache(&self, key: String, registry: ProtoRegistry) {
+        let mut cache = lock(&self.registries);
+        if cache.len() >= MAX_CACHED_REGISTRIES {
+            cache.clear();
+        }
+        cache.insert(key, registry);
     }
 
     /// A relative path is inside the collection and may not climb out of it. An
```

Run: `cargo test -j4 -p rocket-app grpc_service`
Expected: PASS (25 tests).

The reflection cache key is the URL plus a hash of the metadata names and values, so two credentials never see each other's services. A `.proto` is cached by path and modification time. Editing an imported file does not change the entry file's time, so the UI's reload button passes `refresh = true`.

- [ ] **Step 6: Command and wiring**

Add the command to `src-tauri/src/commands/grpc.rs`:

```diff
@@ -3,7 +3,7 @@
 
 use rocket_app::{GrpcExecuteInput, GrpcService, RequestExecutionService};
 use rocket_collection::GrpcRequest;
-use rocket_grpc::GrpcUnaryResponse;
+use rocket_grpc::{GrpcServiceInfo, GrpcUnaryResponse};
 use rocket_shared::error::DomainError;
 use serde::Deserialize;
 use tauri::State;
```

```diff
@@ -111,6 +111,19 @@
     svc.cancel(&session_id)
 }
 
+/// Lists the services and methods of the request's `.proto` file, or of the live server
+/// through reflection when the request has no file. `refresh` skips the descriptor cache.
+#[tauri::command]
+pub async fn grpc_list_services(
+    input: GrpcExecuteDto,
+    refresh: bool,
+    svc: State<'_, GrpcService>,
+    exec: State<'_, RequestExecutionService>,
+) -> Result<Vec<GrpcServiceInfo>, DomainError> {
+    let input = resolve_input(input, &exec).await?;
+    svc.list_services(input, refresh).await
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
```

Register it in `generate_handler!` as `commands::grpc::grpc_list_services`.

Run: `cargo check -j4 -p rocket`
Expected: PASS.

- [ ] **Step 7: Docs**

In `crates/rocket-infra/CLAUDE.md`, add a row to the "Public types" table: `TonicGrpcExecutor` implements `rocket_grpc::GrpcExecutor` (unary, streaming sessions, reflection over tonic with rustls and the OS roots), and `FsProtoFileReader` and `FsProtoLoader` read `.proto` files with import paths confined to their include directories. Add one paragraph under "Key patterns" with the facts from this plan's "Facts found while building" list that a maintainer needs: `ready()` before every call, the client-side deadline, the printable-ASCII check, trailers through the streaming API, the reflection import loop.

In `crates/rocket-grpc/CLAUDE.md`, change the `call.rs` row so it describes what the file holds now (`GrpcCall`, `GrpcExecutor` with `unary`, `open_stream` and `reflect`, `GrpcStatus`, `GrpcUnaryResponse`, `GrpcStreamEvent`, `GrpcStreamHandle`).

- [ ] **Step 8: Manual check against a real server**

Start any gRPC server that has reflection enabled (for example `grpcurl`'s test server or a local service). Then run `yarn tauri dev` and, in the devtools console, call:

```js
await window.__TAURI_INTERNALS__.invoke('grpc_list_services', { input: { request: { uid: 'x', name: 'x', url: 'localhost:50051', methodType: 'unary', auth: { authType: 'none' } } }, refresh: true })
```

Expected: the services and methods of the server, each method with its `methodType`. Repeat with the URL of a server that has no reflection and confirm the error says to choose a `.proto` file. Plan 13 adds the UI for this.

- [ ] **Step 9: Run the checks**

Run:
- `cargo check -j4 -p rocket-grpc -p rocket-infra -p rocket-app -p rocket`
- `cargo test -j4 -p rocket-grpc`
- `cargo test -j4 -p rocket-infra grpc::`
- `cargo test -j4 -p rocket-app grpc_service`
- `cargo test -j4 -p rocket commands::grpc`

Expected: PASS.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by explicit path:

```bash
git add Cargo.lock crates/rocket-grpc crates/rocket-infra/Cargo.toml crates/rocket-infra/CLAUDE.md \
  crates/rocket-infra/src/grpc crates/rocket-app/src/grpc_service.rs \
  src-tauri/src/lib.rs src-tauri/src/commands/grpc.rs
```

Suggested subject: `feat(grpc): list services through server reflection and cache descriptors`.

---

## Known limits (state them in the PR description)

- No mutual TLS and no "skip certificate verification" for gRPC. HTTP has both through the environment's client certificates and `verifySsl`. gRPC trusts the OS roots only. A server with a self-signed certificate fails with an `UnknownIssuer` message until its CA is installed in the OS. `GrpcCall.tls_ca_pem` exists for tests and is a ready hook for a CA setting later.
- A message larger than tonic's 4 MiB decoding limit fails the call.
- Unary calls have no cancel. The UI sets a deadline instead (Plan 13).
- Auth types other than bearer, basic and API key in a header fail with a message. OAuth2 for gRPC is a follow-up.
- Compression is not enabled.
- Binary metadata (`*-bin`) is written and shown as base64 text.
- gRPC items are not Collection Runner steps and cannot be dragged into a Flow.
- `end_all` on exit is best effort.

---

## Next Plan

[Plan 13: gRPC UI and Bruno import](2026-10-05-protocol-parity-plan-13-grpc-ui-and-import.md). It depends on this plan (the commands and the `grpc-session-*` events) and on Plan 11 (the typed item). Chain to it automatically when this one finishes.
