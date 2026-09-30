//! WS-Security UsernameToken headers (WSSE) for HTTP requests.
//!
//! Pure functions only: the executor in `rocket-infra` adds the headers to the request.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use sha1::{Digest as _, Sha1};

/// The two headers a WSSE request carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsseHeaders {
    /// Value for `Authorization`: `WSSE profile="UsernameToken"`.
    pub authorization: String,
    /// Value for `X-WSSE`: the UsernameToken with its password digest.
    pub x_wsse: String,
}

/// Returns 16 random bytes to use as the nonce.
pub fn generate_nonce() -> Vec<u8> {
    let mut nonce = vec![0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce
}

/// Returns the current UTC time in the `Created` format, e.g. `2003-12-15T14:43:07Z`.
pub fn created_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Builds the WSSE headers.
///
/// `PasswordDigest = Base64(SHA1(nonce + created + password))`, and the `Nonce` field is the
/// Base64 of the same nonce bytes. The nonce and timestamp are injected so the result is
/// deterministic in tests.
pub fn wsse_headers(
    username: &str,
    password: &str,
    nonce: &[u8],
    created: &str,
) -> Result<WsseHeaders, String> {
    if username.chars().any(|c| c == '"' || c.is_control()) {
        return Err("WSSE username must not contain quotes or control characters".into());
    }
    let mut hasher = Sha1::new();
    hasher.update(nonce);
    hasher.update(created.as_bytes());
    hasher.update(password.as_bytes());
    let digest = STANDARD.encode(hasher.finalize());

    Ok(WsseHeaders {
        authorization: "WSSE profile=\"UsernameToken\"".to_string(),
        x_wsse: format!(
            "UsernameToken Username=\"{username}\", PasswordDigest=\"{digest}\", \
             Nonce=\"{}\", Created=\"{created}\"",
            STANDARD.encode(nonce)
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Worked example from the original WSSE (Atom authentication) write-up.
    const NONCE_TEXT: &str = "d36e316282959a9ed4c89851497a717f";
    const CREATED: &str = "2003-12-15T14:43:07Z";

    #[test]
    fn digest_matches_published_example() {
        let h = wsse_headers("bob", "taadtaadpstcsm", NONCE_TEXT.as_bytes(), CREATED).unwrap();
        assert!(
            h.x_wsse
                .contains("PasswordDigest=\"quR/EWLAV4xLf9Zqyw4pDmfV9OY=\""),
            "{}",
            h.x_wsse
        );
    }

    #[test]
    fn header_layout_is_username_token_with_base64_nonce() {
        let h = wsse_headers("bob", "pw", NONCE_TEXT.as_bytes(), CREATED).unwrap();
        assert_eq!(h.authorization, "WSSE profile=\"UsernameToken\"");
        assert!(h
            .x_wsse
            .starts_with("UsernameToken Username=\"bob\", PasswordDigest=\""));
        assert!(h
            .x_wsse
            .contains("Nonce=\"ZDM2ZTMxNjI4Mjk1OWE5ZWQ0Yzg5ODUxNDk3YTcxN2Y=\""));
        assert!(h.x_wsse.ends_with(&format!("Created=\"{CREATED}\"")));
    }

    #[test]
    fn username_with_a_quote_is_rejected() {
        assert!(wsse_headers("b\"ob", "pw", b"n", CREATED).is_err());
        assert!(wsse_headers("bob\n", "pw", b"n", CREATED).is_err());
    }

    #[test]
    fn generated_nonces_differ_and_timestamp_has_the_expected_shape() {
        assert_ne!(generate_nonce(), generate_nonce());
        let t = created_now();
        assert_eq!(t.len(), 20, "{t}");
        assert!(t.ends_with('Z'), "{t}");
    }
}
