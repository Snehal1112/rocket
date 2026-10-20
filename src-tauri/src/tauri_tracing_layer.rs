use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// A structured log entry emitted to the frontend via Tauri events.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendLogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: HashMap<String, String>,
    pub span_fields: HashMap<String, String>,
}

/// The most INFO lines sent to the frontend in one `LOG_LIMIT_WINDOW`.
/// WARN and ERROR lines are always sent.
const MAX_INFO_LINES_PER_WINDOW: u32 = 200;

/// The window `MAX_INFO_LINES_PER_WINDOW` counts in.
const LOG_LIMIT_WINDOW: Duration = Duration::from_secs(1);

/// The largest message or field value one log entry carries, in bytes.
const MAX_LOG_TEXT_BYTES: usize = 16 * 1024;

/// What to do with one log line.
#[derive(Debug, PartialEq, Eq)]
enum LogDecision {
    /// Send it. When lines were dropped before it, send a summary first.
    Send { suppressed_before: u64 },
    /// Drop it and count it.
    Drop,
}

/// Limits INFO lines per window. Pure: the caller passes the time in.
#[derive(Debug, Default)]
struct LogRateLimiter {
    window_start: Option<Instant>,
    sent_in_window: u32,
    suppressed: u64,
}

impl LogRateLimiter {
    fn decide(&mut self, always_send: bool, now: Instant) -> LogDecision {
        let window_over = self
            .window_start
            .is_none_or(|start| now.saturating_duration_since(start) >= LOG_LIMIT_WINDOW);
        if window_over {
            self.window_start = Some(now);
            self.sent_in_window = 0;
        }
        if always_send || self.sent_in_window < MAX_INFO_LINES_PER_WINDOW {
            self.sent_in_window = self.sent_in_window.saturating_add(1);
            LogDecision::Send {
                suppressed_before: std::mem::take(&mut self.suppressed),
            }
        } else {
            self.suppressed = self.suppressed.saturating_add(1);
            LogDecision::Drop
        }
    }
}

/// A tracing Layer that forwards log events (INFO and above) to the
/// Tauri frontend via the "backend-log" event channel. INFO lines are rate
/// limited. Nothing in the emit path logs through `tracing`, so the layer
/// never feeds itself.
pub struct TauriTracingLayer {
    app_handle: AppHandle,
    limiter: Mutex<LogRateLimiter>,
}

impl TauriTracingLayer {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            limiter: Mutex::new(LogRateLimiter::default()),
        }
    }
}

/// The entry that says how many lines were dropped.
fn suppressed_entry(count: u64) -> BackendLogEntry {
    BackendLogEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: tracing::Level::WARN.to_string(),
        target: "rocket::backend_log".to_string(),
        message: format!("{count} log lines suppressed"),
        fields: HashMap::new(),
        span_fields: HashMap::new(),
    }
}

/// Cuts one log text to `MAX_LOG_TEXT_BYTES`, with a marker.
fn cap_log_text(text: String) -> String {
    rocket_app::event_caps::cap_with_marker(text, MAX_LOG_TEXT_BYTES).0
}

struct FieldVisitor {
    fields: HashMap<String, String>,
    message: Option<String>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self {
            fields: HashMap::new(),
            message: None,
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        } else {
            self.fields
                .insert(field.name().to_string(), format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

impl<S> Layer<S> for TauriTracingLayer
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if *metadata.level() > tracing::Level::INFO {
            return;
        }
        // Decide before any work, so a dropped line costs no allocation.
        let always_send = *metadata.level() <= tracing::Level::WARN;
        let decision = self
            .limiter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .decide(always_send, Instant::now());
        let suppressed_before = match decision {
            LogDecision::Drop => return,
            LogDecision::Send { suppressed_before } => suppressed_before,
        };
        if suppressed_before > 0 {
            let _ = self
                .app_handle
                .emit("backend-log", &suppressed_entry(suppressed_before));
        }

        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);

        let mut span_fields = HashMap::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                if let Some(fields) = extensions.get::<HashMap<String, String>>() {
                    span_fields.extend(fields.clone());
                }
            }
        }

        let entry = BackendLogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message: cap_log_text(visitor.message.unwrap_or_default()),
            fields: visitor
                .fields
                .into_iter()
                .map(|(name, value)| (name, cap_log_text(value)))
                .collect(),
            span_fields,
        };

        let _ = self.app_handle.emit("backend-log", &entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_log_entry_serializes_camel_case() {
        let entry = BackendLogEntry {
            timestamp: "2026-04-12T10:00:00Z".to_string(),
            level: "INFO".to_string(),
            target: "rocket_app::execution_service".to_string(),
            message: "request completed".to_string(),
            fields: HashMap::from([("status".into(), "200".into())]),
            span_fields: HashMap::from([("method".into(), "GET".into())]),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"spanFields\""));
        assert!(json.contains("\"target\""));
        assert!(!json.contains("span_fields"));
    }

    #[test]
    fn info_lines_over_the_limit_are_dropped_and_counted() {
        let start = Instant::now();
        let mut limiter = LogRateLimiter::default();
        for _ in 0..MAX_INFO_LINES_PER_WINDOW {
            assert_eq!(
                limiter.decide(false, start),
                LogDecision::Send {
                    suppressed_before: 0
                }
            );
        }
        assert_eq!(limiter.decide(false, start), LogDecision::Drop);
        assert_eq!(limiter.decide(false, start), LogDecision::Drop);
        // The next window sends again and reports the two dropped lines once.
        let later = start + LOG_LIMIT_WINDOW;
        assert_eq!(
            limiter.decide(false, later),
            LogDecision::Send {
                suppressed_before: 2
            }
        );
        assert_eq!(
            limiter.decide(false, later),
            LogDecision::Send {
                suppressed_before: 0
            }
        );
    }

    #[test]
    fn warn_and_error_lines_are_always_sent() {
        let start = Instant::now();
        let mut limiter = LogRateLimiter::default();
        for _ in 0..MAX_INFO_LINES_PER_WINDOW + 1 {
            limiter.decide(false, start);
        }
        // One INFO line was dropped; a WARN still goes out and reports it.
        assert_eq!(
            limiter.decide(true, start),
            LogDecision::Send {
                suppressed_before: 1
            }
        );
        assert_eq!(
            limiter.decide(true, start),
            LogDecision::Send {
                suppressed_before: 0
            }
        );
    }

    #[test]
    fn a_long_log_text_is_capped_with_a_marker() {
        let capped = cap_log_text("z".repeat(MAX_LOG_TEXT_BYTES + 3));
        assert!(capped.ends_with("...[truncated 3 bytes]"));
        assert_eq!(cap_log_text("short".to_string()), "short");
    }

    #[test]
    fn the_summary_entry_is_a_warn_with_the_count() {
        let entry = suppressed_entry(7);
        assert_eq!(entry.level, "WARN");
        assert_eq!(entry.message, "7 log lines suppressed");
    }
}
