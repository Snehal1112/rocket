# Flow Request Debug Mode — Plan 01: Backend

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Flow Request node with `debug: true` reports the request as sent and its response, secret-masked, on its step result and step event.

**Architecture:** `FlowNodeKind::Request` gains a persisted `debug` flag. A new `redaction` module holds value-based and header-name masking. `RequestExecutionService` gains a capturing variant of the send path that records the request after the pre-request phase. A pure `build_debug_request` turns the captured request and response into a masked `FlowDebugRequest`, which the Flow run attaches to the step through an out-parameter so failures keep it.

**Tech Stack:** Rust (rocket-flow, rocket-app, rocket-shared), serde, tokio tests.

**Spec:** `docs/superpowers/specs/2026-09-29-flow-request-debug-mode-design.md`

## Global Constraints
- 📖 Before starting ANY task, read `docs/superpowers/specs/opencollection-spec-reference.md` (auth, variable scope, secrets) and `crates/rocket-app/CLAUDE.md`.
- Old flow YAML must load unchanged; a node without debug serializes exactly as before (`#[serde(default, skip_serializing_if = "is_false")]`).
- No camelCase rename on persistence structs (`FlowNodeKind`); `FlowDebugRequest` is an IPC/event DTO and uses `#[serde(rename_all = "camelCase")]`.
- Masking marker is exactly `••••••`; value masking skips secrets shorter than `MIN_REDACTION_LEN` (6) and replaces longest values first.
- Sensitive header names (case-insensitive): `authorization`, `proxy-authorization`, `cookie`, `set-cookie`, `x-api-key`.
- The auth value itself is never shown.
- `execute_with_external_secrets` behaviour and signature stay unchanged for every existing caller.
- No unwrap calls in production code. Code comments: short full sentences ending with a period.
- Cargo: always `-j4`; never `cargo test --workspace`. Targeted: `cargo test -j4 -p <crate> <filter>`, then `cargo clippy -j4 -p <crates> --tests -- -D warnings` and `cargo check -j4 -p rocket-app -p rocket`.
- Commits: use the `dev-workflow-skills:1-git-commit` skill (skip its subagent steps if you cannot dispatch subagents); conventional commits.

## Review Focus
1. A secret that is a prefix of another secret (`abc123` and `abc123xyz`) — both must be fully masked, no tail left visible.
2. A secret used in the URL query, a header and a JSON body at once — masked in all three.
3. A send that fails with no response (bad host) — the debug record still has the request and the error text.
4. A pre-request script that changes a header — the debug record shows the changed value (what was sent), not the template.
5. `debug: false` or absent — no debug record, and existing wire-shape JSON tests stay byte-identical.

---

### Task 1: Debug flag and redaction helpers

**Files:**
- Modify: `crates/rocket-flow/src/node.rs` (`FlowNodeKind::Request` ~18-21), plus any exact `Request { label, source }` patterns or constructors across crates (find with `grep -rn "FlowNodeKind::Request" crates src-tauri`; e.g. `crates/rocket-app/src/flow_execution_service.rs` ~253)
- Modify: `crates/rocket-flow/src/flow.rs` (round-trip tests ~301)
- Create: `crates/rocket-app/src/redaction.rs`; register `mod redaction;` in `crates/rocket-app/src/lib.rs`
- Modify: `crates/rocket-app/src/execution_service.rs` (`redact_secrets_in_url` ~1755 and its caller ~1424)

**Interfaces:**
- Produces:
  - `FlowNodeKind::Request { label: String, source: RequestSource, debug: bool }` with `#[serde(default, skip_serializing_if = "is_false")]` on `debug`, and `fn is_false(b: &bool) -> bool` in node.rs.
  - In `crate::redaction`: `pub(crate) const REDACTED: &str = "••••••";`, `pub(crate) fn redact_secrets(text: &str, secret_values: &HashSet<String>) -> String`, `pub(crate) fn is_sensitive_header(name: &str) -> bool`.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-flow/src/flow.rs` tests (next to the existing Phase 1 round-trip test):
```rust
    #[test]
    fn a_request_node_without_debug_round_trips_without_a_debug_key() {
        let yaml = "nodes:\n- id: n1\n  kind:\n    kind: Request\n    label: Login\n    source:\n      type: Saved\n      request_path: auth/login.yml\n  position:\n    x: 0.0\n    y: 0.0\nedges: []\n";
        let flow: Flow = serde_yaml::from_str(yaml).expect("old yaml loads");
        let FlowNodeKind::Request { debug, .. } = &flow.nodes[0].kind else { panic!("request") };
        assert!(!debug);
        let out = serde_yaml::to_string(&flow).expect("serialize");
        assert!(!out.contains("debug"), "got {out}");
    }

    #[test]
    fn a_debug_request_node_round_trips() {
        let yaml = "nodes:\n- id: n1\n  kind:\n    kind: Request\n    label: Login\n    source:\n      type: Saved\n      request_path: auth/login.yml\n    debug: true\n  position:\n    x: 0.0\n    y: 0.0\nedges: []\n";
        let flow: Flow = serde_yaml::from_str(yaml).expect("loads");
        let out = serde_yaml::to_string(&flow).expect("serialize");
        assert!(out.contains("debug: true"), "got {out}");
    }
```
Adapt the YAML to the exact `Flow`/`FlowNode` field names used by the existing round-trip test in that file (copy its fixture shape; only add `debug: true`). Test code may use `panic!`/`expect`.

In the new `crates/rocket-app/src/redaction.rs`, a `#[cfg(test)] mod tests`:
```rust
    use std::collections::HashSet;
    use super::*;

    fn set(values: &[&str]) -> HashSet<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn masks_every_occurrence_and_the_longest_secret_first() {
        let out = redact_secrets("a=abc123xyz&b=abc123", &set(&["abc123", "abc123xyz"]));
        assert_eq!(out, "a=••••••&b=••••••");
    }

    #[test]
    fn leaves_secrets_shorter_than_the_floor() {
        assert_eq!(redact_secrets("pin=1234", &set(&["1234"])), "pin=1234");
    }

    #[test]
    fn empty_set_returns_the_text_unchanged() {
        assert_eq!(redact_secrets("hello", &HashSet::new()), "hello");
    }

    #[test]
    fn sensitive_header_names_match_case_insensitively() {
        for name in ["Authorization", "proxy-authorization", "COOKIE", "Set-Cookie", "x-api-key"] {
            assert!(is_sensitive_header(name), "{name}");
        }
        assert!(!is_sensitive_header("Content-Type"));
    }
```

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test -j4 -p rocket-flow debug` and `cargo test -j4 -p rocket-app redaction`
Expected: compile errors (no `debug` field, no `redaction` module).

- [ ] **Step 3: Implement**

`node.rs`:
```rust
    Request {
        label: String,
        source: RequestSource,
        /// When true, a run reports the request as sent and its response.
        #[serde(default, skip_serializing_if = "is_false")]
        debug: bool,
    },
```
and
```rust
// Keeps `debug: false` out of saved files, so old flows round-trip unchanged.
fn is_false(b: &bool) -> bool {
    !*b
}
```
Fix every place that builds or exhaustively destructures `Request { label, source }`: builders add `debug: false`; patterns add `..`.

`redaction.rs`:
```rust
//! Masks secret values and sensitive headers in text shown to the user.

use std::collections::HashSet;

/// The text that replaces a masked value.
pub(crate) const REDACTED: &str = "••••••";

/// Secrets shorter than this are left alone, matching the script engine.
pub(crate) const MIN_REDACTION_LEN: usize = 6;

/// Replaces every secret value in `text` with `REDACTED`.
///
/// Longer secrets are replaced first, so a secret that is a prefix of
/// another cannot leave part of the longer one visible.
pub(crate) fn redact_secrets(text: &str, secret_values: &HashSet<String>) -> String {
    let mut secrets: Vec<&String> = secret_values
        .iter()
        .filter(|s| s.len() >= MIN_REDACTION_LEN)
        .collect();
    secrets.sort_by(|a, b| b.len().cmp(&a.len()));
    let mut out = text.to_string();
    for secret in secrets {
        out = out.replace(secret.as_str(), REDACTED);
    }
    out
}

/// Whether a header's value is always masked, whatever it contains.
pub(crate) fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie" | "x-api-key"
    )
}
```
In `execution_service.rs`, make the history URL use `crate::redaction::redact_secrets` and delete `redact_secrets_in_url`. Keep the existing `MIN_REDACTION_LEN` constant there if other code in the file uses it (make it the same value, or import `crate::redaction::MIN_REDACTION_LEN` instead of duplicating it). Existing redaction tests for the history URL must still pass.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test -j4 -p rocket-flow`, `cargo test -j4 -p rocket-app redaction`, `cargo test -j4 -p rocket-app execution_service`, `cargo test -j4 -p rocket-app flow_execution_service`
Expected: all pass.

- [ ] **Step 5: Check and commit**

Run: `cargo clippy -j4 -p rocket-flow -p rocket-app --tests -- -D warnings` and `cargo check -j4 -p rocket-app -p rocket`.
Commit subject: `feat(flow): add request debug flag and redaction helpers`.

---

### Task 2: Capture the sent request and build the debug record

**Files:**
- Modify: `crates/rocket-shared/src/events.rs` (new DTOs next to `FlowLogEntry`)
- Modify: `crates/rocket-app/src/execution_service.rs` (`execute_with_external_secrets` ~1479-1496)
- Create: `crates/rocket-app/src/flow_debug.rs`; register `mod flow_debug;` in `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `crate::redaction::{redact_secrets, is_sensitive_header, REDACTED}` from Task 1.
- Produces:
  - In `rocket_shared::events` (derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`, `#[serde(rename_all = "camelCase")]`):
    ```rust
    pub struct FlowDebugHeader { pub key: String, pub value: String }
    pub struct FlowDebugResponse {
        pub status: u16,
        pub status_text: String,
        pub duration_ms: u64,
        pub size_bytes: u64,
        pub headers: Vec<FlowDebugHeader>,
        pub body: String,
    }
    pub struct FlowDebugRequest {
        pub method: String,
        pub url: String,
        pub headers: Vec<FlowDebugHeader>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub body: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub response: Option<FlowDebugResponse>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub error: Option<String>,
    }
    ```
  - `RequestExecutionService::execute_capturing(&self, input: ExecuteRequestInput, external_secrets: &HashMap<String, String>, sent: &mut Option<HttpRequest>) -> DomainResult<ExecuteRequestOutput>` (`pub(crate)`).
  - `pub(crate) fn build_debug_request(sent: &HttpRequest, response: Option<&HttpResponse>, error: Option<&str>, secret_values: &HashSet<String>) -> FlowDebugRequest` in `crate::flow_debug`.

- [ ] **Step 1: Write the failing tests**

In `execution_service.rs` tests, using the file's existing mock executor, env repo and script-engine test helpers (find a test that runs `execute` with a pre-request script that sets a header and copy its setup):
```rust
    #[tokio::test]
    async fn execute_capturing_records_the_request_after_the_pre_request_script() {
        // Setup: a pre-request script that sets header `X-Trace` to `from-script`.
        // Run `execute_capturing(input, &HashMap::new(), &mut sent)`.
        // Assert: `sent` is Some, its URL is the resolved URL, and its headers contain X-Trace: from-script.
    }

    #[tokio::test]
    async fn execute_capturing_keeps_the_request_when_the_send_fails() {
        // Setup: a mock executor whose `execute` returns Err(DomainError::Internal("connection refused".into())).
        // Assert: the call returns Err, and `sent` is Some with the resolved URL.
    }
```
Write both tests fully with the file's real helpers; the comments above describe the required setup and assertions.

In `flow_debug.rs` tests (pure function, no engine):
```rust
    fn secrets(values: &[&str]) -> HashSet<String> { values.iter().map(|v| v.to_string()).collect() }

    fn request(auth: Auth) -> HttpRequest {
        let mut req = HttpRequest::new(HttpMethod::Post, "https://api.example.com/login?t=sekret-token");
        req.query_params = vec![QueryParam { key: "page".into(), value: "2".into(), enabled: true, description: None },
                                QueryParam { key: "off".into(), value: "x".into(), enabled: false, description: None }];
        req.headers = vec![Header { key: "X-Custom".into(), value: "sekret-token".into(), enabled: true, description: None },
                           Header { key: "Cookie".into(), value: "sid=1".into(), enabled: true, description: None },
                           Header { key: "X-Off".into(), value: "no".into(), enabled: false, description: None }];
        req.body = Some(Body { mode: BodyMode::Json, content: Some(r#"{"p":"sekret-token"}"#.into()), form_data: None, file_path: None });
        req.auth = auth;
        req
    }

    #[test]
    fn masks_secrets_everywhere_and_appends_enabled_query_params() {
        let d = build_debug_request(&request(Auth::None), None, None, &secrets(&["sekret-token"]));
        assert_eq!(d.method, "POST");
        assert!(d.url.contains("page=2") && !d.url.contains("off=x"), "{}", d.url);
        assert!(!d.url.contains("sekret-token"));
        assert_eq!(d.headers.iter().find(|h| h.key == "X-Custom").map(|h| h.value.as_str()), Some("••••••"));
        assert_eq!(d.headers.iter().find(|h| h.key == "Cookie").map(|h| h.value.as_str()), Some("••••••"));
        assert!(d.headers.iter().all(|h| h.key != "X-Off"));
        assert_eq!(d.body.as_deref(), Some(r#"{"p":"••••••"}"#));
    }

    #[test]
    fn shows_auth_as_a_masked_line_never_its_value() {
        let d = build_debug_request(&request(Auth::Bearer { token: "tok-123456".into() }), None, None, &HashSet::new());
        let auth = d.headers.iter().find(|h| h.key == "Authorization").expect("auth line");
        assert_eq!(auth.value, "Bearer ••••••");
        assert!(d.headers.iter().all(|h| !h.value.contains("tok-123456")));
    }

    #[test]
    fn records_the_response_and_masks_its_sensitive_headers() {
        let resp = HttpResponse { status: 400, status_text: "Bad Request".into(),
            headers: vec![Header { key: "Set-Cookie".into(), value: "sid=2".into(), enabled: true, description: None }],
            body: r#"{"error":"sekret-token bad"}"#.into(), duration_ms: 12, ttfb_ms: 5, size_bytes: 30 };
        let d = build_debug_request(&request(Auth::None), Some(&resp), None, &secrets(&["sekret-token"]));
        let r = d.response.expect("response");
        assert_eq!(r.status, 400);
        assert_eq!(r.headers[0].value, "••••••");
        assert!(!r.body.contains("sekret-token"));
    }

    #[test]
    fn records_the_error_when_there_is_no_response() {
        let d = build_debug_request(&request(Auth::None), None, Some("connection refused"), &HashSet::new());
        assert!(d.response.is_none());
        assert_eq!(d.error.as_deref(), Some("connection refused"));
    }
```
Adjust constructors to the real type definitions (e.g. `HttpRequest::new` signature, `HttpResponse` fields, `QueryParam`/`Header` fields); keep the assertions.

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test -j4 -p rocket-app flow_debug` and `cargo test -j4 -p rocket-app execute_capturing`
Expected: compile errors (missing function, module and types).

- [ ] **Step 3: Implement**

`execution_service.rs`: move the body of `execute_with_external_secrets` into `execute_capturing`, adding `*sent = Some(state.http_request.clone());` right after `run_before_request_phase(...).await?;` and before `send_request`. `execute_with_external_secrets` becomes:
```rust
        let mut sent = None;
        self.execute_capturing(input, external_secrets, &mut sent).await
```

`flow_debug.rs` — `build_debug_request`:
- `method`: the method's upper-case name (use `HttpMethod`'s existing string form, e.g. `Display` or `as_str`; check rocket-shared).
- `url`: parse `sent.url` with the `url` crate and append enabled query params with `query_pairs_mut().append_pair` only when there are any (as `reqwest_executor.rs:241-251` does); if parsing fails, use the raw URL. Then `redact_secrets`.
- `headers`: enabled headers only; value = `REDACTED` when `is_sensitive_header(key)`, else `redact_secrets(value)`. Then append one auth line from `sent.auth`:
  - `Basic` → `Authorization: Basic ••••••`; `Bearer` → `Authorization: Bearer ••••••`;
  - `ApiKey { key, placement, .. }` → header placement: `<key>: ••••••`; query placement: `Auth: API key in query "<key>"` (the value is never shown, and it is not appended to the URL);
  - `None`/`Inherit` → no line; every other variant → `Auth: <type>` using a short type name (reuse `sensitive_auth_label` in `execution_service.rs` if it can be made `pub(crate)`, else a local match).
- `body`: text modes → `redact_secrets(content)`; form modes → enabled entries as `key=value` lines, values through `redact_secrets`; no body or no content → `None`.
- `response`: map status, status_text, duration_ms, size_bytes (as u64), headers (same masking as request headers) and `redact_secrets(body)`.
- `error`: `error.map(|e| redact_secrets(e, secret_values))`.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test -j4 -p rocket-app flow_debug`, `cargo test -j4 -p rocket-app execution_service`
Expected: all pass.

- [ ] **Step 5: Check and commit**

Run: `cargo clippy -j4 -p rocket-app -p rocket-shared --tests -- -D warnings` and `cargo check -j4 -p rocket-app -p rocket`.
Commit subject: `feat(flow): build masked debug records of sent requests`.

---

### Task 3: Attach the debug record to Flow steps

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs` (Request arm of `execute_node` ~819-852, run loop where `node_logs` is created ~655-680, `FlowStepResult` ~467-488, `step_completed_event` ~492-504, `result_to_step`/`skipped_step`/`failed_step`)
- Modify: `crates/rocket-shared/src/events.rs` (`DomainEvent::FlowStepCompleted` ~201-219 and its tests)

**Interfaces:**
- Consumes: `FlowDebugRequest` (rocket-shared), `execute_capturing`, `build_debug_request` from Task 2; `FlowNodeKind::Request { debug, .. }` from Task 1.
- Produces:
  - `FlowStepResult.debug_request: Option<FlowDebugRequest>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`; JSON key `debugRequest` via the struct's camelCase rename).
  - `DomainEvent::FlowStepCompleted.debug_request: Option<FlowDebugRequest>` (same attribute; JSON key `debug_request`).
  - `execute_node` gains a last parameter `debug: &mut Option<FlowDebugRequest>`.

- [ ] **Step 1: Write the failing tests**

In `events.rs` tests: add `debug_request: None` to every `FlowStepCompleted { .. }` literal (expected JSON strings stay unchanged), and add:
```rust
    #[test]
    fn flow_step_completed_carries_a_debug_request_and_omits_it_when_absent() {
        let debug = FlowDebugRequest {
            method: "GET".into(),
            url: "https://x.test/a".into(),
            headers: vec![],
            body: None,
            response: None,
            error: Some("boom".into()),
        };
        // Build one event with `debug_request: Some(debug)` (other fields like the existing tests)
        // and assert the JSON contains `"debug_request":{"method":"GET","url":"https://x.test/a","headers":[],"error":"boom"}`.
        // Build one with `debug_request: None` and assert the JSON has no "debug_request" key.
    }
```
Write the two events fully, copying an existing test's other fields.

In `flow_execution_service.rs` tests, with the file's existing mock executor and run helpers (find a run test that executes a saved Request node against a mock executor):
1. A flow with one Request node `debug: true` whose request has URL `{{base}}/login`, header `X-Key: {{secret}}` and JSON body `{"k":"{{secret}}"}`, where the environment defines `base` and a SECRET variable `secret` = `sekret-value`. Run it. Assert the node's step has `debug_request` with the resolved URL, `X-Key: ••••••`, body `{"k":"••••••"}`, and `response.status` equal to the mock's status; assert the published `FlowStepCompleted` for that node carries the same `debug_request`.
2. The same node with `debug: false` → `debug_request` is `None` on the step and the event.
3. A debug node whose executor fails (mock returns Err) → the step is Failed, and `debug_request` is Some with the URL and `error` containing the failure message.

- [ ] **Step 2: Run and watch them fail**

Run: `cargo test -j4 -p rocket-shared flow_step_completed` and `cargo test -j4 -p rocket-app flow_execution_service`
Expected: compile errors for the missing field/parameter.

- [ ] **Step 3: Implement**

- Add the `debug_request` field (doc: `/// The request as sent and its response, masked. Only for Request nodes in debug mode.`) to `FlowStepCompleted` and `FlowStepResult`; set `None` in `result_to_step`'s base, `skipped_step` and `failed_step`; copy it in `step_completed_event`.
- Give `execute_node` the new last parameter `debug: &mut Option<FlowDebugRequest>`. In the Request arm, match `FlowNodeKind::Request { debug: debug_on, .. }` (rename to avoid clashing with the parameter), then:
```rust
                let mut sent = None;
                let result = exec
                    .execute_capturing(request_input, external_secrets, &mut sent)
                    .await;
                if *debug_on {
                    if let Some(sent) = &sent {
                        let error = result.as_ref().err().map(|e| e.to_string());
                        *debug = Some(build_debug_request(
                            sent,
                            result.as_ref().ok().map(|o| &o.response),
                            error.as_deref(),
                            &secret_values,
                        ));
                    }
                }
                let output = result?;
```
  keeping the existing log handling around it (request script logs are still added after a successful send). `secret_values` is the set the arm already builds for script logs.
- In the run loop, next to `let mut node_logs = Vec::new();`, add `let mut node_debug = None;`, pass `&mut node_debug`, and after building the step: `let step = FlowStepResult { logs: node_logs, debug_request: node_debug, ..step };`.

- [ ] **Step 4: Run and watch them pass**

Run: `cargo test -j4 -p rocket-shared`, `cargo test -j4 -p rocket-app flow_execution_service`, `cargo test -j4 -p rocket-app execution_service`
Expected: all pass; existing exact-JSON event tests unchanged.

- [ ] **Step 5: Check and commit**

Run: `cargo clippy -j4 -p rocket-app -p rocket-shared --tests -- -D warnings` and `cargo check -j4 -p rocket-app -p rocket`.
Commit subject: `feat(flow): report the sent request for debug nodes`.
