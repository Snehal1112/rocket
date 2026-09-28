# ACP MCP Tool Server — Plan 04: HTTP MCP Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the in-process `rmcp`-based HTTP MCP tool server (`src-tauri/src/mcp/`) that exposes Rocket's 6 agent tools over a bearer-token-guarded localhost HTTP endpoint, one instance per ACP session, torn down by the existing exit-sweep machinery.

**Architecture:** A new `mcp` module in `src-tauri` hosts an `rmcp` `ServerHandler` (`RocketMcpToolServer`) whose 6 `#[tool]` methods call straight through to the already-built `Arc<McpToolService>` (Plan 03, fetched at call time via `AppHandle::state`), mapping every `DomainResult<T>` to an MCP tool-call result explicitly (success or agent-visible error text — never a panic). That handler is hosted over `rmcp`'s Streamable HTTP transport (`axum` integration), behind an `axum` bearer-token middleware doing a constant-time comparison. `spawn_mcp_http_server` binds an OS-assigned `127.0.0.1` port, mints a UUID token, and registers the resulting handle in a new `McpServerRegistry` (Tauri-managed state) that the app's two existing exit-sweep call sites (the Unix signal listener and the `RunEvent::Exit` handler in `lib.rs`) now also drain, alongside the existing `AcpSessionClient::end_all_sessions()` call.

**Tech Stack:** Rust, `rmcp` (official MCP SDK) + `axum` (both new to this plan — added to `src-tauri/Cargo.toml`, not `rocket-infra/Cargo.toml` as the spec's prose literally says; see Global Constraints), `tokio`, `subtle` (constant-time comparison, already transitively present), `uuid` (already a workspace dependency, reused for the token).

**Spec:** [`docs/superpowers/specs/2026-09-28-acp-mcp-tool-server-design.md`](../../specs/2026-09-28-acp-mcp-tool-server-design.md)

**Plan index (locked interface contracts):** [`00-plan-index.md`](00-plan-index.md)

## Global Constraints

- `-j4` on every `cargo check`/`cargo test` invocation (this repo's convention).
- No panicking shorthand (the method spelled `u`+`n`+`w`+`r`+`a`+`p`+`(`+`)`) in any production code path — this repo's commit hook blocks that literal substring anywhere in a diff, including comments, so this document itself never writes it either. Use `.expect("...")` in tests, and explicit `match`/`?`/`ok_or_else` in production code.
- The spec's dependency-decision paragraph says the two new crates go in `crates/rocket-infra/Cargo.toml`; that is corrected here. The HTTP server, its `AppHandle`-based tool router, and the auth middleware all live in `src-tauri` per the spec's own architecture section and the plan index's locked file path (`src-tauri/src/mcp/tool_server.rs`) — `rocket-infra` has no code in this plan's scope that touches `rmcp` or `axum` at all. Both dependencies are therefore added to `src-tauri/Cargo.toml`.
- One HTTP server instance per ACP session; bound to `127.0.0.1` only; OS-assigned port (`127.0.0.1:0`).
- Bearer token compared with `subtle::ConstantTimeEq`, never `==` or a hand-rolled loop.
- `camelCase` `serde` rename applies only at the Tauri IPC boundary. None of this plan's new types (tool param structs, `McpHttpServerHandle`) cross that boundary — they are MCP wire types or Tauri-internal state — so none of them get a rename attribute.
- The token is never logged and never placed in a process's argv (that constraint's actual enforcement point is Plan 05's stdio shim, but this plan's own code must not introduce a logging call that would print it either).
- `rmcp`'s exact macro/type names are **not yet verified against installed source** as this plan is written (no `Cargo.lock` entry for `rmcp` exists yet). Task 1, Step 1 is the mandatory first real step of this plan: confirm every `rmcp` name used below against the version `cargo add` actually resolves, before writing the handler impl in Task 2. Every code block below that touches `rmcp` types funnels through one `use` block per file specifically so a name correction is a one-line fix, not a rewrite.

## Review Focus

- Missing or incorrect bearer token must be rejected on **every** MCP route this server exposes, not just a single smoke-tested endpoint — Task 3's middleware test covers the router in front of all 6 tools at once (the middleware wraps the whole `/mcp` service, not per-tool), and Task 4's integration test re-confirms this over a real socket.
- `get_env_var`/`set_env_var` errors for "key not found" vs. "key is secret" must stay byte-for-byte identical after passing through this layer's `DomainResult` → tool-result mapping (Plan 03 guarantees the *service* returns one generic message for both; this plan's `to_tool_result` must not accidentally introduce a difference, e.g. by formatting one path with `{:?}` and the other with `{}`) — a same-layer regression test in Task 2 asserts the two tool-result texts are equal.
- A malformed tool input (specifically: an `edit_script` `phase` string that isn't `pre_request`/`post_response`/`tests`) must return a clean, agent-visible tool error, not a panic and not an opaque protocol-level failure — Task 2's test.
- Two concurrent tool calls against one running server instance (e.g. `run_request` followed immediately by a second, unrelated `run_request` before the first's response is read) must not deadlock the single `axum::serve` task or corrupt the per-session test-result cache — Task 4's integration test issues two concurrent HTTP calls against one spawned server.
- After `shutdown()`/`shutdown_all()`/`end_session()` runs, the OS port must actually become free again (a new listener can rebind it), not merely have a shutdown signal fired with nobody left to observe whether the socket really closed — Task 4's integration test polls for the port to become rebindable.

---

### Task 1: Dependencies, verified `rmcp` API shape, and a minimal compiling skeleton

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/mcp/mod.rs`
- Create: `src-tauri/src/mcp/tool_server.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod mcp;`)

**Interfaces:**
- Produces: `rocket_lib::mcp::tool_server::RocketMcpToolServer` (struct, `Clone`, with `pub fn new(app_handle: tauri::AppHandle, session_id: String) -> Self`), implementing `rmcp::ServerHandler`. No `#[tool]` methods yet (Task 2 adds them into the same `#[tool_router]` impl block created here).
- Consumes: nothing from other tasks yet.

- [ ] **Step 1: Verify the real `rmcp` API before writing any handler code**

This is the load-bearing step of this whole plan. Everything below it assumes the following API shape, reconstructed from public `rmcp` documentation and example code at plan-writing time (2026-09-28) — **not** from a locally vendored copy, since no `Cargo.lock` entry for `rmcp` exists in this repo yet. Before writing Step 3's code, do all of:

1. Run `cargo add rmcp --features server,macros,transport-streamable-http-server` and `cargo add axum@0.8 schemars@1 subtle@2` inside `src-tauri/` (Step 2 below does this for real — this step is "read what `cargo add`/`cargo doc` show you", not "assume the plan's version numbers are right").
2. Run `cargo doc -p rmcp -j4 --no-deps --open` (or read `~/.cargo/registry/src/*/rmcp-*/src/`) and confirm each of the following names — if any differ, fix the `use` block at the top of `tool_server.rs`/`auth.rs` and nowhere else, since every other line in this plan refers to these local names, not the fully-qualified `rmcp` path:
   - The tool-definition macros: `#[rmcp::tool_router]` (on an `impl` block), `#[rmcp::tool]` (per method), `#[rmcp::tool_handler]` (on the `impl ServerHandler` block). Confirm the generated struct field type is `rmcp::handler::server::tool::ToolRouter<Self>` and that the macro expects a field literally named `tool_router` initialized via `Self::tool_router()`.
   - The parameter wrapper: `rmcp::handler::server::tool::Parameters<T>` (tuple struct, `Parameters(inner)`), and that `T` needs `serde::Deserialize` + `schemars::JsonSchema`.
   - The result/error types: `rmcp::model::CallToolResult` (confirm it has `CallToolResult::success(Vec<Content>) -> Self` and `CallToolResult::error(Vec<Content>) -> Self` constructors, and public fields `content: Vec<Content>` and `is_error: Option<bool>`), `rmcp::model::Content` (confirm a `Content::text(impl Into<String>) -> Self` constructor and an accessor to read a text content block back out, e.g. `.as_text()` returning something with a `.text` field — needed only by this plan's own tests). Confirm the protocol-level error type's real name and path (referred to as `McpError` in public examples; may be `rmcp::ErrorData` or live at `rmcp::model::ErrorData` in the installed version) and that it has an `internal_error(msg: impl Into<String>, data: Option<serde_json::Value>) -> Self` constructor or equivalent.
   - `ServerHandler::get_info(&self) -> rmcp::model::ServerInfo`, and the `ServerInfo { protocol_version, capabilities, server_info, instructions }` field names, plus `rmcp::model::ServerCapabilities::builder().enable_tools().build()` and `rmcp::model::Implementation { name, version }`.
   - The HTTP transport: `rmcp::transport::streamable_http_server::StreamableHttpService::new(service_factory, session_manager, config)`, where `service_factory: impl Fn() -> Result<S, std::io::Error> + Send + Sync + 'static`, and `rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default().into()` for the session-manager argument. Confirm `StreamableHttpService` implements `tower::Service` in a way `axum::Router::nest_service` accepts directly.
   - Check the crate's changelog/advisories for the session-table DoS advisory affecting `LocalSessionManager` on `rmcp` versions before 1.7.0 (GHSA-9pj6-vhgr-3mwh) — confirm the version `cargo add` resolved is patched (any 2.x/3.x release should be; if it resolves to something older, pin a newer patch version explicitly in `Cargo.toml` instead of accepting the default resolution).
3. Do not proceed to Step 3 until every name above has been confirmed or corrected in your own notes. If several names differ from this plan's text, prefer editing this plan file's remaining code blocks to match reality (so the plan stays accurate for whoever reads it next) over silently diverging from it while implementing.

- [ ] **Step 2: Add the dependencies**

Edit `src-tauri/Cargo.toml`'s `[dependencies]` section (after the existing `base64.workspace = true` line):

```toml
# MCP tool server (Plan 04) — official MCP SDK + the HTTP server crate it
# integrates with. Not workspace-shared: no other crate in this workspace
# needs either of these (rocket-infra maps McpServerSpec to the
# agent-client-protocol crate's own McpServer type, never to rmcp; Plan 02's
# own rmcp/axum addition to rocket-infra/Cargo.toml went unused there for the
# same reason and should be treated as dead weight, not a second source of
# truth for this feature set).
#
# Feature list is the union of what this plan's HTTP server needs (server,
# macros, transport-streamable-http-server) and what Plan 05's stdio bridge
# needs (transport-io, client, transport-streamable-http-client-reqwest) —
# both transports are declared here, in this one line, so Plan 05 does not
# add a second, conflicting `rmcp = { ... }` entry to this same file.
rmcp = { version = "3.5", features = [
    "server",
    "macros",
    "transport-streamable-http-server",
    "transport-io",
    "client",
    "transport-streamable-http-client-reqwest",
] }
axum = "0.8"
schemars = "1"
subtle = "2"
```

`"3.5"` matches the exact version Plan 02 already confirmed via `cargo add rmcp --dry-run` against the live crates.io index (`crates/rocket-infra/Cargo.toml`'s now-unused entry) — reuse that grounded finding rather than re-resolving it here. Still run `cargo add rmcp --features server,macros,transport-streamable-http-server,transport-io,client,transport-streamable-http-client-reqwest -p rocket` followed by `cargo add axum@0.8 schemars@1 subtle@2 -p rocket` (package name `rocket` per `src-tauri/Cargo.toml`'s `[package] name = "rocket"`) from the workspace root, so `cargo` actually pins a real, resolvable version into `Cargo.lock` — adjust the version string above if `cargo add` resolves something other than `3.5.x` (e.g. a newer patch release by the time this plan executes).

Add to `src-tauri/Cargo.toml`'s `[dev-dependencies]` section (it currently has only `tempfile = "3"`):

```toml
tower = { version = "0.5", features = ["util"] }
tauri = { version = "2", features = ["test"] }
```

`tower`'s `ServiceExt::oneshot` is used by Task 3's middleware test to call an `axum::Router` directly without opening a real socket. `tauri`'s `"test"` feature is needed here, not only in Plan 05 — this plan's own tests (Task 1 Step 5's `get_info_reports_the_rocket_tool_server_identity`, Task 2's tool-router tests, Task 4's `mcp_http_server_integration.rs`) all call `tauri::test::mock_builder()`/`mock_context()`/`noop_assets()`, which only exist when that feature is enabled. Cargo unions this with the plain `tauri = { version = "2", features = [] }` already in `[dependencies]` for test builds — adding it here (rather than leaving it to Plan 05, which only needs it for its own later round-trip test) keeps this plan's own tests compiling without depending on a later plan's `Cargo.toml` edit landing first, per this series' "each plan must leave the workspace compiling on its own" rule.

- [ ] **Step 3: Run `cargo check` to confirm the new dependencies resolve**

Run: `cargo check --workspace -j4`
Expected: succeeds (the new dependencies compile; nothing yet uses them, so no new warnings about unused items should appear beyond the crates themselves being unused, which `cargo check` does not flag for a leaf binary crate's direct dependencies).

- [ ] **Step 4: Create the `mcp` module and register it**

Create `src-tauri/src/mcp/mod.rs`:

```rust
//! In-process MCP tool server for ACP agent sessions (Subproject D). One
//! HTTP server instance per ACP session, hosting the 6 tools defined in
//! `tool_server`, guarded by the bearer-token check in `auth`, and tracked
//! by `registry` so the app's exit-sweep machinery can shut every live
//! instance down.

pub mod auth;
pub mod registry;
pub mod tool_server;
```

Modify `src-tauri/src/lib.rs`: add `pub mod mcp;` alongside the existing `mod` declarations at the top of the file:

```rust
mod audit_bridge;
mod commands;
pub mod mcp;
mod tauri_event_bus;
mod tauri_tracing_layer;
```

(`pub` because Task 4's integration test lives under `src-tauri/tests/`, a separate crate that links against this crate's `rocket_lib` library target and needs `rocket_lib::mcp::...` to be reachable.)

- [ ] **Step 5: Write the minimal skeleton — struct, empty tool router, `ServerHandler::get_info`**

Create `src-tauri/src/mcp/tool_server.rs`:

```rust
//! The MCP tool server exposed to ACP agents: one `rmcp` `ServerHandler`
//! wired to `Arc<McpToolService>` (`rocket-app`, Plan 03) via `AppHandle`
//! managed state, hosted over `rmcp`'s Streamable HTTP transport.
//!
//! Every `rmcp` name below was verified against the installed crate version
//! in Task 1, Step 1. If a future `rmcp` upgrade renames any of these, this
//! `use` block — and the mirroring one in this module's own tests — are the
//! only places that should need to change.
use rmcp::handler::server::tool::{Parameters, ToolRouter};
use rmcp::model::{
    CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo,
};
use rmcp::{tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use std::sync::Arc;

use rocket_app::McpToolService;

/// One `RocketMcpToolServer` instance backs exactly one ACP session's MCP
/// HTTP endpoint. `session_id` is fixed at construction (see
/// `spawn_mcp_http_server`'s doc comment for why it cannot be a per-call
/// parameter) and threaded into every `McpToolService` call for that
/// session's audit trail.
#[derive(Clone)]
pub struct RocketMcpToolServer {
    app_handle: tauri::AppHandle,
    session_id: String,
    tool_router: ToolRouter<Self>,
}

impl RocketMcpToolServer {
    pub fn new(app_handle: tauri::AppHandle, session_id: String) -> Self {
        Self {
            app_handle,
            session_id,
            tool_router: Self::tool_router(),
        }
    }
}

/// Looks up the `Arc<McpToolService>` this Tauri app manages. A missing
/// registration is a wiring bug (Plan 05 must `app.manage(Arc::new(...))` it
/// before any session can start), not a business-rule refusal, so it is
/// surfaced as a protocol-level error rather than a tool-result error.
fn mcp_tool_service(app_handle: &tauri::AppHandle) -> Result<Arc<McpToolService>, McpError> {
    match app_handle.try_state::<Arc<McpToolService>>() {
        Some(state) => Ok(Arc::clone(state.inner())),
        None => Err(McpError::internal_error(
            "McpToolService is not managed on this AppHandle",
            None,
        )),
    }
}

#[tool_router]
impl RocketMcpToolServer {
    // Task 2 adds the 6 `#[tool]` methods here.
}

#[tool_handler]
impl ServerHandler for RocketMcpToolServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "rocket-mcp-tool-server".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "Rocket ACP tool server: run requests, edit scripts, and read/write \
                 non-secret environment variables for one active session."
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app_handle() -> tauri::AppHandle {
        tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock tauri app")
            .handle()
            .clone()
    }

    #[test]
    fn get_info_reports_the_rocket_tool_server_identity() {
        let server = RocketMcpToolServer::new(test_app_handle(), "session-1".to_string());
        let info = server.get_info();
        assert_eq!(info.server_info.name, "rocket-mcp-tool-server");
    }
}
```

- [ ] **Step 6: Run the skeleton test**

Run: `cargo test -p rocket get_info_reports_the_rocket_tool_server_identity -j4`
Expected: PASS. If `tauri::test::mock_builder`/`mock_context`/`noop_assets` do not exist under those exact names in the installed `tauri` 2.x version, this is the second (smaller) spot in this plan where a name needs confirming against real source — check `tauri`'s `test` module docs and adjust only this helper function, everywhere it is reused (Task 2 and the Task 4 integration test both reuse the same pattern).

- [ ] **Step 7: `cargo check --workspace`**

Run: `cargo check --workspace -j4`
Expected: clean.

- [ ] **Step 8: Commit**

Use the `dev-workflow-skills:1-git-commit` skill (per this repo's global instructions — never a freeform `git commit -m`) to commit `src-tauri/Cargo.toml`, `Cargo.lock`, `src-tauri/src/mcp/mod.rs`, `src-tauri/src/mcp/tool_server.rs`, `src-tauri/src/lib.rs`.

---

### Task 2: Implement the 6 tools

**Files:**
- Modify: `src-tauri/src/mcp/tool_server.rs`

**Interfaces:**
- Consumes: `rocket_app::McpToolService`'s 6 methods. **Note: Plan 03 (the plan that actually implements `McpToolService`) found that `get_env_var`/`set_env_var`/`get_test_results` need an explicit `collection: &str` parameter that the plan index's original signatures omitted — environments are resolved per-`(collection, name)` everywhere in this codebase, and the autonomy gate itself needs a collection to check. The real, final signatures (matching `crates/rocket-app/src/mcp_tool_service.rs` as landed by Plan 03) are used below**: `list_collection_requests(&self, session_id: &str, collection: &str) -> DomainResult<Vec<McpRequestEntry>>`, `async fn run_request(&self, session_id: &str, collection: &str, request_path: &str, environment_name: Option<&str>) -> DomainResult<McpRunResult>`, `edit_script(&self, session_id: &str, collection: &str, request_path: &str, phase: rocket_collection::RequestScriptPhase, body: String) -> DomainResult<()>`, `get_env_var(&self, session_id: &str, collection: &str, environment_name: &str, key: &str) -> DomainResult<String>`, `set_env_var(&self, session_id: &str, collection: &str, environment_name: &str, key: &str, value: String) -> DomainResult<()>`, `get_test_results(&self, session_id: &str, collection: &str, request_path: &str) -> DomainResult<Vec<rocket_scripting::TestResult>>`.
- Produces: the completed `#[tool_router] impl RocketMcpToolServer` block with all 6 tools registered, callable both directly (as plain async methods, for this task's own tests) and via the MCP tool-call protocol (verified in Task 4).

- [ ] **Step 1: Add the 6 param structs and the `DomainResult` → `CallToolResult` mapping helper**

Insert above the `#[tool_router] impl RocketMcpToolServer` block in `tool_server.rs`:

```rust
use rocket_shared::error::DomainResult;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ListCollectionRequestsParams {
    pub collection: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RunRequestParams {
    pub collection: String,
    pub request_path: String,
    #[serde(default)]
    pub environment_name: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EditScriptParams {
    pub collection: String,
    pub request_path: String,
    /// One of "pre_request", "post_response", "tests".
    pub phase: String,
    pub body: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetEnvVarParams {
    pub collection: String,
    pub environment_name: String,
    pub key: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetEnvVarParams {
    pub collection: String,
    pub environment_name: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetTestResultsParams {
    pub collection: String,
    pub request_path: String,
}

/// Maps a service call's outcome to an MCP tool result. `Ok` becomes a
/// success result carrying the value as JSON text; `Err` becomes an
/// *agent-visible* tool error (`CallToolResult::error`, `is_error: true`),
/// never a protocol-level failure — per the spec, a refusal (autonomy
/// disabled, secret variable, not found) must reach the agent as something
/// it can explain to the user, not a generic transport failure. This is also
/// why `get_env_var`/`set_env_var`'s "not found" and "is secret" errors stay
/// indistinguishable through this layer: both are plain `DomainError`
/// values, both go through this one `e.to_string()` call, so nothing here
/// can accidentally format one differently from the other.
fn to_tool_result<T: serde::Serialize>(result: DomainResult<T>) -> CallToolResult {
    match result {
        Ok(value) => {
            let text = serde_json::to_string(&value).unwrap_or_else(|e| {
                format!("{{\"error\":\"failed to serialize tool result: {e}\"}}")
            });
            CallToolResult::success(vec![Content::text(text)])
        }
        Err(e) => CallToolResult::error(vec![Content::text(e.to_string())]),
    }
}

/// Parses the wire-format `phase` string into the domain enum. An unknown
/// value is a malformed-input case, so it is handled the same way as any
/// other tool-level refusal: an agent-visible `CallToolResult::error`, not a
/// protocol failure and not a call into `McpToolService` at all.
fn parse_phase(phase: &str) -> Result<rocket_collection::RequestScriptPhase, CallToolResult> {
    match phase {
        "pre_request" => Ok(rocket_collection::RequestScriptPhase::PreRequest),
        "post_response" => Ok(rocket_collection::RequestScriptPhase::PostResponse),
        "tests" => Ok(rocket_collection::RequestScriptPhase::Tests),
        other => Err(CallToolResult::error(vec![Content::text(format!(
            "unknown script phase '{other}': expected pre_request, post_response, or tests"
        ))])),
    }
}
```

- [ ] **Step 2: Implement the 6 `#[tool]` methods**

Replace the empty `#[tool_router] impl RocketMcpToolServer { }` block with:

```rust
#[tool_router]
impl RocketMcpToolServer {
    #[tool(description = "List the requests in a Rocket collection (path, name, method, url).")]
    async fn list_collection_requests(
        &self,
        Parameters(params): Parameters<ListCollectionRequestsParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.list_collection_requests(&self.session_id, &params.collection);
        Ok(to_tool_result(result))
    }

    #[tool(description = "Execute a saved request and return its status, duration, and test pass/fail counts.")]
    async fn run_request(
        &self,
        Parameters(params): Parameters<RunRequestParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc
            .run_request(
                &self.session_id,
                &params.collection,
                &params.request_path,
                params.environment_name.as_deref(),
            )
            .await;
        Ok(to_tool_result(result))
    }

    #[tool(description = "Overwrite one script phase (pre_request, post_response, or tests) on a request.")]
    async fn edit_script(
        &self,
        Parameters(params): Parameters<EditScriptParams>,
    ) -> Result<CallToolResult, McpError> {
        let phase = match parse_phase(&params.phase) {
            Ok(phase) => phase,
            Err(tool_error) => return Ok(tool_error),
        };
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.edit_script(
            &self.session_id,
            &params.collection,
            &params.request_path,
            phase,
            params.body,
        );
        Ok(to_tool_result(result))
    }

    #[tool(description = "Read one non-secret environment variable's value.")]
    async fn get_env_var(
        &self,
        Parameters(params): Parameters<GetEnvVarParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_env_var(
            &self.session_id,
            &params.collection,
            &params.environment_name,
            &params.key,
        );
        Ok(to_tool_result(result))
    }

    #[tool(description = "Write one non-secret environment variable's value.")]
    async fn set_env_var(
        &self,
        Parameters(params): Parameters<SetEnvVarParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.set_env_var(
            &self.session_id,
            &params.collection,
            &params.environment_name,
            &params.key,
            params.value,
        );
        Ok(to_tool_result(result))
    }

    #[tool(description = "Read the cached test results from this session's most recent run of a request.")]
    async fn get_test_results(
        &self,
        Parameters(params): Parameters<GetTestResultsParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_test_results(&self.session_id, &params.collection, &params.request_path);
        Ok(to_tool_result(result))
    }
}
```

- [ ] **Step 3: Build the shared test fixture**

Append to `tool_server.rs`'s existing `#[cfg(test)] mod tests` block (from Task 1). This fixture uses real `rocket-infra` filesystem repos over a `tempfile::TempDir`, following this repo's established fixture style (`crates/rocket-infra/CLAUDE.md`: "tempfile for fs fixtures") rather than hand-rolling a fake of `CollectionRepository`'s ~20-method trait, which `rocket-app`'s own inline-mock style would otherwise require — `src-tauri` already depends on `rocket-infra`, so this is the lighter-weight, more realistic choice here:

```rust
use rocket_collection::{settings::CollectionSettings, CollectionRepository};
use rocket_collection::request::Request as CollectionRequest;
use rocket_environment::{Environment, EnvironmentRepositoryFactory, NullSecretStore, NullVaultSecretFetcher, Variable};
use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_infra::{
    FsCollectionRepo, FsCookieRepo, FsEnvironmentRepo, FsHistoryRepo, FsSecretManagerRepo,
    SharedCollectionEnvironmentRepo,
};
use rocket_shared::events::NullEventPublisher;
use rocket_shared::types::HttpMethod;
use std::sync::Mutex as StdMutex;
use tempfile::TempDir;

struct FakeHttpExecutor {
    status: u16,
}

#[async_trait::async_trait]
impl HttpExecutor for FakeHttpExecutor {
    async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
        Ok(HttpResponse {
            status: self.status,
            status_text: "OK".to_string(),
            headers: Vec::new(),
            body: "{}".to_string(),
            duration_ms: 1,
            ttfb_ms: 1,
            size_bytes: 2,
        })
    }
}

/// A "demo" collection with one request ("ping.yml"), one "dev" environment
/// holding one plain variable ("API_KEY") and one secret variable
/// ("SECRET_TOKEN"), and `agent_autonomy_enabled` set as requested.
struct TestFixture {
    _tmp: TempDir,
    app_handle: tauri::AppHandle,
    session_id: String,
}

impl TestFixture {
    fn new(agent_autonomy_enabled: bool) -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let ws_path = Arc::new(StdMutex::new(tmp.path().to_path_buf()));
        let collections_dir = tmp.path().join("collections");

        let setup_repo = FsCollectionRepo::new_standalone(collections_dir.clone());
        setup_repo.create("demo").expect("create collection");
        setup_repo
            .save_settings(
                "demo",
                &CollectionSettings {
                    agent_autonomy_enabled,
                    ..Default::default()
                },
            )
            .expect("save settings");
        setup_repo
            .save_request(
                "demo",
                "ping.yml",
                &CollectionRequest::new("Ping", HttpMethod::Get, "https://example.invalid/ping"),
            )
            .expect("save request");

        let env_factory = SharedCollectionEnvironmentRepo::new(Arc::clone(&ws_path));
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("API_KEY", "plain-value"));
        let mut secret_var = Variable::new("SECRET_TOKEN", "secret-value");
        secret_var.secret = true;
        env.set_variable(secret_var);
        env_factory
            .for_collection("demo")
            .save(&env)
            .expect("save environment");

        let exec_svc = rocket_app::RequestExecutionService::new(
            Box::new(FsEnvironmentRepo::new(tmp.path().join("global_environments"))),
            Arc::new(FakeHttpExecutor { status: 200 }),
            Box::new(FsHistoryRepo::new(tmp.path().join("history"))),
            Box::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
            Box::new(FsCookieRepo::new(tmp.path().join("cookies"))),
            Box::new(NullEventPublisher),
            Box::new(FsSecretManagerRepo::new(tmp.path().join("secret_managers.yml"))),
            Arc::new(NullSecretStore),
            Arc::new(NullVaultSecretFetcher),
        )
        .with_collection_env_repo_factory(Box::new(SharedCollectionEnvironmentRepo::new(
            Arc::clone(&ws_path),
        )));

        let mcp_tool_svc = Arc::new(McpToolService::new(
            Arc::new(FsCollectionRepo::new_standalone(collections_dir)),
            Arc::new(SharedCollectionEnvironmentRepo::new(Arc::clone(&ws_path))),
            Arc::new(exec_svc),
            Arc::new(NullEventPublisher),
        ));

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock tauri app");
        app.manage(mcp_tool_svc);

        Self {
            app_handle: app.handle().clone(),
            _tmp: tmp,
            session_id: "session-1".to_string(),
        }
    }

    fn server(&self) -> RocketMcpToolServer {
        RocketMcpToolServer::new(self.app_handle.clone(), self.session_id.clone())
    }
}

fn tool_text(result: &CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

fn tool_is_error(result: &CallToolResult) -> bool {
    result.is_error.unwrap_or(false)
}
```

- [ ] **Step 4: Write the failing tests**

Append:

```rust
#[tokio::test]
async fn list_collection_requests_returns_the_seeded_request_when_autonomy_is_enabled() {
    let fixture = TestFixture::new(true);
    let result = fixture
        .server()
        .list_collection_requests(Parameters(ListCollectionRequestsParams {
            collection: "demo".to_string(),
        }))
        .await
        .expect("tool call");

    assert!(!tool_is_error(&result));
    assert!(tool_text(&result).contains("ping"));
}

#[tokio::test]
async fn list_collection_requests_is_refused_when_autonomy_is_disabled() {
    let fixture = TestFixture::new(false);
    let result = fixture
        .server()
        .list_collection_requests(Parameters(ListCollectionRequestsParams {
            collection: "demo".to_string(),
        }))
        .await
        .expect("tool call");

    assert!(tool_is_error(&result));
}

#[tokio::test]
async fn edit_script_rejects_an_unknown_phase_without_touching_the_service() {
    let fixture = TestFixture::new(true);
    let result = fixture
        .server()
        .edit_script(Parameters(EditScriptParams {
            collection: "demo".to_string(),
            request_path: "ping.yml".to_string(),
            phase: "not-a-real-phase".to_string(),
            body: "console.log('hi')".to_string(),
        }))
        .await
        .expect("tool call");

    assert!(tool_is_error(&result));
    assert!(tool_text(&result).contains("unknown script phase"));
}

#[tokio::test]
async fn get_env_var_gives_identical_errors_for_missing_and_secret_keys() {
    let fixture = TestFixture::new(true);
    let server = fixture.server();

    let missing = server
        .get_env_var(Parameters(GetEnvVarParams {
            collection: "demo".to_string(),
            environment_name: "dev".to_string(),
            key: "DOES_NOT_EXIST".to_string(),
        }))
        .await
        .expect("tool call");
    let secret = server
        .get_env_var(Parameters(GetEnvVarParams {
            collection: "demo".to_string(),
            environment_name: "dev".to_string(),
            key: "SECRET_TOKEN".to_string(),
        }))
        .await
        .expect("tool call");

    assert!(tool_is_error(&missing));
    assert!(tool_is_error(&secret));
    assert_eq!(
        tool_text(&missing),
        tool_text(&secret),
        "a missing key and a secret key must be indistinguishable to the agent"
    );
}

#[tokio::test]
async fn get_env_var_returns_a_plain_variables_value() {
    let fixture = TestFixture::new(true);
    let result = fixture
        .server()
        .get_env_var(Parameters(GetEnvVarParams {
            collection: "demo".to_string(),
            environment_name: "dev".to_string(),
            key: "API_KEY".to_string(),
        }))
        .await
        .expect("tool call");

    assert!(!tool_is_error(&result));
    assert!(tool_text(&result).contains("plain-value"));
}

#[tokio::test]
async fn run_request_then_get_test_results_round_trips_through_the_session_cache() {
    let fixture = TestFixture::new(true);
    let server = fixture.server();

    let run_result = server
        .run_request(Parameters(RunRequestParams {
            collection: "demo".to_string(),
            request_path: "ping.yml".to_string(),
            environment_name: None,
        }))
        .await
        .expect("tool call");
    assert!(!tool_is_error(&run_result));

    let results = server
        .get_test_results(Parameters(GetTestResultsParams {
            collection: "demo".to_string(),
            request_path: "ping.yml".to_string(),
        }))
        .await
        .expect("tool call");
    assert!(!tool_is_error(&results));
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p rocket mcp::tool_server -j4`
Expected: all 6 tests PASS. If `Content::as_text()` or `CallToolResult`'s field names differ from this plan's guess, this is exactly the kind of correction Task 1 Step 1 flagged — fix `tool_text`/`tool_is_error` only.

- [ ] **Step 6: `cargo check --workspace`**

Run: `cargo check --workspace -j4`
Expected: clean.

- [ ] **Step 7: Commit**

Use `dev-workflow-skills:1-git-commit` for `src-tauri/src/mcp/tool_server.rs`.

---

### Task 3: Bearer-token authentication middleware

**Files:**
- Create: `src-tauri/src/mcp/auth.rs`

**Interfaces:**
- Produces: `pub async fn require_bearer_token(State(expected_token): State<String>, req: Request, next: Next) -> Response` — an `axum::middleware::from_fn_with_state`-compatible handler. Consumed by Task 4's `spawn_mcp_http_server`.

- [ ] **Step 1: Write the middleware and its constant-time comparison**

Create `src-tauri/src/mcp/auth.rs`:

```rust
//! Bearer-token authentication for the per-session MCP HTTP server.
//!
//! Chosen over a hand-rolled `tower::Layer`: `axum::middleware::from_fn_with_state`
//! is the documented, idiomatic way to run a stateful async check in front of
//! an axum router without writing a `Service`/`Layer` pair by hand, and every
//! request here needs exactly one thing checked (the `Authorization` header
//! against this session's token) — a full custom `Layer` would just
//! re-implement `from_fn_with_state`'s plumbing for no extra benefit.
//!
//! The token comparison uses `subtle::ConstantTimeEq` rather than `==`, so a
//! byte-by-byte mismatch does not return early and leak timing information
//! about how many leading bytes of a guessed token were correct. `subtle` is
//! already present in this workspace's dependency graph (pulled in
//! transitively by `sha2`/`hmac`), so this adds no new supply-chain surface —
//! and hand-rolling constant-time comparison is exactly the kind of code a
//! well-audited, purpose-built crate should be preferred over, since it is
//! easy to defeat by accident (an early `return false`, or the compiler
//! optimizing a naive XOR-and-check loop into a short-circuiting comparison).

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

/// Rejects any request whose `Authorization: Bearer <token>` header does not
/// match `expected_token` in constant time. A missing header, wrong scheme,
/// and a mismatched token are all treated identically (401), so a caller
/// cannot distinguish "no token supplied" from "wrong token" through timing
/// or response shape.
pub async fn require_bearer_token(
    State(expected_token): State<String>,
    req: Request,
    next: Next,
) -> Response {
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    let authorized = match provided {
        Some(token) => tokens_match(token.as_bytes(), expected_token.as_bytes()),
        None => false,
    };

    if authorized {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "invalid or missing bearer token").into_response()
    }
}

/// Constant-time byte comparison. The length check runs first — this
/// compares *lengths*, not *contents*, and the token's length (a fixed-size
/// UUID string) is not a secret, so this does not reintroduce the timing
/// side channel the constant-time comparison exists to close.
fn tokens_match(provided: &[u8], expected: &[u8]) -> bool {
    provided.len() == expected.len() && bool::from(provided.ct_eq(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn test_router(token: &str) -> Router {
        Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(
                token.to_string(),
                require_bearer_token,
            ))
    }

    #[tokio::test]
    async fn correct_bearer_token_is_allowed() {
        let request = Request::builder()
            .uri("/ping")
            .header(header::AUTHORIZATION, "Bearer secret-token")
            .body(Body::empty())
            .expect("build request");

        let response = test_router("secret-token")
            .oneshot(request)
            .await
            .expect("router call");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_token_is_rejected() {
        let request = Request::builder()
            .uri("/ping")
            .body(Body::empty())
            .expect("build request");

        let response = test_router("secret-token")
            .oneshot(request)
            .await
            .expect("router call");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn incorrect_token_is_rejected() {
        let request = Request::builder()
            .uri("/ping")
            .header(header::AUTHORIZATION, "Bearer wrong-token")
            .body(Body::empty())
            .expect("build request");

        let response = test_router("secret-token")
            .oneshot(request)
            .await
            .expect("router call");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn empty_expected_token_still_rejects_a_request_with_no_header() {
        // Defends against a future refactor that treats an empty expected
        // token as "auth disabled" -- it must not.
        let request = Request::builder()
            .uri("/ping")
            .body(Body::empty())
            .expect("build request");

        let response = test_router("").oneshot(request).await.expect("router call");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p rocket mcp::auth -j4`
Expected: all 4 tests PASS.

- [ ] **Step 3: `cargo check --workspace`**

Run: `cargo check --workspace -j4`
Expected: clean.

- [ ] **Step 4: Commit**

Use `dev-workflow-skills:1-git-commit` for `src-tauri/src/mcp/auth.rs`.

---

### Task 4: Lifecycle, registry, and exit-sweep wiring

**Files:**
- Modify: `src-tauri/src/mcp/tool_server.rs` (add `McpHttpServerHandle`, `spawn_mcp_http_server`)
- Create: `src-tauri/src/mcp/registry.rs`
- Modify: `src-tauri/src/lib.rs:90-116` (`spawn_exit_signal_listener`), `:479-489` (managed-state block), `:709-719` (`RunEvent::Exit` handler)
- Modify: `src-tauri/src/commands/acp_sessions.rs` (`end_agent_session`)
- Create: `src-tauri/tests/mcp_http_server_integration.rs`

**Interfaces:**
- Produces: `pub struct McpHttpServerHandle { pub port: u16, pub token: String, /* shutdown handle */ }` with `pub fn shutdown(&self)`; `pub async fn spawn_mcp_http_server(app_handle: tauri::AppHandle, session_id: String) -> std::io::Result<McpHttpServerHandle>`; `pub struct McpServerRegistry` with `pub fn new() -> Self`, `pub fn register(&self, session_id: String, handle: McpHttpServerHandle)`, `pub fn end_session(&self, session_id: &str)`, `pub fn shutdown_all(&self)`. **This registry is the one and only `McpServerRegistry` in this subproject — Plan 05's sweeper (Task 4 of that plan) wraps this exact type via an `Arc` clone rather than defining a second, competing registry. See Plan 05 Task 3/4 for how it is reused.**
- Consumes: `RocketMcpToolServer::new` (Task 1/2), `require_bearer_token` (Task 3).

Note on the locked contract's function signature: the plan index (`00-plan-index.md`) writes `spawn_mcp_http_server(app_handle: tauri::AppHandle) -> ...`, without a `session_id` parameter. This plan adds `session_id: String` as a second parameter, because every `McpToolService` method (locked contract, Plan 03) takes `session_id: &str` as its first argument and `RocketMcpToolServer` needs one fixed at construction — there is no other point after this function returns where a session id could be attached to the running server. This is flagged here rather than silently diverging: whoever writes Plan 05 must resolve *which* identifier is available to pass in at the call site, since the real ACP-protocol session id is only known after the agent handshake completes, but the server's port/token must be included in the handshake's own request. That resolution is explicitly out of this plan's scope (see Next Plan, below).

- [ ] **Step 1: Write the failing registry unit tests**

Create `src-tauri/src/mcp/registry.rs`:

```rust
//! Tracks every live per-session MCP HTTP server so the app's existing
//! exit-sweep machinery (see `spawn_exit_signal_listener` and the
//! `RunEvent::Exit` handler in `lib.rs`) can shut all of them down, and so a
//! single ended ACP session can shut down just its own server.
//!
//! This is a separate registry from `AcpAgentClient`'s internal `sessions`/
//! `in_flight` maps (`crates/rocket-infra/src/acp_agent_client.rs`), not a
//! new field on that type: `McpHttpServerHandle` carries a `tauri` runtime
//! handle (an `Arc<Notify>` driven by a `tauri::async_runtime::spawn` task)
//! and is meaningless outside a running Tauri app, whereas `rocket-infra`
//! must not depend on `tauri` at all (this repo's DDD boundary —
//! `rocket-infra` is Tauri-agnostic I/O, wired into the app by `src-tauri`).
//! A parallel registry here, swept from the same two call sites that already
//! sweep `AcpAgentClient` (`spawn_exit_signal_listener`, the `RunEvent::Exit`
//! handler), gives the same guarantee — no bound port or live token survives
//! app exit — without crossing that boundary.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use crate::mcp::tool_server::McpHttpServerHandle;

/// Tauri-managed state (`app.manage(Arc::new(McpServerRegistry::new()))`). Holds one
/// handle per live ACP session's MCP HTTP server, keyed by session id.
#[derive(Default)]
pub struct McpServerRegistry {
    handles: Mutex<HashMap<String, McpHttpServerHandle>>,
}

impl McpServerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a newly spawned server so it is reachable by `end_session`/
    /// `shutdown_all`. Called by `spawn_mcp_http_server` itself.
    pub fn register(&self, session_id: String, handle: McpHttpServerHandle) {
        self.handles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id, handle);
    }

    /// Shuts down and forgets one session's server, if it has one. A no-op
    /// (not an error) if that session never had an HTTP server — e.g. agent
    /// autonomy was disabled for its collection, so `spawn_mcp_http_server`
    /// was never called for it.
    pub fn end_session(&self, session_id: &str) {
        let removed = self
            .handles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session_id);
        if let Some(handle) = removed {
            handle.shutdown();
        }
    }

    /// Shuts down every tracked server. Called from the same two app-exit
    /// paths that already call `AcpSessionClient::end_all_sessions`
    /// (`spawn_exit_signal_listener` and the `RunEvent::Exit` handler in
    /// `lib.rs`), so a crashed or killed app cannot leave a bound port or a
    /// live bearer token behind.
    pub fn shutdown_all(&self) {
        let handles: Vec<McpHttpServerHandle> = {
            let mut map = self.handles.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *map).into_values().collect()
        };
        for handle in handles {
            handle.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Notify;

    fn test_handle(port: u16, token: &str) -> (McpHttpServerHandle, Arc<Notify>) {
        let notify = Arc::new(Notify::new());
        let handle = McpHttpServerHandle {
            port,
            token: token.to_string(),
            shutdown: Arc::clone(&notify),
        };
        (handle, notify)
    }

    #[tokio::test]
    async fn end_session_notifies_only_that_sessions_shutdown() {
        let registry = McpServerRegistry::new();
        let (handle_a, notify_a) = test_handle(4000, "token-a");
        let (handle_b, notify_b) = test_handle(4001, "token-b");
        registry.register("session-a".to_string(), handle_a);
        registry.register("session-b".to_string(), handle_b);

        registry.end_session("session-a");

        // `Notify::notified()` resolves immediately once `notify_one` has
        // already fired, even called after the fact (a stored "permit"), so
        // this does not race `end_session` above.
        tokio::time::timeout(std::time::Duration::from_millis(50), notify_a.notified())
            .await
            .expect("session-a should have been notified to shut down");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), notify_b.notified())
                .await
                .is_err(),
            "session-b must not be notified by ending session-a"
        );
    }

    #[tokio::test]
    async fn shutdown_all_notifies_every_registered_session() {
        let registry = McpServerRegistry::new();
        let (handle_a, notify_a) = test_handle(4002, "token-a");
        let (handle_b, notify_b) = test_handle(4003, "token-b");
        registry.register("session-a".to_string(), handle_a);
        registry.register("session-b".to_string(), handle_b);

        registry.shutdown_all();

        for notify in [&notify_a, &notify_b] {
            tokio::time::timeout(std::time::Duration::from_millis(50), notify.notified())
                .await
                .expect("every registered session should be notified");
        }
    }
}
```

Run: `cargo test -p rocket mcp::registry -j4`
Expected: FAIL to compile (`McpHttpServerHandle` does not exist yet).

- [ ] **Step 2: Add `McpHttpServerHandle` and `spawn_mcp_http_server` to `tool_server.rs`**

Append to `src-tauri/src/mcp/tool_server.rs` (extending the `use rmcp::...` block at the top with the transport-layer names Task 1 Step 1 verified):

```rust
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use std::sync::Arc as StdArc;
use tokio::sync::Notify;
```

```rust
/// Handle to one running per-session MCP HTTP server. `port`/`token` are
/// what Plan 05's `McpServerSpec::Http` is built from; `shutdown()` stops the
/// server's `axum::serve` task. `Clone` so the same handle can be both
/// returned to `spawn_mcp_http_server`'s caller and stored in
/// `McpServerRegistry` — `Arc<Notify>` makes that safe: both clones
/// ultimately notify the same underlying `Notify`, and notifying it twice is
/// harmless (a second `notify_one` with no waiter left just stores an
/// unconsumed permit).
#[derive(Clone)]
pub struct McpHttpServerHandle {
    pub port: u16,
    pub token: String,
    pub(crate) shutdown: StdArc<Notify>,
}

impl McpHttpServerHandle {
    /// Signals the server's `axum::serve(...).with_graceful_shutdown(...)`
    /// future to stop accepting new connections and return, which drops the
    /// listening socket and frees the port. Fire-and-forget: callers on the
    /// app-exit path (`McpServerRegistry::shutdown_all`) have no one left to
    /// report a failure to, and there is nothing to fail here besides "no one
    /// is listening yet", which is harmless.
    pub fn shutdown(&self) {
        self.shutdown.notify_one();
    }
}

/// Binds a fresh localhost MCP HTTP server for one ACP session, registers it
/// with `McpServerRegistry` (Tauri-managed state — must already be present;
/// see `src-tauri/src/lib.rs`'s `app.manage(Arc::clone(&mcp_server_registry))`
/// call) so it is swept on app exit, and returns a handle carrying the port
/// and bearer token the caller advertises to the agent.
///
/// `session_id` is a parameter (not generated here) because tool calls need
/// it up front for their `DomainEvent::AcpToolInvoked` audit events, and
/// because one HTTP server instance serves exactly one ACP session for its
/// whole lifetime. See this task's header note on where that identifier
/// comes from — resolving that call-site question is Plan 05's job.
pub async fn spawn_mcp_http_server(
    app_handle: tauri::AppHandle,
    session_id: String,
) -> std::io::Result<McpHttpServerHandle> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    // A v4 UUID is 36 random-enough characters and is already this
    // workspace's established pattern for one-shot random tokens (see
    // `src-tauri/src/commands/oauth2.rs`'s OAuth `state` parameter) --
    // reusing it avoids adding a `rand` dependency for what `uuid` (already
    // a workspace dependency) already does well.
    let token = uuid::Uuid::new_v4().to_string();
    let shutdown = StdArc::new(Notify::new());

    let tool_server = RocketMcpToolServer::new(app_handle.clone(), session_id.clone());
    let service = StreamableHttpService::new(
        move || Ok(tool_server.clone()),
        LocalSessionManager::default().into(),
        Default::default(),
    );
    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(
            token.clone(),
            crate::mcp::auth::require_bearer_token,
        ));

    let handle = McpHttpServerHandle {
        port,
        token,
        shutdown: StdArc::clone(&shutdown),
    };

    if let Some(registry) = app_handle.try_state::<StdArc<crate::mcp::registry::McpServerRegistry>>() {
        registry.register(session_id, handle.clone());
    }

    tauri::async_runtime::spawn(async move {
        let shutdown_signal = async move { shutdown.notified().await };
        // Best-effort: this task runs on its own once spawned, so there is
        // no caller left to report a bind/serve failure to.
        let _ = axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(shutdown_signal)
            .await;
    });

    Ok(handle)
}
```

Run: `cargo test -p rocket mcp::registry -j4`
Expected: PASS (compiles now, both tests green).

- [ ] **Step 3: Wire `McpServerRegistry` into Tauri managed state and the exit-sweep call sites**

Modify `src-tauri/src/lib.rs`. Introduce a local `Arc<McpServerRegistry>` binding *before* the managed-state block, so it can be cloned both into `app.manage(...)` and (in Plan 05, Task 3) into `TauriMcpServerSweeper` without a second, differently-typed registration:

```rust
let mcp_server_registry = std::sync::Arc::new(mcp::registry::McpServerRegistry::new());
```

Then, in the managed-state block (currently lines 470-489):

```rust
            // Register all services as Tauri managed state.
            app.manage(collection_svc);
            app.manage(contract_svc);
            app.manage(history_svc);
            app.manage(template_svc);
            app.manage(cookie_svc);
            app.manage(exec_svc);
            app.manage(secret_manager_svc);
            app.manage(agent_config_svc);
            app.manage(acp_session_svc);
            app.manage(runner_svc);
            app.manage(flow_exec_svc);
            app.manage(executor);
            app.manage(oauth2_svc);
            app.manage(flow_svc);
            app.manage(git_svc);
            app.manage(CloneDestinationCapabilities::default());
            app.manage(audit_svc);
            app.manage(Mutex::new(workspace_svc));
            app.manage(active_workspace_path);
            app.manage(Arc::clone(&mcp_server_registry));
```

(`McpToolService` itself is managed by Plan 05, once `collection_svc`/`exec_svc`'s underlying `Arc`s are available for it to share — this task only needs the registry to exist so `spawn_mcp_http_server`'s `try_state` lookup and this task's own integration test can find it. `Arc` is already imported in this file. The registry is managed as `Arc<McpServerRegistry>`, not a bare `McpServerRegistry` — Plan 05 relies on being able to clone this same `Arc` into its `TauriMcpServerSweeper`, which is held for the whole lifetime of `AcpSessionService`, not looked up per call.)

In `spawn_exit_signal_listener` (currently lines 89-116), add the registry sweep next to the existing `acp_session_svc.end_all_sessions()` call:

```rust
            if let Some(acp_session_svc) = app_handle.try_state::<rocket_app::AcpSessionService>() {
                let _ = acp_session_svc.end_all_sessions().await;
            }
            if let Some(mcp_registry) = app_handle.try_state::<Arc<mcp::registry::McpServerRegistry>>() {
                mcp_registry.shutdown_all();
            }
            app_handle.exit(0);
```

In the `RunEvent::Exit` handler (currently lines 709-719):

```rust
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(acp_session_svc) =
                    app_handle.try_state::<rocket_app::AcpSessionService>()
                {
                    // Best-effort on app exit. The process is about to tear
                    // down regardless, so there is no caller left to report
                    // a kill failure to.
                    let _ = tauri::async_runtime::block_on(acp_session_svc.end_all_sessions());
                }
                if let Some(mcp_registry) = app_handle.try_state::<Arc<mcp::registry::McpServerRegistry>>() {
                    mcp_registry.shutdown_all();
                }
            }
        });
```

- [ ] **Step 4: Wire per-session teardown into `end_agent_session`**

Modify `src-tauri/src/commands/acp_sessions.rs`. **Note: by the time this plan runs, Plan 03 has already added a `collection: Option<String>` parameter to `start_agent_session`** (so `AcpSessionService::start_session` can look up `agent_autonomy_enabled`) — this task's job is only `end_agent_session`, so `start_agent_session`/`send_agent_prompt` below are shown unchanged from Plan 03's real shape, not reverted to their pre-Plan-03 form:

```rust
use std::sync::Arc;

use rocket_app::AcpSessionService;
use rocket_shared::error::DomainError;
use tauri::State;

use crate::mcp::registry::McpServerRegistry;

#[tauri::command]
pub async fn start_agent_session(
    agent_config_id: String,
    cwd: String,
    collection: Option<String>,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.start_session(&agent_config_id, &cwd, collection.as_deref())
        .await
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
    mcp_registry: State<'_, Arc<McpServerRegistry>>,
) -> Result<(), DomainError> {
    let result = svc.end_session(&session_id).await;
    // Always sweep the MCP server, even if the ACP session was already gone
    // (e.g. the agent process had already crashed) -- a no-op if this
    // session never had one (agent autonomy was off).
    mcp_registry.end_session(&session_id);
    result
}
```

This is a pre-emptive hook for Plan 05 (which is what will actually call `spawn_mcp_http_server` and so is the first plan where a registry entry can exist for `end_agent_session` to find) — wiring it now means Plan 05 does not need to touch this command file's `end_agent_session` at all (Plan 05 does still rewrite `start_agent_session` itself, to add the MCP-server spawn — see Plan 05 Task 3).

**Cross-plan note (registry is `Arc`-wrapped here, not a bare value):** `McpServerRegistry` is managed as `Arc<McpServerRegistry>` (not a bare `McpServerRegistry`) throughout this plan — see Step 3 below — specifically so Plan 05's `TauriMcpServerSweeper` (which is constructed once at startup and held for the lifetime of `AcpSessionService`, not fetched per-call via `AppHandle`) can hold a cloned `Arc` to the same registry instance this plan creates. Do not "simplify" this to a bare `app.manage(McpServerRegistry::new())` — Plan 05 depends on the `Arc`.

- [ ] **Step 5: `cargo check --workspace`**

Run: `cargo check --workspace -j4`
Expected: clean. `commands::acp_sessions::end_agent_session`'s new `State<'_, Arc<McpServerRegistry>>` parameter requires `Arc<McpServerRegistry>` to already be `app.manage`d before any webview loads a command that calls it — Step 3 already added that.

- [ ] **Step 6: Write the integration test**

Create `src-tauri/tests/mcp_http_server_integration.rs` (an external integration-test crate, per Cargo convention — it links against this crate's `rocket_lib` library target, so every `mcp` item it needs must be `pub`, which Task 1/2/4 already made them):

```rust
//! End-to-end tests for the per-session MCP HTTP server: real TCP, real
//! HTTP requests, real bearer-token enforcement. Complements the in-process
//! tool-router tests in `src-tauri/src/mcp/tool_server.rs` (Task 2), which
//! never open a socket.

use std::sync::{Arc, Mutex};

use reqwest::Client;
use rocket_app::McpToolService;
use rocket_collection::{settings::CollectionSettings, request::Request as CollectionRequest, CollectionRepository};
use rocket_environment::{NullSecretStore, NullVaultSecretFetcher};
use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_infra::{
    FsCollectionRepo, FsCookieRepo, FsEnvironmentRepo, FsHistoryRepo, FsSecretManagerRepo,
    SharedCollectionEnvironmentRepo,
};
use rocket_lib::mcp::registry::McpServerRegistry;
use rocket_lib::mcp::tool_server::{spawn_mcp_http_server, McpHttpServerHandle};
use rocket_shared::error::DomainResult;
use rocket_shared::events::NullEventPublisher;
use rocket_shared::types::HttpMethod;
use tempfile::TempDir;

struct FakeHttpExecutor;

#[async_trait::async_trait]
impl HttpExecutor for FakeHttpExecutor {
    async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
        Ok(HttpResponse {
            status: 200,
            status_text: "OK".to_string(),
            headers: Vec::new(),
            body: "{}".to_string(),
            duration_ms: 1,
            ttfb_ms: 1,
            size_bytes: 2,
        })
    }
}

/// Builds a mock Tauri app with `Arc<McpToolService>` and `McpServerRegistry`
/// both managed, over a fresh temp-directory "demo" collection with agent
/// autonomy on, and spawns an MCP HTTP server for `session_id` against it.
async fn spawn_test_server(session_id: &str) -> (McpHttpServerHandle, tauri::AppHandle, TempDir) {
    let tmp = TempDir::new().expect("tempdir");
    let ws_path = Arc::new(Mutex::new(tmp.path().to_path_buf()));
    let collections_dir = tmp.path().join("collections");

    let setup_repo = FsCollectionRepo::new_standalone(collections_dir.clone());
    setup_repo.create("demo").expect("create collection");
    setup_repo
        .save_settings(
            "demo",
            &CollectionSettings {
                agent_autonomy_enabled: true,
                ..Default::default()
            },
        )
        .expect("save settings");
    setup_repo
        .save_request(
            "demo",
            "ping.yml",
            &CollectionRequest::new("Ping", HttpMethod::Get, "https://example.invalid/ping"),
        )
        .expect("save request");

    let exec_svc = rocket_app::RequestExecutionService::new(
        Box::new(FsEnvironmentRepo::new(tmp.path().join("global_environments"))),
        Arc::new(FakeHttpExecutor),
        Box::new(FsHistoryRepo::new(tmp.path().join("history"))),
        Box::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
        Box::new(FsCookieRepo::new(tmp.path().join("cookies"))),
        Box::new(NullEventPublisher),
        Box::new(FsSecretManagerRepo::new(tmp.path().join("secret_managers.yml"))),
        Arc::new(NullSecretStore),
        Arc::new(NullVaultSecretFetcher),
    )
    .with_collection_env_repo_factory(Box::new(SharedCollectionEnvironmentRepo::new(
        Arc::clone(&ws_path),
    )));

    let mcp_tool_svc = Arc::new(McpToolService::new(
        Arc::new(FsCollectionRepo::new_standalone(collections_dir)),
        Arc::new(SharedCollectionEnvironmentRepo::new(Arc::clone(&ws_path))),
        Arc::new(exec_svc),
        Arc::new(NullEventPublisher),
    ));

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build mock tauri app");
    app.manage(mcp_tool_svc);
    app.manage(Arc::new(McpServerRegistry::new()));
    let app_handle = app.handle().clone();

    let handle = spawn_mcp_http_server(app_handle.clone(), session_id.to_string())
        .await
        .expect("spawn mcp http server");

    (handle, app_handle, tmp)
}

fn tools_list_body() -> serde_json::Value {
    serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}})
}

#[tokio::test]
async fn correct_token_reaches_the_tool_router_over_real_http() {
    let (handle, _app_handle, _tmp) = spawn_test_server("session-http-1").await;

    let response = Client::new()
        .post(format!("http://127.0.0.1:{}/mcp", handle.port))
        .bearer_auth(&handle.token)
        .header("content-type", "application/json")
        .json(&tools_list_body())
        .send()
        .await
        .expect("send request");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    handle.shutdown();
}

#[tokio::test]
async fn missing_token_is_rejected_over_real_http() {
    let (handle, _app_handle, _tmp) = spawn_test_server("session-http-2").await;

    let response = Client::new()
        .post(format!("http://127.0.0.1:{}/mcp", handle.port))
        .header("content-type", "application/json")
        .json(&tools_list_body())
        .send()
        .await
        .expect("send request");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    handle.shutdown();
}

#[tokio::test]
async fn incorrect_token_is_rejected_over_real_http() {
    let (handle, _app_handle, _tmp) = spawn_test_server("session-http-3").await;

    let response = Client::new()
        .post(format!("http://127.0.0.1:{}/mcp", handle.port))
        .bearer_auth("not-the-real-token")
        .header("content-type", "application/json")
        .json(&tools_list_body())
        .send()
        .await
        .expect("send request");

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    handle.shutdown();
}

#[tokio::test]
async fn two_concurrent_calls_against_one_server_both_succeed() {
    let (handle, _app_handle, _tmp) = spawn_test_server("session-http-4").await;
    let client = Client::new();

    let call = |id: u64| {
        let client = client.clone();
        let port = handle.port;
        let token = handle.token.clone();
        async move {
            client
                .post(format!("http://127.0.0.1:{port}/mcp"))
                .bearer_auth(&token)
                .header("content-type", "application/json")
                .json(&serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/list", "params": {}}))
                .send()
                .await
                .expect("send request")
        }
    };

    let (first, second) = tokio::join!(call(1), call(2));
    assert_eq!(first.status(), reqwest::StatusCode::OK);
    assert_eq!(second.status(), reqwest::StatusCode::OK);
    handle.shutdown();
}

#[tokio::test]
async fn shutdown_releases_the_port() {
    let (handle, _app_handle, _tmp) = spawn_test_server("session-http-5").await;
    let port = handle.port;
    handle.shutdown();

    // Polling a real OS resource (the port actually being released), not a
    // fixed sleep standing in for synchronization that could be awaited
    // directly -- `axum::serve`'s graceful shutdown has no separate "done"
    // future this test can await instead.
    let mut rebound = false;
    for _ in 0..50 {
        if tokio::net::TcpListener::bind(("127.0.0.1", port)).await.is_ok() {
            rebound = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(rebound, "port {port} should be free after shutdown");
}

#[tokio::test]
async fn end_session_via_the_registry_shuts_down_only_that_sessions_server() {
    let (handle_a, app_handle, _tmp) = spawn_test_server("session-http-6").await;
    let handle_b = spawn_mcp_http_server(app_handle.clone(), "session-http-7".to_string())
        .await
        .expect("spawn second mcp http server");

    app_handle
        .state::<Arc<McpServerRegistry>>()
        .end_session("session-http-6");

    let mut port_a_free = false;
    for _ in 0..50 {
        if tokio::net::TcpListener::bind(("127.0.0.1", handle_a.port))
            .await
            .is_ok()
        {
            port_a_free = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(port_a_free, "session-http-6's port should be free");

    let still_up = Client::new()
        .post(format!("http://127.0.0.1:{}/mcp", handle_b.port))
        .bearer_auth(&handle_b.token)
        .header("content-type", "application/json")
        .json(&tools_list_body())
        .send()
        .await
        .expect("send request to session-http-7");
    assert_eq!(still_up.status(), reqwest::StatusCode::OK);

    handle_b.shutdown();
}
```

- [ ] **Step 7: Run the integration test**

Run: `cargo test -p rocket --test mcp_http_server_integration -j4`
Expected: all 6 tests PASS.

- [ ] **Step 8: `cargo check --workspace` and `cargo test --workspace` for the crates this task touched**

Run: `cargo check --workspace -j4`
Run: `cargo test -p rocket -j4`
Expected: both clean/green.

- [ ] **Step 9: Commit**

Use `dev-workflow-skills:1-git-commit` for `src-tauri/src/mcp/tool_server.rs`, `src-tauri/src/mcp/registry.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/commands/acp_sessions.rs`, `src-tauri/tests/mcp_http_server_integration.rs`.

---

## Next Plan

[Plan 05 — Stdio shim + full wiring](2026-09-28-acp-mcp-tool-server-plan-05-stdio-shim-and-wiring.md) (file created by whoever writes that plan). It must resolve:

- Exactly which identifier is passed as this plan's `spawn_mcp_http_server`'s `session_id` parameter, given the real ACP-protocol session id is not known until after the agent handshake completes, but the server's port/token must be included in that same handshake's `NewSessionRequest`. Either a Rocket-minted pre-handshake correlation id is threaded through and used consistently everywhere `session_id` appears (`McpToolService` calls, `DomainEvent::AcpToolInvoked`, `McpServerRegistry` keys), or the call order changes — this plan deliberately left that decision to Plan 05. **Resolved in Plan 05, Task 3, Step 6: `start_agent_session` mints a Rocket-side UUID before spawning the HTTP server, uses it as `spawn_mcp_http_server`'s `session_id` (so it is what `McpToolService`/`DomainEvent::AcpToolInvoked` see for every tool call on that server), and separately registers the resulting `McpHttpServerHandle` in `McpServerRegistry` keyed by the real post-handshake ACP session id once `start_session` returns (since that is the id `end_agent_session`/`send_agent_prompt` address a session by). The two ids intentionally differ; see Plan 05 for the rationale.**
- Constructing `Arc<McpToolService>` in `src-tauri/src/lib.rs` and `app.manage`-ing it (this plan's `mcp_tool_service()` helper already expects it to be there).
- The `--acp-mcp-stdio-bridge` hidden startup mode: an `rmcp` stdio-transport server whose tool handlers forward 1:1 to an `rmcp` HTTP-transport client against `http://127.0.0.1:<port>` with the token from `ROCKET_MCP_PORT`/`ROCKET_MCP_TOKEN` env vars (never argv).
- `AcpAgentClient::start_session` reading `InitializeResponse.agent_capabilities.mcp_capabilities.http` to choose `McpServerSpec::Http` vs. `::Stdio`.
- `TauriEventBus`'s new `DomainEvent::AcpToolInvoked` match arm.

## Post-Implementation Review

- [ ] Dispatch an Opus-model subagent (`model: "opus"`) to review this plan's full diff (all 4 tasks) against:
  - **Interface conformance vs. the plan index.** Does `McpHttpServerHandle`'s actual shape and `spawn_mcp_http_server`'s actual signature match what Plan 05 will need? Flag the `session_id`-parameter deviation from the index explicitly and confirm the rationale recorded in Task 4 still holds once the real `rmcp` API is in.
  - **`rmcp` API correctness.** Since this plan was written without a vendored copy of `rmcp`, re-verify every name in `tool_server.rs`'s and `auth.rs`'s `use` blocks against the actual installed crate (by this point, `Cargo.lock` has a real pinned version) — this is the single highest-risk area of the whole plan.
  - **Token handling security**, given this is the credential-bearing surface of the whole subproject: confirm the token never appears in a `tracing`/`log` call anywhere in the new code (including at `debug`/`trace` level), confirm `McpHttpServerHandle::token` is never `Debug`-derived in a way that would print it via a stray `{:?}` in a log line, confirm the constant-time comparison in `auth.rs` is actually reached on every request to `/mcp` (not bypassable via a different HTTP method or a sub-path `nest_service` might expose unguarded), and confirm `McpServerRegistry` never leaks a handle to two different sessions.
  - **DDD boundaries.** Confirm no `rocket-infra` or `rocket-app` file was touched by this plan (it shouldn't have been — everything lives in `src-tauri`), and confirm `McpServerRegistry`'s separation from `AcpAgentClient`'s own registry (Task 4's rationale) still reads as correct once the code exists, not just as a plan-time argument.
  - **Code quality and duplication.** The `TestFixture` in `tool_server.rs` and `spawn_test_server` in the integration test file duplicate a similar `McpToolService`-building sequence by necessity (different compilation units). Confirm this duplication stayed minimal and didn't drift into two subtly different fixtures that would mask a real bug in one test file but not the other.
  - Authority to fix any of the above directly, matching the process already used for subprojects A and B.
