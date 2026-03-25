use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};

/// PKCE pair per RFC 7636.
pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
}

/// Generates a PKCE code_verifier (43-128 URL-safe chars) and
/// code_challenge (BASE64URL(SHA256(verifier))).
pub fn generate_pkce() -> PkcePair {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);

    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let digest = hasher.finalize();
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);

    PkcePair { code_verifier, code_challenge }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_verifier_length_valid() {
        let pair = generate_pkce();
        // 32 bytes base64url-encoded = 43 chars (within RFC 7636 range of 43-128).
        assert!(pair.code_verifier.len() >= 43);
        assert!(pair.code_verifier.len() <= 128);
    }

    #[test]
    fn pkce_challenge_matches_verifier() {
        let pair = generate_pkce();
        let mut hasher = Sha256::new();
        hasher.update(pair.code_verifier.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(pair.code_challenge, expected);
    }

    #[test]
    fn pkce_pairs_unique() {
        let a = generate_pkce();
        let b = generate_pkce();
        assert_ne!(a.code_verifier, b.code_verifier);
    }

    #[test]
    fn pkce_verifier_url_safe() {
        let pair = generate_pkce();
        // URL-safe base64 contains only: A-Z, a-z, 0-9, -, _
        assert!(pair.code_verifier.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
