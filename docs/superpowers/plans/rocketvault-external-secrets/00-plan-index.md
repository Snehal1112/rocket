# RocketVault External Secrets — Plan Index

**Spec:** [../../specs/2026-09-22-rocketvault-external-secrets-spec.md](../../specs/2026-09-22-rocketvault-external-secrets-spec.md)

**Bruno parity (re-verified before writing these plans):** this feature must match
Bruno's Azure Key Vault integration shape exactly — a global, app-level "Secret
Manager" connection list (label, base URL, client ID; client secret in the OS
keychain, never on disk), a per-environment "External Secrets" binding under a
user-chosen **alias** with vault name + a "Fetch Secrets" action that pulls
secret *names* only, `{{alias.secretName}}` resolving in request fields exactly
like a normal variable, and a `rok.getSecretVar('alias.secretName')` script API
mirroring Bruno's `bru.getSecretVar()`. Every plan below implements one slice of
this shape; none of them should introduce a different UX (no per-variable
backend toggle, no auto-sync, no write-back to RocketVault — read-only, per the
spec's non-goals).

## Plan breakdown — 10 plans, 28 tasks (max 3 per plan)

| # | Plan | Tasks | Crate/area | Depends on |
|---|---|---|---|---|
| 01 | [Domain types: ExternalSecretBinding, SecretManagerConnection](2026-09-22-rocketvault-secrets-plan-01-domain-types.md) | 3 | `rocket-environment` | — |
| 02 | [VaultSecretFetcher trait + VariableContext + rok.getSecretVar](2026-09-22-rocketvault-secrets-plan-02-fetcher-trait-and-context.md) | 3 | `rocket-environment` + `rocket-infra` (scripting op) | 01 |
| 03 | [ReqwestVaultSecretFetcher (RocketVault wire client)](2026-09-22-rocketvault-secrets-plan-03-wire-client.md) | 3 | `rocket-infra` | 01, 02 |
| 04 | [FsSecretManagerRepo + keychain + Oc persistence layer](2026-09-22-rocketvault-secrets-plan-04-persistence.md) | 3 | `rocket-infra` | 01 |
| 05 | [SecretManagerService + shared resolution helper](2026-09-22-rocketvault-secrets-plan-05-secret-manager-service.md) | 3 | `rocket-app` | 01, 02, 03, 04 |
| 06 | [RequestExecutionService external-secret resolution](2026-09-22-rocketvault-secrets-plan-06-execution-service.md) | 3 | `rocket-app` | 01, 02, 05 |
| 07 | [CollectionRunnerService once-per-run wiring](2026-09-22-rocketvault-secrets-plan-07-runner-service.md) | 2 | `rocket-app` | 06 |
| 08 | [Tauri commands + service wiring](2026-09-22-rocketvault-secrets-plan-08-tauri-commands.md) | 2 | `src-tauri` | 05, 06, 07 |
| 09 | [Frontend: types + Secret Manager Connections UI](2026-09-22-rocketvault-secrets-plan-09-frontend-connections.md) | 3 | frontend | 08 |
| 10 | [Frontend: Environment External Secrets tab](2026-09-22-rocketvault-secrets-plan-10-frontend-external-secrets-tab.md) | 3 | frontend | 08, 09 |

Plans 07 and 08 are 2 tasks, not 3 — both genuinely have no third slice to
carve out without artificially splitting a task in half (Plan 07: one
call-site change, one call-count regression test; Plan 08: command module +
DTOs, then `lib.rs` wiring). Every other plan explains this "max 3, not
exactly 3" reasoning where it's not 3, per the writing-plans skill's Task
Right-Sizing guidance.

Each plan file ends with a **Next Plan** section naming the file above and
linking to it, so a fresh Claude Code session opening any single plan file
knows exactly what to run next without needing this index.

## Locked interface contract

Every plan below is written against these exact types/signatures. If an
implementer needs to deviate, they must update this index and every
downstream plan file that references the changed name — do not let two plan
files disagree on a signature.

### `rocket-environment` (new, Plan 01)

```rust
// crates/rocket-environment/src/external_secret.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretRef {
    pub name: String,
    pub secret_id: String,
}

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

```rust
// crates/rocket-environment/src/secret_manager.rs
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
// NOTE: plain field names (no camelCase rename) — this is app-level config
// persisted to its own secret_managers.yml, not part of the OpenCollection
// format, so the general "no camelCase on persistence structs" rule applies
// here (unlike ExternalSecretBinding/Variable, which nest inside Environment
// and inherit that aggregate's existing camelCase convention).

pub trait SecretManagerRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>>;
    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>>;
    fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}
```

`Environment` (`crates/rocket-environment/src/environment.rs`) gains:
```rust
#[serde(default)]
pub external_secrets: Vec<ExternalSecretBinding>,
```

### `rocket-environment` (new, Plan 02)

```rust
// crates/rocket-environment/src/vault_secret_fetcher.rs
#[async_trait::async_trait]
pub trait VaultSecretFetcher: Send + Sync {
    async fn list_secrets(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>>;

    async fn get_secret_value(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>>;

    async fn test_connection(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<()>;
}

pub struct NullVaultSecretFetcher;
// every method returns Err(DomainError::Internal("no vault secret fetcher configured"))
```

Note this trait takes `connection`/`client_secret` **per call** rather than being
constructed bound to one connection — one injected `Arc<dyn VaultSecretFetcher>`
instance serves every configured connection, exactly like `HttpExecutor`/
`ReqwestExecutor` serves every request regardless of target host. This keeps
`rocket-app` free of any RocketVault-specific concrete type (DDD boundary rule).

`VariableContext` (`crates/rocket-environment/src/context.rs`) gains:
```rust
pub external_secrets: HashMap<String, String>, // key = "{alias}.{secretName}"
```
`flatten()`/`flatten_with_process_env()` insert it between `collection` and
`env` (so `env` wins on the practically-impossible case of a literal key
collision): `... collection → external_secrets → env → folder → request → runtime`.

### `rocket-infra` (new, Plan 03)

```rust
// crates/rocket-infra/src/rocketvault/mod.rs
pub struct ReqwestVaultSecretFetcher { /* reqwest::Client + DashMap<String, TokenCache> keyed by connection.id */ }
impl ReqwestVaultSecretFetcher {
    pub fn new() -> Self;
}
impl VaultSecretFetcher for ReqwestVaultSecretFetcher { /* per §4.1/§4.2 of the spec */ }
```

### `rocket-infra` (new, Plan 04)

```rust
// crates/rocket-infra/src/fs_secret_manager_repo.rs
pub struct FsSecretManagerRepo { /* path: PathBuf, points at secret_managers.yml */ }
impl FsSecretManagerRepo {
    pub fn new(path: PathBuf) -> Self;
}
impl SecretManagerRepository for FsSecretManagerRepo { /* ... */ }
```

`KeyringSecretStore` (`crates/rocket-infra/src/secret_store.rs`) is generalized
to take a service label instead of a hardcoded constant, so it can be reused
for vault-connection client secrets under a distinct keychain namespace:
```rust
impl KeyringSecretStore {
    pub fn new_env_secrets() -> Self;       // service = "com.rocketapi.env-secrets" (existing behavior, unchanged)
    pub fn new_vault_connections() -> Self; // service = "com.rocketapi.vault-connection" (new)
}
```
`src-tauri/src/lib.rs`'s existing `env_secret_store()` helper updates to call
`KeyringSecretStore::new_env_secrets()` (one-line change, behavior-preserving).

`OcEnvironment` (`crates/rocket-infra/src/oc/environment.rs`) gains a mirrored
`external_secrets: Vec<OcExternalSecretBinding>` (camelCase, matching
`OcVariable`'s convention) plus the `From` conversions in
`crates/rocket-infra/src/conversions/environment.rs`.

### `rocket-app` (new, Plan 05)

```rust
// crates/rocket-app/src/secret_manager_service.rs
pub struct SecretManagerService {
    repo: Box<dyn SecretManagerRepository>,
    secret_store: Arc<dyn SecretStore>, // client_secret storage; scope_id = "vault-connection", key = connection.id
    fetcher: Arc<dyn VaultSecretFetcher>,
}
impl SecretManagerService {
    pub fn new(
        repo: Box<dyn SecretManagerRepository>,
        secret_store: Arc<dyn SecretStore>,
        fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self;
    pub fn list(&self) -> DomainResult<Vec<SecretManagerConnection>>;
    // client_secret: Some(..) sets/overwrites the keychain entry; None on an
    // update leaves the existing keychain entry untouched (edit-without-resecret).
    pub fn save(&self, connection: SecretManagerConnection, client_secret: Option<String>) -> DomainResult<()>;
    pub fn delete(&self, id: &str) -> DomainResult<()>;
    pub async fn test_connection(&self, id: &str, vault_name: &str) -> DomainResult<()>;
    pub async fn fetch_secret_names(&self, id: &str, vault_name: &str) -> DomainResult<Vec<ExternalSecretRef>>;
}
```

```rust
// crates/rocket-app/src/vault_secret_resolution.rs — shared by Plan 05 and Plan 06
pub async fn resolve_vault_secret_value(
    repo: &dyn SecretManagerRepository,
    secret_store: &dyn SecretStore,
    fetcher: &dyn VaultSecretFetcher,
    connection_id: &str,
    vault_name: &str,
    secret_id: &str,
) -> DomainResult<Option<String>>;
```

### `rocket-app` (modified, Plan 06/07)

**Finalized during implementation-planning (Plan 06), superseding this
index's original sketch below** — `RequestExecutionService::new`/
`new_with_audit` gain three new **required, trailing** constructor
parameters (not a separate builder method — an earlier draft explored that
shape but Plan 06's actual, line-number-verified content uses required
arguments instead, appended after the existing `audit` parameter, in this
fixed order). This breaks every existing test-construction call site in
`execution_service.rs`/`collection_runner_service.rs`/`load_test_service.rs`
(47 sites, enumerated exhaustively in Plan 06 Task 1 Step 2 by exact line
number) — Plan 06 fixes all of them as part of its own Task 1, using a new
`EmptySecretManagerRepo` test fake alongside the existing `NullSecretStore`/
`NullVaultSecretFetcher`. Plan 08's `lib.rs` wiring passes the real
`FsSecretManagerRepo`/`KeyringSecretStore::new_vault_connections()`/
`ReqwestVaultSecretFetcher` instances directly into `new_with_audit(...)`'s
call, not via a chained builder.

```rust
impl RequestExecutionService {
    pub fn new(
        // ...existing parameters unchanged...
        secret_manager_repo: Box<dyn SecretManagerRepository>,
        vault_connection_secret_store: Arc<dyn SecretStore>,
        vault_fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self;

    pub fn new_with_audit(
        // ...existing parameters unchanged, ending in `audit`...
        secret_manager_repo: Box<dyn SecretManagerRepository>,
        vault_connection_secret_store: Arc<dyn SecretStore>,
        vault_fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self;

    pub async fn resolve_external_secrets(
        &self,
        environment_name: Option<&str>,
    ) -> DomainResult<std::collections::HashMap<String, String>>; // "{alias}.{secretName}" -> value
}
```
`build_variable_context`/`build_variable_scopes`/`resolve_request`/`begin_phases`
(`crates/rocket-app/src/execution_service.rs:259,321,334,831`) each gain an
added `external_secrets: &HashMap<String, String>` parameter, merged into the
returned/mutated context at the position described above. `execute()` calls
`resolve_external_secrets` before `begin_phases` and propagates its error
(hard-fail, per spec §4.6). `CollectionRunnerService`
(`crates/rocket-app/src/collection_runner_service.rs:362`) calls it once per
run and passes the same map into every step's `begin_phases` call.

### Frontend (new, Plan 09/10)

```typescript
export interface ExternalSecretRef {
  name: string;
  secretId: string;
}
export interface ExternalSecretBinding {
  alias: string;
  connectionId: string;
  vaultName: string;
  secretNames: ExternalSecretRef[];
}
export interface SecretManagerConnection {
  id: string;
  label: string;
  baseUrl: string;
  clientId: string;
  verifySsl: boolean;
  allowInsecureHttp: boolean;
}
export interface Environment {
  name: string;
  variables: Variable[];
  externalSecrets: ExternalSecretBinding[]; // new
}
```

## Execution note for whoever runs these plans

Run the plans in numeric order — each one's Global Constraints section
repeats the interfaces it consumes from earlier plans so it's runnable by a
fresh Claude Code session that has only read that one file, but the actual
code those interfaces reference won't exist yet if an earlier plan was
skipped. Use `superpowers:subagent-driven-development` or
`superpowers:executing-plans` per plan, per each file's own header.
