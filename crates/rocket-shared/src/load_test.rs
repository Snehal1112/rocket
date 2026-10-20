//! Load test payloads that travel as domain events.

use serde::{Deserialize, Serialize};

/// Published as a domain event every 250 ms during a running load test.
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
