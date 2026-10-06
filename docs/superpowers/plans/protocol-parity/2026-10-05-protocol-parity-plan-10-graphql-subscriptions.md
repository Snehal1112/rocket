# Protocol parity, Plan 10: GraphQL subscriptions

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A GraphQL tab can run a `subscription` operation: Subscribe opens a WebSocket to the server, speaks `graphql-transport-ws` (or the legacy `graphql-ws` / subscriptions-transport-ws dialect when the server picks it), streams every result into a live log, and Stop ends the subscription cleanly.

**Architecture:** A `GraphQlSubscriptionService` in `rocket-app` is built on the `WebSocketClient` port from [Plan 08](2026-10-05-protocol-parity-plan-08-websocket-backend.md). It owns the protocol, not the socket: a pure `protocol` module builds and parses the messages of both dialects, and one task per subscription drives `connection_init`, `connection_ack`, `subscribe` or `start`, results, ping and pong, `complete` and close, then publishes `GraphQlSubscriptionMessage` and `GraphQlSubscriptionStatus` domain events (Tauri events `graphql:subscription-message` and `graphql:subscription-status`). A resolve step reuses Plan 08's `resolve_websocket` and `resolve_websocket_message`, so `{{variables}}`, collection headers and auth behave exactly as for a WebSocket handshake. On the frontend the existing `useWebSocketStore` and the `MessageLog` from [Plan 09](2026-10-05-protocol-parity-plan-09-websocket-ui-and-import.md) hold and render the stream; a small bridge converts the two subscription events into the store's inputs. `RequestPanel` (as changed by [Plan 06](2026-10-05-protocol-parity-plan-06-graphql-editor-and-execution.md)) swaps Send for Subscribe and Stop when the selected operation is a subscription.

**Tech Stack:** Rust (`tokio`, `serde_json`, `url`), the Plan 08 `WebSocketClient`, a local in-test `tokio-tungstenite` server, Tauri commands; React + TypeScript, Zustand, shadcn/ui, lucide-react, Vitest.

**Spec:** [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) section 2.4 `GraphQLRequest` (the request definition is unchanged). Protocol references: the `graphql-transport-ws` protocol (`connection_init`, `connection_ack`, `ping`, `pong`, `subscribe`, `next`, `error`, `complete`) and the legacy `subscriptions-transport-ws` protocol under the `graphql-ws` subprotocol (`connection_init`, `connection_ack`, `ka`, `start`, `data`, `error`, `complete`, `stop`, `connection_terminate`, `connection_error`). Bruno parity: https://docs.usebruno.com/send-requests/graphql/subscriptions and the WebSocket overview linked from Plan 08.

**Depends on:** [Plan 06](2026-10-05-protocol-parity-plan-06-graphql-editor-and-execution.md) (GraphQL tab, `RequestPanel` edits, `graphql_document::select_operation`, `listGraphQlOperations`, `GraphQlOperation`), [Plan 08](2026-10-05-protocol-parity-plan-08-websocket-backend.md) (`WebSocketClient`, `resolve_websocket`, `resolve_websocket_message`, `WebSocketSessionState`) and [Plan 09](2026-10-05-protocol-parity-plan-09-websocket-ui-and-import.md) (`useWebSocketStore`, `MessageLog`, `websocket-session.ts`). Plan 05 is needed only through Plan 06.

## Facts verified against the repo and the sibling plans (do not re-derive)

- `WebSocketClient::connect(WebSocketConnectRequest) -> WebSocketHandle { subprotocol, outbound: mpsc::Sender<WebSocketCommand>, events: mpsc::Receiver<WebSocketEvent> }`. `WebSocketEvent` is `Frame(WebSocketFrame)` zero or more times and then exactly one `Closed(WebSocketClose { code, reason, clean })`. `WebSocketConnectRequest.subprotocols` is the offer in preference order; the negotiated one comes back in `handle.subprotocol`. The client refuses any URL that does not start with `ws://` or `wss://`, closes the socket when every `outbound` sender is dropped, and applies a 3 second close-reply deadline after a `Close` command (Plan 08 Task 2).
- `RequestExecutionService::resolve_websocket(&WebSocketConnectInput) -> DomainResult<WebSocketConnectRequest>` resolves `{{variables}}`, merges collection headers and auth, and turns Basic, Bearer and API key auth into headers or a query parameter; other auth types are an explicit error. It does **not** check the URL scheme, so a scheme change can be applied to its output. `resolve_websocket_message(&WebSocketScope, WebSocketMessageKind, &str) -> DomainResult<WebSocketFrame>` returns `WebSocketFrame::Text` for text kinds and only fetches external secrets when the text contains `{{`. `WebSocketScope` and `WebSocketConnectInput` are `pub` with `pub` fields (`rocket_app::WebSocketConnectInput`, `rocket_app::WebSocketScope`).
- `crate::graphql_document::select_operation(document, requested, fallback_first) -> DomainResult<Option<String>>` (Plan 06): errors when the document has no operation, when a requested name does not exist, and when several operations exist and none was requested (`fallback_first == false`). `Ok(None)` means an anonymous operation.
- `DomainEvent` payload fields are snake_case on the wire (`session_id`); the variant tag is camelCase. `TauriEventBus::publish` has an exhaustive `match`, so new variants must be mapped (Plan 08 Task 3 did the same for `WebSocket*`).
- Plan 09's `useWebSocketStore` keys a session by tab id, ignores events for unknown session ids, caps the log at `MAX_LOG_ENTRIES`, and appends a system line for `open`, `closed` and `failed`. `MessageLogEntry` has an optional `label` (shown as a badge) for exactly this reuse. `websocket-session.ts` has a private `scopeFor(tab)` that reads the active environment and global environment.
- `RequestPanel.tsx` today builds a `responseArea` constant (`sending ? spinner : response ? <ResponseBodyViewer/> : empty state`), a Send `Button` in the URL bar whose `onClick` validates the URL and calls `send(request)` from `useExecuteRequest(tab.id)`, and `sending` from the same hook (`RequestPanel.tsx:142`, `:966-995`, `:1366-1385`). Plan 06 adds `const isGraphQl = request.requestType === 'graphql'` and `const profile = requestProfile(request.requestType)` near the top of the component. `useKeyboardShortcuts.ts:30-35` calls `sendRequest(tab.id, tab.request)` on Ctrl or Cmd+Enter for every request tab except WebSocket tabs (Plan 09).
- `pane-store.ts` ends a tab's backend sessions through `endSessionIfActive` and `endActiveSessions` (Plan 09 added `releaseWebSocketTab` to both).
- `rocket-app` has `tempfile`, `wiremock` and `rocket-infra` as dev-dependencies already; it has `tokio` (full), `url`, `serde_json` as normal dependencies. It does not have `tokio-tungstenite`, which Task 1 adds as a dev-dependency only.

## Behaviour decisions baked in

- **Dialect.** The offer is `["graphql-transport-ws", "graphql-ws"]`. The server's choice decides. If the server selects no subprotocol the transport dialect is assumed.
- **URL.** A subscription goes to the request URL with `http://` turned into `ws://` and `https://` into `wss://` (`ws` and `wss` pass through). An optional `subscriptionUrl` overrides the URL when a server exposes subscriptions elsewhere.
- **Operation.** One operation per connection, id `"1"`. `select_operation` picks it (with `fallback_first == false`), so a multi-operation document must name one.
- **Connection params.** An optional JSON object sent as the `connection_init` payload. Variables and params are resolved for `{{placeholders}}` first and must then be JSON objects.
- **Status meaning.** `connecting` while the socket opens, `open` once the subscribe message has been sent (the server acknowledged), `closed` for a clean end (stop, `complete`), `failed` for anything else (rejected handshake, no acknowledgement within 10 s, a server `error`, a lost connection).
- **Results.** Every `next` or `data` payload becomes one log line labelled `next`, pretty-printed JSON. `error` and `complete` are labelled too. Nothing is persisted and no History entry is written.
- **One subscription per tab.** The tab id is the key. Starting while one is running is ignored by the frontend.

## Global Constraints

- Rust: `cargo test -j4 -p <crate> <name>`, `cargo check -j4`. Never `cargo test --workspace`. No `unwrap()` or `expect()` in production paths; tests may use `.expect("reason")`. `DomainEvent` payload fields stay snake_case; IPC input DTOs are camelCase.
- Errors and logs never contain header values, tokens or full URLs (Plan 08's rule).
- Commits use the `dev-workflow-skills:1-git-commit` skill (never a freeform `git commit -m`), conventional subjects, staging by explicit path only. Never stage `crates/rocket-app/src/execution_service.rs` here.
- Frontend: shadcn/ui and `lucide-react` only, no raw `<button>`, `<input>`, `<select>`, `<dialog>`, `<form>` and no inline SVG in new code. Single-line variable-aware fields use `SingleLineEditor`. Zustand: narrow selectors, never fully destructure store state at the top of a component.
- Verification: `cargo check -j4` and the focused cargo test for Task 1; `yarn tsc --noEmit`, `yarn check` and `yarn test --run <pattern>` for Task 2.
- `RequestPanel.tsx` is also edited by Plan 06 and is very large. Re-read the current file before each edit below and anchor on the names given here (`responseArea`, `send`, `sending`, `isGraphQl`), not on line numbers.

## Review Focus

1. **The two dialects must not be mixed up.** A server that selects `graphql-ws` must receive `start` and `stop`, not `subscribe` and `complete`, and its `data` frames must read as results. Pinned by `legacy_dialect_is_used_when_the_server_selects_graphql_ws` and the protocol unit tests (Task 1).
2. **No subscribe before the acknowledgement, and no hang without one.** Pinned by `subscribe_is_only_sent_after_the_server_acknowledges` and `a_missing_acknowledgement_times_out_as_failed` (Task 1).
3. **Stop must be clean and must free the session.** The server must receive the stop message, the status must end `closed`, and the id must be reusable. Pinned by `stop_sends_complete_and_ends_the_session_cleanly` (Task 1).
4. **A server `error` or `connection_error` must end the stream as a failure the user can read**, not leave a live-looking tab. Pinned by `a_server_error_is_published_and_ends_the_subscription_as_failed` (Task 1).
5. **A GraphQL tab with a subscription operation must never fall through to an HTTP send, including from Ctrl or Cmd+Enter**, and closing the tab must end the stream. Pinned by `startSubscription registers the session before the call`, `releaseGraphQlSubscriptionTab` and the panel tests (Task 2).

---

## Task 1: Subscription protocol and service in `rocket-app`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/graphql_subscription.rs`, `crates/rocket-app/src/graphql_subscription/protocol.rs`, `src-tauri/src/commands/graphql_subscription.rs`
- Modify: `crates/rocket-shared/src/events.rs`, `crates/rocket-app/src/lib.rs`, `crates/rocket-app/Cargo.toml` (dev-dependencies), `crates/rocket-app/CLAUDE.md`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/tauri_event_bus.rs`

**Interfaces:**
- Consumes: `WebSocketClient`, `WebSocketConnectRequest`, `WebSocketHandle`, `WebSocketCommand`, `WebSocketEvent`, `WebSocketFrame` (Plan 08, `rocket_http::websocket`); `WebSocketConnectInput`, `WebSocketScope`, `RequestExecutionService::{resolve_websocket, resolve_websocket_message}`; `rocket_collection::WebSocketMessageKind`; `crate::graphql_document::select_operation` (Plan 06); `WebSocketSessionState`, `EventPublisher`, `DomainEvent`.
- Produces:
  - `rocket_shared::events::GraphQlSubscriptionEventKind { Next, Error, Complete }` (serde lowercase) and `DomainEvent::{GraphQlSubscriptionMessage { session_id, event, data, timestamp_ms }, GraphQlSubscriptionStatus { session_id, state, dialect, reason }}`.
  - `graphql_subscription::protocol::{Dialect, Incoming, OPERATION_ID, SUBPROTOCOL_TRANSPORT, SUBPROTOCOL_LEGACY, init_message, start_message, stop_messages, pong_message, interpret}`.
  - `rocket_app::{GraphQlSubscribeInput, GraphQlSubscriptionStart, GraphQlSubscriptionService, resolve_graphql_subscription, to_websocket_url}`; `GraphQlSubscriptionService::{new(client, events), with_ack_timeout(client, events, Duration), async start(&self, session_id, GraphQlSubscriptionStart) -> DomainResult<()>, async stop(&self, session_id) -> DomainResult<()>, async end_all(&self), session_count(&self) -> usize}`.
  - Tauri commands `graphql_subscribe(sessionId, input)` and `graphql_unsubscribe(sessionId)`; Tauri events `graphql:subscription-message` and `graphql:subscription-status`.

Wire shape of the events:

```json
{ "type": "graphQlSubscriptionMessage", "session_id": "…", "event": "next", "data": "{\n  \"data\": { … }\n}", "timestamp_ms": 1759650000000 }
{ "type": "graphQlSubscriptionStatus", "session_id": "…", "state": "open", "dialect": "graphql-transport-ws", "reason": null }
```

`event` is `next | error | complete`. `data` is pretty-printed JSON (empty for `complete`). `state` is the Plan 08 `connecting | open | closed | failed`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

### Step group A: protocol (pure)

- [ ] **Step 2: Write the failing protocol tests**

Create `crates/rocket-app/src/graphql_subscription/protocol.rs` containing only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(text: &str) -> serde_json::Value {
        serde_json::from_str(text).expect("valid json")
    }

    #[test]
    fn the_dialect_follows_the_negotiated_subprotocol() {
        assert_eq!(Dialect::from_subprotocol(Some("graphql-ws")), Dialect::Legacy);
        assert_eq!(Dialect::from_subprotocol(Some("graphql-transport-ws")), Dialect::Transport);
        // No subprotocol selected: assume the modern protocol.
        assert_eq!(Dialect::from_subprotocol(None), Dialect::Transport);
        assert_eq!(Dialect::Legacy.subprotocol(), "graphql-ws");
        assert_eq!(Dialect::Transport.subprotocol(), "graphql-transport-ws");
    }

    #[test]
    fn connection_init_carries_the_params_only_when_given() {
        assert_eq!(parse(&init_message(None)), json!({ "type": "connection_init" }));
        let params = json!({ "token": "abc" });
        assert_eq!(
            parse(&init_message(Some(&params))),
            json!({ "type": "connection_init", "payload": { "token": "abc" } })
        );
    }

    #[test]
    fn the_transport_dialect_subscribes() {
        let vars = json!({ "n": 3 });
        let msg = parse(&start_message(Dialect::Transport, "subscription S { s }", Some(&vars), Some("S")));
        assert_eq!(msg["type"], "subscribe");
        assert_eq!(msg["id"], OPERATION_ID);
        assert_eq!(msg["payload"]["query"], "subscription S { s }");
        assert_eq!(msg["payload"]["variables"], vars);
        assert_eq!(msg["payload"]["operationName"], "S");
    }

    #[test]
    fn the_legacy_dialect_starts_and_omits_empty_optionals() {
        let msg = parse(&start_message(Dialect::Legacy, "subscription { s }", None, None));
        assert_eq!(msg["type"], "start");
        assert_eq!(msg["id"], OPERATION_ID);
        assert!(msg["payload"].get("variables").is_none());
        assert!(msg["payload"].get("operationName").is_none());
    }

    #[test]
    fn stopping_differs_per_dialect() {
        let transport: Vec<_> = stop_messages(Dialect::Transport).iter().map(|m| parse(m)).collect();
        assert_eq!(transport, vec![json!({ "id": OPERATION_ID, "type": "complete" })]);

        let legacy: Vec<_> = stop_messages(Dialect::Legacy).iter().map(|m| parse(m)).collect();
        assert_eq!(
            legacy,
            vec![json!({ "id": OPERATION_ID, "type": "stop" }), json!({ "type": "connection_terminate" })]
        );
        assert_eq!(parse(&pong_message()), json!({ "type": "pong" }));
    }

    #[test]
    fn transport_frames_are_interpreted() {
        let d = Dialect::Transport;
        assert_eq!(interpret(d, r#"{"type":"connection_ack"}"#), Incoming::Ack);
        assert_eq!(interpret(d, r#"{"type":"ping"}"#), Incoming::Ping);
        assert_eq!(interpret(d, r#"{"type":"pong"}"#), Incoming::Pong);
        assert_eq!(
            interpret(d, r#"{"id":"1","type":"next","payload":{"data":{"n":1}}}"#),
            Incoming::Result(json!({ "data": { "n": 1 } }))
        );
        assert_eq!(
            interpret(d, r#"{"id":"1","type":"error","payload":[{"message":"boom"}]}"#),
            Incoming::Errors(json!([{ "message": "boom" }]))
        );
        assert_eq!(interpret(d, r#"{"id":"1","type":"complete"}"#), Incoming::Complete);
    }

    #[test]
    fn legacy_frames_are_interpreted() {
        let d = Dialect::Legacy;
        assert_eq!(interpret(d, r#"{"type":"connection_ack"}"#), Incoming::Ack);
        assert_eq!(interpret(d, r#"{"type":"ka"}"#), Incoming::KeepAlive);
        assert_eq!(
            interpret(d, r#"{"id":"1","type":"data","payload":{"data":{"n":2}}}"#),
            Incoming::Result(json!({ "data": { "n": 2 } }))
        );
        assert_eq!(
            interpret(d, r#"{"id":"1","type":"error","payload":{"message":"bad"}}"#),
            Incoming::Errors(json!({ "message": "bad" }))
        );
        assert_eq!(
            interpret(d, r#"{"type":"connection_error","payload":{"message":"no"}}"#),
            Incoming::ConnectionError(json!({ "message": "no" }))
        );
        assert_eq!(interpret(d, r#"{"id":"1","type":"complete"}"#), Incoming::Complete);
    }

    #[test]
    fn a_dialects_own_frame_names_do_not_leak_into_the_other() {
        // `data` and `ka` mean nothing in the transport dialect, and `next` and `ping` nothing in the legacy one.
        assert_eq!(interpret(Dialect::Transport, r#"{"id":"1","type":"data","payload":{}}"#), Incoming::Ignore);
        assert_eq!(interpret(Dialect::Transport, r#"{"type":"ka"}"#), Incoming::Ignore);
        assert_eq!(interpret(Dialect::Legacy, r#"{"id":"1","type":"next","payload":{}}"#), Incoming::Ignore);
        assert_eq!(interpret(Dialect::Legacy, r#"{"type":"ping"}"#), Incoming::Ignore);
    }

    #[test]
    fn frames_for_another_operation_and_garbage_are_ignored() {
        assert_eq!(
            interpret(Dialect::Transport, r#"{"id":"7","type":"next","payload":{}}"#),
            Incoming::Ignore
        );
        assert_eq!(interpret(Dialect::Transport, "not json"), Incoming::Ignore);
        assert_eq!(interpret(Dialect::Transport, r#"{"payload":1}"#), Incoming::Ignore);
        assert_eq!(interpret(Dialect::Transport, r#"{"type":"surprise"}"#), Incoming::Ignore);
    }
}
```

- [ ] **Step 3: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-app graphql_subscription::protocol`
Expected: compile errors (module not declared, items not found).

- [ ] **Step 4: Implement the protocol module**

Prepend to `crates/rocket-app/src/graphql_subscription/protocol.rs`:

```rust
//! Message formats of the two GraphQL-over-WebSocket protocols. Pure: no I/O, no state.
//!
//! - `graphql-transport-ws`: `connection_init`, `connection_ack`, `ping`, `pong`, `subscribe`,
//!   `next`, `error` (an array of errors), `complete`.
//! - legacy `graphql-ws` (subscriptions-transport-ws): `connection_init`, `connection_ack`,
//!   `ka`, `start`, `data`, `error` (one error), `complete`, `stop`, `connection_terminate`,
//!   `connection_error`.

use serde_json::{json, Value};

pub const SUBPROTOCOL_TRANSPORT: &str = "graphql-transport-ws";
pub const SUBPROTOCOL_LEGACY: &str = "graphql-ws";

/// One subscription per connection, so one fixed operation id.
pub const OPERATION_ID: &str = "1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Transport,
    Legacy,
}

impl Dialect {
    /// The server's choice decides. A server that selected nothing is assumed to speak the
    /// modern protocol.
    pub fn from_subprotocol(negotiated: Option<&str>) -> Self {
        match negotiated {
            Some(SUBPROTOCOL_LEGACY) => Self::Legacy,
            _ => Self::Transport,
        }
    }

    pub fn subprotocol(self) -> &'static str {
        match self {
            Self::Transport => SUBPROTOCOL_TRANSPORT,
            Self::Legacy => SUBPROTOCOL_LEGACY,
        }
    }
}

/// A server frame, reduced to what the session cares about.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Ack,
    Ping,
    Pong,
    KeepAlive,
    /// A result payload (`next` or `data`): the full GraphQL response object.
    Result(Value),
    /// An operation error (`error`), still in the dialect's own shape.
    Errors(Value),
    Complete,
    ConnectionError(Value),
    /// Anything else, including frames for another operation and invalid JSON.
    Ignore,
}

pub fn init_message(connection_params: Option<&Value>) -> String {
    match connection_params {
        Some(params) => json!({ "type": "connection_init", "payload": params }),
        None => json!({ "type": "connection_init" }),
    }
    .to_string()
}

pub fn start_message(
    dialect: Dialect,
    query: &str,
    variables: Option<&Value>,
    operation_name: Option<&str>,
) -> String {
    let mut payload = json!({ "query": query });
    if let Some(variables) = variables {
        payload["variables"] = variables.clone();
    }
    if let Some(name) = operation_name {
        payload["operationName"] = json!(name);
    }
    let kind = match dialect {
        Dialect::Transport => "subscribe",
        Dialect::Legacy => "start",
    };
    json!({ "id": OPERATION_ID, "type": kind, "payload": payload }).to_string()
}

/// What to send to end the subscription before closing the socket.
pub fn stop_messages(dialect: Dialect) -> Vec<String> {
    match dialect {
        Dialect::Transport => vec![json!({ "id": OPERATION_ID, "type": "complete" }).to_string()],
        Dialect::Legacy => vec![
            json!({ "id": OPERATION_ID, "type": "stop" }).to_string(),
            json!({ "type": "connection_terminate" }).to_string(),
        ],
    }
}

pub fn pong_message() -> String {
    json!({ "type": "pong" }).to_string()
}

pub fn interpret(dialect: Dialect, text: &str) -> Incoming {
    let Ok(frame) = serde_json::from_str::<Value>(text) else {
        return Incoming::Ignore;
    };
    let payload = frame.get("payload").cloned().unwrap_or(Value::Null);
    let kind = frame.get("type").and_then(Value::as_str);

    // Operation frames for some other id are not ours.
    let for_another_operation = frame
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| id != OPERATION_ID);

    match (dialect, kind) {
        (_, Some("connection_ack")) => Incoming::Ack,
        (Dialect::Transport, Some("ping")) => Incoming::Ping,
        (Dialect::Transport, Some("pong")) => Incoming::Pong,
        (Dialect::Legacy, Some("ka")) => Incoming::KeepAlive,
        (Dialect::Legacy, Some("connection_error")) => Incoming::ConnectionError(payload),
        (_, _) if for_another_operation => Incoming::Ignore,
        (Dialect::Transport, Some("next")) | (Dialect::Legacy, Some("data")) => {
            Incoming::Result(payload)
        }
        (_, Some("error")) => Incoming::Errors(payload),
        (_, Some("complete")) => Incoming::Complete,
        _ => Incoming::Ignore,
    }
}
```

- [ ] **Step 5: Declare the module and run the protocol tests**

Create `crates/rocket-app/src/graphql_subscription.rs` containing only `pub mod protocol;` for now, and in `crates/rocket-app/src/lib.rs` add `pub mod graphql_subscription;` (after `pub mod git_service;`).

Run: `cargo test -j4 -p rocket-app graphql_subscription::protocol`
Expected: 8 passed.

### Step group B: events

- [ ] **Step 6: Write the failing event test**

In `crates/rocket-shared/src/events.rs` tests module (next to Plan 08's `websocket_events_serialize_with_snake_case_fields_and_lowercase_enums`), add:

```rust
    #[test]
    fn graphql_subscription_events_serialize_like_the_websocket_ones() {
        let message = DomainEvent::GraphQlSubscriptionMessage {
            session_id: "s1".into(),
            event: GraphQlSubscriptionEventKind::Next,
            data: "{}".into(),
            timestamp_ms: 7,
        };
        let json = serde_json::to_value(&message).expect("serialize");
        assert_eq!(json["type"], "graphQlSubscriptionMessage");
        assert_eq!(json["session_id"], "s1");
        assert_eq!(json["event"], "next");
        assert_eq!(json["timestamp_ms"], 7);

        let status = DomainEvent::GraphQlSubscriptionStatus {
            session_id: "s1".into(),
            state: WebSocketSessionState::Open,
            dialect: Some("graphql-transport-ws".into()),
            reason: None,
        };
        let json = serde_json::to_value(&status).expect("serialize");
        assert_eq!(json["type"], "graphQlSubscriptionStatus");
        assert_eq!(json["state"], "open");
        assert_eq!(json["dialect"], "graphql-transport-ws");
        assert!(json["reason"].is_null());
    }
```

- [ ] **Step 7: Run it and confirm it fails**

Run: `cargo test -j4 -p rocket-shared graphql_subscription_events`
Expected: compile error, variants not found.

- [ ] **Step 8: Implement the events and map them**

In `crates/rocket-shared/src/events.rs`, next to Plan 08's WebSocket enums add:

```rust
/// What a GraphQL subscription result line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphQlSubscriptionEventKind {
    Next,
    Error,
    Complete,
}
```

and in `DomainEvent`, after Plan 08's `WebSocketStatus`:

```rust
    // GraphQL subscription events
    /// One result, error or completion of a GraphQL subscription.
    GraphQlSubscriptionMessage {
        session_id: String,
        event: GraphQlSubscriptionEventKind,
        /// Pretty-printed JSON. Empty for `complete`.
        data: String,
        timestamp_ms: i64,
    },
    /// A subscription changed state. `dialect` is the subprotocol the server selected.
    GraphQlSubscriptionStatus {
        session_id: String,
        state: WebSocketSessionState,
        dialect: Option<String>,
        reason: Option<String>,
    },
```

In `src-tauri/src/tauri_event_bus.rs`, after the WebSocket arms:

```rust
            // GraphQL subscriptions: one channel for results, one for lifecycle.
            DomainEvent::GraphQlSubscriptionMessage { .. } => "graphql:subscription-message",
            DomainEvent::GraphQlSubscriptionStatus { .. } => "graphql:subscription-status",
```

Run: `cargo test -j4 -p rocket-shared graphql_subscription_events`
Expected: passed.

### Step group C: request resolution

- [ ] **Step 9: Add the dev-dependencies and write the failing resolution tests**

In `crates/rocket-app/Cargo.toml` `[dev-dependencies]` add (the server side of the end-to-end tests only; the production crate gets no new dependency):

```toml
tokio-tungstenite = { version = "0.30", default-features = false, features = ["handshake"] }
futures-util = { version = "0.3", default-features = false, features = ["sink", "std"] }
```

Append to `crates/rocket-app/src/graphql_subscription.rs` (below `pub mod protocol;`) this test module:

```rust
#[cfg(test)]
mod resolve_tests {
    use super::*;
    use crate::execution_service::RequestExecutionService;
    use crate::test_doubles::{
        EmptySecretManagerRepo, InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo,
        RecordingExecutor, SharedCollectionRepo, SharedHistoryRepo, StaticEnvRepo,
    };
    use rocket_collection::Collection;
    use rocket_environment::{Environment, Variable};
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Arc;

    fn service() -> RequestExecutionService {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("host", "api.example.com"));
        env.set_variable(Variable::new("token", "abc123"));
        env.set_variable(Variable::new("room", "general"));
        RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            RecordingExecutor::new(),
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(Collection::new("api")))),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
    }

    fn input(extra: serde_json::Value) -> GraphQlSubscribeInput {
        let mut base = serde_json::json!({
            "url": "https://{{host}}/graphql",
            "query": "subscription OnMsg($room: String) { msg(room: $room) { text } }",
            "collection": "api",
            "environmentName": "dev"
        });
        if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
            for (k, v) in extra {
                base.insert(k.clone(), v.clone());
            }
        }
        serde_json::from_value(base).expect("input")
    }

    #[test]
    fn http_schemes_become_websocket_schemes() {
        assert_eq!(to_websocket_url("http://h/graphql").expect("http"), "ws://h/graphql");
        assert_eq!(to_websocket_url("https://h/graphql?a=1").expect("https"), "wss://h/graphql?a=1");
        assert_eq!(to_websocket_url("wss://h/graphql").expect("wss"), "wss://h/graphql");
        assert_eq!(to_websocket_url("ws://h/graphql").expect("ws"), "ws://h/graphql");
        let err = to_websocket_url("ftp://h/graphql").expect_err("unsupported scheme");
        assert!(err.to_string().contains("http"), "{err}");
    }

    #[tokio::test]
    async fn the_url_variables_and_params_are_resolved_and_the_scheme_is_converted() {
        let start = resolve_graphql_subscription(
            &service(),
            input(serde_json::json!({
                "variables": "{\"room\": \"{{room}}\"}",
                "connectionParams": "{\"token\": \"{{token}}\"}",
                "headers": [{ "key": "X-Token", "value": "{{token}}", "enabled": true }]
            })),
        )
        .await
        .expect("resolve");

        assert_eq!(start.connect.url, "wss://api.example.com/graphql");
        assert_eq!(start.variables, Some(serde_json::json!({ "room": "general" })));
        assert_eq!(start.connection_params, Some(serde_json::json!({ "token": "abc123" })));
        assert_eq!(start.operation_name.as_deref(), Some("OnMsg"));
        assert!(start.connect.headers.iter().any(|(k, v)| k == "X-Token" && v == "abc123"));
        assert_eq!(
            start.connect.subprotocols,
            vec!["graphql-transport-ws".to_string(), "graphql-ws".to_string()]
        );
    }

    #[tokio::test]
    async fn a_subscription_url_overrides_the_request_url() {
        let start = resolve_graphql_subscription(
            &service(),
            input(serde_json::json!({ "subscriptionUrl": "wss://{{host}}/subscriptions" })),
        )
        .await
        .expect("resolve");
        assert_eq!(start.connect.url, "wss://api.example.com/subscriptions");
    }

    #[tokio::test]
    async fn blank_variables_and_params_mean_none() {
        let start = resolve_graphql_subscription(&service(), input(serde_json::json!({ "variables": "  " })))
            .await
            .expect("resolve");
        assert_eq!(start.variables, None);
        assert_eq!(start.connection_params, None);
    }

    #[tokio::test]
    async fn variables_and_params_must_be_json_objects_after_resolution() {
        let svc = service();
        let err = resolve_graphql_subscription(&svc, input(serde_json::json!({ "variables": "[1]" })))
            .await
            .expect_err("array variables");
        assert!(err.to_string().contains("variables"), "{err}");

        let err = resolve_graphql_subscription(&svc, input(serde_json::json!({ "connectionParams": "{oops" })))
            .await
            .expect_err("invalid params");
        assert!(err.to_string().contains("connection parameters"), "{err}");
    }

    #[tokio::test]
    async fn a_document_with_several_operations_needs_a_chosen_one() {
        let svc = service();
        let two = "subscription A { a } subscription B { b }";

        let err = resolve_graphql_subscription(&svc, input(serde_json::json!({ "query": two })))
            .await
            .expect_err("ambiguous");
        assert!(err.to_string().contains("choose"), "{err}");

        let start = resolve_graphql_subscription(
            &svc,
            input(serde_json::json!({ "query": two, "operationName": "B" })),
        )
        .await
        .expect("named");
        assert_eq!(start.operation_name.as_deref(), Some("B"));
    }

    #[test]
    fn the_ipc_input_deserializes_from_camel_case() {
        let i: GraphQlSubscribeInput = serde_json::from_value(serde_json::json!({
            "url": "https://h/graphql",
            "subscriptionUrl": "wss://h/ws",
            "query": "subscription { a }",
            "variables": "{}",
            "operationName": "A",
            "connectionParams": "{}",
            "headers": [],
            "verifySsl": false,
            "timeoutMs": 2000,
            "collection": "api",
            "environmentName": "dev",
            "globalEnvName": "g",
            "requestPath": "q.yml"
        }))
        .expect("deserialize");
        assert_eq!(i.subscription_url.as_deref(), Some("wss://h/ws"));
        assert_eq!(i.verify_ssl, Some(false));
        assert_eq!(i.scope.request_path.as_deref(), Some("q.yml"));
    }
}
```

- [ ] **Step 10: Run them and confirm they fail**

Run: `cargo test -j4 -p rocket-app graphql_subscription::resolve_tests`
Expected: compile errors (`GraphQlSubscribeInput`, `resolve_graphql_subscription`, `to_websocket_url` not found).

- [ ] **Step 11: Implement the input, the scheme conversion and the resolution**

Replace the first line of `crates/rocket-app/src/graphql_subscription.rs` (`pub mod protocol;`) with the following, keeping both test modules at the bottom:

```rust
//! GraphQL subscriptions over WebSocket. The socket comes from the `WebSocketClient` port; this
//! module owns the protocol and the session. See `protocol` for the message formats.

pub mod protocol;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rocket_collection::WebSocketMessageKind;
use rocket_http::websocket::{
    WebSocketClient, WebSocketCommand, WebSocketConnectRequest, WebSocketEvent, WebSocketFrame,
    WebSocketHandle,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{
    DomainEvent, EventPublisher, GraphQlSubscriptionEventKind, WebSocketSessionState,
};
use rocket_shared::types::{Auth, Header};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::oneshot;

use crate::execution_service::websocket_resolution::{WebSocketConnectInput, WebSocketScope};
use crate::execution_service::RequestExecutionService;
use crate::graphql_document::select_operation;
use protocol::{
    init_message, interpret, pong_message, start_message, stop_messages, Dialect, Incoming,
    SUBPROTOCOL_LEGACY, SUBPROTOCOL_TRANSPORT,
};

/// How long to wait for `connection_ack` before giving up.
const DEFAULT_ACK_TIMEOUT: Duration = Duration::from_secs(10);

/// IPC input of `graphql_subscribe`. Built from the open GraphQL tab.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlSubscribeInput {
    /// The request URL. `http` and `https` are turned into `ws` and `wss`.
    pub url: String,
    /// Where subscriptions are served when that differs from `url`.
    #[serde(default)]
    pub subscription_url: Option<String>,
    pub query: String,
    /// JSON object text. Blank means none.
    #[serde(default)]
    pub variables: Option<String>,
    #[serde(default)]
    pub operation_name: Option<String>,
    /// JSON object text sent as the `connection_init` payload. Blank means none.
    #[serde(default)]
    pub connection_params: Option<String>,
    #[serde(default)]
    pub headers: Vec<Header>,
    #[serde(default)]
    pub auth: Option<Auth>,
    #[serde(default)]
    pub verify_ssl: Option<bool>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(flatten)]
    pub scope: WebSocketScope,
}

/// A subscription ready to start: the handshake, and the operation to run on it.
#[derive(Debug, Clone)]
pub struct GraphQlSubscriptionStart {
    pub connect: WebSocketConnectRequest,
    pub query: String,
    pub variables: Option<Value>,
    pub operation_name: Option<String>,
    pub connection_params: Option<Value>,
}

/// `http` becomes `ws`, `https` becomes `wss`, `ws` and `wss` pass through.
pub fn to_websocket_url(url: &str) -> DomainResult<String> {
    if let Some(rest) = url.strip_prefix("https://") {
        Ok(format!("wss://{rest}"))
    } else if let Some(rest) = url.strip_prefix("http://") {
        Ok(format!("ws://{rest}"))
    } else if url.starts_with("wss://") || url.starts_with("ws://") {
        Ok(url.to_string())
    } else {
        Err(DomainError::InvalidInput(
            "a subscription URL must start with http://, https://, ws:// or wss://".into(),
        ))
    }
}

async fn resolved_text(
    exec: &RequestExecutionService,
    scope: &WebSocketScope,
    text: &str,
) -> DomainResult<String> {
    match exec
        .resolve_websocket_message(scope, WebSocketMessageKind::Text, text)
        .await?
    {
        WebSocketFrame::Text(resolved) => Ok(resolved),
        WebSocketFrame::Binary(_) => Err(DomainError::Internal(
            "a text message resolved to a binary frame".into(),
        )),
    }
}

/// Resolves optional JSON object text: blank is `None`, otherwise `{{placeholders}}` are filled
/// in and the result must parse to a JSON object.
async fn resolved_object(
    exec: &RequestExecutionService,
    scope: &WebSocketScope,
    text: Option<&str>,
    what: &str,
) -> DomainResult<Option<Value>> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    let resolved = resolved_text(exec, scope, text).await?;
    match serde_json::from_str::<Value>(&resolved) {
        Ok(value @ Value::Object(_)) => Ok(Some(value)),
        Ok(_) => Err(DomainError::InvalidInput(format!(
            "{what} must be a JSON object"
        ))),
        Err(e) => Err(DomainError::InvalidInput(format!(
            "{what} are not valid JSON: {e}"
        ))),
    }
}

/// Turns the tab's input into a startable subscription: the handshake goes through Plan 08's
/// `resolve_websocket` (variables, collection headers, auth), the texts through
/// `resolve_websocket_message`, and the operation through `select_operation`.
pub async fn resolve_graphql_subscription(
    exec: &RequestExecutionService,
    input: GraphQlSubscribeInput,
) -> DomainResult<GraphQlSubscriptionStart> {
    let operation_name = select_operation(&input.query, input.operation_name.as_deref(), false)?;

    let connect_input = WebSocketConnectInput {
        url: input
            .subscription_url
            .clone()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| input.url.clone()),
        headers: input.headers.clone(),
        auth: input.auth.clone(),
        subprotocols: vec![SUBPROTOCOL_TRANSPORT.to_string(), SUBPROTOCOL_LEGACY.to_string()],
        timeout_ms: input.timeout_ms,
        keep_alive_ms: None,
        verify_ssl: input.verify_ssl,
        scope: input.scope.clone(),
    };
    let mut connect = exec.resolve_websocket(&connect_input).await?;
    connect.url = to_websocket_url(&connect.url)?;

    let query = resolved_text(exec, &input.scope, &input.query).await?;
    let variables = resolved_object(exec, &input.scope, input.variables.as_deref(), "variables").await?;
    let connection_params = resolved_object(
        exec,
        &input.scope,
        input.connection_params.as_deref(),
        "connection parameters",
    )
    .await?;

    Ok(GraphQlSubscriptionStart {
        connect,
        query,
        variables,
        operation_name,
        connection_params,
    })
}
```

(The `use` of items not yet used by the resolution code, such as `HashMap`, `Mutex`, `oneshot`, `WebSocketCommand` and the event types, is satisfied by Step 14. If the compiler warns about unused imports at this step, ignore the warnings until then.)

- [ ] **Step 12: Run the resolution tests**

Run: `cargo test -j4 -p rocket-app graphql_subscription::resolve_tests`
Expected: 7 passed. The test `variables_and_params_must_be_json_objects_after_resolution` asserts on the words "variables" and "connection parameters", which come from `resolved_object`'s `what` argument.

### Step group D: the session service

- [ ] **Step 13: Write the failing service tests**

Append to `crates/rocket-app/src/graphql_subscription.rs`:

```rust
#[cfg(test)]
mod session_tests {
    use super::*;
    use crate::test_doubles::RecordingPublisher;
    use futures_util::{SinkExt, StreamExt};
    use rocket_infra::TungsteniteWebSocketClient;
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    use tokio_tungstenite::tungstenite::Message;

    /// What the scripted server does after it has acknowledged the connection and received the
    /// subscribe (or start) message.
    #[derive(Clone, Copy)]
    enum Script {
        /// Send two results and then `complete`.
        Finite,
        /// Send one result and then wait for the client to stop.
        Hold,
        /// Send an operation error.
        ErrorFrame,
        /// Never acknowledge the connection.
        NoAck,
        /// Acknowledge, ping, send one result, then complete.
        PingFirst,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Offer {
        /// Select `graphql-transport-ws`.
        Modern,
        /// Select `graphql-ws`.
        Legacy,
    }

    async fn spawn_server(script: Script, offer: Offer) -> (u16, Arc<Mutex<Vec<serde_json::Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let received: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
        let log = Arc::clone(&received);
        tokio::spawn(async move {
            let Ok((tcp, _)) = listener.accept().await else { return };
            let chosen = if offer == Offer::Legacy { "graphql-ws" } else { "graphql-transport-ws" };
            let callback = move |_: &Request, mut resp: Response| -> Result<Response, ErrorResponse> {
                if let Ok(value) = chosen.parse() {
                    resp.headers_mut().insert("sec-websocket-protocol", value);
                }
                Ok(resp)
            };
            let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else { return };
            let result_kind = if offer == Offer::Legacy { "data" } else { "next" };
            let result = |n: u32| {
                json!({ "id": "1", "type": result_kind, "payload": { "data": { "n": n } } }).to_string()
            };
            while let Some(Ok(msg)) = ws.next().await {
                let Message::Text(text) = msg else { continue };
                let frame: serde_json::Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                log.lock().expect("lock").push(frame.clone());
                match frame["type"].as_str() {
                    Some("connection_init") => {
                        if !matches!(script, Script::NoAck) {
                            let _ = ws.send(Message::text(json!({ "type": "connection_ack" }).to_string())).await;
                        }
                    }
                    Some("subscribe") | Some("start") => match script {
                        Script::Finite => {
                            let _ = ws.send(Message::text(result(1))).await;
                            let _ = ws.send(Message::text(result(2))).await;
                            let _ = ws.send(Message::text(json!({ "id": "1", "type": "complete" }).to_string())).await;
                        }
                        Script::Hold => {
                            let _ = ws.send(Message::text(result(1))).await;
                        }
                        Script::ErrorFrame => {
                            let payload = if offer == Offer::Legacy {
                                json!({ "message": "boom" })
                            } else {
                                json!([{ "message": "boom" }])
                            };
                            let _ = ws.send(Message::text(json!({ "id": "1", "type": "error", "payload": payload }).to_string())).await;
                        }
                        Script::PingFirst => {
                            let _ = ws.send(Message::text(json!({ "type": "ping" }).to_string())).await;
                            let _ = ws.send(Message::text(result(1))).await;
                            let _ = ws.send(Message::text(json!({ "id": "1", "type": "complete" }).to_string())).await;
                        }
                        Script::NoAck => {}
                    },
                    _ => {}
                }
            }
        });
        (port, received)
    }

    fn start_for(port: u16) -> GraphQlSubscriptionStart {
        GraphQlSubscriptionStart {
            connect: WebSocketConnectRequest {
                url: format!("ws://127.0.0.1:{port}/graphql"),
                headers: Vec::new(),
                subprotocols: vec![SUBPROTOCOL_TRANSPORT.to_string(), SUBPROTOCOL_LEGACY.to_string()],
                connect_timeout: Some(Duration::from_secs(5)),
                keep_alive_interval: None,
                verify_ssl: true,
            },
            query: "subscription OnN { n }".into(),
            variables: Some(json!({ "room": "general" })),
            operation_name: Some("OnN".into()),
            connection_params: Some(json!({ "token": "abc" })),
        }
    }

    fn service() -> (GraphQlSubscriptionService, Arc<RecordingPublisher>) {
        let publisher = RecordingPublisher::new();
        let svc = GraphQlSubscriptionService::with_ack_timeout(
            Arc::new(TungsteniteWebSocketClient::new()),
            publisher.clone(),
            Duration::from_millis(300),
        );
        (svc, publisher)
    }

    async fn wait_for(publisher: &RecordingPublisher, what: &str, pred: impl Fn(&[DomainEvent]) -> bool) {
        for _ in 0..600 {
            if pred(&publisher.events()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {what}; events: {:?}", publisher.events());
    }

    fn states(events: &[DomainEvent]) -> Vec<WebSocketSessionState> {
        events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::GraphQlSubscriptionStatus { state, .. } => Some(*state),
                _ => None,
            })
            .collect()
    }

    fn results(events: &[DomainEvent]) -> Vec<(GraphQlSubscriptionEventKind, String)> {
        events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::GraphQlSubscriptionMessage { event, data, .. } => Some((*event, data.clone())),
                _ => None,
            })
            .collect()
    }

    fn terminal(events: &[DomainEvent]) -> bool {
        states(events)
            .iter()
            .any(|s| matches!(s, WebSocketSessionState::Closed | WebSocketSessionState::Failed))
    }

    fn types(received: &Arc<Mutex<Vec<serde_json::Value>>>) -> Vec<String> {
        received
            .lock()
            .expect("lock")
            .iter()
            .filter_map(|f| f["type"].as_str().map(str::to_string))
            .collect()
    }

    #[tokio::test]
    async fn a_modern_subscription_streams_results_then_completes() {
        let (port, received) = spawn_server(Script::Finite, Offer::Modern).await;
        let (svc, publisher) = service();

        svc.start("s1", start_for(port)).await.expect("start");
        wait_for(&publisher, "terminal status", terminal).await;

        let events = publisher.events();
        assert_eq!(
            states(&events),
            vec![WebSocketSessionState::Connecting, WebSocketSessionState::Open, WebSocketSessionState::Closed]
        );
        let seen = results(&events);
        assert_eq!(seen.len(), 3, "{seen:?}");
        assert_eq!(seen[0].0, GraphQlSubscriptionEventKind::Next);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&seen[0].1).expect("json"), json!({ "data": { "n": 1 } }));
        assert_eq!(seen[1].0, GraphQlSubscriptionEventKind::Next);
        assert_eq!(seen[2], (GraphQlSubscriptionEventKind::Complete, String::new()));

        // The server saw the init (with its params) and then the subscribe (with the operation).
        let frames = received.lock().expect("lock").clone();
        assert_eq!(frames[0], json!({ "type": "connection_init", "payload": { "token": "abc" } }));
        assert_eq!(frames[1]["type"], "subscribe");
        assert_eq!(frames[1]["payload"]["operationName"], "OnN");
        assert_eq!(frames[1]["payload"]["variables"], json!({ "room": "general" }));
        assert_eq!(svc.session_count(), 0);
    }

    #[tokio::test]
    async fn legacy_dialect_is_used_when_the_server_selects_graphql_ws() {
        let (port, received) = spawn_server(Script::Finite, Offer::Legacy).await;
        let (svc, publisher) = service();

        svc.start("s1", start_for(port)).await.expect("start");
        wait_for(&publisher, "terminal status", terminal).await;

        assert_eq!(types(&received).first().map(String::as_str), Some("connection_init"));
        assert!(types(&received).contains(&"start".to_string()), "{:?}", types(&received));
        assert!(!types(&received).contains(&"subscribe".to_string()));
        let next_results = results(&publisher.events())
            .iter()
            .filter(|(kind, _)| *kind == GraphQlSubscriptionEventKind::Next)
            .count();
        assert_eq!(next_results, 2, "legacy `data` frames are results");
        let open_dialect = publisher.events().iter().find_map(|e| match e {
            DomainEvent::GraphQlSubscriptionStatus { state: WebSocketSessionState::Open, dialect, .. } => dialect.clone(),
            _ => None,
        });
        assert_eq!(open_dialect.as_deref(), Some("graphql-ws"));
    }

    #[tokio::test]
    async fn subscribe_is_only_sent_after_the_server_acknowledges() {
        let (port, received) = spawn_server(Script::Finite, Offer::Modern).await;
        let (svc, publisher) = service();

        svc.start("s1", start_for(port)).await.expect("start");
        wait_for(&publisher, "terminal status", terminal).await;

        // The init came first, and Open was published only after the ack led to the subscribe.
        let order = types(&received);
        let init = order.iter().position(|t| t == "connection_init").expect("init");
        let subscribe = order.iter().position(|t| t == "subscribe").expect("subscribe");
        assert!(init < subscribe, "{order:?}");
    }

    #[tokio::test]
    async fn a_missing_acknowledgement_times_out_as_failed() {
        let (port, received) = spawn_server(Script::NoAck, Offer::Modern).await;
        let (svc, publisher) = service();

        svc.start("s1", start_for(port)).await.expect("start");
        wait_for(&publisher, "terminal status", terminal).await;

        let events = publisher.events();
        assert_eq!(states(&events).last(), Some(&WebSocketSessionState::Failed));
        let reason = events.iter().find_map(|e| match e {
            DomainEvent::GraphQlSubscriptionStatus { state: WebSocketSessionState::Failed, reason, .. } => reason.clone(),
            _ => None,
        });
        assert!(reason.unwrap_or_default().contains("acknowledge"), "{events:?}");
        assert!(!types(&received).contains(&"subscribe".to_string()), "no subscribe without an ack");
        assert_eq!(svc.session_count(), 0);
    }

    #[tokio::test]
    async fn stop_sends_complete_and_ends_the_session_cleanly() {
        let (port, received) = spawn_server(Script::Hold, Offer::Modern).await;
        let (svc, publisher) = service();
        svc.start("s1", start_for(port)).await.expect("start");
        wait_for(&publisher, "the first result", |e| !results(e).is_empty()).await;

        svc.stop("s1").await.expect("stop");
        wait_for(&publisher, "terminal status", terminal).await;

        assert_eq!(states(&publisher.events()).last(), Some(&WebSocketSessionState::Closed));
        let completes = received
            .lock()
            .expect("lock")
            .iter()
            .filter(|f| f["type"] == "complete" && f["id"] == "1")
            .count();
        assert_eq!(completes, 1, "the server must receive the stop message");
        assert_eq!(svc.session_count(), 0);
        // The id can be reused, and stopping something unknown is a no-op.
        svc.stop("s1").await.expect("idempotent");
        svc.stop("never-existed").await.expect("unknown id");
    }

    #[tokio::test]
    async fn a_server_error_is_published_and_ends_the_subscription_as_failed() {
        let (port, _received) = spawn_server(Script::ErrorFrame, Offer::Modern).await;
        let (svc, publisher) = service();

        svc.start("s1", start_for(port)).await.expect("start");
        wait_for(&publisher, "terminal status", terminal).await;

        let events = publisher.events();
        let seen = results(&events);
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].0, GraphQlSubscriptionEventKind::Error);
        assert!(seen[0].1.contains("boom"), "{seen:?}");
        assert_eq!(states(&events).last(), Some(&WebSocketSessionState::Failed));
        assert_eq!(svc.session_count(), 0);
    }

    #[tokio::test]
    async fn a_server_ping_is_answered_with_a_pong() {
        let (port, received) = spawn_server(Script::PingFirst, Offer::Modern).await;
        let (svc, publisher) = service();

        svc.start("s1", start_for(port)).await.expect("start");
        wait_for(&publisher, "terminal status", terminal).await;

        assert!(types(&received).contains(&"pong".to_string()), "{:?}", types(&received));
    }

    #[tokio::test]
    async fn a_failed_connect_is_reported_and_the_id_is_freed() {
        let probe = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let dead_port = probe.local_addr().expect("addr").port();
        drop(probe);
        let (svc, publisher) = service();

        let err = svc.start("s1", start_for(dead_port)).await.expect_err("refused");

        assert!(err.to_string().contains("connection failed"), "{err}");
        assert_eq!(states(&publisher.events()), vec![WebSocketSessionState::Connecting, WebSocketSessionState::Failed]);
        assert_eq!(svc.session_count(), 0);
    }

    #[tokio::test]
    async fn a_duplicate_session_id_is_rejected_while_one_is_running() {
        let (port, _received) = spawn_server(Script::Hold, Offer::Modern).await;
        let (svc, publisher) = service();
        svc.start("s1", start_for(port)).await.expect("first");
        wait_for(&publisher, "the first result", |e| !results(e).is_empty()).await;

        let err = svc.start("s1", start_for(port)).await.expect_err("duplicate");
        assert!(matches!(err, DomainError::AlreadyExists(_)), "{err:?}");
        assert!(svc.start("", start_for(port)).await.is_err(), "an empty id is invalid");
        svc.end_all().await;
    }
}
```

- [ ] **Step 14: Run them and confirm they fail**

Run: `cargo test -j4 -p rocket-app graphql_subscription::session_tests`
Expected: compile errors (`GraphQlSubscriptionService` not found).

- [ ] **Step 15: Implement the service**

Insert this into `crates/rocket-app/src/graphql_subscription.rs` between the resolution code (Step 11) and the test modules:

```rust
/// A session slot. `Connecting` reserves the id while the handshake is in flight.
enum Slot {
    Connecting,
    Running(oneshot::Sender<()>),
}

pub struct GraphQlSubscriptionService {
    client: Arc<dyn WebSocketClient>,
    events: Arc<dyn EventPublisher>,
    sessions: Arc<Mutex<HashMap<String, Slot>>>,
    ack_timeout: Duration,
}

/// The operation a session runs.
struct Operation {
    query: String,
    variables: Option<Value>,
    operation_name: Option<String>,
    connection_params: Option<Value>,
}

/// How a session ended.
struct Finish {
    clean: bool,
    code: Option<u16>,
    reason: Option<String>,
}

/// Why this side started closing.
enum Closing {
    /// The user stopped the subscription.
    Requested,
    /// The server sent `complete`.
    Completed,
    /// Something went wrong; the text is the reason shown to the user.
    Failed(String),
}

fn publish_status(
    events: &dyn EventPublisher,
    session_id: &str,
    state: WebSocketSessionState,
    dialect: Option<&str>,
    reason: Option<String>,
) {
    events.publish(DomainEvent::GraphQlSubscriptionStatus {
        session_id: session_id.to_string(),
        state,
        dialect: dialect.map(str::to_string),
        reason: reason.filter(|r| !r.is_empty()),
    });
}

fn publish_message(
    events: &dyn EventPublisher,
    session_id: &str,
    event: GraphQlSubscriptionEventKind,
    data: String,
) {
    events.publish(DomainEvent::GraphQlSubscriptionMessage {
        session_id: session_id.to_string(),
        event,
        data,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    });
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

async fn send_text(handle: &WebSocketHandle, text: String) -> bool {
    handle
        .outbound
        .send(WebSocketCommand::Send(WebSocketFrame::Text(text)))
        .await
        .is_ok()
}

async fn request_close(handle: &WebSocketHandle, reason: &str) {
    // The pump may already be gone; then there is nothing left to close.
    let _ = handle
        .outbound
        .send(WebSocketCommand::Close {
            code: 1000,
            reason: reason.to_string(),
        })
        .await;
}

fn failed(reason: &str) -> Finish {
    Finish {
        clean: false,
        code: None,
        reason: Some(reason.to_string()),
    }
}

/// Runs one subscription on an open socket until it ends, and says how it ended.
async fn drive(
    events: &dyn EventPublisher,
    session_id: &str,
    dialect: Dialect,
    operation: &Operation,
    handle: &mut WebSocketHandle,
    stop_rx: &mut oneshot::Receiver<()>,
    ack_timeout: Duration,
) -> Finish {
    if !send_text(handle, init_message(operation.connection_params.as_ref())).await {
        return failed("the connection closed before the handshake finished");
    }

    let mut acknowledged = false;
    let mut closing: Option<Closing> = None;
    let deadline = tokio::time::sleep(ack_timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            // A stop request, or the service dropping the sender, both mean "end it".
            _ = &mut *stop_rx, if closing.is_none() => {
                for message in stop_messages(dialect) {
                    let _ = send_text(handle, message).await;
                }
                request_close(handle, "unsubscribed").await;
                closing = Some(Closing::Requested);
            }
            _ = &mut deadline, if !acknowledged && closing.is_none() => {
                request_close(handle, "no acknowledgement").await;
                closing = Some(Closing::Failed(
                    "the server did not acknowledge the connection in time".into(),
                ));
            }
            event = handle.events.recv() => match event {
                None => return failed("the connection ended unexpectedly"),
                Some(WebSocketEvent::Closed(close)) => {
                    return match closing {
                        Some(Closing::Requested) | Some(Closing::Completed) => Finish {
                            clean: true,
                            code: close.code,
                            reason: None,
                        },
                        Some(Closing::Failed(reason)) => Finish {
                            clean: false,
                            code: close.code,
                            reason: Some(reason),
                        },
                        None => Finish {
                            clean: close.clean,
                            code: close.code,
                            reason: Some(close.reason),
                        },
                    };
                }
                Some(WebSocketEvent::Frame(WebSocketFrame::Binary(_))) => {}
                Some(WebSocketEvent::Frame(WebSocketFrame::Text(text))) => {
                    // After this side started closing, remaining frames are only drained.
                    if closing.is_some() {
                        continue;
                    }
                    match interpret(dialect, &text) {
                        Incoming::Ack if !acknowledged => {
                            acknowledged = true;
                            let start = start_message(
                                dialect,
                                &operation.query,
                                operation.variables.as_ref(),
                                operation.operation_name.as_deref(),
                            );
                            if !send_text(handle, start).await {
                                return failed("the connection closed before the subscription started");
                            }
                            publish_status(
                                events,
                                session_id,
                                WebSocketSessionState::Open,
                                Some(dialect.subprotocol()),
                                None,
                            );
                        }
                        Incoming::Ping => {
                            let _ = send_text(handle, pong_message()).await;
                        }
                        Incoming::Result(value) => publish_message(
                            events,
                            session_id,
                            GraphQlSubscriptionEventKind::Next,
                            pretty(&value),
                        ),
                        Incoming::Errors(value) => {
                            publish_message(
                                events,
                                session_id,
                                GraphQlSubscriptionEventKind::Error,
                                pretty(&value),
                            );
                            request_close(handle, "operation error").await;
                            closing = Some(Closing::Failed("the server reported an error".into()));
                        }
                        Incoming::ConnectionError(value) => {
                            publish_message(
                                events,
                                session_id,
                                GraphQlSubscriptionEventKind::Error,
                                pretty(&value),
                            );
                            request_close(handle, "connection error").await;
                            closing = Some(Closing::Failed("the server rejected the connection".into()));
                        }
                        Incoming::Complete => {
                            publish_message(
                                events,
                                session_id,
                                GraphQlSubscriptionEventKind::Complete,
                                String::new(),
                            );
                            request_close(handle, "complete").await;
                            closing = Some(Closing::Completed);
                        }
                        Incoming::Ack | Incoming::Pong | Incoming::KeepAlive | Incoming::Ignore => {}
                    }
                }
            }
        }
    }
}

impl GraphQlSubscriptionService {
    pub fn new(client: Arc<dyn WebSocketClient>, events: Arc<dyn EventPublisher>) -> Self {
        Self::with_ack_timeout(client, events, DEFAULT_ACK_TIMEOUT)
    }

    /// Test seam for the acknowledgement timeout. Production wiring uses `new`.
    pub fn with_ack_timeout(
        client: Arc<dyn WebSocketClient>,
        events: Arc<dyn EventPublisher>,
        ack_timeout: Duration,
    ) -> Self {
        Self {
            client,
            events,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            ack_timeout,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Slot>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Number of sessions, including ones still connecting.
    pub fn session_count(&self) -> usize {
        self.lock().len()
    }

    /// Opens the socket and starts the subscription in the background. Returns once the socket is
    /// open; the acknowledgement, the subscribe, the results and the end arrive as events.
    /// A `stop` that arrives while the handshake is in flight cancels it.
    pub async fn start(
        &self,
        session_id: &str,
        start: GraphQlSubscriptionStart,
    ) -> DomainResult<()> {
        if session_id.trim().is_empty() {
            return Err(DomainError::InvalidInput("session id is required".into()));
        }
        {
            let mut sessions = self.lock();
            if sessions.contains_key(session_id) {
                return Err(DomainError::AlreadyExists(format!(
                    "GraphQL subscription session '{session_id}'"
                )));
            }
            sessions.insert(session_id.to_string(), Slot::Connecting);
        }
        publish_status(
            self.events.as_ref(),
            session_id,
            WebSocketSessionState::Connecting,
            None,
            None,
        );

        let mut connect = start.connect.clone();
        if connect.subprotocols.is_empty() {
            connect.subprotocols = vec![SUBPROTOCOL_TRANSPORT.to_string(), SUBPROTOCOL_LEGACY.to_string()];
        }
        let handle = match self.client.connect(connect).await {
            Ok(handle) => handle,
            Err(error) => {
                self.lock().remove(session_id);
                publish_status(
                    self.events.as_ref(),
                    session_id,
                    WebSocketSessionState::Failed,
                    None,
                    Some(error.to_string()),
                );
                return Err(error);
            }
        };

        let (stop_tx, stop_rx) = oneshot::channel();
        {
            let mut sessions = self.lock();
            match sessions.get_mut(session_id) {
                Some(slot @ Slot::Connecting) => *slot = Slot::Running(stop_tx),
                _ => {
                    // Cancelled while connecting: do not leave the new socket open.
                    drop(sessions);
                    request_close(&handle, "cancelled").await;
                    return Err(DomainError::Conflict("subscription was cancelled".into()));
                }
            }
        }

        let dialect = Dialect::from_subprotocol(handle.subprotocol.as_deref());
        let operation = Operation {
            query: start.query,
            variables: start.variables,
            operation_name: start.operation_name,
            connection_params: start.connection_params,
        };
        let events = Arc::clone(&self.events);
        let sessions = Arc::clone(&self.sessions);
        let ack_timeout = self.ack_timeout;
        let id = session_id.to_string();
        tokio::spawn(async move {
            let mut handle = handle;
            let mut stop_rx = stop_rx;
            let finish = drive(
                events.as_ref(),
                &id,
                dialect,
                &operation,
                &mut handle,
                &mut stop_rx,
                ack_timeout,
            )
            .await;
            sessions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            let state = if finish.clean {
                WebSocketSessionState::Closed
            } else {
                WebSocketSessionState::Failed
            };
            publish_status(events.as_ref(), &id, state, None, finish.reason);
        });
        Ok(())
    }

    /// Ends a subscription. The terminal status comes from the session when the close finishes.
    /// Unknown or finished sessions are a no-op; one still connecting is cancelled.
    pub async fn stop(&self, session_id: &str) -> DomainResult<()> {
        let slot = self.lock().remove(session_id);
        if let Some(Slot::Running(stop)) = slot {
            // The session may already have ended; then there is nothing to stop.
            let _ = stop.send(());
        }
        Ok(())
    }

    /// Stops every subscription. Used on app exit.
    pub async fn end_all(&self) {
        let drained: Vec<Slot> = self.lock().drain().map(|(_, slot)| slot).collect();
        for slot in drained {
            if let Slot::Running(stop) = slot {
                let _ = stop.send(());
            }
        }
    }
}
```

In `crates/rocket-app/src/lib.rs` add the re-export next to the other `pub use` lines:

```rust
pub use graphql_subscription::{
    resolve_graphql_subscription, to_websocket_url, GraphQlSubscribeInput,
    GraphQlSubscriptionService, GraphQlSubscriptionStart,
};
```

Add a `GraphQlSubscriptionService` row and a note on the `graphql_subscription/protocol.rs` module to `crates/rocket-app/CLAUDE.md`.

- [ ] **Step 16: Run the service tests**

Run: `cargo test -j4 -p rocket-app graphql_subscription`
Expected: 8 protocol, 7 resolution and 8 session tests pass. Notes if one fails:
- `a_modern_subscription_streams_results_then_completes` expects the final status `Closed`. After `complete` the service sends a close and waits for the client's `Closed` event (at most 3 seconds, the client's close deadline); the scripted server leaves the socket to the library's close handling, which answers a close frame on its next read.
- If `stop_sends_complete_and_ends_the_session_cleanly` is flaky, check that the server loop keeps reading after it sent the first result (it does: the `while let` loop continues), because the stop message has to reach it.
- The timeout test uses a 300 ms acknowledgement timeout; do not shorten it below 100 ms.

### Step group E: Tauri wiring

- [ ] **Step 17: Add the commands and wire the service**

Create `src-tauri/src/commands/graphql_subscription.rs`:

```rust
use rocket_app::{
    resolve_graphql_subscription, GraphQlSubscribeInput, GraphQlSubscriptionService,
    RequestExecutionService,
};
use rocket_shared::error::DomainError;
use tauri::State;

/// Starts a GraphQL subscription under a caller-chosen session id. Results and status changes
/// arrive as the `graphql:subscription-message` and `graphql:subscription-status` events; this
/// only reports whether the socket opened.
#[tauri::command]
pub async fn graphql_subscribe(
    session_id: String,
    input: GraphQlSubscribeInput,
    exec: State<'_, RequestExecutionService>,
    subscriptions: State<'_, GraphQlSubscriptionService>,
) -> Result<(), DomainError> {
    let start = resolve_graphql_subscription(&exec, input).await?;
    subscriptions.start(&session_id, start).await
}

#[tauri::command]
pub async fn graphql_unsubscribe(
    session_id: String,
    subscriptions: State<'_, GraphQlSubscriptionService>,
) -> Result<(), DomainError> {
    subscriptions.stop(&session_id).await
}
```

In `src-tauri/src/commands/mod.rs` add `pub mod graphql_subscription;` (alphabetical, after `pub mod git;`).

In `src-tauri/src/lib.rs`:
1. Next to the `websocket_svc` Plan 08 created, add (reusing the same client type; the two services each hold their own `Arc`):

```rust
            let graphql_subscription_svc = rocket_app::GraphQlSubscriptionService::new(
                Arc::new(rocket_infra::TungsteniteWebSocketClient::new()),
                Arc::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
            );
```

2. After `app.manage(websocket_svc);` add `app.manage(graphql_subscription_svc);`.
3. In the invoke handler list, after `commands::websocket::ws_disconnect,` add:

```rust
            commands::graphql_subscription::graphql_subscribe,
            commands::graphql_subscription::graphql_unsubscribe,
```

4. Next to **both** places Plan 08 added the WebSocket sweep (the signal handler and the `RunEvent::Exit` arm) add the same best-effort sweep for subscriptions. In the signal handler:

```rust
            if let Some(svc) = app_handle.try_state::<rocket_app::GraphQlSubscriptionService>() {
                svc.end_all().await;
            }
```

In the `RunEvent::Exit` arm: `tauri::async_runtime::block_on(svc.end_all());`.

Add a `graphql_subscribe` and `graphql_unsubscribe` entry to `.claude/tauri-commands.md` if that file exists in your checkout.

- [ ] **Step 18: Verify**

Run, in order:
- `cargo check -j4` (the whole workspace; this proves the exhaustive `TauriEventBus` match and the command signatures).
- `cargo test -j4 -p rocket-shared graphql_subscription_events`
- `cargo test -j4 -p rocket-app graphql_subscription`
- `cargo clippy -j4 -p rocket-shared -p rocket-app -- -D warnings`

Expected: green. Do not run the whole workspace test suite.

- [ ] **Step 19: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path: `crates/rocket-shared/src/events.rs`, `crates/rocket-app/src/graphql_subscription.rs`, `crates/rocket-app/src/graphql_subscription/protocol.rs`, `crates/rocket-app/src/lib.rs`, `crates/rocket-app/Cargo.toml`, `Cargo.lock`, `crates/rocket-app/CLAUDE.md`, `src-tauri/src/commands/graphql_subscription.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/tauri_event_bus.rs`.
Suggested subject: `feat(graphql): subscriptions over graphql-transport-ws and legacy graphql-ws`.

---

## Task 2: Subscribe and Stop in the GraphQL tab, with streamed results

**Files:**
- Create: `src/lib/graphql-subscription-session.ts`, `src/lib/graphql-subscription-event-bridge.ts`, `src/lib/streaming-release.ts`, `src/hooks/useSelectedOperationKind.ts`, `src/components/request/GraphQlSubscriptionPanel.tsx`, and tests `src/lib/__tests__/graphql-subscription-session.test.ts`, `src/lib/__tests__/graphql-subscription-event-bridge.test.ts`, `src/stores/__tests__/websocket-store.append-entry.test.ts`, `src/hooks/__tests__/useSelectedOperationKind.test.ts`, `src/components/request/__tests__/GraphQlSubscriptionPanel.test.tsx`
- Modify: `src/lib/tauri-api.ts`, `src/types/pane-types.ts` (`GraphQlState`), `src/stores/websocket-store.ts` (one action), `src/lib/websocket-session.ts` (export `scopeFor`), `src/stores/pane-store.ts` (the two release call sites), `src/App.tsx`, `src/hooks/useKeyboardShortcuts.ts`, `src/components/request/RequestPanel.tsx`

**Interfaces:**
- Consumes: `graphql_subscribe` and `graphql_unsubscribe` (Task 1); `useWebSocketStore`, `IDLE_SESSION`, `MAX_LOG_ENTRIES`, `appendCapped`, `MessageLog`, `MessageLogEntry` (Plan 09); `listGraphQlOperations`, `GraphQlOperation`, `RequestState.graphql`, `GraphQlState`, `isGraphQl` in `RequestPanel` (Plan 06); `toPersistedHeaders`, `toPersistedAuth`.
- Produces:
  - `tauri-api.ts`: `GraphQlSubscribeInput`, `GraphQlSubscriptionMessageEvent`, `GraphQlSubscriptionStatusEvent`, `graphqlSubscribe`, `graphqlUnsubscribe`, `onGraphQlSubscriptionMessage`, `onGraphQlSubscriptionStatus`.
  - `GraphQlState.connectionParams?: string` (session state, never saved).
  - `useWebSocketStore.appendEntry(sessionId, entry: Omit<MessageLogEntry, 'id'>)`.
  - `graphql-subscription-session.ts`: `buildSubscribeInput(tab)`, `startSubscription(tab)`, `stopSubscription(tabId)`, `releaseGraphQlSubscriptionTab(tab)`.
  - `useGraphQlSubscriptionEventBridge()`.
  - `streaming-release.ts`: `releaseStreamingTab(tab)` (WebSocket and GraphQL).
  - `useSelectedOperationKind(query, operationName)` and the pure `pickOperationKind(operations, operationName)`.
  - `<GraphQlSubscriptionPanel tab onConnectionParamsChange />`.

- [ ] **Step 1: Types and commands in `tauri-api.ts` and `pane-types.ts`**

In `src/lib/tauri-api.ts`, after the WebSocket session section Plan 09 added, add:

```ts
// ============================================================
// GraphQL subscriptions
// ============================================================

export interface GraphQlSubscribeInput extends WebSocketScopeInput {
  /** The request URL. `http` and `https` are turned into `ws` and `wss`. */
  url: string;
  /** Where subscriptions are served when that differs from `url`. */
  subscriptionUrl?: string;
  query: string;
  /** JSON object text. */
  variables?: string;
  operationName?: string;
  /** JSON object text sent with `connection_init`. */
  connectionParams?: string;
  headers: Header[];
  auth?: Auth;
  verifySsl?: boolean;
  timeoutMs?: number;
}

/** `graphql_subscribe` only reports whether the socket opened. Results arrive as events. */
export const graphqlSubscribe = (sessionId: string, input: GraphQlSubscribeInput) =>
  invoke<void>('graphql_subscribe', { sessionId, input });

export const graphqlUnsubscribe = (sessionId: string) =>
  invoke<void>('graphql_unsubscribe', { sessionId });

/** Payload of `graphql:subscription-message`. Fields are snake_case, like every `DomainEvent`. */
export interface GraphQlSubscriptionMessageEvent {
  type: 'graphQlSubscriptionMessage';
  session_id: string;
  event: 'next' | 'error' | 'complete';
  /** Pretty-printed JSON. Empty for `complete`. */
  data: string;
  timestamp_ms: number;
}

export interface GraphQlSubscriptionStatusEvent {
  type: 'graphQlSubscriptionStatus';
  session_id: string;
  state: 'connecting' | 'open' | 'closed' | 'failed';
  /** The subprotocol the server selected. */
  dialect: string | null;
  reason: string | null;
}

export const onGraphQlSubscriptionMessage = (
  handler: (event: GraphQlSubscriptionMessageEvent) => void,
): Promise<UnlistenFn> =>
  listen<GraphQlSubscriptionMessageEvent>('graphql:subscription-message', (e) =>
    handler(e.payload),
  );

export const onGraphQlSubscriptionStatus = (
  handler: (event: GraphQlSubscriptionStatusEvent) => void,
): Promise<UnlistenFn> =>
  listen<GraphQlSubscriptionStatusEvent>('graphql:subscription-status', (e) =>
    handler(e.payload),
  );
```

In `src/types/pane-types.ts`, add to Plan 05's `GraphQlState` (after `operationName?`):

```ts
  /** JSON object text sent with `connection_init` for subscriptions. Session state, never saved. */
  connectionParams?: string;
```

- [ ] **Step 2: Write the failing store, bridge and hook tests**

Create `src/stores/__tests__/websocket-store.append-entry.test.ts`:

```ts
import { beforeEach, describe, expect, it } from 'vitest';
import { MAX_LOG_ENTRIES, useWebSocketStore } from '../websocket-store';

beforeEach(() => {
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('websocket-store appendEntry', () => {
  it('appends a labelled entry to the tab that owns the session', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    useWebSocketStore.getState().appendEntry('sess-1', {
      direction: 'in',
      label: 'next',
      kind: 'text',
      data: '{ "n": 1 }',
      size: 10,
      timestampMs: 5,
    });

    const log = useWebSocketStore.getState().byTab['tab-1'].log;
    expect(log).toHaveLength(1);
    expect(log[0]).toMatchObject({ direction: 'in', label: 'next', data: '{ "n": 1 }', size: 10 });
    expect(log[0].id).toBeTruthy();
  });

  it('ignores a session that is unknown or already finished', () => {
    useWebSocketStore.getState().appendEntry('nobody', {
      direction: 'in',
      kind: 'text',
      data: 'x',
      size: 1,
      timestampMs: 1,
    });
    expect(useWebSocketStore.getState().byTab).toEqual({});
  });

  it('keeps the log bounded like the other entries do', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    for (let i = 0; i < MAX_LOG_ENTRIES + 3; i++) {
      useWebSocketStore.getState().appendEntry('sess-1', {
        direction: 'in',
        kind: 'text',
        data: `m${i}`,
        size: 2,
        timestampMs: i,
      });
    }
    const log = useWebSocketStore.getState().byTab['tab-1'].log;
    expect(log).toHaveLength(MAX_LOG_ENTRIES);
    expect(log[0].data).toBe('m3');
  });
});
```

Create `src/lib/__tests__/graphql-subscription-event-bridge.test.ts`:

```ts
import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  GraphQlSubscriptionMessageEvent,
  GraphQlSubscriptionStatusEvent,
} from '@/lib/tauri-api';
import { useGraphQlSubscriptionEventBridge } from '@/lib/graphql-subscription-event-bridge';
import { useWebSocketStore } from '@/stores/websocket-store';

let messageHandler: ((e: GraphQlSubscriptionMessageEvent) => void) | undefined;
let statusHandler: ((e: GraphQlSubscriptionStatusEvent) => void) | undefined;
const unlisten = vi.fn();

vi.mock('@/lib/tauri-api', () => ({
  onGraphQlSubscriptionMessage: vi.fn((h: (e: GraphQlSubscriptionMessageEvent) => void) => {
    messageHandler = h;
    return Promise.resolve(unlisten);
  }),
  onGraphQlSubscriptionStatus: vi.fn((h: (e: GraphQlSubscriptionStatusEvent) => void) => {
    statusHandler = h;
    return Promise.resolve(unlisten);
  }),
}));

beforeEach(() => {
  messageHandler = undefined;
  statusHandler = undefined;
  unlisten.mockClear();
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('useGraphQlSubscriptionEventBridge', () => {
  it('turns subscription events into log entries and status changes of the owning tab', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    renderHook(() => useGraphQlSubscriptionEventBridge());

    statusHandler?.({
      type: 'graphQlSubscriptionStatus',
      session_id: 'sess-1',
      state: 'open',
      dialect: 'graphql-transport-ws',
      reason: null,
    });
    messageHandler?.({
      type: 'graphQlSubscriptionMessage',
      session_id: 'sess-1',
      event: 'next',
      data: '{ "data": 1 }',
      timestamp_ms: 9,
    });
    messageHandler?.({
      type: 'graphQlSubscriptionMessage',
      session_id: 'sess-1',
      event: 'complete',
      data: '',
      timestamp_ms: 10,
    });

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('open');
    expect(session.subprotocol).toBe('graphql-transport-ws');
    expect(session.log.map((e) => [e.direction, e.label, e.data])).toEqual([
      ['system', undefined, 'Connected (graphql-transport-ws)'],
      ['in', 'next', '{ "data": 1 }'],
      ['in', 'complete', ''],
    ]);
    expect(session.log[1].size).toBe(new TextEncoder().encode('{ "data": 1 }').length);
  });

  it('a failed status records the reason', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    renderHook(() => useGraphQlSubscriptionEventBridge());

    statusHandler?.({
      type: 'graphQlSubscriptionStatus',
      session_id: 'sess-1',
      state: 'failed',
      dialect: null,
      reason: 'the server reported an error',
    });

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('failed');
    expect(session.error).toBe('the server reported an error');
  });

  it('unsubscribes both listeners on unmount', async () => {
    const { unmount } = renderHook(() => useGraphQlSubscriptionEventBridge());
    unmount();
    await Promise.resolve();
    await Promise.resolve();
    expect(unlisten).toHaveBeenCalledTimes(2);
  });
});
```

Create `src/hooks/__tests__/useSelectedOperationKind.test.ts`:

```ts
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { pickOperationKind, useSelectedOperationKind } from '../useSelectedOperationKind';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listGraphQlOperations: vi.fn() };
});

describe('pickOperationKind', () => {
  const ops = [
    { name: 'A', kind: 'query' as const },
    { name: 'S', kind: 'subscription' as const },
  ];

  it('uses the named operation', () => {
    expect(pickOperationKind(ops, 'S')).toBe('subscription');
    expect(pickOperationKind(ops, 'A')).toBe('query');
    expect(pickOperationKind(ops, 'Nope')).toBeNull();
  });

  it('uses the only operation when none is named, and nothing when it is ambiguous', () => {
    expect(pickOperationKind([ops[1]], undefined)).toBe('subscription');
    expect(pickOperationKind(ops, undefined)).toBeNull();
    expect(pickOperationKind([], undefined)).toBeNull();
  });
});

describe('useSelectedOperationKind', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listGraphQlOperations).mockReset();
  });

  it('asks the backend scanner and reports the kind', async () => {
    vi.mocked(tauriApi.listGraphQlOperations).mockResolvedValue([
      { name: null, kind: 'subscription' },
    ]);
    const { result } = renderHook(() => useSelectedOperationKind('subscription { a }', undefined));
    await waitFor(() => expect(result.current).toBe('subscription'));
    expect(tauriApi.listGraphQlOperations).toHaveBeenCalledWith('subscription { a }');
  });

  it('reports null for a blank document without calling the backend', async () => {
    const { result } = renderHook(() => useSelectedOperationKind('   ', undefined));
    expect(result.current).toBeNull();
    expect(tauriApi.listGraphQlOperations).not.toHaveBeenCalled();
  });

  it('reports null when the scan fails', async () => {
    vi.mocked(tauriApi.listGraphQlOperations).mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useSelectedOperationKind('query { a }', undefined));
    await waitFor(() => expect(tauriApi.listGraphQlOperations).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });
});
```

- [ ] **Step 3: Run them and confirm they fail**

Run: `yarn test --run websocket-store.append-entry graphql-subscription-event-bridge useSelectedOperationKind`
Expected: FAIL, modules and the `appendEntry` action do not exist.

- [ ] **Step 4: Implement the store action, the bridge and the hook**

In `src/stores/websocket-store.ts` add `appendEntry` to the state interface and the store. Interface line:

```ts
  /** Appends a prebuilt log entry to the tab that owns the session (GraphQL subscription results). */
  appendEntry: (sessionId: string, entry: Omit<MessageLogEntry, 'id'>) => void;
```

and, after `applyMessage`:

```ts
  appendEntry(sessionId, entry) {
    const { byTab, tabBySession } = get();
    const tabId = tabBySession[sessionId];
    if (!tabId) return;
    const session = byTab[tabId];
    if (!session) return;
    const withId: MessageLogEntry = { ...entry, id: nextEntryId() };
    set({ byTab: { ...byTab, [tabId]: { ...session, log: appendCapped(session.log, withId) } } });
  },
```

Create `src/lib/graphql-subscription-event-bridge.ts`:

```ts
import { useEffect } from 'react';
import { onGraphQlSubscriptionMessage, onGraphQlSubscriptionStatus } from '@/lib/tauri-api';
import { useWebSocketStore } from '@/stores/websocket-store';

const encoder = new TextEncoder();

// Subscribes once, for the app's lifetime, to the GraphQL subscription events and feeds them to
// the same store the WebSocket tab uses: results become labelled log lines, status changes map
// onto the WebSocket status shape (the dialect plays the part of the subprotocol).
export function useGraphQlSubscriptionEventBridge(): void {
  useEffect(() => {
    const unsubs = Promise.all([
      onGraphQlSubscriptionMessage((e) =>
        useWebSocketStore.getState().appendEntry(e.session_id, {
          direction: 'in',
          label: e.event,
          kind: 'text',
          data: e.data,
          size: encoder.encode(e.data).length,
          timestampMs: e.timestamp_ms,
        }),
      ),
      onGraphQlSubscriptionStatus((e) =>
        useWebSocketStore.getState().applyStatus({
          type: 'webSocketStatus',
          session_id: e.session_id,
          state: e.state,
          subprotocol: e.dialect,
          code: null,
          reason: e.reason,
        }),
      ),
    ]);
    return () => {
      unsubs.then((fns) => {
        for (const fn of fns) fn();
      });
    };
  }, []);
}
```

Create `src/hooks/useSelectedOperationKind.ts`:

```ts
import { useEffect, useMemo, useState } from 'react';
import { type GraphQlOperation, listGraphQlOperations } from '@/lib/tauri-api';

export type OperationKind = GraphQlOperation['kind'];

/** The kind of the operation a send would run, or null when it cannot be told yet. */
export function pickOperationKind(
  operations: GraphQlOperation[],
  operationName: string | undefined,
): OperationKind | null {
  if (operationName) return operations.find((o) => o.name === operationName)?.kind ?? null;
  return operations.length === 1 ? operations[0].kind : null;
}

// Asks the backend scanner (the same one the editor's operation picker uses), debounced while
// the user types, and reports the kind of the selected operation.
export function useSelectedOperationKind(
  query: string,
  operationName: string | undefined,
): OperationKind | null {
  const [operations, setOperations] = useState<GraphQlOperation[]>([]);

  useEffect(() => {
    if (query.trim() === '') {
      setOperations([]);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      listGraphQlOperations(query)
        .then((ops) => {
          if (!cancelled) setOperations(ops);
        })
        .catch(() => {
          if (!cancelled) setOperations([]);
        });
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query]);

  return useMemo(() => pickOperationKind(operations, operationName), [operations, operationName]);
}
```

Run: `yarn test --run websocket-store.append-entry graphql-subscription-event-bridge useSelectedOperationKind`
Expected: all pass. (The hook test for a scanned document waits for the 300 ms debounce through `waitFor`, whose default timeout is 1000 ms.)

- [ ] **Step 5: Write the failing session and panel tests**

Create `src/lib/__tests__/graphql-subscription-session.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  buildSubscribeInput,
  releaseGraphQlSubscriptionTab,
  startSubscription,
  stopSubscription,
} from '@/lib/graphql-subscription-session';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import * as tauriApi from '@/lib/tauri-api';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    graphqlSubscribe: vi.fn(),
    graphqlUnsubscribe: vi.fn(),
  };
});

function gqlTab(): RequestTab {
  const request = createDefaultRequestFor('graphql');
  request.url = 'https://api.example.com/graphql';
  request.headers = [
    { id: 'h1', key: 'X-Token', value: '{{token}}', enabled: true },
    { id: 'h2', key: '', value: 'draft', enabled: true },
  ];
  request.auth = { authType: 'bearer', bearer: { token: 'abc' } };
  request.graphql = {
    query: 'subscription OnN { n }',
    variables: '{"room":"general"}',
    operationName: 'OnN',
    connectionParams: '{"token":"abc"}',
  };
  return {
    id: 'tab-1',
    title: 'Updates',
    tabType: 'request',
    request,
    response: null,
    isDirty: false,
    source: { collection: 'my-api', path: 'updates.yml' },
  };
}

beforeEach(() => {
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
  vi.mocked(tauriApi.graphqlSubscribe).mockReset().mockResolvedValue(undefined);
  vi.mocked(tauriApi.graphqlUnsubscribe).mockReset().mockResolvedValue(undefined);
});

describe('buildSubscribeInput', () => {
  it('sends the query, variables, operation, params, headers, auth and scope', () => {
    const input = buildSubscribeInput(gqlTab());
    expect(input.url).toBe('https://api.example.com/graphql');
    expect(input.query).toBe('subscription OnN { n }');
    expect(input.variables).toBe('{"room":"general"}');
    expect(input.operationName).toBe('OnN');
    expect(input.connectionParams).toBe('{"token":"abc"}');
    expect(input.headers).toEqual([{ key: 'X-Token', value: '{{token}}', enabled: true }]);
    expect(input.auth).toEqual({ authType: 'bearer', token: 'abc' });
    expect(input.collection).toBe('my-api');
    expect(input.requestPath).toBe('updates.yml');
    expect(input.verifySsl).toBe(true);
  });

  it('omits blank variables and params', () => {
    const tab = gqlTab();
    tab.request.graphql = { query: 'subscription { n }', variables: '  ', connectionParams: ' ' };
    const input = buildSubscribeInput(tab);
    expect(input.variables).toBeUndefined();
    expect(input.connectionParams).toBeUndefined();
  });
});

describe('startSubscription', () => {
  it('registers the session before the call, so early events are routed', async () => {
    let registeredWhenInvoked: string | null = null;
    vi.mocked(tauriApi.graphqlSubscribe).mockImplementation(async (sessionId) => {
      registeredWhenInvoked = useWebSocketStore.getState().tabBySession[sessionId] ?? null;
    });

    await startSubscription(gqlTab());

    expect(registeredWhenInvoked).toBe('tab-1');
  });

  it('shows a rejected start through the store instead of throwing', async () => {
    vi.mocked(tauriApi.graphqlSubscribe).mockRejectedValue('connection failed: refused');
    await startSubscription(gqlTab());

    const session = useWebSocketStore.getState().byTab['tab-1'];
    expect(session.status).toBe('failed');
    expect(session.error).toBe('connection failed: refused');
  });

  it('does not start a second subscription while one is connecting or open', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'existing');
    await startSubscription(gqlTab());
    expect(tauriApi.graphqlSubscribe).not.toHaveBeenCalled();
  });
});

describe('stop and release', () => {
  it('stopSubscription asks the backend to end the live session', async () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    await stopSubscription('tab-1');
    expect(tauriApi.graphqlUnsubscribe).toHaveBeenCalledWith('sess-1');
  });

  it('releasing a graphql tab ends its subscription and forgets its state', () => {
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');

    releaseGraphQlSubscriptionTab(gqlTab());

    expect(tauriApi.graphqlUnsubscribe).toHaveBeenCalledWith('sess-1');
    expect(useWebSocketStore.getState().byTab['tab-1']).toBeUndefined();
  });

  it('releasing an http tab does nothing', () => {
    const http: RequestTab = { ...gqlTab(), request: createDefaultRequestFor('http') };
    useWebSocketStore.getState().beginSession('tab-1', 'sess-1');
    releaseGraphQlSubscriptionTab(http);
    expect(tauriApi.graphqlUnsubscribe).not.toHaveBeenCalled();
  });
});
```

Create `src/components/request/__tests__/GraphQlSubscriptionPanel.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { GraphQlSubscriptionPanel } from '@/components/request/GraphQlSubscriptionPanel';
import { createDefaultRequestFor } from '@/lib/pane-utils';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';

// CodeMirror does not run in jsdom; a stand-in keeps the panel testable.
vi.mock('@/components/editor', () => ({
  SingleLineEditor: ({
    value,
    onChange,
    placeholder,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
  }) => (
    <input aria-label='Connection params' placeholder={placeholder} value={value} onChange={(e) => onChange(e.target.value)} />
  ),
}));

function tab(): RequestTab {
  const request = createDefaultRequestFor('graphql');
  request.graphql = { query: 'subscription { n }', variables: '', connectionParams: '' };
  return { id: 'tab-1', title: 'Updates', tabType: 'request', request, response: null, isDirty: false };
}

beforeEach(() => {
  useWebSocketStore.setState({ byTab: {}, tabBySession: {} });
});

describe('GraphQlSubscriptionPanel', () => {
  it('invites the user to subscribe when there is nothing yet', () => {
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={vi.fn()} />);
    expect(screen.getByText('Subscribe to see streamed results here.')).toBeInTheDocument();
    expect(screen.getByText('Not subscribed')).toBeInTheDocument();
  });

  it('shows the status and the streamed results with their labels', () => {
    useWebSocketStore.setState({
      byTab: {
        'tab-1': {
          sessionId: 's1',
          status: 'open',
          subprotocol: 'graphql-transport-ws',
          error: null,
          log: [
            { id: 'a', direction: 'in', label: 'next', kind: 'text', data: '{"data":1}', size: 10, timestampMs: 1000 },
            { id: 'b', direction: 'in', label: 'complete', kind: 'text', data: '', size: 0, timestampMs: 2000 },
          ],
        },
      },
      tabBySession: { s1: 'tab-1' },
    });
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={vi.fn()} />);

    expect(screen.getByText('Subscribed')).toBeInTheDocument();
    expect(screen.getByText('graphql-transport-ws')).toBeInTheDocument();
    expect(screen.getByText('next')).toBeInTheDocument();
    expect(screen.getByText('complete')).toBeInTheDocument();
    expect(screen.getByText('{"data":1}')).toBeInTheDocument();
  });

  it('a failed subscription shows its reason', () => {
    useWebSocketStore.setState({
      byTab: {
        'tab-1': { sessionId: null, status: 'failed', subprotocol: null, error: 'the server reported an error', log: [] },
      },
      tabBySession: {},
    });
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={vi.fn()} />);
    expect(screen.getByText('Failed')).toBeInTheDocument();
    expect(screen.getByText('the server reported an error')).toBeInTheDocument();
  });

  it('clearing the log empties only the log', async () => {
    useWebSocketStore.setState({
      byTab: {
        'tab-1': {
          sessionId: 's1',
          status: 'open',
          subprotocol: null,
          error: null,
          log: [{ id: 'a', direction: 'in', label: 'next', kind: 'text', data: 'x', size: 1, timestampMs: 1 }],
        },
      },
      tabBySession: { s1: 'tab-1' },
    });
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={vi.fn()} />);

    await userEvent.setup().click(screen.getByRole('button', { name: 'Clear log' }));

    expect(useWebSocketStore.getState().byTab['tab-1'].log).toEqual([]);
    expect(useWebSocketStore.getState().byTab['tab-1'].sessionId).toBe('s1');
  });

  it('edits to the connection params are reported', async () => {
    const onChange = vi.fn();
    render(<GraphQlSubscriptionPanel tab={tab()} onConnectionParamsChange={onChange} />);
    await userEvent.setup().type(screen.getByLabelText('Connection params'), '{');
    expect(onChange).toHaveBeenCalledWith('{');
  });
});
```

- [ ] **Step 6: Run them and confirm they fail**

Run: `yarn test --run graphql-subscription-session GraphQlSubscriptionPanel`
Expected: FAIL, modules not found.

- [ ] **Step 7: Implement the session actions, the release helper and the panel**

In `src/lib/websocket-session.ts` change `function scopeFor(tab: RequestTab): WebSocketScopeInput {` to `export function scopeFor(tab: RequestTab): WebSocketScopeInput {` (the GraphQL session reuses it, so both read the active environment the same way).

Create `src/lib/graphql-subscription-session.ts`:

```ts
import { toPersistedAuth } from '@/lib/persisted-auth';
import { toPersistedHeaders } from '@/lib/persisted-headers';
import { type GraphQlSubscribeInput, graphqlSubscribe, graphqlUnsubscribe } from '@/lib/tauri-api';
import { scopeFor } from '@/lib/websocket-session';
import { useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab, Tab } from '@/types/pane-types';
import { isRequestTab } from '@/types/pane-types';

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** The `graphql_subscribe` input for a tab: raw values, the backend resolves variables. */
export function buildSubscribeInput(tab: RequestTab): GraphQlSubscribeInput {
  const { request } = tab;
  const gql = request.graphql ?? { query: '', variables: '' };
  return {
    url: request.url,
    query: gql.query,
    variables: gql.variables.trim() === '' ? undefined : gql.variables,
    operationName: gql.operationName,
    connectionParams: gql.connectionParams?.trim() ? gql.connectionParams : undefined,
    headers: toPersistedHeaders(request.headers),
    auth: toPersistedAuth(request.auth),
    verifySsl: request.settings.verifySsl,
    timeoutMs: request.settings.timeoutMs > 0 ? request.settings.timeoutMs : undefined,
    ...scopeFor(tab),
  };
}

/**
 * Starts a subscription for the tab. The session id is registered in the store BEFORE the call,
 * because the backend publishes its first events before `graphql_subscribe` resolves. A rejected
 * start is shown through the store, not thrown.
 */
export async function startSubscription(tab: RequestTab): Promise<void> {
  const current = useWebSocketStore.getState().byTab[tab.id];
  if (current && (current.status === 'connecting' || current.status === 'open')) return;

  const sessionId = crypto.randomUUID();
  useWebSocketStore.getState().beginSession(tab.id, sessionId);
  try {
    await graphqlSubscribe(sessionId, buildSubscribeInput(tab));
  } catch (err) {
    useWebSocketStore.getState().failSession(tab.id, errorText(err));
  }
}

/** Asks the backend to end the tab's live subscription. The final status arrives as an event. */
export async function stopSubscription(tabId: string): Promise<void> {
  const sessionId = useWebSocketStore.getState().byTab[tabId]?.sessionId;
  if (!sessionId) return;
  try {
    await graphqlUnsubscribe(sessionId);
  } catch (err) {
    console.error('[graphql-subscription] stop failed:', err);
  }
}

/** Called when a tab is about to be discarded: ends its subscription and forgets its state. */
export function releaseGraphQlSubscriptionTab(tab: Tab): void {
  if (!isRequestTab(tab) || tab.request.requestType !== 'graphql') return;
  void stopSubscription(tab.id);
  useWebSocketStore.getState().forgetTab(tab.id);
}
```

`stopSubscription` reads the session id synchronously before its first `await`, so `releaseGraphQlSubscriptionTab` can call `forgetTab` right after it.

Create `src/lib/streaming-release.ts`:

```ts
import { releaseGraphQlSubscriptionTab } from '@/lib/graphql-subscription-session';
import { releaseWebSocketTab } from '@/lib/websocket-session';
import type { Tab } from '@/types/pane-types';

/** Ends whatever stream a tab owns (WebSocket or GraphQL subscription) when the tab goes away. */
export function releaseStreamingTab(tab: Tab): void {
  releaseWebSocketTab(tab);
  releaseGraphQlSubscriptionTab(tab);
}
```

In `src/stores/pane-store.ts`, replace Plan 09's `releaseWebSocketTab` import with `import { releaseStreamingTab } from '@/lib/streaming-release';` and change the two helpers:

```ts
function endSessionIfActive(tab: Tab): void {
  releaseStreamingTab(tab);
  // ... agent session code unchanged ...
}
```

and in `endActiveSessions` change the WebSocket loop condition to cover both protocols:

```ts
    if (
      isRequestTab(tab) &&
      (tab.request.requestType === 'websocket' || tab.request.requestType === 'graphql') &&
      !seen.has(tab.id)
    ) {
      seen.add(tab.id);
      releaseStreamingTab(tab);
    }
```

Plan 09's pane-store test spies on `releaseWebSocketTab`; it still passes because `releaseStreamingTab` calls it. Add the same kind of test for GraphQL to `src/stores/__tests__/pane-store.test.ts`:

```ts
  it('closing a graphql tab ends its subscription', async () => {
    const session = await import('@/lib/graphql-subscription-session');
    const release = vi.spyOn(session, 'releaseGraphQlSubscriptionTab').mockImplementation(() => undefined);
    const { createDefaultRequestFor } = await import('@/lib/pane-utils');

    const leaf = createDefaultLeaf();
    usePaneStore.setState({ root: leaf, activeGroupId: leaf.groupId });
    usePaneStore.getState().openTab({
      id: 'gql-tab',
      title: 'Updates',
      tabType: 'request',
      request: createDefaultRequestFor('graphql'),
      response: null,
      isDirty: false,
    });

    usePaneStore.getState().closeTab('gql-tab', leaf.groupId);

    expect(release).toHaveBeenCalledTimes(1);
    release.mockRestore();
  });
```

Create `src/components/request/GraphQlSubscriptionPanel.tsx`:

```tsx
import { Badge } from '@/components/ui/badge';
import { SingleLineEditor } from '@/components/editor';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';
import { IDLE_SESSION, useWebSocketStore } from '@/stores/websocket-store';
import type { RequestTab } from '@/types/pane-types';
import { MessageLog } from './websocket/MessageLog';

const STATUS_LABEL = {
  idle: 'Not subscribed',
  connecting: 'Connecting',
  open: 'Subscribed',
  closed: 'Ended',
  failed: 'Failed',
} as const;

interface GraphQlSubscriptionPanelProps {
  tab: RequestTab;
  onConnectionParamsChange: (text: string) => void;
}

// The response area of a GraphQL tab whose operation is a subscription: connection params, the
// subscription status and the live stream of results.
export function GraphQlSubscriptionPanel({
  tab,
  onConnectionParamsChange,
}: GraphQlSubscriptionPanelProps) {
  const session = useWebSocketStore((s) => s.byTab[tab.id]) ?? IDLE_SESSION;
  const clearLog = useWebSocketStore((s) => s.clearLog);

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex flex-wrap items-center gap-2 border-b px-3 py-2'>
        <Badge
          variant='outline'
          className={cn(
            session.status === 'open' && 'text-[hsl(var(--success))]',
            session.status === 'failed' && 'text-destructive',
          )}
        >
          {STATUS_LABEL[session.status]}
        </Badge>
        {session.subprotocol && (
          <Badge variant='secondary' className='font-mono text-[10px]'>
            {session.subprotocol}
          </Badge>
        )}
        {session.status === 'failed' && session.error && (
          <span className='text-xs text-destructive'>{session.error}</span>
        )}
        <div className='ml-auto flex min-w-[220px] flex-1 items-center gap-2 sm:max-w-md'>
          <Label className='shrink-0 text-xs text-muted-foreground'>Connection params</Label>
          <SingleLineEditor
            value={tab.request.graphql?.connectionParams ?? ''}
            onChange={onConnectionParamsChange}
            placeholder='{"authToken": "{{token}}"}'
            className='flex-1'
          />
        </div>
      </div>
      <div className='min-h-0 flex-1'>
        <MessageLog
          entries={session.log}
          onClear={() => clearLog(tab.id)}
          title='Results'
          emptyText='Subscribe to see streamed results here.'
        />
      </div>
    </div>
  );
}
```

Run: `yarn test --run graphql-subscription-session GraphQlSubscriptionPanel pane-store`
Expected: pass. If the connection-params test cannot find the field, the `SingleLineEditor` mock path must match the real import in the component (`@/components/editor`).

- [ ] **Step 8: Wire it into `RequestPanel`, the shortcut hook and `App`**

Re-read the current `src/components/request/RequestPanel.tsx` (Plan 06 changed it). Make these edits, anchored on names rather than line numbers.

1. Imports: `Radio, Square` added to the existing `lucide-react` import; `GraphQlSubscriptionPanel` from `./GraphQlSubscriptionPanel`; `useSelectedOperationKind` from `@/hooks/useSelectedOperationKind`; `startSubscription, stopSubscription` from `@/lib/graphql-subscription-session`; `IDLE_SESSION, useWebSocketStore` from `@/stores/websocket-store`.
2. Directly after Plan 06's `const isGraphQl = request.requestType === 'graphql';` add:

```tsx
  // A GraphQL tab whose selected operation is a subscription streams instead of sending once.
  const operationKind = useSelectedOperationKind(
    isGraphQl ? (request.graphql?.query ?? '') : '',
    request.graphql?.operationName,
  );
  const isSubscription = isGraphQl && operationKind === 'subscription';
  const subscription = useWebSocketStore((s) => s.byTab[tab.id]) ?? IDLE_SESSION;
  const subscribed = subscription.status === 'connecting' || subscription.status === 'open';
```

3. Turn the Send button's inline `onClick` into a named callback placed above the `urlBar` JSX. Keep the existing URL validation as it is and put the subscription branch first:

```tsx
  const handleSend = useCallback(() => {
    if (isSubscription) {
      if (subscribed) void stopSubscription(tab.id);
      else void startSubscription(tab);
      return;
    }
    // ... the existing body of the Send button's onClick: trim the URL, set urlError for an
    // empty or invalid URL, skip the format check for a URL with `{{` in it, then `send(request)`.
  }, [isSubscription, subscribed, tab, request, send]);
```

   Then the button becomes:

```tsx
        <Button
          size='sm'
          className='h-8 px-3'
          disabled={isSubscription ? subscription.status === 'connecting' : sending}
          onClick={handleSend}
        >
          {isSubscription ? (
            subscribed ? (
              <Square className='mr-1 h-3.5 w-3.5' />
            ) : (
              <Radio className='mr-1 h-3.5 w-3.5' />
            )
          ) : (
            <Send className='mr-1 h-3.5 w-3.5' />
          )}
          {isSubscription
            ? subscribed
              ? 'Stop'
              : subscription.status === 'connecting'
                ? 'Connecting...'
                : 'Subscribe'
            : sending
              ? 'Sending...'
              : 'Send'}
        </Button>
```

4. Add a listener for the keyboard shortcut, next to the other `window.addEventListener` effects in the component:

```tsx
  // Ctrl or Cmd+Enter on a GraphQL tab goes through the same decision as the Send button.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId === tab.id) handleSend();
    };
    window.addEventListener('rocket:graphql-send', handler);
    return () => window.removeEventListener('rocket:graphql-send', handler);
  }, [tab.id, handleSend]);
```

5. Make `responseArea` show the subscription panel. Put this branch first in the existing conditional chain (before `sending ? ... : response ? ... : ...`), so a subscription tab, or a GraphQL tab that still has a log from one, shows the stream:

```tsx
  const showSubscription = isGraphQl && (isSubscription || subscription.log.length > 0);
  const responseArea = showSubscription ? (
    <GraphQlSubscriptionPanel
      tab={tab}
      onConnectionParamsChange={(connectionParams) =>
        updateRequest(tab.id, {
          graphql: { ...(request.graphql ?? { query: '', variables: '' }), connectionParams },
        })
      }
    />
  ) : sending ? (
    /* ... existing branches unchanged ... */
```

   (`updateRequest` is the store action `RequestPanel` already uses, and `handleGraphQlChange` from Plan 06 patches the same `graphql` slice; do not reuse it here, it would mark the tab dirty twice with the same effect, which is harmless but noisy. `connectionParams` is session state and is not part of the save mapper, so it never reaches the file.)

In `src/hooks/useKeyboardShortcuts.ts`, in the Ctrl or Cmd+Enter branch Plan 09 already changed, add the GraphQL case before the HTTP send:

```ts
          } else if (tab.request.requestType === 'graphql') {
            // A GraphQL tab decides between Send and Subscribe itself.
            window.dispatchEvent(
              new CustomEvent('rocket:graphql-send', { detail: { tabId: tab.id } }),
            );
          } else {
            sendRequest(tab.id, tab.request);
          }
```

In `src/App.tsx`, import `useGraphQlSubscriptionEventBridge` from `@/lib/graphql-subscription-event-bridge` and call it right after `useWebSocketEventBridge();`.

- [ ] **Step 9: Verify**

Run, in order:
- `yarn test --run graphql-subscription websocket-store GraphQlSubscriptionPanel useSelectedOperationKind pane-store RequestPanel`
- `yarn tsc --noEmit`
- `yarn check`

Expected: green. Manual check in the real app (`yarn tauri dev`) against a real subscription endpoint (for example a local `graphql-ws` or `graphql-transport-ws` demo server, or a public GraphQL subscription playground endpoint the user has access to):
1. Create a GraphQL request, write `subscription { ... }`, confirm the Send button reads Subscribe and the response area shows "Subscribe to see streamed results here."
2. Subscribe: the badge goes Connecting then Subscribed, each result appears as a `next` line with a time and size, and the dialect badge shows the selected subprotocol.
3. Stop: the badge goes to Ended, and the server logs the stop or complete message.
4. Close the tab while subscribed: the server sees the socket close (`ss -tnp | grep rocket` shows no lingering connection).
5. Ctrl or Cmd+Enter on a subscription tab subscribes (and on a query tab still sends).
6. A query operation in the same tab (change `subscription` to `query`) switches back to Send and the normal response area.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path: `src/lib/tauri-api.ts`, `src/types/pane-types.ts`, `src/stores/websocket-store.ts`, `src/lib/websocket-session.ts`, `src/lib/graphql-subscription-session.ts`, `src/lib/graphql-subscription-event-bridge.ts`, `src/lib/streaming-release.ts`, `src/hooks/useSelectedOperationKind.ts`, `src/hooks/useKeyboardShortcuts.ts`, `src/components/request/GraphQlSubscriptionPanel.tsx`, `src/components/request/RequestPanel.tsx`, `src/stores/pane-store.ts`, `src/stores/__tests__/pane-store.test.ts`, `src/App.tsx`, and the five new test files named under Files.
Suggested subject: `feat(graphql): Subscribe and Stop with a live results log`.

---

## Known limits (state them in the PR description)

- No `wss://` handshake is exercised in tests (Plan 08's limit); the protocol tests use plain `ws://`. Check a real `wss://` subscription endpoint by hand.
- One operation per connection, id `"1"`, and one subscription per tab. A document with several operations must name one.
- Subscriptions over Server-Sent Events (`graphql-sse`, multipart HTTP) are not supported; WebSocket only.
- `connection_init` params and the optional subscription URL are not persisted. The subscription URL has no editor in this plan (the IPC field exists; the tab always uses the request URL).
- Auth is limited to what Plan 08 supports on a handshake (Basic, Bearer, API key). OAuth 2 and the rest return an explicit error.
- Results are not written to History and are not asserted on by tests or the Collection Runner.
- A server that selects no subprotocol is assumed to speak `graphql-transport-ws`.
- Reconnect and resubscribe on a dropped connection are not automatic.

## Next Plan

[2026-10-05-protocol-parity-plan-11-grpc-model-and-proto.md](2026-10-05-protocol-parity-plan-11-grpc-model-and-proto.md): the gRPC model, persistence and `.proto` import. Chain to it automatically when this plan finishes.
