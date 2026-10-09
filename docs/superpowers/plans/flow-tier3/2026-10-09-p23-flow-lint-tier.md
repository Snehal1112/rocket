# Flow Lint Tier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Execute this plan:** P23 (roadmap F-20). It needs nothing unmerged. After it is merged, P21 (`2026-10-08-p21-backend-lint-feed.md`) is unblocked once the P21 changes in "Deviations" below are applied.

> **Worktree: do NOT create a new worktree.** Reuse the existing worktree `/home/numericlabs/data/rocket/rocket/.claude/worktrees/flow-p20` (it has its own warm `target/`). Its branch `worktree-flow-p20` sits at main HEAD 18b22468. Before Task 1, start a new branch there from main: `git -C /home/numericlabs/data/rocket/rocket/.claude/worktrees/flow-p20 switch -c worktree-flow-p23 main` (the worktree has no tracked changes). Every path in this plan is relative to that worktree. Read this plan from the main checkout (`/home/numericlabs/data/rocket/rocket/docs/superpowers/plans/flow-tier3/`), because the plan folder is untracked.

**Goal:** Add a non-blocking lint tier to the Rust backend. `rocket_flow::validate_with_warnings(flow, ctx)` returns warnings about a graph that runs but likely does not do what its author meant, and `FlowService::lint(collection, flow)` combines it with the hard `validate` failure, so P21 can feed both into the canvas while the user edits.

**Architecture:** A new pure module `crates/rocket-flow/src/lint.rs` holds `FlowLint`, `LintSeverity`, the code constants, the `LintContext` seam (saved-request lookup and known variables, answered by the app layer, never I/O in `rocket-flow`), `validate_with_warnings` and `graph_error_lints` (one `invalid_graph` error lint per node or wire that a `validate` failure names). The rules are the three P21 takes over from the client (unwired If exit or Switch case, unwired Switch default, no path to an Output). All rules read one `GraphIndex` built once per call, so a lint is linear in the size of the graph. `FlowService::lint` in `rocket-app` runs `validate` on the given graph (not the saved file), turns a failure into error lints, then appends the warnings. `validate`, `FlowService::save` and the run path are unchanged: lints never block save or run.

**Tech Stack:** Rust (`rocket-flow`, `rocket-app`). No TypeScript, no IPC change (the `lint_flow` command and DTO are P21 Task 1).

**Spec:** Roadmap items F-20 (the tier) and the F-23 rules it needs, in `.claude/flow-roadmap.md` (main checkout). The consumer contract is "Required from F-20" in `docs/superpowers/plans/flow-tier3/2026-10-08-p21-backend-lint-feed.md` (lines 15 to 62). The client rules it replaces are in `src/lib/flow-issues.ts` (plan P12, `2026-10-08-p12-issue-badges.md`). Design notes: `01-design-notes.md`, sections "P12" and "P21".

## What P21 consumes (produced here, exactly)

```rust
// crates/rocket-flow/src/lint.rs, exported as `rocket_flow::lint`.
pub const INVALID_GRAPH: &str = "invalid_graph";
pub const EXIT_WITHOUT_EDGE: &str = "exit_without_edge";
pub const SWITCH_WITHOUT_DEFAULT: &str = "switch_without_default";
pub const NO_PATH_TO_OUTPUT: &str = "no_path_to_output";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity { Error, Warning }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowLint {
    pub code: String,
    pub severity: LintSeverity,
    pub node_id: Option<String>,
    pub edge_id: Option<String>,
    pub message: String,
    pub hint: Option<String>,
}

pub trait LintContext {
    fn saved_request_exists(&self, _request_path: &str) -> Option<bool> { None }
    fn variable_is_known(&self, _name: &str) -> Option<bool> { None }
}
pub struct NoLintContext;

pub fn validate_with_warnings(flow: &Flow, ctx: &dyn LintContext) -> Vec<FlowLint>;
pub fn graph_error_lints(flow: &Flow, error: &FlowGraphError) -> Vec<FlowLint>;

// crates/rocket-app/src/flow_service.rs
impl FlowService {
    pub fn lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint>;
}
```

`FlowLint` and `LintSeverity` have no serde derives. P21's `FlowLintDto` (camelCase) maps them field by field, exactly as P21 Task 1 Step 5 already writes it.

## Client rule mapping (what P21 Task 3 may delete)

Every client rule in `src/lib/flow-issues.ts` at HEAD 18b22468, and where it lives after this plan:

| P12 client code | Client severity | Backend after P23 | P21 action |
|---|---|---|---|
| `exit-unwired` (If `true`/`false`, Switch cases) | warning | `exit_without_edge`, warning, one lint per node naming every unwired exit | delete client rule |
| `exit-unwired` (the Switch `default` part of the same issue) | warning | `switch_without_default`, warning | delete client rule |
| `no-path-to-output` | warning | `no_path_to_output`, warning (an Auth node with `apply_to_inherit` on is no longer flagged, see Decisions) | delete client rule |
| `expr-blank` | error | `invalid_graph` (V8), first violation only | keep client rule |
| `input-missing` | error | `invalid_graph` (V1), first violation only | keep client rule |
| `switch-duplicate-match` | error | `invalid_graph` (V7), first violation only | keep client rule |
| `repeat-limits` | error | `invalid_graph` (V9), first violation only | keep client rule |
| `wait-name-invalid`, `wait-name-duplicate`, `wait-timeout-range`, `wait-accept-empty` | error | `invalid_graph` (V10), first violation only | keep client rule |
| `output-no-value` | warning | none (roadmap F-27 makes it a hard error later) | keep client rule |
| `request-path-empty` | error | none (roadmap F-21 later) | keep client rule |
| `request-url-empty` | error | none | keep client rule |
| `save` | error | same rule as `invalid_graph` | merged by P21's alias |

So removing `exitIssues`, `nodesReachingOutput` and the `no-path-to-output` block from `computeFlowIssues` loses nothing. The other P21 codes (`dangling_saved_request`, `unknown_variable`, `auth_no_effect`, `auth_wire_overrides_auth`, `callback_not_wired`) are F-21, F-22, F-24 and F-25. This plan does not emit them. P21 may keep them in `BACKEND_LINT_CODES` as reserved names, because the client never emits them.

## Deviations from P21 (the human partner applies these to the P21 plan)

1. **P21 line 5** says "F-20 is not planned yet". Change to: "F-20 is plan P23 (`2026-10-09-p23-flow-lint-tier.md`)." Keep the rest of the BLOCKED note until P23 is merged.
2. **P21 lines 50 to 60 (code table), column "Client rule in P12 today".** P12 has no client codes named `exit_without_edge`, `switch_without_default` or `no_path_to_output`. It has `exit-unwired` (one issue per node that covers the If exits, the Switch cases and the Switch default together) and `no-path-to-output` (kebab case). Change the three cells to: `exit-unwired` (If exits and Switch cases), `exit-unwired` (Switch default part), `no-path-to-output`. Change the "Source" cells of those three rows from `F-23` to `P23 (F-20 core, F-23 subset)`. Effect to know: on a Switch with unwired cases and an unwired default, the backend reports two lints where the client reported one, so that node's issue count goes from 1 to 2.
3. **P21 line 708 (Task 3 Interfaces, "Produces")** must also say: `computeFlowIssues` no longer emits `exit-unwired` or `no-path-to-output`.
4. **P21 lines 712 to 741 (Task 3 Step 1, guard test).** As written the test passes before any deletion, because the client codes are kebab case and `BACKEND_LINT_CODES` is snake case. It is not a red test. Also the graph has no Output node, so `no-path-to-output` cannot fire at all. Change it in two ways. (a) Add a fourth node `{ id: 'out', position: { x: 0, y: 0 }, kind: { kind: 'Output' as const, label: 'Out' } }` with no wire into it. (b) After the `BACKEND_LINT_CODES` loop, add `expect(codes).not.toContain('exit-unwired'); expect(codes).not.toContain('no-path-to-output');`. The expected failure at line 741 then reads: FAIL (the client still emits `exit-unwired` and `no-path-to-output`).
5. **P21 line 747 (Task 3 Step 2 item 1)**, "If P12 named these three codes differently, first rename them". Replace with: delete `exitIssues`, `nodesReachingOutput`, the `reaching` variable and the `no-path-to-output` block inside `computeFlowIssues`, and the now unused imports (`caseHandle`, `DEFAULT_HANDLE`, `FALSE_HANDLE`, `RESULT_HANDLE`, `TRUE_HANDLE` from `@/lib/flow-handles`, if nothing else in the file uses them). No rename is needed.
6. **P21 line 749 (item 3)**: add `output-no-value`, `request-path-empty` and `request-url-empty` to the list of client rules that stay.
7. **P21 line 757**: "delete the tests that asserted those three warnings" means the five tests in `describe('computeFlowIssues: warnings about the shape of the flow')` in `src/lib/__tests__/flow-issues.test.ts`. The sample object with `code: 'exit-unwired'` near line 288 of that file is only a fixture for the summary helpers and may stay.
8. **P21 line 13 (Spec)**: add `docs/superpowers/plans/flow-tier3/2026-10-09-p23-flow-lint-tier.md`.
9. **P21 lines 107, 108, 109 (Task 1 Files)**: line hints moved. `save_flow` is now at `src-tauri/src/commands/flow.rs:456`, `commands::flow::save_flow,` at `src-tauri/src/lib.rs:683`, `saveFlow` at `src/lib/tauri-api.ts:2175`. The quoted anchors still work.
10. No change is needed to P21's "Required from F-20" Rust block: names, paths, fields and the `FlowService::lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint>` signature are produced exactly. The P21 golden fixture text (`"The 'false' exit of 'Check status' has no wire."`) also matches the message this plan builds.

Deviation from the roadmap wording of F-20: "Every lint carries node ID, label and a fix hint." P21's contract has no `label` field, so the label is carried inside `message` (every message names its node by label, falling back to the kind name). This keeps P21's DTO unchanged.

## Decisions

- `LintContext` has two methods that both default to `None` ("cannot tell"), and a lint that gets `None` is skipped. They return yes or no, never a value, so no lint can quote a resolved variable or a secret. No rule in this plan calls them. F-21 (dangling saved request) and F-22 (unknown variable) add the rules and an app-side context built from the collection repository. Until then `FlowService::lint` passes `NoLintContext`, so `FlowService` keeps its one-argument constructor and `src-tauri/src/lib.rs` is untouched.
- An Auth node with `apply_to_inherit` on and no wires is not flagged by `no_path_to_output`. It acts on requests without a wire, so the client's warning there was a false positive. An Auth node with it off and no wires is still flagged (F-24 owns "Auth node with no effect").
- `no_path_to_output` keeps the client's scope otherwise: it runs only when the flow has an Output, skips Output nodes and counts every wire kind (data, `trigger`, `auth`) as a path. Its hint says to ignore it when the node runs only for its effect, because a final request with a side effect is a valid flow.
- `exit_without_edge` is one lint per node naming every unwired exit, as the client did. `switch_without_default` is separate, because a Switch whose cases cover every value may leave default unwired on purpose, and P21 treats the two codes apart.
- A wire whose target id does not exist still counts as wiring its exit. The ghost target is already an `invalid_graph` error, and a second warning on the same wire is noise.
- `graph_error_lints` turns a cycle into one error per named node and one per named wire (like P12's `saveIssues`), names an invalid node by its label, reports an unknown node on the first wire that points to it (the ghost id is never a `node_id`, because no badge could show it), and writes each reason as a sentence.
- Order is deterministic: `FlowService::lint` returns the `invalid_graph` lints first (in the order `validate` names ids, which already follows file order), then the warnings in node file order, and per node in rule order (`exit_without_edge`, `switch_without_default`, `no_path_to_output`). Hash maps are used for lookups only, never iterated for output. A repeated node id is linted once (its first node).

## Global Constraints

- Every task begins with: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. (The lint context is the seam for saved-request lookup and variable resolution.)
- Work in the existing worktree `/home/numericlabs/data/rocket/rocket/.claude/worktrees/flow-p20` on branch `worktree-flow-p23`. Never create another worktree.
- Rust cargo commands always pass `-j4 -p <crate>`. Never `--workspace` or `--all`. Never `cargo fmt` (write rustfmt-style code by hand).
- No `unwrap()` in production paths. Tests may use `expect`.
- `rocket-flow` does no I/O: no `std::fs`, no repository calls, no new dependencies in `crates/rocket-flow/Cargo.toml`.
- No serde derives on `FlowLint` or `LintSeverity`. `serde(rename_all = "camelCase")` belongs only on P21's IPC DTO. Persistence structs (`Flow`, `FlowNode`, `FlowEdge`, `FlowNodeKind`) are not touched.
- `validate` keeps its behaviour. The only edit in `validate.rs` is the visibility of `kind_name` (`fn` to `pub(crate) fn`). `FlowService::save`, `flow_execution_service.rs` and `src-tauri` are not touched.
- A lint message or hint is built only from node labels, kind names, exit names (`true`, `false`, `default`, case labels) and `validate` reasons. It never contains an Input value, an expression, a condition, a Switch match value, a URL, a header, a body or any auth field.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: the task's targeted `cargo test -j4 -p <crate> <filter>`, `cargo check -j4 -p rocket-flow -p rocket-app`, and `cargo clippy -j4 -p rocket-flow -p rocket-app` with no new warnings in the files the task touched.
- Line numbers are from HEAD 18b22468. Locate edits by the quoted code.
- Not in scope: roadmap F-21 to F-30 (dangling saved requests, unknown variables, auth lints, callback ordering lint, new hard errors, multi-error validation, corrupt-file listing, step guard), the `lint_flow` command, DTO and frontend (P21), lint on save.

## Review Focus

Failure modes the spec implies that are most likely to bite a person using this, most likely first. Each has a test in the task that owns the code.

1. False positives on a valid flow: an exit wired through a `trigger` wire, a Switch with every exit wired, a path to the Output through an `auth` wire, an Auth node that applies to inherited auth, a flow without any Output, an empty flow. None may warn. Pinned in Task 1 (`an_exit_wired_by_a_trigger_wire_counts_as_wired`, `a_switch_with_every_exit_wired_has_no_lint`, `a_fully_wired_if_has_no_exit_lint`) and Task 2 (`paths_through_trigger_and_auth_wires_reach_the_output`, `an_auth_node_that_applies_to_inherited_auth_is_not_flagged`, `no_output_means_no_reach_lint`, `an_empty_flow_has_no_lints`).
2. Secrets never in messages: a lint must not echo an Input value, a condition, a Switch match value or an Auth token. Pinned in Task 2 (`messages_never_quote_values_or_expressions`).
3. Performance on large flows (P21 calls it after each pause in editing): rules must be linear, not one wire scan per node. Pinned in Task 2 (`a_large_flow_lints_in_linear_time`, 20 000 If nodes under 1 s). Note for the reviewer: `validate`'s own V1 check scans all wires per If, Switch and Transform node. That is unchanged, and fine at the sizes people draw.
4. Unknown or ghost node ids (a wire to or from a deleted node, a repeated id, a cycle) must not panic, hang or produce a lint on an id the canvas cannot show. Pinned in Task 1 (`ghost_node_ids_do_not_panic_or_get_lints`, `a_repeated_node_id_is_linted_once`) and Task 2 (`a_cycle_does_not_hang_the_reach_rule`, `an_unknown_node_is_reported_on_the_wire_that_names_it`).
5. Deterministic ordering: the same graph gives the same list in the same order, whatever the wire order. Pinned in Task 2 (`results_follow_node_order_and_ignore_edge_order`) and Task 3 (`lint_puts_structural_errors_first_and_is_stable`).
6. Warnings never alter save or run: a flow with warnings saves unchanged, and lint reads only the graph it is given, never the repository. Pinned in Task 3 (`a_flow_with_warnings_still_saves_unchanged`, `lint_reads_only_the_given_graph`) plus the "untouched files" check in Task 3 Step 6.
7. No I/O in `rocket-flow`, including for future lints that need saved requests: the context answers yes, no or `None`, and `None` never warns. Pinned by the `LintContext` shape (Task 1) and the dependency check in Task 1 Step 7.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-flow/src/lint.rs` (new) | `FlowLint`, `LintSeverity`, codes, `LintContext`, `NoLintContext`, `GraphIndex`, the three warning rules, `graph_error_lints`, tests. |
| `crates/rocket-flow/src/lib.rs` (modify) | `pub mod lint;` and re-exports. |
| `crates/rocket-flow/src/validate.rs` (modify) | `kind_name` becomes `pub(crate)`. Nothing else. |
| `crates/rocket-app/src/flow_service.rs` (modify) | `FlowService::lint`, tests. |
| `crates/rocket-flow/CLAUDE.md`, `crates/rocket-app/CLAUDE.md` (modify) | Document the lint tier. |

---

### Task 1: Lint types, context seam and exit rules

**Files:**
- Create: `crates/rocket-flow/src/lint.rs`
- Modify: `crates/rocket-flow/src/lib.rs`
- Modify: `crates/rocket-flow/src/validate.rs` (the line `fn kind_name(kind: &FlowNodeKind) -> &'static str {`)

**Interfaces:**
- Consumes: `Flow`, `FlowNode` (`crate::flow`), `FlowNodeKind` (`crate::node`), `handle::{TRUE, FALSE, DEFAULT, case_handle}`, `validate::kind_name`.
- Produces: `rocket_flow::lint::{FlowLint, LintSeverity, LintContext, NoLintContext, validate_with_warnings, INVALID_GRAPH, EXIT_WITHOUT_EDGE, SWITCH_WITHOUT_DEFAULT, NO_PATH_TO_OUTPUT}`, also re-exported at the crate root (`rocket_flow::validate_with_warnings`, `rocket_flow::FlowLint`, and so on). Private helpers Task 2 extends: `struct GraphIndex<'a> { wired_exits }` with `GraphIndex::new(flow)` and `is_wired(node_id, exit)`, `fn display_label(node: &FlowNode) -> &str`, `fn warning(code, node, message, hint) -> FlowLint`.

- [ ] **Step 1: Read the spec reference and prepare the branch**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

Then, if not done yet:

```bash
git -C /home/numericlabs/data/rocket/rocket/.claude/worktrees/flow-p20 status --short
git -C /home/numericlabs/data/rocket/rocket/.claude/worktrees/flow-p20 switch -c worktree-flow-p23 main
```

Expected: no tracked changes listed (untracked plan copies are fine), and the switch succeeds.

- [ ] **Step 2: Make `kind_name` visible to the crate**

In `crates/rocket-flow/src/validate.rs`, change:

```rust
fn kind_name(kind: &FlowNodeKind) -> &'static str {
```

to:

```rust
pub(crate) fn kind_name(kind: &FlowNodeKind) -> &'static str {
```

- [ ] **Step 3: Write the module with a stub and the failing tests**

Create `crates/rocket-flow/src/lint.rs`:

```rust
//! Non-blocking lint tier (roadmap F-20). `validate` rejects a graph that
//! cannot be saved or run. This module only warns about a graph that runs
//! but may not do what its author meant. Lints never block save or run.
//!
//! The module does no I/O. Facts that need the collection or the
//! environment come through `LintContext`, which the app layer provides.
//! Results are deterministic: nodes in file order, rules in a fixed order.

use crate::flow::{Flow, FlowNode};
use crate::handle;
use crate::node::FlowNodeKind;
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
}

/// A context that knows nothing about the collection or the environment.
pub struct NoLintContext;

impl LintContext for NoLintContext {}

/// Lints `flow` without blocking anything. Works on any graph, valid or
/// not, and never panics on unknown ids or cycles.
pub fn validate_with_warnings(flow: &Flow, ctx: &dyn LintContext) -> Vec<FlowLint> {
    let _ = (flow, ctx);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowEdge;
    use crate::node::{NodePosition, RequestSource, SwitchCase};

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
            vec![input("in"), if_node("if1", "Check"), output("ok"), output("ko")],
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
            vec![input("in"), if_node("if1", "Check"), request("req"), output("out")],
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
            vec![input("in"), if_node("if1", "First"), if_node("if1", "Second")],
            vec![edge("e1", "in", handle::RESULT, "if1", handle::INPUT)],
        );
        let lints = lint(&f);
        let exits = only(&lints, EXIT_WITHOUT_EDGE);
        assert_eq!(exits.len(), 1);
        assert!(exits[0].message.contains("'First'"));
    }
}
```

In `crates/rocket-flow/src/lib.rs`, add `pub mod lint;` after `pub mod handle;`, and add after `pub use validate::validate;`:

```rust
pub use lint::{validate_with_warnings, FlowLint, LintContext, LintSeverity, NoLintContext};
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow lint::`
Expected: FAIL. The stub returns no lints, so `an_unwired_if_exit_is_one_warning_naming_the_exit_and_label`, `both_unwired_if_exits_are_one_warning`, `switch_cases_and_default_are_reported_separately`, `a_switch_with_no_cases_only_warns_about_default`, `blank_labels_fall_back_to_the_kind_name`, `ghost_node_ids_do_not_panic_or_get_lints` and `a_repeated_node_id_is_linted_once` fail. The three "no lint" tests pass.

- [ ] **Step 5: Implement the index and the exit rules**

In `crates/rocket-flow/src/lint.rs`, replace the stub `validate_with_warnings` with:

```rust
/// Lints `flow` without blocking anything. Works on any graph, valid or
/// not, and never panics on unknown ids or cycles.
pub fn validate_with_warnings(flow: &Flow, ctx: &dyn LintContext) -> Vec<FlowLint> {
    // No rule asks the context yet. F-21 and F-22 add the rules that do.
    let _ = ctx;
    let index = GraphIndex::new(flow);
    let mut seen: HashSet<&str> = HashSet::with_capacity(flow.nodes.len());
    let mut lints = Vec::new();
    for node in &flow.nodes {
        // A repeated id is an invalid_graph error. Lint its first node only.
        if !seen.insert(node.id.as_str()) {
            continue;
        }
        lints.extend(exit_lints(node, &index));
    }
    lints
}

/// Lookups built once per call, so every rule stays linear in graph size.
struct GraphIndex<'a> {
    /// The exits of each node that have at least one wire.
    wired_exits: HashMap<&'a str, HashSet<&'a str>>,
}

impl<'a> GraphIndex<'a> {
    fn new(flow: &'a Flow) -> Self {
        let mut wired_exits: HashMap<&'a str, HashSet<&'a str>> = HashMap::new();
        for edge in &flow.edges {
            wired_exits
                .entry(edge.source_node_id.as_str())
                .or_default()
                .insert(edge.source_handle.as_str());
        }
        Self { wired_exits }
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

fn warning(code: &str, node: &FlowNode, message: String, hint: &str) -> FlowLint {
    FlowLint {
        code: code.to_string(),
        severity: LintSeverity::Warning,
        node_id: Some(node.id.clone()),
        edge_id: None,
        message,
        hint: Some(hint.to_string()),
    }
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
            format!("The {} exits of '{label}' have no wire.", unwired.join(", "))
        };
        lints.push(warning(
            EXIT_WITHOUT_EDGE,
            node,
            message,
            "A run that takes an unwired exit ends that branch. Wire it to a node if the run should go on.",
        ));
    }
    if matches!(node.kind, FlowNodeKind::Switch { .. }) && !index.is_wired(&node.id, handle::DEFAULT)
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
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow lint::`
Expected: PASS (10 tests).

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS. Every existing `validate`, `graph`, `node`, `flow` and `handle` test still passes.

- [ ] **Step 7: Gates and commit**

Run: `cargo check -j4 -p rocket-flow -p rocket-app && cargo clippy -j4 -p rocket-flow`
Expected: no errors, and no warnings in `lint.rs`, `lib.rs` or `validate.rs`.

Run: `git diff main -- crates/rocket-flow/Cargo.toml crates/rocket-flow/src/validate.rs`
Expected: `Cargo.toml` unchanged, and `validate.rs` shows only the `pub(crate)` change.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-flow/src/lint.rs crates/rocket-flow/src/lib.rs crates/rocket-flow/src/validate.rs`
Suggested subject: `feat(flow): add the lint tier with exit warnings`.

---

### Task 2: Reach rule, structural errors as lints, scale and order guards

**Files:**
- Modify: `crates/rocket-flow/src/lint.rs`
- Modify: `crates/rocket-flow/src/lib.rs`

**Interfaces:**
- Consumes: Task 1's `GraphIndex`, `display_label`, `warning`, `exit_lints`, test helpers (`node`, `input`, `output`, `request`, `if_node`, `switch_node`, `edge`, `flow`, `lint`, `only`); `crate::graph::FlowGraphError`; `crate::validate::validate` (tests only).
- Produces: `pub fn graph_error_lints(flow: &Flow, error: &FlowGraphError) -> Vec<FlowLint>` (also `rocket_flow::graph_error_lints`); `validate_with_warnings` now also emits `NO_PATH_TO_OUTPUT`. Per node, rule order is exit lints, then the reach lint.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

In `crates/rocket-flow/src/lint.rs`, inside `mod tests`, add below the existing helpers:

```rust
    use crate::graph::FlowGraphError;
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
```

and these tests:

```rust
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
            vec![auth("a1", false), request("r1"), request("r2"), output("out")],
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
        assert_eq!(keys(&lint(&idle)), vec![(Some("a1"), None, NO_PATH_TO_OUTPUT)]);
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

    #[test]
    fn a_large_flow_lints_in_linear_time() {
        const N: usize = 20_000;
        let mut nodes = vec![input("in")];
        let mut edges = vec![edge("e-in", "in", handle::RESULT, "if0", handle::INPUT)];
        for i in 0..N {
            nodes.push(if_node(&format!("if{i}"), &format!("Check {i}")));
            let (next, field) = if i + 1 == N {
                ("out".to_string(), "value")
            } else {
                (format!("if{}", i + 1), handle::INPUT)
            };
            edges.push(edge(&format!("e{i}"), &format!("if{i}"), handle::TRUE, &next, field));
        }
        nodes.push(output("out"));
        let f = flow(nodes, edges);
        let started = std::time::Instant::now();
        let lints = lint(&f);
        let elapsed = started.elapsed();
        assert_eq!(lints.len(), N, "one unwired 'false' exit per If");
        assert!(lints.iter().all(|l| l.code == EXIT_WITHOUT_EDGE));
        // A rule that scans every wire for every node takes seconds here.
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "took {elapsed:?}"
        );
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
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow lint::`
Expected: FAIL to compile (`graph_error_lints` is not defined).

- [ ] **Step 4: Implement the reach rule and `graph_error_lints`**

In `crates/rocket-flow/src/lint.rs`:

1. Change the imports at the top to:

```rust
use crate::flow::{Flow, FlowNode};
use crate::graph::FlowGraphError;
use crate::handle;
use crate::node::FlowNodeKind;
use crate::validate::kind_name;
use std::collections::{HashMap, HashSet};
```

2. In `validate_with_warnings`, after `lints.extend(exit_lints(node, &index));`, add:

```rust
        lints.extend(no_path_lint(node, &index));
```

3. Replace `struct GraphIndex` and `GraphIndex::new` with:

```rust
/// Lookups built once per call, so every rule stays linear in graph size.
struct GraphIndex<'a> {
    /// The exits of each node that have at least one wire.
    wired_exits: HashMap<&'a str, HashSet<&'a str>>,
    /// Ids with a path to an Output, or `None` when the flow has no Output.
    reaching_output: Option<HashSet<&'a str>>,
}

impl<'a> GraphIndex<'a> {
    fn new(flow: &'a Flow) -> Self {
        let mut wired_exits: HashMap<&'a str, HashSet<&'a str>> = HashMap::new();
        for edge in &flow.edges {
            wired_exits
                .entry(edge.source_node_id.as_str())
                .or_default()
                .insert(edge.source_handle.as_str());
        }
        Self {
            wired_exits,
            reaching_output: nodes_reaching_output(flow),
        }
    }
```

(keep `is_wired` as it is).

4. Add after `exit_lints`:

```rust
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
    Some(warning(
        NO_PATH_TO_OUTPUT,
        node,
        format!(
            "'{}' does not lead to any Output, so its result is never shown.",
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
    let lint = |node_id: Option<&str>, edge_id: Option<&str>, message: String, hint: &str| {
        FlowLint {
            code: INVALID_GRAPH.to_string(),
            severity: LintSeverity::Error,
            node_id: node_id.map(str::to_string),
            edge_id: edge_id.map(str::to_string),
            message,
            hint: Some(hint.to_string()),
        }
    };
    let label_of = |node_id: &str| {
        flow.nodes
            .iter()
            .find(|n| n.id == node_id)
            .map_or_else(|| "This node".to_string(), |n| format!("'{}'", display_label(n)))
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

/// Makes a `validate` reason read as a sentence: a capital first letter and
/// an end mark.
fn sentence(reason: &str) -> String {
    let mut chars = reason.trim().chars();
    let Some(first) = chars.next() else {
        return "The flow has a structural problem.".to_string();
    };
    let mut out: String = first.to_uppercase().chain(chars).collect();
    if !out.ends_with(&['.', '!', '?'][..]) {
        out.push('.');
    }
    out
}
```

In `crates/rocket-flow/src/lib.rs`, extend the Task 1 re-export line to:

```rust
pub use lint::{
    graph_error_lints, validate_with_warnings, FlowLint, LintContext, LintSeverity, NoLintContext,
};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow lint::`
Expected: PASS (24 tests). Task 1's tests stay green because they filter by code, and their flows either have no Output or reach it.

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS.

- [ ] **Step 6: Gates and commit**

Run: `cargo check -j4 -p rocket-flow -p rocket-app && cargo clippy -j4 -p rocket-flow`
Expected: no errors and no warnings in `lint.rs` or `lib.rs`.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-flow/src/lint.rs crates/rocket-flow/src/lib.rs`
Suggested subject: `feat(flow): lint unreachable nodes and report graph errors as lints`.

---

### Task 3: `FlowService::lint` and docs

**Files:**
- Modify: `crates/rocket-app/src/flow_service.rs` (imports at the top; new method after `save`; `mod tests`)
- Modify: `crates/rocket-flow/CLAUDE.md` (module map table; new section at the end)
- Modify: `crates/rocket-app/CLAUDE.md` (new section at the end)

**Interfaces:**
- Consumes: `rocket_flow::lint::{graph_error_lints, validate_with_warnings, FlowLint, NoLintContext}` and the code constants from Tasks 1 and 2; `rocket_flow::validate`.
- Produces: `pub fn lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint>` on `FlowService` (P21 Task 1 calls `svc.lint(&collection, &flow.into())`).

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

In `crates/rocket-app/src/flow_service.rs`, inside `mod tests`, add after the existing `use` lines:

```rust
    use rocket_flow::lint::{LintSeverity, EXIT_WITHOUT_EDGE, INVALID_GRAPH, NO_PATH_TO_OUTPUT};
```

and at the end of the module:

```rust
    /// Panics on every call, so a test proves lint never reads or writes the repo.
    struct UntouchedRepo;
    impl FlowRepository for UntouchedRepo {
        fn list(&self, _collection: &str) -> DomainResult<Vec<String>> {
            panic!("lint must not list flows");
        }
        fn get(&self, _collection: &str, _name: &str) -> DomainResult<Flow> {
            panic!("lint must not read the saved flow");
        }
        fn save(&self, _collection: &str, _flow: &Flow) -> DomainResult<()> {
            panic!("lint must not save");
        }
        fn delete(&self, _collection: &str, _name: &str) -> DomainResult<()> {
            panic!("lint must not delete");
        }
    }

    fn wire(id: &str, from: &str, exit: &str, to: &str, field: &str) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: to.to_string(),
            target_field: field.to_string(),
            expression: String::new(),
            source_handle: exit.to_string(),
        }
    }

    /// in -> if1; if1.true -> out. The 'false' exit has no wire.
    fn routing_flow() -> Flow {
        Flow {
            name: "Routing".to_string(),
            nodes: vec![
                node_of(
                    "in",
                    FlowNodeKind::Input {
                        label: "Status".to_string(),
                        value: rocket_shared::VariableValue::simple("200"),
                    },
                ),
                node_of(
                    "if1",
                    FlowNodeKind::If {
                        label: "Check status".to_string(),
                        condition: "response === '200'".to_string(),
                    },
                ),
                node_of(
                    "out",
                    FlowNodeKind::Output {
                        label: "Result".to_string(),
                    },
                ),
            ],
            edges: vec![
                wire("e1", "in", rocket_flow::handle::RESULT, "if1", rocket_flow::handle::INPUT),
                wire("e2", "if1", rocket_flow::handle::TRUE, "out", "value"),
            ],
            callback_host: None,
        }
    }

    fn lint_keys(lints: &[FlowLint]) -> Vec<(Option<&str>, Option<&str>, &str)> {
        lints
            .iter()
            .map(|l| (l.node_id.as_deref(), l.edge_id.as_deref(), l.code.as_str()))
            .collect()
    }

    #[test]
    fn lint_reads_only_the_given_graph() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let lints = svc.lint("demo", &routing_flow());
        assert_eq!(lint_keys(&lints), vec![(Some("if1"), None, EXIT_WITHOUT_EDGE)]);
        assert_eq!(lints[0].severity, LintSeverity::Warning);
        assert_eq!(
            lints[0].message,
            "The 'false' exit of 'Check status' has no wire."
        );
    }

    #[test]
    fn lint_puts_structural_errors_first_and_is_stable() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let mut flow = cyclic_flow();
        flow.nodes.push(node_of(
            "c",
            FlowNodeKind::Input {
                label: "Lonely".to_string(),
                value: rocket_shared::VariableValue::simple("x"),
            },
        ));
        let lints = svc.lint("demo", &flow);
        assert_eq!(
            lint_keys(&lints),
            vec![
                (Some("a"), None, INVALID_GRAPH),
                (Some("b"), None, INVALID_GRAPH),
                (None, Some("e1"), INVALID_GRAPH),
                (None, Some("e2"), INVALID_GRAPH),
                (Some("c"), None, NO_PATH_TO_OUTPUT),
            ]
        );
        assert!(lints[..4].iter().all(|l| l.severity == LintSeverity::Error));
        assert_eq!(lints[4].severity, LintSeverity::Warning);
        assert_eq!(svc.lint("demo", &flow), lints);
    }

    #[test]
    fn lint_names_an_invalid_node_by_its_label() {
        let svc = FlowService::new(Box::new(UntouchedRepo));
        let flow = Flow {
            name: "Routing".to_string(),
            nodes: vec![node_of(
                "if1",
                FlowNodeKind::If {
                    label: "Logged in?".to_string(),
                    condition: "response.status === 200".to_string(),
                },
            )],
            edges: vec![],
            callback_host: None,
        };
        let lints = svc.lint("demo", &flow);
        assert_eq!(lints[0].code, INVALID_GRAPH);
        assert_eq!(lints[0].node_id.as_deref(), Some("if1"));
        assert!(
            lints[0].message.starts_with("'Logged in?': "),
            "got: {}",
            lints[0].message
        );
        assert!(lints[1..].iter().all(|l| l.severity == LintSeverity::Warning));
    }

    #[test]
    fn a_flow_with_warnings_still_saves_unchanged() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
        let flow = routing_flow();
        assert!(!svc.lint("demo", &flow).is_empty());
        svc.save("demo", flow.clone())
            .expect("a flow with warnings must still save");
        assert_eq!(svc.get("demo", "Routing").expect("get"), flow);
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_service::`
Expected: FAIL to compile (`no method named lint found for struct FlowService`).

- [ ] **Step 4: Implement `FlowService::lint`**

In `crates/rocket-app/src/flow_service.rs`, change the first line:

```rust
use rocket_flow::{validate, Flow, FlowGraphError, FlowRepository};
```

to:

```rust
use rocket_flow::lint::{graph_error_lints, validate_with_warnings, FlowLint, NoLintContext};
use rocket_flow::{validate, Flow, FlowGraphError, FlowRepository};
```

Then add after the `save` method (inside `impl FlowService`, after the closing `}` of `pub fn save`):

```rust
    /// Lints `flow` as given, not the saved file, and never fails. A
    /// structural `validate` failure comes first, as `invalid_graph` error
    /// lints that carry its node or edge ids. Then come the warnings of
    /// `validate_with_warnings`. Lints never block save or run.
    pub fn lint(&self, collection: &str, flow: &Flow) -> Vec<FlowLint> {
        // F-21 and F-22 build a context from `collection` here, for saved
        // requests and known variables. Until then no lint needs one.
        let _ = collection;
        let mut lints = match validate(flow) {
            Ok(_) => Vec::new(),
            Err(error) => graph_error_lints(flow, &error),
        };
        lints.extend(validate_with_warnings(flow, &NoLintContext));
        lints
    }
```

Do not change `save`, `graph_error_message` or the constructor.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_service::`
Expected: PASS (the existing save, rename and list tests plus 4 new ones).

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS.

- [ ] **Step 6: Check that save and run are untouched**

Run:

```bash
git diff --stat main -- crates/rocket-app/src/flow_execution_service.rs crates/rocket-app/src/flow_routing.rs crates/rocket-app/src/flow_partial.rs src-tauri src crates/rocket-flow/src/graph.rs crates/rocket-flow/src/node.rs crates/rocket-flow/src/flow.rs
```

Expected: no output.

Run: `cargo check -j4 -p rocket`
Expected: no errors (the Tauri package still builds against the extended `FlowService`).

- [ ] **Step 7: Document the tier**

In `crates/rocket-flow/CLAUDE.md`, add this row to the Module Map table after the `validate.rs` row:

```markdown
| `lint.rs` | Non-blocking lint tier: `validate_with_warnings(flow, ctx)`, `graph_error_lints`, `FlowLint`, `LintSeverity`, `LintContext` |
```

and add at the end of the file:

```markdown
## Lint tier (`lint.rs`)

`validate_with_warnings(flow, ctx)` warns about a graph that runs but may not
do what its author meant. It never blocks save or run, works on invalid
graphs too, and is linear in graph size (one `GraphIndex` per call). Rules:
`exit_without_edge` (If exits and Switch cases with no wire, one lint per
node), `switch_without_default`, `no_path_to_output` (only when the flow has
an Output; an Auth node with `apply_to_inherit` is exempt).
`graph_error_lints` turns a `validate` failure into `invalid_graph` error
lints, one per named node or wire. Output order is fixed: node file order,
then rule order. Messages name nodes by label and never quote values,
expressions, match values or auth fields. `LintContext` is the seam for
facts that need I/O (saved requests, known variables): the app layer answers
yes, no or `None`, and `None` never warns. No rule uses it yet (F-21, F-22).
```

In `crates/rocket-app/CLAUDE.md`, add at the end of the file:

```markdown
## Flow lint (`flow_service.rs`)

`FlowService::lint(collection, flow)` lints the graph it is given (the
canvas, saved or not) and never fails or touches the repository: a
`validate` failure first, as `invalid_graph` error lints, then the
`rocket_flow::validate_with_warnings` warnings. It passes `NoLintContext`
until F-21 and F-22 add a context built from the collection. `save` and the
run path do not call it, so lints never block either. The `lint_flow` IPC
command is plan P21.
```

- [ ] **Step 8: Gates and commit**

Run: `cargo check -j4 -p rocket-flow -p rocket-app && cargo clippy -j4 -p rocket-flow -p rocket-app`
Expected: no errors and no new warnings in `flow_service.rs` or `lint.rs`.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-app/src/flow_service.rs crates/rocket-flow/CLAUDE.md crates/rocket-app/CLAUDE.md`
Suggested subject: `feat(flow): lint the canvas graph in the flow service`.

---

## After the plan (main loop, in the main checkout)

These files are untracked and live only in the main checkout, so the implementer does not edit them:
- `.claude/flow-roadmap.md`: set F-20 to `done` with the commit range. In F-23's notes, record that its exit and reach rules shipped in P23 (the rest of F-23 is done too, so F-23 can be `done` as well). Add follow-ups from the final review.
- `docs/superpowers/plans/flow-tier3/00-plan-index.md`: add a P23 row (F-20, 3 tasks, needs none, sonnet) and change P21's "Needs" to "P23 (F-20), P12".
- Apply the P21 changes listed in "Deviations".

## Self-Review

- **Spec coverage:** F-20 core: the tier (`validate_with_warnings`, Task 1), the `ctx` seam implemented by the app layer with no I/O in `rocket-flow` (Task 1 trait, Task 3 `NoLintContext` until F-21 and F-22), node id, label in the message and a fix hint on every lint (Tasks 1 and 2). P21's contract: types, paths, codes, `FlowService::lint` signature and the `invalid_graph` mapping (Tasks 1 to 3). The three client rules P21 deletes are covered (mapping table, Tasks 1 and 2). F-21 to F-30 are not built.
- **Placeholders:** none. Every code step shows the code.
- **Type consistency:** `FlowLint`, `LintSeverity`, `LintContext`, `NoLintContext`, `validate_with_warnings(&Flow, &dyn LintContext) -> Vec<FlowLint>`, `graph_error_lints(&Flow, &FlowGraphError) -> Vec<FlowLint>` and `FlowService::lint(&self, &str, &Flow) -> Vec<FlowLint>` are the same in every task and match P21's "Required from F-20". Test helpers added in Task 2 build on Task 1's.
- **Review Focus coverage:** item 1 in Task 1 and Task 2 tests; items 2 and 3 in Task 2; item 4 in Tasks 1 and 2; item 5 in Tasks 2 and 3; item 6 in Task 3 (tests plus the diff check); item 7 by the trait shape and the `Cargo.toml` check in Task 1 Step 7.
