use crate::resolved_certificate::ResolvedClientCertificate;
use rocket_shared::types::{Auth, Body, Header, HttpMethod, QueryParam};
use serde::{Deserialize, Serialize};

/// An HTTP request ready for execution (resolved variables, all fields populated).
/// This is different from collection::Request which is a saved definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<Header>,
    pub query_params: Vec<QueryParam>,
    pub body: Option<Body>,
    pub auth: Auth,
    pub options: RequestOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
    /// Override the maximum number of redirects to follow. `None` uses the executor default (10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<u32>,
    /// Client certificates of the active environment, resolved for this request. The executor
    /// picks the one whose domain matches the request URL and presents it for mutual TLS. It is
    /// never serialized: the environment is the only source, and the material can hold key bytes.
    #[serde(skip)]
    pub client_certificates: Vec<ResolvedClientCertificate>,
}

fn default_true() -> bool {
    true
}
fn default_timeout() -> u64 {
    30_000
}

impl Default for RequestOptions {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            timeout_ms: 30_000,
            verify_ssl: true,
            max_redirects: None,
            client_certificates: Vec::new(),
        }
    }
}

impl HttpRequest {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: None,
            auth: Auth::None,
            options: RequestOptions::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options() {
        let req = HttpRequest::new(HttpMethod::Get, "https://example.com");
        assert!(req.options.follow_redirects);
        assert_eq!(req.options.timeout_ms, 30_000);
        assert!(req.options.verify_ssl);
    }

    mod certificate_leaks {
        use super::*;
        use crate::{CertificateSource, ResolvedClientCertificate};
        use zeroize::Zeroizing;

        const KEY_TEXT: &str =
            "-----BEGIN PRIVATE KEY-----\nMIIEleakcheckbody\n-----END PRIVATE KEY-----\n";

        /// Things that appear in the output if bytes or passphrases are printed. The decimal lists
        /// are what a derived `Debug` of `Vec<u8>` prints for `cert-leak-check-body` and `SUPER`.
        /// `SUPER` is the plain-text form of the PKCS12 bundle bytes, as a lossy UTF-8 print shows.
        const NEEDLES: [&str; 8] = [
            "MIIEleakcheckbody",
            "cert-leak-check-body",
            "BEGIN PRIVATE KEY",
            "hunter2-passphrase",
            "p12-passphrase",
            "99, 101, 114",
            "83, 85, 80",
            "SUPER",
        ];

        fn request_with_certificates() -> HttpRequest {
            let mut req = HttpRequest::new(HttpMethod::Get, "https://api.example.com");
            req.options.client_certificates = vec![
                ResolvedClientCertificate::pem(
                    "api.example.com",
                    CertificateSource::Inline(Zeroizing::new(b"cert-leak-check-body".to_vec())),
                    CertificateSource::Inline(Zeroizing::new(KEY_TEXT.as_bytes().to_vec())),
                    Some("hunter2-passphrase".into()),
                ),
                ResolvedClientCertificate::pkcs12(
                    "b.example.com",
                    CertificateSource::Inline(Zeroizing::new(vec![0x53, 0x55, 0x50, 0x45, 0x52])),
                    Some("p12-passphrase".into()),
                ),
            ];
            req
        }

        fn assert_clean(shown: &str) {
            for needle in NEEDLES {
                assert!(!shown.contains(needle), "{needle} leaked into {shown}");
            }
        }

        #[test]
        fn debug_of_a_resolved_certificate_never_prints_bytes_or_passphrases() {
            for cert in request_with_certificates().options.client_certificates {
                assert_clean(&format!("{cert:?}"));
                assert_clean(&format!("{cert:#?}"));
            }
            let shown = format!("{:?}", request_with_certificates().options.client_certificates);
            assert!(shown.contains("inline 20 bytes"), "{shown}");
            assert!(shown.contains("<redacted>"), "{shown}");
        }

        #[test]
        fn debug_of_request_options_and_http_request_never_prints_key_material() {
            let req = request_with_certificates();
            for shown in [
                format!("{:?}", req.options),
                format!("{:#?}", req.options),
                format!("{req:?}"),
                format!("{req:#?}"),
            ] {
                assert_clean(&shown);
            }
        }

        #[test]
        fn serializing_options_or_a_request_never_contains_certificates() {
            let req = request_with_certificates();
            for json in [
                serde_json::to_string(&req.options).expect("options"),
                serde_json::to_string(&req).expect("request"),
            ] {
                assert_clean(&json);
                assert!(!json.contains("clientCertificates"), "{json}");
            }
        }

        #[test]
        fn certificates_in_ipc_input_are_ignored() {
            let options: RequestOptions = serde_json::from_str(
                r#"{"clientCertificates":[{"type":"pkcs12","domain":"x","pkcs12FilePath":"/etc/shadow"}]}"#,
            )
            .expect("unknown keys are ignored");
            assert!(options.client_certificates.is_empty());
        }
    }

    #[test]
    fn client_certificates_never_cross_serde() {
        use crate::resolved_certificate::{CertificateSource, ResolvedClientCertificate};
        let options = RequestOptions {
            client_certificates: vec![ResolvedClientCertificate::pkcs12(
                "api.example.com",
                CertificateSource::File("/certs/client.p12".into()),
                Some("s3cret".into()),
            )],
            ..RequestOptions::default()
        };
        let json = serde_json::to_string(&options).expect("serialize options");
        assert!(!json.contains("clientCertificates"), "{json}");
        assert!(!json.contains("s3cret"), "{json}");

        // The IPC input cannot carry certificates: the environment is the only source.
        let back: RequestOptions = serde_json::from_str(
            r#"{"clientCertificates":[{"type":"pkcs12","domain":"evil.example.com","pkcs12FilePath":"/etc/shadow"}]}"#,
        )
        .expect("deserialize options");
        assert!(back.client_certificates.is_empty());
    }
}
