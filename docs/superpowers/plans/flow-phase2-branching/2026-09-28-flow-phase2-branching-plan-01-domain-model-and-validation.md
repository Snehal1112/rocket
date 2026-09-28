# Flow Phase 2 — Plan 01: Domain Model and Validation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add If/Switch routing node kinds, named edge exits (`source_handle`), and save-time structural validation (V1–V8) to the Flow domain, while keeping every Phase 1 flow file loading and re-saving byte-identically and the whole workspace compiling.

**Architecture:** All model and validation logic lives in the pure `rocket-flow` crate: a new `handle` module holds exit and input names, `node.rs` gains the two variants plus `SwitchCase`, `flow.rs` gains `FlowEdge.source_handle` with serde default/omission, and a new `validate.rs` wraps `topological_sort` with the structural rules. The IPC DTOs in `src-tauri` mirror the new shapes. `FlowService::save` switches to `validate`. The executor gets a temporary fail-fast arm for the new kinds; plan 03 replaces it with real routing.

**Tech Stack:** Rust (serde, serde_yaml, thiserror), Tauri 2 IPC DTOs, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-28-flow-phase2-branching-design.md` (§5 Data model, §7 Validation, §8.3 DTOs). Plan index and cross-plan contract: `docs/superpowers/plans/flow-phase2-branching/00-plan-index.md`.

## Global Constraints

- `rocket-flow` stays a pure domain crate: dependencies are `rocket-shared`, `serde` and `thiserror` only (dev-dependencies may add `serde_json`, `serde_yaml`). No I/O, no expression evaluation.
- Persistence structs (`FlowEdge`, `FlowNodeKind`, `SwitchCase`) have **no** `rename_all`: snake_case on disk. IPC DTOs use `#[serde(rename_all = "camelCase")]` (structs) or `rename_all_fields = "camelCase"` (tagged enums).
- `FlowEdge.source_handle` uses `#[serde(default = "default_source_handle", skip_serializing_if = "is_result_handle")]`. The default is `"result"`, and it is omitted on serialize when `"result"`.
- Handle names are exactly: `result`, `true`, `false`, `default`, `input`, `trigger`, and the `case:` prefix.
- No panicking unwraps in production code. Tests use `.expect("…")`.
- Always pass `-j4` to cargo.
- Commits: conventional-commit subjects, created through the `dev-workflow-skills:1-git-commit` skill.
- Code comments: short full sentences ending with a punctuation mark.

## Review Focus

1. **A hand-edited flow file with an unknown or empty `source_handle` on a Request edge.** A user editing YAML by hand expects a clear save/run rejection naming the edge, not a silent "never live" edge. Pinned in Task 3 (`v5_request_edge_with_unknown_exit_is_rejected`, `v5_empty_case_handle_is_rejected`).
2. **An old frontend payload without `sourceHandle`.** Plans 01–03 land before the frontend sends the field, so saving from the current UI must default every edge to `result`. Pinned in Task 1 (`flow_edge_dto_without_source_handle_defaults_to_result`).
3. **A Switch with zero cases (only `default`).** A user building a Switch step by step expects it to be valid and to route everything to `default`. The spec does not forbid it. Pinned in Task 3 (`switch_with_no_cases_is_valid`).
4. **A whitespace-only If condition or Switch value.** This is always a mistake and must be rejected, not saved and then failing on every run. Pinned in Task 3 (`v8_whitespace_only_condition_is_rejected`).
5. **A Phase 1 Output edge saved with `target_field: value` and a Phase 1 file with no `source_handle` keys.** The file must still validate, load and re-save byte-identically. Pinned in Task 1 (`phase1_yaml_reserializes_byte_identically`), Task 2 (`fs_repo_resaves_phase1_file_byte_identically`) and Task 3 (`phase1_linear_flow_still_validates`).

---

### Task 1: Handle vocabulary and `FlowEdge.source_handle`

**Files:**
- Create: `crates/rocket-flow/src/handle.rs`
- Modify: `crates/rocket-flow/src/lib.rs`
- Modify: `crates/rocket-flow/src/flow.rs` (the `FlowEdge` struct plus its test at line ~75)
- Modify: `crates/rocket-flow/Cargo.toml` (add `serde_yaml` to dev-dependencies)
- Modify: `crates/rocket-flow/src/graph.rs` (test fixtures `edge()` ~line 221 and `edge_fixture()` ~line 443)
- Modify: `crates/rocket-app/src/flow_service.rs` (test `cyclic_flow()` edge literals ~lines 174 and 181)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (test edge literals at ~lines 1189, 1478, 1508, 1515, 1812, 1873, 2289, 2293)
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs` (test edge literal ~line 290)
- Modify: `src-tauri/src/commands/flow.rs` (`FlowEdgeDto` + both `From` impls, test `sample_dto()` ~line 341)
- Modify: `crates/rocket-flow/CLAUDE.md` (module map)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `rocket_flow::handle::{RESULT, TRUE, FALSE, DEFAULT, INPUT, TRIGGER, CASE_PREFIX}: &str`
  - `rocket_flow::handle::case_handle(case_id: &str) -> String`
  - `rocket_flow::handle::case_id_from_handle(handle: &str) -> Option<&str>`
  - `FlowEdge { …existing…, pub source_handle: String }`
  - `FlowEdgeDto { …existing…, pub source_handle: String }`, serialized as `sourceHandle` and defaulting to `"result"` when absent.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Flow files are a Rocket-only extension under `<collection>/flows/`, and nothing in this task may touch `opencollection.yml`.

- [ ] **Step 2: Write the failing handle tests**

Create `crates/rocket-flow/src/handle.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_handle_prefixes_the_case_id() {
        assert_eq!(case_handle("01J9CASE"), "case:01J9CASE");
    }

    #[test]
    fn case_id_from_handle_strips_the_prefix() {
        assert_eq!(case_id_from_handle("case:01J9CASE"), Some("01J9CASE"));
    }

    #[test]
    fn case_id_from_handle_rejects_non_case_handles() {
        assert_eq!(case_id_from_handle(RESULT), None);
        assert_eq!(case_id_from_handle(DEFAULT), None);
        assert_eq!(case_id_from_handle("cases:x"), None);
    }

    #[test]
    fn case_id_from_handle_rejects_an_empty_case_id() {
        assert_eq!(case_id_from_handle("case:"), None);
    }

    #[test]
    fn handle_names_match_the_spec() {
        assert_eq!(
            [RESULT, TRUE, FALSE, DEFAULT, INPUT, TRIGGER, CASE_PREFIX],
            ["result", "true", "false", "default", "input", "trigger", "case:"]
        );
    }
}
```

Add `pub mod handle;` to `crates/rocket-flow/src/lib.rs` (above `pub mod node;`).

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-flow -j4 handle::`
Expected: FAIL to compile with `cannot find function 'case_handle'` and `cannot find value 'RESULT'`.

- [ ] **Step 4: Implement the handle module**

Prepend to `crates/rocket-flow/src/handle.rs`, above the test module:

```rust
//! Names of node exits (`FlowEdge::source_handle`) and data-less inputs
//! (`FlowEdge::target_field`). The validator, the executor and the tests all
//! use these constants, so there is one spelling for each name.

/// The single exit of Request and Input nodes. It is the default for every edge.
pub const RESULT: &str = "result";
/// The exit an If node takes when its condition is truthy.
pub const TRUE: &str = "true";
/// The exit an If node takes when its condition is falsy.
pub const FALSE: &str = "false";
/// The exit a Switch node takes when no case matches.
pub const DEFAULT: &str = "default";
/// The single data input of If and Switch nodes.
pub const INPUT: &str = "input";
/// The data-less "Run when" input of Request and Output nodes.
pub const TRIGGER: &str = "trigger";
/// Prefix of a Switch case exit. The rest of the handle is the case id.
pub const CASE_PREFIX: &str = "case:";

/// Builds the exit handle for the Switch case with id `case_id`.
pub fn case_handle(case_id: &str) -> String {
    format!("{CASE_PREFIX}{case_id}")
}

/// Returns the case id inside a `case:<id>` handle. Any other handle, and a
/// handle with an empty id, returns `None`.
pub fn case_id_from_handle(handle: &str) -> Option<&str> {
    handle
        .strip_prefix(CASE_PREFIX)
        .filter(|case_id| !case_id.is_empty())
}
```

- [ ] **Step 5: Run the handle tests to verify they pass**

Run: `cargo test -p rocket-flow -j4 handle::`
Expected: PASS, 5 tests.

- [ ] **Step 6: Write the failing `source_handle` serde tests**

Add `serde_yaml.workspace = true` under `[dev-dependencies]` in `crates/rocket-flow/Cargo.toml`.

Append these tests to the `tests` module in `crates/rocket-flow/src/flow.rs`:

```rust
    fn plain_edge_json() -> &'static str {
        r#"{"id":"e1","source_node_id":"a","target_node_id":"b","target_field":"url","expression":"response.body"}"#
    }

    #[test]
    fn edge_without_source_handle_defaults_to_result() {
        let edge: FlowEdge = serde_json::from_str(plain_edge_json()).expect("deserialize edge");
        assert_eq!(edge.source_handle, crate::handle::RESULT);
    }

    #[test]
    fn result_source_handle_is_omitted_on_serialize() {
        let edge: FlowEdge = serde_json::from_str(plain_edge_json()).expect("deserialize edge");
        let json = serde_json::to_string(&edge).expect("serialize edge");
        assert!(!json.contains("source_handle"), "got: {json}");
    }

    #[test]
    fn non_result_source_handle_is_serialized() {
        let mut edge: FlowEdge =
            serde_json::from_str(plain_edge_json()).expect("deserialize edge");
        edge.source_handle = crate::handle::TRUE.to_string();
        let json = serde_json::to_string(&edge).expect("serialize edge");
        assert!(json.contains(r#""source_handle":"true""#), "got: {json}");
        let back: FlowEdge = serde_json::from_str(&json).expect("deserialize edge");
        assert_eq!(back, edge);
    }

    /// Mirrors the Phase 1 on-disk edge shape, which had no `source_handle`.
    #[derive(Serialize)]
    struct Phase1Edge {
        id: String,
        source_node_id: String,
        target_node_id: String,
        target_field: String,
        expression: String,
    }

    /// Mirrors the Phase 1 on-disk flow shape.
    #[derive(Serialize)]
    struct Phase1Flow {
        name: String,
        nodes: Vec<FlowNode>,
        edges: Vec<Phase1Edge>,
    }

    #[test]
    fn phase1_yaml_reserializes_byte_identically() {
        let phase1 = Phase1Flow {
            name: "Login then fetch profile".to_string(),
            nodes: sample_flow().nodes,
            edges: vec![Phase1Edge {
                id: "edge-1".to_string(),
                source_node_id: "node-1".to_string(),
                target_node_id: "node-2".to_string(),
                target_field: "value".to_string(),
                expression: "response.body.token".to_string(),
            }],
        };
        let phase1_yaml = serde_yaml::to_string(&phase1).expect("serialize Phase 1 flow");

        let loaded: Flow = serde_yaml::from_str(&phase1_yaml).expect("load Phase 1 flow");
        assert_eq!(loaded.edges[0].source_handle, crate::handle::RESULT);

        let resaved = serde_yaml::to_string(&loaded).expect("re-serialize flow");
        assert_eq!(resaved, phase1_yaml, "a Phase 1 file must re-save without any diff");
    }
```

Add `use serde::Serialize;` to the top of the `tests` module in `flow.rs` if it isn't already reachable through `use super::*;` (it is, because `flow.rs` imports `serde::{Deserialize, Serialize}`, so no change is needed).

- [ ] **Step 7: Run the serde tests to verify they fail**

Run: `cargo test -p rocket-flow -j4 flow::tests`
Expected: FAIL to compile with `no field 'source_handle' on type 'FlowEdge'`.

- [ ] **Step 8: Add `source_handle` to `FlowEdge`**

In `crates/rocket-flow/src/flow.rs`, append the field to `FlowEdge` (after `expression`) and add the two serde helper functions below the struct:

```rust
    /// Which exit of the source node this edge leaves from, e.g. "result",
    /// "true" or "case:<id>". Phase 1 files have no such key, so it defaults
    /// to "result", and it is left out when "result" so those files re-save
    /// without a diff.
    #[serde(
        default = "default_source_handle",
        skip_serializing_if = "is_result_handle"
    )]
    pub source_handle: String,
}

fn default_source_handle() -> String {
    crate::handle::RESULT.to_string()
}

fn is_result_handle(handle: &str) -> bool {
    handle == crate::handle::RESULT
}
```

Update the existing `sample_flow()` edge literal in the same test module by adding `source_handle: crate::handle::RESULT.to_string(),` after `expression`.

- [ ] **Step 9: Run the rocket-flow tests**

Run: `cargo test -p rocket-flow -j4`
Expected: FAIL to compile in `graph.rs` tests with `missing field 'source_handle' in initializer of 'FlowEdge'`.

- [ ] **Step 10: Fix the rocket-flow edge fixtures**

In `crates/rocket-flow/src/graph.rs`, add `source_handle: crate::handle::RESULT.to_string(),` as the last field of the `FlowEdge` literal inside both `fn edge(...)` (~line 221) and `fn edge_fixture(...)` (~line 443).

Run: `cargo test -p rocket-flow -j4`
Expected: PASS (all existing tests plus the 5 handle tests and 4 new serde tests).

- [ ] **Step 11: Write the failing DTO test**

In `src-tauri/src/commands/flow.rs`, append to the `tests` module:

```rust
    #[test]
    fn flow_edge_dto_without_source_handle_defaults_to_result() {
        let json = r#"{"id":"e1","sourceNodeId":"a","targetNodeId":"b","targetField":"url","expression":"response.body"}"#;
        let dto: FlowEdgeDto = serde_json::from_str(json).expect("deserialize FlowEdgeDto");
        assert_eq!(dto.source_handle, "result");
        let domain: FlowEdge = dto.into();
        assert_eq!(domain.source_handle, rocket_flow::handle::RESULT);
    }

    #[test]
    fn flow_edge_dto_serializes_source_handle_as_camel_case() {
        let mut dto = sample_dto();
        dto.edges[0].source_handle = "true".to_string();
        let json = serde_json::to_string(&dto).expect("serialize FlowDto");
        assert!(json.contains(r#""sourceHandle":"true""#), "got: {json}");
        let domain: Flow = dto.into();
        assert_eq!(domain.edges[0].source_handle, "true");
    }
```

- [ ] **Step 12: Run the DTO tests to verify they fail**

Run: `cargo test -p rocket -j4 commands::flow::tests::flow_edge_dto`
Expected: FAIL to compile with `no field 'source_handle' on type 'FlowEdgeDto'` (and `missing field 'source_handle'` for `FlowEdge` in the `From` impl).

- [ ] **Step 13: Mirror `source_handle` in the DTO and fix every remaining edge literal**

In `src-tauri/src/commands/flow.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEdgeDto {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub target_field: String,
    pub expression: String,
    /// Absent in payloads from a frontend that predates routing nodes.
    #[serde(default = "default_source_handle")]
    pub source_handle: String,
}

fn default_source_handle() -> String {
    rocket_flow::handle::RESULT.to_string()
}
```

Add `source_handle: e.source_handle,` to both `From<FlowEdge> for FlowEdgeDto` and `From<FlowEdgeDto> for FlowEdge`. Add `source_handle: "result".to_string(),` to the `FlowEdgeDto` literal in `sample_dto()`.

Add `source_handle: rocket_flow::handle::RESULT.to_string(),` to every remaining `FlowEdge { … }` literal that lists all fields:
- `crates/rocket-app/src/flow_service.rs`: both edges in `cyclic_flow()`.
- `crates/rocket-app/src/flow_execution_service.rs`: the literals at ~lines 1189 (`fn edge`), 1478, 1508, 1515, 1812, 1873 (`fn wire`), 2289 and 2293. Literals that use struct-update syntax (`..wire(...)`) need no change.
- `crates/rocket-infra/src/fs_flow_repo.rs`: the literal at ~line 290.

Then confirm nothing was missed:

Run: `cargo check -j4 --workspace --tests`
Expected: `Finished` with no `missing field 'source_handle'` errors.

- [ ] **Step 14: Run the affected test suites**

Run: `cargo test -j4 -p rocket-flow -p rocket-app -p rocket-infra -p rocket flow`
Expected: PASS. The two new DTO tests pass, and every pre-existing flow test still passes.

- [ ] **Step 15: Update the crate guide**

In `crates/rocket-flow/CLAUDE.md`, add this row to the Module Map table above `node.rs`:

```markdown
| `handle.rs` | Exit/input handle names (`result`, `true`, `false`, `default`, `input`, `trigger`, `case:<id>`) |
```

Also add this bullet under Key Design Points:

```markdown
- `FlowEdge.source_handle` names the exit an edge leaves from. It defaults
  to "result" and is omitted on disk when "result", so Phase 1 files
  re-save byte-identically.
```

- [ ] **Step 16: Commit**

Stage the files listed above and commit using the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add named edge exits via source_handle`.

---

### Task 2: If/Switch node kinds, DTO mirror and temporary executor arm

**Files:**
- Modify: `crates/rocket-flow/src/node.rs`
- Modify: `crates/rocket-flow/src/lib.rs`
- Modify: `src-tauri/src/commands/flow.rs` (`FlowNodeKindDto`, new `SwitchCaseDto`, both `From` impls, tests)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`execute_node` match ~line 541, one new test)
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs` (two new tests)
- Modify: `crates/rocket-flow/CLAUDE.md`

**Interfaces:**
- Consumes: `rocket_flow::handle::*` (Task 1), `FlowEdge.source_handle` (Task 1).
- Produces:
  - `FlowNodeKind::If { label: String, condition: String }`
  - `FlowNodeKind::Switch { label: String, value: String, cases: Vec<SwitchCase> }`
  - `pub struct SwitchCase { pub id: String, pub label: String, pub matches: String }`, re-exported as `rocket_flow::SwitchCase`.
  - `FlowNodeKindDto::If { label, condition }`, `FlowNodeKindDto::Switch { label, value, cases: Vec<SwitchCaseDto> }`
  - `pub struct SwitchCaseDto { pub id: String, pub label: String, pub matches: String }` (camelCase)
  - A temporary `execute_node` arm that fails If/Switch nodes with `DomainError::InvalidInput("If/Switch nodes are not executable yet")`. **Plan 03 removes this arm.**

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. The new node kinds are Flow-only and must never appear in `opencollection.yml`.

- [ ] **Step 2: Write the failing node serde tests**

Append to the `tests` module in `crates/rocket-flow/src/node.rs`:

```rust
    #[test]
    fn flow_node_kind_if_tagged_roundtrip() {
        let kind = FlowNodeKind::If {
            label: "Logged in?".to_string(),
            condition: "response.status === 200".to_string(),
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"If\""), "got: {json}");
        assert!(json.contains("\"condition\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn flow_node_kind_switch_tagged_roundtrip_keeps_case_order() {
        let kind = FlowNodeKind::Switch {
            label: "Plan router".to_string(),
            value: "response.body.plan".to_string(),
            cases: vec![
                SwitchCase {
                    id: "c1".to_string(),
                    label: "Free".to_string(),
                    matches: "free".to_string(),
                },
                SwitchCase {
                    id: "c2".to_string(),
                    label: "Pro plan".to_string(),
                    matches: "pro".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Switch\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn switch_case_fields_are_snake_case_on_disk() {
        let case = SwitchCase {
            id: "c1".to_string(),
            label: "Free".to_string(),
            matches: "free".to_string(),
        };
        let json = serde_json::to_string(&case).expect("serialize SwitchCase");
        assert_eq!(json, r#"{"id":"c1","label":"Free","matches":"free"}"#);
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-flow -j4 node::tests`
Expected: FAIL to compile with `no variant named 'If'` and `cannot find struct 'SwitchCase'`.

- [ ] **Step 4: Add the variants and `SwitchCase`**

In `crates/rocket-flow/src/node.rs`, extend `FlowNodeKind` after `Output` and add `SwitchCase` below the enum:

```rust
    /// Routes to the "true" exit when `!!(condition)` is truthy, otherwise to
    /// "false". Its output is its input, passed through unchanged.
    If {
        label: String,
        condition: String,
    },
    /// Routes to the first case whose `matches` equals `String(value)`,
    /// otherwise to "default". Its output is its input, passed through.
    Switch {
        label: String,
        value: String,
        cases: Vec<SwitchCase>,
    },
}

/// One named case of a `Switch` node. Edges address a case by `id`, so
/// renaming its `label` never breaks a wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchCase {
    pub id: String,
    pub label: String,
    pub matches: String,
}
```

Update the doc comment on `FlowNodeKind` to mention the routing nodes, e.g. append: "`If` and `Switch` nodes route execution to one of their named exits."

In `crates/rocket-flow/src/lib.rs`, change the node re-export to:

```rust
pub use node::{
    FlowNodeKind, InlineHeader, InlineRequestData, NodePosition, RequestSource, SwitchCase,
};
```

- [ ] **Step 5: Run the rocket-flow tests**

Run: `cargo test -p rocket-flow -j4`
Expected: PASS.

- [ ] **Step 6: Confirm which downstream matches break**

Run: `cargo check -j4 --workspace --tests`
Expected: FAIL with `non-exhaustive patterns: 'FlowNodeKind::If { .. }' and 'FlowNodeKind::Switch { .. }' not covered` in `src-tauri/src/commands/flow.rs` (`From<FlowNodeKind> for FlowNodeKindDto`) and in `crates/rocket-app/src/flow_execution_service.rs` (`execute_node`).

- [ ] **Step 7: Write the failing DTO tests**

Append to the `tests` module in `src-tauri/src/commands/flow.rs`:

```rust
    fn routing_dto() -> FlowDto {
        FlowDto {
            name: "Routing".to_string(),
            nodes: vec![
                FlowNodeDto {
                    id: "if1".to_string(),
                    kind: FlowNodeKindDto::If {
                        label: "Logged in?".to_string(),
                        condition: "response.status === 200".to_string(),
                    },
                    position: NodePositionDto { x: 0.0, y: 0.0 },
                },
                FlowNodeDto {
                    id: "sw1".to_string(),
                    kind: FlowNodeKindDto::Switch {
                        label: "Plan router".to_string(),
                        value: "response.body.plan".to_string(),
                        cases: vec![SwitchCaseDto {
                            id: "c1".to_string(),
                            label: "Pro plan".to_string(),
                            matches: "pro".to_string(),
                        }],
                    },
                    position: NodePositionDto { x: 200.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdgeDto {
                id: "e1".to_string(),
                source_node_id: "if1".to_string(),
                target_node_id: "sw1".to_string(),
                target_field: "input".to_string(),
                expression: String::new(),
                source_handle: "true".to_string(),
            }],
        }
    }

    #[test]
    fn routing_node_dtos_keep_tag_values_and_camel_case_fields() {
        let json = serde_json::to_string(&routing_dto()).expect("serialize FlowDto");
        assert!(json.contains(r#""kind":"If""#), "got: {json}");
        assert!(json.contains(r#""kind":"Switch""#), "got: {json}");
        assert!(json.contains(r#""condition":"response.status === 200""#), "got: {json}");
        assert!(json.contains(r#""cases":[{"id":"c1","label":"Pro plan","matches":"pro"}]"#), "got: {json}");
    }

    #[test]
    fn routing_node_dtos_roundtrip_through_domain_type() {
        let dto = routing_dto();
        let domain: Flow = dto.clone().into();
        assert!(matches!(domain.nodes[0].kind, FlowNodeKind::If { .. }));
        match &domain.nodes[1].kind {
            FlowNodeKind::Switch { cases, .. } => assert_eq!(cases[0].matches, "pro"),
            other => panic!("expected a Switch node, got {other:?}"),
        }
        let back: FlowDto = domain.into();
        let a = serde_json::to_value(&dto).expect("to_value dto");
        let b = serde_json::to_value(&back).expect("to_value back");
        assert_eq!(a, b);
    }
```

- [ ] **Step 8: Mirror the new kinds in the DTOs**

In `src-tauri/src/commands/flow.rs`, add `SwitchCase` to the `rocket_flow` import list, then:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchCaseDto {
    pub id: String,
    pub label: String,
    pub matches: String,
}
impl From<SwitchCase> for SwitchCaseDto {
    fn from(c: SwitchCase) -> Self {
        Self {
            id: c.id,
            label: c.label,
            matches: c.matches,
        }
    }
}
impl From<SwitchCaseDto> for SwitchCase {
    fn from(c: SwitchCaseDto) -> Self {
        Self {
            id: c.id,
            label: c.label,
            matches: c.matches,
        }
    }
}
```

Add to `FlowNodeKindDto` (after `Output`):

```rust
    If {
        label: String,
        condition: String,
    },
    Switch {
        label: String,
        value: String,
        cases: Vec<SwitchCaseDto>,
    },
```

Add to `From<FlowNodeKind> for FlowNodeKindDto`:

```rust
            FlowNodeKind::If { label, condition } => FlowNodeKindDto::If { label, condition },
            FlowNodeKind::Switch {
                label,
                value,
                cases,
            } => FlowNodeKindDto::Switch {
                label,
                value,
                cases: cases.into_iter().map(Into::into).collect(),
            },
```

Add to `From<FlowNodeKindDto> for FlowNodeKind`:

```rust
            FlowNodeKindDto::If { label, condition } => FlowNodeKind::If { label, condition },
            FlowNodeKindDto::Switch {
                label,
                value,
                cases,
            } => FlowNodeKind::Switch {
                label,
                value,
                cases: cases.into_iter().map(Into::into).collect(),
            },
```

- [ ] **Step 9: Write the failing executor placeholder test**

In `crates/rocket-app/src/flow_execution_service.rs`, add this test to the `tests` module directly after `output_node_with_two_incoming_wires_fails_instead_of_dropping_one` (it uses `service_with_flow`, `recording_exec`, `fixed_wire`, `run_input` and `status_of`, which are already defined there):

```rust
    /// Plan 03 replaces this placeholder with real routing and deletes this test.
    #[tokio::test]
    async fn routing_nodes_fail_until_routing_execution_exists() {
        let flow = Flow {
            name: "routing-placeholder".to_string(),
            nodes: vec![FlowNode {
                id: "if1".to_string(),
                kind: FlowNodeKind::If {
                    label: "Logged in?".to_string(),
                    condition: "true".to_string(),
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: Vec::new(),
        };
        let service = service_with_flow(flow);
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("1"));

        let summary = service
            .run(&exec, run_input("routing-placeholder"))
            .await
            .expect("run");

        assert_eq!(status_of(&summary, "if1"), FlowNodeStatus::Failed);
        let error = summary.steps[0].error.as_deref().unwrap_or("");
        assert!(error.contains("not executable yet"), "got: {error}");
    }
```

- [ ] **Step 10: Add the temporary executor arm**

In `execute_node` in `crates/rocket-app/src/flow_execution_service.rs`, add this arm to the `match &node.kind` (after the `Request` arm):

```rust
            // Temporary until plan 03 implements routing execution.
            rocket_flow::FlowNodeKind::If { .. } | rocket_flow::FlowNodeKind::Switch { .. } => {
                Err(DomainError::InvalidInput(format!(
                    "node '{}': If/Switch nodes are not executable yet",
                    node.id
                )))
            }
```

- [ ] **Step 11: Write the infra round-trip tests**

Append to the `tests` module in `crates/rocket-infra/src/fs_flow_repo.rs` (add `SwitchCase` to the `rocket_flow` import in that module):

```rust
    #[test]
    fn if_and_switch_nodes_with_routed_edges_roundtrip() {
        let (_dir, repo) = setup();
        let mut flow = sample("Routing Flow");
        flow.nodes.push(FlowNode {
            id: "if1".to_string(),
            kind: FlowNodeKind::If {
                label: "Logged in?".to_string(),
                condition: "response.status === 200".to_string(),
            },
            position: NodePosition { x: 200.0, y: 0.0 },
        });
        flow.nodes.push(FlowNode {
            id: "sw1".to_string(),
            kind: FlowNodeKind::Switch {
                label: "Plan router".to_string(),
                value: "response.body.plan".to_string(),
                cases: vec![SwitchCase {
                    id: "c1".to_string(),
                    label: "Pro plan".to_string(),
                    matches: "pro".to_string(),
                }],
            },
            position: NodePosition { x: 400.0, y: 0.0 },
        });
        flow.edges.push(FlowEdge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "if1".to_string(),
            target_field: rocket_flow::handle::INPUT.to_string(),
            expression: String::new(),
            source_handle: rocket_flow::handle::RESULT.to_string(),
        });
        flow.edges.push(FlowEdge {
            id: "e2".to_string(),
            source_node_id: "if1".to_string(),
            target_node_id: "sw1".to_string(),
            target_field: rocket_flow::handle::INPUT.to_string(),
            expression: String::new(),
            source_handle: rocket_flow::handle::TRUE.to_string(),
        });
        repo.save("acme", &flow).expect("save");
        assert_eq!(repo.get("acme", "Routing Flow").expect("get"), flow);
    }

    /// Mirrors the Phase 1 on-disk edge shape, which had no `source_handle`.
    #[derive(serde::Serialize)]
    struct Phase1Edge {
        id: String,
        source_node_id: String,
        target_node_id: String,
        target_field: String,
        expression: String,
    }

    /// Mirrors the Phase 1 on-disk flow shape.
    #[derive(serde::Serialize)]
    struct Phase1Flow {
        name: String,
        nodes: Vec<FlowNode>,
        edges: Vec<Phase1Edge>,
    }

    #[test]
    fn fs_repo_resaves_phase1_file_byte_identically() {
        let (dir, repo) = setup();
        let mut nodes = sample("Phase One").nodes;
        nodes.push(FlowNode {
            id: "node-2".to_string(),
            kind: FlowNodeKind::Output {
                label: "Result".to_string(),
            },
            position: NodePosition { x: 400.0, y: 0.0 },
        });
        let phase1 = Phase1Flow {
            name: "Phase One".to_string(),
            nodes,
            edges: vec![Phase1Edge {
                id: "edge-1".to_string(),
                source_node_id: "node-1".to_string(),
                target_node_id: "node-2".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
            }],
        };
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        let path = flows_dir.join("phase-one.yml");
        let original = serde_yaml::to_string(&phase1).expect("serialize Phase 1 flow");
        fs::write(&path, &original).expect("write Phase 1 file");

        let loaded = repo.get("acme", "Phase One").expect("load Phase 1 file");
        repo.save("acme", &loaded).expect("re-save");

        let resaved = fs::read_to_string(&path).expect("read re-saved file");
        assert_eq!(resaved, original, "re-saving a Phase 1 file must not change it");
    }
```

- [ ] **Step 12: Run everything touched by this task**

Run: `cargo test -j4 -p rocket-flow -p rocket-app -p rocket-infra -p rocket flow`
Expected: PASS, including `routing_node_dtos_*`, `routing_nodes_fail_until_routing_execution_exists`, `if_and_switch_nodes_with_routed_edges_roundtrip` and `fs_repo_resaves_phase1_file_byte_identically`.

- [ ] **Step 13: Update the crate guide**

In `crates/rocket-flow/CLAUDE.md`, change the `node.rs` row of the Module Map to:

```markdown
| `node.rs` | `FlowNodeKind` (Request/Input/Output/If/Switch), `SwitchCase`, `RequestSource`, `InlineRequestData`, `InlineHeader`, `NodePosition` |
```

- [ ] **Step 14: Commit**

Commit using the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): add If and Switch routing node kinds`.

---

### Task 3: `validate()` with structural rules V1–V8

**Files:**
- Create: `crates/rocket-flow/src/validate.rs`
- Modify: `crates/rocket-flow/src/graph.rs` (`FlowGraphError` gains two variants)
- Modify: `crates/rocket-flow/src/lib.rs`
- Modify: `crates/rocket-flow/CLAUDE.md`

**Interfaces:**
- Consumes: `topological_sort` and `FlowGraphError` (existing), `rocket_flow::handle::*` (Task 1), `FlowNodeKind::If`/`Switch`/`SwitchCase` (Task 2).
- Produces:
  - `pub fn validate(flow: &Flow) -> Result<Vec<String>, FlowGraphError>`, re-exported as `rocket_flow::validate`. It returns the same topological order as `topological_sort`.
  - `FlowGraphError::InvalidNode { node_id: String, reason: String }` with Display `invalid node {node_id}: {reason}`.
  - `FlowGraphError::InvalidEdge { edge_id: String, reason: String }` with Display `invalid edge {edge_id}: {reason}`.

Rule order (the first violation found is reported): the existing `topological_sort` checks, then V1 over nodes in file order, V2, V3 and V4 each over edges in file order, V5 over edges in file order, V6, V7 and V8 each over nodes in file order.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. Validation applies only to Flow files and must not reject anything OpenCollection allows.

- [ ] **Step 2: Write the failing validation tests**

Create `crates/rocket-flow/src/validate.rs` with only this test module, and add `pub mod validate;` plus `pub use validate::validate;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowEdge, FlowNode};
    use crate::handle;
    use crate::node::{FlowNodeKind, NodePosition, RequestSource, SwitchCase};

    fn node(id: &str, kind: FlowNodeKind) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn request(id: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::Request {
                label: id.to_string(),
                source: RequestSource::Saved {
                    request_path: format!("{id}.yml"),
                },
            },
        )
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

    fn if_node(id: &str, condition: &str) -> FlowNode {
        node(
            id,
            FlowNodeKind::If {
                label: id.to_string(),
                condition: condition.to_string(),
            },
        )
    }

    fn switch_node(id: &str, cases: Vec<(&str, &str)>) -> FlowNode {
        node(
            id,
            FlowNodeKind::Switch {
                label: id.to_string(),
                value: "response.body.plan".to_string(),
                cases: cases
                    .into_iter()
                    .map(|(case_id, matches)| SwitchCase {
                        id: case_id.to_string(),
                        label: case_id.to_string(),
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
        }
    }

    fn invalid_node_id(result: Result<Vec<String>, FlowGraphError>) -> String {
        match result {
            Err(FlowGraphError::InvalidNode { node_id, .. }) => node_id,
            other => panic!("expected InvalidNode, got {other:?}"),
        }
    }

    fn invalid_edge_id(result: Result<Vec<String>, FlowGraphError>) -> String {
        match result {
            Err(FlowGraphError::InvalidEdge { edge_id, .. }) => edge_id,
            other => panic!("expected InvalidEdge, got {other:?}"),
        }
    }

    /// login -> if1 (input); if1.true -> profile (trigger); if1.false -> refresh (trigger).
    fn valid_if_flow() -> Flow {
        flow(
            vec![request("login"), if_node("if1", "response.status === 200"), request("profile"), request("refresh")],
            vec![
                edge("e1", "login", handle::RESULT, "if1", handle::INPUT),
                edge("e2", "if1", handle::TRUE, "profile", handle::TRIGGER),
                edge("e3", "if1", handle::FALSE, "refresh", handle::TRIGGER),
            ],
        )
    }

    #[test]
    fn valid_if_flow_passes_and_returns_topological_order() {
        let order = validate(&valid_if_flow()).expect("valid flow");
        assert_eq!(order[0], "login");
        assert_eq!(order[1], "if1");
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn phase1_linear_flow_still_validates() {
        let f = flow(
            vec![input("in"), request("req"), output("out")],
            vec![
                edge("e1", "in", handle::RESULT, "req", "url"),
                edge("e2", "req", handle::RESULT, "out", "value"),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn cycles_are_still_reported_as_cycles() {
        let f = flow(
            vec![output("a"), output("b")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "value"),
                edge("e2", "b", handle::RESULT, "a", "value"),
            ],
        );
        assert!(matches!(validate(&f), Err(FlowGraphError::Cycle { .. })));
    }

    #[test]
    fn v1_if_without_input_is_rejected() {
        let f = flow(vec![if_node("if1", "true")], vec![]);
        assert_eq!(invalid_node_id(validate(&f)), "if1");
    }

    #[test]
    fn v1_switch_with_two_inputs_is_rejected() {
        let f = flow(
            vec![request("a"), request("b"), switch_node("sw1", vec![("c1", "x")])],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "b", handle::RESULT, "sw1", handle::INPUT),
            ],
        );
        assert_eq!(invalid_node_id(validate(&f)), "sw1");
    }

    #[test]
    fn v1_routing_input_must_target_the_input_field() {
        let f = flow(
            vec![request("a"), if_node("if1", "true")],
            vec![edge("e1", "a", handle::RESULT, "if1", "url")],
        );
        assert_eq!(invalid_node_id(validate(&f)), "if1");
    }

    #[test]
    fn v2_input_field_on_a_request_node_is_rejected() {
        let f = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", handle::RESULT, "b", handle::INPUT)],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn v3_trigger_into_an_input_node_is_rejected() {
        let f = flow(
            vec![request("a"), input("in")],
            vec![edge("e1", "a", handle::RESULT, "in", handle::TRIGGER)],
        );
        match validate(&f) {
            Err(FlowGraphError::InvalidEdge { edge_id, reason }) => {
                assert_eq!(edge_id, "e1");
                assert!(reason.contains("trigger"), "V3 must fire before V4, got: {reason}");
            }
            other => panic!("expected InvalidEdge, got {other:?}"),
        }
    }

    #[test]
    fn v3_trigger_into_request_and_output_is_valid() {
        let f = flow(
            vec![request("a"), request("b"), output("out")],
            vec![
                edge("e1", "a", handle::RESULT, "b", handle::TRIGGER),
                edge("e2", "a", handle::RESULT, "out", handle::TRIGGER),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn v4_data_wire_into_an_input_node_is_rejected() {
        let f = flow(
            vec![request("a"), input("in")],
            vec![edge("e1", "a", handle::RESULT, "in", "value")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn v5_request_edge_with_unknown_exit_is_rejected() {
        let f = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", "true", "b", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn v5_empty_source_handle_is_rejected() {
        let f = flow(
            vec![request("a"), request("b")],
            vec![edge("e1", "a", "", "b", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn v5_if_edge_with_result_exit_is_rejected() {
        let mut f = valid_if_flow();
        f.edges[1].source_handle = handle::RESULT.to_string();
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn v5_edge_from_a_deleted_switch_case_is_rejected() {
        let f = flow(
            vec![request("a"), switch_node("sw1", vec![("c1", "x")]), request("b")],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "sw1", &handle::case_handle("gone"), "b", handle::TRIGGER),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn v5_empty_case_handle_is_rejected() {
        let f = flow(
            vec![request("a"), switch_node("sw1", vec![("c1", "x")]), request("b")],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "sw1", handle::CASE_PREFIX, "b", handle::TRIGGER),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn v5_switch_case_and_default_exits_are_valid() {
        let f = flow(
            vec![request("a"), switch_node("sw1", vec![("c1", "x")]), request("b"), request("c")],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "sw1", &handle::case_handle("c1"), "b", handle::TRIGGER),
                edge("e3", "sw1", handle::DEFAULT, "c", handle::TRIGGER),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn v5_edge_out_of_an_output_node_is_rejected() {
        let f = flow(
            vec![output("out"), request("b")],
            vec![edge("e1", "out", handle::RESULT, "b", "url")],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn switch_with_no_cases_is_valid() {
        let f = flow(
            vec![request("a"), switch_node("sw1", vec![]), request("b")],
            vec![
                edge("e1", "a", handle::RESULT, "sw1", handle::INPUT),
                edge("e2", "sw1", handle::DEFAULT, "b", handle::TRIGGER),
            ],
        );
        assert!(validate(&f).is_ok());
    }

    #[test]
    fn v6_duplicate_case_ids_are_rejected() {
        let f = flow(
            vec![request("a"), switch_node("sw1", vec![("c1", "x"), ("c1", "y")])],
            vec![edge("e1", "a", handle::RESULT, "sw1", handle::INPUT)],
        );
        assert_eq!(invalid_node_id(validate(&f)), "sw1");
    }

    #[test]
    fn v7_duplicate_case_matches_are_rejected() {
        let f = flow(
            vec![request("a"), switch_node("sw1", vec![("c1", "pro"), ("c2", "pro")])],
            vec![edge("e1", "a", handle::RESULT, "sw1", handle::INPUT)],
        );
        match validate(&f) {
            Err(FlowGraphError::InvalidNode { node_id, reason }) => {
                assert_eq!(node_id, "sw1");
                assert!(reason.contains("pro"), "got: {reason}");
            }
            other => panic!("expected InvalidNode, got {other:?}"),
        }
    }

    #[test]
    fn v8_whitespace_only_condition_is_rejected() {
        let mut f = valid_if_flow();
        f.nodes[1] = if_node("if1", "   ");
        assert_eq!(invalid_node_id(validate(&f)), "if1");
    }

    #[test]
    fn v8_empty_switch_value_is_rejected() {
        let mut sw = switch_node("sw1", vec![("c1", "x")]);
        if let FlowNodeKind::Switch { value, .. } = &mut sw.kind {
            value.clear();
        }
        let f = flow(
            vec![request("a"), sw],
            vec![edge("e1", "a", handle::RESULT, "sw1", handle::INPUT)],
        );
        assert_eq!(invalid_node_id(validate(&f)), "sw1");
    }

    #[test]
    fn earlier_rule_wins_over_later_rule() {
        // e1 breaks V2 and if1 breaks V8. V2 comes first in rule order.
        let f = flow(
            vec![request("a"), request("b"), if_node("if1", "")],
            vec![
                edge("e1", "a", handle::RESULT, "b", handle::INPUT),
                edge("e2", "a", handle::RESULT, "if1", handle::INPUT),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e1");
    }

    #[test]
    fn earlier_edge_wins_within_the_same_rule() {
        let f = flow(
            vec![request("a"), request("b"), request("c")],
            vec![
                edge("e1", "a", handle::RESULT, "b", "url"),
                edge("e2", "a", "bogus", "b", "body"),
                edge("e3", "a", "bogus", "c", "body"),
            ],
        );
        assert_eq!(invalid_edge_id(validate(&f)), "e2");
    }

    #[test]
    fn new_error_variants_display_their_id_and_reason() {
        let node_err = FlowGraphError::InvalidNode {
            node_id: "if1".to_string(),
            reason: "needs an input".to_string(),
        };
        assert_eq!(node_err.to_string(), "invalid node if1: needs an input");
        let edge_err = FlowGraphError::InvalidEdge {
            edge_id: "e1".to_string(),
            reason: "unknown exit".to_string(),
        };
        assert_eq!(edge_err.to_string(), "invalid edge e1: unknown exit");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-flow -j4 validate::`
Expected: FAIL to compile with `cannot find function 'validate'` and `no variant named 'InvalidNode'`.

- [ ] **Step 4: Add the error variants**

In `crates/rocket-flow/src/graph.rs`, append to `FlowGraphError`:

```rust
    /// A node breaks a structural rule, e.g. an If node without an input wire.
    #[error("invalid node {node_id}: {reason}")]
    InvalidNode { node_id: String, reason: String },
    /// An edge breaks a structural rule, e.g. it leaves an exit that does not exist.
    #[error("invalid edge {edge_id}: {reason}")]
    InvalidEdge { edge_id: String, reason: String },
```

- [ ] **Step 5: Implement `validate`**

Prepend to `crates/rocket-flow/src/validate.rs`, above the test module:

```rust
//! Save-time and load-time structural validation of a Flow graph. See spec
//! §7 (rules V1-V8). Rules run in table order and the first violation found
//! is returned, so the same file always yields the same error.

use crate::flow::{Flow, FlowEdge, FlowNode};
use crate::graph::{topological_sort, FlowGraphError};
use crate::handle;
use crate::node::FlowNodeKind;
use std::collections::{HashMap, HashSet};

/// Validates `flow` and returns its node ids in topological order.
pub fn validate(flow: &Flow) -> Result<Vec<String>, FlowGraphError> {
    let order = topological_sort(flow)?;
    let kinds: HashMap<&str, &FlowNodeKind> = flow
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), &n.kind))
        .collect();

    check_routing_inputs(flow)?;
    check_edges(flow, &kinds, |edge, target, _| {
        (edge.target_field == handle::INPUT && !is_routing(target)).then(|| {
            format!("only If and Switch nodes have an '{}' input", handle::INPUT)
        })
    })?;
    check_edges(flow, &kinds, |edge, target, _| {
        let accepts_trigger = matches!(
            target,
            FlowNodeKind::Request { .. } | FlowNodeKind::Output { .. }
        );
        (edge.target_field == handle::TRIGGER && !accepts_trigger).then(|| {
            format!(
                "only Request and Output nodes have a '{}' input",
                handle::TRIGGER
            )
        })
    })?;
    check_edges(flow, &kinds, |_, target, _| {
        matches!(target, FlowNodeKind::Input { .. })
            .then(|| "Input nodes cannot receive wires".to_string())
    })?;
    check_edges(flow, &kinds, |edge, _, source| {
        (!source_handle_exists(source, &edge.source_handle)).then(|| {
            format!(
                "the source node has no exit named '{}'",
                edge.source_handle
            )
        })
    })?;
    check_switch_cases(flow, |cases| {
        first_duplicate(cases.iter().map(|c| c.id.as_str()))
            .map(|id| format!("more than one case has id '{id}'"))
    })?;
    check_switch_cases(flow, |cases| {
        first_duplicate(cases.iter().map(|c| c.matches.as_str()))
            .map(|m| format!("more than one case matches '{m}'"))
    })?;
    check_expressions(flow)?;

    Ok(order)
}

fn is_routing(kind: &FlowNodeKind) -> bool {
    matches!(kind, FlowNodeKind::If { .. } | FlowNodeKind::Switch { .. })
}

fn kind_name(kind: &FlowNodeKind) -> &'static str {
    match kind {
        FlowNodeKind::Request { .. } => "Request",
        FlowNodeKind::Input { .. } => "Input",
        FlowNodeKind::Output { .. } => "Output",
        FlowNodeKind::If { .. } => "If",
        FlowNodeKind::Switch { .. } => "Switch",
    }
}

fn invalid_node(node: &FlowNode, reason: String) -> FlowGraphError {
    FlowGraphError::InvalidNode {
        node_id: node.id.clone(),
        reason,
    }
}

/// V1: an If or Switch node has exactly one incoming edge, into `input`.
fn check_routing_inputs(flow: &Flow) -> Result<(), FlowGraphError> {
    for node in flow.nodes.iter().filter(|n| is_routing(&n.kind)) {
        let incoming: Vec<&FlowEdge> = flow
            .edges
            .iter()
            .filter(|e| e.target_node_id == node.id)
            .collect();
        let name = kind_name(&node.kind);
        match incoming.as_slice() {
            [edge] if edge.target_field == handle::INPUT => {}
            [edge] => {
                return Err(invalid_node(
                    node,
                    format!(
                        "the {name} node's wire must target '{}', not '{}'",
                        handle::INPUT,
                        edge.target_field
                    ),
                ))
            }
            _ => {
                return Err(invalid_node(
                    node,
                    format!(
                        "an {name} node needs exactly one input wire, found {}",
                        incoming.len()
                    ),
                ))
            }
        }
    }
    Ok(())
}

/// Runs one edge rule over every edge in file order. `rule` receives the
/// edge, its target kind and its source kind, and returns a reason when the
/// edge breaks the rule.
fn check_edges<F>(
    flow: &Flow,
    kinds: &HashMap<&str, &FlowNodeKind>,
    rule: F,
) -> Result<(), FlowGraphError>
where
    F: Fn(&FlowEdge, &FlowNodeKind, &FlowNodeKind) -> Option<String>,
{
    for edge in &flow.edges {
        let target = kind_of(kinds, &edge.target_node_id)?;
        let source = kind_of(kinds, &edge.source_node_id)?;
        if let Some(reason) = rule(edge, target, source) {
            return Err(FlowGraphError::InvalidEdge {
                edge_id: edge.id.clone(),
                reason,
            });
        }
    }
    Ok(())
}

/// `topological_sort` already rejected unknown ids, so this only fails if
/// that guarantee is ever broken.
fn kind_of<'a>(
    kinds: &HashMap<&str, &'a FlowNodeKind>,
    node_id: &str,
) -> Result<&'a FlowNodeKind, FlowGraphError> {
    kinds
        .get(node_id)
        .copied()
        .ok_or_else(|| FlowGraphError::UnknownNode {
            node_id: node_id.to_string(),
        })
}

/// V5: the exits each node kind has.
fn source_handle_exists(source: &FlowNodeKind, source_handle: &str) -> bool {
    match source {
        FlowNodeKind::Request { .. } | FlowNodeKind::Input { .. } => {
            source_handle == handle::RESULT
        }
        FlowNodeKind::Output { .. } => false,
        FlowNodeKind::If { .. } => source_handle == handle::TRUE || source_handle == handle::FALSE,
        FlowNodeKind::Switch { cases, .. } => {
            source_handle == handle::DEFAULT
                || handle::case_id_from_handle(source_handle)
                    .is_some_and(|case_id| cases.iter().any(|c| c.id == case_id))
        }
    }
}

/// Runs one Switch-case rule (V6 or V7) over every Switch node in file order.
fn check_switch_cases<F>(flow: &Flow, rule: F) -> Result<(), FlowGraphError>
where
    F: Fn(&[crate::node::SwitchCase]) -> Option<String>,
{
    for node in &flow.nodes {
        if let FlowNodeKind::Switch { cases, .. } = &node.kind {
            if let Some(reason) = rule(cases) {
                return Err(invalid_node(node, reason));
            }
        }
    }
    Ok(())
}

fn first_duplicate<'a>(values: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = HashSet::new();
    values.into_iter().find(|v| !seen.insert(*v))
}

/// V8: an If condition and a Switch value must not be blank.
fn check_expressions(flow: &Flow) -> Result<(), FlowGraphError> {
    for node in &flow.nodes {
        let expression = match &node.kind {
            FlowNodeKind::If { condition, .. } => condition,
            FlowNodeKind::Switch { value, .. } => value,
            _ => continue,
        };
        if expression.trim().is_empty() {
            let field = if matches!(node.kind, FlowNodeKind::If { .. }) {
                "condition"
            } else {
                "value"
            };
            return Err(invalid_node(
                node,
                format!("the {} node's {field} is empty", kind_name(&node.kind)),
            ));
        }
    }
    Ok(())
}
```

- [ ] **Step 6: Run the validation tests**

Run: `cargo test -p rocket-flow -j4`
Expected: PASS, including all 25 `validate::tests`.

- [ ] **Step 7: Run clippy on the crate**

Run: `cargo clippy -p rocket-flow -j4 --tests -- -D warnings`
Expected: `Finished` with no warnings.

- [ ] **Step 8: Update the crate guide**

In `crates/rocket-flow/CLAUDE.md`, change the `graph.rs` row and add a `validate.rs` row to the Module Map:

```markdown
| `graph.rs` | `topological_sort`, `reachable_from`, `FlowGraphError` |
| `validate.rs` | `validate` — `topological_sort` plus structural rules V1–V8 (spec 2026-09-28 §7) |
```

- [ ] **Step 9: Commit**

Commit using the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): validate routing node structure`.

---

### Task 4: `FlowService::save` uses `validate`

**Files:**
- Modify: `crates/rocket-app/src/flow_service.rs`

**Interfaces:**
- Consumes: `rocket_flow::validate`, `FlowGraphError::{InvalidNode, InvalidEdge}` (Task 3).
- Produces: the IPC-facing save errors, each `DomainError::InvalidInput` and ending with the tail `node(s): <ids>; edge(s): <ids>`:
  - Cycle (unchanged): `flow contains a cycle through node(s): a, b; edge(s): e1, e2`
  - InvalidNode: `flow is invalid: <reason> — node(s): <node_id>; edge(s): `
  - InvalidEdge: `flow is invalid: <reason> — node(s): ; edge(s): <edge_id>`

  Plan 04's `parseGraphErrorMessage` parses this tail with `/node\(s\): ([^;]*); edge\(s\): (.*)$/` and drops empty ids.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. This task changes the `save_flow` command's rejection path, which persists into the collection folder.

- [ ] **Step 2: Write the failing service tests**

Append to the `tests` module in `crates/rocket-app/src/flow_service.rs`:

```rust
    fn node_of(id: &str, kind: FlowNodeKind) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    #[test]
    fn save_rejects_an_if_node_without_input_and_names_it_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
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
        };
        let err = svc.save("demo", flow).expect_err("invalid flow must be rejected");
        assert!(matches!(
            err,
            rocket_shared::error::DomainError::InvalidInput(_)
        ));
        let message = err.to_string();
        assert!(message.contains("flow is invalid: "), "got: {message}");
        assert!(message.ends_with("node(s): if1; edge(s): "), "got: {message}");
    }

    #[test]
    fn save_rejects_a_bad_edge_and_names_it_in_the_tail() {
        let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
        let output = |id: &str| {
            node_of(
                id,
                FlowNodeKind::Output {
                    label: id.to_string(),
                },
            )
        };
        let flow = Flow {
            name: "Bad Edge".to_string(),
            nodes: vec![output("a"), output("b")],
            edges: vec![FlowEdge {
                id: "e9".to_string(),
                source_node_id: "a".to_string(),
                target_node_id: "b".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
                source_handle: rocket_flow::handle::RESULT.to_string(),
            }],
        };
        let message = svc
            .save("demo", flow)
            .expect_err("an edge out of an Output node must be rejected")
            .to_string();
        assert!(message.ends_with("node(s): ; edge(s): e9"), "got: {message}");
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p rocket-app -j4 flow_service::tests`
Expected: FAIL to compile with `non-exhaustive patterns: 'FlowGraphError::InvalidNode { .. }' and 'FlowGraphError::InvalidEdge { .. }' not covered`. If it compiles because of an earlier partial edit, the two new tests fail with `save must not be called for a cyclic flow` panics instead.

- [ ] **Step 4: Switch `save` to `validate` and map every error**

Replace the imports and `save` in `crates/rocket-app/src/flow_service.rs`:

```rust
use rocket_flow::{validate, Flow, FlowGraphError, FlowRepository};
use rocket_shared::error::{DomainError, DomainResult};
```

```rust
    pub fn save(&self, collection: &str, flow: Flow) -> DomainResult<()> {
        validate(&flow).map_err(|e| DomainError::InvalidInput(graph_error_message(e)))?;
        self.flow_repo.save(collection, &flow)
    }
```

Add this free function below the `impl FlowService` block:

```rust
/// Builds the save-error text. Every message that names graph elements ends
/// with "node(s): <ids>; edge(s): <ids>", which the canvas parses to
/// highlight them in red.
fn graph_error_message(error: FlowGraphError) -> String {
    match error {
        FlowGraphError::Cycle { node_ids, edge_ids } => format!(
            "flow contains a cycle through node(s): {}; edge(s): {}",
            node_ids.join(", "),
            edge_ids.join(", ")
        ),
        FlowGraphError::UnknownNode { node_id } => {
            format!("edge references unknown node: {node_id}")
        }
        FlowGraphError::DuplicateNode { node_id } => {
            format!("flow has more than one node with id: {node_id}")
        }
        FlowGraphError::InvalidNode { node_id, reason } => {
            format!("flow is invalid: {reason} — node(s): {node_id}; edge(s): ")
        }
        FlowGraphError::InvalidEdge { edge_id, reason } => {
            format!("flow is invalid: {reason} — node(s): ; edge(s): {edge_id}")
        }
    }
}
```

If other code in the file still refers to `rocket_shared::error::DomainError` by full path, leave it; both forms work.

- [ ] **Step 5: Run the service tests**

Run: `cargo test -p rocket-app -j4 flow_service::tests`
Expected: PASS, including the pre-existing `save_rejects_cyclic_graph_and_names_the_nodes` (the cycle message is unchanged) and `save_persists_an_acyclic_flow`.

- [ ] **Step 6: Run the full verification set for this plan**

Run: `cargo check -j4 --workspace --tests`
Expected: `Finished`.

Run: `cargo test -j4 -p rocket-flow -p rocket-app -p rocket-infra`
Expected: PASS.

Run: `cargo test -j4 -p rocket flow`
Expected: PASS.

Run: `cargo clippy -j4 -p rocket-flow -p rocket-app -- -D warnings`
Expected: `Finished` with no warnings.

- [ ] **Step 7: Commit**

Commit using the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(flow): reject structurally invalid flows on save`.

---

## Next Plan

[Plan 02 — Events and step results](2026-09-28-flow-phase2-branching-plan-02-events-and-step-results.md)

## Post-Implementation Review

Before starting plan 02, dispatch an Opus-model subagent (Agent tool, `model: "opus"`) to review everything this plan added or modified. Give it explicit authority to fix what it finds directly. It should check:

- Every `FlowEdge` literal and every `FlowNodeKind` match in the workspace compiles, and no struct-update edge silently inherits a wrong `source_handle`.
- `validate` matches spec §7 rule by rule (V1–V8) and in the documented rule order.
- Phase 1 byte-identical re-save holds in both `rocket-flow` and `FsFlowRepo`.
- DTOs are camelCase and persistence structs are snake_case, with no `rename_all` leaking onto domain types.
- The temporary `execute_node` arm and its test are clearly marked for removal by plan 03.
- `crates/rocket-flow/CLAUDE.md` reflects the new modules.
- Error messages keep the `node(s): …; edge(s): …` tail that plan 04 parses.
