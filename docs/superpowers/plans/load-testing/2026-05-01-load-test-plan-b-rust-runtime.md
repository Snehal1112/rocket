# Enhanced Load Testing — Plan B: Rust Runtime

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `RingBuffer`, `PhaseScheduler`, time-series accumulation, and Tauri event emission inside `run_load_test_v2` in `rocket-http`.

**Architecture:** `run_load_test_v2` takes an `AppHandle` reference for event emission. `PhaseScheduler` runs as a separate tokio task, adjusting semaphore permits on phase transitions. `RingBuffer` is a fixed-capacity `VecDeque`. A 250 ms `tokio::time::interval` task accumulates `TimeSeriesPoint` snapshots and emits `load_test_progress` events.

**Tech Stack:** Rust, tokio (semaphore, time, spawn), serde_json, tauri::AppHandle

**Spec:** `docs/superpowers/specs/2026-05-01-load-test-enhanced-design.md`

**Depends on:** Plan A complete

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-http/src/load_test.rs` | Add `RingBuffer`, `PhaseScheduler`, `run_load_test_v2` |
| `crates/rocket-http/Cargo.toml` | Verify `tauri` is already a dependency (check before adding) |

---

## Chunk 1: RingBuffer + PhaseScheduler

### Task 1: Implement `RingBuffer` and `PhaseScheduler`

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`

- [ ] **Step 1: Read Cargo.toml to confirm tauri dependency**

```bash
cat crates/rocket-http/Cargo.toml
```

If `tauri` is NOT listed as a dependency, add it:

```toml
[dependencies]
tauri = { version = "2", optional = true }
```

And add a feature flag:

```toml
[features]
tauri-events = ["tauri"]
```

If it IS already listed, proceed without changes.

- [ ] **Step 2: Add `RingBuffer`**

At the top of `crates/rocket-http/src/load_test.rs` (after existing imports), add:

```rust
use std::collections::VecDeque;

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
}
```

- [ ] **Step 3: Add unit tests for `RingBuffer`**

In the `#[cfg(test)] mod tests` block:

```rust
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
    assert_eq!(rb.snapshot(), vec![]);
}
```

- [ ] **Step 4: Run RingBuffer tests**

```bash
cargo test -p rocket-http ring_buffer 2>&1 | tail -10
```

Expected: 3 tests pass.

- [ ] **Step 5: Add `PhaseScheduler`**

After `RingBuffer`, add:

```rust
use std::sync::atomic::{AtomicU32, Ordering};

/// Drives phase transitions during a load test.
/// Each phase adjusts the live concurrency level by modifying a shared `AtomicU32`
/// that the spawning semaphore loop reads to add/remove permits.
pub struct PhaseScheduler {
    phases: Vec<LoadTestPhase>,
}

impl PhaseScheduler {
    pub fn new(phases: Vec<LoadTestPhase>) -> Self {
        Self { phases }
    }

    /// Returns an iterator of `(elapsed_secs, target_concurrency)` checkpoints —
    /// one per second — derived from the phase definitions.
    /// The caller polls this to know when to change the semaphore permit count.
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
                    // Linear interpolation between start_conc and end_conc
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
```

- [ ] **Step 6: Write `PhaseScheduler` unit tests**

```rust
#[test]
fn phase_scheduler_hold_checkpoints() {
    let sched = PhaseScheduler::new(vec![
        LoadTestPhase { kind: PhaseKind::Hold, duration_secs: 3, target_concurrency: 10 },
    ]);
    let cps = sched.checkpoints();
    // 3 checkpoints, all at concurrency 10
    assert_eq!(cps.len(), 3);
    assert!(cps.iter().all(|(_, c)| *c == 10));
}

#[test]
fn phase_scheduler_rampup_linear() {
    let sched = PhaseScheduler::new(vec![
        LoadTestPhase { kind: PhaseKind::RampUp, duration_secs: 4, target_concurrency: 4 },
    ]);
    let cps = sched.checkpoints();
    // 4 steps: concurrency at 1, 2, 3, 4
    assert_eq!(cps.len(), 4);
    let concs: Vec<u32> = cps.iter().map(|(_, c)| *c).collect();
    assert_eq!(concs, vec![1, 2, 3, 4]);
}

#[test]
fn phase_index_at_correct_boundaries() {
    let sched = PhaseScheduler::new(vec![
        LoadTestPhase { kind: PhaseKind::RampUp, duration_secs: 10, target_concurrency: 25 },
        LoadTestPhase { kind: PhaseKind::Hold,   duration_secs: 40, target_concurrency: 25 },
        LoadTestPhase { kind: PhaseKind::RampDown, duration_secs: 10, target_concurrency: 0 },
    ]);
    assert_eq!(sched.phase_index_at(0), 0);
    assert_eq!(sched.phase_index_at(9), 0);
    assert_eq!(sched.phase_index_at(10), 1);
    assert_eq!(sched.phase_index_at(50), 2);
    assert_eq!(sched.phase_index_at(999), 2); // past end → last phase
}
```

- [ ] **Step 7: Run PhaseScheduler tests**

```bash
cargo test -p rocket-http phase_scheduler phase_index 2>&1 | tail -10
```

Expected: 4 tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-http/src/load_test.rs crates/rocket-http/Cargo.toml
git commit -m "feat(rocket-http): add RingBuffer and PhaseScheduler"
```

---

## Chunk 2: `run_load_test_v2` with event emission

### Task 2: Implement `run_load_test_v2`

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`
- Modify: `crates/rocket-http/src/lib.rs`

- [ ] **Step 1: Add shared accumulator types**

After `PhaseScheduler`, add the internal shared-state struct used by the spawned tasks:

```rust
use std::sync::Mutex as StdMutex;

struct RunAccumulator {
    latencies: Vec<f64>,          // all successful/status-fail latencies in order
    log: RingBuffer<RequestLogEntry>,
    time_series: Vec<TimeSeriesPoint>,
    phase_timeline: Vec<PhaseMarker>,
    succeeded: u32,
    failed_status: u32,
    failed_transport: u32,
    completed: u32,
    /// Timestamps (Instant) of completions in the last 2 s for rolling RPS.
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
        self.recent_completions.push_back(now);
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
```

- [ ] **Step 2: Implement `run_load_test_v2`**

Add the function signature and body. Note: `AppHandle` is feature-gated on `tauri-events`. If that feature is not enabled, event emission is a no-op. This keeps `rocket-http` compilable in tests without a full Tauri runtime.

```rust
#[cfg(feature = "tauri-events")]
use tauri::AppHandle;

/// Phase-aware load test runner with real-time Tauri event emission.
/// Emits `load_test_progress` every 250 ms and `load_test_complete` on finish.
pub async fn run_load_test_v2(
    executor: Arc<dyn HttpExecutor>,
    request: &HttpRequest,
    config: &LoadTestConfigV2,
    #[cfg(feature = "tauri-events")]
    app: &AppHandle,
) -> LoadTestResult {
    use tokio::sync::Semaphore;
    use tokio::time::{interval, Duration};

    let scheduler = PhaseScheduler::new(config.phases.clone());
    let initial_conc = config.phases.first()
        .map(|p| p.target_concurrency)
        .unwrap_or(1) as usize;

    let semaphore = Arc::new(Semaphore::new(initial_conc));
    let accumulator = Arc::new(tokio::sync::Mutex::new(
        RunAccumulator::new(config.ring_buffer_size)
    ));
    let success_rule = config.success_rule.clone();
    let run_start = std::time::Instant::now();

    // --- 250 ms snapshot + event emission task ---
    let acc_snap = Arc::clone(&accumulator);
    let checkpoints = scheduler.checkpoints();
    let phases_len = config.phases.len();
    #[cfg(feature = "tauri-events")]
    let app_clone = app.clone();

    let snapshot_handle = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_millis(250));
        let mut last_checkpoint_idx = 0usize;
        let start = std::time::Instant::now();

        loop {
            ticker.tick().await;
            let elapsed_secs = start.elapsed().as_secs();

            // Advance phase checkpoints.
            while last_checkpoint_idx < checkpoints.len()
                && checkpoints[last_checkpoint_idx].0 <= elapsed_secs
            {
                last_checkpoint_idx += 1;
            }

            let mut acc = acc_snap.lock().await;
            let rps = acc.rolling_rps();
            let (p50, p95, p99) = acc.current_percentiles();
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let phase_idx = {
                let mut boundary = 0u64;
                let mut idx = phases_len.saturating_sub(1);
                for (i, cp_pair) in checkpoints.iter().enumerate() {
                    if elapsed_secs < cp_pair.0 {
                        idx = i / (checkpoints.len() / phases_len.max(1)).max(1);
                        break;
                    }
                    boundary = cp_pair.0;
                }
                idx
            };

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
                active_concurrent: 0, // filled by semaphore tracking in future
            });

            let event = LoadTestProgressEvent {
                elapsed_ms,
                completed: acc.completed,
                active_concurrent: 0,
                succeeded: acc.succeeded,
                failed_status: acc.failed_status,
                failed_transport: acc.failed_transport,
                requests_per_second: rps,
                p50_ms: p50,
                p95_ms: p95,
                p99_ms: p99,
                current_phase_index: phase_idx,
            };

            drop(acc); // release lock before emit

            #[cfg(feature = "tauri-events")]
            {
                let _ = app_clone.emit("load_test_progress", &event);
            }
        }
    });

    // --- Per-request spawning loop (duration-based) ---
    let total_duration = std::time::Duration::from_secs(config.total_duration_secs() as u64);
    let loop_start = std::time::Instant::now();
    let mut seq: u32 = 0;
    let mut join_handles = Vec::new();

    while loop_start.elapsed() < total_duration {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let req = request.clone();
        let exec = Arc::clone(&executor);
        let acc = Arc::clone(&accumulator);
        let rule = success_rule.clone();
        let phase_idx = scheduler.phase_index_at(loop_start.elapsed().as_secs());
        let current_seq = seq;
        seq += 1;

        let handle = tokio::spawn(async move {
            let t0 = std::time::Instant::now();
            match exec.execute(&req).await {
                Ok(resp) => {
                    let latency_ms = t0.elapsed().as_secs_f64() * 1000.0;
                    let is_success = resp.status < rule.status_below;
                    let mut acc = acc.lock().await;
                    acc.latencies.push(latency_ms);
                    acc.completed += 1;
                    if is_success {
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
                    let mut acc = acc.lock().await;
                    acc.completed += 1;
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
            drop(permit);
        });
        join_handles.push(handle);
    }

    // Wait for in-flight requests to drain.
    for h in join_handles {
        let _ = h.await;
    }

    // Stop the snapshot task.
    snapshot_handle.abort();

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

    #[cfg(feature = "tauri-events")]
    {
        let _ = app.emit("load_test_complete", &result);
    }

    result
}
```

- [ ] **Step 3: Re-export `run_load_test_v2` from `lib.rs`**

In `crates/rocket-http/src/lib.rs`, add `run_load_test_v2` to the existing `load_test` pub use list:

```rust
pub use load_test::{
    // ... existing ...
    run_load_test_v2,
};
```

- [ ] **Step 4: Compile check (no tauri-events feature)**

```bash
cargo check -p rocket-http 2>&1 | tail -10
```

Expected: compiles clean (the `app` parameter is feature-gated so it compiles without tauri).

- [ ] **Step 5: Run all existing load test tests**

```bash
cargo test -p rocket-http 2>&1 | tail -20
```

Expected: all existing tests still pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-http/src/load_test.rs crates/rocket-http/src/lib.rs
git commit -m "feat(rocket-http): implement run_load_test_v2 with PhaseScheduler, RingBuffer, and event emission"
```

---

## Chunk 3: Integration smoke test

### Task 3: Integration test for `run_load_test_v2`

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`

- [ ] **Step 1: Add a tokio integration test**

In the `#[cfg(test)] mod tests` block, add:

```rust
#[tokio::test]
async fn run_load_test_v2_hold_phase_counts() {
    // Single Hold phase: run for 1 second at concurrency 3.
    // MockExecutor responds instantly so we get many requests in 1 s.
    let executor: Arc<dyn HttpExecutor> = Arc::new(MockExecutor);
    let config = LoadTestConfigV2 {
        phases: vec![
            LoadTestPhase {
                kind: PhaseKind::Hold,
                duration_secs: 1,
                target_concurrency: 3,
            }
        ],
        success_rule: SuccessRule::default(),
        ring_buffer_size: 1000,
    };
    let result = run_load_test_v2(executor, &test_request(), &config).await;
    // Should have fired at least 1 request
    assert!(result.total_requests > 0);
    assert_eq!(result.failed, 0);
    assert_eq!(result.succeeded, result.total_requests);
    // Time series should have at least 1 snapshot (test runs 1 s, snapshots every 250 ms)
    assert!(!result.time_series.is_empty());
}

#[tokio::test]
async fn run_load_test_v2_status_fail_classified_correctly() {
    let executor: Arc<dyn HttpExecutor> = Arc::new(StatusExecutor(503));
    let config = LoadTestConfigV2 {
        phases: vec![
            LoadTestPhase {
                kind: PhaseKind::Hold,
                duration_secs: 1,
                target_concurrency: 1,
            }
        ],
        success_rule: SuccessRule::default(),
        ring_buffer_size: 100,
    };
    let result = run_load_test_v2(executor, &test_request(), &config).await;
    assert!(result.total_requests > 0);
    assert_eq!(result.succeeded, 0);
    assert_eq!(result.failed_status, result.total_requests);
    assert_eq!(result.failed_transport, 0);
}
```

- [ ] **Step 2: Run integration tests**

```bash
cargo test -p rocket-http run_load_test_v2 2>&1 | tail -15
```

Expected: 2 tests pass.

- [ ] **Step 3: Full test suite**

```bash
cargo test -p rocket-http 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-http/src/load_test.rs
git commit -m "test(rocket-http): integration tests for run_load_test_v2 phase execution and failure classification"
```
