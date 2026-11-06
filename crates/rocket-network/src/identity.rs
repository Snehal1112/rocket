//! A peer's key identity and the local private key bytes.

use zeroize::Zeroize;

use crate::device::DeviceId;

/// A device id plus the SPKI DER bytes the TLS handshake showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerIdentity {
    pub device_id: DeviceId,
    pub spki_der: Vec<u8>,
}

impl PeerIdentity {
    pub fn from_spki(spki_der: Vec<u8>) -> PeerIdentity {
        PeerIdentity {
            device_id: DeviceId::from_spki(&spki_der),
            spki_der,
        }
    }
}

/// The PKCS#8 private key bytes. No `Debug` and no `Clone`, so the key cannot
/// be logged or copied by accident. The bytes are zeroed on drop.
pub struct DeviceKeyMaterial {
    pub pkcs8_der: Vec<u8>,
}

impl DeviceKeyMaterial {
    /// Overwrites every key byte with zero. The length stays the same.
    fn wipe(&mut self) {
        self.pkcs8_der.as_mut_slice().zeroize();
    }
}

impl Drop for DeviceKeyMaterial {
    fn drop(&mut self) {
        self.wipe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_identity_derives_the_device_id_from_the_spki() {
        let spki = vec![0xA1; 44];
        let peer = PeerIdentity::from_spki(spki.clone());
        assert_eq!(peer.device_id, DeviceId::from_spki(&spki));
        assert_eq!(peer.spki_der, spki);
    }

    #[test]
    fn same_spki_gives_equal_identities() {
        let a = PeerIdentity::from_spki(vec![1; 44]);
        let b = PeerIdentity::from_spki(vec![1; 44]);
        let c = PeerIdentity::from_spki(vec![2; 44]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn wipe_zeroes_every_byte_and_keeps_the_length() {
        let mut key = DeviceKeyMaterial {
            pkcs8_der: vec![0x5A; 48],
        };
        key.wipe();
        assert_eq!(key.pkcs8_der.len(), 48);
        assert!(key.pkcs8_der.iter().all(|b| *b == 0));
    }
}
