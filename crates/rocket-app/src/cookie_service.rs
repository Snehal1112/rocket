use rocket_http::{CookieJar, CookieJarLock, CookieRepository};
use rocket_shared::error::DomainResult;
use rocket_shared::events::EventPublisher;

pub struct CookieService {
    repo: Box<dyn CookieRepository>,
    #[allow(dead_code)]
    events: Box<dyn EventPublisher>,
    lock: CookieJarLock,
}

impl CookieService {
    pub fn new(repo: Box<dyn CookieRepository>, events: Box<dyn EventPublisher>) -> Self {
        Self {
            repo,
            events,
            lock: CookieJarLock::default(),
        }
    }

    /// Uses a jar lock shared with the other writers.
    pub fn with_lock(mut self, lock: CookieJarLock) -> Self {
        self.lock = lock;
        self
    }

    pub fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
        self.repo.get_all()
    }

    pub fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
        self.repo.get_by_domain(domain)
    }

    pub fn save(&self, jar: &CookieJar) -> DomainResult<()> {
        let _guard = self.lock.lock();
        self.repo.save(jar)
    }

    pub fn clear(&self) -> DomainResult<()> {
        let _guard = self.lock.lock();
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
            Self {
                jars: Mutex::new(Vec::new()),
            }
        }
    }

    impl CookieRepository for MockCookieRepo {
        fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
            Ok(self.jars.lock().unwrap().clone())
        }

        fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
            Ok(self
                .jars
                .lock()
                .unwrap()
                .iter()
                .find(|j| j.domain == domain)
                .cloned())
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
        CookieService::new(
            Box::new(MockCookieRepo::new()),
            Box::new(NullEventPublisher),
        )
    }

    #[test]
    fn save_waits_for_the_shared_lock() {
        use std::sync::mpsc;
        use std::time::Duration;
        let lock = rocket_http::CookieJarLock::default();
        let svc = std::sync::Arc::new(make_service().with_lock(lock.clone()));
        let guard = lock.lock();
        let (tx, rx) = mpsc::channel();
        let worker = std::sync::Arc::clone(&svc);
        let handle = std::thread::spawn(move || {
            worker.save(&CookieJar::new("example.com")).expect("save");
            tx.send(()).expect("send");
        });
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        drop(guard);
        rx.recv_timeout(Duration::from_secs(10))
            .expect("save finishes");
        handle.join().expect("thread");
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
