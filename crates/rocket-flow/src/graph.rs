use crate::flow::Flow;
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum FlowGraphError {
    #[error("cycle detected through node(s): {node_ids:?}")]
    Cycle { node_ids: Vec<String> },
    #[error("edge references unknown node: {node_id}")]
    UnknownNode { node_id: String },
}

pub fn topological_sort(flow: &Flow) -> Result<Vec<String>, FlowGraphError> {
    let node_ids: HashSet<&str> = flow.nodes.iter().map(|n| n.id.as_str()).collect();

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
    let mut adjacency: HashMap<&str, Vec<&str>> =
        flow.nodes.iter().map(|n| (n.id.as_str(), Vec::new())).collect();

    for edge in &flow.edges {
        adjacency
            .get_mut(edge.source_node_id.as_str())
            .expect("source_node_id validated against node_ids above")
            .push(edge.target_node_id.as_str());
        *in_degree
            .get_mut(edge.target_node_id.as_str())
            .expect("target_node_id validated against node_ids above") += 1;
    }

    let mut queue: VecDeque<&str> = flow
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .filter(|id| in_degree[id] == 0)
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
        let remaining: Vec<String> = flow
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .filter(|id| !order.contains(id))
            .collect();
        return Err(FlowGraphError::Cycle {
            node_ids: remaining,
        });
    }

    Ok(order)
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
        assert_eq!(order, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn linear_chain_sorts_in_dependency_order() {
        let flow = Flow {
            name: "chain".to_string(),
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![edge("e1", "a", "b"), edge("e2", "b", "c")],
        };
        let order = topological_sort(&flow).expect("no cycle");
        assert_eq!(order, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
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
}
