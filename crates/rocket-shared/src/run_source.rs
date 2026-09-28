use serde::{Deserialize, Serialize};

/// Distinguishes how a request execution was triggered, so `HistoryEntry`
/// (and later, IPC-level execution inputs) can tell an agent-driven run
/// apart from a manual one. `Manual` is the default so every existing call
/// site that builds a `RunSource`-carrying type without setting this field
/// keeps its current (manual) behavior unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunSource {
    #[default]
    Manual,
    Runner,
    LoadTest,
    Flow,
    Agent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_manual() {
        assert_eq!(RunSource::default(), RunSource::Manual);
    }

    #[test]
    fn serializes_as_snake_case_string() {
        assert_eq!(serde_json::to_string(&RunSource::Manual).unwrap(), r#""manual""#);
        assert_eq!(serde_json::to_string(&RunSource::Runner).unwrap(), r#""runner""#);
        assert_eq!(
            serde_json::to_string(&RunSource::LoadTest).unwrap(),
            r#""load_test""#
        );
        assert_eq!(serde_json::to_string(&RunSource::Flow).unwrap(), r#""flow""#);
        assert_eq!(serde_json::to_string(&RunSource::Agent).unwrap(), r#""agent""#);
    }

    #[test]
    fn round_trips_through_json() {
        for source in [
            RunSource::Manual,
            RunSource::Runner,
            RunSource::LoadTest,
            RunSource::Flow,
            RunSource::Agent,
        ] {
            let json = serde_json::to_string(&source).expect("serialize");
            let back: RunSource = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, source);
        }
    }
}
