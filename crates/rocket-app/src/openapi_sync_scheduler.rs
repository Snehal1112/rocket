//! The background supervisor for OpenAPI sync checks.
//!
//! One loop ticks, asks the service which collections are due (from the local state only) and
//! runs their checks one at a time, at most a few per tick. It never applies a change. Stopping
//! ends the loop, also during a sleep, but does not cancel a check that is already running.
//! Log lines carry codes and counts only.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rocket_shared::error::DomainError;
use rocket_shared::events::{DomainEvent, EventPublisher};
use tokio::sync::watch;

use crate::openapi_sync_service::{CheckOutcome, OpenApiSyncService};

/// First retry delay for a check that returned an error, then doubled up to the cap.
const ERROR_BACKOFF_BASE_SECS: i64 = 15 * 60;
const ERROR_BACKOFF_CAP_SECS: i64 = 6 * 3600;

/// Builds a service for one unit of work. The scheduler never keeps one across ticks.
pub type ServiceFactory = Arc<dyn Fn() -> OpenApiSyncService + Send + Sync>;
type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Time between two looks at the due list.
    pub tick: Duration,
    /// Quiet time after the start before the first look.
    pub launch_delay: Duration,
    /// Most checks started in one tick.
    pub max_checks_per_tick: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            tick: Duration::from_secs(60),
            launch_delay: Duration::from_secs(30),
            max_checks_per_tick: 5,
        }
    }
}

/// Keeps a running loop alive. Dropping it stops the loop.
pub struct SchedulerHandle {
    stop: watch::Sender<bool>,
}

impl SchedulerHandle {
    /// Ends the loop. A check that is running finishes first.
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// In-memory delay for a collection whose check returned an error. An unreadable local file
/// cannot store a failure count, so this keeps such a collection from being retried every tick.
struct ErrorBackoff {
    failures: u32,
    until: DateTime<Utc>,
}

pub struct OpenApiSyncScheduler {
    factory: ServiceFactory,
    events: Arc<dyn EventPublisher>,
    config: SchedulerConfig,
    clock: Clock,
}

impl OpenApiSyncScheduler {
    pub fn new(
        service_factory: ServiceFactory,
        events: Arc<dyn EventPublisher>,
        config: SchedulerConfig,
    ) -> Self {
        Self {
            factory: service_factory,
            events,
            config,
            clock: Arc::new(Utc::now),
        }
    }

    /// Replaces the clock, so tests can use a fixed time.
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// Starts the loop on the current tokio runtime.
    pub fn start(self) -> SchedulerHandle {
        let (tx, rx) = watch::channel(false);
        tokio::spawn(self.run(rx));
        SchedulerHandle { stop: tx }
    }

    async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut errors: HashMap<PathBuf, ErrorBackoff> = HashMap::new();
        if !sleep_or_stop(&mut stop, self.config.launch_delay).await {
            return;
        }
        loop {
            self.tick(&mut errors, &stop).await;
            if !sleep_or_stop(&mut stop, self.config.tick).await {
                return;
            }
        }
    }

    async fn tick(
        &self,
        errors: &mut HashMap<PathBuf, ErrorBackoff>,
        stop: &watch::Receiver<bool>,
    ) {
        let now = (self.clock)();
        let factory = Arc::clone(&self.factory);
        // The due list reads one local file per collection, so it runs off the async threads.
        let targets = match tokio::task::spawn_blocking(move || factory().due_targets(now)).await {
            Ok(targets) => targets,
            Err(_) => {
                tracing::warn!(code = "due_list_failed", "sync due list task failed");
                return;
            }
        };
        // Forget collections that are no longer due or listed. A held-back one stays due, so
        // its failure count survives until the delay ends and doubles on the next error.
        errors.retain(|root, _| targets.iter().any(|t| &t.root == root));
        let mut ran = 0usize;
        for target in targets {
            if ran >= self.config.max_checks_per_tick || *stop.borrow() {
                break;
            }
            if errors.get(&target.root).is_some_and(|e| e.until > now) {
                continue;
            }
            ran += 1;
            // Each check gets its own task, so a panic fails that check, not the loop. The
            // loop awaits it, which keeps one check at a time and never cancels it on stop.
            let service = (self.factory)();
            let root = target.root.clone();
            let result = match tokio::spawn(async move { service.check(&root).await }).await {
                Ok(result) => result,
                Err(_) => {
                    tracing::warn!(code = "check_panicked", "sync check task failed");
                    Err(DomainError::Internal("The sync check stopped.".to_string()))
                }
            };
            let status = match result {
                Ok(CheckOutcome::Unchanged) => {
                    errors.remove(&target.root);
                    "unchanged"
                }
                Ok(CheckOutcome::Changed { .. }) => {
                    errors.remove(&target.root);
                    "changed"
                }
                Ok(CheckOutcome::Failed { .. }) => {
                    errors.remove(&target.root);
                    "failed"
                }
                Err(_) => {
                    let failures = errors
                        .get(&target.root)
                        .map_or(1, |e| e.failures.saturating_add(1));
                    let secs = error_backoff_secs(failures);
                    // The delay starts when the check failed, not when the tick began.
                    let until = (self.clock)() + chrono::Duration::seconds(secs);
                    errors.insert(target.root.clone(), ErrorBackoff { failures, until });
                    tracing::debug!(
                        code = "check_error",
                        failures,
                        "sync check returned an error"
                    );
                    "failed"
                }
            };
            self.events.publish(DomainEvent::OpenApiSyncChecked {
                collection: target.collection,
                status: status.to_string(),
            });
        }
        if ran > 0 {
            tracing::debug!(ran, "sync checks ran");
        }
    }
}

fn error_backoff_secs(failures: u32) -> i64 {
    let steps = failures.saturating_sub(1).min(16);
    ERROR_BACKOFF_BASE_SECS
        .saturating_mul(1i64 << steps)
        .min(ERROR_BACKOFF_CAP_SECS)
}

/// Sleeps for `wait`. Returns false when a stop came first or the handle is gone.
async fn sleep_or_stop(stop: &mut watch::Receiver<bool>, wait: Duration) -> bool {
    if *stop.borrow() {
        return false;
    }
    tokio::select! {
        _ = tokio::time::sleep(wait) => !*stop.borrow(),
        _ = stop.changed() => false,
    }
}

#[cfg(test)]
#[path = "openapi_sync_scheduler_tests.rs"]
mod tests;
