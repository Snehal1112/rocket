# Client-Chosen Flow Run Id Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every flow run gets an id chosen by the tab that starts it, so two tabs running the same flow can never pick up each other's events and Stop always cancels the run of the tab it was pressed in.

**Architecture:** The toolbar makes a UUID per run (`newFlowRunId()`) after the save and sign-in steps, stores it on the tab as `pendingRunId`, subscribes to the run events, and sends the id in `RunFlowOptions.runId` (a new sixth `runFlow` argument). The backend takes it through `RunFlowInputDto.run_id` and `FlowRunOptions.run_id`, checks its format, and reserves it first thing in the run (`RunRegistration::reserve`). It refuses an id that is in flight or still kept in the run cache. Without an id it makes a ULID, as today. The id then names the run in every event, in the cancel registry, in the run cache and in the summary. The toolbar ignores every event with another id, and every event that arrives after `run_flow` settled. A remounted toolbar follows the tab's `pendingRunId` (or its running `runId`).

**Tech Stack:** Rust (`rocket-app`, `src-tauri` package `rocket`), `ulid`, React, TypeScript, Zustand (`pane-store`), Vitest and Testing Library.

**Spec:** Roadmap item F-03 in `.claude/flow-roadmap.md` (F-05 depends on it). Context: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (run matching at "Run matching by collection and flow name", P19 and P20 sections) and `docs/superpowers/plans/flow-tier3/2026-10-08-p20-run-from-node-ui.md` ("Assumed F-03 contract").

## The bug, verified at HEAD e2bd8487

- `src/components/flow/FlowToolbar.tsx`, `handleRun`: the `onFlowRunStarted` handler adopts the first event whose collection and flow name match (`if (runId !== null) return; if (event.collection !== collection || event.flow_name !== flowName) return; runId = event.run_id;`). Two tabs of one flow that both wait for their start both adopt whichever run starts first. The second tab then shows the first tab's steps, its own run's events are dropped (`runId` is already set), and its Stop calls `cancelFlowRun(liveRunId)` with the other tab's run id.
- The step, progress and step-started handlers already match by `run_id`, but only against the id that the started handler adopted, so they inherit the wrong id.
- `crates/rocket-app/src/flow_execution_service.rs`, `run_inner`: `let run_id = Ulid::new().to_string();` is minted after vault secrets, Auth tokens and callback endpoints, then `RunRegistration::new(self, &run_id)`. The client cannot know the id until `flow-run-started`.
- `cancel_flow_run(run_id)` (`src-tauri/src/commands/flow.rs`) already takes a run id and has one caller (`cancelFlowRun` in `src/lib/tauri-api.ts`, used only by `FlowToolbar.handleStop`). It needs no new signature and no backward-compatible path.
- Everything else that identifies a run already keys by run id and is correct once the id is right: the resume effect in `FlowToolbar` (`event.run_id !== resumedRunId`), `onFlowRunFinished`, `setFlowRunResult` (ignores a result of another run), `recordFlowRun` and `appendRunRecord` (dedupe by `runId`), `buildRunRecord`, `FlowRunStarted.callbacks` (only taken by the started handler), `FlowStepCompleted.trace` (rides on the step event). `useFlowRunAnnouncer` reads only `runState` and node maps. `useKeyboardShortcuts` dispatches `rocket:flow-run` with `{ tabId }` only, and every start goes through `handleRun`, so a Ctrl+Enter run gets its own id with no change there.
- A second, related gap: the toolbar unmounts when its tab is hidden, and the unmount drops its listeners. A tab hidden between Run and `flow-run-started` never learns that its run started, so its remounted toolbar shows Run enabled and can start a second run of the same flow in the same tab. `pendingRunId` closes this.

## Global Constraints

- Rust cargo commands always pass `-j4 -p <crate>`. Never `--workspace` or `--all`. Never `cargo fmt`.
- No `unwrap()` in production paths. Tests may use `expect`.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs (`RunFlowInputDto` already has it). `FlowRunOptions` is not an IPC DTO and gets no serde derive.
- Persistence shapes stay unchanged: `FlowRunSummary`, `FlowRunRecord`, `FlowLastRun` and every `DomainEvent` keep their fields. Only `RunFlowInputDto` (optional `runId`) and the in-memory `FlowTab` (optional `pendingRunId`) gain a field.
- shadcn/ui primitives and `lucide-react` icons only. No new UI elements are needed.
- Zustand: narrow selectors, never destructure store state at component top level.
- Code comments are short full sentences that end with a punctuation mark.
- The TS lib is ES2020: no `Array.prototype.at()`.
- Commits use conventional commits through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, the task's targeted `yarn test <pattern>`, and `cargo check -j4 -p rocket-app -p rocket` when Rust changes.
- One implementer at a time edits `FlowToolbar.tsx`, `FlowPane.tsx` and `flow_execution_service.rs` (P20 and F-05 edit the same code).
- Line numbers are from HEAD e2bd8487. Locate edits by the quoted code.
- Not in scope: making the pre-run phase itself interruptible or showing a "preparing" state (F-05), interrupting an in-flight HTTP send (F-04), cancelling a run when its tab closes.

## Review Focus

Failure modes the spec implies that are most likely to bite, most likely first. Each has a test in the task that owns the code.

1. Two tabs of one flow, where the second tab's run announces itself first: the first tab must not adopt it, and each tab must show only its own steps. Pinned in Task 3 (`each tab follows only its own run, whichever starts first`) and Task 2 (`ignores a flow-run-started event for another run of the same flow`).
2. Stop must cancel the run of the tab it was pressed in, both before and after `flow-run-started` arrived, and never another run. Pinned in Task 3 (`Stop in one tab cancels only that tab's run, before and after it started`), Task 2 (`Stop cancels its own run before flow-run-started arrives`) and Task 1 (`client_run_id_stop_cancels_only_that_run`).
3. A duplicate or forged id (malformed, in flight, or equal to a run still kept for partial runs) must be refused before any event, must not unregister the run that owns it, and must not overwrite that run's cache entry. Pinned in Task 1 (`client_run_id_that_is_malformed_is_refused_before_any_event`, `client_run_id_of_a_run_in_flight_is_refused_and_free_after_it_ends`, `client_run_id_of_a_kept_run_is_refused_and_the_kept_run_is_untouched`).
4. Late events after a run finished, including an old run's events during the next run of the same tab, must change nothing. Pinned in Task 2 (`ignores late events of its run once run_flow settled`) and Task 3 (`events of a finished run change nothing, in the next run too`).
5. Id reuse: every run, including one started by Ctrl+Enter, gets a fresh id, and an id is free again only when its run is neither in flight nor kept. Pinned in Task 2 (`uses a new id for every run`), Task 3 (`runs started with Ctrl+Enter get their own ids too`) and Task 1 (the in-flight and kept-run tests).

---

## Decisions

- The id travels in a `RunFlowOptions` object, the sixth `runFlow` argument, so P20 adds `partial` to the same object. Rust mirrors it with `FlowRunOptions { run_id, partial }` and one entry point, `FlowExecutionService::run_with_options`. `run_with_auth` and `run_partial` stay as thin wrappers, so the about 60 existing Rust call sites and 15 `RunFlowInput` literals stay unchanged.
- The id is minted after `onBeforeRun` (save) and `onPrepareAuth` (sign-in) succeed. A cancelled or failed pre-run leaves no id anywhere.
- The id is stored on the tab as a new `pendingRunId`, not in `runId`. `runId` keeps the last run until `flow-run-started`, so a refused start keeps the last results, and P20's base run stays intact.
- Stop is enabled from the moment the id is sent (the toolbar sets `activeRunId` then, not on `flow-run-started`). The backend reserves the id before it fetches secrets and tokens, so a Stop during that phase marks the run cancelled and it ends before its first node. The fetch itself still runs to its end (F-05). A Stop sent in the few milliseconds before `run_flow` reaches the backend is a no-op, as today.
- Backend id rule: 1 to 64 ASCII letters, digits, `-` or `_` (a UUID is 36 characters, a ULID 26). A malformed id is refused with `InvalidInput`, and the message does not quote it. An id in flight or kept in the run cache is refused with `AlreadyExists`. To close the gap between those two places, a run is now cached before it leaves `in_flight` (`drop(registration)` moves after `remember_run`).
- Matching is by id only. A step event of the tab's own id is applied even if it arrives before `flow-run-started`. The summary stays the final word either way.
- `cancel_flow_run` is unchanged. It already takes the run id, and the toolbar is its only caller.

## What P20 needs from this plan

- TS: `RunFlowOptions { runId?: string }` exists in `src/lib/tauri-api.ts`, and `runFlow`'s sixth parameter is `options?: RunFlowOptions`. P20 Task 1 Step 9 adds `partial?: FlowPartialRunRequest;` to this interface instead of declaring it, and adds `...(options?.partial ? { partial: options.partial } : {})` after the `runId` spread. It must not redefine the parameter list.
- Toolbar: `handleRun` makes one call, `runFlow(collection, flowName, environmentName, globalEnvName ?? null, tokens, { runId })`. P20 Task 2 Step 7 item 5 becomes: build `const options: RunFlowOptions = partial ? { runId, partial } : { runId };` and pass it, instead of the three-branch statement.
- P20 tests: `FlowToolbar.test.tsx` mocks `newFlowRunId` to return `'run-123'`. P20's expected options are `{ runId: 'run-123', partial: { ... } }`, and its `passes the partial info` test must send `run_id: 'run-123'`. `FlowPane.partialRun.test.tsx` must add `vi.mock('@/lib/flow-run-id', () => ({ newFlowRunId: vi.fn(() => 'run-1') }))`, and `mock.calls[0][5]` becomes `{ runId: 'run-1', partial: { ... } }`.
- Rust: `RunFlowInputDto::take_options()` returns `FlowRunOptions`. P20 adds `partial: self.partial.take().map(PartialRun::from)` there, and `run_flow` already calls `flow_exec.run_with_options(&exec, run_input, tokens, options)`, so P20's `match partial` dispatch is not needed.
- A refused start (P20's partial refusal) never touched `tab.runId`, because the new id lives in `pendingRunId` until `flow-run-started`. P20's `onRunStateChange('done', partial.baseRunId)` in the catch block still works, and `setFlowRunState` clears `pendingRunId`.
- `FlowRunStarted.partial` matching needs nothing extra: the started handler only accepts the tab's own id.
- Mark the P20 "BLOCKED" note as resolved (Task 3 Step 6).

## What F-05 can build on

- The id exists in the backend before the pre-run phase, and `cancel(run_id)` already works then (`RunRegistration::reserve` runs first). F-05 can pass `cancel_signal` into the vault fetch, `resolve_flow_credentials` and `RunCallbacks::open_all`, and emit its "preparing" event with the same `run_id`.
- The toolbar already has the id and an enabled Stop between send and `flow-run-started`.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-app/src/flow_run_id.rs` (new) | `choose_run_id`: client id format check, ULID fallback. |
| `crates/rocket-app/src/flow_execution_service.rs` (modify) | `FlowRunOptions`, `run_with_options`, `RunRegistration::reserve`, deregister after caching. |
| `crates/rocket-app/src/flow_run_cache.rs` (modify) | `FlowRunCache::contains` becomes non-test. |
| `crates/rocket-app/src/lib.rs` (modify) | `mod flow_run_id`, export `FlowRunOptions`. |
| `crates/rocket-app/src/flow_partial_run_tests.rs` (modify) | End-to-end run id tests (it has the full harness). |
| `src-tauri/src/commands/flow.rs` (modify) | `RunFlowInputDto.run_id`, `take_options`, `run_flow` dispatch, docs. |
| `src/lib/flow-run-id.ts` (new) | `newFlowRunId()`. |
| `src/lib/tauri-api.ts` (modify) | `RunFlowOptions`, `runFlow` sixth parameter. |
| `src/types/pane-types.ts` (modify) | `FlowTab.pendingRunId`. |
| `src/stores/pane-store.ts` (modify) | `setFlowPendingRun`, `setFlowRunState` clears the pending id. |
| `src/components/flow/FlowToolbar.tsx` (modify) | Mint, store and send the id; match by id; ignore late events; resume a pending run. |
| `src/components/flow/FlowPane.tsx` (modify) | Wire `onRunRequested` and `tabPendingRunId`. |
| `src/components/flow/__tests__/FlowPane.twoTabs.test.tsx` (new) | Two tabs of one flow, end to end. |
| `crates/rocket-app/CLAUDE.md` (modify) | Run id rules. |

---

### Task 1: Backend accepts, checks and reserves a client run id

**Files:**
- Create: `crates/rocket-app/src/flow_run_id.rs`
- Modify: `crates/rocket-app/src/lib.rs:18-26` (module list), `:71-73` (exports)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`use ulid::Ulid;` `:687`, `RunFlowInput` `:691-698`, `RunRegistration` `:785-807`, `run_with_auth`/`run_partial`/`run_inner` `:964-1003`, callback comment `:1062-1065`, run id mint `:1082-1083`, `drop(registration)` `:1264-1265`, `remember_run` call `:1282-1293`)
- Modify: `crates/rocket-app/src/flow_run_cache.rs:555-559`
- Modify: `crates/rocket-app/src/flow_partial_run_tests.rs` (doc line 1, new tests at the end)
- Modify: `src-tauri/src/commands/flow.rs` (imports `:1-3`, `RunFlowInputDto` `:474-509`, `run_flow` `:511-526`, `cancel_flow_run` doc `:528-529`, tests `mod tests`)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces (Rust): `pub struct FlowRunOptions { pub run_id: Option<String>, pub partial: Option<PartialRun> }` (`Debug, Clone, Default`), exported from `rocket_app`.
- Produces (Rust): `pub async fn FlowExecutionService::run_with_options(&self, exec: &RequestExecutionService, input: RunFlowInput, auth_tokens: FlowAuthTokens, options: FlowRunOptions) -> DomainResult<FlowRunSummary>`.
- Produces (Rust): `pub fn RunFlowInputDto::take_options(&mut self) -> FlowRunOptions`.
- Produces (IPC): `run_flow` input key `runId?: string`. Errors: `Invalid input: a flow run id must be 1 to 64 letters, digits, '-' or '_'` and `Already exists: flow run id '<id>' is already used by a running or recent run`.

- [ ] **Step 1: Read the spec reference**

Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`. (`run_flow` is an IPC command that carries collection and environment names. This task passes them through unchanged.)

- [ ] **Step 2: Write the failing id rule tests**

Create `crates/rocket-app/src/flow_run_id.rs` with only the tests for now:

```rust
//! Run ids for Flow runs. The client may choose the id, so it can match every
//! event of the run to the tab that asked for it (roadmap F-03).

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainError;
    use ulid::Ulid;

    #[test]
    fn accepts_a_uuid_and_a_ulid() {
        let uuid = "0b7e2c1a-5d1f-4a7e-9c3b-2f6d8e9a1b2c";
        assert_eq!(choose_run_id(Some(uuid.to_string())).expect("uuid"), uuid);
        let ulid = Ulid::new().to_string();
        assert_eq!(choose_run_id(Some(ulid.clone())).expect("ulid"), ulid);
        let longest = "x".repeat(MAX_RUN_ID_LEN);
        assert_eq!(choose_run_id(Some(longest.clone())).expect("64 chars"), longest);
    }

    #[test]
    fn generates_a_ulid_when_the_client_sent_none() {
        let id = choose_run_id(None).expect("generated");
        assert!(Ulid::from_string(&id).is_ok(), "got {id}");
        assert_ne!(choose_run_id(None).expect("second"), id);
    }

    #[test]
    fn rejects_empty_long_and_odd_ids_without_quoting_them() {
        let long = "x".repeat(MAX_RUN_ID_LEN + 1);
        for bad in ["", "has space", "line\nbreak", "../etc", "ü-umlaut", long.as_str()] {
            match choose_run_id(Some(bad.to_string())) {
                Err(DomainError::InvalidInput(message)) => {
                    if !bad.is_empty() {
                        assert!(!message.contains(bad), "{bad:?} quoted in {message}");
                    }
                }
                other => panic!("{bad:?} must be refused, got {other:?}"),
            }
        }
    }
}
```

In `crates/rocket-app/src/lib.rs`, add after `pub(crate) mod flow_run_cache;`:

```rust
pub(crate) mod flow_run_id;
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app flow_run_id`
Expected: FAIL to compile (`cannot find function choose_run_id`, `cannot find value MAX_RUN_ID_LEN`).

- [ ] **Step 4: Implement the id rule**

In `crates/rocket-app/src/flow_run_id.rs`, insert between the module doc and `#[cfg(test)]`:

```rust
use rocket_shared::error::{DomainError, DomainResult};
use ulid::Ulid;

/// Longest run id a client may choose. A UUID has 36 characters.
pub(crate) const MAX_RUN_ID_LEN: usize = 64;

/// The client's run id when it is well formed, or a new ULID when the client
/// sent none. Whether the id is free is checked when the run registers.
pub(crate) fn choose_run_id(requested: Option<String>) -> DomainResult<String> {
    let Some(id) = requested else {
        return Ok(Ulid::new().to_string());
    };
    if is_well_formed(&id) {
        Ok(id)
    } else {
        // The id is not quoted, because it can hold anything.
        Err(DomainError::InvalidInput(format!(
            "a flow run id must be 1 to {MAX_RUN_ID_LEN} letters, digits, '-' or '_'"
        )))
    }
}

fn is_well_formed(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_RUN_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
```

In the test module, the `use rocket_shared::error::DomainError;` and `use ulid::Ulid;` lines are now also brought in by `use super::*;`. Remove both lines from the test module so there is no duplicate import warning.

- [ ] **Step 5: Run them to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_run_id`
Expected: PASS (3 tests).

- [ ] **Step 6: Write the failing end-to-end tests**

In `crates/rocket-app/src/flow_partial_run_tests.rs`, change the first line from

```rust
//! End-to-end tests for partial runs ("Run this node", "Run from here").
```

to

```rust
//! End-to-end tests for partial runs ("Run this node", "Run from here") and
//! for client-chosen run ids.
```

Append at the end of the file:

```rust
// ---- Client-chosen run ids (roadmap F-03) ----

fn with_id(id: &str) -> FlowRunOptions {
    FlowRunOptions {
        run_id: Some(id.to_string()),
        partial: None,
    }
}

/// The run id of every Flow event, in order.
fn flow_event_run_ids(events: &RecordingPublisher) -> Vec<String> {
    events
        .events()
        .into_iter()
        .filter_map(|e| match e {
            DomainEvent::FlowRunStarted { run_id, .. }
            | DomainEvent::FlowStepStarted { run_id, .. }
            | DomainEvent::FlowStepProgress { run_id, .. }
            | DomainEvent::FlowStepCompleted { run_id, .. }
            | DomainEvent::FlowRunFinished { run_id, .. } => Some(run_id),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn client_run_id_names_the_run_in_every_event_and_the_summary() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    let id = "0b7e2c1a-5d1f-4a7e-9c3b-2f6d8e9a1b2c";

    let summary = h
        .service
        .run_with_options(&h.exec, input(), FlowAuthTokens::new(), with_id(id))
        .await
        .expect("run");

    assert_eq!(summary.run_id, id);
    let ids = flow_event_run_ids(&h.events);
    assert!(ids.len() >= 4, "started, two steps and finished: {ids:?}");
    assert!(ids.iter().all(|e| e == id), "{ids:?}");
}

#[tokio::test]
async fn client_run_id_absent_falls_back_to_a_generated_ulid() {
    let h = harness(login_then_b());

    let summary = h
        .service
        .run_with_options(&h.exec, input(), FlowAuthTokens::new(), FlowRunOptions::default())
        .await
        .expect("run");

    assert!(
        ulid::Ulid::from_string(&summary.run_id).is_ok(),
        "got {}",
        summary.run_id
    );
}

#[tokio::test]
async fn client_run_id_that_is_malformed_is_refused_before_any_event() {
    let h = harness(login_then_b());
    let long = "x".repeat(65);

    for bad in ["", "has space", "line\nbreak", "../etc", long.as_str()] {
        let err = h
            .service
            .run_with_options(&h.exec, input(), FlowAuthTokens::new(), with_id(bad))
            .await
            .expect_err("a malformed id must be refused");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{bad:?}: {err:?}");
    }

    assert!(h.events.events().is_empty(), "a refused id sends no event");
    assert!(h.http.sent_urls().is_empty(), "a refused id sends no request");
    assert!(h.service.in_flight.lock().expect("lock").is_empty());
}

#[tokio::test]
async fn client_run_id_of_a_run_in_flight_is_refused_and_free_after_it_ends() {
    let h = harness(login_then_b());
    let held = RunRegistration::reserve(&h.service, "live-1").expect("reserve");

    let err = h
        .service
        .run_with_options(&h.exec, input(), FlowAuthTokens::new(), with_id("live-1"))
        .await
        .expect_err("an id in flight must be refused");

    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err:?}");
    assert!(h.events.events().is_empty(), "a refused id sends no event");
    assert!(
        h.service.in_flight.lock().expect("lock").contains("live-1"),
        "a refused duplicate must not unregister the run that owns the id"
    );

    drop(held);
    let summary = h
        .service
        .run_with_options(&h.exec, input(), FlowAuthTokens::new(), with_id("live-1"))
        .await
        .expect("the id is free once its run ended without being kept");
    assert_eq!(summary.run_id, "live-1");
}

#[tokio::test]
async fn client_run_id_of_a_kept_run_is_refused_and_the_kept_run_is_untouched() {
    let h = harness(login_then_b());
    h.http.set_body("/login", "https://api.example.com/profile");
    h.service
        .run_with_options(&h.exec, input(), FlowAuthTokens::new(), with_id("kept-1"))
        .await
        .expect("first run");
    let events_before = h.events.events().len();
    let ids_before = flow_event_run_ids(&h.events).len();

    let err = h
        .service
        .run_with_options(&h.exec, input(), FlowAuthTokens::new(), with_id("kept-1"))
        .await
        .expect_err("the id of a kept run must be refused");
    assert!(matches!(err, DomainError::AlreadyExists(_)), "{err:?}");
    assert_eq!(h.events.events().len(), events_before, "no event for a refused id");

    // The kept run still feeds a partial run, which takes its own client id.
    let summary = h
        .service
        .run_with_options(
            &h.exec,
            input(),
            FlowAuthTokens::new(),
            FlowRunOptions {
                run_id: Some("partial-1".to_string()),
                partial: Some(partial("kept-1", "b", FlowPartialMode::Node)),
            },
        )
        .await
        .expect("partial run on the kept run");
    assert_eq!(summary.run_id, "partial-1");
    assert_eq!(
        summary.partial.expect("partial info").base_run_id,
        "kept-1"
    );
    let partial_ids: Vec<String> = flow_event_run_ids(&h.events)
        .into_iter()
        .skip(ids_before)
        .collect();
    assert!(!partial_ids.is_empty());
    assert!(partial_ids.iter().all(|e| e == "partial-1"), "{partial_ids:?}");
}

#[tokio::test]
async fn client_run_id_stop_cancels_only_that_run() {
    let h = harness(login_then_b());
    let (_a, signal_a) = RunRegistration::reserve(&h.service, "run-a").expect("reserve a");
    let (_b, signal_b) = RunRegistration::reserve(&h.service, "run-b").expect("reserve b");

    h.service.cancel("run-b");

    assert!(signal_b.is_cancelled());
    assert!(!signal_a.is_cancelled());
    assert!(h.service.is_cancelled("run-b"));
    assert!(!h.service.is_cancelled("run-a"));
}
```

- [ ] **Step 7: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app client_run_id`
Expected: FAIL to compile (`FlowRunOptions`, `run_with_options` and `RunRegistration::reserve` not found).

- [ ] **Step 8: Add `FlowRunOptions` and `run_with_options`**

In `crates/rocket-app/src/flow_execution_service.rs`:

1. Remove the import line `use ulid::Ulid;` (the only other use was the mint removed below; the id now comes from `flow_run_id`).

2. After the `RunFlowInput` struct (ends with `pub global_env_name: Option<String>,\n}`), add:

```rust
/// How to run a flow, beyond what to run. Not an IPC DTO.
#[derive(Debug, Clone, Default)]
pub struct FlowRunOptions {
    /// The run id the client chose, so it can match every event to the tab
    /// that asked. `None` makes the service generate a ULID. A malformed id,
    /// or one a running or kept run uses, is refused before anything runs.
    pub run_id: Option<String>,
    /// Set for "Run this node" or "Run from here".
    pub partial: Option<PartialRun>,
}
```

3. Replace the bodies of `run_with_auth` and `run_partial`, and the header of `run_inner`. Replace

```rust
    ) -> DomainResult<FlowRunSummary> {
        self.run_inner(exec, input, auth_tokens, None).await
    }
```

with

```rust
    ) -> DomainResult<FlowRunSummary> {
        self.run_with_options(exec, input, auth_tokens, FlowRunOptions::default())
            .await
    }
```

replace

```rust
    ) -> DomainResult<FlowRunSummary> {
        self.run_inner(exec, input, auth_tokens, Some(partial)).await
    }

    /// The run loop shared by full and partial runs.
    async fn run_inner(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
        auth_tokens: FlowAuthTokens,
        partial: Option<PartialRun>,
    ) -> DomainResult<FlowRunSummary> {
        let (flow, order) = self.load_ordered_nodes(&input.collection, &input.flow_name)?;
```

with

```rust
    ) -> DomainResult<FlowRunSummary> {
        let options = FlowRunOptions {
            run_id: None,
            partial: Some(partial),
        };
        self.run_with_options(exec, input, auth_tokens, options).await
    }

    /// The run loop shared by full and partial runs. The run id from
    /// `options` (or a generated one) names the run in every event, in
    /// `cancel`, in the run cache and in the summary.
    pub async fn run_with_options(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
        auth_tokens: FlowAuthTokens,
        options: FlowRunOptions,
    ) -> DomainResult<FlowRunSummary> {
        let FlowRunOptions { run_id, partial } = options;
        let run_id = crate::flow_run_id::choose_run_id(run_id)?;
        // Reserved before anything else, so a duplicate id is refused at once,
        // and a Stop sent while secrets and tokens are fetched is kept and
        // stops the run before its first node.
        let (registration, cancel_signal) = RunRegistration::reserve(self, &run_id)?;
        let (flow, order) = self.load_ordered_nodes(&input.collection, &input.flow_name)?;
```

4. Replace the callback comment

```rust
        // Open every callback endpoint before the run is registered or
        // announced. A failure here ends the call with no events and nothing
        // left in `in_flight`. `callbacks` lives until `run` returns, so
        // every endpoint closes on every exit path.
```

with

```rust
        // Open every callback endpoint before the run is announced. A failure
        // here ends the call with no events, and the registration guard
        // leaves nothing in `in_flight`. `callbacks` lives until `run`
        // returns, so every endpoint closes on every exit path.
```

5. Delete these two lines (the id and registration now exist from the top):

```rust
        let run_id = Ulid::new().to_string();
        let (registration, cancel_signal) = RunRegistration::new(self, &run_id);
```

6. Move the deregistration after the cache write. Replace

```rust
        // Deregister before `FlowRunFinished`, as before this guard existed.
        drop(registration);

        // Every value this run masked,
```

with

```rust
        // Every value this run masked,
```

and replace

```rust
            &fingerprints,
        );

        let failed_count = steps
```

with

```rust
            &fingerprints,
        );
        // Deregister after the run is cached and before `FlowRunFinished`. A
        // kept run is never out of both places, so its id cannot be reused.
        drop(registration);

        let failed_count = steps
```

- [ ] **Step 9: Replace `RunRegistration::new` with `reserve`**

Replace the whole `impl<'a> RunRegistration<'a> { fn new(...) ... }` block with:

```rust
impl<'a> RunRegistration<'a> {
    /// Registers `run_id` as in flight. Refuses an id that a running run or a
    /// kept run already uses, so two runs never share events, Stop or a run
    /// cache entry.
    fn reserve(
        service: &'a FlowExecutionService,
        run_id: &str,
    ) -> DomainResult<(Self, CancelSignal)> {
        let in_use = || {
            DomainError::AlreadyExists(format!(
                "flow run id '{run_id}' is already used by a running or recent run"
            ))
        };
        // A finished run is cached before it leaves `in_flight`, so checking
        // the cache first leaves no moment where a used id passes both checks.
        let kept = service
            .run_cache
            .lock()
            .is_ok_and(|cache| cache.contains(run_id));
        if kept {
            return Err(in_use());
        }
        let mut in_flight = service.in_flight.lock().map_err(|_| {
            DomainError::Internal("the flow run registry is unavailable".to_string())
        })?;
        if !in_flight.insert(run_id.to_string()) {
            return Err(in_use());
        }
        let (handle, signal) = cancel_pair();
        // Still under the `in_flight` lock, so a Stop for this id always finds
        // its handle. `cancel` never holds two of these locks at once.
        if let Ok(mut handles) = service.cancel_handles.lock() {
            handles.insert(run_id.to_string(), handle);
        }
        drop(in_flight);
        let registration = Self {
            service,
            run_id: run_id.to_string(),
        };
        Ok((registration, signal))
    }
}
```

The run id is echoed in the `AlreadyExists` message only after `choose_run_id` checked its characters.

In `crates/rocket-app/src/flow_run_cache.rs`, replace

```rust
    /// Whether a run is kept, without touching its place in the order.
    #[cfg(test)]
    pub(crate) fn contains(&self, run_id: &str) -> bool {
```

with

```rust
    /// Whether a run is kept, without touching its place in the order. A new
    /// run may not reuse a kept run's id.
    pub(crate) fn contains(&self, run_id: &str) -> bool {
```

In `crates/rocket-app/src/lib.rs`, change the export to:

```rust
pub use flow_execution_service::{
    CapturedOutput, FlowExecutionService, FlowRunOptions, FlowRunSummary, FlowStepResult,
    RunFlowInput,
};
```

- [ ] **Step 10: Run the rocket-app tests**

Run: `cargo test -j4 -p rocket-app client_run_id && cargo test -j4 -p rocket-app flow`
Expected: PASS. The existing cancel tests (`cancelling_mid_run_keeps_completed_steps_and_runs_nothing_further`, `a_listener_failure_fails_the_run_before_it_starts`) still assert an empty `in_flight`, `cancelled` and `cancel_handles` after the run, which the guard still guarantees.

- [ ] **Step 11: Write the failing DTO tests**

In `src-tauri/src/commands/flow.rs`, inside `mod tests`, after `run_flow_input_without_auth_tokens_has_none`, add:

```rust
    #[test]
    fn run_flow_input_carries_the_client_run_id_into_the_options() {
        let json = r#"{
            "collection": "c", "flowName": "f", "environmentName": null, "globalEnvName": null,
            "runId": "0b7e2c1a-5d1f-4a7e-9c3b-2f6d8e9a1b2c"
        }"#;
        let mut dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");

        let options = dto.take_options();

        assert_eq!(
            options.run_id.as_deref(),
            Some("0b7e2c1a-5d1f-4a7e-9c3b-2f6d8e9a1b2c")
        );
        assert!(options.partial.is_none());
        assert!(dto.run_id.is_none(), "the id is taken, not copied");
    }

    #[test]
    fn run_flow_input_without_run_id_lets_the_backend_choose() {
        let json =
            r#"{"collection":"c","flowName":"f","environmentName":null,"globalEnvName":null}"#;
        let mut dto: RunFlowInputDto = serde_json::from_str(json).expect("deserialize");
        assert!(dto.take_options().run_id.is_none());
    }
```

- [ ] **Step 12: Run them to verify they fail**

Run: `cargo test -j4 -p rocket run_flow_input`
Expected: FAIL to compile (`run_id` field and `take_options` missing).

- [ ] **Step 13: Add the DTO field and the dispatch**

In `src-tauri/src/commands/flow.rs`, change the first import to:

```rust
use rocket_app::{
    FlowExecutionService, FlowRunOptions, FlowRunSummary, FlowService, RequestExecutionService,
    RunFlowInput,
};
```

Add to `RunFlowInputDto`, after `auth_tokens`:

```rust
    /// The run id the frontend chose. Absent means the backend picks one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
```

Add to `impl RunFlowInputDto`, before `into_parts`:

```rust
    /// Takes what the run needs besides the input and the tokens. Call it
    /// before `into_parts`.
    pub fn take_options(&mut self) -> FlowRunOptions {
        FlowRunOptions {
            run_id: self.run_id.take(),
            partial: None,
        }
    }
```

Replace the `run_flow` doc comment and function with:

```rust
/// Runs a Flow to completion. Streams `flow-run-started`, `flow-step-*` and
/// `flow-run-finished` events while it runs and returns the same data as one
/// summary when the run ends, mirroring `run_collection` in `runner.rs`. The
/// frontend chooses the run id (`runId`) and matches every event by it, so
/// Stop works before the run finishes and two tabs of one flow never share a
/// run. Without `runId` the backend picks one.
#[tauri::command]
pub async fn run_flow(
    mut input: RunFlowInputDto,
    flow_exec: State<'_, FlowExecutionService>,
    exec: State<'_, RequestExecutionService>,
) -> Result<FlowRunSummary, DomainError> {
    let options = input.take_options();
    let (run_input, tokens) = input.into_parts();
    flow_exec
        .run_with_options(&exec, run_input, tokens, options)
        .await
}
```

Change the `cancel_flow_run` doc comment to:

```rust
/// Asks an in-progress Flow run to stop. `run_id` is the id the frontend
/// sent with `run_flow`. An unknown or already-finished run id is a no-op,
/// matching `stop_collection_run`'s existing behavior.
```

- [ ] **Step 14: Run the Rust gates**

Run: `cargo test -j4 -p rocket run_flow_input && cargo check -j4 -p rocket-app -p rocket && cargo clippy -j4 -p rocket-app -p rocket`
Expected: tests PASS, no errors, and no new clippy warning in the touched files.

- [ ] **Step 15: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-app/src/flow_run_id.rs crates/rocket-app/src/lib.rs crates/rocket-app/src/flow_execution_service.rs crates/rocket-app/src/flow_run_cache.rs crates/rocket-app/src/flow_partial_run_tests.rs src-tauri/src/commands/flow.rs`
Suggested subject: `feat(flow): accept a client-chosen run id`.

---

### Task 2: The toolbar chooses, stores and matches the run id

**Files:**
- Create: `src/lib/flow-run-id.ts`
- Modify: `src/lib/tauri-api.ts` (`runFlow` `:2343-2365`)
- Test: `src/lib/queries/__tests__/flow-api.test.ts` (extend)
- Modify: `src/types/pane-types.ts` (`FlowTab`, after `runId?: string;`)
- Modify: `src/stores/pane-store.ts` (interface `:394`, `setFlowRunState` `:1191-1214`)
- Test: `src/stores/__tests__/pane-store.flowRun.test.ts` (extend)
- Modify: `src/components/flow/FlowToolbar.tsx` (props, refs, `resumedRunId`, resume effect, `handleRun` from `cleanupListeners();` to the end of `finally`)
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx` (mock, existing expectations, new tests)
- Modify: `src/components/flow/FlowPane.tsx` (store selectors `:70-73`, `<FlowToolbar` props `:484-486`)
- Test: `src/components/flow/__tests__/FlowPane.test.tsx`, `src/components/flow/__tests__/FlowPane.runHistoryRecord.test.tsx` (adapt to the client id)

**Interfaces:**
- Consumes: IPC input key `runId` from Task 1.
- Produces: `newFlowRunId(): string` (`src/lib/flow-run-id.ts`).
- Produces: `interface RunFlowOptions { runId?: string }`; `runFlow(collection, flowName, environmentName?, globalEnvName?, authTokens?, options?: RunFlowOptions)`.
- Produces: `FlowTab.pendingRunId?: string`; store action `setFlowPendingRun(tabId: string, runId: string | undefined): void`; `setFlowRunState` clears `pendingRunId`.
- Produces: `FlowToolbar` props `onRunRequested?: (runId: string) => void` and `tabPendingRunId?: string`.

- [ ] **Step 1: Write the failing API test**

In `src/lib/queries/__tests__/flow-api.test.ts`, after the test `runFlow leaves authTokens out when the map is empty`, add:

```ts
  it('runFlow sends the run id the client chose', async () => {
    vi.mocked(invoke).mockResolvedValue({ runId: 'r', steps: [], stoppedReason: 'completed' });
    const { runFlow } = await import('@/lib/tauri-api');
    await runFlow('my-collection', 'My Flow', null, null, undefined, { runId: 'run-abc' });
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: {
        collection: 'my-collection',
        flowName: 'My Flow',
        environmentName: null,
        globalEnvName: null,
        runId: 'run-abc',
      },
    });
  });
```

Run: `yarn test src/lib/queries/__tests__/flow-api.test.ts`
Expected: the new test FAILS (`runId` is not sent).

- [ ] **Step 2: Add `RunFlowOptions` and the parameter**

In `src/lib/tauri-api.ts`, replace the `runFlow` doc comment and declaration (from `/**\n * Runs a flow.` to the closing `});` of `runFlow`) with:

```ts
/** Extra settings for one run_flow call. */
export interface RunFlowOptions {
  /**
   * Run id chosen by the client, a UUID. The backend uses it in every
   * flow-run-* event and for Stop, so a tab matches only its own run. The
   * backend picks one when absent and refuses a malformed or used id.
   */
  runId?: string;
}

/**
 * Runs a flow. The promise resolves only when the run ENDS. Subscribe to
 * the flow-run-* events before calling this, and match them by
 * `options.runId`.
 */
export const runFlow = (
  collection: string,
  flowName: string,
  environmentName?: string | null,
  globalEnvName?: string | null,
  authTokens?: Record<string, FlowAuthToken>,
  options?: RunFlowOptions,
) =>
  invoke<FlowRunSummary>('run_flow', {
    input: {
      collection,
      flowName,
      environmentName: environmentName ?? null,
      globalEnvName: globalEnvName ?? null,
      // Sent only when there is something to send, so a flow without Auth nodes
      // calls the command exactly as before.
      ...(authTokens && Object.keys(authTokens).length > 0 ? { authTokens } : {}),
      // Sent only when chosen, so other callers keep the old payload.
      ...(options?.runId ? { runId: options.runId } : {}),
    },
  });
```

Create `src/lib/flow-run-id.ts`:

```ts
/**
 * Returns a new id for one flow run. The toolbar sends it with run_flow and
 * matches every flow-run-* event by it, so two tabs of one flow never share
 * a run. It has a module of its own, so tests can choose the id.
 */
export function newFlowRunId(): string {
  return crypto.randomUUID();
}
```

Run: `yarn test src/lib/queries/__tests__/flow-api.test.ts`
Expected: PASS.

- [ ] **Step 3: Write the failing store tests**

In `src/stores/__tests__/pane-store.flowRun.test.ts`, append at the end of the file:

```ts
describe('pane-store pending flow run', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore
      .getState()
      .openTab({ ...flowTab, nodeStatus: { a: 'success' }, runState: 'done', runId: 'run-0' });
  });

  it('stores the pending id and keeps the last run and its results', () => {
    usePaneStore.getState().setFlowPendingRun(tabId, 'run-1');
    expect(stored().pendingRunId).toBe('run-1');
    expect(stored().runId).toBe('run-0');
    expect(stored().runState).toBe('done');
    expect(stored().nodeStatus).toEqual({ a: 'success' });
  });

  it('every run state change clears the pending id', () => {
    usePaneStore.getState().setFlowPendingRun(tabId, 'run-1');
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1');
    expect(stored().pendingRunId).toBeUndefined();
    usePaneStore.getState().setFlowPendingRun(tabId, 'run-2');
    usePaneStore.getState().setFlowRunState(tabId, 'done');
    expect(stored().pendingRunId).toBeUndefined();
  });

  it('reaches a tab parked in a collection snapshot', () => {
    usePaneStore.setState({
      collectionTabState: { other: { tabs: [{ ...flowTab, id: 'parked' }], activeTabId: 'parked' } },
    });
    usePaneStore.getState().setFlowPendingRun('parked', 'run-3');
    const parked = usePaneStore.getState().collectionTabState.other?.tabs[0];
    expect(parked && isFlowTab(parked) ? parked.pendingRunId : null).toBe('run-3');
  });
});
```

Run: `yarn test src/stores/__tests__/pane-store.flowRun.test.ts`
Expected: the three new tests FAIL (`setFlowPendingRun is not a function`).

- [ ] **Step 4: Add the tab field and the store action**

In `src/types/pane-types.ts`, inside `FlowTab`, after `runId?: string;`, add:

```ts
  /**
   * Id of a run this tab sent whose flow-run-started has not arrived yet.
   * Lets Stop and a remounted toolbar find the run. Cleared on any run state change.
   */
  pendingRunId?: string;
```

In `src/stores/pane-store.ts`, add to the store interface after the `setFlowRunState` line:

```ts
  /** Remembers the id of a run the tab sent, until its run state changes. */
  setFlowPendingRun: (tabId: string, runId: string | undefined) => void;
```

Replace the body of `setFlowRunState` so both returns clear the pending id:

```ts
  setFlowRunState(tabId, runState, runId) {
    // Also updates a tab parked by switchCollection, so a run that ends there leaves no stale URL.
    set(
      updateTabEverywhere(get(), tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        // A new run starts from a clean canvas. Otherwise the last run's
        // results stay on nodes this run skips or never reaches. Callback URLs
        // work only while their run is active, so every change drops them.
        // The pending id has done its job once the run state moves.
        if (runState === 'running') {
          return {
            ...tab,
            runState,
            runId,
            pendingRunId: undefined,
            nodeStatus: {},
            nodeDetail: {},
            lastRun: undefined,
            viewedRunId: null,
            callbackUrls: undefined,
          };
        }
        return { ...tab, runState, runId, pendingRunId: undefined, callbackUrls: undefined };
      }),
    );
  },

  // Keeps the last run and its results. Parked tabs get it too, like the run state.
  setFlowPendingRun(tabId, runId) {
    set(
      updateTabEverywhere(get(), tabId, (tab) =>
        isFlowTab(tab) ? { ...tab, pendingRunId: runId } : tab,
      ),
    );
  },
```

Run: `yarn test src/stores/__tests__/pane-store.flowRun.test.ts`
Expected: PASS.

- [ ] **Step 5: Adapt the existing toolbar tests to a fixed client id**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`:

1. Add after the `import * as tauriApi from '@/lib/tauri-api';` line:

```ts
import { newFlowRunId } from '@/lib/flow-run-id';
```

and after the `vi.mock('sonner', ...)` line:

```ts
// The toolbar's run id. Tests send events with this id.
vi.mock('@/lib/flow-run-id', () => ({ newFlowRunId: vi.fn() }));
```

2. In the top-level `beforeEach`, add as the first two lines:

```ts
    vi.mocked(newFlowRunId).mockReset();
    vi.mocked(newFlowRunId).mockReturnValue('run-123');
```

3. Update every `runFlow` call expectation to the six-argument form:
   - Test `subscribes before running, takes the run id from flow-run-started, and finishes on resolve`: rename it to `subscribes before running, sends its own run id, and finishes on resolve`, and change its expectation to `expect(tauriApi.runFlow).toHaveBeenCalledWith('my-collection', 'my-flow', null, null, undefined, { runId: 'run-123' })`.
   - Test `forwards the active global environment name to runFlow when set`: expected arguments become `'my-collection', 'my-flow', null, 'shared-global', undefined, { runId: 'run-123' }`.
   - Test `reads the global environment name fresh at click-time, not from an earlier render`: expected arguments become `'my-collection', 'my-flow', null, 'fresh-global', undefined, { runId: 'run-123' }`.
   - Test `passes the tokens from onPrepareAuth to runFlow`, and the test in `describe('pending sign-in')` that expects `{ a: { accessToken: 'tok-123456' } }` after a second sign-in: expected arguments become `'my-collection', 'my-flow', null, null, { a: { accessToken: 'tok-123456' } }, { runId: 'run-123' }`.
   - Test `keeps the four-argument runFlow call when there are no tokens`: rename it to `sends no tokens when there are none`, expected arguments `'my-collection', 'my-flow', null, null, undefined, { runId: 'run-123' }`.

4. Replace the test `ignores a flow-run-started event for a different flow` with:

```tsx
  it('ignores a flow-run-started event for another run of the same flow', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
    // Same collection and flow name, another tab's run.
    started('run-other');
    expect(onRunStateChange).not.toHaveBeenCalled();
    started('run-123');
    expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123');
  });
```

5. Tests that start a run and send `run-1`:
   - In `Stop is enabled once a run is active`, `does not call onCallbackUrls for a run without callbacks` and `starts only one run when pressed twice, and none while running`, change `started('run-1');` to `started('run-123');`.
   - In `hands the callback URLs from flow-run-started to onCallbackUrls`, change `run_id: 'run-1',` to `run_id: 'run-123',` and `toHaveBeenCalledWith('running', 'run-1')` to `toHaveBeenCalledWith('running', 'run-123')`.
   - In `describe('run result', ...)`, add as its first statement:

```ts
    beforeEach(() => {
      vi.mocked(newFlowRunId).mockReturnValue('run-1');
    });
```

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: FAIL. The six-argument expectations, `ignores a flow-run-started event for another run of the same flow` and the run-result tests fail, because the toolbar still adopts any run of its flow and calls `runFlow` with four or five arguments.

- [ ] **Step 6: Write the failing new toolbar tests**

In the same file, add before the closing `});` of the top-level `describe('FlowToolbar', ...)`:

```tsx
  describe('client run id', () => {
    const lateStep = (runId: string) =>
      stepHandler?.({
        type: 'flowStepCompleted',
        run_id: runId,
        node_id: 'node-a',
        status: 'failed',
        status_code: null,
        duration_ms: null,
        error: 'late',
        value: null,
      });

    it('stores its run id on the tab before the run is sent', async () => {
      const onRunRequested = vi.fn();
      renderToolbar({ onRunRequested });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      expect(onRunRequested).toHaveBeenCalledWith('run-123');
      expect(onRunRequested.mock.invocationCallOrder[0]).toBeLessThan(
        vi.mocked(tauriApi.runFlow).mock.invocationCallOrder[0],
      );
    });

    it('Stop cancels its own run before flow-run-started arrives', async () => {
      renderToolbar();
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
      expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-123');
    });

    it('ignores late events of its run once run_flow settled', async () => {
      renderToolbar();
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      started('run-123');
      resolveRun({ runId: 'run-123', steps: [], stoppedReason: 'completed' });
      await waitFor(() => expect(onRunStateChange).toHaveBeenCalledWith('done', 'run-123'));
      onPatchStatus.mockClear();
      // The fake unlisten keeps the handler, like an event already queued.
      lateStep('run-123');
      expect(onPatchStatus).not.toHaveBeenCalled();
    });

    it('uses a new id for every run', async () => {
      vi.mocked(newFlowRunId).mockReturnValueOnce('run-a').mockReturnValueOnce('run-b');
      renderToolbar();
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(1));
      started('run-a');
      resolveRun({ runId: 'run-a', steps: [], stoppedReason: 'completed' });
      await waitFor(() => expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled());
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(2));
      expect(vi.mocked(tauriApi.runFlow).mock.calls[0][5]).toEqual({ runId: 'run-a' });
      expect(vi.mocked(tauriApi.runFlow).mock.calls[1][5]).toEqual({ runId: 'run-b' });
    });

    it('a remounted toolbar follows a run that has not announced itself yet', async () => {
      const onCallbackUrls = vi.fn();
      renderToolbar({
        tabRunState: 'done',
        tabRunId: 'run-0',
        tabPendingRunId: 'run-7',
        onCallbackUrls,
      });
      expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-8');
      expect(onRunStateChange).not.toHaveBeenCalled();
      startedHandler?.({
        type: 'flowRunStarted',
        run_id: 'run-7',
        flow_name: 'my-flow',
        collection: 'my-collection',
        total_nodes: 1,
        callbacks: [{ nodeId: 'w', name: 'payment', url: 'http://h:1/cb/tok' }],
      });
      expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-7');
      expect(onCallbackUrls).toHaveBeenCalledWith({ w: 'http://h:1/cb/tok' });
      await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
      expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-7');
    });
  });
```

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: the five new tests FAIL as well, and `yarn tsc --noEmit` reports `onRunRequested` and `tabPendingRunId` as unknown props.

- [ ] **Step 7: Teach the toolbar the client id**

In `src/components/flow/FlowToolbar.tsx`:

1. Add the import after the `@/lib/flow-run-result` import:

```tsx
import { newFlowRunId } from '@/lib/flow-run-id';
```

2. In `FlowToolbarProps`, after `tabRunId?: string;`, add:

```tsx
  // Id of a run this tab sent whose flow-run-started has not arrived yet.
  tabPendingRunId?: string;
  // Receives the run id the toolbar chose, right before the run is sent. The
  // tab stores it, so Stop and a remounted toolbar know the run early.
  onRunRequested?: (runId: string) => void;
```

and add `tabPendingRunId,` and `onRunRequested,` to the destructured parameters after `tabRunId,`.

3. Replace

```tsx
  // A run started by an earlier mount of this toolbar, still in progress.
  const resumedRunId =
    activeRunId === null && tabRunState === 'running' ? (tabRunId ?? null) : null;
```

with

```tsx
  // A run started by an earlier mount of this toolbar, still in progress. A
  // run that has not announced itself yet is found by the tab's pending id.
  let resumedRunId: string | null = null;
  if (activeRunId === null) {
    resumedRunId = tabRunState === 'running' ? (tabRunId ?? null) : (tabPendingRunId ?? null);
  }
```

4. After `onRunStateChangeRef.current = onRunStateChange;`, add:

```tsx
  const onCallbackUrlsRef = useRef(onCallbackUrls);
  onCallbackUrlsRef.current = onCallbackUrls;
```

5. Replace the first three lines of the `isStartingRef` comment

```tsx
  // A ref, not state: `activeRunId` is only set once the `flow-run-started`
  // event round-trips through the backend, so between a click and that
  // event the Run button's `disabled` prop alone does not prevent a second,
```

with

```tsx
  // A ref, not state: `activeRunId` is only set after the save, the sign-in
  // and the event subscriptions, which all await, so between a click and that
  // point the Run button's `disabled` prop alone does not prevent a second,
```

6. Replace the whole resume effect (from `  // Keep streaming step results for a run this mount did not start.` to `  }, [resumedRunId]);`) with:

```tsx
  // Keep streaming step results for a run this mount did not start. The
  // mount that started it still applies the final summary when it ends. A
  // run found by its pending id is marked running when it announces itself.
  useEffect(() => {
    // The mount that is starting a run follows it itself.
    if (!resumedRunId || isStartingRef.current) return;
    let unlistenRun: UnlistenFn | undefined;
    let unlistenStep: UnlistenFn | undefined;
    let unlistenStarted: UnlistenFn | undefined;
    let unlistenProgress: UnlistenFn | undefined;
    let unlistenFinished: UnlistenFn | undefined;
    let disposed = false;
    void onFlowRunStarted((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      onRunStateChangeRef.current('running', event.run_id);
      // After the run state, because a new run drops older URLs.
      const urls = callbackUrlsFrom(event);
      if (Object.keys(urls).length > 0) onCallbackUrlsRef.current?.(urls);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenRun = fn;
    });
    void onFlowStepStarted((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, 'running');
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStarted = fn;
    });
    void onFlowStepCompleted((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, event.status, detailFromEvent(event));
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStep = fn;
    });
    void onFlowStepProgress((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      forwardProgress(onPatchProgressRef.current, event);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenProgress = fn;
    });
    void onFlowRunFinished((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      // The mount that started the run applies the timed summary later, which
      // replaces this counts-only result.
      onRunResultRef.current?.(resultFromFinishedEvent(event));
      onRunStateChangeRef.current('done', event.run_id);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenFinished = fn;
    });
    return () => {
      disposed = true;
      unlistenRun?.();
      unlistenStarted?.();
      unlistenStep?.();
      unlistenProgress?.();
      unlistenFinished?.();
    };
  }, [resumedRunId]);
```

This effect reads `isStartingRef`, so move the `isStartingRef` declaration and its comment block (from `  // A ref, not state:` to `  const isStartingRef = useRef(false);`) up to just before `const unlistenRefs = useRef<UnlistenFn[]>([]);`. The effect is declared later in the component, so the ref exists when it runs either way; the move keeps declarations above their first use.

7. In `handleRun`, replace everything from the line `    cleanupListeners();` (right after the sign-in block) to the closing `    }` of the `finally` block with:

```tsx
    cleanupListeners();
    // Chosen here, before anything is sent, so each event can be matched to
    // this run alone. Another tab running the same flow has its own id.
    const runId = newFlowRunId();
    // Set by flow-run-started for this id. Until then, the wall-clock start.
    let started = false;
    let startedAt = performance.now();
    // Set once run_flow settles. Events that arrive after it change nothing.
    let ended = false;
    // A function, because TypeScript keeps `started` narrowed to false in the
    // catch block below, while the event handler sets it later.
    const hasStarted = (): boolean => started;
    const isOurs = (eventRunId: string) => !ended && eventRunId === runId;

    // Subscribe first. run_flow only resolves when the run ends, so every
    // event is emitted while its promise is still pending.
    const unlistenStarted = await onFlowRunStarted((event) => {
      if (started || !isOurs(event.run_id)) return;
      started = true;
      startedAt = performance.now();
      onRunStateChange('running', runId);
      // After the run state, because a new run drops older URLs.
      const urls = callbackUrlsFrom(event);
      if (Object.keys(urls).length > 0) onCallbackUrls?.(urls);
    });
    const unlistenStepStarted = await onFlowStepStarted((event) => {
      if (!isOurs(event.run_id)) return;
      onPatchStatus(event.node_id, 'running');
    });
    const unlistenStep = await onFlowStepCompleted((event) => {
      if (!isOurs(event.run_id)) return;
      onPatchStatus(event.node_id, event.status, detailFromEvent(event));
    });
    const unlistenProgress = await onFlowStepProgress((event) => {
      if (!isOurs(event.run_id)) return;
      forwardProgress(onPatchProgressRef.current, event);
    });
    unlistenRefs.current = [unlistenStarted, unlistenStepStarted, unlistenStep, unlistenProgress];
    // Known before the request goes out, so Stop works and a remounted
    // toolbar can follow the run before flow-run-started arrives.
    setActiveRunId(runId);
    onRunRequested?.(runId);

    try {
      // Read fresh at click-time, not from a prop snapshotted at an earlier
      // render — the active global environment can change (via the
      // environment switcher's Global tab) while this tab sits mounted but
      // idle, and a stale value here would silently resolve the run against
      // the wrong global environment. Mirrors how runner-execute.ts reads
      // this at execution time rather than caching it.
      const globalEnvName = getActiveGlobalEnvName();
      // Tokens are sent only when there are some, so a flow without Auth nodes
      // sends no authTokens key.
      const tokens = authTokens && Object.keys(authTokens).length > 0 ? authTokens : undefined;
      const summary = await runFlow(
        collection,
        flowName,
        environmentName,
        globalEnvName ?? null,
        tokens,
        { runId },
      );
      ended = true;
      // The summary is the authoritative final state. Event delivery is not
      // guaranteed to finish before the command response arrives.
      for (const step of summary.steps) {
        onPatchStatus(step.nodeId, step.status, detailFromStep(step));
        if (step.logs?.length) onStepLogs?.(step.nodeId, step.logs);
        if (step.debugRequest) onStepDebug?.(step.nodeId, step.debugRequest);
      }
      onRunResult?.({
        ...summarizeRun(summary, Math.round(performance.now() - startedAt)),
        environmentName: runEnvironment,
      });
      onRunStateChange('done', summary.runId);
    } catch (err) {
      ended = true;
      // A run that cannot start rejects before any event is emitted.
      toast.error(`Could not run flow: ${String(err)}`);
      // A run that had started leaves a result, so its partial results stay viewable.
      if (hasStarted()) {
        onRunResult?.({
          runId,
          stoppedReason: 'error',
          totalMs: Math.round(performance.now() - startedAt),
          failedCount: 0,
          skippedCount: 0,
          environmentName: runEnvironment,
        });
      }
      onRunStateChange('done');
    } finally {
      setActiveRunId(null);
      cleanupListeners();
      isStartingRef.current = false;
    }
```

The started handler no longer calls `setActiveRunId`, because the id is set before the run is sent. `handleStop` and the Run button's `disabled` prop stay unchanged: `liveRunId` is now non-null from the send.

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: PASS.

- [ ] **Step 8: Wire the pane and adapt the FlowPane tests**

In `src/components/flow/FlowPane.tsx`, add after `const setFlowRunState = usePaneStore((s) => s.setFlowRunState);`:

```tsx
  const setFlowPendingRun = usePaneStore((s) => s.setFlowPendingRun);
```

and in the `<FlowToolbar` element, replace

```tsx
              tabRunState={tab.runState}
              tabRunId={tab.runId}
```

with

```tsx
              onRunRequested={(runId) => setFlowPendingRun(tab.id, runId)}
              tabRunState={tab.runState}
              tabRunId={tab.runId}
              tabPendingRunId={tab.pendingRunId}
```

In `src/components/flow/__tests__/FlowPane.test.tsx`:

1. In the tests `stores flow-step-progress text on the node detail` and `stores live progress and the callback URLs on the tab`, after `await waitFor(() => expect(progress).toBeDefined());` add:

```tsx
    await waitFor(() => expect(runFlow).toHaveBeenCalled());
    // The toolbar ignores events of any run but the one it sent.
    const runId = vi.mocked(runFlow).mock.calls[0][5]?.runId ?? '';
```

and in both tests replace each `run_id: 'r1',` with `run_id: runId,` (four places in the file, all in these two tests).

2. Replace the expectation

```tsx
    expect(runFlow).toHaveBeenCalledWith('demo', 'login-flow', 'dev', 'g1', {
      auth1: { accessToken: 'tok-abcdef123' },
    });
```

with

```tsx
    expect(runFlow).toHaveBeenCalledWith(
      'demo',
      'login-flow',
      'dev',
      'g1',
      { auth1: { accessToken: 'tok-abcdef123' } },
      { runId: expect.any(String) },
    );
```

In `src/components/flow/__tests__/FlowPane.runHistoryRecord.test.tsx`:

1. Add after the `vi.mock('sonner', ...)` line:

```tsx
vi.mock('@/lib/flow-run-id', () => ({ newFlowRunId: vi.fn(() => 'unused') }));
```

and the import `import { newFlowRunId } from '@/lib/flow-run-id';` after the `@/lib/pane-utils` import.

2. In `runOnce`, add before `await userEvent.click(screen.getByRole('button', { name: 'Run' }));`:

```tsx
  // The run-started event below must carry the id the toolbar sends.
  vi.mocked(newFlowRunId).mockReturnValueOnce(runId);
```

Run: `yarn test src/components/flow src/stores src/lib`
Expected: PASS.

- [ ] **Step 9: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-run-id.ts src/lib/tauri-api.ts src/lib/queries/__tests__/flow-api.test.ts src/types/pane-types.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.flowRun.test.ts src/components/flow/FlowToolbar.tsx src/components/flow/__tests__/FlowToolbar.test.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.test.tsx src/components/flow/__tests__/FlowPane.runHistoryRecord.test.tsx`
Suggested subject: `fix(flow): match run events to the tab by a client run id`.

---

### Task 3: Two tabs of one flow, end to end, and docs

**Files:**
- Create: `src/components/flow/__tests__/FlowPane.twoTabs.test.tsx`
- Modify: `crates/rocket-app/CLAUDE.md` (new section after "Flow partial runs")
- Modify (untracked, not committed): `.claude/flow-roadmap.md` (row F-03), `docs/superpowers/plans/flow-tier3/00-plan-index.md` (P22 row), `docs/superpowers/plans/flow-tier3/2026-10-08-p20-run-from-node-ui.md` (BLOCKED note)

**Interfaces:**
- Consumes: `newFlowRunId`, `RunFlowOptions`, `FlowTab.pendingRunId`, `setFlowPendingRun`, toolbar props `onRunRequested`/`tabPendingRunId` (Task 2); backend `runId` (Task 1).
- Produces: no new code interfaces.

- [ ] **Step 1: Write the two-tab test**

Create `src/components/flow/__tests__/FlowPane.twoTabs.test.tsx`:

```tsx
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import {
  cancelFlowRun,
  type FlowRunStartedEvent,
  type FlowRunSummary,
  type FlowStepCompletedEvent,
  listCollections,
  listFlows,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    runFlow: vi.fn(),
    cancelFlowRun: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowRunFinished: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

// Delivers every event to every subscriber, like Tauri's global listen().
// Run ids are the real UUIDs from newFlowRunId.
function eventBus<T>() {
  const handlers = new Set<(event: T) => void>();
  return {
    listen: async (handler: (event: T) => void) => {
      handlers.add(handler);
      return () => {
        handlers.delete(handler);
      };
    },
    emit: (event: T) =>
      act(() => {
        for (const handler of [...handlers]) handler(event);
      }),
    size: () => handlers.size,
  };
}

let startedBus = eventBus<FlowRunStartedEvent>();
let completedBus = eventBus<FlowStepCompletedEvent>();
// Ends a pending run_flow call, by the run id it was sent with.
const resolvers = new Map<string, (summary: FlowRunSummary) => void>();

const flowTab = (id: string): FlowTab => ({
  id,
  tabType: 'flow',
  title: 'Flow: shared',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'shared',
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});

function stored(id: string): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, id);
  if (!found || !isFlowTab(found.tab)) throw new Error(`Expected flow tab ${id}`);
  return found.tab;
}

function Pane({ id }: { id: string }) {
  const tab = usePaneStore((s) => {
    const found = findTabInTree(s.root, id);
    return found && isFlowTab(found.tab) ? found.tab : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

function renderBoth() {
  render(
    <>
      <div data-testid='pane-a'>
        <Pane id='flow-a' />
      </div>
      <div data-testid='pane-b'>
        <Pane id='flow-b' />
      </div>
    </>,
  );
}

const pane = (which: 'a' | 'b') => within(screen.getByTestId(`pane-${which}`));
const sentRunId = (call: number) => vi.mocked(runFlow).mock.calls[call]?.[5]?.runId ?? '';

const startedEvent = (runId: string): FlowRunStartedEvent => ({
  type: 'flowRunStarted',
  run_id: runId,
  flow_name: 'shared',
  collection: 'demo',
  total_nodes: 1,
});

const completedEvent = (runId: string, status: 'success' | 'failed'): FlowStepCompletedEvent => ({
  type: 'flowStepCompleted',
  run_id: runId,
  node_id: 'out1',
  status,
  status_code: null,
  duration_ms: 3,
  error: status === 'failed' ? 'boom' : null,
  value: null,
});

const summary = (runId: string, status: 'success' | 'failed'): FlowRunSummary => ({
  runId,
  stoppedReason: 'completed',
  steps: [{ nodeId: 'out1', status, statusCode: null, durationMs: 3, error: null, value: null }],
});

// Clicks Run in one pane and returns the run id that pane sent.
async function runIn(which: 'a' | 'b', call: number): Promise<string> {
  await userEvent.click(pane(which).getByRole('button', { name: 'Run' }));
  await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(call + 1));
  return sentRunId(call);
}

describe('two tabs running the same flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(flowTab('flow-a'));
    usePaneStore.getState().openTab(flowTab('flow-b'));
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(cancelFlowRun).mockResolvedValue(undefined);
    startedBus = eventBus<FlowRunStartedEvent>();
    completedBus = eventBus<FlowStepCompletedEvent>();
    const quiet = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(startedBus.listen);
    vi.mocked(onFlowStepCompleted).mockImplementation(completedBus.listen);
    vi.mocked(onFlowRunFinished).mockImplementation(quiet);
    vi.mocked(onFlowStepStarted).mockImplementation(quiet);
    vi.mocked(onFlowStepProgress).mockImplementation(quiet);
    resolvers.clear();
    // Each run stays pending until the test ends it by id.
    vi.mocked(runFlow).mockImplementation(
      (_collection, _flowName, _env, _globalEnv, _tokens, options) =>
        new Promise((resolve) => {
          resolvers.set(options?.runId ?? '', resolve);
        }),
    );
  });

  it('each tab follows only its own run, whichever starts first', async () => {
    renderBoth();
    const idA = await runIn('a', 0);
    const idB = await runIn('b', 1);
    expect(idA).not.toBe('');
    expect(idA).not.toBe(idB);
    expect(stored('flow-a').pendingRunId).toBe(idA);
    expect(stored('flow-b').pendingRunId).toBe(idB);

    // B's run announces itself first. Tab A must not adopt it.
    startedBus.emit(startedEvent(idB));
    expect(stored('flow-b').runState).toBe('running');
    expect(stored('flow-b').runId).toBe(idB);
    expect(stored('flow-a').runState).toBe('idle');
    expect(stored('flow-a').pendingRunId).toBe(idA);

    startedBus.emit(startedEvent(idA));
    expect(stored('flow-a').runId).toBe(idA);

    completedBus.emit(completedEvent(idA, 'failed'));
    completedBus.emit(completedEvent(idB, 'success'));
    expect(stored('flow-a').nodeStatus).toEqual({ out1: 'failed' });
    expect(stored('flow-b').nodeStatus).toEqual({ out1: 'success' });
  });

  it("Stop in one tab cancels only that tab's run, before and after it started", async () => {
    renderBoth();
    const idA = await runIn('a', 0);
    const idB = await runIn('b', 1);

    // No event yet: the tab already knows its run.
    await userEvent.click(pane('b').getByRole('button', { name: 'Stop' }));
    expect(cancelFlowRun).toHaveBeenCalledTimes(1);
    expect(cancelFlowRun).toHaveBeenLastCalledWith(idB);

    startedBus.emit(startedEvent(idB));
    startedBus.emit(startedEvent(idA));
    await userEvent.click(pane('a').getByRole('button', { name: 'Stop' }));
    expect(cancelFlowRun).toHaveBeenCalledTimes(2);
    expect(cancelFlowRun).toHaveBeenLastCalledWith(idA);
  });

  it('events of a finished run change nothing, in the next run too', async () => {
    renderBoth();
    const first = await runIn('a', 0);
    startedBus.emit(startedEvent(first));
    await act(async () => {
      resolvers.get(first)?.(summary(first, 'success'));
    });
    await waitFor(() => expect(stored('flow-a').runState).toBe('done'));
    completedBus.emit(completedEvent(first, 'failed'));
    expect(stored('flow-a').nodeStatus).toEqual({ out1: 'success' });

    await waitFor(() => expect(pane('a').getByRole('button', { name: 'Run' })).toBeEnabled());
    const second = await runIn('a', 1);
    expect(second).not.toBe(first);
    startedBus.emit(startedEvent(second));
    completedBus.emit(completedEvent(first, 'failed'));
    expect(stored('flow-a').runId).toBe(second);
    expect(stored('flow-a').nodeStatus).toEqual({});
  });

  it('a tab whose pane remounts before its run starts still follows it', async () => {
    const view = render(<Pane id='flow-a' />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(1));
    const id = sentRunId(0);
    view.unmount();
    expect(startedBus.size()).toBe(0);
    expect(stored('flow-a').pendingRunId).toBe(id);

    render(<Pane id='flow-a' />);
    await waitFor(() => expect(startedBus.size()).toBe(1));
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
    startedBus.emit(startedEvent(id));
    expect(stored('flow-a').runState).toBe('running');
    expect(stored('flow-a').runId).toBe(id);
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(cancelFlowRun).toHaveBeenCalledWith(id);
  });

  it('runs started with Ctrl+Enter get their own ids too', async () => {
    renderBoth();
    act(() => {
      window.dispatchEvent(new CustomEvent('rocket:flow-run', { detail: { tabId: 'flow-a' } }));
    });
    act(() => {
      window.dispatchEvent(new CustomEvent('rocket:flow-run', { detail: { tabId: 'flow-b' } }));
    });
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(2));
    const a = stored('flow-a').pendingRunId;
    const b = stored('flow-b').pendingRunId;
    expect(a).toBeDefined();
    expect(b).toBeDefined();
    expect(a).not.toBe(b);
    expect([sentRunId(0), sentRunId(1)].sort()).toEqual([a, b].sort());
  });
});
```

- [ ] **Step 2: Run it**

Run: `yarn test src/components/flow/__tests__/FlowPane.twoTabs.test.tsx`
Expected: PASS (5 tests). These tests pin the behavior Task 2 built. If one fails, the fault is in the Task 2 wiring (`FlowToolbar.tsx`, `FlowPane.tsx` or the store), not in the test.

- [ ] **Step 3: Prove the cross-attach test bites**

In `src/components/flow/FlowToolbar.tsx`, temporarily change

```tsx
    const isOurs = (eventRunId: string) => !ended && eventRunId === runId;
```

to

```tsx
    const isOurs = (_eventRunId: string) => !ended;
```

Run: `yarn test src/components/flow/__tests__/FlowPane.twoTabs.test.tsx`
Expected: `each tab follows only its own run, whichever starts first` FAILS (tab A adopts B's run). Then restore the line exactly as it was, and run the test file again: PASS. Do not commit the temporary edit; `git diff src/components/flow/FlowToolbar.tsx` must be empty afterwards.

- [ ] **Step 4: Document the run id rules**

In `crates/rocket-app/CLAUDE.md`, add after the "Flow partial runs" section (before "## Flow step trace"):

```markdown
## Flow run ids (`flow_run_id.rs`)

`FlowExecutionService::run_with_options(exec, input, tokens, FlowRunOptions { run_id, partial })`
is the one run entry; `run_with_auth` and `run_partial` call it. The frontend
chooses the run id (a UUID from `newFlowRunId()` in `src/lib/flow-run-id.ts`)
and sends it as `runId` in `RunFlowInputDto`; without one the service makes a
ULID (`choose_run_id`). A chosen id must be 1 to 64 ASCII letters, digits, `-`
or `_`, or the run is refused with `InvalidInput` before any event (the message
does not quote the id). `RunRegistration::reserve` registers the id first
thing in the run and refuses with `AlreadyExists` an id that is in flight or
still kept in the run cache. A run is cached before it leaves `in_flight`, so a
used id is never free while it is kept. The id names the run in every
`FlowRun*` and `FlowStep*` event, in `cancel` (`cancel_flow_run`), in the run
cache and in `FlowRunSummary.run_id`. A `cancel` that arrives while secrets,
tokens and callback endpoints are prepared is kept and stops the run before its
first node (`FlowRunStarted` and `FlowRunFinished` still go out); the fetch
itself is not interrupted (roadmap F-05).

Frontend: `FlowToolbar` makes the id after the save and sign-in steps, stores it
on the tab as `pendingRunId` (`setFlowPendingRun`) before `run_flow` is sent,
and ignores every event with another id and every event after `run_flow`
settled. A remounted toolbar follows the tab's `pendingRunId` until
`flow-run-started`, then its `runId`. `setFlowRunState` clears `pendingRunId`.
```

- [ ] **Step 5: Run the full gates**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/stores src/lib && cargo check -j4 -p rocket-app -p rocket`
Expected: all pass.

- [ ] **Step 6: Update the roadmap, the index and the P20 note (untracked files, not committed)**

These three files are untracked in git. Edit them in place and do not stage them.

In `.claude/flow-roadmap.md`, replace the F-03 row with:

```markdown
| F-03 | Let the client choose the run ID. Runs are matched to tabs by collection and flow name, so two tabs can cross-attach and Stop can cancel the wrong run. | M | done | `FlowToolbar.tsx` `handleRun`; `flow_execution_service.rs` `run_with_options` | Done in P22: client UUID in `RunFlowOptions.runId`, backend checks and reserves it first, tab keeps `pendingRunId`. F-05 can build on the early reservation. |
```

In `docs/superpowers/plans/flow-tier3/00-plan-index.md`, add after the P21 row:

```markdown
| P22 | Client-chosen run id | F-03 | 1 backend id rule, reserve and `run_with_options`; 2 toolbar mints, stores and matches the id; 3 two-tab test and docs | none | sonnet, review task 1 in main loop | done: see `2026-10-09-p22-client-run-id.md` |
```

and in its "Blocked plans" note, change `P20 needs roadmap F-03 and P19` to `P20 needs P19 (done) and F-03 (done in P22)`.

In `docs/superpowers/plans/flow-tier3/2026-10-08-p20-run-from-node-ui.md`, replace the first blockquote paragraph (starting `> **BLOCKED. Do not start before roadmap F-03`) with:

```markdown
> **F-03 landed in P22** (`2026-10-09-p22-client-run-id.md`). Before starting, read its section "What P20 needs from this plan": `RunFlowOptions` already exists with `runId`, `runFlow` already has the sixth parameter, the toolbar test mock returns `'run-123'`, and Rust dispatch goes through `RunFlowInputDto::take_options` and `run_with_options`.
```

- [ ] **Step 7: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/__tests__/FlowPane.twoTabs.test.tsx crates/rocket-app/CLAUDE.md`
Suggested subject: `test(flow): pin run matching for two tabs of one flow`.

---

## Self-Review

- **Spec coverage:** client UUID per run (Task 2 Step 2, Step 7); sent in an optional `RunFlowInputDto` field with backend fallback (Task 1 Steps 4, 13); format check and refusal of duplicates in flight with a clear error (Task 1 Steps 4, 9); the id used in all events, the cancel registry and the cache (Task 1 Step 8); `cancel_flow_run` takes the run id (already true, doc updated, no compat path needed because there is one caller); frontend matches every `flow-run-*`, node status, progress and step trace event by id (Task 2 Step 7; trace rides on `flow-step-completed`, callback URLs on `flow-run-started`); id stored on the tab before the first event (`onRunRequested` before `runFlow`, `pendingRunId`) and sent on Stop (Task 2 Steps 6-8). Edge cases: events before the tab stored the id cannot happen for the sending mount (the id exists before the request) and are handled for a remount (`pendingRunId` resume); P1 `rocket:flow-run` runs go through `handleRun` (Task 3 Ctrl+Enter test); pre-run phase mints no id until save and sign-in succeed, backend reserves before its own pre-run (Decisions); tab closed mid-run: listeners go, no other tab can adopt the run, the run ends in the background as today; parked tabs: `setFlowPendingRun` and `setFlowRunState` use `updateTabEverywhere` (Task 2 store test); P19 partial runs take the same id (Task 1 kept-run test); `FlowRunSummary` and history records unchanged.
- **Placeholders:** none. Every code step shows the code; edits are anchored by quoted code.
- **Type consistency:** `FlowRunOptions { run_id, partial }` and `run_with_options` match between Task 1 Steps 6, 8 and 13. `RunFlowOptions { runId }`, `newFlowRunId`, `pendingRunId`, `setFlowPendingRun(tabId, runId | undefined)`, `onRunRequested(runId)` and `tabPendingRunId` match between Task 2, Task 3 and the P20 notes.
- **Review Focus coverage:** item 1 in Task 3 and Task 2; item 2 in Task 3, Task 2 and Task 1; item 3 in Task 1 (three tests); item 4 in Task 2 and Task 3; item 5 in Task 2, Task 3 and Task 1.

Known follow-ups (not in this plan): F-05 (interruptible pre-run, "preparing" event); `patchFlowNodeStatus` and `patchFlowNodeProgress` use `updateTabInTree`, so a run that ends while its tab is parked by `switchCollection` leaves that tab's node results stale (pre-existing, unchanged here); closing a tab does not cancel its run; `CLAUDE.md` points at `.claude/frontend.md` and `.claude/tauri-commands.md`, which do not exist.
