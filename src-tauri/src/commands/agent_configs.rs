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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigDto {
    pub id: String,
    pub label: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_domain() -> AgentConfig {
        AgentConfig {
            id: "agent-1".to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: vec!["--stdio".to_string()],
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
