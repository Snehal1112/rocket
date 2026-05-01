use crate::{HttpExecutor, HttpRequest};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Notify, Semaphore};

use tauri::{AppHandle, Emitter};

/// Fixed-capacity circular buffer. When full, the oldest entry is overwritten.
pub struct RingBuffer<T> {
    inner: VecDeque<T>,
    capacity: usize,
}

impl<T: Clone> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.inner.len() == self.capacity {
            self.inner.pop_front();
        }
        self.inner.push_back(item);
    }

    pub fn snapshot(&self) -> Vec<T> {
        self.inner.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Drives phase transitions during a load test. Produces per-second concurrency
/// checkpoints derived from the phase definitions; the runner polls these to
/// adjust semaphore permits at phase boundaries.
pub struct PhaseScheduler {
    phases: Vec<LoadTestPhase>,
}

impl PhaseScheduler {
    pub fn new(phases: Vec<LoadTestPhase>) -> Self {
        Self { phases }
    }

    /// Returns one `(elapsed_secs, target_concurrency)` checkpoint per second
    /// across all phases. RampUp / RampDown linearly interpolate; Hold stays flat.
    pub fn checkpoints(&self) -> Vec<(u64, u32)> {
        let mut result = Vec::new();
        let mut elapsed: u64 = 0;
        for phase in &self.phases {
            let start_conc = result.last().map(|(_, c)| *c).unwrap_or(0);
            let end_conc = phase.target.value();
            let steps = phase.duration_secs as u64;
            for step in 1..=steps {
                let t = elapsed + step;
                let conc = if phase.kind == PhaseKind::Hold {
                    end_conc
                } else {
                    let progress = step as f64 / steps as f64;
                    (start_conc as f64 + (end_conc as f64 - start_conc as f64) * progress)
                        .round() as u32
                };
                result.push((t, conc));
            }
            elapsed += steps;
        }
        result
    }

    /// Which phase index is active at `elapsed_secs`?
    pub fn phase_index_at(&self, elapsed_secs: u64) -> usize {
        let mut boundary: u64 = 0;
        for (i, phase) in self.phases.iter().enumerate() {
            boundary += phase.duration_secs as u64;
            if elapsed_secs < boundary {
                return i;
            }
        }
        self.phases.len().saturating_sub(1)
    }
}

/// Token-bucket rate limiter that releases requests at a target req/sec rate.
///
/// The current rate is updated by a separate task that interpolates between
/// per-second checkpoints emitted by `PhaseScheduler`. The bucket capacity is
/// hard-coded to 250 ms worth of tokens at the *peak* configured rate, so a
/// quiet period cannot store up unlimited bursts.
///
/// `acquire()` blocks until the next token is available. Internally this
/// is driven by a `Notify` woken by a periodic refill task that ticks every
/// `min(50 ms, 1/rate)` and tops up the token count.
pub struct RateDriver {
    /// Current available tokens (millitokens — i.e. tokens × 1000 — for finer rates).
    millitokens: Arc<AtomicU64>,
    /// Bucket capacity in millitokens. Set once at construction from the peak rate.
    capacity_millitokens: u64,
    /// Wakes any `acquire()` waiters when tokens are added.
    notify: Arc<Notify>,
    /// Current rate in millitokens per millisecond (= rps × 1, since
    /// 1 token/sec = 1 millitoken/ms). Updated by the rate-update task.
    rate_per_ms: Arc<AtomicU64>,
}

impl RateDriver {
    /// Construct with an initial rate (req/sec). `peak_rate` sets the bucket
    /// capacity to 250 ms worth of tokens at that rate so the burst tolerance
    /// scales with the loudest phase.
    pub fn new(initial_rate_rps: u32, peak_rate_rps: u32) -> Self {
        let capacity_millitokens = (peak_rate_rps as u64) * 250; // 250 ms × peak_rps
        Self {
            millitokens: Arc::new(AtomicU64::new(0)),
            capacity_millitokens: capacity_millitokens.max(1000), // floor 1 token of slack
            notify: Arc::new(Notify::new()),
            rate_per_ms: Arc::new(AtomicU64::new(initial_rate_rps as u64)),
        }
    }

    /// Update the current rate. Called by the rate-interpolation task.
    pub fn set_rate(&self, rps: u32) {
        self.rate_per_ms.store(rps as u64, Ordering::Release);
    }

    /// Block until one full token is available, then consume it.
    pub async fn acquire(&self) {
        loop {
            // Register the listener first so any subsequent notify_waiters()
            // call is captured even if the bucket re-fills before we await.
            let notified = self.notify.notified();
            tokio::pin!(notified);

            let current = self.millitokens.load(Ordering::Acquire);
            if current >= 1000 {
                // Guard above ensures no underflow; plain sub is safe here.
                let want = current - 1000;
                if self
                    .millitokens
                    .compare_exchange(current, want, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return;
                }
                // CAS lost — retry.
                continue;
            }
            // Not enough — wait to be woken by the refill task.
            notified.await;
        }
    }

    /// Refill loop. Adds `rate × elapsed_ms` millitokens every tick, capped
    /// at `capacity_millitokens`, then wakes one waiter. Runs until the
    /// returned `JoinHandle` is aborted.
    pub fn spawn_refill(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            const TICK_MS: u64 = 25;
            let mut ticker = tokio::time::interval(Duration::from_millis(TICK_MS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            ticker.tick().await; // skip immediate first tick
            loop {
                ticker.tick().await;
                let rate = me.rate_per_ms.load(Ordering::Acquire);
                if rate == 0 {
                    continue;
                }
                let add = rate.saturating_mul(TICK_MS); // millitokens to add
                let mut current = me.millitokens.load(Ordering::Acquire);
                loop {
                    let next = (current + add).min(me.capacity_millitokens);
                    match me.millitokens.compare_exchange(
                        current,
                        next,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => break,
                        Err(observed) => current = observed,
                    }
                }
                me.notify.notify_waiters();
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum PhaseKind {
    RampUp,
    Hold,
    RampDown,
}

/// What a phase is steering toward: a number of concurrent in-flight requests,
/// or a target requests-per-second rate. All phases in a single config must
/// share the same variant — mixing units in one run is rejected at runtime.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum PhaseTarget {
    Concurrency(u32),
    Rps(u32),
}

impl PhaseTarget {
    pub fn value(self) -> u32 {
        match self {
            PhaseTarget::Concurrency(v) => v,
            PhaseTarget::Rps(v) => v,
        }
    }

    pub fn is_concurrency(self) -> bool {
        matches!(self, PhaseTarget::Concurrency(_))
    }

    pub fn is_rps(self) -> bool {
        matches!(self, PhaseTarget::Rps(_))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestPhase {
    pub kind: PhaseKind,
    pub duration_secs: u32,
    pub target: PhaseTarget,
}

impl<'de> Deserialize<'de> for LoadTestPhase {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            kind: PhaseKind,
            duration_secs: u32,
            #[serde(default)]
            target: Option<PhaseTarget>,
            #[serde(default)]
            target_concurrency: Option<u32>,
        }
        let raw = Raw::deserialize(d)?;
        let target = match (raw.target, raw.target_concurrency) {
            (Some(t), _) => t,
            (None, Some(v)) => PhaseTarget::Concurrency(v),
            (None, None) => return Err(serde::de::Error::missing_field("target")),
        };
        Ok(LoadTestPhase {
            kind: raw.kind,
            duration_secs: raw.duration_secs,
            target,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuccessRule {
    pub status_below: u16,
}

impl Default for SuccessRule {
    fn default() -> Self {
        Self { status_below: 400 }
    }
}

/// Describes which unit all phases in a config are steering toward.
/// Internal API only — not serialised.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetUnit {
    Concurrency,
    Rps,
}

/// Phase-based load test configuration (v2).
/// The existing flat `LoadTestConfig` is kept for backwards compat with existing tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestConfigV2 {
    pub phases: Vec<LoadTestPhase>,
    #[serde(default)]
    pub success_rule: SuccessRule,
    #[serde(default = "default_ring_buffer_size")]
    pub ring_buffer_size: usize,
}

fn default_ring_buffer_size() -> usize { 5000 }

impl LoadTestConfigV2 {
    /// Returns the maximum concurrency across all phases.
    pub fn max_concurrency(&self) -> u32 {
        self.phases.iter().map(|p| p.target.value()).max().unwrap_or(1)
    }

    /// Returns total planned duration in seconds.
    pub fn total_duration_secs(&self) -> u32 {
        self.phases.iter().map(|p| p.duration_secs).sum()
    }

    /// Returns `true` iff every phase uses the same `PhaseTarget` variant.
    /// Mixed-unit configs (some Concurrency, some Rps) are not supported.
    pub fn has_uniform_target_unit(&self) -> bool {
        let mut iter = self.phases.iter();
        let Some(first) = iter.next() else { return true };
        let first_is_rps = first.target.is_rps();
        iter.all(|p| p.target.is_rps() == first_is_rps)
    }

    /// Returns the target unit of the run, or `None` if there are no phases.
    pub fn target_unit(&self) -> Option<TargetUnit> {
        self.phases.first().map(|p| {
            if p.target.is_rps() { TargetUnit::Rps } else { TargetUnit::Concurrency }
        })
    }
}

/// Emitted via Tauri event every 250 ms during a running load test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestProgressEvent {
    pub elapsed_ms: u64,
    pub completed: u32,
    pub active_concurrent: u32,
    pub succeeded: u32,
    pub failed_status: u32,
    pub failed_transport: u32,
    /// Rolling requests-per-second over the last 2 seconds.
    pub requests_per_second: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub current_phase_index: usize,
    /// Last 50 entries from the ring buffer — drives the live per-request log.
    pub recent_log: Vec<RequestLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesPoint {
    pub elapsed_ms: u64,
    pub rps: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub error_rate_pct: f64,
    pub active_concurrent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogEntry {
    pub seq: u32,
    /// `None` means transport-level failure (no HTTP response received).
    pub status: Option<u16>,
    pub latency_ms: f64,
    pub response_bytes: u64,
    pub error: Option<String>,
    pub phase_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseMarker {
    pub phase_index: usize,
    pub started_at_ms: u64,
}

/// Configuration for a load test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestConfig {
    pub concurrency: u32,
    pub total_requests: u32,
    #[serde(default)]
    pub interval_ms: u32,
    /// Hard ceiling on total run time. The test stops spawning new requests
    /// once this many seconds have elapsed, even if `total_requests` is not
    /// yet reached. `None` means no cap.
    #[serde(default)]
    pub duration_cap_secs: Option<u32>,
}

/// Aggregated statistics from a completed load test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestResult {
    pub total_requests: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub failed_transport: u32,
    pub failed_status: u32,
    pub min_latency_ms: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub max_latency_ms: f64,
    pub requests_per_second: f64,
    pub total_duration_ms: f64,
    #[serde(default)]
    pub phase_timeline: Vec<PhaseMarker>,
    #[serde(default)]
    pub request_log: Vec<RequestLogEntry>,
    #[serde(default)]
    pub time_series: Vec<TimeSeriesPoint>,
}

/// Returns the value at percentile p (0–100) from a sorted slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Per-task result from a single load-test request.
enum Outcome {
    /// HTTP status < 400 — counted as success, latency recorded.
    Success(f64),
    /// HTTP status >= 400 — counted as failure, latency still recorded.
    StatusFail(f64),
    /// Executor returned Err (connection refused, TLS error, timeout, ...).
    /// No latency sample.
    TransportFail,
}

struct RunAccumulator {
    latencies: Vec<f64>,
    log: RingBuffer<RequestLogEntry>,
    time_series: Vec<TimeSeriesPoint>,
    phase_timeline: Vec<PhaseMarker>,
    succeeded: u32,
    failed_status: u32,
    failed_transport: u32,
    completed: u32,
    /// Completion timestamps in the last 2 s for rolling RPS.
    recent_completions: VecDeque<std::time::Instant>,
}

impl RunAccumulator {
    fn new(ring_buffer_size: usize) -> Self {
        Self {
            latencies: Vec::new(),
            log: RingBuffer::new(ring_buffer_size),
            time_series: Vec::new(),
            phase_timeline: Vec::new(),
            succeeded: 0,
            failed_status: 0,
            failed_transport: 0,
            completed: 0,
            recent_completions: VecDeque::new(),
        }
    }

    fn record_completion(&mut self, now: std::time::Instant) {
        let window = std::time::Duration::from_secs(2);
        while self
            .recent_completions
            .front()
            .map(|t| now.duration_since(*t) > window)
            .unwrap_or(false)
        {
            self.recent_completions.pop_front();
        }
        self.recent_completions.push_back(now);
    }

    fn rolling_rps(&mut self) -> f64 {
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(2);
        while self
            .recent_completions
            .front()
            .map(|t| now.duration_since(*t) > window)
            .unwrap_or(false)
        {
            self.recent_completions.pop_front();
        }
        self.recent_completions.len() as f64 / 2.0
    }

    fn current_percentiles(&self) -> (f64, f64, f64) {
        if self.latencies.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let mut sorted = self.latencies.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (
            percentile(&sorted, 50.0),
            percentile(&sorted, 95.0),
            percentile(&sorted, 99.0),
        )
    }
}

/// Phase-aware load test runner with optional Tauri event emission.
///
/// Drives either a `PhaseScheduler`-backed semaphore (for Concurrency phases)
/// or a `RateDriver` token bucket (for RPS phases). The unit is determined by
/// the first phase; callers MUST validate uniformity before calling — if they
/// don't, downstream phases with a different unit are reinterpreted as the
/// run's chosen unit (their numeric value is preserved). Validation lives in
/// `LoadTestConfigV2::has_uniform_target_unit`.
pub async fn run_load_test_v2(
    executor: Arc<dyn HttpExecutor>,
    request: &HttpRequest,
    config: &LoadTestConfigV2,
    app: Option<&AppHandle>,
) -> LoadTestResult {
    use tokio::time::interval;

    let scheduler = Arc::new(PhaseScheduler::new(config.phases.clone()));
    let target_unit = config.target_unit().unwrap_or(TargetUnit::Concurrency);

    // Shared accumulator + active counter — used by both target-unit paths.
    let active_concurrent = Arc::new(AtomicU32::new(0));
    let accumulator = Arc::new(tokio::sync::Mutex::new(RunAccumulator::new(
        config.ring_buffer_size,
    )));
    let success_rule = config.success_rule.clone();
    let run_start = std::time::Instant::now();

    // Concurrency-mode setup: semaphore + per-second permit reshaping.
    // Rps-mode setup: RateDriver + per-second rate updates.
    let semaphore: Option<Arc<Semaphore>> = match target_unit {
        TargetUnit::Concurrency => {
            let initial_conc = config
                .phases
                .first()
                .map(|p| p.target.value())
                .unwrap_or(1)
                .max(1) as usize;
            Some(Arc::new(Semaphore::new(initial_conc)))
        }
        TargetUnit::Rps => None,
    };

    let rate_driver: Option<Arc<RateDriver>> = match target_unit {
        TargetUnit::Rps => {
            let peak = config
                .phases
                .iter()
                .map(|p| p.target.value())
                .max()
                .unwrap_or(1);
            let initial = config.phases.first().map(|p| p.target.value()).unwrap_or(0);
            Some(Arc::new(RateDriver::new(initial, peak)))
        }
        TargetUnit::Concurrency => None,
    };

    // --- Phase-target updater task: depending on mode, either reshapes the
    //     semaphore permits or updates the rate driver's target rate. ---
    let checkpoints = scheduler.checkpoints();

    // Refill task lives at this scope so we can abort it cleanly during teardown
    // — dropping a JoinHandle in tokio detaches but does not cancel the task.
    let refill_handle: Option<tokio::task::JoinHandle<()>> = match target_unit {
        TargetUnit::Rps => Some(
            rate_driver
                .as_ref()
                .expect("rate driver present in rps mode")
                .spawn_refill(),
        ),
        TargetUnit::Concurrency => None,
    };

    let phase_handle = match target_unit {
        TargetUnit::Concurrency => {
            let phase_sem = semaphore.as_ref().expect("semaphore present in concurrency mode").clone();
            let initial_conc = phase_sem.available_permits() as u32;
            let phase_perm = Arc::new(AtomicU32::new(initial_conc));
            tokio::spawn(async move {
                let phase_start = std::time::Instant::now();
                for (target_secs, target_conc) in checkpoints {
                    let now_secs = phase_start.elapsed().as_secs();
                    if target_secs > now_secs {
                        tokio::time::sleep(Duration::from_secs(target_secs - now_secs)).await;
                    }
                    let current = phase_perm.load(Ordering::SeqCst);
                    if target_conc > current {
                        phase_sem.add_permits((target_conc - current) as usize);
                        phase_perm.store(target_conc, Ordering::SeqCst);
                    } else if target_conc < current {
                        let to_remove = current - target_conc;
                        let sem = Arc::clone(&phase_sem);
                        let perm = Arc::clone(&phase_perm);
                        tokio::spawn(async move {
                            if let Ok(p) = sem.acquire_many(to_remove).await {
                                p.forget();
                                perm.fetch_sub(to_remove, Ordering::SeqCst);
                            }
                        });
                    }
                }
            })
        }
        TargetUnit::Rps => {
            let driver = Arc::clone(rate_driver.as_ref().expect("rate driver present in rps mode"));
            tokio::spawn(async move {
                let phase_start = std::time::Instant::now();
                let mut last_rate = driver.rate_per_ms.load(Ordering::Acquire) as u32;
                // checkpoints already linearly interpolate concurrency-style
                // values; for Rps we treat them the same — the value at
                // each checkpoint is the target rps for that second.
                for (target_secs, target_rps) in checkpoints {
                    let now_secs = phase_start.elapsed().as_secs();
                    if target_secs > now_secs {
                        tokio::time::sleep(Duration::from_secs(target_secs - now_secs)).await;
                    }
                    if target_rps != last_rate {
                        driver.set_rate(target_rps);
                        last_rate = target_rps;
                    }
                }
                // After all checkpoints, this task simply returns. Refill keeps running
                // because its JoinHandle lives at the outer `run_load_test_v2` scope.
            })
        }
    };

    // --- 250 ms snapshot + event emission task ---
    let acc_snap = Arc::clone(&accumulator);
    let active_snap = Arc::clone(&active_concurrent);
    let scheduler_snap = Arc::clone(&scheduler);
    let app_clone = app.cloned();

    let snapshot_handle = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_millis(250));
        ticker.tick().await; // skip the immediate first tick at t=0
        let start = std::time::Instant::now();
        let mut last_phase: Option<usize> = None;

        loop {
            ticker.tick().await;
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let elapsed_secs = elapsed_ms / 1000;
            let phase_idx = scheduler_snap.phase_index_at(elapsed_secs);
            let active = active_snap.load(Ordering::SeqCst);

            let mut acc = acc_snap.lock().await;
            let rps = acc.rolling_rps();
            let (p50, p95, p99) = acc.current_percentiles();

            if last_phase != Some(phase_idx) {
                acc.phase_timeline.push(PhaseMarker {
                    phase_index: phase_idx,
                    started_at_ms: elapsed_ms,
                });
                last_phase = Some(phase_idx);
            }

            let error_rate = if acc.completed > 0 {
                (acc.failed_status + acc.failed_transport) as f64 / acc.completed as f64 * 100.0
            } else {
                0.0
            };

            acc.time_series.push(TimeSeriesPoint {
                elapsed_ms,
                rps,
                p50_ms: p50,
                p95_ms: p95,
                p99_ms: p99,
                error_rate_pct: error_rate,
                active_concurrent: active,
            });

            let log_snapshot = acc.log.snapshot();
            let recent_log: Vec<RequestLogEntry> = log_snapshot
                .into_iter()
                .rev()
                .take(50)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            let event = LoadTestProgressEvent {
                elapsed_ms,
                completed: acc.completed,
                active_concurrent: active,
                succeeded: acc.succeeded,
                failed_status: acc.failed_status,
                failed_transport: acc.failed_transport,
                requests_per_second: rps,
                p50_ms: p50,
                p95_ms: p95,
                p99_ms: p99,
                current_phase_index: phase_idx,
                recent_log,
            };

            drop(acc);

            if let Some(ref handle) = app_clone {
                let _ = handle.emit("load_test_progress", &event);
            }
        }
    });

    // --- Per-request spawning loop (duration-based) ---
    let total_duration = Duration::from_secs(config.total_duration_secs() as u64);
    let loop_start = std::time::Instant::now();
    let mut seq: u32 = 0;
    let mut join_handles = Vec::new();

    while loop_start.elapsed() < total_duration {
        // Gate spawn rate by the active mode.
        let owned_permit = match (&semaphore, &rate_driver) {
            (Some(sem), _) => Some(sem.clone().acquire_owned().await.unwrap()),
            (None, Some(driver)) => {
                let remaining = total_duration.saturating_sub(loop_start.elapsed());
                if remaining.is_zero() {
                    break;
                }
                tokio::select! {
                    _ = driver.acquire() => None,
                    _ = tokio::time::sleep(remaining) => {
                        // Duration expired while waiting for a token. Exit the
                        // spawn loop without spawning another request.
                        break;
                    }
                }
            }
            (None, None) => unreachable!("either semaphore or rate_driver must be set"),
        };

        // Recheck duration after gate (acquire may have blocked).
        if loop_start.elapsed() >= total_duration {
            drop(owned_permit);
            break;
        }

        let req = request.clone();
        let exec = Arc::clone(&executor);
        let acc = Arc::clone(&accumulator);
        let active = Arc::clone(&active_concurrent);
        let rule = success_rule.clone();
        let phase_idx = scheduler.phase_index_at(loop_start.elapsed().as_secs());
        let current_seq = seq;
        seq += 1;

        let handle = tokio::spawn(async move {
            active.fetch_add(1, Ordering::SeqCst);
            let t0 = std::time::Instant::now();
            let outcome = exec.execute(&req).await;
            let latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let now = std::time::Instant::now();

            let mut acc = acc.lock().await;
            acc.completed += 1;
            acc.record_completion(now);
            match outcome {
                Ok(resp) => {
                    acc.latencies.push(latency_ms);
                    if resp.status < rule.status_below {
                        acc.succeeded += 1;
                    } else {
                        acc.failed_status += 1;
                    }
                    acc.log.push(RequestLogEntry {
                        seq: current_seq,
                        status: Some(resp.status),
                        latency_ms,
                        response_bytes: resp.size_bytes as u64,
                        error: None,
                        phase_index: phase_idx,
                    });
                }
                Err(e) => {
                    acc.failed_transport += 1;
                    acc.log.push(RequestLogEntry {
                        seq: current_seq,
                        status: None,
                        latency_ms: 0.0,
                        response_bytes: 0,
                        error: Some(e.to_string()),
                        phase_index: phase_idx,
                    });
                }
            }
            drop(acc);
            active.fetch_sub(1, Ordering::SeqCst);
            drop(owned_permit); // no-op when None
        });
        join_handles.push(handle);
    }

    // Wait for in-flight requests to drain, capped at 60 s to avoid hanging
    // indefinitely when the target server stops responding (e.g. no per-request
    // timeout configured).
    let drain_deadline = tokio::time::sleep(Duration::from_secs(60));
    tokio::pin!(drain_deadline);
    for mut h in join_handles {
        tokio::select! {
            _ = &mut drain_deadline => { h.abort(); let _ = h.await; }
            _ = &mut h => {}
        }
    }

    snapshot_handle.abort();
    phase_handle.abort();
    if let Some(rh) = refill_handle {
        rh.abort();
    }

    let total_duration_ms = run_start.elapsed().as_secs_f64() * 1000.0;
    let acc = accumulator.lock().await;

    let mut sorted_latencies = acc.latencies.clone();
    sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let total_requests = acc.completed;
    let succeeded = acc.succeeded;
    let failed = acc.failed_status + acc.failed_transport;
    let rps = if total_duration_ms > 0.0 {
        total_requests as f64 / (total_duration_ms / 1000.0)
    } else {
        0.0
    };

    let result = LoadTestResult {
        total_requests,
        succeeded,
        failed,
        failed_status: acc.failed_status,
        failed_transport: acc.failed_transport,
        min_latency_ms: sorted_latencies.first().copied().unwrap_or(0.0),
        avg_latency_ms: if sorted_latencies.is_empty() {
            0.0
        } else {
            sorted_latencies.iter().sum::<f64>() / sorted_latencies.len() as f64
        },
        p50_latency_ms: percentile(&sorted_latencies, 50.0),
        p95_latency_ms: percentile(&sorted_latencies, 95.0),
        p99_latency_ms: percentile(&sorted_latencies, 99.0),
        max_latency_ms: sorted_latencies.last().copied().unwrap_or(0.0),
        requests_per_second: rps,
        total_duration_ms,
        phase_timeline: acc.phase_timeline.clone(),
        request_log: acc.log.snapshot(),
        time_series: acc.time_series.clone(),
    };

    if let Some(handle) = app {
        let _ = handle.emit("load_test_complete", &result);
    }

    result
}

/// Per-task result carrying both the outcome classification and the log entry.
struct TaskResult {
    outcome: Outcome,
    entry: RequestLogEntry,
}

/// Fires `config.total_requests` concurrent HTTP requests, bounded by `config.concurrency`,
/// then returns aggregated latency statistics including a per-request log.
pub async fn run_load_test(
    executor: Arc<dyn HttpExecutor>,
    request: &HttpRequest,
    config: &LoadTestConfig,
) -> LoadTestResult {
    let semaphore = Arc::new(Semaphore::new(config.concurrency as usize));
    let total = config.total_requests as usize;
    let start = std::time::Instant::now();
    let cap = config.duration_cap_secs.map(|s| Duration::from_secs(s as u64));

    let mut handles = Vec::new();
    let mut seq: u32 = 0;

    for i in 0..total {
        // Stop spawning if the duration cap has been reached.
        if let Some(cap) = cap {
            if start.elapsed() >= cap {
                break;
            }
        }

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let req = request.clone();
        let exec = executor.clone();
        let current_seq = seq;
        seq += 1;

        let handle = tokio::spawn(async move {
            let t0 = std::time::Instant::now();
            let result = exec.execute(&req).await;
            let latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
            drop(permit);
            match result {
                Ok(resp) => {
                    let ms = resp.duration_ms as f64;
                    let outcome = if resp.status < 400 {
                        Outcome::Success(ms)
                    } else {
                        Outcome::StatusFail(ms)
                    };
                    TaskResult {
                        outcome,
                        entry: RequestLogEntry {
                            seq: current_seq,
                            status: Some(resp.status),
                            latency_ms,
                            response_bytes: resp.size_bytes as u64,
                            error: None,
                            phase_index: 0,
                        },
                    }
                }
                Err(e) => TaskResult {
                    outcome: Outcome::TransportFail,
                    entry: RequestLogEntry {
                        seq: current_seq,
                        status: None,
                        latency_ms: 0.0,
                        response_bytes: 0,
                        error: Some(e.to_string()),
                        phase_index: 0,
                    },
                },
            }
        });
        handles.push(handle);

        // Rate-limit by sleeping between spawns (skip after the last).
        if i + 1 < total && config.interval_ms > 0 {
            tokio::time::sleep(Duration::from_millis(config.interval_ms as u64)).await;
        }
    }

    let mut succeeded: u32 = 0;
    let mut failed_transport: u32 = 0;
    let mut failed_status: u32 = 0;
    let mut latencies = Vec::with_capacity(total);
    let mut log: RingBuffer<RequestLogEntry> = RingBuffer::new(5000);

    for handle in handles {
        match handle.await {
            Ok(task) => {
                log.push(task.entry);
                match task.outcome {
                    Outcome::Success(ms) => {
                        succeeded += 1;
                        latencies.push(ms);
                    }
                    Outcome::StatusFail(ms) => {
                        failed_status += 1;
                        latencies.push(ms);
                    }
                    Outcome::TransportFail => {
                        failed_transport += 1;
                    }
                }
            }
            Err(_) => {
                // tokio::spawn join error — treat as transport-level failure.
                failed_transport += 1;
            }
        }
    }

    let failed = failed_transport + failed_status;
    let total_duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let avg = if latencies.is_empty() {
        0.0
    } else {
        latencies.iter().sum::<f64>() / latencies.len() as f64
    };

    LoadTestResult {
        total_requests: succeeded + failed,
        succeeded,
        failed,
        failed_transport,
        failed_status,
        min_latency_ms: latencies.first().copied().unwrap_or(0.0),
        avg_latency_ms: avg,
        p50_latency_ms: percentile(&latencies, 50.0),
        p95_latency_ms: percentile(&latencies, 95.0),
        p99_latency_ms: percentile(&latencies, 99.0),
        max_latency_ms: latencies.last().copied().unwrap_or(0.0),
        requests_per_second: if total_duration_ms > 0.0 {
            (succeeded + failed) as f64 / (total_duration_ms / 1000.0)
        } else {
            0.0
        },
        total_duration_ms,
        phase_timeline: vec![],
        request_log: log.snapshot(),
        time_series: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpResponse, RequestOptions};
    use async_trait::async_trait;
    use rocket_shared::error::DomainResult;
    use rocket_shared::types::{Auth, HttpMethod};

    #[test]
    fn ring_buffer_overwrites_when_full() {
        let mut rb: RingBuffer<u32> = RingBuffer::new(3);
        rb.push(1);
        rb.push(2);
        rb.push(3);
        rb.push(4); // overwrites 1
        assert_eq!(rb.snapshot(), vec![2, 3, 4]);
        assert_eq!(rb.len(), 3);
    }

    #[test]
    fn ring_buffer_partial_fill() {
        let mut rb: RingBuffer<u32> = RingBuffer::new(10);
        rb.push(42);
        assert_eq!(rb.snapshot(), vec![42]);
        assert_eq!(rb.len(), 1);
    }

    #[test]
    fn ring_buffer_empty_snapshot() {
        let rb: RingBuffer<u32> = RingBuffer::new(5);
        assert_eq!(rb.snapshot(), Vec::<u32>::new());
        assert!(rb.is_empty());
    }

    #[test]
    fn phase_scheduler_hold_checkpoints() {
        let sched = PhaseScheduler::new(vec![LoadTestPhase {
            kind: PhaseKind::Hold,
            duration_secs: 3,
            target: PhaseTarget::Concurrency(10),
        }]);
        let cps = sched.checkpoints();
        assert_eq!(cps.len(), 3);
        assert!(cps.iter().all(|(_, c)| *c == 10));
    }

    #[test]
    fn phase_scheduler_rampup_linear() {
        let sched = PhaseScheduler::new(vec![LoadTestPhase {
            kind: PhaseKind::RampUp,
            duration_secs: 4,
            target: PhaseTarget::Concurrency(4),
        }]);
        let cps = sched.checkpoints();
        assert_eq!(cps.len(), 4);
        let concs: Vec<u32> = cps.iter().map(|(_, c)| *c).collect();
        assert_eq!(concs, vec![1, 2, 3, 4]);
    }

    #[test]
    fn phase_index_at_correct_boundaries() {
        let sched = PhaseScheduler::new(vec![
            LoadTestPhase { kind: PhaseKind::RampUp, duration_secs: 10, target: PhaseTarget::Concurrency(25) },
            LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 40, target: PhaseTarget::Concurrency(25) },
            LoadTestPhase { kind: PhaseKind::RampDown, duration_secs: 10, target: PhaseTarget::Concurrency(0) },
        ]);
        assert_eq!(sched.phase_index_at(0), 0);
        assert_eq!(sched.phase_index_at(9), 0);
        assert_eq!(sched.phase_index_at(10), 1);
        assert_eq!(sched.phase_index_at(50), 2);
        assert_eq!(sched.phase_index_at(999), 2);
    }

    struct MockExecutor;

    #[async_trait]
    impl HttpExecutor for MockExecutor {
        async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: "ok".into(),
                duration_ms: 10,
                ttfb_ms: 10,
                size_bytes: 2,
            })
        }
    }

    // Mock that always returns a given HTTP status with a fixed duration.
    struct StatusExecutor(u16);

    #[async_trait]
    impl HttpExecutor for StatusExecutor {
        async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
            Ok(HttpResponse {
                status: self.0,
                status_text: "".into(),
                headers: vec![],
                body: "".into(),
                duration_ms: 5,
                ttfb_ms: 5,
                size_bytes: 0,
            })
        }
    }

    fn test_request() -> HttpRequest {
        HttpRequest {
            method: HttpMethod::Get,
            url: "http://localhost/test".into(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth: Auth::None,
            options: RequestOptions::default(),
        }
    }

    #[tokio::test]
    async fn load_test_returns_correct_counts() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor);
        let config = LoadTestConfig {
            concurrency: 5,
            total_requests: 20,
            interval_ms: 0,
            duration_cap_secs: None,
        };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.total_requests, 20);
        assert_eq!(result.succeeded, 20);
        assert_eq!(result.failed, 0);
        assert!(result.requests_per_second > 0.0);
        assert!(result.avg_latency_ms >= 10.0);
        assert!(result.total_duration_ms > 0.0);
    }

    #[test]
    fn percentile_computation() {
        let sorted = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        // p50 of 10 elements: round(0.5 * 9.0) = round(4.5) = 5 → index 5 → value 6.0
        assert_eq!(percentile(&sorted, 50.0), 6.0);
        assert_eq!(percentile(&sorted, 0.0), 1.0);
        assert_eq!(percentile(&sorted, 100.0), 10.0);
    }

    #[test]
    fn percentile_empty() {
        assert_eq!(percentile(&[], 50.0), 0.0);
    }

    #[tokio::test]
    async fn load_test_transport_error_counted_as_transport_failure() {
        struct FailingExecutor;
        #[async_trait::async_trait]
        impl HttpExecutor for FailingExecutor {
            async fn execute(&self, _: &HttpRequest) -> rocket_shared::error::DomainResult<HttpResponse> {
                Err(rocket_shared::error::DomainError::Internal("simulated failure".into()))
            }
        }
        let executor: Arc<dyn HttpExecutor> = Arc::new(FailingExecutor);
        let config = LoadTestConfig { concurrency: 2, total_requests: 5, interval_ms: 0, duration_cap_secs: None };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.total_requests, 5);
        assert_eq!(result.failed, 5);
        assert_eq!(result.failed_transport, 5);
        assert_eq!(result.failed_status, 0);
        assert_eq!(result.succeeded, 0);
    }

    #[tokio::test]
    async fn load_test_single_request() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor);
        let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0, duration_cap_secs: None };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.total_requests, 1);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 0);
        // p50 == p99 == the single sample
        assert_eq!(result.p50_latency_ms, result.p99_latency_ms);
    }

    #[tokio::test]
    async fn load_test_4xx_counts_as_failed_status() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(StatusExecutor(404));
        let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0, duration_cap_secs: None };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed_status, 1);
        assert_eq!(result.failed_transport, 0);
        assert_eq!(result.failed, 1);
        // 4xx latency IS included in the latency distribution.
        assert!(result.avg_latency_ms >= 5.0);
    }

    #[tokio::test]
    async fn load_test_5xx_counts_as_failed_status() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(StatusExecutor(502));
        let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0, duration_cap_secs: None };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.failed_status, 1);
        assert_eq!(result.failed_transport, 0);
        assert_eq!(result.failed, 1);
        assert_eq!(result.succeeded, 0);
    }

    #[tokio::test]
    async fn load_test_3xx_counts_as_success() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(StatusExecutor(301));
        let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0, duration_cap_secs: None };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.failed_status, 0);
        assert_eq!(result.failed_transport, 0);
        // 3xx latency is recorded in the distribution just like 2xx.
        assert!(result.avg_latency_ms >= 5.0);
    }

    #[tokio::test]
    async fn load_test_mixed_outcomes_stats() {
        // An executor that alternates between 200 (success) and 500 (status fail).
        // Success responses report 10ms, failure responses report 20ms — different
        // enough that we can distinguish which outcomes contributed to the stats.
        struct AlternatingExecutor {
            counter: std::sync::atomic::AtomicUsize,
        }

        #[async_trait]
        impl HttpExecutor for AlternatingExecutor {
            async fn execute(&self, _: &HttpRequest) -> DomainResult<HttpResponse> {
                let n = self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let (status, duration_ms) = if n % 2 == 0 { (200, 10) } else { (500, 20) };
                Ok(HttpResponse {
                    status,
                    status_text: "".into(),
                    headers: vec![],
                    body: "".into(),
                    duration_ms,
                    ttfb_ms: duration_ms,
                    size_bytes: 0,
                })
            }
        }

        let executor: Arc<dyn HttpExecutor> = Arc::new(AlternatingExecutor {
            counter: std::sync::atomic::AtomicUsize::new(0),
        });
        let config = LoadTestConfig { concurrency: 1, total_requests: 10, interval_ms: 0, duration_cap_secs: None };
        let result = run_load_test(executor, &test_request(), &config).await;

        assert_eq!(result.total_requests, 10);
        assert_eq!(result.succeeded, 5);
        assert_eq!(result.failed_status, 5);
        assert_eq!(result.failed_transport, 0);
        assert_eq!(result.failed, 5);

        // Both outcome classes contribute to the latency distribution.
        // Min latency comes from the 200s (10ms), max from the 500s (20ms).
        assert_eq!(result.min_latency_ms, 10.0);
        assert_eq!(result.max_latency_ms, 20.0);
        // Average is (5*10 + 5*20) / 10 = 15.0
        assert!((result.avg_latency_ms - 15.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn load_test_interval_spacing_lower_bound() {
        // With interval=50ms, total=3, concurrency=1, the spawn loop sleeps
        // between iterations 0→1 and 1→2 (but not after the last), so the
        // total duration is at least 2 * 50ms = 100ms.
        let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor);
        let config = LoadTestConfig {
            concurrency: 1,
            total_requests: 3,
            interval_ms: 50,
            duration_cap_secs: None,
        };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.succeeded, 3);
        assert!(
            result.total_duration_ms >= 100.0,
            "expected >= 100ms, got {}",
            result.total_duration_ms
        );
    }

    #[test]
    fn config_v2_max_concurrency() {
        let config = LoadTestConfigV2 {
            phases: vec![
                LoadTestPhase { kind: PhaseKind::RampUp, duration_secs: 10, target: PhaseTarget::Concurrency(25) },
                LoadTestPhase { kind: PhaseKind::Hold,   duration_secs: 40, target: PhaseTarget::Concurrency(25) },
                LoadTestPhase { kind: PhaseKind::RampDown, duration_secs: 10, target: PhaseTarget::Concurrency(0) },
            ],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 5000,
        };
        assert_eq!(config.max_concurrency(), 25);
        assert_eq!(config.total_duration_secs(), 60);
    }

    #[test]
    fn config_v2_empty_phases() {
        let config = LoadTestConfigV2 {
            phases: vec![],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 5000,
        };
        assert_eq!(config.max_concurrency(), 1);
        assert_eq!(config.total_duration_secs(), 0);
    }

    #[test]
    fn success_rule_default_is_400() {
        let rule = SuccessRule::default();
        assert_eq!(rule.status_below, 400);
    }

    #[test]
    fn legacy_target_concurrency_field_still_deserializes() {
        let json = r#"{
            "kind": "Hold",
            "durationSecs": 30,
            "targetConcurrency": 25
        }"#;
        let phase: LoadTestPhase = serde_json::from_str(json).unwrap();
        assert_eq!(phase.duration_secs, 30);
        assert_eq!(phase.target, PhaseTarget::Concurrency(25));
    }

    #[test]
    fn new_target_field_deserializes() {
        let json = r#"{
            "kind": "Hold",
            "durationSecs": 30,
            "target": { "kind": "rps", "value": 200 }
        }"#;
        let phase: LoadTestPhase = serde_json::from_str(json).unwrap();
        assert_eq!(phase.target, PhaseTarget::Rps(200));
    }

    #[test]
    fn missing_both_target_fields_fails() {
        let json = r#"{ "kind": "Hold", "durationSecs": 30 }"#;
        let res: Result<LoadTestPhase, _> = serde_json::from_str(json);
        assert!(res.is_err());
    }

    #[test]
    fn phase_target_serializes_with_new_shape() {
        let phase = LoadTestPhase {
            kind: PhaseKind::Hold,
            duration_secs: 30,
            target: PhaseTarget::Concurrency(10),
        };
        let json = serde_json::to_string(&phase).unwrap();
        assert!(json.contains("\"target\":{\"kind\":\"concurrency\",\"value\":10}"));
        assert!(!json.contains("targetConcurrency"));
    }

    #[test]
    fn phase_kind_roundtrips_serde() {
        let json = serde_json::to_string(&PhaseKind::RampUp).unwrap();
        let back: PhaseKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, PhaseKind::RampUp);
    }

    #[test]
    fn request_log_entry_status_none_roundtrips() {
        let entry = RequestLogEntry {
            seq: 1,
            status: None,
            latency_ms: 0.0,
            response_bytes: 0,
            error: Some("connection refused".into()),
            phase_index: 0,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: RequestLogEntry = serde_json::from_str(&json).unwrap();
        assert!(back.status.is_none());
        assert_eq!(back.error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn uniform_target_unit_all_concurrency() {
        let cfg = LoadTestConfigV2 {
            phases: vec![
                LoadTestPhase { kind: PhaseKind::RampUp, duration_secs: 10, target: PhaseTarget::Concurrency(0) },
                LoadTestPhase { kind: PhaseKind::Hold,   duration_secs: 10, target: PhaseTarget::Concurrency(10) },
            ],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 100,
        };
        assert!(cfg.has_uniform_target_unit());
        assert_eq!(cfg.target_unit(), Some(TargetUnit::Concurrency));
    }

    #[test]
    fn uniform_target_unit_all_rps() {
        let cfg = LoadTestConfigV2 {
            phases: vec![
                LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 10, target: PhaseTarget::Rps(50) },
                LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 10, target: PhaseTarget::Rps(100) },
            ],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 100,
        };
        assert!(cfg.has_uniform_target_unit());
        assert_eq!(cfg.target_unit(), Some(TargetUnit::Rps));
    }

    #[test]
    fn uniform_target_unit_mixed_rejected() {
        let cfg = LoadTestConfigV2 {
            phases: vec![
                LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 10, target: PhaseTarget::Concurrency(10) },
                LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 10, target: PhaseTarget::Rps(50) },
            ],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 100,
        };
        assert!(!cfg.has_uniform_target_unit());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rate_driver_holds_target_rate() {
        let driver = Arc::new(RateDriver::new(100, 100));
        let _refill = driver.spawn_refill();

        let start = std::time::Instant::now();
        let mut count = 0u32;
        while start.elapsed() < Duration::from_millis(500) {
            driver.acquire().await;
            count += 1;
        }
        // 100 rps × 0.5 s = 50, allow ±25% tolerance for scheduling jitter.
        assert!(count >= 35, "got only {count} requests in 500ms at 100rps");
        assert!(count <= 80, "got {count} requests in 500ms at 100rps (over-firing)");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rate_driver_respects_zero_rate() {
        let driver = Arc::new(RateDriver::new(0, 100));
        let _refill = driver.spawn_refill();

        // With rate=0, a single acquire should never resolve. Race it against a
        // 200 ms timeout — the timeout must win.
        let acquired = tokio::time::timeout(Duration::from_millis(200), driver.acquire()).await;
        assert!(acquired.is_err(), "acquire returned at rate=0");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rate_driver_set_rate_updates_throughput() {
        let driver = Arc::new(RateDriver::new(10, 200));
        let _refill = driver.spawn_refill();

        // Drain ~5 tokens at 10 rps (~500 ms-worth of cap)
        for _ in 0..5 {
            driver.acquire().await;
        }
        driver.set_rate(200);

        let start = std::time::Instant::now();
        let mut count = 0u32;
        while start.elapsed() < Duration::from_millis(300) {
            driver.acquire().await;
            count += 1;
        }
        // 200 rps × 0.3 s = 60. Allow generous tolerance.
        assert!(count >= 30, "set_rate(200) yielded only {count} acquires in 300ms");
    }

    #[test]
    fn load_test_result_new_fields_default_on_missing() {
        let json = r#"{
            "totalRequests": 10,
            "succeeded": 10,
            "failed": 0,
            "failedTransport": 0,
            "failedStatus": 0,
            "minLatencyMs": 1.0,
            "avgLatencyMs": 2.0,
            "p50LatencyMs": 2.0,
            "p95LatencyMs": 3.0,
            "p99LatencyMs": 3.5,
            "maxLatencyMs": 4.0,
            "requestsPerSecond": 5.0,
            "totalDurationMs": 2000.0
        }"#;
        let result: LoadTestResult = serde_json::from_str(json).unwrap();
        assert!(result.phase_timeline.is_empty());
        assert!(result.request_log.is_empty());
        assert!(result.time_series.is_empty());
    }

    #[tokio::test]
    async fn run_load_test_v2_hold_phase_counts() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor);
        let config = LoadTestConfigV2 {
            phases: vec![LoadTestPhase {
                kind: PhaseKind::Hold,
                duration_secs: 1,
                target: PhaseTarget::Concurrency(3),
            }],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 1000,
        };
        let result = run_load_test_v2(executor, &test_request(), &config, None).await;
        assert!(result.total_requests > 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.succeeded, result.total_requests);
        assert!(!result.time_series.is_empty());
        assert_eq!(result.request_log.len() as u32, result.total_requests);
    }

    #[tokio::test]
    async fn run_load_test_v2_status_fail_classified_correctly() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(StatusExecutor(503));
        let config = LoadTestConfigV2 {
            phases: vec![LoadTestPhase {
                kind: PhaseKind::Hold,
                duration_secs: 1,
                target: PhaseTarget::Concurrency(1),
            }],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 100,
        };
        let result = run_load_test_v2(executor, &test_request(), &config, None).await;
        assert!(result.total_requests > 0);
        assert_eq!(result.succeeded, 0);
        assert_eq!(result.failed_status, result.total_requests);
        assert_eq!(result.failed_transport, 0);
    }

    #[tokio::test]
    async fn load_test_interval_zero_no_delay() {
        // Regression: interval_ms=0 should match pre-interval behaviour.
        // total=10 fast requests should finish well under 500ms.
        let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor);
        let config = LoadTestConfig {
            concurrency: 10,
            total_requests: 10,
            interval_ms: 0,
            duration_cap_secs: None,
        };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.succeeded, 10);
        assert!(
            result.total_duration_ms < 500.0,
            "expected < 500ms, got {}",
            result.total_duration_ms
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_load_test_v2_rps_mode_hits_target_rate() {
        // Hold 50 rps for 1 second; expect roughly 50 ± 20 completed requests.
        let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor); // 10ms latency
        let config = LoadTestConfigV2 {
            phases: vec![LoadTestPhase {
                kind: PhaseKind::Hold,
                duration_secs: 1,
                target: PhaseTarget::Rps(50),
            }],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 1000,
        };
        assert!(config.has_uniform_target_unit());
        assert_eq!(config.target_unit(), Some(TargetUnit::Rps));

        let result = run_load_test_v2(executor, &test_request(), &config, None).await;
        assert!(result.total_requests >= 30,
            "expected ≥30 requests at 50rps for 1s, got {}", result.total_requests);
        assert!(result.total_requests <= 75,
            "expected ≤75 requests at 50rps for 1s (over-firing), got {}", result.total_requests);
        assert_eq!(result.failed, 0);
        assert_eq!(result.succeeded, result.total_requests);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_load_test_v2_rps_zero_phase_emits_nothing() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor);
        let config = LoadTestConfigV2 {
            phases: vec![LoadTestPhase {
                kind: PhaseKind::Hold,
                duration_secs: 1,
                target: PhaseTarget::Rps(0),
            }],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 100,
        };
        let result = run_load_test_v2(executor, &test_request(), &config, None).await;
        assert_eq!(result.total_requests, 0);
    }
}
