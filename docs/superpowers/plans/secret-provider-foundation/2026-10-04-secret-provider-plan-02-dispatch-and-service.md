# Secret provider foundation, Plan 02: Dispatching fetcher, service validation and IPC

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route every secret-manager call by `connection.provider`, make connection validation and the credential rule provider-aware, and carry `provider` and `config` over IPC.

**Architecture:** A `DispatchingSecretFetcher` in `rocket-infra` implements `VaultSecretFetcher` over a map of provider implementations and is injected once in `src-tauri/src/lib.rs`, so `rocket-app` still sees only the trait. `SecretManagerService` keeps RocketVault's rules and asks the fetcher for every other provider's rules and capabilities.

**Tech Stack:** Rust, async-trait, tokio tests, wiremock (existing RocketVault tests), Tauri commands.

**Spec:** [../../specs/2026-10-04-secret-provider-foundation-design.md](../../specs/2026-10-04-secret-provider-foundation-design.md) (sections 4.2, 5.2, 5.3, 8)

**Depends on:** [Plan 01](2026-10-04-secret-provider-plan-01-domain.md).

## Global Constraints

- IPC DTOs use `#[serde(rename_all = "camelCase")]`. Persistence structs never do.
- Production code never panics on bad input: use `DomainResult` and explicit error mapping, not unwrap. Tests may use `.expect("reason")`.
- Errors name the provider by `SecretProviderKind::display_name()`.
- `rocket-app` holds only `Arc<dyn VaultSecretFetcher>`. Provider selection lives in `rocket-infra`.
- Always pass `-j4` to `cargo test` and `cargo check`. Never run `cargo test --workspace`.
- Commit with conventional commits, using the `dev-workflow-skills:1-git-commit` skill, staging by explicit path only. Never stage `crates/rocket-app/src/execution_service.rs` in this plan.

## Review Focus

- A connection whose provider has no registered implementation must produce an error that names the provider, for every call, and must not fall back to RocketVault (Task 1 test).
- `forget_connection` must reach every registered provider, because only the connection id is known (Task 1 test).
- RocketVault must still require a stored client secret for a new connection, and its URL and client-id rules must be unchanged (Task 2 test).
- A provider that reports `credential_optional` must accept a new connection with no secret, and a later fetch must receive an empty credential, not an error (Task 2 test).
- The IPC connection must round-trip `provider`, and an IPC payload from an older frontend with no `provider` must still deserialize as RocketVault (Task 3 test).

---

## Task 1: `DispatchingSecretFetcher` and RocketVault capabilities

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-infra/src/secret_providers.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (module and export)
- Modify: `crates/rocket-infra/src/rocketvault/mod.rs` (add `capabilities` to the existing `impl VaultSecretFetcher for ReqwestVaultSecretFetcher`)

**Interfaces:**
- Consumes: `VaultSecretFetcher`, `ProviderCapabilities`, `SecretProviderKind`, `SecretManagerConnection` (Plan 01).
- Produces:
  - `rocket_infra::DispatchingSecretFetcher` with `new() -> Self`, `register(&mut self, kind: SecretProviderKind, fetcher: Arc<dyn VaultSecretFetcher>)`, `with_rocketvault() -> Self`. It implements `VaultSecretFetcher`.

- [ ] **Step 1: Write the failing tests**

Create `crates/rocket-infra/src/secret_providers.rs` containing only the test module first, so the tests fail to compile:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_environment::{ExternalSecretRef, SecretManagerConnection};
    use rocket_shared::error::DomainResult;
    use std::sync::Mutex;

    /// Records which connection ids it was asked to forget and answers a fixed value.
    struct RecordingProvider {
        value: &'static str,
        forgotten: Mutex<Vec<String>>,
        caps: ProviderCapabilities,
    }

    impl RecordingProvider {
        fn new(value: &'static str, caps: ProviderCapabilities) -> Arc<Self> {
            Arc::new(Self {
                value,
                forgotten: Mutex::new(Vec::new()),
                caps,
            })
        }
    }

    #[async_trait]
    impl VaultSecretFetcher for RecordingProvider {
        async fn list_secrets(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(vec![ExternalSecretRef {
                name: self.value.to_string(),
                secret_id: self.value.to_string(),
            }])
        }
        async fn get_secret_value(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
            _id: &str,
        ) -> DomainResult<Option<String>> {
            Ok(Some(self.value.to_string()))
        }
        async fn test_connection(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn forget_connection(&self, id: &str) {
            self.forgotten
                .lock()
                .expect("lock forgotten")
                .push(id.to_string());
        }
        fn capabilities(&self, _c: &SecretManagerConnection) -> ProviderCapabilities {
            self.caps
        }
    }

    fn conn(provider: SecretProviderKind) -> SecretManagerConnection {
        SecretManagerConnection {
            id: "c1".to_string(),
            label: "L".to_string(),
            base_url: String::new(),
            client_id: String::new(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider,
            config: None,
        }
    }

    #[tokio::test]
    async fn routes_each_call_to_the_connections_provider() {
        let mut d = DispatchingSecretFetcher::new();
        d.register(
            SecretProviderKind::RocketVault,
            RecordingProvider::new("from-rv", ProviderCapabilities::default()),
        );
        d.register(
            SecretProviderKind::Azure,
            RecordingProvider::new("from-azure", ProviderCapabilities::default()),
        );

        let rv = d
            .get_secret_value(&conn(SecretProviderKind::RocketVault), "s", "v", "id")
            .await
            .expect("rocketvault routes");
        let az = d
            .get_secret_value(&conn(SecretProviderKind::Azure), "s", "v", "id")
            .await
            .expect("azure routes");

        assert_eq!(rv.as_deref(), Some("from-rv"));
        assert_eq!(az.as_deref(), Some("from-azure"));
    }

    #[tokio::test]
    async fn an_unregistered_provider_errors_and_names_it_without_falling_back() {
        let mut d = DispatchingSecretFetcher::new();
        d.register(
            SecretProviderKind::RocketVault,
            RecordingProvider::new("from-rv", ProviderCapabilities::default()),
        );
        let aws = conn(SecretProviderKind::Aws);

        let err = d
            .list_secrets(&aws, "s", "v")
            .await
            .expect_err("aws is not registered");
        assert!(err.to_string().contains("AWS Secrets Manager"), "got: {err}");

        let err = d
            .test_connection(&aws, "s", "v")
            .await
            .expect_err("aws is not registered");
        assert!(err.to_string().contains("AWS Secrets Manager"), "got: {err}");

        let err = d
            .validate_connection(&aws)
            .expect_err("aws is not registered");
        assert!(err.to_string().contains("AWS Secrets Manager"), "got: {err}");
    }

    #[test]
    fn capabilities_come_from_the_connections_provider_and_default_when_unregistered() {
        let mut d = DispatchingSecretFetcher::new();
        let caps = ProviderCapabilities {
            certificates: false,
            credential_optional: true,
            fetch_on_reference: true,
        };
        d.register(SecretProviderKind::Azure, RecordingProvider::new("x", caps));

        assert_eq!(d.capabilities(&conn(SecretProviderKind::Azure)), caps);
        assert_eq!(
            d.capabilities(&conn(SecretProviderKind::Gcp)),
            ProviderCapabilities::default()
        );
    }

    #[test]
    fn forget_connection_reaches_every_registered_provider() {
        let rv = RecordingProvider::new("a", ProviderCapabilities::default());
        let az = RecordingProvider::new("b", ProviderCapabilities::default());
        let mut d = DispatchingSecretFetcher::new();
        d.register(SecretProviderKind::RocketVault, rv.clone());
        d.register(SecretProviderKind::Azure, az.clone());

        d.forget_connection("c1");

        assert_eq!(rv.forgotten.lock().expect("lock").as_slice(), ["c1"]);
        assert_eq!(az.forgotten.lock().expect("lock").as_slice(), ["c1"]);
    }

    #[test]
    fn with_rocketvault_reports_certificate_support_for_rocketvault_only() {
        let d = DispatchingSecretFetcher::with_rocketvault();
        assert!(d.capabilities(&conn(SecretProviderKind::RocketVault)).certificates);
        assert!(!d.capabilities(&conn(SecretProviderKind::Azure)).certificates);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Add `mod secret_providers;` and `pub use secret_providers::DispatchingSecretFetcher;` to `crates/rocket-infra/src/lib.rs` (next to `pub use rocketvault::ReqwestVaultSecretFetcher;`), then run:
`cargo test -j4 -p rocket-infra secret_providers`
Expected: FAIL to compile (`DispatchingSecretFetcher` not found).

- [ ] **Step 3: Implement the dispatcher**

Put this above the test module in `crates/rocket-infra/src/secret_providers.rs`:

```rust
//! Routes secret-manager calls to the implementation for a connection's provider.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rocket_environment::{
    ExternalSecretRef, ProviderCapabilities, SecretManagerConnection, SecretProviderKind,
    VaultCertificateMaterial, VaultCertificateSummary, VaultSecretFetcher,
};
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};

use crate::rocketvault::ReqwestVaultSecretFetcher;

/// One `VaultSecretFetcher` that holds an implementation per provider and
/// routes every call on `connection.provider`. It is wired once at startup, so
/// `rocket-app` keeps holding a single trait object for every connection.
#[derive(Default)]
pub struct DispatchingSecretFetcher {
    providers: HashMap<SecretProviderKind, Arc<dyn VaultSecretFetcher>>,
}

impl DispatchingSecretFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// A dispatcher with only the RocketVault provider registered.
    pub fn with_rocketvault() -> Self {
        let mut dispatcher = Self::new();
        dispatcher.register(
            SecretProviderKind::RocketVault,
            Arc::new(ReqwestVaultSecretFetcher::new()),
        );
        dispatcher
    }

    pub fn register(&mut self, kind: SecretProviderKind, fetcher: Arc<dyn VaultSecretFetcher>) {
        self.providers.insert(kind, fetcher);
    }

    fn provider_for(
        &self,
        connection: &SecretManagerConnection,
    ) -> DomainResult<&Arc<dyn VaultSecretFetcher>> {
        self.providers.get(&connection.provider).ok_or_else(|| {
            DomainError::InvalidInput(format!(
                "{} is not available in this build of Rocket",
                connection.provider.display_name()
            ))
        })
    }
}

#[async_trait]
impl VaultSecretFetcher for DispatchingSecretFetcher {
    async fn list_secrets(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        self.provider_for(connection)?
            .list_secrets(connection, client_secret, vault_name)
            .await
    }

    async fn get_secret_value(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>> {
        self.provider_for(connection)?
            .get_secret_value(connection, client_secret, vault_name, secret_id)
            .await
    }

    async fn test_connection(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<()> {
        self.provider_for(connection)?
            .test_connection(connection, client_secret, vault_name)
            .await
    }

    async fn list_certificates(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        self.provider_for(connection)?
            .list_certificates(connection, client_secret, vault_name)
            .await
    }

    async fn fetch_certificate(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        certificate_name: &str,
        format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        self.provider_for(connection)?
            .fetch_certificate(connection, client_secret, vault_name, certificate_name, format)
            .await
    }

    fn forget_connection(&self, connection_id: &str) {
        // Only the id is known here, so every provider is told. A provider
        // that never cached anything for it does nothing.
        for provider in self.providers.values() {
            provider.forget_connection(connection_id);
        }
    }

    fn capabilities(&self, connection: &SecretManagerConnection) -> ProviderCapabilities {
        self.providers
            .get(&connection.provider)
            .map(|provider| provider.capabilities(connection))
            .unwrap_or_default()
    }

    fn validate_connection(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
        self.provider_for(connection)?.validate_connection(connection)
    }
}
```

If `VaultCertificateMaterial` or `VaultCertificateSummary` are not re-exported at the `rocket_environment` root, import them from `rocket_environment::vault_secret_fetcher::`. `rocket_shared::certificate::VaultCertificateFormat` is the path used by the trait file.

- [ ] **Step 4: Add RocketVault capabilities**

In `crates/rocket-infra/src/rocketvault/mod.rs`, inside the existing `impl VaultSecretFetcher for ReqwestVaultSecretFetcher` block, add:

```rust
    fn capabilities(
        &self,
        _connection: &SecretManagerConnection,
    ) -> rocket_environment::ProviderCapabilities {
        rocket_environment::ProviderCapabilities {
            certificates: true,
            credential_optional: false,
            fetch_on_reference: false,
        }
    }
```

Add a test to that file's existing test module:

```rust
    #[test]
    fn rocketvault_supports_certificates_and_needs_a_credential() {
        let fetcher = ReqwestVaultSecretFetcher::new();
        let caps = fetcher.capabilities(&test_connection("https://v:8774".to_string()));
        assert!(caps.certificates);
        assert!(!caps.credential_optional);
        assert!(!caps.fetch_on_reference);
    }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -j4 -p rocket-infra secret_providers`
Expected: PASS (5 tests).
Run: `cargo test -j4 -p rocket-infra rocketvault`
Expected: PASS (the existing wiremock tests plus the new one).

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add crates/rocket-infra/src/secret_providers.rs crates/rocket-infra/src/lib.rs crates/rocket-infra/src/rocketvault/mod.rs
```

Suggested subject: `feat(secrets): route secret manager calls by provider`.

---

## Task 2: Provider-aware validation and credential rule in `SecretManagerService`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/secret_manager_service.rs`

**Interfaces:**
- Consumes: `VaultSecretFetcher::capabilities`, `VaultSecretFetcher::validate_connection` (Plan 01), `SecretProviderKind` (Plan 01).
- Produces: `SecretManagerService::save` and the private `connection_and_secret` follow the rules below. No signature changes.

Rules:
- Always: `id` and `label` must be non-empty.
- `provider == RocketVault`: the existing checks stay (non-empty `base_url` and `client_id`, an http(s) URL with a host).
- Any other provider: call `self.fetcher.validate_connection(&connection)`.
- A new connection with no `client_secret` is rejected, unless `self.fetcher.capabilities(&connection).credential_optional`.
- `connection_and_secret` returns an empty string, not an error, when no credential is stored and the provider reports `credential_optional`.

- [ ] **Step 1: Write the failing tests**

Add these to the existing `#[cfg(test)] mod tests` in `secret_manager_service.rs`. They define their own small fakes with unique names so they do not depend on the existing test helpers:

```rust
    use rocket_environment::{ProviderCapabilities, SecretProviderKind};

    struct P2Repo {
        rows: std::sync::Mutex<Vec<SecretManagerConnection>>,
    }
    impl P2Repo {
        fn new() -> Self {
            Self {
                rows: std::sync::Mutex::new(Vec::new()),
            }
        }
    }
    impl SecretManagerRepository for P2Repo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            Ok(self.rows.lock().expect("lock").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            Ok(self.rows.lock().expect("lock").iter().find(|c| c.id == id).cloned())
        }
        fn save(&self, c: &SecretManagerConnection) -> DomainResult<()> {
            let mut rows = self.rows.lock().expect("lock");
            rows.retain(|r| r.id != c.id);
            rows.push(c.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.rows.lock().expect("lock").retain(|r| r.id != id);
            Ok(())
        }
    }

    struct P2Store {
        values: std::sync::Mutex<std::collections::HashMap<String, String>>,
    }
    impl P2Store {
        fn new() -> Self {
            Self {
                values: std::sync::Mutex::new(std::collections::HashMap::new()),
            }
        }
    }
    impl SecretStore for P2Store {
        fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self.values.lock().expect("lock").get(&format!("{scope}/{key}")).cloned())
        }
        fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
            self.values
                .lock()
                .expect("lock")
                .insert(format!("{scope}/{key}"), value.to_string());
            Ok(())
        }
        fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
            self.values.lock().expect("lock").remove(&format!("{scope}/{key}"));
            Ok(())
        }
    }

    /// A fetcher with scripted capabilities and an optional validation error.
    struct P2Fetcher {
        caps: ProviderCapabilities,
        reject: Option<&'static str>,
        seen_credential: std::sync::Mutex<Option<String>>,
    }
    impl P2Fetcher {
        fn new(caps: ProviderCapabilities, reject: Option<&'static str>) -> Arc<Self> {
            Arc::new(Self {
                caps,
                reject,
                seen_credential: std::sync::Mutex::new(None),
            })
        }
    }
    #[async_trait::async_trait]
    impl VaultSecretFetcher for P2Fetcher {
        async fn list_secrets(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(vec![])
        }
        async fn get_secret_value(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
            _id: &str,
        ) -> DomainResult<Option<String>> {
            Ok(None)
        }
        async fn test_connection(
            &self,
            _c: &SecretManagerConnection,
            secret: &str,
            _v: &str,
        ) -> DomainResult<()> {
            *self.seen_credential.lock().expect("lock") = Some(secret.to_string());
            Ok(())
        }
        fn capabilities(&self, _c: &SecretManagerConnection) -> ProviderCapabilities {
            self.caps
        }
        fn validate_connection(&self, _c: &SecretManagerConnection) -> DomainResult<()> {
            match self.reject {
                Some(msg) => Err(DomainError::InvalidInput(msg.to_string())),
                None => Ok(()),
            }
        }
    }

    fn p2_connection(id: &str, provider: SecretProviderKind) -> SecretManagerConnection {
        SecretManagerConnection {
            id: id.to_string(),
            label: "L".to_string(),
            base_url: String::new(),
            client_id: String::new(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider,
            config: None,
        }
    }

    fn p2_service(fetcher: Arc<P2Fetcher>) -> SecretManagerService {
        SecretManagerService::new(Box::new(P2Repo::new()), Arc::new(P2Store::new()), fetcher)
    }

    #[test]
    fn rocketvault_still_requires_base_url_client_id_and_a_secret() {
        let svc = p2_service(P2Fetcher::new(ProviderCapabilities::default(), None));

        let blank = p2_connection("c1", SecretProviderKind::RocketVault);
        let err = svc.save(blank, Some("s".to_string())).expect_err("blank base_url");
        assert!(err.to_string().contains("base_url"), "got: {err}");

        let mut ok = p2_connection("c2", SecretProviderKind::RocketVault);
        ok.base_url = "https://v:8774".to_string();
        ok.client_id = "rocketapi".to_string();
        let err = svc.save(ok, None).expect_err("a new connection needs a secret");
        assert!(err.to_string().contains("client_secret"), "got: {err}");
    }

    #[test]
    fn another_provider_uses_its_own_validation_and_skips_the_rocketvault_url_rules() {
        let svc = p2_service(P2Fetcher::new(
            ProviderCapabilities::default(),
            Some("tenant is required"),
        ));
        let err = svc
            .save(p2_connection("c1", SecretProviderKind::Azure), Some("s".to_string()))
            .expect_err("the provider rejects it");
        assert!(err.to_string().contains("tenant is required"), "got: {err}");

        let svc = p2_service(P2Fetcher::new(ProviderCapabilities::default(), None));
        svc.save(p2_connection("c2", SecretProviderKind::Azure), Some("s".to_string()))
            .expect("empty base_url is fine for a provider that does not use it");
    }

    #[test]
    fn a_credential_optional_provider_saves_without_a_secret() {
        let caps = ProviderCapabilities {
            credential_optional: true,
            ..ProviderCapabilities::default()
        };
        let svc = p2_service(P2Fetcher::new(caps, None));
        svc.save(p2_connection("c1", SecretProviderKind::Azure), None)
            .expect("no credential is allowed for this provider");
    }

    #[test]
    fn a_provider_that_needs_a_credential_rejects_a_new_connection_without_one() {
        let svc = p2_service(P2Fetcher::new(ProviderCapabilities::default(), None));
        let err = svc
            .save(p2_connection("c1", SecretProviderKind::Azure), None)
            .expect_err("a credential is required");
        assert!(err.to_string().contains("client_secret"), "got: {err}");
    }

    #[tokio::test]
    async fn an_optional_credential_reaches_the_fetcher_as_an_empty_string() {
        let caps = ProviderCapabilities {
            credential_optional: true,
            ..ProviderCapabilities::default()
        };
        let fetcher = P2Fetcher::new(caps, None);
        let svc = p2_service(Arc::clone(&fetcher));
        svc.save(p2_connection("c1", SecretProviderKind::Azure), None)
            .expect("save");

        svc.test_connection("c1", "vault")
            .await
            .expect("an optional credential is not an error");

        assert_eq!(
            fetcher.seen_credential.lock().expect("lock").as_deref(),
            Some("")
        );
    }
```

The module may already import `ExternalSecretRef`, `SecretStore`, `SecretManagerRepository`, `DomainError` through `use super::*;`. Add any import the compiler asks for. If a name here clashes with an existing test helper, rename ours with a different prefix.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-app secret_manager_service`
Expected: FAIL. The Azure tests fail on the base_url rule, and the optional-credential tests fail on the `client_secret` rule.

- [ ] **Step 3: Implement**

In `crates/rocket-app/src/secret_manager_service.rs`:

1. Change `validate_connection` so the RocketVault URL rules apply only to RocketVault. Replace the whole function with:

```rust
/// Rejects a connection record that could never work, before anything is
/// written to the keychain or to `secret_managers.yml`. The https-only rule
/// for non-loopback hosts is enforced by the RocketVault fetcher on every
/// call, so it is not repeated here. Other providers' rules come from their
/// fetcher.
fn validate_connection(
    connection: &SecretManagerConnection,
    fetcher: &dyn VaultSecretFetcher,
) -> DomainResult<()> {
    for (field, value) in [("id", &connection.id), ("label", &connection.label)] {
        if value.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "secret manager connection {field} must not be empty"
            )));
        }
    }
    if connection.provider != SecretProviderKind::RocketVault {
        return fetcher.validate_connection(connection);
    }
    for (field, value) in [
        ("base_url", &connection.base_url),
        ("client_id", &connection.client_id),
    ] {
        if value.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "secret manager connection {field} must not be empty"
            )));
        }
    }
    let parsed = url::Url::parse(&connection.base_url).map_err(|e| {
        DomainError::InvalidInput(format!("invalid base_url '{}': {e}", connection.base_url))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(DomainError::InvalidInput(format!(
            "base_url '{}' must be an http:// or https:// URL",
            connection.base_url
        )));
    }
    Ok(())
}
```

Add `SecretProviderKind` to the `use rocket_environment::secret_manager::{...}` import.

2. In `save`, change the first line to `validate_connection(&connection, self.fetcher.as_ref())?;` and change the `None =>` arm so a missing credential is allowed for providers that report it:

```rust
            None => {
                let has_existing_secret = self
                    .secret_store
                    .get(VAULT_CONNECTION_SCOPE, &connection.id)?
                    .is_some();
                let optional = self.fetcher.capabilities(&connection).credential_optional;
                if !has_existing_secret && !optional {
                    return Err(DomainError::InvalidInput(
                        "a new connection must be saved with a client_secret".to_string(),
                    ));
                }
            }
```

3. In `connection_and_secret`, replace the `ok_or_else` on the stored secret with:

```rust
        let stored = self.secret_store.get(VAULT_CONNECTION_SCOPE, id)?;
        let secret = match stored {
            Some(secret) => secret,
            None if self.fetcher.capabilities(&connection).credential_optional => String::new(),
            None => {
                return Err(DomainError::Internal(format!(
                    "no client secret available for connection {id} — it was never stored, or the OS keychain is locked/unavailable"
                )))
            }
        };
```

Keep the `let connection = ...` lines above it unchanged.

Update the doc comment on `save` to say the credential is optional for providers that report `credential_optional`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -j4 -p rocket-app secret_manager_service`
Expected: PASS, including every pre-existing test in that module.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add crates/rocket-app/src/secret_manager_service.rs
```

Suggested subject: `feat(secrets): validate connections per provider`.

---

## Task 3: IPC DTO and startup wiring

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/secret_managers.rs`
- Modify: `src-tauri/src/lib.rs` (the `vault_fetcher` construction near line 330)

**Interfaces:**
- Consumes: `SecretProviderKind`, `ProviderConfig` (Plan 01), `DispatchingSecretFetcher` (Task 1).
- Produces: the IPC shape `{ id, label, baseUrl, clientId, verifySsl, allowInsecureHttp, provider, config? }` where `provider` is the lowercase string. A payload with no `provider` deserializes as `rocketvault`.

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` at the bottom of `src-tauri/src/commands/secret_managers.rs`:

```rust
    #[test]
    fn connection_dto_without_provider_deserializes_as_rocketvault() {
        let json = r#"{"id":"c1","label":"L","baseUrl":"https://v","clientId":"x","verifySsl":true,"allowInsecureHttp":false}"#;
        let dto: SecretManagerConnectionDto = serde_json::from_str(json).expect("older payload");
        let conn: SecretManagerConnection = dto.into();
        assert_eq!(conn.provider, rocket_environment::SecretProviderKind::RocketVault);
        assert!(conn.config.is_none());
    }

    #[test]
    fn connection_dto_round_trips_the_provider_as_lowercase() {
        let mut conn = SecretManagerConnection {
            id: "c1".to_string(),
            label: "L".to_string(),
            base_url: String::new(),
            client_id: String::new(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: rocket_environment::SecretProviderKind::Azure,
            config: None,
        };
        let json = serde_json::to_value(SecretManagerConnectionDto::from(conn.clone()))
            .expect("serialize");
        assert_eq!(json["provider"], "azure");
        assert!(json.get("config").is_none());

        let back: SecretManagerConnection = serde_json::from_value::<SecretManagerConnectionDto>(json)
            .expect("deserialize")
            .into();
        conn.base_url = String::new();
        assert_eq!(back, conn);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket secret_managers`.
Expected: FAIL to compile (the DTO has no `provider`; the `From` impls also fail on the new connection fields).

- [ ] **Step 3: Implement the DTO fields and conversions**

In `src-tauri/src/commands/secret_managers.rs`, add to `SecretManagerConnectionDto` after `allow_insecure_http`:

```rust
    #[serde(default)]
    pub provider: SecretProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ProviderConfig>,
```

Add `use rocket_environment::secret_manager::{ProviderConfig, SecretProviderKind};` beside the existing `SecretManagerConnection` import. In both `From` impls add `provider: c.provider, config: c.config,` (domain to DTO) and `provider: dto.provider, config: dto.config,` (DTO to domain).

- [ ] **Step 4: Wire the dispatcher at startup**

In `src-tauri/src/lib.rs`, replace the construction near line 330:

```rust
            let vault_fetcher: Arc<
                dyn rocket_environment::vault_secret_fetcher::VaultSecretFetcher,
            > = Arc::new(rocket_infra::DispatchingSecretFetcher::with_rocketvault());
```

(only the `Arc::new(...)` expression changes: `ReqwestVaultSecretFetcher::new()` becomes `DispatchingSecretFetcher::with_rocketvault()`).

- [ ] **Step 5: Run the checks**

Run: `cargo test -j4 -p rocket secret_managers`
Expected: PASS.
Run: `cargo check -j4 -p rocket --tests`
Expected: PASS.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add src-tauri/src/commands/secret_managers.rs src-tauri/src/lib.rs
```

Suggested subject: `feat(secrets): carry the provider over IPC and dispatch at startup`.

---

## Next Plan

[Plan 03: Fetch-on-reference and certificate gating](2026-10-04-secret-provider-plan-03-resolution-and-certs.md). It depends on this plan. Chain to it automatically when this one finishes.
