# Protocol parity, Plan 02: Cookie jar, binary responses, AWS SigV4 body signing

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist cookies between requests (including across redirects), stop corrupting binary response bodies and let the user preview images and save any body to a file, and make AWS Signature V4 sign the real request body, the real host and a named credentials profile.

**Architecture:** A `RepoCookieStore` in `rocket-infra` implements `reqwest::cookie::CookieStore` on top of the existing `CookieRepository`, so reqwest applies it to every hop of a redirect chain and the jars stay in the existing `cookies/` directory. All parsing and matching (RFC 6265 domain, path, secure, expiry) is pure code in `rocket-http`. `HttpResponse` gains `is_binary` and `body_base64`, decided by a pure classifier in `rocket-http`. AWS signing moves from `apply_auth` (before the body exists) to a step after the body is applied, like OAuth 1.0 already does.

**Tech Stack:** Rust (`rocket-http`, `rocket-infra`, `rocket-app`, `src-tauri`), reqwest `cookie::CookieStore`, wiremock, React + TypeScript, Tauri dialog and fs plugins, Vitest.

**Spec:** [../../specs/opencollection-spec-reference.md](../../specs/opencollection-spec-reference.md) (request `auth` shape incl. `aws-sig-v4`, `runtime.auth`) plus the audit summary below (verified against the code on 2026-10-05).

- No cookie jar. `ReqwestExecutor` builds clients without a cookie provider (`crates/rocket-infra/src/reqwest_executor.rs`, `build_client_with_identity`). `RequestExecutionService.cookie_repo` is `#[allow(dead_code)]` ("Reserved for automatic cookie persistence", `execution_service.rs:265`). `CookieService` and the Tauri commands `get_cookies`, `set_cookies`, `clear_cookies` exist, but there is no cookie UI in `src/`. Doing it in the executor (not in `rocket-app`) is deliberate: reqwest follows redirects internally, so a `Set-Cookie` on a 302 login response is only visible to a client-level cookie provider.
- Binary corruption is confirmed: `reqwest_executor.rs:418` does `String::from_utf8_lossy(&body_bytes)`; `HttpResponse.body` is a `String`; `ResponseBodyViewer.tsx` renders it in Monaco or an iframe. `size_bytes` is correct because it is taken before the lossy conversion. The IPC DTO is `ExecuteRequestResponse` in `src-tauri/src/commands/execution.rs`.
- SigV4 signs an empty payload: `apply_auth` runs before `finish_builder` applies the body, and passes `b""` (`reqwest_executor.rs:785`). Additional defects found while reading it: the signed `host` omits the port (wrong for LocalStack, MinIO and any non-443 endpoint), the canonical query string is not percent-encoded (`aws_sig.rs`), non-S3 canonical paths are not double-encoded, `profile_name` is ignored by the executor, and empty keys are silently signed.
- AUDIT CORRECTION: AWS SigV4 never reaches the backend from the UI. `toApiAuth` in `src/lib/execute-request.ts:136-139` returns `{ authType: 'none' }` for `aws-sig-v4` with a stale comment saying the backend does not support it. The selector and editor exist, but a send goes out unsigned. Task 3 fixes this on the frontend too, and adds the missing profile-name field.
- AUDIT NOTE on PDF: there is no PDF viewer in the app, WebKitGTK cannot render PDF in an iframe, and the CSP in `src-tauri/tauri.conf.json` (`default-src 'self'`, `img-src 'self' data:`) blocks `data:` frames. This plan previews images and offers "Save to file" for everything else, PDF included. Inline PDF preview is a separate decision.

## Global Constraints

- Never apply `#[serde(rename_all = "camelCase")]` to persistence structs. The existing `Cookie` and `CookieJar` types are both persisted (`cookies/*.yml`) and sent over IPC, which is an existing quirk: do not add fields to them in this plan, so the on-disk format stays readable by older builds. The cookie "host only" distinction is therefore encoded in the stored domain (a leading dot marks a domain cookie), not in a new field.
- Production code never panics on bad input: no `unwrap()`, `expect()` or slicing that can panic outside tests.
- Secrets (AWS keys, session tokens, cookie values) must never appear in error messages or logs.
- Always pass `-j4` to `cargo test` and `cargo check`. Never run `cargo test --workspace`; `cargo check -j4 --workspace --tests` is allowed.
- Commit with conventional commits using the `dev-workflow-skills:1-git-commit` skill, staging by explicit path only (several sessions can share this repo; run `git status` first).
- UI: shadcn/ui primitives and `lucide-react` only. Zustand: narrow selectors only.
- Auth is being worked on elsewhere (flow Auth node, "inherit from parent"). Task 3 edits `toApiAuth` for `aws-sig-v4` only. Do not touch `inherit`, `src/lib/flow-auth*.ts` or `src/components/flow/**`.
- `HttpMethod` is no longer `Copy` after Plan 01. Use `.clone()` where a borrowed method is copied.

## Review Focus

- A `Set-Cookie` with `Domain=` that does not cover the request host (or is a bare public suffix such as `com`) must be rejected, otherwise any server can plant cookies for other sites (Task 1 test `rejects_a_domain_attribute_that_does_not_cover_the_host`).
- A cookie set on a redirect response must be sent on the followed request, and must be persisted in the repository, not only in memory (Task 1 tests `cookie_from_a_redirect_response_is_sent_on_the_followed_request` and `cookie_is_persisted_in_the_repository`).
- `use_cookie_jar: false` must neither send nor store cookies, and the load test must run with the jar off so 100 concurrent requests do not rewrite a jar file each (Task 1 tests).
- A binary body must reach the UI byte-exact, and a body that merely declares a text type but is not valid UTF-8 must keep the old lossy text behavior (Task 2 tests `png_body_is_binary_with_base64` and `declared_text_with_invalid_utf8_stays_lossy_text`).
- SigV4 must hash the actual body (and `UNSIGNED-PAYLOAD` for a streamed multipart body), sign `host:port`, and fail with a clear error instead of signing with empty keys (Task 3 tests `signs_the_actual_body_and_host_with_port`, `streamed_body_is_signed_as_unsigned_payload`, `missing_credentials_are_an_error`).

---

## Task 1: Automatic cookie jar

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-http/src/cookie.rs` (matching and `Set-Cookie` parsing; `CookieJar::add` keys by name and path)
- Modify: `crates/rocket-http/src/request.rs` (`RequestOptions.use_cookie_jar`)
- Create: `crates/rocket-infra/src/cookie_store.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (module and export)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`ReqwestExecutor` fields and constructors, `get_or_build_client`, `build_client_with_identity`, `execute`, tests at about lines 1000-1030)
- Modify: `crates/rocket-app/src/execution_service.rs` (`run_load_test`, line 1861)
- Modify: `src-tauri/src/lib.rs` (executor construction, line 321)
- Modify: `src/lib/tauri-api.ts` (`RequestOptions`, line 50)

**Interfaces:**
- Consumes: `rocket_http::{Cookie, CookieJar, CookieRepository}`, `FsCookieRepo` (`rocket-infra`).
- Produces (`rocket-http`):
  - `cookie::SetCookie` (`Store(Cookie)`, `Remove { domain: String, name: String, path: String }`, `Rejected`).
  - `cookie::parse_set_cookie(header: &str, host: &str, request_path: &str, now: DateTime<Utc>) -> SetCookie`.
  - `cookie::cookies_for_request(jars: &[CookieJar], host: &str, path: &str, https: bool, now: DateTime<Utc>) -> Vec<Cookie>`.
  - `cookie::cookie_header(cookies: &[Cookie]) -> Option<String>`.
  - `cookie::{domain_matches, path_matches, default_cookie_path, is_expired}`.
  - `RequestOptions.use_cookie_jar: bool` (serde default `true`).
- Produces (`rocket-infra`):
  - `RepoCookieStore::new(repo: Arc<dyn CookieRepository>) -> Self`, implementing `reqwest::cookie::CookieStore`.
  - `ReqwestExecutor::with_cookie_repo(self, repo: Arc<dyn CookieRepository>) -> Self`.
  - `get_or_build_client(&self, follow_redirects: bool, verify_ssl: bool, use_cookies: bool)`; `build_client_with_identity(follow_redirects, verify_ssl, max_redirects, identity, cookies: Option<Arc<RepoCookieStore>>)`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing `rocket-http` tests**

Append inside the existing `#[cfg(test)] mod tests` of `crates/rocket-http/src/cookie.rs` (the module already has `use super::*;`):

```rust
    use chrono::{Duration, TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0).single().expect("valid date")
    }

    fn cookie(name: &str, domain: &str, path: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "v".into(),
            domain: domain.into(),
            path: path.into(),
            secure: false,
            http_only: false,
            expires: None,
        }
    }

    #[test]
    fn jar_add_replaces_by_name_and_path_only() {
        let mut jar = CookieJar::new("example.com");
        jar.add(cookie("sid", "example.com", "/"));
        jar.add(cookie("sid", "example.com", "/admin"));
        assert_eq!(jar.cookies.len(), 2, "same name on another path is a different cookie");
        let mut again = cookie("sid", "example.com", "/");
        again.value = "new".into();
        jar.add(again);
        assert_eq!(jar.cookies.len(), 2);
        assert_eq!(jar.cookies[0].value, "new");
    }

    #[test]
    fn parses_a_host_only_cookie_with_flags() {
        let parsed = parse_set_cookie(
            "sid=abc; Path=/; HttpOnly; Secure",
            "API.example.com",
            "/login",
            now(),
        );
        assert_eq!(
            parsed,
            SetCookie::Store(Cookie {
                name: "sid".into(),
                value: "abc".into(),
                domain: "api.example.com".into(),
                path: "/".into(),
                secure: true,
                http_only: true,
                expires: None,
            })
        );
    }

    #[test]
    fn a_domain_attribute_makes_a_domain_cookie_with_a_leading_dot() {
        let SetCookie::Store(c) =
            parse_set_cookie("a=1; Domain=Example.com", "api.example.com", "/", now())
        else {
            panic!("expected Store");
        };
        assert_eq!(c.domain, ".example.com");
    }

    #[test]
    fn rejects_a_domain_attribute_that_does_not_cover_the_host() {
        for (header, host) in [
            ("a=1; Domain=other.com", "api.example.com"),
            ("a=1; Domain=com", "api.example.com"),
            ("a=1; Domain=example.com", "evilexample.com"),
            ("a=1; Domain=0.0.5", "10.0.0.5"),
        ] {
            assert_eq!(
                parse_set_cookie(header, host, "/", now()),
                SetCookie::Rejected,
                "{header} from {host}"
            );
        }
    }

    #[test]
    fn a_domain_attribute_equal_to_a_single_label_host_is_allowed() {
        assert!(matches!(
            parse_set_cookie("a=1; Domain=localhost", "localhost", "/", now()),
            SetCookie::Store(_)
        ));
    }

    #[test]
    fn max_age_wins_over_expires_and_zero_removes() {
        let SetCookie::Store(c) = parse_set_cookie(
            "a=1; Max-Age=3600; Expires=Wed, 21 Oct 2015 07:28:00 GMT",
            "h.test",
            "/",
            now(),
        ) else {
            panic!("expected Store");
        };
        assert_eq!(
            c.expires.as_deref(),
            Some((now() + Duration::seconds(3600)).to_rfc3339().as_str())
        );
        assert_eq!(
            parse_set_cookie("a=1; Max-Age=0", "h.test", "/x/y", now()),
            SetCookie::Remove {
                domain: "h.test".into(),
                name: "a".into(),
                path: "/x".into()
            }
        );
    }

    #[test]
    fn a_past_expires_removes_the_cookie() {
        assert!(matches!(
            parse_set_cookie(
                "a=1; Expires=Wed, 21 Oct 2015 07:28:00 GMT",
                "h.test",
                "/",
                now()
            ),
            SetCookie::Remove { .. }
        ));
    }

    #[test]
    fn rejects_a_header_without_a_name() {
        assert_eq!(parse_set_cookie("=x", "h.test", "/", now()), SetCookie::Rejected);
        assert_eq!(parse_set_cookie("novalue", "h.test", "/", now()), SetCookie::Rejected);
    }

    #[test]
    fn default_path_is_the_directory_of_the_request_path() {
        assert_eq!(default_cookie_path("/a/b/c"), "/a/b");
        assert_eq!(default_cookie_path("/a"), "/");
        assert_eq!(default_cookie_path(""), "/");
    }

    #[test]
    fn domain_and_path_matching() {
        assert!(domain_matches("h.test", "h.test"));
        assert!(!domain_matches("h.test", "a.h.test"), "host-only must not match a subdomain");
        assert!(domain_matches(".h.test", "a.h.test"));
        assert!(domain_matches(".h.test", "h.test"));
        assert!(!domain_matches(".h.test", "evilh.test"));
        assert!(!domain_matches(".0.0.5", "10.0.0.5"));
        assert!(path_matches("/", "/anything"));
        assert!(path_matches("/a", "/a"));
        assert!(path_matches("/a", "/a/b"));
        assert!(!path_matches("/a", "/ab"));
        assert!(path_matches("/a/", "/a/b"));
    }

    #[test]
    fn selects_matching_unexpired_cookies_longest_path_first() {
        let mut secure = cookie("s", "h.test", "/");
        secure.secure = true;
        let mut expired = cookie("old", "h.test", "/");
        expired.expires = Some((now() - Duration::seconds(1)).to_rfc3339());
        let mut jar = CookieJar::new("h.test");
        jar.add(cookie("root", "h.test", "/"));
        jar.add(cookie("deep", "h.test", "/a/b"));
        jar.add(secure);
        jar.add(expired);
        jar.add(cookie("other", "elsewhere.test", "/"));
        let http = cookies_for_request(std::slice::from_ref(&jar), "h.test", "/a/b/c", false, now());
        let names: Vec<_> = http.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["deep", "root"], "secure, expired and foreign cookies are left out");
        let https = cookies_for_request(std::slice::from_ref(&jar), "h.test", "/a/b/c", true, now());
        assert!(https.iter().any(|c| c.name == "s"));
        assert_eq!(cookie_header(&http).as_deref(), Some("deep=v; root=v"));
        assert_eq!(cookie_header(&[]), None);
    }

    #[test]
    fn a_cookie_with_an_empty_domain_uses_the_jar_domain() {
        let mut jar = CookieJar::new("h.test");
        jar.add(cookie("c", "", "/"));
        let found = cookies_for_request(&[jar], "h.test", "/", false, now());
        assert_eq!(found.len(), 1);
    }
```

Also change the existing `add_replaces_existing` expectations only if it fails: it uses the same path `/` for both cookies, so it must keep passing.

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -j4 -p rocket-http cookie`
Expected: FAIL to compile (`parse_set_cookie`, `SetCookie`, `cookies_for_request` not found).

- [ ] **Step 4: Implement parsing and matching**

At the top of `crates/rocket-http/src/cookie.rs` add `use chrono::{DateTime, Duration, Utc};` and `use std::net::IpAddr;`. Change `CookieJar::add` to key by name and path:

```rust
    pub fn add(&mut self, cookie: Cookie) {
        // A cookie is identified by its name and path, so one name can live on several paths.
        if let Some(existing) = self
            .cookies
            .iter_mut()
            .find(|c| c.name == cookie.name && c.path == cookie.path)
        {
            *existing = cookie;
        } else {
            self.cookies.push(cookie);
        }
    }
```

Add, above the tests module:

```rust
/// Result of reading one `Set-Cookie` header.
#[derive(Debug, Clone, PartialEq)]
pub enum SetCookie {
    /// Store this cookie, replacing one with the same name and path.
    Store(Cookie),
    /// The server expired the cookie: remove it.
    Remove {
        domain: String,
        name: String,
        path: String,
    },
    /// Not acceptable for this host: ignore it.
    Rejected,
}

/// RFC 6265 default path: the request path up to, not including, its last `/`.
pub fn default_cookie_path(request_path: &str) -> String {
    match request_path.rfind('/') {
        Some(i) if i > 0 && request_path.starts_with('/') => request_path[..i].to_string(),
        _ => "/".to_string(),
    }
}

/// Longest `Max-Age` kept, in seconds (400 days, as in RFC 6265bis).
const MAX_COOKIE_AGE_SECS: i64 = 400 * 24 * 60 * 60;

/// Reads one `Set-Cookie` header received from `host` for a request to `request_path`.
///
/// A cookie without a `Domain` attribute is host-only and is stored under the exact host. A
/// `Domain` attribute makes a domain cookie, stored under `.domain`. A `Domain` that does not
/// cover the host is rejected, and so is a one-label domain such as `com` (there is no public
/// suffix list here), so a server cannot plant cookies for other sites.
pub fn parse_set_cookie(
    header: &str,
    host: &str,
    request_path: &str,
    now: DateTime<Utc>,
) -> SetCookie {
    let mut parts = header.split(';');
    let Some(first) = parts.next() else {
        return SetCookie::Rejected;
    };
    let Some((name, value)) = first.split_once('=') else {
        return SetCookie::Rejected;
    };
    let (name, value) = (name.trim(), value.trim());
    if name.is_empty() {
        return SetCookie::Rejected;
    }
    let host = host.to_ascii_lowercase();

    let mut domain_attr: Option<String> = None;
    let mut path_attr: Option<String> = None;
    let mut secure = false;
    let mut http_only = false;
    let mut expires: Option<DateTime<Utc>> = None;
    let mut max_age: Option<i64> = None;
    for attr in parts {
        let (key, val) = match attr.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (attr.trim(), ""),
        };
        match key.to_ascii_lowercase().as_str() {
            "domain" => {
                let d = val.trim_start_matches('.').to_ascii_lowercase();
                if !d.is_empty() {
                    domain_attr = Some(d);
                }
            }
            "path" if val.starts_with('/') => path_attr = Some(val.to_string()),
            "secure" => secure = true,
            "httponly" => http_only = true,
            "expires" => {
                expires = DateTime::parse_from_rfc2822(val)
                    .ok()
                    .map(|d| d.with_timezone(&Utc));
            }
            "max-age" => max_age = val.parse::<i64>().ok(),
            _ => {}
        }
    }

    let domain = match domain_attr {
        Some(d) => {
            let covers = host == d
                || (host.ends_with(&format!(".{d}")) && host.parse::<IpAddr>().is_err());
            if !covers || (!d.contains('.') && d != host) {
                return SetCookie::Rejected;
            }
            format!(".{d}")
        }
        None => host,
    };
    let path = path_attr.unwrap_or_else(|| default_cookie_path(request_path));

    // Max-Age wins over Expires.
    let expiry = match (max_age, expires) {
        (Some(secs), _) if secs <= 0 => None,
        (Some(secs), _) => Some(now + Duration::seconds(secs.min(MAX_COOKIE_AGE_SECS))),
        (None, Some(at)) if at <= now => None,
        (None, Some(at)) => Some(at),
        (None, None) => {
            return SetCookie::Store(Cookie {
                name: name.to_string(),
                value: value.to_string(),
                domain,
                path,
                secure,
                http_only,
                expires: None,
            })
        }
    };
    match expiry {
        Some(at) => SetCookie::Store(Cookie {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            path,
            secure,
            http_only,
            expires: Some(at.to_rfc3339()),
        }),
        None => SetCookie::Remove {
            domain,
            name: name.to_string(),
            path,
        },
    }
}

/// Whether a cookie stored under `cookie_domain` is sent to `host`. A domain with a leading dot
/// is a domain cookie and also matches subdomains; any other value is host-only. Subdomain
/// matching never applies to an IP address.
pub fn domain_matches(cookie_domain: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    match cookie_domain.strip_prefix('.') {
        Some(d) => {
            let d = d.to_ascii_lowercase();
            host == d || (host.ends_with(&format!(".{d}")) && host.parse::<IpAddr>().is_err())
        }
        None => host == cookie_domain.to_ascii_lowercase(),
    }
}

/// RFC 6265 path matching.
pub fn path_matches(cookie_path: &str, request_path: &str) -> bool {
    let cookie_path = if cookie_path.is_empty() { "/" } else { cookie_path };
    let request_path = if request_path.is_empty() { "/" } else { request_path };
    if cookie_path == request_path {
        return true;
    }
    request_path.starts_with(cookie_path)
        && (cookie_path.ends_with('/') || request_path[cookie_path.len()..].starts_with('/'))
}

/// True when the cookie has an RFC 3339 `expires` in the past. A cookie with no `expires`, or
/// one this code cannot read, is a session cookie and never expires here.
pub fn is_expired(cookie: &Cookie, now: DateTime<Utc>) -> bool {
    cookie
        .expires
        .as_deref()
        .and_then(|e| DateTime::parse_from_rfc3339(e).ok())
        .is_some_and(|e| e.with_timezone(&Utc) <= now)
}

/// The cookies to send to `host` and `path`, longest path first. `https` says whether the
/// request is secure, which `Secure` cookies require.
pub fn cookies_for_request(
    jars: &[CookieJar],
    host: &str,
    path: &str,
    https: bool,
    now: DateTime<Utc>,
) -> Vec<Cookie> {
    let mut found: Vec<Cookie> = jars
        .iter()
        .flat_map(|jar| {
            jar.cookies.iter().map(move |c| {
                let domain = if c.domain.is_empty() {
                    jar.domain.as_str()
                } else {
                    c.domain.as_str()
                };
                (domain, c)
            })
        })
        .filter(|(domain, c)| {
            domain_matches(domain, host)
                && path_matches(&c.path, path)
                && (!c.secure || https)
                && !is_expired(c, now)
        })
        .map(|(_, c)| c.clone())
        .collect();
    found.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
    found
}

/// The `Cookie` request header value for `cookies`, or `None` when there are none.
pub fn cookie_header(cookies: &[Cookie]) -> Option<String> {
    if cookies.is_empty() {
        return None;
    }
    Some(
        cookies
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; "),
    )
}
```

`rocket-http` already depends on `chrono`. `Duration` here is `chrono::Duration` (an alias of `TimeDelta`).

- [ ] **Step 5: Run to verify the `rocket-http` tests pass**

Run: `cargo test -j4 -p rocket-http cookie`
Expected: PASS (new tests and the 4 existing ones).

- [ ] **Step 6: Add `use_cookie_jar` to `RequestOptions`**

In `crates/rocket-http/src/request.rs` add to `RequestOptions` after `max_redirects`:

```rust
    /// Send stored cookies and keep the ones the server sets. On by default. The load test turns
    /// it off, so a burst of requests does not rewrite a jar file for each response.
    #[serde(default = "default_true")]
    pub use_cookie_jar: bool,
```

and `use_cookie_jar: true,` in `impl Default for RequestOptions`. Add this test to the `tests` module of the same file:

```rust
    #[test]
    fn use_cookie_jar_defaults_to_true_when_missing_from_ipc_input() {
        let options: RequestOptions = serde_json::from_str("{}").expect("deserialize");
        assert!(options.use_cookie_jar);
        let off: RequestOptions =
            serde_json::from_str(r#"{"useCookieJar":false}"#).expect("deserialize");
        assert!(!off.use_cookie_jar);
    }
```

Run: `cargo test -j4 -p rocket-http request`
Expected: PASS. Then `cargo check -j4 --workspace --tests`; fix any `RequestOptions { .. }` literal that now misses the field by adding `..Default::default()`.

- [ ] **Step 7: Write the failing `RepoCookieStore` and executor tests**

Create `crates/rocket-infra/src/cookie_store.rs` containing only its test module first:

```rust
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
            .respond_with(
                ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"),
            )
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
            .respond_with(
                ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"),
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
        assert_eq!(response.status, 200, "the cookie must reach the followed request");
        assert_eq!(response.body, "welcome");
    }

    #[tokio::test]
    async fn use_cookie_jar_false_neither_sends_nor_stores() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"),
            )
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
        assert_eq!(response.status, 404, "no cookie header, so the mock does not match");
    }

    #[tokio::test]
    async fn an_explicit_cookie_header_is_not_overridden() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("Set-Cookie", "sid=abc; Path=/"),
            )
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
        me.headers.push(rocket_shared::types::Header::new("Cookie", "manual=1"));
        assert_eq!(exec.execute(&me).await.expect("me").status, 200);
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
```

Add a load-test test in `crates/rocket-app/src/execution_service.rs`, in its `tests` module after `service_run_load_test_resolves_variables_before_firing`:

```rust
    #[tokio::test]
    async fn run_load_test_turns_the_cookie_jar_off() {
        struct JarFlagExecutor(Mutex<Vec<bool>>);
        #[async_trait]
        impl HttpExecutor for JarFlagExecutor {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.lock().expect("lock").push(req.options.use_cookie_jar);
                Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: "{}".into(),
                    duration_ms: 1,
                    ttfb_ms: 1,
                    size_bytes: 2,
                })
            }
        }
        struct Shared(Arc<JarFlagExecutor>);
        #[async_trait]
        impl HttpExecutor for Shared {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }
        let flags = Arc::new(JarFlagExecutor(Mutex::new(Vec::new())));
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::empty()),
            Arc::new(Shared(Arc::clone(&flags))),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        let config = rocket_http::LoadTestConfig {
            concurrency: 1,
            total_requests: 2,
            interval_ms: 0,
            duration_cap_secs: None,
        };
        svc.run_load_test(sample_input("https://h.test/x", None), config)
            .await
            .expect("load test");
        let seen = flags.0.lock().expect("lock").clone();
        assert!(!seen.is_empty());
        assert!(seen.iter().all(|jar_on| !jar_on), "load test requests must not use the jar");
    }
```

(The `HttpResponse` literal above is patched automatically by the mechanical step in Task 2.)

- [ ] **Step 8: Run to verify failure**

Run: `cargo test -j4 -p rocket-infra cookie_store`
Expected: FAIL to compile (`RepoCookieStore`, `with_cookie_repo` not found; the `cookie_store` module is not declared yet).

- [ ] **Step 9: Implement `RepoCookieStore`**

Prepend to `crates/rocket-infra/src/cookie_store.rs`:

```rust
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

    fn store(&self, cookie: Cookie) -> DomainResult<()> {
        let mut jar = self
            .repo
            .get_by_domain(&cookie.domain)?
            .unwrap_or_else(|| CookieJar::new(cookie.domain.clone()));
        jar.add(cookie);
        self.repo.save(&jar)
    }

    fn remove(&self, domain: &str, name: &str, path: &str) -> DomainResult<()> {
        if let Some(mut jar) = self.repo.get_by_domain(domain)? {
            let before = jar.cookies.len();
            jar.cookies.retain(|c| !(c.name == name && c.path == path));
            if jar.cookies.len() != before {
                self.repo.save(&jar)?;
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
        let now = Utc::now();
        for value in cookie_headers {
            let Ok(text) = value.to_str() else {
                continue;
            };
            let outcome = match parse_set_cookie(text, host, url.path(), now) {
                SetCookie::Store(cookie) => self.store(cookie),
                SetCookie::Remove { domain, name, path } => self.remove(&domain, &name, &path),
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
```

In `crates/rocket-infra/src/lib.rs` add `mod cookie_store;` (alphabetical, after `pub mod clone_destination_capabilities;`) and `pub use cookie_store::RepoCookieStore;` next to the other `pub use` lines. `crates/rocket-infra/Cargo.toml` already has `chrono`, `url` and `tracing`.

- [ ] **Step 10: Wire the store into the executor**

In `crates/rocket-infra/src/reqwest_executor.rs`:

1. Add `use crate::cookie_store::RepoCookieStore;` and `use rocket_http::CookieRepository;`.
2. Change the cache key and add the store field:

```rust
    // Cache of reqwest::Clients keyed on (follow_redirects, verify_ssl, use_cookies).
    // These are the only HttpRequest options that force a different Client::builder()
    // configuration; everything else (headers, body, query, timeout, auth) is applied
    // per-request on the request builder. At most 8 distinct keys can ever exist.
    clients: Mutex<HashMap<(bool, bool, bool), Client>>,
    allowed_base: Option<Arc<Mutex<std::path::PathBuf>>>,
    /// Shared by every client that has cookies on. `None` means no jar is configured.
    cookie_store: Option<Arc<RepoCookieStore>>,
```

3. Add `cookie_store: None,` to both constructors and this builder method:

```rust
    /// Keeps cookies between requests in `repo`, for requests whose `use_cookie_jar` is on.
    pub fn with_cookie_repo(mut self, repo: Arc<dyn CookieRepository>) -> Self {
        self.cookie_store = Some(Arc::new(RepoCookieStore::new(repo)));
        self
    }
```

4. Replace `get_or_build_client`:

```rust
    fn get_or_build_client(
        &self,
        follow_redirects: bool,
        verify_ssl: bool,
        use_cookies: bool,
    ) -> DomainResult<Client> {
        // Without a configured jar the cookie flag changes nothing, so it stays out of the key.
        let use_cookies = use_cookies && self.cookie_store.is_some();
        let key = (follow_redirects, verify_ssl, use_cookies);
        // The cache only holds clients, so a poisoned lock is safe to recover.
        let mut cache = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = cache.get(&key) {
            // reqwest::Client::clone is cheap — internally Arc.
            return Ok(c.clone());
        }
        let cookies = if use_cookies {
            self.cookie_store.clone()
        } else {
            None
        };
        let client =
            build_client_with_identity(follow_redirects, verify_ssl, None, None, cookies)?;
        cache.insert(key, client.clone());
        Ok(client)
    }
```

5. In `build_client_with_identity` add a trailing parameter `cookies: Option<Arc<RepoCookieStore>>` and, before `builder.build()`, add:

```rust
    if let Some(store) = cookies {
        builder = builder.cookie_provider(store);
    }
```

6. Update every other caller of `build_client_with_identity` (`build_client_impl`, `ReqwestTokenClientProvider::client_for`, `fetch_client_credentials_token`) to pass `None` (OAuth2 token requests never use the jar).
7. In `execute`, replace the client selection with:

```rust
        let cookies = if request.options.use_cookie_jar {
            self.cookie_store.clone()
        } else {
            None
        };
        let client = if identity.is_some() || request.options.max_redirects.is_some() {
            // The shared client cache is keyed without an identity, so this gets its own client.
            build_client_with_identity(
                request.options.follow_redirects,
                request.options.verify_ssl,
                request.options.max_redirects,
                identity,
                cookies,
            )?
        } else {
            self.get_or_build_client(
                request.options.follow_redirects,
                request.options.verify_ssl,
                cookies.is_some(),
            )?
        };
```

8. Update the existing tests that call `get_or_build_client(a, b)` (`executor_caches_client_on_first_use`, `executor_keeps_working_after_a_panic_poisons_the_cache_lock`, `executor_builds_different_clients_for_different_options`) to pass `true` as the third argument; their cache-count expectations stay the same because no jar is configured.

In `execution_service.rs` `run_load_test`, after `resolve_request` and the certificate step, add:

```rust
        // A burst of concurrent requests must not rewrite a jar file for every response.
        resolved.options.use_cookie_jar = false;
```

In `src-tauri/src/lib.rs` replace the executor construction (line 321) with:

```rust
            let executor: Arc<dyn rocket_http::HttpExecutor> = Arc::new(
                ReqwestExecutor::with_allowed_base(Arc::clone(&active_workspace_path))
                    .with_cookie_repo(Arc::new(FsCookieRepo::new(cookies_dir.clone()))),
            );
```

In `src/lib/tauri-api.ts` add to `RequestOptions` (line 50): `/** Send and keep cookies. On by default; the backend treats a missing value as true. */ useCookieJar?: boolean;`.

- [ ] **Step 11: Run all checks**

Run: `cargo test -j4 -p rocket-infra cookie_store`
Expected: PASS (6 tests).

Run: `cargo test -j4 -p rocket-infra reqwest_executor`
Expected: PASS.

Run: `cargo test -j4 -p rocket-app run_load_test`
Expected: PASS.

Run: `cargo check -j4 --workspace --tests && yarn tsc --noEmit`
Expected: PASS.

Manual check (needs `yarn tauri dev`): send a request to `https://httpbin.org/cookies/set?a=1`, then `https://httpbin.org/cookies`; the second response must list `a`. Inspect `<workspace>/cookies/httpbin_org.yml`.

- [ ] **Step 12: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, staging by explicit path (every file listed under Files for this task). Suggested subject: `feat(http): keep cookies between requests with a persistent jar`.

---

## Task 2: Binary response handling, image preview and save to file

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-http/src/response.rs` (fields, `Default`, classifier)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (response building at about line 396-430, new test module)
- Modify (mechanical): every `HttpResponse { .. }` literal in the workspace (about 25 sites in `rocket-app`, `rocket-http`, `rocket-infra`, `rocket-scripting`)
- Modify: `src-tauri/src/commands/execution.rs` (`ExecuteRequestResponse` and its `From`)
- Modify: `src-tauri/capabilities/default.json` (`fs:allow-write-file`)
- Modify: `src/lib/tauri-api.ts` (`HttpResponse`, line 331), `src/types/pane-types.ts` (`ResponseState`, line 370), `src/lib/execute-request.ts` (response mapping at line 577 and the console entry)
- Create: `src/lib/response-binary.ts`, `src/lib/__tests__/response-binary.test.ts`
- Create: `src/components/response/BinaryResponsePanel.tsx`, `src/components/response/__tests__/BinaryResponsePanel.test.tsx`
- Modify: `src/components/response/ResponseBodyViewer.tsx`

**Interfaces:**
- Produces (Rust):
  - `HttpResponse.is_binary: bool` (serde default false, skipped when false) and `HttpResponse.body_base64: Option<String>` (skipped when `None`); `HttpResponse` derives `Default`.
  - `rocket_http::response::{BodyPayload, body_from_bytes, MAX_BINARY_BODY_BYTES}` with `body_from_bytes(content_type: Option<&str>, bytes: &[u8], max_binary_bytes: usize) -> BodyPayload` and `BodyPayload { text: String, is_binary: bool, base64: Option<String> }`.
  - `ExecuteRequestResponse.is_binary` and `ExecuteRequestResponse.body_base64` (camelCase over IPC: `isBinary`, `bodyBase64`).
- Produces (TS):
  - `HttpResponse.isBinary?: boolean`, `HttpResponse.bodyBase64?: string`, the same two optional fields on `ResponseState`.
  - `base64ToBytes`, `isPreviewableImage`, `suggestedFileName` in `src/lib/response-binary.ts`.
  - `BinaryResponsePanel({ response, sizeLabel }: { response: ResponseState; sizeLabel: string })`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. This task does not change the persisted format; history entries do not store bodies.

- [ ] **Step 2: Write the failing classifier tests**

Append a test module to `crates/rocket-http/src/response.rs` inside its existing `#[cfg(test)] mod tests` (it has `use super::*;`):

```rust
    #[test]
    fn json_body_stays_text() {
        let p = body_from_bytes(Some("application/json; charset=utf-8"), b"{\"a\":1}", 1024);
        assert!(!p.is_binary);
        assert_eq!(p.text, "{\"a\":1}");
        assert_eq!(p.base64, None);
    }

    #[test]
    fn png_body_is_binary_with_base64() {
        let bytes = [0x89, 0x50, 0x4e, 0x47, 0xff, 0xfe, 0x00];
        let p = body_from_bytes(Some("image/png"), &bytes, 1024);
        assert!(p.is_binary);
        assert_eq!(p.text, "");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(p.base64.expect("payload"))
            .expect("valid base64");
        assert_eq!(decoded, bytes, "bytes must arrive unchanged");
    }

    #[test]
    fn svg_stays_text_because_it_is_xml() {
        let p = body_from_bytes(Some("image/svg+xml"), b"<svg/>", 1024);
        assert!(!p.is_binary);
        assert_eq!(p.text, "<svg/>");
    }

    #[test]
    fn declared_text_with_invalid_utf8_stays_lossy_text() {
        let p = body_from_bytes(Some("text/html; charset=latin-1"), &[b'c', b'a', b'f', 0xe9], 1024);
        assert!(!p.is_binary);
        assert!(p.text.starts_with("caf"));
    }

    #[test]
    fn unknown_type_is_decided_by_the_bytes() {
        assert!(!body_from_bytes(Some("application/x-custom"), "héllo".as_bytes(), 1024).is_binary);
        assert!(body_from_bytes(Some("application/x-custom"), &[0xff, 0xfe, 0xfd], 1024).is_binary);
        assert!(body_from_bytes(None, &[b'a', 0, b'b'], 1024).is_binary, "NUL means binary");
        assert!(!body_from_bytes(None, b"plain", 1024).is_binary);
    }

    #[test]
    fn well_known_binary_types_are_binary_even_when_the_bytes_are_ascii() {
        for ct in [
            "application/pdf",
            "application/zip",
            "application/octet-stream",
            "audio/mpeg",
            "video/mp4",
            "font/woff2",
            "image/jpeg",
        ] {
            assert!(body_from_bytes(Some(ct), b"abc", 1024).is_binary, "{ct}");
        }
    }

    #[test]
    fn binary_over_the_cap_is_flagged_but_carries_no_payload() {
        let p = body_from_bytes(Some("application/pdf"), &[1, 2, 3, 4, 5], 4);
        assert!(p.is_binary);
        assert_eq!(p.base64, None);
        assert_eq!(p.text, "");
    }

    #[test]
    fn office_documents_are_binary_not_xml() {
        let zip_like = [0x50, 0x4b, 0x03, 0x04, 0xff, 0x00];
        for ct in [
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "application/vnd.ms-excel",
        ] {
            assert!(body_from_bytes(Some(ct), &zip_like, 1024).is_binary, "{ct}");
        }
        // A vendor type with a +json or +xml suffix is still text.
        assert!(!body_from_bytes(Some("application/vnd.api+json"), b"{}", 1024).is_binary);
    }

    #[test]
    fn an_empty_body_is_text() {
        let p = body_from_bytes(Some("application/octet-stream"), b"", 1024);
        assert!(!p.is_binary);
        assert_eq!(p.text, "");
    }
```

Add `use base64::Engine;` at the top of that test module.

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -j4 -p rocket-http response`
Expected: FAIL to compile (`body_from_bytes` not found).

- [ ] **Step 4: Implement the classifier and the new fields**

In `crates/rocket-http/src/response.rs` replace the struct definition with:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<Header>,
    /// The body as text. Empty for a binary body, see `is_binary`.
    pub body: String,
    /// Total time from request sent to body fully received, in milliseconds.
    pub duration_ms: u64,
    /// Time from request sent to first byte of the response headers, in milliseconds.
    pub ttfb_ms: u64,
    pub size_bytes: usize,
    /// True when the body is not text, so `body` is empty and the bytes are in `body_base64`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_binary: bool,
    /// The raw bytes of a binary body, base64 encoded. `None` for a text body, and for a binary
    /// body larger than `MAX_BINARY_BODY_BYTES`, which is flagged but not carried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
}

/// Largest binary body carried over IPC, in bytes.
pub const MAX_BINARY_BODY_BYTES: usize = 32 * 1024 * 1024;

/// A response body split into what the rest of the app reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyPayload {
    pub text: String,
    pub is_binary: bool,
    pub base64: Option<String>,
}

fn is_textual_type(essence: &str) -> bool {
    if essence.starts_with("text/") {
        return true;
    }
    // Match the subtype exactly. A substring test would misread office documents
    // (`...openxmlformats...`) as XML.
    let subtype = essence.split('/').nth(1).unwrap_or("");
    subtype.ends_with("+json")
        || subtype.ends_with("+xml")
        || matches!(
            subtype,
            "json"
                | "xml"
                | "javascript"
                | "x-javascript"
                | "ecmascript"
                | "yaml"
                | "x-yaml"
                | "x-www-form-urlencoded"
                | "graphql"
                | "sparql-query"
                | "x-ndjson"
                | "ndjson"
                | "sql"
                | "x-sh"
        )
}

fn is_binary_type(content_type: &str) -> bool {
    const PREFIXES: [&str; 4] = ["image/", "audio/", "video/", "font/"];
    const EXACT: [&str; 9] = [
        "application/octet-stream",
        "application/pdf",
        "application/zip",
        "application/gzip",
        "application/x-gzip",
        "application/x-tar",
        "application/x-7z-compressed",
        "application/wasm",
        "application/x-protobuf",
    ];
    PREFIXES.iter().any(|p| content_type.starts_with(p))
        || EXACT.iter().any(|e| content_type == *e)
}

/// Splits response bytes into text or a base64 payload.
///
/// A declared text type (JSON, XML, `text/*`, SVG, ...) is always text, as before, even when the
/// bytes are not valid UTF-8. A declared binary type (image, audio, video, font, PDF, archives,
/// octet-stream) is binary. Anything else, including a missing `Content-Type`, is binary when
/// the bytes are not valid UTF-8 or contain a NUL byte. A binary body larger than
/// `max_binary_bytes` is flagged but its bytes are not carried.
pub fn body_from_bytes(
    content_type: Option<&str>,
    bytes: &[u8],
    max_binary_bytes: usize,
) -> BodyPayload {
    use base64::Engine;

    let essence = content_type
        .and_then(|c| c.split(';').next())
        .map(|c| c.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let binary = if bytes.is_empty() {
        false
    } else if is_textual_type(&essence) {
        false
    } else if is_binary_type(&essence) {
        true
    } else {
        bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
    };
    if !binary {
        return BodyPayload {
            text: String::from_utf8_lossy(bytes).into_owned(),
            is_binary: false,
            base64: None,
        };
    }
    BodyPayload {
        text: String::new(),
        is_binary: true,
        base64: (bytes.len() <= max_binary_bytes)
            .then(|| base64::engine::general_purpose::STANDARD.encode(bytes)),
    }
}
```

`crates/rocket-http/Cargo.toml` already has `base64`.

- [ ] **Step 5: Run to verify the classifier passes**

Run: `cargo test -j4 -p rocket-http response`
Expected: PASS (new tests; the existing `HttpResponse { .. }` literals in this file's tests fail to compile until Step 6, so do Step 6 first if the compiler complains).

- [ ] **Step 6: Patch every `HttpResponse` literal mechanically**

The new fields break every struct literal. `Default` is derived, so each literal only needs `..Default::default()`. Save this script as `/tmp/claude-1000/-home-numericlabs-data-rocket-rocket/a5084ecb-fbfd-45f4-924d-8e7bd220f602/scratchpad/patch_http_response.py` (any path outside the repo) and run it from the repo root:

```python
import pathlib
import re
import subprocess

files = subprocess.check_output(
    ["grep", "-rl", "HttpResponse {", "--include=*.rs", "crates", "src-tauri/src"],
    text=True,
).split()

SKIP_BEFORE = re.compile(r"(->\s*|struct\s+|impl(<[^>]*>)?\s+(\w+(<[^>]*>)?\s+for\s+)?)(\w+::)*$")

for name in files:
    path = pathlib.Path(name)
    src = path.read_text()
    out, last, changed = [], 0, False
    for m in re.finditer(r"HttpResponse \{", src):
        start = m.start()
        if start < last:
            continue
        line_start = src.rfind("\n", 0, start) + 1
        before = src[line_start:start]
        if SKIP_BEFORE.search(before):
            continue
        # The matched text may be preceded by a path such as rocket_http::, which SKIP_BEFORE handles.
        depth, i = 0, m.end() - 1
        while i < len(src):
            if src[i] == "{":
                depth += 1
            elif src[i] == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        inner = src[m.end():i]
        if ".." in inner:
            continue
        indent = re.match(r"\s*", before).group(0)
        stripped = inner.rstrip()
        if "\n" in inner:
            if not stripped.endswith(","):
                stripped += ","
            new_inner = stripped + "\n" + indent + "    ..Default::default()\n" + indent
        else:
            sep = " " if stripped.endswith(",") else ", "
            new_inner = stripped + sep + "..Default::default() "
        out.append(src[last:m.end()])
        out.append(new_inner)
        last = i
        changed = True
    if changed:
        out.append(src[last:])
        path.write_text("".join(out))
        print("patched", name)
```

Run: `python3 /tmp/claude-1000/-home-numericlabs-data-rocket-rocket/a5084ecb-fbfd-45f4-924d-8e7bd220f602/scratchpad/patch_http_response.py`
Expected: a `patched <file>` line for each file with literals. Then run `cargo fmt -p rocket-http -p rocket-app -p rocket-infra -p rocket-scripting` and `git diff --stat`: only files that contain an `HttpResponse { .. }` literal may show up.

Run: `cargo check -j4 --workspace --tests`
Expected: PASS. If a literal was missed or mis-patched (a string containing an unbalanced `{` can confuse the brace counter), the compiler names the file and line; fix it by hand with `..Default::default()`.

- [ ] **Step 7: Write the failing executor tests**

Append to `crates/rocket-infra/src/reqwest_executor.rs`:

```rust
#[cfg(test)]
mod binary_response_tests {
    use super::*;
    use base64::Engine;
    use rocket_shared::types::HttpMethod;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn a_png_body_arrives_byte_exact_as_base64() {
        let bytes: Vec<u8> = vec![0x89, 0x50, 0x4e, 0x47, 0xff, 0xfe, 0x00, 0x80];
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.clone(), "image/png"))
            .mount(&server)
            .await;
        let req = HttpRequest::new(HttpMethod::Get, format!("{}/i.png", server.uri()));
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert!(response.is_binary);
        assert_eq!(response.body, "");
        assert_eq!(response.size_bytes, bytes.len());
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(response.body_base64.expect("payload"))
            .expect("valid base64");
        assert_eq!(decoded, bytes);
    }

    #[tokio::test]
    async fn a_json_body_is_still_text() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("{\"a\":1}", "application/json"))
            .mount(&server)
            .await;
        let req = HttpRequest::new(HttpMethod::Get, format!("{}/j", server.uri()));
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert!(!response.is_binary);
        assert_eq!(response.body, "{\"a\":1}");
        assert_eq!(response.body_base64, None);
    }
}
```

Run: `cargo test -j4 -p rocket-infra binary_response_tests`
Expected: FAIL (the PNG body is turned into replacement characters; `is_binary` is false).

- [ ] **Step 8: Use the classifier in the executor**

In `execute`, just before `let body_bytes = response.bytes()...`, capture the content type:

```rust
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
```

and replace the end of the function (from `let size_bytes = ...` to the `Ok(HttpResponse { .. })`) with:

```rust
        let size_bytes = body_bytes.len();
        let payload = rocket_http::response::body_from_bytes(
            content_type.as_deref(),
            &body_bytes,
            rocket_http::response::MAX_BINARY_BODY_BYTES,
        );

        Ok(HttpResponse {
            status,
            status_text,
            headers,
            body: payload.text,
            duration_ms,
            ttfb_ms,
            size_bytes,
            is_binary: payload.is_binary,
            body_base64: payload.base64,
        })
```

Run: `cargo test -j4 -p rocket-infra binary_response_tests reqwest_executor`
Expected: PASS.

- [ ] **Step 9: Carry the fields over IPC and allow the file write**

In `src-tauri/src/commands/execution.rs` add to `ExecuteRequestResponse`, after `size_bytes`:

```rust
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
```

and in `From<ExecuteRequestOutput>` add `is_binary: out.response.is_binary,` and `body_base64: out.response.body_base64,` after `size_bytes`. Add a test at the end of that file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::HttpResponse;

    #[test]
    fn binary_fields_reach_the_ipc_response() {
        let out = ExecuteRequestOutput {
            response: HttpResponse {
                status: 200,
                is_binary: true,
                body_base64: Some("AAEC".into()),
                size_bytes: 3,
                ..Default::default()
            },
            test_results: vec![],
            console_entries: vec![],
            script_error: None,
            deferred_history: None,
        };
        let json = serde_json::to_value(ExecuteRequestResponse::from(out)).expect("serialize");
        assert_eq!(json["isBinary"], true);
        assert_eq!(json["bodyBase64"], "AAEC");
    }

    #[test]
    fn text_responses_omit_the_binary_fields() {
        let out = ExecuteRequestOutput {
            response: HttpResponse::default(),
            test_results: vec![],
            console_entries: vec![],
            script_error: None,
            deferred_history: None,
        };
        let json = serde_json::to_value(ExecuteRequestResponse::from(out)).expect("serialize");
        assert!(json.get("isBinary").is_none());
        assert!(json.get("bodyBase64").is_none());
    }
}
```

Run: `cargo test -j4 -p rocket commands::execution`
Expected: PASS (the package is named `rocket`; if the filter matches nothing, run `cargo test -j4 -p rocket binary_fields_reach_the_ipc_response`).

In `src-tauri/capabilities/default.json` add `"fs:allow-write-file",` after `"fs:default",`. The save dialog adds the path the user picks to the fs scope at runtime, and this permission lets the page write to it.

- [ ] **Step 10: Write the failing frontend tests**

Create `src/lib/__tests__/response-binary.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { base64ToBytes, isPreviewableImage, suggestedFileName } from '@/lib/response-binary';

describe('response-binary', () => {
  it('decodes base64 to the exact bytes', () => {
    expect(Array.from(base64ToBytes('iVBORw=='))).toEqual([0x89, 0x50, 0x4e, 0x47]);
    expect(base64ToBytes('').length).toBe(0);
  });

  it('previews common image types only', () => {
    for (const ct of ['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/svg+xml']) {
      expect(isPreviewableImage(ct)).toBe(true);
    }
    expect(isPreviewableImage('application/pdf')).toBe(false);
    expect(isPreviewableImage('image/tiff')).toBe(false);
  });

  it('takes the file name from Content-Disposition and strips any directory part', () => {
    const headers = [{ key: 'Content-Disposition', value: 'attachment; filename="../../etc/report.pdf"' }];
    expect(suggestedFileName(headers, 'application/pdf')).toBe('report.pdf');
    const star = [{ key: 'content-disposition', value: "attachment; filename*=UTF-8''a%20b.zip" }];
    expect(suggestedFileName(star, 'application/zip')).toBe('a b.zip');
  });

  it('falls back to response plus an extension from the content type', () => {
    expect(suggestedFileName([], 'image/png')).toBe('response.png');
    expect(suggestedFileName([], 'application/pdf')).toBe('response.pdf');
    expect(suggestedFileName([], 'application/x-unknown')).toBe('response.bin');
  });
});
```

Create `src/components/response/__tests__/BinaryResponsePanel.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ResponseState } from '@/types/pane-types';

const save = vi.fn();
const writeFile = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: (...a: unknown[]) => save(...a) }));
vi.mock('@tauri-apps/plugin-fs', () => ({ writeFile: (...a: unknown[]) => writeFile(...a) }));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { BinaryResponsePanel } from '../BinaryResponsePanel';

function response(overrides: Partial<ResponseState> = {}): ResponseState {
  return {
    status: 200,
    statusText: 'OK',
    headers: [{ id: '1', key: 'Content-Type', value: 'image/png', enabled: true }],
    body: '',
    durationMs: 1,
    ttfbMs: 1,
    sizeBytes: 4,
    activeView: 'pretty',
    isBinary: true,
    bodyBase64: 'iVBORw==',
    ...overrides,
  };
}

describe('BinaryResponsePanel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('previews an image from a data url', () => {
    render(<BinaryResponsePanel response={response()} sizeLabel='4 B' />);
    const img = screen.getByAltText('Response preview') as HTMLImageElement;
    expect(img.src).toBe('data:image/png;base64,iVBORw==');
  });

  it('offers no preview for a pdf but still offers a save', () => {
    render(
      <BinaryResponsePanel
        response={response({
          headers: [{ id: '1', key: 'Content-Type', value: 'application/pdf', enabled: true }],
        })}
        sizeLabel='4 B'
      />,
    );
    expect(screen.queryByAltText('Response preview')).toBeNull();
    expect(screen.getByRole('button', { name: /save to file/i })).toBeEnabled();
  });

  it('writes the exact bytes to the chosen path', async () => {
    save.mockResolvedValue('/tmp/out.png');
    render(<BinaryResponsePanel response={response()} sizeLabel='4 B' />);
    fireEvent.click(screen.getByRole('button', { name: /save to file/i }));
    await waitFor(() => expect(writeFile).toHaveBeenCalledTimes(1));
    const [path, bytes] = writeFile.mock.calls[0];
    expect(path).toBe('/tmp/out.png');
    expect(Array.from(bytes as Uint8Array)).toEqual([0x89, 0x50, 0x4e, 0x47]);
  });

  it('does not write when the dialog is cancelled', async () => {
    save.mockResolvedValue(null);
    render(<BinaryResponsePanel response={response()} sizeLabel='4 B' />);
    fireEvent.click(screen.getByRole('button', { name: /save to file/i }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(writeFile).not.toHaveBeenCalled();
  });

  it('says so when the body is too large to carry', () => {
    render(
      <BinaryResponsePanel response={response({ bodyBase64: undefined })} sizeLabel='40.0 MB' />,
    );
    expect(screen.getByText(/too large/i)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /save to file/i })).toBeNull();
  });
});
```

- [ ] **Step 11: Run to verify failure**

Run: `yarn test response-binary BinaryResponsePanel`
Expected: FAIL (modules not found).

- [ ] **Step 12: Implement the frontend**

`src/lib/tauri-api.ts` `HttpResponse` (line 331): add `/** Set when the body is not text: \`body\` is empty and the bytes are in \`bodyBase64\`. */ isBinary?: boolean; /** Raw bytes of a binary body, base64. Absent when the body was too large to carry. */ bodyBase64?: string;`. `src/types/pane-types.ts` `ResponseState`: add `isBinary?: boolean;` and `bodyBase64?: string;`.

Create `src/lib/response-binary.ts`:

```ts
import type { KeyValueEntry } from '@/types/pane-types';

type HeaderLike = Pick<KeyValueEntry, 'key' | 'value'>;

const PREVIEWABLE_IMAGES = new Set([
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
  'image/bmp',
  'image/avif',
  'image/svg+xml',
  'image/x-icon',
]);

const EXTENSIONS: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
  'image/svg+xml': 'svg',
  'application/pdf': 'pdf',
  'application/zip': 'zip',
  'application/gzip': 'gz',
  'audio/mpeg': 'mp3',
  'video/mp4': 'mp4',
};

export function base64ToBytes(b64: string): Uint8Array {
  const raw = atob(b64);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

export function isPreviewableImage(contentType: string): boolean {
  return PREVIEWABLE_IMAGES.has(contentType.split(';')[0].trim().toLowerCase());
}

// The file name offered by the save dialog: the server's name when it sends one, else
// "response" plus an extension for the content type. Any directory part is dropped.
export function suggestedFileName(headers: HeaderLike[], contentType: string): string {
  const disposition = headers.find((h) => h.key.toLowerCase() === 'content-disposition')?.value;
  const match = disposition ? /filename\*?=(?:UTF-8'')?"?([^";]+)"?/i.exec(disposition) : null;
  if (match) {
    let name = match[1];
    try {
      name = decodeURIComponent(name);
    } catch {
      // Keep the raw value when it is not valid percent-encoding.
    }
    const base = name.split(/[\\/]/).pop()?.trim();
    if (base) return base;
  }
  const essence = contentType.split(';')[0].trim().toLowerCase();
  return `response.${EXTENSIONS[essence] ?? 'bin'}`;
}
```

Create `src/components/response/BinaryResponsePanel.tsx`:

```tsx
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { FileDown } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { base64ToBytes, isPreviewableImage, suggestedFileName } from '@/lib/response-binary';
import type { ResponseState } from '@/types/pane-types';

interface BinaryResponsePanelProps {
  response: ResponseState;
  sizeLabel: string;
}

// Shown instead of the text views when the response body is not text.
export function BinaryResponsePanel({ response, sizeLabel }: BinaryResponsePanelProps) {
  const contentType =
    response.headers.find((h) => h.key.toLowerCase() === 'content-type')?.value ?? '';
  const payload = response.bodyBase64;
  const type = contentType.split(';')[0].trim() || 'unknown type';

  const handleSave = async () => {
    if (!payload) return;
    try {
      const path = await save({ defaultPath: suggestedFileName(response.headers, contentType) });
      if (!path) return;
      await writeFile(path, base64ToBytes(payload));
      toast.success('Saved response body');
    } catch {
      toast.error('Could not save the response body');
    }
  };

  return (
    <div className='flex h-full flex-col gap-3 overflow-auto p-3'>
      <div className='flex items-center gap-3 text-xs text-muted-foreground'>
        <span>Binary response</span>
        <span className='font-mono'>{type}</span>
        <span>{sizeLabel}</span>
        {payload && (
          <Button variant='outline' size='sm' className='ml-auto' onClick={handleSave}>
            <FileDown className='mr-1.5 h-3.5 w-3.5' />
            Save to file
          </Button>
        )}
      </div>
      {!payload && (
        <p className='text-xs text-muted-foreground'>
          This body is too large to preview or save in the app ({sizeLabel}).
        </p>
      )}
      {payload && isPreviewableImage(contentType) && (
        <img
          src={`data:${type};base64,${payload}`}
          alt='Response preview'
          className='max-h-full max-w-full self-start rounded border border-border object-contain'
        />
      )}
    </div>
  );
}
```

`src/components/response/ResponseBodyViewer.tsx`: add `import { BinaryResponsePanel } from './BinaryResponsePanel';`; after `const hasBody = Boolean(response.body);` add:

```tsx
  const isBinary = response.isBinary === true;
  const showBinary =
    isBinary && (activeView === 'pretty' || activeView === 'raw' || activeView === 'preview');
```

Make `hasBody` false-safe for binary by leaving it as is (the toolbar then hides copy/search). In the tab content area, insert before the Pretty block `{showBinary && <BinaryResponsePanel response={response} sizeLabel={formatBytes(response.sizeBytes)} />}` and change the three block guards to `{!showBinary && activeView === 'pretty' &&`, `{!showBinary && activeView === 'raw' &&` and `{!showBinary && activeView === 'preview' && (`. `JSON.parse(response.body)` in the status bar is only reached when `jsonKeyCount !== null`, and `formatBody('')` yields `isJson: false`, so a binary body is safe there.

`src/lib/execute-request.ts`: in the `ResponseState` object built from `result` (line 577) add `isBinary: result.isBinary, bodyBase64: result.bodyBase64,`; in the success `addHttpEntry` call change `responseBody: result.body,` to `responseBody: result.isBinary ? `(binary, ${result.sizeBytes} bytes)` : result.body,`.

- [ ] **Step 13: Run all checks**

Run: `yarn test response-binary BinaryResponsePanel ResponseBodyViewer execute-request`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

Run: `cargo test -j4 -p rocket-http response && cargo test -j4 -p rocket-infra binary_response_tests && cargo check -j4 --workspace --tests`
Expected: PASS.

Manual check (needs `yarn tauri dev`; the save path cannot be unit-tested): send `GET https://httpbin.org/image/png`, confirm the image renders and "Save to file" writes a valid PNG; send `GET https://httpbin.org/bytes/2048` and confirm the panel shows "Binary response" and the saved file has 2048 bytes. If the save fails with a permission error, the `fs:allow-write-file` capability or the dialog scope is the cause.

- [ ] **Step 14: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, staging by explicit path (every file listed under Files for this task, including all files the Step 6 script patched). Suggested subject: `feat(http): carry binary response bodies and add image preview and save`.

---

## Task 3: AWS SigV4 body signing, host port, profile name and the missing UI wiring

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-http/src/aws_sig.rs` (payload-hash entry point, query and path canonicalization, S3 signed header)
- Create: `crates/rocket-infra/src/aws_profile.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (module)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`execute` request send, `apply_auth` AwsSigV4 arm at about line 745, new tests)
- Modify: `crates/rocket-app/src/execution_service.rs` (`resolve_auth`, line 2083)
- Modify: `src/lib/execute-request.ts` (`toApiAuth`, line 136)
- Modify: `src/components/request/AuthEditor.tsx` (profile name field), `src/types/pane-types.ts` (comment at line 354)
- Test: `src/lib/__tests__/execute-request.test.ts`, `src/components/request/__tests__/AuthEditor.test.tsx`

**Interfaces:**
- Consumes: `rocket_http::aws_sig::{AwsCredentials, SignedHeaders}`, `Auth::AwsSigV4 { access_key, secret_key, region, service, session_token, profile_name }`.
- Produces (`rocket-http`):
  - `aws_sig::sign_request_with_payload_hash(method: &str, url: &str, headers: &[(String, String)], payload_hash: &str, credentials: &AwsCredentials, timestamp: &str) -> Result<SignedHeaders, String>`; `sign_request` delegates to it.
  - `aws_sig::hex_sha256(data: &[u8]) -> String` (now `pub`) and `aws_sig::UNSIGNED_PAYLOAD: &str`.
- Produces (`rocket-infra`):
  - `aws_profile::{AwsProfileCredentials, parse_credentials, load_profile_from, load_profile, resolve_credentials}` with `resolve_credentials(access_key: &str, secret_key: &str, region: &str, service: &str, session_token: Option<&str>, profile_name: Option<&str>) -> DomainResult<AwsCredentials>`.

- [ ] **Step 1: Read the spec reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Pin the signing core with the published AWS test vector**

Append inside the existing `#[cfg(test)] mod tests` of `crates/rocket-http/src/aws_sig.rs`. This is the "get-vanilla" vector from the AWS Signature V4 test suite; it must pass on the current code before anything changes and must keep passing after:

```rust
    #[test]
    fn matches_the_aws_get_vanilla_test_vector() {
        let creds = AwsCredentials {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            service: "service".into(),
            session_token: None,
        };
        let signed = sign_request(
            "GET",
            "https://example.amazonaws.com/",
            &[("host".to_string(), "example.amazonaws.com".to_string())],
            b"",
            &creds,
            "20150830T123600Z",
        )
        .expect("sign");
        assert_eq!(
            signed.authorization,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }
```

Run: `cargo test -j4 -p rocket-http matches_the_aws_get_vanilla_test_vector`
Expected: PASS on the current code. If it fails before any change, the vector was mistyped here: re-check it against the AWS documentation ("Signature Version 4 test suite", `get-vanilla`) before touching the implementation.

- [ ] **Step 3: Write the failing `aws_sig` tests**

Append inside the same test module:

```rust
    fn s3_creds() -> AwsCredentials {
        AwsCredentials {
            access_key: "AKID".into(),
            secret_key: "SECRET".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: None,
        }
    }

    #[test]
    fn the_payload_hash_entry_point_matches_hashing_the_body() {
        let body = b"{\"a\":1}";
        let headers = [("host".to_string(), "example.com".to_string())];
        let by_body = sign_request("POST", "https://example.com/x", &headers, body, &s3_creds(), "20240101T000000Z")
            .expect("sign");
        let by_hash = sign_request_with_payload_hash(
            "POST",
            "https://example.com/x",
            &headers,
            &hex_sha256(body),
            &s3_creds(),
            "20240101T000000Z",
        )
        .expect("sign");
        assert_eq!(by_body.authorization, by_hash.authorization);
        assert_eq!(by_hash.x_amz_content_sha256, hex_sha256(body));
    }

    #[test]
    fn an_explicit_unsigned_payload_marker_is_used_verbatim() {
        let signed = sign_request_with_payload_hash(
            "POST",
            "https://example.com/x",
            &[("host".to_string(), "example.com".to_string())],
            UNSIGNED_PAYLOAD,
            &s3_creds(),
            "20240101T000000Z",
        )
        .expect("sign");
        assert_eq!(signed.x_amz_content_sha256, "UNSIGNED-PAYLOAD");
    }

    #[test]
    fn query_values_are_percent_encoded_in_the_canonical_request() {
        // The URL parser decodes `+` and raw spaces to a space and `%2F` to `/`. The canonical
        // query must encode every key and value again, then sort by the encoded key.
        let url = reqwest::Url::parse("https://example.com/p?b=a b&a=1+2&c=%2F").expect("url");
        assert_eq!(canonical_query(&url), "a=1%202&b=a%20b&c=%2F");
    }

    #[test]
    fn non_s3_paths_are_double_encoded_and_s3_paths_are_not() {
        let headers = [("host".to_string(), "example.com".to_string())];
        let mut api = s3_creds();
        api.service = "execute-api".into();
        let url = "https://example.com/a%20b";
        let api_sig = sign_request("GET", url, &headers, b"", &api, "20240101T000000Z").expect("sign");
        let s3_sig = sign_request("GET", url, &headers, b"", &s3_creds(), "20240101T000000Z").expect("sign");
        // Same URL, different canonical path rule, so different signatures.
        assert_ne!(api_sig.authorization, s3_sig.authorization);
        assert_eq!(canonical_path("/a%20b", "execute-api"), "/a%2520b");
        assert_eq!(canonical_path("/a%20b", "s3"), "/a%20b");
        assert_eq!(canonical_path("", "s3"), "/");
    }

    #[test]
    fn s3_requests_sign_the_content_hash_header() {
        let signed = sign_request(
            "GET",
            "https://example.com/k",
            &[("host".to_string(), "example.com".to_string())],
            b"",
            &s3_creds(),
            "20240101T000000Z",
        )
        .expect("sign");
        assert!(
            signed.authorization.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"),
            "{}",
            signed.authorization
        );
    }
```

- [ ] **Step 4: Run to verify failure**

Run: `cargo test -j4 -p rocket-http aws_sig`
Expected: FAIL to compile (`sign_request_with_payload_hash`, `UNSIGNED_PAYLOAD`, `canonical_query`, `canonical_path` not found; `hex_sha256` is private).

- [ ] **Step 5: Implement the `aws_sig` changes**

In `crates/rocket-http/src/aws_sig.rs`:

1. Make the hash helper public and add the marker:

```rust
/// Payload hash for a body that cannot be hashed up front, such as a streamed multipart body.
/// AWS accepts it over TLS for services that support unsigned payloads, such as S3.
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// Compute hex-encoded SHA-256 digest.
pub fn hex_sha256(data: &[u8]) -> String {
```

2. Rename the body of `sign_request` into `sign_request_with_payload_hash` taking `payload_hash: &str` instead of `body: &[u8]`, and make `sign_request` a thin wrapper:

```rust
pub fn sign_request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    credentials: &AwsCredentials,
    timestamp: &str,
) -> Result<SignedHeaders, String> {
    sign_request_with_payload_hash(method, url, headers, &hex_sha256(body), credentials, timestamp)
}
```

3. Inside the new function remove the `let payload_hash = hex_sha256(body);` line (use the parameter), return `x_amz_content_sha256: payload_hash.to_string()`, and:
   - after pushing `x-amz-date` and the optional token, add for S3: `if credentials.service == "s3" { canonical_headers_map.push(("x-amz-content-sha256".to_string(), payload_hash.to_string())); }` (S3 requires the content hash header to be signed; the executor always sends it).
   - replace the canonical URI block with `let canonical_uri = canonical_path(parsed_url.path(), &credentials.service);`.
   - replace the whole canonical query block (from `let mut query_pairs` to the `join("&")`) with `let canonical_querystring = canonical_query(&parsed_url);`.

4. Add the path helper:

```rust
/// Canonical query string: every key and value percent-encoded, sorted by encoded key.
fn canonical_query(url: &reqwest::Url) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| {
            (
                urlencoding::encode(&k).into_owned(),
                urlencoding::encode(&v).into_owned(),
            )
        })
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Canonical URI for the signature. The URL path is already percent-encoded once. Every service
/// except S3 signs the path encoded a second time; S3 signs it as it is.
fn canonical_path(path: &str, service: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    if service == "s3" {
        return path.to_string();
    }
    path.split('/')
        .map(|segment| urlencoding::encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
```

`urlencoding` is already a dependency of `rocket-http`.

- [ ] **Step 6: Run to verify the `aws_sig` tests pass**

Run: `cargo test -j4 -p rocket-http aws_sig`
Expected: PASS (including the vector, which must still pass: it has no query, a `/` path and a non-S3 service whose `/` path stays `/`).

- [ ] **Step 7: Write the failing credentials tests**

Create `crates/rocket-infra/src/aws_profile.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
[default]
aws_access_key_id = AKIADEFAULT
aws_secret_access_key = defsecret

# a comment
; another comment
[prod]
aws_access_key_id=AKIAPROD
aws_secret_access_key = prodsecret
aws_session_token = tok

[nokeys]
region = eu-west-1
";

    #[test]
    fn reads_a_named_profile() {
        let c = parse_credentials(FILE, "prod").expect("prod");
        assert_eq!(c.access_key, "AKIAPROD");
        assert_eq!(c.secret_key, "prodsecret");
        assert_eq!(c.session_token.as_deref(), Some("tok"));
        let d = parse_credentials(FILE, "default").expect("default");
        assert_eq!(d.session_token, None);
    }

    #[test]
    fn a_missing_profile_or_one_without_keys_is_none() {
        assert!(parse_credentials(FILE, "staging").is_none());
        assert!(parse_credentials(FILE, "nokeys").is_none());
    }

    #[test]
    fn accepts_the_config_file_style_profile_header() {
        let text = "[profile dev]\naws_access_key_id = A\naws_secret_access_key = S\n";
        assert!(parse_credentials(text, "dev").is_some());
    }

    #[test]
    fn load_profile_from_names_the_profile_but_never_the_keys_in_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("credentials");
        std::fs::write(&path, FILE).expect("write");
        let ok = load_profile_from(&path, "prod").expect("prod");
        assert_eq!(ok.access_key, "AKIAPROD");
        let err = load_profile_from(&path, "staging").expect_err("missing profile");
        let text = err.to_string();
        assert!(text.contains("staging"), "{text}");
        assert!(!text.contains("prodsecret") && !text.contains("defsecret"), "{text}");
        let gone = load_profile_from(&dir.path().join("nope"), "prod").expect_err("no file");
        assert!(gone.to_string().contains("credentials file"), "{gone}");
    }

    #[test]
    fn inline_keys_win_over_a_profile_name() {
        let c = resolve_credentials("AK", "SK", "us-east-1", "s3", Some(""), Some("prod")).expect("inline");
        assert_eq!(c.access_key, "AK");
        assert_eq!(c.session_token, None, "an empty token is no token");
    }

    #[test]
    fn missing_credentials_are_an_error() {
        let err = resolve_credentials("", "", "us-east-1", "s3", None, None).expect_err("no creds");
        assert!(err.to_string().contains("access key"), "{err}");
        let half = resolve_credentials("AK", "", "us-east-1", "s3", None, None).expect_err("half");
        assert!(half.to_string().contains("secret key"), "{half}");
    }
}
```

- [ ] **Step 8: Run to verify failure**

Run: `cargo test -j4 -p rocket-infra aws_profile`
Expected: FAIL to compile (module not declared, functions missing).

- [ ] **Step 9: Implement `aws_profile.rs`**

Prepend to `crates/rocket-infra/src/aws_profile.rs`:

```rust
//! AWS credentials for SigV4: either typed into the request, or read from a named profile of
//! the shared credentials file (`~/.aws/credentials`, or `AWS_SHARED_CREDENTIALS_FILE`).
//! Keys and tokens never appear in an error message.

use std::path::{Path, PathBuf};

use rocket_http::aws_sig::AwsCredentials;
use rocket_shared::error::{DomainError, DomainResult};

/// The three values a profile can supply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsProfileCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

/// Reads `profile` from the text of a credentials file. Both `[name]` and the config-file style
/// `[profile name]` headers are accepted. A profile without both an access key and a secret key
/// is treated as absent.
pub fn parse_credentials(text: &str, profile: &str) -> Option<AwsProfileCredentials> {
    let mut in_profile = false;
    let mut access = None;
    let mut secret = None;
    let mut token = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = header.trim();
            let name = name.strip_prefix("profile ").map_or(name, str::trim);
            in_profile = name == profile;
            continue;
        }
        if !in_profile {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "aws_access_key_id" => access = Some(value.to_string()),
            "aws_secret_access_key" => secret = Some(value.to_string()),
            "aws_session_token" if !value.is_empty() => token = Some(value.to_string()),
            _ => {}
        }
    }
    match (access, secret) {
        (Some(a), Some(s)) if !a.is_empty() && !s.is_empty() => Some(AwsProfileCredentials {
            access_key: a,
            secret_key: s,
            session_token: token,
        }),
        _ => None,
    }
}

/// Path of the shared credentials file, or `None` when no home directory is known.
pub fn credentials_file_path() -> Option<PathBuf> {
    std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".aws").join("credentials")))
}

pub fn load_profile_from(path: &Path, profile: &str) -> DomainResult<AwsProfileCredentials> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        DomainError::InvalidInput(format!(
            "Cannot read the AWS credentials file {}: {e}",
            path.display()
        ))
    })?;
    parse_credentials(&text, profile).ok_or_else(|| {
        DomainError::InvalidInput(format!(
            "AWS profile {profile} was not found in {} or has no access key and secret key",
            path.display()
        ))
    })
}

pub fn load_profile(profile: &str) -> DomainResult<AwsProfileCredentials> {
    let path = credentials_file_path().ok_or_else(|| {
        DomainError::InvalidInput(
            "Cannot find the AWS credentials file: no home directory is known".into(),
        )
    })?;
    load_profile_from(&path, profile)
}

/// The credentials to sign with. Keys typed into the request win. With no keys typed and a
/// profile name set, the profile is read from the credentials file. Anything else is an error:
/// signing with empty keys can never succeed, so it must not be attempted.
pub fn resolve_credentials(
    access_key: &str,
    secret_key: &str,
    region: &str,
    service: &str,
    session_token: Option<&str>,
    profile_name: Option<&str>,
) -> DomainResult<AwsCredentials> {
    let profile = profile_name.map(str::trim).filter(|p| !p.is_empty());
    let inline = !access_key.trim().is_empty() || !secret_key.trim().is_empty();
    let (access_key, secret_key, session_token) = match profile {
        Some(name) if !inline => {
            let c = load_profile(name)?;
            (c.access_key, c.secret_key, c.session_token)
        }
        _ => (
            access_key.trim().to_string(),
            secret_key.trim().to_string(),
            session_token
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string),
        ),
    };
    if access_key.is_empty() {
        return Err(DomainError::InvalidInput(
            "AWS Signature V4 needs an access key, or a profile name".into(),
        ));
    }
    if secret_key.is_empty() {
        return Err(DomainError::InvalidInput(
            "AWS Signature V4 needs a secret key, or a profile name".into(),
        ));
    }
    Ok(AwsCredentials {
        access_key,
        secret_key,
        region: region.trim().to_string(),
        service: service.trim().to_string(),
        session_token,
    })
}
```

Add `mod aws_profile;` to `crates/rocket-infra/src/lib.rs` (alphabetical, after `pub mod acp_agent_client;`).

- [ ] **Step 10: Run to verify the credentials tests pass**

Run: `cargo test -j4 -p rocket-infra aws_profile`
Expected: PASS (6 tests).

- [ ] **Step 11: Write the failing executor tests**

Append to `crates/rocket-infra/src/reqwest_executor.rs`:

```rust
#[cfg(test)]
mod sigv4_tests {
    use super::*;
    use rocket_http::aws_sig::{hex_sha256, sign_request_with_payload_hash, AwsCredentials};
    use rocket_shared::types::{FormDataEntry, FormDataType, HttpMethod};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn auth() -> Auth {
        Auth::AwsSigV4 {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "SECRET".into(),
            region: "us-east-1".into(),
            service: "execute-api".into(),
            session_token: None,
            profile_name: None,
        }
    }

    fn creds() -> AwsCredentials {
        AwsCredentials {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "SECRET".into(),
            region: "us-east-1".into(),
            service: "execute-api".into(),
            session_token: None,
        }
    }

    async fn server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    }

    fn header(req: &wiremock::Request, name: &str) -> String {
        req.headers
            .get(name)
            .map(|v| v.to_str().unwrap_or_default().to_string())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn signs_the_actual_body_and_host_with_port() {
        let server = server().await;
        let url = format!("{}/orders", server.uri());
        let body = "{\"id\":1}";
        let mut req = HttpRequest::new(HttpMethod::Post, url.clone());
        req.auth = auth();
        req.body = Some(Body {
            mode: BodyMode::Json,
            content: Some(body.into()),
            form_data: None,
            file_path: None,
        });
        ReqwestExecutor::new().execute(&req).await.expect("send");

        let received = server.received_requests().await.expect("recorded");
        let sent = &received[0];
        assert_eq!(header(sent, "x-amz-content-sha256"), hex_sha256(body.as_bytes()));

        // Recompute the signature the way a verifying server would: with the real host and port
        // and the hash of the body that arrived.
        let host = format!(
            "{}:{}",
            sent.url.host_str().expect("host"),
            sent.url.port().expect("port")
        );
        let expected = sign_request_with_payload_hash(
            "POST",
            sent.url.as_str(),
            &[("host".to_string(), host)],
            &hex_sha256(&sent.body),
            &creds(),
            &header(sent, "x-amz-date"),
        )
        .expect("sign");
        assert_eq!(header(sent, "authorization"), expected.authorization);
    }

    #[tokio::test]
    async fn a_request_without_a_body_signs_the_empty_hash() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/x", server.uri()));
        req.auth = auth();
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let received = server.received_requests().await.expect("recorded");
        assert_eq!(header(&received[0], "x-amz-content-sha256"), hex_sha256(b""));
    }

    #[tokio::test]
    async fn streamed_body_is_signed_as_unsigned_payload() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/upload", server.uri()));
        req.auth = auth();
        req.body = Some(Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "a".into(),
                value: "1".into(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        });
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let received = server.received_requests().await.expect("recorded");
        assert_eq!(header(&received[0], "x-amz-content-sha256"), "UNSIGNED-PAYLOAD");
    }

    #[tokio::test]
    async fn missing_credentials_are_an_error() {
        let mut req = HttpRequest::new(HttpMethod::Get, "http://127.0.0.1:1/x");
        req.auth = Auth::AwsSigV4 {
            access_key: String::new(),
            secret_key: String::new(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: None,
            profile_name: None,
        };
        let err = ReqwestExecutor::new().execute(&req).await.expect_err("must fail");
        assert!(err.to_string().contains("access key"), "{err}");
    }
}
```

- [ ] **Step 12: Run to verify failure**

Run: `cargo test -j4 -p rocket-infra sigv4_tests`
Expected: FAIL: the body hash is the empty hash, the signature was made over `host` without the port, and `missing_credentials_are_an_error` sends a request instead of failing.

- [ ] **Step 13: Sign after the body is applied**

In `crates/rocket-infra/src/reqwest_executor.rs`:

1. Replace the whole `Auth::AwsSigV4 { .. } => { ... }` arm of `apply_auth` with a no-op that matches `Auth::OAuth1`:

```rust
        Auth::AwsSigV4 { .. } => {
            // Signed in `execute` once the body is known; see `apply_aws_sigv4`.
        }
```

   The `method` parameter of `apply_auth` is then unused: remove it from the signature and from the call in `execute` (the call becomes `apply_auth(start_builder(url), &request.auth, &request.options.client_certificates)`), and from any test that calls `apply_auth` directly.

2. Replace the `let mut response = if let Auth::OAuth1(oauth) = &request.auth { ... } else { builder.send().await }.map_err(...)?;` expression with a `match` that also signs SigV4:

```rust
        let mut response = match &request.auth {
            // OAuth1 and AWS signing both sign the final request, so they wait until the body is applied.
            Auth::OAuth1(oauth) => {
                let mut built = builder.build().map_err(|e| {
                    DomainError::Internal(format!("Cannot build request for signing: {e}"))
                })?;
                apply_oauth1(&mut built, &request.method, oauth)?;
                client.execute(built).await
            }
            Auth::AwsSigV4 {
                access_key,
                secret_key,
                region,
                service,
                session_token,
                profile_name,
            } => {
                let creds = crate::aws_profile::resolve_credentials(
                    access_key,
                    secret_key,
                    region,
                    service,
                    session_token.as_deref(),
                    profile_name.as_deref(),
                )?;
                let mut built = builder.build().map_err(|e| {
                    DomainError::Internal(format!("Cannot build request for signing: {e}"))
                })?;
                apply_aws_sigv4(&mut built, &creds)?;
                client.execute(built).await
            }
            _ => builder.send().await,
        }
        .map_err(|e| DomainError::Http(e.to_string()))?;
```

3. Add the signing function next to `apply_oauth1`:

```rust
/// Signs a built request with AWS Signature Version 4 and sets the signing headers.
///
/// The payload hash covers the real body. A streamed body (multipart) cannot be hashed up
/// front, so it is signed as `UNSIGNED-PAYLOAD`. The signed `host` includes the port when the
/// URL has a non-default one, which is what the server receives.
fn apply_aws_sigv4(
    req: &mut reqwest::Request,
    creds: &rocket_http::aws_sig::AwsCredentials,
) -> DomainResult<()> {
    use rocket_http::aws_sig::{
        hex_sha256, sign_request_with_payload_hash, UNSIGNED_PAYLOAD,
    };

    let payload_hash = match req.body().map(|b| b.as_bytes()) {
        None => hex_sha256(&[]),
        Some(Some(bytes)) => hex_sha256(bytes),
        Some(None) => UNSIGNED_PAYLOAD.to_string(),
    };
    let url = req.url().clone();
    let host = match (url.host_str(), url.port()) {
        (Some(h), Some(p)) => format!("{h}:{p}"),
        (Some(h), None) => h.to_string(),
        _ => {
            return Err(DomainError::InvalidInput(
                "AWS Signature V4 needs a URL with a host".into(),
            ))
        }
    };
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let signed = sign_request_with_payload_hash(
        req.method().as_str(),
        url.as_str(),
        &[("host".to_string(), host)],
        &payload_hash,
        creds,
        &timestamp,
    )
    .map_err(|e| DomainError::Internal(format!("AWS signing failed: {e}")))?;

    let headers = req.headers_mut();
    set_header(headers, "authorization", &signed.authorization)?;
    set_header(headers, "x-amz-date", &signed.x_amz_date)?;
    set_header(headers, "x-amz-content-sha256", &signed.x_amz_content_sha256)?;
    if let Some(token) = &signed.x_amz_security_token {
        set_header(headers, "x-amz-security-token", token)?;
    }
    Ok(())
}

fn set_header(
    headers: &mut reqwest::header::HeaderMap,
    name: &'static str,
    value: &str,
) -> DomainResult<()> {
    let value = reqwest::header::HeaderValue::from_str(value)
        .map_err(|e| DomainError::InvalidInput(format!("Invalid {name} header value: {e}")))?;
    headers.insert(reqwest::header::HeaderName::from_static(name), value);
    Ok(())
}
```

- [ ] **Step 14: Run to verify the executor passes**

Run: `cargo test -j4 -p rocket-infra sigv4_tests reqwest_executor`
Expected: PASS. If a pre-existing test referenced the old SigV4 arm of `apply_auth`, update it to go through `execute` as above.

- [ ] **Step 15: Resolve the profile name's placeholders in the backend**

In `crates/rocket-app/src/execution_service.rs` `resolve_auth`, change `profile_name,` in the `Auth::AwsSigV4` arm's output to `profile_name: profile_name.map(&r),`. Add to the `tests` module:

```rust
    #[test]
    fn resolve_auth_resolves_the_aws_profile_name() {
        let mut vars = std::collections::HashMap::new();
        vars.insert("profile".to_string(), "prod".to_string());
        let out = resolve_auth(
            Auth::AwsSigV4 {
                access_key: String::new(),
                secret_key: String::new(),
                region: "us-east-1".into(),
                service: "s3".into(),
                session_token: None,
                profile_name: Some("{{profile}}".into()),
            },
            &vars,
        );
        assert!(matches!(
            out,
            Auth::AwsSigV4 { profile_name: Some(ref n), .. } if n == "prod"
        ));
    }
```

Run: `cargo test -j4 -p rocket-app resolve_auth_resolves_the_aws_profile_name`
Expected: PASS after the change (FAIL before: the name stays `{{profile}}`).

- [ ] **Step 16: Write the failing frontend tests**

Append to `src/lib/__tests__/execute-request.test.ts` inside an existing or new `describe('toApiAuth', ...)` (extend its import with `AuthState` from `@/types/pane-types` if needed):

```ts
describe('toApiAuth aws-sig-v4', () => {
  it('sends the credentials instead of falling back to none', () => {
    const auth = {
      authType: 'aws-sig-v4',
      awsSigV4: {
        accessKey: '{{k}}',
        secretKey: 's',
        region: 'us-east-1',
        service: 's3',
        sessionToken: '',
        profileName: 'prod',
      },
    } as const;
    expect(toApiAuth(auth, (s) => s.replace('{{k}}', 'AKIA'))).toEqual({
      authType: 'aws-sig-v4',
      accessKey: 'AKIA',
      secretKey: 's',
      region: 'us-east-1',
      service: 's3',
      sessionToken: undefined,
      profileName: 'prod',
    });
  });

  it('keeps a session token and omits an empty profile name', () => {
    const auth = {
      authType: 'aws-sig-v4',
      awsSigV4: {
        accessKey: 'a',
        secretKey: 's',
        region: 'r',
        service: 'x',
        sessionToken: 'tok',
        profileName: '',
      },
    } as const;
    const out = toApiAuth(auth) as unknown as Record<string, unknown>;
    expect(out.sessionToken).toBe('tok');
    expect(out.profileName).toBeUndefined();
  });
});
```

Append to `src/components/request/__tests__/AuthEditor.test.tsx`:

```tsx
describe('AuthEditor for aws-sig-v4', () => {
  it('edits the profile name and keeps the other fields', () => {
    const onChange = vi.fn();
    render(
      <AuthEditor
        auth={{
          authType: 'aws-sig-v4',
          awsSigV4: { accessKey: 'a', secretKey: 's', region: 'r', service: 'x', sessionToken: '' },
        }}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByLabelText('Profile name'), { target: { value: 'prod' } });
    expect(onChange).toHaveBeenLastCalledWith({
      authType: 'aws-sig-v4',
      awsSigV4: {
        accessKey: 'a',
        secretKey: 's',
        region: 'r',
        service: 'x',
        sessionToken: '',
        profileName: 'prod',
      },
    });
  });
});
```

- [ ] **Step 17: Run to verify failure**

Run: `yarn test execute-request AuthEditor`
Expected: the three new tests FAIL (`toApiAuth` returns `{ authType: 'none' }`; no `Profile name` field).

- [ ] **Step 18: Implement the frontend**

In `src/lib/execute-request.ts` replace the `case 'aws-sig-v4':` block in `toApiAuth` (lines 136-139) with:

```ts
    case 'aws-sig-v4': {
      const a = auth.awsSigV4;
      // Option<String> on the Rust side: omit an empty value rather than send ''.
      return {
        authType: 'aws-sig-v4',
        accessKey: resolve(a?.accessKey ?? ''),
        secretKey: resolve(a?.secretKey ?? ''),
        region: resolve(a?.region ?? ''),
        service: resolve(a?.service ?? ''),
        sessionToken: resolve(a?.sessionToken ?? '') || undefined,
        profileName: resolve(a?.profileName ?? '') || undefined,
      } as unknown as Auth;
    }
```

In `src/components/request/AuthEditor.tsx`, inside the Credentials card of the `aws-sig-v4` block, after the Session Token `<div>`, add:

```tsx
              <div>
                <Label className='mb-1 block'>Profile name</Label>
                <SingleLineEditor
                  aria-label='Profile name'
                  className='text-sm'
                  placeholder='(optional) default'
                  value={auth.awsSigV4.profileName ?? ''}
                  onChange={(newVal) => patchAWS({ profileName: newVal })}
                  variableContext={variableContext}
                  onNavigateToSource={onNavigateToSource}
                />
                <p className='mt-1 text-xs text-muted-foreground'>
                  Reads the keys from this profile of ~/.aws/credentials when the access and secret
                  key above are empty.
                </p>
              </div>
```

In `src/types/pane-types.ts` replace the `profileName` doc comment (line 354) with `/** Profile of the shared AWS credentials file, used when the keys above are empty. */`. In `RequestPanel.tsx` `handleCopyAsCurl` the warning text stays as it is: the cURL copy still cannot be pre-signed.

- [ ] **Step 19: Run all checks**

Run: `yarn test execute-request AuthEditor RequestPanel`
Expected: PASS.

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

Run: `cargo test -j4 -p rocket-http aws_sig && cargo test -j4 -p rocket-infra sigv4_tests aws_profile && cargo test -j4 -p rocket-app resolve_auth && cargo check -j4 --workspace --tests`
Expected: PASS.

Manual check: against a real endpoint (an API Gateway or S3 `GET` with a session token), send the request from the UI with SigV4 selected and confirm a 200 rather than `403 SignatureDoesNotMatch`; repeat with a JSON `POST` body and with only a profile name set.

- [ ] **Step 20: Commit**

Use the `dev-workflow-skills:1-git-commit` skill, staging by explicit path (every file listed under Files for this task). Suggested subject: `fix(http): sign the real body and host for AWS SigV4 and honor the profile`.

---

## Next Plan

[Plan 03: Multipart, request settings and NTLM](2026-10-05-protocol-parity-plan-03-rest-multipart-settings-ntlm.md). It builds on this plan's `build_client_with_identity` signature (Task 1 here added the `cookies` parameter) and its `HttpResponse` changes. Chain to it automatically when this one finishes.
