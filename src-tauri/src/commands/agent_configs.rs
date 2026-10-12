use rocket_acp::{AgentConfig, CredentialSource, EnvPair};
use rocket_app::AgentConfigService;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

/// One plain environment variable. `Debug` hides the value.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvPairDto {
    pub name: String,
    pub value: String,
}

impl std::fmt::Debug for EnvPairDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvPairDto")
            .field("name", &self.name)
            .field("value", &"***")
            .finish()
    }
}

/// Per-agent tuning on the wire.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTuningDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_timeout_secs: Option<u32>,
    #[serde(default)]
    pub reasoning: rocket_acp::ReasoningLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigDto {
    pub id: String,
    pub label: String,
    // Old callers may omit it.
    #[serde(default)]
    pub kind: rocket_acp::AgentKind,
    // Old callers may omit it.
    #[serde(default)]
    pub tuning: AgentTuningDto,
    pub command: String,
    pub args: Vec<String>,
    // Omitted when unset, to match the frontend's optional `workingDir?: string`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    pub credential_env_var: String,
    // Old callers may omit it.
    #[serde(default)]
    pub extra_env: Vec<EnvPairDto>,
    // Old callers may omit it.
    #[serde(default)]
    pub credential_source: CredentialSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_env_var: Option<String>,
    pub vault_connection_id: String,
    pub vault_name: String,
    pub vault_secret_id: String,
    pub vault_secret_name: String,
}

impl From<AgentConfig> for AgentConfigDto {
    fn from(c: AgentConfig) -> Self {
        Self {
            id: c.id,
            label: c.label,
            kind: c.kind,
            tuning: AgentTuningDto {
                idle_timeout_secs: c.tuning.idle_timeout_secs,
                reasoning: c.tuning.reasoning,
                model: c.tuning.model,
            },
            command: c.command,
            args: c.args,
            working_dir: c.working_dir,
            credential_env_var: c.credential_env_var,
            extra_env: c
                .extra_env
                .into_iter()
                .map(|p| EnvPairDto {
                    name: p.name,
                    value: p.value,
                })
                .collect(),
            credential_source: c.credential_source,
            source_env_var: c.source_env_var,
            vault_connection_id: c.vault_connection_id,
            vault_name: c.vault_name,
            vault_secret_id: c.vault_secret_id,
            vault_secret_name: c.vault_secret_name,
        }
    }
}

impl From<AgentConfigDto> for AgentConfig {
    fn from(dto: AgentConfigDto) -> Self {
        Self {
            id: dto.id,
            label: dto.label,
            kind: dto.kind,
            tuning: rocket_acp::AgentTuning {
                idle_timeout_secs: dto.tuning.idle_timeout_secs,
                reasoning: dto.tuning.reasoning,
                model: dto.tuning.model,
            },
            command: dto.command,
            args: dto.args,
            working_dir: dto.working_dir,
            credential_env_var: dto.credential_env_var,
            extra_env: dto
                .extra_env
                .into_iter()
                .map(|p| EnvPair {
                    name: p.name,
                    value: p.value,
                })
                .collect(),
            credential_source: dto.credential_source,
            source_env_var: dto.source_env_var,
            vault_connection_id: dto.vault_connection_id,
            vault_name: dto.vault_name,
            vault_secret_id: dto.vault_secret_id,
            vault_secret_name: dto.vault_secret_name,
        }
    }
}

#[tauri::command]
pub fn list_agent_configs(
    svc: State<'_, AgentConfigService>,
) -> Result<Vec<AgentConfigDto>, DomainError> {
    Ok(svc.list()?.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn save_agent_config(
    config: AgentConfigDto,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.save(config.into())
}

#[tauri::command]
pub fn delete_agent_config(
    id: String,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.delete(&id)
}

#[tauri::command]
pub async fn test_agent_config(
    id: String,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.test_agent_config(&id).await
}

#[tauri::command]
pub async fn set_agent_credential(
    config_id: String,
    value: String,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.set_local_credential(&config_id, &value)
}

#[tauri::command]
pub async fn clear_agent_credential(
    config_id: String,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.clear_local_credential(&config_id)
}

#[tauri::command]
pub async fn has_agent_credential(
    config_id: String,
    svc: State<'_, AgentConfigService>,
) -> Result<bool, DomainError> {
    svc.has_local_credential(&config_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dto_carries_reasoning_and_model_and_old_callers_get_high() {
        let mut domain = sample_domain();
        domain.tuning.reasoning = rocket_acp::ReasoningLevel::Low;
        domain.tuning.model = Some("gemini-2.5-pro".to_string());
        let json = serde_json::to_value(AgentConfigDto::from(domain)).expect("json");
        assert_eq!(json["tuning"]["reasoning"], "low");
        assert_eq!(json["tuning"]["model"], "gemini-2.5-pro");
        let back: AgentConfig = serde_json::from_value::<AgentConfigDto>(json)
            .expect("dto")
            .into();
        assert_eq!(back.tuning.reasoning, rocket_acp::ReasoningLevel::Low);
        assert_eq!(back.tuning.model.as_deref(), Some("gemini-2.5-pro"));

        let old = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "args": [],
            "credentialEnvVar": "K", "vaultConnectionId": "", "vaultName": "",
            "vaultSecretId": "", "vaultSecretName": ""
        });
        let dto: AgentConfigDto = serde_json::from_value(old).expect("old caller");
        let config = AgentConfig::from(dto);
        assert_eq!(config.tuning.reasoning, rocket_acp::ReasoningLevel::High);
        assert_eq!(config.tuning.model, None);
    }

    fn sample_domain() -> AgentConfig {
        AgentConfig {
            id: "agent-1".to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: vec!["--stdio".to_string()],
            kind: rocket_acp::AgentKind::default(),
            tuning: rocket_acp::AgentTuning::default(),
            working_dir: Some("/home/user/project".to_string()),
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            extra_env: vec![EnvPair {
                name: "HTTPS_PROXY".to_string(),
                value: "http://proxy:3128".to_string(),
            }],
            credential_source: CredentialSource::Vault,
            source_env_var: None,
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "secret-id-1".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    #[test]
    fn the_dto_carries_kind_in_camel_case_json_and_defaults_to_claude() {
        let mut domain = sample_domain();
        domain.kind = rocket_acp::AgentKind::Generic;
        let dto = AgentConfigDto::from(domain);
        let json = serde_json::to_value(&dto).expect("json");
        assert_eq!(json["kind"], "generic");
        let back: AgentConfig = serde_json::from_value::<AgentConfigDto>(json)
            .expect("dto")
            .into();
        assert_eq!(back.kind, rocket_acp::AgentKind::Generic);

        let old = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "args": [],
            "credentialEnvVar": "K", "vaultConnectionId": "", "vaultName": "",
            "vaultSecretId": "", "vaultSecretName": ""
        });
        let dto: AgentConfigDto = serde_json::from_value(old).expect("old caller");
        assert_eq!(AgentConfig::from(dto).kind, rocket_acp::AgentKind::Claude);
    }

    #[test]
    fn the_dto_carries_tuning_in_camel_case_and_old_callers_get_the_defaults() {
        let mut domain = sample_domain();
        domain.tuning.idle_timeout_secs = Some(300);
        let json = serde_json::to_value(AgentConfigDto::from(domain)).expect("json");
        assert_eq!(json["tuning"]["idleTimeoutSecs"], 300);

        let old = serde_json::json!({
            "id": "a", "label": "A", "command": "c", "args": [],
            "credentialEnvVar": "K", "vaultConnectionId": "", "vaultName": "",
            "vaultSecretId": "", "vaultSecretName": ""
        });
        let dto: AgentConfigDto = serde_json::from_value(old).expect("old caller");
        assert_eq!(AgentConfig::from(dto).tuning.idle_timeout_secs, None);
    }

    #[test]
    fn dto_carries_the_credential_source_in_camel_case() {
        let mut config = sample_domain();
        config.credential_source = CredentialSource::Env;
        config.source_env_var = Some("CLAUDE_CODE_OAUTH_TOKEN".to_string());
        let json = serde_json::to_value(AgentConfigDto::from(config)).expect("serialize");
        assert_eq!(json["credentialSource"], "env");
        assert_eq!(json["sourceEnvVar"], "CLAUDE_CODE_OAUTH_TOKEN");
        let old: AgentConfigDto = serde_json::from_value(serde_json::json!({
            "id": "a", "label": "A", "command": "c", "args": [], "credentialEnvVar": "K",
            "vaultConnectionId": "v", "vaultName": "n", "vaultSecretId": "s", "vaultSecretName": "sn"
        }))
        .expect("old caller");
        assert_eq!(
            AgentConfig::from(old).credential_source,
            CredentialSource::Vault
        );
    }

    #[test]
    fn dto_serializes_camelcase() {
        let dto: AgentConfigDto = sample_domain().into();
        let json = serde_json::to_string(&dto).expect("serialize AgentConfigDto");
        assert!(
            json.contains("\"credentialEnvVar\""),
            "expected camelCase, got: {json}"
        );
        assert!(
            json.contains("\"vaultConnectionId\""),
            "expected camelCase, got: {json}"
        );
        assert!(
            json.contains("\"workingDir\""),
            "expected camelCase, got: {json}"
        );
    }

    #[test]
    fn dto_omits_working_dir_when_unset() {
        let mut domain = sample_domain();
        domain.working_dir = None;
        let dto: AgentConfigDto = domain.into();
        let json = serde_json::to_string(&dto).expect("serialize AgentConfigDto");
        assert!(
            !json.contains("workingDir"),
            "expected workingDir omitted, got: {json}"
        );
        let back: AgentConfigDto = serde_json::from_str(&json).expect("deserialize AgentConfigDto");
        assert_eq!(back.working_dir, None);
    }

    #[test]
    fn dto_roundtrips_through_domain_type() {
        let original = sample_domain();
        let dto: AgentConfigDto = original.clone().into();
        let back: AgentConfig = dto.into();
        assert_eq!(original, back);
    }
}
