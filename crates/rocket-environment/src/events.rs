//! Events the environment context publishes.

use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

/// An environment was saved or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EnvironmentEvent {
    EnvironmentSaved { name: String },
    EnvironmentDeleted { name: String },
}

impl PublishedEvent for EnvironmentEvent {
    fn channel(&self) -> &'static str {
        match self {
            EnvironmentEvent::EnvironmentSaved { .. }
            | EnvironmentEvent::EnvironmentDeleted { .. } => "environment-changed",
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
    use serde_json::json;

    #[test]
    fn environment_events_json_are_unchanged() {
        let saved = EnvironmentEvent::EnvironmentSaved { name: "dev".into() };
        let deleted = EnvironmentEvent::EnvironmentDeleted { name: "dev".into() };
        assert_eq!(
            saved.to_json().expect("serializes"),
            json!({"type": "environmentSaved", "name": "dev"})
        );
        assert_eq!(
            deleted.to_json().expect("serializes"),
            json!({"type": "environmentDeleted", "name": "dev"})
        );
    }

    #[test]
    fn both_environment_events_use_one_channel() {
        let saved = EnvironmentEvent::EnvironmentSaved { name: "dev".into() };
        let deleted = EnvironmentEvent::EnvironmentDeleted { name: "dev".into() };
        assert_eq!(saved.channel(), "environment-changed");
        assert_eq!(deleted.channel(), "environment-changed");
    }

    #[test]
    fn environment_events_have_no_extra_channels_or_written_collections() {
        let saved = EnvironmentEvent::EnvironmentSaved { name: "dev".into() };
        let deleted = EnvironmentEvent::EnvironmentDeleted { name: "dev".into() };
        for event in [saved, deleted] {
            assert!(event.also_emit_on().is_empty());
            assert!(event.written_collections().is_empty());
        }
    }
}
