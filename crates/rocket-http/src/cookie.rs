use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: Option<String>,
}

/// CookieJar aggregate — cookies grouped by domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieJar {
    pub domain: String,
    pub cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            cookies: Vec::new(),
        }
    }

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

    pub fn remove(&mut self, name: &str) {
        self.cookies.retain(|c| c.name != name);
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    pub fn get(&self, name: &str) -> Option<&Cookie> {
        self.cookies.iter().find(|c| c.name == name)
    }
}

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
        Some(i) if i > 0 && request_path.starts_with('/') => {
            request_path.get(..i).unwrap_or("/").to_string()
        }
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
            let covers =
                host == d || (host.ends_with(&format!(".{d}")) && host.parse::<IpAddr>().is_err());
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
    let cookie_path = if cookie_path.is_empty() {
        "/"
    } else {
        cookie_path
    };
    let request_path = if request_path.is_empty() {
        "/"
    } else {
        request_path
    };
    if cookie_path == request_path {
        return true;
    }
    request_path.starts_with(cookie_path)
        && (cookie_path.ends_with('/')
            || request_path
                .get(cookie_path.len()..)
                .is_some_and(|rest| rest.starts_with('/')))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cookie(name: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: "val".into(),
            domain: "example.com".into(),
            path: "/".into(),
            secure: false,
            http_only: false,
            expires: None,
        }
    }

    #[test]
    fn add_and_get_cookie() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        assert_eq!(jar.get("session").unwrap().value, "val");
    }

    #[test]
    fn add_replaces_existing() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        let mut updated = sample_cookie("session");
        updated.value = "new_val".into();
        jar.add(updated);
        assert_eq!(jar.cookies.len(), 1);
        assert_eq!(jar.get("session").unwrap().value, "new_val");
    }

    #[test]
    fn remove_cookie() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("session"));
        jar.remove("session");
        assert!(jar.get("session").is_none());
    }

    #[test]
    fn clear_all() {
        let mut jar = CookieJar::new("example.com");
        jar.add(sample_cookie("a"));
        jar.add(sample_cookie("b"));
        jar.clear();
        assert!(jar.cookies.is_empty());
    }

    use chrono::{Duration, TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0)
            .single()
            .expect("valid date")
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
        assert_eq!(
            jar.cookies.len(),
            2,
            "same name on another path is a different cookie"
        );
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
        assert_eq!(
            parse_set_cookie("=x", "h.test", "/", now()),
            SetCookie::Rejected
        );
        assert_eq!(
            parse_set_cookie("novalue", "h.test", "/", now()),
            SetCookie::Rejected
        );
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
        assert!(
            !domain_matches("h.test", "a.h.test"),
            "host-only must not match a subdomain"
        );
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
        let http =
            cookies_for_request(std::slice::from_ref(&jar), "h.test", "/a/b/c", false, now());
        let names: Vec<_> = http.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            ["deep", "root"],
            "secure, expired and foreign cookies are left out"
        );
        let https =
            cookies_for_request(std::slice::from_ref(&jar), "h.test", "/a/b/c", true, now());
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
}
