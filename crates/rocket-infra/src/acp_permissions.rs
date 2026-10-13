//! The pending permission requests of one agent session. A request is
//! registered when the agent asks, and answered once: by the user's choice,
//! or as cancelled when the turn is stopped or ends.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::oneshot;

struct Pending {
    option_ids: Vec<String>,
    answer: oneshot::Sender<Option<String>>,
}

#[derive(Default)]
pub struct PermissionBroker {
    next_id: AtomicU64,
    pending: Mutex<HashMap<String, Pending>>,
}

impl PermissionBroker {
    /// Registers a request offering `option_ids`. Returns its id and the
    /// receiver that resolves with the chosen option id, or `None` when the
    /// request was cancelled.
    pub fn register(&self, option_ids: Vec<String>) -> (String, oneshot::Receiver<Option<String>>) {
        let request_id = format!("perm-{}", self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let (answer, receiver) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(request_id.clone(), Pending { option_ids, answer });
        (request_id, receiver)
    }

    /// Answers one request. Fails, leaving the request pending, when the
    /// option was not offered. Fails when the request is unknown or was
    /// already answered.
    pub fn resolve(&self, request_id: &str, option_id: Option<&str>) -> DomainResult<()> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(entry) = pending.get(request_id) else {
            return Err(DomainError::NotFound(format!(
                "permission request '{request_id}'"
            )));
        };
        if let Some(option_id) = option_id {
            if !entry.option_ids.iter().any(|id| id == option_id) {
                return Err(DomainError::InvalidInput(format!(
                    "option '{option_id}' was not offered for this request"
                )));
            }
        }
        if let Some(entry) = pending.remove(request_id) {
            // A dropped receiver means the waiting task is gone, which only
            // happens when the session ended. Nothing is left to answer.
            let _ = entry.answer.send(option_id.map(str::to_string));
        }
        Ok(())
    }

    /// Answers every pending request as cancelled.
    pub fn drain_cancelled(&self) {
        let drained: Vec<Pending> = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        for entry in drained {
            let _ = entry.answer.send(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Vec<String> {
        vec!["allow".to_string(), "reject".to_string()]
    }

    #[tokio::test]
    async fn resolve_delivers_the_chosen_option() {
        let broker = PermissionBroker::default();
        let (id, rx) = broker.register(options());
        broker.resolve(&id, Some("allow")).expect("resolve");
        assert_eq!(rx.await.expect("answer"), Some("allow".to_string()));
    }

    #[tokio::test]
    async fn requests_get_distinct_ids() {
        let broker = PermissionBroker::default();
        let (first, _rx1) = broker.register(options());
        let (second, _rx2) = broker.register(options());
        assert_ne!(first, second);
    }

    #[test]
    fn an_unknown_request_is_not_found() {
        let broker = PermissionBroker::default();
        let err = broker
            .resolve("perm-99", Some("allow"))
            .expect_err("unknown");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn a_request_can_be_answered_only_once() {
        let broker = PermissionBroker::default();
        let (id, _rx) = broker.register(options());
        broker.resolve(&id, Some("allow")).expect("first answer");
        let err = broker
            .resolve(&id, Some("reject"))
            .expect_err("second answer");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn an_option_that_was_not_offered_is_refused_and_the_request_stays_pending() {
        let broker = PermissionBroker::default();
        let (id, rx) = broker.register(options());
        let err = broker
            .resolve(&id, Some("allow-forever"))
            .expect_err("not offered");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        broker
            .resolve(&id, Some("reject"))
            .expect("still answerable");
        assert_eq!(rx.await.expect("answer"), Some("reject".to_string()));
    }

    #[tokio::test]
    async fn none_cancels_a_request() {
        let broker = PermissionBroker::default();
        let (id, rx) = broker.register(options());
        broker.resolve(&id, None).expect("cancel");
        assert_eq!(rx.await.expect("answer"), None);
    }

    #[tokio::test]
    async fn drain_cancels_every_pending_request() {
        let broker = PermissionBroker::default();
        let (a, rx_a) = broker.register(options());
        let (_b, rx_b) = broker.register(options());
        broker.drain_cancelled();
        assert_eq!(rx_a.await.expect("answer a"), None);
        assert_eq!(rx_b.await.expect("answer b"), None);
        // A drained request cannot be answered afterwards.
        let err = broker.resolve(&a, Some("allow")).expect_err("drained");
        assert!(matches!(err, DomainError::NotFound(_)));
    }
}
