// Integration test proving the fixture ACP agent binary (crates/rocket-infra/
// src/bin/test_acp_agent.rs) speaks real ACP over stdio.
//
// This lives under tests/ (an integration test target) rather than as a unit
// test inside src/acp_agent_client.rs, because Cargo only populates the
// CARGO_BIN_EXE_<name> environment variable used below for integration test
// targets, not for unit tests compiled as part of the library target. See
// the task report for details.
use agent_client_protocol::schema::v1::InitializeRequest;
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent as AgentRole, Client, ConnectionTo};
use rocket_acp::{AcpSessionClient, AcpUpdate, McpServerSpec, PromptPart, ToolCallStatus};
use rocket_infra::AcpAgentClient;
use rocket_shared::error::DomainError;
use tokio::sync::mpsc;

// Returns the path to the fixture ACP agent binary (crates/rocket-infra/src/
// bin/test_acp_agent.rs). Cargo only populates `CARGO_BIN_EXE_<name>` for
// integration-test targets under `tests/`, not for unit tests compiled into
// the lib target -- see the task report for why these tests live here
// rather than in a `#[cfg(test)] mod tests` block inside
// `src/acp_agent_client.rs`.
fn fixture_command() -> String {
    env!("CARGO_BIN_EXE_test_acp_agent").to_string()
}

// Named with an "acp_agent_client" prefix (rather than the brief's plain
// "fixture_agent_completes_initialize_handshake") so it still matches
// `cargo test -p rocket-infra acp_agent_client`. Cargo's test-name filter
// only sees the module-qualified path; for an integration test file (unlike
// a unit test nested under `mod acp_agent_client { mod tests { ... } }`) that
// path is just the bare function name, so the file name alone isn't part of
// the match.
#[tokio::test]
async fn acp_agent_client_fixture_agent_completes_initialize_handshake() {
    // AcpAgent::from_args's exact name/signature is re-verified as part
    // of Task 2 (see this plan's Global Constraints) — this test uses
    // it minimally, just to prove the fixture binary itself is a valid
    // ACP agent, before AcpAgentClient exists.
    let agent = agent_client_protocol::AcpAgent::from_args([env!("CARGO_BIN_EXE_test_acp_agent")])
        .expect("build spawn config for fixture agent");

    let result = Client
        .builder()
        .connect_with(agent, |connection: ConnectionTo<AgentRole>| async move {
            connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await
        })
        .await;

    result.expect("initialize handshake should succeed against the fixture agent");
}

#[tokio::test]
async fn acp_agent_client_start_session_returns_a_session_id() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session should succeed against the fixture agent")
        .session_id;
    assert!(!session_id.is_empty());
}

#[tokio::test]
async fn acp_agent_client_start_session_fails_clearly_for_a_nonexistent_command() {
    let client = AcpAgentClient::new();
    let err = client
        .start_session(
            "definitely-not-a-real-binary-xyz123",
            &[],
            "/tmp",
            &[],
            &[],
            None,
        )
        .await
        .expect_err("nonexistent command must fail, not panic");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test]
async fn acp_agent_client_start_session_error_never_contains_the_credential_value() {
    let client = AcpAgentClient::new();
    let err = client
        .start_session(
            "definitely-not-a-real-binary-xyz123",
            &[],
            "/tmp",
            &[(
                "ANTHROPIC_API_KEY".to_string(),
                "sk-super-secret-test-value".to_string(),
            )],
            &[],
            None,
        )
        .await
        .expect_err("nonexistent command must fail");
    let message = err.to_string();
    assert!(
        !message.contains("sk-super-secret-test-value"),
        "error message must never contain the credential value, got: {message}"
    );
}

// Task 2's review flagged (non-blocking) that the credential-non-leakage
// coverage so far only exercised the synchronous spawn-failure path (an
// altogether nonexistent command). This closes the async-handshake-failure
// gap: `true` is a real, spawnable binary that exits immediately without
// speaking ACP, so the failure surfaces from the background handshake task
// (`start_session`'s `ready_rx` receiving an `Err`) rather than from
// `spawn_process` itself.
#[tokio::test]
async fn acp_agent_client_async_handshake_failure_never_contains_the_credential_value() {
    let client = AcpAgentClient::new();
    let err = client
        .start_session(
            "true",
            &[],
            "/tmp",
            &[(
                "ANTHROPIC_API_KEY".to_string(),
                "sk-super-secret-async-value".to_string(),
            )],
            &[], None)
        .await
        .expect_err(
            "a process that exits immediately without speaking ACP must fail the handshake, not panic or hang",
        );
    assert!(matches!(err, DomainError::Internal(_)));
    let message = err.to_string();
    assert!(
        !message.contains("sk-super-secret-async-value"),
        "async handshake-failure error message must never contain the credential value, got: {message}"
    );
}

// Task 3: send_prompt, end_session, and process/task lifecycle cleanup.
// Named with the `acp_agent_client_` prefix for the same reason as the tests
// above -- Cargo's substring test filter only sees the bare function name for
// an integration-test target, so the prefix is what keeps
// `cargo test -p rocket-infra acp_agent_client` matching these.

#[tokio::test]
async fn acp_agent_client_send_prompt_streams_a_chunk_and_returns_a_stop_reason() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = client
        .send_prompt(&session_id, vec![PromptPart::Text("hello".to_string())], tx)
        .await
        .expect("send_prompt should succeed against the fixture agent");

    assert_eq!(stop_reason, "end_turn");
    assert_eq!(
        rx.recv().await,
        Some(AcpUpdate::Text {
            text: "fixture reply".to_string()
        })
    );
}

#[tokio::test]
async fn acp_agent_client_send_prompt_works_twice_on_the_same_session_for_multi_turn_chat() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    for _ in 0..2 {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let stop_reason = client
            .send_prompt(
                &session_id,
                vec![PromptPart::Text("hello again".to_string())],
                tx,
            )
            .await
            .expect("send_prompt should succeed on a reused session");
        assert_eq!(stop_reason, "end_turn");
        assert_eq!(
            rx.recv().await,
            Some(AcpUpdate::Text {
                text: "fixture reply".to_string()
            })
        );
    }
}

#[tokio::test]
async fn acp_agent_client_send_prompt_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(
            "no-such-session",
            vec![PromptPart::Text("hi".to_string())],
            tx,
        )
        .await
        .expect_err("unknown session id must error, not panic");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_end_session_kills_the_process_and_removes_the_session() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    client
        .end_session(&session_id)
        .await
        .expect("end_session should succeed");

    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(&session_id, vec![PromptPart::Text("hi".to_string())], tx)
        .await
        .expect_err("session must be gone after end_session");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_end_session_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let err = client
        .end_session("no-such-session")
        .await
        .expect_err("unknown session id must error, not panic");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_a_crashed_agent_is_removed_from_the_session_map() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(
            &session_id,
            vec![PromptPart::Text("__CRASH__".to_string())],
            tx,
        )
        .await
        .expect_err("an abrupt process exit must surface as an error, not panic or hang");
    assert!(matches!(err, DomainError::Internal(_)));

    // The crashed session must be gone -- proven the same way end_session's
    // cleanup is proven: a follow-up call on the same id is NotFound, not a
    // second crash-shaped error.
    let (tx2, _rx2) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(&session_id, vec![PromptPart::Text("hi".to_string())], tx2)
        .await
        .expect_err("a crashed session must be removed from the map, not left dangling");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_end_session_unblocks_an_in_flight_prompt() {
    // Plan 04's timeout path ends a session while its prompt is still
    // pending. The pending `send_prompt` must then fail promptly, not hang.
    let client = std::sync::Arc::new(AcpAgentClient::new());
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let prompt_client = std::sync::Arc::clone(&client);
    let prompt_session = session_id.clone();
    let pending = tokio::spawn(async move {
        let (tx, _rx) = mpsc::unbounded_channel();
        prompt_client
            .send_prompt(
                &prompt_session,
                vec![PromptPart::Text("__HANG__".to_string())],
                tx,
            )
            .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    client
        .end_session(&session_id)
        .await
        .expect("end_session should succeed");

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
        .await
        .expect("send_prompt must not hang after end_session")
        .expect("prompt task must not panic");
    assert!(result.is_err(), "an ended session's prompt must fail");
}

// Returns true once `pid` is gone or only left as an unreaped zombie.
#[cfg(target_os = "linux")]
async fn wait_for_process_exit(pid: &str) -> bool {
    for _ in 0..50 {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(_) => return true,
            Ok(stat) if stat.split(") ").nth(1).is_some_and(|s| s.starts_with('Z')) => {
                return true;
            }
            Ok(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    }
    false
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn acp_agent_client_cancelled_start_session_kills_the_whole_process_group() {
    // A wrapper launcher (like `npx`) whose real worker is a grandchild that
    // never answers `initialize`. Dropping `start_session` (as a caller-side
    // timeout does) must kill the grandchild too, not orphan it.
    let dir = tempfile::tempdir().expect("tempdir");
    let pid_file = dir.path().join("grandchild.pid");
    let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());

    let client = AcpAgentClient::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        client.start_session("sh", &["-c".to_string(), script], "/tmp", &[], &[], None),
    )
    .await;
    assert!(outcome.is_err(), "the handshake must still be pending");

    let pid = std::fs::read_to_string(&pid_file).expect("read grandchild pid");
    assert!(
        wait_for_process_exit(pid.trim()).await,
        "grandchild process {} survived a cancelled start_session",
        pid.trim()
    );
}

#[tokio::test]
async fn acp_agent_client_end_session_aborts_the_background_dispatch_task() {
    // Structural proof for the JoinHandle-abort requirement: every
    // `RunningSession` stores the `tokio::spawn` handle of the background
    // task that drives its ACP connection (`RunningSession::join_handle` in
    // acp_agent_client.rs), and `end_session`/`fail_and_remove` must abort
    // it -- otherwise that task, parked forever in
    // `std::future::pending::<()>()` after the handshake (per
    // agent-client-protocol's own docs: a transport EOF does not cancel it),
    // leaks for the life of the process. `tokio::runtime::RuntimeMetrics::
    // num_alive_tasks` is a stable (non-`tokio_unstable`) API as of tokio
    // 1.50 -- this test uses it to prove the abort takes effect rather than
    // only asserting it structurally: the runtime's live task count must not
    // grow across repeated start/end cycles.
    let client = AcpAgentClient::new();

    // Let any tasks already alive in this test's own runtime settle first.
    for _ in 0..3 {
        tokio::task::yield_now().await;
    }
    let baseline = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();

    for _ in 0..5 {
        let session_id = client
            .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
            .await
            .expect("start_session")
            .session_id;
        client
            .end_session(&session_id)
            .await
            .expect("end_session should succeed");
    }

    // Aborting a task only takes effect the next time the runtime schedules
    // it; give the runtime a few passes (plus a short sleep) to actually
    // drop the aborted tasks before asserting on the count.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let after = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();
    assert!(
        after <= baseline + 1,
        "background dispatch tasks appear to have leaked: baseline={baseline}, after 5 start/end cycles={after}"
    );
}

#[tokio::test]
async fn acp_agent_client_end_all_sessions_kills_every_running_session() {
    let client = AcpAgentClient::new();
    let session_a = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session a")
        .session_id;
    let session_b = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session b")
        .session_id;

    client
        .end_all_sessions()
        .await
        .expect("end_all_sessions should succeed");

    let (tx_a, _rx_a) = tokio::sync::mpsc::unbounded_channel();
    let err_a = client
        .send_prompt(&session_a, vec![PromptPart::Text("hi".to_string())], tx_a)
        .await
        .expect_err("session a must be gone after end_all_sessions");
    assert!(matches!(err_a, DomainError::NotFound(_)));

    let (tx_b, _rx_b) = tokio::sync::mpsc::unbounded_channel();
    let err_b = client
        .send_prompt(&session_b, vec![PromptPart::Text("hi".to_string())], tx_b)
        .await
        .expect_err("session b must be gone after end_all_sessions");
    assert!(matches!(err_b, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_end_all_sessions_on_empty_map_succeeds() {
    let client = AcpAgentClient::new();
    client
        .end_all_sessions()
        .await
        .expect("end_all_sessions on an empty session map must succeed, not error");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn acp_agent_client_end_all_sessions_kills_a_session_still_in_its_handshake() {
    // An agent that never answers `initialize` is not in the session map
    // yet. The app-exit sweep must still kill its whole process group, or
    // quitting mid-handshake would orphan it with its credential.
    let dir = tempfile::tempdir().expect("tempdir");
    let pid_file = dir.path().join("grandchild.pid");
    let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());

    let client = std::sync::Arc::new(AcpAgentClient::new());
    let starter = {
        let client = std::sync::Arc::clone(&client);
        tokio::spawn(async move {
            client
                .start_session("sh", &["-c".to_string(), script], "/tmp", &[], &[], None)
                .await
        })
    };

    let mut pid = None;
    for _ in 0..50 {
        if let Ok(text) = std::fs::read_to_string(&pid_file) {
            if !text.trim().is_empty() {
                pid = Some(text.trim().to_string());
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let pid = pid.expect("grandchild pid file was never written");

    client
        .end_all_sessions()
        .await
        .expect("end_all_sessions should succeed");

    assert!(
        wait_for_process_exit(&pid).await,
        "grandchild process {pid} survived end_all_sessions during its handshake"
    );
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), starter)
        .await
        .expect("start_session must finish once its process is killed")
        .expect("start_session task must not panic");
    assert!(
        outcome.is_err(),
        "a killed handshake must not yield a session"
    );
}

#[tokio::test]
async fn acp_agent_client_start_session_after_end_all_sessions_is_refused() {
    let client = AcpAgentClient::new();
    client
        .end_all_sessions()
        .await
        .expect("end_all_sessions should succeed");

    let result = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await;
    assert!(
        result.is_err(),
        "no session may be stored after the app-exit sweep"
    );
}

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
            &[
                (
                    "MCP_SERVERS_DUMP_PATH".to_string(),
                    dump_path.display().to_string(),
                ),
                ("FIXTURE_ADVERTISE_MCP_HTTP".to_string(), "1".to_string()),
            ],
            &specs,
            None,
        )
        .await
        .expect("start_session should succeed against the fixture agent");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump mcp_servers");
    assert!(dumped.contains("\"type\":\"http\""), "got: {dumped}");
    assert!(
        dumped.contains("\"name\":\"rocket-tools\""),
        "got: {dumped}"
    );
    assert!(
        dumped.contains("\"url\":\"http://127.0.0.1:4000/mcp\""),
        "got: {dumped}"
    );
    assert!(
        dumped.contains("\"name\":\"Authorization\""),
        "got: {dumped}"
    );
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
            None,
        )
        .await
        .expect("start_session should succeed against the fixture agent");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump mcp_servers");
    assert!(!dumped.contains("\"type\":\"http\""), "got: {dumped}");
    assert!(dumped.contains("\"command\":\"rocket\""), "got: {dumped}");
    assert!(dumped.contains("--acp-mcp-stdio-bridge"), "got: {dumped}");
    assert!(
        dumped.contains("\"name\":\"ROCKET_MCP_PORT\""),
        "got: {dumped}"
    );
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
            None,
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
    // which this test does not set. A capability mismatch never blocks the
    // session; the unsupported HTTP server is simply not attached.
    let dir = tempfile::tempdir().expect("tempdir");
    let dump_path = dir.path().join("mcp_servers.json");

    let client = AcpAgentClient::new();
    let specs = vec![McpServerSpec::Http {
        name: "rocket-tools".to_string(),
        url: "http://127.0.0.1:4000/mcp".to_string(),
        token: "secret-token".to_string(),
    }];
    let session_id = client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[(
                "MCP_SERVERS_DUMP_PATH".to_string(),
                dump_path.display().to_string(),
            )],
            &specs,
            None,
        )
        .await
        .expect("a capability mismatch must not fail start_session")
        .session_id;
    assert!(!session_id.is_empty());

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump mcp_servers");
    assert_eq!(dumped.trim(), "[]");
}

fn http_and_stdio_specs_with_same_name() -> Vec<McpServerSpec> {
    vec![
        McpServerSpec::Http {
            name: "rocket".to_string(),
            url: "http://127.0.0.1:4000/mcp".to_string(),
            token: "secret-token".to_string(),
        },
        McpServerSpec::Stdio {
            name: "rocket".to_string(),
            command: "rocket".to_string(),
            args: vec!["--acp-mcp-stdio-bridge".to_string()],
            env: vec![("ROCKET_MCP_PORT".to_string(), "4000".to_string())],
        },
    ]
}

#[tokio::test]
async fn acp_agent_client_start_session_picks_http_over_stdio_when_agent_advertises_http() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dump_path = dir.path().join("mcp_servers.json");

    let client = AcpAgentClient::new();
    client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[
                (
                    "MCP_SERVERS_DUMP_PATH".to_string(),
                    dump_path.display().to_string(),
                ),
                ("FIXTURE_ADVERTISE_MCP_HTTP".to_string(), "1".to_string()),
            ],
            &http_and_stdio_specs_with_same_name(),
            None,
        )
        .await
        .expect("start_session should succeed against the fixture agent");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump mcp_servers");
    assert!(dumped.contains("\"type\":\"http\""), "got: {dumped}");
    assert!(!dumped.contains("--acp-mcp-stdio-bridge"), "got: {dumped}");
}

#[tokio::test]
async fn acp_agent_client_start_session_falls_back_to_stdio_when_agent_lacks_http() {
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
            &http_and_stdio_specs_with_same_name(),
            None,
        )
        .await
        .expect("start_session should succeed against the fixture agent");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump mcp_servers");
    assert!(!dumped.contains("\"type\":\"http\""), "got: {dumped}");
    assert!(!dumped.contains("secret-token"), "got: {dumped}");
    assert!(dumped.contains("--acp-mcp-stdio-bridge"), "got: {dumped}");
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
            None,
        )
        .await
        .expect_err("nonexistent command must fail, not panic");
    let message = err.to_string();
    assert!(
        !message.contains("sk-mcp-super-secret-test-value"),
        "error message must never contain the mcp token value, got: {message}"
    );
}
// Plan 01 (workspace AI assistant): typed updates, session info, meta,
// option changes, cancel, permission deny and prompt parts.

fn drain_updates(rx: &mut mpsc::UnboundedReceiver<AcpUpdate>) -> Vec<AcpUpdate> {
    let mut updates = Vec::new();
    while let Ok(update) = rx.try_recv() {
        updates.push(update);
    }
    updates
}

#[tokio::test]
async fn acp_agent_client_start_session_returns_config_options_and_prompt_capabilities() {
    let client = AcpAgentClient::new();
    let info = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session");

    assert_eq!(info.session_id, "fixture-session");
    assert!(info.prompt_capabilities.embedded_context);
    assert!(info.prompt_capabilities.image);
    // The fixture also sends a boolean `fast` option, which must be skipped.
    assert_eq!(
        info.config_options.len(),
        1,
        "got {:?}",
        info.config_options
    );
    let model = &info.config_options[0];
    assert_eq!(model.id, "model");
    assert_eq!(model.category.as_deref(), Some("model"));
    assert_eq!(model.current_value, "default");
    let values: Vec<&str> = model.choices.iter().map(|c| c.value.as_str()).collect();
    assert_eq!(values, vec!["default", "opus"]);
    assert_eq!(
        model.choices[1].description.as_deref(),
        Some("Most capable")
    );
}

#[tokio::test]
async fn acp_agent_client_start_session_passes_meta_through_to_new_session_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dump_path = dir.path().join("meta.json");
    let meta = serde_json::json!({ "claudeCode": { "options": { "tools": [] } } });

    let client = AcpAgentClient::new();
    client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[(
                "SESSION_META_DUMP_PATH".to_string(),
                dump_path.display().to_string(),
            )],
            &[],
            Some(meta.clone()),
        )
        .await
        .expect("start_session");

    let dumped = std::fs::read_to_string(&dump_path).expect("fixture should dump meta");
    let dumped: serde_json::Value = serde_json::from_str(&dumped).expect("parse dump");
    assert_eq!(dumped, meta);
}

#[tokio::test]
async fn acp_agent_client_start_session_rejects_meta_that_is_not_an_object() {
    let client = AcpAgentClient::new();
    let err = client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[],
            &[],
            Some(serde_json::json!("not an object")),
        )
        .await
        .expect_err("a non-object meta must be refused");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test]
async fn acp_agent_client_send_prompt_forwards_tool_calls_usage_and_config_options() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = client
        .send_prompt(
            &session_id,
            vec![PromptPart::Text("__TOOLS__".to_string())],
            tx,
        )
        .await
        .expect("send_prompt");
    assert_eq!(stop_reason, "end_turn");

    let updates = drain_updates(&mut rx);
    assert_eq!(updates.len(), 5, "got {updates:?}");
    assert_eq!(
        updates[0],
        AcpUpdate::ToolCall {
            call_id: "call-1".to_string(),
            title: "Read file".to_string(),
            kind: "read".to_string(),
            status: ToolCallStatus::Pending,
        }
    );
    assert_eq!(
        updates[1],
        AcpUpdate::ToolCallUpdate {
            call_id: "call-1".to_string(),
            title: None,
            status: Some(ToolCallStatus::Completed),
        }
    );
    assert_eq!(
        updates[2],
        AcpUpdate::Usage {
            used: 53_000,
            size: 200_000,
            cost_usd: Some(0.045),
        }
    );
    match &updates[3] {
        AcpUpdate::ConfigOptions { options } => {
            let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
            assert_eq!(ids, vec!["model", "effort"]);
        }
        other => panic!("expected ConfigOptions, got {other:?}"),
    }
    assert_eq!(
        updates[4],
        AcpUpdate::Text {
            text: "fixture reply".to_string()
        }
    );
}

#[tokio::test]
async fn acp_agent_client_cancel_ends_the_turn_as_cancelled_and_keeps_the_session() {
    let client = std::sync::Arc::new(AcpAgentClient::new());
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let prompt_client = std::sync::Arc::clone(&client);
    let prompt_session = session_id.clone();
    let pending = tokio::spawn(async move {
        let (tx, _rx) = mpsc::unbounded_channel();
        prompt_client
            .send_prompt(
                &prompt_session,
                vec![PromptPart::Text("__WAIT_FOR_CANCEL__".to_string())],
                tx,
            )
            .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // Cancel must not wait for the prompt lock that the pending turn holds.
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client.cancel(&session_id),
    )
    .await
    .expect("cancel must not block on the running turn")
    .expect("cancel should succeed");

    let stop_reason = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
        .await
        .expect("the cancelled turn must end")
        .expect("prompt task must not panic")
        .expect("a cancelled turn is a normal finish");
    assert_eq!(stop_reason, "cancelled");

    // The session is still alive after a cancel.
    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = client
        .send_prompt(&session_id, vec![PromptPart::Text("hello".to_string())], tx)
        .await
        .expect("the session must survive a cancel");
    assert_eq!(stop_reason, "end_turn");
    assert_eq!(
        rx.recv().await,
        Some(AcpUpdate::Text {
            text: "fixture reply".to_string()
        })
    );
}

#[tokio::test]
async fn acp_agent_client_cancel_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let err = client
        .cancel("no-such-session")
        .await
        .expect_err("unknown session id must error");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_set_config_option_returns_the_new_option_list() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let options = client
        .set_config_option(&session_id, "model", "opus")
        .await
        .expect("set_config_option");
    let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["model", "effort"]);
    assert_eq!(options[0].current_value, "opus");
    assert_eq!(options[1].category.as_deref(), Some("thought_level"));
}

#[tokio::test]
async fn acp_agent_client_set_config_option_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let err = client
        .set_config_option("no-such-session", "model", "opus")
        .await
        .expect_err("unknown session id must error");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_denies_permission_requests() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.send_prompt(
            &session_id,
            vec![PromptPart::Text("__PERMISSION__".to_string())],
            tx,
        ),
    )
    .await
    .expect("a permission request must never hang the turn")
    .expect("send_prompt");
    assert_eq!(stop_reason, "end_turn");
    assert_eq!(
        drain_updates(&mut rx),
        vec![AcpUpdate::Text {
            text: "permission:selected:reject-once".to_string()
        }]
    );
}

#[tokio::test]
async fn acp_agent_client_sends_resources_as_embedded_resources_when_supported() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[], &[], None)
        .await
        .expect("start_session")
        .session_id;

    let (tx, mut rx) = mpsc::unbounded_channel();
    client
        .send_prompt(
            &session_id,
            vec![
                PromptPart::Resource {
                    uri: "rocket://request/a".to_string(),
                    mime_type: Some("text/plain".to_string()),
                    text: "GET /a".to_string(),
                },
                PromptPart::Text("__DESCRIBE__".to_string()),
            ],
            tx,
        )
        .await
        .expect("send_prompt");
    assert_eq!(
        drain_updates(&mut rx),
        vec![AcpUpdate::Text {
            text: "resource:rocket://request/a:text/plain|text".to_string()
        }]
    );
}

#[tokio::test]
async fn acp_agent_client_sends_resources_as_text_without_embedded_context() {
    let client = AcpAgentClient::new();
    let info = client
        .start_session(
            &fixture_command(),
            &[],
            "/tmp",
            &[("FIXTURE_NO_EMBEDDED_CONTEXT".to_string(), "1".to_string())],
            &[],
            None,
        )
        .await
        .expect("start_session");
    assert!(!info.prompt_capabilities.embedded_context);

    let (tx, mut rx) = mpsc::unbounded_channel();
    client
        .send_prompt(
            &info.session_id,
            vec![
                PromptPart::Resource {
                    uri: "rocket://request/a".to_string(),
                    mime_type: None,
                    text: "GET /a".to_string(),
                },
                PromptPart::Text("__DESCRIBE__".to_string()),
            ],
            tx,
        )
        .await
        .expect("send_prompt");
    assert_eq!(
        drain_updates(&mut rx),
        vec![AcpUpdate::Text {
            text: "text|text".to_string()
        }]
    );
}

#[tokio::test]
async fn acp_agent_client_send_prompt_with_no_parts_is_rejected() {
    let client = AcpAgentClient::new();
    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt("no-such-session", Vec::new(), tx)
        .await
        .expect_err("an empty prompt must be refused");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}
