# Load Test Feature — Design Spec

## Problem

Users want to stress-test their APIs directly from Rocket to see how they handle concurrent load. Currently Rocket can only send one request at a time.

## Feature

A "Load Test" button on any saved request opens a dialog where the user configures concurrency and total request count, runs the test, and sees summary stats.

## User Flow

1. User opens a saved request in the request editor.
2. Clicks "Load Test" button in the toolbar (next to Send).
3. Dialog opens with:
   - **Concurrent requests** — select: 1, 5, 10, 25, 50, 100
   - **Total requests** — number input, default 100
4. Clicks "Run".
5. Dialog shows a loading spinner with progress (e.g. "42 / 100 completed").
6. When done, dialog displays summary stats.

## Summary Stats (LoadTestResult)

| Stat | Type | Description |
|------|------|-------------|
| `totalRequests` | `u32` | Total requests attempted |
| `succeeded` | `u32` | HTTP responses received (any status code) |
| `failed` | `u32` | Connection errors, timeouts |
| `minLatencyMs` | `f64` | Fastest response time |
| `avgLatencyMs` | `f64` | Mean response time |
| `p50LatencyMs` | `f64` | Median response time |
| `p95LatencyMs` | `f64` | 95th percentile response time |
| `p99LatencyMs` | `f64` | 99th percentile response time |
| `maxLatencyMs` | `f64` | Slowest response time |
| `requestsPerSecond` | `f64` | Throughput |
| `totalDurationMs` | `f64` | Wall-clock time from first to last request |

## Architecture

### Rust Backend

**New file: `crates/rocket-http/src/load_test.rs`**

Exports:
- `LoadTestConfig { concurrency: u32, total_requests: u32 }`
- `LoadTestResult` (the stats struct above)
- `async fn run_load_test(executor: &dyn HttpExecutor, request: ExecutableRequest, config: LoadTestConfig) -> LoadTestResult`

Implementation:
- Uses a tokio semaphore to limit concurrency to `config.concurrency`.
- Spawns `total_requests` tasks, each cloning the request and calling `executor.execute()`.
- Collects `(duration, success/fail)` from each task.
- After all complete, sorts durations and computes percentiles.

The function takes `&dyn HttpExecutor` so it uses the existing `ReqwestExecutor` — no new HTTP client needed.

**New file: `crates/rocket-app/src/load_test_service.rs`**

Thin orchestration: resolves variables in the request (same as `RequestExecutionService`), then calls `run_load_test`. This ensures environment variables and collection auth are applied.

**New Tauri command: `src-tauri/src/commands/load_test.rs`**

- `run_load_test(collection: String, request_path: String, concurrency: u32, total_requests: u32) -> LoadTestResult`
- Loads the request from disk, resolves variables, calls the service.

### Frontend

**New file: `src/components/request/LoadTestDialog.tsx`**

- Triggered by a "Load Test" button in the request editor toolbar.
- Config form: concurrency select + total requests input.
- Results display: table of stats.
- Loading state with "N / total completed" (requires the backend to return progress — for v1 just show a spinner since we wait for completion).

**New TS API function in `src/lib/tauri-api.ts`:**

```typescript
export const runLoadTest = (collection: string, requestPath: string, concurrency: number, totalRequests: number) =>
  invoke<LoadTestResult>("run_load_test", { collection, requestPath, concurrency, totalRequests });
```

## Out of Scope (Future)

- Duration-based mode (run for N seconds instead of N requests)
- Ramp-up period (gradually increase concurrency)
- Body/header variations per request
- Real-time streaming progress (v1 waits for all to finish)
- Mid-run cancellation
- Per-request detail log
- Charts/graphs (latency distribution, timeline)

## Dependencies

- `tokio` (already in the workspace for Tauri async runtime)
- `reqwest` (already used by `ReqwestExecutor`)
- No new crate dependencies needed.
