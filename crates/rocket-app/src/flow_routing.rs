//! Decides, for one Flow node at its turn in topological order, whether it
//! runs, is skipped, or fails — from the recorded outcomes of its direct
//! predecessors (spec §6.2–6.3). Pure and synchronous.

use std::collections::HashMap;

use rocket_flow::{handle, FlowEdge};
use rocket_shared::events::FlowSkipReason;

/// What happened to a node earlier in the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeOutcome {
    /// The node ran successfully and left through `chosen_exit`
    /// (`handle::RESULT` for plain nodes).
    Succeeded { chosen_exit: String },
    /// `responded` is true only for a Request that failed because of a
    /// non-2xx status. Its response is captured, so a routing node may
    /// observe it (spec §6.3.1).
    Failed { responded: bool },
    Skipped(FlowSkipReason),
}

/// What a node should do at its turn.
#[derive(Debug, PartialEq)]
pub(crate) enum NodeFate<'a> {
    /// Run it, feeding only these live, non-trigger edges.
    Run { data_edges: Vec<&'a FlowEdge> },
    Skip(FlowSkipReason),
    /// Fail it without executing, with this error message.
    Fail(String),
}

/// An edge is live when its source succeeded and left through the exit the
/// edge is attached to, or when it carries a failed Request's captured
/// response into an If/Switch `input` (spec §6.3.1). `target_is_routing`
/// says whether the edge's target node is an If/Switch.
pub(crate) fn is_live(
    edge: &FlowEdge,
    outcomes: &HashMap<String, NodeOutcome>,
    target_is_routing: bool,
) -> bool {
    match outcomes.get(&edge.source_node_id) {
        Some(NodeOutcome::Succeeded { chosen_exit }) => *chosen_exit == edge.source_handle,
        Some(NodeOutcome::Failed { responded: true }) => {
            target_is_routing && edge.target_field == handle::INPUT
        }
        _ => false,
    }
}

/// Applies spec §6.3 rules 2–5 (rule 1, cancellation, stays in the caller).
/// Several edges into the same `target_field` are alternatives: one live
/// edge is enough. Different fields are all required.
pub(crate) fn decide_fate<'a>(
    incoming: &[&'a FlowEdge],
    outcomes: &HashMap<String, NodeOutcome>,
    target_is_routing: bool,
) -> NodeFate<'a> {
    if incoming.is_empty() {
        return NodeFate::Run {
            data_edges: Vec::new(),
        };
    }

    // Rule 3 only looks at edges that are not live, so an edge made live
    // by failure observation does not poison its routing node.
    let upstream_failed = incoming.iter().any(|e| {
        !is_live(e, outcomes, target_is_routing)
            && matches!(
                outcomes.get(&e.source_node_id),
                Some(NodeOutcome::Failed { .. })
                    | Some(NodeOutcome::Skipped(FlowSkipReason::UpstreamFailed))
            )
    });
    if upstream_failed {
        return NodeFate::Skip(FlowSkipReason::UpstreamFailed);
    }

    // Group by target field, keeping first-seen order so messages are stable.
    let mut groups: Vec<(&'a str, Vec<&'a FlowEdge>)> = Vec::new();
    for &e in incoming {
        match groups.iter_mut().find(|(field, _)| *field == e.target_field) {
            Some((_, edges)) => edges.push(e),
            None => groups.push((e.target_field.as_str(), vec![e])),
        }
    }

    let mut data_edges = Vec::new();
    let mut ambiguous: Option<(&str, usize)> = None;
    for (field, edges) in &groups {
        let live: Vec<&'a FlowEdge> = edges
            .iter()
            .copied()
            .filter(|e| is_live(e, outcomes, target_is_routing))
            .collect();
        if live.is_empty() {
            return NodeFate::Skip(FlowSkipReason::BranchNotTaken);
        }
        // A trigger carries no data, so several live triggers are fine.
        if *field == handle::TRIGGER {
            continue;
        }
        if live.len() > 1 && ambiguous.is_none() {
            ambiguous = Some((*field, live.len()));
        }
        data_edges.extend(live);
    }

    if let Some((field, count)) = ambiguous {
        return NodeFate::Fail(format!("field '{field}' has {count} live inputs"));
    }
    NodeFate::Run { data_edges }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::handle;
    use rocket_shared::events::FlowSkipReason;
    use std::collections::HashMap;

    fn edge(id: &str, from: &str, exit: &str, field: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: "t".to_string(),
            target_field: field.to_string(),
            expression: String::new(),
            source_handle: exit.to_string(),
        }
    }

    fn ok(exit: &str) -> NodeOutcome {
        NodeOutcome::Succeeded {
            chosen_exit: exit.to_string(),
        }
    }

    fn outcomes(entries: &[(&str, NodeOutcome)]) -> HashMap<String, NodeOutcome> {
        entries
            .iter()
            .map(|(id, o)| (id.to_string(), o.clone()))
            .collect()
    }

    fn data_ids(fate: &NodeFate<'_>) -> Vec<String> {
        match fate {
            NodeFate::Run { data_edges } => data_edges.iter().map(|e| e.id.clone()).collect(),
            other => panic!("expected Run, got {other:?}"),
        }
    }

    fn failed(responded: bool) -> NodeOutcome {
        NodeOutcome::Failed { responded }
    }

    #[test]
    fn edge_is_live_only_when_source_succeeded_on_the_same_exit() {
        let o = outcomes(&[
            ("plain", ok(handle::RESULT)),
            ("if", ok(handle::TRUE)),
            ("bad", failed(false)),
        ]);
        assert!(is_live(&edge("e1", "plain", handle::RESULT, "url"), &o, false));
        assert!(is_live(&edge("e2", "if", handle::TRUE, "url"), &o, false));
        assert!(!is_live(&edge("e3", "if", handle::FALSE, "url"), &o, false));
        assert!(!is_live(&edge("e4", "bad", handle::RESULT, "url"), &o, false));
        assert!(!is_live(&edge("e5", "unknown", handle::RESULT, "url"), &o, false));
    }

    #[test]
    fn a_node_without_incoming_edges_runs() {
        let fate = decide_fate(&[], &HashMap::new(), false);
        assert_eq!(fate, NodeFate::Run { data_edges: Vec::new() });
    }

    #[test]
    fn a_failed_source_skips_as_upstream_failed() {
        let e = edge("e1", "a", handle::RESULT, "url");
        for responded in [false, true] {
            let o = outcomes(&[("a", failed(responded))]);
            assert_eq!(
                decide_fate(&[&e], &o, false),
                NodeFate::Skip(FlowSkipReason::UpstreamFailed),
                "responded = {responded}"
            );
        }
    }

    #[test]
    fn an_upstream_failed_skip_propagates_as_upstream_failed() {
        let e = edge("e1", "a", handle::RESULT, "url");
        let o = outcomes(&[("a", NodeOutcome::Skipped(FlowSkipReason::UpstreamFailed))]);
        assert_eq!(
            decide_fate(&[&e], &o, false),
            NodeFate::Skip(FlowSkipReason::UpstreamFailed)
        );
    }

    #[test]
    fn failure_wins_over_not_taken_even_through_a_join() {
        // Spec §6.3 rule 3 runs before rule 4: one arm failed, the other was
        // not taken — the join is upstream_failed, not branch_not_taken.
        let failed_arm = edge("e1", "a", handle::RESULT, "body");
        let not_taken_arm = edge("e2", "b", handle::RESULT, "body");
        let o = outcomes(&[
            ("a", failed(false)),
            ("b", NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken)),
        ]);
        assert_eq!(
            decide_fate(&[&failed_arm, &not_taken_arm], &o, false),
            NodeFate::Skip(FlowSkipReason::UpstreamFailed)
        );
    }

    #[test]
    fn a_responded_failure_is_live_into_a_routing_input() {
        // Spec §6.3.1: a 401 Request feeds an If/Switch `input`.
        let e = edge("e1", "login", handle::RESULT, handle::INPUT);
        let o = outcomes(&[("login", failed(true))]);
        assert!(is_live(&e, &o, true));
        assert_eq!(data_ids(&decide_fate(&[&e], &o, true)), vec!["e1".to_string()]);
    }

    #[test]
    fn a_responded_failure_is_not_live_into_a_request_field_or_an_output() {
        // Only a routing node's `input` observes failures. A Request field,
        // an Output `value`, a trigger, or an `input`-named edge into a
        // non-routing node all stay upstream_failed.
        let o = outcomes(&[("login", failed(true))]);
        for field in ["url", "value", handle::TRIGGER, handle::INPUT] {
            let e = edge("e1", "login", handle::RESULT, field);
            assert!(!is_live(&e, &o, false), "field {field}");
            assert_eq!(
                decide_fate(&[&e], &o, false),
                NodeFate::Skip(FlowSkipReason::UpstreamFailed),
                "field {field}"
            );
        }
    }

    #[test]
    fn a_failure_without_a_response_still_skips_a_routing_node() {
        // A transport error captured nothing, so there is nothing to observe.
        let e = edge("e1", "login", handle::RESULT, handle::INPUT);
        let o = outcomes(&[("login", failed(false))]);
        assert!(!is_live(&e, &o, true));
        assert_eq!(
            decide_fate(&[&e], &o, true),
            NodeFate::Skip(FlowSkipReason::UpstreamFailed)
        );
    }

    #[test]
    fn a_not_taken_exit_skips_as_branch_not_taken() {
        let e = edge("e1", "if", handle::TRUE, "trigger");
        let o = outcomes(&[("if", ok(handle::FALSE))]);
        assert_eq!(
            decide_fate(&[&e], &o, false),
            NodeFate::Skip(FlowSkipReason::BranchNotTaken)
        );
    }

    #[test]
    fn not_taken_propagates_transitively_as_branch_not_taken() {
        let e = edge("e1", "mid", handle::RESULT, "url");
        let o = outcomes(&[("mid", NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken))]);
        assert_eq!(
            decide_fate(&[&e], &o, false),
            NodeFate::Skip(FlowSkipReason::BranchNotTaken)
        );
    }

    #[test]
    fn merge_into_one_field_runs_with_only_the_live_edge() {
        // Case 1: both arms feed `body`; only the false arm ran.
        let from_true_arm = edge("e1", "profile", handle::RESULT, "body");
        let from_false_arm = edge("e2", "refresh", handle::RESULT, "body");
        let o = outcomes(&[
            ("profile", NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken)),
            ("refresh", ok(handle::RESULT)),
        ]);
        let fate = decide_fate(&[&from_true_arm, &from_false_arm], &o, false);
        assert_eq!(data_ids(&fate), vec!["e2".to_string()]);
    }

    #[test]
    fn a_different_field_without_a_live_edge_skips_the_node() {
        // Case 2: url is live, the token header only comes from a not-taken arm.
        let url = edge("e1", "config", handle::RESULT, "url");
        let token = edge("e2", "get_token", handle::RESULT, "headers[Authorization].value");
        let o = outcomes(&[
            ("config", ok(handle::RESULT)),
            ("get_token", NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken)),
        ]);
        assert_eq!(
            decide_fate(&[&url, &token], &o, false),
            NodeFate::Skip(FlowSkipReason::BranchNotTaken)
        );
    }

    #[test]
    fn two_live_edges_into_one_data_field_fail_the_node() {
        // Applies to plain wires too — the intentional Phase 1 change in spec §5.5.
        let a = edge("e1", "a", handle::RESULT, "body");
        let b = edge("e2", "b", handle::RESULT, "body");
        let o = outcomes(&[("a", ok(handle::RESULT)), ("b", ok(handle::RESULT))]);
        assert_eq!(
            decide_fate(&[&a, &b], &o, false),
            NodeFate::Fail("field 'body' has 2 live inputs".to_string())
        );
    }

    #[test]
    fn several_live_triggers_run_and_are_not_data_edges() {
        let t1 = edge("e1", "a", handle::RESULT, handle::TRIGGER);
        let t2 = edge("e2", "b", handle::RESULT, handle::TRIGGER);
        let url = edge("e3", "c", handle::RESULT, "url");
        let o = outcomes(&[
            ("a", ok(handle::RESULT)),
            ("b", ok(handle::RESULT)),
            ("c", ok(handle::RESULT)),
        ]);
        let fate = decide_fate(&[&t1, &t2, &url], &o, false);
        assert_eq!(data_ids(&fate), vec!["e3".to_string()]);
    }
}
