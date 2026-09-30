//! HTTP Digest authentication (RFC 7616, RFC 2617 and the legacy RFC 2069 form).
//!
//! Pure functions only: the executor in `rocket-infra` sends the first request, reads the
//! `WWW-Authenticate` challenge from the 401, and retries with the header built here.

use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256, Sha512_256};

/// Hash algorithms a challenge may ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Md5,
    Sha256,
    Sha512_256,
}

impl Algorithm {
    /// Strength order used to pick between several Digest challenges.
    fn rank(self) -> u8 {
        match self {
            Algorithm::Md5 => 0,
            Algorithm::Sha256 => 1,
            Algorithm::Sha512_256 => 2,
        }
    }

    fn hash(self, data: &str) -> String {
        self.hash_bytes(data.as_bytes())
    }

    fn hash_bytes(self, data: &[u8]) -> String {
        match self {
            Algorithm::Md5 => hex::encode(md5::Md5::digest(data)),
            Algorithm::Sha256 => hex::encode(Sha256::digest(data)),
            Algorithm::Sha512_256 => hex::encode(Sha512_256::digest(data)),
        }
    }
}

/// One parsed `Digest` challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    pub algorithm: Algorithm,
    /// The `algorithm` value exactly as the server sent it, echoed back in the response.
    /// `None` when the server sent none, which means MD5.
    pub algorithm_token: Option<String>,
    /// Whether the `-sess` variant was requested.
    pub session: bool,
    pub qop: Vec<String>,
    pub stale: bool,
}

/// Returns a fresh random client nonce.
pub fn generate_cnonce() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

/// Picks the strongest usable Digest challenge from all `WWW-Authenticate` header values.
///
/// Returns `None` when there is no Digest challenge, or none with a supported algorithm.
pub fn select_challenge(header_values: &[&str]) -> Option<Challenge> {
    header_values
        .iter()
        .flat_map(|v| parse_challenges(v))
        .max_by_key(|c| (c.algorithm.rank(), !c.session))
}

/// Parses every Digest challenge in one `WWW-Authenticate` value.
///
/// A value may hold several challenges (`Basic realm="x", Digest realm="y", ...`), so the
/// parser splits on scheme tokens and keeps only the Digest ones. Challenges that are
/// unusable (no nonce, unknown algorithm) are dropped.
pub fn parse_challenges(value: &str) -> Vec<Challenge> {
    let mut out = Vec::new();
    for (scheme, params) in split_challenges(value) {
        if !scheme.eq_ignore_ascii_case("digest") {
            continue;
        }
        let get = |k: &str| {
            params
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(k))
                .map(|(_, v)| v.clone())
        };
        let Some(nonce) = get("nonce") else { continue };
        let algorithm_token = get("algorithm");
        let Some((algorithm, session)) = parse_algorithm(algorithm_token.as_deref()) else {
            continue;
        };
        out.push(Challenge {
            realm: get("realm").unwrap_or_default(),
            nonce,
            opaque: get("opaque"),
            algorithm,
            algorithm_token,
            session,
            qop: get("qop")
                .map(|q| {
                    q.split(',')
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            stale: get("stale").is_some_and(|s| s.eq_ignore_ascii_case("true")),
        });
    }
    out
}

fn parse_algorithm(token: Option<&str>) -> Option<(Algorithm, bool)> {
    let Some(token) = token else {
        return Some((Algorithm::Md5, false));
    };
    match token.to_ascii_uppercase().as_str() {
        "MD5" => Some((Algorithm::Md5, false)),
        "MD5-SESS" => Some((Algorithm::Md5, true)),
        "SHA-256" => Some((Algorithm::Sha256, false)),
        "SHA-256-SESS" => Some((Algorithm::Sha256, true)),
        "SHA-512-256" => Some((Algorithm::Sha512_256, false)),
        "SHA-512-256-SESS" => Some((Algorithm::Sha512_256, true)),
        _ => None,
    }
}

type Params = Vec<(String, String)>;

/// Splits a header value into `(scheme, params)` pairs.
///
/// A token followed by `=` is a parameter of the current challenge. A token followed by
/// anything else starts a new challenge. Quoted values may hold commas and escaped quotes.
fn split_challenges(value: &str) -> Vec<(String, Params)> {
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    let mut out: Vec<(String, Params)> = Vec::new();
    let is_sep = |c: char| c.is_whitespace() || c == ',';

    while i < chars.len() {
        while i < chars.len() && is_sep(chars[i]) {
            i += 1;
        }
        let start = i;
        while i < chars.len() && !is_sep(chars[i]) && chars[i] != '=' {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();
        if token.is_empty() {
            // A stray `=` with no name: skip it so the loop always advances.
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if j < chars.len() && chars[j] == '=' {
            j += 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let mut val = String::new();
            if j < chars.len() && chars[j] == '"' {
                j += 1;
                while j < chars.len() && chars[j] != '"' {
                    if chars[j] == '\\' && j + 1 < chars.len() {
                        j += 1;
                    }
                    val.push(chars[j]);
                    j += 1;
                }
                j += 1;
            } else {
                while j < chars.len() && chars[j] != ',' && !chars[j].is_whitespace() {
                    val.push(chars[j]);
                    j += 1;
                }
            }
            i = j;
            if let Some((_, params)) = out.last_mut() {
                params.push((token, val));
            }
        } else {
            out.push((token, Vec::new()));
        }
    }
    out
}

/// Builds the `Authorization` header value answering `challenge`.
///
/// `uri` is the request-target (path plus query) of the request that got the 401.
/// `entity_body` is only read for `qop=auth-int`. `nc` is the nonce count, `1` for the
/// first answer to a fresh nonce. `cnonce` is injected so the result is deterministic.
#[allow(clippy::too_many_arguments)]
pub fn authorize(
    challenge: &Challenge,
    username: &str,
    password: &str,
    method: &str,
    uri: &str,
    entity_body: &[u8],
    nc: u32,
    cnonce: &str,
) -> Result<String, String> {
    let alg = challenge.algorithm;
    let qop = choose_qop(&challenge.qop)?;

    let mut ha1 = alg.hash(&format!("{username}:{}:{password}", challenge.realm));
    if challenge.session {
        ha1 = alg.hash(&format!("{ha1}:{}:{cnonce}", challenge.nonce));
    }
    let ha2 = match qop {
        Some("auth-int") => alg.hash(&format!("{method}:{uri}:{}", alg.hash_bytes(entity_body))),
        _ => alg.hash(&format!("{method}:{uri}")),
    };
    let nc_hex = format!("{nc:08x}");
    let response = match qop {
        Some(q) => alg.hash(&format!(
            "{ha1}:{}:{nc_hex}:{cnonce}:{q}:{ha2}",
            challenge.nonce
        )),
        // RFC 2069 form: no qop, so no nc or cnonce either.
        None => alg.hash(&format!("{ha1}:{}:{ha2}", challenge.nonce)),
    };

    let mut parts = vec![
        format!("username=\"{}\"", quote(username)),
        format!("realm=\"{}\"", quote(&challenge.realm)),
        format!("nonce=\"{}\"", quote(&challenge.nonce)),
        format!("uri=\"{}\"", quote(uri)),
    ];
    if let Some(token) = &challenge.algorithm_token {
        parts.push(format!("algorithm={token}"));
    }
    parts.push(format!("response=\"{response}\""));
    if let Some(opaque) = &challenge.opaque {
        parts.push(format!("opaque=\"{}\"", quote(opaque)));
    }
    if let Some(q) = qop {
        parts.push(format!("qop={q}"));
        parts.push(format!("nc={nc_hex}"));
        parts.push(format!("cnonce=\"{}\"", quote(cnonce)));
    }
    Ok(format!("Digest {}", parts.join(", ")))
}

/// Prefers `auth`, falls back to `auth-int`, and uses the legacy form when no qop is offered.
fn choose_qop(offered: &[String]) -> Result<Option<&'static str>, String> {
    if offered.is_empty() {
        return Ok(None);
    }
    if offered.iter().any(|q| q == "auth") {
        return Ok(Some("auth"));
    }
    if offered.iter().any(|q| q == "auth-int") {
        return Ok(Some("auth-int"));
    }
    Err(format!(
        "Digest challenge offers no supported qop: {}",
        offered.join(",")
    ))
}

fn quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 2617 section 3.5 and RFC 7616 section 3.9.1 worked examples.
    const RFC2617: &str = "Digest realm=\"testrealm@host.com\", qop=\"auth,auth-int\", \
        nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", \
        opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";
    const RFC7616_NONCE: &str = "7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v";
    const RFC7616_CNONCE: &str = "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ";

    fn rfc7616(algorithm: &str, qop: &str) -> Challenge {
        let header = format!(
            "Digest realm=\"http-auth@example.org\", qop=\"{qop}\", algorithm={algorithm}, \
             nonce=\"{RFC7616_NONCE}\", opaque=\"FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS\""
        );
        select_challenge(&[&header]).expect("challenge parses")
    }

    fn response_of(header: &str) -> String {
        header
            .split("response=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .expect("response field")
            .to_string()
    }

    #[test]
    fn rfc2617_md5_example() {
        let c = select_challenge(&[RFC2617]).unwrap();
        let h = authorize(
            &c,
            "Mufasa",
            "Circle Of Life",
            "GET",
            "/dir/index.html",
            b"",
            1,
            "0a4f113b",
        )
        .unwrap();
        assert_eq!(response_of(&h), "6629fae49393a05397450978507c4ef1");
        assert!(
            h.contains("qop=auth, nc=00000001, cnonce=\"0a4f113b\""),
            "{h}"
        );
        assert!(
            h.contains("opaque=\"5ccc069c403ebaf9f0171e9517f40e41\""),
            "{h}"
        );
    }

    #[test]
    fn rfc7616_md5_and_sha256_examples() {
        for (alg, expected) in [
            ("MD5", "8ca523f5e9506fed4657c9700eebdbec"),
            (
                "SHA-256",
                "753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1",
            ),
        ] {
            let c = rfc7616(alg, "auth, auth-int");
            let h = authorize(
                &c,
                "Mufasa",
                "Circle of Life",
                "GET",
                "/dir/index.html",
                b"",
                1,
                RFC7616_CNONCE,
            )
            .unwrap();
            assert_eq!(response_of(&h), expected, "{alg}");
            assert!(h.contains(&format!("algorithm={alg}")), "{h}");
        }
    }

    // The next four expected values come from an independent Python hashlib calculation.
    #[test]
    fn sha512_256_md5_sess_legacy_and_auth_int() {
        let auth = |c: &Challenge, body: &[u8]| {
            response_of(
                &authorize(
                    c,
                    "Mufasa",
                    "Circle of Life",
                    "GET",
                    "/dir/index.html",
                    body,
                    1,
                    RFC7616_CNONCE,
                )
                .unwrap(),
            )
        };
        assert_eq!(
            auth(&rfc7616("SHA-512-256", "auth"), b""),
            "430d05014cecc49cab6fbe03176d41a1da86cbfe24a16580e22aaad928d960d0"
        );
        assert_eq!(
            auth(&rfc7616("MD5-sess", "auth"), b""),
            "e783283f46242139c486a698fec7211d"
        );
        assert_eq!(
            auth(&rfc7616("SHA-256", ""), b""),
            "a1306b0595a6c7fe96c448631fb5cfbd5107bd1fe1da729d978dd7446b812363"
        );
        assert_eq!(
            auth(&rfc7616("MD5", "auth-int"), b"hello"),
            "1707eaf5c6ed1a8f6ff2e49286b06bc2"
        );
    }

    #[test]
    fn legacy_form_omits_qop_nc_and_cnonce() {
        let c = rfc7616("SHA-256", "");
        let h = authorize(&c, "u", "p", "GET", "/", b"", 1, "cn").unwrap();
        assert!(!h.contains("qop="), "{h}");
        assert!(!h.contains("nc="), "{h}");
        assert!(!h.contains("cnonce="), "{h}");
    }

    #[test]
    fn missing_algorithm_means_md5_and_is_not_echoed() {
        let c = select_challenge(&[RFC2617]).unwrap();
        assert_eq!(c.algorithm, Algorithm::Md5);
        let h = authorize(&c, "u", "p", "GET", "/", b"", 1, "cn").unwrap();
        assert!(!h.contains("algorithm="), "{h}");
    }

    #[test]
    fn picks_digest_out_of_several_challenges_and_the_strongest_algorithm() {
        let header = "Basic realm=\"x\", Digest realm=\"a, b\", nonce=\"n1\", algorithm=MD5, \
                      Digest realm=\"a\", nonce=\"n2\", algorithm=SHA-256";
        let c = select_challenge(&[header]).unwrap();
        assert_eq!(c.nonce, "n2");
        assert_eq!(c.algorithm, Algorithm::Sha256);

        let split = select_challenge(&["Basic realm=\"x\"", "Digest realm=\"r\", nonce=\"n\""]);
        assert_eq!(split.unwrap().nonce, "n");
    }

    #[test]
    fn unusable_challenges_are_ignored() {
        assert!(select_challenge(&["Basic realm=\"x\""]).is_none());
        assert!(
            select_challenge(&["Digest realm=\"x\""]).is_none(),
            "no nonce"
        );
        assert!(select_challenge(&["Digest realm=\"x\", nonce=\"n\", algorithm=SHA-3"]).is_none());
        assert!(select_challenge(&["NTLM"]).is_none());
        assert!(select_challenge(&[""]).is_none());
    }

    #[test]
    fn parses_stale_and_quoted_commas_and_escapes() {
        let c =
            select_challenge(&["Digest realm=\"a,\\\"b\", nonce=\"n\", stale=TRUE, qop=\"auth\""])
                .unwrap();
        assert_eq!(c.realm, "a,\"b");
        assert!(c.stale);
        assert_eq!(c.qop, vec!["auth".to_string()]);
    }

    #[test]
    fn values_are_escaped_in_the_header() {
        let c = select_challenge(&["Digest realm=\"r\", nonce=\"n\""]).unwrap();
        let h = authorize(&c, "a\"b", "p", "GET", "/", b"", 1, "cn").unwrap();
        assert!(h.contains("username=\"a\\\"b\""), "{h}");
    }

    #[test]
    fn unsupported_qop_is_an_error() {
        let c = select_challenge(&["Digest realm=\"r\", nonce=\"n\", qop=\"weird\""]).unwrap();
        assert!(authorize(&c, "u", "p", "GET", "/", b"", 1, "cn").is_err());
    }

    #[test]
    fn cnonces_differ() {
        assert_ne!(generate_cnonce(), generate_cnonce());
    }
}
