//! In-process mutual TLS tests for `ReqwestExecutor`.
//!
//! A rustls server on `127.0.0.1:0` requires a client certificate and records the one it got.
//! These run in the default test run with no external tools, so a client that stops presenting
//! the environment's certificate fails here (GitHub issue #21).

use super::*;
use rocket_shared::types::HttpMethod;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::rustls;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test-fixtures/mtls");

/// The end-entity certificate the server received, as DER, once a handshake completes.
type Recorded = Arc<Mutex<Option<Vec<u8>>>>;

fn fixture(name: &str) -> String {
    format!("{FIXTURES}/{name}")
}

/// The DER bytes of the fixture client certificate.
fn client_cert_der() -> Vec<u8> {
    CertificateDer::from_pem_file(fixture("client.pem"))
        .expect("client.pem parses")
        .to_vec()
}

fn p12(domain: &str) -> ResolvedClientCertificate {
    ResolvedClientCertificate::pkcs12(
        domain,
        CertificateSource::File(fixture("client.p12")),
        Some("changeit".into()),
    )
}

fn pem(domain: &str) -> ResolvedClientCertificate {
    ResolvedClientCertificate::pem(
        domain,
        CertificateSource::File(fixture("client.pem")),
        CertificateSource::File(fixture("client-key.pem")),
        None,
    )
}

/// Requires a client certificate and accepts any one, so the test can record which one came.
///
/// `WebPkiClientVerifier` rejects `client.pem` because it is a self-signed `CA:TRUE` certificate
/// used as an end entity. This verifier keeps WebPki's CA hints and handshake signature checks,
/// so the client still has to prove it holds the key, and only skips the chain check.
#[derive(Debug)]
struct AnyClientCert {
    inner: Arc<dyn rustls::server::danger::ClientCertVerifier>,
}

impl rustls::server::danger::ClientCertVerifier for AnyClientCert {
    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        self.inner.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

fn server_config() -> rustls::ServerConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_file(fixture("client.pem")).expect("client.pem parses"))
        .expect("client.pem is a usable root");
    let inner = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        provider.clone(),
    )
    .build()
    .expect("client verifier builds");
    let verifier = Arc::new(AnyClientCert { inner });
    let cert = CertificateDer::from_pem_file(fixture("server.pem")).expect("server.pem parses");
    let key = PrivateKeyDer::from_pem_file(fixture("server-key.pem")).expect("server key parses");
    rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![cert], key)
        .expect("server certificate")
}

/// Starts a TLS server that requires a client certificate. Each connection that completes the
/// handshake records the client's certificate and gets an empty 200 response.
async fn start_mtls_server() -> (u16, Recorded) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let port = listener.local_addr().expect("local addr").port();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config()));
    let recorded: Recorded = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&recorded);
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let sink = Arc::clone(&sink);
            tokio::spawn(async move {
                // A rejected handshake is the expected outcome without a certificate.
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                if let Some(first) = tls.get_ref().1.peer_certificates().and_then(|c| c.first()) {
                    *sink.lock().expect("record lock") = Some(first.to_vec());
                }
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = tls
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                let _ = tls.shutdown().await;
            });
        }
    });
    (port, recorded)
}

fn recorded(r: &Recorded) -> Option<Vec<u8>> {
    r.lock().expect("record lock").clone()
}

/// Sends a GET through `ReqwestExecutor` with the given certificates, within 5 seconds.
async fn send(port: u16, certificates: Vec<ResolvedClientCertificate>) -> DomainResult<u16> {
    let mut request = HttpRequest::new(HttpMethod::Get, format!("https://127.0.0.1:{port}/"));
    // The server certificate is self-signed.
    request.options.verify_ssl = false;
    request.options.client_certificates = certificates;
    let exec = ReqwestExecutor::new();
    tokio::time::timeout(Duration::from_secs(5), exec.execute(&request))
        .await
        .expect("request finishes within 5 seconds")
        .map(|response| response.status)
}

#[tokio::test]
async fn mutual_tls_in_process_rejects_a_request_without_a_certificate() {
    let (port, seen) = start_mtls_server().await;
    let result = send(port, vec![]).await;
    assert!(
        result.is_err(),
        "server must reject a client without a certificate: {result:?}"
    );
    assert_eq!(recorded(&seen), None);
}

#[tokio::test]
async fn mutual_tls_in_process_presents_the_pkcs12_certificate() {
    let (port, seen) = start_mtls_server().await;
    let status = send(port, vec![p12("127.0.0.1")])
        .await
        .expect("PKCS12 identity accepted");
    assert_eq!(status, 200);
    assert_eq!(recorded(&seen), Some(client_cert_der()));
}

#[tokio::test]
async fn mutual_tls_in_process_presents_the_pem_certificate() {
    let (port, seen) = start_mtls_server().await;
    let status = send(port, vec![pem("127.0.0.1")])
        .await
        .expect("PEM identity accepted");
    assert_eq!(status, 200);
    assert_eq!(recorded(&seen), Some(client_cert_der()));
}

#[tokio::test]
async fn mutual_tls_in_process_does_not_present_a_certificate_for_another_domain() {
    let (port, seen) = start_mtls_server().await;
    let result = send(port, vec![p12("other.example.com")]).await;
    assert!(
        result.is_err(),
        "a certificate for another domain must not be sent: {result:?}"
    );
    assert_eq!(recorded(&seen), None);
}
