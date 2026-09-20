use std::fmt;

use serde::{Deserialize, Serialize};

/// The reason an SSH host key could not be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SshHostFailureKind {
    UnknownHost,
    ChangedHost,
    VerificationUnavailable,
}

impl fmt::Display for SshHostFailureKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownHost => formatter.write_str("unknown SSH host"),
            Self::ChangedHost => formatter.write_str("changed SSH host key"),
            Self::VerificationUnavailable => {
                formatter.write_str("SSH host verification unavailable")
            }
        }
    }
}

/// Details of an SSH host-key verification failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshHostFailure {
    pub kind: SshHostFailureKind,
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    /// OpenSSH-style SHA-256 fingerprint, including the `SHA256:` prefix.
    pub fingerprint: String,
}

impl fmt::Display for SshHostFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} for {}:{} (algorithm {}, fingerprint {})",
            self.kind, self.host, self.port, self.algorithm, self.fingerprint
        )
    }
}

/// Details of a TLS/HTTPS certificate that failed native verification.
///
/// Unlike SSH host keys, X.509 verification has no offline "known" state to
/// classify against here — native verification (OS/OpenSSL trust store) is
/// the sole authority, so this carries only the identifying details needed
/// to display and diagnose the rejected certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsCertificateFailure {
    pub host: String,
    pub port: u16,
    /// SHA-256 fingerprint of the DER-encoded certificate, formatted like
    /// `SshHostFailure::fingerprint`.
    pub fingerprint: String,
}

impl fmt::Display for TlsCertificateFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid TLS certificate for {}:{} (fingerprint {})",
            self.host, self.port, self.fingerprint
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_certificate_failure_serializes_with_camel_case_names() {
        let failure = TlsCertificateFailure {
            host: "git.example.com".to_owned(),
            port: 443,
            fingerprint: "SHA256:abc".to_owned(),
        };

        let value = serde_json::to_value(failure).expect("failure should serialize");
        assert_eq!(value["host"], "git.example.com");
        assert_eq!(value["port"], 443);
        assert_eq!(value["fingerprint"], "SHA256:abc");
    }

    #[test]
    fn tls_certificate_failure_display_contains_actionable_details() {
        let failure = TlsCertificateFailure {
            host: "git.example.com".to_owned(),
            port: 443,
            fingerprint: "SHA256:abc".to_owned(),
        };

        assert_eq!(
            failure.to_string(),
            "invalid TLS certificate for git.example.com:443 (fingerprint SHA256:abc)"
        );
    }

    #[test]
    fn serializes_with_camel_case_names_and_kind() {
        let failure = SshHostFailure {
            kind: SshHostFailureKind::VerificationUnavailable,
            host: "git.example.com".to_owned(),
            port: 2222,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:abc".to_owned(),
        };

        let value = serde_json::to_value(failure).expect("failure should serialize");
        assert_eq!(value["kind"], "verificationUnavailable");
        assert_eq!(value["host"], "git.example.com");
        assert_eq!(value["port"], 2222);
        assert_eq!(value["algorithm"], "ssh-ed25519");
        assert_eq!(value["fingerprint"], "SHA256:abc");
    }

    #[test]
    fn display_contains_actionable_host_key_details() {
        let failure = SshHostFailure {
            kind: SshHostFailureKind::ChangedHost,
            host: "git.example.com".to_owned(),
            port: 22,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:abc".to_owned(),
        };

        assert_eq!(
            failure.to_string(),
            "changed SSH host key for git.example.com:22 (algorithm ssh-ed25519, fingerprint SHA256:abc)"
        );
    }
}
