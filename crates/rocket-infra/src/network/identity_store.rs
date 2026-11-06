//! The device key in the OS keychain.
//!
//! The key is created once and then only read. Every keychain problem is an error: there is no
//! key file and no lenient read, because a silently new key would change the device id and
//! unpair every peer.

use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rcgen::{KeyPair, PKCS_ED25519};
use rocket_environment::secret_store::SecretStore;
use rocket_network::identity::DeviceKeyMaterial;
use rocket_network::ports::DeviceIdentityStore;
use rocket_shared::error::{DomainError, DomainResult};
use zeroize::Zeroizing;

use crate::network::lock::lock_ignore_poison;

/// Keychain scope of the device key.
const SCOPE: &str = "device";
/// Keychain key of the device key.
const KEY: &str = "ed25519-pkcs8";

/// Keeps the Ed25519 device key in a `SecretStore` (the OS keychain in production).
pub struct KeychainDeviceIdentity {
    store: Arc<dyn SecretStore>,
    /// Serialises the first creation, so two callers never make two keys.
    create_lock: Mutex<()>,
}

impl KeychainDeviceIdentity {
    pub fn new(store: Arc<dyn SecretStore>) -> Self {
        Self {
            store,
            create_lock: Mutex::new(()),
        }
    }

    fn create(&self) -> DomainResult<DeviceKeyMaterial> {
        let pair = KeyPair::generate_for(&PKCS_ED25519)
            .map_err(|_| DomainError::Internal("The device key could not be created".into()))?;
        let der = Zeroizing::new(pair.serialize_der());
        let text = Zeroizing::new(STANDARD.encode(der.as_slice()));
        self.store.set(SCOPE, KEY, text.as_str())?;
        // Read the value back. A keychain that accepts a write and forgets it would give a new
        // identity at every launch.
        let kept = self.store.get_strict(SCOPE, KEY)?.map(Zeroizing::new);
        match kept {
            Some(back) if back.as_str() == text.as_str() => Ok(DeviceKeyMaterial {
                pkcs8_der: der.to_vec(),
            }),
            _ => Err(DomainError::Internal(
                "The keychain did not keep the device key".into(),
            )),
        }
    }
}

impl DeviceIdentityStore for KeychainDeviceIdentity {
    fn load_or_create(&self) -> DomainResult<DeviceKeyMaterial> {
        let _guard = lock_ignore_poison(&self.create_lock);
        // A strict read: a locked keychain is an error here and never looks like "no key yet".
        match self.store.get_strict(SCOPE, KEY)? {
            Some(text) => decode_stored(&Zeroizing::new(text)),
            None => self.create(),
        }
    }
}

/// Decodes and checks a stored key. A damaged value is an error and is never replaced.
fn decode_stored(text: &Zeroizing<String>) -> DomainResult<DeviceKeyMaterial> {
    let damaged = || DomainError::Internal("The device key in the keychain is damaged".into());
    let mut bytes = Zeroizing::new(
        STANDARD
            .decode(text.trim().as_bytes())
            .map_err(|_| damaged())?,
    );
    let pair = KeyPair::try_from(bytes.as_slice()).map_err(|_| damaged())?;
    if pair.algorithm() != &PKCS_ED25519 {
        return Err(damaged());
    }
    Ok(DeviceKeyMaterial {
        pkcs8_der: std::mem::take(&mut *bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// In-memory keychain. `fail_reads` mimics a locked keychain: the lenient `get` reports a
    /// miss and `get_strict` fails, like `KeyringSecretStore`.
    #[derive(Default)]
    struct MemoryStore {
        entries: Mutex<HashMap<String, String>>,
        fail_reads: AtomicBool,
        fail_writes: AtomicBool,
        drop_writes: AtomicBool,
        set_calls: AtomicUsize,
    }

    impl MemoryStore {
        fn slot(scope_id: &str, key: &str) -> String {
            format!("{scope_id}:{key}")
        }

        fn put_raw(&self, value: &str) {
            self.entries
                .lock()
                .expect("entries lock")
                .insert(Self::slot(SCOPE, KEY), value.to_string());
        }

        fn raw(&self) -> Option<String> {
            self.entries
                .lock()
                .expect("entries lock")
                .get(&Self::slot(SCOPE, KEY))
                .cloned()
        }

        fn sets(&self) -> usize {
            self.set_calls.load(Ordering::SeqCst)
        }
    }

    impl SecretStore for MemoryStore {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            if self.fail_reads.load(Ordering::SeqCst) {
                return Ok(None);
            }
            Ok(self
                .entries
                .lock()
                .expect("entries lock")
                .get(&Self::slot(scope_id, key))
                .cloned())
        }

        fn get_strict(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            if self.fail_reads.load(Ordering::SeqCst) {
                return Err(DomainError::Internal(
                    "the keychain could not be read".into(),
                ));
            }
            self.get(scope_id, key)
        }

        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            self.set_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_writes.load(Ordering::SeqCst) {
                return Err(DomainError::Internal("keychain locked".into()));
            }
            if self.drop_writes.load(Ordering::SeqCst) {
                return Ok(());
            }
            self.entries
                .lock()
                .expect("entries lock")
                .insert(Self::slot(scope_id, key), value.to_string());
            Ok(())
        }

        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.entries
                .lock()
                .expect("entries lock")
                .remove(&Self::slot(scope_id, key));
            Ok(())
        }
    }

    fn identity_over(store: &Arc<MemoryStore>) -> KeychainDeviceIdentity {
        KeychainDeviceIdentity::new(store.clone())
    }

    #[test]
    fn creates_a_key_once_and_reuses_it() {
        let store = Arc::new(MemoryStore::default());
        let first = identity_over(&store).load_or_create().expect("first call");
        let again = identity_over(&store).load_or_create().expect("second call");
        let third = identity_over(&store)
            .load_or_create()
            .expect("a new instance over the same keychain");
        assert_eq!(first.pkcs8_der, again.pkcs8_der);
        assert_eq!(first.pkcs8_der, third.pkcs8_der);
        assert_eq!(store.sets(), 1, "only the first call writes");
    }

    #[test]
    fn the_stored_value_is_base64_pkcs8_under_the_device_scope() {
        let store = Arc::new(MemoryStore::default());
        let key = identity_over(&store).load_or_create().expect("create");
        let raw = store.raw().expect("a value is stored");
        let decoded = STANDARD.decode(raw.as_bytes()).expect("valid base64");
        assert_eq!(decoded, key.pkcs8_der);
        assert_eq!(SCOPE, "device");
        assert_eq!(KEY, "ed25519-pkcs8");
    }

    #[test]
    fn the_generated_key_is_an_ed25519_pkcs8_key() {
        let store = Arc::new(MemoryStore::default());
        let key = identity_over(&store).load_or_create().expect("create");
        let pair = rcgen::KeyPair::try_from(key.pkcs8_der.as_slice()).expect("parse the key");
        assert!(pair.algorithm() == &rcgen::PKCS_ED25519);
    }

    #[test]
    fn a_locked_keychain_is_an_error_and_never_creates_a_key() {
        // Empty keychain that cannot be read: no key may be created.
        let empty = Arc::new(MemoryStore::default());
        empty.fail_reads.store(true, Ordering::SeqCst);
        assert!(identity_over(&empty).load_or_create().is_err());
        assert_eq!(empty.sets(), 0);
        assert_eq!(empty.raw(), None);

        // Existing key in a keychain that is now locked: error, key untouched, and the same key
        // comes back once the keychain opens again.
        let store = Arc::new(MemoryStore::default());
        let original = identity_over(&store).load_or_create().expect("create");
        store.fail_reads.store(true, Ordering::SeqCst);
        assert!(identity_over(&store).load_or_create().is_err());
        assert_eq!(store.sets(), 1);
        store.fail_reads.store(false, Ordering::SeqCst);
        let back = identity_over(&store).load_or_create().expect("unlocked");
        assert_eq!(back.pkcs8_der, original.pkcs8_der);
    }

    #[test]
    fn a_failed_write_is_an_error() {
        let store = Arc::new(MemoryStore::default());
        store.fail_writes.store(true, Ordering::SeqCst);
        assert!(identity_over(&store).load_or_create().is_err());
        assert_eq!(store.raw(), None);
    }

    #[test]
    fn a_keychain_that_drops_writes_is_an_error() {
        let store = Arc::new(MemoryStore::default());
        store.drop_writes.store(true, Ordering::SeqCst);
        assert!(identity_over(&store).load_or_create().is_err());
        assert_eq!(store.raw(), None);
    }

    #[test]
    fn a_damaged_stored_key_is_an_error_and_is_left_alone() {
        let p256 = rcgen::KeyPair::generate().expect("generate a P-256 key");
        let wrong_algorithm = STANDARD.encode(p256.serialize_der());
        for damaged in [
            "%%% not base64 %%%".to_string(),
            STANDARD.encode(b"not a pkcs8 key"),
            wrong_algorithm,
            String::new(),
        ] {
            let store = Arc::new(MemoryStore::default());
            store.put_raw(&damaged);
            assert!(
                identity_over(&store).load_or_create().is_err(),
                "value {damaged:?} must be refused"
            );
            assert_eq!(store.sets(), 0, "a damaged key is never overwritten");
            assert_eq!(store.raw().as_deref(), Some(damaged.as_str()));
        }
    }

    #[test]
    fn concurrent_first_calls_create_one_key() {
        let store = Arc::new(MemoryStore::default());
        let identity = Arc::new(identity_over(&store));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let identity = identity.clone();
                std::thread::spawn(move || {
                    identity.load_or_create().expect("load").pkcs8_der.clone()
                })
            })
            .collect();
        let keys: Vec<Vec<u8>> = handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect();
        assert!(keys.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(store.sets(), 1);
    }
}
