# Team Network — Plan 2: Identity, storage and TLS

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 2 of 5.** Previous plan: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-01-domain.md` (must be merged and green).
**Next plan to run: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-03-transport-discovery.md`**
**Recommended model: Sonnet.**

**Goal:** `rocket-infra` can keep a stable Ed25519 device key in the OS keychain, turn it into a self-signed certificate whose public key hashes to the device id, persist the four small YAML lists under `~/.rocket-api/network/`, and build rustls server and client configs that run mutual TLS 1.3 with a custom "any self-signed Ed25519 key, but pin it when the id is known" check. Nothing listens on a socket yet.

**Architecture:** A new module directory `crates/rocket-infra/src/network/` holds four files. `identity_store.rs` implements the `DeviceIdentityStore` port over the existing `SecretStore` trait (new keychain namespace `com.rocketapi.device-identity`), with strict reads and no file fallback. `tls_identity.rs` makes the certificate with `rcgen` and reads the public key back with `rustls-webpki` (already in the tree through rustls), so `DeviceId = SHA-256(SPKI DER)` is computed from the same bytes on both sides of a connection. `repos.rs` has four YAML repos that share one private `YamlFile` helper (lock, atomic write, quarantine of damaged files). `tls_config.rs` builds the rustls configs with two small verifiers that still check the handshake signature, so a peer must hold the private key of the certificate it shows. Plan 03 builds the TLS server, the connector and mDNS on top of these.

**Tech Stack:** Rust, `rcgen` 0.13 (now a normal dependency), `rustls-webpki` 0.103 (already locked at 0.103.10), `rustls` 0.23 (ring provider, TLS 1.3 only), `tokio-rustls` 0.26, `keyring` through the existing `SecretStore`, `serde_yaml`, `tempfile` and `tokio::io::duplex` in tests.

**Spec:** `docs/superpowers/specs/2026-11-05-team-network-discovery-pairing-design.md` sections 5.2 (`rocket-infra`), 6.1 (channel), 8.2 (TLS and identity) and 9 (error handling). Index: `docs/superpowers/plans/team-network/00-plan-index.md` (binding names, deviations 1, 3 and 7).

## Global Constraints

- `-j4` on every cargo invocation. Never `cargo test --workspace` or `--all`. Use `cargo test -j4 -p rocket-infra <filter>`.
- Use only the names from the index. `rocket-network` (Plan 01) is imported by module path: `rocket_network::{device, identity, limits, ports, records}`. If Plan 01 also re-exports them at the crate root, either spelling works.
- No `unwrap` or `expect` in production code. Tests use `expect("reason")`. Never write a bare unwrap call anywhere (a repo hook rejects Rust text that contains one, tests included). Mutex locks use `.lock().unwrap_or_else(|e| e.into_inner())` in production code.
- The private key is never logged, never put in an error message and never written to a file. Local copies of key bytes and of the base64 text live in `zeroize::Zeroizing`. Errors use fixed generic texts.
- Fail closed: any keychain error from `load_or_create` is an `Err`. No key file, no environment variable, no lenient read.
- Persistence structs keep snake_case serde (the records come from Plan 01). No camelCase here.
- Writes go through `crate::atomic_write`. A damaged YAML file is renamed to `<name>.corrupt-<unix-secs>` and the list starts empty (index deviation 1). Nothing is deleted.
- Code comments are short, full sentences that end with a period.
- No commits during this plan. Task 2 writes `.yml` files, so it starts with the OpenCollection read step.
- Line numbers drift. Edits are anchored on quoted text.
- Verification per task: `cargo check -j4 -p rocket-infra` and the task's `cargo test -j4 -p rocket-infra <filter>`. Before the final hand-off also `cargo check -j4 --workspace`.

## Review Focus

- **Keychain locked or unreadable returns `Err`, never a new key (Task 1).** `identity_store::tests::a_locked_keychain_is_an_error_and_never_creates_a_key` makes the lenient `get` report a miss while `get_strict` fails, and asserts that `set` was never called. A regenerated key would silently change the device id and unpair every peer.
- **A key is generated once and reused (Task 1).** `creates_a_key_once_and_reuses_it` (two calls, a second instance, one `set`) and `concurrent_first_calls_create_one_key` (eight threads, one `set`) prove it. `a_keychain_that_drops_writes_is_an_error` proves that a store that forgets writes is caught by the read-back, not discovered at the next launch.
- **A damaged `paired.yml` is quarantined once (Task 2).** `a_damaged_file_is_set_aside_once_and_reported_once` checks the `.corrupt-<secs>` file keeps the original bytes, `take_quarantined()` returns the file name exactly once, and the next `upsert` writes a clean file. A blank file is not damage (`a_blank_file_is_empty_and_not_quarantined`).
- **Manual addresses dedupe case-insensitively and caps hold (Task 2).** `manual_addresses_dedupe_on_lowercased_host_and_port` (`Host.Example` and `host.example` are one entry, the first `added_at` stays), `the_paired_list_stops_at_the_limit_but_allows_updates` (the 201st new device is refused, updating an existing one is not) and `manual_addresses_stop_at_the_limit`.
- **Expected id mismatch and impersonation fail at the handshake (Task 3).** `a_wrong_expected_id_fails_the_client_handshake` has a positive control with the right id. `a_server_with_a_stolen_certificate_fails_the_handshake` and `a_client_with_a_stolen_certificate_fails_the_handshake` present certificate A with the key of B and prove that the signature is verified, not only the key hash.
- **TLS 1.2, no client certificate and non-Ed25519 keys are refused (Task 3).** `a_tls12_only_client_is_refused`, `a_client_without_a_certificate_is_refused` and `a_non_ed25519_device_key_is_refused`.

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/rocket-infra/Cargo.toml` | modify | Add `rocket-network`, `rustls`, `rustls-webpki`, `tokio-rustls`; move `rcgen` from dev to normal dependencies |
| `crates/rocket-infra/src/lib.rs` | modify | `pub mod network;` and re-exports of the infra types |
| `crates/rocket-infra/src/secret_store.rs` | modify | `KeyringSecretStore::new_device_identity()` and every distinct-namespace test list |
| `crates/rocket-infra/src/network/mod.rs` | create | Module list and re-exports |
| `crates/rocket-infra/src/network/identity_store.rs` | create | `KeychainDeviceIdentity` |
| `crates/rocket-infra/src/network/tls_identity.rs` | create | `TlsIdentity`, `build_tls_identity`, `spki_from_cert`, test helpers |
| `crates/rocket-infra/src/network/repos.rs` | create | Four YAML repos, shared `YamlFile`, `QuarantineReport` impls |
| `crates/rocket-infra/src/network/tls_config.rs` | create | `server_config`, `client_config`, `peer_identity_from_certs`, the two verifiers |

---

### Task 1: Device key in the keychain, certificate and device id

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml`, `crates/rocket-infra/src/lib.rs`, `crates/rocket-infra/src/secret_store.rs`
- Create: `crates/rocket-infra/src/network/mod.rs`, `crates/rocket-infra/src/network/identity_store.rs`, `crates/rocket-infra/src/network/tls_identity.rs`
- Test: inline `#[cfg(test)] mod tests` in the three source files.

**Interfaces:**
- Consumes (Plan 01): `rocket_network::device::DeviceId` (`from_spki`), `rocket_network::identity::{PeerIdentity, DeviceKeyMaterial}` (`PeerIdentity::from_spki(Vec<u8>)`, `DeviceKeyMaterial { pub pkcs8_der: Vec<u8> }`), `rocket_network::ports::DeviceIdentityStore`; `rocket_environment::secret_store::SecretStore`; `rocket_shared::error::{DomainError, DomainResult}`.
- Produces (used by Task 3 and Plan 03/04):

```rust
// secret_store.rs
impl KeyringSecretStore {
    /// Backs the device key of the team network ("com.rocketapi.device-identity").
    pub fn new_device_identity() -> Self;
}

// network/identity_store.rs
pub struct KeychainDeviceIdentity { /* store, create_lock */ }
impl KeychainDeviceIdentity { pub fn new(store: Arc<dyn SecretStore>) -> Self; }
impl DeviceIdentityStore for KeychainDeviceIdentity {
    fn load_or_create(&self) -> DomainResult<DeviceKeyMaterial>;
}
// keychain scope "device", key "ed25519-pkcs8", value = base64 (STANDARD) of the PKCS#8 DER.

// network/tls_identity.rs
pub struct TlsIdentity {
    pub cert_der: CertificateDer<'static>,
    pub key_der: PrivatePkcs8KeyDer<'static>,
    pub local: PeerIdentity,
}
pub fn build_tls_identity(key: &DeviceKeyMaterial) -> DomainResult<TlsIdentity>;
pub fn spki_from_cert(cert_der: &[u8]) -> DomainResult<Vec<u8>>;
```

**New dependency decisions (stated, because the tree cannot be resolved offline):**
- `rustls-webpki = { version = "0.103", default-features = false, features = ["alloc", "ring"] }`. It is already in `Cargo.lock` at 0.103.10 as a rustls dependency, so no new crate is downloaded. API used (checked in the 0.103.10 source): `webpki::EndEntityCert::try_from(&CertificateDer<'_>)` and, through `Deref` to `webpki::Cert`, `subject_public_key_info() -> SubjectPublicKeyInfoDer<'static>` (the full SPKI SEQUENCE, `cert.rs:219`). This replaces `x509-parser`, which is not in the local registry (decided by the plan author's reviewer on 2026-11-06).
- `rustls = "0.23"` with `default-features = false, features = ["ring", "std"]` (the lockfile has 0.23.37, so no new version). `tls12` is not enabled by this crate. The Task 3 TLS 1.2 test relies on the `tls12` feature that the existing dev-dependency `tokio-rustls` (`["ring", "tls12"]`) turns on for test builds.
- `rcgen = "0.13"` unchanged, now under `[dependencies]` with the same features (`crypto`, `ring`, `pem`).

- [ ] **Step 1: Add the dependencies**

In `crates/rocket-infra/Cargo.toml`, under `[dependencies]`, after the line `dotenvy = "0.15.7"`, add:

```toml
# Team network: the domain crate, the TLS stack with the custom certificate checks, the device
# certificate maker and the certificate reader. All crypto uses the ring provider.
rocket-network = { path = "../rocket-network" }
rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }
rcgen = { version = "0.13", default-features = false, features = ["crypto", "ring", "pem"] }
rustls-webpki = { version = "0.103", default-features = false, features = ["alloc", "ring"] }
```

Under `[dev-dependencies]`, delete this line (it moved to the normal dependencies):

```toml
rcgen = { version = "0.13", default-features = false, features = ["crypto", "ring", "pem"] }
```

- [ ] **Step 2: Add the module skeleton**

Create `crates/rocket-infra/src/network/mod.rs`:

```rust
//! Team network pieces that touch the machine: the device key, the local lists and the TLS
//! configs. The domain rules live in `rocket-network`.

pub mod identity_store;
pub mod tls_identity;

pub use identity_store::KeychainDeviceIdentity;
pub use tls_identity::{build_tls_identity, spki_from_cert, TlsIdentity};
```

Create `crates/rocket-infra/src/network/identity_store.rs` with this single line:

```rust
//! The device key in the OS keychain.
```

Create `crates/rocket-infra/src/network/tls_identity.rs` with this single line:

```rust
//! The self-signed device certificate and the device id taken from it.
```

In `crates/rocket-infra/src/lib.rs`, replace the quoted text

```rust
pub(crate) mod migration;
pub(crate) mod oc;
```

with

```rust
pub(crate) mod migration;
pub mod network;
pub(crate) mod oc;
```

and after the line `pub use loopback_callback::LoopbackCallbackServer;` add:

```rust
pub use network::{build_tls_identity, spki_from_cert, KeychainDeviceIdentity, TlsIdentity};
```

(The re-export block is extended in Tasks 2 and 3. The TLS config functions stay at `rocket_infra::network::{server_config, client_config, peer_identity_from_certs}` because the names are too generic for the crate root.)

Run: `cargo check -j4 -p rocket-infra`
Expected: PASS (the new modules have no code yet; `Cargo.lock` gains only the new direct-dependency edges, no new packages).

- [ ] **Step 3: Write the failing tests**

3a. In `crates/rocket-infra/src/secret_store.rs`, in the test `the_agent_credentials_namespace_is_distinct`, replace the quoted text

```rust
            KeyringSecretStore::new_proxy(),
        ] {
            assert_ne!(agent.service, other.service);
```

with

```rust
            KeyringSecretStore::new_proxy(),
            KeyringSecretStore::new_device_identity(),
        ] {
            assert_ne!(agent.service, other.service);
```

In the test `the_openapi_sync_namespace_is_distinct`, replace the quoted text

```rust
            KeyringSecretStore::new_agent_credentials(),
        ] {
            assert_ne!(sync.service, other.service);
```

with

```rust
            KeyringSecretStore::new_agent_credentials(),
            KeyringSecretStore::new_device_identity(),
        ] {
            assert_ne!(sync.service, other.service);
```

Then add this test directly after `the_openapi_sync_namespace_is_distinct` (before `a_missing_entry_is_a_miss_in_both_modes`):

```rust
    #[test]
    fn the_device_identity_namespace_is_distinct() {
        let device = KeyringSecretStore::new_device_identity();
        assert_eq!(device.service, "com.rocketapi.device-identity");
        for other in [
            KeyringSecretStore::new_env_secrets(),
            KeyringSecretStore::new_vault_connections(),
            KeyringSecretStore::new_proxy(),
            KeyringSecretStore::new_openapi_sync(),
            KeyringSecretStore::new_agent_credentials(),
        ] {
            assert_ne!(device.service, other.service);
        }
    }
```

3b. Append to `crates/rocket-infra/src/network/tls_identity.rs`:

```rust
/// Shared helpers for the tests of this module directory.
#[cfg(test)]
pub(crate) mod test_support {
    use super::{build_tls_identity, TlsIdentity};
    use rocket_network::identity::DeviceKeyMaterial;

    /// A fresh Ed25519 device key.
    pub(crate) fn key_material() -> DeviceKeyMaterial {
        let pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
            .expect("generate an Ed25519 key");
        DeviceKeyMaterial {
            pkcs8_der: pair.serialize_der(),
        }
    }

    /// A fresh device identity with its own key and certificate.
    pub(crate) fn identity() -> TlsIdentity {
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
```

3c. Append to `crates/rocket-infra/src/network/identity_store.rs`:

```rust
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
                std::thread::spawn(move || identity.load_or_create().expect("load").pkcs8_der)
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
```

- [ ] **Step 4: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-infra network::`
Expected: FAIL to compile. First errors: `unresolved import super::PrivatePkcs8KeyDer`-style or `cannot find function build_tls_identity in this scope`, `cannot find type KeychainDeviceIdentity`, and for `secret_store`: `no function or associated item named new_device_identity found for struct KeyringSecretStore`.

- [ ] **Step 5: Add the keychain namespace**

In `crates/rocket-infra/src/secret_store.rs`, directly after the `new_agent_credentials` constructor (before the closing `}` of `impl KeyringSecretStore`), add:

```rust

    /// Backs the device key of the team network (scope_id = `device`, key = `ed25519-pkcs8`).
    /// A separate keychain service from the other namespaces so the entries can never collide.
    pub fn new_device_identity() -> Self {
        Self {
            service: "com.rocketapi.device-identity",
        }
    }
```

- [ ] **Step 6: Implement the certificate**

Replace the single doc line at the top of `crates/rocket-infra/src/network/tls_identity.rs` (keep everything appended in Step 3) by inserting the following above the `/// Shared helpers` line. The first line of the file stays `//! The self-signed device certificate and the device id taken from it.`

```rust

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

```

- [ ] **Step 7: Implement the keychain identity**

Replace the single doc line `//! The device key in the OS keychain.` at the top of `crates/rocket-infra/src/network/identity_store.rs` with the code below (keep the test module from Step 3 at the bottom):

```rust
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
        let _guard = self.create_lock.lock().unwrap_or_else(|e| e.into_inner());
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
```

- [ ] **Step 8: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-infra network::`
Expected: PASS (all tests in `network::tls_identity::tests` and `network::identity_store::tests`).

Run: `cargo test -j4 -p rocket-infra secret_store::tests`
Expected: PASS, including `the_device_identity_namespace_is_distinct`.

If `the_certificate_public_key_equals_the_public_key_of_the_key_pair` fails, the certificate writes the key differently from `KeyPair::public_key_der()`. Stop and report it: the whole plan depends on both sides hashing the same bytes, so do not weaken the test.

- [ ] **Step 9: Check**

Run: `cargo check -j4 -p rocket-infra`
Expected: PASS with no warnings from the new files.

No commit. The whole plan is reviewed at the plan end.

---

### Task 2: The four YAML repos with quarantine

**Files:**
- Create: `crates/rocket-infra/src/network/repos.rs`
- Modify: `crates/rocket-infra/src/network/mod.rs`, `crates/rocket-infra/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `repos.rs`.

**Interfaces:**
- Consumes (Plan 01): `rocket_network::ports::{PairedDeviceRepository, BlockedDeviceRepository, ManualAddressRepository, NetworkSettingsRepository, QuarantineReport}`, `rocket_network::records::{PairedDevice, BlockedDevice, ManualAddress, NetworkSettings}`, `rocket_network::device::DeviceId`, `rocket_network::limits::{MAX_PAIRED_DEVICES, MAX_MANUAL_ADDRESSES}`; `crate::atomic_write`.
- Produces (used by Plan 04 wiring in `src-tauri/src/lib.rs`):

```rust
pub struct FsPairedDeviceRepo;   // new(path: PathBuf) -> Self   file: paired.yml
pub struct FsBlockedDeviceRepo;  // new(path: PathBuf) -> Self   file: blocked.yml
pub struct FsManualAddressRepo;  // new(path: PathBuf) -> Self   file: addresses.yml
pub struct FsNetworkSettingsRepo; // new(path: PathBuf) -> Self  file: settings.yml
// Each implements its port from the index and `QuarantineReport`.
// Files (private wrappers, snake_case): paired.yml and blocked.yml hold `devices: [...]`,
// addresses.yml holds `addresses: [...]`, settings.yml is the `NetworkSettings` map itself.
```

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. The network files are not collection files and use none of its formats, but this task writes `.yml` files, so the repo rule applies.

- [ ] **Step 2: Add the module and a skeleton**

Create `crates/rocket-infra/src/network/repos.rs` with this single line:

```rust
//! The four small YAML lists of the team network, kept under `~/.rocket-api/network/`.
```

In `crates/rocket-infra/src/network/mod.rs`, replace the whole content with:

```rust
//! Team network pieces that touch the machine: the device key, the local lists and the TLS
//! configs. The domain rules live in `rocket-network`.

pub mod identity_store;
pub mod repos;
pub mod tls_identity;

pub use identity_store::KeychainDeviceIdentity;
pub use repos::{
    FsBlockedDeviceRepo, FsManualAddressRepo, FsNetworkSettingsRepo, FsPairedDeviceRepo,
};
pub use tls_identity::{build_tls_identity, spki_from_cert, TlsIdentity};
```

In `crates/rocket-infra/src/lib.rs`, replace the line

```rust
pub use network::{build_tls_identity, spki_from_cert, KeychainDeviceIdentity, TlsIdentity};
```

with

```rust
pub use network::{
    build_tls_identity, spki_from_cert, FsBlockedDeviceRepo, FsManualAddressRepo,
    FsNetworkSettingsRepo, FsPairedDeviceRepo, KeychainDeviceIdentity, TlsIdentity,
};
```

Run: `cargo check -j4 -p rocket-infra`
Expected: FAIL with `unresolved import self::repos::FsBlockedDeviceRepo` (and the other three). This confirms the re-exports are wired before the code exists.

- [ ] **Step 3: Write the failing tests**

Append to `crates/rocket-infra/src/network/repos.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::net::SocketAddr;
    use tempfile::TempDir;

    fn id(n: u16) -> DeviceId {
        let mut bytes = [0u8; 32];
        bytes[..2].copy_from_slice(&n.to_be_bytes());
        DeviceId::from_bytes(bytes)
    }

    fn paired(n: u16) -> PairedDevice {
        PairedDevice {
            device_id: id(n),
            name_at_pairing: format!("Device {n}"),
            paired_at: Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).single().expect("date"),
            last_addresses: vec!["192.168.1.20:47821".parse::<SocketAddr>().expect("addr")],
        }
    }

    fn blocked(n: u16) -> BlockedDevice {
        BlockedDevice {
            device_id: id(n),
            name: format!("Blocked {n}"),
            blocked_at: Utc.with_ymd_and_hms(2026, 11, 2, 9, 0, 0).single().expect("date"),
        }
    }

    fn address(host: &str, port: u16, hour: u32) -> ManualAddress {
        ManualAddress {
            host: host.to_string(),
            port,
            added_at: Utc.with_ymd_and_hms(2026, 11, 3, hour, 0, 0).single().expect("date"),
        }
    }

    fn corrupt_files(dir: &TempDir, name: &str) -> Vec<std::path::PathBuf> {
        let prefix = format!("{name}.corrupt-");
        let mut found: Vec<_> = fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(&prefix))
                    .unwrap_or(false)
            })
            .collect();
        found.sort();
        found
    }

    fn paired_repo() -> (TempDir, FsPairedDeviceRepo) {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsPairedDeviceRepo::new(dir.path().join("paired.yml"));
        (dir, repo)
    }

    // ---- paired ----

    #[test]
    fn a_missing_file_is_empty_and_creates_nothing() {
        let (dir, repo) = paired_repo();
        assert!(repo.list().expect("list").is_empty());
        assert_eq!(repo.get(&id(1)).expect("get"), None);
        assert!(!repo.remove(&id(1)).expect("remove"));
        assert!(!dir.path().join("paired.yml").exists());
        assert_eq!(repo.take_quarantined(), None);
    }

    #[test]
    fn a_blank_file_is_empty_and_not_quarantined() {
        let (dir, repo) = paired_repo();
        fs::write(dir.path().join("paired.yml"), "  \n\n").expect("write");
        assert!(repo.list().expect("list").is_empty());
        assert_eq!(repo.take_quarantined(), None);
        assert!(corrupt_files(&dir, "paired.yml").is_empty());
        assert!(dir.path().join("paired.yml").exists());
    }

    #[test]
    fn paired_devices_round_trip_and_survive_a_new_instance() {
        let (dir, repo) = paired_repo();
        repo.upsert(paired(1)).expect("upsert 1");
        repo.upsert(paired(2)).expect("upsert 2");
        let again = FsPairedDeviceRepo::new(dir.path().join("paired.yml"));
        let listed = again.list().expect("list");
        assert_eq!(listed, vec![paired(1), paired(2)]);
        assert_eq!(again.get(&id(2)).expect("get"), Some(paired(2)));
        assert!(again.remove(&id(1)).expect("remove"));
        assert!(!again.remove(&id(1)).expect("remove twice"));
        assert_eq!(again.list().expect("list"), vec![paired(2)]);
    }

    #[test]
    fn upsert_replaces_the_same_device_in_place() {
        let (_dir, repo) = paired_repo();
        repo.upsert(paired(1)).expect("upsert 1");
        repo.upsert(paired(2)).expect("upsert 2");
        let mut renamed = paired(1);
        renamed.name_at_pairing = "New name".to_string();
        repo.upsert(renamed.clone()).expect("replace 1");
        assert_eq!(repo.list().expect("list"), vec![renamed, paired(2)]);
    }

    #[test]
    fn a_paired_entry_without_last_addresses_still_loads() {
        let (dir, repo) = paired_repo();
        let text = format!(
            "devices:\n- device_id: {}\n  name_at_pairing: Ada\n  paired_at: 2026-11-01T10:00:00Z\n",
            id(7).to_base32()
        );
        fs::write(dir.path().join("paired.yml"), text).expect("write");
        let listed = repo.list().expect("list");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].last_addresses.is_empty());
        assert_eq!(listed[0].name_at_pairing, "Ada");
    }

    #[test]
    fn the_paired_list_stops_at_the_limit_but_allows_updates() {
        let (dir, repo) = paired_repo();
        let file = PairedFile {
            devices: (0..MAX_PAIRED_DEVICES as u16).map(paired).collect(),
        };
        let yaml = serde_yaml::to_string(&file).expect("yaml");
        atomic_write(&dir.path().join("paired.yml"), yaml.as_bytes()).expect("seed");

        let err = repo
            .upsert(paired(MAX_PAIRED_DEVICES as u16))
            .expect_err("the 201st device is refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(err.to_string().contains("limit of 200"));
        assert_eq!(repo.list().expect("list").len(), MAX_PAIRED_DEVICES);

        let mut renamed = paired(5);
        renamed.name_at_pairing = "Renamed".to_string();
        repo.upsert(renamed.clone()).expect("an existing device can change");
        assert_eq!(repo.get(&id(5)).expect("get"), Some(renamed));
        assert_eq!(repo.list().expect("list").len(), MAX_PAIRED_DEVICES);
    }

    #[test]
    fn a_damaged_file_is_set_aside_once_and_reported_once() {
        let cases: [(&str, &[u8]); 3] = [
            ("not yaml", b": : [ unbalanced"),
            ("wrong shape", b"devices: 5\n"),
            ("not utf8", &[0xff, 0xfe, 0x00, 0x80]),
        ];
        for (label, bytes) in cases {
            let (dir, repo) = paired_repo();
            let path = dir.path().join("paired.yml");
            fs::write(&path, bytes).expect("write damaged file");

            assert!(repo.list().expect("list starts empty").is_empty(), "{label}");
            assert!(!path.exists(), "{label}: the damaged file moved away");
            let aside = corrupt_files(&dir, "paired.yml");
            assert_eq!(aside.len(), 1, "{label}");
            assert_eq!(fs::read(&aside[0]).expect("read aside"), bytes, "{label}");
            assert_eq!(repo.take_quarantined().as_deref(), Some("paired.yml"), "{label}");
            assert_eq!(repo.take_quarantined(), None, "{label}: reported once");

            // Reading again does not quarantine again, and a write makes a clean file.
            assert!(repo.list().expect("list again").is_empty());
            assert_eq!(repo.take_quarantined(), None);
            repo.upsert(paired(1)).expect("upsert after quarantine");
            assert_eq!(repo.list().expect("list").len(), 1);
            assert_eq!(corrupt_files(&dir, "paired.yml").len(), 1);
        }
    }

    #[test]
    fn concurrent_upserts_all_land() {
        let (_dir, repo) = paired_repo();
        let repo = std::sync::Arc::new(repo);
        let handles: Vec<_> = (0..16u16)
            .map(|n| {
                let repo = repo.clone();
                std::thread::spawn(move || repo.upsert(paired(n)).expect("upsert"))
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread");
        }
        assert_eq!(repo.list().expect("list").len(), 16);
    }

    // ---- blocked ----

    #[test]
    fn blocked_devices_add_contain_remove_and_persist() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("blocked.yml");
        let repo = FsBlockedDeviceRepo::new(path.clone());
        assert!(!repo.contains(&id(1)).expect("contains"));
        repo.add(blocked(1)).expect("add");
        repo.add(blocked(1)).expect("add the same device again");
        repo.add(blocked(2)).expect("add 2");
        let again = FsBlockedDeviceRepo::new(path);
        assert_eq!(again.list().expect("list"), vec![blocked(1), blocked(2)]);
        assert!(again.contains(&id(2)).expect("contains"));
        assert!(again.remove(&id(2)).expect("remove"));
        assert!(!again.remove(&id(2)).expect("remove twice"));
        assert!(!again.contains(&id(2)).expect("contains after remove"));
    }

    #[test]
    fn a_damaged_blocked_file_is_quarantined_and_named() {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsBlockedDeviceRepo::new(dir.path().join("blocked.yml"));
        fs::write(dir.path().join("blocked.yml"), "devices: [1, 2").expect("write");
        assert!(repo.list().expect("list").is_empty());
        assert_eq!(repo.take_quarantined().as_deref(), Some("blocked.yml"));
        assert_eq!(corrupt_files(&dir, "blocked.yml").len(), 1);
    }

    // ---- manual addresses ----

    fn address_repo() -> (TempDir, FsManualAddressRepo) {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsManualAddressRepo::new(dir.path().join("addresses.yml"));
        (dir, repo)
    }

    #[test]
    fn manual_addresses_dedupe_on_lowercased_host_and_port() {
        let (_dir, repo) = address_repo();
        repo.add(address("Host.Example", 4000, 8)).expect("add");
        repo.add(address("host.example", 4000, 9)).expect("add the same address");
        repo.add(address("  HOST.EXAMPLE ", 4000, 10)).expect("add with spaces");
        let listed = repo.list().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].host, "host.example");
        assert_eq!(listed[0].added_at, address("x", 1, 8).added_at, "the first entry stays");
        repo.add(address("host.example", 4001, 11)).expect("another port");
        repo.add(address("other.example", 4000, 12)).expect("another host");
        assert_eq!(repo.list().expect("list").len(), 3);
    }

    #[test]
    fn manual_addresses_remove_ignores_case_and_reports_a_miss() {
        let (_dir, repo) = address_repo();
        repo.add(address("10.0.0.5", 47821, 8)).expect("add");
        repo.add(address("Laptop.Local", 47821, 9)).expect("add");
        assert!(repo.remove("LAPTOP.local", 47821).expect("remove"));
        assert!(!repo.remove("laptop.local", 47821).expect("remove twice"));
        assert!(!repo.remove("10.0.0.5", 1).expect("wrong port"));
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn an_empty_host_is_refused() {
        let (_dir, repo) = address_repo();
        let err = repo.add(address("   ", 47821, 8)).expect_err("blank host");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(repo.list().expect("list").is_empty());
    }

    #[test]
    fn manual_addresses_stop_at_the_limit() {
        let (dir, repo) = address_repo();
        let file = AddressFile {
            addresses: (0..MAX_MANUAL_ADDRESSES as u16)
                .map(|n| address(&format!("host{n}.example"), 47821, 8))
                .collect(),
        };
        let yaml = serde_yaml::to_string(&file).expect("yaml");
        atomic_write(&dir.path().join("addresses.yml"), yaml.as_bytes()).expect("seed");

        let err = repo
            .add(address("one-more.example", 47821, 9))
            .expect_err("the 51st address is refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        repo.add(address("HOST3.example", 47821, 9))
            .expect("a duplicate is not a new entry");
        assert_eq!(repo.list().expect("list").len(), MAX_MANUAL_ADDRESSES);
    }

    #[test]
    fn a_damaged_address_file_is_quarantined() {
        let (dir, repo) = address_repo();
        fs::write(dir.path().join("addresses.yml"), "addresses: nope").expect("write");
        assert!(repo.list().expect("list").is_empty());
        assert_eq!(repo.take_quarantined().as_deref(), Some("addresses.yml"));
        repo.add(address("a.example", 1, 8)).expect("add after quarantine");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    // ---- settings ----

    #[test]
    fn settings_default_when_missing_and_round_trip() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("settings.yml");
        let repo = FsNetworkSettingsRepo::new(path.clone());
        assert_eq!(repo.load().expect("load"), NetworkSettings::default());
        let settings = NetworkSettings {
            hidden: true,
            display_name: Some("Ada".to_string()),
            intro_seen: true,
        };
        repo.save(&settings).expect("save");
        let again = FsNetworkSettingsRepo::new(path);
        assert_eq!(again.load().expect("load"), settings);
    }

    #[test]
    fn settings_fill_missing_fields_with_defaults() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("settings.yml"), "hidden: true\n").expect("write");
        let repo = FsNetworkSettingsRepo::new(dir.path().join("settings.yml"));
        let loaded = repo.load().expect("load");
        assert!(loaded.hidden);
        assert_eq!(loaded.display_name, None);
        assert!(!loaded.intro_seen);
    }

    #[test]
    fn a_damaged_settings_file_gives_defaults_and_is_quarantined() {
        let dir = TempDir::new().expect("tempdir");
        let repo = FsNetworkSettingsRepo::new(dir.path().join("settings.yml"));
        fs::write(dir.path().join("settings.yml"), "hidden: [maybe").expect("write");
        assert_eq!(repo.load().expect("load"), NetworkSettings::default());
        assert_eq!(repo.take_quarantined().as_deref(), Some("settings.yml"));
        let settings = NetworkSettings {
            hidden: true,
            ..NetworkSettings::default()
        };
        repo.save(&settings).expect("save after quarantine");
        assert_eq!(repo.load().expect("load"), settings);
        assert_eq!(corrupt_files(&dir, "settings.yml").len(), 1);
    }

    #[test]
    fn every_repo_is_a_quarantine_report() {
        let dir = TempDir::new().expect("tempdir");
        let a = FsPairedDeviceRepo::new(dir.path().join("paired.yml"));
        let b = FsBlockedDeviceRepo::new(dir.path().join("blocked.yml"));
        let c = FsManualAddressRepo::new(dir.path().join("addresses.yml"));
        let d = FsNetworkSettingsRepo::new(dir.path().join("settings.yml"));
        let reports: Vec<&dyn QuarantineReport> = vec![&a, &b, &c, &d];
        assert!(reports.iter().all(|r| r.take_quarantined().is_none()));
    }
}
```

- [ ] **Step 4: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-infra network::repos`
Expected: FAIL to compile with `cannot find type FsPairedDeviceRepo in this scope` (and `PairedFile`, `AddressFile`, `YamlFile`).

- [ ] **Step 5: Implement the repos**

Replace the single doc line at the top of `crates/rocket-infra/src/network/repos.rs` with the code below (the test module from Step 3 stays at the bottom):

```rust
//! The four small YAML lists of the team network, kept under `~/.rocket-api/network/`.
//!
//! Machine-local files: never inside a workspace or collection and never committed. A missing
//! or blank file is an empty list. A damaged file is renamed to `<name>.corrupt-<unix-secs>`,
//! the list starts empty and the file name is remembered once for `take_quarantined()`.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rocket_network::device::DeviceId;
use rocket_network::limits::{MAX_MANUAL_ADDRESSES, MAX_PAIRED_DEVICES};
use rocket_network::ports::{
    BlockedDeviceRepository, ManualAddressRepository, NetworkSettingsRepository,
    PairedDeviceRepository, QuarantineReport,
};
use rocket_network::records::{BlockedDevice, ManualAddress, NetworkSettings, PairedDevice};
use rocket_shared::error::{DomainError, DomainResult};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::atomic_write;

/// One YAML file with a lock, atomic writes and quarantine of damaged content.
///
/// Reads take the lock too, so two readers can never both try to set a damaged file aside.
struct YamlFile {
    path: PathBuf,
    file_name: String,
    write_lock: Mutex<()>,
    quarantined: Mutex<Option<String>>,
}

impl YamlFile {
    fn new(path: PathBuf) -> Self {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "network file".to_string());
        Self {
            path,
            file_name,
            write_lock: Mutex::new(()),
            quarantined: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.write_lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn read<T: DeserializeOwned + Default>(&self) -> DomainResult<T> {
        let _guard = self.lock();
        self.load()
    }

    /// Loads the value, lets `change` edit it and writes it back when `change` says it changed.
    fn modify<T, R>(&self, change: impl FnOnce(&mut T) -> DomainResult<(R, bool)>) -> DomainResult<R>
    where
        T: DeserializeOwned + Default + Serialize,
    {
        let _guard = self.lock();
        let mut value = self.load::<T>()?;
        let (out, changed) = change(&mut value)?;
        if changed {
            self.store(&value)?;
        }
        Ok(out)
    }

    fn take_quarantined(&self) -> Option<String> {
        self.quarantined
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    fn load<T: DeserializeOwned + Default>(&self) -> DomainResult<T> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
            // Bytes that are not UTF-8 count as a damaged file.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                self.quarantine()?;
                return Ok(T::default());
            }
            Err(e) => {
                return Err(DomainError::Io(format!(
                    "Failed to read {}: {e}",
                    self.file_name
                )))
            }
        };
        if text.trim().is_empty() {
            return Ok(T::default());
        }
        match serde_yaml::from_str::<T>(&text) {
            Ok(value) => Ok(value),
            Err(_) => {
                self.quarantine()?;
                Ok(T::default())
            }
        }
    }

    fn store<T: Serialize>(&self, value: &T) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(value).map_err(|e| {
            DomainError::Serialization(format!("Failed to serialize {}: {e}", self.file_name))
        })?;
        atomic_write(&self.path, yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write {}: {e}", self.file_name)))
    }

    /// Moves the damaged file to `<name>.corrupt-<unix-secs>` and remembers its name.
    fn quarantine(&self) -> DomainResult<()> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut target = self.path.as_os_str().to_os_string();
        target.push(format!(".corrupt-{secs}"));
        fs::rename(&self.path, PathBuf::from(target)).map_err(|e| {
            DomainError::Io(format!("Failed to set {} aside: {e}", self.file_name))
        })?;
        tracing::warn!(file = %self.file_name, "network file was damaged and has been set aside");
        *self.quarantined.lock().unwrap_or_else(|e| e.into_inner()) = Some(self.file_name.clone());
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PairedFile {
    #[serde(default)]
    devices: Vec<PairedDevice>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BlockedFile {
    #[serde(default)]
    devices: Vec<BlockedDevice>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AddressFile {
    #[serde(default)]
    addresses: Vec<ManualAddress>,
}

/// `paired.yml`: the devices this node has paired with.
pub struct FsPairedDeviceRepo {
    file: YamlFile,
}

impl FsPairedDeviceRepo {
    /// `path` is the YAML file itself, for example `<data dir>/network/paired.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            file: YamlFile::new(path),
        }
    }
}

impl PairedDeviceRepository for FsPairedDeviceRepo {
    fn list(&self) -> DomainResult<Vec<PairedDevice>> {
        Ok(self.file.read::<PairedFile>()?.devices)
    }

    fn get(&self, id: &DeviceId) -> DomainResult<Option<PairedDevice>> {
        Ok(self
            .file
            .read::<PairedFile>()?
            .devices
            .into_iter()
            .find(|d| d.device_id == *id))
    }

    fn upsert(&self, device: PairedDevice) -> DomainResult<()> {
        self.file.modify(|file: &mut PairedFile| {
            let existing = file
                .devices
                .iter()
                .position(|d| d.device_id == device.device_id);
            match existing {
                Some(index) => file.devices[index] = device,
                None => {
                    if file.devices.len() >= MAX_PAIRED_DEVICES {
                        return Err(DomainError::InvalidInput(format!(
                            "You've reached the limit of {MAX_PAIRED_DEVICES} paired devices. Remove one to pair."
                        )));
                    }
                    file.devices.push(device);
                }
            }
            Ok(((), true))
        })
    }

    fn remove(&self, id: &DeviceId) -> DomainResult<bool> {
        self.file.modify(|file: &mut PairedFile| {
            let before = file.devices.len();
            file.devices.retain(|d| d.device_id != *id);
            let removed = file.devices.len() != before;
            Ok((removed, removed))
        })
    }
}

impl QuarantineReport for FsPairedDeviceRepo {
    fn take_quarantined(&self) -> Option<String> {
        self.file.take_quarantined()
    }
}

/// `blocked.yml`: devices that are refused until the user unblocks them.
pub struct FsBlockedDeviceRepo {
    file: YamlFile,
}

impl FsBlockedDeviceRepo {
    /// `path` is the YAML file itself, for example `<data dir>/network/blocked.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            file: YamlFile::new(path),
        }
    }
}

impl BlockedDeviceRepository for FsBlockedDeviceRepo {
    fn list(&self) -> DomainResult<Vec<BlockedDevice>> {
        Ok(self.file.read::<BlockedFile>()?.devices)
    }

    fn contains(&self, id: &DeviceId) -> DomainResult<bool> {
        Ok(self
            .file
            .read::<BlockedFile>()?
            .devices
            .iter()
            .any(|d| d.device_id == *id))
    }

    fn add(&self, device: BlockedDevice) -> DomainResult<()> {
        self.file.modify(|file: &mut BlockedFile| {
            let existing = file
                .devices
                .iter()
                .position(|d| d.device_id == device.device_id);
            match existing {
                Some(index) => file.devices[index] = device,
                None => file.devices.push(device),
            }
            Ok(((), true))
        })
    }

    fn remove(&self, id: &DeviceId) -> DomainResult<bool> {
        self.file.modify(|file: &mut BlockedFile| {
            let before = file.devices.len();
            file.devices.retain(|d| d.device_id != *id);
            let removed = file.devices.len() != before;
            Ok((removed, removed))
        })
    }
}

impl QuarantineReport for FsBlockedDeviceRepo {
    fn take_quarantined(&self) -> Option<String> {
        self.file.take_quarantined()
    }
}

/// Hosts compare without case and without surrounding spaces.
fn normalize_host(host: &str) -> String {
    host.trim().to_lowercase()
}

/// `addresses.yml`: addresses the user added by hand ("Add by address").
pub struct FsManualAddressRepo {
    file: YamlFile,
}

impl FsManualAddressRepo {
    /// `path` is the YAML file itself, for example `<data dir>/network/addresses.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            file: YamlFile::new(path),
        }
    }
}

impl ManualAddressRepository for FsManualAddressRepo {
    fn list(&self) -> DomainResult<Vec<ManualAddress>> {
        Ok(self.file.read::<AddressFile>()?.addresses)
    }

    fn add(&self, address: ManualAddress) -> DomainResult<()> {
        let host = normalize_host(&address.host);
        if host.is_empty() {
            return Err(DomainError::InvalidInput(
                "The address needs a host".to_string(),
            ));
        }
        self.file.modify(|file: &mut AddressFile| {
            let known = file
                .addresses
                .iter()
                .any(|a| normalize_host(&a.host) == host && a.port == address.port);
            if known {
                return Ok(((), false));
            }
            if file.addresses.len() >= MAX_MANUAL_ADDRESSES {
                return Err(DomainError::InvalidInput(format!(
                    "You've reached the limit of {MAX_MANUAL_ADDRESSES} addresses. Remove one to add another."
                )));
            }
            file.addresses.push(ManualAddress { host, ..address });
            Ok(((), true))
        })
    }

    fn remove(&self, host: &str, port: u16) -> DomainResult<bool> {
        let host = normalize_host(host);
        self.file.modify(|file: &mut AddressFile| {
            let before = file.addresses.len();
            file.addresses
                .retain(|a| !(normalize_host(&a.host) == host && a.port == port));
            let removed = file.addresses.len() != before;
            Ok((removed, removed))
        })
    }
}

impl QuarantineReport for FsManualAddressRepo {
    fn take_quarantined(&self) -> Option<String> {
        self.file.take_quarantined()
    }
}

/// `settings.yml`: visibility, display name and the intro flag.
pub struct FsNetworkSettingsRepo {
    file: YamlFile,
}

impl FsNetworkSettingsRepo {
    /// `path` is the YAML file itself, for example `<data dir>/network/settings.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            file: YamlFile::new(path),
        }
    }
}

impl NetworkSettingsRepository for FsNetworkSettingsRepo {
    fn load(&self) -> DomainResult<NetworkSettings> {
        self.file.read::<NetworkSettings>()
    }

    fn save(&self, settings: &NetworkSettings) -> DomainResult<()> {
        self.file.modify(|current: &mut NetworkSettings| {
            *current = settings.clone();
            Ok(((), true))
        })
    }
}

impl QuarantineReport for FsNetworkSettingsRepo {
    fn take_quarantined(&self) -> Option<String> {
        self.file.take_quarantined()
    }
}
```

- [ ] **Step 6: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-infra network::repos`
Expected: PASS (all tests in `network::repos::tests`).

- [ ] **Step 7: Check**

Run: `cargo check -j4 -p rocket-infra`
Expected: PASS with no warnings from `repos.rs`.

No commit. The whole plan is reviewed at the plan end.

---

### Task 3: rustls configs with the custom certificate checks

**Files:**
- Create: `crates/rocket-infra/src/network/tls_config.rs`
- Modify: `crates/rocket-infra/Cargo.toml`, `crates/rocket-infra/src/network/mod.rs`
- Test: inline `#[cfg(test)] mod tests` in `tls_config.rs`.

**Interfaces:**
- Consumes: `TlsIdentity`, `spki_from_cert` (Task 1); `rocket_network::device::DeviceId`; `rocket_network::identity::PeerIdentity`; rustls 0.23.37 traits `rustls::server::danger::ClientCertVerifier` and `rustls::client::danger::ServerCertVerifier`; `rustls::crypto::{verify_tls13_signature, WebPkiSupportedAlgorithms}`.
- Produces (used by Plan 03 server, connector and tests):

```rust
pub fn server_config(identity: &TlsIdentity) -> DomainResult<Arc<rustls::ServerConfig>>;
pub fn client_config(identity: &TlsIdentity, expected: Option<DeviceId>) -> DomainResult<Arc<rustls::ClientConfig>>;
pub fn peer_identity_from_certs(certs: Option<&[CertificateDer<'_>]>) -> DomainResult<PeerIdentity>;
// crate-private, used by this module's tests:
pub(crate) fn server_config_with_resolver(resolver: Arc<dyn ResolvesServerCert>) -> DomainResult<Arc<ServerConfig>>;
pub(crate) fn client_config_with_resolver(expected: Option<DeviceId>, resolver: Arc<dyn ResolvesClientCert>) -> DomainResult<Arc<ClientConfig>>;
```

Behaviour contract (rustls 0.23.37, checked in `~/.cargo/registry/src/*/rustls-0.23.37/src`):
- Provider: `rustls::crypto::ring::default_provider()` in an `Arc`, used with `ServerConfig::builder_with_provider` and `ClientConfig::builder_with_provider`, then `.with_protocol_versions(&[&rustls::version::TLS13])`. TLS 1.2 is never offered or accepted.
- Server: `ClientCertVerifier` with `offer_client_auth() == true`, `client_auth_mandatory() == true`, `root_hint_subjects()` empty, `verify_client_cert` accepting exactly one syntactically valid certificate with no intermediates, and `verify_tls13_signature` calling `rustls::crypto::verify_tls13_signature(message, cert, dss, &algorithms)` only for the `ED25519` scheme. `supported_verify_schemes()` is `[ED25519]`, so a peer with another key type cannot even offer a certificate.
- Client: `ServerCertVerifier` with the same signature rules; `verify_server_cert` accepts any valid certificate, but when `expected` is `Some(id)` it rejects (`CertificateError::ApplicationVerificationFailure`) unless `DeviceId::from_spki(spki) == id`. The client presents our certificate through `with_client_auth_cert` semantics (`CertifiedKey::from_der` + `SingleCertAndKey`).
- Both configs turn session resumption off, so every connection runs the full certificate check.

- [ ] **Step 1: Promote tokio-rustls and add the skeleton**

In `crates/rocket-infra/Cargo.toml`, under `[dependencies]`, after the `rustls-webpki = ...` line add:

```toml
# Wraps TCP streams for the peer server and connector (Plan 03). The dev-dependency below also
# switches on `tls12`, which the tests use to build a TLS 1.2-only client.
tokio-rustls = { version = "0.26", default-features = false, features = ["ring"] }
```

Leave the existing `[dev-dependencies]` entry `tokio-rustls = { version = "0.26", default-features = false, features = ["ring", "tls12"] }` as it is.

Create `crates/rocket-infra/src/network/tls_config.rs` with this single line:

```rust
//! The rustls configs of the peer channel: TLS 1.3, mutual certificates, no certificate authority.
```

In `crates/rocket-infra/src/network/mod.rs`, replace the whole content with:

```rust
//! Team network pieces that touch the machine: the device key, the local lists and the TLS
//! configs. The domain rules live in `rocket-network`.

pub mod identity_store;
pub mod repos;
pub mod tls_config;
pub mod tls_identity;

pub use identity_store::KeychainDeviceIdentity;
pub use repos::{
    FsBlockedDeviceRepo, FsManualAddressRepo, FsNetworkSettingsRepo, FsPairedDeviceRepo,
};
pub use tls_config::{client_config, peer_identity_from_certs, server_config};
pub use tls_identity::{build_tls_identity, spki_from_cert, TlsIdentity};
```

Run: `cargo check -j4 -p rocket-infra`
Expected: FAIL with `unresolved imports self::tls_config::client_config, ...`.

- [ ] **Step 2: Write the failing tests**

Append to `crates/rocket-infra/src/network/tls_config.rs`:

```rust
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
        Arc::new(CertifiedKey::new(vec![cert_of.cert_der.clone()], signing_key))
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
        assert!(outcome.server.is_err(), "the server must require a certificate");
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
            Arc::new(SingleCertAndKey::from(stolen_certificate(&victim, &attacker))),
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
            .verify_client_cert(&a.cert_der, &[a.cert_der.clone()], now)
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
```

- [ ] **Step 3: Run the tests and see them fail**

Run: `cargo test -j4 -p rocket-infra network::tls_config`
Expected: FAIL to compile with `cannot find function server_config in this scope` and `cannot find struct PeerServerVerifier` / `PeerClientVerifier`.

- [ ] **Step 4: Implement the configs**

Replace the single doc line at the top of `crates/rocket-infra/src/network/tls_config.rs` with the code below (the test module from Step 2 stays at the bottom):

```rust
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
    CertificateError, ClientConfig, DigitallySignedStruct, DistinguishedName, Error,
    ServerConfig, SignatureScheme,
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
```

- [ ] **Step 5: Run the tests and see them pass**

Run: `cargo test -j4 -p rocket-infra network::tls_config`
Expected: PASS (all tests in `network::tls_config::tests`).

Notes for the implementer if a test misbehaves:
- If `a_tls12_only_client_is_refused` does not compile because `rustls::version::TLS12` is missing, the `tls12` feature is not on for the test build. It comes from the dev-dependency `tokio-rustls` (`["ring", "tls12"]`), which must stay in `[dev-dependencies]`.
- `a_wrong_expected_id_fails_the_client_handshake` and the two stolen-certificate tests are the security tests. Do not weaken them to make a run green. If `a_*_with_a_stolen_certificate_*` passes the handshake, the signature check is being skipped, which is a real bug in `verify_device_signature`.
- `CertifiedKey::from_der` checks that the key matches the certificate on the ring provider. A mismatch returns `Err` from `server_config`/`client_config`, which is the wanted production behaviour.

- [ ] **Step 6: Check**

Run: `cargo check -j4 -p rocket-infra`
Expected: PASS with no warnings from `tls_config.rs`.

Run: `cargo clippy -j4 -p rocket-infra --all-targets -- -D warnings`
Expected: no new findings from `crates/rocket-infra/src/network/` or `secret_store.rs`. Findings that already exist in other files are not part of this plan.

No commit. The whole plan is reviewed at the plan end.

---

End of Plan 2. Verify:

```bash
cargo check -j4 -p rocket-infra
cargo test -j4 -p rocket-infra network::
cargo test -j4 -p rocket-infra secret_store::tests
cargo check -j4 --workspace
```

Expected: all green. Stop here and show the user the whole plan's change (`Cargo.toml`, `Cargo.lock`, `lib.rs`, `secret_store.rs`, the new `network/` directory). After the user's OK, commit through the `dev-workflow-skills:1-git-commit` skill, staging explicit paths only, split for example as: `feat(network): add device identity and certificate` (Task 1 files and Cargo files), `feat(network): add network list repos` (`repos.rs`), `feat(network): add mutual TLS configs` (`tls_config.rs`).

Next plan to run: `docs/superpowers/plans/team-network/2026-11-06-team-network-plan-03-transport-discovery.md`.

## Self-Review

**Spec mapping.**
- Spec 5.2 `KeychainDeviceIdentity` with service `com.rocketapi.device-identity` and `rcgen` as a normal dependency: Task 1 (`new_device_identity`, `identity_store.rs`, Cargo move).
- Spec 5.2 `FsPairedDeviceRepo`, `FsBlockedDeviceRepo`, `FsManualAddressRepo` under `~/.rocket-api/network/`, machine-local: Task 2. The fourth repo (`FsNetworkSettingsRepo`, `settings.yml`) is index deviation 3. The caller (Plan 04) picks the directory; the repos take a file path.
- Spec 6.1 (TLS 1.3, both sides present a certificate, also on open routes, keys learned from the handshake): Task 3 `server_config` requires a client certificate for every connection, `peer_identity_from_certs` reads the key from the finished handshake.
- Spec 8.2 (TLS 1.3 only, Ed25519, no CA, custom verifier on both sides, key never leaves the keychain except into memory, fail closed, no key-file fallback): Tasks 1 and 3. The paired-and-not-blocked decision for paired routes is not made in the verifier. It is made by the Plan 03 routes from the identity this plan returns. This matches the index (`PeerHandler::is_paired`).
- Spec 9 rows "Keychain unavailable" (Task 1: `Err`, never a fallback), "Damaged `paired.yml`, `addresses.yml`" and "Damaged `blocked.yml`" (Task 2: quarantine instead of `.bak`, index deviation 1; the `blocked.yml` warning text is built by Plan 04 from `take_quarantined()`), "Paired list full (200)" (Task 2: `InvalidInput` with the text "You've reached the limit of 200 paired devices. Remove one to pair.").
- Index deviation 7 (`DeviceId = SHA-256(SPKI DER)` with `rustls-webpki`): Task 1, tests tie the certificate SPKI to `rcgen`'s `public_key_der()`.

**Names later plans use (exactly as in the index).**
- `rocket_infra::KeyringSecretStore::new_device_identity()`, `rocket_infra::KeychainDeviceIdentity::new(Arc<dyn SecretStore>)`.
- `rocket_infra::{TlsIdentity, build_tls_identity, spki_from_cert}` and `rocket_infra::network::{server_config, client_config, peer_identity_from_certs}`.
- `rocket_infra::{FsPairedDeviceRepo, FsBlockedDeviceRepo, FsManualAddressRepo, FsNetworkSettingsRepo}`, each `new(path: PathBuf)`, each also a `QuarantineReport`.
- Not in the index, new here, crate-private: `server_config_with_resolver`, `client_config_with_resolver`, `YamlFile`, the verifier structs, `tls_identity::test_support`. Plan 03 tests may use `crate::network::tls_identity::test_support::{identity, key_material}` (compiled only in test builds).

**Not covered.**
- No socket, HTTP, WebSocket or mDNS code (Plan 03). No ALPN is set on the configs; Plan 03 sets `alpn_protocols` on its own copy if it needs `http/1.1`.
- No real-keychain test. A manual check on each OS is needed once (first launch creates the key, second launch reuses it, a locked keychain turns networking off). Plan 05 lists it in `docs/manual-tests/team-network.md`.
- No 0600 file-mode handling for the YAML files; the repo has none anywhere, and the files hold no secrets (only public device ids and names).
- Quarantine names use whole seconds, so two damaged copies in the same second would overwrite each other's `.corrupt-<secs>` file. The same limit exists in `fs_trust_store.rs`.
- `DeviceKeyMaterial` zeroizes itself on drop (Plan 01). The key also exists briefly inside `rcgen::KeyPair` and rustls objects, which this plan cannot zero.
- Certificate validity dates are not checked (the default `rcgen` range is 1975 to 4096), by design: trust is the pinned key, not the date.
