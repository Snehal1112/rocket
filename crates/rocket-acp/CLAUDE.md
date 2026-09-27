# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

The `rocket-acp` crate is a pure domain crate in the Rocket HTTP client
workspace. It owns the `AgentConfig` entity (a registered ACP agent
binary/command and a reference to where RocketVault holds its API key) and
the `AgentConfigRepository` trait. It has no I/O — the filesystem
implementation lives in `rocket-infra` (`FsAgentConfigRepo`).

## Commands

```bash
# Check this crate
cargo check -p rocket-acp -j4

# Run all tests in this crate
cargo test -p rocket-acp -j4
```

## Architecture

### Module Map

| Module | Responsibility |
|---|---|
| `agent_config.rs` | `AgentConfig` struct + `AgentConfigRepository` trait |

### Key Design Points

- `AgentConfig` never holds a credential *value* — only
  `vault_connection_id`/`vault_name`/`vault_secret_id`, a reference resolved
  on demand by `rocket-app`'s `AgentConfigService` through the existing
  RocketVault `SecretManagerService`/`VaultSecretFetcher` machinery.
- No cross-domain-crate dependencies — other entities are referenced by plain
  `String` id, not by importing another domain crate's types.
- Plain (non-camelCase) field names — this struct persists to its own
  app-level `agent_configs.yml`, not the OpenCollection format.

### Dependencies

- `rocket-shared` — `DomainResult`
- `serde` — serialization derives
- `serde_json` (dev-only) — serde roundtrip tests
