use crate::flow_lint_context::{CollectionLintContext, LintEnvironment, LintSources};
use rocket_flow::describe::{display_label, wire_ends};
use rocket_flow::lint::{graph_error_lints, validate_with_warnings, FlowLint, NoLintContext};
use rocket_flow::{
    validate_report, Flow, FlowGraphError, FlowListEntry, FlowNode, FlowRepository,
    ValidationReport,
};
use rocket_shared::error::{DomainError, DomainResult};
use std::collections::HashSet;

pub struct FlowService {
    flow_repo: Box<dyn FlowRepository>,
    /// Where lints read the collection and the environments. Without it no
    /// lint that needs them can run.
    lint_sources: Option<LintSources>,
}

impl FlowService {
    pub fn new(flow_repo: Box<dyn FlowRepository>) -> Self {
        Self {
            flow_repo,
            lint_sources: None,
        }
    }

    /// Lets lints ask about saved requests and variables.
    pub fn with_lint_sources(mut self, sources: LintSources) -> Self {
        self.lint_sources = Some(sources);
        self
    }

    /// The repository, for in-crate helpers that must bypass `validate`.
    pub(crate) fn repo(&self) -> &dyn FlowRepository {
        self.flow_repo.as_ref()
    }

    pub fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        self.flow_repo.list(collection)
    }

    /// Like `list`, but a flow file that cannot be read is included as a corrupt entry.
    pub fn list_entries(&self, collection: &str) -> DomainResult<Vec<FlowListEntry>> {
        self.flow_repo.list_entries(collection)
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
        let report = validate_report(&flow);
        if !report.errors.is_empty() {
            return Err(DomainError::InvalidInput(graph_errors_message(
                &flow, &report,
            )));
        }
        self.flow_repo.save(collection, &flow)
    }

    /// Lints `flow` with no environment selected. See `lint_with_environment`.
    pub fn lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint> {
        self.lint_with_environment(collection, flow, LintEnvironment::default())
    }

    /// Lints `flow` as given, not the saved file, and never fails. Every
    /// structural `validate` failure comes first, as `invalid_graph` error
    /// lints that carry their node or edge ids, in the order `validate_all`
    /// found them. Then come the warnings of
    /// `validate_with_warnings`, which ask a context built from `env`. Without
    /// lint sources the context knows nothing and those rules stay silent.
    /// Lints never block save or run.
    pub fn lint_with_environment(
        &self,
        collection: &str,
        flow: &Flow,
        env: LintEnvironment<'_>,
    ) -> Vec<FlowLint> {
        let mut lints: Vec<FlowLint> = validate_report(flow)
            .errors
            .iter()
            .flat_map(|error| graph_error_lints(flow, error))
            .collect();
        match &self.lint_sources {
            Some(sources) => {
                let ctx = CollectionLintContext::new(sources, collection, env, flow);
                lints.extend(validate_with_warnings(flow, &ctx));
            }
            None => lints.extend(validate_with_warnings(flow, &NoLintContext)),
        }
        lints
    }
}

/// Most characters of a label or id that go into a save message.
const MAX_MESSAGE_CHARS: usize = 60;

/// Text from the flow file, made safe to put in the save message. It drops
/// what could break a `[nodes ...; edges ...]` tag or fake the final
/// `node(s):` list, and it cuts long text.
fn message_safe(text: &str) -> String {
    strip_unsafe(text).chars().take(MAX_MESSAGE_CHARS).collect()
}

/// Text with the characters that could break the tag or tail format dropped.
fn strip_unsafe(text: &str) -> String {
    let kept: String = text
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ';' | '[' | ']'))
        .collect();
    defang(&kept)
}

/// Drops what could fake the `node(s):` list or a `[nodes ...]` tag from text
/// that may keep its own `;` and brackets: control characters, the `(s):`
/// sequence and the `[nodes` opener.
fn defang(text: &str) -> String {
    let kept: String = text.chars().filter(|c| !c.is_control()).collect();
    kept.replace("(s):", "(s)-").replace("[nodes", "(nodes")
}

/// Ids joined for a tag or tail, each one made safe first.
fn join_ids<S: AsRef<str>>(ids: &[S]) -> String {
    ids.iter()
        .map(|id| strip_unsafe(id.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One problem: its sentence and the ids the canvas should mark.
struct Problem {
    text: String,
    node_ids: Vec<String>,
    edge_ids: Vec<String>,
}

fn node_by_id<'a>(flow: &'a Flow, node_id: &str) -> Option<&'a FlowNode> {
    flow.nodes.iter().find(|n| n.id == node_id)
}

/// The problem for one error. Its whole text is made safe, because reasons
/// quote labels, field names and handles typed by the flow author.
fn describe_problem(flow: &Flow, error: &FlowGraphError) -> Problem {
    let mut problem = describe_raw(flow, error);
    problem.text = defang(&problem.text);
    problem
}

fn describe_raw(flow: &Flow, error: &FlowGraphError) -> Problem {
    match error {
        FlowGraphError::Cycle { node_ids, edge_ids } => Problem {
            text: "flow contains a cycle".to_string(),
            node_ids: node_ids.clone(),
            edge_ids: edge_ids.clone(),
        },
        FlowGraphError::UnknownNode { node_id } => Problem {
            text: format!("edge references unknown node: {}", message_safe(node_id)),
            node_ids: Vec::new(),
            edge_ids: flow
                .edges
                .iter()
                .filter(|e| e.source_node_id == *node_id || e.target_node_id == *node_id)
                .map(|e| e.id.clone())
                .collect(),
        },
        FlowGraphError::DuplicateNode { node_id } => Problem {
            text: format!(
                "flow has more than one node with id: {}",
                message_safe(node_id)
            ),
            node_ids: vec![node_id.clone()],
            edge_ids: Vec::new(),
        },
        FlowGraphError::DuplicateEdge { edge_id } => Problem {
            text: format!(
                "flow has more than one wire with id: {}",
                message_safe(edge_id)
            ),
            node_ids: Vec::new(),
            edge_ids: vec![edge_id.clone()],
        },
        FlowGraphError::InvalidNode { node_id, reason } => {
            let label = node_by_id(flow, node_id)
                .map(|n| message_safe(display_label(n)))
                .unwrap_or_else(|| message_safe(node_id));
            Problem {
                text: format!("'{label}': {reason}"),
                node_ids: vec![node_id.clone()],
                edge_ids: Vec::new(),
            }
        }
        FlowGraphError::InvalidEdge { edge_id, reason } => {
            let text = match wire_ends(flow, edge_id) {
                Some((from, to)) => format!(
                    "wire from '{}' to '{}': {reason}",
                    message_safe(from),
                    message_safe(to)
                ),
                None => reason.clone(),
            };
            Problem {
                text,
                node_ids: Vec::new(),
                edge_ids: vec![edge_id.clone()],
            }
        }
    }
}

/// The text of a flow with exactly one problem. This is the older shape.
fn single_problem_message(flow: &Flow, error: &FlowGraphError) -> String {
    let problem = describe_problem(flow, error);
    match error {
        FlowGraphError::Cycle { node_ids, edge_ids } => format!(
            "flow contains a cycle through node(s): {}; edge(s): {}",
            join_ids(node_ids),
            join_ids(edge_ids)
        ),
        FlowGraphError::UnknownNode { .. } | FlowGraphError::DuplicateNode { .. } => problem.text,
        FlowGraphError::DuplicateEdge { edge_id } => {
            format!(
                "{} — node(s): ; edge(s): {}",
                problem.text,
                join_ids(&[edge_id])
            )
        }
        FlowGraphError::InvalidNode { node_id, .. } => {
            format!(
                "flow is invalid: {} — node(s): {}; edge(s): ",
                problem.text,
                join_ids(&[node_id])
            )
        }
        FlowGraphError::InvalidEdge { edge_id, .. } => {
            format!(
                "flow is invalid: {} — node(s): ; edge(s): {}",
                problem.text,
                join_ids(&[edge_id])
            )
        }
    }
}

/// Builds the save-error text. Every message that names graph elements ends
/// with "node(s): <ids>; edge(s): <ids>", which the canvas parses to
/// highlight them in red. A flow with several problems lists each one as
/// "<text> [nodes <ids>; edges <ids>]." after a "flow has N problems" header,
/// so the canvas can mark each problem on its own node or wire. The final
/// "node(s):" list holds the ids of every listed problem, once each.
fn graph_errors_message(flow: &Flow, report: &ValidationReport) -> String {
    if let ([only], 0) = (report.errors.as_slice(), report.omitted) {
        return single_problem_message(flow, only);
    }
    let problems: Vec<Problem> = report
        .errors
        .iter()
        .map(|error| describe_problem(flow, error))
        .collect();
    let total = problems.len() + report.omitted;
    let mut message = if report.omitted == 0 {
        format!("flow has {total} problems:")
    } else {
        format!(
            "flow has {total} problems, showing the first {}:",
            problems.len()
        )
    };
    let mut all_nodes: Vec<&str> = Vec::new();
    let mut all_edges: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for problem in &problems {
        message.push_str(&format!(
            " {} [nodes {}; edges {}].",
            problem.text,
            join_ids(&problem.node_ids),
            join_ids(&problem.edge_ids)
        ));
        for id in &problem.node_ids {
            if seen.insert(id.as_str()) {
                all_nodes.push(id);
            }
        }
    }
    seen.clear();
    for problem in &problems {
        for id in &problem.edge_ids {
            if seen.insert(id.as_str()) {
                all_edges.push(id);
            }
        }
    }
    message.push_str(&format!(
        " — node(s): {}; edge(s): {}",
        join_ids(&all_nodes),
        join_ids(&all_edges)
    ));
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::lint::{LintSeverity, EXIT_WITHOUT_EDGE, INVALID_GRAPH, NO_PATH_TO_OUTPUT};
    use rocket_flow::{FlowEdge, FlowNode, FlowNodeKind, NodePosition};
    use std::sync::Mutex;

    fn sample_flow() -> Flow {
        Flow {
            name: "Login Then Fetch".to_string(),
            nodes: vec![
                FlowNode {
                    id: "n0".to_string(),
                    kind: FlowNodeKind::Input {
                        label: "Seed".to_string(),
                        value: rocket_shared::VariableValue::simple("x"),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "n1".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Result".to_string(),
                    },
                    position: NodePosition { x: 100.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdge {
                id: "e0".to_string(),
                source_node_id: "n0".to_string(),
                target_node_id: "n1".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
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
    fn list_entries_defaults_to_healthy_entries_for_a_repo_without_an_override() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        assert_eq!(
            svc.list_entries("demo").expect("list_entries"),
            vec![FlowListEntry::healthy("Login Then Fetch")]
        );
        assert!(svc.list_entries("other").expect("other").is_empty());
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

    /// A Request, so that the loop below breaks only the cycle rule. An Output
    /// has no exits and needs a value wire, which would add more problems.
    fn loop_request_kind(label: &str) -> FlowNodeKind {
        FlowNodeKind::Request {
            debug: false,
            repeat_until: None,
            allow_wired_host: false,
            label: label.to_string(),
            source: rocket_flow::RequestSource::Saved {
                request_path: format!("{label}.yml"),
            },
        }
    }

    fn cyclic_flow() -> Flow {
        Flow {
            name: "Cyclic".to_string(),
            nodes: vec![
                FlowNode {
                    id: "a".to_string(),
                    kind: loop_request_kind("A"),
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "b".to_string(),
                    kind: loop_request_kind("B"),
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
            // The Input feeds `a`, so only the wire out of `a` is invalid.
            nodes: vec![
                node_of(
                    "in",
                    FlowNodeKind::Input {
                        label: "Seed".to_string(),
                        value: rocket_shared::VariableValue::simple("x"),
                    },
                ),
                output("a"),
                output("b"),
            ],
            edges: vec![
                wire("e8", "in", rocket_flow::handle::RESULT, "a", "value"),
                FlowEdge {
                    id: "e9".to_string(),
                    source_node_id: "a".to_string(),
                    target_node_id: "b".to_string(),
                    target_field: "value".to_string(),
                    expression: "response.body".to_string(),
                    source_handle: rocket_flow::handle::RESULT.to_string(),
                },
            ],
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

    #[test]
    fn save_rejects_an_unknown_request_field_and_names_the_wire_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Bad Field".to_string(),
            nodes: vec![
                node_of(
                    "in",
                    FlowNodeKind::Input {
                        label: "Seed".to_string(),
                        value: rocket_shared::VariableValue::simple("x"),
                    },
                ),
                node_of(
                    "req",
                    FlowNodeKind::Request {
                        label: "Get user".to_string(),
                        source: rocket_flow::RequestSource::Saved {
                            request_path: "users/get.yml".to_string(),
                        },
                        debug: false,
                        repeat_until: None,
                        allow_wired_host: false,
                    },
                ),
            ],
            edges: vec![FlowEdge {
                id: "e5".to_string(),
                source_node_id: "in".to_string(),
                target_node_id: "req".to_string(),
                target_field: "payload".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an unknown Request field must be rejected")
            .to_string();
        assert!(
            message.contains("the Request node 'Get user' has no field 'payload'"),
            "got: {message}"
        );
        assert!(
            message.ends_with("node(s): ; edge(s): e5"),
            "got: {message}"
        );
    }

    #[test]
    fn save_rejects_a_duplicate_edge_id_and_names_it_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let out = |id: &str| {
            node_of(
                id,
                FlowNodeKind::Output {
                    label: id.to_string(),
                },
            )
        };
        let wire_to = |to: &str| FlowEdge {
            id: "e7".to_string(),
            source_node_id: "a".to_string(),
            target_node_id: to.to_string(),
            target_field: "value".to_string(),
            expression: "response.body".to_string(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        };
        let flow = Flow {
            name: "Twin Wires".to_string(),
            // `a` is an Input, so the repeated id is the only problem.
            nodes: vec![
                node_of(
                    "a",
                    FlowNodeKind::Input {
                        label: "Seed".to_string(),
                        value: rocket_shared::VariableValue::simple("x"),
                    },
                ),
                out("b"),
                out("c"),
            ],
            edges: vec![wire_to("b"), wire_to("c")],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("a repeated edge id must be rejected")
            .to_string();
        assert!(
            message.contains("more than one wire with id: e7"),
            "got: {message}"
        );
        assert!(
            message.ends_with("node(s): ; edge(s): e7"),
            "got: {message}"
        );
    }

    #[test]
    fn save_rejects_an_output_without_a_value_wire_and_names_it_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Lonely".to_string(),
            nodes: vec![node_of(
                "out",
                FlowNodeKind::Output {
                    label: "Result".to_string(),
                },
            )],
            edges: vec![],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an unwired Output must be rejected")
            .to_string();
        assert!(
            message.contains("the Output node 'Result' needs exactly one value wire"),
            "got: {message}"
        );
        assert!(
            message.ends_with("node(s): out; edge(s): "),
            "got: {message}"
        );
    }

    fn login_request() -> FlowNode {
        node_of(
            "login",
            FlowNodeKind::Request {
                debug: false,
                repeat_until: None,
                allow_wired_host: false,
                label: "Login".to_string(),
                source: rocket_flow::RequestSource::Saved {
                    request_path: "login.yml".to_string(),
                },
            },
        )
    }

    fn if_without_input(id: &str, label: &str) -> FlowNode {
        node_of(
            id,
            FlowNodeKind::If {
                label: label.to_string(),
                condition: "response.status === 200".to_string(),
            },
        )
    }

    /// Two problems: `if1` has no input, and `e9` leaves an exit that `login` lacks.
    /// The Output has no value wire to count, so it is fed by `e9` only.
    fn two_problem_flow() -> Flow {
        Flow {
            name: "Two Problems".to_string(),
            nodes: vec![
                login_request(),
                node_of(
                    "out",
                    FlowNodeKind::Output {
                        label: "Result".to_string(),
                    },
                ),
                if_without_input("if1", "Logged in?"),
            ],
            edges: vec![wire("e9", "login", "true", "out", "value")],
            callback_host: None,
        }
    }

    #[test]
    fn save_reports_every_problem_in_one_message() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let message = svc
            .save("demo", two_problem_flow())
            .expect_err("an invalid flow must be rejected")
            .to_string();
        assert_eq!(
            message,
            "Invalid input: flow has 2 problems: \
             'Logged in?': the If node needs exactly one input wire, found 0 [nodes if1; edges ]. \
             wire from 'Login' to 'Result': the source node has no exit named 'true'; its only exit is 'result' [nodes ; edges e9]. \
             — node(s): if1; edge(s): e9"
        );
    }

    #[test]
    fn a_hostile_label_cannot_forge_the_id_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Hostile".to_string(),
            nodes: vec![if_without_input("if1", "oops; node(s): evil [x]")],
            edges: vec![],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an invalid flow must be rejected")
            .to_string();
        assert_eq!(message.matches("node(s):").count(), 1, "got: {message}");
        assert!(
            message.ends_with("node(s): if1; edge(s): "),
            "got: {message}"
        );
    }

    const FORGERY: &str = "x; node(s): evil; edge(s): e9 [nodes victim; edges ]. ";

    fn input_node(id: &str) -> FlowNode {
        node_of(
            id,
            FlowNodeKind::Input {
                label: "Seed".to_string(),
                value: rocket_shared::VariableValue::simple("x"),
            },
        )
    }

    fn save_message(flow: Flow) -> String {
        FlowService::new(Box::new(PanicsOnSaveRepo))
            .save("demo", flow)
            .expect_err("an invalid flow must be rejected")
            .to_string()
    }

    fn assert_no_forgery(message: &str, expected_tail: &str) {
        assert!(message.ends_with(expected_tail), "got: {message}");
        assert_eq!(message.matches("node(s):").count(), 1, "got: {message}");
        assert!(!message.contains("[nodes victim"), "got: {message}");
        assert!(!message.contains('\n'), "got: {message}");
    }

    #[test]
    fn a_hostile_output_label_cannot_forge_the_tail() {
        let flow = Flow {
            name: "Hostile".to_string(),
            nodes: vec![node_of(
                "out",
                FlowNodeKind::Output {
                    label: FORGERY.to_string(),
                },
            )],
            edges: vec![],
            callback_host: None,
        };
        assert_no_forgery(&save_message(flow), "node(s): out; edge(s): ");
    }

    #[test]
    fn a_hostile_request_target_field_cannot_forge_the_tail() {
        let flow = Flow {
            name: "Hostile".to_string(),
            nodes: vec![input_node("in"), login_request()],
            edges: vec![wire("e1", "in", "result", "login", FORGERY)],
            callback_host: None,
        };
        assert_no_forgery(&save_message(flow), "node(s): ; edge(s): e1");
    }

    #[test]
    fn a_hostile_switch_case_label_cannot_forge_the_tail() {
        let switch = node_of(
            "sw",
            FlowNodeKind::Switch {
                label: "Router".to_string(),
                value: "x".to_string(),
                cases: vec![rocket_flow::SwitchCase {
                    id: "c1".to_string(),
                    label: FORGERY.to_string(),
                    matches: "a".to_string(),
                }],
            },
        );
        let flow = Flow {
            name: "Hostile".to_string(),
            nodes: vec![
                input_node("in"),
                switch,
                node_of(
                    "out",
                    FlowNodeKind::Output {
                        label: "Result".to_string(),
                    },
                ),
            ],
            edges: vec![
                wire("e1", "in", "result", "sw", "input"),
                wire("e2", "sw", "nope", "out", "value"),
            ],
            callback_host: None,
        };
        assert_no_forgery(&save_message(flow), "node(s): ; edge(s): e2");
    }

    #[test]
    fn a_newline_in_a_label_keeps_each_problem_in_one_tag() {
        let flow = Flow {
            name: "Newline".to_string(),
            nodes: vec![
                node_of(
                    "out",
                    FlowNodeKind::Output {
                        label: "a\nb\r\nc".to_string(),
                    },
                ),
                if_without_input("if1", "Check\n"),
            ],
            edges: vec![],
            callback_host: None,
        };
        let message = save_message(flow);
        assert!(
            !message.contains('\n') && !message.contains('\r'),
            "got: {message}"
        );
        assert_eq!(message.matches("[nodes ").count(), 2, "got: {message}");
        assert!(
            message.ends_with("node(s): if1, out; edge(s): "),
            "got: {message}"
        );
    }

    #[test]
    fn a_hostile_label_among_several_problems_cannot_forge_a_tag() {
        let flow = Flow {
            name: "Hostile".to_string(),
            nodes: vec![
                node_of(
                    "out",
                    FlowNodeKind::Output {
                        label: FORGERY.to_string(),
                    },
                ),
                if_without_input("if1", "Check"),
            ],
            edges: vec![],
            callback_host: None,
        };
        let message = save_message(flow);
        assert_eq!(message.matches("[nodes ").count(), 2, "got: {message}");
        assert_no_forgery(&message, "node(s): if1, out; edge(s): ");
    }

    #[test]
    fn a_hostile_case_value_cannot_split_its_problem() {
        let case = |id: &str| rocket_flow::SwitchCase {
            id: id.to_string(),
            label: "Same".to_string(),
            matches: "secret [nodes victim; edges ]. rest".to_string(),
        };
        let switch = node_of(
            "sw",
            FlowNodeKind::Switch {
                label: "Router".to_string(),
                value: "x".to_string(),
                cases: vec![case("c1"), case("c2")],
            },
        );
        let flow = Flow {
            name: "Hostile".to_string(),
            nodes: vec![input_node("in"), switch],
            edges: vec![wire("e1", "in", "result", "sw", "input")],
            callback_host: None,
        };
        let message = save_message(flow);
        assert!(!message.contains("[nodes victim"), "got: {message}");
        assert!(
            message.ends_with("node(s): sw; edge(s): "),
            "got: {message}"
        );
    }

    #[test]
    fn a_long_error_list_is_cut_and_the_message_says_how_many() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Many".to_string(),
            nodes: (0..60)
                .map(|i| if_without_input(&format!("if{i}"), "Check"))
                .collect(),
            edges: vec![],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an invalid flow must be rejected")
            .to_string();
        assert!(
            message.starts_with("Invalid input: flow has 60 problems, showing the first 50:"),
            "got: {message}"
        );
        let tail = message
            .rsplit("node(s): ")
            .next()
            .unwrap_or_default()
            .to_string();
        let node_list = tail.split(';').next().unwrap_or_default();
        assert_eq!(node_list.split(", ").count(), 50, "got: {message}");
    }

    #[test]
    fn lint_reports_every_structural_error_by_label() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let lints = svc.lint("demo", &two_problem_flow());
        let errors: Vec<&FlowLint> = lints.iter().filter(|l| l.code == INVALID_GRAPH).collect();
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].node_id.as_deref(), Some("if1"));
        assert!(errors[0].message.starts_with("'Logged in?': "));
        assert_eq!(errors[1].edge_id.as_deref(), Some("e9"));
        assert!(errors[1]
            .message
            .starts_with("Wire from 'Login' to 'Result': "));
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
                wire(
                    "e1",
                    "in",
                    rocket_flow::handle::RESULT,
                    "if1",
                    rocket_flow::handle::INPUT,
                ),
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
        assert_eq!(
            lint_keys(&lints),
            vec![(Some("if1"), None, EXIT_WITHOUT_EDGE)]
        );
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
        // A wired Output makes the flow have one, so the lonely Input warns.
        // It adds no structural error because `z` has its one value wire.
        flow.nodes.push(node_of(
            "z",
            FlowNodeKind::Output {
                label: "Done".to_string(),
            },
        ));
        flow.nodes.push(node_of(
            "d",
            FlowNodeKind::Input {
                label: "Feed".to_string(),
                value: rocket_shared::VariableValue::simple("x"),
            },
        ));
        flow.edges
            .push(wire("e3", "d", rocket_flow::handle::RESULT, "z", "value"));
        let lints = svc.lint("demo", &flow);
        assert_eq!(
            lint_keys(&lints),
            vec![
                (Some("a"), None, INVALID_GRAPH),
                (Some("b"), None, INVALID_GRAPH),
                (None, Some("e1"), INVALID_GRAPH),
                (None, Some("e2"), INVALID_GRAPH),
                (Some("a"), None, NO_PATH_TO_OUTPUT),
                (Some("b"), None, NO_PATH_TO_OUTPUT),
                (Some("c"), None, NO_PATH_TO_OUTPUT),
            ]
        );
        assert!(lints[..4].iter().all(|l| l.severity == LintSeverity::Error));
        assert!(lints[4..]
            .iter()
            .all(|l| l.severity == LintSeverity::Warning));
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
        assert!(lints[1..]
            .iter()
            .all(|l| l.severity == LintSeverity::Warning));
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

    #[test]
    fn lint_with_environment_without_sources_matches_lint() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let flow = routing_flow();
        let env = crate::flow_lint_context::LintEnvironment {
            environment_name: Some("dev"),
            global_env_name: None,
        };
        assert_eq!(
            svc.lint_with_environment("demo", &flow, env),
            svc.lint("demo", &flow)
        );
    }

    fn lint_sources_with_nothing_saved() -> crate::flow_lint_context::LintSources {
        use crate::test_doubles::{ConfigurableCollectionRepo, NullEnvRepo, SharedCollectionRepo};
        struct NoEnvs;
        impl rocket_environment::EnvironmentRepositoryFactory for NoEnvs {
            fn for_collection(
                &self,
                _collection: &str,
            ) -> Box<dyn rocket_environment::EnvironmentRepository> {
                Box::new(NullEnvRepo)
            }
        }
        crate::flow_lint_context::LintSources {
            collection_repo: Box::new(SharedCollectionRepo(ConfigurableCollectionRepo::new())),
            env_factory: Box::new(NoEnvs),
            global_env_repo: Box::new(NullEnvRepo),
        }
    }

    fn flow_with_saved_request() -> Flow {
        Flow {
            name: "Dangling".to_string(),
            nodes: vec![node_of(
                "req",
                FlowNodeKind::Request {
                    label: "Log in".to_string(),
                    source: rocket_flow::RequestSource::Saved {
                        request_path: "auth/login.yml".to_string(),
                    },
                    debug: false,
                    repeat_until: None,
                    allow_wired_host: false,
                },
            )],
            edges: vec![],
            callback_host: None,
        }
    }

    #[test]
    fn lint_reports_a_saved_request_that_is_gone() {
        let svc = FlowService::new(Box::new(UntouchedRepo))
            .with_lint_sources(lint_sources_with_nothing_saved());
        let lints = svc.lint("demo", &flow_with_saved_request());
        assert_eq!(
            lint_keys(&lints),
            vec![(Some("req"), None, rocket_flow::lint::DANGLING_SAVED_REQUEST)]
        );
        assert_eq!(lints[0].severity, LintSeverity::Error);
    }

    #[test]
    fn a_flow_with_a_dangling_request_still_saves() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()))
            .with_lint_sources(lint_sources_with_nothing_saved());
        let flow = flow_with_saved_request();
        assert!(!svc.lint("demo", &flow).is_empty());
        svc.save("demo", flow.clone())
            .expect("a flow with a dangling request must still save");
        assert_eq!(svc.get("demo", "Dangling").expect("get"), flow);
    }

    fn inline_flow(url: &str) -> Flow {
        Flow {
            name: "Inline".to_string(),
            nodes: vec![node_of(
                "req",
                FlowNodeKind::Request {
                    label: "Get".to_string(),
                    source: rocket_flow::RequestSource::Inline {
                        request: rocket_flow::InlineRequestData {
                            method: "GET".to_string(),
                            url: url.to_string(),
                            headers: vec![],
                            body: None,
                        },
                    },
                    debug: false,
                    repeat_until: None,
                    allow_wired_host: false,
                },
            )],
            edges: vec![],
            callback_host: None,
        }
    }

    #[test]
    fn lint_reports_an_unknown_variable_for_the_active_environment() {
        use crate::test_doubles::{
            ConfigurableCollectionRepo, NullEnvRepo, SharedCollectionRepo, StaticEnvRepo,
        };
        use rocket_environment::{Environment, Variable};
        struct OneEnv(Environment);
        impl rocket_environment::EnvironmentRepositoryFactory for OneEnv {
            fn for_collection(
                &self,
                _collection: &str,
            ) -> Box<dyn rocket_environment::EnvironmentRepository> {
                Box::new(StaticEnvRepo(self.0.clone()))
            }
        }
        let mut env = Environment::new("dev");
        env.variables = vec![Variable::new("host", "h")];
        let svc = FlowService::new(Box::new(UntouchedRepo)).with_lint_sources(
            crate::flow_lint_context::LintSources {
                collection_repo: Box::new(SharedCollectionRepo(ConfigurableCollectionRepo::new())),
                env_factory: Box::new(OneEnv(env)),
                global_env_repo: Box::new(NullEnvRepo),
            },
        );
        let flow = inline_flow("{{host}}/{{typo}}");
        let dev = crate::flow_lint_context::LintEnvironment {
            environment_name: Some("dev"),
            global_env_name: None,
        };
        let lints = svc.lint_with_environment("demo", &flow, dev);
        assert_eq!(
            lint_keys(&lints),
            vec![(Some("req"), None, rocket_flow::lint::UNKNOWN_VARIABLE)]
        );
        assert!(lints[0].message.contains("{{typo}}"));
        assert!(!lints[0].message.contains("host"));
        // No environment selected: the rule cannot tell, so it stays silent.
        assert!(svc.lint("demo", &flow).is_empty());
    }

    #[test]
    fn a_saved_request_in_the_flow_silences_the_variable_rule() {
        let svc = FlowService::new(Box::new(UntouchedRepo))
            .with_lint_sources(lint_sources_with_nothing_saved());
        let mut flow = inline_flow("{{typo}}");
        flow.nodes.push(node_of(
            "saved",
            FlowNodeKind::Request {
                label: "Saved".to_string(),
                source: rocket_flow::RequestSource::Saved {
                    request_path: "a.yml".to_string(),
                },
                debug: false,
                repeat_until: None,
                allow_wired_host: false,
            },
        ));
        let dev = crate::flow_lint_context::LintEnvironment {
            environment_name: Some("dev"),
            global_env_name: None,
        };
        let lints = svc.lint_with_environment("demo", &flow, dev);
        assert!(
            lints
                .iter()
                .all(|l| l.code != rocket_flow::lint::UNKNOWN_VARIABLE),
            "a saved request may set variables at run time"
        );
    }

    #[test]
    fn save_does_not_overwrite_a_corrupt_file_with_a_colliding_name() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let flows_dir = dir.path().join("demo").join("flows");
        std::fs::create_dir_all(&flows_dir).expect("create flows dir");
        std::fs::write(flows_dir.join("broken.yml"), "not: valid: yaml: [")
            .expect("write corrupt file");
        let svc = FlowService::new(Box::new(rocket_infra::FsFlowRepo::new(
            dir.path().to_path_buf(),
        )));

        let err = svc
            .save("demo", named_flow("Broken"))
            .expect_err("must refuse");

        assert!(matches!(
            err,
            rocket_shared::error::DomainError::Conflict(_)
        ));
        let raw = std::fs::read_to_string(flows_dir.join("broken.yml")).expect("read file");
        assert_eq!(raw, "not: valid: yaml: [");
    }
}
