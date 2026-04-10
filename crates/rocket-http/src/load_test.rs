use crate::{HttpExecutor, HttpRequest};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpResponse, RequestOptions};
    use async_trait::async_trait;
    use rocket_shared::error::DomainResult;
    use rocket_shared::types::{Auth, HttpMethod};

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
