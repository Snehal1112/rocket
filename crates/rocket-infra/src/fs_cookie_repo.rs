use std::fs;
use std::io::Write;
use std::path::PathBuf;

use rocket_http::{CookieJar, CookieRepository};
use rocket_shared::error::DomainResult;

use crate::atomic_write;
use crate::yaml_io::read_dir_yaml;

pub struct FsCookieRepo {
    dir: PathBuf,
}

impl FsCookieRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Sanitize domain for use as a filename (replace dots and colons).
    fn file_path(&self, domain: &str) -> PathBuf {
        let sanitized = domain.replace(['.', ':'], "_");
        self.dir.join(format!("{}.yml", sanitized))
    }

    /// Keeps session cookies out of git when the workspace is a repository.
    /// Creates `.gitignore` only if it is missing and never fails the save.
    fn ensure_gitignore(&self) {
        let result = fs::create_dir_all(&self.dir).and_then(|()| {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.dir.join(".gitignore"))
            {
                Ok(mut file) => file.write_all(b"*"),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                Err(e) => Err(e),
            }
        });
        if let Err(e) = result {
            tracing::warn!("Could not write the cookie folder .gitignore: {e}");
        }
    }
}

impl CookieRepository for FsCookieRepo {
    fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
        Ok(read_dir_yaml::<CookieJar>(&self.dir)?
            .into_iter()
            .map(|(_, jar)| jar)
            .collect())
    }

    fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
        let path = self.file_path(domain);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let jar = serde_yaml::from_str(&content).map_err(|e| {
            rocket_shared::error::DomainError::Internal(format!("Failed to parse YAML: {e}"))
        })?;
        Ok(Some(jar))
    }

    fn save(&self, jar: &CookieJar) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(jar).map_err(|e| {
            rocket_shared::error::DomainError::Internal(format!("Failed to serialize YAML: {e}"))
        })?;
        self.ensure_gitignore();
        atomic_write(&self.file_path(&jar.domain), yaml.as_bytes())?;
        Ok(())
    }

    fn clear(&self) -> DomainResult<()> {
        for (path, _) in read_dir_yaml::<CookieJar>(&self.dir)? {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::Cookie;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsCookieRepo) {
        let dir = TempDir::new().unwrap();
        let repo = FsCookieRepo::new(dir.path().to_path_buf());
        (dir, repo)
    }

    fn sample_jar(domain: &str) -> CookieJar {
        let mut jar = CookieJar::new(domain);
        jar.add(Cookie {
            name: "session".into(),
            value: "abc123".into(),
            domain: domain.into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            expires: None,
        });
        jar
    }

    #[test]
    fn save_and_get_all() {
        let (_dir, repo) = setup();
        repo.save(&sample_jar("example.com")).unwrap();
        repo.save(&sample_jar("api.example.com")).unwrap();
        let all = repo.get_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn get_by_domain() {
        let (_dir, repo) = setup();
        repo.save(&sample_jar("example.com")).unwrap();
        let jar = repo.get_by_domain("example.com").unwrap();
        assert!(jar.is_some());
        assert_eq!(jar.unwrap().get("session").unwrap().value, "abc123");
    }

    #[test]
    fn clear_all() {
        let (_dir, repo) = setup();
        repo.save(&sample_jar("a.com")).unwrap();
        repo.save(&sample_jar("b.com")).unwrap();
        repo.clear().unwrap();
        assert!(repo.get_all().unwrap().is_empty());
    }

    fn cookies_setup() -> (TempDir, PathBuf, FsCookieRepo) {
        let dir = TempDir::new().expect("temp dir");
        let cookies = dir.path().join("cookies");
        let repo = FsCookieRepo::new(cookies.clone());
        (dir, cookies, repo)
    }

    #[test]
    fn save_writes_gitignore_with_star() {
        let (_dir, cookies, repo) = cookies_setup();
        repo.save(&sample_jar("example.com")).expect("save");
        let content = fs::read_to_string(cookies.join(".gitignore")).expect("gitignore");
        assert_eq!(content, "*");
    }

    #[test]
    fn second_save_keeps_gitignore_unchanged() {
        let (_dir, cookies, repo) = cookies_setup();
        repo.save(&sample_jar("a.com")).expect("save a");
        repo.save(&sample_jar("b.com")).expect("save b");
        let content = fs::read_to_string(cookies.join(".gitignore")).expect("gitignore");
        assert_eq!(content, "*");
    }

    #[test]
    fn save_leaves_user_edited_gitignore_untouched() {
        let (_dir, cookies, repo) = cookies_setup();
        fs::create_dir_all(&cookies).expect("mkdir");
        fs::write(cookies.join(".gitignore"), "custom\n").expect("write");
        repo.save(&sample_jar("example.com")).expect("save");
        let content = fs::read_to_string(cookies.join(".gitignore")).expect("gitignore");
        assert_eq!(content, "custom\n");
    }

    #[test]
    fn save_succeeds_when_gitignore_cannot_be_written() {
        let (_dir, cookies, repo) = cookies_setup();
        // A directory named .gitignore cannot be replaced by a file.
        fs::create_dir_all(cookies.join(".gitignore")).expect("mkdir");
        repo.save(&sample_jar("example.com")).expect("save");
        let jar = repo.get_by_domain("example.com").expect("get");
        assert!(jar.is_some());
    }

    #[test]
    fn get_all_returns_empty_when_dir_missing() {
        let dir = TempDir::new().unwrap();
        let repo = FsCookieRepo::new(dir.path().join("cookies"));
        assert!(repo.get_all().unwrap().is_empty());
    }
}
