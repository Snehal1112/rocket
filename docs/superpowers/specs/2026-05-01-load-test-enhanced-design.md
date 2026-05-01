# Enhanced Load Testing — Design Spec

**Date:** 2026-05-01
**Status:** Approved

---

## Problem

The current load test feature (`LoadTestDialog`) is a modal with a basic stats grid.
It lacks:
- Real-time streaming feedback while the test runs
- Time-series charts (throughput, latency, error rate, concurrency, distribution)
- Per-request log / waterfall table
- Ramp-up / hold / ramp-down phase scheduling (JMeter parity)
- A dedicated surface with enough space to show all of the above
- Post-run export in HTML, CSV, JSON, and PDF formats

---

## Goals

1. Full-screen dedicated **Load Test tab** in the request editor (alongside Params, Headers, Auth, etc.)
2. **Real-time streaming** via Tauri events — 6 live charts update every 250 ms during the run
3. **Ramp-up phase builder** — `RampUp | Hold | RampDown` phases with per-phase target concurrency and duration
4. **Per-request ring buffer** — scrollable log of the last 5 000 requests with status, latency, size, error
5. **Post-run export** — HTML (self-contained, Chart.js embedded), CSV (raw log), JSON (full result snapshot), PDF (headless render of HTML)

---

## Non-Goals

- Multi-request test scenarios (load-test a sequence of requests)
- Distributed load generation across machines
- Persistent test history / test plan files (follow-up)

---

## Architecture

```
Frontend (React)
  LoadTestTab          — config sidebar + phase builder
  useLoadTestStore     — Zustand, accumulates streaming events into time-series
  LiveDashboard        — 6 recharts panels + request log table

Tauri IPC boundary
  invoke run_load_test_command   — starts the test
  listen('load_test_progress')   — 250 ms snapshot events (during run)
  listen('load_test_complete')   — final result + full request log

Rust backend
  src-tauri/commands/load_test.rs   — thin adapter, no logic
  rocket-app LoadTestService        — variable resolution + phase orchestration
  rocket-http PhaseScheduler        — semaphore permit adjustment between phases
  rocket-http run_load_test         — extended with RingBuffer + TimeSeriesPoint
  rocket-app ExportService          — produces HTML / CSV / JSON / PDF blobs
```

---

## Data Model

### Rust types in `rocket-http` (all `#[serde(rename_all = "camelCase")]`)

```rust
// Config sent from frontend
pub struct LoadTestPhase {
    pub kind: PhaseKind,           // RampUp | Hold | RampDown
    pub duration_secs: u32,
    pub target_concurrency: u32,
}

pub enum PhaseKind { RampUp, Hold, RampDown }

pub struct LoadTestConfig {
    pub phases: Vec<LoadTestPhase>,
    pub success_rule: SuccessRule,   // StatusBelow(u16) — default 400
    pub ring_buffer_size: usize,     // default 5000
}

// Emitted every 250 ms during run
pub struct LoadTestProgressEvent {
    pub elapsed_ms: u64,
    pub completed: u32,
    pub active_concurrent: u32,
    pub succeeded: u32,
    pub failed_status: u32,
    pub failed_transport: u32,
    pub requests_per_second: f64,   // rolling 2 s window
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub current_phase_index: usize,
}

// Emitted once on completion
pub struct LoadTestResult {
    // existing fields (total_requests, succeeded, failed_*, min/avg/p50/p95/p99/max, rps, duration_ms)
    pub phase_timeline: Vec<PhaseMarker>,      // { phase_index, started_at_ms }
    pub request_log: Vec<RequestLogEntry>,     // ring buffer snapshot
    pub time_series: Vec<TimeSeriesPoint>,     // all 250 ms snapshots
}

pub struct RequestLogEntry {
    pub seq: u32,
    pub status: Option<u16>,   // None = transport fail
    pub latency_ms: f64,
    pub response_bytes: u64,
    pub error: Option<String>,
    pub phase_index: usize,
}

pub struct TimeSeriesPoint {
    pub elapsed_ms: u64,
    pub rps: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub error_rate_pct: f64,
    pub active_concurrent: u32,
}

pub struct PhaseMarker {
    pub phase_index: usize,
    pub started_at_ms: u64,
}
```

### TypeScript mirror (in `src/lib/tauri-api.ts`)

```typescript
export interface LoadTestPhase {
  kind: 'RampUp' | 'Hold' | 'RampDown';
  durationSecs: number;
  targetConcurrency: number;
}

export interface LoadTestConfig {
  phases: LoadTestPhase[];
  successRule: { statusBelow: number };
  ringBufferSize: number;
}

export interface LoadTestProgressEvent {
  elapsedMs: number;
  completed: number;
  activeConcurrent: number;
  succeeded: number;
  failedStatus: number;
  failedTransport: number;
  requestsPerSecond: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  currentPhaseIndex: number;
}

export interface RequestLogEntry {
  seq: number;
  status: number | null;
  latencyMs: number;
  responseBytes: number;
  error: string | null;
  phaseIndex: number;
}

export interface TimeSeriesPoint {
  elapsedMs: number;
  rps: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  errorRatePct: number;
  activeConcurrent: number;
}

export interface LoadTestResult {
  // existing fields...
  phaseTimeline: Array<{ phaseIndex: number; startedAtMs: number }>;
  requestLog: RequestLogEntry[];
  timeSeries: TimeSeriesPoint[];
}
```

---

## Backend Design

### `rocket-http` changes

**`load_test.rs`** — extend `run_load_test`:
- Accept `Vec<LoadTestPhase>` instead of flat `concurrency`/`total_requests`
- `PhaseScheduler` — a tokio task that sleeps between phase transitions and adjusts `Semaphore::add_permits` / reduces via `acquire_many`
- `RingBuffer<RequestLogEntry>` — fixed-size circular buffer, default 5 000
- Accumulate `TimeSeriesPoint` every 250 ms via `tokio::time::interval`
- Emit `load_test_progress` Tauri event every 250 ms via `AppHandle`
- Emit `load_test_complete` on finish with full `LoadTestResult`

### `rocket-app` changes

**`load_test_service.rs`** (new file):
- Accepts `ExecuteRequestInput` + `LoadTestConfig`
- Calls existing variable resolution path (`resolve_request`) from `RequestExecutionService`
- Delegates to `rocket_http::run_load_test` with resolved `HttpRequest` and `AppHandle`

**`export_service.rs`** (new file):
- `export_html(result: &LoadTestResult) -> String` — self-contained HTML with embedded Chart.js from cdnjs
- `export_csv(result: &LoadTestResult) -> String` — CSV of `request_log`
- `export_json(result: &LoadTestResult) -> String` — serde_json::to_string_pretty
- `export_pdf(result: &LoadTestResult, app: &AppHandle) -> DomainResult<Vec<u8>>` — use Tauri's `webview` to render the HTML and capture via `print_to_pdf` (Tauri v2 API)

### Tauri command changes

`src-tauri/src/commands/load_test.rs`:
- `run_load_test_command` — resolves variables via `LoadTestService`, fires test, returns `()`; result arrives via event
- `export_load_test_command` — takes `LoadTestResult` + `format: ExportFormat`, calls `ExportService`, writes file via `save_file_dialog`

---

## Frontend Design

### New files

| File | Responsibility |
|---|---|
| `src/components/request/load-test/LoadTestTab.tsx` | Root: sidebar + dashboard layout |
| `src/components/request/load-test/PhaseBuilder.tsx` | Add/edit/remove phases with drag-to-reorder |
| `src/components/request/load-test/LiveDashboard.tsx` | 6 chart panels + request log |
| `src/components/request/load-test/LatencyChart.tsx` | Recharts LineChart: p50/p95/p99 over time |
| `src/components/request/load-test/ThroughputChart.tsx` | Recharts AreaChart: req/sec over time |
| `src/components/request/load-test/ErrorRateChart.tsx` | Recharts LineChart: % failed over time |
| `src/components/request/load-test/HistogramChart.tsx` | Recharts BarChart: response time distribution |
| `src/components/request/load-test/ConcurrencyChart.tsx` | Recharts AreaChart: active concurrent over time |
| `src/components/request/load-test/RequestLogTable.tsx` | Virtualised table (last 5 000 entries) |
| `src/components/request/load-test/ExportMenu.tsx` | DropdownMenu: HTML / CSV / JSON / PDF |
| `src/stores/load-test-store.ts` | Zustand: config, run state, time-series, log |

### Modified files

| File | Change |
|---|---|
| `src/components/request/RequestTabs.tsx` | Add "Load test" tab trigger |
| `src/components/request/RequestTabContent.tsx` | Render `LoadTestTab` for the new tab value |
| `src/lib/tauri-api.ts` | New types + `runLoadTestCommand`, `exportLoadTest`, `listen` wrappers |

### `useLoadTestStore` shape

```typescript
interface LoadTestStore {
  // Config
  phases: LoadTestPhase[];
  successRule: { statusBelow: number };
  ringBufferSize: number;

  // Run state
  status: 'idle' | 'running' | 'complete' | 'error';
  timeSeries: TimeSeriesPoint[];
  requestLog: RequestLogEntry[];
  latestSnapshot: LoadTestProgressEvent | null;
  result: LoadTestResult | null;
  error: string | null;

  // Actions
  setPhases: (phases: LoadTestPhase[]) => void;
  startTest: (request: RequestState, tabId: string) => Promise<void>;
  stopTest: () => Promise<void>;
  exportResult: (format: 'html' | 'csv' | 'json' | 'pdf') => Promise<void>;
  reset: () => void;
}
```

The `startTest` action:
1. Sets `status = 'running'`, clears previous data
2. Calls `invoke('run_load_test_command', { input, config })`
3. Subscribes `listen('load_test_progress', ...)` — appends to `timeSeries`, updates `latestSnapshot`
4. Subscribes `listen('load_test_complete', ...)` — sets `result`, `status = 'complete'`, unlistens both

---

## UI Layout (confirmed in mockup)

```
[ Params | Headers | Body | Auth | Scripts | Load test | Contract ]
─────────────────────────────────────────────────────────────────
│ Sidebar (200px)           │ Main area                           │
│                           │ ┌─ Stat bar (6 KPI cards) ────────┐│
│ Configuration             │ │ Completed Succeeded Failed …    ││
│  Concurrency              │ └──────────────────────────────────┘│
│  Total requests           │ ┌ Progress bar ────────────────────┐│
│  Delay between (ms)       │ └──────────────────────────────────┘│
│  Duration cap (s)         │ ┌──────────┐ ┌──────────┐          │
│                           │ │ Latency  │ │Throughput│          │
│ Ramp-up phases            │ │ p50/p95/ │ │ req/sec  │          │
│  ● 0→25 over 10s          │ │ p99      │ │          │          │
│  ● Hold 25 for 40s        │ └──────────┘ └──────────┘          │
│  ● Ramp down 10s          │ ┌──────────┐ ┌──────────┐          │
│  + add phase              │ │ Error %  │ │Histogram │          │
│                           │ │          │ │          │          │
│ Success rule              │ └──────────┘ └──────────┘          │
│  status < 400             │ ┌─ Request log ───────────────────┐│
│                           │ │ # | Status | Latency | Error    ││
│ [Run load test]           │ │ … virtualised rows …            ││
│ [Stop]            [Export]│ └──────────────────────────────────┘│
```

---

## Error Handling

- `startTest` catches `invoke` errors → sets `status = 'error'`, displays in sidebar
- Transport failures classified as before: `failed_transport` (no latency sample)
- Status failures (≥ 400): `failed_status`, latency included in distribution
- Phase transitions log a `PhaseMarker` in `time_series` rendered as vertical reference lines on charts
- If `load_test_complete` never arrives (crash), a 30 s timeout sets `status = 'error'`

---

## Testing

- `cargo test -p rocket-http` — unit tests for `PhaseScheduler`, `RingBuffer`, `TimeSeriesPoint` accumulation
- `cargo test -p rocket-app` — integration tests for `LoadTestService` variable resolution with phases
- `yarn test` — Vitest RTL tests for `LoadTestTab` phase builder (add/remove/reorder), `useLoadTestStore` state transitions, `RequestLogTable` row rendering
- Manual smoke: run against a local echo server, verify phase transitions appear as vertical lines on the latency chart, verify export opens a save dialog

---

## Sub-Project Breakdown

This spec is implemented across **5 plans** executed sequentially:

| Plan | Scope |
|---|---|
| Plan A | Rust data model — new types in `rocket-http` (`LoadTestPhase`, `PhaseKind`, `LoadTestConfig`, `LoadTestProgressEvent`, `RequestLogEntry`, `TimeSeriesPoint`, `PhaseMarker`, extended `LoadTestResult`) |
| Plan B | Rust runtime — `PhaseScheduler`, `RingBuffer`, time-series accumulation, Tauri event emission in `run_load_test` |
| Plan C | Rust services — `LoadTestService` in `rocket-app`, `ExportService`, updated Tauri commands |
| Plan D | Frontend store + IPC — `useLoadTestStore`, updated `tauri-api.ts`, Tauri event listeners |
| Plan E | Frontend UI — `LoadTestTab`, `PhaseBuilder`, `LiveDashboard`, all 6 chart components, `RequestLogTable`, `ExportMenu`, wire into `RequestTabs` |
