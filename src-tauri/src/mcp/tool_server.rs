//! The MCP tool server exposed to ACP agents: one `rmcp` `ServerHandler`
//! wired to `Arc<McpToolService>` (`rocket-app`, Plan 03) via `AppHandle`
//! managed state, hosted over `rmcp`'s Streamable HTTP transport.
//!
//! Every `rmcp` name below was verified against the installed `rmcp 3.5.0` /
//! `rmcp-macros 3.5.0` source in Task 1, Step 1 (no vendored copy existed
//! before this task added the dependency, so the plan's original names were
//! reconstructed from public docs and several turned out wrong — see the
//! Task 1 report for the full diff). If a future `rmcp` upgrade renames any
//! of these, this `use` block — and the mirroring one in this module's own
//! tests — are the only places that should need to change.
//!
//! Corrections made here vs. the plan's original guess:
//! - `Parameters<T>` lives at `rmcp::handler::server::wrapper::Parameters`,
//!   not `...::tool::Parameters` (`tool.rs` only *uses* it internally, it
//!   does not re-export it).
//! - There is no `rmcp::model::Content` type. The content-block union is
//!   `rmcp::model::ContentBlock`, with `ContentBlock::text(..)` /
//!   `.as_text()` in place of the plan's guessed `Content::text` / `.as_text`.
//! - `ServerHandler::get_info`'s return type is `rmcp::model::ServerConfig`
//!   (`InitializeResult` under the hood). `ServerInfo` still exists as a type
//!   alias for the same type but is `#[deprecated]` in 3.5.0 ("the name
//!   collides with the protocol's `serverInfo` field"), so `ServerConfig` is
//!   used here to keep `cargo check` warning-free.
//! - `ServerConfig` (`InitializeResult`) and `Implementation` are both
//!   `#[non_exhaustive]` in 3.5.0, so neither can be built with a struct
//!   literal from this crate. `ServerConfig::new(capabilities)` +
//!   `.with_server_info(..)` + `.with_instructions(..)` and
//!   `Implementation::new(name, version)` replace the plan's struct-literal
//!   sketch.
//!
//! ## `AppHandle` / `MockRuntime` generic parameter
//!
//! `tauri::AppHandle` is not a concrete type — it is `AppHandle<R: Runtime>`,
//! and bare `tauri::AppHandle` in production code resolves to `AppHandle<Wry>`
//! via tauri's `#[default_runtime(crate::Wry, wry)]` macro. `Wry` drives a
//! real webview/window system, which this (headless) test environment cannot
//! construct. Tauri's own answer for headless unit tests is
//! `tauri::test::mock_builder()` / `mock_context()` / `noop_assets()`, which
//! build an `App`/`AppHandle` over `tauri::test::MockRuntime` instead of
//! `Wry` — a different, non-interchangeable type parameter, not a subtype or
//! a drop-in stand-in.
//!
//! The plan's original sketch typed `RocketMcpToolServer` and
//! `mcp_tool_service` with bare `tauri::AppHandle` (i.e. hard-coded to
//! `AppHandle<Wry>`), which cannot compile against an `AppHandle<MockRuntime>`
//! test double. Rather than special-case this one test, `RocketMcpToolServer`
//! and `mcp_tool_service` are both generic over `R: tauri::Runtime`, with
//! `tauri::Wry` as the default type parameter
//! (`RocketMcpToolServer<R: tauri::Runtime = tauri::Wry>`). Production call
//! sites that pass a plain `tauri::AppHandle` (i.e. `AppHandle<Wry>`) are
//! unaffected by the default; tests instantiate
//! `RocketMcpToolServer<tauri::test::MockRuntime>` explicitly. This was
//! verified to work cleanly with `#[tool_router]` / `#[tool_handler]`
//! (`rmcp-macros` 3.5.0): both macros parse and re-emit whatever generics are
//! already on the `impl` block (see `rmcp-macros-3.5.0/src/tool_router.rs`'s
//! `item_impl.generics` / `split_for_impl()` handling), so
//! `impl<R: tauri::Runtime> RocketMcpToolServer<R>` and
//! `impl<R: tauri::Runtime> ServerHandler for RocketMcpToolServer<R>` expand
//! the same way a concrete `impl RocketMcpToolServer` would have.
//! `ServerHandler: Sized + Send + Sync + 'static` is satisfied for every `R:
//! tauri::Runtime` because `tauri::Runtime: runtime::Runtime<EventLoopMessage>`
//! already requires `Handle: RuntimeHandle<T>: Send + Sync + 'static` and
//! `Self: Debug + Sized + 'static`, so `AppHandle<R>`'s fields (`R::Handle`,
//! `Arc<AppManager<R>>`, `Arc<Mutex<EventLoop>>`) are `Send + Sync + 'static`
//! for any valid `R`, not just `Wry`. This propagates to every later task
//! that constructs a `RocketMcpToolServer` or calls `mcp_tool_service` in a
//! test (Task 2's tool-router tests, Task 4's integration test) — see the
//! plan document's Task 1 correction note for the exact signatures.
use rmcp::handler::server::tool::ToolRouter;
// `Parameters<T>` and the tool-result types are not referenced yet — the
// `#[tool_router]` block below is still empty. They're imported now so Task
// 2 only has to add `#[tool]` methods, not a second edit to this `use`
// block.
#[allow(unused_imports)]
use rmcp::handler::server::wrapper::Parameters;
#[allow(unused_imports)]
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use std::sync::Arc;
use tauri::Manager;

use rocket_app::McpToolService;

/// One `RocketMcpToolServer` instance backs exactly one ACP session's MCP
/// HTTP endpoint. `session_id` is fixed at construction (see
/// `spawn_mcp_http_server`'s doc comment for why it cannot be a per-call
/// parameter) and threaded into every `McpToolService` call for that
/// session's audit trail.
///
/// Generic over `R: tauri::Runtime`, defaulted to `tauri::Wry` (the
/// production runtime bare `tauri::AppHandle` resolves to via tauri's
/// `#[default_runtime(crate::Wry, wry)]`). See the `AppHandle`/`MockRuntime`
/// note below this doc comment for why this is generic at all rather than a
/// concrete `tauri::AppHandle` field.
///
/// `Clone` is implemented by hand, not derived: `#[derive(Clone)]` would add
/// a blanket `R: Clone` bound on the whole impl, but neither `tauri::Wry`
/// nor `tauri::test::MockRuntime` implement `Clone` — that bound is both
/// unsatisfiable for every real `R` this type is ever instantiated with and
/// unnecessary in the first place, since neither field actually requires it:
/// `tauri::AppHandle<R>`'s own `Clone` impl does not bound `R: Clone`
/// (`tauri-2.11.5/src/app.rs:476`), and `rmcp`'s `ToolRouter<S>::clone`
/// likewise does not bound `S: Clone` (`rmcp-3.5.0/src/handler/server/
/// router/tool.rs:361`). Task 4's `spawn_mcp_http_server` clones a
/// `RocketMcpToolServer` per HTTP connection (`StreamableHttpService::new`'s
/// `move || Ok(tool_server.clone())` factory), so this needs to work for a
/// concrete `RocketMcpToolServer<Wry>`, not just compile generically.
pub struct RocketMcpToolServer<R: tauri::Runtime = tauri::Wry> {
    // Read by Task 2's `#[tool]` methods (via `mcp_tool_service` and the
    // audit trail), not by anything in this skeleton yet.
    #[allow(dead_code)]
    app_handle: tauri::AppHandle<R>,
    #[allow(dead_code)]
    session_id: String,
    tool_router: ToolRouter<Self>,
}

impl<R: tauri::Runtime> RocketMcpToolServer<R> {
    pub fn new(app_handle: tauri::AppHandle<R>, session_id: String) -> Self {
        Self {
            app_handle,
            session_id,
            tool_router: Self::tool_router(),
        }
    }
}

impl<R: tauri::Runtime> Clone for RocketMcpToolServer<R> {
    fn clone(&self) -> Self {
        Self {
            app_handle: self.app_handle.clone(),
            session_id: self.session_id.clone(),
            tool_router: self.tool_router.clone(),
        }
    }
}

/// Looks up the `Arc<McpToolService>` this Tauri app manages. A missing
/// registration is a wiring bug (Plan 05 must `app.manage(Arc::new(...))` it
/// before any session can start), not a business-rule refusal, so it is
/// surfaced as a protocol-level error rather than a tool-result error.
///
/// Unused until Task 2 adds `#[tool]` methods that call it.
#[allow(dead_code)]
fn mcp_tool_service<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Arc<McpToolService>, McpError> {
    match app_handle.try_state::<Arc<McpToolService>>() {
        Some(state) => Ok(Arc::clone(state.inner())),
        None => Err(McpError::internal_error(
            "McpToolService is not managed on this AppHandle",
            None,
        )),
    }
}

// `allow_empty`: this impl block has no `#[tool]` methods yet (Task 2 adds
// the 6 tools here). `rmcp-macros` 3.5.0's `#[tool_router]` hard-errors on
// an empty block without this ("found no `#[tool]` fn in this impl block, so
// `Self::tool_router()` would serve no tools") — verified against installed
// source (`rmcp-macros-3.5.0/src/tool_router.rs`). Safe for Task 2 to remove
// once it adds the first `#[tool]` method, though leaving it is harmless.
#[tool_router(allow_empty)]
impl<R: tauri::Runtime> RocketMcpToolServer<R> {
    // Task 2 adds the 6 `#[tool]` methods here.
}

// `router = self.tool_router.clone()`: without this, `#[tool_handler]`
// defaults to `router = Self::tool_router()`, which rebuilds a fresh
// `ToolRouter` on every call and never reads the `tool_router` field this
// struct stores — leaving that field permanently dead code. Reusing the
// stored instance is the point of keeping it on the struct at all.
#[tool_handler(router = self.tool_router.clone())]
impl<R: tauri::Runtime> ServerHandler for RocketMcpToolServer<R> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "rocket-mcp-tool-server",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Rocket ACP tool server: run requests, edit scripts, and read/write \
                 non-secret environment variables for one active session.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app_handle() -> tauri::AppHandle<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock tauri app")
            .handle()
            .clone()
    }

    #[test]
    fn get_info_reports_the_rocket_tool_server_identity() {
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(test_app_handle(), "session-1".to_string());
        let info = server.get_info();
        assert_eq!(info.server_info.name, "rocket-mcp-tool-server");
    }

    /// Exercises the hand-written `Clone` impl on a concrete
    /// `RocketMcpToolServer<MockRuntime>` — `MockRuntime` itself does not
    /// implement `Clone`, so this only compiles (and only proves the
    /// deliberately-omitted `R: Clone` bound is not silently required
    /// somewhere) because `Clone` is implemented by hand rather than
    /// derived. Mirrors `spawn_mcp_http_server`'s real per-connection
    /// `tool_server.clone()` call (Task 4).
    #[test]
    fn rocket_mcp_tool_server_clones_without_requiring_the_runtime_to_be_clone() {
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(test_app_handle(), "session-1".to_string());
        let cloned = server.clone();
        assert_eq!(
            cloned.get_info().server_info.name,
            server.get_info().server_info.name
        );
    }
}
