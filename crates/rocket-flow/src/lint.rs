//! Non-blocking lint tier (roadmap F-20). `validate` rejects a graph that
//! cannot be saved or run. This module only warns about a graph that runs
//! but may not do what its author meant. Lints never block save or run.
//!
//! The module does no I/O. Facts that need the collection or the
//! environment come through `LintContext`, which the app layer provides.
//! Results are deterministic: nodes in file order, rules in a fixed order.

use crate::flow::{Flow, FlowNode};
use crate::graph::FlowGraphError;
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
        lints.extend(no_path_lint(node, &index));
    }
    lints
}

/// Lookups built once per call, so every rule stays linear in graph size.
struct GraphIndex<'a> {
    /// The exits of each node that have at least one wire.
    wired_exits: HashMap<&'a str, HashSet<&'a str>>,
    /// Ids with a path to an Output, or `None` when the flow has no Output.
    reaching_output: Option<HashSet<&'a str>>,
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
        Self {
            wired_exits,
            reaching_output: nodes_reaching_output(flow),
        }
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

/// Walks wires backwards from every Output. Every wire kind counts as a
/// path. The visited set makes a cycle safe, and ids of missing nodes
/// do no harm, because only real nodes are linted.
fn nodes_reaching_output(flow: &Flow) -> Option<HashSet<&str>> {
    let mut stack: Vec<&str> = flow
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, FlowNodeKind::Output { .. }))
        .map(|n| n.id.as_str())
        .collect();
    if stack.is_empty() {
        return None;
    }
    let mut sources_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &flow.edges {
        sources_of
            .entry(edge.target_node_id.as_str())
            .or_default()
            .push(edge.source_node_id.as_str());
    }
    let mut reached: HashSet<&str> = stack.iter().copied().collect();
    while let Some(id) = stack.pop() {
        for &source in sources_of.get(id).into_iter().flatten() {
            if reached.insert(source) {
                stack.push(source);
            }
        }
    }
    Some(reached)
}

/// A node with no path to any Output, in a flow that has one. Output
/// nodes are skipped, and so is an Auth node that applies to inherited
/// auth, because it acts on requests without a wire.
fn no_path_lint(node: &FlowNode, index: &GraphIndex<'_>) -> Option<FlowLint> {
    let reached = index.reaching_output.as_ref()?;
    let exempt = match &node.kind {
        FlowNodeKind::Output { .. } => true,
        FlowNodeKind::Auth {
            apply_to_inherit, ..
        } => *apply_to_inherit,
        _ => false,
    };
    if exempt || reached.contains(node.id.as_str()) {
        return None;
    }
    // An If or a Switch has no result exit of its own.
    let consequence = if matches!(
        node.kind,
        FlowNodeKind::If { .. } | FlowNodeKind::Switch { .. }
    ) {
        "nothing after it reaches an Output"
    } else {
        "its result is never shown"
    };
    Some(warning(
        NO_PATH_TO_OUTPUT,
        node,
        format!(
            "'{}' does not lead to any Output, so {consequence}.",
            display_label(node)
        ),
        "Wire it towards an Output to see its result. Ignore this if the node runs only for its effect.",
    ))
}

/// Hint of an `invalid_graph` lint.
const FIX: &str = "Fix this before you save or run the flow.";
/// Hint of an `invalid_graph` lint on a cycle.
const LOOP: &str = "Remove one of the wires in the loop.";

/// Turns a `validate` failure into error lints, one per node or wire it
/// names, so the canvas can mark each one. An unknown node is reported on
/// the first wire that points to it, because its id is not on the canvas.
pub fn graph_error_lints(flow: &Flow, error: &FlowGraphError) -> Vec<FlowLint> {
    let lint = |node_id: Option<&str>, edge_id: Option<&str>, message: String, hint: &str| {
        FlowLint {
            code: INVALID_GRAPH.to_string(),
            severity: LintSeverity::Error,
            node_id: node_id.map(str::to_string),
            edge_id: edge_id.map(str::to_string),
            message,
            hint: Some(hint.to_string()),
        }
    };
    // Built once, so a large loop stays linear. The first node of an id wins.
    let mut labels: HashMap<&str, &FlowNode> = HashMap::with_capacity(flow.nodes.len());
    for n in &flow.nodes {
        labels.entry(n.id.as_str()).or_insert(n);
    }
    let label_of = |node_id: &str| {
        labels.get(node_id).map_or_else(
            || "This node".to_string(),
            |n| format!("'{}'", display_label(n)),
        )
    };
    match error {
        FlowGraphError::Cycle { node_ids, edge_ids } => node_ids
            .iter()
            .map(|id| {
                let message = format!(
                    "{} is part of a loop. A flow must not lead back to itself.",
                    label_of(id.as_str())
                );
                lint(Some(id.as_str()), None, message, LOOP)
            })
            .chain(edge_ids.iter().map(|id| {
                let message =
                    "This wire is part of a loop. A flow must not lead back to itself.".to_string();
                lint(None, Some(id.as_str()), message, LOOP)
            }))
            .collect(),
        FlowGraphError::UnknownNode { node_id } => {
            let wire = flow
                .edges
                .iter()
                .find(|e| e.source_node_id == *node_id || e.target_node_id == *node_id);
            let message = "This wire is connected to a node that does not exist.".to_string();
            vec![lint(None, wire.map(|e| e.id.as_str()), message, FIX)]
        }
        FlowGraphError::DuplicateNode { node_id } => {
            let message = "More than one node has the same id.".to_string();
            vec![lint(Some(node_id.as_str()), None, message, FIX)]
        }
        FlowGraphError::InvalidNode { node_id, reason } => {
            let message = format!("{}: {}", label_of(node_id.as_str()), sentence(reason));
            vec![lint(Some(node_id.as_str()), None, message, FIX)]
        }
        FlowGraphError::InvalidEdge { edge_id, reason } => {
            vec![lint(None, Some(edge_id.as_str()), sentence(reason), FIX)]
        }
    }
}

/// The kinds whose reasons name a field or a count.
const ROUTED_KINDS: [&str; 3] = ["If", "Switch", "Transform"];

/// Reasons of `validate` that hold no user text, matched exactly.
const FIXED_REASONS: [&str; 12] = [
    "only If, Switch and Transform nodes have an 'input' input",
    "only Request, Output and Wait for callback nodes have a 'trigger' input",
    "Input nodes cannot receive wires",
    "Auth nodes cannot receive wires",
    "an 'auth' wire must go from an Auth node into a Request node",
    "a Request node can take only one 'auth' wire",
    "Wait for callback nodes only have a 'trigger' input",
    "the repeat-until condition is empty",
    "repeat-until timeout must not be shorter than the interval",
    "the Auth node needs an auth type other than none or inherit",
    "only one Auth node can apply to inherited auth; turn this one or the other off",
    "the Wait for callback node's accept_when is empty",
];

/// Reasons of `validate` that end in numbers from constants, as
/// `(prefix, suffix)`. The middle must be one or two plain numbers.
const NUMBERED_REASONS: [(&str, &str); 4] = [
    ("repeat-until interval must be at least ", " ms"),
    ("repeat-until max attempts must be between 1 and ", ""),
    ("repeat-until timeout must be at most ", " ms"),
    ("the Wait for callback timeout must be between ", " ms"),
];

/// The reason as a sentence that never holds user-typed text. `validate`
/// builds some reasons from a match value, an id, a name or a handle, and
/// those get fixed text. Every other known reason is passed on, and a
/// reason that matches nothing here gets a generic sentence.
fn sentence(reason: &str) -> String {
    let reason = reason.trim();
    let fixed = if reason.starts_with("more than one case matches") {
        Some("Two cases of this Switch have the same match value.".to_string())
    } else if reason.starts_with("more than one case has id") {
        Some("Two cases of this Switch have the same id.".to_string())
    } else if reason.starts_with("the source node has no exit named") {
        Some("The wire leaves an exit that the source node does not have.".to_string())
    } else if reason.starts_with("the Wait for callback name") {
        Some("The Wait for callback name must use only letters, digits and _.".to_string())
    } else if reason.starts_with("more than one Wait for callback node is named") {
        Some("More than one Wait for callback node has the same name.".to_string())
    } else if reason.starts_with("Request '") && reason.contains(" sends {{callback.") {
        Some(
            "A Request uses a callback value but is not wired before this Wait for callback node."
                .to_string(),
        )
    } else {
        routed_reason(reason).or_else(|| known_reason(reason))
    };
    fixed.unwrap_or_else(|| "The flow has a structural problem.".to_string())
}

/// Reasons about the wire or the field of an If, Switch or Transform node.
fn routed_reason(reason: &str) -> Option<String> {
    let rest = reason.strip_prefix("the ")?;
    let kind = ROUTED_KINDS.iter().find(|k| rest.starts_with(**k))?;
    let rest = rest.strip_prefix(*kind)?.strip_prefix(" node")?;
    if rest.starts_with("'s wire must target '") {
        return Some(format!("The {kind} node's wire must target its 'input' handle."));
    }
    if let Some(count) = rest.strip_prefix(" needs exactly one input wire, found ") {
        let plain = !count.is_empty() && count.chars().all(|c| c.is_ascii_digit());
        return plain.then(|| format!("The {kind} node needs exactly one input wire, found {count}."));
    }
    let field = match *kind {
        "If" => "condition",
        "Switch" => "value",
        _ => "script",
    };
    (rest == format!("'s {field} is empty"))
        .then(|| format!("The {kind} node's {field} is empty."))
}

/// Reasons with no user text: the exact list and the numbered ones.
fn known_reason(reason: &str) -> Option<String> {
    let known = FIXED_REASONS.contains(&reason)
        || NUMBERED_REASONS.iter().any(|(prefix, suffix)| {
            reason
                .strip_prefix(prefix)
                .and_then(|r| r.strip_suffix(suffix))
                .is_some_and(|numbers| {
                    numbers
                        .split(" and ")
                        .all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                })
        });
    if !known {
        return None;
    }
    let mut chars = reason.chars();
    let first = chars.next()?;
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    Some(out)
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

    use crate::graph::FlowGraphError;
    use crate::validate::validate;

    fn auth(id: &str, apply_to_inherit: bool) -> FlowNode {
        node(
            id,
            FlowNodeKind::Auth {
                label: id.to_string(),
                auth: rocket_shared::types::Auth::Bearer {
                    token: "{{token}}".to_string(),
                },
                apply_to_inherit,
            },
        )
    }

    fn keys(lints: &[FlowLint]) -> Vec<(Option<&str>, Option<&str>, &str)> {
        lints
            .iter()
            .map(|l| (l.node_id.as_deref(), l.edge_id.as_deref(), l.code.as_str()))
            .collect()
    }

    fn graph_lints(f: &Flow) -> Vec<FlowLint> {
        let error = validate(f).expect_err("the flow must be invalid");
        graph_error_lints(f, &error)
    }

    #[test]
    fn an_empty_flow_has_no_lints() {
        assert!(lint(&flow(vec![], vec![])).is_empty());
    }

    #[test]
    fn a_node_with_no_path_to_an_output_is_warned() {
        let f = flow(
            vec![input("in1"), output("out1"), input("in2")],
            vec![edge("e1", "in1", handle::RESULT, "out1", "value")],
        );
        let lints = lint(&f);
        assert_eq!(keys(&lints), vec![(Some("in2"), None, NO_PATH_TO_OUTPUT)]);
        assert_eq!(lints[0].severity, LintSeverity::Warning);
        assert_eq!(
            lints[0].message,
            "'in2' does not lead to any Output, so its result is never shown."
        );
        assert!(lints[0].hint.is_some());
    }

    #[test]
    fn no_output_means_no_reach_lint() {
        let f = flow(vec![input("in1"), input("in2")], vec![]);
        assert!(lint(&f).is_empty());
    }

    #[test]
    fn paths_through_trigger_and_auth_wires_reach_the_output() {
        let f = flow(
            vec![auth("a1", false), request("r1"), request("r2"), output("out")],
            vec![
                edge("e1", "a1", handle::RESULT, "r1", handle::AUTH),
                edge("e2", "r1", handle::RESULT, "r2", handle::TRIGGER),
                edge("e3", "r2", handle::RESULT, "out", "value"),
            ],
        );
        assert!(lint(&f).is_empty());
    }

    #[test]
    fn an_auth_node_that_applies_to_inherited_auth_is_not_flagged() {
        let applies = flow(
            vec![auth("a1", true), request("r1"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        assert!(lint(&applies).is_empty());
        let idle = flow(
            vec![auth("a1", false), request("r1"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        assert_eq!(keys(&lint(&idle)), vec![(Some("a1"), None, NO_PATH_TO_OUTPUT)]);
    }

    #[test]
    fn a_cycle_does_not_hang_the_reach_rule() {
        let f = flow(
            vec![output("a"), output("b"), input("c")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "value"),
                edge("e2", "b", handle::RESULT, "a", "value"),
            ],
        );
        assert_eq!(keys(&lint(&f)), vec![(Some("c"), None, NO_PATH_TO_OUTPUT)]);
    }

    /// in1 -> sw; sw.case c1 -> if1; if1.true -> out1; lonely has no wire.
    fn mixed_flow(reverse_edges: bool) -> Flow {
        let mut edges = vec![
            edge("e1", "in1", handle::RESULT, "sw", handle::INPUT),
            edge("e2", "sw", &handle::case_handle("c1"), "if1", handle::INPUT),
            edge("e3", "if1", handle::TRUE, "out1", "value"),
        ];
        if reverse_edges {
            edges.reverse();
        }
        flow(
            vec![
                input("in1"),
                switch_node("sw", "Route", &[("Gold", "gold")]),
                if_node("if1", "Check"),
                output("out1"),
                input("lonely"),
            ],
            edges,
        )
    }

    #[test]
    fn results_follow_node_order_and_ignore_edge_order() {
        let first = lint(&mixed_flow(false));
        assert_eq!(
            keys(&first),
            vec![
                (Some("sw"), None, SWITCH_WITHOUT_DEFAULT),
                (Some("if1"), None, EXIT_WITHOUT_EDGE),
                (Some("lonely"), None, NO_PATH_TO_OUTPUT),
            ]
        );
        assert_eq!(lint(&mixed_flow(false)), first);
        assert_eq!(lint(&mixed_flow(true)), first);
    }

    #[test]
    fn messages_never_quote_values_or_expressions() {
        let secret = "s3cr3t-value";
        let f = flow(
            vec![
                node(
                    "in1",
                    FlowNodeKind::Input {
                        label: "Token".to_string(),
                        value: rocket_shared::VariableValue::simple(secret),
                    },
                ),
                node(
                    "if1",
                    FlowNodeKind::If {
                        label: "Check".to_string(),
                        condition: format!("response.body.token === '{secret}'"),
                    },
                ),
                node(
                    "sw",
                    FlowNodeKind::Switch {
                        label: "Route".to_string(),
                        value: "{{api_key}}".to_string(),
                        cases: vec![SwitchCase {
                            id: "c1".to_string(),
                            label: String::new(),
                            matches: secret.to_string(),
                        }],
                    },
                ),
                node(
                    "a1",
                    FlowNodeKind::Auth {
                        label: "Sign in".to_string(),
                        auth: rocket_shared::types::Auth::Bearer {
                            token: secret.to_string(),
                        },
                        apply_to_inherit: false,
                    },
                ),
                output("out"),
            ],
            vec![
                edge("e1", "in1", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "in1", handle::RESULT, "sw", handle::INPUT),
            ],
        );
        let lints = lint(&f);
        assert!(!lints.is_empty());
        for l in &lints {
            let text = format!("{} {}", l.message, l.hint.as_deref().unwrap_or(""));
            assert!(!text.contains("s3cr3t"), "leaked in: {text}");
            assert!(!text.contains("api_key"), "leaked in: {text}");
        }
    }

    /// in -> if0 -> if1 -> ... -> out, each If with an unwired 'false' exit.
    fn chain_flow(n: usize) -> Flow {
        let mut nodes = vec![input("in")];
        let mut edges = vec![edge("e-in", "in", handle::RESULT, "if0", handle::INPUT)];
        for i in 0..n {
            nodes.push(if_node(&format!("if{i}"), &format!("Check {i}")));
            let (next, field) = if i + 1 == n {
                ("out".to_string(), "value")
            } else {
                (format!("if{}", i + 1), handle::INPUT)
            };
            edges.push(edge(&format!("e{i}"), &format!("if{i}"), handle::TRUE, &next, field));
        }
        nodes.push(output("out"));
        flow(nodes, edges)
    }

    /// n Output nodes wired into one big cycle.
    fn ring_flow(n: usize) -> Flow {
        let nodes: Vec<FlowNode> = (0..n).map(|i| output(&format!("n{i}"))).collect();
        let edges: Vec<FlowEdge> = (0..n)
            .map(|i| {
                edge(
                    &format!("e{i}"),
                    &format!("n{i}"),
                    handle::RESULT,
                    &format!("n{}", (i + 1) % n),
                    "value",
                )
            })
            .collect();
        flow(nodes, edges)
    }

    /// The fastest of three runs of `work` on a flow of `n` nodes.
    fn best_of_three(n: usize, build: fn(usize) -> Flow, work: fn(&Flow)) -> std::time::Duration {
        let f = build(n);
        (0..3)
            .map(|_| {
                let started = std::time::Instant::now();
                work(&f);
                started.elapsed()
            })
            .min()
            .unwrap_or_default()
    }

    /// A linear rule grows about 8x from 2,000 to 16,000 nodes and a
    /// quadratic one about 64x, so a ratio under 20 tells them apart
    /// without depending on the speed of the machine.
    fn assert_linear(build: fn(usize) -> Flow, work: fn(&Flow)) {
        let small = best_of_three(2_000, build, work).max(std::time::Duration::from_micros(50));
        let big = best_of_three(16_000, build, work);
        assert!(
            big < small * 20,
            "not linear: 2,000 nodes took {small:?}, 16,000 took {big:?}"
        );
        assert!(big < std::time::Duration::from_secs(10), "took {big:?}");
    }

    #[test]
    fn a_large_flow_lints_in_linear_time() {
        let lints = lint(&chain_flow(20_000));
        assert_eq!(lints.len(), 20_000, "one unwired 'false' exit per If");
        assert!(lints.iter().all(|l| l.code == EXIT_WITHOUT_EDGE));
        // A rule that scans every wire for every node is quadratic.
        assert_linear(chain_flow, |f| {
            let _ = lint(f);
        });
    }

    #[test]
    fn a_cycle_becomes_one_error_per_node_and_wire() {
        let f = flow(
            vec![output("a"), output("b")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "value"),
                edge("e2", "b", handle::RESULT, "a", "value"),
            ],
        );
        let lints = graph_lints(&f);
        assert_eq!(
            keys(&lints),
            vec![
                (Some("a"), None, INVALID_GRAPH),
                (Some("b"), None, INVALID_GRAPH),
                (None, Some("e1"), INVALID_GRAPH),
                (None, Some("e2"), INVALID_GRAPH),
            ]
        );
        assert!(lints.iter().all(|l| l.severity == LintSeverity::Error));
        assert_eq!(
            lints[0].message,
            "'a' is part of a loop. A flow must not lead back to itself."
        );
    }

    #[test]
    fn an_invalid_node_names_its_label() {
        let f = flow(vec![if_node("if1", "Logged in?")], vec![]);
        let lints = graph_lints(&f);
        assert_eq!(keys(&lints), vec![(Some("if1"), None, INVALID_GRAPH)]);
        assert_eq!(
            lints[0].message,
            "'Logged in?': The If node needs exactly one input wire, found 0."
        );
        assert!(lints[0].hint.is_some());
    }

    #[test]
    fn an_invalid_edge_carries_the_edge_id() {
        let f = flow(
            vec![output("a"), output("b")],
            vec![edge("e9", "a", handle::RESULT, "b", "value")],
        );
        let lints = graph_lints(&f);
        assert_eq!(keys(&lints), vec![(None, Some("e9"), INVALID_GRAPH)]);
        assert!(lints[0].message.ends_with('.'));
    }

    #[test]
    fn an_unknown_node_is_reported_on_the_wire_that_names_it() {
        let f = flow(
            vec![input("in1"), output("out")],
            vec![
                edge("e1", "in1", handle::RESULT, "out", "value"),
                edge("e2", "in1", handle::RESULT, "ghost", "value"),
            ],
        );
        let lints = graph_lints(&f);
        assert_eq!(keys(&lints), vec![(None, Some("e2"), INVALID_GRAPH)]);
        assert!(!lints[0].message.contains("ghost"));
    }

    #[test]
    fn a_duplicate_node_id_is_reported_on_that_id() {
        let f = flow(vec![output("a"), output("a")], vec![]);
        let lints = graph_lints(&f);
        assert_eq!(keys(&lints), vec![(Some("a"), None, INVALID_GRAPH)]);
    }

    const CANARY: &str = "sk-live-canary-9f3a";

    fn assert_no_canary(lints: &[FlowLint]) {
        for l in lints {
            let all = format!(
                "{:?} {:?} {} {:?}",
                l.node_id, l.edge_id, l.message, l.hint
            );
            assert!(!all.contains(CANARY), "leaked in: {all}");
        }
    }

    #[test]
    fn a_duplicate_switch_match_value_is_never_echoed() {
        let sw = switch_node("sw", "Route", &[("A", CANARY), ("B", CANARY)]);
        let f = flow(
            vec![input("in1"), sw, output("out")],
            vec![
                edge("e1", "in1", handle::RESULT, "sw", handle::INPUT),
                edge("e2", "sw", handle::DEFAULT, "out", "value"),
            ],
        );
        let graph = graph_lints(&f);
        assert_eq!(keys(&graph), vec![(Some("sw"), None, INVALID_GRAPH)]);
        assert_no_canary(&graph);
        assert_eq!(
            graph[0].message,
            "'Route': Two cases of this Switch have the same match value."
        );
        assert_no_canary(&lint(&f));
    }

    #[test]
    fn a_bad_callback_name_is_never_echoed() {
        let wait = |id: &str, name: &str| {
            node(
                id,
                FlowNodeKind::WaitForCallback {
                    label: id.to_string(),
                    name: name.to_string(),
                    timeout_ms: 60_000,
                    accept_when: None,
                },
            )
        };
        let bad = flow(vec![wait("w1", CANARY)], vec![]);
        let graph = graph_lints(&bad);
        assert!(!graph.is_empty());
        assert_no_canary(&graph);
        assert_no_canary(&lint(&bad));
        let same = flow(vec![wait("w1", "pay"), wait("w2", "pay")], vec![]);
        let lints = graph_lints(&same);
        assert!(lints[0].message.contains("same name"), "{}", lints[0].message);
        // The callback name is valid here, and only the order rule quotes it.
        let late = flow(
            vec![
                node(
                    "r1",
                    FlowNodeKind::Request {
                        label: "Pay".to_string(),
                        source: RequestSource::Inline {
                            request: crate::node::InlineRequestData {
                                method: "POST".to_string(),
                                url: "https://x.test/{{callback.canarycallback9f3a}}".to_string(),
                                headers: Vec::new(),
                                body: None,
                            },
                        },
                        debug: false,
                        repeat_until: None,
                    },
                ),
                wait("w1", "canarycallback9f3a"),
            ],
            vec![],
        );
        let lints = graph_lints(&late);
        assert_eq!(keys(&lints), vec![(Some("w1"), None, INVALID_GRAPH)]);
        assert!(!lints[0].message.contains("canarycallback9f3a"));
    }

    #[test]
    fn a_bad_wire_target_or_exit_name_is_never_echoed() {
        let f = flow(
            vec![input("in1"), if_node("if1", "Check"), output("out")],
            vec![edge("e1", "in1", handle::RESULT, "if1", CANARY)],
        );
        assert_no_canary(&graph_lints(&f));
        let g = flow(
            vec![input("in1"), output("out")],
            vec![edge("e1", "in1", CANARY, "out", "value")],
        );
        assert_no_canary(&graph_lints(&g));
    }

    #[test]
    fn a_huge_ring_reports_its_cycle_in_linear_time() {
        let f = ring_flow(20_000);
        let error = validate(&f).expect_err("a ring is a cycle");
        assert_eq!(graph_error_lints(&f, &error).len(), 40_000);
        assert_linear(ring_flow, |f| {
            if let Err(error) = validate(f) {
                let _ = graph_error_lints(f, &error);
            }
        });
    }

    #[test]
    fn a_node_that_feeds_an_output_only_through_a_cycle_is_handled() {
        let f = flow(
            vec![input("in1"), output("a"), output("b")],
            vec![
                edge("e1", "in1", handle::RESULT, "a", "value"),
                edge("e2", "a", handle::RESULT, "b", "value"),
                edge("e3", "b", handle::RESULT, "a", "value"),
            ],
        );
        assert!(lint(&f).is_empty());
    }

    #[test]
    fn a_duplicate_id_output_does_not_panic_or_double_lint() {
        let f = flow(
            vec![input("in1"), output("out"), output("out"), input("lonely")],
            vec![edge("e1", "in1", handle::RESULT, "out", "value")],
        );
        assert_eq!(keys(&lint(&f)), vec![(Some("lonely"), None, NO_PATH_TO_OUTPUT)]);
        assert_eq!(
            keys(&graph_lints(&f)),
            vec![(Some("out"), None, INVALID_GRAPH)]
        );
    }

    #[test]
    fn a_routing_node_without_a_path_does_not_claim_a_result() {
        let f = flow(
            vec![input("in1"), if_node("if1", "Check"), input("in2"), output("out")],
            vec![
                edge("e1", "in1", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "in2", handle::RESULT, "out", "value"),
            ],
        );
        let lints = lint(&f);
        let found = only(&lints, NO_PATH_TO_OUTPUT);
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[1].message,
            "'Check' does not lead to any Output, so nothing after it reaches an Output."
        );
    }

    #[test]
    fn mapped_reasons_read_as_fixed_sentences() {
        let cases = [
            (
                "more than one case matches 'x'",
                "Two cases of this Switch have the same match value.",
            ),
            (
                "more than one case has id 'x'",
                "Two cases of this Switch have the same id.",
            ),
            (
                "the source node has no exit named 'x'",
                "The wire leaves an exit that the source node does not have.",
            ),
            (
                "the Wait for callback name 'x y' must use only letters, digits and _",
                "The Wait for callback name must use only letters, digits and _.",
            ),
            (
                "more than one Wait for callback node is named 'x'",
                "More than one Wait for callback node has the same name.",
            ),
            (
                "Request 'r1' sends {{callback.x}} but is not wired before this Wait for callback node; wire its 'result' exit into this node's 'trigger' input",
                "A Request uses a callback value but is not wired before this Wait for callback node.",
            ),
            (
                "the If node's wire must target 'input', not 'x'",
                "The If node's wire must target its 'input' handle.",
            ),
            (
                "the Transform node's wire must target 'input', not 'x'",
                "The Transform node's wire must target its 'input' handle.",
            ),
            (
                "the Switch node needs exactly one input wire, found 3",
                "The Switch node needs exactly one input wire, found 3.",
            ),
            (
                "the If node's condition is empty",
                "The If node's condition is empty.",
            ),
            (
                "the Switch node's value is empty",
                "The Switch node's value is empty.",
            ),
            (
                "the Transform node's script is empty",
                "The Transform node's script is empty.",
            ),
            (
                "the Wait for callback node's accept_when is empty",
                "The Wait for callback node's accept_when is empty.",
            ),
            (
                "only If, Switch and Transform nodes have an 'input' input",
                "Only If, Switch and Transform nodes have an 'input' input.",
            ),
            (
                "only Request, Output and Wait for callback nodes have a 'trigger' input",
                "Only Request, Output and Wait for callback nodes have a 'trigger' input.",
            ),
            ("Input nodes cannot receive wires", "Input nodes cannot receive wires."),
            ("Auth nodes cannot receive wires", "Auth nodes cannot receive wires."),
            (
                "an 'auth' wire must go from an Auth node into a Request node",
                "An 'auth' wire must go from an Auth node into a Request node.",
            ),
            (
                "a Request node can take only one 'auth' wire",
                "A Request node can take only one 'auth' wire.",
            ),
            (
                "Wait for callback nodes only have a 'trigger' input",
                "Wait for callback nodes only have a 'trigger' input.",
            ),
            (
                "the repeat-until condition is empty",
                "The repeat-until condition is empty.",
            ),
            (
                "repeat-until interval must be at least 100 ms",
                "Repeat-until interval must be at least 100 ms.",
            ),
            (
                "repeat-until max attempts must be between 1 and 50",
                "Repeat-until max attempts must be between 1 and 50.",
            ),
            (
                "repeat-until timeout must be at most 600000 ms",
                "Repeat-until timeout must be at most 600000 ms.",
            ),
            (
                "repeat-until timeout must not be shorter than the interval",
                "Repeat-until timeout must not be shorter than the interval.",
            ),
            (
                "the Auth node needs an auth type other than none or inherit",
                "The Auth node needs an auth type other than none or inherit.",
            ),
            (
                "only one Auth node can apply to inherited auth; turn this one or the other off",
                "Only one Auth node can apply to inherited auth; turn this one or the other off.",
            ),
            (
                "the Wait for callback timeout must be between 1000 and 3600000 ms",
                "The Wait for callback timeout must be between 1000 and 3600000 ms.",
            ),
        ];
        for (reason, expected) in cases {
            assert_eq!(sentence(reason), expected, "for: {reason}");
        }
    }

    #[test]
    fn an_unknown_or_malformed_reason_gets_the_generic_sentence() {
        let generic = "The flow has a structural problem.";
        assert_eq!(sentence("something sk-live-canary-9f3a new"), generic);
        assert_eq!(sentence("it holds 'sk-live-canary-9f3a"), generic);
        assert_eq!(sentence(""), generic);
        // A known shape with a non-numeric count is not trusted.
        assert_eq!(
            sentence("the If node needs exactly one input wire, found sk-live"),
            generic
        );
        assert_eq!(
            sentence("the Foo node's condition is empty"),
            generic
        );
    }
}
