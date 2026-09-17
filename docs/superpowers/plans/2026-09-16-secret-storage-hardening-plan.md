# Secret Storage Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop writing environment secret *values* to `.yml` files — route them through the OS keychain instead — and make the `secret` / `secret_type` flags survive a save → load round-trip.

**Architecture:** A new pure-domain `SecretStore` trait in `rocket-environment` (with a no-op `NullSecretStore`) gets a concrete `KeyringSecretStore` implementation in `rocket-infra`, backed by the `keyring` crate already vetted for git credentials. `OcEnvironment.variables` becomes a `Vec<OcEnvVariableEntry>` untagged sum type so the YAML layer can structurally represent a value-less `SecretVariable`. `FsEnvironmentRepo` becomes the single routing point: `save()` pushes secret values to the store *before* writing YAML (aborting the save if the store rejects them), and `get()`/`list()` hydrate them back so `EnvironmentService` and the whole IPC/frontend contract stay byte-identical.

**Tech Stack:** Rust 2021, Cargo workspace, `serde` / `serde_yaml` (untagged enums), `keyring` 3.6.3, `sha2`, `tracing`, `tempfile` for test fixtures.

**Spec:** [`docs/superpowers/specs/2026-09-16-secret-storage-hardening-spec.md`](../specs/2026-09-16-secret-storage-hardening-spec.md)

## Global Constraints

- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. This plan touches `.yml` files, `rocket-environment`, `rocket-infra`, `FsEnvironmentRepo`, environment data models, and Tauri IPC wiring — the OpenCollection injection rule in `CLAUDE.md` applies to every task below.
- Domain crates (`rocket-environment`) contain logic and traits only — **no I/O**. All concrete I/O lives in `rocket-infra`. (`.claude/rules/rust-ddd-boundaries.md`)
- Never `unwrap()` in production paths — map to `DomainResult` with explicit error handling instead. The existing suite uses `.unwrap()` inside `#[cfg(test)]` modules, but every new test in this plan uses `.expect("...")` so a failure names what broke; keep it that way, and never edit a pre-existing test just to change its style.
- Never shell out to the `git` CLI.
- `#[serde(rename_all = "camelCase")]` on IPC DTOs only — **never** on persistence structs. `OcVariable` / `OcSecretVariable` / `OcEnvVariableEntry` are persistence structs and must not gain it.
- Persistence structs keep backward compatibility: new fields are `#[serde(default, skip_serializing_if = ...)]`.
- `OcEnvVariableEntry` is `#[serde(untagged)]`. **More specific variants must come before less specific ones** (`crates/rocket-infra/CLAUDE.md`, "`OcItem` variant ordering"). `Secret` must be declared before `Plain` — reversing them silently breaks every secret file.
- `keyring` version and features are fixed at exactly `version = "3", features = ["apple-native", "windows-native", "sync-secret-service"]` (currently `src-tauri/Cargo.toml:52`, resolved to 3.6.3 in `Cargo.lock`). Do not change the version or feature set.
- Keychain service name for environment secrets is exactly `"com.rocketapi.env-secrets"` — it must stay distinct from the git-credential service name `"rocket-api"` (`src-tauri/src/commands/git.rs:185`).
- No IPC/DTO contract change is permitted. `EnvironmentService::get`/`list` must keep returning `Environment` values with the real value populated in `Variable.value`.
- Commits use conventional-commit format (`feat:`, `fix:`, `test:`, `refactor:`, `chore:`).
- Tests must **never** touch a real OS keychain by default. See the "Why an in-memory test double" note in Task 2.

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| `crates/rocket-environment/src/secret_store.rs` | Create | `SecretStore` trait + `NullSecretStore`. Pure domain, no I/O. |
| `crates/rocket-environment/src/lib.rs` | Modify | Declare and re-export the new module. |
| `Cargo.toml` (workspace root) | Modify | Promote `keyring` to `[workspace.dependencies]` so the version is declared once. |
| `src-tauri/Cargo.toml` | Modify | Switch `keyring` to `keyring.workspace = true`. |
| `crates/rocket-infra/Cargo.toml` | Modify | Add `keyring.workspace = true`. |
| `crates/rocket-infra/src/secret_store.rs` | Create | `KeyringSecretStore` — the only code that talks to the OS keychain for env secrets. |
| `crates/rocket-infra/src/lib.rs` | Modify | Declare and re-export `KeyringSecretStore`. |
| `crates/rocket-infra/src/oc/variables.rs` | Modify | Add the `OcEnvVariableEntry` untagged enum. |
| `crates/rocket-infra/src/oc/environment.rs` | Modify | `variables: Vec<OcVariable>` → `Vec<OcEnvVariableEntry>`. |
| `crates/rocket-infra/src/oc/mod.rs` | Modify | Add serde tests for the new enum in the existing `mod tests`. |
| `crates/rocket-infra/src/conversions/environment.rs` | Modify | Route secret vs. plain variables through the new enum in both directions. |
| `crates/rocket-infra/src/conversions/tests.rs` | Modify | Fix the one literal `OcEnvironment { variables: ... }` construction; add secret-conversion tests. |
| `crates/rocket-infra/src/fs_environment_repo.rs` | Modify | `with_secret_store` constructor, `scope_id`, secret routing in `save`/`get`/`list`/`delete`, `InMemorySecretStore` test double. |
| `src-tauri/src/lib.rs` | Modify | `env_secret_store()` helper + wire 2 construction sites. |
| `src-tauri/src/commands/environments.rs` | Modify | Wire the per-collection and global construction sites. |
| `src-tauri/src/commands/import.rs` | Modify | Wire the importer's construction site. |

**Deliberate deviations from the spec** (each justified where it appears):

1. §4.3 `scope_id` uses `DefaultHasher`. **Changed to SHA-256** (Task 5) — `DefaultHasher`'s output is explicitly documented as unstable across Rust releases, so a toolchain upgrade would orphan every stored secret. `sha2` is already a `rocket-infra` dependency.
2. §4.2 says "add `keyring` as a dependency of `rocket-infra`". **Promoted to `[workspace.dependencies]`** (Task 2) instead of duplicating the version string, matching the workspace `Cargo.toml` comment "declare versions once here".
3. §4.4 says `get()` sets the value to "empty string if `None`". **Changed to "leave the YAML-derived value untouched if `None`"** (Task 6) — identical behaviour for spec-format `SecretVariable` entries (whose conversion already yields `String::new()`), but it avoids blanking a legacy `Environment`-format file that carried `secret: true` alongside a plaintext value.
4. `delete()` keychain cleanup (Task 8) is **not in the spec**. Included because deleting an environment otherwise leaves its secrets in the keychain forever. Isolated into its own task so a reviewer can reject it without blocking anything else.
5. §6 asks for a changelog entry. `CHANGELOG.md` is generated by release-please from conventional commits, so the migration note travels in the Task 10 commit body instead of a hand-edited changelog section.
6. §5 expects roughly two `FsEnvironmentRepo` construction sites in `src-tauri`. There are **five** — all are wired in Task 9.

---

### Task 1: `SecretStore` trait and `NullSecretStore`

**Files:**
- Create: `crates/rocket-environment/src/secret_store.rs`
- Modify: `crates/rocket-environment/src/lib.rs`
- Test: `crates/rocket-environment/src/secret_store.rs` (inline `#[cfg(test)] mod tests`, matching the crate's existing style in `repository.rs`)

**Interfaces:**
- Consumes: `rocket_shared::error::DomainResult` (already a dependency).
- Produces:
  - `rocket_environment::secret_store::SecretStore` — `pub trait SecretStore: Send + Sync` with `fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>>`, `fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()>`, `fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()>`.
  - `rocket_environment::secret_store::NullSecretStore` — `pub struct NullSecretStore;` (unit struct, no fields).
  - Both re-exported at the crate root as `rocket_environment::{SecretStore, NullSecretStore}`.

- [ ] **Step 1: Write the failing test**

Create `crates/rocket-environment/src/secret_store.rs` containing *only* the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: std::sync::Arc<dyn SecretStore>) {}
    }

    #[test]
    fn null_store_never_returns_a_value() {
        let store = NullSecretStore;
        store.set("scope", "API_KEY", "sk-live-123").expect("null set");
        assert_eq!(store.get("scope", "API_KEY").expect("null get"), None);
    }

    #[test]
    fn null_store_delete_is_ok() {
        assert!(NullSecretStore.delete("scope", "API_KEY").is_ok());
    }
}
```

Register the module in `crates/rocket-environment/src/lib.rs` — add `pub mod secret_store;` to the `pub mod` block (alphabetically, after `pub mod resolver;`) and `pub use secret_store::{NullSecretStore, SecretStore};` after the `pub use resolver::...` line.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-environment secret_store`
Expected: FAIL — compile errors `cannot find trait 'SecretStore' in this scope` and `cannot find value 'NullSecretStore' in this scope`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/rocket-environment/src/secret_store.rs`, above the test module:

```rust
use rocket_shared::error::DomainResult;

/// Backend for the real value of a `secret: true` Variable. Never touches YAML.
///
/// `scope_id` uniquely identifies the environment file a secret belongs to, so
/// two environments that share a variable name never share a stored secret.
/// Implementations live in `rocket-infra` — this crate does no I/O.
pub trait SecretStore: Send + Sync {
    fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>>;
    fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()>;
    fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()>;
}

/// No-op store for tests and contexts with no keychain, such as headless CI.
/// Reads always miss, writes are discarded.
pub struct NullSecretStore;

impl SecretStore for NullSecretStore {
    fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
        Ok(None)
    }

    fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
        Ok(())
    }

    fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
        Ok(())
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rocket-environment secret_store`
Expected: PASS — 3 tests pass (`trait_is_object_safe`, `null_store_never_returns_a_value`, `null_store_delete_is_ok`).

- [ ] **Step 5: Run the whole crate to confirm nothing regressed**

Run: `cargo test -p rocket-environment`
Expected: PASS — all pre-existing tests still pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-environment/src/secret_store.rs crates/rocket-environment/src/lib.rs
git commit -m "feat(environment): add SecretStore trait and NullSecretStore"
```

---

### Task 2: `KeyringSecretStore` in `rocket-infra`

**Files:**
- Modify: `Cargo.toml` (workspace root, `[workspace.dependencies]` block)
- Modify: `src-tauri/Cargo.toml:52`
- Modify: `crates/rocket-infra/Cargo.toml` (`[dependencies]`)
- Create: `crates/rocket-infra/src/secret_store.rs`
- Modify: `crates/rocket-infra/src/lib.rs`
- Test: `crates/rocket-infra/src/secret_store.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `rocket_environment::secret_store::SecretStore` (Task 1).
- Produces:
  - `rocket_infra::secret_store::KeyringSecretStore` — `pub struct KeyringSecretStore;` (unit struct), re-exported at the crate root as `rocket_infra::KeyringSecretStore`.
  - Private `const KEYRING_SERVICE: &str = "com.rocketapi.env-secrets";` and private `fn account(scope_id: &str, key: &str) -> String` returning `"{scope_id}:{key}"`.

**Why an in-memory test double instead of a real keychain:** CI containers have no Secret Service daemon, no macOS Keychain and no Windows Credential Manager, so any test that calls `keyring::Entry` fails there for environmental reasons that say nothing about the code. This task therefore covers only the pure, keychain-free parts (`account`, the service-name namespace) with normal tests, and marks the two real-keychain round-trip tests `#[ignore]` for local-only runs. Everything that actually matters for correctness — the save/get routing in `FsEnvironmentRepo` — is covered against an in-memory `SecretStore` double in Tasks 5-8.

- [ ] **Step 1: Add the dependency**

In the workspace root `Cargo.toml`, inside `[workspace.dependencies]`, add this line immediately after `dashmap = "6"`:

```toml
keyring = { version = "3", features = ["apple-native", "windows-native", "sync-secret-service"] }
```

In `src-tauri/Cargo.toml`, replace line 52:

```toml
keyring = { version = "3", features = ["apple-native", "windows-native", "sync-secret-service"] }
```

with:

```toml
keyring.workspace = true
```

In `crates/rocket-infra/Cargo.toml`, add to `[dependencies]` immediately after `url = "2"`:

```toml
keyring.workspace = true
```

- [ ] **Step 2: Verify the dependency resolves to the same version**

Run: `cargo tree -p rocket-infra -i keyring`
Expected: shows `keyring v3.6.3` — unchanged from `Cargo.lock`. If the version moved, pin it back; the Global Constraints forbid a version change.

- [ ] **Step 3: Write the failing test**

Create `crates/rocket-infra/src/secret_store.rs` containing *only* the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_namespaces_scope_and_key() {
        assert_eq!(account("a1b2c3d4e5f60718:prod", "API_KEY"), "a1b2c3d4e5f60718:prod:API_KEY");
    }

    #[test]
    fn service_name_is_distinct_from_git_credentials() {
        // Git credentials use "rocket-api" (src-tauri/src/commands/git.rs:185).
        // Sharing a service name would let one feature clobber the other's entries.
        assert_ne!(KEYRING_SERVICE, "rocket-api");
        assert_eq!(KEYRING_SERVICE, "com.rocketapi.env-secrets");
    }

    // Real-keychain coverage, ignored by default: CI has no Secret Service or
    // Keychain daemon, so a failure here is an environment problem rather than
    // a code problem. Run locally with:
    //   cargo test -p rocket-infra keyring_ -- --ignored
    #[test]
    #[ignore = "requires a real OS keychain"]
    fn keyring_set_get_delete_roundtrip() {
        let store = KeyringSecretStore;
        let scope = "rocket-infra-test-scope";
        store.set(scope, "TEST_KEY", "sk-live-123").expect("set");
        assert_eq!(store.get(scope, "TEST_KEY").expect("get"), Some("sk-live-123".to_string()));
        store.delete(scope, "TEST_KEY").expect("delete");
        assert_eq!(store.get(scope, "TEST_KEY").expect("get after delete"), None);
    }

    #[test]
    #[ignore = "requires a real OS keychain"]
    fn keyring_delete_of_missing_entry_is_ok() {
        assert!(KeyringSecretStore.delete("rocket-infra-test-scope", "NO_SUCH_KEY").is_ok());
    }
}
```

Register the module in `crates/rocket-infra/src/lib.rs`: add `pub mod secret_store;` after `pub mod reqwest_executor;`, and `pub use secret_store::KeyringSecretStore;` after `pub use reqwest_executor::ReqwestExecutor;`.

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test -p rocket-infra secret_store`
Expected: FAIL — compile errors `cannot find function 'account' in this scope`, `cannot find value 'KEYRING_SERVICE' in this scope`, `cannot find value 'KeyringSecretStore' in this scope`.

- [ ] **Step 5: Write minimal implementation**

Prepend to `crates/rocket-infra/src/secret_store.rs`, above the test module:

```rust
//! OS-keychain backed storage for environment secret values.
//! This is the only place environment secrets touch the keychain.

use rocket_environment::secret_store::SecretStore;
use rocket_shared::error::{DomainError, DomainResult};

/// Keychain service namespace for environment secrets. Deliberately distinct
/// from the git-credential service name ("rocket-api").
const KEYRING_SERVICE: &str = "com.rocketapi.env-secrets";

/// Stores secret values in the OS-native secret store: macOS Keychain,
/// Windows Credential Manager, or the Linux Secret Service.
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            // A locked or unavailable keychain must not hard-fail an environment
            // load, which would brick app startup. Treat it as a miss.
            Err(e) => {
                tracing::warn!(error = %e, "keychain unavailable, environment secret unreadable");
                Ok(None)
            }
        }
    }

    fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        entry
            .set_password(value)
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(DomainError::Internal(e.to_string())),
        }
    }
}

/// Keychain account name for one secret. `scope_id` already encodes the
/// environment file, so this only has to append the variable key.
fn account(scope_id: &str, key: &str) -> String {
    format!("{scope_id}:{key}")
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p rocket-infra secret_store`
Expected: PASS — 2 tests pass, 2 ignored.

- [ ] **Step 7: Confirm the workspace still builds**

Run: `cargo check -p rocket-infra -p rocket`
Expected: no errors (the `keyring.workspace = true` switch in `src-tauri` must not have broken `commands/git.rs`).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src-tauri/Cargo.toml crates/rocket-infra/Cargo.toml crates/rocket-infra/src/secret_store.rs crates/rocket-infra/src/lib.rs
git commit -m "feat(infra): add KeyringSecretStore backed by the OS keychain"
```

---

### Task 3: `OcEnvVariableEntry` untagged enum

**Files:**
- Modify: `crates/rocket-infra/src/oc/variables.rs` (append after `OcSecretVariable`, currently lines 26-35)
- Test: `crates/rocket-infra/src/oc/mod.rs` (existing `#[cfg(test)] mod tests`, starting at line 45)

**Interfaces:**
- Consumes: `OcVariable` and `OcSecretVariable` from `crates/rocket-infra/src/oc/variables.rs` (both unchanged).
- Produces: `crate::oc::OcEnvVariableEntry` — `#[serde(untagged)] pub enum OcEnvVariableEntry { Secret(OcSecretVariable), Plain(OcVariable) }`, re-exported through `pub use variables::*` in `crates/rocket-infra/src/oc/mod.rs`. Variant order is load-bearing: `Secret` first.

This task only *adds* the enum. The `OcEnvironment.variables` field type flips in Task 4, so the crate keeps compiling at the end of this task.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `crates/rocket-infra/src/oc/mod.rs`, immediately after the existing `oc_environment_yaml` test (which ends around line 734):

```rust
    #[test]
    fn env_variable_entry_prefers_secret_variant() {
        let yaml = "secret: true\nname: API_KEY\ntype: string\n";
        let entry: OcEnvVariableEntry = serde_yaml::from_str(yaml).expect("parse secret entry");
        match entry {
            OcEnvVariableEntry::Secret(s) => {
                assert!(s.secret);
                assert_eq!(s.name, "API_KEY");
                assert_eq!(s.secret_type, Some("string".into()));
            }
            OcEnvVariableEntry::Plain(_) => {
                panic!("an entry with `secret: true` must not deserialize as Plain")
            }
        }
    }

    #[test]
    fn env_variable_entry_falls_back_to_plain_variant() {
        let yaml = "name: HOST\nvalue: api.example.com\n";
        let entry: OcEnvVariableEntry = serde_yaml::from_str(yaml).expect("parse plain entry");
        match entry {
            OcEnvVariableEntry::Plain(v) => {
                assert_eq!(v.name, "HOST");
                assert_eq!(
                    v.value.as_ref().map(|x| x.data().to_string()),
                    Some("api.example.com".to_string())
                );
            }
            OcEnvVariableEntry::Secret(_) => {
                panic!("an entry without `secret` must not deserialize as Secret")
            }
        }
    }

    #[test]
    fn env_variable_entry_serializes_without_a_variant_wrapper() {
        let plain = OcEnvVariableEntry::Plain(OcVariable {
            name: "HOST".into(),
            value: Some(VariableValue::simple("api.example.com")),
            initial: None,
            description: None,
            disabled: None,
        });
        let yaml = serde_yaml::to_string(&plain).expect("serialize plain entry");
        assert!(yaml.contains("name: HOST"), "got:\n{yaml}");
        assert!(yaml.contains("value: api.example.com"), "got:\n{yaml}");
        assert!(!yaml.contains("Plain"), "untagged enum must not emit a variant key:\n{yaml}");

        let secret = OcEnvVariableEntry::Secret(OcSecretVariable {
            secret: true,
            name: "API_KEY".into(),
            description: None,
            disabled: None,
            secret_type: Some("string".into()),
        });
        let yaml = serde_yaml::to_string(&secret).expect("serialize secret entry");
        assert!(yaml.contains("secret: true"), "got:\n{yaml}");
        assert!(yaml.contains("name: API_KEY"), "got:\n{yaml}");
        assert!(!yaml.contains("value:"), "a secret entry must never carry a value:\n{yaml}");
        assert!(!yaml.contains("Secret"), "untagged enum must not emit a variant key:\n{yaml}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-infra env_variable_entry`
Expected: FAIL — compile error `cannot find type 'OcEnvVariableEntry' in this scope`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/rocket-infra/src/oc/variables.rs`:

```rust
/// One entry in an Environment's `variables` list. The OpenCollection spec
/// (section 4) allows either a regular Variable or a value-less SecretVariable.
///
/// `#[serde(untagged)]` tries variants top to bottom, so the more specific
/// `Secret` variant — the only one with a required `secret` field — must stay
/// first. Reversing the order makes every secret entry deserialize as `Plain`
/// with its `secret` field silently ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OcEnvVariableEntry {
    Secret(OcSecretVariable),
    Plain(OcVariable),
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rocket-infra env_variable_entry`
Expected: PASS — 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/oc/variables.rs crates/rocket-infra/src/oc/mod.rs
git commit -m "feat(infra): add OcEnvVariableEntry untagged enum for environment variables"
```

---

### Task 4: Route secrets through the `OcEnvironment` conversions

**Files:**
- Modify: `crates/rocket-infra/src/oc/environment.rs:7,19`
- Modify: `crates/rocket-infra/src/conversions/environment.rs:5-31` (whole file)
- Modify: `crates/rocket-infra/src/conversions/tests.rs:364` (the literal `variables: vec![OcVariable { ... }]` inside `environment_oc_to_domain`)
- Test: `crates/rocket-infra/src/conversions/tests.rs` (append new tests after `environment_roundtrip`, which ends around line 394)

**Interfaces:**
- Consumes: `OcEnvVariableEntry` (Task 3); the pre-existing `impl From<OcVariable> for Variable`, `impl From<Variable> for OcVariable`, and `impl From<OcSecretVariable> for Variable` in `crates/rocket-infra/src/conversions/variables.rs` (all unchanged).
- Produces: `OcEnvironment.variables: Vec<OcEnvVariableEntry>`; `impl From<Environment> for OcEnvironment` that emits `OcEnvVariableEntry::Secret` (no value) for `Variable { secret: true, .. }` and `OcEnvVariableEntry::Plain` otherwise; `impl From<OcEnvironment> for Environment` that maps both variants back.

- [ ] **Step 1: Write the failing test**

Append to `crates/rocket-infra/src/conversions/tests.rs`, after the `environment_roundtrip` test:

```rust
#[test]
fn environment_secret_variable_to_oc_drops_the_value() {
    let mut env = Environment::new("prod");
    let mut secret = Variable::secret("API_KEY", "sk-live-123");
    secret.secret_type = Some("string".into());
    env.set_variable(secret);

    let oc: OcEnvironment = env.into();
    assert_eq!(oc.variables.len(), 1);
    match &oc.variables[0] {
        OcEnvVariableEntry::Secret(s) => {
            assert!(s.secret);
            assert_eq!(s.name, "API_KEY");
            assert_eq!(s.secret_type, Some("string".into()));
            assert_eq!(s.disabled, None);
        }
        OcEnvVariableEntry::Plain(_) => {
            panic!("a secret Variable must convert to the Secret variant")
        }
    }

    let yaml = serde_yaml::to_string(&oc).expect("serialize environment");
    assert!(!yaml.contains("sk-live-123"), "secret value leaked into YAML:\n{yaml}");
}

#[test]
fn environment_disabled_secret_variable_keeps_disabled_flag() {
    let mut env = Environment::new("prod");
    let mut secret = Variable::secret("API_KEY", "sk-live-123");
    secret.enabled = false;
    env.set_variable(secret);

    let oc: OcEnvironment = env.into();
    match &oc.variables[0] {
        OcEnvVariableEntry::Secret(s) => assert_eq!(s.disabled, Some(true)),
        OcEnvVariableEntry::Plain(_) => panic!("expected the Secret variant"),
    }
}

#[test]
fn environment_oc_secret_entry_converts_back_with_secret_flag_set() {
    let oc = OcEnvironment {
        name: "prod".into(),
        color: None,
        description: None,
        variables: vec![OcEnvVariableEntry::Secret(OcSecretVariable {
            secret: true,
            name: "API_KEY".into(),
            description: None,
            disabled: None,
            secret_type: Some("string".into()),
        })],
        client_certificates: Vec::new(),
        extends: None,
        dot_env_file_path: None,
    };

    let env: Environment = oc.into();
    assert_eq!(env.variables.len(), 1);
    assert!(env.variables[0].secret);
    assert_eq!(env.variables[0].key, "API_KEY");
    assert_eq!(env.variables[0].value, "", "YAML must not be a source of secret values");
    assert_eq!(env.variables[0].secret_type, Some("string".into()));
    assert!(env.variables[0].enabled);
}

#[test]
fn environment_secret_flag_survives_a_yaml_roundtrip() {
    let mut env = Environment::new("prod");
    env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
    env.set_variable(Variable::new("HOST", "api.example.com"));

    let oc: OcEnvironment = env.into();
    let yaml = serde_yaml::to_string(&oc).expect("serialize environment");
    let parsed: OcEnvironment = serde_yaml::from_str(&yaml).expect("parse environment");
    let back: Environment = parsed.into();

    let api_key = back
        .variables
        .iter()
        .find(|v| v.key == "API_KEY")
        .expect("API_KEY entry");
    assert!(api_key.secret, "the secret flag must survive save -> load");
    let host = back
        .variables
        .iter()
        .find(|v| v.key == "HOST")
        .expect("HOST entry");
    assert!(!host.secret);
    assert_eq!(host.value, "api.example.com");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-infra environment_secret`
Expected: FAIL — compile errors: `expected 'OcVariable', found 'OcEnvVariableEntry'` (the `variables` field is still `Vec<OcVariable>`) and mismatched types in `environment_oc_secret_entry_converts_back_with_secret_flag_set`.

- [ ] **Step 3: Flip the field type**

In `crates/rocket-infra/src/oc/environment.rs`, change line 7 from:

```rust
use super::variables::OcVariable;
```

to:

```rust
use super::variables::OcEnvVariableEntry;
```

and change line 19 from:

```rust
    pub variables: Vec<OcVariable>,
```

to:

```rust
    pub variables: Vec<OcEnvVariableEntry>,
```

- [ ] **Step 4: Update both conversions**

Replace the whole body of `crates/rocket-infra/src/conversions/environment.rs` with:

```rust
use crate::oc::*;
use rocket_environment::environment::Environment;
use rocket_environment::variable::Variable;

impl From<OcEnvironment> for Environment {
    fn from(oc: OcEnvironment) -> Self {
        Environment {
            name: oc.name,
            variables: oc
                .variables
                .into_iter()
                .map(|entry| match entry {
                    OcEnvVariableEntry::Secret(s) => Variable::from(s),
                    OcEnvVariableEntry::Plain(v) => Variable::from(v),
                })
                .collect(),
            color: oc.color,
            description: oc.description,
            extends: oc.extends,
            dot_env_file_path: oc.dot_env_file_path,
            client_certificates: oc.client_certificates,
        }
    }
}

impl From<Environment> for OcEnvironment {
    fn from(env: Environment) -> Self {
        OcEnvironment {
            name: env.name,
            color: env.color,
            description: env.description,
            variables: env
                .variables
                .into_iter()
                .map(|v| {
                    if v.secret {
                        // `v.value` is deliberately dropped: a secret value belongs
                        // in the SecretStore and must never reach a YAML struct.
                        // FsEnvironmentRepo::save routes it there before calling this.
                        OcEnvVariableEntry::Secret(OcSecretVariable {
                            secret: true,
                            name: v.key,
                            description: v.description,
                            disabled: if v.enabled { None } else { Some(true) },
                            secret_type: v.secret_type,
                        })
                    } else {
                        OcEnvVariableEntry::Plain(OcVariable::from(v))
                    }
                })
                .collect(),
            client_certificates: env.client_certificates,
            extends: env.extends,
            dot_env_file_path: env.dot_env_file_path,
        }
    }
}
```

- [ ] **Step 5: Fix the one existing literal construction**

In `crates/rocket-infra/src/conversions/tests.rs`, inside `environment_oc_to_domain`, replace line 364:

```rust
            OcVariable { name: "HOST".into(), value: Some(VariableValue::simple("api.prod.com")), initial: None, description: None, disabled: None },
```

with:

```rust
            OcEnvVariableEntry::Plain(OcVariable { name: "HOST".into(), value: Some(VariableValue::simple("api.prod.com")), initial: None, description: None, disabled: None }),
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p rocket-infra environment`
Expected: PASS — the 4 new tests plus the pre-existing `environment_oc_to_domain`, `environment_roundtrip`, and `environment_client_certificates_survive_oc_roundtrip`.

- [ ] **Step 7: Run the whole crate**

Run: `cargo test -p rocket-infra`
Expected: PASS. In particular `save_writes_spec_field_names`, `save_then_load_roundtrip_via_oc_format`, `load_old_format_with_key_field_still_works` and `list_old_format_with_key_field_still_works` in `fs_environment_repo.rs` must still pass unmodified (acceptance criterion 4).

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-infra/src/oc/environment.rs crates/rocket-infra/src/conversions/environment.rs crates/rocket-infra/src/conversions/tests.rs
git commit -m "fix(infra): persist the secret flag by writing SecretVariable entries to environment YAML"
```

---

### Task 5: `FsEnvironmentRepo::with_secret_store` and secret-aware `save()`

**Files:**
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs:1-23` (imports, struct, constructors) and `:63-69` (`save`)
- Test: `crates/rocket-infra/src/fs_environment_repo.rs` (existing `#[cfg(test)] mod tests`, starting line 76)

**Interfaces:**
- Consumes: `rocket_environment::secret_store::{SecretStore, NullSecretStore}` (Task 1); `OcEnvVariableEntry` (Task 3); the secret-aware `From<Environment> for OcEnvironment` (Task 4).
- Produces:
  - `FsEnvironmentRepo::new(dir: PathBuf) -> Self` — unchanged signature, now delegates to `with_secret_store` with a `NullSecretStore`.
  - `FsEnvironmentRepo::with_secret_store(dir: PathBuf, secret_store: Arc<dyn SecretStore>) -> Self` — new public constructor.
  - Private `FsEnvironmentRepo::scope_id(dir: &Path, env_name: &str) -> String` — format `"{16 hex chars}:{env_name}"`.
  - Private `FsEnvironmentRepo::persisted_secret_keys(&self, name: &str) -> Vec<String>`.
  - Test-only `InMemorySecretStore` in the test module, plus the `setup_with_store()` fixture. Tasks 6, 7 and 8 all build on them.

**Why SHA-256 instead of the spec's `DefaultHasher` (section 4.3):** `std::collections::hash_map::DefaultHasher` is documented as *not* stable across Rust releases — its hashes "should not be relied upon over releases". Since `scope_id` is the lookup key for every stored secret, a toolchain bump would silently orphan every user's secrets. `sha2` is already a `rocket-infra` dependency (`crates/rocket-infra/Cargo.toml:22`), costs nothing extra, and is deterministic forever. The spec's collision argument is unaffected.

**Why `create_dir_all` before `scope_id`:** `scope_id` canonicalizes the directory, and `Path::canonicalize` fails on a directory that does not exist yet, falling back to the raw path. On a first save into a not-yet-created `environments/` directory that fallback would produce a *different* scope than the later read (where the directory exists and canonicalizes — on macOS, for example, resolving `/var` to `/private/var`), permanently losing the secret. Creating the directory first makes both paths canonicalize identically.

- [ ] **Step 1: Replace the test fixtures**

In `crates/rocket-infra/src/fs_environment_repo.rs`, replace the test-module preamble (lines 77-86, from `use super::*;` through the closing brace of `fn setup()`) with:

```rust
    use super::*;
    use rocket_environment::secret_store::SecretStore;
    use rocket_environment::Variable;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// In-memory SecretStore double. Tests must never touch a real OS keychain:
    /// CI has no Secret Service / Keychain daemon, so such a test would fail for
    /// environmental reasons unrelated to the code under test.
    #[derive(Default)]
    struct InMemorySecretStore {
        entries: Mutex<HashMap<String, String>>,
        fail_set: AtomicBool,
        fail_get: AtomicBool,
    }

    impl InMemorySecretStore {
        fn entry_key(scope_id: &str, key: &str) -> String {
            format!("{scope_id}:{key}")
        }

        fn len(&self) -> usize {
            self.entries.lock().expect("store lock").len()
        }

        fn contains_value(&self, value: &str) -> bool {
            self.entries.lock().expect("store lock").values().any(|v| v == value)
        }
    }

    impl SecretStore for InMemorySecretStore {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            if self.fail_get.load(Ordering::SeqCst) {
                return Err(DomainError::Internal("keychain locked".into()));
            }
            Ok(self
                .entries
                .lock()
                .expect("store lock")
                .get(&Self::entry_key(scope_id, key))
                .cloned())
        }

        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            if self.fail_set.load(Ordering::SeqCst) {
                return Err(DomainError::Internal("keychain locked".into()));
            }
            self.entries
                .lock()
                .expect("store lock")
                .insert(Self::entry_key(scope_id, key), value.to_string());
            Ok(())
        }

        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.entries
                .lock()
                .expect("store lock")
                .remove(&Self::entry_key(scope_id, key));
            Ok(())
        }
    }

    fn setup() -> (TempDir, FsEnvironmentRepo) {
        let dir = TempDir::new().expect("temp dir");
        let repo = FsEnvironmentRepo::new(dir.path().to_path_buf());
        (dir, repo)
    }

    fn setup_with_store() -> (TempDir, FsEnvironmentRepo, Arc<InMemorySecretStore>) {
        let dir = TempDir::new().expect("temp dir");
        let store = Arc::new(InMemorySecretStore::default());
        let repo = FsEnvironmentRepo::with_secret_store(dir.path().to_path_buf(), store.clone());
        (dir, repo, store)
    }
```

Leave every pre-existing test in the module untouched — acceptance criterion 4 depends on three of them passing unmodified.

- [ ] **Step 2: Write the failing tests**

Append at the end of the same test module:

```rust
    #[test]
    fn save_keeps_the_secret_value_out_of_the_yaml_file() {
        let (dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(!raw.contains("sk-live-123"), "secret value leaked to disk:\n{raw}");
        assert!(store.contains_value("sk-live-123"), "secret value never reached the store");
    }

    #[test]
    fn save_writes_a_spec_secret_variable_entry() {
        let (dir, repo, _store) = setup_with_store();
        let mut env = Environment::new("prod");
        let mut secret = Variable::secret("API_KEY", "sk-live-123");
        secret.secret_type = Some("string".into());
        env.set_variable(secret);
        repo.save(&env).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(raw.contains("secret: true"), "expected 'secret: true':\n{raw}");
        assert!(raw.contains("name: API_KEY"), "expected 'name: API_KEY':\n{raw}");
        assert!(raw.contains("type: string"), "expected the secret type hint:\n{raw}");
        assert!(!raw.contains("value:"), "a secret entry must carry no value field:\n{raw}");
    }

    #[test]
    fn save_aborts_when_the_secret_store_rejects_the_value() {
        let (dir, repo, store) = setup_with_store();
        store.fail_set.store(true, Ordering::SeqCst);

        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        let err = repo.save(&env).expect_err("save must fail when the store rejects the value");

        assert!(matches!(err, DomainError::Internal(_)), "got {err:?}");
        assert!(
            !dir.path().join("prod.yml").exists(),
            "YAML must not claim a secret is protected when the store rejected it"
        );
    }

    #[test]
    fn secret_survives_a_first_save_into_a_missing_directory() {
        let parent = TempDir::new().expect("temp dir");
        let env_dir = parent.path().join("environments");
        assert!(!env_dir.exists());
        let store = Arc::new(InMemorySecretStore::default());
        let repo = FsEnvironmentRepo::with_secret_store(env_dir, store.clone());

        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("save");

        assert_eq!(store.len(), 1);
        assert!(store.contains_value("sk-live-123"));
    }

    #[test]
    fn non_secret_variables_still_write_their_value_to_yaml() {
        let (dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(raw.contains("name: HOST"), "got:\n{raw}");
        assert!(raw.contains("value: api.example.com"), "got:\n{raw}");
        assert_eq!(store.len(), 0, "a non-secret variable must not touch the secret store");
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-infra fs_environment_repo`
Expected: FAIL — `no function or associated item named 'with_secret_store' found for struct 'FsEnvironmentRepo'`, plus `cannot find value 'Arc' in this scope`.

- [ ] **Step 4: Add the field, the constructors, `scope_id` and `persisted_secret_keys`**

In `crates/rocket-infra/src/fs_environment_repo.rs`, replace lines 1-23 with:

```rust
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rocket_environment::secret_store::{NullSecretStore, SecretStore};
use rocket_environment::{Environment, EnvironmentRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::oc::{OcEnvVariableEntry, OcEnvironment};
use crate::yaml_io::delete_if_exists;

pub struct FsEnvironmentRepo {
    dir: PathBuf,
    secret_store: Arc<dyn SecretStore>,
}

impl FsEnvironmentRepo {
    /// Environments with no secure backend — secret values are dropped on save
    /// and come back empty on load. Used by tests and by the Bruno importer.
    pub fn new(dir: PathBuf) -> Self {
        Self::with_secret_store(dir, Arc::new(NullSecretStore))
    }

    /// Environments backed by a real secret store. Production callers in
    /// `src-tauri` use this with `KeyringSecretStore`.
    pub fn with_secret_store(dir: PathBuf, secret_store: Arc<dyn SecretStore>) -> Self {
        Self { dir, secret_store }
    }

    fn file_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.yml", name))
    }

    /// Stable keychain namespace for one environment file.
    ///
    /// Derived from the canonical environments directory, so the workspace-level
    /// `<workspace>/environments/` and a collection's
    /// `<collection>/environments/` never share an entry even when both hold an
    /// environment called "prod".
    ///
    /// SHA-256 rather than `DefaultHasher`: `DefaultHasher`'s output is not
    /// stable across Rust releases, and this value is the lookup key for every
    /// stored secret — an unstable hash would orphan them on a toolchain bump.
    fn scope_id(dir: &Path, env_name: &str) -> String {
        use sha2::{Digest, Sha256};
        use std::fmt::Write;

        let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
        let prefix = digest[..8].iter().fold(String::with_capacity(16), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        });
        format!("{prefix}:{env_name}")
    }

    /// Keys stored as SecretVariable entries in the file as it exists on disk
    /// right now. Empty when the file is missing or unparseable.
    fn persisted_secret_keys(&self, name: &str) -> Vec<String> {
        let Ok(content) = fs::read_to_string(self.file_path(name)) else {
            return Vec::new();
        };
        let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) else {
            return Vec::new();
        };
        oc.variables
            .into_iter()
            .filter_map(|entry| match entry {
                OcEnvVariableEntry::Secret(s) => Some(s.name),
                OcEnvVariableEntry::Plain(_) => None,
            })
            .collect()
    }
}
```

Note: `unwrap_or_else` on `canonicalize` is a total fallback, not a panicking call — it is the documented behaviour for a directory that does not exist yet.

- [ ] **Step 5: Rewrite `save()`**

Replace the `save` method (originally lines 63-69) with:

```rust
    fn save(&self, env: &Environment) -> DomainResult<()> {
        // Create the directory up front so scope_id() canonicalizes the same
        // path on a first save as on every later read.
        fs::create_dir_all(&self.dir)?;
        let scope = Self::scope_id(&self.dir, &env.name);

        // Every secret value goes to the store before any YAML is written. A
        // store failure aborts the whole save: the file must never claim a
        // variable is secret when its value did not reach secure storage.
        for var in env.variables.iter().filter(|v| v.secret) {
            self.secret_store.set(&scope, &var.key, &var.value)?;
        }

        // The conversion drops secret values by construction — see
        // conversions/environment.rs.
        let oc: OcEnvironment = env.clone().into();
        let yaml = serde_yaml::to_string(&oc)
            .map_err(|e| DomainError::Internal(format!("Failed to serialize environment: {e}")))?;
        atomic_write(&self.file_path(&env.name), yaml.as_bytes())?;
        Ok(())
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-infra fs_environment_repo`
Expected: PASS — all pre-existing tests plus the 5 new ones.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/src/fs_environment_repo.rs
git commit -m "feat(infra): route environment secret values through a SecretStore on save"
```

---

### Task 6: Hydrate secret values in `get()` and `list()`

**Files:**
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs` — `list` (lines 26-48 before Task 5's edits), `get` (lines 50-61), plus a new `hydrate_secrets` helper in the inherent `impl` block
- Test: `crates/rocket-infra/src/fs_environment_repo.rs` (existing test module)

**Interfaces:**
- Consumes: `FsEnvironmentRepo::scope_id` and `self.secret_store` (Task 5); the `InMemorySecretStore` test double and `setup_with_store()` (Task 5).
- Produces: private `FsEnvironmentRepo::hydrate_secrets(&self, env: &mut Environment)`. `EnvironmentRepository::get`/`list` now return `Variable { secret: true, value: <real value> }`, which is what `EnvironmentService`, `RequestExecutionService` and the frontend already expect.

**Deviation from spec section 4.4:** the spec says a store miss sets the value to the empty string. This implementation *leaves the YAML-derived value alone* on a miss. For a spec-format `SecretVariable` entry the two are identical, because `impl From<OcSecretVariable> for Variable` already yields `value: String::new()`. The difference only shows up for a legacy `Environment`-format file that carried `secret: true` next to a plaintext value — there, blanking would destroy the user's data on a plain read.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `crates/rocket-infra/src/fs_environment_repo.rs`:

```rust
    #[test]
    fn secret_value_roundtrips_through_save_and_get() {
        let (_dir, repo, _store) = setup_with_store();
        let mut env = Environment::new("prod");
        let mut secret = Variable::secret("API_KEY", "sk-live-123");
        secret.secret_type = Some("string".into());
        env.set_variable(secret);
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");

        let loaded = repo.get("prod").expect("get");
        let api_key = loaded
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret, "the secret flag must survive a roundtrip");
        assert_eq!(api_key.value, "sk-live-123");
        assert_eq!(api_key.secret_type, Some("string".into()));
        assert_eq!(loaded.get_value("HOST"), Some("api.example.com"));
    }

    #[test]
    fn secret_value_roundtrips_through_save_and_list() {
        let (_dir, repo, _store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("save");

        let list = repo.list().expect("list");
        assert_eq!(list.len(), 1);
        let api_key = list[0]
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret);
        assert_eq!(api_key.value, "sk-live-123");
    }

    #[test]
    fn get_soft_fails_when_the_secret_store_is_unavailable() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");

        store.fail_get.store(true, Ordering::SeqCst);
        // A locked keychain must never fail an environment load.
        let loaded = repo.get("prod").expect("get must not fail on a store error");
        let api_key = loaded
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret);
        assert_eq!(api_key.value, "");
        assert_eq!(loaded.get_value("HOST"), Some("api.example.com"));
    }

    #[test]
    fn get_returns_an_empty_secret_when_the_store_has_no_entry() {
        let (dir, repo, _store) = setup_with_store();
        let yaml = "name: prod\nvariables:\n- secret: true\n  name: API_KEY\n";
        std::fs::write(dir.path().join("prod.yml"), yaml).expect("write prod.yml");

        let loaded = repo.get("prod").expect("get");
        let api_key = loaded
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret);
        assert_eq!(api_key.value, "");
    }

    #[test]
    fn legacy_plaintext_secret_value_is_not_blanked_on_load() {
        let (dir, repo, _store) = setup_with_store();
        // Legacy `Environment`-format file: `key:` instead of `name:`, so the
        // OcEnvironment parse fails and the fallback parser handles it.
        let legacy = "name: legacy\nvariables:\n- key: API_KEY\n  value: plaintext-token\n  enabled: true\n  secret: true\n";
        std::fs::write(dir.path().join("legacy.yml"), legacy).expect("write legacy.yml");

        let loaded = repo.get("legacy").expect("get");
        let api_key = loaded
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("API_KEY entry");
        assert!(api_key.secret);
        assert_eq!(
            api_key.value, "plaintext-token",
            "a store miss must not destroy existing data"
        );
    }

    #[test]
    fn two_directories_do_not_share_secret_entries() {
        let store = Arc::new(InMemorySecretStore::default());
        let dir_a = TempDir::new().expect("temp dir a");
        let dir_b = TempDir::new().expect("temp dir b");
        let repo_a = FsEnvironmentRepo::with_secret_store(dir_a.path().to_path_buf(), store.clone());
        let repo_b = FsEnvironmentRepo::with_secret_store(dir_b.path().to_path_buf(), store.clone());

        let mut env_a = Environment::new("prod");
        env_a.set_variable(Variable::secret("API_KEY", "value-a"));
        repo_a.save(&env_a).expect("save a");

        let mut env_b = Environment::new("prod");
        env_b.set_variable(Variable::secret("API_KEY", "value-b"));
        repo_b.save(&env_b).expect("save b");

        assert_eq!(store.len(), 2, "same env name in different directories must not collide");
        assert_eq!(repo_a.get("prod").expect("get a").get_value("API_KEY"), Some("value-a"));
        assert_eq!(repo_b.get("prod").expect("get b").get_value("API_KEY"), Some("value-b"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra secret_value_roundtrips`
Expected: FAIL — `assertion failed: left: "", right: "sk-live-123"` (the value is written to the store but never read back).

- [ ] **Step 3: Add the hydration helper**

Add to the inherent `impl FsEnvironmentRepo` block, after `persisted_secret_keys`:

```rust
    /// Fill in real values for secret variables from the secret store.
    ///
    /// A store failure must never fail an environment load — that would brick
    /// app startup on a locked keychain — so it is logged and the variable keeps
    /// whatever value the YAML produced (empty for a spec SecretVariable entry).
    fn hydrate_secrets(&self, env: &mut Environment) {
        if !env.variables.iter().any(|v| v.secret) {
            return;
        }
        let scope = Self::scope_id(&self.dir, &env.name);
        for var in env.variables.iter_mut().filter(|v| v.secret) {
            match self.secret_store.get(&scope, &var.key) {
                Ok(Some(value)) => var.value = value,
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        key = %var.key,
                        error = %e,
                        "secret store unavailable, environment secret left unresolved"
                    );
                }
            }
        }
    }
```

- [ ] **Step 4: Call it from `list()`**

Replace the body of `list()` with:

```rust
    fn list(&self) -> DomainResult<Vec<Environment>> {
        let mut result = Vec::new();
        if !self.dir.exists() {
            return Ok(result);
        }
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.extension().is_some_and(|e| e == "yml") {
                continue;
            }
            let content = fs::read_to_string(&path)?;
            let parsed = if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
                Some(Environment::from(oc))
            } else if let Ok(env) = serde_yaml::from_str::<Environment>(&content) {
                Some(env)
            } else {
                tracing::warn!(path = %path.display(), "skipping corrupt environment YAML file");
                None
            };
            if let Some(mut env) = parsed {
                self.hydrate_secrets(&mut env);
                result.push(env);
            }
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }
```

- [ ] **Step 5: Call it from `get()`**

Replace the body of `get()` with:

```rust
    fn get(&self, name: &str) -> DomainResult<Environment> {
        let path = self.file_path(name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!("Environment '{}'", name)));
        }
        let content = fs::read_to_string(&path)?;
        let mut env = if let Ok(oc) = serde_yaml::from_str::<OcEnvironment>(&content) {
            Environment::from(oc)
        } else {
            serde_yaml::from_str::<Environment>(&content).map_err(|e| {
                DomainError::Internal(format!("Failed to parse environment YAML: {e}"))
            })?
        };
        self.hydrate_secrets(&mut env);
        Ok(env)
    }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-infra fs_environment_repo`
Expected: PASS — all tests in the module, including the 6 new ones.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/src/fs_environment_repo.rs
git commit -m "feat(infra): hydrate environment secret values from the SecretStore on load"
```

---

### Task 7: Clean up stale secrets when a variable stops being secret

**Files:**
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs` — the `save` method from Task 5
- Test: `crates/rocket-infra/src/fs_environment_repo.rs` (existing test module)

**Interfaces:**
- Consumes: `FsEnvironmentRepo::persisted_secret_keys` and `scope_id` (Task 5), `self.secret_store.delete`, `setup_with_store()` (Task 5).
- Produces: no new public API. `save()` gains best-effort cleanup of keychain entries whose variable is no longer secret or no longer present.

This implements spec section 4.4's sentence: "If the variable *used to be* secret and no longer is (or was removed), call `secret_store.delete(scope_id, key)` — best-effort, log on failure, don't abort."

- [ ] **Step 1: Write the failing tests**

Append to the test module in `crates/rocket-infra/src/fs_environment_repo.rs`:

```rust
    #[test]
    fn unsetting_the_secret_flag_removes_the_stored_secret() {
        let (dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("first save");
        assert_eq!(store.len(), 1);

        env.set_variable(Variable::new("API_KEY", "not-a-secret-anymore"));
        repo.save(&env).expect("second save");

        assert_eq!(store.len(), 0, "a stale keychain entry must be removed");
        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(raw.contains("value: not-a-secret-anymore"), "got:\n{raw}");
        assert!(!raw.contains("secret: true"), "got:\n{raw}");
    }

    #[test]
    fn removing_a_secret_variable_removes_the_stored_secret() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        env.set_variable(Variable::secret("TOKEN", "tok-456"));
        repo.save(&env).expect("first save");
        assert_eq!(store.len(), 2);

        env.remove_variable("API_KEY");
        repo.save(&env).expect("second save");

        assert_eq!(store.len(), 1);
        assert!(store.contains_value("tok-456"), "the surviving secret must be untouched");
        assert!(!store.contains_value("sk-live-123"));
    }

    #[test]
    fn resaving_an_unchanged_secret_keeps_it() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        repo.save(&env).expect("first save");
        repo.save(&env).expect("second save");

        assert_eq!(store.len(), 1);
        assert_eq!(repo.get("prod").expect("get").get_value("API_KEY"), Some("sk-live-123"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra removes_the_stored_secret`
Expected: FAIL — `assertion failed: left: 1, right: 0` in `unsetting_the_secret_flag_removes_the_stored_secret`.

- [ ] **Step 3: Snapshot the previous secret keys in `save()`**

In the `save` method, insert this immediately after `let scope = Self::scope_id(&self.dir, &env.name);`:

```rust
        // Snapshot which keys were secret before this save, so entries left
        // behind by an un-secreted or removed variable can be cleaned up.
        let previous_secret_keys = self.persisted_secret_keys(&env.name);
```

- [ ] **Step 4: Add the cleanup loop**

In the same method, insert this block immediately before the final `Ok(())`:

```rust
        // Best-effort cleanup. A stale entry leaks nothing new, so a failure
        // here must not fail the save the user just asked for.
        for key in previous_secret_keys {
            if env.variables.iter().any(|v| v.secret && v.key == key) {
                continue;
            }
            if let Err(e) = self.secret_store.delete(&scope, &key) {
                tracing::warn!(key = %key, error = %e, "failed to remove stale environment secret");
            }
        }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-infra fs_environment_repo`
Expected: PASS — all tests in the module, including the 3 new ones.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/fs_environment_repo.rs
git commit -m "fix(infra): delete stale keychain entries when a variable stops being secret"
```

---

### Task 8: Remove an environment's secrets when the environment is deleted

**Files:**
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs` — the `delete` method (originally lines 71-73)
- Test: `crates/rocket-infra/src/fs_environment_repo.rs` (existing test module)

**Interfaces:**
- Consumes: `FsEnvironmentRepo::persisted_secret_keys` and `scope_id` (Task 5), `setup_with_store()` (Task 5).
- Produces: no new public API. `EnvironmentRepository::delete`'s signature is unchanged.

**Not in the spec.** Section 4.4 covers only `save`/`get`/`list`, so deleting an environment currently orphans its secrets in the OS keychain forever. This is isolated in its own task so a reviewer can reject it without blocking the rest of the plan; the cleanup is best-effort and can never fail the delete.

- [ ] **Step 1: Write the failing tests**

Append to the test module in `crates/rocket-infra/src/fs_environment_repo.rs`:

```rust
    #[test]
    fn deleting_an_environment_removes_its_secrets() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::secret("API_KEY", "sk-live-123"));
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");
        assert_eq!(store.len(), 1);

        repo.delete("prod").expect("delete");

        assert_eq!(store.len(), 0, "a deleted environment must not leave secrets behind");
        assert!(repo.list().expect("list").is_empty());
    }

    #[test]
    fn deleting_an_environment_with_no_secrets_still_succeeds() {
        let (_dir, repo, store) = setup_with_store();
        let mut env = Environment::new("prod");
        env.set_variable(Variable::new("HOST", "api.example.com"));
        repo.save(&env).expect("save");

        repo.delete("prod").expect("delete");
        assert_eq!(store.len(), 0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra deleting_an_environment`
Expected: FAIL — `assertion failed: left: 1, right: 0` in `deleting_an_environment_removes_its_secrets`.

- [ ] **Step 3: Write minimal implementation**

Replace the `delete` method with:

```rust
    fn delete(&self, name: &str) -> DomainResult<()> {
        // Read the secret key list while the file still exists.
        let scope = Self::scope_id(&self.dir, name);
        let secret_keys = self.persisted_secret_keys(name);

        delete_if_exists(&self.file_path(name), &format!("Environment '{}'", name))?;

        // Best-effort: the environment is already gone, so a store failure here
        // must not surface as a failed delete.
        for key in secret_keys {
            if let Err(e) = self.secret_store.delete(&scope, &key) {
                tracing::warn!(key = %key, error = %e, "failed to remove secret for deleted environment");
            }
        }
        Ok(())
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-infra fs_environment_repo`
Expected: PASS — all tests in the module, including the 2 new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/fs_environment_repo.rs
git commit -m "fix(infra): remove stored secrets when an environment is deleted"
```

---

### Task 9: Wire `KeyringSecretStore` into every `FsEnvironmentRepo` construction site

**Files:**
- Modify: `src-tauri/src/lib.rs` — imports (lines 9-23), a new `env_secret_store()` helper, and the two construction sites at lines 196 and 208
- Modify: `src-tauri/src/commands/environments.rs:29` (`env_service_for`, per-collection) and `:106` (`global_env_service`, workspace-level)
- Modify: `src-tauri/src/commands/import.rs:11-15` (`FsEnvFactory::make`)

**Interfaces:**
- Consumes: `rocket_infra::KeyringSecretStore` (Task 2), `rocket_environment::secret_store::SecretStore` (Task 1), `FsEnvironmentRepo::with_secret_store` (Task 5).
- Produces: `pub(crate) fn env_secret_store() -> Arc<dyn SecretStore>` in `src-tauri/src/lib.rs`, callable from command modules as `crate::env_secret_store()`.

**Note on the spec's "construct one `Arc` at startup":** `KeyringSecretStore` is a zero-sized, stateless unit struct, and three of the five construction sites build their `FsEnvironmentRepo` per IPC call rather than at startup. Threading a Tauri managed-state handle into those command signatures would buy nothing over calling the helper, so the helper is the single point of definition instead. There are **five** construction sites, not the two the spec anticipated — all five are wired here. `FsEnvironmentRepo::new` stays available and keeps the `rocket-import` integration tests (`crates/rocket-import/tests/integration_test.rs:10`, `crates/rocket-import/tests/postman_integration_test.rs:10`, `crates/rocket-import/src/importer.rs:71`) compiling unchanged.

- [ ] **Step 1: Add the helper**

In `src-tauri/src/lib.rs`, add `KeyringSecretStore,` to the `use rocket_infra::{ ... };` list (alphabetically, right after `FsWorkspaceConfigRepo,`), and add this import line after it:

```rust
use rocket_environment::secret_store::SecretStore;
```

Then add this function immediately above `#[cfg_attr(mobile, tauri::mobile_entry_point)]`:

```rust
/// OS-keychain backend for environment secret values.
///
/// `KeyringSecretStore` is a stateless unit struct, so each call site can build
/// its own handle; this helper keeps the concrete type in one place. Every
/// `FsEnvironmentRepo` that serves user-facing environments must be built with
/// it — `FsEnvironmentRepo::new` silently drops secret values.
pub(crate) fn env_secret_store() -> Arc<dyn SecretStore> {
    Arc::new(KeyringSecretStore)
}
```

- [ ] **Step 2: Wire the two startup sites**

In `src-tauri/src/lib.rs`, change line 196 from:

```rust
                Box::new(FsEnvironmentRepo::new(environments_dir.clone())),
```

to:

```rust
                Box::new(FsEnvironmentRepo::with_secret_store(
                    environments_dir.clone(),
                    env_secret_store(),
                )),
```

and change line 208 from:

```rust
                Box::new(FsEnvironmentRepo::new(environments_dir)),
```

to:

```rust
                Box::new(FsEnvironmentRepo::with_secret_store(
                    environments_dir,
                    env_secret_store(),
                )),
```

- [ ] **Step 3: Wire the two command sites**

In `src-tauri/src/commands/environments.rs`, change line 29 inside `env_service_for` from:

```rust
        Box::new(FsEnvironmentRepo::new(env_dir)),
```

to:

```rust
        Box::new(FsEnvironmentRepo::with_secret_store(env_dir, crate::env_secret_store())),
```

and make the identical change at line 106 inside `global_env_service`.

- [ ] **Step 4: Wire the importer site**

In `src-tauri/src/commands/import.rs`, change `FsEnvFactory::make` from:

```rust
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0.join("collections").join(collection_name).join("environments"),
        ))
    }
```

to:

```rust
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::with_secret_store(
            self.0.join("collections").join(collection_name).join("environments"),
            crate::env_secret_store(),
        ))
    }
```

- [ ] **Step 5: Verify every site is wired**

Run: `grep -rn "FsEnvironmentRepo::new" src-tauri/src`
Expected: **no output**. Any remaining hit is a site that would silently drop user secrets.

- [ ] **Step 6: Verify it compiles cleanly**

Run: `cargo check -p rocket`
Expected: no errors and no unused-import warnings.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands/environments.rs src-tauri/src/commands/import.rs
git commit -m "feat(tauri): back every environment repository with the OS keychain secret store"
```

---

### Task 10: Verify the spec's acceptance criteria end to end

**Files:**
- Modify: none (verification only, unless a check fails)
- Test: the whole workspace plus the frontend type-check

**Interfaces:**
- Consumes: everything from Tasks 1-9.
- Produces: a green workspace and the migration note recorded in the commit body.

**Acceptance-criteria map** (spec section 8 to the test that proves it):

| # | Spec criterion | Test |
|---|---|---|
| 1 | Saved `.yml` does not contain `sk-live-123` anywhere | `rocket-infra` · `save_keeps_the_secret_value_out_of_the_yaml_file` (Task 5) |
| 2 | `.yml` contains `secret: true` and no `value:` on that entry | `rocket-infra` · `save_writes_a_spec_secret_variable_entry` (Task 5), reinforced by `environment_secret_variable_to_oc_drops_the_value` (Task 4) |
| 3 | `get` returns `secret: true` and `value == "sk-live-123"` | `rocket-infra` · `secret_value_roundtrips_through_save_and_get` (Task 6) |
| 4 | Non-secret on-disk representation and round-trip unchanged | The three pre-existing tests `save_writes_spec_field_names`, `save_then_load_roundtrip_via_oc_format`, `load_old_format_with_key_field_still_works` pass **unmodified**, plus the new `non_secret_variables_still_write_their_value_to_yaml` (Task 5) |
| 5 | `cargo test -p rocket-environment -p rocket-infra` passes, with `KeyringSecretStore` covered without a real CI keychain | Step 2 below; `InMemorySecretStore` double (Task 5) plus the `#[ignore]` keychain tests (Task 2) |
| 6 | `yarn tsc --noEmit` passes | Step 4 below |

- [ ] **Step 1: Confirm the three named legacy tests were never edited**

Run: `git log -p -- crates/rocket-infra/src/fs_environment_repo.rs | grep -E "^-.*fn (save_writes_spec_field_names|save_then_load_roundtrip_via_oc_format|load_old_format_with_key_field_still_works)"`
Expected: **no output**. A `-` line would mean one of the three was modified or deleted, which acceptance criterion 4 forbids.

- [ ] **Step 2: Run the two Rust crates named in the spec**

Run: `cargo test -p rocket-environment -p rocket-infra`
Expected: PASS, 0 failed. The `rocket-infra` summary must show at least 2 ignored tests (the `keyring_*` pair from Task 2) alongside the pre-existing ignored network tests noted in `crates/rocket-infra/CLAUDE.md`.

- [ ] **Step 3: Run the rest of the workspace**

Run: `cargo test --workspace`
Expected: PASS. `rocket-app`'s `environment_service` tests and `rocket-import`'s integration tests must be unaffected — `EnvironmentService` was not changed, and `FsEnvironmentRepo::new` still exists.

- [ ] **Step 4: Verify the frontend contract did not move**

Run: `yarn tsc --noEmit`
Expected: no errors. No IPC DTO changed, so this is a regression check rather than an expected-change check.

- [ ] **Step 5: Run the linters**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

Run: `yarn check`
Expected: no Biome errors.

- [ ] **Step 6: Confirm no panicking call crept into a production path**

Run: `grep -n "unwrap()" crates/rocket-environment/src/secret_store.rs crates/rocket-infra/src/secret_store.rs crates/rocket-infra/src/conversions/environment.rs src-tauri/src/commands/import.rs`
Expected: **no output**.

Run: `grep -n "unwrap" crates/rocket-infra/src/fs_environment_repo.rs`
Expected: only the single `unwrap_or_else` on `dir.canonicalize()` inside `scope_id` (a total fallback, not a panic) plus lines inside the `#[cfg(test)]` module. No bare `.unwrap()` above the `#[cfg(test)]` marker.

- [ ] **Step 7: Confirm criterion 1 against a real keychain (local only, optional on CI)**

Run: `cargo test -p rocket-infra keyring_ -- --ignored`
Expected: PASS on a machine with a working Keychain / Credential Manager / Secret Service. If the keychain is unavailable, record that and move on — the Global Constraints keep these tests out of the required gate.

- [ ] **Step 8: Commit the verification and record the migration note**

```bash
git commit --allow-empty -m "chore(environment): verify secret storage hardening acceptance criteria" -m "Secret values for environment variables now live in the OS keychain instead of <workspace>/environments/*.yml. Existing files are not migrated automatically: a variable previously marked secret keeps its plaintext value in YAML until the user reopens that environment and saves it once, which moves the value into the keychain and rewrites the entry as a value-less SecretVariable. Users who have already committed an environments/ directory to git should rotate those credentials - this change stops new leaks but cannot scrub git history."
```

`CHANGELOG.md` is generated by release-please from conventional commits, so this commit body is the release note rather than a hand-edited changelog section.

---

## Compatibility with downstream roadmap items

Roadmap items 3 and 4 depend on this plan. Both were checked against the interfaces above:

- **Item 3 — [`2026-09-16-env-var-write-audit-spec.md`](../specs/2026-09-16-env-var-write-audit-spec.md)** depends only on "`secret`/`secret_type` actually persist correctly once preserved" through `env_repo.save()`. Its section 5 Interfaces introduce `rocket_app::env_audit::publish_env_write_events`, a rewritten `RequestExecutionService::apply_env_writes`, and a refactored `EnvironmentService::save` — none of which this plan touches. The `EnvironmentRepository` trait signature is unchanged, so `apply_env_writes` keeps compiling.
- **Item 4 — [`2026-09-16-secret-aware-variable-context-spec.md`](../specs/2026-09-16-secret-aware-variable-context-spec.md)** depends on "`Variable.secret` actually round-tripping correctly through `EnvironmentRepository`", which Task 6's `secret_value_roundtrips_through_save_and_get` proves. Its section 5 Interfaces add `VariableContext.secret_values`, `ScriptInputState.secret_values` and a console `redact` helper — all in files this plan does not touch (`crates/rocket-environment/src/context.rs`, `crates/rocket-infra/src/scripting/*`).

Neither spec references `SecretStore`, `KeyringSecretStore`, `with_secret_store` or `OcEnvVariableEntry`, so there is no naming or signature conflict to reconcile.

## Out of scope (spec section 7 follow-ups, not implemented here)

- `.gitignore` scaffolding for `environments/` in newly git-init'd workspaces and collections.
- `CollectionVariable.secret` in `rocket-collection` — collection variables are meant to be committed, so whether "secret" is even a supported concept there is a product question.
- An in-app "migrate plaintext secrets now" action.
