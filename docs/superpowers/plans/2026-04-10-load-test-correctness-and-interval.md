# Load Test Correctness and Interval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix load test trust issues (crashes reported as success, handshake overhead dominating latency) and add a configurable delay between requests.

**Architecture:** Classify task outcomes as `Success`/`StatusFail`/`TransportFail` in `rocket-http::run_load_test` so 4xx/5xx responses no longer count as success. Add `interval_ms` to `LoadTestConfig` with staggered-start semantics (sleep between spawns). Move `reqwest::Client` construction into a `Mutex<HashMap>` cache inside `ReqwestExecutor` so load-test requests reuse one client instead of building a fresh one per call. Plumb the new fields through the Tauri boundary and surface them in `LoadTestDialog`.

**Tech Stack:** Rust (tokio, reqwest, serde, async-trait), TypeScript (React, Vitest, @testing-library/react), Tauri IPC.

---

## File Map

| File | Change |
|---|---|
| `crates/rocket-http/src/load_test.rs` | Outcome enum, status-based classification, new `failed_transport`/`failed_status` fields, `interval_ms` field with sleep in spawn loop, new tests |
| `crates/rocket-infra/src/reqwest_executor.rs` | Add `clients: Mutex<HashMap<(bool,bool), Client>>` cache, rename `build_client` → `build_client_impl(bool, bool)`, update `execute()` call site, update existing `build_client_respects_ssl_option` test, add cache tests |
| `crates/rocket-app/src/execution_service.rs` | Update one test constructor to include `interval_ms: 0` |
| `src/lib/tauri-api.ts` | Add `intervalMs` to `LoadTestConfig`; add `failedTransport`/`failedStatus` to `LoadTestResult` |
| `src/components/request/LoadTestDialog.tsx` | New state + input row for delay, forward `intervalMs`, render failure breakdown sub-line |
| `src/components/request/__tests__/LoadTestDialog.test.tsx` (new) | RTL test for interval input forwarding and failure breakdown rendering |

---

## Task 1: Backend outcome classification (status-based success)

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`

This task fixes the user-reported bug: backend crashed but all 100 requests showed as succeeded. It adds status-based success classification without touching the interval feature yet.

- [ ] **Step 1: Add a configurable-status mock executor to the test module**

In `crates/rocket-http/src/load_test.rs`, inside the existing `#[cfg(test)] mod tests` block (after the existing `MockExecutor` struct and its impls, before `fn test_request()`), add:

```rust
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
            size_bytes: 0,
        })
    }
}
```

- [ ] **Step 2: Write the failing tests**

Still inside `#[cfg(test)] mod tests`, after the existing `load_test_single_request` test, add:

```rust
#[tokio::test]
async fn load_test_4xx_counts_as_failed_status() {
    let executor: Arc<dyn HttpExecutor> = Arc::new(StatusExecutor(404));
    let config = LoadTestConfig { concurrency: 1, total_requests: 1 };
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
    let config = LoadTestConfig { concurrency: 1, total_requests: 1 };
    let result = run_load_test(executor, &test_request(), &config).await;
    assert_eq!(result.failed_status, 1);
    assert_eq!(result.failed_transport, 0);
    assert_eq!(result.failed, 1);
    assert_eq!(result.succeeded, 0);
}

#[tokio::test]
async fn load_test_3xx_counts_as_success() {
    let executor: Arc<dyn HttpExecutor> = Arc::new(StatusExecutor(301));
    let config = LoadTestConfig { concurrency: 1, total_requests: 1 };
    let result = run_load_test(executor, &test_request(), &config).await;
    assert_eq!(result.succeeded, 1);
    assert_eq!(result.failed, 0);
    assert_eq!(result.failed_status, 0);
}
```

Also rename the existing `load_test_counts_failures` test to `load_test_transport_error_counted_as_transport_failure` and add the new assertions. Find the existing test and replace it with:

```rust
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
    let config = LoadTestConfig { concurrency: 2, total_requests: 5 };
    let result = run_load_test(executor, &test_request(), &config).await;
    assert_eq!(result.total_requests, 5);
    assert_eq!(result.failed, 5);
    assert_eq!(result.failed_transport, 5);
    assert_eq!(result.failed_status, 0);
    assert_eq!(result.succeeded, 0);
}
```

- [ ] **Step 3: Run tests to confirm they fail**

```bash
cd /home/numericlabs/data/rocket/rocket
cargo test -p rocket-http load_test 2>&1 | tail -25
```

Expected: compile errors — `failed_status` and `failed_transport` are not fields on `LoadTestResult`.

- [ ] **Step 4: Add the new fields and the outcome classification**

At the top of `crates/rocket-http/src/load_test.rs`, the `LoadTestResult` struct currently looks like:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestResult {
    pub total_requests: u32,
    pub succeeded: u32,
    pub failed: u32,
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

Replace it with:

```rust
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
}
```

Above `pub async fn run_load_test(...)` (between the `percentile` function and `run_load_test`), add the Outcome enum:

```rust
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
```

Now replace the body of `run_load_test` with the classification-aware version. The full new body (lines 42–109 currently):

```rust
pub async fn run_load_test(
    executor: Arc<dyn HttpExecutor>,
    request: &HttpRequest,
    config: &LoadTestConfig,
) -> LoadTestResult {
    let semaphore = Arc::new(Semaphore::new(config.concurrency as usize));
    let total = config.total_requests as usize;
    let start = std::time::Instant::now();

    let mut handles = Vec::with_capacity(total);
    for _ in 0..total {
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
    }
}
```

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cargo test -p rocket-http load_test 2>&1 | tail -25
```

Expected: all load_test tests pass, including the 5 new/renamed ones (`load_test_4xx_counts_as_failed_status`, `load_test_5xx_counts_as_failed_status`, `load_test_3xx_counts_as_success`, `load_test_transport_error_counted_as_transport_failure`, plus existing `load_test_returns_correct_counts`, `load_test_single_request`, `percentile_computation`, `percentile_empty`).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-http/src/load_test.rs
git commit -m "fix(rocket-http): classify load test outcomes by HTTP status

4xx and 5xx responses no longer count as success. Adds failed_transport
and failed_status breakdown to LoadTestResult. Status failures still
contribute to latency stats since they are real round-trips."
```

---

## Task 2: Backend interval feature (staggered-start sleep)

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs`
- Modify: `crates/rocket-app/src/execution_service.rs`

This task adds `interval_ms` to `LoadTestConfig` and the sleep logic in the spawn loop. It also updates every existing test that constructs `LoadTestConfig` to include the new field.

- [ ] **Step 1: Write the failing test for interval timing**

In `crates/rocket-http/src/load_test.rs`, inside `#[cfg(test)] mod tests`, after the tests added in Task 1, add:

```rust
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
```

- [ ] **Step 2: Run tests to confirm compile failure**

```bash
cargo test -p rocket-http load_test_interval 2>&1 | tail -15
```

Expected: compile error — `interval_ms` is not a field on `LoadTestConfig`.

- [ ] **Step 3: Add `interval_ms` to `LoadTestConfig`**

In `crates/rocket-http/src/load_test.rs`, the `LoadTestConfig` struct currently looks like:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestConfig {
    pub concurrency: u32,
    pub total_requests: u32,
}
```

Replace it with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestConfig {
    pub concurrency: u32,
    pub total_requests: u32,
    #[serde(default)]
    pub interval_ms: u32,
}
```

`#[serde(default)]` means a missing `intervalMs` in an IPC payload deserialises to 0, preserving backwards compatibility with any caller that hasn't been updated yet.

- [ ] **Step 4: Add `Duration` import and the sleep in the spawn loop**

At the top of `crates/rocket-http/src/load_test.rs`, the imports currently are:

```rust
use crate::{HttpExecutor, HttpRequest};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
```

Add `Duration`:

```rust
use crate::{HttpExecutor, HttpRequest};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
```

In `run_load_test`, change the spawn loop header from `for _ in 0..total {` to `for i in 0..total {` and add the sleep after `handles.push(handle);`. The updated loop:

```rust
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
```

- [ ] **Step 5: Update all existing `LoadTestConfig` constructors in rocket-http**

Inside `crates/rocket-http/src/load_test.rs`'s test module, the existing tests construct `LoadTestConfig` without `interval_ms`, which no longer compiles. Update them:

**`load_test_returns_correct_counts`** — change:
```rust
let config = LoadTestConfig {
    concurrency: 5,
    total_requests: 20,
};
```
to:
```rust
let config = LoadTestConfig {
    concurrency: 5,
    total_requests: 20,
    interval_ms: 0,
};
```

**`load_test_transport_error_counted_as_transport_failure`** (the test renamed in Task 1) — change:
```rust
let config = LoadTestConfig { concurrency: 2, total_requests: 5 };
```
to:
```rust
let config = LoadTestConfig { concurrency: 2, total_requests: 5, interval_ms: 0 };
```

**`load_test_single_request`** — change:
```rust
let config = LoadTestConfig { concurrency: 1, total_requests: 1 };
```
to:
```rust
let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0 };
```

**`load_test_4xx_counts_as_failed_status`**, **`load_test_5xx_counts_as_failed_status`**, **`load_test_3xx_counts_as_success`** (all added in Task 1) — change each:
```rust
let config = LoadTestConfig { concurrency: 1, total_requests: 1 };
```
to:
```rust
let config = LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0 };
```

- [ ] **Step 6: Update the one `LoadTestConfig` constructor in rocket-app**

In `crates/rocket-app/src/execution_service.rs`, find the line inside `service_run_load_test_resolves_variables_before_firing`:

```rust
let config = rocket_http::LoadTestConfig { concurrency: 1, total_requests: 1 };
```

Replace with:

```rust
let config = rocket_http::LoadTestConfig { concurrency: 1, total_requests: 1, interval_ms: 0 };
```

- [ ] **Step 7: Run all affected tests**

```bash
cargo test -p rocket-http -p rocket-app 2>&1 | tail -20
```

Expected: all tests pass. The two new tests (`load_test_interval_spacing_lower_bound`, `load_test_interval_zero_no_delay`) pass; all Task 1 tests still pass; rocket-app's `service_run_load_test_resolves_variables_before_firing` still passes.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-http/src/load_test.rs crates/rocket-app/src/execution_service.rs
git commit -m "feat(rocket-http): add interval_ms to LoadTestConfig

Staggered-start rate limiting: sleep interval_ms between each task
spawn in the semaphore-bounded loop. Backwards compatible via
#[serde(default)]."
```

---

## Task 3: Client cache in `ReqwestExecutor`

**Files:**
- Modify: `crates/rocket-infra/src/reqwest_executor.rs`

Every call to `ReqwestExecutor::execute` currently builds a fresh `reqwest::Client`, paying full handshake overhead and defeating connection pooling. This task moves the client into an internal cache keyed on the two per-request options that actually force a different `Client` configuration.

- [ ] **Step 1: Write the failing tests for the cache**

In `crates/rocket-infra/src/reqwest_executor.rs`, inside the existing `#[cfg(test)] mod tests` block (after the existing `maps_all_http_methods` test), add:

```rust
#[test]
fn executor_starts_with_empty_cache() {
    let exec = ReqwestExecutor::new();
    assert_eq!(exec.cache_len(), 0);
}

#[test]
fn executor_caches_client_on_first_use() {
    let exec = ReqwestExecutor::new();
    let _c1 = exec.get_or_build_client(true, true).unwrap();
    let _c2 = exec.get_or_build_client(true, true).unwrap();
    // Same options → only one cached client.
    assert_eq!(exec.cache_len(), 1);
}

#[test]
fn executor_builds_different_clients_for_different_options() {
    let exec = ReqwestExecutor::new();
    let _a = exec.get_or_build_client(true, true).unwrap();
    let _b = exec.get_or_build_client(true, false).unwrap();
    let _c = exec.get_or_build_client(false, true).unwrap();
    let _d = exec.get_or_build_client(false, false).unwrap();
    // 4 distinct (redirects, ssl) combinations → 4 cached clients.
    assert_eq!(exec.cache_len(), 4);
    // Re-querying one does not grow the cache.
    let _a2 = exec.get_or_build_client(true, true).unwrap();
    assert_eq!(exec.cache_len(), 4);
}
```

Also update the existing test `build_client_respects_ssl_option` (currently at the bottom of the tests module). Its current content:

```rust
#[test]
fn build_client_respects_ssl_option() {
    let mut req = HttpRequest::new(HttpMethod::Get, "https://example.com");
    req.options.verify_ssl = false;
    // Should not error when building a client that accepts invalid certs.
    assert!(build_client(&req).is_ok());
}
```

Replace with:

```rust
#[test]
fn build_client_impl_respects_ssl_option() {
    // Should not error when building a client that accepts invalid certs.
    assert!(build_client_impl(true, false).is_ok());
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p rocket-infra executor_ 2>&1 | tail -15
```

Expected: compile errors — `ReqwestExecutor::get_or_build_client`, `ReqwestExecutor::cache_len`, and `build_client_impl` do not exist.

- [ ] **Step 3: Add imports and restructure `ReqwestExecutor`**

At the top of `crates/rocket-infra/src/reqwest_executor.rs`, the imports currently are:

```rust
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::{redirect, Client, Method};

use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Auth, Body, BodyMode, Header};
```

Replace with:

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::{redirect, Client, Method};

use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Auth, Body, BodyMode, Header};
```

The current struct and impl:

```rust
pub struct ReqwestExecutor;

impl ReqwestExecutor {
    pub fn new() -> Self {
        Self
    }
}
```

Replace with:

```rust
pub struct ReqwestExecutor {
    // Cache of reqwest::Clients keyed on (follow_redirects, verify_ssl).
    // These are the only two HttpRequest options that force a different
    // Client::builder() configuration; everything else (headers, body,
    // query, timeout, auth) is applied per-request on the request builder.
    // At most 4 distinct keys can ever exist.
    clients: Mutex<HashMap<(bool, bool), Client>>,
}

impl ReqwestExecutor {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    fn get_or_build_client(
        &self,
        follow_redirects: bool,
        verify_ssl: bool,
    ) -> DomainResult<Client> {
        let key = (follow_redirects, verify_ssl);
        let mut cache = self.clients.lock().unwrap();
        if let Some(c) = cache.get(&key) {
            // reqwest::Client::clone is cheap — internally Arc.
            return Ok(c.clone());
        }
        let client = build_client_impl(follow_redirects, verify_ssl)?;
        cache.insert(key, client.clone());
        Ok(client)
    }

    #[cfg(test)]
    pub fn cache_len(&self) -> usize {
        self.clients.lock().unwrap().len()
    }
}
```

- [ ] **Step 4: Rename `build_client` to `build_client_impl` with new signature**

The current free function is:

```rust
fn build_client(request: &HttpRequest) -> DomainResult<Client> {
    let redirect_policy = if request.options.follow_redirects {
        redirect::Policy::limited(10)
    } else {
        redirect::Policy::none()
    };

    Client::builder()
        .redirect(redirect_policy)
        .danger_accept_invalid_certs(!request.options.verify_ssl)
        .build()
        .map_err(|e| DomainError::Http(e.to_string()))
}
```

Replace with:

```rust
fn build_client_impl(follow_redirects: bool, verify_ssl: bool) -> DomainResult<Client> {
    let redirect_policy = if follow_redirects {
        redirect::Policy::limited(10)
    } else {
        redirect::Policy::none()
    };

    Client::builder()
        .redirect(redirect_policy)
        .danger_accept_invalid_certs(!verify_ssl)
        .build()
        .map_err(|e| DomainError::Http(e.to_string()))
}
```

- [ ] **Step 5: Update `execute()` to use the cache**

Inside `impl HttpExecutor for ReqwestExecutor`, at the top of `execute()`, the current line is:

```rust
let client = build_client(request)?;
```

Replace with:

```rust
let client = self.get_or_build_client(
    request.options.follow_redirects,
    request.options.verify_ssl,
)?;
```

- [ ] **Step 6: Run all rocket-infra tests**

```bash
cargo test -p rocket-infra 2>&1 | tail -20
```

Expected: all tests pass, including the three new cache tests (`executor_starts_with_empty_cache`, `executor_caches_client_on_first_use`, `executor_builds_different_clients_for_different_options`) and the renamed `build_client_impl_respects_ssl_option`.

- [ ] **Step 7: Verify the full workspace compiles**

```bash
cargo check 2>&1 | tail -10
```

Expected: no errors. (`src-tauri/src/lib.rs` already builds `ReqwestExecutor::new()` which still compiles — the constructor signature is unchanged.)

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-infra/src/reqwest_executor.rs
git commit -m "perf(rocket-infra): cache reqwest::Client in ReqwestExecutor

Each execute() call previously built a fresh Client, paying full
TCP+TLS handshake cost and defeating connection pooling. Cache
clients by (follow_redirects, verify_ssl) — the only per-request
options that force a different Client configuration. At most 4
entries; in practice one load test reuses one client."
```

---

## Task 4: Frontend types + `LoadTestDialog` interval input

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/components/request/LoadTestDialog.tsx`

Merged into one task because adding `intervalMs: number` as a required field on `LoadTestConfig` makes `LoadTestDialog.tsx` stop compiling until the dialog also supplies it. Doing both in one commit keeps every commit green.

- [ ] **Step 1: Extend `LoadTestConfig` with `intervalMs`**

In `src/lib/tauri-api.ts`, the existing interface (around line 423):

```typescript
export interface LoadTestConfig {
  concurrency: number;
  totalRequests: number;
}
```

Replace with:

```typescript
export interface LoadTestConfig {
  concurrency: number;
  totalRequests: number;
  intervalMs: number;
}
```

- [ ] **Step 2: Extend `LoadTestResult` with failure breakdown**

In the same file, the existing interface (around line 428):

```typescript
export interface LoadTestResult {
  totalRequests: number;
  succeeded: number;
  failed: number;
  minLatencyMs: number;
  avgLatencyMs: number;
  p50LatencyMs: number;
  p95LatencyMs: number;
  p99LatencyMs: number;
  maxLatencyMs: number;
  requestsPerSecond: number;
  totalDurationMs: number;
}
```

Replace with:

```typescript
export interface LoadTestResult {
  totalRequests: number;
  succeeded: number;
  failed: number;
  failedTransport: number;
  failedStatus: number;
  minLatencyMs: number;
  avgLatencyMs: number;
  p50LatencyMs: number;
  p95LatencyMs: number;
  p99LatencyMs: number;
  maxLatencyMs: number;
  requestsPerSecond: number;
  totalDurationMs: number;
}
```

(Preserving the existing field order is not essential — serde on the Rust side serialises by name.)

- [ ] **Step 3: Add the interval state to `LoadTestDialog`**

In `src/components/request/LoadTestDialog.tsx`, inside the `LoadTestDialog` component (currently near the top, after `setTotalRequests`), the existing lines look like:

```typescript
const [concurrency, setConcurrency] = useState('10');
const [totalRequests, setTotalRequests] = useState('100');
const [running, setRunning] = useState(false);
```

Add a new state entry between them:

```typescript
const [concurrency, setConcurrency] = useState('10');
const [totalRequests, setTotalRequests] = useState('100');
const [intervalSeconds, setIntervalSeconds] = useState('0');
const [running, setRunning] = useState(false);
```

- [ ] **Step 4: Add the interval input row**

Find the existing grid row in the dialog content:

```tsx
<div className='space-y-4 py-2'>
  <div className='grid grid-cols-2 gap-4'>
    <div className='space-y-1.5'>
      <Label>Concurrent requests</Label>
      {/* ... existing Select ... */}
    </div>
    <div className='space-y-1.5'>
      <Label>Total requests</Label>
      {/* ... existing Input ... */}
    </div>
  </div>
```

Immediately after the closing `</div>` of `grid grid-cols-2` (before the `<p className='text-xs text-muted-foreground'>` line that shows the method+URL), insert a new full-width row:

```tsx
  <div className='space-y-1.5'>
    <Label>Delay between requests (s)</Label>
    <Input
      type='number'
      min={0}
      max={60}
      step={0.1}
      value={intervalSeconds}
      onChange={(e) => setIntervalSeconds(e.target.value)}
      className='h-8 text-sm'
      disabled={running}
    />
  </div>
```

- [ ] **Step 5: Pass `intervalMs` to `runLoadTest`**

In the `handleRun` function, the existing call looks like:

```tsx
const res = await runLoadTest(
  {
    method: request.method,
    /* ... other fields ... */
    collection: resolved.collection,
    environmentName: resolved.environmentName,
    requestPath: resolved.requestPath,
  },
  {
    concurrency: parseInt(concurrency, 10),
    totalRequests: parseInt(totalRequests, 10),
  },
);
```

Replace the config object (second argument) with:

```tsx
  {
    concurrency: parseInt(concurrency, 10),
    totalRequests: parseInt(totalRequests, 10),
    intervalMs: Math.round(parseFloat(intervalSeconds) * 1000) || 0,
  },
```

The `|| 0` handles `NaN` from an empty input.

- [ ] **Step 6: TypeScript check and format**

```bash
yarn tsc --noEmit && yarn check 2>&1 | tail -15
```

Expected: no TypeScript errors. If Biome complains about formatting, run `yarn format` and re-run `yarn check`. The pre-existing `CollectionOverviewTab.tsx` non-null-assertion warning is unrelated and may still appear — that is fine.

- [ ] **Step 7: Commit**

```bash
git add src/lib/tauri-api.ts src/components/request/LoadTestDialog.tsx
git commit -m "feat(frontend): add delay-between-requests input and failure breakdown types"
```

---

## Task 5: `LoadTestDialog` — failure breakdown sub-line

**Files:**
- Modify: `src/components/request/LoadTestDialog.tsx`

- [ ] **Step 1: Replace the `Failed` stat with an inline version that shows the breakdown**

In `src/components/request/LoadTestDialog.tsx`, find the existing `Failed` stat inside the results grid:

```tsx
<Stat
  label='Failed'
  value={result.failed}
  className={result.failed > 0 ? 'text-destructive' : ''}
/>
```

Replace with:

```tsx
<div>
  <p className='text-[10px] text-muted-foreground uppercase'>Failed</p>
  <p className={`text-sm font-medium ${result.failed > 0 ? 'text-destructive' : ''}`}>
    {result.failed}
  </p>
  {result.failed > 0 && (
    <p className='text-[9px] text-muted-foreground leading-tight'>
      {result.failedStatus} status, {result.failedTransport} transport
    </p>
  )}
</div>
```

This preserves the visual alignment with the `Succeeded` and `Total` stats in the same grid row while adding a small third line only when failures exist.

- [ ] **Step 2: TypeScript check and format**

```bash
yarn tsc --noEmit && yarn check 2>&1 | tail -15
```

Expected: no errors. Only the pre-existing `CollectionOverviewTab.tsx` warning may appear.

- [ ] **Step 3: Commit**

```bash
git add src/components/request/LoadTestDialog.tsx
git commit -m "feat(frontend): show transport/status failure breakdown in LoadTestDialog"
```

---

## Task 6: Frontend component tests for `LoadTestDialog`

**Files:**
- Create: `src/components/request/__tests__/LoadTestDialog.test.tsx`

- [ ] **Step 1: Confirm React Testing Library is available**

```bash
grep -E '"@testing-library/react"|"@testing-library/user-event"' package.json
```

Expected: both packages listed. If neither is present, skip this task entirely and document under "Out of scope" in the final commit message.

- [ ] **Step 2: Write the test file**

Create `src/components/request/__tests__/LoadTestDialog.test.tsx` with:

```tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LoadTestDialog } from '../LoadTestDialog';
import type { RequestState } from '@/types/pane-types';

// Mock the tauri-api module so no real IPC call is made.
vi.mock('@/lib/tauri-api', () => ({
  runLoadTest: vi.fn(),
}));

// Mock the execute-request helper so no real variable resolution runs.
vi.mock('@/lib/execute-request', () => ({
  resolveRequestFields: vi.fn(async (_tabId: string, req: RequestState) => ({
    url: req.url,
    headers: req.headers,
    queryParams: req.queryParams,
    body: req.body,
    auth: req.auth,
    collection: undefined,
    environmentName: undefined,
    requestPath: undefined,
  })),
}));

import { runLoadTest } from '@/lib/tauri-api';

function makeRequest(): RequestState {
  return {
    id: 'r1',
    name: 'Test',
    method: 'GET',
    url: 'https://example.com',
    headers: [],
    queryParams: [],
    body: null,
    auth: { type: 'none' },
    settings: {
      followRedirects: true,
      timeoutMs: 30000,
      verifySsl: true,
    },
    docs: '',
  } as unknown as RequestState;
}

describe('LoadTestDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the delay input', () => {
    render(
      <LoadTestDialog
        open
        onOpenChange={() => {}}
        request={makeRequest()}
        tabId='t1'
      />,
    );
    expect(screen.getByLabelText(/delay between requests/i)).toBeInTheDocument();
  });

  it('forwards intervalMs = seconds * 1000 to runLoadTest', async () => {
    (runLoadTest as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      totalRequests: 1,
      succeeded: 1,
      failed: 0,
      failedTransport: 0,
      failedStatus: 0,
      minLatencyMs: 1,
      avgLatencyMs: 1,
      p50LatencyMs: 1,
      p95LatencyMs: 1,
      p99LatencyMs: 1,
      maxLatencyMs: 1,
      requestsPerSecond: 1,
      totalDurationMs: 1,
    });

    render(
      <LoadTestDialog
        open
        onOpenChange={() => {}}
        request={makeRequest()}
        tabId='t1'
      />,
    );

    const delayInput = screen.getByLabelText(/delay between requests/i);
    fireEvent.change(delayInput, { target: { value: '0.5' } });

    fireEvent.click(screen.getByRole('button', { name: /^run$/i }));

    await waitFor(() => expect(runLoadTest).toHaveBeenCalledTimes(1));
    const configArg = (runLoadTest as unknown as ReturnType<typeof vi.fn>).mock
      .calls[0][1];
    expect(configArg.intervalMs).toBe(500);
  });

  it('shows the failure breakdown when failures exist', async () => {
    (runLoadTest as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({
      totalRequests: 10,
      succeeded: 5,
      failed: 5,
      failedTransport: 2,
      failedStatus: 3,
      minLatencyMs: 1,
      avgLatencyMs: 1,
      p50LatencyMs: 1,
      p95LatencyMs: 1,
      p99LatencyMs: 1,
      maxLatencyMs: 1,
      requestsPerSecond: 1,
      totalDurationMs: 1,
    });

    render(
      <LoadTestDialog
        open
        onOpenChange={() => {}}
        request={makeRequest()}
        tabId='t1'
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /^run$/i }));

    await waitFor(() =>
      expect(screen.getByText(/3 status, 2 transport/i)).toBeInTheDocument(),
    );
  });
});
```

- [ ] **Step 3: Run the new test file**

```bash
yarn test src/components/request/__tests__/LoadTestDialog.test.tsx --run 2>&1 | tail -20
```

Expected: 3 tests pass. If the `RequestState` shape used by `makeRequest` doesn't match the real type (the cast `as unknown as RequestState` is a deliberate escape hatch), inspect the error and adjust the mock to add any missing required fields — the point of the test is the interval forwarding and breakdown rendering, not full type fidelity.

- [ ] **Step 4: Run the full frontend test suite**

```bash
yarn test --run 2>&1 | tail -10
```

Expected: all tests pass (existing + 3 new).

- [ ] **Step 5: Commit**

```bash
git add src/components/request/__tests__/LoadTestDialog.test.tsx
git commit -m "test(frontend): cover LoadTestDialog interval forwarding and failure breakdown"
```

---

## Task 7: Final verification

- [ ] **Step 1: Full Rust check and test**

```bash
cd /home/numericlabs/data/rocket/rocket
cargo check && cargo test -p rocket-http -p rocket-app -p rocket-infra 2>&1 | tail -20
```

Expected: compile clean; all tests pass.

- [ ] **Step 2: Full frontend check and test**

```bash
yarn tsc --noEmit && yarn check && yarn test --run 2>&1 | tail -15
```

Expected: no TypeScript errors; no lint errors (only the pre-existing `CollectionOverviewTab.tsx` non-null-assertion warning); all tests pass.

- [ ] **Step 3: Manual smoke test** (optional but recommended)

Launch the app (`yarn tauri dev`), open a request against a backend you control, open the load test dialog, and verify:

1. The "Delay between requests (s)" input is visible and accepts decimals (try `0.5`).
2. Running a load test against a healthy endpoint shows `0 failed` and reasonable latency (much lower than before for HTTPS — client caching means only the first request pays the handshake).
3. Stop the backend mid-test or point at an endpoint returning 500; the result shows non-zero `failed` with a sub-line like `"N status, M transport"`.
4. With `delay = 1` and `total = 5`, the test takes ~4 seconds (4 gaps × 1s).
