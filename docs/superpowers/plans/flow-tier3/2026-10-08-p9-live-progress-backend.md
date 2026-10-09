# Live Progress Backend Implementation Plan

> **Execute this plan:** P9. Before starting it, make sure these are merged to main: P7 (merged first; shares `execute_node` with it). After it is merged, the next plan to execute is P10. Status and the full order are in `00-plan-index.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** While a run waits, the backend says why: a repeat-until poll reports its last status and verdict, a Wait for callback node reports each call it turned down, and the run-started event lists every callback URL. The finished step keeps the same details in its trace.

**Architecture:** `FlowStepProgress` gains an optional structured `live` payload next to its text `message`, and `FlowStepTrace` (from P7) gains `poll` and `wait`. `run_repeat_until` and `wait_for_callback` receive P7's `NodeTrace` and fill it as they go, so a failed or timed-out step still shows how it went. A turned-down call is masked like an accepted one and its body is capped twice: 2 KB in the live event, 256 KB in the trace. `RunCallbacks` keeps a list of `(node id, name, url)` and `FlowRunStarted` carries it. The URL holds a bearer token, so it is sent there only.

**Tech Stack:** Rust, serde, tokio (`select!`, `Instant`), `rocket-shared` events, `rocket-app` Flow engine, `FakeCallbackListener` test double.

**Spec:** Roadmap item F-40 (backend) in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` section "P9 Live progress backend". Builds on `docs/superpowers/plans/flow-tier3/2026-10-08-p7-step-trace-backend.md`.

## Global Constraints

- Requires P7 merged: `FlowStepTrace` (derives `Default`), `NodeTrace { step, .. }`, `flow_debug::cap_text`, and the `execute_node(..., poll_stats, trace, ctx, callbacks)` signature.
- No OpenCollection spec read is needed: no `.yml`, collection, environment, auth or variable-resolution change.
- No new IPC command. The new types are camelCase DTOs nested in snake_case events. No persistence struct changes.
- No `unwrap()` or `expect()` outside `#[cfg(test)]`.
- Masking rules (mandatory): a turned-down call is masked exactly like `callback_exchange`: a sensitive header (`is_sensitive_header`) becomes `REDACTED`, every other header value goes through `redact_secrets`, the URL through `redact_url_secrets`, the body is masked first and capped second. The 1 Hz ticker carries counts only, never a call. The callback URL appears only in `FlowRunStarted`, never in `FlowRunSummary`, a step, a log or history; the token-bearing path of a received call is shown as `/cb/…`.
- Known masking limits, unchanged: secrets shorter than `MIN_REDACTION_LEN` (6) and encoded forms inside a body are not masked.
- Cargo commands use `-j4` and `-p <crate>`. Never `--workspace` or `--all`.
- Code comments are short full sentences ending with a punctuation mark.
- Commits go through the `dev-workflow-skills:1-git-commit` skill with explicit staged paths. Never `git add -A`, `--all` or `.`.
- Only one implementer at a time touches `execute_node` (P7, P9 and P19).
- Not in scope: any frontend change (P10), a callback URL in `FlowRunSummary`, cancelling the pre-run phase (roadmap F-05).

## Decisions assumed

- No open decision from the index (D1 to D6) affects this plan.
- The poll detail lives only in `trace.poll`, filled inside `run_repeat_until`. `FailedPollStats` and `PollStats` keep their fields, so the run-loop override at `flow_execution_service.rs:1070-1078` and the `PollStats` test literal at `:4441` stay unchanged. (The design notes suggested adding `condition_met` and `max_attempts` to `FailedPollStats`; the trace makes that unnecessary.)
- The live payload is boxed (`Option<Box<FlowLiveProgress>>`), like `exchange`, to keep `DomainEvent` small.
- A poll publishes one extra live event after each unmet condition, before it pauses. The pre-send "attempt n/m" event stays unchanged.
- The path of a received call is shown as `/cb/…` in the accepted-call exchange and in a turned-down call, because the path is the bearer token. This changes the existing exchange URL from `/cb/<token>` to `/cb/…`.

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A secret in a turned-down call (an `Authorization` header, a secret echoed in another header, the query or the body) reaches the live event or the trace. Tests pinned in Task 2 (`a_rejected_call_masks_sensitive_headers_secrets_and_the_url`, `a_turned_down_callback_is_reported_live_and_kept_masked_in_the_trace`).
2. An old payload without `live`, `callbacks`, `poll` or `wait` fails to parse, or the new keys appear when empty and break the exact-JSON tests. Tests pinned in Task 1 (`flow_step_progress_without_live_still_deserializes`) and Task 3 (`flow_run_started_without_callbacks_still_deserializes`), plus the unchanged exact tests `flow_run_started_wire_shape` and `flow_step_progress_wire_shape`.
3. A very large turned-down body is sent whole on every live event, or cut before it is masked. Tests pinned in Task 2 (`a_rejected_call_body_is_masked_before_it_is_cut`, `only_the_turn_down_event_carries_the_call`).
4. The callback token leaks past `FlowRunStarted`: into the summary, a step exchange or another event. Tests pinned in Task 2 (`an_accepted_callback_reports_the_call_as_its_exchange` updated to `/cb/…`) and Task 3 (`run_started_lists_the_callback_urls_and_nothing_else_does`).
5. A live progress event for a node arrives after that node's `FlowStepCompleted`, so the UI shows "waiting" on a finished node. Test pinned in Task 1 (`a_poll_reports_live_progress_after_each_unmet_condition`, ordering assertion).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/rocket-shared/src/events.rs` (modify) | `FlowLiveProgress`, `FlowPollDetail`, `FlowRejectedCall`, `FlowWaitDetail`, `FlowCallbackInfo`; `FlowStepProgress.live`; `FlowRunStarted.callbacks`; `FlowStepTrace.poll` and `.wait`. |
| `crates/rocket-app/src/flow_execution_service.rs` (modify) | `publish_live_progress`; passes `trace` to the poll and the wait; `FlowRunStarted.callbacks`; tests. |
| `crates/rocket-app/src/flow_poll.rs` (modify) | Fills `trace.poll`; live event after an unmet condition. |
| `crates/rocket-app/src/flow_wait.rs` (modify) | Fills `trace.wait`; live event for a turned-down call; live counts on the ticker. |
| `crates/rocket-app/src/flow_debug.rs` (modify) | `rejected_call`, `LIVE_REJECTED_BODY_LIMIT`, shared `call_url` (path shown as `/cb/…`) and `mask_call_headers`. |
| `crates/rocket-app/src/flow_callbacks.rs` (modify) | `RunCallbacks::infos()`. |

Existing tests to know: `crates/rocket-shared/src/events.rs` (`flow_run_started_wire_shape` :862, `flow_step_progress_wire_shape` :877, `flow_step_progress_without_attempts_sends_nulls` :893), `crates/rocket-app/src/flow_execution_service.rs` (`publish_progress_sends_the_nodes_ids_and_message` :3930, `poll_publishes_progress_for_each_attempt` :6929, `SequenceExecutor` :6638, `poll_exec` :6678, `repeat` :6697, `poll_flow` :6712, `status_condition` :6733, `wait_node_with` :2865, `event_call` :2878, `an_accepted_callback_reports_the_call_as_its_exchange` :6434, `a_call_during_the_wait_is_accepted_and_progress_is_reported` :7376, `timeout_fails_the_node_and_reports_ignored_calls` :7447), `crates/rocket-app/src/flow_debug.rs` (`secrets` helper, callback tests :540-586), `crates/rocket-app/src/flow_callbacks.rs` (`wait_node`, `each_wait_node_owns_the_endpoint_its_variable_names`). Line numbers are from HEAD b047bbc6 before P7; P7 shifts them, so find each by name.

---

### Task 1: Poll live progress and `trace.poll`

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (new types after P7's `FlowRouteEval`; `FlowStepProgress` variant; `FlowStepTrace`; tests)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`publish_progress`; Request arm call to `run_repeat_until`; tests)
- Modify: `crates/rocket-app/src/flow_poll.rs`

**Interfaces:**
- Consumes: `NodeTrace` (P7).
- Produces (Rust): `FlowLiveProgress { last_status_code: Option<u16>, condition_met: Option<bool>, elapsed_ms: Option<u64>, remaining_ms: Option<u64>, ignored: Option<u32> }` (Task 2 adds `last_rejected`), `FlowPollDetail { attempts: u32, max_attempts: u32, last_status_code: Option<u16>, condition_met: Option<bool>, elapsed_ms: u64, timeout_ms: u64 }`, `DomainEvent::FlowStepProgress.live: Option<Box<FlowLiveProgress>>`, `FlowStepTrace.poll: Option<FlowPollDetail>`, `FlowExecutionService::publish_live_progress(&self, ctx, attempt, max_attempts, message, live: FlowLiveProgress)`, `run_repeat_until(..., poll_stats, trace: &mut NodeTrace, ctx)`.
- Produces (JSON): `live: { lastStatusCode?, conditionMet?, elapsedMs?, remainingMs?, ignored? }`; `trace.poll: { attempts, maxAttempts, lastStatusCode?, conditionMet?, elapsedMs, timeoutMs }`.

- [ ] **Step 1: Write the failing wire-shape tests**

In `crates/rocket-shared/src/events.rs`, inside `mod tests`, append:

```rust
    #[test]
    fn flow_step_progress_carries_live_detail_in_camel_case() {
        let event = DomainEvent::FlowStepProgress {
            run_id: "01J".into(),
            node_id: "n".into(),
            attempt: Some(2),
            max_attempts: Some(5),
            message: "attempt 2/5 · condition false".into(),
            live: Some(Box::new(FlowLiveProgress {
                last_status_code: Some(202),
                condition_met: Some(false),
                remaining_ms: Some(12_000),
                ..Default::default()
            })),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains(r#""live":{"lastStatusCode":202,"conditionMet":false,"remainingMs":12000}"#),
            "{json}"
        );
    }

    #[test]
    fn flow_step_progress_without_live_still_deserializes() {
        let json = r#"{"type":"flowStepProgress","run_id":"01J","node_id":"n","attempt":3,"max_attempts":30,"message":"attempt 3/30"}"#;
        match serde_json::from_str::<DomainEvent>(json).expect("old payload") {
            DomainEvent::FlowStepProgress { live, .. } => assert!(live.is_none()),
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn a_flow_poll_detail_rides_in_the_step_trace() {
        let trace = FlowStepTrace {
            poll: Some(FlowPollDetail {
                attempts: 3,
                max_attempts: 30,
                last_status_code: Some(200),
                condition_met: Some(true),
                elapsed_ms: 2_500,
                timeout_ms: 60_000,
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&trace).expect("serialize");
        assert_eq!(
            json,
            r#"{"poll":{"attempts":3,"maxAttempts":30,"lastStatusCode":200,"conditionMet":true,"elapsedMs":2500,"timeoutMs":60000}}"#
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-shared flow_step_progress`
Expected: FAIL to compile (`FlowLiveProgress`, `FlowPollDetail`, `live`, `poll` do not exist).

- [ ] **Step 3: Add the types and fields**

In `crates/rocket-shared/src/events.rs`, after P7's `FlowRouteEval` struct, add:

```rust
/// Structured progress of a node that is still running. Every field is
/// optional, so each node kind sends only what applies to it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowLiveProgress {
    /// Status code of the last poll response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status_code: Option<u16>,
    /// Verdict of the last poll condition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition_met: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    /// Time left before the node gives up. The UI counts down from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_ms: Option<u64>,
    /// Calls a Wait for callback node turned down so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignored: Option<u32>,
}

/// How a repeat-until poll went. Kept in the step trace, also on failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowPollDetail {
    pub attempts: u32,
    pub max_attempts: u32,
    /// `None` until an attempt got a response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status_code: Option<u16>,
    /// `None` when no verdict was reached, as after a condition script error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition_met: Option<bool>,
    pub elapsed_ms: u64,
    pub timeout_ms: u64,
}
```

In `FlowStepTrace`, add after the `route` field:

```rust
    /// How a repeat-until poll went.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll: Option<FlowPollDetail>,
```

In `DomainEvent::FlowStepProgress`, add after `message: String,`:

```rust
        /// Structured progress for the Last run tab. The nested fields are
        /// camelCase inside this snake_case event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        live: Option<Box<FlowLiveProgress>>,
```

In the existing tests `flow_step_progress_wire_shape` and `flow_step_progress_without_attempts_sends_nulls`, add `live: None,` after the `message: ...,` line of each literal.

- [ ] **Step 4: Run the shared tests**

Run: `cargo test -j4 -p rocket-shared`
Expected: PASS, including `flow_step_progress_wire_shape` (a `None` live is omitted).

- [ ] **Step 5: Write the failing engine tests**

In `crates/rocket-app/src/flow_execution_service.rs`:

1. In `publish_progress_sends_the_nodes_ids_and_message`, change the pattern

```rust
            DomainEvent::FlowStepProgress {
                run_id,
                node_id,
                attempt,
                max_attempts,
                message,
            } => {
```

to:

```rust
            DomainEvent::FlowStepProgress {
                run_id,
                node_id,
                attempt,
                max_attempts,
                message,
                live,
            } => {
                assert!(live.is_none(), "plain progress has no live detail");
```

2. In `poll_publishes_progress_for_each_attempt`, change the first filter so it keeps only the pre-send events:

```rust
            .filter_map(|e| match e {
                DomainEvent::FlowStepProgress {
                    node_id,
                    attempt,
                    message,
                    live: None,
                    ..
                } if node_id == "job" => Some((attempt, message)),
                _ => None,
            })
```

and change the expected order to:

```rust
        // Each unmet condition adds a live event before the pause.
        assert_eq!(
            order,
            vec![
                "started", "progress", "progress", "progress", "progress", "progress",
                "completed"
            ]
        );
```

3. After `poll_publishes_progress_for_each_attempt`, add:

```rust
    #[tokio::test]
    async fn a_poll_reports_live_progress_after_each_unmet_condition() {
        use rocket_shared::events::FlowLiveProgress;

        let executor = SequenceExecutor::new(vec![(404, "{}"), (404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            poll_flow(repeat("response.status === 200", 100, 5, 10_000), false),
            &publisher,
        );

        service.run(&exec, run_input("poll")).await.expect("run");

        let events = publisher.events();
        let live: Vec<FlowLiveProgress> = events
            .iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepProgress {
                    node_id,
                    live: Some(live),
                    ..
                } if node_id == "job" => Some((**live).clone()),
                _ => None,
            })
            .collect();
        assert_eq!(live.len(), 2, "one per unmet condition, none after the met one");
        for l in &live {
            assert_eq!(l.last_status_code, Some(404));
            assert_eq!(l.condition_met, Some(false));
            assert!(l.elapsed_ms.is_some());
            assert!(
                l.remaining_ms.is_some_and(|ms| ms > 0 && ms <= 10_000),
                "{l:?}"
            );
        }
        let completed_at = events
            .iter()
            .position(|e| matches!(e, DomainEvent::FlowStepCompleted { node_id, .. } if node_id == "job"))
            .expect("completed");
        let last_progress = events
            .iter()
            .rposition(|e| matches!(e, DomainEvent::FlowStepProgress { node_id, .. } if node_id == "job"))
            .expect("progress");
        assert!(
            last_progress < completed_at,
            "no progress for a node arrives after its step completed"
        );
    }

    #[tokio::test]
    async fn a_met_poll_keeps_its_poll_detail_in_the_trace() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let poll = step_of(&summary, "job")
            .trace
            .clone()
            .and_then(|t| t.poll)
            .expect("poll detail");
        assert_eq!(poll.attempts, 3);
        assert_eq!(poll.max_attempts, 5);
        assert_eq!(poll.last_status_code, Some(200));
        assert_eq!(poll.condition_met, Some(true));
        assert_eq!(poll.timeout_ms, 10_000);
    }

    #[tokio::test]
    async fn a_given_up_poll_keeps_condition_false_in_the_trace() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 10, 3, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        let poll = step.trace.clone().and_then(|t| t.poll).expect("poll detail");
        assert_eq!(poll.attempts, 3);
        assert_eq!(poll.last_status_code, Some(404));
        assert_eq!(poll.condition_met, Some(false));
    }

    #[tokio::test]
    async fn a_poll_condition_error_keeps_no_verdict() {
        let executor = SequenceExecutor::new(vec![(200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Throw("ReferenceError: nope"))]),
            &history,
        );
        let flow = poll_flow(repeat("nope.ok", 10, 3, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let poll = step_of(&summary, "job")
            .trace
            .clone()
            .and_then(|t| t.poll)
            .expect("poll detail");
        assert_eq!(poll.attempts, 1);
        assert_eq!(poll.last_status_code, Some(200));
        assert_eq!(poll.condition_met, None);
    }

    #[tokio::test]
    async fn a_poll_without_a_response_keeps_its_attempts() {
        let executor = SequenceExecutor::new(vec![(0, "")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 10, 3, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let poll = step_of(&summary, "job")
            .trace
            .clone()
            .and_then(|t| t.poll)
            .expect("poll detail");
        assert_eq!(poll.attempts, 1);
        assert_eq!(poll.last_status_code, None);
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app poll`
Expected: FAIL to compile until `publish_live_progress` and the new `run_repeat_until` parameter exist, then FAIL on the missing live events and `trace.poll`.

- [ ] **Step 7: Add `publish_live_progress`**

In `crates/rocket-app/src/flow_execution_service.rs`, replace `publish_progress`:

```rust
    pub(crate) fn publish_progress(
        &self,
        ctx: &NodeRunContext,
        attempt: Option<u32>,
        max_attempts: Option<u32>,
        message: String,
    ) {
        self.events.publish(DomainEvent::FlowStepProgress {
            run_id: ctx.run_id.clone(),
            node_id: ctx.node_id.clone(),
            attempt,
            max_attempts,
            message,
        });
    }
```

with:

```rust
    pub(crate) fn publish_progress(
        &self,
        ctx: &NodeRunContext,
        attempt: Option<u32>,
        max_attempts: Option<u32>,
        message: String,
    ) {
        self.publish_progress_event(ctx, attempt, max_attempts, message, None);
    }

    /// Reports progress with structured detail for the Last run tab. Every
    /// value in `live` must already be masked.
    pub(crate) fn publish_live_progress(
        &self,
        ctx: &NodeRunContext,
        attempt: Option<u32>,
        max_attempts: Option<u32>,
        message: String,
        live: FlowLiveProgress,
    ) {
        self.publish_progress_event(ctx, attempt, max_attempts, message, Some(Box::new(live)));
    }

    fn publish_progress_event(
        &self,
        ctx: &NodeRunContext,
        attempt: Option<u32>,
        max_attempts: Option<u32>,
        message: String,
        live: Option<Box<FlowLiveProgress>>,
    ) {
        self.events.publish(DomainEvent::FlowStepProgress {
            run_id: ctx.run_id.clone(),
            node_id: ctx.node_id.clone(),
            attempt,
            max_attempts,
            message,
            live,
        });
    }
```

Add `FlowLiveProgress` to the `use rocket_shared::events::{...}` list near line 669.

In the Request arm of `execute_node`, change the `run_repeat_until` call's trailing arguments from

```rust
                            poll_stats,
                            ctx,
                        )
                        .await;
```

to:

```rust
                            poll_stats,
                            trace,
                            ctx,
                        )
                        .await;
```

- [ ] **Step 8: Fill the poll trace and publish live progress**

In `crates/rocket-app/src/flow_poll.rs`:

1. Change the imports to:

```rust
use rocket_shared::events::{FlowDebugRequest, FlowLiveProgress, FlowLogEntry, FlowPollDetail};
```

and add:

```rust
use crate::flow_trace::NodeTrace;
```

2. After `fn millis`, add:

```rust
/// Updates the poll record's elapsed time, when the trace has one.
fn note_elapsed(trace: &mut NodeTrace, elapsed_ms: u64) {
    if let Some(poll) = trace.step.poll.as_mut() {
        poll.elapsed_ms = elapsed_ms;
    }
}
```

3. Change the doc comment's last line and the parameters of `run_repeat_until` so that after `poll_stats: &mut Option<FailedPollStats>,` it reads:

```rust
        poll_stats: &mut Option<FailedPollStats>,
        trace: &mut NodeTrace,
        ctx: &mut NodeRunContext,
```

and add this line to its doc comment after "Every failure after a response sets `poll_stats`.":

```rust
    /// `trace.poll` holds the same, plus the last verdict, on every path.
```

4. After `let mut pending_history = None;`, add:

```rust
        trace.step.poll = Some(FlowPollDetail {
            attempts: 0,
            max_attempts: repeat.max_attempts,
            last_status_code: None,
            condition_met: None,
            elapsed_ms: 0,
            timeout_ms: repeat.timeout_ms,
        });
```

5. In the cancel check at the top of the loop, after

```rust
                if let Some(stats) = poll_stats.as_mut() {
                    stats.elapsed_ms = millis(started.elapsed());
                }
                return Err(DomainError::Internal("cancelled".to_string()));
```

insert `note_elapsed(trace, millis(started.elapsed()));` before the `return`.

6. Replace

```rust
            attempt += 1;
            self.publish_progress(
```

with:

```rust
            attempt += 1;
            if let Some(poll) = trace.step.poll.as_mut() {
                poll.attempts = attempt;
            }
            self.publish_progress(
```

7. In the send-error branch, replace

```rust
                Err(e) => {
                    if let Some(stats) = poll_stats.as_mut() {
                        stats.attempts = attempt;
                        stats.elapsed_ms = millis(started.elapsed());
                    }
                    return Err(e);
                }
```

with:

```rust
                Err(e) => {
                    if let Some(stats) = poll_stats.as_mut() {
                        stats.attempts = attempt;
                        stats.elapsed_ms = millis(started.elapsed());
                    }
                    note_elapsed(trace, millis(started.elapsed()));
                    return Err(e);
                }
```

8. After the `*poll_stats = Some(FailedPollStats { ... });` statement, add:

```rust
            if let Some(poll) = trace.step.poll.as_mut() {
                poll.last_status_code = Some(status_code);
                poll.condition_met = verdict.as_ref().ok().copied();
                poll.elapsed_ms = millis(elapsed);
            }
```

9. Replace the last match arm `Ok(false) => {}` with:

```rust
                Ok(false) => {
                    // Says why the poll goes on, before it pauses.
                    self.publish_live_progress(
                        ctx,
                        Some(attempt),
                        Some(repeat.max_attempts),
                        format!("attempt {attempt}/{} · condition false", repeat.max_attempts),
                        FlowLiveProgress {
                            last_status_code: Some(status_code),
                            condition_met: Some(false),
                            elapsed_ms: Some(millis(elapsed)),
                            remaining_ms: Some(millis(deadline.saturating_duration_since(now))),
                            ..Default::default()
                        },
                    );
                }
```

10. In the cancelled-pause branch, after

```rust
                if let Some(stats) = poll_stats.as_mut() {
                    stats.elapsed_ms = millis(started.elapsed());
                }
```

add `note_elapsed(trace, millis(started.elapsed()));`.

- [ ] **Step 9: Run the tests**

Run: `cargo test -j4 -p rocket-app poll`
Expected: PASS, including every existing poll test.

- [ ] **Step 10: Gates and commit**

Run: `cargo check -j4 -p rocket-shared -p rocket-app && cargo clippy -j4 -p rocket-app && cargo check -j4 -p rocket`
Expected: no errors, and no new clippy warnings in the files this task touched.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-shared/src/events.rs crates/rocket-app/src/flow_execution_service.rs crates/rocket-app/src/flow_poll.rs`
Suggested subject: `feat(flow): report why a repeat-until poll keeps going`.

---

### Task 2: Wait detail and the turned-down call

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (new types, `FlowLiveProgress.last_rejected`, `FlowStepTrace.wait`, tests)
- Modify: `crates/rocket-app/src/flow_debug.rs` (`call_url`, `mask_call_headers`, `rejected_call`, `LIVE_REJECTED_BODY_LIMIT`, `callback_exchange` :150-190, tests :540-586)
- Modify: `crates/rocket-app/src/flow_wait.rs`
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (WaitForCallback arm of `execute_node`; test `an_accepted_callback_reports_the_call_as_its_exchange`; new test)

**Interfaces:**
- Consumes: `publish_live_progress` (Task 1), `NodeTrace`, `cap_text` (P7).
- Produces (Rust): `FlowRejectedCall { method, url, headers: Vec<FlowDebugHeader>, body: String, body_truncated: bool, reason: String }`, `FlowWaitDetail { ignored: u32, timeout_ms: u64, last_rejected: Option<FlowRejectedCall> }`, `FlowLiveProgress.last_rejected: Option<FlowRejectedCall>`, `FlowStepTrace.wait: Option<FlowWaitDetail>`, `flow_debug::rejected_call(call, secret_values, body_limit, reason) -> FlowRejectedCall`, `flow_debug::LIVE_REJECTED_BODY_LIMIT = 2_048`, `wait_for_callback(..., exchange, trace: &mut NodeTrace, ctx, callbacks)`.
- Produces (JSON): `live.lastRejected` and `trace.wait: { ignored, timeoutMs, lastRejected?: { method, url, headers, body, bodyTruncated?, reason } }`.

- [ ] **Step 1: Write the failing wire-shape test**

In `crates/rocket-shared/src/events.rs`, inside `mod tests`, append:

```rust
    #[test]
    fn a_flow_wait_detail_carries_the_last_rejected_call() {
        let trace = FlowStepTrace {
            wait: Some(FlowWaitDetail {
                ignored: 2,
                timeout_ms: 60_000,
                last_rejected: Some(FlowRejectedCall {
                    method: "POST".into(),
                    url: "/cb/…?x=1".into(),
                    headers: Vec::new(),
                    body: "{}".into(),
                    body_truncated: false,
                    reason: "Accept when returned false.".into(),
                }),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&trace).expect("serialize");
        assert_eq!(
            json,
            r#"{"wait":{"ignored":2,"timeoutMs":60000,"lastRejected":{"method":"POST","url":"/cb/…?x=1","headers":[],"body":"{}","reason":"Accept when returned false."}}}"#
        );
    }
```

Run: `cargo test -j4 -p rocket-shared flow_wait_detail`
Expected: FAIL to compile.

- [ ] **Step 2: Add the types and fields**

In `crates/rocket-shared/src/events.rs`, before `FlowLiveProgress`, add:

```rust
/// A call a Wait for callback node turned down, already masked. The body is
/// masked first and cut second.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRejectedCall {
    pub method: String,
    /// The path and query. The token-bearing path is shown as `/cb/…`.
    pub url: String,
    pub headers: Vec<FlowDebugHeader>,
    pub body: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub body_truncated: bool,
    /// Why the call was turned down.
    pub reason: String,
}

/// How a callback wait went. Kept in the step trace, also on a timeout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowWaitDetail {
    pub ignored: u32,
    pub timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rejected: Option<FlowRejectedCall>,
}
```

In `FlowLiveProgress`, add after `ignored`:

```rust
    /// The call just turned down, with its body cut at 2 KB. Only the
    /// turn-down event carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rejected: Option<FlowRejectedCall>,
```

In `FlowStepTrace`, add after `poll`:

```rust
    /// How a callback wait went.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<FlowWaitDetail>,
```

Run: `cargo test -j4 -p rocket-shared`
Expected: PASS.

- [ ] **Step 3: Write the failing `flow_debug` tests**

In `crates/rocket-app/src/flow_debug.rs`, in `callback_exchange_shows_the_call_as_the_response_with_secrets_masked` change line 554 to:

```rust
        assert_eq!(record.url, "/cb/…?event=paid", "the token path is hidden");
```

in `callback_exchange_masks_a_secret_in_the_query_plain_or_encoded` change line 585 to:

```rust
        assert_eq!(record.url, "/cb/…?plain=••••••&enc=••••••");
```

and append inside `mod tests`:

```rust
    fn turned_down(body: &str) -> crate::callback_listener::ReceivedCall {
        crate::callback_listener::ReceivedCall {
            method: "POST".into(),
            path: "/cb/abc".into(),
            query: vec![("token".into(), "sekret-token".into())],
            headers: vec![
                ("Authorization".into(), "Bearer x".into()),
                ("X-Echo".into(), "sekret-token".into()),
                ("Content-Type".into(), "application/json".into()),
            ],
            body: body.into(),
        }
    }

    #[test]
    fn a_rejected_call_masks_sensitive_headers_secrets_and_the_url() {
        let call = turned_down(r#"{"t":"sekret-token"}"#);
        let record = rejected_call(&call, &secrets(&["sekret-token"]), EXCHANGE_BODY_LIMIT, "no match");
        assert_eq!(record.method, "POST");
        assert_eq!(record.url, format!("/cb/…?token={REDACTED}"));
        assert_eq!(record.headers[0].value, REDACTED);
        assert_eq!(record.headers[1].value, REDACTED);
        assert_eq!(record.headers[2].value, "application/json");
        assert_eq!(record.body, format!(r#"{{"t":"{REDACTED}"}}"#));
        assert!(!record.body_truncated);
        assert_eq!(record.reason, "no match");
    }

    #[test]
    fn a_rejected_call_body_is_masked_before_it_is_cut() {
        let body = format!("{}sekret-token", "a".repeat(LIVE_REJECTED_BODY_LIMIT - 4));
        let record = rejected_call(
            &turned_down(&body),
            &secrets(&["sekret-token"]),
            LIVE_REJECTED_BODY_LIMIT,
            "no match",
        );
        assert!(record.body_truncated);
        assert!(record.body.len() <= LIVE_REJECTED_BODY_LIMIT);
        assert!(!record.body.contains("sek"), "half a secret leaked");
    }

    #[test]
    fn a_path_outside_the_callback_prefix_is_kept() {
        let mut call = turned_down("");
        call.path = "/other".into();
        call.query.clear();
        let record = rejected_call(&call, &secrets(&[]), EXCHANGE_BODY_LIMIT, "no match");
        assert_eq!(record.url, "/other");
    }
```

Run: `cargo test -j4 -p rocket-app flow_debug`
Expected: FAIL (`rejected_call` is missing; the URL still shows `/cb/abc`).

- [ ] **Step 4: Implement the shared call helpers**

In `crates/rocket-app/src/flow_debug.rs`, change the events import to:

```rust
use rocket_shared::events::{FlowDebugHeader, FlowDebugRequest, FlowDebugResponse, FlowRejectedCall};
```

add `use crate::callback_listener::ReceivedCall;`, and replace `callback_exchange` (lines 147-190) with:

```rust
/// The path and query of a received call. The path of a callback endpoint is
/// its bearer token, so it is shown as `/cb/…`.
fn call_url(call: &ReceivedCall) -> String {
    let path = if call.path.starts_with("/cb/") {
        "/cb/…".to_string()
    } else {
        call.path.clone()
    };
    let query: Vec<String> = call.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
    if query.is_empty() {
        path
    } else {
        format!("{path}?{}", query.join("&"))
    }
}

/// A received call's headers. A sensitive header is always masked, and every
/// other value has its secrets masked.
fn mask_call_headers(
    headers: &[(String, String)],
    secret_values: &HashSet<String>,
) -> Vec<FlowDebugHeader> {
    headers
        .iter()
        .map(|(key, value)| FlowDebugHeader {
            key: key.clone(),
            value: if is_sensitive_header(key) {
                REDACTED.to_string()
            } else {
                redact_secrets(value, secret_values)
            },
        })
        .collect()
}

/// The record of an accepted callback. The call itself is the response,
/// so a reader sees what arrived; the request side holds its method and
/// path.
pub(crate) fn callback_exchange(
    call: &ReceivedCall,
    duration_ms: u64,
    secret_values: &HashSet<String>,
) -> FlowDebugRequest {
    cap_exchange(FlowDebugRequest {
        method: call.method.clone(),
        url: redact_url_secrets(&call_url(call), secret_values),
        headers: Vec::new(),
        body: None,
        body_truncated: false,
        response: Some(FlowDebugResponse {
            status: 200,
            status_text: call.method.clone(),
            duration_ms,
            size_bytes: call.body.len() as u64,
            headers: mask_call_headers(&call.headers, secret_values),
            body: redact_secrets(&call.body, secret_values),
            truncated: false,
        }),
        error: None,
    })
}

/// The largest turned-down call body a live progress event carries, in bytes.
pub(crate) const LIVE_REJECTED_BODY_LIMIT: usize = 2_048;

/// The masked record of a call a Wait for callback node turned down. The body
/// is masked first and cut to `body_limit` second.
pub(crate) fn rejected_call(
    call: &ReceivedCall,
    secret_values: &HashSet<String>,
    body_limit: usize,
    reason: &str,
) -> FlowRejectedCall {
    let mut body = redact_secrets(&call.body, secret_values);
    let body_truncated = cap_text(&mut body, body_limit);
    FlowRejectedCall {
        method: call.method.clone(),
        url: redact_url_secrets(&call_url(call), secret_values),
        headers: mask_call_headers(&call.headers, secret_values),
        body,
        body_truncated,
        reason: reason.to_string(),
    }
}
```

Run: `cargo test -j4 -p rocket-app flow_debug`
Expected: PASS.

- [ ] **Step 5: Write the failing engine tests**

In `crates/rocket-app/src/flow_execution_service.rs`, in `an_accepted_callback_reports_the_call_as_its_exchange` change `assert_eq!(exchange.url, "/cb/0");` to:

```rust
        assert_eq!(exchange.url, "/cb/…", "the token path is hidden");
```

After `timeout_fails_the_node_and_reports_ignored_calls`, add:

```rust
    #[tokio::test]
    async fn a_turned_down_callback_is_reported_live_and_kept_masked_in_the_trace() {
        let mut env = env_with(&[]);
        let mut key = rocket_environment::Variable::new("apiKey", "sk-live-123456");
        key.secret = true;
        env.set_variable(key);
        // `scoped_exec` answers every `!!(` script with "the body is acme", so
        // the call is turned down and the wait times out after 1 s.
        let exec = scoped_exec(env, Vec::new());
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(crate::callback_listener::ReceivedCall {
            method: "POST".to_string(),
            path: "/cb/0".to_string(),
            query: vec![("key".to_string(), "sk-live-123456".to_string())],
            headers: vec![
                ("Authorization".to_string(), "Bearer abcdef-123".to_string()),
                ("X-Echo".to_string(), "sk-live-123456".to_string()),
            ],
            body: r#"{"secret":"sk-live-123456"}"#.to_string(),
        });
        let flow = Flow {
            name: "cb-reject".to_string(),
            nodes: vec![wait_node_with("w", 1000, Some("request.body.ok"))],
            edges: Vec::new(),
            callback_host: None,
        };
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(flow, &publisher)
            .with_callback_listener(Box::new(Arc::clone(&fake)));
        let mut input = run_input("cb-reject");
        input.environment_name = Some("dev".to_string());

        let summary = service.run(&exec, input).await.expect("run");

        let step = step_of(&summary, "w");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        let wait = step.trace.clone().and_then(|t| t.wait).expect("wait detail");
        assert_eq!(wait.ignored, 1);
        assert_eq!(wait.timeout_ms, 1000);
        let rejected = wait.last_rejected.expect("the turned-down call");
        assert_eq!(
            rejected.url,
            format!("/cb/…?key={}", crate::redaction::REDACTED)
        );
        assert!(
            rejected
                .headers
                .iter()
                .all(|h| h.value == crate::redaction::REDACTED),
            "{:?}",
            rejected.headers
        );
        assert!(rejected.body.contains(crate::redaction::REDACTED));

        let events = publisher.events();
        let live = events
            .iter()
            .find_map(|e| match e {
                DomainEvent::FlowStepProgress {
                    node_id,
                    live: Some(live),
                    ..
                } if node_id == "w" && live.last_rejected.is_some() => Some((**live).clone()),
                _ => None,
            })
            .expect("a live event with the turned-down call");
        assert_eq!(live.ignored, Some(1));
        for event in &events {
            let json = serde_json::to_string(event).expect("serialize");
            assert!(!json.contains("sk-live-123456"), "{json}");
        }
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(!json.contains("sk-live-123456"), "{json}");
    }

    #[tokio::test]
    async fn only_the_turn_down_event_carries_the_call() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.pending"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(
            &executor,
            scripted(vec![("const request", Scripted::Value(serde_json::json!(false)))]),
        );
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            register_then_wait(wait_node_with("w", 1000, Some("request.body.ok"))),
            &publisher,
        )
        .with_callback_listener(Box::new(Arc::clone(&fake)));

        service.run(&exec, run_input("cb")).await.expect("run");

        let live: Vec<rocket_shared::events::FlowLiveProgress> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepProgress {
                    node_id,
                    live: Some(live),
                    ..
                } if node_id == "w" => Some(*live),
                _ => None,
            })
            .collect();
        assert!(live.len() >= 2, "ticks and the turn-down event: {live:?}");
        assert_eq!(
            live.iter().filter(|l| l.last_rejected.is_some()).count(),
            1,
            "the 1 Hz ticker carries counts only"
        );
        assert!(live.iter().all(|l| l.ignored.is_some() && l.remaining_ms.is_some()));
    }
```

Run: `cargo test -j4 -p rocket-app callback`
Expected: FAIL (`trace.wait` is `None`, no live events, the exchange URL still shows `/cb/0`).

- [ ] **Step 6: Fill the wait trace and publish live progress**

In `crates/rocket-app/src/flow_wait.rs`:

1. Change the imports to:

```rust
use rocket_shared::events::{FlowDebugRequest, FlowLiveProgress, FlowLogEntry, FlowWaitDetail};
```

and add:

```rust
use crate::flow_debug::{rejected_call, EXCHANGE_BODY_LIMIT, LIVE_REJECTED_BODY_LIMIT};
use crate::flow_trace::NodeTrace;
```

2. After `fn seconds_left`, add:

```rust
/// Whole milliseconds, for the live payload.
fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Why a call that `accept_when` answered with false was turned down.
const REJECT_REASON: &str = "Accept when returned false.";
```

3. Change the parameters of `wait_for_callback` so that after `exchange: &mut Option<FlowDebugRequest>,` it reads:

```rust
        exchange: &mut Option<FlowDebugRequest>,
        trace: &mut NodeTrace,
        ctx: &mut NodeRunContext,
```

4. Replace

```rust
        let mut ignored: u32 = 0;

        loop {
```

with:

```rust
        let mut ignored: u32 = 0;
        // The trace keeps the count and the last turned-down call, also on a timeout.
        trace.step.wait = Some(FlowWaitDetail {
            ignored: 0,
            timeout_ms,
            last_rejected: None,
        });

        loop {
```

5. Replace

```rust
                            "false" => {
                                ignored += 1;
                                continue;
                            }
```

with:

```rust
                            "false" => {
                                ignored += 1;
                                if let Some(wait) = trace.step.wait.as_mut() {
                                    wait.ignored = ignored;
                                    wait.last_rejected = Some(rejected_call(
                                        &call,
                                        secret_values,
                                        EXCHANGE_BODY_LIMIT,
                                        REJECT_REASON,
                                    ));
                                }
                                // Shown at once, with a smaller body than the trace keeps.
                                let remaining = deadline.saturating_duration_since(Instant::now());
                                self.publish_live_progress(
                                    ctx,
                                    None,
                                    None,
                                    format!(
                                        "waiting… {}s left · {ignored} ignored call(s)",
                                        seconds_left(remaining)
                                    ),
                                    FlowLiveProgress {
                                        remaining_ms: Some(millis(remaining)),
                                        ignored: Some(ignored),
                                        last_rejected: Some(rejected_call(
                                            &call,
                                            secret_values,
                                            LIVE_REJECTED_BODY_LIMIT,
                                            REJECT_REASON,
                                        )),
                                        ..Default::default()
                                    },
                                );
                                continue;
                            }
```

6. Replace the ticker arm

```rust
                _ = ticker.tick() => {
                    let left = seconds_left(deadline.saturating_duration_since(Instant::now()));
                    self.publish_progress(
                        ctx,
                        None,
                        None,
                        format!("waiting… {left}s left · {ignored} ignored call(s)"),
                    );
                }
```

with:

```rust
                _ = ticker.tick() => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    // The ticker carries counts only, never a call.
                    self.publish_live_progress(
                        ctx,
                        None,
                        None,
                        format!(
                            "waiting… {}s left · {ignored} ignored call(s)",
                            seconds_left(remaining)
                        ),
                        FlowLiveProgress {
                            remaining_ms: Some(millis(remaining)),
                            ignored: Some(ignored),
                            ..Default::default()
                        },
                    );
                }
```

In `crates/rocket-app/src/flow_execution_service.rs`, in the WaitForCallback arm of `execute_node`, change the trailing arguments of `wait_for_callback` from

```rust
                    logs,
                    exchange,
                    ctx,
                    callbacks,
```

to:

```rust
                    logs,
                    exchange,
                    trace,
                    ctx,
                    callbacks,
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -j4 -p rocket-app flow_`
Expected: PASS, including `a_call_during_the_wait_is_accepted_and_progress_is_reported` (the message text is unchanged) and `timeout_fails_the_node_and_reports_ignored_calls`.

- [ ] **Step 8: Gates and commit**

Run: `cargo check -j4 -p rocket-shared -p rocket-app && cargo clippy -j4 -p rocket-app && cargo check -j4 -p rocket`
Expected: no errors, and no new clippy warnings in the files this task touched.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-shared/src/events.rs crates/rocket-app/src/flow_debug.rs crates/rocket-app/src/flow_wait.rs crates/rocket-app/src/flow_execution_service.rs`
Suggested subject: `feat(flow): report turned-down callbacks while a wait runs`.

---

### Task 3: Callback URLs on run start

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (`FlowCallbackInfo`, `FlowRunStarted.callbacks`, tests)
- Modify: `crates/rocket-app/src/flow_callbacks.rs`
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`FlowRunStarted` publish in `run_with_auth` :978-983, tests)

**Interfaces:**
- Produces (Rust): `FlowCallbackInfo { node_id: String, name: String, url: String }`, `DomainEvent::FlowRunStarted.callbacks: Vec<FlowCallbackInfo>`, `RunCallbacks::infos(&self) -> &[FlowCallbackInfo]`.
- Produces (JSON): `callbacks?: [{ nodeId, name, url }]` on `flow-run-started`, omitted when empty.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-shared/src/events.rs`, in `flow_run_started_wire_shape` add `callbacks: Vec::new(),` after `total_nodes: 3,`, and append inside `mod tests`:

```rust
    #[test]
    fn flow_run_started_carries_callback_urls_in_camel_case() {
        let event = DomainEvent::FlowRunStarted {
            run_id: "01J".into(),
            flow_name: "Pay".into(),
            collection: "acme".into(),
            total_nodes: 2,
            callbacks: vec![FlowCallbackInfo {
                node_id: "w".into(),
                name: "payment".into(),
                url: "http://10.0.0.5:4000/cb/tok".into(),
            }],
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains(
                r#""callbacks":[{"nodeId":"w","name":"payment","url":"http://10.0.0.5:4000/cb/tok"}]"#
            ),
            "{json}"
        );
    }

    #[test]
    fn flow_run_started_without_callbacks_still_deserializes() {
        let json = r#"{"type":"flowRunStarted","run_id":"01J","flow_name":"Login Flow","collection":"acme","total_nodes":3}"#;
        match serde_json::from_str::<DomainEvent>(json).expect("old payload") {
            DomainEvent::FlowRunStarted { callbacks, .. } => assert!(callbacks.is_empty()),
            other => panic!("unexpected event {other:?}"),
        }
    }
```

In `crates/rocket-app/src/flow_callbacks.rs`, append inside `mod tests`:

```rust
    #[tokio::test]
    async fn infos_list_each_wait_node_with_its_name_and_url_in_node_order() {
        use rocket_shared::events::FlowCallbackInfo;

        let flow = Flow {
            name: "cb".to_string(),
            nodes: vec![wait_node("w1", "first"), wait_node("w2", "second")],
            edges: Vec::new(),
            callback_host: None,
        };
        let fake = FakeCallbackListener::new();
        let listener: Box<dyn CallbackListener> = Box::new(Arc::clone(&fake));

        let callbacks = RunCallbacks::open_all(listener.as_ref(), &flow)
            .await
            .expect("open");

        assert_eq!(
            callbacks.infos(),
            &[
                FlowCallbackInfo {
                    node_id: "w1".to_string(),
                    name: "first".to_string(),
                    url: "http://fake:1/cb/0".to_string(),
                },
                FlowCallbackInfo {
                    node_id: "w2".to_string(),
                    name: "second".to_string(),
                    url: "http://fake:1/cb/1".to_string(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn a_flow_without_wait_nodes_has_no_callback_infos() {
        let flow = Flow {
            name: "plain".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            callback_host: None,
        };
        let callbacks = RunCallbacks::open_all(&crate::callback_listener::NoCallbackListener, &flow)
            .await
            .expect("nothing to open");
        assert!(callbacks.infos().is_empty());
    }
```

In `crates/rocket-app/src/flow_execution_service.rs`, after `only_the_turn_down_event_carries_the_call` (Task 2), add:

```rust
    #[tokio::test]
    async fn run_started_lists_the_callback_urls_and_nothing_else_does() {
        use rocket_shared::events::FlowCallbackInfo;

        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.completed"));
        let publisher = RecordingPublisher::new();
        let flow = Flow {
            name: "cb-urls".to_string(),
            nodes: vec![wait_node_with("w", 60_000, None)],
            edges: Vec::new(),
            callback_host: None,
        };
        let service = service_with_publisher(flow, &publisher)
            .with_callback_listener(Box::new(Arc::clone(&fake)));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary = service.run(&exec, run_input("cb-urls")).await.expect("run");

        let events = publisher.events();
        let callbacks = events
            .iter()
            .find_map(|e| match e {
                DomainEvent::FlowRunStarted { callbacks, .. } => Some(callbacks.clone()),
                _ => None,
            })
            .expect("run started");
        assert_eq!(
            callbacks,
            vec![FlowCallbackInfo {
                node_id: "w".to_string(),
                name: "payment".to_string(),
                url: "http://fake:1/cb/0".to_string(),
            }]
        );
        // The fake's token is "0"; neither the URL nor its token path may appear elsewhere.
        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(!json.contains("http://fake:1/cb/0") && !json.contains("/cb/0"), "{json}");
        for event in events
            .iter()
            .filter(|e| !matches!(e, DomainEvent::FlowRunStarted { .. }))
        {
            let json = serde_json::to_string(event).expect("serialize");
            assert!(!json.contains("/cb/0"), "{json}");
        }
    }
```

Run: `cargo test -j4 -p rocket-app callback && cargo test -j4 -p rocket-shared flow_run_started`
Expected: FAIL to compile (`FlowCallbackInfo`, `callbacks`, `infos` do not exist).

- [ ] **Step 2: Add the type and the event field**

In `crates/rocket-shared/src/events.rs`, after `FlowWaitDetail`, add:

```rust
/// The callback URL of one Wait for callback node. The URL holds a bearer
/// token and works only while its run is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowCallbackInfo {
    pub node_id: String,
    pub name: String,
    pub url: String,
}
```

In `DomainEvent::FlowRunStarted`, add after `total_nodes: usize,`:

```rust
        /// Every Wait for callback node's URL. Sent here only: the URL is a
        /// bearer token, so it never goes into the summary, a step or history.
        /// The nested fields are camelCase inside this snake_case event.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        callbacks: Vec<FlowCallbackInfo>,
```

- [ ] **Step 3: Keep the list in `RunCallbacks`**

In `crates/rocket-app/src/flow_callbacks.rs`, add `use rocket_shared::events::FlowCallbackInfo;` to the imports, add a field to `RunCallbacks`:

```rust
    /// `(node id, name, url)` per Wait for callback node, in node order.
    infos: Vec<FlowCallbackInfo>,
```

in `open_all`, replace

```rust
        let mut vars = HashMap::new();
        let mut endpoints = HashMap::new();
```

with:

```rust
        let mut vars = HashMap::new();
        let mut endpoints = HashMap::new();
        let mut infos = Vec::new();
```

replace

```rust
            vars.insert(format!("{CALLBACK_VAR_PREFIX}{name}"), endpoint.url.clone());
            endpoints.insert(node.id.clone(), endpoint);
        }
        Ok(Self { vars, endpoints })
```

with:

```rust
            vars.insert(format!("{CALLBACK_VAR_PREFIX}{name}"), endpoint.url.clone());
            infos.push(FlowCallbackInfo {
                node_id: node.id.clone(),
                name: name.clone(),
                url: endpoint.url.clone(),
            });
            endpoints.insert(node.id.clone(), endpoint);
        }
        Ok(Self {
            vars,
            endpoints,
            infos,
        })
```

and after `pub(crate) fn vars`, add:

```rust
    /// Every endpoint's node id, name and URL, for the run-started event only.
    pub(crate) fn infos(&self) -> &[FlowCallbackInfo] {
        &self.infos
    }
```

- [ ] **Step 4: Send the list on run start**

In `crates/rocket-app/src/flow_execution_service.rs`, in `run_with_auth`, replace

```rust
        self.events.publish(DomainEvent::FlowRunStarted {
            run_id: run_id.clone(),
            flow_name: input.flow_name.clone(),
            collection: input.collection.clone(),
            total_nodes: flow.nodes.len(),
        });
```

with:

```rust
        self.events.publish(DomainEvent::FlowRunStarted {
            run_id: run_id.clone(),
            flow_name: input.flow_name.clone(),
            collection: input.collection.clone(),
            total_nodes: flow.nodes.len(),
            callbacks: callbacks.infos().to_vec(),
        });
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -j4 -p rocket-shared && cargo test -j4 -p rocket-app flow_`
Expected: PASS, including `flow_run_started_wire_shape` (an empty list is omitted).

- [ ] **Step 6: Gates and commit**

Run: `cargo check -j4 -p rocket-shared -p rocket-app && cargo clippy -j4 -p rocket-app && cargo check -j4 -p rocket`
Expected: no errors, and no new clippy warnings in the files this task touched.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`crates/rocket-shared/src/events.rs crates/rocket-app/src/flow_callbacks.rs crates/rocket-app/src/flow_execution_service.rs`
Suggested subject: `feat(flow): send each callback URL on run start only`.

---

## Self-Review

- **Spec coverage:** Poll: verdict after each attempt, a second progress event after an unmet condition and before the pause with `{ lastStatusCode, conditionMet: false, elapsedMs, remainingMs }`, the pre-send event unchanged, `trace.poll` on success and every failure path (Task 1). Wait: `NodeTrace` passed in, turned-down call masked like `callback_exchange` and capped at 2 KB live and 256 KB in the trace, an immediate live event, the ticker with counts only, `trace.wait` on timeout and error (Task 2). Callback URLs: read-only accessor, `FlowRunStarted.callbacks`, never in the summary (Task 3).
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `FlowLiveProgress`, `FlowPollDetail`, `FlowWaitDetail`, `FlowRejectedCall` and `FlowCallbackInfo` keep the same fields in every task and in P10. P10 reads `live.{lastStatusCode, conditionMet, elapsedMs, remainingMs, ignored, lastRejected}`, `trace.poll.{attempts, maxAttempts, lastStatusCode, conditionMet, elapsedMs, timeoutMs}`, `trace.wait.{ignored, timeoutMs, lastRejected}` and `callbacks[].{nodeId, name, url}`.
- **Review Focus coverage:** item 1 in Task 2, item 2 in Task 1 and Task 3, item 3 in Task 2, item 4 in Task 2 and Task 3, item 5 in Task 1.

Known follow-ups outside this plan: a Request that sends `{{callback.<name>}}` still shows the URL in its own exchange and history, because the user wired it there; masking it needs the URL treated as a run secret (a later item).
