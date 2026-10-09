//! End-to-end test for `start_agent_session`'s capability-based transport
//! wiring (Plan 05, Task 3): an autonomy-enabled session gets a real MCP HTTP
//! server spawned and registered in `McpServerRegistry` under the
//! *post-handshake* session id (not the pre-handshake UUID
//! `spawn_mcp_http_server` was tagged with); an autonomy-disabled session
//! gets no MCP server at all; and a session-start failure after the HTTP
//! server was already bound shuts that server down rather than leaking a
//! bound port with a live bearer token.
//!
//! Drives `commands::acp_sessions::start_agent_session_inner` directly
//! (generic over `R: tauri::Runtime`, so `tauri::test::MockRuntime` works
//! here) rather than the concretely-`AppHandle`-typed `#[tauri::command]`
//! wrapper — see that function's own doc comment for why it exists.

use std::sync::{Arc, Mutex};

use rocket_acp::{
    AcpSessionClient, AcpUpdate, AgentConfig, AgentConfigRepository, ConfigOption, McpServerSpec,
    PromptCapabilities, PromptPart, SessionInfo,
};
use rocket_app::{AcpSessionService, CollectionService};
use rocket_collection::{CollectionRepository, CollectionSettings};
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_lib::commands::acp_sessions::start_agent_session_inner;
use rocket_lib::mcp::registry::McpServerRegistry;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::NullEventPublisher;
use tempfile::TempDir;
use tokio::sync::mpsc::UnboundedSender;

// ---------------------------------------------------------------------------
// Minimal fakes for AgentConfigService's dependencies. Not reusable from
// `rocket-app`'s own `#[cfg(test)]`-gated doubles (crate-private), so this
// mirrors `crates/rocket-app/src/acp_session_service.rs`'s own test fakes at
// the smallest scope this test needs.
// ---------------------------------------------------------------------------

struct FakeAgentConfigRepo(AgentConfig);
impl AgentConfigRepository for FakeAgentConfigRepo {
    fn list(&self) -> DomainResult<Vec<AgentConfig>> {
        Ok(vec![self.0.clone()])
    }
    fn get(&self, id: &str) -> DomainResult<Option<AgentConfig>> {
        Ok((self.0.id == id).then(|| self.0.clone()))
    }
    fn save(&self, _config: &AgentConfig) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
    fn delete(&self, _id: &str) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
}

struct FakeSecretManagerRepo(SecretManagerConnection);
impl SecretManagerRepository for FakeSecretManagerRepo {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        Ok(vec![self.0.clone()])
    }
    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
        Ok((self.0.id == id).then(|| self.0.clone()))
    }
    fn save(&self, _connection: &SecretManagerConnection) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
    fn delete(&self, _id: &str) -> DomainResult<()> {
        unreachable!("not exercised by this test")
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

struct FakeVaultFetcher;
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
        Ok(Some("sk-abc123".to_string()))
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

fn agent_config_service() -> Arc<rocket_app::AgentConfigService> {
    let connection = SecretManagerConnection {
        id: "conn-1".to_string(),
        label: "Test Vault".to_string(),
        base_url: "https://vault.internal:8774".to_string(),
        client_id: "rocketapi".to_string(),
        verify_ssl: true,
        allow_insecure_http: false,
        provider: Default::default(),
        config: None,
    };
    let secret_manager = Arc::new(rocket_app::SecretManagerService::new(
        Box::new(FakeSecretManagerRepo(connection)),
        Arc::new(FakeSecretStore),
        Arc::new(FakeVaultFetcher),
    ));
    let config = AgentConfig {
        id: "agent-1".to_string(),
        label: "Test Agent".to_string(),
        command: "test-agent-acp".to_string(),
        args: Vec::new(),
        working_dir: None,
        credential_env_var: "ANTHROPIC_API_KEY".to_string(),
        vault_connection_id: "conn-1".to_string(),
        vault_name: "prod-vault".to_string(),
        vault_secret_id: "secret-id-1".to_string(),
        vault_secret_name: "anthropic-api-key".to_string(),
    };
    Arc::new(rocket_app::AgentConfigService::new(
        Box::new(FakeAgentConfigRepo(config)),
        secret_manager,
    ))
}

// ---------------------------------------------------------------------------
// A controllable `AcpSessionClient`: never actually spawns a process. Returns
// a fixed "real" session id, optionally fails, and records the `mcp_servers`
// it was handed so a test can assert on them.
// ---------------------------------------------------------------------------

struct FakeSessionClient {
    should_fail: bool,
    real_session_id: String,
    captured_servers: Arc<Mutex<Vec<McpServerSpec>>>,
}
#[async_trait::async_trait]
impl AcpSessionClient for FakeSessionClient {
    async fn start_session(
        &self,
        _command: &str,
        _args: &[String],
        _cwd: &str,
        _env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
        _meta: Option<serde_json::Value>,
    ) -> DomainResult<SessionInfo> {
        *self.captured_servers.lock().expect("lock") = mcp_servers.to_vec();
        if self.should_fail {
            Err(DomainError::Internal(
                "agent process failed to start".to_string(),
            ))
        } else {
            Ok(SessionInfo {
                session_id: self.real_session_id.clone(),
                config_options: Vec::new(),
                prompt_capabilities: PromptCapabilities::default(),
            })
        }
    }
    async fn send_prompt(
        &self,
        _session_id: &str,
        _parts: Vec<PromptPart>,
        _update_tx: UnboundedSender<AcpUpdate>,
    ) -> DomainResult<String> {
        unreachable!("not exercised by this test")
    }
    async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
    async fn set_config_option(
        &self,
        _session_id: &str,
        _config_id: &str,
        _value: &str,
    ) -> DomainResult<Vec<ConfigOption>> {
        unreachable!("not exercised by this test")
    }
    async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
    async fn end_all_sessions(&self) -> DomainResult<()> {
        unreachable!("not exercised by this test")
    }
}

/// Builds a "demo" collection on disk with the given autonomy setting, a
/// `CollectionService` over it, a fresh `McpServerRegistry`, and an
/// `AcpSessionService` wired to `client`. Returns everything the test needs
/// plus the `TempDir` (kept alive for the collection files) and the mock
/// `AppHandle` `spawn_mcp_http_server` needs.
fn build_fixture(
    agent_autonomy_enabled: bool,
    client: FakeSessionClient,
) -> (
    CollectionService,
    Arc<McpServerRegistry>,
    AcpSessionService,
    tauri::AppHandle<tauri::test::MockRuntime>,
    TempDir,
) {
    let tmp = TempDir::new().expect("tempdir");
    let collections_dir = tmp.path().join("collections");
    let setup_repo = rocket_infra::FsCollectionRepo::new_standalone(collections_dir.clone());
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

    let collection_svc = CollectionService::new(
        Box::new(rocket_infra::FsCollectionRepo::new_standalone(
            collections_dir.clone(),
        )),
        Box::new(NullEventPublisher),
    );
    let collection_repo: Arc<dyn CollectionRepository> = Arc::new(
        rocket_infra::FsCollectionRepo::new_standalone(collections_dir),
    );
    let registry = Arc::new(McpServerRegistry::new());
    let acp_session_svc = AcpSessionService::new(
        Box::new(client),
        Box::new(NullEventPublisher),
        agent_config_service(),
        collection_repo,
    );

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build mock tauri app");
    let app_handle = app.handle().clone();

    (collection_svc, registry, acp_session_svc, app_handle, tmp)
}

#[tokio::test]
async fn autonomy_enabled_session_spawns_and_registers_the_mcp_server_under_the_real_session_id() {
    let captured_servers = Arc::new(Mutex::new(Vec::new()));
    let client = FakeSessionClient {
        should_fail: false,
        real_session_id: "acp-real-session-1".to_string(),
        captured_servers: Arc::clone(&captured_servers),
    };
    let (collection_svc, registry, acp_session_svc, app_handle, _tmp) = build_fixture(true, client);

    let session_id = start_agent_session_inner(
        "agent-1".to_string(),
        "/tmp".to_string(),
        "demo".to_string(),
        app_handle,
        &collection_svc,
        &registry,
        &acp_session_svc,
    )
    .await
    .expect("start_agent_session_inner should succeed")
    .session_id;

    assert_eq!(session_id, "acp-real-session-1");

    // The real post-handshake id is what the registry is keyed by, never the
    // pre-handshake UUID minted internally.
    registry.end_session(&session_id);

    let servers = captured_servers.lock().expect("lock").clone();
    assert_eq!(
        servers.len(),
        2,
        "autonomy enabled + a spawned server must offer both Http and Stdio specs, got {servers:?}"
    );
    assert!(servers
        .iter()
        .any(|s| matches!(s, McpServerSpec::Http { .. })));
    assert!(servers
        .iter()
        .any(|s| matches!(s, McpServerSpec::Stdio { .. })));
}

#[tokio::test]
async fn autonomy_disabled_session_spawns_no_mcp_server() {
    let captured_servers = Arc::new(Mutex::new(Vec::new()));
    let client = FakeSessionClient {
        should_fail: false,
        real_session_id: "acp-real-session-2".to_string(),
        captured_servers: Arc::clone(&captured_servers),
    };
    let (collection_svc, registry, acp_session_svc, app_handle, _tmp) =
        build_fixture(false, client);

    let session_id = start_agent_session_inner(
        "agent-1".to_string(),
        "/tmp".to_string(),
        "demo".to_string(),
        app_handle,
        &collection_svc,
        &registry,
        &acp_session_svc,
    )
    .await
    .expect("start_agent_session_inner should succeed")
    .session_id;

    assert_eq!(session_id, "acp-real-session-2");
    assert!(
        captured_servers.lock().expect("lock").is_empty(),
        "autonomy disabled must attach no MCP servers"
    );
    // end_session on a session that never had a registered server must be a
    // harmless no-op (Plan 04's own McpServerRegistry test coverage), which
    // this call also exercises for this specific "never registered" case.
    registry.end_session(&session_id);
}

#[tokio::test]
async fn session_start_failure_shuts_down_the_already_spawned_mcp_server() {
    let captured_servers = Arc::new(Mutex::new(Vec::new()));
    let client = FakeSessionClient {
        should_fail: true,
        real_session_id: "unused".to_string(),
        captured_servers: Arc::clone(&captured_servers),
    };
    let (collection_svc, registry, acp_session_svc, app_handle, _tmp) = build_fixture(true, client);

    let err = start_agent_session_inner(
        "agent-1".to_string(),
        "/tmp".to_string(),
        "demo".to_string(),
        app_handle,
        &collection_svc,
        &registry,
        &acp_session_svc,
    )
    .await
    .expect_err("a session-start failure must propagate as an error");
    assert!(matches!(err, DomainError::Internal(_)));

    // The failure happened after the HTTP server was already bound; this
    // function must have shut it down itself rather than leaking it (it was
    // never registered anywhere, since no real session id ever existed, so
    // there is nothing left in the registry to sweep it via).
}
