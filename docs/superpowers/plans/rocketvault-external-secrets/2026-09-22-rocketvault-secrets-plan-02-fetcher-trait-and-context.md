# RocketVault Secrets Plan 02: VaultSecretFetcher Trait, VariableContext Wiring, and rok.getSecretVar — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the async `VaultSecretFetcher` trait (plus its safe `NullVaultSecretFetcher`
default) to `rocket-environment`, thread a new `external_secrets` scope through
`VariableContext`, and expose that scope to scripts as `rok.getSecretVar` in
`rocket-infra`. No RocketVault network code lands in this plan — this plan
only defines the contract and where its resolved values live; Plan 03 is the
first real implementer.

**Architecture:** Three changes across two crates, each following an existing
pattern in this codebase rather than inventing a new one. (1)
`crates/rocket-environment/src/vault_secret_fetcher.rs` — a new async trait
module shaped exactly like `crates/rocket-http/src/executor.rs`'s
`HttpExecutor` (one trait, `&self` methods, `#[async_trait]`), plus a
`NullVaultSecretFetcher` shaped exactly like `secret_store.rs`'s
`NullSecretStore`. (2) `crates/rocket-environment/src/context.rs` — one new
`HashMap<String, String>` field on the existing `VariableContext`, threaded
through both `flatten()` and `flatten_with_process_env()` at a fixed
precedence tier between `collection` and `env`. (3)
`crates/rocket-infra/src/scripting/ops/rok.rs` — one new `#[op2]` function
shaped exactly like the existing `op_rok_get_env_var`, registered in
`engine.rs`'s ops table and wired into `bootstrap.js` so `rok.getSecretVar`
becomes callable from user scripts.

**Tech Stack:** Rust, `async-trait` (already a `rocket-environment` workspace
dependency), `tokio` (added here as a `rocket-environment` dev-dependency,
test-only), `deno_core`'s `#[op2]` macro (`rocket-infra`, existing).

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.6 resolution shape, §4.7 scripting op, §4.8 redaction — out of scope
here, see Task 2). Plan index:
`docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md` (has
the full locked interface contract every plan in this series depends on,
including this plan's exact `VaultSecretFetcher` signature and the
`VariableContext` precedence rule).

## Global Constraints

- `VaultSecretFetcher`'s method signatures are locked by the plan index —
  implement `list_secrets`/`get_secret_value`/`test_connection` verbatim, do
  not rename methods or reorder/rename parameters. Any deviation requires
  updating the index and every downstream plan that references it.
- `NullVaultSecretFetcher` errors on every method with
  `DomainError::Internal("no vault secret fetcher configured".to_string())`
  — mirrors the existing `NullSecretStore` pattern in
  `crates/rocket-environment/src/secret_store.rs`.
- `rocket-environment` has no I/O (per this crate's own `CLAUDE.md`) — Task 1
  only defines the trait and its safe default. The real HTTP-backed
  implementation (`ReqwestVaultSecretFetcher`) belongs to `rocket-infra` and
  is Plan 03's job, not this plan's.
- `flatten()`/`flatten_with_process_env()` insert the new `external_secrets`
  scope at a fixed tier: `... collection → external_secrets → env → folder →
  request → runtime`. This exact position is locked by the plan index — do
  not reorder it relative to `collection` or `env`.
- This plan introduces no new `Serialize`/`Deserialize` types, so the
  project's camelCase-on-IPC-DTOs-only rule does not apply to anything in
  this plan.
- Redaction wiring — populating `VariableContext.secret_values` from
  `external_secrets` — is explicitly **out of scope** for this plan. It
  happens later, in Plan 06, when `RequestExecutionService.resolve_external_secrets`
  populates `ctx.external_secrets` and `ctx.secret_values` together. Do not
  add any `secret_values` writes in Task 2.
- Test code in this plan (and all other plans in this series) uses
  `.expect("message")` for fallible calls rather than the bare panicking
  shorthand, matching this repository's stricter Rust safety convention even
  in test paths.
- Rust hard rule (repo-wide): never use the bare panicking accessor in
  production code paths — only `.expect("message")` in tests, and proper
  `DomainResult` propagation everywhere else.

---

## Task 1: `VaultSecretFetcher` trait + `NullVaultSecretFetcher`

**Files:**
- Create: `crates/rocket-environment/src/vault_secret_fetcher.rs`
- Modify: `crates/rocket-environment/src/lib.rs`
- Modify: `crates/rocket-environment/Cargo.toml`

**Interfaces:**
- Consumes: `SecretManagerConnection`
  (`crates/rocket-environment/src/secret_manager.rs`, Plan 01 Task 2),
  `ExternalSecretRef` (`crates/rocket-environment/src/external_secret.rs`,
  Plan 01 Task 1), `DomainError`/`DomainResult` (`rocket_shared::error`).
- Produces: `trait VaultSecretFetcher { list_secrets, get_secret_value,
  test_connection }`, `struct NullVaultSecretFetcher` — consumed by Plan 03
  (`ReqwestVaultSecretFetcher` implements this trait), Plan 05
  (`SecretManagerService` holds `Arc<dyn VaultSecretFetcher>`), and
  transitively by Plan 06/07 through `SecretManagerService`.

`rocket-environment`'s `Cargo.toml` currently declares `async-trait` as a
regular dependency but has no `tokio` dependency at all (checked directly —
there is no `[dev-dependencies]` section in this crate's `Cargo.toml` today).
Step 1 below adds it as a dev-dependency so `#[tokio::test]` is available for
this task's tests; do not assume it is already present.

- [ ] **Step 1: Add `tokio` as a dev-dependency**

In `crates/rocket-environment/Cargo.toml`, add a new section after the
existing `[dependencies]` block:

```toml
[dev-dependencies]
tokio.workspace = true
```

This inherits the workspace root's `tokio = { version = "1", features =
["full"] }` entry, the same `tokio.workspace = true` shorthand already used
by `rocket-http`, `rocket-app`, and `rocket-infra`'s own `Cargo.toml` files —
so `#[tokio::test]` is available without pinning a separate version here.

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-environment/src/vault_secret_fetcher.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Test Vault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    #[test]
    fn trait_is_object_safe() {
        // Arc, not Box: Plan 05's SecretManagerService holds
        // `Arc<dyn VaultSecretFetcher>` (see plan index), so this is the
        // shape that actually matters downstream.
        fn _assert(_: std::sync::Arc<dyn VaultSecretFetcher>) {}
    }

    #[tokio::test]
    async fn null_fetcher_list_secrets_errors() {
        let fetcher = NullVaultSecretFetcher;
        let err = fetcher
            .list_secrets(&dummy_connection(), "shh", "prod-vault")
            .await
            .expect_err("null fetcher must error on list_secrets");
        assert_eq!(
            err,
            DomainError::Internal("no vault secret fetcher configured".to_string())
        );
    }

    #[tokio::test]
    async fn null_fetcher_get_secret_value_errors() {
        let fetcher = NullVaultSecretFetcher;
        let err = fetcher
            .get_secret_value(&dummy_connection(), "shh", "prod-vault", "secret-id-1")
            .await
            .expect_err("null fetcher must error on get_secret_value");
        assert_eq!(
            err,
            DomainError::Internal("no vault secret fetcher configured".to_string())
        );
    }

    #[tokio::test]
    async fn null_fetcher_test_connection_errors() {
        let fetcher = NullVaultSecretFetcher;
        let err = fetcher
            .test_connection(&dummy_connection(), "shh", "prod-vault")
            .await
            .expect_err("null fetcher must error on test_connection");
        assert_eq!(
            err,
            DomainError::Internal("no vault secret fetcher configured".to_string())
        );
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-environment vault_secret_fetcher::tests`
Expected: FAIL — compile error, "cannot find type `VaultSecretFetcher`" /
"cannot find struct `NullVaultSecretFetcher`" (module doesn't exist yet).

- [ ] **Step 4: Implement the trait and the null default**

```rust
// crates/rocket-environment/src/vault_secret_fetcher.rs (add above the tests module)
use crate::external_secret::ExternalSecretRef;
use crate::secret_manager::SecretManagerConnection;
use rocket_shared::error::{DomainError, DomainResult};

/// Fetches secret names and values from a RocketVault server.
///
/// Takes `connection`/`client_secret`/`vault_name` as call arguments rather
/// than being constructed bound to one connection. One injected
/// `Arc<dyn VaultSecretFetcher>` instance serves every configured
/// `SecretManagerConnection` in the app, exactly the way `HttpExecutor`
/// (`crates/rocket-http/src/executor.rs`) serves every request regardless of
/// target host — a single `ReqwestExecutor` handles requests to any URL, it
/// is never rebuilt per host. This keeps `rocket-app` free of any
/// RocketVault-specific concrete type, per this repo's DDD boundary rule
/// (rocket-app: trait-first, no infra concrete coupling). A per-connection
/// struct would instead force whatever wires the trait object to either hold
/// one fetcher instance per configured connection or reconstruct one on
/// every call — both push infra concerns upward across the exact crate
/// boundary this trait exists to prevent.
#[async_trait::async_trait]
pub trait VaultSecretFetcher: Send + Sync {
    /// Lists secret *names* (never values) visible in `vault_name` through
    /// `connection`. Backs the "Fetch Secrets" action (spec §4.1/§4.5).
    async fn list_secrets(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>>;

    /// Fetches one secret's value by its vault-assigned `secret_id`.
    /// Returns `Ok(None)` if the id is no longer present in the vault (a
    /// stale binding, per spec §4.6), not an error — only a genuine
    /// transport/auth failure is an `Err` here.
    async fn get_secret_value(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>>;

    /// Verifies `connection`/`client_secret` can authenticate and reach
    /// `vault_name`, without fetching or returning any secret data. Backs
    /// the connection form's "Test Connection" action.
    async fn test_connection(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<()>;
}

/// No-op fetcher for tests and contexts with no RocketVault client wired —
/// mirrors `NullSecretStore` (`crate::secret_store`). Every method fails
/// loudly instead of returning empty data: an empty secret list or a silent
/// `Ok(None)` here could be mistaken for "this vault really has no secrets"
/// rather than "no fetcher is configured", so this type always surfaces the
/// misconfiguration as an error.
pub struct NullVaultSecretFetcher;

#[async_trait::async_trait]
impl VaultSecretFetcher for NullVaultSecretFetcher {
    async fn list_secrets(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }

    async fn get_secret_value(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        _secret_id: &str,
    ) -> DomainResult<Option<String>> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }

    async fn test_connection(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<()> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }
}
```

- [ ] **Step 5: Register the module**

In `crates/rocket-environment/src/lib.rs`, add alongside the existing module
declarations:

```rust
pub mod vault_secret_fetcher;
pub use vault_secret_fetcher::{NullVaultSecretFetcher, VaultSecretFetcher};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-environment vault_secret_fetcher::tests`
Expected: PASS — 4 tests.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-environment/src/vault_secret_fetcher.rs crates/rocket-environment/src/lib.rs crates/rocket-environment/Cargo.toml
git commit -m "feat(environment): add VaultSecretFetcher trait + NullVaultSecretFetcher"
```

---

## Task 2: `VariableContext.external_secrets` field + `flatten()` precedence

**Files:**
- Modify: `crates/rocket-environment/src/context.rs`

**Interfaces:**
- Consumes: nothing new from Task 1 — this is a plain `HashMap<String,
  String>` field, not the trait itself.
- Produces: `VariableContext.external_secrets: HashMap<String, String>`
  (dotted `"{alias}.{secretName}"` keys) — consumed by Task 3 of this plan
  (`rok.getSecretVar` op reads it) and, later, by Plan 06
  (`RequestExecutionService.resolve_external_secrets` populates it) and
  Plan 07 (`CollectionRunnerService` passes it into every step).

This task does **not** touch `VariableContext.secret_values` — redaction
wiring for vault-sourced values is Plan 06's job (it populates
`external_secrets` and `secret_values` together, from the same resolved
map). Adding to `secret_values` here would be duplicated, dead-until-Plan-06
work; do not do it.

- [ ] **Step 1: Write the failing tests**

Open `crates/rocket-environment/src/context.rs`. Its `#[cfg(test)] mod
tests` block already has a `full_hierarchy_runtime_wins` test that
constructs a `VariableContext` with every field named explicitly (no
`..Default::default()`), so once `external_secrets` exists on the struct
this literal needs the new field too or it will not compile. Update it and
add three new tests. The other existing precedence tests in this file
(`env_beats_collection`, `folder_beats_env`, `request_beats_folder`, etc.)
are unchanged by this task — leave them exactly as they are.

```rust
// Update the existing test (add the `external_secrets` field to the literal;
// this plan's test code uses `.expect(...)` rather than the bare panicking
// accessor the surrounding pre-existing tests in this file use, per this
// plan's Global Constraints):
#[test]
fn full_hierarchy_runtime_wins() {
    // All 9 scopes present — runtime must win.
    let ctx = VariableContext {
        runtime: m(&[("k", "runtime")]),
        request: m(&[("k", "request")]),
        folder: m(&[("k", "folder")]),
        env: m(&[("k", "env")]),
        collection: m(&[("k", "collection")]),
        external_secrets: m(&[("k", "vault")]),
        global_env: m(&[("k", "global")]),
        process_env: m(&[("k", "process")]),
        secret_values: std::collections::HashSet::new(),
    };
    assert_eq!(ctx.flatten().get("k").expect("k present"), "runtime");
}

// New tests, added alongside the existing precedence tests:

#[test]
fn env_beats_external_secrets() {
    let ctx = VariableContext {
        env: m(&[("k", "env")]),
        external_secrets: m(&[("k", "vault")]),
        ..Default::default()
    };
    assert_eq!(ctx.flatten().get("k").expect("k present"), "env");
}

#[test]
fn external_secrets_beats_collection() {
    let ctx = VariableContext {
        external_secrets: m(&[("k", "vault")]),
        collection: m(&[("k", "col")]),
        ..Default::default()
    };
    assert_eq!(ctx.flatten().get("k").expect("k present"), "vault");
}

#[test]
fn external_secrets_value_passes_through_flatten_unchanged() {
    // A value present only in external_secrets, with no key collision
    // anywhere else, shows up in flatten()'s output unchanged — plain
    // pass-through, not filtered.
    let ctx = VariableContext {
        external_secrets: m(&[("payments.stripeKey", "sk-live-abcdef123")]),
        ..Default::default()
    };
    let flat = ctx.flatten();
    assert_eq!(
        flat.get("payments.stripeKey")
            .expect("payments.stripeKey present"),
        "sk-live-abcdef123"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-environment context::tests`
Expected: FAIL — compile error, "no field `external_secrets` on type
`VariableContext`" (the field doesn't exist yet; because
`full_hierarchy_runtime_wins` names every field explicitly, this is a
compile-time failure for the whole module, not a runtime one — same shape as
Plan 01 Task 3's `Environment` field addition).

- [ ] **Step 3: Add the field and wire it into both flatten methods**

In `crates/rocket-environment/src/context.rs`, add the field to the struct
(placed next to `collection`, since that is its neighboring precedence tier):

```rust
#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub runtime: HashMap<String, String>,
    pub request: HashMap<String, String>,
    pub folder: HashMap<String, String>,
    pub env: HashMap<String, String>,
    pub collection: HashMap<String, String>,
    /// Values fetched from RocketVault, keyed by "{alias}.{secretName}" per
    /// the active environment's `external_secrets` bindings. Populated once
    /// per resolve/run by `RequestExecutionService::resolve_external_secrets`
    /// (Plan 06) — this crate has no I/O and never fetches these values
    /// itself. Kept as its own field rather than folded into `env`, so
    /// `rok.getEnvVar` never accidentally returns a vault-sourced value
    /// through an unrelated code path.
    pub external_secrets: HashMap<String, String>,
    pub global_env: HashMap<String, String>,
    pub process_env: HashMap<String, String>,
    /// Keys (from any scope) whose *value* must be redacted if it appears in
    /// script-emitted console/test-error text. Not a per-scope map — a value is
    /// either sensitive or not, regardless of which scope surfaced it.
    pub secret_values: HashSet<String>,
}
```

Then update both `flatten()` and `flatten_with_process_env()`, inserting the
new line immediately after `out.extend(self.collection.clone());` and
immediately before `out.extend(self.env.clone());` in each:

```rust
impl VariableContext {
    /// Merge all scopes except process_env.
    /// Insertion order: global_env → collection → external_secrets → env → folder → request → runtime.
    /// Later layers overwrite earlier on key collision.
    pub fn flatten(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        out.extend(self.global_env.clone());
        out.extend(self.collection.clone());
        out.extend(self.external_secrets.clone());
        out.extend(self.env.clone());
        out.extend(self.folder.clone());
        out.extend(self.request.clone());
        out.extend(self.runtime.clone());
        out
    }

    /// Same as flatten() but also inserts process env vars with "process.env." prefix (lowest priority).
    pub fn flatten_with_process_env(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for (k, v) in &self.process_env {
            out.insert(format!("process.env.{}", k), v.clone());
        }
        out.extend(self.global_env.clone());
        out.extend(self.collection.clone());
        out.extend(self.external_secrets.clone());
        out.extend(self.env.clone());
        out.extend(self.folder.clone());
        out.extend(self.request.clone());
        out.extend(self.runtime.clone());
        out
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-environment context::tests`
Expected: PASS — full module, including the 3 new tests, the updated
`full_hierarchy_runtime_wins`, and every pre-existing test (backward
compatibility check — in particular `secret_values_does_not_affect_flatten`
and friends must still pass unchanged, since this task does not touch
`secret_values`).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-environment/src/context.rs
git commit -m "feat(environment): add external_secrets scope to VariableContext"
```

---

## Task 3: `rok.getSecretVar` scripting op

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/rok.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs`
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js`

**Interfaces:**
- Consumes: `VariableContext.external_secrets`
  (`crates/rocket-environment/src/context.rs`, Task 2 of this plan);
  `ScriptInputState.variables` (`crates/rocket-infra/src/scripting/state.rs`,
  existing — already holds a full `VariableContext`, no change needed there).
- Produces: `op_rok_get_secret_var` (Rust `#[op2]` fn), registered in the
  `rocket_scripting_ext` ops table and exposed to user scripts as
  `rok.getSecretVar(key)` — the last piece needed for `{{alias.secretName}}`
  parity with Bruno's `bru.getSecretVar()`, per the spec's Bruno-parity note.

This task crosses from `rocket-environment` (Tasks 1–2) into `rocket-infra`
deliberately: the op is tightly coupled to the `VariableContext.external_secrets`
field this plan just added, and this is the plan where that field's shape is
locked in.

Read `crates/rocket-infra/src/scripting/ops/rok.rs` before starting — this
task copies the exact shape of the existing `op_rok_get_env_var` in that
file (`#[op2]` + `#[string]` attributes, reading
`state.borrow::<ScriptInputState>().variables.<field>.get(&key).cloned().unwrap_or_default()`).
`.unwrap_or_default()` here is a different method from the bare panicking
accessor this repo's hard rule bans — it never panics, it just falls back to
`String::new()` on a missing key, exactly like every other `rok.get*Var` op
in this file already does.

- [ ] **Step 1: Write the failing test**

Add to `crates/rocket-infra/src/scripting/engine.rs`'s existing `#[cfg(test)]
mod tests` block, following the exact pattern of
`rok_get_env_var_reads_from_context`:

```rust
#[tokio::test]
async fn rok_get_secret_var_reads_from_context() {
    let engine = DenoScriptEngine::new();
    let mut vars = VariableContext::default();
    vars.external_secrets
        .insert("payments.stripeKey".into(), "sk-live-abcdef123".into());
    let mut ctx = minimal_ctx("rok.setVar('key', rok.getSecretVar('payments.stripeKey'))");
    ctx.variables = vars;
    let result = engine.execute(ctx).await.expect("execute");
    let val = result.runtime_vars.get("key").expect("key present");
    assert_eq!(val, "sk-live-abcdef123");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rocket-infra rok_get_secret_var_reads_from_context`
Expected: FAIL — `engine.execute(ctx)` still returns `Ok(...)` (script
exceptions are captured into `ScriptResult.error`, not surfaced as an `Err`
from `execute`), but the script throws before reaching `rok.setVar(...)`
because `rok.getSecretVar` is not a function yet (the op doesn't exist and
`bootstrap.js` doesn't expose it), so `result.runtime_vars.get("key")` is
`None` and the test panics at `.expect("key present")`.

- [ ] **Step 3: Add the op**

In `crates/rocket-infra/src/scripting/ops/rok.rs`, add alongside the other
`getEnvVar`/`getCollectionVar`/`getGlobalEnvVar` reads:

```rust
/// rok.getSecretVar('alias.secretName') — reads a fetched External Secret value.
#[op2]
#[string]
pub fn op_rok_get_secret_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .external_secrets
        .get(&key)
        .cloned()
        .unwrap_or_default()
}
```

- [ ] **Step 4: Register the op in the extension! macro**

In `crates/rocket-infra/src/scripting/engine.rs`, inside the
`extension!(rocket_scripting_ext, ops = [ ... ])` block's `// rok ops`
section, add the new op right after `rok::op_rok_get_env_var,`:

```rust
        // rok ops
        rok::op_rok_get_var,
        rok::op_rok_set_var,
        rok::op_rok_get_env_var,
        rok::op_rok_get_secret_var,
        rok::op_rok_set_env_var,
        rok::op_rok_has_env_var,
        rok::op_rok_delete_env_var,
        rok::op_rok_get_env_name,
        rok::op_rok_get_collection_var,
        rok::op_rok_set_collection_var,
        rok::op_rok_get_global_env_var,
        rok::op_rok_set_global_env_var,
        rok::op_rok_interpolate,
        rok::op_rok_set_next_request,
        rok::op_rok_skip_request,
```

- [ ] **Step 5: Expose it on the `rok` global in bootstrap.js**

Registering the op in the extension macro only makes `op_rok_get_secret_var`
callable through `Deno.core.ops` — it is not yet reachable as
`rok.getSecretVar` from a user script. In
`crates/rocket-infra/src/scripting/bootstrap.js`, add a wrapper line to the
`globalThis.rok = { ... }` object, next to the other `getEnvVar`/`getEnvName`
wrappers:

```js
  globalThis.rok = {
    getVar:            (key)        => __ops.op_rok_get_var(key),
    setVar:            (key, value) => __ops.op_rok_set_var(key, JSON.stringify(value)),
    getEnvVar:         (key)        => __ops.op_rok_get_env_var(key),
    setEnvVar:         (key, value, opts) => __ops.op_rok_set_env_var(key, JSON.stringify(value), !!(opts && opts.persist)),
    hasEnvVar:         (key)        => __ops.op_rok_has_env_var(key),
    deleteEnvVar:      (key)        => __ops.op_rok_delete_env_var(key),
    getEnvName:        ()           => __ops.op_rok_get_env_name(),
    getSecretVar:      (key)        => __ops.op_rok_get_secret_var(key),
    getCollectionVar:  (key)        => __ops.op_rok_get_collection_var(key),
    setCollectionVar:  (key, value) => __ops.op_rok_set_collection_var(key, JSON.stringify(value)),
    getGlobalEnvVar:   (key)        => __ops.op_rok_get_global_env_var(key),
    setGlobalEnvVar:   (key, value) => __ops.op_rok_set_global_env_var(key, JSON.stringify(value)),
    interpolate:       (template)   => __ops.op_rok_interpolate(template),
    runner: {
      setNextRequest: (name)  => __ops.op_rok_set_next_request(name),
      skipRequest:    ()      => __ops.op_rok_skip_request(),
    },
  };
```

Only the new `getSecretVar:` line is an addition — every other line above is
shown for placement context and is unchanged.

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p rocket-infra rok_get_secret_var_reads_from_context`
Expected: PASS.

Also run the full scripting test module to confirm nothing else regressed:
Run: `cargo test -p rocket-infra scripting::`
Expected: PASS — all existing `rok.*` tests plus the new one.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/src/scripting/ops/rok.rs crates/rocket-infra/src/scripting/engine.rs crates/rocket-infra/src/scripting/bootstrap.js
git commit -m "feat(scripting): add rok.getSecretVar op"
```

---

## Milestone Checklist — Plan 02

- [ ] `tokio` added as a `rocket-environment` dev-dependency (`Cargo.toml`)
- [ ] `VaultSecretFetcher` trait — `list_secrets`/`get_secret_value`/`test_connection`, signatures exactly match the plan index
- [ ] `NullVaultSecretFetcher` — every method errors with `DomainError::Internal("no vault secret fetcher configured")`
- [ ] `VaultSecretFetcher` is object-safe (`Arc<dyn VaultSecretFetcher>` compiles)
- [ ] `VariableContext.external_secrets: HashMap<String, String>` — new field
- [ ] `flatten()`/`flatten_with_process_env()` insert `external_secrets` between `collection` and `env`, verified by `env_beats_external_secrets`, `external_secrets_beats_collection`, and a pass-through test
- [ ] No writes to `VariableContext.secret_values` added in this plan (confirmed out of scope, deferred to Plan 06)
- [ ] `rok.getSecretVar` — new `#[op2]` op, registered in `rocket_scripting_ext`, wired into `bootstrap.js`'s `rok` global, reads `VariableContext.external_secrets`
- [ ] `cargo test -p rocket-environment` — all pass
- [ ] `cargo test -p rocket-infra` (scripting module) — all pass

## Next Plan

[Plan 03: ReqwestVaultSecretFetcher — the RocketVault wire client](2026-09-22-rocketvault-secrets-plan-03-wire-client.md) —
implements `VaultSecretFetcher` (this plan's Task 1) against the real
RocketVault HTTP API in `rocket-infra`, replacing `NullVaultSecretFetcher` as
the production default once wired in `src-tauri`.
