use rocket_app::{PasswordChange, ProxySettingsService, ProxySettingsView};
use rocket_http::{ProxyMode, ProxySettings};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

/// IPC shape of the proxy setting as shown to the user. The password is never part of it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettingsViewDto {
    pub mode: ProxyMode,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub no_proxy: Option<String>,
    pub username: Option<String>,
    pub has_password: bool,
}

impl From<ProxySettingsView> for ProxySettingsViewDto {
    fn from(view: ProxySettingsView) -> Self {
        Self {
            mode: view.settings.mode,
            http_proxy: view.settings.http_proxy,
            https_proxy: view.settings.https_proxy,
            no_proxy: view.settings.no_proxy,
            username: view.settings.username,
            has_password: view.has_password,
        }
    }
}

/// IPC shape of the setting being saved. Kept apart from `ProxySettings` so the camelCase
/// rename never reaches `proxy.yml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettingsInputDto {
    pub mode: ProxyMode,
    #[serde(default)]
    pub http_proxy: Option<String>,
    #[serde(default)]
    pub https_proxy: Option<String>,
    #[serde(default)]
    pub no_proxy: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

impl From<ProxySettingsInputDto> for ProxySettings {
    fn from(dto: ProxySettingsInputDto) -> Self {
        let clean = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Self {
            mode: dto.mode,
            http_proxy: clean(dto.http_proxy),
            https_proxy: clean(dto.https_proxy),
            no_proxy: clean(dto.no_proxy),
            username: clean(dto.username),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum PasswordChangeDto {
    Keep,
    Clear,
    Set { value: String },
}

/// Never prints a password that is being set.
impl std::fmt::Debug for PasswordChangeDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keep => f.write_str("Keep"),
            Self::Clear => f.write_str("Clear"),
            Self::Set { .. } => f.write_str("Set { value: <redacted> }"),
        }
    }
}

impl From<PasswordChangeDto> for PasswordChange {
    fn from(dto: PasswordChangeDto) -> Self {
        match dto {
            PasswordChangeDto::Keep => PasswordChange::Keep,
            PasswordChangeDto::Clear => PasswordChange::Clear,
            PasswordChangeDto::Set { value } => PasswordChange::Set(value),
        }
    }
}

#[tauri::command]
pub fn get_proxy_settings(
    svc: State<'_, ProxySettingsService>,
) -> Result<ProxySettingsViewDto, DomainError> {
    svc.get().map(ProxySettingsViewDto::from)
}

#[tauri::command]
pub fn save_proxy_settings(
    settings: ProxySettingsInputDto,
    password: PasswordChangeDto,
    svc: State<'_, ProxySettingsService>,
) -> Result<(), DomainError> {
    svc.save(settings.into(), password.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_never_carries_a_password_field() {
        let dto = ProxySettingsViewDto::from(ProxySettingsView {
            settings: ProxySettings::default(),
            has_password: true,
        });
        let json = serde_json::to_value(dto).expect("serialize");
        assert_eq!(json["hasPassword"], true);
        assert!(json.get("password").is_none());
    }

    #[test]
    fn input_trims_and_drops_empty_values() {
        let dto: ProxySettingsInputDto = serde_json::from_str(
            r#"{"mode":"custom","httpProxy":" http://p:1 ","httpsProxy":"  ","username":""}"#,
        )
        .expect("deserialize");
        let settings = ProxySettings::from(dto);
        assert_eq!(settings.http_proxy.as_deref(), Some("http://p:1"));
        assert_eq!(settings.https_proxy, None);
        assert_eq!(settings.username, None);
    }

    #[test]
    fn password_change_is_a_tagged_action() {
        let keep: PasswordChangeDto = serde_json::from_str(r#"{"action":"keep"}"#).expect("keep");
        assert!(matches!(PasswordChange::from(keep), PasswordChange::Keep));
        let set: PasswordChangeDto =
            serde_json::from_str(r#"{"action":"set","value":"pw"}"#).expect("set");
        assert_eq!(PasswordChange::from(set), PasswordChange::Set("pw".into()));
    }

    #[test]
    fn debug_never_prints_a_password_being_set() {
        let dto: PasswordChangeDto =
            serde_json::from_str(r#"{"action":"set","value":"hunter2"}"#).expect("set");
        let shown = format!("{dto:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}
