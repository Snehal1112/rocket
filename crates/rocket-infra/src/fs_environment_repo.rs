use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rocket_environment::secret_store::{NullSecretStore, SecretStore};
use rocket_environment::{Environment, EnvironmentRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::oc::{OcEnvVariableEntry, OcEnvironment};
use crate::yaml_io::delete_if_exists;

pub struct FsEnvironmentRepo {
    dir: PathBuf,
    secret_store: Arc<dyn SecretStore>,
}

impl FsEnvironmentRepo {
    /// Environments with no secure backend — secret values are dropped on save
    /// and come back empty on load. Used by tests and by the Bruno importer.
    pub fn new(dir: PathBuf) -> Self {
        Self::with_secret_store(dir, Arc::new(NullSecretStore))
    }

    /// Environments backed by a real secret store. Production callers in
    /// `src-tauri` use this with `KeyringSecretStore`.
    pub fn with_secret_store(dir: PathBuf, secret_store: Arc<dyn SecretStore>) -> Self {
        Self { dir, secret_store }
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.yml", name))
    }

    /// Stable keychain namespace for one environment file.
    ///
    /// Derived from the canonical environments directory, so the workspace-level
    /// `<workspace>/environments/` and a collection's
    /// `<collection>/environments/` never share an entry even when both hold an
    /// environment called "prod".
    ///
    /// SHA-256 rather than `DefaultHasher`: `DefaultHasher`'s output is not
    /// stable across Rust releases, and this value is the lookup key for every
    /// stored secret — an unstable hash would orphan them on a toolchain bump.
    fn scope_id(dir: &Path, env_name: &str) -> String {
        use sha2::{Digest, Sha256};
        use std::fmt::Write;

        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
        let prefix = digest[..8]
            .iter()
            .fold(String::with_capacity(16), |mut acc, b| {
                let _ = write!(acc, "{b:02x}");
                acc
            });
        format!("{prefix}:{env_name}")
    }

    /// Keys stored as SecretVariable entries in the file as it exists on disk
    /// right now. Empty when the file is missing or unparseable.
    fn persisted_secret_keys(&self, name: &str) -> Vec<String> {
        let Ok(content) = fs::read_to_string(self.file_path(name)) else {
            return Vec::new();
        };
        let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) else {
            return Vec::new();
        };
        oc.variables
            .into_iter()
            .filter_map(|entry| match entry {
                OcEnvVariableEntry::Secret(s) => Some(s.name),
                OcEnvVariableEntry::Plain(_) => None,
            })
            .collect()
    }

    /// Fill in real values for secret variables from the secret store.
    ///
    /// A store failure must never fail an environment load — that would brick
    /// app startup on a locked keychain — so it is logged and the variable keeps
    /// whatever value the YAML produced (empty for a spec SecretVariable entry).
    fn hydrate_secrets(&self, env: &mut Environment) {
        if !env.variables.iter().any(|v| v.secret) {
            return;
        }
        let scope = Self::scope_id(&self.dir, &env.name);
        for var in env.variables.iter_mut().filter(|v| v.secret) {
            match self.secret_store.get(&scope, &var.key) {
                Ok(Some(value)) => var.value = value,
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        key = %var.key,
                        error = %e,
                        "secret store unavailable, environment secret left unresolved"
                    );
                }
            }
        }
    }
}

impl EnvironmentRepository for FsEnvironmentRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        let mut result = Vec::new();
        if !self.dir.exists() {
            return Ok(result);
        }
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.extension().is_some_and(|e| e == "yml") {
                continue;
            }
            let content = fs::read_to_string(&path)?;
            let parsed = if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
                Some(Environment::from(oc))
            } else if let Ok(env) = serde_yaml::from_str::<Environment>(&content) {
                Some(env)
            } else {
                tracing::warn!(path = %path.display(), "skipping corrupt environment YAML file");
                None
            };
            if let Some(mut env) = parsed {
                self.hydrate_secrets(&mut env);
                result.push(env);
            }
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    fn get(&self, name: &str) -> DomainResult<Environment> {
        let path = self.file_path(name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("Environment '{}'", name)));
        }
        let content = fs::read_to_string(&path)?;
        let mut env = if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
            Environment::from(oc)
        } else {
            serde_yaml::from_str::<Environment>(&content).map_err(|e| {
                DomainError::Internal(format!("Failed to parse environment YAML: {e}"))
            })?
        };
        self.hydrate_secrets(&mut env);
        Ok(env)
    }

    fn save(&self, env: &Environment) -> DomainResult<()> {
        // Create the directory up front so scope_id() canonicalizes the same
        // path on a first save as on every later read.
        fs::create_dir_all(&self.dir)?;
        let scope = Self::scope_id(&self.dir, &env.name);

        // Snapshot which keys were secret before this save, so entries left
        // behind by an un-secreted or removed variable can be cleaned up.
        let previous_secret_keys = self.persisted_secret_keys(&env.name);

        // Every secret value goes to the store before any YAML is written. A
        // store failure aborts the whole save: the file must never claim a
        // variable is secret when its value did not reach secure storage.
        for var in env.variables.iter().filter(|v| v.secret) {
            self.secret_store.set(&scope, &var.key, &var.value)?;
        }

        // The conversion drops secret values by construction — see
        // conversions/environment.rs.
        let oc: OcEnvironment = env.clone().into();
        let yaml = serde_yaml::to_string(&oc)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize environment: {e}")))?;
        atomic_write(&self.file_path(&env.name), yaml.as_bytes())?;

        // Best-effort cleanup. A stale entry leaks nothing new, so a failure
        // here must not fail the save the user just asked for.
        for key in previous_secret_keys {
            if env.variables.iter().any(|v| v.secret && v.key == key) {
                continue;
            }
            if let Err(e) = self.secret_store.delete(&scope, &key) {
                tracing::warn!(key = %key, error = %e, "failed to remove stale environment secret");
            }
        }

        Ok(())
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        // Read the secret key list while the file still exists.
        let scope = Self::scope_id(&self.dir, name);
        let secret_keys = self.persisted_secret_keys(name);

        delete_if_exists(&self.file_path(name), &format!("Environment '{}'", name))?;

        // Best-effort: the environment is already gone, so a store failure here
        // must not surface as a failed delete.
        for key in secret_keys {
            if let Err(e) = self.secret_store.delete(&scope, &key) {
                tracing::warn!(key = %key, error = %e, "failed to remove secret for deleted environment");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::secret_store::SecretStore;
    use rocket_environment::Variable;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// In-memory SecretStore double. Tests must never touch a real OS keychain:
    /// CI has no Secret Service / Keychain daemon, so such a test would fail for
    /// environmental reasons unrelated to the code under test.
    #[derive(Default)]
    struct InMemorySecretStore {
        entries: Mutex<HashMap<String, String>>,
        fail_set: AtomicBool,
        fail_get: AtomicBool,
    }

    impl InMemorySecretStore {
        fn entry_key(scope_id: &str, key: &str) -> String {
            format!("{scope_id}:{key}")
        }

        fn len(&self) -> usize {
            self.entries.lock().expect("store lock").len()
        }

        fn contains_value(&self, value: &str) -> bool {
            self.entries
                .lock()
                .expect("store lock")
                .values()
                .any(|v| v == value)
        }
    }

    impl SecretStore for InMemorySecretStore {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            if self.fail_get.load(Ordering::SeqCst) {
                return Err(DomainError::Internal("keychain locked".into()));
            }
            Ok(self
                .entries
                .lock()
                .expect("store lock")
                .get(&Self::entry_key(scope_id, key))
                .cloned())
        }

        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            if self.fail_set.load(Ordering::SeqCst) {
                return Err(DomainError::Internal("keychain locked".into()));
            }
            self.entries
                .lock()
                .expect("store lock")
                .insert(Self::entry_key(scope_id, key), value.to_string());
            Ok(())
        }

        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.entries
                .lock()
                .expect("store lock")
                .remove(&Self::entry_key(scope_id, key));
            Ok(())
        }
    }

    fn setup() -> (TempDir, FsEnvironmentRepo) {
        let dir = TempDir::new().expect("temp dir");
        let repo = FsEnvironmentRepo::new(dir.path().to_path_buf());
        (dir, repo)
    }

    fn setup_with_store() -> (TempDir, FsEnvironmentRepo, Arc<InMemorySecretStore>) {
        let dir = TempDir::new().expect("temp dir");
        let store = Arc::new(InMemorySecretStore::default());
        let repo = FsEnvironmentRepo::with_secret_store(dir.path().to_path_buf(), store.clone());
        (dir, repo, store)
    }

    #[test]
    fn list_empty() {
        let (_dir, repo) = setup();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn save_and_list() {
        let (_dir, repo) = setup();
        let mut env = Environment::new("production");
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "production");
    }

    #[test]
    fn save_and_get() {
        let (_dir, repo) = setup();
        let mut env = Environment::new("staging");
        env.set_variable(Variable::new("BASE_URL", "https://staging.example.com"));
        repo.save(&env).unwrap();
        let loaded = repo.get("staging").unwrap();
        assert_eq!(
            loaded.get_value("BASE_URL"),
            Some("https://staging.example.com")
        );
    }

    #[test]
    fn update_existing() {
        let (_dir, repo) = setup();
        let mut env = Environment::new("test");
        env.set_variable(Variable::new("KEY", "v1"));
        repo.save(&env).unwrap();
        env.set_variable(Variable::new("KEY", "v2"));
        repo.save(&env).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].get_value("KEY"), Some("v2"));
    }

    #[test]
    fn delete_environment() {
        let (_dir, repo) = setup();
        repo.save(&Environment::new("temp")).unwrap();
        repo.delete("temp").unwrap();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn save_writes_spec_field_names() {
        let (dir, repo) = setup();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("BASE_URL", "https://api.example.com"));
        let mut disabled_var = Variable::new("DISABLED_VAR", "x");
        disabled_var.enabled = false;
        env.set_variable(disabled_var);
        repo.save(&env).unwrap();

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).unwrap();
        assert!(
            raw.contains("name: BASE_URL"),
            "expected 'name:' field, got:\n{raw}"
        );
        assert!(
            !raw.contains("key:"),
            "should not contain 'key:' field:\n{raw}"
        );
        assert!(
            raw.contains("disabled: true"),
            "expected 'disabled: true':\n{raw}"
        );
        assert!(
            !raw.contains("enabled:"),
            "should not contain 'enabled:' field:\n{raw}"
        );
    }

    #[test]
    fn save_then_load_roundtrip_via_oc_format() {
        let (_dir, repo) = setup();
        let mut env = Environment::new("staging");
        env.set_variable(Variable::new("HOST", "staging.example.com"));
        repo.save(&env).unwrap();
        let loaded = repo.get("staging").unwrap();
        assert_eq!(loaded.get_value("HOST"), Some("staging.example.com"));
    }

    #[test]
    fn load_old_format_with_key_field_still_works() {
        let (dir, repo) = setup();
        let old_yaml =
            "name: legacy\nvariables:\n- key: OLD_VAR\n  value: hello\n  enabled: true\n";
        std::fs::write(dir.path().join("legacy.yml"), old_yaml).unwrap();
        let env = repo.get("legacy").unwrap();
        assert_eq!(env.get_value("OLD_VAR"), Some("hello"));
    }

    #[test]
    fn list_old_format_with_key_field_still_works() {
        let (dir, repo) = setup();
        let old_yaml =
            "name: legacy\nvariables:\n- key: OLD_VAR\n  value: hello\n  enabled: true\n";
        std::fs::write(dir.path().join("legacy.yml"), old_yaml).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].get_value("OLD_VAR"), Some("hello"));
    }

    #[test]
    fn save_keeps_the_secret_value_out_of_the_yaml_file() {
        let (dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(
            !raw.contains("sk-live-123"),
            "secret value leaked to disk:\n{raw}"
        );
        assert!(
            store.contains_value("sk-live-123"),
            "secret value never reached the store"
        );
    }

    #[test]
    fn save_writes_a_spec_secret_variable_entry() {
        let (dir, repo, _store) = setup_with_store();
        let mut env = Environment::new("prod");
        let mut secret = Variable::secret("API_KEY", "sk-live-123");
        secret.secret_type = Some("string".into());
        env.set_variable(secret);
        repo.save(&env).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(
            raw.contains("secret: true"),
            "expected 'secret: true':\n{raw}"
        );
        assert!(
            raw.contains("name: API_KEY"),
            "expected 'name: API_KEY':\n{raw}"
        );
        assert!(
            raw.contains("type: string"),
            "expected the secret type hint:\n{raw}"
        );
        assert!(
            !raw.contains("value:"),
            "a secret entry must carry no value field:\n{raw}"
        );
    }

    #[test]
    fn save_aborts_when_the_secret_store_rejects_the_value() {
        let (dir, repo, store) = setup_with_store();
        store.fail_set.store(true, Ordering::SeqCst);

        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        let err = repo
            .save(&env)
            .expect_err("save must fail when the store rejects the value");

        assert!(matches!(err, DomainError::Internal(_)), "got {err:?}");
        assert!(
            !dir.path().join("prod.yml").exists(),
            "YAML must not claim a secret is protected when the store rejected it"
        );
    }

    #[test]
    fn secret_survives_a_first_save_into_a_missing_directory() {
        let parent = TempDir::new().expect("temp dir");
        let env_dir = parent.path().join("environments");
        assert!(!env_dir.exists());
        let store = Arc::new(InMemorySecretStore::default());
        let repo = FsEnvironmentRepo::with_secret_store(env_dir, store.clone());

        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("save");

        assert_eq!(store.len(), 1);
        assert!(store.contains_value("sk-live-123"));
    }

    #[test]
    fn non_secret_variables_still_write_their_value_to_yaml() {
        let (dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(raw.contains("name: HOST"), "got:\n{raw}");
        assert!(raw.contains("value: api.example.com"), "got:\n{raw}");
        assert_eq!(
            store.len(),
            0,
            "a non-secret variable must not touch the secret store"
        );
    }

    #[test]
    fn secret_value_roundtrips_through_save_and_get() {
        let (_dir, repo, _store) = setup_with_store();
        let mut env = Environment::new("prod");
        let mut secret = Variable::secret("API_KEY", "sk-live-123");
        secret.secret_type = Some("string".into());
        env.set_variable(secret);
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");

        let loaded = repo.get("prod").expect("get");
        let api_key = loaded
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret, "the secret flag must survive a roundtrip");
        assert_eq!(api_key.value, "sk-live-123");
        assert_eq!(api_key.secret_type, Some("string".into()));
        assert_eq!(loaded.get_value("HOST"), Some("api.example.com"));
    }

    #[test]
    fn secret_value_roundtrips_through_save_and_list() {
        let (_dir, repo, _store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("save");

        let list = repo.list().expect("list");
        assert_eq!(list.len(), 1);
        let api_key = list[0]
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret);
        assert_eq!(api_key.value, "sk-live-123");
    }

    #[test]
    fn get_soft_fails_when_the_secret_store_is_unavailable() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");

        store.fail_get.store(true, Ordering::SeqCst);
        // A locked keychain must never fail an environment load.
        let loaded = repo
            .get("prod")
            .expect("get must not fail on a store error");
        let api_key = loaded
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret);
        assert_eq!(api_key.value, "");
        assert_eq!(loaded.get_value("HOST"), Some("api.example.com"));
    }

    #[test]
    fn get_returns_an_empty_secret_when_the_store_has_no_entry() {
        let (dir, repo, _store) = setup_with_store();
        let yaml = "name: prod\nvariables:\n- secret: true\n  name: API_KEY\n";
        std::fs::write(dir.path().join("prod.yml"), yaml).expect("write prod.yml");

        let loaded = repo.get("prod").expect("get");
        let api_key = loaded
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret);
        assert_eq!(api_key.value, "");
    }

    #[test]
    fn legacy_plaintext_secret_value_is_not_blanked_on_load() {
        let (dir, repo, _store) = setup_with_store();
        // Legacy `Environment`-format file: `key:` instead of `name:`, so the
        // OcEnvironment parse fails and the fallback parser handles it.
        let legacy = "name: legacy\nvariables:\n- key: API_KEY\n  value: plaintext-token\n  enabled: true\n  secret: true\n";
        std::fs::write(dir.path().join("legacy.yml"), legacy).expect("write legacy.yml");

        let loaded = repo.get("legacy").expect("get");
        let api_key = loaded
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret);
        assert_eq!(
            api_key.value, "plaintext-token",
            "a store miss must not destroy existing data"
        );
    }

    #[test]
    fn two_directories_do_not_share_secret_entries() {
        let store = Arc::new(InMemorySecretStore::default());
        let dir_a = TempDir::new().expect("temp dir a");
        let dir_b = TempDir::new().expect("temp dir b");
        let repo_a =
            FsEnvironmentRepo::with_secret_store(dir_a.path().to_path_buf(), store.clone());
        let repo_b =
            FsEnvironmentRepo::with_secret_store(dir_b.path().to_path_buf(), store.clone());

        let mut env_a = Environment::new("prod");
        env_a.set_variable(Variable::secret("API_KEY", "value-a"));
        repo_a.save(&env_a).expect("save a");

        let mut env_b = Environment::new("prod");
        env_b.set_variable(Variable::secret("API_KEY", "value-b"));
        repo_b.save(&env_b).expect("save b");

        assert_eq!(
            store.len(),
            2,
            "same env name in different directories must not collide"
        );
        assert_eq!(
            repo_a.get("prod").expect("get a").get_value("API_KEY"),
            Some("value-a")
        );
        assert_eq!(
            repo_b.get("prod").expect("get b").get_value("API_KEY"),
            Some("value-b")
        );
    }

    #[test]
    fn unsetting_the_secret_flag_removes_the_stored_secret() {
        let (dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("first save");
        assert_eq!(store.len(), 1);

        env.set_variable(Variable::new("API_KEY", "not-a-secret-anymore"));
        repo.save(&env).expect("second save");

        assert_eq!(store.len(), 0, "a stale keychain entry must be removed");
        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(raw.contains("value: not-a-secret-anymore"), "got:\n{raw}");
        assert!(!raw.contains("secret: true"), "got:\n{raw}");
    }

    #[test]
    fn removing_a_secret_variable_removes_the_stored_secret() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        env.set_variable(Variable::secret("TOKEN", "tok-456"));
        repo.save(&env).expect("first save");
        assert_eq!(store.len(), 2);

        env.remove_variable("API_KEY");
        repo.save(&env).expect("second save");

        assert_eq!(store.len(), 1);
        assert!(
            store.contains_value("tok-456"),
            "the surviving secret must be untouched"
        );
        assert!(!store.contains_value("sk-live-123"));
    }

    #[test]
    fn resaving_an_unchanged_secret_keeps_it() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("first save");
        repo.save(&env).expect("second save");

        assert_eq!(store.len(), 1);
        assert_eq!(
            repo.get("prod").expect("get").get_value("API_KEY"),
            Some("sk-live-123")
        );
    }

    #[test]
    fn deleting_an_environment_removes_its_secrets() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");
        assert_eq!(store.len(), 1);

        repo.delete("prod").expect("delete");

        assert_eq!(
            store.len(),
            0,
            "a deleted environment must not leave secrets behind"
        );
        assert!(repo.list().expect("list").is_empty());
    }

    #[test]
    fn deleting_an_environment_with_no_secrets_still_succeeds() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");

        repo.delete("prod").expect("delete");
        assert_eq!(store.len(), 0);
    }
}
