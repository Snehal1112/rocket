//! The port between event producers and the event bus.
//!
//! A bus publishes any type that implements `PublishedEvent`. The wire format
//! stays whatever `to_json` returns.

use std::any::Any;

/// An event the bus can publish to the frontend.
pub trait PublishedEvent: Send + Sync + 'static {
    /// The frontend channel this event is emitted on.
    fn channel(&self) -> &'static str;

    /// The event as a JSON value, before the bus adds its envelope keys.
    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error>;

    /// Extra channels that get the same payload.
    fn also_emit_on(&self) -> &'static [&'static str] {
        &[]
    }

    /// The collections this event says the app wrote.
    fn written_collections(&self) -> Vec<&str> {
        Vec::new()
    }

    /// The event as `Any`, so a listener can downcast it.
    fn as_any(&self) -> &dyn Any;
}

impl dyn PublishedEvent {
    /// The event as `T`, or `None` when it is another type.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Debug, Serialize)]
    #[serde(tag = "type", rename_all = "camelCase")]
    enum Probe {
        Ping,
    }

    impl PublishedEvent for Probe {
        fn channel(&self) -> &'static str {
            "probe"
        }
        fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
            serde_json::to_value(self)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[derive(Debug, Serialize)]
    struct Other;

    impl PublishedEvent for Other {
        fn channel(&self) -> &'static str {
            "other"
        }
        fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
            serde_json::to_value(self)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn downcast_ref_returns_the_concrete_event() {
        let event: Box<dyn PublishedEvent> = Box::new(Probe::Ping);
        assert!(event.downcast_ref::<Probe>().is_some());
    }

    #[test]
    fn downcast_ref_is_none_for_another_type() {
        let event: Box<dyn PublishedEvent> = Box::new(Probe::Ping);
        assert!(event.downcast_ref::<Other>().is_none());
    }

    #[test]
    fn default_hooks_are_empty() {
        let event = Probe::Ping;
        assert!(event.also_emit_on().is_empty());
        assert!(event.written_collections().is_empty());
    }
}
