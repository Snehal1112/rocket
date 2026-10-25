//! The cookie service behind `rok.cookies`. Every call is one locked read-modify-write on the jars.

use chrono::{DateTime, Utc};
use rocket_http::{
    build_cookie, matching_cookies, Cookie, CookieInput, CookieJar, CookieJarLock,
    CookieRepository, UrlScope,
};
use rocket_shared::error::DomainResult;

pub(crate) struct ScriptCookies<'a> {
    repo: &'a dyn CookieRepository,
    lock: &'a CookieJarLock,
}

impl<'a> ScriptCookies<'a> {
    pub(crate) fn new(repo: &'a dyn CookieRepository, lock: &'a CookieJarLock) -> Self {
        Self { repo, lock }
    }

    /// Runs `f` under the jar lock on a repo fixed to one workspace.
    fn locked<T>(
        &self,
        f: impl FnOnce(&dyn CookieRepository) -> DomainResult<T>,
    ) -> DomainResult<T> {
        let _guard = self.lock.lock();
        let pinned = self.repo.pinned();
        f(pinned.as_deref().unwrap_or(self.repo))
    }

    /// The unexpired cookies that would be sent to the URL.
    pub(crate) fn find(&self, scope: &UrlScope, now: DateTime<Utc>) -> DomainResult<Vec<Cookie>> {
        self.locked(|repo| Ok(matching_cookies(&repo.get_all()?, scope, now)))
    }

    /// Adds the cookie, or replaces the one with the same domain, path and name.
    pub(crate) fn upsert(
        &self,
        scope: &UrlScope,
        input: &CookieInput,
        now: DateTime<Utc>,
    ) -> DomainResult<Cookie> {
        let cookie = build_cookie(scope, input, now)?;
        self.locked(|repo| {
            let mut jar = repo
                .get_by_domain(&cookie.domain)?
                .unwrap_or_else(|| CookieJar::new(cookie.domain.clone()));
            jar.add(cookie.clone());
            repo.save(&jar)?;
            Ok(cookie)
        })
    }

    /// Removes the cookies of that name that would be sent to the URL.
    pub(crate) fn remove(
        &self,
        scope: &UrlScope,
        name: &str,
        now: DateTime<Utc>,
    ) -> DomainResult<usize> {
        self.remove_matching(scope, now, |c| c.name == name)
    }

    /// Removes every cookie that would be sent to the URL.
    pub(crate) fn clear_for_url(
        &self,
        scope: &UrlScope,
        now: DateTime<Utc>,
    ) -> DomainResult<usize> {
        self.remove_matching(scope, now, |_| true)
    }

    /// Empties every jar.
    pub(crate) fn clear_all(&self) -> DomainResult<()> {
        self.locked(|repo| repo.clear())
    }

    /// Removes the URL-matching cookies that `pick` accepts and returns how many went.
    fn remove_matching(
        &self,
        scope: &UrlScope,
        now: DateTime<Utc>,
        pick: impl Fn(&Cookie) -> bool,
    ) -> DomainResult<usize> {
        self.locked(|repo| {
            let mut removed = 0;
            for mut jar in repo.get_all()? {
                // Decide per jar so a match never leaks into another jar.
                let doomed: Vec<Cookie> = matching_cookies(std::slice::from_ref(&jar), scope, now)
                    .into_iter()
                    .filter(|c| pick(c))
                    .collect();
                let before = jar.cookies.len();
                jar.cookies.retain(|c| {
                    !doomed
                        .iter()
                        .any(|d| d.domain == c.domain && d.path == c.path && d.name == c.name)
                });
                if jar.cookies.len() != before {
                    removed += before - jar.cookies.len();
                    repo.save(&jar)?;
                }
            }
            Ok(removed)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use rocket_shared::error::DomainError;
    use std::sync::{Arc, Mutex};

    /// An in-memory repo that can hand out a pinned repo.
    #[derive(Default, Clone)]
    struct MemRepo {
        jars: Arc<Mutex<Vec<CookieJar>>>,
        pin_to: Option<Box<MemRepo>>,
    }

    impl CookieRepository for MemRepo {
        fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
            Ok(self.jars.lock().expect("jars").clone())
        }
        fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>> {
            Ok(self
                .jars
                .lock()
                .expect("jars")
                .iter()
                .find(|j| j.domain == domain)
                .cloned())
        }
        fn save(&self, jar: &CookieJar) -> DomainResult<()> {
            let mut jars = self.jars.lock().expect("jars");
            jars.retain(|j| j.domain != jar.domain);
            jars.push(jar.clone());
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            self.jars.lock().expect("jars").clear();
            Ok(())
        }
        fn pinned(&self) -> Option<Box<dyn CookieRepository>> {
            self.pin_to
                .as_ref()
                .map(|p| Box::new((**p).clone()) as Box<dyn CookieRepository>)
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0)
            .single()
            .expect("valid date")
    }

    fn scope(url: &str) -> UrlScope {
        UrlScope::parse(url).expect("scope")
    }

    fn input(key: &str, value: &str) -> CookieInput {
        CookieInput {
            key: key.into(),
            value: value.into(),
            ..Default::default()
        }
    }

    fn count(repo: &MemRepo) -> usize {
        repo.get_all()
            .expect("jars")
            .iter()
            .map(|j| j.cookies.len())
            .sum()
    }

    #[test]
    fn upsert_adds_then_replaces_by_name_domain_and_path() {
        let repo = MemRepo::default();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let s = scope("https://api.example.com/v1/x");
        svc.upsert(&s, &input("sid", "a"), now()).expect("first");
        svc.upsert(&s, &input("sid", "b"), now()).expect("second");
        let all = repo.get_all().expect("jars");
        assert_eq!(count(&repo), 1);
        assert_eq!(all[0].cookies[0].value, "b");
    }

    #[test]
    fn find_returns_only_cookies_that_match_the_url() {
        let repo = MemRepo::default();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let s = scope("http://api.example.com/v1/x");
        svc.upsert(&s, &input("ok", "1"), now()).expect("ok");
        svc.upsert(&scope("http://other.com/"), &input("dom", "1"), now())
            .expect("other domain");
        let mut p = input("path", "1");
        p.path = Some("/admin".into());
        svc.upsert(&s, &p, now()).expect("other path");
        let mut sec = input("sec", "1");
        sec.secure = Some(true);
        svc.upsert(&s, &sec, now()).expect("secure");
        let mut old = input("old", "1");
        old.expires = Some((now() - Duration::days(1)).to_rfc3339());
        svc.upsert(&s, &old, now()).expect("expired");
        let names: Vec<String> = svc
            .find(&s, now())
            .expect("find")
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, vec!["ok".to_string()]);
    }

    #[test]
    fn remove_deletes_only_matching_cookies_of_that_name() {
        let repo = MemRepo::default();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let a = scope("https://a.example.com/");
        let b = scope("https://b.example.com/");
        svc.upsert(&a, &input("sid", "1"), now()).expect("a sid");
        svc.upsert(&a, &input("keep", "1"), now()).expect("a keep");
        svc.upsert(&b, &input("sid", "1"), now()).expect("b sid");
        assert_eq!(svc.remove(&a, "sid", now()).expect("remove"), 1);
        assert_eq!(count(&repo), 2);
        assert_eq!(svc.find(&b, now()).expect("find").len(), 1);
    }

    #[test]
    fn clear_for_url_clears_only_matching_cookies() {
        let repo = MemRepo::default();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let a = scope("https://a.example.com/");
        let b = scope("https://b.example.com/");
        svc.upsert(&a, &input("x", "1"), now()).expect("a");
        svc.upsert(&a, &input("y", "1"), now()).expect("a2");
        svc.upsert(&b, &input("x", "1"), now()).expect("b");
        assert_eq!(svc.clear_for_url(&a, now()).expect("clear"), 2);
        assert_eq!(count(&repo), 1);
    }

    #[test]
    fn clear_all_empties_every_jar() {
        let repo = MemRepo::default();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        svc.upsert(&scope("https://a.example.com/"), &input("x", "1"), now())
            .expect("a");
        svc.upsert(&scope("https://b.example.com/"), &input("x", "1"), now())
            .expect("b");
        svc.clear_all().expect("clear");
        assert_eq!(count(&repo), 0);
    }

    #[test]
    fn upsert_surfaces_a_build_error_and_writes_nothing() {
        let repo = MemRepo::default();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let mut i = input("__Host-sid", "1");
        i.secure = Some(true);
        i.domain = Some("example.com".into());
        let r = svc.upsert(&scope("https://api.example.com/"), &i, now());
        assert!(matches!(r, Err(DomainError::InvalidInput(_))));
        assert_eq!(count(&repo), 0);
    }

    #[test]
    fn operations_hold_the_lock() {
        type Op = Box<dyn Fn(&ScriptCookies) + Send + Sync>;
        let s = scope("https://a.example.com/");
        let ops: Vec<(&str, Op)> = vec![
            (
                "find",
                Box::new({
                    let s = s.clone();
                    move |svc| {
                        svc.find(&s, now()).expect("find");
                    }
                }),
            ),
            (
                "upsert",
                Box::new({
                    let s = s.clone();
                    move |svc| {
                        svc.upsert(&s, &input("x", "1"), now()).expect("upsert");
                    }
                }),
            ),
            (
                "remove",
                Box::new({
                    let s = s.clone();
                    move |svc| {
                        svc.remove(&s, "x", now()).expect("remove");
                    }
                }),
            ),
            (
                "clear_for_url",
                Box::new({
                    let s = s.clone();
                    move |svc| {
                        svc.clear_for_url(&s, now()).expect("clear_for_url");
                    }
                }),
            ),
            (
                "clear_all",
                Box::new(|svc| {
                    svc.clear_all().expect("clear_all");
                }),
            ),
        ];
        for (name, op) in &ops {
            let repo = MemRepo::default();
            let lock = CookieJarLock::default();
            let (tx, rx) = std::sync::mpsc::channel();
            let guard = lock.lock();
            std::thread::scope(|t| {
                t.spawn(|| {
                    op(&ScriptCookies::new(&repo, &lock));
                    tx.send(()).expect("send");
                });
                assert!(
                    rx.recv_timeout(std::time::Duration::from_millis(200))
                        .is_err(),
                    "{name} did not wait for the lock"
                );
                drop(guard);
                assert!(
                    rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok(),
                    "{name} did not finish after the lock was released"
                );
            });
        }
    }

    #[test]
    fn upsert_uses_a_pinned_repo() {
        let pinned = MemRepo::default();
        let repo = MemRepo {
            pin_to: Some(Box::new(pinned.clone())),
            ..Default::default()
        };
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        svc.upsert(&scope("https://a.example.com/"), &input("x", "1"), now())
            .expect("upsert");
        assert_eq!(count(&pinned), 1);
        assert_eq!(count(&repo), 0);
    }

    /// Two jars that each hold a legacy cookie with an empty domain, same name and path.
    fn legacy_repo() -> MemRepo {
        let repo = MemRepo::default();
        for host in ["a.test", "b.test"] {
            let mut jar = CookieJar::new(host);
            jar.cookies.push(Cookie {
                name: "sid".into(),
                value: "1".into(),
                domain: String::new(),
                path: "/".into(),
                secure: false,
                http_only: false,
                expires: None,
                same_site: None,
            });
            repo.save(&jar).expect("save");
        }
        repo
    }

    #[test]
    fn remove_does_not_touch_empty_domain_cookies_in_other_jars() {
        let repo = legacy_repo();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let removed = svc
            .remove(&scope("http://a.test/"), "sid", now())
            .expect("remove");
        assert_eq!(removed, 1);
        let b = repo.get_by_domain("b.test").expect("get").expect("jar");
        assert_eq!(b.cookies.len(), 1);
    }

    #[test]
    fn clear_for_url_does_not_touch_empty_domain_cookies_in_other_jars() {
        let repo = legacy_repo();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let removed = svc
            .clear_for_url(&scope("http://a.test/"), now())
            .expect("clear");
        assert_eq!(removed, 1);
        let b = repo.get_by_domain("b.test").expect("get").expect("jar");
        assert_eq!(b.cookies.len(), 1);
    }

    #[test]
    fn upsert_with_the_same_name_on_another_path_keeps_both() {
        let repo = MemRepo::default();
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let s = scope("https://a.example.com/");
        let mut one = input("sid", "1");
        one.path = Some("/a".into());
        let mut two = input("sid", "2");
        two.path = Some("/b".into());
        svc.upsert(&s, &one, now()).expect("one");
        svc.upsert(&s, &two, now()).expect("two");
        assert_eq!(count(&repo), 2);
    }

    #[test]
    fn upsert_and_find_work_on_an_fs_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = rocket_infra::FsCookieRepo::new(dir.path().to_path_buf());
        let lock = CookieJarLock::default();
        let svc = ScriptCookies::new(&repo, &lock);
        let s = scope("https://a.example.com/");
        svc.upsert(&s, &input("x", "1"), now()).expect("upsert");
        assert_eq!(svc.find(&s, now()).expect("find").len(), 1);
        assert_eq!(svc.remove(&s, "x", now()).expect("remove"), 1);
    }
}
