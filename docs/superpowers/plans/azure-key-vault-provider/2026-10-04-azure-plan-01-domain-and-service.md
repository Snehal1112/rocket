# Azure Key Vault Plan 01: Domain, IPC and Service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give a connection somewhere to store Azure settings, expose them over IPC in camelCase, and close the foundation's save-time carry-over items.

**Architecture:** `ProviderConfig` gains an `Azure` variant in `rocket-environment` (persistence, no camelCase). `src-tauri` gets its own camelCase `ProviderConfigDto` so the persistence enum never crosses IPC. `SecretManagerService::save` rejects a provider change on an existing id, and `FsSecretManagerRepo` explains an unreadable file.

**Tech Stack:** Rust, serde, serde_yaml 0.9, serde_json.

**Spec:** [../../specs/2026-10-04-azure-key-vault-provider-design.md](../../specs/2026-10-04-azure-key-vault-provider-design.md) (sections 4, 6, 8)

**Index:** [00-plan-index.md](00-plan-index.md) holds the locked interface contract and the global constraints. Read both first.

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Global Constraints

- `ProviderConfig` is a persistence type. No `rename_all = "camelCase"` on it.
- RocketVault rows must still serialize without `provider` or `config` keys (older builds read them).
- `cargo` commands take `-j4`. Never call `unwrap` in production code.

## Review Focus

- A RocketVault YAML row written before this change still loads and re-serializes byte-identically (existing tests cover it, keep them green).
- `config` with a missing `tenant_id` fails to load instead of defaulting to an empty tenant.
- A payload from an older frontend with no `config` key still deserializes.
- Saving an edit that tries to flip an existing connection from RocketVault to Azure must not overwrite its stored credential.
- A `secret_managers.yml` holding a `config` tag this build does not know fails with a message that names the likely cause.

---

### Task 1: `ProviderConfig::Azure`

**Files:**
- Modify: `crates/rocket-environment/src/secret_manager.rs:40-45` (the enum) and its `tests` module

**Interfaces:**
- Produces: `ProviderConfig::Azure { tenant_id: String, authority_host: Option<String> }`, re-exported already as `rocket_environment::ProviderConfig`.

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` in `crates/rocket-environment/src/secret_manager.rs`, and extend its `use super::{...}` line to include `ProviderConfig`:

```rust
    fn azure_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "az-1".to_string(),
            label: "Prod Azure".to_string(),
            base_url: "https://prod-kv.vault.azure.net".to_string(),
            client_id: "11111111-1111-1111-1111-111111111111".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::Azure,
            config: Some(ProviderConfig::Azure {
                tenant_id: "22222222-2222-2222-2222-222222222222".to_string(),
                authority_host: None,
            }),
        }
    }

    #[test]
    fn azure_config_round_trips_through_yaml() {
        let c = azure_connection();
        let yaml = serde_yaml::to_string(&c).expect("serialize");
        assert!(yaml.contains("provider: azure"), "got: {yaml}");
        assert!(yaml.contains("tenant_id"), "persistence keeps snake_case: {yaml}");
        assert!(
            !yaml.contains("authority_host"),
            "an unset authority host is not written: {yaml}"
        );
        let back: SecretManagerConnection = serde_yaml::from_str(&yaml).expect("round trip");
        assert_eq!(back, c);
    }

    #[test]
    fn azure_config_keeps_an_authority_host_override() {
        let mut c = azure_connection();
        c.config = Some(ProviderConfig::Azure {
            tenant_id: "t".to_string(),
            authority_host: Some("http://127.0.0.1:9999".to_string()),
        });
        let yaml = serde_yaml::to_string(&c).expect("serialize");
        let back: SecretManagerConnection = serde_yaml::from_str(&yaml).expect("round trip");
        assert_eq!(back, c);
    }

    #[test]
    fn azure_config_without_a_tenant_does_not_load() {
        let yaml = "id: c1\nlabel: X\nbase_url: https://v\nclient_id: a\nprovider: azure\nconfig: !Azure {}\n";
        let err = serde_yaml::from_str::<SecretManagerConnection>(yaml)
            .expect_err("a missing tenant must not default to empty");
        assert!(err.to_string().contains("tenant_id"), "got: {err}");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-environment secret_manager`
Expected: FAIL to compile, "no variant named `Azure` found for enum `ProviderConfig`".

- [ ] **Step 3: Add the variant**

Replace the enum and its doc comment (lines 40-45) with:

```rust
/// Typed, non-secret settings for one provider (tenant, region, project and
/// so on). Each provider's own spec adds its variant. Persisted as a serde
/// YAML tag, and never renamed to camelCase: IPC uses its own DTO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderConfig {
    /// Azure AD service principal settings. The vault URL is the connection's
    /// `base_url` and the app registration id is its `client_id`.
    Azure {
        tenant_id: String,
        /// Overrides `https://login.microsoftonline.com`. Used by tests.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authority_host: Option<String>,
    },
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-environment secret_manager`
Expected: PASS, including the older RocketVault serde tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-environment/src/secret_manager.rs
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(secrets): add the Azure provider config variant`.

---

### Task 2: camelCase IPC DTO for `config`

**Files:**
- Modify: `src-tauri/src/commands/secret_managers.rs:1-54` and its `tests` module

**Interfaces:**
- Consumes: `rocket_environment::ProviderConfig::Azure`.
- Produces: `ProviderConfigDto` (see the index) and `SecretManagerConnectionDto.config: Option<ProviderConfigDto>`.

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` in `src-tauri/src/commands/secret_managers.rs`:

```rust
    #[test]
    fn config_dto_is_tagged_and_camel_case() {
        let dto = ProviderConfigDto::from(ProviderConfig::Azure {
            tenant_id: "tenant-1".to_string(),
            authority_host: Some("http://127.0.0.1:1".to_string()),
        });
        let json = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "azure",
                "tenantId": "tenant-1",
                "authorityHost": "http://127.0.0.1:1"
            })
        );
    }

    #[test]
    fn config_dto_omits_an_unset_authority_host() {
        let dto = ProviderConfigDto::from(ProviderConfig::Azure {
            tenant_id: "tenant-1".to_string(),
            authority_host: None,
        });
        let json = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({ "kind": "azure", "tenantId": "tenant-1" })
        );
    }

    #[test]
    fn azure_connection_dto_round_trips_its_config() {
        let json = serde_json::json!({
            "id": "c1",
            "label": "L",
            "baseUrl": "https://v.vault.azure.net",
            "clientId": "app",
            "verifySsl": true,
            "allowInsecureHttp": false,
            "provider": "azure",
            "config": { "kind": "azure", "tenantId": "t1" }
        });
        let dto: SecretManagerConnectionDto =
            serde_json::from_value(json.clone()).expect("deserialize");
        let conn: SecretManagerConnection = dto.into();
        assert_eq!(
            conn.config,
            Some(ProviderConfig::Azure {
                tenant_id: "t1".to_string(),
                authority_host: None
            })
        );
        let back = serde_json::to_value(SecretManagerConnectionDto::from(conn)).expect("serialize");
        assert_eq!(back, json);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket secret_managers`
Expected: FAIL to compile, "cannot find type `ProviderConfigDto`".

- [ ] **Step 3: Add the DTO and use it**

In `src-tauri/src/commands/secret_managers.rs`, insert before `SecretManagerConnectionDto`:

```rust
/// IPC shape of `ProviderConfig`. Kept apart from the persistence enum so the
/// camelCase rename never reaches `secret_managers.yml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ProviderConfigDto {
    #[serde(rename = "azure", rename_all = "camelCase")]
    Azure {
        tenant_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authority_host: Option<String>,
    },
}

impl From<ProviderConfig> for ProviderConfigDto {
    fn from(config: ProviderConfig) -> Self {
        match config {
            ProviderConfig::Azure {
                tenant_id,
                authority_host,
            } => Self::Azure {
                tenant_id,
                authority_host,
            },
        }
    }
}

impl From<ProviderConfigDto> for ProviderConfig {
    fn from(dto: ProviderConfigDto) -> Self {
        match dto {
            ProviderConfigDto::Azure {
                tenant_id,
                authority_host,
            } => Self::Azure {
                tenant_id,
                authority_host,
            },
        }
    }
}
```

Then change the `config` field of `SecretManagerConnectionDto` to `pub config: Option<ProviderConfigDto>,` and the two conversions to:

```rust
            config: c.config.map(Into::into),
```
(in `From<SecretManagerConnection>`) and
```rust
            config: dto.config.map(Into::into),
```
(in `From<SecretManagerConnectionDto>`).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket secret_managers`
Expected: PASS, including the older DTO tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/secret_managers.rs
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(secrets): add a camelCase DTO for provider config`.

---

### Task 3: Provider-change guard, credential test and file-read hint

**Files:**
- Modify: `crates/rocket-app/src/secret_manager_service.rs:46-52` (`save`) and its `tests` module
- Modify: `crates/rocket-infra/src/fs_secret_manager_repo.rs:40-42` (`read_all`) and its `tests` module

**Interfaces:**
- Consumes: `ProviderConfig::Azure`.
- Produces: `SecretManagerService::save` returns `DomainError::InvalidInput` when an existing id changes provider.

- [ ] **Step 1: Write the failing service tests**

In `mod tests` of `secret_manager_service.rs`, add the import `use rocket_environment::secret_manager::ProviderConfig;` next to the other `use` lines, then add:

```rust
    fn azure_connection(id: &str) -> SecretManagerConnection {
        SecretManagerConnection {
            label: "Prod Azure".to_string(),
            base_url: "https://prod-kv.vault.azure.net".to_string(),
            client_id: "app-id".to_string(),
            provider: SecretProviderKind::Azure,
            config: Some(ProviderConfig::Azure {
                tenant_id: "tenant-1".to_string(),
                authority_host: None,
            }),
            ..sample_connection(id)
        }
    }

    #[test]
    fn save_rejects_changing_the_provider_of_an_existing_connection() {
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        service
            .save(sample_connection("conn-1"), Some("original".to_string()))
            .expect("initial RocketVault save");

        let result = service.save(azure_connection("conn-1"), Some("attacker".to_string()));

        assert!(
            matches!(result, Err(DomainError::InvalidInput(_))),
            "expected InvalidInput, got {result:?}"
        );
        assert_eq!(
            store.get("vault-connection", "conn-1").expect("get"),
            Some("original".to_string()),
            "a rejected save must not overwrite the stored credential"
        );
        let listed = service.list().expect("list");
        assert_eq!(listed[0].provider, SecretProviderKind::RocketVault);
    }

    #[test]
    fn save_keeps_allowing_edits_that_do_not_change_the_provider() {
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let mut conn = azure_connection("az-1");
        service
            .save(conn.clone(), Some("s".to_string()))
            .expect("first save");
        conn.label = "Renamed".to_string();
        service.save(conn, None).expect("edit keeps the provider");
    }

    #[test]
    fn a_credential_required_non_rocketvault_connection_needs_a_secret_on_save() {
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );

        let result = service.save(azure_connection("az-1"), None);

        assert!(
            matches!(result, Err(DomainError::InvalidInput(_))),
            "got {result:?}"
        );
        assert!(service.list().expect("list").is_empty());
    }

    #[tokio::test]
    async fn a_credential_required_connection_with_no_stored_secret_fails_at_connection_time() {
        let repo = FakeRepo::new();
        repo.save(&azure_connection("az-1")).expect("seed the record directly");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(FakeSecretStore::new()) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );

        let result = service.test_connection("az-1", "any").await;

        match result {
            Err(DomainError::Internal(msg)) => {
                assert!(msg.contains("no client secret"), "got: {msg}")
            }
            other => panic!("expected an Internal error, got {other:?}"),
        }
    }
```

Add one more test in the same module, pinning that an Azure connection reaches certificate gating as a provider without certificate support (the fake fetcher reports the default capabilities, Plan 02 pins Azure's own):

```rust
    #[test]
    fn an_azure_connection_reaches_certificate_gating_without_certificate_support() {
        let repo = FakeRepo::new();
        repo.save(&azure_connection("az-1")).expect("seed the record");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(FakeSecretStore::new()) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );

        let provider = rocket_environment::ProviderCapabilityLookup::provider_of(&service, "az-1")
            .expect("lookup")
            .expect("the connection exists");

        assert_eq!(provider.kind, SecretProviderKind::Azure);
        assert!(!provider.capabilities.certificates);
    }
```

- [ ] **Step 2: Run to verify the guard test fails**

Run: `cargo test -j4 -p rocket-app secret_manager_service`
Expected: `save_rejects_changing_the_provider_of_an_existing_connection` FAILS (the save succeeds and overwrites the credential). The other tests pass already, they pin existing behavior.

- [ ] **Step 3: Add the guard**

In `SecretManagerService::save`, directly after `validate_connection(&connection, self.fetcher.as_ref())?;` add:

```rust
        // A saved connection keeps its provider. Otherwise a stored credential
        // could be sent to a different provider by an edit over IPC.
        if let Some(existing) = self.repo.get(&connection.id)? {
            if existing.provider != connection.provider {
                return Err(DomainError::InvalidInput(format!(
                    "the provider of connection {} cannot be changed from {} to {}",
                    connection.id,
                    existing.provider.display_name(),
                    connection.provider.display_name()
                )));
            }
        }
```

- [ ] **Step 4: Write the failing repo test**

In `mod tests` of `fs_secret_manager_repo.rs` add:

```rust
    #[test]
    fn an_unreadable_file_points_at_a_newer_build() {
        let (dir, repo) = setup();
        std::fs::write(
            dir.path().join("secret_managers.yml"),
            "- id: x\n  label: X\n  base_url: https://v\n  client_id: a\n  provider: azure\n  config: !FutureProvider {region: eu}\n",
        )
        .expect("write");

        let err = repo.list().expect_err("an unknown config tag must not load");

        let msg = err.to_string();
        assert!(msg.contains("newer version of Rocket"), "got: {msg}");
        assert!(msg.contains("secret_managers.yml"), "got: {msg}");
    }
```

Run: `cargo test -j4 -p rocket-infra fs_secret_manager_repo`
Expected: the new test FAILS on the missing "newer version of Rocket" text.

- [ ] **Step 5: Add the hint**

In `read_all`, change the parse error mapping to:

```rust
        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::InvalidInput(format!(
                "Failed to parse secret_managers.yml: {e}. The file may have been written by a newer version of Rocket."
            ))
        })
```

The existing test `a_row_with_an_unknown_provider_fails_loudly_and_names_it` still passes, the provider name stays in the message.

- [ ] **Step 6: Run both suites**

Run: `cargo test -j4 -p rocket-app secret_manager_service && cargo test -j4 -p rocket-infra fs_secret_manager_repo`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/secret_manager_service.rs crates/rocket-infra/src/fs_secret_manager_repo.rs
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `fix(secrets): block provider changes and explain unreadable files`.

---

## Next Plan

Plan 02: [Azure fetcher and wiring](2026-10-04-azure-plan-02-fetcher.md). Start it automatically once the three tasks above are committed and `cargo check -j4` is green.
