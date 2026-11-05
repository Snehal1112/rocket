//! The publisher port: `EventPublisher` and `NullEventPublisher`.

pub use crate::published_event::PublishedEvent;

/// Trait for publishing domain events.
/// Implemented by TauriEventBus in infrastructure layer.
pub trait EventPublisher: Send + Sync {
    fn publish(&self, event: &dyn PublishedEvent);
}

/// No-op publisher for tests and contexts where events aren't needed.
pub struct NullEventPublisher;

impl EventPublisher for NullEventPublisher {
    fn publish(&self, _event: &dyn PublishedEvent) {
        // Intentionally empty.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[test]
    fn event_publisher_trait_is_object_safe() {
        fn _assert_object_safe(_: Box<dyn EventPublisher>) {}
    }

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

    #[test]
    fn null_publisher_does_not_panic() {
        NullEventPublisher.publish(&Probe::Ping);
    }

    #[test]
    fn null_publisher_accepts_a_published_event() {
        let event: Box<dyn PublishedEvent> = Box::new(Probe::Ping);
        NullEventPublisher.publish(event.as_ref());
    }
}
