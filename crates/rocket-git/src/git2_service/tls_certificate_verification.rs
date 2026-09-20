use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use url::Url;

use crate::TlsCertificateFailure;

const DEFAULT_HTTPS_PORT: u16 = 443;

/// Resolve the port to report for a certificate failure from the remote's own
/// URL, falling back to the default HTTPS port if it cannot be parsed or
/// carries no explicit port.
pub(super) fn https_port(remote: &str) -> u16 {
    Url::parse(remote)
        .ok()
        .and_then(|url| url.port())
        .unwrap_or(DEFAULT_HTTPS_PORT)
}

/// Format a DER-encoded certificate's SHA-256 digest using the same
/// `SHA256:<base64>` convention as SSH host-key fingerprints.
pub(super) fn sha256_fingerprint(der: &[u8]) -> String {
    format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(der)))
}

/// Build the diagnostic details for a certificate that libgit2/OpenSSL has
/// already rejected. This never decides accept/reject — it only describes the
/// certificate that native verification failed, for display and error
/// reporting.
pub(super) fn classify_tls_certificate(
    remote: &str,
    host: &str,
    der: &[u8],
) -> TlsCertificateFailure {
    TlsCertificateFailure {
        host: host.to_owned(),
        port: https_port(remote),
        fingerprint: sha256_fingerprint(der),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_port_prefers_explicit_url_port() {
        assert_eq!(https_port("https://git.example.com:8443/repo.git"), 8443);
    }

    #[test]
    fn https_port_defaults_to_443_without_explicit_port() {
        assert_eq!(https_port("https://git.example.com/repo.git"), 443);
    }

    #[test]
    fn https_port_defaults_to_443_for_unparseable_url() {
        assert_eq!(https_port("not a url"), 443);
    }

    #[test]
    fn sha256_fingerprint_matches_known_digest() {
        assert_eq!(
            sha256_fingerprint(b""),
            "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU"
        );
    }

    #[test]
    fn classify_tls_certificate_reports_host_port_and_fingerprint() {
        let failure = classify_tls_certificate(
            "https://git.example.com:8443/repo.git",
            "git.example.com",
            b"der bytes",
        );
        assert_eq!(failure.host, "git.example.com");
        assert_eq!(failure.port, 8443);
        assert_eq!(failure.fingerprint, sha256_fingerprint(b"der bytes"));
    }
}
