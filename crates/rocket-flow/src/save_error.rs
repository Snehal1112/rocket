//! The save-error text of an invalid flow. Every message that names graph elements ends with
//! "node(s): <ids>; edge(s): <ids>", which the canvas parses to mark them.

use std::collections::HashSet;

use crate::describe::{display_label, wire_ends};
use crate::flow::{Flow, FlowNode};
use crate::graph::FlowGraphError;
use crate::validate::ValidationReport;

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
pub(crate) fn graph_errors_message(flow: &Flow, report: &ValidationReport) -> String {
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
