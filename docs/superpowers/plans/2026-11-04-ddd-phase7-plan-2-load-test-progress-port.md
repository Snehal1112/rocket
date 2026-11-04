# DDD Phase 7, Plan 2: a progress port for the load test (D-75)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rocket-http`'s load test reports progress through its own small trait and no longer imports `DomainEvent` or `EventPublisher`; `rocket-app` adapts the trait to the existing domain events, so the events the frontend receives stay exactly as they are.

**Architecture:** `rocket-http/src/load_test.rs` gets `pub trait LoadTestProgress { fn progress(&self, event: LoadTestProgressEvent); fn complete(&self, result: LoadTestResult); }`. `run_load_test_v2` takes `Option<Arc<dyn LoadTestProgress>>` instead of `Option<Arc<dyn EventPublisher>>`. `rocket-app` adds a private adapter in `load_test_service.rs` that wraps an `Arc<dyn EventPublisher>` and publishes `DomainEvent::LoadTestProgress` and `DomainEvent::LoadTestComplete`. `LoadTestService::run` keeps its signature, so `src-tauri` and the tests do not change.

**Tech Stack:** Rust, `rocket-http`, `rocket-app`.

**Spec:** `docs/roadmaps/ddd-roadmap.md` item D-75. D-75 is not blocked by D-74: the wire types `LoadTestProgressEvent` and `LoadTestResult` stay in `rocket-shared` (moving them is D-71).

## Global Constraints

- The wire format is unchanged: the same two `DomainEvent` variants carry the same payloads, with the same order (progress events every 250 ms, then exactly one complete event, last).
- `run_load_test_v2` with `None` behaves as before (no reporting, same result).
- `LoadTestService::run` keeps its signature `(execution_service, executor, input, config, Option<Arc<dyn EventPublisher>>)`.
- No `unwrap` in production code. `cargo` commands take `-j4`; never `--workspace`.
- 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (the crate touched is `rocket-http`).

## Review Focus

- A run with a progress sink gets one `complete` call, after every `progress` call, with the same totals as the returned result.
- A run with `None` still returns the same result and calls nothing.
- The adapter turns `progress` into `DomainEvent::LoadTestProgress` and `complete` into `DomainEvent::LoadTestComplete`, nothing else, in call order.
- After the change `rocket-http` has no `rocket_shared::events` import (grep).
- The existing `LoadTestService::run` test that checks "one completion event, and it is the last" still passes unchanged.

---

### Task 1: The port, the runner and the adapter

**Files:**
- Modify: `crates/rocket-http/src/load_test.rs` (trait, `run_load_test_v2` parameter, the two publish sites, the import, the existing `run_load_test_v2` tests that pass `None` need no change), `crates/rocket-http/src/lib.rs` (re-export `LoadTestProgress`)
- Modify: `crates/rocket-app/src/load_test_service.rs` (adapter, `run` wraps the publisher)

**Interfaces:**
- Produces in `rocket-http`:
  ```rust
  pub trait LoadTestProgress: Send + Sync {
      /// Called about every 250 ms while the run is going.
      fn progress(&self, event: LoadTestProgressEvent);
      /// Called once, after the last `progress`, with the final result.
      fn complete(&self, result: LoadTestResult);
  }
  pub async fn run_load_test_v2(
      executor: Arc<dyn HttpExecutor>,
      request: &HttpRequest,
      config: &LoadTestConfigV2,
      progress: Option<Arc<dyn LoadTestProgress>>,
  ) -> LoadTestResult;
  ```
- Produces in `rocket-app` (private to `load_test_service.rs`): `struct EventProgress(Arc<dyn EventPublisher>)` implementing `LoadTestProgress`.
- Consumes: `DomainEvent::{LoadTestProgress, LoadTestComplete}` and `EventPublisher` from `rocket-shared`.

- [ ] **Step 1: Write the failing tests.** In `rocket-http` `load_test.rs` tests: a `RecordingProgress` double (a `Mutex<Vec<...>>` of progress events and of results) and the test `run_load_test_v2_reports_progress_then_one_complete` (a 1 s Hold phase with `MockExecutor` and the sink: at least one progress call, exactly one complete call, the completed result equals the returned result's `total_requests` and `succeeded`, and no progress call comes after the complete call). In `rocket-app` `load_test_service.rs` tests: `event_progress_publishes_the_matching_domain_events` (`EventProgress` over `RecordingPublisher`: `progress(e)` then `complete(r)` gives `[LoadTestProgress(e), LoadTestComplete(r)]` in that order).
- [ ] **Step 2: Run** `cargo test -j4 -p rocket-http load_test_v2_reports` — Expected: compile error (red).
- [ ] **Step 3: Implement** the trait and switch `run_load_test_v2` (the snapshot task calls `progress`, the end of the run calls `complete`; remove the events import). Re-export `LoadTestProgress` from `rocket-http/src/lib.rs`.
- [ ] **Step 4: Implement** `EventProgress` in `load_test_service.rs`; `LoadTestService::run` maps `Option<Arc<dyn EventPublisher>>` to `Option<Arc<dyn LoadTestProgress>>` with it.
- [ ] **Step 5: Run** `cargo test -j4 -p rocket-http load_test` and `cargo test -j4 -p rocket-app load_test_service` — Expected: pass, including the existing "one completion event, and it is the last" test. Then `grep -rn "rocket_shared::events" crates/rocket-http/src` — Expected: no output.
- [ ] **Step 6: Commit** (`feat(http): add the LoadTestProgress port`, then `refactor(app): adapt load test progress to domain events`).

### Task 2: Docs and checks

**Files:**
- Modify: `crates/rocket-http/CLAUDE.md` (the `load_test` row), `crates/rocket-app/CLAUDE.md` if it lists the load test service, `docs/roadmaps/ddd-roadmap.md` (D-75 done; D-71 keeps the note that moving `load_test.rs` waits for D-74)

- [ ] **Step 1: Edit the docs.** Run `cargo check -j4 --all-targets` and `cargo clippy -j4 -p rocket-http -p rocket-app --all-targets` — Expected: no new warnings.
- [ ] **Step 2: Commit** (`docs: mark DDD Phase 7 item D-75 done`).
