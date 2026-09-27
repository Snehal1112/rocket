# ACP Agent Config Plan 04: Tauri Commands — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose `AgentConfigService` over Tauri IPC (`list_agent_configs`,
`save_agent_config`, `delete_agent_config`, `test_agent_config`) and wire the
service into `src-tauri/src/lib.rs`.

**Architecture:** A new `src-tauri/src/commands/agent_configs.rs` module, thin
wrappers over `AgentConfigService` with an `AgentConfigDto` conversion layer —
the exact structure `src-tauri/src/commands/secret_managers.rs` already uses
for `SecretManagerConnectionDto`.

**Tech Stack:** Rust, Tauri 2 commands, serde.

**Spec:** `docs/superpowers/specs/2026-09-27-acp-agent-config-credentials-design.md`
(Tauri IPC section). Plan index:
`docs/superpowers/plans/acp-agent-config-credentials/00-plan-index.md`.

## Global Constraints

- The domain `AgentConfig` (from `rocket-acp`) is never annotated with a serde
  rename. `AgentConfigDto`, defined at the command layer, carries
  `#[serde(rename_all = "camelCase")]` and is converted to/from the domain
  type via `From`/`Into` — per this repo's hard rule ("Serde: `#[serde(rename_all
  = "camelCase")]` on IPC DTOs only — never on persistence structs") and
  matching `SecretManagerConnectionDto`'s existing pattern exactly
  (`src-tauri/src/commands/secret_managers.rs:8-30`).
- `list_agent_configs`/`save_agent_config`/`delete_agent_config` are
  synchronous commands (`AgentConfigService`'s corresponding methods are
  sync); `test_agent_config` is `async` (the underlying service method is
  `async`) — match `secret_managers.rs`'s existing split between its sync
  CRUD commands and its `async fn test_secret_manager_connection`.
- Wire a **second**, independent `SecretManagerService` instance for
  `AgentConfigService` in `src-tauri/src/lib.rs`, rather than changing the
  type already managed for the existing secret-manager commands
  (`State<'_, SecretManagerService>` at
  `src-tauri/src/commands/secret_managers.rs:63` etc.) to `Arc<...>` — that
  would force updating every existing command signature, well outside this
  plan's scope. This repo already constructs a second
  `FsSecretManagerRepo::new(data_dir.join("secret_managers.yml"))` for
  `exec_svc` (`src-tauri/src/lib.rs:305-309`) alongside the one for
  `secret_manager_svc` (`src-tauri/src/lib.rs:286-292`); a third
  construction for `AgentConfigService` follows that same established
  pattern.

## Review Focus

- `save_agent_config` with a DTO whose `id` matches an existing config must
  replace it, not create a duplicate — this is `AgentConfigService::save`'s
  existing behavior (Plan 03), but confirm the DTO conversion doesn't drop or
  mutate `id` along the way.
- A `DomainError::InvalidInput` from `AgentConfigService::save` (e.g. unknown
  `vault_connection_id`) must reach the frontend as a structured, readable
  error via Tauri's error serialization — not a generic/opaque failure. This
  matches how `secret_managers.rs`'s commands already return
  `Result<_, DomainError>` and rely on `DomainError`'s existing serialization.
- `test_agent_config` must remain callable (and useful) even when the config's
  `vault_secret_id` is stale — the command itself does no error translation
  beyond propagating `AgentConfigService::test_agent_config`'s result, so a
  stale-secret `DomainError::NotFound` must reach the frontend intact, not get
  mapped to something less specific.
- Registering the new commands in `tauri::generate_handler!` without also
  adding `pub mod agent_configs;` to `src-tauri/src/commands/mod.rs` is a
  common one-line omission that fails compilation — confirm both are present.
- The second `SecretManagerService` instance for `AgentConfigService` must
  share the *same* `vault_connection_secret_store`/`vault_fetcher` `Arc`s as
  the existing `secret_manager_svc`/`exec_svc` (via `Arc::clone`), not
  construct fresh ones — otherwise a connection's client secret saved through
  the existing Secret Manager Connections UI would be invisible to
  `AgentConfigService`'s credential resolution (two different keychain-store
  instances pointing at the same underlying OS keychain would still work
  correctly here since `KeyringSecretStore` is stateless per call, but two
  different `ReqwestVaultSecretFetcher` instances would each hold their own
  independent token cache — sharing the `Arc` avoids a redundant, easily
  missed second cache).

---

## Task 1: `AgentConfigDto` and command module

**Files:**
- Create: `src-tauri/src/commands/agent_configs.rs`
- Modify: `src-tauri/src/commands/mod.rs`

**Interfaces:**
- Consumes: `AgentConfig` (`rocket-acp`), `AgentConfigService` (`rocket-app`,
  Plan 03).
- Produces: `AgentConfigDto`, `list_agent_configs`, `save_agent_config`,
  `delete_agent_config`, `test_agent_config` — consumed by Task 2 of this
  plan (handler registration) and Plan 05 (frontend `invoke` calls).

- [ ] **Step 1: Write the failing test**

```rust
// src-tauri/src/commands/agent_configs.rs
use rocket_acp::AgentConfig;
use rocket_app::AgentConfigService;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigDto {
    pub id: String,
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub credential_env_var: String,
    pub vault_connection_id: String,
    pub vault_name: String,
    pub vault_secret_id: String,
    pub vault_secret_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_domain() -> AgentConfig {
        AgentConfig {
            id: "agent-1".to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: vec!["--stdio".to_string()],
            working_dir: Some("/home/user/project".to_string()),
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "secret-id-1".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    #[test]
    fn dto_serializes_camelcase() {
        let dto: AgentConfigDto = sample_domain().into();
        let json = serde_json::to_string(&dto).expect("serialize AgentConfigDto");
        assert!(json.contains("\"credentialEnvVar\""), "expected camelCase, got: {json}");
        assert!(json.contains("\"vaultConnectionId\""), "expected camelCase, got: {json}");
        assert!(json.contains("\"workingDir\""), "expected camelCase, got: {json}");
    }

    #[test]
    fn dto_roundtrips_through_domain_type() {
        let original = sample_domain();
        let dto: AgentConfigDto = original.clone().into();
        let back: AgentConfig = dto.into();
        assert_eq!(original, back);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo check -p rocket -j4`
Expected: FAIL — `From<AgentConfig> for AgentConfigDto` (and the reverse) do
not exist yet, so `.into()` does not resolve.

- [ ] **Step 3: Implement the conversions and commands**

```rust
// src-tauri/src/commands/agent_configs.rs (add above the tests module)

impl From<AgentConfig> for AgentConfigDto {
    fn from(c: AgentConfig) -> Self {
        Self {
            id: c.id,
            label: c.label,
            command: c.command,
            args: c.args,
            working_dir: c.working_dir,
            credential_env_var: c.credential_env_var,
            vault_connection_id: c.vault_connection_id,
            vault_name: c.vault_name,
            vault_secret_id: c.vault_secret_id,
            vault_secret_name: c.vault_secret_name,
        }
    }
}

impl From<AgentConfigDto> for AgentConfig {
    fn from(dto: AgentConfigDto) -> Self {
        Self {
            id: dto.id,
            label: dto.label,
            command: dto.command,
            args: dto.args,
            working_dir: dto.working_dir,
            credential_env_var: dto.credential_env_var,
            vault_connection_id: dto.vault_connection_id,
            vault_name: dto.vault_name,
            vault_secret_id: dto.vault_secret_id,
            vault_secret_name: dto.vault_secret_name,
        }
    }
}

#[tauri::command]
pub fn list_agent_configs(
    svc: State<'_, AgentConfigService>,
) -> Result<Vec<AgentConfigDto>, DomainError> {
    Ok(svc.list()?.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn save_agent_config(
    config: AgentConfigDto,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.save(config.into())
}

#[tauri::command]
pub fn delete_agent_config(
    id: String,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.delete(&id)
}

#[tauri::command]
pub async fn test_agent_config(
    id: String,
    svc: State<'_, AgentConfigService>,
) -> Result<(), DomainError> {
    svc.test_agent_config(&id).await
}
```

- [ ] **Step 4: Register the module**

In `src-tauri/src/commands/mod.rs`, add alongside the existing `pub mod
secret_managers;` declaration:

```rust
pub mod agent_configs;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket agent_configs::tests -j4`
Expected: PASS — 2 tests. Also run `cargo check -p rocket -j4` to confirm the
command functions themselves compile (they are not exercised by a unit test
here — they require a running Tauri app context, consistent with how
`secret_managers.rs`'s commands are verified: compile-checked here, exercised
end-to-end once Plan 05's frontend UI calls them).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/agent_configs.rs src-tauri/src/commands/mod.rs
git commit -m "feat(tauri): add agent config commands"
```

---

## Task 2: Wire `AgentConfigService` into `lib.rs`

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `AgentConfigService` (Plan 03), `FsAgentConfigRepo` (Plan 02),
  the existing `vault_connection_secret_store`/`vault_fetcher` `Arc`s and
  `data_dir` already in scope in this closure
  (`src-tauri/src/lib.rs:279-292`).
- Produces: `agent_config_svc` registered as managed state, the four new
  commands registered in `tauri::generate_handler!` — consumed by Plan 05's
  frontend `invoke` calls.

- [ ] **Step 1: Construct the service**

In `src-tauri/src/lib.rs`, immediately after the existing
`secret_manager_svc` construction (right after the closing `);` at line 292,
before the `exec_svc` construction begins at line 294), add:

```rust
// A second SecretManagerService instance, dedicated to AgentConfigService,
// sharing the same vault_connection_secret_store/vault_fetcher Arcs as
// secret_manager_svc/exec_svc above — see this plan's Global Constraints for
// why this isn't a shared Arc<SecretManagerService> instead.
let agent_config_secret_manager = Arc::new(rocket_app::SecretManagerService::new(
    Box::new(rocket_infra::FsSecretManagerRepo::new(
        data_dir.join("secret_managers.yml"),
    )),
    Arc::clone(&vault_connection_secret_store),
    Arc::clone(&vault_fetcher),
));

let agent_config_svc = rocket_app::AgentConfigService::new(
    Box::new(rocket_infra::FsAgentConfigRepo::new(
        data_dir.join("agent_configs.yml"),
    )),
    agent_config_secret_manager,
);
```

- [ ] **Step 2: Register as managed state**

Add, alongside the existing `app.manage(secret_manager_svc);` at line 364:

```rust
app.manage(agent_config_svc);
```

- [ ] **Step 3: Register the commands**

In the `tauri::generate_handler!` list, add alongside the existing
`commands::secret_managers::fetch_external_secret_names,` entry:

```rust
commands::agent_configs::list_agent_configs,
commands::agent_configs::save_agent_config,
commands::agent_configs::delete_agent_config,
commands::agent_configs::test_agent_config,
```

- [ ] **Step 4: Verify the full app builds**

Run: `cargo check -p rocket -j4`
Expected: succeeds — confirms the new service wiring and command
registration compile against the real `AgentConfigService`/
`FsAgentConfigRepo` types (not test doubles), and that no existing command or
service construction was disturbed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(tauri): wire AgentConfigService into app state"
```

---

## Next Plan

[Plan 05: Frontend AI Agents settings dialog](2026-09-27-acp-agent-config-credentials-plan-05-frontend-ui.md) —
adds the TypeScript types, API wrappers, and settings UI consuming the four
commands from this plan.

## Post-Implementation Review

Before starting Plan 05, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `src-tauri/src/commands/agent_configs.rs`, `src-tauri/src/commands/mod.rs`,
> `src-tauri/src/lib.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — do the four commands and
>    `AgentConfigDto` match exactly what the plan index's locked interface
>    contract promises Plan 05's frontend will consume (camelCase field
>    names, parameter names matching what `#[tauri::command]` expects from a
>    frontend `invoke` call)?
> 2. Code quality — naming, error propagation versus this plan's Review Focus
>    section (structured `DomainError` reaching the frontend intact, no
>    silent duplication when saving an existing id).
> 3. DDD/IPC boundary conformance per `.claude/rules/tauri-ipc-boundaries.md`
>    — commands stay thin (validate/call-service/map-output only, no domain
>    logic), and the camelCase rename appears only on `AgentConfigDto`, never
>    on the domain `AgentConfig`.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket agent_configs::tests -j4` and
> `cargo check -p rocket -j4`, and confirm they still pass. Report what you
> found and fixed.

Only proceed to Plan 05 once this review comes back clean (or its fixes are
applied and re-verified).
