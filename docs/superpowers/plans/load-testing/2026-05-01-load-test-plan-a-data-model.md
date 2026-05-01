# Enhanced Load Testing — Plan A: Rust Data Model

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat `LoadTestConfig` with phase-based types and add all new result/event structs in `rocket-http`.

**Architecture:** All new types live in `crates/rocket-http/src/load_test.rs`. They extend (not replace) existing types so the existing `run_load_test` function signature stays compilable until Plan B rewrites it.

**Tech Stack:** Rust, serde, tokio

**Spec:** `docs/superpowers/specs/2026-05-01-load-test-enhanced-design.md`

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-http/src/load_test.rs` | Add new types alongside existing ones |
| `crates/rocket-http/src/lib.rs` | Re-export new public types |

---

## Chunk 1: Phase config + progress event types

### Task 1: Add `LoadTestPhase`, `PhaseKind`, `SuccessRule`, extended `LoadTestConfig`

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`

- [ ] **Step 1: Read current `load_test.rs`**

```bash
cat crates/rocket-http/src/load_test.rs
```

Note the exact names of existing structs (`LoadTestConfig`, `LoadTestResult`, etc.) — do not rename them. We will add new types alongside.

- [ ] **Step 2: Add phase-based config types**

In `crates/rocket-http/src/load_test.rs`, **above** the existing `LoadTestConfig` struct, insert:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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
```

- [ ] **Step 3: Write unit tests for new config types**

In `crates/rocket-http/src/load_test.rs`, inside the existing `#[cfg(test)] mod tests` block, add:

```rust
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
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p rocket-http config_v2 success_rule phase_kind 2>&1 | tail -20
```

Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-http/src/load_test.rs
git commit -m "feat(rocket-http): add LoadTestPhase, PhaseKind, SuccessRule, LoadTestConfigV2 types"
```

---

## Chunk 2: Event and result types

### Task 2: Add `LoadTestProgressEvent`, `RequestLogEntry`, `TimeSeriesPoint`, `PhaseMarker`, extend `LoadTestResult`

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`
- Modify: `crates/rocket-http/src/lib.rs`

- [ ] **Step 1: Add progress event type**

In `crates/rocket-http/src/load_test.rs`, after the `LoadTestConfigV2` block, add:

```rust
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
```

- [ ] **Step 2: Add time-series and log types**

Directly after `LoadTestProgressEvent`, add:

```rust
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
```

- [ ] **Step 3: Extend `LoadTestResult` with new fields**

Find the existing `LoadTestResult` struct in `load_test.rs`. Add three new fields at the end (the existing fields must not change names — they are stable IPC types):

```rust
// Append to the EXISTING LoadTestResult struct:
    pub phase_timeline: Vec<PhaseMarker>,
    pub request_log: Vec<RequestLogEntry>,
    pub time_series: Vec<TimeSeriesPoint>,
```

Also add `#[serde(default)]` to each new field so existing Tauri callers that produce a `LoadTestResult` without these fields still deserialize correctly:

```rust
    #[serde(default)]
    pub phase_timeline: Vec<PhaseMarker>,
    #[serde(default)]
    pub request_log: Vec<RequestLogEntry>,
    #[serde(default)]
    pub time_series: Vec<TimeSeriesPoint>,
```

- [ ] **Step 4: Re-export from `lib.rs`**

In `crates/rocket-http/src/lib.rs`, find the existing `load_test` re-exports (e.g. `pub use load_test::{LoadTestConfig, LoadTestResult, run_load_test}`). Add the new types:

```rust
pub use load_test::{
    LoadTestConfig,         // existing
    LoadTestConfigV2,       // new
    LoadTestPhase,          // new
    LoadTestProgressEvent,  // new
    LoadTestResult,
    PhaseKind,              // new
    PhaseMarker,            // new
    RequestLogEntry,        // new
    SuccessRule,            // new
    TimeSeriesPoint,        // new
    run_load_test,
};
```

- [ ] **Step 5: Write serde roundtrip tests for new result types**

In the `#[cfg(test)] mod tests` block of `load_test.rs`, add:

```rust
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
    // A JSON blob that looks like the OLD LoadTestResult (no new fields).
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
```

- [ ] **Step 6: Full compile + test**

```bash
cargo check -p rocket-http && cargo test -p rocket-http 2>&1 | tail -20
```

Expected: compiles clean, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-http/src/load_test.rs crates/rocket-http/src/lib.rs
git commit -m "feat(rocket-http): add LoadTestProgressEvent, TimeSeriesPoint, RequestLogEntry, PhaseMarker; extend LoadTestResult"
```

---

## Chunk 3: CLAUDE.md update

### Task 3: Update `rocket-http` CLAUDE.md

**Files:**
- Modify: `crates/rocket-http/CLAUDE.md`

- [ ] **Step 1: Update the module map entry for `load_test`**

Find the `load_test` row in the Module Map table and replace it with:

```markdown
| `load_test` | Phase-based load testing harness. `run_load_test_v2()` drives a `PhaseScheduler` that adjusts semaphore permits between `RampUp / Hold / RampDown` phases, accumulates a `RingBuffer<RequestLogEntry>` (default 5 000 entries), emits `load_test_progress` Tauri events every 250 ms, and returns a `LoadTestResult` with full `time_series` and `request_log`. The legacy `run_load_test()` is kept for existing tests. |
```

- [ ] **Step 2: Add a Key Design Rules entry**

Append to the Key Design Rules section:

```markdown
- `LoadTestConfigV2` uses `#[serde(default)]` on `success_rule` and `ring_buffer_size` so callers that omit them get safe defaults (400 / 5 000). The three new fields on `LoadTestResult` (`phase_timeline`, `request_log`, `time_series`) also carry `#[serde(default)]` for backwards-compat with older Tauri call sites.
```

- [ ] **Step 3: Commit**

```bash
git add crates/rocket-http/CLAUDE.md
git commit -m "docs(rocket-http): update CLAUDE.md for phase-based load test types"
```
