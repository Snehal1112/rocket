# ACP MCP Tool Server — Plan 05: Stdio Shim + Full End-to-End Wiring

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Rocket a hidden `--acp-mcp-stdio-bridge` CLI mode that proxies an ACP agent's stdio MCP traffic 1:1 to Plan 04's in-process HTTP MCP backend, then wire `AcpSessionService::start_session` end-to-end so a session with `agent_autonomy_enabled` actually spawns that backend and offers the agent both transports — closing every code path that can leak a bound port or a live bearer token.

**Architecture:** Three pieces, kept strictly separate: (1) `src-tauri/src/mcp/stdio_bridge.rs`, a generic rmcp stdio-server-to-HTTP-client forwarder with zero tool-specific logic, entered via a flag check at the very top of `main()` before any Tauri bootstrap; (2) `AcpSessionService::start_session` (rocket-app), which — kept free of any Tauri type — accepts a plain `(port, token)` pair built by the Tauri command layer and turns it into the `Http` + `Stdio` entries of the `McpServerSpec` list Plan 02's capability negotiation picks between; (3) a small `McpServerRegistry` + `McpServerSweeper` pair in `src-tauri` that closes the gap where a session can end (normal `end_session`, the bulk `end_all_sessions` exit sweep, or `AcpSessionService::send_prompt`'s internal timeout-kill) without ever routing through the Tauri command that would otherwise tear down its MCP HTTP server.

**Tech Stack:** Rust, Tauri 2, `rmcp` (stdio + Streamable HTTP transports), `tokio`, `async-trait`.

**Spec:** [../../specs/2026-09-28-acp-mcp-tool-server-design.md](../../specs/2026-09-28-acp-mcp-tool-server-design.md)
**Plan index (locked contracts):** [00-plan-index.md](00-plan-index.md)

## Global Constraints

- No credential/secret-bearing value (the MCP port or bearer token) may ever appear in a process's argv — only via environment variables (`ROCKET_MCP_PORT`/`ROCKET_MCP_TOKEN`), matching subproject B's argv-visibility lesson.
- Two transports, one implementation of tool logic: the stdio bridge contains **zero** per-tool logic — every call is forwarded verbatim by name and JSON arguments to the real HTTP backend.
- `AppHandle` (or any other `tauri`-crate type) must never appear in `rocket-app`'s public API — Tauri types are confined to `src-tauri`. (`rocket-app`'s `Cargo.toml` already carries a bare `tauri = "2"` dependency used once, in `load_test_service.rs`'s `Option<&tauri::AppHandle>` parameter — this plan does not extend that precedent; see Task 3's Design Note.)
- No use of the panicking shorthand in any production code path — this repo's commit hook blocks it, and it is also a hard rule in this project's `CLAUDE.md`.
- `cargo check --workspace -j4` (this repo's `-j4` convention) must be green at the end of every task below.
- Reuse the exact `rmcp` dependency line Plan 04 already added to `src-tauri/Cargo.toml` (version `3.5`, the real version Plan 02 grounded via a dry-run `cargo add` against crates.io) — do not introduce a second, differently-pinned `rmcp` entry in that file, and do not add `rmcp` to `crates/rocket-infra/Cargo.toml` (nothing in `rocket-infra` uses it; see Plan 02, Task 2).

## Review Focus

- A `send_prompt` timeout force-kills the agent process via `AcpSessionClient::end_session` from inside `AcpSessionService` itself, bypassing the `end_agent_session` Tauri command entirely — a naive registry-cleanup-only-in-the-command-layer design would leak that session's MCP HTTP server (bound port + live token) forever. Task 4 closes this with a uniform sweeper called from every termination path inside the service.
- The bulk exit-sweep (`end_all_sessions`, hit by both the Unix signal listener and Tauri's `RunEvent::Exit`) must tear down every registered MCP HTTP server, not just kill agent processes — a crash mid-session must not leave a listener bound to `127.0.0.1` with a live bearer token. Task 4 test.
- `start_session` can fail (bad agent config, credential resolution failure) *after* the MCP HTTP server for that attempt has already been spawned — the just-bound port and token must not be left orphaned. Task 3 test.
- The stdio bridge started with no `ROCKET_MCP_PORT`/`ROCKET_MCP_TOKEN` set (e.g. a misconfigured agent) must fail fast and clearly, not hang waiting on a display server the way falling through to normal `tauri::Builder` startup would on a headless machine. Task 1 test.
- The `McpServerSpec::Stdio` entry built for the agent must never place the token in `args` — only in `env` — verified by inspecting the actual built spec, not just by code review. Task 3 test.

---

## Assumed state from Plans 01–04 (verify before starting)

This plan is written against the plan index's locked contracts, before Plans 01–04 exist as files. Each task below states the exact signature/shape it assumes from an earlier plan; **the first step of any task touching such a boundary is to read the real file and confirm the shape matches** — if a name differs (e.g. `McpHttpServerHandle`'s shutdown method), adjust only that name and proceed; the logic does not change.

- `crates/rocket-acp/src/mcp_server_spec.rs` (Plan 01): `McpServerSpec::Http { name, url, token }` / `McpServerSpec::Stdio { name, command, args, env }`, both `String`/`Vec<String>`/`Vec<(String, String)>` fields, no `agent_client_protocol` dependency.
- `AcpSessionClient::start_session` (Plan 01, `crates/rocket-acp/src/session.rs`) takes a trailing `mcp_servers: &[McpServerSpec]` parameter; `AcpAgentClient` (Plan 02, `crates/rocket-infra/src/acp_agent_client.rs`) maps it to real `agent_client_protocol::McpServer` values and already reads `InitializeResponse.agent_capabilities.mcp_capabilities.http` to choose which of the two specs it hands the agent.
- `crates/rocket-app/src/acp_session_service.rs` (Plan 03): `AcpSessionService::start_session` actually takes `collection: Option<&str>` (not a bare `&str` — a session with no target collection stays chat-only, matching subproject C's existing behavior) and, when the flag is on, currently builds an empty `Vec<McpServerSpec>` via a private `mcp_server_specs_for` helper as a compiling placeholder. **This plan (Task 3) replaces both**: `collection` becomes a required `&str` (by this point every session-start call site always has a real collection — Plan 06's UI and `AgentChatPanel`'s existing per-collection scoping guarantee it), the empty-vec placeholder and its `mcp_server_specs_for` helper are removed, and a new `mcp_http: Option<McpHttpServerCredentials>` trailing parameter carries the already-spawned HTTP server's port/token in. `AcpSessionService::new`/`with_prompt_timeout` already take a `collection_repo: Arc<dyn CollectionRepository>` 4th parameter as of Plan 03 — reuse that field, do not add a second one.
- `src-tauri/src/mcp/tool_server.rs` (Plan 04): `pub struct McpHttpServerHandle { pub port: u16, pub token: String, /* shutdown handle */ }` with a **synchronous** `pub fn shutdown(&self)` (fire-and-forget `Notify::notify_one`, not `async`, not by-value) and `pub async fn spawn_mcp_http_server(app_handle: tauri::AppHandle, session_id: String) -> std::io::Result<McpHttpServerHandle>` — Plan 04 added a `session_id: String` second parameter (every `McpToolService` call needs one, and `RocketMcpToolServer` fixes it at construction), explicitly leaving to this plan the question of which identifier to pass before the ACP handshake completes. **This plan does not assume `shutdown` is `async` — every call site below calls it synchronously.** `src-tauri/src/mcp/registry.rs` (Plan 04) already defines the one and only `McpServerRegistry` this subproject uses — synchronous methods `new() -> Self`, `register(&self, session_id: String, handle: McpHttpServerHandle)`, `end_session(&self, session_id: &str)`, `shutdown_all(&self)` — managed as `Arc<McpServerRegistry>` Tauri state. **This plan reuses that type as-is; it does not define a second `McpServerRegistry`.**
- `src-tauri/src/mcp/mod.rs` (Plan 04) already exists with `pub mod tool_server;` and `pub mod registry;` inside it, and `src-tauri/src/lib.rs` already has a `pub mod mcp;` declaration plus a local `let mcp_server_registry = Arc::new(mcp::registry::McpServerRegistry::new());` binding right before the managed-state block, `Arc::clone`d into `app.manage(...)` there.
- `rmcp` is already a dependency of **`src-tauri/Cargo.toml`** (Plan 04 Task 1, not `crates/rocket-infra/Cargo.toml` — Plan 02's addition of `rmcp`/`axum` to `rocket-infra/Cargo.toml` turned out to be unnecessary, since no code in `rocket-infra` ends up using either crate; see Plan 02's Task 2 note), with feature set `["server", "macros", "transport-streamable-http-server", "transport-io", "client", "transport-streamable-http-client-reqwest"]` at whatever exact version `cargo add` resolved there (Plan 02 separately confirmed `3.5.0` is the real current version via `cargo add --dry-run` against crates.io — reuse that version number, do not re-resolve it independently). This plan's stdio bridge needs `transport-io`/`client`/`transport-streamable-http-client-reqwest`, which Plan 04's own feature list did not include for its HTTP-server-only needs — Task 1, Step 2 below adds only those missing features to the *existing* `rmcp` line in `src-tauri/Cargo.toml`, it does not add a second `rmcp` dependency line.

---

### Task 1: Hidden CLI startup mode + generic stdio↔HTTP forwarding bridge

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/lib.rs` (ensure `mod mcp;` is `pub mod mcp;`)
- Modify: `src-tauri/src/mcp/mod.rs` (add `pub mod stdio_bridge;`)
- Modify: `src-tauri/Cargo.toml` (confirm/extend the existing `rmcp` dependency's feature list — see Step 2 below; `tauri`'s `"test"` dev-dependency feature is already present, added by Plan 04)
- Create: `src-tauri/src/mcp/stdio_bridge.rs`
- Test: `src-tauri/src/mcp/stdio_bridge.rs` (inline `#[cfg(test)]`), `src-tauri/tests/acp_mcp_stdio_bridge_cli_mode.rs` (new)

**Interfaces:**
- Consumes: `rmcp::ServerHandler` (Plan 02's pinned version), `rmcp::transport::io::stdio`, `rmcp::transport::streamable_http_client::{StreamableHttpClientTransport, StreamableHttpClientTransportConfig}`.
- Produces: `pub async fn run_stdio_bridge() -> std::io::Result<()>` in `src-tauri/src/mcp/stdio_bridge.rs`, called from `main.rs`. Task 2 depends on this function and on the `--acp-mcp-stdio-bridge` flag check in `main.rs`.

- [ ] **Step 1: Confirm the pinned `rmcp` API surface**

Read `src-tauri/Cargo.toml` for the exact `rmcp` version and feature list Plan 04 added (not `crates/rocket-infra/Cargo.toml` — see this plan's "Assumed state" section above), then run:

```bash
cargo doc -p rmcp --no-deps -j4 2>&1 | tail -40
```

Open the generated docs (or the matching `docs.rs/rmcp/<version>` page) and confirm these four names before writing code below — if any differs, substitute the real name everywhere in this task and Task 2, with no other logic change:
- The server-side trait is `rmcp::handler::server::ServerHandler` (re-exported as `rmcp::ServerHandler`), with provided (overridable, non-required) methods `call_tool(&self, request: CallToolRequestParams, context: RequestContext<RoleServer>) -> Result<CallToolResponse, ErrorData>` and `list_tools(&self, request: Option<PaginatedRequestParams>, context: RequestContext<RoleServer>) -> Result<ListToolsResult, ErrorData>` — confirm `CallToolRequestParams`/`CallToolResponse` are not named `CallToolRequestParam`/`CallToolResult` in the pinned version, and confirm the error type is `ErrorData` and not a separate `McpError` alias.
- `rmcp::transport::io::stdio() -> (tokio::io::Stdin, tokio::io::Stdout)`, usable directly as a transport argument to `.serve(...)`.
- `rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig` is a public, `#[non_exhaustive]`, all-`pub`-field struct with a `with_uri(uri) -> Self` constructor and an `auth_header: Option<String>` field, and `StreamableHttpClientTransport::from_config(config) -> Self` builds a transport from it using the default `reqwest`-backed client (feature `transport-streamable-http-client-reqwest`).
- `RunningService<RoleClient, C>` (returned by `<C as ServiceExt<RoleClient>>::serve(transport)`) exposes `call_tool_once(request: CallToolRequestParams) -> Result<CallToolResponse, ServiceError>` (single round-trip, no auto multi-round follow-ups) and `list_tools(request: Option<PaginatedRequestParams>) -> Result<ListToolsResult, ServiceError>`.

This step's deliverable is confirmation (or a short list of renames to carry into the steps below) — it does not itself change any file.

- [ ] **Step 2: Confirm `rmcp`'s features in `src-tauri/Cargo.toml` already cover this plan's needs (no new dependency line)**

Plan 04, Task 1 already added `rmcp` as a direct dependency of `src-tauri/Cargo.toml`, with its feature list written as the union of what Plan 04's HTTP server needs and what this plan's stdio bridge needs (`server`, `macros`, `transport-streamable-http-server`, `transport-io`, `client`, `transport-streamable-http-client-reqwest`), at version `3.5` (the real version Plan 02 grounded via a dry-run `cargo add` against crates.io — see Plan 02, Task 2). **Do not add a second `rmcp = { ... }` line to `src-tauri/Cargo.toml`** — Cargo rejects a duplicate dependency key in the same table, and the existing line already has every feature this task's `stdio_bridge.rs` uses (`transport-io`, `client`, `transport-streamable-http-client-reqwest`).

Confirm this by reading `src-tauri/Cargo.toml` before writing any code in Step 3 below. If, by the time this plan executes, Plan 04's line is somehow missing one of those three features (e.g. an earlier hand-edit dropped one), add only the missing feature name(s) to the existing line — never a new, separate `rmcp` entry.

- [ ] **Step 3: Write the generic forwarding handler and `run_stdio_bridge`**

```rust
// src-tauri/src/mcp/stdio_bridge.rs
//
// Hidden `--acp-mcp-stdio-bridge` CLI mode. An ACP agent that does not
// advertise `mcp_capabilities.http` spawns Rocket's own executable with this
// flag (see AcpSessionService::start_session, Task 3) and talks MCP to it
// over its stdin/stdout. This module implements no tool logic of its own —
// every `tools/list`/`tools/call` request received over stdio is forwarded
// verbatim to Plan 04's real HTTP MCP backend and the response is forwarded
// back unchanged. That backend already re-checks `agent_autonomy_enabled` and
// enforces the `secret` variable boundary on every call, so this bridge has
// no additional authorization logic to duplicate.
use std::io;

use rmcp::model::{CallToolRequestParams, CallToolResponse, ErrorData, ListToolsResult, PaginatedRequestParams};
use rmcp::service::{RequestContext, RoleClient, RoleServer, RunningService};
use rmcp::transport::io::stdio;
use rmcp::transport::streamable_http_client::{StreamableHttpClientTransport, StreamableHttpClientTransportConfig};
use rmcp::{ServerHandler, ServiceExt};

/// Forwards every incoming tool-call to the already-connected HTTP client.
/// Holding the connected client (rather than the raw port/token) means the
/// initial MCP handshake with the HTTP backend happens once, at bridge
/// startup, not on every forwarded call.
struct ForwardingHandler {
    upstream: RunningService<RoleClient, ()>,
}

impl ServerHandler for ForwardingHandler {
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.upstream
            .list_tools(request)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.upstream
            .call_tool_once(request)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }
}

/// Reads the HTTP backend's port and bearer token from the environment
/// (never argv, per this plan's Global Constraints) and runs a stdio MCP
/// server that forwards every call to it until stdin closes.
pub async fn run_stdio_bridge() -> io::Result<()> {
    let port: u16 = std::env::var("ROCKET_MCP_PORT")
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "ROCKET_MCP_PORT is not set"))?
        .parse()
        .map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "ROCKET_MCP_PORT is not a valid port number")
        })?;
    let token = std::env::var("ROCKET_MCP_TOKEN")
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "ROCKET_MCP_TOKEN is not set"))?;

    let config = StreamableHttpClientTransportConfig {
        auth_header: Some(format!("Bearer {token}")),
        ..StreamableHttpClientTransportConfig::with_uri(format!("http://127.0.0.1:{port}"))
    };
    let transport = StreamableHttpClientTransport::from_config(config);
    let upstream = ()
        .serve(transport)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::ConnectionRefused, e.to_string()))?;

    let handler = ForwardingHandler { upstream };
    let server = handler
        .serve(stdio())
        .await
        .map_err(io::Error::other)?;
    server.waiting().await.map_err(io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_stdio_bridge_fails_fast_when_port_env_var_is_missing() {
        std::env::remove_var("ROCKET_MCP_PORT");
        std::env::remove_var("ROCKET_MCP_TOKEN");
        let err = run_stdio_bridge()
            .await
            .expect_err("must fail without ROCKET_MCP_PORT");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("ROCKET_MCP_PORT"));
    }

    #[tokio::test]
    async fn run_stdio_bridge_fails_fast_when_port_env_var_is_not_a_number() {
        std::env::set_var("ROCKET_MCP_PORT", "not-a-port");
        std::env::set_var("ROCKET_MCP_TOKEN", "irrelevant-for-this-test");
        let err = run_stdio_bridge()
            .await
            .expect_err("must fail on a non-numeric port");
        std::env::remove_var("ROCKET_MCP_PORT");
        std::env::remove_var("ROCKET_MCP_TOKEN");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("ROCKET_MCP_PORT"));
    }

    #[tokio::test]
    async fn run_stdio_bridge_fails_fast_when_token_env_var_is_missing() {
        std::env::set_var("ROCKET_MCP_PORT", "65000");
        std::env::remove_var("ROCKET_MCP_TOKEN");
        let err = run_stdio_bridge()
            .await
            .expect_err("must fail without ROCKET_MCP_TOKEN");
        std::env::remove_var("ROCKET_MCP_PORT");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("ROCKET_MCP_TOKEN"));
    }
}
```

- [ ] **Step 4: Wire the hidden flag into `main.rs`, before any Tauri bootstrap**

```rust
// src-tauri/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Hidden startup mode: an ACP agent spawns this exact binary again with
    // this flag when it needs the Stdio MCP transport (AcpSessionService::
    // start_session, Task 3, builds this command line). It must never reach
    // normal Tauri bootstrap below — there is no window or webview in this
    // mode, only a stdio<->HTTP forwarding loop.
    if std::env::args().any(|arg| arg == "--acp-mcp-stdio-bridge") {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to start the Tokio runtime for the MCP stdio bridge");
        if let Err(e) = runtime.block_on(rocket_lib::mcp::stdio_bridge::run_stdio_bridge()) {
            eprintln!("acp-mcp-stdio-bridge failed: {e}");
            std::process::exit(1);
        }
        return;
    }

    rocket_lib::run()
}
```

- [ ] **Step 5: Make the module path public and add `stdio_bridge`**

Open `src-tauri/src/lib.rs` and confirm the existing `mod mcp;` line (added by Plan 04) reads `pub mod mcp;` — `main.rs` is a separate crate root that calls into `rocket_lib`, so the module and the function must both be `pub` all the way down. Change it if it is currently private:

```rust
// src-tauri/src/lib.rs — top-of-file module declarations
pub mod mcp;
```

Open `src-tauri/src/mcp/mod.rs` (created by Plan 04, currently containing `pub mod tool_server;`) and add:

```rust
// src-tauri/src/mcp/mod.rs
pub mod stdio_bridge;
```

- [ ] **Step 6: Run the unit tests**

```bash
cargo test -p rocket --lib mcp::stdio_bridge -j4
```

Expected: the three env-var-failure tests pass.

- [ ] **Step 7: Write the CLI-mode integration test**

```rust
// src-tauri/tests/acp_mcp_stdio_bridge_cli_mode.rs
//
// Proves the `--acp-mcp-stdio-bridge` flag short-circuits before any Tauri
// bootstrap. Running it with no ROCKET_MCP_PORT/ROCKET_MCP_TOKEN set must
// fail fast with the bridge's own clear error, not hang trying to open a
// display connection the way falling through to `tauri::Builder` startup
// would on a headless machine — a real GUI attempt fails differently (or
// hangs), so this specific fast, specific-message failure is the proxy for
// "no GUI bootstrap happened".
use std::process::Command;

#[test]
fn acp_mcp_stdio_bridge_flag_skips_gui_bootstrap_and_fails_fast_without_env() {
    let exe = env!("CARGO_BIN_EXE_rocket");
    let output = Command::new(exe)
        .arg("--acp-mcp-stdio-bridge")
        .env_remove("ROCKET_MCP_PORT")
        .env_remove("ROCKET_MCP_TOKEN")
        .output()
        .expect("failed to run the rocket binary");

    assert!(
        !output.status.success(),
        "bridge mode with no env vars set must exit non-zero, not hang or launch a GUI"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ROCKET_MCP_PORT"),
        "expected the bridge's own missing-env-var error, got: {stderr}"
    );
}
```

- [ ] **Step 8: Run the integration test**

```bash
cargo test -p rocket --test acp_mcp_stdio_bridge_cli_mode -j4
```

Expected: PASS, and the process exits in well under a second (no hang).

- [ ] **Step 9: `cargo check --workspace -j4`**

Expected: green.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill for the commit message (per this repo's global instructions — do not write a freeform `git commit -m`).

```bash
git add src-tauri/src/main.rs src-tauri/src/lib.rs src-tauri/src/mcp/mod.rs \
        src-tauri/src/mcp/stdio_bridge.rs src-tauri/Cargo.toml \
        src-tauri/tests/acp_mcp_stdio_bridge_cli_mode.rs
```

---

### Task 2: End-to-end round-trip integration test (bridge ↔ real Plan 04 backend)

**Files:**
- Create: `src-tauri/tests/acp_mcp_stdio_bridge_roundtrip.rs`
- Confirm only: `src-tauri/Cargo.toml`'s `tauri`/`"test"` dev-dependency feature (already added by Plan 04, Task 1) — no edit expected here

**Interfaces:**
- Consumes: `rocket_lib::mcp::tool_server::spawn_mcp_http_server` and `McpHttpServerHandle` (Plan 04), `rocket_app::McpToolService::new` (Plan 03's locked constructor), `tauri::test::{mock_builder, mock_context, noop_assets}` (Tauri's own test harness).
- Produces: nothing consumed by a later task — this is a leaf verification task proving the full chain (child-process stdio server → HTTP client → real axum/rmcp backend → real `McpToolService` tool) actually round-trips, which Task 1's and Plan 04's own narrower tests cannot prove on their own.

- [ ] **Step 1: Confirm the Tauri mock-app test harness**

```bash
cargo doc -p tauri --no-deps -j4 2>&1 | grep -i "mod test" -A5
```

Confirm `tauri::test::mock_builder() -> tauri::Builder<tauri::test::MockRuntime>`, `tauri::test::mock_context(tauri::test::noop_assets()) -> tauri::Context<...>`, and that `.manage(...)` + `.build(context)` on that builder yields a real `tauri::App<MockRuntime>` whose `.handle()` returns a usable `AppHandle` (Plan 04's own tests almost certainly already use this same pattern to test `spawn_mcp_http_server` without a real window — if Plan 04's test file already exists by the time this task runs, copy its exact mock-app setup instead of re-deriving it here).

- [ ] **Step 2: Confirm the Tauri test feature is already present**

Plan 04, Task 1 already added `tauri = { version = "2", features = ["test"] }` to `src-tauri/Cargo.toml`'s `[dev-dependencies]` (its own tests need `tauri::test::mock_builder` too). Confirm it is there before writing Step 3's test — do not add a second `tauri = { ... }` dev-dependency line.

- [ ] **Step 3: Write the round-trip test**

```rust
// src-tauri/tests/acp_mcp_stdio_bridge_roundtrip.rs
//
// Spawns the real Plan 04 HTTP MCP backend inside a mocked Tauri app (no
// real window/webview — `tauri::test::mock_builder` gives a real AppHandle
// bound to managed state, which is all `spawn_mcp_http_server` needs), then
// spawns the actual compiled `--acp-mcp-stdio-bridge` binary as a real child
// process wired to that backend via ROCKET_MCP_PORT/ROCKET_MCP_TOKEN, and
// drives it as a real ACP agent would: a raw `initialize` handshake followed
// by `tools/list` over the child's stdin/stdout. This is the one test in
// this plan proving both transports are wired together correctly, not just
// each one in isolation.
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use rocket_collection::{CollectionRepository, CollectionSettings};

#[tokio::test]
async fn stdio_bridge_forwards_a_real_tool_list_round_trip() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let collections_dir = fixture.path().join("collections");
    std::fs::create_dir_all(&collections_dir).expect("create collections dir");
    let workspace_path: Arc<Mutex<std::path::PathBuf>> =
        Arc::new(Mutex::new(fixture.path().to_path_buf()));

    let collection_repo: Arc<dyn CollectionRepository> =
        Arc::new(rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone()));
    collection_repo.create("demo").expect("create demo collection");
    collection_repo
        .save_settings(
            "demo",
            &CollectionSettings {
                agent_autonomy_enabled: true,
                ..Default::default()
            },
        )
        .expect("enable agent autonomy for the fixture collection");

    let executor: Arc<dyn rocket_http::HttpExecutor> =
        Arc::new(rocket_infra::ReqwestExecutor::with_allowed_base(Arc::clone(&workspace_path)));
    let exec_svc = Arc::new(rocket_app::RequestExecutionService::new_with_audit(
        Box::new(rocket_infra::FsEnvironmentRepo::with_secret_store(
            fixture.path().join("environments"),
            Arc::new(rocket_environment::secret_store::NullSecretStore),
        )),
        Arc::clone(&executor),
        Box::new(rocket_infra::FsHistoryRepo::new(fixture.path().join("history"))),
        Box::new(rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone())),
        Box::new(rocket_infra::FsCookieRepo::new(fixture.path().join("cookies"))),
        Box::new(rocket_shared::events::NullEventPublisher),
        Arc::new(rocket_audit::publisher::NullSecurityAuditPublisher),
        Box::new(rocket_infra::FsSecretManagerRepo::new(
            fixture.path().join("secret_managers.yml"),
        )),
        Arc::new(rocket_environment::secret_store::NullSecretStore),
        Arc::new(rocket_environment::vault_secret_fetcher::NullVaultSecretFetcher),
    ));
    let env_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory> =
        Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(Arc::clone(&workspace_path)));

    let mcp_tool_svc = Arc::new(rocket_app::McpToolService::new(
        Arc::clone(&collection_repo),
        env_repo_factory,
        exec_svc,
        Arc::new(rocket_shared::events::NullEventPublisher),
    ));

    let app = tauri::test::mock_builder()
        .manage(mcp_tool_svc)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build mock tauri app");

    let handle = rocket_lib::mcp::tool_server::spawn_mcp_http_server(
        app.handle().clone(),
        "test-session".to_string(),
    )
    .await
    .expect("spawn the real Plan 04 HTTP MCP backend");

    let bin = env!("CARGO_BIN_EXE_rocket");
    let mut child = Command::new(bin)
        .arg("--acp-mcp-stdio-bridge")
        .env("ROCKET_MCP_PORT", handle.port.to_string())
        .env("ROCKET_MCP_TOKEN", handle.token.clone())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the stdio bridge subprocess");

    let mut stdin = child.stdin.take().expect("child stdin");
    let mut reader = BufReader::new(child.stdout.take().expect("child stdout"));

    write_line(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test-agent", "version": "0.0.0"}
            }
        }),
    );
    let _initialize_response = read_line(&mut reader);
    write_line(
        &mut stdin,
        &serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );

    write_line(
        &mut stdin,
        &serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
    );
    let response = read_line(&mut reader);

    let tool_names: Vec<&str> = response["result"]["tools"]
        .as_array()
        .expect("tools array in tools/list response")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name is a string"))
        .collect();
    assert!(
        tool_names.contains(&"list_collection_requests"),
        "expected the real Plan 04 tool set to round-trip through the bridge, got {tool_names:?}"
    );

    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    // shutdown() is synchronous (Plan 04's real McpHttpServerHandle) — no `.await`.
    handle.shutdown();
}

fn write_line(stdin: &mut std::process::ChildStdin, value: &serde_json::Value) {
    let mut line = value.to_string();
    line.push('\n');
    stdin.write_all(line.as_bytes()).expect("write to child stdin");
    stdin.flush().expect("flush child stdin");
}

fn read_line(reader: &mut BufReader<std::process::ChildStdout>) -> serde_json::Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read from child stdout");
    serde_json::from_str(&line).expect("child stdout line must be valid JSON-RPC")
}
```

- [ ] **Step 4: Run it**

```bash
cargo test -p rocket --test acp_mcp_stdio_bridge_roundtrip -j4
```

Expected: PASS. If the `initialize` JSON-RPC shape above doesn't match the pinned `rmcp`/ACP protocol version's exact field names, adjust only the JSON literal (protocol version string, param names) to match what Plan 04's own tests send — the test's assertion (the tool name round-trips) does not change.

- [ ] **Step 5: `cargo check --workspace -j4`**

Expected: green.

- [ ] **Step 6: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add src-tauri/tests/acp_mcp_stdio_bridge_roundtrip.rs src-tauri/Cargo.toml
```

---

### Task 3: Capability-based transport selection, end-to-end

**Design note — where the HTTP-server spawn lives (the `AppHandle` question):** `spawn_mcp_http_server` needs a `tauri::AppHandle`, but `AcpSessionService` lives in `rocket-app`, which must not depend on `tauri` for this feature (see Global Constraints). Two options exist: inject an `AppHandle` into `AcpSessionService`'s constructor, or spawn the HTTP server in `src-tauri` and pass only plain data (`port: u16`, `token: String`) into `start_session`. **This plan takes the second option.** `rocket-app`'s `Cargo.toml` already has one narrow, call-time-only `Option<&tauri::AppHandle>` parameter on `LoadTestService` (`crates/rocket-app/src/load_test_service.rs:19`) for live progress events — but that is an existing, narrow wart, not a pattern to extend. Storing an `AppHandle` inside a long-lived struct field (as `AcpSessionService::new` would require) is a materially bigger coupling than a single call-time parameter, and the spec's own "Tool wiring" section already establishes the precedent of doing `AppHandle`-dependent work in `src-tauri` and threading only plain results into `rocket-app`-owned types. The Post-Implementation Review below is asked to re-check this call.

**Files:**
- Modify: `crates/rocket-app/src/acp_session_service.rs`
- Modify: `crates/rocket-app/src/lib.rs` (re-export the new `McpHttpServerCredentials` type)
- Modify: `src-tauri/src/lib.rs` (construction call site for `AcpSessionService::new`)
- Modify: `src-tauri/src/commands/acp_sessions.rs` (`start_agent_session`)
- Test: `crates/rocket-app/src/acp_session_service.rs` (inline `#[cfg(test)]`), `src-tauri/tests/acp_mcp_start_agent_session.rs` (new)

**No new registry file.** Plan 04 already created `src-tauri/src/mcp/registry.rs`'s `McpServerRegistry` (synchronous `new`/`register`/`end_session`/`shutdown_all`, managed as `Arc<McpServerRegistry>`) and `src-tauri/src/lib.rs`'s `mcp_server_registry` local binding. This task reuses that exact type — it does not create a second, `mcp_registry.rs`-housed, async-flavored `McpServerRegistry`. (An earlier draft of this plan defined a duplicate async registry here; that was a parallel-authoring mistake, since Plan 04's registry already covers everything this task needs. `TauriMcpServerSweeper`, the one genuinely new piece Task 4 adds, lives as an addition to Plan 04's `src-tauri/src/mcp/registry.rs`, not a new file.)

**Interfaces:**
- Consumes: `AcpSessionService::start_session`'s real Plan-03 shape — `(agent_config_id: &str, cwd: &str, collection: Option<&str>) -> DomainResult<String>` — internally gated on `agent_autonomy_enabled` via a private `mcp_server_specs_for` helper that always returns an empty `Vec<McpServerSpec>` placeholder today (see "Assumed state" above; note `collection` is `Option<&str>` in Plan 03's real code, not the bare `&str` an earlier draft of this plan assumed). `src-tauri/src/mcp/registry.rs`'s `McpServerRegistry` (Plan 04) — `pub fn register(&self, session_id: String, handle: McpHttpServerHandle)`, `pub fn end_session(&self, session_id: &str)`, `pub fn shutdown_all(&self)`, all synchronous.
- Produces: `rocket_app::McpHttpServerCredentials { pub port: u16, pub token: String }` (plain data, no Tauri types) — the new trailing parameter on `start_session`, consumed by `src-tauri`'s `start_agent_session` command. `start_session`'s `collection` parameter changes from `Option<&str>` to a required `&str` (by this point every session always starts within a known collection — see "Assumed state"), and Plan 03's `mcp_server_specs_for` placeholder helper is removed, its logic inlined into the new `start_session` body below.

- [ ] **Step 1: Confirm Plan 03's actual `start_session` signature**

Open `crates/rocket-app/src/acp_session_service.rs` and diff its real `start_session` signature and `AcpSessionService` struct fields against the "Assumed state" section above. If the collection-gating parameter has a different name, or the flag-check uses a different field/method than `collection_repo.get_settings(collection)?.agent_autonomy_enabled`, or `mcp_server_specs_for` was named differently, note the actual names — the steps below use the assumed names; substitute only names, not structure. Confirm in particular whether `collection` really is `Option<&str>` there (expected) before changing it to a required `&str` in Step 2.

- [ ] **Step 2: Add `McpHttpServerCredentials` and thread it through `start_session`**

```rust
// crates/rocket-app/src/acp_session_service.rs — new plain-data type, no
// Tauri dependency. Built by the Tauri command layer (which owns the
// AppHandle needed to spawn the real HTTP server) and passed in here as
// plain values, keeping this crate free of `tauri` for this feature.
#[derive(Debug, Clone)]
pub struct McpHttpServerCredentials {
    pub port: u16,
    pub token: String,
}
```

Delete Plan 03's private `mcp_server_specs_for` helper entirely — its logic is inlined below — and replace `start_session` itself (note `collection` changes from Plan 03's `Option<&str>` to a required `&str`, and gains the new trailing `mcp_http` parameter) with:

```rust
    pub async fn start_session(
        &self,
        agent_config_id: &str,
        cwd: &str,
        collection: &str,
        mcp_http: Option<McpHttpServerCredentials>,
    ) -> DomainResult<String> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = vec![(config.credential_env_var.clone(), credential)];

        let autonomy_enabled = self
            .collection_repo
            .get_settings(collection)?
            .agent_autonomy_enabled;
        let mcp_servers: Vec<rocket_acp::McpServerSpec> = match (autonomy_enabled, mcp_http) {
            (true, Some(creds)) => {
                let exe = std::env::current_exe().map_err(|e| {
                    DomainError::Internal(format!("could not resolve current executable: {e}"))
                })?;
                vec![
                    rocket_acp::McpServerSpec::Http {
                        name: "rocket".to_string(),
                        url: format!("http://127.0.0.1:{}", creds.port),
                        token: creds.token.clone(),
                    },
                    rocket_acp::McpServerSpec::Stdio {
                        name: "rocket".to_string(),
                        command: exe.to_string_lossy().into_owned(),
                        args: vec!["--acp-mcp-stdio-bridge".to_string()],
                        env: vec![
                            ("ROCKET_MCP_PORT".to_string(), creds.port.to_string()),
                            ("ROCKET_MCP_TOKEN".to_string(), creds.token),
                        ],
                    },
                ]
            }
            // Autonomy is off, or the caller couldn't spawn the HTTP server
            // (fails open to chat-only mode rather than failing the whole
            // session start over a tool-server hiccup) — no MCP servers.
            _ => Vec::new(),
        };

        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env, &mcp_servers)
            .await?;
        self.event_publisher
            .publish(DomainEvent::AcpSessionStarted {
                session_id: session_id.clone(),
            });
        Ok(session_id)
    }
```

If Plan 03 already added a `collection_repo: Arc<dyn rocket_collection::CollectionRepository>` field and constructor parameter, reuse it as-is. If Plan 03's placeholder instead re-fetched settings some other way, adapt this step's `self.collection_repo.get_settings(collection)?` call to match — the field must exist either way, since Plan 03 needed it to decide whether to pass `&[]`.

- [ ] **Step 3: Re-export the new type**

```rust
// crates/rocket-app/src/lib.rs
pub use acp_session_service::{AcpSessionService, McpHttpServerCredentials};
```

- [ ] **Step 4: Write the unit test proving the token never lands in argv**

```rust
// crates/rocket-app/src/acp_session_service.rs, in #[cfg(test)] mod tests
#[tokio::test]
async fn start_session_with_autonomy_enabled_builds_http_and_stdio_specs_with_token_only_in_env() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>> = Arc::new(Mutex::new(Vec::new()));
    let client = CapturingSessionClient {
        captured_servers: Arc::clone(&captured_servers),
    };
    let collection_repo = Arc::new(FakeCollectionRepo::with_autonomy_enabled("demo", true));
    let service = AcpSessionService::new(
        Box::new(client),
        Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        agent_config_service(),
        collection_repo,
    );

    service
        .start_session(
            "agent-1",
            "/tmp",
            "demo",
            Some(McpHttpServerCredentials {
                port: 54321,
                token: "s3cr3t-token".to_string(),
            }),
        )
        .await
        .expect("start_session should succeed");

    let servers = captured_servers.lock().expect("lock").clone();
    assert_eq!(servers.len(), 2, "expected one Http and one Stdio spec, got {servers:?}");
    for server in &servers {
        if let rocket_acp::McpServerSpec::Stdio { args, env, .. } = server {
            assert!(
                !args.iter().any(|a| a.contains("s3cr3t-token")),
                "the token must never appear in argv, got args {args:?}"
            );
            assert!(
                env.iter().any(|(k, v)| k == "ROCKET_MCP_TOKEN" && v == "s3cr3t-token"),
                "the token must be passed via the ROCKET_MCP_TOKEN env var, got {env:?}"
            );
        }
    }
}
```

This requires two small test doubles alongside the existing `FakeSessionClient`/`FakeEventPublisher` in this file's `#[cfg(test)] mod tests`:

```rust
struct CapturingSessionClient {
    captured_servers: Arc<Mutex<Vec<rocket_acp::McpServerSpec>>>,
}
#[async_trait::async_trait]
impl AcpSessionClient for CapturingSessionClient {
    async fn start_session(
        &self,
        _command: &str,
        _args: &[String],
        _cwd: &str,
        _env: &[(String, String)],
        mcp_servers: &[rocket_acp::McpServerSpec],
    ) -> DomainResult<String> {
        *self.captured_servers.lock().expect("lock") = mcp_servers.to_vec();
        Ok("session-1".to_string())
    }
    async fn send_prompt(
        &self,
        _session_id: &str,
        _prompt: String,
        _chunk_tx: UnboundedSender<String>,
    ) -> DomainResult<String> {
        unreachable!("not exercised by this test")
    }
    async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
    async fn end_all_sessions(&self) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
}

struct FakeCollectionRepo {
    name: String,
    settings: rocket_collection::CollectionSettings,
}
impl FakeCollectionRepo {
    fn with_autonomy_enabled(name: &str, enabled: bool) -> Self {
        Self {
            name: name.to_string(),
            settings: rocket_collection::CollectionSettings {
                agent_autonomy_enabled: enabled,
                ..Default::default()
            },
        }
    }
}
impl rocket_collection::CollectionRepository for FakeCollectionRepo {
    fn get_settings(&self, name: &str) -> DomainResult<rocket_collection::CollectionSettings> {
        assert_eq!(name, self.name, "unexpected collection name in test");
        Ok(self.settings.clone())
    }
    // All other trait methods are unused by this test.
    fn list(&self) -> DomainResult<Vec<rocket_collection::CollectionSummary>> { unreachable!() }
    fn get(&self, _name: &str) -> DomainResult<rocket_collection::Collection> { unreachable!() }
    fn get_summaries(&self, _name: &str) -> DomainResult<rocket_collection::Collection> { unreachable!() }
    fn create(&self, _name: &str) -> DomainResult<rocket_collection::Collection> { unreachable!() }
    fn delete(&self, _name: &str) -> DomainResult<()> { unreachable!() }
    fn rename(&self, _old_name: &str, _new_name: &str) -> DomainResult<()> { unreachable!() }
    fn get_request(&self, _collection: &str, _path: &str) -> DomainResult<rocket_collection::Request> { unreachable!() }
    fn save_request(&self, _collection: &str, _path: &str, _request: &rocket_collection::Request) -> DomainResult<String> { unreachable!() }
    fn rename_request(&self, _collection: &str, _old_path: &str, _new_path: &str) -> DomainResult<()> { unreachable!() }
    fn delete_request(&self, _collection: &str, _path: &str) -> DomainResult<()> { unreachable!() }
    fn create_folder(&self, _collection: &str, _path: &str) -> DomainResult<()> { unreachable!() }
    fn delete_folder(&self, _collection: &str, _path: &str) -> DomainResult<()> { unreachable!() }
    fn move_item(&self, _sc: &str, _sp: &str, _dc: &str, _dp: &str) -> DomainResult<()> { unreachable!() }
    fn reorder_items(&self, _collection: &str, _folder_path: &str, _ordered_names: &[String]) -> DomainResult<()> { unreachable!() }
    fn save_settings(&self, _name: &str, _settings: &rocket_collection::CollectionSettings) -> DomainResult<()> { unreachable!() }
    // NOTE: this repo trait has more methods than shown here (folder chain
    // variables, request variables, docs, save_request_script from Plan 01,
    // etc.) — implement the remainder as `unreachable!()` the same way,
    // matching whatever the trait's full method list is by the time this
    // task runs (`cargo check` will list any missing ones).
}
```

Every other existing test in this file's `#[cfg(test)] mod tests` that calls `AcpSessionService::new(...)` or `start_session(...)` must be updated for the new constructor/method arity — add a `FakeCollectionRepo::with_autonomy_enabled("*", false)` (or matching the test's own collection name) to each `AcpSessionService::new(...)` call, and a `"demo"` (or that test's chosen name) plus `None` to each `start_session(...)` call, since those tests exercise `send_prompt`/`end_session` behavior unrelated to MCP wiring.

- [ ] **Step 5: Run the new and updated tests**

```bash
cargo test -p rocket-app acp_session_service -j4
```

Expected: all pass, including the pre-existing tests updated for the new signatures.

- [ ] **Step 6: Wire the Tauri-side spawn + registry in `start_agent_session`**

This reuses Plan 04's `crate::mcp::registry::McpServerRegistry` (managed as `Arc<McpServerRegistry>` — see Plan 04, Task 4, Step 3) rather than defining a new one. It also resolves the open question Plan 04 explicitly left for this plan: **which identifier is available to pass as `spawn_mcp_http_server`'s `session_id` before the ACP handshake completes.** The real ACP-protocol session id is only known once `svc.start_session(...)` returns, but the HTTP server (and the `session_id` baked into its `RocketMcpToolServer`, used to tag every `McpToolService` call and `DomainEvent::AcpToolInvoked` audit event for that server's whole lifetime) must already be running before the handshake, so its port/token can be put in `NewSessionRequest`. This command therefore mints a Rocket-side UUID *only* for that pre-handshake purpose, and separately registers the resulting handle in `McpServerRegistry` keyed by the *real* post-handshake session id — because that is the id `end_agent_session`/`send_agent_prompt` address a session by everywhere else in this codebase. (Per `AcpSessionClient::start_session`'s own doc comment, Rocket deliberately keeps no separate id-translation layer for *ACP session* identity; this UUID is not a second ACP session id, it is only ever used as `RocketMcpToolServer`'s internal, audit-trail-scoped identifier and is discarded once the registry is keyed by the real session id.)

```rust
// src-tauri/src/commands/acp_sessions.rs
use std::sync::Arc;

use rocket_app::{AcpSessionService, CollectionService, McpHttpServerCredentials};
use rocket_shared::error::DomainError;
use tauri::State;

use crate::mcp::registry::McpServerRegistry;

#[tauri::command]
pub async fn start_agent_session(
    agent_config_id: String,
    cwd: String,
    collection: String,
    app_handle: tauri::AppHandle,
    collection_svc: State<'_, CollectionService>,
    registry: State<'_, Arc<McpServerRegistry>>,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    let autonomy_enabled = collection_svc.get_settings(&collection)?.agent_autonomy_enabled;

    let mcp_handle = if autonomy_enabled {
        // A Rocket-minted, pre-handshake-only identifier — see this step's
        // header note. It is never surfaced to the frontend and never used
        // as the session's real identity; it exists solely so
        // RocketMcpToolServer has *something* stable to tag its own tool
        // calls/audit events with for as long as this HTTP server runs.
        let mcp_session_id = uuid::Uuid::new_v4().to_string();
        Some(
            crate::mcp::tool_server::spawn_mcp_http_server(app_handle, mcp_session_id)
                .await
                .map_err(|e| DomainError::Internal(format!("failed to start MCP tool server: {e}")))?,
        )
    } else {
        None
    };
    let mcp_credentials = mcp_handle.as_ref().map(|h| McpHttpServerCredentials {
        port: h.port,
        token: h.token.clone(),
    });

    let result = svc
        .start_session(&agent_config_id, &cwd, &collection, mcp_credentials)
        .await;

    match (result, mcp_handle) {
        (Ok(session_id), Some(handle)) => {
            // Registered under the *real* ACP session id, not the
            // pre-handshake mcp_session_id minted above — this is the id
            // end_agent_session/send_agent_prompt (and McpServerRegistry's
            // other callers) all address a session by.
            registry.register(session_id.clone(), handle);
            Ok(session_id)
        }
        (Ok(session_id), None) => Ok(session_id),
        (Err(e), Some(handle)) => {
            // start_session failed after the HTTP server was already bound —
            // never leave an orphaned listener holding a live token. `shutdown`
            // is synchronous (Plan 04) — no `.await` here.
            handle.shutdown();
            Err(e)
        }
        (Err(e), None) => Err(e),
    }
}

#[tauri::command]
pub async fn send_agent_prompt(
    session_id: String,
    prompt: String,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.send_prompt(&session_id, prompt).await
}

#[tauri::command]
pub async fn end_agent_session(
    session_id: String,
    svc: State<'_, AcpSessionService>,
) -> Result<(), DomainError> {
    svc.end_session(&session_id).await
}
```

Note: `end_agent_session` (already carrying a `mcp_registry: State<'_, Arc<McpServerRegistry>>` parameter and an `mcp_registry.end_session(&session_id)` call added by Plan 04, Task 4, Step 4 — shown above without it purely for this step's diff context) and `send_agent_prompt` are otherwise unchanged here — Task 4 below makes `AcpSessionService` itself responsible for sweeping the registry on every termination path (including the one inside `send_prompt`'s timeout handling, which this command layer cannot see), so no *additional* registry call is added at this layer for those two commands beyond what Plan 04 already wired.

- [ ] **Step 7: Update the frontend's `startAgentSession` call site for the new `collection` parameter**

```bash
grep -rn "start_agent_session\|startAgentSession" src/lib/tauri-api.ts src/components/request/AgentChatPanel.tsx
```

Add a `collection: string` parameter to the TS wrapper in `src/lib/tauri-api.ts` (matching whatever pattern that file already uses for passing a collection name to other `invoke` calls) and pass the active collection name from `AgentChatPanel.tsx`'s existing call site. This is IPC-boundary plumbing only, no new UI in this plan (Plan 06 owns the actual UI checkbox) — the collection name a session is already scoped to is already available wherever `AgentChatPanel` currently calls `startAgentSession`, since Plan 03/subproject C already renders this panel per-collection.

- [ ] **Step 8: Update `src-tauri/src/lib.rs` construction**

`mcp_server_registry` and its `app.manage(Arc::clone(&mcp_server_registry))` registration already exist by this point (Plan 04, Task 4, Step 3) — do not create a second `let mcp_server_registry = ...` binding or a second `app.manage(...)` call for it here. This step only needs `acp_collection_repo` (new) and the `AcpSessionService::new(...)` call site update:

```rust
// src-tauri/src/lib.rs — add near the other per-purpose SharedPathCollectionRepo
// instances (e.g. contract_svc's), before acp_session_svc is constructed:
let acp_collection_repo: Arc<dyn rocket_collection::CollectionRepository> = Arc::new(
    SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path)),
);
```

Update the existing `AcpSessionService::new(...)` call site (Task 4 adds one more argument to this same call — write the final 4-argument-plus-sweeper form directly once Task 4's sweeper type exists, or the 4-argument form without it first and let Task 4 add the fifth, since both tasks are in this same plan executed in order):

```rust
let acp_session_svc = rocket_app::AcpSessionService::new(
    Box::new(rocket_infra::AcpAgentClient::new()),
    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
    acp_agent_config_svc,
    Arc::clone(&acp_collection_repo),
);
```

(No new `app.manage(...)` call is needed for the registry itself in this step — it is already managed by Plan 04. Task 4 below adds `TauriMcpServerSweeper::new(Arc::clone(&mcp_server_registry))` as `AcpSessionService::new`'s 5th argument, reusing that same pre-existing `mcp_server_registry` local.)

- [ ] **Step 9: `cargo check --workspace -j4`**

Expected: green. Fix any remaining call sites the compiler flags (e.g. other tests constructing `AcpSessionService` directly) the same way Step 4 did.

- [ ] **Step 10: `yarn tsc --noEmit`**

Expected: clean, confirming Step 7's TypeScript change compiles.

- [ ] **Step 11: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add crates/rocket-app/src/acp_session_service.rs crates/rocket-app/src/lib.rs \
        src-tauri/src/lib.rs src-tauri/src/commands/acp_sessions.rs \
        src/lib/tauri-api.ts src/components/request/AgentChatPanel.tsx
```

---

### Task 4: Sweep parked/closed sessions — close the timeout-path gap

**Gap found:** `AcpSessionService::send_prompt` force-kills a hung session directly (`self.session_client.end_session(session_id).await`, in its timeout branch) without going through the `end_agent_session` Tauri command. If MCP-server cleanup lived only in that command (or only in `McpServerRegistry` called from `src-tauri`), a timed-out session's HTTP server — a bound `127.0.0.1` port with a live bearer token — would never be torn down. The fix: `AcpSessionService` gets an injected, Tauri-free `McpServerSweeper` trait object and calls it from every place it already ends a session, so the Tauri-side registry (Plan 04's, reused per Task 3's note above — not a second registry) is swept uniformly no matter which path triggered the end.

**Files:**
- Modify: `crates/rocket-app/src/acp_session_service.rs`
- Modify: `crates/rocket-app/src/lib.rs` (re-export `McpServerSweeper`)
- Modify: `src-tauri/src/mcp/registry.rs` (add `TauriMcpServerSweeper`, appended below Plan 04's `McpServerRegistry`)
- Modify: `src-tauri/src/lib.rs` (construction call site, final argument)
- Test: `crates/rocket-app/src/acp_session_service.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Consumes: `crate::mcp::registry::McpServerRegistry` (Plan 04) — its real, synchronous `end_session`/`shutdown_all` methods (not `remove_and_shutdown`/`remove_all_and_shutdown` — those names belonged to the duplicate registry Task 3's note above removed).
- Produces: `rocket_app::McpServerSweeper` trait (`async fn sweep(&self, session_id: &str)`, `async fn sweep_all(&self)`) — nothing later in this plan depends on it further, but Plan 06's security tests (per the index's Review Focus) may exercise it.

- [ ] **Step 1: Define the sweeper trait in `rocket-app`**

```rust
// crates/rocket-app/src/acp_session_service.rs — trait, not a concrete type:
// the concrete registry lives in src-tauri (it manages `McpHttpServerHandle`,
// a Tauri-layer type per Plan 04), so this crate only depends on the
// abstraction, the same pattern already used for `AcpSessionClient`/
// `EventPublisher` on this same struct.
#[async_trait::async_trait]
pub trait McpServerSweeper: Send + Sync {
    /// Idempotent: shuts down and forgets the MCP HTTP server registered for
    /// this session, if any. A no-op when autonomy was off for this session
    /// or it was already swept.
    async fn sweep(&self, session_id: &str);
    /// Idempotent: shuts down and forgets every registered MCP HTTP server.
    async fn sweep_all(&self);
}

/// Test/no-op implementation — every existing test in this file that does
/// not care about MCP sweeping uses this, mirroring `NullEventPublisher`'s
/// role for `EventPublisher`.
pub struct NullMcpServerSweeper;
#[async_trait::async_trait]
impl McpServerSweeper for NullMcpServerSweeper {
    async fn sweep(&self, _session_id: &str) {}
    async fn sweep_all(&self) {}
}
```

- [ ] **Step 2: Add the field, thread it through the constructors, and call it at every termination point**

```rust
pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    mcp_sweeper: Box<dyn McpServerSweeper>,
    prompt_timeout: Duration,
}

impl AcpSessionService {
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        mcp_sweeper: Box<dyn McpServerSweeper>,
    ) -> Self {
        Self::with_prompt_timeout(
            session_client,
            event_publisher,
            agent_config_service,
            collection_repo,
            mcp_sweeper,
            DEFAULT_PROMPT_TIMEOUT,
        )
    }

    pub fn with_prompt_timeout(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        mcp_sweeper: Box<dyn McpServerSweeper>,
        prompt_timeout: Duration,
    ) -> Self {
        Self {
            session_client,
            event_publisher,
            agent_config_service,
            collection_repo,
            mcp_sweeper,
            prompt_timeout,
        }
    }
```

Update the timeout branch of `send_prompt` to sweep right after the forced kill:

```rust
            Err(_elapsed) => {
                let _ = self.session_client.end_session(session_id).await;
                self.mcp_sweeper.sweep(session_id).await;
                let message = format!(
                    "agent did not respond within {}s",
                    self.prompt_timeout.as_secs()
                );
                self.event_publisher.publish(DomainEvent::AcpSessionFailed {
                    session_id: session_id.to_string(),
                    error: message.clone(),
                });
                Err(DomainError::Internal(message))
            }
```

Update `end_session` and `end_all_sessions` to always sweep, regardless of the underlying client call's outcome — a stray MCP server for a session that's ending (or already gone) must still be cleaned up:

```rust
    pub async fn end_session(&self, session_id: &str) -> DomainResult<()> {
        let result = self.session_client.end_session(session_id).await;
        self.mcp_sweeper.sweep(session_id).await;
        result
    }

    pub async fn end_all_sessions(&self) -> DomainResult<()> {
        let result = self.session_client.end_all_sessions().await;
        self.mcp_sweeper.sweep_all().await;
        result
    }
```

- [ ] **Step 3: Re-export from `rocket-app`**

```rust
// crates/rocket-app/src/lib.rs
pub use acp_session_service::{
    AcpSessionService, McpHttpServerCredentials, McpServerSweeper, NullMcpServerSweeper,
};
```

- [ ] **Step 4: Update every existing test call site in this file**

Every `AcpSessionService::new(...)` / `::with_prompt_timeout(...)` call in this file's `#[cfg(test)] mod tests` gets one more trailing argument: `Box::new(NullMcpServerSweeper)`, placed after the `collection_repo` argument Task 3 already added. (The exact count has drifted across this series' authoring — Plan 03 named 9 pre-existing call sites and added 4 more of its own in Task 4, Step 7, and Task 3 above of this plan adds one more — grep the real file for `AcpSessionService::new(` / `AcpSessionService::with_prompt_timeout(` rather than trusting any specific number stated in any of these plans, and update every match.)

- [ ] **Step 5: Write the two sweep-gap tests**

```rust
#[tokio::test]
async fn send_prompt_timeout_sweeps_the_mcp_server_for_that_session() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let client = FakeSessionClient {
        prompt_delay: Duration::from_millis(200),
        ..Default::default()
    };
    let swept: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sweeper = RecordingSweeper {
        swept: Arc::clone(&swept),
        swept_all: Arc::new(AtomicBool::new(false)),
    };
    let service = AcpSessionService::with_prompt_timeout(
        Box::new(client),
        Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        agent_config_service(),
        Arc::new(FakeCollectionRepo::with_autonomy_enabled("demo", false)),
        Box::new(sweeper),
        Duration::from_millis(20),
    );

    let _ = service.send_prompt("session-1", "hi".to_string()).await;

    assert_eq!(
        swept.lock().expect("lock").as_slice(),
        &["session-1".to_string()],
        "a timed-out session's MCP server must be swept, not just its agent process killed"
    );
}

#[tokio::test]
async fn end_all_sessions_sweeps_every_registered_mcp_server() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let swept_all = Arc::new(AtomicBool::new(false));
    let sweeper = RecordingSweeper {
        swept: Arc::new(Mutex::new(Vec::new())),
        swept_all: Arc::clone(&swept_all),
    };
    let service = AcpSessionService::new(
        Box::new(FakeSessionClient::default()),
        Box::new(SharedEventPublisher(Arc::clone(&publisher))),
        agent_config_service(),
        Arc::new(FakeCollectionRepo::with_autonomy_enabled("demo", false)),
        Box::new(sweeper),
    );

    service.end_all_sessions().await.expect("end_all_sessions should succeed");

    assert!(
        swept_all.load(Ordering::SeqCst),
        "end_all_sessions (the exit-sweep path) must sweep every MCP server, not just kill agent processes"
    );
}
```

with one more test double alongside `CapturingSessionClient`/`FakeCollectionRepo`:

```rust
struct RecordingSweeper {
    swept: Arc<Mutex<Vec<String>>>,
    swept_all: Arc<AtomicBool>,
}
#[async_trait::async_trait]
impl McpServerSweeper for RecordingSweeper {
    async fn sweep(&self, session_id: &str) {
        self.swept.lock().expect("lock").push(session_id.to_string());
    }
    async fn sweep_all(&self) {
        self.swept_all.store(true, Ordering::SeqCst);
    }
}
```

- [ ] **Step 6: Run the tests**

```bash
cargo test -p rocket-app acp_session_service -j4
```

Expected: all pass, including the two new sweep tests and every pre-existing test updated in Step 4.

- [ ] **Step 7: Implement the real sweeper in `src-tauri` and wire it**

```rust
// src-tauri/src/mcp/registry.rs — append below Plan 04's McpServerRegistry
// (and its #[cfg(test)] mod tests block, or above it — either is fine, just
// keep it outside that module). `end_session`/`shutdown_all` are Plan 04's
// real, synchronous methods on the one McpServerRegistry this subproject
// has — no `.await` here.
pub struct TauriMcpServerSweeper {
    registry: std::sync::Arc<McpServerRegistry>,
}

impl TauriMcpServerSweeper {
    pub fn new(registry: std::sync::Arc<McpServerRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl rocket_app::McpServerSweeper for TauriMcpServerSweeper {
    async fn sweep(&self, session_id: &str) {
        self.registry.end_session(session_id);
    }
    async fn sweep_all(&self) {
        self.registry.shutdown_all();
    }
}
```

```rust
// src-tauri/src/lib.rs — update the acp_session_svc construction from Task 3
let acp_session_svc = rocket_app::AcpSessionService::new(
    Box::new(rocket_infra::AcpAgentClient::new()),
    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
    acp_agent_config_svc,
    Arc::clone(&acp_collection_repo),
    Box::new(crate::mcp::registry::TauriMcpServerSweeper::new(Arc::clone(
        &mcp_server_registry,
    ))),
);
```

(`mcp_server_registry` here is the same `Arc<McpServerRegistry>` **Plan 04** already created and `app.manage`d (see Plan 04, Task 4, Step 3) — this task does not construct it again, it only clones the existing local binding into the sweeper. Construct `acp_session_svc` after `mcp_server_registry` already exists, both still before the `app.manage(...)` block.)

- [ ] **Step 8: `cargo check --workspace -j4`**

Expected: green.

- [ ] **Step 9: Manual end-to-end sanity check**

Run the app (`yarn tauri dev`); Plan 06 has not yet wired the "Allow this agent to run requests and edit files" checkbox, so instead directly flip `agentAutonomyEnabled: true` in a test collection's `opencollection.yml` under `extensions.rocketapi`, start an agent session against it, then kill the agent process out-of-band (`kill -9 <pid>` on the spawned agent binary) and confirm via `lsof -i :<port>` (Linux) that the port Rocket's log reported for that session's MCP server is no longer bound after `end_agent_session` or app exit.

- [ ] **Step 10: Commit**

Use the `dev-workflow-skills:1-git-commit` skill.

```bash
git add crates/rocket-app/src/acp_session_service.rs crates/rocket-app/src/lib.rs \
        src-tauri/src/mcp/registry.rs src-tauri/src/lib.rs
```

---

## Next Plan

**Plan 06 — Frontend checkbox + integration/security tests**
[2026-09-28-acp-mcp-tool-server-plan-06-frontend-and-tests.md](2026-09-28-acp-mcp-tool-server-plan-06-frontend-and-tests.md)

Plan 06 adds the `AgentChatPanel.tsx` "Allow this agent to run requests and edit files" checkbox (reading/writing `CollectionSettings.agentAutonomyEnabled` via the existing `getCollectionSettings`/`saveCollectionSettings` commands and the `collection` parameter Task 3 above added to `start_agent_session`'s frontend call site), plus the program-wide security tests the index's Review Focus calls for across all six plans: the `get_env_var`/`set_env_var` secret-oracle test, the mid-session toggle test, and the concurrent-write test.

## Post-Implementation Review

Dispatch an Opus-model subagent (`model: "opus"`) to review this plan's own diff (all four tasks above) against:

1. **Interface gaps vs. the index** — does `AcpSessionClient::start_session`'s real (Plan 01/02-landed) signature match what this plan assumed; does `McpServerSpec`'s real shape match; does `McpHttpServerHandle`'s real shutdown mechanism match the `pub async fn shutdown(self)` this plan assumed.
2. **The `AppHandle`-in-`rocket-app` risk called out in Task 3's Design Note** — confirm `rocket-app`'s `Cargo.toml`/source still has no *new* `tauri::AppHandle`-typed field or long-lived storage introduced by this plan (the pre-existing `LoadTestService` call-time parameter is out of scope to fix here, but must not have grown a sibling). If it has, fix it directly by moving the offending logic to `src-tauri`, following this plan's Task 3 pattern.
3. **Code quality and duplication** — confirm `stdio_bridge.rs` truly contains no per-tool logic (a `grep` for any of the 6 tool names inside `src-tauri/src/mcp/stdio_bridge.rs` should return nothing), and confirm there is exactly one `McpServerRegistry` in the codebase (`src-tauri/src/mcp/registry.rs`, from Plan 04) — this plan's Task 3/4 were corrected during cross-plan reconciliation to reuse it via `TauriMcpServerSweeper` rather than defining a second, `mcp_registry.rs`-housed one; grep for `mcp_registry` (the module name, not the variable) to confirm no stray duplicate file or import survived implementation.
4. **DDD boundaries** — `rocket-acp`, `rocket-collection`, and `rocket-app` still compile with zero `tauri` or `rmcp` imports introduced by this plan (only the pre-existing `rocket-app` → `tauri` edge for `load_test_service.rs` remains, unchanged).
5. **No panicking shorthand** — scan this plan's new/changed Rust files for any panicking shorthand call outside `#[cfg(test)]` blocks; fix any found.

Grant this subagent authority to fix what it finds directly (small, targeted diffs only — not a rewrite), then re-run `cargo check --workspace -j4` and the tests this plan added before considering the review closed.
