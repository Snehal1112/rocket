use std::fs;
use std::path::PathBuf;

use rocket_http::{ProxySettings, ProxySettingsRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

/// Stores the non-secret proxy setting in one YAML file.
pub struct FsProxySettingsRepo {
    path: PathBuf,
}

impl FsProxySettingsRepo {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ProxySettingsRepository for FsProxySettingsRepo {
    fn load(&self) -> DomainResult<ProxySettings> {
        if !self.path.exists() {
            return Ok(ProxySettings::default());
        }
        let text = fs::read_to_string(&self.path)?;
        serde_yaml::from_str(&text)
            .map_err(|e| DomainError::Internal(format!("Failed to parse the proxy settings: {e}")))
    }

    fn save(&self, settings: &ProxySettings) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(settings).map_err(|e| {
            DomainError::Internal(format!("Failed to serialize the proxy settings: {e}"))
        })?;
        atomic_write(&self.path, yaml.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::{ProxyMode, ProxySettings};

    #[test]
    fn a_missing_file_is_the_default_setting() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = FsProxySettingsRepo::new(dir.path().join("proxy.yml"));
        assert_eq!(repo.load().expect("load"), ProxySettings::default());
    }

    #[test]
    fn settings_round_trip_and_never_contain_a_password() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("proxy.yml");
        let repo = FsProxySettingsRepo::new(path.clone());
        let settings = ProxySettings {
            mode: ProxyMode::Custom,
            http_proxy: Some("http://p:8080".into()),
            https_proxy: None,
            no_proxy: Some("localhost".into()),
            username: Some("u".into()),
        };
        repo.save(&settings).expect("save");
        assert_eq!(repo.load().expect("load"), settings);
        let text = std::fs::read_to_string(path).expect("read");
        assert!(!text.to_ascii_lowercase().contains("password"), "{text}");
    }

    #[test]
    fn a_corrupt_file_is_an_error_not_a_silent_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("proxy.yml");
        std::fs::write(&path, "mode: [oops").expect("write");
        assert!(FsProxySettingsRepo::new(path).load().is_err());
    }
}
