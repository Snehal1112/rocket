use rocket_app::FlowService;
use rocket_flow::{
    Flow, FlowEdge, FlowNode, FlowNodeKind, InlineHeader, InlineRequestData, NodePosition,
    RequestSource,
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
    },
    Input {
        label: String,
        value: rocket_shared::VariableValue,
    },
    Output {
        label: String,
    },
}
impl From<FlowNodeKind> for FlowNodeKindDto {
    fn from(k: FlowNodeKind) -> Self {
        match k {
            FlowNodeKind::Request { label, source } => FlowNodeKindDto::Request {
                label,
                source: source.into(),
            },
            FlowNodeKind::Input { label, value } => FlowNodeKindDto::Input { label, value },
            FlowNodeKind::Output { label } => FlowNodeKindDto::Output { label },
        }
    }
}
impl From<FlowNodeKindDto> for FlowNodeKind {
    fn from(k: FlowNodeKindDto) -> Self {
        match k {
            FlowNodeKindDto::Request { label, source } => FlowNodeKind::Request {
                label,
                source: source.into(),
            },
            FlowNodeKindDto::Input { label, value } => FlowNodeKind::Input { label, value },
            FlowNodeKindDto::Output { label } => FlowNodeKind::Output { label },
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
}
impl From<FlowEdge> for FlowEdgeDto {
    fn from(e: FlowEdge) -> Self {
        Self {
            id: e.id,
            source_node_id: e.source_node_id,
            target_node_id: e.target_node_id,
            target_field: e.target_field,
            expression: e.expression,
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
    fn flow_dto_roundtrips_through_domain_type() {
        let dto = sample_dto();
        let domain: Flow = dto.clone().into();
        let back: FlowDto = domain.into();
        let a = serde_json::to_value(&dto).expect("to_value dto");
        let b = serde_json::to_value(&back).expect("to_value back");
        assert_eq!(a, b);
    }
}
