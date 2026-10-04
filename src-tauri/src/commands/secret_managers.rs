use rocket_app::SecretManagerService;
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::secret_manager::{
    ProviderConfig, SecretManagerConnection, SecretProviderKind,
};
use rocket_environment::VaultCertificateSummary;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

/// IPC shape of `ProviderConfig`. Kept apart from the persistence enum so the
/// camelCase rename never reaches `secret_managers.yml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ProviderConfigDto {
    #[serde(rename = "azure", rename_all = "camelCase")]
    Azure {
        tenant_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authority_host: Option<String>,
    },
}

impl From<ProviderConfig> for ProviderConfigDto {
    fn from(config: ProviderConfig) -> Self {
        match config {
            ProviderConfig::Azure {
                tenant_id,
                authority_host,
            } => Self::Azure {
                tenant_id,
                authority_host,
            },
        }
    }
}

impl From<ProviderConfigDto> for ProviderConfig {
    fn from(dto: ProviderConfigDto) -> Self {
        match dto {
            ProviderConfigDto::Azure {
                tenant_id,
                authority_host,
            } => Self::Azure {
                tenant_id,
                authority_host,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretManagerConnectionDto {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub client_id: String,
    pub verify_ssl: bool,
    pub allow_insecure_http: bool,
    #[serde(default)]
    pub provider: SecretProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ProviderConfigDto>,
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
            provider: c.provider,
            config: c.config.map(Into::into),
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
            provider: dto.provider,
            config: dto.config.map(Into::into),
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

/// One certificate for the Certificates tab picker. Names and metadata only, never key
/// material, so the IPC payload cannot carry a key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultCertificateSummaryDto {
    pub id: String,
    pub name: String,
    pub exportable: bool,
    pub enabled: bool,
    pub key_algorithm: String,
    pub expires_at: Option<String>,
}

impl From<VaultCertificateSummary> for VaultCertificateSummaryDto {
    fn from(c: VaultCertificateSummary) -> Self {
        Self {
            id: c.id,
            name: c.name,
            exportable: c.exportable,
            enabled: c.enabled,
            key_algorithm: c.key_algorithm,
            expires_at: c.expires_at,
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

#[tauri::command]
pub async fn list_vault_certificates(
    connection_id: String,
    vault_name: String,
    svc: State<'_, SecretManagerService>,
) -> Result<Vec<VaultCertificateSummaryDto>, DomainError> {
    Ok(svc
        .list_certificates(&connection_id, &vault_name)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dto_is_tagged_and_camel_case() {
        let dto = ProviderConfigDto::from(ProviderConfig::Azure {
            tenant_id: "tenant-1".to_string(),
            authority_host: Some("http://127.0.0.1:1".to_string()),
        });
        let json = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "azure",
                "tenantId": "tenant-1",
                "authorityHost": "http://127.0.0.1:1"
            })
        );
    }

    #[test]
    fn config_dto_omits_an_unset_authority_host() {
        let dto = ProviderConfigDto::from(ProviderConfig::Azure {
            tenant_id: "tenant-1".to_string(),
            authority_host: None,
        });
        let json = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({ "kind": "azure", "tenantId": "tenant-1" })
        );
    }

    #[test]
    fn azure_connection_dto_round_trips_its_config() {
        let json = serde_json::json!({
            "id": "c1",
            "label": "L",
            "baseUrl": "https://v.vault.azure.net",
            "clientId": "app",
            "verifySsl": true,
            "allowInsecureHttp": false,
            "provider": "azure",
            "config": { "kind": "azure", "tenantId": "t1" }
        });
        let dto: SecretManagerConnectionDto =
            serde_json::from_value(json.clone()).expect("deserialize");
        let conn: SecretManagerConnection = dto.into();
        assert_eq!(
            conn.config,
            Some(ProviderConfig::Azure {
                tenant_id: "t1".to_string(),
                authority_host: None
            })
        );
        let back = serde_json::to_value(SecretManagerConnectionDto::from(conn)).expect("serialize");
        assert_eq!(back, json);
    }

    #[test]
    fn vault_certificate_dto_is_camel_case_and_carries_no_material() {
        let dto = VaultCertificateSummaryDto::from(VaultCertificateSummary {
            id: "id-1".into(),
            name: "client-a".into(),
            exportable: true,
            enabled: false,
            key_algorithm: "EC-P256".into(),
            expires_at: None,
        });
        let json = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "id": "id-1",
                "name": "client-a",
                "exportable": true,
                "enabled": false,
                "keyAlgorithm": "EC-P256",
                "expiresAt": null
            })
        );
    }

    #[test]
    fn connection_dto_without_provider_deserializes_as_rocketvault() {
        let json = r#"{"id":"c1","label":"L","baseUrl":"https://v","clientId":"x","verifySsl":true,"allowInsecureHttp":false}"#;
        let dto: SecretManagerConnectionDto = serde_json::from_str(json).expect("older payload");
        let conn: SecretManagerConnection = dto.into();
        assert_eq!(
            conn.provider,
            rocket_environment::SecretProviderKind::RocketVault
        );
        assert!(conn.config.is_none());
    }

    #[test]
    fn connection_dto_round_trips_the_provider_as_lowercase() {
        let mut conn = SecretManagerConnection {
            id: "c1".to_string(),
            label: "L".to_string(),
            base_url: String::new(),
            client_id: String::new(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: rocket_environment::SecretProviderKind::Azure,
            config: None,
        };
        let json = serde_json::to_value(SecretManagerConnectionDto::from(conn.clone()))
            .expect("serialize");
        assert_eq!(json["provider"], "azure");
        assert!(json.get("config").is_none());

        let back: SecretManagerConnection =
            serde_json::from_value::<SecretManagerConnectionDto>(json)
                .expect("deserialize")
                .into();
        conn.base_url = String::new();
        assert_eq!(back, conn);
    }
}
