# ACP Agent Config Plan 03: App Service — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `SecretManagerService::resolve_secret_value` (a small addition
to the existing service) and the new `AgentConfigService`, which orchestrates
`AgentConfig` CRUD plus resolving an agent's credential from RocketVault.

**Architecture:** `AgentConfigService` holds `Box<dyn AgentConfigRepository>`
plus `Arc<SecretManagerService>` — it does not talk to `SecretStore` or
`VaultSecretFetcher` directly, reusing `SecretManagerService`'s existing
connection+keychain lookup instead of duplicating it. This mirrors this
crate's trait-object-injection pattern throughout.

**Tech Stack:** Rust, tokio (async tests), the `which` crate (new dependency).

**Spec:** `docs/superpowers/specs/2026-09-27-acp-agent-config-credentials-design.md`
(Credential Resolution section). Plan index:
`docs/superpowers/plans/acp-agent-config-credentials/00-plan-index.md`.

## Global Constraints

- `resolve_secret_value` is added to the *existing*
  `crates/rocket-app/src/secret_manager_service.rs` — do not create a
  parallel helper elsewhere; `AgentConfigService` must call this method, not
  reimplement connection/keychain lookup itself.
- `AgentConfigService::save` validates non-empty `label`, `command`,
  `credential_env_var`, `vault_secret_id`, and that `vault_connection_id`
  resolves against `secret_manager.list()` — but does **not** call out to
  RocketVault to confirm the secret itself still exists (that is discovered
  on use, in `resolve_credential`, matching this repo's existing pattern for
  external-secret staleness).
- `resolve_credential` must map a `None` result from `resolve_secret_value`
  (the vault secret no longer exists) to `DomainError::NotFound`, not treat it
  as a silently-empty value — an agent cannot be launched without a
  credential, so this must be a hard, actionable error.
- Add `which.workspace = true` is not available (no workspace entry exists
  yet) — add `which = "6"` directly to `crates/rocket-app/Cargo.toml`'s
  `[dependencies]` section.
- Test code uses `.expect("message")` for fallible calls, never the bare
  panicking shorthand. Every service module in this crate defines its own
  inline test doubles rather than sharing a mocking library — follow that
  convention here too.

## Review Focus

- `save` called with a `vault_connection_id` that does not exist in
  `SecretManagerService` must be rejected at save time with
  `DomainError::InvalidInput` — accepting it silently would only surface as a
  confusing failure much later, at agent-launch time.
- `resolve_credential` when the underlying `SecretManagerConnection` was
  deleted after the `AgentConfig` was saved (a dangling reference) — confirm
  the error from `SecretManagerService`'s internal connection lookup
  propagates through `resolve_secret_value` and `resolve_credential` rather
  than being swallowed.
- A keychain/vault transport failure while resolving a credential must
  surface as an actionable `DomainError`, not panic — exercise this alongside
  the "secret genuinely doesn't exist" case, since they are different failure
  modes with the same symptom (no usable credential).
- `test_agent_config` on a config whose command resolves but whose credential
  is stale must still fail — two independent checks, both must be exercised
  by a test, not just one.
- Concurrent `save`/`delete` calls on the same `AgentConfig.id` are out of
  scope for this service — thread-safety of the underlying store is
  `FsAgentConfigRepo`'s concern (Plan 02), not this orchestration layer. No
  test here should attempt to cover concurrent access; a reviewer should not
  flag its absence.

---

## Task 1: `SecretManagerService::resolve_secret_value`

**Files:**
- Modify: `crates/rocket-app/src/secret_manager_service.rs`

**Interfaces:**
- Consumes: existing `SecretManagerService::connection_and_secret` (private,
  already defined at `crates/rocket-app/src/secret_manager_service.rs:91-105`)
  and the existing `fetcher: Arc<dyn VaultSecretFetcher>` field.
- Produces: `SecretManagerService::resolve_secret_value(&self, connection_id:
  &str, vault_name: &str, secret_id: &str) -> DomainResult<Option<String>>` —
  consumed by Task 3 of this plan (`AgentConfigService::resolve_credential`).

- [ ] **Step 1: Extend the existing `ConfigurableFakeFetcher` test double**

In `crates/rocket-app/src/secret_manager_service.rs`, the `tests` module
already defines `ConfigurableFakeFetcher` (currently lines 276-326). Add a
third field so a test can configure what `get_secret_value` returns without
touching the other two fields' existing behavior:

```rust
// Existing struct at line ~276 — add the new field:
struct ConfigurableFakeFetcher {
    list_result: DomainResult<Vec<ExternalSecretRef>>,
    test_result: DomainResult<()>,
    secret_value_result: DomainResult<Option<String>>,
}
```

Update its `get_secret_value` impl (currently lines ~306-314, which
unconditionally returns `Ok(None)`) to:

```rust
async fn get_secret_value(
    &self,
    _connection: &SecretManagerConnection,
    _client_secret: &str,
    _vault_name: &str,
    _secret_id: &str,
) -> DomainResult<Option<String>> {
    match &self.secret_value_result {
        Ok(v) => Ok(v.clone()),
        Err(err) => Err(clone_domain_error(err)),
    }
}
```

Then add `secret_value_result: Ok(None),` to each of this file's four
existing `ConfigurableFakeFetcher { ... }` struct literals, so the compiler's
"missing field" error is fixed for all of them (they are in
`test_connection_succeeds_when_fetcher_succeeds`,
`fetch_secret_names_returns_fetcher_output_unchanged`,
`connection_id_with_no_stored_secret_errors_clearly`, and
`unknown_connection_id_errors_clearly` — the compiler will point at each
one after Step 1's struct change).

Run: `cargo check -p rocket-app --tests -j4`
Expected: FAIL — four "missing field `secret_value_result`" errors, one per
call site above. Fix each with `secret_value_result: Ok(None),` before
continuing (this preserves every existing test's prior behavior, since none
of them exercised `get_secret_value`).

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-app/src/secret_manager_service.rs (add to the existing tests module)

#[tokio::test]
async fn resolve_secret_value_returns_fetcher_result_unchanged() {
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
            secret_value_result: Ok(Some("sk-abc123".to_string())),
        }),
    );

    let value = service
        .resolve_secret_value("conn-1", "prod-vault", "secret-id-1")
        .await
        .expect("resolve_secret_value should succeed");

    assert_eq!(value, Some("sk-abc123".to_string()));
}

#[tokio::test]
async fn resolve_secret_value_unknown_connection_errors() {
    let service = SecretManagerService::new(
        Box::new(FakeRepo::new()),
        Arc::new(FakeSecretStore::new()),
        Arc::new(ConfigurableFakeFetcher {
            list_result: Ok(Vec::new()),
            test_result: Ok(()),
            secret_value_result: Ok(None),
        }),
    );

    let result = service
        .resolve_secret_value("no-such-conn", "prod-vault", "secret-id-1")
        .await;

    assert!(
        matches!(result, Err(DomainError::NotFound(_))),
        "expected DomainError::NotFound for an unknown connection id, got {result:?}"
    );
}

#[tokio::test]
async fn resolve_secret_value_propagates_transport_failure() {
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
            secret_value_result: Err(DomainError::Internal("network timeout".to_string())),
        }),
    );

    let result = service
        .resolve_secret_value("conn-1", "prod-vault", "secret-id-1")
        .await;

    assert!(
        matches!(result, Err(DomainError::Internal(_))),
        "a transport failure must surface as DomainError::Internal, not be swallowed, got {result:?}"
    );
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-app secret_manager_service::tests -j4`
Expected: FAIL — `resolve_secret_value` method does not exist yet (compile
error).

- [ ] **Step 4: Implement the method**

```rust
// crates/rocket-app/src/secret_manager_service.rs — add inside `impl SecretManagerService`,
// after the existing `fetch_secret_names` method.

/// Fetches one secret's raw value directly, without going through the
/// per-environment `ExternalSecretBinding`/alias flow — for callers (like
/// `AgentConfigService`) that need an app-global credential rather than a
/// value bound to a specific `Environment`.
pub async fn resolve_secret_value(
    &self,
    connection_id: &str,
    vault_name: &str,
    secret_id: &str,
) -> DomainResult<Option<String>> {
    let (connection, client_secret) = self.connection_and_secret(connection_id)?;
    self.fetcher
        .get_secret_value(&connection, &client_secret, vault_name, secret_id)
        .await
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-app secret_manager_service::tests -j4`
Expected: PASS — all tests in this module, including the 3 new ones.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/secret_manager_service.rs
git commit -m "feat(app): add SecretManagerService::resolve_secret_value"
```

---

## Task 2: `AgentConfigService` CRUD

**Files:**
- Create: `crates/rocket-app/src/agent_config_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Modify: `crates/rocket-app/Cargo.toml`

**Interfaces:**
- Consumes: `AgentConfig`, `AgentConfigRepository` (`rocket-acp`, Plan 01);
  `SecretManagerService::list` (existing).
- Produces: `AgentConfigService::new`, `list`, `save`, `delete` — consumed by
  Task 3 of this plan and by Plan 04's Tauri commands.

- [ ] **Step 1: Add the `rocket-acp` dependency**

In `crates/rocket-app/Cargo.toml`, add `rocket-acp = { path = "../rocket-acp"
}` to `[dependencies]`, alongside the existing `rocket-scripting = { path =
"../rocket-scripting" }` line (this crate uses explicit relative `path =`
entries for some internal crates rather than `.workspace = true` — match that
existing style here).

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-app/src/agent_config_service.rs
use std::sync::{Arc, Mutex};

use rocket_acp::{AgentConfig, AgentConfigRepository};
use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_shared::error::{DomainError, DomainResult};

use crate::secret_manager_service::SecretManagerService;

pub struct AgentConfigService {
    repo: Box<dyn AgentConfigRepository>,
    secret_manager: Arc<SecretManagerService>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::external_secret::ExternalSecretRef;

    struct FakeAgentConfigRepo(Mutex<Vec<AgentConfig>>);
    impl FakeAgentConfigRepo {
        fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }
    }
    impl AgentConfigRepository for FakeAgentConfigRepo {
        fn list(&self) -> DomainResult<Vec<AgentConfig>> {
            Ok(self.0.lock().expect("lock FakeAgentConfigRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeAgentConfigRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, config: &AgentConfig) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeAgentConfigRepo");
            guard.retain(|c| c.id != config.id);
            guard.push(config.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeAgentConfigRepo")
                .retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretManagerRepo(Mutex<Vec<SecretManagerConnection>>);
    impl SecretManagerRepository for FakeSecretManagerRepo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            Ok(self.0.lock().expect("lock FakeSecretManagerRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeSecretManagerRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeSecretManagerRepo");
            guard.retain(|c| c.id != connection.id);
            guard.push(connection.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeSecretManagerRepo")
                .retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretStore;
    impl SecretStore for FakeSecretStore {
        fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
            Ok(Some("shh-its-a-secret".to_string()))
        }
        fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    struct FakeVaultFetcher {
        secret_value_result: DomainResult<Option<String>>,
    }
    #[async_trait::async_trait]
    impl VaultSecretFetcher for FakeVaultFetcher {
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
            match &self.secret_value_result {
                Ok(v) => Ok(v.clone()),
                Err(DomainError::Internal(msg)) => Err(DomainError::Internal(msg.clone())),
                Err(_) => Err(DomainError::Internal("fake fetcher error".to_string())),
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

    fn sample_config(id: &str, vault_connection_id: &str) -> AgentConfig {
        AgentConfig {
            id: id.to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: Vec::new(),
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            vault_connection_id: vault_connection_id.to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "secret-id-1".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    fn service_with(
        secret_value_result: DomainResult<Option<String>>,
        seed_connection: bool,
    ) -> AgentConfigService {
        let sm_repo = FakeSecretManagerRepo(Mutex::new(if seed_connection {
            vec![sample_connection("conn-1")]
        } else {
            Vec::new()
        }));
        let secret_manager = Arc::new(SecretManagerService::new(
            Box::new(sm_repo),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher {
                secret_value_result,
            }),
        ));
        AgentConfigService::new(Box::new(FakeAgentConfigRepo::new()), secret_manager)
    }

    #[test]
    fn list_delegates_to_repo() {
        let service = service_with(Ok(None), true);
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save");
        assert_eq!(service.list().expect("list").len(), 1);
    }

    #[test]
    fn save_rejects_blank_required_fields() {
        let service = service_with(Ok(None), true);
        let mut blank_label = sample_config("agent-1", "conn-1");
        blank_label.label = "  ".to_string();
        let err = service.save(blank_label).expect_err("must reject blank label");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn save_rejects_unknown_vault_connection_id() {
        let service = service_with(Ok(None), false);
        let err = service
            .save(sample_config("agent-1", "no-such-conn"))
            .expect_err("must reject unknown vault_connection_id");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn delete_delegates_to_repo() {
        let service = service_with(Ok(None), true);
        service
            .save(sample_config("agent-1", "conn-1"))
            .expect("save");
        service.delete("agent-1").expect("delete");
        assert!(service.list().expect("list").is_empty());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-app agent_config_service::tests -j4`
Expected: FAIL — `AgentConfigService::new`/`list`/`save`/`delete` don't exist
yet (compile error).

- [ ] **Step 4: Implement the service**

```rust
// crates/rocket-app/src/agent_config_service.rs (add above the tests module)

impl AgentConfigService {
    pub fn new(repo: Box<dyn AgentConfigRepository>, secret_manager: Arc<SecretManagerService>) -> Self {
        Self {
            repo,
            secret_manager,
        }
    }

    pub fn list(&self) -> DomainResult<Vec<AgentConfig>> {
        self.repo.list()
    }

    pub fn save(&self, config: AgentConfig) -> DomainResult<()> {
        validate_config(&config)?;
        let known_connection = self
            .secret_manager
            .list()?
            .iter()
            .any(|c| c.id == config.vault_connection_id);
        if !known_connection {
            return Err(DomainError::InvalidInput(format!(
                "agent '{}': vault_connection_id '{}' does not match any configured RocketVault connection",
                config.label, config.vault_connection_id
            )));
        }
        self.repo.save(&config)
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        self.repo.delete(id)
    }
}

fn validate_config(config: &AgentConfig) -> DomainResult<()> {
    let required = [
        ("label", &config.label),
        ("command", &config.command),
        ("credential_env_var", &config.credential_env_var),
        ("vault_connection_id", &config.vault_connection_id),
        ("vault_name", &config.vault_name),
        ("vault_secret_id", &config.vault_secret_id),
    ];
    for (field, value) in required {
        if value.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "agent config {field} must not be empty"
            )));
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Register the module**

In `crates/rocket-app/src/lib.rs`, add alongside the existing `pub mod
secret_manager_service;` declaration:

```rust
pub mod agent_config_service;
pub use agent_config_service::AgentConfigService;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-app agent_config_service::tests -j4`
Expected: PASS — 4 tests.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app
git commit -m "feat(app): add AgentConfigService CRUD"
```

---

## Task 3: Credential resolution and test action

**Files:**
- Modify: `crates/rocket-app/src/agent_config_service.rs`
- Modify: `crates/rocket-app/Cargo.toml`

**Interfaces:**
- Consumes: `AgentConfigService` (Task 2 of this plan);
  `SecretManagerService::resolve_secret_value` (Task 1 of this plan).
- Produces: `AgentConfigService::resolve_credential`,
  `AgentConfigService::test_agent_config` — consumed by Plan 04's
  `test_agent_config` Tauri command, and eventually by Plan B's ACP process
  spawn (out of scope for this series).

- [ ] **Step 1: Add the `which` dependency**

In `crates/rocket-app/Cargo.toml`, add to `[dependencies]`:

```toml
which = "6"
```

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-app/src/agent_config_service.rs (add to the existing tests module)

#[tokio::test]
async fn resolve_credential_returns_value_when_secret_exists() {
    let service = service_with(Ok(Some("sk-abc123".to_string())), true);
    service
        .save(sample_config("agent-1", "conn-1"))
        .expect("save");

    let value = service
        .resolve_credential("agent-1")
        .await
        .expect("resolve_credential should succeed");

    assert_eq!(value, "sk-abc123");
}

#[tokio::test]
async fn resolve_credential_errors_when_secret_stale() {
    let service = service_with(Ok(None), true);
    service
        .save(sample_config("agent-1", "conn-1"))
        .expect("save");

    let err = service
        .resolve_credential("agent-1")
        .await
        .expect_err("stale vault secret must be a hard error");

    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn resolve_credential_unknown_config_id_errors() {
    let service = service_with(Ok(None), true);
    let err = service
        .resolve_credential("no-such-agent")
        .await
        .expect_err("unknown agent config id must error");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn resolve_credential_errors_when_vault_connection_deleted_after_save() {
    // Built directly (not via service_with) so the test keeps its own handle
    // to the SecretManagerService and can delete the connection between
    // save() and resolve_credential(), simulating a dangling reference.
    let sm_repo = FakeSecretManagerRepo(Mutex::new(vec![sample_connection("conn-1")]));
    let secret_manager = Arc::new(SecretManagerService::new(
        Box::new(sm_repo),
        Arc::new(FakeSecretStore),
        Arc::new(FakeVaultFetcher {
            secret_value_result: Ok(Some("sk-abc123".to_string())),
        }),
    ));
    let service = AgentConfigService::new(Box::new(FakeAgentConfigRepo::new()), Arc::clone(&secret_manager));
    service
        .save(sample_config("agent-1", "conn-1"))
        .expect("save while connection still exists");

    secret_manager.delete("conn-1").expect("delete connection");

    let err = service
        .resolve_credential("agent-1")
        .await
        .expect_err("a dangling vault_connection_id must error, not panic");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn test_agent_config_fails_when_command_not_found() {
    let service = service_with(Ok(Some("sk-abc123".to_string())), true);
    let mut config = sample_config("agent-1", "conn-1");
    config.command = "definitely-not-a-real-binary-xyz123".to_string();
    service.save(config).expect("save");

    let err = service
        .test_agent_config("agent-1")
        .await
        .expect_err("nonexistent command must fail the test");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test]
async fn test_agent_config_fails_when_credential_stale_even_if_command_exists() {
    let service = service_with(Ok(None), true);
    let mut config = sample_config("agent-1", "conn-1");
    // env!("CARGO") is set by Cargo at build time to the exact cargo binary
    // running this test — a real, executable, cross-platform-safe path.
    config.command = env!("CARGO").to_string();
    service.save(config).expect("save");

    let err = service
        .test_agent_config("agent-1")
        .await
        .expect_err("stale credential must fail the test even though the command resolves");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn test_agent_config_succeeds_when_command_and_credential_resolve() {
    let service = service_with(Ok(Some("sk-abc123".to_string())), true);
    let mut config = sample_config("agent-1", "conn-1");
    config.command = env!("CARGO").to_string();
    service.save(config).expect("save");

    service
        .test_agent_config("agent-1")
        .await
        .expect("test_agent_config should succeed");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-app agent_config_service::tests -j4`
Expected: FAIL — `resolve_credential`/`test_agent_config` don't exist yet
(compile error).

- [ ] **Step 4: Implement both methods**

```rust
// crates/rocket-app/src/agent_config_service.rs — add inside `impl AgentConfigService`,
// after `delete`.

pub async fn resolve_credential(&self, id: &str) -> DomainResult<String> {
    let config = self
        .repo
        .get(id)?
        .ok_or_else(|| DomainError::NotFound(id.to_string()))?;
    let value = self
        .secret_manager
        .resolve_secret_value(&config.vault_connection_id, &config.vault_name, &config.vault_secret_id)
        .await?;
    value.ok_or_else(|| {
        DomainError::NotFound(format!(
            "agent '{}': credential no longer exists in RocketVault — reconfigure this agent",
            config.label
        ))
    })
}

pub async fn test_agent_config(&self, id: &str) -> DomainResult<()> {
    let config = self
        .repo
        .get(id)?
        .ok_or_else(|| DomainError::NotFound(id.to_string()))?;
    which::which(&config.command).map_err(|e| {
        DomainError::InvalidInput(format!(
            "agent '{}': command '{}' not found: {e}",
            config.label, config.command
        ))
    })?;
    self.resolve_credential(id).await?;
    Ok(())
}
```

Add `use rocket_shared::error::DomainError;` at the top of the file if it is
not already imported for this module (Task 2 already imports it for
`validate_config`, so this should already be present).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-app agent_config_service::tests -j4`
Expected: PASS — 11 tests total (4 from Task 2, 7 from this task).

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p rocket-app -j4`
Expected: PASS — confirms the `ConfigurableFakeFetcher` field addition in
Task 1 did not break any pre-existing test in this crate.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app
git commit -m "feat(app): add agent credential resolution and test action"
```

---

## Next Plan

[Plan 04: Tauri commands + service wiring](2026-09-27-acp-agent-config-credentials-plan-04-tauri-commands.md) —
exposes `AgentConfigService` over IPC and wires it into `src-tauri/src/lib.rs`
alongside the existing `secret_manager_svc`.

## Post-Implementation Review

Before starting Plan 04, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-app/Cargo.toml`, `crates/rocket-app/src/lib.rs`,
> `crates/rocket-app/src/secret_manager_service.rs`,
> `crates/rocket-app/src/agent_config_service.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — does `AgentConfigService`
>    expose exactly the methods (`new`, `list`, `save`, `delete`,
>    `resolve_credential`, `test_agent_config`) the plan index's locked
>    interface contract promises Plan 04 will consume, with matching
>    signatures?
> 2. Code quality — naming, error handling, and test coverage versus this
>    plan's Review Focus section (unknown vault_connection_id rejected at
>    save time, dangling-connection error propagation, transport-failure vs
>    stale-secret distinction, command-resolves-but-credential-stale case).
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    specifically that `AgentConfigService` reuses
>    `SecretManagerService::resolve_secret_value` rather than duplicating
>    connection/keychain lookup, and that no I/O beyond the injected trait
>    objects was introduced into this crate.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-app -j4` and
> `cargo check -p rocket-app -j4`, and confirm they still pass. Report what
> you found and fixed.

Only proceed to Plan 04 once this review comes back clean (or its fixes are
applied and re-verified).
