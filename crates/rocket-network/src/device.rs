//! The device id: SHA-256 of the device public key.

use std::fmt;

use data_encoding::BASE32_NOPAD;
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// Length of a base32 device id without padding.
const BASE32_LEN: usize = 52;

/// The only thing a node is known by.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId([u8; 32]);

impl DeviceId {
    /// Hashes a SubjectPublicKeyInfo DER blob into a device id.
    pub fn from_spki(spki_der: &[u8]) -> DeviceId {
        DeviceId(Sha256::digest(spki_der).into())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> DeviceId {
        DeviceId(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The full id as 52 uppercase base32 characters.
    pub fn to_base32(&self) -> String {
        BASE32_NOPAD.encode(&self.0)
    }

    /// Parses the canonical base32 form. Anything else is `InvalidInput`.
    pub fn parse_base32(s: &str) -> DomainResult<DeviceId> {
        let invalid = || DomainError::InvalidInput("invalid device id".to_string());
        if s.len() != BASE32_LEN {
            return Err(invalid());
        }
        let bytes = BASE32_NOPAD.decode(s.as_bytes()).map_err(|_| invalid())?;
        let array: [u8; 32] = bytes.try_into().map_err(|_| invalid())?;
        let id = DeviceId(array);
        // Re-encoding rejects non-canonical trailing bits.
        if id.to_base32() != s {
            return Err(invalid());
        }
        Ok(id)
    }

    /// The first 8 base32 characters as `XXXX-XXXX`, for display.
    pub fn short(&self) -> String {
        let full = self.to_base32();
        format!("{}-{}", &full[..4], &full[4..8])
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short())
    }
}

impl Serialize for DeviceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_base32())
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<DeviceId, D::Error> {
        let text = String::deserialize(deserializer)?;
        DeviceId::parse_base32(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainError;

    fn spki() -> Vec<u8> {
        vec![0xA1; 44]
    }

    #[test]
    fn from_spki_is_sha256_in_base32() {
        let id = DeviceId::from_spki(&spki());
        // SHA-256 of 44 bytes of 0xA1, computed outside Rust.
        assert_eq!(
            id.to_base32(),
            "PYE7HWE72X5NAUV5ZHYJISTMRAGMVJ5IYY2BHXJW4C2F5WJ5NCMQ"
        );
        assert_eq!(id.to_base32().len(), 52);
    }

    #[test]
    fn short_is_two_groups_of_four() {
        let id = DeviceId::from_spki(&spki());
        assert_eq!(id.short(), "PYE7-HWE7");
    }

    #[test]
    fn debug_and_display_print_the_short_form_only() {
        let id = DeviceId::from_spki(&spki());
        assert_eq!(format!("{id}"), "PYE7-HWE7");
        assert_eq!(format!("{id:?}"), "PYE7-HWE7");
    }

    #[test]
    fn base32_round_trips() {
        let id = DeviceId::from_bytes([7; 32]);
        let parsed = DeviceId::parse_base32(&id.to_base32()).expect("round trip");
        assert_eq!(parsed, id);
        assert_eq!(parsed.as_bytes(), &[7; 32]);
    }

    #[test]
    fn parse_base32_rejects_bad_input() {
        let good = DeviceId::from_spki(&spki()).to_base32();
        let too_short = &good[..51];
        let too_long = format!("{good}A");
        let lowercase = good.to_lowercase();
        let bad_char = format!("{}1", &good[..51]);
        // The last character carries one data bit, so only A and Q are canonical.
        let non_canonical = format!("{}B", "A".repeat(51));
        let multibyte = "é".repeat(26);
        for bad in [
            "",
            too_short,
            too_long.as_str(),
            lowercase.as_str(),
            bad_char.as_str(),
            non_canonical.as_str(),
            multibyte.as_str(),
        ] {
            let err = DeviceId::parse_base32(bad).expect_err("rejected");
            assert!(matches!(err, DomainError::InvalidInput(_)), "input {bad:?}");
        }
    }

    #[test]
    fn canonical_all_zero_id_parses() {
        let id = DeviceId::parse_base32(&"A".repeat(52)).expect("all zero is canonical");
        assert_eq!(id.as_bytes(), &[0; 32]);
    }

    #[test]
    fn serde_uses_the_base32_string() {
        let id = DeviceId::from_bytes([3; 32]);
        let json = serde_json::to_string(&id).expect("serializes");
        assert_eq!(json, format!("\"{}\"", id.to_base32()));
        let back: DeviceId = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, id);
    }

    #[test]
    fn serde_rejects_a_bad_string() {
        assert!(serde_json::from_str::<DeviceId>("\"nope\"").is_err());
        assert!(serde_json::from_str::<DeviceId>("42").is_err());
    }

    #[test]
    fn ids_order_by_bytes() {
        let low = DeviceId::from_bytes([1; 32]);
        let high = DeviceId::from_bytes([2; 32]);
        assert!(low < high);
        assert_eq!(std::cmp::min(low, high), low);
    }
}
