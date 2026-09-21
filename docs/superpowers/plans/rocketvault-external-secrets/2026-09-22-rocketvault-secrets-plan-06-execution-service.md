# RocketVault Secrets Plan 06: RequestExecutionService External-Secret Resolution — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `RequestExecutionService` a way to fetch live RocketVault secret
values for the active environment's `external_secrets` bindings, and thread
the resulting `{alias}.{secretName} -> value` map through its internal
variable-resolution pipeline so `{{alias.secretName}}` resolves in
`execute()` exactly like any other variable — with a hard stop before dispatch
if the live fetch fails.

**Architecture — the load-bearing design decision this plan makes explicit:**
`RequestExecutionService::execute()` (`crates/rocket-app/src/execution_service.rs:1281`)
is the only `async` entry point in its call chain; `begin_phases` (line 831),
`resolve_request` (line 334), `build_variable_context` (line 321), and
`build_variable_scopes` (line 259) are all synchronous today and read only
from local repositories. This plan does **not** make any of those four
methods `async`. Instead, it threads a pre-resolved
`external_secrets: &std::collections::HashMap<String, String>` parameter
through all four, each one merging it into its own output the same way Plan
02 changed `VariableContext.flatten()` to fold `external_secrets` in between
`collection` and `env`:

- `build_variable_scopes` sets `ctx.external_secrets = external_secrets.clone();`
  on the `VariableContext` it builds, before returning it.
- `build_variable_context` doesn't touch `external_secrets` directly — it just
  passes the parameter down to `build_variable_scopes` and calls the
  now-`external_secrets`-aware `.flatten()` on the result (Plan 02 already
  wired `flatten()` to fold this field in).
- `resolve_request` passes the parameter down to `build_variable_context`.
- `begin_phases` passes the parameter down to both `resolve_request` and
  `build_variable_scopes`.

The only place that actually does the network fetch — the new `async fn
resolve_external_secrets` added in Task 1 below — is called exactly once, by
`execute()`, *before* `begin_phases` runs (Task 3). This keeps the entire
`begin_phases`/`resolve_request`/`build_variable_scopes`/`build_variable_context`
chain synchronous and unchanged in its `async`-ness, matches the shape
`CollectionRunnerService` already independently drives (Plan 07 threads the
same map into its own `begin_phases` call, resolved once per run instead of
once per request), and means a `resolve_external_secrets` failure surfaces as
a plain `DomainResult::Err` from `execute()` before any HTTP dispatch — the
"hard stop before dispatch" spec requirement (§2, §4.6) falls out of ordinary
`?`-early-return control flow, not a new error-handling mechanism.

**Tech Stack:** Rust, tokio (existing `#[tokio::test]` patterns in this file).

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.6). Plan index (locked interface contract):
`docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md`.
Depends on:
[Plan 01: Domain Types](2026-09-22-rocketvault-secrets-plan-01-domain-types.md)
(`Environment.external_secrets`, `ExternalSecretBinding`, `ExternalSecretRef`,
`SecretManagerConnection`, `SecretManagerRepository`),
[Plan 02: Fetcher Trait and Context](2026-09-22-rocketvault-secrets-plan-02-fetcher-trait-and-context.md)
(`VaultSecretFetcher`, `VariableContext.external_secrets` +
`flatten()`/`flatten_with_process_env()` wiring), and Plan 05 (`SecretManagerService
+ shared resolution helper` — not yet written as a plan file as of this
writing; this plan depends only on its **locked** interface from
`00-plan-index.md`:
`crate::vault_secret_resolution::resolve_vault_secret_value(repo: &dyn SecretManagerRepository, secret_store: &dyn SecretStore, fetcher: &dyn VaultSecretFetcher, connection_id: &str, vault_name: &str, secret_id: &str) -> DomainResult<Option<String>>`,
plus `SecretStore` — `crates/rocket-environment/src/secret_store.rs`, already
in the codebase today — and `SecretManagerRepository` from Plan 01). If Plan
05 lands with a different signature for `resolve_vault_secret_value`, update
this plan file and the index before implementing Task 1.

## Global Constraints

- **Current (pre-this-plan) signatures, confirmed by direct reading of
  `crates/rocket-app/src/execution_service.rs`** — every line number below was
  verified, not guessed:
  - `fn build_variable_scopes(&self, collection: Option<&str>, environment_name: Option<&str>, request_path: Option<&str>) -> VariableContext` — line 259, private, synchronous.
  - `pub fn build_variable_context(&self, collection: Option<&str>, environment_name: Option<&str>, request_path: Option<&str>) -> std::collections::HashMap<String, String>` — line 321, calls `build_variable_scopes(...).flatten()` at line 327.
  - `pub(crate) fn resolve_request(&self, input: &ExecuteRequestInput) -> DomainResult<HttpRequest>` — line 334, calls `build_variable_context` at line 336.
  - `pub(crate) fn begin_phases(&self, input: &ExecuteRequestInput) -> DomainResult<PhaseState>` — line 831, calls `resolve_request` at line 832 and `build_variable_scopes` at line 852.
  - `pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<ExecuteRequestOutput>` — line 1281, calls `begin_phases` at line 1285.
  - `pub async fn run_load_test(&self, input: ExecuteRequestInput, config: LoadTestConfig) -> DomainResult<LoadTestResult>` — line 1296, calls `resolve_request` at line 1301. **Not** part of this plan's resolution flow — spec §4.6 only names `execute()` and `CollectionRunnerService::run` as resolution call sites. Its `resolve_request` call permanently passes an empty map; extending vault-secret resolution to load testing is out of scope for this entire plan series (not listed anywhere in `00-plan-index.md`) and is a deliberate scope boundary, not an oversight.
  - `crates/rocket-app/src/load_test_service.rs:30` — `LoadTestService::run` also calls `execution_service.resolve_request(&input)` directly (a second, V2 load-test path). Same scope boundary as `run_load_test` above: permanently passes an empty map.
  - `crates/rocket-app/src/collection_runner_service.rs:362` — `exec.begin_phases(&step_input)`, inside `run_step`. **Do not touch this file's production code in this plan.** Plan 07 (already written — see its Task 1 Step 3) is the plan that updates this exact call site to thread the run-scoped map through. See "Expected transient compile state" below.
- **Test convention:** test code added by this plan uses `.expect("message")`
  for fallible calls, never the bare panicking shorthand — matching every
  other plan in this series.
- **Redaction (spec §4.7/§4.8):** every value folded into `VariableContext` by
  this plan must also land in `ctx.secret_values` (subject to the existing
  `MIN_REDACTION_LEN` = 6 threshold already defined at
  `execution_service.rs:175`), the same set the console/test-output redaction
  mechanism already scans — this is required by spec §4.6 ("inserted into
  `var_ctx.secret_values`... so console/test-output redaction covers it for
  free") and is folded into Task 2 below (`build_variable_scopes` is the one
  place in this file that already does this for `collection`/`env`/global-env
  values — external secrets join the same pattern there).
- **Constructor dependency order:** `RequestExecutionService::new`'s existing
  parameter list ends with `events: Box<dyn EventPublisher>`; `new_with_audit`
  appends `audit: Arc<dyn SecurityAuditPublisher>` after that (the precedent
  for how a previously-added dependency was appended to this constructor,
  rather than inserted mid-list). This plan follows the same append-only
  convention: the three new dependencies go at the very end of both
  constructors' parameter lists, in this fixed order — `secret_manager_repo:
  Box<dyn SecretManagerRepository>`, then `vault_connection_secret_store:
  Arc<dyn SecretStore>`, then `vault_fetcher: Arc<dyn VaultSecretFetcher>` —
  matching the parameter order Plan 05's locked `SecretManagerService::new`
  signature already establishes (repo, then secret_store, then fetcher).
- **Expected transient compile state.** After this plan's Tasks 1–2 land,
  `cargo check -p rocket-app` (non-test target) will report **exactly one**
  remaining error: `collection_runner_service.rs:362`'s `exec.begin_phases(&step_input)`
  call, one argument short of the new signature. This is deliberate — fixing
  it here would collide with Plan 07's own Task 1 Step 3, which fixes that
  exact line immediately after this plan lands. Likewise `src-tauri/src/lib.rs`'s
  `RequestExecutionService::new_with_audit(...)` call
  (`src-tauri/src/lib.rs:220`) is left uncompiled until Plan 08 — this plan
  does not touch `src-tauri`. Task 2's own verification step below explains
  how to confirm no *other* errors slipped in beyond these two known,
  already-owned-by-a-later-plan cases.
- This plan's tests configure their fake `SecretManagerRepository`/`SecretStore`
  chain to succeed all the way up to the `VaultSecretFetcher` call, then vary
  only the fetcher's return value per test case. `resolve_vault_secret_value`'s
  exact internal error variants for a missing connection/secret aren't
  exercised here (Plan 05 owns that unit-level behavior) — this plan only
  depends on its documented black-box contract: `Ok(Some(value))`,
  `Ok(None)`, or `Err(_)`.

---

## Task 1: `resolve_external_secrets` method + its three new constructor dependencies

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`
- Modify: `crates/rocket-app/src/collection_runner_service.rs` (test module only — constructor call sites)
- Modify: `crates/rocket-app/src/load_test_service.rs` (test module only — constructor call sites)
- Modify: `crates/rocket-app/src/test_doubles.rs` (new shared fake, used by `collection_runner_service.rs`'s tests)

**Interfaces:**
- Consumes: `SecretManagerRepository`, `SecretStore` (Plan 01 / existing), `VaultSecretFetcher` (Plan 02), `resolve_vault_secret_value` (Plan 05), `ExternalSecretBinding`/`ExternalSecretRef` (Plan 01), `Environment.external_secrets` (Plan 01).
- Produces: `RequestExecutionService::resolve_external_secrets(&self, environment_name: Option<&str>) -> DomainResult<std::collections::HashMap<String, String>>`, plus the three new constructor parameters on `new`/`new_with_audit` — consumed by Task 3 of this plan (`execute()`) and by Plan 07 (`CollectionRunnerService::run`).

- [ ] **Step 1: Add the three new fields and thread them through both constructors**

In `crates/rocket-app/src/execution_service.rs`, add to the top-of-file
`use rocket_environment::{...}` import (line 7-9):

```rust
use rocket_environment::{
    resolve, Environment, EnvironmentRepository, EnvironmentRepositoryFactory,
    SecretManagerRepository, SecretStore, VariableContext, VaultSecretFetcher,
};
```

Add three fields to the `RequestExecutionService` struct (after the existing
`script_engine` field, line ~166):

```rust
pub struct RequestExecutionService {
    // ... existing fields unchanged ...
    script_engine: Option<Box<dyn ScriptEngine>>,
    /// App-level RocketVault connection registry — `resolve_external_secrets`
    /// looks up the `SecretManagerConnection` named by each binding's
    /// `connection_id`. No I/O in this crate; concrete impl lives in
    /// `rocket-infra` (Plan 04's `FsSecretManagerRepo`).
    secret_manager_repo: Box<dyn SecretManagerRepository>,
    /// Client-secret storage for vault connections — a distinct
    /// `SecretStore` instance from the one `FsEnvironmentRepo` uses for
    /// local `secret: true` variables (that one is scoped to
    /// `com.rocketapi.env-secrets`; this one to
    /// `com.rocketapi.vault-connection`, via Plan 04's
    /// `KeyringSecretStore::new_vault_connections()`).
    vault_connection_secret_store: Arc<dyn SecretStore>,
    /// Fetches live secret values from a configured RocketVault connection.
    /// One shared instance serves every connection, exactly like
    /// `executor: Arc<dyn HttpExecutor>` serves every request regardless of
    /// target host.
    vault_fetcher: Arc<dyn VaultSecretFetcher>,
}
```

Update `new` and `new_with_audit` (append-only, per the Global Constraints
convention):

```rust
pub fn new(
    env_repo: Box<dyn EnvironmentRepository>,
    executor: Arc<dyn HttpExecutor>,
    history_repo: Box<dyn HistoryRepository>,
    collection_repo: Box<dyn CollectionRepository>,
    cookie_repo: Box<dyn CookieRepository>,
    events: Box<dyn EventPublisher>,
    secret_manager_repo: Box<dyn SecretManagerRepository>,
    vault_connection_secret_store: Arc<dyn SecretStore>,
    vault_fetcher: Arc<dyn VaultSecretFetcher>,
) -> Self {
    Self {
        env_repo,
        collection_env_repo_factory: None,
        executor,
        history_repo,
        collection_repo,
        cookie_repo,
        events,
        audit: Arc::new(NullSecurityAuditPublisher),
        script_engine: None,
        secret_manager_repo,
        vault_connection_secret_store,
        vault_fetcher,
    }
}

pub fn new_with_audit(
    env_repo: Box<dyn EnvironmentRepository>,
    executor: Arc<dyn HttpExecutor>,
    history_repo: Box<dyn HistoryRepository>,
    collection_repo: Box<dyn CollectionRepository>,
    cookie_repo: Box<dyn CookieRepository>,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
    secret_manager_repo: Box<dyn SecretManagerRepository>,
    vault_connection_secret_store: Arc<dyn SecretStore>,
    vault_fetcher: Arc<dyn VaultSecretFetcher>,
) -> Self {
    Self {
        env_repo,
        collection_env_repo_factory: None,
        executor,
        history_repo,
        collection_repo,
        cookie_repo,
        events,
        audit,
        script_engine: None,
        secret_manager_repo,
        vault_connection_secret_store,
        vault_fetcher,
    }
}
```

- [ ] **Step 2: Fix every existing constructor call site so the crate compiles again**

This is purely mechanical — every call site below needs the same three
trailing arguments appended:

```rust
Box::new(EmptySecretManagerRepo),
Arc::new(rocket_environment::NullSecretStore),
Arc::new(rocket_environment::NullVaultSecretFetcher),
```

`EmptySecretManagerRepo` doesn't exist yet (Plan 01 didn't add a null impl for
`SecretManagerRepository` — only `SecretStore`/`VaultSecretFetcher` got
`Null*` impls in Plans 01/02). Add a trivial local fake per file, following
this crate's existing convention that `execution_service.rs` and
`load_test_service.rs` keep their own doubles rather than sharing
(`test_doubles.rs`'s own header comment: "`execution_service.rs` predates
this module and keeps its own doubles — do not migrate those"):

```rust
/// No connections configured — every lookup misses. Used by every test in
/// this file that doesn't exercise RocketVault resolution itself.
struct EmptySecretManagerRepo;

impl rocket_environment::SecretManagerRepository for EmptySecretManagerRepo {
    fn list(&self) -> DomainResult<Vec<rocket_environment::SecretManagerConnection>> {
        Ok(vec![])
    }
    fn get(&self, _id: &str) -> DomainResult<Option<rocket_environment::SecretManagerConnection>> {
        Ok(None)
    }
    fn save(&self, _connection: &rocket_environment::SecretManagerConnection) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _id: &str) -> DomainResult<()> {
        Ok(())
    }
}
```

Add this once near the top of each affected file's `#[cfg(test)] mod tests`
block (`execution_service.rs`, `load_test_service.rs`), and once as a `pub`
struct in `test_doubles.rs` (same shape, `pub struct EmptySecretManagerRepo;`
plus `pub` methods) for `collection_runner_service.rs`'s tests to import
alongside `NullEnvRepo`/`NullCookieRepo` from that same module. Add
`use rocket_environment::{NullSecretStore, NullVaultSecretFetcher};` to each
file's test imports as needed (or fully qualify inline, as shown above).

Call sites to update (all verified by direct grep against the current file
state — the compiler's "this function takes 9 arguments but 6 were supplied"
errors will point at each of these once Step 1 lands, but the list below
means you don't have to hunt for them one at a time):

*`crates/rocket-app/src/execution_service.rs`* — 43 call sites inside
`#[cfg(test)] mod tests` (starts at line 1444): `RequestExecutionService::new(`
at lines 1735, 1769, 1788, 1833, 1865, 1994, 2028, 2070, 2140, 2641, 3059,
3140, 3180, 3213, 3246, 3282, 3312, 3348, 3388, 3430, 3478, 3506, 3526, 3549,
3569, 3593, 3617, 3648, 3670, 3726, 3763, 4115, 4192, 4254, 4303, 4351, 4416
(37 sites — one of these, line 2641, is inside the shared `build_svc_with_script`
helper at line 2636, so fixing that one helper covers every test that calls
it); `RequestExecutionService::new_with_audit(` at lines 2186, 2222, 2268,
2789, 2897, 2959 (6 sites).

*`crates/rocket-app/src/collection_runner_service.rs`* — 3 call sites, all
`RequestExecutionService::new(` at lines 566 (inside the shared `harness()`
helper), 1150, 1240 — all three use the identical argument list (`NullEnvRepo`,
`SharedExecutor`, `SharedHistoryRepo`, `SharedCollectionRepo`, `NullCookieRepo`,
`NullEventPublisher`), so the same three trailing arguments apply to each.

*`crates/rocket-app/src/load_test_service.rs`* — 1 call site,
`RequestExecutionService::new(` at line 277.

Since all 47 of these sites are inside `#[cfg(test)]` blocks, plain
`cargo check -p rocket-app` will not catch a missed one — use
`cargo check -p rocket-app --tests` (or `cargo test -p rocket-app --no-run`)
to verify.

- [ ] **Step 3: Write the failing tests for `resolve_external_secrets`**

Add to `crates/rocket-app/src/execution_service.rs`'s existing
`#[cfg(test)] mod tests` block, alongside the fakes added in Step 2:

```rust
fn test_connection(id: &str) -> rocket_environment::SecretManagerConnection {
    rocket_environment::SecretManagerConnection {
        id: id.to_string(),
        label: "Test".to_string(),
        base_url: "https://vault.internal:8774".to_string(),
        client_id: "rocketapi".to_string(),
        verify_ssl: true,
        allow_insecure_http: false,
    }
}

/// Configurable connection registry for `resolve_external_secrets` tests.
struct FakeSecretManagerRepo {
    connections: Vec<rocket_environment::SecretManagerConnection>,
}

impl FakeSecretManagerRepo {
    fn with_connection(conn: rocket_environment::SecretManagerConnection) -> Self {
        Self { connections: vec![conn] }
    }
}

impl rocket_environment::SecretManagerRepository for FakeSecretManagerRepo {
    fn list(&self) -> DomainResult<Vec<rocket_environment::SecretManagerConnection>> {
        Ok(self.connections.clone())
    }
    fn get(&self, id: &str) -> DomainResult<Option<rocket_environment::SecretManagerConnection>> {
        Ok(self.connections.iter().find(|c| c.id == id).cloned())
    }
    fn save(&self, connection: &rocket_environment::SecretManagerConnection) -> DomainResult<()> {
        let _ = connection;
        Ok(())
    }
    fn delete(&self, _id: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Always hands back a fixed client secret — the actual value never matters
/// to these tests, only that the lookup succeeds.
struct FakeSecretStore;

impl rocket_environment::SecretStore for FakeSecretStore {
    fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
        Ok(Some("test-client-secret".to_string()))
    }
    fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
        Ok(())
    }
    fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
        Ok(())
    }
}

/// Per-secret-id scripted outcome for `FakeVaultFetcher::get_secret_value`.
#[derive(Clone)]
enum FakeSecretOutcome {
    Value(String),
    Missing,       // get_secret_value -> Ok(None): deleted on the RocketVault side.
    Error(String), // get_secret_value -> Err(DomainError::Internal(..)).
}

/// Records every secret_id it was asked to resolve, in call order, so tests
/// can assert both the returned value and how many/which calls were made.
struct FakeVaultFetcher {
    responses: std::collections::HashMap<String, FakeSecretOutcome>,
    calls: Mutex<Vec<String>>,
}

impl FakeVaultFetcher {
    fn new(responses: Vec<(&str, FakeSecretOutcome)>) -> Self {
        Self {
            responses: responses.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl rocket_environment::VaultSecretFetcher for FakeVaultFetcher {
    async fn list_secrets(
        &self,
        _connection: &rocket_environment::SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<rocket_environment::ExternalSecretRef>> {
        Ok(vec![])
    }

    async fn get_secret_value(
        &self,
        _connection: &rocket_environment::SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>> {
        self.calls
            .lock()
            .expect("lock FakeVaultFetcher calls")
            .push(secret_id.to_string());
        match self.responses.get(secret_id) {
            Some(FakeSecretOutcome::Value(v)) => Ok(Some(v.clone())),
            Some(FakeSecretOutcome::Missing) | None => Ok(None),
            Some(FakeSecretOutcome::Error(msg)) => Err(DomainError::Internal(msg.clone())),
        }
    }

    async fn test_connection(
        &self,
        _connection: &rocket_environment::SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<()> {
        Ok(())
    }
}

fn binding_with_refs(
    alias: &str,
    refs: Vec<(&str, &str)>, // (name, secret_id)
) -> rocket_environment::ExternalSecretBinding {
    rocket_environment::ExternalSecretBinding {
        alias: alias.to_string(),
        connection_id: "conn-1".to_string(),
        vault_name: "prod-vault".to_string(),
        secret_names: refs
            .into_iter()
            .map(|(name, secret_id)| rocket_environment::ExternalSecretRef {
                name: name.to_string(),
                secret_id: secret_id.to_string(),
            })
            .collect(),
    }
}

fn svc_with_vault(
    env: Option<Environment>,
    fetcher: Arc<FakeVaultFetcher>,
) -> RequestExecutionService {
    let env_repo: Box<dyn rocket_environment::EnvironmentRepository> = match env {
        Some(e) => Box::new(MockEnvRepo::with_env(e)),
        None => Box::new(MockEnvRepo::empty()),
    };
    RequestExecutionService::new(
        env_repo,
        Arc::new(MockExecutor::new(200)),
        Box::new(MockHistoryRepo::new()),
        Box::new(StubCollectionRepo::empty()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(FakeSecretManagerRepo::with_connection(test_connection("conn-1"))),
        Arc::new(FakeSecretStore),
        fetcher,
    )
}

#[tokio::test]
async fn resolve_external_secrets_returns_empty_map_when_no_environment_given() {
    let fetcher = Arc::new(FakeVaultFetcher::new(vec![]));
    let svc = svc_with_vault(None, Arc::clone(&fetcher));

    let result = svc
        .resolve_external_secrets(None)
        .await
        .expect("resolve_external_secrets");

    assert!(result.is_empty());
    assert!(
        fetcher.calls.lock().expect("lock calls").is_empty(),
        "no environment name means no lookups at all, not even a miss"
    );
}

#[tokio::test]
async fn resolve_external_secrets_returns_empty_map_for_environment_with_no_bindings() {
    let env = Environment::new("prod");
    let fetcher = Arc::new(FakeVaultFetcher::new(vec![]));
    let svc = svc_with_vault(Some(env), Arc::clone(&fetcher));

    let result = svc
        .resolve_external_secrets(Some("prod"))
        .await
        .expect("resolve_external_secrets");

    assert!(result.is_empty());
    assert!(fetcher.calls.lock().expect("lock calls").is_empty());
}

#[tokio::test]
async fn resolve_external_secrets_resolves_all_secret_names_in_one_binding() {
    let mut env = Environment::new("prod");
    env.external_secrets.push(binding_with_refs(
        "payments",
        vec![("apiKey", "sec-1"), ("webhookSecret", "sec-2")],
    ));
    let fetcher = Arc::new(FakeVaultFetcher::new(vec![
        ("sec-1", FakeSecretOutcome::Value("sk-live-key".to_string())),
        ("sec-2", FakeSecretOutcome::Value("whsec-abc".to_string())),
    ]));
    let svc = svc_with_vault(Some(env), Arc::clone(&fetcher));

    let result = svc
        .resolve_external_secrets(Some("prod"))
        .await
        .expect("resolve_external_secrets");

    assert_eq!(result.len(), 2);
    assert_eq!(
        result.get("payments.apiKey"),
        Some(&"sk-live-key".to_string())
    );
    assert_eq!(
        result.get("payments.webhookSecret"),
        Some(&"whsec-abc".to_string())
    );
}

#[tokio::test]
async fn resolve_external_secrets_hard_fails_on_a_fetcher_error() {
    let mut env = Environment::new("prod");
    env.external_secrets.push(binding_with_refs(
        "payments",
        vec![("apiKey", "sec-1"), ("webhookSecret", "sec-2")],
    ));
    let fetcher = Arc::new(FakeVaultFetcher::new(vec![
        ("sec-1", FakeSecretOutcome::Value("sk-live-key".to_string())),
        ("sec-2", FakeSecretOutcome::Error("vault unreachable".to_string())),
    ]));
    let svc = svc_with_vault(Some(env), Arc::clone(&fetcher));

    let err = svc
        .resolve_external_secrets(Some("prod"))
        .await
        .expect_err("a fetcher error on one ref must fail the whole call");

    assert!(matches!(err, DomainError::Internal(_)));
}

#[tokio::test]
async fn resolve_external_secrets_skips_a_deleted_secret_silently() {
    let mut env = Environment::new("prod");
    env.external_secrets.push(binding_with_refs(
        "payments",
        vec![
            ("apiKey", "sec-1"),
            ("deletedOnVaultSide", "sec-2"),
            ("webhookSecret", "sec-3"),
        ],
    ));
    let fetcher = Arc::new(FakeVaultFetcher::new(vec![
        ("sec-1", FakeSecretOutcome::Value("sk-live-key".to_string())),
        ("sec-2", FakeSecretOutcome::Missing),
        ("sec-3", FakeSecretOutcome::Value("whsec-abc".to_string())),
    ]));
    let svc = svc_with_vault(Some(env), Arc::clone(&fetcher));

    let result = svc
        .resolve_external_secrets(Some("prod"))
        .await
        .expect("resolve_external_secrets");

    assert_eq!(result.len(), 2, "the deleted secret must be silently omitted, not errored");
    assert!(!result.contains_key("payments.deletedOnVaultSide"));
    assert_eq!(
        result.get("payments.apiKey"),
        Some(&"sk-live-key".to_string())
    );
    assert_eq!(
        result.get("payments.webhookSecret"),
        Some(&"whsec-abc".to_string())
    );
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p rocket-app resolve_external_secrets`
Expected: FAIL to compile — `resolve_external_secrets` is not a method on
`RequestExecutionService` yet.

- [ ] **Step 5: Implement `resolve_external_secrets`**

Add to `impl RequestExecutionService` in `crates/rocket-app/src/execution_service.rs`,
near `build_variable_scopes`:

```rust
/// Fetches real values for every `ExternalSecretRef` in the named
/// environment's `external_secrets` bindings. Returns a flat map keyed
/// `"{alias}.{secretName}" -> value`.
///
/// `None` (no active environment) short-circuits to an empty map with zero
/// network activity — nothing to resolve. A binding whose ref resolves to
/// `Ok(None)` (deleted on the RocketVault side since the last "Fetch
/// Secrets") is silently omitted from the result, not an error. Any other
/// `Err` aborts the whole call immediately: a *configured, previously
/// successfully fetched* external secret name failing to resolve means the
/// network/vault is unreachable right now, not that the secret was never
/// set — partially populating the map and continuing would risk a request
/// going out with some vault-sourced values silently missing (spec §4.6).
pub async fn resolve_external_secrets(
    &self,
    environment_name: Option<&str>,
) -> DomainResult<std::collections::HashMap<String, String>> {
    let mut result = std::collections::HashMap::new();

    let Some(name) = environment_name else {
        return Ok(result);
    };

    let env = self.env_repo.get(name)?;
    for binding in &env.external_secrets {
        for secret_ref in &binding.secret_names {
            let value = crate::vault_secret_resolution::resolve_vault_secret_value(
                self.secret_manager_repo.as_ref(),
                self.vault_connection_secret_store.as_ref(),
                self.vault_fetcher.as_ref(),
                &binding.connection_id,
                &binding.vault_name,
                &secret_ref.secret_id,
            )
            .await?;

            if let Some(value) = value {
                result.insert(format!("{}.{}", binding.alias, secret_ref.name), value);
            }
        }
    }

    Ok(result)
}
```

Note this reads via `self.env_repo.get(name)` directly, the same repo
`begin_phases` already uses for `global_env_name` lookups (line 858) — not
the collection-scoped `regular_env_repo(collection)` helper `build_variable_scopes`
uses for the REGULAR environment. `resolve_external_secrets`'s locked
signature (`00-plan-index.md`) takes only `environment_name`, no `collection`
parameter, so it has no collection to route through in the first place; this
matches how `execute()` will call it in Task 3, passing
`input.environment_name.as_deref()` with no collection involved.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p rocket-app resolve_external_secrets`
Expected: PASS — 5 tests.

- [ ] **Step 7: Run the full crate test suite**

Run: `cargo test -p rocket-app`
Expected: PASS for every test except the ones inside `collection_runner_service.rs`
that were already relying on `begin_phases`'s old (pre-Plan-07) signature —
those don't exist yet at this point since Task 2 below hasn't landed; if
`cargo test -p rocket-app` fails elsewhere, fix it before proceeding — do not
carry a broken build into Task 2.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs crates/rocket-app/src/collection_runner_service.rs crates/rocket-app/src/load_test_service.rs crates/rocket-app/src/test_doubles.rs
git commit -m "feat(app): add RequestExecutionService::resolve_external_secrets"
```

---

## Task 2: Thread `external_secrets` through `build_variable_context`/`build_variable_scopes`/`resolve_request`/`begin_phases`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`
- Modify: `crates/rocket-app/src/load_test_service.rs` (production call site)

**Interfaces:**
- Consumes: `resolve_external_secrets` (Task 1).
- Produces: the four methods' new signatures — consumed by Task 3 of this
  plan (`execute()`) and by Plan 07 (`CollectionRunnerService::run_step`'s
  `begin_phases` call).

- [ ] **Step 1: Write/adjust the failing tests**

The two existing direct-call test sites just need the new argument appended
— they don't need new assertions, since Task 1 and Task 3 already cover the
actual resolution behavior:

- `execution_service.rs:1779` (`resolve_request_handles_hyphenated_variable_names`):
  change the call to `svc.resolve_request(&input, &std::collections::HashMap::new())`
  — leave the rest of that line exactly as it is today (whatever style it
  unwraps the result with is unrelated to this plan).
- `execution_service.rs:4583` (`before_request_phase_records_skip_and_next_request`)
  and `execution_service.rs:4603` (`seed_runtime_puts_carried_vars_in_the_runtime_scope`):
  change both `svc.begin_phases(&input)` calls to
  `svc.begin_phases(&input, &std::collections::HashMap::new())`.

Also add one new test proving `external_secrets` actually reaches the
flattened map `resolve_request` uses — this is the one genuinely new
assertion in this task, and doubles as this task's red/green pair:

```rust
#[tokio::test]
async fn resolve_request_folds_in_external_secrets() {
    let svc = RequestExecutionService::new(
        Box::new(MockEnvRepo::empty()),
        Arc::new(MockExecutor::new(200)),
        Box::new(MockHistoryRepo::new()),
        Box::new(StubCollectionRepo::empty()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    );

    let mut external_secrets = std::collections::HashMap::new();
    external_secrets.insert("payments.apiKey".to_string(), "sk-live-key".to_string());

    let input = sample_input("https://api.example.com/{{payments.apiKey}}", None);
    let resolved = svc
        .resolve_request(&input, &external_secrets)
        .expect("resolve_request");

    assert_eq!(resolved.url, "https://api.example.com/sk-live-key");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app resolve_request_folds_in_external_secrets`
Expected: FAIL to compile — `resolve_request` doesn't take a second argument
yet.

- [ ] **Step 3: Change the four signatures and their internal call sites**

```rust
fn build_variable_scopes(
    &self,
    collection: Option<&str>,
    environment_name: Option<&str>,
    request_path: Option<&str>,
    external_secrets: &std::collections::HashMap<String, String>,
) -> VariableContext {
    // ... unchanged body up through the folder/request variable blocks ...

    ctx.external_secrets = external_secrets.clone();
    for value in external_secrets.values() {
        if value.len() >= MIN_REDACTION_LEN {
            ctx.secret_values.insert(value.clone());
        }
    }

    ctx
}

pub fn build_variable_context(
    &self,
    collection: Option<&str>,
    environment_name: Option<&str>,
    request_path: Option<&str>,
    external_secrets: &std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, String> {
    self.build_variable_scopes(collection, environment_name, request_path, external_secrets)
        .flatten()
}

pub(crate) fn resolve_request(
    &self,
    input: &ExecuteRequestInput,
    external_secrets: &std::collections::HashMap<String, String>,
) -> DomainResult<HttpRequest> {
    let vars = self.build_variable_context(
        input.collection.as_deref(),
        input.environment_name.as_deref(),
        input.request_path.as_deref(),
        external_secrets,
    );
    // ... rest of the method body unchanged ...
}

pub(crate) fn begin_phases(
    &self,
    input: &ExecuteRequestInput,
    external_secrets: &std::collections::HashMap<String, String>,
) -> DomainResult<PhaseState> {
    let http_request = self.resolve_request(input, external_secrets)?;

    // ... unchanged sensitive-auth audit block ...

    let mut var_ctx = self.build_variable_scopes(
        input.collection.as_deref(),
        input.environment_name.as_deref(),
        input.request_path.as_deref(),
        external_secrets,
    );
    // ... rest of the method body unchanged ...
}
```

The `ctx.external_secrets = external_secrets.clone();` plus redaction-set
loop goes right before `build_variable_scopes`'s final bare `ctx` return
(after the existing folder/request-variable blocks, so it runs regardless of
whether `collection`/`environment_name` were `Some` or `None`) — mirroring
exactly how the existing `collection`/`env`/`global_env` blocks in this same
function already populate `ctx.secret_values` under the same
`MIN_REDACTION_LEN` threshold.

- [ ] **Step 4: Fix the remaining production call sites**

- `execution_service.rs:1301` (inside `run_load_test`): change
  `self.resolve_request(&input)?` to
  `self.resolve_request(&input, &std::collections::HashMap::new())?` —
  permanent empty map, per the Global Constraints scope note (`run_load_test`
  is not part of this plan series' resolution flow).
- `execution_service.rs:1285` (inside `execute()`): change
  `self.begin_phases(&input)?` to
  `self.begin_phases(&input, &std::collections::HashMap::new())?` — this is a
  **temporary** placeholder; Task 3 below replaces the empty map with the
  real resolved one. Land it here so the crate compiles at the end of this
  task.
- `crates/rocket-app/src/load_test_service.rs:30`: change
  `execution_service.resolve_request(&input)?` to
  `execution_service.resolve_request(&input, &std::collections::HashMap::new())?`
  — same permanent-empty-map scope note as `run_load_test`.

Do **not** touch `crates/rocket-app/src/collection_runner_service.rs:362`
(`exec.begin_phases(&step_input)`) — see the Global Constraints "Expected
transient compile state" note. Plan 07 owns that line.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p rocket-app resolve_request_folds_in_external_secrets`
Expected: PASS.

- [ ] **Step 6: Verify the rest of the crate, accounting for the one known,
      later-plan-owned error**

Run: `cargo check -p rocket-app`
Expected: exactly one error, at `collection_runner_service.rs:362` (missing
argument to `begin_phases`). If you see any *other* error, a call site was
missed — go back to Step 3/4 and find it (the compiler's error message names
the file and line).

Run: `cargo test -p rocket-app --no-run`
Expected: the same single `collection_runner_service.rs:362` error, plus no
other errors from any of the 47 test call sites fixed in Task 1 — those were
all updated for the constructor's new arity, not for these four methods'
signatures, so this step also confirms Task 1's Step 2 was complete.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs crates/rocket-app/src/load_test_service.rs
git commit -m "feat(app): thread external_secrets through the variable-resolution chain"
```

---

## Task 3: Wire `resolve_external_secrets` into `execute()`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

**Interfaces:**
- Consumes: `resolve_external_secrets` (Task 1), the four threaded signatures (Task 2).
- Produces: `execute()`'s new resolution-then-dispatch ordering, plus
  `redact_secrets_in_url(url, secret_values) -> String` (a private helper —
  strips secret values from the URL persisted to `rocket-history`, spec
  §6/AC4). This is the end of this plan's scope; Plan 07 reuses
  `resolve_external_secrets` for `CollectionRunnerService::run` separately.

- [ ] **Step 1: Write the failing integration tests**

Add to `crates/rocket-app/src/execution_service.rs`'s test module, reusing
the fakes from Task 1 (`FakeSecretManagerRepo`, `FakeSecretStore`,
`FakeVaultFetcher`, `FakeSecretOutcome`, `test_connection`, `binding_with_refs`):

```rust
/// Counts calls instead of just recording the last one, so the failure-path
/// test below can assert the executor was never reached at all.
struct CallCountingExecutor {
    calls: Mutex<usize>,
    response: HttpResponse,
}

impl CallCountingExecutor {
    fn new(status: u16) -> Self {
        Self {
            calls: Mutex::new(0),
            response: HttpResponse {
                status,
                status_text: "OK".into(),
                headers: vec![],
                body: "{}".into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: 2,
            },
        }
    }

    fn call_count(&self) -> usize {
        *self.calls.lock().expect("lock CallCountingExecutor")
    }
}

#[async_trait]
impl HttpExecutor for CallCountingExecutor {
    async fn execute(&self, _req: &HttpRequest) -> DomainResult<HttpResponse> {
        *self.calls.lock().expect("lock CallCountingExecutor") += 1;
        Ok(self.response.clone())
    }
}

#[tokio::test]
async fn execute_resolves_external_secret_before_dispatch() {
    let mut env = Environment::new("prod");
    env.external_secrets.push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

    let fetcher = Arc::new(FakeVaultFetcher::new(vec![
        ("sec-1", FakeSecretOutcome::Value("sk-live-test-value".to_string())),
    ]));

    let executor = Arc::new(MockExecutor::new(200));
    let exec_arc = Arc::clone(&executor);

    let svc = RequestExecutionService::new(
        Box::new(MockEnvRepo::with_env(env)),
        executor,
        Box::new(MockHistoryRepo::new()),
        Box::new(StubCollectionRepo::empty()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(FakeSecretManagerRepo::with_connection(test_connection("conn-1"))),
        Arc::new(FakeSecretStore),
        fetcher,
    );

    let out = svc
        .execute(sample_input(
            "https://api.example.com/{{payments.apiKey}}",
            Some("prod"),
        ))
        .await
        .expect("execute");

    assert_eq!(out.response.status, 200);
    let url = exec_arc
        .last_url
        .lock()
        .expect("lock last_url")
        .clone()
        .expect("executor was called");
    assert_eq!(url, "https://api.example.com/sk-live-test-value");
}

#[tokio::test]
async fn execute_fails_before_dispatch_when_external_secret_fetch_errors() {
    let mut env = Environment::new("prod");
    env.external_secrets.push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

    let fetcher = Arc::new(FakeVaultFetcher::new(vec![
        ("sec-1", FakeSecretOutcome::Error("vault unreachable".to_string())),
    ]));

    let executor = Arc::new(CallCountingExecutor::new(200));
    let exec_arc = Arc::clone(&executor);

    let svc = RequestExecutionService::new(
        Box::new(MockEnvRepo::with_env(env)),
        executor,
        Box::new(MockHistoryRepo::new()),
        Box::new(StubCollectionRepo::empty()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(FakeSecretManagerRepo::with_connection(test_connection("conn-1"))),
        Arc::new(FakeSecretStore),
        fetcher,
    );

    let result = svc
        .execute(sample_input(
            "https://api.example.com/{{payments.apiKey}}",
            Some("prod"),
        ))
        .await;

    assert!(result.is_err(), "execute() must fail when external-secret resolution fails");
    assert_eq!(
        exec_arc.call_count(),
        0,
        "HttpExecutor::execute must never run when resolve_external_secrets errors — \
         this is the 'never a silent empty-string substitution' / hard-stop-before-dispatch \
         requirement from spec §2/§4.6"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-app execute_resolves_external_secret_before_dispatch execute_fails_before_dispatch_when_external_secret_fetch_errors`
Expected: FAIL — the happy-path test fails because `execute()` still passes
an empty map into `begin_phases` (from Task 2 Step 4), so `{{payments.apiKey}}`
stays unresolved in the dispatched URL; the failure-path test fails because
nothing in `execute()` calls `resolve_external_secrets` yet, so it never
errors and the executor does get called once.

- [ ] **Step 3: Wire `execute()`**

```rust
#[tracing::instrument(
    name = "http_request",
    skip(self, input),
    fields(
        method = %input.method,
        url = %input.url,
    )
)]
pub async fn execute(&self, input: ExecuteRequestInput) -> DomainResult<ExecuteRequestOutput> {
    // Fetches must succeed before any variable resolution or dispatch runs —
    // a configured external secret that fails to resolve live is a hard
    // stop, not a silent empty-string substitution (spec §2/§4.6). The `?`
    // here is the entire mechanism: an Err from resolve_external_secrets
    // propagates straight out of execute() before begin_phases (and
    // therefore before send_request) ever runs.
    let external_secrets = self
        .resolve_external_secrets(input.environment_name.as_deref())
        .await?;

    // Every phase runs unconditionally — this is the single-send path. The
    // Collection Runner calls the same methods one at a time so it can act
    // on skip_request / next_request between them.
    let mut state = self.begin_phases(&input, &external_secrets)?;
    self.run_before_request_phase(&input, ExecutionMode::Standalone, &mut state)
        .await?;
    let response = self.send_request(&state).await?;
    self.run_after_response_phase(&input, ExecutionMode::Standalone, &response, &mut state)
        .await;
    self.run_tests_phase(&input, ExecutionMode::Standalone, &response, &mut state)
        .await;
    Ok(self.finish_phases(&input, response, &mut state).await)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-app execute_resolves_external_secret_before_dispatch execute_fails_before_dispatch_when_external_secret_fetch_errors`
Expected: PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "feat(app): resolve external secrets before dispatch in RequestExecutionService::execute"
```

**A second, separate leak this plan must also close: `rocket-history`.**
Spec §6 and acceptance criterion 4 both require that a resolved secret value
"never appears in ... `rocket-history`", not just the saved environment YAML.
`finish_phases` (`execution_service.rs:1192-1276`, unmodified by anything
above) builds its `HistoryEntry` directly from the *resolved* URL:

```rust
// crates/rocket-app/src/execution_service.rs:1244-1255 (current, unmodified)
let mut entry = HistoryEntry::new(
    input.method.to_string(),
    &state.http_request.url,   // ← already-resolved: contains the real secret value
    response.status,
    response.duration_ms,
    response.size_bytes,
);
if let (Some(col), Some(name)) = (&input.collection, &input.request_name) {
    entry = entry.with_collection(col, name);
}
let _ = self.history_repo.save(&entry);
```

`rocket-history`'s own `HistoryEntry` only ever stores method/url/status/
duration/size (confirmed: `crates/rocket-history/src/entry.rs`) — no headers,
no body — so the URL is the *only* field that can leak a secret value into
`~/.rocket-api/history/`. `state.var_ctx.secret_values` (populated by this
task's Step 3 for external secrets, and already populated today for local
`secret: true` variables per the already-shipped secret-aware-variable-context
spec — see Global Constraints) is exactly the redaction list this needs; this
fix incidentally closes the same pre-existing gap for local secrets too,
which today leak into history unredacted.

- [ ] **Step 6: Write the failing test**

```rust
#[tokio::test]
async fn history_entry_redacts_external_secret_value_from_the_url() {
    let mut env = Environment::new("prod");
    env.external_secrets.push(binding_with_refs("payments", vec![("apiKey", "sec-1")]));

    let fetcher = Arc::new(FakeVaultFetcher::new(vec![
        ("sec-1", FakeSecretOutcome::Value("sk-live-test-value".to_string())),
    ]));

    let history_repo = Box::new(MockHistoryRepo::new());
    let history_arc = history_repo.saved_entries_handle(); // see note below

    let svc = RequestExecutionService::new(
        Box::new(MockEnvRepo::with_env(env)),
        Arc::new(MockExecutor::new(200)),
        history_repo,
        Box::new(StubCollectionRepo::empty()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(FakeSecretManagerRepo::with_connection(test_connection("conn-1"))),
        Arc::new(FakeSecretStore),
        fetcher,
    );

    svc.execute(sample_input(
        "https://api.example.com/{{payments.apiKey}}",
        Some("prod"),
    ))
    .await
    .expect("execute");

    let saved = history_arc.lock().expect("lock saved entries");
    assert_eq!(saved.len(), 1);
    assert!(
        !saved[0].url.contains("sk-live-test-value"),
        "history entry must not contain the resolved secret value, got: {}",
        saved[0].url
    );
    assert!(
        saved[0].url.contains("••••••"),
        "expected the redaction marker in place of the secret, got: {}",
        saved[0].url
    );
}
```

(`MockHistoryRepo` already exists in this file's test module per Task 1's
reused fakes; if it doesn't currently expose a way to inspect what was saved
— check its actual definition first — add a `saved_entries_handle(&self) ->
Arc<Mutex<Vec<HistoryEntry>>>` accessor to it, or an equivalent `saved()`
getter matching whatever style its neighbors already use, as a small
prerequisite change in this same step; do not invent a second, parallel mock
history repo type.)

- [ ] **Step 7: Run the test to verify it fails**

Run: `cargo test -p rocket-app history_entry_redacts_external_secret_value_from_the_url`
Expected: FAIL — the saved entry's URL still contains
`sk-live-test-value` verbatim.

- [ ] **Step 8: Implement the redaction**

```rust
// crates/rocket-app/src/execution_service.rs — new private helper, near
// the other small free functions in this file (e.g. alongside
// merge_auth/merge_headers)
fn redact_secrets_in_url(url: &str, secret_values: &std::collections::HashSet<String>) -> String {
    if secret_values.is_empty() {
        return url.to_string();
    }
    let mut out = url.to_string();
    for value in secret_values {
        if value.len() < MIN_REDACTION_LEN {
            continue; // same short-secret exemption already applied when populating secret_values
        }
        out = out.replace(value.as_str(), "••••••");
    }
    out
}
```

Then change `finish_phases`'s `HistoryEntry::new` call
(`execution_service.rs:1245`) from:

```rust
let mut entry = HistoryEntry::new(
    input.method.to_string(),
    &state.http_request.url,
    response.status,
    response.duration_ms,
    response.size_bytes,
);
```

to:

```rust
let redacted_url = redact_secrets_in_url(&state.http_request.url, &state.var_ctx.secret_values);
let mut entry = HistoryEntry::new(
    input.method.to_string(),
    &redacted_url,
    response.status,
    response.duration_ms,
    response.size_bytes,
);
```

Only the history write changes — `state.http_request.url` itself (used for
the actual dispatch, already sent by the time `finish_phases` runs, and for
the `DomainEvent::RequestExecuted` event a few lines below) is left
untouched; redaction applies only at the persistence boundary, matching this
spec's existing "redact at the point output is produced, not at the point
secrets are read" principle from the already-shipped
secret-aware-variable-context spec.

- [ ] **Step 9: Run the test to verify it passes**

Run: `cargo test -p rocket-app history_entry_redacts_external_secret_value_from_the_url`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "fix(app): redact secret values from history URL before persisting"
```

- [ ] **Step 11: Run the full `rocket-app` test suite (excluding the one known error)**

Run: `cargo test -p rocket-app`
Expected: every test passes except the crate still fails to *compile* at
`collection_runner_service.rs:362` (Plan 07's responsibility — see Global
Constraints). If Plan 07 has already landed in this working tree by the time
you run this, `cargo test -p rocket-app` should be fully green with no
excluded cases.

---

## Milestone Checklist — Plan 06

- [ ] `RequestExecutionService` gains `secret_manager_repo`, `vault_connection_secret_store`, `vault_fetcher` fields, appended to both `new` and `new_with_audit`
- [ ] `resolve_external_secrets(environment_name: Option<&str>) -> DomainResult<HashMap<String, String>>` — empty map for `None` or no bindings, dotted-key map for resolved refs, silently skips a `None`-resolving ref, hard-fails the whole call on any `Err`
- [ ] `build_variable_scopes`/`build_variable_context`/`resolve_request`/`begin_phases` each gain an `external_secrets: &HashMap<String, String>` parameter, all four still synchronous
- [ ] `build_variable_scopes` folds `external_secrets` into both `ctx.external_secrets` and (subject to `MIN_REDACTION_LEN`) `ctx.secret_values`
- [ ] `execute()` calls `resolve_external_secrets` before `begin_phases` and propagates its error with `?`, before any HTTP dispatch
- [ ] `redact_secrets_in_url` strips every value in `state.var_ctx.secret_values` (both external-secret and pre-existing local-secret values) out of the URL passed to `HistoryEntry::new` in `finish_phases` — the dispatched request and the `RequestExecuted` event still carry the real URL, only the persisted history entry is redacted (spec §6/AC4)
- [ ] `run_load_test` and `LoadTestService::run` permanently pass an empty map — explicitly out of scope, not a placeholder
- [ ] `collection_runner_service.rs:362` deliberately left broken, owned by Plan 07
- [ ] `cargo test -p rocket-app` — all pass (except the one line owned by Plan 07, until Plan 07 lands)

## Next Plan

[Plan 07: CollectionRunnerService once-per-run wiring](2026-09-22-rocketvault-secrets-plan-07-runner-service.md) —
reuses `resolve_external_secrets` from this plan, but calls it once at the
start of a Collection Runner run rather than once per request, and threads
the resulting map through every step's `begin_phases` call.
