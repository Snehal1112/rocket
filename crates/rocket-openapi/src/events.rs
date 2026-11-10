//! Events the OpenAPI sync publishes.

use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

/// A refresh signal from the OpenAPI sync. The payloads carry no URL, name or path.
// Only the unit tests below read events back, so no Deserialize is derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OpenApiSyncEvent {
    /// Emitted after a background OpenAPI sync check. `status` is `unchanged`,
    /// `changed` or `failed`. It is a refresh signal and carries no URL or message.
    OpenApiSyncChecked { collection: String, status: String },
    /// Emitted once after an OpenAPI sync apply finished. `applied` and `stale` are counts,
    /// so the event carries no names or paths. It is a refresh signal.
    OpenApiSyncApplied {
        collection: String,
        applied: usize,
        stale: usize,
    },
}

impl PublishedEvent for OpenApiSyncEvent {
    fn channel(&self) -> &'static str {
        match self {
            OpenApiSyncEvent::OpenApiSyncChecked { .. } => "openapi-sync-checked",
            OpenApiSyncEvent::OpenApiSyncApplied { .. } => "openapi-sync-applied",
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::published_event::PublishedEvent;
    use serde_json::json;

    /// Every variant, its JSON and its channel. The JSON was captured from
    /// the old `DomainEvent` variants.
    fn cases() -> Vec<(OpenApiSyncEvent, serde_json::Value, &'static str)> {
        vec![
            (
                OpenApiSyncEvent::OpenApiSyncChecked {
                    collection: "my-api".into(),
                    status: "changed".into(),
                },
                json!({"type":"openApiSyncChecked","collection":"my-api","status":"changed"}),
                "openapi-sync-checked",
            ),
            (
                OpenApiSyncEvent::OpenApiSyncApplied {
                    collection: "api".into(),
                    applied: 3,
                    stale: 1,
                },
                json!({"type":"openApiSyncApplied","collection":"api","applied":3,"stale":1}),
                "openapi-sync-applied",
            ),
        ]
    }

    #[test]
    fn openapi_sync_events_json_are_unchanged() {
        for (event, expected, _) in cases() {
            assert_eq!(event.to_json().expect("event serializes"), expected);
        }
    }

    #[test]
    fn openapi_sync_events_keep_their_key_order() {
        let (checked, _, _) = &cases()[0];
        assert_eq!(
            serde_json::to_string(checked).expect("serialize"),
            r#"{"type":"openApiSyncChecked","collection":"my-api","status":"changed"}"#
        );
        let (applied, _, _) = &cases()[1];
        assert_eq!(
            serde_json::to_string(applied).expect("serialize"),
            r#"{"type":"openApiSyncApplied","collection":"api","applied":3,"stale":1}"#
        );
    }

    #[test]
    fn openapi_sync_events_keep_their_channels() {
        for (event, _, channel) in cases() {
            assert_eq!(event.channel(), channel);
        }
    }

    #[test]
    fn openapi_sync_events_have_no_extra_channels_or_written_collections() {
        for (event, _, _) in cases() {
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }
}
