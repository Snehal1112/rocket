//! Lets the workspace assistant (an ACP agent) read and act on the active
//! workspace through MCP tools.
//!
//! Scope: every method that takes a `collection` first checks that it is
//! one of the active workspace's collections (`check_in_workspace`), so a
//! name from another workspace, a traversal-shaped name or a case variant
//! is refused before anything is read or written.
//!
//! Read tools (`get_workspace_outline`, `list_collections`, `get_request`,
//! `get_collection_settings`, `get_environment`, `get_history`,
//! `get_test_results`) are always allowed and return masked views from
//! `mcp_read_views`. `run_request`, and the direct-write tools until Plan
//! 04 removes them, also need the collection's run switch
//! (`agent_autonomy_enabled`, "Allow the agent to run requests in this
//! collection"), re-checked on every call so a mid-session toggle takes
//! effect at once. Every successful call publishes
//! `DomainEvent::AcpToolInvoked` for the audit trail.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::execution_service::RequestExecutionService;
use crate::mcp_read_views::{
    filter_folder, history_limit, literal_credential_values, mask_response_body,
    normalize_folder, outline_entries, render_outline, CollectionBrief, HistoryBrief,
    MaskedEnvironment, MaskedRequest, MaskedSettings, OutlineCollection,
};
use crate::runner_sequence::{build_step_input, RunItem};

/// Summary of one `run_request` call. `Serialize` because
/// `src-tauri/src/mcp/tool_server.rs`'s `to_tool_result` serializes a
/// successful result straight into the MCP tool response.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct McpRunResult {
    pub status: u16,
    pub duration_ms: u64,
    pub test_pass_count: usize,
    pub test_fail_count: usize,
    /// The response body with the collection's and the chosen
    /// environment's secret values masked, then cut to
    /// `RESPONSE_BODY_CAP_BYTES`.
    pub body: String,
    /// Whether `body` was cut.
    pub body_truncated: bool,
}

/// The single generic error `set_env_var` returns for both "no such key"
/// and "key is secret", so the tool cannot be used to find out which names
/// are secret.
const VARIABLE_NOT_ACCESSIBLE: &str = "variable not accessible";

/// Orchestrates the workspace assistant's MCP tools. Holds no I/O of its
/// own: every read and write goes through an injected repository or
/// service. `test_result_cache` is the one piece of state it owns, keyed by
/// `(session_id, collection, request_path)`, because the design keeps test
/// results out of `rocket-history`.
pub struct McpToolService {
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
    execution_svc: Arc<RequestExecutionService>,
    event_publisher: Arc<dyn EventPublisher>,
    /// Resolves `workspace.yml`'s `RequestGuardPolicy` at call time, so
    /// `run_request` honors the workspace's SSRF opt-ins.
    config_repo: Box<dyn rocket_workspace::WorkspaceConfigRepository>,
    /// The live active workspace path, read on every `run_request`.
    active_workspace_path: Arc<Mutex<PathBuf>>,
    /// Read by `get_history`. The same store `RequestExecutionService`
    /// writes to, so an agent run shows up here.
    history_repo: Box<dyn rocket_history::HistoryRepository>,
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
        history_repo: Box<dyn rocket_history::HistoryRepository>,
    ) -> Self {
        Self {
            collection_repo,
            environment_repo_factory,
            execution_svc,
            event_publisher,
            config_repo,
            active_workspace_path,
            history_repo,
            test_result_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Re-checks the collection's run switch. `run_request` calls it on
    /// every call; `edit_script` and `set_env_var` keep calling it until
    /// Plan 04 replaces them with proposals. Read tools never call it.
    fn check_autonomy_enabled(&self, collection: &str) -> DomainResult<()> {
        let settings = self.collection_repo.get_settings(collection)?;
        if !settings.agent_autonomy_enabled {
            return Err(DomainError::InvalidInput(format!(
                "the agent is not allowed to run requests in collection '{collection}'. \
                 Turn on \"Allow the agent to run requests in this collection\" first"
            )));
        }
        Ok(())
    }

    /// Refuses a collection that is not one of the active workspace's
    /// collections. The list is read fresh on every call, so a workspace
    /// switch takes effect at once. Only an exact name matches.
    fn check_in_workspace(&self, collection: &str) -> DomainResult<()> {
        let in_workspace = self
            .collection_repo
            .list()?
            .iter()
            .any(|summary| summary.name == collection);
        if in_workspace {
            Ok(())
        } else {
            Err(DomainError::NotFound(format!(
                "collection '{collection}' is not in the current workspace"
            )))
        }
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

    /// Reads a request for the agent. A path under `environments/` is
    /// refused, and a read or parse failure is reported with fixed text,
    /// because the raw YAML error can quote file content.
    fn read_request(
        &self,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<rocket_collection::Request> {
        let under_environments = request_path
            .split(['/', '\\'])
            .find(|segment| !segment.is_empty() && *segment != ".")
            .is_some_and(|segment| segment.eq_ignore_ascii_case("environments"));
        if under_environments {
            return Err(DomainError::InvalidInput(
                "environments are not requests; use get_environment".to_string(),
            ));
        }
        match self.collection_repo.get_request(collection, request_path) {
            Err(DomainError::Internal(_)) => Err(DomainError::InvalidInput(format!(
                "request '{request_path}' could not be read"
            ))),
            other => other,
        }
    }

    fn publish_tool_invoked(&self, session_id: &str, tool: &str, summary: String) {
        self.event_publisher.publish(DomainEvent::AcpToolInvoked {
            session_id: session_id.to_string(),
            tool: tool.to_string(),
            summary,
        });
    }

    /// The compact workspace index (spec section 6). With no `collection`,
    /// every collection in the workspace; with one, only that collection,
    /// optionally under `folder`.
    pub fn get_workspace_outline(
        &self,
        session_id: &str,
        collection: Option<&str>,
        folder: Option<&str>,
    ) -> DomainResult<String> {
        let folder = match folder {
            Some(raw) => normalize_folder(raw)?,
            None => None,
        };
        let names: Vec<String> = match collection {
            Some(name) => {
                self.check_in_workspace(name)?;
                vec![name.to_string()]
            }
            None => {
                if folder.is_some() {
                    return Err(DomainError::InvalidInput(
                        "a folder filter needs a collection".to_string(),
                    ));
                }
                self.collection_repo
                    .list()?
                    .into_iter()
                    .map(|summary| summary.name)
                    .collect()
            }
        };
        let collections: Vec<OutlineCollection> = names
            .into_iter()
            .map(|name| self.outline_collection(name, folder.as_deref()))
            .collect();
        let text = render_outline(&collections);
        self.publish_tool_invoked(
            session_id,
            "get_workspace_outline",
            format!("read the outline of {} collection(s)", collections.len()),
        );
        Ok(text)
    }

    /// One collection's outline section. A collection whose tree cannot be
    /// read is listed as unreadable instead of failing the whole outline.
    fn outline_collection(&self, name: String, folder: Option<&str>) -> OutlineCollection {
        let run_allowed = self
            .collection_repo
            .get_settings(&name)
            .map(|settings| settings.agent_autonomy_enabled)
            .unwrap_or(false);
        match self.collection_repo.get_summaries(&name) {
            Ok(tree) => {
                let entries = outline_entries(&tree.root);
                let entries = match folder {
                    Some(f) => filter_folder(entries, f),
                    None => entries,
                };
                OutlineCollection {
                    name,
                    run_allowed,
                    readable: true,
                    entries,
                }
            }
            Err(_) => OutlineCollection {
                name,
                run_allowed,
                readable: false,
                entries: Vec::new(),
            },
        }
    }

    pub fn list_collections(&self, session_id: &str) -> DomainResult<Vec<CollectionBrief>> {
        let briefs: Vec<CollectionBrief> = self
            .collection_repo
            .list()?
            .into_iter()
            .map(|summary| {
                let run_allowed = self
                    .collection_repo
                    .get_settings(&summary.name)
                    .map(|settings| settings.agent_autonomy_enabled)
                    .unwrap_or(false);
                let mut environments: Vec<String> = self
                    .environment_repo_factory
                    .for_collection(&summary.name)
                    .list()
                    .map(|envs| envs.into_iter().map(|env| env.name).collect())
                    .unwrap_or_default();
                environments.sort();
                CollectionBrief {
                    name: summary.name,
                    request_count: summary.request_count,
                    run_allowed,
                    environments,
                }
            })
            .collect();
        self.publish_tool_invoked(
            session_id,
            "list_collections",
            format!("listed {} collection(s)", briefs.len()),
        );
        Ok(briefs)
    }

    pub fn get_request(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
    ) -> DomainResult<MaskedRequest> {
        self.check_in_workspace(collection)?;
        let request = self.read_request(collection, request_path)?;
        let view = MaskedRequest::from_request(request_path, &request);
        self.publish_tool_invoked(
            session_id,
            "get_request",
            format!("read request '{request_path}' in '{collection}'"),
        );
        Ok(view)
    }

    pub fn get_collection_settings(
        &self,
        session_id: &str,
        collection: &str,
    ) -> DomainResult<MaskedSettings> {
        self.check_in_workspace(collection)?;
        let settings = self.collection_repo.get_settings(collection)?;
        let view = MaskedSettings::from_settings(&settings);
        self.publish_tool_invoked(
            session_id,
            "get_collection_settings",
            format!("read the settings of '{collection}'"),
        );
        Ok(view)
    }

    pub fn get_environment(
        &self,
        session_id: &str,
        collection: &str,
        environment: &str,
    ) -> DomainResult<MaskedEnvironment> {
        self.check_in_workspace(collection)?;
        Self::validate_environment_name(environment)?;
        let env = self
            .environment_repo_factory
            .for_collection(collection)
            .get(environment)?;
        let view = MaskedEnvironment::from_environment(&env);
        self.publish_tool_invoked(
            session_id,
            "get_environment",
            format!("read environment '{environment}' of '{collection}'"),
        );
        Ok(view)
    }

    /// The last runs of a request, newest first, at most `HISTORY_LIMIT_MAX`
    /// (0 means the maximum). History records the collection and the
    /// request name, not the path, so two requests with the same name in
    /// one collection share their history here.
    pub fn get_history(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
        limit: usize,
    ) -> DomainResult<Vec<HistoryBrief>> {
        self.check_in_workspace(collection)?;
        let request = self.read_request(collection, request_path)?;
        let mut entries: Vec<rocket_history::HistoryEntry> = self
            .history_repo
            .list(None)?
            .into_iter()
            .filter(|entry| {
                entry.collection.as_deref() == Some(collection)
                    && entry.request_name.as_deref() == Some(request.name.as_str())
            })
            .collect();
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries.truncate(history_limit(limit));
        let briefs: Vec<HistoryBrief> = entries.iter().map(HistoryBrief::from_entry).collect();
        self.publish_tool_invoked(
            session_id,
            "get_history",
            format!("read {} history entr(ies) for '{request_path}'", briefs.len()),
        );
        Ok(briefs)
    }

    pub async fn run_request(
        &self,
        session_id: &str,
        collection: &str,
        request_path: &str,
        environment_name: Option<&str>,
    ) -> DomainResult<McpRunResult> {
        self.check_in_workspace(collection)?;
        self.check_autonomy_enabled(collection)?;
        if let Some(name) = environment_name {
            Self::validate_environment_name(name)?;
        }
        // Evict any stale cache entry before dispatching, so a failed run
        // leaves no cached result behind and `get_test_results` falls back
        // to its "run the request first" `NotFound`.
        self.test_result_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&test_result_key(session_id, collection, request_path));
        let request = self.read_request(collection, request_path)?;
        // The request's own literal credentials, which are not secret
        // variables, are masked in the returned body too.
        let literal_credentials = literal_credential_values(
            &request,
            self.collection_repo.get_settings(collection).ok().as_ref(),
        );
        let item = RunItem::http(request.name.clone(), request_path.to_string(), request);
        // Resolved fresh on every call against the current active workspace,
        // so a workspace switch or a `workspace.yml` edit takes effect at
        // once, and agent runs honor the same request guard opt-ins as the
        // Collection Runner.
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
        // A `DomainError::Http` message can embed the resolved URL or an
        // OAuth2 response body, either of which may hold a secret value.
        // Replace it with fixed text before it reaches the agent; keep the
        // variant. Other variants come from validation and carry no
        // response or URL content.
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

        // The run's own secret set holds every secret variable the executor
        // resolved (collection, environment, global, vault).
        let mut secrets = output.run_secret_values.clone();
        secrets.extend(literal_credentials);
        let (body, body_truncated) = mask_response_body(&output.response.body, &secrets);

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
            body,
            body_truncated,
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
        self.check_in_workspace(collection)?;
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

    pub fn set_env_var(
        &self,
        session_id: &str,
        collection: &str,
        environment_name: &str,
        key: &str,
        value: String,
    ) -> DomainResult<()> {
        self.check_in_workspace(collection)?;
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
        self.check_in_workspace(collection)?;
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

    /// Drops everything this service keeps for `session_id`. Called from
    /// every session end path (Plan 02's `TauriSessionCleanup`, and
    /// `end_agent_session`). A session with nothing stored is a no-op.
    pub fn forget_session(&self, session_id: &str) {
        self.test_result_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(sid, _, _), _| sid != session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdHashMap;
    use std::path::Path;
    use std::sync::Mutex as StdMutex;

    use rocket_collection::{
        Collection, CollectionRepository, CollectionSettings, CollectionVariable, Folder,
        Request as CollectionRequest, RequestScriptPhase, RequestSummary,
    };
    use rocket_environment::{
        Environment, EnvironmentRepository, EnvironmentRepositoryFactory, Variable,
    };
    use rocket_history::HistoryEntry;
    use rocket_shared::types::{Auth, Header, HttpMethod};
    use rocket_workspace::{RequestGuardPolicy, WorkspaceConfig, WorkspaceConfigRepository};

    use crate::mcp_read_views::{CollectionBrief, HISTORY_LIMIT_MAX, RESPONSE_BODY_CAP_BYTES};
    use crate::redaction::REDACTED;
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

    /// Builds an `McpToolService` and its `RequestExecutionService`, sharing
    /// one `ConfigurableCollectionRepo`, one `RecordingPublisher` and one
    /// history store, so a run's history entry is visible to `get_history`.
    fn service_with_history(
        collection_repo: Arc<ConfigurableCollectionRepo>,
        env_factory: Arc<FakeEnvRepoFactory>,
        publisher: Arc<RecordingPublisher>,
        history: Arc<InMemoryHistoryRepo>,
    ) -> McpToolService {
        // Bound as `Arc<dyn HttpExecutor>` at the binding: `Arc::clone`'s
        // generic `Self` does not coerce to a `dyn` target at the call site.
        let executor: Arc<dyn rocket_http::HttpExecutor> = RecordingExecutor::new();
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
            Box::new(SharedHistoryRepo(history)),
        )
    }

    fn service_with(
        collection_repo: Arc<ConfigurableCollectionRepo>,
        env_factory: Arc<FakeEnvRepoFactory>,
        publisher: Arc<RecordingPublisher>,
    ) -> McpToolService {
        service_with_history(collection_repo, env_factory, publisher, InMemoryHistoryRepo::new())
    }

    fn sample_request(name: &str) -> CollectionRequest {
        CollectionRequest::new(name, HttpMethod::Get, "https://api.test/ping")
    }

    /// The run switch still gates the direct-write tools (until Plan 04
    /// replaces them with proposals). `run_request` is checked in its own
    /// async test below.
    #[test]
    fn write_tools_are_refused_when_the_run_switch_is_off() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        let results: Vec<(&str, DomainResult<()>)> = vec![
            (
                "edit_script",
                svc.edit_script("s1", "my-api", "login.yml", RequestScriptPhase::Tests, "// x".into()),
            ),
            ("set_env_var", svc.set_env_var("s1", "my-api", "dev", "HOST", "x".into())),
        ];
        for (tool, result) in results {
            assert_refused_by_autonomy_gate(tool, result);
        }
        assert!(repo.saved_scripts().is_empty(), "a refused edit_script must not write");
        assert!(
            !publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { .. })),
            "a refused call must not publish an audit event"
        );
    }

    /// Spec decision 4: reading any collection in the workspace is always
    /// allowed; the switch only gates running (and, until Plan 04, writing).
    #[test]
    fn read_tools_work_with_the_run_switch_off() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        repo.with_summaries("my-api", two_level_tree());
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let svc = service_with(Arc::clone(&repo), env_factory, RecordingPublisher::new());

        svc.get_workspace_outline("s1", None, None).expect("outline");
        svc.list_collections("s1").expect("list_collections");
        svc.get_request("s1", "my-api", "login.yml").expect("get_request");
        svc.get_collection_settings("s1", "my-api").expect("settings");
        svc.get_environment("s1", "my-api", "dev").expect("environment");
        svc.get_history("s1", "my-api", "login.yml", 5).expect("history");
        let err = svc
            .get_test_results("s1", "my-api", "login.yml")
            .expect_err("nothing ran yet");
        assert!(
            matches!(err, DomainError::NotFound(_)),
            "get_test_results is a read tool, not gated by the switch"
        );
    }

    /// Asserts `result` is the run-switch refusal, not some other error.
    fn assert_refused_by_autonomy_gate(tool: &str, result: DomainResult<()>) {
        match result {
            Err(DomainError::InvalidInput(msg)) => assert!(
                msg.contains("not allowed to run requests in collection"),
                "{tool} failed, but not via the run switch: {msg}"
            ),
            other => panic!("{tool} must be refused by the run switch, got {other:?}"),
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

    fn request_summary(name: &str, method: &str, file: &str) -> RequestSummary {
        RequestSummary {
            uid: format!("uid-{file}"),
            name: name.into(),
            method: method.into(),
            url: format!("https://api.test/{file}"),
            file_name: Some(file.into()),
            kind: Default::default(),
        }
    }

    /// `login.yml` at the root and `auth/refresh.yml` in a subfolder.
    fn two_level_tree() -> Collection {
        let mut collection = Collection::new("my-api");
        collection
            .root
            .add_summary(request_summary("Login", "POST", "login.yml"));
        let mut auth = Folder::new("auth");
        auth.dir_name = Some("auth".into());
        auth.add_summary(request_summary("Refresh", "POST", "refresh.yml"));
        collection.root.add_subfolder(auth);
        collection
    }

    #[test]
    fn every_collection_tool_refuses_a_collection_outside_the_workspace() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        for outside in [
            "other-api",
            "../other-workspace/collections/my-api",
            "My-Api",
        ] {
            let results: Vec<(&str, DomainResult<()>)> = vec![
                (
                    "get_workspace_outline",
                    svc.get_workspace_outline("s1", Some(outside), None).map(|_| ()),
                ),
                ("get_request", svc.get_request("s1", outside, "login.yml").map(|_| ())),
                (
                    "get_collection_settings",
                    svc.get_collection_settings("s1", outside).map(|_| ()),
                ),
                ("get_environment", svc.get_environment("s1", outside, "dev").map(|_| ())),
                ("get_history", svc.get_history("s1", outside, "login.yml", 5).map(|_| ())),
                (
                    "get_test_results",
                    svc.get_test_results("s1", outside, "login.yml").map(|_| ()),
                ),
                (
                    "edit_script",
                    svc.edit_script("s1", outside, "login.yml", RequestScriptPhase::Tests, "// x".into()),
                ),
                ("set_env_var", svc.set_env_var("s1", outside, "dev", "HOST", "x".into())),
            ];
            for (tool, result) in results {
                match result {
                    Err(DomainError::NotFound(msg)) => assert!(
                        msg.contains("not in the current workspace"),
                        "{tool}: {msg}"
                    ),
                    other => panic!("{tool} must refuse '{outside}' by the scope check, got {other:?}"),
                }
            }
        }
        assert!(repo.saved_scripts().is_empty(), "a refused write must not write");
        assert!(
            !publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { .. })),
            "a refused call must not publish an audit event"
        );
    }

    #[test]
    fn get_workspace_outline_walks_folders_shows_the_run_switch_and_filters_by_folder() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_summaries("my-api", two_level_tree());
        repo.set_autonomy("docs-api", false);
        repo.with_summaries("docs-api", Collection::new("docs-api"));
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), FakeEnvRepoFactory::new(), Arc::clone(&publisher));

        let text = svc
            .get_workspace_outline("s1", None, None)
            .expect("outline");
        assert!(text.contains("## my-api (run: on, 2 request(s))"), "{text}");
        assert!(text.contains("POST login.yml"));
        assert!(text.contains("POST auth/refresh.yml"));
        assert!(text.contains("## docs-api (run: off, 0 request(s))"));

        let auth_only = svc
            .get_workspace_outline("s1", Some("my-api"), Some("auth/"))
            .expect("folder outline");
        assert!(auth_only.contains("POST auth/refresh.yml"));
        assert!(!auth_only.contains("POST login.yml"));

        let err = svc
            .get_workspace_outline("s1", None, Some("auth"))
            .expect_err("a folder filter needs a collection");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        let err = svc
            .get_workspace_outline("s1", Some("my-api"), Some("../x"))
            .expect_err("a traversal-shaped folder must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)));

        assert!(publisher.events().iter().any(
            |e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "get_workspace_outline")
        ));
    }

    #[test]
    fn an_unreadable_collection_does_not_break_the_outline() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_summaries("my-api", two_level_tree());
        // Known to the workspace, but with no tree: get_summaries fails.
        repo.set_autonomy("broken", false);
        let svc = service_with(Arc::clone(&repo), FakeEnvRepoFactory::new(), RecordingPublisher::new());

        let text = svc
            .get_workspace_outline("s1", None, None)
            .expect("one broken collection must not fail the outline");
        assert!(text.contains("## broken (run: off, could not be read)"));
        assert!(text.contains("POST login.yml"));
    }

    #[test]
    fn list_collections_reports_request_counts_the_run_switch_and_environment_names() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let svc = service_with(Arc::clone(&repo), env_factory, RecordingPublisher::new());

        let briefs = svc.list_collections("s1").expect("list_collections");
        assert_eq!(
            briefs,
            vec![CollectionBrief {
                name: "my-api".into(),
                request_count: 1,
                run_allowed: true,
                environments: vec!["dev".into()],
            }]
        );
    }

    #[test]
    fn get_request_masks_literal_credentials_and_keeps_references_and_scripts() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        let mut request = sample_request("Login")
            .with_header("Authorization", "Bearer sk-live-abc123")
            .with_header("X-Api-Key", "{{apiKey}}")
            .with_header("Accept", "application/json")
            .with_auth(Auth::Basic {
                username: "alice".into(),
                password: "hunter22".into(),
            });
        request.tests = Some("rok.test('ok', () => {});".to_string());
        repo.with_request("my-api", "login.yml", request);
        let svc = service_with(Arc::clone(&repo), FakeEnvRepoFactory::new(), RecordingPublisher::new());

        let view = svc
            .get_request("s1", "my-api", "login.yml")
            .expect("reading needs no run switch");
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(!json.contains("sk-live-abc123"));
        assert!(!json.contains("hunter22"));
        let header = |key: &str| {
            view.headers
                .iter()
                .find(|h| h.key == key)
                .map(|h| h.value.clone())
                .expect("header present")
        };
        assert_eq!(header("Authorization"), REDACTED);
        assert_eq!(header("X-Api-Key"), "{{apiKey}}");
        assert_eq!(header("Accept"), "application/json");
        assert_eq!(view.auth["username"], "alice");
        assert_eq!(view.tests.as_deref(), Some("rok.test('ok', () => {});"));
    }

    #[test]
    fn get_collection_settings_masks_auth_cookie_and_secret_variables() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings(
            "my-api",
            CollectionSettings {
                agent_autonomy_enabled: true,
                auth: Some(Auth::Bearer {
                    token: "sk-live-collection".into(),
                }),
                headers: vec![Header::new("Cookie", "session=abcdef123")],
                variables: vec![
                    CollectionVariable {
                        key: "baseUrl".into(),
                        value: "https://api.test".into(),
                        initial_value: String::new(),
                        enabled: true,
                        secret: false,
                    },
                    CollectionVariable {
                        key: "clientSecret".into(),
                        value: "cs-live-999".into(),
                        initial_value: String::new(),
                        enabled: true,
                        secret: true,
                    },
                ],
                ..Default::default()
            },
        );
        let svc = service_with(Arc::clone(&repo), FakeEnvRepoFactory::new(), RecordingPublisher::new());

        let view = svc
            .get_collection_settings("s1", "my-api")
            .expect("get_collection_settings");
        let json = serde_json::to_string(&view).expect("serialize");
        for secret in ["sk-live-collection", "abcdef123", "cs-live-999"] {
            assert!(!json.contains(secret), "{secret} leaked");
        }
        assert_eq!(view.auth_type, "bearer");
        assert!(view.run_allowed);
        assert_eq!(view.variables[0].value.as_deref(), Some("https://api.test"));
        assert_eq!(view.variables[1].value, None);
    }

    #[test]
    fn get_history_returns_the_newest_runs_of_that_request_capped_at_ten() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let history = InMemoryHistoryRepo::new();
        {
            let mut entries = history.entries.lock().expect("lock history");
            for i in 0..12u16 {
                let url = if i == 11 {
                    "https://api.test/ping?api_key=sk-live-zzz".to_string()
                } else {
                    format!("https://api.test/ping?page={i}")
                };
                let mut entry = HistoryEntry::new("GET", url, 200 + i, 5, 10)
                    .with_collection("my-api", "Login");
                entry.timestamp =
                    chrono::Utc::now() - chrono::Duration::seconds(i64::from(100 - i));
                entries.push(entry);
            }
            entries.push(
                HistoryEntry::new("GET", "https://api.test/other", 500, 5, 10)
                    .with_collection("my-api", "Other"),
            );
            entries.push(
                HistoryEntry::new("GET", "https://other.test/", 404, 5, 10)
                    .with_collection("other-api", "Login"),
            );
        }
        let svc = service_with_history(
            Arc::clone(&repo),
            FakeEnvRepoFactory::new(),
            RecordingPublisher::new(),
            Arc::clone(&history),
        );

        let briefs = svc
            .get_history("s1", "my-api", "login.yml", 50)
            .expect("get_history");
        assert_eq!(briefs.len(), HISTORY_LIMIT_MAX);
        assert_eq!(briefs[0].status, 211, "newest first");
        assert!(briefs.iter().all(|b| (202..=211).contains(&b.status)));
        assert!(!briefs[0].url.contains("sk-live-zzz"));
        assert_eq!(
            svc.get_history("s1", "my-api", "login.yml", 3)
                .expect("limit 3")
                .len(),
            3
        );
        assert_eq!(
            svc.get_history("s1", "my-api", "login.yml", 0)
                .expect("limit 0 means the maximum")
                .len(),
            HISTORY_LIMIT_MAX
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
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
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
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
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
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
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
    fn get_environment_returns_plain_values_and_names_secrets_without_values() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(env_with_vars());
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, Arc::clone(&publisher));

        let view = svc
            .get_environment("s1", "my-api", "dev")
            .expect("get_environment");
        let host = view
            .variables
            .iter()
            .find(|v| v.key == "HOST")
            .expect("HOST is listed");
        assert_eq!(host.value.as_deref(), Some("api.example.com"));
        let api_key = view
            .variables
            .iter()
            .find(|v| v.key == "API_KEY")
            .expect("a secret is listed by name");
        assert_eq!(api_key.value, None);
        assert!(api_key.secret);
        assert!(!serde_json::to_string(&view)
            .expect("serialize")
            .contains("sk-live-abc"));
        assert!(publisher.events().iter().any(
            |e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "get_environment")
        ));
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

        let env = svc
            .get_environment("s1", "my-api", "dev")
            .expect("read back");
        let host = env
            .variables
            .iter()
            .find(|v| v.key == "HOST")
            .expect("HOST is listed");
        assert_eq!(host.value.as_deref(), Some("api2.example.com"));

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
    fn get_environment_refuses_a_traversal_shaped_environment_name() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let svc = service_with(Arc::clone(&repo), FakeEnvRepoFactory::new(), RecordingPublisher::new());

        let err = svc
            .get_environment("s1", "my-api", "../../other-api/environments/prod")
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
    fn disabling_the_run_switch_mid_session_blocks_the_very_next_write() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let svc = service_with(Arc::clone(&repo), FakeEnvRepoFactory::new(), RecordingPublisher::new());

        svc.edit_script("s1", "my-api", "login.yml", RequestScriptPhase::Tests, "// 1".into())
            .expect("the first write succeeds while the switch is on");

        repo.set_autonomy("my-api", false);

        let err = svc
            .edit_script("s1", "my-api", "login.yml", RequestScriptPhase::Tests, "// 2".into())
            .expect_err("the very next write must be refused once the switch is off");
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
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
        );

        let err = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .expect_err("a redirect to a blocked internal host must fail once the guard is on");
        assert!(matches!(err, DomainError::InvalidInput(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn run_request_refuses_a_collection_outside_the_workspace() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let svc = service_with(Arc::clone(&repo), FakeEnvRepoFactory::new(), RecordingPublisher::new());

        let err = svc
            .run_request("s1", "../other-workspace/collections/my-api", "login.yml", None)
            .await
            .expect_err("a collection outside the workspace must be refused");
        assert!(matches!(err, DomainError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn run_request_returns_a_masked_body_cut_to_eight_kilobytes() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings(
            "my-api",
            CollectionSettings {
                agent_autonomy_enabled: true,
                variables: vec![CollectionVariable {
                    key: "token".into(),
                    value: "sk-live-collection-secret".into(),
                    initial_value: String::new(),
                    enabled: true,
                    secret: true,
                }],
                ..Default::default()
            },
        );
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();

        let executor = RecordingExecutor::new();
        executor.set_body(
            "api.test",
            &format!(
                "{{\"token\":\"sk-live-collection-secret\"}}{}",
                "x".repeat(9_000)
            ),
        );
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
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
        );

        let result = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .expect("run_request");
        assert!(result.body_truncated);
        assert!(result.body.len() <= RESPONSE_BODY_CAP_BYTES);
        assert!(!result.body.contains("sk-live-collection-secret"));
        assert!(result.body.contains(REDACTED));
    }

    /// Runs `login.yml` in `my-api` against a fake server that answers with
    /// `echo`, and returns the tool result.
    async fn run_with_echo(repo: Arc<ConfigurableCollectionRepo>, echo: &str) -> McpRunResult {
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let executor = RecordingExecutor::new();
        executor.set_body("api.test", echo);
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
            exec_svc,
            publisher_dyn,
            FixedPolicyConfigRepo::permissive(),
            dummy_workspace_path(),
            Box::new(SharedHistoryRepo(history)),
        );
        svc.run_request("s1", "my-api", "login.yml", None)
            .await
            .expect("run_request")
    }

    #[tokio::test]
    async fn a_secret_collection_variable_with_only_an_initial_value_is_masked_in_the_body() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_settings(
            "my-api",
            CollectionSettings {
                agent_autonomy_enabled: true,
                variables: vec![CollectionVariable {
                    key: "token".into(),
                    value: String::new(),
                    initial_value: "sk-live-x-initial".into(),
                    enabled: true,
                    secret: true,
                }],
                ..Default::default()
            },
        );
        repo.with_request("my-api", "login.yml", sample_request("Login"));

        let result = run_with_echo(repo, "{\"echo\":\"sk-live-x-initial\"}").await;
        assert!(!result.body.contains("sk-live-x-initial"), "{}", result.body);
        assert!(result.body.contains(REDACTED));
    }

    #[tokio::test]
    async fn literal_request_credentials_are_masked_in_an_echoing_response() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", true);
        let request = sample_request("Login")
            .with_header("Authorization", "Bearer sk-live-header-token")
            .with_auth(Auth::Basic {
                username: "alice".into(),
                password: "hunter2-literal".into(),
            });
        repo.with_request("my-api", "login.yml", request);

        let echo = "Authorization: Bearer sk-live-header-token; password=hunter2-literal; \
                    token only: sk-live-header-token";
        let result = run_with_echo(repo, echo).await;
        for secret in ["sk-live-header-token", "hunter2-literal"] {
            assert!(!result.body.contains(secret), "{secret} leaked: {}", result.body);
        }
        assert!(result.body.contains("alice") || result.body.contains(REDACTED));
    }

    #[test]
    fn get_request_refuses_environment_files() {
        let repo = ConfigurableCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "environments/prod.yml", sample_request("Env"));
        let svc = service_with(Arc::clone(&repo), FakeEnvRepoFactory::new(), RecordingPublisher::new());

        for path in ["environments/prod.yml", "./Environments/prod.yml", "/environments/x.yml"] {
            let err = svc
                .get_request("s1", "my-api", path)
                .expect_err("an environment file is not a request");
            assert!(matches!(err, DomainError::InvalidInput(_)), "{path}: {err:?}");
        }
    }
}

