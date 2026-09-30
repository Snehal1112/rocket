//! Prepares a PEM private key for the TLS library.
//!
//! The platform TLS backends only accept an unencrypted PKCS#8 key that starts with
//! `-----BEGIN PRIVATE KEY-----`. An encrypted PKCS#8 key (`BEGIN ENCRYPTED PRIVATE KEY`) is
//! decrypted here in memory and re-encoded, so the decrypted key is never written to disk. The
//! old OpenSSL formats cannot be read and get a hint on how to convert them.

use pkcs8::der::pem::LineEnding;
use pkcs8::{EncryptedPrivateKeyInfo, SecretDocument};
use rocket_shared::error::{DomainError, DomainResult};
use zeroize::Zeroizing;

const ENCRYPTED_HEADER: &[u8] = b"-----BEGIN ENCRYPTED PRIVATE KEY-----";

/// DER encoding of the PBES2 object identifier (1.2.840.113549.1.5.13). A PKCS#8 key encrypted
/// with anything else (PBES1 or a PKCS#12 scheme) cannot be parsed by the `pkcs8` crate.
const PBES2_OID_DER: &[u8] = &[
    0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x05, 0x0D,
];

/// Returns the key as an unencrypted PKCS#8 PEM, decrypting it with `passphrase` if needed.
/// `path` is only used in error messages. The passphrase is never put in an error.
pub(crate) fn unencrypted_key_pem(
    key_pem: &[u8],
    passphrase: Option<&str>,
    path: &str,
) -> DomainResult<Zeroizing<Vec<u8>>> {
    if contains(key_pem, b"Proc-Type: 4,ENCRYPTED") {
        return Err(DomainError::InvalidInput(format!(
            "The private key {path} uses the old OpenSSL encrypted format, which is not supported. \
             Convert it with `openssl pkcs8 -topk8 -v2 aes-256-cbc -in key.pem -out key-pkcs8.pem`, \
             or use a PKCS12 bundle."
        )));
    }
    if !contains(key_pem, ENCRYPTED_HEADER) {
        return Ok(Zeroizing::new(key_pem.to_vec()));
    }

    let passphrase = passphrase.filter(|p| !p.is_empty()).ok_or_else(|| {
        DomainError::InvalidInput(format!(
            "The private key {path} is encrypted. Enter its passphrase."
        ))
    })?;
    let text = std::str::from_utf8(key_pem).map_err(|_| damaged(path))?;
    let (_, document) = SecretDocument::from_pem(text).map_err(|_| damaged(path))?;
    // Only PBES2 (PBKDF2 or scrypt with AES or 3DES) is supported. Older PBES1 keys are what
    // OpenSSL 1.x wrote by default, and the parser rejects them, so check for it first to give
    // the right message instead of "damaged".
    if !contains(document.as_bytes(), PBES2_OID_DER) {
        return Err(DomainError::InvalidInput(format!(
            "The private key {path} uses an old encryption scheme (PBES1), which is not supported. \
             Convert it with `openssl pkcs8 -topk8 -v2 aes-256-cbc -in key.pem -out key-pkcs8.pem`, \
             or use a PKCS12 bundle."
        )));
    }
    let encrypted =
        EncryptedPrivateKeyInfo::try_from(document.as_bytes()).map_err(|_| damaged(path))?;

    let decrypted = encrypted.decrypt(passphrase).map_err(|_| {
        DomainError::InvalidInput(format!(
            "Cannot decrypt the private key {path}: wrong passphrase or damaged file."
        ))
    })?;
    let pem = decrypted
        .to_pem("PRIVATE KEY", LineEnding::LF)
        .map_err(|_| damaged(path))?;
    Ok(Zeroizing::new(pem.as_bytes().to_vec()))
}

fn damaged(path: &str) -> DomainError {
    DomainError::InvalidInput(format!(
        "Cannot read the private key {path}: the file is damaged or not a PKCS#8 key."
    ))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test-fixtures/mtls");

    fn read(name: &str) -> Vec<u8> {
        std::fs::read(format!("{DIR}/{name}")).expect("fixture exists")
    }

    /// The base64 body of a PEM, so two encodings of the same key compare equal.
    fn body(pem: &[u8]) -> String {
        String::from_utf8_lossy(pem)
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect()
    }

    fn unencrypted(name: &str, pass: Option<&str>) -> DomainResult<Zeroizing<Vec<u8>>> {
        unencrypted_key_pem(&read(name), pass, name)
    }

    #[test]
    fn an_unencrypted_key_is_returned_unchanged() {
        let key = unencrypted("client-key.pem", None).unwrap();
        assert_eq!(key.as_slice(), read("client-key.pem").as_slice());
    }

    #[test]
    fn each_pbes2_variant_decrypts_to_the_original_key() {
        let original = body(&read("client-key.pem"));
        for name in [
            "client-key-encrypted.pem", // PBKDF2-SHA256 + AES-256-CBC
            "client-key-scrypt.pem",    // scrypt + AES-256-CBC
            "client-key-3des.pem",      // PBKDF2 + 3DES-CBC
            "client-key-sha1prf.pem",   // PBKDF2-HMAC-SHA1 + AES-256-CBC
        ] {
            let key = unencrypted(name, Some("changeit")).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(key.starts_with(b"-----BEGIN PRIVATE KEY-----"), "{name}");
            assert_eq!(body(&key), original, "{name}");
        }
    }

    #[test]
    fn a_wrong_passphrase_names_the_file_and_never_echoes_the_passphrase() {
        let err = unencrypted("client-key-encrypted.pem", Some("hunter2-wrong"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("wrong passphrase") && err.contains("client-key-encrypted.pem"),
            "{err}"
        );
        assert!(!err.contains("hunter2-wrong"), "{err}");
    }

    #[test]
    fn a_missing_or_empty_passphrase_asks_for_one() {
        for pass in [None, Some("")] {
            let err = unencrypted("client-key-encrypted.pem", pass)
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("is encrypted") && err.contains("passphrase"),
                "{err}"
            );
        }
    }

    #[test]
    fn the_old_pbes1_scheme_is_rejected_with_a_conversion_hint() {
        let err = unencrypted("client-key-pbes1.pem", Some("changeit"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("PBES1") && err.contains("openssl pkcs8 -topk8 -v2"),
            "{err}"
        );
    }

    #[test]
    fn the_traditional_openssl_format_is_rejected_with_a_conversion_hint() {
        let err = unencrypted("client-key-traditional.pem", Some("changeit"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("old OpenSSL") && err.contains("openssl pkcs8 -topk8 -v2"),
            "{err}"
        );
    }

    #[test]
    fn a_damaged_encrypted_key_is_an_error_not_a_panic() {
        let mut pem = read("client-key-encrypted.pem");
        pem.truncate(pem.len() / 2);
        pem.extend_from_slice(b"\n-----END ENCRYPTED PRIVATE KEY-----\n");
        let err = unencrypted_key_pem(&pem, Some("changeit"), "k.pem")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("k.pem") && (err.contains("damaged") || err.contains("wrong passphrase")),
            "{err}"
        );
    }
}
