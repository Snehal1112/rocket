# Run From Node, Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the engine re-run one node ("Run this node") or one node and everything below it ("Run from here") on top of an earlier run's cached results, refusing with a clear message whenever the cache cannot serve the run safely.

**Architecture:** Three new pieces in `rocket-app`. `flow_partial.rs` is a pure planner: it picks the nodes to run, the cached "seed" nodes that feed them, and refuses unsafe plans. `flow_run_cache.rs` keeps the raw results of the last 8 runs in memory (64 MiB budget), with Merkle-style fingerprints to detect upstream edits and the earlier run's masking secrets. `FlowExecutionService::run_partial` checks the plan before any secret fetch or event, seeds `captured` and `outcomes`, then runs the existing loop over the planned nodes only. Every run, full or partial, records its results in the cache when it ends.

**Tech Stack:** Rust (`rocket-app`, `rocket-shared`, `rocket-flow`), tokio tests, `std::collections::hash_map::DefaultHasher`, serde_json for canonical fingerprint text.

**Spec:** Roadmap item F-41 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` section "P19 Run from node, backend (F-41)", corrected below.

**Decisions assumed (plan index, change them before this plan runs):**
- D1: refuse. A partial run never re-runs ancestors. When a needed input has no usable cached value, the run is refused and the message says "Run the full flow or Run from '<node>'".
- D5: refuse on upstream change. Any edit to a node upstream of the run, including a saved request file it reads, refuses the run. Changes to environment, global or collection variable values are NOT detected. This is a documented limitation.

## Design corrections (checked against HEAD b047bbc6)

The design notes were checked against the code. These points differ, and this plan follows the code:

1. Masking keys. The notes store the base run's secrets under a `prev-run.` prefix and say the prefix "stops them resolving references". It does not: `VariableContext::flatten` puts every `external_secrets` key into the variable map (`crates/rocket-environment/src/context.rs:34`), so `{{prev-run.x}}` would resolve to an old token. (`{{flow-auth.<id>}}` already resolves today, which is existing behaviour and out of scope.) This plan uses keys that start with `}}` (`}}prev-run.<n>`). The resolver ends a name at the first `}}` (`crates/rocket-environment/src/resolver.rs:26-38`), so no template can reach them.
2. What to mask. The notes keep only the base run's `external_secrets`. That misses secret environment, global and collection variables (masked through `secret_values`, `execution_service.rs:648-652` and `:658-673`) and the per-request `flow-auth-sent.<id>` values, which live only in a request-local map (`flow_execution_service.rs:1314-1336`). This plan stores `secret_values(...)` plus `credentials.secret_forms()` plus every sent credential (a new `ExecutedNode::sent_secret`).
3. Fingerprints. `rocket_collection::Request::uid` defaults to a fresh `generate_uid()` when the file has none (`crates/rocket-collection/src/request.rs:16-17`), so hashing a saved request as loaded would change on every load. This plan clears `uid`, `file_name` and `seq` first and hashes a key-sorted JSON form, so map order cannot change the hash. Node `position` is not hashed, so moving a node is not an edit.
4. Cancel. A cancelled node records a step but no outcome (the loop breaks before `outcomes.insert`, `flow_execution_service.rs:1098-1103`). The merged cache entry must treat it as not run and mark it stale, not merge it.
5. Wait sender rule. V15 already forces every inline sender of `{{callback.<name>}}` to be an ancestor of its Wait (`crates/rocket-flow/src/validate.rs:388-431`), but skips saved requests. So "Run from here" on a Wait with an upstream sender is always refused, and the runtime check must also read saved requests. `mentions_callback` is made `pub` in `rocket-flow` and reused.
6. Auth nodes. `resolve_flow_credentials` resolves every Auth node of the flow, not only those in the run (`flow_execution_service.rs:954-961`). A partial run therefore needs the same tokens as a full run. The UI preflight already collects them for every Auth node.
7. API shape. `RunFlowInput` is not changed, because 11 struct literals use it. A new `run_partial(exec, input, auth_tokens, partial)` sits next to `run_with_auth`, and both call one private `run_inner`. The mode enum lives in `rocket-shared` as `FlowPartialMode { Node, FromHere }` (wire `"node"` and `"fromHere"`), so the event, the summary and the P20 DTO share one type.
8. P7 dependency. This plan does not use `NodeTrace`. Its only `execute_node` change is the Request arm's return value. Run it after P7 and P9 only because all three edit `execute_node` and the run loop. Line numbers below are from b047bbc6. Use the quoted code as anchors and keep every line P7 and P9 added.
9. `execute_node` is at `:1177`, not `:1166`.

## Global Constraints

- Every task starts with: Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.
- No `unwrap()` or `expect()` in production code. Tests may use `expect("reason")`.
- Cargo commands always pass `-j4` and `-p <crate>`. Never `--workspace` or `--all`.
- Code comments are short full sentences that end with a punctuation mark.
- Types holding raw outputs or secrets get no derived `Debug`, or a redacting one (`CachedRun`).
- Raw cached outputs never cross IPC and are never persisted. Nothing in this plan adds an IPC command (that is P20).
- `serde(rename_all = "camelCase")` only on the new IPC-facing event types in `rocket-shared`, never on persistence structs.
- Commits use conventional commit format through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- One implementer per worktree. P7, P9 and P19 all edit `flow_execution_service.rs`; run them one at a time.
- Risk: the user is building Flow auth changes on another PC (project memory) that touch `run_with_auth` and `flow_auth.rs`. Task 3 starts with a pre-flight check.
- Not in scope: IPC DTO, TypeScript, UI (P20); "run to here" (re-running ancestors, later work on the same machinery); detecting variable value changes.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A cached response holds a token from the base run that has since rotated. The partial run must still mask it in every reported value, and no template may resolve the old value. Pinned: Task 2 `a_previous_run_secret_key_cannot_be_referenced_from_a_template`, Task 3 `a_rotated_token_in_a_cached_response_stays_masked`.
2. A node upstream of the run, or a saved request it reads, changed since the base run. The run must be refused before any event, naming the edited node, while an edit to the start node itself is allowed. Pinned: Task 2 `an_upstream_edit_refuses_and_names_the_edited_node`, `saved_request_text_ignores_uid_file_name_and_seq`, Task 3 `an_upstream_edit_since_the_base_run_refuses_with_no_events`.
3. The start node's input came from a branch not taken, or from a failed request with no response. The run must be refused, not run with a missing value. Pinned: Task 1 `a_start_node_fed_by_a_not_taken_branch_is_refused`, `a_start_node_fed_by_a_failed_request_is_refused`.
4. A Wait in the run whose callback sender is outside the run would wait for a URL nobody received. Pinned: Task 1 `a_wait_whose_sender_is_outside_the_run_is_refused`, Task 3 `run_from_a_wait_with_an_upstream_sender_is_refused`.
5. Two partial runs from one base, and a cancelled partial run, must never corrupt the base entry, and nodes they did not reach must not seed later runs. Pinned: Task 2 `a_partial_entry_overlays_its_nodes_and_leaves_the_base_untouched`, Task 3 `two_partial_runs_from_one_base_both_work_and_leave_it_intact`, `a_cancelled_partial_run_marks_unreached_nodes_stale`.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-shared/src/events.rs` (modify) | `FlowPartialMode`, `FlowPartialRunInfo` (Task 1); `partial` on `FlowRunStarted` (Task 3). |
| `crates/rocket-flow/src/validate.rs` (modify) | `mentions_callback` becomes `pub`. |
| `crates/rocket-app/src/flow_partial.rs` (new) | `PartialRun`, `SeedView`, `PartialPlan`, `PartialRefusal`, `select_nodes`, `check_seeds`, `callback_senders`. Pure. |
| `crates/rocket-app/src/flow_run_cache.rs` (new) | `CachedRun`, `CachedNode`, `RunResults`, `FlowRunCache` (LRU and byte budget), `fingerprints`, `saved_request_text`, `previous_run_secret_key`. Pure. |
| `crates/rocket-app/src/flow_execution_service.rs` (modify) | `run_partial`, `run_inner`, `prepare_partial`, `remember_run`, `clear_run_cache`, `ExecutedNode::sent_secret`, `FlowRunSummary::partial`. |
| `crates/rocket-app/src/flow_poll.rs` (modify) | One `ExecutedNode` literal gains `sent_secret: None`. |
| `crates/rocket-app/src/flow_partial_run_tests.rs` (new) | End-to-end partial-run tests, a child test module of `flow_execution_service`. |
| `crates/rocket-app/src/lib.rs` (modify) | Module declarations and `pub use flow_partial::PartialRun`. |
| `crates/rocket-app/CLAUDE.md` (modify) | Short "Partial runs" section. |

Existing tests to know: `flow_routing.rs` tests (style of pure tests), `flow_execution_service.rs` tests `poll_stops_between_attempts_when_cancelled` (mid-run cancel pattern) and `an_auth_node_succeeds_without_reporting_its_token_and_an_output_masks_it` (masking pattern), `crate::test_doubles` (`RecordingExecutor`, `RecordingPublisher`, `FakeCallbackListener`, `InMemoryCollectionRepo`).

---

### Task 1: Pure planner `flow_partial.rs`

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (new types after `FlowLogEntry`, near line 36; new test in the `tests` module near line 875)
- Modify: `crates/rocket-flow/src/validate.rs:433-434`
- Create: `crates/rocket-app/src/flow_partial.rs`
- Modify: `crates/rocket-app/src/lib.rs` (module list near line 20, re-exports near line 68)

**Interfaces:**
- Produces: `rocket_shared::events::FlowPartialMode { Node, FromHere }` (serde `"node"`, `"fromHere"`), `FlowPartialRunInfo { base_run_id, start_node_id, mode, node_ids }` (camelCase).
- Produces: `pub struct PartialRun { pub base_run_id: String, pub start_node_id: String, pub mode: FlowPartialMode }`, re-exported as `rocket_app::PartialRun`.
- Produces: `pub(crate) struct SeedView { outcome: NodeOutcome, has_output: bool, stale: bool }`.
- Produces: `pub(crate) struct PartialPlan { run_order: Vec<String>, seeds: Vec<String>, dropped_edges: HashSet<String> }`.
- Produces: `pub(crate) struct PartialRefusal { message, node_ids, edge_ids }` with `From<PartialRefusal> for DomainError` (message ends `— node(s): a, b; edge(s): e1`).
- Produces: `select_nodes(flow, order, partial, senders) -> Result<PartialPlan, PartialRefusal>`, `check_seeds(flow, plan, start_node_id, base) -> Result<(), PartialRefusal>`, `callback_senders(flow, saved) -> HashMap<String, Vec<String>>`, `is_free_node(kind)`, `node_label(node)`, `label_in(flow, id)`.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing wire-shape test for the shared types**

In `crates/rocket-shared/src/events.rs`, inside `mod tests`, after `flow_run_started_wire_shape`:

```rust
    #[test]
    fn flow_partial_run_info_wire_shape() {
        let info = FlowPartialRunInfo {
            base_run_id: "01A".into(),
            start_node_id: "n2".into(),
            mode: FlowPartialMode::FromHere,
            node_ids: vec!["n2".into(), "n3".into()],
        };
        let json = serde_json::to_string(&info).expect("serialize");
        assert_eq!(
            json,
            r#"{"baseRunId":"01A","startNodeId":"n2","mode":"fromHere","nodeIds":["n2","n3"]}"#
        );
        let back: FlowPartialRunInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, info);
        assert_eq!(
            serde_json::to_string(&FlowPartialMode::Node).expect("serialize"),
            r#""node""#
        );
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test -j4 -p rocket-shared flow_partial_run_info_wire_shape`
Expected: FAIL to compile, `FlowPartialRunInfo` not found.

- [ ] **Step 4: Add the shared types**

In `crates/rocket-shared/src/events.rs`, after the `FlowLogEntry` struct:

```rust
/// Which nodes a partial Flow run executes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowPartialMode {
    /// "Run this node": the start node and the Input and Auth nodes it reads.
    Node,
    /// "Run from here": the start node and every node downstream of it.
    FromHere,
}

/// Describes a partial run on `FlowRunStarted` and on the run summary.
/// Nested keys are camelCase, like `FlowDebugRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowPartialRunInfo {
    /// The run whose cached results feed this one.
    pub base_run_id: String,
    pub start_node_id: String,
    pub mode: FlowPartialMode,
    /// Every node this run executes, in execution order.
    pub node_ids: Vec<String>,
}
```

- [ ] **Step 5: Run it to verify it passes**

Run: `cargo test -j4 -p rocket-shared flow_partial_run_info_wire_shape`
Expected: PASS.

- [ ] **Step 6: Make `mentions_callback` public**

In `crates/rocket-flow/src/validate.rs`, change the function at line 433-434 to:

```rust
/// True when `text` holds the variable `{{callback.<name>}}`, spaces allowed.
/// The engine reuses it to find the requests that send a Wait's callback URL.
pub fn mentions_callback(text: &str, name: &str) -> bool {
```

Run: `cargo test -j4 -p rocket-flow v15`
Expected: PASS (existing V15 tests).

- [ ] **Step 7: Write the failing planner tests**

Create `crates/rocket-app/src/flow_partial.rs` with only the test module first (the module body comes in Step 9):

```rust
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
}
```

Add the module to `crates/rocket-app/src/lib.rs`, after `pub mod flow_execution_service;`:

```rust
// Used only by tests until Task 3 of plan P19 wires it into the engine.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) mod flow_partial;
```

- [ ] **Step 8: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_partial`
Expected: FAIL to compile (`select_nodes`, `PartialRun` and the other items do not exist).

- [ ] **Step 9: Write the planner**

Put this above the test module in `crates/rocket-app/src/flow_partial.rs`:

```rust
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
            let Some(request) = saved.get(request_path) else {
                return Vec::new();
            };
            let mut texts = vec![request.url.clone()];
            for header in &request.headers {
                texts.push(header.key.clone());
                texts.push(header.value.clone());
            }
            texts.extend(request.query_params.iter().map(|q| q.value.clone()));
            texts.extend(request.body.as_ref().and_then(|b| b.content.clone()));
            texts.extend(request.pre_request_script.clone());
            texts
        }
    }
}
```

In `crates/rocket-app/src/lib.rs`, after the `pub use flow_execution_service::{...};` block, add:

```rust
pub use flow_partial::PartialRun;
```

- [ ] **Step 10: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_partial`
Expected: PASS (17 tests).

- [ ] **Step 11: Gates and commit**

Run: `cargo check -j4 -p rocket-app && cargo check -j4 -p rocket-flow && cargo clippy -j4 -p rocket-app --tests -- -D warnings`
Expected: no errors, no clippy warnings.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-shared/src/events.rs crates/rocket-flow/src/validate.rs crates/rocket-app/src/flow_partial.rs crates/rocket-app/src/lib.rs`
Suggested subject: `feat(flow): plan partial runs from cached results`.

---

### Task 2: Run cache `flow_run_cache.rs`

**Files:**
- Create: `crates/rocket-app/src/flow_run_cache.rs`
- Modify: `crates/rocket-app/src/lib.rs` (module list)

**Interfaces:**
- Consumes: `SeedView`, `PartialRefusal`, `is_free_node`, `label_in` from Task 1; `CapturedOutput`, `RunFlowInput` from `flow_execution_service`; `NodeOutcome` from `flow_routing`.
- Produces: `MAX_CACHED_RUNS = 8`, `CACHE_BYTE_BUDGET = 64 MiB`, `MAX_CACHED_OUTPUT_BYTES = 16 MiB`.
- Produces: `CachedNode { outcome, output: Option<Arc<CapturedOutput>>, fingerprint: u64, stale: bool }`, `CachedRun { scope: RunFlowInput, nodes: HashMap<String, CachedNode>, masking_secrets: HashSet<String> }` with a counts-only `Debug`.
- Produces: `RunResults { outcomes, captured, masking_secrets }`.
- Produces: `CachedRun::from_full_run(scope, flow, results, fingerprints)`, `merge_partial(&self, scope, flow, start_node_id, results, fingerprints)`, `seed_views()`, `check_scope(scope, start_node_id)`, `changed_roots(flow, seeds, current)`, `check_unchanged(flow, seeds, current)`.
- Produces: `fingerprints(flow, order, saved_request: &dyn Fn(&str) -> Option<String>) -> HashMap<String, u64>`, `saved_request_text(&Request) -> String`, `output_size(&CapturedOutput) -> usize`, `previous_run_secret_key(usize) -> String`.
- Produces: `FlowRunCache::{new, with_limits, get, insert, clear, contains}`.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

Create `crates/rocket-app/src/flow_run_cache.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_flow::{handle, InlineRequestData, NodePosition};
    use rocket_shared::types::HttpMethod;
    use rocket_shared::VariableValue;

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

    /// `ids[0] -url-> ids[1] -url-> ...`, with edge ids e0, e1, ...
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
        let vars = HashMap::from([(key, "old-token-123456".to_string())]);
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
```

Add the module to `crates/rocket-app/src/lib.rs`, after `pub(crate) mod flow_routing;`:

```rust
// Used only by tests until Task 3 of plan P19 wires it into the engine.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) mod flow_run_cache;
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_run_cache`
Expected: FAIL to compile (`fingerprints`, `CachedRun` and the other items do not exist).

- [ ] **Step 4: Write the cache**

Put this above the test module in `crates/rocket-app/src/flow_run_cache.rs`:

```rust
//! In-memory results of recent Flow runs, so a partial run can reuse them
//! (`flow_partial`). Outputs are raw and unmasked: nothing here is
//! persisted, sent over IPC or printed. `CachedRun`'s `Debug` shows counts.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rocket_collection::Request;
use rocket_flow::{Flow, FlowEdge, FlowNode, FlowNodeKind, RequestSource};

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

/// Bytes an output holds: body, base64 body and headers.
pub(crate) fn output_size(output: &CapturedOutput) -> usize {
    match output {
        CapturedOutput::Request(out) => {
            let response = &out.response;
            response.body.len()
                + response.body_base64.as_ref().map_or(0, String::len)
                + response
                    .headers
                    .iter()
                    .map(|h| h.key.len() + h.value.len())
                    .sum::<usize>()
        }
        CapturedOutput::Value(value) => value.data().len(),
    }
}

fn index(flow: &Flow) -> HashMap<&str, &FlowNode> {
    flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect()
}

fn cached_node(
    flow_node: Option<&FlowNode>,
    outcome: NodeOutcome,
    output: Option<CapturedOutput>,
    fingerprint: u64,
) -> CachedNode {
    // Input and Auth nodes always run again, and an Auth output is a token.
    let never_seeded = flow_node.is_some_and(|n| is_free_node(&n.kind));
    let output = output
        .filter(|o| !never_seeded && output_size(o) <= MAX_CACHED_OUTPUT_BYTES)
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
        let nodes_by_id = index(flow);
        let RunResults {
            outcomes,
            mut captured,
            masking_secrets,
        } = results;
        let mut nodes = self.nodes.clone();
        let ran: HashSet<String> = outcomes.keys().cloned().collect();
        for (id, outcome) in outcomes {
            let node = cached_node(
                nodes_by_id.get(id.as_str()).copied(),
                outcome,
                captured.remove(&id),
                fingerprints.get(&id).copied().unwrap_or_default(),
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

fn canonical_text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .map(|v| canonical(v).to_string())
        .unwrap_or_default()
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
/// process.
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_run_cache`
Expected: PASS (14 tests).

- [ ] **Step 6: Gates and commit**

Run: `cargo check -j4 -p rocket-app && cargo clippy -j4 -p rocket-app --tests -- -D warnings`
Expected: no errors, no clippy warnings.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-app/src/flow_run_cache.rs crates/rocket-app/src/lib.rs`
Suggested subject: `feat(flow): keep recent run results for partial runs`.

---

### Task 3: Wire partial runs into the engine

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (imports `:1-21` and `:666-672`, `ExecutedNode` `:31-53`, `FlowRunSummary` `:740-747`, `FlowExecutionService` fields `:799-833`, `run_with_auth` `:918-1142`, Request arm `:1309-1382`, If `:1406-1411`, Switch `:1431-1436`, test literal `:4438-4446`, end of file)
- Modify: `crates/rocket-app/src/flow_poll.rs:159-167`
- Modify: `crates/rocket-shared/src/events.rs` (`FlowRunStarted` `:270-275`, test `:864-875`)
- Create: `crates/rocket-app/src/flow_partial_run_tests.rs`
- Modify: `crates/rocket-app/src/lib.rs` (remove the two `cfg_attr` lines from Tasks 1 and 2)
- Modify: `crates/rocket-app/CLAUDE.md` (after the "Flow Auth nodes" section)

**Interfaces:**
- Consumes: everything from Tasks 1 and 2.
- Produces: `pub async fn run_partial(&self, exec, input: RunFlowInput, auth_tokens: FlowAuthTokens, partial: PartialRun) -> DomainResult<FlowRunSummary>`.
- Produces: `pub fn clear_run_cache(&self)` (P20 calls it on workspace switch).
- Produces: `FlowRunSummary::partial: Option<FlowPartialRunInfo>` (camelCase `partial`, omitted when `None`), `DomainEvent::FlowRunStarted::partial: Option<FlowPartialRunInfo>` (omitted when `None`).

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Pre-flight check for the parallel Flow auth work**

The user is changing Flow auth on another machine (project memory). Run:

```bash
git fetch --all --prune
git log --oneline main -8 -- crates/rocket-app/src/flow_auth.rs crates/rocket-app/src/flow_execution_service.rs
git log --oneline HEAD..main -- crates/rocket-app/src/flow_auth.rs crates/rocket-app/src/flow_execution_service.rs
git diff main...HEAD --stat -- crates/rocket-app/src/flow_auth.rs crates/rocket-app/src/flow_execution_service.rs
git status --short crates/rocket-app/src/
```

Expected: no commits on `main` that this branch lacks, and no uncommitted changes in `crates/rocket-app/src/`. If `main` has new commits touching `run_with_auth` or `resolve_flow_credentials`, stop and rebase onto `main` first (ask the human partner before rebasing a shared branch). If the credential block `let credentials = resolve_flow_credentials(` ... `external_secrets.insert(format!("flow-auth.{node_id}"), ...)` no longer exists in this shape, stop and report to the human partner instead of guessing.

- [ ] **Step 3: Write the failing end-to-end tests**

Create `crates/rocket-app/src/flow_partial_run_tests.rs`:

```rust
//! End-to-end tests for partial runs ("Run this node", "Run from here").

use super::*;

use async_trait::async_trait;
use rocket_collection::Collection;
use rocket_flow::{FlowRepository, NodePosition};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::events::FlowPartialMode;

use crate::flow_auth::SuppliedToken;
use crate::flow_partial::PartialRun;
use crate::test_doubles::{
    EmptySecretManagerRepo, FakeCallbackListener, InMemoryCollectionRepo, InMemoryHistoryRepo,
    NullCookieRepo, NullEnvRepo, RecordingExecutor, RecordingPublisher, SharedCollectionRepo,
    SharedExecutor, SharedHistoryRepo, SharedPublisher,
};

const FLOW: &str = "partial";

/// A flow repo the test can edit between runs.
#[derive(Clone, Default)]
struct SharedFlowRepo(Arc<Mutex<HashMap<String, rocket_flow::Flow>>>);

impl SharedFlowRepo {
    fn put(&self, flow: rocket_flow::Flow) {
        self.0.lock().expect("lock").insert(flow.name.clone(), flow);
    }
    fn edit(&self, change: impl FnOnce(&mut rocket_flow::Flow)) {
        if let Some(flow) = self.0.lock().expect("lock").get_mut(FLOW) {
            change(flow);
        }
    }
}

impl FlowRepository for SharedFlowRepo {
    fn list(&self, _collection: &str) -> DomainResult<Vec<String>> {
        Ok(self.0.lock().expect("lock").keys().cloned().collect())
    }
    fn get(&self, _collection: &str, name: &str) -> DomainResult<rocket_flow::Flow> {
        self.0
            .lock()
            .expect("lock")
            .get(name)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(name.to_string()))
    }
    fn save(&self, _collection: &str, flow: &rocket_flow::Flow) -> DomainResult<()> {
        self.put(flow.clone());
        Ok(())
    }
    fn delete(&self, _collection: &str, name: &str) -> DomainResult<()> {
        self.0.lock().expect("lock").remove(name);
        Ok(())
    }
}

/// Answers every wire expression with the body of the response it reads.
struct BodyEngine;

#[async_trait]
impl ScriptEngine for BodyEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        let body = ctx
            .response
            .as_ref()
            .map(|r| r.body.clone())
            .unwrap_or_default();
        let mut vars = HashMap::new();
        vars.insert("__jsonq_result__".to_string(), serde_json::json!(body));
        Ok(ScriptResult {
            runtime_vars: vars,
            ..Default::default()
        })
    }
}

struct Harness {
    flows: SharedFlowRepo,
    http: Arc<RecordingExecutor>,
    events: Arc<RecordingPublisher>,
    callbacks: Arc<FakeCallbackListener>,
    service: FlowExecutionService,
    exec: RequestExecutionService,
}

fn harness(flow: rocket_flow::Flow) -> Harness {
    let flows = SharedFlowRepo::default();
    flows.put(flow);
    let http = RecordingExecutor::new();
    let events = RecordingPublisher::new();
    let callbacks = FakeCallbackListener::new();
    let service = FlowExecutionService::new(
        Box::new(flows.clone()),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            Collection::new("my-api"),
        ))),
        Box::new(SharedPublisher(Arc::clone(&events))),
    )
    .with_callback_listener(Box::new(Arc::clone(&callbacks)));
    let exec = RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(SharedExecutor(Arc::clone(&http))),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            Collection::new("my-api"),
        ))),
        Box::new(NullCookieRepo),
        Box::new(SharedPublisher(RecordingPublisher::new())),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(BodyEngine));
    Harness {
        flows,
        http,
        events,
        callbacks,
        service,
        exec,
    }
}

fn at(id: &str, kind: FlowNodeKind) -> FlowNode {
    FlowNode {
        id: id.to_string(),
        kind,
        position: NodePosition { x: 0.0, y: 0.0 },
    }
}

fn request(id: &str, url: &str) -> FlowNode {
    at(
        id,
        FlowNodeKind::Request {
            debug: false,
            repeat_until: None,
            label: id.to_string(),
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

fn output(id: &str) -> FlowNode {
    at(
        id,
        FlowNodeKind::Output {
            label: id.to_string(),
        },
    )
}

fn wait(id: &str, name: &str) -> FlowNode {
    at(
        id,
        FlowNodeKind::WaitForCallback {
            label: id.to_string(),
            name: name.to_string(),
            timeout_ms: 60_000,
            accept_when: None,
        },
    )
}

fn sign_in(id: &str) -> FlowNode {
    at(
        id,
        FlowNodeKind::Auth {
            label: id.to_string(),
            auth: rocket_shared::types::Auth::OAuth2(Box::new(
                crate::flow_auth::test_support::authorization_code(),
            )),
            apply_to_inherit: true,
        },
    )
}

fn wire(id: &str, from: &str, to: &str, field: &str) -> FlowEdge {
    FlowEdge {
        id: id.to_string(),
        source_node_id: from.to_string(),
        target_node_id: to.to_string(),
        target_field: field.to_string(),
        expression: "response.body".to_string(),
        source_handle: handle::RESULT.to_string(),
    }
}

fn trigger(id: &str, from: &str, to: &str) -> FlowEdge {
    FlowEdge {
        id: id.to_string(),
        source_node_id: from.to_string(),
        target_node_id: to.to_string(),
        target_field: handle::TRIGGER.to_string(),
        expression: String::new(),
        source_handle: handle::RESULT.to_string(),
    }
}

fn flow(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> rocket_flow::Flow {
    rocket_flow::Flow {
        name: FLOW.to_string(),
        nodes,
        edges,
        callback_host: None,
    }
}

fn input() -> RunFlowInput {
    RunFlowInput {
        collection: "my-api".to_string(),
        flow_name: FLOW.to_string(),
        environment_name: None,
        global_env_name: None,
    }
}

fn partial(base: &str, start: &str, mode: FlowPartialMode) -> PartialRun {
    PartialRun {
        base_run_id: base.to_string(),
        start_node_id: start.to_string(),
        mode,
    }
}

fn tokens(token: &str) -> FlowAuthTokens {
    HashMap::from([(
        "au".to_string(),
        SuppliedToken {
            access_token: token.to_string(),
        },
    )])
}

fn node_ids(summary: &FlowRunSummary) -> Vec<&str> {
    summary.steps.iter().map(|s| s.node_id.as_str()).collect()
}

fn step<'s>(summary: &'s FlowRunSummary, id: &str) -> &'s FlowStepResult {
    summary
        .steps
        .iter()
        .find(|s| s.node_id == id)
        .expect("step recorded")
}

/// login -url-> b, where login answers with the URL b should call.
fn login_then_b() -> rocket_flow::Flow {
    flow(
        vec![
            request("a", "https://api.example.com/login"),
            request("b", "https://api.example.com/placeholder"),
        ],
        vec![wire("e1", "a", "b", "url")],
    )
}

#[tokio::test]
async fn run_this_node_reuses_the_cached_upstream_response() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let base = h.service.run(&h.exec, input()).await.expect("base run");

    let summary = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect("partial run");

    let sent = h.http.sent_urls();
    assert_eq!(
        sent.iter().filter(|u| u.contains("/login")).count(),
        1,
        "the cached request is not sent again: {sent:?}"
    );
    assert!(sent.last().is_some_and(|u| u.contains("/profile")), "{sent:?}");
    assert_eq!(node_ids(&summary), vec!["b"]);
    assert_ne!(summary.run_id, base.run_id);
    let info = summary.partial.clone().expect("partial info");
    assert_eq!(info.base_run_id, base.run_id);
    assert_eq!(info.node_ids, vec!["b".to_string()]);
    let started = h
        .events
        .events()
        .into_iter()
        .find_map(|e| match e {
            DomainEvent::FlowRunStarted {
                run_id,
                total_nodes,
                partial,
                ..
            } if run_id == summary.run_id => Some((total_nodes, partial)),
            _ => None,
        })
        .expect("started event");
    assert_eq!(started, (1, Some(info)));
    let finished = h
        .events
        .events()
        .into_iter()
        .find_map(|e| match e {
            DomainEvent::FlowRunFinished {
                run_id, node_count, ..
            } if run_id == summary.run_id => Some(node_count),
            _ => None,
        })
        .expect("finished event");
    assert_eq!(finished, 1);
}

#[tokio::test]
async fn an_upstream_edit_since_the_base_run_refuses_with_no_events() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let base = h.service.run(&h.exec, input()).await.expect("base run");
    h.flows.edit(|f| f.nodes[0] = request("a", "https://api.example.com/login?v=2"));
    let events_before = h.events.events().len();
    let sent_before = h.http.sent_urls().len();

    let err = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect_err("a changed");

    let text = err.to_string();
    assert!(text.contains("'a' changed since the earlier run"), "{text}");
    assert!(text.ends_with("node(s): a; edge(s): "), "{text}");
    assert_eq!(h.events.events().len(), events_before, "a refused run sends no events");
    assert_eq!(h.http.sent_urls().len(), sent_before);
}

#[tokio::test]
async fn editing_the_start_node_itself_is_allowed() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let base = h.service.run(&h.exec, input()).await.expect("base run");
    h.flows.edit(|f| f.nodes[1] = request("b", "https://api.example.com/other"));

    h.service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect("only the start node changed");
}

#[tokio::test]
async fn a_rotated_token_in_a_cached_response_stays_masked() {
    // The login response echoes the token of the base run. The token has
    // rotated by the time the partial run reads that cached response.
    let h = harness(flow(
        vec![
            sign_in("au"),
            request("login", "https://api.example.com/login"),
            output("out"),
        ],
        vec![wire("e1", "login", "out", "value")],
    ));
    h.http.set_body("/login", r#"{"echo":"token-AAAA-123456"}"#);
    let base = h
        .service
        .run_with_auth(&h.exec, input(), tokens("token-AAAA-123456"))
        .await
        .expect("base run");
    let base_value = step(&base, "out").value.clone().expect("base value");
    assert!(!base_value.contains("token-AAAA-123456"), "{base_value}");

    let summary = h
        .service
        .run_partial(
            &h.exec,
            input(),
            tokens("token-BBBB-654321"),
            partial(&base.run_id, "out", FlowPartialMode::Node),
        )
        .await
        .expect("partial run");

    let value = step(&summary, "out").value.clone().expect("value");
    assert!(!value.contains("token-AAAA-123456"), "{value}");
    assert!(value.contains(crate::redaction::REDACTED), "{value}");
}

/// a -> b -> c -> d, each answering with the next URL.
fn four_chain() -> Harness {
    let h = harness(flow(
        vec![
            request("a", "https://api.example.com/n1"),
            request("b", "https://api.example.com/placeholder"),
            request("c", "https://api.example.com/placeholder"),
            request("d", "https://api.example.com/placeholder"),
        ],
        vec![
            wire("e1", "a", "b", "url"),
            wire("e2", "b", "c", "url"),
            wire("e3", "c", "d", "url"),
        ],
    ));
    h.http.set_body("/n1", "https://api.example.com/n2");
    h.http.set_body("/n2", "https://api.example.com/n3");
    h.http.set_body("/n3", "https://api.example.com/n4");
    h
}

#[tokio::test]
async fn two_partial_runs_from_one_base_both_work_and_leave_it_intact() {
    let h = four_chain();
    let base = h.service.run(&h.exec, input()).await.expect("base run");
    let run_b = |base_id: String| {
        h.service.run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base_id, "b", FlowPartialMode::Node),
        )
    };
    let first = run_b(base.run_id.clone()).await.expect("first");
    let second = run_b(base.run_id.clone()).await.expect("second");
    assert_ne!(first.run_id, second.run_id);

    // Neither partial run changed the base, so c still has fresh inputs there.
    h.service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "d", FlowPartialMode::Node),
        )
        .await
        .expect("d from the base");

    // In the first partial run, c was not re-run after b changed.
    let err = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&first.run_id, "d", FlowPartialMode::Node),
        )
        .await
        .expect_err("c is out of date there");
    let text = err.to_string();
    assert!(text.contains("is out of date"), "{text}");
    assert!(text.ends_with("node(s): c; edge(s): "), "{text}");
}

fn callback_call() -> crate::callback_listener::ReceivedCall {
    crate::callback_listener::ReceivedCall {
        method: "POST".to_string(),
        path: "/cb/0".to_string(),
        query: Vec::new(),
        headers: Vec::new(),
        body: "{}".to_string(),
    }
}

/// reg (sends {{callback.pay}}) -trigger-> w -value-> out.
fn callback_flow() -> rocket_flow::Flow {
    flow(
        vec![
            request("reg", "https://api.example.com/register?cb={{callback.pay}}"),
            wait("w", "pay"),
            output("out"),
        ],
        vec![trigger("e1", "reg", "w"), wire("e2", "w", "out", "value")],
    )
}

#[tokio::test]
async fn a_cancelled_partial_run_marks_unreached_nodes_stale() {
    let h = harness(callback_flow());
    h.callbacks.queue_on_open(callback_call());
    let base = h.service.run(&h.exec, input()).await.expect("base run");
    assert_eq!(base.stopped_reason, "completed");

    let run = h.service.run_partial(
        &h.exec,
        input(),
        FlowAuthTokens::new(),
        partial(&base.run_id, "reg", FlowPartialMode::FromHere),
    );
    let base_id = base.run_id.clone();
    let stop = async {
        loop {
            let events = h.events.events();
            let partial_id = events.iter().find_map(|e| match e {
                DomainEvent::FlowRunStarted { run_id, .. } if *run_id != base_id => {
                    Some(run_id.clone())
                }
                _ => None,
            });
            if let Some(id) = partial_id {
                let waiting = events.iter().any(|e| {
                    matches!(e, DomainEvent::FlowStepStarted { run_id, node_id } if *run_id == id && node_id == "w")
                });
                if waiting {
                    h.service.cancel(&id);
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    let (summary, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(run, stop)
    })
    .await
    .expect("Stop must end the wait");
    let summary = summary.expect("partial run");

    assert_eq!(summary.stopped_reason, "cancelled");
    assert_eq!(node_ids(&summary), vec!["reg", "w"]);
    assert_eq!(step(&summary, "w").error.as_deref(), Some("cancelled"));

    let err = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&summary.run_id, "out", FlowPartialMode::Node),
        )
        .await
        .expect_err("w never finished in that run");
    let text = err.to_string();
    assert!(text.contains("is out of date"), "{text}");
    assert!(text.ends_with("node(s): w; edge(s): "), "{text}");
}

#[tokio::test]
async fn run_from_a_wait_with_an_upstream_sender_is_refused() {
    let h = harness(callback_flow());
    h.callbacks.queue_on_open(callback_call());
    let base = h.service.run(&h.exec, input()).await.expect("base run");

    let err = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "w", FlowPartialMode::FromHere),
        )
        .await
        .expect_err("reg would not send the new URL");
    let text = err.to_string();
    assert!(text.contains("is not part of this run"), "{text}");
    assert!(text.ends_with("node(s): w, reg; edge(s): "), "{text}");
}

#[tokio::test]
async fn an_unknown_base_another_environment_and_a_cleared_cache_are_refused() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let base = h.service.run(&h.exec, input()).await.expect("base run");

    let unknown = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial("no-such-run", "b", FlowPartialMode::Node),
        )
        .await
        .expect_err("unknown base");
    assert!(unknown.to_string().contains("no longer kept"), "{unknown}");

    let mut staging = input();
    staging.environment_name = Some("staging".to_string());
    let other_env = h
        .service
        .run_partial(
            &h.exec,
            staging,
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect_err("another environment");
    assert!(other_env.to_string().contains("environment"), "{other_env}");

    h.service.clear_run_cache();
    let cleared = h
        .service
        .run_partial(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            partial(&base.run_id, "b", FlowPartialMode::Node),
        )
        .await
        .expect_err("cache cleared");
    assert!(cleared.to_string().contains("no longer kept"), "{cleared}");
}
```

At the very end of `crates/rocket-app/src/flow_execution_service.rs` (after the existing `mod tests` block), add:

```rust
#[cfg(test)]
#[path = "flow_partial_run_tests.rs"]
mod partial_run_tests;
```

- [ ] **Step 4: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app partial_run_tests`
Expected: FAIL to compile (`run_partial`, `clear_run_cache`, `FlowRunSummary::partial` and `FlowRunStarted::partial` do not exist).

- [ ] **Step 5: Add `partial` to the started event and the summary**

In `crates/rocket-shared/src/events.rs`, change `FlowRunStarted` to:

```rust
    FlowRunStarted {
        run_id: String,
        flow_name: String,
        collection: String,
        total_nodes: usize,
        /// Set for a partial run. `total_nodes` then counts only its nodes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        partial: Option<FlowPartialRunInfo>,
    },
```

In the test `flow_run_started_wire_shape`, add `partial: None,` after `total_nodes: 3,` (the expected JSON does not change). Add after that test:

```rust
    #[test]
    fn flow_run_started_carries_partial_run_info() {
        let event = DomainEvent::FlowRunStarted {
            run_id: "01J".into(),
            flow_name: "f".into(),
            collection: "c".into(),
            total_nodes: 1,
            partial: Some(FlowPartialRunInfo {
                base_run_id: "01A".into(),
                start_node_id: "n2".into(),
                mode: FlowPartialMode::Node,
                node_ids: vec!["n2".into()],
            }),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"f","collection":"c","total_nodes":1,"partial":{"baseRunId":"01A","startNodeId":"n2","mode":"node","nodeIds":["n2"]}}"#
        );
        let old = r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"f","collection":"c","total_nodes":1}"#;
        let back: DomainEvent = serde_json::from_str(old).expect("old JSON still reads");
        assert!(matches!(back, DomainEvent::FlowRunStarted { partial: None, .. }));
    }
```

In `crates/rocket-app/src/flow_execution_service.rs`, change the events import (line 669-671) to:

```rust
use rocket_shared::events::{
    DomainEvent, FlowDebugRequest, FlowLogEntry, FlowLogLevel, FlowNodeStatus,
    FlowPartialRunInfo, FlowSkipReason,
};
```

and add to `FlowRunSummary` after `stopped_reason`:

```rust
    /// Set for a partial run ("Run this node", "Run from here").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<FlowPartialRunInfo>,
```

- [ ] **Step 6: Carry the sent credential on `ExecutedNode`**

In `ExecutedNode` (line 34-42) add the field after `reported_value`:

```rust
    /// The credential value a Request sent for its Auth node, when it was
    /// resolved at send time. A later partial run masks it too.
    pub(crate) sent_secret: Option<String>,
```

In `ExecutedNode::plain` add `sent_secret: None,`, and add this method to the same `impl`:

```rust
    pub(crate) fn with_sent_secret(mut self, sent_secret: Option<String>) -> Self {
        self.sent_secret = sent_secret;
        self
    }
```

Add `sent_secret: None,` after `reported_value: None,` in four struct literals: the If arm (`Ok(ExecutedNode { output: source.clone(), chosen_exit: chosen_exit.to_string(), ...`), the Switch arm (`Ok(ExecutedNode { output: source.clone(), chosen_exit, ...`), `flow_poll.rs:159-167` (`return Ok(ExecutedNode { output: captured, ...`), and the test literal in `a_met_poll_is_a_success_even_on_a_non_2xx_response` (`let executed = ExecutedNode {`).

In the Request arm of `execute_node`, right after the `let sent_secret = auth_from.and_then(|node_id| { ... });` statement, add:

```rust
                // Kept on the result, so a partial run built on this run masks it.
                let sent_value = sent_secret.as_ref().map(|(_, value)| value.clone());
```

Change the repeat-until return to end with:

```rust
                        .await
                        .map(|node| node.with_sent_secret(sent_value));
```

and change the arm's final `Ok(ExecutedNode::plain(CapturedOutput::Request(Box::new(output))))` to:

```rust
                Ok(ExecutedNode::plain(CapturedOutput::Request(Box::new(output)))
                    .with_sent_secret(sent_value))
```

- [ ] **Step 7: Add the cache to the service**

Add to the imports near the top of `flow_execution_service.rs`:

```rust
use crate::flow_partial::{PartialPlan, PartialRun};
use crate::flow_run_cache::{CachedRun, FlowRunCache, RunResults, MAX_CACHED_RUNS};
```

Add a field to `FlowExecutionService` after `token_fetch_timeout`:

```rust
    /// Results of recent runs, for partial runs. In memory only.
    run_cache: Arc<Mutex<FlowRunCache>>,
```

initialise it in `new` with `run_cache: Arc::new(Mutex::new(FlowRunCache::new())),`, and add next to `cancel`:

```rust
    /// Forgets every cached run. Called when the workspace changes, because a
    /// collection and flow of the same name in another workspace are different.
    pub fn clear_run_cache(&self) {
        if let Ok(mut cache) = self.run_cache.lock() {
            cache.clear();
        }
    }
```

Add, just above `pub struct FlowExecutionService`:

```rust
/// A partial run that passed every check, ready to execute.
struct PreparedPartial {
    partial: PartialRun,
    base: Arc<CachedRun>,
    plan: PartialPlan,
}
```

- [ ] **Step 8: Split `run_with_auth` and add `run_partial`**

Replace the signature line block

```rust
    pub async fn run_with_auth(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
        auth_tokens: FlowAuthTokens,
    ) -> DomainResult<FlowRunSummary> {
```

with:

```rust
    pub async fn run_with_auth(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
        auth_tokens: FlowAuthTokens,
    ) -> DomainResult<FlowRunSummary> {
        self.run_inner(exec, input, auth_tokens, None).await
    }

    /// Runs part of a flow ("Run this node" or "Run from here") on top of the
    /// cached results of `partial.base_run_id`. Refused with no events when
    /// that run is gone, used other environments, anything upstream changed
    /// since, or a needed input has no cached value (decisions D1 and D5).
    /// Otherwise an ordinary run: a new run id, the same events for the nodes
    /// it runs, the same Stop.
    pub async fn run_partial(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
        auth_tokens: FlowAuthTokens,
        partial: PartialRun,
    ) -> DomainResult<FlowRunSummary> {
        self.run_inner(exec, input, auth_tokens, Some(partial)).await
    }

    /// The run loop shared by full and partial runs.
    async fn run_inner(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
        auth_tokens: FlowAuthTokens,
        partial: Option<PartialRun>,
    ) -> DomainResult<FlowRunSummary> {
```

Inside `run_inner`, make these anchored edits. Keep every line P7 and P9 added.

1. After `let (flow, order) = self.load_ordered_nodes(&input.collection, &input.flow_name)?;` insert:

```rust
        // Saved requests are read once, for fingerprints and the callback check.
        let saved = self.saved_requests(&input.collection, &flow);
        let fingerprints = crate::flow_run_cache::fingerprints(&flow, &order, &|path| {
            saved.get(path).map(crate::flow_run_cache::saved_request_text)
        });
        // A partial run is checked before any secret is fetched or event sent,
        // so a refusal costs nothing and leaves no trace.
        let prepared = match &partial {
            Some(p) => Some(self.prepare_partial(&input, &flow, &order, &saved, &fingerprints, p)?),
            None => None,
        };
```

2. After the loop `for (node_id, secret) in credentials.secrets() { external_secrets.insert(format!("flow-auth.{node_id}"), secret.to_string()); }` insert:

```rust
        // Cached outputs may hold secrets of the earlier run, such as a token
        // rotated since. Mask them too, under keys no template can reference.
        if let Some(prep) = &prepared {
            for (i, value) in prep.base.masking_secrets.iter().enumerate() {
                external_secrets.insert(
                    crate::flow_run_cache::previous_run_secret_key(i),
                    value.clone(),
                );
            }
        }
```

3. Replace the `self.events.publish(DomainEvent::FlowRunStarted { ... });` statement with:

```rust
        let partial_info = prepared.as_ref().map(|p| FlowPartialRunInfo {
            base_run_id: p.partial.base_run_id.clone(),
            start_node_id: p.partial.start_node_id.clone(),
            mode: p.partial.mode,
            node_ids: p.plan.run_order.clone(),
        });
        let run_order: &[String] = prepared
            .as_ref()
            .map_or(order.as_slice(), |p| p.plan.run_order.as_slice());
        self.events.publish(DomainEvent::FlowRunStarted {
            run_id: run_id.clone(),
            flow_name: input.flow_name.clone(),
            collection: input.collection.clone(),
            total_nodes: run_order.len(),
            partial: partial_info.clone(),
        });
```

(If P9 added `callbacks` to this event, keep that field in the literal.)

4. Replace `let mut captured: HashMap<String, CapturedOutput> = HashMap::new();` and `let mut outcomes: HashMap<String, NodeOutcome> = HashMap::new();` with:

```rust
        let mut captured: HashMap<String, CapturedOutput> = HashMap::new();
        let mut outcomes: HashMap<String, NodeOutcome> = HashMap::new();
        // Seeds come from the earlier run. They get no events and are left
        // out of this run's cache entry.
        let mut seeded: HashSet<String> = HashSet::new();
        let mut sent_secrets: HashSet<String> = HashSet::new();
        if let Some(prep) = &prepared {
            for seed in &prep.plan.seeds {
                if let Some(node) = prep.base.nodes.get(seed) {
                    outcomes.insert(seed.clone(), node.outcome.clone());
                    if let Some(output) = &node.output {
                        captured.insert(seed.clone(), output.as_ref().clone());
                    }
                    seeded.insert(seed.clone());
                }
            }
        }
        let dropped_edges: HashSet<String> = prepared
            .as_ref()
            .map(|p| p.plan.dropped_edges.clone())
            .unwrap_or_default();
```

5. Change `for node_id in &order {` to `for node_id in run_order {`, and change the `incoming` filter to:

```rust
                .filter(|e| e.target_node_id == *node_id && !dropped_edges.contains(&e.id))
```

6. Replace

```rust
                    if let Ok(executed) = result {
                        captured.insert(node_id.clone(), executed.output);
                    }
```

with:

```rust
                    if let Ok(executed) = result {
                        if let Some(value) = &executed.sent_secret {
                            sent_secrets.extend(crate::redaction::redaction_forms(value));
                        }
                        captured.insert(node_id.clone(), executed.output);
                    }
```

7. Replace the final `Ok(FlowRunSummary { run_id, steps, stopped_reason })` with:

```rust
        // Every value this run masked, so a partial run built on it masks
        // them too, even after a token rotates.
        let mut masking_secrets = exec.secret_values(
            input.global_env_name.as_deref(),
            Some(&input.collection),
            input.environment_name.as_deref(),
            &external_secrets,
        );
        masking_secrets.extend(credentials.secret_forms());
        masking_secrets.extend(sent_secrets);
        outcomes.retain(|id, _| !seeded.contains(id));
        captured.retain(|id, _| !seeded.contains(id));
        self.remember_run(
            &run_id,
            &input,
            &flow,
            prepared.as_ref(),
            RunResults {
                outcomes,
                captured,
                masking_secrets,
            },
            &fingerprints,
        );

        Ok(FlowRunSummary {
            run_id,
            steps,
            stopped_reason,
            partial: partial_info,
        })
```

- [ ] **Step 9: Add the helpers**

Add these methods to `impl FlowExecutionService`, after `run_inner`:

```rust
    /// Every saved request the flow's Request nodes read, by request path.
    /// A request that cannot be read is left out; the run reports that
    /// failure at its node.
    fn saved_requests(&self, collection: &str, flow: &rocket_flow::Flow) -> HashMap<String, Request> {
        flow.nodes
            .iter()
            .filter_map(|n| match &n.kind {
                FlowNodeKind::Request {
                    source: RequestSource::Saved { request_path },
                    ..
                } => Some(request_path.clone()),
                _ => None,
            })
            .filter_map(|path| {
                self.collection_repo
                    .get_request(collection, &path)
                    .ok()
                    .map(|request| (path, request))
            })
            .collect()
    }

    /// Checks a partial run against its base run and the current flow.
    fn prepare_partial(
        &self,
        input: &RunFlowInput,
        flow: &rocket_flow::Flow,
        order: &[String],
        saved: &HashMap<String, Request>,
        fingerprints: &HashMap<String, u64>,
        partial: &PartialRun,
    ) -> DomainResult<PreparedPartial> {
        let base = self
            .run_cache
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(&partial.base_run_id))
            .ok_or_else(|| crate::flow_partial::PartialRefusal {
                message: format!(
                    "the earlier run this builds on is no longer kept (the last {MAX_CACHED_RUNS} runs stay in memory until Rocket restarts or the workspace changes). Run the full flow first"
                ),
                node_ids: vec![partial.start_node_id.clone()],
                edge_ids: Vec::new(),
            })?;
        base.check_scope(input, &partial.start_node_id)?;
        let senders = crate::flow_partial::callback_senders(flow, saved);
        let plan = crate::flow_partial::select_nodes(flow, order, partial, &senders)?;
        base.check_unchanged(flow, &plan.seeds, fingerprints)?;
        crate::flow_partial::check_seeds(flow, &plan, &partial.start_node_id, &base.seed_views())?;
        Ok(PreparedPartial {
            partial: partial.clone(),
            base,
            plan,
        })
    }

    /// Keeps this run's results for later partial runs. A partial run adds a
    /// new entry built on its base, so the base itself never changes.
    fn remember_run(
        &self,
        run_id: &str,
        input: &RunFlowInput,
        flow: &rocket_flow::Flow,
        prepared: Option<&PreparedPartial>,
        results: RunResults,
        fingerprints: &HashMap<String, u64>,
    ) {
        let entry = match prepared {
            Some(p) => p.base.merge_partial(
                input,
                flow,
                &p.partial.start_node_id,
                results,
                fingerprints,
            ),
            None => CachedRun::from_full_run(input, flow, results, fingerprints),
        };
        if let Ok(mut cache) = self.run_cache.lock() {
            cache.insert(run_id.to_string(), entry);
        }
    }
```

Remove the two `#[cfg_attr(not(test), allow(dead_code))]` lines and their comments from `crates/rocket-app/src/lib.rs`.

- [ ] **Step 10: Run the new tests to verify they pass**

Run: `cargo test -j4 -p rocket-app partial_run_tests`
Expected: PASS (8 tests).

Run: `cargo test -j4 -p rocket-shared flow_run_started`
Expected: PASS.

- [ ] **Step 11: Run every Flow test in the crate**

Run: `cargo test -j4 -p rocket-app flow_`
Expected: PASS. Full runs now also fill the cache and call `secret_values` once more at the end; no existing test counts those calls. If a test fails on a struct literal, it is a missing `sent_secret: None` or `partial: None`.

- [ ] **Step 12: Document the feature**

In `crates/rocket-app/CLAUDE.md`, after the "Flow Auth nodes" section, add:

```markdown
## Flow partial runs (`flow_partial.rs`, `flow_run_cache.rs`)

`FlowExecutionService::run_partial` re-runs one node (`FlowPartialMode::Node`)
or a node and its descendants (`FromHere`) on top of a cached earlier run.
`flow_run_cache` keeps the last 8 runs in memory (64 MiB of outputs, 16 MiB
per output, never persisted, never sent over IPC, cleared on workspace
switch). Input and Auth nodes always run again; their outputs are never
cached. A run is refused before any event when the base run is gone, used
other environments, a node upstream changed (Merkle fingerprints, saved
request text without `uid`), a needed input was skipped, failed or not kept,
a seed is stale after an earlier partial run, or a Wait's callback sender is
outside the run. Variable value changes are not detected (decision D5).
Values the base run masked are masked again under `}}prev-run.<n>`
external-secret keys, which no `{{template}}` can reference.
```

- [ ] **Step 13: Gates and commit**

Run: `cargo check -j4 -p rocket-app && cargo check -j4 -p rocket && cargo clippy -j4 -p rocket-app --tests -- -D warnings`
Expected: no errors and no warnings (`rocket` is the `src-tauri` package; it only returns `FlowRunSummary`, so it compiles unchanged).

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-app/src/flow_execution_service.rs crates/rocket-app/src/flow_poll.rs crates/rocket-app/src/flow_partial_run_tests.rs crates/rocket-app/src/lib.rs crates/rocket-shared/src/events.rs crates/rocket-app/CLAUDE.md`
Suggested subject: `feat(flow): run a single node or a subgraph from cached results`.

---

## Self-Review

- **Spec coverage:** F-41 backend. Run set S for both modes, seeds, free Input and Auth nodes, dropped start triggers, branch and failure refusals, Wait sender rule (Task 1). Fingerprints, environment check, LRU and byte budget, masking secrets, stale merge, Auth outputs excluded (Task 2). Engine integration, event and summary fields, cancel, cache on every run, workspace clear method (Task 3). The IPC DTO, the workspace-switch call and the UI are P20.
- **Rejected alternatives (kept out on purpose):** the client sending cached values (masked, capped, secrets over IPC); persisting the cache (raw secrets on disk); re-running ancestors as the only mode (repeats side effects; "run to here" can reuse `select_nodes` later); caching Auth tokens; no fingerprint check.
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `PartialRun { base_run_id, start_node_id, mode: FlowPartialMode }` is used in Tasks 1 and 3 and by P20. `SeedView`, `PartialPlan`, `PartialRefusal` come from Task 1 and are used unchanged in Tasks 2 and 3. `RunResults` and `CachedRun::{from_full_run, merge_partial}` keep the same arguments in Tasks 2 and 3. `FlowPartialRunInfo` is the same type on the event and the summary.
- **Review Focus coverage:** item 1 in Task 2 and Task 3; item 2 in Task 2 and Task 3; item 3 in Task 1; item 4 in Task 1 and Task 3; item 5 in Task 2 and Task 3.

Known limitations to state in the PR: variable value changes since the base run are not detected (D5); folder and collection settings a saved request inherits are not fingerprinted; a script can still read a `}}prev-run.<n>` value by exact name from the variable context; cached outputs stay in memory for up to 8 runs.
