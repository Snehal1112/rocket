use serde::{Deserialize, Serialize};

/// One registered ACP agent binary/command and where to find its API key.
/// The credential *value* never lives on this struct — only a reference to
/// where RocketVault holds it (`vault_connection_id`/`vault_name`/
/// `vault_secret_id`), resolved on demand by `AgentConfigService`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    pub credential_env_var: String,
    pub vault_connection_id: String,
    pub vault_name: String,
    pub vault_secret_id: String,
    pub vault_secret_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AgentConfig {
        AgentConfig {
            id: "agent-1".to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: vec!["--stdio".to_string()],
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    #[test]
    fn agent_config_serde_roundtrip_no_camelcase() {
        let c = sample();
        let json = serde_json::to_string(&c).expect("serialize AgentConfig");
        assert!(
            json.contains("\"credential_env_var\""),
            "expected snake_case field, got: {json}"
        );
        assert!(
            json.contains("\"vault_connection_id\""),
            "expected snake_case field, got: {json}"
        );
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert_eq!(c, back);
    }

    #[test]
    fn agent_config_args_roundtrips_when_empty() {
        let mut c = sample();
        c.args = Vec::new();
        let json = serde_json::to_string(&c).expect("serialize AgentConfig");
        let back: AgentConfig = serde_json::from_str(&json).expect("deserialize AgentConfig");
        assert!(back.args.is_empty());
    }

    #[test]
    fn agent_config_working_dir_defaults_to_none_when_absent() {
        let json = r#"{
            "id":"agent-1","label":"Claude Agent","command":"claude-agent-acp",
            "credential_env_var":"ANTHROPIC_API_KEY","vault_connection_id":"conn-1",
            "vault_name":"prod-vault","vault_secret_id":"b6f1c2e0-1234-4a5b-9abc-000000000001",
            "vault_secret_name":"anthropic-api-key"
        }"#;
        let c: AgentConfig = serde_json::from_str(json).expect("deserialize minimal AgentConfig");
        assert_eq!(c.working_dir, None);
        assert!(c.args.is_empty());
    }
}
