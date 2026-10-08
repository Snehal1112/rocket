//! Plans a partial Flow run ("Run this node" or "Run from here") on top of
//! an earlier run. Pure and synchronous: `flow_run_cache` keeps the earlier
//! run's results and `FlowExecutionService::run_partial` executes the plan.
//!
//! Decisions D1 and D5 (plan index): a run whose inputs the earlier run
//! cannot serve is refused, never completed by re-running ancestors, and an
//! upstream edit since the earlier run refuses it too.

use std::collections::{HashMap, HashSet};

use rocket_collection::Request;
use rocket_flow::{handle, Flow, FlowEdge, FlowNode, FlowNodeKind, RequestSource};
use rocket_shared::error::DomainError;
use rocket_shared::events::{FlowPartialMode, FlowSkipReason};

use crate::flow_routing::{decide_fate, is_live, NodeFate, NodeOutcome};

/// A request to re-run part of a flow on top of an earlier run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialRun {
    /// The run whose cached results feed this one.
    pub base_run_id: String,
    pub start_node_id: String,
    pub mode: FlowPartialMode,
}

/// What the earlier run left for one node, as the planner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SeedView {
    pub(crate) outcome: NodeOutcome,
    /// False when the node captured nothing, or its output was too large to keep.
    pub(crate) has_output: bool,
    /// True when a later partial run changed a node upstream of this one.
    pub(crate) stale: bool,
}

/// The nodes a partial run executes and the cached nodes that feed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartialPlan {
    /// Nodes to execute, in the flow's topological order.
    pub(crate) run_order: Vec<String>,
    /// Nodes outside `run_order` whose cached results feed it, in topological order.
    pub(crate) seeds: Vec<String>,
    /// Trigger edges into the start node. The run ignores them.
    pub(crate) dropped_edges: HashSet<String>,
}

/// Why a partial run cannot start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartialRefusal {
    pub(crate) message: String,
    pub(crate) node_ids: Vec<String>,
    pub(crate) edge_ids: Vec<String>,
}

impl From<PartialRefusal> for DomainError {
    // The message ends with the ids in the shape of a save error, so the
    // canvas (`parseGraphErrorMessage`) highlights them.
    fn from(refusal: PartialRefusal) -> Self {
        DomainError::InvalidInput(format!(
            "{} — node(s): {}; edge(s): {}",
            refusal.message,
            refusal.node_ids.join(", "),
            refusal.edge_ids.join(", ")
        ))
    }
}

/// Input and Auth nodes always run again. They are cheap and side-effect
/// free, and a cached Auth output would be an old token.
pub(crate) fn is_free_node(kind: &FlowNodeKind) -> bool {
    matches!(kind, FlowNodeKind::Input { .. } | FlowNodeKind::Auth { .. })
}

fn is_routing(kind: &FlowNodeKind) -> bool {
    matches!(kind, FlowNodeKind::If { .. } | FlowNodeKind::Switch { .. })
}

/// The node's label, or its id when the label is blank.
pub(crate) fn node_label(node: &FlowNode) -> &str {
    let label = match &node.kind {
        FlowNodeKind::Request { label, .. }
        | FlowNodeKind::Input { label, .. }
        | FlowNodeKind::Output { label, .. }
        | FlowNodeKind::If { label, .. }
        | FlowNodeKind::Switch { label, .. }
        | FlowNodeKind::WaitForCallback { label, .. }
        | FlowNodeKind::Transform { label, .. }
        | FlowNodeKind::Auth { label, .. } => label,
    };
    if label.trim().is_empty() {
        &node.id
    } else {
        label
    }
}

/// The label of node `id` in `flow`, or the id itself when it is not there.
pub(crate) fn label_in(flow: &Flow, id: &str) -> String {
    flow.nodes
        .iter()
        .find(|n| n.id == id)
        .map_or_else(|| id.to_string(), |n| node_label(n).to_string())
}

fn refuse(message: String, node_ids: Vec<String>, edge_ids: Vec<String>) -> PartialRefusal {
    PartialRefusal {
        message,
        node_ids,
        edge_ids,
    }
}

/// Picks the nodes to run and the seeds that feed them. Refuses an unknown
/// start node, "Run this node" on a Wait, and a Wait whose callback sender
/// would not run. `callback_senders` comes from `callback_senders`.
pub(crate) fn select_nodes(
    flow: &Flow,
    order: &[String],
    partial: &PartialRun,
    callback_senders: &HashMap<String, Vec<String>>,
) -> Result<PartialPlan, PartialRefusal> {
    let nodes: HashMap<&str, &FlowNode> = flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let start_id = partial.start_node_id.as_str();
    let Some(start) = nodes.get(start_id).copied() else {
        return Err(refuse(
            format!("node '{start_id}' is not in the saved flow. Save the flow and try again"),
            vec![start_id.to_string()],
            Vec::new(),
        ));
    };
    if partial.mode == FlowPartialMode::Node
        && matches!(start.kind, FlowNodeKind::WaitForCallback { .. })
    {
        return Err(refuse(
            format!(
                "'{}' waits for a callback whose URL is new on every run, so it cannot run on its own. Use Run from here on the request that sends the callback",
                node_label(start)
            ),
            vec![start_id.to_string()],
            Vec::new(),
        ));
    }

    let mut core: HashSet<String> = HashSet::from([start_id.to_string()]);
    if partial.mode == FlowPartialMode::FromHere {
        core.extend(rocket_flow::graph::reachable_from(flow, start_id));
    }
    // The user asked for the start node, so its "Run when" gates do not apply.
    let dropped_edges: HashSet<String> = flow
        .edges
        .iter()
        .filter(|e| e.target_node_id == start_id && e.target_field == handle::TRIGGER)
        .map(|e| e.id.clone())
        .collect();

    // Input and Auth nodes that feed the run join it.
    let mut run_set = core.clone();
    for e in &flow.edges {
        let feeds_core = core.contains(&e.target_node_id)
            && !core.contains(&e.source_node_id)
            && !dropped_edges.contains(&e.id);
        if feeds_core
            && nodes
                .get(e.source_node_id.as_str())
                .is_some_and(|n| is_free_node(&n.kind))
        {
            run_set.insert(e.source_node_id.clone());
        }
    }

    let seed_set: HashSet<&str> = flow
        .edges
        .iter()
        .filter(|e| {
            run_set.contains(&e.target_node_id)
                && !run_set.contains(&e.source_node_id)
                && !dropped_edges.contains(&e.id)
        })
        .map(|e| e.source_node_id.as_str())
        .collect();
    let run_order: Vec<String> = order
        .iter()
        .filter(|id| run_set.contains(*id))
        .cloned()
        .collect();
    let seeds: Vec<String> = order
        .iter()
        .filter(|id| seed_set.contains(id.as_str()))
        .cloned()
        .collect();

    // A Wait's callback URL is new on every run. A sender outside the run
    // would never send the new URL, so the Wait would only time out.
    for wait_id in &run_order {
        let Some(senders) = callback_senders.get(wait_id) else {
            continue;
        };
        if let Some(outside) = senders.iter().find(|s| !run_set.contains(*s)) {
            let wait_label = label_in(flow, wait_id);
            let sender_label = label_in(flow, outside);
            return Err(refuse(
                format!(
                    "'{wait_label}' waits for a callback that '{sender_label}' sends, but '{sender_label}' is not part of this run, so it would never send the new callback URL. Use Run from here on '{sender_label}'"
                ),
                vec![wait_id.clone(), outside.clone()],
                Vec::new(),
            ));
        }
    }

    Ok(PartialPlan {
        run_order,
        seeds,
        dropped_edges,
    })
}

/// Checks that the earlier run can feed `plan`: every seed ran and is not
/// stale, the start node would run with its cached inputs, and every live
/// data edge from a seed has a cached output. Call it after the fingerprint
/// check, so an edited upstream node is reported as an edit first.
pub(crate) fn check_seeds(
    flow: &Flow,
    plan: &PartialPlan,
    start_node_id: &str,
    base: &HashMap<String, SeedView>,
) -> Result<(), PartialRefusal> {
    let nodes: HashMap<&str, &FlowNode> = flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let run_set: HashSet<&str> = plan.run_order.iter().map(String::as_str).collect();

    let mut outcomes: HashMap<String, NodeOutcome> = HashMap::new();
    for seed in &plan.seeds {
        let label = label_in(flow, seed);
        match base.get(seed) {
            None => {
                return Err(refuse(
                    format!("'{label}' did not run in the earlier run. Run the full flow first"),
                    vec![seed.clone()],
                    Vec::new(),
                ))
            }
            Some(view) if view.stale => {
                return Err(refuse(
                    format!(
                        "the cached result of '{label}' is out of date after an earlier partial run. Run the full flow or Run from '{label}'"
                    ),
                    vec![seed.clone()],
                    Vec::new(),
                ))
            }
            Some(view) => {
                outcomes.insert(seed.clone(), view.outcome.clone());
            }
        }
    }
    // Input and Auth nodes run again and always succeed.
    for id in &plan.run_order {
        if nodes.get(id.as_str()).is_some_and(|n| is_free_node(&n.kind)) {
            outcomes.insert(
                id.clone(),
                NodeOutcome::Succeeded {
                    chosen_exit: handle::RESULT.to_string(),
                },
            );
        }
    }

    let start_incoming: Vec<&FlowEdge> = flow
        .edges
        .iter()
        .filter(|e| e.target_node_id == start_node_id && !plan.dropped_edges.contains(&e.id))
        .collect();
    let start_is_routing = nodes
        .get(start_node_id)
        .is_some_and(|n| is_routing(&n.kind));
    match decide_fate(&start_incoming, &outcomes, start_is_routing) {
        NodeFate::Run { data_edges } => {
            for e in data_edges {
                require_output(flow, e, base, &run_set)?;
            }
        }
        NodeFate::Skip(_) => {
            let blocked: Vec<&FlowEdge> = start_incoming
                .iter()
                .copied()
                .filter(|e| !is_live(e, &outcomes, start_is_routing))
                .collect();
            let (source, why) = match blocked.first() {
                Some(e) => (
                    e.source_node_id.clone(),
                    describe_outcome(outcomes.get(&e.source_node_id), &e.source_handle),
                ),
                None => (start_node_id.to_string(), "not run".to_string()),
            };
            let label = label_in(flow, &source);
            let mut node_ids = vec![start_node_id.to_string()];
            for e in &blocked {
                if !node_ids.contains(&e.source_node_id) {
                    node_ids.push(e.source_node_id.clone());
                }
            }
            return Err(refuse(
                format!(
                    "input from '{label}' has no cached value ({why}). Run the full flow or Run from '{label}'"
                ),
                node_ids,
                blocked.iter().map(|e| e.id.clone()).collect(),
            ));
        }
        NodeFate::Fail(message) => {
            return Err(refuse(
                format!("'{}' cannot run: {message}", label_in(flow, start_node_id)),
                vec![start_node_id.to_string()],
                Vec::new(),
            ))
        }
    }

    // Other nodes read seeds through the normal routing rules. A seed on a
    // not-taken branch simply skips them, as in a full run. A live data edge
    // from a seed needs that seed's cached output.
    for e in &flow.edges {
        let from_seed = run_set.contains(e.target_node_id.as_str())
            && !run_set.contains(e.source_node_id.as_str())
            && e.target_node_id != start_node_id
            && e.target_field != handle::TRIGGER;
        if !from_seed {
            continue;
        }
        let routing = nodes
            .get(e.target_node_id.as_str())
            .is_some_and(|n| is_routing(&n.kind));
        if is_live(e, &outcomes, routing) {
            require_output(flow, e, base, &run_set)?;
        }
    }
    Ok(())
}

/// A data edge from a seed needs that seed's cached output.
fn require_output(
    flow: &Flow,
    edge: &FlowEdge,
    base: &HashMap<String, SeedView>,
    run_set: &HashSet<&str>,
) -> Result<(), PartialRefusal> {
    if run_set.contains(edge.source_node_id.as_str()) {
        return Ok(());
    }
    if base.get(&edge.source_node_id).is_some_and(|v| v.has_output) {
        return Ok(());
    }
    let label = label_in(flow, &edge.source_node_id);
    Err(refuse(
        format!(
            "the output of '{label}' was too large to keep, so it cannot feed this run. Run the full flow or Run from '{label}'"
        ),
        vec![edge.source_node_id.clone()],
        vec![edge.id.clone()],
    ))
}

fn describe_outcome(outcome: Option<&NodeOutcome>, edge_exit: &str) -> String {
    match outcome {
        Some(NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken)) => {
            "skipped: branch_not_taken".to_string()
        }
        Some(NodeOutcome::Skipped(FlowSkipReason::UpstreamFailed)) => {
            "skipped: upstream_failed".to_string()
        }
        Some(NodeOutcome::Failed { responded: true }) => {
            "failed with a non-2xx response".to_string()
        }
        Some(NodeOutcome::Failed { responded: false }) => "failed".to_string(),
        Some(NodeOutcome::Succeeded { chosen_exit }) => {
            format!("took the '{chosen_exit}' exit, not '{edge_exit}'")
        }
        None => "not run".to_string(),
    }
}

/// Maps every Wait for callback node to the Request nodes whose text holds
/// `{{callback.<its name>}}`. Inline requests are read from the flow; saved
/// ones from `saved` (request path to request). Values fed in by wires and
/// URLs built in scripts without the literal text are not seen.
pub(crate) fn callback_senders(
    flow: &Flow,
    saved: &HashMap<String, Request>,
) -> HashMap<String, Vec<String>> {
    let texts: Vec<(&str, Vec<String>)> = flow
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            FlowNodeKind::Request { source, .. } => {
                Some((n.id.as_str(), request_texts(source, saved)))
            }
            _ => None,
        })
        .collect();
    let mut senders = HashMap::new();
    for wait in &flow.nodes {
        let FlowNodeKind::WaitForCallback { name, .. } = &wait.kind else {
            continue;
        };
        let mut ids: Vec<String> = texts
            .iter()
            .filter(|(_, t)| {
                t.iter()
                    .any(|text| rocket_flow::validate::mentions_callback(text, name))
            })
            .map(|(id, _)| (*id).to_string())
            .collect();
        ids.sort();
        if !ids.is_empty() {
            senders.insert(wait.id.clone(), ids);
        }
    }
    senders
}

fn request_texts(source: &RequestSource, saved: &HashMap<String, Request>) -> Vec<String> {
    match source {
        RequestSource::Inline { request } => {
            let mut texts = vec![request.url.clone()];
            for header in &request.headers {
                texts.push(header.name.clone());
                texts.push(header.value.clone());
            }
            texts.extend(request.body.clone());
            texts
        }
        RequestSource::Saved { request_path } => {
            // The whole request as text, so a mention in form data, path
            // params, variables, auth, scripts or tests counts too.
            saved
                .get(request_path)
                .map(|request| vec![crate::flow_run_cache::saved_request_text(request)])
                .unwrap_or_default()
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::{InlineRequestData, NodePosition};
    use rocket_shared::types::HttpMethod;
    use rocket_shared::VariableValue;

    fn node(id: &str, kind: FlowNodeKind) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn request_to(id: &str, url: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                label: id.to_string(),
                debug: false,
                repeat_until: None,
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "get".to_string(),
                        url: url.to_string(),
                        headers: Vec::new(),
                        body: None,
                    },
                },
            },
        )
    }

    fn request(id: &str) -> FlowNode {
        request_to(id, &format!("https://api.example.com/{id}"))
    }

    fn input(id: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Input {
                label: id.to_string(),
                value: VariableValue::simple("v"),
            },
        )
    }

    fn if_node(id: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::If {
                label: id.to_string(),
                condition: "true".to_string(),
            },
        )
    }

    fn wait(id: &str, name: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::WaitForCallback {
                label: id.to_string(),
                name: name.to_string(),
                timeout_ms: 60_000,
                accept_when: None,
            },
        )
    }

    fn edge(id: &str, from: &str, exit: &str, to: &str, field: &str) -> FlowEdge {
        let carries_data = field != handle::TRIGGER && field != handle::INPUT;
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: to.to_string(),
            target_field: field.to_string(),
            expression: if carries_data {
                "response.body".to_string()
            } else {
                String::new()
            },
            source_handle: exit.to_string(),
        }
    }

    fn flow(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> (Flow, Vec<String>) {
        let flow = Flow {
            name: "f".to_string(),
            nodes,
            edges,
            callback_host: None,
        };
        let order = rocket_flow::validate(&flow).expect("test flow must be valid");
        (flow, order)
    }

    fn seen(outcome: NodeOutcome) -> SeedView {
        SeedView {
            outcome,
            has_output: true,
            stale: false,
        }
    }

    fn ok(exit: &str) -> SeedView {
        seen(NodeOutcome::Succeeded {
            chosen_exit: exit.to_string(),
        })
    }

    fn base(entries: Vec<(&str, SeedView)>) -> HashMap<String, SeedView> {
        entries
            .into_iter()
            .map(|(id, view)| (id.to_string(), view))
            .collect()
    }

    fn partial(start: &str, mode: FlowPartialMode) -> PartialRun {
        PartialRun {
            base_run_id: "base".to_string(),
            start_node_id: start.to_string(),
            mode,
        }
    }

    /// Runs both planner stages, as `prepare_partial` does.
    fn plan(
        flow: &Flow,
        order: &[String],
        run: &PartialRun,
        seeds: &HashMap<String, SeedView>,
    ) -> Result<PartialPlan, PartialRefusal> {
        let plan = select_nodes(flow, order, run, &HashMap::new())?;
        check_seeds(flow, &plan, &run.start_node_id, seeds)?;
        Ok(plan)
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// a -url-> b -url-> c.
    fn chain() -> (Flow, Vec<String>) {
        flow(
            vec![request("a"), request("b"), request("c")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "url"),
                edge("e2", "b", handle::RESULT, "c", "url"),
            ],
        )
    }

    #[test]
    fn run_this_node_runs_only_the_start_node_fed_by_its_cached_source() {
        let (flow, order) = chain();
        let plan = plan(
            &flow,
            &order,
            &partial("b", FlowPartialMode::Node),
            &base(vec![("a", ok(handle::RESULT))]),
        )
        .expect("plan");
        assert_eq!(plan.run_order, ids(&["b"]));
        assert_eq!(plan.seeds, ids(&["a"]));
    }

    #[test]
    fn run_from_here_runs_the_start_node_and_every_descendant_in_order() {
        let (flow, order) = chain();
        let plan = plan(
            &flow,
            &order,
            &partial("b", FlowPartialMode::FromHere),
            &base(vec![("a", ok(handle::RESULT))]),
        )
        .expect("plan");
        assert_eq!(plan.run_order, ids(&["b", "c"]));
        assert_eq!(plan.seeds, ids(&["a"]));
    }

    #[test]
    fn seeds_include_a_join_sibling_outside_the_run() {
        // a -url-> c and b -body-> c. Run from a, so b feeds c from the cache.
        let (flow, order) = flow(
            vec![request("a"), request("b"), request("c")],
            vec![
                edge("e1", "a", handle::RESULT, "c", "url"),
                edge("e2", "b", handle::RESULT, "c", "body"),
            ],
        );
        let plan = plan(
            &flow,
            &order,
            &partial("a", FlowPartialMode::FromHere),
            &base(vec![("b", ok(handle::RESULT))]),
        )
        .expect("plan");
        assert_eq!(plan.run_order, ids(&["a", "c"]));
        assert_eq!(plan.seeds, ids(&["b"]));
    }

    #[test]
    fn input_nodes_feeding_the_run_run_again_instead_of_being_seeds() {
        // i (Input) -url-> b and a -body-> b.
        let (flow, order) = flow(
            vec![input("i"), request("a"), request("b")],
            vec![
                edge("e1", "i", handle::RESULT, "b", "url"),
                edge("e2", "a", handle::RESULT, "b", "body"),
            ],
        );
        let plan = plan(
            &flow,
            &order,
            &partial("b", FlowPartialMode::Node),
            &base(vec![("a", ok(handle::RESULT))]),
        )
        .expect("plan");
        assert_eq!(plan.run_order, ids(&["i", "b"]));
        assert_eq!(plan.seeds, ids(&["a"]));
    }

    /// a -input-> chk (If); chk true -trigger-> y; y -url-> z.
    fn branch() -> (Flow, Vec<String>) {
        flow(
            vec![request("a"), if_node("chk"), request("y"), request("z")],
            vec![
                edge("e1", "a", handle::RESULT, "chk", handle::INPUT),
                edge("e2", "chk", handle::TRUE, "y", handle::TRIGGER),
                edge("e3", "y", handle::RESULT, "z", "url"),
            ],
        )
    }

    #[test]
    fn a_start_node_fed_by_a_not_taken_branch_is_refused() {
        let (flow, order) = branch();
        let err = plan(
            &flow,
            &order,
            &partial("z", FlowPartialMode::Node),
            &base(vec![(
                "y",
                seen(NodeOutcome::Skipped(FlowSkipReason::BranchNotTaken)),
            )]),
        )
        .expect_err("y produced nothing");
        assert!(
            err.message
                .contains("input from 'y' has no cached value (skipped: branch_not_taken)"),
            "{}",
            err.message
        );
        assert!(err.message.contains("Run from 'y'"), "{}", err.message);
        assert_eq!(err.node_ids, ids(&["z", "y"]));
        assert_eq!(err.edge_ids, ids(&["e3"]));
        let text = DomainError::from(err).to_string();
        assert!(text.ends_with("node(s): z, y; edge(s): e3"), "{text}");
    }

    #[test]
    fn trigger_edges_into_the_start_node_are_dropped() {
        // y only had a trigger from the not-taken true exit. It runs anyway.
        let (flow, order) = branch();
        let plan = plan(
            &flow,
            &order,
            &partial("y", FlowPartialMode::Node),
            &HashMap::new(),
        )
        .expect("plan");
        assert_eq!(plan.run_order, ids(&["y"]));
        assert!(plan.seeds.is_empty());
        assert!(plan.dropped_edges.contains("e2"));
    }

    #[test]
    fn a_start_node_fed_by_a_failed_request_is_refused() {
        let (flow, order) = chain();
        let err = plan(
            &flow,
            &order,
            &partial("b", FlowPartialMode::Node),
            &base(vec![(
                "a",
                SeedView {
                    outcome: NodeOutcome::Failed { responded: false },
                    has_output: false,
                    stale: false,
                },
            )]),
        )
        .expect_err("a failed");
        assert!(err.message.contains("(failed)"), "{}", err.message);
    }

    #[test]
    fn a_routing_start_node_may_observe_a_non_2xx_seed() {
        let (flow, order) = branch();
        let plan = plan(
            &flow,
            &order,
            &partial("chk", FlowPartialMode::Node),
            &base(vec![("a", seen(NodeOutcome::Failed { responded: true }))]),
        )
        .expect("an If may read a failed response");
        assert_eq!(plan.run_order, ids(&["chk"]));
    }

    #[test]
    fn a_seed_whose_output_was_not_kept_is_refused() {
        let (flow, order) = chain();
        let err = plan(
            &flow,
            &order,
            &partial("b", FlowPartialMode::Node),
            &base(vec![(
                "a",
                SeedView {
                    has_output: false,
                    ..ok(handle::RESULT)
                },
            )]),
        )
        .expect_err("no output to read");
        assert!(err.message.contains("too large to keep"), "{}", err.message);
        assert_eq!(err.node_ids, ids(&["a"]));
    }

    #[test]
    fn a_later_node_reading_a_seed_without_output_is_refused() {
        let (flow, order) = flow(
            vec![request("a"), request("b"), request("c")],
            vec![
                edge("e1", "a", handle::RESULT, "c", "url"),
                edge("e2", "b", handle::RESULT, "c", "body"),
            ],
        );
        let err = plan(
            &flow,
            &order,
            &partial("a", FlowPartialMode::FromHere),
            &base(vec![(
                "b",
                SeedView {
                    has_output: false,
                    ..ok(handle::RESULT)
                },
            )]),
        )
        .expect_err("c reads b");
        assert_eq!(err.node_ids, ids(&["b"]));
        assert_eq!(err.edge_ids, ids(&["e2"]));
    }

    #[test]
    fn a_stale_or_missing_seed_is_refused() {
        let (flow, order) = chain();
        let stale = plan(
            &flow,
            &order,
            &partial("b", FlowPartialMode::Node),
            &base(vec![(
                "a",
                SeedView {
                    stale: true,
                    ..ok(handle::RESULT)
                },
            )]),
        )
        .expect_err("stale");
        assert!(stale.message.contains("is out of date"), "{}", stale.message);
        let missing = plan(
            &flow,
            &order,
            &partial("b", FlowPartialMode::Node),
            &HashMap::new(),
        )
        .expect_err("missing");
        assert!(
            missing.message.contains("did not run in the earlier run"),
            "{}",
            missing.message
        );
    }

    #[test]
    fn an_unknown_start_node_is_refused() {
        let (flow, order) = chain();
        let err = select_nodes(
            &flow,
            &order,
            &partial("ghost", FlowPartialMode::Node),
            &HashMap::new(),
        )
        .expect_err("unknown");
        assert!(err.message.contains("is not in the saved flow"), "{}", err.message);
    }

    /// r (sends {{callback.pay}}) -trigger-> w.
    fn callback_flow() -> (Flow, Vec<String>) {
        flow(
            vec![
                request_to("r", "https://api.example.com/pay?cb={{callback.pay}}"),
                wait("w", "pay"),
            ],
            vec![edge("e1", "r", handle::RESULT, "w", handle::TRIGGER)],
        )
    }

    #[test]
    fn run_this_node_on_a_wait_is_refused() {
        let (flow, order) = callback_flow();
        let err = select_nodes(
            &flow,
            &order,
            &partial("w", FlowPartialMode::Node),
            &HashMap::new(),
        )
        .expect_err("a Wait cannot run alone");
        assert!(err.message.contains("cannot run on its own"), "{}", err.message);
    }

    #[test]
    fn a_wait_whose_sender_is_outside_the_run_is_refused() {
        let (flow, order) = callback_flow();
        let senders = callback_senders(&flow, &HashMap::new());
        let err = select_nodes(
            &flow,
            &order,
            &partial("w", FlowPartialMode::FromHere),
            &senders,
        )
        .expect_err("r would never get the new URL");
        assert!(err.message.contains("is not part of this run"), "{}", err.message);
        assert_eq!(err.node_ids, ids(&["w", "r"]));
    }

    #[test]
    fn a_wait_with_its_sender_inside_the_run_is_allowed() {
        let (flow, order) = callback_flow();
        let senders = callback_senders(&flow, &HashMap::new());
        let plan = select_nodes(
            &flow,
            &order,
            &partial("r", FlowPartialMode::FromHere),
            &senders,
        )
        .expect("plan");
        assert_eq!(plan.run_order, ids(&["r", "w"]));
    }

    #[test]
    fn a_saved_request_that_sends_the_callback_counts_as_a_sender() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: vec![
                node(
                    "s",
                    FlowNodeKind::Request {
                        label: "s".to_string(),
                        debug: false,
                        repeat_until: None,
                        source: RequestSource::Saved {
                            request_path: "pay.yml".to_string(),
                        },
                    },
                ),
                request("plain"),
                wait("w", "pay"),
            ],
            edges: Vec::new(),
            callback_host: None,
        };
        let mut saved_request =
            Request::new("Pay", HttpMethod::Post, "https://api.example.com/pay");
        saved_request.pre_request_script = Some("// {{callback.pay}}".to_string());
        let saved = HashMap::from([("pay.yml".to_string(), saved_request)]);
        let senders = callback_senders(&flow, &saved);
        assert_eq!(senders.get("w"), Some(&ids(&["s"])));
    }

    #[test]
    fn a_saved_request_sending_the_callback_in_form_data_is_refused() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: vec![
                node(
                    "s",
                    FlowNodeKind::Request {
                        label: "s".to_string(),
                        debug: false,
                        repeat_until: None,
                        source: RequestSource::Saved {
                            request_path: "pay.yml".to_string(),
                        },
                    },
                ),
                wait("w", "pay"),
            ],
            edges: vec![edge("e1", "s", handle::RESULT, "w", handle::TRIGGER)],
            callback_host: None,
        };
        let order = vec!["s".to_string(), "w".to_string()];
        let mut saved_request =
            Request::new("Pay", HttpMethod::Post, "https://api.example.com/pay");
        saved_request.body = Some(rocket_shared::types::Body {
            mode: rocket_shared::types::BodyMode::FormUrlEncoded,
            content: None,
            form_data: Some(vec![rocket_shared::types::FormDataEntry {
                key: "notify".to_string(),
                value: "{{callback.pay}}".to_string(),
                entry_type: rocket_shared::types::FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        });
        let saved = HashMap::from([("pay.yml".to_string(), saved_request)]);
        let senders = callback_senders(&flow, &saved);
        assert_eq!(senders.get("w"), Some(&ids(&["s"])));
        let err = select_nodes(&flow, &order, &partial("w", FlowPartialMode::FromHere), &senders)
            .expect_err("the sender is upstream of the run");
        assert!(err.message.contains("is not part of this run"), "{}", err.message);
    }
}
