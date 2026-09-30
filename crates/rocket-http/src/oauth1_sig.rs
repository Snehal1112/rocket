//! OAuth 1.0 request signing (RFC 5849).
//!
//! Pure functions only: the executor in `rocket-infra` decides where the returned
//! parameters go on the outgoing request.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use rand::{distributions::Alphanumeric, Rng};
use rocket_shared::types::OAuth1Auth;
use sha1::{Digest as _, Sha1};
use sha2::{Sha256, Sha512};

/// Percent-encodes with the RFC 5849 §3.6 rules: only unreserved characters stay as-is.
pub fn percent_encode(input: &str) -> String {
    urlencoding::encode(input).into_owned()
}

/// Returns a fresh random nonce.
pub fn generate_nonce() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// Returns the current Unix time in seconds.
pub fn unix_timestamp() -> String {
    chrono::Utc::now().timestamp().to_string()
}

/// Signs a request and returns every `oauth_*` parameter, `oauth_signature` included.
///
/// `extra_params` are the request parameters that take part in the signature: the URL query
/// pairs plus the body pairs when the body is `application/x-www-form-urlencoded`.
/// `body` is only read to build `oauth_body_hash` when `include_body_hash` is set.
pub fn sign(
    method: &str,
    url: &str,
    extra_params: &[(String, String)],
    body: &[u8],
    auth: &OAuth1Auth,
    default_timestamp: &str,
    default_nonce: &str,
) -> Result<Vec<(String, String)>, String> {
    let method_name = auth.signature_method.as_deref().unwrap_or("HMAC-SHA1");
    let consumer_key = non_empty(&auth.consumer_key)
        .ok_or_else(|| "OAuth1 auth needs a consumer key".to_string())?;

    let mut oauth: Vec<(String, String)> = vec![
        ("oauth_consumer_key".into(), consumer_key.to_string()),
        (
            "oauth_nonce".into(),
            non_empty(&auth.nonce).unwrap_or(default_nonce).to_string(),
        ),
        ("oauth_signature_method".into(), method_name.to_string()),
        (
            "oauth_timestamp".into(),
            non_empty(&auth.timestamp)
                .unwrap_or(default_timestamp)
                .to_string(),
        ),
        (
            "oauth_version".into(),
            non_empty(&auth.version).unwrap_or("1.0").to_string(),
        ),
    ];
    if let Some(v) = non_empty(&auth.access_token) {
        oauth.push(("oauth_token".into(), v.to_string()));
    }
    if let Some(v) = non_empty(&auth.callback_url) {
        oauth.push(("oauth_callback".into(), v.to_string()));
    }
    if let Some(v) = non_empty(&auth.verifier) {
        oauth.push(("oauth_verifier".into(), v.to_string()));
    }
    if auth.include_body_hash == Some(true) {
        oauth.push(("oauth_body_hash".into(), body_hash(method_name, body)?));
    }

    let mut all: Vec<(String, String)> = extra_params.to_vec();
    all.extend(oauth.iter().cloned());
    let base = signature_base_string(method, url, &all)?;

    let key = format!(
        "{}&{}",
        percent_encode(auth.consumer_secret.as_deref().unwrap_or("")),
        percent_encode(auth.access_token_secret.as_deref().unwrap_or(""))
    );
    let signature = compute_signature(method_name, &base, &key)?;
    oauth.push(("oauth_signature".into(), signature));
    Ok(oauth)
}

/// Builds the `Authorization: OAuth ...` header value.
pub fn authorization_header(realm: Option<&str>, oauth_params: &[(String, String)]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(r) = realm.filter(|r| !r.is_empty()) {
        parts.push(format!("realm=\"{}\"", percent_encode(r)));
    }
    for (k, v) in oauth_params {
        parts.push(format!("{}=\"{}\"", percent_encode(k), percent_encode(v)));
    }
    format!("OAuth {}", parts.join(", "))
}

/// Signature base string: `METHOD&enc(base-url)&enc(normalized-params)` (RFC 5849 §3.4.1).
pub fn signature_base_string(
    method: &str,
    url: &str,
    params: &[(String, String)],
) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Failed to parse URL: {e}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    // `Url::port` is None for the scheme's default port, which the base string must omit.
    let authority = match parsed.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    };
    let base_url = format!("{}://{}{}", parsed.scheme(), authority, parsed.path());

    let mut encoded: Vec<(String, String)> = params
        .iter()
        .filter(|(k, _)| k != "oauth_signature")
        .map(|(k, v)| (percent_encode(k), percent_encode(v)))
        .collect();
    encoded.sort();
    let normalized = encoded
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    Ok(format!(
        "{}&{}&{}",
        method.to_uppercase(),
        percent_encode(&base_url),
        percent_encode(&normalized)
    ))
}

fn compute_signature(method: &str, base: &str, key: &str) -> Result<String, String> {
    match method {
        "HMAC-SHA1" => Ok(hmac_b64::<Hmac<Sha1>>(key, base)),
        "HMAC-SHA256" => Ok(hmac_b64::<Hmac<Sha256>>(key, base)),
        "HMAC-SHA512" => Ok(hmac_b64::<Hmac<Sha512>>(key, base)),
        "PLAINTEXT" => Ok(key.to_string()),
        "RSA-SHA1" | "RSA-SHA256" | "RSA-SHA512" => Err(format!(
            "OAuth1 signature method {method} is not supported yet"
        )),
        other => Err(format!("Unknown OAuth1 signature method: {other}")),
    }
}

fn hmac_b64<M: Mac + hmac::digest::KeyInit>(key: &str, data: &str) -> String {
    // HMAC accepts keys of any length, so `new_from_slice` cannot fail here.
    let mut mac = <M as Mac>::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(data.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}

/// `oauth_body_hash`: base64 of the body digest that matches the signature method.
fn body_hash(method: &str, body: &[u8]) -> Result<String, String> {
    let digest = match method {
        "HMAC-SHA1" | "RSA-SHA1" => Sha1::digest(body).to_vec(),
        "HMAC-SHA256" | "RSA-SHA256" => Sha256::digest(body).to_vec(),
        "HMAC-SHA512" | "RSA-SHA512" => Sha512::digest(body).to_vec(),
        other => {
            return Err(format!(
                "OAuth1 body hash is not defined for signature method {other}"
            ))
        }
    };
    Ok(STANDARD.encode(digest))
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Worked example from the Twitter OAuth 1.0a signing documentation.
    const URL: &str = "https://api.twitter.com/1.1/statuses/update.json";
    const CONSUMER_SECRET: &str = "kAcSOqF21Fu85e7zjz7ZN2U4ZRhfV3WpwPAoE3Z7kBw";
    const TOKEN_SECRET: &str = "LswwdoUaIvS8ltyTt5jkRh4J50vUPVVHtR2YPi5kE";

    fn sample_auth(method: &str) -> OAuth1Auth {
        OAuth1Auth {
            consumer_key: Some("xvz1evFS4wEEPTGEFPHBog".into()),
            consumer_secret: Some(CONSUMER_SECRET.into()),
            access_token: Some("370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb".into()),
            access_token_secret: Some(TOKEN_SECRET.into()),
            signature_method: Some(method.into()),
            ..Default::default()
        }
    }

    fn body_params() -> Vec<(String, String)> {
        vec![
            (
                "status".into(),
                "Hello Ladies + Gentlemen, a signed OAuth request!".into(),
            ),
            ("include_entities".into(), "true".into()),
        ]
    }

    fn signature_of(params: &[(String, String)]) -> &str {
        &params
            .iter()
            .find(|(k, _)| k == "oauth_signature")
            .expect("signature present")
            .1
    }

    const NONCE: &str = "kYjzVBB8Y0ZFabxSWbWovY3uYSQ2pTgmZeNu2VS4cg";

    #[test]
    fn hmac_sha1_matches_published_example() {
        let out = sign(
            "POST",
            URL,
            &body_params(),
            b"",
            &sample_auth("HMAC-SHA1"),
            "1318622958",
            NONCE,
        )
        .unwrap();
        assert_eq!(signature_of(&out), "hCtSmYh+iHYCEqBWrE7C7hYmtUk=");
    }

    #[test]
    fn hmac_sha256_matches_reference_value() {
        let out = sign(
            "POST",
            URL,
            &body_params(),
            b"",
            &sample_auth("HMAC-SHA256"),
            "1318622958",
            NONCE,
        )
        .unwrap();
        assert_eq!(
            signature_of(&out),
            "PLbq+OWUE2vwiOZeZBSR06GFvymUHoaBdCIHyD66IcM="
        );
    }

    #[test]
    fn plaintext_signature_is_the_encoded_secret_pair() {
        let out = sign("GET", URL, &[], b"", &sample_auth("PLAINTEXT"), "1", "n").unwrap();
        assert_eq!(
            signature_of(&out),
            format!("{CONSUMER_SECRET}&{TOKEN_SECRET}")
        );
    }

    #[test]
    fn explicit_timestamp_and_nonce_override_the_defaults() {
        let mut auth = sample_auth("HMAC-SHA1");
        auth.timestamp = Some("42".into());
        auth.nonce = Some("fixed".into());
        let out = sign("GET", URL, &[], b"", &auth, "1", "generated").unwrap();
        let get = |k: &str| out.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("oauth_timestamp"), Some("42"));
        assert_eq!(get("oauth_nonce"), Some("fixed"));
        assert_eq!(get("oauth_version"), Some("1.0"));
    }

    #[test]
    fn base_string_drops_default_port_and_keeps_custom_port() {
        let a = signature_base_string("get", "HTTPS://Example.COM:443/a/b?x=1", &[]).unwrap();
        assert!(
            a.starts_with("GET&https%3A%2F%2Fexample.com%2Fa%2Fb&"),
            "{a}"
        );
        let b = signature_base_string("GET", "http://example.com:8080/a", &[]).unwrap();
        assert!(b.contains("http%3A%2F%2Fexample.com%3A8080%2Fa"), "{b}");
    }

    #[test]
    fn base_string_sorts_params_by_encoded_key_then_value() {
        let params = vec![
            ("b".to_string(), "2".to_string()),
            ("a".to_string(), "z".to_string()),
            ("a".to_string(), "1".to_string()),
        ];
        let base = signature_base_string("GET", "https://e.com/", &params).unwrap();
        assert!(base.ends_with("&a%3D1%26a%3Dz%26b%3D2"), "{base}");
    }

    #[test]
    fn body_hash_is_added_and_uses_the_method_digest() {
        let mut auth = sample_auth("HMAC-SHA1");
        auth.include_body_hash = Some(true);
        let out = sign("POST", URL, &[], b"hello", &auth, "1", "n").unwrap();
        let hash = &out.iter().find(|(k, _)| k == "oauth_body_hash").unwrap().1;
        // base64(sha1("hello"))
        assert_eq!(hash, "qvTGHdzF6KLavt4PO0gs2a6pQ00=");
    }

    #[test]
    fn rsa_methods_fail_loudly_instead_of_sending_unsigned() {
        let err = sign("GET", URL, &[], b"", &sample_auth("RSA-SHA1"), "1", "n").unwrap_err();
        assert!(err.contains("RSA-SHA1"), "{err}");
    }

    #[test]
    fn missing_consumer_key_is_an_error() {
        let mut auth = sample_auth("HMAC-SHA1");
        auth.consumer_key = None;
        assert!(sign("GET", URL, &[], b"", &auth, "1", "n").is_err());
    }

    #[test]
    fn authorization_header_encodes_values_and_leads_with_realm() {
        let header = authorization_header(
            Some("api realm"),
            &[("oauth_signature".into(), "a+b/c=".into())],
        );
        assert_eq!(
            header,
            "OAuth realm=\"api%20realm\", oauth_signature=\"a%2Bb%2Fc%3D\""
        );
    }
}
