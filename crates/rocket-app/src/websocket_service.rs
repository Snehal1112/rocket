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
        publish_status(
            self.events.as_ref(),
            session_id,
            WebSocketSessionState::Connecting,
            None,
            None,
        );

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

        // Take the slot under the lock, then act on the result with the lock released, so the
        // guard is never held across an await.
        let registered = {
            let mut sessions = self.lock();
            match sessions.get_mut(session_id) {
                Some(slot @ None) => {
                    *slot = Some(outbound.clone());
                    true
                }
                _ => false,
            }
        };
        if !registered {
            // Cancelled while connecting: do not leave the new socket open.
            let _ = outbound
                .send(WebSocketCommand::Close {
                    code: 1000,
                    reason: "cancelled".into(),
                })
                .await;
            return Err(DomainError::Conflict("connection was cancelled".into()));
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
                    return Err(DomainError::NotFound(format!(
                        "WebSocket session '{session_id}'"
                    )))
                }
            }
        };
        sender
            .send(WebSocketCommand::Send(frame.clone()))
            .await
            .map_err(|_| DomainError::Conflict("session is closed".into()))?;
        publish_frame(
            self.events.as_ref(),
            session_id,
            WebSocketDirection::Out,
            &frame,
        );
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
                .send(WebSocketCommand::Close {
                    code: 1000,
                    reason: "client disconnect".into(),
                })
                .await;
        }
        Ok(())
    }

    /// Closes every session. Used on app exit.
    pub async fn end_all_sessions(&self) {
        let drained: Vec<Slot> = self.lock().drain().map(|(_, slot)| slot).collect();
        for slot in drained.into_iter().flatten() {
            let _ = slot
                .send(WebSocketCommand::Close {
                    code: 1001,
                    reason: "app exit".into(),
                })
                .await;
        }
    }
}

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
        async fn connect(
            &self,
            _request: WebSocketConnectRequest,
        ) -> DomainResult<WebSocketHandle> {
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
        (
            WebSocketService::new(client.clone(), publisher.clone()),
            publisher,
        )
    }

    async fn wait_for(
        publisher: &RecordingPublisher,
        what: &str,
        pred: impl Fn(&[DomainEvent]) -> bool,
    ) {
        for _ in 0..400 {
            if pred(&publisher.events()) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!(
            "timed out waiting for {what}; events: {:?}",
            publisher.events()
        );
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
        let client = Arc::new(FakeClient {
            subprotocol: Some("graphql-transport-ws".into()),
            ..Default::default()
        });
        let (svc, publisher) = service(&client);

        svc.connect("s1", request()).await.expect("connect");

        let events = publisher.events();
        assert_eq!(
            statuses(&events),
            vec![
                WebSocketSessionState::Connecting,
                WebSocketSessionState::Open
            ]
        );
        match &events[1] {
            DomainEvent::WebSocketStatus {
                session_id,
                subprotocol,
                ..
            } => {
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

        endpoints
            .events
            .send(WebSocketEvent::Frame(WebSocketFrame::Text("one".into())))
            .await
            .expect("send");
        endpoints
            .events
            .send(WebSocketEvent::Frame(WebSocketFrame::Binary(vec![1, 2, 3])))
            .await
            .expect("send");
        wait_for(&publisher, "two messages", |e| {
            e.iter()
                .filter(|x| matches!(x, DomainEvent::WebSocketMessage { .. }))
                .count()
                == 2
        })
        .await;

        let messages: Vec<_> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::WebSocketMessage {
                    direction,
                    kind,
                    data,
                    size,
                    ..
                } => Some((direction, kind, data, size)),
                _ => None,
            })
            .collect();
        assert_eq!(
            messages[0],
            (
                WebSocketDirection::In,
                WebSocketPayloadKind::Text,
                "one".to_string(),
                3
            )
        );
        assert_eq!(
            messages[1],
            (
                WebSocketDirection::In,
                WebSocketPayloadKind::Binary,
                "AQID".to_string(),
                3
            )
        );
    }

    #[tokio::test]
    async fn send_enqueues_the_frame_and_publishes_an_out_message() {
        let client = Arc::new(FakeClient::default());
        let (svc, publisher) = service(&client);
        svc.connect("s1", request()).await.expect("connect");
        let mut endpoints = client.take();

        svc.send("s1", WebSocketFrame::Text("hello".into()))
            .await
            .expect("send");

        assert_eq!(
            endpoints.commands.recv().await,
            Some(WebSocketCommand::Send(WebSocketFrame::Text("hello".into())))
        );
        assert!(publisher.events().iter().any(|e| matches!(
            e,
            DomainEvent::WebSocketMessage { direction: WebSocketDirection::Out, data, .. } if data == "hello"
        )));

        let err = svc
            .send("nope", WebSocketFrame::Text("x".into()))
            .await
            .expect_err("unknown session");
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
            .send(WebSocketEvent::Closed(WebSocketClose {
                code: Some(1000),
                reason: "bye".into(),
                clean: true,
            }))
            .await
            .expect("close");
        wait_for(&publisher, "terminal status", |e| {
            statuses(e).contains(&WebSocketSessionState::Closed)
        })
        .await;

        assert_eq!(svc.session_count(), 0);
        let terminal = statuses(&publisher.events())
            .into_iter()
            .filter(|s| {
                matches!(
                    s,
                    WebSocketSessionState::Closed | WebSocketSessionState::Failed
                )
            })
            .count();
        assert_eq!(terminal, 1, "exactly one terminal status");
        assert!(svc
            .send("s1", WebSocketFrame::Text("late".into()))
            .await
            .is_err());
        // The id can be reused.
        svc.connect("s1", request())
            .await
            .expect("reconnect with the same id");
    }

    #[tokio::test]
    async fn an_event_stream_that_ends_without_closed_is_reported_as_failed() {
        let client = Arc::new(FakeClient::default());
        let (svc, publisher) = service(&client);
        svc.connect("s1", request()).await.expect("connect");
        let endpoints = client.take();

        drop(endpoints.events); // the pump died without a Closed event
        wait_for(&publisher, "failed status", |e| {
            statuses(e).contains(&WebSocketSessionState::Failed)
        })
        .await;
        assert_eq!(svc.session_count(), 0);
    }

    #[tokio::test]
    async fn a_failed_connect_publishes_failed_returns_the_error_and_frees_the_id() {
        let client = Arc::new(FakeClient {
            fail_with: Some("handshake rejected with HTTP 401".into()),
            ..Default::default()
        });
        let (svc, publisher) = service(&client);

        let err = svc.connect("s1", request()).await.expect_err("must fail");

        assert!(err.to_string().contains("401"), "{err}");
        assert_eq!(
            statuses(&publisher.events()),
            vec![
                WebSocketSessionState::Connecting,
                WebSocketSessionState::Failed
            ]
        );
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
        svc.disconnect("s1")
            .await
            .expect("second disconnect is a no-op");
        svc.disconnect("never-existed")
            .await
            .expect("unknown id is a no-op");
    }

    #[tokio::test]
    async fn disconnect_during_connect_cancels_and_closes_the_late_socket() {
        let gate = Arc::new(Notify::new());
        let client = Arc::new(FakeClient {
            gate: Some(Arc::clone(&gate)),
            ..Default::default()
        });
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
        assert_eq!(
            svc.session_count(),
            1,
            "the id is reserved while connecting"
        );

        svc.disconnect("s1").await.expect("cancel");
        gate.notify_one();

        let result = connecting.await.expect("join");
        assert!(
            matches!(result, Err(DomainError::Conflict(_))),
            "{result:?}"
        );
        assert_eq!(svc.session_count(), 0);
        let mut endpoints = client.take();
        assert!(
            matches!(
                endpoints.commands.recv().await,
                Some(WebSocketCommand::Close { .. })
            ),
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

        assert!(matches!(
            first.commands.recv().await,
            Some(WebSocketCommand::Close { .. })
        ));
        assert!(matches!(
            second.commands.recv().await,
            Some(WebSocketCommand::Close { .. })
        ));
        assert_eq!(svc.session_count(), 0);
    }
}
