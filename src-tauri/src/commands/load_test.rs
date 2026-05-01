use std::sync::Arc;

use rocket_app::{
    ExecuteRequestInput, ExportFormat, ExportService, LoadTestService, RequestExecutionService,
};
use rocket_http::{HttpExecutor, LoadTestConfig, LoadTestConfigV2, LoadTestResult};
use rocket_shared::error::DomainError;
use tauri::{AppHandle, State};

/// Runs a load test against the given request and returns aggregated statistics.
/// Variable resolution is handled by RequestExecutionService using the same
/// scopes as execute_request (collection < env < folder < request).
///
/// Legacy flat-config command. Kept for the existing frontend until Plans D/E
/// migrate to the v2 streaming API.
#[tauri::command]
pub async fn run_load_test_command(
    input: ExecuteRequestInput,
    config: LoadTestConfig,
    svc: State<'_, RequestExecutionService>,
) -> Result<LoadTestResult, DomainError> {
    svc.run_load_test(input, config).await
}

/// Phase-based load test. Streams `load_test_progress` events every 250 ms
/// and a final `load_test_complete` event. The function returns once the
/// run finishes; the frontend reads results from the events.
#[tauri::command]
pub async fn run_load_test_v2_command(
    app: AppHandle,
    input: ExecuteRequestInput,
    config: LoadTestConfigV2,
    exec_svc: State<'_, RequestExecutionService>,
    executor: State<'_, Arc<dyn HttpExecutor>>,
) -> Result<(), DomainError> {
    LoadTestService::run(&exec_svc, Arc::clone(&executor), input, config, &app).await?;
    Ok(())
}

/// Export a load test result. Returns `(base64_bytes, extension)`.
/// The frontend writes the file via the dialog plugin.
#[tauri::command]
pub async fn export_load_test(
    result: LoadTestResult,
    format: ExportFormat,
) -> Result<(String, String), DomainError> {
    let (bytes, ext) = ExportService::export(&result, format)?;
    let b64 = base64_encode(&bytes);
    Ok((b64, ext.to_string()))
}

fn base64_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.encode(data)
}
