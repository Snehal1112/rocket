//! GraphQL subscriptions over WebSocket. The socket comes from the `WebSocketClient` port; this
//! module owns the protocol and the session. See `protocol` for the message formats.

pub mod protocol;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
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
use crate::graphql_request::resolve_json_text;
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

/// Resolves the `{{placeholders}}` of JSON text. A value inside a JSON string is escaped, so a
/// quote, backslash or newline in a variable or secret cannot break the payload.
async fn resolved_json(
    exec: &RequestExecutionService,
    scope: &WebSocketScope,
    text: &str,
) -> DomainResult<String> {
    let mut values: HashMap<String, String> = HashMap::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) if !after[..end].contains('}') => {
                let placeholder = format!("{{{{{}}}}}", &after[..end]);
                if !values.contains_key(&placeholder) {
                    let value = resolved_text(exec, scope, &placeholder).await?;
                    values.insert(placeholder, value);
                }
                rest = &after[end + 2..];
            }
            _ => rest = after,
        }
    }
    Ok(resolve_json_text(text, |placeholder| {
        values
            .get(placeholder)
            .cloned()
            .unwrap_or_else(|| placeholder.to_string())
    }))
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
    let resolved = resolved_json(exec, scope, text).await?;
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

/// A session slot. `Connecting` reserves the id while the handshake is in flight. The number is
/// the generation: it tells apart two sessions that reuse one id, so a stale task never acts on
/// the newer one.
enum Slot {
    Connecting(u64),
    Running(u64, oneshot::Sender<()>),
}

impl Slot {
    fn generation(&self) -> u64 {
        match self {
            Self::Connecting(generation) | Self::Running(generation, _) => *generation,
        }
    }
}

pub struct GraphQlSubscriptionService {
    client: Arc<dyn WebSocketClient>,
    events: Arc<dyn EventPublisher>,
    sessions: Arc<Mutex<HashMap<String, Slot>>>,
    next_generation: AtomicU64,
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

/// Removes the slot only if it still belongs to `generation`. Returns whether it did.
fn release(sessions: &Mutex<HashMap<String, Slot>>, session_id: &str, generation: u64) -> bool {
    let mut sessions = sessions.lock().unwrap_or_else(|e| e.into_inner());
    match sessions.get(session_id) {
        Some(slot) if slot.generation() == generation => {
            sessions.remove(session_id);
            true
        }
        _ => false,
    }
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
                            reason: None,
                        },
                        Some(Closing::Failed(reason)) => Finish {
                            clean: false,
                            reason: Some(reason),
                        },
                        // The server ended it without `complete`. Only a normal closure is clean;
                        // `graphql-transport-ws` rejects with 4xxx codes (4401, 4403, ...).
                        None => {
                            let normal = matches!(close.code, Some(1000) | Some(1001));
                            let mut reason = match close.code {
                                Some(code) => format!("the server closed the connection ({code})"),
                                None => "the server closed the connection".to_string(),
                            };
                            if !close.reason.is_empty() {
                                reason.push_str(": ");
                                reason.push_str(&close.reason);
                            }
                            Finish {
                                clean: normal && close.clean,
                                reason: if normal { None } else { Some(reason) },
                            }
                        }
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
            next_generation: AtomicU64::new(1),
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
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        {
            let mut sessions = self.lock();
            if sessions.contains_key(session_id) {
                return Err(DomainError::AlreadyExists(format!(
                    "GraphQL subscription session '{session_id}'"
                )));
            }
            sessions.insert(session_id.to_string(), Slot::Connecting(generation));
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
                // A cancelled connect must not touch an id that a newer session now holds.
                if release(&self.sessions, session_id, generation) {
                    publish_status(
                        self.events.as_ref(),
                        session_id,
                        WebSocketSessionState::Failed,
                        None,
                        Some(error.to_string()),
                    );
                }
                return Err(error);
            }
        };

        let (stop_tx, stop_rx) = oneshot::channel();
        // Decide under the lock, act with it released, so the guard is never held across an await.
        let registered = {
            let mut sessions = self.lock();
            match sessions.get_mut(session_id) {
                Some(slot) if slot.generation() == generation && matches!(slot, Slot::Connecting(_)) => {
                    *slot = Slot::Running(generation, stop_tx);
                    true
                }
                _ => false,
            }
        };
        if !registered {
            // Cancelled while connecting: do not leave the new socket open.
            request_close(&handle, "cancelled").await;
            return Err(DomainError::Conflict("subscription was cancelled".into()));
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
            // Only free the id if it still belongs to this session.
            release(&sessions, &id, generation);
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
        if let Some(Slot::Running(_, stop)) = slot {
            // The session may already have ended; then there is nothing to stop.
            let _ = stop.send(());
        }
        Ok(())
    }

    /// Stops every subscription. Used on app exit.
    pub async fn end_all(&self) {
        let drained: Vec<Slot> = self.lock().drain().map(|(_, slot)| slot).collect();
        for slot in drained {
            if let Slot::Running(_, stop) = slot {
                let _ = stop.send(());
            }
        }
    }
}

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
        env.set_variable(Variable::new("tricky", "O\"Brien\\ \nline"));
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
    async fn resolved_values_are_escaped_inside_json_strings() {
        let tricky = "O\"Brien\\ \nline";
        let start = resolve_graphql_subscription(
            &service(),
            input(serde_json::json!({
                "variables": "{\"who\": \"{{tricky}}\", \"n\": {{count}}}".replace("{{count}}", "3"),
                "connectionParams": "{\"token\": \"{{tricky}}\"}"
            })),
        )
        .await
        .expect("a value holding quotes, backslashes or newlines must not break the JSON");

        assert_eq!(start.variables, Some(serde_json::json!({ "who": tricky, "n": 3 })));
        assert_eq!(start.connection_params, Some(serde_json::json!({ "token": tricky })));
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

#[cfg(test)]
mod session_tests {
    use super::*;
    use crate::test_doubles::RecordingPublisher;
    use futures_util::{SinkExt, StreamExt};
    use rocket_infra::TungsteniteWebSocketClient;
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
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
        /// Reject the connection with a 4403 close code instead of acknowledging it.
        Reject,
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
                        if matches!(script, Script::Reject) {
                            let frame = CloseFrame {
                                code: CloseCode::Library(4403),
                                reason: "Forbidden".into(),
                            };
                            let _ = ws.send(Message::Close(Some(frame))).await;
                        } else if !matches!(script, Script::NoAck) {
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
                        Script::NoAck | Script::Reject => {}
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
    async fn a_close_code_rejection_ends_as_failed_with_the_code() {
        let (port, _received) = spawn_server(Script::Reject, Offer::Modern).await;
        let (svc, publisher) = service();

        svc.start("s1", start_for(port)).await.expect("start");
        wait_for(&publisher, "terminal status", terminal).await;

        let events = publisher.events();
        assert_eq!(states(&events).last(), Some(&WebSocketSessionState::Failed));
        let reason = events.iter().find_map(|e| match e {
            DomainEvent::GraphQlSubscriptionStatus { state: WebSocketSessionState::Failed, reason, .. } => reason.clone(),
            _ => None,
        });
        let reason = reason.unwrap_or_default();
        assert!(reason.contains("4403") && reason.contains("Forbidden"), "{reason}");
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

    #[tokio::test]
    async fn a_finished_session_cannot_free_an_id_a_newer_one_reused() {
        let (port_a, _received_a) = spawn_server(Script::Hold, Offer::Modern).await;
        let (port_b, received_b) = spawn_server(Script::Hold, Offer::Modern).await;
        let (svc, publisher) = service();
        svc.start("s1", start_for(port_a)).await.expect("first");
        wait_for(&publisher, "the first result", |e| !results(e).is_empty()).await;

        // `start` reserves the id before its first await, so the first session's task has not
        // finished yet when the id is reused.
        svc.stop("s1").await.expect("stop first");
        svc.start("s1", start_for(port_b)).await.expect("reuse the id");
        wait_for(&publisher, "the first session's end", terminal).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(svc.session_count(), 1, "the new session must still be registered");
        svc.stop("s1").await.expect("stop second");
        for _ in 0..300 {
            if types(&received_b).contains(&"complete".to_string()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the second session never received its stop: {:?}", types(&received_b));
    }
}
