# Flow Async P1 — Repeat Until Engine Loop — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a Request node with `repeat_until` send its request again and again until its condition holds, with progress events, Stop support, one History entry per poll, and an `attempts` count on the step result.

**Architecture:** The Request arm of `FlowExecutionService::execute_node` resolves wires once, then hands off to a new `run_repeat_until` method in `crates/rocket-app/src/flow_poll.rs`. Each attempt calls `execute_capturing` with `skip_history = true`, evaluates the condition with the existing If-node evaluator (`evaluate_flow_route_expression`, `FlowCoercion::Bool`), and sleeps through the P0 `CancelSignal`. The attempt that ends the poll saves its deferred History entry. A successful poll is reported through `ExecutedNode.poll`, which `result_to_step` turns into a `Success` step even for a non-2xx final response.

**Tech Stack:** Rust, tokio (`time`, `sync::watch` via P0), rocket-app services, Deno script engine for real-engine tests.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` (§6.3, §6.4). Index and locked contract: `docs/superpowers/plans/flow-async/00-index.md`.

**Depends on:** plans 01-02 (P0: `CancelSignal`, `NodeRunContext`, `execute_node(…, ctx)`, `publish_progress`, `DomainEvent::FlowStepProgress`, and `run` turning a cancelled node into a `failed "cancelled"` step with `stopped_reason = "cancelled"`) and plan 03 (`RepeatUntil`, `repeat_until` field).

## Global Constraints

- Every attempt runs the pre-request and post-response scripts. Only the attempt that ends the poll is saved to History.
- A send error (no response) or a condition script error fails the node at once, without retrying.
- A non-2xx response is evaluated like any other; a true condition succeeds the node whatever the status.
- Give-up error text: `condition not met after {n} attempts ({secs:.1}s)`.
- Progress message text: `attempt {n}/{max}`.
- Stop between attempts ends the node at once; an in-flight request still finishes.
- Cargo commands use `-j4` and target `rocket-app` (or `rocket-shared`). Never run the full workspace suite.
- Commit each task with the `dev-workflow-skills:1-git-commit` skill.
- Never write the literal panicking-unwrap call text in any file. Use `?`, `expect`, `unwrap_or`.
- `interval_ms` must be at least 100 in every test flow, because `run` validates the flow (rule V9) before executing.

## Review Focus

1. **The condition evaluates to something other than a boolean string** (for example the evaluator returns `"null"`). Expected: the node fails with a clear message instead of looping. `FlowCoercion::Bool` wraps with `!!(…)`, so this should not happen; the code still rejects any other value. Pinned in Task 3 (`poll_condition_script_error_fails_at_once` covers the error path; the non-boolean branch is covered by the `match` returning `InvalidInput`).
2. **The deadline falls in the middle of an interval.** Expected: the pause is cut short, one last attempt is sent at the deadline, and the node gives up. Pinned in Task 3 (`poll_gives_up_at_the_deadline`).
3. **Stop is pressed while the poll is sleeping.** Expected: the node fails with `cancelled` within a second and the run's `stopped_reason` is `cancelled`. Pinned in Task 3 (`poll_stops_between_attempts_when_cancelled`).
4. **A poll that gives up.** Expected: its last attempt is still in History, exactly once. Pinned in Task 3 (`poll_saves_one_history_entry_even_when_it_gives_up`).
5. **Debug mode on a polling node.** Expected: the debug record shows the last attempt, not the first. Pinned in Task 3 (`poll_debug_record_shows_the_last_attempt`).

---

### Task 1: Deferred History on `execute_capturing`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`ExecuteRequestInput` :29-104, `ExecuteRequestOutput` :127-133, `finish_phases` History block :1410-1426 and return literal :1436-1441, new method `save_deferred_history` after `finish_phases`)
- Modify (add `skip_history: false` to every `ExecuteRequestInput { … }` literal): `crates/rocket-app/src/execution_service.rs` (3 sites), `crates/rocket-app/src/flow_execution_service.rs` (1), `crates/rocket-app/src/load_test_service.rs` (1), `crates/rocket-app/src/runner_sequence.rs` (2)
- Modify (add `deferred_history: None` to every other `ExecuteRequestOutput { … }` literal): `crates/rocket-app/src/execution_service.rs`, `crates/rocket-app/src/flow_execution_service.rs` (test helpers such as `sample_response_output` :1499)
- Test: `crates/rocket-app/src/execution_service.rs` (tests module, next to `execute_capturing_records_the_request_after_the_pre_request_script` :4520)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `ExecuteRequestInput.skip_history: bool` (`#[serde(default)]`).
  - `ExecuteRequestOutput.deferred_history: Option<rocket_history::HistoryEntry>`.
  - `RequestExecutionService::save_deferred_history(&self, entry: &rocket_history::HistoryEntry)` (`pub(crate)`).

- [ ] **Step 1: Write the failing tests**

Add to the `execution_service.rs` tests module:

```rust
    fn history_svc() -> (RequestExecutionService, Arc<Mutex<Vec<HistoryEntry>>>) {
        let history_repo = Box::new(MockHistoryRepo::new());
        let saved = history_repo.saved_entries_handle();
        let svc = RequestExecutionService::new(
            Box::new(MockEnvRepo::with_env(Environment::new("dev"))),
            Arc::new(MockExecutor::new(200)),
            history_repo,
            Box::new(StubCollectionRepo::empty()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo::with_connection(test_connection("conn-1"))),
            Arc::new(FakeSecretStore),
            Arc::new(FakeVaultFetcher::new(vec![])),
        );
        (svc, saved)
    }

    #[tokio::test]
    async fn execute_capturing_saves_history_by_default() {
        let (svc, saved) = history_svc();
        let mut sent = None;
        let out = svc
            .execute_capturing(
                sample_input("https://example.com/a", None),
                &std::collections::HashMap::new(),
                &mut sent,
            )
            .await
            .expect("execute");
        assert_eq!(saved.lock().expect("lock").len(), 1);
        assert!(out.deferred_history.is_none());
    }

    #[tokio::test]
    async fn execute_capturing_with_skip_history_defers_the_entry() {
        let (svc, saved) = history_svc();
        let mut input = sample_input("https://example.com/a", None);
        input.skip_history = true;
        let mut sent = None;
        let out = svc
            .execute_capturing(input, &std::collections::HashMap::new(), &mut sent)
            .await
            .expect("execute");

        assert_eq!(saved.lock().expect("lock").len(), 0, "nothing saved yet");
        let entry = out.deferred_history.expect("the entry is handed back");
        assert_eq!(entry.status, 200);

        svc.save_deferred_history(&entry);
        assert_eq!(saved.lock().expect("lock").len(), 1);
    }
```

If `FakeVaultFetcher::new` has a different signature in this module, use whatever the existing `history_entry_redacts_external_secret_value_from_the_url` test (:2651) passes, with an empty value list. If `HistoryEntry` names its status field differently, assert on that field.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app execute_capturing_`
Expected: FAIL to compile, "no field `skip_history`", "no field `deferred_history`", "no method named `save_deferred_history`".

- [ ] **Step 3: Implement**

In `ExecuteRequestInput`, after `request_guard_policy`:

```rust
    /// When true, the History entry is not saved. It is returned in
    /// `ExecuteRequestOutput::deferred_history` so the caller can save it
    /// later, as a Flow poll does for its final attempt only.
    #[serde(default)]
    pub skip_history: bool,
```

In `ExecuteRequestOutput`:

```rust
    /// The History entry of a request run with `skip_history`, not yet saved.
    pub deferred_history: Option<HistoryEntry>,
```

In `finish_phases`, replace `let _ = self.history_repo.save(&entry);` with:

```rust
        let deferred_history = if input.skip_history {
            Some(entry)
        } else {
            let _ = self.history_repo.save(&entry);
            None
        };
```

and add `deferred_history,` to the returned `ExecuteRequestOutput`.

After `finish_phases`, add:

```rust
    /// Saves a History entry returned in `deferred_history`. A failure is
    /// ignored, like the save in `finish_phases`.
    pub(crate) fn save_deferred_history(&self, entry: &HistoryEntry) {
        let _ = self.history_repo.save(entry);
    }
```

Then run `cargo check -j4 -p rocket-app --tests` and add `skip_history: false,` / `deferred_history: None,` to each literal the compiler flags. Also run `cargo check -j4 -p rocket --tests` in case `src-tauri` builds either struct.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app execute_capturing_ && cargo test -j4 -p rocket-app history`
Expected: PASS, including the existing history tests.

- [ ] **Step 5: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): let a request defer its History entry`.

---

### Task 2: `attempts` on step results and `ExecutedNode.poll`

**Files:**
- Create: `crates/rocket-app/src/flow_poll.rs` (only `PollStats` in this task)
- Modify: `crates/rocket-app/src/lib.rs:14-16` (add `pub(crate) mod flow_poll;`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (`ExecutedNode` :27-40, `FlowStepResult` :466-493, `step_completed_event` :495-510, `result_to_step` :974-1030, `skipped_step`/`failed_step` :1033-1060, the two `FlowStepResult` literals in tests :3076, :3093)
- Modify: `crates/rocket-shared/src/events.rs` (`FlowStepCompleted` :236-266, test literals :732-989)
- Test: `crates/rocket-app/src/flow_execution_service.rs` (tests), `crates/rocket-shared/src/events.rs` (tests)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `crate::flow_poll::PollStats { attempts: u32, elapsed_ms: u64 }` (`Debug, Clone, Copy, PartialEq, Eq`).
  - `ExecutedNode.poll: Option<PollStats>` (`pub(crate)`), `None` from `ExecutedNode::plain` and every existing literal.
  - `FlowStepResult.attempts: Option<u32>` and `DomainEvent::FlowStepCompleted.attempts: Option<u32>`, both `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  - `result_to_step`: `Ok(ExecutedNode { output: CapturedOutput::Request(out), poll: Some(stats), .. })` → `Success`, `status_code: Some(out.response.status)`, `duration_ms: Some(stats.elapsed_ms)`, `attempts: Some(stats.attempts)`.

- [ ] **Step 1: Write the failing tests**

In `flow_execution_service.rs` tests:

```rust
    #[test]
    fn a_met_poll_is_a_success_even_on_a_non_2xx_response() {
        let mut out = sample_response_output();
        out.response.status = 404;
        let node = request_flow_node("job", "https://api.example.com/job");
        let executed = ExecutedNode {
            output: CapturedOutput::Request(Box::new(out)),
            chosen_exit: handle::RESULT.to_string(),
            poll: Some(crate::flow_poll::PollStats {
                attempts: 7,
                elapsed_ms: 14_200,
            }),
        };

        let step = result_to_step("job", Some(&node), &Ok(executed));

        assert_eq!(step.status, FlowNodeStatus::Success);
        assert_eq!(step.status_code, Some(404));
        assert_eq!(step.duration_ms, Some(14_200));
        assert_eq!(step.attempts, Some(7));
        assert_eq!(step.error, None);
    }

    #[test]
    fn a_plain_request_step_has_no_attempts() {
        let node = request_flow_node("r", "https://api.example.com/r");
        let executed = ExecutedNode::plain(CapturedOutput::Request(Box::new(
            sample_response_output(),
        )));
        let step = result_to_step("r", Some(&node), &Ok(executed));
        assert_eq!(step.attempts, None);
        let json = serde_json::to_value(&step).expect("serialize");
        assert!(json.get("attempts").is_none(), "None attempts are omitted");
    }

    #[test]
    fn step_attempts_serialize_camel_case_and_reach_the_event() {
        let step = FlowStepResult {
            attempts: Some(3),
            ..failed_step("job", "x".into())
        };
        let json = serde_json::to_value(&step).expect("serialize");
        assert_eq!(json["attempts"], 3);
        match step_completed_event("run", &step) {
            DomainEvent::FlowStepCompleted { attempts, .. } => assert_eq!(attempts, Some(3)),
            other => panic!("unexpected event {other:?}"),
        }
    }
```

Check `handle` is in scope in the tests module (`use rocket_flow::handle;` if not).

In `crates/rocket-shared/src/events.rs` tests:

```rust
    #[test]
    fn flow_step_completed_attempts_is_optional_and_omitted_when_none() {
        let json = r#"{"type":"flowStepCompleted","run_id":"r","node_id":"n","status":"success","status_code":200,"duration_ms":5,"error":null,"value":null}"#;
        let event: DomainEvent = serde_json::from_str(json).expect("deserialize old payload");
        match &event {
            DomainEvent::FlowStepCompleted { attempts, .. } => assert_eq!(*attempts, None),
            other => panic!("unexpected {other:?}"),
        }
        let back = serde_json::to_string(&event).expect("serialize");
        assert!(!back.contains("attempts"), "got: {back}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-shared flow_step_completed_attempts` then `cargo test -j4 -p rocket-app poll`
Expected: FAIL to compile, "no field `attempts`", "no field `poll`", "unresolved module `flow_poll`".

- [ ] **Step 3: Implement**

Create `crates/rocket-app/src/flow_poll.rs`:

```rust
//! Repeat until: sends one Request node's request until its condition holds.

/// How a successful repeat-until poll went, for its step result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollStats {
    pub(crate) attempts: u32,
    /// Time from the first send to the attempt that met the condition.
    pub(crate) elapsed_ms: u64,
}
```

Register it in `lib.rs`: `pub(crate) mod flow_poll;` next to `flow_debug`.

In `flow_execution_service.rs`:
- `ExecutedNode` gains:

```rust
    /// Set by a repeat-until poll that met its condition.
    pub(crate) poll: Option<crate::flow_poll::PollStats>,
```

  `ExecutedNode::plain` sets `poll: None`; the If and Switch arms' `Ok(ExecutedNode { … })` literals add `poll: None`.
- `FlowStepResult` gains, after `debug_request`:

```rust
    /// How many times a repeat-until Request node sent its request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
```

- `step_completed_event` passes `attempts: step.attempts,`.
- `result_to_step`: add `attempts: None` to `base`, and insert this arm directly after the `is_routing` arm:

```rust
        // A poll that met its condition succeeds whatever the final status:
        // the author's condition decides "done".
        Ok(ExecutedNode {
            output: CapturedOutput::Request(out),
            poll: Some(stats),
            ..
        }) => FlowStepResult {
            status_code: Some(out.response.status),
            duration_ms: Some(stats.elapsed_ms),
            attempts: Some(stats.attempts),
            ..base
        },
```

- `skipped_step` and `failed_step` add `attempts: None`; so do the test literals at :3076 and :3093.

In `crates/rocket-shared/src/events.rs`, `FlowStepCompleted` gains after `debug_request`:

```rust
        /// How many times a repeat-until Request node sent its request.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempts: Option<u32>,
```

Run `cargo check -j4 -p rocket-shared --tests && cargo check -j4 -p rocket-app --tests && cargo check -j4 -p rocket --tests` and add `attempts: None,` (literals) or `attempts,`/`..` (exhaustive patterns such as :859) where flagged.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-shared flow_step && cargo test -j4 -p rocket-app flow_execution_service`
Expected: PASS.

- [ ] **Step 5: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): report poll attempts on step results`.

---

### Task 3: The repeat-until loop

**Files:**
- Modify: `crates/rocket-app/src/flow_poll.rs` (add `run_repeat_until`)
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (Request arm of `execute_node` :828-877; make `to_flow_logs` :221 `pub(crate)`; tests module)
- Modify: `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md` (§6.3 step 2.2 and 2.6, §6.4)
- Test: `crates/rocket-app/src/flow_execution_service.rs` (tests module, new `// ---- Repeat until ----` section at the end)

**Interfaces:**
- Consumes: `CancelSignal::sleep`, `NodeRunContext`, `FlowExecutionService::publish_progress` (P0, `pub(crate)`); `RepeatUntil` (plan 03); `skip_history`, `deferred_history`, `save_deferred_history` (Task 1); `PollStats`, `ExecutedNode.poll` (Task 2).
- Produces: `FlowExecutionService::run_repeat_until(&self, exec, input, request_input, repeat, external_secrets, secret_values, debug_on, logs, debug, ctx) -> DomainResult<ExecutedNode>` exactly as in the index.

- [ ] **Step 1: Add the test helpers**

At the end of the `flow_execution_service.rs` tests module:

```rust
    // ---- Repeat until -------------------------------------------------------

    use crate::test_doubles::{InMemoryHistoryRepo, SharedHistoryRepo};
    use rocket_flow::RepeatUntil;

    /// Answers each send with the next `(status, body)` in `script` and keeps
    /// answering with the last one after that. Status 0 fails the send.
    struct SequenceExecutor {
        script: Vec<(u16, &'static str)>,
        sent: std::sync::Mutex<usize>,
    }
    impl SequenceExecutor {
        fn new(script: Vec<(u16, &'static str)>) -> Arc<Self> {
            Arc::new(Self {
                script,
                sent: std::sync::Mutex::new(0),
            })
        }
        fn sent_count(&self) -> usize {
            *self.sent.lock().expect("lock")
        }
    }
    #[async_trait]
    impl HttpExecutor for SequenceExecutor {
        async fn execute(&self, _request: &HttpRequest) -> DomainResult<HttpResponse> {
            let index = {
                let mut sent = self.sent.lock().expect("lock");
                *sent += 1;
                *sent - 1
            };
            let (status, body) = self.script[index.min(self.script.len() - 1)];
            if status == 0 {
                return Err(DomainError::Http("connection refused".into()));
            }
            Ok(HttpResponse {
                status,
                status_text: "X".into(),
                headers: vec![],
                body: body.into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: body.len(),
            })
        }
    }

    fn poll_exec(
        executor: &Arc<SequenceExecutor>,
        engine: Box<dyn ScriptEngine>,
        history: &Arc<InMemoryHistoryRepo>,
    ) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(NullEnvRepo),
            Arc::clone(executor) as Arc<dyn HttpExecutor>,
            Box::new(SharedHistoryRepo(Arc::clone(history))),
            Box::new(FakeCollectionRepo::new()),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        )
        .with_script_engine(engine)
    }

    fn repeat(condition: &str, interval_ms: u64, max_attempts: u32, timeout_ms: u64) -> RepeatUntil {
        RepeatUntil {
            condition: condition.to_string(),
            interval_ms,
            max_attempts,
            timeout_ms,
        }
    }

    /// One polling Request node, id `job`.
    fn poll_flow(repeat_until: RepeatUntil, debug_on: bool) -> Flow {
        let mut node = request_flow_node("job", "https://api.example.com/job");
        if let FlowNodeKind::Request {
            repeat_until: slot,
            debug,
            ..
        } = &mut node.kind
        {
            *slot = Some(repeat_until);
            *debug = debug_on;
        }
        Flow {
            name: "poll".to_string(),
            nodes: vec![node],
            edges: Vec::new(),
        }
    }

    /// A scripted engine whose Bool conditions are true when the response
    /// status is `ok_status`.
    fn status_condition(ok_status: u16) -> Box<dyn ScriptEngine> {
        // `FromResponse` takes a fn pointer, so each status needs its own fn.
        fn is_200(r: Option<&rocket_http::HttpResponse>) -> serde_json::Value {
            serde_json::json!(r.map(|r| r.status == 200).unwrap_or(false))
        }
        fn is_404(r: Option<&rocket_http::HttpResponse>) -> serde_json::Value {
            serde_json::json!(r.map(|r| r.status == 404).unwrap_or(false))
        }
        let f = match ok_status {
            200 => is_200 as fn(Option<&rocket_http::HttpResponse>) -> serde_json::Value,
            404 => is_404,
            other => panic!("no condition fn for {other}"),
        };
        scripted(vec![("!!(", Scripted::FromResponse(f))])
    }
```

If `NullEnvRepo`, `NullCookieRepo`, `EmptySecretManagerRepo`, `FakeCollectionRepo` or `NullEventPublisher` resolve to both a local mock and a `test_doubles` type, use the local ones the existing `recording_exec` helper (:2674) uses.

- [ ] **Step 2: Write the failing loop tests**

```rust
    #[tokio::test]
    async fn poll_succeeds_on_the_attempt_where_the_condition_holds() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), false);

        let summary = service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Success);
        assert_eq!(step.status_code, Some(200));
        assert_eq!(step.attempts, Some(3));
        assert_eq!(executor.sent_count(), 3);
        assert_eq!(history.saved_count(), 1, "only the final attempt is kept");
    }

    #[tokio::test]
    async fn poll_gives_up_after_max_attempts() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 3, 10_000), false);

        let summary = service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        let error = step.error.clone().expect("an error");
        assert!(error.starts_with("condition not met after 3 attempts ("), "got: {error}");
        assert_eq!(executor.sent_count(), 3);
    }

    #[tokio::test]
    async fn poll_saves_one_history_entry_even_when_it_gives_up() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 3, 10_000), false);

        service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        assert_eq!(history.saved_count(), 1);
    }

    #[tokio::test]
    async fn poll_gives_up_at_the_deadline() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        // 100 ms apart, 250 ms deadline: sends at about 0, 100, 200 and 250 ms.
        let flow = poll_flow(repeat("response.status === 200", 100, 1000, 250), false);
        let started = std::time::Instant::now();

        let summary = service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert!(step.error.as_deref().unwrap_or("").starts_with("condition not met after "));
        let sent = executor.sent_count();
        assert!((2..=6).contains(&sent), "sent {sent} times");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[tokio::test]
    async fn poll_condition_true_on_a_non_2xx_response_succeeds() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(404), &history);
        let flow = poll_flow(repeat("response.status === 404", 100, 5, 10_000), false);

        let summary = service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Success);
        assert_eq!(step.status_code, Some(404));
        assert_eq!(step.attempts, Some(1));
    }

    #[tokio::test]
    async fn poll_condition_script_error_fails_at_once() {
        let executor = SequenceExecutor::new(vec![(200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(
            &executor,
            scripted(vec![("!!(", Scripted::Throw("ReferenceError: nope"))]),
            &history,
        );
        let flow = poll_flow(repeat("response.body.done", 100, 5, 10_000), false);

        let summary = service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert!(step.error.as_deref().unwrap_or("").contains("nope"));
        assert_eq!(executor.sent_count(), 1, "a script error is not retried");
    }

    #[tokio::test]
    async fn poll_send_error_fails_at_once() {
        let executor = SequenceExecutor::new(vec![(0, "")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), false);

        let summary = service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        assert_eq!(step_of(&summary, "job").status, FlowNodeStatus::Failed);
        assert_eq!(executor.sent_count(), 1, "a send error is not retried");
    }

    #[tokio::test]
    async fn poll_publishes_progress_for_each_attempt() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            poll_flow(repeat("response.status === 200", 100, 5, 10_000), false),
            &publisher,
        );

        service.run(&exec, run_input("poll")).await.expect("run");

        let progress: Vec<(Option<u32>, String)> = publisher
            .events()
            .into_iter()
            .filter_map(|e| match e {
                DomainEvent::FlowStepProgress {
                    node_id,
                    attempt,
                    message,
                    ..
                } if node_id == "job" => Some((attempt, message)),
                _ => None,
            })
            .collect();
        assert_eq!(
            progress,
            vec![
                (Some(1), "attempt 1/5".to_string()),
                (Some(2), "attempt 2/5".to_string()),
                (Some(3), "attempt 3/5".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn poll_stops_between_attempts_when_cancelled() {
        let executor = SequenceExecutor::new(vec![(404, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let publisher = RecordingPublisher::new();
        // A 30 s interval: without Stop the test would hang for 30 s.
        let service = service_with_publisher(
            poll_flow(repeat("response.status === 200", 30_000, 5, 60_000), false),
            &publisher,
        );
        let started = std::time::Instant::now();

        let run = service.run(&exec, run_input("poll"));
        let stop = async {
            loop {
                let events = publisher.events();
                let run_id = events.iter().find_map(|e| match e {
                    DomainEvent::FlowRunStarted { run_id, .. } => Some(run_id.clone()),
                    _ => None,
                });
                let polling = events.iter().any(|e| {
                    matches!(e, DomainEvent::FlowStepProgress { node_id, .. } if node_id == "job")
                });
                if let (Some(id), true) = (run_id, polling) {
                    service.cancel(&id);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        };
        let (summary, ()) = tokio::join!(run, stop);
        let summary = summary.expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Failed);
        assert_eq!(step.error.as_deref(), Some("cancelled"));
        assert_eq!(summary.stopped_reason, "cancelled");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[tokio::test]
    async fn poll_debug_record_shows_the_last_attempt() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (200, "{}")]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), true);

        let summary = service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        let debug = step_of(&summary, "job").debug_request.clone().expect("debug record");
        assert_eq!(debug.response.expect("a response").status, 200);
    }

    #[tokio::test]
    async fn real_engine_poll_waits_for_a_body_field() {
        let executor = SequenceExecutor::new(vec![
            (200, r#"{"status":"pending"}"#),
            (200, r#"{"status":"done"}"#),
        ]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(
            &executor,
            Box::new(rocket_infra::scripting::DenoScriptEngine::new()),
            &history,
        );
        let flow = poll_flow(repeat(r#"response.body.status === "done""#, 100, 5, 10_000), false);

        let summary = service_with_flow(flow).run(&exec, run_input("poll")).await.expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.status, FlowNodeStatus::Success, "error: {:?}", step.error);
        assert_eq!(step.attempts, Some(2));
    }
```

Only `real_engine_poll_waits_for_a_body_field` runs real JavaScript. The others use the scripted fake: its `"!!("` needle matches only Bool-coerced conditions, and `FromResponse` reads the status the fake receives.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app poll_`
Expected: FAIL. The polling tests see `attempts: None` and a single send (the Request arm ignores `repeat_until`), for example `assertion failed: left: None, right: Some(3)`.

- [ ] **Step 4: Implement `run_repeat_until`**

Make `to_flow_logs` in `flow_execution_service.rs` `pub(crate)`.

Append to `crates/rocket-app/src/flow_poll.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use rocket_flow::{handle, RepeatUntil};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{FlowDebugRequest, FlowLogEntry};

use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
use crate::flow_debug::build_debug_request;
use crate::flow_execution_service::{
    to_flow_logs, CapturedOutput, ExecutedNode, FlowCoercion, FlowExecutionService,
    NodeRunContext, RunFlowInput,
};

impl FlowExecutionService {
    /// Sends `request_input` until `repeat.condition` is truthy. Each attempt
    /// runs the request's scripts. Only the attempt that ends the poll is
    /// saved to History. A send error or a condition script error fails at
    /// once; giving up fails with "condition not met after …".
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_repeat_until(
        &self,
        exec: &RequestExecutionService,
        input: &RunFlowInput,
        request_input: ExecuteRequestInput,
        repeat: &RepeatUntil,
        external_secrets: &HashMap<String, String>,
        secret_values: &HashSet<String>,
        debug_on: bool,
        logs: &mut Vec<FlowLogEntry>,
        debug: &mut Option<FlowDebugRequest>,
        ctx: &mut NodeRunContext,
    ) -> DomainResult<ExecutedNode> {
        let started = tokio::time::Instant::now();
        let deadline = started + Duration::from_millis(repeat.timeout_ms);
        let interval = Duration::from_millis(repeat.interval_ms);
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            self.publish_progress(
                ctx,
                Some(attempt),
                Some(repeat.max_attempts),
                format!("attempt {attempt}/{}", repeat.max_attempts),
            );

            let mut attempt_input = request_input.clone();
            attempt_input.skip_history = true;
            let mut sent = None;
            let result = exec
                .execute_capturing(attempt_input, external_secrets, &mut sent)
                .await;
            if debug_on {
                if let Some(sent) = &sent {
                    let error = result.as_ref().err().map(|e| e.to_string());
                    *debug = Some(build_debug_request(
                        sent,
                        result.as_ref().ok().map(|o| &o.response),
                        error.as_deref(),
                        secret_values,
                    ));
                }
            }
            // A send that got no response is not retried.
            let mut output = result?;
            logs.extend(to_flow_logs(output.console_entries.clone()));
            let history = output.deferred_history.take();
            let captured = CapturedOutput::Request(Box::new(output));

            let outcome = exec
                .evaluate_flow_route_expression(
                    &input.collection,
                    &captured,
                    &repeat.condition,
                    FlowCoercion::Bool,
                    secret_values,
                )
                .await;
            logs.extend(outcome.logs);
            let verdict = outcome.result.and_then(|raw| match raw.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                other => Err(DomainError::InvalidInput(format!(
                    "repeat-until condition evaluated to '{other}', expected true or false"
                ))),
            });

            let now = tokio::time::Instant::now();
            let elapsed = now - started;
            let gives_up = attempt >= repeat.max_attempts || now >= deadline;
            let ends_poll = !matches!(verdict, Ok(false)) || gives_up;
            if ends_poll {
                if let Some(entry) = &history {
                    exec.save_deferred_history(entry);
                }
            }
            match verdict {
                Err(e) => return Err(e),
                Ok(true) => {
                    return Ok(ExecutedNode {
                        output: captured,
                        chosen_exit: handle::RESULT.to_string(),
                        poll: Some(PollStats {
                            attempts: attempt,
                            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                        }),
                    })
                }
                Ok(false) if gives_up => {
                    return Err(DomainError::InvalidInput(format!(
                        "condition not met after {attempt} attempts ({:.1}s)",
                        elapsed.as_secs_f64()
                    )))
                }
                Ok(false) => {}
            }

            // The pause is cut short so the last attempt lands on the deadline.
            let pause = interval.min(deadline - now);
            ctx.cancel
                .sleep(pause)
                .await
                .map_err(|_| DomainError::Internal("cancelled".to_string()))?;
        }
    }
}
```

Plan 01 defines `NodeRunContext` in `flow_execution_service.rs` (after `FlowRunSummary`). Check the actual P0 code before writing the imports.

Now that `publish_progress` and `CancelSignal::sleep` have a production caller, remove the `#[cfg_attr(not(test), allow(dead_code))]` attribute from `publish_progress` (plan 02). In `flow_cancel.rs` the attribute sits on the whole `impl CancelSignal` block (plan 01); move it onto `cancelled` alone, because plan 08 is its first production caller.

- [ ] **Step 5: Route the Request arm into the loop**

In `execute_node`, change the Request arm's pattern to:

```rust
            FlowNodeKind::Request {
                debug: debug_on,
                repeat_until,
                ..
            } => {
```

and directly after `apply_wired_overrides(&mut request_input, &resolved, &edges_owned)?;` add:

```rust
                // Wires were resolved once above; every attempt reuses them.
                if let Some(repeat) = repeat_until {
                    return self
                        .run_repeat_until(
                            exec,
                            input,
                            request_input,
                            repeat,
                            external_secrets,
                            &secret_values,
                            *debug_on,
                            logs,
                            debug,
                            ctx,
                        )
                        .await;
                }
```

In the `execute_node` signature, rename plan 01's `_ctx: &mut NodeRunContext` parameter to `ctx: &mut NodeRunContext`, since this arm now reads it.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app poll_ && cargo test -j4 -p rocket-app real_engine_poll && cargo test -j4 -p rocket-app flow_execution_service`
Expected: PASS. The whole run takes a few seconds because of the 100 ms intervals.

Then: `cargo clippy -j4 -p rocket-app --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Update the spec to match the design**

In `docs/superpowers/specs/2026-09-29-flow-async-poll-callback-design.md`:
- §6.3 step 2.2: replace "History is skipped for this attempt (6.4)." with "The attempt runs with `skip_history = true`, so its History entry is returned in `deferred_history` instead of being saved (6.4)."
- §6.3 step 2.6: replace "`result_to_step` gets this case from a `condition_met` flag rather than the status code." with "`result_to_step` gets this case from `ExecutedNode.poll` (`PollStats`) rather than the status code."
- §6.4: replace the first paragraph with: "`ExecuteRequestInput` gains `skip_history: bool` (serde default `false`). When it is true, `finish_phases` builds the History entry as usual but returns it in `ExecuteRequestOutput::deferred_history` instead of saving it. The poll loop sets it on every attempt and saves only the entry of the attempt that ends the poll, through `RequestExecutionService::save_deferred_history`. Every other caller keeps the default, so History behaves as before."

- [ ] **Step 8: Commit**

Commit with the `dev-workflow-skills:1-git-commit` skill. Suggested message: `feat(flow): poll a Request node until its condition holds`.
