# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

The `rocket-acp` crate is a pure domain crate in the Rocket HTTP client
workspace. It owns the `AgentConfig` entity (a registered ACP agent
binary/command and a reference to where RocketVault holds its API key) and
the `AgentConfigRepository` trait, plus the `AcpSessionClient` trait that
drives one ACP agent session (spawn + handshake, streamed prompt, kill). It
has no I/O — the filesystem implementation lives in `rocket-infra`
(`FsAgentConfigRepo`), and the process/protocol implementation lives in
`rocket-infra` (`AcpAgentClient`).

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
| `session.rs` | `AcpSessionClient` trait (`start_session`, `send_prompt`, `cancel`, `set_config_option`, `end_session`, `end_all_sessions`) |
| `session_info.rs` | `SessionInfo`, `PromptCapabilities`; re-exports `ConfigOption`/`ConfigChoice` from `rocket_shared::acp` |
| `update.rs` | `AcpUpdate` (typed agent updates) and `ToolCallStatus` |
| `prompt.rs` | `PromptPart` (text and embedded text resources) |
| `mcp_server_spec.rs` | `McpServerSpec` |

### Key Design Points

- `AgentConfig` never holds a credential *value* — only
  `vault_connection_id`/`vault_name`/`vault_secret_id`, a reference resolved
  on demand by `rocket-app`'s `AgentConfigService` through the existing
  RocketVault `SecretManagerService`/`VaultSecretFetcher` machinery.
- No cross-domain-crate dependencies — other entities are referenced by plain
  `String` id, not by importing another domain crate's types.
- `AcpSessionClient` must stay object-safe (`rocket-app` holds it as
  `Box<dyn AcpSessionClient>`) and must not depend on `agent-client-protocol`,
  `DomainEvent`, or Tauri. `send_prompt` streams typed `AcpUpdate`s through a plain
  `tokio::sync::mpsc::UnboundedSender<AcpUpdate>` — event publishing belongs in
  `rocket-app`'s `AcpSessionService`, not in this trait. `ConfigOption` is
  defined in `rocket-shared` because `DomainEvent` carries it.
- Plain (non-camelCase) field names — this struct persists to its own
  app-level `agent_configs.yml`, not the OpenCollection format.

### Dependencies

- `rocket-shared` — `DomainResult`
- `serde` — serialization derives
- `async-trait` — async methods on `AcpSessionClient`
- `tokio` — only the `mpsc::UnboundedSender` channel type
- `serde_json` — the `_meta` value of `start_session`
