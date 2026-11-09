use rocket_app::{
    ExecuteGraphQlInput, ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService,
    SentRequest,
};
use rocket_collection::graphql_document::{list_operations, GraphQlOperation};
use rocket_scripting::{ConsoleLevel, TestStatus};
use rocket_shared::error::DomainError;
use serde::Serialize;
use tauri::State;

/// IPC DTO for a single script test result.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcTestResult {
    pub name: String,
    pub status: String,
    pub error: Option<String>,
}

/// IPC DTO for a single console entry.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcConsoleEntry {
    pub level: String,
    pub message: String,
}

/// Full IPC response for an executed request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequestResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<rocket_shared::types::Header>,
    pub body: String,
    pub duration_ms: u64,
    pub ttfb_ms: u64,
    pub size_bytes: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_base64: Option<String>,
    pub test_results: Vec<IpcTestResult>,
    pub console_entries: Vec<IpcConsoleEntry>,
    pub script_error: Option<String>,
    /// The request as sent, with RocketVault values masked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_request: Option<SentRequest>,
}

impl From<ExecuteRequestOutput> for ExecuteRequestResponse {
    fn from(out: ExecuteRequestOutput) -> Self {
        Self {
            status: out.response.status,
            status_text: out.response.status_text,
            headers: out.response.headers,
            body: out.response.body,
            duration_ms: out.response.duration_ms,
            ttfb_ms: out.response.ttfb_ms,
            size_bytes: out.response.size_bytes,
            is_binary: out.response.is_binary,
            body_base64: out.response.body_base64,
            test_results: out
                .test_results
                .iter()
                .map(|t| IpcTestResult {
                    name: t.name.clone(),
                    status: match t.status {
                        TestStatus::Passed => "passed".into(),
                        TestStatus::Failed => "failed".into(),
                    },
                    error: t.error.clone(),
                })
                .collect(),
            console_entries: out
                .console_entries
                .iter()
                .map(|e| IpcConsoleEntry {
                    level: match e.level {
                        ConsoleLevel::Log => "log".into(),
                        ConsoleLevel::Warn => "warn".into(),
                        ConsoleLevel::Error => "error".into(),
                    },
                    message: e.message.clone(),
                })
                .collect(),
            script_error: out.script_error,
            sent_request: out.sent_request,
        }
    }
}

#[tauri::command]
pub async fn execute_request(
    input: ExecuteRequestInput,
    svc: State<'_, RequestExecutionService>,
) -> Result<ExecuteRequestResponse, DomainError> {
    svc.execute(input).await.map(ExecuteRequestResponse::from)
}

/// Sends a GraphQL operation through the HTTP execution path.
#[tauri::command]
pub async fn execute_graphql_request(
    input: ExecuteGraphQlInput,
    svc: State<'_, RequestExecutionService>,
) -> Result<ExecuteRequestResponse, DomainError> {
    svc.execute_graphql(input)
        .await
        .map(ExecuteRequestResponse::from)
}

/// Lists the operations a GraphQL document defines, for the operation picker.
#[tauri::command]
pub fn list_graphql_operations(document: String) -> Vec<GraphQlOperation> {
    list_operations(&document)
}

/// Preview-evaluates a jsonq expression against a captured response, for the
/// Vars tab's "Test" affordance.
#[tauri::command]
pub async fn evaluate_var_expression(
    collection_root: String,
    expression: String,
    response_json: String,
    svc: State<'_, RequestExecutionService>,
) -> Result<serde_json::Value, DomainError> {
    svc.evaluate_var_expression(&collection_root, &expression, &response_json)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::HttpResponse;

    #[test]
    fn binary_fields_reach_the_ipc_response() {
        let out = ExecuteRequestOutput {
            response: HttpResponse {
                status: 200,
                is_binary: true,
                body_base64: Some("AAEC".into()),
                size_bytes: 3,
                ..Default::default()
            },
            test_results: vec![],
            console_entries: vec![],
            script_error: None,
            deferred_history: None,
            run_secret_values: Default::default(),
            run_sent_credentials: Default::default(),
            run_process_env_values: Default::default(),
            process_env_withheld: false,
            vault_withheld: false,
            sent_request: None,
        };
        let json = serde_json::to_value(ExecuteRequestResponse::from(out)).expect("serialize");
        assert_eq!(json["isBinary"], true);
        assert_eq!(json["bodyBase64"], "AAEC");
    }

    #[test]
    fn text_responses_omit_the_binary_fields() {
        let out = ExecuteRequestOutput {
            response: HttpResponse::default(),
            test_results: vec![],
            console_entries: vec![],
            script_error: None,
            deferred_history: None,
            run_secret_values: Default::default(),
            run_sent_credentials: Default::default(),
            run_process_env_values: Default::default(),
            process_env_withheld: false,
            vault_withheld: false,
            sent_request: None,
        };
        let json = serde_json::to_value(ExecuteRequestResponse::from(out)).expect("serialize");
        assert!(json.get("isBinary").is_none());
        assert!(json.get("bodyBase64").is_none());
        assert!(json.get("sentRequest").is_none());
    }

    #[test]
    fn the_sent_request_reaches_the_ipc_response() {
        let sent = SentRequest {
            url: "https://api.test/v1".into(),
            headers: vec![rocket_shared::types::Header::new("X-A", "1")],
            query_params: vec![rocket_shared::types::Header::new("q", "2")],
            body: None,
            auth: rocket_shared::types::Auth::None,
            secret_values: vec![],
        };
        let out = ExecuteRequestOutput {
            response: HttpResponse::default(),
            test_results: vec![],
            console_entries: vec![],
            script_error: None,
            deferred_history: None,
            run_secret_values: Default::default(),
            run_sent_credentials: Default::default(),
            run_process_env_values: Default::default(),
            process_env_withheld: false,
            vault_withheld: false,
            sent_request: Some(sent),
        };
        let json = serde_json::to_value(ExecuteRequestResponse::from(out)).expect("serialize");
        assert_eq!(json["sentRequest"]["url"], "https://api.test/v1");
        assert_eq!(json["sentRequest"]["queryParams"][0]["key"], "q");
    }
}
