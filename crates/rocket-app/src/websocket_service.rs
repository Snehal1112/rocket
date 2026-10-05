//! WebSocket session registry. A session id is chosen by the caller (the frontend), because the
//! pump publishes events before `connect` returns and the frontend has to know which id to route.
//! Inbound frames and lifecycle changes leave as `DomainEvent`s, never as return values.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// A session slot. `tx` is `None` while the id is reserved and the handshake is in flight.
/// `generation` tells apart two sessions that reuse one id, so a stale task never acts on
/// the newer session.
struct Slot {
    generation: u64,
    tx: Option<mpsc::Sender<WebSocketCommand>>,
}

pub struct WebSocketService {
    client: Arc<dyn WebSocketClient>,
    events: Arc<dyn EventPublisher>,
    sessions: Arc<Mutex<HashMap<String, Slot>>>,
    next_generation: AtomicU64,
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
            next_generation: AtomicU64::new(1),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Slot>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Number of sessions, including ones still connecting.
    pub fn session_count(&self) -> usize {
        self.lock().len()
    }

    /// Removes the slot only if it still belongs to `generation`. Returns whether it did.
    fn release(&self, session_id: &str, generation: u64) -> bool {
        let mut sessions = self.lock();
        match sessions.get(session_id) {
            Some(slot) if slot.generation == generation => {
                sessions.remove(session_id);
                true
            }
            _ => false,
        }
    }

    /// Opens a session. Publishes `Connecting`, then `Open` (with the negotiated subprotocol) or
    /// `Failed`. A `disconnect` that arrives while the handshake is in flight cancels it: the
    /// late socket is closed and this returns `Conflict`.
    pub async fn connect(
        &self,
        session_id: &str,
        request: WebSocketConnectRequest,
    ) -> DomainResult<()> {
        self.connect_with(session_id, async move { Ok(request) })
            .await
    }

    /// Like `connect`, but the id is reserved first and the request is produced afterwards.
    /// Resolving variables and secrets can take a while, and a `disconnect` during that time
    /// cancels the connect before any socket is opened.
    pub async fn connect_with(
        &self,
        session_id: &str,
        resolve: impl Future<Output = DomainResult<WebSocketConnectRequest>>,
    ) -> DomainResult<()> {
        if session_id.trim().is_empty() {
            return Err(DomainError::InvalidInput("session id is required".into()));
        }
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        {
            let mut sessions = self.lock();
            if sessions.contains_key(session_id) {
                return Err(DomainError::AlreadyExists(format!(
                    "WebSocket session '{session_id}'"
                )));
            }
            sessions.insert(
                session_id.to_string(),
                Slot {
                    generation,
                    tx: None,
                },
            );
        }
        publish_status(
            self.events.as_ref(),
            session_id,
            WebSocketSessionState::Connecting,
            None,
            None,
        );

        let request = match resolve.await {
            Ok(request) => request,
            Err(error) => return Err(self.fail_connect(session_id, generation, error)),
        };
        // A disconnect while resolving removed the slot: do not open a socket nobody wants.
        if !self.owns(session_id, generation) {
            return Err(self.cancelled(session_id));
        }

        let WebSocketHandle {
            subprotocol,
            outbound,
            events: inbound,
        } = match self.client.connect(request).await {
            Ok(handle) => handle,
            Err(error) => return Err(self.fail_connect(session_id, generation, error)),
        };

        // Take the slot under the lock, then act on the result with the lock released, so the
        // guard is never held across an await.
        let registered = {
            let mut sessions = self.lock();
            match sessions.get_mut(session_id) {
                Some(slot) if slot.generation == generation && slot.tx.is_none() => {
                    slot.tx = Some(outbound.clone());
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
            return Err(self.cancelled(session_id));
        }
        publish_status(
            self.events.as_ref(),
            session_id,
            WebSocketSessionState::Open,
            subprotocol,
            None,
        );
        self.spawn_pump(session_id.to_string(), generation, inbound);
        Ok(())
    }

    fn owns(&self, session_id: &str, generation: u64) -> bool {
        self.lock()
            .get(session_id)
            .is_some_and(|slot| slot.generation == generation)
    }

    /// A connect that was cancelled by `disconnect`. Publishes its one terminal status.
    fn cancelled(&self, session_id: &str) -> DomainError {
        let close = WebSocketClose {
            code: None,
            reason: "cancelled".into(),
            clean: true,
        };
        publish_status(
            self.events.as_ref(),
            session_id,
            WebSocketSessionState::Closed,
            None,
            Some(&close),
        );
        DomainError::Conflict("connection was cancelled".into())
    }

    /// A connect that failed. If it was already cancelled, the cancel is what gets reported and
    /// the reservation (which may now belong to a newer connect) is left alone.
    fn fail_connect(&self, session_id: &str, generation: u64, error: DomainError) -> DomainError {
        if !self.release(session_id, generation) {
            return self.cancelled(session_id);
        }
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
        error
    }

    /// Forwards inbound events as `DomainEvent`s, then publishes exactly one terminal status and
    /// frees the session id.
    fn spawn_pump(
        &self,
        session_id: String,
        generation: u64,
        mut inbound: mpsc::Receiver<WebSocketEvent>,
    ) {
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
            // Only free the id if it still belongs to this session. After a disconnect the id may
            // already be in use by a newer one.
            {
                let mut sessions = sessions.lock().unwrap_or_else(|e| e.into_inner());
                if sessions
                    .get(&session_id)
                    .is_some_and(|slot| slot.generation == generation)
                {
                    sessions.remove(&session_id);
                }
            }
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
                Some(Slot {
                    tx: Some(sender), ..
                }) => sender.clone(),
                Some(Slot { tx: None, .. }) => {
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
        if let Some(Slot {
            tx: Some(sender), ..
        }) = slot
        {
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
        for slot in drained.into_iter().filter_map(|slot| slot.tx) {
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
        fail_first: Mutex<Option<String>>,
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
            if let Some(message) = self.fail_first.lock().expect("lock").take() {
                return Err(DomainError::Http(message));
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

    /// Spawns a connect and waits until it has reserved its id.
    async fn start_gated_connect(
        svc: &Arc<WebSocketService>,
        id: &'static str,
        expected_count: usize,
    ) -> tokio::task::JoinHandle<DomainResult<()>> {
        let handle = {
            let svc = Arc::clone(svc);
            tokio::spawn(async move { svc.connect(id, request()).await })
        };
        for _ in 0..200 {
            if svc.session_count() == expected_count {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        handle
    }

    #[tokio::test]
    async fn the_old_pump_cannot_free_a_session_that_reused_its_id() {
        let client = Arc::new(FakeClient::default());
        let (svc, _publisher) = service(&client);
        svc.connect("s1", request()).await.expect("first");
        let old = client.take();
        svc.disconnect("s1").await.expect("disconnect");
        svc.connect("s1", request())
            .await
            .expect("reconnect with the same id");
        let mut fresh = client.take();

        // The old socket's close finally arrives.
        old.events
            .send(WebSocketEvent::Closed(WebSocketClose {
                code: Some(1000),
                reason: String::new(),
                clean: true,
            }))
            .await
            .expect("old close");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(
            svc.session_count(),
            1,
            "the new session must still be registered"
        );
        svc.send("s1", WebSocketFrame::Text("still here".into()))
            .await
            .expect("the new session still works");
        assert!(fresh.commands.recv().await.is_some());
    }

    #[tokio::test]
    async fn a_cancelled_connect_cannot_take_over_a_reconnect() {
        let gate = Arc::new(Notify::new());
        let client = Arc::new(FakeClient {
            gate: Some(Arc::clone(&gate)),
            ..Default::default()
        });
        let (svc, _publisher) = service(&client);
        let svc = Arc::new(svc);

        let first = start_gated_connect(&svc, "s1", 1).await;
        svc.disconnect("s1").await.expect("cancel the first");
        let second = start_gated_connect(&svc, "s1", 1).await;
        // Release the first handshake, then the second.
        gate.notify_one();
        let first_result = first.await.expect("join first");
        gate.notify_one();
        let second_result = second.await.expect("join second");

        assert!(
            matches!(first_result, Err(DomainError::Conflict(_))),
            "the cancelled connect must not succeed: {first_result:?}"
        );
        assert!(
            second_result.is_ok(),
            "the reconnect must win: {second_result:?}"
        );
        assert_eq!(svc.session_count(), 1);
    }

    #[tokio::test]
    async fn a_failed_cancelled_connect_keeps_the_reconnects_reservation() {
        let gate = Arc::new(Notify::new());
        let client = Arc::new(FakeClient {
            gate: Some(Arc::clone(&gate)),
            fail_first: Mutex::new(Some("refused".into())),
            ..Default::default()
        });
        let (svc, _publisher) = service(&client);
        let svc = Arc::new(svc);

        let first = start_gated_connect(&svc, "s1", 1).await;
        svc.disconnect("s1").await.expect("cancel the first");
        let second = start_gated_connect(&svc, "s1", 1).await;
        gate.notify_one();
        let first_result = first.await.expect("join first");
        assert!(first_result.is_err());
        gate.notify_one();
        let second_result = second.await.expect("join second");

        assert!(
            second_result.is_ok(),
            "the reconnect must win: {second_result:?}"
        );
        assert_eq!(svc.session_count(), 1);
    }

    #[tokio::test]
    async fn disconnect_during_resolution_never_opens_a_socket() {
        let client = Arc::new(FakeClient::default());
        let (svc, publisher) = service(&client);
        let svc = Arc::new(svc);
        let release = Arc::new(Notify::new());

        let connecting = {
            let svc = Arc::clone(&svc);
            let release = Arc::clone(&release);
            tokio::spawn(async move {
                svc.connect_with("s1", async move {
                    release.notified().await;
                    Ok(request())
                })
                .await
            })
        };
        for _ in 0..200 {
            if svc.session_count() == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(svc.session_count(), 1, "the id is reserved while resolving");

        svc.disconnect("s1").await.expect("cancel");
        release.notify_one();
        let result = connecting.await.expect("join");

        assert!(
            matches!(result, Err(DomainError::Conflict(_))),
            "{result:?}"
        );
        assert!(
            client.endpoints.lock().expect("lock").is_empty(),
            "no socket may be opened for a cancelled connect"
        );
        assert_eq!(svc.session_count(), 0);
        assert!(statuses(&publisher.events()).contains(&WebSocketSessionState::Closed));
    }

    #[tokio::test]
    async fn a_failed_resolution_publishes_failed_and_frees_the_id() {
        let client = Arc::new(FakeClient::default());
        let (svc, publisher) = service(&client);

        let err = svc
            .connect_with("s1", async {
                Err(DomainError::InvalidInput(
                    "Digest auth is not supported".into(),
                ))
            })
            .await
            .expect_err("resolution failed");

        assert!(err.to_string().contains("Digest"), "{err}");
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
