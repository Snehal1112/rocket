use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_http::{CookieJar, CookieRepository};
use rocket_shared::error::DomainResult;

use crate::FsCookieRepo;

/// The cookie jars in `<active workspace>/cookies`.
///
/// Every call builds a short-lived `FsCookieRepo` from the shared active
/// workspace path, like `SharedPathCollectionRepo`, so a workspace switch takes
/// effect on the next call. `RepoCookieStore` pins it once per batch of
/// `Set-Cookie` headers, so one read-modify-write never spans two workspaces.
pub struct SharedPathCookieRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl SharedPathCookieRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    /// The repository for the workspace that is active right now.
    fn current(&self) -> FsCookieRepo {
        let dir = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("cookies");
        FsCookieRepo::new(dir)
    }
}

impl CookieRepository for SharedPathCookieRepo {
    fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
        self.current().get_all()
    }

    fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
        self.current().get_by_domain(domain)
    }

    fn save(&self, jar: &CookieJar) -> DomainResult<()> {
        self.current().save(jar)
    }

    fn clear(&self) -> DomainResult<()> {
        self.current().clear()
    }

    fn pinned(&self) -> Option<Box<dyn CookieRepository>> {
        Some(Box::new(self.current()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::Cookie;

    fn jar(domain: &str, value: &str) -> CookieJar {
        let mut jar = CookieJar::new(domain);
        jar.add(Cookie {
            name: "sid".into(),
            value: value.into(),
            domain: domain.into(),
            path: "/".into(),
            secure: false,
            http_only: false,
            expires: None,
        });
        jar
    }

    fn value(repo: &dyn CookieRepository, domain: &str) -> Option<String> {
        repo.get_by_domain(domain)
            .expect("read")
            .and_then(|j| j.cookies.first().map(|c| c.value.clone()))
    }

    #[test]
    fn reads_and_writes_follow_the_active_workspace() {
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let repo = SharedPathCookieRepo::new(Arc::clone(&path));

        repo.save(&jar("h.test", "a")).expect("save in a");
        assert!(a.path().join("cookies/h_test.yml").exists());

        *path.lock().expect("lock") = b.path().to_path_buf();
        assert!(repo.get_all().expect("list in b").is_empty());
        repo.save(&jar("h.test", "b")).expect("save in b");
        assert!(b.path().join("cookies/h_test.yml").exists());
        repo.clear().expect("clear b");
        assert!(!b.path().join("cookies/h_test.yml").exists());
        assert!(
            a.path().join("cookies/h_test.yml").exists(),
            "clearing b must leave a's jars alone"
        );
    }

    #[test]
    fn switching_back_and_forth_does_not_mix_data() {
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let repo = SharedPathCookieRepo::new(Arc::clone(&path));
        repo.save(&jar("h.test", "a")).expect("save in a");
        *path.lock().expect("lock") = b.path().to_path_buf();
        repo.save(&jar("h.test", "b")).expect("save in b");
        *path.lock().expect("lock") = a.path().to_path_buf();
        assert_eq!(value(&repo, "h.test").as_deref(), Some("a"));
        *path.lock().expect("lock") = b.path().to_path_buf();
        assert_eq!(value(&repo, "h.test").as_deref(), Some("b"));
    }

    #[test]
    fn a_pinned_repository_stays_on_its_workspace_after_a_switch() {
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let repo = SharedPathCookieRepo::new(Arc::clone(&path));
        let pinned = repo.pinned().expect("a shared repository pins");
        *path.lock().expect("lock") = b.path().to_path_buf();
        pinned.save(&jar("h.test", "a")).expect("save");
        assert!(a.path().join("cookies/h_test.yml").exists());
        assert!(!b.path().join("cookies").exists());
    }
}
