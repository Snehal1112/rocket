use std::collections::HashMap;
use std::time::Duration;

use rocket_app::{GrpcExecuteInput, GrpcService, RequestExecutionService};
use rocket_collection::GrpcRequest;
use rocket_grpc::GrpcUnaryResponse;
use rocket_shared::error::DomainError;
use serde::Deserialize;
use tauri::State;

/// What the gRPC tab sends for one call. `request` is the editor state, which may be unsaved.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcExecuteDto {
    pub collection: Option<String>,
    pub request: GrpcRequest,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub environment_name: Option<String>,
    #[serde(default)]
    pub global_env_name: Option<String>,
    /// Path of the request file, so folder-level variables apply.
    #[serde(default)]
    pub request_path: Option<String>,
    /// Deadline in milliseconds. 0 or absent means none.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl GrpcExecuteDto {
    fn into_input(self, variables: HashMap<String, String>) -> GrpcExecuteInput {
        GrpcExecuteInput {
            collection: self.collection,
            request: self.request,
            message: self.message,
            variables,
            timeout: self
                .timeout_ms
                .filter(|ms| *ms > 0)
                .map(Duration::from_millis),
        }
    }
}

/// Collects the variables in scope for the call: global, collection, environment, folders,
/// and RocketVault values. The request's own variables are added by the service.
async fn resolve_input(
    dto: GrpcExecuteDto,
    exec: &RequestExecutionService,
) -> Result<GrpcExecuteInput, DomainError> {
    // RocketVault values for the environment's bindings, so `{{alias.secretName}}` resolves.
    // An environment without bindings needs no vault access.
    let secrets = exec
        .resolve_external_secrets(dto.collection.as_deref(), dto.environment_name.as_deref())
        .await?;
    let variables = exec.build_variable_context(
        dto.global_env_name.as_deref(),
        dto.collection.as_deref(),
        dto.environment_name.as_deref(),
        dto.request_path.as_deref(),
        &secrets,
    );
    Ok(dto.into_input(variables))
}

#[tauri::command]
pub async fn grpc_unary_call(
    input: GrpcExecuteDto,
    svc: State<'_, GrpcService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<GrpcUnaryResponse, DomainError> {
    let input = resolve_input(input, &exec).await?;
    svc.call_unary(input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dto_json(extra: &str) -> String {
        format!(
            r#"{{"collection": "api", "request": {{"name": "Say", "url": "localhost:50051", "methodType": "unary"}}{extra}}}"#
        )
    }

    #[test]
    fn the_dto_reads_camel_case_fields_from_the_frontend() {
        let json = dto_json(
            r#", "message": "{}", "environmentName": "dev", "globalEnvName": "g", "requestPath": "a/b.yml", "timeoutMs": 1500"#,
        );
        let dto: GrpcExecuteDto = serde_json::from_str(&json).expect("dto");
        assert_eq!(dto.environment_name.as_deref(), Some("dev"));
        assert_eq!(dto.global_env_name.as_deref(), Some("g"));
        assert_eq!(dto.request_path.as_deref(), Some("a/b.yml"));
        assert_eq!(dto.request.url, "localhost:50051");
    }

    #[test]
    fn a_zero_or_missing_timeout_means_no_deadline() {
        let zero: GrpcExecuteDto =
            serde_json::from_str(&dto_json(r#", "timeoutMs": 0"#)).expect("dto");
        assert_eq!(zero.into_input(HashMap::new()).timeout, None);
        let none: GrpcExecuteDto = serde_json::from_str(&dto_json("")).expect("dto");
        assert_eq!(none.into_input(HashMap::new()).timeout, None);
        let some: GrpcExecuteDto =
            serde_json::from_str(&dto_json(r#", "timeoutMs": 1500"#)).expect("dto");
        assert_eq!(
            some.into_input(HashMap::new()).timeout,
            Some(Duration::from_millis(1500))
        );
    }

    #[test]
    fn variables_and_message_reach_the_service_input() {
        let dto: GrpcExecuteDto =
            serde_json::from_str(&dto_json(r#", "message": "{\"a\":1}""#)).expect("dto");
        let input = dto.into_input(HashMap::from([("k".to_string(), "v".to_string())]));
        assert_eq!(input.message.as_deref(), Some("{\"a\":1}"));
        assert_eq!(input.variables.get("k").map(String::as_str), Some("v"));
        assert_eq!(input.collection.as_deref(), Some("api"));
    }
}
