use crate::{HttpExecutor, HttpRequest};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Configuration for a load test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadTestConfig {
    pub concurrency: u32,
    pub total_requests: u32,
}

/// Aggregated statistics from a completed load test.
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

/// Returns the value at percentile p (0–100) from a sorted slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
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
    for _ in 0..total {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let req = request.clone();
        let exec = executor.clone();
        let handle = tokio::spawn(async move {
            let result = exec.execute(&req).await;
            drop(permit);
            match result {
                Ok(resp) => (true, resp.duration_ms as f64),
                Err(_) => (false, 0.0),
            }
        });
        handles.push(handle);
    }

    let mut succeeded: u32 = 0;
    let mut failed: u32 = 0;
    let mut latencies = Vec::with_capacity(total);

    for handle in handles {
        match handle.await {
            Ok((true, ms)) => {
                succeeded += 1;
                latencies.push(ms);
            }
            _ => {
                failed += 1;
            }
        }
    }

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
}
