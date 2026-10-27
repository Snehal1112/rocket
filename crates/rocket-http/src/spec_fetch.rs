//! Port for downloading an OpenAPI spec from a link. The reqwest implementation lives in
//! `rocket-infra`; this module holds the contract and the pure link checks both sides share.

use async_trait::async_trait;
use std::fmt;
use std::net::IpAddr;

/// Largest spec a fetch will read, in bytes.
pub const DEFAULT_MAX_SPEC_BYTES: usize = 25 * 1024 * 1024;

/// Most redirects one fetch follows.
pub const MAX_SPEC_REDIRECTS: usize = 5;

/// What to download. `bearer_token` is sent only to the origin of `url`, never after a
/// redirect to another origin.
#[derive(Clone)]
pub struct SpecFetchRequest {
    pub url: String,
    pub accept: Option<String>,
    pub bearer_token: Option<String>,
    pub if_none_match: Option<String>,
    pub if_modified_since: Option<String>,
    pub max_bytes: usize,
}

/// The URL without its query values, which may carry a secret.
fn redact_query(url: &str) -> String {
    match url.split_once('?') {
        Some((base, _)) => format!("{base}?<redacted>"),
        None => url.to_string(),
    }
}

impl fmt::Debug for SpecFetchRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpecFetchRequest")
            .field("url", &redact_query(&self.url))
            .field("accept", &self.accept)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "<redacted>"),
            )
            .field("if_none_match", &self.if_none_match)
            .field("if_modified_since", &self.if_modified_since)
            .field("max_bytes", &self.max_bytes)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecFetchOutcome {
    Fetched {
        body: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
        /// The URL the body came from, after redirects.
        final_url: String,
    },
    /// The server answered 304: the spec has not changed since the validators were taken.
    NotModified,
}

/// Why a fetch gave nothing. Each state reads differently in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecFetchFailure {
    NotFound,
    Unauthorized,
    RateLimited { retry_after_secs: Option<u64> },
    TooLarge,
    NotHttps,
    TooManyRedirects,
    BlockedRedirect,
    Status(u16),
    Network(String),
}

impl fmt::Display for SpecFetchFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(
                f,
                "The file was not found. For a private repository, add a token."
            ),
            Self::Unauthorized => write!(f, "The server refused access. Check the token."),
            Self::RateLimited {
                retry_after_secs: Some(s),
            } => {
                write!(
                    f,
                    "The server is rate limiting requests. Try again in {s} seconds."
                )
            }
            Self::RateLimited {
                retry_after_secs: None,
            } => {
                write!(f, "The server is rate limiting requests. Try again later.")
            }
            Self::TooLarge => write!(f, "The file is larger than the 25 MB limit."),
            Self::NotHttps => write!(f, "Only https links can be fetched."),
            Self::TooManyRedirects => write!(f, "The link redirected too many times."),
            Self::BlockedRedirect => {
                write!(f, "The link redirected to an address that is not allowed.")
            }
            Self::Status(code) => write!(f, "The server answered with status {code}."),
            Self::Network(message) => write!(f, "The file could not be fetched: {message}"),
        }
    }
}

impl std::error::Error for SpecFetchFailure {}

#[async_trait]
pub trait SpecFetcher: Send + Sync {
    async fn fetch(&self, request: SpecFetchRequest) -> Result<SpecFetchOutcome, SpecFetchFailure>;
}

/// True for a plain `http` link to this machine: the host is exactly `localhost`, `127.0.0.1`
/// or `::1`, with no user info. Anything else on `http` stays refused, and a host that only
/// starts with one of these names is another host.
pub fn is_loopback_http(url: &reqwest::Url) -> bool {
    use std::net::{Ipv4Addr, Ipv6Addr};
    if url.scheme() != "http" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(name)) => name == "localhost",
        Some(url::Host::Ipv4(ip)) => ip == Ipv4Addr::LOCALHOST,
        Some(url::Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
        None => false,
    }
}

/// True when a redirect to `to` may be followed: it must be `https`, and its host must not be
/// a loopback, private, link-local or unspecified address. Host names are not resolved, so
/// this only stops address literals and `localhost`.
pub fn redirect_target_allowed(to: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(to) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    // An IPv6 literal comes back in brackets.
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return !ip_is_local(ip);
    }
    // A trailing dot is the same host, so `localhost.` must not slip through.
    let name = host.to_ascii_lowercase();
    let name = name.trim_end_matches('.');
    name != "localhost" && !name.ends_with(".localhost")
}

fn ip_is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return ip_is_local(IpAddr::V4(mapped));
            }
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                // Unique local (fc00::/7) and link-local (fe80::/10).
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
        }
    }
}

/// True when both URLs have the same scheme, host and port.
pub fn same_origin(a: &str, b: &str) -> bool {
    let (Ok(a), Ok(b)) = (reqwest::Url::parse(a), reqwest::Url::parse(b)) else {
        return false;
    };
    a.scheme() == b.scheme()
        && a.host_str().map(str::to_ascii_lowercase) == b.host_str().map(str::to_ascii_lowercase)
        && a.port_or_known_default() == b.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_target_allowed_rejects_http_and_private_ips() {
        for ok in [
            "https://example.com/spec.yaml",
            "https://raw.githubusercontent.com/a/b/main/x.yaml",
            "https://8.8.8.8/x",
        ] {
            assert!(redirect_target_allowed(ok), "{ok}");
        }
        for bad in [
            "http://example.com/x",
            "https://127.0.0.1/",
            "https://10.0.0.5/",
            "https://172.16.0.1/",
            "https://192.168.1.2/",
            "https://169.254.169.254/",
            "https://0.0.0.0/",
            "https://[::1]/",
            "https://[fd00::1]/",
            "https://[fe80::1]/",
            "https://[::ffff:10.0.0.1]/",
            "https://localhost/x",
            "https://api.localhost/x",
            "https://localhost./x",
            "https://api.localhost./x",
            "ftp://example.com/x",
            "not a url",
        ] {
            assert!(!redirect_target_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn is_loopback_http_table() {
        let parse = |s: &str| reqwest::Url::parse(s).expect("url");
        for ok in [
            "http://localhost/x",
            "http://LOCALHOST:3000/x",
            "http://127.0.0.1:8080/x",
            "http://[::1]/x",
        ] {
            assert!(is_loopback_http(&parse(ok)), "{ok}");
        }
        for bad in [
            "https://localhost/x",
            "http://localhost.evil.com/x",
            "http://127.0.0.1.evil.com/x",
            "http://user@localhost/x",
            "http://user:pw@127.0.0.1/x",
            "http://0.0.0.0/x",
            "http://192.168.1.5/x",
            "http://10.0.0.1/x",
            "http://localhost./x",
            "http://api.localhost/x",
            "http://example.com/x",
            "ftp://localhost/x",
        ] {
            assert!(!is_loopback_http(&parse(bad)), "{bad}");
        }
    }

    #[test]
    fn same_origin_compares_scheme_host_and_port() {
        assert!(same_origin("https://a.com/x", "https://A.com/y?z=1"));
        assert!(same_origin("https://a.com:443/x", "https://a.com/y"));
        assert!(!same_origin("https://a.com/x", "http://a.com/x"));
        assert!(!same_origin("https://a.com/x", "https://b.com/x"));
        assert!(!same_origin("https://a.com:8443/x", "https://a.com/x"));
        assert!(!same_origin("garbage", "https://a.com/x"));
    }

    #[test]
    fn request_debug_hides_query_values() {
        let request = SpecFetchRequest {
            url: "https://a.com/x?token=abc123&format=json".to_string(),
            accept: None,
            bearer_token: None,
            if_none_match: None,
            if_modified_since: None,
            max_bytes: 10,
        };
        let text = format!("{request:?}");
        assert!(!text.contains("abc123"), "{text}");
        assert!(text.contains("https://a.com/x"), "{text}");
    }

    #[test]
    fn request_debug_hides_the_token() {
        let request = SpecFetchRequest {
            url: "https://a.com/x".to_string(),
            accept: None,
            bearer_token: Some("s3cret-token".to_string()),
            if_none_match: None,
            if_modified_since: None,
            max_bytes: 10,
        };
        let text = format!("{request:?}");
        assert!(!text.contains("s3cret-token"));
        assert!(text.contains("<redacted>"));
    }
}
