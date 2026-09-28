//! End-to-end tests for the per-session MCP HTTP server: real TCP, real
//! HTTP requests, real bearer-token enforcement. Complements the in-process
//! tool-router tests in `src-tauri/src/mcp/tool_server.rs` (Task 2), which
//! never open a socket.

use std::sync::{Arc, Mutex};

use reqwest::Client;
use rocket_app::McpToolService;
use rocket_collection::{
    request::Request as CollectionRequest, settings::CollectionSettings, CollectionRepository,
};
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
use tauri::Manager;
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
async fn spawn_test_server(
    session_id: &str,
) -> (
    McpHttpServerHandle,
    tauri::AppHandle<tauri::test::MockRuntime>,
    TempDir,
) {
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
        Box::new(FsEnvironmentRepo::new(
            tmp.path().join("global_environments"),
        )),
        Arc::new(FakeHttpExecutor),
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
    .with_collection_env_repo_factory(Box::new(SharedCollectionEnvironmentRepo::new(Arc::clone(
        &ws_path,
    ))));

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

/// A minimal `initialize` JSON-RPC request body. `initialize` is the one
/// request the Streamable HTTP transport accepts with no prior session:
/// `LocalSessionManager` runs in `legacy_session_mode` (the default), which
/// requires every session to begin with an `initialize` call and rejects any
/// other method with HTTP 422 ("Unexpected message, expect initialize
/// request") until one has been made -- see
/// `transport/streamable_http_server/tower.rs`'s `handle_post`. Using
/// `initialize` here (rather than a bare `tools/list`, which is what this
/// test was originally transcribed from the plan with) lets these tests
/// exercise the real request path -- routing, `Accept`/auth middleware, and
/// the tool router -- without also standing up a full
/// initialize/initialized/tools-call handshake, which is exhaustively
/// covered by `rmcp`'s own test suite rather than this one.
fn initialize_body(id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "mcp-http-server-integration-test", "version": "0.0.0"}
        }
    })
}

/// `rmcp`'s Streamable HTTP `POST` handler (`handle_post` in
/// `transport/streamable_http_server/tower.rs`) 406s any request whose
/// `Accept` header does not name both `application/json` and
/// `text/event-stream` — the MCP Streamable HTTP transport spec requires a
/// client to accept either response shape, since the server may reply with
/// either a single JSON body or an SSE stream. Discovered by actually
/// running this test against the real crate (the brief this test was
/// transcribed from predated a vendored `rmcp` copy and omitted it).
const ACCEPT_JSON_AND_EVENT_STREAM: &str = "application/json, text/event-stream";

#[tokio::test]
async fn correct_token_reaches_the_tool_router_over_real_http() {
    let (handle, _app_handle, _tmp) = spawn_test_server("session-http-1").await;

    let response = Client::new()
        .post(format!("http://127.0.0.1:{}/mcp", handle.port))
        .bearer_auth(&handle.token)
        .header("content-type", "application/json")
        .header("accept", ACCEPT_JSON_AND_EVENT_STREAM)
        .json(&initialize_body(1))
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
        .header("accept", ACCEPT_JSON_AND_EVENT_STREAM)
        .json(&initialize_body(1))
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
        .header("accept", ACCEPT_JSON_AND_EVENT_STREAM)
        .json(&initialize_body(1))
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
                .header("accept", ACCEPT_JSON_AND_EVENT_STREAM)
                .json(&initialize_body(id))
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
        if tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .is_ok()
        {
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
        .header("accept", ACCEPT_JSON_AND_EVENT_STREAM)
        .json(&initialize_body(1))
        .send()
        .await
        .expect("send request to session-http-7");
    assert_eq!(still_up.status(), reqwest::StatusCode::OK);

    handle_b.shutdown();
}
