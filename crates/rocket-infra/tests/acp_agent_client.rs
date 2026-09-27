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
