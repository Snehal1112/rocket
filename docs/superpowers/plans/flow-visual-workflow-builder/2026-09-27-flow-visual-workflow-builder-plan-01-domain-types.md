# Flow Plan 01: rocket-flow Domain Types — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the new `rocket-flow` crate holding the Flow/FlowNode/FlowEdge
data model (Request/Input/Output node kinds, the Saved/Inline request source,
and the `FlowRepository` persistence-boundary trait) that the rest of the
Flow feature builds on.

**Architecture:** A pure domain crate — no I/O, no network, zero
cross-domain-crate imports — modeled directly on `rocket-acp`'s
`agent_config.rs`: plain serde structs/enums plus one trait for the
persistence boundary. `rocket-infra` (Plan 03) implements the trait;
`rocket-app` (Plans 05-06) orchestrates it.

**Tech Stack:** Rust, serde, serde_json (tests).

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§4 Data model). Plan index:
`docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md` (has
the full locked interface contract every later plan in this series depends
on).

## Global Constraints

- `rocket-flow` depends only on `rocket-shared`, `serde` (+ `serde_json` as a
  dev-dependency for tests) — no cross-domain-crate imports (no
  `rocket-collection`, no `rocket-http`). `InlineRequestData` is a new,
  minimal shape defined in this crate — it does **not** reuse
  `rocket-http::HttpRequest` (see the plan index's "Key design resolutions"
  section); translating it into `rocket-app`'s `ExecuteRequestInput` is
  Plan 05's job.
- No `additionalProperties: false` schema constraint applies to any type in
  this crate — these are a Rocket-only extension format (design spec §5), not
  OpenCollection types. Do not add schema-validation code.
- `FlowNodeKind` uses `#[serde(tag = "kind")]` and `RequestSource` uses
  `#[serde(tag = "type")]` — both internally-tagged enums, per the plan
  index's locked contract. Verify the exact JSON shape with a round-trip test
  rather than assuming serde's default behavior.
- `InlineRequestData.headers: Vec<InlineHeader>` and `.body: Option<String>`
  both need `#[serde(default)]` so a minimal hand-written fixture without
  those fields still deserializes, matching this repo's standard practice for
  `Vec`/`Option` fields on every other domain crate.
- This repo's hard rule against panicking shorthand on `Result`/`Option` in
  production code paths applies here too. Test code uses `.expect("message")`
  for fallible calls, never the bare panicking shorthand.
- `FlowNodeKind::Input`'s `value` field is `rocket_shared::VariableValue` —
  that type already implements `Serialize`/`Deserialize` by hand (a custom
  visitor, not `#[derive]`), so no extra work is needed to embed it.

## Review Focus

- Internally-tagged enum round-trip: both `FlowNodeKind` (all three variants:
  `Request`, `Input`, `Output`) and `RequestSource` (`Saved`, `Inline`) must
  serialize with their declared tag field and deserialize back to an equal
  value.
- `InlineRequestData.headers` empty-vs-populated must round-trip through
  serde without the field disappearing when empty; `.body: None` absent from
  input JSON must deserialize to `None`, not error.
- `FlowRepository` must be object-safe (`Box<dyn FlowRepository>` compiles)
  since `rocket-app` (Plans 05-06) holds it as a trait object.
- Two `FlowNode`s with different `id` but the same node kind must not be
  conflated by a fake in-memory `FlowRepository` test double — `id` is the
  only identity key within a `Flow`'s `nodes` list.
- `Flow.name` and all node positions round-trip unchanged through a
  `FlowRepository::save` → `::get` cycle in a fake repository (guards against
  an accidental rename or lossy field leaking into the domain type itself —
  slugification is Plan 03's filesystem-layer concern, not this crate's).

---

## Task 1: Crate scaffold + node/edge value types

**Files:**
- Create: `crates/rocket-flow/Cargo.toml`
- Create: `crates/rocket-flow/src/lib.rs`
- Create: `crates/rocket-flow/src/node.rs`
- Modify: `Cargo.toml` (root workspace manifest)

**Interfaces:**
- Produces: `NodePosition { x, y }`, `FlowNodeKind::{Request, Input, Output}`,
  `RequestSource::{Saved, Inline}`, `InlineRequestData { method, url, headers,
  body }`, `InlineHeader { name, value }` — consumed by Task 2 of this plan
  and every later plan in this series.

- [ ] **Step 1: 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.**

This plan's types are collection-adjacent (`RequestSource::Saved` references
a request by its collection-relative path) even though they are not part of
the OpenCollection schema themselves — confirm you understand why before
writing any serde attributes here (design spec §5: this is a Rocket-only
extension, not a schema addition).

- [ ] **Step 2: Scaffold the crate and register it in the workspace**

Create `crates/rocket-flow/Cargo.toml`:

```toml
[package]
name = "rocket-flow"
version.workspace = true
edition.workspace = true

[dependencies]
rocket-shared.workspace = true
serde.workspace = true

[dev-dependencies]
serde_json.workspace = true
```

Create `crates/rocket-flow/src/lib.rs`:

```rust
pub mod node;
```

In the root `Cargo.toml`, add `"crates/rocket-flow",` to the `[workspace]`
`members` list (alongside the existing `"crates/rocket-acp",` entry — add it
right after that line), and add `rocket-flow = { path = "crates/rocket-flow" }`
to the `[workspace.dependencies]` internal-crates section (alongside
`rocket-acp = { path = "crates/rocket-acp" }` — add it right after that
line).

Run: `cargo check -p rocket-flow -j4`
Expected: succeeds (empty crate, compiles with just the empty `node` module —
create an empty `crates/rocket-flow/src/node.rs` file first if the compiler
complains about the missing module file).

- [ ] **Step 3: Write the failing tests**

```rust
// crates/rocket-flow/src/node.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_node_kind_request_tagged_roundtrip() {
        let kind = FlowNodeKind::Request {
            label: "Get Auth Token".to_string(),
            source: RequestSource::Saved {
                request_path: "auth/login.yml".to_string(),
            },
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Request\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn flow_node_kind_input_tagged_roundtrip() {
        let kind = FlowNodeKind::Input {
            label: "Username".to_string(),
            value: rocket_shared::VariableValue::simple("alice"),
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Input\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn flow_node_kind_output_tagged_roundtrip() {
        let kind = FlowNodeKind::Output {
            label: "Result".to_string(),
        };
        let json = serde_json::to_string(&kind).expect("serialize FlowNodeKind");
        assert!(json.contains("\"kind\":\"Output\""), "got: {json}");
        let back: FlowNodeKind = serde_json::from_str(&json).expect("deserialize FlowNodeKind");
        assert_eq!(kind, back);
    }

    #[test]
    fn request_source_saved_tagged_roundtrip() {
        let source = RequestSource::Saved {
            request_path: "auth/login.yml".to_string(),
        };
        let json = serde_json::to_string(&source).expect("serialize RequestSource");
        assert!(json.contains("\"type\":\"Saved\""), "got: {json}");
        let back: RequestSource = serde_json::from_str(&json).expect("deserialize RequestSource");
        assert_eq!(source, back);
    }

    #[test]
    fn request_source_inline_tagged_roundtrip() {
        let source = RequestSource::Inline {
            request: InlineRequestData {
                method: "GET".to_string(),
                url: "https://api.example.com/users".to_string(),
                headers: vec![InlineHeader {
                    name: "Accept".to_string(),
                    value: "application/json".to_string(),
                }],
                body: None,
            },
        };
        let json = serde_json::to_string(&source).expect("serialize RequestSource");
        assert!(json.contains("\"type\":\"Inline\""), "got: {json}");
        let back: RequestSource = serde_json::from_str(&json).expect("deserialize RequestSource");
        assert_eq!(source, back);
    }

    #[test]
    fn inline_request_data_headers_roundtrip_when_empty() {
        let data = InlineRequestData {
            method: "GET".to_string(),
            url: "https://api.example.com".to_string(),
            headers: Vec::new(),
            body: None,
        };
        let json = serde_json::to_string(&data).expect("serialize InlineRequestData");
        let back: InlineRequestData =
            serde_json::from_str(&json).expect("deserialize InlineRequestData");
        assert!(back.headers.is_empty());
        assert_eq!(back.body, None);
    }

    #[test]
    fn inline_request_data_defaults_when_headers_and_body_absent() {
        let json = r#"{"method":"GET","url":"https://api.example.com"}"#;
        let data: InlineRequestData =
            serde_json::from_str(json).expect("deserialize minimal InlineRequestData");
        assert!(data.headers.is_empty());
        assert_eq!(data.body, None);
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p rocket-flow node::tests -j4`
Expected: FAIL with "cannot find type `FlowNodeKind`" (compile error — none
of these types exist yet).

- [ ] **Step 5: Implement the types**

```rust
// crates/rocket-flow/src/node.rs (add above the tests module)
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

/// One node on a Flow canvas. `Request` nodes call an HTTP request when the
/// flow runs; `Input` nodes hold a constant/variable-backed value with no
/// incoming wires; `Output` nodes display whatever their single incoming
/// wire resolves to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FlowNodeKind {
    Request { label: String, source: RequestSource },
    Input { label: String, value: rocket_shared::VariableValue },
    Output { label: String },
}

/// Where a `Request` node's method/url/headers/body/auth come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RequestSource {
    /// Live reference into the collection tree — `request_path` is relative
    /// to the collection root, e.g. "auth/login.yml". Resolved at run time
    /// (Plan 05), not snapshotted here.
    Saved { request_path: String },
    /// A full ad hoc request embedded directly in the flow file. Deliberately
    /// a minimal, crate-local shape — not a reuse of `rocket-http::HttpRequest`
    /// — so `rocket-flow` stays free of cross-domain-crate dependencies.
    Inline { request: InlineRequestData },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineRequestData {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<InlineHeader>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineHeader {
    pub name: String,
    pub value: String,
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-flow node::tests -j4`
Expected: PASS — 7 tests.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/rocket-flow
git commit -m "feat(flow): add rocket-flow crate with node/edge value types"
```

---

## Task 2: `FlowNode`/`FlowEdge`/`Flow` aggregate + `FlowRepository` trait

**Files:**
- Create: `crates/rocket-flow/src/flow.rs`
- Modify: `crates/rocket-flow/src/lib.rs`
- Create: `crates/rocket-flow/CLAUDE.md`

**Interfaces:**
- Consumes: `NodePosition`, `FlowNodeKind` from Task 1.
- Produces: `FlowNode { id, kind, position }`, `FlowEdge { id, source_node_id,
  target_node_id, target_field, expression }`, `Flow { name, nodes, edges }`,
  `FlowRepository { list, get, save, delete }` — consumed by Plan 02 (graph
  validation reads `Flow`), Plan 03 (`FsFlowRepo` impl), and Plans 05-06
  (`rocket-app` orchestration).

- [ ] **Step 1: Write the types and the failing tests together**

```rust
// crates/rocket-flow/src/flow.rs
use crate::node::{FlowNodeKind, NodePosition};
use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    pub kind: FlowNodeKind,
    pub position: NodePosition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowEdge {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub target_field: String,
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
}

/// Persistence boundary for `Flow`. No I/O in this crate —
/// `rocket-infra`'s `FsFlowRepo` (Plan 03) implements this.
pub trait FlowRepository: Send + Sync {
    fn list(&self, collection: &str) -> DomainResult<Vec<String>>;
    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow>;
    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()>;
    fn delete(&self, collection: &str, name: &str) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::RequestSource;
    use std::sync::Mutex;

    fn sample_flow() -> Flow {
        Flow {
            name: "Login then fetch profile".to_string(),
            nodes: vec![
                FlowNode {
                    id: "node-1".to_string(),
                    kind: FlowNodeKind::Request {
                        label: "Login".to_string(),
                        source: RequestSource::Saved {
                            request_path: "auth/login.yml".to_string(),
                        },
                    },
                    position: NodePosition { x: 0.0, y: 0.0 },
                },
                FlowNode {
                    id: "node-2".to_string(),
                    kind: FlowNodeKind::Output {
                        label: "Token".to_string(),
                    },
                    position: NodePosition { x: 200.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdge {
                id: "edge-1".to_string(),
                source_node_id: "node-1".to_string(),
                target_node_id: "node-2".to_string(),
                target_field: "value".to_string(),
                expression: "response.body.token".to_string(),
            }],
        }
    }

    struct FakeRepo(Mutex<Vec<(String, Flow)>>); // (collection, flow)
    impl FakeRepo {
        fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }
    }
    impl FlowRepository for FakeRepo {
        fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeRepo")
                .iter()
                .filter(|(c, _)| c == collection)
                .map(|(_, f)| f.name.clone())
                .collect())
        }
        fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
            self.0
                .lock()
                .expect("lock FakeRepo")
                .iter()
                .find(|(c, f)| c == collection && f.name == name)
                .map(|(_, f)| f.clone())
                .ok_or_else(|| rocket_shared::error::DomainError::NotFound(name.to_string()))
        }
        fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|(c, f)| !(c == collection && f.name == flow.name));
            guard.push((collection.to_string(), flow.clone()));
            Ok(())
        }
        fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeRepo")
                .retain(|(c, f)| !(c == collection && f.name == name));
            Ok(())
        }
    }

    #[test]
    fn repository_trait_save_get_delete_roundtrip() {
        let repo = FakeRepo::new();
        let flow = sample_flow();
        repo.save("my-collection", &flow).expect("save flow");
        let fetched = repo.get("my-collection", &flow.name).expect("get flow");
        assert_eq!(fetched, flow);
        assert_eq!(
            repo.list("my-collection").expect("list flows"),
            vec![flow.name.clone()]
        );
        repo.delete("my-collection", &flow.name).expect("delete flow");
        assert!(repo.get("my-collection", &flow.name).is_err());
    }

    #[test]
    fn repository_save_replaces_existing_entry_with_same_name_instead_of_duplicating() {
        let repo = FakeRepo::new();
        let mut flow = sample_flow();
        repo.save("my-collection", &flow).expect("save first");
        flow.nodes[0].position = NodePosition { x: 500.0, y: 500.0 };
        repo.save("my-collection", &flow).expect("save update");
        let all = repo.list("my-collection").expect("list");
        assert_eq!(all.len(), 1, "same name must replace, not append");
        let fetched = repo.get("my-collection", &flow.name).expect("get updated");
        assert_eq!(
            fetched.nodes[0].position,
            NodePosition { x: 500.0, y: 500.0 }
        );
    }

    #[test]
    fn two_nodes_with_different_ids_are_not_conflated() {
        let flow = sample_flow();
        assert_ne!(flow.nodes[0].id, flow.nodes[1].id);
        let by_id = |id: &str| flow.nodes.iter().find(|n| n.id == id).expect("node exists");
        assert!(matches!(by_id("node-1").kind, FlowNodeKind::Request { .. }));
        assert!(matches!(by_id("node-2").kind, FlowNodeKind::Output { .. }));
    }

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn FlowRepository>) {}
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-flow flow::tests -j4`
Expected: FAIL with "unresolved module `flow`" (the module isn't registered
in `lib.rs` yet — proceed to Step 3, then re-run).

- [ ] **Step 3: Register the module export**

In `crates/rocket-flow/src/lib.rs`:

```rust
pub mod flow;
pub mod node;

pub use flow::{Flow, FlowEdge, FlowNode, FlowRepository};
pub use node::{FlowNodeKind, InlineHeader, InlineRequestData, NodePosition, RequestSource};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-flow -j4`
Expected: PASS — 11 tests total (7 from Task 1, 4 from this task).

- [ ] **Step 5: Write the crate CLAUDE.md**

Create `crates/rocket-flow/CLAUDE.md`:

```markdown
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

The `rocket-flow` crate is a pure domain crate in the Rocket HTTP client
workspace. It owns the `Flow`/`FlowNode`/`FlowEdge` entities (a visual,
node-canvas API workflow definition — see
`docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`)
and the `FlowRepository` trait. It has no I/O — the filesystem implementation
lives in `rocket-infra` (`FsFlowRepo`).

## Commands

\`\`\`bash
# Check this crate
cargo check -p rocket-flow -j4

# Run all tests in this crate
cargo test -p rocket-flow -j4
\`\`\`

## Architecture

### Module Map

| Module | Responsibility |
|---|---|
| `node.rs` | `FlowNodeKind`, `RequestSource`, `InlineRequestData`, `InlineHeader`, `NodePosition` |
| `flow.rs` | `FlowNode`, `FlowEdge`, `Flow` aggregate, `FlowRepository` trait |
| `graph.rs` | `topological_sort` + `FlowGraphError` (added in Plan 02) |

### Key Design Points

- No cross-domain-crate dependencies — `RequestSource::Saved` references a
  collection request by plain `String` path, not by importing
  `rocket-collection`'s types. `RequestSource::Inline` embeds its own minimal
  `InlineRequestData`, not `rocket-http::HttpRequest`.
- `FlowEdge.target_field` is a plain string path ("url", "headers[1].value",
  "body"), not a fixed enum, so new wireable fields on a node type never
  require a schema change here.
- `FlowEdge.expression` is a JS/jsonq expression evaluated against the source
  node's captured output at run time (`rocket-app`, Plan 05) — this crate
  does not evaluate it, only carries it as data.
- Not part of the OpenCollection schema (`additionalProperties: false` does
  not apply) — this is a Rocket-only extension format.

### Dependencies

- `rocket-shared` — `DomainResult`, `VariableValue`
- `serde` — serialization
```

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-flow
git commit -m "feat(flow): add Flow aggregate and FlowRepository trait"
```

---

## Next Plan

[Plan 02: rocket-flow graph validation](2026-09-27-flow-visual-workflow-builder-plan-02-graph-validation.md) —
adds `topological_sort` (Kahn's algorithm) and `FlowGraphError` (cycle/unknown-node
detection) on top of the `Flow` type this plan defines.

## Post-Implementation Review

Before starting Plan 02, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `Cargo.toml` (root), `crates/rocket-flow/Cargo.toml`,
> `crates/rocket-flow/src/lib.rs`, `crates/rocket-flow/src/node.rs`,
> `crates/rocket-flow/src/flow.rs`, `crates/rocket-flow/CLAUDE.md`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — do `FlowNodeKind`,
>    `RequestSource`, `InlineRequestData`, `FlowNode`, `FlowEdge`, `Flow`, and
>    `FlowRepository` match exactly what
>    `docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md`'s
>    locked interface contract promises Plans 02/03/05/06 will consume?
> 2. Code quality — naming, doc comments, test coverage versus this plan's
>    Review Focus section (tagged-enum round-trips for both `FlowNodeKind`
>    and `RequestSource`; empty/absent `InlineRequestData` fields; trait
>    object-safety; per-name save semantics not conflating different flows).
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    specifically that `rocket-flow` has zero cross-domain-crate dependencies
>    (no `rocket-collection`, no `rocket-http`) and contains no I/O.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-flow -j4` and
> `cargo check -p rocket-flow -j4`, and confirm they still pass. Report what
> you found and fixed.

Only proceed to Plan 02 once this review comes back clean (or its fixes are
applied and re-verified).
