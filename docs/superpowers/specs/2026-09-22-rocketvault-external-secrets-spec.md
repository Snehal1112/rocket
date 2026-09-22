# Spec: RocketVault External Secrets Integration

**Status:** Draft
**Severity:** N/A (new feature)
**Related:** builds alongside, does not replace, the existing local `SecretStore`
mechanism ([2026-09-16-secret-storage-hardening-spec.md](2026-09-16-secret-storage-hardening-spec.md),
[2026-09-16-secret-aware-variable-context-spec.md](2026-09-16-secret-aware-variable-context-spec.md)).
Sub-project A of a two-part integration (RocketVault + rocket-mem); rocket-mem is
tracked separately and out of scope here.

> 📖 Before starting implementation, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## 1. Problem

RocketAPI users who run their own [RocketVault](https://github.com/) server (a
self-hosted, Azure-Key-Vault-parity secrets manager the user already operates,
located at `~/data/rocket/rocketvault`) have no way to pull secrets from it into
RocketAPI. Today the only secret storage is per-variable and purely local (OS
keychain via `KeyringSecretStore`, see the hardening spec) — there is no way for a
team to share one secret source across RocketAPI installs.

RocketVault exposes a REST API (Go, default port `:8774`) with an OAuth2
client-credentials grant (`POST /api/v1/oauth2/token`, RFC 6749 §4.4) meant for
exactly this kind of machine-to-machine access, separate from its human,
TOTP-gated CLI login. A reference Go client already exists in the RocketVault repo
(`internal/vaultclient/client.go`) and defines the exact wire contract this spec
replicates in Rust.

Bruno (a comparable open-source API client) ships an equivalent integration for
Azure Key Vault; this spec follows the same UX shape so the feature is
immediately familiar: a reusable, app-level "secret manager connection" plus a
per-environment "External Secrets" binding that fetches secret *names* once and
resolves *values* live at request-send time.

## 2. Goals

- A user can configure one or more named RocketVault connections at the app level
  (server URL, client ID; client secret stored in the OS keychain, never in a
  config file on disk).
- A user can bind a connection + a target vault name to a specific environment
  under an alias, and fetch the list of available secret names from that vault.
- `{{alias.secretName}}` resolves in request URL/headers/body exactly like a
  normal `{{variable}}`, fetching the real value from RocketVault at
  request-send time — never persisted to disk.
- `rok.getSecretVar('alias.secretName')` is available to pre/post-request and
  test scripts, alongside the existing `rok.getEnvVar`/`getVar`/etc. family.
- Values returned by either path are redacted from console/test-output text via
  the existing `VariableContext.secret_values` mechanism, exactly like local
  `secret: true` variables today.
- A connection or vault-reachability failure produces a clear, actionable error
  — never a silent empty-string substitution (which could send a broken or
  insecure request) and never a silent fallback to some other secret source.

## 3. Non-goals

- Not replacing or touching the existing `Variable.secret` / `SecretStore` /
  `KeyringSecretStore` mechanism at all. External Secrets is strictly additive —
  a separate, parallel concept, matching Bruno's own design (local secret
  variables and External Secrets coexist).
- Not writing to RocketVault (no create/update/delete of secrets from RocketAPI).
  This is a **read-only** integration in v1 — RocketVault secrets are managed via
  the existing RocketVault CLI/UI, RocketAPI only consumes them.
- Not supporting other secret-manager providers (AWS Secrets Manager, HashiCorp
  Vault, cloud Azure Key Vault). RocketVault only, for now — the connection model
  should not preclude adding providers later, but no other provider is built here.
- Not resolving external secrets inside `OAuth2Service`'s own variable
  interpolation (`crates/rocket-app/src/oauth2_service.rs`, which duplicates
  `build_variable_context`, per its own comment). Using an external-secret
  reference as part of an OAuth2 token-request field is a reasonable follow-up,
  explicitly deferred here to keep the resolution-path change scoped to
  `RequestExecutionService`/`CollectionRunnerService`.
- Not building an app-wide Preferences dialog beyond what this feature needs. A
  minimal "Secret Managers" panel is in scope; a general settings framework is
  not — see §4.5.
- Not caching fetched secret *values* to disk under any circumstances. Fetched
  secret *names* (not values) are the only External Secrets data that persists.

## 4. Design

### 4.1 RocketVault wire contract (verified against the reference Go client)

Confirmed directly from `~/data/rocket/rocketvault/internal/vaultclient/client.go`
and `~/data/rocket/rocketvault/api/secrets.go`:

- **Token:** `POST {base_url}/api/v1/oauth2/token`, `Content-Type:
  application/x-www-form-urlencoded`, body `grant_type=client_credentials&client_id=...&client_secret=...`.
  Response `200`: `{"access_token": "...", "expires_in": <seconds>}`. `401` means
  bad credentials (terminal, do not retry).
- **List secrets (vault-scoped):** `GET {base_url}/api/v1/vaults/{vault_name}/secrets`
  with `Authorization: Bearer <token>`. Returns the `model.ListSecretsResponse`
  envelope, `{"secrets": [...], "total": <count>}` — not a bare array — where
  each entry is a `{id (uuid), name, tags, version, created_at, enabled}`
  object (`model.SecretResponse`, confirmed at
  `~/data/rocket/rocketvault/api/secrets.go:189-200`, built explicitly
  "without values for security" — the server never sends a `value` field in a
  list response at all, unlike the single-secret get endpoint below). The
  parsing side still decodes defensively (only `id`/`name`, ignoring every
  other field via serde's default unknown-field skip) so this holds even if
  that ever changes, but the value-in-list-response premise itself is not
  something to design around. The endpoint is server-side paginated
  (`per_page` default 60, max 200); this integration requests `per_page=200`
  as a pragmatic mitigation and does not implement cursor-based pagination
  beyond that in v1.
- **Get secret value (vault-scoped):** `GET
  {base_url}/api/v1/vaults/{vault_name}/secrets/{secret_id}` with the same
  Bearer token. Response `200`: `{"value": "..."}`. `404` → secret gone
  (treat as unresolved, see §4.6). `401` → invalidate cached token, do not
  silently retry with the same token.
- Secrets are addressed by **UUID**, not name — RocketVault has no
  get-by-name endpoint. The reference client resolves name→UUID once via a
  static config mapping; this spec's equivalent is the one-time "Fetch Secrets"
  action (§4.3), which captures `{name, id}` pairs at that moment.
- Token caching: minimum 30s TTL enforced client-side even if `expires_in` is 0;
  refresh 60s before actual expiry (early-refresh cutoff), matching the
  reference client's `tokenExpiryCutoff`.
- Security default: require `https://` for any non-loopback `base_url` unless an
  explicit opt-out is set (mirrors the reference client's `AllowInsecureHTTP`) —
  loopback (`localhost`/`127.0.0.1`/`::1`) is always allowed over plain HTTP for
  local dev servers.

### 4.2 New infra client: `RocketVaultClient` (in `rocket-infra`)

> **Superseded by the implementation plans.** The plan series
> (`docs/superpowers/plans/rocketvault-external-secrets/`) replaces the
> single-connection `RocketVaultClient`/`RocketVaultConnectionConfig` shape
> below with a `VaultSecretFetcher` trait (`rocket-environment`, Plan 02) plus
> one stateless `ReqwestVaultSecretFetcher` implementation (`rocket-infra`,
> Plan 03) that takes the connection/secret as call parameters instead of
> being constructed bound to one connection. This keeps `rocket-app` free of
> any RocketVault-specific concrete type — it only ever holds `Arc<dyn
> VaultSecretFetcher>` — matching the DDD boundary rule
> (`.claude/rules/rust-ddd-boundaries.md`) more precisely than this section's
> original design. Every `RocketVaultClient` reference below is superseded;
> the plan index's "Locked interface contract" is authoritative for the
> actual shape.

New module `crates/rocket-infra/src/rocketvault/client.rs`, built on `reqwest`
(already a workspace dependency) — same crate/pattern `ReqwestExecutor` uses for
OAuth2 token fetches.

```rust
pub struct RocketVaultConnectionConfig {
    pub base_url: String,
    pub client_id: String,
    pub client_secret: String,   // resolved from OS keychain by the caller, never persisted here
    pub verify_ssl: bool,
    pub allow_insecure_http: bool,
}

pub struct RocketVaultClient { /* holds reqwest::Client, cfg, cached token behind a Mutex */ }

impl RocketVaultClient {
    pub fn new(cfg: RocketVaultConnectionConfig) -> DomainResult<Self>;
    pub async fn list_secrets(&self, vault_name: &str) -> DomainResult<Vec<RocketVaultSecretSummary>>; // {id, name}
    pub async fn get_secret_value(&self, vault_name: &str, secret_id: &str) -> DomainResult<Option<String>>;
    pub async fn test_connection(&self, vault_name: &str) -> DomainResult<()>; // token + list_secrets, no value fetch
}
```

Error handling mirrors the reference client's terminal-vs-retryable split: 401
invalidates the cached token and does not retry with the same token; 404 on
`get_secret_value` returns `Ok(None)` (soft, matches `SecretStore::get`'s
existing `Option<String>` contract); network/5xx errors bubble up as
`DomainError` for the caller to decide (soft-fail on read, hard-fail on
connection test — see §4.6).

### 4.3 App-level Secret Manager connections

**Storage.** A new small repository, `FsSecretManagerRepo`, writes
`~/.rocket-api/secret_managers.yml` — a flat list of connections: `{id, label,
base_url, client_id, verify_ssl, allow_insecure_http}`. This is app-wide, not
per-workspace (matches Bruno's global Preferences scoping, and sidesteps the
"which workspace owns a connection" question entirely). `client_secret` is never
written here — it goes to the OS keychain via the existing `keyring` crate,
under a new service label `com.rocketapi.vault-connection`, keyed by connection
`id` (same pattern as `KeyringSecretStore`, just a different service namespace so
the two never collide).

**Tauri commands** (new file `src-tauri/src/commands/secret_managers.rs`,
following the thin-command pattern in `rules/tauri-ipc-boundaries.md`):
`list_secret_manager_connections`, `save_secret_manager_connection`,
`delete_secret_manager_connection`, `test_secret_manager_connection(id, vault_name)`
(builds a `RocketVaultClient` from the stored config + keychain secret and calls
`test_connection`). All return mapped `DomainError`s, never raw reqwest errors.

**Frontend.** No app-level Preferences surface exists yet in this codebase (see
§4.5) — this feature is the first one, kept intentionally minimal: a dialog
(`SecretManagerConnectionsDialog`, modeled directly on the existing
`GitCredentialsDialog` for the "connection + secret credential" form pattern)
reachable from a new small entry point (a settings icon in the app's global
chrome — exact placement is a plan-time UI decision, not a design fork). Fields:
label, base URL (`SingleLineEditor`, no `{{var}}` awareness needed here since
this isn't request-scoped), client ID, client secret (masked `Input`), verify
SSL toggle, "Test Connection" button (needs a vault name to test against —
prompt for one inline, since RocketVault has no "list vaults" call usable
without a specific vault-scoped grant).

### 4.4 Domain model: `ExternalSecretBinding` on `Environment`

New type in `rocket-environment` (`crates/rocket-environment/src/external_secret.rs`),
alongside `variable.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretBinding {
    pub alias: String,               // used as {{alias.secretName}} and rok.getSecretVar('alias.secretName')
    pub connection_id: String,       // references a Secret Manager connection (§4.3)
    pub vault_name: String,
    pub secret_names: Vec<ExternalSecretRef>, // {name, secret_id} pairs captured at "Fetch Secrets" time
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSecretRef {
    pub name: String,
    pub secret_id: String, // RocketVault UUID, captured once so lookups skip a list round-trip
}
```

`Environment` (`crates/rocket-environment/src/environment.rs`) gains
`#[serde(default)] pub external_secrets: Vec<ExternalSecretBinding>` — default
empty, backward-compatible with every existing environment `.yml` per the
crate's own "all new fields use `#[serde(default, ...)]`" rule. This is **not**
routed through `OcVariable`/`OcSecretVariable` (§4.5 of the hardening spec) —
it's a new top-level field on `OcEnvironment`
(`crates/rocket-infra/src/oc/environment.rs`), a new section in the persisted
YAML, not a variable entry. No secret *values* ever appear in this struct or its
YAML representation — only names and RocketVault's own UUIDs, which are not
sensitive (matches how `OcSecretVariable` already omits `value` by construction).

**Refreshing the name list** ("Fetch Secrets", re-run any time) calls
`RocketVaultClient::list_secrets(vault_name)` and replaces `secret_names`
wholesale — additions/removals on the RocketVault side are picked up the next
time the user clicks Fetch, not automatically.

### 4.5 Frontend: External Secrets tab

`EnvironmentDialog.tsx` currently renders one flat `VariableTable` per selected
environment with no internal tabs. Add a lightweight tab switcher (`Variables` /
`External Secrets`) inside the dialog body, and a new `ExternalSecretsTab.tsx`
component: a list of `ExternalSecretBinding` rows (alias, connection picker
sourced from `list_secret_manager_connections`, vault name field, "Fetch
Secrets" button, and the resulting name list rendered read-only underneath each
binding). Uses shadcn primitives only, per the hard rules — no new UI framework.

New TS types in `src/lib/tauri-api.ts`, mirroring §4.4:

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

export interface Environment {
  name: string;
  variables: Variable[];
  externalSecrets: ExternalSecretBinding[]; // new
}
```

(Note the existing `Variable` TS interface is already missing `description`/
`valueVariants`/`secretType`, which exist on the Rust side — pre-existing drift,
unrelated to this spec, not fixed here.)

### 4.6 Resolution: request execution and the collection runner

**Where values get fetched.** `RequestExecutionService::execute`
(`crates/rocket-app/src/execution_service.rs:1281`) is `async`; the phase
methods it drives (`begin_phases`, `resolve_request`, `build_variable_context`,
`build_variable_scopes`) are currently synchronous and read only from local
repositories. `CollectionRunnerService`
(`crates/rocket-app/src/collection_runner_service.rs:362`) independently calls
`begin_phases` once per step in a run loop.

Add a new **async** method:

```rust
impl RequestExecutionService {
    /// Fetches real values for every ExternalSecretRef in the active
    /// environment's external_secrets bindings. Returns a flat map keyed
    /// "{alias}.{secretName}" -> value, merged into the variable maps
    /// consumed by build_variable_context/build_variable_scopes.
    ///
    /// Reads the named environment through regular_env_repo(collection), not
    /// self.env_repo directly — the environment a real request uses is
    /// collection-scoped in the normal case, served by a different repo than
    /// the app-level ("global") one self.env_repo points at. A missing
    /// environment soft-fails to an empty map, matching
    /// build_variable_scopes's own convention for this lookup.
    pub async fn resolve_external_secrets(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
    ) -> DomainResult<HashMap<String, String>>;
}
```

Called once by `execute()` before `begin_phases()`, and once by
`CollectionRunnerService` at the **start of a run** (not per-step — the active
environment's bindings don't change mid-run, and re-fetching per request would
multiply RocketVault round-trips unnecessarily). The resulting map is threaded
as an added parameter into `build_variable_context`/`build_variable_scopes`
(and therefore `resolve_request`/`begin_phases`), merged into the flattened map
at the same precedence tier as `env` (an external secret is conceptually
environment-scoped data) — exact signature threading is a plan-time detail, but
the insertion points are these four methods
(`execution_service.rs:259,321,334,831`) plus the runner's call site
(`collection_runner_service.rs:362`). Every value pulled in is also inserted
into `var_ctx.secret_values` (same set already used for the local-secret
redaction mechanism), so console/test-output redaction covers it for free —
no changes needed to `crates/rocket-infra/src/scripting/ops/console.rs`.

**Unresolved reference behavior.** If `{{alias.secretName}}` doesn't match any
configured binding/name, it is left as-is by `resolve()` exactly like today's
behavior for any unknown variable (`ResolveResult::unresolved`) — no new error
path needed there. If it *does* match a binding but the live fetch fails
(network/auth/404), the request is **not silently sent with a broken
placeholder** — `resolve_external_secrets` returns an error, and `execute()`
surfaces it before dispatch (matches the "never a silent empty-string
substitution" goal in §2). This is a deliberate asymmetry from
`KeyringSecretStore`'s soft-fail-on-read: a local keychain read failing is
treated as "not yet set" (empty), but a *configured, previously-successfully-fetched*
external secret name failing to resolve at send time is a hard stop, because
here the name is already known to exist — a failure means the network/vault is
unreachable right now, not that the secret was never set.

### 4.7 Scripting: `rok.getSecretVar`

New op in `crates/rocket-infra/src/scripting/ops/rok.rs`, following the exact
shape of `op_rok_get_env_var`:

```rust
/// rok.getSecretVar('alias.secretName') — reads a fetched External Secret value.
#[op2]
#[string]
pub fn op_rok_get_secret_var(state: &OpState, #[string] key: String) -> String {
    state
        .borrow::<ScriptInputState>()
        .variables
        .external_secrets // new field on VariableContext, populated per §4.6
        .get(&key)
        .cloned()
        .unwrap_or_default()
}
```

`VariableContext` (`crates/rocket-environment/src/context.rs`) gains
`pub external_secrets: HashMap<String, String>` (dotted `alias.secretName` keys)
— kept as its own field rather than folded into `env`, so `rok.getEnvVar` never
accidentally returns a vault-sourced value through an unrelated code path, and
so `{{alias.secretName}}` resolution (§4.6) and `rok.getSecretVar` read from the
exact same source of truth. `flatten()`/`flatten_with_process_env()` fold it in
at the same tier as `env` per §4.6.

### 4.8 Redaction

No new redaction code — §4.6 and §4.7 both populate `var_ctx.secret_values`,
which the already-shipped `redact()` helper in
`crates/rocket-infra/src/scripting/ops/console.rs` already scans (per the
secret-aware-variable-context spec, already implemented). Same
`MIN_REDACTION_LEN` (6-char) threshold applies.

## 5. Interfaces (for the implementation plan)

- `rocket_infra::rocketvault::RocketVaultClient` — new, `list_secrets`/
  `get_secret_value`/`test_connection`, `reqwest`-backed.
- `rocket_infra::FsSecretManagerRepo` — new, `~/.rocket-api/secret_managers.yml`.
- `rocket_environment::external_secret::{ExternalSecretBinding, ExternalSecretRef}` — new.
- `Environment.external_secrets: Vec<ExternalSecretBinding>` — new field, `#[serde(default)]`.
- `OcEnvironment.external_secrets` (`crates/rocket-infra/src/oc/environment.rs`) plus the
  corresponding `From` conversions (`crates/rocket-infra/src/conversions/environment.rs`) —
  new top-level YAML section, mirroring `ExternalSecretBinding`/`ExternalSecretRef` 1:1
  (no value field, ever).
- `VariableContext.external_secrets: HashMap<String, String>` — new field. `flatten()` and
  `flatten_with_process_env()` (`crates/rocket-environment/src/context.rs`) must be updated to
  fold this field in at the same precedence position as `env` — currently they only iterate
  `global_env`/`collection`/`env`/`folder`/`request`/`runtime`.
- `RequestExecutionService::resolve_external_secrets(...)` — new async method.
- `build_variable_context`/`build_variable_scopes`/`resolve_request`/`begin_phases`
  (execution_service.rs) — signature changes to accept the resolved external-secrets map.
- `CollectionRunnerService` — one `resolve_external_secrets` call per run, not per step.
- New Tauri commands: `list_secret_manager_connections`, `save_secret_manager_connection`,
  `delete_secret_manager_connection`, `test_secret_manager_connection`,
  `fetch_external_secret_names` (calls `list_secrets`, used by the "Fetch Secrets" button).
- `rok.getSecretVar(key)` — new scripting op.
- Frontend: `ExternalSecretsTab.tsx`, `SecretManagerConnectionsDialog.tsx`,
  new TS types in `tauri-api.ts` (§4.5).

## 6. Security considerations

- `client_secret` for every connection lives only in the OS keychain — never in
  `secret_managers.yml`, never in an `Environment` YAML, never logged.
- Fetched secret *values* are never written to disk in any form (no cache file,
  no environment YAML field) — only names/UUIDs persist, matching the "Fetch
  Secrets" list-only semantics in §4.1/§4.4.
- `https://` required for non-loopback `base_url`s by default (§4.1), matching
  the reference client's guard rail.
- A test-connection failure or a live-resolution failure surfaces as a visible
  error, never a silent fallback to another secret source or an empty value
  substitution (§4.6) — sending a request with a missing credential silently is
  worse than failing loudly.

## 7. Testing

- `wiremock` for `RocketVaultClient` (token issuance incl. 401, `list_secrets`,
  `get_secret_value` incl. 404, token refresh/early-cutoff behavior) — same
  pattern already used for `ReqwestExecutor`'s OAuth2 tests.
- `tempfile` fixtures for `FsSecretManagerRepo` round-trip (connection metadata
  persists; `client_secret` never appears in the written YAML).
- `rocket-app` tests for `resolve_external_secrets` merging into
  `build_variable_context`/`build_variable_scopes`, using an in-memory fake
  `RocketVaultClient` (mirroring existing inline-mock test patterns in
  `execution_service.rs`).
- Frontend: component tests for `ExternalSecretsTab` (fetch/save flow) and
  `SecretManagerConnectionsDialog`, following existing patterns (e.g.
  `GitCredentialsDialog.test.tsx`).
- No real RocketVault server in CI.

## 8. Acceptance criteria

1. A user can add a RocketVault connection (label, base URL, client ID, client
   secret) and the client secret never appears in any file under
   `~/.rocket-api/` — only in the OS keychain.
2. "Test Connection" against a real local RocketVault instance (manual/dev
   verification, not CI) succeeds with valid credentials and a valid vault name,
   and fails clearly with invalid credentials or an unreachable server.
3. Binding a connection + vault + alias to an environment and clicking "Fetch
   Secrets" populates the name list; the environment's `.yml` file contains
   only names and RocketVault UUIDs for that binding, never a secret value.
4. A request whose URL/header/body contains `{{alias.secretName}}` for a
   fetched name resolves to the real value at send time; the value never
   appears in the saved environment YAML or in `rocket-history`.
5. `console.log(rok.getSecretVar('alias.secretName'))` in a script produces a
   redacted `"••••••"` output, matching the existing local-secret redaction
   behavior.
6. If the RocketVault connection is unreachable at send time, `execute()`
   returns an error before dispatch rather than sending a request with a
   literal, unresolved `{{alias.secretName}}` in it.
7. Running a Collection Runner run against a folder that uses external secrets
   issues exactly one `list`/value-resolution pass per run, not one per request
   in the run (verified via a call-count assertion against the fake
   `RocketVaultClient` in tests).
8. `cargo test -p rocket-environment -p rocket-infra -p rocket-app` and
   `yarn tsc --noEmit` both pass.
