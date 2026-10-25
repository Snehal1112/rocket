//! Pure helpers for the script cookie API: URL scoping, cookie building and entry projection.

use crate::cookie::{
    cookies_for_request, default_cookie_path, domain_matches, normalize_same_site, Cookie,
    CookieJar, MAX_COOKIE_AGE_SECS,
};
use chrono::{DateTime, Duration, Utc};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

/// The host, path and scheme a script cookie call is scoped to.
#[derive(Debug, Clone, PartialEq)]
pub struct UrlScope {
    pub host: String,
    pub path: String,
    pub https: bool,
}

impl UrlScope {
    /// Reads the host, path and scheme from `url`. A URL without a host is invalid input.
    pub fn parse(url: &str) -> DomainResult<UrlScope> {
        let parsed = url::Url::parse(url)
            .map_err(|e| DomainError::InvalidInput(format!("Invalid URL: {e}")))?;
        let host = parsed
            .host_str()
            .filter(|h| !h.is_empty())
            .ok_or_else(|| DomainError::InvalidInput("The URL has no host".to_string()))?;
        Ok(UrlScope {
            host: host.to_ascii_lowercase(),
            path: parsed.path().to_string(),
            https: parsed.scheme() == "https",
        })
    }
}

/// A cookie as a script sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieEntry {
    pub id: String,
    pub key: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: Option<String>,
    pub max_age: Option<i64>,
    pub same_site: Option<String>,
}

/// A cookie a script asks to store.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CookieInput {
    pub key: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: Option<bool>,
    pub http_only: Option<bool>,
    pub expires: Option<String>,
    pub max_age: Option<i64>,
    pub same_site: Option<String>,
}

/// The stable id of a cookie: `domain|path|name`.
pub fn cookie_id(cookie: &Cookie) -> String {
    format!("{}|{}|{}", cookie.domain, cookie.path, cookie.name)
}

/// Projects a stored cookie to a script entry. `max_age` is the whole seconds left, if any.
pub fn to_entry(cookie: &Cookie, now: DateTime<Utc>) -> CookieEntry {
    let max_age = cookie
        .expires
        .as_deref()
        .and_then(|e| DateTime::parse_from_rfc3339(e).ok())
        .map(|e| (e.with_timezone(&Utc) - now).num_seconds())
        .filter(|secs| *secs > 0);
    CookieEntry {
        id: cookie_id(cookie),
        key: cookie.name.clone(),
        value: cookie.value.clone(),
        domain: cookie.domain.clone(),
        path: cookie.path.clone(),
        secure: cookie.secure,
        http_only: cookie.http_only,
        expires: cookie.expires.clone(),
        max_age,
        same_site: cookie.same_site.clone(),
    }
}

fn invalid(message: &str) -> DomainError {
    DomainError::InvalidInput(message.to_string())
}

/// The most bytes a cookie name and value may use together.
const MAX_NAME_VALUE_BYTES: usize = 4096;

/// True for a character of an RFC 7230 token: visible ASCII except separators.
fn is_token_char(c: char) -> bool {
    c.is_ascii_graphic() && !"\"(),/:;<=>?@[\\]{}".contains(c)
}

/// Builds the cookie a script asked for, scoped to `scope`.
///
/// Without a `domain` the cookie is host-only on the URL host. A `domain` must cover the host.
/// `max_age` wins over `expires` and is capped at 400 days; zero or negative is rejected.
pub fn build_cookie(
    scope: &UrlScope,
    input: &CookieInput,
    now: DateTime<Utc>,
) -> DomainResult<Cookie> {
    let name = input.key.trim();
    if name.is_empty() {
        return Err(invalid("The cookie key cannot be empty"));
    }
    if !name.chars().all(is_token_char) {
        return Err(invalid(
            "The cookie key has a character that is not allowed",
        ));
    }
    if input.value.chars().any(|c| c.is_control() || c == ';') {
        return Err(invalid(
            "The cookie value cannot contain a control character or a semicolon",
        ));
    }
    if name.len() + input.value.len() > MAX_NAME_VALUE_BYTES {
        return Err(invalid("The cookie key and value are too long"));
    }
    let host_prefixed = name.starts_with("__Host-");
    let is_ip = scope.host.parse::<IpAddr>().is_ok();

    let domain_attr = input
        .domain
        .as_deref()
        .map(|d| d.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|d| !d.is_empty());
    if host_prefixed && domain_attr.is_some() {
        return Err(invalid("A __Host- cookie cannot have a domain"));
    }
    let domain = match domain_attr {
        Some(d) => {
            if !domain_matches(&format!(".{d}"), &scope.host) {
                return Err(invalid("The cookie domain does not match the URL host"));
            }
            if is_ip || (d == scope.host && !d.contains('.')) {
                // An IP address or a one-label host can only be host-only.
                d
            } else if !d.contains('.') {
                return Err(invalid("The cookie domain is too broad"));
            } else {
                format!(".{d}")
            }
        }
        None => scope.host.clone(),
    };

    let secure = input.secure.unwrap_or(false);
    if host_prefixed && !secure {
        return Err(invalid("A __Host- cookie must be secure"));
    }
    let path = match input
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        Some(p) if !p.starts_with('/') => return Err(invalid("The cookie path must start with /")),
        Some(p) => p.to_string(),
        None if host_prefixed => "/".to_string(),
        None => default_cookie_path(&scope.path),
    };
    if host_prefixed && path != "/" {
        return Err(invalid("A __Host- cookie must have the path /"));
    }

    let same_site = match input.same_site.as_deref() {
        Some(s) => Some(normalize_same_site(s).ok_or_else(|| invalid("Invalid sameSite value"))?),
        None => None,
    };

    let expires = match (input.max_age, input.expires.as_deref()) {
        (Some(secs), _) if secs <= 0 => {
            return Err(invalid("maxAge must be positive, use remove to delete"))
        }
        (Some(secs), _) => Some(now + Duration::seconds(secs.min(MAX_COOKIE_AGE_SECS))),
        (None, Some(text)) => Some(
            DateTime::parse_from_rfc3339(text)
                .map_err(|_| invalid("expires must be an RFC 3339 date"))?
                .with_timezone(&Utc),
        ),
        (None, None) => None,
    };

    Ok(Cookie {
        name: name.to_string(),
        value: input.value.clone(),
        domain,
        path,
        secure,
        http_only: input.http_only.unwrap_or(false),
        expires: expires.map(|at| at.to_rfc3339()),
        same_site,
    })
}

/// The unexpired cookies that match `scope`, longest path first.
pub fn matching_cookies(jars: &[CookieJar], scope: &UrlScope, now: DateTime<Utc>) -> Vec<Cookie> {
    cookies_for_request(jars, &scope.host, &scope.path, scope.https, now)
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0)
            .single()
            .expect("valid date")
    }

    fn scope() -> UrlScope {
        UrlScope::parse("https://api.example.com/v1/items").expect("scope")
    }

    fn build(key: &str, value: &str) -> DomainResult<Cookie> {
        let input = CookieInput {
            key: key.into(),
            value: value.into(),
            ..Default::default()
        };
        build_cookie(&scope(), &input, now())
    }

    fn rejected(key: &str, value: &str) {
        match build(key, value) {
            Err(DomainError::InvalidInput(_)) => {}
            other => panic!("expected InvalidInput for {key:?}, got {other:?}"),
        }
    }

    #[test]
    fn value_rejects_control_characters_and_semicolon() {
        for bad in [
            "a\rb",
            "a\nb",
            "a\0b",
            "a\x7fb",
            "a\tb",
            "abc\n",
            "x; admin=1",
        ] {
            rejected("sid", bad);
        }
    }

    #[test]
    fn value_keeps_equals_space_quotes_and_non_ascii() {
        let c = build("sid", "a=b \"c\" caf\u{e9}").expect("cookie");
        assert_eq!(c.value, "a=b \"c\" caf\u{e9}");
    }

    #[test]
    fn name_must_be_a_token() {
        for bad in [
            "a=b", "a;b", "a b", "a\nb", "a\tb", "a,b", "a(b", "a\"b", "a/b", "a[b", "a{b",
            "a\x7fb",
        ] {
            rejected(bad, "1");
        }
        assert!(build("ok-name_1.x", "1").is_ok());
    }

    #[test]
    fn name_plus_value_is_capped_at_4096_bytes() {
        assert!(build("n", &"v".repeat(4095)).is_ok());
        rejected("n", &"v".repeat(4096));
    }

    #[test]
    fn errors_do_not_contain_the_value_or_an_invalid_name() {
        let e = build("sid", "s3cr3t-token\n")
            .expect_err("rejected")
            .to_string();
        assert!(!e.contains("s3cr3t"), "{e}");
        let e = build("bad;name", "1").expect_err("rejected").to_string();
        assert!(!e.contains("bad;name"), "{e}");
        let e = build("n", &"s3cr3t".repeat(1000))
            .expect_err("rejected")
            .to_string();
        assert!(!e.contains("s3cr3t"), "{e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0)
            .single()
            .expect("valid date")
    }

    fn scope() -> UrlScope {
        UrlScope::parse("https://api.example.com/v1/items").expect("scope")
    }

    fn input(key: &str) -> CookieInput {
        CookieInput {
            key: key.into(),
            value: "1".into(),
            ..Default::default()
        }
    }

    fn invalid<T: std::fmt::Debug>(r: DomainResult<T>) {
        match r {
            Err(DomainError::InvalidInput(_)) => {}
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn url_scope_parses_host_path_and_scheme() {
        let s = scope();
        assert_eq!(s.host, "api.example.com");
        assert_eq!(s.path, "/v1/items");
        assert!(s.https);
        let http = UrlScope::parse("http://Example.com:8080").expect("scope");
        assert_eq!(http.host, "example.com");
        assert_eq!(http.path, "/");
        assert!(!http.https);
    }

    #[test]
    fn url_scope_rejects_a_url_without_a_host() {
        invalid(UrlScope::parse("not a url"));
        invalid(UrlScope::parse("file:///tmp/x"));
    }

    #[test]
    fn build_cookie_without_domain_is_host_only_with_the_default_path() {
        let c = build_cookie(&scope(), &input("sid"), now()).expect("cookie");
        assert_eq!(c.domain, "api.example.com");
        assert_eq!(c.path, "/v1");
        assert_eq!(c.name, "sid");
        assert_eq!(c.expires, None);
        assert!(!c.secure && !c.http_only);
    }

    #[test]
    fn build_cookie_strips_a_leading_dot_and_rejects_a_domain_that_does_not_match() {
        let mut i = input("sid");
        i.domain = Some(".example.com".into());
        let c = build_cookie(&scope(), &i, now()).expect("cookie");
        assert_eq!(c.domain, ".example.com");
        i.domain = Some("other.com".into());
        invalid(build_cookie(&scope(), &i, now()));
        i.domain = Some("com".into());
        invalid(build_cookie(&scope(), &i, now()));
    }

    #[test]
    fn build_cookie_max_age_wins_over_expires_and_is_capped_at_400_days() {
        let mut i = input("sid");
        i.max_age = Some(60);
        i.expires = Some("2030-01-01T00:00:00Z".into());
        let c = build_cookie(&scope(), &i, now()).expect("cookie");
        assert_eq!(
            c.expires,
            Some((now() + Duration::seconds(60)).to_rfc3339())
        );
        i.max_age = Some(i64::MAX / 2);
        let c = build_cookie(&scope(), &i, now()).expect("cookie");
        assert_eq!(c.expires, Some((now() + Duration::days(400)).to_rfc3339()));
        let mut e = input("sid");
        e.expires = Some("2030-01-01T00:00:00Z".into());
        let c = build_cookie(&scope(), &e, now()).expect("cookie");
        assert_eq!(c.expires, Some("2030-01-01T00:00:00+00:00".to_string()));
        e.expires = Some("tomorrow".into());
        invalid(build_cookie(&scope(), &e, now()));
    }

    #[test]
    fn build_cookie_rejects_max_age_zero_and_negative() {
        let mut i = input("sid");
        i.max_age = Some(0);
        invalid(build_cookie(&scope(), &i, now()));
        i.max_age = Some(-5);
        invalid(build_cookie(&scope(), &i, now()));
    }

    #[test]
    fn host_prefixed_cookie_needs_secure_and_no_domain_and_root_path() {
        let mut i = input("__Host-sid");
        i.secure = Some(true);
        i.domain = Some("example.com".into());
        invalid(build_cookie(&scope(), &i, now()));
        let mut i = input("__Host-sid");
        invalid(build_cookie(&scope(), &i, now()));
        i.secure = Some(true);
        i.path = Some("/x".into());
        invalid(build_cookie(&scope(), &i, now()));
        i.path = None;
        let c = build_cookie(&scope(), &i, now()).expect("cookie");
        assert_eq!(c.path, "/");
        assert!(c.secure);
    }

    #[test]
    fn to_entry_derives_max_age_and_id() {
        let mut i = input("sid");
        i.max_age = Some(3600);
        let c = build_cookie(&scope(), &i, now()).expect("cookie");
        let e = to_entry(&c, now());
        assert_eq!(e.max_age, Some(3600));
        assert_eq!(e.id, "api.example.com|/v1|sid");
        assert_eq!(e.key, "sid");
        let session = build_cookie(&scope(), &input("sid"), now()).expect("cookie");
        assert_eq!(to_entry(&session, now()).max_age, None);
        assert_eq!(cookie_id(&session), "api.example.com|/v1|sid");
    }

    #[test]
    fn cookie_entry_serializes_camel_case_keys() {
        let c = build_cookie(&scope(), &input("sid"), now()).expect("cookie");
        let v = serde_json::to_value(to_entry(&c, now())).expect("json");
        assert!(v.get("httpOnly").is_some());
        assert!(v.get("maxAge").is_some());
        assert!(v.get("sameSite").is_some());
    }

    #[test]
    fn cookie_input_reads_partial_json() {
        let i: CookieInput = serde_json::from_str(r#"{"key":"a","value":"1"}"#).expect("json");
        assert_eq!(i.key, "a");
        assert_eq!(i.value, "1");
        assert_eq!(i.domain, None);
        assert_eq!(i.max_age, None);
    }

    #[test]
    fn build_cookie_keeps_flags_and_same_site() {
        let mut i = input("sid");
        i.secure = Some(true);
        i.http_only = Some(true);
        i.same_site = Some("lax".into());
        let c = build_cookie(&scope(), &i, now()).expect("cookie");
        assert!(c.secure && c.http_only);
        assert_eq!(c.same_site.as_deref(), Some("Lax"));
        i.same_site = Some("bogus".into());
        invalid(build_cookie(&scope(), &i, now()));
        invalid(build_cookie(&scope(), &input(" "), now()));
    }

    #[test]
    fn matching_cookies_applies_domain_path_secure_and_expiry() {
        let mut secure = input("s");
        secure.secure = Some(true);
        let mut expired = build_cookie(&scope(), &input("old"), now()).expect("cookie");
        expired.expires = Some((now() - Duration::seconds(1)).to_rfc3339());
        let mut jar = CookieJar::new("api.example.com");
        jar.add(build_cookie(&scope(), &secure, now()).expect("cookie"));
        jar.add(build_cookie(&scope(), &input("plain"), now()).expect("cookie"));
        jar.add(expired);
        let jars = vec![jar];
        let names = |s: &UrlScope| {
            let mut n: Vec<String> = matching_cookies(&jars, s, now())
                .into_iter()
                .map(|c| c.name)
                .collect();
            n.sort();
            n
        };
        assert_eq!(names(&scope()), vec!["plain", "s"]);
        let http = UrlScope::parse("http://api.example.com/v1/items").expect("scope");
        assert_eq!(names(&http), vec!["plain"]);
        let other = UrlScope::parse("https://api.example.com/v2").expect("scope");
        assert!(names(&other).is_empty());
    }

    #[test]
    fn build_cookie_for_an_ip_host_is_host_only() {
        let ip = UrlScope::parse("http://127.0.0.1:3000/a/b").expect("scope");
        let c = build_cookie(&ip, &input("sid"), now()).expect("cookie");
        assert_eq!(c.domain, "127.0.0.1");
        let mut i = input("sid");
        i.domain = Some("127.0.0.1".into());
        let c = build_cookie(&ip, &i, now()).expect("cookie");
        assert_eq!(c.domain, "127.0.0.1");
        i.domain = Some("0.0.1".into());
        invalid(build_cookie(&ip, &i, now()));
        let local = UrlScope::parse("http://localhost:3000/").expect("scope");
        let c = build_cookie(&local, &input("sid"), now()).expect("cookie");
        assert_eq!(c.domain, "localhost");
    }
}
