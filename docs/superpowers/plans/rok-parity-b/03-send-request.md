# rok parity B, plan 03: `rok.sendRequest` end to end

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish `rok.sendRequest` as in Bruno: the callback form, the `httpsAgent` error, Console entries and redaction in the engine, and a real host in `rocket-app` that sends through `HttpExecutor` with the calling request's TLS, redirect and client-certificate settings.

**Architecture:** The engine op adds a redacted Console line per call and masks secrets in rejection messages. `rocket-app` gets `ExecutionScriptHost<'a>`, which borrows the service and carries the calling request's resolved `RequestOptions`. `run_script_phase` passes it to `ScriptEngine::execute_with_host` in all three request phases, so single sends, Collection Runner steps and Flow request nodes all get it.

**Tech Stack:** Rust, `deno_core`, `async-trait`, Tokio, `wiremock` 0.6, `ReqwestExecutor`, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`, section 3. Index with rulings: `00-plan-index.md` (rulings 3, 7, 8 and 12 apply here). Requires plans 01 and 02.

## Global Constraints

- `cargo` commands always pass `-j4`. No `cargo test --workspace`.
- Options: `method`, `url`, `headers`, `data` (objects serialized as JSON), `timeout` (default 30 s). Works with `await` or a callback `(err, res)`.
- Resolves to `{ status, statusText, headers, data, responseTime }`, `data` JSON-parsed when possible. 4xx and 5xx resolve (ruling 7).
- No variable interpolation of the options, as in Bruno.
- `httpsAgent` rejects with `rok.sendRequest: httpsAgent is not supported`.
- Each call adds a Console entry. Secret values are masked in Console lines and in rejection messages. Network errors reject with a plain `Error`.
- `src-tauri` does not change (ruling 3).
- Comments are short full sentences ending in a period.

## Review Focus

- A secret that appears in the request URL and therefore in the network error text: it is masked in `e.message` and in the Console line.
- A server that echoes a secret back and the script logs `res.data`: the Console shows the mask, not the value.
- A server slower than the script's `timeout`: the promise rejects with `timed out after N ms` close to N, not after the 30 s default.
- A request whose environment turns TLS verification off: the side request also skips verification (same options).
- A 404 from the side request: the script gets `status: 404` and keeps running.

---

### Task 1: Engine-side completeness

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/host.rs` (`op_rok_send_request`)
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js` (`_sendOptions`, `sendRequest`)
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (tests)
- Modify: `src/components/editor/rok-types.ts`

**Interfaces:**
- Consumes: plan 01 Task 2 (`op_rok_send_request`, `send_host_call`, `host_error`, `FakeHost` in the engine tests).
- Produces (JS): `rok.sendRequest(options, callback?)`. With a callback the returned promise resolves to the callback's return value. Console lines read `rok.sendRequest <METHOD> <url> -> <status> (<ms> ms)` and `rok.sendRequest <METHOD> <url> failed: <message>`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `engine.rs`, after the `host_` tests:

```rust
    #[tokio::test]
    async fn send_request_callback_gets_null_and_the_response() {
        let host = FakeHost::ok(201, "{}");
        let ctx = minimal_ctx(
            "await rok.sendRequest({ url: 'https://x.test' }, (err, res) => { \
               rok.setVar('cb', String(err) + '|' + res.status); })",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(result.runtime_vars.get("cb").expect("cb present"), "null|201");
    }

    #[tokio::test]
    async fn send_request_callback_gets_the_error_and_null() {
        let host = FakeHost::failing("rok.sendRequest: refused");
        let ctx = minimal_ctx(
            "await rok.sendRequest({ url: 'https://x.test' }, (err, res) => { \
               rok.setVar('cb', err.message + '|' + String(res)); })",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(
            result.runtime_vars.get("cb").expect("cb present"),
            "rok.sendRequest: refused|null"
        );
    }

    #[tokio::test]
    async fn send_request_unawaited_callback_still_runs() {
        let host = FakeHost::ok(201, "{}");
        let ctx = minimal_ctx(
            "rok.sendRequest({ url: 'https://x.test' }, (err, res) => rok.setVar('late', res.status));",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(result.runtime_vars.get("late").expect("late present"), 201);
    }

    #[tokio::test]
    async fn send_request_rejects_an_https_agent() {
        let host = FakeHost::ok(200, "{}");
        let ctx = minimal_ctx(
            "try { await rok.sendRequest({ url: 'https://x.test', httpsAgent: {} }); } \
             catch (e) { rok.setVar('e', e.message); }",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(
            result.runtime_vars.get("e").expect("e present"),
            "rok.sendRequest: httpsAgent is not supported"
        );
        assert!(host.sent().is_empty());
    }

    #[tokio::test]
    async fn send_request_adds_a_console_entry() {
        let host = FakeHost::ok(201, "{}");
        let ctx = minimal_ctx("await rok.sendRequest({ url: 'https://x.test/a' })");
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert!(
            result
                .console_entries
                .iter()
                .any(|c| c.message == "rok.sendRequest GET https://x.test/a -> 201 (7 ms)"),
            "{:?}",
            result.console_entries
        );
    }

    #[tokio::test]
    async fn send_request_masks_secrets_in_errors_and_console_lines() {
        let secret = "sk-live-abcdef123";
        let host = FakeHost::failing(&format!(
            "rok.sendRequest: error sending request for url (https://x.test/?k={secret})"
        ));
        let mut ctx = minimal_ctx(&format!(
            "try {{ await rok.sendRequest({{ url: 'https://x.test/?k={secret}' }}); }} \
             catch (e) {{ rok.setVar('e', e.message); }}"
        ));
        ctx.variables.secret_values.insert(secret.to_string());
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        let message = result
            .runtime_vars
            .get("e")
            .and_then(|v| v.as_str())
            .expect("e is a string")
            .to_string();
        assert!(!message.contains(secret), "{message}");
        assert!(message.contains("••••••"), "{message}");
        assert!(result
            .console_entries
            .iter()
            .all(|c| !c.message.contains(secret)));
        assert!(result
            .console_entries
            .iter()
            .any(|c| c.level == rocket_scripting::ConsoleLevel::Error));
    }

    #[tokio::test]
    async fn send_request_json_data_sets_the_content_type_once() {
        let host = FakeHost::ok(200, "{}");
        let ctx = minimal_ctx(
            "await rok.sendRequest({ url: 'https://x.test/1', data: { a: 1 } }); \
             await rok.sendRequest({ url: 'https://x.test/2', data: { a: 1 }, \
               headers: { 'content-type': 'application/vnd.api+json' } });",
        );
        DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        let sent = host.sent();
        let content_types = |i: usize| -> Vec<String> {
            sent[i]
                .headers
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.clone())
                .collect()
        };
        assert_eq!(content_types(0), vec!["application/json".to_string()]);
        assert_eq!(content_types(1), vec!["application/vnd.api+json".to_string()]);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra send_request_`
Expected: FAIL. The callback is never called, `httpsAgent` is sent, no Console line exists, the secret is not masked, and no Content-Type is added.

- [ ] **Step 3: Add Console lines and masking to the op**

In `crates/rocket-infra/src/scripting/ops/host.rs`, change the `rocket_scripting` import to `use rocket_scripting::{ConsoleLevel, HostError, HostRequest};`, add `use crate::scripting::state::ScriptOutputState;`, and replace `op_rok_send_request` with:

```rust
/// rok.sendRequest(options) — sends one HTTP request through the host and adds
/// a Console line for it. Takes a `HostRequest` and returns a `HostResponse`,
/// both as JSON. Secret values are masked in the line and in the error.
#[op2]
#[string]
pub async fn op_rok_send_request(
    state: Rc<RefCell<OpState>>,
    #[string] request_json: String,
) -> Result<String, ScriptHostError> {
    const API: &str = "rok.sendRequest";
    let request: HostRequest = serde_json::from_str(&request_json)
        .map_err(|e| ScriptHostError(format!("{API}: invalid options - {e}")))?;
    let label = format!("{API} {} {}", request.method, request.url);
    let (reply, answer) = oneshot::channel();
    send_host_call(&state, HostCall::Send { request, reply }, API)?;
    let outcome = answer.await.unwrap_or(Err(HostError::Unavailable));
    let mut state = state.borrow_mut();
    match outcome {
        Ok(response) => {
            let line = redact(
                &state,
                format!(
                    "{label} -> {} ({} ms)",
                    response.status, response.response_time_ms
                ),
            );
            state
                .borrow_mut::<ScriptOutputState>()
                .add_console(ConsoleLevel::Log, line);
            serde_json::to_string(&response).map_err(|e| ScriptHostError(format!("{API}: {e}")))
        }
        Err(error) => {
            let error = host_error(&state, API, error);
            let line = redact(&state, format!("{label} failed: {}", error.0));
            state
                .borrow_mut::<ScriptOutputState>()
                .add_console(ConsoleLevel::Error, line);
            Err(error)
        }
    }
}
```

- [ ] **Step 4: Add the callback form, `httpsAgent` and the JSON content type**

In `bootstrap.js`, inside `_sendOptions`, add directly before its `const timeout = ...` line:

```js
    if (bodyIsJson && !headers.some(([k]) => k.toLowerCase() === 'content-type')) {
      headers.push(['Content-Type', 'application/json']);
    }
```

Replace the `sendRequest:` line in the `rok` block with:

```js
    sendRequest: (options, callback) => {
      const sent = (async () => {
        if (options && typeof options === 'object' && options.httpsAgent !== undefined) {
          throw new Error('rok.sendRequest: httpsAgent is not supported');
        }
        return _hostResponse(JSON.parse(await __ops.op_rok_send_request(_sendOptions(options))));
      })();
      if (typeof callback !== 'function') return sent;
      return sent.then((res) => callback(null, res), (err) => callback(err, null));
    },
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -j4 -p rocket-infra -- send_request_ host_`
Expected: PASS (7 new tests plus the 7 `host_` tests from plan 01).

- [ ] **Step 6: Update the typings**

In `ROK_DEFS` in `src/components/editor/rok-types.ts`, add to `interface RokSendRequestOptions`, after the `timeout?: number;` line:

```ts
  /** Not supported. Passing it rejects the call. */
  httpsAgent?: never;
```

and replace the single `sendRequest(...)` line with:

```ts
  /** Send an HTTP request from the script. Variables in the options are not resolved; use rok.interpolate. 4xx and 5xx responses resolve; network errors reject. */
  sendRequest(options: RokSendRequestOptions): Promise<RokResponse>;
  /** Callback form: the callback gets (null, response) or (error, null), and the promise resolves to what it returns. */
  sendRequest(options: RokSendRequestOptions, callback: (err: Error | null, res: RokResponse | null) => unknown): Promise<unknown>;
```

- [ ] **Step 7: Run all checks**

Run: `cargo test -j4 -p rocket-infra scripting && cargo check -j4 && yarn test rok-types && yarn tsc --noEmit && yarn check`
Expected: all PASS.

- [ ] **Step 8: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-infra/src/scripting/ops/host.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `crates/rocket-infra/src/scripting/engine.rs`, `src/components/editor/rok-types.ts`.
Suggested subject: `feat(scripting): add sendRequest callbacks, console lines and masking`.

---

### Task 2: The `rocket-app` host and its wiring

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-app/src/execution_service/script_host.rs`
- Create: `crates/rocket-app/src/execution_service/script_host_tests.rs`
- Modify: `crates/rocket-app/src/execution_service.rs` (module declarations, the `rocket_scripting` import, new `script_host` method, `run_script_phase`, the three phase loops)

**Interfaces:**
- Consumes: plan 01 (`ScriptHost`, `HostRequest`, `HostResponse`, `HostError`, `ScriptEngine::execute_with_host`), Task 1 (the Console line format).
- Produces: `pub(crate) struct ExecutionScriptHost<'a> { pub(crate) svc: &'a RequestExecutionService, pub(crate) options: RequestOptions }` implementing `ScriptHost::send_request`; `pub(crate) fn host_response(response: &HttpResponse) -> HostResponse`; `pub(crate) fn http_request_for(request: &HostRequest, options: &RequestOptions) -> Result<HttpRequest, HostError>`; `RequestExecutionService::script_host(&self, state: &PhaseState) -> ExecutionScriptHost<'_>`; `run_script_phase(&self, script, ctx, host: &dyn ScriptHost, request_name, phase, all_console)`. Plan 04 adds fields to `ExecutionScriptHost` and to `PhaseState`.

- [ ] **Step 1: Write the host module with its failing unit test**

Create `crates/rocket-app/src/execution_service/script_host.rs`:

```rust
//! The `ScriptHost` a request's scripts get. It serves `rok.sendRequest`.
//!
//! It borrows the service, so a call back into the service needs no shared
//! handle and no second script engine.

use std::time::Duration;

use async_trait::async_trait;
use rocket_http::{HttpRequest, HttpResponse, RequestOptions};
use rocket_scripting::{HostError, HostRequest, HostResponse, ScriptHost};
use rocket_shared::types::{Body, BodyMode, Header, HttpMethod};

use super::{body_mode_from_content_type, RequestExecutionService};

/// Serves the host calls of one script run.
pub(crate) struct ExecutionScriptHost<'a> {
    pub(crate) svc: &'a RequestExecutionService,
    /// TLS, redirect, cookie and client-certificate options of the request the
    /// script belongs to. Script requests reuse them.
    pub(crate) options: RequestOptions,
}

/// Turns an executor response into what a script sees.
pub(crate) fn host_response(response: &HttpResponse) -> HostResponse {
    HostResponse {
        status: response.status,
        status_text: response.status_text.clone(),
        headers: response
            .headers
            .iter()
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect(),
        body: response.body.clone(),
        response_time_ms: response.duration_ms,
    }
}

/// Builds the executor request for a `rok.sendRequest` call.
///
/// Variables are not resolved, as in Bruno. The host enforces the script's
/// time limit, so the executor's own limit is one second later as a backstop.
pub(crate) fn http_request_for(
    request: &HostRequest,
    options: &RequestOptions,
) -> Result<HttpRequest, HostError> {
    let method: HttpMethod = request.method.parse().map_err(|_| {
        HostError::Failed(format!("rok.sendRequest: invalid method - {}", request.method))
    })?;
    let mut http = HttpRequest::new(method, request.url.clone());
    http.headers = request
        .headers
        .iter()
        .map(|(key, value)| Header::new(key.clone(), value.clone()))
        .collect();
    if let Some(content) = &request.body {
        let has_content_type = http
            .headers
            .iter()
            .any(|h| h.key.eq_ignore_ascii_case("content-type"));
        let mode = if request.body_is_json {
            BodyMode::Json
        } else if has_content_type {
            body_mode_from_content_type(&http.headers)
        } else {
            BodyMode::Text
        };
        http.body = Some(Body {
            mode,
            content: Some(content.clone()),
            form_data: None,
            file_path: None,
        });
    }
    http.options = options.clone();
    http.options.timeout_ms = request.timeout_ms.saturating_add(1_000);
    Ok(http)
}

#[async_trait]
impl ScriptHost for ExecutionScriptHost<'_> {
    async fn send_request(&self, request: HostRequest) -> Result<HostResponse, HostError> {
        let http = http_request_for(&request, &self.options)?;
        // A RocketVault certificate selected for this URL is fetched first, as for a send.
        let http = self.svc.with_vault_certificates(&http).await;
        let limit = Duration::from_millis(request.timeout_ms);
        match tokio::time::timeout(limit, self.svc.executor.execute(&http)).await {
            Err(_) => Err(HostError::Failed(format!(
                "rok.sendRequest: timed out after {} ms",
                request.timeout_ms
            ))),
            Ok(Err(e)) => Err(HostError::Failed(format!("rok.sendRequest: {e}"))),
            Ok(Ok(response)) => Ok(host_response(&response)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::{CertificateSource, ResolvedClientCertificate};

    fn host_request(method: &str, body: Option<&str>, body_is_json: bool) -> HostRequest {
        HostRequest {
            method: method.into(),
            url: "https://side.test/x".into(),
            headers: vec![],
            body: body.map(str::to_string),
            body_is_json,
            timeout_ms: 1500,
        }
    }

    #[test]
    fn http_request_for_copies_the_calling_request_options() {
        let mut options = RequestOptions::default();
        options.verify_ssl = false;
        options.follow_redirects = false;
        options.max_redirects = Some(2);
        options.client_certificates = vec![ResolvedClientCertificate::pem(
            "side.test",
            CertificateSource::File("/certs/c.pem".into()),
            CertificateSource::File("/certs/k.pem".into()),
            None,
        )];
        let http = http_request_for(&host_request("PUT", None, false), &options).expect("valid");
        assert_eq!(http.method, HttpMethod::Put);
        assert!(!http.options.verify_ssl);
        assert!(!http.options.follow_redirects);
        assert_eq!(http.options.max_redirects, Some(2));
        assert_eq!(http.options.client_certificates.len(), 1);
        assert_eq!(http.options.client_certificates[0].domain, "side.test");
        assert_eq!(http.options.timeout_ms, 2_500);
        assert!(http.body.is_none());
    }

    #[test]
    fn http_request_for_picks_the_body_mode() {
        let options = RequestOptions::default();
        let json = http_request_for(&host_request("POST", Some("{}"), true), &options).expect("valid");
        assert_eq!(json.body.expect("body").mode, BodyMode::Json);
        let text = http_request_for(&host_request("POST", Some("hi"), false), &options).expect("valid");
        assert_eq!(text.body.expect("body").mode, BodyMode::Text);
        let mut xml = host_request("POST", Some("<a/>"), false);
        xml.headers = vec![("Content-Type".into(), "application/xml".into())];
        let xml = http_request_for(&xml, &options).expect("valid");
        assert_eq!(xml.body.expect("body").mode, BodyMode::Xml);
    }

    #[test]
    fn http_request_for_rejects_an_invalid_method() {
        let err = http_request_for(&host_request("NOT A METHOD", None, false), &RequestOptions::default())
            .expect_err("invalid");
        assert_eq!(
            err,
            HostError::Failed("rok.sendRequest: invalid method - NOT A METHOD".into())
        );
    }
}
```

In `crates/rocket-app/src/execution_service.rs`, add after `pub(crate) mod script_chain;`:

```rust
pub(crate) mod script_host;
#[cfg(test)]
mod script_host_tests;
```

Create `crates/rocket-app/src/execution_service/script_host_tests.rs` with only this line for now (Step 4 fills it):

```rust
//! The script host of a request, run with the real script engine.
```

Run: `cargo test -j4 -p rocket-app script_host::tests`
Expected: PASS (3 tests). If `ResolvedClientCertificate` or `CertificateSource` is not exported at the `rocket_http` root, import them from `rocket_http::resolved_certificate` instead.

- [ ] **Step 2: Pass the host through the phases**

In `crates/rocket-app/src/execution_service.rs`:

1. Add `ScriptHost` to the `use rocket_scripting::{ ... }` list at the top.

2. Add this method to `impl RequestExecutionService`, directly above `async fn run_script_phase(`:

```rust
    /// The host for one script run of a request. Script requests reuse the
    /// request's TLS, redirect, cookie and client-certificate options.
    fn script_host(&self, state: &PhaseState) -> script_host::ExecutionScriptHost<'_> {
        script_host::ExecutionScriptHost {
            svc: self,
            options: state.http_request.options.clone(),
        }
    }
```

3. Change `run_script_phase` to take the host and use it. Its signature becomes:

```rust
    async fn run_script_phase(
        &self,
        script: &ChainedScript,
        ctx: ScriptContext,
        host: &dyn ScriptHost,
        request_name: &str,
        phase: &str,
        all_console: &mut Vec<ConsoleEntry>,
    ) -> ScriptResult {
```

and its `match engine.execute(ctx).await {` line becomes:

```rust
        match engine.execute_with_host(ctx, host).await {
```

4. In `run_before_request_phase`, replace:

```rust
            let result = self
                .run_script_phase(
                    script,
                    ctx,
                    &request_name,
                    "before-request",
                    &mut state.console,
                )
                .await;
```

with:

```rust
            let host = self.script_host(state);
            let result = self
                .run_script_phase(
                    script,
                    ctx,
                    &host,
                    &request_name,
                    "before-request",
                    &mut state.console,
                )
                .await;
```

5. In `run_after_response_phase`, replace:

```rust
            let result = self
                .run_script_phase(
                    script,
                    ctx,
                    &request_name,
                    "after-response",
                    &mut state.console,
                )
                .await;
```

with:

```rust
            let host = self.script_host(state);
            let result = self
                .run_script_phase(
                    script,
                    ctx,
                    &host,
                    &request_name,
                    "after-response",
                    &mut state.console,
                )
                .await;
```

6. In `run_tests_phase`, replace:

```rust
            let result = self
                .run_script_phase(script, ctx, &request_name, "tests", &mut state.console)
                .await;
```

with:

```rust
            let host = self.script_host(state);
            let result = self
                .run_script_phase(script, ctx, &host, &request_name, "tests", &mut state.console)
                .await;
```

Run: `cargo check -j4 -p rocket-app`
Expected: compiles.

- [ ] **Step 3: Run the existing app tests**

Run: `cargo test -j4 -p rocket-app execution_service && cargo test -j4 -p rocket-app collection_runner_service`
Expected: PASS. Every test engine there relies on the default `execute_with_host`, which proves nothing changed for engines that ignore the host.

- [ ] **Step 4: Write the failing end-to-end tests with the real engine**

Replace the contents of `crates/rocket-app/src/execution_service/script_host_tests.rs` with:

```rust
//! The script host of a request, run with the real script engine.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rocket_collection::Collection;
use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_shared::error::DomainResult;
use rocket_shared::events::NullEventPublisher;
use rocket_shared::types::{BodyMode, HttpMethod};

use super::RequestExecutionService;
use crate::graphql_request::test_support::input;
use crate::test_doubles::{
    EmptySecretManagerRepo, InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo,
    NullEnvRepo, SharedCollectionRepo, SharedHistoryRepo,
};

/// Executor that keeps every request it gets and answers 200 with `{"ok":true}`.
#[derive(Default)]
struct KeepingExecutor {
    seen: Mutex<Vec<HttpRequest>>,
}

impl KeepingExecutor {
    fn seen(&self) -> Vec<HttpRequest> {
        self.seen.lock().expect("lock").clone()
    }
}

#[async_trait]
impl HttpExecutor for KeepingExecutor {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse> {
        self.seen.lock().expect("lock").push(request.clone());
        Ok(HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![],
            body: "{\"ok\":true}".into(),
            duration_ms: 3,
            ttfb_ms: 1,
            size_bytes: 11,
            ..Default::default()
        })
    }
}

/// Hands one `Arc<KeepingExecutor>` to a service expecting `Arc<dyn HttpExecutor>`.
struct SharedKeeping(Arc<KeepingExecutor>);

#[async_trait]
impl HttpExecutor for SharedKeeping {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse> {
        self.0.execute(request).await
    }
}

/// A service with the real script engine, sending through `executor`.
fn service(executor: Arc<KeepingExecutor>) -> RequestExecutionService {
    RequestExecutionService::new(
        Box::new(NullEnvRepo),
        Arc::new(SharedKeeping(executor)),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            Collection::new("api"),
        ))),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(rocket_infra::scripting::DenoScriptEngine::new()))
}

#[tokio::test]
async fn a_pre_request_script_sends_with_the_request_options() {
    let executor = Arc::new(KeepingExecutor::default());
    let svc = service(Arc::clone(&executor));
    let mut req = input("https://main.test/x");
    req.method = HttpMethod::Get;
    req.options.verify_ssl = false;
    req.options.follow_redirects = false;
    req.pre_request_script = Some(
        "const r = await rok.sendRequest({ method: 'POST', url: 'https://side.test/token', \
         data: { a: 1 } }); console.log('side', r.status, r.data.ok);"
            .into(),
    );
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);

    let seen = executor.seen();
    assert_eq!(seen.len(), 2, "the side request, then the main request");
    assert_eq!(seen[0].url, "https://side.test/token");
    assert_eq!(seen[0].method, HttpMethod::Post);
    assert!(!seen[0].options.verify_ssl);
    assert!(!seen[0].options.follow_redirects);
    assert_eq!(seen[0].options.timeout_ms, 31_000);
    let body = seen[0].body.as_ref().expect("side body");
    assert_eq!(body.mode, BodyMode::Json);
    assert_eq!(body.content.as_deref(), Some("{\"a\":1}"));
    assert_eq!(seen[1].url, "https://main.test/x");

    let lines: Vec<&str> = out.console_entries.iter().map(|e| e.message.as_str()).collect();
    assert!(lines.contains(&"side 200 true"), "{lines:?}");
    assert!(
        lines.contains(&"rok.sendRequest POST https://side.test/token -> 200 (3 ms)"),
        "{lines:?}"
    );
}

#[tokio::test]
async fn post_response_and_tests_scripts_can_send_too() {
    let executor = Arc::new(KeepingExecutor::default());
    let svc = service(Arc::clone(&executor));
    let mut req = input("https://main.test/x");
    req.method = HttpMethod::Get;
    req.post_response_script =
        Some("await rok.sendRequest({ url: 'https://side.test/after' });".into());
    req.tests_script = Some("await rok.sendRequest({ url: 'https://side.test/tests' });".into());
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    let urls: Vec<String> = executor.seen().into_iter().map(|r| r.url).collect();
    assert_eq!(
        urls,
        vec![
            "https://main.test/x".to_string(),
            "https://side.test/after".to_string(),
            "https://side.test/tests".to_string(),
        ]
    );
}
```

Run: `cargo test -j4 -p rocket-app script_host_tests`
Expected: PASS (2 tests). Before Step 2 these would have failed with `rok.sendRequest is not available here`; to see that, temporarily revert Step 2's `execute_with_host` line and run again, then restore it.

- [ ] **Step 5: Run all checks**

Run: `cargo check -j4 && cargo test -j4 -p rocket-app execution_service && cargo test -j4 -p rocket-app collection_runner_service && cargo test -j4 -p rocket-app flow_execution_service`
Expected: all PASS.

- [ ] **Step 6: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-app/src/execution_service/script_host.rs`, `crates/rocket-app/src/execution_service/script_host_tests.rs`, `crates/rocket-app/src/execution_service.rs`.
Suggested subject: `feat(app): serve rok.sendRequest through the request executor`.

---

### Task 3: End-to-end tests against a real server

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-app/src/execution_service/script_host_tests.rs`

**Interfaces:**
- Consumes: Task 2 (`ExecutionScriptHost`, the wiring), Task 1 (masking).
- Produces: tests only.

- [ ] **Step 1: Write the tests**

Add to the end of `crates/rocket-app/src/execution_service/script_host_tests.rs`:

```rust
// ── against a real server ────────────────────────────────────────────────────

use rocket_environment::{Environment, Variable};
use std::time::Duration;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::test_doubles::StaticEnvRepo;

const SECRET: &str = "sk-live-abcdef123";

/// A service with the real engine and the real `ReqwestExecutor`. The
/// environment `dev` holds the secret variable `API_KEY`.
fn real_service() -> RequestExecutionService {
    let mut env = Environment::new("dev");
    env.set_variable(Variable::secret("API_KEY", SECRET));
    RequestExecutionService::new(
        Box::new(StaticEnvRepo(env)),
        Arc::new(rocket_infra::ReqwestExecutor::new()),
        Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
        Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(
            Collection::new("api"),
        ))),
        Box::new(NullCookieRepo),
        Box::new(NullEventPublisher),
        Box::new(EmptySecretManagerRepo),
        Arc::new(rocket_environment::NullSecretStore),
        Arc::new(rocket_environment::NullVaultSecretFetcher),
    )
    .with_script_engine(Box::new(rocket_infra::scripting::DenoScriptEngine::new()))
}

/// Starts a server whose `/main` answers 200, and returns it.
async fn server_with_main() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/main"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

/// Sends `GET <server>/main` in environment `dev` with `script` as its
/// pre-request script and returns the Console lines.
async fn run_pre_request(server: &MockServer, script: String) -> Vec<String> {
    let svc = real_service();
    let mut req = input(&format!("{}/main", server.uri()));
    req.method = HttpMethod::Get;
    req.environment_name = Some("dev".into());
    req.pre_request_script = Some(script);
    let out = svc.execute(req).await.expect("execute");
    assert!(out.script_error.is_none(), "{:?}", out.script_error);
    out.console_entries.into_iter().map(|e| e.message).collect()
}

#[tokio::test]
async fn real_server_json_reply_is_parsed() {
    let server = server_with_main().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header("content-type", "application/json"))
        .and(body_json(serde_json::json!({ "user": "a" })))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("X-Trace", "t-1")
                .set_body_json(serde_json::json!({ "token": "abc" })),
        )
        .mount(&server)
        .await;
    let lines = run_pre_request(
        &server,
        format!(
            "const r = await rok.sendRequest({{ method: 'POST', url: '{}/token', data: {{ user: 'a' }} }}); \
             console.log(r.status, r.headers['x-trace'], r.data.token);",
            server.uri()
        ),
    )
    .await;
    assert!(lines.iter().any(|l| l == "201 t-1 abc"), "{lines:?}");
}

#[tokio::test]
async fn real_server_404_resolves() {
    let server = server_with_main().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let lines = run_pre_request(
        &server,
        format!(
            "const r = await rok.sendRequest({{ url: '{}/missing' }}); console.log('status', r.status);",
            server.uri()
        ),
    )
    .await;
    assert!(lines.iter().any(|l| l == "status 404"), "{lines:?}");
}

#[tokio::test]
async fn real_server_slower_than_the_timeout_rejects() {
    let server = server_with_main().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let lines = run_pre_request(
        &server,
        format!(
            "try {{ await rok.sendRequest({{ url: '{}/slow', timeout: 200 }}); }} \
             catch (e) {{ console.log('caught', e.message); }}",
            server.uri()
        ),
    )
    .await;
    assert!(
        lines
            .iter()
            .any(|l| l == "caught rok.sendRequest: timed out after 200 ms"),
        "{lines:?}"
    );
    assert!(started.elapsed() < Duration::from_millis(1_800));
}

#[tokio::test]
async fn real_network_error_is_masked() {
    let server = server_with_main().await;
    // A port that was just free: connecting to it is refused.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .expect("free port");
    let lines = run_pre_request(
        &server,
        format!(
            "try {{ await rok.sendRequest({{ url: 'http://127.0.0.1:{port}/?key=' + rok.getEnvVar('API_KEY') }}); }} \
             catch (e) {{ console.log('caught', e.message); }}"
        ),
    )
    .await;
    assert!(lines.iter().all(|l| !l.contains(SECRET)), "{lines:?}");
    assert!(
        lines.iter().any(|l| l.starts_with("caught rok.sendRequest:")),
        "{lines:?}"
    );
}

#[tokio::test]
async fn real_server_echo_of_a_secret_is_masked_in_the_console() {
    let server = server_with_main().await;
    Mock::given(method("GET"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "echo": SECRET })))
        .mount(&server)
        .await;
    let lines = run_pre_request(
        &server,
        format!(
            "const r = await rok.sendRequest({{ url: '{}/echo' }}); console.log(JSON.stringify(r.data));",
            server.uri()
        ),
    )
    .await;
    assert!(lines.iter().all(|l| !l.contains(SECRET)), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("••••••")), "{lines:?}");
}
```

- [ ] **Step 2: Run them**

Run: `cargo test -j4 -p rocket-app script_host_tests`
Expected: PASS (7 tests: 2 from Task 2 and 5 here). These need no code change if Tasks 1 and 2 are right; a failure here is a real bug in them, so fix it there and re-run both tasks' tests.

- [ ] **Step 3: Run all checks**

Run: `cargo check -j4 && cargo test -j4 -p rocket-app execution_service`
Expected: PASS.

- [ ] **Step 4: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of `crates/rocket-app/src/execution_service/script_host_tests.rs`.
Suggested subject: `test(app): cover rok.sendRequest against a real server`.

## Manual check (real app)

Run `yarn tauri dev`. In a request's pre-request script run `const r = await rok.sendRequest({ url: 'https://httpbin.org/json' }); console.log(r.status, r.data.slideshow.title)`. The Console shows the `rok.sendRequest GET ... -> 200` line and the logged values. Try a `timeout: 1` call and confirm the rejection message, and a URL holding a secret environment variable and confirm the Console masks it.

---

## Next plan to execute

When Task 3 is complete, its checks pass, the manual check is done or handed to the user, and the ledger (`.superpowers/sdd/rok-parity-b-03-send-request/progress.md`) shows "Task 3: complete", **the executing Claude must go straight on to plan 04**: `docs/superpowers/plans/rok-parity-b/04-run-request.md`. No consent is needed between plans. Run one plan at a time, and swap the visible task list to plan 04's tasks when it starts.

Plan 04 depends on this plan: it adds fields to `ExecutionScriptHost` and reuses `host_response`, `script_host_tests.rs` and its `KeepingExecutor`. Do not start it on a tree where this plan's checks fail.
