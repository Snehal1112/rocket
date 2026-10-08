use rocket_flow::lint::{graph_error_lints, validate_with_warnings, FlowLint, NoLintContext};
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

    /// Renames a flow. The new name is trimmed. A blank or unchanged name is
    /// rejected here, so the repository only sees real renames.
    pub fn rename(&self, collection: &str, old_name: &str, new_name: &str) -> DomainResult<()> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(DomainError::InvalidInput(
                "Flow name must not be empty".to_string(),
            ));
        }
        if new_name == old_name {
            return Err(DomainError::InvalidInput(
                "The new flow name is the same as the current one".to_string(),
            ));
        }
        self.flow_repo.rename(collection, old_name, new_name)
    }

    pub fn save(&self, collection: &str, flow: Flow) -> DomainResult<()> {
        validate(&flow).map_err(|e| DomainError::InvalidInput(graph_error_message(e)))?;
        self.flow_repo.save(collection, &flow)
    }

    /// Lints `flow` as given, not the saved file, and never fails. A
    /// structural `validate` failure comes first, as `invalid_graph` error
    /// lints that carry its node or edge ids. Then come the warnings of
    /// `validate_with_warnings`. Lints never block save or run.
    pub fn lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint> {
        // F-21 and F-22 build a context from `collection` here, for saved
        // requests and known variables. Until then no lint needs one.
        let _ = collection;
        let mut lints = match validate(flow) {
            Ok(_) => Vec::new(),
            Err(error) => graph_error_lints(flow, &error),
        };
        lints.extend(validate_with_warnings(flow, &NoLintContext));
        lints
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
    use rocket_flow::lint::{LintSeverity, EXIT_WITHOUT_EDGE, INVALID_GRAPH, NO_PATH_TO_OUTPUT};

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

    fn named_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            ..sample_flow()
        }
    }

    #[test]
    fn rename_moves_the_flow_and_trims_the_new_name() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        svc.rename("demo", "Login Then Fetch", "  Sign In  ")
            .expect("rename");
        assert_eq!(svc.list("demo").expect("list"), vec!["Sign In".to_string()]);
        assert_eq!(svc.get("demo", "Sign In").expect("get").name, "Sign In");
    }

    #[test]
    fn rename_rejects_a_blank_name_without_touching_the_repo() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        let err = svc
            .rename("demo", "Login Then Fetch", "   ")
            .expect_err("blank name");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::InvalidInput(_)
        ));
        assert!(svc.get("demo", "Login Then Fetch").is_ok());
    }

    #[test]
    fn rename_rejects_an_unchanged_name_even_with_surrounding_spaces() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        let err = svc
            .rename("demo", "Login Then Fetch", " Login Then Fetch ")
            .expect_err("unchanged name");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::InvalidInput(_)
        ));
        assert!(svc.get("demo", "Login Then Fetch").is_ok());
    }

    #[test]
    fn rename_onto_an_existing_flow_is_a_conflict_and_keeps_both() {
        let repo = FakeFlowRepo::seeded("demo", sample_flow());
        repo.save("demo", &named_flow("Other")).expect("seed other");
        let svc = FlowService::new(Box::new(repo));
        let err = svc
            .rename("demo", "Login Then Fetch", "Other")
            .expect_err("target exists");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::Conflict(_)
        ));
        assert_eq!(svc.list("demo").expect("list").len(), 2);
    }

    #[test]
    fn rename_of_a_missing_flow_is_not_found() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
        let err = svc
            .rename("demo", "missing", "Anything")
            .expect_err("missing flow");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::NotFound(_)
        ));
    }

    /// Panics on every call, so a test proves lint never reads or writes the repo.
    struct UntouchedRepo;
    impl FlowRepository for UntouchedRepo {
        fn list(&self, _collection: &str) -> DomainResult<Vec<String>> {
            panic!("lint must not list flows");
        }
        fn get(&self, _collection: &str, _name: &str) -> DomainResult<Flow> {
            panic!("lint must not read the saved flow");
        }
        fn save(&self, _collection: &str, _flow: &Flow) -> DomainResult<()> {
            panic!("lint must not save");
        }
        fn delete(&self, _collection: &str, _name: &str) -> DomainResult<()> {
            panic!("lint must not delete");
        }
    }

    fn wire(id: &str, from: &str, exit: &str, to: &str, field: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: to.to_string(),
            target_field: field.to_string(),
            expression: String::new(),
            source_handle: exit.to_string(),
        }
    }

    /// in -> if1; if1.true -> out. The 'false' exit has no wire.
    fn routing_flow() -> Flow {
        Flow {
            name: "Routing".to_string(),
            nodes: vec![
                node_of(
                    "in",
                    FlowNodeKind::Input {
                        label: "Status".to_string(),
                        value: rocket_shared::VariableValue::simple("200"),
                    },
                ),
                node_of(
                    "if1",
                    FlowNodeKind::If {
                        label: "Check status".to_string(),
                        condition: "response === '200'".to_string(),
                    },
                ),
                node_of(
                    "out",
                    FlowNodeKind::Output {
                        label: "Result".to_string(),
                    },
                ),
            ],
            edges: vec![
                wire("e1", "in", rocket_flow::handle::RESULT, "if1", rocket_flow::handle::INPUT),
                wire("e2", "if1", rocket_flow::handle::TRUE, "out", "value"),
            ],
            callback_host: None,
        }
    }

    fn lint_keys(lints: &[FlowLint]) -> Vec<(Option<&str>, Option<&str>, &str)> {
        lints
            .iter()
            .map(|l| (l.node_id.as_deref(), l.edge_id.as_deref(), l.code.as_str()))
            .collect()
    }

    #[test]
    fn lint_reads_only_the_given_graph() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let lints = svc.lint("demo", &routing_flow());
        assert_eq!(lint_keys(&lints), vec![(Some("if1"), None, EXIT_WITHOUT_EDGE)]);
        assert_eq!(lints[0].severity, LintSeverity::Warning);
        assert_eq!(
            lints[0].message,
            "The 'false' exit of 'Check status' has no wire."
        );
    }

    #[test]
    fn lint_puts_structural_errors_first_and_is_stable() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let mut flow = cyclic_flow();
        flow.nodes.push(node_of(
            "c",
            FlowNodeKind::Input {
                label: "Lonely".to_string(),
                value: rocket_shared::VariableValue::simple("x"),
            },
        ));
        let lints = svc.lint("demo", &flow);
        assert_eq!(
            lint_keys(&lints),
            vec![
                (Some("a"), None, INVALID_GRAPH),
                (Some("b"), None, INVALID_GRAPH),
                (None, Some("e1"), INVALID_GRAPH),
                (None, Some("e2"), INVALID_GRAPH),
                (Some("c"), None, NO_PATH_TO_OUTPUT),
            ]
        );
        assert!(lints[..4].iter().all(|l| l.severity == LintSeverity::Error));
        assert_eq!(lints[4].severity, LintSeverity::Warning);
        assert_eq!(svc.lint("demo", &flow), lints);
    }

    #[test]
    fn lint_names_an_invalid_node_by_its_label() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
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
        let lints = svc.lint("demo", &flow);
        assert_eq!(lints[0].code, INVALID_GRAPH);
        assert_eq!(lints[0].node_id.as_deref(), Some("if1"));
        assert!(
            lints[0].message.starts_with("'Logged in?': "),
            "got: {}",
            lints[0].message
        );
        assert!(lints[1..].iter().all(|l| l.severity == LintSeverity::Warning));
    }

    #[test]
    fn a_flow_with_warnings_still_saves_unchanged() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
        let flow = routing_flow();
        assert!(!svc.lint("demo", &flow).is_empty());
        svc.save("demo", flow.clone())
            .expect("a flow with warnings must still save");
        assert_eq!(svc.get("demo", "Routing").expect("get"), flow);
    }
}
