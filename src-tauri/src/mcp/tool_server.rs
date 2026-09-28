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
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
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
            CallToolResult::success(vec![ContentBlock::text(text)])
        }
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
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
        other => Err(CallToolResult::error(vec![ContentBlock::text(format!(
            "unknown script phase '{other}': expected pre_request, post_response, or tests"
        ))])),
    }
}

#[tool_router]
impl<R: tauri::Runtime> RocketMcpToolServer<R> {
    #[tool(description = "List the requests in a Rocket collection (path, name, method, url).")]
    async fn list_collection_requests(
        &self,
        Parameters(params): Parameters<ListCollectionRequestsParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.list_collection_requests(&self.session_id, &params.collection);
        Ok(to_tool_result(result))
    }

    #[tool(
        description = "Execute a saved request and return its status, duration, and test pass/fail counts."
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

    use rocket_collection::request::Request as CollectionRequest;
    use rocket_collection::{settings::CollectionSettings, CollectionRepository};
    use rocket_environment::{
        Environment, EnvironmentRepositoryFactory, NullSecretStore, NullVaultSecretFetcher,
        Variable,
    };
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
}
