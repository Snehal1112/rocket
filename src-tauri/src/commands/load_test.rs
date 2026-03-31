use std::sync::Arc;

use rocket_http::{HttpRequest, HttpExecutor, LoadTestConfig, LoadTestResult, run_load_test};
use rocket_infra::ReqwestExecutor;
use rocket_shared::error::DomainError;

/// Runs a load test against the given request and returns aggregated statistics.
#[tauri::command]
pub async fn run_load_test_command(
    request: HttpRequest,
    config: LoadTestConfig,
) -> Result<LoadTestResult, DomainError> {
    let executor: Arc<dyn HttpExecutor> = Arc::new(ReqwestExecutor::new());
    Ok(run_load_test(executor, &request, &config).await)
}
