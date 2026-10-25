//! Turns a pasted spec link into a `SpecLocation` and into something that can be fetched.

use crate::error::{ImportError, ImportResult};
use rocket_collection::openapi_source::SpecLocation;

/// What to request for a `SpecLocation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchTarget {
    pub url: String,
    /// The `Accept` header to send, when the server needs one to return the raw file.
    pub accept: Option<String>,
    /// `scheme://host[:port]` of `url`. A token may only be sent to this origin.
    pub origin: String,
}

fn reject(message: &str) -> ImportError {
    ImportError::NotAnOpenApiSpec(message.to_string())
}

/// Read a pasted link. GitHub links (blob, raw, raw.githubusercontent.com and the contents
/// API) become a `Github` location. Any other `https` link stays as it is, without its
/// fragment. Anything else is rejected with a message the user can act on.
pub fn parse_spec_url(input: &str) -> ImportResult<SpecLocation> {
    let text = input.trim();
    if text.is_empty() {
        return Err(reject("paste a link to an OpenAPI file"));
    }
    let Some(rest) = strip_https(text) else {
        return Err(if text.contains("://") {
            reject("only https links are supported")
        } else {
            reject("that is not a link")
        });
    };
    let rest = rest.split('#').next().unwrap_or_default();
    let (before_query, query) = match rest.split_once('?') {
        Some((before, query)) => (before, Some(query)),
        None => (rest, None),
    };
    let (authority, path) = match before_query.split_once('/') {
        Some((authority, path)) => (authority, path),
        None => (before_query, ""),
    };
    if authority.is_empty() {
        return Err(reject("that is not a link"));
    }
    if authority.contains('@') {
        return Err(reject(
            "links with a user name or password are not supported",
        ));
    }
    let host = authority.to_ascii_lowercase();
    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .collect();
    let file_hint = "link to the file inside the repository";

    match host.as_str() {
        "github.com" => {
            if segments.len() >= 3 && segments[2] == "tree" {
                return Err(reject("link to the file, not a folder"));
            }
            if segments.len() >= 5 && (segments[2] == "blob" || segments[2] == "raw") {
                return github(&segments[0], &segments[1], &segments[3], &segments[4..]);
            }
            Err(reject(file_hint))
        }
        "raw.githubusercontent.com" => {
            if segments.len() >= 6
                && segments[2] == "refs"
                && (segments[3] == "heads" || segments[3] == "tags")
            {
                return github(&segments[0], &segments[1], &segments[4], &segments[5..]);
            }
            if segments.len() >= 4 {
                return github(&segments[0], &segments[1], &segments[2], &segments[3..]);
            }
            Err(reject(file_hint))
        }
        "api.github.com" => {
            if segments.len() >= 5 && segments[0] == "repos" && segments[3] == "contents" {
                let git_ref = query
                    .into_iter()
                    .flat_map(|q| q.split('&'))
                    .find_map(|pair| pair.strip_prefix("ref="))
                    .map(percent_decode)
                    .filter(|r| !r.is_empty())
                    .unwrap_or_else(|| "HEAD".to_string());
                return github(&segments[1], &segments[2], &git_ref, &segments[4..]);
            }
            Err(reject(file_hint))
        }
        _ => {
            let mut url = format!("https://{before_query}");
            if let Some(query) = query {
                if query_has_secret_name(query) {
                    return Err(reject(
                        "the link has a secret in its query string; remove it from the link",
                    ));
                }
                url.push('?');
                url.push_str(query);
            }
            Ok(SpecLocation::Https { url })
        }
    }
}

/// Builds a `Github` location after checking every part. A `.` or `..` segment (even one that
/// arrived percent-encoded), an empty part, or a slash inside an owner, repo or path segment
/// could send a token-carrying request to a different API path, so it is refused. A ref may
/// contain `/` (a branch like `feature/x`) but not as a way to climb out.
fn github(owner: &str, repo: &str, git_ref: &str, path: &[String]) -> ImportResult<SpecLocation> {
    let plain = |s: &str| !s.is_empty() && !is_dot_segment(s) && !s.contains(['/', '\\']);
    let ref_ok = !git_ref.contains('\\')
        && git_ref
            .split('/')
            .all(|piece| !piece.is_empty() && !is_dot_segment(piece));
    if !(plain(owner) && plain(repo) && ref_ok && path.iter().all(|s| plain(s))) {
        return Err(reject("that link has a part that is not allowed"));
    }
    Ok(SpecLocation::Github {
        owner: owner.to_string(),
        repo: repo.to_string(),
        git_ref: git_ref.to_string(),
        path: path.join("/"),
    })
}

fn is_dot_segment(segment: &str) -> bool {
    segment == "." || segment == ".."
}

/// Query names that usually carry a secret. A link with one is refused, because the link is
/// saved in a file that is committed to git.
const SECRET_QUERY_HINTS: [&str; 7] = [
    "token",
    "key",
    "secret",
    "sig",
    "pass",
    "auth",
    "credential",
];

fn query_has_secret_name(query: &str) -> bool {
    query.split('&').any(|pair| {
        let name = percent_decode(pair.split('=').next().unwrap_or_default()).to_ascii_lowercase();
        SECRET_QUERY_HINTS.iter().any(|hint| name.contains(hint))
    })
}

fn strip_https(text: &str) -> Option<&str> {
    let prefix = text.get(..8)?;
    prefix.eq_ignore_ascii_case("https://").then(|| &text[8..])
}

/// What to request for a location. With a token, GitHub files are read through the contents
/// API (which accepts it for private repositories); without one, from the raw host.
pub fn fetch_target(location: &SpecLocation, with_token: bool) -> FetchTarget {
    match location {
        SpecLocation::Github {
            owner,
            repo,
            git_ref,
            path,
        } => {
            let (owner, repo) = (encode_segment(owner), encode_segment(repo));
            let encoded_path = encode_path(path);
            if with_token {
                FetchTarget {
                    url: format!(
                        "https://api.github.com/repos/{owner}/{repo}/contents/{encoded_path}?ref={}",
                        encode_segment(git_ref)
                    ),
                    accept: Some("application/vnd.github.raw+json".to_string()),
                    origin: "https://api.github.com".to_string(),
                }
            } else {
                FetchTarget {
                    url: format!(
                        "https://raw.githubusercontent.com/{owner}/{repo}/{}/{encoded_path}",
                        encode_path(git_ref)
                    ),
                    accept: None,
                    origin: "https://raw.githubusercontent.com".to_string(),
                }
            }
        }
        SpecLocation::Https { url } => FetchTarget {
            url: url.clone(),
            accept: None,
            origin: origin_of(url),
        },
    }
}

/// `scheme://host[:port]` of a URL.
fn origin_of(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        authority.to_ascii_lowercase()
    )
}

fn encode_path(path: &str) -> String {
    path.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// Percent-encode everything except letters, digits and `-._~`.
fn encode_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github(git_ref: &str, path: &str) -> SpecLocation {
        SpecLocation::Github {
            owner: "acme".to_string(),
            repo: "orders".to_string(),
            git_ref: git_ref.to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn blob_raw_and_api_links_give_the_same_location() {
        let want = github("main", "docs/openapi.yaml");
        for link in [
            "https://github.com/acme/orders/blob/main/docs/openapi.yaml",
            "https://github.com/acme/orders/raw/main/docs/openapi.yaml",
            "https://raw.githubusercontent.com/acme/orders/main/docs/openapi.yaml",
            "https://api.github.com/repos/acme/orders/contents/docs/openapi.yaml?ref=main",
        ] {
            assert_eq!(parse_spec_url(link).expect(link), want, "{link}");
        }
    }

    #[test]
    fn queries_fragments_refs_heads_and_case_are_handled() {
        assert_eq!(
            parse_spec_url("https://GitHub.com/acme/orders/blob/main/openapi.yaml?raw=true#L10")
                .expect("blob with query"),
            github("main", "openapi.yaml")
        );
        assert_eq!(
            parse_spec_url(
                "https://raw.githubusercontent.com/acme/orders/refs/heads/main/openapi.yaml"
            )
            .expect("refs/heads"),
            github("main", "openapi.yaml")
        );
        assert_eq!(
            parse_spec_url("  https://github.com/acme/orders/blob/v1.2.0/spec.json  ")
                .expect("padded"),
            github("v1.2.0", "spec.json")
        );
        assert_eq!(
            parse_spec_url(
                "https://api.github.com/repos/acme/orders/contents/spec.json?ref=feature%2Fx"
            )
            .expect("encoded ref"),
            github("feature/x", "spec.json")
        );
        assert_eq!(
            parse_spec_url("https://api.github.com/repos/acme/orders/contents/spec.json")
                .expect("default ref"),
            github("HEAD", "spec.json")
        );
    }

    #[test]
    fn other_https_links_stay_as_they_are() {
        assert_eq!(
            parse_spec_url("https://example.com/api/openapi.json#top").expect("https"),
            SpecLocation::Https {
                url: "https://example.com/api/openapi.json".to_string()
            }
        );
    }

    #[test]
    fn rejects_unsafe_or_unusable_links() {
        for link in [
            "http://example.com/openapi.json",
            "file:///etc/passwd",
            "ftp://example.com/x",
            "https://user:pass@example.com/x",
            "https://github.com/acme/orders/tree/main/docs",
            "https://github.com/acme",
            "",
            "not a url",
        ] {
            let err = parse_spec_url(link).expect_err(link);
            assert!(matches!(err, ImportError::NotAnOpenApiSpec(_)), "{link}");
        }
    }

    #[test]
    fn dot_segments_cannot_steer_a_request_elsewhere() {
        for link in [
            "https://github.com/acme/orders/blob/main/%2e%2e/%2e%2e/%2e%2e/%2e%2e/user/repos",
            "https://github.com/acme/orders/blob/main/docs/../../../user",
            "https://github.com/../../blob/main/user",
            "https://github.com/acme/orders/blob/main/a%2F..%2F..%2F..%2Fuser",
            "https://github.com/acme/orders/blob/../x/spec.yaml",
            "https://raw.githubusercontent.com/acme/orders/main/../../other/main/x.yaml",
            "https://api.github.com/repos/acme/orders/contents/spec.yaml?ref=../..",
            "https://api.github.com/repos/acme/orders/contents/spec.yaml?ref=feature%2F..%2Fx",
            "https://api.github.com/repos/../orders/contents/spec.yaml",
        ] {
            let err = parse_spec_url(link).expect_err(link);
            assert!(matches!(err, ImportError::NotAnOpenApiSpec(_)), "{link}");
        }
        // A ref with a slash and a path with dots in names stay valid.
        assert_eq!(
            parse_spec_url("https://api.github.com/repos/acme/orders/contents/a/.hidden/spec.v1.yaml?ref=feature%2Fx")
                .expect("valid"),
            github("feature/x", "a/.hidden/spec.v1.yaml")
        );
    }

    #[test]
    fn https_links_with_secret_looking_query_names_are_rejected() {
        for link in [
            "https://example.com/spec.json?token=abc",
            "https://example.com/spec.json?format=json&access_key=x",
            "https://example.com/spec.json?X-Amz-Signature=deadbeef",
            "https://example.com/spec.json?api_key=1",
            "https://example.com/spec.json?Password=hunter2",
            "https://example.com/spec.json?auth=Zm9v",
        ] {
            let err = parse_spec_url(link).expect_err(link);
            assert!(matches!(err, ImportError::NotAnOpenApiSpec(_)), "{link}");
        }
        for link in [
            "https://example.com/spec.json?format=json",
            "https://example.com/spec.json?v=2&lang=en",
        ] {
            assert!(parse_spec_url(link).is_ok(), "{link}");
        }
    }

    #[test]
    fn fetch_target_picks_raw_or_contents_api() {
        let loc = github("feature/x", "docs/open api.yaml");
        let api = fetch_target(&loc, true);
        assert_eq!(
            api.url,
            "https://api.github.com/repos/acme/orders/contents/docs/open%20api.yaml?ref=feature%2Fx"
        );
        assert_eq!(
            api.accept.as_deref(),
            Some("application/vnd.github.raw+json")
        );
        assert_eq!(api.origin, "https://api.github.com");

        let raw = fetch_target(&github("main", "docs/openapi.yaml"), false);
        assert_eq!(
            raw.url,
            "https://raw.githubusercontent.com/acme/orders/main/docs/openapi.yaml"
        );
        assert_eq!(raw.accept, None);
        assert_eq!(raw.origin, "https://raw.githubusercontent.com");

        let other = fetch_target(
            &SpecLocation::Https {
                url: "https://example.com:8443/a/openapi.json".to_string(),
            },
            true,
        );
        assert_eq!(other.url, "https://example.com:8443/a/openapi.json");
        assert_eq!(other.accept, None);
        assert_eq!(other.origin, "https://example.com:8443");
    }

    #[test]
    fn immutable_ref_is_detected() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        assert!(github(sha, "a.yaml").ref_is_immutable());
        assert!(!github("main", "a.yaml").ref_is_immutable());
        assert!(!github("v1.2.0", "a.yaml").ref_is_immutable());
        assert!(!SpecLocation::Https {
            url: "https://example.com/a".to_string()
        }
        .ref_is_immutable());
    }

    #[test]
    fn display_url_round_trips_through_the_parser() {
        let loc = github("main", "docs/openapi.yaml");
        assert_eq!(
            loc.display_url(),
            "https://github.com/acme/orders/blob/main/docs/openapi.yaml"
        );
        assert_eq!(parse_spec_url(&loc.display_url()).expect("round trip"), loc);
    }
}
