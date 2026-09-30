//! Client certificates of the selected environment, prepared for the HTTP layer.
//!
//! Shared by request execution and the OAuth2 token requests, so both resolve placeholders and
//! relative paths in exactly the same way.

use rocket_environment::{resolve, EnvironmentRepository};
use rocket_shared::certificate::ClientCertificate;

/// Returns the named environment's client certificates, ready for the executor: `{{placeholders}}`
/// resolved with `vars`, and relative file paths joined onto `collection_dir`.
///
/// No environment name, or an environment that cannot be read, means no certificates, like it
/// means no variables.
pub(crate) fn environment_client_certificates(
    repo: &dyn EnvironmentRepository,
    environment_name: Option<&str>,
    collection_dir: Option<&std::path::Path>,
    vars: &std::collections::HashMap<String, String>,
) -> Vec<ClientCertificate> {
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
        .collect()
}

/// Joins a relative certificate file path onto the collection folder `base`.
///
/// Absolute paths and `~/` paths stay as written. So does a relative path with a `..` in it, so
/// an environment file cannot point outside the collection folder. The executor rejects any
/// path that is still relative, with a message that says what is allowed.
fn absolutize_certificate_paths(
    cert: ClientCertificate,
    base: Option<&std::path::Path>,
) -> ClientCertificate {
    let Some(base) = base else { return cert };
    let join = |p: String| -> String {
        let path = std::path::Path::new(&p);
        let stays = p.is_empty()
            || p.starts_with("~/")
            || path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
        if stays {
            p
        } else {
            // Drop `.` components so `./certs/a.pem` joins as `certs/a.pem`.
            let tidy: std::path::PathBuf = path
                .components()
                .filter(|c| !matches!(c, std::path::Component::CurDir))
                .collect();
            base.join(tidy).to_string_lossy().into_owned()
        }
    };
    match cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            passphrase,
        } => ClientCertificate::Pem {
            domain,
            certificate_file_path: join(certificate_file_path),
            private_key_file_path: join(private_key_file_path),
            passphrase,
        },
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            passphrase,
        } => ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path: join(pkcs12_file_path),
            passphrase,
        },
    }
}

/// Resolves `{{placeholders}}` in a client certificate's domain, file paths and passphrase.
fn resolve_client_certificate(
    cert: ClientCertificate,
    vars: &std::collections::HashMap<String, String>,
) -> ClientCertificate {
    let r = |s: String| resolve(&s, vars).output;
    match cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            passphrase,
        } => ClientCertificate::Pem {
            domain: r(domain),
            certificate_file_path: r(certificate_file_path),
            private_key_file_path: r(private_key_file_path),
            passphrase: passphrase.map(&r),
        },
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            passphrase,
        } => ClientCertificate::Pkcs12 {
            domain: r(domain),
            pkcs12_file_path: r(pkcs12_file_path),
            passphrase: passphrase.map(&r),
        },
    }
}
