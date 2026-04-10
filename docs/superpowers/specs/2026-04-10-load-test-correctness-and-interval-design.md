# Load Test Correctness and Interval Feature

**Date:** 2026-04-10
**Status:** Approved

## Problem

Two distinct issues erode trust in the load test feature:

1. **Backend crashes are not detected.** `run_load_test` classifies any `Ok(_)` from
   the executor as success. `reqwest` returns `Ok(response)` for 4xx and 5xx responses
   — it only returns `Err` for transport-level failures (connection refused, TLS
   error, timeout). When a backend crashes behind a proxy, the proxy returns
   `502/503/504`; when the backend's own panic handler catches the crash, it returns
   `500`. Both show up as `Ok(resp)` and the load test reports "100 succeeded, 0
   failed".

2. **Fresh `reqwest::Client` built per request.** `ReqwestExecutor::execute` calls
   `build_client(request)` on every invocation. For a 100-request load test that
   means 100 new clients, 100 TCP handshakes, 100 TLS negotiations, and zero
   connection pooling. The latency numbers reflect client handshake overhead more
   than server response time, often by 10–100× on HTTPS endpoints.

The user also asked for a new feature: **a configurable delay between requests** so
the load test can be used as a rate-limited generator rather than a burst gun.

## Design

### Layer responsibilities

```
LoadTestDialog.tsx         → UI input, invokes runLoadTest
tauri-api.ts               → TS types, IPC invoke
load_test_command (tauri)  → thin adapter, no logic
RequestExecutionService    → resolves variables, delegates to rocket-http
rocket-http::run_load_test → scheduling, concurrency, outcome classification
ReqwestExecutor (rocket-infra) → HTTP execution with cached clients
```

### Change 1 — Outcome classification in `run_load_test`

Each task now produces one of three outcomes:

```rust
enum Outcome {
    Success(f64),     // HTTP status < 400, includes latency
    StatusFail(f64),  // HTTP status ≥ 400, includes latency
    TransportFail,    // executor returned Err, no latency sample
}

match exec.execute(&req).await {
    Ok(resp) => {
        let ms = resp.duration_ms as f64;
        if resp.status < 400 { Outcome::Success(ms) } else { Outcome::StatusFail(ms) }
    }
    Err(_) => Outcome::TransportFail,
}
```

Aggregation:

- `Success` → `succeeded += 1`, latency pushed onto `latencies`.
- `StatusFail` → `failed_status += 1`, latency pushed onto `latencies` (4xx/5xx
  responses are real measurements and still belong in the distribution).
- `TransportFail` → `failed_transport += 1`, **no** latency sample.
- `failed = failed_transport + failed_status`.

Rationale for including `StatusFail` latencies in the distribution: an error
response is still a real round-trip. A test where the server returns 500 in 5 ms
has different characteristics from one that returns 500 in 500 ms, and the user
needs to see both.

**Success rule:** HTTP status `< 400`. This matches k6, hey, and Postman Newman
defaults. Redirects pass; 4xx and 5xx fail.

### Change 2 — `LoadTestConfig` gains `interval_ms`

```rust
pub struct LoadTestConfig {
    pub concurrency: u32,
    pub total_requests: u32,
    #[serde(default)]
    pub interval_ms: u32,
}
```

`#[serde(default)]` preserves backwards compatibility. A missing `intervalMs` in the
IPC payload deserialises to 0, matching existing behaviour exactly.

**Semantics (staggered start):** after each spawn in the loop, sleep `interval_ms`
before the next iteration. Skip the sleep on the final iteration.

```rust
for i in 0..total {
    let permit = semaphore.clone().acquire_owned().await.unwrap();
    // spawn task with permit
    if i + 1 < total && config.interval_ms > 0 {
        tokio::time::sleep(Duration::from_millis(config.interval_ms as u64)).await;
    }
}
```

Interaction with concurrency: the semaphore still caps in-flight requests, but the
interval rate-limits the spawn rate on top. With `interval=1000ms, concurrency=10,
total=100`, tasks are spawned ~1 per second; the 10-slot concurrency pool rarely
engages because new tasks are spaced further apart than most responses take.

This matches the user's mental model of "delay between each request".

### Change 3 — `LoadTestResult` gains failure breakdown

```rust
pub struct LoadTestResult {
    pub total_requests: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub failed_transport: u32,  // NEW
    pub failed_status: u32,     // NEW
    pub min_latency_ms: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub max_latency_ms: f64,
    pub requests_per_second: f64,
    pub total_duration_ms: f64,
}
```

`failed` is kept for compact display and backwards compatibility; it is always
equal to `failed_transport + failed_status`.

`requests_per_second` remains `succeeded / total_duration` (successful
throughput). Changing this is out of scope.

### Change 4 — Client cache in `ReqwestExecutor`

```rust
use std::collections::HashMap;
use std::sync::Mutex;

pub struct ReqwestExecutor {
    clients: Mutex<HashMap<(bool, bool), Client>>,
}

impl ReqwestExecutor {
    pub fn new() -> Self {
        Self { clients: Mutex::new(HashMap::new()) }
    }

    fn get_or_build_client(
        &self,
        follow_redirects: bool,
        verify_ssl: bool,
    ) -> DomainResult<Client> {
        let key = (follow_redirects, verify_ssl);
        let mut cache = self.clients.lock().unwrap();
        if let Some(c) = cache.get(&key) {
            return Ok(c.clone());
        }
        let client = build_client_impl(follow_redirects, verify_ssl)?;
        cache.insert(key, client.clone());
        Ok(client)
    }
}
```

Inside `execute()`:

```rust
// Before:
let client = build_client(request)?;

// After:
let client = self.get_or_build_client(
    request.options.follow_redirects,
    request.options.verify_ssl,
)?;
```

The existing free function `build_client(request: &HttpRequest)` is renamed to
`build_client_impl(follow_redirects: bool, verify_ssl: bool)` and no longer takes
the full request. Per-request `timeout` stays on the request builder
(`builder.timeout(...)`) — it's not bound to the `Client`.

**Key design choices:**

- The cache key is `(follow_redirects, verify_ssl)` because those are the only two
  request options that force a different `Client::builder()` configuration. All
  other options (headers, body, query, timeout, auth) are applied per-request on
  the request builder.
- At most 4 distinct keys can exist. In practice one load test uses exactly 1
  cached client.
- `reqwest::Client::clone()` is cheap (internally `Arc`); no allocation per request.
- The `Mutex` is held only during lookup and one-time insert, never across the
  `execute()` await. No contention at realistic concurrency levels.
- Cache is per-executor-instance; a new `ReqwestExecutor::new()` starts empty.

**Why not `OnceCell` per key?** A `Mutex<HashMap>` is simpler (one data structure,
no unsafe, no per-key boilerplate) and the performance cost is irrelevant — we
lock for microseconds, not milliseconds. `OnceCell` would help if we had dozens of
keys; we have at most 4.

### Change 5 — Tauri command and Rust service

**No changes needed.** `run_load_test_command` already accepts `LoadTestConfig`
and `run_load_test` already accepts `LoadTestResult`; serde handles the new
fields. `RequestExecutionService::run_load_test` already forwards the config
untouched.

### Change 6 — Frontend types (`src/lib/tauri-api.ts`)

```typescript
export interface LoadTestConfig {
  concurrency: number;
  totalRequests: number;
  intervalMs: number;
}

export interface LoadTestResult {
  totalRequests: number;
  succeeded: number;
  failed: number;
  failedTransport: number;
  failedStatus: number;
  // ... latency + throughput fields unchanged
}
```

### Change 7 — `LoadTestDialog.tsx`

- The existing `grid-cols-2` row holding Concurrent / Total stays as-is.
- Add a **new row** below it with a single full-width input:
  - Label: **"Delay between requests (s)"**
  - `<Input type="number" step={0.1} min={0} max={60} value={intervalSeconds} />`
  - Default: `"0"`
  - Disabled while running

  A new row (rather than squeezing a third column into `sm:max-w-md`) keeps the
  input comfortably sized and leaves room for the label and hint text.
- State: `const [intervalSeconds, setIntervalSeconds] = useState('0')`
- When calling `runLoadTest`, convert: `intervalMs: Math.round(parseFloat(intervalSeconds) * 1000) || 0`
- Result display: when `result.failed > 0`, render a sub-line under the `Failed`
  stat in `text-[10px] text-muted-foreground`:

  ```
  Failed
  100
  95 status, 5 transport
  ```

  When `result.failed === 0`, no sub-line.

## Data flow after fix

```
User clicks Run
  → resolveRequestFields(tabId, request)              [frontend, 7-scope resolve]
  → runLoadTest(request, { concurrency, totalRequests, intervalMs })
  → run_load_test_command(input, config, svc)          [Tauri]
  → svc.run_load_test(input, config)                   [rocket-app]
      → resolve_request(&input)                        [variable resolution]
      → http_run_load_test(executor, &resolved, &config)
          → for each of total:
              → acquire permit
              → spawn task
                  → executor.execute(&req)             [uses cached Client]
                      → status < 400 → Success(ms)
                      → status >= 400 → StatusFail(ms)
                      → Err → TransportFail
              → sleep(interval_ms) if not last
          → collect outcomes, compute stats
```

## Testing

### `rocket-http` — new tests in `load_test.rs`

- **`load_test_4xx_counts_as_failed_status`** — `MockExecutor` returns 404; assert
  `failed_status=1, succeeded=0, failed_transport=0`, and p50 equals the mock's
  latency (latency included in stats).
- **`load_test_5xx_counts_as_failed_status`** — same shape with 502.
- **`load_test_3xx_counts_as_success`** — `MockExecutor` returns 301; assert
  `succeeded=1`.
- **`load_test_transport_error_counted_as_transport_failure`** — updated version
  of existing `load_test_counts_failures`: executor returns `Err`; assert
  `failed_transport=5, failed_status=0, failed=5`.
- **`load_test_mixed_outcomes_stats`** — executor alternates 200 and 500; assert
  `succeeded=5, failed_status=5, failed_transport=0`, and latency stats reflect
  both.
- **`load_test_interval_spacing_lower_bound`** — `interval_ms=50, total=3,
  concurrency=1`; assert `total_duration_ms >= 100` (two gaps of 50ms).
- **`load_test_interval_zero_no_delay`** — regression check: `interval_ms=0,
  total=10`; assert `total_duration_ms < 500` (no forced delays).

### `rocket-infra` — new tests in `reqwest_executor.rs`

Use a test-only accessor:

```rust
#[cfg(test)]
impl ReqwestExecutor {
    pub fn cache_len(&self) -> usize { self.clients.lock().unwrap().len() }
}
```

- **`executor_caches_client_on_first_use`** — construct, call internal
  `get_or_build_client(true, true)` twice; assert `cache_len() == 1`.
- **`executor_builds_different_clients_for_different_options`** — call with
  `(true, true)` then `(true, false)`; assert `cache_len() == 2`.
- **`executor_starts_with_empty_cache`** — `ReqwestExecutor::new().cache_len() == 0`.

### Frontend — new tests

- **`LoadTestDialog.test.tsx` (new)** — render the dialog, assert the interval
  input exists, change it, click Run (mock `runLoadTest`), assert the call
  received `intervalMs === seconds * 1000`.
- **Result breakdown rendering** — mock a result with `failed_status=3,
  failed_transport=2`, assert the sub-line text `"3 status, 2 transport"` appears.
  Mock with `failed=0`, assert no sub-line.

### Existing tests

The existing `load_test_returns_correct_counts`, `percentile_computation`,
`percentile_empty`, `load_test_single_request`, and the renamed
`load_test_transport_error_counted_as_transport_failure` should all continue to
pass without modification (the first four don't depend on the new fields;
the last is the renamed counterpart of the existing `load_test_counts_failures`).

## Out of scope

- **Integer-ms latency precision.** `duration_ms` stays `u64`. Sub-millisecond
  precision is a separate change touching `HttpResponse` and every executor
  implementation.
- **OAuth2 token caching.** `apply_auth` still re-fetches tokens per request for
  `client_credentials`. Real fix requires a token cache with expiry tracking,
  which is its own design.
- **Per-request error surfacing.** The user still sees aggregate counts, not
  individual failure reasons. A future change could surface the first N errors
  with their status codes and messages.
- **Warmup / ramp-up phase.** No logic to discard the first N requests from
  stats. Client caching mitigates most of the need.
- **Changing `requests_per_second` from `succeeded/duration` to
  `total/duration`.** Successful throughput is what most users want.
- **Body size stats.** No min/avg/max response size.
