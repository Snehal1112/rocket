//! Helpers for background loops that end on a stop signal.

use std::future::Future;
use std::time::Duration;

use tokio::sync::watch;

/// Sleeps for `wait`. Returns false when a stop came first or the handle is gone.
pub(crate) async fn sleep_or_stop(stop: &mut watch::Receiver<bool>, wait: Duration) -> bool {
    if *stop.borrow() {
        return false;
    }
    tokio::select! {
        _ = tokio::time::sleep(wait) => !*stop.borrow(),
        _ = stop.changed() => false,
    }
}

/// Waits `interval`, then runs `tick`, again and again, until a stop comes. The first tick runs
/// after one interval, not at once. A tick that is running is not cancelled by a stop.
pub(crate) async fn run_every<F, Fut>(
    stop: &mut watch::Receiver<bool>,
    interval: Duration,
    mut tick: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    while sleep_or_stop(stop, interval).await {
        tick().await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn sleep_or_stop_returns_true_after_the_wait() {
        let (_tx, mut rx) = watch::channel(false);
        assert!(sleep_or_stop(&mut rx, Duration::from_secs(5)).await);
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_or_stop_returns_false_when_already_stopped() {
        let (tx, mut rx) = watch::channel(false);
        tx.send_replace(true);
        assert!(!sleep_or_stop(&mut rx, Duration::from_secs(5)).await);
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_or_stop_returns_false_when_a_stop_comes_during_the_wait() {
        let (tx, mut rx) = watch::channel(false);
        let task =
            tokio::spawn(async move { sleep_or_stop(&mut rx, Duration::from_secs(60)).await });
        tokio::time::advance(Duration::from_secs(1)).await;
        tx.send_replace(true);
        assert!(!task.await.expect("the task finishes"));
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_or_stop_returns_false_when_the_sender_is_gone() {
        let (tx, mut rx) = watch::channel(false);
        drop(tx);
        assert!(!sleep_or_stop(&mut rx, Duration::from_secs(5)).await);
    }

    #[tokio::test(start_paused = true)]
    async fn run_every_ticks_once_per_interval_and_not_at_time_zero() {
        let (tx, mut rx) = watch::channel(false);
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&ticks);
        let tick = move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        };
        let stopper = async {
            tokio::time::sleep(Duration::from_secs(25)).await;
            tx.send_replace(true);
        };
        tokio::join!(run_every(&mut rx, Duration::from_secs(10), tick), stopper);
        assert_eq!(ticks.load(Ordering::SeqCst), 2, "ticks at 10 s and 20 s");
    }

    #[tokio::test(start_paused = true)]
    async fn run_every_does_not_tick_when_already_stopped() {
        let (tx, mut rx) = watch::channel(false);
        tx.send_replace(true);
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&ticks);
        run_every(&mut rx, Duration::from_secs(10), move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await;
        assert_eq!(ticks.load(Ordering::SeqCst), 0);
    }
}
