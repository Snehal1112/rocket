# Flow Tier 2 All Validation Errors Implementation Plan (Plan 4 of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Flow validation report every problem at once, not only the first (F-28). Save and lint list all structural errors, name nodes by label and wires by both ends, and the two weakest messages get better: an unknown exit lists the valid exits, and an `auth` wire error names what it connected.

**Architecture:** Task 1 adds `validate_all` / `validate_report` to `rocket-flow` by turning each rule in `validate.rs` into a collector that pushes to a `Vec`. `validate` becomes the first error of that list, so every caller and every existing test is unchanged. Task 2 moves `display_label` into a new shared `describe` module, improves the two reasons, and makes `FlowService::save` and `FlowService::lint` use the full list. The multi-error save text is one string: a header, one sentence per problem with a tag that holds its ids, and the existing combined `— node(s): ...; edge(s): ...` tail. Task 3 teaches `flow-wiring.ts` and `flow-issues.ts` to read that string.

**Tech Stack:** Rust (`rocket-flow`, `rocket-app`), React, TypeScript, Vitest, Biome. Run with `cargo -j4` and `yarn`.

**Spec:** `.claude/flow-roadmap.md` row F-28 (Tier 2). F-50 (a structured IPC error) is a later Tier 4 item and is NOT done here. The string stays parseable by the existing regexp, so F-50 can replace it later.

## Global Constraints

- Plan 3 runs before this plan and adds hard errors to `validate.rs` and `graph.rs` (Request `target_field` whitelist F-26, duplicate edge ids and Output wire count F-27). Every task starts with: re-read `crates/rocket-flow/src/validate.rs` and `crates/rocket-flow/src/graph.rs` as Plan 3 left them, then adapt the names and line numbers below. Each rule Plan 3 added is converted with the same pattern as the rules shown here (push to `errors`, never return early). Where a test below fixes an exact error list, run it, and if Plan 3 added a rule that also fires on the fixture, add that error to the expected list or change the fixture so only one rule fires. State the change in the commit body.
- The rule order in `collect_errors` stays exactly the order `validate` ran the rules before. That is what makes `validate` equal to the first error of `validate_all`. A new Plan 3 rule goes at the position Plan 3 gave it.
- Rust gates: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`. Never `--workspace` or `--all`. Never `cargo fmt` over a crate. Check only your own files with `rustfmt --check --edition 2021 <file>` and fix only the hunks you wrote.
- No unwrap or expect in production code. Tests may use `expect`.
- serde `camelCase` only on IPC DTOs. This plan adds no serde types.
- Frontend gates after the task: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern> --run --maxWorkers=2`. The commit hook runs the first two on staged files.
- Comments are short full sentences ending in a punctuation mark. No emojis.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths (`git add <paths> && git commit --only -m "..." -- <paths>`). Never `git add -A`.
- `rocket-flow` is not one of the crates that need the OpenCollection spec read, and no task touches collection or environment models, variable resolution or auth resolution, so the spec-reference line is not added.
- Do not change the single-error text shape. A flow with exactly one problem keeps `... — node(s): a; edge(s): e1`, as the existing `flow_service` tests expect.

## Review Focus

- `validate(flow)` equals `Err(validate_all(flow)[0])` on every invalid fixture, and `validate_all` is empty exactly when `validate` is `Ok` (Task 1).
- Duplicate and unknown ids are reported once each and never make a later rule panic or skip the other errors (Task 1).
- The list is capped at 50 and the report says how many were cut (Task 1, Task 2).
- The multi-error message still ends in a tail the old regexp parses to the union of ids. A hostile node label cannot forge that tail (Task 2, Task 3).
- The bad exit name and case match values are never echoed by lints, and the valid exits are listed (Task 2).
- Each problem lands on its own node or wire in the canvas, the toast stays short, and masking of case values still applies (Task 3).

---

### Task 1: `validate_all` in rocket-flow (F-28)

**Files:**
- Modify: `crates/rocket-flow/src/validate.rs` (header and `validate` ~1-90, every rule function ~90-395, tests module ~455-1473)
- Modify: `crates/rocket-flow/src/lib.rs` (the `pub use validate::validate;` line)

**Interfaces:**
- Consumes: `topological_sort(flow: &Flow) -> Result<Vec<String>, FlowGraphError>`, `reachable_from`, `FlowGraphError` (all in `graph.rs`), the `handle` constants.
- Produces in `validate.rs`:
  - `pub const MAX_VALIDATION_ERRORS: usize = 50;`
  - `pub struct ValidationReport { pub errors: Vec<FlowGraphError>, pub omitted: usize }` (`Debug, Clone, PartialEq`).
  - `pub fn validate(flow: &Flow) -> Result<Vec<String>, FlowGraphError>` (signature unchanged).
  - `pub fn validate_all(flow: &Flow) -> Vec<FlowGraphError>` returns at most 50 errors.
  - `pub fn validate_report(flow: &Flow) -> ValidationReport`.
- Order of errors: duplicate node ids (node order), unknown node ids (edge order, source before target), or, when none of those exist, a cycle. Then the rules in the original table order. Inside one rule the errors follow node order or edge order.

- [ ] **Step 1: Write the failing tests**

First re-read `validate.rs` and `graph.rs` as Plan 3 left them (see Global Constraints). In the `tests` module of `validate.rs`, after `new_error_variants_display_their_id_and_reason`, add:

```rust
    /// Short tags such as `node:if1` or `edge:e1`, to compare error lists.
    fn tags(errors: &[FlowGraphError]) -> Vec<String> {
        errors
            .iter()
            .map(|e| match e {
                FlowGraphError::Cycle { .. } => "cycle".to_string(),
                FlowGraphError::UnknownNode { node_id } => format!("unknown:{node_id}"),
                FlowGraphError::DuplicateNode { node_id } => format!("dup:{node_id}"),
                FlowGraphError::InvalidNode { node_id, .. } => format!("node:{node_id}"),
                FlowGraphError::InvalidEdge { edge_id, .. } => format!("edge:{edge_id}"),
            })
            .collect()
    }

    /// One flow per rule family. Each must be invalid.
    fn invalid_fixtures() -> Vec<(&'static str, Flow)> {
        let mut blank_condition = valid_if_flow();
        blank_condition.nodes[1] = if_node("if1", "   ");
        let mut unknown_exit = valid_if_flow();
        unknown_exit.edges[1].source_handle = "bogus".to_string();
        vec![
            (
                "cycle",
                flow(
                    vec![request("a"), request("b")],
                    vec![
                        edge("e1", "a", handle::RESULT, "b", "url"),
                        edge("e2", "b", handle::RESULT, "a", "url"),
                    ],
                ),
            ),
            ("if without input", flow(vec![if_node("if1", "true")], vec![])),
            (
                "switch with two inputs",
                flow(
                    vec![
                        request("a"),
                        request("b"),
                        switch_node("sw1", vec![("c1", "x")]),
                    ],
                    vec![
                        edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                        edge("e2", "b", handle::RESULT, "sw1", handle::INPUT),
                    ],
                ),
            ),
            (
                "input field on a request",
                flow(
                    vec![request("a"), request("b")],
                    vec![edge("e1", "a", handle::RESULT, "b", handle::INPUT)],
                ),
            ),
            (
                "wire into an input node",
                flow(
                    vec![request("a"), input("in1")],
                    vec![edge("e1", "a", handle::RESULT, "in1", handle::TRIGGER)],
                ),
            ),
            ("unknown exit", unknown_exit),
            (
                "duplicate case ids",
                flow(
                    vec![
                        request("a"),
                        switch_node("sw1", vec![("c1", "x"), ("c1", "y")]),
                    ],
                    vec![edge("e1", "a", handle::RESULT, "sw1", handle::INPUT)],
                ),
            ),
            ("blank condition", blank_condition),
            (
                "repeat-until interval",
                flow(
                    vec![polling_request(
                        "p",
                        crate::node::RepeatUntil {
                            interval_ms: 99,
                            ..Default::default()
                        },
                    )],
                    vec![],
                ),
            ),
            (
                "wait name",
                flow(vec![wait_node("w", "pay ment", 60_000)], vec![]),
            ),
            (
                "two auth wires",
                flow(
                    vec![
                        auth_node("a", bearer(), true),
                        auth_node("b", bearer(), false),
                        request("r"),
                    ],
                    vec![
                        edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                        edge("e2", "b", handle::RESULT, "r", handle::AUTH),
                    ],
                ),
            ),
            (
                "auth type none",
                flow(
                    vec![auth_node("a", rocket_shared::types::Auth::None, false)],
                    vec![],
                ),
            ),
            (
                "callback order",
                flow(
                    vec![
                        input("i"),
                        wait_node("w", "pay", 60_000),
                        inline_request("r", "https://x.test?cb={{callback.pay}}"),
                    ],
                    vec![edge("e1", "i", handle::RESULT, "r", "url")],
                ),
            ),
            (
                "earlier rule and later rule",
                flow(
                    vec![request("a"), request("b"), if_node("if1", "")],
                    vec![
                        edge("e1", "a", handle::RESULT, "b", handle::INPUT),
                        edge("e2", "a", handle::RESULT, "if1", handle::INPUT),
                    ],
                ),
            ),
            ("duplicate node", flow(vec![output("a"), output("a")], vec![])),
            (
                "unknown node",
                flow(
                    vec![request("a")],
                    vec![edge("e1", "a", handle::RESULT, "ghost", "url")],
                ),
            ),
        ]
    }

    #[test]
    fn validate_is_the_first_error_of_validate_all() {
        for (name, f) in invalid_fixtures() {
            let all = validate_all(&f);
            assert!(!all.is_empty(), "{name}: validate_all found nothing");
            assert_eq!(validate(&f), Err(all[0].clone()), "{name}");
        }
    }

    #[test]
    fn a_valid_flow_has_no_errors_and_keeps_its_order() {
        for f in [valid_if_flow(), valid_transform_flow()] {
            assert!(validate_all(&f).is_empty());
            assert_eq!(validate(&f).map(|o| o.len()), Ok(f.nodes.len()));
        }
    }

    #[test]
    fn validate_all_returns_every_error_in_rule_order() {
        // if1 has no input (V1) and a blank condition (V8). e1 wires into an
        // Input node, e2 leaves an exit that does not exist, and sw1 repeats a
        // case id.
        let f = flow(
            vec![
                request("a"),
                request("b"),
                input("in1"),
                if_node("if1", ""),
                switch_node("sw1", vec![("c1", "x"), ("c1", "y")]),
            ],
            vec![
                edge("e1", "a", handle::RESULT, "in1", "value"),
                edge("e2", "a", "bogus", "b", "url"),
                edge("e3", "a", handle::RESULT, "sw1", handle::INPUT),
            ],
        );
        assert_eq!(
            tags(&validate_all(&f)),
            vec!["node:if1", "edge:e1", "edge:e2", "node:sw1", "node:if1"]
        );
    }

    #[test]
    fn a_cycle_does_not_hide_the_other_errors() {
        let f = flow(
            vec![request("a"), request("b"), if_node("if1", "")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "url"),
                edge("e2", "b", handle::RESULT, "a", "url"),
            ],
        );
        assert_eq!(
            tags(&validate_all(&f)),
            vec!["cycle", "node:if1", "node:if1"]
        );
    }

    #[test]
    fn every_bad_wire_of_one_rule_is_reported() {
        let f = flow(
            vec![request("a"), request("b"), request("c")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "url"),
                edge("e2", "a", "bogus", "b", "body"),
                edge("e3", "a", "bogus", "c", "body"),
            ],
        );
        assert_eq!(tags(&validate_all(&f)), vec!["edge:e2", "edge:e3"]);
    }

    #[test]
    fn every_extra_auth_wire_is_reported() {
        let f = flow(
            vec![
                auth_node("a", bearer(), true),
                auth_node("b", bearer(), false),
                auth_node("c", bearer(), false),
                request("r"),
            ],
            vec![
                edge("e1", "a", handle::RESULT, "r", handle::AUTH),
                edge("e2", "b", handle::RESULT, "r", handle::AUTH),
                edge("e3", "c", handle::RESULT, "r", handle::AUTH),
            ],
        );
        assert_eq!(tags(&validate_all(&f)), vec!["edge:e2", "edge:e3"]);
    }

    #[test]
    fn duplicate_and_unknown_ids_are_reported_once_and_skip_their_wires() {
        let f = flow(
            vec![output("a"), output("a"), output("a")],
            vec![edge("e1", "a", handle::RESULT, "ghost", "value")],
        );
        assert_eq!(tags(&validate_all(&f)), vec!["dup:a", "unknown:ghost"]);
    }

    #[test]
    fn the_error_list_is_cut_at_the_limit_and_the_cut_is_counted() {
        let nodes: Vec<FlowNode> = (0..60)
            .map(|i| if_node(&format!("if{i}"), "true"))
            .collect();
        let f = flow(nodes, vec![]);
        let report = validate_report(&f);
        assert_eq!(report.errors.len(), MAX_VALIDATION_ERRORS);
        assert_eq!(report.omitted, 10);
        assert_eq!(validate_all(&f), report.errors);
        assert_eq!(validate(&f), Err(report.errors[0].clone()));
        let small = validate_report(&invalid_fixtures().remove(1).1);
        assert_eq!(small.omitted, 0);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow validate`
Expected: the crate does not compile. The errors are `cannot find function validate_all`, `cannot find function validate_report` and `cannot find value MAX_VALIDATION_ERRORS` in the tests module.

- [ ] **Step 3: Write the implementation**

Replace the module doc and everything from the module doc down to the end of `check_callback_order` (leave `mentions_callback` and the tests module as they are). Keep any rule Plan 3 added and convert it with the same pattern. The functions that do not change (`is_routing`, `takes_input`, `kind_name`, `invalid_node`, `first_duplicate`, `source_handle_exists`) stay as they are, so only the changed or new items are listed here.

Module doc and imports:

```rust
//! Save-time and load-time structural validation of a Flow graph. See spec
//! §7 (rules V1-V15). Rules run in table order. `validate_all` collects every
//! violation, `validate` returns the first one, so the same file always
//! yields the same error.

use crate::flow::{Flow, FlowEdge, FlowNode};
use crate::graph::{reachable_from, topological_sort, FlowGraphError};
use crate::handle;
use crate::node::{FlowNodeKind, RequestSource, CALLBACK_MAX_TIMEOUT_MS, CALLBACK_MIN_TIMEOUT_MS};
use std::collections::{HashMap, HashSet};

/// The most errors `validate_all` returns. A longer list is cut and counted.
pub const MAX_VALIDATION_ERRORS: usize = 50;

/// Every error found in a flow, cut at `MAX_VALIDATION_ERRORS`.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationReport {
    pub errors: Vec<FlowGraphError>,
    /// How many errors were found beyond the limit and left out.
    pub omitted: usize,
}

/// Validates `flow` and returns its node ids in topological order. The error
/// is the first one `validate_all` would report.
pub fn validate(flow: &Flow) -> Result<Vec<String>, FlowGraphError> {
    let (errors, order) = collect_errors(flow);
    match (errors.into_iter().next(), order) {
        (Some(first), _) => Err(first),
        (None, Some(order)) => Ok(order),
        (None, None) => topological_sort(flow),
    }
}

/// Every error in `flow`, in a fixed order, at most `MAX_VALIDATION_ERRORS`.
pub fn validate_all(flow: &Flow) -> Vec<FlowGraphError> {
    validate_report(flow).errors
}

/// Like `validate_all`, and also says how many errors were cut.
pub fn validate_report(flow: &Flow) -> ValidationReport {
    let (mut errors, _) = collect_errors(flow);
    let omitted = errors.len().saturating_sub(MAX_VALIDATION_ERRORS);
    errors.truncate(MAX_VALIDATION_ERRORS);
    ValidationReport { errors, omitted }
}

/// Runs every rule and returns all errors, plus the topological order when
/// the graph has no id or cycle error. Ids come first because the later
/// rules look nodes up by id. They cannot panic on a bad id, so they still
/// run: an edge with an unknown end is skipped, and a repeated id means the
/// first node with that id is the one the rules see.
fn collect_errors(flow: &Flow) -> (Vec<FlowGraphError>, Option<Vec<String>>) {
    let mut errors = reference_errors(flow);
    let mut order = None;
    // A cycle is only looked for on a graph whose ids are sound.
    if errors.is_empty() {
        match topological_sort(flow) {
            Ok(sorted) => order = Some(sorted),
            Err(error) => errors.push(error),
        }
    }
    let mut kinds: HashMap<&str, &FlowNodeKind> = HashMap::with_capacity(flow.nodes.len());
    for node in &flow.nodes {
        kinds.entry(node.id.as_str()).or_insert(&node.kind);
    }

    check_routing_inputs(flow, &mut errors);
    check_edges(flow, &kinds, &mut errors, |edge, target, _| {
        (edge.target_field == handle::INPUT && !takes_input(target)).then(|| {
            format!(
                "only If, Switch and Transform nodes have an '{}' input",
                handle::INPUT
            )
        })
    });
    check_edges(flow, &kinds, &mut errors, |edge, target, _| {
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
    });
    check_edges(flow, &kinds, &mut errors, |_, target, _| {
        matches!(target, FlowNodeKind::Input { .. })
            .then(|| "Input nodes cannot receive wires".to_string())
    });
    check_edges(flow, &kinds, &mut errors, |_, target, _| {
        matches!(target, FlowNodeKind::Auth { .. })
            .then(|| "Auth nodes cannot receive wires".to_string())
    });
    check_edges(flow, &kinds, &mut errors, |edge, target, source| {
        let from_auth = matches!(source, FlowNodeKind::Auth { .. });
        let into_request = matches!(target, FlowNodeKind::Request { .. });
        (edge.target_field == handle::AUTH && !(from_auth && into_request)).then(|| {
            format!(
                "an '{}' wire must go from an Auth node into a Request node",
                handle::AUTH
            )
        })
    });
    check_single_auth_wire(flow, &mut errors);
    check_edges(flow, &kinds, &mut errors, |edge, target, _| {
        (matches!(target, FlowNodeKind::WaitForCallback { .. })
            && edge.target_field != handle::TRIGGER)
            .then(|| {
                format!(
                    "Wait for callback nodes only have a '{}' input",
                    handle::TRIGGER
                )
            })
    });
    check_edges(flow, &kinds, &mut errors, |edge, _, source| {
        (!source_handle_exists(source, &edge.source_handle))
            .then(|| format!("the source node has no exit named '{}'", edge.source_handle))
    });
    check_switch_cases(flow, &mut errors, |cases| {
        first_duplicate(cases.iter().map(|c| c.id.as_str()))
            .map(|id| format!("more than one case has id '{id}'"))
    });
    check_switch_cases(flow, &mut errors, |cases| {
        first_duplicate(cases.iter().map(|c| c.matches.as_str()))
            .map(|m| format!("more than one case matches '{m}'"))
    });
    check_expressions(flow, &mut errors);
    check_repeat_until(flow, &mut errors);
    check_wait_nodes(flow, &mut errors);
    check_callback_order(flow, &mut errors);
    check_auth_nodes(flow, &mut errors);

    (errors, order)
}

/// A node id used twice, then an edge end that names no node. Each id is
/// reported once, in the order `topological_sort` would have met it.
fn reference_errors(flow: &Flow) -> Vec<FlowGraphError> {
    let mut errors = Vec::new();
    let mut seen: HashSet<&str> = HashSet::with_capacity(flow.nodes.len());
    let mut repeated: HashSet<&str> = HashSet::new();
    for node in &flow.nodes {
        if !seen.insert(node.id.as_str()) && repeated.insert(node.id.as_str()) {
            errors.push(FlowGraphError::DuplicateNode {
                node_id: node.id.clone(),
            });
        }
    }
    let mut missing: HashSet<&str> = HashSet::new();
    for edge in &flow.edges {
        for id in [edge.source_node_id.as_str(), edge.target_node_id.as_str()] {
            if !seen.contains(id) && missing.insert(id) {
                errors.push(FlowGraphError::UnknownNode {
                    node_id: id.to_string(),
                });
            }
        }
    }
    errors
}
```

Then the rule functions, each pushing instead of returning early:

```rust
/// V1: an If, Switch or Transform node has exactly one incoming edge, into `input`.
fn check_routing_inputs(flow: &Flow, errors: &mut Vec<FlowGraphError>) {
    for node in flow.nodes.iter().filter(|n| takes_input(&n.kind)) {
        let incoming: Vec<&FlowEdge> = flow
            .edges
            .iter()
            .filter(|e| e.target_node_id == node.id)
            .collect();
        let name = kind_name(&node.kind);
        match incoming.as_slice() {
            [edge] if edge.target_field == handle::INPUT => {}
            [edge] => errors.push(invalid_node(
                node,
                format!(
                    "the {name} node's wire must target '{}', not '{}'",
                    handle::INPUT,
                    edge.target_field
                ),
            )),
            _ => errors.push(invalid_node(
                node,
                format!(
                    "the {name} node needs exactly one input wire, found {}",
                    incoming.len()
                ),
            )),
        }
    }
}

/// Runs one edge rule over every edge in file order. `rule` receives the
/// edge, its target kind and its source kind, and returns a reason when the
/// edge breaks the rule. An edge with an unknown end is skipped, because
/// `reference_errors` already reported it.
fn check_edges<F>(
    flow: &Flow,
    kinds: &HashMap<&str, &FlowNodeKind>,
    errors: &mut Vec<FlowGraphError>,
    rule: F,
) where
    F: Fn(&FlowEdge, &FlowNodeKind, &FlowNodeKind) -> Option<String>,
{
    for edge in &flow.edges {
        let (Some(target), Some(source)) = (
            kinds.get(edge.target_node_id.as_str()).copied(),
            kinds.get(edge.source_node_id.as_str()).copied(),
        ) else {
            continue;
        };
        if let Some(reason) = rule(edge, target, source) {
            errors.push(FlowGraphError::InvalidEdge {
                edge_id: edge.id.clone(),
                reason,
            });
        }
    }
}

/// V14: a Request node has at most one incoming `auth` wire. Every later
/// edge in file order is reported.
fn check_single_auth_wire(flow: &Flow, errors: &mut Vec<FlowGraphError>) {
    let mut seen: HashSet<&str> = HashSet::new();
    for edge in flow.edges.iter().filter(|e| e.target_field == handle::AUTH) {
        if !seen.insert(edge.target_node_id.as_str()) {
            errors.push(FlowGraphError::InvalidEdge {
                edge_id: edge.id.clone(),
                reason: format!("a Request node can take only one '{}' wire", handle::AUTH),
            });
        }
    }
}

/// Runs one Switch-case rule (V6 or V7) over every Switch node in file order.
fn check_switch_cases<F>(flow: &Flow, errors: &mut Vec<FlowGraphError>, rule: F)
where
    F: Fn(&[crate::node::SwitchCase]) -> Option<String>,
{
    for node in &flow.nodes {
        if let FlowNodeKind::Switch { cases, .. } = &node.kind {
            if let Some(reason) = rule(cases) {
                errors.push(invalid_node(node, reason));
            }
        }
    }
}

/// V8: an If condition, a Switch value and a Transform script must not be blank.
fn check_expressions(flow: &Flow, errors: &mut Vec<FlowGraphError>) {
    for node in &flow.nodes {
        let (expression, field) = match &node.kind {
            FlowNodeKind::If { condition, .. } => (condition, "condition"),
            FlowNodeKind::Switch { value, .. } => (value, "value"),
            FlowNodeKind::Transform { script, .. } => (script, "script"),
            _ => continue,
        };
        if expression.trim().is_empty() {
            errors.push(invalid_node(
                node,
                format!("the {} node's {field} is empty", kind_name(&node.kind)),
            ));
        }
    }
}
```

`check_repeat_until`: change the signature to `fn check_repeat_until(flow: &Flow, errors: &mut Vec<FlowGraphError>)`, keep the whole `reason` if-chain, and replace the tail `if let Some(reason) = reason { return Err(invalid_node(node, reason)); }` with `if let Some(reason) = reason { errors.push(invalid_node(node, reason)); }`. Remove the final `Ok(())`.

```rust
/// V13: an Auth node holds a concrete auth type, and at most one Auth node
/// applies to inherited auth.
fn check_auth_nodes(flow: &Flow, errors: &mut Vec<FlowGraphError>) {
    use rocket_shared::types::Auth;
    let mut applying = false;
    for node in &flow.nodes {
        let FlowNodeKind::Auth {
            auth,
            apply_to_inherit,
            ..
        } = &node.kind
        else {
            continue;
        };
        if matches!(auth, Auth::None | Auth::Inherit) {
            errors.push(invalid_node(
                node,
                "the Auth node needs an auth type other than none or inherit".to_string(),
            ));
        }
        if *apply_to_inherit {
            if applying {
                errors.push(invalid_node(
                    node,
                    "only one Auth node can apply to inherited auth; turn this one or the other off"
                        .to_string(),
                ));
            }
            applying = true;
        }
    }
}

/// V10: a Wait for callback node has a usable, unique name, a timeout in
/// range, and no blank `accept_when`.
fn check_wait_nodes(flow: &Flow, errors: &mut Vec<FlowGraphError>) {
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
            errors.push(invalid_node(
                node,
                format!("the Wait for callback name '{name}' must use only letters, digits and _"),
            ));
        } else if !seen.insert(name.as_str()) {
            errors.push(invalid_node(
                node,
                format!("more than one Wait for callback node is named '{name}'"),
            ));
        }
        if !(CALLBACK_MIN_TIMEOUT_MS..=CALLBACK_MAX_TIMEOUT_MS).contains(timeout_ms) {
            errors.push(invalid_node(
                node,
                format!(
                    "the Wait for callback timeout must be between {CALLBACK_MIN_TIMEOUT_MS} and {CALLBACK_MAX_TIMEOUT_MS} ms"
                ),
            ));
        }
        if accept_when.as_deref().is_some_and(|s| s.trim().is_empty()) {
            errors.push(invalid_node(
                node,
                "the Wait for callback node's accept_when is empty".to_string(),
            ));
        }
    }
}
```

`check_callback_order`: change the signature to `fn check_callback_order(flow: &Flow, errors: &mut Vec<FlowGraphError>)`, keep the loops and the `texts` iterator, replace `return Err(invalid_node(wait, format!(...)))` with `errors.push(invalid_node(wait, format!(...)));` (same message), and drop the final `Ok(())`.

Delete `kind_of` (no longer used). In `lib.rs` change the export to:

```rust
pub use validate::{
    validate, validate_all, validate_report, ValidationReport, MAX_VALIDATION_ERRORS,
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow validate`
Expected: PASS, including every old `v1_` to `v15_`, `vt_`, `earlier_rule_wins_over_later_rule` and `earlier_edge_wins_within_the_same_rule` test, which still call `validate`. If `validate_all_returns_every_error_in_rule_order` fails only because Plan 3 added a rule that also fires on the fixture, adapt the list as the Global Constraints say.

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS (lint tests call `validate` too).

Run: `cargo check -j4 -p rocket-app` and `rustfmt --check --edition 2021 crates/rocket-flow/src/validate.rs` (fix only your own hunks).
Expected: no errors.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then commit with explicit paths:
`git add crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lib.rs && git commit --only -m "feat(flow): collect every validation error with validate_all" -- crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lib.rs`

---

### Task 2: Better messages and an all-errors save and lint (F-28)

**Files:**
- Create: `crates/rocket-flow/src/describe.rs` (~110 lines plus tests)
- Modify: `crates/rocket-flow/src/lib.rs` (add `pub mod describe;`)
- Modify: `crates/rocket-flow/src/validate.rs` (exit rule and auth rule in `collect_errors`, tests)
- Modify: `crates/rocket-flow/src/lint.rs` (`display_label` ~116-133 removed, imports ~9-14, `sentence` ~366-392, `graph_error_lints` InvalidEdge branch ~315-317, `FIXED_REASONS` ~341-354, tests ~711 and ~1191)
- Modify: `crates/rocket-app/src/flow_service.rs` (imports 1-2, `save` 43-46, `lint` 52-62, `graph_error_message` 66-90, tests)

**Interfaces:**
- Consumes: `validate_report`, `ValidationReport` (Task 1), `kind_name` (`validate.rs`, `pub(crate)`), `FlowGraphError`, `graph_error_lints`.
- Produces in `rocket_flow::describe`:
  - `pub fn display_label(node: &FlowNode) -> &str` (moved from `lint.rs`, same body).
  - `pub fn exits_text(kind: &FlowNodeKind) -> String`, for example `its exits are 'true' and 'false'`.
  - `pub fn wire_ends<'a>(flow: &'a Flow, edge_id: &str) -> Option<(&'a str, &'a str)>` returns the source and target labels.
  - `pub fn source_kind<'a>(flow: &'a Flow, edge_id: &str) -> Option<&'a FlowNodeKind>`.
- Produces in `flow_service.rs` (private): `graph_errors_message(flow: &Flow, report: &ValidationReport) -> String`.
- The message format (documented here, parsed by Task 3):
  - One problem: unchanged, `flow is invalid: <text> — node(s): <ids>; edge(s): <ids>`. A cycle, an unknown node and a duplicate node keep their old text. The text of an invalid node is now `'<label>': <reason>` and of an invalid wire `wire from '<A>' to '<B>': <reason>`.
  - Two or more problems, or any cut: `flow has <total> problems:` (or `flow has <total> problems, showing the first <n>:` when some were cut), then for each problem ` <text> [nodes <ids>; edges <ids>].`, then ` — node(s): <all ids>; edge(s): <all ids>`. Ids are joined with `, `. The tags never contain the literal `node(s):`, so the old regexp finds only the tail.
  - Labels are cleaned: `;`, `[`, `]` and control characters are dropped, `(s):` becomes `(s)`, and the label is cut at 60 characters. A label cannot forge a tail or a tag.

- [ ] **Step 1: Write the failing tests**

Re-read `validate.rs`, `graph.rs`, `lint.rs` and `flow_service.rs` as Plan 3 left them. Then add the tests.

In `crates/rocket-flow/src/validate.rs` tests, after `invalid_node_reason`, add a helper and tests:

```rust
    fn invalid_edge_reason(result: Result<Vec<String>, FlowGraphError>) -> String {
        match result {
            Err(FlowGraphError::InvalidEdge { reason, .. }) => reason,
            other => panic!("expected InvalidEdge, got {other:?}"),
        }
    }

    #[test]
    fn v5_unknown_exit_reason_lists_the_valid_exits() {
        let mut if_flow = valid_if_flow();
        if_flow.edges[1].source_handle = "bogus".to_string();
        assert_eq!(
            invalid_edge_reason(validate(&if_flow)),
            "the source node has no exit named 'bogus'; its exits are 'true' and 'false'"
        );
        let request_flow = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", handle::TRUE, "b", "url")],
        );
        assert_eq!(
            invalid_edge_reason(validate(&request_flow)),
            "the source node has no exit named 'true'; its only exit is 'result'"
        );
        let output_flow = flow(
            vec![output("out"), request("b")],
            vec![edge("e1", "out", handle::RESULT, "b", "url")],
        );
        assert_eq!(
            invalid_edge_reason(validate(&output_flow)),
            "the source node has no exit named 'result'; an Output node has no exits"
        );
        let switch_flow = flow(
            vec![
                request("a"),
                switch_node("sw1", vec![("c1", "x"), ("c2", "y")]),
                request("b"),
            ],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "sw1", &handle::case_handle("gone"), "b", handle::TRIGGER),
            ],
        );
        assert_eq!(
            invalid_edge_reason(validate(&switch_flow)),
            "the source node has no exit named 'case:gone'; its exits are 'default' and the case exits 'c1', 'c2'"
        );
    }

    #[test]
    fn an_auth_wire_reason_names_what_it_connected() {
        let from_request = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", handle::RESULT, "b", handle::AUTH)],
        );
        assert_eq!(
            invalid_edge_reason(validate(&from_request)),
            "an 'auth' wire must go from an Auth node into a Request node, not from Request to Request"
        );
        let into_output = flow(
            vec![auth_node("a", bearer(), true), output("o")],
            vec![edge("e1", "a", handle::RESULT, "o", handle::AUTH)],
        );
        assert_eq!(
            invalid_edge_reason(validate(&into_output)),
            "an 'auth' wire must go from an Auth node into a Request node, not from Auth to Output"
        );
    }
```

Create `crates/rocket-flow/src/describe.rs` with its tests first (the functions come in Step 3, so the file will not compile yet). Write only this test module now, at the bottom of the new file:

```rust
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
```

In `crates/rocket-flow/src/lint.rs` tests: next to `use crate::validate::validate;` (~line 711) add `use crate::validate::validate_all;`. Add these tests after `an_invalid_edge_carries_the_edge_id`:

```rust
    /// One lint per error, for every error `validate_all` found.
    fn all_graph_lints(f: &Flow) -> Vec<FlowLint> {
        validate_all(f)
            .iter()
            .flat_map(|error| graph_error_lints(f, error))
            .collect()
    }

    #[test]
    fn every_structural_error_becomes_its_own_lint() {
        let f = flow(
            vec![request("a"), request("b"), if_node("if1", "Check")],
            vec![edge("e1", "a", handle::RESULT, "b", handle::AUTH)],
        );
        let lints = all_graph_lints(&f);
        assert_eq!(
            keys(&lints),
            vec![
                (Some("if1"), None, INVALID_GRAPH),
                (None, Some("e1"), INVALID_GRAPH),
            ]
        );
        assert_eq!(
            lints[0].message,
            "'Check': The If node needs exactly one input wire, found 0."
        );
    }

    #[test]
    fn a_wire_error_names_both_ends_by_label() {
        let f = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", handle::RESULT, "b", handle::AUTH)],
        );
        let lints = graph_lints(&f);
        assert_eq!(
            lints[0].message,
            "Wire from 'a' to 'b': An 'auth' wire must go from an Auth node into a Request node."
        );
    }

    #[test]
    fn an_unknown_exit_lint_lists_the_valid_exits_and_hides_the_bad_name() {
        let f = flow(
            vec![input("in1"), if_node("if1", "Check"), output("out")],
            vec![
                edge("e0", "in1", handle::RESULT, "if1", handle::INPUT),
                edge("e1", "if1", CANARY, "out", "value"),
            ],
        );
        let lints = graph_lints(&f);
        assert_eq!(keys(&lints), vec![(None, Some("e1"), INVALID_GRAPH)]);
        assert_eq!(
            lints[0].message,
            "Wire from 'Check' to 'out': 'Check' does not have that exit; its exits are 'true' and 'false'."
        );
        assert_no_canary(&lints);
    }
```

In the table of `mapped_reasons_read_as_fixed_sentences`, add two rows after the existing "the source node has no exit named 'x'" row and after the existing auth row:

```rust
            (
                "the source node has no exit named 'x'; its exits are 'true' and 'false'",
                "The wire leaves an exit that the source node does not have.",
            ),
            (
                "an 'auth' wire must go from an Auth node into a Request node, not from Request to Output",
                "An 'auth' wire must go from an Auth node into a Request node.",
            ),
```

In `crates/rocket-app/src/flow_service.rs` tests, after `save_rejects_a_bad_edge_and_names_it_in_the_tail`, add:

```rust
    fn login_request() -> FlowNode {
        node_of(
            "login",
            FlowNodeKind::Request {
                debug: false,
                repeat_until: None,
                label: "Login".to_string(),
                source: rocket_flow::RequestSource::Saved {
                    request_path: "login.yml".to_string(),
                },
            },
        )
    }

    fn if_without_input(id: &str, label: &str) -> FlowNode {
        node_of(
            id,
            FlowNodeKind::If {
                label: label.to_string(),
                condition: "response.status === 200".to_string(),
            },
        )
    }

    /// Two problems: `if1` has no input, and `e9` leaves an exit that `login` lacks.
    fn two_problem_flow() -> Flow {
        Flow {
            name: "Two Problems".to_string(),
            nodes: vec![
                login_request(),
                node_of(
                    "out",
                    FlowNodeKind::Output {
                        label: "Result".to_string(),
                    },
                ),
                if_without_input("if1", "Logged in?"),
            ],
            edges: vec![wire("e9", "login", "true", "out", "value")],
            callback_host: None,
        }
    }

    #[test]
    fn save_reports_every_problem_in_one_message() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let message = svc
            .save("demo", two_problem_flow())
            .expect_err("an invalid flow must be rejected")
            .to_string();
        assert_eq!(
            message,
            "Invalid input: flow has 2 problems: \
             'Logged in?': the If node needs exactly one input wire, found 0 [nodes if1; edges ]. \
             wire from 'Login' to 'Result': the source node has no exit named 'true'; its only exit is 'result' [nodes ; edges e9]. \
             — node(s): if1; edge(s): e9"
        );
    }

    #[test]
    fn a_hostile_label_cannot_forge_the_id_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Hostile".to_string(),
            nodes: vec![if_without_input("if1", "oops; node(s): evil [x]")],
            edges: vec![],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an invalid flow must be rejected")
            .to_string();
        assert_eq!(message.matches("node(s):").count(), 1, "got: {message}");
        assert!(
            message.ends_with("node(s): if1; edge(s): "),
            "got: {message}"
        );
    }

    #[test]
    fn a_long_error_list_is_cut_and_the_message_says_how_many() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Many".to_string(),
            nodes: (0..60)
                .map(|i| if_without_input(&format!("if{i}"), "Check"))
                .collect(),
            edges: vec![],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an invalid flow must be rejected")
            .to_string();
        assert!(
            message.starts_with("Invalid input: flow has 60 problems, showing the first 50:"),
            "got: {message}"
        );
        let tail = message
            .rsplit("node(s): ")
            .next()
            .unwrap_or_default()
            .to_string();
        let node_list = tail.split(';').next().unwrap_or_default();
        assert_eq!(node_list.split(", ").count(), 50, "got: {message}");
    }

    #[test]
    fn lint_reports_every_structural_error_by_label() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let lints = svc.lint("demo", &two_problem_flow());
        let errors: Vec<&FlowLint> = lints.iter().filter(|l| l.code == INVALID_GRAPH).collect();
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].node_id.as_deref(), Some("if1"));
        assert!(errors[0].message.starts_with("'Logged in?': "));
        assert_eq!(errors[1].edge_id.as_deref(), Some("e9"));
        assert!(errors[1].message.starts_with("Wire from 'Login' to 'Result': "));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow`
Expected: the crate does not compile (`describe` has no functions yet, and `describe` is not declared in `lib.rs`). After you add `pub mod describe;` to `lib.rs` and only the test module exists in `describe.rs`, the errors are `cannot find function display_label`, `wire_ends`, `source_kind` and `exits_text`.

Run: `cargo test -j4 -p rocket-app flow_service`
Expected: FAIL. `save_reports_every_problem_in_one_message` gets the old single-error text, the hostile label test finds two `node(s):`, the cut test finds no header, and the lint test finds one error lint, not two.

- [ ] **Step 3: Write the implementation**

Add `pub mod describe;` to `crates/rocket-flow/src/lib.rs` (after `pub mod flow;`).

Write the top of `crates/rocket-flow/src/describe.rs`, above the tests module from Step 1:

```rust
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
                let names: Vec<String> =
                    cases.iter().map(|c| format!("'{}'", case_name(c))).collect();
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
```

`validate.rs`: add `use crate::describe::exits_text;` and change the two rules in `collect_errors`:

```rust
    check_edges(flow, &kinds, &mut errors, |edge, target, source| {
        let from_auth = matches!(source, FlowNodeKind::Auth { .. });
        let into_request = matches!(target, FlowNodeKind::Request { .. });
        (edge.target_field == handle::AUTH && !(from_auth && into_request)).then(|| {
            format!(
                "an '{}' wire must go from an Auth node into a Request node, not from {} to {}",
                handle::AUTH,
                kind_name(source),
                kind_name(target)
            )
        })
    });
```

```rust
    check_edges(flow, &kinds, &mut errors, |edge, _, source| {
        (!source_handle_exists(source, &edge.source_handle)).then(|| {
            format!(
                "the source node has no exit named '{}'; {}",
                edge.source_handle,
                exits_text(source)
            )
        })
    });
```

`lint.rs`:

1. Delete the `display_label` function (the doc line and the whole function, ~116-133) and the line `use crate::validate::kind_name;`. Add `use crate::describe::{display_label, exits_text, source_kind, wire_ends};`.
2. In `FIXED_REASONS` change the type to `[&str; 11]` and delete the line `"an 'auth' wire must go from an Auth node into a Request node",`. The reason now ends in kind names, so `sentence` matches it by prefix.
3. In `sentence`, add a branch after the `the source node has no exit named` branch:

```rust
    } else if reason.starts_with("an 'auth' wire must go from an Auth node into a Request node") {
        Some("An 'auth' wire must go from an Auth node into a Request node.".to_string())
```

4. Add after `sentence`:

```rust
/// The start of the reason `validate` gives for an exit that does not exist.
const NO_EXIT: &str = "the source node has no exit named";

/// The message of an invalid wire. It names both ends by label when the wire
/// and its nodes are known. It never quotes the wire's own exit name, and it
/// lists the exits the source node does have.
fn edge_message(flow: &Flow, edge_id: &str, reason: &str) -> String {
    let Some((from, to)) = wire_ends(flow, edge_id) else {
        return sentence(reason);
    };
    let body = match source_kind(flow, edge_id) {
        Some(kind) if reason.trim().starts_with(NO_EXIT) => {
            format!("'{from}' does not have that exit; {}.", exits_text(kind))
        }
        _ => sentence(reason),
    };
    format!("Wire from '{from}' to '{to}': {body}")
}
```

5. In `graph_error_lints` replace the `InvalidEdge` arm with:

```rust
        FlowGraphError::InvalidEdge { edge_id, reason } => {
            let message = edge_message(flow, edge_id, reason);
            vec![lint(None, Some(edge_id.as_str()), message, FIX)]
        }
```

`flow_service.rs`. Change the imports:

```rust
use rocket_flow::describe::{display_label, wire_ends};
use rocket_flow::lint::{graph_error_lints, validate_with_warnings, FlowLint, NoLintContext};
use rocket_flow::{validate_report, Flow, FlowGraphError, FlowNode, FlowRepository, ValidationReport};
use rocket_shared::error::{DomainError, DomainResult};
use std::collections::HashSet;
```

Replace `save` and `lint`:

```rust
    pub fn save(&self, collection: &str, flow: Flow) -> DomainResult<()> {
        let report = validate_report(&flow);
        if !report.errors.is_empty() {
            return Err(DomainError::InvalidInput(graph_errors_message(
                &flow, &report,
            )));
        }
        self.flow_repo.save(collection, &flow)
    }

    /// Lints `flow` as given, not the saved file, and never fails. Every
    /// structural `validate` failure comes first, as `invalid_graph` error
    /// lints that carry their node or edge ids, in the order `validate_all`
    /// found them. Then come the warnings of `validate_with_warnings`. Lints
    /// never block save or run.
    pub fn lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint> {
        // F-21 and F-22 build a context from `collection` here, for saved
        // requests and known variables. Until then no lint needs one.
        let _ = collection;
        let report = validate_report(flow);
        let mut lints: Vec<FlowLint> = report
            .errors
            .iter()
            .flat_map(|error| graph_error_lints(flow, error))
            .collect();
        lints.extend(validate_with_warnings(flow, &NoLintContext));
        lints
    }
```

Replace `graph_error_message` and its doc with:

```rust
/// Most characters of a label or id that go into a save message.
const MAX_MESSAGE_CHARS: usize = 60;

/// Text from the flow file, made safe to put in the save message. It drops
/// what could break a `[nodes ...; edges ...]` tag or fake the final
/// `node(s):` list, and it cuts long text.
fn message_safe(text: &str) -> String {
    let kept: String = text
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ';' | '[' | ']'))
        .collect();
    kept.replace("(s):", "(s)")
        .chars()
        .take(MAX_MESSAGE_CHARS)
        .collect()
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

fn describe_problem(flow: &Flow, error: &FlowGraphError) -> Problem {
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
            node_ids.join(", "),
            edge_ids.join(", ")
        ),
        FlowGraphError::UnknownNode { .. } | FlowGraphError::DuplicateNode { .. } => problem.text,
        FlowGraphError::InvalidNode { node_id, .. } => {
            format!("flow is invalid: {} — node(s): {node_id}; edge(s): ", problem.text)
        }
        FlowGraphError::InvalidEdge { edge_id, .. } => {
            format!("flow is invalid: {} — node(s): ; edge(s): {edge_id}", problem.text)
        }
    }
}

/// Builds the save-error text. Every message that names graph elements ends
/// with "node(s): <ids>; edge(s): <ids>", which the canvas parses to
/// highlight them in red. A flow with several problems lists each one as
/// "<text> [nodes <ids>; edges <ids>]." after a "flow has N problems" header,
/// so the canvas can mark each problem on its own node or wire. The final
/// "node(s):" list holds the ids of every listed problem, once each.
fn graph_errors_message(flow: &Flow, report: &ValidationReport) -> String {
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
        format!("flow has {total} problems, showing the first {}:", problems.len())
    };
    let mut all_nodes: Vec<&str> = Vec::new();
    let mut all_edges: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for problem in &problems {
        message.push_str(&format!(
            " {} [nodes {}; edges {}].",
            problem.text,
            problem.node_ids.join(", "),
            problem.edge_ids.join(", ")
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
        all_nodes.join(", "),
        all_edges.join(", ")
    ));
    message
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS, including the old lint tests (`a_bad_wire_target_or_exit_name_is_never_echoed`, `mapped_reasons_read_as_fixed_sentences`).

Run: `cargo test -j4 -p rocket-app flow_service`
Expected: PASS, including the old `save_rejects_*` tests that pin the single-error tail.

Run: `cargo check -j4 -p rocket-flow`, `cargo check -j4 -p rocket-app`, and `rustfmt --check --edition 2021` on `describe.rs` (fix only your own hunks in the other files).
Expected: no errors. If the `if let ([only], 0) = ...` pattern is rejected, use `if report.omitted == 0 && report.errors.len() == 1 { if let Some(only) = report.errors.first() { return single_problem_message(flow, only); } }`.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then commit with explicit paths:
`git add crates/rocket-flow/src/describe.rs crates/rocket-flow/src/lib.rs crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs crates/rocket-app/src/flow_service.rs && git commit --only -m "feat(flow): report all validation errors by label on save and lint" -- crates/rocket-flow/src/describe.rs crates/rocket-flow/src/lib.rs crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs crates/rocket-app/src/flow_service.rs`

---

### Task 3: Read a multi-problem save message in the canvas (F-28)

**Files:**
- Modify: `src/lib/flow-wiring.ts` (after `parseGraphErrorMessage`, ~155)
- Modify: `src/lib/flow-issues.ts` (imports ~1-7, `cleanSaveMessage` ~237-255, `saveIssues` ~257-274)
- Modify: `src/components/flow/FlowPane.tsx` (import ~20, panel `saveError` ~754-758)
- Test: `src/lib/__tests__/flow-wiring.test.ts` (after the `parseGraphErrorMessage` describe, ~276)
- Test: `src/lib/__tests__/flow-issues.test.ts` (after the `cleanSaveMessage` describe, ~327)

**Interfaces:**
- Consumes: the message format from Task 2, `splitIds` and `parseGraphErrorMessage` (`flow-wiring.ts`), `QUOTED_CASE_VALUE` and `cleanSaveMessage` (`flow-issues.ts`), `computeFlowIssues`, `FlowIssue`.
- Produces in `flow-wiring.ts`: `GraphProblem { text: string; nodeIds: string[]; edgeIds: string[] }`, `GraphProblems { total: number; problems: GraphProblem[] }`, `parseGraphProblems(message: string): GraphProblems | null`. It returns `null` for any message that is not the multi-problem shape.
- Produces in `flow-issues.ts`: `saveMessageForNode(message: string, nodeId: string): string`. `cleanSaveMessage` keeps its signature. For a multi-problem message it returns `Flow has N problems.` followed by the first three problems and `And K more.` when more exist.
- `parseGraphErrorMessage` is not changed. On a multi-problem message it returns the union of ids, because the tags never contain `node(s):`.

- [ ] **Step 1: Write the failing tests**

Re-read `flow-wiring.ts` and `flow-issues.ts` as Tier 1 and the earlier plans left them, then adapt. The strings below are the exact text `FlowService::save` produces in Task 2 (`save_reports_every_problem_in_one_message`), with the `Invalid input: ` prefix that `DomainError::InvalidInput` adds.

In `src/lib/__tests__/flow-wiring.test.ts`, extend the `../flow-wiring` import with `parseGraphProblems` and add after the `parseGraphErrorMessage` describe:

```ts
const MULTI_MESSAGE =
  "Invalid input: flow has 2 problems: 'Logged in?': the If node needs exactly one input wire, found 0 [nodes if1; edges ]. wire from 'Login' to 'Result': the source node has no exit named 'true'; its only exit is 'result' [nodes ; edges e9]. — node(s): if1; edge(s): e9";

describe('parseGraphProblems', () => {
  it('splits a multi-problem message into one problem per sentence', () => {
    expect(parseGraphProblems(MULTI_MESSAGE)).toEqual({
      total: 2,
      problems: [
        {
          text: "'Logged in?': the If node needs exactly one input wire, found 0",
          nodeIds: ['if1'],
          edgeIds: [],
        },
        {
          text: "wire from 'Login' to 'Result': the source node has no exit named 'true'; its only exit is 'result'",
          nodeIds: [],
          edgeIds: ['e9'],
        },
      ],
    });
  });

  it('reads the total of a message whose list was cut', () => {
    const cut =
      "Invalid input: flow has 53 problems, showing the first 2: 'Check': the If node's condition is empty [nodes if1; edges ]. 'Pick': the Switch node's value is empty [nodes sw; edges ]. — node(s): if1, sw; edge(s): ";
    const parsed = parseGraphProblems(cut);
    expect(parsed?.total).toBe(53);
    expect(parsed?.problems.map((p) => p.nodeIds)).toEqual([['if1'], ['sw']]);
  });

  it('keeps several ids of one problem, such as a cycle', () => {
    const cycle =
      'Invalid input: flow has 2 problems: flow contains a cycle [nodes a, b; edges e1, e2]. \'Check\': the If node\'s condition is empty [nodes if1; edges ]. — node(s): a, b, if1; edge(s): e1, e2';
    expect(parseGraphProblems(cycle)?.problems[0]).toEqual({
      text: 'flow contains a cycle',
      nodeIds: ['a', 'b'],
      edgeIds: ['e1', 'e2'],
    });
  });

  it('returns null for a single-problem or unrelated message', () => {
    expect(
      parseGraphProblems(
        'Invalid input: flow is invalid: the If node needs exactly one input wire, found 0 — node(s): b; edge(s): ',
      ),
    ).toBeNull();
    expect(parseGraphProblems('disk full')).toBeNull();
  });

  it('leaves the old id parser reading the union of ids', () => {
    expect(parseGraphErrorMessage(MULTI_MESSAGE)).toEqual({
      nodeIds: ['if1'],
      edgeIds: ['e9'],
    });
  });
});
```

In `src/lib/__tests__/flow-issues.test.ts`, extend the `../flow-issues` import with `saveMessageForNode`, and add after the `cleanSaveMessage` describe:

```ts
const MULTI_MESSAGE =
  "Invalid input: flow has 2 problems: 'Logged in?': the If node needs exactly one input wire, found 0 [nodes if1; edges ]. wire from 'Login' to 'Result': the source node has no exit named 'true'; its only exit is 'result' [nodes ; edges e9]. — node(s): if1; edge(s): e9";

describe('multi-problem save messages', () => {
  const nodes = [
    node('login', {
      kind: 'Request',
      label: 'Login',
      source: { type: 'Saved', requestPath: 'login.yml' },
    }),
    node('out', { kind: 'Output', label: 'Result' }),
    node('if1', { kind: 'If', label: 'Logged in?', condition: 'response.status === 200' }),
  ];
  const edges = [wire('e9', 'login', 'out', 'value', 'true')];

  it('puts each problem on its own node or wire', () => {
    const issues = computeFlowIssues(nodes, edges, {
      save: { nodeIds: ['if1'], edgeIds: ['e9'], message: MULTI_MESSAGE },
    }).filter((i) => i.code === 'save');
    expect(issues).toEqual([
      {
        code: 'save',
        severity: 'error',
        nodeId: 'if1',
        message: "'Logged in?': the If node needs exactly one input wire, found 0.",
        hint: 'Fix this, then save again.',
      },
      {
        code: 'save',
        severity: 'error',
        edgeId: 'e9',
        message:
          "Wire from 'Login' to 'Result': the source node has no exit named 'true'; its only exit is 'result'.",
        hint: 'Fix this, then save again.',
      },
    ]);
  });

  it('skips a problem whose node is no longer on the canvas', () => {
    const issues = computeFlowIssues(nodes.slice(0, 2), edges, {
      save: { nodeIds: ['if1'], edgeIds: ['e9'], message: MULTI_MESSAGE },
    }).filter((i) => i.code === 'save');
    expect(issues.map((i) => i.edgeId ?? i.nodeId)).toEqual(['e9']);
  });

  it('keeps the toast short and counts the rest', () => {
    expect(cleanSaveMessage(MULTI_MESSAGE)).toBe(
      "Flow has 2 problems. 'Logged in?': the If node needs exactly one input wire, found 0. Wire from 'Login' to 'Result': the source node has no exit named 'true'; its only exit is 'result'.",
    );
    const cut =
      "Invalid input: flow has 53 problems, showing the first 2: 'Check': the If node's condition is empty [nodes if1; edges ]. 'Pick': the Switch node's value is empty [nodes sw; edges ]. — node(s): if1, sw; edge(s): ";
    expect(cleanSaveMessage(cut)).toBe(
      "Flow has 53 problems. 'Check': the If node's condition is empty. 'Pick': the Switch node's value is empty. And 51 more.",
    );
  });

  it('still hides a case value in a multi-problem message', () => {
    const message =
      "Invalid input: flow has 2 problems: 'Router': more than one case matches 'secret-token' [nodes sw; edges ]. 'Check': the If node's condition is empty [nodes if1; edges ]. — node(s): sw, if1; edge(s): ";
    expect(cleanSaveMessage(message)).not.toContain('secret-token');
    const issues = computeFlowIssues(
      [
        node('sw', { kind: 'Switch', label: 'Router', value: 'response.body', cases: [] }),
        node('if1', { kind: 'If', label: 'Check', condition: 'true' }),
      ],
      [],
      { save: { nodeIds: ['sw', 'if1'], edgeIds: [], message } },
    ).filter((i) => i.code === 'save');
    expect(issues[0]?.message).toBe("'Router': more than one case matches value hidden.");
    expect(JSON.stringify(issues)).not.toContain('secret-token');
  });

  it('shows the panel only the problems of its own node', () => {
    expect(saveMessageForNode(MULTI_MESSAGE, 'if1')).toBe(
      "'Logged in?': the If node needs exactly one input wire, found 0.",
    );
    // A node the message does not name gets the whole short text.
    expect(saveMessageForNode(MULTI_MESSAGE, 'other')).toBe(cleanSaveMessage(MULTI_MESSAGE));
    // A single-problem message is cleaned as before.
    expect(
      saveMessageForNode(
        'Invalid input: flow is invalid: the If node needs exactly one input wire, found 0 — node(s): b; edge(s): ',
        'b',
      ),
    ).toBe('The If node needs exactly one input wire, found 0.');
  });
});
```

If `wire` in `flow-issues.test.ts` does not accept the fifth argument as shown above, use the signature at the top of that file: `wire(id, source, target, field, sourceHandle?)`. Its fourth argument is the target field, and the fifth is the source handle.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test flow-wiring flow-issues --run --maxWorkers=2`
Expected: FAIL. `parseGraphProblems` and `saveMessageForNode` are not exported, so the new tests fail with `is not a function`. The old tests still pass.

- [ ] **Step 3: Write the implementation**

`src/lib/flow-wiring.ts`, after `parseGraphErrorMessage`:

```ts
export interface GraphProblem {
  text: string;
  nodeIds: string[];
  edgeIds: string[];
}

export interface GraphProblems {
  /** The number of problems the flow has, which can be more than were listed. */
  total: number;
  problems: GraphProblem[];
}

// The header of a save error that lists several problems. rocket_app builds it:
// "flow has N problems:" or "flow has N problems, showing the first M:".
const PROBLEMS_HEADER = /^(?:Invalid input:\s*)?flow has (\d+) problems(?:, showing the first \d+)?:\s*/;
// One listed problem: its text, then a tag that holds its ids, then a full stop.
const PROBLEM = /(.+?) \[nodes ([^;\]]*); edges ([^\]]*)\]\./g;
// The combined list that closes every graph error.
const COMBINED_TAIL = /\s*—\s*node\(s\):[^;]*;\s*edge\(s\):.*$/;

// Reads a save error that lists several problems, one entry per problem with
// its own ids. Returns null for any other message, so the caller falls back to
// parseGraphErrorMessage, which still reads the combined list of ids.
export function parseGraphProblems(message: string): GraphProblems | null {
  const header = message.match(PROBLEMS_HEADER);
  if (!header) return null;
  const body = message.slice(header[0].length).replace(COMBINED_TAIL, '');
  const problems = [...body.matchAll(PROBLEM)].map(
    (m): GraphProblem => ({
      text: (m[1] ?? '').trim(),
      nodeIds: splitIds(m[2]),
      edgeIds: splitIds(m[3]),
    }),
  );
  if (problems.length === 0) return null;
  return { total: Number(header[1]), problems };
}
```

`src/lib/flow-issues.ts`: add `import { parseGraphProblems } from '@/lib/flow-wiring';` after the `flow-handles` import. Replace `cleanSaveMessage` and `saveIssues` (keep the comment block and `QUOTED_CASE_VALUE` above them):

```ts
// Most problems the toast spells out. The canvas shows every one on its node or wire.
const MAX_TOAST_PROBLEMS = 3;

// Hides quoted case values, then makes the text one sentence.
function tidySentence(text: string): string {
  const masked = text.replace(QUOTED_CASE_VALUE, '$1value hidden').trim();
  if (!masked) return '';
  const sentence = masked.charAt(0).toUpperCase() + masked.slice(1);
  return /[.!?]$/.test(sentence) ? sentence : `${sentence}.`;
}

export function cleanSaveMessage(message: string): string {
  const multi = parseGraphProblems(message);
  if (multi) {
    const shown = multi.problems
      .slice(0, MAX_TOAST_PROBLEMS)
      .map((p) => tidySentence(p.text))
      .filter(Boolean);
    const more = multi.total - shown.length;
    return [
      `Flow has ${multi.total} problems.`,
      ...shown,
      ...(more > 0 ? [`And ${more} more.`] : []),
    ].join(' ');
  }
  const cleaned = message
    .replace(/^Invalid input:\s*/, '')
    .replace(/^flow is invalid:\s*/, '')
    .replace(/\s*(?:—|-)?\s*node\(s\):[^;]*(?:;\s*edge\(s\):.*)?$/, '')
    .replace(/\s+through\s*$/, '');
  return tidySentence(cleaned) || 'The last save was rejected.';
}

// The text the properties panel shows for one flagged node. A multi-problem
// message gives only the problems that name this node.
export function saveMessageForNode(message: string, nodeId: string): string {
  const own = parseGraphProblems(message)?.problems.filter((p) => p.nodeIds.includes(nodeId));
  if (!own || own.length === 0) return cleanSaveMessage(message);
  return own.map((p) => tidySentence(p.text)).join(' ');
}

function saveIssues(save: SaveErrorInfo, nodeIds: Set<string>, edgeIds: Set<string>): FlowIssue[] {
  const hint =
    save.kind === 'run'
      ? 'Run the full flow, or Run from the named node.'
      : 'Fix this, then save again.';
  const multi = parseGraphProblems(save.message ?? '');
  if (multi) {
    return multi.problems.flatMap((problem) => {
      const message = tidySentence(problem.text);
      return [
        ...[...new Set(problem.nodeIds)]
          .filter((id) => nodeIds.has(id))
          .map(
            (nodeId): FlowIssue => ({ code: 'save', severity: 'error', nodeId, message, hint }),
          ),
        ...[...new Set(problem.edgeIds)]
          .filter((id) => edgeIds.has(id))
          .map(
            (edgeId): FlowIssue => ({ code: 'save', severity: 'error', edgeId, message, hint }),
          ),
      ];
    });
  }
  const message = cleanSaveMessage(save.message ?? '');
  return [
    ...[...new Set(save.nodeIds)]
      .filter((id) => nodeIds.has(id))
      .map((nodeId): FlowIssue => ({ code: 'save', severity: 'error', nodeId, message, hint })),
    ...[...new Set(save.edgeIds)]
      .filter((id) => edgeIds.has(id))
      .map((edgeId): FlowIssue => ({ code: 'save', severity: 'error', edgeId, message, hint })),
  ];
}
```

`src/components/flow/FlowPane.tsx`: change the import to `import { cleanSaveMessage, computeFlowIssues, saveMessageForNode } from '@/lib/flow-issues';` and change the panel prop to:

```tsx
              saveError={
                saveErrorMessage && cycleNodeIds.includes(panelNode.id)
                  ? saveMessageForNode(saveErrorMessage, panelNode.id)
                  : undefined
              }
```

`cleanSaveMessage` is still used by `handleSave`'s toast and by `save-tab.ts`, so the toast stays masked and short for every caller. Do not change those call sites.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test flow-wiring flow-issues --run --maxWorkers=2`
Expected: PASS, including the old `cleanSaveMessage` and `parseGraphErrorMessage` tests.

Run: `yarn test FlowPane --run --maxWorkers=2`
Expected: PASS (the panel prop only changes for a multi-problem message).

Run: `yarn tsc --noEmit` and `yarn check`
Expected: no errors.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then commit with explicit paths:
`git add src/lib/flow-wiring.ts src/lib/flow-issues.ts src/components/flow/FlowPane.tsx src/lib/__tests__/flow-wiring.test.ts src/lib/__tests__/flow-issues.test.ts && git commit --only -m "feat(flow): show each save problem on its own node or wire" -- src/lib/flow-wiring.ts src/lib/flow-issues.ts src/components/flow/FlowPane.tsx src/lib/__tests__/flow-wiring.test.ts src/lib/__tests__/flow-issues.test.ts`

---

## Open decisions

- **Rule order and grouping.** Errors come grouped by rule, in the old rule-table order, not sorted by node. Recommended default: keep it, because it is what makes `validate` equal the first error. A node-sorted view can be built by the canvas.
- **Cap of 50.** Recommended default: 50 errors, with the count of the rest in the header and no summary lint. Raise it only if a real flow needs more.
- **Message format versus F-50.** Recommended default: the tagged string in Task 2, because it keeps the old regexp working. F-50 will replace it with `{code, message, nodeIds, edgeIds}`, and then `parseGraphProblems` goes away.
- **Raw reason text in the save message.** The reasons still echo a Switch match value and a bad exit or field name, as they did before. The client masks match values after a `case` or `matches` word. Recommended default: leave it, and give the lints and F-50 the server-side masking. A server-side pass would reuse `sentence` from `lint.rs`.
- **Reasons that use node ids.** The callback-order reason (V15) still names the Request by id (`Request 'r1' sends ...`). Recommended default: leave it for now. It is a one-line change in `check_callback_order` to look up the label with `display_label`, but `lint.rs` `sentence` matches the `Request '` prefix and would need the same change.
- **Cycle id lists.** A very large cycle repeats its ids in the problem tag and in the tail. Recommended default: accept it, since the cycle list was already unbounded.
- **Roadmap.** `.claude/flow-roadmap.md` F-28 should be marked done when this plan merges. It is untracked on main, so the final merge step of the series should update it, not this plan.

## Self-review

- Spec coverage: all-errors with a deterministic order and a cap (Task 1); unknown exit lists the valid exits, `auth` error names both kinds, labels in save and lint, one lint per item (Task 2); the client reads each problem, keeps the toast short and keeps masking (Task 3).
- Compatibility: `validate` keeps its signature and equals the first error (fixture test); single-error text is unchanged; the old regexp reads the union of ids from a multi-problem message (test in Task 3).
- Names used across tasks: `validate_all`, `validate_report`, `ValidationReport`, `MAX_VALIDATION_ERRORS` (Task 1, 2); `display_label`, `exits_text`, `wire_ends`, `source_kind` (Task 2); `graph_errors_message`, `single_problem_message`, `describe_problem`, `message_safe` (Task 2); `parseGraphProblems`, `GraphProblem`, `GraphProblems`, `saveMessageForNode` (Task 3). The tag shape `[nodes <ids>; edges <ids>].` is identical in Task 2 and Task 3.
- No placeholders. Plan 3 interaction is handled by the re-read step in each task.

Next plan to execute: 05-corrupt-flows-and-step-guard.md
