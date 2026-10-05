//! Client certificates of the selected environment, prepared for the HTTP layer.
//!
//! Shared by request execution and the OAuth2 token requests, so both resolve placeholders,
//! relative paths and RocketVault references in exactly the same way.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rocket_environment::{resolve, EnvironmentRepository, ExternalSecretBinding};
use rocket_http::{CertificateSource, ResolvedClientCertificate, VaultCertificateBinding};
use rocket_shared::certificate::ClientCertificate;
use zeroize::Zeroizing;

/// The largest vault secret accepted as certificate material. A PKCS12 bundle as base64 is tens
/// of KiB, so this only stops a wrong secret (a large file, a dump) from being copied around.
pub(crate) const MAX_INLINE_SECRET_BYTES: usize = 1024 * 1024;

/// Returns the named environment's client certificates, ready for the executor: `{{placeholders}}`
/// resolved with `vars`, relative file paths joined onto `collection_dir`, and each RocketVault
/// reference replaced by the bytes found under `alias.secretName` in `external_secrets`.
/// A `vault` entry becomes `CertificateMaterial::Deferred` with the connection and vault of its
/// External Secrets binding. It holds names only, and is exported later, only when selected.
///
/// A reference or binding that cannot be resolved does not fail here. The entry becomes
/// `CertificateMaterial::Unavailable`, and the executor fails the request only when that entry is
/// the one selected for the URL.
///
/// No environment name, or an environment that cannot be read, means no certificates, like it
/// means no variables.
pub(crate) fn environment_client_certificates(
    repo: &dyn EnvironmentRepository,
    environment_name: Option<&str>,
    collection_dir: Option<&Path>,
    vars: &HashMap<String, String>,
    external_secrets: &HashMap<String, String>,
) -> Vec<ResolvedClientCertificate> {
    let Some(name) = environment_name else {
        return Vec::new();
    };
    let Ok(env) = repo.get(name) else {
        return Vec::new();
    };
    let bindings = env.external_secrets;
    env.client_certificates
        .into_iter()
        .map(|c| resolve_client_certificate(c, vars))
        .map(|c| absolutize_certificate_paths(c, collection_dir))
        .map(|c| resolve_references(c, external_secrets, &bindings))
        .collect()
}

/// Resolves `{{placeholders}}` in a certificate's domain, file paths and passphrase.
/// A reference (`certificateSecret` and the like) is a key, never a template, so it stays as is.
/// So do a `vault` entry's binding and certificate name; only its domain is resolved.
fn resolve_client_certificate(
    mut cert: ClientCertificate,
    vars: &HashMap<String, String>,
) -> ClientCertificate {
    let r = |s: &mut String| {
        *s = resolve(s, vars).output;
    };
    match &mut cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            passphrase,
            ..
        } => {
            r(domain);
            r(certificate_file_path);
            r(private_key_file_path);
            if let Some(p) = passphrase {
                r(p);
            }
        }
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            passphrase,
            ..
        } => {
            r(domain);
            r(pkcs12_file_path);
            if let Some(p) = passphrase {
                r(p);
            }
        }
        ClientCertificate::Vault { domain, .. } => r(domain),
    }
    cert
}

/// Joins relative file paths of `cert` onto the collection folder `base`.
fn absolutize_certificate_paths(
    mut cert: ClientCertificate,
    base: Option<&Path>,
) -> ClientCertificate {
    let Some(base) = base else { return cert };
    match &mut cert {
        ClientCertificate::Pem {
            certificate_file_path,
            private_key_file_path,
            ..
        } => {
            *certificate_file_path = absolutize(std::mem::take(certificate_file_path), Some(base));
            *private_key_file_path = absolutize(std::mem::take(private_key_file_path), Some(base));
        }
        ClientCertificate::Pkcs12 {
            pkcs12_file_path, ..
        } => *pkcs12_file_path = absolutize(std::mem::take(pkcs12_file_path), Some(base)),
        // A vault entry has no file path.
        ClientCertificate::Vault { .. } => {}
    }
    cert
}

/// Joins a relative file path onto the collection folder `base`. Certificates and upload files
/// share this rule.
///
/// Absolute paths and `~/` paths stay as written. So does a relative path with a `..` in it, so
/// an environment file cannot point outside the collection folder. The executor rejects any
/// path that is still relative, with a message that says what is allowed.
pub(crate) fn absolutize(p: String, base: Option<&Path>) -> String {
    let Some(base) = base else { return p };
    let path = Path::new(&p);
    let stays = p.is_empty()
        || p.starts_with("~/")
        || path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir));
    if stays {
        return p;
    }
    // Drop `.` components so `./certs/a.pem` joins as `certs/a.pem`.
    let tidy: PathBuf = path
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    base.join(tidy).to_string_lossy().into_owned()
}

/// How the text of a secret becomes bytes.
#[derive(Clone, Copy)]
enum Encoding {
    /// PEM text, used byte for byte.
    Text,
    /// A base64 PKCS12 bundle. Whitespace, including line breaks, is ignored.
    Base64,
}

/// Turns one persisted entry into the runtime form, looking each reference up in `secrets`
/// and each `vault` binding up in `bindings`.
fn resolve_references(
    cert: ClientCertificate,
    secrets: &HashMap<String, String>,
    bindings: &[ExternalSecretBinding],
) -> ResolvedClientCertificate {
    match cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            certificate_secret,
            private_key_secret,
            passphrase,
        } => {
            let certificate = piece_source(
                &domain,
                "certificate",
                certificate_file_path,
                certificate_secret,
                secrets,
                Encoding::Text,
            );
            let private_key = piece_source(
                &domain,
                "private key",
                private_key_file_path,
                private_key_secret,
                secrets,
                Encoding::Text,
            );
            match (certificate, private_key) {
                (Ok(certificate), Ok(private_key)) => {
                    ResolvedClientCertificate::pem(domain, certificate, private_key, passphrase)
                }
                (Err(reason), _) | (_, Err(reason)) => {
                    ResolvedClientCertificate::unavailable(domain, reason)
                }
            }
        }
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            pkcs12_secret,
            passphrase,
        } => {
            match piece_source(
                &domain,
                "PKCS12 bundle",
                pkcs12_file_path,
                pkcs12_secret,
                secrets,
                Encoding::Base64,
            ) {
                Ok(bundle) => ResolvedClientCertificate::pkcs12(domain, bundle, passphrase),
                Err(reason) => ResolvedClientCertificate::unavailable(domain, reason),
            }
        }
        ClientCertificate::Vault {
            domain,
            binding,
            certificate,
            format: export_format,
        } => match bindings.iter().find(|b| b.alias == binding) {
            Some(found) => ResolvedClientCertificate::deferred(
                domain,
                VaultCertificateBinding {
                    alias: found.alias.clone(),
                    connection_id: found.connection_id.clone(),
                    vault_name: found.vault_name.clone(),
                },
                certificate,
                export_format,
            ),
            None => {
                let reason = format!(
                    "Client certificate for {domain} uses the External Secrets binding \
                     {binding}, which this environment does not have."
                );
                ResolvedClientCertificate::unavailable(domain, reason)
            }
        },
    }
}

/// The source of one piece: its file, or the bytes of its secret. Exactly one must be set.
/// The error is the user-facing reason, and never contains a secret value.
fn piece_source(
    domain: &str,
    piece: &str,
    file_path: String,
    reference: Option<String>,
    secrets: &HashMap<String, String>,
    encoding: Encoding,
) -> Result<CertificateSource, String> {
    // An empty or whitespace-only reference counts as absent, like in the save validator.
    let reference = reference.filter(|r| !r.trim().is_empty());
    match (file_path.is_empty(), reference) {
        (false, None) => Ok(CertificateSource::File(file_path)),
        (true, Some(reference)) => {
            inline_from_secret(&reference, secrets, encoding).map(CertificateSource::Inline)
        }
        (true, None) => Err(format!(
            "Client certificate for {domain} has neither a file path nor a secret reference for its {piece}."
        )),
        (false, Some(reference)) => Err(format!(
            "Client certificate for {domain} has both a file path and the secret reference {reference} for its {piece}. Use only one."
        )),
    }
}

fn inline_from_secret(
    reference: &str,
    secrets: &HashMap<String, String>,
    encoding: Encoding,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let Some(value) = secrets.get(reference) else {
        return Err(format!(
            "Client certificate secret {reference} was not found. \
             Check the External Secrets binding and fetch the secret names."
        ));
    };
    if value.len() > MAX_INLINE_SECRET_BYTES {
        return Err(format!(
            "Client certificate secret {reference} is larger than 1 MiB. \
             Check that it holds a certificate and not another file."
        ));
    }
    if value.trim().is_empty() {
        return Err(format!("Client certificate secret {reference} is empty."));
    }
    match encoding {
        Encoding::Text => Ok(Zeroizing::new(value.as_bytes().to_vec())),
        Encoding::Base64 => {
            let compact: Zeroizing<String> =
                Zeroizing::new(value.chars().filter(|c| !c.is_whitespace()).collect());
            STANDARD
                .decode(compact.as_bytes())
                .map(Zeroizing::new)
                // The decoder's message can quote input, so it is dropped.
                .map_err(|_| format!("Client certificate secret {reference} is not valid base64."))
        }
    }
}

/// Load tests send without RocketVault access, so each vault certificate becomes
/// `Unavailable` with a clear reason. As everywhere else, it fails a request only when it is
/// the one selected for the URL.
pub(crate) fn unavailable_in_load_tests(certificates: &mut [ResolvedClientCertificate]) {
    for cert in certificates.iter_mut().filter(|c| c.is_deferred()) {
        let reason = format!(
            "RocketVault certificates are not available in load tests. Use a file or a vault \
             secret certificate for {}.",
            cert.domain
        );
        *cert = ResolvedClientCertificate::unavailable(cert.domain.clone(), reason);
    }
}

/// One line per certificate, for test assertions: `pkcs12 <domain> file:<path> pass:<value>`,
/// or `deferred <domain> <alias>:<name> <format> conn:<id> vault:<name>`.
/// It prints the passphrase, so it only exists in tests.
#[cfg(test)]
pub(crate) fn describe_all(certs: &[ResolvedClientCertificate]) -> Vec<String> {
    use rocket_http::CertificateMaterial;
    let source = |s: &CertificateSource| match s {
        CertificateSource::File(path) => format!("file:{path}"),
        CertificateSource::Inline(bytes) => format!("inline:{}", bytes.len()),
    };
    certs
        .iter()
        .map(|c| match &c.material {
            CertificateMaterial::Pem {
                certificate,
                private_key,
                passphrase,
            } => format!(
                "pem {} {} {} pass:{}",
                c.domain,
                source(certificate),
                source(private_key),
                passphrase.as_deref().map_or("-", |p| p.as_str())
            ),
            CertificateMaterial::Pkcs12 { bundle, passphrase } => format!(
                "pkcs12 {} {} pass:{}",
                c.domain,
                source(bundle),
                passphrase.as_deref().map_or("-", |p| p.as_str())
            ),
            CertificateMaterial::Unavailable { reason } => {
                format!("unavailable {} {reason}", c.domain)
            }
            CertificateMaterial::Deferred {
                binding,
                certificate,
                format: kind,
            } => format!(
                "deferred {} {}:{} {} conn:{} vault:{}",
                c.domain,
                binding.alias,
                certificate,
                kind.as_str(),
                binding.connection_id,
                binding.vault_name
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::StaticEnvRepo;
    use rocket_environment::Environment;
    use rocket_shared::certificate::VaultCertificateFormat;

    fn binding(alias: &str) -> ExternalSecretBinding {
        ExternalSecretBinding {
            alias: alias.into(),
            connection_id: "conn-1".into(),
            vault_name: "prod-vault".into(),
            secret_names: Vec::new(),
        }
    }

    fn vault_entry(domain: &str, alias: &str) -> ClientCertificate {
        ClientCertificate::Vault {
            domain: domain.into(),
            binding: alias.into(),
            certificate: "client-a".into(),
            format: VaultCertificateFormat::Pkcs12,
        }
    }

    fn resolve_env(env: Environment, vars: &[(&str, &str)]) -> Vec<String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let certs = environment_client_certificates(
            &StaticEnvRepo(env),
            Some("prod"),
            None,
            &vars,
            &HashMap::new(),
        );
        describe_all(&certs)
    }

    #[test]
    fn a_vault_entry_becomes_deferred_with_the_connection_and_vault_of_its_binding() {
        let mut env = Environment::new("prod");
        env.external_secrets = vec![binding("payments"), binding("prod")];
        env.client_certificates = vec![vault_entry("api.example.com", "prod")];
        assert_eq!(
            resolve_env(env, &[]),
            vec!["deferred api.example.com prod:client-a pkcs12 conn:conn-1 vault:prod-vault"]
        );
    }

    #[test]
    fn a_vault_entry_with_a_binding_the_environment_lacks_is_unavailable_and_names_it() {
        let mut env = Environment::new("prod");
        env.external_secrets = vec![binding("prod")];
        env.client_certificates = vec![vault_entry("api.example.com", "payments")];
        let lines = resolve_env(env, &[]);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("unavailable api.example.com") && lines[0].contains("payments"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_placeholder_in_a_vault_entry_domain_is_resolved() {
        let mut env = Environment::new("prod");
        env.external_secrets = vec![binding("prod")];
        env.client_certificates = vec![vault_entry("{{apiHost}}", "prod")];
        let lines = resolve_env(env, &[("apiHost", "api.example.com")]);
        assert!(
            lines[0].starts_with("deferred api.example.com "),
            "{lines:?}"
        );
    }

    // Spec section 6.
    #[test]
    fn load_tests_turn_a_vault_certificate_into_a_clear_error() {
        let mut certs = vec![
            ResolvedClientCertificate::deferred(
                "api.example.com",
                VaultCertificateBinding {
                    alias: "prod".into(),
                    connection_id: "conn-1".into(),
                    vault_name: "prod-vault".into(),
                },
                "client-a",
                VaultCertificateFormat::Pem,
            ),
            ResolvedClientCertificate::pkcs12(
                "files.example.com",
                CertificateSource::File("/certs/client.p12".into()),
                None,
            ),
        ];
        unavailable_in_load_tests(&mut certs);
        let lines = describe_all(&certs);
        assert!(
            lines[0].starts_with("unavailable api.example.com")
                && lines[0].contains("not available in load tests"),
            "{lines:?}"
        );
        assert_eq!(
            lines[1],
            "pkcs12 files.example.com file:/certs/client.p12 pass:-"
        );
    }
}
