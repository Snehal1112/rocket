//! Non-blocking lint tier (roadmap F-20). `validate` rejects a graph that
//! cannot be saved or run. This module only warns about a graph that runs
//! but may not do what its author meant. Lints never block save or run.
//!
//! The module does no I/O. Facts that need the collection or the
//! environment come through `LintContext`, which the app layer provides.
//! Results are deterministic: nodes in file order, rules in a fixed order.

use crate::flow::{Flow, FlowNode};
use crate::handle;
use crate::node::FlowNodeKind;
use crate::validate::kind_name;
use std::collections::{HashMap, HashSet};

/// A structural `validate` failure, reported as an error lint.
pub const INVALID_GRAPH: &str = "invalid_graph";
/// An If exit or a Switch case exit has no wire.
pub const EXIT_WITHOUT_EDGE: &str = "exit_without_edge";
/// A Switch node's `default` exit has no wire.
pub const SWITCH_WITHOUT_DEFAULT: &str = "switch_without_default";
/// A node has no path to any Output, in a flow that has an Output.
pub const NO_PATH_TO_OUTPUT: &str = "no_path_to_output";

/// How serious a lint is. Neither severity blocks save or run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
}

/// One finding of the lint tier. It has the shape of the client
/// `FlowIssue`, so the canvas can merge both lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowLint {
    pub code: String,
    pub severity: LintSeverity,
    pub node_id: Option<String>,
    pub edge_id: Option<String>,
    /// Names the node by label, never by a resolved value.
    pub message: String,
    pub hint: Option<String>,
}

/// Facts about the world outside the flow file, answered by the app layer.
/// Each method answers `None` when it cannot tell, and a lint that gets
/// `None` is skipped, so a context that knows nothing never causes a lint.
/// The methods answer yes or no, never a value, so no lint can quote a
/// resolved variable or a secret.
pub trait LintContext {
    /// Whether the collection has a saved request at `request_path`.
    fn saved_request_exists(&self, _request_path: &str) -> Option<bool> {
        None
    }

    /// Whether `{{name}}` resolves in a scope the run will see.
    fn variable_is_known(&self, _name: &str) -> Option<bool> {
        None
    }
}

/// A context that knows nothing about the collection or the environment.
pub struct NoLintContext;

impl LintContext for NoLintContext {}

/// Lints `flow` without blocking anything. Works on any graph, valid or
/// not, and never panics on unknown ids or cycles.
pub fn validate_with_warnings(flow: &Flow, ctx: &dyn LintContext) -> Vec<FlowLint> {
    // No rule asks the context yet. F-21 and F-22 add the rules that do.
    let _ = ctx;
    let index = GraphIndex::new(flow);
    let mut seen: HashSet<&str> = HashSet::with_capacity(flow.nodes.len());
    let mut lints = Vec::new();
    for node in &flow.nodes {
        // A repeated id is an invalid_graph error. Lint its first node only.
        if !seen.insert(node.id.as_str()) {
            continue;
        }
        lints.extend(exit_lints(node, &index));
    }
    lints
}

/// Lookups built once per call, so every rule stays linear in graph size.
struct GraphIndex<'a> {
    /// The exits of each node that have at least one wire.
    wired_exits: HashMap<&'a str, HashSet<&'a str>>,
}

impl<'a> GraphIndex<'a> {
    fn new(flow: &'a Flow) -> Self {
        let mut wired_exits: HashMap<&'a str, HashSet<&'a str>> = HashMap::new();
        for edge in &flow.edges {
            wired_exits
                .entry(edge.source_node_id.as_str())
                .or_default()
                .insert(edge.source_handle.as_str());
        }
        Self { wired_exits }
    }

    fn is_wired(&self, node_id: &str, exit: &str) -> bool {
        self.wired_exits
            .get(node_id)
            .is_some_and(|exits| exits.contains(exit))
    }
}

/// The node's label, or its kind name when the label is blank.
fn display_label(node: &FlowNode) -> &str {
    let label = match &node.kind {
        FlowNodeKind::Request { label, .. }
        | FlowNodeKind::Input { label, .. }
        | FlowNodeKind::Output { label, .. }
        | FlowNodeKind::If { label, .. }
        | FlowNodeKind::Switch { label, .. }
        | FlowNodeKind::WaitForCallback { label, .. }
        | FlowNodeKind::Transform { label, .. }
        | FlowNodeKind::Auth { label, .. } => label.trim(),
    };
    if label.is_empty() {
        kind_name(&node.kind)
    } else {
        label
    }
}

fn warning(code: &str, node: &FlowNode, message: String, hint: &str) -> FlowLint {
    FlowLint {
        code: code.to_string(),
        severity: LintSeverity::Warning,
        node_id: Some(node.id.clone()),
        edge_id: None,
        message,
        hint: Some(hint.to_string()),
    }
}

/// The routing exits of a node as `(handle, name shown to the user)`. The
/// Switch `default` exit is left out, because it has its own rule.
fn routed_exits(kind: &FlowNodeKind) -> Vec<(String, String)> {
    match kind {
        FlowNodeKind::If { .. } => vec![
            (handle::TRUE.to_string(), handle::TRUE.to_string()),
            (handle::FALSE.to_string(), handle::FALSE.to_string()),
        ],
        FlowNodeKind::Switch { cases, .. } => cases
            .iter()
            .enumerate()
            .map(|(i, case)| {
                let name = match case.label.trim() {
                    "" => format!("Case {}", i + 1),
                    label => label.to_string(),
                };
                (handle::case_handle(&case.id), name)
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Unwired If exits and Switch cases (one lint per node that names every
/// unwired exit), then an unwired Switch default.
fn exit_lints(node: &FlowNode, index: &GraphIndex<'_>) -> Vec<FlowLint> {
    let mut lints = Vec::new();
    let label = display_label(node);
    let unwired: Vec<String> = routed_exits(&node.kind)
        .into_iter()
        .filter(|(exit, _)| !index.is_wired(&node.id, exit))
        .map(|(_, name)| format!("'{name}'"))
        .collect();
    if !unwired.is_empty() {
        let message = if unwired.len() == 1 {
            format!("The {} exit of '{label}' has no wire.", unwired.join(", "))
        } else {
            format!("The {} exits of '{label}' have no wire.", unwired.join(", "))
        };
        lints.push(warning(
            EXIT_WITHOUT_EDGE,
            node,
            message,
            "A run that takes an unwired exit ends that branch. Wire it to a node if the run should go on.",
        ));
    }
    if matches!(node.kind, FlowNodeKind::Switch { .. }) && !index.is_wired(&node.id, handle::DEFAULT)
    {
        lints.push(warning(
            SWITCH_WITHOUT_DEFAULT,
            node,
            format!("The 'default' exit of '{label}' has no wire."),
            "A value that matches no case ends that branch. Wire the default exit, or ignore this if every value has a case.",
        ));
    }
    lints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowEdge;
    use crate::node::{NodePosition, RequestSource, SwitchCase};

    fn node(id: &str, kind: FlowNodeKind) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn input(id: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Input {
                label: id.to_string(),
                value: rocket_shared::VariableValue::simple("x"),
            },
        )
    }

    fn output(id: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Output {
                label: id.to_string(),
            },
        )
    }

    fn request(id: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                label: id.to_string(),
                source: RequestSource::Saved {
                    request_path: format!("{id}.yml"),
                },
                debug: false,
                repeat_until: None,
            },
        )
    }

    fn if_node(id: &str, label: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::If {
                label: label.to_string(),
                condition: "response.status === 200".to_string(),
            },
        )
    }

    /// Case ids are `c1`, `c2`, ... in the order given.
    fn switch_node(id: &str, label: &str, cases: &[(&str, &str)]) -> FlowNode {
        node(
            id,
            FlowNodeKind::Switch {
                label: label.to_string(),
                value: "response.body.plan".to_string(),
                cases: cases
                    .iter()
                    .enumerate()
                    .map(|(i, (case_label, matches))| SwitchCase {
                        id: format!("c{}", i + 1),
                        label: case_label.to_string(),
                        matches: matches.to_string(),
                    })
                    .collect(),
            },
        )
    }

    fn edge(id: &str, from: &str, exit: &str, to: &str, field: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: to.to_string(),
            target_field: field.to_string(),
            expression: String::new(),
            source_handle: exit.to_string(),
        }
    }

    fn flow(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> Flow {
        Flow {
            name: "f".to_string(),
            nodes,
            edges,
            callback_host: None,
        }
    }

    fn lint(f: &Flow) -> Vec<FlowLint> {
        validate_with_warnings(f, &NoLintContext)
    }

    fn only<'a>(lints: &'a [FlowLint], code: &str) -> Vec<&'a FlowLint> {
        lints.iter().filter(|l| l.code == code).collect()
    }

    #[test]
    fn a_fully_wired_if_has_no_exit_lint() {
        let f = flow(
            vec![input("in"), if_node("if1", "Check"), output("ok"), output("ko")],
            vec![
                edge("e1", "in", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "if1", handle::TRUE, "ok", "value"),
                edge("e3", "if1", handle::FALSE, "ko", "value"),
            ],
        );
        assert!(only(&lint(&f), EXIT_WITHOUT_EDGE).is_empty());
    }

    #[test]
    fn an_unwired_if_exit_is_one_warning_naming_the_exit_and_label() {
        let f = flow(
            vec![input("in"), if_node("if1", "Check status"), output("out")],
            vec![
                edge("e1", "in", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "if1", handle::TRUE, "out", "value"),
            ],
        );
        let lints = lint(&f);
        let found = only(&lints, EXIT_WITHOUT_EDGE);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert_eq!(found[0].node_id.as_deref(), Some("if1"));
        assert_eq!(found[0].edge_id, None);
        assert_eq!(
            found[0].message,
            "The 'false' exit of 'Check status' has no wire."
        );
        assert!(found[0].hint.is_some());
    }

    #[test]
    fn both_unwired_if_exits_are_one_warning() {
        let f = flow(
            vec![input("in"), if_node("if1", "Check")],
            vec![edge("e1", "in", handle::RESULT, "if1", handle::INPUT)],
        );
        let lints = lint(&f);
        let found = only(&lints, EXIT_WITHOUT_EDGE);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].message,
            "The 'true', 'false' exits of 'Check' have no wire."
        );
    }

    #[test]
    fn switch_cases_and_default_are_reported_separately() {
        let f = flow(
            vec![
                input("in"),
                switch_node("sw", "Route", &[("Gold", "gold"), ("  ", "free")]),
            ],
            vec![edge("e1", "in", handle::RESULT, "sw", handle::INPUT)],
        );
        let lints = lint(&f);
        let exits = only(&lints, EXIT_WITHOUT_EDGE);
        assert_eq!(exits.len(), 1);
        assert_eq!(
            exits[0].message,
            "The 'Gold', 'Case 2' exits of 'Route' have no wire."
        );
        let default = only(&lints, SWITCH_WITHOUT_DEFAULT);
        assert_eq!(default.len(), 1);
        assert_eq!(default[0].node_id.as_deref(), Some("sw"));
        assert_eq!(
            default[0].message,
            "The 'default' exit of 'Route' has no wire."
        );
    }

    #[test]
    fn a_switch_with_every_exit_wired_has_no_lint() {
        let f = flow(
            vec![
                input("in"),
                switch_node("sw", "Route", &[("Gold", "gold")]),
                output("a"),
                output("b"),
            ],
            vec![
                edge("e1", "in", handle::RESULT, "sw", handle::INPUT),
                edge("e2", "sw", &handle::case_handle("c1"), "a", "value"),
                edge("e3", "sw", handle::DEFAULT, "b", "value"),
            ],
        );
        let lints = lint(&f);
        assert!(only(&lints, EXIT_WITHOUT_EDGE).is_empty());
        assert!(only(&lints, SWITCH_WITHOUT_DEFAULT).is_empty());
    }

    #[test]
    fn a_switch_with_no_cases_only_warns_about_default() {
        let f = flow(
            vec![input("in"), switch_node("sw", "Route", &[])],
            vec![edge("e1", "in", handle::RESULT, "sw", handle::INPUT)],
        );
        let lints = lint(&f);
        assert!(only(&lints, EXIT_WITHOUT_EDGE).is_empty());
        assert_eq!(only(&lints, SWITCH_WITHOUT_DEFAULT).len(), 1);
    }

    #[test]
    fn blank_labels_fall_back_to_the_kind_name() {
        let f = flow(
            vec![input("in"), if_node("if1", "   "), output("out")],
            vec![
                edge("e1", "in", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "if1", handle::TRUE, "out", "value"),
            ],
        );
        let lints = lint(&f);
        assert_eq!(
            only(&lints, EXIT_WITHOUT_EDGE)[0].message,
            "The 'false' exit of 'If' has no wire."
        );
    }

    #[test]
    fn an_exit_wired_by_a_trigger_wire_counts_as_wired() {
        let f = flow(
            vec![input("in"), if_node("if1", "Check"), request("req"), output("out")],
            vec![
                edge("e1", "in", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "if1", handle::TRUE, "req", handle::TRIGGER),
                edge("e3", "if1", handle::FALSE, "out", handle::TRIGGER),
            ],
        );
        assert!(only(&lint(&f), EXIT_WITHOUT_EDGE).is_empty());
    }

    #[test]
    fn ghost_node_ids_do_not_panic_or_get_lints() {
        let f = flow(
            vec![input("in"), if_node("if1", "Check")],
            vec![
                edge("e1", "in", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "if1", handle::TRUE, "ghost-target", "value"),
                edge("e3", "ghost-source", handle::TRUE, "if1", handle::INPUT),
            ],
        );
        let lints = lint(&f);
        assert!(lints
            .iter()
            .all(|l| l.node_id.as_deref() != Some("ghost-target")
                && l.node_id.as_deref() != Some("ghost-source")));
        let exits = only(&lints, EXIT_WITHOUT_EDGE);
        assert_eq!(exits.len(), 1);
        assert_eq!(exits[0].message, "The 'false' exit of 'Check' has no wire.");
    }

    #[test]
    fn a_repeated_node_id_is_linted_once() {
        let f = flow(
            vec![input("in"), if_node("if1", "First"), if_node("if1", "Second")],
            vec![edge("e1", "in", handle::RESULT, "if1", handle::INPUT)],
        );
        let lints = lint(&f);
        let exits = only(&lints, EXIT_WITHOUT_EDGE);
        assert_eq!(exits.len(), 1);
        assert!(exits[0].message.contains("'First'"));
    }
}
