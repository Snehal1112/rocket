# ACP MCP Tool Server — Plan 01: Domain Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land every domain-layer type and trait signature subproject D (the ACP MCP tool server) needs — `rocket-shared`'s `RunSource` enum and `AcpToolInvoked` event, `rocket-acp`'s `McpServerSpec` and the new `AcpSessionClient::start_session` parameter, `rocket-collection`'s `agent_autonomy_enabled` flag and `save_request_script` method (implemented end-to-end), and `rocket-history`'s `run_source` field — while keeping `cargo check --workspace -j4` green throughout.

**Architecture:** This is a pure domain-contracts plan: no MCP server, no `rmcp`/`axum`, no tool dispatch logic lands here. Every change is either (a) a new small type in a domain crate, (b) a trait signature addition with all existing implementors/callers updated to keep compiling, or (c) one fully-implemented repository method (`save_request_script`) that is small and self-contained enough to finish end-to-end now rather than split its signature and body across plans. Later plans (02–06) build the actual MCP server, tool dispatch, and UI on top of these contracts.

**Tech Stack:** Rust (workspace crates `rocket-shared`, `rocket-acp`, `rocket-collection`, `rocket-history`, `rocket-infra`, `rocket-app`), `serde`/`serde_yaml`, `async-trait`, `tokio`.

**Spec:** [`docs/superpowers/specs/2026-09-28-acp-mcp-tool-server-design.md`](../../specs/2026-09-28-acp-mcp-tool-server-design.md). Locked cross-plan interface contracts: [`docs/superpowers/plans/acp-mcp-tool-server/00-plan-index.md`](00-plan-index.md).

## Global Constraints

- Always pass `-j4` to every `cargo check`/`cargo test` invocation in this repo.
- `cargo check --workspace -j4` must stay green at the end of every task in this plan — never leave the workspace non-compiling between tasks or between this plan and Plan 02.
- Rust: never `unwrap()` in production paths (test code may use `.unwrap()`/`.expect()`).
- Serde: `#[serde(rename_all = "camelCase")]` only on IPC DTOs, never on persistence structs. `DomainEvent` itself uses `#[serde(tag = "type", rename_all = "camelCase")]` (pre-existing, do not change).
- No existing `DomainEvent` variant carries a timestamp field; `AcpToolInvoked` must not add one either — ordering comes from emission order.
- `CollectionRepository` is a synchronous trait (no `async`) — `save_request_script` must be a plain synchronous method, matching every other method on the trait.
- Commits: conventional commits format (`feat:`, `fix:`, `chore:`, etc.), created via the `dev-workflow-skills:1-git-commit` skill, never a freeform `git commit -m`.
- 📖 Before starting Task 1 (and keep in mind for every task that touches `.yml`/collection/request data), read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Review Focus

- **Old on-disk data missing the new fields must still load.** A `HistoryEntry` YAML/JSON with no `runSource` key, and a `CollectionSettings`/`opencollection.yml` with no `agent_autonomy_enabled`-equivalent data, must deserialize successfully and default to `Manual`/`false` rather than failing — covered in Task 3 (`CollectionSettings` backward-compat) and Task 5 (`HistoryEntry` backward-compat).
- **`save_request_script` must touch only the targeted phase.** A request that already has all three scripts set (pre-request, post-response, tests) must keep the other two untouched, byte-for-byte, when only one phase is edited — covered in Task 4.
- **`save_request_script` against a request path that does not exist must error clearly, not panic.** Covered in Task 4, asserting the same `DomainError::Io` shape `save_request_variables` already produces for the identical missing-file case (verified against this codebase's actual `resolve_request_path` behavior, not assumed).
- **Concurrent `save_request_script` calls must not corrupt either file.** Reuses the same per-collection mutex `save_request_variables`/`save_request` already take; covered in Task 4 with a two-thread test against `SharedPathCollectionRepo`, mirroring its existing `concurrent_saves_to_same_collection_both_complete_without_error` test.
- **Every existing `AcpSessionClient` caller/implementor must keep working unchanged with an empty `mcp_servers` slice.** The signature change must not alter today's chat-only session behavior for any of the trait's current callers — covered in Task 2, which updates and re-runs every existing test for `AcpAgentClient`, `AcpSessionService`, and the trait's own test doubles.

---

### Task 1: `rocket-shared` — `RunSource` enum, `AcpToolInvoked` event, and the `TauriEventBus` mapping

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-shared/src/run_source.rs`
- Modify: `crates/rocket-shared/src/lib.rs`
- Modify: `crates/rocket-shared/src/events.rs`
- Modify: `src-tauri/src/tauri_event_bus.rs`

**Interfaces:**
- Consumes: nothing new — extends the existing `DomainEvent` enum (`#[serde(tag = "type", rename_all = "camelCase")]`) and `TauriEventBus`'s exhaustive `match` in `crates/rocket-shared/src/events.rs` / `src-tauri/src/tauri_event_bus.rs`.
- Produces: `rocket_shared::RunSource` (`Manual` (default) / `Runner` / `LoadTest` / `Flow` / `Agent`, `#[serde(rename_all = "snake_case")]`), consumed by Task 5 (`rocket-history`) and later by Plan 03 (`rocket-app`). `DomainEvent::AcpToolInvoked { session_id: String, tool: String, summary: String }`, consumed by Plan 03's `McpToolService`.

- [ ] **Step 1: Write the failing test for `RunSource`**

Create `crates/rocket-shared/src/run_source.rs` with only the test module (the type does not exist yet, so this fails to compile):

```rust
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_manual() {
        assert_eq!(RunSource::default(), RunSource::Manual);
    }

    #[test]
    fn serializes_as_snake_case_string() {
        assert_eq!(serde_json::to_string(&RunSource::Manual).unwrap(), r#""manual""#);
        assert_eq!(serde_json::to_string(&RunSource::Runner).unwrap(), r#""runner""#);
        assert_eq!(
            serde_json::to_string(&RunSource::LoadTest).unwrap(),
            r#""load_test""#
        );
        assert_eq!(serde_json::to_string(&RunSource::Flow).unwrap(), r#""flow""#);
        assert_eq!(serde_json::to_string(&RunSource::Agent).unwrap(), r#""agent""#);
    }

    #[test]
    fn round_trips_through_json() {
        for source in [
            RunSource::Manual,
            RunSource::Runner,
            RunSource::LoadTest,
            RunSource::Flow,
            RunSource::Agent,
        ] {
            let json = serde_json::to_string(&source).expect("serialize");
            let back: RunSource = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, source);
        }
    }
}
```

Add the new module to `crates/rocket-shared/src/lib.rs` so the file is actually compiled (insert alphabetically, between `proxy` and `types`):

```rust
pub mod proxy;
pub mod run_source;
pub mod types;
```

And re-export the type at crate root (insert between the `oauth2` and `types` re-exports, matching this file's existing one-line-per-type style):

```rust
pub use oauth2::OAuth2Flow;
pub use run_source::RunSource;
pub use types::{Header, PathParam, QueryParam};
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rocket-shared -j4 run_source`
Expected: FAIL to compile — `cannot find type \`RunSource\` in this scope`.

- [ ] **Step 3: Implement `RunSource`**

Add the enum above the test module in `crates/rocket-shared/src/run_source.rs`:

```rust
/// Distinguishes how a request execution was triggered, so `HistoryEntry`
/// (and later, IPC-level execution inputs) can tell an agent-driven run
/// apart from a manual one. `Manual` is the default so every existing call
/// site that builds a `RunSource`-carrying type without setting this field
/// keeps its current (manual) behavior unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunSource {
    #[default]
    Manual,
    Runner,
    LoadTest,
    Flow,
    Agent,
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p rocket-shared -j4 run_source`
Expected: PASS (3 tests).

- [ ] **Step 5: Write the failing test for `DomainEvent::AcpToolInvoked`**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/rocket-shared/src/events.rs`, right after `acp_session_failed_wire_shape`:

```rust
#[test]
fn acp_tool_invoked_wire_shape() {
    let event = DomainEvent::AcpToolInvoked {
        session_id: "sess-1".into(),
        tool: "run_request".into(),
        summary: "Ran GET /users".into(),
    };
    let json = serde_json::to_string(&event).expect("serialize");
    assert_eq!(
        json,
        r#"{"type":"acpToolInvoked","session_id":"sess-1","tool":"run_request","summary":"Ran GET /users"}"#
    );
}
```

- [ ] **Step 6: Run the test to verify it fails**

Run: `cargo test -p rocket-shared -j4 acp_tool_invoked_wire_shape`
Expected: FAIL to compile — `no variant or associated item named \`AcpToolInvoked\` found for enum \`DomainEvent\``.

- [ ] **Step 7: Implement `DomainEvent::AcpToolInvoked`**

In `crates/rocket-shared/src/events.rs`, add the new variant to the `DomainEvent` enum, directly after `AcpSessionFailed` (still inside the "ACP session events" group — this variant is emitted by the ACP tool dispatcher, not a session lifecycle event, but groups naturally next to the other Acp* variants):

```rust
    /// Emitted by the MCP tool dispatcher for every tool call it handles
    /// (list/run/edit/get/set), regardless of outcome kind, so agent-driven
    /// actions are auditable and distinguishable from manual ones. No
    /// timestamp field, matching every other `DomainEvent` variant —
    /// ordering comes from emission order, not a payload timestamp.
    AcpToolInvoked {
        session_id: String,
        tool: String,
        summary: String,
    },
```

- [ ] **Step 8: Run the test to verify it passes**

Run: `cargo test -p rocket-shared -j4 acp_tool_invoked_wire_shape`
Expected: PASS.

- [ ] **Step 9: Add the required `TauriEventBus` match arm**

`DomainEvent`'s new variant breaks `cargo check -p rocket -j4` (the `src-tauri` package) immediately, since `TauriEventBus::publish`'s `match` has no wildcard arm. In `src-tauri/src/tauri_event_bus.rs`, add the new arm directly after the existing `AcpSessionFailed` arm (same "ACP AI-assist session events" group):

```rust
            DomainEvent::AcpSessionFailed { .. } => "agent-session-failed",
            DomainEvent::AcpToolInvoked { .. } => "agent-tool-invoked",
```

- [ ] **Step 10: Run the full check to verify the workspace compiles**

Run: `cargo check --workspace -j4`
Expected: PASS with no errors.

- [ ] **Step 11: Commit**

Use the `dev-workflow-skills:1-git-commit` skill (`Skill` tool, skill name `dev-workflow-skills:1-git-commit`) to draft and create the commit for:
- `crates/rocket-shared/src/run_source.rs`
- `crates/rocket-shared/src/lib.rs`
- `crates/rocket-shared/src/events.rs`
- `src-tauri/src/tauri_event_bus.rs`

---

### Task 2: `rocket-acp` — `McpServerSpec` and the `AcpSessionClient::start_session` `mcp_servers` parameter

**Files:**
- Create: `crates/rocket-acp/src/mcp_server_spec.rs`
- Modify: `crates/rocket-acp/src/lib.rs`
- Modify: `crates/rocket-acp/src/session.rs`
- Modify: `crates/rocket-infra/src/acp_agent_client.rs`
- Modify: `crates/rocket-infra/tests/acp_agent_client.rs`
- Modify: `crates/rocket-app/src/acp_session_service.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `rocket_acp::McpServerSpec` (`Http { name, url, token }` / `Stdio { name, command, args, env }`), and the new `AcpSessionClient::start_session` signature:
  ```rust
  async fn start_session(
      &self,
      command: &str,
      args: &[String],
      cwd: &str,
      env: &[(String, String)],
      mcp_servers: &[McpServerSpec],
  ) -> DomainResult<String>;
  ```
  Plan 02 is where `AcpAgentClient` actually maps `McpServerSpec` values into `agent_client_protocol::McpServer` and attaches them to `NewSessionRequest`; this task only adds the parameter and accepts-but-ignores it in the one real implementation, so the workspace keeps compiling. Plan 03 is where `AcpSessionService::start_session` starts building a real, non-empty `Vec<McpServerSpec>`.

This task changes a `Send + Sync` trait's signature. Every implementor and every call site across the workspace was located by grepping for `AcpSessionClient`/`start_session` — there are exactly four: the trait's own test doubles in `rocket-acp`, the one production implementor `AcpAgentClient` in `rocket-infra`, that implementor's integration test suite (also in `rocket-infra`), and the one production caller `AcpSessionService::start_session` in `rocket-app` (which has its own test double too). `src-tauri/src/commands/acp_sessions.rs` calls `AcpSessionService::start_session(agent_config_id, cwd)` — that method's own signature does not change in this plan — so it needs no edit.

- [ ] **Step 1: Write the failing test for `McpServerSpec`**

Create `crates/rocket-acp/src/mcp_server_spec.rs`:

```rust
/// A single MCP server the ACP agent should be told about for one session.
/// Rocket-owned type — deliberately NOT `agent_client_protocol::McpServer`,
/// since `rocket-acp` must not depend on that crate (existing DDD boundary).
/// `rocket-infra`'s `AcpAgentClient` maps this to the real
/// `agent_client_protocol::McpServer` type when it builds `NewSessionRequest`
/// (Plan 02).
#[derive(Debug, Clone)]
pub enum McpServerSpec {
    Http {
        name: String,
        url: String,
        token: String,
    },
    Stdio {
        name: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_variant_holds_its_fields() {
        let spec = McpServerSpec::Http {
            name: "rocket-mcp".to_string(),
            url: "http://127.0.0.1:4000/mcp".to_string(),
            token: "tok-abc".to_string(),
        };
        match spec {
            McpServerSpec::Http { name, url, token } => {
                assert_eq!(name, "rocket-mcp");
                assert_eq!(url, "http://127.0.0.1:4000/mcp");
                assert_eq!(token, "tok-abc");
            }
            McpServerSpec::Stdio { .. } => panic!("expected Http variant"),
        }
    }

    #[test]
    fn stdio_variant_holds_its_fields() {
        let spec = McpServerSpec::Stdio {
            name: "rocket-mcp-stdio".to_string(),
            command: "/usr/bin/rocket".to_string(),
            args: vec!["--acp-mcp-stdio-bridge".to_string()],
            env: vec![("ROCKET_MCP_PORT".to_string(), "4000".to_string())],
        };
        match spec {
            McpServerSpec::Stdio {
                name,
                command,
                args,
                env,
            } => {
                assert_eq!(name, "rocket-mcp-stdio");
                assert_eq!(command, "/usr/bin/rocket");
                assert_eq!(args, vec!["--acp-mcp-stdio-bridge".to_string()]);
                assert_eq!(
                    env,
                    vec![("ROCKET_MCP_PORT".to_string(), "4000".to_string())]
                );
            }
            McpServerSpec::Http { .. } => panic!("expected Stdio variant"),
        }
    }

    #[test]
    fn is_clonable_and_debug_formattable() {
        let spec = McpServerSpec::Http {
            name: "a".into(),
            url: "b".into(),
            token: "c".into(),
        };
        let cloned = spec.clone();
        let _ = format!("{cloned:?}");
    }
}
```

Wire the module into `crates/rocket-acp/src/lib.rs`:

```rust
pub mod agent_config;
pub mod mcp_server_spec;
pub mod session;
pub use agent_config::{AgentConfig, AgentConfigRepository};
pub use mcp_server_spec::McpServerSpec;
pub use session::AcpSessionClient;
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test -p rocket-acp -j4 mcp_server_spec`
Expected: PASS (3 tests) — this is a self-contained new type with no dependency on the trait change, so it compiles standalone.

- [ ] **Step 3: Write the failing test for the new `start_session` parameter**

In `crates/rocket-acp/src/session.rs`, update both test doubles' `start_session` signatures and both call sites to pass a trailing `&[]`. First, the trait's own doc/behavior tests (this is the "failing test" step: the trait signature has not changed yet, so passing 5 args here fails to compile against the old 4-arg trait):

```rust
    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            _mcp_servers: &[McpServerSpec],
        ) -> DomainResult<String> {
            Ok("session-1".to_string())
        }
```

```rust
        let session_id = client
            .start_session("echo", &[], "/tmp", &[], &[])
            .await
            .expect("start_session");
```

```rust
            async fn start_session(
                &self,
                _command: &str,
                _args: &[String],
                _cwd: &str,
                _env: &[(String, String)],
                _mcp_servers: &[McpServerSpec],
            ) -> DomainResult<String> {
                Err(DomainError::InvalidInput("command not found".to_string()))
            }
```

```rust
        let err = client
            .start_session("bad-command", &[], "/tmp", &[], &[])
            .await
            .expect_err("must propagate the error");
```

Add one new import at the top of `session.rs` itself (outside the test module, alongside its existing `use rocket_shared::error::DomainResult;`):

```rust
use crate::McpServerSpec;
```

The test module's existing `use super::*;` picks this up automatically, so no separate import is needed inside `mod tests`.

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p rocket-acp -j4 trait_is_object_safe_and_callable_through_a_trait_object`
Expected: FAIL to compile — the trait still declares `start_session` with 4 arguments, so both impls now have a method that doesn't match any trait method, and both call sites pass one argument too many.

- [ ] **Step 5: Implement the trait signature change**

In `crates/rocket-acp/src/session.rs`, change the trait method:

```rust
    /// Spawns the agent process and performs the `initialize` → `session/new`
    /// handshake. Returns the ACP-provided `sessionId`, used as-is for every
    /// later call — no separate Rocket-side id translation layer.
    /// `mcp_servers` is passed through to the agent's `session/new` request so
    /// it can reach the in-process MCP tool server (Plan 02 wires the mapping
    /// into `NewSessionRequest`; an empty slice means chat-only, matching
    /// today's behavior exactly).
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
    ) -> DomainResult<String>;
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p rocket-acp -j4`
Expected: PASS — both tests in `session.rs` compile and pass again.

- [ ] **Step 7: Update the one production implementor, `AcpAgentClient`**

In `crates/rocket-infra/src/acp_agent_client.rs`, update the import:

```rust
use rocket_acp::{AcpSessionClient, McpServerSpec};
```

And the `start_session` signature (the real mapping of `McpServerSpec` into `agent_client_protocol::McpServer`/`NewSessionRequest` is Plan 02's job — this task only accepts and ignores the parameter, exactly as the plan index requires, so the workspace keeps compiling):

```rust
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
    ) -> DomainResult<String> {
        // Real MCP-server attachment (mapping `McpServerSpec` into
        // `agent_client_protocol::McpServer` and threading it into
        // `NewSessionRequest`) lands in Plan 02. Accepting-but-ignoring the
        // parameter here keeps the workspace compiling in the meantime.
        let _ = mcp_servers;
```

(That `let _ = mcp_servers;` line goes immediately after the signature's opening brace; every line of the existing function body after it is unchanged.)

- [ ] **Step 8: Update every call site in `AcpAgentClient`'s integration test suite**

In `crates/rocket-infra/tests/acp_agent_client.rs`, every `.start_session(...)` call needs a trailing `&[]`. Thirteen call sites share one of two shapes plus two multi-line outliers.

The 13 single-line calls (all of the exact form `.start_session(<args...>, &[])` or `.start_session(<args...>, &[]),`) each get `&[]` appended as a new trailing argument. Concretely, replace every occurrence of the literal line:

```rust
        .start_session(&fixture_command(), &[], "/tmp", &[])
```

with:

```rust
        .start_session(&fixture_command(), &[], "/tmp", &[], &[])
```

This exact line appears at 9 call sites (as of this writing: the fixture-agent tests for `start_session_returns_a_session_id`, `send_prompt_streams_a_chunk_and_returns_a_stop_reason`, `send_prompt_works_twice_on_the_same_session_for_multi_turn_chat`, `end_session_kills_the_process_and_removes_the_session`, `a_crashed_agent_is_removed_from_the_session_map`, `end_session_unblocks_an_in_flight_prompt`, `end_session_aborts_the_background_dispatch_task` (inside its loop), both `session_a`/`session_b` calls in `end_all_sessions_kills_every_running_session`, and `start_session_after_end_all_sessions_is_refused`) — apply the same one-argument append to each.

The remaining single-line variants:

```rust
        .start_session("definitely-not-a-real-binary-xyz123", &[], "/tmp", &[])
```
→
```rust
        .start_session("definitely-not-a-real-binary-xyz123", &[], "/tmp", &[], &[])
```

```rust
        client.start_session("sh", &["-c".to_string(), script], "/tmp", &[]),
```
→
```rust
        client.start_session("sh", &["-c".to_string(), script], "/tmp", &[], &[]),
```
(inside `acp_agent_client_cancelled_start_session_kills_the_whole_process_group`)

```rust
                .start_session("sh", &["-c".to_string(), script], "/tmp", &[])
```
→
```rust
                .start_session("sh", &["-c".to_string(), script], "/tmp", &[], &[])
```
(inside `acp_agent_client_end_all_sessions_kills_a_session_still_in_its_handshake`)

And the two multi-line calls, each building an explicit credential env vec — append `&[],` as a new line directly before the closing `)` of the call:

```rust
    let err = client
        .start_session(
            "definitely-not-a-real-binary-xyz123",
            &[],
            "/tmp",
            &[(
                "ANTHROPIC_API_KEY".to_string(),
                "sk-super-secret-test-value".to_string(),
            )],
            &[],
        )
        .await
        .expect_err("nonexistent command must fail");
```

```rust
    let err = client
        .start_session(
            "true",
            &[],
            "/tmp",
            &[(
                "ANTHROPIC_API_KEY".to_string(),
                "sk-super-secret-async-value".to_string(),
            )],
            &[],
        )
        .await
        .expect_err(
            "a process that exits immediately without speaking ACP must fail the handshake, not panic or hang",
        );
```

- [ ] **Step 9: Run `AcpAgentClient`'s integration tests to verify they pass**

Run: `cargo test -p rocket-infra -j4 --test acp_agent_client`
Expected: PASS (all tests in this file).

- [ ] **Step 10: Update the one production caller, `AcpSessionService`**

In `crates/rocket-app/src/acp_session_service.rs`, update the call site inside `start_session` (Plan 03 replaces this `&[]` with a real, conditionally-built `Vec<McpServerSpec>`; this task only keeps the workspace compiling with chat-only behavior, identical to today):

```rust
        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &[])
            .await?;
```

And update its test double's signature in the `#[cfg(test)] mod tests` block:

```rust
    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
            _mcp_servers: &[rocket_acp::McpServerSpec],
        ) -> DomainResult<String> {
            if self.start_should_fail {
                Err(DomainError::InvalidInput("command not found".to_string()))
            } else {
                Ok("session-1".to_string())
            }
        }
```

- [ ] **Step 11: Run `rocket-app`'s ACP session tests to verify they pass**

Run: `cargo test -p rocket-app -j4 acp_session_service`
Expected: PASS (all tests in this module, including `start_session_resolves_config_and_credential_and_publishes_started`).

- [ ] **Step 12: Run the full workspace check**

Run: `cargo check --workspace -j4`
Expected: PASS with no errors.

- [ ] **Step 13: Commit**

Use the `dev-workflow-skills:1-git-commit` skill to create the commit for:
- `crates/rocket-acp/src/mcp_server_spec.rs`
- `crates/rocket-acp/src/lib.rs`
- `crates/rocket-acp/src/session.rs`
- `crates/rocket-infra/src/acp_agent_client.rs`
- `crates/rocket-infra/tests/acp_agent_client.rs`
- `crates/rocket-app/src/acp_session_service.rs`

---

### Task 3: `rocket-collection` — `CollectionSettings.agent_autonomy_enabled`

**Files:**
- Modify: `crates/rocket-collection/src/settings.rs`
- Modify: `crates/rocket-infra/src/fs_collection/settings.rs`
- Modify: `crates/rocket-infra/src/fs_collection/tests.rs`
- Modify: `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`
- Modify: `crates/rocket-infra/src/conversions/folder.rs`
- Modify: `crates/rocket-app/src/execution_service.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `CollectionSettings.agent_autonomy_enabled: bool` (`#[serde(default)]`, defaults to `false`). Plan 02 wires the real `extensions.rocketapi.agentAutonomyEnabled` persistence mapping (mirroring the existing `sandbox_mode` ↔ `extensions.rocketapi.sandboxMode` pattern in `crates/rocket-infra/src/fs_collection/settings.rs`); this task adds the domain field and a compiling `false` stub everywhere the struct is constructed by field list rather than `..Default::default()`, per the cross-plan compilation rule.

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

`CollectionSettings` does not derive `#[non_exhaustive]`, so every struct-literal construction across the workspace that lists all fields explicitly (rather than spreading `..Default::default()`) breaks the moment a new field is added. A workspace-wide search for `CollectionSettings {` found six such call sites that do **not** use a `..` spread — all six are updated in this task so `cargo check --workspace -j4` stays green. Sites that already use `..Default::default()` / `..CollectionSettings::default()` (there are many, e.g. in `execution_service.rs` and `crates/rocket-collection/src/settings.rs`'s own tests) need no change, since `bool`'s `Default` is `false`.

- [ ] **Step 1: Write the failing test for the new field's default**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/rocket-collection/src/settings.rs`, after `sandbox_mode_developer_roundtrips_as_camel_case`:

```rust
    #[test]
    fn agent_autonomy_enabled_defaults_to_false_when_absent_from_json() {
        let json = r#"{"headers":[],"variables":[]}"#;
        let settings: CollectionSettings = serde_json::from_str(json).expect("deserialize");
        assert!(!settings.agent_autonomy_enabled);
    }

    #[test]
    fn agent_autonomy_enabled_true_roundtrips_as_camel_case() {
        let settings = CollectionSettings {
            agent_autonomy_enabled: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        assert!(
            json.contains(r#""agentAutonomyEnabled":true"#),
            "expected camelCase agentAutonomyEnabled field, got {json}"
        );
        let round: CollectionSettings = serde_json::from_str(&json).expect("deserialize");
        assert!(round.agent_autonomy_enabled);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rocket-collection -j4 agent_autonomy_enabled`
Expected: FAIL to compile — `struct \`CollectionSettings\` has no field named \`agent_autonomy_enabled\``.

- [ ] **Step 3: Add the field to `CollectionSettings`**

In `crates/rocket-collection/src/settings.rs`, add to the struct (after `sandbox_mode`):

```rust
    /// JS sandbox capability level for scripts in this collection.
    #[serde(default)]
    pub sandbox_mode: SandboxMode,

    /// Whether the ACP AI-assist agent may run requests, edit scripts, and
    /// write non-secret env vars against this collection without further
    /// per-action confirmation. Defaults to `false` so an imported or freshly
    /// cloned collection never silently grants agent write access.
    #[serde(default)]
    pub agent_autonomy_enabled: bool,
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p rocket-collection -j4 agent_autonomy_enabled`
Expected: PASS (2 tests).

- [ ] **Step 5: Fix the six non-spreading struct literals so the workspace compiles**

In `crates/rocket-infra/src/fs_collection/settings.rs`, in `get_settings`'s `if let Some(defaults) = oc.request` branch, add the stub field after `sandbox_mode,`:

```rust
    if let Some(defaults) = oc.request {
        Ok(CollectionSettings {
            docs: oc.docs,
            auth: defaults.auth.map(rocket_shared::types::Auth::from),
            headers: defaults
                .headers
                .unwrap_or_default()
                .into_iter()
                .map(rocket_shared::types::Header::from)
                .collect(),
            variables: defaults
                .variables
                .unwrap_or_default()
                .into_iter()
                .map(CollectionVariable::from)
                .collect(),
            sandbox_mode,
            // Plan 02 wires this to the extensions.rocketapi.agentAutonomyEnabled
            // mapping (same pattern as sandbox_mode above); stubbed to the
            // type's own default here so CollectionSettings compiles with the
            // new field.
            agent_autonomy_enabled: false,
        })
    } else {
```

In `crates/rocket-infra/src/conversions/folder.rs`, in the equivalent `if let Some(defaults) = oc.request` branch, add the same stub after `sandbox_mode: SandboxMode::Safe,`:

```rust
    let settings = if let Some(defaults) = oc.request {
        CollectionSettings {
            docs: oc.docs,
            auth: defaults.auth.map(Auth::from),
            headers: defaults
                .headers
                .unwrap_or_default()
                .into_iter()
                .map(Header::from)
                .collect(),
            variables: defaults
                .variables
                .unwrap_or_default()
                .into_iter()
                .map(CollectionVariable::from)
                .collect(),
            sandbox_mode: SandboxMode::Safe,
            // See the matching comment in fs_collection/settings.rs::get_settings —
            // Plan 02 wires the real extensions mapping.
            agent_autonomy_enabled: false,
        }
    } else {
```

In `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`, add the field to the one full-literal test fixture:

```rust
        &CollectionSettings {
            docs: Some("docs".into()),
            auth: Some(Auth::None),
            headers: vec![Header::new("X-Tenant", "acme")],
            variables: vec![CollectionVariable {
                key: "base".into(),
                value: "https://x".into(),
                initial_value: "https://x".into(),
                enabled: true,
                secret: false,
            }],
            sandbox_mode: SandboxMode::Developer,
            agent_autonomy_enabled: false,
        },
```

In `crates/rocket-infra/src/fs_collection/tests.rs`, fix the three full-literal fixtures. First:

```rust
    let original = rocket_collection::CollectionSettings {
        docs: None,
        auth: Some(Auth::Bearer {
            token: "tok_abc".into(),
        }),
        headers: vec![Header::new("X-Tenant", "acme")],
        variables: vec![],
        sandbox_mode: SandboxMode::Safe,
        agent_autonomy_enabled: false,
    };
```

Second:

```rust
    let settings = rocket_collection::CollectionSettings {
        docs: None,
        auth: Some(Auth::None),
        headers: vec![],
        variables: vec![],
        sandbox_mode: SandboxMode::Safe,
        agent_autonomy_enabled: false,
    };
```

Third:

```rust
    let settings = CollectionSettings {
        docs: Some("My API docs".into()),
        auth: Some(Auth::Bearer {
            token: "tok".into(),
        }),
        headers: vec![Header::new("X-Tenant", "acme")],
        variables: vec![],
        sandbox_mode: SandboxMode::Safe,
        agent_autonomy_enabled: false,
    };
```

In `crates/rocket-app/src/execution_service.rs`, fix the one full-literal test fixture inside `execute_uses_collection_auth_when_request_auth_is_none`:

```rust
        let settings = CollectionSettings {
            docs: None,
            auth: Some(Auth::Bearer {
                token: "col_tok".into(),
            }),
            headers: vec![],
            variables: vec![],
            sandbox_mode: rocket_collection::settings::SandboxMode::Safe,
            agent_autonomy_enabled: false,
        };
```

- [ ] **Step 6: Run the affected crates' tests to verify they pass**

Run: `cargo test -p rocket-collection -p rocket-infra -p rocket-app -j4`
Expected: PASS (no regressions in any of the six touched files' existing tests).

- [ ] **Step 7: Run the full workspace check**

Run: `cargo check --workspace -j4`
Expected: PASS with no errors.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill to create the commit for:
- `crates/rocket-collection/src/settings.rs`
- `crates/rocket-infra/src/fs_collection/settings.rs`
- `crates/rocket-infra/src/fs_collection/tests.rs`
- `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`
- `crates/rocket-infra/src/conversions/folder.rs`
- `crates/rocket-app/src/execution_service.rs`

---

### Task 4: `save_request_script` — `RequestScriptPhase` and the full repository method (`rocket-collection` + `rocket-infra`)

**Files:**
- Modify: `crates/rocket-collection/src/repository.rs`
- Modify: `crates/rocket-collection/src/lib.rs`
- Modify: `crates/rocket-infra/src/fs_collection/variables.rs`
- Modify: `crates/rocket-infra/src/fs_collection/mod.rs`
- Modify: `crates/rocket-infra/src/fs_collection/tests.rs`
- Modify: `crates/rocket-infra/src/shared_path_collection_repo.rs`

**Interfaces:**
- Consumes: `crate::conversions::{oc_http_request_to_request, request_to_oc_http_request}` and `crate::oc::OcHttpRequest` (both already `pub(crate)`-visible inside `rocket-infra` and already used by `crates/rocket-infra/src/fs_collection/requests.rs`), `FsCollectionRepo::{collection_mutex, collection_path}`, `super::paths::resolve_request_path`, `crate::atomic_write` — all pre-existing.
- Produces: `rocket_collection::RequestScriptPhase` (`PreRequest` / `PostResponse` / `Tests`) and the new `CollectionRepository::save_request_script` method, fully implemented on both `FsCollectionRepo` and `SharedPathCollectionRepo`, consumed by Plan 03's `McpToolService::edit_script`.

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Why `RequestScriptPhase` is a local enum, not a reuse of `rocket-scripting::ScriptPhase`:** `rocket-scripting` depends on `rocket-environment` and `rocket-http` (see `crates/rocket-scripting/Cargo.toml`), while `rocket-collection` today depends on nothing but `rocket-shared` plus serialization crates (see `crates/rocket-collection/Cargo.toml`) — it sits at a lower layer. Adding a `rocket-collection → rocket-scripting` dependency would invert that direction just to reuse a 3-variant enum, and `ScriptPhase`'s variant names (`BeforeRequest`/`AfterResponse`/`Tests`) don't match the locked `RequestScriptPhase` name (`PreRequest`/`PostResponse`/`Tests`) the rest of this feature's plans already reference. A small local enum is the correct, boundary-respecting choice here.

**Why the infra implementation round-trips through `Request`, not `OcHttpRequest.runtime.scripts` directly:** the OpenCollection YAML does not store `pre_request_script`/`post_response_script`/`tests` as three direct fields — `crates/rocket-infra/src/oc/http.rs`'s `OcHttpRequest.runtime.scripts` is a `Vec<OcScript>` where each entry has a `script_type: String` (`"before-request"`/`"after-response"`/`"tests"`) and a `code: String`. `crates/rocket-infra/src/conversions/request.rs`'s `oc_http_request_to_request`/`request_to_oc_http_request` already implement the exact bidirectional mapping between that `Vec<OcScript>` shape and `Request`'s three `Option<String>` fields (`extract_scripts`, `request_to_oc_http_request`'s script-building block). Reusing that existing, already-tested round trip — parse YAML → convert to `Request` → mutate one field → convert back → serialize — is simpler and safer than re-deriving the `Vec<OcScript>` insert/replace logic a second time in `variables.rs`.

- [ ] **Step 1: Write the failing test for `RequestScriptPhase` and the trait method's object-safety**

Add to `crates/rocket-collection/src/repository.rs`'s `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn request_script_phase_is_copy_and_comparable() {
        let a = RequestScriptPhase::PreRequest;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(RequestScriptPhase::PreRequest, RequestScriptPhase::Tests);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rocket-collection -j4 request_script_phase_is_copy_and_comparable`
Expected: FAIL to compile — `cannot find type \`RequestScriptPhase\` in this scope`.

- [ ] **Step 3: Add `RequestScriptPhase` and the trait method**

In `crates/rocket-collection/src/repository.rs`, add the enum above the trait, and the method inside it, right after `save_request_variables`:

```rust
use rocket_shared::error::DomainResult;

use crate::collection::Collection;
use crate::request::Request;
use crate::settings::{CollectionSettings, CollectionVariable};
use crate::summary::CollectionSummary;

/// Identifies which of a request's three script fields `save_request_script`
/// targets. A local enum (not a reuse of `rocket-scripting::ScriptPhase`) —
/// see this task's doc comment in the plan for why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestScriptPhase {
    PreRequest,
    PostResponse,
    Tests,
}

/// Repository trait for Collection persistence.
/// Implemented by FsCollectionRepo in rocket-infra.
pub trait CollectionRepository: Send + Sync {
```

(everything else in the trait is unchanged up to `save_request_variables`; then add:)

```rust
    /// Persist request-level variables to a request .yml file's runtime.variables[].
    fn save_request_variables(
        &self,
        collection: &str,
        request_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()>;

    /// Overwrite one of a request's three script fields (pre-request,
    /// post-response, or tests) in place, leaving the other two and every
    /// other request field untouched. Deliberately not a full
    /// read-modify-write of the whole `Request` from a caller-supplied copy —
    /// that risks clobbering a concurrent manual edit to unrelated fields,
    /// and `Request` has no optimistic-concurrency mechanism to detect that.
    fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()>;
}
```

Re-export it from `crates/rocket-collection/src/lib.rs`:

```rust
pub use repository::{CollectionRepository, RequestScriptPhase};
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p rocket-collection -j4`
Expected: PASS (all tests, including `trait_is_object_safe` and the new `request_script_phase_is_copy_and_comparable`).

- [ ] **Step 5: Write the failing tests for `FsCollectionRepo::save_request_script`**

Add to `crates/rocket-infra/src/fs_collection/tests.rs`, after `save_request_preserves_variables_written_by_save_request_variables`:

```rust
#[test]
fn save_request_script_only_touches_the_targeted_phase() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let mut req = rocket_collection::Request::new("Get Users", HttpMethod::Get, "/users");
    req.pre_request_script = Some("console.log('pre');".into());
    req.post_response_script = Some("console.log('post');".into());
    req.tests = Some("rok.test('ok', () => {});".into());
    repo.save_request("my-api", "get-users.yml", &req)
        .expect("initial save");

    repo.save_request_script(
        "my-api",
        "get-users.yml",
        rocket_collection::RequestScriptPhase::PostResponse,
        "console.log('post-updated');".into(),
    )
    .expect("save_request_script");

    let loaded = repo.get_request("my-api", "get-users.yml").expect("load request");
    assert_eq!(
        loaded.pre_request_script,
        Some("console.log('pre');".to_string()),
        "an unrelated phase must not be touched"
    );
    assert_eq!(
        loaded.post_response_script,
        Some("console.log('post-updated');".to_string())
    );
    assert_eq!(
        loaded.tests,
        Some("rok.test('ok', () => {});".to_string()),
        "an unrelated phase must not be touched"
    );
}

#[test]
fn save_request_script_errors_for_a_missing_request() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");
    let err = repo
        .save_request_script(
            "my-api",
            "does-not-exist.yml",
            rocket_collection::RequestScriptPhase::Tests,
            "rok.test('x', () => {});".into(),
        )
        .expect_err("missing request file must error, not panic");
    // Matches save_request_variables's exact behavior for the identical
    // missing-file case: resolve_request_path returns a canonicalized path
    // under the (existing) collection directory even when the file itself
    // doesn't exist, so the failure surfaces from fs::read_to_string as an
    // io::Error, converted to DomainError::Io — not DomainError::NotFound.
    assert!(matches!(err, DomainError::Io(_)));
}
```

- [ ] **Step 6: Run the tests to verify they fail**

Run: `cargo test -p rocket-infra -j4 save_request_script`
Expected: FAIL to compile — `no method named \`save_request_script\` found for struct \`FsCollectionRepo\``.

- [ ] **Step 7: Implement `save_request_script` in `fs_collection/variables.rs`**

In `crates/rocket-infra/src/fs_collection/variables.rs`, add `RequestScriptPhase` to the existing `rocket_collection` import and add the imports needed for the round trip:

```rust
use std::fs;

use rocket_collection::{Collection, CollectionVariable, RequestScriptPhase};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;
use crate::conversions::{oc_http_request_to_request, request_to_oc_http_request};
use crate::oc::{
    OcFolder, OcFolderInfo, OcHttpRequest, OcHttpRequestRuntime, OcRequestDefaults, OcVariable,
};

use super::folder_file::{parse_folder_yml, read_folder_yml, write_folder_yml};
use super::paths::resolve_request_path;
use super::FsCollectionRepo;
```

Then add the function at the end of the file, after `save_request_variables`:

```rust
pub(super) fn save_request_script(
    repo: &FsCollectionRepo,
    collection: &str,
    request_path: &str,
    phase: RequestScriptPhase,
    body: String,
) -> DomainResult<()> {
    Collection::validate_name(collection)?;
    let mutex = repo.collection_mutex(collection);
    let _guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
    let collection_dir = repo.collection_path(collection);
    let file_path = resolve_request_path(repo, &collection_dir, request_path)?;
    let content = fs::read_to_string(&file_path)?;
    let oc: OcHttpRequest = serde_yaml::from_str(&content)
        .map_err(|e| DomainError::Internal(format!("Failed to parse request file: {e}")))?;

    // Round-trip through the domain Request rather than editing
    // runtime.scripts directly: oc_http_request_to_request/
    // request_to_oc_http_request already implement the exact bidirectional
    // mapping between the three Option<String> script fields and the OC
    // YAML's Vec<OcScript> shape (see this task's doc comment for why).
    let mut req = oc_http_request_to_request(oc);
    match phase {
        RequestScriptPhase::PreRequest => req.pre_request_script = Some(body),
        RequestScriptPhase::PostResponse => req.post_response_script = Some(body),
        RequestScriptPhase::Tests => req.tests = Some(body),
    }
    let oc = request_to_oc_http_request(&req);

    let yaml = serde_yaml::to_string(&oc)
        .map_err(|e| DomainError::Internal(format!("Failed to serialize request file: {e}")))?;
    atomic_write(&file_path, yaml.as_bytes())?;
    Ok(())
}
```

- [ ] **Step 8: Wire the new method into `FsCollectionRepo`'s trait impl**

In `crates/rocket-infra/src/fs_collection/mod.rs`, add `RequestScriptPhase` to the `rocket_collection` import:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    Request, RequestScriptPhase,
};
```

And add the trait method, after `save_request_variables`:

```rust
    fn save_request_variables(
        &self,
        collection: &str,
        request_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        variables::save_request_variables(self, collection, request_path, vars)
    }

    fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        variables::save_request_script(self, collection, request_path, phase, body)
    }
}
```

- [ ] **Step 9: Run the `FsCollectionRepo` tests to verify they pass**

Run: `cargo test -p rocket-infra -j4 save_request_script`
Expected: PASS (2 tests).

- [ ] **Step 10: Write the failing concurrency test on `SharedPathCollectionRepo`**

Add to `crates/rocket-infra/src/shared_path_collection_repo.rs`'s `#[cfg(test)] mod tests` block, after `concurrent_saves_to_same_collection_both_complete_without_error`:

```rust
    #[test]
    fn concurrent_save_request_script_calls_on_different_requests_both_complete_without_error() {
        // Mirrors concurrent_saves_to_same_collection_both_complete_without_error
        // above, for save_request_script's per-collection mutex instead of
        // save_request's.
        let (_dir, repo) = setup();
        repo.create("shared-col").unwrap();
        let req_a = rocket_collection::Request::new("Req A", HttpMethod::Get, "/a");
        let req_b = rocket_collection::Request::new("Req B", HttpMethod::Post, "/b");
        repo.save_request("shared-col", "req-a.yml", &req_a).unwrap();
        repo.save_request("shared-col", "req-b.yml", &req_b).unwrap();

        let repo = Arc::new(repo);
        let r1 = Arc::clone(&repo);
        let r2 = Arc::clone(&repo);

        let h1 = std::thread::spawn(move || {
            r1.save_request_script(
                "shared-col",
                "req-a.yml",
                rocket_collection::RequestScriptPhase::PreRequest,
                "console.log('a');".to_string(),
            )
        });
        let h2 = std::thread::spawn(move || {
            r2.save_request_script(
                "shared-col",
                "req-b.yml",
                rocket_collection::RequestScriptPhase::PostResponse,
                "console.log('b');".to_string(),
            )
        });

        h1.join()
            .unwrap()
            .expect("first concurrent save_request_script should succeed");
        h2.join()
            .unwrap()
            .expect("second concurrent save_request_script should succeed");

        let loaded_a = repo.get_request("shared-col", "req-a.yml").unwrap();
        let loaded_b = repo.get_request("shared-col", "req-b.yml").unwrap();
        assert_eq!(
            loaded_a.pre_request_script,
            Some("console.log('a');".to_string())
        );
        assert_eq!(
            loaded_b.post_response_script,
            Some("console.log('b');".to_string())
        );
    }
```

- [ ] **Step 11: Run the test to verify it fails**

Run: `cargo test -p rocket-infra -j4 concurrent_save_request_script_calls_on_different_requests_both_complete_without_error`
Expected: FAIL to compile — `no method named \`save_request_script\` found for struct \`SharedPathCollectionRepo\``.

- [ ] **Step 12: Wire the new method into `SharedPathCollectionRepo`'s trait impl**

In `crates/rocket-infra/src/shared_path_collection_repo.rs`, add `RequestScriptPhase` to the `rocket_collection` import:

```rust
use rocket_collection::{
    Collection, CollectionRepository, CollectionSettings, CollectionSummary, CollectionVariable,
    Request, RequestScriptPhase,
};
```

And delegate, after `save_request_variables`:

```rust
    fn save_request_variables(
        &self,
        collection: &str,
        request_path: &str,
        vars: Vec<CollectionVariable>,
    ) -> DomainResult<()> {
        self.repo()
            .save_request_variables(collection, request_path, vars)
    }

    fn save_request_script(
        &self,
        collection: &str,
        request_path: &str,
        phase: RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        self.repo()
            .save_request_script(collection, request_path, phase, body)
    }
}
```

- [ ] **Step 13: Run the test to verify it passes**

Run: `cargo test -p rocket-infra -j4 concurrent_save_request_script_calls_on_different_requests_both_complete_without_error`
Expected: PASS.

- [ ] **Step 14: Run the full workspace check**

Run: `cargo check --workspace -j4`
Expected: PASS with no errors.

- [ ] **Step 15: Commit**

Use the `dev-workflow-skills:1-git-commit` skill to create the commit for:
- `crates/rocket-collection/src/repository.rs`
- `crates/rocket-collection/src/lib.rs`
- `crates/rocket-infra/src/fs_collection/variables.rs`
- `crates/rocket-infra/src/fs_collection/mod.rs`
- `crates/rocket-infra/src/fs_collection/tests.rs`
- `crates/rocket-infra/src/shared_path_collection_repo.rs`

---

### Task 5: `rocket-history` — `HistoryEntry.run_source`

**Files:**
- Modify: `crates/rocket-history/src/entry.rs`

**Interfaces:**
- Consumes: `rocket_shared::RunSource` (Task 1).
- Produces: `HistoryEntry.run_source: rocket_shared::RunSource` (`#[serde(default)]`) and a `with_run_source` builder method, consumed by Plan 03 when `RequestExecutionService` threads `ExecuteRequestInput.run_source` into the `HistoryEntry` it builds.

- [ ] **Step 1: Write the failing tests**

Add to `crates/rocket-history/src/entry.rs`'s `#[cfg(test)] mod tests` block, after `two_entries_have_distinct_ids`:

```rust
    #[test]
    fn new_entry_defaults_run_source_to_manual() {
        let entry = HistoryEntry::new("GET", "https://api.example.com", 200, 150, 1024);
        assert_eq!(entry.run_source, rocket_shared::RunSource::Manual);
    }

    #[test]
    fn with_run_source_overrides_the_default() {
        let entry = HistoryEntry::new("GET", "/", 200, 10, 0)
            .with_run_source(rocket_shared::RunSource::Agent);
        assert_eq!(entry.run_source, rocket_shared::RunSource::Agent);
    }

    #[test]
    fn old_json_without_run_source_deserializes_to_manual() {
        // Backward compat: a HistoryEntry persisted before this field existed
        // must still load, defaulting to Manual rather than failing.
        let json = r#"{"id":"1","method":"GET","url":"/","status":200,"durationMs":10,"responseSize":0,"timestamp":"2024-01-01T00:00:00Z","collection":null,"requestName":null}"#;
        let entry: HistoryEntry = serde_json::from_str(json).expect("deserialize");
        assert_eq!(entry.run_source, rocket_shared::RunSource::Manual);
    }

    #[test]
    fn run_source_serializes_as_camel_case_run_source_key() {
        let entry = HistoryEntry::new("GET", "/", 200, 10, 0)
            .with_run_source(rocket_shared::RunSource::LoadTest);
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(
            json.contains(r#""runSource":"load_test""#),
            "expected camelCase runSource field with snake_case value, got {json}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p rocket-history -j4 run_source`
Expected: FAIL to compile — `no field \`run_source\` on type \`HistoryEntry\`` / `no method named \`with_run_source\` found`.

- [ ] **Step 3: Implement the field and builder**

In `crates/rocket-history/src/entry.rs`, add the field:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub duration_ms: u64,
    pub response_size: usize,
    pub timestamp: DateTime<Utc>,
    pub collection: Option<String>,
    pub request_name: Option<String>,
    /// Distinguishes a manual send from a Collection Runner step, a load
    /// test, a Flow node, or an agent-driven tool call. Defaults to `Manual`
    /// so a `HistoryEntry` persisted before this field existed still
    /// deserializes correctly.
    #[serde(default)]
    pub run_source: rocket_shared::RunSource,
}
```

Set it to `Manual` in `new` (matching `RunSource`'s own `#[default]`, spelled out explicitly here since `new` already builds every field explicitly rather than via `..Default::default()`):

```rust
    pub fn new(
        method: impl Into<String>,
        url: impl Into<String>,
        status: u16,
        duration_ms: u64,
        response_size: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: method.into(),
            url: url.into(),
            status,
            duration_ms,
            response_size,
            timestamp: Utc::now(),
            collection: None,
            request_name: None,
            run_source: rocket_shared::RunSource::Manual,
        }
    }
```

Add the builder method, after `with_collection` (arity of `new` is unchanged, matching the locked contract):

```rust
    pub fn with_collection(
        mut self,
        collection: impl Into<String>,
        request_name: impl Into<String>,
    ) -> Self {
        self.collection = Some(collection.into());
        self.request_name = Some(request_name.into());
        self
    }

    /// Builder method: tag this entry with how its execution was triggered.
    pub fn with_run_source(mut self, source: rocket_shared::RunSource) -> Self {
        self.run_source = source;
        self
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p rocket-history -j4`
Expected: PASS (all tests in this file, including the 4 new ones).

- [ ] **Step 5: Run the full workspace check and the plan's complete verification command**

Run: `cargo check --workspace -j4`
Expected: PASS with no errors.

Run: `cargo test -p rocket-shared -p rocket-acp -p rocket-collection -p rocket-history -p rocket-infra -p rocket-app -j4`
Expected: PASS — every test across all six crates touched by this plan, with no regressions.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill to create the commit for:
- `crates/rocket-history/src/entry.rs`

---

## Next Plan

**Plan 02 — Infra implementations** (`docs/superpowers/plans/acp-mcp-tool-server/2026-09-28-acp-mcp-tool-server-plan-02-infra-implementations.md`, not yet written): builds on this plan's contracts inside `rocket-infra`. It maps `McpServerSpec` into the real `agent_client_protocol::McpServer` and threads it into `NewSessionRequest` in `AcpAgentClient::start_session` (replacing this plan's `let _ = mcp_servers;` stub), reads `InitializeResponse.agent_capabilities.mcp_capabilities.http` to choose `McpServerSpec::Http` vs `::Stdio`, and wires the real `extensions.rocketapi.agentAutonomyEnabled` ↔ `CollectionSettings.agent_autonomy_enabled` persistence mapping in `fs_collection/settings.rs` and `conversions/folder.rs` (replacing this plan's `agent_autonomy_enabled: false` stubs), mirroring the existing `sandbox_mode` pattern exactly.

## Post-Implementation Review

After every task above is checked off and `cargo test -p rocket-shared -p rocket-acp -p rocket-collection -p rocket-history -p rocket-infra -p rocket-app -j4` and `cargo check --workspace -j4` are both green, dispatch a review subagent:

```
Agent({
  subagent_type: "general-purpose",
  model: "opus",
  description: "Plan 01 domain-contracts review",
  prompt: "Review the full diff this plan produced (`git diff main...HEAD` or the
    equivalent range covering all 5 tasks of
    docs/superpowers/plans/acp-mcp-tool-server/2026-09-28-acp-mcp-tool-server-plan-01-domain-contracts.md).
    Read that plan file and docs/superpowers/plans/acp-mcp-tool-server/00-plan-index.md
    in full first. You have authority to fix anything you find directly (edit
    files, re-run cargo check/test, commit via the dev-workflow-skills:1-git-commit
    skill) rather than only reporting it. Check specifically for:
    (1) Interface gaps against the plan index's locked signatures — RunSource,
    McpServerSpec, AcpSessionClient::start_session's new parameter,
    CollectionSettings.agent_autonomy_enabled, RequestScriptPhase,
    CollectionRepository::save_request_script, and HistoryEntry.run_source /
    with_run_source — do the actual shipped types/signatures match the index
    byte-for-byte in field names, variant names, and parameter order?
    (2) Code quality — comments follow this repo's short-full-sentence
    convention, no leftover TODO/placeholder code, no unwrap() in production
    paths.
    (3) Code duplication — did save_request_script's round-trip-through-Request
    approach actually avoid duplicating oc_http_request_to_request /
    request_to_oc_http_request's logic, or did a parallel implementation creep
    in anywhere?
    (4) DDD boundary conformance — rocket-collection still has no dependency on
    rocket-scripting, rocket-acp still has no dependency on agent-client-protocol,
    rocket-shared still has no dependency on any other workspace crate.
    Report what you found and what you fixed, in under 400 words."
})
```
