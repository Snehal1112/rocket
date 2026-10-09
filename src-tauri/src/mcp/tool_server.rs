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
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use std::sync::Arc;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

/// The path the MCP Streamable HTTP endpoint is mounted at. Every client
/// (the agent's own HTTP MCP client via `McpServerSpec::Http`, and Plan 05's
/// stdio bridge) must target `http://127.0.0.1:<port>/mcp`, not the bare
/// origin — the router below has no route at `/`.
pub const MCP_HTTP_PATH: &str = "/mcp";

use rocket_app::mcp_read_views::HISTORY_LIMIT_MAX;
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
    app_handle: tauri::AppHandle<R>,
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

use rocket_shared::error::DomainResult;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct WorkspaceOutlineParams {
    /// Only this collection. Leave out for the whole workspace.
    #[serde(default)]
    pub collection: Option<String>,
    /// Only requests under this folder of `collection`, for example "auth" or "auth/v2".
    #[serde(default)]
    pub folder: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CollectionParams {
    pub collection: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetRequestParams {
    pub collection: String,
    pub request_path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetEnvironmentParams {
    pub collection: String,
    pub environment_name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetHistoryParams {
    pub collection: String,
    pub request_path: String,
    /// How many runs to return, newest first. At most 10, the default.
    #[serde(default)]
    pub limit: Option<usize>,
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
/// why `set_env_var`'s "not found" and "is secret" errors stay
/// indistinguishable through this layer: both are plain `DomainError`
/// values, both go through this one `e.to_string()` call, so nothing here
/// can accidentally format one differently from the other.
fn to_tool_result<T: serde::Serialize>(result: DomainResult<T>) -> CallToolResult {
    match result {
        Ok(value) => {
            let text = serde_json::to_string(&value).unwrap_or_else(|e| {
                format!("{{\"error\":\"failed to serialize tool result: {e}\"}}")
            });
            CallToolResult::success(vec![ContentBlock::text(text)])
        }
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    }
}

/// Like `to_tool_result`, for tools whose result is already prose (the
/// outline): the text goes out as it is, not as a quoted JSON string.
fn to_text_tool_result(result: DomainResult<String>) -> CallToolResult {
    match result {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    }
}

/// Parses the wire-format `phase` string into the domain enum. An unknown
/// value is a malformed-input case, so it is handled the same way as any
/// other tool-level refusal: an agent-visible `CallToolResult::error`, not a
/// protocol failure and not a call into `McpToolService` at all.
/// The error is the message text only, so `Result` stays small (clippy's
/// `result_large_err`); the caller wraps it into the tool error.
fn parse_phase(phase: &str) -> Result<rocket_collection::RequestScriptPhase, String> {
    match phase {
        "pre_request" => Ok(rocket_collection::RequestScriptPhase::PreRequest),
        "post_response" => Ok(rocket_collection::RequestScriptPhase::PostResponse),
        "tests" => Ok(rocket_collection::RequestScriptPhase::Tests),
        other => Err(format!(
            "unknown script phase '{other}': expected pre_request, post_response, or tests"
        )),
    }
}

#[tool_router]
impl<R: tauri::Runtime> RocketMcpToolServer<R> {
    #[tool(
        description = "Compact index of the current workspace: each collection with its run permission, and METHOD path for each HTTP request. Capped at 400 requests; above that it lists counts only, and you pass collection (and optionally folder) to list requests."
    )]
    async fn get_workspace_outline(
        &self,
        Parameters(params): Parameters<WorkspaceOutlineParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_workspace_outline(
            &self.session_id,
            params.collection.as_deref(),
            params.folder.as_deref(),
        );
        Ok(to_text_tool_result(result))
    }

    #[tool(
        description = "List the workspace's collections with request counts, environment names and whether running requests is allowed."
    )]
    async fn list_collections(&self) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        Ok(to_tool_result(svc.list_collections(&self.session_id)))
    }

    #[tool(
        description = "Read one request's full definition (URL, headers, params, body, auth, scripts). Literal credentials are masked; {{variable}} references are kept."
    )]
    async fn get_request(
        &self,
        Parameters(params): Parameters<GetRequestParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_request(&self.session_id, &params.collection, &params.request_path);
        Ok(to_tool_result(result))
    }

    #[tool(
        description = "Read a collection's settings: auth type, default headers, variables (secret values masked) and whether running requests is allowed."
    )]
    async fn get_collection_settings(
        &self,
        Parameters(params): Parameters<CollectionParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_collection_settings(&self.session_id, &params.collection);
        Ok(to_tool_result(result))
    }

    #[tool(
        description = "Read one environment of a collection: variable names and non-secret values. Secret variables appear by name only."
    )]
    async fn get_environment(
        &self,
        Parameters(params): Parameters<GetEnvironmentParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_environment(
            &self.session_id,
            &params.collection,
            &params.environment_name,
        );
        Ok(to_tool_result(result))
    }

    #[tool(
        description = "Read the last runs of a request, newest first: time, status, duration and size (at most 10)."
    )]
    async fn get_history(
        &self,
        Parameters(params): Parameters<GetHistoryParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_history(
            &self.session_id,
            &params.collection,
            &params.request_path,
            params.limit.unwrap_or(HISTORY_LIMIT_MAX),
        );
        Ok(to_tool_result(result))
    }

    #[tool(
        description = "Execute a saved request in a collection whose run switch is on, and return its status, duration, test counts and the response body (secrets masked, cut to 8 KB)."
    )]
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

    #[tool(
        description = "Overwrite one script phase (pre_request, post_response, or tests) on a request."
    )]
    async fn edit_script(
        &self,
        Parameters(params): Parameters<EditScriptParams>,
    ) -> Result<CallToolResult, McpError> {
        let phase = match parse_phase(&params.phase) {
            Ok(phase) => phase,
            Err(message) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(message)]));
            }
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

    #[tool(
        description = "Read the cached test results from this session's most recent run of a request."
    )]
    async fn get_test_results(
        &self,
        Parameters(params): Parameters<GetTestResultsParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result =
            svc.get_test_results(&self.session_id, &params.collection, &params.request_path);
        Ok(to_tool_result(result))
    }
}

// `router = self.tool_router`: without this, `#[tool_handler]` defaults to
// `router = Self::tool_router()`, which rebuilds a fresh `ToolRouter` on
// every call and never reads the `tool_router` field this struct stores.
// The macro only calls `&self` methods on the expression (`call`,
// `list_all`, `get`), so the stored router is borrowed, not cloned. This
// matches the form `rmcp`'s own tests use.
#[tool_handler(router = self.tool_router)]
impl<R: tauri::Runtime> ServerHandler for RocketMcpToolServer<R> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "rocket-mcp-tool-server",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Rocket workspace tools for one assistant session: read the current \
                 workspace (outline, collections, requests, settings, environments, history, \
                 test results) with secrets masked, and run requests in collections where the \
                 user allows it.",
            )
    }
}

/// Handle to one running per-session MCP HTTP server. `port`/`token` are
/// what Plan 05's `McpServerSpec::Http` is built from; `shutdown()` stops the
/// server. `Clone` so the same handle can be both kept by the caller and
/// stored in `McpServerRegistry`. All clones share one `CancellationToken`,
/// and cancelling it more than once is harmless.
///
/// Deliberately not `Debug`: `token` is the bearer credential, and a stray
/// `{:?}` on this type in a log line must not be able to print it.
#[derive(Clone)]
pub struct McpHttpServerHandle {
    pub port: u16,
    pub token: String,
    pub(crate) shutdown: CancellationToken,
}

impl McpHttpServerHandle {
    /// Stops the server. The same token drives two things, and both are
    /// needed. First, `axum::serve`'s graceful shutdown stops accepting and
    /// drops the listening socket, which frees the port. Second, `rmcp`'s
    /// `StreamableHttpServerConfig::cancellation_token` terminates every
    /// live MCP session and ends its open SSE streams. Without the second,
    /// an agent's long-lived `GET /mcp` SSE stream keeps its connection
    /// open, and graceful shutdown waits on it forever. That would leave the
    /// serve task, the session worker and its `AppHandle` alive.
    ///
    /// Fire-and-forget: callers on the app-exit path
    /// (`McpServerRegistry::shutdown_all`) have no one left to report a
    /// failure to, and cancelling cannot fail.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}

/// Binds a fresh localhost MCP HTTP server for one ACP session and returns a
/// handle carrying the port and bearer token the caller advertises to the
/// agent. The endpoint is served at [`MCP_HTTP_PATH`].
///
/// This function does **not** register the handle in `McpServerRegistry`.
/// The caller must do that, under the id it will later end the session by.
/// Plan 05 passes a pre-handshake UUID as `session_id` here, but ends
/// sessions by the real post-handshake ACP session id. If this function
/// registered under `session_id` itself, every session would leave a stale
/// second registry entry behind that `end_session` never removes.
///
/// `session_id` is a parameter (not generated here) because tool calls need
/// it up front for their `DomainEvent::AcpToolInvoked` audit events, and
/// because one HTTP server instance serves exactly one ACP session for its
/// whole lifetime.
pub async fn spawn_mcp_http_server<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
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
    let shutdown = CancellationToken::new();

    let tool_server = RocketMcpToolServer::new(app_handle, session_id);
    // The default config keeps `rmcp`'s loopback-only `Host` allowlist, which
    // guards against DNS rebinding. Only the cancellation token is replaced,
    // so `shutdown()` also ends live MCP sessions (see its doc comment).
    let config = StreamableHttpServerConfig::default().with_cancellation_token(shutdown.clone());
    let service = StreamableHttpService::new(
        move || Ok(tool_server.clone()),
        LocalSessionManager::default().into(),
        config,
    );
    let router = axum::Router::new()
        .nest_service(MCP_HTTP_PATH, service)
        .layer(axum::middleware::from_fn_with_state(
            token.clone(),
            crate::mcp::auth::require_bearer_token,
        ));

    let handle = McpHttpServerHandle {
        port,
        token,
        shutdown: shutdown.clone(),
    };

    tauri::async_runtime::spawn(async move {
        // Best-effort: this task runs on its own once spawned, so there is
        // no caller left to report a serve failure to.
        let _ = axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await;
    });

    Ok(handle)
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

    use rocket_collection::request::Request as CollectionRequest;
    use rocket_collection::{settings::CollectionSettings, CollectionRepository};
    use rocket_environment::{
        Environment, EnvironmentRepositoryFactory, NullSecretStore, NullVaultSecretFetcher,
        Variable,
    };
    use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
    use rocket_infra::{
        FsCollectionRepo, FsCookieRepo, FsEnvironmentRepo, FsHistoryRepo, FsSecretManagerRepo,
        FsWorkspaceConfigRepo, SharedCollectionEnvironmentRepo,
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
                ..Default::default()
            })
        }
    }

    /// A "demo" collection with one request ("ping.yml"), one "dev" environment
    /// holding one plain variable ("API_KEY") and one secret variable
    /// ("SECRET_TOKEN"), and `agent_autonomy_enabled` set as requested.
    struct TestFixture {
        _tmp: TempDir,
        // `MockRuntime`, not a bare `tauri::AppHandle` (which would default to
        // `AppHandle<Wry>`, requiring a real webview/window system this
        // headless test cannot construct) — see the "AppHandle / MockRuntime
        // generic parameter" note at the end of Task 1.
        app_handle: tauri::AppHandle<tauri::test::MockRuntime>,
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
                    &CollectionRequest::new(
                        "Ping",
                        HttpMethod::Get,
                        "https://example.invalid/ping",
                    ),
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
                Box::new(FsEnvironmentRepo::new(
                    tmp.path().join("global_environments"),
                )),
                Arc::new(FakeHttpExecutor { status: 200 }),
                Box::new(FsHistoryRepo::new(tmp.path().join("history"))),
                Box::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
                Box::new(FsCookieRepo::new(tmp.path().join("cookies"))),
                Box::new(NullEventPublisher),
                Box::new(FsSecretManagerRepo::new(
                    tmp.path().join("secret_managers.yml"),
                )),
                Arc::new(NullSecretStore),
                Arc::new(NullVaultSecretFetcher),
            )
            .with_collection_env_repo_factory(Box::new(
                SharedCollectionEnvironmentRepo::new(Arc::clone(&ws_path)),
            ));

            let mcp_tool_svc = Arc::new(McpToolService::new(
                Arc::new(FsCollectionRepo::new_standalone(collections_dir)),
                Arc::new(SharedCollectionEnvironmentRepo::new(Arc::clone(&ws_path))),
                Arc::new(exec_svc),
                Arc::new(NullEventPublisher),
                Box::new(FsWorkspaceConfigRepo::new()),
                Arc::clone(&ws_path),
                Box::new(FsHistoryRepo::new(tmp.path().join("history"))),
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

        fn server(&self) -> RocketMcpToolServer<tauri::test::MockRuntime> {
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

    #[tokio::test]
    async fn get_workspace_outline_returns_plain_text_with_the_seeded_request() {
        let fixture = TestFixture::new(true);
        let result = fixture
            .server()
            .get_workspace_outline(Parameters(WorkspaceOutlineParams {
                collection: None,
                folder: None,
            }))
            .await
            .expect("tool call");

        assert!(!tool_is_error(&result));
        let text = tool_text(&result);
        assert!(
            text.starts_with("Workspace outline"),
            "the outline is prose, not a quoted JSON string: {text}"
        );
        assert!(text.contains("GET ping.yml"));
        assert!(text.contains("## demo (run: on, 1 request(s))"));
    }

    #[tokio::test]
    async fn read_tools_work_with_the_run_switch_off() {
        let fixture = TestFixture::new(false);
        let result = fixture
            .server()
            .get_request(Parameters(GetRequestParams {
                collection: "demo".to_string(),
                request_path: "ping.yml".to_string(),
            }))
            .await
            .expect("tool call");

        assert!(!tool_is_error(&result), "{}", tool_text(&result));
        assert!(tool_text(&result).contains("example.invalid/ping"));
    }

    #[tokio::test]
    async fn a_collection_outside_the_workspace_is_an_agent_visible_refusal() {
        let fixture = TestFixture::new(true);
        let result = fixture
            .server()
            .get_collection_settings(Parameters(CollectionParams {
                collection: "../elsewhere".to_string(),
            }))
            .await
            .expect("tool call");

        assert!(tool_is_error(&result));
        assert!(tool_text(&result).contains("not in the current workspace"));
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
    async fn get_environment_shows_the_plain_value_and_hides_the_secret_one() {
        let fixture = TestFixture::new(true);
        let result = fixture
            .server()
            .get_environment(Parameters(GetEnvironmentParams {
                collection: "demo".to_string(),
                environment_name: "dev".to_string(),
            }))
            .await
            .expect("tool call");

        assert!(!tool_is_error(&result), "{}", tool_text(&result));
        let text = tool_text(&result);
        assert!(text.contains("plain-value"));
        assert!(text.contains("SECRET_TOKEN"));
        assert!(!text.contains("secret-value"));
    }

    #[tokio::test]
    async fn get_history_lists_a_run_made_through_run_request() {
        let fixture = TestFixture::new(true);
        let server = fixture.server();
        let run = server
            .run_request(Parameters(RunRequestParams {
                collection: "demo".to_string(),
                request_path: "ping.yml".to_string(),
                environment_name: None,
            }))
            .await
            .expect("tool call");
        assert!(!tool_is_error(&run), "{}", tool_text(&run));

        let result = server
            .get_history(Parameters(GetHistoryParams {
                collection: "demo".to_string(),
                request_path: "ping.yml".to_string(),
                limit: None,
            }))
            .await
            .expect("tool call");
        assert!(!tool_is_error(&result), "{}", tool_text(&result));
        assert!(tool_text(&result).contains("\"status\":200"));
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
}
