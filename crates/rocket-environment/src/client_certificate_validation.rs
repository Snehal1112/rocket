//! Save-time checks for an environment's client certificates.
//!
//! Every piece of material needs exactly one source, a reference must name a bound secret, and
//! no path or reference field may hold key text, so a private key never lands in the
//! environment file or in git.

use rocket_shared::certificate::ClientCertificate;
use rocket_shared::error::{DomainError, DomainResult};

use crate::external_secret::ExternalSecretBinding;

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
}
