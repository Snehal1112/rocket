//! Per-run cancel signal for Flow runs. `FlowExecutionService::cancel`
//! triggers it, and a node that waits listens to it, so Stop ends the wait
//! at once instead of after the node.

use std::time::Duration;

use tokio::sync::watch;

/// Returned by `CancelSignal::sleep` when the run was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cancelled;

/// Owner side of a run's cancel signal. `FlowExecutionService` holds one per
/// in-flight run.
pub(crate) struct CancelHandle {
    tx: watch::Sender<bool>,
}

/// Node side of a run's cancel signal. Cheap to clone.
#[derive(Clone)]
pub(crate) struct CancelSignal {
    rx: watch::Receiver<bool>,
}

/// Creates a connected handle and signal for one run.
pub(crate) fn cancel_pair() -> (CancelHandle, CancelSignal) {
    let (tx, rx) = watch::channel(false);
    (CancelHandle { tx }, CancelSignal { rx })
}

impl CancelHandle {
    /// Marks the run as cancelled. Every clone of the signal sees it.
    pub(crate) fn cancel(&self) {
        // `send_replace` stores the value even when no signal is listening.
        self.tx.send_replace(true);
    }
}

// Waiting nodes (plans 04 and 08) call `cancelled` and `sleep`. Until then
// only the tests do, so the release build would warn.
#[cfg_attr(not(test), allow(dead_code))]
impl CancelSignal {
    pub(crate) fn is_cancelled(&self) -> bool {
        *self.rx.borrow()
    }

    /// Resolves when the run is cancelled. It never resolves otherwise.
    pub(crate) async fn cancelled(&mut self) {
        // `wait_for` fails only when the handle is gone. The run is then
        // over without a cancel, so this waits forever.
        if self.rx.wait_for(|cancelled| *cancelled).await.is_err() {
            std::future::pending::<()>().await;
        }
    }

    /// Sleeps for `dur`, or returns `Err(Cancelled)` as soon as the run is
    /// cancelled.
    pub(crate) async fn sleep(&mut self, dur: Duration) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            return Err(Cancelled);
        }
        tokio::select! {
            _ = tokio::time::sleep(dur) => Ok(()),
            _ = self.cancelled() => Err(Cancelled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_signal_is_not_cancelled() {
        let (_handle, signal) = cancel_pair();
        assert!(!signal.is_cancelled());
    }

    #[test]
    fn cancel_is_seen_by_every_clone() {
        let (handle, signal) = cancel_pair();
        let clone = signal.clone();
        handle.cancel();
        assert!(signal.is_cancelled());
        assert!(clone.is_cancelled());
    }

    #[tokio::test]
    async fn sleep_finishes_when_not_cancelled() {
        let (_handle, mut signal) = cancel_pair();
        assert_eq!(signal.sleep(Duration::from_millis(10)).await, Ok(()));
    }

    #[tokio::test]
    async fn sleep_returns_cancelled_as_soon_as_the_run_is_cancelled() {
        let (handle, mut signal) = cancel_pair();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            handle.cancel();
        });
        let outcome = tokio::time::timeout(
            Duration::from_secs(1),
            signal.sleep(Duration::from_secs(30)),
        )
        .await
        .expect("a cancel must end the sleep well before 30s");
        assert_eq!(outcome, Err(Cancelled));
    }

    #[tokio::test]
    async fn sleep_after_a_cancel_returns_at_once() {
        let (handle, mut signal) = cancel_pair();
        handle.cancel();
        let outcome = tokio::time::timeout(
            Duration::from_millis(100),
            signal.sleep(Duration::from_secs(30)),
        )
        .await
        .expect("an already-cancelled signal must not sleep");
        assert_eq!(outcome, Err(Cancelled));
    }

    #[tokio::test]
    async fn cancelled_never_resolves_after_the_handle_is_dropped() {
        let (handle, mut signal) = cancel_pair();
        drop(handle);
        let waited = tokio::time::timeout(Duration::from_millis(50), signal.cancelled()).await;
        assert!(waited.is_err(), "a finished run must never look cancelled");
        assert!(!signal.is_cancelled());
    }
}
