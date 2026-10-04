# Secret provider foundation, Plan 03: Fetch-on-reference and certificate gating

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a single send fetch only the secrets it references for providers that opt in, and reject `vault` client certificates whose binding points at a provider that cannot supply them.

**Architecture:** `resolve_external_secrets_partial` gains an optional request filter that applies only to connections whose provider reports `fetch_on_reference`. A small `ProviderCapabilityLookup` trait in `rocket-environment` lets `EnvironmentService` check certificate support without holding connections. `SecretManagerService` implements it and the environment save commands pass it in.

**Tech Stack:** Rust, async-trait, tokio tests, Tauri commands.

**Spec:** [../../specs/2026-10-04-secret-provider-foundation-design.md](../../specs/2026-10-04-secret-provider-foundation-design.md) (sections 6, 7)

**Depends on:** [Plan 01](2026-10-04-secret-provider-plan-01-domain.md) and [Plan 02](2026-10-04-secret-provider-plan-02-dispatch-and-service.md).

## Global Constraints

- RocketVault behavior must not change: it reports `fetch_on_reference: false`, so it keeps fetching every ref.
- The runner and the flow executor keep calling `resolve_external_secrets` and keep fetching every ref once per run.
- A failed binding fails a send only when the request references that alias. Keep that policy.
- Production code never panics on bad input: use `DomainResult` and explicit error mapping, not unwrap. Tests may use `.expect("reason")`.
- Always pass `-j4` to `cargo test` and `cargo check`. Never run `cargo test --workspace`.
- Commit with conventional commits, using the `dev-workflow-skills:1-git-commit` skill, staging by explicit path only. Task 1 edits `crates/rocket-app/src/execution_service.rs`, which may already carry unrelated local edits. Before editing, run `git diff --stat crates/rocket-app/src/execution_service.rs`. If it shows changes you did not make, stop and ask the user how to proceed. Do not stage or discard them.

## Review Focus

- A script that reads a secret through a literal name (`rok.getSecretVar('cloud.one')`) must be seen by the reference check, so its secret is fetched (Task 1 test).
- An unreferenced secret that would fail must not fail the send (Task 1 test).
- A referenced secret that fails must fail the send (Task 1 test).
- A connection that has been deleted must not make `save_with_capabilities` fail (Task 2 test).
- The certificate error must name the provider and say what to do instead (Task 2 test).
- A dynamically built secret name is invisible to the text check. This is the documented reason RocketVault stays eager. Do not add a test that pins it. Mention it in the provider specs.

---

## Task 1: Fetch only referenced secrets for `fetch_on_reference` providers (DEFERRED)

> **Deferred, not part of this branch.** This task builds on `resolve_external_secrets_partial` and `references_alias`, which exist only as uncommitted work outside this branch, and no provider reports `fetch_on_reference` yet. It moves to the first cloud provider's plan (see spec section 6). Skip it when executing this plan.

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`references_alias` at about line 489, `resolve_external_secrets` at about line 406, `resolve_external_secrets_partial` at about line 426, `execute` at about line 1780, and the test fake `FakeVaultFetcher` at about line 2540)

**Interfaces:**
- Consumes: `ProviderCapabilities::fetch_on_reference` (Plan 01), the `VaultSecretFetcher::capabilities` method (Plan 01).
- Produces: private `fn references_text(&self, input: &ExecuteRequestInput, needle: &str, resolved: &HashMap<String, String>) -> bool`. `references_alias` now calls it with `format!("{alias}.")`. `resolve_external_secrets_partial` takes a third argument `only_referenced_by: Option<&ExecuteRequestInput>`.

- [ ] **Step 1: Extend the test fake and write the failing tests**

In the test module, give `FakeVaultFetcher` a capabilities field. Change the struct and its constructor to:

```rust
    struct FakeVaultFetcher {
        responses: std::collections::HashMap<String, FakeSecretOutcome>,
        calls: Mutex<Vec<String>>,
        caps: rocket_environment::ProviderCapabilities,
    }

    impl FakeVaultFetcher {
        fn new(responses: Vec<(&str, FakeSecretOutcome)>) -> Self {
            Self {
                responses: responses
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                calls: Mutex::new(Vec::new()),
                caps: rocket_environment::ProviderCapabilities::default(),
            }
        }

        /// A fetcher whose provider fetches only referenced secrets on a send.
        fn lazy(responses: Vec<(&str, FakeSecretOutcome)>) -> Self {
            let mut fetcher = Self::new(responses);
            fetcher.caps.fetch_on_reference = true;
            fetcher
        }
    }
```

In `impl rocket_environment::VaultSecretFetcher for FakeVaultFetcher`, add:

```rust
        fn capabilities(
            &self,
            _connection: &rocket_environment::SecretManagerConnection,
        ) -> rocket_environment::ProviderCapabilities {
            self.caps
        }
```

Then add these tests after `resolve_external_secrets_skips_a_deleted_secret_silently`:

```rust
    fn cloud_env() -> Environment {
        let mut env = Environment::new("prod");
        env.external_secrets.push(binding_with_refs(
            "cloud",
            vec![("one", "sec-1"), ("two", "sec-2")],
        ));
        env
    }

    fn two_secrets() -> Vec<(&'static str, FakeSecretOutcome)> {
        vec![
            ("sec-1", FakeSecretOutcome::Value("v1".to_string())),
            ("sec-2", FakeSecretOutcome::Value("v2".to_string())),
        ]
    }

    fn recorded_calls(fetcher: &FakeVaultFetcher) -> Vec<String> {
        fetcher.calls.lock().expect("lock calls").clone()
    }

    #[tokio::test]
    async fn a_lazy_provider_fetches_only_the_secret_the_url_references() {
        let fetcher = Arc::new(FakeVaultFetcher::lazy(two_secrets()));
        let svc = svc_with_vault(Some(cloud_env()), Arc::clone(&fetcher));

        svc.execute(sample_input(
            "https://api.example.com/{{cloud.one}}",
            Some("prod"),
        ))
        .await
        .expect("send");

        assert_eq!(recorded_calls(&fetcher), vec!["sec-1".to_string()]);
    }

    #[tokio::test]
    async fn a_lazy_provider_fetches_a_secret_a_script_reads_by_literal_name() {
        let fetcher = Arc::new(FakeVaultFetcher::lazy(two_secrets()));
        let svc = svc_with_vault(Some(cloud_env()), Arc::clone(&fetcher));
        let mut input = sample_input("https://api.example.com/x", Some("prod"));
        input.pre_request_script = Some("const k = rok.getSecretVar('cloud.two');".to_string());

        svc.execute(input).await.expect("send");

        assert_eq!(recorded_calls(&fetcher), vec!["sec-2".to_string()]);
    }

    #[tokio::test]
    async fn a_lazy_provider_fetches_nothing_when_the_request_references_nothing() {
        let fetcher = Arc::new(FakeVaultFetcher::lazy(vec![
            ("sec-1", FakeSecretOutcome::Error("unreachable".to_string())),
            ("sec-2", FakeSecretOutcome::Error("unreachable".to_string())),
        ]));
        let svc = svc_with_vault(Some(cloud_env()), Arc::clone(&fetcher));

        svc.execute(sample_input("https://api.example.com/x", Some("prod")))
            .await
            .expect("an unreferenced failing secret must not block the send");

        assert!(recorded_calls(&fetcher).is_empty());
    }

    #[tokio::test]
    async fn a_lazy_provider_fails_the_send_when_a_referenced_secret_fails() {
        let fetcher = Arc::new(FakeVaultFetcher::lazy(vec![(
            "sec-1",
            FakeSecretOutcome::Error("unreachable".to_string()),
        )]));
        let svc = svc_with_vault(Some(cloud_env()), Arc::clone(&fetcher));

        let result = svc
            .execute(sample_input(
                "https://api.example.com/{{cloud.one}}",
                Some("prod"),
            ))
            .await;

        assert!(result.is_err(), "a referenced secret that fails must fail the send");
    }

    #[tokio::test]
    async fn an_eager_provider_still_fetches_every_secret_on_a_send() {
        let fetcher = Arc::new(FakeVaultFetcher::new(two_secrets()));
        let svc = svc_with_vault(Some(cloud_env()), Arc::clone(&fetcher));

        svc.execute(sample_input("https://api.example.com/x", Some("prod")))
            .await
            .expect("send");

        assert_eq!(recorded_calls(&fetcher).len(), 2);
    }

    #[tokio::test]
    async fn resolving_for_a_run_still_fetches_every_secret_even_for_a_lazy_provider() {
        let fetcher = Arc::new(FakeVaultFetcher::lazy(two_secrets()));
        let svc = svc_with_vault(Some(cloud_env()), Arc::clone(&fetcher));

        let result = svc
            .resolve_external_secrets(None, Some("prod"))
            .await
            .expect("resolve");

        assert_eq!(result.len(), 2);
        assert_eq!(recorded_calls(&fetcher).len(), 2);
    }
```

- [ ] **Step 2: Run to verify the new tests fail**

Run: `cargo test -j4 -p rocket-app execution_service::tests::a_lazy_provider`
Expected: the lazy tests that assert fewer fetches FAIL (every secret is fetched today). `an_eager_provider_still_fetches_every_secret_on_a_send` and the run-level test pass already.

- [ ] **Step 3: Refactor `references_alias` into a reusable text check**

In `crates/rocket-app/src/execution_service.rs`, rename the existing function and change how it gets its needle. Replace the head of the function:

```rust
    fn references_alias(
        &self,
        input: &ExecuteRequestInput,
        alias: &str,
        resolved: &std::collections::HashMap<String, String>,
    ) -> bool {
        let needle = format!("{alias}.");
```

with:

```rust
    fn references_alias(
        &self,
        input: &ExecuteRequestInput,
        alias: &str,
        resolved: &std::collections::HashMap<String, String>,
    ) -> bool {
        self.references_text(input, &format!("{alias}."), resolved)
    }

    /// Whether anything this send can read contains `needle`: the request
    /// input (URL, headers, body, auth, scripts and so on) or a variable
    /// value from any scope. A full-line `//` comment in a script is not a
    /// use. `needle` is `alias.` to ask about a whole binding or `alias.name`
    /// to ask about one secret.
    fn references_text(
        &self,
        input: &ExecuteRequestInput,
        needle: &str,
        resolved: &std::collections::HashMap<String, String>,
    ) -> bool {
```

In the body that follows, change the three uses of `&needle` to `needle`:
`line.contains(&needle)` becomes `line.contains(needle)`, `text.contains(&needle)` becomes `text.contains(needle)`, and `value.contains(&needle)` becomes `value.contains(needle)`.

- [ ] **Step 4: Add the filter to `resolve_external_secrets_partial`**

Change the signature to take the request, and decide per binding whether the connection is lazy:

```rust
    async fn resolve_external_secrets_partial(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
        only_referenced_by: Option<&ExecuteRequestInput>,
    ) -> (
        std::collections::HashMap<String, String>,
        Vec<UnresolvedBinding>,
    ) {
```

Inside `'bindings: for binding in &env.external_secrets {`, before the inner `for secret_ref ...` loop, add:

```rust
            // A provider that opts in is fetched only for secrets the request
            // references. A connection that cannot be read here is treated as
            // eager, so the existing deleted-connection handling still runs.
            let lazy = only_referenced_by.is_some()
                && matches!(
                    self.secret_manager_repo.get(&binding.connection_id),
                    Ok(Some(connection))
                        if self.vault_fetcher.capabilities(&connection).fetch_on_reference
                );
```

and as the first statement inside the `for secret_ref in &binding.secret_names {` loop, add:

```rust
                if let (true, Some(input)) = (lazy, only_referenced_by) {
                    let key = format!("{}.{}", binding.alias, secret_ref.name);
                    if !self.references_text(input, &key, &std::collections::HashMap::new()) {
                        continue;
                    }
                }
```

- [ ] **Step 5: Update the two callers**

In `resolve_external_secrets`, change the call to `self.resolve_external_secrets_partial(collection, environment_name, None)`:

```rust
        let (result, failures) = self
            .resolve_external_secrets_partial(collection, environment_name, None)
            .await;
```

In `execute`, pass the request:

```rust
        let (external_secrets, failures) = self
            .resolve_external_secrets_partial(
                input.collection.as_deref(),
                input.environment_name.as_deref(),
                Some(&input),
            )
            .await;
```

Update the doc comment above `resolve_external_secrets_partial` with one sentence: "For connections whose provider reports `fetch_on_reference`, `only_referenced_by` limits the fetch to secrets that request references."

- [ ] **Step 6: Run the tests**

Run: `cargo test -j4 -p rocket-app execution_service`
Expected: PASS, including every pre-existing external-secret test (RocketVault behavior is unchanged).
Run: `cargo test -j4 -p rocket-app collection_runner_service`
Expected: PASS (the once-per-run tests still hold).

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage only the hunks of this task:

```bash
git add -p crates/rocket-app/src/execution_service.rs
```

If the file carried unrelated local edits before this task, stage only the hunks you wrote. Suggested subject: `feat(secrets): fetch only referenced secrets for opted-in providers`.

---

## Task 2: Reject `vault` certificates on providers without certificate support

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-environment/src/secret_manager.rs` (add the lookup trait)
- Modify: `crates/rocket-environment/src/client_certificate_validation.rs` (add the validator and tests)
- Modify: `crates/rocket-environment/src/lib.rs` (exports)
- Modify: `crates/rocket-app/src/secret_manager_service.rs` (implement the lookup)
- Modify: `crates/rocket-app/src/environment_service.rs` (add `save_with_capabilities`)
- Modify: `src-tauri/src/commands/environments.rs` (`save_environment` and `save_global_environment`)

**Interfaces:**
- Consumes: `ProviderCapabilities`, `SecretProviderKind` (Plan 01), `SecretManagerService`'s repo and fetcher (Plan 02).
- Produces:
  - `rocket_environment::secret_manager::ConnectionProvider { kind: SecretProviderKind, capabilities: ProviderCapabilities }`.
  - `rocket_environment::secret_manager::ProviderCapabilityLookup` with `fn provider_of(&self, connection_id: &str) -> DomainResult<Option<ConnectionProvider>>`. `Ok(None)` means the connection does not exist.
  - `rocket_environment::validate_vault_certificate_providers(certs: &[ClientCertificate], bindings: &[ExternalSecretBinding], lookup: &dyn ProviderCapabilityLookup) -> DomainResult<()>`.
  - `EnvironmentService::save_with_capabilities(&self, env: &Environment, lookup: &dyn ProviderCapabilityLookup) -> DomainResult<()>`.

- [ ] **Step 1: Write the failing validator tests**

Add to the `#[cfg(test)] mod tests` of `crates/rocket-environment/src/client_certificate_validation.rs` (it already defines `bindings()` with alias `vault` and connection `conn-1`):

```rust
    use crate::secret_manager::{ConnectionProvider, ProviderCapabilityLookup, SecretProviderKind};
    use crate::vault_secret_fetcher::ProviderCapabilities;
    struct FixedLookup(Option<ConnectionProvider>);
    impl ProviderCapabilityLookup for FixedLookup {
        fn provider_of(&self, _id: &str) -> DomainResult<Option<ConnectionProvider>> {
            Ok(self.0.clone())
        }
    }

    fn vault_entry() -> Vec<ClientCertificate> {
        vec![vault("api.example.com", "vault", "client")]
    }

    fn provider(kind: SecretProviderKind, certificates: bool) -> Option<ConnectionProvider> {
        Some(ConnectionProvider {
            kind,
            capabilities: ProviderCapabilities {
                certificates,
                ..ProviderCapabilities::default()
            },
        })
    }

    #[test]
    fn a_vault_entry_on_a_certificate_capable_provider_is_accepted() {
        let lookup = FixedLookup(provider(SecretProviderKind::RocketVault, true));
        validate_vault_certificate_providers(&vault_entry(), &bindings(), &lookup)
            .expect("rocketvault supplies certificates");
    }

    #[test]
    fn a_vault_entry_on_another_provider_is_rejected_naming_the_provider() {
        let lookup = FixedLookup(provider(SecretProviderKind::Azure, false));
        let err = validate_vault_certificate_providers(&vault_entry(), &bindings(), &lookup)
            .expect_err("azure cannot supply client certificates");
        let text = err.to_string();
        assert!(text.contains("Azure Key Vault"), "got: {text}");
        assert!(text.contains("Client certificate 1"), "got: {text}");
        assert!(text.contains("RocketVault"), "should say what to do instead: {text}");
    }

    #[test]
    fn a_missing_connection_is_not_rejected_here() {
        let lookup = FixedLookup(None);
        validate_vault_certificate_providers(&vault_entry(), &bindings(), &lookup)
            .expect("a deleted connection already has its own warning");
    }

    #[test]
    fn file_and_secret_certificates_are_never_checked_against_the_provider() {
        let lookup = FixedLookup(provider(SecretProviderKind::Azure, false));
        let pem_only = vec![pem("api.example.com", "c.pem", "k.pem", None, None)];
        validate_vault_certificate_providers(&pem_only, &bindings(), &lookup)
            .expect("only vault entries need certificate support");
    }
```

The `pem(...)`, `vault(...)` and `bindings()` helpers already exist in this test module (`pem(domain, cert_path, key_path, cert_secret, key_secret)` and `vault(domain, binding, certificate)`), so the tests above use them as they are.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-environment client_certificate_validation`
Expected: FAIL to compile (`ConnectionProvider` and `validate_vault_certificate_providers` not found).

- [ ] **Step 3: Implement the trait and the validator**

In `crates/rocket-environment/src/secret_manager.rs`, add `use crate::vault_secret_fetcher::ProviderCapabilities;` to the imports at the top (the file already imports `DomainResult`), then add after `SecretManagerRepository`:

```rust
/// A connection's provider and what that provider supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProvider {
    pub kind: SecretProviderKind,
    pub capabilities: ProviderCapabilities,
}

/// Answers "which provider does this connection id use and what can it do".
/// Lets save-time validation reject a certificate that a connection's provider
/// cannot supply, without the validator holding connections itself.
pub trait ProviderCapabilityLookup: Send + Sync {
    /// `Ok(None)` means no connection has this id.
    fn provider_of(&self, connection_id: &str) -> DomainResult<Option<ConnectionProvider>>;
}
```

In `crates/rocket-environment/src/client_certificate_validation.rs`, add below `validate_client_certificates`:

```rust
/// A `vault` entry needs a binding whose connection can supply client
/// certificates. Only RocketVault can. A binding whose connection no longer
/// exists is skipped, because that case already has its own warning in the
/// External Secrets tab. Entries of other types are never checked.
pub fn validate_vault_certificate_providers(
    certs: &[ClientCertificate],
    bindings: &[ExternalSecretBinding],
    lookup: &dyn ProviderCapabilityLookup,
) -> DomainResult<()> {
    for (index, cert) in certs.iter().enumerate() {
        let ClientCertificate::Vault { binding, .. } = cert else {
            continue;
        };
        let Some(bound) = bindings.iter().find(|b| b.alias == *binding) else {
            continue;
        };
        let Some(info) = lookup.provider_of(&bound.connection_id)? else {
            continue;
        };
        if !info.capabilities.certificates {
            return Err(invalid(format!(
                "Client certificate {}: {} cannot supply client certificates. Use a RocketVault \
                 binding, or reference the certificate as a secret instead.",
                index + 1,
                info.kind.display_name()
            )));
        }
    }
    Ok(())
}
```

Add `use crate::secret_manager::ProviderCapabilityLookup;` to that file's imports. In `crates/rocket-environment/src/lib.rs`, change the validation export to `pub use client_certificate_validation::{validate_client_certificates, validate_vault_certificate_providers};` and the secret_manager export to also include `ConnectionProvider, ProviderCapabilityLookup`.

- [ ] **Step 4: Run the validator tests**

Run: `cargo test -j4 -p rocket-environment client_certificate_validation`
Expected: PASS (the four new tests and the existing ones).

- [ ] **Step 5: Write the failing service tests**

In `crates/rocket-app/src/environment_service.rs`'s test module (it has `MockEnvRepo` and `NullEventPublisher`; see line 142 for how a service is built), add:

```rust
    use rocket_environment::{
        ConnectionProvider, ExternalSecretBinding, ProviderCapabilities, ProviderCapabilityLookup,
        SecretProviderKind,
    };
    use rocket_shared::certificate::{ClientCertificate, VaultCertificateFormat};

    struct FixedLookup(Option<ConnectionProvider>);
    impl ProviderCapabilityLookup for FixedLookup {
        fn provider_of(&self, _id: &str) -> DomainResult<Option<ConnectionProvider>> {
            Ok(self.0.clone())
        }
    }

    fn env_with_vault_certificate() -> Environment {
        let mut env = Environment::new("prod");
        env.external_secrets.push(ExternalSecretBinding {
            alias: "vault".to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "v".to_string(),
            secret_names: vec![],
        });
        env.client_certificates.push(ClientCertificate::Vault {
            domain: "api.example.com".to_string(),
            binding: "vault".to_string(),
            certificate: "client".to_string(),
            format: VaultCertificateFormat::default(),
        });
        env
    }

    #[test]
    fn save_with_capabilities_rejects_a_vault_certificate_on_azure() {
        let svc = svc();
        let lookup = FixedLookup(Some(ConnectionProvider {
            kind: SecretProviderKind::Azure,
            capabilities: ProviderCapabilities::default(),
        }));

        let err = svc
            .save_with_capabilities(&env_with_vault_certificate(), &lookup)
            .expect_err("azure cannot supply certificates");

        assert!(err.to_string().contains("Azure Key Vault"), "got: {err}");
    }

    #[test]
    fn save_with_capabilities_saves_when_the_provider_supports_certificates() {
        let svc = svc();
        let lookup = FixedLookup(Some(ConnectionProvider {
            kind: SecretProviderKind::RocketVault,
            capabilities: ProviderCapabilities {
                certificates: true,
                ..ProviderCapabilities::default()
            },
        }));

        svc.save_with_capabilities(&env_with_vault_certificate(), &lookup)
            .expect("rocketvault supplies certificates");
    }

    #[test]
    fn save_with_capabilities_saves_when_the_connection_was_deleted() {
        let svc = svc();
        svc.save_with_capabilities(&env_with_vault_certificate(), &FixedLookup(None))
            .expect("a deleted connection is not a save error");
    }
```

The test module already builds a service with `EnvironmentService::new(Box::new(MockEnvRepo::new()), Box::new(NullEventPublisher))` (line 142). If that is inside a helper, call the helper in place of `svc()`. If it is inline, define `fn svc() -> EnvironmentService { EnvironmentService::new(Box::new(MockEnvRepo::new()), Box::new(NullEventPublisher)) }` in the test module.

- [ ] **Step 6: Implement `save_with_capabilities` and the lookup**

In `crates/rocket-app/src/environment_service.rs`, add inside `impl EnvironmentService`, after `save`:

```rust
    /// Like `save`, but also rejects a `vault` client certificate whose
    /// binding points at a provider that cannot supply certificates. The
    /// lookup is passed in because this service holds no connections.
    pub fn save_with_capabilities(
        &self,
        env: &Environment,
        lookup: &dyn rocket_environment::ProviderCapabilityLookup,
    ) -> DomainResult<()> {
        rocket_environment::validate_vault_certificate_providers(
            &env.client_certificates,
            &env.external_secrets,
            lookup,
        )?;
        self.save(env)
    }
```

In `crates/rocket-app/src/secret_manager_service.rs`, add:

```rust
impl rocket_environment::ProviderCapabilityLookup for SecretManagerService {
    fn provider_of(
        &self,
        connection_id: &str,
    ) -> DomainResult<Option<rocket_environment::ConnectionProvider>> {
        let Some(connection) = self.repo.get(connection_id)? else {
            return Ok(None);
        };
        Ok(Some(rocket_environment::ConnectionProvider {
            kind: connection.provider,
            capabilities: self.fetcher.capabilities(&connection),
        }))
    }
}
```

Add one test to `secret_manager_service.rs`'s test module that reuses Plan 02's `p2_service` helper:

```rust
    #[test]
    fn the_service_reports_a_connections_provider_and_capabilities() {
        use rocket_environment::ProviderCapabilityLookup;
        let caps = ProviderCapabilities {
            certificates: true,
            ..ProviderCapabilities::default()
        };
        let svc = p2_service(P2Fetcher::new(caps, None));
        let mut c = p2_connection("c1", SecretProviderKind::RocketVault);
        c.base_url = "https://v:8774".to_string();
        c.client_id = "rocketapi".to_string();
        svc.save(c, Some("s".to_string())).expect("save");

        let info = svc.provider_of("c1").expect("lookup").expect("exists");
        assert_eq!(info.kind, SecretProviderKind::RocketVault);
        assert!(info.capabilities.certificates);
        assert!(svc.provider_of("missing").expect("lookup").is_none());
    }
```

- [ ] **Step 7: Use it in the save commands**

In `src-tauri/src/commands/environments.rs`, change both save commands to take the connection service and call the new method:

```rust
#[tauri::command]
pub fn save_environment(
    collection: String,
    env: Environment,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
    secret_managers: State<'_, rocket_app::SecretManagerService>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    env_service_for(&collection, &ws)?.save_with_capabilities(&env, &*secret_managers)
}
```

```rust
#[tauri::command]
pub fn save_global_environment(
    env: Environment,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
    secret_managers: State<'_, rocket_app::SecretManagerService>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    global_env_service(&ws)?.save_with_capabilities(&env, &*secret_managers)
}
```

The frontend invoke calls do not change, because `State` parameters are injected by Tauri and not sent by the caller.

- [ ] **Step 8: Run the checks**

Run: `cargo test -j4 -p rocket-environment client_certificate_validation`
Expected: PASS.
Run: `cargo test -j4 -p rocket-app environment_service`
Expected: PASS.
Run: `cargo test -j4 -p rocket-app secret_manager_service`
Expected: PASS.
Run: `cargo check -j4 -p rocket --tests`
Expected: PASS.

- [ ] **Step 9: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add crates/rocket-environment/src/secret_manager.rs crates/rocket-environment/src/client_certificate_validation.rs crates/rocket-environment/src/lib.rs crates/rocket-app/src/secret_manager_service.rs crates/rocket-app/src/environment_service.rs src-tauri/src/commands/environments.rs
```

Suggested subject: `feat(secrets): reject vault certificates on other providers`.

---

## Next Plan

[Plan 04: Frontend provider selector and certificate gating](2026-10-04-secret-provider-plan-04-frontend.md). It depends on Plan 02 for the IPC shape and Plan 03 for the save-time error. Chain to it automatically when this one finishes.
