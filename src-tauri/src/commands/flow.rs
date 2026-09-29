use rocket_app::{
    FlowExecutionService, FlowRunSummary, FlowService, RequestExecutionService, RunFlowInput,
};
use rocket_flow::{
    Flow, FlowEdge, FlowNode, FlowNodeKind, InlineHeader, InlineRequestData, NodePosition,
    RequestSource, SwitchCase,
};
use rocket_shared::error::DomainError;
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
}
impl From<FlowNodeKind> for FlowNodeKindDto {
    fn from(k: FlowNodeKind) -> Self {
        match k {
            FlowNodeKind::Request {
                label,
                source,
                debug,
                repeat_until: _,
            } => FlowNodeKindDto::Request {
                label,
                source: source.into(),
                debug,
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
            } => FlowNodeKind::Request {
                label,
                source: source.into(),
                debug,
                repeat_until: None,
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
}
impl From<Flow> for FlowDto {
    fn from(f: Flow) -> Self {
        Self {
            name: f.name,
            nodes: f.nodes.into_iter().map(Into::into).collect(),
            edges: f.edges.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<FlowDto> for Flow {
    fn from(f: FlowDto) -> Self {
        Self {
            name: f.name,
            nodes: f.nodes.into_iter().map(Into::into).collect(),
            edges: f.edges.into_iter().map(Into::into).collect(),
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
pub fn save_flow(
    collection: String,
    flow: FlowDto,
    svc: State<'_, FlowService>,
) -> Result<(), DomainError> {
    svc.save(&collection, flow.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFlowInputDto {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
    pub global_env_name: Option<String>,
}
impl From<RunFlowInputDto> for RunFlowInput {
    fn from(i: RunFlowInputDto) -> Self {
        Self {
            collection: i.collection,
            flow_name: i.flow_name,
            environment_name: i.environment_name,
            global_env_name: i.global_env_name,
        }
    }
}

/// Runs a Flow to completion. Streams `flow-run-started`,
/// `flow-step-completed`, and `flow-run-finished` events while it runs
/// (`FlowExecutionService::run` publishes these through the injected
/// `TauriEventBus` as it goes) and returns the same data as one summary when
/// the run ends — mirroring `run_collection` in `runner.rs` exactly. The
/// frontend reads `run_id` off the `flow-run-started` event payload, not off
/// this command's return value, so Stop is available before the run finishes.
#[tauri::command]
pub async fn run_flow(
    input: RunFlowInputDto,
    flow_exec: State<'_, FlowExecutionService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<FlowRunSummary, DomainError> {
    flow_exec.run(&exec, input.into()).await
}

/// Asks an in-progress Flow run to stop. An unknown or already-finished run
/// id is a no-op, matching `stop_collection_run`'s existing behavior.
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
}
