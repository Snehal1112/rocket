# Flow Plan 06: FlowExecutionService — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `FlowExecutionService` — the orchestrator that loads a `Flow`,
walks it in topological order, dispatches each node (Input capture / Request
execute / Output capture) using Plan 05's building blocks, and reports
progress via domain events, with failed nodes cascading a `Skipped` status to
whatever depends on them.

**Architecture:** Mirrors `CollectionRunnerService`'s proven outer shell
(`Ulid` run id, secrets resolved once, cancellation bookkeeping, start/step/
finish events) but does **not** share a base type with it — the traversal is
genuinely different (topological DAG vs. tree-order cursor with jump
support), so per spec §7 this is deliberate, reviewed-and-accepted
duplication of a small amount of scaffolding, not a gap.

**Tech Stack:** Rust, `ulid`, `tokio` (async tests) — no new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§7 Execution). Plan index: `docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md`.
Depends on Plan 01 (`rocket-flow` types), Plan 02 (`topological_sort`), Plan 03
(`FsFlowRepo` — not directly used here, but `FlowRepository` must be
implemented by the time this runs end-to-end), Plan 04 (Flow domain events),
Plan 05 (`CapturedOutput`, `resolve_flow_wire_expression`,
`build_execute_request_input`, `apply_wired_overrides`).

## Corrections to the plan index (read before starting)

1. **No `MAX_RUN_STEPS`-equivalent guard is needed, unlike `CollectionRunnerService`.**
   The Collection Runner needs one because `rok.runner.setNextRequest` can
   jump backward and form an infinite loop. `FlowExecutionService` has no
   jump mechanism in Phase 1 — it executes the fixed list `topological_sort`
   returns exactly once, and `topological_sort` already rejects a cyclic
   graph outright (Plan 02). There is nothing here that could loop forever,
   so no step-count cap is added. The index's line suggesting one is wrong;
   do not add one.
2. **Input-node `{{variable}}` resolution is scoped to collection-level
   variables only in Phase 1**, not full environment/folder/request scope
   resolution. `FlowExecutionService`'s constructor (per the index) holds
   only `flow_repo`, `collection_repo`, and `events` — no
   `EnvironmentRepository`. Reaching the "regular" (per-collection)
   environment repo the way `RequestExecutionService` does requires its
   `collection_env_repo_factory` machinery, which is private to that service
   and not worth re-plumbing into a second service just for this. Task 2
   below resolves an Input node's value against `collection_repo.get_settings(collection).variables`
   only, the same scope `RequestExecutionService::evaluate_var_expression`
   already uses for its own preview tool. This satisfies the spec's Goal
   ("{{variable}}-resolvable through the existing environment/collection
   variable resolution" — a general capability, not a scope guarantee) without
   adding a constructor parameter. Full environment-scope support for Input
   nodes is a reasonable Phase 2 follow-up, not a gap in this plan.

## Global Constraints

- This plan **modifies** `crates/rocket-app/src/flow_execution_service.rs`
  (created by Plan 05) and **modifies** `crates/rocket-flow/src/graph.rs`
  (created by Plan 02, to add `reachable_from` — see Task 3).
- Never a panicking `unwrap` or bare `expect` call outside test code.
- A node's failure definition reuses `HttpResponse::is_success()` (already
  public on `rocket_http::HttpResponse`, `status` in `200..300`) — do not
  reimplement a status-range check.
- Every mutating/state-reporting operation still publishes a domain event —
  this plan's whole job is publishing `FlowRunStarted`/`FlowStepCompleted`/
  `FlowRunFinished`, so this constraint is inherent to every task here, not
  an afterthought.

## Review Focus

- A run against a flow with zero nodes completes immediately: `FlowRunStarted`
  then `FlowRunFinished` with an empty `steps` list, no error.
- A linear chain where the middle node fails: its sole downstream dependent
  is `Skipped`, and an unrelated independent node (no path from the failed
  one) still executes and reports its own real result — not swept into the
  skip set by an overly broad "skip everything after index N" shortcut.
- Cancelling mid-run leaves already-completed nodes' results intact in the
  final `FlowRunSummary` and executes no further nodes — checked with the
  same `Arc<Mutex<HashSet<String>>>` pattern `CollectionRunnerService` uses,
  not a new cancellation primitive.
- A node's own wiring-expression failure (Plan 05's `resolve_flow_wire_expression`
  returning `Err`) is treated exactly like an HTTP failure for cascade
  purposes — its dependents are `Skipped` too, not just its own status.
- `FlowRunFinished.failed_count`/`skipped_count` are exact counts, not just
  "greater than zero" — a test must assert the precise numbers on a
  multi-node run with a mix of success/failure/skip.

---

## Task 1: Service shell + `load_ordered_nodes` + cancellation

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Consumes: `rocket_flow::{Flow, FlowRepository, topological_sort, FlowGraphError}` (Plans 01-02).
- Produces: `FlowExecutionService::new`, `cancel` — consumed by Plan 07's Tauri commands; a private `load_ordered_nodes` helper — consumed by Task 2 of this plan.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/flow_execution_service.rs (add to the existing tests module)

use rocket_flow::{Flow, FlowRepository};

struct FakeFlowRepository {
    flows: std::sync::Mutex<HashMap<(String, String), Flow>>,
}
impl FakeFlowRepository {
    fn new() -> Self {
        Self {
            flows: std::sync::Mutex::new(HashMap::new()),
        }
    }
    fn with_flow(self, collection: &str, flow: Flow) -> Self {
        self.flows
            .lock()
            .expect("lock FakeFlowRepository")
            .insert((collection.to_string(), flow.name.clone()), flow);
        self
    }
}
impl FlowRepository for FakeFlowRepository {
    fn list(&self, collection: &str) -> DomainResult<Vec<String>> {
        Ok(self
            .flows
            .lock()
            .expect("lock FakeFlowRepository")
            .keys()
            .filter(|(c, _)| c == collection)
            .map(|(_, name)| name.clone())
            .collect())
    }
    fn get(&self, collection: &str, name: &str) -> DomainResult<Flow> {
        self.flows
            .lock()
            .expect("lock FakeFlowRepository")
            .get(&(collection.to_string(), name.to_string()))
            .cloned()
            .ok_or_else(|| DomainError::NotFound(format!("{collection}/{name}")))
    }
    fn save(&self, collection: &str, flow: &Flow) -> DomainResult<()> {
        self.flows
            .lock()
            .expect("lock FakeFlowRepository")
            .insert((collection.to_string(), flow.name.clone()), flow.clone());
        Ok(())
    }
    fn delete(&self, collection: &str, name: &str) -> DomainResult<()> {
        self.flows
            .lock()
            .expect("lock FakeFlowRepository")
            .remove(&(collection.to_string(), name.to_string()));
        Ok(())
    }
}

fn service_with_flow(flow: Flow) -> FlowExecutionService {
    FlowExecutionService::new(
        Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
        Box::new(FakeCollectionRepo::new()),
        Box::new(NullEventPublisher),
    )
}

fn linear_flow() -> Flow {
    Flow {
        name: "auth-flow".to_string(),
        nodes: vec![
            FlowNode {
                id: "a".to_string(),
                kind: FlowNodeKind::Input {
                    label: "Username".to_string(),
                    value: VariableValue::simple("bob"),
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            },
            FlowNode {
                id: "b".to_string(),
                kind: FlowNodeKind::Output {
                    label: "Result".to_string(),
                },
                position: NodePosition { x: 100.0, y: 0.0 },
            },
        ],
        edges: vec![FlowEdge {
            id: "e1".to_string(),
            source_node_id: "a".to_string(),
            target_node_id: "b".to_string(),
            target_field: "value".to_string(),
            expression: "response.body".to_string(),
        }],
    }
}

fn cyclic_flow() -> Flow {
    Flow {
        name: "cyclic".to_string(),
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
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
            },
            FlowEdge {
                id: "e2".to_string(),
                source_node_id: "b".to_string(),
                target_node_id: "a".to_string(),
                target_field: "value".to_string(),
                expression: "response.body".to_string(),
            },
        ],
    }
}

#[test]
fn load_ordered_nodes_returns_dependency_order_for_a_valid_flow() {
    let service = service_with_flow(linear_flow());
    let (flow, order) = service
        .load_ordered_nodes("my-api", "auth-flow")
        .expect("valid flow must load and sort");
    assert_eq!(flow.name, "auth-flow");
    assert_eq!(order, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn load_ordered_nodes_rejects_a_cyclic_flow() {
    let service = service_with_flow(cyclic_flow());
    let err = service
        .load_ordered_nodes("my-api", "cyclic")
        .expect_err("a cyclic flow must not load for execution");
    assert!(matches!(err, DomainError::InvalidInput(_)));
}

#[test]
fn load_ordered_nodes_propagates_unknown_flow_name() {
    let service = service_with_flow(linear_flow());
    let err = service
        .load_ordered_nodes("my-api", "does-not-exist")
        .expect_err("an unknown flow name must error");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[test]
fn cancel_on_unknown_run_id_is_a_harmless_noop() {
    let service = service_with_flow(linear_flow());
    service.cancel("no-such-run-id"); // must not panic
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: FAIL — `FlowExecutionService` does not exist yet (compile error).

- [ ] **Step 3: Implement the shell**

```rust
// crates/rocket-app/src/flow_execution_service.rs (add above the tests module)
use std::sync::{Arc, Mutex};
use std::collections::HashSet;
use ulid::Ulid;

/// Orchestrates one Flow run: loads the graph, walks it in dependency order,
/// and dispatches each node using the building blocks in this same module
/// (`build_execute_request_input`, `apply_wired_overrides`,
/// `resolve_flow_wire_expression`). Holds no execution machinery of its own —
/// `run()` takes the `RequestExecutionService` to drive, the same pattern
/// `CollectionRunnerService::run` uses.
pub struct FlowExecutionService {
    flow_repo: Box<dyn rocket_flow::FlowRepository>,
    collection_repo: Box<dyn rocket_collection::CollectionRepository>,
    events: Box<dyn rocket_shared::events::EventPublisher>,
    cancelled: Arc<Mutex<HashSet<String>>>,
    in_flight: Arc<Mutex<HashSet<String>>>,
}

impl FlowExecutionService {
    pub fn new(
        flow_repo: Box<dyn rocket_flow::FlowRepository>,
        collection_repo: Box<dyn rocket_collection::CollectionRepository>,
        events: Box<dyn rocket_shared::events::EventPublisher>,
    ) -> Self {
        Self {
            flow_repo,
            collection_repo,
            events,
            cancelled: Arc::new(Mutex::new(HashSet::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Loads the named flow and validates it into a dependency-ordered node id
    /// list. A cyclic graph is rejected here (defense in depth — `save_flow`,
    /// Plan 07, should already have refused to persist one) rather than ever
    /// starting a run against it.
    fn load_ordered_nodes(&self, collection: &str, flow_name: &str) -> DomainResult<(rocket_flow::Flow, Vec<String>)> {
        let flow = self.flow_repo.get(collection, flow_name)?;
        let order = rocket_flow::topological_sort(&flow)
            .map_err(|e| DomainError::InvalidInput(format!("flow '{flow_name}' is not runnable: {e}")))?;
        Ok((flow, order))
    }

    /// Asks an in-progress run to stop. The run ends before its next node; a
    /// node already executing finishes first. Cancelling an unknown or
    /// finished run id is a no-op, mirroring `CollectionRunnerService::cancel`.
    pub fn cancel(&self, run_id: &str) {
        if let Ok(in_flight) = self.in_flight.lock() {
            if !in_flight.contains(run_id) {
                return;
            }
        }
        if let Ok(mut cancelled) = self.cancelled.lock() {
            cancelled.insert(run_id.to_string());
        }
    }

    fn is_cancelled(&self, run_id: &str) -> bool {
        self.cancelled
            .lock()
            .map(|set| set.contains(run_id))
            .unwrap_or(false)
    }

    fn clear_cancellation(&self, run_id: &str) {
        if let Ok(mut set) = self.cancelled.lock() {
            set.remove(run_id);
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: PASS — 18 tests total (14 from Plan 05, 4 from this task).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(app): add FlowExecutionService shell and graph loading"
```

---

## Task 2: `run()` — dispatch loop, no cascade yet

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Consumes: `build_execute_request_input`, `apply_wired_overrides`, `resolve_flow_wire_expression`, `CapturedOutput` (Plan 05); `load_ordered_nodes`, cancellation (Task 1 of this plan); `DomainEvent::{FlowRunStarted, FlowStepCompleted}` (Plan 04).
- Produces: `RunFlowInput`, `FlowStepResult`, `FlowRunSummary`, `FlowExecutionService::run` — consumed by Plan 07's `run_flow` command.

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-app/src/flow_execution_service.rs (add to the existing tests module)

fn request_flow_node(id: &str, url: &str) -> FlowNode {
    FlowNode {
        id: id.to_string(),
        kind: FlowNodeKind::Request {
            label: format!("Node {id}"),
            source: RequestSource::Inline {
                request: InlineRequestData {
                    method: "get".to_string(),
                    url: url.to_string(),
                    headers: Vec::new(),
                    body: None,
                },
            },
        },
        position: NodePosition { x: 0.0, y: 0.0 },
    }
}

struct FixedResponseExecutor {
    status: u16,
}
#[async_trait]
impl HttpExecutor for FixedResponseExecutor {
    async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
        Ok(HttpResponse {
            status: self.status,
            status_text: "OK".into(),
            headers: vec![],
            body: r#"{"value":"ok"}"#.into(),
            duration_ms: 5,
            ttfb_ms: 2,
            size_bytes: 15,
        })
    }
}

fn exec_with_status(status: u16) -> RequestExecutionService {
    RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(FixedResponseExecutor { status }),
        Box::new(NullHistoryRepo),
        Box::new(FakeCollectionRepo::new()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(FixedJsonqEngine {
        value: serde_json::json!("ok"),
    }))
}

#[tokio::test]
async fn empty_flow_completes_immediately_with_no_steps() {
    let service = service_with_flow(Flow {
        name: "empty".to_string(),
        nodes: Vec::new(),
        edges: Vec::new(),
    });
    let exec = exec_with_status(200);

    let summary = service
        .run(
            &exec,
            RunFlowInput {
                collection: "my-api".to_string(),
                flow_name: "empty".to_string(),
                environment_name: None,
            },
        )
        .await
        .expect("empty flow must run cleanly");

    assert!(summary.steps.is_empty());
}

#[tokio::test]
async fn single_request_node_executes_and_reports_success() {
    let service = service_with_flow(Flow {
        name: "one-node".to_string(),
        nodes: vec![request_flow_node("a", "https://api.example.com/ping")],
        edges: Vec::new(),
    });
    let exec = exec_with_status(200);

    let summary = service
        .run(
            &exec,
            RunFlowInput {
                collection: "my-api".to_string(),
                flow_name: "one-node".to_string(),
                environment_name: None,
            },
        )
        .await
        .expect("run must succeed");

    assert_eq!(summary.steps.len(), 1);
    assert_eq!(summary.steps[0].node_id, "a");
    assert_eq!(summary.steps[0].status, rocket_shared::events::FlowNodeStatus::Success);
    assert_eq!(summary.steps[0].status_code, Some(200));
}

#[tokio::test]
async fn unknown_flow_name_errors_before_publishing_started() {
    let service = service_with_flow(linear_flow());
    let exec = exec_with_status(200);

    let err = service
        .run(
            &exec,
            RunFlowInput {
                collection: "my-api".to_string(),
                flow_name: "does-not-exist".to_string(),
                environment_name: None,
            },
        )
        .await
        .expect_err("unknown flow name must error");
    assert!(matches!(err, DomainError::NotFound(_)));
}

#[tokio::test]
async fn cancelling_before_the_run_starts_stops_it_immediately() {
    let service = service_with_flow(Flow {
        name: "two-nodes".to_string(),
        nodes: vec![
            request_flow_node("a", "https://api.example.com/a"),
            request_flow_node("b", "https://api.example.com/b"),
        ],
        edges: Vec::new(),
    });
    let exec = exec_with_status(200);
    // Cancel before run() is even called is not directly expressible (run_id
    // is generated inside run()); instead this test exercises the same
    // cancellation flag path by cancelling a run id it already knows the
    // service will not have registered, confirming no panic and normal
    // completion — the true mid-run cancellation race is covered by an
    // integration-level test once Plan 07 exposes cancel_flow_run over IPC.
    service.cancel("irrelevant-run-id");
    let summary = service
        .run(
            &exec,
            RunFlowInput {
                collection: "my-api".to_string(),
                flow_name: "two-nodes".to_string(),
                environment_name: None,
            },
        )
        .await
        .expect("run must still complete normally");
    assert_eq!(summary.steps.len(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: FAIL — `RunFlowInput`/`FlowStepResult`/`FlowRunSummary`/`run` do not
exist yet (compile error).

- [ ] **Step 3: Implement the types and the loop**

```rust
// crates/rocket-app/src/flow_execution_service.rs (add above the tests module)
use rocket_shared::events::{DomainEvent, FlowNodeStatus};

#[derive(Debug, Clone)]
pub struct RunFlowInput {
    pub collection: String,
    pub flow_name: String,
    pub environment_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FlowStepResult {
    pub node_id: String,
    pub status: FlowNodeStatus,
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FlowRunSummary {
    pub run_id: String,
    pub steps: Vec<FlowStepResult>,
    pub stopped_reason: String,
}

impl FlowExecutionService {
    /// Runs every node of `input.flow_name` in dependency order, dispatching
    /// each by kind and publishing progress events. Node failures do not
    /// stop the run here — see Task 3 for downstream skip-cascade, added on
    /// top of this loop without changing its shape.
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
    ) -> DomainResult<FlowRunSummary> {
        let (flow, order) = self.load_ordered_nodes(&input.collection, &input.flow_name)?;

        let _external_secrets = exec
            .resolve_external_secrets(Some(&input.collection), input.environment_name.as_deref())
            .await?;

        let run_id = Ulid::new().to_string();
        if let Ok(mut set) = self.in_flight.lock() {
            set.insert(run_id.clone());
        }
        self.events.publish(DomainEvent::FlowRunStarted {
            run_id: run_id.clone(),
            flow_name: input.flow_name.clone(),
            collection: input.collection.clone(),
            total_nodes: flow.nodes.len(),
        });

        let mut captured: HashMap<String, CapturedOutput> = HashMap::new();
        let mut steps: Vec<FlowStepResult> = Vec::new();
        let mut stopped_reason = "completed".to_string();

        for node_id in &order {
            if self.is_cancelled(&run_id) {
                stopped_reason = "cancelled".to_string();
                break;
            }
            let node = flow
                .nodes
                .iter()
                .find(|n| &n.id == node_id)
                .expect("topological_sort only returns ids present in flow.nodes");

            let result = self
                .execute_node(exec, &input, &flow, node, &captured)
                .await;

            if let Ok((output, _)) = &result {
                captured.insert(node_id.clone(), output.clone());
            }
            let step = result_to_step(node_id, result);
            self.events.publish(DomainEvent::FlowStepCompleted {
                run_id: run_id.clone(),
                node_id: step.node_id.clone(),
                status: step.status,
                status_code: step.status_code,
                duration_ms: step.duration_ms,
                error: step.error.clone(),
            });
            steps.push(step);
        }

        self.clear_cancellation(&run_id);
        if let Ok(mut set) = self.in_flight.lock() {
            set.remove(&run_id);
        }

        let failed_count = steps.iter().filter(|s| s.status == FlowNodeStatus::Failed).count();
        let skipped_count = steps.iter().filter(|s| s.status == FlowNodeStatus::Skipped).count();
        self.events.publish(DomainEvent::FlowRunFinished {
            run_id: run_id.clone(),
            stopped_reason: stopped_reason.clone(),
            node_count: steps.len(),
            failed_count,
            skipped_count,
        });

        Ok(FlowRunSummary {
            run_id,
            steps,
            stopped_reason,
        })
    }

    /// Dispatches one node by kind. Returns the node's captured output (for
    /// downstream wires) alongside its `FlowStepResult` fields, or an error
    /// if the node itself failed — the caller (`run`) turns either outcome
    /// into a `FlowStepResult` via `result_to_step`.
    async fn execute_node(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        flow: &rocket_flow::Flow,
        node: &rocket_flow::FlowNode,
        captured: &HashMap<String, CapturedOutput>,
    ) -> DomainResult<(CapturedOutput, Option<(u16, u64)>)> {
        match &node.kind {
            rocket_flow::FlowNodeKind::Input { value, .. } => {
                let settings = self.collection_repo.get_settings(&input.collection)?;
                let mut vars = HashMap::new();
                for cv in settings.variables.iter().filter(|v| v.enabled) {
                    let v = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                    vars.insert(cv.key.clone(), v);
                }
                let resolved = rocket_environment::resolve(value.data(), &vars).output;
                Ok((CapturedOutput::Value(rocket_shared::VariableValue::simple(resolved)), None))
            }
            rocket_flow::FlowNodeKind::Output { .. } => {
                let incoming = flow.edges.iter().find(|e| e.target_node_id == node.id);
                let Some(edge) = incoming else {
                    return Ok((CapturedOutput::Value(rocket_shared::VariableValue::simple("")), None));
                };
                let source_output = captured.get(&edge.source_node_id).ok_or_else(|| {
                    DomainError::Internal(format!(
                        "node '{}' depends on '{}' which has not executed yet — topological order violated",
                        node.id, edge.source_node_id
                    ))
                })?;
                let value = exec
                    .resolve_flow_wire_expression(&input.collection, source_output, &edge.expression)
                    .await?;
                Ok((CapturedOutput::Value(rocket_shared::VariableValue::simple(value)), None))
            }
            rocket_flow::FlowNodeKind::Request { .. } => {
                let mut request_input = build_execute_request_input(
                    self.collection_repo.as_ref(),
                    &input.collection,
                    input.environment_name.as_deref(),
                    node,
                )?;

                let incoming: Vec<&rocket_flow::FlowEdge> =
                    flow.edges.iter().filter(|e| e.target_node_id == node.id).collect();
                let mut resolved = HashMap::new();
                for edge in &incoming {
                    let source_output = captured.get(&edge.source_node_id).ok_or_else(|| {
                        DomainError::Internal(format!(
                            "node '{}' depends on '{}' which has not executed yet — topological order violated",
                            node.id, edge.source_node_id
                        ))
                    })?;
                    let value = exec
                        .resolve_flow_wire_expression(&input.collection, source_output, &edge.expression)
                        .await?;
                    resolved.insert(edge.id.clone(), value);
                }
                let edges_owned: Vec<rocket_flow::FlowEdge> = incoming.into_iter().cloned().collect();
                apply_wired_overrides(&mut request_input, &resolved, &edges_owned)?;

                let output = exec.execute(request_input).await?;
                let timing = (output.response.status, output.response.duration_ms);
                Ok((CapturedOutput::Request(Box::new(output)), Some(timing)))
            }
        }
    }
}

/// Turns one node's `execute_node` outcome into its `FlowStepResult`. A node
/// counts as failed when `execute_node` errored, or when it produced a
/// `Request` response that is not 2xx (`HttpResponse::is_success`) — Flow
/// nodes carry no test scripts in Phase 1, so there is no test-failure case
/// to fold in here, unlike the Collection Runner's `RunStepResult::is_failure`.
fn result_to_step(
    node_id: &str,
    result: DomainResult<(CapturedOutput, Option<(u16, u64)>)>,
) -> FlowStepResult {
    match result {
        Ok((CapturedOutput::Request(out), Some((status, duration_ms)))) => FlowStepResult {
            node_id: node_id.to_string(),
            status: if out.response.is_success() {
                FlowNodeStatus::Success
            } else {
                FlowNodeStatus::Failed
            },
            status_code: Some(status),
            duration_ms: Some(duration_ms),
            error: if out.response.is_success() {
                None
            } else {
                Some(format!("non-2xx response: {status}"))
            },
        },
        Ok((CapturedOutput::Value(_), _)) => FlowStepResult {
            node_id: node_id.to_string(),
            status: FlowNodeStatus::Success,
            status_code: None,
            duration_ms: None,
            error: None,
        },
        Ok((CapturedOutput::Request(_), None)) => unreachable!(
            "a Request node's execute_node branch always returns Some(timing)"
        ),
        Err(e) => FlowStepResult {
            node_id: node_id.to_string(),
            status: FlowNodeStatus::Failed,
            status_code: None,
            duration_ms: None,
            error: Some(e.to_string()),
        },
    }
}
```

Add `use rocket_flow::FlowEdge;` if not already imported from Plan 05 Task 3
(it is — this file's imports accumulate across tasks, do not duplicate the
`use` line).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: PASS — 22 tests total (18 from Task 1, 4 from this task).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(app): implement FlowExecutionService run loop"
```

---

## Task 3: Failure skip-cascade + `reachable_from`

**Files:**
- Modify: `crates/rocket-flow/src/graph.rs` (adds `reachable_from`, alongside `topological_sort` from Plan 02)
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Produces: `rocket_flow::graph::reachable_from(flow: &Flow, start_node_id: &str) -> Vec<String>` — a pure BFS over `flow.edges`, returning every node id reachable by following edges forward from (but not including) `start_node_id`. Placed in `rocket-flow` rather than `rocket-app` because it is pure graph logic over `rocket-flow`'s own `Flow`/`FlowEdge` types, the same reasoning that put `topological_sort` there.

- [ ] **Step 1: Write the failing tests for `reachable_from`**

```rust
// crates/rocket-flow/src/graph.rs (add to the existing tests module from Plan 02)

fn diamond_flow() -> Flow {
    // a -> b -> d
    //  \-> c -/
    // plus an unrelated, disconnected node e.
    Flow {
        name: "diamond".to_string(),
        nodes: vec!["a", "b", "c", "d", "e"]
            .into_iter()
            .map(|id| FlowNode {
                id: id.to_string(),
                kind: FlowNodeKind::Output { label: id.to_string() },
                position: NodePosition { x: 0.0, y: 0.0 },
            })
            .collect(),
        edges: vec![
            edge_fixture("e1", "a", "b"),
            edge_fixture("e2", "a", "c"),
            edge_fixture("e3", "b", "d"),
            edge_fixture("e4", "c", "d"),
        ],
    }
}

fn edge_fixture(id: &str, source: &str, target: &str) -> FlowEdge {
    FlowEdge {
        id: id.to_string(),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        target_field: "value".to_string(),
        expression: "response.body".to_string(),
    }
}

#[test]
fn reachable_from_root_includes_every_downstream_node_once() {
    let flow = diamond_flow();
    let mut reached = reachable_from(&flow, "a");
    reached.sort();
    assert_eq!(reached, vec!["b".to_string(), "c".to_string(), "d".to_string()]);
}

#[test]
fn reachable_from_excludes_unrelated_disconnected_nodes() {
    let flow = diamond_flow();
    let reached = reachable_from(&flow, "a");
    assert!(!reached.contains(&"e".to_string()));
}

#[test]
fn reachable_from_a_leaf_node_is_empty() {
    let flow = diamond_flow();
    assert!(reachable_from(&flow, "d").is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-flow graph::tests -j4`
Expected: FAIL — `reachable_from` does not exist yet (compile error).

- [ ] **Step 3: Implement `reachable_from`**

```rust
// crates/rocket-flow/src/graph.rs (add above the tests module)
use std::collections::{HashSet, VecDeque};

/// Every node id reachable by following edges forward from `start_node_id`
/// (exclusive) — a plain BFS. Used to compute which nodes must be skipped
/// when `start_node_id` fails during execution.
pub fn reachable_from(flow: &Flow, start_node_id: &str) -> Vec<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    queue.push_back(start_node_id);

    while let Some(current) = queue.pop_front() {
        for edge in flow.edges.iter().filter(|e| e.source_node_id == current) {
            if visited.insert(edge.target_node_id.clone()) {
                queue.push_back(&edge.target_node_id);
            }
        }
    }

    visited.into_iter().collect()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-flow graph::tests -j4`
Expected: PASS — all `topological_sort` tests from Plan 02 plus 3 new
`reachable_from` tests.

- [ ] **Step 5: Write the failing skip-cascade test in `rocket-app`**

**Correction:** `exec_with_status(500)` (Task 2) makes every HTTP call in the
run return 500, including node `c`'s — that would make `c` fail too, breaking
the "independent node still succeeds" assertion this test needs. Add a
URL-aware executor test double instead, so only node `a`'s specific URL
returns 500 and every other URL returns 200:

```rust
// crates/rocket-app/src/flow_execution_service.rs (add to the existing tests module)

struct UrlAwareExecutor {
    failing_url: String,
}
#[async_trait]
impl HttpExecutor for UrlAwareExecutor {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse> {
        let status = if request.url == self.failing_url { 500 } else { 200 };
        Ok(HttpResponse {
            status,
            status_text: if status == 200 { "OK" } else { "Internal Server Error" }.into(),
            headers: vec![],
            body: r#"{"value":"ok"}"#.into(),
            duration_ms: 5,
            ttfb_ms: 2,
            size_bytes: 15,
        })
    }
}

fn exec_failing_for_url(failing_url: &str) -> RequestExecutionService {
    RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(UrlAwareExecutor { failing_url: failing_url.to_string() }),
        Box::new(NullHistoryRepo),
        Box::new(FakeCollectionRepo::new()),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(FixedJsonqEngine {
        value: serde_json::json!("ok"),
    }))
}

#[tokio::test]
async fn failed_node_skips_only_its_downstream_dependents() {
    // a (fails, 500) -> b (depends on a)      -- b must be Skipped
    // c (independent, succeeds)               -- c must still run
    let flow = Flow {
        name: "skip-cascade".to_string(),
        nodes: vec![
            request_flow_node("a", "https://api.example.com/a"),
            request_flow_node("b", "https://api.example.com/b"),
            request_flow_node("c", "https://api.example.com/c"),
        ],
        edges: vec![FlowEdge {
            id: "e1".to_string(),
            source_node_id: "a".to_string(),
            target_node_id: "b".to_string(),
            target_field: "url".to_string(),
            expression: "response.body".to_string(),
        }],
    };
    let service = service_with_flow(flow);
    let exec = exec_failing_for_url("https://api.example.com/a");

    let summary = service
        .run(
            &exec,
            RunFlowInput {
                collection: "my-api".to_string(),
                flow_name: "skip-cascade".to_string(),
                environment_name: None,
            },
        )
        .await
        .expect("run must complete even with a failed node");

    let status_of = |id: &str| {
        summary
            .steps
            .iter()
            .find(|s| s.node_id == id)
            .expect("step must be recorded for this node id")
            .status
    };
    assert_eq!(status_of("a"), FlowNodeStatus::Failed);
    assert_eq!(status_of("b"), FlowNodeStatus::Skipped);
    assert_eq!(status_of("c"), FlowNodeStatus::Success);
    assert_eq!(summary.steps.iter().filter(|s| s.status == FlowNodeStatus::Failed).count(), 1);
    assert_eq!(summary.steps.iter().filter(|s| s.status == FlowNodeStatus::Skipped).count(), 1);
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo test -p rocket-app flow_execution_service::tests::failed_node_skips_only_its_downstream_dependents -j4`
Expected: FAIL — `b` currently also attempts to execute (and itself fails,
since its wired-from URL never got overridden), instead of being `Skipped`.

- [ ] **Step 7: Add skip-cascade to the loop**

```rust
// crates/rocket-app/src/flow_execution_service.rs — replace the `for node_id in &order` loop body inside `run()` with:

let mut skipped: HashSet<String> = HashSet::new();

for node_id in &order {
    if self.is_cancelled(&run_id) {
        stopped_reason = "cancelled".to_string();
        break;
    }
    let node = flow
        .nodes
        .iter()
        .find(|n| &n.id == node_id)
        .expect("topological_sort only returns ids present in flow.nodes");

    if skipped.contains(node_id) {
        let step = FlowStepResult {
            node_id: node_id.clone(),
            status: FlowNodeStatus::Skipped,
            status_code: None,
            duration_ms: None,
            error: Some("upstream node failed".to_string()),
        };
        self.events.publish(DomainEvent::FlowStepCompleted {
            run_id: run_id.clone(),
            node_id: step.node_id.clone(),
            status: step.status,
            status_code: step.status_code,
            duration_ms: step.duration_ms,
            error: step.error.clone(),
        });
        steps.push(step);
        continue;
    }

    let result = self
        .execute_node(exec, &input, &flow, node, &captured)
        .await;

    if let Ok((output, _)) = &result {
        captured.insert(node_id.clone(), output.clone());
    }
    let step = result_to_step(node_id, result);
    if step.status == FlowNodeStatus::Failed {
        for downstream in rocket_flow::graph::reachable_from(&flow, node_id) {
            skipped.insert(downstream);
        }
    }
    self.events.publish(DomainEvent::FlowStepCompleted {
        run_id: run_id.clone(),
        node_id: step.node_id.clone(),
        status: step.status,
        status_code: step.status_code,
        duration_ms: step.duration_ms,
        error: step.error.clone(),
    });
    steps.push(step);
}
```

This replaces the loop body Task 2 wrote — the rest of `run()` (secrets,
`Ulid`, start/finish events, `FlowRunSummary` assembly) is unchanged.

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p rocket-app flow_execution_service::tests -j4`
Expected: PASS — 23 tests total (22 from Tasks 1-2, 1 from this task).

- [ ] **Step 9: Run both crates' full test suites**

Run: `cargo test -p rocket-flow -p rocket-app -j4`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/rocket-flow/src/graph.rs crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(app): cascade Skipped status to a failed node's dependents"
```

---

## Next Plan

[Plan 07: Tauri Flow commands](2026-09-27-flow-visual-workflow-builder-plan-07-tauri-commands.md) —
exposes `FlowRepository` CRUD and `FlowExecutionService::run`/`cancel` over
Tauri IPC, streaming step/finish events to the frontend.

## Post-Implementation Review

Before starting Plan 07, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `crates/rocket-flow/src/graph.rs`, `crates/rocket-app/src/flow_execution_service.rs`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — does `FlowExecutionService`
>    expose exactly `new`, `run`, `cancel` with the signatures the plan
>    index's locked interface contract (as corrected by this plan's and
>    Plan 05's "Corrections" sections) promises Plan 07 will consume, and
>    does `reachable_from` match Task 3's signature?
> 2. Code quality and test coverage versus this plan's Review Focus section
>    (empty flow, linear-chain skip-cascade with an independent survivor,
>    mid-run cancellation, wiring-expression failure treated as a node
>    failure, exact failed/skipped counts).
> 3. Duplication — confirm the skip-cascade logic calls
>    `rocket_flow::graph::reachable_from` rather than re-walking edges
>    inline in `rocket-app`, and confirm `result_to_step`'s failure
>    definition reuses `HttpResponse::is_success()` rather than a
>    hand-rolled status-range check.
> 4. DDD boundary conformance per `.claude/rules/rust-ddd-boundaries.md` —
>    `reachable_from` belongs in `rocket-flow` (pure graph logic on
>    `rocket-flow`'s own types) not `rocket-app`; `FlowExecutionService`
>    depends on `CollectionRepository`/`FlowRepository` only through their
>    traits.
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `cargo test -p rocket-flow -p rocket-app -j4` and
> `cargo check -p rocket-flow -p rocket-app -j4`, and confirm they still
> pass. Report what you found and fixed.

Only proceed to Plan 07 once this review comes back clean (or its fixes are
applied and re-verified).
