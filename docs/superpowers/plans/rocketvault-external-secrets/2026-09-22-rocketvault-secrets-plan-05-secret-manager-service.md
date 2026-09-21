# RocketVault Secrets Plan 05: SecretManagerService and the Shared Vault-Secret Resolution Helper

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `SecretManagerService` (CRUD for `SecretManagerConnection` plus
live-fetch operations against RocketVault) and the shared
`resolve_vault_secret_value` free function, both in `rocket-app`. This is the
first service in the RocketVault External Secrets series to live in
`rocket-app` — it is the orchestration layer that combines the domain traits
from Plans 01–04 (all already shipped by the time this plan runs) into one
usable unit. It introduces no new domain types and no new I/O — every trait it
consumes is injected as a trait object.

**Architecture:** `SecretManagerService` follows the exact shape of every
other `rocket-app` service (see `EnvironmentService`,
`SecurityAuditService`): a plain struct holding trait objects, a `new`
constructor, and thin methods that call through to those trait objects with
minimal orchestration logic in between. `resolve_vault_secret_value` is a
free function, not a method, because it is shared by two different callers —
this plan's own module and, starting in Plan 06, `RequestExecutionService` —
and this codebase's existing precedent for logic shared across services is a
small free function rather than a service-to-service dependency (services
never call other services directly).

**Tech Stack:** Rust, `async-trait`, `tokio` (dev-dep, for `#[tokio::test]`).

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.1, §4.6). Plan index: `docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md`
(has the full locked interface contract every plan in this series depends on
— this plan implements exactly the `rocket-app (new, Plan 05)` section of
that contract; do not deviate from the signatures given there).

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
> (This plan's scope sits directly upstream of collection/environment variable
> resolution wired in Plan 06, and this repo's `rust-ddd-boundaries.md` rule
> file requires this reference for any yml/collection/environment/variable-
> resolution-adjacent scope.)

## Global Constraints

- **Consume, do not redefine.** This plan builds entirely on trait objects
  from earlier plans in this series:
  - `rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository}`
    (Plan 01) — `list`, `get`, `save`, `delete`, all `DomainResult`.
  - `rocket_environment::secret_store::SecretStore` (already shipped, existing
    type, unrelated to this feature series originally but reused here) —
    `fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>>`,
    `fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()>`,
    `fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()>`.
  - `rocket_environment::vault_secret_fetcher::VaultSecretFetcher` (Plan 02) —
    `async fn list_secrets(&self, connection: &SecretManagerConnection, client_secret: &str, vault_name: &str) -> DomainResult<Vec<ExternalSecretRef>>`,
    `async fn get_secret_value(&self, connection: &SecretManagerConnection, client_secret: &str, vault_name: &str, secret_id: &str) -> DomainResult<Option<String>>`,
    `async fn test_connection(&self, connection: &SecretManagerConnection, client_secret: &str, vault_name: &str) -> DomainResult<()>`.

  Do not add methods to any of these traits, do not change their signatures,
  and do not re-implement a concrete `SecretStore`/`VaultSecretFetcher` here —
  this plan only writes orchestration code in `rocket-app` against these
  interfaces.

- **Client-secret storage convention — read this carefully, it is easy to get
  backwards.** A connection's `client_secret` is stored via the *existing*
  `SecretStore` trait, injected into `SecretManagerService` as
  `Arc<dyn SecretStore>` — this is **not** a new trait, and it is **not** the
  same `SecretStore` instance used for environment-variable secrets (Plan 04
  wires a distinct `KeyringSecretStore::new_vault_connections()` keychain
  namespace for it in `src-tauri`; that wiring is Plan 08's job, not this
  plan's — this plan just takes whatever `Arc<dyn SecretStore>` its
  constructor is given).

  The fixed `scope_id` constant `"vault-connection"` is used for **every**
  connection — all connections share this one scope, because `key` (the
  connection's `id`) already uniquely identifies each one within it. There is
  no need for a second dimension of scoping here, unlike environment secrets,
  where `scope_id` distinguishes different environment files from each other
  (see `crates/rocket-environment/src/secret_store.rs`'s doc comment: "two
  environments that share a variable name never share a stored secret" — that
  problem does not exist for vault connections, since connection `id`s are
  already globally unique). Concretely:
  - Store: `secret_store.set("vault-connection", &connection.id, &client_secret)`
  - Retrieve: `secret_store.get("vault-connection", &connection.id)`
  - Remove: `secret_store.delete("vault-connection", &connection.id)`

- **Trait-object injection.** Match `crates/rocket-app/CLAUDE.md`'s "Key
  Patterns" section exactly: every service takes its dependencies via
  constructor as `Box<dyn Trait>` or `Arc<dyn Trait>`, no concrete types
  appear in `rocket-app`. `SecretManagerRepository` is owned
  (`Box<dyn SecretManagerRepository>`, matching `EnvironmentRepository`'s
  ownership shape in `EnvironmentService`); `SecretStore` and
  `VaultSecretFetcher` are shared (`Arc<dyn ...>`, matching
  `HttpExecutor`'s/`SecurityAuditPublisher`'s ownership shape in
  `RequestExecutionService`/`EnvironmentService`) because both are expected to
  be reused by other services later in this series (Plan 06's
  `RequestExecutionService` will hold its own `Arc` clones of the same
  `SecretStore`/`VaultSecretFetcher` instances).

- **`DomainResult` everywhere.** All fallible methods return
  `rocket_shared::error::DomainResult<T>`.

- **"Not found" error convention.** This codebase's existing pattern for a
  missing entity, confirmed by grepping `crates/rocket-app/src/*.rs`
  (`environment_service.rs`, `collection_service.rs`, `history_service.rs`,
  `template_service.rs`), is
  `.ok_or_else(|| DomainError::NotFound(id.into()))` — use that exact
  pattern for an unknown connection id. `DomainError`'s actual variants
  (confirmed from `crates/rocket-shared/src/error.rs`) are `NotFound`,
  `InvalidInput`, `AlreadyExists`, `Io`, `Serialization`, `Http`, `Internal`,
  `Conflict`, plus the SSH/TLS verification variants — use `NotFound` for a
  missing connection and `Internal` for a missing stored client secret (a
  connection that exists in the repo but has no keychain entry is an
  inconsistent-state bug, not a normal "not found", matching the wording
  already used for this exact case in the locked interface contract).

- **Tests use inline mocks.** Per `crates/rocket-app/CLAUDE.md`: "Each service
  module contains its own mock implementations in `#[cfg(test)]`." Write
  minimal fakes for `SecretManagerRepository`, `SecretStore`, and
  `VaultSecretFetcher` inline in each task's test module — do not pull in a
  mocking crate.
- Async trait mocks use `#[async_trait]` on the `impl` block exactly like
  `MockExecutor`'s `impl HttpExecutor` in
  `crates/rocket-app/src/execution_service.rs`; async tests use
  `#[tokio::test]`.
- Test code uses `.expect("message")` for fallible calls, never the bare
  panicking shorthand, matching every earlier plan in this series.
- **No bare panicking-unwrap calls anywhere in this file or in any code it
  specifies** — use `.expect("message")` in tests, and proper
  `DomainResult`/`?`/`ok_or_else` propagation in production code.

---

## Task 1: `SecretManagerService` CRUD

**Files:**
- Create: `crates/rocket-app/src/secret_manager_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `SecretManagerConnection`/`SecretManagerRepository` (Plan 01),
  `SecretStore` (existing), `VaultSecretFetcher` (Plan 02) — construction
  only in this task; `VaultSecretFetcher` is not called until Task 2.
- Produces: `SecretManagerService::{new, list, save, delete}` — consumed by
  Task 2 of this plan (adds `test_connection`/`fetch_secret_names` to the
  same struct) and by Plan 08 (Tauri commands wrap all five methods).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/secret_manager_service.rs
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_shared::error::{DomainError, DomainResult};
use std::sync::Arc;

/// scope_id under which every vault connection's client_secret is stored in
/// the injected SecretStore. All connections share this one scope because
/// `key` (the connection's `id`) already uniquely identifies each one within
/// it — see this plan's Global Constraints for why that differs from
/// environment-secret scoping.
const VAULT_CONNECTION_SCOPE: &str = "vault-connection";

pub struct SecretManagerService {
    repo: Box<dyn SecretManagerRepository>,
    secret_store: Arc<dyn SecretStore>,
    fetcher: Arc<dyn VaultSecretFetcher>,
}

impl SecretManagerService {
    pub fn new(
        repo: Box<dyn SecretManagerRepository>,
        secret_store: Arc<dyn SecretStore>,
        fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self {
        Self {
            repo,
            secret_store,
            fetcher,
        }
    }

    pub fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        self.repo.list()
    }

    // Implemented in Step 3.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeRepo(Mutex<Vec<SecretManagerConnection>>);

    impl FakeRepo {
        fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }
    }

    impl SecretManagerRepository for FakeRepo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            Ok(self.0.lock().expect("lock FakeRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|c| c.id != connection.id);
            guard.push(connection.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0.lock().expect("lock FakeRepo").retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretStore {
        entries: Mutex<std::collections::HashMap<(String, String), String>>,
        fail_next_set: std::sync::atomic::AtomicBool,
    }

    impl FakeSecretStore {
        fn new() -> Self {
            Self {
                entries: Mutex::new(std::collections::HashMap::new()),
                fail_next_set: std::sync::atomic::AtomicBool::new(false),
            }
        }
        fn fail_next_set(&self) {
            self.fail_next_set
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    impl SecretStore for FakeSecretStore {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .entries
                .lock()
                .expect("lock FakeSecretStore")
                .get(&(scope_id.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            if self
                .fail_next_set
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(DomainError::Internal("keychain write failed".to_string()));
            }
            self.entries.lock().expect("lock FakeSecretStore").insert(
                (scope_id.to_string(), key.to_string()),
                value.to_string(),
            );
            Ok(())
        }
        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.entries
                .lock()
                .expect("lock FakeSecretStore")
                .remove(&(scope_id.to_string(), key.to_string()));
            Ok(())
        }
    }

    struct FakeFetcher;

    #[async_trait::async_trait]
    impl VaultSecretFetcher for FakeFetcher {
        async fn list_secrets(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(Vec::new())
        }
        async fn get_secret_value(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            _secret_id: &str,
        ) -> DomainResult<Option<String>> {
            Ok(None)
        }
        async fn test_connection(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    fn sample_connection(id: &str) -> SecretManagerConnection {
        SecretManagerConnection {
            id: id.to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    #[test]
    fn save_with_secret_stores_both_connection_and_keychain_entry() {
        let repo = FakeRepo::new();
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let conn = sample_connection("conn-1");

        service
            .save(conn.clone(), Some("shh-its-a-secret".to_string()))
            .expect("save with secret");

        let listed = service.list().expect("list connections");
        assert_eq!(listed, vec![conn]);
        assert_eq!(
            store
                .get("vault-connection", "conn-1")
                .expect("get keychain entry"),
            Some("shh-its-a-secret".to_string())
        );
    }

    #[test]
    fn save_without_secret_leaves_existing_keychain_entry_untouched() {
        let repo = FakeRepo::new();
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let mut conn = sample_connection("conn-1");
        service
            .save(conn.clone(), Some("original-secret".to_string()))
            .expect("initial save with secret");

        conn.label = "Renamed RocketVault".to_string();
        service.save(conn.clone(), None).expect("edit without resecret");

        let listed = service.list().expect("list connections");
        assert_eq!(listed, vec![conn]);
        assert_eq!(
            store
                .get("vault-connection", "conn-1")
                .expect("get keychain entry"),
            Some("original-secret".to_string()),
            "keychain entry must be untouched by a client_secret: None save"
        );
    }

    #[test]
    fn delete_removes_both_connection_and_keychain_entry() {
        let repo = FakeRepo::new();
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let conn = sample_connection("conn-1");
        service
            .save(conn, Some("shh-its-a-secret".to_string()))
            .expect("save with secret");

        service.delete("conn-1").expect("delete connection");

        assert!(service.list().expect("list connections").is_empty());
        assert_eq!(
            store
                .get("vault-connection", "conn-1")
                .expect("get keychain entry after delete"),
            None
        );
    }

    #[test]
    fn keychain_set_failure_during_save_prevents_connection_persistence() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store.fail_next_set();
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            Arc::new(FakeFetcher),
        );
        let conn = sample_connection("conn-1");

        let result = service.save(conn, Some("shh-its-a-secret".to_string()));

        assert!(result.is_err(), "expected keychain failure to surface as an error");
        assert!(
            service.list().expect("list connections").is_empty(),
            "connection record must not be persisted when the keychain write fails"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app secret_manager_service::tests`
Expected: FAIL to compile — `secret_manager_service` module is not yet
registered in `lib.rs`, and `save`/`delete` do not exist on
`SecretManagerService` yet.

- [ ] **Step 3: Implement `save` and `delete`**

Add to `impl SecretManagerService` (after `list`):

```rust
    /// `client_secret: Some(s)` sets/overwrites the keychain entry for this
    /// connection before persisting the connection record — if the keychain
    /// write fails, the connection record is never saved, so we never end up
    /// with a connection pointing at a secret that was never actually
    /// stored. `client_secret: None` is an edit that does not change the
    /// secret (e.g. relabeling a connection) — the keychain write is skipped
    /// entirely and only the connection record is saved.
    pub fn save(
        &self,
        connection: SecretManagerConnection,
        client_secret: Option<String>,
    ) -> DomainResult<()> {
        if let Some(secret) = client_secret {
            self.secret_store
                .set(VAULT_CONNECTION_SCOPE, &connection.id, &secret)?;
        }
        self.repo.save(&connection)
    }

    /// Deletes the connection record, then best-effort deletes its keychain
    /// entry. A keychain delete failure is logged and ignored rather than
    /// failing the whole delete: a stale keychain entry for a connection
    /// that no longer exists is not a confidentiality problem (nothing can
    /// look it up without the connection id, which is already gone from the
    /// repo), mirroring the existing `KeyringSecretStore`/hardening-spec
    /// precedent that delete-path keychain failures are non-fatal.
    pub fn delete(&self, id: &str) -> DomainResult<()> {
        self.repo.delete(id)?;
        if let Err(err) = self.secret_store.delete(VAULT_CONNECTION_SCOPE, id) {
            eprintln!("warning: failed to delete keychain entry for vault connection {id}: {err}");
        }
        Ok(())
    }
```

- [ ] **Step 4: Register the module**

In `crates/rocket-app/src/lib.rs`, add the module declaration and re-export
alongside the existing ones (match the existing alphabetical ordering in that
file):

```rust
pub mod secret_manager_service;
```

```rust
pub use secret_manager_service::SecretManagerService;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-app secret_manager_service::tests`
Expected: PASS — 4 tests.

Also run: `cargo check -p rocket-app` to confirm the crate as a whole still
compiles (this module now depends on `rocket_environment::vault_secret_fetcher`,
which only exists once Plan 02 has landed — if this plan is being run out of
order, stop and implement Plan 02 first).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/secret_manager_service.rs crates/rocket-app/src/lib.rs
git commit -m "feat(app): add SecretManagerService CRUD"
```

---

## Task 2: `test_connection` and `fetch_secret_names`

**Files:**
- Modify: `crates/rocket-app/src/secret_manager_service.rs`

**Interfaces:**
- Consumes: `VaultSecretFetcher::{test_connection, list_secrets}` (Plan 02),
  `SecretManagerRepository::get` (Plan 01), `SecretStore::get` (existing).
- Produces: `SecretManagerService::{test_connection, fetch_secret_names}` —
  consumed by Plan 08 (Tauri commands `test_vault_connection`/
  `fetch_vault_secret_names` or equivalent).

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block (needs a configurable fetcher
fake, since these tests assert on what `SecretManagerService` passes
through — add this alongside the Task 1 `FakeFetcher`, do not replace it,
since Task 1's tests still rely on it):

```rust
    struct ConfigurableFakeFetcher {
        list_result: DomainResult<Vec<ExternalSecretRef>>,
        test_result: DomainResult<()>,
    }

    // DomainError derives PartialEq but not Clone — this test-only helper
    // reconstructs an equivalent error by matching on the variants this
    // module's tests actually produce, so a fake can be configured with a
    // canned Err(..) and still return that error from every call.
    fn clone_domain_error(err: &DomainError) -> DomainError {
        match err {
            DomainError::NotFound(msg) => DomainError::NotFound(msg.clone()),
            DomainError::Internal(msg) => DomainError::Internal(msg.clone()),
            other => DomainError::Internal(other.to_string()),
        }
    }

    #[async_trait::async_trait]
    impl VaultSecretFetcher for ConfigurableFakeFetcher {
        async fn list_secrets(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            match &self.list_result {
                Ok(refs) => Ok(refs.clone()),
                Err(err) => Err(clone_domain_error(err)),
            }
        }
        async fn get_secret_value(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            _secret_id: &str,
        ) -> DomainResult<Option<String>> {
            Ok(None)
        }
        async fn test_connection(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            match &self.test_result {
                Ok(()) => Ok(()),
                Err(err) => Err(clone_domain_error(err)),
            }
        }
    }

    #[tokio::test]
    async fn test_connection_succeeds_when_fetcher_succeeds() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
            }),
        );

        service
            .test_connection("conn-1", "prod-vault")
            .await
            .expect("test_connection should succeed");
    }

    #[tokio::test]
    async fn fetch_secret_names_returns_fetcher_output_unchanged() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let expected = vec![ExternalSecretRef {
            name: "stripe-key".to_string(),
            secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
        }];
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(expected.clone()),
                test_result: Ok(()),
            }),
        );

        let names = service
            .fetch_secret_names("conn-1", "prod-vault")
            .await
            .expect("fetch_secret_names should succeed");

        assert_eq!(names, expected);
    }

    #[tokio::test]
    async fn connection_id_with_no_stored_secret_errors_clearly() {
        let repo = FakeRepo::new();
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(FakeSecretStore::new()), // no keychain entry seeded
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
            }),
        );

        let result = service.test_connection("conn-1", "prod-vault").await;

        assert!(
            matches!(result, Err(DomainError::Internal(_))),
            "expected DomainError::Internal for a connection with no stored secret, got {result:?}"
        );
    }

    #[tokio::test]
    async fn unknown_connection_id_errors_clearly() {
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
            }),
        );

        let result = service.fetch_secret_names("no-such-conn", "prod-vault").await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "expected DomainError::NotFound for an unknown connection id, got {result:?}"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app secret_manager_service::tests`
Expected: FAIL to compile — `test_connection`/`fetch_secret_names` do not
exist on `SecretManagerService` yet.

- [ ] **Step 3: Implement `test_connection` and `fetch_secret_names`**

Add to `impl SecretManagerService` (after `delete`):

```rust
    async fn connection_and_secret(
        &self,
        id: &str,
    ) -> DomainResult<(SecretManagerConnection, String)> {
        let connection = self
            .repo
            .get(id)?
            .ok_or_else(|| DomainError::NotFound(id.to_string()))?;
        let secret = self
            .secret_store
            .get(VAULT_CONNECTION_SCOPE, id)?
            .ok_or_else(|| {
                DomainError::Internal("connection has no stored client secret".to_string())
            })?;
        Ok((connection, secret))
    }

    pub async fn test_connection(&self, id: &str, vault_name: &str) -> DomainResult<()> {
        let (connection, secret) = self.connection_and_secret(id).await?;
        self.fetcher
            .test_connection(&connection, &secret, vault_name)
            .await
    }

    pub async fn fetch_secret_names(
        &self,
        id: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        let (connection, secret) = self.connection_and_secret(id).await?;
        self.fetcher.list_secrets(&connection, &secret, vault_name).await
    }
```

`connection_and_secret` is a private helper shared by both public methods —
it is not part of this plan's public interface contract and may be named
differently by the implementer as long as both public methods look up the
connection then the secret in that order and surface the same two error
cases.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app secret_manager_service::tests`
Expected: PASS — 8 tests total (4 from Task 1, 4 from this task).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/secret_manager_service.rs
git commit -m "feat(app): add SecretManagerService test_connection/fetch_secret_names"
```

---

## Task 3: Shared `resolve_vault_secret_value` helper

**Files:**
- Create: `crates/rocket-app/src/vault_secret_resolution.rs`
- Modify: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `SecretManagerRepository::get` (Plan 01), `SecretStore::get`
  (existing), `VaultSecretFetcher::get_secret_value` (Plan 02).
- Produces: `resolve_vault_secret_value(...)` — consumed by Plan 06's
  `RequestExecutionService::resolve_external_secrets`, which calls this
  function once per `{{alias.secretName}}` binding it needs to resolve.

This is a **free function**, not a method on `SecretManagerService` — see
this plan's Architecture note above for why. It takes borrowed trait objects
(`&dyn Trait`, not `Box`/`Arc`) so callers that already hold an `Arc<dyn
SecretStore>`/`Arc<dyn VaultSecretFetcher>` (such as
`RequestExecutionService` in Plan 06) can pass `secret_store.as_ref()` /
`fetcher.as_ref()` without cloning the `Arc` or restructuring ownership.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/vault_secret_resolution.rs
use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_shared::error::{DomainError, DomainResult};

const VAULT_CONNECTION_SCOPE: &str = "vault-connection";

/// Resolves one `{{alias.secretName}}` binding to its live value by looking
/// up the connection, its stored client_secret, then calling the fetcher.
/// Shared between `SecretManagerService`-adjacent callers (this plan) and
/// `RequestExecutionService` (Plan 06) so both go through one code path
/// rather than duplicating this three-step lookup.
///
/// A fetcher `Ok(None)` (the secret was deleted from the vault after the
/// binding was created) passes through as `Ok(None)` unchanged — this
/// function does not decide whether that is a hard failure. Per spec §4.6,
/// that decision belongs to Plan 06's caller, which knows whether it is
/// resolving for a live request (hard-fail) or a background/advisory check.
pub async fn resolve_vault_secret_value(
    repo: &dyn SecretManagerRepository,
    secret_store: &dyn SecretStore,
    fetcher: &dyn VaultSecretFetcher,
    connection_id: &str,
    vault_name: &str,
    secret_id: &str,
) -> DomainResult<Option<String>> {
    let connection = repo
        .get(connection_id)?
        .ok_or_else(|| DomainError::NotFound(connection_id.to_string()))?;
    let client_secret = secret_store
        .get(VAULT_CONNECTION_SCOPE, connection_id)?
        .ok_or_else(|| {
            DomainError::Internal("connection has no stored client secret".to_string())
        })?;
    fetcher
        .get_secret_value(&connection, &client_secret, vault_name, secret_id)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::external_secret::ExternalSecretRef;
    use std::sync::Mutex;

    struct FakeRepo(Mutex<Vec<SecretManagerConnection>>);

    impl SecretManagerRepository for FakeRepo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            Ok(self.0.lock().expect("lock FakeRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|c| c.id != connection.id);
            guard.push(connection.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0.lock().expect("lock FakeRepo").retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretStore(Mutex<std::collections::HashMap<(String, String), String>>);

    impl SecretStore for FakeSecretStore {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeSecretStore")
                .get(&(scope_id.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            self.0.lock().expect("lock FakeSecretStore").insert(
                (scope_id.to_string(), key.to_string()),
                value.to_string(),
            );
            Ok(())
        }
        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeSecretStore")
                .remove(&(scope_id.to_string(), key.to_string()));
            Ok(())
        }
    }

    struct FakeFetcher {
        value_result: DomainResult<Option<String>>,
    }

    #[async_trait::async_trait]
    impl VaultSecretFetcher for FakeFetcher {
        async fn list_secrets(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(Vec::new())
        }
        async fn get_secret_value(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            _secret_id: &str,
        ) -> DomainResult<Option<String>> {
            match &self.value_result {
                Ok(value) => Ok(value.clone()),
                Err(DomainError::Internal(msg)) => Err(DomainError::Internal(msg.clone())),
                Err(other) => Err(DomainError::Internal(other.to_string())),
            }
        }
        async fn test_connection(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    fn sample_connection(id: &str) -> SecretManagerConnection {
        SecretManagerConnection {
            id: id.to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    #[tokio::test]
    async fn successful_resolution_returns_the_value() {
        let repo = FakeRepo(Mutex::new(vec![sample_connection("conn-1")]));
        let store = FakeSecretStore(Mutex::new(std::collections::HashMap::new()));
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        let fetcher = FakeFetcher {
            value_result: Ok(Some("sk-live-abc123".to_string())),
        };

        let result = resolve_vault_secret_value(
            &repo,
            &store,
            &fetcher,
            "conn-1",
            "prod-vault",
            "secret-id-1",
        )
        .await
        .expect("resolve_vault_secret_value should succeed");

        assert_eq!(result, Some("sk-live-abc123".to_string()));
    }

    #[tokio::test]
    async fn missing_connection_errors() {
        let repo = FakeRepo(Mutex::new(Vec::new()));
        let store = FakeSecretStore(Mutex::new(std::collections::HashMap::new()));
        let fetcher = FakeFetcher {
            value_result: Ok(Some("unused".to_string())),
        };

        let result = resolve_vault_secret_value(
            &repo,
            &store,
            &fetcher,
            "no-such-conn",
            "prod-vault",
            "secret-id-1",
        )
        .await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "expected DomainError::NotFound for a missing connection, got {result:?}"
        );
    }

    #[tokio::test]
    async fn missing_stored_secret_errors() {
        let repo = FakeRepo(Mutex::new(vec![sample_connection("conn-1")]));
        let store = FakeSecretStore(Mutex::new(std::collections::HashMap::new())); // no keychain entry
        let fetcher = FakeFetcher {
            value_result: Ok(Some("unused".to_string())),
        };

        let result = resolve_vault_secret_value(
            &repo,
            &store,
            &fetcher,
            "conn-1",
            "prod-vault",
            "secret-id-1",
        )
        .await;

        assert!(
            matches!(result, Err(DomainError::Internal(_))),
            "expected DomainError::Internal for a connection with no stored secret, got {result:?}"
        );
    }

    #[tokio::test]
    async fn fetcher_returning_none_passes_through_unchanged() {
        let repo = FakeRepo(Mutex::new(vec![sample_connection("conn-1")]));
        let store = FakeSecretStore(Mutex::new(std::collections::HashMap::new()));
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        let fetcher = FakeFetcher { value_result: Ok(None) };

        let result = resolve_vault_secret_value(
            &repo,
            &store,
            &fetcher,
            "conn-1",
            "prod-vault",
            "deleted-secret-id",
        )
        .await
        .expect("a fetcher Ok(None) must not be turned into an error by this helper");

        assert_eq!(
            result, None,
            "Ok(None) from the fetcher (secret deleted from the vault) must pass through as Ok(None), not an error"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app vault_secret_resolution::tests`
Expected: FAIL — module is not yet registered in `lib.rs`.

- [ ] **Step 3: Register the module**

In `crates/rocket-app/src/lib.rs`, add the module declaration and re-export
alongside the existing ones:

```rust
pub mod vault_secret_resolution;
```

```rust
pub use vault_secret_resolution::resolve_vault_secret_value;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app vault_secret_resolution::tests`
Expected: PASS — 4 tests.

Also run: `cargo check -p rocket-app` and `cargo test -p rocket-app` (full
crate) to confirm nothing in Task 1/2's module was broken by this addition.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/vault_secret_resolution.rs crates/rocket-app/src/lib.rs
git commit -m "feat(app): add shared resolve_vault_secret_value helper"
```

---

## Milestone Checklist — Plan 05

- [ ] `SecretManagerService::new/list/save/delete` — trait-object injection (`Box<dyn SecretManagerRepository>`, `Arc<dyn SecretStore>`, `Arc<dyn VaultSecretFetcher>`), matching every other `rocket-app` service's construction shape
- [ ] `save` writes the keychain entry (when `client_secret` is `Some`) before the connection record, and fails the whole save if the keychain write fails
- [ ] `save` with `client_secret: None` leaves any existing keychain entry untouched (edit-without-resecret)
- [ ] `delete` removes the connection record, then best-effort deletes the keychain entry (log-and-ignore on failure)
- [ ] `test_connection`/`fetch_secret_names` — look up connection then stored secret, in that order, surfacing `NotFound` vs. `Internal` distinctly; call through to the injected `VaultSecretFetcher` unchanged
- [ ] `resolve_vault_secret_value` free function in its own module, taking borrowed trait objects, matching the locked signature in the plan index exactly
- [ ] `resolve_vault_secret_value` passes a fetcher `Ok(None)` through as `Ok(None)` — the one case that is not an error at this layer
- [ ] All new code uses `"vault-connection"` as the fixed `SecretStore` scope_id, connection `id` as the key
- [ ] `cargo test -p rocket-app` — all pass, including pre-existing tests in other modules
- [ ] `cargo check -p rocket-app` — clean

## Next Plan

[Plan 06: RequestExecutionService external-secret resolution](2026-09-22-rocketvault-secrets-plan-06-execution-service.md) —
consumes this plan's `resolve_vault_secret_value` helper directly (calling it
once per `{{alias.secretName}}` binding inside its new
`resolve_external_secrets` method), and adds the `external_secrets:
&HashMap<String, String>` parameter threading described in the plan index's
`rocket-app (modified, Plan 06/07)` section.
