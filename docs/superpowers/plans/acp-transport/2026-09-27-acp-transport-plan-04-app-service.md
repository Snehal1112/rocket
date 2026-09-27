# ACP Transport Plan 04: App Service — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `AgentConfigService::get` and the new `AcpSessionService` — the orchestration layer that resolves an agent's command/credential, drives `AcpSessionClient` (Plan 01/03), and publishes the `DomainEvent`s from Plan 02.

**Architecture:** `AcpSessionService` holds `Box<dyn AcpSessionClient>` + `Box<dyn EventPublisher>` + `Arc<AgentConfigService>`, the same trait-object-injection pattern every service in this crate already uses. Chunk-to-event translation and the chunk-before-finished ordering guarantee are done with `tokio::join!` inside `send_prompt` (concurrently draining the chunk channel while awaiting the trait call, in the same async function) rather than a separately spawned task — this avoids `Send`/`'static` complications from sharing `Box<dyn EventPublisher>` across a spawned task boundary, while still giving a provable ordering guarantee: `tokio::join!` only returns once *both* futures resolve, and the chunk-draining future only resolves once the channel closes, which happens after the trait call has already sent every chunk — so every `AcpSessionChunk` publish has already happened by the time `send_prompt` goes on to publish `AcpSessionFinished`.

**Tech Stack:** Rust, `tokio` (`time::timeout`, `sync::mpsc`, `join!`).

**Spec:** `docs/superpowers/specs/2026-09-27-acp-transport-design.md` (Architecture & crate placement, Session lifecycle & data flow, Error handling). Plan index: `docs/superpowers/plans/acp-transport/00-plan-index.md`.

## Global Constraints

- `AgentConfigService::get` is a public wrapper over the crate's existing private `get_config` helper (already used internally by `resolve_credential`/`test_agent_config`) — do not duplicate its logic.
- The 120-second `send_prompt` timeout (per the spec, "a fixed 120-second constant... not user-configurable") is the *production* value, wired via `AcpSessionService::new`. A second constructor, `with_prompt_timeout`, takes an explicit duration purely as a test seam — it does not add end-user configurability (there is no settings surface calling it; only Plan 04's own tests and, if ever needed, another test module would use it). Plan 05's `lib.rs` wiring always uses `new`.
- On timeout, `AcpSessionService` calls `AcpSessionClient::end_session` itself to force-kill the process (reusing Plan 03's existing kill logic) rather than duplicating process-management logic here — dropping the timed-out future does not, by itself, kill anything.
- `DomainError` derives `PartialEq` but not `Clone` — test doubles reconstruct errors fresh per call rather than storing a pre-built `DomainResult` to return, matching the existing pattern in `secret_manager_service.rs`'s test module.
- `DomainEvent` derives `Clone`/`Debug` but not `PartialEq` — tests inspect published events with `matches!(...)` / field destructuring, not `assert_eq!` on the whole event.
- Test code uses `.expect("message")` for fallible calls, never the bare panicking shorthand.

## Review Focus

- `start_session`'s credential resolution failure (stale vault secret, unknown connection, etc. — all already handled by `AgentConfigService`) must propagate unchanged, not get re-wrapped or swallowed.
- The ordering guarantee (every `AcpSessionChunk` before `AcpSessionFinished`) needs a test with more than one chunk, not just one — a single-chunk test can pass by accident even with a buggy ordering implementation.
- The timeout path must call `end_session` on the underlying client (verified via a test double that records whether it was called), not just give up and leave the process running.
- A `send_prompt` failure (crash, protocol error) must publish `AcpSessionFailed` *and* still return the error to the caller — both must happen, not one or the other.

---

## Task 1: `AgentConfigService::get`

**Files:**
- Modify: `crates/rocket-app/src/agent_config_service.rs`

**Interfaces:**
- Produces: `AgentConfigService::get(&self, id: &str) -> DomainResult<AgentConfig>` — consumed by Task 2 of this plan (`AcpSessionService::start_session`).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/agent_config_service.rs (add to the existing tests module)

#[test]
fn get_returns_config_when_it_exists() {
    let service = service_with(Ok(None), true);
    service
        .save(sample_config("agent-1", "conn-1"))
        .expect("save");
    let config = service.get("agent-1").expect("get should find the config");
    assert_eq!(config.id, "agent-1");
}

#[test]
fn get_errors_when_unknown() {
    let service = service_with(Ok(None), true);
    let err = service
        .get("no-such-agent")
        .expect_err("unknown id must error");
    assert!(matches!(err, DomainError::NotFound(_)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app agent_config_service::tests::get_ -j4`
Expected: FAIL with "no method named `get` found" (compile error — the crate already has a private `get_config`, but no public `get`).

- [ ] **Step 3: Add the public method**

```rust
// crates/rocket-app/src/agent_config_service.rs — add inside `impl AgentConfigService`,
// anywhere after `delete`.

/// Fetches one agent's full configuration by id — needed by `AcpSessionService`
/// (subproject B), which requires `command`/`args`/`working_dir`/
/// `credential_env_var`, not just the resolved credential value.
pub fn get(&self, id: &str) -> DomainResult<AgentConfig> {
    self.get_config(id)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app agent_config_service -j4`
Expected: PASS — 2 new tests, plus every pre-existing test in this module unaffected.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/agent_config_service.rs
git commit -m "feat(app): add AgentConfigService::get"
```

---

## Task 2: `AcpSessionService::start_session` and `send_prompt`

**Files:**
- Create: `crates/rocket-app/src/acp_session_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `AcpSessionClient` (Plan 01/03), `DomainEvent::AcpSession*` (Plan 02), `AgentConfigService::get`/`resolve_credential` (Task 1 of this plan, subproject A).
- Produces: `AcpSessionService::new`/`start_session`/`send_prompt` — consumed by Task 3 of this plan (`end_session` extends the same struct) and Plan 05 (Tauri commands call these).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/acp_session_service.rs
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rocket_acp::AcpSessionClient;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use tokio::sync::mpsc::UnboundedSender;

use crate::agent_config_service::AgentConfigService;

pub struct AcpSessionService {
    session_client: Box<dyn AcpSessionClient>,
    event_publisher: Box<dyn EventPublisher>,
    agent_config_service: Arc<AgentConfigService>,
    prompt_timeout: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::{AgentConfig, AgentConfigRepository};
    use rocket_environment::external_secret::ExternalSecretRef;
    use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
    use rocket_environment::secret_store::SecretStore;
    use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;

    struct FakeAgentConfigRepo(Mutex<Vec<AgentConfig>>);
    impl AgentConfigRepository for FakeAgentConfigRepo {
        fn list(&self) -> DomainResult<Vec<AgentConfig>> {
            Ok(self.0.lock().expect("lock FakeAgentConfigRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeAgentConfigRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, config: &AgentConfig) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeAgentConfigRepo");
            guard.retain(|c| c.id != config.id);
            guard.push(config.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeAgentConfigRepo")
                .retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretManagerRepo(Mutex<Vec<SecretManagerConnection>>);
    impl SecretManagerRepository for FakeSecretManagerRepo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            Ok(self.0.lock().expect("lock FakeSecretManagerRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeSecretManagerRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeSecretManagerRepo");
            guard.retain(|c| c.id != connection.id);
            guard.push(connection.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeSecretManagerRepo")
                .retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretStore;
    impl SecretStore for FakeSecretStore {
        fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
            Ok(Some("shh-its-a-secret".to_string()))
        }
        fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    /// `secret_value_result: Ok(None)` simulates a stale/deleted vault
    /// secret (the credential-resolution-failure case this plan's Review
    /// Focus requires a test for) without needing a second fake type.
    struct FakeVaultFetcher {
        secret_value_result: DomainResult<Option<String>>,
    }
    impl Default for FakeVaultFetcher {
        fn default() -> Self {
            Self {
                secret_value_result: Ok(Some("sk-abc123".to_string())),
            }
        }
    }
    #[async_trait::async_trait]
    impl VaultSecretFetcher for FakeVaultFetcher {
        async fn list_secrets(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(Vec::new())
        }
        async fn get_secret_value(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            _secret_id: &str,
        ) -> DomainResult<Option<String>> {
            match &self.secret_value_result {
                Ok(v) => Ok(v.clone()),
                Err(DomainError::Internal(msg)) => Err(DomainError::Internal(msg.clone())),
                Err(_) => Err(DomainError::Internal("fake fetcher error".to_string())),
            }
        }
        async fn test_connection(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    fn sample_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    fn sample_config() -> AgentConfig {
        AgentConfig {
            id: "agent-1".to_string(),
            label: "Claude Agent".to_string(),
            command: "claude-agent-acp".to_string(),
            args: Vec::new(),
            working_dir: None,
            credential_env_var: "ANTHROPIC_API_KEY".to_string(),
            vault_connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            vault_secret_id: "secret-id-1".to_string(),
            vault_secret_name: "anthropic-api-key".to_string(),
        }
    }

    fn agent_config_service() -> Arc<AgentConfigService> {
        agent_config_service_with(FakeVaultFetcher::default())
    }

    fn agent_config_service_with(fetcher: FakeVaultFetcher) -> Arc<AgentConfigService> {
        let sm_repo = FakeSecretManagerRepo(Mutex::new(vec![sample_connection()]));
        let secret_manager = Arc::new(crate::secret_manager_service::SecretManagerService::new(
            Box::new(sm_repo),
            Arc::new(FakeSecretStore),
            Arc::new(fetcher),
        ));
        let repo = FakeAgentConfigRepo(Mutex::new(vec![sample_config()]));
        Arc::new(AgentConfigService::new(Box::new(repo), secret_manager))
    }

    struct FakeSessionClient {
        start_should_fail: bool,
        prompt_chunks: Vec<String>,
        prompt_stop_reason: String,
        prompt_should_fail: bool,
        prompt_delay: Duration,
        end_session_called: Arc<AtomicBool>,
    }
    impl Default for FakeSessionClient {
        fn default() -> Self {
            Self {
                start_should_fail: false,
                prompt_chunks: vec!["hello".to_string()],
                prompt_stop_reason: "end_turn".to_string(),
                prompt_should_fail: false,
                prompt_delay: Duration::ZERO,
                end_session_called: Arc::new(AtomicBool::new(false)),
            }
        }
    }
    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
        ) -> DomainResult<String> {
            if self.start_should_fail {
                Err(DomainError::InvalidInput("command not found".to_string()))
            } else {
                Ok("session-1".to_string())
            }
        }
        async fn send_prompt(
            &self,
            _session_id: &str,
            _prompt: String,
            chunk_tx: UnboundedSender<String>,
        ) -> DomainResult<String> {
            tokio::time::sleep(self.prompt_delay).await;
            for chunk in &self.prompt_chunks {
                let _ = chunk_tx.send(chunk.clone());
            }
            if self.prompt_should_fail {
                Err(DomainError::Internal("agent crashed".to_string()))
            } else {
                Ok(self.prompt_stop_reason.clone())
            }
        }
        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            self.end_session_called.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FakeEventPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }
    impl FakeEventPublisher {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }
    impl EventPublisher for FakeEventPublisher {
        fn publish(&self, event: DomainEvent) {
            self.events.lock().expect("lock FakeEventPublisher").push(event);
        }
    }
    impl EventPublisher for Arc<FakeEventPublisher> {
        fn publish(&self, event: DomainEvent) {
            FakeEventPublisher::publish(self, event);
        }
    }

    #[tokio::test]
    async fn start_session_resolves_config_and_credential_and_publishes_started() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let service = AcpSessionService::new(
            Box::new(FakeSessionClient::default()),
            Box::new(Arc::clone(&publisher)),
            agent_config_service(),
        );

        let session_id = service
            .start_session("agent-1", "/tmp")
            .await
            .expect("start_session should succeed");
        assert_eq!(session_id, "session-1");

        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DomainEvent::AcpSessionStarted { .. }));
    }

    #[tokio::test]
    async fn send_prompt_publishes_every_chunk_before_finished_in_order() {
        let publisher = Arc::new(FakeEventPublisher::new());
        let client = FakeSessionClient {
            prompt_chunks: vec!["Hello, ".to_string(), "world!".to_string()],
            ..Default::default()
        };
        let service = AcpSessionService::new(
            Box::new(client),
            Box::new(Arc::clone(&publisher)),
            agent_config_service(),
        );

        let stop_reason = service
            .send_prompt("session-1", "hi".to_string())
            .await
            .expect("send_prompt should succeed");
        assert_eq!(stop_reason, "end_turn");

        let events = publisher.events.lock().expect("lock");
        assert_eq!(events.len(), 3, "expected 2 chunks then 1 finished, got {events:?}");
        match (&events[0], &events[1], &events[2]) {
            (
                DomainEvent::AcpSessionChunk { text: t0, .. },
                DomainEvent::AcpSessionChunk { text: t1, .. },
                DomainEvent::AcpSessionFinished { stop_reason, .. },
            ) => {
                assert_eq!(t0, "Hello, ");
                assert_eq!(t1, "world!");
                assert_eq!(stop_reason, "end_turn");
            }
            other => panic!("expected [Chunk, Chunk, Finished] in order, got {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app acp_session_service -j4`
Expected: FAIL with "no function or associated item named `new` found" (compile error — `AcpSessionService` has no methods yet).

- [ ] **Step 3: Implement `new`, `with_prompt_timeout`, `start_session`, `send_prompt`**

```rust
// crates/rocket-app/src/acp_session_service.rs (add above the tests module)

const DEFAULT_PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

impl AcpSessionService {
    pub fn new(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
    ) -> Self {
        Self::with_prompt_timeout(
            session_client,
            event_publisher,
            agent_config_service,
            DEFAULT_PROMPT_TIMEOUT,
        )
    }

    /// Test seam only — production wiring (Plan 05) always uses `new`, which
    /// fixes this at the spec's 120-second constant. This constructor does
    /// not add end-user configurability.
    pub fn with_prompt_timeout(
        session_client: Box<dyn AcpSessionClient>,
        event_publisher: Box<dyn EventPublisher>,
        agent_config_service: Arc<AgentConfigService>,
        prompt_timeout: Duration,
    ) -> Self {
        Self {
            session_client,
            event_publisher,
            agent_config_service,
            prompt_timeout,
        }
    }

    pub async fn start_session(&self, agent_config_id: &str, cwd: &str) -> DomainResult<String> {
        let config = self.agent_config_service.get(agent_config_id)?;
        let credential = self
            .agent_config_service
            .resolve_credential(agent_config_id)
            .await?;
        let env = vec![(config.credential_env_var.clone(), credential)];
        let session_id = self
            .session_client
            .start_session(&config.command, &config.args, cwd, &env)
            .await?;
        self.event_publisher.publish(DomainEvent::AcpSessionStarted {
            session_id: session_id.clone(),
        });
        Ok(session_id)
    }

    pub async fn send_prompt(&self, session_id: &str, prompt: String) -> DomainResult<String> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let session_id_owned = session_id.to_string();

        let drain_chunks = async {
            while let Some(text) = rx.recv().await {
                self.event_publisher.publish(DomainEvent::AcpSessionChunk {
                    session_id: session_id_owned.clone(),
                    text,
                });
            }
        };
        let send = self.session_client.send_prompt(session_id, prompt, tx);

        let joined = tokio::time::timeout(self.prompt_timeout, async {
            tokio::join!(drain_chunks, send)
        })
        .await;

        match joined {
            Ok((_, Ok(stop_reason))) => {
                self.event_publisher.publish(DomainEvent::AcpSessionFinished {
                    session_id: session_id.to_string(),
                    stop_reason: stop_reason.clone(),
                });
                Ok(stop_reason)
            }
            Ok((_, Err(e))) => {
                self.event_publisher.publish(DomainEvent::AcpSessionFailed {
                    session_id: session_id.to_string(),
                    error: e.to_string(),
                });
                Err(e)
            }
            Err(_elapsed) => {
                let _ = self.session_client.end_session(session_id).await;
                let message = format!(
                    "agent did not respond within {}s",
                    self.prompt_timeout.as_secs()
                );
                self.event_publisher.publish(DomainEvent::AcpSessionFailed {
                    session_id: session_id.to_string(),
                    error: message.clone(),
                });
                Err(DomainError::Internal(message))
            }
        }
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/rocket-app/src/lib.rs`, add alongside the existing `pub mod agent_config_service;`:

```rust
pub mod acp_session_service;
pub use acp_session_service::AcpSessionService;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-app acp_session_service -j4`
Expected: PASS — 2 new tests.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app
git commit -m "feat(app): add AcpSessionService start_session and send_prompt"
```

---

## Task 3: `end_session` and full error-mapping coverage

**Files:**
- Modify: `crates/rocket-app/src/acp_session_service.rs`

**Interfaces:**
- Produces: `AcpSessionService::end_session` — completes the interface Plan 05's Tauri commands consume.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/acp_session_service.rs (add to the existing tests module)

#[tokio::test]
async fn end_session_delegates_to_session_client() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let end_session_called = Arc::new(AtomicBool::new(false));
    let client = FakeSessionClient {
        end_session_called: Arc::clone(&end_session_called),
        ..Default::default()
    };
    let service = AcpSessionService::new(
        Box::new(client),
        Box::new(Arc::clone(&publisher)),
        agent_config_service(),
    );

    service
        .end_session("session-1")
        .await
        .expect("end_session should succeed");
    assert!(end_session_called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn start_session_unknown_agent_config_id_errors() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let service = AcpSessionService::new(
        Box::new(FakeSessionClient::default()),
        Box::new(Arc::clone(&publisher)),
        agent_config_service(),
    );

    let err = service
        .start_session("no-such-agent", "/tmp")
        .await
        .expect_err("unknown agent_config_id must error");
    assert!(matches!(err, DomainError::NotFound(_)));
    assert!(
        publisher.events.lock().expect("lock").is_empty(),
        "no event should publish when config resolution fails before any session starts"
    );
}

#[tokio::test]
async fn start_session_propagates_spawn_failure() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let client = FakeSessionClient {
        start_should_fail: true,
        ..Default::default()
    };
    let service = AcpSessionService::new(
        Box::new(client),
        Box::new(Arc::clone(&publisher)),
        agent_config_service(),
    );

    let err = service
        .start_session("agent-1", "/tmp")
        .await
        .expect_err("spawn failure must propagate");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[tokio::test]
async fn start_session_propagates_credential_resolution_failure_unchanged() {
    // Ok(None) simulates a stale/deleted vault secret — AgentConfigService
    // maps this to NotFound (proven by its own tests in subproject A);
    // this test's job is narrower: prove AcpSessionService::start_session
    // doesn't re-wrap or swallow that result on its way through.
    let publisher = Arc::new(FakeEventPublisher::new());
    let service = AcpSessionService::new(
        Box::new(FakeSessionClient::default()),
        Box::new(Arc::clone(&publisher)),
        agent_config_service_with(FakeVaultFetcher {
            secret_value_result: Ok(None),
        }),
    );

    let err = service
        .start_session("agent-1", "/tmp")
        .await
        .expect_err("a stale vault secret must fail start_session, not silently proceed");
    assert!(matches!(err, DomainError::NotFound(_)));
    assert!(
        publisher.events.lock().expect("lock").is_empty(),
        "no event should publish when credential resolution fails before any session starts"
    );
}

#[tokio::test]
async fn send_prompt_failure_publishes_failed_and_returns_the_error() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let client = FakeSessionClient {
        prompt_should_fail: true,
        ..Default::default()
    };
    let service = AcpSessionService::new(
        Box::new(client),
        Box::new(Arc::clone(&publisher)),
        agent_config_service(),
    );

    let err = service
        .send_prompt("session-1", "hi".to_string())
        .await
        .expect_err("a crashed/errored prompt must return an error");
    assert!(matches!(err, DomainError::Internal(_)));

    let events = publisher.events.lock().expect("lock");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })),
        "AcpSessionFailed must be published, got {events:?}"
    );
}

#[tokio::test]
async fn send_prompt_timeout_kills_the_session_and_publishes_failed() {
    let publisher = Arc::new(FakeEventPublisher::new());
    let end_session_called = Arc::new(AtomicBool::new(false));
    let client = FakeSessionClient {
        prompt_delay: Duration::from_millis(200),
        end_session_called: Arc::clone(&end_session_called),
        ..Default::default()
    };
    let service = AcpSessionService::with_prompt_timeout(
        Box::new(client),
        Box::new(Arc::clone(&publisher)),
        agent_config_service(),
        Duration::from_millis(20),
    );

    let err = service
        .send_prompt("session-1", "hi".to_string())
        .await
        .expect_err("a hung prompt must time out as an error");
    assert!(matches!(err, DomainError::Internal(_)));
    assert!(
        end_session_called.load(Ordering::SeqCst),
        "timeout must force-kill the session via end_session"
    );

    let events = publisher.events.lock().expect("lock");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DomainEvent::AcpSessionFailed { .. })),
        "AcpSessionFailed must be published on timeout, got {events:?}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app acp_session_service -j4`
Expected: FAIL — `end_session` doesn't exist yet (the other four tests in this step exercise `start_session`/`send_prompt` paths already implemented in Task 2, so only the `end_session`-calling test fails to compile; run the full module to confirm the others already pass at this point, which double-checks Task 2's work rather than duplicating it).

- [ ] **Step 3: Implement `end_session`**

```rust
// crates/rocket-app/src/acp_session_service.rs — add inside `impl AcpSessionService`

pub async fn end_session(&self, session_id: &str) -> DomainResult<()> {
    self.session_client.end_session(session_id).await
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app acp_session_service -j4`
Expected: PASS — 8 tests total (2 from Task 2, 6 from this task).

- [ ] **Step 5: Run the full crate suite**

Run: `cargo test -p rocket-app -j4`
Expected: PASS — confirms no regression to any other `rocket-app` test.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/acp_session_service.rs
git commit -m "feat(app): add AcpSessionService::end_session"
```

---

## Next Plan

[Plan 05: Tauri commands + event bus + wiring](2026-09-27-acp-transport-plan-05-tauri-commands.md) — exposes `AcpSessionService` over IPC, maps Plan 02's `DomainEvent` variants in `TauriEventBus`, and wires everything into `src-tauri/src/lib.rs`.

## Post-Implementation Review

Before starting Plan 05, dispatch a subagent (Agent tool, `subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-app/src/agent_config_service.rs`,
> `crates/rocket-app/src/acp_session_service.rs`,
> `crates/rocket-app/src/lib.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interface — does `AcpSessionService` expose
>    exactly `new`/`with_prompt_timeout`/`start_session`/`send_prompt`/
>    `end_session` as the plan index's locked interface contract promises
>    Plan 05 will consume?
> 2. Code quality and correctness versus this plan's Review Focus section —
>    credential-resolution failures propagate unchanged, the chunk-before-
>    finished ordering guarantee is actually exercised with more than one
>    chunk, the timeout path genuinely calls `end_session`, and a
>    `send_prompt` failure both publishes `AcpSessionFailed` and returns the
>    error.
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    trait-object injection throughout, no concrete `rocket-infra` type
>    referenced from this crate, no bare panicking shorthand in production
>    code paths.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-app -j4` and
> `cargo check -p rocket-app -j4`, and confirm they still pass. Report what
> you found and fixed.

Only proceed to Plan 05 once this review comes back clean (or its fixes are applied and re-verified).
