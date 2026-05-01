use std::sync::Arc;

use rocket_http::{run_load_test_v2, HttpExecutor, LoadTestConfigV2, LoadTestResult};
use rocket_shared::error::DomainResult;

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
        #[cfg(feature = "tauri-events")] app: &tauri::AppHandle,
    ) -> DomainResult<LoadTestResult> {
        let resolved = execution_service.resolve_request(&input)?;
        let result = run_load_test_v2(
            executor,
            &resolved,
            &config,
            #[cfg(feature = "tauri-events")]
            app,
        )
        .await;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
    use rocket_shared::error::DomainResult;
    use std::sync::{Arc, Mutex};

    struct CapturingExecutor {
        last_url: Arc<Mutex<Option<String>>>,
    }

    impl CapturingExecutor {
        fn new() -> (Self, Arc<Mutex<Option<String>>>) {
            let url = Arc::new(Mutex::new(None));
            (
                Self {
                    last_url: Arc::clone(&url),
                },
                url,
            )
        }
    }

    #[async_trait::async_trait]
    impl HttpExecutor for CapturingExecutor {
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

    #[test]
    fn load_test_service_module_compiles() {
        let (exec, _url) = CapturingExecutor::new();
        let _ = Arc::new(exec) as Arc<dyn HttpExecutor>;
    }
}
