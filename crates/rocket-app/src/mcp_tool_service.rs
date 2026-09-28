//! Lets an ACP agent act on a Rocket collection: list requests, run one,
//! edit a script, read/write a non-secret environment variable, and read
//! the last cached test results. Every method first re-checks the target
//! collection's `agent_autonomy_enabled` flag and refuses if it is off —
//! this is the safety valve described in the design spec, checked fresh on
//! every call so a mid-session toggle takes effect immediately. Every
//! successful call publishes `DomainEvent::AcpToolInvoked` for the audit
//! trail.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::Arc;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};

use crate::execution_service::RequestExecutionService;
use crate::runner_sequence::{build_step_input, RunItem};

/// One request entry in a `list_collection_requests` result. `path` is
/// relative to the collection root, matching the shape `run_request` and
/// `edit_script` expect back.
#[derive(Debug, Clone, PartialEq)]
pub struct McpRequestEntry {
    pub path: String,
    pub name: String,
    pub method: String,
    pub url: String,
}

/// Summary of one `run_request` call, enough for an agent to decide what to
/// do next without re-fetching the full response body.
#[derive(Debug, Clone, PartialEq)]
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
/// piece of state this service owns: an in-memory, session-lifetime map from
/// `(session_id, request_path)` to the test results of that pair's most
/// recent `run_request` call, per the design spec's explicit choice not to
/// persist test results into `rocket-history`.
pub struct McpToolService {
    collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
    environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
    execution_svc: Arc<RequestExecutionService>,
    event_publisher: Arc<dyn EventPublisher>,
    test_result_cache: Mutex<HashMap<(String, String), Vec<rocket_scripting::TestResult>>>,
}

impl McpToolService {
    pub fn new(
        collection_repo: Arc<dyn rocket_collection::CollectionRepository>,
        environment_repo_factory: Arc<dyn rocket_environment::EnvironmentRepositoryFactory>,
        execution_svc: Arc<RequestExecutionService>,
        event_publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            collection_repo,
            environment_repo_factory,
            execution_svc,
            event_publisher,
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
        let request = self.collection_repo.get_request(collection, request_path)?;
        let item = RunItem {
            name: request.name.clone(),
            request_path: request_path.to_string(),
            request,
        };
        let input = build_step_input(
            &item,
            collection,
            environment_name,
            None,
            rocket_workspace::RequestGuardPolicy::default(),
            rocket_shared::RunSource::Agent,
        );
        let output = self.execution_svc.execute(input).await?;

        let test_pass_count = output
            .test_results
            .iter()
            .filter(|t| matches!(t.status, rocket_scripting::TestStatus::Passed))
            .count();
        let test_fail_count = output.test_results.len() - test_pass_count;

        self.test_result_cache
            .lock()
            .expect("lock McpToolService test_result_cache")
            .insert(
                (session_id.to_string(), request_path.to_string()),
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
        let repo = self.environment_repo_factory.for_collection(collection);
        let mut env = repo.get(environment_name)?;
        let variable = env
            .variables
            .iter_mut()
            .find(|v| v.key == key)
            .ok_or_else(|| DomainError::InvalidInput(VARIABLE_NOT_ACCESSIBLE.to_string()))?;
        if variable.secret {
            return Err(DomainError::InvalidInput(VARIABLE_NOT_ACCESSIBLE.to_string()));
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
            .expect("lock McpToolService test_result_cache")
            .get(&(session_id.to_string(), request_path.to_string()))
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
            format!("read {} cached test result(s) for '{request_path}'", results.len()),
        );
        Ok(results)
    }
}

/// Depth-first walk of a `get_summaries()` tree, collecting one
/// `McpRequestEntry` per `CollectionItem::Summary` leaf. Mirrors
/// `runner_sequence::collect_items`'s traversal, but over summary leaves
/// instead of full `Request` bodies — the two item shapes are different
/// enum variants (`Summary` vs `Request`), so this is a separate, small
/// walk rather than a shared generic one.
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
                let dir_name = sub.dir_name.as_deref().unwrap_or(&sub.name);
                let sub_prefix = format!("{prefix}{dir_name}/");
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
    use std::sync::Mutex as StdMutex;

    use rocket_collection::{
        Collection, CollectionRepository, CollectionSettings, CollectionSummary,
        CollectionVariable, Folder, Request as CollectionRequest, RequestScriptPhase,
        RequestSummary,
    };
    use rocket_environment::{Environment, EnvironmentRepository, EnvironmentRepositoryFactory, Variable};
    use rocket_shared::types::HttpMethod;

    use crate::test_doubles::{
        EmptySecretManagerRepo, InMemoryHistoryRepo, NullCookieRepo, NullEnvRepo,
        RecordingExecutor, RecordingPublisher, SharedHistoryRepo,
        SharedPublisher,
    };

    /// Collection repo double with mutable, per-collection settings (so a
    /// test can toggle `agent_autonomy_enabled` mid-test), configurable
    /// requests and summary trees, and a record of every
    /// `save_request_script` call. Purpose-built for this file rather than
    /// reusing `crate::test_doubles::InMemoryCollectionRepo`, which holds one
    /// immutable `Collection` and cannot support the mid-session-toggle test
    /// below.
    struct FakeCollectionRepo {
        settings: StdMutex<StdHashMap<String, CollectionSettings>>,
        requests: StdMutex<StdHashMap<(String, String), CollectionRequest>>,
        summaries: StdMutex<StdHashMap<String, Collection>>,
        saved_scripts: StdMutex<Vec<(String, String, RequestScriptPhase, String)>>,
    }

    impl FakeCollectionRepo {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                settings: StdMutex::new(StdHashMap::new()),
                requests: StdMutex::new(StdHashMap::new()),
                summaries: StdMutex::new(StdHashMap::new()),
                saved_scripts: StdMutex::new(Vec::new()),
            })
        }

        fn set_autonomy(&self, collection: &str, enabled: bool) {
            let mut settings = CollectionSettings::default();
            settings.agent_autonomy_enabled = enabled;
            self.settings
                .lock()
                .expect("lock FakeCollectionRepo settings")
                .insert(collection.to_string(), settings);
        }

        fn with_request(&self, collection: &str, path: &str, request: CollectionRequest) {
            self.requests
                .lock()
                .expect("lock FakeCollectionRepo requests")
                .insert((collection.to_string(), path.to_string()), request);
        }

        fn with_summaries(&self, collection: &str, tree: Collection) {
            self.summaries
                .lock()
                .expect("lock FakeCollectionRepo summaries")
                .insert(collection.to_string(), tree);
        }

        fn saved_scripts(&self) -> Vec<(String, String, RequestScriptPhase, String)> {
            self.saved_scripts
                .lock()
                .expect("lock FakeCollectionRepo saved_scripts")
                .clone()
        }
    }

    impl CollectionRepository for FakeCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            Ok(vec![])
        }
        fn get(&self, name: &str) -> DomainResult<Collection> {
            self.summaries
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn get_summaries(&self, name: &str) -> DomainResult<Collection> {
            self.get(name)
        }
        fn create(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn get_request(&self, collection: &str, path: &str) -> DomainResult<CollectionRequest> {
            self.requests
                .lock()
                .expect("lock")
                .get(&(collection.to_string(), path.to_string()))
                .cloned()
                .ok_or_else(|| DomainError::NotFound(format!("{collection}/{path}")))
        }
        fn save_request(&self, _: &str, path: &str, _: &CollectionRequest) -> DomainResult<String> {
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            Ok(())
        }
        fn get_settings(&self, name: &str) -> DomainResult<CollectionSettings> {
            Ok(self
                .settings
                .lock()
                .expect("lock")
                .get(name)
                .cloned()
                .unwrap_or_default())
        }
        fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn get_folder_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> {
            Ok(())
        }
        fn get_request_variables(&self, _: &str, _: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_request_variables(&self, _: &str, _: &str, _: Vec<CollectionVariable>) -> DomainResult<()> {
            Ok(())
        }
        fn save_request_script(
            &self,
            collection: &str,
            request_path: &str,
            phase: RequestScriptPhase,
            body: String,
        ) -> DomainResult<()> {
            self.saved_scripts
                .lock()
                .expect("lock")
                .push((collection.to_string(), request_path.to_string(), phase, body));
            Ok(())
        }
    }

    /// A thin `Box<dyn CollectionRepository>`-shaped wrapper around one
    /// shared `Arc<FakeCollectionRepo>`, so the same repo instance can be
    /// handed to both `RequestExecutionService` (which owns a `Box`) and
    /// `McpToolService` (which owns an `Arc`) in the same test.
    struct SharedFakeCollectionRepo(Arc<FakeCollectionRepo>);
    impl CollectionRepository for SharedFakeCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            self.0.list()
        }
        fn get(&self, n: &str) -> DomainResult<Collection> {
            self.0.get(n)
        }
        fn get_summaries(&self, n: &str) -> DomainResult<Collection> {
            self.0.get_summaries(n)
        }
        fn create(&self, n: &str) -> DomainResult<Collection> {
            self.0.create(n)
        }
        fn delete(&self, n: &str) -> DomainResult<()> {
            self.0.delete(n)
        }
        fn rename(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.rename(a, b)
        }
        fn get_request(&self, a: &str, b: &str) -> DomainResult<CollectionRequest> {
            self.0.get_request(a, b)
        }
        fn save_request(&self, a: &str, b: &str, c: &CollectionRequest) -> DomainResult<String> {
            self.0.save_request(a, b, c)
        }
        fn rename_request(&self, a: &str, b: &str, c: &str) -> DomainResult<()> {
            self.0.rename_request(a, b, c)
        }
        fn delete_request(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.delete_request(a, b)
        }
        fn create_folder(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.create_folder(a, b)
        }
        fn delete_folder(&self, a: &str, b: &str) -> DomainResult<()> {
            self.0.delete_folder(a, b)
        }
        fn move_item(&self, a: &str, b: &str, c: &str, d: &str) -> DomainResult<()> {
            self.0.move_item(a, b, c, d)
        }
        fn reorder_items(&self, a: &str, b: &str, c: &[String]) -> DomainResult<()> {
            self.0.reorder_items(a, b, c)
        }
        fn get_settings(&self, n: &str) -> DomainResult<CollectionSettings> {
            self.0.get_settings(n)
        }
        fn save_settings(&self, n: &str, s: &CollectionSettings) -> DomainResult<()> {
            self.0.save_settings(n, s)
        }
        fn get_folder_chain_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_folder_chain_variables(a, b)
        }
        fn get_folder_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_folder_variables(a, b)
        }
        fn save_folder_variables(&self, a: &str, b: &str, c: Vec<CollectionVariable>) -> DomainResult<()> {
            self.0.save_folder_variables(a, b, c)
        }
        fn get_request_variables(&self, a: &str, b: &str) -> DomainResult<Vec<CollectionVariable>> {
            self.0.get_request_variables(a, b)
        }
        fn save_request_variables(&self, a: &str, b: &str, c: Vec<CollectionVariable>) -> DomainResult<()> {
            self.0.save_request_variables(a, b, c)
        }
        fn save_request_script(
            &self,
            a: &str,
            b: &str,
            c: RequestScriptPhase,
            d: String,
        ) -> DomainResult<()> {
            self.0.save_request_script(a, b, c, d)
        }
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
            self.envs.lock().expect("lock").insert(env.name.clone(), env);
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
            self.0.lock().expect("lock").insert(env.name.clone(), env.clone());
            Ok(())
        }
        fn delete(&self, name: &str) -> DomainResult<()> {
            self.0.lock().expect("lock").remove(name);
            Ok(())
        }
    }

    /// Builds an `McpToolService` plus its backing `RequestExecutionService`,
    /// sharing one `FakeCollectionRepo` and one `RecordingPublisher` between
    /// them so a test can both drive HTTP dispatch and inspect every
    /// `DomainEvent` (including `AcpToolInvoked`) either service published.
    fn service_with(
        collection_repo: Arc<FakeCollectionRepo>,
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
            Box::new(SharedFakeCollectionRepo(Arc::clone(&collection_repo))),
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
        )
    }

    fn sample_request(name: &str) -> CollectionRequest {
        CollectionRequest::new(name, HttpMethod::Get, "https://api.test/ping")
    }

    /// Table-driven proof that all 6 tools refuse when autonomy is off — the
    /// Review Focus item this plan and the index both call out.
    #[test]
    fn every_tool_is_refused_when_autonomy_is_disabled() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        env_factory.with_env(Environment::new("dev"));
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        assert!(svc.list_collection_requests("s1", "my-api").is_err());
        assert!(svc.edit_script("s1", "my-api", "login.yml", RequestScriptPhase::Tests, "// x".into()).is_err());
        assert!(svc.get_env_var("s1", "my-api", "dev", "HOST").is_err());
        assert!(svc.set_env_var("s1", "my-api", "dev", "HOST", "x".into()).is_err());
        assert!(svc.get_test_results("s1", "my-api", "login.yml").is_err());
        // run_request is async — checked in its own test below (Step 6) since
        // this test function is synchronous; the assertion set above already
        // covers every synchronous tool with the shared fixture.
    }

    #[tokio::test]
    async fn run_request_is_refused_when_autonomy_is_disabled() {
        let repo = FakeCollectionRepo::new();
        repo.set_autonomy("my-api", false);
        repo.with_request("my-api", "login.yml", sample_request("Login"));
        let env_factory = FakeEnvRepoFactory::new();
        let publisher = RecordingPublisher::new();
        let svc = service_with(Arc::clone(&repo), env_factory, publisher);

        let err = svc
            .run_request("s1", "my-api", "login.yml", None)
            .await
            .expect_err("run_request must be refused when autonomy is disabled");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn list_collection_requests_walks_folders_and_publishes_audit_event() {
        let repo = FakeCollectionRepo::new();
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
        let repo = FakeCollectionRepo::new();
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
            Box::new(SharedFakeCollectionRepo(Arc::clone(&repo))),
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
        assert!(cached.is_empty(), "this fixture's request has no test script, so no results");

        assert!(
            publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "run_request")),
            "expected an AcpToolInvoked event for run_request"
        );
    }

    #[test]
    fn edit_script_saves_via_the_repository_and_publishes_audit_event() {
        let repo = FakeCollectionRepo::new();
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
            publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "edit_script")),
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
        let repo = FakeCollectionRepo::new();
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
        let repo = FakeCollectionRepo::new();
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

        assert_eq!(not_found.to_string(), is_secret.to_string(), "the two error messages must be indistinguishable");

        // Case sensitivity: a differently-cased key is also just "not found",
        // not a secret-detection bypass or a distinct error shape.
        let wrong_case = svc
            .get_env_var("s1", "my-api", "dev", "host")
            .expect_err("key lookup is case-sensitive, so this must also be the same error");
        assert_eq!(wrong_case.to_string(), not_found.to_string());
    }

    #[test]
    fn set_env_var_writes_a_non_secret_variable_and_it_is_readable_back() {
        let repo = FakeCollectionRepo::new();
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
            publisher
                .events()
                .iter()
                .any(|e| matches!(e, DomainEvent::AcpToolInvoked { tool, .. } if tool == "set_env_var")),
            "expected an AcpToolInvoked event for set_env_var"
        );
    }

    #[test]
    fn set_env_var_refuses_a_secret_variable_and_does_not_create_missing_keys() {
        let repo = FakeCollectionRepo::new();
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
        let repo = FakeCollectionRepo::new();
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
    fn disabling_autonomy_mid_session_blocks_the_very_next_call() {
        let repo = FakeCollectionRepo::new();
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
}
