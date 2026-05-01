use crate::{HttpExecutor, HttpRequest};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

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
            let end_conc = phase.target_concurrency;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum PhaseKind {
    RampUp,
    Hold,
    RampDown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestPhase {
    pub kind: PhaseKind,
    pub duration_secs: u32,
    pub target_concurrency: u32,
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
        self.phases.iter().map(|p| p.target_concurrency).max().unwrap_or(1)
    }

    /// Returns total planned duration in seconds.
    pub fn total_duration_secs(&self) -> u32 {
        self.phases.iter().map(|p| p.duration_secs).sum()
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
/// Drives a `PhaseScheduler` whose per-second checkpoints reshape semaphore
/// capacity at phase boundaries, accumulates a `RingBuffer<RequestLogEntry>`,
/// and, when an `AppHandle` is provided, emits `load_test_progress`
/// every 250 ms and `load_test_complete` on finish.
pub async fn run_load_test_v2(
    executor: Arc<dyn HttpExecutor>,
    request: &HttpRequest,
    config: &LoadTestConfigV2,
    app: Option<&AppHandle>,
) -> LoadTestResult {
    use tokio::time::interval;

    let scheduler = Arc::new(PhaseScheduler::new(config.phases.clone()));
    let initial_conc = config
        .phases
        .first()
        .map(|p| p.target_concurrency)
        .unwrap_or(1)
        .max(1) as usize;

    let semaphore = Arc::new(Semaphore::new(initial_conc));
    let current_permits = Arc::new(AtomicU32::new(initial_conc as u32));
    let active_concurrent = Arc::new(AtomicU32::new(0));
    let accumulator = Arc::new(tokio::sync::Mutex::new(RunAccumulator::new(
        config.ring_buffer_size,
    )));
    let success_rule = config.success_rule.clone();
    let run_start = std::time::Instant::now();

    // --- Phase scheduler task: rewrites semaphore capacity each second. ---
    let phase_sem = Arc::clone(&semaphore);
    let phase_perm = Arc::clone(&current_permits);
    let checkpoints = scheduler.checkpoints();
    let phase_handle = tokio::spawn(async move {
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
                let to_remove = (current - target_conc) as u32;
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
    });

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
        let permit = semaphore.clone().acquire_owned().await.unwrap();
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
            drop(permit);
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

/// Fires `config.total_requests` concurrent HTTP requests, bounded by `config.concurrency`,
/// then returns aggregated latency statistics.
pub async fn run_load_test(
    executor: Arc<dyn HttpExecutor>,
    request: &HttpRequest,
    config: &LoadTestConfig,
) -> LoadTestResult {
    let semaphore = Arc::new(Semaphore::new(config.concurrency as usize));
    let total = config.total_requests as usize;
    let start = std::time::Instant::now();

    let mut handles = Vec::with_capacity(total);
    for i in 0..total {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let req = request.clone();
        let exec = executor.clone();
        let handle = tokio::spawn(async move {
            let result = exec.execute(&req).await;
            drop(permit);
            match result {
                Ok(resp) => {
                    let ms = resp.duration_ms as f64;
                    if resp.status < 400 {
                        Outcome::Success(ms)
                    } else {
                        Outcome::StatusFail(ms)
                    }
                }
                Err(_) => Outcome::TransportFail,
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

    for handle in handles {
        match handle.await {
            Ok(Outcome::Success(ms)) => {
                succeeded += 1;
                latencies.push(ms);
            }
            Ok(Outcome::StatusFail(ms)) => {
                failed_status += 1;
                latencies.push(ms);
            }
            Ok(Outcome::TransportFail) => {
                failed_transport += 1;
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
        total_requests: config.total_requests,
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
            (succeeded as f64) / (total_duration_ms / 1000.0)
        } else {
            0.0
        },
        total_duration_ms,
        phase_timeline: vec![],
        request_log: vec![],
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
            target_concurrency: 10,
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
            target_concurrency: 4,
        }]);
        let cps = sched.checkpoints();
        assert_eq!(cps.len(), 4);
        let concs: Vec<u32> = cps.iter().map(|(_, c)| *c).collect();
        assert_eq!(concs, vec![1, 2, 3, 4]);
    }

    #[test]
    fn phase_index_at_correct_boundaries() {
        let sched = PhaseScheduler::new(vec![
            LoadTestPhase { kind: PhaseKind::RampUp, duration_secs: 10, target_concurrency: 25 },
            LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 40, target_concurrency: 25 },
            LoadTestPhase { kind: PhaseKind::RampDown, duration_secs: 10, target_concurrency: 0 },
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
        let config = LoadTestConfig { concurrency: 2, total_requests: 5, interval_ms: 0 };
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
        let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0 };
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
        let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0 };
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
        let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0 };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.failed_status, 1);
        assert_eq!(result.failed_transport, 0);
        assert_eq!(result.failed, 1);
        assert_eq!(result.succeeded, 0);
    }

    #[tokio::test]
    async fn load_test_3xx_counts_as_success() {
        let executor: Arc<dyn HttpExecutor> = Arc::new(StatusExecutor(301));
        let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0 };
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
        let config = LoadTestConfig { concurrency: 1, total_requests: 10, interval_ms: 0 };
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
                LoadTestPhase { kind: PhaseKind::RampUp, duration_secs: 10, target_concurrency: 25 },
                LoadTestPhase { kind: PhaseKind::Hold,   duration_secs: 40, target_concurrency: 25 },
                LoadTestPhase { kind: PhaseKind::RampDown, duration_secs: 10, target_concurrency: 0 },
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
                target_concurrency: 3,
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
                target_concurrency: 1,
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
        };
        let result = run_load_test(executor, &test_request(), &config).await;
        assert_eq!(result.succeeded, 10);
        assert!(
            result.total_duration_ms < 500.0,
            "expected < 500ms, got {}",
            result.total_duration_ms
        );
    }
}
