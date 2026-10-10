use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_history::{Template, TemplateRepository};
use rocket_shared::error::DomainResult;

use crate::FsTemplateRepo;

/// The request templates in `<active workspace>/templates`.
///
/// Every call builds a short-lived `FsTemplateRepo` from the shared active
/// workspace path, like `SharedPathCollectionRepo`, so a workspace switch takes
/// effect on the next call.
pub struct SharedPathTemplateRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl SharedPathTemplateRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    fn current(&self) -> FsTemplateRepo {
        let dir = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("templates");
        FsTemplateRepo::new(dir)
    }
}

impl TemplateRepository for SharedPathTemplateRepo {
    fn list(&self) -> DomainResult<Vec<Template>> {
        self.current().list()
    }

    fn get(&self, name: &str) -> DomainResult<Template> {
        self.current().get(name)
    }

    fn save(&self, template: &Template) -> DomainResult<()> {
        self.current().save(template)
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        self.current().delete(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::types::HttpMethod;

    fn template(name: &str, url: &str) -> Template {
        Template::new(name, HttpMethod::Get, url)
    }

    #[test]
    fn reads_and_writes_follow_the_active_workspace() {
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let repo = SharedPathTemplateRepo::new(Arc::clone(&path));

        repo.save(&template("t", "https://a.test"))
            .expect("save in a");
        assert!(a.path().join("templates/t.yml").exists());

        *path.lock().expect("lock") = b.path().to_path_buf();
        assert!(repo.list().expect("list in b").is_empty());
        assert!(repo.get("t").is_err(), "a's template must not show in b");
        repo.save(&template("t", "https://b.test"))
            .expect("save in b");
        assert!(b.path().join("templates/t.yml").exists());
        repo.delete("t").expect("delete in b");
        assert!(a.path().join("templates/t.yml").exists());
    }

    #[test]
    fn switching_back_and_forth_does_not_mix_data() {
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let repo = SharedPathTemplateRepo::new(Arc::clone(&path));
        repo.save(&template("t", "https://a.test"))
            .expect("save in a");
        *path.lock().expect("lock") = b.path().to_path_buf();
        repo.save(&template("t", "https://b.test"))
            .expect("save in b");
        *path.lock().expect("lock") = a.path().to_path_buf();
        assert_eq!(repo.get("t").expect("a").url, "https://a.test");
        *path.lock().expect("lock") = b.path().to_path_buf();
        assert_eq!(repo.get("t").expect("b").url, "https://b.test");
    }
}
