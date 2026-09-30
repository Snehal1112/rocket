# Flow Phase 3 — Plan 02: Executor and Step Reporting — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Run this plan on its own. Do not start plan 03 in the same run.**

**Goal:** Make a Transform node run during a Flow: evaluate its script through the existing sandbox, capture the returned text as the node's output, report a masked value on the step, and skip it correctly when its branch is not taken or its upstream failed.

**Architecture:** A new `FlowCoercion::Required` makes the generated JS throw when the script returns `undefined`. `evaluate_flow_transform_script` is a thin, named wrapper over `evaluate_flow_route_expression`, which already turns any result into text. `execute_node` gets a real `Transform` arm that replaces plan 01's stub and stores `CapturedOutput::Value`. `decide_fate` and the events are unchanged.

**Tech Stack:** Rust, tokio tests, the in-file `ScriptedJsonqEngine` fake.

**Spec:** `docs/superpowers/specs/2026-09-30-flow-phase3-transform-node-design.md` (§7 Execution, §8 Events and IPC, §10 Errors). Plan index and cross-plan contract: `docs/superpowers/plans/flow-phase3-transform-node/00-plan-index.md`.

## Global Constraints

- Plan 01 is merged: `FlowNodeKind::Transform { label, script }` exists and `execute_node` has a stub arm containing the text `is not runnable yet`. If it does not, stop and report.
- The Transform script runs through the existing script engine only. No new sandbox or execution path.
- `decide_fate` and `is_live` in `flow_routing.rs` do not change. A Transform after a failed Request is skipped as "upstream failed".
- Wires downstream get the **raw** text. Only the step's reported `value` is masked with `redact_secrets`.
- No panicking calls in production code. Map failures to `DomainError`. Tests use `.expect("…")`.
- Always pass `-j4` to cargo. Never run `cargo test --workspace`.
- Commits: conventional-commit subjects, created through the `dev-workflow-skills:1-git-commit` skill. Stage only the task's own paths.
- Code comments: short full sentences ending with a punctuation mark.

## Review Focus

1. **A script with no `return`.** A user who writes `const x = 1;` expects "script returned no value", not the text `null` flowing downstream. Pinned in Task 1 (`required_coercion_guards_against_undefined`) and Task 2 (`transform_that_returns_nothing_fails_with_a_clear_error`).
2. **A script that returns `null`, `0`, `false` or an empty string.** These are values, not "no value", so they must pass. Pinned in Task 1 (`transform_script_turns_null_into_the_text_null`, `transform_script_keeps_zero_and_false`).
3. **A script that returns an object or array.** Downstream nodes get compact JSON text they can `JSON.parse`. Pinned in Task 1 (`transform_script_returns_an_object_as_compact_json`).
4. **A script that echoes a secret variable.** The step and its event must show the masked value while downstream wires get the real one. Pinned in Task 3 (`a_transform_reports_a_secret_masked_but_passes_it_on_raw`).
5. **A Transform after a branch that was not taken, or after a failed node.** It must be skipped with the right reason and must never run. Pinned in Task 3 (`transform_after_a_not_taken_branch_is_skipped`) and Task 2 (`a_throwing_transform_fails_and_skips_its_dependents`).

---

### Task 1: The `Required` coercion and the transform evaluator

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`FlowCoercion` near line 74, `flow_script` near line 91, `evaluate_flow_route_expression` near line 227, tests at the end of `mod tests`)

**Interfaces:**
- Consumes: `evaluate_flow_route_expression(&self, collection: &str, output: &CapturedOutput, source: &str, coercion: FlowCoercion, secret_values: &HashSet<String>) -> FlowScriptOutcome` and `FlowScriptOutcome { result: DomainResult<String>, logs }`, both already in the file.
- Produces: `FlowCoercion::Required` and `RequestExecutionService::evaluate_flow_transform_script(&self, collection: &str, output: &CapturedOutput, source: &str, secret_values: &HashSet<String>) -> FlowScriptOutcome`. Task 2 calls the second.

- [ ] **Step 1: Write the failing tests**

Append to the end of `mod tests` in `crates/rocket-app/src/flow_execution_service.rs`:

```rust
    // ---- Phase 3: Transform ------------------------------------------------

    #[test]
    fn required_coercion_guards_against_undefined() {
        let script =
            flow_script("return response.body;", FlowCoercion::Required).expect("wrapper");
        assert!(
            script.contains("return __requireValue(fn(response));"),
            "got: {script}"
        );
        assert!(script.contains("script returned no value"), "got: {script}");
    }

    #[test]
    fn other_coercions_do_not_call_the_guard() {
        for coercion in [FlowCoercion::Raw, FlowCoercion::Bool, FlowCoercion::Str] {
            let script = flow_script("1", coercion).expect("wrapper");
            assert!(!script.contains("__requireValue(fn"), "got: {script}");
        }
    }

    async fn transform_result(answer: Scripted) -> FlowScriptOutcome {
        let svc = service_with_engine(
            FakeCollectionRepo::new(),
            scripted(vec![("__requireValue(", answer)]),
        );
        let output = CapturedOutput::Request(Box::new(sample_response_output()));
        svc.evaluate_flow_transform_script("my-api", &output, "return 1;", &HashSet::new())
            .await
    }

    #[tokio::test]
    async fn transform_script_returns_a_string_as_is() {
        let outcome = transform_result(Scripted::Value(serde_json::json!("abc"))).await;
        assert_eq!(outcome.result.expect("script result"), "abc");
    }

    #[tokio::test]
    async fn transform_script_returns_an_object_as_compact_json() {
        let outcome = transform_result(Scripted::Value(serde_json::json!({"a": 1, "b": [2]}))).await;
        assert_eq!(outcome.result.expect("script result"), r#"{"a":1,"b":[2]}"#);
    }

    #[tokio::test]
    async fn transform_script_turns_null_into_the_text_null() {
        let outcome = transform_result(Scripted::Value(serde_json::Value::Null)).await;
        assert_eq!(outcome.result.expect("script result"), "null");
    }

    #[tokio::test]
    async fn transform_script_keeps_zero_and_false() {
        let zero = transform_result(Scripted::Value(serde_json::json!(0))).await;
        assert_eq!(zero.result.expect("script result"), "0");
        let no = transform_result(Scripted::Value(serde_json::json!(false))).await;
        assert_eq!(no.result.expect("script result"), "false");
    }

    #[tokio::test]
    async fn transform_script_reports_a_script_error() {
        let outcome = transform_result(Scripted::Throw("script returned no value")).await;
        assert!(matches!(
            outcome.result,
            Err(DomainError::InvalidInput(ref m)) if m.contains("script returned no value")
        ));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app transform_script`
Expected: FAIL to compile with "no variant named `Required`" and "no method named `evaluate_flow_transform_script`".

- [ ] **Step 3: Add the coercion and the guard**

In `FlowCoercion`, add after `Str`:

```rust
    /// A Transform node needs any value except `undefined`.
    Required,
```

In `flow_script`, extend the coercion `match`:

```rust
        FlowCoercion::Str => ("String(", ")"),
        FlowCoercion::Required => ("__requireValue(", ")"),
```

In the format string, add the guard before `const response = {{`:

```
  const __requireValue = (v) => {{
    if (v === undefined) throw new Error('script returned no value');
    return v;
  }};
```

The `return {open}fn(response){close};` line then reads `return __requireValue(fn(response));` for this coercion. Keep the guard text free of the substrings `String(` and `!!(`, because the test fakes match on them.

- [ ] **Step 4: Add the evaluator**

Below `evaluate_flow_route_expression`, add:

```rust
    /// Evaluates a Transform node's script against `output`. The script may be
    /// one expression or a function body that returns a value. Any result
    /// except `undefined` is accepted, and it comes back as text: a string as
    /// is, `null` as `"null"`, and anything else as compact JSON.
    pub async fn evaluate_flow_transform_script(
        &self,
        collection: &str,
        output: &CapturedOutput,
        source: &str,
        secret_values: &HashSet<String>,
    ) -> FlowScriptOutcome {
        self.evaluate_flow_route_expression(
            collection,
            output,
            source,
            FlowCoercion::Required,
            secret_values,
        )
        .await
    }
```

Change the doc comment of `evaluate_flow_route_expression` to start with `Evaluates an If/Switch routing script, or a Transform script, against output.` so it stays accurate.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow`
Expected: PASS, including every existing route, wire and callback test (the guard text is now in every generated script, and none of them may break).

- [ ] **Step 6: Commit**

Stage `crates/rocket-app/src/flow_execution_service.rs`, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): evaluate Transform scripts`.

---

### Task 2: Run Transform nodes in the executor

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`execute_node`, `single_route_input`, tests)

**Interfaces:**
- Consumes: `evaluate_flow_transform_script` from Task 1, `ExecutedNode::plain`, `redact_secrets`, `VariableValue::simple`.
- Produces: a working `Transform` arm, and `fn single_input(node: &FlowNode, data_edges: &[&FlowEdge], captured: &HashMap<String, CapturedOutput>) -> DomainResult<&CapturedOutput>`, the renamed `single_route_input`.

- [ ] **Step 1: Write the failing tests**

Append to `mod tests`, under the Phase 3 heading from Task 1:

```rust
    fn transform_node(id: &str, script: &str) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Transform {
                label: id.to_string(),
                script: script.to_string(),
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// in("pro") -> t -> out (value).
    fn transform_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                input_node_with("in", "pro"),
                transform_node("t", "return response.body.toUpperCase();"),
                output_node_named("out"),
            ],
            edges: vec![
                input_edge("e1", "in", "t"),
                edge_from("e2", "t", handle::RESULT, "out", "value", "response.body"),
            ],
            callback_host: None,
        }
    }

    /// Answers the Transform wrapper with `answer` and any wire from the body.
    fn transform_engine(answer: Scripted) -> Box<dyn ScriptEngine> {
        scripted(vec![
            ("__requireValue(", answer),
            (
                "response.body",
                Scripted::FromResponse(|r| {
                    serde_json::json!(r.map(|r| r.body.clone()).unwrap_or_default())
                }),
            ),
        ])
    }

    async fn run_transform(name: &str, answer: Scripted) -> FlowRunSummary {
        let service = service_with_flow(transform_flow(name));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, transform_engine(answer));
        service.run(&exec, run_input(name)).await.expect("run")
    }

    #[tokio::test]
    async fn transform_reshapes_its_input_for_a_downstream_output() {
        let summary = run_transform("tf-ok", Scripted::Value(serde_json::json!("PRO"))).await;

        assert_eq!(status_of(&summary, "t"), FlowNodeStatus::Success);
        assert_eq!(step_of(&summary, "t").value.as_deref(), Some("PRO"));
        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("PRO"));
    }

    #[tokio::test]
    async fn transform_reports_an_object_result_as_compact_json() {
        let summary =
            run_transform("tf-obj", Scripted::Value(serde_json::json!({"plan": "pro"}))).await;

        assert_eq!(
            step_of(&summary, "t").value.as_deref(),
            Some(r#"{"plan":"pro"}"#)
        );
    }

    #[tokio::test]
    async fn a_throwing_transform_fails_and_skips_its_dependents() {
        let summary = run_transform("tf-throw", Scripted::Throw("ReferenceError: nope")).await;

        assert_eq!(status_of(&summary, "t"), FlowNodeStatus::Failed);
        assert!(step_of(&summary, "t")
            .error
            .as_deref()
            .is_some_and(|e| e.contains("ReferenceError")));
        assert_eq!(status_of(&summary, "out"), FlowNodeStatus::Skipped);
        assert_eq!(
            step_of(&summary, "out").skip_reason,
            Some(FlowSkipReason::UpstreamFailed)
        );
    }

    #[tokio::test]
    async fn transform_that_returns_nothing_fails_with_a_clear_error() {
        let summary =
            run_transform("tf-none", Scripted::Throw("script returned no value")).await;

        assert_eq!(status_of(&summary, "t"), FlowNodeStatus::Failed);
        assert!(step_of(&summary, "t")
            .error
            .as_deref()
            .is_some_and(|e| e.contains("script returned no value")));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app transform_reshapes`
Expected: FAIL. The step for `t` is `Failed` with the error `Transform node 't' is not runnable yet`.

- [ ] **Step 3: Rename the input helper**

In `flow_execution_service.rs`, rename `single_route_input` to `single_input` at its definition and at both call sites (the `If` and `Switch` arms). Update its doc comment's first sentence to `The captured output feeding a single-input node (If, Switch or Transform) through its one live input edge.` and its message to:

```rust
            "node '{}' needs exactly one live '{}' input, found {}",
```

- [ ] **Step 4: Replace the stub arm**

Replace the temporary arm (the one containing `is not runnable yet`) with:

```rust
            FlowNodeKind::Transform { script, .. } => {
                let source = single_input(node, data_edges, captured)?;
                let outcome = exec
                    .evaluate_flow_transform_script(
                        &input.collection,
                        source,
                        script,
                        &secret_values,
                    )
                    .await;
                logs.extend(outcome.logs);
                let text = outcome.result?;
                // Wires get the raw text. The step shows it with secrets masked.
                let reported = crate::redaction::redact_secrets(&text, &secret_values);
                Ok(ExecutedNode {
                    reported_value: Some(reported),
                    ..ExecutedNode::plain(CapturedOutput::Value(VariableValue::simple(text)))
                })
            }
```

- [ ] **Step 5: Report the value on the step**

In `result_to_step`, in the `CapturedOutput::Value(v)` branch, change the `Input` arm so Transform shares it:

```rust
                Some(FlowNodeKind::Input { .. }) | Some(FlowNodeKind::Transform { .. }) => {
                    reported_value
                        .clone()
                        .or_else(|| Some(v.data().to_string()))
                }
```

Update the comment above it to `An Input or Transform node reports its masked value, an Output node its capture.`

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow`
Expected: PASS, including the four new tests and every existing If/Switch/Output test.

- [ ] **Step 7: Commit**

Stage `crates/rocket-app/src/flow_execution_service.rs`, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `feat(flow): run Transform nodes`.

---

### Task 3: Masking, skipping and fan-out

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (tests only, unless a test exposes a defect)

**Interfaces:**
- Consumes: the Transform arm and `single_input` from Task 2, and the test helpers `scoped_exec`, `env_with`, `service_with_publisher`, `RecordingPublisher`, `if_node`, `edge_from`, `input_node_with`, `output_node_named`.
- Produces: regression tests that pin the masking, skip and fan-out rules. No new production names.

- [ ] **Step 1: Write the tests**

Append to `mod tests`:

```rust
    #[tokio::test]
    async fn a_transform_reports_a_secret_masked_but_passes_it_on_raw() {
        let mut env = env_with(&[]);
        let mut key = rocket_environment::Variable::new("apiKey", "sk-live-123456");
        key.secret = true;
        env.set_variable(key);
        let exec = scoped_exec(env, Vec::new());
        let flow = Flow {
            name: "tf-secret".to_string(),
            nodes: vec![
                input_node_with("in", "{{apiKey}}"),
                transform_node("t", "response.body"),
                output_node_named("out"),
            ],
            edges: vec![
                input_edge("e1", "in", "t"),
                edge_from("e2", "t", handle::RESULT, "out", "value", "response.body"),
            ],
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let mut input = run_input("tf-secret");
        input.environment_name = Some("dev".to_string());

        let summary = service_with_publisher(flow, &publisher)
            .run(&exec, input)
            .await
            .expect("run");

        assert_eq!(
            step_of(&summary, "t").value.as_deref(),
            Some(crate::redaction::REDACTED)
        );
        let event_value = publisher.events().iter().find_map(|e| match e {
            DomainEvent::FlowStepCompleted { node_id, value, .. } if node_id == "t" => {
                Some(value.clone())
            }
            _ => None,
        });
        assert_eq!(
            event_value.flatten().as_deref(),
            Some(crate::redaction::REDACTED)
        );
        assert_eq!(
            step_of(&summary, "out").value.as_deref(),
            Some("sk-live-123456"),
            "the downstream wire still gets the real value"
        );
    }

    /// in -> check(if) -> true: t -> out.
    fn transform_after_if_flow(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![
                input_node_with("in", "pro"),
                if_node("check", "response.body === 'pro'"),
                transform_node("t", "return response.body;"),
                output_node_named("out"),
            ],
            edges: vec![
                input_edge("e1", "in", "check"),
                edge_from("e2", "check", handle::TRUE, "t", handle::INPUT, ""),
                edge_from("e3", "t", handle::RESULT, "out", "value", "response.body"),
            ],
            callback_host: None,
        }
    }

    #[tokio::test]
    async fn transform_after_a_not_taken_branch_is_skipped() {
        let service = service_with_flow(transform_after_if_flow("tf-branch"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Value(serde_json::json!(false)))]),
        );

        let summary = service
            .run(&exec, run_input("tf-branch"))
            .await
            .expect("run");

        for id in ["t", "out"] {
            assert_eq!(status_of(&summary, id), FlowNodeStatus::Skipped, "node {id}");
            assert_eq!(
                step_of(&summary, id).skip_reason,
                Some(FlowSkipReason::BranchNotTaken),
                "node {id}"
            );
        }
    }

    #[tokio::test]
    async fn transform_after_a_taken_branch_runs() {
        let service = service_with_flow(transform_after_if_flow("tf-taken"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![
                ("!!(", Scripted::Value(serde_json::json!(true))),
                ("__requireValue(", Scripted::Value(serde_json::json!("PRO"))),
                (
                    "response.body",
                    Scripted::FromResponse(|r| {
                        serde_json::json!(r.map(|r| r.body.clone()).unwrap_or_default())
                    }),
                ),
            ]),
        );

        let summary = service.run(&exec, run_input("tf-taken")).await.expect("run");

        assert_eq!(status_of(&summary, "t"), FlowNodeStatus::Success);
        assert_eq!(step_of(&summary, "out").value.as_deref(), Some("PRO"));
    }

    #[tokio::test]
    async fn one_transform_can_feed_two_consumers() {
        let flow = Flow {
            name: "tf-fan".to_string(),
            nodes: vec![
                input_node_with("in", "pro"),
                transform_node("t", "return response.body;"),
                output_node_named("out1"),
                output_node_named("out2"),
            ],
            edges: vec![
                input_edge("e1", "in", "t"),
                edge_from("e2", "t", handle::RESULT, "out1", "value", "response.body"),
                edge_from("e3", "t", handle::RESULT, "out2", "value", "response.body"),
            ],
            callback_host: None,
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            transform_engine(Scripted::Value(serde_json::json!("SHARED"))),
        );

        let summary = service.run(&exec, run_input("tf-fan")).await.expect("run");

        assert_eq!(step_of(&summary, "out1").value.as_deref(), Some("SHARED"));
        assert_eq!(step_of(&summary, "out2").value.as_deref(), Some("SHARED"));
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -j4 -p rocket-app transform`
Expected: PASS for every test with `transform` in its name. If a test fails, the failure points at a real defect in Task 2's arm or the fate rules: fix the production code, not the test, unless the test itself is wrong about a helper's signature.

- [ ] **Step 3: Run the wider checks**

Run: `cargo test -j4 -p rocket-app flow` then `cargo test -j4 -p rocket-flow`
Expected: PASS for both.

Run: `cargo fmt --all -- --check` and `cargo clippy -j4 -p rocket-app -- -D warnings`
Expected: no output and no warnings.

Run: `cargo check -j4 --workspace`
Expected: PASS.

- [ ] **Step 4: Commit**

Stage `crates/rocket-app/src/flow_execution_service.rs`, then commit through the `dev-workflow-skills:1-git-commit` skill with subject `test(flow): cover Transform masking, skips and fan-out`.

---

## Next Plan

**Next plan to execute:** `docs/superpowers/plans/flow-phase3-transform-node/2026-09-30-flow-phase3-transform-plan-03-frontend-types-and-wiring.md`

Do not start it in this run. Finish the review below, report to the user, and wait for them to start plan 03.

## Post-Implementation Review

Before plan 03 starts, dispatch one Opus-model subagent (read and fix allowed) with this brief: "Review every change made by plan 02 of `docs/superpowers/plans/flow-phase3-transform-node/`, using `git log` for its three commits. Check: (a) the generated JS in `flow_script` still works for Raw, Bool and Str coercion and the guard is only called for Required (run `cargo test -j4 -p rocket-app flow`); (b) `decide_fate` and `is_live` are unchanged; (c) the reported step value is masked while wires get the raw text; (d) no panicking calls were added outside tests; (e) `single_input` replaced every use of the old name. Fix any defect you find and report what changed. Do not start plan 03."
