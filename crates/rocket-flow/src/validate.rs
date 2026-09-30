//! Save-time and load-time structural validation of a Flow graph. See spec
//! §7 (rules V1-V10). Rules run in table order and the first violation found
//! is returned, so the same file always yields the same error.

use crate::flow::{Flow, FlowEdge, FlowNode};
use crate::graph::{topological_sort, FlowGraphError};
use crate::handle;
use crate::node::{FlowNodeKind, CALLBACK_MAX_TIMEOUT_MS, CALLBACK_MIN_TIMEOUT_MS};
use std::collections::{HashMap, HashSet};

/// Validates `flow` and returns its node ids in topological order.
pub fn validate(flow: &Flow) -> Result<Vec<String>, FlowGraphError> {
    let order = topological_sort(flow)?;
    let kinds: HashMap<&str, &FlowNodeKind> = flow
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), &n.kind))
        .collect();

    check_routing_inputs(flow)?;
    check_edges(flow, &kinds, |edge, target, _| {
        (edge.target_field == handle::INPUT && !takes_input(target)).then(|| {
            format!(
                "only If, Switch and Transform nodes have an '{}' input",
                handle::INPUT
            )
        })
    })?;
    check_edges(flow, &kinds, |edge, target, _| {
        let accepts_trigger = matches!(
            target,
            FlowNodeKind::Request { .. }
                | FlowNodeKind::Output { .. }
                | FlowNodeKind::WaitForCallback { .. }
        );
        (edge.target_field == handle::TRIGGER && !accepts_trigger).then(|| {
            format!(
                "only Request, Output and Wait for callback nodes have a '{}' input",
                handle::TRIGGER
            )
        })
    })?;
    check_edges(flow, &kinds, |_, target, _| {
        matches!(target, FlowNodeKind::Input { .. })
            .then(|| "Input nodes cannot receive wires".to_string())
    })?;
    check_edges(flow, &kinds, |edge, target, _| {
        (matches!(target, FlowNodeKind::WaitForCallback { .. })
            && edge.target_field != handle::TRIGGER)
            .then(|| {
                format!(
                    "Wait for callback nodes only have a '{}' input",
                    handle::TRIGGER
                )
            })
    })?;
    check_edges(flow, &kinds, |edge, _, source| {
        (!source_handle_exists(source, &edge.source_handle))
            .then(|| format!("the source node has no exit named '{}'", edge.source_handle))
    })?;
    check_switch_cases(flow, |cases| {
        first_duplicate(cases.iter().map(|c| c.id.as_str()))
            .map(|id| format!("more than one case has id '{id}'"))
    })?;
    check_switch_cases(flow, |cases| {
        first_duplicate(cases.iter().map(|c| c.matches.as_str()))
            .map(|m| format!("more than one case matches '{m}'"))
    })?;
    check_expressions(flow)?;
    check_repeat_until(flow)?;
    check_wait_nodes(flow)?;

    Ok(order)
}

fn is_routing(kind: &FlowNodeKind) -> bool {
    matches!(kind, FlowNodeKind::If { .. } | FlowNodeKind::Switch { .. })
}

/// True for the node kinds that evaluate one upstream value through an
/// `input` handle: If, Switch and Transform.
fn takes_input(kind: &FlowNodeKind) -> bool {
    is_routing(kind) || matches!(kind, FlowNodeKind::Transform { .. })
}

fn kind_name(kind: &FlowNodeKind) -> &'static str {
    match kind {
        FlowNodeKind::Request { .. } => "Request",
        FlowNodeKind::Input { .. } => "Input",
        FlowNodeKind::Output { .. } => "Output",
        FlowNodeKind::If { .. } => "If",
        FlowNodeKind::Switch { .. } => "Switch",
        FlowNodeKind::WaitForCallback { .. } => "Wait for callback",
        FlowNodeKind::Transform { .. } => "Transform",
    }
}

fn invalid_node(node: &FlowNode, reason: String) -> FlowGraphError {
    FlowGraphError::InvalidNode {
        node_id: node.id.clone(),
        reason,
    }
}

/// V1: an If, Switch or Transform node has exactly one incoming edge, into `input`.
fn check_routing_inputs(flow: &Flow) -> Result<(), FlowGraphError> {
    for node in flow.nodes.iter().filter(|n| takes_input(&n.kind)) {
        let incoming: Vec<&FlowEdge> = flow
            .edges
            .iter()
            .filter(|e| e.target_node_id == node.id)
            .collect();
        let name = kind_name(&node.kind);
        match incoming.as_slice() {
            [edge] if edge.target_field == handle::INPUT => {}
            [edge] => {
                return Err(invalid_node(
                    node,
                    format!(
                        "the {name} node's wire must target '{}', not '{}'",
                        handle::INPUT,
                        edge.target_field
                    ),
                ))
            }
            _ => {
                return Err(invalid_node(
                    node,
                    format!(
                        "the {name} node needs exactly one input wire, found {}",
                        incoming.len()
                    ),
                ))
            }
        }
    }
    Ok(())
}

/// Runs one edge rule over every edge in file order. `rule` receives the
/// edge, its target kind and its source kind, and returns a reason when the
/// edge breaks the rule.
fn check_edges<F>(
    flow: &Flow,
    kinds: &HashMap<&str, &FlowNodeKind>,
    rule: F,
) -> Result<(), FlowGraphError>
where
    F: Fn(&FlowEdge, &FlowNodeKind, &FlowNodeKind) -> Option<String>,
{
    for edge in &flow.edges {
        let target = kind_of(kinds, &edge.target_node_id)?;
        let source = kind_of(kinds, &edge.source_node_id)?;
        if let Some(reason) = rule(edge, target, source) {
            return Err(FlowGraphError::InvalidEdge {
                edge_id: edge.id.clone(),
                reason,
            });
        }
    }
    Ok(())
}

/// `topological_sort` already rejected unknown ids, so this only fails if
/// that guarantee is ever broken.
fn kind_of<'a>(
    kinds: &HashMap<&str, &'a FlowNodeKind>,
    node_id: &str,
) -> Result<&'a FlowNodeKind, FlowGraphError> {
    kinds
        .get(node_id)
        .copied()
        .ok_or_else(|| FlowGraphError::UnknownNode {
            node_id: node_id.to_string(),
        })
}

/// V5: the exits each node kind has.
fn source_handle_exists(source: &FlowNodeKind, source_handle: &str) -> bool {
    match source {
        FlowNodeKind::Request { .. }
        | FlowNodeKind::Input { .. }
        | FlowNodeKind::WaitForCallback { .. }
        | FlowNodeKind::Transform { .. } => source_handle == handle::RESULT,
        FlowNodeKind::Output { .. } => false,
        FlowNodeKind::If { .. } => source_handle == handle::TRUE || source_handle == handle::FALSE,
        FlowNodeKind::Switch { cases, .. } => {
            source_handle == handle::DEFAULT
                || handle::case_id_from_handle(source_handle)
                    .is_some_and(|case_id| cases.iter().any(|c| c.id == case_id))
        }
    }
}

/// Runs one Switch-case rule (V6 or V7) over every Switch node in file order.
fn check_switch_cases<F>(flow: &Flow, rule: F) -> Result<(), FlowGraphError>
where
    F: Fn(&[crate::node::SwitchCase]) -> Option<String>,
{
    for node in &flow.nodes {
        if let FlowNodeKind::Switch { cases, .. } = &node.kind {
            if let Some(reason) = rule(cases) {
                return Err(invalid_node(node, reason));
            }
        }
    }
    Ok(())
}

fn first_duplicate<'a>(values: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = HashSet::new();
    values.into_iter().find(|v| !seen.insert(*v))
}

/// V8: an If condition, a Switch value and a Transform script must not be blank.
fn check_expressions(flow: &Flow) -> Result<(), FlowGraphError> {
    for node in &flow.nodes {
        let (expression, field) = match &node.kind {
            FlowNodeKind::If { condition, .. } => (condition, "condition"),
            FlowNodeKind::Switch { value, .. } => (value, "value"),
            FlowNodeKind::Transform { script, .. } => (script, "script"),
            _ => continue,
        };
        if expression.trim().is_empty() {
            return Err(invalid_node(
                node,
                format!("the {} node's {field} is empty", kind_name(&node.kind)),
            ));
        }
    }
    Ok(())
}

/// V9: a Request node's `repeat_until` settings are within their limits.
fn check_repeat_until(flow: &Flow) -> Result<(), FlowGraphError> {
    use crate::node::RepeatUntil;
    for node in &flow.nodes {
        let FlowNodeKind::Request {
            repeat_until: Some(r),
            ..
        } = &node.kind
        else {
            continue;
        };
        let reason = if r.condition.trim().is_empty() {
            Some("the repeat-until condition is empty".to_string())
        } else if r.interval_ms < RepeatUntil::MIN_INTERVAL_MS {
            Some(format!(
                "repeat-until interval must be at least {} ms",
                RepeatUntil::MIN_INTERVAL_MS
            ))
        } else if r.max_attempts < 1 || r.max_attempts > RepeatUntil::MAX_MAX_ATTEMPTS {
            Some(format!(
                "repeat-until max attempts must be between 1 and {}",
                RepeatUntil::MAX_MAX_ATTEMPTS
            ))
        } else if r.timeout_ms > RepeatUntil::MAX_TIMEOUT_MS {
            Some(format!(
                "repeat-until timeout must be at most {} ms",
                RepeatUntil::MAX_TIMEOUT_MS
            ))
        } else if r.timeout_ms < r.interval_ms {
            Some("repeat-until timeout must not be shorter than the interval".to_string())
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(invalid_node(node, reason));
        }
    }
    Ok(())
}

/// V10: a Wait for callback node has a usable, unique name, a timeout in
/// range, and no blank `accept_when`.
fn check_wait_nodes(flow: &Flow) -> Result<(), FlowGraphError> {
    let mut seen = HashSet::new();
    for node in &flow.nodes {
        let FlowNodeKind::WaitForCallback {
            name,
            timeout_ms,
            accept_when,
            ..
        } = &node.kind
        else {
            continue;
        };
        let valid_name =
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid_name {
            return Err(invalid_node(
                node,
                format!("the Wait for callback name '{name}' must use only letters, digits and _"),
            ));
        }
        if !seen.insert(name.as_str()) {
            return Err(invalid_node(
                node,
                format!("more than one Wait for callback node is named '{name}'"),
            ));
        }
        if !(CALLBACK_MIN_TIMEOUT_MS..=CALLBACK_MAX_TIMEOUT_MS).contains(timeout_ms) {
            return Err(invalid_node(
                node,
                format!(
                    "the Wait for callback timeout must be between {CALLBACK_MIN_TIMEOUT_MS} and {CALLBACK_MAX_TIMEOUT_MS} ms"
                ),
            ));
        }
        if accept_when.as_deref().is_some_and(|s| s.trim().is_empty()) {
            return Err(invalid_node(
                node,
                "the Wait for callback node's accept_when is empty".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowEdge, FlowNode};
    use crate::handle;
    use crate::node::{FlowNodeKind, NodePosition, RequestSource, SwitchCase};

    fn node(id: &str, kind: FlowNodeKind) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn request(id: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                debug: false,
                repeat_until: None,
                label: id.to_string(),
                source: RequestSource::Saved {
                    request_path: format!("{id}.yml"),
                },
            },
        )
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

    fn if_node(id: &str, condition: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::If {
                label: id.to_string(),
                condition: condition.to_string(),
            },
        )
    }

    fn switch_node(id: &str, cases: Vec<(&str, &str)>) -> FlowNode {
        node(
            id,
            FlowNodeKind::Switch {
                label: id.to_string(),
                value: "response.body.plan".to_string(),
                cases: cases
                    .into_iter()
                    .map(|(case_id, matches)| SwitchCase {
                        id: case_id.to_string(),
                        label: case_id.to_string(),
                        matches: matches.to_string(),
                    })
                    .collect(),
            },
        )
    }

    fn transform(id: &str, script: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Transform {
                label: id.to_string(),
                script: script.to_string(),
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

    fn invalid_node_id(result: Result<Vec<String>, FlowGraphError>) -> String {
        match result {
            Err(FlowGraphError::InvalidNode { node_id, .. }) => node_id,
            other => panic!("expected InvalidNode, got {other:?}"),
        }
    }

    fn invalid_edge_id(result: Result<Vec<String>, FlowGraphError>) -> String {
        match result {
            Err(FlowGraphError::InvalidEdge { edge_id, .. }) => edge_id,
            other => panic!("expected InvalidEdge, got {other:?}"),
        }
    }

    /// login -> if1 (input); if1.true -> profile (trigger); if1.false -> refresh (trigger).
    fn valid_if_flow() -> Flow {
        flow(
            vec![
                request("login"),
                if_node("if1", "response.status === 200"),
                request("profile"),
                request("refresh"),
            ],
            vec![
                edge("e1", "login", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "if1", handle::TRUE, "profile", handle::TRIGGER),
                edge("e3", "if1", handle::FALSE, "refresh", handle::TRIGGER),
            ],
        )
    }

    #[test]
    fn valid_if_flow_passes_and_returns_topological_order() {
        let order = validate(&valid_if_flow()).expect("valid flow");
        assert_eq!(order[0], "login");
        assert_eq!(order[1], "if1");
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn phase1_linear_flow_still_validates() {
        let f = flow(
            vec![input("in"), request("req"), output("out")],
            vec![
                edge("e1", "in", handle::RESULT, "req", "url"),
                edge("e2", "req", handle::RESULT, "out", "value"),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn cycles_are_still_reported_as_cycles() {
        let f = flow(
            vec![output("a"), output("b")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "value"),
                edge("e2", "b", handle::RESULT, "a", "value"),
            ],
        );
        assert!(matches!(validate(&f), Err(FlowGraphError::Cycle { .. })));
    }

    #[test]
    fn v1_if_without_input_is_rejected() {
        let f = flow(vec![if_node("if1", "true")], vec![]);
        assert_eq!(invalid_node_id(validate(&f)), "if1");
    }

    #[test]
    fn v1_switch_with_two_inputs_is_rejected() {
        let f = flow(
            vec![
                request("a"),
                request("b"),
                switch_node("sw1", vec![("c1", "x")]),
            ],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "b", handle::RESULT, "sw1", handle::INPUT),
            ],
        );
        assert_eq!(invalid_node_id(validate(&f)), "sw1");
    }

    #[test]
    fn v1_routing_input_must_target_the_input_field() {
        let f = flow(
            vec![request("a"), if_node("if1", "true")],
            vec![edge("e1", "a", handle::RESULT, "if1", "url")],
        );
        assert_eq!(invalid_node_id(validate(&f)), "if1");
    }

    /// in -> t (input); t -> out (value).
    fn valid_transform_flow() -> Flow {
        flow(
            vec![input("in"), transform("t", "return response.body;"), output("out")],
            vec![
                edge("e1", "in", handle::RESULT, "t", handle::INPUT),
                edge("e2", "t", handle::RESULT, "out", "value"),
            ],
        )
    }

    #[test]
    fn valid_transform_flow_passes() {
        let order = validate(&valid_transform_flow()).expect("valid flow");
        assert_eq!(order, vec!["in", "t", "out"]);
    }

    #[test]
    fn vt_transform_without_input_is_rejected() {
        let f = flow(vec![transform("t", "return 1;")], vec![]);
        assert_eq!(invalid_node_id(validate(&f)), "t");
    }

    #[test]
    fn vt_transform_with_two_inputs_is_rejected() {
        let f = flow(
            vec![request("a"), request("b"), transform("t", "return 1;")],
            vec![
                edge("e1", "a", handle::RESULT, "t", handle::INPUT),
                edge("e2", "b", handle::RESULT, "t", handle::INPUT),
            ],
        );
        assert_eq!(invalid_node_id(validate(&f)), "t");
    }

    #[test]
    fn vt_transform_input_must_target_the_input_field() {
        let f = flow(
            vec![request("a"), transform("t", "return 1;")],
            vec![edge("e1", "a", handle::RESULT, "t", "url")],
        );
        assert_eq!(invalid_node_id(validate(&f)), "t");
    }

    #[test]
    fn vt_transform_trigger_is_rejected() {
        let f = flow(
            vec![request("a"), transform("t", "return 1;")],
            vec![edge("e1", "a", handle::RESULT, "t", handle::TRIGGER)],
        );
        assert_eq!(invalid_node_id(validate(&f)), "t");
    }

    #[test]
    fn vt_blank_script_is_rejected() {
        let f = flow(
            vec![input("in"), transform("t", "  \n\t ")],
            vec![edge("e1", "in", handle::RESULT, "t", handle::INPUT)],
        );
        assert_eq!(invalid_node_id(validate(&f)), "t");
    }

    #[test]
    fn vt_transform_has_only_the_result_exit() {
        let f = flow(
            vec![input("in"), transform("t", "return 1;"), output("out")],
            vec![
                edge("e1", "in", handle::RESULT, "t", handle::INPUT),
                edge("e2", "t", handle::TRUE, "out", "value"),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn vt_transform_can_feed_several_consumers() {
        let f = flow(
            vec![
                input("in"),
                transform("t", "return 1;"),
                output("out1"),
                output("out2"),
            ],
            vec![
                edge("e1", "in", handle::RESULT, "t", handle::INPUT),
                edge("e2", "t", handle::RESULT, "out1", "value"),
                edge("e3", "t", handle::RESULT, "out2", "value"),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn vt_transform_can_sit_after_a_branch_exit() {
        let f = flow(
            vec![
                request("login"),
                if_node("if1", "response.status === 200"),
                transform("t", "return response.body;"),
            ],
            vec![
                edge("e1", "login", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "if1", handle::TRUE, "t", handle::INPUT),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn v2_input_field_on_a_request_node_is_rejected() {
        let f = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", handle::RESULT, "b", handle::INPUT)],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn v3_trigger_into_an_input_node_is_rejected() {
        let f = flow(
            vec![request("a"), input("in")],
            vec![edge("e1", "a", handle::RESULT, "in", handle::TRIGGER)],
        );
        match validate(&f) {
            Err(FlowGraphError::InvalidEdge { edge_id, reason }) => {
                assert_eq!(edge_id, "e1");
                assert!(
                    reason.contains("trigger"),
                    "V3 must fire before V4, got: {reason}"
                );
            }
            other => panic!("expected InvalidEdge, got {other:?}"),
        }
    }

    #[test]
    fn v3_trigger_into_request_and_output_is_valid() {
        let f = flow(
            vec![request("a"), request("b"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "b", handle::TRIGGER),
                edge("e2", "a", handle::RESULT, "out", handle::TRIGGER),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn v4_data_wire_into_an_input_node_is_rejected() {
        let f = flow(
            vec![request("a"), input("in")],
            vec![edge("e1", "a", handle::RESULT, "in", "value")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn v5_request_edge_with_unknown_exit_is_rejected() {
        let f = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", "true", "b", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn v5_empty_source_handle_is_rejected() {
        let f = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", "", "b", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn v5_if_edge_with_result_exit_is_rejected() {
        let mut f = valid_if_flow();
        f.edges[1].source_handle = handle::RESULT.to_string();
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn v5_edge_from_a_deleted_switch_case_is_rejected() {
        let f = flow(
            vec![
                request("a"),
                switch_node("sw1", vec![("c1", "x")]),
                request("b"),
            ],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge(
                    "e2",
                    "sw1",
                    &handle::case_handle("gone"),
                    "b",
                    handle::TRIGGER,
                ),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn v5_empty_case_handle_is_rejected() {
        let f = flow(
            vec![
                request("a"),
                switch_node("sw1", vec![("c1", "x")]),
                request("b"),
            ],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "sw1", handle::CASE_PREFIX, "b", handle::TRIGGER),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn v5_switch_case_and_default_exits_are_valid() {
        let f = flow(
            vec![
                request("a"),
                switch_node("sw1", vec![("c1", "x")]),
                request("b"),
                request("c"),
            ],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge(
                    "e2",
                    "sw1",
                    &handle::case_handle("c1"),
                    "b",
                    handle::TRIGGER,
                ),
                edge("e3", "sw1", handle::DEFAULT, "c", handle::TRIGGER),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn v5_edge_out_of_an_output_node_is_rejected() {
        let f = flow(
            vec![output("out"), request("b")],
            vec![edge("e1", "out", handle::RESULT, "b", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn switch_with_no_cases_is_valid() {
        let f = flow(
            vec![request("a"), switch_node("sw1", vec![]), request("b")],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "sw1", handle::DEFAULT, "b", handle::TRIGGER),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn v6_duplicate_case_ids_are_rejected() {
        let f = flow(
            vec![
                request("a"),
                switch_node("sw1", vec![("c1", "x"), ("c1", "y")]),
            ],
            vec![edge("e1", "a", handle::RESULT, "sw1", handle::INPUT)],
        );
        assert_eq!(invalid_node_id(validate(&f)), "sw1");
    }

    #[test]
    fn v7_duplicate_case_matches_are_rejected() {
        let f = flow(
            vec![
                request("a"),
                switch_node("sw1", vec![("c1", "pro"), ("c2", "pro")]),
            ],
            vec![edge("e1", "a", handle::RESULT, "sw1", handle::INPUT)],
        );
        match validate(&f) {
            Err(FlowGraphError::InvalidNode { node_id, reason }) => {
                assert_eq!(node_id, "sw1");
                assert!(reason.contains("pro"), "got: {reason}");
            }
            other => panic!("expected InvalidNode, got {other:?}"),
        }
    }

    #[test]
    fn v8_whitespace_only_condition_is_rejected() {
        let mut f = valid_if_flow();
        f.nodes[1] = if_node("if1", "   ");
        assert_eq!(invalid_node_id(validate(&f)), "if1");
    }

    #[test]
    fn v8_empty_switch_value_is_rejected() {
        let mut sw = switch_node("sw1", vec![("c1", "x")]);
        if let FlowNodeKind::Switch { value, .. } = &mut sw.kind {
            value.clear();
        }
        let f = flow(
            vec![request("a"), sw],
            vec![edge("e1", "a", handle::RESULT, "sw1", handle::INPUT)],
        );
        assert_eq!(invalid_node_id(validate(&f)), "sw1");
    }

    #[test]
    fn earlier_rule_wins_over_later_rule() {
        // e1 breaks V2 and if1 breaks V8. V2 comes first in rule order.
        let f = flow(
            vec![request("a"), request("b"), if_node("if1", "")],
            vec![
                edge("e1", "a", handle::RESULT, "b", handle::INPUT),
                edge("e2", "a", handle::RESULT, "if1", handle::INPUT),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn earlier_edge_wins_within_the_same_rule() {
        let f = flow(
            vec![request("a"), request("b"), request("c")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "url"),
                edge("e2", "a", "bogus", "b", "body"),
                edge("e3", "a", "bogus", "c", "body"),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn new_error_variants_display_their_id_and_reason() {
        let node_err = FlowGraphError::InvalidNode {
            node_id: "if1".to_string(),
            reason: "needs an input".to_string(),
        };
        assert_eq!(node_err.to_string(), "invalid node if1: needs an input");
        let edge_err = FlowGraphError::InvalidEdge {
            edge_id: "e1".to_string(),
            reason: "unknown exit".to_string(),
        };
        assert_eq!(edge_err.to_string(), "invalid edge e1: unknown exit");
    }

    fn polling_request(id: &str, repeat: crate::node::RepeatUntil) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                debug: false,
                label: id.to_string(),
                source: RequestSource::Saved {
                    request_path: format!("{id}.yml"),
                },
                repeat_until: Some(repeat),
            },
        )
    }

    fn invalid_node_reason(result: Result<Vec<String>, FlowGraphError>) -> String {
        match result {
            Err(FlowGraphError::InvalidNode { reason, .. }) => reason,
            other => panic!("expected InvalidNode, got {other:?}"),
        }
    }

    #[test]
    fn a_valid_repeat_until_passes() {
        let f = flow(
            vec![polling_request("p", crate::node::RepeatUntil::default())],
            vec![],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn repeat_until_blank_condition_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            condition: "  ".to_string(),
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "the repeat-until condition is empty"
        );
    }

    #[test]
    fn repeat_until_short_interval_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            interval_ms: 99,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until interval must be at least 100 ms"
        );
    }

    #[test]
    fn repeat_until_zero_attempts_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            max_attempts: 0,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until max attempts must be between 1 and 1000"
        );
    }

    #[test]
    fn repeat_until_too_many_attempts_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            max_attempts: 1001,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until max attempts must be between 1 and 1000"
        );
    }

    #[test]
    fn repeat_until_long_timeout_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            timeout_ms: 3_600_001,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until timeout must be at most 3600000 ms"
        );
    }

    #[test]
    fn repeat_until_timeout_shorter_than_interval_is_rejected() {
        let repeat = crate::node::RepeatUntil {
            interval_ms: 5000,
            timeout_ms: 1000,
            ..Default::default()
        };
        let f = flow(vec![polling_request("p", repeat.clone())], vec![]);
        assert_eq!(invalid_node_id(validate(&f)), "p");
        let f = flow(vec![polling_request("p", repeat)], vec![]);
        assert_eq!(
            invalid_node_reason(validate(&f)),
            "repeat-until timeout must not be shorter than the interval"
        );
    }

    fn wait_node(id: &str, name: &str, timeout_ms: u64) -> FlowNode {
        node(
            id,
            FlowNodeKind::WaitForCallback {
                label: id.to_string(),
                name: name.to_string(),
                timeout_ms,
                accept_when: None,
            },
        )
    }

    #[test]
    fn a_wait_node_with_a_run_when_wire_is_valid() {
        let f = flow(
            vec![
                request("register"),
                wait_node("w", "payment", 60_000),
                request("use"),
            ],
            vec![
                edge("e1", "register", handle::RESULT, "w", handle::TRIGGER),
                edge("e2", "w", handle::RESULT, "use", "url"),
            ],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    #[test]
    fn a_wait_node_rejects_a_data_input() {
        let f = flow(
            vec![request("a"), wait_node("w", "payment", 60_000)],
            vec![edge("e1", "a", handle::RESULT, "w", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn a_wait_node_has_only_a_result_exit() {
        let f = flow(
            vec![wait_node("w", "payment", 60_000), request("b")],
            vec![edge("e1", "w", handle::TRUE, "b", handle::TRIGGER)],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn a_wait_node_name_must_use_letters_digits_and_underscores() {
        for bad in ["", "pay ment", "pay-ment", "päy", "a.b"] {
            let f = flow(vec![wait_node("w", bad, 60_000)], Vec::new());
            assert_eq!(
                invalid_node_id(validate(&f)),
                "w",
                "name {bad:?} must be rejected"
            );
        }
        let f = flow(vec![wait_node("w", "Payment_2", 60_000)], Vec::new());
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn wait_node_names_must_be_unique() {
        let f = flow(
            vec![
                wait_node("w1", "payment", 60_000),
                wait_node("w2", "payment", 60_000),
            ],
            Vec::new(),
        );
        assert_eq!(invalid_node_id(validate(&f)), "w2");
    }

    #[test]
    fn a_wait_node_timeout_must_be_between_one_second_and_one_hour() {
        for bad in [0, 999, 3_600_001] {
            let f = flow(vec![wait_node("w", "payment", bad)], Vec::new());
            assert_eq!(
                invalid_node_id(validate(&f)),
                "w",
                "timeout {bad} must be rejected"
            );
        }
        for good in [1000, 3_600_000] {
            let f = flow(vec![wait_node("w", "payment", good)], Vec::new());
            assert!(validate(&f).is_ok(), "timeout {good} is allowed");
        }
    }

    #[test]
    fn a_blank_accept_when_is_rejected() {
        let f = flow(
            vec![node(
                "w",
                FlowNodeKind::WaitForCallback {
                    label: "w".to_string(),
                    name: "payment".to_string(),
                    timeout_ms: 60_000,
                    accept_when: Some("   ".to_string()),
                },
            )],
            Vec::new(),
        );
        assert_eq!(invalid_node_id(validate(&f)), "w");
    }
}
