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
