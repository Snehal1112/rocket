//! Client certificates of the selected environment, prepared for the HTTP layer.
//!
//! Shared by request execution and the OAuth2 token requests, so both resolve placeholders,
//! relative paths and RocketVault references in exactly the same way.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rocket_environment::{resolve, EnvironmentRepository};
use rocket_http::{CertificateSource, ResolvedClientCertificate};
use rocket_shared::certificate::ClientCertificate;
use zeroize::Zeroizing;

/// The largest vault secret accepted as certificate material. A PKCS12 bundle as base64 is tens
/// of KiB, so this only stops a wrong secret (a large file, a dump) from being copied around.
pub(crate) const MAX_INLINE_SECRET_BYTES: usize = 1024 * 1024;

/// Returns the named environment's client certificates, ready for the executor: `{{placeholders}}`
/// resolved with `vars`, relative file paths joined onto `collection_dir`, and each RocketVault
/// reference replaced by the bytes found under `alias.secretName` in `external_secrets`.
///
/// A reference that cannot be resolved does not fail here. The entry becomes
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
    env.client_certificates
        .into_iter()
        .map(|c| resolve_client_certificate(c, vars))
        .map(|c| absolutize_certificate_paths(c, collection_dir))
        .map(|c| resolve_references(c, external_secrets))
        .collect()
}

/// Resolves `{{placeholders}}` in a certificate's domain, file paths and passphrase.
/// A reference (`certificateSecret` and the like) is a key, never a template, so it stays as is.
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
    }
    cert
}

/// Joins a relative certificate file path onto the collection folder `base`.
///
/// Absolute paths and `~/` paths stay as written. So does a relative path with a `..` in it, so
/// an environment file cannot point outside the collection folder. The executor rejects any
/// path that is still relative, with a message that says what is allowed.
fn absolutize(p: String, base: Option<&Path>) -> String {
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

/// Turns one persisted entry into the runtime form, looking each reference up in `secrets`.
fn resolve_references(
    cert: ClientCertificate,
    secrets: &HashMap<String, String>,
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
