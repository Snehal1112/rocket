use std::fs;
use std::path::PathBuf;

use rocket_environment::{Environment, EnvironmentRepository};
use rocket_shared::error::{DomainError, DomainResult};

pub struct FsEnvironmentRepo {
    dir: PathBuf,
}

impl FsEnvironmentRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.json", name))
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
            if path.extension().is_some_and(|e| e == "json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(env) = serde_json::from_str::<Environment>(&content) {
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
        Ok(serde_json::from_str(&content)?)
    }

    fn save(&self, env: &Environment) -> DomainResult<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(env)?;
        fs::write(self.file_path(&env.name), json)?;
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
}
