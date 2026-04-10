use rocket_app::{ExecuteRequestInput, RequestExecutionService};
use rocket_http::{LoadTestConfig, LoadTestResult};
use rocket_shared::error::DomainError;
use tauri::State;

/// Runs a load test against the given request and returns aggregated statistics.
/// Variable resolution is handled by RequestExecutionService using the same
/// scopes as execute_request (collection < env < folder < request).
#[tauri::command]
pub async fn run_load_test_command(
    input: ExecuteRequestInput,
    config: LoadTestConfig,
    svc: State<'_, RequestExecutionService>,
) -> Result<LoadTestResult, DomainError> {
    svc.run_load_test(input, config).await
}
