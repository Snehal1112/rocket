//! ACP session option values. They live here, not in `rocket-acp`, because
//! `DomainEvent::AcpConfigOptionsChanged` carries them and this crate depends
//! on no other workspace crate. `rocket-acp` re-exports both types.

use serde::{Deserialize, Serialize};

/// One session setting the agent reports, such as the model or the effort
/// level. Only select-style options are represented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigOption {
    /// The agent's option id, for example `model` or `effort`.
    pub id: String,
    pub name: String,
    /// The agent's category, for example `model` or `thought_level`.
    pub category: Option<String>,
    pub current_value: String,
    pub choices: Vec<ConfigChoice>,
}

/// One value a `ConfigOption` can take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigChoice {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_option_serializes_with_snake_case_keys() {
        let option = ConfigOption {
            id: "model".to_string(),
            name: "Model".to_string(),
            category: Some("model".to_string()),
            current_value: "opus".to_string(),
            choices: vec![ConfigChoice {
                value: "opus".to_string(),
                name: "Opus".to_string(),
                description: None,
            }],
        };
        let json = serde_json::to_string(&option).expect("serialize");
        assert_eq!(
            json,
            r#"{"id":"model","name":"Model","category":"model","current_value":"opus","choices":[{"value":"opus","name":"Opus","description":null}]}"#
        );
        let back: ConfigOption = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, option);
    }
}
