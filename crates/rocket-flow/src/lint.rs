//! Non-blocking lint tier (roadmap F-20). `validate` rejects a graph that
//! cannot be saved or run. This module only warns about a graph that runs
//! but may not do what its author meant. Lints never block save or run.
//!
//! The module does no I/O. Facts that need the collection or the
//! environment come through `LintContext`, which the app layer provides.
//! Results are deterministic: nodes in file order, rules in a fixed order.

use crate::flow::{Flow, FlowEdge, FlowNode};
use crate::graph::FlowGraphError;
use crate::handle;
use crate::node::{FlowNodeKind, RequestSource};
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
/// A Request node uses a saved request that is no longer in the collection.
pub const DANGLING_SAVED_REQUEST: &str = "dangling_saved_request";
/// An inline Request or an Input node sends `{{name}}` as literal text,
/// because no variable has that name.
pub const UNKNOWN_VARIABLE: &str = "unknown_variable";
/// An Auth node that no Request can take its credential from.
pub const AUTH_NO_EFFECT: &str = "auth_no_effect";
/// An `auth` wire into a saved request that already has its own explicit auth.
pub const AUTH_WIRE_OVERRIDES_AUTH: &str = "auth_wire_overrides_auth";
/// A saved request sends a Wait for callback URL but has no path to that Wait.
pub const CALLBACK_NOT_WIRED: &str = "callback_not_wired";

/// Longest variable name the variable rule lints.
const MAX_NAME_LEN: usize = 64;
/// How many names a message lists before it counts the rest.
const MAX_LISTED_NAMES: usize = 5;

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

    /// Whether the saved request at `request_path` has its own auth, meaning
    /// an auth other than none and inherit.
    fn saved_request_auth_is_explicit(&self, _request_path: &str) -> Option<bool> {
        None
    }

    /// Which of the Wait for callback `names` the saved request at
    /// `request_path` sends as `{{callback.<name>}}`. The answer holds only
    /// names from `names`, never request text. `None` when it cannot tell.
    fn saved_request_callbacks(&self, _request_path: &str, _names: &[&str]) -> Option<Vec<String>> {
        None
    }
}

/// A context that knows nothing about the collection or the environment.
pub struct NoLintContext;

impl LintContext for NoLintContext {}

/// Lints `flow` without blocking anything. Works on any graph, valid or
/// not, and never panics on unknown ids or cycles.
pub fn validate_with_warnings(flow: &Flow, ctx: &dyn LintContext) -> Vec<FlowLint> {
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
        lints.extend(dangling_request_lint(node, ctx));
        lints.extend(unknown_variable_lint(node, &index, ctx));
        lints.extend(auth_no_effect_lint(node, flow, &index, ctx));
        lints.extend(auth_wire_lints(node, &index, ctx));
        lints.extend(callback_lints(node, &index, ctx));
    }
    lints
}

/// Lookups built once per call, so every rule stays linear in graph size.
struct GraphIndex<'a> {
    /// The exits of each node that have at least one wire.
    wired_exits: HashMap<&'a str, HashSet<&'a str>>,
    /// The request fields each node receives by wire.
    wired_fields: HashMap<&'a str, WiredFields>,
    /// Ids with a path to an Output, or `None` when the flow has no Output.
    reaching_output: Option<HashSet<&'a str>>,
    /// The first node of each id.
    nodes_by_id: HashMap<&'a str, &'a FlowNode>,
    /// The `auth` wires into each node.
    auth_wires_into: HashMap<&'a str, Vec<&'a FlowEdge>>,
    /// Ids of nodes with at least one outgoing `auth` wire.
    auth_sources: HashSet<&'a str>,
    /// The nodes each node has a wire to, of any kind.
    successors: HashMap<&'a str, Vec<&'a str>>,
    /// `(name, node)` of each Wait for callback node, first of a name wins.
    waits: Vec<(&'a str, &'a FlowNode)>,
}

/// Which fields of a node a wire replaces.
#[derive(Debug, Clone, Copy, Default)]
struct WiredFields {
    url: bool,
    body: bool,
    /// Any `headers[...]` wire. One flag, so a selector the rule cannot
    /// match to a header never leads to a false lint.
    headers: bool,
}

impl<'a> GraphIndex<'a> {
    fn new(flow: &'a Flow) -> Self {
        let mut wired_exits: HashMap<&'a str, HashSet<&'a str>> = HashMap::new();
        let mut wired_fields: HashMap<&'a str, WiredFields> = HashMap::new();
        let mut auth_wires_into: HashMap<&'a str, Vec<&'a FlowEdge>> = HashMap::new();
        let mut auth_sources: HashSet<&'a str> = HashSet::new();
        let mut successors: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
        for edge in &flow.edges {
            wired_exits
                .entry(edge.source_node_id.as_str())
                .or_default()
                .insert(edge.source_handle.as_str());
            successors
                .entry(edge.source_node_id.as_str())
                .or_default()
                .push(edge.target_node_id.as_str());
            if edge.target_field == handle::AUTH {
                auth_wires_into
                    .entry(edge.target_node_id.as_str())
                    .or_default()
                    .push(edge);
                auth_sources.insert(edge.source_node_id.as_str());
            }
            let fields = wired_fields.entry(edge.target_node_id.as_str()).or_default();
            match edge.target_field.as_str() {
                "url" => fields.url = true,
                "body" => fields.body = true,
                field if field.starts_with("headers[") => fields.headers = true,
                _ => {}
            }
        }
        let mut nodes_by_id: HashMap<&'a str, &'a FlowNode> =
            HashMap::with_capacity(flow.nodes.len());
        let mut waits: Vec<(&'a str, &'a FlowNode)> = Vec::new();
        let mut wait_names: HashSet<&'a str> = HashSet::new();
        for node in &flow.nodes {
            nodes_by_id.entry(node.id.as_str()).or_insert(node);
            if let FlowNodeKind::WaitForCallback { name, .. } = &node.kind {
                if !name.is_empty() && wait_names.insert(name.as_str()) {
                    waits.push((name.as_str(), node));
                }
            }
        }
        Self {
            wired_exits,
            wired_fields,
            reaching_output: nodes_reaching_output(flow),
            nodes_by_id,
            auth_wires_into,
            auth_sources,
            successors,
            waits,
        }
    }

    /// Every id reachable by following wires forward from `start`. The
    /// visited set makes a cycle safe.
    fn downstream_of(&self, start: &str) -> HashSet<&'a str> {
        let mut seen: HashSet<&'a str> = HashSet::new();
        let mut stack: Vec<&str> = vec![start];
        while let Some(id) = stack.pop() {
            for &next in self.successors.get(id).into_iter().flatten() {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        seen
    }

    fn wired_fields(&self, node_id: &str) -> WiredFields {
        self.wired_fields.get(node_id).copied().unwrap_or_default()
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

fn node_lint(
    severity: LintSeverity,
    code: &str,
    node: &FlowNode,
    message: String,
    hint: &str,
) -> FlowLint {
    FlowLint {
        code: code.to_string(),
        severity,
        node_id: Some(node.id.clone()),
        edge_id: None,
        message,
        hint: Some(hint.to_string()),
    }
}

fn warning(code: &str, node: &FlowNode, message: String, hint: &str) -> FlowLint {
    node_lint(LintSeverity::Warning, code, node, message, hint)
}

/// A Request node whose saved request the context says is gone. A run that
/// reaches the node reads the same file and fails there, so this is an error
/// lint. A node the context cannot answer for is skipped. The path is not
/// quoted.
fn dangling_request_lint(node: &FlowNode, ctx: &dyn LintContext) -> Option<FlowLint> {
    let FlowNodeKind::Request {
        source: RequestSource::Saved { request_path },
        ..
    } = &node.kind
    else {
        return None;
    };
    if ctx.saved_request_exists(request_path) != Some(false) {
        return None;
    }
    Some(node_lint(
        LintSeverity::Error,
        DANGLING_SAVED_REQUEST,
        node,
        format!(
            "'{}' uses a saved request that no longer exists in this collection.",
            display_label(node)
        ),
        "Pick another saved request in the node's properties, or restore the request file. A run that reaches this node fails there.",
    ))
}

/// The text of `{{name}}` placeholders in `text` that the variable rule
/// may lint, first seen first, each name once. This reads the text like
/// `rocket_environment::resolve`: the name runs to the first `}}` and is
/// trimmed. An unclosed `{{` is ignored. Only plain variable-like names
/// are kept, so JSON, GraphQL or script text that holds `{{` adds nothing.
fn placeholder_names<'a>(text: &'a str, seen: &mut HashSet<&'a str>, out: &mut Vec<&'a str>) {
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return;
        };
        let name = after[..end].trim();
        if lintable_name(name) && seen.insert(name) {
            out.push(name);
        }
        rest = &after[end + 2..];
    }
}

/// A plain variable name that the run resolves from the scopes the
/// context reads. Dynamic (`$`), `process.env.` and `callback.` names are
/// resolved by other means, so they are left out.
fn lintable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !name.starts_with("process.env.")
        && !name.starts_with("callback.")
}

fn braced(name: &str) -> String {
    let mut text = String::from("{{");
    text.push_str(name);
    text.push_str("}}");
    text
}

/// One warning per node that sends `{{name}}` as literal text. It covers an
/// inline Request's url, header names and values and body, and an Input
/// node's value, which are the fields the run resolves from variables and
/// leaves as written when a name is missing. A field that a wire replaces
/// is skipped. The message lists variable names only, never any other text.
fn unknown_variable_lint(
    node: &FlowNode,
    index: &GraphIndex<'_>,
    ctx: &dyn LintContext,
) -> Option<FlowLint> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut names: Vec<&str> = Vec::new();
    match &node.kind {
        FlowNodeKind::Request {
            source: RequestSource::Inline { request },
            ..
        } => {
            let wired = index.wired_fields(&node.id);
            if !wired.url {
                placeholder_names(&request.url, &mut seen, &mut names);
            }
            for header in &request.headers {
                placeholder_names(&header.name, &mut seen, &mut names);
                if !wired.headers {
                    placeholder_names(&header.value, &mut seen, &mut names);
                }
            }
            if let (Some(body), false) = (&request.body, wired.body) {
                placeholder_names(body, &mut seen, &mut names);
            }
        }
        FlowNodeKind::Input { value, .. } => placeholder_names(value.data(), &mut seen, &mut names),
        _ => return None,
    }
    let unknown: Vec<&str> = names
        .into_iter()
        .filter(|name| ctx.variable_is_known(name) == Some(false))
        .collect();
    if unknown.is_empty() {
        return None;
    }
    let mut list = unknown
        .iter()
        .take(MAX_LISTED_NAMES)
        .map(|name| braced(name))
        .collect::<Vec<_>>()
        .join(", ");
    if unknown.len() > MAX_LISTED_NAMES {
        list.push_str(&format!(" and {} more", unknown.len() - MAX_LISTED_NAMES));
    }
    let which = if unknown.len() == 1 {
        "that name"
    } else {
        "those names"
    };
    Some(warning(
        UNKNOWN_VARIABLE,
        node,
        format!(
            "'{}' leaves {list} as literal text, because no variable has {which}.",
            display_label(node)
        ),
        "Define the variable in the collection variables or in the active environment, or correct the name in the node.",
    ))
}

/// An Auth node that nothing uses. It is used when it has an `auth` wire, or
/// when it applies to inherited auth and some Request might take it. A
/// Request might take it when no `auth` wire sets its auth and it is inline,
/// or a saved request that does not have explicit auth, or one the context
/// cannot read. So the lint fires only when it is certain that no Request does.
fn auth_no_effect_lint(
    node: &FlowNode,
    flow: &Flow,
    index: &GraphIndex<'_>,
    ctx: &dyn LintContext,
) -> Option<FlowLint> {
    let FlowNodeKind::Auth {
        apply_to_inherit, ..
    } = &node.kind
    else {
        return None;
    };
    // Any wire out of the node uses its value, such as an `auth` wire or a data
    // wire into a header. Only a node with no wire out can be idle.
    if index.auth_sources.contains(node.id.as_str()) || index.successors.contains_key(node.id.as_str())
    {
        return None;
    }
    if *apply_to_inherit && inherited_auth_may_apply(flow, index, ctx) {
        return None;
    }
    let label = display_label(node);
    if *apply_to_inherit {
        Some(warning(
            AUTH_NO_EFFECT,
            node,
            format!("No Request in this flow takes the credential of '{label}'."),
            "Every Request has its own auth or an 'auth' wire, or the flow has no Request. Remove the node, or let a request inherit auth.",
        ))
    } else {
        Some(warning(
            AUTH_NO_EFFECT,
            node,
            format!(
                "'{label}' is not wired to any Request and does not apply to inherited auth, so no request uses its credential."
            ),
            "Wire its result into a Request's 'auth' input, or let it apply to requests that inherit auth.",
        ))
    }
}

/// False only when no Request can take the inherited credential.
fn inherited_auth_may_apply(flow: &Flow, index: &GraphIndex<'_>, ctx: &dyn LintContext) -> bool {
    flow.nodes.iter().any(|n| {
        let FlowNodeKind::Request { source, .. } = &n.kind else {
            return false;
        };
        // An `auth` wire sets the auth after the inherited credential, so it wins.
        if index.auth_wires_into.contains_key(n.id.as_str()) {
            return false;
        }
        match source {
            RequestSource::Inline { .. } => true,
            RequestSource::Saved { request_path } => {
                ctx.saved_request_auth_is_explicit(request_path) != Some(true)
            }
        }
    })
}

/// One lint per `auth` wire from an Auth node into a saved request that has
/// its own explicit auth. At run time the wire wins, so the request's own
/// auth is replaced. Inline requests have no auth of their own.
fn auth_wire_lints(
    node: &FlowNode,
    index: &GraphIndex<'_>,
    ctx: &dyn LintContext,
) -> Vec<FlowLint> {
    let FlowNodeKind::Request {
        source: RequestSource::Saved { request_path },
        ..
    } = &node.kind
    else {
        return Vec::new();
    };
    let Some(wires) = index.auth_wires_into.get(node.id.as_str()) else {
        return Vec::new();
    };
    if ctx.saved_request_auth_is_explicit(request_path) != Some(true) {
        return Vec::new();
    }
    wires
        .iter()
        .filter_map(|edge| {
            let source = index.nodes_by_id.get(edge.source_node_id.as_str())?;
            if !matches!(source.kind, FlowNodeKind::Auth { .. }) {
                return None;
            }
            let mut lint = warning(
                AUTH_WIRE_OVERRIDES_AUTH,
                node,
                format!(
                    "The credential of '{}' replaces the own auth of '{}'.",
                    display_label(source),
                    display_label(node)
                ),
                "The wire wins at run time. Set the request's auth to inherit, or remove the wire.",
            );
            lint.edge_id = Some(edge.id.clone());
            Some(lint)
        })
        .collect()
}

/// One lint per Wait for callback node whose `{{callback.<name>}}` a saved
/// request sends without a path to that Wait. The request registers the URL,
/// so it must run before the Wait starts (rule V15 checks inline requests).
/// Disabled fields and scripts are not scanned, so a lint here is certain.
fn callback_lints(node: &FlowNode, index: &GraphIndex<'_>, ctx: &dyn LintContext) -> Vec<FlowLint> {
    let FlowNodeKind::Request {
        source: RequestSource::Saved { request_path },
        ..
    } = &node.kind
    else {
        return Vec::new();
    };
    if index.waits.is_empty() {
        return Vec::new();
    }
    // A wire replaces the saved field at run time, so the saved text may not be
    // sent. Stay silent rather than guess which field carried the callback URL.
    let wired = index.wired_fields(&node.id);
    if wired.url || wired.body || wired.headers {
        return Vec::new();
    }
    let names: Vec<&str> = index.waits.iter().map(|(name, _)| *name).collect();
    let Some(used) = ctx.saved_request_callbacks(request_path, &names) else {
        return Vec::new();
    };
    if used.is_empty() {
        return Vec::new();
    }
    let downstream = index.downstream_of(&node.id);
    index
        .waits
        .iter()
        .filter(|(name, wait)| {
            used.iter().any(|u| u.as_str() == *name) && !downstream.contains(wait.id.as_str())
        })
        .map(|(_, wait)| {
            warning(
                CALLBACK_NOT_WIRED,
                node,
                format!(
                    "'{}' sends the callback URL of '{}', but it is not wired before that Wait for callback node.",
                    display_label(node),
                    display_label(wait)
                ),
                "Wire this request's 'result' exit into the Wait for callback node's 'trigger' input, so the URL is sent before the wait starts.",
            )
        })
        .collect()
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
            format!(
                "The {} exits of '{label}' have no wire.",
                unwired.join(", ")
            )
        };
        lints.push(warning(
            EXIT_WITHOUT_EDGE,
            node,
            message,
            "A run that takes an unwired exit ends that branch. Wire it to a node if the run should go on.",
        ));
    }
    if matches!(node.kind, FlowNodeKind::Switch { .. })
        && !index.is_wired(&node.id, handle::DEFAULT)
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
    let lint =
        |node_id: Option<&str>, edge_id: Option<&str>, message: String, hint: &str| FlowLint {
            code: INVALID_GRAPH.to_string(),
            severity: LintSeverity::Error,
            node_id: node_id.map(str::to_string),
            edge_id: edge_id.map(str::to_string),
            message,
            hint: Some(hint.to_string()),
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
        return Some(format!(
            "The {kind} node's wire must target its 'input' handle."
        ));
    }
    if let Some(count) = rest.strip_prefix(" needs exactly one input wire, found ") {
        let plain = !count.is_empty() && count.chars().all(|c| c.is_ascii_digit());
        return plain
            .then(|| format!("The {kind} node needs exactly one input wire, found {count}."));
    }
    let field = match *kind {
        "If" => "condition",
        "Switch" => "value",
        _ => "script",
    };
    (rest == format!("'s {field} is empty")).then(|| format!("The {kind} node's {field} is empty."))
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
    use crate::node::{InlineHeader, InlineRequestData, NodePosition, RequestSource, SwitchCase};

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
            vec![
                input("in"),
                if_node("if1", "Check"),
                output("ok"),
                output("ko"),
            ],
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
            vec![
                input("in"),
                if_node("if1", "Check"),
                request("req"),
                output("out"),
            ],
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
            vec![
                input("in"),
                if_node("if1", "First"),
                if_node("if1", "Second"),
            ],
            vec![edge("e1", "in", handle::RESULT, "if1", handle::INPUT)],
        );
        let lints = lint(&f);
        let exits = only(&lints, EXIT_WITHOUT_EDGE);
        assert_eq!(exits.len(), 1);
        assert!(exits[0].message.contains("'First'"));
    }

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

    /// A context that answers from maps, so a test controls every fact.
    #[derive(Default)]
    struct AuthCallbackContext {
        explicit_auth: HashMap<String, bool>,
        callbacks: HashMap<String, Vec<String>>,
    }

    impl LintContext for AuthCallbackContext {
        fn saved_request_auth_is_explicit(&self, request_path: &str) -> Option<bool> {
            self.explicit_auth.get(request_path).copied()
        }

        fn saved_request_callbacks(
            &self,
            request_path: &str,
            names: &[&str],
        ) -> Option<Vec<String>> {
            self.callbacks.get(request_path).map(|used| {
                used.iter()
                    .filter(|name| names.contains(&name.as_str()))
                    .cloned()
                    .collect()
            })
        }
    }

    fn explicit(paths: &[(&str, bool)]) -> AuthCallbackContext {
        AuthCallbackContext {
            explicit_auth: paths.iter().map(|(p, v)| (p.to_string(), *v)).collect(),
            ..AuthCallbackContext::default()
        }
    }

    fn lint_with(f: &Flow, ctx: &dyn LintContext) -> Vec<FlowLint> {
        validate_with_warnings(f, ctx)
    }

    fn inline_req(id: &str) -> FlowNode {
        inline_request(id, id, "https://example.test/x", &[], None)
    }

    #[test]
    fn an_auth_node_that_applies_to_an_inline_request_is_not_flagged() {
        let f = flow(
            vec![auth("a", true), inline_req("r"), output("out")],
            vec![edge("e1", "r", handle::RESULT, "out", "value")],
        );
        assert!(only(&lint(&f), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn an_idle_auth_node_with_no_wire_is_one_warning_naming_the_label() {
        let f = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![edge("e1", "r", handle::RESULT, "out", "value")],
        );
        let lints = lint(&f);
        let found = only(&lints, AUTH_NO_EFFECT);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id.as_deref(), Some("a"));
        assert_eq!(found[0].edge_id, None);
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert!(found[0].message.contains("'a'"), "{}", found[0].message);
        assert!(found[0].hint.is_some());
    }

    #[test]
    fn an_auth_node_with_an_auth_wire_has_an_effect_even_when_not_applied() {
        let f = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        assert!(only(&lint(&f), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn an_applying_auth_node_in_a_flow_without_requests_is_flagged() {
        let f = flow(vec![auth("a", true), input("in"), output("out")], Vec::new());
        assert_eq!(only(&lint(&f), AUTH_NO_EFFECT).len(), 1);
    }

    #[test]
    fn an_applying_auth_node_is_flagged_when_every_saved_request_has_explicit_auth() {
        let f = flow(
            vec![auth("a", true), request("r1"), request("r2"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        let ctx = explicit(&[("r1.yml", true), ("r2.yml", true)]);
        assert_eq!(only(&lint_with(&f, &ctx), AUTH_NO_EFFECT).len(), 1);
    }

    #[test]
    fn an_applying_auth_node_is_not_flagged_when_one_saved_request_inherits() {
        let f = flow(
            vec![auth("a", true), request("r1"), request("r2"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        let ctx = explicit(&[("r1.yml", true), ("r2.yml", false)]);
        assert!(only(&lint_with(&f, &ctx), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn an_unknown_saved_request_never_causes_an_auth_no_effect_lint() {
        let f = flow(
            vec![auth("a", true), request("r1"), request("r2"), output("out")],
            vec![edge("e1", "r1", handle::RESULT, "out", "value")],
        );
        // The second request is unknown to the context, so it might inherit.
        let ctx = explicit(&[("r1.yml", true)]);
        assert!(only(&lint_with(&f, &ctx), AUTH_NO_EFFECT).is_empty());
        assert!(only(&lint(&f), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn a_request_taking_another_auth_wire_does_not_use_the_inherited_credential() {
        let f = flow(
            vec![auth("a1", true), auth("a2", false), inline_req("r"), output("out")],
            vec![
                edge("e1", "a2", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        let lints = lint(&f);
        let found = only(&lints, AUTH_NO_EFFECT);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id.as_deref(), Some("a1"));
    }

    #[test]
    fn an_auth_wire_into_a_saved_request_with_explicit_auth_is_one_edge_warning() {
        let f = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        let ctx = explicit(&[("r.yml", true)]);
        let lints = lint_with(&f, &ctx);
        let found = only(&lints, AUTH_WIRE_OVERRIDES_AUTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id.as_deref(), Some("r"));
        assert_eq!(found[0].edge_id.as_deref(), Some("e1"));
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert!(found[0].message.contains("'a'") && found[0].message.contains("'r'"));
        assert!(found[0].message.contains("replaces"), "{}", found[0].message);
    }

    #[test]
    fn an_auth_wire_is_silent_when_the_request_inherits_or_is_unknown_or_inline() {
        let saved = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        assert!(only(
            &lint_with(&saved, &explicit(&[("r.yml", false)])),
            AUTH_WIRE_OVERRIDES_AUTH
        )
        .is_empty());
        assert!(only(&lint(&saved), AUTH_WIRE_OVERRIDES_AUTH).is_empty());
        let inline = flow(
            vec![auth("a", false), inline_req("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        assert!(only(
            &lint_with(&inline, &explicit(&[("r", true)])),
            AUTH_WIRE_OVERRIDES_AUTH
        )
        .is_empty());
    }

    #[test]
    fn auth_lints_never_quote_the_credential() {
        let f = flow(
            vec![auth("a", false), request("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        let idle = flow(vec![auth("a", false), request("r")], Vec::new());
        let mut all = lint_with(&f, &explicit(&[("r.yml", true)]));
        all.extend(lint(&idle));
        assert!(!all.is_empty());
        for l in &all {
            let text = format!("{} {}", l.message, l.hint.as_deref().unwrap_or(""));
            assert!(!text.contains("{{token}}"), "leaked in: {text}");
        }
    }

    fn wait_node(id: &str, label: &str, name: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::WaitForCallback {
                label: label.to_string(),
                name: name.to_string(),
                timeout_ms: 60_000,
                accept_when: None,
            },
        )
    }

    /// `path` sends `{{callback.<name>}}` for each given name.
    fn sends(path: &str, names: &[&str]) -> AuthCallbackContext {
        AuthCallbackContext {
            callbacks: HashMap::from([(
                path.to_string(),
                names.iter().map(|n| n.to_string()).collect(),
            )]),
            ..AuthCallbackContext::default()
        }
    }

    #[test]
    fn a_saved_request_wired_before_its_wait_is_not_flagged() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment"), output("out")],
            vec![
                edge("e1", "r", handle::RESULT, "w", handle::TRIGGER),
                edge("e2", "w", handle::RESULT, "out", "value"),
            ],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        assert!(only(&lints, CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn a_saved_request_with_no_path_to_its_wait_is_one_warning_naming_both_labels() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment"), output("out")],
            vec![edge("e1", "w", handle::RESULT, "out", "value")],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        let found = only(&lints, CALLBACK_NOT_WIRED);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node_id.as_deref(), Some("r"));
        assert_eq!(found[0].edge_id, None);
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert!(found[0].message.contains("'r'") && found[0].message.contains("'Pay wait'"));
        assert!(found[0].hint.is_some());
    }

    #[test]
    fn a_request_downstream_of_its_wait_is_flagged() {
        let f = flow(
            vec![wait_node("w", "Pay wait", "payment"), request("r"), output("out")],
            vec![
                edge("e1", "w", handle::RESULT, "r", handle::TRIGGER),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        assert_eq!(only(&lints, CALLBACK_NOT_WIRED).len(), 1);
    }

    #[test]
    fn a_request_reaching_its_wait_through_other_nodes_is_not_flagged() {
        let f = flow(
            vec![request("r1"), request("r2"), wait_node("w", "Pay wait", "payment")],
            vec![
                edge("e1", "r1", handle::RESULT, "r2", handle::TRIGGER),
                edge("e2", "r2", handle::RESULT, "w", handle::TRIGGER),
            ],
        );
        let lints = lint_with(&f, &sends("r1.yml", &["payment"]));
        assert!(only(&lints, CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn a_request_that_does_not_use_the_variable_or_is_unknown_is_silent() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment")],
            Vec::new(),
        );
        assert!(only(&lint_with(&f, &sends("r.yml", &[])), CALLBACK_NOT_WIRED).is_empty());
        assert!(only(
            &lint_with(&f, &sends("other.yml", &["payment"])),
            CALLBACK_NOT_WIRED
        )
        .is_empty());
        assert!(only(&lint(&f), CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn only_the_wait_without_a_path_is_named() {
        let f = flow(
            vec![
                request("r"),
                wait_node("w1", "First wait", "one"),
                wait_node("w2", "Second wait", "two"),
            ],
            vec![edge("e1", "r", handle::RESULT, "w1", handle::TRIGGER)],
        );
        let lints = lint_with(&f, &sends("r.yml", &["one", "two"]));
        let found = only(&lints, CALLBACK_NOT_WIRED);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("'Second wait'"), "{}", found[0].message);
    }

    #[test]
    fn an_inline_request_is_left_to_the_validate_rule() {
        let f = flow(
            vec![inline_req("r"), wait_node("w", "Pay wait", "payment")],
            Vec::new(),
        );
        assert!(only(&lint_with(&f, &sends("r", &["payment"])), CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn a_cycle_does_not_hang_the_callback_rule() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment")],
            vec![
                edge("e1", "r", handle::RESULT, "w", handle::TRIGGER),
                edge("e2", "w", handle::RESULT, "r", handle::TRIGGER),
            ],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        assert!(only(&lints, CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn an_auth_node_wired_into_a_data_field_is_not_idle() {
        let f = flow(
            vec![auth("a", false), inline_req("r"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "r", "headers[Authorization].value"),
                edge("e2", "r", handle::RESULT, "out", "value"),
            ],
        );
        assert!(only(&lint(&f), AUTH_NO_EFFECT).is_empty());
    }

    #[test]
    fn a_wired_url_means_the_saved_url_is_not_scanned_for_callbacks() {
        let f = flow(
            vec![
                input("in"),
                request("r"),
                wait_node("w", "Pay wait", "payment"),
            ],
            vec![edge("e1", "in", handle::RESULT, "r", "url")],
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        assert!(only(&lints, CALLBACK_NOT_WIRED).is_empty());
    }

    #[test]
    fn the_callback_lint_never_quotes_the_wait_name() {
        let f = flow(
            vec![request("r"), wait_node("w", "Pay wait", "payment")],
            Vec::new(),
        );
        let lints = lint_with(&f, &sends("r.yml", &["payment"]));
        let found = only(&lints, CALLBACK_NOT_WIRED);
        assert_eq!(found.len(), 1);
        let text = format!(
            "{} {}",
            found[0].message,
            found[0].hint.as_deref().unwrap_or("")
        );
        assert!(
            !text.contains("payment") && !text.contains("callback."),
            "{text}"
        );
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
            vec![
                auth("a1", false),
                request("r1"),
                request("r2"),
                output("out"),
            ],
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
        let no_path: Vec<FlowLint> = lint(&idle)
            .into_iter()
            .filter(|l| l.code == NO_PATH_TO_OUTPUT)
            .collect();
        assert_eq!(keys(&no_path), vec![(Some("a1"), None, NO_PATH_TO_OUTPUT)]);
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
            edges.push(edge(
                &format!("e{i}"),
                &format!("if{i}"),
                handle::TRUE,
                &next,
                field,
            ));
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
            let all = format!("{:?} {:?} {} {:?}", l.node_id, l.edge_id, l.message, l.hint);
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
        assert!(
            lints[0].message.contains("same name"),
            "{}",
            lints[0].message
        );
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
        assert_eq!(
            keys(&lint(&f)),
            vec![(Some("lonely"), None, NO_PATH_TO_OUTPUT)]
        );
        assert_eq!(
            keys(&graph_lints(&f)),
            vec![(Some("out"), None, INVALID_GRAPH)]
        );
    }

    #[test]
    fn a_routing_node_without_a_path_does_not_claim_a_result() {
        let f = flow(
            vec![
                input("in1"),
                if_node("if1", "Check"),
                input("in2"),
                output("out"),
            ],
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
        assert_eq!(sentence("the Foo node's condition is empty"), generic);
    }

    /// Answers from fixed lists. Anything not listed gets `None`.
    struct FakeContext {
        requests: Vec<(&'static str, bool)>,
        variables: Vec<(&'static str, bool)>,
    }

    impl LintContext for FakeContext {
        fn saved_request_exists(&self, request_path: &str) -> Option<bool> {
            self.requests
                .iter()
                .find(|(path, _)| *path == request_path)
                .map(|(_, exists)| *exists)
        }

        fn variable_is_known(&self, name: &str) -> Option<bool> {
            self.variables
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, answer)| *answer)
        }
    }

    /// Says every saved request is missing and every variable is unknown.
    struct EverythingMissing;

    impl LintContext for EverythingMissing {
        fn saved_request_exists(&self, _request_path: &str) -> Option<bool> {
            Some(false)
        }

        fn variable_is_known(&self, _name: &str) -> Option<bool> {
            Some(false)
        }
    }

    /// Counts the questions, to prove how often a rule asks.
    #[derive(Default)]
    struct Counting {
        requests: std::cell::Cell<usize>,
        variables: std::cell::Cell<usize>,
    }

    impl LintContext for Counting {
        fn saved_request_exists(&self, _request_path: &str) -> Option<bool> {
            self.requests.set(self.requests.get() + 1);
            Some(true)
        }

        fn variable_is_known(&self, _name: &str) -> Option<bool> {
            self.variables.set(self.variables.get() + 1);
            Some(true)
        }
    }

    fn saved_request(id: &str, label: &str, path: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                label: label.to_string(),
                source: RequestSource::Saved {
                    request_path: path.to_string(),
                },
                debug: false,
                repeat_until: None,
            },
        )
    }

    fn missing(path: &'static str) -> FakeContext {
        FakeContext {
            requests: vec![(path, false)],
            variables: vec![],
        }
    }

    #[test]
    fn a_missing_saved_request_is_an_error_naming_the_node() {
        let f = flow(vec![saved_request("n1", "Log in", "auth/login.yml")], vec![]);
        let lints = validate_with_warnings(&f, &missing("auth/login.yml"));
        let found = only(&lints, DANGLING_SAVED_REQUEST);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, LintSeverity::Error);
        assert_eq!(found[0].node_id.as_deref(), Some("n1"));
        assert_eq!(found[0].edge_id, None);
        assert_eq!(
            found[0].message,
            "'Log in' uses a saved request that no longer exists in this collection."
        );
        let hint = found[0].hint.as_deref().expect("a hint");
        assert!(hint.starts_with("Pick another saved request"), "got: {hint}");
    }

    #[test]
    fn the_message_never_quotes_the_path() {
        let f = flow(vec![saved_request("n1", "Log in", "private/s3cr3t-dir/login.yml")], vec![]);
        let lints = validate_with_warnings(&f, &missing("private/s3cr3t-dir/login.yml"));
        for l in &lints {
            let text = format!("{} {}", l.message, l.hint.as_deref().unwrap_or(""));
            assert!(!text.contains("s3cr3t"), "leaked in: {text}");
            assert!(!text.contains("login.yml"), "leaked in: {text}");
        }
    }

    #[test]
    fn an_existing_or_unanswered_request_has_no_lint() {
        let f = flow(
            vec![
                saved_request("a", "A", "a.yml"),
                saved_request("b", "B", "b.yml"),
                saved_request("c", "C", "c.yml"),
            ],
            vec![],
        );
        let ctx = FakeContext {
            requests: vec![("a.yml", true), ("b.yml", true)],
            variables: vec![],
        };
        // `c.yml` is not listed, so the context cannot tell and the rule is skipped.
        assert!(only(&validate_with_warnings(&f, &ctx), DANGLING_SAVED_REQUEST).is_empty());
        assert!(only(&lint(&f), DANGLING_SAVED_REQUEST).is_empty());
    }

    #[test]
    fn an_inline_request_is_never_checked() {
        let f = flow(vec![inline_request("a", "A", "https://x.test", &[], None)], vec![]);
        let counting = Counting::default();
        let lints = validate_with_warnings(&f, &counting);
        assert!(only(&lints, DANGLING_SAVED_REQUEST).is_empty());
        assert_eq!(counting.requests.get(), 0);
    }

    #[test]
    fn lints_follow_node_order_and_each_node_asks_once() {
        let f = flow(
            vec![
                saved_request("z", "Zed", "z.yml"),
                saved_request("a", "Aye", "a.yml"),
            ],
            vec![],
        );
        let lints = validate_with_warnings(&f, &EverythingMissing);
        let ids: Vec<_> = only(&lints, DANGLING_SAVED_REQUEST)
            .iter()
            .map(|l| l.node_id.as_deref())
            .collect();
        assert_eq!(ids, vec![Some("z"), Some("a")]);
        let counting = Counting::default();
        let _ = validate_with_warnings(&f, &counting);
        assert_eq!(counting.requests.get(), 2);
    }

    /// n saved Request nodes and no wires.
    fn saved_requests(n: usize) -> Flow {
        flow(
            (0..n)
                .map(|i| saved_request(&format!("n{i}"), &format!("Req {i}"), &format!("r{i}.yml")))
                .collect(),
            vec![],
        )
    }

    #[test]
    fn saved_request_rule_is_linear() {
        let lints = validate_with_warnings(&saved_requests(20_000), &EverythingMissing);
        assert_eq!(only(&lints, DANGLING_SAVED_REQUEST).len(), 20_000);
        assert_linear(saved_requests, |f| {
            let _ = validate_with_warnings(f, &EverythingMissing);
        });
    }

    /// An inline Request. Header pairs are `(name, value)`.
    fn inline_request(
        id: &str,
        label: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                label: label.to_string(),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "GET".to_string(),
                        url: url.to_string(),
                        headers: headers
                            .iter()
                            .map(|(name, value)| InlineHeader {
                                name: name.to_string(),
                                value: value.to_string(),
                            })
                            .collect(),
                        body: body.map(str::to_string),
                    },
                },
                debug: false,
                repeat_until: None,
            },
        )
    }

    fn unknown_in(f: &Flow) -> Vec<FlowLint> {
        let all = validate_with_warnings(f, &EverythingMissing);
        all.into_iter().filter(|l| l.code == UNKNOWN_VARIABLE).collect()
    }

    #[test]
    fn unknown_names_in_url_headers_and_body_are_one_warning_listing_names() {
        let f = flow(
            vec![inline_request(
                "n1",
                "Create user",
                "https://x.test/{{userId}}?k={{ apiKey }}",
                &[("X-{{tenant}}", "{{userId}}")],
                Some("{\"a\": \"{{payload}}\"}"),
            )],
            vec![],
        );
        let found = unknown_in(&f);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, LintSeverity::Warning);
        assert_eq!(found[0].node_id.as_deref(), Some("n1"));
        assert_eq!(
            found[0].message,
            "'Create user' leaves {{userId}}, {{apiKey}}, {{tenant}}, {{payload}} as literal text, because no variable has those names."
        );
        let hint = found[0].hint.as_deref().expect("a hint");
        assert!(hint.starts_with("Define the variable"), "got: {hint}");
    }

    #[test]
    fn one_unknown_name_reads_in_the_singular() {
        let f = flow(
            vec![inline_request("n1", "Get", "https://x.test/{{id}}/{{id}}", &[], None)],
            vec![],
        );
        assert_eq!(
            unknown_in(&f)[0].message,
            "'Get' leaves {{id}} as literal text, because no variable has that name."
        );
    }

    #[test]
    fn more_than_five_names_are_cut_with_a_count() {
        let f = flow(
            vec![inline_request(
                "n1",
                "Big",
                "{{a}}{{b}}{{c}}{{d}}{{e}}{{f}}{{g}}",
                &[],
                None,
            )],
            vec![],
        );
        assert_eq!(
            unknown_in(&f)[0].message,
            "'Big' leaves {{a}}, {{b}}, {{c}}, {{d}}, {{e}} and 2 more as literal text, because no variable has those names."
        );
    }

    #[test]
    fn known_and_unanswered_names_are_skipped() {
        let f = flow(
            vec![inline_request("n1", "Get", "{{known}}/{{maybe}}/{{gone}}", &[], None)],
            vec![],
        );
        let ctx = FakeContext {
            requests: vec![],
            variables: vec![("known", true), ("gone", false)],
        };
        let lints = validate_with_warnings(&f, &ctx);
        let found = only(&lints, UNKNOWN_VARIABLE);
        assert_eq!(found.len(), 1);
        // `maybe` got None and `known` got true, so only `gone` is named.
        assert!(found[0].message.contains("{{gone}}"));
        assert!(!found[0].message.contains("maybe"));
        assert!(!found[0].message.contains("known"));
        assert!(only(&lint(&f), UNKNOWN_VARIABLE).is_empty());
    }

    #[test]
    fn run_time_and_odd_names_are_never_linted() {
        let f = flow(
            vec![inline_request(
                "n1",
                "Get",
                "{{$guid}} {{ $randomInt }} {{process.env.HOME}} {{callback.pay}} {{}} {{\"x\":1}} {{a b}} {{open",
                &[],
                Some("query {{ user { id } }}"),
            )],
            vec![],
        );
        // EverythingMissing says "no" to anything it is asked, so silence proves the rule's own filter.
        assert!(unknown_in(&f).is_empty());
    }

    #[test]
    fn the_message_lists_names_and_nothing_else() {
        let f = flow(
            vec![inline_request(
                "n1",
                "Get",
                "https://secret-host.test/{{token}}?key=s3cr3t",
                &[("Authorization", "Bearer s3cr3t {{token}}")],
                Some("{\"password\": \"s3cr3t\"}"),
            )],
            vec![],
        );
        let found = unknown_in(&f);
        let text = format!("{} {}", found[0].message, found[0].hint.as_deref().unwrap_or(""));
        for leaked in ["s3cr3t", "secret-host", "https", "Bearer", "password"] {
            assert!(!text.contains(leaked), "{leaked} leaked in: {text}");
        }
    }

    #[test]
    fn a_wired_field_is_not_linted() {
        let f = flow(
            vec![
                input("in"),
                inline_request(
                    "n1",
                    "Get",
                    "{{urlVar}}",
                    &[("X-A", "{{headerVar}}")],
                    Some("{{bodyVar}}"),
                ),
            ],
            vec![
                edge("e1", "in", handle::RESULT, "n1", "url"),
                edge("e2", "in", handle::RESULT, "n1", "headers[X-A].value"),
                edge("e3", "in", handle::RESULT, "n1", "body"),
            ],
        );
        assert!(unknown_in(&f).is_empty());
        // A wire into one field leaves the other fields linted.
        let f = flow(
            vec![input("in"), inline_request("n1", "Get", "{{urlVar}}", &[], Some("{{bodyVar}}"))],
            vec![edge("e1", "in", handle::RESULT, "n1", "url")],
        );
        let found = unknown_in(&f);
        assert!(found[0].message.contains("{{bodyVar}}"));
        assert!(!found[0].message.contains("urlVar"));
    }

    #[test]
    fn an_input_node_value_is_linted() {
        let f = flow(
            vec![node(
                "in",
                FlowNodeKind::Input {
                    label: "User".to_string(),
                    value: rocket_shared::VariableValue::simple("{{who}}"),
                },
            )],
            vec![],
        );
        assert_eq!(
            unknown_in(&f)[0].message,
            "'User' leaves {{who}} as literal text, because no variable has that name."
        );
    }

    #[test]
    fn a_saved_request_has_no_variable_lint() {
        let f = flow(vec![saved_request("n1", "Log in", "{{x}}.yml")], vec![]);
        assert!(unknown_in(&f).is_empty());
    }

    #[test]
    fn the_rule_asks_once_per_distinct_name_per_node() {
        let f = flow(
            vec![inline_request("n1", "Get", "{{a}}{{a}}{{b}}", &[("H", "{{a}}")], Some("{{b}}"))],
            vec![],
        );
        let counting = Counting::default();
        let _ = validate_with_warnings(&f, &counting);
        assert_eq!(counting.variables.get(), 2);
    }

    /// n inline Request nodes that each use one unknown name.
    fn inline_requests(n: usize) -> Flow {
        flow(
            (0..n)
                .map(|i| inline_request(&format!("n{i}"), &format!("Req {i}"), &format!("{{{{v{i}}}}}"), &[], None))
                .collect(),
            vec![],
        )
    }

    #[test]
    fn unknown_variable_rule_is_linear() {
        assert_eq!(unknown_in(&inline_requests(20_000)).len(), 20_000);
        assert_linear(inline_requests, |f| {
            let _ = validate_with_warnings(f, &EverythingMissing);
        });
    }
}
