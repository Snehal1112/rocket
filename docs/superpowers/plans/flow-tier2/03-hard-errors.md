# Flow Tier 2 Hard Errors Implementation Plan (Plan 3 of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make three more structural mistakes impossible to save or run: a wire into a Request node that names a field the engine cannot apply (F-26), two wires that share an id, and an Output node that does not have exactly one value wire (F-27).

**Architecture:** All three are hard errors from `rocket_flow::validate`, so `FlowService::save` and `load_ordered_nodes` (run) both refuse them. Task 1 adds rule V16 and a public `is_request_wire_field` that is the one spelling of the field whitelist. Task 2 adds `FlowGraphError::DuplicateEdge` to `topological_sort` and maps it in `graph_error_message` and `graph_error_lints`. Task 3 adds rule V17 and moves the client `output-no-value` issue from a warning to an error so the canvas and the backend agree. Lint sentences never echo user-typed text, so every new reason also gets a fixed lint sentence.

**Tech Stack:** Rust (`rocket-flow`, `rocket-app`, `rocket-infra` tests), serde_yaml (fixture), React, TypeScript, Vitest, Biome.

**Spec:** `.claude/flow-roadmap.md` rows F-26 and F-27 (Tier 2). Phase 1 spec section 6 of `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`: "Output nodes have exactly one incoming wire and no outgoing ones".

## Global Constraints

- Rust gates: `cargo check -j4 -p <crate>` and `cargo test -j4 -p <crate> <filter>`. Never `--workspace` or `--all`. Never run `cargo fmt` over a crate. Run `rustfmt --check --edition 2021 <file>` on a file you edited and fix only diffs inside lines you added.
- No unwrap or expect calls in production code. Test code may use `expect("message")`.
- Serde `camelCase` only on IPC DTOs. This plan adds no DTO and no persistence field.
- Frontend gates: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern> --run --maxWorkers=2`.
- Comments are short full sentences ending in a punctuation mark. No emojis.
- Commit through the `dev-workflow-skills:1-git-commit` skill with explicit paths (`git add <paths> && git commit --only -m "..." -- <paths>`). Never `git add -A`.
- Task 1 reads and parses a flow `.yml` and touches a `rocket-infra` test, so it starts with the OpenCollection read. Tasks 2 and 3 touch no `.yml` and none of the listed crates.
- The error text format is fixed: a message that names graph elements ends with `node(s): <ids>; edge(s): <ids>` (`graph_error_message`, parsed by `parseGraphErrorMessage` in `src/lib/flow-wiring.ts`). Do not change it.
- New reasons may quote the node label and the field in the save message. They must never reach a lint sentence (`lint.rs` `sentence`) except as a fixed sentence, because the lint tier never echoes typed text.

## Review Focus

- A wire into a Request node is accepted only for `url`, `body`, `trigger`, `auth` and `headers[<index or name>].value`, and the error names the node label and the field (Task 1, `v16_*` tests).
- Every flow the editor can produce today stays valid, and `is_request_wire_field` agrees with `apply_wired_overrides` field by field (Task 1, YAML fixture test, legacy repo test and the agreement test).
- The lint for a bad Request field and for a bad Output wire count never contains the typed field (Task 1 canary test, Task 3 lint test).
- A repeated edge id is reported before an edge to an unknown node, as `DuplicateEdge`, and the save error ends with `node(s): ; edge(s): <id>` (Task 2).
- An Output counts only non-`trigger` wires, zero and two both fail with the node label, the rule runs after all edge rules, and the canvas message equals the backend message so a rejected save adds no second line (Task 3).

---

### Task 1: Unknown Request wire field is a hard error (F-26)

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Real field set (read from the code, not guessed).**

- Engine, `apply_wired_overrides` in `crates/rocket-app/src/flow_execution_service.rs` (~564-606): `url`, `body`, `headers[N].value` (all-digit `N`), `headers[Name].value` (any other non-blank `Name`). `auth` is applied earlier by the Request arm (~1649) and `trigger` is filtered out before it (`decide_fate` in `flow_routing.rs`, ~108). Everything else, including a bare `headers`, errors at run time with "unrecognized target_field".
- Editor, `src/lib/flow-wiring.ts`: `REQUEST_TARGETS = ['url', 'headers', 'body', trigger, auth]`. The bare `headers` handle is transient: `WireScriptDialog.tsx` rewrites it to `headers[<Name>].value` on commit and `FlowPane.tsx` (~467-472, `isUncommittedHeadersEdge`) drops it when the popover is dismissed. No frontend change is needed.
- Other kinds are already constrained: If, Switch and Transform take `input` only (V1), Wait takes `trigger` only, Input and Auth take no wires, Output takes `value` and `trigger` in the editor but the validator does not police its field (the engine ignores the name). This plan leaves Output fields alone (see Open decisions).

**Form and binary bodies.** `InlineRequestData.body` is `Option<String>` and the engine builds every inline body as `BodyMode::Json` (`flow_execution_service.rs` ~548). An inline request therefore can never have a form or binary body, so there is nothing to reject for inline requests. A saved request's body mode cannot be read in the pure `rocket-flow` crate. That half of F-26 stays a run-time error (`cannot wire a value into a FormUrlEncoded body`) and moves to a non-blocking lint, see Open decisions. A pinning test shows an inline body wire stays valid.

**Migration risk.** A flow on disk with a Request wire to an unknown field (a hand-edited file, a bare `headers` saved by an old build, a `value` wire into a Request) used to save, and it ran as long as that wire sat on a branch that was never taken. After this task it fails to save and fails to run. The flow still opens and the failing wire is highlighted through the `edge(s):` suffix. Recommended fix path: no automatic rewrite. The message names the node label and the field, the user deletes the wire or redraws it, and the release note says so.

**Files:**
- Modify: `crates/rocket-flow/src/validate.rs` (header comment line 2, `validate` ~46, new helpers after `check_single_auth_wire` ~170, tests at end of `mod tests` ~1470)
- Modify: `crates/rocket-flow/src/lint.rs` (`sentence` ~370-388, tests after `a_duplicate_switch_match_value_is_never_echoed` ~1068)
- Modify: `crates/rocket-app/src/flow_service.rs` (test after `save_rejects_a_bad_edge_and_names_it_in_the_tail` ~375)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (test after `unrecognized_target_field_is_an_error_not_a_silent_noop` ~3121)
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs` (`a_legacy_flow_file_loads_through_the_repo` ~735)
- Modify: `crates/rocket-flow/CLAUDE.md` (module table row for `validate.rs`, new section after "Auth node")

**Interfaces:**
- Produces in `rocket_flow::validate`:
  - `pub fn is_request_wire_field(field: &str) -> bool` is true for `url`, `body`, `trigger`, `auth` and `headers[<selector>].value` with a non-blank selector.
  - Rule V16 in `validate`: an `InvalidEdge { edge_id, reason }` with reason `the Request node '<label>' has no field '<field>' to wire into; use url, body, headers[Name].value, auth or trigger`.
- Produces in `lint.rs`: `sentence` maps that reason to the fixed text `A wire targets a field that the Request node does not have.`
- Consumes: `check_edges`, `handle::TRIGGER`, `handle::AUTH`, `FlowNodeKind::Request { label, .. }` (existing).

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-flow/src/validate.rs`, inside `mod tests`, add these helpers and tests at the end (the helpers `node`, `request`, `input`, `output`, `edge`, `flow`, `inline_request` and `invalid_edge_id` already exist):

```rust
    fn invalid_edge_reason(result: Result<Vec<String>, FlowGraphError>) -> String {
        match result {
            Err(FlowGraphError::InvalidEdge { reason, .. }) => reason,
            other => panic!("expected InvalidEdge, got {other:?}"),
        }
    }

    fn labelled_request(id: &str, label: &str) -> FlowNode {
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

    #[test]
    fn v16_unknown_request_field_is_rejected_naming_the_label_and_the_field() {
        let f = flow(
            vec![input("in"), labelled_request("req", "Get user")],
            vec![edge("e1", "in", handle::RESULT, "req", "payload")],
        );
        let result = validate(&f);
        let reason = invalid_edge_reason(result.clone());
        assert!(reason.contains("'Get user'"), "{reason}");
        assert!(reason.contains("'payload'"), "{reason}");
        assert_eq!(invalid_edge_id(result), "e1");
    }

    #[test]
    fn v16_rejects_fields_the_engine_cannot_apply() {
        for field in [
            "headers",
            "headers[].value",
            "headers[ ].value",
            "headers[X].v",
            "value",
            "Url",
            "auth.token",
            "",
        ] {
            let f = flow(
                vec![input("in"), request("req")],
                vec![edge("e1", "in", handle::RESULT, "req", field)],
            );
            assert_eq!(invalid_edge_id(validate(&f)), "e1", "field {field:?}");
        }
    }

    #[test]
    fn v16_accepts_every_field_the_engine_applies() {
        for field in [
            "url",
            "body",
            handle::TRIGGER,
            "headers[0].value",
            "headers[Authorization].value",
            "headers[X-Request Id].value",
        ] {
            let f = flow(
                vec![input("in"), request("req")],
                vec![edge("e1", "in", handle::RESULT, "req", field)],
            );
            assert!(validate(&f).is_ok(), "field {field:?}: {:?}", validate(&f));
        }
    }

    #[test]
    fn v16_allows_a_body_wire_into_an_inline_request() {
        // An inline body is always sent as JSON text, so a body wire cannot
        // hit a form or binary body here. Saved requests are not inspected.
        let f = flow(
            vec![input("in"), inline_request("req", "https://x.test")],
            vec![edge("e1", "in", handle::RESULT, "req", "body")],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    #[test]
    fn v16_leaves_the_fields_of_other_node_kinds_alone() {
        let f = flow(
            vec![input("in"), output("out")],
            vec![edge("e1", "in", handle::RESULT, "out", "payload")],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    /// A flow file as the editor writes it today: Saved and Inline requests,
    /// every wireable Request field, a Run when wire and one Output value wire.
    const WIRED_FLOW_YAML: &str = r"
name: Wired Flow
nodes:
- id: in1
  kind:
    kind: Input
    label: Token
    value: abc
  position: {x: 0.0, y: 0.0}
- id: login
  kind:
    kind: Request
    label: Login
    source:
      type: Saved
      request_path: auth/login.yml
  position: {x: 200.0, y: 0.0}
- id: create
  kind:
    kind: Request
    label: Create item
    source:
      type: Inline
      request:
        method: POST
        url: https://api.example.com/items
        headers:
        - name: X-Token
          value: old
        body: '{}'
  position: {x: 400.0, y: 0.0}
- id: out1
  kind:
    kind: Output
    label: Result
  position: {x: 600.0, y: 0.0}
edges:
- {id: e1, source_node_id: in1, target_node_id: login, target_field: url, expression: response.body}
- {id: e2, source_node_id: in1, target_node_id: login, target_field: 'headers[X-Token].value', expression: response.body}
- {id: e3, source_node_id: in1, target_node_id: login, target_field: 'headers[0].value', expression: response.body}
- {id: e4, source_node_id: in1, target_node_id: login, target_field: body, expression: response.body}
- {id: e5, source_node_id: login, target_node_id: create, target_field: trigger, expression: ''}
- {id: e6, source_node_id: create, target_node_id: out1, target_field: value, expression: response.body}
";

    #[test]
    fn a_flow_file_using_every_editor_field_stays_valid() {
        let f: Flow = serde_yaml::from_str(WIRED_FLOW_YAML).expect("fixture parses");
        assert_eq!(f.edges.len(), 6);
        validate(&f).expect("the fixture must stay valid");
    }
```

In `crates/rocket-flow/src/lint.rs`, inside `mod tests`, after `a_duplicate_switch_match_value_is_never_echoed`, add:

```rust
    #[test]
    fn a_bad_request_field_is_linted_without_echoing_the_field() {
        let f = flow(
            vec![input("in1"), request("req")],
            vec![edge("e1", "in1", handle::RESULT, "req", CANARY)],
        );
        let graph = graph_lints(&f);
        assert_eq!(keys(&graph), vec![(None, Some("e1"), INVALID_GRAPH)]);
        assert_no_canary(&graph);
        assert_eq!(
            graph[0].message,
            "A wire targets a field that the Request node does not have."
        );
    }
```

In `crates/rocket-app/src/flow_service.rs`, inside `mod tests`, after `save_rejects_a_bad_edge_and_names_it_in_the_tail`, add:

```rust
    #[test]
    fn save_rejects_an_unknown_request_field_and_names_the_wire_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Bad Field".to_string(),
            nodes: vec![
                node_of(
                    "in",
                    FlowNodeKind::Input {
                        label: "Seed".to_string(),
                        value: rocket_shared::VariableValue::simple("x"),
                    },
                ),
                node_of(
                    "req",
                    FlowNodeKind::Request {
                        label: "Get user".to_string(),
                        source: rocket_flow::RequestSource::Saved {
                            request_path: "users/get.yml".to_string(),
                        },
                        debug: false,
                        repeat_until: None,
                    },
                ),
            ],
            edges: vec![FlowEdge {
                id: "e5".to_string(),
                source_node_id: "in".to_string(),
                target_node_id: "req".to_string(),
                target_field: "payload".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an unknown Request field must be rejected")
            .to_string();
        assert!(
            message.contains("the Request node 'Get user' has no field 'payload'"),
            "got: {message}"
        );
        assert!(
            message.ends_with("node(s): ; edge(s): e5"),
            "got: {message}"
        );
    }
```

In `crates/rocket-app/src/flow_execution_service.rs`, inside `mod tests`, after `unrecognized_target_field_is_an_error_not_a_silent_noop`, add:

```rust
    #[test]
    fn the_validator_and_the_engine_agree_on_request_wire_fields() {
        // `auth` and `trigger` never reach `apply_wired_overrides`, so they are not listed.
        for field in [
            "url",
            "body",
            "headers[0].value",
            "headers[X-Id].value",
            "headers[ x-y ].value",
            "headers",
            "headers[].value",
            "headers[ ].value",
            "payload",
            "value",
            "Url",
            "auth.token",
        ] {
            let mut input = sample_execute_input();
            let mut resolved = HashMap::new();
            resolved.insert("e1".to_string(), "v".to_string());
            let applied = apply_wired_overrides(&mut input, &resolved, &[edge("e1", "n2", field)]);
            assert_eq!(
                applied.is_ok(),
                rocket_flow::validate::is_request_wire_field(field),
                "field {field:?}"
            );
        }
    }
```

In `crates/rocket-infra/src/fs_flow_repo.rs`, in `a_legacy_flow_file_loads_through_the_repo`, add after the `assert_eq!(loaded, parsed);` line:

```rust
        rocket_flow::validate(&loaded).expect("a legacy flow stays valid");
```

- [ ] **Step 2: Run the tests and state the expected failure**

Run: `cargo test -j4 -p rocket-flow v16` (the unknown-field, rejects-fields and lint tests FAIL with `expected InvalidEdge, got Ok(...)`; the accept, inline-body, other-kinds tests PASS because nothing rejects yet).
Run: `cargo test -j4 -p rocket-flow a_flow_file_using_every_editor_field` (PASS: it pins the current behaviour before the rule exists).
Run: `cargo test -j4 -p rocket-app the_validator_and_the_engine_agree` (FAILS to compile: `is_request_wire_field` is not defined).
Run: `cargo test -j4 -p rocket-app save_rejects_an_unknown_request_field` (FAILS: `save` returns Ok, so `expect_err` panics).
Run: `cargo test -j4 -p rocket-infra a_legacy_flow_file_loads_through_the_repo` (PASS, pins the legacy fixture).

- [ ] **Step 3: Write the implementation**

In `crates/rocket-flow/src/validate.rs`:

1. Change line 2 from `§7 (rules V1-V15)` to `§7 (rules V1-V17)`.

2. In `validate`, after `check_single_auth_wire(flow)?;` insert:

```rust
    check_edges(flow, &kinds, |edge, target, _| {
        let FlowNodeKind::Request { label, .. } = target else {
            return None;
        };
        (!is_request_wire_field(&edge.target_field)).then(|| {
            format!(
                "the Request node '{label}' has no field '{}' to wire into; use url, body, headers[Name].value, auth or trigger",
                edge.target_field
            )
        })
    })?;
```

3. After `check_single_auth_wire` add:

```rust
/// The Request fields a wire can target, besides `headers[...]`.
const REQUEST_FIELDS: [&str; 4] = ["url", "body", handle::TRIGGER, handle::AUTH];

/// V16: true when a wire into a Request node may target `field`. These are the
/// fields the editor offers and `apply_wired_overrides` applies: `url`,
/// `body`, `trigger`, `auth`, and `headers[<index or name>].value` with a
/// non-blank selector. Anything else would fail at run time, or do nothing.
pub fn is_request_wire_field(field: &str) -> bool {
    REQUEST_FIELDS.contains(&field)
        || field
            .strip_prefix("headers[")
            .and_then(|rest| rest.strip_suffix("].value"))
            .is_some_and(|selector| !selector.trim().is_empty())
}
```

In `crates/rocket-flow/src/lint.rs`, in `sentence`, replace the final `} else {\n        routed_reason(reason).or_else(|| known_reason(reason))\n    };` with:

```rust
    } else if reason.starts_with("the Request node '") && reason.contains("' has no field '") {
        Some("A wire targets a field that the Request node does not have.".to_string())
    } else {
        routed_reason(reason).or_else(|| known_reason(reason))
    };
```

In `crates/rocket-flow/CLAUDE.md`:

- In the module table, change the `validate.rs` row to: `` `validate` — `topological_sort` plus structural rules V1–V17 (spec §7, listed in the file header), and `is_request_wire_field` ``.
- After the "Auth node" section, add:

```markdown
## Hard wire and graph rules

After V1-V15, `validate` rejects: V16 (an edge rule) a wire into a Request node whose `target_field` is not `url`, `body`, `trigger`, `auth` or `headers[<index or name>].value`. `is_request_wire_field` is the one spelling of that list, and a `rocket-app` test pins it against `apply_wired_overrides`. Other node kinds keep their own field rules (V1-V4); Output fields are not policed. `rocket-flow` cannot read a saved request, so a `body` wire into a saved form or binary body is still only caught at run time. Inline bodies are always JSON text, so they cannot hit that case.
```

- [ ] **Step 4: Run the tests and the gates**

Run: `cargo test -j4 -p rocket-flow` (all PASS).
Run: `cargo test -j4 -p rocket-app flow_service` and `cargo test -j4 -p rocket-app the_validator_and_the_engine_agree` (PASS).
Run: `cargo test -j4 -p rocket-app flow` to catch any existing fixture that wires a Request to an unusual field. If one fails, change that fixture's field to `url`, `body` or `trigger` (do not widen the whitelist) and record it in the commit message body.
Run: `cargo test -j4 -p rocket-infra a_legacy_flow` (PASS).
Run: `cargo check -j4 -p rocket-flow -p rocket-app -p rocket-infra`.
Run: `rustfmt --check --edition 2021 crates/rocket-flow/src/validate.rs` and fix only diffs inside lines this task added.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs crates/rocket-flow/CLAUDE.md crates/rocket-app/src/flow_service.rs crates/rocket-app/src/flow_execution_service.rs crates/rocket-infra/src/fs_flow_repo.rs && git commit --only -m "feat(flow): reject wires to unknown Request fields" -- crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs crates/rocket-flow/CLAUDE.md crates/rocket-app/src/flow_service.rs crates/rocket-app/src/flow_execution_service.rs crates/rocket-infra/src/fs_flow_repo.rs`

---

### Task 2: Duplicate edge ids are a hard error (F-27a)

**Files:**
- Modify: `crates/rocket-flow/src/graph.rs` (enum ~6-24, doc and body of `topological_sort` ~26-50, tests after `parallel_edges_count_twice_and_keep_the_exact_order` ~290)
- Modify: `crates/rocket-flow/src/validate.rs` (test `new_error_variants_display_their_id_and_reason` ~988, one new test)
- Modify: `crates/rocket-flow/src/lint.rs` (`graph_error_lints` ~324, test after `a_duplicate_node_id_is_reported_on_that_id` ~1037)
- Modify: `crates/rocket-app/src/flow_service.rs` (`graph_error_message` ~78, test after the Task 1 test)
- Modify: `src/lib/__tests__/flow-wiring.test.ts` (`parseGraphErrorMessage` describe ~250)
- Modify: `src/lib/__tests__/flow-issues.test.ts` (`cleanSaveMessage` describe ~279)
- Modify: `crates/rocket-flow/CLAUDE.md` (the section added in Task 1)

**Choice.** A new variant `FlowGraphError::DuplicateEdge { edge_id }` mirrors `DuplicateNode` and keeps the lint mapping and the test assertions exact. The only `match` on `FlowGraphError` outside `rocket-flow` is `graph_error_message` in `rocket-app/src/flow_service.rs` (grep of `crates` and `src-tauri` finds no other). Inside `rocket-flow` the only match is `graph_error_lints`. The Tauri layer and the frontend only see the message string.

**Interfaces:**
- Produces: `FlowGraphError::DuplicateEdge { edge_id: String }` with display `duplicate edge id: <id>`.
- Produces: `topological_sort` order of checks: duplicate node id, duplicate edge id, edge to unknown node.
- Produces: save message `flow has more than one wire with id: <id> — node(s): ; edge(s): <id>`.
- Produces: lint `invalid_graph` on the edge with message `More than one wire has the same id.`
- Consumes: `FlowEdge.id`, `FIX` hint constant in `lint.rs`.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-flow/src/graph.rs` `mod tests`, add:

```rust
    #[test]
    fn duplicate_edge_ids_are_rejected_and_name_the_id() {
        let flow = Flow {
            name: "dup-edge".to_string(),
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![edge("e1", "a", "b"), edge("e2", "b", "c"), edge("e1", "a", "c")],
            callback_host: None,
        };
        assert_eq!(
            topological_sort(&flow),
            Err(FlowGraphError::DuplicateEdge {
                edge_id: "e1".to_string()
            })
        );
    }

    #[test]
    fn a_duplicate_edge_id_is_reported_before_an_unknown_node() {
        let flow = Flow {
            name: "dup-then-ghost".to_string(),
            nodes: vec![node("a"), node("b")],
            edges: vec![edge("e1", "a", "ghost"), edge("e1", "a", "b")],
            callback_host: None,
        };
        assert_eq!(
            topological_sort(&flow),
            Err(FlowGraphError::DuplicateEdge {
                edge_id: "e1".to_string()
            })
        );
    }
```

In `crates/rocket-flow/src/validate.rs` `mod tests`, extend `new_error_variants_display_their_id_and_reason` by appending before its closing brace:

```rust
        let dup_edge = FlowGraphError::DuplicateEdge {
            edge_id: "e1".to_string(),
        };
        assert_eq!(dup_edge.to_string(), "duplicate edge id: e1");
```

and add:

```rust
    #[test]
    fn validate_reports_a_duplicate_edge_id() {
        let f = flow(
            vec![input("in"), output("a"), output("b")],
            vec![
                edge("e1", "in", handle::RESULT, "a", "value"),
                edge("e1", "in", handle::RESULT, "b", "value"),
            ],
        );
        assert_eq!(
            validate(&f),
            Err(FlowGraphError::DuplicateEdge {
                edge_id: "e1".to_string()
            })
        );
    }
```

In `crates/rocket-flow/src/lint.rs` `mod tests`, after `a_duplicate_node_id_is_reported_on_that_id`, add:

```rust
    #[test]
    fn a_duplicate_edge_id_is_reported_on_that_wire() {
        let f = flow(
            vec![input("in1"), output("a"), output("b")],
            vec![
                edge("e1", "in1", handle::RESULT, "a", "value"),
                edge("e1", "in1", handle::RESULT, "b", "value"),
            ],
        );
        let lints = graph_lints(&f);
        assert_eq!(keys(&lints), vec![(None, Some("e1"), INVALID_GRAPH)]);
        assert_eq!(lints[0].message, "More than one wire has the same id.");
        assert!(lints[0].hint.is_some());
    }
```

In `crates/rocket-app/src/flow_service.rs` `mod tests`, add:

```rust
    #[test]
    fn save_rejects_a_duplicate_edge_id_and_names_it_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let out = |id: &str| {
            node_of(
                id,
                FlowNodeKind::Output {
                    label: id.to_string(),
                },
            )
        };
        let wire_to = |to: &str| FlowEdge {
            id: "e7".to_string(),
            source_node_id: "a".to_string(),
            target_node_id: to.to_string(),
            target_field: "value".to_string(),
            expression: "response.body".to_string(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        };
        let flow = Flow {
            name: "Twin Wires".to_string(),
            nodes: vec![out("a"), out("b"), out("c")],
            edges: vec![wire_to("b"), wire_to("c")],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("a repeated edge id must be rejected")
            .to_string();
        assert!(
            message.contains("more than one wire with id: e7"),
            "got: {message}"
        );
        assert!(
            message.ends_with("node(s): ; edge(s): e7"),
            "got: {message}"
        );
    }
```

In `src/lib/__tests__/flow-wiring.test.ts`, inside `describe('parseGraphErrorMessage', ...)`, after the "parses an edge validation error whose node list is empty" test, add:

```ts
  it('parses a duplicate wire id rejection', () => {
    const message =
      'Invalid input: flow has more than one wire with id: e7 — node(s): ; edge(s): e7';
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: [], edgeIds: ['e7'] });
  });
```

In `src/lib/__tests__/flow-issues.test.ts`, inside `describe('cleanSaveMessage', ...)`, after its last `it`, add:

```ts
  it('words a duplicate wire id rejection without the id lists', () => {
    expect(
      cleanSaveMessage(
        'Invalid input: flow has more than one wire with id: e7 — node(s): ; edge(s): e7',
      ),
    ).toBe('Flow has more than one wire with id: e7.');
  });
```

- [ ] **Step 2: Run the tests and state the expected failure**

Run: `cargo test -j4 -p rocket-flow duplicate_edge` (FAILS to compile: `no variant named DuplicateEdge`; every Rust test in the crate fails with it until the variant exists).
Run: `yarn test flow-wiring flow-issues --run --maxWorkers=2` (the two new TypeScript tests PASS already. They pin the message format the backend must now produce, they do not fail first).

- [ ] **Step 3: Write the implementation**

In `crates/rocket-flow/src/graph.rs`, add the variant after `DuplicateNode`:

```rust
    /// Two edges in `flow.edges` share the same id.
    #[error("duplicate edge id: {edge_id}")]
    DuplicateEdge { edge_id: String },
```

Update the `topological_sort` doc paragraph to: `Duplicate node ids, duplicate edge ids and edges to unknown nodes are rejected, in that order, before any sorting starts.` Then insert after the node-id loop and before the unknown-node loop:

```rust
    let mut seen_edges: HashSet<&str> = HashSet::with_capacity(flow.edges.len());
    for edge in &flow.edges {
        if !seen_edges.insert(edge.id.as_str()) {
            return Err(FlowGraphError::DuplicateEdge {
                edge_id: edge.id.clone(),
            });
        }
    }
```

In `crates/rocket-flow/src/lint.rs`, in `graph_error_lints`, add after the `DuplicateNode` arm:

```rust
        FlowGraphError::DuplicateEdge { edge_id } => {
            let message = "More than one wire has the same id.".to_string();
            vec![lint(None, Some(edge_id.as_str()), message, FIX)]
        }
```

In `crates/rocket-app/src/flow_service.rs`, in `graph_error_message`, add after the `DuplicateNode` arm:

```rust
        FlowGraphError::DuplicateEdge { edge_id } => format!(
            "flow has more than one wire with id: {edge_id} — node(s): ; edge(s): {edge_id}"
        ),
```

In `crates/rocket-flow/CLAUDE.md`, in the "Hard wire and graph rules" section, append: `` `topological_sort` rejects, in order, duplicate node ids, duplicate edge ids (`FlowGraphError::DuplicateEdge`) and edges to unknown nodes. ``

- [ ] **Step 4: Run the tests and the gates**

Run: `cargo test -j4 -p rocket-flow` (all PASS). If a fixture fails because it reuses one edge id, give the edges distinct ids and note it in the commit body.
Run: `cargo test -j4 -p rocket-app flow` (all PASS; same fixture rule).
Run: `cargo check -j4 -p rocket-flow -p rocket-app` (a missing match arm anywhere would fail here).
Run: `yarn tsc --noEmit`, `yarn check`, `yarn test flow-wiring flow-issues --run --maxWorkers=2`.
Run: `rustfmt --check --edition 2021 crates/rocket-flow/src/graph.rs`, fixing only diffs in added lines.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-flow/src/graph.rs crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs crates/rocket-flow/CLAUDE.md crates/rocket-app/src/flow_service.rs src/lib/__tests__/flow-wiring.test.ts src/lib/__tests__/flow-issues.test.ts && git commit --only -m "feat(flow): reject duplicate edge ids" -- crates/rocket-flow/src/graph.rs crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs crates/rocket-flow/CLAUDE.md crates/rocket-app/src/flow_service.rs src/lib/__tests__/flow-wiring.test.ts src/lib/__tests__/flow-issues.test.ts`

---

### Task 3: An Output node has exactly one value wire (F-27b)

**The rule.** Phase 1 spec section 6 (`2026-09-27-flow-visual-workflow-builder-design.md`): "Output nodes have exactly one incoming wire and no outgoing ones". "No outgoing ones" is already V5 (`source_handle_exists` returns false for Output). The "Run when" `trigger` input was added to Output after that spec, and a trigger carries no value. The engine agrees: `decide_fate` filters `trigger` edges out of `data_edges` (`flow_routing.rs` ~108), and the Output arm (`flow_execution_service.rs` ~1581-1595) returns an empty value for zero data edges and fails for more than one. So the rule counts every incoming edge whose `target_field` is not `trigger`. Today the engine tolerates zero (it shows an empty value) and the client only warns. This task makes zero an error as the roadmap asks, which is the largest behaviour change in this plan, see the risk below.

**Migration and workflow risk.** (1) A flow on disk with an Output that has no value wire, or two, used to save and run (the second case failed only that Output step). After this task it fails to save and run until fixed. (2) A user who drops an Output on the canvas cannot save the flow until it is wired. The client now shows the error on the node at once, so the cause is visible. Recommended fix path: no automatic rewrite. The message names the node label, and the canvas highlights the node through the `node(s):` suffix.

**Files:**
- Modify: `crates/rocket-flow/src/validate.rs` (`validate` ~74, new helper after `check_auth_nodes`, existing test `v3_trigger_into_request_and_output_is_valid` ~783, new tests)
- Modify: `crates/rocket-flow/src/lint.rs` (import line 12, `sentence`, new `output_wire_count`, test after Task 1's lint test)
- Modify: `crates/rocket-app/src/flow_service.rs` (`sample_flow` ~97, new test)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (test `output_node_with_two_incoming_wires_fails_instead_of_dropping_one` ~5123)
- Modify: `src/lib/flow-issues.ts` (imports, `outputIssues` ~85)
- Modify: `src/lib/__tests__/flow-issues.test.ts` (test ~87)
- Modify: `crates/rocket-flow/CLAUDE.md` (the section from Tasks 1 and 2)

**Interfaces:**
- Produces in `validate.rs`: `pub(crate) const OUTPUT_WIRE_TAIL: &str = "' needs exactly one value wire (Run when wires do not count)";` and rule V17 `InvalidNode { node_id, reason }` with reason `the Output node '<label>' needs exactly one value wire (Run when wires do not count), found <n>`. V17 runs last, so every earlier rule keeps its error for flows that already failed.
- Produces in `lint.rs`: `sentence` maps that reason to `The Output node needs exactly one value wire, found <n>.` The lint prefix already adds `'<label>': `.
- Produces in `flow-issues.ts`: issue `output-no-value` (count 0) and `output-many-values` (count above 1), both `error`, message `The Output node '<label>' needs exactly one value wire (Run when wires do not count), found <n>.` That is the backend reason after `cleanSaveMessage`, so `dropRepeatedSaveIssues` removes the duplicate save line.
- Consumes: `TRIGGER_HANDLE` from `@/lib/flow-handles`, `issue` helper in `flow-issues.ts`, `handle::TRIGGER`.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-flow/src/validate.rs` `mod tests`:

Replace the existing `v3_trigger_into_request_and_output_is_valid` with (the Output now also needs a value wire):

```rust
    #[test]
    fn v3_trigger_into_request_and_output_is_valid() {
        let f = flow(
            vec![request("a"), request("b"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "b", handle::TRIGGER),
                edge("e2", "a", handle::RESULT, "out", handle::TRIGGER),
                edge("e3", "b", handle::RESULT, "out", "value"),
            ],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }
```

and add:

```rust
    fn labelled_output(id: &str, label: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Output {
                label: label.to_string(),
            },
        )
    }

    #[test]
    fn v17_output_without_a_value_wire_is_rejected_with_its_label() {
        let f = flow(vec![input("in"), labelled_output("out", "Final answer")], vec![]);
        let result = validate(&f);
        let reason = invalid_node_reason(result.clone());
        assert_eq!(
            reason,
            "the Output node 'Final answer' needs exactly one value wire (Run when wires do not count), found 0"
        );
        assert_eq!(invalid_node_id(result), "out");
    }

    #[test]
    fn v17_a_run_when_wire_is_not_a_value_wire() {
        let f = flow(
            vec![input("in"), output("out")],
            vec![edge("e1", "in", handle::RESULT, "out", handle::TRIGGER)],
        );
        let reason = invalid_node_reason(validate(&f));
        assert!(reason.ends_with("found 0"), "{reason}");
    }

    #[test]
    fn v17_two_value_wires_are_rejected() {
        let f = flow(
            vec![input("a"), input("b"), labelled_output("out", "Final answer")],
            vec![
                edge("e1", "a", handle::RESULT, "out", "value"),
                edge("e2", "b", handle::RESULT, "out", "value"),
            ],
        );
        let result = validate(&f);
        let reason = invalid_node_reason(result.clone());
        assert!(reason.contains("'Final answer'"), "{reason}");
        assert!(reason.ends_with("found 2"), "{reason}");
        assert_eq!(invalid_node_id(result), "out");
    }

    #[test]
    fn v17_one_value_wire_with_several_run_when_wires_is_valid() {
        let f = flow(
            vec![input("a"), request("r1"), request("r2"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "out", "value"),
                edge("e2", "r1", handle::RESULT, "out", handle::TRIGGER),
                edge("e3", "r2", handle::RESULT, "out", handle::TRIGGER),
            ],
        );
        assert!(validate(&f).is_ok(), "{:?}", validate(&f));
    }

    #[test]
    fn v17_runs_after_the_edge_rules() {
        // The existing V5 test keeps its InvalidEdge even though `out` has no
        // incoming wire, because V17 is the last rule.
        let f = flow(
            vec![output("out"), request("b")],
            vec![edge("e1", "out", handle::RESULT, "b", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }
```

In `crates/rocket-flow/src/lint.rs` `mod tests`, after `a_bad_request_field_is_linted_without_echoing_the_field`, add:

```rust
    #[test]
    fn an_output_without_a_value_wire_is_linted_with_its_label_once() {
        let f = flow(
            vec![
                input("in1"),
                node(
                    "out",
                    FlowNodeKind::Output {
                        label: "Final".to_string(),
                    },
                ),
            ],
            vec![],
        );
        let graph = graph_lints(&f);
        assert_eq!(keys(&graph), vec![(Some("out"), None, INVALID_GRAPH)]);
        assert_eq!(
            graph[0].message,
            "'Final': The Output node needs exactly one value wire, found 0."
        );
    }

    #[test]
    fn an_output_label_that_imitates_the_count_cannot_change_the_sentence() {
        let f = flow(
            vec![
                input("in1"),
                node(
                    "out",
                    FlowNodeKind::Output {
                        label: "x', found 9".to_string(),
                    },
                ),
            ],
            vec![],
        );
        let graph = graph_lints(&f);
        assert!(
            graph[0].message.ends_with("The Output node needs exactly one value wire, found 0."),
            "{}",
            graph[0].message
        );
    }
```

In `crates/rocket-app/src/flow_service.rs` `mod tests`, replace `sample_flow` with a version whose Output is wired (this fixture feeds `save_persists_an_acyclic_flow` and the repo tests):

```rust
    fn sample_flow() -> Flow {
        Flow {
            name: "Login Then Fetch".to_string(),
            nodes: vec![
                FlowNode {
                    id: "n0".to_string(),
                    kind: FlowNodeKind::Input {
                        label: "Seed".to_string(),
                        value: rocket_shared::VariableValue::simple("x"),
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "n1".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Result".to_string(),
                    },
                    position: NodePosition { x: 100.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdge {
                id: "e0".to_string(),
                source_node_id: "n0".to_string(),
                target_node_id: "n1".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
            callback_host: None,
        }
    }
```

and add:

```rust
    #[test]
    fn save_rejects_an_output_without_a_value_wire_and_names_it_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let flow = Flow {
            name: "Lonely".to_string(),
            nodes: vec![node_of(
                "out",
                FlowNodeKind::Output {
                    label: "Result".to_string(),
                },
            )],
            edges: vec![],
            callback_host: None,
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an unwired Output must be rejected")
            .to_string();
        assert!(
            message.contains("the Output node 'Result' needs exactly one value wire"),
            "got: {message}"
        );
        assert!(
            message.ends_with("node(s): out; edge(s): "),
            "got: {message}"
        );
    }
```

In `crates/rocket-app/src/flow_execution_service.rs` `mod tests`, replace the final part of `output_node_with_two_incoming_wires_fails_instead_of_dropping_one` (everything from `let summary = service` to the end of the test) with the following, and rename the test to `output_node_with_two_incoming_wires_is_refused_before_the_run`. The engine guard in the Output arm stays as defence in depth:

```rust
        let err = service
            .run(&exec, run_input("two-into-output"))
            .await
            .expect_err("a second value wire is refused before the run starts");
        assert!(
            err.to_string().contains("needs exactly one value wire"),
            "got {err}"
        );
```

In `src/lib/__tests__/flow-issues.test.ts`, replace the test `warns for an Output without a value wire, even with a Run when wire` with:

```ts
  it('errors for an Output without a value wire, even with a Run when wire', () => {
    const nodes = [input('in1'), output('out1')];
    const trigger = [wire('e1', 'in1', 'out1', 'trigger')];
    const found = only(computeFlowIssues(nodes, trigger), 'output-no-value');
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({ severity: 'error', nodeId: 'out1' });
    expect(found[0].message).toBe(
      "The Output node 'out1' needs exactly one value wire (Run when wires do not count), found 0.",
    );
    expect(
      only(computeFlowIssues(nodes, [wire('e2', 'in1', 'out1', 'value')]), 'output-no-value'),
    ).toEqual([]);
  });

  it('errors for an Output with more than one value wire', () => {
    const nodes = [input('in1'), input('in2'), output('out1')];
    const edges = [
      wire('e1', 'in1', 'out1', 'value'),
      wire('e2', 'in2', 'out1', 'value'),
      wire('e3', 'in1', 'out1', 'trigger'),
    ];
    const found = only(computeFlowIssues(nodes, edges), 'output-many-values');
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({ severity: 'error', nodeId: 'out1' });
    expect(found[0].message).toContain('found 2');
  });

  it('words the Output error like the save error, so a rejected save adds no second line', () => {
    const backend =
      "Invalid input: flow is invalid: the Output node 'out1' needs exactly one value wire (Run when wires do not count), found 0 — node(s): out1; edge(s): ";
    const issues = computeFlowIssues([output('out1')], [], {
      save: { nodeIds: ['out1'], edgeIds: [], message: backend },
    });
    expect(issues.filter((i) => i.nodeId === 'out1').map((i) => i.code)).toEqual([
      'output-no-value',
    ]);
  });
```

- [ ] **Step 2: Run the tests and state the expected failure**

Run: `cargo test -j4 -p rocket-flow v17` (`v17_output_without_a_value_wire...`, `v17_a_run_when_wire...` and `v17_two_value_wires...` FAIL with `expected InvalidNode, got Ok(...)`; the valid and ordering tests PASS).
Run: `cargo test -j4 -p rocket-flow output_label_that_imitates` and `an_output_without_a_value_wire_is_linted` (FAIL at `graph_lints`, which calls `expect_err` on a flow that is still valid).
Run: `cargo test -j4 -p rocket-app save_rejects_an_output_without` and `output_node_with_two_incoming_wires_is_refused` (FAIL: save returns Ok and the run reaches the Output step).
Run: `yarn test flow-issues --run --maxWorkers=2` (the three Output tests FAIL: severity is `warning`, there is no `output-many-values` and the message text differs).

- [ ] **Step 3: Write the implementation**

In `crates/rocket-flow/src/validate.rs`:

1. In `validate`, add after `check_auth_nodes(flow)?;`:

```rust
    check_output_wires(flow)?;
```

2. After `check_auth_nodes` add:

```rust
/// The part of a V17 reason after the label. `lint.rs` matches it to word the
/// lint without the label, so keep both in step.
pub(crate) const OUTPUT_WIRE_TAIL: &str =
    "' needs exactly one value wire (Run when wires do not count)";

/// V17: an Output node has exactly one value wire (spec section 6). A
/// `trigger` wire carries no value, so it does not count. This is the rule the
/// engine applies: it feeds an Output only its non-trigger wires. It runs
/// last, so a flow that breaks an edge rule keeps that error.
fn check_output_wires(flow: &Flow) -> Result<(), FlowGraphError> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for edge in flow.edges.iter().filter(|e| e.target_field != handle::TRIGGER) {
        *counts.entry(edge.target_node_id.as_str()).or_insert(0) += 1;
    }
    for node in &flow.nodes {
        let FlowNodeKind::Output { label } = &node.kind else {
            continue;
        };
        let found = counts.get(node.id.as_str()).copied().unwrap_or(0);
        if found != 1 {
            return Err(invalid_node(
                node,
                format!("the Output node '{label}{OUTPUT_WIRE_TAIL}, found {found}"),
            ));
        }
    }
    Ok(())
}
```

In `crates/rocket-flow/src/lint.rs`:

1. Change `use crate::validate::kind_name;` to `use crate::validate::{kind_name, OUTPUT_WIRE_TAIL};`.
2. In `sentence`, add a branch after the Request-field branch from Task 1:

```rust
    } else if let Some(count) = output_wire_count(reason) {
        Some(format!(
            "The Output node needs exactly one value wire, found {count}."
        ))
```

3. After `routed_reason` add:

```rust
/// The count in a V17 reason, when `reason` is one. The label between the
/// quotes is user text, so only the digits are passed on.
fn output_wire_count(reason: &str) -> Option<&str> {
    let rest = reason.strip_prefix("the Output node '")?;
    let (head, count) = rest.rsplit_once(", found ")?;
    let plain = !count.is_empty() && count.chars().all(|c| c.is_ascii_digit());
    (plain && head.ends_with(OUTPUT_WIRE_TAIL)).then_some(count)
}
```

In `src/lib/flow-issues.ts`:

1. Change the import `import { takesSingleInput } from '@/lib/flow-handles';` to `import { TRIGGER_HANDLE, takesSingleInput } from '@/lib/flow-handles';`.
2. Replace `outputIssues` with:

```ts
// Wires into an Output that carry a value. A "Run when" trigger wire carries none.
const valueWireCount = (edges: FlowEdge[], nodeId: string) =>
  edges.filter((e) => e.targetNodeId === nodeId && e.targetField !== TRIGGER_HANDLE).length;

// Mirrors rule V17 of rocket_flow::validate, with the same wording as its
// reason, so a rejected save does not add a second line to the node.
function outputIssues(node: FlowNode, edges: FlowEdge[]): FlowIssue[] {
  const { kind } = node;
  if (kind.kind !== 'Output') return [];
  const found = valueWireCount(edges, node.id);
  if (found === 1) return [];
  const message = `The Output node '${kind.label}' needs exactly one value wire (Run when wires do not count), found ${found}.`;
  return found === 0
    ? [
        issue(
          'output-no-value',
          'error',
          node,
          message,
          'Wire a node into the value field to show a result.',
        ),
      ]
    : [
        issue(
          'output-many-values',
          'error',
          node,
          message,
          'Remove the extra wires so only one feeds the value field.',
        ),
      ];
}
```

In `crates/rocket-flow/CLAUDE.md`, in the "Hard wire and graph rules" section, append: `` V17 (`check_output_wires`) requires exactly one non-`trigger` wire into every Output node (spec section 6). It runs last, and `OUTPUT_WIRE_TAIL` is shared with `lint.rs` so the lint sentence can drop the label. The client rule in `src/lib/flow-issues.ts` (`output-no-value`, `output-many-values`) uses the same wording. ``

- [ ] **Step 4: Run the tests and the gates**

Run: `cargo test -j4 -p rocket-flow` (all PASS).
Run: `cargo test -j4 -p rocket-app flow` and read every failure. A failing fixture is an Output with no value wire or with two: give it exactly one `value` wire (an Input node feeding it is enough). Do not loosen V17. Known cases: `sample_flow` (fixed above) and the two-wire test (rewritten above). `flow_partial_run_tests.rs`, the `linear_flow` and `output_of_input` fixtures already have one wire per Output.
Run: `cargo check -j4 -p rocket-flow -p rocket-app`.
Run: `yarn tsc --noEmit`, `yarn check`, `yarn test flow --run --maxWorkers=2` and read every failure. Tests that render an unwired Output and assert an exact issue list need the Output wired or the expected list extended by `output-no-value`. The `lists errors before warnings` test in `flow-issues.test.ts` still passes, but it no longer has a client warning to order (the backend lint feed supplies warnings), so leave it as a pin of the sort.
Run: `rustfmt --check --edition 2021 crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs`, fixing only diffs in added lines.

- [ ] **Step 5: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:
`git add crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs crates/rocket-flow/CLAUDE.md crates/rocket-app/src/flow_service.rs crates/rocket-app/src/flow_execution_service.rs src/lib/flow-issues.ts src/lib/__tests__/flow-issues.test.ts && git commit --only -m "feat(flow): require exactly one value wire on an Output" -- crates/rocket-flow/src/validate.rs crates/rocket-flow/src/lint.rs crates/rocket-flow/CLAUDE.md crates/rocket-app/src/flow_service.rs crates/rocket-app/src/flow_execution_service.rs src/lib/flow-issues.ts src/lib/__tests__/flow-issues.test.ts`
(Add any other fixture file you had to fix to both path lists.)

---

## Open decisions

1. **Zero value wires on an Output is an error (recommended: yes, as the roadmap says).** The alternative is to keep zero as a warning and reject only more than one. That would match the engine, which shows an empty value today, and would let a user save a flow with a freshly dropped Output. The cost of the recommended default is that an unwired Output blocks save and run.
2. **Duplicate edge ids as a new variant (recommended: `DuplicateEdge`).** The alternative is `InvalidEdge` with a reason. A variant keeps tests and lint mapping exact and mirrors `DuplicateNode`.
3. **Form and binary body wires into saved requests (recommended: defer to a lint).** `rocket-flow` cannot read a saved request. Add `LintContext::saved_request_body_mode` with the F-21 context work and warn there. The run-time error stays as the hard stop. Inline requests cannot have these bodies, so no hard rule is possible in this crate.
4. **Count of Output wires (recommended: every non-`trigger` wire, whatever its field name).** This equals the engine's `data_edges`. The alternative, counting only `value`, would let a wire to a stray field slip through and fail at run time.
5. **Output target field whitelist (recommended: not now).** The editor offers `value` and `trigger` only, but the engine ignores the name and old files may differ. A `value`/`trigger` whitelist can follow once V17 has shipped without complaints.
6. **Migration of existing flows (recommended: none).** No automatic rewrite or file scan. The message names the node label and field, the canvas highlights the wire or node, and the release note lists the three new rules.

## Self-review

- Spec coverage: F-26 is Task 1 (hard error, label and field in the message, fixture and agreement tests, form/binary half explained and deferred with a reason). F-27 duplicate edge ids is Task 2, F-27 Output exactly-one is Task 3 with the client rule in `flow-issues.ts`. `crates/rocket-flow/CLAUDE.md` is updated in each task.
- Placeholders: none. Every test and implementation block is real code against names read from the repo (`check_edges`, `invalid_edge_id`, `invalid_node_reason`, `graph_lints`, `keys`, `CANARY`, `assert_no_canary`, `node_of`, `PanicsOnSaveRepo`, `sample_execute_input`, `edge`, `apply_wired_overrides`, `issue`, `only`, `wire`).
- Name consistency: `is_request_wire_field`, `REQUEST_FIELDS`, `OUTPUT_WIRE_TAIL`, `check_output_wires`, `FlowGraphError::DuplicateEdge { edge_id }`, `output-no-value` and `output-many-values` are spelled the same in every task. Reason texts in Rust tests, lint sentences and the TypeScript message are identical where the UI compares them.
- Known soft spots to confirm at run time: other `rocket-app` fixtures that put a Request on an unusual field or leave an Output unwired, and TypeScript tests that render unwired Outputs. Steps 4 of Tasks 1 and 3 tell the implementer how to fix those without loosening a rule.
- Review Focus: each line maps to a named test above.

## Next plan to execute

`04-all-validation-errors.md`
