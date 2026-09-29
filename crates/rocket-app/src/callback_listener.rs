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
            err.to_string()
                .contains("callback listener is not configured"),
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
