use std::fs;
use std::path::PathBuf;

use rocket_http::{CookieJar, CookieRepository};
use rocket_shared::error::DomainResult;

pub struct FsCookieRepo {
    dir: PathBuf,
}

impl FsCookieRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Sanitize domain for use as a filename (replace dots and colons).
    fn file_path(&self, domain: &str) -> PathBuf {
        let sanitized = domain.replace('.', "_").replace(':', "_");
        self.dir.join(format!("{}.json", sanitized))
    }
}

impl CookieRepository for FsCookieRepo {
    fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
        let mut result = Vec::new();
        if !self.dir.exists() {
            return Ok(result);
        }
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(jar) = serde_json::from_str::<CookieJar>(&content) {
                    result.push(jar);
                }
            }
        }
        Ok(result)
    }

    fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
        let path = self.file_path(domain);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    fn save(&self, jar: &CookieJar) -> DomainResult<()> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(jar)?;
        fs::write(self.file_path(&jar.domain), json)?;
        Ok(())
    }

    fn clear(&self) -> DomainResult<()> {
        if self.dir.exists() {
            for entry in fs::read_dir(&self.dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "json") {
                    fs::remove_file(&path)?;
                }
            }
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
}
