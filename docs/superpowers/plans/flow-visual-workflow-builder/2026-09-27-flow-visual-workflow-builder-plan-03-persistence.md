# Flow Plan 03: Persistence — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `FlowRepository` as `FsFlowRepo` in `rocket-infra` — one
`.yml` file per Flow, stored at `<collection>/flows/<slug>.yml`.

**Architecture:** Modeled on `FsCollectionRepo`'s `base_dir.join(collection)`
pattern (`crates/rocket-infra/src/fs_collection/mod.rs`), not
`FsEnvironmentRepo`'s fixed-single-directory pattern — `FlowRepository`'s
trait methods take `collection: &str` per call (see the plan index's locked
contract), so one `FsFlowRepo` instance, holding the same collections
`base_dir` `FsCollectionRepo` uses, serves every collection. This avoids
introducing a second, environment-style "one repo instance per directory"
convention just for Flow.

**Tech Stack:** Rust, serde_yaml, tempfile (tests).

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§5 Persistence). Plan index:
`docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md` (has
the full locked interface contract every later plan depends on).

## Global Constraints

- 📖 Before starting Task 1, read
  `docs/superpowers/specs/opencollection-spec-reference.md` — `rocket-infra`
  is on this repo's OpenCollection-spec trigger list
  (`.claude/rules/rust-ddd-boundaries.md`), even though a Flow file has
  nothing to do with the OpenCollection schema itself; the rule triggers on
  the crate touched, not the content. Confirm while reading it: `flows/` is a
  **Rocket-only extension directory**, never referenced from
  `opencollection.yml`'s `items[]` or any spec-defined field — a reviewer
  checking OpenCollection compliance elsewhere in this repo must not mistake
  a `<collection>/flows/` directory for a schema violation, since it sits
  beside `opencollection.yml`/`environments/` without the schema validator
  ever looking at it.
- All flow files are `.yml`, never `.json`, matching every other on-disk
  format in this repo.
- `Flow`/`FlowNode`/`FlowEdge` (from `rocket-flow`, Plan 01) get **no**
  `#[serde(rename_all = "camelCase")]` — same "no camelCase on persistence
  structs" rule as `AgentConfig` and `SecretManagerConnection`. The camelCase
  IPC boundary is a separate DTO layer, added in Plan 07.
- Use `atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()>`
  (`crates/rocket-infra/src/atomic_write.rs`) for every write — never a raw
  `std::fs::write`.
- No panicking-unwrap shorthand in production code paths — return
  `DomainResult` and map errors explicitly. `.expect("message")` is
  acceptable in test code only, per this repo's stricter Rust safety
  convention.
- Test code uses `.expect("message")` for fallible calls, never the bare
  panicking shorthand.

## Review Focus

- `list` on a collection whose `flows/` directory does not exist yet (no
  flow ever saved) returns an empty list, not an error — mirroring
  `FsEnvironmentRepo::list`'s `if !self.dir.exists() { return Ok(...) }`
  guard (`crates/rocket-infra/src/fs_environment_repo.rs:107-109`).
- `save` under the same flow name twice overwrites the same file (one file
  per name), not two files — since the filename is derived from
  `flow.name` via slugification, this falls out naturally as long as the
  slug function is deterministic; add an explicit test rather than assuming
  it.
- `delete` of a flow name that was never saved returns
  `DomainError::NotFound`, matching `FsCollectionRepo`'s
  `delete_request`'s behavior on a missing file exactly
  (`crates/rocket-infra/src/fs_collection/requests.rs:150-152`) — **not** a
  silent no-op (that's the ACP/`FsAgentConfigRepo` convention for its
  flat-list file, which does not apply here; Flow is one-file-per-entity,
  like requests, not one-file-holds-a-list, like `agent_configs.yml`).
- A flow name containing spaces, mixed case, or punctuation (e.g. `"My
  First Flow!"`) must slugify to a stable, filesystem-safe name (e.g.
  `my-first-flow`) and round-trip correctly through `save`/`get`/`list` —
  there is no existing shared slugify helper in this codebase to reuse (only
  `Collection::validate_name` exists, which rejects rather than transforms),
  so this plan adds a small private one; do not search further for a
  nonexistent shared utility.
- Malformed/corrupt YAML in a flow file must surface as
  `DomainError::InvalidInput`, not panic, matching
  `FsEnvironmentRepo`/`FsAgentConfigRepo`'s parse-failure handling.

---

## Task 1: `FsFlowRepo` CRUD

**Files:**
- Create: `crates/rocket-infra/src/fs_flow_repo.rs`
- Modify: `crates/rocket-infra/src/lib.rs`
- Modify: `crates/rocket-infra/Cargo.toml`

**Interfaces:**
- Consumes: `Flow`, `FlowRepository` from `rocket-flow` (Plan 01).
- Produces: `FsFlowRepo::new(base_dir: PathBuf) -> Self` implementing
  `FlowRepository` — consumed by Plan 06 (`FlowExecutionService`) and Plan
  07 (`src-tauri/src/lib.rs` wiring).

- [ ] **Step 1: Add the `rocket-flow` dependency**

In `crates/rocket-infra/Cargo.toml`, add `rocket-flow.workspace = true` to
`[dependencies]`, alongside the existing `rocket-collection.workspace =
true` line.

Run: `cargo check -p rocket-infra -j4`
Expected: succeeds (no code uses the new dependency yet, but it must
resolve — this also requires Plan 01's `rocket-flow` crate to already be
registered in the workspace; if this fails with "package `rocket-flow` not
found", Plan 01 has not been completed yet).

- [ ] **Step 2: Write the failing tests**

```rust
// crates/rocket-infra/src/fs_flow_repo.rs
use std::fs;
use std::path::{Path, PathBuf};

use rocket_flow::{Flow, FlowEdge, FlowNode, FlowNodeKind, FlowRepository, NodePosition, RequestSource};
use rocket_shared::error::{DomainError, DomainResult};

use crate::atomic_write;

pub struct FsFlowRepo {
    base_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FsFlowRepo) {
        let dir = TempDir::new().expect("create temp dir");
        let repo = FsFlowRepo::new(dir.path().to_path_buf());
        fs::create_dir_all(dir.path().join("acme")).expect("create collection dir");
        (dir, repo)
    }

    fn sample(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: vec![FlowNode {
                id: "node-1".to_string(),
                kind: FlowNodeKind::Input {
                    label: "Base URL".to_string(),
                    value: rocket_shared::VariableValue::simple("https://api.example.com"),
                },
                position: NodePosition { x: 100.0, y: 200.0 },
            }],
            edges: Vec::new(),
        }
    }

    #[test]
    fn list_on_collection_with_no_flows_dir_returns_empty() {
        let (_dir, repo) = setup();
        assert_eq!(repo.list("acme").expect("list"), Vec::<String>::new());
    }

    #[test]
    fn get_on_missing_flow_returns_not_found() {
        let (_dir, repo) = setup();
        let err = repo.get("acme", "no-such-flow").expect_err("must not find a flow that was never saved");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn save_get_list_roundtrip() {
        let (_dir, repo) = setup();
        let flow = sample("Login Flow");
        repo.save("acme", &flow).expect("save");

        let loaded = repo.get("acme", "Login Flow").expect("get");
        assert_eq!(loaded, flow);
        assert_eq!(repo.list("acme").expect("list"), vec!["Login Flow".to_string()]);
    }

    #[test]
    fn save_under_same_name_replaces_not_duplicates() {
        let (_dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save first");

        let mut updated = sample("Login Flow");
        updated.nodes.push(FlowNode {
            id: "node-2".to_string(),
            kind: FlowNodeKind::Output { label: "Result".to_string() },
            position: NodePosition { x: 400.0, y: 200.0 },
        });
        repo.save("acme", &updated).expect("save update");

        let names = repo.list("acme").expect("list");
        assert_eq!(names.len(), 1, "same flow name must replace, not append a second file");
        assert_eq!(repo.get("acme", "Login Flow").expect("get").nodes.len(), 2);
    }

    #[test]
    fn delete_of_missing_flow_returns_not_found() {
        let (_dir, repo) = setup();
        let err = repo
            .delete("acme", "no-such-flow")
            .expect_err("deleting a flow that was never saved must error");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[test]
    fn delete_removes_the_flow() {
        let (_dir, repo) = setup();
        repo.save("acme", &sample("Login Flow")).expect("save");
        repo.delete("acme", "Login Flow").expect("delete");
        assert!(repo.list("acme").expect("list").is_empty());
    }

    #[test]
    fn flow_name_with_spaces_and_punctuation_slugifies_stably() {
        let (dir, repo) = setup();
        repo.save("acme", &sample("My First Flow!")).expect("save");

        assert!(
            dir.path().join("acme").join("flows").join("my-first-flow.yml").exists(),
            "expected slugified filename my-first-flow.yml"
        );
        let loaded = repo.get("acme", "My First Flow!").expect("get by original name");
        assert_eq!(loaded.name, "My First Flow!");
    }

    #[test]
    fn malformed_yaml_errors_clearly_instead_of_panicking() {
        let (dir, repo) = setup();
        let flows_dir = dir.path().join("acme").join("flows");
        fs::create_dir_all(&flows_dir).expect("create flows dir");
        fs::write(flows_dir.join("broken.yml"), b"not: valid: yaml: [").expect("write malformed file");

        let err = repo.get("acme", "broken").expect_err("malformed YAML must error, not panic");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn on_disk_file_has_no_camelcase_field_names() {
        let (dir, repo) = setup();
        let mut flow = sample("Login Flow");
        flow.edges.push(FlowEdge {
            id: "edge-1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "node-2".to_string(),
            target_field: "url".to_string(),
            expression: "response.body".to_string(),
        });
        repo.save("acme", &flow).expect("save");
        let raw = fs::read_to_string(dir.path().join("acme").join("flows").join("login-flow.yml"))
            .expect("read saved flow file");
        assert!(raw.contains("source_node_id"), "expected snake_case field, got:\n{raw}");
        assert!(!raw.contains("sourceNodeId"), "must not contain camelCase, got:\n{raw}");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p rocket-infra fs_flow_repo -j4`
Expected: FAIL — `FsFlowRepo::new`, `list`, `get`, `save`, `delete` don't
exist yet (compile error).

- [ ] **Step 4: Implement the repository**

```rust
// crates/rocket-infra/src/fs_flow_repo.rs (add above the tests module,
// replacing the bare `pub struct FsFlowRepo { base_dir: PathBuf }` stub)

impl FsFlowRepo {
    /// `base_dir` is the same collections base directory `FsCollectionRepo`
    /// uses — one `FsFlowRepo` instance serves every collection, dispatched
    /// by the `collection` argument on each trait method.
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn flows_dir(&self, collection: &str) -> PathBuf {
        self.base_dir.join(collection).join("flows")
    }

    fn file_path(&self, collection: &str, name: &str) -> PathBuf {
        self.flows_dir(collection).join(format!("{}.yml", slugify(name)))
    }

    fn read_flow(&self, path: &Path, not_found_label: &str) -> DomainResult<Flow> {
        if !path.exists() {
            return Err(DomainError::NotFound(not_found_label.to_string()));
        }
        let content = fs::read_to_string(path)
            .map_err(|e| DomainError::Io(format!("Failed to read flow file: {e}")))?;
        serde_yaml::from_str(&content)
            .map_err(|e| DomainError::InvalidInput(format!("Failed to parse flow YAML: {e}")))
    }
}

/// Lowercase, hyphen-separated slug for a Flow's filename. This repo has no
/// existing shared slugify helper to reuse (`Collection::validate_name`
/// rejects invalid names rather than transforming them) — this is new,
/// self-contained logic.
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_hyphen = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if !last_was_hyphen && !slug.is_empty() {
            slug.push('-');
            last_was_hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

impl FlowRepository for FsFlowRepo {
    fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        let dir = self.flows_dir(collection);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|e| DomainError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| DomainError::Io(e.to_string()))?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "yml") {
                continue;
            }
            let content = fs::read_to_string(&path).map_err(|e| DomainError::Io(e.to_string()))?;
            let flow: Flow = serde_yaml::from_str(&content).map_err(|e| {
                DomainError::InvalidInput(format!("Failed to parse flow YAML: {e}"))
            })?;
            names.push(flow.name);
        }
        names.sort();
        Ok(names)
    }

    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        let path = self.file_path(collection, name);
        self.read_flow(&path, &format!("Flow '{name}' in collection '{collection}'"))
    }

    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
        let dir = self.flows_dir(collection);
        fs::create_dir_all(&dir).map_err(|e| DomainError::Io(e.to_string()))?;
        let yaml = serde_yaml::to_string(flow)
            .map_err(|e| DomainError::InvalidInput(format!("Failed to serialize flow: {e}")))?;
        atomic_write(&self.file_path(collection, &flow.name), yaml.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write flow file: {e}")))
    }

    fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        let path = self.file_path(collection, name);
        if !path.exists() {
            return Err(DomainError::NotFound(format!(
                "Flow '{name}' in collection '{collection}'"
            )));
        }
        fs::remove_file(&path).map_err(|e| DomainError::Io(e.to_string()))
    }
}
```

- [ ] **Step 5: Register the module**

In `crates/rocket-infra/src/lib.rs`, add alongside the existing module
declarations:

```rust
pub mod fs_flow_repo;
pub use fs_flow_repo::FsFlowRepo;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rocket-infra fs_flow_repo -j4`
Expected: PASS — 9 tests.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra Cargo.toml
git commit -m "feat(infra): add FsFlowRepo"
```

---

## Task 2: Tagged-enum YAML round-trip coverage

**Files:**
- Modify: `crates/rocket-infra/src/fs_flow_repo.rs`

**Interfaces:**
- Consumes: `FlowNodeKind` (`Request`/`Input`/`Output` variants),
  `RequestSource` (`Saved`/`Inline` variants), `InlineRequestData`,
  `InlineHeader` from `rocket-flow` (Plan 01).
- Produces: confidence that every `FlowNodeKind`/`RequestSource` variant
  round-trips through `FsFlowRepo::save`/`get` — the gate before Plan 05/06
  build on these types for real request execution.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-infra/src/fs_flow_repo.rs (add to the existing tests module)
use rocket_flow::{InlineHeader, InlineRequestData};

fn repo_pair() -> (TempDir, FsFlowRepo) {
    setup()
}

#[test]
fn request_node_with_saved_source_roundtrips() {
    let (_dir, repo) = setup();
    let flow = Flow {
        name: "Saved Source Flow".to_string(),
        nodes: vec![FlowNode {
            id: "node-1".to_string(),
            kind: FlowNodeKind::Request {
                label: "Get User".to_string(),
                source: RequestSource::Saved {
                    request_path: "users/get-user.yml".to_string(),
                },
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }],
        edges: Vec::new(),
    };
    repo.save("acme", &flow).expect("save");
    let loaded = repo.get("acme", "Saved Source Flow").expect("get");
    assert_eq!(loaded, flow);
}

#[test]
fn request_node_with_inline_source_roundtrips() {
    let (_dir, repo) = repo_pair();
    let flow = Flow {
        name: "Inline Source Flow".to_string(),
        nodes: vec![FlowNode {
            id: "node-1".to_string(),
            kind: FlowNodeKind::Request {
                label: "Ad Hoc Login".to_string(),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "POST".to_string(),
                        url: "https://api.example.com/login".to_string(),
                        headers: vec![InlineHeader {
                            name: "Content-Type".to_string(),
                            value: "application/json".to_string(),
                        }],
                        body: Some("{\"user\":\"{{u}}\"}".to_string()),
                    },
                },
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }],
        edges: Vec::new(),
    };
    repo.save("acme", &flow).expect("save");
    let loaded = repo.get("acme", "Inline Source Flow").expect("get");
    assert_eq!(loaded, flow);
}

#[test]
fn inline_request_with_no_body_roundtrips_as_none() {
    let (_dir, repo) = repo_pair();
    let flow = Flow {
        name: "No Body Flow".to_string(),
        nodes: vec![FlowNode {
            id: "node-1".to_string(),
            kind: FlowNodeKind::Request {
                label: "Ping".to_string(),
                source: RequestSource::Inline {
                    request: InlineRequestData {
                        method: "GET".to_string(),
                        url: "https://api.example.com/ping".to_string(),
                        headers: Vec::new(),
                        body: None,
                    },
                },
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }],
        edges: Vec::new(),
    };
    repo.save("acme", &flow).expect("save");
    let loaded = repo.get("acme", "No Body Flow").expect("get");
    match &loaded.nodes[0].kind {
        FlowNodeKind::Request { source: RequestSource::Inline { request }, .. } => {
            assert_eq!(request.body, None);
            assert!(request.headers.is_empty());
        }
        other => panic!("expected an Inline Request node, got {other:?}"),
    }
}

#[test]
fn all_three_node_kinds_in_one_flow_roundtrip_together() {
    let (_dir, repo) = repo_pair();
    let mut flow = sample("Mixed Kinds Flow");
    flow.nodes.push(FlowNode {
        id: "node-2".to_string(),
        kind: FlowNodeKind::Request {
            label: "Call".to_string(),
            source: RequestSource::Saved { request_path: "call.yml".to_string() },
        },
        position: NodePosition { x: 200.0, y: 0.0 },
    });
    flow.nodes.push(FlowNode {
        id: "node-3".to_string(),
        kind: FlowNodeKind::Output { label: "Result".to_string() },
        position: NodePosition { x: 400.0, y: 0.0 },
    });
    repo.save("acme", &flow).expect("save");
    let loaded = repo.get("acme", "Mixed Kinds Flow").expect("get");
    assert_eq!(loaded.nodes.len(), 3);
    assert_eq!(loaded, flow);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra fs_flow_repo -j4`
Expected: FAIL only if `rocket-flow`'s enums are not tagged the way Task 1
assumed (e.g. missing `#[serde(tag = "kind")]`/`#[serde(tag = "type")]`) —
otherwise these should already pass once Task 1's implementation is in
place, since no new production code is needed for this task. If they fail
for a serialization-shape reason, that is a real gap in Plan 01's output;
do not change the test's expectations to work around it — flag it in the
Post-Implementation Review below instead.

- [ ] **Step 3: Confirm the tests pass as-is, or fix a genuine `FsFlowRepo` bug if one surfaces**

Run: `cargo test -p rocket-infra fs_flow_repo -j4`
Expected: PASS — 13 tests total (9 from Task 1, 4 from this task). If a
fix in `fs_flow_repo.rs` itself was needed (not in `rocket-flow`), make it
now and re-run.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-infra
git commit -m "test(infra): cover Flow tagged-enum YAML round-trip"
```

---

## Next Plan

[Plan 04: Flow domain events](2026-09-27-flow-visual-workflow-builder-plan-04-domain-events.md) —
adds `FlowNodeStatus` and the `FlowRunStarted`/`FlowStepCompleted`/`FlowRunFinished`
`DomainEvent` variants in `rocket-shared`, independent of this plan's
persistence work but needed before Plan 06's `FlowExecutionService`.

## Post-Implementation Review

Before starting Plan 04, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-infra/Cargo.toml`, `crates/rocket-infra/src/lib.rs`,
> `crates/rocket-infra/src/fs_flow_repo.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — does `FsFlowRepo` implement
>    `FlowRepository` exactly as the plan index's locked interface contract
>    promises Plan 06/07 will consume, including the `base_dir`
>    (collections-root) constructor convention (not a per-collection
>    directory, unlike `FsEnvironmentRepo`)?
> 2. Code quality and test coverage versus this plan's Review Focus section
>    (empty `flows/` dir, replace-not-duplicate on save, `NotFound` on a
>    missing get/delete, slugification stability, malformed-YAML error
>    path).
> 3. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    all I/O stays in `rocket-infra`, every write goes through
>    `atomic_write`, no raw `std::fs::write` calls, no camelCase serde on
>    the persisted `Flow`/`FlowNode`/`FlowEdge` types.
> 4. Confirm `flows/` is never referenced from any OpenCollection-schema
>    file (`opencollection.yml`, `folder.yml`) this plan touches — it
>    shouldn't touch either at all; flag it if it does.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-infra fs_flow_repo -j4` and
> `cargo check -p rocket-infra -j4`, and confirm they still pass. Report
> what you found and fixed.

Only proceed to Plan 04 once this review comes back clean (or its fixes are
applied and re-verified).
