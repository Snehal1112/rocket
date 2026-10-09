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
use rocket_acp::proposal::{
    AgentProposal, ProposedChange, ProposedRequest, RequestPatch, ScriptPhase,
};
use rocket_app::AssistantMode;
use rocket_app::McpToolService;
use rocket_app::ProposalService;
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod, QueryParam};

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
    binding: Arc<McpSessionBinding>,
    tool_router: ToolRouter<Self>,
}

impl<R: tauri::Runtime> RocketMcpToolServer<R> {
    /// A server whose calls are tagged with `session_id` for good. Used by
    /// tests; `spawn_mcp_http_server` uses `with_binding`.
    pub fn new(app_handle: tauri::AppHandle<R>, session_id: String) -> Self {
        Self::with_binding(app_handle, Arc::new(McpSessionBinding::new(session_id)))
    }

    /// A server that tags its calls with whatever `binding` holds at call
    /// time.
    pub fn with_binding(app_handle: tauri::AppHandle<R>, binding: Arc<McpSessionBinding>) -> Self {
        Self {
            app_handle,
            binding,
            tool_router: Self::tool_router(),
        }
    }
}

impl<R: tauri::Runtime> Clone for RocketMcpToolServer<R> {
    fn clone(&self) -> Self {
        Self {
            app_handle: self.app_handle.clone(),
            binding: Arc::clone(&self.binding),
            tool_router: self.tool_router.clone(),
        }
    }
}

/// Which session id a tool server tags its `McpToolService` calls with.
///
/// The server must run before the ACP handshake, because its port and
/// token go into `session/new`, so it starts with a Rocket-minted
/// provisional id. Once the handshake returns the real ACP session id, the
/// command layer calls `bind`, and every later call uses the real id. That
/// is the id `set_assistant_mode`, `end_agent_session` and the session
/// cleanup address a session by, so the mode, the test-result cache and the
/// pending outline all live under one key. A call that arrives before
/// `bind` uses the provisional id, which has no mode, so it runs in Ask.
#[derive(Debug)]
pub struct McpSessionBinding {
    provisional_id: String,
    acp_session_id: std::sync::OnceLock<String>,
}

impl McpSessionBinding {
    pub fn new(provisional_id: String) -> Self {
        Self {
            provisional_id,
            acp_session_id: std::sync::OnceLock::new(),
        }
    }

    /// Records the real ACP session id. The first call wins.
    pub fn bind(&self, acp_session_id: &str) {
        let _ = self.acp_session_id.set(acp_session_id.to_string());
    }

    /// The real ACP session id once bound, the provisional id before.
    pub fn session_id(&self) -> &str {
        self.acp_session_id
            .get()
            .map(String::as_str)
            .unwrap_or(self.provisional_id.as_str())
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

/// Looks up the `Arc<ProposalService>` this app manages. A missing
/// registration is a wiring bug, so it is a protocol-level error.
fn proposal_service<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Arc<ProposalService>, McpError> {
    match app_handle.try_state::<Arc<ProposalService>>() {
        Some(state) => Ok(Arc::clone(state.inner())),
        None => Err(McpError::internal_error(
            "ProposalService is not managed on this AppHandle",
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
pub struct GetTestResultsParams {
    pub collection: String,
    pub request_path: String,
}

/// What `propose_changes` tells the agent.
const PROPOSALS_QUEUED: &str = "queued; awaiting user approval";

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ProposeChangesParams {
    /// Each change becomes its own proposal that the user accepts or rejects.
    pub changes: Vec<ProposedChangeParams>,
}

/// One change. Paths are relative to the collection root; "" is the root.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProposedChangeParams {
    /// Create an empty folder.
    CreateFolder {
        collection: String,
        #[serde(default)]
        parent_path: String,
        name: String,
    },
    /// Create an HTTP request. It inherits auth from its folder or collection.
    CreateRequest {
        collection: String,
        #[serde(default)]
        folder_path: String,
        request: ProposedRequestParams,
    },
    /// Change some fields of an HTTP request. Omitted fields stay as they are.
    UpdateRequest {
        collection: String,
        request_path: String,
        patch: RequestPatchParams,
    },
    /// Replace one script: "pre_request", "post_response" or "tests".
    EditScript {
        collection: String,
        request_path: String,
        phase: String,
        body: String,
    },
    /// Move a request or folder into another folder of the same collection.
    MoveItem {
        collection: String,
        from_path: String,
        #[serde(default)]
        to_folder: String,
    },
    /// Rename a request (its display name) or a folder.
    RenameItem {
        collection: String,
        path: String,
        new_name: String,
    },
    /// Set or add a non-secret environment variable.
    SetEnvVar {
        collection: String,
        environment: String,
        key: String,
        value: String,
    },
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KeyValueParams {
    pub key: String,
    pub value: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BodyParams {
    /// One of "none", "json", "xml", "text", "sparql", "formurlencoded".
    pub mode: String,
    #[serde(default)]
    pub content: Option<String>,
}

/// Unknown fields, such as `auth`, are refused rather than dropped.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposedRequestParams {
    pub name: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<KeyValueParams>,
    #[serde(default)]
    pub query_params: Vec<KeyValueParams>,
    #[serde(default)]
    pub body: Option<BodyParams>,
    #[serde(default)]
    pub docs: Option<String>,
    #[serde(default)]
    pub pre_request_script: Option<String>,
    #[serde(default)]
    pub post_response_script: Option<String>,
    #[serde(default)]
    pub tests: Option<String>,
}

/// Unknown fields, such as `auth`, are refused rather than dropped.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestPatchParams {
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Option<Vec<KeyValueParams>>,
    #[serde(default)]
    pub query_params: Option<Vec<KeyValueParams>>,
    #[serde(default)]
    pub body: Option<BodyParams>,
    #[serde(default)]
    pub docs: Option<String>,
}

/// What `propose_changes` returns.
#[derive(Debug, serde::Serialize)]
pub struct ProposeChangesResult {
    pub proposal_ids: Vec<String>,
    pub status: &'static str,
}

/// One proposal as `list_proposals` shows it to the agent. It carries the
/// value-free summary and the status only, never the change itself, so no
/// secret header or variable value can come back through it.
#[derive(Debug, serde::Serialize)]
pub struct ProposalView {
    pub id: String,
    pub summary: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl From<AgentProposal> for ProposalView {
    fn from(proposal: AgentProposal) -> Self {
        Self {
            message: proposal.status.message().map(str::to_string),
            status: proposal.status.as_str(),
            id: proposal.id,
            summary: proposal.summary,
        }
    }
}

/// Parses the wire-format phase. The error is the message text only, so the
/// caller can return it as an agent-visible tool error.
fn parse_script_phase(phase: &str) -> Result<ScriptPhase, String> {
    match phase {
        "pre_request" => Ok(ScriptPhase::PreRequest),
        "post_response" => Ok(ScriptPhase::PostResponse),
        "tests" => Ok(ScriptPhase::Tests),
        other => Err(format!(
            "unknown script phase '{other}': expected pre_request, post_response or tests"
        )),
    }
}

fn parse_method(method: &str) -> Result<HttpMethod, String> {
    method
        .parse::<HttpMethod>()
        .map_err(|_| format!("'{method}' is not a valid HTTP method"))
}

/// Only text-like bodies can be proposed. Form data and files need the user.
fn to_body(body: BodyParams) -> Result<Body, String> {
    let mode: BodyMode = serde_json::from_value(serde_json::Value::String(body.mode.clone()))
        .map_err(|_| format!("unknown body mode '{}'", body.mode))?;
    if matches!(mode, BodyMode::FormData | BodyMode::Binary | BodyMode::GraphQl) {
        return Err(format!(
            "body mode '{}' cannot be proposed; use none, json, xml, text, sparql or formurlencoded",
            body.mode
        ));
    }
    Ok(Body {
        mode,
        content: body.content,
        form_data: None,
        file_path: None,
    })
}

fn to_headers(pairs: Vec<KeyValueParams>) -> Vec<Header> {
    pairs
        .into_iter()
        .map(|pair| Header {
            key: pair.key,
            value: pair.value,
            enabled: pair.enabled,
            description: None,
        })
        .collect()
}

fn to_query_params(pairs: Vec<KeyValueParams>) -> Vec<QueryParam> {
    pairs
        .into_iter()
        .map(|pair| QueryParam {
            key: pair.key,
            value: pair.value,
            enabled: pair.enabled,
            description: None,
        })
        .collect()
}

fn to_domain_request(request: ProposedRequestParams) -> Result<ProposedRequest, String> {
    Ok(ProposedRequest {
        method: parse_method(&request.method)?,
        body: request.body.map(to_body).transpose()?,
        name: request.name,
        url: request.url,
        headers: to_headers(request.headers),
        query_params: to_query_params(request.query_params),
        docs: request.docs,
        pre_request_script: request.pre_request_script,
        post_response_script: request.post_response_script,
        tests: request.tests,
    })
}

fn to_domain_patch(patch: RequestPatchParams) -> Result<RequestPatch, String> {
    Ok(RequestPatch {
        method: patch.method.as_deref().map(parse_method).transpose()?,
        body: patch.body.map(to_body).transpose()?,
        url: patch.url,
        headers: patch.headers.map(to_headers),
        query_params: patch.query_params.map(to_query_params),
        docs: patch.docs,
    })
}

/// Converts one tool-input change to the domain type. `ProposalService`
/// fills in the base fingerprint, so the agent never supplies one.
fn to_domain_change(params: ProposedChangeParams) -> Result<ProposedChange, String> {
    Ok(match params {
        ProposedChangeParams::CreateFolder {
            collection,
            parent_path,
            name,
        } => ProposedChange::CreateFolder {
            collection,
            parent_path,
            name,
        },
        ProposedChangeParams::CreateRequest {
            collection,
            folder_path,
            request,
        } => ProposedChange::CreateRequest {
            collection,
            folder_path,
            request: to_domain_request(request)?,
        },
        ProposedChangeParams::UpdateRequest {
            collection,
            request_path,
            patch,
        } => ProposedChange::UpdateRequest {
            collection,
            request_path,
            patch: to_domain_patch(patch)?,
            base_fingerprint: String::new(),
        },
        ProposedChangeParams::EditScript {
            collection,
            request_path,
            phase,
            body,
        } => ProposedChange::EditScript {
            collection,
            request_path,
            phase: parse_script_phase(&phase)?,
            body,
            base_fingerprint: String::new(),
        },
        ProposedChangeParams::MoveItem {
            collection,
            from_path,
            to_folder,
        } => ProposedChange::MoveItem {
            collection,
            from_path,
            to_folder,
            base_fingerprint: String::new(),
        },
        ProposedChangeParams::RenameItem {
            collection,
            path,
            new_name,
        } => ProposedChange::RenameItem {
            collection,
            path,
            new_name,
            base_fingerprint: String::new(),
        },
        ProposedChangeParams::SetEnvVar {
            collection,
            environment,
            key,
            value,
        } => ProposedChange::SetEnvVar {
            collection,
            environment,
            key,
            value,
        },
    })
}

/// Maps a service call's outcome to an MCP tool result. `Ok` becomes a
/// success result carrying the value as JSON text; `Err` becomes an
/// *agent-visible* tool error (`CallToolResult::error`, `is_error: true`),
/// never a protocol-level failure — per the spec, a refusal (autonomy
/// disabled, not in the mode, not found) must reach the agent as something
/// it can explain to the user, not a generic transport failure. All errors
/// go through this one `e.to_string()` call, so nothing here can format one
/// refusal differently from another.
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
            self.binding.session_id(),
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
        Ok(to_tool_result(svc.list_collections(self.binding.session_id())))
    }

    #[tool(
        description = "Read one request's full definition (URL, headers, params, body, auth, scripts). Literal credentials are masked; {{variable}} references are kept."
    )]
    async fn get_request(
        &self,
        Parameters(params): Parameters<GetRequestParams>,
    ) -> Result<CallToolResult, McpError> {
        let svc = mcp_tool_service(&self.app_handle)?;
        let result = svc.get_request(self.binding.session_id(), &params.collection, &params.request_path);
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
        let result = svc.get_collection_settings(self.binding.session_id(), &params.collection);
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
            self.binding.session_id(),
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
            self.binding.session_id(),
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
                self.binding.session_id(),
                &params.collection,
                &params.request_path,
                params.environment_name.as_deref(),
            )
            .await;
        Ok(to_tool_result(result))
    }

    #[tool(
        description = "Propose workspace changes for the user to review: create_folder, create_request, update_request, edit_script, move_item, rename_item or set_env_var (non-secret only). Nothing is written until the user accepts each proposal. Returns the new proposal ids. Not available in Ask mode."
    )]
    async fn propose_changes(
        &self,
        Parameters(params): Parameters<ProposeChangesParams>,
    ) -> Result<CallToolResult, McpError> {
        let tools = mcp_tool_service(&self.app_handle)?;
        if let Err(refusal) = tools.check_mode(self.binding.session_id(), AssistantMode::Edit) {
            return Ok(to_tool_result::<()>(Err(refusal)));
        }
        let mut changes = Vec::with_capacity(params.changes.len());
        for change in params.changes {
            match to_domain_change(change) {
                Ok(change) => changes.push(change),
                Err(message) => {
                    return Ok(CallToolResult::error(vec![ContentBlock::text(message)]));
                }
            }
        }
        let proposals = proposal_service(&self.app_handle)?;
        let result = proposals
            .propose(self.binding.session_id(), changes)
            .map(|proposal_ids| ProposeChangesResult {
                proposal_ids,
                status: PROPOSALS_QUEUED,
            });
        Ok(to_tool_result(result))
    }

    #[tool(
        description = "List this session's proposals and their status: pending, accepted, rejected, stale (the item changed after it was proposed; read it again and propose again) or failed (with a message)."
    )]
    async fn list_proposals(&self) -> Result<CallToolResult, McpError> {
        let proposals = proposal_service(&self.app_handle)?;
        let views: Vec<ProposalView> = proposals
            .list(self.binding.session_id())
            .into_iter()
            .map(ProposalView::from)
            .collect();
        Ok(to_tool_result(Ok(views)))
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
            svc.get_test_results(self.binding.session_id(), &params.collection, &params.request_path);
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
                "Rocket workspace assistant tools: read the workspace, propose changes \
                 for the user to accept, and run requests where the user allows it.",
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
    /// Shared with the server's `RocketMcpToolServer`. The caller binds it
    /// to the real ACP session id after the handshake.
    pub binding: Arc<McpSessionBinding>,
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

    let binding = Arc::new(McpSessionBinding::new(session_id));
    let tool_server = RocketMcpToolServer::with_binding(app_handle, Arc::clone(&binding));
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
        binding,
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
        mcp_tool_svc: Arc<McpToolService>,
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
            app.manage(Arc::clone(&mcp_tool_svc));

            Self {
                app_handle: app.handle().clone(),
                _tmp: tmp,
                session_id: "session-1".to_string(),
                mcp_tool_svc,
            }
        }

        fn server(&self) -> RocketMcpToolServer<tauri::test::MockRuntime> {
            RocketMcpToolServer::new(self.app_handle.clone(), self.session_id.clone())
        }

        /// A server whose session runs in `mode`.
        fn server_in(&self, mode: AssistantMode) -> RocketMcpToolServer<tauri::test::MockRuntime> {
            self.mcp_tool_svc.open_session(&self.session_id, mode);
            self.server()
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
        let server = fixture.server_in(AssistantMode::Agent);
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
        let server = fixture.server_in(AssistantMode::Agent);

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

    #[test]
    fn the_tool_list_is_the_same_in_every_mode() {
        let fixture = TestFixture::new(true);
        let expected = vec![
            "get_collection_settings",
            "get_environment",
            "get_history",
            "get_request",
            "get_test_results",
            "get_workspace_outline",
            "list_collections",
            "list_proposals",
            "propose_changes",
            "run_request",
        ];
        for mode in [AssistantMode::Ask, AssistantMode::Edit, AssistantMode::Agent] {
            let names: Vec<String> = fixture
                .server_in(mode)
                .tool_router
                .list_all()
                .into_iter()
                .map(|tool| tool.name.to_string())
                .collect();
            assert_eq!(names, expected, "tool list in {mode:?} mode");
        }
    }

    #[tokio::test]
    async fn run_request_in_ask_mode_is_an_agent_visible_refusal() {
        let fixture = TestFixture::new(true);
        let result = fixture
            .server_in(AssistantMode::Ask)
            .run_request(Parameters(RunRequestParams {
                collection: "demo".to_string(),
                request_path: "ping.yml".to_string(),
                environment_name: None,
            }))
            .await
            .expect("tool call");

        assert!(tool_is_error(&result));
        assert!(tool_text(&result).contains("Not available in Ask mode"));
    }

    #[test]
    fn a_binding_reports_the_provisional_id_until_bound_and_the_first_bind_wins() {
        let binding = McpSessionBinding::new("provisional".to_string());
        assert_eq!(binding.session_id(), "provisional");
        binding.bind("acp-1");
        assert_eq!(binding.session_id(), "acp-1");
        binding.bind("acp-2");
        assert_eq!(binding.session_id(), "acp-1");
    }

    #[tokio::test]
    async fn calls_before_bind_run_in_ask_mode_and_after_bind_use_the_real_session() {
        let fixture = TestFixture::new(true);
        let binding = Arc::new(McpSessionBinding::new("provisional-1".to_string()));
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::with_binding(fixture.app_handle.clone(), Arc::clone(&binding));
        fixture
            .mcp_tool_svc
            .open_session("acp-real-1", AssistantMode::Agent);
        let run_params = || {
            Parameters(RunRequestParams {
                collection: "demo".to_string(),
                request_path: "ping.yml".to_string(),
                environment_name: None,
            })
        };

        let before = server.run_request(run_params()).await.expect("tool call");
        assert!(tool_text(&before).contains("Not available in Ask mode"));

        binding.bind("acp-real-1");
        let after = server.run_request(run_params()).await.expect("tool call");
        assert!(!tool_is_error(&after), "{}", tool_text(&after));

        // The cached results live under the real id, so forgetting the real
        // id (what the session cleanup does) clears them.
        fixture.mcp_tool_svc.forget_session("acp-real-1");
        let results = server
            .get_test_results(Parameters(GetTestResultsParams {
                collection: "demo".to_string(),
                request_path: "ping.yml".to_string(),
            }))
            .await
            .expect("tool call");
        assert!(tool_is_error(&results));
    }

    #[test]
    fn the_tool_list_has_the_propose_tools_and_no_direct_write_tools() {
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(test_app_handle(), "session-1".to_string());
        assert!(server.tool_router.has_route("propose_changes"));
        assert!(server.tool_router.has_route("list_proposals"));
        assert!(!server.tool_router.has_route("edit_script"));
        assert!(!server.tool_router.has_route("set_env_var"));
    }

    fn parse_one(change: serde_json::Value) -> ProposedChangeParams {
        let params: ProposeChangesParams =
            serde_json::from_value(serde_json::json!({ "changes": [change] })).expect("parse");
        params.changes.into_iter().next().expect("one change")
    }

    #[test]
    fn a_patch_or_new_request_with_an_auth_field_is_refused() {
        let patch = serde_json::from_value::<ProposeChangesParams>(serde_json::json!({
            "changes": [{
                "op": "update_request", "collection": "demo", "request_path": "ping.yml",
                "patch": { "url": "https://x", "auth": { "authType": "bearer", "token": "t" } }
            }]
        }));
        assert!(patch.is_err(), "auth must not be dropped silently");
        let create = serde_json::from_value::<ProposeChangesParams>(serde_json::json!({
            "changes": [{
                "op": "create_request", "collection": "demo",
                "request": { "name": "A", "method": "GET", "url": "https://x",
                             "auth": { "authType": "none" } }
            }]
        }));
        assert!(create.is_err(), "auth must not be dropped silently");
    }

    #[test]
    fn create_request_params_convert_to_the_domain_change() {
        let change = to_domain_change(parse_one(serde_json::json!({
            "op": "create_request", "collection": "demo",
            "request": {
                "name": "List Users", "method": "get", "url": "https://x/users",
                "headers": [{ "key": "Accept", "value": "application/json" }],
                "body": { "mode": "json", "content": "{}" }
            }
        })))
        .expect("convert");
        match change {
            ProposedChange::CreateRequest {
                collection,
                folder_path,
                request,
            } => {
                assert_eq!(collection, "demo");
                assert_eq!(folder_path, "");
                assert_eq!(request.method, HttpMethod::Get);
                assert!(request.headers[0].enabled);
                assert_eq!(request.body.map(|b| b.mode), Some(BodyMode::Json));
            }
            other => panic!("expected CreateRequest, got {other:?}"),
        }
    }

    #[test]
    fn a_bad_phase_method_or_body_mode_is_refused_with_a_message() {
        let refuse = |change: serde_json::Value| {
            to_domain_change(parse_one(change)).expect_err("must be refused")
        };
        assert!(refuse(serde_json::json!({
            "op": "edit_script", "collection": "demo", "request_path": "a.yml",
            "phase": "before", "body": ""
        }))
        .contains("script phase"));
        assert!(refuse(serde_json::json!({
            "op": "update_request", "collection": "demo", "request_path": "a.yml",
            "patch": { "method": "GET POST" }
        }))
        .contains("HTTP method"));
        assert!(refuse(serde_json::json!({
            "op": "update_request", "collection": "demo", "request_path": "a.yml",
            "patch": { "body": { "mode": "binary" } }
        }))
        .contains("body mode"));
    }

    #[tokio::test]
    async fn list_proposals_returns_the_sessions_proposals_with_their_status() {
        use rocket_collection::CollectionRepository;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let collections_dir = tmp.path().join("collections");
        rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone())
            .create("demo")
            .expect("create collection");
        let ws_path = Arc::new(std::sync::Mutex::new(tmp.path().to_path_buf()));
        let proposals = Arc::new(ProposalService::new(
            rocket_app::CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(collections_dir)),
                Box::new(rocket_shared::events::NullEventPublisher),
            ),
            Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(ws_path)),
            Arc::new(rocket_shared::events::NullEventPublisher),
        ));
        let ids = proposals
            .propose(
                "session-1",
                vec![ProposedChange::CreateFolder {
                    collection: "demo".into(),
                    parent_path: String::new(),
                    name: "reports".into(),
                }],
            )
            .expect("propose");

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock tauri app");
        app.manage(proposals);
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(app.handle().clone(), "session-1".to_string());

        let result = server.list_proposals().await.expect("tool call");
        assert!(!result.is_error.unwrap_or(false));
        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default();
        assert!(text.contains(&ids[0]));
        assert!(text.contains("pending"));
    }

    #[tokio::test]
    async fn list_proposals_uses_the_bound_session_id() {
        use rocket_collection::CollectionRepository;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let collections_dir = tmp.path().join("collections");
        rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone())
            .create("demo")
            .expect("create collection");
        let ws_path = Arc::new(std::sync::Mutex::new(tmp.path().to_path_buf()));
        let proposals = Arc::new(ProposalService::new(
            rocket_app::CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(collections_dir)),
                Box::new(rocket_shared::events::NullEventPublisher),
            ),
            Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(ws_path)),
            Arc::new(rocket_shared::events::NullEventPublisher),
        ));
        let ids = proposals
            .propose(
                "acp-1",
                vec![ProposedChange::CreateFolder {
                    collection: "demo".into(),
                    parent_path: String::new(),
                    name: "reports".into(),
                }],
            )
            .expect("propose");

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock tauri app");
        app.manage(proposals);
        // Plan 03's binding: provisional until the handshake returns the
        // real ACP session id.
        let binding = Arc::new(McpSessionBinding::new("provisional-1".to_string()));
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::with_binding(app.handle().clone(), Arc::clone(&binding));
        let text_of = |result: &CallToolResult| {
            result
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.clone())
                .unwrap_or_default()
        };

        let before = server.list_proposals().await.expect("tool call");
        assert!(!text_of(&before).contains(&ids[0]), "unbound: the provisional id has none");

        binding.bind("acp-1");
        let after = server.list_proposals().await.expect("tool call");
        assert!(text_of(&after).contains(&ids[0]), "bound: the real session's proposals");
    }

    #[tokio::test]
    async fn propose_changes_is_refused_in_ask_mode() {
        let fixture = TestFixture::new(true);
        let params: ProposeChangesParams = serde_json::from_value(serde_json::json!({
            "changes": [{ "op": "create_folder", "collection": "demo", "name": "reports" }]
        }))
        .expect("parse");
        let result = fixture
            .server_in(AssistantMode::Ask)
            .propose_changes(Parameters(params))
            .await
            .expect("tool call");
        assert!(tool_is_error(&result));
        assert!(tool_text(&result).contains("Not available in Ask mode"));
    }

    #[tokio::test]
    async fn list_proposals_never_shows_header_values_or_variable_values() {
        use rocket_collection::CollectionRepository;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let collections_dir = tmp.path().join("collections");
        rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone())
            .create("demo")
            .expect("create collection");
        let ws_path = Arc::new(std::sync::Mutex::new(tmp.path().to_path_buf()));
        let proposals = Arc::new(ProposalService::new(
            rocket_app::CollectionService::new(
                Box::new(rocket_infra::FsCollectionRepo::new_standalone(collections_dir)),
                Box::new(rocket_shared::events::NullEventPublisher),
            ),
            Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(ws_path)),
            Arc::new(rocket_shared::events::NullEventPublisher),
        ));
        let change = to_domain_change(parse_one(serde_json::json!({
            "op": "create_request", "collection": "demo",
            "request": {
                "name": "Login", "method": "POST", "url": "https://x/login",
                "headers": [{ "key": "Authorization", "value": "Bearer sk-hidden-123" }],
                "body": { "mode": "json", "content": "{\"pw\":\"hidden-body-456\"}" },
                "pre_request_script": "const k = 'hidden-script-789';"
            }
        })))
        .expect("convert");
        proposals.propose("session-1", vec![change]).expect("propose");

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock tauri app");
        app.manage(proposals);
        let server: RocketMcpToolServer<tauri::test::MockRuntime> =
            RocketMcpToolServer::new(app.handle().clone(), "session-1".to_string());

        let result = server.list_proposals().await.expect("tool call");
        let text = tool_text(&result);
        assert!(text.contains("Login"));
        for hidden in ["sk-hidden-123", "hidden-body-456", "hidden-script-789"] {
            assert!(!text.contains(hidden), "leaked {hidden}: {text}");
        }
    }
}
