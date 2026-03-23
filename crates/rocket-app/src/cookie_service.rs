use rocket_http::{CookieJar, CookieRepository};
use rocket_shared::error::DomainResult;
use rocket_shared::events::EventPublisher;

pub struct CookieService {
    repo: Box<dyn CookieRepository>,
    #[allow(dead_code)]
    events: Box<dyn EventPublisher>,
}

impl CookieService {
    pub fn new(repo: Box<dyn CookieRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self { repo, events }
    }

    pub fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
        self.repo.get_all()
    }

    pub fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
        self.repo.get_by_domain(domain)
    }

    pub fn save(&self, jar: &CookieJar) -> DomainResult<()> {
        self.repo.save(jar)
    }

    pub fn clear(&self) -> DomainResult<()> {
        self.repo.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainResult;
    use rocket_shared::events::NullEventPublisher;
    use std::sync::Mutex;

    struct MockCookieRepo {
        jars: Mutex<Vec<CookieJar>>,
    }

    impl MockCookieRepo {
        fn new() -> Self {
            Self { jars: Mutex::new(Vec::new()) }
        }
    }

    impl CookieRepository for MockCookieRepo {
        fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
            Ok(self.jars.lock().unwrap().clone())
        }

        fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
            Ok(self.jars.lock().unwrap().iter().find(|j| j.domain == domain).cloned())
        }

        fn save(&self, jar: &CookieJar) -> DomainResult<()> {
            let mut jars = self.jars.lock().unwrap();
            if let Some(existing) = jars.iter_mut().find(|j| j.domain == jar.domain) {
                *existing = jar.clone();
            } else {
                jars.push(jar.clone());
            }
            Ok(())
        }

        fn clear(&self) -> DomainResult<()> {
            self.jars.lock().unwrap().clear();
            Ok(())
        }
    }

    fn make_service() -> CookieService {
        CookieService::new(Box::new(MockCookieRepo::new()), Box::new(NullEventPublisher))
    }

    #[test]
    fn save_and_get_all() {
        let svc = make_service();
        svc.save(&CookieJar::new("example.com")).unwrap();
        assert_eq!(svc.get_all().unwrap().len(), 1);
    }

    #[test]
    fn get_by_domain() {
        let svc = make_service();
        svc.save(&CookieJar::new("api.example.com")).unwrap();
        assert!(svc.get_by_domain("api.example.com").unwrap().is_some());
        assert!(svc.get_by_domain("other.com").unwrap().is_none());
    }

    #[test]
    fn clear_removes_all() {
        let svc = make_service();
        svc.save(&CookieJar::new("a.com")).unwrap();
        svc.save(&CookieJar::new("b.com")).unwrap();
        svc.clear().unwrap();
        assert!(svc.get_all().unwrap().is_empty());
    }
}
