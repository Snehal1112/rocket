use std::fs;
use std::path::PathBuf;

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
        self.dir.join(format!("{}.json", name))
    }
}

impl TemplateRepository for FsTemplateRepo {
    fn list(&self) -> DomainResult<Vec<Template>> {
        let mut result = Vec::new();
        if !self.dir.exists() {
            return Ok(result);
        }
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(t) = serde_json::from_str::<Template>(&content) {
                    result.push(t);
                }
            }
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    fn get(&self, name: &str) -> DomainResult<Template> {
        let path = self.file_path(name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("Template '{}'", name)));
        }
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save(&self, template: &Template) -> DomainResult<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(template)?;
        fs::write(self.file_path(&template.name), json)?;
        Ok(())
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        let path = self.file_path(name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("Template '{}'", name)));
        }
        fs::remove_file(&path)?;
        Ok(())
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
}
