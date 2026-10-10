// Hidden `--acp-mcp-stdio-bridge` CLI mode. An ACP agent that does not
// advertise `mcp_capabilities.http` spawns Rocket's own executable with this
// flag (see AcpSessionService::start_session, Task 3) and talks MCP to it
// over its stdin/stdout. This module implements no tool logic of its own —
// every `tools/list`/`tools/call` request received over stdio is forwarded
// verbatim to Plan 04's real HTTP MCP backend and the response is forwarded
// back unchanged. That backend already re-checks the effective agent run switch and
// enforces the `secret` variable boundary on every call, so this bridge has
// no additional authorization logic to duplicate.
//
// Every `rmcp` name below was verified against the vendored `rmcp 3.5.0`
// source (Task 1, Step 1) before writing this file: `ServerHandler::
// call_tool`/`list_tools` take `CallToolRequestParams`/`Option<
// PaginatedRequestParams>` and return `Result<CallToolResponse, ErrorData>`/
// `Result<ListToolsResult, ErrorData>` (`src/handler/server.rs`);
// `RunningService<RoleClient, C>::call_tool_once`/`list_tools` return the
// same `CallToolResponse`/`ListToolsResult` types, just with `ServiceError`
// instead (`src/service/client.rs`); `StreamableHttpClientTransportConfig`
// is `#[non_exhaustive]` with a `with_uri` constructor and an `auth_header`
// builder method taking the bare token (`src/transport/
// streamable_http_client.rs`); and `StreamableHttpClientTransport::
// from_config` is the reqwest-backed constructor gated behind
// `transport-streamable-http-client-reqwest` (`src/transport/common/reqwest/
// streamable_http_client.rs`). No renames were needed.
use std::io;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, ErrorData, ListToolsResult, PaginatedRequestParams,
};
use rmcp::service::{RequestContext, RoleClient, RoleServer, RunningService};
use rmcp::transport::io::stdio;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
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
///
/// Three wire-contract facts Plan 04's Final Review found the hard way, all
/// load-bearing here: the URL must include `MCP_HTTP_PATH` (`"/mcp"` — the
/// bare origin 404s); `auth_header` takes the *bare* token, since `rmcp`
/// itself sends `Bearer <value>` (pre-formatting it here would double the
/// prefix and get 401); and `StreamableHttpClientTransportConfig` is
/// `#[non_exhaustive]`, so it is built via `with_uri(..).auth_header(..)`,
/// never `..Self::with_uri(..)` struct-update syntax (which does not
/// compile from outside `rmcp`'s own crate).
pub async fn run_stdio_bridge() -> io::Result<()> {
    // rmcp's HTTP client is reqwest-backed; this workspace's rustls has no
    // default crypto provider (tauri-plugin-updater pulls in
    // "rustls-no-provider"), so the first real connection below panics with
    // "No provider set" unless one is installed first. Harmless to call more
    // than once within a process — fails silently if already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let port: u16 = std::env::var("ROCKET_MCP_PORT")
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "ROCKET_MCP_PORT is not set"))?
        .parse()
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "ROCKET_MCP_PORT is not a valid port number",
            )
        })?;
    let token = std::env::var("ROCKET_MCP_TOKEN")
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "ROCKET_MCP_TOKEN is not set"))?;

    let config = StreamableHttpClientTransportConfig::with_uri(format!(
        "http://127.0.0.1:{port}{}",
        crate::mcp::tool_server::MCP_HTTP_PATH
    ))
    .auth_header(token);
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
    use tokio::sync::Mutex;

    use super::*;

    // `ROCKET_MCP_PORT`/`ROCKET_MCP_TOKEN` are process-wide state, and Rust's
    // test harness runs `#[tokio::test]` functions from this module
    // concurrently by default. Without serializing them, one test's
    // `set_var`/`remove_var` races another's, and a test can observe a mix of
    // env vars it never set itself (e.g. this file's own first version
    // intermittently reached the network stage and failed with
    // `ConnectionRefused` instead of the `InvalidInput` it expected). This
    // guard forces the three tests below to run one at a time. A
    // `tokio::sync::Mutex` (not `std::sync::Mutex`) is used deliberately: its
    // guard is designed to be held across an `.await` point, unlike a std
    // guard, which `clippy::await_holding_lock` flags for good reason on a
    // multi-threaded runtime.
    static ENV_VAR_TEST_LOCK: Mutex<()> = Mutex::const_new(());

    #[tokio::test]
    async fn run_stdio_bridge_fails_fast_when_port_env_var_is_missing() {
        let _guard = ENV_VAR_TEST_LOCK.lock().await;
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
        let _guard = ENV_VAR_TEST_LOCK.lock().await;
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
        let _guard = ENV_VAR_TEST_LOCK.lock().await;
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
