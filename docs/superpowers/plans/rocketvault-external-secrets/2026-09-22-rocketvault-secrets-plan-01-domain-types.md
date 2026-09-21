# RocketVault Secrets Plan 01: Domain Types — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the pure domain types for RocketVault External Secrets:
`ExternalSecretBinding`/`ExternalSecretRef` (nested inside `Environment`) and
`SecretManagerConnection`/`SecretManagerRepository` (the app-level connection
registry). No I/O, no network — this crate has none per its own CLAUDE.md.

**Architecture:** Two new modules in `rocket-environment`, following the exact
pattern `variable.rs`/`repository.rs` already establish: plain serde structs
plus a trait for the persistence boundary, both consumed later by
`rocket-infra` (concrete impls) and `rocket-app` (orchestration). `Environment`
gains one new field with `#[serde(default)]` so every existing environment
`.yml` file keeps loading unchanged.

**Tech Stack:** Rust, serde, serde_json (tests).

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.4). Plan index: `docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md`
(has the full locked interface contract every later plan in this series depends on).

## Global Constraints

- `ExternalSecretBinding`/`ExternalSecretRef` use `#[serde(rename_all = "camelCase")]`
  — they nest inside `Environment`, which already uses camelCase for its own
  `Variable` entries (see `crates/rocket-environment/src/variable.rs:6`); match
  that existing convention exactly, do not use snake_case here.
- `SecretManagerConnection` does **not** get a camelCase rename — it persists to
  its own app-level `secret_managers.yml` (Plan 04), not the OpenCollection
  format, so the codebase's general "no camelCase on persistence structs" rule
  applies to it specifically.
- Every new field added to an existing type (`Environment.external_secrets`)
  must use `#[serde(default)]` so old `.yml` files without the field still
  deserialize — this crate's CLAUDE.md states this as a hard rule.
- Read `docs/superpowers/specs/opencollection-spec-reference.md` before writing
  the `Environment` field change in Task 3 (per this repo's OpenCollection
  injection rule for any change touching environment/variable data models).
- Test code in this plan (and all other plans in this series) uses
  `.expect("message")` for fallible calls rather than the bare panicking
  shorthand, matching this repository's stricter Rust safety convention even
  in test paths.

---

## Task 1: `ExternalSecretRef` and `ExternalSecretBinding`

**Files:**
- Create: `crates/rocket-environment/src/external_secret.rs`
- Modify: `crates/rocket-environment/src/lib.rs`

**Interfaces:**
- Produces: `ExternalSecretRef { name: String, secret_id: String }`,
  `ExternalSecretBinding { alias: String, connection_id: String, vault_name: String, secret_names: Vec<ExternalSecretRef> }`
  — consumed by Task 3 of this plan, and by every later plan in this series
  (Plans 02–10) that touches an environment's external secrets.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-environment/src/external_secret.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_secret_ref_serde_roundtrip() {
        let r = ExternalSecretRef {
            name: "stripe-key".to_string(),
            secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
        };
        let json = serde_json::to_string(&r).expect("serialize ExternalSecretRef");
        assert!(json.contains("\"secretId\""), "expected camelCase field, got: {json}");
        let back: ExternalSecretRef = serde_json::from_str(&json).expect("deserialize ExternalSecretRef");
        assert_eq!(r, back);
    }

    #[test]
    fn external_secret_binding_serde_roundtrip() {
        let b = ExternalSecretBinding {
            alias: "payments".to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            secret_names: vec![ExternalSecretRef {
                name: "stripe-key".to_string(),
                secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
            }],
        };
        let json = serde_json::to_string(&b).expect("serialize ExternalSecretBinding");
        assert!(json.contains("\"connectionId\""), "expected camelCase field, got: {json}");
        assert!(json.contains("\"vaultName\""), "expected camelCase field, got: {json}");
        let back: ExternalSecretBinding = serde_json::from_str(&json).expect("deserialize ExternalSecretBinding");
        assert_eq!(b, back);
    }

    #[test]
    fn external_secret_binding_defaults_secret_names_to_empty() {
        let json = r#"{"alias":"payments","connectionId":"conn-1","vaultName":"prod-vault"}"#;
        let b: ExternalSecretBinding = serde_json::from_str(json).expect("deserialize without secretNames");
        assert!(b.secret_names.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-environment external_secret::tests`
Expected: FAIL with "cannot find type `ExternalSecretRef`" (module doesn't exist yet).

- [ ] **Step 3: Implement the types**

```rust
// crates/rocket-environment/src/external_secret.rs (add above the tests module)
use serde::{Deserialize, Serialize};

/// One secret name captured from a RocketVault "Fetch Secrets" action, paired
/// with the vault's own UUID so later value lookups skip a list round-trip.
/// Never carries a secret *value* — see spec §4.4/§4.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretRef {
    pub name: String,
    pub secret_id: String,
}

/// Binds a RocketVault connection + vault to an environment under a
/// user-chosen alias. `{{alias.secretName}}` and
/// `rok.getSecretVar('alias.secretName')` both resolve through this binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretBinding {
    pub alias: String,
    pub connection_id: String,
    pub vault_name: String,
    #[serde(default)]
    pub secret_names: Vec<ExternalSecretRef>,
}
```

- [ ] **Step 4: Register the module**

In `crates/rocket-environment/src/lib.rs`, add alongside the existing module
declarations:

```rust
pub mod external_secret;
pub use external_secret::{ExternalSecretBinding, ExternalSecretRef};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-environment external_secret::tests`
Expected: PASS — 3 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-environment/src/external_secret.rs crates/rocket-environment/src/lib.rs
git commit -m "feat(environment): add ExternalSecretRef/Binding types"
```

---

## Task 2: `SecretManagerConnection` and `SecretManagerRepository`

**Files:**
- Create: `crates/rocket-environment/src/secret_manager.rs`
- Modify: `crates/rocket-environment/src/lib.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `SecretManagerConnection { id, label, base_url, client_id, verify_ssl, allow_insecure_http }`,
  `trait SecretManagerRepository { list, get, save, delete }` — consumed by
  Plan 04 (`FsSecretManagerRepo` impl), Plan 05 (`SecretManagerService`), and
  Plan 02's `VaultSecretFetcher` trait (which takes `&SecretManagerConnection`
  by reference).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-environment/src/secret_manager.rs
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainResult;
    use std::sync::Mutex;

    #[test]
    fn connection_serde_roundtrip_no_camelcase() {
        let c = SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        };
        let json = serde_json::to_string(&c).expect("serialize SecretManagerConnection");
        // Deliberately NOT camelCase — plain field names, see Global Constraints.
        assert!(json.contains("\"base_url\""), "expected snake_case field, got: {json}");
        assert!(json.contains("\"client_id\""), "expected snake_case field, got: {json}");
        let back: SecretManagerConnection =
            serde_json::from_str(&json).expect("deserialize SecretManagerConnection");
        assert_eq!(c, back);
    }

    #[test]
    fn connection_verify_ssl_defaults_true_when_absent() {
        let json = r#"{"id":"c1","label":"L","base_url":"https://x","client_id":"cid"}"#;
        let c: SecretManagerConnection = serde_json::from_str(json).expect("deserialize minimal connection");
        assert!(c.verify_ssl, "verify_ssl should default to true for safety");
        assert!(!c.allow_insecure_http);
    }

    // In-memory fake exercising the trait contract — mirrors the style of
    // inline mocks already used throughout rocket-app's own tests.
    struct FakeRepo(Mutex<Vec<SecretManagerConnection>>);
    impl SecretManagerRepository for FakeRepo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            let guard = self.0.lock().expect("lock FakeRepo");
            Ok(guard.clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            let guard = self.0.lock().expect("lock FakeRepo");
            Ok(guard.iter().find(|c| c.id == id).cloned())
        }
        fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|c| c.id != connection.id);
            guard.push(connection.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|c| c.id != id);
            Ok(())
        }
    }

    #[test]
    fn repository_trait_save_get_delete_roundtrip() {
        let repo = FakeRepo(Mutex::new(Vec::new()));
        let c = SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Prod".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        };
        repo.save(&c).expect("save connection");
        assert_eq!(repo.get("conn-1").expect("get connection"), Some(c.clone()));
        assert_eq!(repo.list().expect("list connections").len(), 1);
        repo.delete("conn-1").expect("delete connection");
        assert_eq!(repo.get("conn-1").expect("get after delete"), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-environment secret_manager::tests`
Expected: FAIL with "cannot find type `SecretManagerConnection`".

- [ ] **Step 3: Implement the type and trait**

```rust
// crates/rocket-environment/src/secret_manager.rs (add above the tests module)
use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// An app-level, reusable connection to a RocketVault server. Persisted
/// separately from any workspace/environment (see Plan 04's
/// `FsSecretManagerRepo`) — the actual `client_secret` never lives on this
/// struct or in its persisted form; it is stored only in the OS keychain,
/// looked up by `id` (see Plan 04/05).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecretManagerConnection {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub client_id: String,
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
    #[serde(default)]
    pub allow_insecure_http: bool,
}

/// Persistence boundary for `SecretManagerConnection`. No I/O in this crate —
/// `rocket-infra`'s `FsSecretManagerRepo` (Plan 04) implements this.
pub trait SecretManagerRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>>;
    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>>;
    fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}
```

- [ ] **Step 4: Register the module**

In `crates/rocket-environment/src/lib.rs`:

```rust
pub mod secret_manager;
pub use secret_manager::{SecretManagerConnection, SecretManagerRepository};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-environment secret_manager::tests`
Expected: PASS — 3 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-environment/src/secret_manager.rs crates/rocket-environment/src/lib.rs
git commit -m "feat(environment): add SecretManagerConnection + repository trait"
```

---

## Task 3: `Environment.external_secrets` field

**Files:**
- Modify: `crates/rocket-environment/src/environment.rs`

**Interfaces:**
- Consumes: `ExternalSecretBinding` from Task 1.
- Produces: `Environment.external_secrets: Vec<ExternalSecretBinding>` —
  consumed by Plan 04 (Oc conversions), Plan 06 (`resolve_external_secrets`
  reads it), Plan 10 (frontend reads/writes it via the existing
  `get_environment`/`save_environment` commands).

> 📖 Before starting this task, read
> `docs/superpowers/specs/opencollection-spec-reference.md` (touches the
> `Environment` domain model, per this repo's OpenCollection injection rule).

- [ ] **Step 1: Write the failing test**

Open `crates/rocket-environment/src/environment.rs` and find its existing
`#[cfg(test)] mod tests` block (it already has fixtures constructing
`Environment { name, variables, ... }` — match that construction style
exactly, adding `external_secrets: Vec::new()` to every existing literal that
the compiler flags as missing a field once Step 3 lands). Add:

```rust
#[test]
fn environment_external_secrets_defaults_to_empty_on_old_yaml() {
    // Simulates loading a pre-existing environment file saved before this
    // field existed — must not fail to deserialize.
    let json = r#"{"name":"prod","variables":[]}"#;
    let env: Environment = serde_json::from_str(json).expect("deserialize old-format environment");
    assert!(env.external_secrets.is_empty());
}

#[test]
fn environment_external_secrets_roundtrip() {
    use crate::external_secret::{ExternalSecretBinding, ExternalSecretRef};
    let mut env = Environment::new("prod");
    env.external_secrets.push(ExternalSecretBinding {
        alias: "payments".to_string(),
        connection_id: "conn-1".to_string(),
        vault_name: "prod-vault".to_string(),
        secret_names: vec![ExternalSecretRef {
            name: "stripe-key".to_string(),
            secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
        }],
    });
    let json = serde_json::to_string(&env).expect("serialize environment");
    let back: Environment = serde_json::from_str(&json).expect("deserialize environment");
    assert_eq!(env.external_secrets, back.external_secrets);
}
```

(If `Environment::new(name)` doesn't already exist as a constructor, check the
existing test fixtures in this file for however `Environment` values are
currently built in tests and match that pattern instead — do not invent a new
constructor as part of this task.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-environment environment::tests`
Expected: FAIL — `external_secrets` field does not exist yet (compile error on
the second test; the first test fails at the `env.external_secrets` assertion
once the struct exists but before the field is added it won't compile at all,
so expect a compile-time failure for the whole module, not a runtime one).

- [ ] **Step 3: Add the field**

In `crates/rocket-environment/src/environment.rs`, add to the `Environment`
struct definition (alongside its existing `name`/`variables`/etc. fields):

```rust
#[serde(default)]
pub external_secrets: Vec<crate::external_secret::ExternalSecretBinding>,
```

Then fix every existing struct-literal construction of `Environment` in this
file (both non-test code, if any, and the `#[cfg(test)]` fixtures) by adding
`external_secrets: Vec::new()` — the compiler will point at each one.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-environment`
Expected: PASS — full crate, including the two new tests and every
pre-existing test in `environment.rs` (backward compatibility check).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-environment/src/environment.rs
git commit -m "feat(environment): add external_secrets field to Environment"
```

---

## Milestone Checklist — Plan 01

- [ ] `ExternalSecretRef` — `name`, `secretId` (camelCase), serde roundtrip
- [ ] `ExternalSecretBinding` — `alias`, `connectionId`, `vaultName`, `secretNames` (camelCase), defaults `secretNames` to empty
- [ ] `SecretManagerConnection` — `id`, `label`, `base_url`, `client_id`, `verify_ssl` (defaults `true`), `allow_insecure_http` (defaults `false`); plain field names, no camelCase
- [ ] `SecretManagerRepository` trait — `list`/`get`/`save`/`delete`, exercised by an in-memory fake
- [ ] `Environment.external_secrets: Vec<ExternalSecretBinding>` — `#[serde(default)]`, old environment YAML without the field still loads
- [ ] `cargo test -p rocket-environment` — all pass

## Next Plan

[Plan 02: VaultSecretFetcher trait + VariableContext wiring](2026-09-22-rocketvault-secrets-plan-02-fetcher-trait-and-context.md) —
adds the async fetcher trait (consumes `SecretManagerConnection` and
`ExternalSecretRef` from this plan) and wires `VariableContext` for
`{{alias.secretName}}` resolution.
