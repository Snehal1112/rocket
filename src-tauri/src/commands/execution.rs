use rocket_app::{ExecuteRequestInput, RequestExecutionService};
use rocket_http::HttpResponse;
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub async fn execute_request(
    input: ExecuteRequestInput,
    svc: State<'_, RequestExecutionService>,
) -> Result<HttpResponse, DomainError> {
    svc.execute(input).await
}
