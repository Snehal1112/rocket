//! The self-signed device certificate and the device id taken from it.

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, PKCS_ED25519};
use rocket_network::identity::{DeviceKeyMaterial, PeerIdentity};
use rocket_shared::error::{DomainError, DomainResult};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

/// Common name of the device certificate. The name carries no meaning: peers trust the key.
const CERT_COMMON_NAME: &str = "rocket-node";

/// The certificate and private key rustls presents, and the identity they stand for.
///
/// There is no `Debug` or `Clone`: the private key must not be printed or copied by accident.
pub struct TlsIdentity {
    pub cert_der: CertificateDer<'static>,
    pub key_der: PrivatePkcs8KeyDer<'static>,
    pub local: PeerIdentity,
}

/// Makes the self-signed Ed25519 certificate for the device key.
///
/// `local.device_id` is the SHA-256 of the SubjectPublicKeyInfo read back out of the finished
/// certificate. A peer computes the same bytes from the certificate it receives.
pub fn build_tls_identity(key: &DeviceKeyMaterial) -> DomainResult<TlsIdentity> {
    let key_der = PrivatePkcs8KeyDer::from(key.pkcs8_der.clone());
    let pair = KeyPair::try_from(&key_der)
        .map_err(|_| DomainError::Internal("The device key could not be read".into()))?;
    if pair.algorithm() != &PKCS_ED25519 {
        return Err(DomainError::Internal(
            "The device key is not an Ed25519 key".into(),
        ));
    }
    let mut params = CertificateParams::new(Vec::<String>::new())
        .map_err(|_| DomainError::Internal("The device certificate could not be made".into()))?;
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, CERT_COMMON_NAME);
    params.distinguished_name = name;
    let cert = params
        .self_signed(&pair)
        .map_err(|_| DomainError::Internal("The device certificate could not be made".into()))?;
    let cert_der = cert.der().clone();
    let spki = spki_from_cert(cert_der.as_ref())?;
    Ok(TlsIdentity {
        cert_der,
        key_der,
        local: PeerIdentity::from_spki(spki),
    })
}

/// Returns the SubjectPublicKeyInfo DER of a certificate. The certificate must be one complete
/// DER value with no bytes after it.
pub fn spki_from_cert(cert_der: &[u8]) -> DomainResult<Vec<u8>> {
    let invalid = || DomainError::InvalidInput("The peer certificate is not valid".into());
    let der = rustls::pki_types::CertificateDer::from(cert_der);
    let cert = webpki::EndEntityCert::try_from(&der).map_err(|_| invalid())?;
    Ok(cert.subject_public_key_info().as_ref().to_vec())
}

/// Shared helpers for the tests of this module directory and, through the `test-support`
/// feature, for the end-to-end tests in `rocket-app` (Plan 04b).
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::{build_tls_identity, TlsIdentity};
    use rocket_network::identity::DeviceKeyMaterial;

    /// A fresh Ed25519 device key.
    pub fn key_material() -> DeviceKeyMaterial {
        let pair =
            rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).expect("generate an Ed25519 key");
        DeviceKeyMaterial {
            pkcs8_der: pair.serialize_der(),
        }
    }

    /// A fresh device identity with its own key and certificate.
    pub fn identity() -> TlsIdentity {
        build_tls_identity(&key_material()).expect("build a device identity")
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{identity, key_material};
    use super::*;
    use rocket_network::device::DeviceId;
    use rocket_network::identity::DeviceKeyMaterial;

    #[test]
    fn the_device_id_is_the_hash_of_the_certificate_public_key() {
        let key = key_material();
        let built = build_tls_identity(&key).expect("build");
        let spki = spki_from_cert(built.cert_der.as_ref()).expect("spki from the certificate");
        assert_eq!(built.local.device_id, DeviceId::from_spki(&spki));
        assert_eq!(built.local.spki_der, spki);
    }

    #[test]
    fn the_certificate_public_key_equals_the_public_key_of_the_key_pair() {
        let key = key_material();
        let built = build_tls_identity(&key).expect("build");
        let pair = rcgen::KeyPair::try_from(&PrivatePkcs8KeyDer::from(key.pkcs8_der.clone()))
            .expect("parse the key");
        assert_eq!(pair.public_key_der(), built.local.spki_der);
    }

    #[test]
    fn the_same_key_always_gives_the_same_device_id() {
        let key = key_material();
        let first = build_tls_identity(&key).expect("first");
        let second = build_tls_identity(&key).expect("second");
        assert_eq!(first.local.device_id, second.local.device_id);
        assert_ne!(identity().local.device_id, first.local.device_id);
    }

    #[test]
    fn the_private_key_in_the_identity_is_the_input_key() {
        let key = key_material();
        let built = build_tls_identity(&key).expect("build");
        assert_eq!(built.key_der.secret_pkcs8_der(), key.pkcs8_der.as_slice());
    }

    #[test]
    fn a_key_that_is_not_pkcs8_is_refused() {
        let bad = DeviceKeyMaterial {
            pkcs8_der: vec![1, 2, 3, 4],
        };
        assert!(build_tls_identity(&bad).is_err());
    }

    #[test]
    fn a_key_of_another_algorithm_is_refused() {
        let pair = rcgen::KeyPair::generate().expect("generate a P-256 key");
        let key = DeviceKeyMaterial {
            pkcs8_der: pair.serialize_der(),
        };
        assert!(build_tls_identity(&key).is_err());
    }

    #[test]
    fn spki_from_cert_refuses_garbage_and_trailing_bytes() {
        assert!(spki_from_cert(&[]).is_err());
        assert!(spki_from_cert(&[0x30, 0x03, 0x01, 0x01]).is_err());
        let built = identity();
        let mut padded = built.cert_der.as_ref().to_vec();
        padded.push(0);
        assert!(spki_from_cert(&padded).is_err());
    }
}
