//! Load test payloads and the events the load test context publishes.

use rocket_shared::published_event::PublishedEvent;
use serde::{Deserialize, Serialize};

/// Something the load test reported while it runs or once it is done.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LoadTestEvent {
    /// Emitted every 250 ms while a load test runs.
    LoadTestProgress(LoadTestProgressEvent),
    /// Emitted once when a load test has finished.
    LoadTestComplete(LoadTestResult),
}

impl PublishedEvent for LoadTestEvent {
    fn channel(&self) -> &'static str {
        match self {
            LoadTestEvent::LoadTestProgress(_) => "load_test_progress",
            LoadTestEvent::LoadTestComplete(_) => "load_test_complete",
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::published_event::PublishedEvent;
    use serde_json::json;

    fn progress() -> LoadTestProgressEvent {
        LoadTestProgressEvent {
            elapsed_ms: 250,
            completed: 3,
            active_concurrent: 2,
            succeeded: 2,
            failed_status: 1,
            failed_transport: 0,
            requests_per_second: 12.0,
            p50_ms: 1.0,
            p95_ms: 2.0,
            p99_ms: 3.0,
            current_phase_index: 0,
            recent_log: vec![RequestLogEntry {
                seq: 1,
                status: Some(200),
                latency_ms: 1.5,
                response_bytes: 10,
                error: None,
                phase_index: 0,
            }],
        }
    }

    fn complete() -> LoadTestResult {
        LoadTestResult {
            total_requests: 3,
            succeeded: 2,
            failed: 1,
            failed_transport: 0,
            failed_status: 1,
            min_latency_ms: 1.0,
            avg_latency_ms: 2.0,
            p50_latency_ms: 2.0,
            p95_latency_ms: 3.0,
            p99_latency_ms: 3.0,
            max_latency_ms: 3.0,
            requests_per_second: 5.0,
            total_duration_ms: 600.0,
            phase_timeline: vec![],
            request_log: vec![],
            time_series: vec![],
        }
    }

    #[test]
    fn progress_event_json_is_unchanged() {
        let event = LoadTestEvent::LoadTestProgress(progress());
        assert_eq!(
            event.to_json().expect("serializes"),
            json!({"type":"loadTestProgress","elapsedMs":250,"completed":3,"activeConcurrent":2,"succeeded":2,"failedStatus":1,"failedTransport":0,"requestsPerSecond":12.0,"p50Ms":1.0,"p95Ms":2.0,"p99Ms":3.0,"currentPhaseIndex":0,"recentLog":[{"seq":1,"status":200,"latencyMs":1.5,"responseBytes":10,"error":null,"phaseIndex":0}]})
        );
    }

    #[test]
    fn complete_event_json_is_unchanged() {
        let event = LoadTestEvent::LoadTestComplete(complete());
        assert_eq!(
            event.to_json().expect("serializes"),
            json!({"type":"loadTestComplete","totalRequests":3,"succeeded":2,"failed":1,"failedTransport":0,"failedStatus":1,"minLatencyMs":1.0,"avgLatencyMs":2.0,"p50LatencyMs":2.0,"p95LatencyMs":3.0,"p99LatencyMs":3.0,"maxLatencyMs":3.0,"requestsPerSecond":5.0,"totalDurationMs":600.0,"phaseTimeline":[],"requestLog":[],"timeSeries":[]})
        );
    }

    #[test]
    fn load_test_events_keep_their_channels() {
        assert_eq!(
            LoadTestEvent::LoadTestProgress(progress()).channel(),
            "load_test_progress"
        );
        assert_eq!(
            LoadTestEvent::LoadTestComplete(complete()).channel(),
            "load_test_complete"
        );
    }

    #[test]
    fn progress_payload_keys_stay_camel_case() {
        let value = LoadTestEvent::LoadTestProgress(progress())
            .to_json()
            .expect("serializes");
        for key in ["elapsedMs", "requestsPerSecond", "recentLog"] {
            assert!(value.get(key).is_some(), "missing key {key}");
        }
    }

    #[test]
    fn result_without_the_default_lists_still_deserializes() {
        let value = json!({
            "totalRequests": 3, "succeeded": 2, "failed": 1, "failedTransport": 0,
            "failedStatus": 1, "minLatencyMs": 1.0, "avgLatencyMs": 2.0,
            "p50LatencyMs": 2.0, "p95LatencyMs": 3.0, "p99LatencyMs": 3.0,
            "maxLatencyMs": 3.0, "requestsPerSecond": 5.0, "totalDurationMs": 600.0
        });
        let result: LoadTestResult = serde_json::from_value(value).expect("deserializes");
        assert!(result.phase_timeline.is_empty());
        assert!(result.request_log.is_empty());
        assert!(result.time_series.is_empty());
    }
}
