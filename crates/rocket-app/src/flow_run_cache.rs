//! In-memory results of recent Flow runs, so a partial run can reuse them
//! (`flow_partial`). Outputs are raw and unmasked: nothing here is
//! persisted, sent over IPC or printed. `CachedRun`'s `Debug` shows counts.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rocket_collection::Request;
use rocket_flow::{Flow, FlowEdge, FlowNode, FlowNodeKind, RequestSource};
use rocket_shared::VariableValue;

use crate::flow_execution_service::{CapturedOutput, RunFlowInput};
use crate::flow_partial::{is_free_node, label_in, PartialRefusal, SeedView};
use crate::flow_routing::NodeOutcome;

/// How many runs the cache keeps.
pub(crate) const MAX_CACHED_RUNS: usize = 8;
/// Upper bound on the output bytes the cache holds across all runs.
pub(crate) const CACHE_BYTE_BUDGET: usize = 64 * 1024 * 1024;
/// A larger output is not kept, and a partial run that needs it is refused.
pub(crate) const MAX_CACHED_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

/// One node's result in a cached run.
#[derive(Clone)]
pub(crate) struct CachedNode {
    pub(crate) outcome: NodeOutcome,
    /// The raw output. `None` for Input and Auth nodes, oversized outputs and
    /// nodes that captured nothing.
    pub(crate) output: Option<Arc<CapturedOutput>>,
    pub(crate) fingerprint: u64,
    /// True when a later partial run changed something upstream of this node.
    pub(crate) stale: bool,
}

/// One run's results, kept for partial runs built on it.
pub(crate) struct CachedRun {
    /// Collection, flow and environments the run used.
    pub(crate) scope: RunFlowInput,
    pub(crate) nodes: HashMap<String, CachedNode>,
    /// Every masked form the run used. A partial run masks them too.
    pub(crate) masking_secrets: HashSet<String>,
}

impl std::fmt::Debug for CachedRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedRun")
            .field("collection", &self.scope.collection)
            .field("flow_name", &self.scope.flow_name)
            .field("nodes", &self.nodes.len())
            .field("masking_secrets", &self.masking_secrets.len())
            .finish()
    }
}

/// What one run produced, for the cache. Holds raw values, so no `Debug`.
pub(crate) struct RunResults {
    /// Outcomes of the nodes this run executed. Seeds are left out.
    pub(crate) outcomes: HashMap<String, NodeOutcome>,
    /// Raw captured outputs of those nodes.
    pub(crate) captured: HashMap<String, CapturedOutput>,
    /// Every masked form this run used.
    pub(crate) masking_secrets: HashSet<String>,
}

/// Bytes an output holds: response body, base64 body, headers, status text,
/// script console, test results, script error and any deferred History entry.
pub(crate) fn output_size(output: &CapturedOutput) -> usize {
    match output {
        CapturedOutput::Request(out) => {
            let response = &out.response;
            let history = out.deferred_history.as_ref().map_or(0, |h| {
                h.id.len()
                    + h.method.len()
                    + h.url.len()
                    + h.collection.as_ref().map_or(0, String::len)
                    + h.request_name.as_ref().map_or(0, String::len)
            });
            response.body.len()
                + response.status_text.len()
                + response.body_base64.as_ref().map_or(0, String::len)
                + response
                    .headers
                    .iter()
                    .map(|h| h.key.len() + h.value.len())
                    .sum::<usize>()
                + out
                    .console_entries
                    .iter()
                    .map(|e| e.message.len())
                    .sum::<usize>()
                + out
                    .test_results
                    .iter()
                    .map(|t| t.name.len() + t.error.as_ref().map_or(0, String::len))
                    .sum::<usize>()
                + out.script_error.as_ref().map_or(0, String::len)
                + history
        }
        CapturedOutput::Value(VariableValue::Simple(text)) => text.len(),
        CapturedOutput::Value(VariableValue::Typed { data, value_type }) => {
            data.len() + value_type.len()
        }
    }
}

fn index(flow: &Flow) -> HashMap<&str, &FlowNode> {
    flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect()
}

/// Builds one node's entry. `used` is the output bytes the run already holds.
/// An output that would take the run past `budget` is dropped, so one run can
/// never pin more than the budget. Which of several outputs is dropped
/// depends on map order, but the planner refuses a dropped one cleanly.
fn cached_node(
    flow_node: Option<&FlowNode>,
    outcome: NodeOutcome,
    output: Option<CapturedOutput>,
    fingerprint: u64,
    used: &mut usize,
    budget: usize,
) -> CachedNode {
    // Input and Auth nodes always run again, and an Auth output is a token.
    let never_seeded = flow_node.is_some_and(|n| is_free_node(&n.kind));
    let output = output
        .map(|mut o| {
            // A partial run never needs the deferred History entry.
            if let CapturedOutput::Request(out) = &mut o {
                out.deferred_history = None;
            }
            o
        })
        .filter(|o| {
            let size = output_size(o);
            let keep = !never_seeded
                && size <= MAX_CACHED_OUTPUT_BYTES
                && used.saturating_add(size) <= budget;
            if keep {
                *used += size;
            }
            keep
        })
        .map(Arc::new);
    CachedNode {
        outcome,
        output,
        fingerprint,
        stale: false,
    }
}

impl CachedRun {
    /// Records a full run.
    pub(crate) fn from_full_run(
        scope: &RunFlowInput,
        flow: &Flow,
        results: RunResults,
        fingerprints: &HashMap<String, u64>,
    ) -> Self {
        Self::from_full_run_with_budget(scope, flow, results, fingerprints, CACHE_BYTE_BUDGET)
    }

    fn from_full_run_with_budget(
        scope: &RunFlowInput,
        flow: &Flow,
        results: RunResults,
        fingerprints: &HashMap<String, u64>,
        budget: usize,
    ) -> Self {
        let mut used = 0;
        let nodes_by_id = index(flow);
        let RunResults {
            outcomes,
            mut captured,
            masking_secrets,
        } = results;
        let nodes = outcomes
            .into_iter()
            .map(|(id, outcome)| {
                let node = cached_node(
                    nodes_by_id.get(id.as_str()).copied(),
                    outcome,
                    captured.remove(&id),
                    fingerprints.get(&id).copied().unwrap_or_default(),
                    &mut used,
                    budget,
                );
                (id, node)
            })
            .collect();
        Self {
            scope: scope.clone(),
            nodes,
            masking_secrets,
        }
    }

    /// The entry for a partial run built on `self`. `self` is not changed, so
    /// two partial runs on one base never see each other's results. Nodes
    /// downstream of the start node that the run did not reach (not in its
    /// plan, or cut off by Stop) are marked stale.
    pub(crate) fn merge_partial(
        &self,
        scope: &RunFlowInput,
        flow: &Flow,
        start_node_id: &str,
        results: RunResults,
        fingerprints: &HashMap<String, u64>,
    ) -> Self {
        self.merge_partial_with_budget(
            scope,
            flow,
            start_node_id,
            results,
            fingerprints,
            CACHE_BYTE_BUDGET,
        )
    }

    fn merge_partial_with_budget(
        &self,
        scope: &RunFlowInput,
        flow: &Flow,
        start_node_id: &str,
        results: RunResults,
        fingerprints: &HashMap<String, u64>,
        budget: usize,
    ) -> Self {
        let nodes_by_id = index(flow);
        let RunResults {
            outcomes,
            mut captured,
            masking_secrets,
        } = results;
        let mut nodes = self.nodes.clone();
        // Kept outputs of the base count first, except those this run replaces.
        let mut used: usize = self
            .nodes
            .iter()
            .filter(|(id, _)| !outcomes.contains_key(*id))
            .filter_map(|(_, node)| node.output.as_ref())
            .map(|output| output_size(output))
            .sum();
        let ran: HashSet<String> = outcomes.keys().cloned().collect();
        for (id, outcome) in outcomes {
            let node = cached_node(
                nodes_by_id.get(id.as_str()).copied(),
                outcome,
                captured.remove(&id),
                fingerprints.get(&id).copied().unwrap_or_default(),
                &mut used,
                budget,
            );
            nodes.insert(id, node);
        }
        let mut affected = rocket_flow::graph::reachable_from(flow, start_node_id);
        affected.push(start_node_id.to_string());
        for id in affected.iter().filter(|id| !ran.contains(*id)) {
            if let Some(node) = nodes.get_mut(id) {
                node.stale = true;
            }
        }
        let mut masks = self.masking_secrets.clone();
        masks.extend(masking_secrets);
        Self {
            scope: scope.clone(),
            nodes,
            masking_secrets: masks,
        }
    }

    /// What the planner needs to know about each node.
    pub(crate) fn seed_views(&self) -> HashMap<String, SeedView> {
        self.nodes
            .iter()
            .map(|(id, node)| {
                (
                    id.clone(),
                    SeedView {
                        outcome: node.outcome.clone(),
                        has_output: node.output.is_some(),
                        stale: node.stale,
                    },
                )
            })
            .collect()
    }

    /// Refuses a partial run for another flow or other environments.
    /// Variable values are not compared (decision D5).
    pub(crate) fn check_scope(
        &self,
        scope: &RunFlowInput,
        start_node_id: &str,
    ) -> Result<(), PartialRefusal> {
        let start = vec![start_node_id.to_string()];
        if self.scope.collection != scope.collection || self.scope.flow_name != scope.flow_name {
            return Err(PartialRefusal {
                message: "the earlier run belongs to another flow. Run the full flow first"
                    .to_string(),
                node_ids: start,
                edge_ids: Vec::new(),
            });
        }
        if self.scope.environment_name != scope.environment_name
            || self.scope.global_env_name != scope.global_env_name
        {
            let describe =
                |name: &Option<String>| name.as_deref().map_or("none".to_string(), |n| format!("'{n}'"));
            return Err(PartialRefusal {
                message: format!(
                    "the earlier run used environment {} and global environment {}, this run uses {} and {}. Run the full flow first",
                    describe(&self.scope.environment_name),
                    describe(&self.scope.global_env_name),
                    describe(&scope.environment_name),
                    describe(&scope.global_env_name)
                ),
                node_ids: start,
                edge_ids: Vec::new(),
            });
        }
        Ok(())
    }

    /// The upstream-most nodes that differ from this run, found by walking
    /// back from `seeds`. A node is reported when its fingerprint differs (or
    /// it did not run) and none of its sources differ, so the user sees the
    /// node they edited, not everything below it.
    pub(crate) fn changed_roots(
        &self,
        flow: &Flow,
        seeds: &[String],
        current: &HashMap<String, u64>,
    ) -> Vec<String> {
        let differs =
            |id: &str| self.nodes.get(id).map(|n| n.fingerprint) != current.get(id).copied();
        let mut roots = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = seeds.iter().filter(|s| differs(s)).cloned().collect();
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let changed_sources: Vec<String> = flow
                .edges
                .iter()
                .filter(|e| e.target_node_id == id && differs(&e.source_node_id))
                .map(|e| e.source_node_id.clone())
                .collect();
            if changed_sources.is_empty() {
                roots.push(id);
            } else {
                stack.extend(changed_sources);
            }
        }
        roots.sort();
        roots
    }

    /// Refuses when anything upstream of `seeds` changed since this run.
    pub(crate) fn check_unchanged(
        &self,
        flow: &Flow,
        seeds: &[String],
        current: &HashMap<String, u64>,
    ) -> Result<(), PartialRefusal> {
        let roots = self.changed_roots(flow, seeds, current);
        if roots.is_empty() {
            return Ok(());
        }
        let labels = roots
            .iter()
            .map(|id| format!("'{}'", label_in(flow, id)))
            .collect::<Vec<_>>()
            .join(", ");
        Err(PartialRefusal {
            message: format!(
                "{labels} changed since the earlier run, or did not run in it. Run the full flow, or Run from the first changed node"
            ),
            node_ids: roots,
            edge_ids: Vec::new(),
        })
    }
}

/// Sorts object keys at every level, so the text never depends on map order.
fn canonical(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(key, item)| (key, canonical(item)))
                    .collect(),
            )
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonical).collect())
        }
        other => other,
    }
}

/// Text no other call returns, for a value that failed to serialize. A
/// failure then reads as a change instead of hiding an edit.
fn unserializable_text() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "<unserializable:{}>",
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn canonical_text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .map(|v| canonical(v).to_string())
        .unwrap_or_else(|_| unserializable_text())
}

/// The text a saved request is fingerprinted by. `uid`, `file_name` and
/// `seq` are left out: a file without a stored uid gets a fresh one on every
/// load, and the other two describe placement, not what is sent.
pub(crate) fn saved_request_text(request: &Request) -> String {
    let mut request = request.clone();
    request.uid.clear();
    request.file_name = None;
    request.seq = None;
    canonical_text(&request)
}

/// One fingerprint per node, Merkle-style: the node's own configuration, its
/// saved request text, its incoming edges and its sources' fingerprints. An
/// edit anywhere upstream changes every fingerprint below it. Positions are
/// not hashed. `DefaultHasher` is fine because the value never leaves this
/// process. Settings a saved request inherits from its folder or collection
/// are not hashed, so an edit to them goes unseen (decision D5).
pub(crate) fn fingerprints(
    flow: &Flow,
    order: &[String],
    saved_request: &dyn Fn(&str) -> Option<String>,
) -> HashMap<String, u64> {
    let nodes_by_id = index(flow);
    let mut out: HashMap<String, u64> = HashMap::new();
    for id in order {
        let Some(node) = nodes_by_id.get(id.as_str()) else {
            continue;
        };
        let mut hasher = DefaultHasher::new();
        canonical_text(&node.kind).hash(&mut hasher);
        if let FlowNodeKind::Request {
            source: RequestSource::Saved { request_path },
            ..
        } = &node.kind
        {
            saved_request(request_path)
                .unwrap_or_else(|| "<missing>".to_string())
                .hash(&mut hasher);
        }
        let mut incoming: Vec<&FlowEdge> =
            flow.edges.iter().filter(|e| e.target_node_id == *id).collect();
        incoming.sort_by(|a, b| a.id.cmp(&b.id));
        for e in incoming {
            (
                &e.id,
                &e.source_node_id,
                &e.source_handle,
                &e.target_field,
                &e.expression,
            )
                .hash(&mut hasher);
            out.get(&e.source_node_id)
                .copied()
                .unwrap_or_default()
                .hash(&mut hasher);
        }
        out.insert(id.clone(), hasher.finish());
    }
    out
}

/// The external-secrets key under which a partial run masks a value from an
/// earlier run. A `{{name}}` lookup ends at the first `}}`
/// (`rocket_environment::resolve`), so a key that starts with `}}` can never
/// be referenced from a template.
pub(crate) fn previous_run_secret_key(index: usize) -> String {
    format!("}}}}prev-run.{index}")
}

/// The last few runs, most recently used first, within a byte budget.
pub(crate) struct FlowRunCache {
    runs: VecDeque<(String, Arc<CachedRun>)>,
    max_runs: usize,
    byte_budget: usize,
}

impl Default for FlowRunCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowRunCache {
    pub(crate) fn new() -> Self {
        Self::with_limits(MAX_CACHED_RUNS, CACHE_BYTE_BUDGET)
    }

    pub(crate) fn with_limits(max_runs: usize, byte_budget: usize) -> Self {
        Self {
            runs: VecDeque::new(),
            max_runs,
            byte_budget,
        }
    }

    /// The run with this id. A hit makes it the most recently used.
    pub(crate) fn get(&mut self, run_id: &str) -> Option<Arc<CachedRun>> {
        let index = self.runs.iter().position(|(id, _)| id == run_id)?;
        let entry = self.runs.remove(index)?;
        let run = Arc::clone(&entry.1);
        self.runs.push_front(entry);
        Some(run)
    }

    /// Adds a run as the most recent, then evicts the least recently used
    /// runs over the count limit or the byte budget. The newest entry always
    /// stays, even when it alone is over budget.
    pub(crate) fn insert(&mut self, run_id: String, run: CachedRun) {
        self.runs.retain(|(id, _)| *id != run_id);
        self.runs.push_front((run_id, Arc::new(run)));
        self.runs.truncate(self.max_runs.max(1));
        while self.runs.len() > 1 && self.output_bytes() > self.byte_budget {
            self.runs.pop_back();
        }
    }

    pub(crate) fn clear(&mut self) {
        self.runs.clear();
    }

    /// Whether a run is kept, without touching its place in the order.
    #[cfg(test)]
    pub(crate) fn contains(&self, run_id: &str) -> bool {
        self.runs.iter().any(|(id, _)| id == run_id)
    }

    /// Output bytes held, counting an output shared by several runs once.
    fn output_bytes(&self) -> usize {
        let mut seen: HashSet<*const CapturedOutput> = HashSet::new();
        self.runs
            .iter()
            .flat_map(|(_, run)| run.nodes.values())
            .filter_map(|node| node.output.as_ref())
            .filter(|output| seen.insert(Arc::as_ptr(output)))
            .map(|output| output_size(output))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::{handle, InlineRequestData, NodePosition};
    use rocket_shared::types::HttpMethod;

    fn request(id: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Request {
                label: id.to_string(),
                debug: false,
                repeat_until: None,
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "get".to_string(),
                        url: format!("https://api.example.com/{id}"),
                        headers: Vec::new(),
                        body: None,
                    },
                },
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn auth(id: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Auth {
                label: id.to_string(),
                auth: rocket_shared::types::Auth::Bearer {
                    token: "tok-123456".to_string(),
                },
                apply_to_inherit: false,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// `ids[0] -url-> ids[1] -url-> ...`, with edge ids e0, e1 and so on.
    fn chain(ids: &[&str]) -> (Flow, Vec<String>) {
        let nodes = ids.iter().map(|id| request(id)).collect();
        let edges = ids
            .windows(2)
            .enumerate()
            .map(|(i, pair)| FlowEdge {
                id: format!("e{i}"),
                source_node_id: pair[0].to_string(),
                target_node_id: pair[1].to_string(),
                target_field: "url".to_string(),
                expression: "response.body".to_string(),
                source_handle: handle::RESULT.to_string(),
            })
            .collect();
        let flow = Flow {
            name: "f".to_string(),
            nodes,
            edges,
            callback_host: None,
        };
        (flow, ids.iter().map(|s| s.to_string()).collect())
    }

    fn set_url(flow: &mut Flow, id: &str, url: &str) {
        if let Some(FlowNodeKind::Request {
            source: RequestSource::Inline { request },
            ..
        }) = flow.nodes.iter_mut().find(|n| n.id == id).map(|n| &mut n.kind)
        {
            request.url = url.to_string();
        }
    }

    fn scope() -> RunFlowInput {
        RunFlowInput {
            collection: "c".to_string(),
            flow_name: "f".to_string(),
            environment_name: None,
            global_env_name: None,
        }
    }

    fn results(entries: &[(&str, &str)]) -> RunResults {
        RunResults {
            outcomes: entries
                .iter()
                .map(|(id, _)| {
                    (
                        id.to_string(),
                        NodeOutcome::Succeeded {
                            chosen_exit: handle::RESULT.to_string(),
                        },
                    )
                })
                .collect(),
            captured: entries
                .iter()
                .map(|(id, v)| (id.to_string(), CapturedOutput::Value(VariableValue::simple(*v))))
                .collect(),
            masking_secrets: HashSet::new(),
        }
    }

    fn full_run(flow: &Flow, order: &[String], entries: &[(&str, &str)]) -> CachedRun {
        CachedRun::from_full_run(
            &scope(),
            flow,
            results(entries),
            &fingerprints(flow, order, &|_| None),
        )
    }

    fn value_of<'r>(run: &'r CachedRun, id: &str) -> Option<&'r str> {
        match run.nodes.get(id)?.output.as_deref()? {
            CapturedOutput::Value(v) => Some(v.data()),
            CapturedOutput::Request(_) => None,
        }
    }

    #[test]
    fn moving_a_node_keeps_fingerprints_and_an_edit_changes_every_downstream_one() {
        let (flow, order) = chain(&["a", "b", "c"]);
        let before = fingerprints(&flow, &order, &|_| None);
        let mut moved = flow.clone();
        moved.nodes[0].position.x = 500.0;
        assert_eq!(fingerprints(&moved, &order, &|_| None), before);
        let mut edited = flow.clone();
        set_url(&mut edited, "a", "https://api.example.com/a2");
        let after = fingerprints(&edited, &order, &|_| None);
        for id in ["a", "b", "c"] {
            assert_ne!(after[id], before[id], "{id}");
        }
    }

    #[test]
    fn editing_a_saved_request_file_changes_the_fingerprint() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: vec![FlowNode {
                id: "s".to_string(),
                kind: FlowNodeKind::Request {
                    label: "s".to_string(),
                    debug: false,
                    repeat_until: None,
                    source: RequestSource::Saved {
                        request_path: "login.yml".to_string(),
                    },
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: Vec::new(),
            callback_host: None,
        };
        let order = vec!["s".to_string()];
        let one = fingerprints(&flow, &order, &|_| Some("v1".to_string()));
        let two = fingerprints(&flow, &order, &|_| Some("v2".to_string()));
        assert_ne!(one["s"], two["s"]);
    }

    #[test]
    fn saved_request_text_ignores_uid_file_name_and_seq() {
        let mut one = Request::new("Login", HttpMethod::Post, "https://api.example.com/login");
        let mut two = one.clone();
        one.uid = "uid-1".to_string();
        two.uid = "uid-2".to_string();
        two.file_name = Some("Login.yml".to_string());
        two.seq = Some(3);
        assert_eq!(saved_request_text(&one), saved_request_text(&two));
        two.url = "https://api.example.com/login2".to_string();
        assert_ne!(saved_request_text(&one), saved_request_text(&two));
    }

    #[test]
    fn canonical_json_does_not_depend_on_key_order() {
        let mut first = serde_json::Map::new();
        first.insert("b".to_string(), serde_json::json!(1));
        first.insert("a".to_string(), serde_json::json!({ "y": 1, "x": 2 }));
        let mut second = serde_json::Map::new();
        second.insert("a".to_string(), serde_json::json!({ "x": 2, "y": 1 }));
        second.insert("b".to_string(), serde_json::json!(1));
        assert_eq!(
            canonical(serde_json::Value::Object(first)).to_string(),
            canonical(serde_json::Value::Object(second)).to_string()
        );
    }

    #[test]
    fn an_upstream_edit_refuses_and_names_the_edited_node() {
        let (flow, order) = chain(&["a", "b", "c"]);
        let base = full_run(&flow, &order, &[("a", "1"), ("b", "2"), ("c", "3")]);
        let mut edited = flow.clone();
        set_url(&mut edited, "a", "https://api.example.com/a2");
        let current = fingerprints(&edited, &order, &|_| None);
        let err = base
            .check_unchanged(&edited, &["b".to_string()], &current)
            .expect_err("a changed");
        assert_eq!(err.node_ids, vec!["a".to_string()]);
        assert!(
            err.message.contains("'a' changed since the earlier run"),
            "{}",
            err.message
        );
    }

    #[test]
    fn editing_the_start_node_is_allowed() {
        let (flow, order) = chain(&["a", "b", "c"]);
        let base = full_run(&flow, &order, &[("a", "1"), ("b", "2"), ("c", "3")]);
        let mut edited = flow.clone();
        set_url(&mut edited, "c", "https://api.example.com/c2");
        let current = fingerprints(&edited, &order, &|_| None);
        base.check_unchanged(&edited, &["b".to_string()], &current)
            .expect("only the start node c changed");
    }

    #[test]
    fn a_different_environment_is_refused() {
        let (flow, order) = chain(&["a"]);
        let base = full_run(&flow, &order, &[("a", "1")]);
        let mut other = scope();
        other.environment_name = Some("staging".to_string());
        let err = base.check_scope(&other, "a").expect_err("environment differs");
        assert!(err.message.contains("environment"), "{}", err.message);
        base.check_scope(&scope(), "a").expect("same scope");
    }

    #[test]
    fn a_full_run_keeps_outcomes_but_not_auth_or_oversized_outputs() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: vec![auth("au"), request("big"), request("ok")],
            edges: Vec::new(),
            callback_host: None,
        };
        let order = vec!["au".to_string(), "big".to_string(), "ok".to_string()];
        let huge = "x".repeat(MAX_CACHED_OUTPUT_BYTES + 1);
        let run = CachedRun::from_full_run(
            &scope(),
            &flow,
            results(&[("au", "tok-123456"), ("big", huge.as_str()), ("ok", "fine")]),
            &fingerprints(&flow, &order, &|_| None),
        );
        assert!(run.nodes["au"].output.is_none(), "an Auth token is never cached");
        assert!(run.nodes["big"].output.is_none());
        assert_eq!(value_of(&run, "ok"), Some("fine"));
        let views = run.seed_views();
        assert!(!views["big"].has_output);
        assert_eq!(
            views["au"].outcome,
            NodeOutcome::Succeeded {
                chosen_exit: handle::RESULT.to_string()
            }
        );
    }

    #[test]
    fn a_partial_entry_overlays_its_nodes_and_leaves_the_base_untouched() {
        let (flow, order) = chain(&["a", "b", "c", "d"]);
        let base = full_run(&flow, &order, &[("a", "1"), ("b", "old"), ("c", "3"), ("d", "4")]);
        let merged = base.merge_partial(
            &scope(),
            &flow,
            "b",
            results(&[("b", "new")]),
            &fingerprints(&flow, &order, &|_| None),
        );
        assert_eq!(value_of(&merged, "b"), Some("new"));
        assert_eq!(value_of(&base, "b"), Some("old"));
        assert!(merged.nodes["c"].stale && merged.nodes["d"].stale);
        assert!(!merged.nodes["a"].stale && !merged.nodes["b"].stale);
        assert!(!base.nodes["c"].stale, "the base entry never changes");
        let shared_a = merged.nodes["a"].output.as_ref().expect("a in merged");
        let base_a = base.nodes["a"].output.as_ref().expect("a in base");
        assert!(Arc::ptr_eq(shared_a, base_a), "untouched outputs are shared, not copied");
    }

    #[test]
    fn a_partial_entry_keeps_the_masks_of_its_base() {
        let (flow, order) = chain(&["a", "b"]);
        let mut base_results = results(&[("a", "1"), ("b", "2")]);
        base_results.masking_secrets.insert("old-token-123456".to_string());
        let base = CachedRun::from_full_run(
            &scope(),
            &flow,
            base_results,
            &fingerprints(&flow, &order, &|_| None),
        );
        let mut partial_results = results(&[("b", "3")]);
        partial_results.masking_secrets.insert("new-token-654321".to_string());
        let merged = base.merge_partial(
            &scope(),
            &flow,
            "b",
            partial_results,
            &fingerprints(&flow, &order, &|_| None),
        );
        assert!(merged.masking_secrets.contains("old-token-123456"));
        assert!(merged.masking_secrets.contains("new-token-654321"));
    }

    #[test]
    fn a_previous_run_secret_key_cannot_be_referenced_from_a_template() {
        let key = previous_run_secret_key(0);
        assert_eq!(key, "}}prev-run.0");
        let vars = HashMap::from([
            (key, "old-token-123456".to_string()),
            ("prev".to_string(), "control-value".to_string()),
        ]);
        let control = rocket_environment::resolve("{{prev}}", &vars).output;
        assert_eq!(control, "control-value", "a normal key resolves in this harness");
        for template in ["{{}}prev-run.0}}", "{{ }}prev-run.0 }}", "{{prev-run.0}}"] {
            let out = rocket_environment::resolve(template, &vars).output;
            assert!(!out.contains("old-token-123456"), "{template} -> {out}");
        }
    }

    #[test]
    fn the_debug_output_of_a_cached_run_hides_secrets_and_outputs() {
        let (flow, order) = chain(&["a"]);
        let mut entry = results(&[("a", "raw-body-with-token-123456")]);
        entry.masking_secrets.insert("old-token-123456".to_string());
        let run = CachedRun::from_full_run(
            &scope(),
            &flow,
            entry,
            &fingerprints(&flow, &order, &|_| None),
        );
        let printed = format!("{run:?}");
        assert!(printed.contains("CachedRun"));
        assert!(!printed.contains("123456"), "{printed}");
    }

    fn request_output(
        body: &str,
        with_history: bool,
    ) -> crate::execution_service::ExecuteRequestOutput {
        crate::execution_service::ExecuteRequestOutput {
            response: rocket_http::HttpResponse {
                status: 200,
                status_text: String::new(),
                headers: Vec::new(),
                body: body.to_string(),
                duration_ms: 0,
                ttfb_ms: 0,
                size_bytes: 0,
                is_binary: false,
                body_base64: None,
            },
            test_results: Vec::new(),
            console_entries: Vec::new(),
            script_error: None,
            deferred_history: with_history.then(|| {
                rocket_history::HistoryEntry::new("GET", "https://api.example.com/aaaa", 200, 1, 1)
            }),
        }
    }

    #[test]
    fn output_size_counts_console_tests_script_error_history_and_typed_values() {
        let plain = output_size(&CapturedOutput::Request(Box::new(request_output("", false))));
        let mut full = request_output("", true);
        full.console_entries.push(rocket_scripting::ConsoleEntry {
            level: rocket_scripting::ConsoleLevel::Log,
            message: "c".repeat(100),
        });
        full.test_results.push(rocket_scripting::TestResult {
            name: "n".repeat(10),
            status: rocket_scripting::TestStatus::Failed,
            error: Some("e".repeat(20)),
        });
        full.script_error = Some("s".repeat(30));
        let bigger = output_size(&CapturedOutput::Request(Box::new(full)));
        assert!(bigger >= plain + 100 + 10 + 20 + 30 + "https://api.example.com/aaaa".len());
        let typed = CapturedOutput::Value(VariableValue::typed("abc", "number"));
        assert_eq!(output_size(&typed), 9);
    }

    #[test]
    fn a_cached_request_output_drops_its_deferred_history() {
        let flow = Flow {
            name: "f".to_string(),
            nodes: vec![request("r")],
            edges: Vec::new(),
            callback_host: None,
        };
        let order = vec!["r".to_string()];
        let mut run_results = results(&[]);
        run_results.outcomes.insert(
            "r".to_string(),
            NodeOutcome::Succeeded {
                chosen_exit: handle::RESULT.to_string(),
            },
        );
        run_results.captured.insert(
            "r".to_string(),
            CapturedOutput::Request(Box::new(request_output("body", true))),
        );
        let run = CachedRun::from_full_run(
            &scope(),
            &flow,
            run_results,
            &fingerprints(&flow, &order, &|_| None),
        );
        match run.nodes["r"].output.as_deref() {
            Some(CapturedOutput::Request(out)) => assert!(out.deferred_history.is_none()),
            _ => panic!("the request output should be cached"),
        }
    }

    #[test]
    fn a_run_over_the_byte_budget_drops_outputs_and_the_planner_sees_them_missing() {
        let (flow, order) = chain(&["a", "b", "c"]);
        let text = "x".repeat(600);
        let run = CachedRun::from_full_run_with_budget(
            &scope(),
            &flow,
            results(&[("a", text.as_str()), ("b", text.as_str()), ("c", text.as_str())]),
            &fingerprints(&flow, &order, &|_| None),
            1_000,
        );
        let kept: usize = run
            .nodes
            .values()
            .filter_map(|n| n.output.as_ref())
            .map(|o| output_size(o))
            .sum();
        assert!(kept <= 1_000, "{kept}");
        assert_eq!(run.nodes.len(), 3, "outcomes are never dropped");
        let views = run.seed_views();
        assert_eq!(views.values().filter(|v| !v.has_output).count(), 2);
    }

    #[test]
    fn a_partial_merge_counts_the_base_outputs_against_the_budget() {
        let (flow, order) = chain(&["a", "b"]);
        let text = "x".repeat(600);
        let base = CachedRun::from_full_run_with_budget(
            &scope(),
            &flow,
            results(&[("a", text.as_str())]),
            &fingerprints(&flow, &order, &|_| None),
            1_000,
        );
        let merged = base.merge_partial_with_budget(
            &scope(),
            &flow,
            "b",
            results(&[("b", text.as_str())]),
            &fingerprints(&flow, &order, &|_| None),
            1_000,
        );
        assert!(merged.nodes["a"].output.is_some());
        assert!(merged.nodes["b"].output.is_none());
    }

    #[test]
    fn a_serialization_failure_never_hides_an_edit() {
        assert_ne!(unserializable_text(), unserializable_text());
    }

    #[test]
    fn an_over_budget_newest_run_evicts_the_older_runs_but_stays() {
        let (flow, order) = chain(&["a"]);
        let big = "x".repeat(2_000);
        let mut cache = FlowRunCache::with_limits(2, 1_000);
        cache.insert("r1".to_string(), full_run(&flow, &order, &[("a", "1")]));
        cache.insert("r2".to_string(), full_run(&flow, &order, &[("a", big.as_str())]));
        assert!(cache.contains("r2") && !cache.contains("r1"));
        cache.insert("r3".to_string(), full_run(&flow, &order, &[("a", "3")]));
        cache.insert("r4".to_string(), full_run(&flow, &order, &[("a", big.as_str())]));
        assert!(cache.contains("r4"));
        assert!(!cache.contains("r2") && !cache.contains("r3"));
    }

    #[test]
    fn the_cache_keeps_the_most_recently_used_runs() {
        let (flow, order) = chain(&["a"]);
        let mut cache = FlowRunCache::with_limits(2, usize::MAX);
        cache.insert("r1".to_string(), full_run(&flow, &order, &[("a", "1")]));
        cache.insert("r2".to_string(), full_run(&flow, &order, &[("a", "2")]));
        assert!(cache.get("r1").is_some(), "a lookup makes r1 the most recent");
        cache.insert("r3".to_string(), full_run(&flow, &order, &[("a", "3")]));
        assert!(cache.contains("r1") && cache.contains("r3"));
        assert!(!cache.contains("r2"));
        cache.clear();
        assert!(!cache.contains("r1"));
    }

    #[test]
    fn the_byte_budget_counts_a_shared_output_once_and_evicts_the_oldest() {
        let (flow, order) = chain(&["a", "b"]);
        let big = "x".repeat(600);
        let base = full_run(&flow, &order, &[("a", big.as_str()), ("b", "small")]);
        let merged = base.merge_partial(
            &scope(),
            &flow,
            "b",
            results(&[("b", "new")]),
            &fingerprints(&flow, &order, &|_| None),
        );
        let mut cache = FlowRunCache::with_limits(8, 1_000);
        cache.insert("base".to_string(), base);
        cache.insert("partial".to_string(), merged);
        assert!(
            cache.contains("base") && cache.contains("partial"),
            "a's 600 bytes are shared, so both entries fit"
        );
        let other = "y".repeat(600);
        cache.insert("other".to_string(), full_run(&flow, &order, &[("a", other.as_str())]));
        assert!(cache.contains("other"), "the newest entry always stays");
        assert!(!cache.contains("base") && !cache.contains("partial"));
    }
}
