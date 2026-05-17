use std::sync::Arc;

use rocket_http::{run_load_test_v2, HttpExecutor, LoadTestConfigV2, LoadTestResult};
use rocket_shared::error::{DomainError, DomainResult};

use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};

/// Orchestrates phase-based load testing with full variable resolution.
/// Reuses `RequestExecutionService::resolve_request` so collection / env / folder /
/// request variables are merged identically to a regular request execution.
pub struct LoadTestService;

impl LoadTestService {
    pub async fn run(
        execution_service: &RequestExecutionService,
        executor: Arc<dyn HttpExecutor>,
        input: ExecuteRequestInput,
        config: LoadTestConfigV2,
        app: Option<&tauri::AppHandle>,
    ) -> DomainResult<LoadTestResult> {
        if !config.has_uniform_target_unit() {
            return Err(DomainError::InvalidInput(
                "Load test phases must all use the same target unit \
                 (either all concurrency-based or all rps-based). \
                 Mixing units in a single run is not supported."
                    .into(),
            ));
        }

        let resolved = execution_service.resolve_request(&input)?;
        let result = run_load_test_v2(executor, &resolved, &config, app).await;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_collection::{
        Collection, CollectionRepository, CollectionSettings, CollectionSummary,
        CollectionVariable, Request as CollectionRequest,
    };
    use rocket_environment::{Environment, EnvironmentRepository, Variable};
    use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
    use rocket_http::{
        CookieJar, CookieRepository, HttpRequest, HttpResponse, LoadTestPhase, PhaseKind,
        PhaseTarget, RequestOptions, SuccessRule,
    };
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::HttpMethod;
    use std::sync::{Arc, Mutex};

    // Captures the last URL the executor saw so the test can assert variable resolution.
    struct MockExecutor {
        last_url: Mutex<Option<String>>,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                last_url: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl HttpExecutor for MockExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            *self.last_url.lock().unwrap() = Some(req.url.clone());
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: "{}".into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: 2,
            })
        }
    }

    // Environment repo holding a single pre-loaded environment.
    struct MockEnvRepo {
        env: Option<Environment>,
    }

    impl MockEnvRepo {
        fn with_env(env: Environment) -> Self {
            Self { env: Some(env) }
        }
    }

    impl EnvironmentRepository for MockEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.env.iter().cloned().collect())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.env
                .as_ref()
                .filter(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    struct MockHistoryRepo {
        entries: Mutex<Vec<HistoryEntry>>,
    }

    impl MockHistoryRepo {
        fn new() -> Self {
            Self {
                entries: Mutex::new(Vec::new()),
            }
        }
    }

    impl HistoryRepository for MockHistoryRepo {
        fn list(&self, _: Option<usize>) -> DomainResult<Vec<HistoryEntry>> {
            Ok(self.entries.lock().unwrap().clone())
        }
        fn get(&self, id: &str) -> DomainResult<HistoryEntry> {
            self.entries
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.id == id)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(id.into()))
        }
        fn save(&self, entry: &HistoryEntry) -> DomainResult<()> {
            self.entries.lock().unwrap().push(entry.clone());
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            self.entries.lock().unwrap().clear();
            Ok(())
        }
        fn search(&self, _: &HistoryFilter) -> DomainResult<Vec<HistoryEntry>> {
            Ok(self.entries.lock().unwrap().clone())
        }
    }

    struct NullCookieRepo;

    impl CookieRepository for NullCookieRepo {
        fn get_all(&self) -> DomainResult<Vec<CookieJar>> {
            Ok(vec![])
        }
        fn get_by_domain(&self, _: &str) -> DomainResult<Option<CookieJar>> {
            Ok(None)
        }
        fn save(&self, _: &CookieJar) -> DomainResult<()> {
            Ok(())
        }
        fn clear(&self) -> DomainResult<()> {
            Ok(())
        }
    }

    // Empty collection repo. Variable resolution comes from the environment alone.
    struct StubCollectionRepo;

    impl StubCollectionRepo {
        fn empty() -> Self {
            Self
        }
    }

    impl CollectionRepository for StubCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            Ok(vec![])
        }
        fn get(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn get_summaries(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
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
        fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn save_request(
            &self,
            _: &str,
            path: &str,
            _: &CollectionRequest,
        ) -> DomainResult<String> {
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
        fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
            Ok(CollectionSettings::default())
        }
        fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn get_folder_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn get_request_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn run_resolves_variables_before_firing_load_test() {
        // Environment provides the variable referenced in the request URL.
        let mut env = Environment::new("staging");
        env.set_variable(Variable::new("oidc-baseurl", "https://auth.local"));

        // Share a single executor between the service and the assertion.
        let executor = Arc::new(MockExecutor::new());

        // Newtype wrapper so we can hand the same executor to the service as a trait object.
        struct SharedExec(Arc<MockExecutor>);
        #[async_trait]
        impl HttpExecutor for SharedExec {
            async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
                self.0.execute(req).await
            }
        }

        let exec_arc = Arc::clone(&executor);
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(env)),
            Arc::new(SharedExec(executor)),
            Box::new(MockHistoryRepo::new()),
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
        );

        let input = ExecuteRequestInput {
            method: HttpMethod::Get,
            url: "{{oidc-baseurl}}/api/data".to_string(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth: rocket_shared::types::Auth::None,
            options: RequestOptions::default(),
            environment_name: Some("staging".into()),
            collection: None,
            request_name: None,
            request_path: None,
        };

        // One short Hold phase to keep the test fast.
        let config = LoadTestConfigV2 {
            phases: vec![LoadTestPhase {
                kind: PhaseKind::Hold,
                duration_secs: 1,
                target: PhaseTarget::Concurrency(1),
            }],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 100,
            max_requests: None,
        };

        let load_exec: Arc<dyn HttpExecutor> = Arc::new(SharedExec(Arc::clone(&exec_arc)));
        let result = LoadTestService::run(&svc, load_exec, input, config, None)
            .await
            .unwrap();

        assert!(
            result.total_requests > 0,
            "expected at least one request to fire"
        );
        assert!(
            result.succeeded > 0,
            "expected at least one successful request"
        );

        // The URL captured by the executor proves variable resolution happened.
        let url = exec_arc.last_url.lock().unwrap().clone().unwrap();
        assert_eq!(url, "https://auth.local/api/data");
    }

    #[test]
    fn mixed_unit_config_validation() {
        // Verify the validation method that the service uses returns false
        // for mixed-unit configs. We don't actually run() here because that
        // requires wiring the full RequestExecutionService — see the existing
        // integration test in this file for the full setup pattern.
        use rocket_http::{LoadTestConfigV2, LoadTestPhase, PhaseKind, PhaseTarget, SuccessRule};

        let mixed = LoadTestConfigV2 {
            phases: vec![
                LoadTestPhase {
                    kind: PhaseKind::Hold,
                    duration_secs: 5,
                    target: PhaseTarget::Concurrency(10),
                },
                LoadTestPhase {
                    kind: PhaseKind::Hold,
                    duration_secs: 5,
                    target: PhaseTarget::Rps(50),
                },
            ],
            success_rule: SuccessRule::default(),
            ring_buffer_size: 100,
            max_requests: None,
        };
        assert!(!mixed.has_uniform_target_unit());
    }
}
