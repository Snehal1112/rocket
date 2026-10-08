use rocket_app::{
    FlowExecutionService, FlowRunOptions, FlowRunSummary, FlowService, PartialRun,
    RequestExecutionService, RunFlowInput,
};
use rocket_flow::{
    Flow, FlowEdge, FlowNode, FlowNodeKind, InlineHeader, InlineRequestData, NodePosition,
    RepeatUntil, RequestSource, SwitchCase,
};
use rocket_shared::error::DomainError;
use rocket_shared::events::FlowPartialMode;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePositionDto {
    pub x: f64,
    pub y: f64,
}
impl From<NodePosition> for NodePositionDto {
    fn from(p: NodePosition) -> Self {
        Self { x: p.x, y: p.y }
    }
}
impl From<NodePositionDto> for NodePosition {
    fn from(p: NodePositionDto) -> Self {
        Self { x: p.x, y: p.y }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineHeaderDto {
    pub name: String,
    pub value: String,
}
impl From<InlineHeader> for InlineHeaderDto {
    fn from(h: InlineHeader) -> Self {
        Self {
            name: h.name,
            value: h.value,
        }
    }
}
impl From<InlineHeaderDto> for InlineHeader {
    fn from(h: InlineHeaderDto) -> Self {
        Self {
            name: h.name,
            value: h.value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineRequestDataDto {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<InlineHeaderDto>,
    #[serde(default)]
    pub body: Option<String>,
}
impl From<InlineRequestData> for InlineRequestDataDto {
    fn from(r: InlineRequestData) -> Self {
        Self {
            method: r.method,
            url: r.url,
            headers: r.headers.into_iter().map(Into::into).collect(),
            body: r.body,
        }
    }
}
impl From<InlineRequestDataDto> for InlineRequestData {
    fn from(r: InlineRequestDataDto) -> Self {
        Self {
            method: r.method,
            url: r.url,
            headers: r.headers.into_iter().map(Into::into).collect(),
            body: r.body,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepeatUntilDto {
    pub condition: String,
    pub interval_ms: u64,
    pub max_attempts: u32,
    pub timeout_ms: u64,
}
impl From<RepeatUntil> for RepeatUntilDto {
    fn from(r: RepeatUntil) -> Self {
        Self {
            condition: r.condition,
            interval_ms: r.interval_ms,
            max_attempts: r.max_attempts,
            timeout_ms: r.timeout_ms,
        }
    }
}
impl From<RepeatUntilDto> for RepeatUntil {
    fn from(r: RepeatUntilDto) -> Self {
        Self {
            condition: r.condition,
            interval_ms: r.interval_ms,
            max_attempts: r.max_attempts,
            timeout_ms: r.timeout_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum RequestSourceDto {
    Saved { request_path: String },
    Inline { request: InlineRequestDataDto },
}
impl From<RequestSource> for RequestSourceDto {
    fn from(s: RequestSource) -> Self {
        match s {
            RequestSource::Saved { request_path } => RequestSourceDto::Saved { request_path },
            RequestSource::Inline { request } => RequestSourceDto::Inline {
                request: request.into(),
            },
        }
    }
}
impl From<RequestSourceDto> for RequestSource {
    fn from(s: RequestSourceDto) -> Self {
        match s {
            RequestSourceDto::Saved { request_path } => RequestSource::Saved { request_path },
            RequestSourceDto::Inline { request } => RequestSource::Inline {
                request: request.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum FlowNodeKindDto {
    Request {
        label: String,
        source: RequestSourceDto,
        #[serde(default)]
        debug: bool,
        #[serde(default)]
        repeat_until: Option<RepeatUntilDto>,
    },
    Input {
        label: String,
        value: rocket_shared::VariableValue,
    },
    Output {
        label: String,
    },
    If {
        label: String,
        condition: String,
    },
    Switch {
        label: String,
        value: String,
        cases: Vec<SwitchCaseDto>,
    },
    WaitForCallback {
        label: String,
        name: String,
        timeout_ms: u64,
        #[serde(default)]
        accept_when: Option<String>,
    },
    Transform {
        label: String,
        script: String,
    },
    Auth {
        label: String,
        auth: rocket_shared::types::Auth,
        #[serde(default = "default_true")]
        apply_to_inherit: bool,
    },
}

fn default_true() -> bool {
    true
}
impl From<FlowNodeKind> for FlowNodeKindDto {
    fn from(k: FlowNodeKind) -> Self {
        match k {
            FlowNodeKind::Request {
                label,
                source,
                debug,
                repeat_until,
            } => FlowNodeKindDto::Request {
                label,
                source: source.into(),
                debug,
                repeat_until: repeat_until.map(Into::into),
            },
            FlowNodeKind::Input { label, value } => FlowNodeKindDto::Input { label, value },
            FlowNodeKind::Output { label } => FlowNodeKindDto::Output { label },
            FlowNodeKind::If { label, condition } => FlowNodeKindDto::If { label, condition },
            FlowNodeKind::Switch {
                label,
                value,
                cases,
            } => FlowNodeKindDto::Switch {
                label,
                value,
                cases: cases.into_iter().map(Into::into).collect(),
            },
            FlowNodeKind::WaitForCallback {
                label,
                name,
                timeout_ms,
                accept_when,
            } => FlowNodeKindDto::WaitForCallback {
                label,
                name,
                timeout_ms,
                accept_when,
            },
            FlowNodeKind::Transform { label, script } => {
                FlowNodeKindDto::Transform { label, script }
            }
            FlowNodeKind::Auth {
                label,
                auth,
                apply_to_inherit,
            } => FlowNodeKindDto::Auth {
                label,
                auth,
                apply_to_inherit,
            },
        }
    }
}
impl From<FlowNodeKindDto> for FlowNodeKind {
    fn from(k: FlowNodeKindDto) -> Self {
        match k {
            FlowNodeKindDto::Request {
                label,
                source,
                debug,
                repeat_until,
            } => FlowNodeKind::Request {
                label,
                source: source.into(),
                debug,
                repeat_until: repeat_until.map(Into::into),
            },
            FlowNodeKindDto::Input { label, value } => FlowNodeKind::Input { label, value },
            FlowNodeKindDto::Output { label } => FlowNodeKind::Output { label },
            FlowNodeKindDto::If { label, condition } => FlowNodeKind::If { label, condition },
            FlowNodeKindDto::Switch {
                label,
                value,
                cases,
            } => FlowNodeKind::Switch {
                label,
                value,
                cases: cases.into_iter().map(Into::into).collect(),
            },
            FlowNodeKindDto::WaitForCallback {
                label,
                name,
                timeout_ms,
                accept_when,
            } => FlowNodeKind::WaitForCallback {
                label,
                name,
                timeout_ms,
                accept_when,
            },
            FlowNodeKindDto::Transform { label, script } => {
                FlowNodeKind::Transform { label, script }
            }
            FlowNodeKindDto::Auth {
                label,
                auth,
                apply_to_inherit,
            } => FlowNodeKind::Auth {
                label,
                auth,
                apply_to_inherit,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchCaseDto {
    pub id: String,
    pub label: String,
    pub matches: String,
}
impl From<SwitchCase> for SwitchCaseDto {
    fn from(c: SwitchCase) -> Self {
        Self {
            id: c.id,
            label: c.label,
            matches: c.matches,
        }
    }
}
impl From<SwitchCaseDto> for SwitchCase {
    fn from(c: SwitchCaseDto) -> Self {
        Self {
            id: c.id,
            label: c.label,
            matches: c.matches,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNodeDto {
    pub id: String,
    pub kind: FlowNodeKindDto,
    pub position: NodePositionDto,
}
impl From<FlowNode> for FlowNodeDto {
    fn from(n: FlowNode) -> Self {
        Self {
            id: n.id,
            kind: n.kind.into(),
            position: n.position.into(),
        }
    }
}
impl From<FlowNodeDto> for FlowNode {
    fn from(n: FlowNodeDto) -> Self {
        Self {
            id: n.id,
            kind: n.kind.into(),
            position: n.position.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEdgeDto {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub target_field: String,
    pub expression: String,
    /// Absent in payloads from a frontend that predates routing nodes.
    #[serde(default = "default_source_handle")]
    pub source_handle: String,
}

fn default_source_handle() -> String {
    rocket_flow::handle::RESULT.to_string()
}

impl From<FlowEdge> for FlowEdgeDto {
    fn from(e: FlowEdge) -> Self {
        Self {
            id: e.id,
            source_node_id: e.source_node_id,
            target_node_id: e.target_node_id,
            target_field: e.target_field,
            expression: e.expression,
            source_handle: e.source_handle,
        }
    }
}
impl From<FlowEdgeDto> for FlowEdge {
    fn from(e: FlowEdgeDto) -> Self {
        Self {
            id: e.id,
            source_node_id: e.source_node_id,
            target_node_id: e.target_node_id,
            target_field: e.target_field,
            expression: e.expression,
            source_handle: e.source_handle,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDto {
    pub name: String,
    pub nodes: Vec<FlowNodeDto>,
    pub edges: Vec<FlowEdgeDto>,
    #[serde(default)]
    pub callback_host: Option<String>,
}
impl From<Flow> for FlowDto {
    fn from(f: Flow) -> Self {
        Self {
            name: f.name,
            nodes: f.nodes.into_iter().map(Into::into).collect(),
            edges: f.edges.into_iter().map(Into::into).collect(),
            callback_host: f.callback_host,
        }
    }
}
impl From<FlowDto> for Flow {
    fn from(f: FlowDto) -> Self {
        Self {
            name: f.name,
            nodes: f.nodes.into_iter().map(Into::into).collect(),
            edges: f.edges.into_iter().map(Into::into).collect(),
            callback_host: f.callback_host,
        }
    }
}

#[tauri::command]
pub fn list_flows(
    collection: String,
    svc: State<'_, FlowService>,
) -> Result<Vec<String>, DomainError> {
    svc.list(&collection)
}

#[tauri::command]
pub fn get_flow(
    collection: String,
    name: String,
    svc: State<'_, FlowService>,
) -> Result<FlowDto, DomainError> {
    svc.get(&collection, &name).map(FlowDto::from)
}

#[tauri::command]
pub fn delete_flow(
    collection: String,
    name: String,
    svc: State<'_, FlowService>,
) -> Result<(), DomainError> {
    svc.delete(&collection, &name)
}

#[tauri::command]
pub fn rename_flow(
    collection: String,
    old_name: String,
    new_name: String,
    svc: State<'_, FlowService>,
) -> Result<(), DomainError> {
    svc.rename(&collection, &old_name, &new_name)
}

#[tauri::command]
pub fn save_flow(
    collection: String,
    flow: FlowDto,
    svc: State<'_, FlowService>,
) -> Result<(), DomainError> {
    svc.save(&collection, flow.into())
}

/// A token the UI obtained before the run. `Debug` never shows the value.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowAuthTokenDto {
    pub access_token: String,
}
impl std::fmt::Debug for FlowAuthTokenDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FlowAuthTokenDto(<redacted>)")
    }
}

/// "Run this node" or "Run from here", on top of the run `base_run_id`.
/// It carries ids and the mode only, never cached outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialRunDto {
    pub base_run_id: String,
    pub start_node_id: String,
    /// `"node"` or `"fromHere"`.
    pub mode: FlowPartialMode,
}

impl From<PartialRunDto> for PartialRun {
    fn from(dto: PartialRunDto) -> Self {
        Self {
            base_run_id: dto.base_run_id,
            start_node_id: dto.start_node_id,
            mode: dto.mode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFlowInputDto {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
    pub global_env_name: Option<String>,
    /// Tokens the UI obtained for Auth nodes, keyed by node id. Never serialized.
    #[serde(default, skip_serializing)]
    pub auth_tokens: std::collections::HashMap<String, FlowAuthTokenDto>,
    /// The run id the frontend chose. Absent means the backend picks one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Set for a partial run. Absent for a full run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<PartialRunDto>,
}
impl RunFlowInputDto {
    /// Takes what the run needs besides the input and the tokens. Call it
    /// before `into_parts`.
    pub fn take_options(&mut self) -> FlowRunOptions {
        FlowRunOptions {
            run_id: self.run_id.take(),
            partial: self.partial.take().map(PartialRun::from),
        }
    }

    /// Splits the DTO into the run input and the tokens, so the tokens cannot
    /// be dropped by accident.
    pub fn into_parts(self) -> (RunFlowInput, rocket_app::FlowAuthTokens) {
        let tokens = self
            .auth_tokens
            .into_iter()
            .map(|(node_id, t)| {
                (
                    node_id,
                    rocket_app::SuppliedToken {
                        access_token: t.access_token,
                    },
                )
            })
            .collect();
        let input = RunFlowInput {
            collection: self.collection,
            flow_name: self.flow_name,
            environment_name: self.environment_name,
            global_env_name: self.global_env_name,
        };
        (input, tokens)
    }
}

/// Runs a Flow to completion. Streams `flow-run-started`, `flow-step-*` and
/// `flow-run-finished` events while it runs and returns the same data as one
/// summary when the run ends, mirroring `run_collection` in `runner.rs`. The
/// frontend chooses the run id (`runId`) and matches every event by it, so
/// Stop works before the run finishes and two tabs of one flow never share a
/// run. Without `runId` the backend picks one.
#[tauri::command]
pub async fn run_flow(
    mut input: RunFlowInputDto,
    flow_exec: State<'_, FlowExecutionService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<FlowRunSummary, DomainError> {
    let options = input.take_options();
    let (run_input, tokens) = input.into_parts();
    flow_exec
        .run_with_options(&exec, run_input, tokens, options)
        .await
}

/// Asks an in-progress Flow run to stop. `run_id` is the id the frontend
/// sent with `run_flow`. An unknown or already-finished run id is a no-op,
/// matching `stop_collection_run`'s existing behavior.
#[tauri::command]
pub fn cancel_flow_run(
    run_id: String,
    flow_exec: State<'_, FlowExecutionService>,
) -> Result<(), DomainError> {
    flow_exec.cancel(&run_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_flow_input_carries_auth_tokens_into_the_run() {
        let json = r#"{
            "collection": "my-api",
            "flowName": "login",
            "environmentName": null,
            "globalEnvName": null,
            "authTokens": { "a": { "accessToken": "tok-123456" } }
        }"#;
        let dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");

        let (input, tokens) = dto.into_parts();

        assert_eq!(input.collection, "my-api");
        assert_eq!(input.flow_name, "login");
        assert_eq!(
            tokens.get("a").map(|t| t.access_token.as_str()),
            Some("tok-123456")
        );
    }

    #[test]
    fn run_flow_input_without_auth_tokens_has_none() {
        let json =
            r#"{"collection":"c","flowName":"f","environmentName":null,"globalEnvName":null}"#;
        let dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");
        let (_input, tokens) = dto.into_parts();
        assert!(tokens.is_empty());
    }

    #[test]
    fn run_flow_input_carries_the_client_run_id_into_the_options() {
        let json = r#"{
            "collection": "c", "flowName": "f", "environmentName": null, "globalEnvName": null,
            "runId": "0b7e2c1a-5d1f-4a7e-9c3b-2f6d8e9a1b2c"
        }"#;
        let mut dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");

        let options = dto.take_options();

        assert_eq!(
            options.run_id.as_deref(),
            Some("0b7e2c1a-5d1f-4a7e-9c3b-2f6d8e9a1b2c")
        );
        assert!(options.partial.is_none());
        assert!(dto.run_id.is_none(), "the id is taken, not copied");
    }

    #[test]
    fn run_flow_input_carries_a_partial_run() {
        let json = r#"{
            "collection": "c",
            "flowName": "f",
            "environmentName": null,
            "globalEnvName": null,
            "partial": { "baseRunId": "01A", "startNodeId": "n2", "mode": "fromHere" }
        }"#;
        let mut dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");
        let partial = dto.take_options().partial.expect("partial");
        assert_eq!(
            partial,
            PartialRun {
                base_run_id: "01A".to_string(),
                start_node_id: "n2".to_string(),
                mode: FlowPartialMode::FromHere,
            }
        );
        assert!(dto.partial.is_none(), "the partial is taken, not copied");
    }

    #[test]
    fn run_flow_input_without_partial_is_a_full_run() {
        let json =
            r#"{"collection":"c","flowName":"f","environmentName":null,"globalEnvName":null}"#;
        let mut dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");
        assert!(dto.partial.is_none());
        assert!(dto.take_options().partial.is_none());
    }

    #[test]
    fn an_unknown_partial_mode_is_rejected() {
        let json = r#"{"collection":"c","flowName":"f","environmentName":null,"globalEnvName":null,"partial":{"baseRunId":"01A","startNodeId":"n2","mode":"everything"}}"#;
        assert!(serde_json::from_str::<RunFlowInputDto>(json).is_err());
    }

    #[test]
    fn run_flow_input_without_run_id_lets_the_backend_choose() {
        let json =
            r#"{"collection":"c","flowName":"f","environmentName":null,"globalEnvName":null}"#;
        let mut dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");
        assert!(dto.take_options().run_id.is_none());
    }

    #[test]
    fn run_flow_input_never_prints_or_serializes_a_token() {
        let json = r#"{
            "collection": "c", "flowName": "f", "environmentName": null, "globalEnvName": null,
            "authTokens": { "a": { "accessToken": "super-secret-token" } }
        }"#;
        let dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");

        assert!(
            !format!("{dto:?}").contains("super-secret-token"),
            "Debug must redact tokens"
        );
        assert!(
            !serde_json::to_string(&dto)
                .expect("serialize")
                .contains("super-secret-token"),
            "Serialize must omit tokens"
        );
    }

    fn sample_dto() -> FlowDto {
        FlowDto {
            name: "Login Then Fetch".to_string(),
            nodes: vec![
                FlowNodeDto {
                    id: "n1".to_string(),
                    kind: FlowNodeKindDto::Request {
                        label: "Login".to_string(),
                        source: RequestSourceDto::Saved {
                            request_path: "auth/login.yml".to_string(),
                        },
                        debug: false,
                        repeat_until: None,
                    },
                    position: NodePositionDto { x: 0.0, y: 0.0 },
                },
                FlowNodeDto {
                    id: "n2".to_string(),
                    kind: FlowNodeKindDto::Output {
                        label: "Result".to_string(),
                    },
                    position: NodePositionDto { x: 200.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdgeDto {
                id: "e1".to_string(),
                source_node_id: "n1".to_string(),
                target_node_id: "n2".to_string(),
                target_field: "body".to_string(),
                expression: "response.body".to_string(),
                source_handle: "result".to_string(),
            }],
            callback_host: None,
        }
    }

    #[test]
    fn flow_dto_serializes_camelcase_including_nested_fields() {
        let json = serde_json::to_string(&sample_dto()).expect("serialize FlowDto");
        assert!(
            json.contains("\"requestPath\""),
            "nested RequestSource field must be camelCase, got: {json}"
        );
        assert!(
            json.contains("\"sourceNodeId\""),
            "FlowEdgeDto field must be camelCase, got: {json}"
        );
        assert!(
            json.contains("\"targetField\""),
            "FlowEdgeDto field must be camelCase, got: {json}"
        );
        assert!(
            json.contains("\"type\":\"Saved\""),
            "RequestSource tag value must stay 'Saved', not be renamed by rename_all_fields, got: {json}"
        );
        assert!(
            json.contains("\"kind\":\"Request\""),
            "FlowNodeKind tag value must stay 'Request', not be renamed by rename_all_fields, got: {json}"
        );
    }

    #[test]
    fn request_debug_flag_converts_both_ways_and_defaults_to_false() {
        let dto = FlowNodeKindDto::Request {
            label: "Login".to_string(),
            source: RequestSourceDto::Saved {
                request_path: "auth/login.yml".to_string(),
            },
            debug: true,
            repeat_until: None,
        };
        let domain: FlowNodeKind = dto.into();
        assert!(matches!(domain, FlowNodeKind::Request { debug: true, .. }));
        let back: FlowNodeKindDto = domain.into();
        assert!(matches!(back, FlowNodeKindDto::Request { debug: true, .. }));

        let json =
            r#"{"kind":"Request","label":"L","source":{"type":"Saved","requestPath":"a.yml"}}"#;
        let parsed: FlowNodeKindDto = serde_json::from_str(json).expect("deserialize");
        assert!(matches!(
            parsed,
            FlowNodeKindDto::Request { debug: false, .. }
        ));
    }

    #[test]
    fn request_repeat_until_converts_both_ways_and_defaults_to_none() {
        let dto = FlowNodeKindDto::Request {
            label: "Poll".to_string(),
            source: RequestSourceDto::Saved {
                request_path: "jobs/get.yml".to_string(),
            },
            debug: false,
            repeat_until: Some(RepeatUntilDto {
                condition: "response.body.done".to_string(),
                interval_ms: 500,
                max_attempts: 4,
                timeout_ms: 3000,
            }),
        };
        let json = serde_json::to_string(&dto).expect("serialize");
        assert!(json.contains("\"repeatUntil\""), "got: {json}");
        assert!(json.contains("\"intervalMs\":500"), "got: {json}");
        assert!(json.contains("\"maxAttempts\":4"), "got: {json}");

        let domain: FlowNodeKind = dto.into();
        let FlowNodeKind::Request {
            repeat_until: Some(r),
            ..
        } = &domain
        else {
            panic!("repeat_until must survive the conversion, got {domain:?}");
        };
        assert_eq!(r.timeout_ms, 3000);
        let back: FlowNodeKindDto = domain.into();
        assert!(matches!(
            back,
            FlowNodeKindDto::Request {
                repeat_until: Some(RepeatUntilDto {
                    max_attempts: 4,
                    ..
                }),
                ..
            }
        ));

        let old =
            r#"{"kind":"Request","label":"L","source":{"type":"Saved","requestPath":"a.yml"}}"#;
        let parsed: FlowNodeKindDto = serde_json::from_str(old).expect("deserialize");
        assert!(matches!(
            parsed,
            FlowNodeKindDto::Request {
                repeat_until: None,
                ..
            }
        ));
    }

    #[test]
    fn flow_dto_roundtrips_through_domain_type() {
        let dto = sample_dto();
        let domain: Flow = dto.clone().into();
        let back: FlowDto = domain.into();
        let a = serde_json::to_value(&dto).expect("to_value dto");
        let b = serde_json::to_value(&back).expect("to_value back");
        assert_eq!(a, b);
    }

    #[test]
    fn save_flow_then_reopening_with_a_fresh_repo_instance_returns_the_same_flow() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        std::fs::create_dir_all(dir.path().join("acme")).expect("create collection dir");
        let dto = sample_dto();

        {
            let svc = rocket_app::FlowService::new(Box::new(rocket_infra::FsFlowRepo::new(
                dir.path().to_path_buf(),
            )));
            svc.save("acme", dto.clone().into()).expect("save flow");
        }

        // A fresh repo/service instance over the same directory simulates
        // reopening the app, rather than reusing the instance that saved.
        let svc2 = rocket_app::FlowService::new(Box::new(rocket_infra::FsFlowRepo::new(
            dir.path().to_path_buf(),
        )));
        let loaded = svc2.get("acme", &dto.name).expect("get flow after reopen");
        let loaded_dto: FlowDto = loaded.into();

        assert_eq!(
            serde_json::to_value(&loaded_dto).expect("serialize loaded"),
            serde_json::to_value(&dto).expect("serialize original"),
            "the flow saved by one instance must round-trip identically through a fresh instance"
        );
    }

    #[test]
    fn saving_a_flow_makes_the_file_visible_as_untracked_in_git_status() {
        use rocket_git::{GitService, GitStatus};

        let dir = tempfile::TempDir::new().expect("create temp dir");
        std::fs::create_dir_all(dir.path().join("acme")).expect("create collection dir");

        let git = rocket_git::Git2Service::new();
        let repo_path = dir.path().to_str().expect("utf8 path");
        git.init(repo_path).expect("init git repo");

        let svc = rocket_app::FlowService::new(Box::new(rocket_infra::FsFlowRepo::new(
            dir.path().to_path_buf(),
        )));
        svc.save("acme", sample_dto().into()).expect("save flow");

        let status = git.status(repo_path).expect("git status");
        let flow_file = status
            .files
            .iter()
            .find(|f| f.path.ends_with("login-then-fetch.yml"))
            .unwrap_or_else(|| {
                panic!(
                    "expected the saved flow file to appear in git status, got: {:?}",
                    status.files
                )
            });
        assert_eq!(flow_file.status, GitStatus::Untracked);
    }

    fn routing_dto() -> FlowDto {
        FlowDto {
            name: "Routing".to_string(),
            nodes: vec![
                FlowNodeDto {
                    id: "if1".to_string(),
                    kind: FlowNodeKindDto::If {
                        label: "Logged in?".to_string(),
                        condition: "response.status === 200".to_string(),
                    },
                    position: NodePositionDto { x: 0.0, y: 0.0 },
                },
                FlowNodeDto {
                    id: "sw1".to_string(),
                    kind: FlowNodeKindDto::Switch {
                        label: "Plan router".to_string(),
                        value: "response.body.plan".to_string(),
                        cases: vec![SwitchCaseDto {
                            id: "c1".to_string(),
                            label: "Pro plan".to_string(),
                            matches: "pro".to_string(),
                        }],
                    },
                    position: NodePositionDto { x: 200.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdgeDto {
                id: "e1".to_string(),
                source_node_id: "if1".to_string(),
                target_node_id: "sw1".to_string(),
                target_field: "input".to_string(),
                expression: String::new(),
                source_handle: "true".to_string(),
            }],
            callback_host: None,
        }
    }

    #[test]
    fn routing_node_dtos_keep_tag_values_and_camel_case_fields() {
        let json = serde_json::to_string(&routing_dto()).expect("serialize FlowDto");
        assert!(json.contains(r#""kind":"If""#), "got: {json}");
        assert!(json.contains(r#""kind":"Switch""#), "got: {json}");
        assert!(
            json.contains(r#""condition":"response.status === 200""#),
            "got: {json}"
        );
        assert!(
            json.contains(r#""cases":[{"id":"c1","label":"Pro plan","matches":"pro"}]"#),
            "got: {json}"
        );
    }

    #[test]
    fn routing_node_dtos_roundtrip_through_domain_type() {
        let dto = routing_dto();
        let domain: Flow = dto.clone().into();
        assert!(matches!(domain.nodes[0].kind, FlowNodeKind::If { .. }));
        match &domain.nodes[1].kind {
            FlowNodeKind::Switch { cases, .. } => assert_eq!(cases[0].matches, "pro"),
            other => panic!("expected a Switch node, got {other:?}"),
        }
        let back: FlowDto = domain.into();
        let a = serde_json::to_value(&dto).expect("to_value dto");
        let b = serde_json::to_value(&back).expect("to_value back");
        assert_eq!(a, b);
    }

    #[test]
    fn transform_node_dto_keeps_tag_and_roundtrips() {
        let dto = FlowDto {
            name: "Transform".to_string(),
            nodes: vec![FlowNodeDto {
                id: "t1".to_string(),
                kind: FlowNodeKindDto::Transform {
                    label: "Pick token".to_string(),
                    script: "return response.body.token;".to_string(),
                },
                position: NodePositionDto { x: 0.0, y: 0.0 },
            }],
            edges: vec![],
            callback_host: None,
        };
        let json = serde_json::to_string(&dto).expect("serialize FlowDto");
        assert!(json.contains(r#""kind":"Transform""#), "got: {json}");
        assert!(
            json.contains(r#""script":"return response.body.token;""#),
            "got: {json}"
        );
        let domain: Flow = dto.clone().into();
        assert!(matches!(
            domain.nodes[0].kind,
            FlowNodeKind::Transform { .. }
        ));
        let back: FlowDto = domain.into();
        let a = serde_json::to_value(&dto).expect("to_value dto");
        let b = serde_json::to_value(&back).expect("to_value back");
        assert_eq!(a, b);
    }

    #[test]
    fn auth_node_dto_uses_camel_case_and_roundtrips() {
        let kind = FlowNodeKind::Auth {
            label: "Sign in".to_string(),
            auth: rocket_shared::types::Auth::Bearer {
                token: "t".to_string(),
            },
            apply_to_inherit: false,
        };
        let dto: FlowNodeKindDto = kind.clone().into();
        let json = serde_json::to_string(&dto).expect("serialize FlowNodeKindDto");
        assert!(json.contains(r#""kind":"Auth""#), "got: {json}");
        assert!(json.contains(r#""applyToInherit":false"#), "got: {json}");
        assert!(json.contains(r#""authType":"bearer""#), "got: {json}");
        let back: FlowNodeKindDto = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(FlowNodeKind::from(back), kind);
    }

    #[test]
    fn auth_node_dto_defaults_apply_to_inherit_to_true() {
        let json = r#"{"kind":"Auth","label":"Sign in","auth":{"authType":"basic","username":"u","password":"p"}}"#;
        let dto: FlowNodeKindDto = serde_json::from_str(json).expect("deserialize");
        match FlowNodeKind::from(dto) {
            FlowNodeKind::Auth {
                apply_to_inherit, ..
            } => assert!(apply_to_inherit),
            other => panic!("expected an Auth node, got {other:?}"),
        }
    }

    #[test]
    fn flow_edge_dto_without_source_handle_defaults_to_result() {
        let json = r#"{"id":"e1","sourceNodeId":"a","targetNodeId":"b","targetField":"url","expression":"response.body"}"#;
        let dto: FlowEdgeDto = serde_json::from_str(json).expect("deserialize FlowEdgeDto");
        assert_eq!(dto.source_handle, "result");
        let domain: FlowEdge = dto.into();
        assert_eq!(domain.source_handle, rocket_flow::handle::RESULT);
    }

    #[test]
    fn flow_edge_dto_serializes_source_handle_as_camel_case() {
        let mut dto = sample_dto();
        dto.edges[0].source_handle = "true".to_string();
        let json = serde_json::to_string(&dto).expect("serialize FlowDto");
        assert!(json.contains(r#""sourceHandle":"true""#), "got: {json}");
        let domain: Flow = dto.into();
        assert_eq!(domain.edges[0].source_handle, "true");
    }

    #[test]
    fn wait_for_callback_dto_uses_camel_case_json_and_roundtrips() {
        let json = r#"{
            "name": "cb",
            "callbackHost": "host.docker.internal",
            "nodes": [{
                "id": "w",
                "kind": { "kind": "WaitForCallback", "label": "Hook", "name": "payment",
                          "timeoutMs": 60000, "acceptWhen": "request.body.ok" },
                "position": { "x": 0, "y": 0 }
            }],
            "edges": []
        }"#;
        let dto: FlowDto = serde_json::from_str(json).expect("parse");
        let flow: Flow = dto.into();
        assert_eq!(flow.callback_host.as_deref(), Some("host.docker.internal"));
        assert_eq!(
            flow.nodes[0].kind,
            FlowNodeKind::WaitForCallback {
                label: "Hook".to_string(),
                name: "payment".to_string(),
                timeout_ms: 60_000,
                accept_when: Some("request.body.ok".to_string()),
            }
        );
        let back = serde_json::to_string(&FlowDto::from(flow)).expect("serialize");
        assert!(back.contains("\"timeoutMs\":60000"), "got: {back}");
        assert!(back.contains("\"callbackHost\""), "got: {back}");
    }
}
