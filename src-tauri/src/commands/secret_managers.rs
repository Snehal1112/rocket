use rocket_app::SecretManagerService;
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::secret_manager::SecretManagerConnection;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretManagerConnectionDto {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub client_id: String,
    pub verify_ssl: bool,
    pub allow_insecure_http: bool,
}

impl From<SecretManagerConnection> for SecretManagerConnectionDto {
    fn from(c: SecretManagerConnection) -> Self {
        Self {
            id: c.id,
            label: c.label,
            base_url: c.base_url,
            client_id: c.client_id,
            verify_ssl: c.verify_ssl,
            allow_insecure_http: c.allow_insecure_http,
        }
    }
}

impl From<SecretManagerConnectionDto> for SecretManagerConnection {
    fn from(dto: SecretManagerConnectionDto) -> Self {
        Self {
            id: dto.id,
            label: dto.label,
            base_url: dto.base_url,
            client_id: dto.client_id,
            verify_ssl: dto.verify_ssl,
            allow_insecure_http: dto.allow_insecure_http,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretRefDto {
    pub name: String,
    pub secret_id: String,
}

impl From<ExternalSecretRef> for ExternalSecretRefDto {
    fn from(r: ExternalSecretRef) -> Self {
        Self {
            name: r.name,
            secret_id: r.secret_id,
        }
    }
}

#[tauri::command]
pub fn list_secret_manager_connections(
    svc: State<'_, SecretManagerService>,
) -> Result<Vec<SecretManagerConnectionDto>, DomainError> {
    Ok(svc.list()?.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn save_secret_manager_connection(
    connection: SecretManagerConnectionDto,
    client_secret: Option<String>,
    svc: State<'_, SecretManagerService>,
) -> Result<(), DomainError> {
    svc.save(connection.into(), client_secret)
}

#[tauri::command]
pub fn delete_secret_manager_connection(
    id: String,
    svc: State<'_, SecretManagerService>,
) -> Result<(), DomainError> {
    svc.delete(&id)
}

#[tauri::command]
pub async fn test_secret_manager_connection(
    id: String,
    vault_name: String,
    svc: State<'_, SecretManagerService>,
) -> Result<(), DomainError> {
    svc.test_connection(&id, &vault_name).await
}

#[tauri::command]
pub async fn fetch_external_secret_names(
    id: String,
    vault_name: String,
    svc: State<'_, SecretManagerService>,
) -> Result<Vec<ExternalSecretRefDto>, DomainError> {
    Ok(svc
        .fetch_secret_names(&id, &vault_name)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}
