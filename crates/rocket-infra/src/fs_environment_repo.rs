use std::fs;
use std::path::PathBuf;

use rocket_environment::{Environment, EnvironmentRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::opencollection::OcEnvironment;

pub struct FsEnvironmentRepo {
    dir: PathBuf,
}

impl FsEnvironmentRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.yml", name))
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
            if path.extension().is_some_and(|e| e == "yml") {
                let content = fs::read_to_string(&path)?;
                if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
                    result.push(Environment::from(oc));
                } else if let Ok(env) = serde_yaml::from_str::<Environment>(&content) {
                    result.push(env);
                }
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
        if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
            return Ok(Environment::from(oc));
        }
        serde_yaml::from_str::<Environment>(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse environment YAML: {e}")))
    }

    fn save(&self, env: &Environment) -> DomainResult<()> {
        fs::create_dir_all(&self.dir)?;
        let oc: OcEnvironment = env.clone().into();
        let yaml = serde_yaml::to_string(&oc)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize environment: {e}")))?;
        fs::write(self.file_path(&env.name), yaml)?;
        Ok(())
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        let path = self.file_path(name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("Environment '{}'", name)));
        }
        fs::remove_file(&path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::Variable;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsEnvironmentRepo) {
        let dir = TempDir::new().unwrap();
        let repo = FsEnvironmentRepo::new(dir.path().to_path_buf());
        (dir, repo)
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
        assert_eq!(loaded.get_value("BASE_URL"), Some("https://staging.example.com"));
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
        assert!(raw.contains("name: BASE_URL"), "expected 'name:' field, got:\n{raw}");
        assert!(!raw.contains("key:"), "should not contain 'key:' field:\n{raw}");
        assert!(raw.contains("disabled: true"), "expected 'disabled: true':\n{raw}");
        assert!(!raw.contains("enabled:"), "should not contain 'enabled:' field:\n{raw}");
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
        let old_yaml = "name: legacy\nvariables:\n- key: OLD_VAR\n  value: hello\n  enabled: true\n";
        std::fs::write(dir.path().join("legacy.yml"), old_yaml).unwrap();
        let env = repo.get("legacy").unwrap();
        assert_eq!(env.get_value("OLD_VAR"), Some("hello"));
    }
}
