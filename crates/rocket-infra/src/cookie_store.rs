//! A reqwest cookie store backed by the `CookieRepository`, so cookies survive restarts and
//! the existing `cookies/` jars are the single source of truth.
//!
//! reqwest calls the store for every response of a redirect chain, so a `Set-Cookie` on a 302
//! login response is kept and sent on the followed request. Matching and parsing live in
//! `rocket_http::cookie`; this file only does the I/O.

use std::sync::{Arc, Mutex};

use chrono::Utc;
use reqwest::cookie::CookieStore;
use reqwest::header::HeaderValue;
use rocket_http::cookie::{cookie_header, cookies_for_request, parse_set_cookie, SetCookie};
use rocket_http::{Cookie, CookieJar, CookieRepository};
use rocket_shared::error::DomainResult;

pub struct RepoCookieStore {
    repo: Arc<dyn CookieRepository>,
    /// Serializes read-modify-write cycles on the jar files.
    write_lock: Mutex<()>,
}

impl RepoCookieStore {
    pub fn new(repo: Arc<dyn CookieRepository>) -> Self {
        Self {
            repo,
            write_lock: Mutex::new(()),
        }
    }

    fn store(repo: &dyn CookieRepository, cookie: Cookie) -> DomainResult<()> {
        let mut jar = repo
            .get_by_domain(&cookie.domain)?
            .unwrap_or_else(|| CookieJar::new(cookie.domain.clone()));
        jar.add(cookie);
        repo.save(&jar)
    }

    fn remove(
        repo: &dyn CookieRepository,
        domain: &str,
        name: &str,
        path: &str,
    ) -> DomainResult<()> {
        if let Some(mut jar) = repo.get_by_domain(domain)? {
            let before = jar.cookies.len();
            jar.cookies.retain(|c| !(c.name == name && c.path == path));
            if jar.cookies.len() != before {
                repo.save(&jar)?;
            }
        }
        Ok(())
    }
}

impl CookieStore for RepoCookieStore {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &url::Url) {
        let Some(host) = url.host_str() else {
            return;
        };
        // The lock guards no data of its own, so a poisoned lock is safe to recover.
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        // One workspace for the whole batch. A repository that follows the active
        // workspace is read at this moment, so a response that arrives after a
        // workspace switch stores its cookies in the new workspace.
        let pinned = self.repo.pinned();
        let repo: &dyn CookieRepository = pinned.as_deref().unwrap_or(self.repo.as_ref());
        let now = Utc::now();
        for value in cookie_headers {
            let Ok(text) = value.to_str() else {
                continue;
            };
            let outcome = match parse_set_cookie(text, host, url.path(), now) {
                SetCookie::Store(cookie) => Self::store(repo, cookie),
                SetCookie::Remove { domain, name, path } => {
                    Self::remove(repo, &domain, &name, &path)
                }
                SetCookie::Rejected => Ok(()),
            };
            // Never log the header: it carries the cookie value.
            if let Err(e) = outcome {
                tracing::warn!(error = %e, "could not save a cookie");
            }
        }
    }

    fn cookies(&self, url: &url::Url) -> Option<HeaderValue> {
        let host = url.host_str()?;
        let jars = match self.repo.get_all() {
            Ok(jars) => jars,
            Err(e) => {
                tracing::warn!(error = %e, "could not read the cookie jars");
                return None;
            }
        };
        let cookies =
            cookies_for_request(&jars, host, url.path(), url.scheme() == "https", Utc::now());
        HeaderValue::from_str(&cookie_header(&cookies)?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FsCookieRepo;
    use rocket_http::{Cookie, HttpExecutor, HttpRequest};
    use rocket_shared::types::HttpMethod;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn repo() -> (tempfile::TempDir, Arc<FsCookieRepo>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = Arc::new(FsCookieRepo::new(dir.path().to_path_buf()));
        (dir, repo)
    }

    fn executor(repo: Arc<FsCookieRepo>) -> crate::ReqwestExecutor {
        crate::ReqwestExecutor::new().with_cookie_repo(repo)
    }

    #[tokio::test]
    async fn cookie_is_persisted_in_the_repository() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"))
            .mount(&server)
            .await;
        let (_dir, repo) = repo();
        let exec = executor(Arc::clone(&repo));
        let req = HttpRequest::new(HttpMethod::Get, format!("{}/login", server.uri()));
        exec.execute(&req).await.expect("send");

        let jars = repo.get_all().expect("jars");
        let stored: Vec<&Cookie> = jars.iter().flat_map(|j| j.cookies.iter()).collect();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "sid");
        assert_eq!(stored[0].value, "abc");
        assert_eq!(stored[0].domain, "127.0.0.1");
    }

    #[tokio::test]
    async fn stored_cookie_is_sent_on_the_next_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("cookie", "sid=abc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("welcome"))
            .mount(&server)
            .await;
        let (_dir, repo) = repo();
        let exec = executor(repo);
        exec.execute(&HttpRequest::new(
            HttpMethod::Get,
            format!("{}/login", server.uri()),
        ))
        .await
        .expect("login");
        let me = exec
            .execute(&HttpRequest::new(
                HttpMethod::Get,
                format!("{}/me", server.uri()),
            ))
            .await
            .expect("me");
        assert_eq!(me.status, 200);
        assert_eq!(me.body, "welcome");
    }

    #[tokio::test]
    async fn cookie_from_a_redirect_response_is_sent_on_the_followed_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("Location", "/me")
                    .insert_header("Set-Cookie", "sid=abc; Path=/"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("cookie", "sid=abc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("welcome"))
            .mount(&server)
            .await;
        let (_dir, repo) = repo();
        let response = executor(repo)
            .execute(&HttpRequest::new(
                HttpMethod::Get,
                format!("{}/login", server.uri()),
            ))
            .await
            .expect("send");
        assert_eq!(
            response.status, 200,
            "the cookie must reach the followed request"
        );
        assert_eq!(response.body, "welcome");
    }

    #[tokio::test]
    async fn use_cookie_jar_false_neither_sends_nor_stores() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("cookie", "sid=abc"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let (_dir, repo) = repo();
        let exec = executor(Arc::clone(&repo));

        // Jar off: the Set-Cookie is not stored.
        let mut off = HttpRequest::new(HttpMethod::Get, format!("{}/login", server.uri()));
        off.options.use_cookie_jar = false;
        exec.execute(&off).await.expect("login without the jar");
        assert!(repo.get_all().expect("jars").is_empty());

        // Jar on: store it, then a request with the jar off must not send it.
        exec.execute(&HttpRequest::new(
            HttpMethod::Get,
            format!("{}/login", server.uri()),
        ))
        .await
        .expect("login");
        let mut me = HttpRequest::new(HttpMethod::Get, format!("{}/me", server.uri()));
        me.options.use_cookie_jar = false;
        let response = exec.execute(&me).await.expect("me");
        assert_eq!(
            response.status, 404,
            "no cookie header, so the mock does not match"
        );
    }

    #[tokio::test]
    async fn an_explicit_cookie_header_is_not_overridden() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("cookie", "manual=1"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let (_dir, repo) = repo();
        let exec = executor(repo);
        exec.execute(&HttpRequest::new(
            HttpMethod::Get,
            format!("{}/login", server.uri()),
        ))
        .await
        .expect("login");
        let mut me = HttpRequest::new(HttpMethod::Get, format!("{}/me", server.uri()));
        me.headers
            .push(rocket_shared::types::Header::new("Cookie", "manual=1"));
        assert_eq!(exec.execute(&me).await.expect("me").status, 200);
    }

    #[tokio::test]
    async fn the_cookie_jar_follows_a_workspace_switch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("cookie", "sid=abc"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let active = Arc::new(Mutex::new(a.path().to_path_buf()));
        let exec = crate::ReqwestExecutor::new().with_cookie_repo(Arc::new(
            crate::SharedPathCookieRepo::new(Arc::clone(&active)),
        ));
        let get = |p: &str| HttpRequest::new(HttpMethod::Get, format!("{}{p}", server.uri()));

        exec.execute(&get("/login")).await.expect("login in a");
        assert!(a.path().join("cookies").exists());

        *active.lock().expect("lock") = b.path().to_path_buf();
        let in_b = exec.execute(&get("/me")).await.expect("me in b");
        assert_eq!(in_b.status, 404, "a's cookie must not be sent from b");
        assert!(!b.path().join("cookies").exists());

        *active.lock().expect("lock") = a.path().to_path_buf();
        let back_in_a = exec.execute(&get("/me")).await.expect("me in a");
        assert_eq!(
            back_in_a.status, 200,
            "a's cookie is sent again after switching back"
        );
    }

    #[test]
    fn a_max_age_zero_cookie_is_removed_from_the_repository() {
        let (_dir, repo) = repo();
        let store = RepoCookieStore::new(Arc::clone(&repo) as Arc<dyn CookieRepository>);
        let url = reqwest::Url::parse("https://h.test/a").expect("url");
        let set = reqwest::header::HeaderValue::from_static("sid=abc; Path=/");
        store.set_cookies(&mut std::iter::once(&set), &url);
        assert_eq!(repo.get_all().expect("jars")[0].cookies.len(), 1);
        let kill = reqwest::header::HeaderValue::from_static("sid=; Path=/; Max-Age=0");
        store.set_cookies(&mut std::iter::once(&kill), &url);
        assert!(repo.get_all().expect("jars")[0].cookies.is_empty());
    }
}
