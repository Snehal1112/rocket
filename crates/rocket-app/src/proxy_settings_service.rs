use std::sync::Arc;

use rocket_environment::SecretStore;
use rocket_http::{ProxySettings, ProxySettingsRepository, SharedProxy};
use rocket_shared::error::DomainResult;
use zeroize::Zeroizing;

/// Keychain location of the proxy password.
const SECRET_SCOPE: &str = "proxy";
const SECRET_KEY: &str = "password";

/// What a save does with the stored password.
#[derive(Clone, PartialEq, Eq)]
pub enum PasswordChange {
    Keep,
    Clear,
    Set(String),
}

/// Never prints a password that is being set.
impl std::fmt::Debug for PasswordChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Keep => f.write_str("Keep"),
            Self::Clear => f.write_str("Clear"),
            Self::Set(_) => f.write_str("Set(<redacted>)"),
        }
    }
}

/// The setting as shown to the user: never the password, only whether one is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxySettingsView {
    pub settings: ProxySettings,
    pub has_password: bool,
}

/// Owns changing the app-level proxy: validates, stores the settings and the password
/// separately, and publishes the result to the handle the executor reads.
pub struct ProxySettingsService {
    repo: Box<dyn ProxySettingsRepository>,
    secrets: Arc<dyn SecretStore>,
    shared: SharedProxy,
}

impl ProxySettingsService {
    /// Loads what was saved before and publishes it. A file or keychain that cannot be read
    /// must not stop the app from starting, so it falls back to the system setting.
    pub fn new(
        repo: Box<dyn ProxySettingsRepository>,
        secrets: Arc<dyn SecretStore>,
        shared: SharedProxy,
    ) -> Self {
        let svc = Self {
            repo,
            secrets,
            shared,
        };
        let settings = svc.repo.load().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "could not read the proxy settings, using the system proxy");
            ProxySettings::default()
        });
        let password = svc.stored_password();
        svc.publish(settings, password);
        svc
    }

    pub fn get(&self) -> DomainResult<ProxySettingsView> {
        Ok(ProxySettingsView {
            settings: self.repo.load()?,
            has_password: self.stored_password().is_some(),
        })
    }

    pub fn save(&self, settings: ProxySettings, password: PasswordChange) -> DomainResult<()> {
        settings.validate()?;
        match &password {
            PasswordChange::Keep => {}
            PasswordChange::Clear => self.secrets.delete(SECRET_SCOPE, SECRET_KEY)?,
            PasswordChange::Set(value) if value.is_empty() => {
                self.secrets.delete(SECRET_SCOPE, SECRET_KEY)?
            }
            PasswordChange::Set(value) => self.secrets.set(SECRET_SCOPE, SECRET_KEY, value)?,
        }
        self.repo.save(&settings)?;
        let password = self.stored_password();
        self.publish(settings, password);
        Ok(())
    }

    fn stored_password(&self) -> Option<Zeroizing<String>> {
        self.secrets
            .get(SECRET_SCOPE, SECRET_KEY)
            .ok()
            .flatten()
            .filter(|p| !p.is_empty())
            .map(Zeroizing::new)
    }

    fn publish(&self, settings: ProxySettings, password: Option<Zeroizing<String>>) {
        // A poisoned lock only means a writer panicked; the value is still a whole proxy.
        let mut shared = self.shared.write().unwrap_or_else(|e| e.into_inner());
        shared.settings = settings;
        shared.password = password;
        shared.generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::SecretStore;
    use rocket_http::{new_shared_proxy, ProxyMode};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemSecrets(Mutex<HashMap<(String, String), String>>);

    impl SecretStore for MemSecrets {
        fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .0
                .lock()
                .expect("lock")
                .get(&(scope.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock")
                .insert((scope.to_string(), key.to_string()), value.to_string());
            Ok(())
        }
        fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock")
                .remove(&(scope.to_string(), key.to_string()));
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemRepo(Mutex<ProxySettings>);

    impl ProxySettingsRepository for MemRepo {
        fn load(&self) -> DomainResult<ProxySettings> {
            Ok(self.0.lock().expect("lock").clone())
        }
        fn save(&self, settings: &ProxySettings) -> DomainResult<()> {
            *self.0.lock().expect("lock") = settings.clone();
            Ok(())
        }
    }

    fn custom() -> ProxySettings {
        ProxySettings {
            mode: ProxyMode::Custom,
            http_proxy: Some("http://p:8080".into()),
            username: Some("u".into()),
            ..Default::default()
        }
    }

    fn service() -> (
        ProxySettingsService,
        rocket_http::SharedProxy,
        Arc<MemSecrets>,
    ) {
        let shared = new_shared_proxy();
        let secrets = Arc::new(MemSecrets::default());
        let svc = ProxySettingsService::new(
            Box::new(MemRepo::default()),
            Arc::clone(&secrets) as Arc<dyn SecretStore>,
            Arc::clone(&shared),
        );
        (svc, shared, secrets)
    }

    #[test]
    fn save_updates_the_shared_handle_and_bumps_the_generation() {
        let (svc, shared, _) = service();
        let before = shared.read().expect("lock").generation;
        svc.save(custom(), PasswordChange::Set("pw".into()))
            .expect("save");
        let now = shared.read().expect("lock");
        assert_eq!(now.settings.mode, ProxyMode::Custom);
        assert_eq!(now.password.as_ref().map(|p| p.as_str()), Some("pw"));
        assert!(now.generation > before);
    }

    #[test]
    fn the_password_goes_to_the_secret_store_only() {
        let (svc, _, secrets) = service();
        svc.save(custom(), PasswordChange::Set("pw".into()))
            .expect("save");
        assert_eq!(
            secrets.get("proxy", "password").expect("get").as_deref(),
            Some("pw")
        );
        let view = svc.get().expect("get");
        assert!(view.has_password);
        assert_eq!(view.settings, custom());
    }

    #[test]
    fn keep_leaves_the_password_and_clear_removes_it() {
        let (svc, shared, secrets) = service();
        svc.save(custom(), PasswordChange::Set("pw".into()))
            .expect("save");
        svc.save(custom(), PasswordChange::Keep).expect("keep");
        assert_eq!(
            secrets.get("proxy", "password").expect("get").as_deref(),
            Some("pw")
        );
        assert!(shared.read().expect("lock").password.is_some());
        svc.save(custom(), PasswordChange::Clear).expect("clear");
        assert_eq!(secrets.get("proxy", "password").expect("get"), None);
        assert!(shared.read().expect("lock").password.is_none());
        assert!(!svc.get().expect("get").has_password);
    }

    #[test]
    fn invalid_settings_change_nothing() {
        let (svc, shared, secrets) = service();
        let bad = ProxySettings {
            mode: ProxyMode::Custom,
            ..Default::default()
        };
        assert!(svc.save(bad, PasswordChange::Set("pw".into())).is_err());
        assert_eq!(secrets.get("proxy", "password").expect("get"), None);
        assert_eq!(
            shared.read().expect("lock").settings.mode,
            ProxyMode::System
        );
    }

    #[test]
    fn debug_never_prints_a_password_being_set() {
        let shown = format!("{:?}", PasswordChange::Set("hunter2".into()));
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }

    #[test]
    fn startup_publishes_what_was_saved_before() {
        let shared = new_shared_proxy();
        let secrets = Arc::new(MemSecrets::default());
        secrets.set("proxy", "password", "pw").expect("seed");
        let repo = MemRepo::default();
        repo.save(&custom()).expect("seed");
        let _svc = ProxySettingsService::new(
            Box::new(repo),
            secrets as Arc<dyn SecretStore>,
            Arc::clone(&shared),
        );
        let now = shared.read().expect("lock");
        assert_eq!(now.settings, custom());
        assert_eq!(now.password.as_ref().map(|p| p.as_str()), Some("pw"));
    }
}
