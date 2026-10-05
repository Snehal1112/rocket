//! The app-level proxy setting. Pure data and validation; the executor in `rocket-infra`
//! applies it and `rocket-app` owns the use case of changing it.

use std::fmt;
use std::sync::{Arc, RwLock};

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Where requests go through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    /// reqwest's default: the `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` environment variables.
    #[default]
    System,
    /// Always connect directly.
    None,
    /// Use the URLs below.
    Custom,
}

/// The persisted proxy setting (`proxy.yml`). It never holds the password: that lives in the OS
/// keychain, so this file is safe to read, back up and share.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProxySettings {
    #[serde(default)]
    pub mode: ProxyMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub https_proxy: Option<String>,
    /// Comma-separated hosts that bypass the proxy, in the `NO_PROXY` format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

impl ProxySettings {
    /// Checks the custom URLs. Errors never repeat the URL, because it is user input.
    pub fn validate(&self) -> DomainResult<()> {
        if self.mode != ProxyMode::Custom {
            return Ok(());
        }
        let urls = [
            ("HTTP", self.http_proxy.as_deref()),
            ("HTTPS", self.https_proxy.as_deref()),
        ];
        let mut any = false;
        for (label, url) in urls {
            let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) else {
                continue;
            };
            any = true;
            let parsed = reqwest::Url::parse(url).map_err(|_| {
                DomainError::InvalidInput(format!(
                    "The {label} proxy URL is not valid. Use http://host:port or https://host:port"
                ))
            })?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return Err(DomainError::InvalidInput(format!(
                    "The {label} proxy URL must start with http:// or https://"
                )));
            }
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err(DomainError::InvalidInput(format!(
                    "The {label} proxy URL must not contain a username or password. \
                     Use the username and password fields instead"
                )));
            }
        }
        if any {
            Ok(())
        } else {
            Err(DomainError::InvalidInput(
                "A custom proxy needs an HTTP or an HTTPS proxy URL".into(),
            ))
        }
    }
}

/// The setting as the executor uses it: the persisted values plus the password from the
/// keychain. Never serialized.
#[derive(Clone, Default)]
pub struct ResolvedProxy {
    pub settings: ProxySettings,
    pub password: Option<Zeroizing<String>>,
    /// Bumped on every change, so clients built for an older setting are never reused.
    pub generation: u64,
}

impl fmt::Debug for ResolvedProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedProxy")
            .field("settings", &self.settings)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("generation", &self.generation)
            .finish()
    }
}

/// The handle the executor reads on every request and the proxy service updates.
pub type SharedProxy = Arc<RwLock<ResolvedProxy>>;

pub fn new_shared_proxy() -> SharedProxy {
    Arc::new(RwLock::new(ResolvedProxy::default()))
}

/// Persistence of the non-secret part of the setting.
pub trait ProxySettingsRepository: Send + Sync {
    /// A missing file is the default setting (`System`), not an error.
    fn load(&self) -> DomainResult<ProxySettings>;
    fn save(&self, settings: &ProxySettings) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(http: Option<&str>, https: Option<&str>) -> ProxySettings {
        ProxySettings {
            mode: ProxyMode::Custom,
            http_proxy: http.map(str::to_string),
            https_proxy: https.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn system_and_none_always_validate() {
        assert!(ProxySettings::default().validate().is_ok());
        assert!(ProxySettings {
            mode: ProxyMode::None,
            ..Default::default()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn custom_needs_at_least_one_url() {
        assert!(custom(None, None).validate().is_err());
        assert!(custom(Some("  "), None).validate().is_err());
        assert!(custom(Some("http://proxy.corp:8080"), None)
            .validate()
            .is_ok());
        assert!(custom(None, Some("https://proxy.corp:8443"))
            .validate()
            .is_ok());
    }

    #[test]
    fn rejects_credentials_in_the_url_without_echoing_them() {
        let err = custom(Some("http://user:hunter2@proxy.corp:8080"), None)
            .validate()
            .expect_err("credentials in the URL");
        let text = err.to_string();
        assert!(text.contains("username"), "{text}");
        assert!(
            !text.contains("hunter2") && !text.contains("proxy.corp"),
            "{text}"
        );
    }

    #[test]
    fn rejects_unsupported_schemes_and_junk() {
        for bad in [
            "socks5://proxy.corp:1080",
            "proxy.corp:8080",
            "not a url",
            "ftp://p:1",
        ] {
            assert!(custom(Some(bad), None).validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn debug_never_prints_the_password() {
        let resolved = ResolvedProxy {
            settings: ProxySettings::default(),
            password: Some(zeroize::Zeroizing::new("hunter2".to_string())),
            generation: 3,
        };
        let shown = format!("{resolved:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }

    #[test]
    fn persistence_format_is_snake_case_and_skips_empty_values() {
        let yaml = serde_yaml::to_string(&custom(Some("http://p:1"), None)).expect("serialize");
        assert!(yaml.contains("mode: custom"), "{yaml}");
        assert!(yaml.contains("http_proxy: http://p:1"), "{yaml}");
        assert!(!yaml.contains("https_proxy"), "{yaml}");
        assert!(!yaml.contains("password"), "{yaml}");
        let back: ProxySettings = serde_yaml::from_str("mode: none\n").expect("minimal file");
        assert_eq!(back.mode, ProxyMode::None);
    }
}
