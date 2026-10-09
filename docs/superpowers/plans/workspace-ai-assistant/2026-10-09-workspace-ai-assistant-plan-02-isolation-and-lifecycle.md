# Workspace AI Assistant — Plan 02: Isolation and Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Start every ACP agent session isolated (no user/project/local settings, no built-in tools, only Rocket's MCP tools, an empty per-session `CLAUDE_CONFIG_DIR` and an empty per-session working directory), and make sure every session end path (End session, idle timeout, failed prompt, app exit, stale-session sweep) releases the session's MCP server, its test-result cache and its scratch directories exactly once.

**Architecture:** `rocket-app` gains a pure `agent_isolation` module (the `_meta` JSON, the env var name, the Rocket system prompt) and a `SessionCleanup` port that `AcpSessionService` calls exactly once per tracked session on every end path. `src-tauri` owns all I/O: `SessionScratch` creates and removes the per-session directories, `SessionResourceRegistry` holds them per session, and `TauriSessionCleanup` implements the port (MCP registry entry, `McpToolService::forget_session`, scratch removal). A new `end_stale_assistant_sessions` command lets the webview end every session it no longer owns after a reload.

**Tech Stack:** Rust (`rocket-app`, `src-tauri`), `serde_json`, `tokio`, `uuid`, Tauri 2 (`tauri::test::MockRuntime` in tests), TypeScript (`src/lib/tauri-api.ts`), Vitest.

**Spec:** [`docs/superpowers/specs/2026-10-09-workspace-ai-assistant-design.md`](../../specs/2026-10-09-workspace-ai-assistant-design.md), sections "Agent isolation (the token fix)" and "Safety and error handling". Locked cross-plan contracts: [`00-plan-index.md`](00-plan-index.md).

## Global Constraints

- Always pass `-j4` to every `cargo` invocation.
- The agent runs only `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check`. Test commands below are shown **for the user to run**. Never run `cargo test --workspace`.
- `cargo check --workspace --all-targets -j4` and `yarn tsc --noEmit` stay green at the end of every task.
- Rust: no unwrap calls in production code. Test code uses `.expect("message")`. Locks use `.lock().unwrap_or_else(PoisonError::into_inner)` in production code, like `src-tauri/src/mcp/registry.rs`.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs. This plan adds no DTO.
- `rocket-app` does no I/O: no `std::fs`, no `std::env::temp_dir` there. Directory work lives in `src-tauri`.
- `isolation_meta` returns exactly the JSON locked in the plan index. Do not add keys (for example `persistSession`, `skills`, `plugins`) in this plan.
- The MCP server name stays `"rocket"`, so the allowed-tools pattern is exactly `"mcp__rocket__*"`.
- The isolation env var is exactly `"CLAUDE_CONFIG_DIR"`.
- Scratch directories live under `std::env::temp_dir().join("rocket-agent-sessions")`, one `<uuid>/` per session with `cwd/` and `config/` inside, created with mode `0o700` on Unix.
- Commits: conventional commits, created through the `dev-workflow-skills:1-git-commit` skill, never a freeform `git commit -m`. Stage with explicit paths only (peer sessions share this repo's index).
- Code comments: short, full sentences that end with a punctuation mark.

## Verified facts

Read on 2026-10-09 in this worktree (before Plan 01 lands; line numbers will shift a little after Plan 01).

1. `crates/rocket-app/src/acp_session_service.rs:16-22` — `AcpSessionService { session_client, event_publisher, agent_config_service, collection_repo, prompt_timeout }`; `:14-15` doc says the service "keeps no session map of its own".
2. `acp_session_service.rs:41-54` `new(session_client, event_publisher, agent_config_service, collection_repo)`; `:59-73` `with_prompt_timeout(..., prompt_timeout)`.
3. `acp_session_service.rs:101-165` `start_session(&self, agent_config_id, cwd, collection, mcp_http: Option<McpHttpServerCredentials>)`; `:113` builds `env` as `vec![(config.credential_env_var.clone(), credential)]`; `:126` and `:140` name the MCP server `"rocket"`; `:156-159` is the only call to the client's `start_session`.
4. `acp_session_service.rs:215-221` — the client-error branch of `send_prompt` publishes `AcpSessionFailed` and does **not** end the session or sweep anything. `:222-236` — the timeout branch kills via `let _ = self.session_client.end_session(session_id).await;` (`:226`) and also sweeps nothing.
5. `acp_session_service.rs:242-244` `end_session` and `:250-252` `end_all_sessions` only delegate to the client.
6. `acp_session_service.rs:430-492` — test `FakeSessionClient` (fields `start_should_fail`, `prompt_chunks`, `prompt_stop_reason`, `prompt_should_fail`, `prompt_delay`, `end_session_called`, `end_all_sessions_called`; `start_session` returns `"session-1"` at `:465`); `:877-907` — `CapturingSessionClient { captured_servers }`, built at `:790`, `:830`, `:915`.
7. Test call sites in `acp_session_service.rs`: `AcpSessionService::new(` at `:528, :553, :594, :611, :636, :657, :684, :750, :771, :795, :834, :857, :919`; `with_prompt_timeout(` at `:715`; `.start_session(` at `:536, :619, :644, :667, :779, :803, :842, :865, :927`.
8. `crates/rocket-infra/src/acp_agent_client.rs:430-435` — a failed `send_request` calls `fail_and_remove` (`:485-495`), which kills and forgets the session, then returns `DomainError::Internal`. An unknown id returns `DomainError::NotFound` (`:405-411`).
9. `acp_agent_client.rs:450-454` — `end_all_sessions` sets `shutting_down`, after which `start_session` is refused. It is therefore unusable for the webview stale-session sweep.
10. `acp_agent_client.rs:198-200` — `env` is applied with `.envs(...)` on top of the inherited environment, so a pushed `CLAUDE_CONFIG_DIR` overrides any value the user's shell set.
11. `src-tauri/src/commands/acp_sessions.rs:54-112` `start_agent_session_inner` (generic over `R: tauri::Runtime`); `:73` mints the pre-handshake `mcp_session_id`; `:89-91` calls `svc.start_session(&agent_config_id, &cwd, &collection, mcp_credentials)`; `:99` registers the handle under the real ACP id.
12. `acp_sessions.rs:123-142` `end_agent_session` is the only place that sweeps (`mcp_registry.end_session` at `:134`, `mcp_tool_svc.forget_session(&session_id)` at `:140`).
13. `src-tauri/src/mcp/tool_server.rs:408-411,422` — `spawn_mcp_http_server(app_handle, session_id)` builds `RocketMcpToolServer::new(app_handle, session_id)`, and every tool call passes `&self.session_id` (for example `:247`, `:334`), which is the **pre-handshake** id. `crates/rocket-app/src/mcp_tool_service.rs:85` keys `test_result_cache` by that id and `:393-398` `forget_session` filters on it. So today's `forget_session(<ACP id>)` at `acp_sessions.rs:140` never matches a cached entry. This plan forgets both ids as an interim measure. Plan 03 supersedes it with `McpSessionBinding`, which tags every tool call made after the handshake with the real ACP id; forgetting the pre-handshake id then stays as a harmless backstop, and this plan's cleanup code and its test do not change.
14. `src-tauri/src/mcp/registry.rs:59-68` `end_session` (sync, no-op when absent) and `:75-83` `shutdown_all`.
15. `src-tauri/src/lib.rs:428-433` builds `acp_session_svc`; `:557-566` builds `mcp_tool_svc: Arc<McpToolService>`; `:648` builds `mcp_server_registry`; `:662` and `:675-676` manage them; exit sweeps at `:115-131` (signal listener) and `:931-956` (`RunEvent::Exit`); commands registered at `:920-922`; `:6-7` declare `pub mod commands; pub mod mcp;`. `acp_collection_repo` (`:424`) is only `Arc::clone`d, so it is still in scope at `:648`. `app_handle` is bound at `:238`.
16. `src-tauri/Cargo.toml:58` `uuid.workspace = true` and `:48` `serde_json.workspace = true` are regular dependencies; `:105` `tempfile = "3"` is a dev-dependency only. So `SessionScratch` uses `std::fs` and `uuid`, and only tests use `tempfile`.
17. `crates/rocket-app/Cargo.toml:20` has `serde_json`; `crates/rocket-app/src/lib.rs:53` re-exports `AcpSessionService, McpHttpServerCredentials`.
18. `src-tauri/tests/acp_mcp_start_agent_session.rs:196-243` `build_fixture` returns `(CollectionService, Arc<McpServerRegistry>, AcpSessionService, AppHandle<MockRuntime>, TempDir)`; `:230-235` builds the service; three tests call `start_agent_session_inner` at `:256`, `:295`, `:329`; `:153-189` is its `FakeSessionClient { should_fail, real_session_id, captured_servers }`.
19. `src/lib/tauri-api.ts:2518-2526` holds the AI Assist wrappers (`startAgentSession`, `sendAgentPrompt`, `endAgentSession`); `src/lib/queries/__tests__/agent-session-api.test.ts` tests them with a mocked `invoke`.
20. Adapter `@agentclientprotocol/claude-agent-acp@0.88.0` (`dist/acp-agent.js`): `:7091-7107` a `_meta.systemPrompt` **object** is spread into the `claude_code` preset (so `{append}` is kept), while a **string** replaces the whole preset; `:7115-7117` bypass is disabled only when `_meta.claudeCode.options.allowDangerouslySkipPermissions` is exactly `false`; `:7123` `userProvidedOptions = _meta.claudeCode.options`; `:7144-7145` `tools` comes from `userProvidedOptions.tools`; `:7222-7225` options start with `settingSources: ["user","project","local"]` and then spread `...userProvidedOptions`; `:7240-7241` `allowDangerouslySkipPermissions` and `permissionMode` are set after the spread (adapter-controlled); `:4373-4385` `usage_update` carries `used`, `size` and `cost.amount`; `:333-373` lists `allowedTools`, `settingSources`, `strictMcpConfig`, `tools`, `skills` and `plugins` as SDK options.
21. Adapter `dist/paths.js:4-6` — `claudeConfigDir()` is `process.env.CLAUDE_CONFIG_DIR ?? ~/.claude`; `dist/settings.js:79-82` — the adapter's own settings manager reads `<configDir>/settings.json`, `<cwd>/.claude/settings.json`, `<cwd>/.claude/settings.local.json` and the managed-policy file. With an empty config dir and an empty cwd only managed (enterprise) policy can still apply.

## Assumed state from Plan 01 (verify before starting)

Plan 01 is written in parallel and lands first. This plan assumes exactly the plan index's contracts. **The first step of Tasks 1 and 2 reads the real files.** If a name differs, adapt only that name; the logic does not change.

- `AcpSessionClient::start_session(&self, command, args, cwd, env, mcp_servers, meta: Option<serde_json::Value>) -> DomainResult<SessionInfo>`. `rocket_acp::{SessionInfo, PromptPart}` are re-exported at the crate root, and `SessionInfo` has a public `session_id: String`.
- `AcpSessionService::start_session(&self, agent_config_id, cwd, collection, mcp_http) -> DomainResult<SessionInfo>`, calling the client with `meta = None`.
- `AcpSessionService::send_prompt` ends in `match outcome { Some(Ok(stop_reason)) => ..., Some(Err(e)) => { /* publishes AcpSessionFailed */ }, None => ... }` (the "error from the client" branch is `Some(Err(e))`), and its idle-timeout branch returns `Err(self.end_idle_session(session_id).await)`. The private helper `end_idle_session` contains `let _ = self.session_client.end_session(session_id).await;`.
- The test seam is `AcpSessionService::with_prompt_idle_timeout(session_client, event_publisher, agent_config_service, collection_repo, prompt_idle_timeout)` and the field is `prompt_idle_timeout` (Plan 01 renamed both; `with_prompt_timeout` no longer exists). The constant is `DEFAULT_PROMPT_IDLE_TIMEOUT`.
- Plan 01's test module already has helpers `hi() -> Vec<PromptPart>` and `service_with(client, &publisher, idle) -> AcpSessionService` (which calls `with_prompt_idle_timeout`), plus `FakeEventPublisher` and `SharedEventPublisher`. This plan's own helper is therefore named `service_with_cleanup`, so the two do not collide.
- `start_agent_session_inner` returns `Result<SessionInfo, DomainError>` (`use rocket_acp::SessionInfo;` is imported in `acp_sessions.rs`), and the `start_agent_session` command converts with `.map(AgentSessionStartedDto::from)`. The DTOs live in `src-tauri/src/commands/acp_session_dto.rs` and are imported with `use crate::commands::acp_session_dto::{prompt_parts, AgentSessionStartedDto, ConfigOptionDto, PromptResourceDto};`.
- The test fakes in `acp_session_service.rs` and `acp_mcp_start_agent_session.rs` already implement Plan 01's trait methods (`cancel`, `set_config_option`). This plan only adds fields and lines to them.

## Interface additions (no renames)

Every name in the index's Plan 02 contract is used unchanged. These are additions the index does not mention:

- `rocket_app::agent_isolation::{SessionIsolation, ROCKET_ASSISTANT_SYSTEM_PROMPT, ROCKET_MCP_SERVER_NAME, ROCKET_MCP_TOOL_PATTERN}`.
- `AcpSessionService::start_session` gains a trailing `isolation: Option<SessionIsolation>` parameter. This is how the command layer's scratch paths reach the service, which builds `env` and `meta` itself.
- `rocket_app::NoopSessionCleanup` (unit struct, the test and fallback implementation).
- `AcpSessionService::end_tracked_sessions(&self) -> usize`, used by `end_stale_assistant_sessions`, which returns that count (`number` in TypeScript).
- `src-tauri`: `agent_session::scratch::SessionScratch`, `agent_session::cleanup::{SessionResources, SessionResourceRegistry, TauriSessionCleanup}`; `start_agent_session_inner` gains `resources: &SessionResourceRegistry` after `registry`.

## Review Focus

- **Two end paths racing for one session must run cleanup once.** For example, End session after a failed prompt already ended it, or a second `end_session` call. `AcpSessionService` removes the id from its live set under a lock, and only the remover runs cleanup. Covered in Task 1 by `end_session_runs_cleanup_once_for_a_started_session` and `failed_prompt_runs_cleanup_once_and_kills_the_session`.
- **A prompt rejected as `InvalidInput` (for example, Plan 01's prompt-part capability check) must not end the session.** Only other errors end it. Covered in Task 1 by `invalid_input_prompt_error_keeps_the_session_and_runs_no_cleanup`.
- **A session start that fails after the scratch directories and the MCP server exist must leave nothing behind.** Covered in Task 2 by the integration test `session_start_failure_removes_the_scratch_directories` (dirs gone, registry empty), next to the existing `session_start_failure_shuts_down_the_already_spawned_mcp_server`.
- **The test-result cache is keyed by the pre-handshake MCP id, not the ACP id (Verified fact 13).** Cleanup must forget both until Plan 03's `McpSessionBinding` moves the cache to the real id; the second forget then stays as a backstop. Covered in Task 2 by `on_session_ended_forgets_both_ids_shuts_the_server_and_removes_scratch`.
- **The stale-session sweep must not put the client into shutdown mode (Verified fact 9).** After the sweep a new session must still start. Covered in Task 3 by `end_tracked_sessions_ends_every_tracked_session_without_shutting_the_client_down`.

---

### Task 1: `rocket-app` — `agent_isolation`, `SessionCleanup` and lifecycle tracking in `AcpSessionService`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/agent_isolation.rs`
- Modify: `crates/rocket-app/src/lib.rs:4` (add `pub mod agent_isolation;` after it) and `:53` (re-exports)
- Modify: `crates/rocket-app/src/acp_session_service.rs` (struct `:16-22`, constructors `:41-73`, `start_session` `:101-165`, `send_prompt` error and timeout branches `:215-236`, `end_session` `:242-244`, `end_all_sessions` `:250-252`, tests `:255-959`)
- Modify: `crates/rocket-app/CLAUDE.md` (the `AcpSessionService` row of the Public Types table)
- Modify: `src-tauri/src/commands/acp_sessions.rs:89-91` (pass `None` for `isolation`)
- Modify: `src-tauri/src/lib.rs:428-433` (pass `NoopSessionCleanup` for now)
- Modify: `src-tauri/tests/acp_mcp_start_agent_session.rs:230-235` (pass `NoopSessionCleanup`)

**Interfaces:**
- Consumes: `AcpSessionClient` after Plan 01 (`start_session(..., meta: Option<serde_json::Value>) -> DomainResult<SessionInfo>`, `end_session`, `end_all_sessions`).
- Produces:
  ```rust
  // crates/rocket-app/src/agent_isolation.rs
  pub const ISOLATION_ENV_CONFIG_DIR: &str = "CLAUDE_CONFIG_DIR";
  pub const ROCKET_MCP_SERVER_NAME: &str = "rocket";
  pub const ROCKET_MCP_TOOL_PATTERN: &str = "mcp__rocket__*";
  pub const ROCKET_ASSISTANT_SYSTEM_PROMPT: &str;
  pub fn isolation_meta(system_prompt_append: &str) -> serde_json::Value;
  pub struct SessionIsolation { pub config_dir: String, pub system_prompt_append: String }
  impl SessionIsolation {
      pub fn new(config_dir: impl Into<String>) -> Self;
      pub fn meta(&self) -> serde_json::Value;
      pub fn env_entry(&self) -> (String, String);
  }
  // crates/rocket-app/src/acp_session_service.rs
  pub trait SessionCleanup: Send + Sync { fn on_session_ended(&self, session_id: &str); }
  pub struct NoopSessionCleanup;
  impl AcpSessionService {
      pub fn new(session_client: Box<dyn AcpSessionClient>, event_publisher: Box<dyn EventPublisher>,
                 cleanup: Arc<dyn SessionCleanup>, agent_config_service: Arc<AgentConfigService>,
                 collection_repo: Arc<dyn rocket_collection::CollectionRepository>) -> Self;
      pub fn with_prompt_idle_timeout(/* same five */, prompt_idle_timeout: Duration) -> Self;
      pub async fn start_session(&self, agent_config_id: &str, cwd: &str, collection: &str,
                                 mcp_http: Option<McpHttpServerCredentials>,
                                 isolation: Option<SessionIsolation>) -> DomainResult<SessionInfo>;
  }
  ```

- [ ] **Step 1: Read Plan 01's landed shapes**

Read `crates/rocket-app/src/acp_session_service.rs` and `crates/rocket-acp/src/session.rs` in full. Confirm the bullets in "Assumed state from Plan 01". Plan 01's tests call `service.send_prompt("session-1", hi())`; Step 2's `send_hi` helper builds the same `Vec<PromptPart>`. Also note `send_prompt_failure_publishes_failed_and_returns_the_error`: after this task a failed prompt also ends the session, so if that test asserts that `end_session` was not called, change that assertion to expect the call.

- [ ] **Step 2: Write the failing tests**

Create `crates/rocket-app/src/agent_isolation.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolation_meta_matches_the_locked_shape() {
        let meta = isolation_meta("Be brief.");
        assert_eq!(
            meta,
            serde_json::json!({
                "claudeCode": {
                    "options": {
                        "settingSources": [],
                        "strictMcpConfig": true,
                        "tools": [],
                        "allowedTools": ["mcp__rocket__*"],
                        "allowDangerouslySkipPermissions": false
                    }
                },
                "systemPrompt": { "append": "Be brief." }
            })
        );
    }

    #[test]
    fn system_prompt_is_an_append_object_never_a_bare_string() {
        // A bare string would replace the whole claude_code preset in the adapter.
        let meta = isolation_meta("x");
        assert!(meta["systemPrompt"].is_object());
        assert_eq!(meta["systemPrompt"]["append"], "x");
    }

    #[test]
    fn allowed_tools_pattern_follows_the_mcp_server_name() {
        assert_eq!(
            ROCKET_MCP_TOOL_PATTERN,
            format!("mcp__{ROCKET_MCP_SERVER_NAME}__*")
        );
    }

    #[test]
    fn default_prompt_names_rocket_and_its_tools_and_forbids_file_assumptions() {
        assert!(ROCKET_ASSISTANT_SYSTEM_PROMPT.contains("Rocket"));
        assert!(ROCKET_ASSISTANT_SYSTEM_PROMPT.contains("mcp__rocket__"));
        assert!(ROCKET_ASSISTANT_SYSTEM_PROMPT.contains("Do not assume that any files exist"));
    }

    #[test]
    fn session_isolation_builds_meta_and_the_config_dir_env_entry() {
        let isolation = SessionIsolation::new("/scratch/config");
        assert_eq!(isolation.system_prompt_append, ROCKET_ASSISTANT_SYSTEM_PROMPT);
        assert_eq!(isolation.meta(), isolation_meta(ROCKET_ASSISTANT_SYSTEM_PROMPT));
        assert_eq!(
            isolation.env_entry(),
            ("CLAUDE_CONFIG_DIR".to_string(), "/scratch/config".to_string())
        );
    }
}
```

Register the module in `crates/rocket-app/src/lib.rs` (alphabetical, after `pub mod agent_config_service;` at `:4`):

```rust
pub mod agent_config_service;
pub mod agent_isolation;
```

In `crates/rocket-app/src/acp_session_service.rs`, extend the test module:

1. Add imports at the top of `mod tests` (next to the existing `use` lines):

```rust
    use std::collections::VecDeque;

    use rocket_shared::events::NullEventPublisher;

    use crate::agent_isolation::{isolation_meta, SessionIsolation, ROCKET_ASSISTANT_SYSTEM_PROMPT};
```

2. Add two fields to `FakeSessionClient` (`:430-438`) and their defaults (`:439-451`):

```rust
        prompt_invalid_input: bool,
        start_ids: Arc<Mutex<VecDeque<String>>>,
```

```rust
                prompt_invalid_input: false,
                start_ids: Arc::new(Mutex::new(VecDeque::new())),
```

3. Add a helper impl right after the `Default` impl:

```rust
    impl FakeSessionClient {
        /// Returns the next queued session id, or "session-1" when none is queued.
        fn next_session_id(&self) -> String {
            self.start_ids
                .lock()
                .expect("lock start_ids")
                .pop_front()
                .unwrap_or_else(|| "session-1".to_string())
        }
    }
```

4. In `FakeSessionClient::start_session`, replace the `"session-1".to_string()` literal of the success branch with `self.next_session_id()`.

5. In `FakeSessionClient::send_prompt`, insert as the first statement of the body:

```rust
            if self.prompt_invalid_input {
                return Err(DomainError::InvalidInput("unsupported prompt part".to_string()));
            }
```

6. Make `CapturingSessionClient` (`:877-879`) record `env` and `meta`, and derive `Default`:

```rust
    #[derive(Default)]
    struct CapturingSessionClient {
        captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>>,
        captured_env: Arc<Mutex<Vec<(String, String)>>>,
        captured_meta: Arc<Mutex<Option<serde_json::Value>>>,
    }
```

In its `start_session`, rename the `_env` parameter to `env` and the `_meta` parameter to `meta`, and add before the existing `captured_servers` line:

```rust
            *self.captured_env.lock().expect("lock") = env.to_vec();
            *self.captured_meta.lock().expect("lock") = meta;
```

Append `..Default::default()` to the three `CapturingSessionClient { captured_servers: Arc::clone(&captured_servers) }` literals (`:790`, `:830`, `:915`), for example:

```rust
        let client = CapturingSessionClient {
            captured_servers: Arc::clone(&captured_servers),
            ..Default::default()
        };
```

7. Add the cleanup double and helpers after `SharedEventPublisher` (`:518-523`). The helper is named `service_with_cleanup` because Plan 01 already defines `service_with(client, &publisher, idle)` in this module:

```rust
    #[derive(Default)]
    struct RecordingCleanup {
        ended: Mutex<Vec<String>>,
    }
    impl RecordingCleanup {
        fn ended(&self) -> Vec<String> {
            self.ended.lock().expect("lock RecordingCleanup").clone()
        }
    }
    impl SessionCleanup for RecordingCleanup {
        fn on_session_ended(&self, session_id: &str) {
            self.ended
                .lock()
                .expect("lock RecordingCleanup")
                .push(session_id.to_string());
        }
    }

    fn noop_cleanup() -> Arc<dyn SessionCleanup> {
        Arc::new(NoopSessionCleanup)
    }

    fn service_with_cleanup(client: FakeSessionClient, cleanup: Arc<dyn SessionCleanup>) -> AcpSessionService {
        AcpSessionService::new(
            Box::new(client),
            Box::new(NullEventPublisher),
            cleanup,
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        )
    }

    async fn start(service: &AcpSessionService) {
        service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");
    }

    /// Sends one text prompt. Keep the argument form identical to the
    /// existing send_prompt tests in this module.
    async fn send_hi(service: &AcpSessionService, session_id: &str) -> DomainResult<String> {
        service
            .send_prompt(session_id, vec![rocket_acp::PromptPart::Text("hi".to_string())])
            .await
    }
```

8. Adapt every existing call: insert `noop_cleanup(),` as the third argument of every `AcpSessionService::new(` call and every `AcpSessionService::with_prompt_idle_timeout(` call in the module. After Plan 01 that is the 13 `new(` calls of Verified fact 7, the `with_prompt_idle_timeout(` call in `send_prompt_timeout_kills_the_session_and_publishes_failed`, and the one inside Plan 01's `service_with(client, &publisher, idle)` helper (whose own signature stays unchanged). Find them all with `grep -n "AcpSessionService::new(\|with_prompt_idle_timeout(" crates/rocket-app/src/acp_session_service.rs`. For example:

```rust
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(SharedEventPublisher(Arc::clone(&publisher))),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );
```

and append `None` (the `isolation` argument) to all 9 `.start_session(` calls, for example `.start_session("agent-1", "/tmp", "demo", None, None)`, and in the two calls that pass `Some(McpHttpServerCredentials { .. })` add `None,` after that argument.

9. Add the new tests at the end of `mod tests`:

```rust
    #[tokio::test]
    async fn start_session_with_isolation_passes_meta_and_the_config_dir_env() {
        let client = CapturingSessionClient::default();
        let captured_env = Arc::clone(&client.captured_env);
        let captured_meta = Arc::clone(&client.captured_meta);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(NullEventPublisher),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        service
            .start_session(
                "agent-1",
                "/scratch/cwd",
                "demo",
                None,
                Some(SessionIsolation::new("/scratch/config")),
            )
            .await
            .expect("start_session should succeed");

        let env = captured_env.lock().expect("lock").clone();
        assert!(
            env.iter().any(|(k, v)| k == "CLAUDE_CONFIG_DIR" && v == "/scratch/config"),
            "CLAUDE_CONFIG_DIR must point at the scratch config dir, got {env:?}"
        );
        assert!(
            env.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"),
            "the credential env var must still be passed, got {env:?}"
        );
        assert_eq!(
            *captured_meta.lock().expect("lock"),
            Some(isolation_meta(ROCKET_ASSISTANT_SYSTEM_PROMPT))
        );
    }

    #[tokio::test]
    async fn start_session_without_isolation_passes_no_meta_and_no_config_dir() {
        let client = CapturingSessionClient::default();
        let captured_env = Arc::clone(&client.captured_env);
        let captured_meta = Arc::clone(&client.captured_meta);
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(NullEventPublisher),
            noop_cleanup(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
        );

        service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect("start_session should succeed");

        assert!(captured_meta.lock().expect("lock").is_none());
        assert!(!captured_env
            .lock()
            .expect("lock")
            .iter()
            .any(|(k, _)| k == "CLAUDE_CONFIG_DIR"));
    }

    #[tokio::test]
    async fn end_session_runs_cleanup_once_for_a_started_session() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let service = service_with_cleanup(FakeSessionClient::default(), cleanup.clone());
        start(&service).await;

        service.end_session("session-1").await.expect("first end");
        let _ = service.end_session("session-1").await;

        assert_eq!(cleanup.ended(), vec!["session-1".to_string()]);
    }

    #[tokio::test]
    async fn end_session_on_an_untracked_id_still_calls_the_client_but_runs_no_cleanup() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());

        let _ = service.end_session("never-started").await;

        assert!(end_session_called.load(Ordering::SeqCst));
        assert!(cleanup.ended().is_empty());
    }

    #[tokio::test]
    async fn idle_timeout_runs_cleanup_once() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let client = FakeSessionClient {
            prompt_delay: Duration::from_millis(200),
            ..Default::default()
        };
        let service = AcpSessionService::with_prompt_idle_timeout(
            Box::new(client),
            Box::new(NullEventPublisher),
            cleanup.clone(),
            agent_config_service(),
            ConfigurableCollectionRepo::new(),
            Duration::from_millis(20),
        );
        start(&service).await;

        send_hi(&service, "session-1")
            .await
            .expect_err("a hung prompt must time out");
        let _ = service.end_session("session-1").await;

        assert_eq!(cleanup.ended(), vec!["session-1".to_string()]);
    }

    #[tokio::test]
    async fn failed_prompt_runs_cleanup_once_and_kills_the_session() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_should_fail: true,
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        start(&service).await;

        send_hi(&service, "session-1")
            .await
            .expect_err("a crashed prompt must fail");
        let _ = send_hi(&service, "session-1").await;
        let _ = service.end_session("session-1").await;

        assert!(end_session_called.load(Ordering::SeqCst));
        assert_eq!(cleanup.ended(), vec!["session-1".to_string()]);
    }

    #[tokio::test]
    async fn invalid_input_prompt_error_keeps_the_session_and_runs_no_cleanup() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            prompt_invalid_input: true,
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        start(&service).await;

        let err = send_hi(&service, "session-1")
            .await
            .expect_err("an invalid prompt must fail");
        assert!(matches!(err, DomainError::InvalidInput(_)));

        assert!(!end_session_called.load(Ordering::SeqCst));
        assert!(cleanup.ended().is_empty());
    }

    #[tokio::test]
    async fn end_all_sessions_runs_cleanup_for_every_tracked_session() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let client = FakeSessionClient {
            start_ids: Arc::new(Mutex::new(VecDeque::from(vec![
                "s-a".to_string(),
                "s-b".to_string(),
            ]))),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        start(&service).await;
        start(&service).await;

        service.end_all_sessions().await.expect("end_all_sessions");
        let _ = service.end_session("s-a").await;

        let mut ended = cleanup.ended();
        ended.sort();
        assert_eq!(ended, vec!["s-a".to_string(), "s-b".to_string()]);
    }

    #[tokio::test]
    async fn a_failed_start_tracks_nothing() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let client = FakeSessionClient {
            start_should_fail: true,
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());

        service
            .start_session("agent-1", "/tmp", "demo", None, None)
            .await
            .expect_err("spawn failure must propagate");
        service.end_all_sessions().await.expect("end_all_sessions");

        assert!(cleanup.ended().is_empty());
    }
```

- [ ] **Step 3: Confirm the tests fail to compile**

Run: `cargo check -p rocket-app --all-targets -j4`
Expected: FAIL — `cannot find function isolation_meta`, `cannot find trait SessionCleanup`, `NoopSessionCleanup` not found, and "this function takes 4 arguments but 5 were supplied" on `AcpSessionService::new` / `start_session`.

- [ ] **Step 4: Implement `agent_isolation.rs`**

Put this above the test module in `crates/rocket-app/src/agent_isolation.rs`:

```rust
//! Pure builders for an isolated agent session. The adapter reads these
//! values from `session/new` `_meta` and from the process environment. This
//! module does no I/O; `src-tauri` creates the directories it names.

/// The environment variable that points Claude Code at its config directory.
pub const ISOLATION_ENV_CONFIG_DIR: &str = "CLAUDE_CONFIG_DIR";

/// The name Rocket's MCP server is registered under in `session/new`.
pub const ROCKET_MCP_SERVER_NAME: &str = "rocket";

/// Claude Code names MCP tools `mcp__<server>__<tool>`. This pattern allows
/// every Rocket tool in advance, so none of them asks for permission.
pub const ROCKET_MCP_TOOL_PATTERN: &str = "mcp__rocket__*";

/// Appended to the adapter's `claude_code` system prompt preset.
pub const ROCKET_ASSISTANT_SYSTEM_PROMPT: &str = "You are Rocket's API assistant. \
You run inside Rocket, a desktop API client, and you help the user understand, write and fix \
HTTP requests, scripts and tests in their Rocket workspace. \
You have only the Rocket tools, whose names start with mcp__rocket__. \
You have no shell, no file system and no web access. \
Do not assume that any files exist, and do not try to read, write or list files. \
When you need workspace data, call a Rocket tool. \
When no Rocket tool can answer, say what you could not check instead of guessing.";

/// Builds the `_meta` object for `session/new`. `settingSources: []` drops
/// user, project and local settings, `tools: []` drops every built-in tool,
/// and `allowDangerouslySkipPermissions` must be the boolean `false`, because
/// the adapter disables bypass mode only for that exact value.
pub fn isolation_meta(system_prompt_append: &str) -> serde_json::Value {
    serde_json::json!({
        "claudeCode": {
            "options": {
                "settingSources": [],
                "strictMcpConfig": true,
                "tools": [],
                "allowedTools": [ROCKET_MCP_TOOL_PATTERN],
                "allowDangerouslySkipPermissions": false
            }
        },
        "systemPrompt": { "append": system_prompt_append }
    })
}

/// The per-session isolation inputs the command layer hands to
/// `AcpSessionService::start_session`. `config_dir` is an empty directory
/// that becomes the agent's `CLAUDE_CONFIG_DIR`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIsolation {
    pub config_dir: String,
    pub system_prompt_append: String,
}

impl SessionIsolation {
    /// Uses the default Rocket system prompt.
    pub fn new(config_dir: impl Into<String>) -> Self {
        Self {
            config_dir: config_dir.into(),
            system_prompt_append: ROCKET_ASSISTANT_SYSTEM_PROMPT.to_string(),
        }
    }

    /// The `_meta` value for `session/new`.
    pub fn meta(&self) -> serde_json::Value {
        isolation_meta(&self.system_prompt_append)
    }

    /// The `CLAUDE_CONFIG_DIR` entry for the agent's environment.
    pub fn env_entry(&self) -> (String, String) {
        (ISOLATION_ENV_CONFIG_DIR.to_string(), self.config_dir.clone())
    }
}
```

Extend the re-exports in `crates/rocket-app/src/lib.rs` (`:53`, keep any names Plan 01 added to that line):

```rust
pub use acp_session_service::{
    AcpSessionService, McpHttpServerCredentials, NoopSessionCleanup, SessionCleanup,
};
pub use agent_isolation::{
    isolation_meta, SessionIsolation, ISOLATION_ENV_CONFIG_DIR, ROCKET_ASSISTANT_SYSTEM_PROMPT,
};
```

- [ ] **Step 5: Implement `SessionCleanup` and tracking in `AcpSessionService`**

In `crates/rocket-app/src/acp_session_service.rs`:

1. Replace Plan 01's `std` imports (`use std::collections::HashMap;`, `use std::sync::Arc;`, `use std::time::Duration;`) with:

```rust
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
```

and add below the existing `use crate::agent_config_service::AgentConfigService;`:

```rust
use crate::agent_isolation::{SessionIsolation, ROCKET_MCP_SERVER_NAME};
```

2. Add the port and its no-op implementation above the struct:

```rust
/// Releases per-session resources that live outside this crate, such as the
/// MCP tool server, its caches and the session's scratch directories.
pub trait SessionCleanup: Send + Sync {
    /// Called exactly once per session on EVERY end path: end_session, idle
    /// timeout, failed prompt, end_all_sessions. Idempotent.
    fn on_session_ended(&self, session_id: &str);
}

/// A cleanup that does nothing. Used by tests and by callers that own no
/// per-session resources.
pub struct NoopSessionCleanup;

impl SessionCleanup for NoopSessionCleanup {
    fn on_session_ended(&self, _session_id: &str) {}
}
```

3. Replace the struct doc sentence "This service keeps no session map of its own. The session client owns session state and process lifecycle." with:

```rust
/// This service keeps only the set of live session ids, so it can run
/// `SessionCleanup` exactly once per session. The session client still owns
/// session state and process lifecycle.
```

and add two fields after `event_publisher`:

```rust
    cleanup: Arc<dyn SessionCleanup>,
    live_sessions: Mutex<HashSet<String>>,
```

4. Add `cleanup: Arc<dyn SessionCleanup>,` as the third parameter of both `new` and `with_prompt_idle_timeout`, pass it through in `new`'s delegation (`cleanup,` after `event_publisher,`), and initialise the fields in `with_prompt_idle_timeout`:

```rust
        Self {
            session_client,
            event_publisher,
            cleanup,
            live_sessions: Mutex::new(HashSet::new()),
            agent_config_service,
            collection_repo,
            prompt_idle_timeout,
        }
```

5. Add the private helpers inside `impl AcpSessionService`:

```rust
    fn track(&self, session_id: &str) {
        self.live_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id.to_string());
    }

    /// Forgets a tracked session and runs its cleanup. Only the first caller
    /// for an id finds it tracked, so cleanup runs exactly once per session.
    fn release(&self, session_id: &str) {
        let was_tracked = self
            .live_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session_id);
        if was_tracked {
            self.cleanup.on_session_ended(session_id);
        }
    }
```

6. In `start_session`, add the parameter `isolation: Option<SessionIsolation>,` after `mcp_http`, add this doc paragraph to its doc comment:

```rust
    /// `isolation`, when present, starts the agent isolated: its `_meta`
    /// comes from `SessionIsolation::meta`, and `CLAUDE_CONFIG_DIR` points at
    /// the caller's empty scratch directory. `cwd` should then be an empty
    /// scratch directory too.
```

make `env` mutable and build `meta` right after it:

```rust
        let mut env = vec![(config.credential_env_var.clone(), credential)];
        let meta = isolation.as_ref().map(|isolation| {
            env.push(isolation.env_entry());
            isolation.meta()
        });
```

replace both `name: "rocket".to_string(),` lines with `name: ROCKET_MCP_SERVER_NAME.to_string(),`, pass `meta` instead of Plan 01's `None` to the client, and track the new id right after the client call:

```rust
        let info = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &mcp_servers, meta)
            .await?;
        self.track(&info.session_id);
```

(Keep Plan 01's `AcpSessionStarted` publish and return after these lines.)

7. In `send_prompt`, in the `Some(Err(e)) => { ... }` arm of the final `match outcome` (the error returned by the client), insert before the `AcpSessionFailed` publish:

```rust
                // A prompt the agent could not take (InvalidInput) leaves the
                // session usable. Any other failure means the session is gone
                // or broken, so end it and release its resources.
                if !matches!(e, DomainError::InvalidInput(_)) {
                    let _ = self.session_client.end_session(session_id).await;
                    self.release(session_id);
                }
```

For the idle-timeout branch, edit Plan 01's private `end_idle_session` helper (the `select!` arm only calls it): right after its `let _ = self.session_client.end_session(session_id).await;`, add:

```rust
        self.release(session_id);
```

Add to `send_prompt`'s doc comment: `/// A failed prompt also ends the session, unless the error is InvalidInput.`

8. Replace `end_session` and `end_all_sessions`:

```rust
    /// Ends the session and kills its agent process, then releases its
    /// resources once. No event is published. An unknown or already-ended
    /// session id returns the client's error and runs no cleanup.
    pub async fn end_session(&self, session_id: &str) -> DomainResult<()> {
        let result = self.session_client.end_session(session_id).await;
        self.release(session_id);
        result
    }

    /// Kills every agent session's process for app exit, then releases each
    /// tracked session's resources. The client refuses new sessions
    /// afterwards. No event is published.
    pub async fn end_all_sessions(&self) -> DomainResult<()> {
        let ended: Vec<String> = self
            .live_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .collect();
        let result = self.session_client.end_all_sessions().await;
        for session_id in &ended {
            self.cleanup.on_session_ended(session_id);
        }
        result
    }
```

- [ ] **Step 6: Adapt the callers outside this crate**

`src-tauri/src/commands/acp_sessions.rs:89-91` — pass no isolation yet (Task 2 turns it on):

```rust
    let result = svc
        .start_session(&agent_config_id, &cwd, &collection, mcp_credentials, None)
        .await;
```

`src-tauri/src/lib.rs:428-433` — insert the no-op cleanup as the third argument:

```rust
            let acp_session_svc = rocket_app::AcpSessionService::new(
                Box::new(rocket_infra::AcpAgentClient::new()),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                Arc::new(rocket_app::NoopSessionCleanup),
                acp_agent_config_svc,
                Arc::clone(&acp_collection_repo),
            );
```

`src-tauri/tests/acp_mcp_start_agent_session.rs:230-235`:

```rust
    let acp_session_svc = AcpSessionService::new(
        Box::new(client),
        Box::new(NullEventPublisher),
        Arc::new(rocket_app::NoopSessionCleanup),
        agent_config_service(),
        collection_repo,
    );
```

In `crates/rocket-app/CLAUDE.md`, append to the end of the `AcpSessionService` row's Purpose cell: ` Tracks live session ids and calls the injected SessionCleanup exactly once per session on every end path (end_session, idle timeout, failed prompt except InvalidInput, end_all_sessions). start_session takes an optional SessionIsolation (agent_isolation.rs: _meta options, CLAUDE_CONFIG_DIR, Rocket system prompt).`

- [ ] **Step 7: Verify**

Run: `cargo check --workspace --all-targets -j4` — expected: green.
Run: `yarn tsc --noEmit` — expected: green (no TypeScript changed).

For the user to run:

```bash
cargo test -p rocket-app agent_isolation -j4
cargo test -p rocket-app acp_session_service -j4
cargo test -p rocket --test acp_mcp_start_agent_session -j4
```

Expected: all pass, including the 14 new tests (5 in agent_isolation, 9 in acp_session_service).

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-app/src/agent_isolation.rs crates/rocket-app/src/lib.rs \
        crates/rocket-app/src/acp_session_service.rs crates/rocket-app/CLAUDE.md \
        src-tauri/src/commands/acp_sessions.rs src-tauri/src/lib.rs \
        src-tauri/tests/acp_mcp_start_agent_session.rs
```

Use the `dev-workflow-skills:1-git-commit` skill. Message: `feat: add agent isolation options and session cleanup port`.

---

### Task 2: `src-tauri` — scratch directories, `TauriSessionCleanup`, isolated session start

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `src-tauri/src/agent_session/mod.rs`
- Create: `src-tauri/src/agent_session/scratch.rs`
- Create: `src-tauri/src/agent_session/cleanup.rs`
- Modify: `src-tauri/src/lib.rs:6-7` (module), `:428-433` (move and rewire `acp_session_svc`), `:641-648` (registry comment and new registry), `:662-676` (managed state), `:126-131` and `:951-955` (exit backstop)
- Modify: `src-tauri/src/commands/acp_sessions.rs:1-142` (`start_agent_session`, `start_agent_session_inner`, `end_agent_session`)
- Modify: `src-tauri/tests/acp_mcp_start_agent_session.rs` (fixture, fake, three existing calls, two new tests)

**Interfaces:**
- Consumes: `rocket_app::{SessionCleanup, SessionIsolation, McpToolService}` (Task 1), `McpServerRegistry::{end_session, register}`, `McpToolService::forget_session(&self, &str)`.
- Produces:
  ```rust
  // src-tauri/src/agent_session/scratch.rs
  pub const SCRATCH_PARENT_DIR: &str = "rocket-agent-sessions";
  pub struct SessionScratch;   // removes its directories on Drop
  impl SessionScratch {
      pub fn create() -> std::io::Result<Self>;
      pub fn create_in(parent: &Path) -> std::io::Result<Self>;
      pub fn root(&self) -> &Path; pub fn cwd(&self) -> &Path; pub fn config_dir(&self) -> &Path;
      pub fn isolation(&self) -> Result<(String, SessionIsolation), DomainError>;  // (cwd, isolation)
  }
  // src-tauri/src/agent_session/cleanup.rs
  pub struct SessionResources { pub scratch: SessionScratch, pub mcp_session_id: Option<String> }
  pub struct SessionResourceRegistry;
  impl SessionResourceRegistry {
      pub fn new() -> Self; pub fn register(&self, session_id: String, resources: SessionResources);
      pub fn take(&self, session_id: &str) -> Option<SessionResources>;
      pub fn clear_all(&self); pub fn len(&self) -> usize; pub fn is_empty(&self) -> bool;
  }
  pub struct TauriSessionCleanup;
  impl TauriSessionCleanup {
      pub fn new(mcp_registry: Arc<McpServerRegistry>, mcp_tool_svc: Arc<McpToolService>,
                 resources: Arc<SessionResourceRegistry>) -> Self;
      pub fn with_cache_forgetter(mcp_registry: Arc<McpServerRegistry>, resources: Arc<SessionResourceRegistry>,
                 forget_cache: impl Fn(&str) + Send + Sync + 'static) -> Self;
  }
  impl SessionCleanup for TauriSessionCleanup;
  // src-tauri/src/commands/acp_sessions.rs
  pub async fn start_agent_session_inner<R: tauri::Runtime>(agent_config_id: String, _requested_cwd: String,
      collection: String, app_handle: tauri::AppHandle<R>, collection_svc: &CollectionService,
      registry: &McpServerRegistry, resources: &SessionResourceRegistry, svc: &AcpSessionService)
      -> Result<SessionInfo, DomainError>;   // Plan 01's return type, unchanged
  ```

- [ ] **Step 1: Read Plan 01's landed command shapes**

Read `src-tauri/src/commands/acp_sessions.rs` and `src-tauri/tests/acp_mcp_start_agent_session.rs` in full. Confirm the return type of `start_agent_session_inner` and the conversion it uses (see "Assumed state from Plan 01"), and how the three existing tests read the session id from its result.

- [ ] **Step 2: Write the failing unit tests**

Create `src-tauri/src/agent_session/mod.rs`:

```rust
//! Per-session resources of isolated agent sessions: scratch directories and
//! the `SessionCleanup` implementation that releases them.

pub mod cleanup;
pub mod scratch;
```

Register it in `src-tauri/src/lib.rs` next to `pub mod mcp;` (`:6-7`), `pub` so integration tests can use it:

```rust
pub mod agent_session;
pub mod commands;
pub mod mcp;
```

Create `src-tauri/src/agent_session/scratch.rs` with only its tests:

```rust
use std::path::{Path, PathBuf};

use rocket_app::SessionIsolation;
use rocket_shared::error::DomainError;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_in_makes_empty_cwd_and_config_dirs_under_a_fresh_root() {
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");

        assert!(scratch.root().starts_with(parent.path()));
        assert!(scratch.cwd().is_dir());
        assert!(scratch.config_dir().is_dir());
        assert_ne!(scratch.cwd(), scratch.config_dir());
        assert_eq!(std::fs::read_dir(scratch.cwd()).expect("read cwd").count(), 0);
        assert_eq!(
            std::fs::read_dir(scratch.config_dir()).expect("read config").count(),
            0
        );
    }

    #[test]
    fn two_scratches_never_share_a_root() {
        let parent = TempDir::new().expect("tempdir");
        let a = SessionScratch::create_in(parent.path()).expect("create a");
        let b = SessionScratch::create_in(parent.path()).expect("create b");
        assert_ne!(a.root(), b.root());
    }

    #[test]
    fn dropping_removes_the_whole_root_even_with_files_inside() {
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        let root = scratch.root().to_path_buf();
        std::fs::create_dir_all(scratch.config_dir().join("projects"))
            .expect("agent-written subdir");
        std::fs::write(scratch.config_dir().join(".claude.json"), "{}").expect("agent-written file");

        drop(scratch);

        assert!(!root.exists());
    }

    #[cfg(unix)]
    #[test]
    fn scratch_dirs_are_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        for dir in [scratch.root(), scratch.cwd(), scratch.config_dir()] {
            let mode = std::fs::metadata(dir).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{} must be 0700", dir.display());
        }
    }

    #[test]
    fn isolation_returns_the_cwd_and_points_config_dir_at_the_scratch() {
        let parent = TempDir::new().expect("tempdir");
        let scratch = SessionScratch::create_in(parent.path()).expect("create scratch");
        let (cwd, isolation) = scratch.isolation().expect("utf-8 paths");
        assert_eq!(Path::new(&cwd), scratch.cwd());
        assert_eq!(Path::new(&isolation.config_dir), scratch.config_dir());
    }
}
```

Create `src-tauri/src/agent_session/cleanup.rs` with only its tests:

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use rocket_app::{McpToolService, SessionCleanup};

use crate::agent_session::scratch::SessionScratch;
use crate::mcp::registry::McpServerRegistry;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tool_server::McpHttpServerHandle;
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    fn handle() -> (McpHttpServerHandle, CancellationToken) {
        let shutdown = CancellationToken::new();
        let handle = McpHttpServerHandle {
            port: 4100,
            token: "token".to_string(),
            shutdown: shutdown.clone(),
        };
        (handle, shutdown)
    }

    fn recording_cleanup(
        mcp_registry: Arc<McpServerRegistry>,
        resources: Arc<SessionResourceRegistry>,
    ) -> (TauriSessionCleanup, Arc<Mutex<Vec<String>>>) {
        let forgotten = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&forgotten);
        let cleanup = TauriSessionCleanup::with_cache_forgetter(mcp_registry, resources, move |id| {
            sink.lock().expect("lock forgotten").push(id.to_string());
        });
        (cleanup, forgotten)
    }

    #[test]
    fn on_session_ended_forgets_both_ids_shuts_the_server_and_removes_scratch() {
        let parent = TempDir::new().expect("tempdir");
        let mcp_registry = Arc::new(McpServerRegistry::new());
        let resources = Arc::new(SessionResourceRegistry::new());
        let (server, shutdown) = handle();
        mcp_registry.register("acp-1".to_string(), server);
        let scratch = SessionScratch::create_in(parent.path()).expect("scratch");
        let root = scratch.root().to_path_buf();
        resources.register(
            "acp-1".to_string(),
            SessionResources {
                scratch,
                mcp_session_id: Some("mcp-pre-handshake".to_string()),
            },
        );
        let (cleanup, forgotten) = recording_cleanup(Arc::clone(&mcp_registry), Arc::clone(&resources));

        cleanup.on_session_ended("acp-1");

        assert!(shutdown.is_cancelled(), "the MCP server must be shut down");
        assert!(!root.exists(), "the scratch directories must be removed");
        assert!(resources.is_empty());
        assert_eq!(
            *forgotten.lock().expect("lock"),
            vec!["acp-1".to_string(), "mcp-pre-handshake".to_string()],
            "the test-result cache is keyed by the pre-handshake id, so both must be forgotten"
        );
    }

    #[test]
    fn on_session_ended_twice_is_harmless() {
        let parent = TempDir::new().expect("tempdir");
        let mcp_registry = Arc::new(McpServerRegistry::new());
        let resources = Arc::new(SessionResourceRegistry::new());
        resources.register(
            "acp-2".to_string(),
            SessionResources {
                scratch: SessionScratch::create_in(parent.path()).expect("scratch"),
                mcp_session_id: None,
            },
        );
        let (cleanup, forgotten) = recording_cleanup(mcp_registry, Arc::clone(&resources));

        cleanup.on_session_ended("acp-2");
        cleanup.on_session_ended("acp-2");

        assert!(resources.is_empty());
        assert_eq!(
            *forgotten.lock().expect("lock"),
            vec!["acp-2".to_string(), "acp-2".to_string()]
        );
    }

    #[test]
    fn on_session_ended_leaves_other_sessions_alone() {
        let parent = TempDir::new().expect("tempdir");
        let mcp_registry = Arc::new(McpServerRegistry::new());
        let resources = Arc::new(SessionResourceRegistry::new());
        let other = SessionScratch::create_in(parent.path()).expect("scratch");
        let other_root = other.root().to_path_buf();
        resources.register(
            "other".to_string(),
            SessionResources {
                scratch: other,
                mcp_session_id: None,
            },
        );
        let (cleanup, _forgotten) = recording_cleanup(mcp_registry, Arc::clone(&resources));

        cleanup.on_session_ended("acp-3");

        assert!(other_root.exists());
        assert_eq!(resources.len(), 1);
    }

    #[test]
    fn clear_all_removes_every_scratch() {
        let parent = TempDir::new().expect("tempdir");
        let resources = SessionResourceRegistry::new();
        let a = SessionScratch::create_in(parent.path()).expect("scratch a");
        let b = SessionScratch::create_in(parent.path()).expect("scratch b");
        let (root_a, root_b) = (a.root().to_path_buf(), b.root().to_path_buf());
        resources.register("a".to_string(), SessionResources { scratch: a, mcp_session_id: None });
        resources.register("b".to_string(), SessionResources { scratch: b, mcp_session_id: None });

        resources.clear_all();

        assert!(resources.is_empty());
        assert!(!root_a.exists());
        assert!(!root_b.exists());
    }
}
```

- [ ] **Step 3: Confirm the tests fail to compile**

Run: `cargo check -p rocket --all-targets -j4`
Expected: FAIL — `cannot find type SessionScratch`, `SessionResourceRegistry`, `TauriSessionCleanup`, `SessionResources`.

- [ ] **Step 4: Implement `SessionScratch`**

Add above the test module in `src-tauri/src/agent_session/scratch.rs`:

```rust
/// The directory under the system temp dir that holds every session's scratch.
pub const SCRATCH_PARENT_DIR: &str = "rocket-agent-sessions";

/// One session's private scratch: `<root>/cwd` is the agent's working
/// directory and `<root>/config` its `CLAUDE_CONFIG_DIR`. Both start empty.
/// Dropping the value removes the whole root, including anything the agent
/// wrote there.
pub struct SessionScratch {
    root: PathBuf,
    cwd: PathBuf,
    config_dir: PathBuf,
}

impl SessionScratch {
    /// Creates a scratch under the system temp dir.
    pub fn create() -> std::io::Result<Self> {
        Self::create_in(&std::env::temp_dir().join(SCRATCH_PARENT_DIR))
    }

    /// Creates a scratch under `parent`, which is created when missing.
    pub fn create_in(parent: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(parent)?;
        let root = parent.join(uuid::Uuid::new_v4().to_string());
        create_private_dir(&root)?;
        // From here on, an early return drops `scratch` and removes the root.
        let scratch = Self {
            cwd: root.join("cwd"),
            config_dir: root.join("config"),
            root,
        };
        create_private_dir(&scratch.cwd)?;
        create_private_dir(&scratch.config_dir)?;
        Ok(scratch)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Returns the cwd string for `session/new` and the isolation inputs
    /// that point `CLAUDE_CONFIG_DIR` at this scratch.
    pub fn isolation(&self) -> Result<(String, SessionIsolation), DomainError> {
        let cwd = path_to_string(&self.cwd)?;
        let config_dir = path_to_string(&self.config_dir)?;
        Ok((cwd, SessionIsolation::new(config_dir)))
    }
}

impl Drop for SessionScratch {
    fn drop(&mut self) {
        // Best effort. A leftover directory holds no credential, because the
        // API key reaches the agent through its environment only.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn path_to_string(path: &Path) -> Result<String, DomainError> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        DomainError::Internal("the agent scratch directory path is not valid UTF-8".to_string())
    })
}
```

- [ ] **Step 5: Implement `SessionResourceRegistry` and `TauriSessionCleanup`**

Add above the test module in `src-tauri/src/agent_session/cleanup.rs`:

```rust
/// What one live agent session owns outside `AcpSessionService`, apart from
/// its MCP server handle, which stays in `McpServerRegistry`.
pub struct SessionResources {
    pub scratch: SessionScratch,
    /// The pre-handshake id the session's MCP tool server tags its calls
    /// with until it is bound to the real id. Cleanup forgets it as well, so
    /// no cache entry made under it is left behind.
    pub mcp_session_id: Option<String>,
}

/// Tauri-managed state (`Arc<SessionResourceRegistry>`), keyed by the real
/// ACP session id.
#[derive(Default)]
pub struct SessionResourceRegistry {
    entries: Mutex<HashMap<String, SessionResources>>,
}

impl SessionResourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a session's resources. A replaced entry is dropped, which
    /// removes its scratch directories.
    pub fn register(&self, session_id: String, resources: SessionResources) {
        let replaced = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id, resources);
        drop(replaced);
    }

    /// Removes and returns a session's resources, if it has any.
    pub fn take(&self, session_id: &str) -> Option<SessionResources> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session_id)
    }

    /// Drops every entry, which removes every scratch directory. The app-exit
    /// paths call this as a backstop after `end_all_sessions`.
    pub fn clear_all(&self) {
        let drained = std::mem::take(&mut *self.entries.lock().unwrap_or_else(PoisonError::into_inner));
        drop(drained);
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

type CacheForgetter = Box<dyn Fn(&str) + Send + Sync>;

/// The production `SessionCleanup`. `AcpSessionService` calls it once per
/// session on every end path.
pub struct TauriSessionCleanup {
    mcp_registry: Arc<McpServerRegistry>,
    resources: Arc<SessionResourceRegistry>,
    forget_cache: CacheForgetter,
}

impl TauriSessionCleanup {
    pub fn new(
        mcp_registry: Arc<McpServerRegistry>,
        mcp_tool_svc: Arc<McpToolService>,
        resources: Arc<SessionResourceRegistry>,
    ) -> Self {
        Self::with_cache_forgetter(mcp_registry, resources, move |id| {
            mcp_tool_svc.forget_session(id)
        })
    }

    /// Test seam. `forget_cache` stands in for `McpToolService::forget_session`.
    pub fn with_cache_forgetter(
        mcp_registry: Arc<McpServerRegistry>,
        resources: Arc<SessionResourceRegistry>,
        forget_cache: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        Self {
            mcp_registry,
            resources,
            forget_cache: Box::new(forget_cache),
        }
    }
}

impl SessionCleanup for TauriSessionCleanup {
    fn on_session_ended(&self, session_id: &str) {
        // A no-op for a session that never had an MCP server.
        self.mcp_registry.end_session(session_id);
        (self.forget_cache)(session_id);
        if let Some(resources) = self.resources.take(session_id) {
            if let Some(mcp_session_id) = resources.mcp_session_id.as_deref() {
                (self.forget_cache)(mcp_session_id);
            }
            // Dropping the scratch removes its directories.
            drop(resources);
        }
    }
}
```

Run: `cargo check -p rocket --all-targets -j4` — expected: green.

- [ ] **Step 6: Write the failing integration tests**

In `src-tauri/tests/acp_mcp_start_agent_session.rs`:

1. Add imports:

```rust
use std::path::PathBuf;

use rocket_app::{isolation_meta, SessionCleanup, ISOLATION_ENV_CONFIG_DIR, ROCKET_ASSISTANT_SYSTEM_PROMPT};
use rocket_lib::agent_session::cleanup::{SessionResourceRegistry, TauriSessionCleanup};
```

2. Add the capture type and field to `FakeSessionClient` (`:153-157`):

```rust
#[derive(Clone)]
struct CapturedStart {
    cwd: String,
    env: Vec<(String, String)>,
    meta: Option<serde_json::Value>,
}
```

```rust
    captured_start: Arc<Mutex<Option<CapturedStart>>>,
```

In its `start_session`, rename `_cwd`, `_env` and `_meta` to `cwd`, `env` and `meta`, and add as the first statement:

```rust
        *self.captured_start.lock().expect("lock") = Some(CapturedStart {
            cwd: cwd.to_string(),
            env: env.to_vec(),
            meta,
        });
```

Add `captured_start: Arc::new(Mutex::new(None)),` to the three existing `FakeSessionClient { .. }` literals (`:248`, `:287`, `:321`).

3. Replace `build_fixture`'s signature and tail (`:196-243`) so it also returns a `SessionResourceRegistry`:

```rust
fn build_fixture(
    agent_autonomy_enabled: bool,
    client: FakeSessionClient,
) -> (
    CollectionService,
    Arc<McpServerRegistry>,
    Arc<SessionResourceRegistry>,
    AcpSessionService,
    tauri::AppHandle<tauri::test::MockRuntime>,
    TempDir,
) {
```

and at its end:

```rust
    let resources = Arc::new(SessionResourceRegistry::new());
    (collection_svc, registry, resources, acp_session_svc, app_handle, tmp)
```

4. In the three existing tests, destructure `let (collection_svc, registry, resources, acp_session_svc, app_handle, _tmp) = build_fixture(...);` and add `&resources,` after `&registry,` in each `start_agent_session_inner(` call.

5. Add two tests at the end of the file:

```rust
#[tokio::test]
async fn session_start_isolates_the_agent_in_fresh_scratch_directories() {
    let captured_start = Arc::new(Mutex::new(None));
    let client = FakeSessionClient {
        should_fail: false,
        real_session_id: "acp-real-session-4".to_string(),
        captured_servers: Arc::new(Mutex::new(Vec::new())),
        captured_start: Arc::clone(&captured_start),
    };
    let (collection_svc, registry, resources, acp_session_svc, app_handle, _tmp) =
        build_fixture(true, client);

    start_agent_session_inner(
        "agent-1".to_string(),
        "/tmp".to_string(),
        "demo".to_string(),
        app_handle,
        &collection_svc,
        &registry,
        &resources,
        &acp_session_svc,
    )
    .await
    .expect("start_agent_session_inner should succeed");

    let start = captured_start
        .lock()
        .expect("lock")
        .clone()
        .expect("the client's start_session must have been called");
    assert_ne!(start.cwd, "/tmp", "the requested cwd must be replaced by a scratch directory");
    let cwd = PathBuf::from(&start.cwd);
    assert!(cwd.is_dir());
    assert_eq!(std::fs::read_dir(&cwd).expect("read cwd").count(), 0);
    let config_dir = start
        .env
        .iter()
        .find(|(k, _)| k == ISOLATION_ENV_CONFIG_DIR)
        .map(|(_, v)| PathBuf::from(v))
        .expect("CLAUDE_CONFIG_DIR must be set");
    assert!(config_dir.is_dir());
    assert_ne!(config_dir, cwd);
    assert!(start.env.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"));
    assert_eq!(start.meta, Some(isolation_meta(ROCKET_ASSISTANT_SYSTEM_PROMPT)));
    assert_eq!(resources.len(), 1);

    let cleanup = TauriSessionCleanup::with_cache_forgetter(
        Arc::clone(&registry),
        Arc::clone(&resources),
        |_| {},
    );
    cleanup.on_session_ended("acp-real-session-4");

    assert!(!cwd.exists());
    assert!(!config_dir.exists());
    assert!(resources.is_empty());
}

#[tokio::test]
async fn session_start_failure_removes_the_scratch_directories() {
    let captured_start = Arc::new(Mutex::new(None));
    let client = FakeSessionClient {
        should_fail: true,
        real_session_id: "unused".to_string(),
        captured_servers: Arc::new(Mutex::new(Vec::new())),
        captured_start: Arc::clone(&captured_start),
    };
    let (collection_svc, registry, resources, acp_session_svc, app_handle, _tmp) =
        build_fixture(true, client);

    start_agent_session_inner(
        "agent-1".to_string(),
        "/tmp".to_string(),
        "demo".to_string(),
        app_handle,
        &collection_svc,
        &registry,
        &resources,
        &acp_session_svc,
    )
    .await
    .expect_err("a session-start failure must propagate as an error");

    let start = captured_start
        .lock()
        .expect("lock")
        .clone()
        .expect("the client's start_session must have been called");
    assert!(!PathBuf::from(&start.cwd).exists(), "the scratch cwd must be removed");
    let config_dir = start
        .env
        .iter()
        .find(|(k, _)| k == ISOLATION_ENV_CONFIG_DIR)
        .map(|(_, v)| PathBuf::from(v))
        .expect("CLAUDE_CONFIG_DIR must be set");
    assert!(!config_dir.exists(), "the scratch config dir must be removed");
    assert!(resources.is_empty());
}
```

Run: `cargo check -p rocket --all-targets -j4`
Expected: FAIL — `start_agent_session_inner` takes 7 arguments but 8 were supplied.

- [ ] **Step 7: Isolate the session start and fold the sweep into the cleanup**

In `src-tauri/src/commands/acp_sessions.rs`:

1. Update the imports (`McpToolService` is no longer used here):

```rust
use std::sync::Arc;

use rocket_acp::SessionInfo;
use rocket_app::{AcpSessionService, CollectionService, McpHttpServerCredentials};
use rocket_shared::error::DomainError;
use tauri::State;

use crate::agent_session::cleanup::{SessionResourceRegistry, SessionResources};
use crate::agent_session::scratch::SessionScratch;
use crate::commands::acp_session_dto::{
    prompt_parts, AgentSessionStartedDto, ConfigOptionDto, PromptResourceDto,
};
use crate::mcp::registry::McpServerRegistry;
```

(`rocket_acp::SessionInfo` and the `acp_session_dto` line are Plan 01's imports, kept as they are.)

2. Add the state parameter to the `start_agent_session` command and pass it on:

```rust
    registry: State<'_, Arc<McpServerRegistry>>,
    resources: State<'_, Arc<SessionResourceRegistry>>,
    svc: State<'_, AcpSessionService>,
```

```rust
        &registry,
        &resources,
        &svc,
```

3. Replace `start_agent_session_inner`'s signature and body (keep its doc comment and append the paragraph below to it):

```rust
/// Every session starts isolated. A fresh `SessionScratch` provides an empty
/// working directory and an empty `CLAUDE_CONFIG_DIR`, so the agent loads no
/// user, project or local settings, and `SessionIsolation` adds the `_meta`
/// options that switch off built-in tools. The frontend's requested cwd is
/// ignored for that reason. The scratch and the pre-handshake MCP id are
/// registered under the real session id, and `TauriSessionCleanup` releases
/// them on every end path.
#[allow(clippy::too_many_arguments)]
pub async fn start_agent_session_inner<R: tauri::Runtime>(
    agent_config_id: String,
    _requested_cwd: String,
    collection: String,
    app_handle: tauri::AppHandle<R>,
    collection_svc: &CollectionService,
    registry: &McpServerRegistry,
    resources: &SessionResourceRegistry,
    svc: &AcpSessionService,
) -> Result<SessionInfo, DomainError> {
    let autonomy_enabled = collection_svc
        .get_settings(&collection)?
        .agent_autonomy_enabled;

    // Created before the MCP server, so a failure here leaves nothing bound.
    let scratch = SessionScratch::create().map_err(|e| {
        DomainError::Internal(format!("failed to create the agent scratch directory: {e}"))
    })?;
    let (scratch_cwd, isolation) = scratch.isolation()?;

    // A Rocket-minted, pre-handshake-only identifier. See this function's
    // doc comment. It is kept so cleanup can forget the tool server's cache.
    let mcp_session_id = autonomy_enabled.then(|| uuid::Uuid::new_v4().to_string());
    let mcp_handle = match &mcp_session_id {
        Some(id) => Some(
            crate::mcp::tool_server::spawn_mcp_http_server(app_handle, id.clone())
                .await
                .map_err(|e| {
                    DomainError::Internal(format!("failed to start MCP tool server: {e}"))
                })?,
        ),
        None => None,
    };
    let mcp_credentials = mcp_handle.as_ref().map(|h| McpHttpServerCredentials {
        port: h.port,
        token: h.token.clone(),
    });

    let result = svc
        .start_session(
            &agent_config_id,
            &scratch_cwd,
            &collection,
            mcp_credentials,
            Some(isolation),
        )
        .await;

    match (result, mcp_handle) {
        (Ok(info), handle) => {
            // Registered under the real ACP session id, which every other
            // command addresses a session by.
            if let Some(handle) = handle {
                registry.register(info.session_id.clone(), handle);
            }
            resources.register(
                info.session_id.clone(),
                SessionResources {
                    scratch,
                    mcp_session_id,
                },
            );
            // The `start_agent_session` command maps this to the DTO with
            // `.map(AgentSessionStartedDto::from)`, as Plan 01 left it.
            Ok(info)
        }
        (Err(e), handle) => {
            // Never leave a bound listener with a live token behind.
            if let Some(handle) = handle {
                handle.shutdown();
            }
            // Dropping the scratch removes its directories.
            drop(scratch);
            Err(e)
        }
    }
}
```

4. Replace `end_agent_session`; the sweep now lives in `TauriSessionCleanup`, which `AcpSessionService::end_session` calls:

```rust
#[tauri::command]
pub async fn end_agent_session(
    session_id: String,
    svc: State<'_, AcpSessionService>,
) -> Result<(), DomainError> {
    // AcpSessionService runs TauriSessionCleanup, which ends the MCP server,
    // forgets the tool caches and removes the scratch directories.
    svc.end_session(&session_id).await
}
```

- [ ] **Step 8: Wire `TauriSessionCleanup` in `lib.rs`**

In `src-tauri/src/lib.rs`:

1. Delete the `let acp_session_svc = rocket_app::AcpSessionService::new(...);` statement (`:428-433`, as changed in Task 1).

2. Replace the comment block and binding at `:641-648` with:

```rust
            // Tracks every live per-session MCP HTTP server so it can be
            // swept on app exit and on individual session end. Shared with
            // TauriSessionCleanup below, which ends one session's server.
            let mcp_server_registry = Arc::new(mcp::registry::McpServerRegistry::new());

            // Per-session scratch directories and pre-handshake MCP ids.
            let session_resources =
                Arc::new(agent_session::cleanup::SessionResourceRegistry::new());

            // Built here, after mcp_tool_svc and the registries exist, because
            // its SessionCleanup needs all three.
            let acp_session_svc = rocket_app::AcpSessionService::new(
                Box::new(rocket_infra::AcpAgentClient::new()),
                Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
                Arc::new(agent_session::cleanup::TauriSessionCleanup::new(
                    Arc::clone(&mcp_server_registry),
                    Arc::clone(&mcp_tool_svc),
                    Arc::clone(&session_resources),
                )),
                acp_agent_config_svc,
                Arc::clone(&acp_collection_repo),
            );
```

3. In the managed-state block, after `app.manage(Arc::clone(&mcp_server_registry));`, add:

```rust
            app.manage(Arc::clone(&session_resources));
```

4. In both exit paths, right after the `mcp_registry.shutdown_all();` block (`:126-131` and `:951-955`), add:

```rust
            if let Some(resources) =
                app_handle.try_state::<Arc<agent_session::cleanup::SessionResourceRegistry>>()
            {
                // Backstop for a session that started while end_all_sessions ran.
                resources.clear_all();
            }
```

(Indent to match each block.)

- [ ] **Step 9: Verify**

Run: `cargo check --workspace --all-targets -j4` — expected: green.
Run: `yarn tsc --noEmit` — expected: green.

For the user to run:

```bash
cargo test -p rocket agent_session -j4
cargo test -p rocket --test acp_mcp_start_agent_session -j4
cargo test -p rocket mcp::registry -j4
```

Expected: all pass (9 new unit tests, 2 new integration tests, the 3 existing integration tests unchanged in behavior).

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/agent_session/mod.rs src-tauri/src/agent_session/scratch.rs \
        src-tauri/src/agent_session/cleanup.rs src-tauri/src/commands/acp_sessions.rs \
        src-tauri/src/lib.rs src-tauri/tests/acp_mcp_start_agent_session.rs
```

Use the `dev-workflow-skills:1-git-commit` skill. Message: `feat: isolate agent sessions in per-session scratch directories`.

---

### Task 3: Stale-session sweep command, TypeScript wrapper and manual checklist

**Files:**
- Modify: `crates/rocket-app/src/acp_session_service.rs` (new method after `end_all_sessions`; two tests at the end of `mod tests`)
- Modify: `src-tauri/src/commands/acp_sessions.rs` (new command after `end_agent_session`)
- Modify: `src-tauri/src/lib.rs:922` (register the command after `end_agent_session`)
- Modify: `src/lib/tauri-api.ts:2525-2526` (new wrapper after `endAgentSession`)
- Modify: `src/lib/queries/__tests__/agent-session-api.test.ts` (one test after the `endAgentSession` test)
- Create: `docs/superpowers/plans/workspace-ai-assistant/manual-checks-plan-02.md`

**Interfaces:**
- Consumes: `AcpSessionService`'s live set and `release` (Task 1).
- Produces:
  ```rust
  pub async fn end_tracked_sessions(&self) -> usize;                    // AcpSessionService
  #[tauri::command] pub async fn end_stale_assistant_sessions(svc: State<'_, AcpSessionService>) -> Result<usize, DomainError>;
  ```
  ```ts
  export const endStaleAssistantSessions: () => Promise<number>;
  ```

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `crates/rocket-app/src/acp_session_service.rs`:

```rust
    #[tokio::test]
    async fn end_tracked_sessions_ends_every_tracked_session_without_shutting_the_client_down() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let end_all_sessions_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            start_ids: Arc::new(Mutex::new(VecDeque::from(vec![
                "s-a".to_string(),
                "s-b".to_string(),
            ]))),
            end_session_called: Arc::clone(&end_session_called),
            end_all_sessions_called: Arc::clone(&end_all_sessions_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());
        start(&service).await;
        start(&service).await;

        assert_eq!(service.end_tracked_sessions().await, 2);

        let mut ended = cleanup.ended();
        ended.sort();
        assert_eq!(ended, vec!["s-a".to_string(), "s-b".to_string()]);
        assert!(end_session_called.load(Ordering::SeqCst));
        assert!(
            !end_all_sessions_called.load(Ordering::SeqCst),
            "end_all_sessions would make the client refuse every later session"
        );

        start(&service).await;
        assert_eq!(service.end_tracked_sessions().await, 1);
    }

    #[tokio::test]
    async fn end_tracked_sessions_with_nothing_tracked_ends_nothing() {
        let cleanup = Arc::new(RecordingCleanup::default());
        let end_session_called = Arc::new(AtomicBool::new(false));
        let client = FakeSessionClient {
            end_session_called: Arc::clone(&end_session_called),
            ..Default::default()
        };
        let service = service_with_cleanup(client, cleanup.clone());

        assert_eq!(service.end_tracked_sessions().await, 0);
        assert!(!end_session_called.load(Ordering::SeqCst));
        assert!(cleanup.ended().is_empty());
    }
```

In `src/lib/queries/__tests__/agent-session-api.test.ts`, after the `endAgentSession invokes end_agent_session with the session id` test:

```ts
  it('endStaleAssistantSessions invokes end_stale_assistant_sessions and returns the count', async () => {
    vi.mocked(invoke).mockResolvedValue(2);
    const { endStaleAssistantSessions } = await import('@/lib/tauri-api');
    const ended = await endStaleAssistantSessions();
    expect(invoke).toHaveBeenCalledWith('end_stale_assistant_sessions');
    expect(ended).toBe(2);
  });
```

- [ ] **Step 2: Confirm they fail**

Run: `cargo check -p rocket-app --all-targets -j4` — expected: FAIL, `no method named end_tracked_sessions`.
Run: `yarn tsc --noEmit` — expected: FAIL, `Property 'endStaleAssistantSessions' does not exist`.

- [ ] **Step 3: Implement the method, the command and the wrapper**

In `crates/rocket-app/src/acp_session_service.rs`, after `end_all_sessions`:

```rust
    /// Ends every session this service still tracks, one at a time, and
    /// returns how many it ended. Each one runs its cleanup once. Unlike
    /// `end_all_sessions`, the client keeps accepting new sessions, so the
    /// webview can start a fresh one right after this sweep.
    pub async fn end_tracked_sessions(&self) -> usize {
        let tracked: Vec<String> = self
            .live_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect();
        for session_id in &tracked {
            let _ = self.session_client.end_session(session_id).await;
            self.release(session_id);
        }
        tracked.len()
    }
```

In `src-tauri/src/commands/acp_sessions.rs`, after `end_agent_session`:

```rust
/// Ends every agent session the backend still tracks and returns how many.
/// The webview calls this once per load, from its app-lifetime assistant
/// event bridge, and every assistant start waits for it. At that point the
/// webview owns no session, so every tracked session is a leftover from
/// before a reload.
#[tauri::command]
pub async fn end_stale_assistant_sessions(
    svc: State<'_, AcpSessionService>,
) -> Result<usize, DomainError> {
    Ok(svc.end_tracked_sessions().await)
}
```

In `src-tauri/src/lib.rs`, after `commands::acp_sessions::end_agent_session,` (`:922`):

```rust
            commands::acp_sessions::end_stale_assistant_sessions,
```

In `src/lib/tauri-api.ts`, after `endAgentSession` (`:2525-2526`):

```ts
/**
 * Ends every agent session the backend still tracks and resolves to how many
 * it ended. Call once per webview load, before starting a session (Plan 05's
 * assistant event bridge does this, and every start waits for it). Sessions
 * started by this webview would be ended too.
 */
export const endStaleAssistantSessions = () => invoke<number>('end_stale_assistant_sessions');
```

- [ ] **Step 4: Write the manual checklist**

Create `docs/superpowers/plans/workspace-ai-assistant/manual-checks-plan-02.md`:

````markdown
# Manual checks — Plan 02 (isolation and lifecycle)

Run these in `yarn tauri dev` with an agent config that uses
`claude-agent-acp` 0.88.0 and an API-key credential (`ANTHROPIC_API_KEY` or
`CLAUDE_CODE_OAUTH_TOKEN` as the credential env var). The isolated session has
an empty `CLAUDE_CONFIG_DIR`, so a login stored only in `~/.claude` is not
available to it.

## 1. Token comparison for a first "hello" turn

The usage numbers come from the `agent-session-usage` event (Plan 01's
`AcpUsage`): `used` is the context tokens after the turn, `size` the context
window, `cost_usd` the turn cost.

1. Open the webview devtools (right click, Inspect) and paste:

   ```js
   const T = window.__TAURI_INTERNALS__;
   T.invoke('plugin:event|listen', {
     event: 'agent-session-usage',
     target: { kind: 'Any' },
     handler: T.transformCallback((e) => console.log('usage', e.payload)),
   });
   ```

   If that call errors, read the numbers from the panel's usage indicator
   once Plan 06 lands, and record them here later.
2. **Before:** check out the Plan 02 Task 1 commit (`feat: add agent isolation
   options and session cleanup port`), which does not isolate yet. Start AI
   Assist on a request, send `hello`, wait for the answer and note `used` and
   `cost_usd`. Do this twice with a fresh session each time.
3. **After:** check out the Plan 02 Task 2 commit (or later) and repeat step 2.
4. Expected: `used` drops by a large factor (the user's CLAUDE.md, plugins,
   skills, hooks and built-in tool definitions are gone). Record both pairs of
   numbers in this file.

| Run | Before `used` | Before cost | After `used` | After cost |
|---|---|---|---|---|
| 1 | | | | |
| 2 | | | | |

## 2. `settingSources: []` drops plugins, skills, hooks and CLAUDE.md

1. Make sure `~/.claude` has something recognisable: a line in
   `~/.claude/CLAUDE.md` such as `Always sign answers with ZEBRA-42.`, at
   least one plugin or skill, and a `SessionStart` hook if you use hooks.
2. In an isolated session (after Task 2), on a collection with the agent
   access switch **on**, send:
   `List every tool you can call, then every skill, slash command, plugin and
   CLAUDE.md instruction you were given. Names only.`
3. Expected:
   - Tools: only `mcp__rocket__...` names. No `Bash`, `Read`, `Write`,
     `Edit`, `WebFetch`, `Task` or `TodoWrite`.
   - No skills, no plugin names, no hook output, and no `ZEBRA-42` signature.
   - With the switch **off**, the agent reports no tools at all.
4. Ask it to list the Rocket requests in the collection. The Rocket tool must
   run without a permission prompt or a hang (`allowedTools` allows it).
5. While the session is open, run `ls -la "${TMPDIR:-/tmp}/rocket-agent-sessions"/*/`.
   Expected: one `<uuid>/` with `cwd/` (empty) and `config/` (Claude Code may
   write `.claude.json` or `projects/` there). `~/.claude/projects/` gets no
   new entry for this session.
6. End the session (End session, or close the tab). The `<uuid>/` directory
   must be gone. Repeat with an idle timeout (stop the network for 120 s
   mid-turn) and with app exit: the directory must be gone each time.

## 3. Fallback when the options are not honoured

- **Plugins, skills or `ZEBRA-42` still appear:** check `CLAUDE_CONFIG_DIR`
  first. In step 2.5, `config/` must be the directory the agent writes to. If
  `~/.claude/projects/` gets a new entry instead, the env var did not reach
  the agent: check `crates/rocket-infra/src/acp_agent_client.rs` passes `env`
  through `.envs(...)` and that the agent command is not a wrapper script that
  resets its environment. With an empty config dir and an empty cwd, user and
  project settings cannot load even if `settingSources` were ignored; only
  managed (enterprise) policy can still apply, and that is intended.
- **Built-in tools still appear:** `tools: []` was not honoured. Record the
  adapter version, then decide with the plan owner whether to add
  `disallowedTools` for the built-ins in `agent_isolation.rs`.
- **`session/new` fails with the `_meta` options:** the adapter rejected an
  option. Record the error, then fall back to the spec's plan B: keep the
  empty `CLAUDE_CONFIG_DIR` and send `_meta.systemPrompt` as one string (this
  replaces the whole `claude_code` preset, so test that tool use still works).
  Changing `isolation_meta` needs an index update, because its shape is locked.

## 4. Stale-session sweep command

Not wired to the UI until Plan 05. In devtools, with one AI Assist session
open:

```js
window.__TAURI_INTERNALS__.invoke('end_stale_assistant_sessions').then(console.log);
```

Expected: logs `1`, the scratch directory disappears, and a new AI Assist
session still starts afterwards.
````

- [ ] **Step 5: Verify**

Run: `cargo check --workspace --all-targets -j4` — expected: green.
Run: `yarn tsc --noEmit` — expected: green.
Run: `yarn check` — expected: green.

For the user to run:

```bash
cargo test -p rocket-app end_tracked_sessions -j4
yarn test agent-session-api
```

Expected: 2 Rust tests and the new Vitest case pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/acp_session_service.rs src-tauri/src/commands/acp_sessions.rs \
        src-tauri/src/lib.rs src/lib/tauri-api.ts \
        src/lib/queries/__tests__/agent-session-api.test.ts \
        docs/superpowers/plans/workspace-ai-assistant/manual-checks-plan-02.md
```

Use the `dev-workflow-skills:1-git-commit` skill. Message: `feat: add stale agent session sweep command`.

---

## Next Plan

**Plan 03 — Workspace-scoped read tools and modes** (`docs/superpowers/plans/workspace-ai-assistant/2026-10-09-workspace-ai-assistant-plan-03-workspace-read-tools-and-modes.md`). It adds `start_workspace_assistant(agent_config_id, mode, model)`, which must reuse this plan's isolated start: `SessionScratch::create()` then `scratch.isolation()`, hand the `SessionIsolation` (with its own `system_prompt_append`) to `AcpSessionService::start_workspace_session`, which tracks the session exactly like `start_session`, and register `SessionResources { scratch, mcp_session_id }` in `SessionResourceRegistry` under the real session id. Its per-session state (mode, workspace outline) is released by `McpToolService::forget_session`, which `TauriSessionCleanup::on_session_ended` already calls, so every end path covers it. Plan 03 also adds `McpSessionBinding`, which tags tool calls with the real ACP id after the handshake; this plan's "forget both ids" then remains only as a backstop. Plan 04's `ProposalService::clear_session` joins the cache forgetter in `TauriSessionCleanup::new`. Plan 05 calls `endStaleAssistantSessions()` once per webview load from its app-lifetime event bridge, and every `start_workspace_assistant` call waits for it.

Known limit left open: scratch directories of a crashed Rocket process stay under `rocket-agent-sessions/`. A startup sweep is not added, because a second running Rocket instance would lose its live directories.

## Post-Implementation Review

After all three tasks are checked off and `cargo check --workspace --all-targets -j4`, `yarn tsc --noEmit` and `yarn check` are green, dispatch a review subagent:

```
Agent({
  subagent_type: "general-purpose",
  model: "opus",
  description: "Plan 02 isolation and lifecycle review",
  prompt: "Review the full diff this plan produced (the three commits of
    docs/superpowers/plans/workspace-ai-assistant/2026-10-09-workspace-ai-assistant-plan-02-isolation-and-lifecycle.md,
    e.g. `git log --oneline -5` then `git diff <first>^..HEAD`). Read that plan
    file and docs/superpowers/plans/workspace-ai-assistant/00-plan-index.md in
    full first. You have authority to fix what you find directly (edit files,
    re-run cargo check --workspace --all-targets -j4, yarn tsc --noEmit,
    yarn check, and commit via the dev-workflow-skills:1-git-commit skill)
    rather than only reporting it. Do not run cargo test --workspace; run only
    targeted tests (cargo test -p <crate> <name> -j4) if you need to.
    Check specifically for:
    (1) Interface gaps against the index's locked Plan 02 contract —
    SessionCleanup::on_session_ended(&self, &str), AcpSessionService::new with
    cleanup right after event_publisher, isolation_meta(&str) -> Value with
    exactly the locked JSON, ISOLATION_ENV_CONFIG_DIR = \"CLAUDE_CONFIG_DIR\",
    end_stale_assistant_sessions(). Are the plan's listed additions the only
    additions?
    (2) Exactly-once cleanup — trace every end path (end_session, idle
    timeout, failed prompt, InvalidInput prompt, end_all_sessions,
    end_tracked_sessions, start failure) and confirm each tracked session
    reaches SessionCleanup once and no path leaves an MCP server, a
    test-result cache entry (both ids) or a scratch directory behind.
    (3) Code quality — no unwrap calls in production code, PoisonError
    recovery on every production lock, short full-sentence comments, no
    leftover TODOs, the stale comment about a TauriMcpServerSweeper in
    lib.rs is gone.
    (4) DDD boundaries — rocket-app does no I/O (no std::fs, no temp_dir) and
    has no tauri dependency; the commands stay thin; directory work lives in
    src-tauri/src/agent_session.
    Report what you found and what you fixed, in under 400 words."
})
```
