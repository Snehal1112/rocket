# Flow Plan 02: rocket-flow Graph Validation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `topological_sort(&Flow)` to `rocket-flow` — a pure, I/O-free
function that orders a flow's nodes so every node appears after everything it
depends on, and rejects malformed graphs (cycles, edges pointing at unknown
nodes) with a typed error instead of panicking or silently misordering.

**Architecture:** A single new module, `graph.rs`, implementing Kahn's
algorithm over the `Flow`/`FlowNode`/`FlowEdge` types from Plan 01. No new
public struct beyond the function and its error type — this is a pure
algorithm task, not a new entity.

**Tech Stack:** Rust, `thiserror` (new dependency for this crate).

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§7 — "Validation at save time, not just run time" and "Traversal"). Plan
index: `docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md`.

## Global Constraints

- Depends on Plan 01's `Flow`, `FlowNode`, `FlowEdge` types — do not modify
  their shape here; this plan only adds a new module that reads them.
- Add `thiserror.workspace = true` to `crates/rocket-flow/Cargo.toml`'s
  `[dependencies]` — it is already a workspace dependency (used by
  `rocket-collection`, `rocket-git`, `rocket-import`, `rocket-scripting`,
  `rocket-shared`, `rocket-infra`, `rocket-audit`), so no version needs
  choosing.
- `topological_sort` takes `&Flow` and returns `Result<Vec<String>, FlowGraphError>`
  — a plain `Result`, not `DomainResult`. This is a pure-algorithm error type
  distinct from `DomainError`; Plan 07 (Tauri `save_flow` command) is
  responsible for mapping a `FlowGraphError` into an IPC-facing error, not
  this crate.
- Every node id referenced by any edge (`source_node_id` or `target_node_id`)
  must exist in `flow.nodes` — an edge pointing at a missing node is
  `FlowGraphError::UnknownNode`, checked and returned before any topological
  work begins, not discovered mid-sort.
- Independent branches (nodes with no dependency relationship to each other)
  may appear in either relative order in the returned `Vec<String>` — do not
  write a test that asserts one specific tie-break order between them; assert
  only the relative ordering constraints that actually matter (a depends on
  b, so b comes before a).
- This repo's hard rule against panicking shorthand on `Result`/`Option` in
  production code paths applies here too — build the algorithm so every
  lookup into a map you constructed yourself from validated data is provably
  safe, and prefer `.expect("reason the invariant holds")` with a real reason
  over silent indexing if a fallible-looking lookup remains, in the rare case
  it is unavoidable. Test code uses `.expect("message")`, never the bare
  panicking shorthand.

## Review Focus

- A simple linear chain (A → B → C) sorts to exactly `[A, B, C]` (only one
  valid order exists here, so this one case may assert the exact sequence).
- A diamond (A → B, A → C, B → D, C → D) places A before both B and C, and
  both B and C before D — asserting relative positions via index comparison,
  not the exact `Vec` value, since B/C order is not fixed.
- A single-node self-loop (an edge whose `source_node_id` and
  `target_node_id` are the same node) is reported as
  `FlowGraphError::Cycle` containing that node's id.
- A longer cycle (A → B → C → A) is reported as `FlowGraphError::Cycle`
  containing all three node ids (order within the `Vec<String>` inside the
  error is not asserted, only membership).
- An edge whose `source_node_id` or `target_node_id` does not match any
  `flow.nodes[].id` returns `FlowGraphError::UnknownNode` naming that
  specific missing id, and does so without panicking.
- A flow with nodes but zero edges returns `Ok` containing every node id
  exactly once, in any order.
- An empty flow (`nodes: vec![]`, `edges: vec![]`) returns `Ok(vec![])`.

---

## Task 1: `topological_sort` + `FlowGraphError`

**Files:**
- Create: `crates/rocket-flow/src/graph.rs`
- Modify: `crates/rocket-flow/src/lib.rs`
- Modify: `crates/rocket-flow/Cargo.toml`

**Interfaces:**
- Consumes: `Flow`, `FlowNode`, `FlowEdge` from Plan 01.
- Produces: `topological_sort(flow: &Flow) -> Result<Vec<String>, FlowGraphError>`,
  `FlowGraphError::{Cycle { node_ids }, UnknownNode { node_id }}` — consumed
  by Plan 06 (`FlowExecutionService`'s run-time traversal) and Plan 07
  (`save_flow`'s save-time validation).

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

This algorithm operates directly on `Flow`/`FlowEdge`, which reference
collection requests via `RequestSource::Saved.request_path` — re-confirm
before writing tests that this module still has zero cross-domain-crate
dependencies (it must not need to know anything about the collection tree
itself, only the plain string ids already on `FlowNode`/`FlowEdge`).

- [ ] **Step 2: Add the `thiserror` dependency**

In `crates/rocket-flow/Cargo.toml`, add to `[dependencies]`:

```toml
thiserror.workspace = true
```

- [ ] **Step 3: Write the failing tests**

```rust
// crates/rocket-flow/src/graph.rs
use crate::flow::{Flow, FlowEdge, FlowNode};
use crate::node::{FlowNodeKind, NodePosition};
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
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

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
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p rocket-flow graph::tests -j4`
Expected: FAIL — the `todo!()` body panics on every test that reaches it
(the two `empty`/basic-shape tests will panic too, since `todo!()` runs
unconditionally).

- [ ] **Step 5: Implement `topological_sort`**

```rust
// crates/rocket-flow/src/graph.rs — replace the `todo!()` body
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
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-flow -j4`
Expected: PASS — 19 tests total (11 from Plan 01, 8 from this task).

- [ ] **Step 7: Register the module export**

In `crates/rocket-flow/src/lib.rs`:

```rust
pub mod flow;
pub mod graph;
pub mod node;

pub use flow::{Flow, FlowEdge, FlowNode, FlowRepository};
pub use graph::{topological_sort, FlowGraphError};
pub use node::{FlowNodeKind, InlineHeader, InlineRequestData, NodePosition, RequestSource};
```

Run: `cargo check -p rocket-flow -j4`
Expected: succeeds.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-flow
git commit -m "feat(flow): add topological_sort and FlowGraphError"
```

---

## Next Plan

[Plan 03: FsFlowRepo persistence](2026-09-27-flow-visual-workflow-builder-plan-03-persistence.md) —
implements `FlowRepository` on top of `<collection>/flows/<slug>.yml` files
in `rocket-infra`, mirroring `FsCollectionRepo`'s conventions.

## Post-Implementation Review

Before starting Plan 03, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-flow/Cargo.toml`, `crates/rocket-flow/src/lib.rs`,
> `crates/rocket-flow/src/graph.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interface — does `topological_sort`
>    match exactly what
>    `docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md`'s
>    locked interface contract promises Plans 06/07 will consume (signature,
>    error variants and their field names)?
> 2. Code quality — no panicking shorthand on `Result`/`Option` in the
>    production algorithm itself (every `.expect(...)` call must have a
>    correct, provable justification in its message given the code around
>    it — verify each one), test coverage versus this plan's Review Focus
>    section (linear chain, diamond, self-loop, longer cycle, unknown
>    source/target node, no-edges, empty flow).
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    `rocket-flow` still has zero cross-domain-crate dependencies and no I/O
>    after this addition.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-flow -j4` and
> `cargo check -p rocket-flow -j4`, and confirm they still pass. Report what
> you found and fixed.

Only proceed to Plan 03 once this review comes back clean (or its fixes are
applied and re-verified).
