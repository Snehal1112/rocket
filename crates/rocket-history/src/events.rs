//! Events the history context publishes.

use rocket_shared::published_event::PublishedEvent;
use serde::Serialize;

/// Something changed in the request history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HistoryEvent {
    HistoryCleared,
}

impl PublishedEvent for HistoryEvent {
    fn channel(&self) -> &'static str {
        match self {
            HistoryEvent::HistoryCleared => "history-changed",
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
    fn history_cleared_keeps_its_channel_and_json() {
        let event = HistoryEvent::HistoryCleared;
        assert_eq!(event.channel(), "history-changed");
        assert_eq!(
            event.to_json().expect("serializes"),
            json!({"type": "historyCleared"})
        );
    }

    #[test]
    fn history_cleared_has_no_extra_channels_or_written_collections() {
        let event = HistoryEvent::HistoryCleared;
        assert!(event.also_emit_on().is_empty());
        assert!(event.written_collections().is_empty());
    }
}
