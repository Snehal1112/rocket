//! Lets an ACP agent act on a Rocket collection: list requests, run one,
//! edit a script, read/write a non-secret environment variable, and read
//! the last cached test results. Every method first re-checks the target
//! collection's `agent_autonomy_enabled` flag and refuses if it is off —
//! this is the safety valve described in the design spec, checked fresh on
//! every call so a mid-session toggle takes effect immediately. Every
//! successful call publishes `DomainEvent::AcpToolInvoked` for the audit
//! trail.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::execution_service::RequestExecutionService;
use crate::runner_sequence::{build_step_input, folder_dir_name, RunItem};

/// One request entry in a `list_collection_requests` result. `path` is
/// relative to the collection root, matching the shape `run_request` and
/// `edit_script` expect back.
///
/// `Serialize` (not just the domain-side `Debug`/`Clone`/`PartialEq`) is
/// needed because `src-tauri/src/mcp/tool_server.rs`'s `to_tool_result`
/// helper serializes a successful `McpToolService` result straight to the
/// MCP tool response's JSON text body.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct McpRequestEntry {
    pub path: String,
    pub name: String,
    pub method: String,
    pub url: String,
}

/// Summary of one `run_request` call, enough for an agent to decide what to
/// do next without re-fetching the full response body. `Serialize` for the
/// same reason as `McpRequestEntry` above.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct McpRunResult {
    pub status: u16,
    pub duration_ms: u64,
    pub test_pass_count: usize,
    pub test_fail_count: usize,
}

/// The single generic error returned by `get_env_var`/`set_env_var` for both
/// "no such key" and "key is secret". Keeping these indistinguishable stops
/// the tool from being an oracle for enumerating which env var names are
/// secret-flagged.
const VARIABLE_NOT_ACCESSIBLE: &str = "variable not accessible";

/// Orchestrates the 6 MCP tools an ACP agent can call against a collection.
/// Holds no process/filesystem state of its own — every method delegates to
/// an existing domain repository or service. `test_result_cache` is the one
/// piece of state this service owns: an in-memory map from
/// `(session_id, collection, request_path)` to the test results of that
/// triple's most recent `run_request` call, per the design spec's explicit
/// choice not to persist test results into `rocket-history`. The collection
/// is part of the key because two collections can hold the same relative
/// request path.
pub struct McpToolService {
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
    execution_svc: Arc<RequestExecutionService>,
    event_publisher: Arc<dyn EventPublisher>,
    /// Resolves `workspace.yml`'s `RequestGuardPolicy` at call time (never
    /// cached from construction time), so `run_request` honors a workspace's
    /// `block_script_redirects_to_internal_hosts`/`also_block_private_ranges`
    /// opt-in instead of always running with the fully-permissive default.
    config_repo: Box<dyn rocket_workspace::WorkspaceConfigRepository>,
    /// Shared with `SharedPathCollectionRepo`/`SharedPathFlowRepo`/
    /// `ReqwestExecutor::with_allowed_base` in `src-tauri`'s service graph —
    /// resolved fresh on every `run_request` call, the same
    /// "live workspace path" pattern `SharedPathCollectionRepo::repo()` uses,
    /// so a workspace switch takes effect immediately.
    active_workspace_path: Arc<Mutex<PathBuf>>,
    test_result_cache: Mutex<HashMap<TestResultKey, Vec<rocket_scripting::TestResult>>>,
}

/// `(session_id, collection, request_path)`.
type TestResultKey = (String, String, String);

fn test_result_key(session_id: &str, collection: &str, request_path: &str) -> TestResultKey {
    (
        session_id.to_string(),
        collection.to_string(),
        request_path.to_string(),
    )
}

impl McpToolService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
        execution_svc: Arc<RequestExecutionService>,
        event_publisher: Arc<dyn EventPublisher>,
        config_repo: Box<dyn rocket_workspace::WorkspaceConfigRepository>,
        active_workspace_path: Arc<Mutex<PathBuf>>,
    ) -> Self {
        Self {
            collection_repo,
            environment_repo_factory,
            execution_svc,
            event_publisher,
            config_repo,
            active_workspace_path,
            test_result_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Re-checks the opt-in flag for `collection`. Every public method calls
    /// this first, before doing anything else — including the read-only
    /// tools, per the design spec.
    fn check_autonomy_enabled(&self, collection: &str) -> DomainResult<()> {
        let settings = self.collection_repo.get_settings(collection)?;
        if !settings.agent_autonomy_enabled {
            return Err(DomainError::InvalidInput(format!(
                "the agent is not allowed to act on collection '{collection}' — \
                 enable \"Allow this agent to run requests and edit files\" in \
                 the chat panel first"
            )));
        }
        Ok(())
    }

    /// Rejects an environment name that could escape the collection's
    /// `environments/` directory. Mirrors the check in
    /// `src-tauri/src/commands/environments.rs::env_service_for`.
    fn validate_environment_name(name: &str) -> DomainResult<()> {
        if name.is_empty()
            || name.contains('\0')
            || name.starts_with('/')
            || name.starts_with('\\')
            || name.starts_with('.')
            || std::path::Path::new(name)
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        {
            return Err(DomainError::InvalidInput(
                "invalid environment name".to_string(),
            ));
        }
        Ok(())
    }

    fn publish_tool_invoked(&self, session_id: &str, tool: &str, summary: String) {
        self.event_publisher.publish(DomainEvent::AcpToolInvoked {
            session_id: session_id.to_string(),
            tool: tool.to_string(),
            summary,
        });
    }

    pub fn list_collection_requests(
        &self,
        session_id: &str,
        collection: &str,
    ) -> DomainResult<Vec<McpRequestEntry>> {
        self.check_autonomy_enabled(collection)?;
        let tree = self.collection_repo.get_summaries(collection)?;
        let mut entries = Vec::new();
        collect_request_entries(&tree.root, "", &mut entries);
        self.publish_tool_invoked(
            session_id,
            "list_collection_requests",
            format!("listed {} request(s) in '{collection}'", entries.len()),
        );
        Ok(entries)
    }

    pub async fn run_request(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
        environment_name: Option<&str>,
    ) -> DomainResult<McpRunResult> {
        self.check_autonomy_enabled(collection)?;
        if let Some(name) = environment_name {
            Self::validate_environment_name(name)?;
        }
        // Evict any stale cache entry for this key up front, before
        // dispatching the request. That way a failed run leaves no cached
        // result behind — `get_test_results` naturally falls back to its
        // "run the request first" `NotFound` instead of returning a prior
        // run's now-stale results.
        self.test_result_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&test_result_key(session_id, collection, request_path));
        let request = self.collection_repo.get_request(collection, request_path)?;
        let item = RunItem {
            name: request.name.clone(),
            request_path: request_path.to_string(),
            request,
        };
        // Resolved fresh on every call against the *current* active
        // workspace path — never cached from construction time — mirroring
        // `SharedPathCollectionRepo::repo()`'s own "resolve against the live
        // workspace path on every call" pattern, so a workspace switch (or a
        // mid-session `workspace.yml` edit) takes effect immediately, and so
        // agent-driven tool calls honor the same
        // `block_script_redirects_to_internal_hosts`/`also_block_private_ranges`
        // opt-in the Collection Runner's `RunCollectionInput` already does.
        let workspace_path = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let request_guard_policy = self.config_repo.load(&workspace_path)?.request_guard_policy;
        let input = build_step_input(
            &item,
            collection,
            environment_name,
            None,
            request_guard_policy,
            rocket_shared::RunSource::Agent,
        );
        // `execution_svc.execute` can fail with `DomainError::Http` whose
        // `Display` text embeds the fully-resolved request URL (or, for an
        // OAuth2 token-fetch failure, a response body) — either of which may
        // contain a resolved `secret: true` variable's value. That text is
        // safe for the human-facing "Send" button (see
        // `rocket-infra/src/reqwest_executor.rs`), but here it would flow
        // straight into the ACP agent's chat via `to_tool_result`, breaking
        // the "secret variables never appear in chat" guarantee for the
        // ordinary case of an unreachable host. Replace the message with
        // fixed text before it propagates; keep the `Http` variant so
        // callers matching on it still work. Other variants come from this
        // service's own validation and carry no response/URL content, so
        // they pass through unchanged.
        let output = match self.execution_svc.execute(input).await {
            Ok(output) => output,
            Err(DomainError::Http(_)) => {
                return Err(DomainError::Http(
                    "the request failed to complete — check Rocket's request history for details"
                        .to_string(),
                ));
            }
            Err(other) => return Err(other),
        };

        let test_pass_count = output
            .test_results
            .iter()
            .filter(|t| matches!(t.status, rocket_scripting::TestStatus::Passed))
            .count();
        let test_fail_count = output.test_results.len() - test_pass_count;

        self.test_result_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                test_result_key(session_id, collection, request_path),
                output.test_results.clone(),
            );

        self.publish_tool_invoked(
            session_id,
            "run_request",
            format!(
                "ran '{request_path}' in '{collection}' -> {} ({}ms)",
                output.response.status, output.response.duration_ms
            ),
        );

        Ok(McpRunResult {
            status: output.response.status,
            duration_ms: output.response.duration_ms,
            test_pass_count,
            test_fail_count,
        })
    }

    pub fn edit_script(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
        phase: rocket_collection::RequestScriptPhase,
        body: String,
    ) -> DomainResult<()> {
        self.check_autonomy_enabled(collection)?;
        let phase_name = match phase {
            rocket_collection::RequestScriptPhase::PreRequest => "pre-request",
            rocket_collection::RequestScriptPhase::PostResponse => "post-response",
            rocket_collection::RequestScriptPhase::Tests => "tests",
        };
        self.collection_repo
            .save_request_script(collection, request_path, phase, body)?;
        self.publish_tool_invoked(
            session_id,
            "edit_script",
            format!("updated the {phase_name} script on '{request_path}' in '{collection}'"),
        );
        Ok(())
    }

    pub fn get_env_var(
        &self,
        session_id: &str,
        collection: &str,
        environment_name: &str,
        key: &str,
    ) -> DomainResult<String> {
        self.check_autonomy_enabled(collection)?;
        Self::validate_environment_name(environment_name)?;
        let repo = self.environment_repo_factory.for_collection(collection);
        let env = repo.get(environment_name)?;
        let value = env
            .variables
            .iter()
            .find(|v| v.key == key && !v.secret)
            .map(|v| v.value.clone())
            .ok_or_else(|| DomainError::InvalidInput(VARIABLE_NOT_ACCESSIBLE.to_string()))?;
        self.publish_tool_invoked(
            session_id,
            "get_env_var",
            format!("read variable '{key}' from environment '{environment_name}'"),
        );
        Ok(value)
    }

    pub fn set_env_var(
        &self,
        session_id: &str,
        collection: &str,
        environment_name: &str,
        key: &str,
        value: String,
    ) -> DomainResult<()> {
        self.check_autonomy_enabled(collection)?;
        Self::validate_environment_name(environment_name)?;
        let repo = self.environment_repo_factory.for_collection(collection);
        let mut env = repo.get(environment_name)?;
        let variable = env
            .variables
            .iter_mut()
            .find(|v| v.key == key)
            .ok_or_else(|| DomainError::InvalidInput(VARIABLE_NOT_ACCESSIBLE.to_string()))?;
        if variable.secret {
            return Err(DomainError::InvalidInput(
                VARIABLE_NOT_ACCESSIBLE.to_string(),
            ));
        }
        variable.value = value;
        repo.save(&env)?;
        self.publish_tool_invoked(
            session_id,
            "set_env_var",
            format!("wrote variable '{key}' in environment '{environment_name}'"),
        );
        Ok(())
    }

    pub fn get_test_results(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<Vec<rocket_scripting::TestResult>> {
        self.check_autonomy_enabled(collection)?;
        let results = self
            .test_result_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&test_result_key(session_id, collection, request_path))
            .cloned()
            .ok_or_else(|| {
                DomainError::NotFound(format!(
                    "no cached test results for '{request_path}' in session '{session_id}' \
                     — run the request first"
                ))
            })?;
        self.publish_tool_invoked(
            session_id,
            "get_test_results",
            format!(
                "read {} cached test result(s) for '{request_path}'",
                results.len()
            ),
        );
        Ok(results)
    }

    /// Drops every `test_result_cache` entry belonging to `session_id`. Call
    /// this from wherever a session's `McpServerRegistry` entry is torn down
    /// (e.g. `end_agent_session`) — `test_result_cache` has no eviction of
    /// its own otherwise, and Plan 05 mints a fresh `session_id` per ACP
    /// session, so entries would otherwise accumulate in memory for the life
    /// of the process. A session that never ran an MCP tool call (e.g. agent
    /// autonomy was disabled the whole time) simply has nothing to remove —
    /// a no-op, not an error.
    pub fn forget_session(&self, session_id: &str) {
        self.test_result_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(sid, _, _), _| sid != session_id);
    }
}

/// Depth-first walk of a `get_summaries()` tree, collecting one
/// `McpRequestEntry` per `CollectionItem::Summary` leaf. Mirrors
/// `runner_sequence::collect_items`'s traversal, but over summary leaves
/// instead of full `Request` bodies — the two item shapes are different
/// enum variants (`Summary` vs `Request`), so this is a separate, small
/// walk rather than a shared generic one. The folder-path rule is shared via
/// `folder_dir_name`, so the paths listed here match the ones the runner uses.
fn collect_request_entries(
    folder: &rocket_collection::Folder,
    prefix: &str,
    out: &mut Vec<McpRequestEntry>,
) {
    for item in &folder.items {
        match item {
            rocket_collection::CollectionItem::Summary(summary) => {
                let Some(file_name) = summary.file_name.as_ref() else {
                    continue;
                };
                out.push(McpRequestEntry {
                    path: format!("{prefix}{file_name}"),
                    name: summary.name.clone(),
                    method: summary.method.clone(),
                    url: summary.url.clone(),
                });
            }
            rocket_collection::CollectionItem::Folder(sub) => {
                let sub_prefix = format!("{prefix}{}/", folder_dir_name(sub));
                collect_request_entries(sub, &sub_prefix, out);
            }
            rocket_collection::CollectionItem::Request(_)
            | rocket_collection::CollectionItem::OpaqueItem(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdHashMap;
    use std::path::Path;
    use std::sync::Mutex as StdMutex;

    use rocket_collection::{
        Collection, CollectionRepository, Folder, Request as CollectionRequest, RequestScriptPhase,
        RequestSummary,
    };
    use rocket_environment::{
        Environment, EnvironmentRepository, EnvironmentRepositoryFactory, Variable,
    };
    use rocket_shared::types::HttpMethod;
    use rocket_workspace::{RequestGuardPolicy, WorkspaceConfig, WorkspaceConfigRepository};

    use crate::test_doubles::{
        ConfigurableCollectionRepo, EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo,
        NullEnvRepo, RecordingExecutor, RecordingPublisher, SharedCollectionRepo, SharedExecutor,
        SharedHistoryRepo, SharedPublisher,
    };

    /// `WorkspaceConfigRepository` double that always reports a fixed
    /// `RequestGuardPolicy`, ignoring the requested path entirely — fine for
    /// every test in this file except the ones that specifically exercise
    /// policy resolution below, which build their own.
    struct FixedPolicyConfigRepo(RequestGuardPolicy);
    impl FixedPolicyConfigRepo {
        fn permissive() -> Box<dyn WorkspaceConfigRepository> {
            Box::new(Self(RequestGuardPolicy::default()))
        }
    }
    impl WorkspaceConfigRepository for FixedPolicyConfigRepo {
        fn load(&self, _workspace_path: &Path) -> DomainResult<WorkspaceConfig> {
            let mut config = WorkspaceConfig::new("test-workspace");
            config.request_guard_policy = self.0.clone();
            Ok(config)
        }
        fn save(&self, _workspace_path: &Path, _config: &WorkspaceConfig) -> DomainResult<()> {
            Ok(())
        }
        fn read_collection_name(&self, _collection_dir: &Path) -> DomainResult<Option<String>> {
            Ok(None)
        }
    }

    /// A placeholder active-workspace path. `FixedPolicyConfigRepo::load`
    /// ignores its argument entirely, so no real directory needs to exist at
    /// this path for these tests.
    fn dummy_workspace_path() -> Arc<StdMutex<PathBuf>> {
        Arc::new(StdMutex::new(PathBuf::from("/dummy-workspace")))
    }

    /// Environment repo factory double whose `for_collection` handles all
    /// share one underlying map, so a `set_env_var` write is visible to a
    /// later `get_env_var` call even though each call asks for a fresh
    /// `Box<dyn EnvironmentRepository>`.
    struct FakeEnvRepoFactory {
        envs: Arc<StdMutex<StdHashMap<String, Environment>>>,
    }
    impl FakeEnvRepoFactory {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                envs: Arc::new(StdMutex::new(StdHashMap::new())),
            })
        }
        fn with_env(&self, env: Environment) {
            self.envs
                .lock()
                .expect("lock")
                .insert(env.name.clone(), env);
        }
    }
    impl EnvironmentRepositoryFactory for FakeEnvRepoFactory {
        fn for_collection(&self, _collection: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(FakeEnvRepoHandle(Arc::clone(&self.envs)))
        }
    }
    struct FakeEnvRepoHandle(Arc<StdMutex<StdHashMap<String, Environment>>>);
    impl EnvironmentRepository for FakeEnvRepoHandle {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.lock().expect("lock").values().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, env: &Environment) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock")
                .insert(env.name.clone(), env.clone());
            Ok(())
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            self.0.lock().expect("lock").remove(name);
            Ok(())
        }
    }

    /// Builds an `McpToolService` plus its backing `RequestExecutionService`,
    /// sharing one `ConfigurableCollectionRepo` and one `RecordingPublisher` between
    /// them so a test can both drive HTTP dispatch and inspect every
    /// `DomainEvent` (including `AcpToolInvoked`) either service published.
    fn service_with(
        collection_repo: Arc<ConfigurableCollectionRepo>,
        env_factory: Arc<FakeEnvRepoFactory>,
        publisher: Arc<RecordingPublisher>,
    ) -> McpToolService {
        // Annotated as `Arc<dyn HttpExecutor>` at the binding, not via an
        // `as` cast (invalid Rust for `Arc`) or bare argument-position
        // coercion (`Arc::clone`'s generic `Self` is resolved from the
        // reference type before coercion applies, so it does not unify with
        // a `dyn` target at the call site) — matching this crate's existing
        // test style (e.g. `load_test_service.rs`'s
        // `let load_exec: Arc<dyn HttpExecutor> = Arc::new(...)`).
        let executor: Arc<dyn rocket_http::HttpExecutor> = RecordingExecutor::new();
        let history = InMemoryHistoryRepo::new();
        let exec_svc = Arc::new(RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(&executor),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedCollectionRepo(Arc::clone(&collection_repo))),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        ));
        McpToolService::new(
            collection_repo,
            env_factory,
            exec_svc,
            publisher,
            FixedPolicyConfigRepo::permissive(),
            dummy_workspace_path(),
        )
    }

    fn sample_request(name: &str) -> CollectionRequest {
        CollectionRequest::new(name, HttpMethod::Get, "https://api.test/ping")
    }

    /// Table-driven proof that all 6 tools refuse when autonomy is off — the
    /// Review Focus item this plan and the index both call out.
    #[test]
    fn every_tool_is_refused_when_autonomy_is_disabled() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        // A tree and a readable variable exist, so a missing gate would make
        // these calls succeed rather than fail for an unrelated reason.
        repo.with_summaries("my-api", Collection::new("my-api"));
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        let results: Vec<(&str, DomainResult<()>)> = vec![
            (
                "list_collection_requests",
                svc.list_collection_requests("s1", "my-api").map(|_| ()),
            ),
            (
                "edit_script",
                svc.edit_script(
                    "s1",
                    "my-api",
                    "login.yml",
                    RequestScriptPhase::Tests,
                    "// x".into(),
                ),
            ),
            (
                "get_env_var",
                svc.get_env_var("s1", "my-api", "dev", "HOST").map(|_| ()),
            ),
            (
                "set_env_var",
                svc.set_env_var("s1", "my-api", "dev", "HOST", "x".into()),
            ),
            (
                "get_test_results",
                svc.get_test_results("s1", "my-api", "login.yml")
                    .map(|_| ()),
            ),
        ];
        for (tool, result) in results {
            assert_refused_by_autonomy_gate(tool, result);
        }
        // run_request is async, so it is checked in its own test below.
        assert!(
            repo.saved_scripts().is_empty(),
            "a refused edit_script must not write"
        );
        assert!(
            !publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { .. })),
            "a refused call must not publish an audit event"
        );
    }

    /// Asserts `result` is the autonomy-gate refusal, not some other error.
    fn assert_refused_by_autonomy_gate(tool: &str, result: DomainResult<()>) {
        match result {
            Err(DomainError::InvalidInput(msg)) => assert!(
                msg.contains("not allowed to act on collection"),
                "{tool} failed, but not via the autonomy gate: {msg}"
            ),
            other => panic!("{tool} must be refused by the autonomy gate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_request_is_refused_when_autonomy_is_disabled() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let result = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .map(|_| ());
        assert_refused_by_autonomy_gate("run_request", result);
    }

    #[test]
    fn list_collection_requests_walks_folders_and_publishes_audit_event() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);

        let mut collection = Collection::new("my-api");
        collection.root.add_summary(RequestSummary {
            uid: "u1".into(),
            name: "Login".into(),
            method: "POST".into(),
            url: "https://api.test/login".into(),
            file_name: Some("login.yml".into()),
        });
        let mut auth = Folder::new("auth");
        auth.dir_name = Some("auth".into());
        auth.add_summary(RequestSummary {
            uid: "u2".into(),
            name: "Refresh".into(),
            method: "POST".into(),
            url: "https://api.test/refresh".into(),
            file_name: Some("refresh.yml".into()),
        });
        collection.root.add_subfolder(auth);
        repo.with_summaries("my-api", collection);

        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        let entries = svc
            .list_collection_requests("s1", "my-api")
            .expect("list_collection_requests");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "login.yml");
        assert_eq!(entries[1].path, "auth/refresh.yml");
        assert_eq!(entries[1].method, "POST");

        assert!(
            publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "list_collection_requests")),
            "expected an AcpToolInvoked event for list_collection_requests"
        );
    }

    #[tokio::test]
    async fn run_request_dispatches_tags_history_agent_and_caches_test_results() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();

        // As in `service_with` above, `executor` is bound with the `dyn`
        // type up front, since `Arc::clone`'s generic `Self` does not
        // unify with a `dyn` target at the call site (`as` does not perform
        // this coercion for `Arc` either).
        let executor: Arc<dyn rocket_http::HttpExecutor> = RecordingExecutor::new();
        let history = InMemoryHistoryRepo::new();
        let exec_svc = Arc::new(RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(&executor),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        ));
        // `repo`/`publisher` stay bound to their concrete types above (this
        // test still calls concrete-only methods on them, e.g. `.events()`
        // below), so the `dyn` coercion for `McpToolService::new` is done
        // here via separate explicitly-typed bindings instead. Method-call
        // syntax (`repo.clone()`), not the `Arc::clone(&repo)` associated-
        // function form, because method syntax fixes `Self` from the
        // receiver's own concrete type before coercing the result, whereas
        // `Arc::clone(&repo)` lets the `let`'s expected `dyn` type drive
        // `Self` resolution first and then fails to match `&repo`.
        let repo_dyn: Arc<dyn CollectionRepository> = repo.clone();
        let publisher_dyn: Arc<dyn EventPublisher> = publisher.clone();
        let svc = McpToolService::new(
            repo_dyn,
            env_factory,
            Arc::clone(&exec_svc),
            publisher_dyn,
            FixedPolicyConfigRepo::permissive(),
            dummy_workspace_path(),
        );

        let result = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .expect("run_request");

        assert_eq!(result.status, 200);
        assert_eq!(result.test_pass_count, 0);
        assert_eq!(result.test_fail_count, 0);

        let saved = history.entries.lock().expect("lock history");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].run_source, rocket_shared::RunSource::Agent);
        drop(saved);

        let cached = svc
            .get_test_results("s1", "my-api", "login.yml")
            .expect("get_test_results after a run must be cached, not an error");
        assert!(
            cached.is_empty(),
            "this fixture's request has no test script, so no results"
        );

        // The cache is scoped per collection: the same relative path in a
        // different collection has no cached results.
        repo.set_autonomy("other-api", true);
        let other = svc
            .get_test_results("s1", "other-api", "login.yml")
            .expect_err("another collection's same-named request was never run");
        assert!(matches!(other, DomainError::NotFound(_)));

        assert!(
            publisher.events().iter().any(
                |e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "run_request")
            ),
            "expected an AcpToolInvoked event for run_request"
        );
    }

    #[tokio::test]
    async fn a_failed_run_request_clears_the_previous_run_s_cached_test_results() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();

        // Bound with the `dyn` type up front (see the comment on the same
        // pattern in `service_with` above); kept as the concrete
        // `Arc<RecordingExecutor>` too, so this test can flip it to fail
        // between the two `run_request` calls below.
        let executor = RecordingExecutor::new();
        let executor_dyn: Arc<dyn rocket_http::HttpExecutor> =
            Arc::new(SharedExecutor(Arc::clone(&executor)));
        let history = InMemoryHistoryRepo::new();
        let exec_svc = Arc::new(RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(&executor_dyn),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        ));
        let repo_dyn: Arc<dyn CollectionRepository> = repo.clone();
        let publisher_dyn: Arc<dyn EventPublisher> = publisher.clone();
        let svc = McpToolService::new(
            repo_dyn,
            env_factory,
            Arc::clone(&exec_svc),
            publisher_dyn,
            FixedPolicyConfigRepo::permissive(),
            dummy_workspace_path(),
        );

        // First run succeeds and populates the cache.
        svc.run_request("s1", "my-api", "login.yml", None)
            .await
            .expect("first run_request must succeed");
        svc.get_test_results("s1", "my-api", "login.yml")
            .expect("cache must be populated after a successful run");

        // Make the same request's send fail with a genuine transport error,
        // then run it again.
        executor.set_status("api.test", 0);
        svc.run_request("s1", "my-api", "login.yml", None)
            .await
            .expect_err("second run_request must fail once the executor errors");

        // The stale first-run results must not still be served — the failed
        // run must have evicted them, not left them cached.
        let err = svc
            .get_test_results("s1", "my-api", "login.yml")
            .expect_err("a failed run must not leave a stale cached test result");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn run_request_sanitizes_an_http_error_instead_of_leaking_it_to_the_agent() {
        // I-2 regression test: `reqwest_executor.rs` can fail a send with a
        // `DomainError::Http` whose `Display` text embeds the fully-resolved
        // request URL (or, for OAuth2 token-fetch failures, a response
        // body) — either of which may contain a resolved `secret: true`
        // variable's value. `run_request` must never let that text reach
        // its caller (and, from there, the ACP agent's chat) verbatim.
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();

        let executor = RecordingExecutor::new();
        let executor_dyn: Arc<dyn rocket_http::HttpExecutor> =
            Arc::new(SharedExecutor(Arc::clone(&executor)));
        let history = InMemoryHistoryRepo::new();
        let exec_svc = Arc::new(RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(&executor_dyn),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedCollectionRepo(Arc::clone(&repo))),
            Box::new(NullCookieRepo),
            Box::new(SharedPublisher(Arc::clone(&publisher))),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        ));
        let repo_dyn: Arc<dyn CollectionRepository> = repo.clone();
        let publisher_dyn: Arc<dyn EventPublisher> = publisher.clone();
        let svc = McpToolService::new(
            repo_dyn,
            env_factory,
            Arc::clone(&exec_svc),
            publisher_dyn,
            FixedPolicyConfigRepo::permissive(),
            dummy_workspace_path(),
        );

        const SECRET: &str = "super-secret";
        executor.set_error(
            "api.test",
            &format!(
                "error sending request for url (https://api.test/ping?api_key={SECRET}): \
                 connection refused"
            ),
        );

        let err = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .expect_err("run_request must fail once the executor errors");

        assert!(matches!(err, DomainError::Http(_)));
        assert!(
            !err.to_string().contains(SECRET),
            "sanitized error must not contain the original error's secret-shaped content, got: {err}"
        );
    }

    #[test]
    fn edit_script_saves_via_the_repository_and_publishes_audit_event() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        svc.edit_script(
            "s1",
            "my-api",
            "login.yml",
            RequestScriptPhase::PostResponse,
            "rok.setEnvVar('token', res.body.token);".into(),
        )
        .expect("edit_script");

        let saved = repo.saved_scripts();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].0, "my-api");
        assert_eq!(saved[0].1, "login.yml");
        assert_eq!(saved[0].2, RequestScriptPhase::PostResponse);
        assert!(saved[0].3.contains("setEnvVar"));

        assert!(
            publisher.events().iter().any(
                |e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "edit_script")
            ),
            "expected an AcpToolInvoked event for edit_script"
        );
    }

    fn env_with_vars() -> Environment {
        let mut env = Environment::new("dev");
        env.set_variable(Variable {
            key: "HOST".into(),
            value: "api.example.com".into(),
            enabled: true,
            secret: false,
            description: None,
            value_variants: None,
            secret_type: None,
        });
        env.set_variable(Variable {
            key: "API_KEY".into(),
            value: "sk-live-abc".into(),
            enabled: true,
            secret: true,
            description: None,
            value_variants: None,
            secret_type: None,
        });
        env
    }

    #[test]
    fn get_env_var_reads_a_non_secret_variable() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let value = svc
            .get_env_var("s1", "my-api", "dev", "HOST")
            .expect("HOST is non-secret and must be readable");
        assert_eq!(value, "api.example.com");
    }

    #[test]
    fn get_env_var_not_found_and_is_secret_produce_the_identical_error_message() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let not_found = svc
            .get_env_var("s1", "my-api", "dev", "NO_SUCH_KEY")
            .expect_err("unknown key must error");
        let is_secret = svc
            .get_env_var("s1", "my-api", "dev", "API_KEY")
            .expect_err("secret key must error");

        assert_eq!(
            not_found.to_string(),
            is_secret.to_string(),
            "the two error messages must be indistinguishable"
        );

        // Case sensitivity: a differently-cased key is also just "not found",
        // not a secret-detection bypass or a distinct error shape.
        let wrong_case = svc
            .get_env_var("s1", "my-api", "dev", "host")
            .expect_err("key lookup is case-sensitive, so this must also be the same error");
        assert_eq!(wrong_case.to_string(), not_found.to_string());
    }

    #[test]
    fn set_env_var_writes_a_non_secret_variable_and_it_is_readable_back() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        svc.set_env_var("s1", "my-api", "dev", "HOST", "api2.example.com".into())
            .expect("set_env_var on a non-secret existing key");

        let value = svc
            .get_env_var("s1", "my-api", "dev", "HOST")
            .expect("read back");
        assert_eq!(value, "api2.example.com");

        assert!(
            publisher.events().iter().any(
                |e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "set_env_var")
            ),
            "expected an AcpToolInvoked event for set_env_var"
        );
    }

    #[test]
    fn set_env_var_refuses_a_secret_variable_and_does_not_create_missing_keys() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let secret_err = svc
            .set_env_var("s1", "my-api", "dev", "API_KEY", "sk-new".into())
            .expect_err("writing a secret variable must be refused");
        let missing_err = svc
            .set_env_var("s1", "my-api", "dev", "NO_SUCH_KEY", "x".into())
            .expect_err("writing an unknown key must be refused, not create it");

        assert_eq!(
            secret_err.to_string(),
            missing_err.to_string(),
            "the two error messages must be indistinguishable"
        );
    }

    #[test]
    fn get_test_results_errors_with_not_found_when_nothing_is_cached() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let err = svc
            .get_test_results("s1", "my-api", "login.yml")
            .expect_err("nothing has run yet in this session for this path");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn get_env_var_refuses_a_traversal_shaped_environment_name() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let err = svc
            .get_env_var("s1", "my-api", "../../other-api/environments/prod", "HOST")
            .expect_err("a traversal-shaped environment name must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn set_env_var_refuses_a_traversal_shaped_environment_name() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let err = svc
            .set_env_var(
                "s1",
                "my-api",
                "../../other-api/environments/prod",
                "HOST",
                "evil".into(),
            )
            .expect_err("a traversal-shaped environment name must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn run_request_refuses_a_traversal_shaped_environment_name() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let err = svc
            .run_request("s1", "my-api", "login.yml", Some("../evil"))
            .await
            .expect_err("a traversal-shaped environment name must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn disabling_autonomy_mid_session_blocks_the_very_next_call() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        // An empty tree so the autonomy-enabled first call below has
        // something to list — the assertion this test cares about is the
        // toggle behavior, not the walk itself.
        repo.with_summaries("my-api", Collection::new("my-api"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        svc.list_collection_requests("s1", "my-api")
            .expect("first call succeeds while autonomy is enabled");

        repo.set_autonomy("my-api", false);

        let err = svc
            .list_collection_requests("s1", "my-api")
            .expect_err("the very next call must be refused once autonomy is disabled");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn forget_session_evicts_only_that_sessions_cached_results() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        svc.run_request("session-a", "my-api", "login.yml", None)
            .await
            .expect("run_request for session-a");
        svc.run_request("session-b", "my-api", "login.yml", None)
            .await
            .expect("run_request for session-b");

        svc.forget_session("session-a");

        let forgotten = svc
            .get_test_results("session-a", "my-api", "login.yml")
            .expect_err("session-a's cached results must be gone after forget_session");
        assert!(matches!(forgotten, DomainError::NotFound(_)));
        svc.get_test_results("session-b", "my-api", "login.yml")
            .expect("session-b's cached results must survive forgetting a different session");
    }

    #[test]
    fn forget_session_on_a_session_with_nothing_cached_is_a_harmless_no_op() {
        let repo = ConfigurableCollectionRepo::new();
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(repo, env_factory, publisher);

        // Must not panic — a session that never ran an MCP tool call (e.g.
        // agent autonomy was disabled the whole time) still gets swept.
        svc.forget_session("never-existed");
    }

    #[tokio::test]
    async fn run_request_resolves_the_request_guard_policy_from_config_repo_at_call_time() {
        // Proof that `run_request` no longer hard-codes
        // `RequestGuardPolicy::default()` (Post-Plan-03 review caveat (d)):
        // a `WorkspaceConfigRepository` reporting the guard as enabled makes
        // a script's redirect to a blocked host fail the run, instead of the
        // permissive default silently allowing it through.
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let mut request = sample_request("Login");
        // A non-empty pre-request script is required for the before-request
        // phase (and therefore the guard check) to run at all.
        request.pre_request_script = Some("// pre".to_string());
        repo.with_request("my-api", "login.yml", request);
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();

        let engine = crate::test_doubles::ProgrammableEngine::new();
        engine.on(
            "Login",
            "before-request",
            rocket_scripting::ScriptResult {
                request_mutations: Some(rocket_scripting::RequestMutations {
                    url: Some("http://169.254.169.254/latest/meta-data/".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        let executor: Arc<dyn rocket_http::HttpExecutor> = RecordingExecutor::new();
        let history = InMemoryHistoryRepo::new();
        let exec_svc = Arc::new(
            RequestExecutionService::new(
                Box::new(NullEnvRepo),
                Arc::clone(&executor),
                Box::new(SharedHistoryRepo(Arc::clone(&history))),
                Box::new(SharedCollectionRepo(Arc::clone(&repo))),
                Box::new(NullCookieRepo),
                Box::new(SharedPublisher(Arc::clone(&publisher))),
                Box::new(EmptySecretManagerRepo),
                Arc::new(rocket_environment::NullSecretStore),
                Arc::new(rocket_environment::NullVaultSecretFetcher),
            )
            .with_script_engine(Box::new(crate::test_doubles::SharedEngine(engine))),
        );
        let repo_dyn: Arc<dyn CollectionRepository> = repo.clone();
        let publisher_dyn: Arc<dyn EventPublisher> = publisher.clone();
        let guarded_config_repo: Box<dyn WorkspaceConfigRepository> =
            Box::new(FixedPolicyConfigRepo(RequestGuardPolicy {
                block_script_redirects_to_internal_hosts: true,
                also_block_private_ranges: false,
            }));
        let svc = McpToolService::new(
            repo_dyn,
            env_factory,
            Arc::clone(&exec_svc),
            publisher_dyn,
            guarded_config_repo,
            dummy_workspace_path(),
        );

        let err = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .expect_err("a redirect to a blocked internal host must fail once the guard is on");
        assert!(matches!(err, DomainError::InvalidInput(_)), "got {err:?}");
    }
}
