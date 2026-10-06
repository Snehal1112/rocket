# Protocol parity, Plan 08: WebSocket backend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn WebSocket from a hidden raw-YAML passthrough into a real feature on the backend: a typed `WebSocketRequest` that loads, saves and renames, a `WebSocketClient` that can connect (headers, auth, TLS, timeout, keep-alive ping), send text and binary frames and close, and a session service plus Tauri commands (`ws_connect`, `ws_send`, `ws_disconnect`) that push inbound frames and lifecycle changes to the frontend as `ws:message` and `ws:status` events.

**Architecture:** Three layers, each in the crate that already owns that kind of work.

1. `rocket-collection` gets `WebSocketRequest` and `CollectionItem::WebSocket(Box<WebSocketRequest>)`, which replaces `OpaqueItem` for `info.type: websocket`. It reuses the `RequestKind` discriminator, `RequestSummary.kind` and the defaulted `CollectionRepository::request_kind` that [Plan 05](2026-10-05-protocol-parity-plan-05-graphql-model-and-import.md) adds, and follows Plan 05's pattern exactly: the sidebar gets a `RequestSummary` with `kind: WebSocket`, a full load gets the typed item, and the repository trait gains defaulted `get_websocket_request` and `save_websocket_request` next to Plan 05's `get_graphql_request` and `save_graphql_request`. `rocket-infra` converts both ways against the existing `OcWebSocketRequest` structs (persistence stays OpenCollection YAML).
2. `rocket-http` gets the `WebSocketClient` trait (next to `HttpExecutor`). A handle is two channels, not a stream object, so reading and sending never contend for one `&mut`. `rocket-infra` implements it with `tokio-tungstenite`; a spawned pump task owns the socket.
3. `rocket-app` gets `WebSocketService`, a session registry keyed by a frontend-chosen session id, and a `resolve_websocket` step on `RequestExecutionService` that applies `{{variables}}`, collection defaults and auth. `TauriEventBus` maps two new `DomainEvent` variants to the `ws:message` and `ws:status` Tauri events.

**Tech Stack:** Rust, `tokio`, `tokio-tungstenite 0.30` with `native-tls`, `async-trait`, Tauri commands, `tempfile` and a local in-test `tokio-tungstenite` server (no network).

**Spec:** [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) section 2.6 `WebSocketRequest` and `WebSocketMessage` (`type: text | json | xml | binary`, `message` single or an array of `{ title, selected, message }`, `settings.timeout` and `settings.keepAliveInterval` as number or `"inherit"`). Behaviour reference: https://docs.usebruno.com/send-requests/websocket/overview (connect and disconnect, several saved messages per request, live message log, headers and auth on the handshake).

**Depends on:** [Plan 05](2026-10-05-protocol-parity-plan-05-graphql-model-and-import.md) **Task 1** (`RequestKind`, `RequestSummary.kind`, `CollectionRepository::request_kind`, the kind-aware `rename_request`, `update_request_docs` and request-variable helpers, and the rewritten opaque-loading tests). Tasks 2 and 3 of this plan do not need Plan 05. Plans 09 (UI and import) and 10 (GraphQL subscriptions) build on this plan.

**Delta on Plan 05 Task 1 (read this first).** Plan 05 changes the same files and the same `match` statements. This plan does **not** redo any of that; it adds the WebSocket arm next to Plan 05's GraphQL arm, and edits the tests Plan 05 already rewrote:

| Plan 05 change | What this plan adds on top |
|---|---|
| `RequestKind { Http, GraphQl, Grpc, WebSocket }`, `RequestSummary.kind` | Nothing new. `RequestKind::WebSocket` already exists and `FsCollectionRepo::request_kind` already detects a `websocket:` key. |
| Defaulted `get_graphql_request`, `save_graphql_request`, `request_kind` | Parallel defaulted `get_websocket_request`, `save_websocket_request`. `request_kind` is reused as is. |
| `load_request_summary` returns a GraphQL summary, `Ok(None)` for gRPC, WebSocket and script files | WebSocket now returns a summary with `kind: WebSocket` too. gRPC and script files keep `Ok(None)`. |
| `rename_request` and `update_request_docs` try `request_kind == GraphQl` first | One more branch for `WebSocket`, built on the same `request_kind` call. |
| `runtime_variables_of` and `with_runtime_variables` in `fs_collection/variables.rs` (HTTP and GraphQL) | A third parse attempt for WebSocket. |
| `build_folder_tree_loads_non_http_items_as_opaque` expects only `("grpc", "Get User")` opaque at the root | Its `realtime` folder assertions (WebSocket) change to a typed item (Step 14). |
| `schema_shape_tests.rs` loop with a `GraphQl` arm and an `OpaqueItem` arm that still handles `"websocket"` | The `"websocket"` opaque arm is replaced by a `CollectionItem::WebSocket` arm (Step 14). |
| `get_summaries_skips_grpc_items_without_error` | Unchanged. |
| `OcGraphQLRequest.uid` added, `"GraphQLRequest.uid"` in `KNOWN_DEFERRED` | `OcWebSocketRequest` has no `uid` either; Step 10 adds it and `"WebSocketRequest.uid"` goes into `KNOWN_DEFERRED`. |

## Facts verified against the repo (do not re-derive)

- `OcWebSocketRequest` (`crates/rocket-infra/src/oc/websocket.rs`) already models `info`, `websocket { url, headers, message, auth }`, `runtime { variables, scripts, auth }`, `settings { timeout, keepAliveInterval }`, `docs`. It has **no `uid` field**. `OcHttpRequest` has a top-level `uid: Option<String>`, which the schema-shape test tracks as the known deviation `"HttpRequest.uid"` in `KNOWN_DEFERRED`. WebSocket follows the same convention and is added there as `"WebSocketRequest.uid"`.
- `OcItem` is `#[serde(untagged)]` with order `Http, GraphQL, Grpc, WebSocket, Folder, ScriptFile` (`oc/folder.rs:59`). `oc_item_to_collection_item` (`conversions/folder.rs:15`) turns WebSocket into `CollectionItem::OpaqueItem` today (Plan 05 changes only the GraphQL arm). `folder_to_oc_folder` (`:81`) and `collection_to_oc_collection` (`:191`) turn an opaque item back by re-parsing its raw YAML.
- The sidebar loads `get_summaries`. `build_folder_tree_summaries` (`fs_collection/tree.rs`) calls `load_request_summary`, which returns `Ok(None)` for every non-HTTP item (Plan 05 makes GraphQL the first exception), so WebSocket items are dropped from the sidebar payload entirely. That, not only the frontend `item.type === 'opaque'` guards, is why they are hidden. The full loader `build_folder_tree` keeps them as opaque items. Both paths change here.
- `CollectionRepository` has 11 implementors besides the two real ones (test doubles in `rocket-app`). New trait methods therefore get **default bodies that return `DomainError::InvalidInput`**, so no double needs touching.
- `CollectionService::rename_request` (`collection_service.rs`) calls `repo.get_request`, which parses an HTTP file and would fail on a WebSocket file. Plan 05 makes it (and `update_request_docs`) branch on `repo.request_kind(...)` for GraphQL; this plan adds the WebSocket branch. `get_request_variables` and `save_request_variables` (`fs_collection/variables.rs`) parse the file as HTTP and have the same problem; Plan 05 adds `runtime_variables_of` and `with_runtime_variables` helpers for GraphQL and this plan adds WebSocket to them.
- A save through the IPC payload must not clobber runtime variables edited elsewhere. `save_request` preserves on-disk `runtime.variables` when the payload has none; `save_graphql_request` (Plan 05) does the same. `save_websocket_request` follows the rule, and the frontend never sends `variables` for a WebSocket tab.
- `DomainEvent` is `#[serde(tag = "type", rename_all = "camelCase")]`. `rename_all` renames variants only, so event **fields stay snake_case** on the wire (`session_id`, as `AcpSessionChunk` already does and as `src/lib/tauri-api.ts` `AgentSessionChunkEvent` mirrors). `TauriEventBus::publish` (`src-tauri/src/tauri_event_bus.rs`) has an exhaustive `match`, so a new variant fails to compile until it is mapped. Tauri event names may contain `:`.
- HTTP auth is applied inside `ReqwestExecutor::apply_auth`, not in `rocket-app`. A WebSocket handshake never goes through it, so Task 3 turns auth into headers or a query parameter itself.
- `RequestExecutionService` (`execution_service.rs`) is managed in Tauri as a plain `State<'_, RequestExecutionService>` (not `Arc`). `build_variable_context(...)` and `resolve_external_secrets(...)` are `pub`; `merge_auth`, `resolve_auth`, `merge_headers` and the `collection_repo` field are private to that module. A **child module** of `execution_service` can reach all of them, so the new resolution code lives in `crates/rocket-app/src/execution_service/websocket_resolution.rs` and `execution_service.rs` only gains one `mod` line.
- The request-mutation host guard (`request_guard.rs`) only checks URLs a script mutates. WebSocket has no scripts in this plan, so the guard does not apply. State this in the PR description.
- `rocket-http` and `rocket-app` already depend on `tokio` with the `full` feature (workspace). `reqwest` uses **`native-tls`** (`Cargo.toml:35`). `Cargo.lock` has no `tungstenite` or `tokio-tungstenite` yet; it already has `native-tls 0.2.18`, `tokio-native-tls 0.3.1` and `futures-util 0.3.32`.
- `crates/rocket-app/src/test_doubles.rs` (`pub(crate)`, `cfg(test)`) provides `InMemoryCollectionRepo::new(Collection) -> Arc<Self>`, `SharedCollectionRepo(Arc<..>)`, `StaticEnvRepo(Environment)`, `RecordingExecutor::new()`, `InMemoryHistoryRepo::new()`, `SharedHistoryRepo`, `NullCookieRepo`, `EmptySecretManagerRepo`, `RecordingPublisher::new() -> Arc<Self>` (`events()` returns a clone).

## TLS decision

`tokio-tungstenite = { version = "0.30", default-features = false, features = ["connect", "native-tls"] }`.

- Use **native-tls**, not rustls. `reqwest` is already built with `native-tls`, so HTTP and WebSocket validate certificates through the same OS trust store, and a corporate root installed in the OS works for both. rustls would add a second TLS stack (`rustls 0.23` is only in the lock transitively) with its own root store and its own surprises.
- `native-tls` is added as a direct dependency of `rocket-infra` (it resolves to the same `0.2.18` already in the lock) because disabling certificate verification needs `native_tls::TlsConnector::builder()`.
- Verified against the downloaded `tokio-tungstenite 0.30.0` source: `connect_async_tls_with_config(request, config, disable_nagle, Option<Connector>)` and `Connector::NativeTls(native_tls::TlsConnector)` exist behind the `native-tls` feature; `tungstenite 0.30` `Message` is `Text(Utf8Bytes) | Binary(Bytes) | Ping(Bytes) | Pong(Bytes) | Close(Option<CloseFrame>) | Frame(_)`.
- TLS handshakes cannot be tested without a certificate fixture. The plan tests plain `ws://` end to end and tests the connector factory (`verify_ssl` true gives the default connector, false gives a permissive one). A real `wss://` handshake is a manual check, listed in Known limits.

## Behaviour decisions baked in

- **Session ids come from the frontend.** `ws_connect(session_id, input)`. The pump publishes `Connecting` and the first frames before the invoke returns, so the frontend must already know the id to route them.
- **Timeouts.** `timeout` is the connect and handshake timeout in milliseconds: absent or `"inherit"` means 30 000, `0` means none. `keepAliveInterval` is milliseconds between client pings: absent, `"inherit"` or `0` means no pings. No frame-idle timeout.
- **Message kinds.** `json` and `xml` are composer hints. All three text kinds go on the wire as text frames. `binary` data is stored and sent as base64.
- **Auth.** Basic, Bearer and API key (header or query) are supported. OAuth 2, OAuth 1, Digest, NTLM, WSSE and AWS SigV4 return an explicit `InvalidInput` error, never a silent unauthenticated connect.
- **Reserved headers.** `Host`, `Connection`, `Upgrade`, `Sec-WebSocket-Key`, `-Version` and `-Extensions` are rejected by name. `Sec-WebSocket-Protocol` is merged into the subprotocol list. Plan 10 relies on `subprotocols`.
- **External secrets.** Connect resolves them with the strict `resolve_external_secrets` (any failing binding fails the connect). The HTTP path tolerates a failing binding the request does not reference; WebSocket does not yet.

## Global Constraints

- Always pass `-j4` to cargo: `cargo test -j4 -p <crate> <name>`, `cargo check -j4`. Never run `cargo test --workspace`.
- Commit with the `dev-workflow-skills:1-git-commit` skill (never a freeform `git commit -m`), conventional subjects, staging by explicit path only (peer sessions share this repo's index). Never stage a file this plan does not name.
- Production code never uses `unwrap()` or `expect()`. Tests may use `.expect("reason")`.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs and the domain types that already follow that convention (`Request` does). Never on `Oc*` persistence structs. `DomainEvent` payload fields stay snake_case.
- Errors and logs name header **names**, never header values, tokens or full URLs.
- Plan 05 (GraphQL) edits the same `match` statements and the same test files, and the gRPC plan will too. Keep the `OpaqueItem` arms and touch only the WebSocket lines so the merges stay trivial.
- Verification before each commit: `cargo check -j4`, the focused `cargo test -j4 -p <crate> <name>` named in the step, and `cargo clippy -j4 -p <crate> -- -D warnings` for crates you changed.

## Review Focus

1. **A save must not lose fields the UI does not edit, nor erase variables edited on their own path.** Description, `seq`, tags, scripts and `runtime.auth` have no editor yet. Runtime variables are edited by `save_request_variables`, so a payload with none must keep the file's. Pinned by `websocket_roundtrip_through_the_repo_preserves_every_field` and `saving_a_websocket_request_keeps_runtime_variables_edited_on_their_own_path` (Task 1).
2. **A single untitled message stays a `message:` mapping; several become variants.** Writing the wrong shape breaks other OpenCollection tools. Pinned by `single_untitled_message_is_written_in_the_single_form` and `several_messages_are_written_as_variants` (Task 1).
3. **Secrets never reach an error string.** A refused connection and a rejected handshake must not echo `Authorization` or the URL query. Pinned by `connection_errors_never_contain_header_values` (Task 2).
4. **One terminal event, and the session is freed.** A peer close, an error, or a dropped handle each publish exactly one `Closed` or `Failed` status, and the id can be reused afterwards. Pinned by `peer_close_publishes_one_terminal_status_and_frees_the_session` (Task 3) and `dropping_the_outbound_sender_closes_the_socket` (Task 2).
5. **Disconnect while still connecting must not leave an open socket.** Pinned by `disconnect_during_connect_cancels_and_closes_the_late_socket` (Task 3).

---

## File Structure

| File | Task | Change |
|---|---|---|
| `crates/rocket-collection/src/websocket.rs` | 1 | create: domain types |
| `crates/rocket-collection/src/lib.rs` | 1 | `pub mod websocket;` and re-exports |
| `crates/rocket-collection/src/folder.rs` | 1 | `CollectionItem::WebSocket`, `request_count` arm |
| `crates/rocket-collection/src/repository.rs` | 1 | `get_websocket_request`, `save_websocket_request` (default bodies, next to Plan 05's GraphQL ones) |
| `crates/rocket-infra/src/oc/websocket.rs` | 1 | add `uid` to `OcWebSocketRequest`, `Default` on its runtime |
| `crates/rocket-infra/src/conversions/websocket.rs` | 1 | create: both conversions plus unit tests |
| `crates/rocket-infra/src/conversions/mod.rs`, `folder.rs` | 1 | export, three `match` arms |
| `crates/rocket-infra/src/fs_collection/{mod,requests,tree,variables}.rs` | 1 | trait impl, read/write, tree and summary loading, request-variable helpers |
| `crates/rocket-infra/src/shared_path_collection_repo.rs` | 1 | delegate the two new methods |
| `crates/rocket-infra/src/fs_collection/{tests,schema_shape_tests}.rs` | 1 | update opaque-WebSocket tests, add new ones |
| `crates/rocket-app/src/{collection_service,runner_sequence,contract_service}.rs` | 1 | service methods, kind-aware rename and docs, exhaustive-match arms |
| `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs` | 1 | `get_websocket_request`, `save_websocket_request` |
| `crates/rocket-http/src/websocket.rs`, `lib.rs` | 2 | create: trait and value types |
| `crates/rocket-infra/Cargo.toml` | 2 | new dependencies |
| `crates/rocket-infra/src/websocket_client.rs`, `lib.rs` | 2 | create: `TungsteniteWebSocketClient` plus tests |
| `crates/rocket-shared/src/events.rs` | 3 | three enums, two `DomainEvent` variants |
| `crates/rocket-app/src/execution_service/websocket_resolution.rs` | 3 | create: input DTOs, `resolve_websocket`, `resolve_websocket_message` |
| `crates/rocket-app/src/execution_service.rs`, `lib.rs` | 3 | `mod` line, re-exports |
| `crates/rocket-app/src/websocket_service.rs` | 3 | create: `WebSocketService` plus tests |
| `src-tauri/src/commands/websocket.rs`, `mod.rs`, `lib.rs`, `tauri_event_bus.rs` | 3 | commands, wiring, event names |

---

## Task 1: Typed `WebSocketRequest`, `CollectionItem::WebSocket` and persistence

**Files:**
- Create: `crates/rocket-collection/src/websocket.rs`, `crates/rocket-infra/src/conversions/websocket.rs`
- Modify: `crates/rocket-collection/src/{lib,folder,repository}.rs`; `crates/rocket-infra/src/oc/websocket.rs`; `crates/rocket-infra/src/conversions/{mod,folder}.rs`; `crates/rocket-infra/src/fs_collection/{mod,requests,tree,variables,tests,schema_shape_tests}.rs`; `crates/rocket-infra/src/shared_path_collection_repo.rs`; `crates/rocket-app/src/{collection_service,runner_sequence,contract_service}.rs`; `src-tauri/src/commands/collections.rs`; `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `OcWebSocketRequest` and friends, `persisted_oc_auth`, `OcAuth::from(Auth)`, `Auth::from(OcAuth)`, `Header::from(OcHttpRequestHeader)`, `OcVariable::from(CollectionVariable)`, `request_filename_for`, `atomic_write`, `generate_uid`; from Plan 05: `RequestKind`, `RequestSummary.kind`, `CollectionRepository::request_kind`, `runtime_variables_of`, `with_runtime_variables`.
- Produces:
  - `rocket_collection::websocket::{WebSocketRequest, WebSocketMessage, WebSocketMessageKind, WebSocketScript, WebSocketSettings}`, re-exported at the crate root.
  - `CollectionItem::WebSocket(Box<WebSocketRequest>)`, serialized with `"type": "websocket"`.
  - `CollectionRepository::get_websocket_request(&self, collection: &str, path: &str) -> DomainResult<WebSocketRequest>` and `save_websocket_request(&self, collection: &str, path: &str, request: &WebSocketRequest) -> DomainResult<String>`.
  - `rocket_infra::conversions::{oc_websocket_to_request, websocket_to_oc_websocket, with_file_identity}` (crate-internal).
  - A sidebar `RequestSummary { kind: RequestKind::WebSocket, method: "GET", .. }` for every WebSocket file in `get_summaries`.
  - `CollectionService::{get_websocket_request, save_websocket_request}`, and WebSocket-aware `rename_request`, `update_request_docs`, `get_request_variables`, `save_request_variables`.
  - Tauri commands `get_websocket_request(collection, path)` and `save_websocket_request(collection, path, request)`.

### Step group A: domain type

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing domain tests**

Create `crates/rocket-collection/src/websocket.rs` containing only this test module first, so it fails to compile:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::folder::{CollectionItem, Folder};

    #[test]
    fn websocket_item_serializes_with_the_websocket_type_tag() {
        let mut ws = WebSocketRequest::new("Chat", "wss://chat.example.com/ws");
        ws.messages.push(WebSocketMessage {
            title: "hello".into(),
            selected: true,
            kind: WebSocketMessageKind::Json,
            data: "{}".into(),
        });
        let item = CollectionItem::WebSocket(Box::new(ws));

        let value = serde_json::to_value(&item).expect("serialize");
        assert_eq!(value["type"], "websocket");
        assert_eq!(value["name"], "Chat");
        assert_eq!(value["url"], "wss://chat.example.com/ws");
        assert_eq!(value["messages"][0]["kind"], "json");

        let back: CollectionItem = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, item);
    }

    #[test]
    fn a_frontend_payload_with_only_the_required_fields_deserializes() {
        let json = serde_json::json!({
            "uid": "u1",
            "name": "Chat",
            "url": "wss://chat.example.com/ws"
        });
        let ws: WebSocketRequest = serde_json::from_value(json).expect("deserialize");
        assert!(ws.headers.is_empty());
        assert!(ws.messages.is_empty());
        assert_eq!(ws.auth, rocket_shared::types::Auth::None);
        assert!(ws.settings.is_none());
    }

    #[test]
    fn message_kind_parses_the_opencollection_type_strings() {
        assert_eq!(WebSocketMessageKind::parse("json"), Some(WebSocketMessageKind::Json));
        assert_eq!(WebSocketMessageKind::parse(" XML "), Some(WebSocketMessageKind::Xml));
        assert_eq!(WebSocketMessageKind::parse("binary"), Some(WebSocketMessageKind::Binary));
        assert_eq!(WebSocketMessageKind::parse("graphql"), None);
        assert_eq!(WebSocketMessageKind::Text.as_str(), "text");
    }

    #[test]
    fn websocket_items_count_as_requests_like_graphql_items() {
        let mut folder = Folder::new("root");
        folder
            .items
            .push(CollectionItem::WebSocket(Box::new(WebSocketRequest::new("Chat", "ws://x"))));
        assert_eq!(folder.request_count(), 1);
    }
}
```

- [ ] **Step 3: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-collection websocket`
Expected: compile error, `WebSocketRequest` and `CollectionItem::WebSocket` not found (the module is not declared yet either).

- [ ] **Step 4: Implement the domain type**

Prepend to `crates/rocket-collection/src/websocket.rs` (above the test module):

```rust
//! WebSocket request definition. Pure domain type, no I/O.

use rocket_shared::description::Description;
use rocket_shared::types::{Auth, Header, RequestSettingValue};
use serde::{Deserialize, Serialize};

use crate::settings::CollectionVariable;

/// How the composer treats a message. `Json` and `Xml` are editor hints; all
/// three text kinds go on the wire as text frames. `Binary` data is base64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSocketMessageKind {
    #[default]
    Text,
    Json,
    Xml,
    Binary,
}

impl WebSocketMessageKind {
    /// The OpenCollection `type` string for this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Binary => "binary",
        }
    }

    /// Parses an OpenCollection `type` string. Unknown values return `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            "binary" => Some(Self::Binary),
            _ => None,
        }
    }
}

/// One saved message. A request keeps several; `selected` marks the one the
/// composer sends. An empty `title` is how a lone, untitled message is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketMessage {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub kind: WebSocketMessageKind,
    #[serde(default)]
    pub data: String,
}

/// A script block carried through unchanged. WebSocket requests do not run
/// scripts yet, but a load and save must not drop them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketScript {
    pub script_type: String,
    pub code: String,
}

/// Per-request settings. Both values are milliseconds or `"inherit"`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<RequestSettingValue<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_interval: Option<RequestSettingValue<f64>>,
}

fn default_auth() -> Auth {
    Auth::None
}

/// A saved WebSocket request definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketRequest {
    #[serde(default = "crate::generate_uid")]
    pub uid: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub messages: Vec<WebSocketMessage>,
    #[serde(default = "default_auth")]
    pub auth: Auth,
    /// `runtime.auth` from the file, kept apart from `auth` like `Request` does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_auth: Option<Auth>,
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,
    #[serde(default)]
    pub scripts: Vec<WebSocketScript>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<WebSocketSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    /// On-disk filename. `None` until loaded from or saved to disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}

impl WebSocketRequest {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            uid: crate::generate_uid(),
            name: name.into(),
            description: None,
            seq: None,
            tags: Vec::new(),
            url: url.into(),
            headers: Vec::new(),
            messages: Vec::new(),
            auth: Auth::None,
            runtime_auth: None,
            variables: Vec::new(),
            scripts: Vec::new(),
            settings: None,
            docs: None,
            file_name: None,
        }
    }

    /// The message the composer sends: the first one marked `selected`, else the first.
    pub fn selected_message(&self) -> Option<&WebSocketMessage> {
        self.messages
            .iter()
            .find(|m| m.selected)
            .or_else(|| self.messages.first())
    }
}
```

- [ ] **Step 5: Add the tree variant, the count arm and the re-exports**

In `crates/rocket-collection/src/lib.rs` add `pub mod websocket;` after `pub mod summary;` and, after the `pub use summary::CollectionSummary;` line:

```rust
pub use websocket::{
    WebSocketMessage, WebSocketMessageKind, WebSocketRequest, WebSocketScript, WebSocketSettings,
};
```

In `crates/rocket-collection/src/folder.rs` add `use crate::websocket::WebSocketRequest;` at the top, then the variant after `OpaqueItem`:

```rust
    /// A typed WebSocket request (`info.type: websocket`). Replaces the opaque
    /// passthrough for this protocol only; GraphQL and gRPC stay opaque here.
    #[serde(rename = "websocket")]
    WebSocket(Box<WebSocketRequest>),
```

and in `request_count` add the arm next to Plan 05's `GraphQl(_) => 1` one (a WebSocket request is a saved request, so it counts, like a GraphQL one):

```rust
                CollectionItem::WebSocket(_) => 1,
```

Plan 05 already changed the `OpaqueItem` doc to "gRPC, WebSocket"; change it to "gRPC" and list `WebSocket` as a `CollectionItem` variant in the `CollectionItem` and `OpaqueProtocolItem.raw` bullets of `crates/rocket-collection/CLAUDE.md`. Do not add a second discriminator: `RequestKind::WebSocket` from Plan 05 is the one.

- [ ] **Step 6: Add the repository methods with default bodies**

In `crates/rocket-collection/src/repository.rs` Plan 05 has already extended the `rocket_shared::error` import with `DomainError`; add `use crate::websocket::WebSocketRequest;` and add inside the trait, after Plan 05's `request_kind` default (`request_kind` is reused unchanged and already answers `RequestKind::WebSocket` for a `websocket:` file in `FsCollectionRepo`):

```rust
    /// Read one WebSocket request file. Repositories that do not store
    /// WebSocket requests keep this default.
    fn get_websocket_request(&self, _collection: &str, _path: &str) -> DomainResult<WebSocketRequest> {
        Err(DomainError::InvalidInput(
            "websocket requests are not supported by this repository".into(),
        ))
    }

    /// Save one WebSocket request. Returns the filename actually written.
    fn save_websocket_request(
        &self,
        _collection: &str,
        _path: &str,
        _request: &WebSocketRequest,
    ) -> DomainResult<String> {
        Err(DomainError::InvalidInput(
            "websocket requests are not supported by this repository".into(),
        ))
    }
```

- [ ] **Step 7: Run the domain tests**

Run: `cargo test -j4 -p rocket-collection websocket`
Expected: 4 passed. Then `cargo check -j4 -p rocket-collection`. Other crates stay red until the arms below are added; that is expected.

### Step group B: persistence

- [ ] **Step 8: Write the failing conversion tests**

Create `crates/rocket-infra/src/conversions/websocket.rs` containing only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::settings::CollectionVariable;
    use rocket_shared::description::Description;
    use rocket_shared::types::{Auth, Header, RequestSettingValue};

    fn full_request() -> WebSocketRequest {
        let mut ws = WebSocketRequest::new("Chat", "wss://chat.example.com/ws");
        ws.uid = "ws-uid-1".into();
        ws.description = Some(Description::text("Team chat"));
        ws.seq = Some(3);
        ws.tags = vec!["realtime".into()];
        ws.headers = vec![Header::new("Origin", "https://example.com"), Header::disabled("X-Off", "1")];
        ws.messages = vec![
            WebSocketMessage {
                title: "ping".into(),
                selected: false,
                kind: WebSocketMessageKind::Text,
                data: "ping".into(),
            },
            WebSocketMessage {
                title: "bytes".into(),
                selected: true,
                kind: WebSocketMessageKind::Binary,
                data: "AQID".into(),
            },
        ];
        ws.auth = Auth::Bearer { token: "{{token}}".into() };
        ws.runtime_auth = Some(Auth::Basic { username: "u".into(), password: "p".into() });
        ws.variables = vec![CollectionVariable {
            key: "room".into(),
            value: "general".into(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }];
        ws.scripts = vec![WebSocketScript {
            script_type: "before-request".into(),
            code: "// pre".into(),
        }];
        ws.settings = Some(WebSocketSettings {
            timeout: Some(RequestSettingValue::Value(5000.0)),
            keep_alive_interval: Some(RequestSettingValue::Inherit("inherit".into())),
        });
        ws.docs = Some("# Chat".into());
        ws
    }

    #[test]
    fn every_field_survives_a_domain_oc_domain_roundtrip() {
        let original = full_request();
        let oc = websocket_to_oc_websocket(&original);
        let back = oc_websocket_to_request(oc);
        assert_eq!(back, original);
    }

    #[test]
    fn single_untitled_message_is_written_in_the_single_form() {
        let mut ws = WebSocketRequest::new("Chat", "ws://x");
        ws.messages = vec![WebSocketMessage {
            title: String::new(),
            selected: true,
            kind: WebSocketMessageKind::Json,
            data: "{\"a\":1}".into(),
        }];
        let oc = websocket_to_oc_websocket(&ws);
        match oc.websocket.message {
            Some(OcWebSocketMessageOrVariants::Single(m)) => {
                assert_eq!(m.message_type, "json");
                assert_eq!(m.data, "{\"a\":1}");
            }
            other => panic!("expected the single form, got {other:?}"),
        }
    }

    #[test]
    fn several_messages_are_written_as_variants() {
        let mut ws = WebSocketRequest::new("Chat", "ws://x");
        ws.messages = vec![
            WebSocketMessage { title: "a".into(), selected: true, kind: WebSocketMessageKind::Text, data: "1".into() },
            WebSocketMessage { title: "b".into(), selected: false, kind: WebSocketMessageKind::Xml, data: "<x/>".into() },
        ];
        let oc = websocket_to_oc_websocket(&ws);
        match oc.websocket.message {
            Some(OcWebSocketMessageOrVariants::Variants(v)) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[1].title, "b");
                assert_eq!(v[1].message.message_type, "xml");
            }
            other => panic!("expected variants, got {other:?}"),
        }
    }

    #[test]
    fn a_single_titled_message_stays_a_variant_so_the_title_is_kept() {
        let mut ws = WebSocketRequest::new("Chat", "ws://x");
        ws.messages = vec![WebSocketMessage {
            title: "only".into(),
            selected: true,
            kind: WebSocketMessageKind::Text,
            data: "x".into(),
        }];
        let back = oc_websocket_to_request(websocket_to_oc_websocket(&ws));
        assert_eq!(back.messages[0].title, "only");
    }

    #[test]
    fn an_unknown_message_type_loads_as_text() {
        let yaml = "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: ws://x\n  message:\n    type: graphql\n    data: hi\n";
        let oc: OcWebSocketRequest = serde_yaml::from_str(yaml).expect("parse");
        let ws = oc_websocket_to_request(oc);
        assert_eq!(ws.messages.len(), 1);
        assert_eq!(ws.messages[0].kind, WebSocketMessageKind::Text);
        assert_eq!(ws.messages[0].data, "hi");
    }

    #[test]
    fn empty_runtime_is_not_written() {
        let ws = WebSocketRequest::new("Chat", "ws://x");
        let oc = websocket_to_oc_websocket(&ws);
        assert!(oc.runtime.is_none());
        assert!(oc.settings.is_none());
        assert_eq!(oc.info.request_type.as_deref(), Some("websocket"));
    }

    #[test]
    fn a_file_without_uid_gets_a_stable_uid_from_its_file_name() {
        let mut a = WebSocketRequest::new("Chat", "ws://x");
        a.uid = String::new();
        with_file_identity(&mut a, "chat.yml");
        let mut b = WebSocketRequest::new("Chat", "ws://x");
        b.uid = String::new();
        with_file_identity(&mut b, "chat.yml");
        assert_eq!(a.uid, "ws-chat.yml");
        assert_eq!(a.uid, b.uid);
        assert_eq!(a.file_name.as_deref(), Some("chat.yml"));

        let mut c = WebSocketRequest::new("Chat", "ws://x");
        c.uid = "keep-me".into();
        with_file_identity(&mut c, "chat.yml");
        assert_eq!(c.uid, "keep-me");
    }
}
```

- [ ] **Step 9: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-infra conversions::websocket`
Expected: compile errors (`websocket_to_oc_websocket` etc. not found; the module is not declared).

- [ ] **Step 10: Add `uid` to the persistence struct**

In `crates/rocket-infra/src/oc/websocket.rs` change `OcWebSocketRequest` to start with the uid field (same convention as `OcHttpRequest`):

```rust
pub struct OcWebSocketRequest {
    /// Stable identity for tab deduplication across reloads. Not in the
    /// OpenCollection schema; mirrors `OcHttpRequest.uid`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    pub info: OcWebSocketRequestInfo,
```

Also add `Default` to the derives of `OcWebSocketRequestRuntime` (all of its fields are `Vec` or `Option`). The request-variable helpers below take and replace the runtime block with `unwrap_or_default()`, exactly like Plan 05 does for GraphQL.

- [ ] **Step 11: Implement the conversions**

Prepend to `crates/rocket-infra/src/conversions/websocket.rs`:

```rust
use rocket_collection::settings::CollectionVariable;
use rocket_collection::websocket::{
    WebSocketMessage, WebSocketMessageKind, WebSocketRequest, WebSocketScript, WebSocketSettings,
};
use rocket_shared::types::{Auth, Header, RequestSettingValue};

use super::auth::persisted_oc_auth;
use crate::oc::*;

/// Converts a parsed OpenCollection WebSocket request to the domain type.
pub fn oc_websocket_to_request(oc: OcWebSocketRequest) -> WebSocketRequest {
    let (variables, scripts, runtime_auth) = match oc.runtime {
        Some(runtime) => (
            runtime
                .variables
                .into_iter()
                .map(CollectionVariable::from)
                .collect(),
            runtime
                .scripts
                .into_iter()
                .map(|s| WebSocketScript {
                    script_type: s.script_type,
                    code: s.code,
                })
                .collect(),
            runtime.auth.map(Auth::from),
        ),
        None => (Vec::new(), Vec::new(), None),
    };

    WebSocketRequest {
        uid: oc.uid.unwrap_or_default(),
        name: oc.info.name,
        description: oc.info.description,
        seq: oc.info.seq,
        tags: oc.info.tags,
        url: oc.websocket.url,
        headers: oc.websocket.headers.into_iter().map(Header::from).collect(),
        messages: messages_from_oc(oc.websocket.message),
        auth: oc.websocket.auth.map(Auth::from).unwrap_or(Auth::None),
        runtime_auth,
        variables,
        scripts,
        settings: oc.settings.map(settings_from_oc),
        docs: oc.docs,
        file_name: None,
    }
}

/// Converts a domain WebSocket request back to the OpenCollection structs.
pub fn websocket_to_oc_websocket(ws: &WebSocketRequest) -> OcWebSocketRequest {
    let runtime_auth = ws.runtime_auth.clone().map(OcAuth::from);
    let runtime = if ws.variables.is_empty() && ws.scripts.is_empty() && runtime_auth.is_none() {
        None
    } else {
        Some(OcWebSocketRequestRuntime {
            variables: ws.variables.iter().cloned().map(OcVariable::from).collect(),
            scripts: ws
                .scripts
                .iter()
                .map(|s| OcScript {
                    script_type: s.script_type.clone(),
                    code: s.code.clone(),
                })
                .collect(),
            auth: runtime_auth,
        })
    };

    OcWebSocketRequest {
        uid: if ws.uid.is_empty() {
            None
        } else {
            Some(ws.uid.clone())
        },
        info: OcWebSocketRequestInfo {
            name: ws.name.clone(),
            description: ws.description.clone(),
            request_type: Some("websocket".into()),
            seq: ws.seq,
            tags: ws.tags.clone(),
        },
        websocket: OcWebSocketRequestDetails {
            url: ws.url.clone(),
            headers: ws
                .headers
                .iter()
                .cloned()
                .map(OcHttpRequestHeader::from)
                .collect(),
            message: messages_to_oc(&ws.messages),
            auth: persisted_oc_auth(ws.auth.clone()),
        },
        runtime,
        settings: ws.settings.clone().map(settings_to_oc),
        docs: ws.docs.clone(),
    }
}

/// Sets the on-disk file name and, for a file that has no `uid`, a uid derived
/// from that name. A derived uid is stable across loads, so tab identity does
/// not change until the next save writes a uid into the file.
pub fn with_file_identity(ws: &mut WebSocketRequest, file_name: &str) {
    ws.file_name = Some(file_name.to_string());
    if ws.uid.is_empty() {
        ws.uid = format!("ws-{file_name}");
    }
}

fn kind_from_oc(message_type: &str) -> WebSocketMessageKind {
    WebSocketMessageKind::parse(message_type).unwrap_or_else(|| {
        tracing::warn!(
            message_type = %message_type,
            "unknown WebSocket message type, loading as text"
        );
        WebSocketMessageKind::Text
    })
}

fn messages_from_oc(message: Option<OcWebSocketMessageOrVariants>) -> Vec<WebSocketMessage> {
    match message {
        None => Vec::new(),
        Some(OcWebSocketMessageOrVariants::Single(m)) => vec![WebSocketMessage {
            title: String::new(),
            selected: true,
            kind: kind_from_oc(&m.message_type),
            data: m.data,
        }],
        Some(OcWebSocketMessageOrVariants::Variants(variants)) => variants
            .into_iter()
            .map(|v| WebSocketMessage {
                title: v.title,
                selected: v.selected,
                kind: kind_from_oc(&v.message.message_type),
                data: v.message.data,
            })
            .collect(),
    }
}

fn messages_to_oc(messages: &[WebSocketMessage]) -> Option<OcWebSocketMessageOrVariants> {
    let to_oc = |m: &WebSocketMessage| OcWebSocketMessage {
        message_type: m.kind.as_str().to_string(),
        data: m.data.clone(),
    };
    match messages {
        [] => None,
        [only] if only.title.is_empty() => Some(OcWebSocketMessageOrVariants::Single(to_oc(only))),
        many => Some(OcWebSocketMessageOrVariants::Variants(
            many.iter()
                .map(|m| OcWebSocketMessageVariant {
                    title: m.title.clone(),
                    selected: m.selected,
                    message: to_oc(m),
                })
                .collect(),
        )),
    }
}

fn setting_from_oc(value: InheritableNumber) -> RequestSettingValue<f64> {
    match value {
        InheritableNumber::Value(v) => RequestSettingValue::Value(v),
        InheritableNumber::Inherit(s) => RequestSettingValue::Inherit(s),
    }
}

fn setting_to_oc(value: RequestSettingValue<f64>) -> InheritableNumber {
    match value {
        RequestSettingValue::Value(v) => InheritableNumber::Value(v),
        RequestSettingValue::Inherit(s) => InheritableNumber::Inherit(s),
    }
}

fn settings_from_oc(oc: OcWebSocketRequestSettings) -> WebSocketSettings {
    WebSocketSettings {
        timeout: oc.timeout.map(setting_from_oc),
        keep_alive_interval: oc.keep_alive_interval.map(setting_from_oc),
    }
}

fn settings_to_oc(s: WebSocketSettings) -> OcWebSocketRequestSettings {
    OcWebSocketRequestSettings {
        timeout: s.timeout.map(setting_to_oc),
        keep_alive_interval: s.keep_alive_interval.map(setting_to_oc),
    }
}
```

In `crates/rocket-infra/src/conversions/mod.rs` add `mod websocket;` (after `mod variables;`) and, with the other `pub use` lines:

```rust
#[allow(unused_imports)]
pub use websocket::{oc_websocket_to_request, websocket_to_oc_websocket, with_file_identity};
```

- [ ] **Step 12: Update the three `match` arms in `conversions/folder.rs`**

Add `use super::websocket::{oc_websocket_to_request, websocket_to_oc_websocket};` to the imports. Replace the WebSocket arm of `oc_item_to_collection_item`:

```rust
        OcItem::WebSocket(ws) => Some(CollectionItem::WebSocket(Box::new(
            oc_websocket_to_request(ws),
        ))),
```

Add this arm to **both** `folder_to_oc_folder` and `collection_to_oc_collection`, directly above the `CollectionItem::OpaqueItem(opaque)` arm:

```rust
            CollectionItem::WebSocket(ws) => Some(OcItem::WebSocket(websocket_to_oc_websocket(&ws))),
```

Update the doc comment of `oc_item_to_collection_item` (Plan 05 already says GraphQL is typed): only gRPC items become `OpaqueItem`s; GraphQL and WebSocket items become typed items.

- [ ] **Step 13: Run the conversion tests**

Run: `cargo test -j4 -p rocket-infra conversions::websocket`
Expected: 7 passed. If `every_field_survives_a_domain_oc_domain_roundtrip` fails on `variables`, read the `From<CollectionVariable> for OcVariable` impl in `conversions/variables.rs` and adjust the test's variable (not the conversion) to a shape that impl round-trips, for example a non-empty `initial_value`. The point of the test is that variables are carried, not the exact initial-value rule.

- [ ] **Step 14: Write the failing repository tests**

Append to `crates/rocket-infra/src/fs_collection/tests.rs` (it already has `setup()`, `fs`, and the `WEBSOCKET_ITEM_YML` and `GRAPHQL_ITEM_YML` constants):

```rust
#[test]
fn websocket_roundtrip_through_the_repo_preserves_every_field() {
    use rocket_collection::websocket::*;
    use rocket_collection::CollectionVariable;
    use rocket_shared::description::Description;
    use rocket_shared::types::{Auth, Header, RequestSettingValue};

    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();

    let mut ws = WebSocketRequest::new("Chat", "wss://chat.example.com/ws");
    ws.description = Some(Description::text("Team chat"));
    ws.seq = Some(4);
    ws.tags = vec!["realtime".into()];
    ws.headers = vec![Header::new("Origin", "https://example.com")];
    ws.messages = vec![
        WebSocketMessage { title: "hi".into(), selected: true, kind: WebSocketMessageKind::Json, data: "{}".into() },
        WebSocketMessage { title: "raw".into(), selected: false, kind: WebSocketMessageKind::Binary, data: "AQID".into() },
    ];
    ws.auth = Auth::Bearer { token: "t".into() };
    ws.runtime_auth = Some(Auth::Basic { username: "u".into(), password: "p".into() });
    ws.variables = vec![CollectionVariable {
        key: "room".into(),
        value: "general".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    ws.scripts = vec![WebSocketScript { script_type: "before-request".into(), code: "// pre".into() }];
    ws.settings = Some(WebSocketSettings {
        timeout: Some(RequestSettingValue::Value(5000.0)),
        keep_alive_interval: Some(RequestSettingValue::Inherit("inherit".into())),
    });
    ws.docs = Some("# Chat".into());

    let written = repo.save_websocket_request("my-api", "chat", &ws).unwrap();
    assert_eq!(written, "chat.yml");

    let loaded = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    let mut expected = ws.clone();
    expected.file_name = Some("chat.yml".into());
    assert_eq!(loaded, expected);
}

#[test]
fn single_untitled_message_is_written_in_the_single_form() {
    use rocket_collection::websocket::*;

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut ws = WebSocketRequest::new("Chat", "ws://x");
    ws.messages = vec![WebSocketMessage {
        title: String::new(),
        selected: true,
        kind: WebSocketMessageKind::Json,
        data: "{}".into(),
    }];
    repo.save_websocket_request("my-api", "chat", &ws).unwrap();

    let raw: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(dir.path().join("my-api/chat.yml")).unwrap()).unwrap();
    assert!(raw["websocket"]["message"].is_mapping(), "{raw:?}");
    assert_eq!(raw["websocket"]["message"]["type"].as_str(), Some("json"));
    assert_eq!(raw["info"]["type"].as_str(), Some("websocket"));
}

#[test]
fn several_messages_are_written_as_variants() {
    use rocket_collection::websocket::*;

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut ws = WebSocketRequest::new("Chat", "ws://x");
    ws.messages = vec![
        WebSocketMessage { title: "a".into(), selected: true, kind: WebSocketMessageKind::Text, data: "1".into() },
        WebSocketMessage { title: "b".into(), selected: false, kind: WebSocketMessageKind::Text, data: "2".into() },
    ];
    repo.save_websocket_request("my-api", "chat", &ws).unwrap();

    let raw: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(dir.path().join("my-api/chat.yml")).unwrap()).unwrap();
    assert!(raw["websocket"]["message"].is_sequence(), "{raw:?}");
    assert_eq!(raw["websocket"]["message"][1]["title"].as_str(), Some("b"));
}

#[test]
fn a_websocket_file_without_uid_loads_with_the_same_derived_uid_everywhere() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/chat.yml"), WEBSOCKET_ITEM_YML).unwrap();

    let by_path = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    assert_eq!(by_path.uid, "ws-chat.yml");

    let full = repo.get("my-api").unwrap();
    let in_tree = full
        .root
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::WebSocket(w) => Some(w),
            _ => None,
        })
        .expect("websocket item in the full tree");
    assert_eq!(in_tree.uid, by_path.uid);
    assert_eq!(in_tree.file_name.as_deref(), Some("chat.yml"));
}

#[test]
fn summary_loading_returns_a_websocket_summary_for_the_sidebar() {
    use rocket_collection::{CollectionItem, RequestKind};

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/chat.yml"), WEBSOCKET_ITEM_YML).unwrap();
    fs::write(dir.path().join("my-api/get-user.yml"), GRPC_ITEM_YML).unwrap();

    let col = repo.get_summaries("my-api").unwrap();

    // gRPC is still left out of the summary payload (its own plan owns that).
    assert_eq!(col.root.items.len(), 1, "{:?}", col.root.items);
    match &col.root.items[0] {
        CollectionItem::Summary(s) => {
            assert_eq!(s.kind, RequestKind::WebSocket);
            assert_eq!(s.name, "Chat");
            assert_eq!(s.method, "GET");
            assert_eq!(s.url, "wss://chat.example.com/ws");
            assert_eq!(s.file_name.as_deref(), Some("chat.yml"));
            // The same derived uid as a full load, so a tab opened from the sidebar keeps its id.
            assert_eq!(s.uid, "ws-chat.yml");
        }
        other => panic!("expected a summary, got {other:?}"),
    }
}

#[test]
fn request_kind_reports_websocket_files() {
    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(dir.path().join("my-api/chat.yml"), WEBSOCKET_ITEM_YML).unwrap();
    assert_eq!(
        repo.request_kind("my-api", "chat.yml").unwrap(),
        rocket_collection::RequestKind::WebSocket
    );
}

#[test]
fn saving_a_websocket_request_keeps_runtime_variables_edited_on_their_own_path() {
    use rocket_collection::CollectionVariable;

    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let ws = rocket_collection::WebSocketRequest::new("Chat", "ws://x");
    repo.save_websocket_request("my-api", "chat", &ws).unwrap();

    // Variables are edited through their own commands, not through the request payload.
    let var = CollectionVariable {
        key: "room".into(),
        value: "general".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    };
    repo.save_request_variables("my-api", "chat.yml", vec![var.clone()]).unwrap();
    assert_eq!(repo.get_request_variables("my-api", "chat.yml").unwrap(), vec![var.clone()]);

    // A later save from the UI sends no variables and must not erase them.
    let mut again = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    again.variables = Vec::new();
    again.name = "Chat v2".into();
    repo.save_websocket_request("my-api", "chat", &again).unwrap();

    assert_eq!(repo.get_request_variables("my-api", "chat.yml").unwrap(), vec![var]);
    let after = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    assert_eq!(after.name, "Chat v2");
    assert_eq!(after.url, "ws://x", "the file is still a websocket file");
}

#[test]
fn get_websocket_on_an_http_file_is_an_error_not_a_panic() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let req = rocket_collection::Request::new("Get", rocket_shared::types::HttpMethod::Get, "https://x");
    repo.save_request("my-api", "get", &req).unwrap();
    assert!(repo.get_websocket_request("my-api", "get.yml").is_err());
}

#[test]
fn save_websocket_rejects_an_empty_uid() {
    let (_dir, repo) = setup();
    repo.create("my-api").unwrap();
    let mut ws = rocket_collection::WebSocketRequest::new("Chat", "ws://x");
    ws.uid = String::new();
    assert!(repo.save_websocket_request("my-api", "chat", &ws).is_err());
}
```

Also update the existing tests that assert WebSocket is opaque. This is a delta on what Plan 05 Task 1 already did to the same file (its Step 9 and Step 12):

1. `reorder_items_writes_order_file_and_get_respects_it`: Plan 05 added `CollectionItem::GraphQl(g) => g.name.as_str(),` to the local `item_name` helper. Add `CollectionItem::WebSocket(w) => w.name.as_str(),` next to it.
2. `build_folder_tree_loads_non_http_items_as_opaque`: Plan 05 left its `realtime` folder block (the last assertions) asserting an opaque WebSocket item. Keep Plan 05's rewritten root assertions and its name, and replace the `realtime` block with:

```rust
    let realtime = col.root.find_folder("realtime").unwrap();
    assert!(opaque_items(realtime).is_empty(), "websocket is typed now");
    let ws = realtime
        .items
        .iter()
        .find_map(|i| match i {
            rocket_collection::CollectionItem::WebSocket(w) => Some(w),
            _ => None,
        })
        .expect("typed websocket item");
    assert_eq!(ws.name, "Chat");
    assert_eq!(ws.url, "wss://chat.example.com/ws");
```

3. Replace `websocket_settings_preserved_in_opaque_item` with:

```rust
#[test]
fn websocket_settings_survive_the_typed_load() {
    use rocket_shared::types::RequestSettingValue;

    let (dir, repo) = setup();
    repo.create("my-api").unwrap();
    fs::write(
        dir.path().join("my-api/chat.yml"),
        "info:\n  name: Chat\n  type: websocket\nwebsocket:\n  url: wss://chat.example.com/ws\nsettings:\n  timeout: 5000\n  keepAliveInterval: 30000\n",
    )
    .unwrap();

    let ws = repo.get_websocket_request("my-api", "chat.yml").unwrap();
    let settings = ws.settings.expect("settings");
    assert_eq!(settings.timeout, Some(RequestSettingValue::Value(5000.0)));
    assert_eq!(settings.keep_alive_interval, Some(RequestSettingValue::Value(30000.0)));
}
```

4. `schema_shape_tests.rs`: add `"WebSocketRequest.uid",` to `KNOWN_DEFERRED` (next to Plan 05's `"GraphQLRequest.uid"`). Plan 05's loop over `users.items` has a `GraphQl` arm and an `OpaqueItem` arm that still handles `"websocket"`. Replace the `OpaqueItem` arm and add a `WebSocket` arm so the loop reads:

```rust
            CollectionItem::OpaqueItem(o) => panic!("unexpected opaque protocol {}", o.protocol),
            CollectionItem::WebSocket(ws) => {
                protocols.push("websocket".to_string());
                let raw = serde_yaml::to_value(crate::conversions::websocket_to_oc_websocket(ws))
                    .expect("serialize websocket");
                check_websocket_request(&mut v, &ws.name, &raw);
            }
```

(keep Plan 05's `GraphQl` arm and the final `_ => {}`), and the final assertion on `protocols` stays `vec!["graphql", "websocket"]`.

- [ ] **Step 15: Run them and confirm they fail**

Run: `cargo test -j4 -p rocket-infra fs_collection::tests::websocket`
Expected: compile errors (`save_websocket_request` and `get_websocket_request` are only the trait defaults on `FsCollectionRepo`, so the new round-trip test fails at run time, and the updated `item_name` match does not compile until the variant exists in all crates).

- [ ] **Step 16: Implement the repository methods**

In `crates/rocket-infra/src/fs_collection/requests.rs` add the imports `use rocket_collection::WebSocketRequest;`, `use crate::conversions::{oc_websocket_to_request, websocket_to_oc_websocket, with_file_identity};` and `use crate::oc::OcWebSocketRequest;`, then append:

```rust
pub(super) fn get_websocket_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
) -> DomainResult<WebSocketRequest> {
    Collection::validate_name(collection)?;
    let collection_dir = repo.collection_path(collection);
    let file_path = repo.validate_path(&collection_dir, Path::new(&request_filename_for(path)))?;
    if !file_path.exists() {
        return Err(DomainError::NotFound(format!("{collection}/{path}")));
    }
    let content = fs::read_to_string(&file_path)?;
    let oc: OcWebSocketRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse WebSocket request: {e}")))?;
    let mut ws = oc_websocket_to_request(oc);
    let file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    with_file_identity(&mut ws, &file_name);
    Ok(ws)
}

#[tracing::instrument(name = "collection_save_websocket", skip(repo, request), fields(collection_name = %collection, request_path = %path))]
pub(super) fn save_websocket_request(
    repo: &FsCollectionRepo,
    collection: &str,
    path: &str,
    request: &WebSocketRequest,
) -> DomainResult<String> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    if request.uid.is_empty() {
        return Err(DomainError::Internal(format!(
            "save_websocket_request: empty uid on request for '{path}' in collection '{collection}'"
        )));
    }

    let collection_dir = repo.collection_path(collection);
    let file_path = repo.validate_path(&collection_dir, Path::new(&request_filename_for(path)))?;
    let mut oc = websocket_to_oc_websocket(request);

    // Request variables are saved on their own path (`save_request_variables`), so an empty
    // list in the payload must keep what is on disk. Same rule as `save_request`.
    if request.variables.is_empty() && file_path.exists() {
        if let Ok(existing_content) = fs::read_to_string(&file_path) {
            if let Ok(existing) = serde_yaml::from_str::<OcWebSocketRequest>(&existing_content) {
                if let Some(existing_runtime) = existing.runtime {
                    if !existing_runtime.variables.is_empty() {
                        let runtime = oc.runtime.get_or_insert_with(Default::default);
                        runtime.variables = existing_runtime.variables;
                    }
                }
            }
        }
    }

    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize WebSocket YAML: {e}")))?;
    atomic_write(&file_path, yaml.as_bytes())?;

    Ok(file_path
        .strip_prefix(&collection_dir)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string())
}
```

In `fs_collection/mod.rs` add `WebSocketRequest` to the `rocket_collection` import list and, inside `impl CollectionRepository for FsCollectionRepo`, after Plan 05's `request_kind` override:

```rust
    fn get_websocket_request(&self, collection: &str, path: &str) -> DomainResult<WebSocketRequest> {
        requests::get_websocket_request(self, collection, path)
    }

    fn save_websocket_request(
        &self,
        collection: &str,
        path: &str,
        request: &WebSocketRequest,
    ) -> DomainResult<String> {
        requests::save_websocket_request(self, collection, path, request)
    }
```

In `shared_path_collection_repo.rs` add the same two methods with bodies `self.repo().get_websocket_request(collection, path)` and `self.repo().save_websocket_request(collection, path, request)`, and import `WebSocketRequest`.

- [ ] **Step 17: Load WebSocket items in both tree builders, and make the request-variable helpers WebSocket-aware**

Add a shared derived-uid helper to `crates/rocket-infra/src/conversions/websocket.rs` and make `with_file_identity` use it, so a summary and a full load always agree:

```rust
/// The uid a file with no `uid` key gets. Stable across loads, so tab identity survives a reload.
pub fn derived_websocket_uid(file_name: &str) -> String {
    format!("ws-{file_name}")
}
```

(change `ws.uid = format!("ws-{file_name}");` in `with_file_identity` to `ws.uid = derived_websocket_uid(file_name);`, and export `derived_websocket_uid` from `conversions/mod.rs` next to `with_file_identity`.)

In `fs_collection/tree.rs`, in `build_folder_tree`'s closure add an arm before `Ok(other) => Ok(other),` (next to Plan 05's `GraphQl` arm):

```rust
            Ok(Some(CollectionItem::WebSocket(mut ws))) => {
                crate::conversions::with_file_identity(&mut ws, entry_name);
                Ok(Some(CollectionItem::WebSocket(ws)))
            }
```

In `load_request_summary`, in the `match serde_yaml::from_str::<OcItem>(&content)` that Plan 05 rewrote, add a WebSocket arm and remove `OcItem::WebSocket(_)` from the `Ok(None)` arm. The WebSocket sidebar row is a summary, exactly like GraphQL; the whole file is opened on demand through `get_websocket_request`:

```rust
            Ok(OcItem::WebSocket(ws)) => Ok(Some(RequestSummary {
                uid: ws
                    .uid
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| crate::conversions::derived_websocket_uid(entry_name)),
                name: ws.info.name,
                // A WebSocket handshake is a GET. The sidebar badge comes from `kind`, not this.
                method: "GET".to_string(),
                url: ws.websocket.url,
                file_name: Some(entry_name.to_string()),
                kind: RequestKind::WebSocket,
            })),
            Ok(OcItem::Grpc(_) | OcItem::ScriptFile(_)) => Ok(None),
```

Update the doc comment on `load_request_summary`: GraphQL and WebSocket files return a summary with their `kind`; gRPC and script files return `Ok(None)`.

In `fs_collection/variables.rs`, Plan 05 added `runtime_variables_of` and `with_runtime_variables`, which try HTTP, then GraphQL, then return the HTTP error. Add a WebSocket attempt to each, before the final error. Import `OcWebSocketRequest` from `crate::oc`. In `runtime_variables_of`:

```rust
    if let Ok(ws) = serde_yaml::from_str::<OcWebSocketRequest>(content) {
        return Ok(ws.runtime.map(|r| r.variables).unwrap_or_default());
    }
```

and in `with_runtime_variables`:

```rust
    if let Ok(mut ws) = serde_yaml::from_str::<OcWebSocketRequest>(content) {
        let mut runtime = ws.runtime.take().unwrap_or_default();
        runtime.variables = vars;
        ws.runtime = Some(runtime);
        return serde_yaml::to_string(&ws).map_err(to_err);
    }
```

Place both blocks after the GraphQL attempt and before the `Err(...)` that reports the HTTP parse error. An `OcWebSocketRequest` needs a `websocket:` key, so an HTTP or GraphQL file never matches it by accident.

- [ ] **Step 18: Run the repository tests**

Run: `cargo test -j4 -p rocket-infra fs_collection`
Expected: all pass, including the updated ones. Then `cargo test -j4 -p rocket-infra conversions`.

### Step group C: app layer, exhaustive matches, rename, IPC

- [ ] **Step 19: Write the failing app tests**

In `crates/rocket-app/src/runner_sequence.rs` tests, after `opaque_protocol_items_are_never_steps`:

```rust
    #[test]
    fn websocket_items_are_never_steps() {
        let mut collection = Collection::new("my-api");
        collection.root.add_request(req("Login", "login.yml"));
        collection.root.items.push(rocket_collection::CollectionItem::WebSocket(Box::new(
            rocket_collection::WebSocketRequest::new("Chat", "wss://x"),
        )));

        let items = flatten_run_set(&collection, None).expect("flatten");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Login");
    }
```

Append to `crates/rocket-app/src/collection_service.rs`:

```rust
#[cfg(test)]
mod websocket_tests {
    use super::*;
    use rocket_collection::WebSocketRequest;
    use rocket_shared::events::NullEventPublisher;

    fn service(dir: &std::path::Path) -> CollectionService {
        CollectionService::new(
            Box::new(rocket_infra::FsCollectionRepo::new_standalone(dir.to_path_buf())),
            Box::new(NullEventPublisher),
        )
    }

    #[test]
    fn rename_request_renames_a_websocket_request_in_place() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        let saved = svc
            .save_websocket_request("api", "chat", &WebSocketRequest::new("Chat", "wss://chat.example.com/ws"))
            .expect("save");
        assert_eq!(saved.file_name.as_deref(), Some("chat.yml"));

        svc.rename_request("api", "chat.yml", "Team Chat").expect("rename");

        let renamed = svc.get_websocket_request("api", "chat.yml").expect("reload");
        assert_eq!(renamed.name, "Team Chat");
        assert_eq!(renamed.uid, saved.uid);
        assert_eq!(renamed.url, "wss://chat.example.com/ws");
    }

    #[test]
    fn update_request_docs_keeps_a_websocket_request_a_websocket_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        svc.save_websocket_request("api", "chat", &WebSocketRequest::new("Chat", "wss://x/ws"))
            .expect("save");

        svc.update_request_docs("api", "chat.yml", Some("# Notes".into()))
            .expect("update docs");

        let back = svc.get_websocket_request("api", "chat.yml").expect("reload");
        assert_eq!(back.docs.as_deref(), Some("# Notes"));
        assert_eq!(back.url, "wss://x/ws");
    }

    #[test]
    fn request_variables_round_trip_for_a_websocket_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        svc.save_websocket_request("api", "chat", &WebSocketRequest::new("Chat", "wss://x/ws"))
            .expect("save");
        let var = rocket_collection::CollectionVariable {
            key: "room".into(),
            value: "general".into(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        };

        svc.save_request_variables("api", "chat.yml", vec![var.clone()]).expect("save vars");

        assert_eq!(svc.get_request_variables("api", "chat.yml").expect("get vars"), vec![var]);
        assert!(svc.get_websocket_request("api", "chat.yml").is_ok(), "still a websocket file");
    }

    #[test]
    fn rename_request_still_works_for_an_http_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let svc = service(dir.path());
        svc.create("api").expect("create collection");
        let req = Request::new("Get", rocket_shared::types::HttpMethod::Get, "https://x");
        svc.save_request("api", "get", &req).expect("save");

        svc.rename_request("api", "get.yml", "Fetch").expect("rename");

        assert_eq!(svc.get_request("api", "get.yml").expect("reload").name, "Fetch");
    }
}
```

- [ ] **Step 20: Run them and confirm they fail**

Run: `cargo test -j4 -p rocket-app websocket`
Expected: compile errors (non-exhaustive matches; no `save_websocket_request` on the service). If Plan 05 Task 1 is not merged yet, stop: `request_kind` does not exist and this task cannot be built.

- [ ] **Step 21: Implement the app changes**

`runner_sequence.rs`: Plan 05 turned the skip arm in `collect_items` into `CollectionItem::GraphQl(_) | CollectionItem::OpaqueItem(_) | CollectionItem::Summary(_) => {}`. Add the WebSocket variant to it:

```rust
            CollectionItem::GraphQl(_)
            | CollectionItem::OpaqueItem(_)
            | CollectionItem::WebSocket(_)
            | CollectionItem::Summary(_) => {}
```

`contract_service.rs` (`walk_folder`): Plan 05 added a `GraphQl(_) => {}` arm with the comment "Contracts describe HTTP request signatures only". Add `CollectionItem::WebSocket(_) => {}` beside it.

`collection_service.rs`: import `WebSocketRequest` from `rocket_collection`, then add after `get_request`:

```rust
    /// Get one WebSocket request by collection and relative path.
    pub fn get_websocket_request(&self, collection: &str, path: &str) -> DomainResult<WebSocketRequest> {
        self.repo.get_websocket_request(collection, path)
    }

    /// Save a WebSocket request and return it re-read from disk, like `save_request`.
    pub fn save_websocket_request(
        &self,
        collection: &str,
        path: &str,
        request: &WebSocketRequest,
    ) -> DomainResult<WebSocketRequest> {
        let actual_path = self.repo.save_websocket_request(collection, path, request)?;
        self.events.publish(DomainEvent::RequestSaved {
            collection: collection.to_string(),
            path: actual_path.clone(),
        });
        self.repo.get_websocket_request(collection, &actual_path)
    }
```

Plan 05 put a GraphQL branch at the top of `rename_request` and `update_request_docs`, each starting with `if self.repo.request_kind(collection, <path>)? == RequestKind::GraphQl {`. Turn each into one `request_kind` call and add the WebSocket branch. In `rename_request`:

```rust
        let kind = self.repo.request_kind(collection, old_path)?;
        if kind == RequestKind::GraphQl {
            // ... Plan 05's body, unchanged ...
        }
        if kind == RequestKind::WebSocket {
            let mut websocket = self.repo.get_websocket_request(collection, old_path)?;
            websocket.name = new_name.to_string();
            let actual_path = self.repo.save_websocket_request(collection, old_path, &websocket)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
```

and in `update_request_docs` (a WebSocket request keeps its docs as plain markdown text, so no `Documentation::text` wrapper):

```rust
        let kind = self.repo.request_kind(collection, path)?;
        if kind == RequestKind::GraphQl {
            // ... Plan 05's body, unchanged ...
        }
        if kind == RequestKind::WebSocket {
            let mut websocket = self.repo.get_websocket_request(collection, path)?;
            websocket.docs = docs;
            let actual_path = self.repo.save_websocket_request(collection, path, &websocket)?;
            self.events.publish(DomainEvent::RequestSaved {
                collection: collection.to_string(),
                path: actual_path,
            });
            return Ok(());
        }
```

Check that the existing tail of each method returns `Ok(())`; if the signature differs, adapt the early return to match. `get_request_variables` and `save_request_variables` need no service change: they go through the repository helpers extended in Step 17.

- [ ] **Step 22: Add the Tauri commands**

In `src-tauri/src/commands/collections.rs` add `WebSocketRequest` to the `rocket_collection` import and:

```rust
#[tauri::command]
pub fn get_websocket_request(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<WebSocketRequest, DomainError> {
    svc.get_websocket_request(&collection, &path)
}

#[tauri::command]
pub fn save_websocket_request(
    collection: String,
    path: String,
    request: WebSocketRequest,
    svc: State<'_, CollectionService>,
) -> Result<WebSocketRequest, DomainError> {
    svc.save_websocket_request(&collection, &path, &request)
}
```

Register both in `src-tauri/src/lib.rs` directly after `commands::collections::save_request,`.

- [ ] **Step 23: Verify**

Run, in order: `cargo test -j4 -p rocket-app websocket`, `cargo test -j4 -p rocket-app runner_sequence`, `cargo check -j4` (the whole workspace compiles, including `src-tauri`), `cargo clippy -j4 -p rocket-collection -p rocket-infra -p rocket-app -- -D warnings`.
Expected: green. `cargo check -j4` is the proof that no exhaustive `match` on `CollectionItem` was missed.

- [ ] **Step 24: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path: `crates/rocket-collection/src/websocket.rs`, `crates/rocket-collection/src/lib.rs`, `crates/rocket-collection/src/folder.rs`, `crates/rocket-collection/src/repository.rs`, `crates/rocket-collection/CLAUDE.md`, `crates/rocket-infra/src/oc/websocket.rs`, `crates/rocket-infra/src/conversions/websocket.rs`, `crates/rocket-infra/src/conversions/mod.rs`, `crates/rocket-infra/src/conversions/folder.rs`, `crates/rocket-infra/src/fs_collection/mod.rs`, `crates/rocket-infra/src/fs_collection/requests.rs`, `crates/rocket-infra/src/fs_collection/tree.rs`, `crates/rocket-infra/src/fs_collection/variables.rs`, `crates/rocket-infra/src/fs_collection/tests.rs`, `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`, `crates/rocket-infra/src/shared_path_collection_repo.rs`, `crates/rocket-app/src/collection_service.rs`, `crates/rocket-app/src/runner_sequence.rs`, `crates/rocket-app/src/contract_service.rs`, `src-tauri/src/commands/collections.rs`, `src-tauri/src/lib.rs`.
Suggested subject: `feat(websocket): typed WebSocketRequest item with sidebar summary, load, save and rename`.

---

## Task 2: `WebSocketClient` trait and the `tokio-tungstenite` implementation

**Files:**
- Create: `crates/rocket-http/src/websocket.rs`, `crates/rocket-infra/src/websocket_client.rs`
- Modify: `crates/rocket-http/src/lib.rs`, `crates/rocket-infra/Cargo.toml`, `Cargo.lock`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-http/CLAUDE.md`, `crates/rocket-infra/CLAUDE.md`

**Interfaces:**
- Consumes: `DomainError`, `DomainResult`.
- Produces (all in `rocket_http::websocket`, re-exported at the crate root):

```rust
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 30_000;

pub struct WebSocketConnectRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub subprotocols: Vec<String>,
    pub connect_timeout: Option<std::time::Duration>,
    pub keep_alive_interval: Option<std::time::Duration>,
    pub verify_ssl: bool,
}                                   // manual Debug prints header names only
pub enum WebSocketFrame { Text(String), Binary(Vec<u8>) }
pub struct WebSocketClose { pub code: Option<u16>, pub reason: String, pub clean: bool }
pub enum WebSocketEvent { Frame(WebSocketFrame), Closed(WebSocketClose) }
pub enum WebSocketCommand { Send(WebSocketFrame), Close { code: u16, reason: String } }
pub struct WebSocketHandle {
    pub subprotocol: Option<String>,
    pub outbound: tokio::sync::mpsc::Sender<WebSocketCommand>,
    pub events: tokio::sync::mpsc::Receiver<WebSocketEvent>,
}
#[async_trait]
pub trait WebSocketClient: Send + Sync {
    async fn connect(&self, request: WebSocketConnectRequest) -> DomainResult<WebSocketHandle>;
}
```

  and `rocket_infra::TungsteniteWebSocketClient` (`new() -> Self`, implements `WebSocketClient`).

Contract of the handle: `events` yields zero or more `Frame` events and then **exactly one** `Closed`, after which the channel ends. Dropping `outbound` (and every clone) makes the pump close the socket.

- [ ] **Step 0: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 1: Write the failing trait-crate tests**

Create `crates/rocket-http/src/websocket.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_contains_header_values() {
        let request = WebSocketConnectRequest {
            url: "wss://chat.example.com/ws".into(),
            headers: vec![("Authorization".into(), "Bearer super-secret".into())],
            subprotocols: vec![],
            connect_timeout: None,
            keep_alive_interval: None,
            verify_ssl: true,
        };
        let shown = format!("{request:?}");
        assert!(shown.contains("Authorization"), "{shown}");
        assert!(!shown.contains("super-secret"), "{shown}");
    }

    #[test]
    fn frame_helpers_report_payload_size_in_bytes() {
        assert_eq!(WebSocketFrame::Text("héllo".into()).size(), 6);
        assert_eq!(WebSocketFrame::Binary(vec![1, 2, 3]).size(), 3);
    }

    #[test]
    fn client_trait_is_object_safe() {
        fn _assert(_: std::sync::Arc<dyn WebSocketClient>) {}
    }
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-http websocket`
Expected: compile errors, types not found (the module is not declared yet either).

- [ ] **Step 3: Implement the trait crate module**

Prepend to `crates/rocket-http/src/websocket.rs`:

```rust
//! WebSocket client port. Pure types and one trait; the socket code lives in
//! `rocket-infra`. A connection is a pair of channels so a caller can read
//! events and send commands at the same time without sharing a `&mut`.

use std::time::Duration;

use async_trait::async_trait;
use rocket_shared::error::DomainResult;
use tokio::sync::mpsc;

/// Connect and handshake timeout used when a request does not set one.
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 30_000;

/// A fully resolved connect request: variables already substituted and auth
/// already turned into headers or a query parameter.
#[derive(Clone)]
pub struct WebSocketConnectRequest {
    pub url: String,
    /// Header name and value pairs sent on the handshake.
    pub headers: Vec<(String, String)>,
    /// Subprotocols offered in `Sec-WebSocket-Protocol`, in preference order.
    pub subprotocols: Vec<String>,
    /// `None` waits forever.
    pub connect_timeout: Option<Duration>,
    /// `None` sends no pings.
    pub keep_alive_interval: Option<Duration>,
    pub verify_ssl: bool,
}

// Header values commonly hold credentials, and the URL can carry a token in its
// query, so Debug prints header names only and omits the URL.
impl std::fmt::Debug for WebSocketConnectRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("WebSocketConnectRequest")
            .field("headers", &names)
            .field("subprotocols", &self.subprotocols)
            .field("connect_timeout", &self.connect_timeout)
            .field("keep_alive_interval", &self.keep_alive_interval)
            .field("verify_ssl", &self.verify_ssl)
            .finish_non_exhaustive()
    }
}

/// One data frame, in either direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketFrame {
    Text(String),
    Binary(Vec<u8>),
}

impl WebSocketFrame {
    /// Payload size in bytes.
    pub fn size(&self) -> usize {
        match self {
            Self::Text(t) => t.len(),
            Self::Binary(b) => b.len(),
        }
    }
}

/// How a connection ended. `clean` is true when both sides finished the
/// WebSocket close handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSocketClose {
    pub code: Option<u16>,
    pub reason: String,
    pub clean: bool,
}

/// What a connection reports. A stream of `Frame`s ends with exactly one `Closed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketEvent {
    Frame(WebSocketFrame),
    Closed(WebSocketClose),
}

/// What a caller asks a connection to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketCommand {
    Send(WebSocketFrame),
    Close { code: u16, reason: String },
}

/// A live connection. Dropping every clone of `outbound` closes the socket.
#[derive(Debug)]
pub struct WebSocketHandle {
    /// The subprotocol the server selected, if any.
    pub subprotocol: Option<String>,
    pub outbound: mpsc::Sender<WebSocketCommand>,
    pub events: mpsc::Receiver<WebSocketEvent>,
}

/// Opens WebSocket connections. Implemented by `TungsteniteWebSocketClient`
/// in `rocket-infra`.
#[async_trait]
pub trait WebSocketClient: Send + Sync {
    async fn connect(&self, request: WebSocketConnectRequest) -> DomainResult<WebSocketHandle>;
}
```

In `crates/rocket-http/src/lib.rs` add `pub mod websocket;` (after `pub mod token_client;`) and

```rust
pub use websocket::{
    WebSocketClient, WebSocketClose, WebSocketCommand, WebSocketConnectRequest, WebSocketEvent,
    WebSocketFrame, WebSocketHandle, DEFAULT_CONNECT_TIMEOUT_MS,
};
```

Add a `websocket` row to the module table in `crates/rocket-http/CLAUDE.md`: "`websocket` | `WebSocketClient` port, connect request, frame, event and command types. `Debug` prints header names only. Implemented by `TungsteniteWebSocketClient` in `rocket-infra`."

- [ ] **Step 4: Run the trait tests**

Run: `cargo test -j4 -p rocket-http websocket`
Expected: 3 passed.

- [ ] **Step 5: Add the dependencies**

`crates/rocket-infra/Cargo.toml`, in `[dependencies]` (these three are only used by this crate, so they are declared here rather than in `[workspace.dependencies]`):

```toml
# WebSocket client. `native-tls` matches reqwest's TLS backend (see Cargo.toml:35), so HTTP and
# WebSocket trust the same OS certificate store. `native-tls` itself is needed to build the
# permissive connector used when a request turns certificate verification off.
tokio-tungstenite = { version = "0.30", default-features = false, features = ["connect", "native-tls"] }
futures-util = { version = "0.3", default-features = false, features = ["sink", "std"] }
native-tls = "0.2"
```

Run `cargo check -j4 -p rocket-infra` and confirm `Cargo.lock` gains `tungstenite 0.30.0` and `tokio-tungstenite 0.30.0` and keeps a single `native-tls 0.2.18` (`grep -c '^name = "native-tls"$' Cargo.lock` prints 1).

- [ ] **Step 6: Write the failing client tests**

Create `crates/rocket-infra/src/websocket_client.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    use tokio_tungstenite::tungstenite::http::StatusCode;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::Message;

    #[derive(Default)]
    struct ServerState {
        handshake_headers: Mutex<Vec<(String, String)>>,
        pings: AtomicUsize,
    }

    #[derive(Clone, Copy)]
    enum Mode {
        Echo,
        RejectUnauthorized,
    }

    /// Local server. In `Echo` mode it echoes text and binary frames, offers back the first
    /// subprotocol it was sent, closes with code 4001 when it receives the text `close-me`,
    /// and counts pings. In `RejectUnauthorized` mode it answers the handshake with 401.
    async fn spawn_server(mode: Mode) -> (u16, Arc<ServerState>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let state = Arc::new(ServerState::default());
        let shared = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    return;
                };
                let state = Arc::clone(&shared);
                tokio::spawn(async move {
                    let for_callback = Arc::clone(&state);
                    let callback = move |req: &Request,
                                         mut resp: Response|
                          -> Result<Response, ErrorResponse> {
                        for (k, v) in req.headers() {
                            for_callback
                                .handshake_headers
                                .lock()
                                .expect("lock")
                                .push((k.as_str().to_string(), v.to_str().unwrap_or("").to_string()));
                        }
                        if matches!(mode, Mode::RejectUnauthorized) {
                            let mut rejection = ErrorResponse::new(Some("denied".to_string()));
                            *rejection.status_mut() = StatusCode::UNAUTHORIZED;
                            return Err(rejection);
                        }
                        let offered = req
                            .headers()
                            .get("sec-websocket-protocol")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.split(',').next())
                            .map(str::trim)
                            .filter(|s| !s.is_empty());
                        if let Some(first) = offered {
                            if let Ok(value) = first.parse() {
                                resp.headers_mut().insert("sec-websocket-protocol", value);
                            }
                        }
                        Ok(resp)
                    };
                    let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else {
                        return;
                    };
                    while let Some(Ok(msg)) = ws.next().await {
                        match msg {
                            Message::Text(t) if t.as_str() == "close-me" => {
                                let _ = ws
                                    .close(Some(CloseFrame {
                                        code: CloseCode::from(4001),
                                        reason: "bye".into(),
                                    }))
                                    .await;
                            }
                            Message::Text(_) | Message::Binary(_) => {
                                if ws.send(msg).await.is_err() {
                                    break;
                                }
                            }
                            Message::Ping(_) => {
                                state.pings.fetch_add(1, Ordering::SeqCst);
                            }
                            _ => {}
                        }
                    }
                });
            }
        });
        (port, state)
    }

    fn request_for(port: u16) -> WebSocketConnectRequest {
        WebSocketConnectRequest {
            url: format!("ws://127.0.0.1:{port}/socket"),
            headers: Vec::new(),
            subprotocols: Vec::new(),
            connect_timeout: Some(Duration::from_secs(5)),
            keep_alive_interval: None,
            verify_ssl: true,
        }
    }

    async fn next_event(handle: &mut WebSocketHandle) -> WebSocketEvent {
        tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
            .await
            .expect("an event within 5s")
            .expect("event channel still open")
    }

    #[tokio::test]
    async fn text_and_binary_frames_round_trip_through_an_echo_server() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let mut handle = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");

        handle
            .outbound
            .send(WebSocketCommand::Send(WebSocketFrame::Text("hello".into())))
            .await
            .expect("send text");
        assert_eq!(
            next_event(&mut handle).await,
            WebSocketEvent::Frame(WebSocketFrame::Text("hello".into()))
        );

        handle
            .outbound
            .send(WebSocketCommand::Send(WebSocketFrame::Binary(vec![1, 2, 3])))
            .await
            .expect("send binary");
        assert_eq!(
            next_event(&mut handle).await,
            WebSocketEvent::Frame(WebSocketFrame::Binary(vec![1, 2, 3]))
        );
    }

    #[tokio::test]
    async fn headers_are_sent_on_the_handshake() {
        let (port, state) = spawn_server(Mode::Echo).await;
        let mut request = request_for(port);
        request.headers = vec![
            ("Authorization".into(), "Bearer t0k".into()),
            ("X-Trace".into(), "abc".into()),
        ];
        let _handle = TungsteniteWebSocketClient::new().connect(request).await.expect("connect");

        let seen = state.handshake_headers.lock().expect("lock").clone();
        assert!(seen.contains(&("authorization".to_string(), "Bearer t0k".to_string())), "{seen:?}");
        assert!(seen.contains(&("x-trace".to_string(), "abc".to_string())), "{seen:?}");
    }

    #[tokio::test]
    async fn the_negotiated_subprotocol_is_reported() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let mut request = request_for(port);
        request.subprotocols = vec!["graphql-transport-ws".into(), "graphql-ws".into()];
        let handle = TungsteniteWebSocketClient::new().connect(request).await.expect("connect");
        assert_eq!(handle.subprotocol.as_deref(), Some("graphql-transport-ws"));
    }

    #[tokio::test]
    async fn a_server_close_yields_one_closed_event_with_its_code_then_ends() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let mut handle = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");
        handle
            .outbound
            .send(WebSocketCommand::Send(WebSocketFrame::Text("close-me".into())))
            .await
            .expect("send");

        match next_event(&mut handle).await {
            WebSocketEvent::Closed(close) => {
                assert_eq!(close.code, Some(4001));
                assert_eq!(close.reason, "bye");
                assert!(close.clean);
            }
            other => panic!("expected Closed, got {other:?}"),
        }
        let after = tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
            .await
            .expect("channel ends");
        assert!(after.is_none(), "no event may follow Closed, got {after:?}");
    }

    #[tokio::test]
    async fn a_client_close_command_ends_with_a_clean_close() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let mut handle = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");
        handle
            .outbound
            .send(WebSocketCommand::Close { code: 1000, reason: "done".into() })
            .await
            .expect("close");

        match next_event(&mut handle).await {
            WebSocketEvent::Closed(close) => assert!(close.clean, "{close:?}"),
            other => panic!("expected Closed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dropping_the_outbound_sender_closes_the_socket() {
        let (port, _state) = spawn_server(Mode::Echo).await;
        let WebSocketHandle { outbound, mut events, .. } = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect("connect");
        drop(outbound);

        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("the pump must finish once every sender is gone")
            .expect("one Closed event");
        assert!(matches!(event, WebSocketEvent::Closed(_)), "{event:?}");
    }

    #[tokio::test]
    async fn keep_alive_pings_reach_the_server() {
        let (port, state) = spawn_server(Mode::Echo).await;
        let mut request = request_for(port);
        request.keep_alive_interval = Some(Duration::from_millis(50));
        let _handle = TungsteniteWebSocketClient::new().connect(request).await.expect("connect");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while state.pings.load(Ordering::SeqCst) < 2 {
            assert!(tokio::time::Instant::now() < deadline, "fewer than 2 pings in 5s");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_rejected_handshake_reports_the_status() {
        let (port, _state) = spawn_server(Mode::RejectUnauthorized).await;
        let err = TungsteniteWebSocketClient::new()
            .connect(request_for(port))
            .await
            .expect_err("401 must fail the connect");
        assert!(err.to_string().contains("401"), "{err}");
    }

    #[tokio::test]
    async fn connection_errors_never_contain_header_values() {
        // A rejected handshake.
        let (port, _state) = spawn_server(Mode::RejectUnauthorized).await;
        let mut request = request_for(port);
        request.headers = vec![("Authorization".into(), "Bearer super-secret".into())];
        request.url = format!("{}?token=query-secret", request.url);
        let err = TungsteniteWebSocketClient::new()
            .connect(request)
            .await
            .expect_err("rejected");
        let text = err.to_string();
        assert!(!text.contains("super-secret"), "{text}");
        assert!(!text.contains("query-secret"), "{text}");

        // A refused connection: bind a port, then free it.
        let probe = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let dead_port = probe.local_addr().expect("addr").port();
        drop(probe);
        let mut refused = request_for(dead_port);
        refused.headers = vec![("Authorization".into(), "Bearer super-secret".into())];
        refused.url = format!("{}?token=query-secret", refused.url);
        let err = TungsteniteWebSocketClient::new()
            .connect(refused)
            .await
            .expect_err("refused");
        let text = err.to_string();
        assert!(!text.contains("super-secret"), "{text}");
        assert!(!text.contains("query-secret"), "{text}");
    }

    #[tokio::test]
    async fn a_silent_server_hits_the_connect_timeout() {
        // The kernel completes the TCP connect from its backlog, but nothing ever answers
        // the handshake.
        let silent = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = silent.local_addr().expect("addr").port();
        let mut request = request_for(port);
        request.connect_timeout = Some(Duration::from_millis(200));
        let err = TungsteniteWebSocketClient::new()
            .connect(request)
            .await
            .expect_err("must time out");
        assert!(err.to_string().contains("timed out"), "{err}");
        drop(silent);
    }

    #[test]
    fn reserved_handshake_headers_are_rejected_by_name() {
        for name in ["Host", "Upgrade", "Connection", "Sec-WebSocket-Key", "sec-websocket-version"] {
            let mut request = request_for(1);
            request.headers = vec![(name.into(), "x".into())];
            let err = build_client_request(&request).expect_err("reserved header");
            assert!(err.to_string().contains(name), "{err}");
        }
    }

    #[test]
    fn a_protocol_header_is_merged_into_the_subprotocol_offer() {
        let mut request = request_for(1);
        request.subprotocols = vec!["a".into()];
        request.headers = vec![("Sec-WebSocket-Protocol".into(), "b, a".into())];
        let built = build_client_request(&request).expect("build");
        assert_eq!(
            built.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok()),
            Some("a, b")
        );
    }

    #[test]
    fn a_non_websocket_scheme_is_rejected_before_any_network_use() {
        let mut request = request_for(1);
        request.url = "https://example.com/socket".into();
        let err = build_client_request(&request).expect_err("https is not a websocket scheme");
        assert!(err.to_string().contains("ws://"), "{err}");
    }

    #[test]
    fn the_tls_connector_is_only_replaced_when_verification_is_off() {
        assert!(tls_connector(true).expect("default").is_none());
        assert!(tls_connector(false).expect("permissive").is_some());
    }
}
```

- [ ] **Step 7: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-infra websocket_client`
Expected: compile errors (`TungsteniteWebSocketClient`, `build_client_request`, `tls_connector` not found; module not declared).

- [ ] **Step 8: Implement the client**

Prepend to `crates/rocket-infra/src/websocket_client.rs`:

```rust
//! `tokio-tungstenite` implementation of the `WebSocketClient` port.
//!
//! `connect` performs the handshake and then spawns a pump task that owns the socket. The caller
//! talks to the pump through the channels in `WebSocketHandle`. Errors name header names, never
//! values, and never include the URL.

use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rocket_http::websocket::{
    WebSocketClient, WebSocketClose, WebSocketCommand, WebSocketConnectRequest, WebSocketEvent,
    WebSocketFrame, WebSocketHandle,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{connect_async_tls_with_config, Connector};

/// How long to wait for the server's close reply after sending our close frame.
const CLOSE_REPLY_TIMEOUT: Duration = Duration::from_secs(3);
const COMMAND_BUFFER: usize = 64;
const EVENT_BUFFER: usize = 256;

/// Headers the handshake itself owns. A caller cannot override them.
const RESERVED_HEADERS: [&str; 6] = [
    "host",
    "connection",
    "upgrade",
    "sec-websocket-key",
    "sec-websocket-version",
    "sec-websocket-extensions",
];

#[derive(Debug, Default, Clone, Copy)]
pub struct TungsteniteWebSocketClient;

impl TungsteniteWebSocketClient {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WebSocketClient for TungsteniteWebSocketClient {
    async fn connect(&self, request: WebSocketConnectRequest) -> DomainResult<WebSocketHandle> {
        let client_request = build_client_request(&request)?;
        let connector = tls_connector(request.verify_ssl)?;
        let attempt = connect_async_tls_with_config(client_request, None, false, connector);

        let result = match request.connect_timeout {
            Some(limit) => tokio::time::timeout(limit, attempt)
                .await
                .map_err(|_| DomainError::Http("WebSocket connection timed out".into()))?,
            None => attempt.await,
        };
        let (stream, response) = result.map_err(|e| DomainError::Http(describe_error(&e)))?;

        let subprotocol = response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        let (command_tx, command_rx) = mpsc::channel(COMMAND_BUFFER);
        let (event_tx, event_rx) = mpsc::channel(EVENT_BUFFER);
        tokio::spawn(pump(stream, command_rx, event_tx, request.keep_alive_interval));

        Ok(WebSocketHandle {
            subprotocol,
            outbound: command_tx,
            events: event_rx,
        })
    }
}

/// Builds the handshake request: URL, caller headers and the subprotocol offer.
fn build_client_request(
    request: &WebSocketConnectRequest,
) -> DomainResult<tokio_tungstenite::tungstenite::http::Request<()>> {
    let scheme_ok = request.url.starts_with("ws://") || request.url.starts_with("wss://");
    if !scheme_ok {
        return Err(DomainError::InvalidInput(
            "WebSocket URL must start with ws:// or wss://".into(),
        ));
    }
    let mut built = request
        .url
        .as_str()
        .into_client_request()
        .map_err(|e| DomainError::InvalidInput(format!("invalid WebSocket URL: {e}")))?;

    let mut subprotocols: Vec<String> = Vec::new();
    for offered in &request.subprotocols {
        push_unique(&mut subprotocols, offered.trim());
    }
    for (name, value) in &request.headers {
        let lower = name.trim().to_ascii_lowercase();
        if lower == "sec-websocket-protocol" {
            for offered in value.split(',') {
                push_unique(&mut subprotocols, offered.trim());
            }
            continue;
        }
        if RESERVED_HEADERS.contains(&lower.as_str()) {
            return Err(DomainError::InvalidInput(format!(
                "header '{name}' is managed by the WebSocket handshake and cannot be set"
            )));
        }
        let header_name = HeaderName::from_bytes(lower.as_bytes())
            .map_err(|_| DomainError::InvalidInput(format!("invalid header name '{name}'")))?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|_| DomainError::InvalidInput(format!("invalid value for header '{name}'")))?;
        built.headers_mut().append(header_name, header_value);
    }
    if !subprotocols.is_empty() {
        let joined = HeaderValue::from_str(&subprotocols.join(", "))
            .map_err(|_| DomainError::InvalidInput("invalid WebSocket subprotocol".into()))?;
        built.headers_mut().insert("sec-websocket-protocol", joined);
    }
    Ok(built)
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !list.iter().any(|existing| existing == value) {
        list.push(value.to_string());
    }
}

/// The default connector verifies certificates against the OS store, exactly like reqwest.
/// Only an explicit `verify_ssl: false` builds the permissive one.
fn tls_connector(verify_ssl: bool) -> DomainResult<Option<Connector>> {
    if verify_ssl {
        return Ok(None);
    }
    let connector = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .map_err(|e| DomainError::Internal(format!("TLS setup failed: {e}")))?;
    Ok(Some(Connector::NativeTls(connector)))
}

/// A user-facing message that never includes header values or the request URL.
fn describe_error(error: &WsError) -> String {
    match error {
        WsError::Http(response) => {
            format!("handshake rejected with HTTP {}", response.status().as_u16())
        }
        WsError::Tls(e) => format!("TLS handshake failed: {e}"),
        WsError::Io(e) => format!("connection failed: {e}"),
        WsError::Url(e) => format!("invalid WebSocket URL: {e}"),
        other => format!("WebSocket error: {other}"),
    }
}

fn to_message(frame: WebSocketFrame) -> Message {
    match frame {
        WebSocketFrame::Text(text) => Message::text(text),
        WebSocketFrame::Binary(bytes) => Message::binary(bytes),
    }
}

fn unclean(reason: String) -> WebSocketClose {
    WebSocketClose {
        code: None,
        reason,
        clean: false,
    }
}

/// Owns the socket. Sends exactly one `Closed` event, then returns.
async fn pump(
    mut stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut commands: mpsc::Receiver<WebSocketCommand>,
    events: mpsc::Sender<WebSocketEvent>,
    keep_alive: Option<Duration>,
) {
    let mut ticker = keep_alive.filter(|d| !d.is_zero()).map(|period| {
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval
    });
    let mut closing_deadline: Option<tokio::time::Instant> = None;
    let mut commands_open = true;

    let outcome: WebSocketClose = loop {
        tokio::select! {
            command = commands.recv(), if commands_open => match command {
                Some(WebSocketCommand::Send(frame)) => {
                    if let Err(e) = stream.send(to_message(frame)).await {
                        break unclean(describe_error(&e));
                    }
                }
                Some(WebSocketCommand::Close { code, reason }) => {
                    let frame = CloseFrame { code: CloseCode::from(code), reason: reason.into() };
                    if let Err(e) = stream.close(Some(frame)).await {
                        break unclean(describe_error(&e));
                    }
                    closing_deadline = Some(tokio::time::Instant::now() + CLOSE_REPLY_TIMEOUT);
                    commands_open = false;
                }
                None => {
                    // Every sender is gone: close politely and wait for the reply.
                    let frame = CloseFrame { code: CloseCode::Normal, reason: "".into() };
                    let _ = stream.close(Some(frame)).await;
                    closing_deadline = Some(tokio::time::Instant::now() + CLOSE_REPLY_TIMEOUT);
                    commands_open = false;
                }
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let event = WebSocketEvent::Frame(WebSocketFrame::Text(text.as_str().to_string()));
                    if events.send(event).await.is_err() {
                        break unclean("listener dropped".into());
                    }
                }
                Some(Ok(Message::Binary(bytes))) => {
                    let event = WebSocketEvent::Frame(WebSocketFrame::Binary(bytes.to_vec()));
                    if events.send(event).await.is_err() {
                        break unclean("listener dropped".into());
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    break WebSocketClose {
                        code: frame.as_ref().map(|f| u16::from(f.code)),
                        reason: frame.map(|f| f.reason.as_str().to_string()).unwrap_or_default(),
                        clean: true,
                    };
                }
                // Pings are answered by the library and pongs need no handling.
                Some(Ok(_)) => {}
                Some(Err(WsError::ConnectionClosed)) => {
                    break WebSocketClose { code: None, reason: String::new(), clean: closing_deadline.is_some() };
                }
                Some(Err(e)) => {
                    break unclean(describe_error(&e));
                }
                None => {
                    break WebSocketClose {
                        code: None,
                        reason: "connection ended".into(),
                        clean: closing_deadline.is_some(),
                    };
                }
            },
            _ = async {
                match ticker.as_mut() {
                    Some(t) => { t.tick().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                if let Err(e) = stream.send(Message::Ping(Vec::<u8>::new().into())).await {
                    break unclean(describe_error(&e));
                }
            }
            _ = async {
                match closing_deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                break unclean("close reply timed out".into());
            }
        }
    };
    // The receiver may already be gone; then there is nobody left to tell.
    let _ = events.send(WebSocketEvent::Closed(outcome)).await;
}
```

In `crates/rocket-infra/src/lib.rs` add `mod websocket_client;` (with the other `mod` lines) and `pub use websocket_client::TungsteniteWebSocketClient;` next to `pub use secret_store::KeyringSecretStore;`. Add a short entry for `websocket_client.rs` to `crates/rocket-infra/CLAUDE.md`.

- [ ] **Step 9: Run the client tests**

Run: `cargo test -j4 -p rocket-infra websocket_client`
Expected: 14 passed. If a `tungstenite 0.30` method name differs from what is written above (for example `Message::text`, `Message::binary` or `Utf8Bytes::as_str`), fix the call from the compiler error; the API was checked against the downloaded 0.30.0 source (`Message::Text(Utf8Bytes)`, `Message::Binary(Bytes)`, `Message::Ping(Bytes)`, `CloseFrame { code, reason: Utf8Bytes }`).
If `keep_alive_pings_reach_the_server` is flaky on a loaded machine, raise the 5 second deadline; do not lower the ping interval below 50 ms.

- [ ] **Step 10: Verify**

Run: `cargo check -j4`, `cargo clippy -j4 -p rocket-http -p rocket-infra -- -D warnings`.
Expected: clean.

- [ ] **Step 11: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path: `crates/rocket-http/src/websocket.rs`, `crates/rocket-http/src/lib.rs`, `crates/rocket-http/CLAUDE.md`, `crates/rocket-infra/Cargo.toml`, `Cargo.lock`, `crates/rocket-infra/src/websocket_client.rs`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-infra/CLAUDE.md`.
Suggested subject: `feat(websocket): WebSocketClient port and tokio-tungstenite implementation`.

---

## Task 3: Session service, resolution, Tauri commands and events

**Files:**
- Create: `crates/rocket-app/src/execution_service/websocket_resolution.rs`, `crates/rocket-app/src/websocket_service.rs`, `src-tauri/src/commands/websocket.rs`
- Modify: `crates/rocket-shared/src/events.rs`, `crates/rocket-app/src/execution_service.rs` (one `mod` line), `crates/rocket-app/src/lib.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/tauri_event_bus.rs`, `crates/rocket-app/CLAUDE.md`

**Interfaces:**
- Consumes: `WebSocketClient`, `WebSocketConnectRequest`, `WebSocketFrame`, `WebSocketCommand`, `WebSocketEvent`, `WebSocketClose`, `WebSocketHandle` (Task 2); `WebSocketMessageKind` (Task 1); `RequestExecutionService::{build_variable_context, resolve_external_secrets}` and the private `merge_auth`, `merge_headers`, `resolve_auth`.
- Produces:
  - `rocket_shared::events::{WebSocketDirection, WebSocketPayloadKind, WebSocketSessionState}` and `DomainEvent::{WebSocketMessage, WebSocketStatus}`.
  - `rocket_app::{WebSocketScope, WebSocketConnectInput, WebSocketSendInput}` (Deserialize, camelCase) and, on `RequestExecutionService`:
    - `pub async fn resolve_websocket(&self, input: &WebSocketConnectInput) -> DomainResult<WebSocketConnectRequest>`
    - `pub async fn resolve_websocket_message(&self, scope: &WebSocketScope, kind: WebSocketMessageKind, data: &str) -> DomainResult<WebSocketFrame>`
  - `rocket_app::WebSocketService`: `new(Arc<dyn WebSocketClient>, Arc<dyn EventPublisher>)`, `async connect(&self, session_id: &str, request: WebSocketConnectRequest) -> DomainResult<()>`, `async send(&self, session_id: &str, frame: WebSocketFrame) -> DomainResult<()>`, `async disconnect(&self, session_id: &str) -> DomainResult<()>`, `async end_all_sessions(&self)`, `session_count(&self) -> usize`.
  - Tauri commands `ws_connect(sessionId, input)`, `ws_send(sessionId, input)`, `ws_disconnect(sessionId)`; Tauri events `ws:message` and `ws:status`.

Wire shape of the events (snake_case fields, camelCase variant tag):

```json
{ "type": "webSocketMessage", "session_id": "…", "direction": "in", "kind": "text", "data": "…", "size": 5, "timestamp_ms": 1759650000000 }
{ "type": "webSocketStatus", "session_id": "…", "state": "open", "subprotocol": "graphql-transport-ws", "code": null, "reason": null }
```

`data` is the text itself for `kind: "text"` and base64 for `kind: "binary"`. `state` is `connecting | open | closed | failed`; `closed` is a clean close, `failed` is a refused connect or an unclean end.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

### Step group A: events

- [ ] **Step 2: Write the failing event test**

In `crates/rocket-shared/src/events.rs`, in its `tests` module (next to the `AcpSession*` serialization tests), add:

```rust
    #[test]
    fn websocket_events_serialize_with_snake_case_fields_and_lowercase_enums() {
        let message = DomainEvent::WebSocketMessage {
            session_id: "s1".into(),
            direction: WebSocketDirection::In,
            kind: WebSocketPayloadKind::Binary,
            data: "AQID".into(),
            size: 3,
            timestamp_ms: 42,
        };
        let json = serde_json::to_value(&message).expect("serialize");
        assert_eq!(json["type"], "webSocketMessage");
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["direction"], "in");
        assert_eq!(json["kind"], "binary");
        assert_eq!(json["timestamp_ms"], 42);

        let status = DomainEvent::WebSocketStatus {
            session_id: "s1".into(),
            state: WebSocketSessionState::Failed,
            subprotocol: None,
            code: Some(4001),
            reason: Some("bye".into()),
        };
        let json = serde_json::to_value(&status).expect("serialize");
        assert_eq!(json["type"], "webSocketStatus");
        assert_eq!(json["state"], "failed");
        assert_eq!(json["code"], 4001);
        assert!(json["subprotocol"].is_null());
    }
```

- [ ] **Step 3: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-shared websocket_events`
Expected: compile error, variants not found.

- [ ] **Step 4: Implement the events**

In `crates/rocket-shared/src/events.rs`, above `DomainEvent`, add:

```rust
/// Direction of a WebSocket frame from the client's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSocketDirection {
    In,
    Out,
}

/// Wire kind of a logged frame. `Binary` data is base64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSocketPayloadKind {
    Text,
    Binary,
}

/// Lifecycle of a streaming session. `Closed` is a clean close; `Failed` is a
/// refused connect or an unclean end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSocketSessionState {
    Connecting,
    Open,
    Closed,
    Failed,
}
```

Inside `DomainEvent`, after `AcpSessionFailed { .. }`:

```rust
    // WebSocket session events
    /// One frame sent or received on a WebSocket session.
    WebSocketMessage {
        session_id: String,
        direction: WebSocketDirection,
        kind: WebSocketPayloadKind,
        /// Text as is, or base64 for binary frames.
        data: String,
        /// Payload size in bytes (before base64).
        size: usize,
        timestamp_ms: i64,
    },
    /// A WebSocket session changed state. `code` and `reason` describe a close.
    WebSocketStatus {
        session_id: String,
        state: WebSocketSessionState,
        subprotocol: Option<String>,
        code: Option<u16>,
        reason: Option<String>,
    },
```

In `src-tauri/src/tauri_event_bus.rs` add, after the `AcpSessionFailed` arm:

```rust
            // WebSocket sessions: one channel for frames, one for lifecycle.
            DomainEvent::WebSocketMessage { .. } => "ws:message",
            DomainEvent::WebSocketStatus { .. } => "ws:status",
```

- [ ] **Step 5: Run it**

Run: `cargo test -j4 -p rocket-shared websocket_events`
Expected: passed. (`cargo check -j4` of `src-tauri` is part of the final verification.)

### Step group B: resolution

- [ ] **Step 6: Write the failing resolution tests**

Create `crates/rocket-app/src/execution_service/websocket_resolution.rs` containing only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_service::RequestExecutionService;
    use crate::test_doubles::{
        EmptySecretManagerRepo, InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo,
        RecordingExecutor, SharedCollectionRepo, SharedHistoryRepo, StaticEnvRepo,
    };
    use rocket_collection::{Collection, CollectionSettings};
    use rocket_environment::{Environment, Variable};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Arc;

    fn service(env: Environment, settings: CollectionSettings) -> RequestExecutionService {
        let mut collection = Collection::new("api");
        collection.settings = settings;
        RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            RecordingExecutor::new(),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(collection))),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
    }

    fn dev_env() -> Environment {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("host", "chat.example.com"));
        env.set_variable(Variable::new("token", "abc123"));
        env
    }

    fn input(url: &str) -> WebSocketConnectInput {
        serde_json::from_value(serde_json::json!({
            "url": url,
            "collection": "api",
            "environmentName": "dev"
        }))
        .expect("input")
    }

    fn header<'a>(request: &'a WebSocketConnectRequest, name: &str) -> Option<&'a str> {
        request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[tokio::test]
    async fn variables_are_resolved_in_the_url_and_headers() {
        let svc = service(dev_env(), CollectionSettings::default());
        let mut i = input("wss://{{host}}/ws");
        i.headers = vec![Header::new("X-Token", "{{token}}")];

        let resolved = svc.resolve_websocket(&i).await.expect("resolve");

        assert_eq!(resolved.url, "wss://chat.example.com/ws");
        assert_eq!(header(&resolved, "X-Token"), Some("abc123"));
    }

    #[tokio::test]
    async fn collection_headers_and_auth_apply_and_request_values_win() {
        let settings = CollectionSettings {
            headers: vec![Header::new("X-Team", "core"), Header::new("X-Token", "collection")],
            auth: Some(Auth::Bearer { token: "from-collection".into() }),
            ..CollectionSettings::default()
        };
        let svc = service(dev_env(), settings);
        let mut i = input("wss://h/ws");
        i.headers = vec![Header::new("X-Token", "request")];
        i.auth = Some(Auth::Inherit);

        let resolved = svc.resolve_websocket(&i).await.expect("resolve");

        assert_eq!(header(&resolved, "X-Team"), Some("core"));
        assert_eq!(header(&resolved, "X-Token"), Some("request"));
        assert_eq!(header(&resolved, "Authorization"), Some("Bearer from-collection"));
    }

    #[tokio::test]
    async fn disabled_headers_are_not_sent() {
        let svc = service(dev_env(), CollectionSettings::default());
        let mut i = input("wss://h/ws");
        i.headers = vec![Header::disabled("X-Off", "1"), Header::new("X-On", "1")];

        let resolved = svc.resolve_websocket(&i).await.expect("resolve");

        assert_eq!(header(&resolved, "X-Off"), None);
        assert_eq!(header(&resolved, "X-On"), Some("1"));
    }

    #[tokio::test]
    async fn basic_bearer_and_api_key_auth_become_handshake_credentials() {
        let svc = service(dev_env(), CollectionSettings::default());

        let mut basic = input("wss://h/ws");
        basic.auth = Some(Auth::Basic { username: "u".into(), password: "p".into() });
        let r = svc.resolve_websocket(&basic).await.expect("basic");
        assert_eq!(header(&r, "Authorization"), Some("Basic dTpw"));

        let mut bearer = input("wss://h/ws");
        bearer.auth = Some(Auth::Bearer { token: "{{token}}".into() });
        let r = svc.resolve_websocket(&bearer).await.expect("bearer");
        assert_eq!(header(&r, "Authorization"), Some("Bearer abc123"));

        let mut key_header = input("wss://h/ws");
        key_header.auth = Some(Auth::ApiKey { key: "X-Key".into(), value: "k".into(), placement: "header".into() });
        let r = svc.resolve_websocket(&key_header).await.expect("api key header");
        assert_eq!(header(&r, "X-Key"), Some("k"));

        let mut key_query = input("wss://h/ws?a=1");
        key_query.auth = Some(Auth::ApiKey { key: "api_key".into(), value: "k 1".into(), placement: "query".into() });
        let r = svc.resolve_websocket(&key_query).await.expect("api key query");
        assert_eq!(r.url, "wss://h/ws?a=1&api_key=k+1");
        assert_eq!(header(&r, "api_key"), None);
    }

    #[tokio::test]
    async fn an_unsupported_auth_type_is_an_explicit_error_not_an_anonymous_connect() {
        let svc = service(dev_env(), CollectionSettings::default());
        let mut i = input("wss://h/ws");
        i.auth = Some(Auth::Digest { username: "u".into(), password: "p".into() });

        let err = svc.resolve_websocket(&i).await.expect_err("digest is unsupported");

        assert!(err.to_string().contains("Digest"), "{err}");
        assert!(err.to_string().contains("not supported"), "{err}");
    }

    #[tokio::test]
    async fn timeouts_and_tls_default_sensibly_and_zero_means_off() {
        let svc = service(dev_env(), CollectionSettings::default());

        let defaults = svc.resolve_websocket(&input("wss://h/ws")).await.expect("defaults");
        assert_eq!(defaults.connect_timeout, Some(std::time::Duration::from_millis(30_000)));
        assert_eq!(defaults.keep_alive_interval, None);
        assert!(defaults.verify_ssl);

        let mut custom = input("wss://h/ws");
        custom.timeout_ms = Some(0);
        custom.keep_alive_ms = Some(2500);
        custom.verify_ssl = Some(false);
        let r = svc.resolve_websocket(&custom).await.expect("custom");
        assert_eq!(r.connect_timeout, None);
        assert_eq!(r.keep_alive_interval, Some(std::time::Duration::from_millis(2500)));
        assert!(!r.verify_ssl);
    }

    #[tokio::test]
    async fn message_variables_resolve_and_binary_is_decoded_from_base64() {
        let svc = service(dev_env(), CollectionSettings::default());
        let scope = WebSocketScope {
            collection: Some("api".into()),
            environment_name: Some("dev".into()),
            ..WebSocketScope::default()
        };

        let text = svc
            .resolve_websocket_message(&scope, WebSocketMessageKind::Json, "{\"t\":\"{{token}}\"}")
            .await
            .expect("json");
        assert_eq!(text, WebSocketFrame::Text("{\"t\":\"abc123\"}".into()));

        let bytes = svc
            .resolve_websocket_message(&scope, WebSocketMessageKind::Binary, "AQID\n")
            .await
            .expect("binary");
        assert_eq!(bytes, WebSocketFrame::Binary(vec![1, 2, 3]));

        let err = svc
            .resolve_websocket_message(&scope, WebSocketMessageKind::Binary, "not base64!!")
            .await
            .expect_err("invalid base64");
        assert!(err.to_string().contains("base64"), "{err}");
    }

    #[test]
    fn inputs_deserialize_from_the_camel_case_ipc_shape() {
        let i: WebSocketConnectInput = serde_json::from_value(serde_json::json!({
            "url": "wss://h/ws",
            "headers": [{ "key": "A", "value": "1", "enabled": true }],
            "auth": { "authType": "bearer", "token": "t" },
            "subprotocols": ["graphql-transport-ws"],
            "timeoutMs": 1000,
            "keepAliveMs": 500,
            "verifySsl": false,
            "collection": "api",
            "environmentName": "dev",
            "globalEnvName": "g",
            "requestPath": "chat.yml"
        }))
        .expect("deserialize");
        assert_eq!(i.subprotocols, vec!["graphql-transport-ws".to_string()]);
        assert_eq!(i.timeout_ms, Some(1000));
        assert_eq!(i.scope.request_path.as_deref(), Some("chat.yml"));

        let s: WebSocketSendInput = serde_json::from_value(serde_json::json!({
            "kind": "binary", "data": "AQID", "collection": "api"
        }))
        .expect("send input");
        assert_eq!(s.kind, WebSocketMessageKind::Binary);
        assert_eq!(s.scope.collection.as_deref(), Some("api"));
    }
}
```

- [ ] **Step 7: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-app websocket_resolution`
Expected: compile errors (module not declared, types missing).

- [ ] **Step 8: Implement the resolution module**

Prepend to `crates/rocket-app/src/execution_service/websocket_resolution.rs`:

```rust
//! Turns a WebSocket connect request or message into something the client can send:
//! `{{variables}}` resolved, collection defaults merged, auth turned into a header or a
//! query parameter. This is a child module of `execution_service` so it can reuse the
//! private merge and resolve helpers that HTTP sends already use.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rocket_collection::WebSocketMessageKind;
use rocket_environment::resolve;
use rocket_http::websocket::{
    WebSocketConnectRequest, WebSocketFrame, DEFAULT_CONNECT_TIMEOUT_MS,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Auth, Header};
use serde::Deserialize;

use super::{merge_auth, merge_headers, resolve_auth, RequestExecutionService};

/// Where `{{variables}}` come from. Mirrors the scope fields of `ExecuteRequestInput`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketScope {
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
    #[serde(default)]
    pub global_env_name: Option<String>,
    #[serde(default)]
    pub request_path: Option<String>,
}

/// IPC input of `ws_connect`. Built from the open tab, like `ExecuteRequestInput`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketConnectInput {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub auth: Option<Auth>,
    #[serde(default)]
    pub subprotocols: Vec<String>,
    /// Connect timeout in milliseconds. `None` uses the default, `0` waits forever.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Milliseconds between client pings. `None` or `0` sends none.
    #[serde(default)]
    pub keep_alive_ms: Option<u64>,
    #[serde(default)]
    pub verify_ssl: Option<bool>,
    #[serde(flatten)]
    pub scope: WebSocketScope,
}

/// IPC input of `ws_send`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketSendInput {
    pub kind: WebSocketMessageKind,
    pub data: String,
    #[serde(flatten)]
    pub scope: WebSocketScope,
}

fn millis(value: u64) -> Option<Duration> {
    if value == 0 {
        None
    } else {
        Some(Duration::from_millis(value))
    }
}

fn auth_label(auth: &Auth) -> &'static str {
    match auth {
        Auth::OAuth2(_) => "OAuth 2.0",
        Auth::OAuth1(_) => "OAuth 1.0",
        Auth::Digest { .. } => "Digest",
        Auth::Ntlm { .. } => "NTLM",
        Auth::Wsse { .. } => "WSSE",
        Auth::AwsSigV4 { .. } => "AWS Signature",
        _ => "This",
    }
}

fn append_query(url: &str, key: &str, value: &str) -> DomainResult<String> {
    let mut parsed = url::Url::parse(url)
        .map_err(|_| DomainError::InvalidInput("invalid WebSocket URL".into()))?;
    parsed.query_pairs_mut().append_pair(key, value);
    Ok(parsed.to_string())
}

/// Applies a resolved auth value to the handshake. Anything that needs more than a
/// header or a query parameter is refused loudly rather than connecting anonymously.
fn apply_auth(
    auth: &Auth,
    url: &mut String,
    headers: &mut Vec<(String, String)>,
) -> DomainResult<()> {
    match auth {
        Auth::None | Auth::Inherit => Ok(()),
        Auth::Basic { username, password } => {
            let encoded = STANDARD.encode(format!("{username}:{password}"));
            headers.push(("Authorization".into(), format!("Basic {encoded}")));
            Ok(())
        }
        Auth::Bearer { token } => {
            headers.push(("Authorization".into(), format!("Bearer {token}")));
            Ok(())
        }
        Auth::ApiKey { key, value, placement } => match placement.as_str() {
            "header" => {
                headers.push((key.clone(), value.clone()));
                Ok(())
            }
            "query" => {
                *url = append_query(url, key, value)?;
                Ok(())
            }
            _ => Err(DomainError::InvalidInput(
                "API key placement must be header or query".into(),
            )),
        },
        other => Err(DomainError::InvalidInput(format!(
            "{} auth is not supported for WebSocket connections yet",
            auth_label(other)
        ))),
    }
}

impl RequestExecutionService {
    /// Resolves a connect input into a ready-to-send handshake request.
    ///
    /// External secrets use the strict `resolve_external_secrets`: any failing binding fails the
    /// connect. HTTP sends tolerate a failing binding the request never references; this does not.
    pub async fn resolve_websocket(
        &self,
        input: &WebSocketConnectInput,
    ) -> DomainResult<WebSocketConnectRequest> {
        let scope = &input.scope;
        let secrets = self
            .resolve_external_secrets(scope.collection.as_deref(), scope.environment_name.as_deref())
            .await?;
        let vars = self.build_variable_context(
            scope.global_env_name.as_deref(),
            scope.collection.as_deref(),
            scope.environment_name.as_deref(),
            scope.request_path.as_deref(),
            &secrets,
        );

        let request_auth = input.auth.clone().unwrap_or(Auth::None);
        let (auth, headers) = match scope.collection.as_deref() {
            Some(collection) => {
                let settings = self.collection_repo.get_settings(collection).unwrap_or_default();
                (
                    merge_auth(request_auth, settings.auth),
                    merge_headers(&settings.headers, &input.headers),
                )
            }
            None => (request_auth, input.headers.clone()),
        };
        let auth = resolve_auth(auth, &vars);

        let mut url = resolve(&input.url, &vars).output;
        let mut wire_headers: Vec<(String, String)> = headers
            .iter()
            .filter(|h| h.enabled && !h.key.trim().is_empty())
            .map(|h| (resolve(&h.key, &vars).output, resolve(&h.value, &vars).output))
            .collect();
        apply_auth(&auth, &mut url, &mut wire_headers)?;

        Ok(WebSocketConnectRequest {
            url,
            headers: wire_headers,
            subprotocols: input
                .subprotocols
                .iter()
                .map(|s| resolve(s, &vars).output)
                .collect(),
            connect_timeout: millis(input.timeout_ms.unwrap_or(DEFAULT_CONNECT_TIMEOUT_MS)),
            keep_alive_interval: millis(input.keep_alive_ms.unwrap_or(0)),
            verify_ssl: input.verify_ssl.unwrap_or(true),
        })
    }

    /// Resolves one outgoing message. Text kinds get `{{variables}}`; binary data is base64
    /// (whitespace ignored) and is decoded after substitution.
    ///
    /// External secrets are only fetched when the message actually contains a placeholder, so a
    /// plain message never costs a vault round trip.
    pub async fn resolve_websocket_message(
        &self,
        scope: &WebSocketScope,
        kind: WebSocketMessageKind,
        data: &str,
    ) -> DomainResult<WebSocketFrame> {
        let secrets = if data.contains("{{") {
            self.resolve_external_secrets(
                scope.collection.as_deref(),
                scope.environment_name.as_deref(),
            )
            .await?
        } else {
            std::collections::HashMap::new()
        };
        let vars = self.build_variable_context(
            scope.global_env_name.as_deref(),
            scope.collection.as_deref(),
            scope.environment_name.as_deref(),
            scope.request_path.as_deref(),
            &secrets,
        );
        let resolved = resolve(data, &vars).output;
        match kind {
            WebSocketMessageKind::Binary => {
                let compact: String = resolved.split_whitespace().collect();
                STANDARD
                    .decode(compact.as_bytes())
                    .map(WebSocketFrame::Binary)
                    .map_err(|_| {
                        DomainError::InvalidInput("binary message must be valid base64".into())
                    })
            }
            _ => Ok(WebSocketFrame::Text(resolved)),
        }
    }
}
```

In `crates/rocket-app/src/execution_service.rs` add, directly under the existing top-level `use` lines (before the first `struct`/`impl`):

```rust
pub mod websocket_resolution;
```

In `crates/rocket-app/src/lib.rs` add, near the `pub use execution_service::{...}` line:

```rust
pub use execution_service::websocket_resolution::{
    WebSocketConnectInput, WebSocketScope, WebSocketSendInput,
};
```

Rust looks for the child module at `crates/rocket-app/src/execution_service/websocket_resolution.rs`, next to `execution_service.rs`; no `mod.rs` is needed.

- [ ] **Step 9: Run the resolution tests**

Run: `cargo test -j4 -p rocket-app websocket_resolution`
Expected: 8 passed. If `build_variable_context` returns nothing for `dev`, check that `StaticEnvRepo::get` is reached through the `regular_env_repo()` fallback (no factory is set in this test, so it is).

### Step group C: session service

- [ ] **Step 10: Write the failing service tests**

Create `crates/rocket-app/src/websocket_service.rs` containing only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::RecordingPublisher;
    use std::sync::Mutex;
    use tokio::sync::{mpsc, Notify};

    /// Test side of one fake connection.
    struct Endpoints {
        commands: mpsc::Receiver<WebSocketCommand>,
        events: mpsc::Sender<WebSocketEvent>,
    }

    /// A scriptable client. `connect` can be held at a gate, or made to fail.
    #[derive(Default)]
    struct FakeClient {
        gate: Option<Arc<Notify>>,
        fail_with: Option<String>,
        subprotocol: Option<String>,
        endpoints: Mutex<Vec<Endpoints>>,
    }

    impl FakeClient {
        fn take(&self) -> Endpoints {
            self.endpoints.lock().expect("lock").remove(0)
        }
    }

    #[async_trait::async_trait]
    impl WebSocketClient for FakeClient {
        async fn connect(&self, _request: WebSocketConnectRequest) -> DomainResult<WebSocketHandle> {
            if let Some(gate) = &self.gate {
                gate.notified().await;
            }
            if let Some(message) = &self.fail_with {
                return Err(DomainError::Http(message.clone()));
            }
            let (command_tx, command_rx) = mpsc::channel(8);
            let (event_tx, event_rx) = mpsc::channel(8);
            self.endpoints.lock().expect("lock").push(Endpoints {
                commands: command_rx,
                events: event_tx,
            });
            Ok(WebSocketHandle {
                subprotocol: self.subprotocol.clone(),
                outbound: command_tx,
                events: event_rx,
            })
        }
    }

    fn request() -> WebSocketConnectRequest {
        WebSocketConnectRequest {
            url: "ws://x/ws".into(),
            headers: Vec::new(),
            subprotocols: Vec::new(),
            connect_timeout: None,
            keep_alive_interval: None,
            verify_ssl: true,
        }
    }

    fn service(client: &Arc<FakeClient>) -> (WebSocketService, Arc<RecordingPublisher>) {
        let publisher = RecordingPublisher::new();
        (WebSocketService::new(client.clone(), publisher.clone()), publisher)
    }

    async fn wait_for(publisher: &RecordingPublisher, what: &str, pred: impl Fn(&[DomainEvent]) -> bool) {
        for _ in 0..400 {
            if pred(&publisher.events()) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("timed out waiting for {what}; events: {:?}", publisher.events());
    }

    fn statuses(events: &[DomainEvent]) -> Vec<WebSocketSessionState> {
        events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::WebSocketStatus { state, .. } => Some(*state),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn connect_publishes_connecting_then_open_with_the_subprotocol() {
        let client = Arc::new(FakeClient { subprotocol: Some("graphql-transport-ws".into()), ..Default::default() });
        let (svc, publisher) = service(&client);

        svc.connect("s1", request()).await.expect("connect");

        let events = publisher.events();
        assert_eq!(statuses(&events), vec![WebSocketSessionState::Connecting, WebSocketSessionState::Open]);
        match &events[1] {
            DomainEvent::WebSocketStatus { session_id, subprotocol, .. } => {
                assert_eq!(session_id, "s1");
                assert_eq!(subprotocol.as_deref(), Some("graphql-transport-ws"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(svc.session_count(), 1);
    }

    #[tokio::test]
    async fn inbound_frames_are_published_in_order_with_binary_as_base64() {
        let client = Arc::new(FakeClient::default());
        let (svc, publisher) = service(&client);
        svc.connect("s1", request()).await.expect("connect");
        let endpoints = client.take();

        endpoints.events.send(WebSocketEvent::Frame(WebSocketFrame::Text("one".into()))).await.expect("send");
        endpoints.events.send(WebSocketEvent::Frame(WebSocketFrame::Binary(vec![1, 2, 3]))).await.expect("send");
        wait_for(&publisher, "two messages", |e| {
            e.iter().filter(|x| matches!(x, DomainEvent::WebSocketMessage { .. })).count() == 2
        })
        .await;

        let messages: Vec<_> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::WebSocketMessage { direction, kind, data, size, .. } => Some((direction, kind, data, size)),
                _ => None,
            })
            .collect();
        assert_eq!(messages[0], (WebSocketDirection::In, WebSocketPayloadKind::Text, "one".to_string(), 3));
        assert_eq!(messages[1], (WebSocketDirection::In, WebSocketPayloadKind::Binary, "AQID".to_string(), 3));
    }

    #[tokio::test]
    async fn send_enqueues_the_frame_and_publishes_an_out_message() {
        let client = Arc::new(FakeClient::default());
        let (svc, publisher) = service(&client);
        svc.connect("s1", request()).await.expect("connect");
        let mut endpoints = client.take();

        svc.send("s1", WebSocketFrame::Text("hello".into())).await.expect("send");

        assert_eq!(
            endpoints.commands.recv().await,
            Some(WebSocketCommand::Send(WebSocketFrame::Text("hello".into())))
        );
        assert!(publisher.events().iter().any(|e| matches!(
            e,
            DomainEvent::WebSocketMessage { direction: WebSocketDirection::Out, data, .. } if data == "hello"
        )));

        let err = svc.send("nope", WebSocketFrame::Text("x".into())).await.expect_err("unknown session");
        assert!(matches!(err, DomainError::NotFound(_)), "{err:?}");
    }

    #[tokio::test]
    async fn peer_close_publishes_one_terminal_status_and_frees_the_session() {
        let client = Arc::new(FakeClient::default());
        let (svc, publisher) = service(&client);
        svc.connect("s1", request()).await.expect("connect");
        let endpoints = client.take();

        endpoints
            .events
            .send(WebSocketEvent::Closed(WebSocketClose { code: Some(1000), reason: "bye".into(), clean: true }))
            .await
            .expect("close");
        wait_for(&publisher, "terminal status", |e| statuses(e).contains(&WebSocketSessionState::Closed)).await;

        assert_eq!(svc.session_count(), 0);
        let terminal = statuses(&publisher.events())
            .into_iter()
            .filter(|s| matches!(s, WebSocketSessionState::Closed | WebSocketSessionState::Failed))
            .count();
        assert_eq!(terminal, 1, "exactly one terminal status");
        assert!(svc.send("s1", WebSocketFrame::Text("late".into())).await.is_err());
        // The id can be reused.
        svc.connect("s1", request()).await.expect("reconnect with the same id");
    }

    #[tokio::test]
    async fn an_event_stream_that_ends_without_closed_is_reported_as_failed() {
        let client = Arc::new(FakeClient::default());
        let (svc, publisher) = service(&client);
        svc.connect("s1", request()).await.expect("connect");
        let endpoints = client.take();

        drop(endpoints.events); // the pump died without a Closed event
        wait_for(&publisher, "failed status", |e| statuses(e).contains(&WebSocketSessionState::Failed)).await;
        assert_eq!(svc.session_count(), 0);
    }

    #[tokio::test]
    async fn a_failed_connect_publishes_failed_returns_the_error_and_frees_the_id() {
        let client = Arc::new(FakeClient { fail_with: Some("handshake rejected with HTTP 401".into()), ..Default::default() });
        let (svc, publisher) = service(&client);

        let err = svc.connect("s1", request()).await.expect_err("must fail");

        assert!(err.to_string().contains("401"), "{err}");
        assert_eq!(statuses(&publisher.events()), vec![WebSocketSessionState::Connecting, WebSocketSessionState::Failed]);
        assert_eq!(svc.session_count(), 0);
    }

    #[tokio::test]
    async fn a_duplicate_session_id_is_rejected_while_the_first_is_open() {
        let client = Arc::new(FakeClient::default());
        let (svc, _publisher) = service(&client);
        svc.connect("s1", request()).await.expect("first");

        let err = svc.connect("s1", request()).await.expect_err("duplicate");

        assert!(matches!(err, DomainError::AlreadyExists(_)), "{err:?}");
        assert!(svc.connect("", request()).await.is_err(), "empty id");
    }

    #[tokio::test]
    async fn disconnect_sends_a_close_command_and_is_idempotent() {
        let client = Arc::new(FakeClient::default());
        let (svc, _publisher) = service(&client);
        svc.connect("s1", request()).await.expect("connect");
        let mut endpoints = client.take();

        svc.disconnect("s1").await.expect("disconnect");

        match endpoints.commands.recv().await {
            Some(WebSocketCommand::Close { code, .. }) => assert_eq!(code, 1000),
            other => panic!("expected Close, got {other:?}"),
        }
        svc.disconnect("s1").await.expect("second disconnect is a no-op");
        svc.disconnect("never-existed").await.expect("unknown id is a no-op");
    }

    #[tokio::test]
    async fn disconnect_during_connect_cancels_and_closes_the_late_socket() {
        let gate = Arc::new(Notify::new());
        let client = Arc::new(FakeClient { gate: Some(Arc::clone(&gate)), ..Default::default() });
        let (svc, _publisher) = service(&client);
        let svc = Arc::new(svc);

        let connecting = {
            let svc = Arc::clone(&svc);
            tokio::spawn(async move { svc.connect("s1", request()).await })
        };
        // Let the connect task reserve the id and park at the gate.
        for _ in 0..200 {
            if svc.session_count() == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(svc.session_count(), 1, "the id is reserved while connecting");

        svc.disconnect("s1").await.expect("cancel");
        gate.notify_one();

        let result = connecting.await.expect("join");
        assert!(matches!(result, Err(DomainError::Conflict(_))), "{result:?}");
        assert_eq!(svc.session_count(), 0);
        let mut endpoints = client.take();
        assert!(
            matches!(endpoints.commands.recv().await, Some(WebSocketCommand::Close { .. })),
            "the socket that opened after the cancel must be closed"
        );
    }

    #[tokio::test]
    async fn end_all_sessions_closes_every_open_session() {
        let client = Arc::new(FakeClient::default());
        let (svc, _publisher) = service(&client);
        svc.connect("a", request()).await.expect("a");
        svc.connect("b", request()).await.expect("b");
        let mut first = client.take();
        let mut second = client.take();

        svc.end_all_sessions().await;

        assert!(matches!(first.commands.recv().await, Some(WebSocketCommand::Close { .. })));
        assert!(matches!(second.commands.recv().await, Some(WebSocketCommand::Close { .. })));
        assert_eq!(svc.session_count(), 0);
    }
}
```

- [ ] **Step 11: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-app websocket_service`
Expected: compile errors (module not declared, `WebSocketService` missing).

- [ ] **Step 12: Implement the service**

Prepend to `crates/rocket-app/src/websocket_service.rs`:

```rust
//! WebSocket session registry. A session id is chosen by the caller (the frontend), because the
//! pump publishes events before `connect` returns and the frontend has to know which id to route.
//! Inbound frames and lifecycle changes leave as `DomainEvent`s, never as return values.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rocket_http::websocket::{
    WebSocketClient, WebSocketClose, WebSocketCommand, WebSocketConnectRequest, WebSocketEvent,
    WebSocketFrame, WebSocketHandle,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{
    DomainEvent, EventPublisher, WebSocketDirection, WebSocketPayloadKind, WebSocketSessionState,
};
use tokio::sync::mpsc;

/// A session slot. `None` reserves the id while the handshake is in flight.
type Slot = Option<mpsc::Sender<WebSocketCommand>>;

pub struct WebSocketService {
    client: Arc<dyn WebSocketClient>,
    events: Arc<dyn EventPublisher>,
    sessions: Arc<Mutex<HashMap<String, Slot>>>,
}

fn publish_status(
    events: &dyn EventPublisher,
    session_id: &str,
    state: WebSocketSessionState,
    subprotocol: Option<String>,
    close: Option<&WebSocketClose>,
) {
    events.publish(DomainEvent::WebSocketStatus {
        session_id: session_id.to_string(),
        state,
        subprotocol,
        code: close.and_then(|c| c.code),
        reason: close.map(|c| c.reason.clone()).filter(|r| !r.is_empty()),
    });
}

fn publish_frame(
    events: &dyn EventPublisher,
    session_id: &str,
    direction: WebSocketDirection,
    frame: &WebSocketFrame,
) {
    let (kind, data) = match frame {
        WebSocketFrame::Text(text) => (WebSocketPayloadKind::Text, text.clone()),
        WebSocketFrame::Binary(bytes) => (WebSocketPayloadKind::Binary, STANDARD.encode(bytes)),
    };
    events.publish(DomainEvent::WebSocketMessage {
        session_id: session_id.to_string(),
        direction,
        kind,
        data,
        size: frame.size(),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    });
}

impl WebSocketService {
    pub fn new(client: Arc<dyn WebSocketClient>, events: Arc<dyn EventPublisher>) -> Self {
        Self {
            client,
            events,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Slot>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Number of sessions, including ones still connecting.
    pub fn session_count(&self) -> usize {
        self.lock().len()
    }

    /// Opens a session. Publishes `Connecting`, then `Open` (with the negotiated subprotocol) or
    /// `Failed`. A `disconnect` that arrives while the handshake is in flight cancels it: the
    /// late socket is closed and this returns `Conflict`.
    pub async fn connect(
        &self,
        session_id: &str,
        request: WebSocketConnectRequest,
    ) -> DomainResult<()> {
        if session_id.trim().is_empty() {
            return Err(DomainError::InvalidInput("session id is required".into()));
        }
        {
            let mut sessions = self.lock();
            if sessions.contains_key(session_id) {
                return Err(DomainError::AlreadyExists(format!(
                    "WebSocket session '{session_id}'"
                )));
            }
            sessions.insert(session_id.to_string(), None);
        }
        publish_status(self.events.as_ref(), session_id, WebSocketSessionState::Connecting, None, None);

        let WebSocketHandle {
            subprotocol,
            outbound,
            events: inbound,
        } = match self.client.connect(request).await {
            Ok(handle) => handle,
            Err(error) => {
                self.lock().remove(session_id);
                let close = WebSocketClose {
                    code: None,
                    reason: error.to_string(),
                    clean: false,
                };
                publish_status(
                    self.events.as_ref(),
                    session_id,
                    WebSocketSessionState::Failed,
                    None,
                    Some(&close),
                );
                return Err(error);
            }
        };

        {
            let mut sessions = self.lock();
            match sessions.get_mut(session_id) {
                Some(slot @ None) => *slot = Some(outbound.clone()),
                _ => {
                    // Cancelled while connecting: do not leave the new socket open.
                    drop(sessions);
                    let _ = outbound
                        .send(WebSocketCommand::Close { code: 1000, reason: "cancelled".into() })
                        .await;
                    return Err(DomainError::Conflict("connection was cancelled".into()));
                }
            }
        }
        publish_status(
            self.events.as_ref(),
            session_id,
            WebSocketSessionState::Open,
            subprotocol,
            None,
        );
        self.spawn_pump(session_id.to_string(), inbound);
        Ok(())
    }

    /// Forwards inbound events as `DomainEvent`s, then publishes exactly one terminal status and
    /// frees the session id.
    fn spawn_pump(&self, session_id: String, mut inbound: mpsc::Receiver<WebSocketEvent>) {
        let events = Arc::clone(&self.events);
        let sessions = Arc::clone(&self.sessions);
        tokio::spawn(async move {
            let mut close: Option<WebSocketClose> = None;
            while let Some(event) = inbound.recv().await {
                match event {
                    WebSocketEvent::Frame(frame) => {
                        publish_frame(events.as_ref(), &session_id, WebSocketDirection::In, &frame);
                    }
                    WebSocketEvent::Closed(c) => {
                        close = Some(c);
                        break;
                    }
                }
            }
            // A stream that ended without `Closed` means the pump died: report it as unclean.
            let close = close.unwrap_or(WebSocketClose {
                code: None,
                reason: "connection ended unexpectedly".into(),
                clean: false,
            });
            sessions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&session_id);
            let state = if close.clean {
                WebSocketSessionState::Closed
            } else {
                WebSocketSessionState::Failed
            };
            publish_status(events.as_ref(), &session_id, state, None, Some(&close));
        });
    }

    /// Sends one frame on an open session and publishes it as an `Out` message.
    pub async fn send(&self, session_id: &str, frame: WebSocketFrame) -> DomainResult<()> {
        let sender = {
            let sessions = self.lock();
            match sessions.get(session_id) {
                Some(Some(sender)) => sender.clone(),
                Some(None) => {
                    return Err(DomainError::Conflict("session is still connecting".into()))
                }
                None => {
                    return Err(DomainError::NotFound(format!("WebSocket session '{session_id}'")))
                }
            }
        };
        sender
            .send(WebSocketCommand::Send(frame.clone()))
            .await
            .map_err(|_| DomainError::Conflict("session is closed".into()))?;
        publish_frame(self.events.as_ref(), session_id, WebSocketDirection::Out, &frame);
        Ok(())
    }

    /// Asks the session to close. The terminal status comes from the pump when the close
    /// finishes. Unknown or already-closed sessions are a no-op. A session still connecting is
    /// cancelled.
    pub async fn disconnect(&self, session_id: &str) -> DomainResult<()> {
        let slot = self.lock().remove(session_id);
        if let Some(Some(sender)) = slot {
            // The pump may already be gone; then there is nothing left to close.
            let _ = sender
                .send(WebSocketCommand::Close { code: 1000, reason: "client disconnect".into() })
                .await;
        }
        Ok(())
    }

    /// Closes every session. Used on app exit.
    pub async fn end_all_sessions(&self) {
        let drained: Vec<Slot> = self.lock().drain().map(|(_, slot)| slot).collect();
        for slot in drained.into_iter().flatten() {
            let _ = slot
                .send(WebSocketCommand::Close { code: 1001, reason: "app exit".into() })
                .await;
        }
    }
}
```

In `crates/rocket-app/src/lib.rs` add `pub mod websocket_service;` (after `pub mod vault_secret_resolution;`) and `pub use websocket_service::WebSocketService;` (after the `pub use vault_secret_resolution::...` line).

- [ ] **Step 13: Run the service tests**

Run: `cargo test -j4 -p rocket-app websocket_service`
Expected: 10 passed.

### Step group D: Tauri wiring

- [ ] **Step 14: Add the commands**

Create `src-tauri/src/commands/websocket.rs`:

```rust
use rocket_app::{RequestExecutionService, WebSocketConnectInput, WebSocketSendInput, WebSocketService};
use rocket_shared::error::DomainError;
use tauri::State;

/// Opens a session under a caller-chosen id. Frames and status changes arrive as the
/// `ws:message` and `ws:status` events; this only reports whether the connect succeeded.
#[tauri::command]
pub async fn ws_connect(
    session_id: String,
    input: WebSocketConnectInput,
    exec: State<'_, RequestExecutionService>,
    sessions: State<'_, WebSocketService>,
) -> Result<(), DomainError> {
    let request = exec.resolve_websocket(&input).await?;
    sessions.connect(&session_id, request).await
}

#[tauri::command]
pub async fn ws_send(
    session_id: String,
    input: WebSocketSendInput,
    exec: State<'_, RequestExecutionService>,
    sessions: State<'_, WebSocketService>,
) -> Result<(), DomainError> {
    let frame = exec
        .resolve_websocket_message(&input.scope, input.kind, &input.data)
        .await?;
    sessions.send(&session_id, frame).await
}

#[tauri::command]
pub async fn ws_disconnect(
    session_id: String,
    sessions: State<'_, WebSocketService>,
) -> Result<(), DomainError> {
    sessions.disconnect(&session_id).await
}
```

In `src-tauri/src/commands/mod.rs` add `pub mod websocket;` (after `pub mod ui_state;`).

In `src-tauri/src/lib.rs`:
1. After `let acp_session_svc = ...;` add:

```rust
            let websocket_svc = rocket_app::WebSocketService::new(
                Arc::new(rocket_infra::TungsteniteWebSocketClient::new()),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );
```

2. After `app.manage(acp_session_svc);` add `app.manage(websocket_svc);`.
3. In the invoke handler list, after `commands::acp_sessions::end_agent_session,` add:

```rust
            commands::websocket::ws_connect,
            commands::websocket::ws_send,
            commands::websocket::ws_disconnect,
```

4. Next to **both** existing ACP shutdown hooks (the signal handler near line 111 and the `RunEvent::Exit` arm near line 742) add the same best-effort sweep. In the signal handler:

```rust
            if let Some(websocket_svc) = app_handle.try_state::<rocket_app::WebSocketService>() {
                websocket_svc.end_all_sessions().await;
            }
```

In the `RunEvent::Exit` arm use `tauri::async_runtime::block_on(websocket_svc.end_all_sessions());`, mirroring the ACP line there.

Add a `WebSocketService` row and a `resolve_websocket` note to `crates/rocket-app/CLAUDE.md` (public types table and key patterns), and a `ws_*` entry to `.claude/tauri-commands.md` if that file exists in your checkout (the project CLAUDE.md references it).

- [ ] **Step 15: Verify**

Run, in order:
- `cargo check -j4` (the whole workspace, including `src-tauri`; this proves the exhaustive `TauriEventBus` match and the command signatures).
- `cargo test -j4 -p rocket-shared websocket_events`
- `cargo test -j4 -p rocket-app websocket`
- `cargo test -j4 -p rocket-infra websocket_client`
- `cargo clippy -j4 -p rocket-shared -p rocket-http -p rocket-infra -p rocket-app -- -D warnings`

Expected: all green. Do not run the whole workspace test suite.

- [ ] **Step 16: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path: `crates/rocket-shared/src/events.rs`, `crates/rocket-app/src/execution_service/websocket_resolution.rs`, `crates/rocket-app/src/execution_service.rs`, `crates/rocket-app/src/websocket_service.rs`, `crates/rocket-app/src/lib.rs`, `crates/rocket-app/CLAUDE.md`, `src-tauri/src/commands/websocket.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/tauri_event_bus.rs`. Stage `execution_service.rs` only if `git diff` shows nothing but the one `pub mod` line; if a peer session has other edits in it, stop and ask rather than staging them.
Suggested subject: `feat(websocket): session service, handshake resolution and ws commands`.

---

## Known limits (state them in the PR description)

- No real `wss://` handshake is exercised in tests. Check once by hand against a public echo server and against a self-signed one with `verifySsl: false`.
- OAuth 2, OAuth 1, Digest, NTLM, WSSE and AWS SigV4 are refused for WebSocket connections with an explicit error.
- External secrets are strict on connect (see Behaviour decisions). Client certificates from the environment are not presented on the handshake.
- Runtime variables, scripts and `runtime.auth` of a WebSocket file are preserved on save but never executed.
- `request_count` and the contract audit ignore WebSocket items, and the collection runner never runs them.
- A WebSocket file with no `uid` gets `ws-<file name>` in memory until its next save; two such files with the same name in different folders collide on that derived id.
- There is no frame-idle timeout and no automatic reconnect.

## Next Plan

[2026-10-05-protocol-parity-plan-09-websocket-ui-and-import.md](2026-10-05-protocol-parity-plan-09-websocket-ui-and-import.md): Bruno WebSocket import, the typed frontend item, sidebar and create dialog, and the WebSocket tab UI. Chain to it automatically when this plan finishes.
