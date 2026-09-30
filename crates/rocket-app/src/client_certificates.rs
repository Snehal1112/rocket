//! Client certificates of the selected environment, prepared for the HTTP layer.
//!
//! Shared by request execution and the OAuth2 token requests, so both resolve placeholders and
//! relative paths in exactly the same way.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use rocket_environment::{resolve, EnvironmentRepository};
use rocket_http::{CertificateSource, ResolvedClientCertificate};
use rocket_shared::certificate::ClientCertificate;

/// Returns the named environment's client certificates, ready for the executor: `{{placeholders}}`
/// resolved with `vars`, and relative file paths joined onto `collection_dir`.
///
/// No environment name, or an environment that cannot be read, means no certificates, like it
/// means no variables.
pub(crate) fn environment_client_certificates(
    repo: &dyn EnvironmentRepository,
    environment_name: Option<&str>,
    collection_dir: Option<&Path>,
    vars: &HashMap<String, String>,
) -> Vec<ResolvedClientCertificate> {
    let Some(name) = environment_name else {
        return Vec::new();
    };
    let Ok(env) = repo.get(name) else {
        return Vec::new();
    };
    env.client_certificates
        .into_iter()
        .map(|c| resolve_client_certificate(c, collection_dir, vars))
        .collect()
}

/// Resolves `{{placeholders}}` in a certificate's domain, file paths and passphrase, then joins
/// relative file paths onto the collection folder.
fn resolve_client_certificate(
    cert: ClientCertificate,
    base: Option<&Path>,
    vars: &HashMap<String, String>,
) -> ResolvedClientCertificate {
    let r = |s: String| resolve(&s, vars).output;
    let file = |p: String| CertificateSource::File(absolutize(r(p), base));
    match cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            certificate_secret,
            private_key_secret,
            passphrase,
        } => {
            if let Some(reference) = certificate_secret.or(private_key_secret) {
                return ResolvedClientCertificate::unavailable(
                    r(domain),
                    not_resolved_yet(&reference),
                );
            }
            ResolvedClientCertificate::pem(
                r(domain),
                file(certificate_file_path),
                file(private_key_file_path),
                passphrase.map(&r),
            )
        }
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            pkcs12_secret,
            passphrase,
        } => {
            if let Some(reference) = pkcs12_secret {
                return ResolvedClientCertificate::unavailable(
                    r(domain),
                    not_resolved_yet(&reference),
                );
            }
            ResolvedClientCertificate::pkcs12(r(domain), file(pkcs12_file_path), passphrase.map(&r))
        }
    }
}

/// The error for a vault reference before references are resolved. It names the reference only.
fn not_resolved_yet(reference: &str) -> String {
    format!(
        "Client certificate secret {reference} cannot be used yet: certificate material from \
         RocketVault is not supported in this build."
    )
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

/// One line per certificate, for test assertions: `pkcs12 <domain> file:<path> pass:<value>`.
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
        })
        .collect()
}
