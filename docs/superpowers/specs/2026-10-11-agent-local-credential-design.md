# Agent local credential: design

Date: 2026-10-11. Status: design approved by the user in chat, spec ready for review. Roadmap row R-18.
Read first: `crates/rocket-acp/CLAUDE.md`, `.claude/assistant-followups.md`, `docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md`.

## 1. Summary

An ACP agent session needs an API key or token. Today the only source is a RocketVault secret that the agent config points at. RocketVault is read-only from Rocket's side (a deliberate decision), so a user with no vault, or with a token they cannot store there, cannot start a session. That led to a real OAuth token being pasted into a plain config field.

This design adds two local credential sources next to the vault one:
- a variable in Rocket's own process environment, and
- an entry in the OS keychain that the user saves from the agent dialog.

## 2. Decisions made with the user

- Accepted sources (2026-10-11): Rocket's own environment variable, and an OS keychain entry made in Rocket.
- Refused: a token file path, and the agent's own CLI login. The CLI login would need the session to drop the empty `CLAUDE_CONFIG_DIR`, so the agent would load the user's whole Claude Code setup. Isolation stays as it is.
- One explicit source per agent config. There is no fallback chain. A failure is reported as it is and Rocket never switches silently to another source.

## 3. Goals and non-goals

Goals:
1. A config can name its credential source: `vault`, `env` or `keychain`.
2. A credential value is never written to `agent_configs.yml`, a log, an event, History, or any value returned to the UI.
3. Old config files keep working unchanged.

Non-goals:
- A token file source, the agent's CLI login, a fallback chain.
- Scrubbing these values from stored conversations (same limit as vault values today; see section 9).
- Writing to RocketVault.

OpenCollection: not affected. `agent_configs.yml` is an app-level file, not part of any collection.

## 4. Data model

`rocket-acp` `AgentConfig` (persistence struct, snake_case, no `rename_all`):

```rust
pub enum CredentialSource { Vault, Env, Keychain }   // serde lowercase, default Vault

pub struct AgentConfig {
    // existing fields unchanged ...
    #[serde(default)]
    pub credential_source: CredentialSource,
    /// For `env`: the name of the variable Rocket reads from its own environment.
    #[serde(default)]
    pub source_env_var: Option<String>,
}
```

- `credential_env_var` (the name the agent receives) is unchanged and always required.
- `vault_connection_id`, `vault_name`, `vault_secret_id` and `vault_secret_name` stay as they are. They must be non-empty only when the source is `vault`; for the other sources they may be empty.
- `source_env_var` must be set when the source is `env` and must be absent or empty otherwise. It is checked with the existing `validate_env_var_name`, so token-looking text is refused. A source name equal to `CLAUDE_CONFIG_DIR` is refused by the same function.
- The keychain entry is keyed by the config id, so no field is needed.
- `validate_config` in `AgentConfigService` becomes source-aware. The error text never contains a value.

## 5. Resolving the credential

`AgentConfigService::resolve_credential_for` branches on the source and returns the same `String` as today, so `session_env`, the failed-start stderr tail masking and the events are unchanged.

- `vault`: as today (`SecretManagerService::resolve_secret_value`).
- `env`: reads `source_env_var` from Rocket's process environment through an injected reader (`Arc<dyn Fn(&str) -> Option<String> + Send + Sync>`, so `rocket-app` stays free of direct process state and tests need no real environment). An unset or empty value is `NotFound` with a message naming the variable, for example "agent 'x': environment variable 'Y' is not set in Rocket's environment". The value is trimmed.
- `keychain`: `SecretStore::get_strict("agent-credential", config_id)`. A locked or unavailable keychain is an error, not "not set". A missing entry is `NotFound`: "agent 'x': no credential saved in the keychain; open the agent settings and save one".

Nothing in the messages quotes a value. The agent process receives the value only under `credential_env_var`, as today.

## 6. Writing and clearing the keychain entry

- `rocket-infra` `KeyringSecretStore::new_agent_credentials()` with the keychain service `com.rocketapi.agent-credentials`, distinct from the other namespaces.
- `AgentConfigService` gets an injected `Arc<dyn SecretStore>` and three methods: `set_local_credential(config_id, value)`, `clear_local_credential(config_id)` and `has_local_credential(config_id) -> bool` (uses `get_strict`; a keychain error is an error).
- `set_local_credential` requires the config to exist and its source to be `keychain`. It trims the value, and refuses an empty value, a value with control characters, and a value over 4096 bytes. The error never contains the value.
- `delete` of an agent config also deletes its keychain entry (best effort, logged on failure without the value). Changing a config from `keychain` to another source clears the entry.
- IPC commands in `src-tauri/src/commands/agent_configs.rs`: `set_agent_credential(configId, value)`, `clear_agent_credential(configId)`, `has_agent_credential(configId) -> bool`. No command returns a value. Commands stay thin and async (`spawn_blocking` around keychain calls).

## 7. Frontend

`AgentConfigsDialog`:
- The vault block becomes a source choice (shadcn `RadioGroup`): RocketVault secret, Rocket environment variable, Keychain.
- Vault: the existing connection and secret pickers.
- Environment variable: one name field with the hint "Rocket must be started from a shell that exports it. A desktop-icon launch may not see it."
- Keychain: a password field. When `has_agent_credential` is true it shows "Saved in the keychain" and a blank field keeps the entry; typing replaces it; a "Remove" button clears it. The typed value is sent only through `set_agent_credential` and the field is cleared after the call.
- Save order for a new keychain config: save the config first, then `set_agent_credential`; if the second call fails the dialog stays open with the error and the config exists without a credential (the Test button reports it).
- The Credential Env Var help text is kept. Test works for every source.
- `src/lib/tauri-api.ts`: the `AgentConfig` type gets `credentialSource` and `sourceEnvVar` (camelCase on the IPC DTO), plus three wrappers.

## 8. Errors and edge cases

- Old file without the new fields: loads as `vault`, unchanged.
- `env` with a name Rocket does not have: clear error at session start and in Test, before any process is spawned.
- Keychain unavailable (headless Linux without a secret service): `get_strict` error is surfaced as "the OS keychain is not available"; there is no plaintext fallback.
- A config switched from `vault` to `keychain` keeps its old vault fields in the file (harmless), and they are ignored.
- Two sessions started from the same config read the credential separately at each start.

## 9. Privacy and known limits

- The file `agent_configs.yml` holds only names. The value lives in the keychain or in Rocket's own environment.
- The env source sees only what Rocket's process was started with. The agent already inherits Rocket's environment today (documented in `crates/rocket-acp/CLAUDE.md`), so this adds no new exposure.
- Masking of the failed-start stderr tail is exact-match, as documented. The new sources feed the same string, so they inherit those limits.
- Stored conversations do not scrub these values, like vault values. A token typed into a prompt is stored. The History view notice already states the vault limit; it is extended to say "agent credentials".

## 10. Testing

- `rocket-acp`: `CredentialSource` serde default and lowercase wire names; an old YAML without the fields loads as `vault`; `validate_env_config` unchanged.
- `rocket-app` `agent_config_service`: validation per source (vault fields required only for `vault`; `env` needs a valid `source_env_var`; token-looking name refused); resolve per source with test doubles (env set, unset, empty and trimmed; keychain hit, miss, error); messages never contain the value; set/clear/has rules (config must exist, source must be `keychain`, value rules); delete clears the entry; a source change clears the entry.
- `rocket-infra`: the new keychain service label is distinct (unit test on the constants, as for the other namespaces).
- `src-tauri`: command wiring and DTO casing.
- Vitest: dialog shows the right fields per source, the typed value goes only to `set_agent_credential` and the field clears, "Saved" state, Remove, validation of the env name field.
- Manual: start a session with each source; check `agent_configs.yml` holds no value.

## 11. Plans

Plans hold at most 3 tasks each.

- Plan 1 (backend): `CredentialSource` and validation in `rocket-acp` and `AgentConfigService`; the keychain namespace, resolve per source and the set/clear/has methods with the delete and source-change cleanup; the three IPC commands and wiring.
- Plan 2 (dialog): `tauri-api` types and wrappers; the source choice and fields in `AgentConfigsDialog`; docs (`crates/rocket-acp/CLAUDE.md`, `.claude/assistant-followups.md`, the History notice text).

Opus reviews the task that reads and writes credentials.
