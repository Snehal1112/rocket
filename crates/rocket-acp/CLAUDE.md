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
| `agent_config.rs` | `AgentConfig` struct + `AgentKind` (Claude, Gemini, Generic; per-kind reserved env names and `validate_kind_env`) + `AgentConfigRepository` trait |
| `session.rs` | `AcpSessionClient` trait (`start_session`, `send_prompt`, `cancel`, `set_config_option`, `end_session`, `end_all_sessions`) |
| `session_info.rs` | `SessionInfo`, `PromptCapabilities`; re-exports `ConfigOption`/`ConfigChoice` from `rocket_shared::acp` |
| `update.rs` | `AcpUpdate` (typed agent updates) and `ToolCallStatus` |
| `prompt.rs` | `PromptPart` (text and embedded text resources) |
| `mcp_server_spec.rs` | `McpServerSpec` |
| `proposal.rs` | `AgentProposal`, `ProposalStatus`, `ProposedChange`, `ProposedRequest`, `RequestPatch`, `FolderSettingsPatch`, `ProposedVariable`, `ScriptPhase`: pure proposal data, applied by `rocket-app`'s `ProposalService` |
| `conversation.rs` | Stored assistant conversations: `ConversationRecord`, `ConversationSummary`, `StoredMessage`, caps (`MAX_CONVERSATIONS`, `MAX_MESSAGES`, `MAX_MESSAGE_TEXT_BYTES`), `validate_conversation_id` (lowercase UUID) and the `ConversationRepository` trait. Persisted as snake_case YAML by `rocket-infra`; see `docs/features/assistant-conversations.md`. |

### Key Design Points

- `AgentKind` is persisted on `AgentConfig` with `#[serde(default)]` (old files load as Claude). It decides how `rocket-app`'s `session_plan` isolates and instructs a session. It carries no behavior itself.
- `AgentConfig` never holds a credential *value*. It holds `credential_source`
  (vault, keychain or env) and `source_env_var`. The vault source keeps only
  `vault_connection_id`/`vault_name`/`vault_secret_id`, resolved on demand by
  `rocket-app`'s `AgentConfigService` through the existing RocketVault
  machinery. The keychain source reads a value stored under the config id in
  the `com.rocketapi.agent-credentials` keychain service. The env source reads
  the variable named by `source_env_var` from Rocket's own process env.
  Env names are validated.
- The env source sees only what Rocket's process was started with.
- No cross-domain-crate dependencies — other entities are referenced by plain
  `String` id, not by importing another domain crate's types.
- `FolderSettingsPatch` has no auth field and uses `ProposedVariable` because this crate must not depend on `rocket-collection`.
- `AcpSessionClient` must stay object-safe (`rocket-app` holds it as
  `Box<dyn AcpSessionClient>`) and must not depend on `agent-client-protocol`,
  `DomainEvent`, or Tauri. `send_prompt` streams typed `AcpUpdate`s through a plain
  `tokio::sync::mpsc::UnboundedSender<AcpUpdate>` — event publishing belongs in
  `rocket-app`'s `AcpSessionService`, not in this trait. `ConfigOption` is
  defined in `rocket-shared` because `DomainEvent` carries it.
- Plain (non-camelCase) field names — this struct persists to its own
  app-level `agent_configs.yml`, not the OpenCollection format.
- `extra_env` (plain env vars for the agent process) caveats:
  - It is stored in plaintext in `agent_configs.yml`. Keep secrets in the
    vault credential, not here.
  - A variable such as `ANTHROPIC_BASE_URL` can send the credential to another
    host. Names are validated (`validate_env_config`, also at session start),
    but values and names such as `PATH` or `LD_PRELOAD` are not restricted.
  - The agent inherits Rocket's own process environment. An agent that dumps
    its env on stderr can show inherited values unmasked in a failed-start
    message, because only the values Rocket passes are masked.
  - Masking of the failed-start stderr tail (rocket-infra `acp_stderr_tail.rs`)
    is an exact-match replace. It does not cover base64, URL-encoded,
    case-changed, escape-code-split or line-wrapped forms of a secret.
    Overlapping secrets (one value containing part of another) can also leave
    a fragment, especially at the front of an overflowed buffer.
  - Env names compare case-insensitively (Windows semantics).

### Dependencies

- `rocket-shared` — `DomainResult`
- `serde` — serialization derives
- `async-trait` — async methods on `AcpSessionClient`
- `tokio` — only the `mpsc::UnboundedSender` channel type
- `serde_json` — the `_meta` value of `start_session`
