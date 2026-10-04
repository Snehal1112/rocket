# Secret provider foundation, Plan 01: Domain types, capabilities and persistence

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the provider tag, the provider capabilities and the persistence guarantees to `rocket-environment` and `rocket-infra`, with no new provider.

**Architecture:** `SecretManagerConnection` gains a defaulted `provider` and an optional typed `config`. The `VaultSecretFetcher` trait gains defaulted `capabilities` and `validate_connection` methods. Old `secret_managers.yml` files load unchanged and RocketVault rows are written exactly as before.

**Tech Stack:** Rust, serde, serde_yaml, `cargo test -j4 -p <crate>`.

**Spec:** [../../specs/2026-10-04-secret-provider-foundation-design.md](../../specs/2026-10-04-secret-provider-foundation-design.md) (sections 4, 5.1, 12)

## Global Constraints

- Never apply `#[serde(rename_all = "camelCase")]` to persistence structs. Only IPC DTOs use camelCase.
- Every new persisted field is `#[serde(default)]` and is skipped when it holds its default, so an older build still reads the file.
- Production code never panics on bad input: use `DomainResult` and explicit error mapping, not unwrap. Tests may use `.expect("reason")`.
- Always pass `-j4` to `cargo test` and `cargo check`. Never run `cargo test --workspace`.
- Commit with conventional commits, using the `dev-workflow-skills:1-git-commit` skill, and stage by explicit path only. `crates/rocket-app/src/execution_service.rs` may carry unrelated local edits: never stage it in this plan.

## Review Focus

- A `secret_managers.yml` written before this change must load as `provider: rocketvault` with no `config` (Task 3 test).
- A row with an unknown provider string must fail with an error that names the string, not load as RocketVault (Task 3 test).
- A RocketVault connection must serialize with no `provider` and no `config` keys, so an older build can read it back (Task 1 and Task 3 tests).
- The default `capabilities()` must be all false, so a fake fetcher written before this change never claims certificate support (Task 2 test).
- `NullVaultSecretFetcher` must keep failing loudly for every call, including the new `validate_connection` (Task 2 test).

---

## Task 1: `SecretProviderKind`, `ProviderConfig` and the new connection fields

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-environment/src/secret_manager.rs`
- Modify: `crates/rocket-environment/src/lib.rs` (re-exports)
- Modify (mechanical): every Rust file that builds a `SecretManagerConnection { .. }` literal, except `src-tauri/src/commands/secret_managers.rs`, which Plan 02 handles.

**Interfaces:**
- Produces:
  - `rocket_environment::secret_manager::SecretProviderKind` (`RocketVault` default, `Azure`, `Aws`, `Hashicorp`, `Gcp`) with `is_default(&self) -> bool` and `display_name(&self) -> &'static str`.
  - `rocket_environment::secret_manager::ProviderConfig` (an enum with no variants in this plan).
  - `SecretManagerConnection.provider: SecretProviderKind` and `SecretManagerConnection.config: Option<ProviderConfig>`.

- [ ] **Step 1: Write the failing tests**

Append inside the existing `#[cfg(test)] mod tests` of `crates/rocket-environment/src/secret_manager.rs`:

```rust
    #[test]
    fn provider_defaults_to_rocketvault_when_absent_from_yaml() {
        let yaml = "id: c1\nlabel: Prod\nbase_url: https://v:8774\nclient_id: rocketapi\n";
        let c: SecretManagerConnection = serde_yaml::from_str(yaml).expect("old row parses");
        assert_eq!(c.provider, SecretProviderKind::RocketVault);
        assert!(c.config.is_none());
    }

    #[test]
    fn rocketvault_connection_serializes_without_provider_or_config() {
        let c = SecretManagerConnection {
            id: "c1".to_string(),
            label: "Prod".to_string(),
            base_url: "https://v:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::RocketVault,
            config: None,
        };
        let yaml = serde_yaml::to_string(&c).expect("serialize");
        assert!(!yaml.contains("provider"), "an older build must read this: {yaml}");
        assert!(!yaml.contains("config"), "an older build must read this: {yaml}");
    }

    #[test]
    fn non_default_provider_is_serialized_lowercase() {
        let c = SecretManagerConnection {
            id: "c2".to_string(),
            label: "Azure".to_string(),
            base_url: String::new(),
            client_id: String::new(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::Azure,
            config: None,
        };
        let yaml = serde_yaml::to_string(&c).expect("serialize");
        assert!(yaml.contains("provider: azure"), "got: {yaml}");
        let back: SecretManagerConnection = serde_yaml::from_str(&yaml).expect("round trip");
        assert_eq!(back, c);
    }

    #[test]
    fn unknown_provider_is_rejected_and_named() {
        let yaml = "id: c1\nlabel: X\nprovider: bogus\n";
        let err = serde_yaml::from_str::<SecretManagerConnection>(yaml)
            .expect_err("unknown provider must not load");
        assert!(err.to_string().contains("bogus"), "got: {err}");
    }

    #[test]
    fn provider_display_names() {
        assert_eq!(SecretProviderKind::RocketVault.display_name(), "RocketVault");
        assert_eq!(SecretProviderKind::Azure.display_name(), "Azure Key Vault");
        assert_eq!(SecretProviderKind::Aws.display_name(), "AWS Secrets Manager");
        assert_eq!(SecretProviderKind::Hashicorp.display_name(), "HashiCorp Vault");
        assert_eq!(SecretProviderKind::Gcp.display_name(), "Google Secret Manager");
    }
```

If `serde_yaml` is not already a dev-dependency of `rocket-environment`, check `crates/rocket-environment/Cargo.toml`. If it is missing, add `serde_yaml = { workspace = true }` under `[dev-dependencies]` using the version the workspace already uses (look in `crates/rocket-infra/Cargo.toml`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-environment secret_manager`
Expected: FAIL to compile (`SecretProviderKind` not found).

- [ ] **Step 3: Implement the types and fields**

In `crates/rocket-environment/src/secret_manager.rs`, add above `SecretManagerConnection`:

```rust
/// Which kind of secret manager a connection talks to. Rows written before
/// providers existed have no `provider` key and load as `RocketVault`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretProviderKind {
    #[default]
    RocketVault,
    Azure,
    Aws,
    Hashicorp,
    Gcp,
}

impl SecretProviderKind {
    /// True for the value a row gets when the key is absent. Used to keep
    /// RocketVault rows byte-identical to the format older builds read.
    pub fn is_default(&self) -> bool {
        *self == Self::RocketVault
    }

    /// The name shown to users and used in error messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::RocketVault => "RocketVault",
            Self::Azure => "Azure Key Vault",
            Self::Aws => "AWS Secrets Manager",
            Self::Hashicorp => "HashiCorp Vault",
            Self::Gcp => "Google Secret Manager",
        }
    }
}

/// Typed, non-secret settings for one provider (tenant, region, project and
/// so on). Each provider's own spec adds its variant. The foundation has
/// none, so no value of this type can exist yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderConfig {}
```

Then add the two fields to `SecretManagerConnection`, after `allow_insecure_http`:

```rust
    #[serde(default, skip_serializing_if = "SecretProviderKind::is_default")]
    pub provider: SecretProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ProviderConfig>,
```

Update the two struct literals already inside this file's tests (`connection_serde_roundtrip_no_camelcase` at about line 42 and the one at about line 102) by adding `provider: SecretProviderKind::RocketVault,` and `config: None,`.

In `crates/rocket-environment/src/lib.rs`, change the line `pub use secret_manager::{SecretManagerConnection, SecretManagerRepository};` to:

```rust
pub use secret_manager::{
    ProviderConfig, SecretManagerConnection, SecretManagerRepository, SecretProviderKind,
};
```

- [ ] **Step 4: Fix every other struct literal mechanically**

Run from the repo root:

```bash
FILES=$(grep -rl "SecretManagerConnection {" crates src-tauri/src --include='*.rs' \
  | grep -v "crates/rocket-environment/src/secret_manager.rs" \
  | grep -v "src-tauri/src/commands/secret_managers.rs")
echo "$FILES"
perl -0pi -e 's/^(\s*)allow_insecure_http:\s*([^,\n]+),\n/$1allow_insecure_http: $2,\n$1provider: Default::default(),\n$1config: None,\n/mg' $FILES
git diff --stat -- $FILES
```

Expected: about 20 literal sites changed across the files listed by `echo`. Every change is two added lines after an `allow_insecure_http:` line.

- [ ] **Step 5: Check the crates compile**

Run: `cargo check -j4 -p rocket-environment -p rocket-infra -p rocket-app --tests`
Expected: PASS. If a literal was missed, the compiler names the file and line. Add the same two lines there. Do not check `src-tauri` yet: its connection conversions are fixed in Plan 02 Task 3.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-environment secret_manager`
Expected: PASS (the five new tests and the existing ones).

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill. Stage by path:

```bash
git add crates/rocket-environment/src/secret_manager.rs crates/rocket-environment/src/lib.rs crates/rocket-environment/Cargo.toml $FILES
```

Suggested subject: `feat(secrets): add provider tag to secret manager connections`.

---

## Task 2: `ProviderCapabilities` and the defaulted trait methods

**Files:**
- Modify: `crates/rocket-environment/src/vault_secret_fetcher.rs`
- Modify: `crates/rocket-environment/src/lib.rs` (re-export)

**Interfaces:**
- Consumes: `SecretManagerConnection` (Task 1).
- Produces:
  - `rocket_environment::ProviderCapabilities { certificates: bool, credential_optional: bool, fetch_on_reference: bool }` with `Default`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Debug`.
  - On `VaultSecretFetcher`: `fn capabilities(&self, connection: &SecretManagerConnection) -> ProviderCapabilities` (default all false) and `fn validate_connection(&self, connection: &SecretManagerConnection) -> DomainResult<()>` (default `Ok(())`).

- [ ] **Step 1: Write the failing tests**

Append inside the existing `#[cfg(test)] mod tests` of `crates/rocket-environment/src/vault_secret_fetcher.rs` (it already has `dummy_connection()` and a fetcher fake named `SecretsOnlyFetcher`):

```rust
    #[test]
    fn default_capabilities_are_all_false() {
        let fetcher = SecretsOnlyFetcher;
        let caps = fetcher.capabilities(&dummy_connection());
        assert_eq!(caps, ProviderCapabilities::default());
        assert!(!caps.certificates);
        assert!(!caps.credential_optional);
        assert!(!caps.fetch_on_reference);
    }

    #[test]
    fn default_validate_connection_accepts() {
        let fetcher = SecretsOnlyFetcher;
        assert!(fetcher.validate_connection(&dummy_connection()).is_ok());
    }

    #[test]
    fn null_fetcher_still_fails_validate_connection() {
        let err = NullVaultSecretFetcher
            .validate_connection(&dummy_connection())
            .expect_err("a null fetcher must never accept a connection");
        assert!(err.to_string().contains("no vault secret fetcher configured"));
    }
```

If `SecretsOnlyFetcher` is not unit-constructible, use the same expression the existing tests in that module use to build it.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-environment vault_secret_fetcher`
Expected: FAIL to compile (`ProviderCapabilities` not found).

- [ ] **Step 3: Implement**

In `crates/rocket-environment/src/vault_secret_fetcher.rs`, add above the trait:

```rust
/// What a provider can do beyond listing and reading secrets. The default is
/// all false, so a fetcher written before a capability existed never claims it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProviderCapabilities {
    /// Can list and export client certificates (RocketVault only).
    pub certificates: bool,
    /// A new connection may be saved with no stored credential.
    pub credential_optional: bool,
    /// A single send fetches only the secrets the request references.
    pub fetch_on_reference: bool,
}
```

Add these two methods to the `VaultSecretFetcher` trait, after `forget_connection`:

```rust
    /// What this provider supports for `connection`. The default is nothing.
    fn capabilities(&self, _connection: &SecretManagerConnection) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    /// Provider-specific checks on a connection record, run on save before
    /// anything is written. The default accepts. RocketVault's own rules live
    /// in `SecretManagerService`, so only other providers override this.
    fn validate_connection(&self, _connection: &SecretManagerConnection) -> DomainResult<()> {
        Ok(())
    }
```

In `impl VaultSecretFetcher for NullVaultSecretFetcher`, add:

```rust
    fn validate_connection(&self, _connection: &SecretManagerConnection) -> DomainResult<()> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }
```

Reword the trait's doc comment so it says "secret manager" in place of "RocketVault" where it describes the contract (leave RocketVault-specific notes in place).

In `crates/rocket-environment/src/lib.rs`, add `ProviderCapabilities` to the existing `pub use vault_secret_fetcher::{ ... };` list.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-environment vault_secret_fetcher`
Expected: PASS.

- [ ] **Step 5: Check dependents still compile**

Run: `cargo check -j4 -p rocket-infra -p rocket-app --tests`
Expected: PASS. The new methods have defaults, so no implementor changes.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add crates/rocket-environment/src/vault_secret_fetcher.rs crates/rocket-environment/src/lib.rs
```

Suggested subject: `feat(secrets): add provider capabilities to the fetcher trait`.

---

## Task 3: Persistence guarantees in `FsSecretManagerRepo`

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/fs_secret_manager_repo.rs` (tests only, plus a fix if a test exposes one)

**Interfaces:**
- Consumes: `SecretProviderKind`, the new `SecretManagerConnection` fields (Task 1).
- Produces: nothing new. This task pins behavior.

- [ ] **Step 1: Read the existing tests**

Read the `#[cfg(test)]` module at the bottom of `crates/rocket-infra/src/fs_secret_manager_repo.rs` (it has `sample(id)` and uses `tempfile`). Reuse its helpers and its way of building a repo for a temp path.

- [ ] **Step 2: Write the tests**

Add inside that test module (adapt the repo constructor call to match the existing tests):

```rust
    #[test]
    fn an_old_file_with_no_provider_loads_as_rocketvault() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret_managers.yml");
        std::fs::write(
            &path,
            "- id: old-1\n  label: Prod\n  base_url: https://v:8774\n  client_id: rocketapi\n  verify_ssl: true\n  allow_insecure_http: false\n",
        )
        .expect("write old file");
        let repo = FsSecretManagerRepo::new(path);

        let rows = repo.list().expect("old file loads");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, SecretProviderKind::RocketVault);
        assert!(rows[0].config.is_none());
    }

    #[test]
    fn a_saved_rocketvault_row_has_no_provider_key_on_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret_managers.yml");
        let repo = FsSecretManagerRepo::new(path.clone());

        repo.save(&sample("c1")).expect("save");

        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(!text.contains("provider"), "older builds must read this file: {text}");
        assert!(!text.contains("config"), "older builds must read this file: {text}");
    }

    #[test]
    fn a_row_with_an_unknown_provider_fails_loudly_and_names_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret_managers.yml");
        std::fs::write(&path, "- id: x\n  label: X\n  provider: bogus\n").expect("write");
        let repo = FsSecretManagerRepo::new(path);

        let err = repo.list().expect_err("must not silently become RocketVault");

        assert!(err.to_string().contains("bogus"), "got: {err}");
    }
```

Add `use rocket_environment::SecretProviderKind;` to the test module imports if it is not already in scope.

- [ ] **Step 3: Run the tests**

Run: `cargo test -j4 -p rocket-infra fs_secret_manager_repo`
Expected: PASS for the first two. For the third, if `list()` swallows parse errors and returns an empty list, the test fails with the `expect_err` message. In that case, change the load path in `fs_secret_manager_repo.rs` so a parse error is returned as `DomainError::Internal(format!("could not read secret_managers.yml: {e}"))` (the message must contain the serde error text, which names the bad value). Do not change how a missing or zero-byte file loads: those stay an empty list.

- [ ] **Step 4: Re-run after any fix**

Run: `cargo test -j4 -p rocket-infra fs_secret_manager_repo`
Expected: PASS, including the existing tests.

- [ ] **Step 5: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add crates/rocket-infra/src/fs_secret_manager_repo.rs
```

Suggested subject: `test(secrets): pin secret_managers.yml compatibility`.

---

## Next Plan

[Plan 02: Dispatching fetcher, service validation and IPC](2026-10-04-secret-provider-plan-02-dispatch-and-service.md). It depends on this plan. Chain to it automatically when this one finishes.
