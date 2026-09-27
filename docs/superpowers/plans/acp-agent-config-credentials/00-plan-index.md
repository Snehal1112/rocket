# ACP Agent Configuration & Credentials — Plan Index

**Spec:** [../../specs/2026-09-27-acp-agent-config-credentials-design.md](../../specs/2026-09-27-acp-agent-config-credentials-design.md)

**Context:** this is subproject A of the larger ACP AI-assist feature (see the
project memory `project_acp_ai_assist_feature.md` for the full A→E
decomposition). It delivers only: registering one or more external ACP agent
binaries and sourcing each one's API key from RocketVault. No ACP protocol
code, no process spawning, no chat UI — those are later subprojects.

## Plan breakdown — 5 plans, 10 tasks (max 3 per plan)

| # | Plan | Tasks | Crate/area | Depends on |
|---|---|---|---|---|
| 01 | [Domain crate: AgentConfig + AgentConfigRepository](2026-09-27-acp-agent-config-credentials-plan-01-domain-crate.md) | 2 | `rocket-acp` (new) | — |
| 02 | [FsAgentConfigRepo persistence](2026-09-27-acp-agent-config-credentials-plan-02-persistence.md) | 1 | `rocket-infra` | 01 |
| 03 | [AgentConfigService + credential resolution](2026-09-27-acp-agent-config-credentials-plan-03-app-service.md) | 3 | `rocket-app` | 01, 02 |
| 04 | [Tauri commands + service wiring](2026-09-27-acp-agent-config-credentials-plan-04-tauri-commands.md) | 2 | `src-tauri` | 03 |
| 05 | [Frontend: AI Agents settings dialog](2026-09-27-acp-agent-config-credentials-plan-05-frontend-ui.md) | 2 | frontend | 04 |

Plan 01 is 2 tasks, not 3 — the struct and its repository trait are the only
two genuinely separable slices; a third task would just split one of them in
half. Plan 02 is a single task — `FsAgentConfigRepo` mirrors
`FsSecretManagerRepo` exactly with none of the extra keychain-generalization
or OpenCollection-nesting work the equivalent RocketVault plan (Plan 04 of
`rocketvault-external-secrets`) needed, so there is no second slice to carve
out. Plan 05 is 2 tasks — this repo currently has no multi-section Settings
shell to plug into (the existing `SecretManagerConnectionsDialog` is a
standalone dialog opened from its own `TitleBar` icon), so the frontend work
is "build the dialog" + "wire it into `TitleBar`", with no natural third
slice.

Each plan file ends with a **Next Plan** section naming the file above and
linking to it, so a fresh Claude Code session opening any single plan file
knows exactly what to run next without needing this index. Each plan file
also ends with a **Post-Implementation Review** section: before moving to the
next plan, dispatch an Opus-model subagent to review everything that plan
added or modified for interface gaps, code quality, and DDD boundary
conformance, with authority to fix what it finds.

## Locked interface contract

Every plan below is written against these exact types/signatures. If an
implementer needs to deviate, they must update this index and every
downstream plan file that references the changed name — do not let two plan
files disagree on a signature.

### `rocket-acp` (new, Plan 01)

```rust
// crates/rocket-acp/src/agent_config.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    pub credential_env_var: String,
    pub vault_connection_id: String,
    pub vault_name: String,
    pub vault_secret_id: String,
    pub vault_secret_name: String,
}
// NOTE: plain field names (no camelCase rename) — this is app-level config
// persisted to its own agent_configs.yml, not the OpenCollection format, so
// the general "no camelCase on persistence structs" rule applies, exactly
// like SecretManagerConnection.

pub trait AgentConfigRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<AgentConfig>>;
    fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>>;
    fn save(&self, config: &AgentConfig) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}
```

### `rocket-infra` (new, Plan 02)

```rust
// crates/rocket-infra/src/fs_agent_config_repo.rs
pub struct FsAgentConfigRepo { /* path: PathBuf, points at agent_configs.yml */ }
impl FsAgentConfigRepo {
    pub fn new(path: PathBuf) -> Self;
}
impl AgentConfigRepository for FsAgentConfigRepo { /* ... */ }
```

### `rocket-app` (modified, Plan 03 Task 1)

```rust
// crates/rocket-app/src/secret_manager_service.rs — new method
impl SecretManagerService {
    pub async fn resolve_secret_value(
        &self,
        connection_id: &str,
        vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>>;
}
```

### `rocket-app` (new, Plan 03 Tasks 2-3)

```rust
// crates/rocket-app/src/agent_config_service.rs
pub struct AgentConfigService {
    repo: Box<dyn AgentConfigRepository>,
    secret_manager: Arc<SecretManagerService>,
}
impl AgentConfigService {
    pub fn new(repo: Box<dyn AgentConfigRepository>, secret_manager: Arc<SecretManagerService>) -> Self;
    pub fn list(&self) -> DomainResult<Vec<AgentConfig>>;
    pub fn save(&self, config: AgentConfig) -> DomainResult<()>;
    pub fn delete(&self, id: &str) -> DomainResult<()>;
    pub async fn resolve_credential(&self, id: &str) -> DomainResult<String>;
    pub async fn test_agent_config(&self, id: &str) -> DomainResult<()>;
}
```

`save` validates non-empty `label`/`command`/`credential_env_var`/`vault_secret_id`
and that `vault_connection_id` resolves via `secret_manager.list()`, but does
**not** call out to RocketVault to confirm the secret itself still exists
(stale references are discovered on use, per the spec). `resolve_credential`
maps a `None` fetcher result (stale/deleted vault secret) to
`DomainError::NotFound`. `test_agent_config` resolves `command` via the
`which` crate (no process spawned) and calls `resolve_credential`.

### `src-tauri` (new, Plan 04)

```rust
// src-tauri/src/commands/agent_configs.rs
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
// list_agent_configs, save_agent_config, delete_agent_config, test_agent_config
```

### Frontend (new, Plan 05)

```typescript
// src/lib/tauri-api.ts
export interface AgentConfig {
  id: string;
  label: string;
  command: string;
  args: string[];
  workingDir?: string;
  credentialEnvVar: string;
  vaultConnectionId: string;
  vaultName: string;
  vaultSecretId: string;
  vaultSecretName: string;
}
// listAgentConfigs, saveAgentConfig, deleteAgentConfig, testAgentConfig
```

## Execution note for whoever runs these plans

Run the plans in numeric order — each one's Global Constraints section
repeats the interfaces it consumes from earlier plans so it's runnable by a
fresh Claude Code session that has only read that one file, but the actual
code those interfaces reference won't exist yet if an earlier plan was
skipped. Use `superpowers:subagent-driven-development` or
`superpowers:executing-plans` per plan, per each file's own header. After each
plan's tasks are done, run that plan's Post-Implementation Review step before
starting the next plan.
