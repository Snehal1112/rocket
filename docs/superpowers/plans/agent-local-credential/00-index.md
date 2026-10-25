# Agent Local Credential: Plan Index

**Spec:** [../../specs/2026-10-11-agent-local-credential-design.md](../../specs/2026-10-11-agent-local-credential-design.md) (roadmap R-18).

**Context:** builds on the workspace AI assistant, which is on local `main`. Work on a branch cut from `main`, never on `main` itself. Never push; merging is the user's decision.

## Plans

| # | Plan | Areas | Depends on |
|---|---|---|---|
| 01 | [Backend](2026-10-11-agent-local-credential-plan-01-backend.md): `CredentialSource` model and DTO, `AgentConfigService` (validate, resolve per source, keychain set/clear/has, cleanup), keychain namespace, three IPC commands, wiring | `rocket-acp`, `rocket-app`, `rocket-infra`, `src-tauri` | — |
| 02 | [Dialog](2026-10-11-agent-local-credential-plan-02-dialog.md): TS types and wrappers, source choice with env variable field, keychain field flow, History notice, docs | `src/lib`, `src/components/settings`, `src/components/assistant` | 01 |

Build in order 01, 02, one plan at a time. The next plan starts without asking once the current one is complete.

## Model hints (one line per task)

| Task | Implementer | Reviewer |
|---|---|---|
| P01 T1 Model, validation and DTO | sonnet | sonnet |
| P01 T2 `AgentConfigService` resolve, validate, set/clear/has, cleanup | sonnet | **opus** (credential reads and writes, error text, fail-closed keychain) |
| P01 T3 Keychain namespace, IPC commands, wiring | sonnet | sonnet |
| P02 T1 TS types and wrappers | sonnet | haiku |
| P02 T2 Source choice, env variable field, gating, save payload | sonnet | sonnet |
| P02 T3 Keychain field flow, notice, docs | sonnet | sonnet |

The final whole-branch review after Plan 02 runs on opus.

## Execution notes

- **Ledger:** one per plan at `.superpowers/sdd/2026-10-11-agent-local-credential-plan-0N-*/progress.md`. "Task N: complete" means done. Log decisions as `Ruling:` lines. After `/clear` or compaction, use the `resume-plan` skill.
- **One implementer per task, one at a time per worktree.** Reviewers read a diff file, not the live tree. Re-reviews cover only the fixes since the last review.
- **Cross-plan compile rule:** `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check` are green at the end of every task.
- **Tests:** each task writes its tests and runs only the focused commands named in it, plus the compile and lint checks. The user runs the full suites.
- **Cargo:** always `-j4`; never `cargo test --workspace` or `--all`.
- **Commits:** every commit through the `dev-workflow-skills:1-git-commit` skill, conventional format, explicit `git add <paths>` and `git commit --only -- <paths>`; never `git add -A`, `--all` or `.`; no bare `git stash`. Dispatched implementers get this rule in their prompt.
- **OpenCollection:** `agent_configs.yml` is an app-level file in `~/.rocket-api`, not part of any collection, so the OpenCollection reference read step does not apply to these plans.
- **Security rule for every task:** no credential value in an error message, a log line, an event, a `Debug` output, a test name or any value returned to the UI. Tests that check this use a recognizable fake value and assert it is absent.
- **Docs:** tasks update `crates/rocket-acp/CLAUDE.md`, `crates/rocket-app/CLAUDE.md`, `docs/features/assistant-followups.md`. Tasks do not edit the roadmap.

## Rulings (controller, 2026-10-11)

- Ruling: the three IPC commands are `async` and call the service directly, like `test_agent_config`, not through `spawn_blocking`, because the services are managed as plain `AgentConfigService` state and not as `Arc`. The keychain call is short — cost if wrong: one blocked runtime thread for a keychain prompt; wrapping them later needs the managed state to become an `Arc`.
- Ruling: `AgentConfig` is built in about ten places (tests in four crates and the DTO). Plan 01 T1 adds the two fields to every literal explicitly (`credential_source: CredentialSource::Vault`, `source_env_var: None`). `AgentConfig` does not get a `Default` derive.
- Ruling: `AgentConfigService::new` gains two parameters (`local_secrets: Arc<dyn SecretStore>`, `env_reader: EnvReader`). Both service instances in `src-tauri/src/lib.rs` share one keychain store and `process_env_reader()`.
- Ruling: the TS field is `credentialSource?: 'vault' | 'env' | 'keychain'` (optional; a missing value means `vault`), like `extraEnv`, so existing TS fixtures keep compiling.
- Ruling: the dialog form is no longer gated on RocketVault connections. Only the vault block shows the "no connections" notice, so a user with no vault can configure an `env` or `keychain` agent.
- Ruling: the save order for a keychain config is config first, then `set_agent_credential`. A failed second call leaves the config without a credential and shows an error; Test reports the missing credential.

## Locked contracts

```rust
// rocket_acp
pub enum CredentialSource { Vault, Env, Keychain }   // serde lowercase, Default = Vault, Copy
pub struct AgentConfig { /* existing */ pub credential_source: CredentialSource, pub source_env_var: Option<String> }
pub fn validate_credential_source(config: &AgentConfig) -> Result<(), String>   // never contains a value

// rocket_app
pub type EnvReader = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
pub fn process_env_reader() -> EnvReader;
AgentConfigService::new(repo, secret_manager, local_secrets: Arc<dyn SecretStore>, env_reader: EnvReader)
AgentConfigService::set_local_credential(&self, config_id: &str, value: &str) -> DomainResult<()>
AgentConfigService::clear_local_credential(&self, config_id: &str) -> DomainResult<()>
AgentConfigService::has_local_credential(&self, config_id: &str) -> DomainResult<bool>
// keychain: SecretStore scope "agent-credential", key = config id, service "com.rocketapi.agent-credentials"

// rocket_infra
KeyringSecretStore::new_agent_credentials()
```

```text
// IPC (camelCase args): set_agent_credential(configId, value), clear_agent_credential(configId), has_agent_credential(configId) -> bool
// AgentConfigDto adds: credentialSource ('vault'|'env'|'keychain', default 'vault'), sourceEnvVar? (omitted when unset)
```
