//! Save-time checks for an environment's client certificates.
//!
//! Every piece of material needs exactly one source, a reference must name a bound secret, and
//! no path or reference field may hold key text, so a private key never lands in the
//! environment file or in git. A `vault` entry must name one of the environment's External
//! Secrets aliases and a certificate, and none of its fields may hold key text either.

use rocket_shared::certificate::ClientCertificate;
use rocket_shared::error::{DomainError, DomainResult};

use crate::external_secret::ExternalSecretBinding;
use crate::secret_manager::ProviderCapabilityLookup;

const PEM_MARKER: &str = "-----BEGIN";

/// Checks the rules of spec section 4. Certificates are numbered from 1 in messages, because
/// the domain can be the thing that is missing.
pub fn validate_client_certificates(
    certs: &[ClientCertificate],
    bindings: &[ExternalSecretBinding],
) -> DomainResult<()> {
    for (index, cert) in certs.iter().enumerate() {
        let entry = index + 1;
        if cert.domain().trim().is_empty() {
            return Err(invalid(format!(
                "Client certificate {entry} needs a domain."
            )));
        }
        match cert {
            ClientCertificate::Pem {
                certificate_file_path,
                private_key_file_path,
                certificate_secret,
                private_key_secret,
                ..
            } => {
                check_piece(
                    entry,
                    ("certificateFilePath", certificate_file_path.as_str()),
                    ("certificateSecret", certificate_secret.as_deref()),
                    bindings,
                )?;
                check_piece(
                    entry,
                    ("privateKeyFilePath", private_key_file_path.as_str()),
                    ("privateKeySecret", private_key_secret.as_deref()),
                    bindings,
                )?;
            }
            ClientCertificate::Pkcs12 {
                pkcs12_file_path,
                pkcs12_secret,
                ..
            } => {
                check_piece(
                    entry,
                    ("pkcs12FilePath", pkcs12_file_path.as_str()),
                    ("pkcs12Secret", pkcs12_secret.as_deref()),
                    bindings,
                )?;
            }
            ClientCertificate::Vault {
                domain,
                binding,
                certificate,
                ..
            } => {
                check_vault_entry(entry, domain, binding, certificate, bindings)?;
            }
        }
    }
    Ok(())
}

/// Checks one piece: no key text, then exactly one source, then the reference itself.
fn check_piece(
    entry: usize,
    (path_field, path): (&str, &str),
    (secret_field, secret): (&str, Option<&str>),
    bindings: &[ExternalSecretBinding],
) -> DomainResult<()> {
    reject_key_text(entry, path_field, path)?;
    if let Some(reference) = secret {
        reject_key_text(entry, secret_field, reference)?;
    }
    let secret = secret.map(str::trim).filter(|s| !s.is_empty());
    match (path.trim().is_empty(), secret) {
        (false, Some(_)) => Err(invalid(format!(
            "Client certificate {entry}: set either {path_field} or {secret_field}, not both."
        ))),
        (true, None) => Err(invalid(format!(
            "Client certificate {entry}: set {path_field} or {secret_field}."
        ))),
        (false, None) => Ok(()),
        (true, Some(reference)) => check_reference(entry, secret_field, reference, bindings),
    }
}

fn reject_key_text(entry: usize, field: &str, value: &str) -> DomainResult<()> {
    if value.trim_start().starts_with(PEM_MARKER) {
        return Err(invalid(format!(
            "Client certificate {entry}: field {field} must be a file path or a vault secret \
             reference, not key text."
        )));
    }
    Ok(())
}

/// A `vault` entry names a bound alias and a certificate. The format needs no check: serde
/// already rejects anything but `pem` and `pkcs12`. The alias is compared exactly, like the
/// lookup at send time, so a value that passes here also resolves there.
fn check_vault_entry(
    entry: usize,
    domain: &str,
    binding: &str,
    certificate: &str,
    bindings: &[ExternalSecretBinding],
) -> DomainResult<()> {
    reject_key_text_in_name(entry, "domain", domain)?;
    reject_key_text_in_name(entry, "binding", binding)?;
    reject_key_text_in_name(entry, "certificate", certificate)?;
    if binding.trim().is_empty() {
        return Err(invalid(format!(
            "Client certificate {entry}: set binding to an External Secrets alias of this \
             environment."
        )));
    }
    if !bindings.iter().any(|b| b.alias == binding) {
        return Err(invalid(format!(
            "Client certificate {entry}: binding {binding} has no External Secrets binding in \
             this environment."
        )));
    }
    if certificate.trim().is_empty() {
        return Err(invalid(format!(
            "Client certificate {entry}: set certificate to the certificate name in the vault."
        )));
    }
    Ok(())
}

/// Rejects key text in a field that holds a name, without echoing the value.
fn reject_key_text_in_name(entry: usize, field: &str, value: &str) -> DomainResult<()> {
    if value.trim_start().starts_with(PEM_MARKER) {
        return Err(invalid(format!(
            "Client certificate {entry}: field {field} must be a name, not key text."
        )));
    }
    Ok(())
}

/// A reference is `alias.secretName`: the alias is bound in this environment and the name is
/// one of the secret names fetched for that binding.
fn check_reference(
    entry: usize,
    field: &str,
    reference: &str,
    bindings: &[ExternalSecretBinding],
) -> DomainResult<()> {
    let (alias, name) = match reference.split_once('.') {
        Some((alias, name)) if !alias.is_empty() && !name.is_empty() => (alias, name),
        _ => {
            return Err(invalid(format!(
                "Client certificate {entry}: {field} must have the form alias.secretName."
            )))
        }
    };
    let Some(binding) = bindings.iter().find(|b| b.alias == alias) else {
        return Err(invalid(format!(
            "Client certificate {entry}: {field} uses the alias {alias}, which has no External \
             Secrets binding in this environment."
        )));
    };
    if !binding.secret_names.iter().any(|s| s.name == name) {
        return Err(invalid(format!(
            "Client certificate {entry}: {field} names the secret {name}, which is not in the \
             {alias} binding. Fetch the secret names first."
        )));
    }
    Ok(())
}

/// A `vault` entry needs a binding whose connection can supply client
/// certificates. Only RocketVault can. A binding whose connection no longer
/// exists is skipped, because that case already has its own warning in the
/// External Secrets tab. Entries of other types are never checked.
pub fn validate_vault_certificate_providers(
    certs: &[ClientCertificate],
    bindings: &[ExternalSecretBinding],
    lookup: &dyn ProviderCapabilityLookup,
) -> DomainResult<()> {
    for (index, cert) in certs.iter().enumerate() {
        let ClientCertificate::Vault { binding, .. } = cert else {
            continue;
        };
        let Some(bound) = bindings.iter().find(|b| b.alias == *binding) else {
            continue;
        };
        let Some(info) = lookup.provider_of(&bound.connection_id)? else {
            continue;
        };
        if !info.capabilities.certificates {
            return Err(invalid(format!(
                "Client certificate {}: {} cannot supply client certificates. Use a RocketVault \
                 binding, or reference the certificate as a secret instead.",
                index + 1,
                info.kind.display_name()
            )));
        }
    }
    Ok(())
}

fn invalid(message: String) -> DomainError {
    DomainError::InvalidInput(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::external_secret::ExternalSecretRef;

    const KEY_TEXT: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQsecret\n-----END PRIVATE KEY-----";

    fn bindings() -> Vec<ExternalSecretBinding> {
        vec![ExternalSecretBinding {
            alias: "vault".to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            secret_names: ["clientCertPem", "clientKeyPem", "clientBundleB64"]
                .iter()
                .map(|name| ExternalSecretRef {
                    name: name.to_string(),
                    secret_id: format!("id-{name}"),
                })
                .collect(),
        }]
    }

    fn pem(
        domain: &str,
        cert_path: &str,
        key_path: &str,
        cert_secret: Option<&str>,
        key_secret: Option<&str>,
    ) -> ClientCertificate {
        ClientCertificate::Pem {
            domain: domain.to_string(),
            certificate_file_path: cert_path.to_string(),
            private_key_file_path: key_path.to_string(),
            certificate_secret: cert_secret.map(String::from),
            private_key_secret: key_secret.map(String::from),
            passphrase: Some("{{vault.clientKeyPass}}".to_string()),
        }
    }

    fn pkcs12(domain: &str, path: &str, secret: Option<&str>) -> ClientCertificate {
        ClientCertificate::Pkcs12 {
            domain: domain.to_string(),
            pkcs12_file_path: path.to_string(),
            pkcs12_secret: secret.map(String::from),
            passphrase: None,
        }
    }

    fn vault(domain: &str, binding: &str, certificate: &str) -> ClientCertificate {
        ClientCertificate::Vault {
            domain: domain.to_string(),
            binding: binding.to_string(),
            certificate: certificate.to_string(),
            format: rocket_shared::certificate::VaultCertificateFormat::Pem,
        }
    }

    fn message(result: DomainResult<()>) -> String {
        match result.expect_err("must reject") {
            DomainError::InvalidInput(m) => m,
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn accepts_file_and_vault_sources() {
        let certs = [
            pem("api.example.com", "certs/client.pem", "/k.pem", None, None),
            pem(
                "api.example.com",
                "",
                "",
                Some("vault.clientCertPem"),
                Some("vault.clientKeyPem"),
            ),
            pkcs12("*.internal.example.com", "", Some("vault.clientBundleB64")),
            pkcs12("b.example.com", "{{certDir}}/client.p12", None),
        ];
        assert!(validate_client_certificates(&certs, &bindings()).is_ok());
        assert!(validate_client_certificates(&[], &[]).is_ok());
    }

    #[test]
    fn rejects_an_empty_domain() {
        let msg = message(validate_client_certificates(
            &[pem("  ", "/c.pem", "/k.pem", None, None)],
            &bindings(),
        ));
        assert!(msg.contains("domain"), "{msg}");
    }

    #[test]
    fn rejects_a_piece_with_no_source() {
        let msg = message(validate_client_certificates(
            &[pem("a.example.com", "/c.pem", "", None, None)],
            &bindings(),
        ));
        assert!(
            msg.contains("privateKeyFilePath") && msg.contains("privateKeySecret"),
            "{msg}"
        );
    }

    #[test]
    fn rejects_a_piece_with_two_sources() {
        let msg = message(validate_client_certificates(
            &[pkcs12(
                "a.example.com",
                "/c.p12",
                Some("vault.clientBundleB64"),
            )],
            &bindings(),
        ));
        assert!(
            msg.contains("pkcs12FilePath")
                && msg.contains("pkcs12Secret")
                && msg.contains("not both"),
            "{msg}"
        );
    }

    #[test]
    fn rejects_a_reference_that_is_not_alias_dot_secret_name() {
        for bad in ["clientCertPem", ".clientCertPem", "vault."] {
            let msg = message(validate_client_certificates(
                &[pem("a.example.com", "", "/k.pem", Some(bad), None)],
                &bindings(),
            ));
            assert!(
                msg.contains("certificateSecret") && msg.contains("alias.secretName"),
                "{bad}: {msg}"
            );
        }
    }

    // Review Focus 5.
    #[test]
    fn rejects_a_reference_with_no_matching_binding() {
        let msg = message(validate_client_certificates(
            &[pem(
                "a.example.com",
                "",
                "/k.pem",
                Some("payments.clientCertPem"),
                None,
            )],
            &bindings(),
        ));
        assert!(
            msg.contains("certificateSecret") && msg.contains("payments"),
            "{msg}"
        );

        let msg = message(validate_client_certificates(
            &[pkcs12("a.example.com", "", Some("vault.otherBundle"))],
            &bindings(),
        ));
        assert!(
            msg.contains("pkcs12Secret") && msg.contains("otherBundle") && msg.contains("Fetch"),
            "{msg}"
        );
    }

    // Review Focus 5.
    #[test]
    fn rejects_key_text_in_a_path_or_reference_field_and_names_the_field() {
        let indented = format!("  {KEY_TEXT}");
        let cases = [
            (
                pem("a.example.com", KEY_TEXT, "/k.pem", None, None),
                "certificateFilePath",
            ),
            (
                pem("a.example.com", "/c.pem", &indented, None, None),
                "privateKeyFilePath",
            ),
            (
                pem("a.example.com", "/c.pem", "", None, Some(KEY_TEXT)),
                "privateKeySecret",
            ),
            (pkcs12("a.example.com", KEY_TEXT, None), "pkcs12FilePath"),
            (pkcs12("a.example.com", "", Some(KEY_TEXT)), "pkcs12Secret"),
        ];
        for (cert, field) in cases {
            let msg = message(validate_client_certificates(&[cert], &bindings()));
            assert!(
                msg.contains(field) && msg.contains("not key text"),
                "{field}: {msg}"
            );
            assert!(
                !msg.contains("MIIEvQsecret"),
                "the key must not be echoed: {msg}"
            );
        }
    }

    #[test]
    fn accepts_a_vault_entry_with_a_bound_alias_and_a_certificate_name() {
        let certs = [vault("api.example.com", "vault", "client-a")];
        assert!(validate_client_certificates(&certs, &bindings()).is_ok());
    }

    #[test]
    fn rejects_a_vault_entry_whose_binding_is_not_in_the_environment() {
        let msg = message(validate_client_certificates(
            &[vault("api.example.com", "payments", "client-a")],
            &bindings(),
        ));
        assert!(
            msg.contains("binding payments") && msg.contains("no External Secrets binding"),
            "{msg}"
        );
    }

    #[test]
    fn rejects_a_vault_entry_with_no_binding_or_no_certificate_name() {
        let msg = message(validate_client_certificates(
            &[vault("api.example.com", "  ", "client-a")],
            &bindings(),
        ));
        assert!(msg.contains("set binding"), "{msg}");

        let msg = message(validate_client_certificates(
            &[vault("api.example.com", "vault", " ")],
            &bindings(),
        ));
        assert!(msg.contains("set certificate"), "{msg}");
    }

    #[test]
    fn rejects_key_text_in_any_vault_field_without_echoing_it() {
        let cases = [
            (vault(KEY_TEXT, "vault", "client-a"), "domain"),
            (vault("a.example.com", KEY_TEXT, "client-a"), "binding"),
            (vault("a.example.com", "vault", KEY_TEXT), "certificate"),
        ];
        for (cert, field) in cases {
            let msg = message(validate_client_certificates(&[cert], &bindings()));
            assert!(
                msg.contains(&format!("field {field}")) && msg.contains("not key text"),
                "{field}: {msg}"
            );
            assert!(
                !msg.contains("MIIEvQsecret"),
                "the key must not be echoed: {msg}"
            );
        }
    }

    use crate::secret_manager::{ConnectionProvider, ProviderCapabilityLookup, SecretProviderKind};
    use crate::vault_secret_fetcher::ProviderCapabilities;
    struct FixedLookup(Option<ConnectionProvider>);
    impl ProviderCapabilityLookup for FixedLookup {
        fn provider_of(&self, _id: &str) -> DomainResult<Option<ConnectionProvider>> {
            Ok(self.0.clone())
        }
    }

    fn vault_entry() -> Vec<ClientCertificate> {
        vec![vault("api.example.com", "vault", "client")]
    }

    fn provider(kind: SecretProviderKind, certificates: bool) -> Option<ConnectionProvider> {
        Some(ConnectionProvider {
            kind,
            capabilities: ProviderCapabilities {
                certificates,
                ..ProviderCapabilities::default()
            },
        })
    }

    #[test]
    fn a_vault_entry_on_a_certificate_capable_provider_is_accepted() {
        let lookup = FixedLookup(provider(SecretProviderKind::RocketVault, true));
        validate_vault_certificate_providers(&vault_entry(), &bindings(), &lookup)
            .expect("rocketvault supplies certificates");
    }

    #[test]
    fn a_vault_entry_on_another_provider_is_rejected_naming_the_provider() {
        let lookup = FixedLookup(provider(SecretProviderKind::Azure, false));
        let err = validate_vault_certificate_providers(&vault_entry(), &bindings(), &lookup)
            .expect_err("azure cannot supply client certificates");
        let text = err.to_string();
        assert!(text.contains("Azure Key Vault"), "got: {text}");
        assert!(text.contains("Client certificate 1"), "got: {text}");
        assert!(
            text.contains("RocketVault"),
            "should say what to do instead: {text}"
        );
    }

    #[test]
    fn a_missing_connection_is_not_rejected_here() {
        let lookup = FixedLookup(None);
        validate_vault_certificate_providers(&vault_entry(), &bindings(), &lookup)
            .expect("a deleted connection already has its own warning");
    }

    #[test]
    fn file_and_secret_certificates_are_never_checked_against_the_provider() {
        let lookup = FixedLookup(provider(SecretProviderKind::Azure, false));
        let pem_only = vec![pem("api.example.com", "c.pem", "k.pem", None, None)];
        validate_vault_certificate_providers(&pem_only, &bindings(), &lookup)
            .expect("only vault entries need certificate support");
    }
}
