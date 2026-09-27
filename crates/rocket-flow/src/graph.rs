use crate::flow::Flow;
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum FlowGraphError {
    /// Holds the nodes that lie on a cycle (or on a path between two
    /// cycles). Nodes that are only downstream of a cycle are left out.
    #[error("cycle detected through node(s): {node_ids:?}")]
    Cycle { node_ids: Vec<String> },
    #[error("edge references unknown node: {node_id}")]
    UnknownNode { node_id: String },
    /// Two nodes in `flow.nodes` share the same id.
    #[error("duplicate node id: {node_id}")]
    DuplicateNode { node_id: String },
}

/// Kahn's-algorithm topological sort. Returns node ids in an order where
/// every node appears after all nodes it depends on (i.e. after every node
/// that has an edge pointing *into* it). Independent nodes/branches may
/// appear in either relative order.
///
/// Duplicate node ids and edges to unknown nodes are rejected before any
/// sorting starts.
pub fn topological_sort(flow: &Flow) -> Result<Vec<String>, FlowGraphError> {
    let mut node_ids: HashSet<&str> = HashSet::with_capacity(flow.nodes.len());
    for node in &flow.nodes {
        if !node_ids.insert(node.id.as_str()) {
            return Err(FlowGraphError::DuplicateNode {
                node_id: node.id.clone(),
            });
        }
    }

    for edge in &flow.edges {
        if !node_ids.contains(edge.source_node_id.as_str()) {
            return Err(FlowGraphError::UnknownNode {
                node_id: edge.source_node_id.clone(),
            });
        }
        if !node_ids.contains(edge.target_node_id.as_str()) {
            return Err(FlowGraphError::UnknownNode {
                node_id: edge.target_node_id.clone(),
            });
        }
    }

    let mut in_degree: HashMap<&str, usize> =
        flow.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut adjacency: HashMap<&str, Vec<&str>> = flow
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), Vec::new()))
        .collect();

    for edge in &flow.edges {
        adjacency
            .get_mut(edge.source_node_id.as_str())
            .expect("source_node_id validated against node_ids above")
            .push(edge.target_node_id.as_str());
        *in_degree
            .get_mut(edge.target_node_id.as_str())
            .expect("target_node_id validated against node_ids above") += 1;
    }

    // Node ids are unique, so each node is enqueued at most once. Each edge
    // is therefore walked at most once, and a degree never drops below zero.
    let mut queue: VecDeque<&str> = flow
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .filter(|id| in_degree.get(id) == Some(&0))
        .collect();

    let mut order: Vec<String> = Vec::with_capacity(flow.nodes.len());
    while let Some(id) = queue.pop_front() {
        order.push(id.to_string());
        for &next in adjacency
            .get(id)
            .expect("every node id was inserted into adjacency above")
        {
            let degree = in_degree
                .get_mut(next)
                .expect("next came from adjacency, built only from validated node ids");
            *degree -= 1;
            if *degree == 0 {
                queue.push_back(next);
            }
        }
    }

    if order.len() != flow.nodes.len() {
        return Err(FlowGraphError::Cycle {
            node_ids: cycle_nodes(flow, &in_degree),
        });
    }

    Ok(order)
}

/// Picks the nodes to report after Kahn's algorithm stalls. Every node it
/// did not emit still has a non-zero in-degree. That set also holds nodes
/// that only sit downstream of a cycle, so this peels those off by running
/// Kahn's algorithm backwards over the leftover subgraph.
fn cycle_nodes(flow: &Flow, in_degree: &HashMap<&str, usize>) -> Vec<String> {
    let leftover: HashSet<&str> = in_degree
        .iter()
        .filter(|(_, &degree)| degree > 0)
        .map(|(&id, _)| id)
        .collect();

    let mut out_degree: HashMap<&str, usize> = leftover.iter().map(|&id| (id, 0)).collect();
    let mut predecessors: HashMap<&str, Vec<&str>> =
        leftover.iter().map(|&id| (id, Vec::new())).collect();
    for edge in &flow.edges {
        let (source, target) = (edge.source_node_id.as_str(), edge.target_node_id.as_str());
        if leftover.contains(source) && leftover.contains(target) {
            *out_degree
                .get_mut(source)
                .expect("source is in leftover, which seeded out_degree") += 1;
            predecessors
                .get_mut(target)
                .expect("target is in leftover, which seeded predecessors")
                .push(source);
        }
    }

    let mut queue: VecDeque<&str> = out_degree
        .iter()
        .filter(|(_, &degree)| degree == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut peeled: HashSet<&str> = HashSet::new();
    while let Some(id) = queue.pop_front() {
        peeled.insert(id);
        for &prev in predecessors
            .get(id)
            .expect("queued ids come from out_degree, which shares keys with predecessors")
        {
            let degree = out_degree
                .get_mut(prev)
                .expect("prev came from predecessors, built only from leftover ids");
            *degree -= 1;
            if *degree == 0 {
                queue.push_back(prev);
            }
        }
    }

    // Keep the caller's node order so the error message is stable.
    flow.nodes
        .iter()
        .map(|n| n.id.as_str())
        .filter(|id| leftover.contains(id) && !peeled.contains(id))
        .map(str::to_string)
        .collect()
}

/// Every node id reachable by following edges forward from `start_node_id`
/// (exclusive) — a plain BFS. Used to compute which nodes must be skipped
/// when `start_node_id` fails during execution.
pub fn reachable_from(flow: &Flow, start_node_id: &str) -> Vec<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    queue.push_back(start_node_id);

    while let Some(current) = queue.pop_front() {
        for edge in flow.edges.iter().filter(|e| e.source_node_id == current) {
            if visited.insert(edge.target_node_id.clone()) {
                queue.push_back(&edge.target_node_id);
            }
        }
    }

    visited.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowEdge;
    use crate::flow::FlowNode;
    use crate::node::{FlowNodeKind, NodePosition};

    fn node(id: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Output {
                label: id.to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn edge(id: &str, from: &str, to: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: to.to_string(),
            target_field: "value".to_string(),
            expression: "response.body".to_string(),
        }
    }

    #[test]
    fn empty_flow_sorts_to_empty_order() {
        let flow = Flow {
            name: "empty".to_string(),
            nodes: vec![],
            edges: vec![],
        };
        assert_eq!(topological_sort(&flow), Ok(vec![]));
    }

    #[test]
    fn flow_with_no_edges_orders_every_node_once() {
        let flow = Flow {
            name: "disconnected".to_string(),
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![],
        };
        let mut order = topological_sort(&flow).expect("no cycle");
        order.sort();
        assert_eq!(
            order,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn linear_chain_sorts_in_dependency_order() {
        let flow = Flow {
            name: "chain".to_string(),
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![edge("e1", "a", "b"), edge("e2", "b", "c")],
        };
        let order = topological_sort(&flow).expect("no cycle");
        assert_eq!(
            order,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn diamond_places_dependencies_before_dependents() {
        let flow = Flow {
            name: "diamond".to_string(),
            nodes: vec![node("a"), node("b"), node("c"), node("d")],
            edges: vec![
                edge("e1", "a", "b"),
                edge("e2", "a", "c"),
                edge("e3", "b", "d"),
                edge("e4", "c", "d"),
            ],
        };
        let order = topological_sort(&flow).expect("no cycle");
        let pos = |id: &str| order.iter().position(|x| x == id).expect("node present");
        assert!(pos("a") < pos("b"));
        assert!(pos("a") < pos("c"));
        assert!(pos("b") < pos("d"));
        assert!(pos("c") < pos("d"));
    }

    #[test]
    fn self_loop_is_reported_as_cycle() {
        let flow = Flow {
            name: "self-loop".to_string(),
            nodes: vec![node("a")],
            edges: vec![edge("e1", "a", "a")],
        };
        let err = topological_sort(&flow).expect_err("must detect cycle");
        match err {
            FlowGraphError::Cycle { node_ids } => assert_eq!(node_ids, vec!["a".to_string()]),
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn longer_cycle_is_reported_with_all_member_nodes() {
        let flow = Flow {
            name: "cycle".to_string(),
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![
                edge("e1", "a", "b"),
                edge("e2", "b", "c"),
                edge("e3", "c", "a"),
            ],
        };
        let err = topological_sort(&flow).expect_err("must detect cycle");
        match err {
            FlowGraphError::Cycle { mut node_ids } => {
                node_ids.sort();
                assert_eq!(
                    node_ids,
                    vec!["a".to_string(), "b".to_string(), "c".to_string()]
                );
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn edge_referencing_unknown_source_node_is_rejected() {
        let flow = Flow {
            name: "bad-source".to_string(),
            nodes: vec![node("a")],
            edges: vec![edge("e1", "ghost", "a")],
        };
        assert_eq!(
            topological_sort(&flow),
            Err(FlowGraphError::UnknownNode {
                node_id: "ghost".to_string()
            })
        );
    }

    #[test]
    fn edge_referencing_unknown_target_node_is_rejected() {
        let flow = Flow {
            name: "bad-target".to_string(),
            nodes: vec![node("a")],
            edges: vec![edge("e1", "a", "ghost")],
        };
        assert_eq!(
            topological_sort(&flow),
            Err(FlowGraphError::UnknownNode {
                node_id: "ghost".to_string()
            })
        );
    }

    #[test]
    fn cycle_error_leaves_out_nodes_only_downstream_of_the_cycle() {
        // z -> a <-> b -> c -> d: z is upstream, c and d are downstream.
        let flow = Flow {
            name: "cycle-with-tail".to_string(),
            nodes: vec![node("z"), node("a"), node("b"), node("c"), node("d")],
            edges: vec![
                edge("e0", "z", "a"),
                edge("e1", "a", "b"),
                edge("e2", "b", "a"),
                edge("e3", "b", "c"),
                edge("e4", "c", "d"),
            ],
        };
        assert_eq!(
            topological_sort(&flow),
            Err(FlowGraphError::Cycle {
                node_ids: vec!["a".to_string(), "b".to_string()]
            })
        );
    }

    #[test]
    fn duplicate_node_id_is_rejected_without_panicking() {
        let flow = Flow {
            name: "dup".to_string(),
            nodes: vec![node("a"), node("a"), node("b")],
            edges: vec![edge("e1", "a", "b")],
        };
        assert_eq!(
            topological_sort(&flow),
            Err(FlowGraphError::DuplicateNode {
                node_id: "a".to_string()
            })
        );
    }

    #[test]
    fn parallel_edges_between_the_same_nodes_still_sort() {
        let flow = Flow {
            name: "parallel".to_string(),
            nodes: vec![node("b"), node("a")],
            edges: vec![edge("e1", "a", "b"), edge("e2", "a", "b")],
        };
        assert_eq!(
            topological_sort(&flow),
            Ok(vec!["a".to_string(), "b".to_string()])
        );
    }

    fn diamond_flow() -> Flow {
        // a -> b -> d
        //  \-> c -/
        // plus an unrelated, disconnected node e.
        Flow {
            name: "diamond".to_string(),
            nodes: vec!["a", "b", "c", "d", "e"]
                .into_iter()
                .map(|id| FlowNode {
                    id: id.to_string(),
                    kind: FlowNodeKind::Output { label: id.to_string() },
                    position: NodePosition { x: 0.0, y: 0.0 },
                })
                .collect(),
            edges: vec![
                edge_fixture("e1", "a", "b"),
                edge_fixture("e2", "a", "c"),
                edge_fixture("e3", "b", "d"),
                edge_fixture("e4", "c", "d"),
            ],
        }
    }

    fn edge_fixture(id: &str, source: &str, target: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            target_field: "value".to_string(),
            expression: "response.body".to_string(),
        }
    }

    #[test]
    fn reachable_from_root_includes_every_downstream_node_once() {
        let flow = diamond_flow();
        let mut reached = reachable_from(&flow, "a");
        reached.sort();
        assert_eq!(reached, vec!["b".to_string(), "c".to_string(), "d".to_string()]);
    }

    #[test]
    fn reachable_from_excludes_unrelated_disconnected_nodes() {
        let flow = diamond_flow();
        let reached = reachable_from(&flow, "a");
        assert!(!reached.contains(&"e".to_string()));
    }

    #[test]
    fn reachable_from_a_leaf_node_is_empty() {
        let flow = diamond_flow();
        assert!(reachable_from(&flow, "d").is_empty());
    }
}
