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
| `session_info.rs` | `SessionInfo`, `PromptCapabilities`; `SessionInfo` carries `ConfigOption`/`ConfigChoice` (defined in `options.rs`, re-exported here and at the crate root) |
| `events.rs` | `AcpEvent` (13 session and proposal events plus 3 assistant conversation events; implements `PublishedEvent`, same channels and JSON as the old `DomainEvent` variants). Published by `AcpSessionService`, `ProposalService`, `ConversationService` and `McpToolService` in `rocket-app`. |
| `options.rs` | `ConfigOption`, `ConfigChoice`, `PermissionOption`, `PermissionOptionKind`: option types that `AcpEvent` and `SessionInfo` carry |
| `update.rs` | `AcpUpdate` (typed agent updates) and `ToolCallStatus` |
| `prompt.rs` | `PromptPart` (text and embedded text resources) |
| `mcp_server_spec.rs` | `McpServerSpec` |
| `proposal.rs` | `AgentProposal`, `ProposalStatus`, `ProposedChange`, `ProposedRequest`, `RequestPatch`, `FolderSettingsPatch`, `ProposedVariable`, `ScriptPhase`: pure proposal data, applied by `rocket-app`'s `ProposalService` |
| `proposal_rules.rs` | Pure structural rules for proposals, used by `rocket-app`'s `ProposalService`. Path rules: `Target`, `locate`, `locate_with`, `is_folder`, `is_free`, `normalize_path(s)`, `validate_relative_path`, `validate_item_name`, `check_reserved_name`, `join_path`, `parent_of`, `move_destination`, `paths_of`. Apply rules: `apply_patch`, `build_request`, `set_script`, `stored_body`, `apply_folder_patch`, `check_folder_variables`. Also `folder_shape` (the folder structure string; `ProposalService`'s `folder_fingerprint` imports it) and `sha256_hex`. No I/O. |
| `mode.rs` | `AssistantMode` (`Ask`, `Edit`, `Agent`; ordered `Ask < Edit < Agent`, default `Ask`, serde lowercase) with `label()` and `summary()`. The live session mode. `rocket-app`'s `McpToolService` gates tools by it. |
| `conversation.rs` | Stored assistant conversations: `ConversationRecord`, `ConversationSummary`, `StoredMessage`, caps (`MAX_CONVERSATIONS`, `MAX_MESSAGES`, `MAX_MESSAGE_TEXT_BYTES`, `MAX_INPUT_TEXT_BYTES`), the input bound helpers `first_kept_index` and `text_bytes`, `validate_conversation_id` (lowercase UUID) and the `ConversationRepository` trait. Persisted as snake_case YAML by `rocket-infra`; see `docs/features/assistant-conversations.md`. `ConversationRecord.mode` is an `AssistantMode` from `mode.rs` (there is no separate conversation mode type). |

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
- The only domain crate dependency besides `rocket-shared` is `rocket-collection`
  (added for `proposal_rules.rs`, which works on `Folder`, `Request` and
  `FolderSettings`). `rocket-collection` must not depend on `rocket-acp`. Other
  entities are still referenced by plain `String` id.
- `FolderSettingsPatch` has no auth field and uses `ProposedVariable`, so `proposal.rs` itself does not import `rocket-collection` types.
- `AcpSessionClient` must stay object-safe (`rocket-app` holds it as
  `Box<dyn AcpSessionClient>`) and must not depend on `agent-client-protocol`,
  the event enums, or Tauri. `send_prompt` streams typed `AcpUpdate`s through a plain
  `tokio::sync::mpsc::UnboundedSender<AcpUpdate>` — event publishing belongs in
  `rocket-app`'s `AcpSessionService`, not in this trait. `ConfigOption` is defined in this crate (`options.rs`) because `AcpEvent` carries it.
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
- `rocket-collection` — `Folder`, `Request` and `FolderSettings`, for `proposal_rules.rs`
- `sha2` — `sha256_hex` in `proposal_rules.rs`
- `serde` — serialization derives
- `async-trait` — async methods on `AcpSessionClient`
- `tokio` — only the `mpsc::UnboundedSender` channel type
- `serde_json` — the `_meta` value of `start_session`
