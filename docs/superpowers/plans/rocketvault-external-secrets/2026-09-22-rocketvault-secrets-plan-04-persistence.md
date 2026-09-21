# RocketVault Secrets Plan 04: FsSecretManagerRepo, Keychain Generalization, and Oc Persistence

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `rocket-infra` everything it needs to persist RocketVault Secret
Manager data: a flat-file repository for connection metadata
(`FsSecretManagerRepo`), a `KeyringSecretStore` that can serve a second,
distinct OS-keychain namespace (vault-connection client secrets) instead of
only environment secrets, and the `OcEnvironment.external_secrets` mirror plus
`From` conversions so an environment's `ExternalSecretBinding` list survives a
round trip through the OpenCollection YAML format.

**Architecture:** Three independent-but-related persistence jobs, all inside
`rocket-infra` (the only crate in this workspace that does I/O). Task 1
generalizes an existing type in place. Task 2 adds one new file following the
single-flat-file pattern already established by `FsWorkspaceRepo`
(`crates/rocket-infra/src/fs_workspace_repo.rs`) rather than the
one-file-per-entry pattern used by `FsHistoryRepo`/`FsCookieRepo`. Task 3
extends the existing `OcEnvironment` serde struct and its `From` conversions,
following the exact pattern `OcEnvVariableEntry`/`Variable` already establish
for a nested, camelCase Oc-layer mirror of a domain type.

**Tech Stack:** Rust, serde, serde_yaml, the `keyring` crate, `tempfile` (tests).

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.3 Secret Manager connections, §4.4 `ExternalSecretBinding` on
`Environment`). Plan index:
`docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md` (has
the full locked interface contract every plan in this series depends on,
including this plan's exact `FsSecretManagerRepo`/`KeyringSecretStore`
signatures under "`rocket-infra` (new, Plan 04)").

## Global Constraints

- **Depends on Plan 01** (`crates/rocket-environment/src/secret_manager.rs`:
  `SecretManagerConnection`, `SecretManagerRepository`;
  `crates/rocket-environment/src/external_secret.rs`: `ExternalSecretBinding`,
  `ExternalSecretRef`; `Environment.external_secrets`). This plan does not
  implement those — it consumes them. If Plan 01 has not landed yet in the
  branch you're working from, stop and run it first.
- `SecretManagerConnection` has **no** `client_secret` field, by design (see
  Plan 01's doc comment on the struct) — the client secret lives only in the
  OS keychain, looked up by connection `id` (Plan 05's `SecretManagerService`
  wires `KeyringSecretStore::new_vault_connections()` to that lookup). Nothing
  in this plan should introduce a way for a client secret to reach disk.
- `ExternalSecretRef`/`ExternalSecretBinding` (and their Oc-layer mirrors
  added in this plan, `OcExternalSecretRef`/`OcExternalSecretBinding`) never
  carry a secret *value* — only names and RocketVault's own UUIDs, per spec
  §4.4. This is a structural guarantee (no such field exists), not a runtime
  check, but Task 3's round-trip tests still assert on it where practical.
- Every new field added to an existing persisted struct (`OcEnvironment`)
  must use `#[serde(default, skip_serializing_if = "Vec::is_empty")]` (for a
  `Vec` field) so every existing environment `.yml` file — none of which have
  an `externalSecrets` key — keeps loading, and so environments that don't
  use the feature don't grow a stray `externalSecrets: []` in their YAML.
  This matches `OcEnvironment.client_certificates`'s existing convention
  exactly (`crates/rocket-infra/src/oc/environment.rs:20-21`).
- `OcExternalSecretRef`/`OcExternalSecretBinding` use
  `#[serde(rename_all = "camelCase")]` — they nest inside `OcEnvironment`,
  which already uses camelCase for its own top-level fields (`clientCertificates`,
  `dotEnvFilePath`, etc. — see `crates/rocket-infra/src/oc/environment.rs:11`).
  `OcVariable` itself (`crates/rocket-infra/src/oc/variables.rs:12-22`) has no
  `rename_all` attribute of its own, but its fields (`name`, `value`,
  `initial`, `description`, `disabled`) are all single words, so plain and
  camelCase serialization are indistinguishable for it — it is not evidence
  against using camelCase here. `OcEnvironment`'s own struct-level attribute
  is the convention that actually governs multi-word field names in this
  file, and this plan follows it.
- `SecretManagerConnection` itself does **not** get a camelCase rename (Plan
  01's choice, unchanged here) — it persists to its own app-level
  `secret_managers.yml`, not the OpenCollection format, so the codebase's
  general "no camelCase on persistence structs" rule applies to it. This
  plan's Task 2 (`FsSecretManagerRepo`) persists that struct as-is; it does
  not introduce an Oc-layer mirror for it (there isn't one — it's not part of
  a collection or environment).
- Rust: production code paths must never call the panicking `Result`/`Option`
  unwrap method (this crate's hard rule, and the repo-wide hard rule in
  `CLAUDE.md`). Test code in this plan uses `.expect("message")` for fallible
  calls instead, matching this repository's stricter safety convention even
  in test paths.
- No git CLI shell-outs — not relevant to this plan's scope, listed for
  completeness per `crates/rocket-infra`'s DDD boundary rules.
- 📖 Before starting Task 3 (it touches the `Environment`/`OcEnvironment`
  data model), read
  `docs/superpowers/specs/opencollection-spec-reference.md`, per this repo's
  OpenCollection injection rule.

---

## Task 1: Generalize `KeyringSecretStore` to a configurable service label

**Files:**
- Modify: `crates/rocket-infra/src/secret_store.rs`
- Modify: `src-tauri/src/lib.rs:27-35` (the `env_secret_store()` helper and
  its doc comment)

**Interfaces:**
- Consumes: nothing new — `rocket_environment::secret_store::SecretStore`
  (the trait `KeyringSecretStore` already implements) is unchanged.
- Produces: `KeyringSecretStore::new_env_secrets() -> Self` and
  `KeyringSecretStore::new_vault_connections() -> Self` — consumed by Plan
  05's `SecretManagerService` (`new_vault_connections()`, for
  connection client-secret storage) and by `src-tauri/src/lib.rs`'s existing
  `env_secret_store()` helper (`new_env_secrets()`, behavior-preserving).

Today, `crates/rocket-infra/src/secret_store.rs` has a module-level
`const KEYRING_SERVICE: &str = "com.rocketapi.env-secrets";` and a unit struct
`pub struct KeyringSecretStore;` whose three `SecretStore` trait methods
(`get`/`set`/`delete`) each build `keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))`.
This task turns the hardcoded constant into a per-instance field so the same
type can back a second keychain namespace (RocketVault connection client
secrets, wired up in Plan 05) without colliding with environment secrets.

- [ ] **Step 1: Write the failing test**

Add this test to the existing `#[cfg(test)] mod tests` block in
`crates/rocket-infra/src/secret_store.rs` (alongside `account_namespaces_scope_and_key`):

```rust
#[test]
fn distinct_keychain_namespaces_for_env_and_vault_connections() {
    let env_store = KeyringSecretStore::new_env_secrets();
    let vault_store = KeyringSecretStore::new_vault_connections();
    // Two genuinely distinct keychain service labels — sharing one would let
    // an environment-secret entry collide with a vault-connection entry (or
    // vice versa) if the same scope_id/key pair were ever reused across
    // features. No real OS keychain is touched here; this only inspects the
    // struct's own field, the same way `service_name_is_distinct_from_git_credentials`
    // below checks a string constant without touching a keychain.
    assert_ne!(env_store.service, vault_store.service);
    assert_eq!(env_store.service, "com.rocketapi.env-secrets");
    assert_eq!(vault_store.service, "com.rocketapi.vault-connection");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rocket-infra secret_store::tests::distinct_keychain_namespaces`
Expected: FAIL to compile — `no function or associated item named
'new_env_secrets' found for struct 'KeyringSecretStore'`.

- [ ] **Step 3: Generalize the struct, impl block, and trait impl**

Replace the module-level constant and unit struct (lines 7-13 of the current
file) with:

```rust
/// Stores secret values in the OS-native secret store: macOS Keychain,
/// Windows Credential Manager, or the Linux Secret Service. Configurable by
/// keychain service label so one implementation serves multiple secret
/// namespaces (environment variable secrets, RocketVault connection client
/// secrets) without their entries colliding.
pub struct KeyringSecretStore {
    service: &'static str,
}

impl KeyringSecretStore {
    /// Backs environment variable secret values. Uses the exact service
    /// string this type used before it was generalized to take a
    /// configurable label, so existing keychain entries keep resolving
    /// unchanged.
    pub fn new_env_secrets() -> Self {
        Self {
            service: "com.rocketapi.env-secrets",
        }
    }

    /// Backs RocketVault connection client secrets (Plan 05's
    /// `SecretManagerService`, scope_id = "vault-connection", key =
    /// connection id). A distinct keychain service label from
    /// `new_env_secrets()` so the two features can never share or clobber
    /// each other's entries.
    pub fn new_vault_connections() -> Self {
        Self {
            service: "com.rocketapi.vault-connection",
        }
    }
}
```

(This removes the now-unused module-level `KEYRING_SERVICE` constant
entirely rather than keeping it as a shared literal referenced from
`new_env_secrets()` — with only two call sites, each owning its own inline
literal keeps the diff smallest and avoids a constant that would otherwise
need a comment explaining why only one of the two constructors uses it.)

Then, in the same file, update all three `SecretStore` trait methods to read
`self.service` instead of the deleted `KEYRING_SERVICE` constant:

```rust
impl SecretStore for KeyringSecretStore {
    fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
        let entry = keyring::Entry::new(self.service, &account(scope_id, key))
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
        let entry = keyring::Entry::new(self.service, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        entry
            .set_password(value)
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
        let entry = keyring::Entry::new(self.service, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(DomainError::Internal(e.to_string())),
        }
    }
}
```

The `account()` free function and its doc comment are unchanged.

- [ ] **Step 4: Fix the three pre-existing tests that construct `KeyringSecretStore` as a unit value**

These three tests in the same file's `#[cfg(test)] mod tests` block reference
either the deleted `KEYRING_SERVICE` constant or the old unit-struct
construction `KeyringSecretStore` (with no parens/braces); they will not
compile until updated. Their assertions are otherwise unchanged — this is an
API-surface update, not a behavior change:

```rust
#[test]
fn service_name_is_distinct_from_git_credentials() {
    // Git credentials use "rocket-api" (src-tauri/src/commands/git.rs:185).
    // Sharing a service name would let one feature clobber the other's entries.
    let store = KeyringSecretStore::new_env_secrets();
    assert_ne!(store.service, "rocket-api");
    assert_eq!(store.service, "com.rocketapi.env-secrets");
}
```

```rust
#[test]
#[ignore = "requires a real OS keychain"]
fn keyring_set_get_delete_roundtrip() {
    let store = KeyringSecretStore::new_env_secrets();
    let scope = "rocket-infra-test-scope";
    store.set(scope, "TEST_KEY", "sk-live-123").expect("set");
    assert_eq!(
        store.get(scope, "TEST_KEY").expect("get"),
        Some("sk-live-123".to_string())
    );
    store.delete(scope, "TEST_KEY").expect("delete");
    assert_eq!(
        store.get(scope, "TEST_KEY").expect("get after delete"),
        None
    );
}

#[test]
#[ignore = "requires a real OS keychain"]
fn keyring_delete_of_missing_entry_is_ok() {
    assert!(KeyringSecretStore::new_env_secrets()
        .delete("rocket-infra-test-scope", "NO_SUCH_KEY")
        .is_ok());
}
```

`account_namespaces_scope_and_key` (the remaining pre-existing test) does not
reference `KeyringSecretStore` or `KEYRING_SERVICE` at all — leave it
untouched.

- [ ] **Step 5: Update the one call site in `src-tauri/src/lib.rs`**

Open `src-tauri/src/lib.rs`. It currently has, around lines 27-35:

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

Change it to:

```rust
/// OS-keychain backend for environment secret values.
///
/// `KeyringSecretStore::new_env_secrets()` builds a handle scoped to the
/// environment-secrets keychain namespace; this helper keeps the concrete
/// type and that namespace choice in one place. Every `FsEnvironmentRepo`
/// that serves user-facing environments must be built with it —
/// `FsEnvironmentRepo::new` silently drops secret values.
pub(crate) fn env_secret_store() -> Arc<dyn SecretStore> {
    Arc::new(KeyringSecretStore::new_env_secrets())
}
```

This is the only call site in the codebase (confirmed by
`grep -rn "KeyringSecretStore" src-tauri/src crates/rocket-infra/src`); the
two `FsEnvironmentRepo::with_secret_store(...)` call sites at
`src-tauri/src/lib.rs:221` and `:240` both go through this helper and need no
changes of their own. The keychain service string passed for environment
secrets is identical to before (`"com.rocketapi.env-secrets"`), so this is
behavior-preserving for every existing user's keychain entries.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-infra secret_store::tests`
Expected: PASS — 3 runnable tests (`account_namespaces_scope_and_key`,
`service_name_is_distinct_from_git_credentials`,
`distinct_keychain_namespaces_for_env_and_vault_connections`); the two
`#[ignore]`d real-keychain tests are skipped by default, which is expected —
run them locally with `cargo test -p rocket-infra keyring_ -- --ignored` if
you have a working OS keychain, to confirm the roundtrip still works with the
generalized constructor.

Then run: `cargo check -p rocket-infra -p rocket-app`
Expected: clean — confirms the `src-tauri` crate's caller and every other
`Arc<dyn SecretStore>` consumer still type-checks. (`cargo check -p src-tauri`
if you want to also compile-check the Tauri crate itself; it pulls in
platform-specific dependencies so it's slower and not strictly required to
validate this task's change.)

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/src/secret_store.rs src-tauri/src/lib.rs
git commit -m "refactor(infra): generalize KeyringSecretStore to a configurable service label"
```

---

## Task 2: `FsSecretManagerRepo`

**Files:**
- Create: `crates/rocket-infra/src/fs_secret_manager_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs` (register the module)

**Interfaces:**
- Consumes: `rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository}`
  (Plan 01).
- Produces: `FsSecretManagerRepo::new(path: PathBuf) -> Self`, implementing
  `SecretManagerRepository` — consumed by Plan 05's `SecretManagerService`
  (`repo: Box<dyn SecretManagerRepository>`) and by Plan 08's Tauri command
  wiring (constructs the concrete `FsSecretManagerRepo` pointed at
  `<app_data_dir>/secret_managers.yml`, mirroring how `FsWorkspaceRepo::new`
  is pointed at `<app_data_dir>/workspaces.yml` in `src-tauri/src/lib.rs`).

Unlike `FsHistoryRepo`/`FsCookieRepo` (one file per entry under a directory),
this repo stores its entire list as a single flat YAML file — the same
pattern `FsWorkspaceRepo` (`crates/rocket-infra/src/fs_workspace_repo.rs`)
already uses for `workspaces.yml`, adapted from "one registry object" to "one
`Vec` of connections" since `SecretManagerRepository`'s trait shape
(`list`/`get`/`save`/`delete`, one connection at a time) is closer to a
CRUD-over-a-list contract than a single-document load/save.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-infra/src/fs_secret_manager_repo.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsSecretManagerRepo) {
        let dir = TempDir::new().expect("create temp dir");
        let repo = FsSecretManagerRepo::new(dir.path().join("secret_managers.yml"));
        (dir, repo)
    }

    fn sample(id: &str) -> SecretManagerConnection {
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
    fn list_on_missing_file_returns_empty() {
        let (_dir, repo) = setup();
        assert_eq!(repo.list().expect("list"), Vec::new());
    }

    #[test]
    fn get_on_missing_file_returns_none() {
        let (_dir, repo) = setup();
        assert_eq!(repo.get("conn-1").expect("get"), None);
    }

    #[test]
    fn save_get_list_delete_roundtrip() {
        let (_dir, repo) = setup();
        let conn = sample("conn-1");
        repo.save(&conn).expect("save");

        assert_eq!(repo.get("conn-1").expect("get"), Some(conn.clone()));
        assert_eq!(repo.list().expect("list"), vec![conn.clone()]);

        repo.delete("conn-1").expect("delete");
        assert_eq!(repo.get("conn-1").expect("get after delete"), None);
        assert!(repo.list().expect("list after delete").is_empty());
    }

    #[test]
    fn save_replaces_existing_entry_with_same_id_instead_of_duplicating() {
        let (_dir, repo) = setup();
        repo.save(&sample("conn-1")).expect("save first");

        let mut updated = sample("conn-1");
        updated.label = "Renamed".to_string();
        repo.save(&updated).expect("save update");

        let all = repo.list().expect("list");
        assert_eq!(all.len(), 1, "same id must replace, not append");
        assert_eq!(all[0].label, "Renamed");
    }

    #[test]
    fn save_appends_distinct_ids() {
        let (_dir, repo) = setup();
        repo.save(&sample("conn-1")).expect("save conn-1");
        repo.save(&sample("conn-2")).expect("save conn-2");
        assert_eq!(repo.list().expect("list").len(), 2);
    }

    #[test]
    fn delete_of_missing_id_is_a_no_op() {
        let (_dir, repo) = setup();
        repo.save(&sample("conn-1")).expect("save");
        repo.delete("no-such-id").expect("delete of missing id must not error");
        assert_eq!(repo.list().expect("list").len(), 1);
    }

    #[test]
    fn persisted_yaml_never_contains_client_secret() {
        // `SecretManagerConnection` (rocket-environment, Plan 01) has no
        // `client_secret` field — the client secret lives only in the OS
        // keychain (Plan 05's `SecretManagerService`, via
        // `KeyringSecretStore::new_vault_connections()`). This is really a
        // compile-time guarantee since the struct has no such field to leak;
        // this test is a defensive regression guard so a future edit that
        // added a secret-bearing field to `SecretManagerConnection` would be
        // caught here, at the point it would first reach disk in cleartext.
        let (dir, repo) = setup();
        repo.save(&sample("conn-1")).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("secret_managers.yml"))
            .expect("read persisted file");
        assert!(
            !raw.contains("client_secret"),
            "secret_managers.yml must never contain a client_secret field: {raw}"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra fs_secret_manager_repo::tests`
Expected: FAIL to compile — `crate::fs_secret_manager_repo` doesn't exist yet
(the module isn't registered, and `FsSecretManagerRepo`/`SecretManagerConnection`
aren't in scope).

- [ ] **Step 3: Implement `FsSecretManagerRepo`**

Add this above the `#[cfg(test)]` block in the same file:

```rust
//! Filesystem persistence for RocketVault Secret Manager connections.
//! Stores the full connection list as one flat YAML file, matching the
//! pattern `FsWorkspaceRepo` uses for `workspaces.yml`. Client secrets never
//! appear here — `SecretManagerConnection` has no such field; they live only
//! in the OS keychain (see `KeyringSecretStore::new_vault_connections`,
//! Plan 05's `SecretManagerService`).

use std::fs;
use std::path::PathBuf;

use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

pub struct FsSecretManagerRepo {
    path: PathBuf,
}

impl FsSecretManagerRepo {
    /// `path` should point directly at the YAML file (e.g.
    /// `<app_data_dir>/secret_managers.yml`), not a containing directory —
    /// matching `FsWorkspaceRepo::new`'s convention for `workspaces.yml`.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read_all(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.path)
            .map_err(|e| DomainError::Io(format!("Failed to read secret_managers.yml: {e}")))?;
        if content.trim().is_empty() {
            // A zero-byte file (e.g. left behind by an interrupted first
            // write) is not an error — treat it the same as "missing".
            return Ok(Vec::new());
        }
        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to parse secret_managers.yml: {e}"))
        })
    }

    fn write_all(&self, connections: &[SecretManagerConnection]) -> DomainResult<()> {
        let yaml = serde_yaml::to_string(connections).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to serialize secret_managers.yml: {e}"))
        })?;
        atomic_write(&self.path, yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write secret_managers.yml: {e}")))
    }
}

impl SecretManagerRepository for FsSecretManagerRepo {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        self.read_all()
    }

    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
        Ok(self.read_all()?.into_iter().find(|c| c.id == id))
    }

    fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
        let mut connections = self.read_all()?;
        connections.retain(|c| c.id != connection.id);
        connections.push(connection.clone());
        self.write_all(&connections)
    }

    fn delete(&self, id: &str) -> DomainResult<()> {
        let mut connections = self.read_all()?;
        connections.retain(|c| c.id != id);
        self.write_all(&connections)
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/rocket-infra/src/lib.rs`, add the module declaration alphabetically
next to `fs_repository_path_resolver`:

```rust
pub mod fs_repository_path_resolver;
pub mod fs_secret_manager_repo;
pub mod fs_template_repo;
```

And the re-export alongside the other `Fs*Repo` re-exports:

```rust
pub use fs_repository_path_resolver::FsRepositoryPathResolver;
pub use fs_secret_manager_repo::FsSecretManagerRepo;
pub use fs_template_repo::FsTemplateRepo;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-infra fs_secret_manager_repo::tests`
Expected: PASS — 7 tests.

Then run: `cargo check -p rocket-infra`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/fs_secret_manager_repo.rs crates/rocket-infra/src/lib.rs
git commit -m "feat(infra): add FsSecretManagerRepo for Secret Manager connection persistence"
```

---

## Task 3: `OcEnvironment.external_secrets` + conversions

**Files:**
- Modify: `crates/rocket-infra/src/oc/environment.rs`
- Modify: `crates/rocket-infra/src/conversions/environment.rs`
- Modify: `crates/rocket-infra/src/oc/mod.rs` (add two backward-compatibility
  / camelCase tests alongside the existing `oc_environment_yaml` test)
- Modify: `crates/rocket-infra/src/conversions/tests.rs` (add the domain
  round-trip tests alongside the existing `environment_roundtrip` test; also
  fix pre-existing `Environment { .. }`/`OcEnvironment { .. }` struct literals
  that the compiler will flag once the new field exists — see Step 4)

**Interfaces:**
- Consumes: `rocket_environment::external_secret::{ExternalSecretBinding, ExternalSecretRef}`
  and `Environment.external_secrets: Vec<ExternalSecretBinding>` (both Plan 01).
- Produces: `OcExternalSecretRef { name, secretId }`,
  `OcExternalSecretBinding { alias, connectionId, vaultName, secretNames }`,
  `OcEnvironment.external_secrets: Vec<OcExternalSecretBinding>`, and the
  `From` conversions between them and their domain counterparts — consumed by
  `FsEnvironmentRepo` (unchanged in this plan; it already round-trips
  `Environment`/`OcEnvironment` via these same `From` impls for every other
  field) and, downstream, by Plan 10's frontend `ExternalSecretsTab`, which
  reads/writes `external_secrets` through the existing
  `get_environment`/`save_environment` Tauri commands.

> 📖 Before starting this task, read
> `docs/superpowers/specs/opencollection-spec-reference.md` (touches the
> `Environment`/`OcEnvironment` data model, per this repo's OpenCollection
> injection rule).

- [ ] **Step 1: Write the failing Oc-layer tests**

Add these two tests to the existing `#[cfg(test)] mod tests` block in
`crates/rocket-infra/src/oc/mod.rs`, alongside `oc_environment_yaml` (the
existing test for `OcEnvironment` parsing):

```rust
#[test]
fn oc_environment_external_secrets_absent_is_backward_compatible() {
    // Every environment .yml file written before this feature existed has
    // no `externalSecrets` key at all. It must keep loading unchanged.
    let yaml = "name: production\nvariables: []\n";
    let env: OcEnvironment = serde_yaml::from_str(yaml).expect("parse old-format environment");
    assert!(env.external_secrets.is_empty());
}

#[test]
fn oc_external_secret_binding_yaml_is_camel_case() {
    let yaml = r#"
name: production
externalSecrets:
  - alias: payments
    connectionId: conn-1
    vaultName: prod-vault
    secretNames:
      - name: stripe-key
        secretId: b6f1c2e0-1234-4a5b-9abc-000000000001
"#;
    let env: OcEnvironment = serde_yaml::from_str(yaml).expect("parse environment with external secrets");
    assert_eq!(env.external_secrets.len(), 1);
    let binding = &env.external_secrets[0];
    assert_eq!(binding.alias, "payments");
    assert_eq!(binding.connection_id, "conn-1");
    assert_eq!(binding.vault_name, "prod-vault");
    assert_eq!(binding.secret_names.len(), 1);
    assert_eq!(binding.secret_names[0].name, "stripe-key");
    assert_eq!(
        binding.secret_names[0].secret_id,
        "b6f1c2e0-1234-4a5b-9abc-000000000001"
    );

    // Round-trip and confirm the serialized form is camelCase, not snake_case.
    let out = serde_yaml::to_string(&env).expect("serialize environment");
    assert!(out.contains("connectionId:"), "expected camelCase, got:\n{out}");
    assert!(out.contains("vaultName:"), "expected camelCase, got:\n{out}");
    assert!(out.contains("secretNames:"), "expected camelCase, got:\n{out}");
    assert!(out.contains("secretId:"), "expected camelCase, got:\n{out}");
    assert!(!out.contains("connection_id:"), "must not emit snake_case:\n{out}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra oc::tests::oc_environment_external_secrets`
Run: `cargo test -p rocket-infra oc::tests::oc_external_secret_binding`
Expected: both FAIL to compile — `no field 'external_secrets' on type
'OcEnvironment'`.

- [ ] **Step 3: Add the Oc-layer types and field**

In `crates/rocket-infra/src/oc/environment.rs`, add these two new structs
above `OcEnvironment` (this file has no `secret_id`/`connection_id`/etc.
fields to conflict with, so naming is unambiguous):

```rust
/// Mirrors `rocket_environment::external_secret::ExternalSecretRef` for the
/// OpenCollection YAML format. Never carries a secret *value* — only a
/// captured name and RocketVault's own UUID (see spec §4.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcExternalSecretRef {
    pub name: String,
    pub secret_id: String,
}

/// Mirrors `rocket_environment::external_secret::ExternalSecretBinding` for
/// the OpenCollection YAML format. A new top-level section on
/// `OcEnvironment` (`externalSecrets`), not routed through
/// `OcVariable`/`OcSecretVariable` — this is not a variable entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcExternalSecretBinding {
    pub alias: String,
    pub connection_id: String,
    pub vault_name: String,
    #[serde(default)]
    pub secret_names: Vec<OcExternalSecretRef>,
}
```

Then add the new field to the `OcEnvironment` struct, immediately after
`variables` (so the OpenCollection-mirroring fields stay grouped ahead of the
client-certificate/inheritance fields):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcEnvironment {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<OcDescription>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<OcEnvVariableEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_secrets: Vec<OcExternalSecretBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_certificates: Vec<OcClientCertificate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dot_env_file_path: Option<String>,
}
```

- [ ] **Step 4: Fix every other `OcEnvironment { .. }` struct literal in this crate**

Once `external_secrets` is a required struct field, the compiler will flag
every existing `OcEnvironment { .. }` literal that doesn't set it. Search for
them:

```bash
grep -rn "OcEnvironment {" crates/rocket-infra/src
```

At minimum, `crates/rocket-infra/src/conversions/tests.rs` has two
(`environment_oc_to_domain` around line 434 and
`environment_oc_secret_entry_converts_back_with_secret_flag_set` around line
520). Add `external_secrets: Vec::new(),` to each (right after `variables:
...,`, matching the field order chosen in Step 3). `cargo check -p
rocket-infra` after this step is the reliable way to confirm every site is
caught — trust the compiler over this grep for completeness.

- [ ] **Step 5: Run the Oc-layer tests to verify they pass**

Run: `cargo test -p rocket-infra oc::tests`
Expected: PASS — all pre-existing `oc::tests` (including the untouched
`oc_environment_yaml`) plus the two new tests from Step 1.

- [ ] **Step 6: Commit the Oc-layer change**

```bash
git add crates/rocket-infra/src/oc/environment.rs crates/rocket-infra/src/oc/mod.rs crates/rocket-infra/src/conversions/tests.rs
git commit -m "feat(infra): add OcExternalSecretBinding/Ref to OcEnvironment"
```

- [ ] **Step 7: Write the failing domain conversion tests**

Add these two tests to `crates/rocket-infra/src/conversions/tests.rs`,
alongside the existing `environment_roundtrip` test (this file already
imports `rocket_environment::environment::Environment` and `crate::oc::*` at
its top, both of which cover everything these tests need):

```rust
#[test]
fn environment_external_secrets_survive_oc_roundtrip() {
    use rocket_environment::external_secret::{ExternalSecretBinding, ExternalSecretRef};

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

    let oc: OcEnvironment = env.clone().into();
    let back: Environment = oc.into();

    assert_eq!(env.external_secrets, back.external_secrets);
}

#[test]
fn oc_environment_without_external_secrets_key_still_deserializes() {
    // Every environment .yml file saved before this feature existed has no
    // externalSecrets key — loading it must not fail, and it must convert to
    // an Environment with an empty external_secrets list, not error out.
    let yaml = "name: production\nvariables: []\n";
    let oc: OcEnvironment = serde_yaml::from_str(yaml).expect("parse old-format environment");
    let env: Environment = oc.into();
    assert!(env.external_secrets.is_empty());
}
```

- [ ] **Step 8: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra conversions::tests::environment_external_secrets_survive_oc_roundtrip`
Run: `cargo test -p rocket-infra conversions::tests::oc_environment_without_external_secrets_key_still_deserializes`
Expected: both FAIL to compile — `From<OcEnvironment> for Environment`
doesn't set `external_secrets` yet, and vice versa (the compiler reports a
missing-field error inside the `From` impls themselves once
`Environment.external_secrets` exists but nothing populates it — see the next
step).

- [ ] **Step 9: Extend the `From` conversions**

In `crates/rocket-infra/src/conversions/environment.rs`, add the import and
four small `From` impls for the external-secret types (pure field-by-field
mapping, no value transformation — neither struct ever carries a secret
value):

```rust
use crate::oc::*;
use rocket_environment::environment::Environment;
use rocket_environment::external_secret::{ExternalSecretBinding, ExternalSecretRef};
use rocket_environment::variable::Variable;

impl From<OcExternalSecretRef> for ExternalSecretRef {
    fn from(oc: OcExternalSecretRef) -> Self {
        ExternalSecretRef {
            name: oc.name,
            secret_id: oc.secret_id,
        }
    }
}

impl From<ExternalSecretRef> for OcExternalSecretRef {
    fn from(r: ExternalSecretRef) -> Self {
        OcExternalSecretRef {
            name: r.name,
            secret_id: r.secret_id,
        }
    }
}

impl From<OcExternalSecretBinding> for ExternalSecretBinding {
    fn from(oc: OcExternalSecretBinding) -> Self {
        ExternalSecretBinding {
            alias: oc.alias,
            connection_id: oc.connection_id,
            vault_name: oc.vault_name,
            secret_names: oc
                .secret_names
                .into_iter()
                .map(ExternalSecretRef::from)
                .collect(),
        }
    }
}

impl From<ExternalSecretBinding> for OcExternalSecretBinding {
    fn from(b: ExternalSecretBinding) -> Self {
        OcExternalSecretBinding {
            alias: b.alias,
            connection_id: b.connection_id,
            vault_name: b.vault_name,
            secret_names: b
                .secret_names
                .into_iter()
                .map(OcExternalSecretRef::from)
                .collect(),
        }
    }
}
```

Then extend the two existing `Environment`/`OcEnvironment` conversions to
carry the new field:

```rust
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
            external_secrets: oc
                .external_secrets
                .into_iter()
                .map(ExternalSecretBinding::from)
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
            external_secrets: env
                .external_secrets
                .into_iter()
                .map(OcExternalSecretBinding::from)
                .collect(),
            client_certificates: env.client_certificates,
            extends: env.extends,
            dot_env_file_path: env.dot_env_file_path,
        }
    }
}
```

- [ ] **Step 10: Run tests to verify they pass**

Run: `cargo test -p rocket-infra conversions::tests::environment`
Expected: PASS — every pre-existing `environment_*` test in
`conversions/tests.rs` (backward compatibility for the fields this task
didn't touch) plus the two new tests from Step 7.

Then run the full crate to catch anything Step 4's `grep` search missed:

```bash
cargo test -p rocket-infra
```

Expected: PASS — full crate.

- [ ] **Step 11: Commit**

```bash
git add crates/rocket-infra/src/conversions/environment.rs crates/rocket-infra/src/conversions/tests.rs
git commit -m "feat(infra): convert ExternalSecretBinding between Environment and OcEnvironment"
```

---

## Milestone Checklist — Plan 04

- [ ] `KeyringSecretStore::new_env_secrets()` — same keychain service string
  as before generalization (`"com.rocketapi.env-secrets"`), behavior-preserving
- [ ] `KeyringSecretStore::new_vault_connections()` — distinct keychain
  service string (`"com.rocketapi.vault-connection"`)
- [ ] `src-tauri/src/lib.rs`'s `env_secret_store()` helper calls
  `KeyringSecretStore::new_env_secrets()`
- [ ] `FsSecretManagerRepo` — `list`/`get`/`save`/`delete` over a single flat
  `secret_managers.yml`; missing file reads as empty; `save` replaces by `id`
  or appends; no `client_secret` ever reaches the persisted YAML
- [ ] `OcExternalSecretRef` / `OcExternalSecretBinding` — camelCase, mirror
  `ExternalSecretRef`/`ExternalSecretBinding` field-for-field
- [ ] `OcEnvironment.external_secrets: Vec<OcExternalSecretBinding>` —
  `#[serde(default, skip_serializing_if = "Vec::is_empty")]`, old environment
  YAML without the key still loads
- [ ] `From<OcEnvironment> for Environment` and `From<Environment> for
  OcEnvironment` both carry `external_secrets`; round-trip preserves it
  exactly
- [ ] `cargo test -p rocket-infra` — all pass
- [ ] `cargo check -p rocket-infra -p rocket-app` — clean

## Next Plan

[Plan 05: SecretManagerService + shared resolution helper](2026-09-22-rocketvault-secrets-plan-05-secret-manager-service.md) —
adds the `rocket-app` orchestration layer that wires this plan's
`FsSecretManagerRepo` and both `KeyringSecretStore` constructors together with
Plan 02's `VaultSecretFetcher` trait, plus the `resolve_vault_secret_value`
helper shared with Plan 06's `RequestExecutionService` integration.
