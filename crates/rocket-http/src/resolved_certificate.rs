//! Client certificate material ready for the TLS layer.
//!
//! This is the runtime form of an environment's client certificate. It can hold key bytes and a
//! passphrase, so it is not `Serialize`, its secrets are wiped on drop, and its `Debug` shows
//! only the source kind and size.

use std::fmt;

use rocket_shared::certificate::VaultCertificateFormat;
use zeroize::Zeroizing;

/// The RocketVault connection and vault behind an External Secrets binding, copied from the
/// environment when the certificate is resolved. Names and ids only, never a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCertificateBinding {
    /// The binding alias, used in messages.
    pub alias: String,
    pub connection_id: String,
    pub vault_name: String,
}

/// Where one piece of material (certificate, private key or PKCS12 bundle) comes from.
#[derive(Clone)]
pub enum CertificateSource {
    /// An absolute path, or a `~/` path, read when the certificate is selected.
    File(String),
    /// The bytes themselves, for example PEM text or a PKCS12 bundle from a vault secret.
    Inline(Zeroizing<Vec<u8>>),
}

/// The material of one client certificate.
#[derive(Clone)]
pub enum CertificateMaterial {
    Pem {
        certificate: CertificateSource,
        private_key: CertificateSource,
        passphrase: Option<Zeroizing<String>>,
    },
    Pkcs12 {
        bundle: CertificateSource,
        passphrase: Option<Zeroizing<String>>,
    },
    /// The material could not be resolved. Selecting this certificate fails with `reason`.
    Unavailable { reason: String },
    /// A RocketVault certificate that `rocket-app` exports only when it is selected for a URL.
    /// It holds names, never material. The executor does not load it: one that reaches the
    /// executor is an error.
    Deferred {
        binding: VaultCertificateBinding,
        certificate: String,
        format: VaultCertificateFormat,
    },
}

/// A client certificate with its domain, ready for the executor.
#[derive(Clone)]
pub struct ResolvedClientCertificate {
    pub domain: String,
    pub material: CertificateMaterial,
}

impl ResolvedClientCertificate {
    pub fn pem(
        domain: impl Into<String>,
        certificate: CertificateSource,
        private_key: CertificateSource,
        passphrase: Option<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            material: CertificateMaterial::Pem {
                certificate,
                private_key,
                passphrase: passphrase.map(Zeroizing::new),
            },
        }
    }

    pub fn pkcs12(
        domain: impl Into<String>,
        bundle: CertificateSource,
        passphrase: Option<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            material: CertificateMaterial::Pkcs12 {
                bundle,
                passphrase: passphrase.map(Zeroizing::new),
            },
        }
    }

    pub fn unavailable(domain: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            material: CertificateMaterial::Unavailable {
                reason: reason.into(),
            },
        }
    }

    pub fn deferred(
        domain: impl Into<String>,
        binding: VaultCertificateBinding,
        certificate: impl Into<String>,
        format: VaultCertificateFormat,
    ) -> Self {
        Self {
            domain: domain.into(),
            material: CertificateMaterial::Deferred {
                binding,
                certificate: certificate.into(),
                format,
            },
        }
    }

    /// True while the material is a RocketVault certificate that has not been fetched yet.
    pub fn is_deferred(&self) -> bool {
        matches!(self.material, CertificateMaterial::Deferred { .. })
    }
}

/// Prints `<redacted>` in place of a passphrase.
struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

// Hand-written so a `{:?}` of a request never prints key bytes.
impl fmt::Debug for CertificateSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CertificateSource::File(path) => write!(f, "file {path}"),
            CertificateSource::Inline(bytes) => write!(f, "inline {} bytes", bytes.len()),
        }
    }
}

// Hand-written so a `{:?}` of a request never prints a passphrase.
impl fmt::Debug for CertificateMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redact = |p: &Option<Zeroizing<String>>| p.as_ref().map(|_| Redacted);
        match self {
            CertificateMaterial::Pem {
                certificate,
                private_key,
                passphrase,
            } => f
                .debug_struct("Pem")
                .field("certificate", certificate)
                .field("private_key", private_key)
                .field("passphrase", &redact(passphrase))
                .finish(),
            CertificateMaterial::Pkcs12 { bundle, passphrase } => f
                .debug_struct("Pkcs12")
                .field("bundle", bundle)
                .field("passphrase", &redact(passphrase))
                .finish(),
            CertificateMaterial::Unavailable { reason } => f
                .debug_struct("Unavailable")
                .field("reason", reason)
                .finish(),
            CertificateMaterial::Deferred {
                binding,
                certificate,
                format,
            } => f
                .debug_struct("Deferred")
                .field("binding", &binding.alias)
                .field("certificate", certificate)
                .field("format", &format.as_str())
                .finish(),
        }
    }
}

impl fmt::Debug for ResolvedClientCertificate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedClientCertificate")
            .field("domain", &self.domain)
            .field("material", &self.material)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] =
        b"-----BEGIN PRIVATE KEY-----\nc2VjcmV0LWtleS1ieXRlcw==\n-----END PRIVATE KEY-----\n";

    fn inline(bytes: &[u8]) -> CertificateSource {
        CertificateSource::Inline(Zeroizing::new(bytes.to_vec()))
    }

    #[test]
    fn debug_shows_sources_but_never_key_bytes_or_the_passphrase() {
        let cert = ResolvedClientCertificate::pem(
            "api.example.com",
            CertificateSource::File("/certs/client.pem".into()),
            inline(KEY),
            Some("hunter2".into()),
        );
        let shown = format!("{cert:?}");
        assert!(shown.contains("api.example.com"), "{shown}");
        assert!(shown.contains("file /certs/client.pem"), "{shown}");
        assert!(
            shown.contains(&format!("inline {} bytes", KEY.len())),
            "{shown}"
        );
        assert!(shown.contains("<redacted>"), "{shown}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(!shown.contains("BEGIN"), "{shown}");
        assert!(!shown.contains("c2VjcmV0"), "{shown}");
    }

    #[test]
    fn debug_of_a_pkcs12_bundle_prints_only_its_size() {
        let cert = ResolvedClientCertificate::pkcs12(
            "*.example.com",
            inline(&[0x30, 0x82, 0x01, 0x02]),
            Some("changeit".into()),
        );
        let shown = format!("{cert:#?}");
        assert!(shown.contains("Pkcs12"), "{shown}");
        assert!(shown.contains("inline 4 bytes"), "{shown}");
        assert!(!shown.contains("changeit"), "{shown}");
        // A byte vector would print as `[48, 130, ...]`.
        assert!(!shown.contains('['), "{shown}");
    }

    #[test]
    fn an_unavailable_certificate_shows_its_domain_and_reason() {
        let cert = ResolvedClientCertificate::unavailable(
            "api.example.com",
            "Client certificate secret vault.clientCertPem was not found.",
        );
        let shown = format!("{cert:?}");
        assert!(shown.contains("Unavailable"), "{shown}");
        assert!(shown.contains("vault.clientCertPem"), "{shown}");
    }

    #[test]
    fn constructors_keep_the_domain_and_wrap_the_passphrase() {
        let cert = ResolvedClientCertificate::pkcs12(
            "api.example.com",
            CertificateSource::File("/c.p12".into()),
            Some("pw".into()),
        );
        assert_eq!(cert.domain, "api.example.com");
        match &cert.material {
            CertificateMaterial::Pkcs12 {
                bundle: CertificateSource::File(path),
                passphrase: Some(p),
            } => {
                assert_eq!(path, "/c.p12");
                assert_eq!(p.as_str(), "pw");
            }
            other => panic!("unexpected material {other:?}"),
        }
        let none = ResolvedClientCertificate::pem(
            "a",
            CertificateSource::File("/c.pem".into()),
            CertificateSource::File("/k.pem".into()),
            None,
        );
        assert!(matches!(
            none.material,
            CertificateMaterial::Pem {
                passphrase: None,
                ..
            }
        ));
    }

    fn prod_binding() -> VaultCertificateBinding {
        VaultCertificateBinding {
            alias: "prod".into(),
            connection_id: "conn-1".into(),
            vault_name: "prod-vault".into(),
        }
    }

    #[test]
    fn a_deferred_certificate_shows_its_names_and_holds_no_material() {
        let cert = ResolvedClientCertificate::deferred(
            "api.example.com",
            prod_binding(),
            "client-a",
            VaultCertificateFormat::Pkcs12,
        );
        assert_eq!(cert.domain, "api.example.com");
        match &cert.material {
            CertificateMaterial::Deferred {
                binding,
                certificate,
                format,
            } => {
                assert_eq!(binding, &prod_binding());
                assert_eq!(certificate, "client-a");
                assert_eq!(*format, VaultCertificateFormat::Pkcs12);
            }
            other => panic!("unexpected material {other:?}"),
        }
        let shown = format!("{cert:?}");
        assert!(shown.contains("Deferred"), "{shown}");
        assert!(shown.contains("prod"), "{shown}");
        assert!(shown.contains("client-a"), "{shown}");
        assert!(shown.contains("pkcs12"), "{shown}");
    }

    #[test]
    fn is_deferred_is_true_only_for_deferred_material() {
        let deferred = ResolvedClientCertificate::deferred(
            "a",
            prod_binding(),
            "client-a",
            VaultCertificateFormat::Pem,
        );
        assert!(deferred.is_deferred());
        assert!(!ResolvedClientCertificate::unavailable("a", "gone").is_deferred());
        assert!(!ResolvedClientCertificate::pkcs12("a", inline(&[1, 2]), None).is_deferred());
    }
}
