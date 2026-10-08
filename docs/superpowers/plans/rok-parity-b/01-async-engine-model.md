# rok parity B, plan 01: async engine model, host bridge and `rok.sleep`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run user scripts as async functions on a real event loop, add the `ScriptHost` trait with `ScriptEngine::execute_with_host`, a basic `rok.sendRequest` that reaches the host, and `rok.sleep`.

**Architecture:** The blocking script thread builds its own current-thread Tokio runtime and drives deno_core's event loop until the wrapped script's promise settles and no work is left. Async ops reach the host by sending a `HostCall` with a oneshot reply over an mpsc channel to the task that called the engine, which serves it with the borrowed `&dyn ScriptHost`. Without a host the channel is absent and host ops reject with "is not available here".

**Tech Stack:** Rust, `deno_core` 0.400 (`#[op2]` async ops, `JsRuntime::resolve`, `with_event_loop_promise`, `run_event_loop`), Tokio, `futures-util`, `async-trait`, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-07-rok-js-api-parity-b-async-design.md`. Index with rulings: `00-plan-index.md` (rulings 1, 2, 5, 6 and 12 apply here).

## Global Constraints

- `cargo` commands always pass `-j4`. No `cargo test --workspace`.
- `this` at the top level of a script stays `globalThis`. `require`, bundled modules and local modules behave as before.
- Script error text keeps the `Uncaught <Name>: <message>` shape. `(in promise)` never reaches the user.
- With no host, `rok.sendRequest` rejects with exactly `rok.sendRequest is not available here`.
- `rok.sleep(ms)` clamps to 0..60000. A non-number or `NaN` rejects with a `TypeError`.
- The 5 s wall-clock `SCRIPT_TIMEOUT` stays as it is in this plan. Plan 02 replaces it.
- `rok-types.ts` and `bootstrap.js` stay in sync (the existing sync test enforces it).
- Comments are short full sentences ending in a period.

## Review Focus

- A script whose last line is a `// comment` with no trailing newline: the wrapper still closes and the script runs.
- A script that starts with `"use strict";`: it is still strict (assigning an undeclared name throws).
- A promise callback that throws and is never awaited: it becomes the script error instead of vanishing.
- `Promise.all` over two `rok.sendRequest` calls: the host serves both and both resolve.
- `test('x', async () => { throw ... })` without `await`: it is recorded as failed, not passed.

---

### Task 1: Wrap user code and run the event loop

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (the `deno_core` import, `run_script`, new helpers above it, tests module)

**Interfaces:**
- Consumes: nothing new.
- Produces: `fn run_script(ctx: ScriptContext, handle_tx: oneshot::Sender<v8::IsolateHandle>) -> DomainResult<ScriptResult>` (same signature, now builds a current-thread runtime), `async fn run_script_async(...)` (the old body), `fn wrap_user_code(code: &str) -> String`, `async fn run_user_code(runtime: &mut JsRuntime, code: &str) -> Option<String>`, `fn script_error_message(raw: String) -> String`. Later tasks add parameters to `run_script` and `run_script_async`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/rocket-infra/src/scripting/engine.rs`, after `script_error_captured`:

```rust
    // ── async engine model ───────────────────────────────────────────────────

    #[tokio::test]
    async fn async_model_top_level_await_and_return_work() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            "const v = await Promise.resolve(41); rok.setVar('v', v + 1); \
             if (v) { return; } rok.setVar('after', 1);",
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.runtime_vars.get("v").expect("v present"), 42);
        assert!(!result.runtime_vars.contains_key("after"));
    }

    #[tokio::test]
    async fn async_model_this_is_still_the_global_object() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('same', this === globalThis)");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("same").expect("same present"), true);
    }

    #[tokio::test]
    async fn async_model_promise_callbacks_now_run() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("Promise.resolve().then(() => rok.setVar('late', 'yes'))");
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("late").expect("late present"), "yes");
    }

    #[tokio::test]
    async fn async_model_a_sync_throw_keeps_its_old_message_shape() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("throw new Error('deliberate')");
        let result = engine.execute(ctx).await.expect("execute");
        let err = result.error.expect("script error");
        assert!(err.contains("Error: deliberate"), "{err}");
        assert!(!err.contains("in promise"), "{err}");
    }

    #[tokio::test]
    async fn async_model_a_rejected_await_is_the_script_error() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("await Promise.reject(new Error('nope'))");
        let result = engine.execute(ctx).await.expect("execute");
        let err = result.error.expect("script error");
        assert!(err.contains("Error: nope"), "{err}");
        assert!(!err.contains("in promise"), "{err}");
    }

    #[tokio::test]
    async fn async_model_an_unhandled_rejection_is_the_script_error() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            "Promise.resolve().then(() => { throw new Error('stray'); }); rok.setVar('ran', 1)",
        );
        let result = engine.execute(ctx).await.expect("execute");
        let err = result.error.expect("script error");
        assert!(err.contains("stray"), "{err}");
        assert_eq!(result.runtime_vars.get("ran").expect("ran present"), 1);
    }

    #[tokio::test]
    async fn async_model_a_promise_that_never_settles_ends_with_an_error() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("await new Promise(() => {}); rok.setVar('after', 1)");
        let result = engine.execute(ctx).await.expect("execute");
        let err = result.error.expect("script error");
        assert!(err.contains("never settles"), "{err}");
        assert!(!result.runtime_vars.contains_key("after"));
    }

    #[tokio::test]
    async fn async_model_a_syntax_error_is_still_reported() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("const = ;");
        let result = engine.execute(ctx).await.expect("execute");
        let err = result.error.expect("script error");
        assert!(err.contains("SyntaxError"), "{err}");
    }

    #[tokio::test]
    async fn async_model_top_level_declarations_are_function_scoped() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            "var a = 1; function f() {} \
             rok.setVar('t', typeof globalThis.a + ',' + typeof globalThis.f + ',' + typeof f)",
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("t").expect("t present"),
            "undefined,undefined,function"
        );
    }

    #[tokio::test]
    async fn async_model_require_and_hidden_globals_are_unchanged() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx(
            "const { v4 } = require('uuid'); \
             rok.setVar('n', v4().length + ',' + typeof Deno + ',' + typeof __ops + ',' + typeof __bootstrap)",
        );
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("n").expect("n present"),
            "36,undefined,undefined,undefined"
        );
    }

    #[tokio::test]
    async fn async_model_a_trailing_line_comment_does_not_break_the_wrapper() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("rok.setVar('ok', true) // done");
        let result = engine.execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.runtime_vars.get("ok").expect("ok present"), true);
    }

    #[tokio::test]
    async fn async_model_use_strict_still_applies() {
        let engine = DenoScriptEngine::new();
        let ctx = minimal_ctx("\"use strict\"; undeclaredName = 1;");
        let result = engine.execute(ctx).await.expect("execute");
        let err = result.error.expect("script error");
        assert!(err.contains("ReferenceError"), "{err}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra async_model_`
Expected: FAIL. `top_level_await_and_return_work` and `never_settles` fail with a `SyntaxError` (top-level `await` and `return` are not allowed in a classic script), `promise_callbacks_now_run` panics on the missing `late` key, `top_level_declarations_are_function_scoped` gets `number,function,function`. Some of the others pass already; that is expected.

- [ ] **Step 3: Implement the async model**

In `crates/rocket-infra/src/scripting/engine.rs`:

1. Change the `deno_core` import to:

```rust
use deno_core::{extension, op2, v8, JsRuntime, OpState, PollEventLoopOptions, RuntimeOptions};
```

2. Rename the existing `fn run_script(` to `async fn run_script_async(`. Keep its parameters and body, except replace this block:

```rust
    // Capture script-level exceptions rather than propagating them as errors.
    let script_error = match runtime.execute_script("<user>", code) {
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    };
```

with:

```rust
    // Capture script-level exceptions rather than propagating them as errors.
    let script_error = run_user_code(&mut runtime, &code).await;
```

3. Directly above `async fn run_script_async(`, add:

```rust
/// Runs one script on the calling blocking thread.
///
/// deno_core spawns async op futures on the current Tokio runtime and expects
/// that runtime to be single-threaded, so this thread builds its own
/// current-thread runtime and drives the script on it.
fn run_script(
    ctx: ScriptContext,
    handle_tx: oneshot::Sender<v8::IsolateHandle>,
) -> DomainResult<ScriptResult> {
    let tokio_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| DomainError::Internal(format!("script runtime could not start: {e}")))?;
    tokio_rt.block_on(run_script_async(ctx, handle_tx))
}

/// Wraps user code as the body of an async function, so top-level `await` and
/// `return` work. The opening stays on the first line, so error line numbers
/// do not move. The newline before the closing brace ends a trailing comment.
fn wrap_user_code(code: &str) -> String {
    format!("(async function () {{ {code}\n}}).call(globalThis)")
}

/// Runs the wrapped user code and the event loop, and returns the script error.
///
/// The loop runs until the script's promise settles and then until no work is
/// left, so callback-style work the script did not await still finishes.
async fn run_user_code(runtime: &mut JsRuntime, code: &str) -> Option<String> {
    let promise = match runtime.execute_script("<user>", wrap_user_code(code)) {
        Ok(promise) => promise,
        Err(e) => return Some(script_error_message(e.to_string())),
    };
    let resolve = runtime.resolve(promise);
    if let Err(e) = runtime
        .with_event_loop_promise(resolve, PollEventLoopOptions::default())
        .await
    {
        return Some(script_error_message(e.to_string()));
    }
    runtime
        .run_event_loop(PollEventLoopOptions::default())
        .await
        .err()
        .map(|e| script_error_message(e.to_string()))
}

/// Keeps script errors in the shape they had before scripts ran as async functions.
fn script_error_message(raw: String) -> String {
    if raw.contains("Promise resolution is still pending") {
        return "the script awaited a promise that never settles".to_string();
    }
    raw.replacen("Uncaught (in promise) ", "Uncaught ", 1)
}
```

`run_script_with_timeout` still calls `run_script(ctx, handle_tx)` inside `spawn_blocking`; leave it unchanged.

- [ ] **Step 4: Run the new tests**

Run: `cargo test -j4 -p rocket-infra async_model_`
Expected: PASS (12 tests). If the build panics with "Cannot start a runtime from within a runtime", stop and follow the matching open risk in `00-plan-index.md`.

- [ ] **Step 5: Run the whole scripting suite to prove nothing else moved**

Run: `cargo test -j4 -p rocket-infra scripting`
Expected: PASS, including the timeout tests (`infinite_loop_script_is_terminated_by_timeout`, `queued_script_that_times_out_before_starting_is_still_terminated`), the `require_local_*` tests and the redaction tests. If an existing test relied on a top-level `var` becoming a global, change that test to use `globalThis.<name> = ...` and log a `Ruling:` line in the ledger.

- [ ] **Step 6: Run the app-level script tests**

Run: `cargo test -j4 -p rocket-app flow_execution_service && cargo test -j4 -p rocket-app execution_service`
Expected: PASS. These use the real `DenoScriptEngine` for Flow transforms and jsonq actions.

- [ ] **Step 7: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of exactly `crates/rocket-infra/src/scripting/engine.rs`.
Suggested subject: `feat(scripting): run scripts as async functions on an event loop`.

---

### Task 2: `ScriptHost`, `execute_with_host` and a basic `rok.sendRequest`

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Create: `crates/rocket-scripting/src/host.rs`
- Modify: `crates/rocket-scripting/src/engine.rs`, `crates/rocket-scripting/src/lib.rs`
- Create: `crates/rocket-infra/src/scripting/host_bridge.rs`, `crates/rocket-infra/src/scripting/ops/host.rs`
- Modify: `crates/rocket-infra/src/scripting/mod.rs`, `crates/rocket-infra/src/scripting/ops/mod.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`
- Modify: `src/components/editor/rok-types.ts`

**Interfaces:**
- Consumes: Task 1 (`run_script`, `run_script_async`).
- Produces (rocket-scripting): `pub struct HostRequest { method: String, url: String, headers: Vec<(String, String)>, body: Option<String>, body_is_json: bool, timeout_ms: u64 }`, `pub struct HostResponse { status: u16, status_text: String, headers: Vec<(String, String)>, body: String, response_time_ms: u64 }` (both `Serialize`/`Deserialize`, snake_case), `pub enum HostError { Unavailable, Failed(String) }`, `#[async_trait] pub trait ScriptHost: Send + Sync { async fn send_request(&self, request: HostRequest) -> Result<HostResponse, HostError> }` (default body returns `Err(HostError::Unavailable)`), and `ScriptEngine::execute_with_host(&self, ctx: ScriptContext, host: &dyn ScriptHost) -> DomainResult<ScriptResult>` (default delegates to `execute`).
- Produces (rocket-infra): `pub enum HostCall { Send { request, reply } }`, `pub struct HostChannel(pub Option<mpsc::UnboundedSender<HostCall>>)`, `pub async fn serve_host_call(host: &dyn ScriptHost, call: HostCall)`, `pub struct ScriptHostError(pub String)` (JS class `Error`), `op_rok_send_request`, `pub(crate) fn send_host_call(...)`, `pub(crate) fn host_error(...)`, and `async fn run_script_bounded(ctx: ScriptContext, host: Option<&dyn ScriptHost>, timeout: Duration) -> DomainResult<ScriptResult>`. `run_script` gains `calls: Option<mpsc::UnboundedSender<HostCall>>`.
- Produces (JS): `rok.sendRequest(options)` resolving to `{ status, statusText, headers, data, responseTime }`; private helpers `_hostResponse(r)` and `_sendOptions(options)` in `bootstrap.js`.

- [ ] **Step 1: Write the failing rocket-scripting tests and the host module**

Create `crates/rocket-scripting/src/host.rs`:

```rust
//! The host side of async script calls such as `rok.sendRequest`.
//!
//! `rocket-infra` runs the script and forwards each call to a `ScriptHost`.
//! `rocket-app` implements it. The types are plain data, so this crate does no I/O.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// An HTTP request a script sends with `rok.sendRequest`.
///
/// The script engine builds it from JSON. It is not an IPC or persistence type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<String>,
    /// True when the script passed an object or array, so `body` is JSON text.
    #[serde(default)]
    pub body_is_json: bool,
    /// Time limit for this request in milliseconds.
    pub timeout_ms: u64,
}

/// A response handed back to a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub response_time_ms: u64,
}

/// Why a host call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// No host serves this call, for example in a Flow transform node.
    Unavailable,
    /// The call ran and failed. The text is shown to the script after redaction.
    Failed(String),
}

/// Calls a script makes that need the application, such as network requests.
///
/// Every method has a default that reports `Unavailable`, so a host implements
/// only what it supports.
#[async_trait]
pub trait ScriptHost: Send + Sync {
    /// Sends one HTTP request for `rok.sendRequest`.
    async fn send_request(&self, _request: HostRequest) -> Result<HostResponse, HostError> {
        Err(HostError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_request_reads_the_engine_json_shape() {
        let json = r#"{"method":"POST","url":"https://x.test","headers":[["a","1"]],"body":"{}","body_is_json":true,"timeout_ms":30000}"#;
        let request: HostRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(request.headers, vec![("a".to_string(), "1".to_string())]);
        assert_eq!(request.body.as_deref(), Some("{}"));
        assert!(request.body_is_json);
        assert_eq!(request.timeout_ms, 30_000);
    }

    #[test]
    fn host_request_defaults_its_optional_fields() {
        let request: HostRequest =
            serde_json::from_str(r#"{"method":"GET","url":"u","timeout_ms":1}"#).expect("parse");
        assert!(request.headers.is_empty());
        assert!(request.body.is_none());
        assert!(!request.body_is_json);
    }

    #[test]
    fn host_response_writes_snake_case_keys() {
        let response = HostResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![],
            body: String::new(),
            response_time_ms: 3,
        };
        let json = serde_json::to_string(&response).expect("serialize");
        assert!(json.contains("\"status_text\":\"OK\""), "{json}");
        assert!(json.contains("\"response_time_ms\":3"), "{json}");
    }

    #[test]
    fn script_host_is_object_safe() {
        fn _assert(_: &dyn ScriptHost) {}
    }
}
```

In `crates/rocket-scripting/src/lib.rs`, add `pub mod host;` after `pub mod engine;` and add this export line after `pub use engine::ScriptEngine;`:

```rust
pub use host::{HostError, HostRequest, HostResponse, ScriptHost};
```

- [ ] **Step 2: Add `execute_with_host` to the engine trait**

Replace the body of `crates/rocket-scripting/src/engine.rs` with:

```rust
use crate::{ScriptContext, ScriptHost, ScriptResult};
use async_trait::async_trait;
use rocket_shared::error::DomainResult;

/// Contract for a JS script execution engine.
///
/// `rocket-infra` provides `DenoScriptEngine` which implements this using `deno_core`.
/// `rocket-app` depends on this trait via `Box<dyn ScriptEngine>` — it never
/// constructs `DenoScriptEngine` directly.
#[async_trait]
pub trait ScriptEngine: Send + Sync {
    /// Execute `ctx.code` in a sandboxed JS runtime for the given lifecycle phase.
    ///
    /// Returns a `ScriptResult` carrying all side-effects to apply (variable mutations,
    /// request mutations, test outcomes, console entries). The engine itself applies
    /// nothing — callers apply mutations after this call returns. Host calls such as
    /// `rok.sendRequest` reject because no host is attached.
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult>;

    /// Like `execute`, with a host that serves `rok.sendRequest` and similar calls.
    ///
    /// The default ignores the host, so test engines keep working unchanged.
    async fn execute_with_host(
        &self,
        ctx: ScriptContext,
        host: &dyn ScriptHost,
    ) -> DomainResult<ScriptResult> {
        let _ = host;
        self.execute(ctx).await
    }
}
```

Run: `cargo test -j4 -p rocket-scripting`
Expected: PASS (the four new `host::tests` plus the existing ones).

- [ ] **Step 3: Write the failing engine tests**

Add to the `tests` module in `crates/rocket-infra/src/scripting/engine.rs`, after the `async_model_` tests:

```rust
    // ── host calls ───────────────────────────────────────────────────────────

    use rocket_scripting::{HostError, HostRequest, HostResponse, ScriptHost};
    use std::sync::Mutex as StdMutex;

    /// Host that records each request and answers with a fixed result.
    struct FakeHost {
        sent: StdMutex<Vec<HostRequest>>,
        answer: Result<HostResponse, HostError>,
    }

    impl FakeHost {
        fn ok(status: u16, body: &str) -> Self {
            Self {
                sent: StdMutex::new(Vec::new()),
                answer: Ok(HostResponse {
                    status,
                    status_text: "OK".into(),
                    headers: vec![("Content-Type".into(), "application/json".into())],
                    body: body.into(),
                    response_time_ms: 7,
                }),
            }
        }

        fn failing(message: &str) -> Self {
            Self {
                sent: StdMutex::new(Vec::new()),
                answer: Err(HostError::Failed(message.into())),
            }
        }

        fn sent(&self) -> Vec<HostRequest> {
            self.sent.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl ScriptHost for FakeHost {
        async fn send_request(&self, request: HostRequest) -> Result<HostResponse, HostError> {
            self.sent.lock().expect("lock").push(request);
            self.answer.clone()
        }
    }

    /// Host that implements nothing, so every call reports `Unavailable`.
    struct BareHost;

    #[async_trait]
    impl ScriptHost for BareHost {}

    #[tokio::test]
    async fn host_send_request_resolves_with_the_host_response() {
        let host = FakeHost::ok(201, "{\"id\":7}");
        let ctx = minimal_ctx(
            "const r = await rok.sendRequest({ method: 'post', url: 'https://x.test/a', \
             headers: { 'X-A': 1 }, data: { n: 1 }, timeout: 1500 }); \
             rok.setVar('out', [r.status, r.statusText, r.headers['content-type'], r.data.id, r.responseTime].join('|'))",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(
            result.runtime_vars.get("out").expect("out present"),
            "201|OK|application/json|7|7"
        );
        let sent = host.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].method, "POST");
        assert_eq!(sent[0].url, "https://x.test/a");
        assert_eq!(sent[0].headers[0], ("X-A".to_string(), "1".to_string()));
        assert_eq!(sent[0].body.as_deref(), Some("{\"n\":1}"));
        assert!(sent[0].body_is_json);
        assert_eq!(sent[0].timeout_ms, 1500);
    }

    #[tokio::test]
    async fn host_send_request_defaults_method_and_timeout_and_keeps_text_bodies() {
        let host = FakeHost::ok(200, "plain text");
        let ctx = minimal_ctx(
            "const r = await rok.sendRequest({ url: 'https://x.test', data: 'hello' }); \
             rok.setVar('d', r.data)",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(result.runtime_vars.get("d").expect("d present"), "plain text");
        let sent = host.sent();
        assert_eq!(sent[0].method, "GET");
        assert_eq!(sent[0].body.as_deref(), Some("hello"));
        assert!(!sent[0].body_is_json);
        assert_eq!(sent[0].timeout_ms, 30_000);
    }

    #[tokio::test]
    async fn host_send_request_without_a_host_rejects_as_not_available() {
        let ctx = minimal_ctx(
            "try { await rok.sendRequest({ url: 'https://x.test' }); } \
             catch (e) { rok.setVar('e', e.message); }",
        );
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert_eq!(
            result.runtime_vars.get("e").expect("e present"),
            "rok.sendRequest is not available here"
        );
    }

    #[tokio::test]
    async fn host_send_request_with_a_host_that_lacks_it_rejects_as_not_available() {
        let ctx = minimal_ctx(
            "try { await rok.sendRequest({ url: 'https://x.test' }); } \
             catch (e) { rok.setVar('e', e.message); }",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &BareHost)
            .await
            .expect("execute");
        assert_eq!(
            result.runtime_vars.get("e").expect("e present"),
            "rok.sendRequest is not available here"
        );
    }

    #[tokio::test]
    async fn host_send_request_failure_rejects_with_a_plain_error() {
        let host = FakeHost::failing("rok.sendRequest: connection refused");
        let ctx = minimal_ctx(
            "try { await rok.sendRequest({ url: 'https://x.test' }); } \
             catch (e) { rok.setVar('e', e.message); rok.setVar('plain', e instanceof Error && !(e instanceof TypeError)); }",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(
            result.runtime_vars.get("e").expect("e present"),
            "rok.sendRequest: connection refused"
        );
        assert_eq!(result.runtime_vars.get("plain").expect("plain present"), true);
    }

    #[tokio::test]
    async fn host_send_request_without_a_url_rejects_before_the_host() {
        let host = FakeHost::ok(200, "");
        let ctx = minimal_ctx(
            "try { await rok.sendRequest({ method: 'GET' }); } \
             catch (e) { rok.setVar('e', e.message); }",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(
            result.runtime_vars.get("e").expect("e present"),
            "rok.sendRequest: url is required"
        );
        assert!(host.sent().is_empty());
    }

    #[tokio::test]
    async fn host_promise_all_serves_two_requests() {
        let host = FakeHost::ok(200, "{}");
        let ctx = minimal_ctx(
            "const [a, b] = await Promise.all([\
               rok.sendRequest({ url: 'https://x.test/1' }), \
               rok.sendRequest({ url: 'https://x.test/2' })]); \
             rok.setVar('both', a.status + b.status)",
        );
        let result = DenoScriptEngine::new()
            .execute_with_host(ctx, &host)
            .await
            .expect("execute");
        assert_eq!(result.runtime_vars.get("both").expect("both present"), 400);
        assert_eq!(host.sent().len(), 2);
    }
```

- [ ] **Step 4: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra host_`
Expected: it compiles (`execute_with_host` exists through the trait default from Step 2) and the tests FAIL: the script throws `rok.sendRequest is not a function`, so the `runtime_vars` lookups panic.

- [ ] **Step 5: Add the host bridge and the error type**

Create `crates/rocket-infra/src/scripting/host_bridge.rs`:

```rust
//! Carries host calls from the script thread to the task that owns the `ScriptHost`.
//!
//! The script thread has its own Tokio runtime and `JsRuntime` is not `Send`, so
//! ops never touch the host. They send a `HostCall` with a oneshot reply, and
//! `run_script_bounded` in `engine.rs` serves it on the caller's task.

use rocket_scripting::{HostError, HostRequest, HostResponse, ScriptHost};
use tokio::sync::{mpsc, oneshot};

/// One call from a script to the host.
pub enum HostCall {
    /// `rok.sendRequest`.
    Send {
        request: HostRequest,
        reply: oneshot::Sender<Result<HostResponse, HostError>>,
    },
}

/// Where ops send host calls. `None` when the script runs without a host.
pub struct HostChannel(pub Option<mpsc::UnboundedSender<HostCall>>);

/// Serves one call and sends the answer back. A failed send means the script is gone.
pub async fn serve_host_call(host: &dyn ScriptHost, call: HostCall) {
    match call {
        HostCall::Send { request, reply } => {
            let _ = reply.send(host.send_request(request).await);
        }
    }
}
```

In `crates/rocket-infra/src/scripting/mod.rs`, add `pub mod host_bridge;` after `pub mod engine;`.

In `crates/rocket-infra/src/scripting/ops/mod.rs`, add `pub mod host;` after `pub mod fs;`, and add below `ScriptOpError`:

```rust
/// JS-visible error for a failed host call such as `rok.sendRequest`.
/// It surfaces as a plain `Error`, not a `TypeError`.
#[derive(Debug, thiserror::Error, deno_error::JsError)]
#[class(generic)]
#[error("{0}")]
pub struct ScriptHostError(pub String);
```

- [ ] **Step 6: Add the send op**

Create `crates/rocket-infra/src/scripting/ops/host.rs`:

```rust
//! Async ops that reach the host, such as `rok.sendRequest`.

use std::cell::RefCell;
use std::rc::Rc;

use deno_core::{op2, OpState};
use rocket_scripting::{HostError, HostRequest};
use tokio::sync::oneshot;

use crate::scripting::host_bridge::{HostCall, HostChannel};
use crate::scripting::ops::{redact, ScriptHostError};

/// The error for a call that has no host, or whose host went away.
pub(crate) fn unavailable(api: &str) -> ScriptHostError {
    ScriptHostError(format!("{api} is not available here"))
}

/// Queues `call` for the host. Fails when the script runs without one.
pub(crate) fn send_host_call(
    state: &Rc<RefCell<OpState>>,
    call: HostCall,
    api: &str,
) -> Result<(), ScriptHostError> {
    let state = state.borrow();
    let sender = state
        .try_borrow::<HostChannel>()
        .and_then(|channel| channel.0.as_ref())
        .ok_or_else(|| unavailable(api))?;
    sender.send(call).map_err(|_| unavailable(api))
}

/// The error a script sees for a failed host call, with secret values masked.
pub(crate) fn host_error(state: &OpState, api: &str, error: HostError) -> ScriptHostError {
    let message = match error {
        HostError::Unavailable => format!("{api} is not available here"),
        HostError::Failed(message) => message,
    };
    ScriptHostError(redact(state, message))
}

/// rok.sendRequest(options) — sends one HTTP request through the host.
/// Takes a `HostRequest` and returns a `HostResponse`, both as JSON.
#[op2]
#[string]
pub async fn op_rok_send_request(
    state: Rc<RefCell<OpState>>,
    #[string] request_json: String,
) -> Result<String, ScriptHostError> {
    const API: &str = "rok.sendRequest";
    let request: HostRequest = serde_json::from_str(&request_json)
        .map_err(|e| ScriptHostError(format!("{API}: invalid options - {e}")))?;
    let (reply, answer) = oneshot::channel();
    send_host_call(&state, HostCall::Send { request, reply }, API)?;
    let outcome = answer.await.unwrap_or(Err(HostError::Unavailable));
    let state = state.borrow();
    match outcome {
        Ok(response) => serde_json::to_string(&response)
            .map_err(|e| ScriptHostError(format!("{API}: {e}"))),
        Err(error) => Err(host_error(&state, API, error)),
    }
}
```

- [ ] **Step 7: Serve host calls from the engine**

In `crates/rocket-infra/src/scripting/engine.rs`:

1. Replace the imports at the top of the file (above `/// JS scripting engine backed by ...`) with:

```rust
use async_trait::async_trait;
use deno_core::{extension, op2, v8, JsRuntime, OpState, PollEventLoopOptions, RuntimeOptions};
use futures_util::stream::{FuturesUnordered, StreamExt};
use rocket_scripting::{SandboxMode, ScriptContext, ScriptEngine, ScriptHost, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use crate::scripting::host_bridge::{serve_host_call, HostCall, HostChannel};
use crate::scripting::local_modules::build_roots;
use crate::scripting::ops::{console, fs, host, modules, process, redact, req, res, rok};
use crate::scripting::state::{ScriptInputState, ScriptOutputState};
```

2. Replace the `impl ScriptEngine for DenoScriptEngine` block with:

```rust
#[async_trait]
impl ScriptEngine for DenoScriptEngine {
    async fn execute(&self, ctx: ScriptContext) -> DomainResult<ScriptResult> {
        run_script_bounded(ctx, None, SCRIPT_TIMEOUT).await
    }

    async fn execute_with_host(
        &self,
        ctx: ScriptContext,
        host: &dyn ScriptHost,
    ) -> DomainResult<ScriptResult> {
        run_script_bounded(ctx, Some(host), SCRIPT_TIMEOUT).await
    }
}
```

3. Replace the whole `async fn run_script_with_timeout(...)` function (its doc comment included) with the two functions below. The long comment inside the timeout branch is the existing one, moved as is:

```rust
/// Runs a script on a blocking thread, serves its host calls, and aborts it if
/// `timeout` elapses.
///
/// Host calls arrive over a channel and are served here, on the caller's task,
/// because the host may borrow data that cannot move to the script thread.
/// Cancelling the async future alone would not stop the OS thread running V8,
/// so on timeout we ask V8 itself to abort the script through the isolate
/// handle the thread published on start.
async fn run_script_bounded(
    ctx: ScriptContext,
    host: Option<&dyn ScriptHost>,
    timeout: Duration,
) -> DomainResult<ScriptResult> {
    // JsRuntime is !Send, so all V8 work must stay on one thread.
    let (handle_tx, handle_rx) = oneshot::channel();
    let (call_tx, mut call_rx) = mpsc::unbounded_channel::<HostCall>();
    // Without a host the sender is dropped, so host ops find no channel and reject.
    let call_tx = host.map(|_| call_tx);
    let mut join = tokio::task::spawn_blocking(move || run_script(ctx, handle_tx, call_tx));
    let mut serving = FuturesUnordered::new();
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            joined = &mut join => {
                return joined
                    .map_err(|e| DomainError::Internal(format!("script thread panic: {e}")))?;
            }
            Some(call) = call_rx.recv(), if host.is_some() => {
                if let Some(host) = host {
                    serving.push(serve_host_call(host, call));
                }
            }
            Some(()) = serving.next(), if !serving.is_empty() => {}
            () = &mut deadline => break,
        }
    }

    // Terminate whenever the handle arrives, however late. Bounding
    // this wait would abandon a script that had not started yet: it
    // would then run unterminated and pin a blocking thread forever,
    // since dropping a spawn_blocking JoinHandle detaches rather than
    // cancels it.
    //
    // This must be a plain OS thread, not a `tokio::spawn`ed task: a
    // detached async task is tied to this call's Tokio runtime, and
    // on a short-lived runtime (every #[tokio::test] creates and
    // drops one per test) it can be cancelled before it ever gets
    // polled, deadlocking against the `spawn_blocking` thread that
    // Runtime::Drop waits on. A `std::thread` keeps running
    // regardless of what happens to the runtime that spawned it.
    //
    // Terminating makes the blocking thread's execute_script return
    // an "execution terminated" error; it then tears the runtime
    // down on its own and its result is discarded, so we do not wait
    // for it here.
    std::thread::spawn(move || {
        if let Ok(isolate_handle) = handle_rx.blocking_recv() {
            isolate_handle.terminate_execution();
        }
    });
    Err(DomainError::Internal(format!(
        "script execution timed out after {timeout:?}"
    )))
}

/// Runs a script with no host and a plain time limit. The timeout tests use it.
#[cfg(test)]
async fn run_script_with_timeout(
    ctx: ScriptContext,
    timeout: Duration,
) -> DomainResult<ScriptResult> {
    run_script_bounded(ctx, None, timeout).await
}
```

4. In the `extension!(rocket_scripting_ext, ops = [ ... ])` list, add after `rok::op_rok_get_process_env,`:

```rust
        // host ops
        host::op_rok_send_request,
```

5. Give `run_script` and `run_script_async` the channel. Change `run_script` to:

```rust
fn run_script(
    ctx: ScriptContext,
    handle_tx: oneshot::Sender<v8::IsolateHandle>,
    calls: Option<mpsc::UnboundedSender<HostCall>>,
) -> DomainResult<ScriptResult> {
    let tokio_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| DomainError::Internal(format!("script runtime could not start: {e}")))?;
    tokio_rt.block_on(run_script_async(ctx, handle_tx, calls))
}
```

Add the parameter `calls: Option<mpsc::UnboundedSender<HostCall>>,` after `handle_tx` in `async fn run_script_async(`, and in its "Seed OpState" block add after `state.put(ScriptOutputState::default());`:

```rust
        state.put(HostChannel(calls));
```

- [ ] **Step 8: Add the JS wrapper**

In `crates/rocket-infra/src/scripting/bootstrap.js`, immediately above `// ── rok ───...` (the line before `globalThis.rok = {`), add:

```js
  // ── host calls ──────────────────────────────────────────────────────────────
  // Async calls such as rok.sendRequest reach the app through async ops. The ops
  // take and return JSON with snake_case keys (HostRequest and HostResponse).
  function _hostResponse(r) {
    const headers = {};
    for (const [k, v] of r.headers) headers[String(k).toLowerCase()] = v;
    let data = r.body;
    try { data = JSON.parse(r.body); } catch (_e) { /* Not JSON, keep the text. */ }
    return { status: r.status, statusText: r.status_text, headers, data, responseTime: r.response_time_ms };
  }

  function _sendOptions(options) {
    if (!options || typeof options !== 'object') {
      throw new TypeError('rok.sendRequest: options must be an object');
    }
    if (typeof options.url !== 'string' || options.url === '') {
      throw new TypeError('rok.sendRequest: url is required');
    }
    const headers = Object.entries(options.headers || {}).map(([k, v]) => [String(k), String(v)]);
    let body = null;
    let bodyIsJson = false;
    if (options.data !== undefined && options.data !== null) {
      if (typeof options.data === 'string') {
        body = options.data;
      } else {
        body = JSON.stringify(options.data);
        bodyIsJson = true;
      }
    }
    const timeout = typeof options.timeout === 'number' && options.timeout > 0
      ? Math.min(Math.floor(options.timeout), 300000)
      : 30000;
    return JSON.stringify({
      method: String(options.method || 'GET').toUpperCase(),
      url: options.url,
      headers,
      body,
      body_is_json: bodyIsJson,
      timeout_ms: timeout,
    });
  }

```

Then in the `globalThis.rok = {` block, add after the `setNextRequest:` line:

```js
    sendRequest: async (options) => _hostResponse(JSON.parse(await __ops.op_rok_send_request(_sendOptions(options)))),
```

- [ ] **Step 9: Run the engine tests**

Run: `cargo test -j4 -p rocket-infra host_`
Expected: PASS (7 tests).

- [ ] **Step 10: Add the typings**

In `src/components/editor/rok-types.ts`, change the start of `ROK_DEFS` from:

```ts
const ROK_DEFS = `
declare const rok: {
```

to:

```ts
const ROK_DEFS = `
interface RokSendRequestOptions {
  /** HTTP method. Defaults to GET. */
  method?: string;
  url: string;
  headers?: Record<string, string>;
  /** An object or array is sent as JSON. A string is sent as is. */
  data?: unknown;
  /** Time limit in milliseconds. Defaults to 30000. */
  timeout?: number;
}
interface RokResponse {
  status: number;
  statusText: string;
  /** Header names are lowercased. */
  headers: Record<string, string>;
  /** The body, parsed as JSON when possible, otherwise the text. */
  data: any;
  /** Time taken in milliseconds. */
  responseTime: number;
}
declare const rok: {
```

and add inside `declare const rok: {`, after the `setNextRequest(name: string | null): void;` line:

```ts
  /** Send an HTTP request from the script. Variables in the options are not resolved; use rok.interpolate. */
  sendRequest(options: RokSendRequestOptions): Promise<RokResponse>;
```

- [ ] **Step 11: Run all checks**

Run: `cargo test -j4 -p rocket-infra scripting && cargo test -j4 -p rocket-scripting && cargo test -j4 -p rocket-app execution_service && cargo check -j4 && yarn test rok-types && yarn tsc --noEmit && yarn check`
Expected: all PASS. The `rocket-app` mocks use the default `execute_with_host`, which proves the default delegates to `execute`.

- [ ] **Step 12: Commit**

Use the `dev-workflow-skills:1-git-commit` skill with a pathspec commit of exactly: `crates/rocket-scripting/src/host.rs`, `crates/rocket-scripting/src/engine.rs`, `crates/rocket-scripting/src/lib.rs`, `crates/rocket-infra/src/scripting/host_bridge.rs`, `crates/rocket-infra/src/scripting/mod.rs`, `crates/rocket-infra/src/scripting/ops/host.rs`, `crates/rocket-infra/src/scripting/ops/mod.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `src/components/editor/rok-types.ts`.
Suggested subject: `feat(scripting): add ScriptHost and a host-backed rok.sendRequest`.

---

### Task 3: `rok.sleep` and async `test()` bodies

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

**Files:**
- Modify: `crates/rocket-infra/src/scripting/ops/host.rs`
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (registration, tests)
- Modify: `crates/rocket-infra/src/scripting/bootstrap.js` (`rok` block, `globalThis.test`)
- Modify: `src/components/editor/rok-types.ts` (`ROK_DEFS`, `TEST_DEFS`)

**Interfaces:**
- Consumes: Task 1 (event loop), Task 2 (`ops/host.rs`).
- Produces: `pub(crate) const MAX_SLEEP_MS: f64 = 60_000.0`, `op_rok_sleep(ms: f64)` (plan 02 Task 1 changes its signature to read the cap from `OpState`), `rok.sleep(ms): Promise<void>`, and `test(name, fn)` that records an async body when its promise settles.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `engine.rs`, after the `host_` tests:

```rust
    // ── sleep and async tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn sleep_waits_and_resolves() {
        let ctx = minimal_ctx(
            "const t = Date.now(); await rok.sleep(50); rok.setVar('waited', Date.now() - t >= 45)",
        );
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.runtime_vars.get("waited").expect("waited present"), true);
    }

    #[tokio::test]
    async fn sleep_clamps_a_negative_value_to_zero() {
        let ctx = minimal_ctx("await rok.sleep(-100); rok.setVar('ok', true)");
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("ok").expect("ok present"), true);
    }

    #[tokio::test]
    async fn sleep_rejects_non_numbers() {
        let ctx = minimal_ctx(
            "let n = 0; \
             for (const v of ['5', NaN, undefined, null]) { \
               try { await rok.sleep(v); } catch (e) { if (e instanceof TypeError) n++; } \
             } \
             rok.setVar('n', n)",
        );
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert_eq!(result.runtime_vars.get("n").expect("n present"), 4);
    }

    #[tokio::test]
    async fn async_test_bodies_are_recorded_when_they_settle() {
        let ctx = minimal_ctx(
            "test('slow pass', async () => { await rok.sleep(10); }); \
             test('slow fail', async () => { await rok.sleep(10); throw new Error('late'); });",
        );
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        assert!(result.error.is_none(), "{:?}", result.error);
        let pass = result
            .test_results
            .iter()
            .find(|t| t.name == "slow pass")
            .expect("slow pass recorded");
        assert_eq!(pass.status, TestStatus::Passed);
        let fail = result
            .test_results
            .iter()
            .find(|t| t.name == "slow fail")
            .expect("slow fail recorded");
        assert_eq!(fail.status, TestStatus::Failed);
        assert!(fail.error.as_deref().unwrap_or_default().contains("late"));
    }

    #[tokio::test]
    async fn sync_test_bodies_still_record_in_order() {
        let ctx = minimal_ctx(
            "test('a', () => {}); test('b', () => { throw new Error('x'); }); test('c', () => {});",
        );
        let result = DenoScriptEngine::new().execute(ctx).await.expect("execute");
        let names: Vec<_> = result.test_results.iter().map(|t| t.name.clone()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
        assert_eq!(result.test_results[1].status, TestStatus::Failed);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra -- sleep_ async_test_bodies sync_test_bodies`
Expected: FAIL. `rok.sleep is not a function`, and the async test bodies are recorded as passed straight away.

- [ ] **Step 3: Add the sleep op**

In `crates/rocket-infra/src/scripting/ops/host.rs`, add `use std::time::Duration;` to the imports and add at the end of the file:

```rust
/// Longest single `rok.sleep`, in milliseconds.
pub(crate) const MAX_SLEEP_MS: f64 = 60_000.0;

/// rok.sleep(ms) — waits without blocking the event loop. The value is
/// clamped to 0..=60000. The JS wrapper rejects non-numbers first.
#[op2]
pub async fn op_rok_sleep(ms: f64) {
    let ms = if ms.is_nan() { 0.0 } else { ms.clamp(0.0, MAX_SLEEP_MS) };
    tokio::time::sleep(Duration::from_millis(ms as u64)).await;
}
```

In `engine.rs`, add to the extension list after `host::op_rok_send_request,`:

```rust
        host::op_rok_sleep,
```

- [ ] **Step 4: Add the JS side**

In `bootstrap.js`, in the `globalThis.rok = {` block, add after the `sendRequest:` line:

```js
    sleep: (ms) => ((typeof ms !== 'number' || Number.isNaN(ms))
      ? Promise.reject(new TypeError('rok.sleep: ms must be a number'))
      : __ops.op_rok_sleep(ms)),
```

Replace the existing `globalThis.test = function(name, fn) { ... };` with:

```js
  // An async body is recorded when its promise settles. The event loop keeps
  // running until then, even when the script does not await the test.
  globalThis.test = function(name, fn) {
    __ops.op_test_run(name);
    let out;
    try {
      out = fn();
    } catch (e) {
      __ops.op_test_fail(name, String(e));
      return;
    }
    if (out && typeof out.then === 'function') {
      return Promise.resolve(out).then(
        () => { __ops.op_test_pass(name); },
        (e) => { __ops.op_test_fail(name, String(e)); },
      );
    }
    __ops.op_test_pass(name);
  };
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -j4 -p rocket-infra -- sleep_ async_test_bodies sync_test_bodies`
Expected: PASS (5 tests).

- [ ] **Step 6: Add the typings**

In `ROK_DEFS` in `src/components/editor/rok-types.ts`, add after the `sendRequest(...)` line:

```ts
  /** Wait ms milliseconds, clamped to 0..60000. Use with await. */
  sleep(ms: number): Promise<void>;
```

In `TEST_DEFS`, replace:

```ts
declare function test(name: string, fn: () => void): void;
```

with:

```ts
declare function test(name: string, fn: () => void | Promise<void>): void;
```

- [ ] **Step 7: Run all checks**

Run: `cargo test -j4 -p rocket-infra scripting && cargo check -j4 && yarn test rok-types && yarn tsc --noEmit && yarn check`
Expected: all PASS.

- [ ] **Step 8: Commit**

Skill `dev-workflow-skills:1-git-commit`, pathspec commit of: `crates/rocket-infra/src/scripting/ops/host.rs`, `crates/rocket-infra/src/scripting/engine.rs`, `crates/rocket-infra/src/scripting/bootstrap.js`, `src/components/editor/rok-types.ts`.
Suggested subject: `feat(scripting): add rok.sleep and await async test bodies`.

---

## Next plan to execute

When Task 3 is complete, its checks pass and the ledger (`.superpowers/sdd/rok-parity-b-01-async-engine-model/progress.md`) shows "Task 3: complete", **the executing Claude must go straight on to plan 02**: `docs/superpowers/plans/rok-parity-b/02-split-budget.md`. No consent is needed between plans. Run one plan at a time, and swap the visible task list to plan 02's tasks when it starts.

Plan 02 depends on this plan: it changes `run_script_bounded`, `run_script`, `run_script_async`, `run_user_code` and `op_rok_sleep` as they stand after Task 3. Do not start it on a tree where this plan's checks fail.
