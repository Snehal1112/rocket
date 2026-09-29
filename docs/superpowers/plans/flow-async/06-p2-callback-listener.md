# Flow Async P2 — Plan 06: Callback Listener Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `CallbackListener` port in `rocket-app`, an in-memory fake for tests, and a real `hyper` server in `rocket-infra` that serves one run-scoped `/cb/<token>` endpoint per call to `open`.

**Architecture:** `rocket-app` owns the trait and the plain data types (`ReceivedCall`, `CallbackEndpoint`), plus `NoCallbackListener` as the default. `rocket-infra` owns the I/O: a `HyperCallbackListener` that binds `0.0.0.0:0`, answers every call at once, and forwards accepted calls into a bounded `tokio::sync::mpsc` channel. Dropping the endpoint's `guard` stops the accept loop and closes the port. `src-tauri` wires the real listener into `FlowExecutionService` with `with_callback_listener`.

**Tech Stack:** Rust, tokio, hyper 1.x (`server`, `http1`), hyper-util (`tokio`), http-body-util, bytes, url, uuid, reqwest (tests).

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` §7.3. Interfaces are locked in `docs/superpowers/plans/flow-async/00-index.md` ("P2 — listener").

## Global Constraints

- Bind address: `0.0.0.0:0` (a free port), one listener per `open` call.
- Only path served: `/cb/<token>`; `<token>` is 32 random URL-safe characters per endpoint. Any other path gets `404`.
- Every accepted call is answered at once with `200` and body `{"received":true}`.
- Request body cap: 1 MB (`1_048_576` bytes). Larger calls get `413` and are not delivered.
- Channel: bounded, holds up to 100 calls. When full, the call gets `503` and is not delivered.
- URL format: `http://<host>:<port>/cb/<token>`. `<host>` is the `host` argument, else this machine's first non-loopback IPv4 address, else `127.0.0.1`.
- Dropping `guard` closes the endpoint; later calls cannot connect.
- `rocket-app` holds traits only; concrete I/O lives in `rocket-infra` (`.claude/rules/rust-ddd-boundaries.md`).
- No panicking-unwrap calls in production paths. Tests use `expect`.
- Cargo commands always use `-j4` and target one crate.

## Review Focus

1. A call with a query string and repeated headers (`/cb/<token>?a=1&b=two`) must deliver `query = [("a","1"),("b","two")]` and every header, not only the first. → Task 2, Step 1 test `delivers_method_path_query_headers_and_body`.
2. A request to the right port but the wrong token (`/cb/nope`) must get `404` and deliver nothing. → Task 2 test `wrong_token_gets_404_and_is_not_delivered`.
3. A body just over 1 MB must get `413` and deliver nothing; the listener must keep serving the next call. → Task 2 test `oversized_body_gets_413_and_listener_keeps_serving`.
4. A caller that floods an endpoint nobody reads must get `503` on call 101, not hang. → Task 2 test `full_channel_gets_503`.
5. After the guard is dropped, a new connection must fail rather than hang. → Task 2 test `dropping_the_guard_closes_the_port`.

---

### Task 1: `CallbackListener` port, `NoCallbackListener`, fake, and service hook

**Files:**
- Create: `crates/rocket-app/src/callback_listener.rs`
- Modify: `crates/rocket-app/src/lib.rs` (module + re-exports)
- Modify: `crates/rocket-app/src/test_doubles.rs` (add `FakeCallbackListener`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (field + `with_callback_listener`)

**Interfaces:**
- Consumes: nothing from earlier P2 plans. Plans 01–05 are already merged into the branch, so `FlowExecutionService` has the `cancel_handles` field from plan 01.
- Produces (locked in `00-index.md`):
  - `rocket_app::callback_listener::{ReceivedCall, CallbackEndpoint, CallbackListener, NoCallbackListener}`, re-exported from `rocket_app`.
  - `FlowExecutionService::with_callback_listener(self, listener: Box<dyn CallbackListener>) -> Self`.
  - Field `callback_listener: Box<dyn CallbackListener>` on `FlowExecutionService` (private; plan 07 reads it).
  - Test double `crate::test_doubles::FakeCallbackListener` with `new() -> Arc<Self>`, `failing(message: &str) -> Arc<Self>`, `queue_on_open(call)`, `sender(index) -> mpsc::Sender<ReceivedCall>`, `is_closed(index) -> bool`, `opened_count() -> usize`, `hosts() -> Vec<Option<String>>`, `async wait_opened(count)`; `CallbackListener` is implemented for `Arc<FakeCallbackListener>`; URLs are `http://fake:1/cb/<index>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/rocket-app/src/callback_listener.rs` with only the test module first, so the types it names do not exist yet:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::FakeCallbackListener;
    use std::sync::Arc;

    fn call(body: &str) -> ReceivedCall {
        ReceivedCall {
            method: "POST".to_string(),
            path: "/cb/0".to_string(),
            query: Vec::new(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: body.to_string(),
        }
    }

    #[tokio::test]
    async fn no_callback_listener_refuses_to_open() {
        let err = NoCallbackListener
            .open(None)
            .await
            .err()
            .expect("the default listener must not open anything");
        assert!(
            err.to_string().contains("callback listener is not configured"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn fake_listener_hands_out_numbered_urls_and_delivers_calls() {
        let fake = FakeCallbackListener::new();
        let listener: Box<dyn CallbackListener> = Box::new(Arc::clone(&fake));

        let mut first = listener.open(Some("10.0.0.5")).await.expect("open first");
        let second = listener.open(None).await.expect("open second");

        assert_eq!(first.url, "http://fake:1/cb/0");
        assert_eq!(second.url, "http://fake:1/cb/1");
        assert_eq!(fake.opened_count(), 2);
        assert_eq!(fake.hosts(), vec![Some("10.0.0.5".to_string()), None]);

        fake.sender(0).send(call("{}")).await.expect("send");
        assert_eq!(first.calls.recv().await, Some(call("{}")));
    }

    #[tokio::test]
    async fn fake_listener_reports_a_dropped_guard_as_closed() {
        let fake = FakeCallbackListener::new();
        let listener: Box<dyn CallbackListener> = Box::new(Arc::clone(&fake));
        let endpoint = listener.open(None).await.expect("open");

        assert!(!fake.is_closed(0));
        drop(endpoint);
        assert!(fake.is_closed(0), "dropping the endpoint drops its guard");
    }

    #[tokio::test]
    async fn fake_listener_delivers_queued_calls_when_an_endpoint_opens() {
        let fake = FakeCallbackListener::new();
        fake.queue_on_open(call("early"));
        let listener: Box<dyn CallbackListener> = Box::new(Arc::clone(&fake));

        let mut endpoint = listener.open(None).await.expect("open");

        assert_eq!(endpoint.calls.recv().await, Some(call("early")));
    }

    #[tokio::test]
    async fn failing_fake_listener_returns_its_error() {
        let fake = FakeCallbackListener::failing("port in use");
        let listener: Box<dyn CallbackListener> = Box::new(Arc::clone(&fake));
        let err = listener.open(None).await.err().expect("must fail");
        assert!(err.to_string().contains("port in use"), "got: {err}");
    }
}
```

Add to `crates/rocket-app/src/lib.rs`, after `pub mod assertion_evaluator;`:

```rust
pub mod callback_listener;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app callback_listener`
Expected: FAIL to compile — `cannot find type ReceivedCall`, `cannot find type NoCallbackListener`, `unresolved import crate::test_doubles::FakeCallbackListener`.

- [ ] **Step 3: Write the port**

Put this above the test module in `crates/rocket-app/src/callback_listener.rs`:

```rust
//! Port for receiving inbound HTTP callbacks during a Flow run. The
//! concrete server lives in `rocket-infra` (`HyperCallbackListener`); this
//! crate only knows the shape of an endpoint and of a received call.

use async_trait::async_trait;
use rocket_shared::error::{DomainError, DomainResult};
use serde::Serialize;

/// One inbound call to a callback endpoint, captured as plain strings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReceivedCall {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// An open endpoint. Calls arrive on `calls` in the order they were received.
pub struct CallbackEndpoint {
    pub url: String,
    pub calls: tokio::sync::mpsc::Receiver<ReceivedCall>,
    /// Dropping this closes the endpoint.
    pub guard: Box<dyn Send + Sync>,
}

#[async_trait]
pub trait CallbackListener: Send + Sync {
    /// `host` is `Flow.callback_host`; `None` means auto-detect the LAN IP.
    async fn open(&self, host: Option<&str>) -> DomainResult<CallbackEndpoint>;
}

/// Default when nothing is wired. A flow without Wait for callback nodes
/// never calls `open`, so this only fails flows that need a listener.
pub struct NoCallbackListener;

#[async_trait]
impl CallbackListener for NoCallbackListener {
    async fn open(&self, _host: Option<&str>) -> DomainResult<CallbackEndpoint> {
        Err(DomainError::Internal(
            "callback listener is not configured".to_string(),
        ))
    }
}
```

Extend the re-exports in `crates/rocket-app/src/lib.rs` (next to the other `pub use` lines):

```rust
pub use callback_listener::{CallbackEndpoint, CallbackListener, NoCallbackListener, ReceivedCall};
```

- [ ] **Step 4: Write the fake**

Append to `crates/rocket-app/src/test_doubles.rs`:

```rust
// ---------------------------------------------------------------------------
// Callback listener
// ---------------------------------------------------------------------------

use crate::callback_listener::{CallbackEndpoint, CallbackListener, ReceivedCall};
use std::sync::atomic::AtomicBool;

/// Flips its flag when dropped, so a test can see that an endpoint closed.
struct FakeGuard(Arc<AtomicBool>);

impl Drop for FakeGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct FakeEndpointState {
    sender: tokio::sync::mpsc::Sender<ReceivedCall>,
    closed: Arc<AtomicBool>,
    host: Option<String>,
}

/// In-memory `CallbackListener`. Endpoint `i` gets the URL
/// `http://fake:1/cb/<i>`; tests push calls in with `sender(i)`.
pub struct FakeCallbackListener {
    endpoints: Mutex<Vec<FakeEndpointState>>,
    fail_with: Option<String>,
    /// Calls put into the next endpoint the moment it opens, so a test can
    /// deliver a call before its node's turn.
    queued: Mutex<Vec<ReceivedCall>>,
}

impl FakeCallbackListener {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            endpoints: Mutex::new(Vec::new()),
            fail_with: None,
            queued: Mutex::new(Vec::new()),
        })
    }

    /// A listener whose every `open` fails with `message`.
    pub fn failing(message: &str) -> Arc<Self> {
        Arc::new(Self {
            endpoints: Mutex::new(Vec::new()),
            fail_with: Some(message.to_string()),
            queued: Mutex::new(Vec::new()),
        })
    }

    /// Delivers `call` into the next endpoint as soon as it opens.
    pub fn queue_on_open(&self, call: ReceivedCall) {
        self.queued.lock().expect("lock").push(call);
    }

    pub fn sender(&self, index: usize) -> tokio::sync::mpsc::Sender<ReceivedCall> {
        self.endpoints.lock().expect("lock")[index].sender.clone()
    }

    pub fn is_closed(&self, index: usize) -> bool {
        self.endpoints.lock().expect("lock")[index]
            .closed
            .load(Ordering::SeqCst)
    }

    pub fn opened_count(&self) -> usize {
        self.endpoints.lock().expect("lock").len()
    }

    pub fn hosts(&self) -> Vec<Option<String>> {
        self.endpoints
            .lock()
            .expect("lock")
            .iter()
            .map(|e| e.host.clone())
            .collect()
    }

    /// Waits until at least `count` endpoints are open, for up to 2 seconds.
    pub async fn wait_opened(&self, count: usize) {
        for _ in 0..400 {
            if self.opened_count() >= count {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("expected {count} open endpoints, found {}", self.opened_count());
    }
}

#[async_trait]
impl CallbackListener for Arc<FakeCallbackListener> {
    async fn open(&self, host: Option<&str>) -> DomainResult<CallbackEndpoint> {
        if let Some(message) = &self.fail_with {
            return Err(DomainError::Io(message.clone()));
        }
        let (sender, calls) = tokio::sync::mpsc::channel(100);
        for call in self.queued.lock().expect("lock").drain(..) {
            sender.try_send(call).expect("the fake channel has room");
        }
        let closed = Arc::new(AtomicBool::new(false));
        let mut endpoints = self.endpoints.lock().expect("lock");
        let index = endpoints.len();
        endpoints.push(FakeEndpointState {
            sender,
            closed: Arc::clone(&closed),
            host: host.map(str::to_string),
        });
        Ok(CallbackEndpoint {
            url: format!("http://fake:1/cb/{index}"),
            calls,
            guard: Box::new(FakeGuard(closed)),
        })
    }
}
```

`Ordering`, `Arc`, `Mutex`, `async_trait`, `DomainError` and `DomainResult` are already imported at the top of `test_doubles.rs`.

- [ ] **Step 5: Add the service hook**

In `crates/rocket-app/src/flow_execution_service.rs`, add the field to `FlowExecutionService` (after `in_flight`, and after plan 01's `cancel_handles`):

```rust
    /// Opens run-scoped callback endpoints for Wait for callback nodes.
    callback_listener: Box<dyn crate::callback_listener::CallbackListener>,
```

In `FlowExecutionService::new`, initialise it:

```rust
            callback_listener: Box::new(crate::callback_listener::NoCallbackListener),
```

Add the builder right after `new`:

```rust
    /// Replaces the default `NoCallbackListener`. `src-tauri` passes the
    /// real server; tests pass a `FakeCallbackListener`.
    pub fn with_callback_listener(
        mut self,
        listener: Box<dyn crate::callback_listener::CallbackListener>,
    ) -> Self {
        self.callback_listener = listener;
        self
    }
```

Add one test to the existing `tests` module of `flow_execution_service.rs`:

```rust
    #[test]
    fn with_callback_listener_replaces_the_default_listener() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        let _service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new()),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullEventPublisher),
        )
        .with_callback_listener(Box::new(Arc::clone(&fake)));
        assert_eq!(fake.opened_count(), 0, "building the service opens nothing");
    }
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app callback_listener`
Expected: PASS — 5 tests.

Run: `cargo test -j4 -p rocket-app with_callback_listener_replaces_the_default_listener`
Expected: PASS.

Run: `cargo check -j4 -p rocket --tests`
Expected: finishes with no errors (`src-tauri` still calls `FlowExecutionService::new` with three arguments).

- [ ] **Step 7: Commit**

Stage `crates/rocket-app/src/callback_listener.rs`, `crates/rocket-app/src/lib.rs`, `crates/rocket-app/src/test_doubles.rs`, `crates/rocket-app/src/flow_execution_service.rs`, then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add callback listener port`.

---

### Task 2: `HyperCallbackListener` in `rocket-infra`, wired in `src-tauri`

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`
- Create: `crates/rocket-infra/src/callback_server.rs`
- Modify: `crates/rocket-infra/src/lib.rs`
- Modify: `src-tauri/src/lib.rs:441-449`

**Interfaces:**
- Consumes: `rocket_app::{CallbackEndpoint, CallbackListener, ReceivedCall}` from Task 1.
- Produces: `rocket_infra::callback_server::HyperCallbackListener` (`new()`), constants `MAX_BODY_BYTES = 1_048_576`, `CHANNEL_CAPACITY = 100`, `TOKEN_LEN = 32`, and `pub fn detect_lan_ip() -> String`. Re-exported as `rocket_infra::HyperCallbackListener`.

- [ ] **Step 1: Add dependencies**

`hyper 1.8.1`, `hyper-util 0.1.20`, `http-body-util 0.1.3` and `bytes 1.11.1` are already in `Cargo.lock` through `reqwest`, so no new crates are downloaded. In `crates/rocket-infra/Cargo.toml`, under `[dependencies]`, after `url = "2"`:

```toml
hyper = { version = "1", features = ["server", "http1"] }
hyper-util = { version = "0.1", features = ["tokio"] }
http-body-util = "0.1"
bytes = "1"
```

`rocket-infra` does not depend on `rocket-app`, and `rocket-app` dev-depends on `rocket-infra`, so making `rocket-infra` implement `rocket_app::CallbackListener` would create a dependency cycle. `rocket-infra` therefore exposes the server with its own mirror types (`ServerCall`, `ServerEndpoint`), and a thin adapter in `src-tauri` implements the trait (Step 5). This is recorded as a contract extension in `00-index.md`.

- [ ] **Step 2: Write the failing tests**

Create `crates/rocket-infra/src/callback_server.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    async fn open_local() -> ServerEndpoint {
        HyperCallbackListener::new()
            .open(Some("127.0.0.1"))
            .await
            .expect("open a local endpoint")
    }

    #[tokio::test]
    async fn url_has_host_port_and_a_32_char_token() {
        let endpoint = open_local().await;
        let rest = endpoint
            .url
            .strip_prefix("http://127.0.0.1:")
            .expect("host prefix");
        let (port, token) = rest.split_once("/cb/").expect("/cb/ path");
        assert!(port.parse::<u16>().is_ok(), "port: {port}");
        assert_eq!(token.len(), TOKEN_LEN);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[tokio::test]
    async fn two_endpoints_get_different_tokens() {
        let a = open_local().await;
        let b = open_local().await;
        assert_ne!(a.url, b.url);
    }

    #[tokio::test]
    async fn delivers_method_path_query_headers_and_body() {
        let mut endpoint = open_local().await;
        let url = format!("{}?a=1&b=two", endpoint.url);

        let response = reqwest::Client::new()
            .post(&url)
            .header("x-event", "payment.completed")
            .header("x-multi", "one")
            .header("x-multi", "two")
            .body(r#"{"orderId":42}"#)
            .send()
            .await
            .expect("send");

        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().await.expect("body"), r#"{"received":true}"#);

        let call = endpoint.calls.recv().await.expect("a delivered call");
        assert_eq!(call.method, "POST");
        assert!(call.path.starts_with("/cb/"), "path: {}", call.path);
        assert_eq!(
            call.query,
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "two".to_string())
            ]
        );
        assert!(call
            .headers
            .contains(&("x-event".to_string(), "payment.completed".to_string())));
        let multi: Vec<&str> = call
            .headers
            .iter()
            .filter(|(k, _)| k == "x-multi")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(multi, vec!["one", "two"]);
        assert_eq!(call.body, r#"{"orderId":42}"#);
    }

    #[tokio::test]
    async fn wrong_token_gets_404_and_is_not_delivered() {
        let mut endpoint = open_local().await;
        let (base, _token) = endpoint.url.split_once("/cb/").expect("/cb/ path");

        let response = reqwest::Client::new()
            .post(format!("{base}/cb/nope"))
            .send()
            .await
            .expect("send");

        assert_eq!(response.status().as_u16(), 404);
        assert!(endpoint.calls.try_recv().is_err(), "nothing is delivered");
    }

    #[tokio::test]
    async fn oversized_body_gets_413_and_listener_keeps_serving() {
        let mut endpoint = open_local().await;
        let client = reqwest::Client::new();

        let big = vec![b'x'; MAX_BODY_BYTES + 1];
        let response = client
            .post(&endpoint.url)
            .body(big)
            .send()
            .await
            .expect("send big");
        assert_eq!(response.status().as_u16(), 413);
        assert!(endpoint.calls.try_recv().is_err());

        let response = client
            .post(&endpoint.url)
            .body("small")
            .send()
            .await
            .expect("send small");
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(endpoint.calls.recv().await.expect("call").body, "small");
    }

    #[tokio::test]
    async fn full_channel_gets_503() {
        let endpoint = open_local().await;
        let client = reqwest::Client::new();

        for i in 0..CHANNEL_CAPACITY {
            let status = client
                .post(&endpoint.url)
                .body(i.to_string())
                .send()
                .await
                .expect("send")
                .status()
                .as_u16();
            assert_eq!(status, 200, "call {i} fits in the channel");
        }
        let status = client
            .post(&endpoint.url)
            .body("one too many")
            .send()
            .await
            .expect("send")
            .status()
            .as_u16();
        assert_eq!(status, 503);
    }

    #[tokio::test]
    async fn dropping_the_guard_closes_the_port() {
        let endpoint = open_local().await;
        let url = endpoint.url.clone();
        drop(endpoint);
        // Let the accept loop observe the shutdown signal.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result = reqwest::Client::new()
            .post(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;
        assert!(result.is_err(), "a closed endpoint must refuse the connection");
    }

    #[test]
    fn detect_lan_ip_returns_an_ipv4_address() {
        let ip: std::net::Ipv4Addr = detect_lan_ip().parse().expect("an IPv4 address");
        assert!(!ip.is_unspecified());
    }
}
```

Add to `crates/rocket-infra/src/lib.rs`, after `pub mod acp_agent_client;`:

```rust
pub mod callback_server;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra callback_server`
Expected: FAIL to compile — `cannot find type ServerEndpoint`, `cannot find struct HyperCallbackListener`, `cannot find value TOKEN_LEN`.

- [ ] **Step 4: Write the server**

Put this above the test module in `crates/rocket-infra/src/callback_server.rs`:

```rust
//! Local HTTP server for Flow "Wait for callback" nodes. Each `open` binds
//! a fresh port on all interfaces and serves exactly one path,
//! `/cb/<token>`. Calls are answered at once and forwarded to a bounded
//! channel. Dropping the endpoint's guard stops the accept loop.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rocket_shared::error::{DomainError, DomainResult};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

pub const MAX_BODY_BYTES: usize = 1_048_576;
pub const CHANNEL_CAPACITY: usize = 100;
pub const TOKEN_LEN: usize = 32;

/// One inbound call. Mirrors `rocket_app::ReceivedCall` field for field;
/// `src-tauri` converts between them.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerCall {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// An open endpoint. Mirrors `rocket_app::CallbackEndpoint`.
pub struct ServerEndpoint {
    pub url: String,
    pub calls: mpsc::Receiver<ServerCall>,
    /// Dropping this closes the endpoint.
    pub guard: Box<dyn Send + Sync>,
}

/// Stops the accept loop when dropped: dropping the sender resolves the
/// loop's receiver.
struct ShutdownGuard(#[allow(dead_code)] oneshot::Sender<()>);

pub struct HyperCallbackListener;

impl HyperCallbackListener {
    pub fn new() -> Self {
        Self
    }

    pub async fn open(&self, host: Option<&str>) -> DomainResult<ServerEndpoint> {
        let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
            .await
            .map_err(|e| DomainError::Io(format!("bind callback port: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| DomainError::Io(format!("read callback port: {e}")))?
            .port();
        let token = new_token();
        let host = host
            .map(str::to_string)
            .unwrap_or_else(detect_lan_ip);
        let url = format!("http://{host}:{port}/cb/{token}");

        let (sender, calls) = mpsc::channel(CHANNEL_CAPACITY);
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        tokio::spawn(accept_loop(listener, Arc::new(format!("/cb/{token}")), sender, stop_rx));

        Ok(ServerEndpoint {
            url,
            calls,
            guard: Box::new(ShutdownGuard(stop_tx)),
        })
    }
}

impl Default for HyperCallbackListener {
    fn default() -> Self {
        Self::new()
    }
}

/// 32 alphanumeric characters from a random v4 UUID (122 random bits).
fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// This machine's first non-loopback IPv4 address, or `127.0.0.1`.
/// `connect` on a UDP socket sends no packet; it only picks the route.
pub fn detect_lan_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("192.0.2.1:80")?;
            socket.local_addr()
        })
        .ok()
        .map(|addr| addr.ip())
        .filter(|ip| ip.is_ipv4() && !ip.is_loopback() && !ip.is_unspecified())
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

async fn accept_loop(
    listener: TcpListener,
    path: Arc<String>,
    sender: mpsc::Sender<ServerCall>,
    mut stop: oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            // The guard was dropped: stop accepting. Dropping `listener`
            // at the end of this function closes the port.
            _ = &mut stop => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                let path = Arc::clone(&path);
                let sender = sender.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        handle(req, Arc::clone(&path), sender.clone())
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        }
    }
}

async fn handle(
    req: Request<Incoming>,
    path: Arc<String>,
    sender: mpsc::Sender<ServerCall>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    if req.uri().path() != path.as_str() {
        return Ok(reply(StatusCode::NOT_FOUND, "not found"));
    }
    let method = req.method().to_string();
    let request_path = req.uri().path().to_string();
    let query: Vec<(String, String)> = req
        .uri()
        .query()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();
    let headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();

    let body = match Limited::new(req.into_body(), MAX_BODY_BYTES).collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return Ok(reply(StatusCode::PAYLOAD_TOO_LARGE, "body too large")),
    };
    let call = ServerCall {
        method,
        path: request_path,
        query,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    match sender.try_send(call) {
        Ok(()) => Ok(json_reply(StatusCode::OK, r#"{"received":true}"#)),
        Err(_) => Ok(reply(StatusCode::SERVICE_UNAVAILABLE, "callback queue is full")),
    }
}

fn reply(status: StatusCode, text: &'static str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(text.as_bytes())));
    *response.status_mut() = status;
    response
}

fn json_reply(status: StatusCode, json: &'static str) -> Response<Full<Bytes>> {
    let mut response = reply(status, json);
    response.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    response
}
```

A `Limited` error is either the size limit or a broken connection. Both mean the call is not delivered, and `413` is the only status a client can still read, so both map to `413`.

Re-export at the end of the `pub use` block in `crates/rocket-infra/src/lib.rs` (or after the module list if there is none):

```rust
pub use callback_server::HyperCallbackListener;
```

- [ ] **Step 5: Wire it into `src-tauri`**

`rocket-infra` cannot depend on `rocket-app` (the dev-dependency points the other way), so the trait adapter lives in `src-tauri`. Create `src-tauri/src/callback_adapter.rs`:

```rust
//! Adapts `rocket_infra::HyperCallbackListener` to `rocket_app::CallbackListener`.

use async_trait::async_trait;
use rocket_app::{CallbackEndpoint, CallbackListener, ReceivedCall};
use rocket_infra::callback_server::{HyperCallbackListener, ServerCall};
use rocket_shared::error::DomainResult;

pub struct HyperCallbackAdapter(pub HyperCallbackListener);

fn to_received(call: ServerCall) -> ReceivedCall {
    ReceivedCall {
        method: call.method,
        path: call.path,
        query: call.query,
        headers: call.headers,
        body: call.body,
    }
}

#[async_trait]
impl CallbackListener for HyperCallbackAdapter {
    async fn open(&self, host: Option<&str>) -> DomainResult<CallbackEndpoint> {
        let mut server = self.0.open(host).await?;
        // Capacity 1: the server channel already holds up to 100 calls, so
        // the 503 limit stays close to the spec's 100 held calls.
        let (sender, calls) = tokio::sync::mpsc::channel(1);
        // Forward server calls as rocket-app calls until either side closes.
        tokio::spawn(async move {
            while let Some(call) = server.calls.recv().await {
                if sender.send(to_received(call)).await.is_err() {
                    break;
                }
            }
        });
        Ok(CallbackEndpoint {
            url: server.url,
            calls,
            guard: server.guard,
        })
    }
}
```

The forwarding task ends when the server side closes (guard dropped, accept loop and its senders gone) or when the flow drops its receiver. `tokio` is already a `src-tauri` dependency; `async-trait` is not. Add it to `src-tauri/Cargo.toml` under `[dependencies]`, next to `tokio.workspace = true`:

```toml
async-trait.workspace = true
```

Register the module in `src-tauri/src/lib.rs` next to `mod tauri_event_bus;`:

```rust
mod callback_adapter;
```

Change the construction at `src-tauri/src/lib.rs:441-449` to:

```rust
            let flow_exec_svc = rocket_app::FlowExecutionService::new(
                Box::new(rocket_infra::SharedPathFlowRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Box::new(SharedPathCollectionRepo::new(Arc::clone(
                    &active_workspace_path,
                ))),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            )
            .with_callback_listener(Box::new(callback_adapter::HyperCallbackAdapter(
                rocket_infra::HyperCallbackListener::new(),
            )));
```

Add an adapter test at the bottom of `src-tauri/src/callback_adapter.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn adapter_forwards_a_real_call_as_a_received_call() {
        let adapter = HyperCallbackAdapter(HyperCallbackListener::new());
        let mut endpoint = adapter.open(Some("127.0.0.1")).await.expect("open");

        let status = reqwest::Client::new()
            .put(&endpoint.url)
            .body("done")
            .send()
            .await
            .expect("send")
            .status()
            .as_u16();

        assert_eq!(status, 200);
        let call = endpoint.calls.recv().await.expect("call");
        assert_eq!(call.method, "PUT");
        assert_eq!(call.body, "done");
    }
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra callback_server`
Expected: PASS — 8 tests.

Run: `cargo test -j4 -p rocket adapter_forwards_a_real_call_as_a_received_call`
Expected: PASS.

Run: `cargo check -j4 -p rocket --tests`
Expected: no errors.

- [ ] **Step 7: Commit**

Stage `crates/rocket-infra/Cargo.toml`, `Cargo.lock`, `crates/rocket-infra/src/callback_server.rs`, `crates/rocket-infra/src/lib.rs`, `src-tauri/src/callback_adapter.rs`, `src-tauri/src/lib.rs` (and `src-tauri/Cargo.toml` if changed), then commit with the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): serve callback endpoints over local HTTP`.
