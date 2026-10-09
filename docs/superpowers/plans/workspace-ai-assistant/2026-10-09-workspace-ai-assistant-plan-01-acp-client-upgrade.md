# Workspace AI Assistant — Plan 01: ACP Client Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Rocket's text-only ACP pipe with typed agent updates, captured session info (config options and prompt capabilities), `_meta` pass-through on `session/new`, option changes, Stop, prompt parts with embedded text resources, an idle prompt timeout, and a deny answer to permission requests, plumbed end to end through events, Tauri commands and TypeScript wrappers.

**Architecture:** `rocket-acp` gains protocol-free domain types (`AcpUpdate`, `SessionInfo`, `PromptPart`) and the reshaped `AcpSessionClient` trait. `rocket-infra`'s `AcpAgentClient` maps the real `agent-client-protocol` 2.2 types to them. `rocket-app`'s `AcpSessionService` turns updates into new `DomainEvent`s under a per-update idle timeout, and `src-tauri` exposes the new commands and DTOs that `src/lib/tauri-api.ts` wraps. The existing per-tab `AgentChatPanel` keeps working with a one-line adaptation until Plan 05 removes it.

**Tech Stack:** Rust (`rocket-shared`, `rocket-acp`, `rocket-infra`, `rocket-app`, `src-tauri`), `agent-client-protocol` 2.2.0 with `agent-client-protocol-schema` 1.9.1, `tokio` (`select!`, `mpsc`, `Notify`), `serde`/`serde_json`, Tauri 2, React + TypeScript, Vitest.

**Spec:** [`docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md`](../../specs/2026-10-09-workspace-ai-assistant-design.md), section "ACP client upgrade". Locked contracts: [`00-plan-index.md`](00-plan-index.md), "Plan 01".

## Verified facts

Checked by reading the code on 2026-10-09 (worktree `acp-mcp-tool-server`, HEAD `0a955981`). Line numbers are as of the start of this plan; later tasks shift them.

- `crates/rocket-acp/src/session.rs:20-27` — `start_session(..., mcp_servers: &[McpServerSpec]) -> DomainResult<String>`; `:33-38` — `send_prompt(session_id, prompt: String, chunk_tx: UnboundedSender<String>)`; test doubles `FakeSessionClient` `:62-94` and `FailingClient` `:119-146`.
- `crates/rocket-acp/Cargo.toml:12-13` — `serde_json` is a dev-dependency only.
- `crates/rocket-shared/Cargo.toml` — depends on no workspace crate (also stated in `crates/rocket-shared/CLAUDE.md`). `crates/rocket-shared/src/lib.rs:1-12` lists modules; there is no `acp` module.
- `crates/rocket-shared/src/events.rs:274-276` — `DomainEvent` is `#[serde(tag = "type", rename_all = "camelCase")]` with snake_case fields; ACP variants `:520-554`; `acp_tool_invoked_wire_shape` test `:1864-1875` is the last test in the file.
- `crates/rocket-infra/src/acp_agent_client.rs:11-24` imports; `:48-57` `RunningSession` (`prompt_lock`, `current_chunk_tx`); `:59` `ChunkSlot`; `:72-76` `set_chunk_sender`; `:184-191` `start_session` signature; `:222-223` slot creation; `:231-232` ready channel carries `(String, ConnectionTo<AgentRole>)`; `:247-266` notification handler forwards only `AgentMessageChunk` text; `:270-299` handshake keeps only the `NewSessionResponse`; `:301-308` sends only `response.session_id`; `:372-396` stores the session and returns the id; `:399-437` `send_prompt` holds `prompt_lock` and sends one `ContentBlock::Text`; `:476-477` end of the trait impl; `:479-495` inherent impl with `fail_and_remove`; `:546-582` `select_mcp_servers_for_agent`; `:598-607` `stop_reason_to_wire_string`. The file has no unit-test module.
- `crates/rocket-infra/src/bin/test_acp_agent.rs:1-104` — fixture agent: `initialize`, `session/new` (dumps `mcp_servers` to `MCP_SERVERS_DUMP_PATH`), `session/prompt` with `__CRASH__` and `__HANG__` sentinels read from the first block only.
- `crates/rocket-infra/tests/acp_agent_client.rs` — 22 `start_session(` calls, 10 `let session_id|session_a|session_b = client.start_session(...)...expect(...);` bindings, 9 single-line `.send_prompt(<id>, "<text>".to_string(), <tx>)` calls, two `assert_eq!(rx.recv().await, Some("fixture reply".to_string()));` (`:153`, `:171`). The file is rustfmt-clean today (`rustfmt --edition 2021 --check` exits 0).
- `crates/rocket-app/src/acp_session_service.rs:16-22` struct with `prompt_timeout`; `:25` `DEFAULT_PROMPT_TIMEOUT` 120 s; `:41-73` `new` and `with_prompt_timeout`; `:156-164` client call and `AcpSessionStarted`; `:187-238` `send_prompt` with `tokio::join!` of a drain future and the client future under one total `tokio::time::timeout`; test fakes `FakeSessionClient` `:430-492` and `CapturingSessionClient` `:877-907`; `assert_eq!(session_id, "session-1")` at `:539`, `:782`, `:814`, `:845`; `.send_prompt("session-1", "hi".to_string())` at `:561`, `:692`, `:724`; `with_prompt_timeout` used at `:715`. `with_prompt_timeout` has no caller outside this file.
- `src-tauri/tests/acp_mcp_start_agent_session.rs:155-188` implements `AcpSessionClient`; `:256` and `:295` bind `let session_id = start_agent_session_inner(...)`, asserted at `:268` and `:307`.
- `src-tauri/src/commands/acp_sessions.rs:9-29` `start_agent_session` returns `Result<String, DomainError>`; `:54-112` `start_agent_session_inner` returns the session id; `:114-121` `send_agent_prompt(session_id, prompt)`.
- `src-tauri/src/tauri_event_bus.rs:32-37,49` — ACP channel arms; the match has no wildcard.
- `src-tauri/src/lib.rs:920-922` — the three `acp_sessions` commands in `generate_handler!`. `src-tauri/src/commands/mod.rs:1` — `pub mod acp_sessions;`. `src-tauri/src/commands/folder_settings_dto.rs` is the DTO-module precedent (camelCase, full destructuring `From` impls). The `src-tauri` package is named `rocket`.
- `src/lib/tauri-api.ts:2518-2526` — `startAgentSession` returns `invoke<string>`, `sendAgentPrompt(sessionId, prompt)`, `endAgentSession`; event types and listeners `:2528-2560` and `:2627-2636`, snake_case payloads.
- `src/components/request/AgentChatPanel.tsx:57` — `const newSessionId = await startAgentSession(...)`; `:88` — `await sendAgentPrompt(agentSession.sessionId, text)`.
- `src/components/request/__tests__/AgentChatPanel.test.tsx:98,119,134` — `startAgentSession` mocked with a string; `src/lib/queries/__tests__/agent-session-api.test.ts:9-34` asserts the old return and argument shapes.
- `agent-client-protocol-schema-1.9.1/src/v1`: `NewSessionRequest` has `meta: Option<Meta>` with a `.meta(impl IntoOption<Meta>)` builder (`agent.rs:802-866`, builder at `:862`); `Meta = serde_json::Map<String, serde_json::Value>` (`ext.rs:14`); `NewSessionResponse.config_options: Option<Vec<SessionConfigOption>>` (`agent.rs:878`); `SessionConfigOption` with `kind: SessionConfigKind::{Select, Boolean}`, `category: Option<SessionConfigOptionCategory>` and `SessionConfigSelectOptions::{Ungrouped, Grouped}` (`agent.rs:2120-2410`); `SetSessionConfigOptionRequest::new(session_id, config_id, value)` and `SetSessionConfigOptionResponse.config_options` (`agent.rs:2542-2620`); `CancelNotification::new(session_id)` (`agent.rs:5197`); `PromptCapabilities { image, audio, embedded_context }` with builders (`agent.rs:4465-4530`); `SessionUpdate` variants `ToolCall`, `ToolCallUpdate`, `ConfigOptionUpdate`, `UsageUpdate` are not feature-gated (`client.rs:99-138`); `UsageUpdate { used, size, cost: Option<Cost> }`, `Cost { amount, currency }` (`client.rs:609-695`); `RequestPermissionRequest`, `PermissionOptionKind::{AllowOnce, AllowAlways, RejectOnce, RejectAlways}`, `RequestPermissionOutcome::{Cancelled, Selected}` (`client.rs:968-1200`); `ToolCall`, `ToolCallUpdate { tool_call_id, fields }`, `ToolKind`, `ToolCallStatus::{Pending, InProgress, Completed, Failed}` (`tool_call.rs:29-530`); `EmbeddedResource::new(EmbeddedResourceResource::TextResourceContents(TextResourceContents::new(text, uri).mime_type(..)))` (`content.rs:265-372`). Most of these are `#[non_exhaustive]`, so matches need a wildcard arm.
- `agent-client-protocol-2.2.0`: `Builder::on_receive_request(op, on_receive_request!())` with `op(Req, Responder<Req::Response>, ConnectionTo<_>)` (`src/jsonrpc.rs:1494`); handlers run inside the dispatch loop and block it until they finish (`src/concepts/ordering.rs`); `ConnectionTo::send_notification` (`src/jsonrpc.rs:4205`) and `ConnectionTo::spawn` (`:3709`); `Responder::respond` (`:4668`); `CancelNotification` is `"session/cancel"` (`src/schema/client_to_agent/notifications.rs:3`); `SetSessionConfigOptionRequest` is `"session/set_config_option"` (`src/schema/client_to_agent/requests.rs:49-51`); `RequestPermissionRequest` is an agent-to-client request (`src/schema/agent_to_client/requests.rs:11-12`); `examples/yolo_one_shot_client.rs:55-71` registers a `RequestPermissionRequest` handler on `Client.builder()`.
- `@agentclientprotocol/claude-agent-acp@0.88.0` (`dist/acp-agent.js`): advertises `promptCapabilities { image: true, embeddedContext: true }` (`:1380-1383`); returns `configOptions` from `session/new` (`:1487`); handles `session/cancel` (`:5357`) and `session/set_config_option` (`:5752`, `:8980`), returning `{ configOptions }`; option ids `model` (`session-model.js:2`) and `effort` (`session-config-ids.js:4`); sends `type: "boolean"` options only to clients that advertise `session.configOptions.boolean` (`:8094-8101`), which Rocket does not, so every option Rocket sees is a select.

## Interface deviations

1. **`ConfigOption` and `ConfigChoice` are defined in `rocket-shared`, not in `rocket-acp`.** They live in a new `crates/rocket-shared/src/acp.rs` and `crates/rocket-acp/src/session_info.rs` re-exports them (`pub use rocket_shared::acp::{ConfigChoice, ConfigOption};`), so `rocket_acp::ConfigOption` and `rocket_acp::ConfigChoice` resolve exactly as the index says, with the same field names. Reason: `DomainEvent::AcpConfigOptionsChanged { options: Vec<ConfigOption> }` lives in `rocket-shared`, which may not depend on any workspace crate. Names, fields and the event shape are unchanged.

Additive details that do not change any locked name: `AcpUpdate` derives `Debug, Clone, PartialEq`; `ToolCallStatus` also derives `Debug` and gets `as_str()`; `PromptCapabilities` and `SessionInfo` derive `Debug, Clone, PartialEq, Eq` (and `Copy, Default` for `PromptCapabilities`); `AcpSessionService::with_prompt_timeout` is renamed `with_prompt_idle_timeout` (the index renames the field; the constructor keeps its position); `send_agent_prompt` enforces 8 resources of at most 8192 bytes each (the spec's chip limits) at the IPC boundary.

## Global Constraints

- Always pass `-j4` to every `cargo check` and `cargo test` invocation.
- The agent runs only `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check`. Tests are written in every task; test commands appear as "for the user to run" and the agent runs them only when the user asks.
- Never run `cargo test --workspace` (the harness blocks it).
- `cargo check --workspace --all-targets -j4` and `yarn tsc --noEmit` are green at the end of every task.
- Rust: no unwrap calls in production code. Test code uses `.expect("message")`.
- Serde: `#[serde(rename_all = "camelCase")]` only on the IPC DTOs in `src-tauri/src/commands/acp_session_dto.rs`. `DomainEvent` fields stay snake_case with the camelCase `type` tag. `rocket_shared::acp::ConfigOption` has no rename.
- `rocket-acp` must not depend on `agent-client-protocol`, `DomainEvent` or Tauri. `rocket-shared` must not depend on any workspace crate.
- Prompt idle timeout: 120 seconds (`DEFAULT_PROMPT_IDLE_TIMEOUT`), reset by every `AcpUpdate`.
- Prompt resource limits at the IPC boundary: at most 8 resources, each `text` at most 8192 bytes.
- Event channels: `agent-session-tool-activity`, `agent-session-config-options`, `agent-session-usage`.
- Commits: conventional commits, created through the `dev-workflow-skills:1-git-commit` skill (never a freeform `git commit -m`). Stage explicit paths only (no `git add -A`, `--all` or `.`); peer sessions may share this repository's index.
- Code comments are short, full sentences that end with a punctuation mark.

## Review Focus

- **Agent updates Rocket does not model are dropped, never mis-typed.** Thought chunks, user-message chunks, image chunks inside an agent message, plans, mode and available-command updates must produce no `AcpUpdate` and no error. Test: `non_text_chunks_and_unmodelled_updates_are_dropped` (Task 1, unit test in `acp_agent_client.rs`).
- **Config options in shapes the UI cannot show.** A grouped select must flatten to one choice list, and a boolean option must be skipped (Rocket never advertises boolean support, but an agent may still send one). Tests: `config_options_flatten_groups_and_skip_boolean_options` (Task 1 unit test) and `acp_agent_client_start_session_returns_config_options_and_prompt_capabilities` (Task 1 integration test; the fixture sends a boolean `fast` option).
- **Usage cost in a currency other than USD.** `cost_usd` must be `None`, never a mislabelled amount. Test: `usage_cost_is_kept_only_in_usd` (Task 1 unit test).
- **A tool call update for a call the turn never announced, or with neither title nor status.** It must still publish one `AcpToolActivity` with an empty title and `pending` status instead of panicking or being dropped. Test: `tool_call_update_for_an_unknown_call_publishes_an_empty_title_and_pending` (Task 2).
- **Hostile or empty prompt input at the IPC boundary.** More than 8 resources, a resource over 8192 bytes, or an empty prompt with no resources must fail with `InvalidInput` before reaching the agent; a non-object `meta` must fail before any process is spawned. Tests: `prompt_parts_rejects_too_many_resources`, `prompt_parts_rejects_an_oversized_resource`, `prompt_parts_rejects_an_empty_prompt` (Task 3) and `acp_agent_client_start_session_rejects_meta_that_is_not_an_object` (Task 1).

---

### Task 1: Typed ACP domain types, the reshaped `AcpSessionClient` trait, and the `AcpAgentClient` implementation

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-shared/src/acp.rs`
- Modify: `crates/rocket-shared/src/lib.rs:1` (module list)
- Create: `crates/rocket-acp/src/update.rs`, `crates/rocket-acp/src/session_info.rs`, `crates/rocket-acp/src/prompt.rs`
- Modify: `crates/rocket-acp/src/lib.rs:1-6`, `crates/rocket-acp/Cargo.toml:6-13`, `crates/rocket-acp/src/session.rs:1-155`, `crates/rocket-acp/CLAUDE.md` (module map and design points)
- Modify: `crates/rocket-infra/src/acp_agent_client.rs` (lines 11-24, 48-59, 72-76, 184-191, 222-223, 231-232, 247-308, 372-437, 476-479, after 582, end of file)
- Modify: `crates/rocket-infra/src/bin/test_acp_agent.rs:1-104`
- Modify: `crates/rocket-infra/tests/acp_agent_client.rs` (imports `:12`, every `start_session` and `send_prompt` call, `:153`, `:171`, new tests at the end)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (imports `:4`, `:156-159`, `:188-199`, test imports after `:266`, fakes `:430-492` and `:877-907`, test `:547-584`)
- Modify: `src-tauri/tests/acp_mcp_start_agent_session.rs:16,28,155-188`

**Interfaces:**
- Consumes: `agent-client-protocol` 2.2.0 and its schema 1.9.1 (see Verified facts); existing `McpServerSpec`, `select_mcp_servers_for_agent`, `mcp_server_specs_to_wire`, `stop_reason_to_wire_string`.
- Produces (exact):
  ```rust
  // rocket_shared::acp (re-exported as rocket_acp::{ConfigOption, ConfigChoice})
  pub struct ConfigOption { pub id: String, pub name: String, pub category: Option<String>,
                            pub current_value: String, pub choices: Vec<ConfigChoice> }
  pub struct ConfigChoice { pub value: String, pub name: String, pub description: Option<String> }
  // rocket_acp
  pub enum AcpUpdate {
      Text { text: String },
      ToolCall { call_id: String, title: String, kind: String, status: ToolCallStatus },
      ToolCallUpdate { call_id: String, title: Option<String>, status: Option<ToolCallStatus> },
      ConfigOptions { options: Vec<ConfigOption> },
      Usage { used: u64, size: u64, cost_usd: Option<f64> },
  }
  pub enum ToolCallStatus { Pending, InProgress, Completed, Failed }   // + fn as_str(self) -> &'static str
  pub struct PromptCapabilities { pub embedded_context: bool, pub image: bool }
  pub struct SessionInfo { pub session_id: String, pub config_options: Vec<ConfigOption>,
                           pub prompt_capabilities: PromptCapabilities }
  pub enum PromptPart { Text(String), Resource { uri: String, mime_type: Option<String>, text: String } }
  // AcpSessionClient
  async fn start_session(&self, command: &str, args: &[String], cwd: &str,
      env: &[(String, String)], mcp_servers: &[McpServerSpec],
      meta: Option<serde_json::Value>) -> DomainResult<SessionInfo>;
  async fn send_prompt(&self, session_id: &str, parts: Vec<PromptPart>,
      update_tx: UnboundedSender<AcpUpdate>) -> DomainResult<String>;
  async fn cancel(&self, session_id: &str) -> DomainResult<()>;
  async fn set_config_option(&self, session_id: &str, config_id: &str, value: &str)
      -> DomainResult<Vec<ConfigOption>>;
  async fn end_session(&self, session_id: &str) -> DomainResult<()>;
  async fn end_all_sessions(&self) -> DomainResult<()>;
  ```
  `AcpSessionService`'s public signatures do not change in this task (Task 2 changes them).

- [ ] **Step 1: Verify the crate APIs this task relies on**

Run each command and compare with the expected output. If any line is missing, stop and report it; do not guess another API.

```bash
ACP=$(ls -d ~/.cargo/registry/src/*/agent-client-protocol-2.2.0)
SCH=$(ls -d ~/.cargo/registry/src/*/agent-client-protocol-schema-1.9.1)
sed -n 802,866p $SCH/src/v1/agent.rs | grep -n "pub struct NewSessionRequest\|pub meta: Option<Meta>\|pub fn meta(mut self, meta: impl IntoOption<Meta>)"   # expect all three (builder at file line 862)
grep -n "pub type Meta = serde_json::Map<String, serde_json::Value>;" $SCH/src/v1/ext.rs      # expect line 14
grep -n "pub config_options: Option<Vec<SessionConfigOption>>" $SCH/src/v1/agent.rs           # expect a hit at line 894 (NewSessionResponse)
grep -n "impl_jsonrpc_notification!(CancelNotification, \"session/cancel\")" $ACP/src/schema/client_to_agent/notifications.rs
grep -n "\"session/set_config_option\"" $ACP/src/schema/client_to_agent/requests.rs
grep -n "RequestPermissionRequest," $ACP/src/schema/agent_to_client/requests.rs              # expect the request/response pair
grep -n "pub fn on_receive_request<Req: JsonRpcRequest" $ACP/src/jsonrpc.rs                    # expect line 1494
grep -n "pub fn send_notification<N: JsonRpcNotification>" $ACP/src/jsonrpc.rs                 # expect 3330 and 4205 (4205 is ConnectionTo)
grep -n "on_receive_request(" $ACP/examples/yolo_one_shot_client.rs                            # expect line 55
grep -n "ToolCall(ToolCall)\|ToolCallUpdate(ToolCallUpdate)\|ConfigOptionUpdate(ConfigOptionUpdate)\|UsageUpdate(UsageUpdate)" $SCH/src/v1/client.rs
```

- [ ] **Step 2: Write the failing tests for the new domain types**

Create `crates/rocket-shared/src/acp.rs` with only its test module (the types do not exist yet):

```rust
//! ACP session option values. They live here, not in `rocket-acp`, because
//! `DomainEvent::AcpConfigOptionsChanged` carries them and this crate depends
//! on no other workspace crate. `rocket-acp` re-exports both types.

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_option_serializes_with_snake_case_keys() {
        let option = ConfigOption {
            id: "model".to_string(),
            name: "Model".to_string(),
            category: Some("model".to_string()),
            current_value: "opus".to_string(),
            choices: vec![ConfigChoice {
                value: "opus".to_string(),
                name: "Opus".to_string(),
                description: None,
            }],
        };
        let json = serde_json::to_string(&option).expect("serialize");
        assert_eq!(
            json,
            r#"{"id":"model","name":"Model","category":"model","current_value":"opus","choices":[{"value":"opus","name":"Opus","description":null}]}"#
        );
        let back: ConfigOption = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, option);
    }
}
```

In `crates/rocket-shared/src/lib.rs`, add the module as the first line (alphabetical, before `pub mod action;`):

```rust
pub mod acp;
pub mod action;
```

Create `crates/rocket-acp/src/update.rs` with only its tests:

```rust
use crate::session_info::ConfigOption;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_status_has_snake_case_wire_names() {
        assert_eq!(ToolCallStatus::Pending.as_str(), "pending");
        assert_eq!(ToolCallStatus::InProgress.as_str(), "in_progress");
        assert_eq!(ToolCallStatus::Completed.as_str(), "completed");
        assert_eq!(ToolCallStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn updates_compare_by_value() {
        let options: Vec<ConfigOption> = Vec::new();
        assert_eq!(
            AcpUpdate::ConfigOptions { options: options.clone() },
            AcpUpdate::ConfigOptions { options }
        );
        assert_ne!(
            AcpUpdate::Text { text: "a".to_string() },
            AcpUpdate::Text { text: "b".to_string() }
        );
    }
}
```

Create `crates/rocket-acp/src/session_info.rs` with only its tests:

```rust
pub use rocket_shared::acp::{ConfigChoice, ConfigOption};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_capabilities_default_to_nothing_supported() {
        let caps = PromptCapabilities::default();
        assert!(!caps.embedded_context);
        assert!(!caps.image);
    }

    #[test]
    fn session_info_holds_the_reported_options() {
        let info = SessionInfo {
            session_id: "s-1".to_string(),
            config_options: vec![ConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                category: None,
                current_value: "default".to_string(),
                choices: Vec::new(),
            }],
            prompt_capabilities: PromptCapabilities {
                embedded_context: true,
                image: false,
            },
        };
        assert_eq!(info.clone(), info);
        assert_eq!(info.config_options[0].id, "model");
    }
}
```

Create `crates/rocket-acp/src/prompt.rs` with only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_parts_compare_by_value() {
        let part = PromptPart::Resource {
            uri: "rocket://request/a".to_string(),
            mime_type: Some("text/plain".to_string()),
            text: "GET /a".to_string(),
        };
        assert_eq!(part.clone(), part);
        assert_ne!(PromptPart::Text("a".to_string()), part);
    }
}
```

Replace `crates/rocket-acp/src/lib.rs` with:

```rust
pub mod agent_config;
pub mod mcp_server_spec;
pub mod prompt;
pub mod session;
pub mod session_info;
pub mod update;
pub use agent_config::{AgentConfig, AgentConfigRepository};
pub use mcp_server_spec::McpServerSpec;
pub use prompt::PromptPart;
pub use session::AcpSessionClient;
pub use session_info::{ConfigChoice, ConfigOption, PromptCapabilities, SessionInfo};
pub use update::{AcpUpdate, ToolCallStatus};
```

- [ ] **Step 3: Check that the new tests fail to compile**

Run: `cargo check --workspace --all-targets -j4`
Expected: FAIL — `cannot find type ConfigOption`, `cannot find type AcpUpdate`, `cannot find type PromptPart`, `cannot find type SessionInfo` and similar.

For the user to run later: `cargo test -p rocket-shared -j4 acp` and `cargo test -p rocket-acp -j4`.

- [ ] **Step 4: Implement the domain types**

In `crates/rocket-shared/src/acp.rs`, add above the test module:

```rust
/// One session setting the agent reports, such as the model or the effort
/// level. Only select-style options are represented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigOption {
    /// The agent's option id, for example `model` or `effort`.
    pub id: String,
    pub name: String,
    /// The agent's category, for example `model` or `thought_level`.
    pub category: Option<String>,
    pub current_value: String,
    pub choices: Vec<ConfigChoice>,
}

/// One value a `ConfigOption` can take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigChoice {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}
```

In `crates/rocket-acp/src/update.rs`, add between the `use` line and the test module:

```rust
/// One typed update from the agent during a prompt turn. It carries no
/// `agent-client-protocol` types, so this crate stays protocol-free.
#[derive(Debug, Clone, PartialEq)]
pub enum AcpUpdate {
    /// A chunk of the agent's reply text.
    Text { text: String },
    /// The agent started a tool call. `kind` is the ACP tool kind in
    /// snake_case, for example `read`, `execute` or `other`.
    ToolCall {
        call_id: String,
        title: String,
        kind: String,
        status: ToolCallStatus,
    },
    /// A change to an earlier tool call. A field the agent left out is `None`.
    ToolCallUpdate {
        call_id: String,
        title: Option<String>,
        status: Option<ToolCallStatus>,
    },
    /// The agent's full, current list of session options.
    ConfigOptions { options: Vec<ConfigOption> },
    /// Context window use, and the session cost when the agent reports it in US dollars.
    Usage {
        used: u64,
        size: u64,
        cost_usd: Option<f64>,
    },
}

/// Progress of one tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl ToolCallStatus {
    /// The snake_case name used on events and in the frontend.
    pub fn as_str(self) -> &'static str {
        match self {
            ToolCallStatus::Pending => "pending",
            ToolCallStatus::InProgress => "in_progress",
            ToolCallStatus::Completed => "completed",
            ToolCallStatus::Failed => "failed",
        }
    }
}
```

In `crates/rocket-acp/src/session_info.rs`, add below the `pub use` line:

```rust
/// What the agent accepts inside a prompt, from its `initialize` answer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PromptCapabilities {
    /// The agent accepts embedded text resources.
    pub embedded_context: bool,
    pub image: bool,
}

/// Everything Rocket keeps from the `initialize` and `session/new` handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// The ACP session id, used as-is for every later call.
    pub session_id: String,
    /// Select-style options such as the model and the effort level.
    pub config_options: Vec<ConfigOption>,
    pub prompt_capabilities: PromptCapabilities,
}
```

In `crates/rocket-acp/src/prompt.rs`, add above the test module:

```rust
/// One part of a prompt. Images and audio are not used in v1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptPart {
    Text(String),
    /// A text resource Rocket generated from its own data, such as a request
    /// definition. It is sent as an embedded resource when the agent
    /// supports that, and as plain text otherwise.
    Resource {
        uri: String,
        mime_type: Option<String>,
        text: String,
    },
}
```

In `crates/rocket-acp/Cargo.toml`, move `serde_json` from `[dev-dependencies]` to `[dependencies]` (the trait now takes a `serde_json::Value`):

```toml
[dependencies]
async-trait.workspace = true
rocket-shared.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
```

and delete the now-empty `[dev-dependencies]` section (lines 12-13).

- [ ] **Step 5: Reshape the `AcpSessionClient` trait and its own test doubles**

Replace the whole of `crates/rocket-acp/src/session.rs` with:

```rust
use crate::{AcpUpdate, ConfigOption, McpServerSpec, PromptPart, SessionInfo};
use rocket_shared::error::DomainResult;
use tokio::sync::mpsc::UnboundedSender;

/// Protocol-focused contract for driving one ACP agent session: spawning the
/// process and handshaking, sending a prompt and streaming typed updates,
/// changing session options, stopping a turn, and ending the session. Has no
/// knowledge of `DomainEvent`, Tauri, or the `agent-client-protocol` crate.
/// Those live in `rocket-infra`'s `AcpAgentClient` and `rocket-app`'s
/// `AcpSessionService`.
#[async_trait::async_trait]
pub trait AcpSessionClient: Send + Sync {
    /// Spawns the agent process and performs the `initialize` → `session/new`
    /// handshake. Returns the ACP session id with the option list and prompt
    /// capabilities the agent reported. `mcp_servers` is passed through to
    /// `session/new` (an empty slice means chat-only). `meta`, when given,
    /// must be a JSON object and is sent as the `_meta` of `session/new`.
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
        meta: Option<serde_json::Value>,
    ) -> DomainResult<SessionInfo>;

    /// Sends `session/prompt` with the given parts. Every update the agent
    /// sends during the turn is forwarded through `update_tx` while this call
    /// is pending. Resolves with the raw `stopReason` string; a stopped turn
    /// resolves with `cancelled`.
    async fn send_prompt(
        &self,
        session_id: &str,
        parts: Vec<PromptPart>,
        update_tx: UnboundedSender<AcpUpdate>,
    ) -> DomainResult<String>;

    /// Sends `session/cancel`. It never waits for the running turn, so it
    /// must not take the per-session prompt lock. The session stays open.
    async fn cancel(&self, session_id: &str) -> DomainResult<()>;

    /// Sends `session/set_config_option` and returns the agent's new option
    /// list. Changing the model can add or remove other options.
    async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: &str,
    ) -> DomainResult<Vec<ConfigOption>>;

    /// Ends the session by explicitly killing the agent process — dropping a
    /// connection handle does not kill the underlying child process by
    /// default in either `std` or `tokio`, so this must be an active kill,
    /// not passive cleanup. No ACP-level shutdown handshake exists.
    async fn end_session(&self, session_id: &str) -> DomainResult<()>;

    /// Kills every currently-tracked session's process, for use when the
    /// whole application is shutting down (Tauri does not drop managed
    /// state on exit, so nothing else calls `end_session` for sessions
    /// still open at quit time). Best-effort: an individual session's kill
    /// failure must not prevent cleanup of the rest. It must also cover a
    /// session still in its handshake, and later `start_session` calls may be
    /// refused, since nothing would kill a session stored after the sweep.
    async fn end_all_sessions(&self) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PromptCapabilities;
    use rocket_shared::error::{DomainError, DomainResult};
    use tokio::sync::mpsc;

    struct FakeSessionClient;

    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            _mcp_servers: &[McpServerSpec],
            _meta: Option<serde_json::Value>,
        ) -> DomainResult<SessionInfo> {
            Ok(SessionInfo {
                session_id: "session-1".to_string(),
                config_options: Vec::new(),
                prompt_capabilities: PromptCapabilities::default(),
            })
        }

        async fn send_prompt(
            &self,
            _session_id: &str,
            _parts: Vec<PromptPart>,
            update_tx: mpsc::UnboundedSender<AcpUpdate>,
        ) -> DomainResult<String> {
            let _ = update_tx.send(AcpUpdate::Text {
                text: "hello".to_string(),
            });
            Ok("end_turn".to_string())
        }

        async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
            Ok(())
        }

        async fn set_config_option(
            &self,
            _session_id: &str,
            _config_id: &str,
            _value: &str,
        ) -> DomainResult<Vec<ConfigOption>> {
            Ok(Vec::new())
        }

        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            Ok(())
        }

        async fn end_all_sessions(&self) -> DomainResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn trait_is_object_safe_and_callable_through_a_trait_object() {
        let client: Box<dyn AcpSessionClient> = Box::new(FakeSessionClient);

        let info = client
            .start_session("echo", &[], "/tmp", &[], &[], None)
            .await
            .expect("start_session");
        assert_eq!(info.session_id, "session-1");

        let (tx, mut rx) = mpsc::unbounded_channel();
        let stop_reason = client
            .send_prompt(&info.session_id, vec![PromptPart::Text("hi".to_string())], tx)
            .await
            .expect("send_prompt");
        assert_eq!(stop_reason, "end_turn");
        assert_eq!(
            rx.recv().await,
            Some(AcpUpdate::Text {
                text: "hello".to_string()
            })
        );

        client.cancel(&info.session_id).await.expect("cancel");
        let options = client
            .set_config_option(&info.session_id, "model", "opus")
            .await
            .expect("set_config_option");
        assert!(options.is_empty());
        client.end_session(&info.session_id).await.expect("end_session");
    }

    #[tokio::test]
    async fn start_session_errors_propagate_as_domain_errors() {
        struct FailingClient;
        #[async_trait::async_trait]
        impl AcpSessionClient for FailingClient {
            async fn start_session(
                &self,
                _command: &str,
                _args: &[String],
                _cwd: &str,
                _env: &[(String, String)],
                _mcp_servers: &[McpServerSpec],
                _meta: Option<serde_json::Value>,
            ) -> DomainResult<SessionInfo> {
                Err(DomainError::InvalidInput("command not found".to_string()))
            }
            async fn send_prompt(
                &self,
                _session_id: &str,
                _parts: Vec<PromptPart>,
                _update_tx: mpsc::UnboundedSender<AcpUpdate>,
            ) -> DomainResult<String> {
                unreachable!("not exercised by this test")
            }
            async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
                unreachable!("not exercised by this test")
            }
            async fn set_config_option(
                &self,
                _session_id: &str,
                _config_id: &str,
                _value: &str,
            ) -> DomainResult<Vec<ConfigOption>> {
                unreachable!("not exercised by this test")
            }
            async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
                unreachable!("not exercised by this test")
            }
            async fn end_all_sessions(&self) -> DomainResult<()> {
                unreachable!("not exercised by this test")
            }
        }

        let client: Box<dyn AcpSessionClient> = Box::new(FailingClient);
        let err = client
            .start_session("bad-command", &[], "/tmp", &[], &[], None)
            .await
            .expect_err("must propagate the error");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
```

- [ ] **Step 6: Extend the fixture agent**

Replace the whole of `crates/rocket-infra/src/bin/test_acp_agent.rs` with:

```rust
// Fixture ACP agent used by integration tests in rocket-infra. Speaks real
// ACP over stdio so tests can exercise the client transport against a real
// (if trivial) agent process instead of a fake.
use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ConfigOptionUpdate, ContentBlock, ContentChunk, Cost,
    EmbeddedResourceResource, InitializeRequest, InitializeResponse, McpCapabilities,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind,
    PromptCapabilities, PromptRequest, PromptResponse, RequestPermissionOutcome,
    RequestPermissionRequest, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOption, SessionId, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, StopReason, TextContent,
    ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind, UsageUpdate,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Responder, Result, Stdio};
use tokio::sync::Notify;

/// The fixture's session options. Choosing the `opus` model adds an effort
/// option, like the real adapter does for models that support it. The
/// boolean `fast` option is a test hook: Rocket must skip boolean options.
fn fixture_config_options(model: &str) -> Vec<SessionConfigOption> {
    let mut options = vec![
        SessionConfigOption::select(
            "model",
            "Model",
            model.to_string(),
            vec![
                SessionConfigSelectOption::new("default", "Default"),
                SessionConfigSelectOption::new("opus", "Opus").description("Most capable"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::boolean("fast", "Fast mode", false),
    ];
    if model == "opus" {
        options.push(
            SessionConfigOption::select(
                "effort",
                "Effort",
                "high",
                vec![
                    SessionConfigSelectOption::new("low", "Low"),
                    SessionConfigSelectOption::new("high", "High"),
                ],
            )
            .category(SessionConfigOptionCategory::ThoughtLevel),
        );
    }
    options
}

/// A text chunk of the agent's reply.
fn text_update(text: &str) -> SessionUpdate {
    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
        text,
    ))))
}

/// Describes the prompt's blocks, so tests can see how the client sent them.
fn describe_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text(_) => "text".to_string(),
            ContentBlock::Resource(resource) => match &resource.resource {
                EmbeddedResourceResource::TextResourceContents(contents) => format!(
                    "resource:{}:{}",
                    contents.uri,
                    contents.mime_type.as_deref().unwrap_or("none")
                ),
                _ => "resource:blob".to_string(),
            },
            _ => "other".to_string(),
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set by `session/cancel` and awaited by a `__WAIT_FOR_CANCEL__` prompt.
    // `notify_one` keeps a permit, so a cancel that arrives first is not lost.
    let cancel = Arc::new(Notify::new());
    let cancel_for_prompt = Arc::clone(&cancel);

    Agent
        .builder()
        .on_receive_request(
            async move |req: InitializeRequest, responder, _conn: ConnectionTo<Client>| {
                // Test hook: lets tests prove `AcpAgentClient::start_session`
                // reads `InitializeResponse.agent_capabilities.mcp_capabilities.http`
                // correctly whether it is `false` (the default here) or `true`.
                let mcp_capabilities =
                    if std::env::var("FIXTURE_ADVERTISE_MCP_HTTP").as_deref() == Ok("1") {
                        McpCapabilities::new().http(true)
                    } else {
                        McpCapabilities::new()
                    };
                // Test hook: embedded context is advertised unless this is set.
                let embedded_context =
                    std::env::var("FIXTURE_NO_EMBEDDED_CONTEXT").as_deref() != Ok("1");
                responder.respond(
                    InitializeResponse::new(req.protocol_version).agent_capabilities(
                        AgentCapabilities::new()
                            .mcp_capabilities(mcp_capabilities)
                            .prompt_capabilities(
                                PromptCapabilities::new()
                                    .image(true)
                                    .embedded_context(embedded_context),
                            ),
                    ),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: NewSessionRequest,
                        responder: Responder<NewSessionResponse>,
                        _conn: ConnectionTo<Client>| {
                // Test hook: dumps the `mcp_servers` this session request
                // carried, so integration tests can assert on
                // `AcpAgentClient`'s `McpServerSpec` -> `McpServer` mapping
                // without implementing an MCP client themselves.
                if let Ok(path) = std::env::var("MCP_SERVERS_DUMP_PATH") {
                    let dump = serde_json::to_string(&req.mcp_servers)
                        .unwrap_or_else(|e| format!("<serialize error: {e}>"));
                    let _ = std::fs::write(path, dump);
                }
                // Test hook: dumps the `_meta` this session request carried.
                if let Ok(path) = std::env::var("SESSION_META_DUMP_PATH") {
                    let dump = serde_json::to_string(&req.meta)
                        .unwrap_or_else(|e| format!("<serialize error: {e}>"));
                    let _ = std::fs::write(path, dump);
                }
                responder.respond(
                    NewSessionResponse::new(SessionId::new("fixture-session"))
                        .config_options(fixture_config_options("default")),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: SetSessionConfigOptionRequest,
                        responder: Responder<SetSessionConfigOptionResponse>,
                        _conn: ConnectionTo<Client>| {
                let model = req
                    .value
                    .as_value_id()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                responder.respond(SetSessionConfigOptionResponse::new(fixture_config_options(
                    &model,
                )))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            move |_notification: CancelNotification, _conn: ConnectionTo<Client>| {
                let cancel = Arc::clone(&cancel);
                async move {
                    cancel.notify_one();
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            move |req: PromptRequest,
                  responder: Responder<PromptResponse>,
                  conn: ConnectionTo<Client>| {
                let cancel = Arc::clone(&cancel_for_prompt);
                async move {
                    // Sentinels may sit in any text block, because resources
                    // come before the prompt text.
                    let texts: Vec<String> = req
                        .prompt
                        .iter()
                        .filter_map(|block| match block {
                            ContentBlock::Text(text) => Some(text.text.clone()),
                            _ => None,
                        })
                        .collect();
                    let has = |sentinel: &str| texts.iter().any(|t| t.as_str() == sentinel);
                    // Test hook: an abrupt, uncooperative exit (no response,
                    // the connection just drops) for the crash-handling path.
                    if has("__CRASH__") {
                        std::process::exit(1);
                    }
                    // Test hook: never answer, so tests can end a prompt
                    // that is still in flight.
                    if has("__HANG__") {
                        std::future::pending::<()>().await;
                    }
                    let session_id = req.session_id.clone();
                    // Test hook: answer `cancelled` once `session/cancel`
                    // arrives. The answer comes from a spawned task, so the
                    // dispatch loop stays free to receive the cancel.
                    if has("__WAIT_FOR_CANCEL__") {
                        return conn.spawn(async move {
                            cancel.notified().await;
                            responder.respond(PromptResponse::new(StopReason::Cancelled))
                        });
                    }
                    // Test hook: ask the client for a permission and report
                    // its answer as reply text. Spawned, because waiting for
                    // the answer inside a handler would block the loop.
                    if has("__PERMISSION__") {
                        let task_conn = conn.clone();
                        return conn.spawn(async move {
                            let response = task_conn
                                .send_request(RequestPermissionRequest::new(
                                    session_id.clone(),
                                    ToolCallUpdate::new(
                                        "call-p",
                                        ToolCallUpdateFields::new()
                                            .title("Run a command".to_string()),
                                    ),
                                    vec![
                                        PermissionOption::new(
                                            "allow-once",
                                            "Allow",
                                            PermissionOptionKind::AllowOnce,
                                        ),
                                        PermissionOption::new(
                                            "reject-once",
                                            "Reject",
                                            PermissionOptionKind::RejectOnce,
                                        ),
                                    ],
                                ))
                                .block_task()
                                .await?;
                            let outcome = match response.outcome {
                                RequestPermissionOutcome::Selected(selected) => {
                                    format!("permission:selected:{}", selected.option_id)
                                }
                                RequestPermissionOutcome::Cancelled => {
                                    "permission:cancelled".to_string()
                                }
                                _ => "permission:other".to_string(),
                            };
                            task_conn.send_notification(SessionNotification::new(
                                session_id,
                                text_update(&outcome),
                            ))?;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        });
                    }
                    // Test hook: one of each typed update before the reply.
                    if has("__TOOLS__") {
                        for update in [
                            SessionUpdate::ToolCall(
                                ToolCall::new("call-1", "Read file")
                                    .kind(ToolKind::Read)
                                    .status(ToolCallStatus::Pending),
                            ),
                            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                                "call-1",
                                ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
                            )),
                            SessionUpdate::UsageUpdate(
                                UsageUpdate::new(53_000, 200_000).cost(Cost::new(0.045, "USD")),
                            ),
                            SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(
                                fixture_config_options("opus"),
                            )),
                        ] {
                            conn.send_notification(SessionNotification::new(
                                session_id.clone(),
                                update,
                            ))?;
                        }
                    }
                    // Test hook: reply with a description of the prompt blocks.
                    let reply = if has("__DESCRIBE__") {
                        describe_blocks(&req.prompt)
                    } else {
                        "fixture reply".to_string()
                    };
                    conn.send_notification(SessionNotification::new(
                        session_id,
                        text_update(&reply),
                    ))?;
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(Stdio::new())
        .await
}
```

- [ ] **Step 7: Write the failing `AcpAgentClient` tests**

Update the existing integration tests in `crates/rocket-infra/tests/acp_agent_client.rs` mechanically. First change the import at line 12 to:

```rust
use rocket_acp::{AcpSessionClient, AcpUpdate, McpServerSpec, PromptPart, ToolCallStatus};
```

Then run these four rewrites from the worktree root (dry-run verified on a copy: 22 `start_session` calls gain `None`, 10 bindings gain `.session_id`, 9 `send_prompt` calls take a part list, and 2 receive assertions expect `AcpUpdate::Text`), then reformat the file (it is rustfmt-clean today, so only the edited lines change):

```bash
F=crates/rocket-infra/tests/acp_agent_client.rs
# 1. Every start_session call gets a trailing `None` meta argument.
perl -0pi -e 's/\.start_session(\(((?:[^()]++|(?1))*)\))/my $a=$2; $a =~ s{,\s*\z}{}; ".start_session($a, None)"/ge' $F
# 2. Every binding of a successful start_session keeps only the session id.
perl -0pi -e 's/(let (?:session_id|session_a|session_b) = client\s*\.start_session\((?:[^;])*?\.expect\((?:[^;])*?\));/$1.session_id;/g' $F
# 3. Every single-line send_prompt sends one text part.
perl -pi -e 's/\.send_prompt\(([^,()]+), ("[^"]*")\.to_string\(\), (\w+)\)/.send_prompt($1, vec![PromptPart::Text($2.to_string())], $3)/g' $F
# 4. The two receive assertions expect a typed text update.
perl -pi -e 's/Some\("fixture reply"\.to_string\(\)\)/Some(AcpUpdate::Text { text: "fixture reply".to_string() })/g' $F
# Count before formatting, because rustfmt splits the multi-line calls.
grep -c ", None)" $F            # expect 22
grep -c "\.session_id;" $F      # expect 10
grep -c "PromptPart::Text(" $F  # expect 9
grep -c "AcpUpdate::Text {" $F  # expect 2
rustfmt --edition 2021 $F
```

Then append these tests at the end of the file:

```rust
// Plan 01 (workspace AI assistant): typed updates, session info, meta,
// option changes, cancel, permission deny and prompt parts.

fn drain_updates(rx: &mut mpsc::UnboundedReceiver<AcpUpdate>) -> Vec<AcpUpdate> {
    let mut updates = Vec::new();
    while let Ok(update) = rx.try_recv() {
        updates.push(update);
    }
    updates
}

#[tokio::test]
async fn acp_agent_client_start_session_returns_config_options_and_prompt_capabilities() {
    let client = AcpAgentClient::new();
    let info = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session");

    assert_eq!(info.session_id, "fixture-session");
    assert!(info.prompt_capabilities.embedded_context);
    assert!(info.prompt_capabilities.image);
    // The fixture also sends a boolean `fast` option, which must be skipped.
    assert_eq!(info.config_options.len(), 1, "got {:?}", info.config_options);
    let model = &info.config_options[0];
    assert_eq!(model.id, "model");
    assert_eq!(model.category.as_deref(), Some("model"));
    assert_eq!(model.current_value, "default");
    let values: Vec<&str> = model.choices.iter().map(|c| c.value.as_str()).collect();
    assert_eq!(values, vec!["default", "opus"]);
    assert_eq!(model.choices[1].description.as_deref(), Some("Most capable"));
}

#[tokio::test]
async fn acp_agent_client_start_session_passes_meta_through_to_new_session_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dump_path = dir.path().join("meta.json");
    let meta = serde_json::json!({ "claudeCode": { "options": { "tools": [] } } });

    let client = AcpAgentClient::new();
    client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[(
                "SESSION_META_DUMP_PATH".to_string(),
                dump_path.display().to_string(),
            )],
            &[],
            Some(meta.clone()),
        )
        .await
        .expect("start_session");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump meta");
    let dumped: serde_json::Value = serde_json::from_str(&dumped).expect("parse dump");
    assert_eq!(dumped, meta);
}

#[tokio::test]
async fn acp_agent_client_start_session_rejects_meta_that_is_not_an_object() {
    let client = AcpAgentClient::new();
    let err = client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[],
            &[],
            Some(serde_json::json!("not an object")),
        )
        .await
        .expect_err("a non-object meta must be refused");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test]
async fn acp_agent_client_send_prompt_forwards_tool_calls_usage_and_config_options() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = client
        .send_prompt(&session_id, vec![PromptPart::Text("__TOOLS__".to_string())], tx)
        .await
        .expect("send_prompt");
    assert_eq!(stop_reason, "end_turn");

    let updates = drain_updates(&mut rx);
    assert_eq!(updates.len(), 5, "got {updates:?}");
    assert_eq!(
        updates[0],
        AcpUpdate::ToolCall {
            call_id: "call-1".to_string(),
            title: "Read file".to_string(),
            kind: "read".to_string(),
            status: ToolCallStatus::Pending,
        }
    );
    assert_eq!(
        updates[1],
        AcpUpdate::ToolCallUpdate {
            call_id: "call-1".to_string(),
            title: None,
            status: Some(ToolCallStatus::Completed),
        }
    );
    assert_eq!(
        updates[2],
        AcpUpdate::Usage {
            used: 53_000,
            size: 200_000,
            cost_usd: Some(0.045),
        }
    );
    match &updates[3] {
        AcpUpdate::ConfigOptions { options } => {
            let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
            assert_eq!(ids, vec!["model", "effort"]);
        }
        other => panic!("expected ConfigOptions, got {other:?}"),
    }
    assert_eq!(
        updates[4],
        AcpUpdate::Text {
            text: "fixture reply".to_string()
        }
    );
}

#[tokio::test]
async fn acp_agent_client_cancel_ends_the_turn_as_cancelled_and_keeps_the_session() {
    let client = std::sync::Arc::new(AcpAgentClient::new());
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let prompt_client = std::sync::Arc::clone(&client);
    let prompt_session = session_id.clone();
    let pending = tokio::spawn(async move {
        let (tx, _rx) = mpsc::unbounded_channel();
        prompt_client
            .send_prompt(
                &prompt_session,
                vec![PromptPart::Text("__WAIT_FOR_CANCEL__".to_string())],
                tx,
            )
            .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // Cancel must not wait for the prompt lock that the pending turn holds.
    tokio::time::timeout(std::time::Duration::from_secs(2), client.cancel(&session_id))
        .await
        .expect("cancel must not block on the running turn")
        .expect("cancel should succeed");

    let stop_reason = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
        .await
        .expect("the cancelled turn must end")
        .expect("prompt task must not panic")
        .expect("a cancelled turn is a normal finish");
    assert_eq!(stop_reason, "cancelled");

    // The session is still alive after a cancel.
    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = client
        .send_prompt(&session_id, vec![PromptPart::Text("hello".to_string())], tx)
        .await
        .expect("the session must survive a cancel");
    assert_eq!(stop_reason, "end_turn");
    assert_eq!(
        rx.recv().await,
        Some(AcpUpdate::Text {
            text: "fixture reply".to_string()
        })
    );
}

#[tokio::test]
async fn acp_agent_client_cancel_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let err = client
        .cancel("no-such-session")
        .await
        .expect_err("unknown session id must error");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_set_config_option_returns_the_new_option_list() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let options = client
        .set_config_option(&session_id, "model", "opus")
        .await
        .expect("set_config_option");
    let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["model", "effort"]);
    assert_eq!(options[0].current_value, "opus");
    assert_eq!(options[1].category.as_deref(), Some("thought_level"));
}

#[tokio::test]
async fn acp_agent_client_set_config_option_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let err = client
        .set_config_option("no-such-session", "model", "opus")
        .await
        .expect_err("unknown session id must error");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_denies_permission_requests() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.send_prompt(
            &session_id,
            vec![PromptPart::Text("__PERMISSION__".to_string())],
            tx,
        ),
    )
    .await
    .expect("a permission request must never hang the turn")
    .expect("send_prompt");
    assert_eq!(stop_reason, "end_turn");
    assert_eq!(
        drain_updates(&mut rx),
        vec![AcpUpdate::Text {
            text: "permission:selected:reject-once".to_string()
        }]
    );
}

#[tokio::test]
async fn acp_agent_client_sends_resources_as_embedded_resources_when_supported() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let (tx, mut rx) = mpsc::unbounded_channel();
    client
        .send_prompt(
            &session_id,
            vec![
                PromptPart::Resource {
                    uri: "rocket://request/a".to_string(),
                    mime_type: Some("text/plain".to_string()),
                    text: "GET /a".to_string(),
                },
                PromptPart::Text("__DESCRIBE__".to_string()),
            ],
            tx,
        )
        .await
        .expect("send_prompt");
    assert_eq!(
        drain_updates(&mut rx),
        vec![AcpUpdate::Text {
            text: "resource:rocket://request/a:text/plain|text".to_string()
        }]
    );
}

#[tokio::test]
async fn acp_agent_client_sends_resources_as_text_without_embedded_context() {
    let client = AcpAgentClient::new();
    let info = client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[("FIXTURE_NO_EMBEDDED_CONTEXT".to_string(), "1".to_string())],
            &[],
            None,
        )
        .await
        .expect("start_session");
    assert!(!info.prompt_capabilities.embedded_context);

    let (tx, mut rx) = mpsc::unbounded_channel();
    client
        .send_prompt(
            &info.session_id,
            vec![
                PromptPart::Resource {
                    uri: "rocket://request/a".to_string(),
                    mime_type: None,
                    text: "GET /a".to_string(),
                },
                PromptPart::Text("__DESCRIBE__".to_string()),
            ],
            tx,
        )
        .await
        .expect("send_prompt");
    assert_eq!(
        drain_updates(&mut rx),
        vec![AcpUpdate::Text {
            text: "text|text".to_string()
        }]
    );
}

#[tokio::test]
async fn acp_agent_client_send_prompt_with_no_parts_is_rejected() {
    let client = AcpAgentClient::new();
    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt("no-such-session", Vec::new(), tx)
        .await
        .expect_err("an empty prompt must be refused");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}
```

Append this unit-test module at the end of `crates/rocket-infra/src/acp_agent_client.rs` (it tests the pure mapping functions added in Step 9):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        ConfigOptionUpdate, ContentChunk, Cost, ImageContent, SessionConfigSelectGroup,
        SessionConfigSelectOption, ToolCall, ToolCallUpdate, ToolCallUpdateFields, UsageUpdate,
    };

    fn text_chunk(text: &str) -> ContentChunk {
        ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
    }

    #[test]
    fn text_chunks_become_text_updates() {
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentMessageChunk(text_chunk("hi"))),
            Some(AcpUpdate::Text {
                text: "hi".to_string()
            })
        );
    }

    #[test]
    fn non_text_chunks_and_unmodelled_updates_are_dropped() {
        let image = ContentChunk::new(ContentBlock::Image(ImageContent::new("aGk=", "image/png")));
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentMessageChunk(image)),
            None
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::AgentThoughtChunk(text_chunk("thinking"))),
            None
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::UserMessageChunk(text_chunk("echo"))),
            None
        );
    }

    #[test]
    fn tool_calls_map_id_title_kind_and_status() {
        let call = ToolCall::new("call-1", "Run tests")
            .kind(ToolKind::Execute)
            .status(WireToolCallStatus::InProgress);
        assert_eq!(
            session_update_to_acp(SessionUpdate::ToolCall(call)),
            Some(AcpUpdate::ToolCall {
                call_id: "call-1".to_string(),
                title: "Run tests".to_string(),
                kind: "execute".to_string(),
                status: ToolCallStatus::InProgress,
            })
        );
    }

    #[test]
    fn tool_call_updates_keep_missing_fields_as_none() {
        let update = ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new().status(WireToolCallStatus::Failed),
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::ToolCallUpdate(update)),
            Some(AcpUpdate::ToolCallUpdate {
                call_id: "call-1".to_string(),
                title: None,
                status: Some(ToolCallStatus::Failed),
            })
        );
    }

    #[test]
    fn usage_cost_is_kept_only_in_usd() {
        let usd = UsageUpdate::new(10, 100).cost(Cost::new(1.5, "USD"));
        let eur = UsageUpdate::new(10, 100).cost(Cost::new(1.5, "EUR"));
        let none = UsageUpdate::new(10, 100);
        assert_eq!(
            session_update_to_acp(SessionUpdate::UsageUpdate(usd)),
            Some(AcpUpdate::Usage {
                used: 10,
                size: 100,
                cost_usd: Some(1.5)
            })
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::UsageUpdate(eur)),
            Some(AcpUpdate::Usage {
                used: 10,
                size: 100,
                cost_usd: None
            })
        );
        assert_eq!(
            session_update_to_acp(SessionUpdate::UsageUpdate(none)),
            Some(AcpUpdate::Usage {
                used: 10,
                size: 100,
                cost_usd: None
            })
        );
    }

    #[test]
    fn config_options_flatten_groups_and_skip_boolean_options() {
        let grouped = SessionConfigOption::select(
            "model",
            "Model",
            "b",
            vec![
                SessionConfigSelectGroup::new(
                    "g1",
                    "Group 1",
                    vec![SessionConfigSelectOption::new("a", "A")],
                ),
                SessionConfigSelectGroup::new(
                    "g2",
                    "Group 2",
                    vec![SessionConfigSelectOption::new("b", "B").description("Bee")],
                ),
            ],
        )
        .category(SessionConfigOptionCategory::Other("custom".to_string()));
        let boolean = SessionConfigOption::boolean("fast", "Fast", true);

        let options = config_options_from_wire(vec![grouped, boolean]);
        assert_eq!(
            options,
            vec![ConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                category: Some("custom".to_string()),
                current_value: "b".to_string(),
                choices: vec![
                    ConfigChoice {
                        value: "a".to_string(),
                        name: "A".to_string(),
                        description: None,
                    },
                    ConfigChoice {
                        value: "b".to_string(),
                        name: "B".to_string(),
                        description: Some("Bee".to_string()),
                    },
                ],
            }]
        );
    }

    #[test]
    fn config_option_updates_map_to_config_options() {
        let option = SessionConfigOption::select(
            "effort",
            "Effort",
            "high",
            vec![SessionConfigSelectOption::new("high", "High")],
        )
        .category(SessionConfigOptionCategory::ThoughtLevel);
        match session_update_to_acp(SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(
            vec![option],
        ))) {
            Some(AcpUpdate::ConfigOptions { options }) => {
                assert_eq!(options.len(), 1);
                assert_eq!(options[0].category.as_deref(), Some("thought_level"));
            }
            other => panic!("expected ConfigOptions, got {other:?}"),
        }
    }

    #[test]
    fn deny_outcome_prefers_reject_once_then_reject_always_then_cancelled() {
        let allow = PermissionOption::new("allow", "Allow", PermissionOptionKind::AllowOnce);
        let reject_always =
            PermissionOption::new("never", "Never", PermissionOptionKind::RejectAlways);
        let reject_once = PermissionOption::new("no", "No", PermissionOptionKind::RejectOnce);

        assert_eq!(
            deny_outcome(&[allow.clone(), reject_always.clone(), reject_once]),
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new("no"))
        );
        assert_eq!(
            deny_outcome(&[allow.clone(), reject_always]),
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new("never"))
        );
        assert_eq!(deny_outcome(&[allow]), RequestPermissionOutcome::Cancelled);
    }

    #[test]
    fn resources_are_embedded_when_the_agent_supports_it() {
        let blocks = prompt_parts_to_wire(
            vec![PromptPart::Resource {
                uri: "rocket://x".to_string(),
                mime_type: Some("text/plain".to_string()),
                text: "body".to_string(),
            }],
            true,
        );
        match &blocks[..] {
            [ContentBlock::Resource(resource)] => match &resource.resource {
                EmbeddedResourceResource::TextResourceContents(contents) => {
                    assert_eq!(contents.uri, "rocket://x");
                    assert_eq!(contents.text, "body");
                    assert_eq!(contents.mime_type.as_deref(), Some("text/plain"));
                }
                other => panic!("expected text contents, got {other:?}"),
            },
            other => panic!("expected one resource block, got {other:?}"),
        }
    }

    #[test]
    fn resources_fall_back_to_text_without_embedded_context() {
        let blocks = prompt_parts_to_wire(
            vec![PromptPart::Resource {
                uri: "rocket://x".to_string(),
                mime_type: None,
                text: "body".to_string(),
            }],
            false,
        );
        match &blocks[..] {
            [ContentBlock::Text(text)] => assert_eq!(text.text, "Context from rocket://x:\nbody"),
            other => panic!("expected one text block, got {other:?}"),
        }
    }
}
```

- [ ] **Step 8: Check that the new tests fail to compile**

Run: `cargo check --workspace --all-targets -j4`
Expected: FAIL — `AcpAgentClient` does not implement `cancel`/`set_config_option`, `start_session` has the wrong arity, and `session_update_to_acp`, `config_options_from_wire`, `deny_outcome`, `prompt_parts_to_wire` are not found.

For the user to run later: `cargo test -p rocket-infra -j4 --lib acp_agent_client` and `cargo test -p rocket-infra -j4 --test acp_agent_client`.

- [ ] **Step 9: Implement the `AcpAgentClient` changes**

All edits are in `crates/rocket-infra/src/acp_agent_client.rs`.

(a) Replace the imports at lines 11-24 with:

```rust
use agent_client_protocol::schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, EmbeddedResource,
    EmbeddedResourceResource, EnvVariable, FileSystemCapabilities, HttpHeader, Implementation,
    InitializeRequest, McpServer, McpServerHttp, McpServerStdio, Meta, NewSessionRequest,
    PermissionOption, PermissionOptionKind, PromptCapabilities as WirePromptCapabilities,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigOptionValue, SessionConfigSelectOptions,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, StopReason, TextContent,
    TextResourceContents, ToolCallStatus as WireToolCallStatus, ToolKind,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent as AgentRole, ByteStreams, Client, ConnectionTo, Responder,
};
use async_process::Child;
use rocket_acp::{
    AcpSessionClient, AcpUpdate, ConfigChoice, ConfigOption, McpServerSpec, PromptCapabilities,
    PromptPart, SessionInfo, ToolCallStatus,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::sync::{mpsc::UnboundedSender, oneshot, Mutex};
use tokio::task::JoinHandle;
```

(b) Replace lines 48-59 (the `prompt_lock` and `current_chunk_tx` fields, the closing brace and `type ChunkSlot`) with:

```rust
    /// Serializes prompts on one session. `current_update_tx` holds a single
    /// sender, so two overlapping prompts would otherwise steal or clear
    /// each other's update stream. ACP also allows only one turn at a time.
    /// `cancel` and `set_config_option` never take this lock.
    prompt_lock: Mutex<()>,
    /// Set by `send_prompt` for the duration of one call, read by the
    /// notification handler registered at connect time -- `session/update`
    /// is a push notification uncorrelated with any specific request, so
    /// this indirection is how a fresh per-call `update_tx` receives it.
    /// Updates that arrive between turns find no sender and are dropped.
    current_update_tx: UpdateSlot,
    /// What the agent accepts in a prompt, from its `initialize` answer.
    prompt_capabilities: PromptCapabilities,
}

type UpdateSlot = Arc<std::sync::Mutex<Option<UnboundedSender<AcpUpdate>>>>;
```

(c) Replace `set_chunk_sender` at lines 72-76 with:

```rust
/// Sets the per-prompt update sender. A poisoned lock is recovered because
/// the guarded value is a plain `Option` that cannot be left half-written.
fn set_update_sender(slot: &UpdateSlot, sender: Option<UnboundedSender<AcpUpdate>>) {
    *slot.lock().unwrap_or_else(PoisonError::into_inner) = sender;
}
```

(d) Replace the `start_session` signature at lines 184-191 and add the meta check as the first statement of the body (before the existing `AcpAgentConfig` comment):

```rust
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
        meta: Option<serde_json::Value>,
    ) -> DomainResult<SessionInfo> {
        // `_meta` must be a JSON object. Checked before anything is spawned.
        let meta: Option<Meta> = match meta {
            None => None,
            Some(serde_json::Value::Object(map)) => Some(map),
            Some(_) => {
                return Err(DomainError::InvalidInput(
                    "session meta must be a JSON object".to_string(),
                ))
            }
        };
```

(e) Replace lines 222-223 with:

```rust
        let current_update_tx: UpdateSlot = Arc::new(std::sync::Mutex::new(None));
        let notif_update_tx = Arc::clone(&current_update_tx);
```

(f) Replace the ready channel at lines 231-232 with:

```rust
        let (ready_tx, ready_rx) =
            oneshot::channel::<Result<(SessionInfo, ConnectionTo<AgentRole>), String>>();
```

(g) Replace the builder's notification handler at lines 249-266 (from `.on_receive_notification(` up to and including `agent_client_protocol::on_receive_notification!(),\n                )`) with a typed handler plus the permission handler:

```rust
                .on_receive_notification(
                    move |notification: SessionNotification, _cx: ConnectionTo<AgentRole>| {
                        let notif_update_tx = Arc::clone(&notif_update_tx);
                        async move {
                            if let Some(update) = session_update_to_acp(notification.update) {
                                if let Ok(guard) = notif_update_tx.lock() {
                                    if let Some(tx) = guard.as_ref() {
                                        let _ = tx.send(update);
                                    }
                                }
                            }
                            Ok(())
                        }
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                // Rocket never grants a permission. Answering at once means a
                // permission request can never hang a turn. With built-in
                // tools off and the Rocket tools allowed in advance (Plan 02),
                // no request is expected.
                .on_receive_request(
                    move |request: RequestPermissionRequest,
                          responder: Responder<RequestPermissionResponse>,
                          _cx: ConnectionTo<AgentRole>| async move {
                        tracing::warn!("the agent asked for a permission; Rocket denied it");
                        responder.respond(RequestPermissionResponse::new(deny_outcome(
                            &request.options,
                        )))
                    },
                    agent_client_protocol::on_receive_request!(),
                )
```

Also add `let meta_owned = meta;` directly below `let mcp_servers_owned = mcp_servers.to_vec();` (line 237).

(h) Replace the handshake and its `Ok` arm (lines 270-308, from `let handshake =` through the `let _ = tx.send(Ok((session_id, connection.clone())));` block's closing braces) with:

```rust
                        let handshake = async {
                            let init_response = connection
                                .send_request(
                                    InitializeRequest::new(ProtocolVersion::V1)
                                        .client_capabilities(ClientCapabilities::new().fs(
                                            FileSystemCapabilities::new()
                                                .read_text_file(false)
                                                .write_text_file(false),
                                        ))
                                        .client_info(Implementation::new(
                                            "rocket",
                                            env!("CARGO_PKG_VERSION"),
                                        )),
                                )
                                .block_task()
                                .await?;
                            let prompt_capabilities = prompt_capabilities_from_wire(
                                &init_response.agent_capabilities.prompt_capabilities,
                            );
                            let selected_mcp_servers = select_mcp_servers_for_agent(
                                &mcp_servers_owned,
                                init_response.agent_capabilities.mcp_capabilities.http,
                            );
                            let response = connection
                                .send_request(
                                    NewSessionRequest::new(cwd)
                                        .mcp_servers(mcp_server_specs_to_wire(
                                            &selected_mcp_servers,
                                        ))
                                        .meta(meta_owned),
                                )
                                .block_task()
                                .await?;
                            Ok::<SessionInfo, agent_client_protocol::Error>(SessionInfo {
                                session_id: response.session_id.to_string(),
                                config_options: config_options_from_wire(
                                    response.config_options.unwrap_or_default(),
                                ),
                                prompt_capabilities,
                            })
                        };

                        match handshake.await {
                            Ok(info) => {
                                if let Ok(mut guard) = ready_tx.lock() {
                                    if let Some(tx) = guard.take() {
                                        let _ = tx.send(Ok((info, connection.clone())));
                                    }
                                }
```

(The rest of the `Ok` arm — the comment, `std::future::pending::<()>().await;` and `Ok(())` — and the whole `Err` arm stay as they are.)

(i) Replace lines 372-396 (from `let (session_id, connection) = match handshake {` to `Ok(session_id)`) with:

```rust
        let (info, connection) = match handshake {
            Ok(ready) => ready,
            Err(e) => {
                let _ = terminate_process(&process);
                return Err(e);
            }
        };

        let running = Arc::new(RunningSession {
            connection,
            process,
            prompt_lock: Mutex::new(()),
            current_update_tx,
            prompt_capabilities: info.prompt_capabilities,
        });
        // The flag is read under the `sessions` lock, which the sweep also
        // takes after setting it. So a session is either stored before the
        // sweep drains the map, or it is refused and killed here.
        let mut sessions = self.sessions.lock().await;
        if self.shutting_down.load(Ordering::SeqCst) {
            drop(sessions);
            let _ = terminate_session(&running);
            return Err(shutting_down_error());
        }
        sessions.insert(info.session_id.clone(), running);
        Ok(info)
```

(j) Replace `send_prompt` at lines 399-437 with:

```rust
    async fn send_prompt(
        &self,
        session_id: &str,
        parts: Vec<PromptPart>,
        update_tx: UnboundedSender<AcpUpdate>,
    ) -> DomainResult<String> {
        if parts.is_empty() {
            return Err(DomainError::InvalidInput(
                "a prompt needs at least one part".to_string(),
            ));
        }
        let running = self.running(session_id).await?;
        let prompt = prompt_parts_to_wire(parts, running.prompt_capabilities.embedded_context);

        let _turn = running.prompt_lock.lock().await;
        set_update_sender(&running.current_update_tx, Some(update_tx));

        // `connection.send_request(...)` takes `&self` and `ConnectionTo` is
        // cheaply `Clone` and safe to call concurrently, so no lock is needed
        // around the connection itself (see `RunningSession`'s doc comment).
        let result = running
            .connection
            .send_request(PromptRequest::new(session_id.to_string(), prompt))
            .block_task()
            .await;

        set_update_sender(&running.current_update_tx, None);

        match result {
            Ok(response) => Ok(stop_reason_to_wire_string(response.stop_reason)),
            Err(e) => {
                self.fail_and_remove(session_id).await;
                Err(DomainError::Internal(format!("agent session failed: {e}")))
            }
        }
    }

    async fn cancel(&self, session_id: &str) -> DomainResult<()> {
        // A notification, sent without the prompt lock: the running turn
        // holds that lock until the agent answers it with `cancelled`.
        let running = self.running(session_id).await?;
        running
            .connection
            .send_notification(CancelNotification::new(session_id.to_string()))
            .map_err(|e| DomainError::Internal(format!("failed to send cancel: {e}")))
    }

    async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: &str,
    ) -> DomainResult<Vec<ConfigOption>> {
        let running = self.running(session_id).await?;
        let response = running
            .connection
            .send_request(SetSessionConfigOptionRequest::new(
                session_id.to_string(),
                config_id.to_string(),
                SessionConfigOptionValue::value_id(value.to_string()),
            ))
            .block_task()
            .await
            .map_err(|e| DomainError::Internal(format!("failed to change option: {e}")))?;
        Ok(config_options_from_wire(response.config_options))
    }
```

(k) In the inherent `impl AcpAgentClient` block (line 479), add as its first method:

```rust
    /// Looks up a running session without holding the map lock afterwards.
    async fn running(&self, session_id: &str) -> DomainResult<Arc<RunningSession>> {
        self.sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(format!("acp session '{session_id}'")))
    }
```

(l) After `select_mcp_servers_for_agent` (after line 582), add the pure mapping functions:

```rust
/// Maps one `session/update` to Rocket's typed update. Kinds Rocket does not
/// model (thoughts, user echoes, plans, modes, commands, session info) and
/// non-text message chunks yield `None` and are dropped.
fn session_update_to_acp(update: SessionUpdate) -> Option<AcpUpdate> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
            ContentBlock::Text(text) => Some(AcpUpdate::Text { text: text.text }),
            _ => None,
        },
        SessionUpdate::ToolCall(call) => Some(AcpUpdate::ToolCall {
            call_id: call.tool_call_id.to_string(),
            title: call.title,
            kind: tool_kind_to_wire(call.kind).to_string(),
            status: tool_status_from_wire(call.status),
        }),
        SessionUpdate::ToolCallUpdate(update) => Some(AcpUpdate::ToolCallUpdate {
            call_id: update.tool_call_id.to_string(),
            title: update.fields.title,
            status: update.fields.status.map(tool_status_from_wire),
        }),
        SessionUpdate::ConfigOptionUpdate(update) => Some(AcpUpdate::ConfigOptions {
            options: config_options_from_wire(update.config_options),
        }),
        SessionUpdate::UsageUpdate(usage) => Some(AcpUpdate::Usage {
            used: usage.used,
            size: usage.size,
            cost_usd: usage
                .cost
                .filter(|cost| cost.currency == "USD")
                .map(|cost| cost.amount),
        }),
        _ => None,
    }
}

fn tool_status_from_wire(status: WireToolCallStatus) -> ToolCallStatus {
    match status {
        WireToolCallStatus::Pending => ToolCallStatus::Pending,
        WireToolCallStatus::InProgress => ToolCallStatus::InProgress,
        WireToolCallStatus::Completed => ToolCallStatus::Completed,
        WireToolCallStatus::Failed => ToolCallStatus::Failed,
        _ => ToolCallStatus::InProgress,
    }
}

/// The ACP tool kind in its snake_case wire spelling.
fn tool_kind_to_wire(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "read",
        ToolKind::Edit => "edit",
        ToolKind::Delete => "delete",
        ToolKind::Move => "move",
        ToolKind::Search => "search",
        ToolKind::Execute => "execute",
        ToolKind::Think => "think",
        ToolKind::Fetch => "fetch",
        ToolKind::SwitchMode => "switch_mode",
        _ => "other",
    }
}

fn prompt_capabilities_from_wire(caps: &WirePromptCapabilities) -> PromptCapabilities {
    PromptCapabilities {
        embedded_context: caps.embedded_context,
        image: caps.image,
    }
}

/// Maps the agent's options. Boolean options are skipped: Rocket does not
/// advertise boolean config support, and v1 shows no toggles.
fn config_options_from_wire(options: Vec<SessionConfigOption>) -> Vec<ConfigOption> {
    options
        .into_iter()
        .filter_map(config_option_from_wire)
        .collect()
}

fn config_option_from_wire(option: SessionConfigOption) -> Option<ConfigOption> {
    let SessionConfigKind::Select(select) = option.kind else {
        return None;
    };
    let choices = match select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options,
        SessionConfigSelectOptions::Grouped(groups) => groups
            .into_iter()
            .flat_map(|group| group.options)
            .collect(),
        _ => Vec::new(),
    };
    Some(ConfigOption {
        id: option.id.to_string(),
        name: option.name,
        category: option.category.and_then(category_to_wire),
        current_value: select.current_value.to_string(),
        choices: choices
            .into_iter()
            .map(|choice| ConfigChoice {
                value: choice.value.to_string(),
                name: choice.name,
                description: choice.description,
            })
            .collect(),
    })
}

fn category_to_wire(category: SessionConfigOptionCategory) -> Option<String> {
    match category {
        SessionConfigOptionCategory::Mode => Some("mode".to_string()),
        SessionConfigOptionCategory::Model => Some("model".to_string()),
        SessionConfigOptionCategory::ModelConfig => Some("model_config".to_string()),
        SessionConfigOptionCategory::ThoughtLevel => Some("thought_level".to_string()),
        SessionConfigOptionCategory::Other(other) => Some(other),
        _ => None,
    }
}

/// Picks a reject option, so the agent hears a clear "no". Without one, the
/// only other answer that grants nothing is `Cancelled`.
fn deny_outcome(options: &[PermissionOption]) -> RequestPermissionOutcome {
    let reject = options
        .iter()
        .find(|option| matches!(option.kind, PermissionOptionKind::RejectOnce))
        .or_else(|| {
            options
                .iter()
                .find(|option| matches!(option.kind, PermissionOptionKind::RejectAlways))
        });
    match reject {
        Some(option) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
            option.option_id.clone(),
        )),
        None => RequestPermissionOutcome::Cancelled,
    }
}

/// Builds the prompt blocks. A resource is embedded when the agent accepts
/// embedded context, and sent as labelled plain text otherwise.
fn prompt_parts_to_wire(parts: Vec<PromptPart>, embedded_context: bool) -> Vec<ContentBlock> {
    parts
        .into_iter()
        .map(|part| match part {
            PromptPart::Text(text) => ContentBlock::Text(TextContent::new(text)),
            PromptPart::Resource {
                uri,
                mime_type,
                text,
            } if embedded_context => ContentBlock::Resource(EmbeddedResource::new(
                EmbeddedResourceResource::TextResourceContents(
                    TextResourceContents::new(text, uri).mime_type(mime_type),
                ),
            )),
            PromptPart::Resource { uri, text, .. } => {
                ContentBlock::Text(TextContent::new(format!("Context from {uri}:\n{text}")))
            }
        })
        .collect()
}
```

- [ ] **Step 10: Keep `rocket-app` compiling with the new trait (minimal adaptation)**

`AcpSessionService`'s own signatures stay the same until Task 2. In `crates/rocket-app/src/acp_session_service.rs`:

Replace line 4 with:

```rust
use rocket_acp::{AcpSessionClient, AcpUpdate, PromptPart};
```

Replace lines 156-159 with:

```rust
        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &mcp_servers, None)
            .await?
            .session_id;
```

Replace lines 188-199 (the channel, the drain future and the client call) with:

```rust
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AcpUpdate>();
        let session_id_owned = session_id.to_string();

        let drain_chunks = async {
            while let Some(update) = rx.recv().await {
                // Task 2 of the ACP client upgrade publishes the other update kinds.
                if let AcpUpdate::Text { text } = update {
                    self.event_publisher.publish(DomainEvent::AcpSessionChunk {
                        session_id: session_id_owned.clone(),
                        text,
                    });
                }
            }
        };
        let send = self
            .session_client
            .send_prompt(session_id, vec![PromptPart::Text(prompt)], tx);
```

In the test module, add after `use tokio::sync::mpsc::UnboundedSender;` (line 266):

```rust
    use rocket_acp::{ConfigOption, PromptCapabilities, SessionInfo};
```

Replace `FakeSessionClient`, its `Default` impl and its trait impl (lines 430-492) with the final fake, which Task 2's tests also use:

```rust
    struct FakeSessionClient {
        start_should_fail: bool,
        start_config_options: Vec<ConfigOption>,
        prompt_updates: Vec<AcpUpdate>,
        update_interval: Duration,
        prompt_stop_reason: String,
        prompt_should_fail: bool,
        prompt_delay: Duration,
        end_session_called: Arc<AtomicBool>,
        end_all_sessions_called: Arc<AtomicBool>,
        cancel_called: Arc<AtomicBool>,
        options_after_set: Vec<ConfigOption>,
    }
    impl Default for FakeSessionClient {
        fn default() -> Self {
            Self {
                start_should_fail: false,
                start_config_options: Vec::new(),
                prompt_updates: vec![AcpUpdate::Text {
                    text: "hello".to_string(),
                }],
                update_interval: Duration::ZERO,
                prompt_stop_reason: "end_turn".to_string(),
                prompt_should_fail: false,
                prompt_delay: Duration::ZERO,
                end_session_called: Arc::new(AtomicBool::new(false)),
                end_all_sessions_called: Arc::new(AtomicBool::new(false)),
                cancel_called: Arc::new(AtomicBool::new(false)),
                options_after_set: Vec::new(),
            }
        }
    }
    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            _mcp_servers: &[rocket_acp::McpServerSpec],
            _meta: Option<serde_json::Value>,
        ) -> DomainResult<SessionInfo> {
            if self.start_should_fail {
                Err(DomainError::InvalidInput("command not found".to_string()))
            } else {
                Ok(SessionInfo {
                    session_id: "session-1".to_string(),
                    config_options: self.start_config_options.clone(),
                    prompt_capabilities: PromptCapabilities {
                        embedded_context: true,
                        image: false,
                    },
                })
            }
        }
        async fn send_prompt(
            &self,
            _session_id: &str,
            _parts: Vec<PromptPart>,
            update_tx: UnboundedSender<AcpUpdate>,
        ) -> DomainResult<String> {
            tokio::time::sleep(self.prompt_delay).await;
            for update in &self.prompt_updates {
                tokio::time::sleep(self.update_interval).await;
                let _ = update_tx.send(update.clone());
            }
            if self.prompt_should_fail {
                Err(DomainError::Internal("agent crashed".to_string()))
            } else {
                Ok(self.prompt_stop_reason.clone())
            }
        }
        async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
            self.cancel_called.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn set_config_option(
            &self,
            _session_id: &str,
            _config_id: &str,
            _value: &str,
        ) -> DomainResult<Vec<ConfigOption>> {
            Ok(self.options_after_set.clone())
        }
        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            self.end_session_called.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn end_all_sessions(&self) -> DomainResult<()> {
            self.end_all_sessions_called.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
```

In `send_prompt_publishes_every_chunk_before_finished_in_order` (lines 547-584), replace the client construction (lines 549-552) with:

```rust
        let client = FakeSessionClient {
            prompt_updates: vec![
                AcpUpdate::Text {
                    text: "Hello, ".to_string(),
                },
                AcpUpdate::Text {
                    text: "world!".to_string(),
                },
            ],
            ..Default::default()
        };
```

Replace the `CapturingSessionClient` trait impl (lines 880-907) with:

```rust
    #[async_trait::async_trait]
    impl AcpSessionClient for CapturingSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            mcp_servers: &[rocket_acp::McpServerSpec],
            _meta: Option<serde_json::Value>,
        ) -> DomainResult<SessionInfo> {
            *self.captured_servers.lock().expect("lock") = mcp_servers.to_vec();
            Ok(SessionInfo {
                session_id: "session-1".to_string(),
                config_options: Vec::new(),
                prompt_capabilities: PromptCapabilities::default(),
            })
        }
        async fn send_prompt(
            &self,
            _session_id: &str,
            _parts: Vec<PromptPart>,
            _update_tx: UnboundedSender<AcpUpdate>,
        ) -> DomainResult<String> {
            unreachable!("not exercised by this test")
        }
        async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
            unreachable!("not exercised by this test")
        }
        async fn set_config_option(
            &self,
            _session_id: &str,
            _config_id: &str,
            _value: &str,
        ) -> DomainResult<Vec<ConfigOption>> {
            unreachable!("not exercised by this test")
        }
        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            unreachable!("not exercised by this test")
        }
        async fn end_all_sessions(&self) -> DomainResult<()> {
            unreachable!("not exercised by this test")
        }
    }
```

- [ ] **Step 11: Keep the `src-tauri` integration test's fake compiling**

In `src-tauri/tests/acp_mcp_start_agent_session.rs`, replace line 16 with:

```rust
use rocket_acp::{
    AcpSessionClient, AcpUpdate, AgentConfig, AgentConfigRepository, ConfigOption, McpServerSpec,
    PromptCapabilities, PromptPart, SessionInfo,
};
```

Line 28 (`use tokio::sync::mpsc::UnboundedSender;`) stays. Replace the trait impl at lines 158-188 with:

```rust
#[async_trait::async_trait]
impl AcpSessionClient for FakeSessionClient {
    async fn start_session(
        &self,
        _command: &str,
        _args: &[String],
        _cwd: &str,
        _env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
        _meta: Option<serde_json::Value>,
    ) -> DomainResult<SessionInfo> {
        *self.captured_servers.lock().expect("lock") = mcp_servers.to_vec();
        if self.should_fail {
            Err(DomainError::Internal("agent process failed to start".to_string()))
        } else {
            Ok(SessionInfo {
                session_id: self.real_session_id.clone(),
                config_options: Vec::new(),
                prompt_capabilities: PromptCapabilities::default(),
            })
        }
    }
    async fn send_prompt(
        &self,
        _session_id: &str,
        _parts: Vec<PromptPart>,
        _update_tx: UnboundedSender<AcpUpdate>,
    ) -> DomainResult<String> {
        unreachable!("not exercised by this test")
    }
    async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
    async fn set_config_option(
        &self,
        _session_id: &str,
        _config_id: &str,
        _value: &str,
    ) -> DomainResult<Vec<ConfigOption>> {
        unreachable!("not exercised by this test")
    }
    async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
    async fn end_all_sessions(&self) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
}
```

`src-tauri` already depends on `serde_json` (`src-tauri/Cargo.toml:48`), so `serde_json::Value` resolves in its integration tests.

- [ ] **Step 12: Update `crates/rocket-acp/CLAUDE.md`**

Replace the module map table rows with:

```markdown
| `agent_config.rs` | `AgentConfig` struct + `AgentConfigRepository` trait |
| `session.rs` | `AcpSessionClient` trait (`start_session`, `send_prompt`, `cancel`, `set_config_option`, `end_session`, `end_all_sessions`) |
| `session_info.rs` | `SessionInfo`, `PromptCapabilities`; re-exports `ConfigOption`/`ConfigChoice` from `rocket_shared::acp` |
| `update.rs` | `AcpUpdate` (typed agent updates) and `ToolCallStatus` |
| `prompt.rs` | `PromptPart` (text and embedded text resources) |
| `mcp_server_spec.rs` | `McpServerSpec` |
```

Replace the sentence "`send_prompt` streams text through a plain `tokio::sync::mpsc::UnboundedSender<String>` — event publishing belongs in `rocket-app`'s `AcpSessionService`, not in this trait." with "`send_prompt` streams typed `AcpUpdate`s through a plain `tokio::sync::mpsc::UnboundedSender<AcpUpdate>` — event publishing belongs in `rocket-app`'s `AcpSessionService`, not in this trait. `ConfigOption` is defined in `rocket-shared` because `DomainEvent` carries it." In the Dependencies list, change the `serde_json` line to "`serde_json` — the `_meta` value of `start_session`".

- [ ] **Step 13: Verify the workspace compiles**

Run: `cargo check --workspace --all-targets -j4`
Expected: PASS with no errors.

For the user to run: `cargo test -p rocket-shared -j4 acp`, `cargo test -p rocket-acp -j4`, `cargo test -p rocket-infra -j4 --lib acp_agent_client`, `cargo test -p rocket-infra -j4 --test acp_agent_client`, `cargo test -p rocket-app -j4 acp_session_service`, `cargo test -p rocket -j4 --test acp_mcp_start_agent_session`.

- [ ] **Step 14: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with the message `feat(rocket-acp): add typed ACP updates, session info and prompt parts`, staging exactly:
- `crates/rocket-shared/src/acp.rs`, `crates/rocket-shared/src/lib.rs`
- `crates/rocket-acp/Cargo.toml`, `crates/rocket-acp/CLAUDE.md`, `crates/rocket-acp/src/lib.rs`, `crates/rocket-acp/src/session.rs`, `crates/rocket-acp/src/session_info.rs`, `crates/rocket-acp/src/update.rs`, `crates/rocket-acp/src/prompt.rs`
- `crates/rocket-infra/src/acp_agent_client.rs`, `crates/rocket-infra/src/bin/test_acp_agent.rs`, `crates/rocket-infra/tests/acp_agent_client.rs`
- `crates/rocket-app/src/acp_session_service.rs`
- `src-tauri/tests/acp_mcp_start_agent_session.rs`
- `Cargo.lock` only if `cargo check` changed it

---

### Task 2: `AcpSessionService` — typed events, idle timeout, cancel and option changes

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (variants after `:550-554`, tests after `:1875`, import at `:1`)
- Modify: `src-tauri/src/tauri_event_bus.rs:49`
- Modify: `crates/rocket-app/src/acp_session_service.rs` (struct, constants, constructors, `start_session`, `send_prompt`, new methods, tests)
- Modify: `crates/rocket-app/CLAUDE.md:94` (the `AcpSessionService` row)
- Modify: `src-tauri/src/commands/acp_sessions.rs:1-3,9-29,54-112,114-121`
- Modify: `src-tauri/tests/acp_mcp_start_agent_session.rs:256-268,295-307`

**Interfaces:**
- Consumes: Task 1's `AcpUpdate`, `ToolCallStatus::as_str`, `ConfigOption`, `PromptPart`, `SessionInfo`, and the new `AcpSessionClient` methods.
- Produces:
  ```rust
  // rocket_shared::events::DomainEvent
  AcpToolActivity { session_id: String, call_id: String, title: String, status: String },
  AcpConfigOptionsChanged { session_id: String, options: Vec<ConfigOption> },
  AcpUsage { session_id: String, used: u64, size: u64, cost_usd: Option<f64> },
  // rocket_app::AcpSessionService
  pub fn new(session_client, event_publisher, agent_config_service, collection_repo) -> Self;
  pub fn with_prompt_idle_timeout(session_client, event_publisher, agent_config_service,
                                  collection_repo, prompt_idle_timeout: Duration) -> Self;
  pub async fn start_session(&self, agent_config_id: &str, cwd: &str, collection: &str,
      mcp_http: Option<McpHttpServerCredentials>) -> DomainResult<SessionInfo>;
  pub async fn send_prompt(&self, session_id: &str, parts: Vec<PromptPart>) -> DomainResult<String>;
  pub async fn cancel(&self, session_id: &str) -> DomainResult<()>;
  pub async fn set_config_option(&self, session_id: &str, config_id: &str, value: &str)
      -> DomainResult<Vec<ConfigOption>>;
  // src-tauri (crate-internal, used by Task 3)
  pub async fn start_agent_session_inner<R: tauri::Runtime>(...) -> Result<SessionInfo, DomainError>;
  ```
  Channels: `AcpToolActivity` → `agent-session-tool-activity`, `AcpConfigOptionsChanged` → `agent-session-config-options`, `AcpUsage` → `agent-session-usage`.

- [ ] **Step 1: Write the failing event wire-shape tests**

In `crates/rocket-shared/src/events.rs`, append inside the test module after `acp_tool_invoked_wire_shape`:

```rust
    #[test]
    fn acp_tool_activity_wire_shape() {
        let event = DomainEvent::AcpToolActivity {
            session_id: "sess-1".into(),
            call_id: "call-1".into(),
            title: "Read file".into(),
            status: "in_progress".into(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpToolActivity","session_id":"sess-1","call_id":"call-1","title":"Read file","status":"in_progress"}"#
        );
    }

    #[test]
    fn acp_config_options_changed_wire_shape() {
        use crate::acp::ConfigChoice;
        let event = DomainEvent::AcpConfigOptionsChanged {
            session_id: "sess-1".into(),
            options: vec![ConfigOption {
                id: "model".into(),
                name: "Model".into(),
                category: Some("model".into()),
                current_value: "opus".into(),
                choices: vec![ConfigChoice {
                    value: "opus".into(),
                    name: "Opus".into(),
                    description: None,
                }],
            }],
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"acpConfigOptionsChanged","session_id":"sess-1","options":[{"id":"model","name":"Model","category":"model","current_value":"opus","choices":[{"value":"opus","name":"Opus","description":null}]}]}"#
        );
    }

    #[test]
    fn acp_usage_wire_shape_with_and_without_cost() {
        let with_cost = DomainEvent::AcpUsage {
            session_id: "sess-1".into(),
            used: 53_000,
            size: 200_000,
            cost_usd: Some(0.045),
        };
        assert_eq!(
            serde_json::to_string(&with_cost).expect("serialize"),
            r#"{"type":"acpUsage","session_id":"sess-1","used":53000,"size":200000,"cost_usd":0.045}"#
        );
        let without_cost = DomainEvent::AcpUsage {
            session_id: "sess-1".into(),
            used: 1,
            size: 2,
            cost_usd: None,
        };
        assert_eq!(
            serde_json::to_string(&without_cost).expect("serialize"),
            r#"{"type":"acpUsage","session_id":"sess-1","used":1,"size":2,"cost_usd":null}"#
        );
    }
```

- [ ] **Step 2: Write the failing `AcpSessionService` tests**

In `crates/rocket-app/src/acp_session_service.rs`'s test module, replace the Task 1 import line `use rocket_acp::{ConfigOption, PromptCapabilities, SessionInfo};` with:

```rust
    use rocket_acp::{ConfigChoice, ConfigOption, PromptCapabilities, SessionInfo, ToolCallStatus};
```

Update the existing tests to the new service signatures:
- In `start_session_resolves_config_and_credential_and_publishes_started`, `start_session_with_default_unconfigured_settings_still_works`, `start_session_with_autonomy_disabled_ignores_provided_mcp_http_credentials` and `start_session_with_autonomy_enabled_but_no_mcp_http_credentials_fails_open_to_chat_only`, replace `assert_eq!(session_id, "session-1");` with `assert_eq!(session_id.session_id, "session-1");` (four sites).
- In `send_prompt_publishes_every_chunk_before_finished_in_order`, `send_prompt_failure_publishes_failed_and_returns_the_error` and `send_prompt_timeout_kills_the_session_and_publishes_failed`, replace `.send_prompt("session-1", "hi".to_string())` with `.send_prompt("session-1", hi())` (three sites).
- In `send_prompt_timeout_kills_the_session_and_publishes_failed`, replace `AcpSessionService::with_prompt_timeout(` with `AcpSessionService::with_prompt_idle_timeout(`.

Append these helpers and tests at the end of the test module:

```rust
    fn hi() -> Vec<PromptPart> {
        vec![PromptPart::Text("hi".to_string())]
    }

    fn sample_option(id: &str, current: &str) -> ConfigOption {
        ConfigOption {
            id: id.to_string(),
            name: id.to_string(),
            category: None,
            current_value: current.to_string(),
            choices: vec![ConfigChoice {
                value: current.to_string(),
                name: current.to_string(),
                description: None,
            }],
        }
    }

    fn service_with(
        client: FakeSessionClient,
        publisher: &Arc<FakeEventPublisher>,
        idle: Duration,
    ) -> AcpSessionService {
        AcpSessionService::with_prompt_idle_timeout(
            Box::new(client),
            Box::new(SharedEventPublisher(Arc::clone(publisher))),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
            idle,
        )
    }

    #[tokio::test]
    async fn start_session_returns_the_client_session_info() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            start_config_options: vec![sample_option("model", "default")],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        let info = service
            .start_session("agent-1", "/tmp", "demo", None)
            .await
            .expect("start_session should succeed");
        assert_eq!(info.session_id, "session-1");
        assert_eq!(info.config_options, vec![sample_option("model", "default")]);
        assert!(info.prompt_capabilities.embedded_context);
    }

    #[tokio::test]
    async fn send_prompt_publishes_every_update_kind_in_order_before_finished() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_updates: vec![
                AcpUpdate::Text {
                    text: "Reading".to_string(),
                },
                AcpUpdate::ToolCall {
                    call_id: "call-1".to_string(),
                    title: "Read GET /orders".to_string(),
                    kind: "read".to_string(),
                    status: ToolCallStatus::Pending,
                },
                AcpUpdate::ToolCallUpdate {
                    call_id: "call-1".to_string(),
                    title: None,
                    status: Some(ToolCallStatus::Completed),
                },
                AcpUpdate::ConfigOptions {
                    options: vec![sample_option("model", "opus")],
                },
                AcpUpdate::Usage {
                    used: 1_200,
                    size: 200_000,
                    cost_usd: Some(0.01),
                },
            ],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        let stop_reason = service
            .send_prompt("session-1", hi())
            .await
            .expect("send_prompt should succeed");
        assert_eq!(stop_reason, "end_turn");

        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 6, "got {events:?}");
        assert!(
            matches!(&events[0], DomainEvent::AcpSessionChunk { text, .. } if text == "Reading")
        );
        assert!(matches!(
            &events[1],
            DomainEvent::AcpToolActivity { call_id, title, status, .. }
                if call_id == "call-1" && title == "Read GET /orders" && status == "pending"
        ));
        assert!(
            matches!(
                &events[2],
                DomainEvent::AcpToolActivity { title, status, .. }
                    if title == "Read GET /orders" && status == "completed"
            ),
            "an update without a title keeps the last known title, got {:?}",
            events[2]
        );
        assert!(matches!(
            &events[3],
            DomainEvent::AcpConfigOptionsChanged { options, .. }
                if options.len() == 1 && options[0].current_value == "opus"
        ));
        assert!(matches!(
            &events[4],
            DomainEvent::AcpUsage {
                used: 1_200,
                size: 200_000,
                cost_usd: Some(_),
                ..
            }
        ));
        assert!(matches!(
            &events[5],
            DomainEvent::AcpSessionFinished { stop_reason, .. } if stop_reason == "end_turn"
        ));
    }

    #[tokio::test]
    async fn tool_call_update_for_an_unknown_call_publishes_an_empty_title_and_pending() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_updates: vec![AcpUpdate::ToolCallUpdate {
                call_id: "call-9".to_string(),
                title: None,
                status: None,
            }],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        service
            .send_prompt("session-1", hi())
            .await
            .expect("send_prompt should succeed");

        let events = publisher.events.lock().expect("lock");
        assert!(
            matches!(
                &events[0],
                DomainEvent::AcpToolActivity { call_id, title, status, .. }
                    if call_id == "call-9" && title.is_empty() && status == "pending"
            ),
            "got {events:?}"
        );
    }

    #[tokio::test]
    async fn idle_timeout_restarts_on_every_update() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_updates: (0..5)
                .map(|i| AcpUpdate::Text {
                    text: format!("chunk {i}"),
                })
                .collect(),
            update_interval: Duration::from_millis(40),
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        // Five updates 40 ms apart take about 200 ms, longer than the 150 ms
        // idle limit, but no single gap reaches it.
        let service = service_with(client, &publisher, Duration::from_millis(150));

        let stop_reason = service
            .send_prompt("session-1", hi())
            .await
            .expect("a turn that keeps sending updates must not time out");
        assert_eq!(stop_reason, "end_turn");
        assert!(!end_session_called.load(Ordering::SeqCst));
        let events = publisher.events.lock().expect("lock");
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })));
    }

    #[tokio::test]
    async fn cancelled_stop_reason_is_a_normal_finish_and_keeps_the_session() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_stop_reason: "cancelled".to_string(),
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        let stop_reason = service
            .send_prompt("session-1", hi())
            .await
            .expect("a cancelled turn is a normal finish");
        assert_eq!(stop_reason, "cancelled");
        assert!(!end_session_called.load(Ordering::SeqCst));

        let events = publisher.events.lock().expect("lock");
        assert!(matches!(
            events.last(),
            Some(DomainEvent::AcpSessionFinished { stop_reason, .. }) if stop_reason == "cancelled"
        ));
        assert!(!events
            .iter()
            .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })));
    }

    #[tokio::test]
    async fn cancel_delegates_to_the_session_client_without_events() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let cancel_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            cancel_called: Arc::clone(&cancel_called),
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        service.cancel("session-1").await.expect("cancel should succeed");
        assert!(cancel_called.load(Ordering::SeqCst));
        assert!(publisher.events.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn set_config_option_returns_the_new_options_and_publishes_them() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            options_after_set: vec![sample_option("model", "opus"), sample_option("effort", "high")],
            ..Default::default()
        };
        let service = service_with(client, &publisher, Duration::from_secs(5));

        let options = service
            .set_config_option("session-1", "model", "opus")
            .await
            .expect("set_config_option should succeed");
        assert_eq!(options.len(), 2);

        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            DomainEvent::AcpConfigOptionsChanged { session_id, options }
                if session_id == "session-1" && options.len() == 2
        ));
    }
```

- [ ] **Step 3: Check that the new tests fail to compile**

Run: `cargo check --workspace --all-targets -j4`
Expected: FAIL — no variants `AcpToolActivity`, `AcpConfigOptionsChanged`, `AcpUsage`; no function `with_prompt_idle_timeout`; no methods `cancel`/`set_config_option` on `AcpSessionService`; `send_prompt` takes a `String`.

For the user to run later: `cargo test -p rocket-shared -j4 acp_` and `cargo test -p rocket-app -j4 acp_session_service`.

- [ ] **Step 4: Add the `DomainEvent` variants and the event-bus mapping**

In `crates/rocket-shared/src/events.rs`, change line 1 to:

```rust
use serde::{Deserialize, Serialize};

use crate::acp::ConfigOption;
```

Add after the `AcpToolInvoked` variant (after line 554):

```rust
    /// Emitted for every tool call start or change during a prompt turn.
    /// `title` and `status` are the last known values, so the UI can upsert
    /// by `call_id`. `status` is `pending`, `in_progress`, `completed` or
    /// `failed`.
    AcpToolActivity {
        session_id: String,
        call_id: String,
        title: String,
        status: String,
    },
    /// Emitted with the agent's full option list whenever it changes, during
    /// a turn or after `set_config_option`.
    AcpConfigOptionsChanged {
        session_id: String,
        options: Vec<ConfigOption>,
    },
    /// Emitted when the agent reports context window use. `cost_usd` is the
    /// cumulative session cost, present only when reported in US dollars.
    AcpUsage {
        session_id: String,
        used: u64,
        size: u64,
        cost_usd: Option<f64>,
    },
```

In `src-tauri/src/tauri_event_bus.rs`, add after line 49 (`DomainEvent::AcpToolInvoked { .. } => "agent-tool-invoked",`):

```rust
            DomainEvent::AcpToolActivity { .. } => "agent-session-tool-activity",
            DomainEvent::AcpConfigOptionsChanged { .. } => "agent-session-config-options",
            DomainEvent::AcpUsage { .. } => "agent-session-usage",
```

- [ ] **Step 5: Implement the `AcpSessionService` changes**

In `crates/rocket-app/src/acp_session_service.rs`:

Replace lines 1-25 (imports through `DEFAULT_PROMPT_TIMEOUT`) with:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rocket_acp::{
    AcpSessionClient, AcpUpdate, ConfigOption, PromptPart, SessionInfo, ToolCallStatus,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::agent_config_service::AgentConfigService;

/// Orchestrates ACP agent sessions. It resolves an agent's command and
/// credential through `AgentConfigService`, then drives the injected
/// `AcpSessionClient`. It publishes `AcpSession*`, `AcpToolActivity`,
/// `AcpConfigOptionsChanged` and `AcpUsage` domain events for the UI.
///
/// This service keeps no session map of its own. The session client owns
/// session state and process lifecycle.
pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    prompt_idle_timeout: Duration,
}

/// Fixed idle limit for one prompt turn, from the spec. Every update from the
/// agent restarts it, so a long turn that keeps making progress is never cut
/// off. It is not user-configurable.
const DEFAULT_PROMPT_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Last known title and status of each tool call in one prompt turn. A tool
/// call update may leave either out, and the event always carries both.
type ToolCallStates = HashMap<String, (String, ToolCallStatus)>;
```

Replace the two constructors (lines 40-73 as of the start of this plan) with:

```rust
    /// Production constructor. Uses the fixed 120-second idle limit.
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    ) -> Self {
        Self::with_prompt_idle_timeout(
            session_client,
            event_publisher,
            agent_config_service,
            collection_repo,
            DEFAULT_PROMPT_IDLE_TIMEOUT,
        )
    }

    /// Test seam only. Production wiring always uses `new`, which fixes the
    /// idle limit at the spec's 120-second constant.
    pub fn with_prompt_idle_timeout(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        prompt_idle_timeout: Duration,
    ) -> Self {
        Self {
            session_client,
            event_publisher,
            agent_config_service,
            collection_repo,
            prompt_idle_timeout,
        }
    }
```

Change `start_session`'s return type from `DomainResult<String>` to `DomainResult<SessionInfo>`, change the last sentence of its doc comment to "On success, `AcpSessionStarted` is published and the session info is returned.", and replace its tail (the Task 1 client call through `Ok(session_id)`) with:

```rust
        // Plan 02 passes the isolation `_meta` here.
        let info = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &mcp_servers, None)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: info.session_id.clone(),
            });
        Ok(info)
    }
```

Replace the whole `send_prompt` method (doc comment and body) with these methods:

```rust
    /// Sends one prompt turn and returns the agent's stop reason string.
    ///
    /// Each update is published as it arrives: text as `AcpSessionChunk`, tool
    /// calls as `AcpToolActivity`, options as `AcpConfigOptionsChanged`, usage
    /// as `AcpUsage`. Then exactly one terminal event follows:
    /// `AcpSessionFinished` on success (including the `cancelled` stop reason,
    /// which is a normal finish), or `AcpSessionFailed` on error or idle
    /// timeout. The error is still returned to the caller.
    ///
    /// Ordering: the loop ends only once the client's future has resolved
    /// and the update channel has closed, so every update event is published
    /// before the terminal event. The client must forward a turn's updates
    /// before its `send_prompt` resolves; a later update is dropped, never
    /// reordered.
    ///
    /// Idle timeout: the limit restarts after every update. When it runs out,
    /// the pending prompt is dropped and the session is force-killed via
    /// `end_session`, because a hung agent process is still running.
    pub async fn send_prompt(
        &self,
        session_id: &str,
        parts: Vec<PromptPart>,
    ) -> DomainResult<String> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AcpUpdate>();
        let mut send = self.session_client.send_prompt(session_id, parts, tx);
        let mut outcome: Option<DomainResult<String>> = None;
        let mut channel_open = true;
        let mut tool_calls = ToolCallStates::new();

        while outcome.is_none() || channel_open {
            tokio::select! {
                biased;
                update = rx.recv(), if channel_open => match update {
                    Some(update) => self.publish_update(session_id, update, &mut tool_calls),
                    None => channel_open = false,
                },
                result = &mut send, if outcome.is_none() => outcome = Some(result),
                () = tokio::time::sleep(self.prompt_idle_timeout) => {
                    // Drop the pending prompt first, so the client releases
                    // the turn before the session is killed.
                    drop(send);
                    return Err(self.end_idle_session(session_id).await);
                }
            }
        }

        match outcome {
            Some(Ok(stop_reason)) => {
                self.event_publisher
                    .publish(DomainEvent::AcpSessionFinished {
                        session_id: session_id.to_string(),
                        stop_reason: stop_reason.clone(),
                    });
                Ok(stop_reason)
            }
            Some(Err(e)) => {
                self.event_publisher.publish(DomainEvent::AcpSessionFailed {
                    session_id: session_id.to_string(),
                    error: e.to_string(),
                });
                Err(e)
            }
            None => Err(DomainError::Internal(
                "agent prompt ended without a result".to_string(),
            )),
        }
    }

    /// Asks the agent to stop the running turn and returns at once. The
    /// pending `send_prompt` then finishes with the `cancelled` stop reason,
    /// and the session stays open. No event is published here.
    pub async fn cancel(&self, session_id: &str) -> DomainResult<()> {
        self.session_client.cancel(session_id).await
    }

    /// Changes one session option, such as the model, and returns the
    /// agent's new option list. The list is also published as
    /// `AcpConfigOptionsChanged`, because a model change can add or remove
    /// the effort option.
    pub async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: &str,
    ) -> DomainResult<Vec<ConfigOption>> {
        let options = self
            .session_client
            .set_config_option(session_id, config_id, value)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpConfigOptionsChanged {
                session_id: session_id.to_string(),
                options: options.clone(),
            });
        Ok(options)
    }

    /// Publishes one update as its domain event. Tool call updates are merged
    /// with the last known title and status of the same call.
    fn publish_update(&self, session_id: &str, update: AcpUpdate, tool_calls: &mut ToolCallStates) {
        let session_id = session_id.to_string();
        let event = match update {
            AcpUpdate::Text { text } => DomainEvent::AcpSessionChunk { session_id, text },
            AcpUpdate::ToolCall {
                call_id,
                title,
                kind: _,
                status,
            } => {
                tool_calls.insert(call_id.clone(), (title.clone(), status));
                DomainEvent::AcpToolActivity {
                    session_id,
                    call_id,
                    title,
                    status: status.as_str().to_string(),
                }
            }
            AcpUpdate::ToolCallUpdate {
                call_id,
                title,
                status,
            } => {
                let entry = tool_calls
                    .entry(call_id.clone())
                    .or_insert_with(|| (String::new(), ToolCallStatus::Pending));
                if let Some(title) = title {
                    entry.0 = title;
                }
                if let Some(status) = status {
                    entry.1 = status;
                }
                DomainEvent::AcpToolActivity {
                    session_id,
                    call_id,
                    title: entry.0.clone(),
                    status: entry.1.as_str().to_string(),
                }
            }
            AcpUpdate::ConfigOptions { options } => DomainEvent::AcpConfigOptionsChanged {
                session_id,
                options,
            },
            AcpUpdate::Usage {
                used,
                size,
                cost_usd,
            } => DomainEvent::AcpUsage {
                session_id,
                used,
                size,
                cost_usd,
            },
        };
        self.event_publisher.publish(event);
    }

    /// Kills a session whose turn sent nothing for the idle limit, publishes
    /// `AcpSessionFailed`, and returns the error for the caller. The kill
    /// result is ignored on purpose: the timeout is the error the caller must
    /// see, and a session that already crashed changes nothing.
    async fn end_idle_session(&self, session_id: &str) -> DomainError {
        let _ = self.session_client.end_session(session_id).await;
        let message = format!(
            "agent sent no update for {}s",
            self.prompt_idle_timeout.as_secs()
        );
        self.event_publisher.publish(DomainEvent::AcpSessionFailed {
            session_id: session_id.to_string(),
            error: message.clone(),
        });
        DomainError::Internal(message)
    }
```

In `crates/rocket-app/CLAUDE.md`, replace the `AcpSessionService` row (line 94) with:

```markdown
| `AcpSessionService` | Starts/prompts/cancels/ends ACP agent sessions and changes session options via `Box<dyn AcpSessionClient>`; publishes `AcpSessionStarted/Chunk/Finished/Failed`, `AcpToolActivity`, `AcpConfigOptionsChanged` and `AcpUsage` (all update events before the terminal event); a 120 s idle limit, restarted by every update, force-kills a silent session; a `cancelled` stop reason is a normal finish. |
```

- [ ] **Step 6: Adapt the `src-tauri` command layer to the new service signatures**

In `src-tauri/src/commands/acp_sessions.rs`, replace line 3 with:

```rust
use rocket_acp::{PromptPart, SessionInfo};
use rocket_app::{AcpSessionService, CollectionService, McpHttpServerCredentials, McpToolService};
```

In `start_agent_session` (lines 9-29), keep the `Result<String, DomainError>` return type until Task 3 and map the result:

```rust
    start_agent_session_inner(
        agent_config_id,
        cwd,
        collection,
        app_handle,
        &collection_svc,
        &registry,
        &svc,
    )
    .await
    .map(|info| info.session_id)
```

Change `start_agent_session_inner`'s return type (line 62) to `Result<SessionInfo, DomainError>` and replace its final `match` (lines 93-111) with:

```rust
    match (result, mcp_handle) {
        (Ok(info), Some(handle)) => {
            // Registered under the *real* ACP session id, not the
            // pre-handshake mcp_session_id minted above — this is the id
            // end_agent_session/send_agent_prompt (and McpServerRegistry's
            // other callers) all address a session by.
            registry.register(info.session_id.clone(), handle);
            Ok(info)
        }
        (Ok(info), None) => Ok(info),
        (Err(e), Some(handle)) => {
            // start_session failed after the HTTP server was already bound —
            // never leave an orphaned listener holding a live token. `shutdown`
            // is synchronous (Plan 04) — no `.await` here.
            handle.shutdown();
            Err(e)
        }
        (Err(e), None) => Err(e),
    }
```

Replace the body of `send_agent_prompt` (line 120) with:

```rust
    svc.send_prompt(&session_id, vec![PromptPart::Text(prompt)])
        .await
```

In `src-tauri/tests/acp_mcp_start_agent_session.rs`, in both tests that bind `let session_id = start_agent_session_inner(` (lines 256-266 and 295-305), change the trailing `.expect("start_agent_session_inner should succeed");` to `.expect("start_agent_session_inner should succeed").session_id;`.

- [ ] **Step 7: Verify the workspace compiles**

Run: `cargo check --workspace --all-targets -j4`
Expected: PASS with no errors.

For the user to run: `cargo test -p rocket-shared -j4 acp_`, `cargo test -p rocket-app -j4 acp_session_service`, `cargo test -p rocket -j4 --test acp_mcp_start_agent_session`.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with the message `feat(rocket-app): publish typed agent updates with an idle prompt timeout`, staging exactly:
- `crates/rocket-shared/src/events.rs`
- `src-tauri/src/tauri_event_bus.rs`
- `crates/rocket-app/src/acp_session_service.rs`, `crates/rocket-app/CLAUDE.md`
- `src-tauri/src/commands/acp_sessions.rs`
- `src-tauri/tests/acp_mcp_start_agent_session.rs`

---

### Task 3: IPC commands, DTOs and TypeScript wrappers

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src-tauri/src/commands/acp_session_dto.rs`
- Modify: `src-tauri/src/commands/mod.rs:1`
- Modify: `src-tauri/src/commands/acp_sessions.rs` (imports, `start_agent_session`, `send_agent_prompt`, two new commands)
- Modify: `src-tauri/src/lib.rs:920-922`
- Modify: `src/lib/tauri-api.ts:2518-2526` and after `:2636`
- Modify: `src/lib/queries/__tests__/agent-session-api.test.ts`
- Modify: `src/components/request/AgentChatPanel.tsx:57`
- Modify: `src/components/request/__tests__/AgentChatPanel.test.tsx:98,119,134`

`src/lib/agent-session-event-bridge.ts` needs no change in this plan: a `cancelled` stop reason arrives as `agent-session-finished`, which already completes the streaming message, and the new tool, option and usage events are consumed by Plan 05's workspace-level bridge.

**Interfaces:**
- Consumes: Task 2's `AcpSessionService::{send_prompt, cancel, set_config_option}` and `start_agent_session_inner -> Result<SessionInfo, DomainError>`; `rocket_acp::{ConfigChoice, ConfigOption, PromptPart, SessionInfo}`.
- Produces:
  ```rust
  // src-tauri/src/commands/acp_session_dto.rs (camelCase on the wire)
  pub struct ConfigChoiceDto { pub value: String, pub name: String, pub description: Option<String> }
  pub struct ConfigOptionDto { pub id: String, pub name: String, pub category: Option<String>,
                               pub current_value: String, pub choices: Vec<ConfigChoiceDto> }
  pub struct AgentSessionStartedDto { pub session_id: String, pub config_options: Vec<ConfigOptionDto> }
  pub struct PromptResourceDto { pub uri: String, pub mime_type: Option<String>, pub text: String }
  pub const MAX_PROMPT_RESOURCES: usize = 8;
  pub const MAX_PROMPT_RESOURCE_BYTES: usize = 8 * 1024;
  pub fn prompt_parts(prompt: String, resources: Option<Vec<PromptResourceDto>>) -> DomainResult<Vec<PromptPart>>;
  // commands
  start_agent_session(agent_config_id, cwd, collection) -> AgentSessionStartedDto
  send_agent_prompt(session_id, prompt: String, resources: Option<Vec<PromptResourceDto>>) -> String
  cancel_agent_prompt(session_id) -> ()
  set_agent_config_option(session_id, config_id, value) -> Vec<ConfigOptionDto>
  ```
  ```ts
  // src/lib/tauri-api.ts
  export interface ConfigChoice { value: string; name: string; description: string | null }
  export interface ConfigOption { id: string; name: string; category: string | null; currentValue: string; choices: ConfigChoice[] }
  export interface AgentSessionStarted { sessionId: string; configOptions: ConfigOption[] }
  export interface PromptResourceDto { uri: string; mimeType: string | null; text: string }
  startAgentSession(agentConfigId, cwd, collection): Promise<AgentSessionStarted>
  sendAgentPrompt(sessionId, prompt, resources?: PromptResourceDto[]): Promise<string>
  cancelAgentPrompt(sessionId): Promise<void>
  setAgentConfigOption(sessionId, configId, value): Promise<ConfigOption[]>
  onAgentToolActivity, onAgentConfigOptions, onAgentUsage (listeners)
  configOptionsFromEvent(options: AgentConfigOptionPayload[]): ConfigOption[]
  ```

- [ ] **Step 1: Write the failing DTO tests**

Create `src-tauri/src/commands/acp_session_dto.rs` with the imports and only its test module:

```rust
//! IPC DTOs for ACP agent sessions.
//!
//! The `rocket-acp` domain types carry no camelCase serde. These DTOs own the
//! camelCase wire shape that `src/lib/tauri-api.ts` reads and sends.

use rocket_acp::{ConfigChoice, ConfigOption, PromptPart, SessionInfo};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::PromptCapabilities;

    fn resource(uri: &str, text: &str) -> PromptResourceDto {
        PromptResourceDto {
            uri: uri.to_string(),
            mime_type: Some("text/plain".to_string()),
            text: text.to_string(),
        }
    }

    #[test]
    fn session_started_dto_serializes_camel_case() {
        let info = SessionInfo {
            session_id: "s-1".to_string(),
            config_options: vec![ConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                category: Some("model".to_string()),
                current_value: "opus".to_string(),
                choices: vec![ConfigChoice {
                    value: "opus".to_string(),
                    name: "Opus".to_string(),
                    description: None,
                }],
            }],
            prompt_capabilities: PromptCapabilities::default(),
        };
        let json = serde_json::to_string(&AgentSessionStartedDto::from(info)).expect("serialize");
        assert_eq!(
            json,
            r#"{"sessionId":"s-1","configOptions":[{"id":"model","name":"Model","category":"model","currentValue":"opus","choices":[{"value":"opus","name":"Opus","description":null}]}]}"#
        );
    }

    #[test]
    fn prompt_resource_dto_reads_camel_case_and_a_missing_mime_type() {
        let dto: PromptResourceDto =
            serde_json::from_str(r#"{"uri":"rocket://a","mimeType":"text/plain","text":"x"}"#)
                .expect("deserialize");
        assert_eq!(dto.mime_type.as_deref(), Some("text/plain"));
        let dto: PromptResourceDto =
            serde_json::from_str(r#"{"uri":"rocket://a","text":"x"}"#).expect("deserialize");
        assert_eq!(dto.mime_type, None);
    }

    #[test]
    fn prompt_parts_puts_resources_before_the_prompt_text() {
        let parts = prompt_parts(
            "explain".to_string(),
            Some(vec![resource("rocket://request/a", "GET /a")]),
        )
        .expect("parts");
        assert_eq!(
            parts,
            vec![
                PromptPart::Resource {
                    uri: "rocket://request/a".to_string(),
                    mime_type: Some("text/plain".to_string()),
                    text: "GET /a".to_string(),
                },
                PromptPart::Text("explain".to_string()),
            ]
        );
    }

    #[test]
    fn prompt_parts_without_resources_is_one_text_part() {
        let parts = prompt_parts("hi".to_string(), None).expect("parts");
        assert_eq!(parts, vec![PromptPart::Text("hi".to_string())]);
    }

    #[test]
    fn prompt_parts_rejects_too_many_resources() {
        let resources = (0..=MAX_PROMPT_RESOURCES)
            .map(|i| resource(&format!("rocket://r/{i}"), "x"))
            .collect();
        let err = prompt_parts("hi".to_string(), Some(resources))
            .expect_err("more than the limit must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn prompt_parts_rejects_an_oversized_resource() {
        let big = "a".repeat(MAX_PROMPT_RESOURCE_BYTES + 1);
        let err = prompt_parts("hi".to_string(), Some(vec![resource("rocket://big", &big)]))
            .expect_err("an oversized resource must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        // A resource of exactly the limit is accepted.
        let exact = "a".repeat(MAX_PROMPT_RESOURCE_BYTES);
        prompt_parts("hi".to_string(), Some(vec![resource("rocket://ok", &exact)]))
            .expect("a resource at the limit is accepted");
    }

    #[test]
    fn prompt_parts_rejects_an_empty_prompt() {
        let err = prompt_parts("   ".to_string(), None).expect_err("empty must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        // Whitespace text with a resource sends only the resource.
        let parts = prompt_parts("  ".to_string(), Some(vec![resource("rocket://a", "x")]))
            .expect("parts");
        assert_eq!(parts.len(), 1);
    }
}
```

Register the module in `src-tauri/src/commands/mod.rs` before `pub mod acp_sessions;`:

```rust
pub mod acp_session_dto;
pub mod acp_sessions;
```

- [ ] **Step 2: Write the failing TypeScript tests**

Replace the first two tests of `src/lib/queries/__tests__/agent-session-api.test.ts` (`startAgentSession invokes ...` and `sendAgentPrompt invokes ...`, lines 9-34) with:

```ts
  it('startAgentSession invokes start_agent_session and returns the session info', async () => {
    const started = { sessionId: 'session-1', configOptions: [] };
    vi.mocked(invoke).mockResolvedValue(started);
    const { startAgentSession } = await import('@/lib/tauri-api');
    const result = await startAgentSession(
      'agent-1',
      '/collections/my-collection',
      'my-collection',
    );
    expect(invoke).toHaveBeenCalledWith('start_agent_session', {
      agentConfigId: 'agent-1',
      cwd: '/collections/my-collection',
      collection: 'my-collection',
    });
    expect(result).toEqual(started);
  });

  it('sendAgentPrompt sends a null resource list when none is given', async () => {
    vi.mocked(invoke).mockResolvedValue('end_turn');
    const { sendAgentPrompt } = await import('@/lib/tauri-api');
    const result = await sendAgentPrompt('session-1', 'hello');
    expect(invoke).toHaveBeenCalledWith('send_agent_prompt', {
      sessionId: 'session-1',
      prompt: 'hello',
      resources: null,
    });
    expect(result).toBe('end_turn');
  });

  it('sendAgentPrompt passes resources through', async () => {
    vi.mocked(invoke).mockResolvedValue('end_turn');
    const { sendAgentPrompt } = await import('@/lib/tauri-api');
    const resources = [{ uri: 'rocket://request/a', mimeType: 'text/plain', text: 'GET /a' }];
    await sendAgentPrompt('session-1', 'explain', resources);
    expect(invoke).toHaveBeenCalledWith('send_agent_prompt', {
      sessionId: 'session-1',
      prompt: 'explain',
      resources,
    });
  });

  it('cancelAgentPrompt invokes cancel_agent_prompt with the session id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { cancelAgentPrompt } = await import('@/lib/tauri-api');
    await cancelAgentPrompt('session-1');
    expect(invoke).toHaveBeenCalledWith('cancel_agent_prompt', { sessionId: 'session-1' });
  });

  it('setAgentConfigOption invokes set_agent_config_option and returns the options', async () => {
    const options = [
      { id: 'model', name: 'Model', category: 'model', currentValue: 'opus', choices: [] },
    ];
    vi.mocked(invoke).mockResolvedValue(options);
    const { setAgentConfigOption } = await import('@/lib/tauri-api');
    const result = await setAgentConfigOption('session-1', 'model', 'opus');
    expect(invoke).toHaveBeenCalledWith('set_agent_config_option', {
      sessionId: 'session-1',
      configId: 'model',
      value: 'opus',
    });
    expect(result).toEqual(options);
  });
```

Append inside the same `describe` block, after the `onAgentSessionFailed` test:

```ts
  it.each([
    [
      'onAgentToolActivity',
      'agent-session-tool-activity',
      {
        type: 'acpToolActivity',
        session_id: 'session-1',
        call_id: 'call-1',
        title: 'Read GET /orders',
        status: 'in_progress',
      },
    ],
    [
      'onAgentConfigOptions',
      'agent-session-config-options',
      { type: 'acpConfigOptionsChanged', session_id: 'session-1', options: [] },
    ],
    [
      'onAgentUsage',
      'agent-session-usage',
      { type: 'acpUsage', session_id: 'session-1', used: 10, size: 100, cost_usd: null },
    ],
  ] as const)('%s subscribes to %s and unwraps the payload', async (name, channel, payload) => {
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const api = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await api[name](handler);
    expect(listen).toHaveBeenCalledWith(channel, expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('configOptionsFromEvent converts snake_case options to the camelCase shape', async () => {
    const { configOptionsFromEvent } = await import('@/lib/tauri-api');
    expect(
      configOptionsFromEvent([
        {
          id: 'effort',
          name: 'Effort',
          category: 'thought_level',
          current_value: 'high',
          choices: [{ value: 'high', name: 'High', description: null }],
        },
      ]),
    ).toEqual([
      {
        id: 'effort',
        name: 'Effort',
        category: 'thought_level',
        currentValue: 'high',
        choices: [{ value: 'high', name: 'High', description: null }],
      },
    ]);
  });
```

In `src/components/request/__tests__/AgentChatPanel.test.tsx`, change the three `startAgentSession` mocks:
- line 98: `vi.mocked(tauriApi.startAgentSession).mockResolvedValue({ sessionId: 'session-1', configOptions: [] });`
- line 119: `vi.mocked(tauriApi.startAgentSession).mockResolvedValue({ sessionId: 'session-orphan', configOptions: [] });`
- line 134: `vi.mocked(tauriApi.startAgentSession).mockResolvedValue({ sessionId: 'session-1', configOptions: [] });`

- [ ] **Step 3: Check that the new tests fail**

Run: `cargo check --workspace --all-targets -j4`
Expected: FAIL — `cannot find type AgentSessionStartedDto`, `PromptResourceDto`, `cannot find function prompt_parts`, `cannot find value MAX_PROMPT_RESOURCES`.

Run: `yarn tsc --noEmit`
Expected: FAIL — `cancelAgentPrompt`, `setAgentConfigOption`, `onAgentToolActivity`, `onAgentConfigOptions`, `onAgentUsage` and `configOptionsFromEvent` are not exported; `sendAgentPrompt` expects 2 arguments; the AgentChatPanel mocks do not match `Promise<string>`.

For the user to run later: `cargo test -p rocket -j4 --lib acp_session_dto` and `yarn test agent-session-api AgentChatPanel`.

- [ ] **Step 4: Implement the DTOs**

In `src-tauri/src/commands/acp_session_dto.rs`, add between the imports and the test module:

```rust
/// Most resources one prompt may carry (the spec's chip limit).
pub const MAX_PROMPT_RESOURCES: usize = 8;
/// Largest resource text in bytes (the spec's 8 KB per chip).
pub const MAX_PROMPT_RESOURCE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChoiceDto {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOptionDto {
    pub id: String,
    pub name: String,
    pub category: Option<String>,
    pub current_value: String,
    pub choices: Vec<ConfigChoiceDto>,
}

/// What `start_agent_session` returns. Prompt capabilities stay in the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionStartedDto {
    pub session_id: String,
    pub config_options: Vec<ConfigOptionDto>,
}

/// One text resource sent with a prompt, such as a request definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptResourceDto {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: String,
}

impl From<ConfigChoice> for ConfigChoiceDto {
    fn from(choice: ConfigChoice) -> Self {
        // Destructure fully, so a new domain field fails to compile here.
        let ConfigChoice {
            value,
            name,
            description,
        } = choice;
        Self {
            value,
            name,
            description,
        }
    }
}

impl From<ConfigOption> for ConfigOptionDto {
    fn from(option: ConfigOption) -> Self {
        let ConfigOption {
            id,
            name,
            category,
            current_value,
            choices,
        } = option;
        Self {
            id,
            name,
            category,
            current_value,
            choices: choices.into_iter().map(ConfigChoiceDto::from).collect(),
        }
    }
}

impl From<SessionInfo> for AgentSessionStartedDto {
    fn from(info: SessionInfo) -> Self {
        Self {
            session_id: info.session_id,
            config_options: info
                .config_options
                .into_iter()
                .map(ConfigOptionDto::from)
                .collect(),
        }
    }
}

/// Builds the prompt parts: the resources first, then the prompt text. The
/// limits are checked here, at the IPC boundary, so nothing oversized reaches
/// the agent. The errors name the limit, never the resource content.
pub fn prompt_parts(
    prompt: String,
    resources: Option<Vec<PromptResourceDto>>,
) -> DomainResult<Vec<PromptPart>> {
    let resources = resources.unwrap_or_default();
    if resources.len() > MAX_PROMPT_RESOURCES {
        return Err(DomainError::InvalidInput(format!(
            "a prompt may carry at most {MAX_PROMPT_RESOURCES} resources"
        )));
    }
    let mut parts = Vec::with_capacity(resources.len() + 1);
    for resource in resources {
        if resource.text.len() > MAX_PROMPT_RESOURCE_BYTES {
            return Err(DomainError::InvalidInput(format!(
                "a prompt resource may hold at most {MAX_PROMPT_RESOURCE_BYTES} bytes"
            )));
        }
        parts.push(PromptPart::Resource {
            uri: resource.uri,
            mime_type: resource.mime_type,
            text: resource.text,
        });
    }
    if !prompt.trim().is_empty() {
        parts.push(PromptPart::Text(prompt));
    }
    if parts.is_empty() {
        return Err(DomainError::InvalidInput("the prompt is empty".to_string()));
    }
    Ok(parts)
}
```

- [ ] **Step 5: Implement the commands and register them**

In `src-tauri/src/commands/acp_sessions.rs`, replace the Task 2 imports at the top with:

```rust
use std::sync::Arc;

use rocket_acp::SessionInfo;
use rocket_app::{AcpSessionService, CollectionService, McpHttpServerCredentials, McpToolService};
use rocket_shared::error::DomainError;
use tauri::State;

use crate::commands::acp_session_dto::{
    prompt_parts, AgentSessionStartedDto, ConfigOptionDto, PromptResourceDto,
};
use crate::mcp::registry::McpServerRegistry;
```

Change `start_agent_session`'s return type to `Result<AgentSessionStartedDto, DomainError>` and its final `.map(|info| info.session_id)` to `.map(AgentSessionStartedDto::from)`.

Replace `send_agent_prompt` with these three commands:

```rust
/// Sends one prompt turn. `resources` become embedded text resources ahead
/// of the prompt text. Resolves with the stop reason; a stopped turn
/// resolves with `cancelled`.
#[tauri::command]
pub async fn send_agent_prompt(
    session_id: String,
    prompt: String,
    resources: Option<Vec<PromptResourceDto>>,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    let parts = prompt_parts(prompt, resources)?;
    svc.send_prompt(&session_id, parts).await
}

/// Asks the agent to stop the running turn. The session stays open.
#[tauri::command]
pub async fn cancel_agent_prompt(
    session_id: String,
    svc: State<'_, AcpSessionService>,
) -> Result<(), DomainError> {
    svc.cancel(&session_id).await
}

/// Changes one session option, such as the model or the effort level, and
/// returns the agent's new option list.
#[tauri::command]
pub async fn set_agent_config_option(
    session_id: String,
    config_id: String,
    value: String,
    svc: State<'_, AcpSessionService>,
) -> Result<Vec<ConfigOptionDto>, DomainError> {
    let options = svc
        .set_config_option(&session_id, &config_id, &value)
        .await?;
    Ok(options.into_iter().map(ConfigOptionDto::from).collect())
}
```

(`SessionInfo` stays imported because `start_agent_session_inner` returns it.)

In `src-tauri/src/lib.rs`, extend the `generate_handler!` list (lines 920-922) to:

```rust
            commands::acp_sessions::start_agent_session,
            commands::acp_sessions::send_agent_prompt,
            commands::acp_sessions::cancel_agent_prompt,
            commands::acp_sessions::set_agent_config_option,
            commands::acp_sessions::end_agent_session,
```

- [ ] **Step 6: Implement the TypeScript wrappers and the panel adaptation**

In `src/lib/tauri-api.ts`, replace lines 2518-2523 (the section header, `startAgentSession` and `sendAgentPrompt`; `endAgentSession` stays) with:

```ts
// ==== AI Assist (ACP chat sessions) ====

/** One choice of a session option. */
export interface ConfigChoice {
  value: string;
  name: string;
  description: string | null;
}

/** A session option the agent reports, such as the model or the effort level. */
export interface ConfigOption {
  id: string;
  name: string;
  /** `model`, `thought_level`, `mode`, `model_config`, or another agent value. */
  category: string | null;
  currentValue: string;
  choices: ConfigChoice[];
}

export interface AgentSessionStarted {
  sessionId: string;
  configOptions: ConfigOption[];
}

/** A text resource sent with a prompt, such as a request definition. */
export interface PromptResourceDto {
  uri: string;
  mimeType: string | null;
  text: string;
}

export const startAgentSession = (agentConfigId: string, cwd: string, collection: string) =>
  invoke<AgentSessionStarted>('start_agent_session', { agentConfigId, cwd, collection });

/** Resolves with the stop reason. A stopped turn resolves with `cancelled`. */
export const sendAgentPrompt = (
  sessionId: string,
  prompt: string,
  resources?: PromptResourceDto[],
) => invoke<string>('send_agent_prompt', { sessionId, prompt, resources: resources ?? null });

/** Asks the agent to stop the running turn. The session stays open. */
export const cancelAgentPrompt = (sessionId: string) =>
  invoke<void>('cancel_agent_prompt', { sessionId });

/** Changes one session option and resolves with the agent's new option list. */
export const setAgentConfigOption = (sessionId: string, configId: string, value: string) =>
  invoke<ConfigOption[]>('set_agent_config_option', { sessionId, configId, value });
```

After `onAgentSessionFailed` (line 2636 as of the start of this plan), add:

```ts
export type AgentToolCallStatus = 'pending' | 'in_progress' | 'completed' | 'failed';

export interface AgentToolActivityEvent {
  type: 'acpToolActivity';
  session_id: string;
  call_id: string;
  title: string;
  status: AgentToolCallStatus;
}

export const onAgentToolActivity = (
  handler: (event: AgentToolActivityEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentToolActivityEvent>('agent-session-tool-activity', (e) => handler(e.payload));

/** A config option as the event carries it. Keys are snake_case, like every event field. */
export interface AgentConfigOptionPayload {
  id: string;
  name: string;
  category: string | null;
  current_value: string;
  choices: ConfigChoice[];
}

export interface AgentConfigOptionsEvent {
  type: 'acpConfigOptionsChanged';
  session_id: string;
  options: AgentConfigOptionPayload[];
}

export const onAgentConfigOptions = (
  handler: (event: AgentConfigOptionsEvent) => void,
): Promise<UnlistenFn> =>
  listen<AgentConfigOptionsEvent>('agent-session-config-options', (e) => handler(e.payload));

export interface AgentUsageEvent {
  type: 'acpUsage';
  session_id: string;
  used: number;
  size: number;
  /** Cumulative session cost in US dollars, or null when not reported in dollars. */
  cost_usd: number | null;
}

export const onAgentUsage = (handler: (event: AgentUsageEvent) => void): Promise<UnlistenFn> =>
  listen<AgentUsageEvent>('agent-session-usage', (e) => handler(e.payload));

/** Converts the event's options to the camelCase shape the commands return. */
export function configOptionsFromEvent(options: AgentConfigOptionPayload[]): ConfigOption[] {
  return options.map((option) => ({
    id: option.id,
    name: option.name,
    category: option.category,
    currentValue: option.current_value,
    choices: option.choices,
  }));
}
```

In `src/components/request/AgentChatPanel.tsx`, replace line 57 with:

```tsx
      const { sessionId: newSessionId } = await startAgentSession(
        selectedAgentConfigId,
        cwd,
        collectionName,
      );
```

- [ ] **Step 7: Verify**

Run: `cargo check --workspace --all-targets -j4`
Expected: PASS.

Run: `yarn tsc --noEmit`
Expected: PASS.

Run: `yarn check`
Expected: PASS (test files are excluded from Biome by `biome.json`; `tauri-api.ts` and `AgentChatPanel.tsx` are checked).

For the user to run: `cargo test -p rocket -j4 --lib acp_session_dto` and `yarn test agent-session-api AgentChatPanel agent-session-event-bridge`.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with the message `feat: add agent cancel, config option and prompt resource commands`, staging exactly:
- `src-tauri/src/commands/acp_session_dto.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/acp_sessions.rs`, `src-tauri/src/lib.rs`
- `src/lib/tauri-api.ts`, `src/lib/queries/__tests__/agent-session-api.test.ts`
- `src/components/request/AgentChatPanel.tsx`, `src/components/request/__tests__/AgentChatPanel.test.tsx`

---

## Manual checklist (for the user, after all three tasks)

- Start an AI Assist session from a request's Scripts tab with the real `claude-agent-acp` agent; it still chats as before.
- With a long answer, confirm no timeout fires while text keeps streaming for more than two minutes in total.
- Option changes, Stop and prompt resources have no UI until Plans 05 and 06; check them through the Rust and Vitest tests above.

## Next Plan

**Plan 02 — Isolation and lifecycle** (`docs/superpowers/plans/workspace-ai-assistant/2026-10-09-workspace-ai-assistant-plan-02-isolation-and-lifecycle.md`, to be written). It first confirms the adapter facts listed in the spec ("Facts taken from reading `@agentclientprotocol/claude-agent-acp@0.88.0`"), then adds the pure `agent_isolation::isolation_meta` and passes its value where `AcpSessionService::start_session` now passes `None` as `meta` (this plan's `AcpAgentClient` already forwards it as `_meta` of `session/new`, proven by `acp_agent_client_start_session_passes_meta_through_to_new_session_request`). It adds `SessionCleanup` to `AcpSessionService::new` after `event_publisher`, calls it on every end path, including this plan's `end_idle_session` and the failed-prompt branch of `send_prompt`, creates and removes the per-session scratch and `CLAUDE_CONFIG_DIR` directories in `src-tauri`, and adds `end_stale_assistant_sessions`. Plan 03's outline resource is added after `prompt_parts` builds the user's parts, so the 8-resource limit applies only to user chips.

## Post-Implementation Review

After all three tasks are checked off and `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check` are green, dispatch a review subagent:

```
Agent({
  subagent_type: "general-purpose",
  model: "opus",
  description: "Plan 01 ACP client upgrade review",
  prompt: "Review the full diff this plan produced (the three commits of
    docs/superpowers/plans/workspace-ai-assistant/2026-10-09-workspace-ai-assistant-plan-01-acp-client-upgrade.md;
    write it to a file with `git diff <base>..HEAD > /tmp/plan01.diff` and review that file).
    Read that plan file, docs/superpowers/plans/workspace-ai-assistant/00-plan-index.md and the
    spec section 'ACP client upgrade' in docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md
    in full first. You have authority to fix what you find directly (edit files, re-run
    `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check`, commit
    via the dev-workflow-skills:1-git-commit skill with explicit paths). Do not run tests unless
    the user asks; never run `cargo test --workspace`; always pass -j4 to cargo. Check:
    (1) Interface gaps against the index's locked Plan 01 contracts — AcpUpdate variants and
    fields, ToolCallStatus, ConfigOption/ConfigChoice (defined in rocket_shared::acp and
    re-exported from rocket_acp, per the plan's 'Interface deviations'), PromptCapabilities,
    SessionInfo, PromptPart, the six AcpSessionClient methods, the three DomainEvent variants
    and their channel names, prompt_idle_timeout, the four DTOs, the four commands, and the
    TypeScript wrappers and listeners. Names, field names and parameter order must match.
    (2) Behaviour: cancel never takes the prompt lock; a cancelled stop reason publishes
    AcpSessionFinished and does not end the session; the idle timer restarts on every update
    and still kills a silent session; every update event precedes the terminal event; a
    permission request is answered at once with a reject option or Cancelled; resources fall
    back to text without embeddedContext; non-object meta is refused before spawning.
    (3) Code quality — comments are short full sentences ending with punctuation, no
    leftover TODO or placeholder code, no unwrap calls in production paths, no duplicated
    session lookup (AcpAgentClient::running is used by send_prompt, cancel and
    set_config_option).
    (4) DDD boundaries — rocket-acp has no dependency on agent-client-protocol, Tauri or
    DomainEvent; rocket-shared has no workspace dependency; camelCase serde appears only in
    src-tauri/src/commands/acp_session_dto.rs; rocket-app does no I/O.
    Report what you found and what you fixed, in under 400 words."
})
```
