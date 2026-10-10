//! End-to-end test for the workspace assistant's start wiring: a session gets
//! a real MCP HTTP server spawned and registered in `McpServerRegistry` under
//! the post-handshake session id (not the pre-handshake UUID the server was
//! tagged with), starts isolated in fresh scratch directories, records its
//! mode and outline, and a start failure after the HTTP server was bound
//! shuts that server down and removes the scratch directories.
//!
//! Drives `commands::acp_sessions::start_workspace_assistant_inner` directly
//! (generic over `R: tauri::Runtime`, so `tauri::test::MockRuntime` works
//! here) rather than the concretely-`AppHandle`-typed `#[tauri::command]`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_acp::{
    AcpSessionClient, AcpUpdate, AgentConfig, AgentConfigRepository, ConfigOption, McpServerSpec,
    PromptCapabilities, PromptPart, SessionInfo,
};
use rocket_app::{
    isolation_meta, AcpSessionService, AssistantMode, McpToolService, SessionCleanup,
    ISOLATION_ENV_CONFIG_DIR, WORKSPACE_ASSISTANT_INSTRUCTIONS,
};
use rocket_collection::CollectionRepository;
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_lib::agent_session::cleanup::{SessionResourceRegistry, TauriSessionCleanup};
use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_infra::{
    FsCollectionRepo, FsCookieRepo, FsEnvironmentRepo, FsHistoryRepo, FsSecretManagerRepo,
    FsWorkspaceConfigRepo, SharedCollectionEnvironmentRepo,
};
use rocket_lib::commands::acp_sessions::start_workspace_assistant_inner;
use rocket_lib::mcp::registry::McpServerRegistry;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::NullEventPublisher;
use rocket_environment::{NullSecretStore, NullVaultSecretFetcher};
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

#[derive(Clone)]
struct CapturedStart {
    cwd: String,
    env: Vec<(String, String)>,
    meta: Option<serde_json::Value>,
}

struct FakeSessionClient {
    should_fail: bool,
    real_session_id: String,
    captured_servers: Arc<Mutex<Vec<McpServerSpec>>>,
    captured_start: Arc<Mutex<Option<CapturedStart>>>,
}
#[async_trait::async_trait]
impl AcpSessionClient for FakeSessionClient {
    async fn start_session(
        &self,
        _command: &str,
        _args: &[String],
        cwd: &str,
        env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
        meta: Option<serde_json::Value>,
    ) -> DomainResult<SessionInfo> {
        *self.captured_start.lock().expect("lock") = Some(CapturedStart {
            cwd: cwd.to_string(),
            env: env.to_vec(),
            meta,
        });
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

struct FakeHttpExecutor;

#[async_trait::async_trait]
impl HttpExecutor for FakeHttpExecutor {
    async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
        Ok(HttpResponse::default())
    }
}

struct Fixture {
    mcp_tool_svc: McpToolService,
    registry: Arc<McpServerRegistry>,
    resources: Arc<SessionResourceRegistry>,
    acp_session_svc: AcpSessionService,
    app_handle: tauri::AppHandle<tauri::test::MockRuntime>,
    _tmp: TempDir,
}

/// Builds an `McpToolService` over an empty temp workspace, a fresh
/// `McpServerRegistry`, and an `AcpSessionService` wired to `client`. The
/// `TempDir` is kept alive for the workspace files, and the mock
/// `AppHandle` is what `spawn_mcp_http_server` needs.
fn build_fixture(client: FakeSessionClient) -> Fixture {
    let tmp = TempDir::new().expect("tempdir");
    let ws_path = Arc::new(Mutex::new(tmp.path().to_path_buf()));
    let collections_dir = tmp.path().join("collections");
    FsCollectionRepo::new_standalone(collections_dir.clone())
        .create("demo")
        .expect("create collection");

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
    let mcp_tool_svc = McpToolService::new(
        Arc::new(FsCollectionRepo::new_standalone(collections_dir.clone())),
        Arc::new(SharedCollectionEnvironmentRepo::new(Arc::clone(&ws_path))),
        Arc::new(exec_svc),
        Arc::new(NullEventPublisher),
        Box::new(FsWorkspaceConfigRepo::new()),
        Arc::clone(&ws_path),
        Box::new(FsHistoryRepo::new(tmp.path().join("history"))),
    );

    let collection_repo: Arc<dyn CollectionRepository> =
        Arc::new(FsCollectionRepo::new_standalone(collections_dir));
    let acp_session_svc = AcpSessionService::new(
        Box::new(client),
        Box::new(NullEventPublisher),
        Arc::new(rocket_app::NoopSessionCleanup),
        agent_config_service(),
        collection_repo,
    );

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build mock tauri app");
    Fixture {
        mcp_tool_svc,
        registry: Arc::new(McpServerRegistry::new()),
        resources: Arc::new(SessionResourceRegistry::new()),
        acp_session_svc,
        app_handle: app.handle().clone(),
        _tmp: tmp,
    }
}

type ClientParts = (
    FakeSessionClient,
    Arc<Mutex<Vec<McpServerSpec>>>,
    Arc<Mutex<Option<CapturedStart>>>,
);

fn client(should_fail: bool, session_id: &str) -> ClientParts {
    let servers = Arc::new(Mutex::new(Vec::new()));
    let start = Arc::new(Mutex::new(None));
    let client = FakeSessionClient {
        should_fail,
        real_session_id: session_id.to_string(),
        captured_servers: Arc::clone(&servers),
        captured_start: Arc::clone(&start),
    };
    (client, servers, start)
}

async fn start(fx: &Fixture) -> DomainResult<rocket_lib::commands::acp_session_dto::AgentSessionStartedDto> {
    start_workspace_assistant_inner(
        "agent-1".to_string(),
        AssistantMode::Edit,
        None,
        fx.app_handle.clone(),
        &fx.registry,
        &fx.mcp_tool_svc,
        &fx.resources,
        &fx.acp_session_svc,
    )
    .await
}

#[tokio::test]
async fn session_spawns_and_registers_the_mcp_server_under_the_real_session_id() {
    let (client, servers, _) = client(false, "acp-real-session-1");
    let fx = build_fixture(client);

    let session_id = start(&fx)
        .await
        .expect("start_workspace_assistant_inner should succeed")
        .session_id;

    assert_eq!(session_id, "acp-real-session-1");
    // The mode and the outline are recorded under the real id too.
    assert_eq!(fx.mcp_tool_svc.mode(&session_id), AssistantMode::Edit);
    assert!(fx.mcp_tool_svc.peek_outline_preamble(&session_id).is_some());
    // The real post-handshake id is what the registry is keyed by, never the
    // pre-handshake UUID minted internally.
    fx.registry.end_session(&session_id);

    let servers = servers.lock().expect("lock").clone();
    assert_eq!(
        servers.len(),
        2,
        "the tool server must offer both Http and Stdio specs, got {servers:?}"
    );
    assert!(servers
        .iter()
        .any(|s| matches!(s, McpServerSpec::Http { .. })));
    assert!(servers
        .iter()
        .any(|s| matches!(s, McpServerSpec::Stdio { .. })));
}

#[tokio::test]
async fn session_start_failure_propagates_and_registers_nothing() {
    let (client, _, _) = client(true, "unused");
    let fx = build_fixture(client);

    let err = start(&fx)
        .await
        .expect_err("a session-start failure must propagate as an error");
    assert!(matches!(err, DomainError::Internal(_)));

    // The HTTP server was already bound, so the function must have shut it
    // down itself. It was never registered, as no real session id existed.
    assert!(fx.resources.is_empty());
}

#[tokio::test]
async fn session_start_isolates_the_agent_in_fresh_scratch_directories() {
    let (client, _, captured_start) = client(false, "acp-real-session-4");
    let fx = build_fixture(client);

    start(&fx)
        .await
        .expect("start_workspace_assistant_inner should succeed");

    let start = captured_start
        .lock()
        .expect("lock")
        .clone()
        .expect("the client's start_session must have been called");
    let cwd = PathBuf::from(&start.cwd);
    assert!(cwd.is_dir());
    assert_eq!(std::fs::read_dir(&cwd).expect("read cwd").count(), 0);
    let config_dir = start
        .env
        .iter()
        .find(|(k, _)| k == ISOLATION_ENV_CONFIG_DIR)
        .map(|(_, v)| PathBuf::from(v))
        .expect("CLAUDE_CONFIG_DIR must be set");
    assert!(config_dir.is_dir());
    assert_ne!(config_dir, cwd);
    assert!(start.env.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"));
    assert_eq!(
        start.meta,
        Some(isolation_meta(WORKSPACE_ASSISTANT_INSTRUCTIONS))
    );
    assert_eq!(fx.resources.len(), 1);

    let cleanup = TauriSessionCleanup::with_cache_forgetter(
        Arc::clone(&fx.registry),
        Arc::clone(&fx.resources),
        |_| {},
    );
    cleanup.on_session_ended("acp-real-session-4");

    assert!(!cwd.exists());
    assert!(!config_dir.exists());
    assert!(fx.resources.is_empty());
}

#[tokio::test]
async fn session_start_failure_removes_the_scratch_directories() {
    let (client, _, captured_start) = client(true, "unused");
    let fx = build_fixture(client);

    start(&fx)
        .await
        .expect_err("a session-start failure must propagate as an error");

    let start = captured_start
        .lock()
        .expect("lock")
        .clone()
        .expect("the client's start_session must have been called");
    assert!(
        !PathBuf::from(&start.cwd).exists(),
        "the scratch cwd must be removed"
    );
    let config_dir = start
        .env
        .iter()
        .find(|(k, _)| k == ISOLATION_ENV_CONFIG_DIR)
        .map(|(_, v)| PathBuf::from(v))
        .expect("CLAUDE_CONFIG_DIR must be set");
    assert!(!config_dir.exists(), "the scratch config dir must be removed");
    assert!(fx.resources.is_empty());
}
