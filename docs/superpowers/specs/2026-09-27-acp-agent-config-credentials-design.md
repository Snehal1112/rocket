# ACP Agent Configuration & Credentials (Subproject A)

## Context

Rocket is gaining an AI assist feature in the Scripts tab, built on the Agent Client Protocol (ACP): a fully agentic, autonomous, whole-collection-scoped assistant that writes and validates pre-request/test scripts by calling back into Rocket via a local MCP tool server. The full feature was decomposed into five subprojects (build order A → B → {C, D in parallel} → E); see the project memory `project_acp_ai_assist_feature.md` for the complete decomposition and the decisions that apply across all of them.

This spec covers **only subproject A**: letting the user register one or more external ACP agent binaries/commands and giving each one a credential sourced from RocketVault, with nothing yet that speaks the ACP protocol or spawns a live agent session. Subproject B (the `rocket-acp` transport/session layer) builds on top of what this spec produces.

**Why RocketVault, and why not the existing environment-secret flow:** RocketVault secrets are resolvable today only through `ExternalSecretBinding {alias, connection_id, vault_name, secret_names}`, which is bound to a specific `Environment` and surfaced to scripts as pre-resolved `{{alias.secretName}}` template variables. An agent's API key is an app-global setting, not tied to any environment, so it cannot use that binding/alias flow. Instead it uses the lower-level `SecretManagerConnection` + `VaultSecretFetcher` machinery directly — the same underlying trait, a different (unbound, on-demand) call path.

**Scope decision (confirmed):** credentials are sourced from RocketVault only for v1 — no local/direct-paste fallback into Rocket's own OS keychain. If that's ever needed it's a small, additive change later (the generic `SecretStore` trait already supports it).

## Domain Model (new crate `rocket-acp`)

A new crate, `rocket-acp`, holds the config shape. It is a pure domain crate: it depends only on `rocket-shared`, and references other entities (a vault connection, a vault secret) by plain `String` id rather than importing `rocket-environment` types — the same loose-coupling convention used elsewhere between domain crates.

```rust
pub struct AgentConfig {
    pub id: String,
    pub label: String,
    pub command: String,             // binary path, or a bare name resolved via PATH
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub credential_env_var: String,  // e.g. "ANTHROPIC_API_KEY" — name the spawned process expects
    pub vault_connection_id: String, // -> SecretManagerConnection.id
    pub vault_name: String,
    pub vault_secret_id: String,     // -> ExternalSecretRef.secret_id
    pub vault_secret_name: String,   // ExternalSecretRef.name, display-only
}

pub trait AgentConfigRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<AgentConfig>>;
    fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>>;
    fn save(&self, config: &AgentConfig) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}
```

This is a direct structural mirror of `SecretManagerConnection`/`SecretManagerRepository` (crates/rocket-environment/src/secret_manager.rs): no secret material lives on the struct, only a reference to where to fetch it.

Multiple named agent configs are supported (a list, like `SecretManagerConnection`), since the backend is pluggable and a user may want more than one configured agent to choose from later, in subproject C's UI.

## Persistence (`rocket-infra`)

`FsAgentConfigRepo` implements `AgentConfigRepository`, storing the full list as one flat YAML file (`agent_configs.yml`) directly under the app data directory — byte-for-byte the same pattern as `FsSecretManagerRepo` (crates/rocket-infra/src/fs_secret_manager_repo.rs): `read_all`/`write_all` via `atomic_write`, missing/empty file treated as an empty list, `save` replaces by id rather than appending duplicates.

Because `rocket-infra` is on this repo's OpenCollection-spec trigger list, the implementation plan's task for `FsAgentConfigRepo` must start with reading `docs/superpowers/specs/opencollection-spec-reference.md` first, per the rule in `.claude/rules/rust-ddd-boundaries.md` — even though this data has nothing to do with collections or environments, the rule triggers on the crate touched, not the content.

## Credential Resolution (`rocket-app`)

`AgentConfigService` orchestrates config CRUD and credential resolution:

```rust
pub struct AgentConfigService {
    repo: Box<dyn AgentConfigRepository>,
    secret_manager: Arc<SecretManagerService>,
}
```

One new public method is added to the existing `SecretManagerService` (crates/rocket-app/src/secret_manager_service.rs) rather than duplicating its private connection+keychain lookup:

```rust
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

`AgentConfigService` methods:

- `list()` / `delete(id)` — passthrough to the repository.
- `save(config)` — validates that `label`, `command`, `credential_env_var`, and `vault_secret_id` are non-empty, and that `vault_connection_id` refers to an existing `SecretManagerConnection`. It does **not** call out to RocketVault to confirm the secret itself still exists — matches the existing pattern (per `VaultSecretFetcher::get_secret_value`'s doc comment) where a stale vault reference is discovered on use, not on save.
- `resolve_credential(id) -> DomainResult<String>` — loads the `AgentConfig` and calls `secret_manager.resolve_secret_value(...)`. Maps a fetcher result of `Ok(None)` (the vault secret no longer exists) to `Err(DomainError::NotFound("agent '{label}': credential no longer exists in RocketVault — reconfigure this agent"))`. This must be a hard error rather than treated as a silently-unresolved value (the way an unresolved `{{template}}` variable is) because subproject B cannot spawn the agent process at all without a credential.
- `test_agent_config(id) -> DomainResult<()>` — resolves `command` to an existing, executable path using the `which` crate (new dependency for `rocket-app`) — no process is spawned; actually spawning and speaking ACP is subproject B's job — and calls `resolve_credential` to confirm the credential resolves. Backs a "Test Agent" action in the settings UI.

Error mapping is consistent with the rest of the codebase: unknown id → `DomainError::NotFound`; blank required fields or an unresolvable `vault_connection_id` at save time → `DomainError::InvalidInput`; keychain or vault transport failures → `DomainError::Internal`, propagated unchanged from `SecretManagerService`.

## Tauri IPC

New module `src-tauri/src/commands/agent_configs.rs`, mirroring `secret_managers.rs`:

- `list_agent_configs`
- `save_agent_config`
- `delete_agent_config`
- `test_agent_config`

Each is a thin wrapper over `AgentConfigService`: validate input shape, call the service, map `DomainError` to the stable IPC error shape. Per this repo's hard rule, the domain `AgentConfig` struct is never annotated with a serde rename; a dedicated `AgentConfigDto` with `#[serde(rename_all = "camelCase")]` is defined at the command layer and converted to/from the domain type there.

## Frontend UI

A new "AI Agents" section in Settings, alongside the existing Secret Manager Connections UI. A table (shadcn `Table`, `lucide-react` icons for row actions) lists configured agents with Add/Edit/Delete/Test actions. The Add/Edit dialog (shadcn `Dialog`) has plain `SingleLineEditor` fields for label, command, args, working directory, and credential env var name (no `{{variable}}` templating needed for any of these), plus a vault-connection/vault-name/secret picker that reuses the existing external-secret-binding picker flow (`fetch_external_secret_names` → pick one `ExternalSecretRef`). The "Test" action calls `test_agent_config` and shows a pass/fail result inline, matching the existing "Test Connection" UX for vault connections.

## Testing

- `rocket-acp`: serde round-trip and `AgentConfigRepository` object-safety tests, mirroring `crates/rocket-environment/src/secret_manager.rs`'s test module.
- `rocket-infra`: `FsAgentConfigRepo` CRUD/roundtrip tests with `tempfile`, mirroring `fs_secret_manager_repo.rs` — including a regression test that persisted YAML never contains anything secret-shaped, since (as with `SecretManagerConnection`) there is no secret field on this struct to leak.
- `rocket-app`: `AgentConfigService` tests with fake repo/service doubles covering: successful `resolve_credential`, stale-secret → `NotFound` mapping, `save` validation rejections (blank fields, unknown `vault_connection_id`), unknown-config-id rejection.
- `yarn tsc --noEmit` for the DTO/frontend wiring; `cargo check -p rocket-acp -p rocket-infra -p rocket-app -j4` and the above `cargo test` targets, also with `-j4` per this repo's convention.

## Out of scope for this subproject

- Anything that speaks the ACP protocol, spawns a live agent process, or performs a real handshake — that is subproject B.
- The chat UI, MCP tool server, and autonomous-execution safety valve — subprojects C, D, and E respectively.
- A local/direct-paste credential fallback that bypasses RocketVault — explicitly deferred (see Scope decision above).
