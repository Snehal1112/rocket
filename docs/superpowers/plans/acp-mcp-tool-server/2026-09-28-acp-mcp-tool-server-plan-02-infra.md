# ACP MCP Tool Server — Plan 02: Infra Implementations

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the `rocket-infra` side of the ACP MCP tool server subproject: ground the real `rmcp`/`axum` versions later plans need (without adding either as a `rocket-infra` dependency — see Task 2), persist the new `agent_autonomy_enabled` collection setting, and make `AcpAgentClient` actually attach MCP servers to a session instead of discarding the negotiated capability and ignoring the new `mcp_servers` parameter.

**Architecture:** Three independent, narrowly-scoped changes, each leaving the workspace compiling and green on its own: (1) ground the real, current `rmcp`/`axum` versions via a dry-run `cargo add` — **no dependency is added to `crates/rocket-infra/Cargo.toml`**, since nothing in this plan's own scope (or `rocket-infra` generally) uses either crate; Plan 04 adds the real, kept dependency to `src-tauri/Cargo.toml` instead (see Task 2's cross-plan correction note); (2) extend the existing `sandbox_mode` ↔ `extensions.rocketapi` YAML mapping pattern in `fs_collection/settings.rs` to cover the new `agent_autonomy_enabled` field; (3) make `AcpAgentClient::start_session` map the real (Plan-01-defined) `rocket_acp::McpServerSpec` list into `agent_client_protocol::schema::v1::McpServer` values and attach them to the `NewSessionRequest`, and read (without acting on) the agent's negotiated `mcp_capabilities.http` flag.

**Tech Stack:** Rust, `agent-client-protocol`/`agent-client-protocol-schema` 2.2/1.9 (already a dependency, unchanged here), `serde_yaml`, `tokio`, `tempfile` (dev-dependency, integration tests). `rmcp` 3.5 (official Rust MCP SDK) and `axum` 0.8 are grounded (version-confirmed) by this plan's Task 2 but land as a real dependency only in `src-tauri/Cargo.toml` (Plan 04), not here.

**Spec:** [`docs/superpowers/specs/2026-09-28-acp-mcp-tool-server-design.md`](../../specs/2026-09-28-acp-mcp-tool-server-design.md)

**Plan index (locked interface contracts this plan depends on):** [`docs/superpowers/plans/acp-mcp-tool-server/00-plan-index.md`](00-plan-index.md)

## Global Constraints

- Every `cargo` invocation in this repo uses `-j4` (project convention — see `crates/*/CLAUDE.md` and prior session memory).
- Rust: never call `.unwrap` on a `Result`/`Option` in a production path (root `CLAUDE.md` Hard Rules). Test-only code (the fixture agent binary, `#[cfg(test)]` modules, integration tests) may use `.expect(...)` with a message, matching this file's existing style, but production code in `src/acp_agent_client.rs` and `src/fs_collection/settings.rs` must map errors explicitly instead.
- Rust: never shell out to the `git` CLI (not touched by this plan; listed for completeness per root `CLAUDE.md`).
- Serde: `#[serde(rename_all = "camelCase")]` only on IPC DTOs, never on persistence structs (root `CLAUDE.md` Hard Rules). The `agent_client_protocol_schema` wire types this plan maps into (`McpServer`, `McpServerHttp`, `McpServerStdio`, `HttpHeader`, `EnvVariable`) already use `camelCase` as part of that third-party crate's own wire format — this is untouched, upstream behavior, not a Rocket persistence or IPC struct, so the rule does not apply to it.
- `agent-client-protocol = "2.2"` is already pinned in `crates/rocket-infra/Cargo.toml`; this plan does not change it.
- `rmcp`'s current published version is `3.5.0` and `axum`'s is `0.8.9`, both confirmed via `cargo add --dry-run` against the live crates.io index as part of this plan's research (not guessed).
- Commits: conventional commits format (`feat:`, `fix:`, `chore:`, etc.), created by invoking the `dev-workflow-skills:1-git-commit` skill — per this user's global instructions, never a freeform `git commit -m "..."` message, in this project or any other.
- `#[non_exhaustive]` structs in `agent-client-protocol-schema` (`HttpHeader`, `EnvVariable`, `McpServerHttp`, `McpServerStdio`, `AgentCapabilities`, `McpCapabilities`, `InitializeResponse`) cannot be constructed with struct-literal syntax outside their defining crate — every value of these types in this plan is built through their `::new(...)` constructor plus builder methods (`.headers(...)`, `.args(...)`, `.env(...)`, `.mcp_capabilities(...)`, `.http(...)`), confirmed by reading `agent-client-protocol-schema-1.9.1/src/v1/agent.rs` directly rather than assumed.

## Review Focus

- `agentAutonomyEnabled` absent from a hand-authored or pre-existing `opencollection.yml` must load as `false`, not error and not `true` — a collection must opt in explicitly. Covered by Task 1's `agent_autonomy_enabled_from_extensions_none_defaults_to_false` and `settings_agent_autonomy_enabled_defaults_to_false_without_rocketapi_extension`.
- A malformed value for `agentAutonomyEnabled` in hand-edited YAML (e.g. the string `"yes"` instead of a bool) must fall back to `false`, not panic or bubble a parse error up through `get_settings`. Covered by Task 1's `agent_autonomy_enabled_from_extensions_non_bool_value_falls_back_to_false`.
- Writing `sandboxMode` and `agentAutonomyEnabled` together (in either order) must not clobber each other or any pre-existing sibling key under `extensions`/`extensions.rocketapi`. Covered by Task 1's `set_sandbox_mode_then_agent_autonomy_enabled_both_persist_together` and `save_settings_sandbox_mode_and_agent_autonomy_enabled_persist_together`.
- An MCP HTTP server's bearer token must never leak outside the one `Authorization` header value it belongs in — not into a `start_session` error message (e.g. a failed spawn), and not into any other field of the mapped `McpServer::Http`. Covered by Task 3's `acp_agent_client_start_session_error_never_contains_the_mcp_token_value` and the header-shape assertions in `acp_agent_client_start_session_maps_http_mcp_server_spec_into_new_session_request`.
- A capability mismatch — the caller offers both an `Http` and a `Stdio` `McpServerSpec` for the same server, but the agent's `InitializeResponse` never advertised `mcp_capabilities.http` — must not block or fail session start.

  **Correction (found and fixed during this plan's Post-Implementation Review, commit `c9a85a9e`):** this bullet originally said choosing which `McpServerSpec` variant to send was `rocket-app`'s job and that `rocket-infra` should only map-and-warn on a mismatch. That contradicted the locked plan index, the design spec's "Capability negotiation" section, and Plan 05's own text (which already assumed `AcpAgentClient` does the selection) — `rocket-app` (Plan 03) always offers both specs under the same name; `AcpAgentClient::start_session` is where the actual Http-vs-Stdio choice happens, via `select_mcp_servers_for_agent` (replacing the originally-planned `warn_if_http_mcp_server_unsupported`, which never blocked but also never chose). Covered by Task 3's `acp_agent_client_start_session_still_succeeds_when_agent_lacks_http_mcp_capability` plus two tests added by the fix: one confirming Http is chosen when advertised, one confirming Stdio is chosen (and the token never appears) when it isn't.

---

### Task 1: `CollectionSettings` persistence mapping for `agent_autonomy_enabled`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/fs_collection/settings.rs:14-104` (read functions + `get_settings`), `:106-194` (`save_settings`), `:196-241` (test module)
- Modify: `crates/rocket-infra/src/fs_collection/tests.rs` (append new integration tests after `save_settings_preserves_unrelated_extensions_data`, currently ending around line 295)

**Interfaces:**
- Consumes: `rocket_collection::CollectionSettings.agent_autonomy_enabled: bool` (added by Plan 01, `#[serde(default)]`, defaults `false` via `Default` derive) and `rocket_collection::settings::SandboxMode` (existing). `FsCollectionRepo::get_settings`/`save_settings` (existing trait methods, unchanged signatures).
- Produces: two new private functions in `fs_collection/settings.rs` — `fn agent_autonomy_enabled_from_extensions(extensions: &Option<serde_yaml::Value>) -> bool` and `fn set_agent_autonomy_enabled_in_extensions(extensions: Option<serde_yaml::Value>, enabled: bool) -> Option<serde_yaml::Value>` — used only inside this file (mirrors `sandbox_mode_from_extensions`/`set_sandbox_mode_in_extensions`, which stay unchanged and are reused as-is by later tests).

- [ ] **Step 1: Write the failing unit tests in `settings.rs`**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/rocket-infra/src/fs_collection/settings.rs` (after `set_sandbox_mode_in_extensions_preserves_sibling_keys`):

```rust
    #[test]
    fn agent_autonomy_enabled_from_extensions_none_defaults_to_false() {
        assert!(!agent_autonomy_enabled_from_extensions(&None));
    }

    #[test]
    fn agent_autonomy_enabled_from_extensions_reads_existing_rocketapi_mapping_with_other_keys() {
        let yaml = "rocketapi:\n  agentAutonomyEnabled: true\n  someOtherField: 1\nunrelatedTool:\n  foo: bar\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        assert!(agent_autonomy_enabled_from_extensions(&Some(value)));
    }

    #[test]
    fn agent_autonomy_enabled_from_extensions_non_bool_value_falls_back_to_false() {
        let yaml = "rocketapi:\n  agentAutonomyEnabled: \"yes\"\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        assert!(!agent_autonomy_enabled_from_extensions(&Some(value)));
    }

    #[test]
    fn set_agent_autonomy_enabled_in_extensions_preserves_sibling_keys() {
        let yaml = "someOtherTool:\n  foo: bar\nrocketapi:\n  unrelatedFlag: true\n";
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse fixture yaml");
        let result = set_agent_autonomy_enabled_in_extensions(Some(value), true)
            .expect("extensions value");

        assert!(agent_autonomy_enabled_from_extensions(&Some(result.clone())));
        let serialized = serde_yaml::to_string(&result).expect("serialize extensions");
        assert!(serialized.contains("someOtherTool"));
        assert!(serialized.contains("foo: bar"));
        assert!(serialized.contains("unrelatedFlag: true"));
    }

    #[test]
    fn set_sandbox_mode_then_agent_autonomy_enabled_both_persist_together() {
        let extensions = set_sandbox_mode_in_extensions(None, SandboxMode::Developer);
        let extensions = set_agent_autonomy_enabled_in_extensions(extensions, true);
        assert_eq!(
            sandbox_mode_from_extensions(&extensions),
            SandboxMode::Developer
        );
        assert!(agent_autonomy_enabled_from_extensions(&extensions));
    }
```

Then add these integration-level tests to `crates/rocket-infra/src/fs_collection/tests.rs`, after `save_settings_preserves_unrelated_extensions_data`:

```rust
#[test]
fn settings_agent_autonomy_enabled_roundtrips() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    let settings = CollectionSettings {
        agent_autonomy_enabled: true,
        ..Default::default()
    };
    repo.save_settings("my-api", &settings)
        .expect("save settings");

    let loaded = repo.get_settings("my-api").expect("get settings");
    assert!(loaded.agent_autonomy_enabled);
}

#[test]
fn settings_agent_autonomy_enabled_defaults_to_false_without_rocketapi_extension() {
    let (dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    let path = dir.path().join("my-api/opencollection.yml");
    fs::write(&path, "opencollection: \"1.0.0\"\ninfo:\n  name: my-api\n").expect("write fixture");

    let loaded = repo.get_settings("my-api").expect("get settings");
    assert!(!loaded.agent_autonomy_enabled);
}

#[test]
fn save_settings_sandbox_mode_and_agent_autonomy_enabled_persist_together() {
    let (_dir, repo) = setup();
    repo.create("my-api").expect("create collection");

    let settings = CollectionSettings {
        sandbox_mode: SandboxMode::Developer,
        agent_autonomy_enabled: true,
        ..Default::default()
    };
    repo.save_settings("my-api", &settings)
        .expect("save settings");

    let loaded = repo.get_settings("my-api").expect("get settings");
    assert_eq!(loaded.sandbox_mode, SandboxMode::Developer);
    assert!(loaded.agent_autonomy_enabled);
}
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p rocket-infra -j4 agent_autonomy_enabled`

Expected: the four unit tests in `settings.rs` fail to **compile** (`error[E0425]: cannot find function` for `agent_autonomy_enabled_from_extensions` and the same for `set_agent_autonomy_enabled_in_extensions`). The three integration tests in `tests.rs` compile but `settings_agent_autonomy_enabled_roundtrips` and `save_settings_sandbox_mode_and_agent_autonomy_enabled_persist_together` fail their `assert!`/`assert_eq!` calls (the field always reads back `false` because nothing persists it yet).

- [ ] **Step 3: Implement the read/write mapping functions**

In `crates/rocket-infra/src/fs_collection/settings.rs`, add these two functions immediately after `set_sandbox_mode_in_extensions` (i.e. right before `pub(super) fn get_settings`):

```rust
/// Reads `agent_autonomy_enabled` out of `opencollection.yml`'s free-form `extensions` field
/// (`extensions.rocketapi.agentAutonomyEnabled`), mirroring `sandbox_mode_from_extensions`
/// above exactly. Missing or non-boolean values default to `false` -- a collection must opt
/// in explicitly to agent write access; it is never autonomous by default.
fn agent_autonomy_enabled_from_extensions(extensions: &Option<serde_yaml::Value>) -> bool {
    extensions
        .as_ref()
        .and_then(|v| v.get("rocketapi"))
        .and_then(|v| v.get("agentAutonomyEnabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Writes `agent_autonomy_enabled` into `extensions.rocketapi.agentAutonomyEnabled`, preserving
/// any other keys already present under `extensions` or under `extensions.rocketapi` -- mirrors
/// `set_sandbox_mode_in_extensions` exactly, so the two settings can be written in either order
/// (or the same call) without clobbering each other or unrelated tooling's data.
fn set_agent_autonomy_enabled_in_extensions(
    extensions: Option<serde_yaml::Value>,
    enabled: bool,
) -> Option<serde_yaml::Value> {
    let mut root = match extensions {
        Some(serde_yaml::Value::Mapping(map)) => map,
        _ => serde_yaml::Mapping::new(),
    };

    let rocketapi_key = serde_yaml::Value::String("rocketapi".into());
    let mut rocketapi = match root.get(&rocketapi_key) {
        Some(serde_yaml::Value::Mapping(map)) => map.clone(),
        _ => serde_yaml::Mapping::new(),
    };
    rocketapi.insert(
        serde_yaml::Value::String("agentAutonomyEnabled".into()),
        serde_yaml::Value::Bool(enabled),
    );
    root.insert(rocketapi_key, serde_yaml::Value::Mapping(rocketapi));

    Some(serde_yaml::Value::Mapping(root))
}
```

Then wire both into `get_settings` — replace:

```rust
    let sandbox_mode = sandbox_mode_from_extensions(&oc.extensions);

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
        })
    } else {
        Ok(CollectionSettings {
            docs: oc.docs,
            sandbox_mode,
            ..CollectionSettings::default()
        })
    }
```

with:

```rust
    let sandbox_mode = sandbox_mode_from_extensions(&oc.extensions);
    let agent_autonomy_enabled = agent_autonomy_enabled_from_extensions(&oc.extensions);

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
            agent_autonomy_enabled,
        })
    } else {
        Ok(CollectionSettings {
            docs: oc.docs,
            sandbox_mode,
            agent_autonomy_enabled,
            ..CollectionSettings::default()
        })
    }
```

And in `save_settings`, replace:

```rust
    oc.docs = settings.docs.clone();
    oc.extensions = set_sandbox_mode_in_extensions(oc.extensions.take(), settings.sandbox_mode);
```

with:

```rust
    oc.docs = settings.docs.clone();
    let extensions = set_sandbox_mode_in_extensions(oc.extensions.take(), settings.sandbox_mode);
    oc.extensions =
        set_agent_autonomy_enabled_in_extensions(extensions, settings.agent_autonomy_enabled);
```

Note: Plan 01 is responsible for updating any existing exhaustive `CollectionSettings { ... }` struct literals elsewhere in the workspace (e.g. `settings_roundtrip`, `settings_file_not_counted_as_request`, `settings_stored_in_opencollection_yml` in `crates/rocket-infra/src/fs_collection/tests.rs`, which list every field by name with no `..Default::default()`) to include `agent_autonomy_enabled`, since adding the field is what breaks them, not this task's change. If `cargo check` in Step 5 below reports a missing-field error in one of those pre-existing literals, add `agent_autonomy_enabled: false,` to it as a minimal compiling fix and note this in the task's commit message body.

- [ ] **Step 4: Run the tests again to confirm they pass**

Run: `cargo test -p rocket-infra -j4 agent_autonomy_enabled`
Expected: all seven tests (four unit, three integration) pass.

- [ ] **Step 5: Run the full workspace check**

Run: `cargo check --workspace -j4`
Expected: no errors.

- [ ] **Step 6: Commit**

Stage `crates/rocket-infra/src/fs_collection/settings.rs` and `crates/rocket-infra/src/fs_collection/tests.rs` (plus any minimal fixup from Step 3's note, if needed). Invoke the `dev-workflow-skills:1-git-commit` skill (`Skill` tool, skill name `dev-workflow-skills:1-git-commit`) to draft and create the commit — per this user's global instructions, do not write a freeform `git commit -m "..."` message for this or any other commit in this plan.

---

### Task 2: `rmcp`/`axum` version grounding (no dependency added to `rocket-infra`)

**Cross-plan correction (found while reconciling this plan with Plans 04/05):** this task originally added `rmcp`/`axum` as direct dependencies of `crates/rocket-infra/Cargo.toml`, on the assumption (inherited from the spec's prose) that the in-process MCP HTTP server and Stdio bridge live in this crate. They do not — the plan index locks both `src-tauri/src/mcp/tool_server.rs` (Plan 04) and `src-tauri/src/mcp/stdio_bridge.rs` (Plan 05), and Plan 04's own Global Constraints explicitly correct the spec's paragraph and add `rmcp`/`axum` to **`src-tauri/Cargo.toml`** instead. Nothing else in this plan's scope (Task 1's `CollectionSettings` persistence mapping, Task 3's `AcpAgentClient` MCP-server attachment) uses `rmcp` or `axum` either — Task 3 maps `McpServerSpec` into `agent_client_protocol::schema::v1::McpServer` and friends, a different crate entirely. Adding `rmcp`/`axum` to `rocket-infra/Cargo.toml` would be dead weight (an unused dependency violating this crate's own DDD boundary — `rocket-infra` has no code that touches either crate) and would risk a duplicate, differently-versioned `rmcp` entry once Plan 04 also declares one in `src-tauri/Cargo.toml`.

**This task is reduced to a research/grounding step only — it makes no `Cargo.toml` edit.** Its sole deliverable, still useful to the rest of this series, is confirming the real, current `rmcp`/`axum` versions so Plans 04/05 do not have to guess:

- [ ] **Step 1: Confirm the real current versions via a dry-run `cargo add` (no file changes kept)**

Run, from the workspace root, in a scratch location that does not modify any tracked `Cargo.toml` (e.g. `cargo add rmcp axum --dry-run -p rocket-infra`, then discard any resulting changes):

```bash
cargo add rmcp --dry-run -p rocket-infra
cargo add axum --dry-run -p rocket-infra
```

Expected: the dry run reports the versions it would add — confirmed at plan-authoring time as `rmcp` `3.5.0` and `axum` `0.8.9` against the live crates.io index. Record these two version numbers; Plan 04's Task 1 (`src-tauri/Cargo.toml`) is where the real, kept dependency declaration lands, pinned to `rmcp = "3.5"` with the union of features both Plan 04 (HTTP server) and Plan 05 (stdio bridge) need, plus `axum = "0.8"`.

- [ ] **Step 2: Run the full workspace check to confirm nothing changed**

Run: `cargo check --workspace -j4`
Expected: no errors, no diff to any `Cargo.toml`/`Cargo.lock` from this task (the dry run in Step 1 must not have left anything staged).

(Steps 1-2 above are the whole of this task now — see the correction note. There is nothing to commit for this task; it changes no tracked file.)

---

### Task 3: `AcpAgentClient` real MCP-server attachment

**Files:**
- Modify: `crates/rocket-infra/src/acp_agent_client.rs:11-22` (imports), `:183-189` (`start_session` signature — as landed by Plan 01, which added the `mcp_servers: &[rocket_acp::McpServerSpec]` parameter but left it unused; if Plan 01 named it `_mcp_servers` to silence the unused-parameter warning, rename it to `mcp_servers` here since it is no longer unused), `:233-234` (owned-copy prep before `tokio::spawn`), `:267-289` (the handshake closure), and adds two new free functions near `stop_reason_to_wire_string` (currently `:505-514`)
- Modify: `crates/rocket-infra/src/bin/test_acp_agent.rs` (test fixture: add two test hooks)
- Modify: `crates/rocket-infra/tests/acp_agent_client.rs` (new integration tests, appended)

**Interfaces:**
- Consumes: `rocket_acp::McpServerSpec` (Plan 01: `pub enum McpServerSpec { Http { name: String, url: String, token: String }, Stdio { name: String, command: String, args: Vec<String>, env: Vec<(String, String)> } }`, `#[derive(Debug, Clone)]`, not `#[non_exhaustive]`); `agent_client_protocol::schema::v1::{McpServer, McpServerHttp, McpServerStdio, HttpHeader, EnvVariable, InitializeResponse}` (confirmed by reading `agent-client-protocol-schema-1.9.1/src/v1/agent.rs`: `McpServer` is `#[serde(tag = "type", rename_all = "snake_case")]` with `Http(McpServerHttp)`/`Sse(McpServerSse)` tagged and `#[serde(untagged)] Stdio(McpServerStdio)` untagged; `McpServerHttp::new(name, url).headers(Vec<HttpHeader>)`; `McpServerStdio::new(name, command).args(Vec<String>).env(Vec<EnvVariable>)`; `HttpHeader::new(name, value)`; `EnvVariable::new(name, value)`; all five are `#[non_exhaustive]`, so only their `::new`/builder methods are usable outside the defining crate; `InitializeResponse.agent_capabilities.mcp_capabilities.http: bool`, a plain field, not a method).
- Produces: `fn mcp_server_specs_to_wire(specs: &[McpServerSpec]) -> Vec<McpServer>` and `fn warn_if_http_mcp_server_unsupported(mcp_servers: &[McpServerSpec], agent_supports_http: bool)`, both private to `acp_agent_client.rs`. `NewSessionRequest` sent during `start_session`'s handshake now carries `.mcp_servers(...)` built from the caller's `mcp_servers` argument, instead of always being empty.

- [ ] **Step 1: Extend the fixture agent binary with two test hooks**

In `crates/rocket-infra/src/bin/test_acp_agent.rs`, add `McpCapabilities` to the import list — replace:

```rust
use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
```

with:

```rust
use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    McpCapabilities, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
    SessionId, SessionNotification, SessionUpdate, StopReason, TextContent,
};
```

Then replace the `InitializeRequest` and `NewSessionRequest` handlers — currently:

```rust
        .on_receive_request(
            async move |req: InitializeRequest, responder, _conn: ConnectionTo<Client>| {
                responder.respond(
                    InitializeResponse::new(req.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: NewSessionRequest,
                        responder: Responder<NewSessionResponse>,
                        _conn: ConnectionTo<Client>| {
                responder.respond(NewSessionResponse::new(SessionId::new("fixture-session")))
            },
            agent_client_protocol::on_receive_request!(),
        )
```

with:

```rust
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
                responder.respond(
                    InitializeResponse::new(req.protocol_version).agent_capabilities(
                        AgentCapabilities::new().mcp_capabilities(mcp_capabilities),
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
                responder.respond(NewSessionResponse::new(SessionId::new("fixture-session")))
            },
            agent_client_protocol::on_receive_request!(),
        )
```

- [ ] **Step 2: Write the failing integration tests**

Append to `crates/rocket-infra/tests/acp_agent_client.rs`. First widen the import (replace `use rocket_acp::AcpSessionClient;` with `use rocket_acp::{AcpSessionClient, McpServerSpec};`), then add:

```rust
// Task 3 (Plan 02): McpServerSpec -> agent_client_protocol::McpServer mapping.

#[tokio::test]
async fn acp_agent_client_start_session_maps_http_mcp_server_spec_into_new_session_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dump_path = dir.path().join("mcp_servers.json");

    let client = AcpAgentClient::new();
    let specs = vec![McpServerSpec::Http {
        name: "rocket-tools".to_string(),
        url: "http://127.0.0.1:4000/mcp".to_string(),
        token: "secret-token".to_string(),
    }];
    client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[(
                "MCP_SERVERS_DUMP_PATH".to_string(),
                dump_path.display().to_string(),
            )],
            &specs,
        )
        .await
        .expect("start_session should succeed against the fixture agent");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump mcp_servers");
    assert!(dumped.contains("\"type\":\"http\""), "got: {dumped}");
    assert!(dumped.contains("\"name\":\"rocket-tools\""), "got: {dumped}");
    assert!(
        dumped.contains("\"url\":\"http://127.0.0.1:4000/mcp\""),
        "got: {dumped}"
    );
    assert!(dumped.contains("\"name\":\"Authorization\""), "got: {dumped}");
    assert!(
        dumped.contains("\"value\":\"Bearer secret-token\""),
        "got: {dumped}"
    );
}

#[tokio::test]
async fn acp_agent_client_start_session_maps_stdio_mcp_server_spec_into_new_session_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dump_path = dir.path().join("mcp_servers.json");

    let client = AcpAgentClient::new();
    let specs = vec![McpServerSpec::Stdio {
        name: "rocket-tools-stdio".to_string(),
        command: "rocket".to_string(),
        args: vec!["--acp-mcp-stdio-bridge".to_string()],
        env: vec![("ROCKET_MCP_PORT".to_string(), "4000".to_string())],
    }];
    client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[(
                "MCP_SERVERS_DUMP_PATH".to_string(),
                dump_path.display().to_string(),
            )],
            &specs,
        )
        .await
        .expect("start_session should succeed against the fixture agent");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump mcp_servers");
    assert!(!dumped.contains("\"type\":\"http\""), "got: {dumped}");
    assert!(dumped.contains("\"command\":\"rocket\""), "got: {dumped}");
    assert!(dumped.contains("--acp-mcp-stdio-bridge"), "got: {dumped}");
    assert!(dumped.contains("\"name\":\"ROCKET_MCP_PORT\""), "got: {dumped}");
    assert!(dumped.contains("\"value\":\"4000\""), "got: {dumped}");
}

#[tokio::test]
async fn acp_agent_client_start_session_with_no_mcp_servers_sends_an_empty_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dump_path = dir.path().join("mcp_servers.json");

    let client = AcpAgentClient::new();
    client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[(
                "MCP_SERVERS_DUMP_PATH".to_string(),
                dump_path.display().to_string(),
            )],
            &[],
        )
        .await
        .expect("start_session should succeed with no mcp servers");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump mcp_servers");
    assert_eq!(dumped.trim(), "[]");
}

#[tokio::test]
async fn acp_agent_client_start_session_still_succeeds_when_agent_lacks_http_mcp_capability() {
    // The fixture agent's InitializeResponse never sets mcp_capabilities.http
    // unless FIXTURE_ADVERTISE_MCP_HTTP=1 is set (see test_acp_agent.rs),
    // which this test does not set. Deciding which McpServerSpec variant to
    // send is rocket-app's job (Plan 03); this crate only maps and warns on
    // a mismatch, it never blocks the session over it.
    let client = AcpAgentClient::new();
    let specs = vec![McpServerSpec::Http {
        name: "rocket-tools".to_string(),
        url: "http://127.0.0.1:4000/mcp".to_string(),
        token: "secret-token".to_string(),
    }];
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &specs)
        .await
        .expect("a capability mismatch must not fail start_session");
    assert!(!session_id.is_empty());
}

#[tokio::test]
async fn acp_agent_client_start_session_error_never_contains_the_mcp_token_value() {
    let client = AcpAgentClient::new();
    let specs = vec![McpServerSpec::Http {
        name: "rocket-tools".to_string(),
        url: "http://127.0.0.1:4000/mcp".to_string(),
        token: "sk-mcp-super-secret-test-value".to_string(),
    }];
    let err = client
        .start_session(
            "definitely-not-a-real-binary-xyz123",
            &[],
            "/tmp",
            &[],
            &specs,
        )
        .await
        .expect_err("nonexistent command must fail, not panic");
    let message = err.to_string();
    assert!(
        !message.contains("sk-mcp-super-secret-test-value"),
        "error message must never contain the mcp token value, got: {message}"
    );
}
```

- [ ] **Step 3: Run the tests to confirm they fail**

Run: `cargo test -p rocket-infra -j4 acp_agent_client_start_session_maps`
Expected: compile succeeds (the `mcp_servers` parameter already exists per Plan 01), but `acp_agent_client_start_session_maps_http_mcp_server_spec_into_new_session_request` and `..._stdio_..._into_new_session_request` fail their `assert!` calls — the dump file contains `[]` regardless of what was passed in, because `start_session` does not yet call `.mcp_servers(...)` on the `NewSessionRequest`.

- [ ] **Step 4: Implement the mapping and wire it into `start_session`**

In `crates/rocket-infra/src/acp_agent_client.rs`, widen the imports — replace:

```rust
use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, FileSystemCapabilities, Implementation, InitializeRequest,
    NewSessionRequest, PromptRequest, SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent as AgentRole, ByteStreams, Client, ConnectionTo,
};
use async_process::Child;
use rocket_acp::AcpSessionClient;
```

with:

```rust
use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, EnvVariable, FileSystemCapabilities, HttpHeader,
    Implementation, InitializeRequest, McpServer, McpServerHttp, McpServerStdio,
    NewSessionRequest, PromptRequest, SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent as AgentRole, ByteStreams, Client, ConnectionTo,
};
use async_process::Child;
use rocket_acp::{AcpSessionClient, McpServerSpec};
```

Add the two mapping/logging helpers near the bottom of the file, immediately before `stop_reason_to_wire_string`:

```rust
/// Maps Rocket's transport-agnostic `McpServerSpec` (owned by `rocket-acp`,
/// which must not depend on `agent-client-protocol` -- see that crate's DDD
/// boundary) to the real `agent_client_protocol::schema::v1::McpServer` wire
/// type. `rocket-app` (Plan 03) decides *which* variants to build for a given
/// session; this function only translates that decision, it never chooses
/// Http vs Stdio itself.
fn mcp_server_specs_to_wire(specs: &[McpServerSpec]) -> Vec<McpServer> {
    specs
        .iter()
        .map(|spec| match spec {
            McpServerSpec::Http { name, url, token } => McpServer::Http(
                McpServerHttp::new(name.clone(), url.clone()).headers(vec![HttpHeader::new(
                    "Authorization",
                    format!("Bearer {token}"),
                )]),
            ),
            McpServerSpec::Stdio {
                name,
                command,
                args,
                env,
            } => McpServer::Stdio(
                McpServerStdio::new(name.clone(), command.clone())
                    .args(args.clone())
                    .env(
                        env.iter()
                            .map(|(k, v)| EnvVariable::new(k.clone(), v.clone()))
                            .collect(),
                    ),
            ),
        })
        .collect()
}

/// Logs (but never blocks on) a negotiation mismatch: the caller asked for an
/// HTTP MCP server but the agent's `InitializeResponse` never advertised
/// `mcp_capabilities.http`. Choosing *which* `McpServerSpec` variant to send
/// is `rocket-app`'s job (Plan 03) -- this only makes a mismatch visible in
/// logs instead of letting it fail silently deep inside the agent process.
fn warn_if_http_mcp_server_unsupported(mcp_servers: &[McpServerSpec], agent_supports_http: bool) {
    if !agent_supports_http
        && mcp_servers
            .iter()
            .any(|spec| matches!(spec, McpServerSpec::Http { .. }))
    {
        tracing::warn!(
            "requested an HTTP MCP server for this ACP session, but the agent's \
             InitializeResponse did not advertise mcp_capabilities.http; the agent may refuse it"
        );
    }
}
```

Update the trait method signature (as landed by Plan 01) to actually use the parameter — replace:

```rust
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
    ) -> DomainResult<String> {
```

with:

```rust
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
    ) -> DomainResult<String> {
```

(If Plan 01 already landed this parameter under a different name, e.g. `_mcp_servers`, rename every occurrence to `mcp_servers` here.)

Capture an owned copy before the process is spawned — replace:

```rust
        let cwd = cwd.to_string();
        let command_owned = command.to_string();
```

with:

```rust
        let cwd = cwd.to_string();
        let command_owned = command.to_string();
        let mcp_servers_owned = mcp_servers.to_vec();
```

Finally, update the handshake to capture `InitializeResponse` and attach the mapped MCP servers — replace:

```rust
                        let handshake = async {
                            connection
                                .send_request(
                                    InitializeRequest::new(ProtocolVersion::V1)
                                        .client_capabilities(
                                            ClientCapabilities::new().fs(
                                                FileSystemCapabilities::new()
                                                    .read_text_file(false)
                                                    .write_text_file(false),
                                            ),
                                        )
                                        .client_info(Implementation::new(
                                            "rocket",
                                            env!("CARGO_PKG_VERSION"),
                                        )),
                                )
                                .block_task()
                                .await?;
                            connection
                                .send_request(NewSessionRequest::new(cwd))
                                .block_task()
                                .await
                        };
```

with:

```rust
                        let handshake = async {
                            let init_response = connection
                                .send_request(
                                    InitializeRequest::new(ProtocolVersion::V1)
                                        .client_capabilities(
                                            ClientCapabilities::new().fs(
                                                FileSystemCapabilities::new()
                                                    .read_text_file(false)
                                                    .write_text_file(false),
                                            ),
                                        )
                                        .client_info(Implementation::new(
                                            "rocket",
                                            env!("CARGO_PKG_VERSION"),
                                        )),
                                )
                                .block_task()
                                .await?;
                            warn_if_http_mcp_server_unsupported(
                                &mcp_servers_owned,
                                init_response.agent_capabilities.mcp_capabilities.http,
                            );
                            connection
                                .send_request(
                                    NewSessionRequest::new(cwd)
                                        .mcp_servers(mcp_server_specs_to_wire(&mcp_servers_owned)),
                                )
                                .block_task()
                                .await
                        };
```

- [ ] **Step 5: Run the tests again to confirm they pass**

Run: `cargo test -p rocket-infra -j4 acp_agent_client`
Expected: all tests in `crates/rocket-infra/tests/acp_agent_client.rs` pass, including the 5 new ones from Step 2 and every pre-existing test (Plan 01 already updated their call sites to pass a 5th `&[]` argument, per the plan index's cross-plan compilation rule).

- [ ] **Step 6: Run the full workspace check**

Run: `cargo check --workspace -j4`
Expected: no errors.

- [ ] **Step 7: Commit**

Stage `crates/rocket-infra/src/acp_agent_client.rs`, `crates/rocket-infra/src/bin/test_acp_agent.rs`, and `crates/rocket-infra/tests/acp_agent_client.rs`. Invoke the `dev-workflow-skills:1-git-commit` skill (`Skill` tool, skill name `dev-workflow-skills:1-git-commit`) to draft and create the commit.

---

## Next Plan

[Plan 03 — `rocket-app` orchestration](2026-09-28-acp-mcp-tool-server-plan-03-app-orchestration.md) builds `McpToolService`, threads `RunSource` through `ExecuteRequestInput`/`HistoryEntry` at every call site, and updates `AcpSessionService::start_session` to build the real `Vec<McpServerSpec>` (gated on `agent_autonomy_enabled`) that this plan's `AcpAgentClient::start_session` now knows how to consume.

## Post-Implementation Review

After all three tasks above are committed, dispatch a fresh subagent with `model: "opus"` to review this plan's full diff (all commits made while executing this file) against:

- **Interface conformance vs the index:** confirm `crates/rocket-infra/Cargo.toml` gained **no** `rmcp`/`axum` entry from this plan (Task 2 is grounding-only; the real dependency lives in `src-tauri/Cargo.toml` per Plan 04); `CollectionSettings.agent_autonomy_enabled` round-trips through `extensions.rocketapi.agentAutonomyEnabled` exactly like `sandbox_mode` does through `extensions.rocketapi.sandboxMode`; `AcpAgentClient::start_session`'s `mcp_servers` parameter is mapped and attached, not silently dropped anywhere on an error path.
- **Code quality and duplication:** `agent_autonomy_enabled_from_extensions`/`set_agent_autonomy_enabled_in_extensions` should read as obvious siblings of the `sandbox_mode` pair, not a divergent reimplementation; `mcp_server_specs_to_wire` should have no dead branches now that both `McpServerSpec` variants are handled.
- **DDD boundaries:** `rocket-acp`'s `McpServerSpec` is never imported by anything outside `rocket-infra`/`rocket-app` in this plan's diff; `agent_client_protocol` types stay confined to `rocket-infra` (never leak into `rocket-app` or `rocket-collection`).
- **Security:** the MCP bearer token appears only inside the one `HttpHeader::new("Authorization", format!("Bearer {token}"))` call — grep the diff for `token` to confirm it is never logged, never placed in an error string, and never passed as a bare argv element anywhere this plan touches.
- **Test coverage vs this plan's own Review Focus section** (above) — confirm each of the five listed risks has a passing test, not just a task that plausibly covers it.

The reviewing subagent has authority to fix anything it finds directly (small, targeted commits, using the `dev-workflow-skills:1-git-commit` skill per this user's global instructions) rather than only reporting issues, mirroring the process already used for subprojects A and B of this program.
