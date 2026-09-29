# Flow Async P0 — Cancel Signal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every Flow run a cancel signal that a node can await, so Stop ends a poll or callback wait at once instead of after the node.

**Architecture:** A new `flow_cancel` module wraps a `tokio::sync::watch` channel in a `CancelHandle` (owner) and a `CancelSignal` (node side). `FlowExecutionService` stores one handle per in-flight run, triggers it from `cancel()`, and passes the signal to `execute_node` inside a `NodeRunContext`. A drop guard removes every per-run registration on every exit path of `run`. After each node, `run` checks the signal and stops.

**Tech Stack:** Rust, `tokio` 1.50 (`sync`, `time`, already a `rocket-app` dependency through the workspace `full` feature set).

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` §5.1. Index and locked contract: `docs/superpowers/plans/flow-async/00-index.md`.

## Global Constraints

- Never use the panicking-unwrap call in production code. A write hook also blocks that literal text anywhere, prose included.
- No new crates. `tokio-util` is only a transitive dependency; use `tokio::sync::watch`.
- Cargo commands always pass `-j4` and target `rocket-app` only. Never run the full workspace test suite.
- A request that is already in flight still finishes. Aborting the HTTP call is out of scope.
- Existing flows run with the same statuses and events as before. No saved fields change.
- Commit every task with the `dev-workflow-skills:1-git-commit` skill.

## Review Focus

1. Stop pressed while a node is executing (not between nodes): the run must stop after that node, and the next node must not run. Pinned by Task 2 `cancel_during_a_node_stops_the_run_after_it`.
2. A node that errors because the run was cancelled: the step must read `cancelled`, not an internal error text. Pinned by Task 2 `a_node_that_fails_during_a_cancel_reports_cancelled`.
3. A cancel for a run id that is not in flight (finished, or never started): no panic, no stale handle. Pinned by Task 2 `cancel_on_unknown_run_id_is_a_harmless_noop` (existing) plus `cancel_triggers_the_runs_signal`.
4. A run that ends normally or by cancel must leave no handle behind, or later runs leak memory. Pinned by Task 2 `finished_runs_leave_no_cancel_handle`.
5. A signal whose handle was dropped without a cancel (run finished) must never fire. Pinned by Task 1 `cancelled_never_resolves_after_the_handle_is_dropped`.

---

### Task 1: Cancel signal module

**Files:**
- Create: `crates/rocket-app/src/flow_cancel.rs`
- Modify: `crates/rocket-app/src/lib.rs:14-16` (module list, next to `flow_debug` / `flow_routing`)

**Interfaces:**
- Consumes: nothing.
- Produces (locked, `00-index.md`):
  - `pub(crate) struct CancelHandle`, `pub(crate) struct CancelSignal` (`Clone`), `pub(crate) struct Cancelled` (`Debug, Clone, Copy, PartialEq, Eq`)
  - `pub(crate) fn cancel_pair() -> (CancelHandle, CancelSignal)`
  - `CancelHandle::cancel(&self)`
  - `CancelSignal::is_cancelled(&self) -> bool`, `async fn cancelled(&mut self)`, `async fn sleep(&mut self, dur: Duration) -> Result<(), Cancelled>`

- [ ] **Step 1: Register the module and write the failing tests**

In `crates/rocket-app/src/lib.rs`, add after `pub(crate) mod flow_debug;`:

```rust
pub(crate) mod flow_cancel;
```

Create `crates/rocket-app/src/flow_cancel.rs` with only the tests:

```rust
//! Per-run cancel signal for Flow runs. `FlowExecutionService::cancel`
//! triggers it, and a node that waits listens to it, so Stop ends the wait
//! at once instead of after the node.

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_new_signal_is_not_cancelled() {
        let (_handle, signal) = cancel_pair();
        assert!(!signal.is_cancelled());
    }

    #[test]
    fn cancel_is_seen_by_every_clone() {
        let (handle, signal) = cancel_pair();
        let clone = signal.clone();
        handle.cancel();
        assert!(signal.is_cancelled());
        assert!(clone.is_cancelled());
    }

    #[tokio::test]
    async fn sleep_finishes_when_not_cancelled() {
        let (_handle, mut signal) = cancel_pair();
        assert_eq!(signal.sleep(Duration::from_millis(10)).await, Ok(()));
    }

    #[tokio::test]
    async fn sleep_returns_cancelled_as_soon_as_the_run_is_cancelled() {
        let (handle, mut signal) = cancel_pair();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            handle.cancel();
        });
        let outcome = tokio::time::timeout(
            Duration::from_secs(1),
            signal.sleep(Duration::from_secs(30)),
        )
        .await
        .expect("a cancel must end the sleep well before 30s");
        assert_eq!(outcome, Err(Cancelled));
    }

    #[tokio::test]
    async fn sleep_after_a_cancel_returns_at_once() {
        let (handle, mut signal) = cancel_pair();
        handle.cancel();
        let outcome = tokio::time::timeout(
            Duration::from_millis(100),
            signal.sleep(Duration::from_secs(30)),
        )
        .await
        .expect("an already-cancelled signal must not sleep");
        assert_eq!(outcome, Err(Cancelled));
    }

    #[tokio::test]
    async fn cancelled_never_resolves_after_the_handle_is_dropped() {
        let (handle, mut signal) = cancel_pair();
        drop(handle);
        let waited =
            tokio::time::timeout(Duration::from_millis(50), signal.cancelled()).await;
        assert!(waited.is_err(), "a finished run must never look cancelled");
        assert!(!signal.is_cancelled());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_cancel`
Expected: compile error, `cannot find function 'cancel_pair' in this scope` (and `Cancelled` not found).

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `crates/rocket-app/src/flow_cancel.rs`:

```rust
use std::time::Duration;

use tokio::sync::watch;

/// Returned by `CancelSignal::sleep` when the run was cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cancelled;

/// Owner side of a run's cancel signal. `FlowExecutionService` holds one per
/// in-flight run.
pub(crate) struct CancelHandle {
    tx: watch::Sender<bool>,
}

/// Node side of a run's cancel signal. Cheap to clone.
#[derive(Clone)]
pub(crate) struct CancelSignal {
    rx: watch::Receiver<bool>,
}

/// Creates a connected handle and signal for one run.
pub(crate) fn cancel_pair() -> (CancelHandle, CancelSignal) {
    let (tx, rx) = watch::channel(false);
    (CancelHandle { tx }, CancelSignal { rx })
}

impl CancelHandle {
    /// Marks the run as cancelled. Every clone of the signal sees it.
    pub(crate) fn cancel(&self) {
        // `send_replace` stores the value even when no signal is listening.
        self.tx.send_replace(true);
    }
}

// Waiting nodes (plans 04 and 08) call `cancelled` and `sleep`. Until then
// only the tests do, so the release build would warn.
#[cfg_attr(not(test), allow(dead_code))]
impl CancelSignal {
    pub(crate) fn is_cancelled(&self) -> bool {
        *self.rx.borrow()
    }

    /// Resolves when the run is cancelled. It never resolves otherwise.
    pub(crate) async fn cancelled(&mut self) {
        // `wait_for` fails only when the handle is gone. The run is then
        // over without a cancel, so this waits forever.
        if self.rx.wait_for(|cancelled| *cancelled).await.is_err() {
            std::future::pending::<()>().await;
        }
    }

    /// Sleeps for `dur`, or returns `Err(Cancelled)` as soon as the run is
    /// cancelled.
    pub(crate) async fn sleep(&mut self, dur: Duration) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            return Err(Cancelled);
        }
        tokio::select! {
            _ = tokio::time::sleep(dur) => Ok(()),
            _ = self.cancelled() => Err(Cancelled),
        }
    }
}
```

Remove the now-duplicate `use std::time::Duration;` from the test module only if the compiler reports it as unused (the parent `use` makes `super::*` bring it in).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_cancel`
Expected: 6 passed.

Run: `cargo clippy -j4 -p rocket-app --tests -- -D warnings`
Expected: no warnings from `flow_cancel.rs`.

- [ ] **Step 5: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): add a per-run cancel signal`.

---

### Task 2: Wire the cancel signal into Flow runs

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`
  - imports near the top (after `use crate::flow_debug::build_debug_request;`)
  - `FlowExecutionService` struct and `new` (~:520-545)
  - `cancel` (~:569-578)
  - `run` (~:599-750): registration, per-node `NodeRunContext`, post-node check, cleanup
  - `execute_node` signature (~:764)
  - tests module: new tests next to `cancelling_mid_run_keeps_completed_steps_and_runs_nothing_further` (~:2787)

**Interfaces:**
- Consumes (Task 1): `cancel_pair`, `CancelHandle`, `CancelSignal`.
- Produces (locked, `00-index.md`):
  - `FlowExecutionService.cancel_handles: Arc<Mutex<HashMap<String, CancelHandle>>>`
  - `pub(crate) struct NodeRunContext { pub(crate) run_id: String, pub(crate) node_id: String, pub(crate) cancel: CancelSignal }`
  - `execute_node(..., ctx: &mut NodeRunContext)` as the last parameter. In this plan the parameter is named `_ctx` because no node reads it yet. Plans 04 and 08 rename it to `ctx` when they use it.
  - Post-node rule (extends the locked contract, see `00-index.md`): after `execute_node` returns, if `ctx.cancel.is_cancelled()`, the node's step is kept when it succeeded or failed for its own reason, but an `Err` result is recorded as `failed_step(node_id, "cancelled")`. The step is published, `stopped_reason` becomes `"cancelled"`, and the loop breaks.

- [ ] **Step 1: Write the failing tests**

Add to the tests module of `crates/rocket-app/src/flow_execution_service.rs`, right after the `CancelAfterSteps` publisher and its tests:

```rust
    /// Publisher that triggers the run's cancel handle as soon as `node_id`
    /// starts, so the cancel lands while that node is executing.
    struct CancelOnStart {
        node_id: &'static str,
        handles: Arc<Mutex<HashMap<String, CancelHandle>>>,
    }
    impl EventPublisher for CancelOnStart {
        fn publish(&self, event: DomainEvent) {
            if let DomainEvent::FlowStepStarted { run_id, node_id } = event {
                if node_id == self.node_id {
                    if let Some(handle) = self.handles.lock().expect("lock handles").get(&run_id) {
                        handle.cancel();
                    }
                }
            }
        }
    }

    fn service_cancelling_on_start(flow: Flow, node_id: &'static str) -> FlowExecutionService {
        let handles: Arc<Mutex<HashMap<String, CancelHandle>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut service = FlowExecutionService::new(
            Box::new(FakeFlowRepository::new().with_flow("my-api", flow)),
            Box::new(FakeCollectionRepo::new()),
            Box::new(CancelOnStart {
                node_id,
                handles: Arc::clone(&handles),
            }),
        );
        // Share one handle registry between the service and the canceller.
        service.cancel_handles = handles;
        service
    }

    #[tokio::test]
    async fn cancel_during_a_node_stops_the_run_after_it() {
        let flow = Flow {
            name: "two".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: Vec::new(),
        };
        let service = service_cancelling_on_start(flow, "a");
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service.run(&exec, run_input("two")).await.expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        assert_eq!(summary.steps.len(), 1, "b must not run after the cancel");
        assert_eq!(summary.steps[0].node_id, "a");
        assert_eq!(
            summary.steps[0].status,
            FlowNodeStatus::Success,
            "a request that finished keeps its real result"
        );
        assert_eq!(
            executor.sent_urls(),
            vec!["https://api.example.com/a".to_string()]
        );
    }

    #[tokio::test]
    async fn a_node_that_fails_during_a_cancel_reports_cancelled() {
        // b's wire script errors while the run is being cancelled.
        let flow = Flow {
            name: "wired".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: vec![wire("e1", "a", "b")],
        };
        let service = service_cancelling_on_start(flow, "b");
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, Box::new(ErrorJsonqEngine));

        let summary = service.run(&exec, run_input("wired")).await.expect("run");

        assert_eq!(summary.stopped_reason, "cancelled");
        let b = step_of(&summary, "b");
        assert_eq!(b.status, FlowNodeStatus::Failed);
        assert_eq!(b.error.as_deref(), Some("cancelled"));
    }

    #[test]
    fn cancel_triggers_the_runs_signal() {
        let service = service_with_flow(Flow {
            name: "x".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
        });
        let (handle, signal) = cancel_pair();
        service
            .in_flight
            .lock()
            .expect("lock in_flight")
            .insert("r1".to_string());
        service
            .cancel_handles
            .lock()
            .expect("lock handles")
            .insert("r1".to_string(), handle);

        service.cancel("r1");

        assert!(signal.is_cancelled());
    }

    #[tokio::test]
    async fn finished_runs_leave_no_cancel_handle() {
        let flow = Flow {
            name: "two".to_string(),
            nodes: vec![
                request_flow_node("a", "https://api.example.com/a"),
                request_flow_node("b", "https://api.example.com/b"),
            ],
            edges: Vec::new(),
        };
        // One run that completes and one that is cancelled.
        for cancel_on in [None, Some("a")] {
            let service = match cancel_on {
                Some(node) => service_cancelling_on_start(flow.clone(), node),
                None => service_with_flow(flow.clone()),
            };
            let executor = RecordingExecutor::new();
            let exec = recording_exec(&executor, fixed_wire("x"));

            service.run(&exec, run_input("two")).await.expect("run");

            assert!(
                service.cancel_handles.lock().expect("lock").is_empty(),
                "a finished run must drop its cancel handle"
            );
            assert!(service.in_flight.lock().expect("lock").is_empty());
            assert!(service.cancelled.lock().expect("lock").is_empty());
        }
    }
```

`Flow` must be `Clone` for the last test. It already derives `Clone` in `crates/rocket-flow/src/flow.rs`; if the compiler says otherwise, build the flow twice instead of cloning.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_execution_service::tests::cancel`
Expected: compile error, `no field 'cancel_handles' on type 'FlowExecutionService'` and `cannot find type 'CancelHandle'`.

- [ ] **Step 3: Add the imports, field and `NodeRunContext`**

After `use crate::flow_debug::build_debug_request;` add:

```rust
use crate::flow_cancel::{cancel_pair, CancelHandle, CancelSignal};
```

In the tests module's imports nothing changes: `use super::*;` brings these in.

Replace the struct and `new`:

```rust
pub struct FlowExecutionService {
    flow_repo: Box<dyn rocket_flow::FlowRepository>,
    collection_repo: Box<dyn rocket_collection::CollectionRepository>,
    events: Box<dyn rocket_shared::events::EventPublisher>,
    cancelled: Arc<Mutex<HashSet<String>>>,
    in_flight: Arc<Mutex<HashSet<String>>>,
    /// One cancel handle per in-flight run. `cancel` triggers it, so a node
    /// that is waiting stops at once.
    cancel_handles: Arc<Mutex<HashMap<String, CancelHandle>>>,
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
            cancel_handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }
```

Add, right after the `FlowRunSummary` struct:

```rust
/// What a node needs to know about the run it belongs to.
pub(crate) struct NodeRunContext {
    pub(crate) run_id: String,
    pub(crate) node_id: String,
    /// Fires when the run is cancelled. A waiting node selects on it.
    pub(crate) cancel: CancelSignal,
}

/// Registers a run as in flight and removes every trace of it when dropped,
/// so no exit path of `run` can leak the run id or its cancel handle.
struct RunRegistration<'a> {
    service: &'a FlowExecutionService,
    run_id: String,
}

impl<'a> RunRegistration<'a> {
    fn new(service: &'a FlowExecutionService, run_id: &str) -> (Self, CancelSignal) {
        let (handle, signal) = cancel_pair();
        if let Ok(mut handles) = service.cancel_handles.lock() {
            handles.insert(run_id.to_string(), handle);
        }
        if let Ok(mut set) = service.in_flight.lock() {
            set.insert(run_id.to_string());
        }
        let registration = Self {
            service,
            run_id: run_id.to_string(),
        };
        (registration, signal)
    }
}

impl Drop for RunRegistration<'_> {
    fn drop(&mut self) {
        self.service.clear_cancellation(&self.run_id);
        if let Ok(mut set) = self.service.in_flight.lock() {
            set.remove(&self.run_id);
        }
        if let Ok(mut handles) = self.service.cancel_handles.lock() {
            handles.remove(&self.run_id);
        }
    }
}
```

- [ ] **Step 4: Trigger the handle from `cancel`**

Replace `cancel` and its doc comment:

```rust
    /// Asks an in-progress run to stop. The run ends after the node that is
    /// running now. A node that is waiting (a poll or a callback wait) stops
    /// waiting at once; a request already in flight still finishes.
    /// Cancelling an unknown or finished run id is a no-op, mirroring
    /// `CollectionRunnerService::cancel`.
    pub fn cancel(&self, run_id: &str) {
        if let Ok(in_flight) = self.in_flight.lock() {
            if !in_flight.contains(run_id) {
                return;
            }
        }
        if let Ok(mut cancelled) = self.cancelled.lock() {
            cancelled.insert(run_id.to_string());
        }
        if let Ok(handles) = self.cancel_handles.lock() {
            if let Some(handle) = handles.get(run_id) {
                handle.cancel();
            }
        }
    }
```

- [ ] **Step 5: Register the run, pass the context, check after each node**

In `run`, replace:

```rust
        let run_id = Ulid::new().to_string();
        if let Ok(mut set) = self.in_flight.lock() {
            set.insert(run_id.clone());
        }
```

with:

```rust
        let run_id = Ulid::new().to_string();
        let (registration, cancel_signal) = RunRegistration::new(self, &run_id);
```

Inside the `NodeFate::Run { data_edges } => {` arm, before `let result = match node_opt {`, add:

```rust
                    let mut ctx = NodeRunContext {
                        run_id: run_id.clone(),
                        node_id: node_id.clone(),
                        cancel: cancel_signal.clone(),
                    };
```

and pass `&mut ctx` as the last argument of `self.execute_node(...)`.

Still inside that arm, replace `let step = result_to_step(node_id, node_opt, &result);` with:

```rust
                    // A cancel that landed while this node ran turns an error
                    // into "cancelled". A finished node keeps its real result.
                    let cancelled_now = ctx.cancel.is_cancelled();
                    let step = if cancelled_now && result.is_err() {
                        failed_step(node_id, "cancelled".to_string())
                    } else {
                        result_to_step(node_id, node_opt, &result)
                    };
```

At the end of the arm, replace the final `(step, outcome)` with:

```rust
                    if cancelled_now {
                        self.events.publish(step_completed_event(&run_id, &step));
                        steps.push(step);
                        stopped_reason = "cancelled".to_string();
                        break;
                    }
                    (step, outcome)
```

Replace the cleanup after the loop:

```rust
        self.clear_cancellation(&run_id);
        if let Ok(mut set) = self.in_flight.lock() {
            set.remove(&run_id);
        }
```

with:

```rust
        // Deregister before `FlowRunFinished`, as before this guard existed.
        drop(registration);
```

- [ ] **Step 6: Accept the context in `execute_node`**

Add the parameter last, after `debug: &mut Option<FlowDebugRequest>,`:

```rust
        // No node waits yet. Plans 04 and 08 rename this to `ctx` and use it.
        _ctx: &mut NodeRunContext,
```

Update the `run` doc comment's last sentence to: "Cancellation is checked before each node and after it, so a cancelled run records the node that was running and nothing after it."

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_execution_service`
Expected: all tests pass, including the four new ones and the existing `cancelling_mid_run_keeps_completed_steps_and_runs_nothing_further`, `cancel_is_checked_before_a_skipped_node_too` and `cancel_on_unknown_run_id_is_a_harmless_noop`.

Run: `cargo test -j4 -p rocket-app flow_cancel`
Expected: 6 passed.

Run: `cargo clippy -j4 -p rocket-app --tests -- -D warnings`
Expected: no warnings.

Run: `cargo check -j4 -p rocket --tests`
Expected: `src-tauri` still compiles (it only calls `FlowExecutionService::new`, `run` and `cancel`, whose signatures did not change).

- [ ] **Step 8: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): let Stop reach the node that is running`.
