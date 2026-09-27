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
use rocket_acp::AcpSessionClient;
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
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session should succeed against the fixture agent");
    assert!(!session_id.is_empty());
}

#[tokio::test]
async fn acp_agent_client_start_session_fails_clearly_for_a_nonexistent_command() {
    let client = AcpAgentClient::new();
    let err = client
        .start_session("definitely-not-a-real-binary-xyz123", &[], "/tmp", &[])
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
        )
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
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    let (tx, mut rx) = mpsc::unbounded_channel();
    let stop_reason = client
        .send_prompt(&session_id, "hello".to_string(), tx)
        .await
        .expect("send_prompt should succeed against the fixture agent");

    assert_eq!(stop_reason, "end_turn");
    assert_eq!(rx.recv().await, Some("fixture reply".to_string()));
}

#[tokio::test]
async fn acp_agent_client_send_prompt_works_twice_on_the_same_session_for_multi_turn_chat() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    for _ in 0..2 {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let stop_reason = client
            .send_prompt(&session_id, "hello again".to_string(), tx)
            .await
            .expect("send_prompt should succeed on a reused session");
        assert_eq!(stop_reason, "end_turn");
        assert_eq!(rx.recv().await, Some("fixture reply".to_string()));
    }
}

#[tokio::test]
async fn acp_agent_client_send_prompt_on_unknown_session_id_errors() {
    let client = AcpAgentClient::new();
    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt("no-such-session", "hi".to_string(), tx)
        .await
        .expect_err("unknown session id must error, not panic");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn acp_agent_client_end_session_kills_the_process_and_removes_the_session() {
    let client = AcpAgentClient::new();
    let session_id = client
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    client
        .end_session(&session_id)
        .await
        .expect("end_session should succeed");

    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(&session_id, "hi".to_string(), tx)
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
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    let (tx, _rx) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(&session_id, "__CRASH__".to_string(), tx)
        .await
        .expect_err("an abrupt process exit must surface as an error, not panic or hang");
    assert!(matches!(err, DomainError::Internal(_)));

    // The crashed session must be gone -- proven the same way end_session's
    // cleanup is proven: a follow-up call on the same id is NotFound, not a
    // second crash-shaped error.
    let (tx2, _rx2) = mpsc::unbounded_channel();
    let err = client
        .send_prompt(&session_id, "hi".to_string(), tx2)
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
        .start_session(&fixture_command(), &[], "/tmp", &[])
        .await
        .expect("start_session");

    let prompt_client = std::sync::Arc::clone(&client);
    let prompt_session = session_id.clone();
    let pending = tokio::spawn(async move {
        let (tx, _rx) = mpsc::unbounded_channel();
        prompt_client
            .send_prompt(&prompt_session, "__HANG__".to_string(), tx)
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
        client.start_session("sh", &["-c".to_string(), script], "/tmp", &[]),
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
    let baseline = tokio::runtime::Handle::current().metrics().num_alive_tasks();

    for _ in 0..5 {
        let session_id = client
            .start_session(&fixture_command(), &[], "/tmp", &[])
            .await
            .expect("start_session");
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

    let after = tokio::runtime::Handle::current().metrics().num_alive_tasks();
    assert!(
        after <= baseline + 1,
        "background dispatch tasks appear to have leaked: baseline={baseline}, after 5 start/end cycles={after}"
    );
}
