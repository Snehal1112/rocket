//! The RocketVault v4 certificate HTTP contract, in one place.
//!
//! Routes, request and response shapes, error codes and the user-facing messages for them live
//! here and nowhere else, so a change on the RocketVault side (its v-4.0.0 branch is not
//! published yet) is a change to this file only. The calls themselves are in `certificates.rs`.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::StatusCode;
use rocket_environment::{
    SecretManagerConnection, VaultCertificateMaterial, VaultCertificateSummary,
};
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::vault_api_url;

/// Page size asked for when listing certificates. RocketVault allows up to 200.
pub(super) const LIST_PAGE_SIZE: usize = 200;
/// The list walk stops here, so a server that never reports a last page cannot loop forever.
pub(super) const MAX_LIST_PAGES: usize = 100;
/// Largest export response read. A PEM chain with its key, or a PKCS12 bundle, is a few KiB.
pub(super) const MAX_EXPORT_BYTES: usize = 1024 * 1024;
/// Largest error response read. Only its error code is used.
pub(super) const MAX_ERROR_BYTES: usize = 64 * 1024;
/// Length of the one-time PKCS12 password.
pub(super) const PKCS12_PASSWORD_LEN: usize = 32;

pub(super) const NOT_FOUND: &str = "Certificate not found in this vault.";
pub(super) const NOT_EXPORTABLE: &str = "Certificate is not marked exportable.";
pub(super) const MISSING_ROLE: &str = "The service account lacks the Certificate Exporter role.";
pub(super) const DISABLED: &str = "Certificate is disabled.";
pub(super) const TOKEN_REJECTED: &str = "RocketVault rejected the access token (401).";

/// `GET /api/v1/vaults/{vault}/certificates?page={page}&per_page=200`. Pages count from 0.
/// The route has no name filter.
pub(super) fn list_url(
    connection: &SecretManagerConnection,
    vault_name: &str,
    page: usize,
) -> DomainResult<url::Url> {
    let mut url = vault_api_url(
        connection,
        &["api", "v1", "vaults", vault_name, "certificates"],
    )?;
    url.query_pairs_mut()
        .append_pair("page", &page.to_string())
        .append_pair("per_page", &LIST_PAGE_SIZE.to_string());
    Ok(url)
}

/// `POST /api/v1/vaults/{vault}/certificates/{id}/export`. The route takes the id, not the name.
pub(super) fn export_url(
    connection: &SecretManagerConnection,
    vault_name: &str,
    certificate_id: &str,
) -> DomainResult<url::Url> {
    vault_api_url(
        connection,
        &[
            "api",
            "v1",
            "vaults",
            vault_name,
            "certificates",
            certificate_id,
            "export",
        ],
    )
}

fn default_true() -> bool {
    true
}

/// One list entry. Assumed shape, mirroring the secret list; confirm against RocketVault
/// v-4.0.0. Unknown fields (`not_before`, `version`, ...) are ignored.
#[derive(Deserialize)]
struct RawCertificateSummary {
    id: String,
    name: String,
    #[serde(default)]
    exportable: bool,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    key_algorithm: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
}

/// One list page: `{"certificates": [...], "total": N}`. Assumed like the secret list.
#[derive(Deserialize)]
struct RawCertificatePage {
    #[serde(default)]
    certificates: Vec<RawCertificateSummary>,
    #[serde(default)]
    total: Option<usize>,
}

/// One decoded list page.
pub(super) struct CertificatePage {
    pub certificates: Vec<VaultCertificateSummary>,
    pub total: Option<usize>,
}

/// Decodes a list page. A list never carries key material, so the decoder's message is kept.
pub(super) fn parse_certificate_page(body: &[u8]) -> DomainResult<CertificatePage> {
    let raw: RawCertificatePage = serde_json::from_slice(body).map_err(|e| {
        DomainError::Http(format!(
            "failed to decode RocketVault certificate list: {e}"
        ))
    })?;
    Ok(CertificatePage {
        certificates: raw
            .certificates
            .into_iter()
            .map(|c| VaultCertificateSummary {
                id: c.id,
                name: c.name,
                exportable: c.exportable,
                enabled: c.enabled,
                key_algorithm: c.key_algorithm.unwrap_or_default(),
                expires_at: c.expires_at,
            })
            .collect(),
        total: raw.total,
    })
}

/// True when the page just read is the last one: it was short, or `seen` reached `total`.
pub(super) fn is_last_page(page_len: usize, seen: usize, total: Option<usize>) -> bool {
    page_len < LIST_PAGE_SIZE || total.is_some_and(|t| seen >= t)
}

/// A failed list call as a user-facing error. A 401 is handled by the caller.
pub(super) fn list_failed(status: StatusCode) -> DomainError {
    match status.as_u16() {
        403 => DomainError::Http(
            "The service account cannot list certificates in this vault (403).".to_string(),
        ),
        404 => DomainError::NotFound("RocketVault has no vault with this name.".to_string()),
        other => DomainError::Http(format!(
            "RocketVault returned status {other} while listing certificates."
        )),
    }
}

/// The JSON body of an export request.
#[derive(Serialize)]
struct ExportRequest<'a> {
    format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compat: Option<&'static str>,
}

/// The body of an export. PEM sends only the format. PKCS12 sends the one-time password and
/// `compat: legacy`, which every platform TLS stack can open. The bytes are wiped on drop.
pub(super) fn export_request_body(
    format: VaultCertificateFormat,
    password: Option<&str>,
) -> DomainResult<Zeroizing<Vec<u8>>> {
    let request = match format {
        VaultCertificateFormat::Pem => ExportRequest {
            format: "pem",
            password: None,
            compat: None,
        },
        VaultCertificateFormat::Pkcs12 => ExportRequest {
            format: "pkcs12",
            password: Some(password.ok_or_else(|| {
                DomainError::Internal("a PKCS12 export needs a password".to_string())
            })?),
            compat: Some("legacy"),
        },
    };
    serde_json::to_vec(&request)
        .map(Zeroizing::new)
        .map_err(|_| DomainError::Internal("could not build the export request".to_string()))
}

/// A successful export response. Every secret string moves into `Zeroizing` right after
/// parsing. Unknown fields (`id`, `name`, `version`, `not_before`, `expires_at`) are ignored.
#[derive(Deserialize)]
struct RawExport {
    #[serde(default)]
    key_algorithm: Option<String>,
    #[serde(default)]
    certificate_pem: Option<String>,
    #[serde(default)]
    private_key_pem: Option<String>,
    #[serde(default)]
    pkcs12_base64: Option<String>,
}

/// Turns a successful export response into material. A decoder message can quote the input,
/// so every parse error is replaced by a fixed message.
pub(super) fn parse_export(
    body: &[u8],
    format: VaultCertificateFormat,
    password: Option<Zeroizing<String>>,
) -> DomainResult<VaultCertificateMaterial> {
    let RawExport {
        key_algorithm,
        certificate_pem,
        private_key_pem,
        pkcs12_base64,
    } = serde_json::from_slice::<RawExport>(body).map_err(|_| {
        DomainError::Http("RocketVault returned a certificate export Rocket cannot read.".into())
    })?;
    let key_algorithm = key_algorithm.unwrap_or_default();
    let certificate_pem = certificate_pem.map(Zeroizing::new);
    let private_key_pem = private_key_pem.map(Zeroizing::new);
    let pkcs12_base64 = pkcs12_base64.map(Zeroizing::new);

    match format {
        VaultCertificateFormat::Pem => {
            let certificate = certificate_pem.filter(|c| !c.trim().is_empty());
            let private_key = private_key_pem.filter(|k| !k.trim().is_empty());
            let (Some(certificate), Some(private_key)) = (certificate, private_key) else {
                return Err(DomainError::Http(
                    "RocketVault returned a PEM export without the certificate or the private key."
                        .into(),
                ));
            };
            Ok(VaultCertificateMaterial::Pem {
                certificate: Zeroizing::new(certificate.as_bytes().to_vec()),
                private_key: Zeroizing::new(private_key.as_bytes().to_vec()),
                key_algorithm,
            })
        }
        VaultCertificateFormat::Pkcs12 => {
            let Some(encoded) = pkcs12_base64 else {
                return Err(DomainError::Http(
                    "RocketVault returned a PKCS12 export without the bundle.".into(),
                ));
            };
            let Some(password) = password else {
                return Err(DomainError::Internal(
                    "a PKCS12 export needs its password".into(),
                ));
            };
            // Whitespace, including line breaks, is ignored, like for vault-secret bundles.
            let compact: Zeroizing<String> =
                Zeroizing::new(encoded.chars().filter(|c| !c.is_whitespace()).collect());
            let bundle = STANDARD
                .decode(compact.as_bytes())
                .map(Zeroizing::new)
                .map_err(|_| {
                    DomainError::Http(
                        "RocketVault returned a PKCS12 bundle that is not valid base64.".into(),
                    )
                })?;
            Ok(VaultCertificateMaterial::Pkcs12 {
                bundle,
                password,
                key_algorithm,
            })
        }
    }
}

/// `{"error": {"code": ..., "message": ...}}`. Only the code is read.
#[derive(Deserialize)]
struct RawErrorEnvelope {
    error: RawErrorBody,
}

#[derive(Deserialize)]
struct RawErrorBody {
    code: String,
}

/// What a failed export means for the caller.
pub(super) enum ExportFailure {
    /// 401: the cached token is stale. The caller evicts it and fails the request.
    TokenRejected,
    /// 404: the cached id may be stale. The caller looks the name up again and retries once.
    NotFound,
    /// Anything else, as the user-facing error.
    Failed(DomainError),
}

/// Maps a failed export to its meaning. A 401 and a missing-role 403 come from middleware with
/// no JSON body, so they are read by status alone.
pub(super) fn classify_export_error(status: StatusCode, body: &[u8]) -> ExportFailure {
    let code = serde_json::from_slice::<RawErrorEnvelope>(body)
        .ok()
        .map(|e| e.error.code);
    match (status.as_u16(), code.as_deref()) {
        (401, _) => ExportFailure::TokenRejected,
        (404, _) => ExportFailure::NotFound,
        (403, Some("certificate_not_exportable")) => {
            ExportFailure::Failed(DomainError::InvalidInput(NOT_EXPORTABLE.to_string()))
        }
        (403, None) => ExportFailure::Failed(DomainError::Http(MISSING_ROLE.to_string())),
        (403, Some(other)) => ExportFailure::Failed(DomainError::Http(match shown_code(other) {
            Some(code) => format!("RocketVault refused the export (403, {code})."),
            None => "RocketVault refused the export (403).".to_string(),
        })),
        (409, _) => ExportFailure::Failed(DomainError::InvalidInput(DISABLED.to_string())),
        (other, _) => ExportFailure::Failed(DomainError::Http(format!(
            "RocketVault returned status {other} for the certificate export."
        ))),
    }
}

/// A server-supplied error code, if it is safe to show: 1 to 64 ASCII letters, digits, `_` or
/// `-`. Anything else is dropped, so a hostile server cannot inject text into a message.
fn shown_code(code: &str) -> Option<&str> {
    let safe = (1..=64).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    safe.then_some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".into(),
            label: "Test".into(),
            base_url: "https://vault.example.com/".into(),
            client_id: "rocketapi".into(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    const CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\nZmFrZS1jZXJ0\n-----END CERTIFICATE-----\n";
    const KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nZmFrZS1rZXk=\n-----END PRIVATE KEY-----\n";

    #[test]
    fn list_url_asks_for_one_page_of_200() {
        let url = list_url(&connection(), "prod-vault", 2).expect("url");
        assert_eq!(
            url.as_str(),
            "https://vault.example.com/api/v1/vaults/prod-vault/certificates?page=2&per_page=200"
        );
    }

    #[test]
    fn export_url_uses_the_id_and_percent_encodes_segments() {
        let url = export_url(&connection(), "a/b", "id?1").expect("url");
        assert_eq!(
            url.as_str(),
            "https://vault.example.com/api/v1/vaults/a%2Fb/certificates/id%3F1/export"
        );
        assert!(export_url(&connection(), "prod-vault", "..").is_err());
    }

    #[test]
    fn a_pem_export_body_sends_only_the_format() {
        let body = export_request_body(VaultCertificateFormat::Pem, None).expect("body");
        assert_eq!(body.as_slice(), br#"{"format":"pem"}"#);
    }

    #[test]
    fn a_pkcs12_export_body_sends_the_password_and_legacy_compat() {
        let body =
            export_request_body(VaultCertificateFormat::Pkcs12, Some("pw-123")).expect("body");
        assert_eq!(
            body.as_slice(),
            br#"{"format":"pkcs12","password":"pw-123","compat":"legacy"}"#
        );
        assert!(export_request_body(VaultCertificateFormat::Pkcs12, None).is_err());
    }

    #[test]
    fn parse_certificate_page_maps_fields_and_defaults() {
        let body = br#"{"certificates":[
            {"id":"id-1","name":"client-a","exportable":true,"enabled":false,
             "key_algorithm":"EC-P256","expires_at":"2027-01-01T00:00:00Z","not_before":"x"},
            {"id":"id-2","name":"bare"}],"total":2}"#;
        let page = parse_certificate_page(body).expect("page");
        assert_eq!(page.total, Some(2));
        assert_eq!(
            page.certificates,
            vec![
                VaultCertificateSummary {
                    id: "id-1".into(),
                    name: "client-a".into(),
                    exportable: true,
                    enabled: false,
                    key_algorithm: "EC-P256".into(),
                    expires_at: Some("2027-01-01T00:00:00Z".into()),
                },
                VaultCertificateSummary {
                    id: "id-2".into(),
                    name: "bare".into(),
                    exportable: false,
                    enabled: true,
                    key_algorithm: String::new(),
                    expires_at: None,
                },
            ]
        );
    }

    #[test]
    fn is_last_page_stops_on_a_short_page_or_the_total() {
        assert!(is_last_page(3, 3, None));
        assert!(is_last_page(0, 400, None));
        assert!(!is_last_page(LIST_PAGE_SIZE, LIST_PAGE_SIZE, None));
        assert!(is_last_page(
            LIST_PAGE_SIZE,
            LIST_PAGE_SIZE,
            Some(LIST_PAGE_SIZE)
        ));
        assert!(!is_last_page(
            LIST_PAGE_SIZE,
            LIST_PAGE_SIZE,
            Some(LIST_PAGE_SIZE + 1)
        ));
    }

    #[test]
    fn list_failed_names_the_problem() {
        assert!(list_failed(StatusCode::FORBIDDEN)
            .to_string()
            .contains("cannot list certificates"));
        assert!(matches!(
            list_failed(StatusCode::NOT_FOUND),
            DomainError::NotFound(_)
        ));
        assert!(list_failed(StatusCode::BAD_GATEWAY)
            .to_string()
            .contains("502"));
    }

    #[test]
    fn parse_export_pem_returns_both_pieces() {
        let body = serde_json::to_vec(&serde_json::json!({
            "id": "id-1", "name": "client-a", "version": 3, "key_algorithm": "RSA-2048",
            "certificate_pem": CERT_PEM, "private_key_pem": KEY_PEM
        }))
        .expect("json");
        match parse_export(&body, VaultCertificateFormat::Pem, None).expect("material") {
            VaultCertificateMaterial::Pem {
                certificate,
                private_key,
                key_algorithm,
            } => {
                assert_eq!(certificate.as_slice(), CERT_PEM.as_bytes());
                assert_eq!(private_key.as_slice(), KEY_PEM.as_bytes());
                assert_eq!(key_algorithm, "RSA-2048");
            }
            other => panic!("expected PEM, got {other:?}"),
        }
    }

    #[test]
    fn parse_export_pkcs12_decodes_wrapped_base64_and_keeps_the_password() {
        let body = serde_json::to_vec(&serde_json::json!({
            "key_algorithm": "EC-P256", "pkcs12_base64": "AQID\nBAU="
        }))
        .expect("json");
        let password = Zeroizing::new("one-time-pass-123".to_string());
        match parse_export(&body, VaultCertificateFormat::Pkcs12, Some(password)).expect("material")
        {
            VaultCertificateMaterial::Pkcs12 {
                bundle, password, ..
            } => {
                assert_eq!(bundle.as_slice(), &[1, 2, 3, 4, 5]);
                assert_eq!(password.as_str(), "one-time-pass-123");
            }
            other => panic!("expected PKCS12, got {other:?}"),
        }
    }

    #[test]
    fn parse_export_rejects_a_pem_export_without_the_key() {
        let body =
            serde_json::to_vec(&serde_json::json!({ "certificate_pem": CERT_PEM })).expect("json");
        let err = parse_export(&body, VaultCertificateFormat::Pem, None)
            .expect_err("a PEM export needs both pieces");
        assert!(err.to_string().contains("private key"), "{err}");
    }

    #[test]
    fn parse_export_errors_never_quote_the_body() {
        let body = b"{\"private_key_pem\": \"-----BEGIN PRIVATE KEY-----\\nc2VjcmV0\" ,,, }";
        let err = parse_export(body, VaultCertificateFormat::Pem, None)
            .expect_err("malformed JSON")
            .to_string();
        assert!(!err.contains("c2VjcmV0") && !err.contains("BEGIN"), "{err}");
    }

    #[test]
    fn classify_maps_each_status_and_code() {
        let json = |code: &str| {
            format!(r#"{{"error":{{"code":"{code}","message":"from RocketVault"}}}}"#).into_bytes()
        };
        let failed = |status: u16, body: &[u8]| {
            let status = StatusCode::from_u16(status).expect("status");
            match classify_export_error(status, body) {
                ExportFailure::Failed(err) => err.to_string(),
                _ => panic!("expected Failed for {status}"),
            }
        };
        assert!(matches!(
            classify_export_error(StatusCode::UNAUTHORIZED, b"Unauthorized"),
            ExportFailure::TokenRejected
        ));
        assert!(matches!(
            classify_export_error(StatusCode::NOT_FOUND, &json("not_found")),
            ExportFailure::NotFound
        ));
        assert!(failed(403, &json("certificate_not_exportable")).contains(NOT_EXPORTABLE));
        assert!(failed(403, b"Forbidden").contains(MISSING_ROLE));
        assert!(failed(403, &json("vault_forbidden")).contains("vault_forbidden"));
        assert!(failed(409, &json("certificate_disabled")).contains(DISABLED));
        assert!(failed(400, &json("bad_request")).contains("400"));
        assert!(failed(500, &json("internal_error")).contains("500"));
    }

    // A server-supplied code is echoed only when it is short and plain.
    #[test]
    fn classify_never_echoes_an_unsafe_403_code() {
        let forbidden = |code: &str| {
            let body = serde_json::to_vec(&serde_json::json!({ "error": { "code": code } }))
                .expect("json");
            match classify_export_error(StatusCode::FORBIDDEN, &body) {
                ExportFailure::Failed(err) => err.to_string(),
                _ => panic!("expected Failed"),
            }
        };
        let long = "a".repeat(65);
        for unsafe_code in [
            "<b>x</b>",
            "line\nbreak",
            "sp ace",
            "dot.ted",
            "",
            long.as_str(),
        ] {
            let message = forbidden(unsafe_code);
            assert!(message.contains("403"), "{message}");
            // A JSON body is still not the missing-role case.
            assert!(!message.contains(MISSING_ROLE), "{message}");
            if !unsafe_code.is_empty() {
                assert!(!message.contains(unsafe_code), "{message}");
            }
            assert!(
                !message.contains('<') && !message.contains('\n'),
                "{message}"
            );
        }
        assert!(forbidden("vault-forbidden_2").contains("vault-forbidden_2"));
    }

    #[test]
    fn a_null_key_algorithm_reads_as_empty() {
        let page = parse_certificate_page(
            br#"{"certificates":[{"id":"id-1","name":"a","key_algorithm":null}]}"#,
        )
        .expect("page");
        assert_eq!(page.certificates[0].key_algorithm, "");

        let body = br#"{"key_algorithm":null,"pkcs12_base64":"AQID"}"#;
        let material = parse_export(
            body,
            VaultCertificateFormat::Pkcs12,
            Some(Zeroizing::new("pw".to_string())),
        )
        .expect("material");
        assert_eq!(material.key_algorithm(), "");
    }
}
