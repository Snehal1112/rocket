//! IPC DTO for one folder's settings.
//!
//! The domain `FolderSettings` carries no serde. This DTO owns the camelCase
//! wire shape. Headers, auth and variables reuse the shapes that
//! `get_collection_settings` already sends, so the frontend editors work for both.

use rocket_collection::{CollectionVariable, FolderSettings};
use rocket_shared::types::{Auth, Header};
use serde::{Deserialize, Serialize};

/// Folder settings as the frontend reads and writes them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSettingsDto {
    #[serde(default)]
    pub headers: Vec<Header>,
    /// `None` and `Some(Auth::Inherit)` both mean the folder sets no auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    /// Pre-request folder variables.
    #[serde(default)]
    pub variables: Vec<CollectionVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_request_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_response_script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_script: Option<String>,
    /// Markdown docs content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

impl From<FolderSettings> for FolderSettingsDto {
    fn from(settings: FolderSettings) -> Self {
        // Destructure fully, so a new domain field fails to compile here.
        let FolderSettings {
            headers,
            auth,
            variables,
            pre_request_script,
            post_response_script,
            tests_script,
            docs,
        } = settings;
        Self {
            headers,
            auth,
            variables,
            pre_request_script,
            post_response_script,
            tests_script,
            docs,
        }
    }
}

impl From<FolderSettingsDto> for FolderSettings {
    fn from(dto: FolderSettingsDto) -> Self {
        // Destructure fully, so a new DTO field fails to compile here.
        let FolderSettingsDto {
            headers,
            auth,
            variables,
            pre_request_script,
            post_response_script,
            tests_script,
            docs,
        } = dto;
        Self {
            headers,
            auth,
            variables,
            pre_request_script,
            post_response_script,
            tests_script,
            docs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_domain() -> FolderSettings {
        FolderSettings {
            headers: vec![
                Header::new("X-Team", "core"),
                Header::disabled("X-Debug", "1"),
            ],
            auth: Some(Auth::Bearer {
                token: "{{token}}".into(),
            }),
            variables: vec![CollectionVariable {
                key: "baseUrl".into(),
                value: "https://api.example.com".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            }],
            pre_request_script: Some("console.log('pre');".into()),
            post_response_script: Some("console.log('post');".into()),
            tests_script: Some("test('ok', () => {});".into()),
            docs: Some("# Auth".into()),
        }
    }

    #[test]
    fn folder_settings_dto_round_trip_is_lossless() {
        let dto = FolderSettingsDto::from(full_domain());
        assert_eq!(FolderSettings::from(dto), full_domain());
    }

    #[test]
    fn folder_settings_dto_uses_camel_case_keys() {
        let json =
            serde_json::to_value(FolderSettingsDto::from(full_domain())).expect("serialize dto");
        assert_eq!(json["preRequestScript"], "console.log('pre');");
        assert_eq!(json["postResponseScript"], "console.log('post');");
        assert_eq!(json["testsScript"], "test('ok', () => {});");
        assert_eq!(json["docs"], "# Auth");
        assert_eq!(json["auth"]["authType"], "bearer");
        assert_eq!(json["headers"][1]["enabled"], false);
        assert_eq!(json["variables"][0]["initialValue"], "");
        assert!(json.get("pre_request_script").is_none());
    }

    #[test]
    fn folder_settings_dto_default_omits_optional_keys() {
        let json = serde_json::to_value(FolderSettingsDto::default()).expect("serialize dto");
        assert_eq!(json, serde_json::json!({ "headers": [], "variables": [] }));
    }

    #[test]
    fn folder_settings_dto_accepts_a_partial_payload() {
        let dto: FolderSettingsDto =
            serde_json::from_str(r#"{"docs":"hi","auth":{"authType":"inherit"}}"#)
                .expect("partial payload");
        let settings = FolderSettings::from(dto);
        assert_eq!(settings.docs.as_deref(), Some("hi"));
        assert_eq!(settings.auth, Some(Auth::Inherit));
        assert!(settings.headers.is_empty());
        assert!(settings.variables.is_empty());
        assert!(settings.tests_script.is_none());
    }
}
