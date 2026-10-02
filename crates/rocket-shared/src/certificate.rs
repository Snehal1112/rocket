use serde::{Deserialize, Serialize};

/// How a RocketVault certificate is exported: PEM text (the certificate chain and an
/// unencrypted PKCS#8 key) or a PKCS12 bundle. Persisted as `pem` or `pkcs12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultCertificateFormat {
    #[default]
    Pem,
    Pkcs12,
}

impl VaultCertificateFormat {
    /// The persisted and wire name: `pem` or `pkcs12`.
    pub fn as_str(self) -> &'static str {
        match self {
            VaultCertificateFormat::Pem => "pem",
            VaultCertificateFormat::Pkcs12 => "pkcs12",
        }
    }
}

/// Client certificate — PEM, PKCS12 or RocketVault, discriminated by the `type` field.
///
/// For PEM and PKCS12, each piece of material comes from a file path or from a RocketVault
/// reference (`alias.secretName`, never a value). A path that is not used is empty and is not
/// written. The `*Secret` keys are Rocket extensions outside the OpenCollection schema, like
/// `externalSecrets`. The whole `vault` type is a Rocket extension too: it names a certificate
/// that RocketVault exports at send time, and stores names only.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientCertificate {
    #[serde(rename = "pem", rename_all = "camelCase")]
    Pem {
        domain: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        certificate_file_path: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        private_key_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        certificate_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        private_key_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    #[serde(rename = "pkcs12", rename_all = "camelCase")]
    Pkcs12 {
        domain: String,
        #[serde(
            rename = "pkcs12FilePath",
            default,
            skip_serializing_if = "String::is_empty"
        )]
        pkcs12_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkcs12_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    /// A certificate that RocketVault exports when it is selected. `binding` is an External
    /// Secrets alias of this environment (the connection and vault come from it) and
    /// `certificate` is the certificate name in that vault. No id, password, key or path is
    /// stored.
    #[serde(rename = "vault", rename_all = "camelCase")]
    Vault {
        domain: String,
        binding: String,
        certificate: String,
        #[serde(default)]
        format: VaultCertificateFormat,
    },
}

impl ClientCertificate {
    /// The domain this certificate is presented to.
    pub fn domain(&self) -> &str {
        match self {
            ClientCertificate::Pem { domain, .. }
            | ClientCertificate::Pkcs12 { domain, .. }
            | ClientCertificate::Vault { domain, .. } => domain,
        }
    }
}

// Hand-written so a `{:?}` of a request or environment never prints a passphrase. References
// are names, not values, so they are shown.
impl std::fmt::Debug for ClientCertificate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redact = |p: &Option<String>| p.as_ref().map(|_| "<redacted>");
        match self {
            ClientCertificate::Pem {
                domain,
                certificate_file_path,
                private_key_file_path,
                certificate_secret,
                private_key_secret,
                passphrase,
            } => f
                .debug_struct("Pem")
                .field("domain", domain)
                .field("certificate_file_path", certificate_file_path)
                .field("private_key_file_path", private_key_file_path)
                .field("certificate_secret", certificate_secret)
                .field("private_key_secret", private_key_secret)
                .field("passphrase", &redact(passphrase))
                .finish(),
            ClientCertificate::Pkcs12 {
                domain,
                pkcs12_file_path,
                pkcs12_secret,
                passphrase,
            } => f
                .debug_struct("Pkcs12")
                .field("domain", domain)
                .field("pkcs12_file_path", pkcs12_file_path)
                .field("pkcs12_secret", pkcs12_secret)
                .field("passphrase", &redact(passphrase))
                .finish(),
            ClientCertificate::Vault {
                domain,
                binding,
                certificate,
                format,
            } => f
                .debug_struct("Vault")
                .field("domain", domain)
                .field("binding", binding)
                .field("certificate", certificate)
                .field("format", format)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_contains_the_passphrase() {
        let cert = ClientCertificate::Pkcs12 {
            domain: "a.example.com".into(),
            pkcs12_file_path: "/c.p12".into(),
            pkcs12_secret: None,
            passphrase: Some("hunter2".into()),
        };
        let shown = format!("{cert:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }

    #[test]
    fn pem_certificate_serde() {
        let cert = ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: "/certs/client.pem".into(),
            private_key_file_path: "/certs/client-key.pem".into(),
            certificate_secret: None,
            private_key_secret: None,
            passphrase: None,
        };
        let json = serde_json::to_string(&cert).unwrap();
        assert!(json.contains("\"type\":\"pem\""));
        assert!(json.contains("\"certificateFilePath\""));
        let back: ClientCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(cert, back);
    }

    #[test]
    fn pkcs12_certificate_serde() {
        let cert = ClientCertificate::Pkcs12 {
            domain: "secure.example.com".into(),
            pkcs12_file_path: "/certs/client.p12".into(),
            pkcs12_secret: None,
            passphrase: Some("secret".into()),
        };
        let json = serde_json::to_string(&cert).unwrap();
        assert!(json.contains("\"type\":\"pkcs12\""));
        assert!(json.contains("\"pkcs12FilePath\""));
        assert!(json.contains("\"passphrase\":\"secret\""));
        let back: ClientCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(cert, back);
    }

    #[test]
    fn pem_with_passphrase() {
        let cert = ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: "/certs/client.pem".into(),
            private_key_file_path: "/certs/client-key.pem".into(),
            certificate_secret: None,
            private_key_secret: None,
            passphrase: Some("my-pass".into()),
        };
        let json = serde_json::to_string(&cert).unwrap();
        let back: ClientCertificate = serde_json::from_str(&json).unwrap();
        assert_eq!(cert, back);
    }

    #[test]
    fn certificate_dispatch_on_type() {
        let pem_json = r#"{"type":"pem","domain":"a.com","certificateFilePath":"/c.pem","privateKeyFilePath":"/k.pem"}"#;
        let pkcs_json = r#"{"type":"pkcs12","domain":"b.com","pkcs12FilePath":"/c.p12"}"#;

        let pem: ClientCertificate = serde_json::from_str(pem_json).unwrap();
        assert!(matches!(pem, ClientCertificate::Pem { .. }));

        let pkcs: ClientCertificate = serde_json::from_str(pkcs_json).unwrap();
        assert!(matches!(pkcs, ClientCertificate::Pkcs12 { .. }));
    }

    #[test]
    fn a_vault_sourced_pem_writes_only_its_references() {
        let cert = ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: Some("{{vault.clientKeyPass}}".into()),
        };
        let json = serde_json::to_string(&cert).expect("serialize");
        assert!(!json.contains("FilePath"), "{json}");
        assert!(
            json.contains("\"certificateSecret\":\"vault.clientCertPem\""),
            "{json}"
        );
        assert!(
            json.contains("\"privateKeySecret\":\"vault.clientKeyPem\""),
            "{json}"
        );
        let back: ClientCertificate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cert, back);
    }

    #[test]
    fn a_vault_sourced_pkcs12_writes_only_its_reference() {
        let cert = ClientCertificate::Pkcs12 {
            domain: "*.internal.example.com".into(),
            pkcs12_file_path: String::new(),
            pkcs12_secret: Some("vault.clientBundleB64".into()),
            passphrase: None,
        };
        let json = serde_json::to_string(&cert).expect("serialize");
        assert!(!json.contains("pkcs12FilePath"), "{json}");
        assert!(
            json.contains("\"pkcs12Secret\":\"vault.clientBundleB64\""),
            "{json}"
        );
        let back: ClientCertificate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cert, back);
    }

    #[test]
    fn an_old_file_only_entry_loads_and_round_trips_unchanged() {
        let old_entries = [
            r#"{"type":"pem","domain":"a.com","certificateFilePath":"/c.pem","privateKeyFilePath":"/k.pem"}"#,
            r#"{"type":"pkcs12","domain":"b.com","pkcs12FilePath":"/c.p12","passphrase":"{{p}}"}"#,
        ];
        for old in old_entries {
            let cert: ClientCertificate = serde_json::from_str(old).expect("old entry loads");
            let written = serde_json::to_value(&cert).expect("serialize");
            let original: serde_json::Value = serde_json::from_str(old).expect("parse");
            assert_eq!(written, original, "{old}");
        }
    }

    #[test]
    fn domain_returns_the_domain_of_either_variant() {
        let pem: ClientCertificate = serde_json::from_str(
            r#"{"type":"pem","domain":"a.com","certificateFilePath":"/c.pem","privateKeyFilePath":"/k.pem"}"#,
        )
        .expect("pem");
        let p12: ClientCertificate =
            serde_json::from_str(r#"{"type":"pkcs12","domain":"b.com","pkcs12Secret":"v.b"}"#)
                .expect("pkcs12");
        assert_eq!(pem.domain(), "a.com");
        assert_eq!(p12.domain(), "b.com");
    }

    #[test]
    fn debug_shows_references_but_never_the_passphrase() {
        let cert = ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: Some("hunter2".into()),
        };
        let shown = format!("{cert:?}");
        assert!(shown.contains("vault.clientCertPem"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }

    #[test]
    fn vault_certificate_format_uses_lowercase_names_and_defaults_to_pem() {
        assert_eq!(
            serde_json::to_string(&VaultCertificateFormat::Pkcs12).expect("serialize"),
            "\"pkcs12\""
        );
        assert_eq!(
            serde_json::from_str::<VaultCertificateFormat>("\"pem\"").expect("deserialize"),
            VaultCertificateFormat::Pem
        );
        assert_eq!(VaultCertificateFormat::default(), VaultCertificateFormat::Pem);
        assert!(serde_json::from_str::<VaultCertificateFormat>("\"der\"").is_err());
        assert_eq!(VaultCertificateFormat::Pem.as_str(), "pem");
        assert_eq!(VaultCertificateFormat::Pkcs12.as_str(), "pkcs12");
    }

    fn vault_entry() -> ClientCertificate {
        ClientCertificate::Vault {
            domain: "api.example.com".into(),
            binding: "prod".into(),
            certificate: "client-a".into(),
            format: VaultCertificateFormat::Pkcs12,
        }
    }

    #[test]
    fn a_vault_entry_round_trips_with_its_names_and_format() {
        let json = serde_json::to_string(&vault_entry()).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"vault","domain":"api.example.com","binding":"prod","certificate":"client-a","format":"pkcs12"}"#
        );
        let back: ClientCertificate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, vault_entry());
    }

    #[test]
    fn a_vault_entry_without_a_format_defaults_to_pem() {
        let cert: ClientCertificate = serde_json::from_str(
            r#"{"type":"vault","domain":"a.com","binding":"prod","certificate":"client-a"}"#,
        )
        .expect("format is optional");
        assert!(matches!(
            cert,
            ClientCertificate::Vault {
                format: VaultCertificateFormat::Pem,
                ..
            }
        ));
    }

    #[test]
    fn a_vault_entry_with_an_unknown_format_is_rejected() {
        let result = serde_json::from_str::<ClientCertificate>(
            r#"{"type":"vault","domain":"a.com","binding":"prod","certificate":"c","format":"der"}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn domain_and_debug_cover_a_vault_entry() {
        let cert = vault_entry();
        assert_eq!(cert.domain(), "api.example.com");
        let shown = format!("{cert:?}");
        assert!(shown.contains("Vault"), "{shown}");
        assert!(shown.contains("prod") && shown.contains("client-a"), "{shown}");
    }
}
