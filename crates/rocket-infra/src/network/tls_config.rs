//! The rustls configs of the peer channel: TLS 1.3, mutual certificates, no certificate authority.
//!
//! Both sides accept any self-signed Ed25519 certificate, because trust comes from pairing and
//! not from a certificate chain. Two rules keep that safe. The handshake signature is always
//! verified, so a peer must hold the private key of the certificate it shows. And a client that
//! knows which device it dials passes `expected`, so a different key fails the handshake.
//!
//! Whether a key is paired or blocked is decided later by the server routes (Plan 03), from the
//! identity that `peer_identity_from_certs` reads off the finished connection.

use std::sync::Arc;

use rocket_network::device::DeviceId;
use rocket_network::identity::PeerIdentity;
use rocket_shared::error::{DomainError, DomainResult};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::{ResolvesClientCert, Resumption};
use rustls::crypto::{verify_tls13_signature, CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::server::{NoServerSessionStorage, ResolvesServerCert};
use rustls::sign::{CertifiedKey, SingleCertAndKey};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, DistinguishedName, Error, ServerConfig,
    SignatureScheme,
};

use super::tls_identity::{spki_from_cert, TlsIdentity};

/// The only signature scheme of a device key.
const DEVICE_SCHEME: SignatureScheme = SignatureScheme::ED25519;

fn setup_error(e: Error) -> DomainError {
    DomainError::Internal(format!("The TLS setup failed: {e}"))
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The server config: TLS 1.3 only, a client certificate is required and any Ed25519 key is
/// accepted after its handshake signature is verified.
pub fn server_config(identity: &TlsIdentity) -> DomainResult<Arc<ServerConfig>> {
    let certified = CertifiedKey::from_der(
        vec![identity.cert_der.clone()],
        PrivateKeyDer::Pkcs8(identity.key_der.clone_key()),
        &provider(),
    )
    .map_err(setup_error)?;
    server_config_with_resolver(Arc::new(SingleCertAndKey::from(certified)))
}

/// The client config: TLS 1.3 only, presents our certificate. With `expected = Some(id)` the
/// server's key must hash to `id`, otherwise the handshake fails.
pub fn client_config(
    identity: &TlsIdentity,
    expected: Option<DeviceId>,
) -> DomainResult<Arc<ClientConfig>> {
    let certified = CertifiedKey::from_der(
        vec![identity.cert_der.clone()],
        PrivateKeyDer::Pkcs8(identity.key_der.clone_key()),
        &provider(),
    )
    .map_err(setup_error)?;
    client_config_with_resolver(expected, Arc::new(SingleCertAndKey::from(certified)))
}

pub(crate) fn server_config_with_resolver(
    resolver: Arc<dyn ResolvesServerCert>,
) -> DomainResult<Arc<ServerConfig>> {
    let provider = provider();
    let verifier = Arc::new(PeerClientVerifier {
        algorithms: provider.signature_verification_algorithms,
    });
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(setup_error)?
        .with_client_cert_verifier(verifier)
        .with_cert_resolver(resolver);
    // Every connection runs the full certificate check: no tickets, no stored sessions.
    config.send_tls13_tickets = 0;
    config.session_storage = Arc::new(NoServerSessionStorage {});
    Ok(Arc::new(config))
}

pub(crate) fn client_config_with_resolver(
    expected: Option<DeviceId>,
    resolver: Arc<dyn ResolvesClientCert>,
) -> DomainResult<Arc<ClientConfig>> {
    let provider = provider();
    let verifier = Arc::new(PeerServerVerifier {
        expected,
        algorithms: provider.signature_verification_algorithms,
    });
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(setup_error)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_cert_resolver(resolver);
    config.resumption = Resumption::disabled();
    Ok(Arc::new(config))
}

/// Reads the identity of the peer from the certificates of a finished handshake. Only the first
/// certificate (the end entity) counts. No certificate is an error.
pub fn peer_identity_from_certs(
    certs: Option<&[CertificateDer<'_>]>,
) -> DomainResult<PeerIdentity> {
    let first = certs
        .and_then(|chain| chain.first())
        .ok_or_else(|| DomainError::InvalidInput("The peer sent no certificate".into()))?;
    Ok(PeerIdentity::from_spki(spki_from_cert(first.as_ref())?))
}

/// A device sends one certificate, which must parse. Nothing else is checked here.
fn check_device_certificate(
    end_entity: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
) -> Result<Vec<u8>, Error> {
    if !intermediates.is_empty() {
        return Err(Error::InvalidCertificate(CertificateError::BadEncoding));
    }
    spki_from_cert(end_entity.as_ref())
        .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))
}

/// Checks the handshake signature with the provider's algorithms. Only Ed25519 is allowed.
fn verify_device_signature(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
    algorithms: &WebPkiSupportedAlgorithms,
) -> Result<HandshakeSignatureValid, Error> {
    if dss.scheme != DEVICE_SCHEME {
        return Err(Error::General(
            "only Ed25519 device keys are accepted".into(),
        ));
    }
    verify_tls13_signature(message, cert, dss, algorithms)
}

fn tls12_refused() -> Error {
    Error::General("TLS 1.2 is not allowed".into())
}

/// Server side: requires a client certificate and accepts any Ed25519 key.
#[derive(Debug)]
struct PeerClientVerifier {
    algorithms: WebPkiSupportedAlgorithms,
}

impl ClientCertVerifier for PeerClientVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        check_device_certificate(end_entity, intermediates)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(tls12_refused())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_device_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![DEVICE_SCHEME]
    }
}

/// Client side: accepts any Ed25519 key, or only the expected one when the id is known.
#[derive(Debug)]
struct PeerServerVerifier {
    expected: Option<DeviceId>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PeerServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let spki = check_device_certificate(end_entity, intermediates)?;
        if let Some(expected) = &self.expected {
            if DeviceId::from_spki(&spki) != *expected {
                return Err(Error::InvalidCertificate(
                    CertificateError::ApplicationVerificationFailure,
                ));
            }
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(tls12_refused())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_device_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![DEVICE_SCHEME]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::tls_identity::test_support::identity;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    /// What each side of one handshake saw. `Ok` holds the identity of the other side.
    struct Outcome {
        server: Result<PeerIdentity, String>,
        client: Result<PeerIdentity, String>,
    }

    fn peer_of(certs: Option<&[CertificateDer<'static>]>) -> Result<PeerIdentity, String> {
        peer_identity_from_certs(certs).map_err(|e| e.to_string())
    }

    /// Runs a real TLS handshake over an in-memory pipe, then one ping and pong.
    async fn handshake(server: Arc<ServerConfig>, client: Arc<ClientConfig>) -> Outcome {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let acceptor = TlsAcceptor::from(server);
        let connector = TlsConnector::from(client);

        let server_task = tokio::spawn(async move {
            let mut tls = acceptor
                .accept(server_io)
                .await
                .map_err(|e| e.to_string())?;
            let peer = peer_of(tls.get_ref().1.peer_certificates())?;
            let mut ping = [0u8; 4];
            tls.read_exact(&mut ping).await.map_err(|e| e.to_string())?;
            tls.write_all(b"pong").await.map_err(|e| e.to_string())?;
            tls.flush().await.map_err(|e| e.to_string())?;
            Ok::<PeerIdentity, String>(peer)
        });
        let client_task = tokio::spawn(async move {
            let name = rustls::pki_types::ServerName::try_from("rocket-node").expect("name");
            let mut tls = connector
                .connect(name, client_io)
                .await
                .map_err(|e| e.to_string())?;
            let peer = peer_of(tls.get_ref().1.peer_certificates())?;
            tls.write_all(b"ping").await.map_err(|e| e.to_string())?;
            tls.flush().await.map_err(|e| e.to_string())?;
            let mut pong = [0u8; 4];
            tls.read_exact(&mut pong).await.map_err(|e| e.to_string())?;
            Ok::<PeerIdentity, String>(peer)
        });
        let (server, client) = tokio::time::timeout(Duration::from_secs(10), async {
            (server_task.await, client_task.await)
        })
        .await
        .expect("the handshake finishes");
        Outcome {
            server: server.expect("server task"),
            client: client.expect("client task"),
        }
    }

    /// A certificate that carries the public key of `cert_of` but is signed with the key of
    /// `key_of`. A peer that only hashes the certificate would accept it.
    fn stolen_certificate(
        cert_of: &crate::network::tls_identity::TlsIdentity,
        key_of: &crate::network::tls_identity::TlsIdentity,
    ) -> Arc<CertifiedKey> {
        let provider = rustls::crypto::ring::default_provider();
        let signing_key = provider
            .key_provider
            .load_private_key(PrivateKeyDer::Pkcs8(key_of.key_der.clone_key()))
            .expect("load the signing key");
        Arc::new(CertifiedKey::new(
            vec![cert_of.cert_der.clone()],
            signing_key,
        ))
    }

    #[tokio::test]
    async fn two_identities_complete_mutual_tls_and_see_each_other() {
        let a = identity();
        let b = identity();
        let outcome = handshake(
            server_config(&a).expect("server config"),
            client_config(&b, None).expect("client config"),
        )
        .await;
        assert_eq!(outcome.server.expect("server side"), b.local);
        assert_eq!(outcome.client.expect("client side"), a.local);
        assert_ne!(a.local.device_id, b.local.device_id);
    }

    #[tokio::test]
    async fn the_right_expected_id_is_accepted() {
        let a = identity();
        let b = identity();
        let outcome = handshake(
            server_config(&a).expect("server config"),
            client_config(&b, Some(a.local.device_id)).expect("client config"),
        )
        .await;
        assert_eq!(outcome.client.expect("client side"), a.local);
        assert_eq!(outcome.server.expect("server side"), b.local);
    }

    #[tokio::test]
    async fn a_wrong_expected_id_fails_the_client_handshake() {
        let a = identity();
        let b = identity();
        let other = identity();
        let outcome = handshake(
            server_config(&a).expect("server config"),
            client_config(&b, Some(other.local.device_id)).expect("client config"),
        )
        .await;
        assert!(outcome.client.is_err(), "client must refuse the wrong key");
        assert!(outcome.server.is_err(), "no data may flow after a refusal");
    }

    #[tokio::test]
    async fn a_tls12_only_client_is_refused() {
        let a = identity();
        let b = identity();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = Arc::new(PeerServerVerifier {
            expected: None,
            algorithms: provider.signature_verification_algorithms,
        });
        let client = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS12])
            .expect("tls 1.2 versions")
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_client_auth_cert(
                vec![b.cert_der.clone()],
                PrivateKeyDer::Pkcs8(b.key_der.clone_key()),
            )
            .expect("client auth cert");
        let outcome = handshake(server_config(&a).expect("server config"), Arc::new(client)).await;
        assert!(outcome.client.is_err());
        assert!(outcome.server.is_err());
    }

    #[tokio::test]
    async fn a_client_without_a_certificate_is_refused() {
        let a = identity();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = Arc::new(PeerServerVerifier {
            expected: None,
            algorithms: provider.signature_verification_algorithms,
        });
        let client = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .expect("tls 1.3 versions")
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        let outcome = handshake(server_config(&a).expect("server config"), Arc::new(client)).await;
        assert!(
            outcome.server.is_err(),
            "the server must require a certificate"
        );
        assert!(outcome.client.is_err());
    }

    #[tokio::test]
    async fn a_non_ed25519_device_key_is_refused() {
        let a = identity();
        let pair = rcgen::KeyPair::generate().expect("generate a P-256 key");
        let cert = rcgen::CertificateParams::new(Vec::<String>::new())
            .expect("params")
            .self_signed(&pair)
            .expect("self-signed certificate");
        let spki = spki_from_cert(cert.der().as_ref()).expect("spki");
        let ecdsa = crate::network::tls_identity::TlsIdentity {
            cert_der: cert.der().clone(),
            key_der: rustls::pki_types::PrivatePkcs8KeyDer::from(pair.serialize_der()),
            local: PeerIdentity::from_spki(spki),
        };

        // An ECDSA client cannot join an Ed25519 server.
        let client_case = handshake(
            server_config(&a).expect("server config"),
            client_config(&ecdsa, None).expect("client config"),
        )
        .await;
        assert!(client_case.server.is_err());

        // An ECDSA server cannot be reached by an Ed25519 client.
        let b = identity();
        let server_case = handshake(
            server_config(&ecdsa).expect("server config"),
            client_config(&b, None).expect("client config"),
        )
        .await;
        assert!(server_case.client.is_err());
    }

    #[tokio::test]
    async fn a_server_with_a_stolen_certificate_fails_the_handshake() {
        let victim = identity();
        let attacker = identity();
        let client = identity();
        let server = server_config_with_resolver(Arc::new(SingleCertAndKey::from(
            stolen_certificate(&victim, &attacker),
        )))
        .expect("server config");
        let outcome = handshake(
            server,
            client_config(&client, Some(victim.local.device_id)).expect("client config"),
        )
        .await;
        assert!(
            outcome.client.is_err(),
            "the handshake signature must prove the key, not only the hash"
        );
    }

    #[tokio::test]
    async fn a_client_with_a_stolen_certificate_fails_the_handshake() {
        let victim = identity();
        let attacker = identity();
        let server = identity();
        let client = client_config_with_resolver(
            None,
            Arc::new(SingleCertAndKey::from(stolen_certificate(
                &victim, &attacker,
            ))),
        )
        .expect("client config");
        let outcome = handshake(server_config(&server).expect("server config"), client).await;
        assert!(
            outcome.server.is_err(),
            "the server must verify the client's handshake signature"
        );
    }

    #[test]
    fn the_client_verifier_requires_a_certificate_and_names_no_authority() {
        let provider = rustls::crypto::ring::default_provider();
        let verifier = PeerClientVerifier {
            algorithms: provider.signature_verification_algorithms,
        };
        assert!(verifier.offer_client_auth());
        assert!(verifier.client_auth_mandatory());
        assert!(verifier.root_hint_subjects().is_empty());
        assert_eq!(
            verifier.supported_verify_schemes(),
            vec![SignatureScheme::ED25519]
        );
    }

    #[test]
    fn certificates_are_checked_for_shape() {
        let a = identity();
        let provider = rustls::crypto::ring::default_provider();
        let verifier = PeerClientVerifier {
            algorithms: provider.signature_verification_algorithms,
        };
        let now = rustls::pki_types::UnixTime::now();
        assert!(verifier.verify_client_cert(&a.cert_der, &[], now).is_ok());
        assert!(verifier
            .verify_client_cert(&a.cert_der, std::slice::from_ref(&a.cert_der), now)
            .is_err());
        let garbage = CertificateDer::from(vec![1u8, 2, 3]);
        assert!(verifier.verify_client_cert(&garbage, &[], now).is_err());
    }

    #[test]
    fn peer_identity_from_certs_reads_the_first_certificate() {
        let a = identity();
        let chain = [a.cert_der.clone()];
        let got = peer_identity_from_certs(Some(&chain)).expect("identity");
        assert_eq!(got, a.local);
    }

    #[test]
    fn peer_identity_from_certs_refuses_missing_and_invalid_certificates() {
        assert!(peer_identity_from_certs(None).is_err());
        assert!(peer_identity_from_certs(Some(&[])).is_err());
        let garbage = [CertificateDer::from(vec![0u8; 8])];
        assert!(peer_identity_from_certs(Some(&garbage)).is_err());
    }
}
