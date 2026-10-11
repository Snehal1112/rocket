//! Plain-language names for nodes, wires and exits. The lint layer and the
//! save-error text share these, so both name a graph element the same way.
//! Pure functions, no I/O.

use crate::flow::{Flow, FlowNode};
use crate::handle;
use crate::node::{FlowNodeKind, SwitchCase};
use crate::validate::kind_name;

/// The node's label, or its kind name when the label is blank.
pub fn display_label(node: &FlowNode) -> &str {
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

fn case_name(case: &SwitchCase) -> &str {
    let label = case.label.trim();
    if label.is_empty() {
        "unnamed"
    } else {
        label
    }
}

/// The exits a node of this kind has, as a clause that follows a semicolon.
/// A Switch lists its cases by label. The case match values are never used.
pub fn exits_text(kind: &FlowNodeKind) -> String {
    match kind {
        FlowNodeKind::Output { .. } => "an Output node has no exits".to_string(),
        FlowNodeKind::If { .. } => {
            format!("its exits are '{}' and '{}'", handle::TRUE, handle::FALSE)
        }
        FlowNodeKind::Switch { cases, .. } => {
            let mut text = format!("its exits are '{}'", handle::DEFAULT);
            if !cases.is_empty() {
                let names: Vec<String> = cases
                    .iter()
                    .map(|c| format!("'{}'", case_name(c)))
                    .collect();
                text.push_str(" and the case exits ");
                text.push_str(&names.join(", "));
            }
            text
        }
        FlowNodeKind::Request { .. }
        | FlowNodeKind::Input { .. }
        | FlowNodeKind::WaitForCallback { .. }
        | FlowNodeKind::Transform { .. }
        | FlowNodeKind::Auth { .. } => format!("its only exit is '{}'", handle::RESULT),
    }
}

/// The labels of the source and target nodes of wire `edge_id`. `None` when
/// the wire is missing or one of its ends names no node. A repeated node id
/// resolves to the first node with that id.
pub fn wire_ends<'a>(flow: &'a Flow, edge_id: &str) -> Option<(&'a str, &'a str)> {
    let edge = flow.edges.iter().find(|e| e.id == edge_id)?;
    let label = |id: &str| flow.nodes.iter().find(|n| n.id == id).map(display_label);
    Some((label(&edge.source_node_id)?, label(&edge.target_node_id)?))
}

/// The kind of the node that wire `edge_id` leaves.
pub fn source_kind<'a>(flow: &'a Flow, edge_id: &str) -> Option<&'a FlowNodeKind> {
    let edge = flow.edges.iter().find(|e| e.id == edge_id)?;
    flow.nodes
        .iter()
        .find(|n| n.id == edge.source_node_id)
        .map(|n| &n.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowEdge;
    use crate::node::{NodePosition, RequestSource};

    fn node(id: &str, kind: FlowNodeKind) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn request(id: &str, label: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                debug: false,
                repeat_until: None,
                label: label.to_string(),
                source: RequestSource::Saved {
                    request_path: format!("{id}.yml"),
                },
            },
        )
    }

    fn output(id: &str, label: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Output {
                label: label.to_string(),
            },
        )
    }

    fn switch(cases: &[(&str, &str)]) -> FlowNodeKind {
        FlowNodeKind::Switch {
            label: "Route".to_string(),
            value: "response.body.plan".to_string(),
            cases: cases
                .iter()
                .map(|(id, label)| crate::node::SwitchCase {
                    id: id.to_string(),
                    label: label.to_string(),
                    matches: "x".to_string(),
                })
                .collect(),
        }
    }

    fn wire(id: &str, from: &str, to: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: to.to_string(),
            target_field: "value".to_string(),
            expression: String::new(),
            source_handle: crate::handle::RESULT.to_string(),
        }
    }

    fn two_node_flow() -> Flow {
        Flow {
            name: "f".to_string(),
            nodes: vec![request("a", "Login"), output("b", "  ")],
            edges: vec![wire("e1", "a", "b"), wire("e2", "a", "ghost")],
            callback_host: None,
        }
    }

    #[test]
    fn a_blank_label_falls_back_to_the_kind_name() {
        let f = two_node_flow();
        assert_eq!(display_label(&f.nodes[0]), "Login");
        assert_eq!(display_label(&f.nodes[1]), "Output");
    }

    #[test]
    fn wire_ends_names_both_nodes_or_nothing() {
        let f = two_node_flow();
        assert_eq!(wire_ends(&f, "e1"), Some(("Login", "Output")));
        assert_eq!(wire_ends(&f, "e2"), None);
        assert_eq!(wire_ends(&f, "missing"), None);
    }

    #[test]
    fn source_kind_finds_the_node_a_wire_leaves() {
        let f = two_node_flow();
        assert!(matches!(
            source_kind(&f, "e1"),
            Some(FlowNodeKind::Request { .. })
        ));
        assert!(source_kind(&f, "missing").is_none());
    }

    #[test]
    fn exits_text_lists_the_exits_of_each_kind() {
        let f = two_node_flow();
        assert_eq!(exits_text(&f.nodes[0].kind), "its only exit is 'result'");
        assert_eq!(exits_text(&f.nodes[1].kind), "an Output node has no exits");
        let if_kind = FlowNodeKind::If {
            label: "Check".to_string(),
            condition: "true".to_string(),
        };
        assert_eq!(exits_text(&if_kind), "its exits are 'true' and 'false'");
        assert_eq!(exits_text(&switch(&[])), "its exits are 'default'");
        assert_eq!(
            exits_text(&switch(&[("c1", "Gold"), ("c2", " ")])),
            "its exits are 'default' and the case exits 'Gold', 'unnamed'"
        );
    }
}
