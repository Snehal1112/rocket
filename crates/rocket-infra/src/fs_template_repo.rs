use std::path::PathBuf;

use crate::atomic_write;
use crate::yaml_io::{delete_if_exists, read_dir_yaml};
use rocket_history::{Template, TemplateRepository};
use rocket_shared::error::{DomainError, DomainResult};

pub struct FsTemplateRepo {
    dir: PathBuf,
}

impl FsTemplateRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.yml", name))
    }
}

impl TemplateRepository for FsTemplateRepo {
    fn list(&self) -> DomainResult<Vec<Template>> {
        let mut items: Vec<Template> = read_dir_yaml::<Template>(&self.dir)?
            .into_iter()
            .map(|(_, t)| t)
            .collect();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(items)
    }

    fn get(&self, name: &str) -> DomainResult<Template> {
        let path = self.file_path(name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("Template '{}'", name)));
        }
        let content = std::fs::read_to_string(&path)?;
        serde_yaml::from_str(&content)
            .map_err(|e| DomainError::Internal(format!("Failed to parse YAML: {e}")))
    }

    fn save(&self, template: &Template) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(template)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize YAML: {e}")))?;
        atomic_write(&self.file_path(&template.name), yaml.as_bytes())?;
        Ok(())
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        delete_if_exists(&self.file_path(name), &format!("Template '{}'", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::HttpMethod;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsTemplateRepo) {
        let dir = TempDir::new().unwrap();
        let repo = FsTemplateRepo::new(dir.path().to_path_buf());
        (dir, repo)
    }

    #[test]
    fn save_and_list() {
        let (_dir, repo) = setup();
        let t = Template::new("JSON POST", HttpMethod::Post, "https://api.example.com");
        repo.save(&t).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "JSON POST");
    }

    #[test]
    fn get_by_name() {
        let (_dir, repo) = setup();
        let t = Template::new("GET Users", HttpMethod::Get, "https://api.example.com/users");
        repo.save(&t).unwrap();
        let loaded = repo.get("GET Users").unwrap();
        assert_eq!(loaded.url, "https://api.example.com/users");
    }

    #[test]
    fn delete_template() {
        let (_dir, repo) = setup();
        let t = Template::new("temp", HttpMethod::Delete, "/resource");
        repo.save(&t).unwrap();
        repo.delete("temp").unwrap();
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn delete_nonexistent_returns_not_found() {
        let (_dir, repo) = setup();
        let err = repo.delete("ghost").unwrap_err();
        assert!(matches!(err, DomainError::NotFound(_)));
    }
}
