# Flow Plan 07: Tauri Commands — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose Flow CRUD and execution over Tauri IPC (`list_flows`,
`get_flow`, `save_flow`, `delete_flow`, `run_flow`, `cancel_flow_run`) and
wire the new services into `src-tauri/src/lib.rs`.

**Architecture:** A new `crates/rocket-app/src/flow_service.rs` (`FlowService`)
handles CRUD + save-time cycle validation, mirroring how `CollectionService`
(CRUD) and `CollectionRunnerService` (run orchestration) are two separate
`rocket-app` services for the same domain rather than one service doing both
jobs. A new `src-tauri/src/commands/flow.rs` wraps both `FlowService` and
Plan 06's `FlowExecutionService` behind thin commands with a full camelCase
DTO tree — not an opaque JSON passthrough (see Global Constraints, deviation
from the plan index's sketch).

**Tech Stack:** Rust, Tauri 2 commands, serde.

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§8 Tauri IPC, §10 error handling). Plan index:
`docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md` (has
the full locked interface contract every plan in this series depends on).

## Global Constraints

- **Corrected during Plan 01's post-implementation review (Opus):**
  `RequestSourceDto` and `FlowNodeKindDto` (Task 1, below) use
  `#[serde(tag = "...", rename_all_fields = "camelCase")]`, **not**
  `#[serde(tag = "...", rename_all = "camelCase")]`. On a `#[serde(tag = ...)]`
  enum, plain `rename_all` renames the variant tags themselves (e.g. `Saved`
  → `saved`), which would both break the TS contract's `type: 'Saved'` value
  and leave nested fields like `request_path` untouched (still snake_case) —
  the opposite of what's needed. `rename_all_fields` (stable since serde
  1.0.127; this workspace is on 1.0.228) renames only the fields *inside*
  each variant, leaving the tag values as-written (`Saved`/`Request`/etc.),
  which is what makes this plan's own
  `flow_dto_serializes_camelcase_including_nested_fields` test (asserting
  `"requestPath"` while the TS union still discriminates on `'Saved'`)
  actually pass. The code block below already reflects this fix — do not
  revert to plain `rename_all` on these two enums.
- **`FlowService` is a new addition beyond the plan index's original
  contract** — the index only specified `FlowExecutionService` (Plan 06).
  Tauri's IPC boundary rule (`.claude/rules/tauri-ipc-boundaries.md`)
  requires commands to "route via rocket-app traits/services" and contain
  "no domain business logic in command modules" — CRUD-with-cycle-validation
  is domain logic and does not belong inline in `src-tauri/src/commands/flow.rs`.
  Once this plan is implemented, update `00-plan-index.md`'s `rocket-app`
  section to add `FlowService`'s contract (shown in Task 1 below) so later
  readers don't mistake `FlowExecutionService` as the only `rocket-app` Flow
  type.
- **DTOs are a full mirrored tree, not `serde_json::Value` passthrough.** The
  plan index's sketch (`FlowNodeDto.kind: serde_json::Value`) would leave
  nested fields like `RequestSource::Saved.request_path` serialized as
  snake_case JSON reaching the frontend — violating this repo's hard rule
  that camelCase is IPC-DTO-only, applied at every level a domain type
  crosses the IPC boundary, not just the outermost struct. This plan defines
  `FlowNodeKindDto`/`RequestSourceDto`/`InlineRequestDataDto`/`InlineHeaderDto`/`NodePositionDto`,
  each carrying `#[serde(rename_all = "camelCase")]`, converted via `From`
  both directions — the same pattern `AgentConfigDto` uses
  (`docs/superpowers/plans/acp-agent-config-credentials/2026-09-27-acp-agent-config-credentials-plan-04-tauri-commands.md`
  Task 1), just with more nesting. `rocket_shared::VariableValue` (used
  verbatim inside `FlowNodeKindDto::Input`) is the one exception needing no
  wrapper DTO — its own fields (`type`/`data`) are already single lowercase
  words, identical under snake_case and camelCase, and it is already used
  directly in other IPC DTOs elsewhere in this codebase.
- **Every `FsFlowRepo` use in this plan must go through a new
  `SharedPathFlowRepo` wrapper, never a path-pinned `FsFlowRepo` directly —
  resolved during Plan 03's post-implementation review (Opus), which found
  this gap.** `src-tauri/src/lib.rs:326-336` constructs `CollectionRunnerService`
  with `Box::new(SharedPathCollectionRepo::new(Arc::clone(&active_workspace_path)))`
  specifically because, per that file's own comment, "the run set... must
  follow workspace switches the same way `collection_svc`'s sidebar reads
  do, not read whatever workspace was active at process startup." Flow CRUD
  (`FlowService`, Task 1) and Flow execution (`FlowExecutionService`, Task 3)
  are both surfaces a user can invoke after switching workspaces mid-session,
  so both need the identical treatment — a path-pinned `FsFlowRepo` (like
  `FsCollectionRepo::new_standalone` used for `exec_svc`/`oauth2_svc` at
  `src-tauri/src/lib.rs:301,323`) would silently keep reading/writing the
  workspace active at process startup after a switch. Task 1 below creates
  `crates/rocket-infra/src/shared_path_flow_repo.rs` (`SharedPathFlowRepo`,
  mirroring `SharedPathCollectionRepo` exactly — an `Arc<Mutex<PathBuf>>`
  resolved to a fresh `FsFlowRepo` per call) before wiring `FlowService`, and
  Task 3 uses the same wrapper for `FlowExecutionService`'s `flow_repo` field.
- Production command and service code never panics on a fallible call (no
  bare panicking shorthand on a `Result`/`Option`) — always propagate via
  `DomainResult`/`?`. Test code may use `.expect("message")`, matching this
  repo's existing test convention.
- Command modules stay thin: validate input shape, call the service, map
  output/error — no domain logic (cycle validation, DTO↔domain mapping
  beyond field renaming) inline in `src-tauri/src/commands/flow.rs` itself
  beyond what a `From` impl does.
- The `src-tauri` package name is `rocket` (`src-tauri/Cargo.toml:2`) — every
  verification command in this plan uses `cargo check -p rocket -j4` /
  `cargo test -p rocket -j4`, not `rocket-tauri` or `rocket_lib`.
- Task 2 (`save_flow`) touches collection-adjacent persistence — 📖 Before
  starting Task 2, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Review Focus

- `save_flow` on a cyclic graph is rejected with the specific node ids named
  in the error message (not a generic "cycle detected"), and nothing is
  written to disk — verified with a fake `FlowRepository` whose `save`
  panics if called, so the test fails loudly if validation is bypassed.
- `get_flow`/`delete_flow` for a flow name that doesn't exist return
  `DomainError::NotFound` intact to the frontend, not a generic/opaque
  error swallowed by the command layer.
- `run_flow` is a plain `async` command that runs to completion and returns
  the full `FlowRunSummary` (matching `run_collection`'s existing pattern in
  `runner.rs` — it does **not** return early with just a `run_id`; the
  frontend learns the `run_id` from the `FlowRunStarted` event, published
  before the command's own `await` resolves, exactly like
  `RunnerStarted` today). A test must confirm `run_id` is populated in the
  event payload before the command call resolves, not only in the final
  summary.
- `cancel_flow_run` with an unrecognized or already-finished run id is a
  no-op that returns `Ok(())`, not an error — matches
  `FlowExecutionService::cancel`'s semantics (Plan 06) and
  `stop_collection_run`'s existing behavior in `runner.rs`.
- Registering the new commands in `tauri::generate_handler!` without also
  adding `pub mod flow;` to `src-tauri/src/commands/mod.rs` is a common
  one-line omission that fails compilation — confirmed present.

---

## Task 1: `FlowService` (CRUD) + DTOs + `list_flows`/`get_flow`/`delete_flow`

**Files:**
- Create: `crates/rocket-app/src/flow_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`
- Create: `src-tauri/src/commands/flow.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `rocket_flow::{Flow, FlowNode, FlowNodeKind, RequestSource, InlineRequestData, InlineHeader, NodePosition, FlowEdge, FlowRepository}` (Plan 01).
- Produces: `FlowService { list, get, delete }` (this task) and `save` (Task 2) — consumed by the Tauri commands in this file. `FlowDto`/`FlowNodeDto`/`FlowNodeKindDto`/`RequestSourceDto`/`InlineRequestDataDto`/`InlineHeaderDto`/`NodePositionDto`/`FlowEdgeDto` — consumed by every later task in this plan and by Plan 08's frontend bindings (field-for-field, once camelCase-rendered).

- [ ] **Step 1: Write the failing tests for `FlowService`**

```rust
// crates/rocket-app/src/flow_service.rs
use rocket_flow::{Flow, FlowEdge, FlowNode, FlowNodeKind, FlowRepository, NodePosition, RequestSource};
use rocket_shared::error::DomainResult;

pub struct FlowService {
    flow_repo: Box<dyn FlowRepository>,
}

impl FlowService {
    pub fn new(flow_repo: Box<dyn FlowRepository>) -> Self {
        Self { flow_repo }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn sample_flow() -> Flow {
        Flow {
            name: "Login Then Fetch".to_string(),
            nodes: vec![FlowNode {
                id: "n1".to_string(),
                kind: FlowNodeKind::Output {
                    label: "Result".to_string(),
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: vec![],
        }
    }

    struct FakeFlowRepo {
        flows: Mutex<Vec<(String, Flow)>>, // (collection, flow)
    }
    impl FakeFlowRepo {
        fn new() -> Self {
            Self {
                flows: Mutex::new(Vec::new()),
            }
        }
        fn seeded(collection: &str, flow: Flow) -> Self {
            let repo = Self::new();
            repo.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .push((collection.to_string(), flow));
            repo
        }
    }
    impl FlowRepository for FakeFlowRepo {
        fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
            Ok(self
                .flows
                .lock()
                .expect("lock FakeFlowRepo")
                .iter()
                .filter(|(c, _)| c == collection)
                .map(|(_, f)| f.name.clone())
                .collect())
        }
        fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .iter()
                .find(|(c, f)| c == collection && f.name == name)
                .map(|(_, f)| f.clone())
                .ok_or_else(|| {
                    rocket_shared::error::DomainError::NotFound(format!(
                        "flow '{name}' not found in collection '{collection}'"
                    ))
                })
        }
        fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
            let mut guard = self.flows.lock().expect("lock FakeFlowRepo");
            guard.retain(|(c, f)| !(c == collection && f.name == flow.name));
            guard.push((collection.to_string(), flow.clone()));
            Ok(())
        }
        fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
            self.flows
                .lock()
                .expect("lock FakeFlowRepo")
                .retain(|(c, f)| !(c == collection && f.name == name));
            Ok(())
        }
    }

    #[test]
    fn list_returns_flow_names_for_collection() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::seeded("demo", sample_flow())));
        assert_eq!(
            svc.list("demo").expect("list"),
            vec!["Login Then Fetch".to_string()]
        );
        assert_eq!(svc.list("other").expect("list other"), Vec::<String>::new());
    }

    #[test]
    fn get_returns_not_found_for_missing_flow() {
        let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
        let err = svc.get("demo", "missing").expect_err("expected NotFound");
        assert!(matches!(err, rocket_shared::error::DomainError::NotFound(_)));
    }

    #[test]
    fn delete_removes_flow() {
        let repo = FakeFlowRepo::seeded("demo", sample_flow());
        let svc = FlowService::new(Box::new(repo));
        svc.delete("demo", "Login Then Fetch").expect("delete");
        assert!(svc.get("demo", "Login Then Fetch").is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app flow_service::tests -j4`
Expected: FAIL — `FlowService` has no `list`/`get`/`delete` methods yet
(compile error).

- [ ] **Step 3: Implement `list`/`get`/`delete`**

```rust
// crates/rocket-app/src/flow_service.rs (add to the impl block above the tests module)
impl FlowService {
    pub fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        self.flow_repo.list(collection)
    }

    pub fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        self.flow_repo.get(collection, name)
    }

    pub fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        self.flow_repo.delete(collection, name)
    }
}
```

- [ ] **Step 4: Register the module**

In `crates/rocket-app/src/lib.rs`, add alongside the existing `pub mod
execution_service;` declaration:

```rust
pub mod flow_service;
```

And alongside `pub use execution_service::{...};`:

```rust
pub use flow_service::FlowService;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-app flow_service::tests -j4`
Expected: PASS — 3 tests.

- [ ] **Step 6: Write the DTOs and read-only commands**

```rust
// src-tauri/src/commands/flow.rs
use rocket_app::FlowService;
use rocket_flow::{
    Flow, FlowEdge, FlowNode, FlowNodeKind, InlineHeader, InlineRequestData, NodePosition,
    RequestSource,
};
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePositionDto {
    pub x: f64,
    pub y: f64,
}
impl From<NodePosition> for NodePositionDto {
    fn from(p: NodePosition) -> Self {
        Self { x: p.x, y: p.y }
    }
}
impl From<NodePositionDto> for NodePosition {
    fn from(p: NodePositionDto) -> Self {
        Self { x: p.x, y: p.y }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineHeaderDto {
    pub name: String,
    pub value: String,
}
impl From<InlineHeader> for InlineHeaderDto {
    fn from(h: InlineHeader) -> Self {
        Self {
            name: h.name,
            value: h.value,
        }
    }
}
impl From<InlineHeaderDto> for InlineHeader {
    fn from(h: InlineHeaderDto) -> Self {
        Self {
            name: h.name,
            value: h.value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineRequestDataDto {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<InlineHeaderDto>,
    #[serde(default)]
    pub body: Option<String>,
}
impl From<InlineRequestData> for InlineRequestDataDto {
    fn from(r: InlineRequestData) -> Self {
        Self {
            method: r.method,
            url: r.url,
            headers: r.headers.into_iter().map(Into::into).collect(),
            body: r.body,
        }
    }
}
impl From<InlineRequestDataDto> for InlineRequestData {
    fn from(r: InlineRequestDataDto) -> Self {
        Self {
            method: r.method,
            url: r.url,
            headers: r.headers.into_iter().map(Into::into).collect(),
            body: r.body,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum RequestSourceDto {
    Saved { request_path: String },
    Inline { request: InlineRequestDataDto },
}
impl From<RequestSource> for RequestSourceDto {
    fn from(s: RequestSource) -> Self {
        match s {
            RequestSource::Saved { request_path } => RequestSourceDto::Saved { request_path },
            RequestSource::Inline { request } => RequestSourceDto::Inline {
                request: request.into(),
            },
        }
    }
}
impl From<RequestSourceDto> for RequestSource {
    fn from(s: RequestSourceDto) -> Self {
        match s {
            RequestSourceDto::Saved { request_path } => RequestSource::Saved { request_path },
            RequestSourceDto::Inline { request } => RequestSource::Inline {
                request: request.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum FlowNodeKindDto {
    Request {
        label: String,
        source: RequestSourceDto,
    },
    Input {
        label: String,
        value: rocket_shared::VariableValue,
    },
    Output {
        label: String,
    },
}
impl From<FlowNodeKind> for FlowNodeKindDto {
    fn from(k: FlowNodeKind) -> Self {
        match k {
            FlowNodeKind::Request { label, source } => FlowNodeKindDto::Request {
                label,
                source: source.into(),
            },
            FlowNodeKind::Input { label, value } => FlowNodeKindDto::Input { label, value },
            FlowNodeKind::Output { label } => FlowNodeKindDto::Output { label },
        }
    }
}
impl From<FlowNodeKindDto> for FlowNodeKind {
    fn from(k: FlowNodeKindDto) -> Self {
        match k {
            FlowNodeKindDto::Request { label, source } => FlowNodeKind::Request {
                label,
                source: source.into(),
            },
            FlowNodeKindDto::Input { label, value } => FlowNodeKind::Input { label, value },
            FlowNodeKindDto::Output { label } => FlowNodeKind::Output { label },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNodeDto {
    pub id: String,
    pub kind: FlowNodeKindDto,
    pub position: NodePositionDto,
}
impl From<FlowNode> for FlowNodeDto {
    fn from(n: FlowNode) -> Self {
        Self {
            id: n.id,
            kind: n.kind.into(),
            position: n.position.into(),
        }
    }
}
impl From<FlowNodeDto> for FlowNode {
    fn from(n: FlowNodeDto) -> Self {
        Self {
            id: n.id,
            kind: n.kind.into(),
            position: n.position.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEdgeDto {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub target_field: String,
    pub expression: String,
}
impl From<FlowEdge> for FlowEdgeDto {
    fn from(e: FlowEdge) -> Self {
        Self {
            id: e.id,
            source_node_id: e.source_node_id,
            target_node_id: e.target_node_id,
            target_field: e.target_field,
            expression: e.expression,
        }
    }
}
impl From<FlowEdgeDto> for FlowEdge {
    fn from(e: FlowEdgeDto) -> Self {
        Self {
            id: e.id,
            source_node_id: e.source_node_id,
            target_node_id: e.target_node_id,
            target_field: e.target_field,
            expression: e.expression,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDto {
    pub name: String,
    pub nodes: Vec<FlowNodeDto>,
    pub edges: Vec<FlowEdgeDto>,
}
impl From<Flow> for FlowDto {
    fn from(f: Flow) -> Self {
        Self {
            name: f.name,
            nodes: f.nodes.into_iter().map(Into::into).collect(),
            edges: f.edges.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<FlowDto> for Flow {
    fn from(f: FlowDto) -> Self {
        Self {
            name: f.name,
            nodes: f.nodes.into_iter().map(Into::into).collect(),
            edges: f.edges.into_iter().map(Into::into).collect(),
        }
    }
}

#[tauri::command]
pub fn list_flows(
    collection: String,
    svc: State<'_, FlowService>,
) -> Result<Vec<String>, DomainError> {
    svc.list(&collection)
}

#[tauri::command]
pub fn get_flow(
    collection: String,
    name: String,
    svc: State<'_, FlowService>,
) -> Result<FlowDto, DomainError> {
    svc.get(&collection, &name).map(FlowDto::from)
}

#[tauri::command]
pub fn delete_flow(
    collection: String,
    name: String,
    svc: State<'_, FlowService>,
) -> Result<(), DomainError> {
    svc.delete(&collection, &name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dto() -> FlowDto {
        FlowDto {
            name: "Login Then Fetch".to_string(),
            nodes: vec![
                FlowNodeDto {
                    id: "n1".to_string(),
                    kind: FlowNodeKindDto::Request {
                        label: "Login".to_string(),
                        source: RequestSourceDto::Saved {
                            request_path: "auth/login.yml".to_string(),
                        },
                    },
                    position: NodePositionDto { x: 0.0, y: 0.0 },
                },
                FlowNodeDto {
                    id: "n2".to_string(),
                    kind: FlowNodeKindDto::Output {
                        label: "Result".to_string(),
                    },
                    position: NodePositionDto { x: 200.0, y: 0.0 },
                },
            ],
            edges: vec![FlowEdgeDto {
                id: "e1".to_string(),
                source_node_id: "n1".to_string(),
                target_node_id: "n2".to_string(),
                target_field: "body".to_string(),
                expression: "response.body".to_string(),
            }],
        }
    }

    #[test]
    fn flow_dto_serializes_camelcase_including_nested_fields() {
        let json = serde_json::to_string(&sample_dto()).expect("serialize FlowDto");
        assert!(
            json.contains("\"requestPath\""),
            "nested RequestSource field must be camelCase, got: {json}"
        );
        assert!(
            json.contains("\"sourceNodeId\""),
            "FlowEdgeDto field must be camelCase, got: {json}"
        );
        assert!(
            json.contains("\"targetField\""),
            "FlowEdgeDto field must be camelCase, got: {json}"
        );
    }

    #[test]
    fn flow_dto_roundtrips_through_domain_type() {
        let dto = sample_dto();
        let domain: Flow = dto.clone().into();
        let back: FlowDto = domain.into();
        let a = serde_json::to_value(&dto).expect("to_value dto");
        let b = serde_json::to_value(&back).expect("to_value back");
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 7: Register the command module**

In `src-tauri/src/commands/mod.rs`, add alongside the existing `pub mod
runner;` declaration:

```rust
pub mod flow;
```

- [ ] **Step 8: Create `SharedPathFlowRepo` and wire `FlowService` into `lib.rs`**

**Corrected during Plan 03's post-implementation review (Opus):** `FsFlowRepo`
must **not** be pinned to the collections directory at process startup the
way `exec_svc`/`oauth2_svc` pin their collection repos — a user can switch
workspaces mid-session, and Flow CRUD (this task) and Flow execution
(Task 3) both need to follow that switch, exactly like `collection_svc`'s
sidebar reads and `CollectionRunnerService`'s `collection_repo` already do
via `SharedPathCollectionRepo`
(`crates/rocket-infra/src/shared_path_collection_repo.rs`). Create the
equivalent wrapper for `FsFlowRepo` first:

Create `crates/rocket-infra/src/shared_path_flow_repo.rs`:

```rust
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_flow::{Flow, FlowRepository};
use rocket_shared::error::DomainResult;

use crate::FsFlowRepo;

/// Delegates all `FlowRepository` operations to a short-lived `FsFlowRepo`
/// whose base directory is resolved from a shared, mutable workspace path
/// at call time — mirrors `SharedPathCollectionRepo` exactly, so Flow CRUD
/// and Flow execution follow workspace switches without rebuilding the
/// Tauri service graph.
pub struct SharedPathFlowRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl SharedPathFlowRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    fn repo(&self) -> FsFlowRepo {
        let base = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("collections");
        FsFlowRepo::new(base)
    }
}

impl FlowRepository for SharedPathFlowRepo {
    fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        self.repo().list(collection)
    }

    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        self.repo().get(collection, name)
    }

    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
        self.repo().save(collection, flow)
    }

    fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        self.repo().delete(collection, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample(name: &str) -> Flow {
        Flow {
            name: name.to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    #[test]
    fn switching_workspace_path_redirects_subsequent_calls() {
        let dir_a = TempDir::new().expect("temp dir a");
        let dir_b = TempDir::new().expect("temp dir b");
        std::fs::create_dir_all(dir_a.path().join("collections").join("acme"))
            .expect("create collection a");
        std::fs::create_dir_all(dir_b.path().join("collections").join("acme"))
            .expect("create collection b");

        let shared_path = Arc::new(Mutex::new(dir_a.path().to_path_buf()));
        let repo = SharedPathFlowRepo::new(Arc::clone(&shared_path));

        repo.save("acme", &sample("A-side Flow")).expect("save in workspace a");
        assert_eq!(repo.list("acme").expect("list a").len(), 1);

        *shared_path.lock().expect("lock shared path") = dir_b.path().to_path_buf();

        assert!(
            repo.list("acme").expect("list b").is_empty(),
            "after workspace switch, list() should show workspace B's flows"
        );
    }
}
```

Add `pub mod shared_path_flow_repo; pub use shared_path_flow_repo::SharedPathFlowRepo;`
to `crates/rocket-infra/src/lib.rs`, alongside the existing
`shared_path_collection_repo` module declaration.

Run: `cargo test -p rocket-infra shared_path_flow_repo -j4`
Expected: PASS — 1 test.

Then, in `src-tauri/src/lib.rs`, immediately after the existing `oauth2_svc`
construction (after its closing `);` — see the block ending around line
324), add:

```rust
// Flow CRUD and Flow execution both need to follow workspace switches, the
// same reasoning CollectionRunnerService's collection_repo already follows
// — see SharedPathFlowRepo's doc comment.
let flow_svc = rocket_app::FlowService::new(Box::new(
    rocket_infra::SharedPathFlowRepo::new(Arc::clone(&active_workspace_path)),
));
```

Add, alongside the existing `app.manage(oauth2_svc);`:

```rust
app.manage(flow_svc);
```

- [ ] **Step 9: Register the read-only commands**

In the `tauri::generate_handler!` list, add alongside the existing
`commands::runner::run_collection,` entry:

```rust
commands::flow::list_flows,
commands::flow::get_flow,
commands::flow::delete_flow,
```

- [ ] **Step 10: Run tests and verify the app builds**

Run: `cargo test -p rocket-app flow_service::tests -j4 && cargo test -p rocket flow::tests -j4 && cargo check -p rocket -j4`
Expected: all PASS/succeed.

- [ ] **Step 11: Commit**

```bash
git add crates/rocket-app/src/flow_service.rs crates/rocket-app/src/lib.rs src-tauri/src/commands/flow.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(flow): add FlowService and read-only Tauri commands"
```

---

## Task 2: `save_flow` with cycle validation

**Files:**
- Modify: `crates/rocket-app/src/flow_service.rs`
- Modify: `src-tauri/src/commands/flow.rs`

**Interfaces:**
- Consumes: `rocket_flow::{topological_sort, FlowGraphError}` (Plan 02).
- Produces: `FlowService::save`, the `save_flow` Tauri command — consumed by
  Plan 10's frontend Save action.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/flow_service.rs (add to the existing tests module)
use rocket_flow::{FlowEdge, RequestSource};

fn cyclic_flow() -> Flow {
    Flow {
        name: "Cyclic".to_string(),
        nodes: vec![
            FlowNode {
                id: "a".to_string(),
                kind: FlowNodeKind::Output {
                    label: "A".to_string(),
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            },
            FlowNode {
                id: "b".to_string(),
                kind: FlowNodeKind::Output {
                    label: "B".to_string(),
                },
                position: NodePosition { x: 100.0, y: 0.0 },
            },
        ],
        edges: vec![
            FlowEdge {
                id: "e1".to_string(),
                source_node_id: "a".to_string(),
                target_node_id: "b".to_string(),
                target_field: "body".to_string(),
                expression: "response.body".to_string(),
            },
            FlowEdge {
                id: "e2".to_string(),
                source_node_id: "b".to_string(),
                target_node_id: "a".to_string(),
                target_field: "body".to_string(),
                expression: "response.body".to_string(),
            },
        ],
    }
}

struct PanicsOnSaveRepo;
impl FlowRepository for PanicsOnSaveRepo {
    fn list(&self, _collection: &str) -> DomainResult<Vec<String>> {
        Ok(Vec::new())
    }
    fn get(&self, _collection: &str, name: &str) -> DomainResult<Flow> {
        Err(rocket_shared::error::DomainError::NotFound(name.to_string()))
    }
    fn save(&self, _collection: &str, _flow: &Flow) -> DomainResult<()> {
        panic!("save must not be called for a cyclic flow");
    }
    fn delete(&self, _collection: &str, _name: &str) -> DomainResult<()> {
        Ok(())
    }
}

#[test]
fn save_rejects_cyclic_graph_and_names_the_nodes() {
    let svc = FlowService::new(Box::new(PanicsOnSaveRepo));
    let err = svc
        .save("demo", cyclic_flow())
        .expect_err("cyclic flow must be rejected");
    let message = err.to_string();
    assert!(
        message.contains('a') && message.contains('b'),
        "error should name the cyclic nodes, got: {message}"
    );
}

#[test]
fn save_persists_an_acyclic_flow() {
    let svc = FlowService::new(Box::new(FakeFlowRepo::new()));
    svc.save("demo", sample_flow()).expect("save acyclic flow");
    assert_eq!(
        svc.list("demo").expect("list"),
        vec!["Login Then Fetch".to_string()]
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app flow_service::tests -j4`
Expected: FAIL — `FlowService` has no `save` method yet (compile error).

- [ ] **Step 3: Implement `save`**

```rust
// crates/rocket-app/src/flow_service.rs (add to the impl block)
use rocket_flow::{topological_sort, FlowGraphError};

impl FlowService {
    pub fn save(&self, collection: &str, flow: Flow) -> DomainResult<()> {
        topological_sort(&flow).map_err(|e| match e {
            FlowGraphError::Cycle { node_ids } => rocket_shared::error::DomainError::InvalidInput(
                format!("flow contains a cycle through node(s): {}", node_ids.join(", ")),
            ),
            FlowGraphError::UnknownNode { node_id } => {
                rocket_shared::error::DomainError::InvalidInput(format!(
                    "edge references unknown node: {node_id}"
                ))
            }
            FlowGraphError::DuplicateNode { node_id } => {
                rocket_shared::error::DomainError::InvalidInput(format!(
                    "flow has more than one node with id: {node_id}"
                ))
            }
        })?;
        self.flow_repo.save(collection, &flow)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app flow_service::tests -j4`
Expected: PASS — 5 tests total (3 from Task 1, 2 from this task).

- [ ] **Step 5: Add the `save_flow` command**

```rust
// src-tauri/src/commands/flow.rs (add below delete_flow)
#[tauri::command]
pub fn save_flow(
    collection: String,
    flow: FlowDto,
    svc: State<'_, FlowService>,
) -> Result<(), DomainError> {
    svc.save(&collection, flow.into())
}
```

In the `tauri::generate_handler!` list in `src-tauri/src/lib.rs`, add:

```rust
commands::flow::save_flow,
```

- [ ] **Step 6: Verify the app builds**

Run: `cargo check -p rocket -j4`
Expected: succeeds.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/flow_service.rs src-tauri/src/commands/flow.rs src-tauri/src/lib.rs
git commit -m "feat(flow): reject cyclic graphs in save_flow"
```

---

## Task 3: `run_flow` + `cancel_flow_run`

**Files:**
- Modify: `src-tauri/src/commands/flow.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `rocket_app::{FlowExecutionService, RunFlowInput, FlowRunSummary}` (Plan 06), `rocket_app::RequestExecutionService` (existing).
- Produces: `run_flow`, `cancel_flow_run` — consumed by Plan 10's frontend
  Run/Stop toolbar.

- [ ] **Step 1: Add the DTO for `RunFlowInput` and the two commands**

```rust
// src-tauri/src/commands/flow.rs (add below save_flow)
use rocket_app::{FlowExecutionService, FlowRunSummary, RequestExecutionService, RunFlowInput};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFlowInputDto {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
}
impl From<RunFlowInputDto> for RunFlowInput {
    fn from(i: RunFlowInputDto) -> Self {
        Self {
            collection: i.collection,
            flow_name: i.flow_name,
            environment_name: i.environment_name,
        }
    }
}

/// Runs a Flow to completion. Streams `flow-run-started`,
/// `flow-step-completed`, and `flow-run-finished` events while it runs
/// (`FlowExecutionService::run` publishes these through the injected
/// `TauriEventBus` as it goes) and returns the same data as one summary when
/// the run ends — mirroring `run_collection` in `runner.rs` exactly. The
/// frontend reads `run_id` off the `flow-run-started` event payload, not off
/// this command's return value, so Stop is available before the run finishes.
#[tauri::command]
pub async fn run_flow(
    input: RunFlowInputDto,
    flow_exec: State<'_, FlowExecutionService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<FlowRunSummary, DomainError> {
    flow_exec.run(&exec, input.into()).await
}

/// Asks an in-progress Flow run to stop. An unknown or already-finished run
/// id is a no-op, matching `stop_collection_run`'s existing behavior.
#[tauri::command]
pub fn cancel_flow_run(
    run_id: String,
    flow_exec: State<'_, FlowExecutionService>,
) -> Result<(), DomainError> {
    flow_exec.cancel(&run_id);
    Ok(())
}
```

(`FlowRunSummary` must derive `Serialize` for this command to compile as a
Tauri return type — confirm this is present on Plan 06's definition; if not,
add it there rather than wrapping it in a second DTO here, since it carries
no snake_case-vs-camelCase-sensitive persistence role, only an IPC-return
role, the same reasoning `RunSummary`/`RunStepResult` already follow for the
Collection Runner.)

- [ ] **Step 2: Wire `FlowExecutionService` into `lib.rs`**

In `src-tauri/src/lib.rs`, immediately after the existing `runner_svc`
construction (after its closing `);` around line 336), add:

```rust
// Flow execution — both repos are workspace-following (SharedPathFlowRepo,
// SharedPathCollectionRepo), matching runner_svc's own collection_repo
// exactly and for the same reason: a run must follow workspace switches,
// not read whatever workspace was active at process startup.
let flow_exec_svc = rocket_app::FlowExecutionService::new(
    Box::new(rocket_infra::SharedPathFlowRepo::new(Arc::clone(
        &active_workspace_path,
    ))),
    Box::new(SharedPathCollectionRepo::new(Arc::clone(
        &active_workspace_path,
    ))),
    Box::new(tauri_event_bus::TauriEventBus::new(app_handle.clone())),
);
```

Add, alongside the existing `app.manage(runner_svc);`:

```rust
app.manage(flow_exec_svc);
```

- [ ] **Step 3: Register the commands**

In the `tauri::generate_handler!` list, add alongside the existing
`commands::runner::stop_collection_run,` entry:

```rust
commands::flow::run_flow,
commands::flow::cancel_flow_run,
```

- [ ] **Step 4: Verify the app builds**

Run: `cargo check -p rocket -j4`
Expected: succeeds — confirms the full command set, both new services, and
the `SharedPathCollectionRepo` wiring compile together.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/flow.rs src-tauri/src/lib.rs
git commit -m "feat(flow): add run_flow and cancel_flow_run commands"
```

---

## Next Plan

[Plan 08: Frontend types, bindings, FlowTab](2026-09-27-flow-visual-workflow-builder-plan-08-frontend-types-and-tab.md) —
adds the TypeScript `Flow`/`FlowNode`/`FlowEdge` types, `tauri-api.ts`
bindings for the six commands in this plan, and the new `FlowTab` pane type.

## Post-Implementation Review

Before starting Plan 08, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-app/src/flow_service.rs`, `crates/rocket-app/src/lib.rs`,
> `src-tauri/src/commands/flow.rs`, `src-tauri/src/commands/mod.rs`,
> `src-tauri/src/lib.rs`.
>
> Check for:
> 1. Gaps versus the plan index's locked interface contract — do
>    `FlowDto`/`FlowNodeDto`/`FlowEdgeDto` and the six commands match what
>    Plan 08's frontend bindings will expect? Note that this plan added a
>    `FlowService` type and a fully-mirrored camelCase DTO tree that were
>    **not** in the original index (both are deliberate, documented
>    deviations — see this plan's Global Constraints) — update
>    `docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md`'s
>    `rocket-app` section to add `FlowService`'s contract so it's not lost
>    for anyone reading the index later.
> 2. Code quality versus this plan's Review Focus section — cyclic-graph
>    rejection names the actual node ids and writes nothing to disk;
>    `run_flow`'s behavior (blocks until done, returns the full summary,
>    `run_id` obtainable from the `flow-run-started` event before then) is
>    implemented and tested as such, not as a "returns run_id immediately"
>    shape (a wrong assumption in an earlier draft of this plan's directive —
>    confirm the delivered code does NOT do that); `cancel_flow_run` on an
>    unknown id is a no-op.
> 3. DDD/IPC boundary conformance per `.claude/rules/tauri-ipc-boundaries.md`
>    and `.claude/rules/rust-ddd-boundaries.md` — commands stay thin,
>    camelCase appears only on the DTO tree (never on `rocket_flow`'s or
>    `rocket_app`'s domain types), and both `FlowService`'s `flow_repo` and
>    `FlowExecutionService`'s `flow_repo`/`collection_repo` are genuinely
>    `SharedPathFlowRepo`/`SharedPathCollectionRepo` (never a path-pinned
>    `FsFlowRepo`/`FsCollectionRepo::new_standalone` that would silently
>    ignore workspace switches).
>
> You have explicit authority to apply fixes directly for anything you find,
> including updating the plan index file. After fixing, re-run
> `cargo test -p rocket-app flow_service::tests -j4`,
> `cargo test -p rocket flow::tests -j4`, and `cargo check -p rocket -j4`,
> and confirm they still pass. Report what you found and fixed.

Only proceed to Plan 08 once this review comes back clean (or its fixes are
applied and re-verified).
