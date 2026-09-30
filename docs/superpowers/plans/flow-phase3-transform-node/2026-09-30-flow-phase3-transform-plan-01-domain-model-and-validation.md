# Flow Phase 3 — Plan 01: Domain Model and Validation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Run this plan on its own. Do not start plan 02 in the same run.**

**Goal:** Add the `Transform` node kind to the Flow domain with save-time validation, keep every existing flow file loading and re-saving unchanged, and keep the whole workspace compiling.

**Architecture:** `rocket-flow` gains `FlowNodeKind::Transform { label, script }` and a default-script constant. The Phase 2 input rules (exactly one incoming edge, into `input`) are widened from "routing nodes" to "nodes that take an input", which now includes Transform. The IPC DTO in `src-tauri` mirrors the new variant, and the executor gets a temporary fail-fast arm that plan 02 replaces.

**Tech Stack:** Rust (serde, serde_yaml, thiserror), Tauri 2 IPC DTOs, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` (§5 Data model, §6 Validation, §8 Events and IPC). Plan index and cross-plan contract: `docs/superpowers/plans/flow-phase3-transform-node/00-plan-index.md`.

## Global Constraints

- `rocket-flow` stays a pure domain crate: no I/O and no script evaluation.
- Persistence structs (`FlowNodeKind`) have **no** `rename_all`, so fields are snake_case on disk. IPC DTOs use `rename_all_fields = "camelCase"` on the tagged enum, which is already set on `FlowNodeKindDto`.
- Existing flow files, including ones with If, Switch and Wait for callback nodes, must load, validate and re-save byte-identically. Do not touch any existing variant's serde attributes.
- No panicking calls in production code: map failures to `DomainError` instead. Tests use `.expect("…")`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Commits: conventional-commit subjects, created through the `dev-workflow-skills:1-git-commit` skill. Stage only the task's own paths.
- Code comments: short full sentences ending with a punctuation mark.

## Review Focus

1. **A Transform node with no incoming wire.** A user who drops a Transform on the canvas and saves before wiring it expects a clear error naming the node, not a save that later fails on every run. Pinned in Task 2 (`vt_transform_without_input_is_rejected`).
2. **A script that is only whitespace or newlines.** This is always a mistake and must be rejected on save. Pinned in Task 2 (`vt_blank_script_is_rejected`).
3. **A hand-edited file that wires a `trigger` or a `url` field into a Transform.** The error must name the Transform node, not silently accept the wire. Pinned in Task 2 (`vt_transform_input_must_target_the_input_field`, `vt_transform_trigger_is_rejected`).
4. **A hand-edited file where a Transform's edge leaves through `true`, `false` or a `case:` exit.** It must be rejected with the edge id. Pinned in Task 2 (`vt_transform_has_only_the_result_exit`).
5. **A multi-line script with quotes, tabs and a blank line.** It must survive a YAML save and load unchanged. Pinned in Task 3 (`transform_node_roundtrips_a_multiline_script`).

---

### Task 1: The `Transform` node kind

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/node.rs` (enum near line 16-56, tests module at the end)
- Modify: `crates/rocket-flow/src/lib.rs`
- Modify: `crates/rocket-flow/src/validate.rs` (`kind_name` and `source_handle_exists`, so the crate compiles)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `FlowNodeKind::Transform { label: String, script: String }` and `pub const TRANSFORM_DEFAULT_SCRIPT: &str`, re-exported from the crate root. Tasks 2 and 3 and plans 02-05 rely on these exact names.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module of `crates/rocket-flow/src/node.rs`, after `flow_node_kind_switch_tagged_roundtrip_keeps_case_order`:

```rust
    #[test]
    fn flow_node_kind_transform_tagged_roundtrip() {
        let kind = FlowNodeKind::Transform {
            label: "Pick token".to_string(),
            script: "return response.body.token;".to_string(),
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Transform\""), "got: {json}");
        assert!(json.contains("\"script\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn transform_default_script_returns_the_body() {
        assert_eq!(TRANSFORM_DEFAULT_SCRIPT, "return response.body;");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow transform`
Expected: FAIL to compile with "no variant named `Transform`" and "cannot find value `TRANSFORM_DEFAULT_SCRIPT`".

- [ ] **Step 3: Add the variant and the constant**

In `crates/rocket-flow/src/node.rs`, extend the enum doc comment by one sentence: `` `Transform` nodes reshape their one input with a script. `` Then add the variant after `WaitForCallback`:

```rust
    /// Reshapes its single input with a script. The script reads `response`
    /// (the upstream value, response-shaped) and returns the node's output.
    Transform {
        label: String,
        script: String,
    },
```

Add the constant next to `CALLBACK_VAR_PREFIX`:

```rust
/// The script a new Transform node starts with.
pub const TRANSFORM_DEFAULT_SCRIPT: &str = "return response.body;";
```

In `crates/rocket-flow/src/lib.rs`, add `TRANSFORM_DEFAULT_SCRIPT` to the `pub use node::{ … }` list, after `CALLBACK_VAR_PREFIX`.

- [ ] **Step 4: Keep `validate.rs` compiling**

In `kind_name`, add the arm after `WaitForCallback`:

```rust
        FlowNodeKind::Transform { .. } => "Transform",
```

In `source_handle_exists`, add `Transform` to the arm that only has the `result` exit:

```rust
        FlowNodeKind::Request { .. }
        | FlowNodeKind::Input { .. }
        | FlowNodeKind::WaitForCallback { .. }
        | FlowNodeKind::Transform { .. } => source_handle == handle::RESULT,
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS, all `rocket-flow` tests green (the two new tests included).

- [ ] **Step 6: Commit**

Stage `crates/rocket-flow/src/node.rs`, `crates/rocket-flow/src/lib.rs` and `crates/rocket-flow/src/validate.rs`, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): add Transform node kind`.

---

### Task 2: Validation rules for Transform

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-flow/src/validate.rs` (`validate`, `check_routing_inputs`, `check_expressions`, tests)
- Modify: `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` (§6 item 4)

**Interfaces:**
- Consumes: `FlowNodeKind::Transform` from Task 1.
- Produces: `validate` rejects a Transform with a wrong or missing `input` wire, a blank script, or a non-`result` exit. Plan 03's client-side rules copy these exactly.

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `crates/rocket-flow/src/validate.rs`, add a helper next to `switch_node`:

```rust
    fn transform(id: &str, script: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Transform {
                label: id.to_string(),
                script: script.to_string(),
            },
        )
    }
```

Add these tests after `v1_routing_input_must_target_the_input_field`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-flow vt_`
Expected: FAIL. `vt_transform_can_sit_after_a_branch_exit` and `valid_transform_flow_passes` fail with `only If and Switch nodes have an 'input' input`, and `vt_transform_without_input_is_rejected` fails because validation returns `Ok`.

- [ ] **Step 3: Widen the input rules**

In `validate.rs`, add next to `is_routing`:

```rust
/// True for the node kinds that evaluate one upstream value through an
/// `input` handle: If, Switch and Transform.
fn takes_input(kind: &FlowNodeKind) -> bool {
    is_routing(kind) || matches!(kind, FlowNodeKind::Transform { .. })
}
```

In `check_routing_inputs`, change the filter and the doc comment:

```rust
/// V1: an If, Switch or Transform node has exactly one incoming edge, into `input`.
fn check_routing_inputs(flow: &Flow) -> Result<(), FlowGraphError> {
    for node in flow.nodes.iter().filter(|n| takes_input(&n.kind)) {
```

In `validate`, change the second `check_edges` call (the `input`-only rule):

```rust
    check_edges(flow, &kinds, |edge, target, _| {
        (edge.target_field == handle::INPUT && !takes_input(target)).then(|| {
            format!(
                "only If, Switch and Transform nodes have an '{}' input",
                handle::INPUT
            )
        })
    })?;
```

Replace `check_expressions` with:

```rust
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
```

The trigger rule stays as it is: Transform is not in its accept list, and rule V1 already rejects a `trigger` wire first.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-flow`
Expected: PASS, including every existing `v1_`, `v2_`, `v3_` and `v8_` test.

- [ ] **Step 5: Update the spec**

In `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` §6, replace item 4 with:

```
4. A `trigger` edge into a Transform node is rejected, because rule 1 allows one incoming edge and it must target `input`. This matches If and Switch.
```

- [ ] **Step 6: Commit**

Stage `crates/rocket-flow/src/validate.rs` and the spec file, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): validate Transform node structure`.

---

### Task 3: Keep the workspace compiling (DTO, executor stub, persistence)

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `src-tauri/src/commands/flow.rs` (`FlowNodeKindDto`, both `From` impls, tests)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`execute_node`, a temporary arm)
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs` (tests)

**Interfaces:**
- Consumes: `FlowNodeKind::Transform` from Task 1.
- Produces: `FlowNodeKindDto::Transform { label: String, script: String }`, which plan 03's TypeScript type mirrors. A temporary executor arm that plan 02 Task 2 replaces.

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/commands/flow.rs`, add to the tests module after `routing_node_dtos_roundtrip_through_domain_type`:

```rust
    #[test]
    fn transform_node_dto_keeps_tag_and_roundtrips() {
        let dto = FlowDto {
            name: "Transform".to_string(),
            nodes: vec![FlowNodeDto {
                id: "t1".to_string(),
                kind: FlowNodeKindDto::Transform {
                    label: "Pick token".to_string(),
                    script: "return response.body.token;".to_string(),
                },
                position: NodePositionDto { x: 0.0, y: 0.0 },
            }],
            edges: vec![],
            callback_host: None,
        };
        let json = serde_json::to_string(&dto).expect("serialize FlowDto");
        assert!(json.contains(r#""kind":"Transform""#), "got: {json}");
        assert!(
            json.contains(r#""script":"return response.body.token;""#),
            "got: {json}"
        );
        let domain: Flow = dto.clone().into();
        assert!(matches!(
            domain.nodes[0].kind,
            FlowNodeKind::Transform { .. }
        ));
        let back: FlowDto = domain.into();
        let a = serde_json::to_value(&dto).expect("to_value dto");
        let b = serde_json::to_value(&back).expect("to_value back");
        assert_eq!(a, b);
    }
```

In `crates/rocket-infra/src/fs_flow_repo.rs`, add after `if_and_switch_nodes_with_routed_edges_roundtrip`:

```rust
    #[test]
    fn transform_node_roundtrips_a_multiline_script() {
        let (_dir, repo) = setup();
        let mut flow = sample("Transform Flow");
        flow.nodes.push(FlowNode {
            id: "t1".to_string(),
            kind: FlowNodeKind::Transform {
                label: "Pick token".to_string(),
                script: "const t = response.body.token;\n\n\treturn \"a\" + 'b' + t;".to_string(),
            },
            position: NodePosition { x: 200.0, y: 0.0 },
        });
        flow.edges.push(FlowEdge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "t1".to_string(),
            target_field: rocket_flow::handle::INPUT.to_string(),
            expression: String::new(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        });
        repo.save("acme", &flow).expect("save");
        assert_eq!(repo.get("acme", "Transform Flow").expect("get"), flow);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo check -j4 --workspace`
Expected: FAIL with `non-exhaustive patterns: FlowNodeKind::Transform { .. } not covered` in `src-tauri/src/commands/flow.rs` and `crates/rocket-app/src/flow_execution_service.rs`, and `no variant named Transform` for `FlowNodeKindDto`.

- [ ] **Step 3: Add the DTO variant**

In `src-tauri/src/commands/flow.rs`, add to `FlowNodeKindDto` after `WaitForCallback`:

```rust
    Transform {
        label: String,
        script: String,
    },
```

Add the arm to `impl From<FlowNodeKind> for FlowNodeKindDto`:

```rust
            FlowNodeKind::Transform { label, script } => {
                FlowNodeKindDto::Transform { label, script }
            }
```

And to `impl From<FlowNodeKindDto> for FlowNodeKind`:

```rust
            FlowNodeKindDto::Transform { label, script } => {
                FlowNodeKind::Transform { label, script }
            }
```

- [ ] **Step 4: Add the temporary executor arm**

In `execute_node` in `crates/rocket-app/src/flow_execution_service.rs`, add after the `WaitForCallback { .. }` arm (find it with `grep -n "FlowNodeKind::WaitForCallback {" crates/rocket-app/src/flow_execution_service.rs`; the arm ends before the closing brace of the `match`):

```rust
            // Replaced by the real execution in plan 02.
            FlowNodeKind::Transform { .. } => Err(DomainError::Internal(format!(
                "Transform node '{}' is not runnable yet",
                node.id
            ))),
```

- [ ] **Step 5: Run the checks and tests**

Run: `cargo check -j4 --workspace`
Expected: PASS with no errors. If another crate reports a non-exhaustive match on `FlowNodeKind`, add the smallest arm that keeps the current behavior and mention it in the commit body.

Run: `cargo test -j4 -p rocket-infra transform_node` then `cargo test -j4 -p rocket-flow` then `cargo test -j4 -p rocket-app flow`
Expected: PASS for all three.

Run: `cargo test -j4 -p rocket transform_node_dto` (the `src-tauri` package name is in `src-tauri/Cargo.toml`; use that name if it differs from `rocket`).
Expected: PASS.

- [ ] **Step 6: Run the linter and formatter**

Run: `cargo fmt --all -- --check` and `cargo clippy -j4 -p rocket-flow -p rocket-app -p rocket-infra -- -D warnings`
Expected: no output and no warnings. Fix anything reported in the files this plan touched.

- [ ] **Step 7: Commit**

Stage `src-tauri/src/commands/flow.rs`, `crates/rocket-app/src/flow_execution_service.rs` and `crates/rocket-infra/src/fs_flow_repo.rs`, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): mirror Transform node in IPC and repo`.

---

## Next Plan

**Next plan to execute:** `docs/superpowers/plans/flow-phase3-transform-node/2026-09-30-flow-phase3-transform-plan-02-executor-and-step-reporting.md`

Do not start it in this run. Finish the review below, report to the user, and wait for them to start plan 02.

## Post-Implementation Review

Before plan 02 starts, dispatch one Opus-model subagent (read and fix allowed) with this brief: "Review every change made by plan 01 of `docs/superpowers/plans/flow-phase3-transform-node/`, using `git log` for the three commits. Check: (a) old flow files still load and re-save byte-identically (run `cargo test -j4 -p rocket-infra`); (b) each validation rule in the spec §6 has a test; (c) no panicking calls were added outside tests; (d) the temporary executor arm is clearly marked. Fix any defect you find and report what changed. Do not start plan 02."
