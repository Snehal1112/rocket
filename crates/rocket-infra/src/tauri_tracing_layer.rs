use serde::Serialize;
use std::collections::HashMap;
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

/// A tracing Layer that forwards log events (INFO and above) to the
/// Tauri frontend via the "backend-log" event channel.
pub struct TauriTracingLayer {
    app_handle: AppHandle,
}

impl TauriTracingLayer {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
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
            message: visitor.message.unwrap_or_default(),
            fields: visitor.fields,
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
}
