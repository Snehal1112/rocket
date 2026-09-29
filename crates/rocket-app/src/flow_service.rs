use rocket_flow::{validate, Flow, FlowGraphError, FlowRepository};
use rocket_shared::error::{DomainError, DomainResult};

pub struct FlowService {
    flow_repo: Box<dyn FlowRepository>,
}

impl FlowService {
    pub fn new(flow_repo: Box<dyn FlowRepository>) -> Self {
        Self { flow_repo }
    }

    pub fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        self.flow_repo.list(collection)
    }

    pub fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        self.flow_repo.get(collection, name)
    }

    pub fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        self.flow_repo.delete(collection, name)
    }

    pub fn save(&self, collection: &str, flow: Flow) -> DomainResult<()> {
        validate(&flow).map_err(|e| DomainError::InvalidInput(graph_error_message(e)))?;
        self.flow_repo.save(collection, &flow)
    }
}

/// Builds the save-error text. Every message that names graph elements ends
/// with "node(s): <ids>; edge(s): <ids>", which the canvas parses to
/// highlight them in red.
fn graph_error_message(error: FlowGraphError) -> String {
    match error {
        FlowGraphError::Cycle { node_ids, edge_ids } => format!(
            "flow contains a cycle through node(s): {}; edge(s): {}",
            node_ids.join(", "),
            edge_ids.join(", ")
        ),
        FlowGraphError::UnknownNode { node_id } => {
            format!("edge references unknown node: {node_id}")
        }
        FlowGraphError::DuplicateNode { node_id } => {
            format!("flow has more than one node with id: {node_id}")
        }
        FlowGraphError::InvalidNode { node_id, reason } => {
            format!("flow is invalid: {reason} — node(s): {node_id}; edge(s): ")
        }
        FlowGraphError::InvalidEdge { edge_id, reason } => {
            format!("flow is invalid: {reason} — node(s): ; edge(s): {edge_id}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::{FlowEdge, FlowNode, FlowNodeKind, NodePosition};
    use std::sync::Mutex;

    fn sample_flow() -> Flow {
        Flow {
            name: "Login Then Fetch".to_string(),
            nodes: vec![FlowNode {
                id: "n1".to_string(),
                kind: FlowNodeKind::Output {
                    label: "Result".to_string(),
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: vec![],
            callback_host: None,
        }
    }

    struct FakeFlowRepo {
        flows: Mutex<Vec<(String, Flow)>>, // (collection, flow)
    }
    impl FakeFlowRepo {
        fn new() -> Self {
            Self {
                flows: Mutex::new(Vec::new()),
            }
        }
        fn seeded(collection: &str, flow: Flow) -> Self {
            let repo = Self::new();
            repo.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .push((collection.to_string(), flow));
            repo
        }
    }
    impl FlowRepository for FakeFlowRepo {
        fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
            Ok(self
                .flows
                .lock()
                .expect("lock FakeFlowRepo")
                .iter()
                .filter(|(c, _)| c == collection)
                .map(|(_, f)| f.name.clone())
                .collect())
        }
        fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .iter()
                .find(|(c, f)| c == collection && f.name == name)
                .map(|(_, f)| f.clone())
                .ok_or_else(|| {
                    rocket_shared::error::DomainError::NotFound(format!(
                        "flow '{name}' not found in collection '{collection}'"
                    ))
                })
        }
        fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
            let mut guard = self.flows.lock().expect("lock FakeFlowRepo");
            guard.retain(|(c, f)| !(c == collection && f.name == flow.name));
            guard.push((collection.to_string(), flow.clone()));
            Ok(())
        }
        fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .retain(|(c, f)| !(c == collection && f.name == name));
            Ok(())
        }
    }

    #[test]
    fn list_returns_flow_names_for_collection() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        assert_eq!(
            svc.list("demo").expect("list"),
            vec!["Login Then Fetch".to_string()]
        );
        assert_eq!(svc.list("other").expect("list other"), Vec::<String>::new());
    }

    #[test]
    fn get_returns_not_found_for_missing_flow() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
        let err = svc.get("demo", "missing").expect_err("expected NotFound");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::NotFound(_)
        ));
    }

    #[test]
    fn delete_removes_flow() {
        let repo = FakeFlowRepo::seeded("demo", sample_flow());
        let svc = FlowService::new(Box::new(repo));
        svc.delete("demo", "Login Then Fetch").expect("delete");
        assert!(svc.get("demo", "Login Then Fetch").is_err());
    }

    fn cyclic_flow() -> Flow {
        Flow {
            name: "Cyclic".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "A".to_string(),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "b".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "B".to_string(),
                    },
                    position: NodePosition { x: 100.0, y: 0.0 },
                },
            ],
            edges: vec![
                FlowEdge {
                    id: "e1".to_string(),
                    source_node_id: "a".to_string(),
                    target_node_id: "b".to_string(),
                    target_field: "body".to_string(),
                    expression: "response.body".to_string(),
                    source_handle: rocket_flow::handle::RESULT.to_string(),
                },
                FlowEdge {
                    id: "e2".to_string(),
                    source_node_id: "b".to_string(),
                    target_node_id: "a".to_string(),
                    target_field: "body".to_string(),
                    expression: "response.body".to_string(),
                    source_handle: rocket_flow::handle::RESULT.to_string(),
                },
            ],
            callback_host: None,
        }
    }

    struct PanicsOnSaveRepo;
    impl FlowRepository for PanicsOnSaveRepo {
        fn list(&self, _collection: &str) -> DomainResult<Vec<String>> {
            Ok(Vec::new())
        }
        fn get(&self, _collection: &str, name: &str) -> DomainResult<Flow> {
            Err(rocket_shared::error::DomainError::NotFound(
                name.to_string(),
            ))
        }
        fn save(&self, _collection: &str, _flow: &Flow) -> DomainResult<()> {
            panic!("save must not be called for a cyclic flow");
        }
        fn delete(&self, _collection: &str, _name: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    #[test]
    fn save_rejects_cyclic_graph_and_names_the_nodes() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let err = svc
            .save("demo", cyclic_flow())
            .expect_err("cyclic flow must be rejected");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::InvalidInput(_)
        ));
        let message = err.to_string();

        // Parse the node id list between "node(s): " and the "; edge(s):"
        // separator, so single letters inside the message prose cannot
        // satisfy the check by accident.
        let node_segment = message
            .split("node(s): ")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .unwrap_or_else(|| panic!("error should list the cyclic nodes, got: {message}"));
        let mut ids: Vec<&str> = node_segment.split(", ").collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["a", "b"], "got: {message}");

        let edge_segment = message
            .split("edge(s): ")
            .nth(1)
            .unwrap_or_else(|| panic!("error should list the cyclic edges, got: {message}"));
        let mut edge_ids: Vec<&str> = edge_segment.split(", ").collect();
        edge_ids.sort_unstable();
        assert_eq!(edge_ids, vec!["e1", "e2"], "got: {message}");
    }

    #[test]
    fn save_persists_an_acyclic_flow() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
        svc.save("demo", sample_flow()).expect("save acyclic flow");
        assert_eq!(
            svc.list("demo").expect("list"),
            vec!["Login Then Fetch".to_string()]
        );
    }

    fn node_of(id: &str, kind: FlowNodeKind) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    #[test]
    fn save_rejects_an_if_node_without_input_and_names_it_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Routing".to_string(),
            nodes: vec![node_of(
                "if1",
                FlowNodeKind::If {
                    label: "Logged in?".to_string(),
                    condition: "response.status === 200".to_string(),
                },
            )],
            edges: vec![],
            callback_host: None,
        };
        let err = svc
            .save("demo", flow)
            .expect_err("invalid flow must be rejected");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::InvalidInput(_)
        ));
        let message = err.to_string();
        assert!(message.contains("flow is invalid: "), "got: {message}");
        assert!(
            message.ends_with("node(s): if1; edge(s): "),
            "got: {message}"
        );
    }

    #[test]
    fn save_rejects_a_bad_edge_and_names_it_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let output = |id: &str| {
            node_of(
                id,
                FlowNodeKind::Output {
                    label: id.to_string(),
                },
            )
        };
        let flow = Flow {
            name: "Bad Edge".to_string(),
            nodes: vec![output("a"), output("b")],
            edges: vec![FlowEdge {
                id: "e9".to_string(),
                source_node_id: "a".to_string(),
                target_node_id: "b".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an edge out of an Output node must be rejected")
            .to_string();
        assert!(
            message.ends_with("node(s): ; edge(s): e9"),
            "got: {message}"
        );
    }
}
