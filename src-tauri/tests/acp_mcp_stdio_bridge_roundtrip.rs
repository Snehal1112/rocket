// Spawns the real Plan 04 HTTP MCP backend inside a mocked Tauri app (no
// real window/webview -- `tauri::test::mock_builder` gives a real AppHandle
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

// `flavor = "multi_thread"` is load-bearing, not stylistic: this test does
// blocking (std, non-async) stdio reads/writes against the child bridge
// process on the SAME task that also drives the in-process axum server
// spawned by `spawn_mcp_http_server` via `tauri::async_runtime::spawn`. On
// the default single-threaded `#[tokio::test]` runtime, the blocking
// `read_line` call below starves that server task of its only thread, so
// the child's own upstream request to it can never be serviced -- a real
// deadlock, confirmed by inspecting the stuck connection with `ss` (bytes
// sat unread in the server's kernel receive buffer indefinitely). A second
// worker thread lets the server task keep making progress while this task
// blocks on the child.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_bridge_forwards_a_real_tool_list_round_trip() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let collections_dir = fixture.path().join("collections");
    std::fs::create_dir_all(&collections_dir).expect("create collections dir");
    let workspace_path: Arc<Mutex<std::path::PathBuf>> =
        Arc::new(Mutex::new(fixture.path().to_path_buf()));

    let collection_repo: Arc<dyn CollectionRepository> = Arc::new(
        rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone()),
    );
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

    let executor: Arc<dyn rocket_http::HttpExecutor> = Arc::new(
        rocket_infra::ReqwestExecutor::with_allowed_base(Arc::clone(&workspace_path)),
    );
    let exec_svc = Arc::new(rocket_app::RequestExecutionService::new_with_audit(
        Box::new(rocket_infra::FsEnvironmentRepo::with_secret_store(
            fixture.path().join("environments"),
            Arc::new(rocket_environment::secret_store::NullSecretStore),
        )),
        Arc::clone(&executor),
        Box::new(rocket_infra::FsHistoryRepo::new(fixture.path().join("history"))),
        Box::new(rocket_infra::FsCollectionRepo::new_standalone(
            collections_dir.clone(),
        )),
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
        Arc::new(rocket_infra::SharedCollectionEnvironmentRepo::new(
            Arc::clone(&workspace_path),
        ));

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
    // shutdown() is synchronous (Plan 04's real McpHttpServerHandle) -- no `.await`.
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
