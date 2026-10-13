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

/// What a permission option does. Serialized as snake_case, like the ACP
/// wire values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

impl PermissionOptionKind {
    /// The snake_case name used on events and in the frontend.
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionOptionKind::AllowOnce => "allow_once",
            PermissionOptionKind::AllowAlways => "allow_always",
            PermissionOptionKind::RejectOnce => "reject_once",
            PermissionOptionKind::RejectAlways => "reject_always",
        }
    }
}

/// One answer an agent offers to a permission request. `id` is the agent's
/// own option id and is the only value Rocket ever sends back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    pub kind: PermissionOptionKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_option_serializes_with_a_snake_case_kind() {
        let option = PermissionOption {
            id: "proceed_always_server".to_string(),
            name: "Allow all server tools for this session".to_string(),
            kind: PermissionOptionKind::AllowAlways,
        };
        assert_eq!(
            serde_json::to_string(&option).expect("serialize"),
            r#"{"id":"proceed_always_server","name":"Allow all server tools for this session","kind":"allow_always"}"#
        );
        assert_eq!(PermissionOptionKind::RejectOnce.as_str(), "reject_once");
    }

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
