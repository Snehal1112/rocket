# Plan 01 — Exchange record and storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every Request node (and an accepted Wait for callback) reports a masked, size-capped record of the request it sent and the response it got, on every run, and the frontend stores it with the step's logs.

**Architecture:** The backend reuses the Debug-mode record builder (`build_debug_request`) for a new always-on `exchange` field on `FlowStepResult` and `FlowStepCompleted`, capped at 262 144 body bytes with a `truncated` flag. `debug_request` stays Debug-only and keeps feeding the Console. Input steps also report their resolved value. The frontend mirrors the types, keeps `exchange` and `logs` in `FlowNodeDetail`, and gets a small sensitive-header helper for plan 02.

**Tech Stack:** Rust (rocket-shared, rocket-app), TypeScript/React (Vitest).

**Spec:** `docs/superpowers/specs/2026-09-30-flow-properties-panel-tabs-design.md` (§5.1, §5.2, §4.2). Contract: `docs/superpowers/plans/flow-properties-tabs/00-index.md`.

## Global Constraints

- `EXCHANGE_BODY_LIMIT` is `262_144` bytes; the cut never splits a UTF-8 character.
- `FlowDebugResponse.truncated`: `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`, so records that were not cut serialize exactly as before.
- `exchange` on `FlowStepResult` and `FlowStepCompleted`: `#[serde(default, skip_serializing_if = "Option::is_none")]`. The event holds `Option<Box<FlowDebugRequest>>`, like `debug_request`.
- `exchange` uses the same masking as the Debug-mode record (`build_debug_request`, `redact_secrets`, `is_sensitive_header`, `REDACTED`).
- `debug_request` is still set only when a node's Debug mode is on.
- Event payload fields stay snake_case; the nested record is camelCase (as `debug_request` is today).
- Cargo: `-j4`, one crate at a time. Never the whole workspace suite. Never write the literal panicking-unwrap call text.
- Commit each task with the `dev-workflow-skills:1-git-commit` skill. Never `git stash`.

## Review Focus

1. **A response body with a multi-byte character exactly at the 262 144-byte cut.** Expected: the body is cut before that character, is valid UTF-8, and `truncated` is `true`. Pinned in Task 1 (`cap_exchange_never_splits_a_utf8_character`).
2. **A plain Request with Debug mode off.** Expected: `exchange` is present and masked; `debug_request` is absent. Pinned in Task 2 (`a_request_without_debug_still_reports_a_masked_exchange`).
3. **A request that fails to send (no response).** Expected: `exchange` carries the request and the error, no response. Pinned in Task 2 (`a_request_that_fails_to_send_reports_its_exchange_with_the_error`).
4. **A polled Request that takes three attempts.** Expected: `exchange` shows the last attempt's response. Pinned in Task 2 (`a_polled_request_reports_the_last_attempts_exchange`).
5. **An old frontend/summary payload without `exchange` or `truncated`.** Expected: it deserializes with `None` / `false`. Pinned in Task 1 (`flow_step_completed_without_exchange_still_deserializes`).

---

### Task 1: `truncated`, `cap_exchange`, `callback_exchange` and the event field

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-shared/src/events.rs:46-56` (`FlowDebugResponse`), `:247-279` (`FlowStepCompleted`), tests module (~`:760-830`)
- Modify: `crates/rocket-app/src/flow_debug.rs` (constant, `cap_exchange`, `callback_exchange`, `truncated: false` in `build_debug_request`, tests)
- Modify: every `DomainEvent::FlowStepCompleted { .. }` literal the compiler reports (add `exchange: None`), including `crates/rocket-app/src/flow_execution_service.rs:600-615` (`step_completed_event`, set properly in Task 2 — use `exchange: None` here)

**Interfaces:**
- Produces: `FlowDebugResponse.truncated: bool`; `DomainEvent::FlowStepCompleted.exchange: Option<Box<FlowDebugRequest>>`; `pub(crate) const EXCHANGE_BODY_LIMIT: usize = 262_144`; `pub(crate) fn cap_exchange(record: FlowDebugRequest) -> FlowDebugRequest`; `pub(crate) fn callback_exchange(call: &ReceivedCall, duration_ms: u64, secret_values: &HashSet<String>) -> FlowDebugRequest`.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-shared/src/events.rs` tests module, add:

```rust
    #[test]
    fn flow_step_completed_without_exchange_still_deserializes() {
        let json = r#"{"type":"flowStepCompleted","run_id":"r","node_id":"n","status":"success","status_code":200,"duration_ms":5,"error":null,"value":null}"#;
        let event: DomainEvent = serde_json::from_str(json).expect("old payload");
        match event {
            DomainEvent::FlowStepCompleted { exchange, .. } => assert!(exchange.is_none()),
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn a_response_that_was_not_cut_serializes_without_truncated() {
        let response = FlowDebugResponse {
            status: 200,
            status_text: "OK".into(),
            duration_ms: 1,
            size_bytes: 2,
            headers: Vec::new(),
            body: "{}".into(),
            truncated: false,
        };
        let json = serde_json::to_string(&response).expect("serialize");
        assert!(!json.contains("truncated"), "{json}");
        let old: FlowDebugResponse = serde_json::from_str(
            r#"{"status":200,"statusText":"OK","durationMs":1,"sizeBytes":2,"headers":[],"body":"{}"}"#,
        )
        .expect("old record");
        assert!(!old.truncated);
    }

    #[test]
    fn flow_step_completed_carries_the_exchange_in_camel_case() {
        let event = DomainEvent::FlowStepCompleted {
            run_id: "r".into(),
            node_id: "n".into(),
            status: FlowNodeStatus::Success,
            status_code: Some(200),
            duration_ms: Some(5),
            error: None,
            value: None,
            skip_reason: None,
            branch: None,
            logs: Vec::new(),
            debug_request: None,
            attempts: None,
            exchange: Some(Box::new(FlowDebugRequest {
                method: "GET".into(),
                url: "https://x.test".into(),
                headers: Vec::new(),
                body: None,
                response: Some(FlowDebugResponse {
                    status: 200,
                    status_text: "OK".into(),
                    duration_ms: 5,
                    size_bytes: 300_000,
                    headers: Vec::new(),
                    body: "x".into(),
                    truncated: true,
                }),
                error: None,
            })),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains(r#""exchange":{"method":"GET""#), "{json}");
        assert!(json.contains(r#""sizeBytes":300000"#), "{json}");
        assert!(json.contains(r#""truncated":true"#), "{json}");
    }
```

In `crates/rocket-app/src/flow_debug.rs` tests module, add:

```rust
    fn response_record(body: String) -> FlowDebugRequest {
        FlowDebugRequest {
            method: "GET".into(),
            url: "https://x.test".into(),
            headers: Vec::new(),
            body: None,
            response: Some(FlowDebugResponse {
                status: 200,
                status_text: "OK".into(),
                duration_ms: 1,
                size_bytes: body.len() as u64,
                headers: Vec::new(),
                body,
                truncated: false,
            }),
            error: None,
        }
    }

    #[test]
    fn cap_exchange_keeps_a_body_at_the_limit_untouched() {
        let body = "a".repeat(EXCHANGE_BODY_LIMIT);
        let capped = cap_exchange(response_record(body.clone()));
        let response = capped.response.expect("response");
        assert_eq!(response.body, body);
        assert!(!response.truncated);
    }

    #[test]
    fn cap_exchange_cuts_a_long_body_and_keeps_the_real_size() {
        let body = "a".repeat(EXCHANGE_BODY_LIMIT + 10);
        let capped = cap_exchange(response_record(body));
        let response = capped.response.expect("response");
        assert_eq!(response.body.len(), EXCHANGE_BODY_LIMIT);
        assert!(response.truncated);
        assert_eq!(response.size_bytes, (EXCHANGE_BODY_LIMIT + 10) as u64);
    }

    #[test]
    fn cap_exchange_never_splits_a_utf8_character() {
        // "é" is two bytes. It starts one byte before the limit, so a naive
        // cut would split it.
        let mut body = "a".repeat(EXCHANGE_BODY_LIMIT - 1);
        body.push('é');
        body.push_str("tail");
        let capped = cap_exchange(response_record(body));
        let response = capped.response.expect("response");
        assert_eq!(response.body.len(), EXCHANGE_BODY_LIMIT - 1);
        assert!(response.body.chars().all(|c| c == 'a'));
        assert!(response.truncated);
    }

    #[test]
    fn cap_exchange_leaves_a_record_without_a_response_alone() {
        let mut record = response_record(String::new());
        record.response = None;
        record.error = Some("connection refused".into());
        assert_eq!(cap_exchange(record.clone()), record);
    }

    #[test]
    fn callback_exchange_shows_the_call_as_the_response_with_secrets_masked() {
        let call = crate::callback_listener::ReceivedCall {
            method: "POST".into(),
            path: "/cb/abc".into(),
            query: vec![("event".into(), "paid".into())],
            headers: vec![
                ("authorization".into(), "Bearer t".into()),
                ("x-note".into(), "sekret-token".into()),
            ],
            body: r#"{"k":"sekret-token"}"#.into(),
        };
        let record = callback_exchange(&call, 42, &secrets(&["sekret-token"]));
        assert_eq!(record.method, "POST");
        assert_eq!(record.url, "/cb/abc?event=paid");
        let response = record.response.expect("response");
        assert_eq!(response.status, 200);
        assert_eq!(response.status_text, "POST");
        assert_eq!(response.duration_ms, 42);
        assert_eq!(response.body, r#"{"k":"••••••"}"#);
        let value = |key: &str| {
            response
                .headers
                .iter()
                .find(|h| h.key == key)
                .map(|h| h.value.clone())
        };
        assert_eq!(value("authorization").as_deref(), Some("••••••"));
        assert_eq!(value("x-note").as_deref(), Some("••••••"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-shared flow_step_completed && cargo test -j4 -p rocket-app flow_debug`
Expected: FAIL to compile — `no field truncated`, `no field exchange`, `cannot find value EXCHANGE_BODY_LIMIT`, `cannot find function cap_exchange` / `callback_exchange`.

- [ ] **Step 3: Implement**

`crates/rocket-shared/src/events.rs`, in `FlowDebugResponse` after `body`:

```rust
    /// True when `body` was cut to the exchange size limit. `size_bytes`
    /// still holds the full size.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
```

In `DomainEvent::FlowStepCompleted`, after `attempts`:

```rust
        /// The request this step sent and its response, masked and
        /// size-capped. Set for every Request node that sent and for an
        /// accepted Wait for callback, whatever the Debug mode.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exchange: Option<Box<FlowDebugRequest>>,
```

`crates/rocket-app/src/flow_debug.rs`: add `truncated: false,` to the `FlowDebugResponse` literal in `build_debug_request`, then add:

```rust
/// The largest response body an exchange record keeps, in bytes.
pub(crate) const EXCHANGE_BODY_LIMIT: usize = 262_144;

/// Cuts the response body to `EXCHANGE_BODY_LIMIT` bytes at a UTF-8
/// boundary and marks the record as truncated.
pub(crate) fn cap_exchange(mut record: FlowDebugRequest) -> FlowDebugRequest {
    if let Some(response) = record.response.as_mut() {
        if response.body.len() > EXCHANGE_BODY_LIMIT {
            let mut cut = EXCHANGE_BODY_LIMIT;
            while !response.body.is_char_boundary(cut) {
                cut -= 1;
            }
            response.body.truncate(cut);
            response.truncated = true;
        }
    }
    record
}

/// The record of an accepted callback. The call itself is the response,
/// so a reader sees what arrived; the request side holds its method and
/// path.
pub(crate) fn callback_exchange(
    call: &crate::callback_listener::ReceivedCall,
    duration_ms: u64,
    secret_values: &HashSet<String>,
) -> FlowDebugRequest {
    let query: Vec<String> = call.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let url = if query.is_empty() {
        call.path.clone()
    } else {
        format!("{}?{}", call.path, query.join("&"))
    };
    let headers = call
        .headers
        .iter()
        .map(|(key, value)| FlowDebugHeader {
            key: key.clone(),
            value: if is_sensitive_header(key) {
                REDACTED.to_string()
            } else {
                redact_secrets(value, secret_values)
            },
        })
        .collect();
    cap_exchange(FlowDebugRequest {
        method: call.method.clone(),
        url: redact_url_secrets(&url, secret_values),
        headers: Vec::new(),
        body: None,
        response: Some(FlowDebugResponse {
            status: 200,
            status_text: call.method.clone(),
            duration_ms,
            size_bytes: call.body.len() as u64,
            headers,
            body: redact_secrets(&call.body, secret_values),
            truncated: false,
        }),
        error: None,
    })
}
```

Add `exchange: None,` to every `DomainEvent::FlowStepCompleted { .. }` literal the compiler reports (tests in `events.rs`, `step_completed_event` in `flow_execution_service.rs`, and any in `src-tauri`). Both functions are first used in Task 2; until then give them `#[cfg_attr(not(test), allow(dead_code))]` with the comment `// Used by the step recording in Task 2.`

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-shared && cargo test -j4 -p rocket-app flow_debug && cargo check -j4 -p rocket-app && cargo check -j4 -p rocket --tests`
Expected: PASS; no warnings.

- [ ] **Step 5: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): add capped exchange records to step events`.

---

### Task 2: Report `exchange` on every step that sent, and Input values

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` — `FlowStepResult` (`:570-596`), `step_completed_event` (`:600-615`), run loop (`:844-886`), `execute_node` signature (`:992-1005`) and Request arm (`:1112-1128`), `result_to_step` Value branch (`:1306-1316`), `skipped_step`/`failed_step`/`result_to_step` base literals (`exchange: None`), test `a_run_reports_the_output_nodes_captured_value_but_not_the_input_nodes` (`:2404-2435`), new tests next to `a_request_node_without_debug_has_no_debug_record` (`:5438`)
- Modify: `crates/rocket-app/src/flow_poll.rs:46-100` (`run_repeat_until` signature and record per attempt)
- Modify: `crates/rocket-app/src/flow_wait.rs:61-131` (`wait_for_callback` signature and accepted-call record)

**Interfaces:**
- Consumes: `cap_exchange`, `callback_exchange` (Task 1).
- Produces: `FlowStepResult.exchange: Option<FlowDebugRequest>` (camelCase `exchange` in the summary); `execute_node`, `run_repeat_until` and `wait_for_callback` gain an `exchange: &mut Option<FlowDebugRequest>` parameter, placed right after `debug`.

- [ ] **Step 1: Write the failing tests**

In the `flow_execution_service.rs` tests module, next to the Debug-mode tests, add:

```rust
    #[tokio::test]
    async fn a_request_without_debug_still_reports_a_masked_exchange() {
        let (summary, events) = run_debug_node(false, "https://x.test", 200).await;
        let step = step_of(&summary, "r");
        assert_eq!(step.debug_request, None, "Debug mode still owns debug_request");
        let exchange = step.exchange.as_ref().expect("exchange record");
        assert_eq!(exchange.url, "https://x.test/login");
        let key = exchange
            .headers
            .iter()
            .find(|h| h.key == "X-Key")
            .expect("X-Key header");
        assert_eq!(key.value, "••••••");
        assert_eq!(exchange.body.as_deref(), Some(r#"{"k":"••••••"}"#));
        assert_eq!(exchange.response.as_ref().map(|r| r.status), Some(200));
        let event_exchange = events.iter().find_map(|e| match e {
            DomainEvent::FlowStepCompleted {
                node_id, exchange, ..
            } if node_id == "r" => Some(exchange.clone()),
            _ => None,
        });
        assert_eq!(event_exchange.flatten().map(|b| *b), step.exchange);
    }

    #[tokio::test]
    async fn a_request_that_fails_to_send_reports_its_exchange_with_the_error() {
        let (summary, _) = run_debug_node(false, "https://x.test", 0).await;
        let exchange = step_of(&summary, "r").exchange.clone().expect("exchange");
        assert!(exchange.response.is_none());
        assert!(
            exchange
                .error
                .as_deref()
                .is_some_and(|e| e.contains("connection refused")),
            "got {:?}",
            exchange.error
        );
    }

    #[tokio::test]
    async fn a_polled_request_reports_the_last_attempts_exchange() {
        let executor = SequenceExecutor::new(vec![(404, "{}"), (404, "{}"), (200, r#"{"ok":1}"#)]);
        let history = InMemoryHistoryRepo::new();
        let exec = poll_exec(&executor, status_condition(200), &history);
        let flow = poll_flow(repeat("response.status === 200", 100, 5, 10_000), false);

        let summary = service_with_flow(flow)
            .run(&exec, run_input("poll"))
            .await
            .expect("run");

        let step = step_of(&summary, "job");
        assert_eq!(step.debug_request, None);
        let response = step
            .exchange
            .clone()
            .and_then(|e| e.response)
            .expect("exchange response");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, r#"{"ok":1}"#);
    }

    #[tokio::test]
    async fn an_accepted_callback_reports_the_call_as_its_exchange() {
        let fake = crate::test_doubles::FakeCallbackListener::new();
        fake.queue_on_open(event_call("payment.completed"));
        let executor = RecordingExecutor::new();
        let exec = recording_exec(&executor, fixed_wire("x"));

        let summary =
            service_with_listener(register_then_wait(wait_node_with("w", 60_000, None)), &fake)
                .run(&exec, run_input("cb"))
                .await
                .expect("run");

        let exchange = step_of(&summary, "w").exchange.clone().expect("exchange");
        assert_eq!(exchange.method, "POST");
        assert_eq!(exchange.url, "/cb/0");
        let response = exchange.response.expect("response");
        assert!(response.body.contains("payment.completed"));
    }

    #[tokio::test]
    async fn routing_and_output_steps_have_no_exchange() {
        let service = service_with_flow(linear_flow());
        let exec = service_with_engine(
            FakeCollectionRepo::new(),
            Box::new(FixedJsonqEngine {
                value: serde_json::json!("bob"),
            }),
        );
        let summary = service.run(&exec, run_input("auth-flow")).await.expect("run");
        for step in &summary.steps {
            assert!(step.exchange.is_none(), "{} has an exchange", step.node_id);
        }
    }
```

Change the existing test `a_run_reports_the_output_nodes_captured_value_but_not_the_input_nodes` (`:2404`): rename it to `a_run_reports_the_captured_value_of_output_and_input_nodes`, and replace its last assertion with:

```rust
        assert_eq!(
            step_for("a").value.as_deref(),
            Some("bob"),
            "an Input node reports its resolved value"
        );
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-app exchange && cargo test -j4 -p rocket-app a_run_reports_the_captured_value`
Expected: FAIL to compile (`no field exchange on FlowStepResult`), then after adding the field, assertion failures (`exchange record` missing, Input value `None`).

- [ ] **Step 3: Implement**

`FlowStepResult`, after `attempts`:

```rust
    /// The request this step sent and its response, masked and size-capped.
    /// Set for every Request node that sent and for an accepted Wait for
    /// callback, whatever the Debug mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<FlowDebugRequest>,
```

Add `exchange: None,` to the base literals in `result_to_step`, `skipped_step` and `failed_step`, and to any test literal the compiler reports. In `step_completed_event`, replace `exchange: None` with `exchange: step.exchange.clone().map(Box::new),`.

`execute_node`: add the parameter `exchange: &mut Option<FlowDebugRequest>,` after `debug`. In the Request arm, replace the Debug block (`:1116-1127`) with:

```rust
                if let Some(sent) = &sent {
                    let error = result.as_ref().err().map(|e| e.to_string());
                    let record = build_debug_request(
                        sent,
                        result.as_ref().ok().map(|o| &o.response),
                        error.as_deref(),
                        &secret_values,
                    );
                    if *debug_on {
                        *debug = Some(record.clone());
                    }
                    *exchange = Some(cap_exchange(record));
                }
```

Pass `exchange` to `run_repeat_until` (after `debug`) and to `wait_for_callback` (after `logs`). Import `cap_exchange` next to `build_debug_request`.

`flow_poll.rs` `run_repeat_until`: add `exchange: &mut Option<FlowDebugRequest>,` after `debug`, and replace its Debug block (`:92-101`) with the same always-build pattern (`secret_values` there is already a reference, so pass it as is). Each attempt overwrites `*exchange`, so the step keeps the last one.

`flow_wait.rs` `wait_for_callback`: add `exchange: &mut Option<FlowDebugRequest>,` after `logs`, and before returning the accepted call:

```rust
                    *exchange = Some(crate::flow_debug::callback_exchange(
                        &call,
                        duration_ms,
                        secret_values,
                    ));
```

Remove the Task 1 `cfg_attr(not(test), allow(dead_code))` attributes from `cap_exchange` and `callback_exchange`.

Run loop (`:844-886`): add `let mut node_exchange = None;` next to `node_debug`, pass `&mut node_exchange` after `&mut node_debug`, and extend the step update:

```rust
                    let step = FlowStepResult {
                        logs: node_logs,
                        debug_request: node_debug,
                        exchange: node_exchange,
                        ..step
                    };
```

`result_to_step`, Value branch: report Input values too:

```rust
            let reports_value = matches!(
                kind,
                Some(FlowNodeKind::Output { .. }) | Some(FlowNodeKind::Input { .. })
            );
            FlowStepResult {
                value: reports_value.then(|| v.data().to_string()),
                ..base
            }
```

Update the doc comments of `FlowStepResult.value` and `FlowStepCompleted.value` to say "Output and Input nodes" instead of "Output nodes".

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-app flow_ && cargo test -j4 -p rocket-app debug && cargo check -j4 -p rocket-app && cargo check -j4 -p rocket --tests && cargo clippy -j4 -p rocket-app --all-targets`
Expected: PASS; no warnings from the touched files.

- [ ] **Step 5: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): report each request's exchange on every run`.

---

### Task 3: Frontend types, stored detail and the sensitive-header helper

**Files:**
- Modify: `src/lib/tauri-api.ts:1840-1856` (`FlowStepResult`), `:1865-1872` (`FlowDebugResponse`), `:1938-1955` (`FlowStepCompletedEvent`)
- Modify: `src/types/pane-types.ts:156-169` (`FlowNodeDetail`)
- Modify: `src/components/flow/FlowToolbar.tsx:41-65` (`detailFromEvent`, `detailFromStep`)
- Create: `src/lib/sensitive-headers.ts`, `src/lib/__tests__/sensitive-headers.test.ts`
- Test: `src/components/flow/__tests__/FlowToolbar.test.tsx` (next to `forwards attempts from the step event and the summary`, `:227`)

**Interfaces:**
- Produces: `FlowDebugResponse.truncated?: boolean`; `FlowStepResult.exchange?: FlowDebugRequest`; `FlowStepCompletedEvent.exchange?: FlowDebugRequest`; `FlowNodeDetail.exchange?: FlowDebugRequest`, `FlowNodeDetail.logs?: FlowLogEntry[]`; `isSensitiveHeader(name: string): boolean`; `REDACTED_VALUE = '••••••'`.

- [ ] **Step 1: Write the failing tests**

`src/lib/__tests__/sensitive-headers.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { isSensitiveHeader, REDACTED_VALUE } from '../sensitive-headers';

describe('isSensitiveHeader', () => {
  it('matches the backend list whatever the case', () => {
    for (const name of ['Authorization', 'proxy-authorization', 'COOKIE', 'Set-Cookie', 'X-Api-Key']) {
      expect(isSensitiveHeader(name)).toBe(true);
    }
  });

  it('leaves other headers alone', () => {
    expect(isSensitiveHeader('Content-Type')).toBe(false);
    expect(isSensitiveHeader('X-Api-Keys')).toBe(false);
  });

  it('uses the backend redaction marker', () => {
    expect(REDACTED_VALUE).toBe('••••••');
  });
});
```

In `FlowToolbar.test.tsx`, add after the attempts test:

```tsx
  it('stores the exchange and logs from the step event and the summary', async () => {
    const exchange = {
      method: 'GET',
      url: 'https://x.test',
      headers: [],
      response: {
        status: 200,
        statusText: 'OK',
        durationMs: 5,
        sizeBytes: 2,
        headers: [],
        body: '{}',
      },
    };
    const logs = [{ level: 'log' as const, message: 'hi' }];
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'r',
      status: 'success',
      status_code: 200,
      duration_ms: 5,
      error: null,
      value: null,
      logs,
      exchange,
    });
    expect(onPatchStatus).toHaveBeenCalledWith(
      'r',
      'success',
      expect.objectContaining({ exchange, logs }),
    );

    resolveRun({
      runId: 'run-123',
      steps: [
        {
          nodeId: 'r',
          status: 'success',
          statusCode: 200,
          durationMs: 5,
          error: null,
          value: null,
          logs,
          exchange,
        },
      ],
      stoppedReason: 'completed',
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenLastCalledWith(
        'r',
        'success',
        expect.objectContaining({ exchange, logs }),
      ),
    );
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `yarn test src/lib/__tests__/sensitive-headers.test.ts src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: FAIL — `Cannot find module '../sensitive-headers'`, and the toolbar test fails because the detail has no `exchange`/`logs`.

- [ ] **Step 3: Implement**

`src/lib/sensitive-headers.ts`:

```ts
// Mirrors is_sensitive_header and REDACTED in crates/rocket-app/src/redaction.rs.
// Keep both lists in step.
const SENSITIVE = new Set(['authorization', 'proxy-authorization', 'cookie', 'set-cookie', 'x-api-key']);

/** The marker the backend shows in place of a masked value. */
export const REDACTED_VALUE = '••••••';

/** True for a header whose value must never be shown. */
export function isSensitiveHeader(name: string): boolean {
  return SENSITIVE.has(name.toLowerCase());
}
```

`src/lib/tauri-api.ts`: add `/** True when the body was cut at 256 KB. */ truncated?: boolean;` to `FlowDebugResponse`; add `/** Masked request and response of a step that sent. */ exchange?: FlowDebugRequest;` to `FlowStepResult` and to `FlowStepCompletedEvent` (key `exchange`).

`src/types/pane-types.ts`, in `FlowNodeDetail`:

```ts
  /** Masked request and response of the last run, for Request and Wait nodes. */
  exchange?: import('@/lib/tauri-api').FlowDebugRequest;
  /** Script console output of the last run. */
  logs?: import('@/lib/tauri-api').FlowLogEntry[];
```

`FlowToolbar.tsx`: add to `detailFromEvent` — `exchange: event.exchange ?? undefined, logs: event.logs?.length ? event.logs : undefined,` — and to `detailFromStep` — `exchange: step.exchange ?? undefined, logs: step.logs?.length ? step.logs : undefined,`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `yarn test src/components/flow src/lib src/stores && yarn tsc --noEmit && yarn check`
Expected: PASS; tsc and Biome clean.

- [ ] **Step 5: Commit**

Commit with the dev-workflow-skills:1-git-commit skill. Suggested message: `feat(flow): keep each step's exchange and logs`.
