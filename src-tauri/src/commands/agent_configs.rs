use rocket_acp::AgentConfig;
use rocket_app::AgentConfigService;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigDto {
    pub id: String,
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub credential_env_var: String,
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
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "secret-id-1".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    #[test]
    fn dto_serializes_camelcase() {
        let dto: AgentConfigDto = sample_domain().into();
        let json = serde_json::to_string(&dto).expect("serialize AgentConfigDto");
        assert!(json.contains("\"credentialEnvVar\""), "expected camelCase, got: {json}");
        assert!(json.contains("\"vaultConnectionId\""), "expected camelCase, got: {json}");
        assert!(json.contains("\"workingDir\""), "expected camelCase, got: {json}");
    }

    #[test]
    fn dto_roundtrips_through_domain_type() {
        let original = sample_domain();
        let dto: AgentConfigDto = original.clone().into();
        let back: AgentConfig = dto.into();
        assert_eq!(original, back);
    }
}
